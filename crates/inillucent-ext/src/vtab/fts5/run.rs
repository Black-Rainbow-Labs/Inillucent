//! Running the `MATCH` of one plan: parsing its texts, finding the rows, and ordering them.
//!
//! Invariant: **the arguments of a plan are handled in the order SQLite handles them, and the
//! first thing that is wrong with one is the error the caller gets.** Each `MATCH` text is parsed
//! on its own, with the column it was written against applied to its tree, and the trees are then
//! joined with `AND`. A text that begins with `*` is a request for an internal value and is
//! answered before the texts after it are read.

use inillucent_base::DbResult;

use super::super::{Context, FilterPlan};
use super::constraints::{match_rowid_limit, read_constraints, Constraints};
use super::expr::{syntax, Query};
use super::index::buffered_totals;
use super::matcher::{self, Evaluated};
use super::query::{Fts5Cursor, MatchedRow};
use super::{bm25, expr, Dialect, PLAN_DESCENDING, PLAN_RANKED};

impl Fts5Cursor {
    /// Runs the `MATCH` constraints of a plan and collects the rows they matched.
    ///
    /// @param context - the running statement
    /// @param plan - the plan, which has at least one `MATCH` argument
    pub(super) fn run_match(
        &mut self,
        context: &mut Context<'_>,
        plan: &FilterPlan,
    ) -> DbResult<()> {
        let mut constraints = read_constraints(plan);
        // SQLite is given the constraints of a statement last to first, so it reads the last
        // `MATCH` first. That decides which error is reported when two texts are wrong, which
        // text is a special query, and the order the phrases of the joined query are numbered in.
        if self.dialect == Dialect::Five {
            constraints.queries.reverse();
        }
        if self.answer_special_query(&constraints)? {
            return Ok(());
        }
        let Some(query) = self.parse_constraints(&constraints)? else {
            return Ok(());
        };
        self.rank_function = match &constraints.rank {
            Some(text) => Some(bm25::parse_rank(text.as_deref()).map_err(syntax)?),
            None => None,
        };
        let ranked = plan.index_number & PLAN_RANKED != 0;
        let descending = plan.index_number & PLAN_DESCENDING != 0;
        // The totals are the buffered ones. A score is computed from the document count and the
        // token totals, and those are held in the buffer until the commit, so a query inside the
        // transaction that read `%_data` would score against the state the transaction started
        // from.
        let totals = buffered_totals(context, &self.shadows, &self.pending, self.columns);
        self.rank_weights.clear();
        if ranked {
            self.refuse_unknown_rank_function()?;
            self.rank_weights = self.load_rank_weights(context)?;
        }
        let evaluated = self.evaluate_for(&query, context, ranked)?;
        self.hits_built = !evaluated.hits.is_empty();
        let rows = self.collect_rows(&evaluated, &query, context, &totals, ranked)?;
        self.phrases = query.phrases.clone();
        self.stats = bm25::statistics(&evaluated.hits, &totals);
        self.totals = totals;
        self.evaluated = evaluated;
        self.query = Some(query);
        self.rows = limit_to_rowid(rows, constraints.rowid.as_ref());
        self.order_rows(ranked, descending);
        Ok(())
    }

    /// Answers `MATCH '*id'` and `MATCH '*reads'`, which are not searches.
    ///
    /// SQLite answers one row with rowid 0 whose table column holds the value. `*reads` is how
    /// many index pages the connection has read, which this index does not count in pages, so it
    /// is 0. Any other word is `unknown special query`. Returns whether the plan was one.
    ///
    /// @param constraints - what the plan's arguments ask for
    fn answer_special_query(&mut self, constraints: &Constraints) -> DbResult<bool> {
        let special = |text: &Vec<u8>| text.first() == Some(&b'*');
        if self.dialect != Dialect::Five
            || !constraints.queries.iter().any(|(text, _)| special(text))
        {
            return Ok(false);
        }
        for (text, column) in &constraints.queries {
            if text.first() != Some(&b'*') {
                self.parse_text(text, *column)?;
                continue;
            }
            let rest = text.get(1..).unwrap_or_default();
            let start = rest.iter().take_while(|byte| **byte == b' ').count();
            let word: Vec<u8> = rest
                .iter()
                .skip(start)
                .take_while(|byte| **byte != b' ')
                .copied()
                .collect();
            let value = if word.eq_ignore_ascii_case(b"reads") {
                0
            } else if word.eq_ignore_ascii_case(b"id") {
                self.cursor_id
            } else {
                let said = format!("unknown special query: {}", String::from_utf8_lossy(&word));
                return Err(syntax(said));
            };
            self.special = Some(value);
            self.rows = vec![MatchedRow {
                rowid: 0,
                score: None,
            }];
            return Ok(true);
        }
        Ok(false)
    }

    /// Parses one `MATCH` text for this table's dialect.
    ///
    /// Answers `None` when an FTS3 or FTS4 query has no terms and so matches nothing.
    ///
    /// @param text - the text
    /// @param column - the column the match was written against, when it was
    fn parse_text(&self, text: &[u8], column: Option<usize>) -> DbResult<Option<Query>> {
        if self.dialect == Dialect::Three {
            let Some(rewritten) = expr::fts3_pattern(text)? else {
                return Ok(None);
            };
            let text = expr::fts5_text_of_fts3(&rewritten);
            return Query::parse_in(&text, &self.tokenizer, &self.names, column).map(Some);
        }
        Query::parse_in(text, &self.tokenizer, &self.names, column).map(Some)
    }

    /// Parses every `MATCH` text of a plan and joins them with `AND`, as SQLite does.
    ///
    /// @param constraints - what the plan's arguments ask for
    fn parse_constraints(&self, constraints: &Constraints) -> DbResult<Option<Query>> {
        let mut joined: Option<Query> = None;
        for (text, column) in &constraints.queries {
            let Some(next) = self.parse_text(text, *column)? else {
                return Ok(None);
            };
            joined = Some(match joined {
                None => next,
                Some(held) => held.and(next),
            });
        }
        Ok(joined)
    }

    /// Finds the rows of a query, reading positions only when the answer needs them.
    ///
    /// **A ranked plan needs the positions, and most plans do not.** The cheap walk answers
    /// `None` for the queries whose answer depends on them, and those fall through to the full
    /// evaluation.
    ///
    /// @param query - the parsed query
    /// @param context - the running statement
    /// @param ranked - whether the rows are to come back ranked
    fn evaluate_for(
        &self,
        query: &Query,
        context: &mut Context<'_>,
        ranked: bool,
    ) -> DbResult<Evaluated> {
        let cheap = if ranked {
            None
        } else {
            matcher::evaluate_rows(query, context, &self.shadows, &self.pending, self.columns)?
        };
        match cheap {
            Some(rows) => Ok(Evaluated {
                rows,
                hits: Vec::new(),
                cooccur: Vec::new(),
            }),
            None => matcher::evaluate(query, context, &self.shadows, &self.pending, self.columns),
        }
    }

    /// Turns the rows a query matched into the cursor's rows, scoring them when they are ranked.
    ///
    /// @param evaluated - the rows and the hits of the phrases
    /// @param query - the parsed query
    /// @param context - the running statement
    /// @param totals - the collection totals
    /// @param ranked - whether the rows are to come back ranked
    fn collect_rows(
        &self,
        evaluated: &Evaluated,
        query: &Query,
        context: &mut Context<'_>,
        totals: &super::Totals,
        ranked: bool,
    ) -> DbResult<Vec<MatchedRow>> {
        if !ranked {
            return Ok(evaluated
                .rows
                .iter()
                .map(|rowid| MatchedRow {
                    rowid: *rowid,
                    score: None,
                })
                .collect());
        }
        // The sort key has to exist before the sort, so this is the one plan that scores every
        // row up front.
        let scores = bm25::score(
            evaluated,
            query,
            context,
            &self.shadows,
            totals,
            self.columns,
            &self.rank_weights,
        )?;
        Ok(scores
            .into_iter()
            .map(|(rowid, score)| MatchedRow {
                rowid,
                score: Some(score),
            })
            .collect())
    }

    /// Puts the rows in the order the plan promised: by score when ranked, by rowid otherwise.
    ///
    /// @param ranked - whether the rows are to come back ranked
    /// @param descending - whether the ranked rows come worst first
    fn order_rows(&mut self, ranked: bool, descending: bool) {
        if !ranked {
            self.rows.sort_by_key(|row| row.rowid);
            return;
        }
        self.rows.sort_by(|left, right| {
            // Both sides are scored here: this is only reached when every row was.
            let order = left
                .score
                .unwrap_or(0.0)
                .partial_cmp(&right.score.unwrap_or(0.0))
                .unwrap_or(core::cmp::Ordering::Equal);
            let order = if descending { order.reverse() } else { order };
            order.then(left.rowid.cmp(&right.rowid))
        });
    }

    /// Refuses a rank function that is not one this table has, as `fts5FindRankFunction` does.
    pub(super) fn refuse_unknown_rank_function(&self) -> DbResult<()> {
        match &self.rank_function {
            Some(function) if !function.name.eq_ignore_ascii_case("bm25") => {
                Err(syntax(format!("no such function: {}", function.name)))
            }
            _ => Ok(()),
        }
    }
}

/// Keeps only the row a statement's `rowid = ?` named, when it named one.
///
/// @param rows - the rows the match found
/// @param rowid - the value the rowid was compared with
fn limit_to_rowid(
    rows: Vec<MatchedRow>,
    rowid: Option<&inillucent_value::Value<'static>>,
) -> Vec<MatchedRow> {
    let Some(value) = rowid else {
        return rows;
    };
    match match_rowid_limit(value) {
        Some(wanted) => rows.into_iter().filter(|row| row.rowid == wanted).collect(),
        None => rows,
    }
}

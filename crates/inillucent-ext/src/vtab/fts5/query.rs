//! The cursor an FTS5 scan reads.
//!
//! Invariant: **a matched row is decided before the cursor moves onto it.**
//! `filter` resolves the query to a rowid list and the cursor walks that,
//! so a row the cursor is on has already matched and `column` never has to
//! decide whether to skip it.

use inillucent_base::DbResult;
use inillucent_value::Value;

use super::super::{Context, FilterPlan, VirtualCursor};
use crate::shadow::ShadowTables;

use super::expr::Phrase;
use super::expr::Query;
use super::index::*;
use super::matcher::{self, Evaluated, Position};
use super::tokenize::Tokenizer;
use super::*;

/// One row a query matched, with what it needs to be scored.
#[derive(Clone, Debug)]
pub(crate) struct MatchedRow {
    /// Which row.
    pub(super) rowid: i64,
    /// The score `rank` reports, which is negative so that smaller is better.
    ///
    /// `None` until something asks for it. Scoring one row costs a read of its
    /// `%_docsize`, and a query that names neither `rank` nor `ORDER BY rank`
    /// never looks at the answer - which is most of them, and `count(*)` in
    /// particular. The one query that needs every score up front is the ranked
    /// one, where the score *is* the sort key.
    pub(super) score: Option<f64>,
}
/// A cursor over the rows one query matched.
pub(crate) struct Fts5Cursor {
    /// Which surface this table presents.
    pub(crate) dialect: Dialect,
    /// The option this build cannot honour, when the table declares one.
    ///
    /// Every query of the table reports it; see [`super::Options::unsupported`]
    /// for why the refusal is here rather than at open.
    pub(crate) unsupported: Option<String>,
    pub(crate) columns: usize,
    /// The table's own name, so a refusal about its index can say which one.
    pub(crate) table: Vec<u8>,
    /// The shadow suffix the rows are reached under.
    pub(crate) content: Vec<u8>,
    /// Whether the table stores no document text at all.
    ///
    /// A `content=''` table has no `%_content` shadow, so every declared column
    /// reads back NULL - which is what SQLite answers and what task-1979, R5
    /// found this returning the stored document for instead.
    pub(crate) contentless: bool,
    /// Where each declared column sits in a stored row.
    pub(crate) offsets: Vec<usize>,
    /// The table the rows belong to, when they are not this table's.
    pub(crate) external: Option<Vec<u8>>,
    /// The column names, for a `column:term` filter to resolve against.
    pub(crate) names: Vec<Vec<u8>>,
    pub(crate) match_column: i32,
    pub(crate) rank_column: i32,
    /// Which columns were declared UNINDEXED.
    pub(crate) unindexed: Vec<bool>,
    /// The column weights `rank` scores with, from the `rank` setting; see
    /// `bm25::configured_weights`.
    pub(crate) rank_weights: Vec<f64>,
    /// The rank function a `rank MATCH '...'` constraint gave this query, such
    /// as `bm25(10.0, 1.0)`, which replaces the `rank` setting for it.
    pub(crate) rank_function: Option<bm25::RankFunction>,
    /// What a `MATCH '*id'` query answers, when that is what the query was.
    pub(crate) special: Option<i64>,
    pub(crate) tokenizer: Tokenizer,
    pub(crate) shadows: ShadowTables,
    pub(crate) rows: Vec<MatchedRow>,
    pub(crate) at: usize,
    /// The rows the query matched and what each phrase matched, kept so
    /// `bm25(t, w1, w2)` can score a row with the weights that call asked for.
    /// `rank` is the same score with every weight one, so the two cannot
    /// disagree.
    pub(crate) evaluated: Evaluated,
    /// The numbers the query's score is relative to, worked out once.
    pub(crate) stats: bm25::Statistics,
    /// The phrases, in the order `matched` holds them.
    pub(crate) phrases: Vec<Phrase>,
    /// The collection totals the score is relative to.
    pub(crate) totals: Totals,
    /// The parsed query, kept so the hits can be built if a score is asked for.
    pub(crate) query: Option<Query>,
    /// Whether `matched` holds this query's hits.
    ///
    /// A query that reads no score never builds them; one that reads a score
    /// after not building them builds them once, here, rather than per row.
    pub(crate) hits_built: bool,
    /// The doclists the transaction has staged, shared with the table.
    ///
    /// A query inside a transaction that has written has to see what it wrote,
    /// and the buffer is where those doclists are until the commit.
    pub(crate) pending: Buffer,
    /// The `%_content` row the cursor is on, kept for the columns after the
    /// first.
    ///
    /// **One read per row, not one per column.** `column` is asked for each
    /// column in turn and read the whole row back for every one of them, so a
    /// two-column table read `%_content` twice per matched row and a
    /// ten-column table ten times. The row is the same row each time.
    pub(crate) held: Option<(i64, Vec<Value<'static>>)>,
    /// The number of this cursor among those opened on the table, from 1.
    ///
    /// Reading the hidden column that carries the table's name answers this number, which is
    /// what SQLite answers: `SELECT f FROM f` is not NULL, it identifies the cursor.
    pub(crate) cursor_id: i64,
}
/// Reads one `bm25()` weight argument the way `sqlite3_value_double` does.
///
/// An integer weight such as `bm25(t, 10, 1)` used to be read as 1 because only
/// a real was accepted. Any numeric value is a weight: an integer, a real, text
/// that starts with a number, and zero or a negative number as well. NULL and a
/// text or blob that does not start with a number read as 0, which is what
/// SQLite does.
///
/// @param argument - one argument of the `bm25` call
fn weight_of(argument: &Value<'static>) -> f64 {
    match argument {
        Value::Integer(number) => *number as f64,
        Value::Real(number) => *number,
        Value::Text(text) => leading_number(&String::from_utf8_lossy(&text.utf8_bytes())),
        Value::Blob(blob) => leading_number(&String::from_utf8_lossy(blob.raw())),
        Value::Null => 0.0,
    }
}

/// Reads the number a piece of text starts with, as `atof` does.
///
/// Leading spaces are skipped, `3abc` is 3, and text that does not start with a
/// number is 0. Words such as `inf` and `nan` are not numbers here.
///
/// @param text - the text to read
pub(super) fn leading_number(text: &str) -> f64 {
    let trimmed = text.trim_start();
    let allowed = |character: char| matches!(character, '0'..='9' | '.' | '+' | '-' | 'e' | 'E');
    let candidate_end = trimmed
        .char_indices()
        .find(|(_, character)| !allowed(*character))
        .map(|(at, _)| at)
        .unwrap_or(trimmed.len());
    let mut end = candidate_end;
    while end > 0 {
        if let Some(prefix) = trimmed.get(..end) {
            if let Ok(number) = prefix.parse::<f64>() {
                return number;
            }
        }
        end -= 1;
    }
    0.0
}

/// Reads an argument as an integer the way `sqlite3_value_int64` does.
///
/// A real is cut to its whole part and text is read up to its first character that is not a digit,
/// so `'2'` and `2.9` are both 2, `'abc'` and NULL are 0, and nothing is refused.
///
/// @param argument - one argument of the call, when it was given
fn int_argument(argument: Option<&Value<'static>>) -> i64 {
    match argument {
        Some(Value::Integer(number)) => *number,
        Some(Value::Real(number)) => *number as i64,
        Some(Value::Text(text)) => leading_integer(&text.utf8_bytes()),
        Some(Value::Blob(blob)) => leading_integer(blob.raw()),
        _ => 0,
    }
}

/// Reads the whole number a piece of text starts with, as `sqlite3Atoi64` does.
///
/// @param bytes - the text
fn leading_integer(bytes: &[u8]) -> i64 {
    let mut at = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_whitespace())
        .count();
    let negative = bytes.get(at) == Some(&b'-');
    if matches!(bytes.get(at), Some(b'-' | b'+')) {
        at += 1;
    }
    let mut value = 0i64;
    for byte in bytes
        .iter()
        .skip(at)
        .take_while(|byte| byte.is_ascii_digit())
    {
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(byte - b'0'));
    }
    if negative {
        -value
    } else {
        value
    }
}

impl Fts5Cursor {
    /// Returns the column weights `rank` scores this query with.
    ///
    /// The function a `rank MATCH '...'` constraint named wins over the
    /// table's `rank` setting, which is what SQLite does for that query.
    ///
    /// @param context - the running statement
    pub(super) fn load_rank_weights(&self, context: &mut Context<'_>) -> DbResult<Vec<f64>> {
        match &self.rank_function {
            Some(function) => Ok(function.column_weights(self.columns)),
            None => bm25::configured_weights(context, &self.shadows, self.columns),
        }
    }

    /// Returns where each phrase counts in the row the cursor is on.
    ///
    /// A phrase counts only where its node is part of the row's match, and a `NEAR`
    /// only where its window is, which is how SQLite reports them to `bm25`,
    /// `highlight` and `snippet`. See [`matcher::Evaluated::live_positions`].
    fn live_positions(&self) -> Vec<Vec<Position>> {
        let (Some(row), Some(query)) = (self.rows.get(self.at), self.query.as_ref()) else {
            return Vec::new();
        };
        self.evaluated.live_positions(&query.root, row.rowid)
    }

    /// Returns where the query's phrases occur in one column of the current row.
    ///
    /// FTS5 reads them from the positions the index holds for the row, as SQLite
    /// does, so a phrase in a branch that did not match the row, or outside the
    /// window of a `NEAR`, is not reported. FTS3 and FTS4 find them in the
    /// column's tokens.
    ///
    /// @param live - where each phrase counts in the row
    /// @param spans - the column's tokens with their byte ranges
    /// @param column - which column the tokens are from
    fn instances_of(
        &self,
        live: &[Vec<Position>],
        spans: &[snippet::Span],
        column: usize,
    ) -> Vec<snippet::Instance> {
        if self.dialect == Dialect::Five {
            return snippet::instances_from_positions(&self.phrases, live, column);
        }
        let found = snippet::instances_in(&self.phrases, spans, column);
        let groups = self
            .query
            .as_ref()
            .map(Query::near_groups)
            .unwrap_or_default();
        if groups.is_empty() {
            return found;
        }
        snippet::keep_near_matches(found, &groups, &self.phrases)
    }

    /// Builds the phrase hits if the cheap walk did not.
    ///
    /// The one caller that needs them is a score, and a score is asked for per
    /// row - so this runs once and every later row reads what it left.
    ///
    /// @param context - the host
    fn ensure_hits(&mut self, context: &mut Context<'_>) -> DbResult<()> {
        if self.hits_built {
            return Ok(());
        }
        self.hits_built = true;
        let Some(query) = self.query.clone() else {
            return Ok(());
        };
        self.evaluated =
            matcher::evaluate(&query, context, &self.shadows, &self.pending, self.columns)?;
        self.stats = bm25::statistics(&self.evaluated.hits, &self.totals);
        Ok(())
    }
}
impl Fts5Cursor {
    /// Answers `highlight(t, column, open, close)` on the current row.
    ///
    /// The column's text is **re-tokenised here** rather than read out of the
    /// index, and that is the design rather than a shortcut. The index stores a
    /// token's *position* - which word it is - and highlighting needs its
    /// *extent* in bytes, which the index never held: a token is folded before
    /// it is stored, and folding changes its length. Re-tokenising is the only
    /// way to get from a position back to a range of the original text, and it
    /// is what SQLite does too.
    ///
    /// **A phrase, not a token.** `MATCH '"quick brown"'` marks the two words
    /// as one range and `MATCH 'quick AND brown'` marks them as two, because
    /// the first is one phrase of two terms and the second is two phrases of
    /// one. Phrases that overlap are one range. A column that does not exist
    /// answers an empty string, as SQLite does.
    ///
    /// @param context - the statement's context
    /// @param arguments - the column index, the opening mark, the closing mark
    fn highlight(
        &mut self,
        context: &mut Context<'_>,
        arguments: &[Value<'static>],
    ) -> DbResult<Value<'static>> {
        let requested = int_argument(arguments.first());
        let open = text_argument(arguments.get(1));
        let close = text_argument(arguments.get(2));
        let column = match usize::try_from(requested) {
            Ok(column) if column < self.columns => column,
            _ => return Value::owned_text(b""),
        };
        let Some(text) = self.column_text(context, column)? else {
            return Ok(Value::Null);
        };
        self.ensure_hits(context)?;
        let live = self.live_positions();
        let spans = self.tokenizer.spans(&text);
        let instances = self.instances_of(&live, &spans, column);
        let ranges = snippet::coalesced(&instances);
        let out = snippet::highlight_text(&text, &spans, &ranges, (&open, &close));
        Ok(Value::owned_text(&out).unwrap_or(Value::Null))
    }

    /// Answers `snippet(t, column, open, close, ellipsis, tokens)`.
    ///
    /// The window is chosen the way SQLite chooses it, by the `snippet` module:
    /// every occurrence of a phrase starts a window centred on it, so does the
    /// start of the sentence the occurrence is in, and the first window with the
    /// best score wins. A column of `-1` considers every column and shows the
    /// one holding the best window; the first column is shown when nothing
    /// matches in any of them. The ellipsis is written only where text was
    /// actually cut, so a snippet of a short column reads as the whole column.
    ///
    /// @param context - the statement's context
    /// @param arguments - column, open, close, ellipsis, token count
    fn snippet(
        &mut self,
        context: &mut Context<'_>,
        arguments: &[Value<'static>],
    ) -> DbResult<Value<'static>> {
        let requested = int_argument(arguments.first());
        let open = text_argument(arguments.get(1));
        let close = text_argument(arguments.get(2));
        let ellipsis = text_argument(arguments.get(3));
        let wanted = int_argument(arguments.get(4)).clamp(0, 64);
        self.ensure_hits(context)?;
        let live = self.live_positions();
        let best = self.best_snippet_window(context, &live, requested, wanted)?;
        let Some(text) = self.column_text(context, best.column)? else {
            return Ok(Value::Null);
        };
        let spans = self.tokenizer.spans(&text);
        // `nColSize` is what the best window was found in, and the column's own
        // token count when no window scored.
        // An UNINDEXED column has no size in `%_docsize`, which SQLite reads as no tokens, so the
        // whole text is shown.
        let tokens = match best.size {
            _ if self.unindexed.get(best.column) == Some(&true) => 0,
            0 => spans.len() as i64,
            size => size,
        };
        let instances = self.instances_of(&live, &spans, best.column);
        let ranges = snippet::coalesced(&instances);
        let words = (open.as_slice(), close.as_slice(), ellipsis.as_slice());
        let out =
            snippet::snippet_text(&text, &spans, &ranges, (best.start, wanted, tokens), words);
        Ok(Value::owned_text(&out).unwrap_or(Value::Null))
    }

    /// Answers FTS3 and FTS4 `snippet(t, open, close, ellipsis, column, tokens)`.
    ///
    /// The fragments are chosen and written the way SQLite's `fts3_snippet.c`
    /// does it, by the `fts3_snippet` module: up to four fragments, each holding
    /// the tokens argument divided by the fragment count, joined by the
    /// ellipsis. A negative column considers every column for every fragment.
    ///
    /// @param context - the statement's context
    /// @param arguments - open, close, ellipsis, column, token count, all optional
    fn snippet_three(
        &mut self,
        context: &mut Context<'_>,
        arguments: &[Value<'static>],
    ) -> DbResult<Value<'static>> {
        let open = arguments
            .first()
            .map_or(b"<b>".to_vec(), |v| text_argument(Some(v)));
        let close = arguments
            .get(1)
            .map_or(b"</b>".to_vec(), |v| text_argument(Some(v)));
        let ellipsis = arguments
            .get(2)
            .map_or(b"<b>...</b>".to_vec(), |v| text_argument(Some(v)));
        let named = arguments.get(3).and_then(Value::as_integer).unwrap_or(-1);
        let tokens = arguments.get(4).and_then(Value::as_integer).unwrap_or(15);
        if tokens == 0 || self.phrases.is_empty() {
            return Value::owned_text(b"");
        }
        let tokens = tokens.clamp(-64, 64);
        let texts = self.column_sources(context)?;
        self.ensure_hits(context)?;
        let live = self.live_positions();
        let columns = self.column_hits(&live, &texts, named);
        let (fragments, size) = fts3_snippet::choose_fragments(&columns, tokens);
        let marks = fts3_snippet::Marks {
            open: &open,
            close: &close,
            ellipsis: &ellipsis,
        };
        let mut out = fts3_snippet::Buffer::default();
        for (index, fragment) in fragments.iter().enumerate() {
            let Some(Some((text, spans))) = texts.get(fragment.column) else {
                continue;
            };
            let source = fts3_snippet::Source { text, spans };
            let place = fts3_snippet::Placement {
                index,
                last: index + 1 == fragments.len(),
                size,
            };
            fts3_snippet::append_fragment(&mut out, &source, fragment, &place, &marks);
        }
        match out.finish() {
            Some(bytes) => Ok(Value::owned_text(&bytes).unwrap_or(Value::Null)),
            None => Ok(Value::Null),
        }
    }

    /// Reads every column's text and tokens for the current row.
    ///
    /// @param context - the statement's context
    fn column_sources(
        &mut self,
        context: &mut Context<'_>,
    ) -> DbResult<Vec<Option<(Vec<u8>, Vec<snippet::Span>)>>> {
        let mut texts = Vec::with_capacity(self.columns);
        for column in 0..self.columns {
            let text = self.column_text(context, column)?;
            texts.push(text.map(|bytes| {
                let spans = self.tokenizer.spans(&bytes);
                (bytes, spans)
            }));
        }
        Ok(texts)
    }

    /// Finds the phrase occurrences of the columns a snippet considers.
    ///
    /// @param live - where each phrase counts in the row
    /// @param texts - each column's text and tokens
    /// @param named - the column argument, negative for every column
    fn column_hits(
        &self,
        live: &[Vec<Position>],
        texts: &[Option<(Vec<u8>, Vec<snippet::Span>)>],
        named: i64,
    ) -> Vec<fts3_snippet::ColumnHits> {
        let mut columns = Vec::new();
        for column in 0..self.columns {
            if named >= 0 && column as i64 != named {
                continue;
            }
            let spans = texts
                .get(column)
                .and_then(Option::as_ref)
                .map(|(_, spans)| spans.as_slice());
            let found = self.instances_of(live, spans.unwrap_or_default(), column);
            columns.push(fts3_snippet::ColumnHits {
                column,
                hits: fts3_snippet::hits_of(&found, self.phrases.len()),
            });
        }
        columns
    }

    /// Returns the column and the first token of the window `snippet` shows.
    ///
    /// @param context - the statement's context
    /// @param live - where each phrase counts in the row
    /// @param requested - the column argument, negative for every column
    /// @param wanted - how many tokens the snippet shows
    fn best_snippet_window(
        &mut self,
        context: &mut Context<'_>,
        live: &[Vec<Position>],
        requested: i64,
        wanted: i64,
    ) -> DbResult<snippet::Best> {
        let first = usize::try_from(requested).unwrap_or(0);
        let candidates: Vec<usize> = if requested < 0 {
            (0..self.columns).collect()
        } else {
            vec![first]
        };
        let mut best = snippet::Best {
            column: first,
            start: 0,
            score: 0,
            size: 0,
        };
        for column in candidates {
            let Some(text) = self.column_text(context, column)? else {
                continue;
            };
            let spans = self.tokenizer.spans(&text);
            let instances = self.instances_of(live, &spans, column);
            let sentences = snippet::sentence_starts(&text, &spans);
            let windows = snippet::ColumnWindows {
                column,
                instances: &instances,
                sentences: &sentences,
                tokens: spans.len() as i64,
            };
            snippet::consider_column(&mut best, &windows, self.phrases.len(), wanted);
        }
        Ok(best)
    }

    /// Fills in where each declared column sits, the first time a row is read.
    ///
    /// `open` is handed no catalog - a cursor is made before a statement runs -
    /// so an external content table's column positions cannot be resolved until
    /// here, where a context exists.
    ///
    /// @param context - the running statement
    fn resolve_offsets(&mut self, context: &Context<'_>) {
        if self.external.is_some() && self.offsets.contains(&usize::MAX) {
            self.offsets =
                content_offsets_named(self.external.as_deref(), &self.names, context.catalog);
        }
    }

    /// Returns one column's stored text for the current row.
    ///
    /// @param context - the statement's context
    /// @param column - which declared column
    fn column_text(
        &mut self,
        context: &mut Context<'_>,
        column: usize,
    ) -> DbResult<Option<Vec<u8>>> {
        let Some(row) = self.rows.get(self.at).cloned() else {
            return Ok(None);
        };
        if self.contentless {
            return Ok(None);
        }
        if self.held.as_ref().map(|(rowid, _)| *rowid) != Some(row.rowid) {
            self.resolve_offsets(context);
            self.held = self
                .shadows
                .read_row(context, &self.content.clone(), row.rowid)?
                .map(|values| (row.rowid, values));
        }
        let Some((_, content)) = self.held.as_ref() else {
            return Ok(None);
        };
        // A number in a column is read as the text it is written as, which is what SQLite's
        // `sqlite3_column_text` gives `highlight` and `snippet`.
        Ok(self
            .offsets
            .get(column)
            .and_then(|at| content.get(*at))
            .and_then(text_of))
    }

    /// Returns every term the query asked for, across its phrases.
    /// Returns `offsets(t)`: where every hit in the row is, as four numbers.
    ///
    /// **Column, term, byte offset, byte length**, one quadruple per hit,
    /// separated by single spaces and in column-then-position order. It is
    /// FTS3's most direct answer - the caller gets the positions and does its
    /// own marking - and the term number is the *query's*, so a two-word query
    /// reports 0 for one word and 1 for the other however they interleave in
    /// the text.
    ///
    /// @param context - the running statement
    fn offsets(&mut self, context: &mut Context<'_>) -> DbResult<Value<'static>> {
        let wanted = self.wanted_terms();
        let mut out: Vec<String> = Vec::new();
        for column in 0..self.columns {
            let Some(text) = self.column_text(context, column)? else {
                continue;
            };
            for (token, start, stop) in self.tokenizer.spans(&text) {
                let Some(term) = wanted
                    .iter()
                    .position(|term| matches_a_term(&token, std::slice::from_ref(term)))
                else {
                    continue;
                };
                out.push(format!(
                    "{column} {term} {start} {}",
                    stop.saturating_sub(start)
                ));
            }
        }
        Value::owned_text(out.join(" ").as_bytes())
    }

    /// Returns `matchinfo(t, format)`: the counts a caller ranks with.
    ///
    /// A blob of native-endian 32-bit integers, one group per format letter,
    /// in the order the letters were written. `pcx` is the default and is what
    /// FTS3's own documentation builds its example ranking function from:
    ///
    /// | letter | what it contributes |
    /// |---|---|
    /// | `p` | how many phrases the query has |
    /// | `c` | how many columns the table has |
    /// | `x` | three numbers per phrase and column: hits in this row, hits in every row, and rows with at least one |
    /// | `n` | how many rows the table holds |
    /// | `a` | the average token count of each column |
    /// | `l` | this row's token count per column |
    ///
    /// @param context - the running statement
    /// @param arguments - the format string, when one was written
    fn matchinfo(
        &mut self,
        context: &mut Context<'_>,
        arguments: &[Value<'static>],
    ) -> DbResult<Value<'static>> {
        let format = match arguments.first() {
            Some(Value::Text(text)) => text.utf8_bytes().into_owned(),
            _ => b"pcx".to_vec(),
        };
        let Some(row) = self.rows.get(self.at).cloned() else {
            return Ok(Value::Null);
        };
        self.ensure_hits(context)?;
        let sizes = bm25::row_sizes(context, &self.shadows, row.rowid, self.columns)?;
        let mut out: Vec<u8> = Vec::new();
        let mut push = |number: i64| out.extend_from_slice(&(number as u32).to_ne_bytes());
        for letter in format {
            match letter {
                b'p' => push(self.phrases.len() as i64),
                b'c' => push(self.columns as i64),
                b'n' => push(self.totals.rows),
                b'a' => {
                    for column in 0..self.columns {
                        let total = self.totals.tokens.get(column).copied().unwrap_or(0);
                        let average = if self.totals.rows > 0 {
                            total / self.totals.rows
                        } else {
                            0
                        };
                        push(average);
                    }
                }
                b'l' => {
                    for column in 0..self.columns {
                        push(sizes.get(column).copied().unwrap_or(0));
                    }
                }
                b'x' => {
                    for at in 0..self.phrases.len() {
                        let hits = self.evaluated.hits.get(at);
                        for column in 0..self.columns {
                            let (here, every, rows) = match hits {
                                Some(hits) => phrase_counts(hits, row.rowid, column),
                                None => (0, 0, 0),
                            };
                            push(here);
                            push(every);
                            push(rows);
                        }
                    }
                }
                _ => {}
            }
        }
        Value::owned_blob(&out)
    }

    /// Returns every term the query asked for, in order.
    fn wanted_terms(&self) -> Vec<expr::Term> {
        self.phrases
            .iter()
            .flat_map(|phrase| phrase.terms.iter().cloned())
            .collect()
    }

    /// Finds the one row a `rowid = ?` plan names, when the table has it.
    ///
    /// `%_docsize` holds one row per indexed document and a contentless table has no
    /// `%_content` to ask, so it is the record of which rowids exist for both.
    ///
    /// @param context - the connection's shadow table access
    /// @param plan - the plan, whose first argument is the rowid
    fn filter_by_rowid(&mut self, context: &mut Context<'_>, plan: &FilterPlan) -> DbResult<()> {
        let Some(rowid) = plan
            .arguments
            .first()
            .and_then(super::constraints::rowid_of)
        else {
            return Ok(());
        };
        let suffix: &[u8] = match self.contentless {
            true => b"docsize",
            false => &self.content.clone(),
        };
        if self.shadows.read_row(context, suffix, rowid)?.is_some() {
            self.rows.push(MatchedRow { rowid, score: None });
        }
        Ok(())
    }
}
/// Returns an error whose own words reach the caller, for a misuse of an auxiliary function.
///
/// @param said - the message SQLite gives for the same misuse
fn function_refusal(said: &str) -> inillucent_base::DbError {
    crate::vtab::failure(said.to_string()).with_message(said.to_string())
}

impl VirtualCursor for Fts5Cursor {
    /// Runs the query and collects every row it matched.
    fn filter(&mut self, context: &mut Context<'_>, plan: &FilterPlan) -> DbResult<()> {
        if let Some(what) = self.unsupported.clone() {
            return Err(super::unsupported_option(&what));
        }
        self.rows.clear();
        self.special = None;
        self.query = None;
        self.evaluated = Evaluated::default();
        self.phrases.clear();
        self.at = 0;
        self.held = None;
        if plan.index_number == PLAN_ROWID {
            return self.filter_by_rowid(context, plan);
        }
        if plan.index_number & PLAN_MATCH == 0 {
            // **An external content table reads through to the owner.** SQLite
            // answers `SELECT * FROM t` on `content='n'` with every row of `n`,
            // before any `rebuild`, because the scan runs a query over `n` and
            // never looks at the index. That holds for FTS3, FTS4 and FTS5, and
            // for an owner that is a view as well as for one that is a table.
            // A contentless table has no owner, so `%_docsize` has the rows.
            let reads_owner = self.external.is_some();
            let suffix: &[u8] = if reads_owner {
                b""
            } else if self.contentless {
                b"docsize"
            } else {
                b"content"
            };
            self.shadows.scan(context, suffix, |rowid, _| {
                self.rows.push(MatchedRow { rowid, score: None });
                Ok(true)
            })?;
            return Ok(());
        }
        // **Here rather than at the top of `filter`, because this is where the
        // index is read.** The two plans above answer out of `%_content` and
        // `%_docsize`, which every layout holds the same way - which is why
        // 0.1.1 answered `count(*)` over a newer index correctly and answered
        // the `MATCH` with nothing. The refusal belongs on the one query whose
        // answer depends on the dictionary. See `layout.rs`.
        super::layout::readable(context, &self.shadows, &self.pending, &self.table)?;
        self.run_match(context, plan)
    }

    /// Moves to the next matching row.
    fn next(&mut self, _context: &mut Context<'_>) -> DbResult<()> {
        self.at = self.at.saturating_add(1);
        self.held = None;
        Ok(())
    }

    /// Returns whether the walk is finished.
    fn eof(&self) -> bool {
        self.at >= self.rows.len()
    }

    /// Returns one column of the current row.
    fn column(&mut self, context: &mut Context<'_>, index: usize) -> DbResult<Value<'static>> {
        let Some(row) = self.rows.get(self.at).cloned() else {
            return Ok(Value::Null);
        };
        if index as i32 == self.rank_column {
            // In FTS3 this slot is `docid`, which is the rowid under another
            // name and is the way that dialect writes a row's identity.
            if self.dialect == Dialect::Three {
                return Ok(Value::Integer(row.rowid));
            }
            if let Some(score) = row.score {
                return Ok(Value::Real(score));
            }
            if self.special.is_some() {
                return Ok(Value::Null);
            }
            // **An unknown rank function reads as NULL here and is refused by `ORDER BY rank`.**
            // The engine reads every column of a matched row for `count(*)` and `SELECT rowid`,
            // and SQLite refuses the function only when the statement asks for `rank`, so an
            // error from this read would fail statements that never mention it.
            if self.refuse_unknown_rank_function().is_err() {
                return Ok(Value::Null);
            }
            if self.phrases.is_empty() {
                // A row reached without a `MATCH` has nothing to score against,
                // which SQLite answers NULL for.
                return Ok(Value::Null);
            }
            // The same arithmetic `bm25::score` would have done in `filter`,
            // for this row alone, with the weights the `rank` setting names,
            // so the two cannot disagree.
            self.ensure_hits(context)?;
            if self.rank_weights.is_empty() {
                self.rank_weights = self.load_rank_weights(context)?;
            }
            let sizes = bm25::row_sizes(context, &self.shadows, row.rowid, self.columns)?;
            let live = self.live_positions();
            return Ok(Value::Real(bm25::score_row(
                &self.stats,
                &live,
                &sizes,
                &self.rank_weights,
            )));
        }
        if index as i32 == self.match_column {
            // The hidden column that carries the query exists to be constrained, and read as a
            // value it is the number of the cursor, as it is in SQLite. After a special query it
            // is that query's answer.
            if let Some(value) = self.special {
                return Ok(Value::Integer(value));
            }
            if self.dialect == Dialect::Five {
                return Ok(Value::Integer(self.cursor_id));
            }
            return Ok(Value::Null);
        }
        if self.special.is_some() {
            return Ok(Value::Null);
        }
        if self.contentless {
            return Ok(Value::Null);
        }
        if self.held.as_ref().map(|(rowid, _)| *rowid) != Some(row.rowid) {
            self.resolve_offsets(context);
            self.held = self
                .shadows
                .read_row(context, &self.content.clone(), row.rowid)?
                .map(|values| (row.rowid, values));
        }
        let Some((_, content)) = self.held.as_ref() else {
            return Ok(Value::Null);
        };
        Ok(self
            .offsets
            .get(index)
            .and_then(|at| content.get(*at))
            .cloned()
            .unwrap_or(Value::Null))
    }

    /// Returns the row's rowid.
    fn rowid(&self) -> DbResult<i64> {
        Ok(self.rows.get(self.at).map(|row| row.rowid).unwrap_or(0))
    }

    /// Answers `bm25(t [, weight...])` on the current row.
    ///
    /// The weights are per call, which is why this cannot be a column: the
    /// same row scores differently in `bm25(docs)` and `bm25(docs, 10.0, 1.0)`
    /// in the same SELECT. A row reached without a `MATCH` scores zero, which
    /// is what SQLite answers for a function that had no query to score
    /// against.
    fn auxiliary(
        &mut self,
        context: &mut Context<'_>,
        name: &[u8],
        arguments: &[Value<'static>],
    ) -> DbResult<Value<'static>> {
        if name == b"highlight" {
            if arguments.len() != 3 {
                return Err(function_refusal(
                    "wrong number of arguments to function highlight()",
                ));
            }
            return self.highlight(context, arguments);
        }
        if name == b"snippet" {
            // FTS3 and FTS4 choose and write fragments by their own routine.
            if self.dialect == Dialect::Three {
                return self.snippet_three(context, arguments);
            }
            if arguments.len() != 5 {
                return Err(function_refusal(
                    "wrong number of arguments to function snippet()",
                ));
            }
            let column = int_argument(arguments.first());
            if column >= i64::try_from(self.columns).unwrap_or(i64::MAX) {
                return Err(function_refusal("column index out of range"));
            }
            return self.snippet(context, arguments);
        }
        if name == b"offsets" && self.dialect == Dialect::Three {
            return self.offsets(context);
        }
        if name == b"matchinfo" && self.dialect == Dialect::Three {
            return self.matchinfo(context, arguments);
        }
        // **`optimize(t)`, which is FTS3/4's and always has nothing to do
        // here.** SQLite merges an FTS3/4 index's b-tree segments into one and
        // answers `Index already optimal` when there is nothing to merge. This
        // index keeps one doclist per term - see `stage_doclist` - so there are
        // never segments to merge and the answer is the second case, always.
        // Saying so is the truthful answer; refusing the name was not, and it
        // was the last function of SQLite's register that this engine answered
        // to by another route and did not name.
        if name == b"optimize" && self.dialect == Dialect::Three {
            return Value::owned_text(b"Index already optimal");
        }
        if name != b"bm25" {
            return Err(crate::vtab::failure(format!(
                "no such function: {}",
                String::from_utf8_lossy(name)
            )));
        }
        let Some(row) = self.rows.get(self.at).cloned() else {
            return Ok(Value::Null);
        };
        if self.phrases.is_empty() {
            return Ok(Value::Real(0.0));
        }
        let mut weights = vec![1.0f64; self.columns];
        for (slot, argument) in weights.iter_mut().zip(arguments.iter()) {
            *slot = weight_of(argument);
        }
        // The hits are the query's, so the same maps score every row of it.
        self.ensure_hits(context)?;
        let sizes = bm25::row_sizes(context, &self.shadows, row.rowid, self.columns)?;
        let live = self.live_positions();
        Ok(Value::Real(bm25::score_row(
            &self.stats,
            &live,
            &sizes,
            &weights,
        )))
    }
}

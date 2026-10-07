//! The SELECT grammar: `WITH`, compound arms, FROM terms, and windows.
//!
//! Invariant: nothing is normalised here. A comma join and a `CROSS JOIN` are
//! different nodes even though both are cross joins, because SQLite refuses to
//! reorder one and will reorder the other; `VALUES` is its own arm rather than
//! a `SELECT` over nothing; and an `ORDER BY` after a compound belongs to the
//! compound, not to its last arm.

use super::Parser;
use crate::ast::{
    CommonTableExpr, CompoundOp, FromSource, FromTerm, FromTermId, IndexHint, JoinConstraint,
    JoinKind, NameId, ResultColumn, Select, SelectBody, SelectCore, SelectCoreId, SelectId,
    Statement, With,
};
use crate::diagnostic::{ParseError, ParseErrorKind};
use crate::keyword::Keyword;
use crate::lexer::{Punctuator, Span};
use inillucent_base::limits::Limit;

/// The `ORDER BY` terms, `LIMIT` and `OFFSET` written after one arm.
type SelectTail = (
    Vec<crate::ast::OrderTerm>,
    Option<crate::ast::ExprId>,
    Option<crate::ast::ExprId>,
);

/// Returns the words SQLite uses for a compound operator in its refusals.
///
/// @param op - the operator
fn compound_operator_name(op: CompoundOp) -> &'static str {
    match op {
        CompoundOp::Union => "UNION",
        CompoundOp::UnionAll => "UNION ALL",
        CompoundOp::Intersect => "INTERSECT",
        CompoundOp::Except => "EXCEPT",
    }
}

impl Parser<'_> {
    /// Parses a SELECT statement, including any `WITH` prefix.
    pub(super) fn parse_select_statement(&mut self) -> Result<Statement, ParseError> {
        Ok(Statement::Select(self.parse_select()?))
    }

    /// Parses a complete SELECT: `WITH`, arms, `ORDER BY`, `LIMIT`.
    pub(super) fn parse_select(&mut self) -> Result<SelectId, ParseError> {
        // Counted so `CHECK` can tell whether the expression it just read
        // contained a subquery. See `Parser::no_subquery_in_check`.
        self.selects = self.selects.saturating_add(1);
        self.enter()?;
        let parsed = self.parse_select_inner();
        self.leave();
        parsed
    }

    /// The body of [`Parser::parse_select`], with the depth charge applied.
    fn parse_select_inner(&mut self) -> Result<SelectId, ParseError> {
        // The span starts before the `WITH`, because it is the whole query:
        // `CREATE TABLE ... AS` fills the table by running the text this span
        // covers, and a span that began after the prefix ran the query without
        // its CTEs, so `CREATE TABLE x AS WITH c AS (...) SELECT a FROM c` was
        // "no such table: c".
        let start = self.cursor();
        let with = self.parse_with_prefix()?;
        let mut last_is_values = self.at_keyword(Keyword::VALUES)?;
        let first = self.parse_select_core()?;
        let mut compounds = Vec::new();
        let mut misplaced: Option<(&'static str, CompoundOp)> = None;
        let (order_by, limit, offset) = loop {
            let tail = self.parse_select_tail(last_is_values)?;
            let Some(op) = self.parse_compound_operator()? else {
                break tail;
            };
            // **An `ORDER BY` or a `LIMIT` before a compound operator is read
            // and then refused by name.** SQLite's grammar gives every arm a
            // tail and reports the rightmost arm that has one but is not last.
            if !tail.0.is_empty() || tail.1.is_some() {
                let clause = if tail.0.is_empty() {
                    "LIMIT"
                } else {
                    "ORDER BY"
                };
                misplaced = Some((clause, op));
            }
            if compounds.len() as i64 >= self.limits.get(Limit::CompoundSelect) {
                return Err(ParseError::new(
                    ParseErrorKind::LimitExceeded("too many terms in compound SELECT"),
                    Span::at(self.cursor()),
                ));
            }
            last_is_values = self.at_keyword(Keyword::VALUES)?;
            compounds.push((op, self.parse_select_core()?));
        };
        if let Some((clause, op)) = misplaced {
            return Err(ParseError::new(
                ParseErrorKind::Refused(format!(
                    "{clause} clause should come after {} not before",
                    compound_operator_name(op)
                )),
                Span::default(),
            ));
        }
        let end = self.cursor();
        Ok(self.ast.add_select(Select {
            with,
            first,
            compounds,
            order_by,
            limit,
            offset,
            span: Span::new(start, end),
            nested_from: false,
        }))
    }

    /// Parses the `ORDER BY` and `LIMIT` that follow one arm of a `SELECT`.
    ///
    /// **`ORDER BY` and `LIMIT` belong to a `SELECT`, never to a `VALUES`.** In
    /// SQLite's grammar they are the tail of a `SELECT` core, and the compound's
    /// last core carries them for the whole compound; a `VALUES` core has no
    /// tail. So when the arm is a `VALUES`, one row or several, the `ORDER` or
    /// `LIMIT` after it is a syntax error there - `SELECT 1, 2 INTERSECT VALUES
    /// (1, 1), (2, 2) ORDER BY 1, 2` included - and it was accepted here.
    ///
    /// @param is_values - whether the arm just read is a `VALUES` list
    fn parse_select_tail(&mut self, is_values: bool) -> Result<SelectTail, ParseError> {
        if is_values && (self.at_keyword(Keyword::ORDER)? || self.at_keyword(Keyword::LIMIT)?) {
            return Err(self.unexpected(&["the end of the statement"])?);
        }
        let order_by = if self.at_keyword(Keyword::ORDER)? {
            self.bump()?;
            self.expect_keyword(Keyword::BY)?;
            self.parse_order_terms()?
        } else {
            Vec::new()
        };
        let (limit, offset) = self.parse_limit_clause()?;
        Ok((order_by, limit, offset))
    }

    /// Parses `LIMIT expr [OFFSET expr | , expr]`.
    ///
    /// The comma form reverses the operands: `LIMIT a, b` means offset `a`,
    /// limit `b`, which is the opposite of what it reads like.
    pub(super) fn parse_limit_clause(
        &mut self,
    ) -> Result<(Option<crate::ast::ExprId>, Option<crate::ast::ExprId>), ParseError> {
        if !self.eat_keyword(Keyword::LIMIT)? {
            return Ok((None, None));
        }
        let first = self.parse_expr()?;
        if self.eat_keyword(Keyword::OFFSET)? {
            return Ok((Some(first), Some(self.parse_expr()?)));
        }
        if self.eat(Punctuator::Comma)? {
            let second = self.parse_expr()?;
            return Ok((Some(second), Some(first)));
        }
        Ok((Some(first), None))
    }

    /// Parses a `WITH [RECURSIVE] name AS (...)` prefix, if there is one.
    pub(super) fn parse_with_prefix(&mut self) -> Result<With, ParseError> {
        if !self.eat_keyword(Keyword::WITH)? {
            return Ok(With::default());
        }
        let recursive = self.eat_keyword(Keyword::RECURSIVE)?;
        let mut ctes = Vec::new();
        loop {
            let name = self.parse_name()?;
            let mut columns = Vec::new();
            if self.at(Punctuator::LeftParen)? && !self.peek_at(1)?.is(Punctuator::RightParen) {
                // A CTE's column list and its body are both parenthesised; the
                // column list is the one that is not followed by SELECT.
                let is_column_list = !matches!(
                    self.peek_at(1)?.keyword(),
                    Some(Keyword::SELECT) | Some(Keyword::VALUES) | Some(Keyword::WITH)
                );
                if is_column_list {
                    self.bump()?;
                    loop {
                        columns.push(self.parse_name()?);
                        if !self.eat(Punctuator::Comma)? {
                            break;
                        }
                    }
                    self.expect(Punctuator::RightParen)?;
                }
            }
            self.expect_keyword(Keyword::AS)?;
            let materialized = if self.eat_keyword(Keyword::MATERIALIZED)? {
                Some(true)
            } else if self.at_keyword(Keyword::NOT)?
                && self.at_keyword_ahead(1, Keyword::MATERIALIZED)?
            {
                self.bump()?;
                self.bump()?;
                Some(false)
            } else {
                None
            };
            self.expect(Punctuator::LeftParen)?;
            let select = self.parse_select()?;
            self.expect(Punctuator::RightParen)?;
            ctes.push(CommonTableExpr {
                name,
                columns,
                materialized,
                select,
            });
            if !self.eat(Punctuator::Comma)? {
                break;
            }
        }
        Ok(With { recursive, ctes })
    }

    /// Parses a compound operator, if the next tokens spell one.
    fn parse_compound_operator(&mut self) -> Result<Option<CompoundOp>, ParseError> {
        if self.at_keyword(Keyword::UNION)? {
            self.bump()?;
            if self.eat_keyword(Keyword::ALL)? {
                return Ok(Some(CompoundOp::UnionAll));
            }
            return Ok(Some(CompoundOp::Union));
        }
        if self.eat_keyword(Keyword::INTERSECT)? {
            return Ok(Some(CompoundOp::Intersect));
        }
        if self.eat_keyword(Keyword::EXCEPT)? {
            return Ok(Some(CompoundOp::Except));
        }
        Ok(None)
    }

    /// Parses one arm: a `SELECT ...` or a `VALUES ...`.
    fn parse_select_core(&mut self) -> Result<SelectCoreId, ParseError> {
        let start = self.cursor();
        if self.at_keyword(Keyword::VALUES)? {
            self.bump()?;
            let mut rows = Vec::new();
            loop {
                self.expect(Punctuator::LeftParen)?;
                // A row has at least one value: SQLite reads `VALUES()` as a syntax
                // error at the `)`, so the first expression is required.
                let mut row = Vec::new();
                loop {
                    row.push(self.parse_expr()?);
                    if !self.eat(Punctuator::Comma)? {
                        break;
                    }
                }
                self.expect(Punctuator::RightParen)?;
                rows.push(row);
                if !self.eat(Punctuator::Comma)? {
                    break;
                }
            }
            let end = self.cursor();
            return Ok(self.ast.add_core(SelectCore {
                body: SelectBody::Values(rows),
                span: Span::new(start, end),
            }));
        }
        self.expect_keyword(Keyword::SELECT)?;
        let distinct = self.eat_keyword(Keyword::DISTINCT)?;
        let all = if distinct {
            false
        } else {
            self.eat_keyword(Keyword::ALL)?
        };
        let columns = self.parse_result_columns()?;
        let from = if self.eat_keyword(Keyword::FROM)? {
            self.parse_from_clause()?
        } else {
            Vec::new()
        };
        let filter = if self.eat_keyword(Keyword::WHERE)? {
            Some(self.parse_expr()?)
        } else {
            None
        };
        let mut group_by = Vec::new();
        if self.at_keyword(Keyword::GROUP)? {
            self.bump()?;
            self.expect_keyword(Keyword::BY)?;
            loop {
                group_by.push(self.parse_expr()?);
                if !self.eat(Punctuator::Comma)? {
                    break;
                }
            }
        }
        // **`HAVING` is parsed whether or not a `GROUP BY` came first
        // (task-2040).** It used to be read inside the `GROUP BY` arm, so
        // `SELECT count(*) AS n FROM t HAVING n > 0` - which SQLite answers
        // with the count, because a query aggregating with no `GROUP BY` is
        // one group over the whole table - stopped at
        // `near "HAVING": syntax error`. A syntax error is the worst answer
        // available for it: exit code 3 and the `unsupported` status exist so a
        // caller can tell "not built" from "your SQL is wrong", and this said
        // the SQL was wrong about a statement that is correct. Which of the
        // parsed shapes are legal is the binder's question, and
        // `bind_select_core` answers it in SQLite's own words.
        let having = if self.eat_keyword(Keyword::HAVING)? {
            Some(self.parse_expr()?)
        } else {
            None
        };
        let windows = self.parse_window_clause()?;
        let end = self.cursor();
        Ok(self.ast.add_core(SelectCore {
            body: SelectBody::Select {
                distinct,
                all,
                columns,
                from,
                filter,
                group_by,
                having,
                windows,
            },
            span: Span::new(start, end),
        }))
    }

    /// Parses the result column list.
    fn parse_result_columns(&mut self) -> Result<Vec<ResultColumn>, ParseError> {
        let mut columns = Vec::new();
        loop {
            let start = self.cursor();
            let expr = self.parse_expr()?;
            let (alias, alias_was_explicit) = match self.ast.expr(expr) {
                // `*` and `t.*` take no alias; a word after them is a syntax
                // error rather than an alias, which is what SQLite reports.
                Some(crate::ast::Expr::Star { .. }) => (None, false),
                _ => self.parse_alias()?,
            };
            let end = self.cursor();
            columns.push(ResultColumn {
                expr,
                alias,
                alias_was_explicit,
                span: Span::new(start, end),
            });
            if columns.len() as i64 > self.limits.get(Limit::Column) {
                return Err(ParseError::new(
                    ParseErrorKind::LimitExceeded("too many columns in result set"),
                    Span::at(start),
                ));
            }
            if !self.eat(Punctuator::Comma)? {
                return Ok(columns);
            }
        }
    }

    /// Parses a FROM clause: a first term followed by joined terms.
    pub(super) fn parse_from_clause(&mut self) -> Result<Vec<FromTermId>, ParseError> {
        let mut terms = Vec::new();
        let first = self.parse_from_term(JoinKind::Comma, false, JoinConstraint::None)?;
        // **The first term takes an `ON` or `USING` in SQLite's grammar and
        // refuses it afterwards.** That is the whole reason an upsert after an
        // `INSERT ... SELECT ... FROM t` with no `WHERE` is a syntax error:
        // `ON CONFLICT(k)` is read as a join condition, and the parser stops
        // at the `DO` that follows it.
        let at = self.cursor();
        match self.parse_join_constraint()? {
            JoinConstraint::None => {}
            constraint => {
                let word = if matches!(constraint, JoinConstraint::On(_)) {
                    "ON"
                } else {
                    "USING"
                };
                return Err(ParseError::new(
                    ParseErrorKind::Refused(format!("a JOIN clause is required before {word}")),
                    Span::at(at),
                ));
            }
        }
        // The first term of a FROM clause is left flat unless it has an alias.
        self.wrap_joined_group(first, true);
        terms.push(first);
        loop {
            let Some((join, natural)) = self.parse_join_operator()? else {
                return Ok(terms);
            };
            let start = self.cursor();
            let term = self.parse_from_term(join, natural, JoinConstraint::None)?;
            self.wrap_joined_group(term, false);
            let constraint = self.parse_join_constraint()?;
            if natural && constraint != JoinConstraint::None {
                return Err(ParseError::new(
                    ParseErrorKind::Unsupported("a NATURAL join may not have ON or USING"),
                    Span::at(start),
                ));
            }
            if let Some(stored) = self.ast_from_term_mut(term) {
                stored.constraint = constraint;
            }
            terms.push(term);
        }
    }

    /// Returns a mutable handle to a stored FROM term.
    ///
    /// The constraint is parsed after the term it belongs to, because `ON` and
    /// `USING` follow the table rather than precede it, so the term is patched
    /// once rather than being built out of order.
    fn ast_from_term_mut(&mut self, id: FromTermId) -> Option<&mut FromTerm> {
        self.ast.from_term_mut(id)
    }

    /// Parses a join operator, returning the kind and whether it was natural.
    ///
    /// SQLite's grammar is `JOIN`, or one to three join words and then `JOIN`,
    /// in any order and with repeats, so `OUTER LEFT NATURAL JOIN` and
    /// `LEFT RIGHT JOIN` are accepted. The words are read as a set of flags
    /// the way `sqlite3JoinType` does, so the kind is the same one it picks.
    fn parse_join_operator(&mut self) -> Result<Option<(JoinKind, bool)>, ParseError> {
        if self.eat(Punctuator::Comma)? {
            return Ok(Some((JoinKind::Comma, false)));
        }
        let mut words = 0;
        while words < 3 && Self::is_join_word(self.peek_at(words)?.keyword()) {
            words += 1;
        }
        if words == 0 || self.peek_at(words)?.keyword() != Some(Keyword::JOIN) {
            if words == 0 && self.at_keyword(Keyword::JOIN)? {
                self.bump()?;
                return Ok(Some((JoinKind::Inner, false)));
            }
            return Ok(None);
        }
        let first = self.cursor();
        let (mut natural, mut left, mut right, mut outer) = (false, false, false, false);
        let (mut inner, mut cross) = (false, false);
        let mut written = Vec::new();
        for _ in 0..words {
            written.push(String::from_utf8_lossy(self.peek()?.text(self.source())).into_owned());
            match self.peek()?.keyword() {
                Some(Keyword::NATURAL) => natural = true,
                Some(Keyword::LEFT) => (left, outer) = (true, true),
                Some(Keyword::RIGHT) => (right, outer) = (true, true),
                Some(Keyword::FULL) => (left, right, outer) = (true, true, true),
                Some(Keyword::OUTER) => outer = true,
                Some(Keyword::INNER) => inner = true,
                _ => (inner, cross) = (true, true),
            }
            self.bump()?;
        }
        self.expect_keyword(Keyword::JOIN)?;
        // SQLite refuses a mix of inner and outer words, and `OUTER` on its own.
        if (inner && outer) || (outer && !left && !right) {
            return Err(ParseError::new(
                ParseErrorKind::Refused(format!("unknown join type: {}", written.join(" "))),
                Span::at(first),
            ));
        }
        let kind = match (left, right) {
            (true, true) => JoinKind::Full,
            (true, false) => JoinKind::Left,
            (false, true) => JoinKind::Right,
            _ if cross => JoinKind::Cross,
            _ => JoinKind::Inner,
        };
        Ok(Some((kind, natural)))
    }

    /// Returns whether a keyword is one of the words SQLite allows before `JOIN`.
    fn is_join_word(keyword: Option<Keyword>) -> bool {
        matches!(
            keyword,
            Some(Keyword::NATURAL)
                | Some(Keyword::LEFT)
                | Some(Keyword::RIGHT)
                | Some(Keyword::FULL)
                | Some(Keyword::OUTER)
                | Some(Keyword::INNER)
                | Some(Keyword::CROSS)
        )
    }

    /// Parses `ON expr` or `USING (a, b)`.
    fn parse_join_constraint(&mut self) -> Result<JoinConstraint, ParseError> {
        if self.eat_keyword(Keyword::ON)? {
            let condition = self.parse_expr()?;
            // `DO` cannot follow a join condition, and is what follows
            // `ON CONFLICT(k)` read as one; see `parse_from_clause`.
            if self.at_keyword(Keyword::DO)? {
                return Err(self.unexpected(&["end of the FROM clause"])?);
            }
            return Ok(JoinConstraint::On(condition));
        }
        if self.eat_keyword(Keyword::USING)? {
            self.expect(Punctuator::LeftParen)?;
            let mut columns = Vec::new();
            loop {
                columns.push(self.parse_name()?);
                if !self.eat(Punctuator::Comma)? {
                    break;
                }
            }
            self.expect(Punctuator::RightParen)?;
            return Ok(JoinConstraint::Using(columns));
        }
        Ok(JoinConstraint::None)
    }

    /// Parses one FROM term: a table, a subquery, or a parenthesised join.
    pub(super) fn parse_from_term(
        &mut self,
        join: JoinKind,
        natural: bool,
        constraint: JoinConstraint,
    ) -> Result<FromTermId, ParseError> {
        let start = self.cursor();
        if self.at(Punctuator::LeftParen)? {
            self.bump()?;
            let source = if self.at_keyword(Keyword::SELECT)?
                || self.at_keyword(Keyword::WITH)?
                || self.at_keyword(Keyword::VALUES)?
            {
                FromSource::Subquery(self.parse_select()?)
            } else {
                FromSource::Join(self.parse_from_clause()?)
            };
            self.expect(Punctuator::RightParen)?;
            let (alias, _) = self.parse_alias()?;
            let end = self.cursor();
            return Ok(self.ast.add_from_term(FromTerm {
                source,
                alias,
                join,
                natural,
                constraint,
                span: Span::new(start, end),
            }));
        }
        let (database, name) = self.parse_qualified_name()?;
        let arguments = if self.at(Punctuator::LeftParen)? {
            self.bump()?;
            let mut list = Vec::new();
            if !self.at(Punctuator::RightParen)? {
                loop {
                    list.push(self.parse_expr()?);
                    if !self.eat(Punctuator::Comma)? {
                        break;
                    }
                }
            }
            self.expect(Punctuator::RightParen)?;
            Some(list)
        } else {
            None
        };
        let (alias, _) = self.parse_alias()?;
        let indexed_by = self.parse_index_hint()?;
        let end = self.cursor();
        Ok(self.ast.add_from_term(FromTerm {
            source: FromSource::Table {
                database,
                name,
                arguments,
                indexed_by,
            },
            alias,
            join,
            natural,
            constraint,
            span: Span::new(start, end),
        }))
    }

    /// Turns a parenthesised join of several terms into a subquery.
    ///
    /// **SQLite reads it as `(SELECT * FROM ...)`** unless it is the first term
    /// of the FROM clause with no alias: its `USING` columns merge inside it,
    /// the `ON` or `USING` written after it joins it as one term, and a column
    /// name two of its terms share is renamed `a:1`. Flattening it into the
    /// enclosing list lost the condition written after the closing parenthesis,
    /// and the null extension a `LEFT JOIN` gives the group as a whole.
    ///
    /// @param term - the FROM term just parsed
    /// @param first - whether it opens the FROM clause
    fn wrap_joined_group(&mut self, term: FromTermId, first: bool) {
        let Some(held) = self.ast.from_term(term) else {
            return;
        };
        let FromSource::Join(inner) = &held.source else {
            return;
        };
        if inner.len() == 1 && !(first && held.alias.is_none()) {
            self.merge_single_group(term);
            return;
        }
        if inner.len() < 2 || (first && held.alias.is_none()) {
            return;
        }
        let (inner, span) = (inner.clone(), held.span);
        let select = self.select_over_terms(inner, span);
        if let Some(stored) = self.ast_from_term_mut(term) {
            stored.source = FromSource::Subquery(select);
        }
    }

    /// Replaces a parenthesised group of one term with that term.
    ///
    /// **SQLite does this when the group has an alias or follows a join**
    /// (`seltablist ::= stl_prefix LP seltablist RP as on_using`). The outer
    /// term keeps its join kind, alias and constraint and takes the inner
    /// term's source, so `JOIN (t2) AS x USING (b)` merges `b` like
    /// `JOIN t2 AS x USING (b)`. Wrapping it in a subquery gave the result
    /// `b` twice. The inner term's own alias and index hint are dropped, as
    /// SQLite drops them.
    ///
    /// @param term - the FROM term whose source is a join of exactly one term
    fn merge_single_group(&mut self, term: FromTermId) {
        let Some(held) = self.ast.from_term(term) else {
            return;
        };
        let FromSource::Join(inner) = &held.source else {
            return;
        };
        let Some(only) = inner.first().and_then(|id| self.ast.from_term(*id)) else {
            return;
        };
        let mut source = only.source.clone();
        if let FromSource::Table { indexed_by, .. } = &mut source {
            *indexed_by = IndexHint::None;
        }
        if let Some(stored) = self.ast_from_term_mut(term) {
            stored.source = source;
        }
    }

    /// Builds `SELECT * FROM <terms>` for a parenthesised join.
    ///
    /// @param terms - the joined terms inside the parentheses
    /// @param span - where the parenthesised join was written
    fn select_over_terms(&mut self, terms: Vec<FromTermId>, span: Span) -> SelectId {
        let star = self
            .ast
            .add_expr(crate::ast::Expr::Star { table: None }, span);
        let core = self.ast.add_core(SelectCore {
            body: SelectBody::Select {
                distinct: false,
                all: false,
                columns: vec![ResultColumn {
                    expr: star,
                    alias: None,
                    alias_was_explicit: false,
                    span,
                }],
                from: terms,
                filter: None,
                group_by: Vec::new(),
                having: None,
                windows: Vec::new(),
            },
            span,
        });
        self.ast.add_select(Select {
            with: With::default(),
            first: core,
            compounds: Vec::new(),
            order_by: Vec::new(),
            limit: None,
            offset: None,
            span,
            nested_from: true,
        })
    }

    /// Parses `INDEXED BY name` or `NOT INDEXED`.
    fn parse_index_hint(&mut self) -> Result<IndexHint, ParseError> {
        if self.at_keyword(Keyword::INDEXED)? {
            self.bump()?;
            self.expect_keyword(Keyword::BY)?;
            return Ok(IndexHint::IndexedBy(self.parse_name()?));
        }
        if self.at_keyword(Keyword::NOT)? && self.at_keyword_ahead(1, Keyword::INDEXED)? {
            self.bump()?;
            self.bump()?;
            return Ok(IndexHint::NotIndexed);
        }
        Ok(IndexHint::None)
    }

    /// Parses a `WINDOW name AS (...)` clause.
    fn parse_window_clause(&mut self) -> Result<Vec<(NameId, crate::ast::WindowId)>, ParseError> {
        if !self.at_keyword(Keyword::WINDOW)? {
            return Ok(Vec::new());
        }
        self.bump()?;
        let mut windows = Vec::new();
        loop {
            let name = self.parse_name()?;
            self.expect_keyword(Keyword::AS)?;
            let (window, _) = self.parse_over_clause()?;
            windows.push((name, window));
            if !self.eat(Punctuator::Comma)? {
                return Ok(windows);
            }
        }
    }

    /// Parses the body of an `OVER` clause, which is either a window name or a
    /// parenthesised definition.
    pub(super) fn parse_over_clause(&mut self) -> Result<(crate::ast::WindowId, Span), ParseError> {
        use crate::ast::{FrameBound, FrameExclude, FrameUnit, Window};
        let start = self.cursor();
        if !self.at(Punctuator::LeftParen)? {
            // **The token's span, not the interned name's (task-1913).**
            // Interning deduplicates, so the second `w` in
            // `SELECT first_value(n) OVER w, last_value(n) OVER w` carried the
            // first one's position, and the second column took its name from a
            // slice running backwards through the query: `w, last_value`.
            let (base, span) = self.parse_name_spanned()?;
            let id = self.ast.add_window(Window {
                base: Some(base),
                partition_by: Vec::new(),
                order_by: Vec::new(),
                unit: None,
                start: None,
                end: None,
                exclude: FrameExclude::NoOthers,
                span,
                bare_name: true,
            });
            return Ok((id, span));
        }
        self.expect(Punctuator::LeftParen)?;
        let base = if self.at_name()?
            && !self.at_keyword(Keyword::PARTITION)?
            && !self.at_keyword(Keyword::ORDER)?
            && !self.at_keyword(Keyword::ROWS)?
            && !self.at_keyword(Keyword::RANGE)?
            && !self.at_keyword(Keyword::GROUPS)?
        {
            Some(self.parse_name()?)
        } else {
            None
        };
        let mut partition_by = Vec::new();
        if self.at_keyword(Keyword::PARTITION)? {
            self.bump()?;
            self.expect_keyword(Keyword::BY)?;
            loop {
                partition_by.push(self.parse_expr()?);
                if !self.eat(Punctuator::Comma)? {
                    break;
                }
            }
        }
        let order_by = if self.at_keyword(Keyword::ORDER)? {
            self.bump()?;
            self.expect_keyword(Keyword::BY)?;
            self.parse_order_terms()?
        } else {
            Vec::new()
        };
        let unit = if self.eat_keyword(Keyword::ROWS)? {
            Some(FrameUnit::Rows)
        } else if self.eat_keyword(Keyword::RANGE)? {
            Some(FrameUnit::Range)
        } else if self.eat_keyword(Keyword::GROUPS)? {
            Some(FrameUnit::Groups)
        } else {
            None
        };
        let (frame_start, frame_end) = if unit.is_some() {
            if self.eat_keyword(Keyword::BETWEEN)? {
                let low = self.parse_frame_bound()?;
                self.expect_keyword(Keyword::AND)?;
                let high = self.parse_frame_bound()?;
                (Some(low), Some(high))
            } else {
                (
                    Some(self.parse_frame_bound()?),
                    Some(FrameBound::CurrentRow),
                )
            }
        } else {
            (None, None)
        };
        // **`EXCLUDE` needs a frame clause to exclude anything from
        // (task-1979, F18).** SQLite's grammar hangs the exclusion off the
        // frame rule, so `OVER (ORDER BY v EXCLUDE TIES)` is
        // `near "EXCLUDE": syntax error` there. This parser read it as a
        // separate clause and accepted it against the default frame, which
        // meant four spellings of a frame nobody had written answered rows
        // where the reference answers nothing at all.
        if unit.is_none() && self.at_keyword(Keyword::EXCLUDE)? {
            return Err(self.unexpected(&[")"])?);
        }
        let exclude = if self.eat_keyword(Keyword::EXCLUDE)? {
            if self.eat_keyword(Keyword::NO)? {
                self.expect_keyword(Keyword::OTHERS)?;
                FrameExclude::NoOthers
            } else if self.eat_keyword(Keyword::CURRENT)? {
                self.expect_keyword(Keyword::ROW)?;
                FrameExclude::CurrentRow
            } else if self.eat_keyword(Keyword::GROUP)? {
                FrameExclude::Group
            } else {
                self.expect_keyword(Keyword::TIES)?;
                FrameExclude::Ties
            }
        } else {
            FrameExclude::NoOthers
        };
        let close = self.expect(Punctuator::RightParen)?;
        let span = Span::new(start, close.span.end as usize);
        let id = self.ast.add_window(Window {
            base,
            partition_by,
            order_by,
            unit,
            start: frame_start,
            end: frame_end,
            exclude,
            span,
            bare_name: false,
        });
        Ok((id, span))
    }

    /// Parses one end of a window frame.
    fn parse_frame_bound(&mut self) -> Result<crate::ast::FrameBound, ParseError> {
        use crate::ast::FrameBound;
        if self.eat_keyword(Keyword::UNBOUNDED)? {
            if self.eat_keyword(Keyword::PRECEDING)? {
                return Ok(FrameBound::UnboundedPreceding);
            }
            self.expect_keyword(Keyword::FOLLOWING)?;
            return Ok(FrameBound::UnboundedFollowing);
        }
        if self.at_keyword(Keyword::CURRENT)? {
            self.bump()?;
            self.expect_keyword(Keyword::ROW)?;
            return Ok(FrameBound::CurrentRow);
        }
        let expr = self.parse_expr()?;
        if self.eat_keyword(Keyword::PRECEDING)? {
            return Ok(FrameBound::Preceding(expr));
        }
        self.expect_keyword(Keyword::FOLLOWING)?;
        Ok(FrameBound::Following(expr))
    }

    /// Parses a `RETURNING` clause.
    pub(super) fn parse_returning(&mut self) -> Result<Vec<ResultColumn>, ParseError> {
        if !self.eat_keyword(Keyword::RETURNING)? {
            return Ok(Vec::new());
        }
        self.parse_result_columns()
    }
}

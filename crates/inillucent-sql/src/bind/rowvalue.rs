//! Row values: `(a, b) = (c, d)`, `(a, b) IN ((1, 2), (3, 4))`, and
//! `(a, b) < (SELECT x, y ...)`.
//!
//! Invariant: **nothing below the binder has a row value in it, and this is
//! the only place that stays true.** A row value is a syntax for comparing
//! several columns at once, and it is the whole of what the engine keeps of
//! one: `BoundExpr` has no tuple, the planner has no tuple, and the register
//! machine compares one value against one value. Every spelling here is
//! rewritten into ordinary scalar comparisons joined by `AND`, `OR` and `NOT`
//! before it leaves, so a row value costs the rest of the engine nothing and
//! can never be the reason a later pass has a case it does not handle.
//!
//! That is also why the three desugarings live beside each other rather than
//! next to the operators that reach them. `=` over two rows and `=` over a row
//! and a one-row query are the same chain of equalities over different
//! operands, and `<` over two rows is the lexicographic chain that `IN` does
//! not need at all - but all three have to agree about NULL, about affinity and
//! about collation, or `(1, NULL) <> (1, 2)` answers differently depending on
//! which spelling was written. They agree because they are one function called
//! three times, `compare_bound_rows`, and keeping them together is what makes
//! that visible to the next person to add a fourth spelling.

use inillucent_value::Collation;

use super::{comparison_rules, result_collation, Binder, BoundExpr, SubqueryKind};
use crate::ast::{self, BinaryOp, Expr, ExprId, InRhs, SelectBody};
use crate::bind::refused;
use crate::diagnostic::{ParseError, ParseErrorKind};
use crate::lexer::Span;

impl Binder<'_> {
    /// Binds `(a, b) IN (VALUES (...), (...))`.
    ///
    /// Only the value-list form, because that is what the row-value `IN` is for:
    /// a written list of tuples. `(a, b) IN (SELECT x, y FROM u)` is a
    /// correlated membership test over a query and is refused by name.
    ///
    /// @param parts - the operand row's parts
    /// @param rhs - what was written after `IN`
    /// @param negated - whether `NOT IN` was written
    /// @param span - where the test was written
    pub(super) fn bind_row_in(
        &mut self,
        parts: &[ExprId],
        rhs: &InRhs,
        negated: bool,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let mut bound_lefts = Vec::with_capacity(parts.len());
        for part in parts {
            bound_lefts.push(self.bind_expr(*part)?);
        }
        self.bind_row_in_bound(bound_lefts, rhs, negated, span)
    }

    /// Binds `(a, b) [NOT] IN ...` for an operand row that is already bound.
    ///
    /// @param bound_lefts - the operand row's parts, bound
    /// @param rhs - what was written after `IN`
    /// @param negated - whether `NOT IN` was written
    /// @param span - where the test was written
    pub(super) fn bind_row_in_bound(
        &mut self,
        bound_lefts: Vec<BoundExpr>,
        rhs: &InRhs,
        negated: bool,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let Some(rows) = self.row_value_list(rhs) else {
            return match rhs {
                InRhs::Select(select) => {
                    self.bind_row_in_query(bound_lefts, *select, negated, span)
                }
                _ => Err(ParseError::new(
                    ParseErrorKind::Unsupported("a row value IN a query rather than a value list"),
                    span,
                )),
            };
        };
        let mut bound_rows = Vec::with_capacity(rows.len());
        for row in &rows {
            if row.len() != bound_lefts.len() {
                // SQLite counts the terms of the element that does not fit and puts
                // an `s` on `term` unless there is exactly one.
                let plural = if row.len() == 1 { "" } else { "s" };
                let _ = span;
                return Err(ParseError::new(
                    ParseErrorKind::Refused(format!(
                        "IN(...) element has {} term{plural} - expected {}",
                        row.len(),
                        bound_lefts.len()
                    )),
                    Span::default(),
                ));
            }
            let mut bound_rights = Vec::with_capacity(row.len());
            for value in row {
                bound_rights.push(self.bind_expr(*value)?);
            }
            bound_rows.push(bound_rights);
        }
        // One row is an ordinary row equality. Two or more are a query in
        // SQLite, and the query's rules apply: see `row_in_rules`.
        let rules = match bound_rows.len() {
            0 | 1 => None,
            _ => Some(row_in_rules(
                &bound_lefts,
                &bound_rows,
                matches!(rhs, InRhs::Select(_)),
            )),
        };
        let mut chain: Option<BoundExpr> = None;
        for bound_rights in &bound_rows {
            let one = match &rules {
                Some(rules) => equality_chain_under(&bound_lefts, bound_rights, rules),
                None => equality_chain(&bound_lefts, bound_rights),
            };
            chain = Some(match chain {
                None => one,
                Some(held) => BoundExpr::Or(Box::new(held), Box::new(one)),
            });
        } // An empty list is false, and `NOT IN ()` is true, whatever the
          // operand - including a NULL one. That is SQLite's rule and it is the
          // one place `IN` is not three-valued.
        let bound = chain.unwrap_or(BoundExpr::Integer(0));
        Ok(if negated {
            BoundExpr::Not(Box::new(bound))
        } else {
            bound
        })
    }

    /// Returns the rows of a written `VALUES` list, when the right-hand side is
    /// one.
    ///
    /// @param rhs - what was written after `IN`
    fn row_value_list(&self, rhs: &InRhs) -> Option<Vec<Vec<ExprId>>> {
        match rhs {
            InRhs::Select(select) => {
                let held = self.ast.select(*select)?;
                if !held.compounds.is_empty() || !held.with.ctes.is_empty() {
                    return None;
                }
                let core = self.ast.core(held.first)?;
                match &core.body {
                    SelectBody::Values(rows) => Some(rows.clone()),
                    _ => None,
                }
            }
            // `IN ((1,2), (3,4))` is a list of row values rather than a
            // `VALUES` clause, and means the same thing.
            InRhs::List(items) => {
                let mut rows = Vec::with_capacity(items.len());
                for item in items {
                    rows.push(self.row_value_parts(*item)?);
                }
                Some(rows)
            }
            InRhs::Table { .. } => None,
        }
    }

    /// Returns the parts of a row value, or `None` when the expression is not
    /// one.
    ///
    /// @param id - the expression
    pub(super) fn row_value_parts(&self, id: ExprId) -> Option<Vec<ExprId>> {
        match self.ast.expr(id)? {
            Expr::RowValue(parts) => Some(parts.clone()),
            _ => None,
        }
    }

    /// Binds a comparison between a row value and a one-row query.
    ///
    /// **The query is bound once and read column by column.** Each part becomes
    /// its own scalar subquery over the same block with the other result
    /// columns trimmed away - which is what makes `(a, b) = (SELECT x, y ...)`
    /// mean `a = x AND b = y` over *one* row rather than two independent
    /// lookups: the block is the same block, so it plans and folds once, and
    /// `crate::subquery` answers an uncorrelated one exactly once per
    /// execution.
    ///
    /// The comparison is then the ordinary lexicographic desugaring the
    /// row-against-a-row form already uses, so `<` and `<=` mean here what they
    /// mean there.
    ///
    /// @param op - the operator
    /// @param lefts - the left row's parts
    /// @param select - the query on the right
    /// @param span - where the comparison was written
    pub(super) fn bind_row_against_query(
        &mut self,
        op: BinaryOp,
        lefts: &[ExprId],
        select: ast::SelectId,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let block = self.bind_value_subquery(select, span)?;
        if block.columns.len() != lefts.len() {
            return Err(misused(span));
        }
        let mut bound_lefts = Vec::with_capacity(lefts.len());
        for part in lefts {
            bound_lefts.push(self.bind_expr(*part)?);
        }
        let bound_rights = self.query_columns(block);
        compare_bound_rows(op, &bound_lefts, &bound_rights, span)
    }

    /// Binds a one-row query as one scalar subquery per result column.
    ///
    /// The same block for every column, so each part reads the same row.
    /// Shared by the row comparison above and by `UPDATE ... SET (a, b) =
    /// (SELECT x, y ...)`, which assigns the parts.
    ///
    /// @param select - the query
    /// @param span - where it was written
    pub(crate) fn bind_query_columns(
        &mut self,
        select: ast::SelectId,
        span: Span,
    ) -> Result<Vec<BoundExpr>, ParseError> {
        let block = self.bind_value_subquery(select, span)?;
        Ok(self.query_columns(block))
    }

    /// Splits a bound query into one scalar subquery per result column.
    ///
    /// @param block - the bound query
    fn query_columns(&mut self, block: crate::bind::BoundSelect) -> Vec<BoundExpr> {
        let mut bound_rights = Vec::with_capacity(block.columns.len());
        for at in 0..block.columns.len() {
            let mut one = block.clone();
            one.columns = block
                .columns
                .get(at..at.saturating_add(1))
                .map_or_else(Vec::new, <[crate::bind::BoundResultColumn]>::to_vec);
            let collation = one
                .columns
                .first()
                .map(|column| result_collation(&column.expr))
                .unwrap_or(Collation::Binary);
            bound_rights.push(BoundExpr::Subquery {
                id: self.next_subquery_id(),
                kind: SubqueryKind::Scalar,
                negated: false,
                operand: None,
                block: Box::new(one),
                affinity: None,
                collation,
            });
        }
        bound_rights
    }

    /// Binds a comparison between two row values.
    ///
    /// @param op - the operator
    /// @param lefts - the left row's parts
    /// @param rights - the right row's parts
    /// @param span - where the comparison was written
    pub(super) fn bind_row_comparison(
        &mut self,
        op: BinaryOp,
        lefts: &[ExprId],
        rights: &[ExprId],
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        if lefts.len() != rights.len() || lefts.is_empty() {
            return Err(misused(span));
        }
        let mut bound_lefts = Vec::with_capacity(lefts.len());
        let mut bound_rights = Vec::with_capacity(rights.len());
        for (left, right) in lefts.iter().zip(rights.iter()) {
            bound_lefts.push(self.bind_expr(*left)?);
            bound_rights.push(self.bind_expr(*right)?);
        }
        match op {
            BinaryOp::Equal => Ok(equality_chain(&bound_lefts, &bound_rights)),
            // `<>` is the negation of `=` rather than an inequality of its own,
            // which is what makes `(1, NULL) <> (1, 2)` unknown rather than
            // true: the equality is unknown, and NOT of unknown is unknown.
            BinaryOp::NotEqual => Ok(BoundExpr::Not(Box::new(equality_chain(
                &bound_lefts,
                &bound_rights,
            )))),
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                Ok(lexicographic_chain(op, &bound_lefts, &bound_rights, 0))
            }
            _ => Err(misused(span)),
        }
    }
}

impl Binder<'_> {
    /// Returns the query when an expression is a subquery of several columns.
    ///
    /// **SQLite reads `(SELECT 1, 2) = (1, 2)` as a row value.** The subquery is
    /// the row, one part per result column. The width comes from the text of the
    /// query, so a `SELECT *` is not recognised here and keeps being refused as
    /// a subquery that returns more than one column.
    ///
    /// @param id - the expression
    pub(super) fn row_query(&self, id: ExprId) -> Option<ast::SelectId> {
        let Some(Expr::Subquery(select)) = self.ast.expr(id) else {
            return None;
        };
        let width = self.query_width(*select)?;
        (width > 1).then_some(*select)
    }

    /// Returns how many result columns the first arm of a query writes, or
    /// `None` when a `*` makes that depend on the tables.
    ///
    /// @param select - the query
    fn query_width(&self, select: ast::SelectId) -> Option<usize> {
        let held = self.ast.select(select)?;
        match &self.ast.core(held.first)?.body {
            SelectBody::Values(rows) => rows.first().map(Vec::len),
            SelectBody::Select { columns, .. } => {
                let star = columns
                    .iter()
                    .any(|column| matches!(self.ast.expr(column.expr), Some(Expr::Star { .. })));
                (!star).then_some(columns.len())
            }
        }
    }

    /// Binds a comparison whose left operand is a subquery of several columns.
    ///
    /// @param op - the operator
    /// @param select - the subquery on the left
    /// @param right - the expression on the right
    /// @param span - where the comparison was written
    pub(super) fn bind_row_query_versus(
        &mut self,
        op: BinaryOp,
        select: ast::SelectId,
        right: ExprId,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let lefts = self.bind_query_columns(select, span)?;
        let rights = self.bind_row_operand(right, span)?;
        if lefts.len() != rights.len() {
            return Err(misused(span));
        }
        compare_bound_rows(op, &lefts, &rights, span)
    }

    /// Binds `<subquery of several columns> IS [NOT] <row>`.
    ///
    /// @param negated - whether the test is `IS NOT`
    /// @param select - the subquery on the left
    /// @param right - the expression on the right
    /// @param span - where the test was written
    pub(super) fn bind_row_query_is(
        &mut self,
        negated: bool,
        select: ast::SelectId,
        right: ExprId,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let lefts = self.bind_query_columns(select, span)?;
        let rights = self.bind_row_operand(right, span)?;
        if lefts.len() != rights.len() {
            return Err(misused(span));
        }
        Ok(is_chain(negated, &lefts, &rights))
    }

    /// Binds the parts of an expression that has to be a row: a row value or a
    /// subquery of several columns.
    ///
    /// @param id - the expression
    /// @param span - where the comparison was written
    fn bind_row_operand(&mut self, id: ExprId, span: Span) -> Result<Vec<BoundExpr>, ParseError> {
        if let Some(parts) = self.row_value_parts(id) {
            let mut bound = Vec::with_capacity(parts.len());
            for part in &parts {
                bound.push(self.bind_expr(*part)?);
            }
            return Ok(bound);
        }
        match self.row_query(id) {
            Some(select) => self.bind_query_columns(select, span),
            None => Err(misused(span)),
        }
    }

    /// Binds a row value compared with whatever is on the other side: another
    /// row value or a one-row query.
    ///
    /// @param op - the operator
    /// @param lefts - the left row's parts
    /// @param right - the expression on the right
    /// @param span - where the comparison was written
    fn bind_row_versus(
        &mut self,
        op: BinaryOp,
        lefts: &[ExprId],
        right: ExprId,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        if let Some(rights) = self.row_value_parts(right) {
            return self.bind_row_comparison(op, lefts, &rights, span);
        }
        if let Some(Expr::Subquery(select)) = self.ast.expr(right) {
            let select = *select;
            return self.bind_row_against_query(op, lefts, select, span);
        }
        Err(misused(span))
    }

    /// Binds `(a, b) IS (c, d)` and `(a, b) IS NOT (c, d)`.
    ///
    /// `IS` over rows is `IS` over each pair, joined by `AND`; SQLite answers
    /// it, and it was refused as `unsupported: row values`. `IS` never answers
    /// NULL, so the negation is the negation of the chain.
    ///
    /// @param negated - whether the test is `IS NOT` (or `IS DISTINCT FROM`)
    /// @param lefts - the left row's parts
    /// @param rights - the right row's parts
    /// @param span - where the test was written
    pub(super) fn bind_row_is(
        &mut self,
        negated: bool,
        lefts: &[ExprId],
        rights: &[ExprId],
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        if lefts.len() != rights.len() || lefts.is_empty() {
            return Err(misused(span));
        }
        let mut bound_lefts = Vec::with_capacity(lefts.len());
        let mut bound_rights = Vec::with_capacity(rights.len());
        for (left, right) in lefts.iter().zip(rights.iter()) {
            bound_lefts.push(self.bind_expr(*left)?);
            bound_rights.push(self.bind_expr(*right)?);
        }
        Ok(is_chain(negated, &bound_lefts, &bound_rights))
    }

    /// Binds `(a, b) [NOT] BETWEEN (c, d) AND (e, f)`.
    ///
    /// SQLite's meaning, `row >= low AND row <= high` with the lexicographic
    /// comparisons a row value already has. It was refused as
    /// `unsupported: row values`.
    ///
    /// @param negated - whether `NOT` was written
    /// @param parts - the tested row's parts
    /// @param low - the lower bound
    /// @param high - the upper bound
    /// @param span - where the test was written
    pub(super) fn bind_row_between(
        &mut self,
        negated: bool,
        parts: &[ExprId],
        low: ExprId,
        high: ExprId,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let above = self.bind_row_versus(BinaryOp::GreaterEqual, parts, low, span)?;
        let below = self.bind_row_versus(BinaryOp::LessEqual, parts, high, span)?;
        let both = BoundExpr::And(Box::new(above), Box::new(below));
        Ok(match negated {
            true => BoundExpr::Not(Box::new(both)),
            false => both,
        })
    }

    /// Binds `CASE (a, b) WHEN (c, d) THEN ... END` as the searched `CASE` it
    /// means, one row equality per `WHEN`.
    ///
    /// @param parts - the operand row's parts
    /// @param branches - the `WHEN` and `THEN` expressions
    /// @param otherwise - the `ELSE`, when written
    /// @param span - where the `CASE` was written
    pub(super) fn bind_row_case(
        &mut self,
        parts: &[ExprId],
        branches: &[(ExprId, ExprId)],
        otherwise: Option<ExprId>,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let mut bound_branches = Vec::with_capacity(branches.len());
        for (when, then) in branches {
            let test = self.bind_row_versus(BinaryOp::Equal, parts, *when, span)?;
            bound_branches.push((test, self.bind_expr(*then)?));
        }
        let otherwise = match otherwise {
            Some(expr) => Some(Box::new(self.bind_expr(expr)?)),
            None => None,
        };
        Ok(BoundExpr::Case {
            operand: None,
            branches: bound_branches,
            otherwise,
            comparisons: Vec::new(),
        })
    }
}

/// Returns the `AND` chain of `IS` tests that a row value `IS` another means.
///
/// @param negated - whether the test is `IS NOT`
/// @param lefts - the left row's parts, bound
/// @param rights - the right row's parts, bound
fn is_chain(negated: bool, lefts: &[BoundExpr], rights: &[BoundExpr]) -> BoundExpr {
    let mut chain: Option<BoundExpr> = None;
    for (left, right) in lefts.iter().zip(rights.iter()) {
        let (affinity, collation) = comparison_rules(left, right);
        let one = BoundExpr::Is {
            negated: false,
            left: Box::new(left.clone()),
            right: Box::new(right.clone()),
            affinity,
            collation,
        };
        chain = Some(match chain {
            None => one,
            Some(held) => BoundExpr::And(Box::new(held), Box::new(one)),
        });
    }
    let chain = chain.unwrap_or(BoundExpr::Null);
    match negated {
        true => BoundExpr::Not(Box::new(chain)),
        false => chain,
    }
}

/// Returns SQLite's refusal of a query whose width is not the row value's.
///
/// @param found - how many columns the query returns
/// @param expected - how many parts the row value has
/// @param span - where the test was written
pub(super) fn query_arity_refusal(found: usize, expected: usize, span: Span) -> ParseError {
    refused(
        format!("sub-select returns {found} columns - expected {expected}"),
        span,
    )
}

/// Returns SQLite's refusal of a row value where a single value belongs.
///
/// @param span - where the row value was written
pub(super) fn misused(span: Span) -> ParseError {
    // SQLite prints no caret for this failure, so the position is dropped.
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused("row value misused".to_string()),
        Span::default(),
    )
}

/// Returns the `AND` chain that a row-value equality means.
///
/// @param lefts - the left row's parts, bound
/// @param rights - the right row's parts, bound
pub(super) fn equality_chain(lefts: &[BoundExpr], rights: &[BoundExpr]) -> BoundExpr {
    let rules: Vec<_> = lefts
        .iter()
        .zip(rights.iter())
        .map(|(left, right)| comparison_rules(left, right))
        .collect();
    equality_chain_under(lefts, rights, &rules)
}

/// Returns the rules a row value `IN` over several rows compares each part with.
///
/// Measured against 3.53.4: SQLite turns `(a, b) IN ((1, 2), (3, 4))` into a
/// query over the rows and reads the affinity and the collation of each part
/// off ONE row of it, not off the row being compared. For a written list that
/// row is the last one. For a `VALUES` clause the affinity also comes from the
/// last row, but an explicit `COLLATE` comes from the first row (an implicit
/// column collation still from the last), so `('a') IN (VALUES('A' COLLATE
/// NOCASE), ('x'))` is true and the same with the `COLLATE` on `'x'` is false.
///
/// @param lefts - the left row's parts, bound
/// @param rows - every right row, bound, at least one
/// @param values_clause - whether the rows were written as a `VALUES` clause
pub(super) fn row_in_rules(
    lefts: &[BoundExpr],
    rows: &[Vec<BoundExpr>],
    values_clause: bool,
) -> Vec<(Option<inillucent_value::Affinity>, Collation)> {
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else {
        return Vec::new();
    };
    let mut rules = Vec::with_capacity(lefts.len());
    for (at, left) in lefts.iter().enumerate() {
        let (Some(first), Some(last)) = (first.get(at), last.get(at)) else {
            rules.push((None, Collation::Binary));
            continue;
        };
        let (mut affinity, mut collation) = comparison_rules(left, last);
        if values_clause {
            // The implicit collation of the last row: an expression with an
            // explicit `COLLATE` anywhere in it is not asked for one.
            let implicit = match last.explicit_collation() {
                Some(_) => None,
                None => last.collation(),
            };
            collation = left
                .explicit_collation()
                .or_else(|| first.explicit_collation())
                .or_else(|| left.collation())
                .or(implicit)
                .unwrap_or(Collation::Binary);
            // Measured: a `CAST` in a `VALUES` row gives the part no affinity,
            // although it does in a written list and in a compound query.
            if matches!(last, BoundExpr::Cast { .. }) {
                affinity = left.affinity();
            }
        }
        rules.push((affinity, collation));
    }
    rules
}

/// Returns the `AND` chain of part equalities, each compared under given rules.
///
/// @param lefts - the left row's parts, bound
/// @param rights - the right row's parts, bound
/// @param rules - the affinity and collation of each part, in part order
pub(super) fn equality_chain_under(
    lefts: &[BoundExpr],
    rights: &[BoundExpr],
    rules: &[(Option<inillucent_value::Affinity>, Collation)],
) -> BoundExpr {
    let mut chain: Option<BoundExpr> = None;
    for ((left, right), (affinity, collation)) in lefts.iter().zip(rights.iter()).zip(rules.iter())
    {
        let (affinity, collation) = (*affinity, *collation);
        let one = BoundExpr::Compare {
            op: BinaryOp::Equal,
            left: Box::new(left.clone()),
            right: Box::new(right.clone()),
            affinity,
            collation,
        };
        chain = Some(match chain {
            None => one,
            Some(held) => BoundExpr::And(Box::new(held), Box::new(one)),
        });
    }
    chain.unwrap_or(BoundExpr::Null)
}

/// Returns the chain a lexicographic row-value comparison means.
///
/// `(a, b, c) < (x, y, z)` is `a < x OR (a = x AND (b < y OR (b = y AND c <
/// z)))`, and the strictness only ever applies to the last part: everything
/// before it is compared for equality to decide whether the next part matters.
///
/// @param op - the operator
/// @param lefts - the left row's parts, bound
/// @param rights - the right row's parts, bound
/// @param at - which part this level compares
fn lexicographic_chain(
    op: BinaryOp,
    lefts: &[BoundExpr],
    rights: &[BoundExpr],
    at: usize,
) -> BoundExpr {
    let (Some(left), Some(right)) = (lefts.get(at), rights.get(at)) else {
        return BoundExpr::Null;
    };
    let (affinity, collation) = comparison_rules(left, right);
    let last = at.saturating_add(1) >= lefts.len();
    // The last part carries the operator as written, including its
    // or-equal half; every earlier part is compared strictly, with the
    // equal case handled by the branch beside it.
    let strict = match op {
        BinaryOp::LessEqual if !last => BinaryOp::Less,
        BinaryOp::GreaterEqual if !last => BinaryOp::Greater,
        other => other,
    };
    let decided = BoundExpr::Compare {
        op: strict,
        left: Box::new(left.clone()),
        right: Box::new(right.clone()),
        affinity,
        collation,
    };
    if last {
        return decided;
    }
    let same = BoundExpr::Compare {
        op: BinaryOp::Equal,
        left: Box::new(left.clone()),
        right: Box::new(right.clone()),
        affinity,
        collation,
    };
    BoundExpr::Or(
        Box::new(decided),
        Box::new(BoundExpr::And(
            Box::new(same),
            Box::new(lexicographic_chain(op, lefts, rights, at.saturating_add(1))),
        )),
    )
}

/// Returns the comparison a row value against a row value means.
///
/// **One desugaring, shared by both spellings.** `=` is a chain of equalities,
/// `<>` is the negation of that chain rather than an inequality of its own -
/// which is what makes `(1, NULL) <> (1, 2)` unknown - and the ordering
/// operators are lexicographic. The row-against-a-query form binds different
/// operands and then means exactly this.
///
/// @param op - the operator
/// @param lefts - the left row, already bound
/// @param rights - the right row, already bound
/// @param span - where the comparison was written
fn compare_bound_rows(
    op: BinaryOp,
    lefts: &[BoundExpr],
    rights: &[BoundExpr],
    span: Span,
) -> Result<BoundExpr, ParseError> {
    match op {
        BinaryOp::Equal => Ok(equality_chain(lefts, rights)),
        BinaryOp::NotEqual => Ok(BoundExpr::Not(Box::new(equality_chain(lefts, rights)))),
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
            Ok(lexicographic_chain(op, lefts, rights, 0))
        }
        _ => Err(misused(span)),
    }
}

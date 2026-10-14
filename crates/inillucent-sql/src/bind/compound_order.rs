//! A compound query's `ORDER BY`, which may only name a result column.
//!
//! Invariant: **a compound's `ORDER BY` is resolved against the compound's
//! output, never against an arm's `FROM` clause**, because the arms do not share
//! one. A term is an ordinal or the name some arm gives a result column.

use inillucent_value::Collation;

use super::{
    compound_collation, compound_order_unmatched, no_such_collation, order_out_of_range, Binder,
    BoundExpr, BoundOrderTerm, BoundResultColumn, BoundSelect,
};
use crate::ast::{self, Expr, ExprId, NullOrder, SortOrder};
use crate::diagnostic::ParseError;
use crate::lexer::Span;

impl Binder<'_> {
    /// Binds a compound's `ORDER BY`, which may only name a result column.
    ///
    /// SQLite resolves a compound's `ORDER BY` against the output of the
    /// compound rather than against any arm's FROM clause, because the arms do
    /// not share one. A term that is neither an ordinal nor the name of a
    /// result column is an error there and is an error here.
    ///
    /// **A name may be one an arm gives its column, not only the first
    /// arm's.** `SELECT a FROM t UNION SELECT x FROM u ORDER BY x` sorts the
    /// first column, because the second arm calls it `x`; the arms are tried in
    /// written order after the first.
    ///
    /// @param terms - the `ORDER BY` terms
    /// @param columns - the first arm's result columns, which name the compound's
    /// @param arms - the other arms, whose column names also count
    pub(super) fn bind_compound_order_by(
        &mut self,
        terms: &[ast::OrderTerm],
        columns: &[BoundResultColumn],
        arms: &[(ast::CompoundOp, BoundSelect)],
    ) -> Result<Vec<BoundOrderTerm>, ParseError> {
        let mut bound = Vec::with_capacity(terms.len());
        for (at, term) in terms.iter().enumerate() {
            let position = at.saturating_add(1);
            let span = self.ast.expr_span(term.expr);
            let (target, named) = self.order_term_collation(term.expr, span)?;
            let index = match self.as_ordinal(target) {
                Some(ordinal) => match ordinal.checked_sub(1) {
                    Some(index) if index < columns.len() => index,
                    _ => return Err(order_out_of_range(position, columns.len(), span)),
                },
                None => {
                    let by_name = match self.ast.expr(target) {
                        Some(Expr::Column {
                            database: None,
                            table: None,
                            column,
                        }) => {
                            let folded = self.ast.folded(*column).to_vec();
                            let named_in = |result: &[BoundResultColumn]| {
                                result.iter().position(|candidate| {
                                    candidate.name.eq_ignore_ascii_case(&folded)
                                })
                            };
                            named_in(columns)
                                .or_else(|| arms.iter().find_map(|(_, arm)| named_in(&arm.columns)))
                        }
                        _ => None,
                    };
                    // A term that names no column is still accepted when it is the same
                    // expression some arm writes as a result column (bug hunt 10). A term
                    // with its own `COLLATE` is not: SQLite compares the expression with
                    // the collation still on it, and finds no match.
                    let matched = match (by_name, named) {
                        (Some(index), _) => Some(index),
                        (None, None) => self.column_written_as(target, columns, arms),
                        (None, Some(_)) => None,
                    };
                    let Some(index) = matched else {
                        return Err(compound_order_unmatched(position, span));
                    };
                    index
                }
            };
            let Some(column) = columns.get(index) else {
                return Err(order_out_of_range(position, columns.len(), span));
            };
            // With no `COLLATE` on the term, the result column's own collation
            // governs, read the same way the compound's duplicate removal reads
            // it - an explicit `COLLATE` on the result column beats the implicit
            // one - so the sort and the duplicate removal cannot disagree about
            // a column.
            let collation = named.unwrap_or_else(|| {
                let later = arms
                    .iter()
                    .filter_map(|(_, arm)| arm.columns.get(index))
                    .map(|later| &later.expr);
                compound_collation(std::iter::once(&column.expr).chain(later))
            });
            let nulls = term.nulls.unwrap_or(match term.order {
                SortOrder::Ascending => NullOrder::First,
                SortOrder::Descending => NullOrder::Last,
            });
            bound.push(BoundOrderTerm {
                expr: BoundExpr::SorterColumn {
                    column: index as u16,
                },
                order: term.order,
                nulls,
                collation,
            });
        }
        Ok(bound)
    }

    /// Finds the result column an `ORDER BY` term repeats, by binding the term
    /// in each arm's own FROM clause and comparing it with that arm's result
    /// columns.
    ///
    /// **Why each arm and not the first only.** SQLite resolves the term in every
    /// arm in turn, so `SELECT a FROM t UNION SELECT a FROM u ORDER BY t.a` and
    /// `... ORDER BY u.a` both sort the one result column. `ORDER BY a+1` over
    /// `SELECT a+1` and `ORDER BY upper(b)` over `SELECT upper(b)` match in the
    /// same way. A term that binds in no arm, or binds to something no arm
    /// selects, is not an error here: the caller reports it as unmatched.
    ///
    /// @param term - the `ORDER BY` term as written, without a `COLLATE`
    /// @param columns - the first arm's result columns
    /// @param arms - the other arms
    fn column_written_as(
        &mut self,
        term: ExprId,
        columns: &[BoundResultColumn],
        arms: &[(ast::CompoundOp, BoundSelect)],
    ) -> Option<usize> {
        let first_scope = self.scope().to_vec();
        let mut candidates: Vec<(Vec<usize>, &[BoundResultColumn])> = vec![(first_scope, columns)];
        for (_, arm) in arms {
            let ids = arm.sources.iter().map(|source| source.id).collect();
            candidates.push((ids, arm.columns.as_slice()));
        }
        for (ids, result) in candidates {
            // Binding the term must leave nothing behind in the statement being bound.
            let kept = (
                self.aggregates.len(),
                self.windows.len(),
                self.correlations.len(),
            );
            self.scopes.push(ids);
            let bound = self.bind_expr(term);
            self.scopes.pop();
            self.aggregates.truncate(kept.0);
            self.windows.truncate(kept.1);
            self.correlations.truncate(kept.2);
            let Ok(bound) = bound else {
                continue;
            };
            if let Some(index) = result.iter().position(|column| column.expr == bound) {
                return Some(index);
            }
        }
        None
    }

    /// Splits a compound `ORDER BY` term into the term itself and the
    /// collation an explicit `COLLATE` named on it.
    ///
    /// **`UNION ... ORDER BY a COLLATE NOCASE` was a parse error (task-1979,
    /// F15).** A compound's `ORDER BY` may only name a result column, and the
    /// match was made against the term exactly as written, so `a COLLATE
    /// NOCASE` was an `Expr::Collate` rather than an `Expr::Column` and the
    /// term matched nothing. SQLite reads through the `COLLATE`, matches the
    /// name underneath it, and sorts that column with the collation the term
    /// named rather than the one the column carries.
    ///
    /// @param expr - the term as written
    /// @param span - where to point a `no such collation` diagnostic
    pub(super) fn order_term_collation(
        &self,
        expr: ExprId,
        span: Span,
    ) -> Result<(ExprId, Option<Collation>), ParseError> {
        let Some(Expr::Collate { operand, collation }) = self.ast.expr(expr) else {
            return Ok((expr, None));
        };
        let name = self.ast.text(*collation);
        let Some(named) = self.collation_named(name) else {
            return Err(no_such_collation(name, span));
        };
        Ok((*operand, Some(named)))
    }
}

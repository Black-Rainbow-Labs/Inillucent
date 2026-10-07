//! `ORDER BY` terms that a `WHERE` equality has made constant.
//!
//! Invariant: **an `ORDER BY` column that `WHERE` equates to a value computed
//! outside the query is not sorted by.** SQLite does this without checking the
//! affinities of the two sides, so rows that compare equal under numeric
//! affinity but differ as text (`'1'` and `' 1'`) come out in scan order.

use super::*;

/// Returns whether a plan over one FROM term needs no sort because every
/// `ORDER BY` term is made constant by `WHERE`; see [`every_term_is_constant`].
///
/// @param select - the bound statement
/// @param sources - the planned FROM terms
/// @param aggregation - how the rows are aggregated, which must be not at all
pub(super) fn plan_skips_sort(
    select: &BoundSelect,
    sources: &[PlannedSource],
    aggregation: AggregationMode,
) -> bool {
    aggregation == AggregationMode::None
        && sources
            .first()
            .is_some_and(|outer| every_term_is_constant(select, outer.id))
}

/// Returns whether every `ORDER BY` term of a query over one FROM term is a
/// column that `WHERE` equates to a value computed outside the query.
///
/// The executor decides for itself whether a scan already gives the order, and
/// it knows only the order of the scan, so the plan carries this answer for it.
///
/// @param select - the bound statement
/// @param id - the only FROM term
pub(super) fn every_term_is_constant(select: &BoundSelect, id: usize) -> bool {
    !select.order_by.is_empty()
        && select.order_by.iter().all(|term| {
            let mut expr = &term.expr;
            while let BoundExpr::Collate { operand, .. } = expr {
                expr = operand;
            }
            equated_to_outside_value(select, id, expr, term.collation)
        })
}

/// Returns whether `WHERE` equates a column of a lone FROM term to a value that
/// does not depend on the query's own rows.
///
/// SQLite skips such a column when it works out whether a scan already gives
/// the `ORDER BY` (`wherePathSatisfiesOrderBy`), provided the comparison and the
/// `ORDER BY` use the same collation. The affinity of the comparison is not
/// asked, so `SELECT (SELECT t FROM t1 WHERE i = t ORDER BY t)` with an
/// integer outer `i` and text `t` returns the first row in scan order. Sorting
/// those rows returned a different one.
///
/// @param select - the bound statement, for its `WHERE`
/// @param id - the FROM term the `ORDER BY` column belongs to
/// @param expr - the `ORDER BY` expression with any `COLLATE` removed
/// @param collation - the collation the `ORDER BY` term sorts under
pub(super) fn equated_to_outside_value(
    select: &BoundSelect,
    id: usize,
    expr: &BoundExpr,
    collation: Collation,
) -> bool {
    let BoundExpr::Column { source, column, .. } = expr else {
        return false;
    };
    if *source != id {
        return false;
    }
    let Some(filter) = &select.filter else {
        return false;
    };
    conjunction(filter).iter().any(|term| {
        let BoundExpr::Compare {
            op: BinaryOp::Equal,
            left,
            right,
            collation: used,
            ..
        } = term
        else {
            return false;
        };
        if *used != collation {
            return false;
        }
        let other = match (left.as_ref(), right.as_ref()) {
            (
                BoundExpr::Column {
                    source,
                    column: held,
                    ..
                },
                other,
            )
            | (
                other,
                BoundExpr::Column {
                    source,
                    column: held,
                    ..
                },
            ) if *source == id && held == column => other,
            _ => return false,
        };
        let mut used = Vec::new();
        other.sources_used(&mut used);
        !used.contains(&id)
    })
}

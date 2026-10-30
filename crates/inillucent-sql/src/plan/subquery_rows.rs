//! Sizing a FROM term for the join order search.
//!
//! Invariant: **a subquery is sized only when its text says how many rows it
//! has.** A `SELECT` with no `FROM` clause, an aggregate with no `GROUP BY`, a
//! `UNION` of such arms and a constant `LIMIT` all say so. Anything else keeps
//! the default size of a table nobody has analysed, so the written order stands
//! for it, as it does in SQLite.
//!
//! Here rather than in [`super`] because `plan.rs` is at its recorded size.

use super::*;

/// Returns whether a block, its residual filters or any compound arm holds a subquery used as a
/// value.
///
/// @param select - the bound block
/// @param residuals - the predicates the loops still evaluate, one per nesting level
/// @param constant_filter - the predicate over no columns
/// @param compounds - the planned later arms of a compound
pub(super) fn plan_holds_subquery(
    select: &BoundSelect,
    residuals: &[Option<BoundExpr>],
    constant_filter: &Option<BoundExpr>,
    compounds: &[(CompoundOp, PhysicalPlan)],
) -> bool {
    holds_subquery(select)
        || residuals.iter().flatten().any(expression_holds_subquery)
        || constant_filter
            .as_ref()
            .is_some_and(expression_holds_subquery)
        || compounds.iter().any(|(_op, arm)| arm.subqueries)
}

/// Returns whether the rows of a FROM term hold a subquery used as a value.
///
/// A derived table holds one when its query does. A recursive CTE holds one when its own `LIMIT`
/// or `OFFSET` or any of its arms does: `LIMIT (SELECT 3)` on a recursive CTE was refused as "a
/// correlated subquery used as a value", because the flag `subquery::fold` asks first was clear
/// and the slot the limit reads was never filled.
///
/// @param rows - where the FROM term's rows come from
pub(super) fn rows_hold_subquery(rows: &SourceRows) -> bool {
    match rows {
        SourceRows::Subquery(block) => holds_subquery(block),
        SourceRows::Recursive(body) => {
            body.limit.iter().any(expression_holds_subquery)
                || body.offset.iter().any(expression_holds_subquery)
                || body
                    .seeds
                    .iter()
                    .chain(body.steps.iter())
                    .any(|(_op, arm)| holds_subquery(arm))
        }
        _ => false,
    }
}

/// Returns what one FROM term's path costs in the join order search, and how
/// many rows it produces.
///
/// **A derived table is sized by what it selects.** `FROM t, (SELECT 1 k UNION ALL
/// SELECT 2) j` priced `j` at the default row count of an unmeasured table, so
/// both orders cost the same and the written order stood: `t` was the outer
/// loop. SQLite sizes the subquery (two rows here) and makes it the outer loop,
/// so it scans `t` twice rather than the subquery a million times, and the two
/// engines returned the same rows in a different order. The estimate follows
/// SQLite's: a `SELECT` with no `FROM` clause or with an aggregate and no
/// `GROUP BY` is one row, a `UNION` or `UNION ALL` adds its arms, and a constant
/// `LIMIT` caps it. Anything else keeps the default, so a subquery over a table
/// is still ordered by the written order.
///
/// **Not beside a recursive term.** A recursive CTE that is not the outer loop
/// is filled completely before the loop over it starts, and SQLite streams it.
/// `WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) SELECT n FROM
/// r, (SELECT 1) WHERE n > 3 LIMIT 2` ends because the `LIMIT` stops the
/// recursion, which only works while `r` stays first. Sizing the subquery made
/// it the outer loop and the recursion never ended.
///
/// @param select - the block the term belongs to
/// @param source - the FROM term
/// @param path - how the term is read
pub(super) fn ordered_path_cost(
    select: &BoundSelect,
    source: &BoundSource,
    path: &AccessPath,
) -> (f64, f64) {
    let sized = match (&source.rows, path) {
        (SourceRows::Subquery(inner), AccessPath::Subquery { .. })
            if !has_recursive_term(select) =>
        {
            Some(subquery_rows(inner))
        }
        _ => None,
    };
    match sized {
        Some(rows) => (cost::scan_cost(rows), rows),
        None => path_cost(source, path),
    }
}

/// Reports whether any FROM term of a block is a recursive CTE or reads one.
///
/// @param select - the block
fn has_recursive_term(select: &BoundSelect) -> bool {
    select.sources.iter().any(|source| {
        matches!(
            source.rows,
            SourceRows::Recursive(_) | SourceRows::RecursiveSelf { .. }
        )
    })
}

/// Returns how many rows a subquery in a FROM clause is estimated to produce.
///
/// @param select - the subquery, with the arms of its compound
fn subquery_rows(select: &BoundSelect) -> f64 {
    let mut rows = arm_rows(select);
    for (op, arm) in &select.compounds {
        let arm = arm_rows(arm);
        rows = match op {
            CompoundOp::Union | CompoundOp::UnionAll => rows + arm,
            CompoundOp::Intersect => rows.min(arm),
            CompoundOp::Except => rows,
        };
    }
    match select.limit.as_ref() {
        Some(BoundExpr::Integer(limit)) if *limit >= 0 => rows.min((*limit as f64).max(1.0)),
        _ => rows,
    }
}

/// Returns how many rows one arm of a subquery is estimated to produce, before
/// its compound or `LIMIT` applies.
///
/// @param select - the arm
fn arm_rows(select: &BoundSelect) -> f64 {
    let one_row =
        select.sources.is_empty() || (select.group_by.is_empty() && !select.aggregates.is_empty());
    match one_row {
        true => 1.0,
        false => cost::DEFAULT_ROWS,
    }
}

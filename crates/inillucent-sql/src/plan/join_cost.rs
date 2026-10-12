//! What a join order costs once its filters, its `ORDER BY` and its `LIMIT` are counted.
//!
//! Invariant: **these functions only change which join order the planner prefers.** The rows a
//! plan returns do not depend on them, so a wrong estimate here costs time and never an answer.

use super::{
    cost, is_outer, limit_walk, AggregationMode, BinaryOp, BoundExpr, BoundSelect, BoundSource,
    JoinKind, Levers,
};

/// Adjusts a join order's cost for the `ORDER BY` and the `LIMIT` of the statement.
///
/// An order whose outermost term walks in the `ORDER BY`'s order needs no sort, and a constant
/// `LIMIT` stops it after the share of its rows that produces that many output rows. Every other
/// order pays for sorting the whole join result. Without this a join with `ORDER BY p.created_at
/// DESC LIMIT 20` started from `users`, joined every post and sorted, where SQLite scans the
/// `created_at` index and probes `users` 20 times (244 ms against 9 ms).
///
/// @param select - the bound statement
/// @param levers - which optimizations are on
/// @param total - the order's cost when it produces every row
/// @param rows - how many rows the order is estimated to produce
/// @param first_provides - whether the outermost term's walk is in the `ORDER BY`'s order
pub(super) fn ordered_output_cost(
    select: &BoundSelect,
    levers: Levers,
    total: f64,
    rows: f64,
    first_provides: bool,
) -> f64 {
    let eligible = levers.has(Levers::ORDERED_WALK)
        && !select.order_by.is_empty()
        && outer_order_survives_joins(select)
        && select.windows.is_empty()
        && select.group_by.is_empty()
        && select.aggregates.is_empty()
        && !select.distinct;
    if !eligible {
        return total;
    }
    if !first_provides {
        return total + cost::sort_cost(rows.max(1.0));
    }
    match limit_walk::limit_rows(select) {
        Some(wanted) => total * (wanted.max(1.0) / rows.max(1.0)).min(1.0),
        None => total,
    }
}

/// Returns the share of a term's rows that survive the `WHERE` terms tested at its level.
///
/// A term no index consumed is still tested on every row the loop reads, and rows it rejects do
/// not reach the loops inside. Ignoring that priced `WHERE c.region = 'R3'` over an unindexed
/// column as keeping every customer, so a six table join started from a covering index scan of
/// 100,000 users where SQLite starts from the filtered customers (680 ms against 159 ms). SQLite
/// takes one part in 1.07 off the estimate for each such term, and caps the reduction of an
/// equality at one part in four; the same numbers are used here.
///
/// @param select - the bound statement
/// @param terms - the statement's terms
/// @param consumed - which terms a chosen path already enforces
/// @param ids - the source ids in visiting order
/// @param level - the term's place in the visiting order
/// @param source - the term being priced
pub(super) fn residual_share(
    select: &BoundSelect,
    terms: &[BoundExpr],
    consumed: &[bool],
    ids: &[usize],
    level: usize,
    source: &BoundSource,
) -> f64 {
    if is_outer(source.join) || select.sources.len() < 2 {
        return 1.0;
    }
    let mut share = 1.0f64;
    let mut equality = false;
    for (index, term) in terms.iter().enumerate() {
        if consumed.get(index).copied().unwrap_or(true) {
            continue;
        }
        let mut used = Vec::new();
        term.sources_used(&mut used);
        let last = used
            .iter()
            .filter_map(|id| ids.iter().position(|held| held == id))
            .max();
        if last != Some(level) || !used.contains(&source.id) {
            continue;
        }
        share *= 0.93;
        equality |= matches!(
            term,
            BoundExpr::Compare {
                op: BinaryOp::Equal,
                ..
            }
        );
    }
    if equality {
        share = share.min(0.25);
    }
    share
}

/// Reports whether a join comes out in the order the outermost term's walk delivers.
///
/// True for two or more terms when no `RIGHT` or `FULL` join null extends the outer term, and the
/// statement neither aggregates, windows nor removes duplicates, which all reorder or collapse
/// the rows.
///
/// @param select - the bound statement
/// @param count - how many FROM terms were planned
/// @param levers - which optimizations are on
/// @param aggregation - how the statement aggregates
pub(super) fn joined_in_order(
    select: &BoundSelect,
    count: usize,
    levers: Levers,
    aggregation: AggregationMode,
) -> bool {
    count > 1
        && levers.has(Levers::ORDERED_WALK)
        && outer_order_survives_joins(select)
        && select.windows.is_empty()
        && aggregation == AggregationMode::None
        && !select.distinct
}

/// Reports whether the outermost term's order is the order a join delivers its rows in.
///
/// The loops inside the outer one run once per outer row, so the output follows the outer walk
/// unless a `RIGHT` or `FULL` join adds rows that match no outer row.
///
/// @param select - the bound statement
pub(super) fn outer_order_survives_joins(select: &BoundSelect) -> bool {
    !select
        .sources
        .iter()
        .any(|source| matches!(source.join, JoinKind::Right | JoinKind::Full))
}

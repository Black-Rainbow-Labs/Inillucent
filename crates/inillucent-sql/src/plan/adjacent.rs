//! Whether the outer walk brings equal keys together, so a grouping or a
//! de-duplication can stream.
//!
//! Invariant: **adjacency is claimed only where every row of one key arrives
//! in one run.** A walk ordered by a key brings its rows together; a join
//! that answers one outer row at a time keeps them together; anything that
//! emits rows out of the outer walk's order is not adjacent.

use inillucent_value::Collation;

use super::{path_ordering, walk_key_of, Levers, OrderedBy, PathOrdering, PlannedSource};
use crate::ast::JoinKind;
use crate::bind::BoundSelect;

/// Returns whether the statement's shape lets the outer walk's adjacency reach
/// the grouping.
///
/// Adjacency is a weaker property than order, so it is asked first and for a
/// wider set of statements: a grouped aggregate can be streamed whether or
/// not it also answers an ORDER BY.
///
/// **A join keeps the outer term's rows together (task-2183).** Every inner
/// term is joined one outer row at a time - an index nested loop, a probe
/// of a table built once, a left join's probe and test - so the rows one
/// outer row produces arrive together and in the outer walk's order, and a
/// grouping by the outer term's leading keys is still adjacent. A `RIGHT`
/// or `FULL` term emits its unmatched rows at the end, so neither is
/// allowed. `GROUP BY m.category` over `main_category` joined to
/// `side_table` sent every joined row through the hash aggregate, where
/// SQLite streams them.
///
/// @param select - the bound statement
/// @param sources - the planned FROM terms, outer first
/// @param levers - the optimizations turned off
pub(super) fn walk_keeps_rows_together(
    select: &BoundSelect,
    sources: &[PlannedSource],
    levers: Levers,
) -> bool {
    let joined_in_order = sources.iter().skip(1).all(|term| {
        matches!(
            term.join,
            JoinKind::Inner | JoinKind::Comma | JoinKind::Cross | JoinKind::Left
        )
    });
    levers.has(Levers::STREAMING_GROUP)
        && joined_in_order
        && select.windows.is_empty()
        && select.compounds.is_empty()
}

/// Returns whether the walk brings the rows of each `GROUP BY` key together.
///
/// Grouping needs adjacency rather than order, so the direction does not
/// matter: what matters is that the walk's leading keys are exactly the group
/// columns. Exactly, not merely a superset - a walk ordered by `(a, b)` groups
/// `a` and groups `(a, b)`, and does not group `b`.
///
/// The collation does matter. Grouping compares keys with the result collation
/// and the walk compares them with the structure's, so a `NOCASE` index does
/// not group a `BINARY` key: it would put `Ada` and `ADA` next to each other
/// and the grouping would then treat them as one.
/// @param select - the bound statement
/// @param outer - the planned outer term
pub(super) fn grouped_by_walk(select: &BoundSelect, outer: &PlannedSource) -> bool {
    if select.group_by.is_empty() {
        return false;
    }
    let Some(key) = path_ordering(&outer.table, &outer.path) else {
        return false;
    };
    let mut wanted: Vec<(OrderedBy, Collation)> = Vec::new();
    for expr in &select.group_by {
        let Some(named) = walk_key_of(expr, outer.id, &outer.table) else {
            return false;
        };
        let collation = crate::bind::result_collation(expr);
        if !wanted.iter().any(|(held, _)| *held == named) {
            wanted.push((named, collation));
        }
    }
    covers_prefix(&key, &wanted)
}

/// Returns whether the walk brings duplicate result rows together.
///
/// The same rule as [`grouped_by_walk`], over the result columns rather than
/// the group ones - and it is only asked when there is no grouping, because a
/// `DISTINCT` over aggregates is distinct over values the walk never saw.
/// @param select - the bound statement
/// @param outer - the planned outer term
pub(super) fn distinct_by_walk(select: &BoundSelect, outer: &PlannedSource) -> bool {
    if !select.distinct || !select.group_by.is_empty() || !select.aggregates.is_empty() {
        return false;
    }
    let Some(key) = path_ordering(&outer.table, &outer.path) else {
        return false;
    };
    let mut wanted: Vec<(OrderedBy, Collation)> = Vec::new();
    for column in &select.columns {
        let Some(named) = walk_key_of(&column.expr, outer.id, &outer.table) else {
            return false;
        };
        let collation = crate::bind::result_collation(&column.expr);
        if !wanted.iter().any(|(held, _)| *held == named) {
            wanted.push((named, collation));
        }
    }
    covers_prefix(&key, &wanted)
}

/// Returns whether a set of keys is exactly the walk's leading keys.
///
/// A key an equality pinned counts as held: it has one value for every row the
/// walk returns, so it is constant across the whole scan and cannot separate
/// two rows that are otherwise equal.
/// @param key - what the walk is ordered by
/// @param wanted - the keys that have to arrive together, with their collations
fn covers_prefix(key: &PathOrdering, wanted: &[(OrderedBy, Collation)]) -> bool {
    let free: Vec<&(OrderedBy, Collation)> = wanted
        .iter()
        .filter(|(named, _)| !key.pinned.contains(named))
        .collect();
    if free.len() > key.columns.len() {
        return false;
    }
    let prefix = match key.columns.get(..free.len()) {
        Some(prefix) => prefix,
        None => return false,
    };
    free.iter().all(|(named, collation)| {
        prefix
            .iter()
            .any(|(held, _, held_collation)| held == named && held_collation == collation)
    })
}

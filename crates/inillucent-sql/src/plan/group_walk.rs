//! Walking an index that computes a `GROUP BY` term, so the groups arrive together.
//!
//! Invariant: **a walk offered here brings every row of a group together, and its entries hold
//! every column the statement reads once the expressions the index computes are answered from the
//! entry.** Both are checked before the walk is a candidate: the first by `grouped_by_path`, the
//! second by `covering_slots` over what the statement reads beyond those expressions.
//!
//! **Why.** `SELECT json_extract(p, '$.k'), count(*) FROM t GROUP BY json_extract(p, '$.k')` over an
//! index on `json_extract(p, '$.k')` read every row of the table, evaluated the function on every
//! `p`, and grouped through a temporary B-tree (bug hunt 11). The index holds the value in key
//! order already, so a covering walk of it streams the groups and reads no table row.
//!
//! Here rather than in [`super`] because `plan.rs` is at its recorded size.

use super::limit_walk::unbounded_walk;
use super::{
    adjacent, covering, covering_slots, index_usable, AccessPath, BoundExpr, BoundSelect,
    BoundSource, Levers,
};

/// Returns a covering walk of every index that computes a `GROUP BY` term and keeps the groups
/// together, for the one term of a statement that groups.
///
/// A partial index is offered only when the statement implies its predicate, `NOT INDEXED` and
/// `INDEXED BY` offer nothing, and a statement that joins, windows, compounds or removes
/// duplicates is left to the ordinary paths.
///
/// @param id - the term's statement wide source id
/// @param source - the term
/// @param select - the bound statement
/// @param terms - the statement's terms
/// @param levers - which optimizations are on
pub(super) fn walks(
    id: usize,
    source: &BoundSource,
    select: &BoundSelect,
    terms: &[BoundExpr],
    levers: Levers,
) -> Vec<AccessPath> {
    let table = &source.table;
    if select.group_by.is_empty()
        || !levers.has(Levers::STREAMING_GROUP)
        || !levers.has(Levers::ORDERED_WALK)
        || !levers.has(Levers::COVERING_INDEX)
        || select.sources.len() != 1
        || !select.windows.is_empty()
        || !select.compounds.is_empty()
        || select.distinct
        || source.index_hint != crate::bind::IndexChoice::Any
    {
        return Vec::new();
    }
    let mut walks = Vec::new();
    for (at, index) in table.indexes.iter().enumerate() {
        if index.origin == crate::catalog_view::IndexOrigin::Module {
            continue;
        }
        let Some(held) = source.index_exprs.iter().find(|held| held.position == at) else {
            continue;
        };
        let keys: Vec<BoundExpr> = held.keys.iter().flatten().cloned().collect();
        let usable = index_usable(source, at, index, terms);
        if keys.is_empty() || (index.partial_sql.is_some() && !usable) {
            continue;
        }
        let reduced = covering::needed_beyond_computed(select, id, &keys);
        let Some(slots) = covering_slots(table, index, &reduced, usable) else {
            continue;
        };
        let walk = unbounded_walk(table, index, Some(slots));
        if adjacent::grouped_by_path(select, id, table, &source.index_exprs, &walk) {
            walks.push(walk);
        }
    }
    walks
}

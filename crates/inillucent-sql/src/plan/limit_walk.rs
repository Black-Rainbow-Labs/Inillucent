//! Walking an index in `ORDER BY` order when a small `LIMIT` stops the walk early.
//!
//! Invariant: **a walk offered here returns the rows the sort it replaces would have returned.**
//! It is a whole walk of one index, with no bound and no equality, so every row of the table is
//! on it, and it is only offered when `ordering_provided` says the walk is already in the order
//! the statement asked for. What changes is the price: a walk that stops after `LIMIT` rows reads
//! that many entries and fetches that many rows, not the whole table.
//!
//! **Why.** `SELECT id, title FROM posts ORDER BY created_at DESC LIMIT 20` over an index on
//! `created_at` scanned all 200,000 posts and sorted them, 19 to 116 ms against SQLite's 0.1 ms
//! (bug hunt 9). An index that does not cover the query was never a candidate: with nothing to
//! seek to and no column it saves reading, `index_candidate` drops it. And had it been one, it
//! would have been priced as a lookup for every row of the table, which is dearer than the scan
//! and the sort together. SQLite prices the walk by the rows the `LIMIT` lets it stop at.

use super::{
    cost, covering, covering_slots, estimated_rows, index_usable,
    join_cost::outer_order_survives_joins, ordering_provided, path_cost, AccessPath, BoundExpr,
    BoundSelect, BoundSource, Levers,
};

/// How many rows the walk is assumed to read for each row it keeps when a `WHERE` term is left
/// for it to test.
///
/// The planner has no estimate of a residual's selectivity here, and pricing the walk as if every
/// row passed would take it for a filter that keeps one row in a thousand. SQLite makes the same
/// guess with its own number; ten keeps the walk for the ordinary filter and gives it up for a
/// rare one, where the scan and sort is also the better plan.
const RESIDUAL_FACTOR: f64 = 10.0;

/// Returns the rows a constant `LIMIT` and `OFFSET` let the statement stop at.
///
/// `None` when there is no `LIMIT`, when either is not an integer literal, or when the statement
/// aggregates, groups or removes duplicates, because then the `LIMIT` counts output rows that
/// are not walked rows.
///
/// @param select - the bound statement
pub(super) fn limit_rows(select: &BoundSelect) -> Option<f64> {
    if !select.group_by.is_empty() || !select.aggregates.is_empty() || select.distinct {
        return None;
    }
    let limit = match select.limit.as_ref()? {
        BoundExpr::Integer(value) if *value >= 0 => *value,
        _ => return None,
    };
    let offset = match select.offset.as_ref() {
        None => 0,
        Some(BoundExpr::Integer(value)) => (*value).max(0),
        Some(_) => return None,
    };
    Some(limit.saturating_add(offset) as f64)
}

/// Scales an ordered path's cost by the share of its rows a `LIMIT` lets it read.
///
/// @param cost - the path's cost when it reads every row it can return
/// @param rows - how many rows it can return
/// @param wanted - the rows the `LIMIT` stops at
/// @param residual - whether a `WHERE` term is left for the walk to test
pub(super) fn limited_cost(cost: f64, rows: f64, wanted: f64, residual: bool) -> f64 {
    let read = if residual {
        wanted * RESIDUAL_FACTOR
    } else {
        wanted
    };
    let share = if rows > 0.0 {
        (read.max(1.0) / rows).min(1.0)
    } else {
        1.0
    };
    cost * share
}

/// Picks the cheapest of a term's candidate paths, pricing in the sort it saves and the rows a
/// `LIMIT` lets it stop at, and marks the terms the chosen path enforces as consumed.
///
/// A scan beats a search that returns most of the table: an index that has to fetch every row
/// costs a second descent per row on top of the scan it was meant to avoid. And a path that
/// already produces the ORDER BY beats one that does not by the whole cost of the sort it saves.
/// A walk of an index in the ORDER BY's order is added here when a constant LIMIT lets it stop.
///
/// @param term - the term being planned and the statement it is in
/// @param consumed - which terms are enforced; updated with the chosen path's
/// @param candidates - each path with the terms it would enforce
pub(super) fn cheapest(
    term: &PathChoice<'_>,
    consumed: &mut [bool],
    mut candidates: Vec<(AccessPath, Vec<bool>)>,
) -> AccessPath {
    let PathChoice {
        position,
        id,
        source,
        select,
        terms,
        levers,
    } = *term;
    let table = &source.table;
    let sort = sort_penalty(select, position, source, levers);
    // A walk of an index in the ORDER BY's order, offered when a constant LIMIT lets it stop
    // early. See `limit_walk`: a walk that does not cover the query was never a candidate, so
    // `ORDER BY created_at DESC LIMIT 20` scanned and sorted the whole table.
    let limited = if sort > 0.0 { limit_rows(select) } else { None };
    if limited.is_some() && source.index_hint == crate::bind::IndexChoice::Any {
        for walk in ordered_walks(id, source, select, terms, levers) {
            candidates.push((walk, consumed.to_vec()));
        }
    }
    let mut best: Option<(f64, AccessPath, Vec<bool>)> = None;
    for (path, trial) in candidates {
        let (mut cost, rows) = path_cost(source, &path);
        let ordered = levers.has(Levers::ORDERED_WALK)
            && ordering_provided(select, id, table, &source.index_exprs, &path).is_some();
        match (ordered, limited) {
            (false, _) => cost += sort,
            // Every term a path does not enforce is tested on each row it walks, which is
            // what the residual factor prices.
            (true, Some(wanted)) => {
                let residual = trial.iter().any(|enforced| !enforced);
                cost = limited_path_cost(&path, cost, rows, wanted, residual);
            }
            (true, None) => {}
        }
        if best
            .as_ref()
            .is_none_or(|(existing, _, _)| cost < *existing - 1e-9)
        {
            best = Some((cost, path, trial));
        }
    }
    match best {
        Some((_, path, trial)) => {
            consumed.copy_from_slice(&trial);
            if sort > 0.0 {
                covering_after_order(path, term)
            } else {
                path
            }
        }
        None => AccessPath::TableScan { root: table.root },
    }
}

/// Gives a chosen index path the covering entry it would have had if the `ORDER BY` the walk
/// answers read no columns.
///
/// **A seek on `a = 1` over an index on `(a, json_extract(p, '$.v'))` answers
/// `ORDER BY json_extract(p, '$.v')` by its own order**, and the executor then never evaluates
/// that term, so `SELECT id` does not need `p` from the table. The candidate that built the path
/// could not know the order was provided, because that is decided against the whole statement.
/// A path that already covers, or that does not use an index which computes the order, is
/// returned unchanged.
///
/// @param path - the access path the term was given
/// @param term - the term being planned and the statement it is in
fn covering_after_order(mut path: AccessPath, term: &PathChoice<'_>) -> AccessPath {
    let AccessPath::IndexSeek {
        covering: None,
        index_name,
        ..
    } = &path
    else {
        return path;
    };
    let index_name = index_name.clone();
    let PathChoice {
        id,
        source,
        select,
        terms,
        levers,
        ..
    } = *term;
    let table = &source.table;
    if !levers.has(Levers::COVERING_INDEX)
        || select.order_by.is_empty()
        || ordering_provided(select, id, table, &source.index_exprs, &path).is_none()
    {
        return path;
    }
    let Some(reduced) = covering::needed_beyond_order(select, id) else {
        return path;
    };
    let Some((at, index)) = table
        .indexes
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.name == index_name)
    else {
        return path;
    };
    let usable = index_usable(source, at, index, terms);
    if let (AccessPath::IndexSeek { covering, .. }, Some(slots)) =
        (&mut path, covering_slots(table, index, &reduced, usable))
    {
        *covering = Some(slots);
    }
    path
}

/// Returns what a sort would cost this term, or nothing when no path could
/// avoid one anyway.
///
/// Only the outermost term of a single-term statement can answer an `ORDER BY`
/// by walking: an inner loop restarts for every outer row, and the order it
/// produces inside one of those runs is not the order of the result. Charging
/// the sort anywhere else would tilt a plan towards an index for a saving it
/// would not make.
/// @param select - the bound statement
/// @param position - which visiting position this term is at
/// @param source - the term being priced
pub(super) fn sort_penalty(
    select: &BoundSelect,
    position: usize,
    source: &BoundSource,
    levers: Levers,
) -> f64 {
    // A grouped or DISTINCT statement that streams over the walk answers its
    // ORDER BY the same way an ungrouped one does, so it is priced the same
    // way. Charging it the sort regardless would hide the saving that makes the
    // index path worth taking.
    let streams = levers.has(Levers::STREAMING_GROUP)
        && ((!select.group_by.is_empty() && !select.distinct)
            || (select.distinct && select.group_by.is_empty() && select.aggregates.is_empty()));
    let answerable = levers.has(Levers::ORDERED_WALK)
        && position == 0
        && (select.sources.len() == 1
            || (outer_order_survives_joins(select)
                && select.group_by.is_empty()
                && select.aggregates.is_empty()
                && !select.distinct))
        && select.windows.is_empty()
        && select.compounds.is_empty()
        && !select.order_by.is_empty()
        && ((select.group_by.is_empty() && select.aggregates.is_empty() && !select.distinct)
            || streams);
    if !answerable {
        return 0.0;
    }
    cost::sort_cost(estimated_rows(&source.table))
}

/// One FROM term being given a path, and the statement it is in.
#[derive(Clone, Copy)]
pub(super) struct PathChoice<'a> {
    /// Which visiting position the term is at.
    pub(super) position: usize,
    /// The term's statement wide source id.
    pub(super) id: usize,
    /// The term.
    pub(super) source: &'a BoundSource,
    /// The bound statement.
    pub(super) select: &'a BoundSelect,
    /// The statement's terms.
    pub(super) terms: &'a [BoundExpr],
    /// Which optimizations are on.
    pub(super) levers: Levers,
}

/// Prices an ordered path that a `LIMIT` stops early, with a range bound counted in its favour.
///
/// A bound starts the walk where the rows begin. The same walk without it reads the same share of
/// a longer range and has to step over every row the bound would have excluded, which the
/// proportional scaling in `limited_cost` does not see. Without the discount the two tie, and the
/// walk that starts keyset pages from the end of the index wins by coming first.
///
/// @param path - the access path being priced
/// @param cost - the path's cost when it reads every row it can return
/// @param rows - how many rows it can return
/// @param wanted - the rows the `LIMIT` stops at
/// @param residual - whether a `WHERE` term is left for the walk to test
pub(super) fn limited_path_cost(
    path: &AccessPath,
    cost: f64,
    rows: f64,
    wanted: f64,
    residual: bool,
) -> f64 {
    let bounded =
        matches!(path, AccessPath::IndexSeek { low, high, .. } if low.is_some() || high.is_some());
    let discount = if bounded { 0.9 } else { 1.0 };
    limited_cost(cost, rows, wanted, residual) * discount
}

/// Returns an unbounded walk of every index whose order answers the `ORDER BY`.
///
/// Only for the one term of a single term statement with a constant `LIMIT`, the case
/// `limited_cost` prices. A partial index is offered only when the statement implies its
/// predicate, and `NOT INDEXED` offers nothing.
///
/// @param id - the term's source id
/// @param source - the term
/// @param select - the bound statement
/// @param terms - the statement's terms
/// @param levers - which optimizations are on
pub(super) fn ordered_walks(
    id: usize,
    source: &BoundSource,
    select: &BoundSelect,
    terms: &[BoundExpr],
    levers: Levers,
) -> Vec<AccessPath> {
    let table = &source.table;
    if limit_rows(select).is_none()
        || !levers.has(Levers::ORDERED_WALK)
        || select.sources.iter().any(|held| {
            matches!(
                held.join,
                crate::ast::JoinKind::Right | crate::ast::JoinKind::Full
            )
        })
        || source.index_hint != crate::bind::IndexChoice::Any
    {
        return Vec::new();
    }
    let needed = select.columns_read(id);
    // A walk of an index that computes the `ORDER BY` replaces the sort, so what only the sort
    // read need not be in the entry.
    let beyond_order = covering::needed_beyond_order(select, id);
    let mut walks = Vec::new();
    for (at, index) in table.indexes.iter().enumerate() {
        if index.origin == crate::catalog_view::IndexOrigin::Module {
            continue;
        }
        let usable = index_usable(source, at, index, terms);
        if index.partial_sql.is_some() && !usable {
            continue;
        }
        let computes_order = beyond_order.is_some()
            && source.index_exprs.iter().any(|held| {
                table
                    .indexes
                    .get(held.position)
                    .is_some_and(|candidate| candidate.name == index.name)
            });
        let wanted = match &beyond_order {
            Some(reduced) if computes_order => reduced,
            _ => &needed,
        };
        let covering = levers
            .has(Levers::COVERING_INDEX)
            .then(|| covering_slots(table, index, wanted, usable))
            .flatten();
        let walk = AccessPath::IndexSeek {
            table_root: table.root,
            index_root: index.root,
            index_name: index.name.clone(),
            equalities: Vec::new(),
            unconverted: Vec::new(),
            low: None,
            high: None,
            collations: Vec::new(),
            descending: Vec::new(),
            columns: Vec::new(),
            without_rowid: table.without_rowid,
            key_entry_slots: if table.without_rowid && index.root != table.root {
                let leading = index.columns.len();
                (0..table.primary_key().len())
                    .map(|offset| leading.saturating_add(offset))
                    .collect()
            } else {
                Vec::new()
            },
            covering,
        };
        let ordered = ordering_provided(select, id, table, &source.index_exprs, &walk);
        if ordered.is_some_and(|_| !select.order_by.is_empty()) {
            walks.push(walk);
        }
    }
    walks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A walk that stops after 20 of 200,000 rows costs a ten thousandth of the whole walk, and a
    /// residual multiplies what it reads by the residual factor.
    #[test]
    fn a_limit_scales_the_walk_by_the_rows_it_stops_at() {
        assert!((limited_cost(1000.0, 200_000.0, 20.0, false) - 0.1).abs() < 1e-9);
        assert!((limited_cost(1000.0, 200_000.0, 20.0, true) - 1.0).abs() < 1e-9);
        assert!((limited_cost(1000.0, 10.0, 20.0, false) - 1000.0).abs() < 1e-9);
    }
}

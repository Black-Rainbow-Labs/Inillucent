//! The order the terms of one run of ordinary joins are visited in.
//!
//! Invariant: **an order chosen here only changes how fast a statement runs, never its rows,
//! and a term whose table function arguments read another term always comes after it.** Each
//! candidate order is priced by [`super::order_cost`]; the only constraint on which orders may be
//! priced is the prerequisite rule in [`argument_prerequisites`].

use super::{choose_path, flatten, order_cost, residual_share, subquery_rows, Levers};
use crate::bind::{BoundExpr, BoundSelect};

/// The most terms a run is searched exhaustively for: 8! is 40,320 orders,
/// which is milliseconds, and 9! is ten times that.
const EXHAUSTIVE_TERMS: usize = 8;

/// Returns the cheapest visiting order for one run of reorderable terms.
///
/// **Above [`EXHAUSTIVE_TERMS`] the order is searched for by
/// [`searched_order`], and the written order is kept only when the model prices
/// it no higher.** Keeping the written order for every longer run turned a chain of equalities into a cross
/// product whenever it was not written in join order: sqllogictest's
/// `select5.test` joins 19 tables of 10 rows, and its written order scanned
/// four unrelated tables one inside another before the first key lookup, so
/// the join held ten thousand rows and then a hundred times that, and one
/// process passed 3 GB in a second. SQLite answers it in 0.2 s with every
/// term but one a key lookup, because beyond its own exhaustive limit it
/// still searches.
///
/// @param select - the bound statement
/// @param terms - the statement's `WHERE` and `ON` conjuncts
/// @param run - the positions of one run of reorderable terms
/// @param levers - the planner's switches
pub(super) fn best_order(
    select: &BoundSelect,
    terms: &[BoundExpr],
    run: &[usize],
    levers: Levers,
) -> Vec<usize> {
    if run.len() < 2 {
        return run.to_vec();
    }
    let prerequisites = argument_prerequisites(select, terms, run);
    if run.len() > EXHAUSTIVE_TERMS {
        return searched_order(select, terms, run, &prerequisites, levers);
    }
    let mut best: Option<(f64, Vec<usize>)> = None;
    let mut candidate = run.to_vec();
    permute(&mut candidate, 0, &mut |order| {
        if !respects_prerequisites(order, &prerequisites) {
            return;
        }
        let cost = order_cost(select, terms, order, levers);
        let better = best
            .as_ref()
            .is_none_or(|(existing, _)| cost < *existing - 1e-9);
        if better {
            best = Some((cost, order.to_vec()));
        }
    });
    best.map(|(_, order)| order).unwrap_or_else(|| run.to_vec())
}

/// How many partial orders [`searched_order`] keeps at each step.
///
/// SQLite keeps 10 for a join of three or more tables. Keeping one per term
/// instead planned `select5.test`'s join of 64 tables in 0.6 s against 0.1 s,
/// and answered none of its 732 queries differently.
const KEPT_ORDERS: usize = 10;

/// One partial visiting order in [`searched_order`], and what it costs so far.
#[derive(Clone)]
struct Partial {
    /// The positions placed so far, in visiting order.
    order: Vec<usize>,
    /// Which positions in `select.sources` are placed, by position.
    placed: Vec<bool>,
    /// Which terms the paths placed so far enforce.
    consumed: Vec<bool>,
    /// What visiting them costs.
    cost: f64,
    /// The rows the last of them hands on.
    rows: f64,
}

/// Returns a visiting order for a run too long to search exhaustively.
///
/// **A search over the most promising partial orders, as SQLite's path
/// solver does.** Each step extends every kept order by every term that may
/// come next, and keeps the cheapest [`KEPT_ORDERS`], and only the cheapest of
/// the orders that hold the same terms. One
/// order built greedily is not enough: in `select5.test`'s chains each table's
/// `b` is the next one's key, so after the pinned table every other table costs
/// one scan, and the scan that pays is the chain's first table, after which
/// every other table is a key lookup. A greedy step cannot see that, and
/// following the chain backwards, from scan to scan, looks cheaper one step at
/// a time. Kept side by side, the order through the first table is the cheaper
/// one a step later.
///
/// The orders are ranked by what visiting their terms costs. What ordering the
/// output then costs, and what a first term that already provides the
/// `ORDER BY` saves, is added only to the finished orders, so an order whose
/// saving is all in its first term can be dropped before it counts. The
/// written order is priced with the finished ones and kept on a tie: the
/// binder refuses a table function argument that names a later term, so the
/// written order always respects the prerequisites.
///
/// A step prices one more term for each kept order, so the whole is quadratic
/// in the run's length: about 20,000 terms priced for the 64 tables
/// `select5.test` joins. A term whose arguments read another term waits for
/// that term, as in the exhaustive search.
///
/// @param select - the bound statement
/// @param terms - the statement's `WHERE` and `ON` conjuncts
/// @param run - the positions of one run of reorderable terms
/// @param prerequisites - pairs from [`argument_prerequisites`]
/// @param levers - the planner's switches
fn searched_order(
    select: &BoundSelect,
    terms: &[BoundExpr],
    run: &[usize],
    prerequisites: &[(usize, usize)],
    levers: Levers,
) -> Vec<usize> {
    let mut partials = vec![Partial {
        order: Vec::with_capacity(run.len()),
        placed: vec![false; select.sources.len()],
        consumed: vec![false; terms.len()],
        cost: 0.0,
        rows: 1.0,
    }];
    for _ in 0..run.len() {
        let mut next: Vec<Partial> = Vec::new();
        // Which entry of `next` holds each set of placed terms, so that only
        // the cheapest order of one set is kept, found without a scan of
        // `next`: a step of 64 terms makes about 600 orders.
        let mut by_set: std::collections::HashMap<Vec<bool>, usize> =
            std::collections::HashMap::new();
        for partial in &partials {
            let is_placed = |at: &usize| partial.placed.get(*at).copied().unwrap_or(false);
            for candidate in run.iter().filter(|at| !is_placed(at)) {
                let waits = prerequisites
                    .iter()
                    .any(|(needs, needed)| needs == candidate && !is_placed(needed));
                if waits {
                    continue;
                }
                let extended = extend(select, terms, run, partial, *candidate, levers);
                match by_set
                    .get(&extended.placed)
                    .and_then(|at| next.get_mut(*at))
                {
                    Some(held) if cheaper(&extended, held) => *held = extended,
                    Some(_) => {}
                    None => {
                        by_set.insert(extended.placed.clone(), next.len());
                        next.push(extended);
                    }
                }
            }
        }
        // Only a cycle of prerequisites leaves nothing placeable, and the
        // written order is then as good as any.
        if next.is_empty() {
            return run.to_vec();
        }
        next.sort_by(|left, right| {
            left.cost
                .total_cmp(&right.cost)
                .then(left.rows.total_cmp(&right.rows))
        });
        next.truncate(KEPT_ORDERS);
        partials = next;
    }
    let mut best = (order_cost(select, terms, run, levers), run.to_vec());
    for partial in partials {
        let cost = order_cost(select, terms, &partial.order, levers);
        if cost < best.0 - 1e-9 {
            best = (cost, partial.order);
        }
    }
    best.1
}

/// Reports whether one partial order is cheaper than another, its rows breaking a tie.
///
/// @param candidate - the order that might replace `held`
/// @param held - the order kept so far
fn cheaper(candidate: &Partial, held: &Partial) -> bool {
    candidate.cost < held.cost - 1e-9
        || (candidate.cost <= held.cost + 1e-9 && candidate.rows < held.rows)
}

/// Returns a partial order with one more term visited after it, priced.
///
/// The same arithmetic as [`order_cost`], one level at a time. **The terms not
/// yet placed follow the new one in the order its path is chosen against,**
/// because a path may read only the terms before it, and a term left out of the
/// order would count as one an enclosing block holds, which every level may
/// read: every term would then be priced as a key lookup.
///
/// @param select - the bound statement
/// @param terms - the statement's `WHERE` and `ON` conjuncts
/// @param run - the positions of the run being ordered
/// @param partial - the order so far
/// @param candidate - the position to visit next
/// @param levers - the planner's switches
fn extend(
    select: &BoundSelect,
    terms: &[BoundExpr],
    run: &[usize],
    partial: &Partial,
    candidate: usize,
    levers: Levers,
) -> Partial {
    let mut order = partial.order.clone();
    order.push(candidate);
    let mut placed = partial.placed.clone();
    if let Some(slot) = placed.get_mut(candidate) {
        *slot = true;
    }
    let mut consumed = partial.consumed.clone();
    let Some(source) = select.sources.get(candidate) else {
        return Partial {
            order,
            placed,
            consumed,
            ..partial.clone()
        };
    };
    let unplaced = run
        .iter()
        .filter(|at| !placed.get(**at).copied().unwrap_or(false));
    let ids: Vec<usize> = order
        .iter()
        .chain(unplaced)
        .filter_map(|position| select.sources.get(*position))
        .map(|held| held.id)
        .collect();
    let level = partial.order.len();
    let path = choose_path(level, &ids, source, select, terms, &mut consumed, levers);
    let (cost, rows) = subquery_rows::ordered_path_cost(select, source, &path);
    let kept = rows * residual_share(select, terms, &consumed, &ids, level, source);
    Partial {
        order,
        placed,
        consumed,
        cost: partial.cost + partial.rows * cost,
        rows: partial.rows * kept.max(1.0),
    }
}

/// Returns, for each table function in a run, the terms its arguments read.
///
/// **A function cannot run before its argument exists.** The cost model
/// priced a function whose argument was not yet available as an ordinary
/// scan, so in `FROM j, json_each(j.doc) e JOIN t ON t.id = e.value` the cheap
/// order put `e` first. The function then ran with a NULL document, returned
/// nothing, and the query answered no rows where SQLite answers every match.
/// This is SQLite's prerequisite rule for a virtual table's constraints.
///
/// Each pair is (the position that needs, the position it needs), both in
/// `select.sources`. A term outside the run, such as one an earlier pinned run
/// holds, is already placed and adds no pair.
///
/// @param select - the bound statement
/// @param terms - the statement's `WHERE` and `ON` conjuncts
/// @param run - the positions of one run of reorderable terms
fn argument_prerequisites(
    select: &BoundSelect,
    terms: &[BoundExpr],
    run: &[usize],
) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for term in terms {
        if !flatten::is_table_argument(select, term) {
            continue;
        }
        let BoundExpr::Compare { left, right, .. } = term else {
            continue;
        };
        let BoundExpr::Column { source: needs, .. } = left.as_ref() else {
            continue;
        };
        let position_of = |id: usize| {
            run.iter()
                .copied()
                .find(|at| select.sources.get(*at).is_some_and(|held| held.id == id))
        };
        let Some(dependent) = position_of(*needs) else {
            continue;
        };
        let mut used = Vec::new();
        right.sources_used(&mut used);
        for id in used.into_iter().filter(|id| id != needs) {
            if let Some(needed) = position_of(id) {
                pairs.push((dependent, needed));
            }
        }
    }
    pairs
}

/// Reports whether every term in an order comes after the terms it needs.
///
/// @param order - one permutation of a run
/// @param prerequisites - pairs from [`argument_prerequisites`]
fn respects_prerequisites(order: &[usize], prerequisites: &[(usize, usize)]) -> bool {
    prerequisites.iter().all(|(needs, needed)| {
        order.iter().position(|at| at == needs) > order.iter().position(|at| at == needed)
    })
}

/// Calls a closure with every permutation of a slice.
fn permute(order: &mut Vec<usize>, at: usize, visit: &mut impl FnMut(&[usize])) {
    if at >= order.len() {
        visit(order);
        return;
    }
    for index in at..order.len() {
        order.swap(at, index);
        permute(order, at.saturating_add(1), visit);
        order.swap(at, index);
    }
}

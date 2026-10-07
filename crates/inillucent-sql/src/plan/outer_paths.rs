//! The access path chosen for one term of a join, and the `ON` terms an inner join
//! leaves for a later `RIGHT` or `FULL` join.
//!
//! Invariant: **a term on the null extendable side of an outer join is never
//! filtered by a `WHERE` term on the way in, and a `RIGHT` or `FULL` term is read
//! whole.** Only the `ON` condition of a `LEFT` term may become a seek, because
//! only then is every row the join needs still produced.

use super::*;

/// What the planner decided for one term of a join.
pub(super) struct TermPath {
    /// The access path the term is read through.
    pub path: AccessPath,
    /// Whether every conjunct of the `ON` condition became part of the seek key.
    pub on_enforced: bool,
    /// The `ON` condition the join still has to test, when a module consumed part of it.
    pub remaining_on: Option<Option<BoundExpr>>,
}

/// Chooses the access path for one term of the join order.
///
/// An outer term's rows are not filtered on the way in: a `WHERE` predicate over
/// the null extendable side is applied after the join, so only the term's own `ON`
/// condition is offered to the path choice, and only for a `LEFT` join.
///
/// @param level - the term's position in the visiting order
/// @param ids - the source ids in visiting order
/// @param source - the term to choose a path for
/// @param select - the bound statement
/// @param terms - the statement's terms
/// @param consumed - which terms an access path already enforces; updated
/// @param levers - which optimizations are on
pub(super) fn choose_term_path(
    level: usize,
    ids: &[usize],
    source: &BoundSource,
    select: &BoundSelect,
    terms: &[BoundExpr],
    consumed: &mut [bool],
    levers: Levers,
) -> TermPath {
    let mut on_enforced = false;
    let mut remaining_on: Option<Option<BoundExpr>> = None;
    let path = if is_outer(source.join) && matches!(source.rows, SourceRows::Table) {
        // **Only a `LEFT` term may seek on its `ON`.** A `RIGHT` or `FULL` term keeps
        // the rows of its own that matched nothing, and only a side read whole can
        // know which those are: `list l RIGHT JOIN todo t ON t.list_id = l.id` with an
        // index on `list_id` probed `todo` per list and never produced the todo whose
        // list does not exist.
        //
        // **A table function on a `RIGHT` or `FULL` term is still given its
        // arguments.** The function learns them only from what it is offered,
        // and a function's arguments are not a filter on the rows the join
        // keeps, since every row it returns satisfies them. With nothing
        // offered, `a RIGHT JOIN generate_series(1, 2)` failed with "first
        // argument missing" and `a RIGHT JOIN json_each('[..]')` returned no
        // rows. Only constant arguments, because the term is read once.
        let all_on = outer_terms(source);
        let on_terms: Vec<BoundExpr> = if source.join == JoinKind::Left {
            all_on.clone()
        } else if source.table.module.is_some() {
            all_on
                .iter()
                .filter(|term| is_constant_table_argument(select, term))
                .cloned()
                .collect()
        } else {
            Vec::new()
        };
        let mut on_consumed = vec![false; on_terms.len()];
        let chosen = choose_path(
            level,
            ids,
            source,
            select,
            &on_terms,
            &mut on_consumed,
            levers,
        );
        // Every conjunct of the condition turned into part of the key, so the probe
        // answers the condition and an index nested loop can null-extend on an empty
        // probe.
        on_enforced = source.join == JoinKind::Left
            && !on_terms.is_empty()
            && on_consumed.iter().all(|held| *held);
        // A term the module was offered is applied by the module itself, and its
        // hidden columns hold no value in the rows it returns, so the join must not
        // test it again.
        //
        // `on_terms` is only part of the `ON` for a `RIGHT` or `FULL` term, so
        // what remains is every conjunct of the whole `ON` the module did not
        // consume.
        if source.table.module.is_some() {
            let taken: Vec<&BoundExpr> = on_terms
                .iter()
                .zip(&on_consumed)
                .filter(|(_, held)| **held)
                .map(|(term, _)| term)
                .collect();
            remaining_on = Some(
                all_on
                    .iter()
                    .filter(|term| !taken.contains(term))
                    .cloned()
                    .reduce(|left, right| BoundExpr::And(Box::new(left), Box::new(right))),
            );
        }
        chosen
    } else if precedes_a_right_join(select, source.id) {
        choose_path_without_where(level, ids, source, select, terms, consumed, levers)
    } else {
        choose_path(level, ids, source, select, terms, consumed, levers)
    };
    TermPath {
        path,
        on_enforced,
        remaining_on,
    }
}

/// Reports whether a conjunct is a table function argument whose value reads
/// no FROM term.
///
/// @param select - the bound statement
/// @param term - one conjunct of an outer join's `ON`
fn is_constant_table_argument(select: &BoundSelect, term: &BoundExpr) -> bool {
    let BoundExpr::Compare { right, .. } = term else {
        return false;
    };
    let mut used = Vec::new();
    right.sources_used(&mut used);
    used.is_empty() && super::flatten::is_table_argument(select, term)
}

/// Reports whether a `RIGHT` or `FULL` join comes after a term in the FROM
/// clause, which makes the term null extendable.
///
/// @param select - the bound statement
/// @param id - the term
fn precedes_a_right_join(select: &BoundSelect, id: usize) -> bool {
    let Some(position) = select.sources.iter().position(|source| source.id == id) else {
        return false;
    };
    select
        .sources
        .iter()
        .skip(position.saturating_add(1))
        .any(|source| matches!(source.join, JoinKind::Right | JoinKind::Full))
}

/// Chooses the path of a term a later `RIGHT` or `FULL` join null extends,
/// offering it the inner joins' `ON` terms and none of the `WHERE`.
///
/// **The `WHERE` is tested after the null extension.** Turned into a seek, `s
/// RIGHT JOIN r ON r.a = s.a WHERE s.k = 1` read no row of an empty `s`, the
/// join null extended every row of `r`, and the consumed term was never tested
/// again, so the query returned every row of `r` where SQLite returns none. An
/// inner join's `ON` is tested before the null extension, so it may still seek.
/// The `WHERE` conjuncts come first in `terms`, which is how they are told apart.
/// A table valued function's arguments are bound into the `WHERE` but are not
/// part of it in SQLite, so they are still offered.
///
/// @param level - the term's position in the visiting order
/// @param ids - the source ids in visiting order
/// @param source - the term to choose a path for
/// @param select - the bound statement
/// @param terms - the statement's terms
/// @param consumed - which terms an access path already enforces; updated
/// @param levers - which optimizations are on
fn choose_path_without_where(
    level: usize,
    ids: &[usize],
    source: &BoundSource,
    select: &BoundSelect,
    terms: &[BoundExpr],
    consumed: &mut [bool],
    levers: Levers,
) -> AccessPath {
    let mut filter_terms = Vec::new();
    if let Some(filter) = &select.filter {
        split_conjunction(filter, &mut filter_terms);
    }
    let withheld: Vec<bool> = filter_terms
        .iter()
        .map(|term| !super::flatten::is_table_argument(select, term))
        .collect();
    let mut offered = consumed.to_vec();
    for (slot, held_back) in offered.iter_mut().zip(&withheld) {
        if *held_back {
            *slot = true;
        }
    }
    // An `ON` term of an inner join written after the `RIGHT` or `FULL` join is
    // tested on the null extended rows too, so it is withheld like the `WHERE`.
    // A `LEFT JOIN t1 ON c = 3 ... WHERE t1.a <> 0` becomes an inner join, and
    // `c = 3` was turned into a seek on the first term, which dropped the rows
    // the `RIGHT JOIN` null extends and then left `t1` matching on a NULL `c`.
    let late = ons_written_after_the_right_join(select, source.id);
    for (slot, held_back) in offered.iter_mut().zip(&late) {
        if *held_back {
            *slot = true;
        }
    }
    let path = choose_path(level, ids, source, select, terms, &mut offered, levers);
    for (index, (slot, held)) in consumed.iter_mut().zip(&offered).enumerate() {
        if !withheld.get(index).copied().unwrap_or(false)
            && !late.get(index).copied().unwrap_or(false)
        {
            *slot = *held;
        }
    }
    path
}

/// Returns, for each statement term, whether it is the `ON` of an inner join
/// written after the first `RIGHT` or `FULL` join that follows a term.
///
/// @param select - the bound statement
/// @param id - the term that the `RIGHT` or `FULL` join null extends
fn ons_written_after_the_right_join(select: &BoundSelect, id: usize) -> Vec<bool> {
    let owners = hint::statement_terms_with_owners(select).1;
    let at = select.sources.iter().position(|source| source.id == id);
    let right = at.and_then(|at| {
        (at.saturating_add(1)..select.sources.len()).find(|later| {
            select
                .sources
                .get(*later)
                .is_some_and(|source| matches!(source.join, JoinKind::Right | JoinKind::Full))
        })
    });
    owners
        .iter()
        .map(|owner| matches!((owner, right), (Some(owner), Some(right)) if *owner > right))
        .collect()
}

/// Moves the unenforced `ON` terms of inner joins into the `RIGHT` or `FULL`
/// join that follows them.
///
/// @param select - the bound statement
/// @param terms - the statement's terms
/// @param held_before - for each term, the FROM position of the join to hold it for
/// @param consumed - which terms an access path already enforces; updated
/// @param sources - the planned terms, in visiting order
pub(super) fn hold_terms_before_right_joins(
    select: &BoundSelect,
    terms: &[BoundExpr],
    held_before: &[Option<usize>],
    consumed: &mut [bool],
    sources: &mut [PlannedSource],
) {
    for (index, term) in terms.iter().enumerate() {
        let Some(Some(position)) = held_before.get(index) else {
            continue;
        };
        if consumed.get(index).copied().unwrap_or(false) {
            continue;
        }
        let Some(id) = select.sources.get(*position).map(|source| source.id) else {
            continue;
        };
        let Some(planned) = sources.iter_mut().find(|planned| planned.id == id) else {
            continue;
        };
        planned.before = Some(match planned.before.take() {
            Some(existing) => BoundExpr::And(Box::new(existing), Box::new(term.clone())),
            None => term.clone(),
        });
        if let Some(mark) = consumed.get_mut(index) {
            *mark = true;
        }
    }
}

/// Returns the affinity each column of a derived table is stored with.
///
/// The first arm decides, as it does for the column's declared type. A column
/// only: a `CAST(i AS REAL)` in the first arm leaves the integers of the other
/// arms as they are. Every arm must also agree that the column is numeric: an
/// arm of a column with no declared type keeps its integers.
///
/// @param block - the derived table's query
pub(super) fn derived_affinities(block: &BoundSelect) -> Vec<inillucent_value::Affinity> {
    block
        .columns
        .iter()
        .enumerate()
        .map(|(at, column)| match &column.expr {
            BoundExpr::Column { affinity, .. }
                if *affinity != inillucent_value::Affinity::Real
                    || block.column_affinity(at).is_numeric() =>
            {
                *affinity
            }
            _ => inillucent_value::Affinity::Blob,
        })
        .collect()
}

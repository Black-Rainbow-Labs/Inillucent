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
        let on_terms = if source.join == JoinKind::Left {
            outer_terms(source)
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
        on_enforced = !on_terms.is_empty() && on_consumed.iter().all(|held| *held);
        // A term the module was offered is applied by the module itself, and its
        // hidden columns hold no value in the rows it returns, so the join must not
        // test it again.
        if source.table.module.is_some() {
            remaining_on = Some(
                on_terms
                    .iter()
                    .zip(&on_consumed)
                    .filter(|(_, held)| !**held)
                    .map(|(term, _)| term.clone())
                    .reduce(|left, right| BoundExpr::And(Box::new(left), Box::new(right))),
            );
        }
        chosen
    } else {
        choose_path(level, ids, source, select, terms, consumed, levers)
    };
    TermPath {
        path,
        on_enforced,
        remaining_on,
    }
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

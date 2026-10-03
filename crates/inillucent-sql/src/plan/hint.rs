//! What `INDEXED BY` and `NOT INDEXED` allow the planner to choose.
//!
//! Invariant: **the binder's refusal and the planner's choice read the same
//! terms.** A statement whose named index cannot answer it is refused before
//! it is planned, because `choose_path` returns an `AccessPath` and has no way
//! to refuse. That is only sound if the question asked here, before planning,
//! is the one `choose_path` answers during it, so both take their terms from
//! [`statement_terms`] and [`outer_terms`] and their verdict on a partial index
//! from [`index_usable`].
//!
//! Here rather than in [`super`] because `plan.rs` is at its recorded size and
//! these five functions are one idea, added in task-2078.

use super::*;

/// Returns the conjuncts a path for an inner or comma joined term may seek on.
///
/// The statement's `WHERE`, and the `ON` of every term that is not the
/// null-extendable side of an outer join. [`plan_select_with`] plans with these,
/// and [`unanswerable_index_hint`] proves a forced index with them, so the two
/// cannot reach different verdicts about the same statement.
/// @param select - the bound statement
pub(super) fn statement_terms(select: &BoundSelect) -> Vec<BoundExpr> {
    statement_terms_with_owners(select).0
}

/// Returns [`statement_terms`] together with, for each term, the position in the
/// FROM list of the join whose `ON` it came from, or `None` for a term of the
/// `WHERE`.
///
/// **The owners are empty unless a `RIGHT` or `FULL` join is present**, because
/// [`terms_held_before_a_right_join`] is their only reader and it answers `None`
/// for every term without one. Building a vector nobody reads cost every compile
/// an allocation, and the compile budget test counts them. The terms are split
/// straight into one vector for the same reason: a temporary vector per
/// conjunction was two more.
///
/// @param select - the bound statement
pub(super) fn statement_terms_with_owners(
    select: &BoundSelect,
) -> (Vec<BoundExpr>, Vec<Option<usize>>) {
    let track = select
        .sources
        .iter()
        .any(|source| matches!(source.join, JoinKind::Right | JoinKind::Full));
    let mut terms = Vec::new();
    let mut owners = Vec::new();
    if let Some(filter) = &select.filter {
        split_conjunction(filter, &mut terms);
    }
    for (position, source) in select.sources.iter().enumerate() {
        if is_outer(source.join) {
            continue;
        }
        if let Some(constraint) = &source.constraint {
            if track {
                owners.resize(terms.len(), None);
            }
            split_conjunction(constraint, &mut terms);
            if track {
                owners.resize(terms.len(), Some(position));
            }
        }
    }
    (terms, owners)
}

/// Returns, for each statement term, the FROM position of the `RIGHT` or `FULL`
/// join it has to be tested before.
///
/// **An inner join's `ON` is a condition on the rows that join produces, and a
/// later `RIGHT` or `FULL` join can null extend those rows.** Tested after that
/// join, as the `WHERE` is, `a JOIN b ON a.x = b.x RIGHT JOIN c ON c.x = b.x`
/// dropped every row of `c` that matched nothing, because the null extended
/// `a.x = b.x` is NULL. The term belongs to the first `RIGHT` or `FULL` join
/// after the join it was written on.
///
/// @param select - the bound statement
/// @param owners - the position each term was written on, `None` for the `WHERE`
pub(super) fn terms_held_before_a_right_join(
    select: &BoundSelect,
    owners: &[Option<usize>],
) -> Vec<Option<usize>> {
    if owners.is_empty() {
        return Vec::new();
    }
    owners
        .iter()
        .map(|owner| {
            let from = (*owner)?;
            (from.saturating_add(1)..select.sources.len()).find(|later| {
                select
                    .sources
                    .get(*later)
                    .is_some_and(|source| matches!(source.join, JoinKind::Right | JoinKind::Full))
            })
        })
        .collect()
}

/// Returns the conjuncts of an outer join term's own `ON`, which are the only
/// ones its path may seek on. `plan_select_with` says why.
/// @param source - the null-extendable term
pub(super) fn outer_terms(source: &BoundSource) -> Vec<BoundExpr> {
    let mut terms = Vec::new();
    if let Some(constraint) = &source.constraint {
        split_conjunction(constraint, &mut terms);
    }
    terms
}

/// Returns the name of an `INDEXED BY` index that cannot answer its term.
///
/// **This is the refusal `choose_path` has nowhere to put.** It returns an
/// `AccessPath` and has a dozen callers, so the binder asks this instead,
/// once per block, before anything is planned. SQLite's answer to such a
/// statement is `no query solution`, and the cases are few, because a named
/// b-tree index can always be walked from end to end: the pinned 3.53.4 shell
/// plans `INDEXED BY h_a` over `WHERE c = 3`, with nothing on `a` at all, as
/// `SCAN h USING INDEX h_a`. What cannot be walked is a partial index whose
/// predicate the statement does not imply, because it would lose the rows the
/// predicate leaves out. `CREATE INDEX h_part ON h(c) WHERE c > 3` refuses
/// `SELECT * FROM h INDEXED BY h_part WHERE a = 1` there, and here.
///
/// An index a module owns is answerable only by the nearest neighbour probe,
/// and a virtual table has no index this clause can name.
/// @param select - one bound block, with its sources attached
pub fn unanswerable_index_hint(select: &BoundSelect) -> Option<Vec<u8>> {
    // `SELECT count(*) FROM t INDEXED BY a_partial_index` counts the table's
    // rows without a scan, so SQLite never asks the index to answer anything.
    if is_a_plain_count(select) {
        return None;
    }
    let mut shared: Option<Vec<BoundExpr>> = None;
    for (position, source) in select.sources.iter().enumerate() {
        let crate::bind::IndexChoice::Only(wanted) = &source.index_hint else {
            continue;
        };
        if !matches!(source.rows, SourceRows::Table) {
            continue;
        }
        let table = &source.table;
        let Some((at, index)) = table
            .indexes
            .iter()
            .enumerate()
            .find(|(_, index)| &index.folded == wanted)
        else {
            continue;
        };
        let answerable = if table.module.is_some() {
            false
        } else if index.origin == crate::catalog_view::IndexOrigin::Module {
            let id = source.id;
            matches!(
                vector_path(id, position, source, select),
                Some(AccessPath::VectorProbe { index: ref chosen, .. }) if chosen == &index.name
            )
        } else if is_outer(source.join) {
            index_usable(source, at, index, &outer_terms(source))
        } else {
            let terms = shared.get_or_insert_with(|| statement_terms(select));
            index_usable(source, at, index, terms)
        };
        if !answerable {
            return Some(index.name.clone());
        }
    }
    None
}

/// Reports whether a block is `SELECT count(*) FROM one_table` and nothing more.
///
/// That is the shape SQLite answers from the size of the table's tree.
///
/// @param select - one bound block, with its sources attached
fn is_a_plain_count(select: &BoundSelect) -> bool {
    select.sources.len() == 1
        && select.filter.is_none()
        && select.group_by.is_empty()
        && select.having.is_none()
        && select.compounds.is_empty()
        && select.windows.is_empty()
        && !select.distinct
        && select.columns.len() == 1
        && select.aggregates.len() == 1
        && select
            .aggregates
            .iter()
            .all(|aggregate| aggregate.star && aggregate.filter.is_none() && !aggregate.distinct)
}

/// Reports whether one b-tree index may be read for a term at all.
///
/// Every index may, except a partial one whose predicate the terms do not
/// imply; `index_path` says why that one would lose rows.
/// @param source - the term
/// @param at - the index's position in the table's list
/// @param index - the index
/// @param terms - the conjuncts the term's path may seek on
pub(super) fn index_usable(
    source: &BoundSource,
    at: usize,
    index: &IndexInfo,
    terms: &[BoundExpr],
) -> bool {
    let computed = source.index_exprs.iter().find(|held| held.position == at);
    index.partial_sql.is_none() || implies(computed, terms)
}

/// Chooses the path for a term written `INDEXED BY name`: that index, read the
/// cheapest way it can be.
///
/// **Nothing else is a candidate**, not the table scan and not the rowid, which
/// is SQLite's rule and was measured against the pinned 3.53.4 shell:
/// `SELECT count(*) FROM h INDEXED BY h_a WHERE a = 3 AND b = 100` searches
/// `h_a` there even though `ANALYZE` prefers `h_b`, and until task-2078 it
/// searched `h_b` here. When nothing in the statement seeks the index it is
/// walked end to end, which is what `SCAN h USING INDEX h_a` means.
///
/// The binder has already refused a statement the index cannot answer, through
/// [`unanswerable_index_hint`], so the table scan at the bottom is only reached
/// by a caller that built a `BoundSource` without the binder. It returns every
/// row, which is the answer that cannot be wrong.
/// @param id - the term's statement-wide id
/// @param position - its place in the visiting order
/// @param ids - every term's id in visiting order
/// @param source - the term
/// @param select - the whole statement, for the columns it reads
/// @param terms - the conjuncts the path may seek on
/// @param consumed - which conjuncts an earlier path already answers
/// @param levers - which optimizations are on
pub(super) fn forced_path(
    id: usize,
    position: usize,
    ids: &[usize],
    source: &BoundSource,
    select: &BoundSelect,
    terms: &[BoundExpr],
    consumed: &mut [bool],
    levers: Levers,
) -> AccessPath {
    let needed = select.columns_read(id);
    index_path(id, position, ids, source, terms, consumed, &needed, levers).unwrap_or(
        AccessPath::TableScan {
            root: source.table.root,
        },
    )
}

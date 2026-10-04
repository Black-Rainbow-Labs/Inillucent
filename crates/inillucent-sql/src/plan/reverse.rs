//! What `PRAGMA reverse_unordered_selects` turns around.
//!
//! Invariant: **a query with no `ORDER BY` returns its rows in the reverse of
//! the order it returns them in with the pragma off, wherever SQLite would**,
//! and a query the pragma cannot reverse in SQLite is left alone.

use super::*;

/// Returns whether `PRAGMA reverse_unordered_selects` makes the outer scan run
/// backwards.
///
/// SQLite reverses every scan whose direction nothing asks for. This reverses
/// the outermost term of a query with no `ORDER BY`, no `GROUP BY` and no
/// `DISTINCT` (a plain aggregate such as `group_concat(a)` is included, because
/// the order the rows reach it in is visible), which is the order the rows leave in when the inner terms are
/// seeks that find one row each. A statement that sorts anyway, or whose outer
/// term is not a scan of a table or an index, is left alone.
///
/// @param select - the bound statement
/// @param sources - the planned FROM terms
/// @param levers - the optimizations that were on when the plan was chosen
pub(super) fn reverses_unordered_scan(
    select: &BoundSelect,
    sources: &[PlannedSource],
    levers: Levers,
) -> bool {
    !levers.has(Levers::FORWARD_UNORDERED)
        && select.order_by.is_empty()
        && !select.distinct
        && select.group_by.is_empty()
        && select.windows.is_empty()
        && select.compounds.is_empty()
        && sources.first().is_some_and(|outer| {
            matches!(
                outer.path,
                AccessPath::TableScan { .. }
                    | AccessPath::RowidRange { .. }
                    | AccessPath::IndexSeek { .. }
            )
        })
}

/// Returns whether `PRAGMA reverse_unordered_selects` turns the whole joined row
/// stream around.
///
/// SQLite runs every loop of a query with no `ORDER BY` backwards: the outer
/// scan, each inner join term, the inner side of a `LEFT JOIN`, and the values
/// of an `IN` list, which it walks from the largest. Reversing only the outer
/// scan left the rows each outer row joined to in forward order, so `SELECT *
/// FROM a, b WHERE b.aid = a.id` disagreed with SQLite on every outer row with
/// more than one match. A nested loop whose every level runs backwards
/// produces the forward rows in reverse, so the executor reverses the joined
/// rows instead.
///
/// That holds only when every term is a loop SQLite can run backwards. It
/// cannot reverse a virtual table, a subquery it reads as a co-routine, or the
/// queue of a recursive CTE, so a query with one of those keeps the outer scan
/// rule in `reverses_unordered_scan`. A `GROUP BY` or `DISTINCT` is answered
/// in key order by SQLite, so neither is reversed. Each arm of a compound is
/// planned on its own and reverses its own rows, which is what SQLite does for
/// `UNION ALL`.
///
/// @param select - the bound statement
/// @param sources - the planned FROM terms
/// @param levers - the optimizations that were on when the plan was chosen
pub(super) fn reverses_the_row_stream(
    select: &BoundSelect,
    sources: &[PlannedSource],
    levers: Levers,
) -> bool {
    !levers.has(Levers::FORWARD_UNORDERED)
        && select.order_by.is_empty()
        && !select.distinct
        && select.group_by.is_empty()
        && select.windows.is_empty()
        && !sources.is_empty()
        && sources.iter().all(|source| {
            matches!(
                source.path,
                AccessPath::TableScan { .. }
                    | AccessPath::RowidSeek { .. }
                    | AccessPath::RowidRange { .. }
                    | AccessPath::RowidSeekUnion { .. }
                    | AccessPath::IndexSeek { .. }
                    | AccessPath::IndexSeekUnion { .. }
            ) && source.table.kind == crate::catalog_view::TableKind::Table
        })
}

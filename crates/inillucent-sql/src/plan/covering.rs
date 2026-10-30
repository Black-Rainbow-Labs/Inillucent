//! Which columns of a query an index's entries hold.
//!
//! Invariant: **a path is covering only when every column the query reads sits in the entry**, so
//! a scan of the index never has to fetch the table row.

use super::{BoundExpr, ColumnUse, IndexInfo, TableInfo, ROWID_ENTRY_SLOT};

/// Returns where each column the query reads sits in one index's entries.
///
/// `None` when the index does not hold them all, which is the ordinary case and
/// is why a covering path is worth naming when it happens. A `WITHOUT ROWID`
/// table's own tree is excluded: its rows *are* index entries, so the question
/// is already answered by whether the seek is on the table's own key. A
/// secondary index on such a table ends each entry with the whole primary key,
/// and those fields count as held too, so `SELECT tenant, item, score` over
/// `PRIMARY KEY(tenant, item)` and an index on `(score)` is answered from the
/// index alone, as SQLite does.
/// @param table - the table being read
/// @param index - the index being considered
/// @param needed - what the query reads from this term
pub(super) fn covering_slots(
    table: &TableInfo,
    index: &IndexInfo,
    needed: &ColumnUse,
    usable: bool,
) -> Option<Vec<(u16, usize)>> {
    if needed.opaque || !usable || (table.without_rowid && index.root == table.root) {
        return None;
    }
    // Only a WITHOUT ROWID entry holds the primary key, and building the list allocates, so an
    // ordinary table never asks for it.
    let key = if table.without_rowid {
        table.primary_key()
    } else {
        Vec::new()
    };
    let mut slots = Vec::with_capacity(needed.columns.len());
    for slot in &needed.columns {
        // **`needed` names record slots and an index names declared columns.**
        // For a `WITHOUT ROWID` table the record starts with the primary key,
        // so with `PRIMARY KEY(c)` over `(a, b, c)` record slot 2 is `b` and
        // declared column 2 is `c`. Comparing the two directly made an index on
        // `a` look like it held `b` (as the key's `c`), and `SELECT c FROM t
        // WHERE b = 2` was planned as a covering scan of that index and failed
        // with "the tree read for FROM term 0 does not carry column 1".
        let column = declared_column(table, *slot)?;
        // The rowid alias is a column of the table and the *rowid* of the
        // entry, so it is covered whatever the index holds - but it is read
        // with `IdxRowid` rather than out of the entry's record, so it is not
        // in the list.
        if table.rowid_alias == Some(column) && !table.without_rowid {
            slots.push((*slot, ROWID_ENTRY_SLOT));
            continue;
        }
        let held = index
            .columns
            .iter()
            .position(|held| held.plain_column() == Some(column));
        // The primary key fields follow the index's own columns in a WITHOUT ROWID entry.
        let position = match held {
            Some(position) => position,
            None if table.without_rowid => {
                let offset = key.iter().position(|held| *held == column)?;
                index.columns.len().saturating_add(offset)
            }
            None => return None,
        };
        slots.push((*slot, position));
    }
    Some(slots)
}

/// Returns the declared column a record slot holds.
///
/// A `WITHOUT ROWID` table's record puts the primary key first, and a rowid
/// table's record leaves out its `VIRTUAL` generated columns, so a record slot
/// is not a declared position. `None` when the slot is past the record.
///
/// @param table - the table being read
/// @param slot - a record slot a query reads
fn declared_column(table: &TableInfo, slot: u16) -> Option<u16> {
    if table.without_rowid {
        return table.record_order().get(usize::from(slot)).copied();
    }
    let mut remaining = usize::from(slot);
    for (position, info) in table.columns.iter().enumerate() {
        if info.generated && !info.stored {
            continue;
        }
        if remaining == 0 {
            return u16::try_from(position).ok();
        }
        remaining = remaining.saturating_sub(1);
    }
    None
}

/// Returns what a query reads of a term once its `ORDER BY` is left out, when an index that
/// computes an `ORDER BY` term as a key could answer that order by being walked.
///
/// **A walk of an index on `json_extract(p, '$.k')` produces rows in the order of that
/// expression without reading `p`.** `SELECT id FROM t ORDER BY json_extract(p, '$.k') LIMIT 10`
/// reads `p` only to sort, and the walk replaces the sort, so the entry holds everything the
/// statement reads. `None` when no `ORDER BY` term is a computed key, so the caller keeps what
/// the whole statement reads and a statement that cannot use one never pays for the copy.
///
/// @param select - the bound statement
/// @param id - the statement wide number of the FROM term
pub(super) fn needed_beyond_order(
    select: &crate::bind::BoundSelect,
    id: usize,
) -> Option<ColumnUse> {
    if !super::ordering::orders_by_computed_key(select, id) {
        return None;
    }
    let mut trimmed = select.clone();
    trimmed.order_by.clear();
    Some(trimmed.columns_read(id))
}

/// Returns what a query reads of a term once the equalities a computed key enforces are left out.
///
/// **An index on an expression covers a query whose only use of a column is inside that
/// expression.** `SELECT count(*) FROM events WHERE json_extract(payload, '$.type') = 'buy'` over
/// an index on `json_extract(payload, '$.type')` reads `payload` only in the term the seek
/// answers, and SQLite counts the index entries. This read `payload` as needed, found the index
/// did not hold it, and scanned the table: 129 ms against 11 ms on 300,000 events (bug hunt 9).
/// The term is answered by the seek and not tested again, so the columns it reads are not read.
///
/// `None` when no such equality was used, so the caller keeps what the query reads.
///
/// @param id - the statement wide number of the FROM term
/// @param needed - what the whole query reads of the term
/// @param terms - the statement's terms
/// @param answered - the terms the computed keys' equalities consumed
pub(super) fn needed_beyond_expressions(
    id: usize,
    needed: &ColumnUse,
    terms: &[BoundExpr],
    answered: &[usize],
) -> Option<ColumnUse> {
    if answered.is_empty() {
        return None;
    }
    let mut reduced = ColumnUse {
        columns: needed.outside_filter.clone(),
        rowid: needed.rowid,
        opaque: needed.opaque,
        functions: needed.functions.clone(),
        outside_filter: needed.outside_filter.clone(),
    };
    for (position, term) in terms.iter().enumerate() {
        if !answered.contains(&position) {
            term.columns_read(id, &mut reduced);
        }
    }
    Some(reduced)
}

/// Returns what a query reads of a term once every occurrence of an expression one of the term's
/// indexes computes is left out.
///
/// **An index entry holds the value of the expression it indexes, so a query that reads `p` only
/// inside `json_extract(p, '$.k')` does not need `p` when it reads that index.** The executor
/// answers such an expression from the entry (`PreparedStage::computed`), so the list returned
/// here is what the walk of that index has to hold for the statement to be covered by it. Every
/// expression of the block is visited, because one missed occurrence is a read of a column the
/// entry does not hold, which fails the statement instead of answering it.
///
/// @param select - the bound statement
/// @param id - the statement wide number of the FROM term
/// @param keys - the bound expressions the index computes
pub(super) fn needed_beyond_computed(
    select: &crate::bind::BoundSelect,
    id: usize,
    keys: &[BoundExpr],
) -> ColumnUse {
    let mut trimmed = select.clone();
    let mut blank = |expr: &mut BoundExpr| blank_computed(expr, keys);
    trimmed.filter.iter_mut().for_each(&mut blank);
    trimmed.having.iter_mut().for_each(&mut blank);
    trimmed.group_by.iter_mut().for_each(&mut blank);
    trimmed.limit.iter_mut().for_each(&mut blank);
    trimmed.offset.iter_mut().for_each(&mut blank);
    for column in &mut trimmed.columns {
        blank(&mut column.expr);
    }
    for term in &mut trimmed.order_by {
        blank(&mut term.expr);
    }
    for call in &mut trimmed.aggregates {
        call.arguments.iter_mut().for_each(&mut blank);
        call.filter.iter_mut().for_each(&mut blank);
        for term in &mut call.order_by {
            blank(&mut term.expr);
        }
    }
    trimmed.columns_read(id)
}

/// Replaces every occurrence of a computed key inside an expression with a constant.
///
/// @param expr - the expression being rewritten in place
/// @param keys - the bound expressions the index computes
fn blank_computed(expr: &mut BoundExpr, keys: &[BoundExpr]) {
    if keys.iter().any(|key| key == expr) {
        *expr = BoundExpr::Integer(0);
        return;
    }
    for child in expr.children_mut() {
        blank_computed(child, keys);
    }
}

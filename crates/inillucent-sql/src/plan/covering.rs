//! Which columns of a query an index's entries hold.
//!
//! Invariant: **a path is covering only when every column the query reads sits in the entry**, so
//! a scan of the index never has to fetch the table row.

use super::{ColumnUse, IndexInfo, TableInfo, ROWID_ENTRY_SLOT};

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
        // The rowid alias is a column of the table and the *rowid* of the
        // entry, so it is covered whatever the index holds - but it is read
        // with `IdxRowid` rather than out of the entry's record, so it is not
        // in the list.
        if table.rowid_alias == Some(*slot) && !table.without_rowid {
            slots.push((*slot, ROWID_ENTRY_SLOT));
            continue;
        }
        let held = index
            .columns
            .iter()
            .position(|column| column.plain_column() == Some(*slot));
        // The primary key fields follow the index's own columns in a WITHOUT ROWID entry.
        let position = match held {
            Some(position) => position,
            None if table.without_rowid => {
                let offset = key.iter().position(|column| column == slot)?;
                index.columns.len().saturating_add(offset)
            }
            None => return None,
        };
        slots.push((*slot, position));
    }
    Some(slots)
}

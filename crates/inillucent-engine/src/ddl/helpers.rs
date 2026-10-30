//! Small helpers the DDL statements share: duplicate detection, identifier quoting, automatic index names and pragma columns.
//!
//! Invariant: **each function here is a pure function of its arguments.** None reads or writes the catalog, a tree or the log, so the DDL code that calls them can be read without following any state.

use inillucent_base::DbResult;
use inillucent_sql::catalog_view::{IndexInfo, TableInfo};

use crate::entries::EntrySet;

/// Refuses a `CREATE UNIQUE INDEX` whose entries are not unique.
///
/// The entries are already in key order, so a duplicate is two adjacent rows
/// agreeing on every key column but the rowid - which is the definition, and is
/// one comparison rather than a set.
///
/// @param rows - the entries, sorted
/// @param owner - the table, for the message
/// @param index - the index, for the message
/// @param key_columns - how wide the entry is, rowid included
pub(crate) fn refuse_duplicates(
    entries: &EntrySet,
    order: &[u32],
    owner: &TableInfo,
    index: &IndexInfo,
    key_columns: usize,
) -> DbResult<()> {
    let compared = key_columns.saturating_sub(1);
    for at in 0..order.len().saturating_sub(1) {
        if entries.shares_key_prefix(order, at, compared) {
            return Err(unique_violation(owner, index));
        }
    }
    Ok(())
}

pub(crate) use crate::index_sort::unique_violation;

/// Returns an identifier quoted the way SQL wants it.
///
/// Double quotes, with any inside doubled: a column called `odd"name` is legal
/// and a query built by pasting it in would not parse.
///
/// @param name - the identifier
pub(crate) fn quoted(name: &[u8]) -> String {
    let written = String::from_utf8_lossy(name).replace('"', "\"\"");
    format!("\"{written}\"")
}

/// Returns the new name of an automatic index when its table is renamed.
///
/// SQLite names them `sqlite_autoindex_<table>_<n>`, so the name has to follow
/// the table or the next `CREATE TABLE` of the old name would collide with it.
///
/// @param name - the index's current name
/// @param from - the table's old name
/// @param to - its new one
pub(crate) fn renamed_automatic(name: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let prefix = b"sqlite_autoindex_";
    let Some(rest) = name.strip_prefix(prefix.as_slice()) else {
        return name.to_vec();
    };
    let Some(suffix) = rest.strip_prefix(from) else {
        return name.to_vec();
    };
    let mut out = prefix.to_vec();
    out.extend_from_slice(to);
    out.extend_from_slice(suffix);
    out
}

/// Returns one column of a `pragma_*` function's declaration.
///
/// Everything about it is the default: a pragma's answer is untyped, so BLOB
/// affinity and BINARY collation are what SQLite gives it too.
///
/// @param name - the column's name
/// @param hidden - whether it is one of the two argument columns
pub(crate) fn pragma_column(name: &[u8], hidden: bool) -> inillucent_sql::catalog_view::ColumnInfo {
    inillucent_sql::catalog_view::ColumnInfo {
        name: name.to_vec(),
        folded: name.to_ascii_lowercase(),
        declared_type: Vec::new(),
        affinity: inillucent_value::Affinity::Blob,
        collation: Vec::new(),
        not_null: false,
        not_null_conflict: None,
        primary_key_conflict: None,
        default_sql: None,
        primary_key_position: None,
        hidden,
        generated: false,
        stored: true,
        generated_sql: None,
    }
}

//! Where each declared column of a full-text table sits in a stored content row.
//!
//! Invariant: **a declared column is found by name in an external content table and by
//! position in the module's own `%_content`.** A column the owner does not have reads as
//! NULL rather than as another column's value.

use super::ColumnSpec;

/// Returns where each declared column sits in a stored content row.
///
/// A row read out of a shadow table arrives as one value per stored column,
/// the rowid first. `%_content` is written by this module as `(id, c0, c1...)`,
/// so a declared column is one past its own number; an **external** content
/// table is somebody else's, so the columns are found by *name* - which is
/// what SQLite does too, and is why an external table has to declare columns
/// the index can recognise. A name that is not there reads as NULL rather than
/// as the wrong column.
///
/// @param external - the owner's name, when the rows are not this table's
/// @param columns - the declared columns of the full-text table
/// @param catalog - the schema the statement was compiled against
pub(super) fn content_offsets(
    external: Option<&[u8]>,
    columns: &[ColumnSpec],
    catalog: Option<&inillucent_sql::catalog_view::StaticCatalog>,
) -> Vec<usize> {
    let names: Vec<Vec<u8>> = columns.iter().map(|column| column.name.clone()).collect();
    content_offsets_named(external, &names, catalog)
}

/// The same, from the column names alone.
///
/// The cursor holds names rather than specifications, and resolving from a
/// third shape would be a third description of the same rule.
///
/// @param external - the owner's name, when the rows are not this table's
/// @param names - the declared column names of the full-text table
/// @param catalog - the schema the statement was compiled against
pub(super) fn content_offsets_named(
    external: Option<&[u8]>,
    names: &[Vec<u8>],
    catalog: Option<&inillucent_sql::catalog_view::StaticCatalog>,
) -> Vec<usize> {
    let Some(owner) = external else {
        return (0..names.len()).map(|at| at.saturating_add(1)).collect();
    };
    let found = catalog.and_then(|catalog| catalog.table_named(&owner.to_ascii_lowercase()));
    // **A view's rows are the declared columns in declared order** (see `owner_query_of`), so
    // a column's position in the row is its position in the declaration.
    if found.is_some_and(|table| table.kind == inillucent_sql::catalog_view::TableKind::View) {
        return (0..names.len()).collect();
    }
    names
        .iter()
        .map(|name| {
            let folded = name.to_ascii_lowercase();
            found
                .and_then(|table| {
                    table
                        .columns
                        .iter()
                        .position(|held| held.name.to_ascii_lowercase() == folded)
                })
                .unwrap_or(usize::MAX)
        })
        .collect()
}

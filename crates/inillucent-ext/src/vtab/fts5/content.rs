//! Where each declared column of a full-text table sits in a stored content row.
//!
//! Invariant: **a declared column is found by name in an external content table and by
//! position in the module's own `%_content`.** A column the owner does not have reads as
//! NULL rather than as another column's value.

use super::options::parse_options;
use super::options::ColumnSpec;
use inillucent_sql::vtab::ModuleArguments;

/// Returns the owner of an external content table and the query that reads its rows.
///
/// SQLite reads external content with a query over the named table or view**, so a
/// view works as an owner. The first column is the rowid, taken from `content_rowid=`
/// and `rowid` when none was written, and the rest are the columns this table declares,
/// in declared order. [`content_offsets_named`] reads a view's rows by those positions.
///
/// @param arguments - the arguments the table was created with
pub(super) fn owner_query_of(arguments: &ModuleArguments) -> Option<(Vec<u8>, String)> {
    let options = parse_options(&arguments.arguments).ok()?;
    let owner = options.content?;
    let quoted =
        |name: &[u8]| format!("\"{}\"", String::from_utf8_lossy(name).replace('"', "\"\""));
    let mut selected = vec![quoted(&options.content_rowid)];
    selected.extend(options.columns.iter().map(|column| quoted(&column.name)));
    let query = format!("SELECT {} FROM {}", selected.join(", "), quoted(&owner));
    Some((owner, query))
}

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
    // **A stored row is the rowid followed by every column that is not the rowid alias.** The
    // alias (`INTEGER PRIMARY KEY`) is the tree's key and is not repeated in the record, so a
    // column's place in the stored row is its declared position moved up one for the rowid and
    // down one when the alias is declared before it. Using the declared position alone read
    // every column one place early for a table with no alias, and the rowid as the first column.
    let alias = found.and_then(|table| table.rowid_alias).map(usize::from);
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
                .map(|position| match alias {
                    Some(alias) if alias == position => 0,
                    Some(alias) if alias < position => position,
                    _ => position.saturating_add(1),
                })
                .unwrap_or(usize::MAX)
        })
        .collect()
}

/// Returns where the column `content_rowid` names sits in a stored row, when it is an ordinary
/// column of the owner and not the owner's rowid.
///
/// **`content_rowid` may name any integer column**, and then the rowid of a row of the full-text
/// table is that column's value and not the rowid of the row in the owner. The owner's rowid and
/// its `INTEGER PRIMARY KEY` are the rowid itself and need no lookup, and a view's first column
/// is already the rowid.
///
/// @param external - the owner's name, when the rows are not this table's
/// @param content_rowid - the `content_rowid` option, `rowid` when none was written
/// @param catalog - the schema the statement was compiled against
pub(super) fn rowid_column_offset(
    external: Option<&[u8]>,
    content_rowid: &[u8],
    catalog: Option<&inillucent_sql::catalog_view::StaticCatalog>,
) -> Option<usize> {
    let owner = external?;
    let found = catalog?.table_named(&owner.to_ascii_lowercase())?;
    if found.kind == inillucent_sql::catalog_view::TableKind::View {
        return None;
    }
    let folded = content_rowid.to_ascii_lowercase();
    let position = found
        .columns
        .iter()
        .position(|held| held.name.to_ascii_lowercase() == folded)?;
    let alias = found.rowid_alias.map(usize::from);
    match alias {
        Some(alias) if alias == position => None,
        Some(alias) if alias < position => Some(position),
        _ => Some(position.saturating_add(1)),
    }
}

/// Reads every row of the owner that has an integer in the column `content_rowid` names, keyed
/// by that integer.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param suffix - the shadow the owner's rows are reached under
/// @param offset - where the column sits in a stored row
pub(super) fn rows_by_rowid_column(
    context: &mut super::Context<'_>,
    shadows: &crate::shadow::ShadowTables,
    suffix: &[u8],
    offset: usize,
) -> inillucent_base::DbResult<std::collections::HashMap<i64, Vec<inillucent_value::Value<'static>>>>
{
    let mut rows = std::collections::HashMap::new();
    shadows.scan(context, suffix, |_, values| {
        if let Some(inillucent_value::Value::Integer(rowid)) = values.get(offset) {
            rows.entry(*rowid).or_insert_with(|| values.to_vec());
        }
        Ok(true)
    })?;
    Ok(rows)
}

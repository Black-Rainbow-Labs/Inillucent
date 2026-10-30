//! What a bound directive does to which schema.
//!
//! Invariant: **a directive is judged from its bound form and not from its text.** The read only and query only checks ask here, so an `IF EXISTS` that does nothing is not a write.

use inillucent_sql::directive::Directive;

/// Returns which schema a directive is about, as the binder numbered them.
///
/// Zero - `main` - for every directive that names no database, which includes
/// the transaction-control statements, `ATTACH` and `DETACH` themselves, and
/// every `PRAGMA` this engine answers from the connection rather than from a
/// file.
///
/// @param directive - the bound statement
pub(super) fn schema_of(directive: &Directive) -> usize {
    match directive {
        Directive::CreateTable { database, .. }
        | Directive::CreateTableAsSelect { database, .. }
        | Directive::CreateVirtualTable { database, .. }
        | Directive::CreateView { database, .. }
        | Directive::CreateIndex { database, .. }
        | Directive::CreateTrigger { database, .. }
        | Directive::Drop { database, .. }
        | Directive::Alter { database, .. }
        | Directive::Reindex { database, .. }
        | Directive::Vacuum { database, .. }
        | Directive::Analyze { database, .. } => *database,
        // A pragma may name a database and mostly does not; the ones this
        // engine answers from the connection rather than from a file are about
        // `main` either way.
        Directive::Pragma { database, .. } => database.unwrap_or(crate::MAIN),
        _ => crate::MAIN,
    }
}

/// Returns the error a connection under `PRAGMA query_only` gets for a write.
///
/// The text is SQLite's own, which is what an application's error handling is
/// written against.
pub(crate) fn query_only_refusal() -> inillucent_base::error::DbError {
    inillucent_base::error::DbError::primary(inillucent_base::error::PrimaryCode::ReadOnly)
        .with_message("attempt to write a readonly database")
        .with_detail("attempt to write a readonly database")
}

/// Returns whether a directive writes to the database, for `PRAGMA query_only`.
///
/// Decided from the bound directive and not from the statement's first word,
/// because `CREATE TABLE IF NOT EXISTS` on a table that exists and `DROP TABLE
/// IF EXISTS` on one that does not are no ops that SQLite runs under
/// `query_only`. `VACUUM`, `ANALYZE` and `REINDEX` always write. A `PRAGMA` is
/// judged by the pragma itself, because most of them only read.
///
/// @param directive - the bound statement
pub(super) fn directive_writes(directive: &Directive) -> bool {
    match directive {
        Directive::CreateTable {
            if_not_exists,
            exists,
            ..
        }
        | Directive::CreateTableAsSelect {
            if_not_exists,
            exists,
            ..
        }
        | Directive::CreateVirtualTable {
            if_not_exists,
            exists,
            ..
        }
        | Directive::CreateView {
            if_not_exists,
            exists,
            ..
        }
        | Directive::CreateIndex {
            if_not_exists,
            exists,
            ..
        } => !(*if_not_exists && *exists),
        Directive::CreateTrigger { .. }
        | Directive::Alter { .. }
        | Directive::Reindex { .. }
        | Directive::Vacuum { .. }
        | Directive::Analyze { .. } => true,
        Directive::Drop {
            if_exists, exists, ..
        } => !(*if_exists && !*exists),
        _ => false,
    }
}

/// Returns whether a directive may run on a connection that opened the file read only.
///
/// **Only the temporary schema may be written, and `:memory:` attached.** `CREATE TEMP TABLE`
/// and the statements that change a temporary object are allowed, as they are under
/// `sqlite3 -readonly`. A change to `main` or to an attached file is refused, and so is the
/// attach of a file, which would open it for writing.
///
/// @param directive - the bound statement
pub(super) fn read_only_allows(directive: &Directive) -> bool {
    match directive {
        Directive::Attach { file, file_sql, .. } => {
            file_sql.is_none() && (file.is_empty() || file.as_slice() == b":memory:")
        }
        other => !directive_writes(other) || schema_of(other) == crate::TEMP,
    }
}

/// Returns whether a directive changes the schema the cookie describes.
///
/// The `CREATE`s, the `DROP`s and `ALTER`. A pragma, a transaction control or
/// an `ANALYZE` does not - `ANALYZE` writes statistics rather than a schema -
/// and neither does anything the engine answers from the connection.
///
/// @param directive - the bound statement
pub(super) fn schema_change(directive: &Directive) -> bool {
    matches!(
        directive,
        Directive::CreateTable { .. }
            | Directive::CreateTableAsSelect { .. }
            | Directive::CreateVirtualTable { .. }
            | Directive::CreateView { .. }
            | Directive::CreateIndex { .. }
            | Directive::CreateTrigger { .. }
            | Directive::Drop { .. }
            | Directive::Alter { .. }
    )
}

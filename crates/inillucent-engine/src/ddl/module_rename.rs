//! Renaming what a virtual table's module depends on.
//!
//! Invariant: **a module is reconnected after the rows that describe it
//! changed.** A module reads its arguments and the roots of its shadow tables
//! once, when it connects, so a rename that only rewrote the catalog left the
//! connection answering for a name that no longer existed.
//!
//! Two renames reach a module. `ALTER TABLE f RENAME TO g` on a virtual table
//! renames the table and its shadow tables. `ALTER TABLE p RENAME TO q` and
//! `RENAME COLUMN` on a table a vector index is built over rewrite the
//! `source=` and `source_column=` arguments of the index's store.

use inillucent_base::error::{refusal, statement_refusal};
use inillucent_base::DbResult;
use inillucent_catalog::paged::{ObjectKind, SchemaEntry};
use inillucent_sql::directive::AlterKind;

use super::*;
use crate::vtab::shadow_table_name;

/// A shadow table of a virtual table: the name it has now and the name it takes.
struct ShadowPair {
    /// The name as the catalog stores it.
    from: Vec<u8>,
    /// The name after the rename.
    to: Vec<u8>,
}

impl ImportedDatabase {
    /// Returns whether a table of one schema is a virtual table with a connected module.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    pub(super) fn is_connected_virtual_table(&self, at: usize, folded: &[u8]) -> bool {
        at == crate::MAIN
            && self.session_state.virtual_tables.contains_key(folded)
            && self
                .schema
                .tables
                .iter()
                .any(|held| held.database == at && held.folded == folded && held.module.is_some())
    }

    /// Lists the shadow tables a virtual table owns, with the names they take.
    ///
    /// **Only the tables named after the virtual table are renamed.** A module
    /// may be handed a table that belongs to the application (an FTS5 index over
    /// an external content table is given that table under an empty suffix), and
    /// renaming the index must leave it alone. `drop_module_table` makes the same
    /// cut, by name.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the virtual table's folded name
    /// @param table - the virtual table's name as stored
    /// @param to - the name it is being given
    fn shadow_pairs(&self, at: usize, folded: &[u8], table: &[u8], to: &[u8]) -> Vec<ShadowPair> {
        let Some(connected) = self.session_state.virtual_tables.get(folded) else {
            return Vec::new();
        };
        let mut pairs = Vec::new();
        for shadow in connected
            .arguments
            .shadows
            .iter()
            .filter(|shadow| !shadow.suffix.is_empty())
        {
            let wanted = shadow_table_name(table, &shadow.suffix).to_ascii_lowercase();
            let stored = self.entries_of(at).iter().find(|held| {
                held.entry.kind == ObjectKind::Table
                    && held.entry.name.to_ascii_lowercase() == wanted
            });
            if let Some(held) = stored {
                pairs.push(ShadowPair {
                    from: held.entry.name.clone(),
                    to: shadow_table_name(to, &shadow.suffix),
                });
            }
        }
        pairs
    }

    /// Refuses a rename that would take the name of a table or index that exists.
    ///
    /// @param at - the schema the table is in
    /// @param pairs - the shadow tables and the names they would take
    fn refuse_taken_shadow_names(&self, at: usize, pairs: &[ShadowPair]) -> DbResult<()> {
        for pair in pairs {
            let wanted = pair.to.to_ascii_lowercase();
            let leaving = pair.from.to_ascii_lowercase();
            let taken = self.entries_of(at).iter().any(|held| {
                let name = held.entry.name.to_ascii_lowercase();
                name == wanted && name != leaving
            });
            if taken {
                return Err(statement_refusal(format!(
                    "there is already another table or index with this name: {}",
                    String::from_utf8_lossy(&pair.to)
                )));
            }
        }
        Ok(())
    }

    /// Renames one table's schema rows, the way `ALTER TABLE ... RENAME TO` does.
    ///
    /// @param at - the schema the table is in
    /// @param source - the statement text
    /// @param from - the table's name as stored
    /// @param to - the new name
    fn rename_table_rows(
        &mut self,
        at: usize,
        source: &[u8],
        from: &[u8],
        to: &[u8],
    ) -> DbResult<()> {
        let action = AlterKind::RenameTable { to: to.to_vec() };
        let folded = from.to_ascii_lowercase();
        let updates = self.altered_entries(at, source, from, &folded, &action)?;
        for (rowid, entry) in updates {
            self.rewrite(rowid, entry)?;
        }
        Ok(())
    }

    /// Renames a virtual table and the shadow tables its module keeps.
    ///
    /// **`ALTER TABLE f RENAME TO g` on an FTS5 table failed with `cannot alter f:
    /// not a table`.** SQLite asks the module to rename its storage, and every
    /// module here names its shadow tables `<table>_<suffix>`, so the shadow
    /// tables are renamed with the table and the module is connected again under
    /// the new name. The roots of the shadow trees do not move, so the rows the
    /// module wrote are the rows it reads.
    ///
    /// @param source - the statement text
    /// @param at - the schema the table is in
    /// @param table - the table's name as written
    /// @param folded - the table's folded name
    /// @param to - the new name
    pub(super) fn rename_virtual_table(
        &mut self,
        source: &[u8],
        at: usize,
        table: &[u8],
        folded: &[u8],
        to: &[u8],
    ) -> DbResult<Outcome> {
        let action = AlterKind::RenameTable { to: to.to_vec() };
        let pairs = self.shadow_pairs(at, folded, table, to);
        self.refuse_taken_shadow_names(at, &pairs)?;
        self.check_schema_before(at, &action)?;
        // The module's buffered writes reach its trees before the connection
        // that holds them is replaced.
        self.sync_modules()?;
        self.rename_table_rows(at, source, table, to)?;
        for pair in &pairs {
            self.rename_table_rows(at, source, &pair.from, &pair.to)?;
        }
        self.rewrite_temp_dependents(at, source, table, folded, &action)?;
        self.session_state.virtual_tables.remove(folded);
        self.reconnect_after_rename()?;
        self.seal()?;
        Ok(Outcome::empty())
    }

    /// Connects the modules that lost their connection, and derives the schema again.
    ///
    /// The tables are derived once before the module connects, because it finds
    /// its shadow trees through the catalog, and once after, because a virtual
    /// table's columns are the module's answer.
    fn reconnect_after_rename(&mut self) -> DbResult<()> {
        self.rebuild_tables()?;
        self.connect_missing()?;
        self.rebuild_tables()?;
        self.refresh_catalog();
        self.refresh_vector_indexes();
        Ok(())
    }

    /// Returns the rows of vector index stores that name a table or column being renamed.
    ///
    /// **A rename left the index pointing at a table that no longer existed.** The
    /// store's definition is `CREATE VIRTUAL TABLE pv USING inillucent_search(body,
    /// dims=3, source=p, source_column=v)`, and the engine finds the indexes of a
    /// table by reading `source=` back out of it. After `RENAME TO q` nothing was
    /// named `p`, so inserts into `q` no longer reached the index and `DROP INDEX
    /// pv` reported `no such index`.
    ///
    /// @param at - the schema the renamed table is in
    /// @param folded - the table's folded name
    /// @param action - what the statement does
    pub(super) fn store_argument_updates(
        &self,
        at: usize,
        folded: &[u8],
        action: &AlterKind,
    ) -> Vec<(i64, SchemaEntry)> {
        if at != crate::MAIN {
            return Vec::new();
        }
        let mut updates = Vec::new();
        for held in self.entries_of(at) {
            if held.entry.kind != ObjectKind::Table {
                continue;
            }
            let Some(sql) = renamed_store_text(&held.entry.sql, folded, action) else {
                continue;
            };
            let mut moved = held.entry.clone();
            moved.sql = sql;
            updates.push((held.rowid, moved));
        }
        updates
    }

    /// Writes the rewritten store rows and connects those stores again.
    ///
    /// @param updates - the rows `store_argument_updates` returned
    pub(super) fn write_store_updates(&mut self, updates: Vec<(i64, SchemaEntry)>) -> DbResult<()> {
        if updates.is_empty() {
            return Ok(());
        }
        self.sync_modules()?;
        for (rowid, entry) in updates {
            let folded = entry.name.to_ascii_lowercase();
            self.rewrite(rowid, entry)?;
            self.session_state.virtual_tables.remove(&folded);
        }
        self.rebuild_tables()?;
        self.connect_missing()?;
        Ok(())
    }
}

/// Returns the store's definition with the renamed table or column written into it.
///
/// `None` when the definition is not a store over the renamed table, or the
/// rename does not change an argument it carries.
///
/// @param sql - a catalog row's `CREATE` text
/// @param folded - the folded name of the table being renamed
/// @param action - what the statement does
fn renamed_store_text(sql: &[u8], folded: &[u8], action: &AlterKind) -> Option<Vec<u8>> {
    if !sql
        .windows(7)
        .any(|word| word.eq_ignore_ascii_case(b"virtual"))
    {
        return None;
    }
    let source = argument_value(sql, b"source")?;
    if source.to_ascii_lowercase() != folded {
        return None;
    }
    match action {
        AlterKind::RenameTable { to } => replace_argument(sql, b"source", to),
        AlterKind::RenameColumn { from, to, .. } => {
            let held = argument_value(sql, b"source_column")?;
            if held.to_ascii_lowercase() != from.to_ascii_lowercase() {
                return None;
            }
            replace_argument(sql, b"source_column", to)
        }
        _ => None,
    }
}

/// Finds where `key=value` sits in a store's definition.
///
/// The text is the one the engine wrote, so a key is preceded by the opening
/// parenthesis, a comma or a space and its value runs to the next comma or the
/// closing parenthesis.
///
/// @param sql - the `CREATE VIRTUAL TABLE` text
/// @param key - the argument's name
fn argument_span(sql: &[u8], key: &[u8]) -> Option<(usize, usize)> {
    let mut at = 0usize;
    while at < sql.len() {
        let rest = sql.get(at..)?;
        let found = rest
            .windows(key.len().saturating_add(1))
            .position(|window| {
                window
                    .get(..key.len())
                    .is_some_and(|name| name.eq_ignore_ascii_case(key))
                    && window.get(key.len()) == Some(&b'=')
            })?;
        let start = at.saturating_add(found);
        let before = start.checked_sub(1).and_then(|index| sql.get(index));
        let value_start = start.saturating_add(key.len()).saturating_add(1);
        if matches!(before, Some(b'(' | b',' | b' ')) {
            let tail = sql.get(value_start..)?;
            let length = tail
                .iter()
                .position(|byte| matches!(byte, b',' | b')'))
                .unwrap_or(tail.len());
            return Some((value_start, value_start.saturating_add(length)));
        }
        at = start.saturating_add(1);
    }
    None
}

/// Returns an argument's value, trimmed.
///
/// @param sql - the `CREATE VIRTUAL TABLE` text
/// @param key - the argument's name
fn argument_value(sql: &[u8], key: &[u8]) -> Option<Vec<u8>> {
    let (start, end) = argument_span(sql, key)?;
    let text = String::from_utf8_lossy(sql.get(start..end)?);
    Some(text.trim().as_bytes().to_vec())
}

/// Returns the definition with one argument's value replaced.
///
/// @param sql - the `CREATE VIRTUAL TABLE` text
/// @param key - the argument's name
/// @param value - what to write after the `=`
fn replace_argument(sql: &[u8], key: &[u8], value: &[u8]) -> Option<Vec<u8>> {
    let (start, end) = argument_span(sql, key)?;
    let mut out = sql.get(..start)?.to_vec();
    out.extend_from_slice(value);
    out.extend_from_slice(sql.get(end..)?);
    Some(out)
}

/// Builds the refusal for a rename of a virtual table in a schema other than `main`.
///
/// @param table - the table's name as written
pub(super) fn virtual_table_elsewhere(table: &[u8]) -> inillucent_base::DbError {
    refusal(format!(
        "virtual table {} may only be renamed in main",
        String::from_utf8_lossy(table)
    ))
}

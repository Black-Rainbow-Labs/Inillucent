//! Re-deriving the one table a statement changed, instead of every table.
//!
//! Invariant: **the table this puts in `tables` is the one `rebuild_tables` would have put
//! there.** It is derived by `tables_from_entries` from the catalog rows that belong to the table
//! (its own row, its indexes and its triggers), and it answers `false`, leaving the full rebuild
//! to run, whenever anything outside those rows could change what a rebuild produces.
//!
//! **Why.** `rebuild_tables` parses the `CREATE` text of every table in the catalog, so each
//! `CREATE INDEX`, `CREATE VIEW` and `CREATE TRIGGER` cost time in proportion to the tables that
//! already existed. 800 tables with an index each in one transaction took 4.0 s against SQLite's
//! 0.17 s, 800 with a view and a trigger as well took 6.0 s against 0.28 s, and 3,000 took 162 s.

use inillucent_catalog::paged::{tables_from_entries, ObjectKind, SchemaEntry};

use super::*;

impl crate::ImportedDatabase {
    /// Re-derives the table an index or trigger was just created on and puts it where it already
    /// was, and reports whether it did.
    ///
    /// It answers `false`, and the caller rebuilds every table, when the connection holds
    /// something a rebuild arranges and this does not: an imposter, a temporary trigger, a
    /// virtual table, an index withdrawn as unusable, or measurements `ANALYZE` read onto the
    /// table, which a derived table would not carry until they are read again.
    ///
    /// @param name - the table's name
    pub(crate) fn replace_table_named(&mut self, name: &[u8]) -> DbResult<bool> {
        let at = self.schema.ddl_schema;
        let folded = name.to_ascii_lowercase();
        if !self.rebuild_would_match(at) {
            return Ok(false);
        }
        let Some(position) = self
            .schema
            .tables
            .iter()
            .position(|held| held.database == at && held.folded == folded)
        else {
            return Ok(false);
        };
        let measured = self.schema.tables.get(position).is_some_and(|held| {
            held.analysed_rows.is_some()
                || held
                    .indexes
                    .iter()
                    .any(|index| !index.prefix_rows.is_empty())
        });
        if measured || !self.schema.repairs.unusable_indexes.is_empty() {
            return Ok(false);
        }
        let (entries, roots) = self.rows_of_table(at, &folded);
        let mut derived = tables_from_entries(&entries, &roots, at)?;
        if derived.len() != 1 {
            return Ok(false);
        }
        let Some(table) = derived.pop() else {
            return Ok(false);
        };
        if let Some(slot) = self.schema.tables.get_mut(position) {
            *slot = table;
        }
        self.schema.appended = Some((
            position,
            self.schema.tables.len(),
            self.schema.catalog_generation,
            true,
        ));
        Ok(true)
    }

    /// Adds the view just recorded to the connection's table list without deriving every table
    /// again, and reports whether it did.
    ///
    /// The view goes after the last table of its database, which is where a full rebuild puts a
    /// row recorded last. It answers `false` when the database holds no table yet, or when
    /// `replace_table_named` would have answered `false` for the same reasons.
    pub(crate) fn append_recorded_view(&mut self) -> DbResult<bool> {
        let at = self.schema.ddl_schema;
        if !self.rebuild_would_match(at) {
            return Ok(false);
        }
        let Some(last) = self
            .schema
            .tables
            .iter()
            .rposition(|held| held.database == at)
        else {
            return Ok(false);
        };
        let Some(row) = self
            .entries_of(at)
            .last()
            .filter(|row| row.entry.kind == ObjectKind::View)
        else {
            return Ok(false);
        };
        let (entry, root) = (row.entry.clone(), row.root);
        let mut derived = tables_from_entries(&[entry], &[root], at)?;
        if derived.len() != 1 {
            return Ok(false);
        }
        let Some(view) = derived.pop() else {
            return Ok(false);
        };
        let position = last.saturating_add(1);
        self.schema.tables.insert(position, view);
        self.schema.appended = Some((
            position,
            self.schema.tables.len(),
            self.schema.catalog_generation,
            false,
        ));
        Ok(true)
    }

    /// Reports whether a one table derivation arranges what a full rebuild would.
    ///
    /// A full rebuild also attaches temporary triggers to the tables they fire on, connects the
    /// modules of virtual tables and puts imposters back, and none of that is done for one table.
    /// A statement in the temporary database always takes the full rebuild.
    ///
    /// @param at - the schema the statement writes to
    fn rebuild_would_match(&self, at: usize) -> bool {
        let has_temp_trigger = self
            .entries_of(crate::TEMP)
            .iter()
            .any(|row| row.entry.kind == ObjectKind::Trigger);
        at != crate::TEMP
            && !has_temp_trigger
            && self.schema.imposters.is_empty()
            && self.session_state.virtual_tables.is_empty()
    }

    /// Returns the catalog rows that describe one table, and the identifier of each one's tree.
    ///
    /// The table's own row, its indexes and its triggers, in the order the catalog holds them,
    /// which is the order `tables_from_entries` is given them by a full rebuild.
    ///
    /// @param at - the schema the table is in
    /// @param folded - the table's folded name
    fn rows_of_table(&self, at: usize, folded: &[u8]) -> (Vec<SchemaEntry>, Vec<u32>) {
        let mut entries = Vec::new();
        let mut roots = Vec::new();
        for row in self.entries_of(at) {
            let belongs = match row.entry.kind {
                ObjectKind::Table => row.entry.name.eq_ignore_ascii_case(folded),
                ObjectKind::Index | ObjectKind::Trigger => {
                    row.entry.table.eq_ignore_ascii_case(folded)
                }
                _ => false,
            };
            if belongs {
                entries.push(row.entry.clone());
                roots.push(row.root);
            }
        }
        (entries, roots)
    }
}

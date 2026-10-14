//! Foreign keys left pointing at a table that `DROP TABLE` removed.
//!
//! Invariant: **a deferred key whose parent was dropped fails the commit unless
//! the child rows are gone or the parent's rows were put back.** SQLite counts a
//! violation when the implicit `DELETE` of `DROP TABLE` orphans a child row, and
//! the count only falls when a child row goes or a parent row arrives. This
//! engine asks the question at the commit instead of counting (see
//! `engine::keys`), and the question has nothing to read once the parent table is
//! gone, so the drop is remembered here.

use inillucent_base::error::DbError;
use inillucent_base::DbResult;
use inillucent_sql::catalog_view::{TableInfo, TableKind};

use super::*;

/// A parent table dropped while child rows still named it.
#[derive(Clone, Debug)]
pub(crate) struct DroppedParent {
    /// The schema the table was in.
    pub(crate) database: usize,
    /// The table's folded name.
    pub(crate) folded: Vec<u8>,
    /// The handle of the tree the dropped table had, which tells the original
    /// table from a later one of the same name.
    pub(crate) root: u32,
    /// Whether a `CREATE TABLE` of the same name has run since the drop.
    ///
    /// A table of that name that was not created after the drop arrived by
    /// `ALTER TABLE ... RENAME TO`, as in the table rebuild that Django and Rails
    /// make. Its rows were copied before the drop, so SQLite's count was never
    /// lowered, and the commit fails although every child row has a parent.
    pub(crate) recreated: bool,
}

/// Builds the error a violated foreign key gives.
fn foreign_key_failed() -> DbError {
    DbError::new(inillucent_base::ExtendedCode(
        inillucent_sql::dml::codes::FOREIGN_KEY,
    ))
    .with_message("FOREIGN KEY constraint failed")
    .with_detail("FOREIGN KEY constraint failed")
}

impl ImportedDatabase {
    /// Remembers a table that is about to be dropped when child rows still name it.
    ///
    /// Called after the implicit `DELETE`, so a child row that cascaded, was set
    /// to NULL, or broke an immediate key (which refuses the drop) is not counted
    /// here. What remains is a row of a deferred key with no parent.
    ///
    /// @param owner - the table about to be dropped
    pub(super) fn note_orphans_of_dropped_parent(&mut self, owner: &TableInfo) -> DbResult<()> {
        if !self.pragmas.foreign_keys() || owner.kind != TableKind::Table || owner.module.is_some()
        {
            return Ok(());
        }
        let record = DroppedParent {
            database: owner.database,
            folded: owner.folded.clone(),
            root: owner.root,
            recreated: false,
        };
        for sql in self.dropped_parent_queries(&record, None, true) {
            if !self.query_internally(&sql)?.is_empty() {
                self.session_state.dropped_parents.borrow_mut().push(record);
                break;
            }
        }
        Ok(())
    }

    /// Records that a table of a name has been created, after one of that name was dropped.
    ///
    /// @param at - the schema the table is created in
    /// @param name - the table's name
    pub(super) fn note_parent_recreated(&self, at: usize, name: &[u8]) {
        let folded = name.to_ascii_lowercase();
        for record in self.session_state.dropped_parents.borrow_mut().iter_mut() {
            if record.database == at && record.folded == folded {
                record.recreated = true;
            }
        }
    }

    /// Fails a `DROP TABLE` that is its own transaction when it orphaned a deferred key.
    ///
    /// Inside a transaction the commit checks; a statement outside one is its own
    /// transaction, so the check has to run here. The records are dropped either
    /// way, because nothing after this statement could use them.
    pub(super) fn refuse_commit_with_dropped_parents(&mut self) -> DbResult<()> {
        if self.writing.batch().is_some() {
            return Ok(());
        }
        let checked = self.check_dropped_parents();
        self.forget_dropped_parents();
        checked
    }

    /// Checks every parent table the transaction dropped, and fails if a deferred key is broken by one.
    pub(crate) fn check_dropped_parents(&mut self) -> DbResult<()> {
        let records = self.session_state.dropped_parents.borrow().clone();
        let every_key = self.pragmas.defer_foreign_keys();
        for record in &records {
            let current = self.dropped_parent_table_now(record);
            for sql in self.dropped_parent_queries(record, current.as_ref(), every_key) {
                if !self.query_internally(&sql)?.is_empty() {
                    return Err(foreign_key_failed());
                }
            }
        }
        Ok(())
    }

    /// Forgets the dropped parents, when the transaction ends.
    pub(crate) fn forget_dropped_parents(&self) {
        self.session_state.dropped_parents.borrow_mut().clear();
    }

    /// Returns the table now holding the dropped table's name, unless it is the dropped table itself.
    ///
    /// A `ROLLBACK TO` that undid the drop puts the original table back, and the
    /// record is then about nothing.
    ///
    /// @param record - the dropped parent
    fn dropped_parent_table_now(&self, record: &DroppedParent) -> Option<TableInfo> {
        self.schema
            .tables
            .iter()
            .find(|held| held.database == record.database && held.folded == record.folded)
            .cloned()
    }

    /// Returns the queries whose rows are the child rows a dropped parent left broken.
    ///
    /// With the parent missing, or present but not created after the drop, every
    /// child row with a key value counts. With a parent created after the drop,
    /// the ordinary question is asked: a row counts when the new table has no row
    /// for it.
    ///
    /// @param record - the dropped parent
    /// @param current - the table that now has the parent's name, when there is one
    /// @param every_key - whether `PRAGMA defer_foreign_keys` makes every key deferred
    fn dropped_parent_queries(
        &self,
        record: &DroppedParent,
        current: Option<&TableInfo>,
        every_key: bool,
    ) -> Vec<String> {
        if current.is_some_and(|held| held.root == record.root) {
            return Vec::new();
        }
        let database = self.schema_label(record.database);
        let mut queries = Vec::new();
        for child in &self.schema.tables {
            if child.database != record.database
                || child.kind != TableKind::Table
                || child.folded == record.folded
            {
                continue;
            }
            for key in &child.foreign_keys {
                if key.parent_folded != record.folded || !(key.is_deferred() || every_key) {
                    continue;
                }
                let sql = match current {
                    Some(parent) if record.recreated => {
                        inillucent_sql::foreign_key::violation_query(child, parent, key, &database)
                    }
                    _ => inillucent_sql::foreign_key::orphan_query(child, key, &database),
                };
                queries.extend(sql);
            }
        }
        queries
    }
}

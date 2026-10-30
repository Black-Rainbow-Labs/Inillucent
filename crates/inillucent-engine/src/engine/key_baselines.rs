//! Counting only the foreign key violations a transaction makes.
//!
//! Invariant: **a deferred key fails a commit only when the transaction left it
//! with more violations than it had before the transaction wrote a table the key
//! involves.** SQLite counts the violations the current transaction creates, so a
//! row that already broke a key (written while `foreign_keys` was off) never fails
//! a later commit. The engine still asks the question at the commit rather than
//! keeping a counter (see `engine::keys`), so each key's violation count is asked
//! once, before the first statement of the transaction that can write a table the
//! key involves, and the commit compares against that number.
//!
//! A key whose child and parent the transaction never wrote has no baseline and is
//! not asked at all at the commit. That also removes a full scan of every deferred
//! key from each commit of a statement that writes an unrelated table.

use crate::*;
use std::collections::HashMap;

/// Identifies one foreign key: its database, its child table's folded name, and its number.
pub(crate) type KeyId = (usize, Vec<u8>, u16);

/// The violation count of each key, as the transaction found it.
pub(crate) type KeyBaselines = HashMap<KeyId, usize>;

/// The tables a statement can write, as far as the baseline needs to know.
enum Written {
    /// The statement writes no table a key could involve.
    Nothing,
    /// The statement writes this table directly.
    Table(std::rc::Rc<inillucent_sql::catalog_view::TableInfo>),
    /// The statement may write any table: schema changes, and a table whose triggers or
    /// foreign key actions write others.
    Everything,
}

impl Written {
    /// Works out what a compiled statement can write.
    ///
    /// A table with a trigger, or one a key's action runs from, can write any table, so it
    /// counts as everything. That over-asks and never under-asks, which is the safe side:
    /// an extra baseline costs one query, a missing one lets a new violation through.
    ///
    /// @param cached - the compiled statement
    fn of(cached: &Cached) -> Written {
        match cached {
            Cached::Insert(statement, ..) => Written::of_table(&statement.table),
            Cached::Update(statement, ..) => Written::of_table(&statement.table),
            Cached::Delete(statement, ..) => Written::of_table(&statement.table),
            Cached::Ddl(sql) if changes_schema(sql) => Written::Everything,
            _ => Written::Nothing,
        }
    }

    /// Classifies a write to one table.
    ///
    /// @param table - the table the statement writes
    fn of_table(table: &std::rc::Rc<inillucent_sql::catalog_view::TableInfo>) -> Written {
        let runs_other_writes = !table.triggers.is_empty()
            || table
                .foreign_key_triggers
                .iter()
                .any(|trigger| !trigger.is_check);
        match runs_other_writes {
            true => Written::Everything,
            false => Written::Table(table.clone()),
        }
    }

    /// Reports whether writing this could change a key's violation count.
    ///
    /// @param child - the table declaring the key
    /// @param key - the foreign key
    fn involves(
        &self,
        child: &inillucent_sql::catalog_view::TableInfo,
        key: &inillucent_sql::catalog_view::ForeignKeyInfo,
    ) -> bool {
        match self {
            Written::Nothing => false,
            Written::Everything => true,
            Written::Table(table) => {
                table.database == child.database
                    && (table.folded == child.folded || table.folded == key.parent_folded)
            }
        }
    }
}

/// Reports whether a statement the session carries out itself changes the schema.
///
/// `COMMIT`, `ROLLBACK`, `BEGIN`, `SAVEPOINT` and `PRAGMA` must not take a baseline: one
/// taken at the `COMMIT` would hide the violations the transaction made. Anything that
/// cannot be told from the text is counted as a schema change, which only costs queries.
///
/// @param sql - the statement's text
fn changes_schema(sql: &str) -> bool {
    let mut rest = sql.trim_start();
    loop {
        if let Some(after) = rest.strip_prefix("--") {
            rest = after
                .split_once('\n')
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after
                .split_once("*/")
                .map_or("", |(_, tail)| tail)
                .trim_start();
        } else {
            break;
        }
    }
    let word: String = rest
        .chars()
        .take_while(|character| character.is_ascii_alphabetic())
        .collect::<String>()
        .to_ascii_lowercase();
    matches!(
        word.as_str(),
        "create" | "drop" | "alter" | "replace" | "with" | "insert" | "update" | "delete"
    )
}

impl ImportedDatabase {
    /// Records the violation count of every deferred key a statement can change, once per transaction.
    ///
    /// Called before the outermost statement runs, with the file locked, so the count is of
    /// the state the transaction started this key from. A key already recorded keeps its
    /// number: the first write is the one that has to be measured against.
    ///
    /// @param cached - the statement about to run
    pub(crate) fn record_key_baselines(&mut self, cached: &Cached) -> DbResult<()> {
        if !self.pragmas.foreign_keys() {
            return Ok(());
        }
        let written = Written::of(cached);
        if matches!(written, Written::Nothing) {
            return Ok(());
        }
        let every_key = self.pragmas.defer_foreign_keys();
        for (id, sql) in self.unrecorded_keys(&written, every_key) {
            let count = match sql {
                Some(sql) => self.count_violations(&sql)?,
                None => 0,
            };
            self.session_state
                .key_baselines
                .borrow_mut()
                .insert(id, count);
        }
        Ok(())
    }

    /// Lists the keys a write can change that have no baseline yet, with the query that counts each.
    ///
    /// The query is `None` for a key whose parent table does not exist, which has no
    /// violations to count.
    ///
    /// @param written - what the statement can write
    /// @param every_key - whether `PRAGMA defer_foreign_keys` makes every key deferred
    fn unrecorded_keys(&self, written: &Written, every_key: bool) -> Vec<(KeyId, Option<String>)> {
        let recorded = self.session_state.key_baselines.borrow();
        let mut wanted = Vec::new();
        for child in &self.schema.tables {
            if child.kind != inillucent_sql::catalog_view::TableKind::Table
                || child.folded.starts_with(b"sqlite_")
            {
                continue;
            }
            for key in &child.foreign_keys {
                let id = (
                    child.database,
                    child.folded.clone(),
                    u16::try_from(key.id).unwrap_or_default(),
                );
                if !(key.is_deferred() || every_key)
                    || !written.involves(child, key)
                    || recorded.contains_key(&id)
                {
                    continue;
                }
                let parent = self.schema.tables.iter().find(|candidate| {
                    candidate.folded == key.parent_folded && candidate.database == child.database
                });
                let sql = parent.and_then(|parent| {
                    let database = self.schema_label(child.database);
                    inillucent_sql::foreign_key::violation_query(child, parent, key, &database)
                });
                wanted.push((id, sql));
            }
        }
        wanted
    }

    /// Counts the rows a violation query finds.
    ///
    /// @param sql - a key's violation query
    pub(crate) fn count_violations(&mut self, sql: &str) -> DbResult<usize> {
        let rows = self.query_internally(&format!("SELECT count(*) FROM ({sql})"))?;
        let count = match rows.first().and_then(|row| row.first()) {
            Some(OwnedDatum::Int(count)) => *count,
            _ => 0,
        };
        Ok(usize::try_from(count).unwrap_or_default())
    }

    /// Runs a statement, after measuring the keys it can change when it is the outermost one.
    ///
    /// **Measured before the statement writes anything**, and inside the nesting count so the
    /// queries it runs are not taken for outermost statements.
    ///
    /// @param cached - the compiled statement
    /// @param params - the bound parameters
    /// @param outermost - whether no other statement is running
    pub(crate) fn apply_measuring_keys(
        &mut self,
        cached: &std::rc::Rc<Cached>,
        params: &Params,
        outermost: bool,
    ) -> DbResult<Outcome> {
        if outermost {
            self.record_key_baselines(cached)?;
        }
        self.apply_compiled(cached, params)
    }

    /// Clears what an autocommit statement kept for its own transaction.
    ///
    /// The key baselines end with the statement, and so does `PRAGMA defer_foreign_keys` after
    /// a statement that read a table.
    ///
    /// @param outermost - whether no other statement was running
    /// @param reads - whether the statement opened a transaction by reading a table
    pub(crate) fn end_statement_state(&self, outermost: bool, reads: bool) {
        if self.writing.batch().is_some() {
            return;
        }
        if outermost {
            self.clear_key_baselines();
        }
        if reads {
            self.clear_defer_foreign_keys();
        }
    }

    /// Forgets the baselines, when the transaction ends.
    pub(crate) fn clear_key_baselines(&self) {
        self.session_state.key_baselines.borrow_mut().clear();
    }
}

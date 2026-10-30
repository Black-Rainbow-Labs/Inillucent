//! What a record from a SQLite file reads as for a column added after it was written.
//!
//! Invariant: **a record with fewer fields than its table declares reads, for each
//! missing field, as that column's `DEFAULT` with the column's affinity applied.**
//! SQLite does the same on every read. `ALTER TABLE ... ADD COLUMN` rewrites no
//! row, so the rows that were there keep their old, shorter records and the new
//! column's default is supplied when they are read. A migration that copied the
//! short records field by field wrote NULL for every one of those values, into a
//! `NOT NULL` column as well.

use std::collections::HashMap;
use std::path::PathBuf;

use inillucent_base::DbResult;
use inillucent_sql::catalog_view::TableInfo;
use inillucent_tree::datum::OwnedDatum;

use crate::*;

/// Supplies the value of a record field that a short record does not hold.
///
/// One of these is made per table. It answers by the field's position in the
/// record, which is the position the reader counts, and it remembers every answer
/// so a table of a million short rows evaluates each default once.
pub struct RecordFills<'a> {
    /// The table the records belong to.
    info: &'a TableInfo,
    /// The declared column each record field holds, indexed by record field.
    declared: Vec<usize>,
    /// The value already worked out for each record field.
    known: HashMap<usize, OwnedDatum>,
    /// An empty database in memory, made the first time a default has to be run.
    scratch: Option<ImportedDatabase>,
}

impl<'a> RecordFills<'a> {
    /// Starts the fills for one table's records.
    ///
    /// @param info - the table, as the catalog loader parsed its declaration
    pub fn new(info: &'a TableInfo) -> RecordFills<'a> {
        RecordFills {
            info,
            declared: record_declared_positions(info),
            known: HashMap::new(),
            scratch: None,
        }
    }

    /// Returns the value of a record field that the record being read does not have.
    ///
    /// **Only a column that has a default can have been added after the row was
    /// written**, so a column with none, and a field that holds a `PRIMARY KEY`
    /// alias, read as NULL.
    ///
    /// @param field - the position of the field in the record
    pub fn fill(&mut self, field: usize) -> DbResult<OwnedDatum> {
        if let Some(value) = self.known.get(&field) {
            return Ok(value.clone());
        }
        let value = self.work_out(field)?;
        self.known.insert(field, value.clone());
        Ok(value)
    }

    /// Works out one field's fill from the column's declaration.
    ///
    /// @param field - the position of the field in the record
    fn work_out(&mut self, field: usize) -> DbResult<OwnedDatum> {
        let Some(column) = self
            .declared
            .get(field)
            .and_then(|declared| self.info.columns.get(*declared))
        else {
            return Ok(OwnedDatum::Null);
        };
        let Some(default_sql) = column.default_sql.clone() else {
            return Ok(OwnedDatum::Null);
        };
        let raw = match crate::ddl::pads::literal_default(&default_sql) {
            Some(value) => value,
            None => self.run_default(&default_sql)?,
        };
        Ok(
            crate::ddl::pads::stored_default(column, self.info.strict, raw)
                .unwrap_or(OwnedDatum::Null),
        )
    }

    /// Evaluates a default that is not a plain literal, such as `TRUE` or `-'7'`.
    ///
    /// SQLite accepts only constants after `ADD COLUMN`, so the expression has no
    /// column to read and the evaluation needs no table. It runs in an empty
    /// database of its own, through the engine's own expression evaluator, so the
    /// answer is the one an `INSERT` into the column would have stored.
    ///
    /// @param default_sql - the `DEFAULT` text as the declaration wrote it
    fn run_default(&mut self, default_sql: &[u8]) -> DbResult<OwnedDatum> {
        if self.scratch.is_none() {
            let vfs: std::sync::Arc<dyn inillucent_vfs::Vfs> =
                std::sync::Arc::new(inillucent_vfs::MemoryVfs::new());
            self.scratch = Some(ImportedDatabase::create_on(
                vfs,
                PathBuf::from(":memory:"),
                4096,
                64,
            )?);
        }
        match self.scratch.as_mut() {
            Some(database) => database.constant_default(default_sql),
            None => Ok(OwnedDatum::Null),
        }
    }
}

/// Returns the declared column each field of a table's stored record holds.
///
/// For a rowid table that is every stored column in declared order, the `INTEGER
/// PRIMARY KEY` alias included, because its field is in the record as a NULL. For
/// a `WITHOUT ROWID` table the primary key columns come first, in key order.
///
/// @param info - the table's declaration
pub fn record_declared_positions(info: &TableInfo) -> Vec<usize> {
    if !info.without_rowid {
        return stored_positions(info);
    }
    keyed_record_order(info)
        .map(|(order, _)| order)
        .unwrap_or_default()
}

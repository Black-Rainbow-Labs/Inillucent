//! Automatic indexes that releases 0.1.1 to 2.0.9 wrote with no value for a collated key column.
//!
//! Invariant: **an automatic index that holds NULL for a key column where its table holds a
//! value is never used to answer a query, and it is rebuilt before the first statement of a
//! connection that may write.** The check reads the index and the table; the repair is a
//! `REINDEX`.
//!
//! Those releases did not understand a `COLLATE` written inside a table level `UNIQUE` or
//! `PRIMARY KEY`. `UNIQUE (a, b COLLATE NOCASE)` made an automatic index whose key for `b` was
//! the expression `b COLLATE NOCASE` with no column behind it, so every entry holds NULL where
//! `b` should be. 2.1.0 reads the same declaration as a key on the column `b`, finds nothing
//! where it looks for `('x', 'Abc')`, and does not enforce the constraint. Nothing in the file
//! says which release wrote an index, so the index is compared with its table: an index is
//! withdrawn when it holds more NULL keys in a collated column than the table has NULL values
//! there. An index a current release wrote has exactly as many, and the table is read only when
//! the index holds a NULL in such a column at all.

use inillucent_base::DbResult;
use inillucent_sql::catalog_view::{IndexInfo, IndexOrigin, TableInfo, TableKind};
use inillucent_tree::datum::OwnedDatum;

use crate::*;

impl crate::ImportedDatabase {
    /// Finds the automatic indexes an older release wrote with no value for a collated key
    /// column and takes them out of the schema the planner reads.
    ///
    /// Runs at the end of an open and at the end of an `ATTACH`, after the tables are derived.
    pub(crate) fn withdraw_indexes_written_without_their_collation(&mut self) -> DbResult<()> {
        let mut found: Vec<(usize, Vec<u8>, u32, u32)> = Vec::new();
        for table in &self.schema.tables {
            for index in &table.indexes {
                if self.index_may_have_been_written_without_its_collation(table, index)
                    && self.index_lacks_values_its_table_has(table, index)?
                {
                    found.push((table.database, index.folded.clone(), table.root, index.root));
                }
            }
        }
        if found.is_empty() {
            return Ok(());
        }
        for (database, folded, table_root, index_root) in found {
            if let Some(roots) = self.schema.covering.get_mut(&table_root) {
                roots.retain(|root| *root != index_root);
            }
            self.schema
                .repairs
                .unusable_indexes
                .push((database, folded));
        }
        self.rebuild_tables()?;
        self.refresh_catalog();
        Ok(())
    }

    /// Reports whether any index is withdrawn and waiting for a rebuild.
    pub(crate) fn has_withdrawn_indexes(&self) -> bool {
        !self.schema.repairs.unusable_indexes.is_empty()
    }

    /// Rebuilds every withdrawn index, so that the table's constraint is enforced and answered
    /// through the index again.
    ///
    /// **A duplicate under the collation does not stop the rebuild.** A release that wrote no
    /// key for the column enforced nothing on it, so `'ABC'` and `'abc'` can both be in a
    /// `UNIQUE (a COLLATE NOCASE)` table. A `REINDEX` that refused them would leave the index
    /// withdrawn for ever and the table unwritable. The index is built with both entries, later
    /// inserts are checked against the collation, and `integrity_check` reports the two rows as
    /// the duplicate they are.
    pub(crate) fn repair_withdrawn_indexes(&mut self) -> DbResult<()> {
        // **The indexes go back into the schema first**, because `REINDEX` finds an index by
        // asking the catalog for it. They are withdrawn again if the rebuild fails, so a failed
        // repair never leaves a query reading them.
        let withdrawn = std::mem::take(&mut self.schema.repairs.unusable_indexes);
        self.rebuild_tables()?;
        self.refresh_catalog();
        self.schema.repairs.reindex_tolerates_duplicates = true;
        let mut outcome = Ok(());
        for (database, folded) in &withdrawn {
            let name = String::from_utf8_lossy(folded).replace('"', "\"\"");
            let schema = self.schema_name_of(*database);
            let schema = String::from_utf8_lossy(&schema).replace('"', "\"\"");
            outcome = self
                .execute_ddl(&format!("REINDEX \"{schema}\".\"{name}\""))
                .map(|_| ());
            if outcome.is_err() {
                break;
            }
        }
        self.schema.repairs.reindex_tolerates_duplicates = false;
        if outcome.is_err() {
            self.schema.repairs.unusable_indexes = withdrawn;
            let _ = self.rebuild_tables();
            self.refresh_catalog();
        }
        outcome
    }

    /// Reports whether an index is one a release before 2.1.0 could have written without a value
    /// for a key column: an automatic index of a rowid table, with a key column that names a
    /// column and is not `BINARY`.
    ///
    /// A column level `COLLATE`, a `CREATE INDEX` and a `WITHOUT ROWID` table were written
    /// correctly by those releases, and an index on an expression or with a predicate is not
    /// compared because its entries are not the table's columns.
    ///
    /// @param table - the table
    /// @param index - one of its indexes
    fn index_may_have_been_written_without_its_collation(
        &self,
        table: &TableInfo,
        index: &IndexInfo,
    ) -> bool {
        matches!(index.origin, IndexOrigin::Unique | IndexOrigin::PrimaryKey)
            && !table.without_rowid
            && table.kind == TableKind::Table
            && index.root != 0
            && index.root != table.root
            && index.partial_sql.is_none()
            && index.columns.iter().all(|key| key.expr_sql.is_none())
            && !collated_key_positions(index).is_empty()
            && self.schema.trees.contains_key(&index.root)
    }

    /// Reports whether the index holds more NULL keys in a collated column than its table holds
    /// NULL values in that column.
    ///
    /// The index is read first, and the table only when the index holds a NULL in a collated
    /// column at all.
    ///
    /// @param table - the table
    /// @param index - the index to read
    fn index_lacks_values_its_table_has(
        &self,
        table: &TableInfo,
        index: &IndexInfo,
    ) -> DbResult<bool> {
        let positions = collated_key_positions(index);
        let (Some(index_tree), Some(table_tree), Some(layout)) = (
            self.schema.trees.get(&index.root),
            self.schema.trees.get(&table.root),
            self.schema.layouts.get(&table.root),
        ) else {
            return Ok(false);
        };
        let (Some(index_file), Some(table_file)) = (
            self.schema_file(self.session_state.schema_of(index.root)),
            self.schema_file(self.session_state.schema_of(table.root)),
        ) else {
            return Ok(false);
        };
        let mut in_index = vec![0u64; positions.len()];
        index_tree.visit_leaves(index_file.pool(), &mut |leaf| {
            for entry in leaf.live()? {
                count_nulls(&positions, &mut in_index, |at| {
                    entry.get(at).is_none_or(|datum| datum.is_null())
                });
            }
            Ok(true)
        })?;
        if in_index.iter().all(|count| *count == 0) {
            return Ok(false);
        }
        let mut in_table = vec![0u64; positions.len()];
        table_tree.visit_leaves(table_file.pool(), &mut |leaf| {
            for row in leaf.live()? {
                let owned: Vec<OwnedDatum> = row.iter().map(OwnedDatum::from_datum).collect();
                let implied = plain_index_entry(index, layout, &owned);
                count_nulls(&positions, &mut in_table, |at| {
                    matches!(implied.get(at), None | Some(OwnedDatum::Null))
                });
            }
            Ok(true)
        })?;
        Ok(in_index
            .iter()
            .zip(&in_table)
            .any(|(held, wanted)| held > wanted))
    }
}

/// Returns the positions in an index's key whose column has a collation other than `BINARY`.
///
/// @param index - the index
fn collated_key_positions(index: &IndexInfo) -> Vec<usize> {
    index
        .columns
        .iter()
        .enumerate()
        .filter(|(_, key)| key.column.is_some() && !key.collation.eq_ignore_ascii_case(b"binary"))
        .map(|(position, _)| position)
        .collect()
}

/// Adds one to the count of each watched position that is NULL in the entry being read.
///
/// @param positions - the key positions being counted
/// @param counts - one count per watched position
/// @param is_null - whether the entry holds NULL at a key position
fn count_nulls(positions: &[usize], counts: &mut [u64], is_null: impl Fn(usize) -> bool) {
    for (count, position) in counts.iter_mut().zip(positions) {
        if is_null(*position) {
            *count = count.saturating_add(1);
        }
    }
}

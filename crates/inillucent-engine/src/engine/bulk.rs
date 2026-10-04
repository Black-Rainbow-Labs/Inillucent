//! `INSERT ... SELECT` into an empty table, built as one tree instead of row
//! by row.
//!
//! Invariant: **the bulk build writes nothing until every row has been built
//! and checked, and then writes exactly the trees and catalog rows the
//! ordinary insert would have left.** A statement it refuses either is handed
//! back to the ordinary insert unchanged, having written nothing, or fails with
//! the error the ordinary insert reports for it, also having written nothing.
//!
//! ## Why
//!
//! Measured on the performance hill climb: copying 100,000 rows into an empty
//! table with `INSERT INTO copied SELECT ... FROM main_table` took about 195 ms
//! through the Connection where SQLite took 37 ms, and a profile inside a
//! transaction put a third of it in making room in leaves (compactions and
//! splits) and a fifth in the log, most of that whole page images for the
//! splits. A table that holds no row has no leaf worth keeping, so the rows are
//! sorted and packed left to right by the bulk builder `CREATE INDEX` and
//! `ALTER TABLE` already use, which writes each page once and logs no image.
//!
//! ## How it replaces the tree
//!
//! The way `ALTER TABLE` rebuilds a table (`rebuild_table_tree`): the empty
//! tree is released, a new one is built under the same handle, the covering
//! index list is put back, and the catalog row is rewritten to name the new
//! root page. The handle does not change, so a compiled statement that names
//! it still reads the right tree, and recovery already knows the tree's shape
//! from the catalog row it has. A rollback restores the catalog row from the
//! undo buffer, and `reattach_entries` then attaches the tree that row names,
//! which is the empty one, whose pages were never freed because frees wait for
//! the commit. The table's indexes are rebuilt the same way, from entries
//! sorted the way `CREATE INDEX` sorts them.

use inillucent_base::error::refusal;
use inillucent_base::DbResult;
use inillucent_catalog::paged::SchemaEntry;
use inillucent_exec::dml::bulk::BulkRows;
use inillucent_exec::dml::{self, Changes, Row};
use inillucent_exec::physical::Params;
use inillucent_pool::PageId;
use inillucent_sql::catalog_view::{IndexInfo, TableInfo};
use inillucent_sql::dml::BoundInsert;
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::paged::KeyEncoding;
use inillucent_tree::types::ColumnSpec;
use inillucent_tree::PagedTree;
use inillucent_value::collation::Collation;

use crate::entries::EntrySet;
use crate::*;

/// How many rows a statement must insert before the bulk build is used.
///
/// The bulk build pays a fixed cost the ordinary insert does not: it syncs the
/// data file once for every tree it builds, because its pages reach the file
/// with no log record behind them, and it rewrites a catalog row per tree.
/// Measured with release builds, sixty copies inside one transaction so the
/// ordinary insert paid no sync of its own: at 64 and 256 rows the bulk build
/// was slower by about 1.3 ms a statement, at 1,000 rows the two were level,
/// and at 4,000 and 8,000 rows the bulk build took half the time or less, with
/// and without two indexes on the target. The box was shared with other work,
/// so the numbers moved by up to 40% between runs; the crossover did not.
const BULK_FLOOR: usize = 1_024;

/// What [`ImportedDatabase::insert_in_bulk`] did with a statement.
enum Bulk {
    /// The rows were built into the table, and this is what the statement
    /// reports.
    Done(Outcome),
    /// The statement takes the ordinary insert, with its rows given back.
    Declined(Vec<Row>),
}

/// One index tree, ready to be built.
struct IndexBuild {
    /// The handle the index's tree is registered under.
    root: u32,
    /// The index tree's column directory.
    columns: Vec<ColumnSpec>,
    /// How the index tree's columns map onto the table's.
    layout: SourceLayout,
    /// The entries, unsorted.
    entries: EntrySet,
    /// The entries' key order.
    order: Vec<u32>,
}

/// A slice of owned rows, read by the leaf builder without a borrowed copy of
/// each row.
///
/// `ALTER TABLE` makes a `Vec<Datum>` per row to hand the builder, which is
/// one allocation per row. Here that would be 100,000 allocations on the path
/// this module exists to make fast.
struct OwnedRows<'r>(&'r [Row]);

impl<'r> inillucent_tree::leaf::Rows<'r> for OwnedRows<'r> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn value(&self, row: usize, column: usize) -> Datum<'r> {
        self.0
            .get(row)
            .and_then(|held| held.get(column))
            .map_or(Datum::Null, OwnedDatum::borrow)
    }
}

impl ImportedDatabase {
    /// Runs an `INSERT` whose rows are already in hand, through the bulk build
    /// when the statement allows it and through the ordinary insert otherwise.
    ///
    /// @param statement - the bound insert
    /// @param params - the bound parameters
    /// @param rows - the rows a `SELECT` source produced, empty for `VALUES`
    pub(crate) fn insert_rows(
        &mut self,
        statement: &BoundInsert,
        params: &Params,
        rows: Vec<Row>,
    ) -> DbResult<Outcome> {
        let rows = match self.insert_in_bulk(statement, params, rows)? {
            Bulk::Done(outcome) => return Ok(outcome),
            Bulk::Declined(rows) => rows,
        };
        self.write(
            params,
            returning_names(&statement.returning),
            |target, params| dml::insert(statement, target, params, &rows),
        )
    }

    /// Builds an empty table's tree from an `INSERT ... SELECT`'s rows, when
    /// the statement and the table allow it.
    ///
    /// **The source rows are kept only when an index can refuse a
    /// duplicate.** Then a duplicate the sort finds hands the statement back
    /// to the ordinary insert, which writes the rows in order and so names the
    /// constraint SQLite names. Otherwise every failure is found in input order
    /// by `dml::bulk_rows`, which reports the ordinary insert's error itself,
    /// and the rows are given up as their images are built, so the statement
    /// never holds two copies of them.
    ///
    /// @param statement - the bound insert
    /// @param params - the bound parameters
    /// @param rows - the rows the `SELECT` produced
    fn insert_in_bulk(
        &mut self,
        statement: &BoundInsert,
        params: &Params,
        rows: Vec<Row>,
    ) -> DbResult<Bulk> {
        if rows.len() < BULK_FLOOR || !dml::bulk::bulk_shape(statement) {
            return Ok(Bulk::Declined(rows));
        }
        let txn = self.current_txn();
        if !self.with_write_view(txn, |view| dml::bulk::bulk_target_ready(statement, view))? {
            return Ok(Bulk::Declined(rows));
        }
        let table = &statement.table;
        let (built, kept) = if dml::bulk::has_unique_index(table) {
            let built = self.with_write_view(txn, |view| {
                dml::bulk::bulk_rows(statement, view, params, rows.iter())
            });
            match built {
                Ok(built) => (built, Some(rows)),
                Err(_) => return Ok(Bulk::Declined(rows)),
            }
        } else {
            let built = self.with_write_view(txn, |view| {
                dml::bulk::bulk_rows(statement, view, params, rows)
            });
            match built {
                Ok(built) => (built, None),
                Err(failure) => {
                    // What the ordinary insert's failure path records: the
                    // last rowid it wrote, and no rows changed.
                    self.remember_rowid(failure.last_rowid);
                    self.record_changes(0, 0);
                    return Err(failure.error);
                }
            }
        };
        let indexes = match self.index_builds(table, &built) {
            Ok(indexes) => indexes,
            Err(error) => {
                return match kept {
                    Some(rows) => Ok(Bulk::Declined(rows)),
                    None => Err(error),
                }
            }
        };
        self.apply_bulk(table, built, indexes).map(Bulk::Done)
    }

    /// Sorts every index's entries and refuses a duplicate in a unique one.
    ///
    /// An index on plain columns has its entries read straight out of the
    /// table rows, through the table's layout, the way `index_entries` reads
    /// them out of the leaves. An index whose key or predicate is an
    /// expression has its entries from `BulkRows::computed`, which the write
    /// path's evaluator made.
    ///
    /// @param table - the table being written
    /// @param built - the rows and the computed entries
    fn index_builds(&self, table: &TableInfo, built: &BulkRows) -> DbResult<Vec<IndexBuild>> {
        let table_layout = self
            .schema
            .layouts
            .get(&table.root)
            .cloned()
            .ok_or_else(|| refusal("no layout for the table being built"))?;
        let mut builds = Vec::new();
        for (position, index) in table.indexes.iter().enumerate() {
            if index.root == 0 || index.root == table.root {
                continue;
            }
            let (columns, layout) = index_shape(table, index, index.root);
            let width = columns.len();
            let collations: Vec<Collation> = columns.iter().map(|spec| spec.collation).collect();
            let directions: Vec<bool> = columns.iter().map(|spec| spec.descending).collect();
            let encoding = KeyEncoding::choose(&columns, width);
            let mut entries = EntrySet::with_capacity(
                width,
                built.rows.len(),
                encoding,
                &collations,
                &directions,
            );
            match built.computed.iter().find(|(at, _)| *at == position) {
                Some((_, computed)) => {
                    let mut entry: Vec<Datum<'_>> = Vec::with_capacity(width);
                    for row in computed {
                        entry.clear();
                        entry.extend(row.iter().map(OwnedDatum::borrow));
                        entries.push(&entry);
                    }
                }
                None => project_entries(&mut entries, index, &table_layout, &built.rows)?,
            }
            let order = entries.order();
            if index.unique {
                crate::ddl::refuse_duplicates(&entries, &order, table, index, width)?;
            }
            entries.release_sort_scratch();
            builds.push(IndexBuild {
                root: index.root,
                columns,
                layout,
                entries,
                order,
            });
        }
        Ok(builds)
    }

    /// Writes the table's tree and its indexes' trees, and commits when the
    /// statement is its own transaction.
    ///
    /// A failure part way undoes everything written here, the way
    /// `execute_ddl` undoes a directive that failed: back to the statement's
    /// mark, with the catalog reloaded so the table is attached at the root
    /// its restored row names.
    ///
    /// @param table - the table being written
    /// @param built - the rows, in key order
    /// @param indexes - the index trees to build
    fn apply_bulk(
        &mut self,
        table: &TableInfo,
        built: BulkRows,
        indexes: Vec<IndexBuild>,
    ) -> DbResult<Outcome> {
        let autocommit = self.writing.batch().is_none();
        let mark = self.statement_mark();
        let txn = self.current_txn();
        let previous = self.schema.ddl_schema;
        self.schema.ddl_schema = table.database;
        let written = self
            .replace_trees(table, &built.rows, indexes)
            .and_then(|()| self.seal());
        self.schema.ddl_schema = previous;
        if let Err(error) = written {
            let undone = self.undo_to_floor(mark, true, txn);
            if autocommit {
                self.writing.undo().borrow_mut().clear();
                self.writing.pending_frees().borrow_mut().clear();
                self.writing.built().borrow_mut().clear();
            }
            return Err(undone.err().unwrap_or(error));
        }
        if autocommit {
            // The statement committed in `seal`, so nothing can abandon it
            // now, and a later `ROLLBACK` must not reach its before-images.
            self.writing.undo().borrow_mut().clear();
        }
        let count = built.rows.len();
        self.remember_rowid(built.last_rowid);
        self.record_changes(count as i64, count as i64);
        Ok(Outcome {
            rows: Vec::new(),
            names: std::rc::Rc::new(Vec::new()),
            changes: Changes {
                rows: count,
                last_rowid: built.last_rowid,
                ..Default::default()
            },
        })
    }

    /// Releases the table's empty trees, builds the full ones under the same
    /// handles, and rewrites their catalog rows.
    ///
    /// **The covering list is saved first and put back last.** `release_tree`
    /// takes the table's own list out and takes each index out of every list,
    /// and nothing derives the list again, so leaving it would make a query
    /// that picks a covering index fail with `bad parameter or other API
    /// misuse`. That is the second of the three things the repository notes
    /// say a rebuild under a handle has to carry.
    ///
    /// @param table - the table being written
    /// @param rows - the rows, in key order
    /// @param indexes - the index trees to build
    fn replace_trees(
        &mut self,
        table: &TableInfo,
        rows: &[Row],
        indexes: Vec<IndexBuild>,
    ) -> DbResult<()> {
        let root = table.root;
        let covering = self.schema.covering.get(&root).cloned().unwrap_or_default();
        let (columns, key_columns) = self
            .schema
            .trees
            .get(&root)
            .map(|tree| (tree.columns().to_vec(), tree.key_columns()))
            .ok_or_else(|| refusal("no tree for the table being built"))?;
        let layout = self
            .schema
            .layouts
            .get(&root)
            .map(|held| (**held).clone())
            .ok_or_else(|| refusal("no layout for the table being built"))?;
        let mut handles = vec![root];
        self.release_tree(root)?;
        self.build_tree_rows(root, columns, key_columns, layout, &OwnedRows(rows))?;
        for build in indexes {
            let IndexBuild {
                root: handle,
                columns,
                layout,
                entries,
                order,
            } = build;
            let key_columns = columns.len();
            self.release_tree(handle)?;
            self.build_tree_rows(
                handle,
                columns,
                key_columns,
                layout,
                &entries.in_order(&order),
            )?;
            handles.push(handle);
        }
        if !covering.is_empty() {
            self.schema.covering.insert(root, covering);
            self.sort_covering(root);
        }
        for handle in handles {
            self.record_new_root(table.database, handle)?;
        }
        Ok(())
    }

    /// Rewrites the catalog row of one rebuilt tree so it names the new root
    /// page and the new statistics.
    ///
    /// The third thing a rebuild under a handle has to carry. `ALTER TABLE`
    /// rewrites only the root page and leaves the statistics as they were;
    /// here they go from an empty tree to a full one, and the row count is
    /// what a reattach after another process's commit gives the tree.
    ///
    /// @param at - the schema the tree is in
    /// @param handle - the tree's handle
    fn record_new_root(&mut self, at: usize, handle: u32) -> DbResult<()> {
        let page = self
            .schema
            .trees
            .get(&handle)
            .map(PagedTree::root)
            .unwrap_or(PageId::NONE);
        let stats = self.tree_stats(handle);
        let update = self
            .entries_of(at)
            .iter()
            .find(|held| held.root == handle)
            .map(|held| {
                let moved = SchemaEntry {
                    root: page,
                    stats,
                    ..held.entry.clone()
                };
                (held.rowid, moved)
            });
        match update {
            Some((rowid, entry)) => self.rewrite(rowid, entry),
            None => Err(refusal("a rebuilt tree has no catalog row")),
        }
    }
}

/// Fills an index's entries from table rows, for an index on plain columns.
///
/// An entry is the indexed columns followed by the rowid, which is what
/// `index_shape` describes and what `index_entries` reads out of the leaves.
///
/// @param entries - where the entries go
/// @param index - the index
/// @param layout - the table's layout, which says where each column is
/// @param rows - the table rows
fn project_entries(
    entries: &mut EntrySet,
    index: &IndexInfo,
    layout: &SourceLayout,
    rows: &[Row],
) -> DbResult<()> {
    let mut slots: Vec<usize> = Vec::with_capacity(index.columns.len().saturating_add(1));
    for key in &index.columns {
        let slot = key
            .column
            .and_then(|declared| layout.slots.get(usize::from(declared)).copied().flatten())
            .ok_or_else(|| refusal("an index on a column the tree does not carry"))?;
        slots.push(slot);
    }
    slots.push(
        layout
            .rowid
            .ok_or_else(|| refusal("an index on a table that identifies no row"))?,
    );
    let mut entry: Vec<Datum<'_>> = Vec::with_capacity(slots.len());
    for row in rows {
        entry.clear();
        entry.extend(
            slots
                .iter()
                .map(|slot| row.get(*slot).map_or(Datum::Null, OwnedDatum::borrow)),
        );
        entries.push(&entry);
    }
    Ok(())
}

//! `INSERT ... SELECT` into an empty table, built as one tree instead of row
//! by row.
//!
//! Invariant: **the bulk build leaves exactly the trees and catalog rows the
//! ordinary insert would have left, or nothing.** A statement it refuses either
//! is handed back to the ordinary insert unchanged, having written nothing, or
//! fails with the error the ordinary insert reports for it, also having written
//! nothing. A build from rows in memory checks every row before it writes. A
//! build from rows in a file checks each row as it writes it, and a row that
//! fails stops the build and the statement's writes are undone to its mark (see
//! [`ImportedDatabase::insert_in_bulk_streamed`]).
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

use crate::index_sort::EntrySorter;
use inillucent_base::error::refusal;
use inillucent_base::DbResult;
use inillucent_catalog::paged::SchemaEntry;
use inillucent_exec::dml::bulk::BulkRows;
use inillucent_exec::dml::{self, Changes, Row};
use inillucent_exec::gather::{Gathered, SpooledRows};
use inillucent_exec::insert_plan::RowSource;
use inillucent_exec::packed::PackedRows;
use inillucent_exec::physical::Params;
use inillucent_pool::PageId;
use inillucent_sql::catalog_view::{IndexInfo, TableInfo, TableKind};
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

/// The rows an `INSERT`'s source produced, one vector a row, packed, or in a transient file.
///
/// Packed when the source is a `SELECT` the bulk build could take (task-2226); see
/// [`PackedRows`]. The ordinary insert takes rows one vector each, so a packed set the bulk
/// build declines is unpacked for it. A source past the byte limit of [`inillucent_exec::gather`]
/// is in a file and is read back a row at a time, by the ordinary insert only: the bulk build needs
/// every row at once.
pub(crate) enum Collected {
    /// One vector a row.
    Rows(Vec<Row>),
    /// Every row in two vectors.
    Packed(PackedRows),
    /// Every row in a transient file.
    Spooled(SpooledRows),
}

impl From<Gathered> for Collected {
    /// Names what a collection of an insert's source produced.
    ///
    /// @param gathered - the collection
    fn from(gathered: Gathered) -> Collected {
        match gathered {
            Gathered::Rows(rows) => Collected::Rows(rows),
            Gathered::Packed(packed) => Collected::Packed(packed),
            Gathered::Spooled(spooled) => Collected::Spooled(spooled),
        }
    }
}

impl Collected {
    /// Returns how many rows there are.
    fn len(&self) -> usize {
        match self {
            Collected::Rows(rows) => rows.len(),
            Collected::Packed(packed) => packed.len(),
            Collected::Spooled(spooled) => usize::try_from(spooled.len()).unwrap_or(usize::MAX),
        }
    }

    /// Reports whether there are none.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the rows one vector each, for the ordinary insert.
    ///
    /// A set in a file is read back whole, which is why a statement that can stream does not call
    /// this.
    pub(crate) fn into_rows(self) -> DbResult<Vec<Row>> {
        match self {
            Collected::Rows(rows) => Ok(rows),
            Collected::Packed(packed) => Ok(packed.into_rows()),
            Collected::Spooled(spooled) => spooled.collect(),
        }
    }
}

/// Lends each row of a packed set to the bulk build, in order.
///
/// @param packed - the rows
fn packed_sources(packed: &PackedRows) -> impl Iterator<Item = RowSource<'_>> {
    (0..packed.len()).map(move |row| RowSource::Packed(packed, row))
}

/// The failure for rows in a file reaching the bulk build, which `insert_in_bulk` declines first.
fn never_bulk_built() -> dml::bulk::BulkFailure {
    dml::bulk::BulkFailure {
        error: refusal("rows held in a file were offered to the bulk build"),
        last_rowid: None,
    }
}

/// One index tree of a streamed bulk build: where its entries come from and where they are sorted.
struct StreamedIndex {
    /// The index's position in `TableInfo::indexes`.
    position: usize,
    /// The handle the index's tree is registered under.
    root: u32,
    /// The index tree's column directory.
    columns: Vec<ColumnSpec>,
    /// How the index tree's columns map onto the table's.
    layout: SourceLayout,
    /// The table tree slots an entry is read from, or none when the evaluator computed it.
    slots: Option<Vec<usize>>,
    /// The entries, sorted in runs when there are too many for memory.
    sorter: EntrySorter,
}

/// The images of a streamed bulk build, handed to the tree builder a chunk at a time.
///
/// Each source row is built and checked here, once. A row that fails, or a rowid that does not
/// increase, ends the input at the row before it, so the builder finishes a whole tree of the rows
/// so far and registers it, and the reason is kept in `stopped` for the caller to undo the
/// statement by.
struct ImageChunks<'a> {
    /// Builds and checks each image.
    stream: dml::bulk::BulkStream,
    /// The bound insert.
    statement: &'a BoundInsert,
    /// The source rows.
    spooled: &'a mut SpooledRows,
    /// The index trees, which take an entry from every image.
    builds: &'a mut Vec<StreamedIndex>,
    /// Why the input ended before the source did, when it did.
    stopped: Option<dml::bulk::StreamFailure>,
}

impl inillucent_tree::paged::RowChunks for ImageChunks<'_> {
    fn for_each_chunk(
        &mut self,
        each: &mut dyn for<'d> FnMut(&dyn inillucent_tree::leaf::Rows<'d>) -> DbResult<()>,
    ) -> DbResult<()> {
        let per_chunk = inillucent_exec::spill::chunk_rows(CHUNK_IMAGES);
        loop {
            let mut packed = PackedRows::new();
            let mut taken = 0usize;
            while taken < per_chunk {
                let Some(row) = self.spooled.next() else {
                    break;
                };
                let streamed = match self.stream.add(self.statement, row?) {
                    Ok(Some(streamed)) => streamed,
                    Ok(None) => continue,
                    Err(stopped) => {
                        self.stopped = Some(stopped);
                        break;
                    }
                };
                for build in self.builds.iter_mut() {
                    let mut entry: Vec<Datum<'_>> = Vec::new();
                    match &build.slots {
                        Some(slots) => entry.extend(slots.iter().map(|slot| {
                            streamed
                                .image
                                .get(*slot)
                                .map_or(Datum::Null, OwnedDatum::borrow)
                        })),
                        None => match streamed
                            .entries
                            .iter()
                            .find(|(at, _)| *at == build.position)
                        {
                            Some((_, computed)) => {
                                entry.extend(computed.iter().map(OwnedDatum::borrow))
                            }
                            None => continue,
                        },
                    }
                    build.sorter.push(&entry)?;
                }
                if !packed.push_owned(&streamed.image) {
                    return Err(refusal(
                        "a row built for a bulk insert is not as wide as the rows before it",
                    ));
                }
                taken = taken.saturating_add(1);
            }
            if taken > 0 {
                let view: &PackedRows = &packed;
                each(&view)?;
            }
            if taken == 0 || self.stopped.is_some() {
                return Ok(());
            }
        }
    }
}

/// How many images are handed to the tree builder at a time.
const CHUNK_IMAGES: usize = 16 * 1024;

/// What [`ImportedDatabase::insert_in_bulk`] did with a statement.
enum Bulk {
    /// The rows were built into the table, and this is what the statement
    /// reports.
    Done(Outcome),
    /// The statement takes the ordinary insert, with its rows given back.
    Declined(Collected),
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

impl ImportedDatabase {
    /// Runs an `INSERT` whose rows are already in hand, through the bulk build
    /// when the statement allows it and through the ordinary insert otherwise.
    ///
    /// @param statement - the bound insert
    /// @param params - the bound parameters
    /// @param rows - the rows a `SELECT` source produced, empty for `VALUES`
    /// @param cache - where the insert's setup is kept between executions, or
    ///   `None` when this execution's parameters were folded and are its own
    pub(crate) fn insert_rows(
        &mut self,
        statement: &BoundInsert,
        params: &Params,
        rows: Collected,
        cache: Option<&dml::insert::cached::InsertCache>,
    ) -> DbResult<Outcome> {
        let rows = match self.insert_in_bulk(statement, params, rows)? {
            Bulk::Done(outcome) => return Ok(outcome),
            Bulk::Declined(rows) => rows,
        };
        // **A source in a file is written a row at a time, inside the one write view.** Anything
        // else is a slice, as it always was. A view is written through its `INSTEAD OF` trigger,
        // which takes the rows as a slice, so a view's source is read back whole.
        let mut rows = match rows {
            Collected::Spooled(spooled) if statement.table.kind == TableKind::View => {
                Collected::Rows(spooled.collect::<DbResult<Vec<Row>>>()?)
            }
            other => other,
        };
        self.write(
            params,
            returning_names(&statement.returning),
            |target, params| match &mut rows {
                Collected::Spooled(spooled) => {
                    dml::insert::cached::insert_streamed(statement, target, params, spooled, cache)
                }
                other => {
                    let held = std::mem::replace(other, Collected::Rows(Vec::new())).into_rows()?;
                    match cache {
                        Some(cache) => dml::insert::cached::insert_cached(
                            statement, target, params, &held, cache,
                        ),
                        None => dml::insert(statement, target, params, &held),
                    }
                }
            },
        )
    }

    /// Reports whether the bulk build could take an insert's rows, asked before its source runs.
    ///
    /// The statement's shape and an empty target, the two questions `insert_in_bulk` asks
    /// first; the row count it asks too is not known yet. A yes collects the source's rows
    /// packed (task-2226).
    ///
    /// @param statement - the bound insert
    pub(crate) fn bulk_could_take(&mut self, statement: &BoundInsert) -> DbResult<bool> {
        if !dml::bulk::bulk_shape_with(statement, true) {
            return Ok(false);
        }
        let txn = self.current_txn();
        self.with_write_view(txn, |view| dml::bulk::bulk_target_ready(statement, view))
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
        rows: Collected,
    ) -> DbResult<Bulk> {
        if matches!(rows, Collected::Spooled(_)) {
            return self.insert_in_bulk_streamed(statement, params, rows);
        }
        if rows.len() < BULK_FLOOR || !dml::bulk::bulk_shape_with(statement, !rows.is_empty()) {
            return Ok(Bulk::Declined(rows));
        }
        let txn = self.current_txn();
        if !self.with_write_view(txn, |view| dml::bulk::bulk_target_ready(statement, view))? {
            return Ok(Bulk::Declined(rows));
        }
        let table = &statement.table;
        let (built, kept) = if dml::bulk::has_unique_index(table) {
            let built = self.with_write_view(txn, |view| match &rows {
                Collected::Rows(held) => dml::bulk::bulk_rows(statement, view, params, held.iter()),
                Collected::Packed(packed) => {
                    dml::bulk::bulk_rows(statement, view, params, packed_sources(packed))
                }
                Collected::Spooled(_) => Err(never_bulk_built()),
            });
            match built {
                Ok(built) => (built, Some(rows)),
                Err(_) => return Ok(Bulk::Declined(rows)),
            }
        } else {
            let built = self.with_write_view(txn, |view| match rows {
                Collected::Rows(held) => dml::bulk::bulk_rows(statement, view, params, held),
                Collected::Packed(packed) => {
                    dml::bulk::bulk_rows(statement, view, params, packed_sources(&packed))
                }
                Collected::Spooled(_) => Err(never_bulk_built()),
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
        let count = built.rows.len();
        self.apply_bulk_with(table, |database| {
            database
                .replace_trees(table, &built.rows, indexes)
                .map(|()| (count, built.last_rowid))
        })
    }

    /// Runs the part of a bulk build that writes, and settles the statement after it.
    ///
    /// @param table - the table being written
    /// @param write - builds the trees, and returns how many rows it wrote and the rowid of the
    ///   last row kept, in input order
    fn apply_bulk_with(
        &mut self,
        table: &TableInfo,
        write: impl FnOnce(&mut Self) -> DbResult<(usize, Option<i64>)>,
    ) -> DbResult<Outcome> {
        let autocommit = self.writing.batch().is_none();
        let mark = self.statement_mark();
        let txn = self.current_txn();
        let previous = self.schema.ddl_schema;
        self.schema.ddl_schema = table.database;
        let written = write(self).and_then(|wrote| self.seal().map(|()| wrote));
        self.schema.ddl_schema = previous;
        let (count, last_rowid) = match written {
            Ok(wrote) => wrote,
            Err(error) => {
                let undone = self.undo_to_floor(mark, true, txn);
                if autocommit {
                    self.writing.undo().borrow_mut().clear();
                    self.writing.pending_frees().borrow_mut().clear();
                    self.writing.built().borrow_mut().clear();
                }
                return Err(undone.err().unwrap_or(error));
            }
        };
        if autocommit {
            // The statement committed in `seal`, so nothing can abandon it
            // now, and a later `ROLLBACK` must not reach its before-images.
            self.writing.undo().borrow_mut().clear();
        }
        self.remember_rowid(last_rowid);
        self.record_changes(count as i64, count as i64);
        Ok(Outcome {
            rows: Vec::new(),
            names: std::rc::Rc::new(Vec::new()),
            changes: Changes {
                rows: count,
                last_rowid,
                ..Default::default()
            },
        })
    }

    /// Builds an empty table's tree from an `INSERT ... SELECT` whose source is in a file.
    ///
    /// **The fast path for a copy too large to hold, which reads its source twice.** The first pass
    /// builds and checks every row in input order and keeps nothing, so a constraint that fails is
    /// found, and named, before a page is written, exactly as the bulk build names it. The second
    /// builds each row again and hands the images to the tree builder a chunk at a time, while the
    /// entries of the table's indexes go to sorters that write runs past a byte limit. The rows
    /// must arrive with increasing rowids, which an `INSERT ... SELECT` from a rowid table and one
    /// that lets every rowid be allocated both produce; any other order is handed back to the
    /// ordinary insert, which needs no order, having written nothing.
    ///
    /// A table with a `UNIQUE` index is not built this way, because the ordinary insert names the
    /// constraint SQLite names when two fail on different rows.
    ///
    /// @param statement - the bound insert
    /// @param params - the bound parameters
    /// @param rows - the source rows, which are in a file
    fn insert_in_bulk_streamed(
        &mut self,
        statement: &BoundInsert,
        params: &Params,
        rows: Collected,
    ) -> DbResult<Bulk> {
        let Collected::Spooled(mut spooled) = rows else {
            return Ok(Bulk::Declined(rows));
        };
        let table = &statement.table;
        let txn = self.current_txn();
        let ready = dml::bulk::bulk_shape_with(statement, true)
            && !dml::bulk::has_unique_index(table)
            && self.with_write_view(txn, |view| dml::bulk::bulk_target_ready(statement, view))?;
        if !ready {
            return Ok(Bulk::Declined(Collected::Spooled(spooled)));
        }
        let stream = self.with_write_view(txn, |view| {
            dml::bulk::BulkStream::open(statement, view, params)
        });
        let stream = match stream {
            Ok(stream) => stream,
            Err(failure) => return self.bulk_failed(failure),
        };
        let mut stopped: Option<dml::bulk::StreamFailure> = None;
        let written = self.apply_bulk_with(table, |database| {
            database.replace_trees_streamed(statement, stream, &mut spooled, &mut stopped)
        });
        match (written, stopped) {
            (Ok(outcome), None) => {
                inillucent_exec::gather::note_streamed_build();
                Ok(Bulk::Done(outcome))
            }
            // The rows do not arrive in rowid order. What was built is undone, and the ordinary
            // insert, which needs no order, takes the rows from the first.
            (_, Some(dml::bulk::StreamFailure::NotIncreasing)) => {
                spooled.rewind();
                Ok(Bulk::Declined(Collected::Spooled(spooled)))
            }
            (_, Some(dml::bulk::StreamFailure::Failed(failure))) => self.bulk_failed(failure),
            (Err(error), None) => Err(error),
        }
    }

    /// Reports a row a bulk build refused, as the ordinary insert's failure path would.
    ///
    /// The ordinary insert records the last rowid it wrote and no rows changed.
    ///
    /// @param failure - the error, and the rowid of the last row kept before it
    fn bulk_failed(&mut self, failure: dml::bulk::BulkFailure) -> DbResult<Bulk> {
        self.remember_rowid(failure.last_rowid);
        self.record_changes(0, 0);
        Err(failure.error)
    }

    /// Builds the table's tree and its indexes' trees from a source in a file, a chunk at a time.
    ///
    /// The shape of [`Self::replace_trees`], with the images built and checked as the table's tree
    /// is built.
    ///
    /// **One pass over the file, not two.** The rows used to be read, built and checked once to
    /// find a failure before anything was written, and read, built and checked again to write
    /// them. At 3,000,000 rows into an empty table that took 1.09 s where the 2.7.0 path, which
    /// held every row in memory and checked each once, took 0.78 s; the second check was the
    /// whole of the difference. A failure no longer needs finding first, because the statement
    /// is undone to its mark when this returns an error: a row that fails, or a rowid that does
    /// not increase, ends the table's input there, the builder finishes and registers the tree of
    /// the rows before it so the undo can give its pages back, and this returns an error with the
    /// reason in `stopped`.
    ///
    /// Returns how many rows were written and the rowid of the last.
    ///
    /// @param statement - the bound insert
    /// @param stream - builds and checks each row
    /// @param spooled - the source rows, positioned at the first
    /// @param stopped - where the reason the rows ended early is put
    fn replace_trees_streamed(
        &mut self,
        statement: &BoundInsert,
        stream: dml::bulk::BulkStream,
        spooled: &mut SpooledRows,
        stopped: &mut Option<dml::bulk::StreamFailure>,
    ) -> DbResult<(usize, Option<i64>)> {
        let table = &statement.table;
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
        let table_layout = self
            .schema
            .layouts
            .get(&root)
            .cloned()
            .ok_or_else(|| refusal("no layout for the table being built"))?;
        let mut builds = self.streamed_index_builds(table, &table_layout)?;
        let mut handles = vec![root];
        self.release_tree(root)?;
        let mut chunks = ImageChunks {
            stream,
            statement,
            spooled,
            builds: &mut builds,
            stopped: None,
        };
        self.build_tree_chunks(root, columns, key_columns, layout, &mut chunks)?;
        let (count, last_rowid) = (chunks.stream.kept(), chunks.stream.last_rowid());
        if let Some(reason) = chunks.stopped.take() {
            *stopped = Some(reason);
            return Err(refusal(
                "a streamed copy stopped at a row it could not write",
            ));
        }
        for build in builds {
            let StreamedIndex {
                root: handle,
                columns,
                layout,
                sorter,
                ..
            } = build;
            let width = columns.len();
            let entries = sorter.finish()?;
            entries.refuse_long_keys(&columns, width)?;
            self.release_tree(handle)?;
            self.build_tree_from_entries(handle, columns, width, layout, &entries)?;
            handles.push(handle);
        }
        if !covering.is_empty() {
            self.schema.covering.insert(root, covering);
            self.sort_covering(root);
        }
        for handle in handles {
            self.record_new_root(table.database, handle)?;
        }
        Ok((usize::try_from(count).unwrap_or(usize::MAX), last_rowid))
    }

    /// Returns a sorter for each index tree the table has, and where its entries come from.
    ///
    /// @param table - the table being written
    /// @param table_layout - the table's layout, which says where each column is
    fn streamed_index_builds(
        &self,
        table: &TableInfo,
        table_layout: &SourceLayout,
    ) -> DbResult<Vec<StreamedIndex>> {
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
            let computed = index.partial_sql.is_some()
                || index.columns.iter().any(|key| key.expr_sql.is_some());
            let slots = if computed {
                None
            } else {
                let mut slots: Vec<usize> =
                    Vec::with_capacity(index.columns.len().saturating_add(1));
                for key in &index.columns {
                    let slot = key
                        .column
                        .and_then(|declared| {
                            table_layout
                                .slots
                                .get(usize::from(declared))
                                .copied()
                                .flatten()
                        })
                        .ok_or_else(|| refusal("an index on a column the tree does not carry"))?;
                    slots.push(slot);
                }
                slots.push(
                    table_layout
                        .rowid
                        .ok_or_else(|| refusal("an index on a table that identifies no row"))?,
                );
                Some(slots)
            };
            builds.push(StreamedIndex {
                position,
                root: index.root,
                columns,
                layout,
                slots,
                sorter: EntrySorter::new(
                    width,
                    0,
                    encoding,
                    &collations,
                    &directions,
                    inillucent_exec::physical::TreeCatalog::spill(self),
                ),
            });
        }
        Ok(builds)
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
        rows: &PackedRows,
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
        self.build_tree_rows(root, columns, key_columns, layout, &rows)?;
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
    rows: &PackedRows,
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
    for row in 0..rows.len() {
        entry.clear();
        entry.extend(slots.iter().map(|slot| rows.value(row, *slot)));
        entries.push(&entry);
    }
    Ok(())
}

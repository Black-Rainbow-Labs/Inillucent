//! A bulk build over rows that arrive a chunk at a time, so the whole input is never resident.
//!
//! Invariant: **the tree this builds holds exactly the rows the chunks held, in the order they were
//! delivered, and every page is written once.** The rows of one chunk are planned and packed the
//! way a single row source is (`plan_leaves` and `encode_rows_with`), each chunk's leaves are one
//! run of consecutive pages, and the right sibling of the last leaf of a chunk is the first leaf of
//! the next. A leaf is therefore held back until the page after it is known, which is the only
//! thing a chunk boundary changes.
//!
//! ## What a chunk boundary costs
//!
//! A leaf that ends a chunk is as full as the chunk made it, so a build of many chunks has one
//! leaf at each boundary that a single pass would have filled further. A chunk is thousands of
//! rows, so the extra leaves are a fraction of a percent of the tree. The separator of the first
//! leaf of a chunk is the whole first key, because the shortening rule needs the row before it and
//! that row is in the previous chunk; a longer separator is always correct.
//!
//! ## Why the key length is checked for a chunk before any of its pages are written
//!
//! The same refusal a single row source gets (`refuse_rows_with_a_key_over_the_limit`) runs for each
//! chunk. A caller that must not leave pages behind for a refusal checks every chunk first with
//! [`PagedTree::refuse_chunk_keys`], which writes nothing.

use super::super::*;
use super::leafplan::plan_leaves;
use super::queue::{flush_built_pages, queue_built_page, PendingRun};
use super::{build_interior_levels, BuiltLeaves, KeyOrder};

/// Sorted rows delivered a chunk at a time.
///
/// A callback rather than an iterator, because a chunk borrows from the producer's own buffers and
/// the producer reuses them for the next chunk. The callback is given each chunk in order and the
/// chunk is gone when it returns.
pub trait RowChunks {
    /// Hands each chunk, in key order, to `each`, and stops at the first failure.
    ///
    /// @param each - what to do with one chunk
    fn for_each_chunk(
        &mut self,
        each: &mut dyn for<'d> FnMut(&dyn crate::leaf::Rows<'d>) -> DbResult<()>,
    ) -> DbResult<()>;
}

/// The leaves written so far, and the one held back.
struct ChunkWriter<'a> {
    /// The leaf builder, which owns the page size and the layout.
    builder: LeafBuilder,
    /// How a row's key becomes the bytes a descent compares.
    keys: KeyOrder<'a>,
    /// The tree the pages belong to.
    tree_id: u64,
    /// Built leaves waiting to be written as one run.
    pending: PendingRun,
    /// The last leaf built, which is not queued until its right sibling is known.
    held: Option<(PageId, Vec<u8>)>,
    /// Every leaf, in key order.
    leaves: Vec<PageId>,
    /// Each leaf's first key.
    separators: Vec<Vec<u8>>,
    /// The first leaf, which `Body::BulkBuilt` records.
    first_leaf: Option<PageId>,
    /// How many pages the leaves took.
    pages_written: u64,
    /// How many rows have been placed.
    row_count: u64,
    /// The file's page size, which decides how long a separator may be.
    page_size: usize,
}

impl ChunkWriter<'_> {
    /// Queues the held leaf with `next` as its right sibling.
    ///
    /// @param database - the file the pages are written into
    /// @param log - where the allocation records go, when there are any
    /// @param next - the page to the right of the held leaf, or `PageId::NONE`
    fn release_held(
        &mut self,
        database: &mut Database,
        log: &mut Option<&mut dyn crate::write::TreeLog>,
        next: PageId,
    ) -> DbResult<()> {
        let Some((id, mut image)) = self.held.take() else {
            return Ok(());
        };
        page::set_right(&mut image, next)?;
        queue_built_page(log, database, &mut self.pending, id, &mut image)
    }

    /// Plans and packs one chunk's leaves.
    ///
    /// @param database - the file the pages are written into
    /// @param log - where the allocation records go, when there are any
    /// @param rows - the chunk, in key order and after every earlier chunk
    fn add_chunk<'d>(
        &mut self,
        database: &mut Database,
        log: &mut Option<&mut dyn crate::write::TreeLog>,
        rows: &dyn crate::leaf::Rows<'d>,
    ) -> DbResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        crate::keylimit::refuse_rows_with_a_key_over_the_limit(
            rows,
            self.keys.key_columns,
            self.keys.encoding,
            self.keys.collations,
            self.keys.directions,
        )?;
        let plan = plan_leaves(
            &self.builder,
            rows,
            self.keys.key_columns,
            self.keys.encoding,
            self.keys.collations,
            self.keys.directions,
            self.page_size,
        )?;
        let first = database.allocate(plan.boundaries.len() as u64)?;
        let mut nowhere = crate::write::NoLog::default();
        let mut placed_at = 0usize;
        for (index, placed) in plan.boundaries.iter().copied().enumerate() {
            let image = {
                let keys = &self.keys;
                let owner_of = |row: usize| keys.owner_of(rows, placed_at.saturating_add(row));
                let mut spiller = Extender {
                    database: &mut *database,
                    log: match log.as_deref_mut() {
                        Some(log) => log,
                        None => &mut nowhere,
                    },
                    tree_id: self.tree_id,
                    key_columns: self.keys.key_columns,
                    written: Vec::new(),
                    owner_of: &owner_of,
                };
                self.builder.encode_rows_with(
                    rows,
                    placed_at,
                    placed,
                    Some(&mut spiller),
                    plan.layouts.get(index),
                )?
            };
            placed_at = placed_at.saturating_add(placed);
            let id = PageId(first.0.saturating_add(index as u64));
            self.release_held(database, log, id)?;
            self.held = Some((id, image));
            self.leaves.push(id);
            if self.first_leaf.is_none() {
                self.first_leaf = Some(id);
            }
            self.pages_written = self.pages_written.saturating_add(1);
        }
        self.separators.extend(plan.separators);
        self.row_count = self.row_count.saturating_add(plan.row_count);
        Ok(())
    }

    /// Writes the last leaf, and one empty leaf when no row arrived.
    ///
    /// @param database - the file the pages are written into
    /// @param log - where the allocation records go, when there are any
    fn finish(
        mut self,
        database: &mut Database,
        log: &mut Option<&mut dyn crate::write::TreeLog>,
    ) -> DbResult<BuiltLeaves> {
        if self.held.is_none() && self.leaves.is_empty() {
            // An empty tree is still a tree: one empty leaf, so every reader has a page to land
            // on. `bulk_build_rows` writes the same page for an empty row source.
            let id = database.allocate(1)?;
            let image = self.builder.encode_empty()?;
            self.held = Some((id, image));
            self.leaves.push(id);
            self.separators.push(Vec::new());
            self.first_leaf = Some(id);
            self.pages_written = 1;
        }
        self.release_held(database, log, PageId::NONE)?;
        flush_built_pages(database, &mut self.pending)?;
        Ok(BuiltLeaves {
            leaves: self.leaves,
            first_leaf: self.first_leaf.unwrap_or(PageId::NONE),
            pages_written: self.pages_written,
            separators: self.separators,
            row_count: self.row_count,
        })
    }
}

impl PagedTree {
    /// Builds a tree bottom up from sorted rows that arrive a chunk at a time.
    ///
    /// **For an input larger than memory.** [`PagedTree::bulk_build_rows`] needs every row
    /// addressable at once, three times over (the key length check, the leaf plan and the
    /// packing). This holds one chunk and the leaves' separators, which are a key per leaf rather
    /// than per row.
    ///
    /// @param database - the file the pages are allocated and written in
    /// @param log - where the records go, when the build is inside a transaction
    /// @param tree_id - the identifier stamped into every page
    /// @param columns - the column directory, key columns first
    /// @param key_columns - how many leading columns form the key
    /// @param chunks - the rows, in key order
    /// @param sync - whether to sync the data file once the pages are written
    pub fn bulk_build_chunks(
        database: &mut Database,
        mut log: Option<&mut dyn crate::write::TreeLog>,
        tree_id: u64,
        columns: Vec<ColumnSpec>,
        key_columns: usize,
        chunks: &mut dyn RowChunks,
        sync: bool,
    ) -> DbResult<PagedTree> {
        let page_size = database.page_size();
        let encoding = KeyEncoding::choose(&columns, key_columns);
        let collations = collations_of(&columns, key_columns);
        let directions = directions_of(&columns, key_columns);
        let mut writer = ChunkWriter {
            builder: LeafBuilder::new(page_size, tree_id, columns.clone(), key_columns)?,
            keys: KeyOrder {
                encoding,
                collations: &collations,
                directions: &directions,
                key_columns,
            },
            tree_id,
            pending: PendingRun::default(),
            held: None,
            leaves: Vec::new(),
            separators: Vec::new(),
            first_leaf: None,
            pages_written: 0,
            row_count: 0,
            page_size,
        };
        chunks.for_each_chunk(&mut |rows| writer.add_chunk(database, &mut log, rows))?;
        let built = writer.finish(database, &mut log)?;
        PagedTree::finish_bulk_build(database, log, tree_id, columns, key_columns, built, sync)
    }

    /// Builds the interior levels over leaves already written, syncs when asked, and returns the
    /// tree.
    ///
    /// **The tail every bulk build shares.** A build from one row source and a build from a
    /// sequence of chunks write their leaves differently and finish the same way: the interior
    /// levels are built from the leaves' separators, the data file is synced before the
    /// statement's records, and one record names what happened.
    ///
    /// @param database - the file the pages were written into
    /// @param log - where the build's record goes, when it is logged
    /// @param tree_id - the tree's identifier
    /// @param columns - the column directory
    /// @param key_columns - how many leading columns form the key
    /// @param built - the leaves and what pass one worked out about them
    /// @param sync - whether to sync the data file once the pages are written
    pub(super) fn finish_bulk_build(
        database: &mut Database,
        mut log: Option<&mut dyn crate::write::TreeLog>,
        tree_id: u64,
        columns: Vec<ColumnSpec>,
        key_columns: usize,
        built: BuiltLeaves,
        sync: bool,
    ) -> DbResult<PagedTree> {
        let page_size = database.page_size();
        let encoding = KeyEncoding::choose(&columns, key_columns);
        let BuiltLeaves {
            leaves,
            first_leaf,
            pages_written: leaf_pages_written,
            separators,
            row_count,
        } = built;
        let levels =
            build_interior_levels(database, &mut log, tree_id, page_size, &leaves, separators)?;
        let root = levels.root;
        let height = levels.height;
        let pages_written = leaf_pages_written.saturating_add(levels.pages_written);
        // **The data file is synced before the statement's own records are, and
        // that is the whole of design 2's safety argument** (task-2000). Every page
        // this build wrote went straight into the file with no log record behind
        // it, so the pages have to be on the media before the commit that names
        // the root is. A crash before the commit's log sync leaves a catalog that
        // never named the root and `AllocPage` records belonging to an uncommitted
        // transaction, which are not replayed - so the pages are still free. A
        // crash after it leaves pages that were durable first. A torn page cannot
        // exist at the commit point, because this sync preceded it.
        if sync {
            database.pool().sync_data_file()?;
        }
        // And one record naming what happened, which recovery applies nothing for.
        // Without it the log holds `AllocPage` records for a run of pages whose
        // contents no record describes, which a reader cannot tell from a gap. See
        // `inillucent_wal::record::Body::BulkBuilt`.
        if let Some(log) = log.as_mut() {
            log.log(inillucent_wal::record::Body::BulkBuilt {
                root: root.0,
                first: first_leaf.0,
                count: pages_written,
            })?;
        }
        let collations = collations_of(&columns, key_columns);
        let directions = directions_of(&columns, key_columns);
        Ok(PagedTree {
            tree_id,
            root,
            shape: std::cell::Cell::new(Some((height, *leaves.first().unwrap_or(&PageId::NONE)))),
            columns,
            key_columns,
            page_size,
            encoding,
            collations,
            directions,
            leaf_count: leaves.len() as u64,
            row_count,
            scratch: RefCell::new(Vec::new()),
            leaf_hints: std::cell::RefCell::new(Vec::new()),
            growing: false,
            hint_victim: std::cell::Cell::new(0),
            largest_hint: std::cell::Cell::new(None),
            stats: std::cell::Cell::new(crate::write::WriteStats::default()),
            merge_backoff: RefCell::new(std::collections::HashMap::new()),
            pending_unlinks: RefCell::new(Vec::new()),
            last_leaf: std::cell::Cell::new(None),
        })
    }

    /// Refuses a chunk holding a key over the limit, writing nothing.
    ///
    /// The refusal [`PagedTree::bulk_build_chunks`] makes for each chunk as it reaches it, for a
    /// caller that has to find the refusal before the first page is allocated.
    ///
    /// @param columns - the column directory, key columns first
    /// @param key_columns - how many leading columns form the key
    /// @param rows - the chunk
    pub fn refuse_chunk_keys<'d>(
        columns: &[ColumnSpec],
        key_columns: usize,
        rows: &dyn crate::leaf::Rows<'d>,
    ) -> DbResult<()> {
        crate::keylimit::refuse_rows_with_a_key_over_the_limit(
            rows,
            key_columns,
            KeyEncoding::choose(columns, key_columns),
            &collations_of(columns, key_columns),
            &directions_of(columns, key_columns),
        )
    }
}

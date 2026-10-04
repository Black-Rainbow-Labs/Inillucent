//! Deleting or rewriting the keys one leaf holds together, for a statement that
//! changes many rows.
//!
//! Invariant: **a run writes the changes that changing its keys one at a time
//! would write, and changes its leaf once.** An undo image per key, so rollback
//! sees exactly what it saw before; for a delete one `DeleteRows` record naming
//! every key of the run (task-2183), which recovery replays key by key the way
//! it replays a `DeleteRow`; for an update an `UpdateInPlace` per key. The page
//! side is batched: one parse of the leaf for every key of the run, one
//! `pool.modify`, the page LSN set to the last record's, and for a delete one
//! look at whether the leaf has emptied enough to merge.
//!
//! ## Why the page LSN can be the last record's
//!
//! Redo applies a record to a page only when the page's LSN is below the
//! record's. The page reaches the file only after the log does, and when it
//! does it holds every change of the run, so its LSN is at least every one of
//! the run's records and redo skips them all. A page that did not reach the
//! file is replayed record by record, and each record finds its key by value,
//! so the order the page side applied them in does not matter.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::extent::ExtentRef;
use inillucent_pool::{Database, PageId, Pool};
use inillucent_wal::record::Body;

use super::{underflows, Located, TreeLog};
use crate::datum::{Datum, OwnedDatum};
use crate::leaf::LeafRef;
use crate::mutate::would_update_slot;
use crate::mutate::{Applied, LeafMut};
use crate::paged::PagedTree;

/// What an `UPDATE` wants done to one row, decided by the caller from the row as it is.
#[derive(Debug)]
pub enum Rewrite {
    /// Leave the row alone and do not count it: a constraint skipped it.
    Skip,
    /// Leave the row alone and count it: the new image is the old one.
    Unchanged,
    /// Write one non key column's new value.
    Column(usize, OwnedDatum),
    /// The row needs a write a run does not make, and the caller makes it.
    Row,
}

/// What one call of [`PagedTree::update_sorted`] did.
#[derive(Debug, Default)]
pub struct UpdateRun {
    /// How many keys it used, written or not.
    pub taken: usize,
    /// How many of those it wrote or found unchanged, which the caller counts.
    pub counted: usize,
    /// The row of the key straight after the used ones, as it is, when the
    /// caller was asked about it and has to write it itself.
    pub pending: Option<Vec<OwnedDatum>>,
    /// The column and new value the caller decided for `pending`, when the
    /// run asked about later rows before it handed `pending` back; the caller's
    /// own record of its last decision is then for a different row.
    pub pending_column: Option<(usize, OwnedDatum)>,
}

/// One slot a run writes: the key's place in the list and the value.
struct Slotted {
    /// Which key of the list this is.
    key: usize,
    /// Which column.
    column: usize,
    /// The new value.
    value: OwnedDatum,
    /// The row as it was, for the undo image.
    before: Vec<OwnedDatum>,
    /// Where the row lies in the leaf.
    located: Located,
}

/// One row a run deletes: where it sits in the leaf and what it held.
struct Doomed {
    /// Where the row is, as the search of the leaf found it.
    located: Located,
    /// The row, copied out, for the undo image and for the caller, or empty
    /// when neither needs it; see [`Run::page`].
    row: Vec<OwnedDatum>,
}

/// What one read of a leaf found for the front of a key list.
struct Run {
    /// The rows of the leading keys the leaf holds, in key order.
    doomed: Vec<Doomed>,
    /// The out-of-line values of the delta rows among them, which nobody
    /// owns once those rows are gone.
    orphaned: Vec<ExtentRef>,
    /// Whether the leaf already has a tombstone bitmap.
    has_tombstones: bool,
    /// A copy of the leaf as it was, when the rows were not copied out.
    ///
    /// The undo images then name a row's place in this copy rather than
    /// carrying the row; see [`TreeLog::undo_from_page`].
    page: Option<std::rc::Rc<[u8]>>,
}

impl PagedTree {
    /// Deletes the leading keys of a sorted list that one leaf holds.
    ///
    /// **One leaf per call (task-2180).** A statement that deletes many rows
    /// hands its keys over in this tree's order, so consecutive keys share a
    /// leaf, and deleting them one at a time parsed that leaf three times per
    /// key, changed it twice and asked twice whether it had emptied. Here the
    /// leaf is parsed once for the run, changed once, and asked once. The
    /// records are unchanged; see the module's note. Measured through the
    /// `Connection` on the hillclimb plan's `edge.delete.range`, 40,000 rows:
    /// 32.5 ms one at a time, 23.8 ms a leaf at a time, 11.4 ms in SQLite.
    ///
    /// Returns how many keys it used, at least one when `keys` is not empty,
    /// so a caller loops until the list is used up. A key the tree does not
    /// hold is used and deleted nothing. `deleted` is shown each row the run
    /// removes, in key order, before anything is logged or changed, so a
    /// failure there leaves the leaf and the log as they were.
    ///
    /// **Shown, not handed (task-2183).** The row then goes into the undo
    /// image by move. It used to be handed to `deleted` and cloned for the
    /// undo image, a copy of every value of every deleted row: 7.5% of a
    /// 40,000 row range delete.
    ///
    /// @param database - the file, for freeing a page a merge empties
    /// @param log - where the records go
    /// @param keys - the keys, in this tree's order, without repeats
    /// @param near - the leaf to look in first; set to the leaf this run used
    /// @param deleted - called once for each row that is removed
    /// @param shown - whether `deleted` is shown the row; when it is not, it
    ///   gets an empty slice, and a row is copied out only for an undo image
    ///   that cannot be read from a copy of the leaf
    pub fn delete_sorted(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        keys: &[Vec<Datum<'_>>],
        near: &mut Option<PageId>,
        deleted: &mut dyn FnMut(&[OwnedDatum]) -> DbResult<()>,
        shown: bool,
    ) -> DbResult<usize> {
        let Some(first) = keys.first() else {
            return Ok(0);
        };
        // A key found in `near` is in `near`, as `delete_near` argues, so the
        // remembered leaf needs no descent when it holds the first key.
        if let Some(page) = *near {
            if self.is_own_leaf(database.pool(), page) {
                let taken = self.delete_run(database, log, page, keys, near, deleted, shown)?;
                if taken > 0 {
                    return Ok(taken);
                }
            }
        }
        let encoded = self.encode_key(first);
        let (page, _) = self.leaf_for(database.pool(), &encoded)?;
        *near = Some(page);
        let taken = self.delete_run(database, log, page, keys, near, deleted, shown)?;
        // None means the first key's own leaf does not hold it, so the tree
        // does not, and the key is used without deleting anything.
        Ok(taken.max(1))
    }

    /// Deletes the leading keys of a list that one known leaf holds.
    ///
    /// Returns 0 when the leaf does not hold the first key.
    ///
    /// @param database - the file
    /// @param log - where the records go
    /// @param page - the leaf
    /// @param keys - the keys, in this tree's order
    /// @param near - the remembered leaf, cleared when the per row path ran
    /// @param deleted - called once for each row that is removed
    /// @param shown - whether `deleted` is shown the row or an empty slice
    fn delete_run(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        page: PageId,
        keys: &[Vec<Datum<'_>>],
        near: &mut Option<PageId>,
        deleted: &mut dyn FnMut(&[OwnedDatum]) -> DbResult<()>,
        shown: bool,
    ) -> DbResult<usize> {
        let undo = log.wants_undo();
        let run = self.read_run(database.pool(), page, keys, shown, undo)?;
        let (Some(first), false) = (keys.first(), run.doomed.is_empty()) else {
            return Ok(0);
        };
        let tombstoning = run
            .doomed
            .iter()
            .any(|held| matches!(held.located, Located::Sorted(_)));
        if tombstoning && !run.has_tombstones && !self.tombstone_fits(database.pool(), page)? {
            // The bitmap has to be made and there is no room for it. The per
            // row delete compacts or splits the leaf and tries again, which is
            // rare enough to leave to it, one key at a time.
            if let Some(row) = self.delete_near(database, log, first, near)? {
                deleted(if shown { &row } else { &[] })?;
            }
            return Ok(1);
        }
        for held in &run.doomed {
            deleted(&held.row)?;
        }
        let located: Vec<Located> = run.doomed.iter().map(|held| held.located).collect();
        let lsn = self.log_run(log, page, keys, run.doomed, run.page.as_ref())?;
        database
            .pool()
            .modify(page, |bytes| apply_run(bytes, &located, lsn))?;
        for reference in run.orphaned {
            crate::paged::free_extent(database, log, reference)?;
        }
        let taken = located.len();
        let mut stats = self.stats.get();
        stats.deleted = stats.deleted.saturating_add(taken as u64);
        self.stats.set(stats);
        self.note_rows(-(taken as i64));
        let underflowed = {
            let guard = database.pool().fetch(page)?;
            underflows(&LeafRef::parse(&guard)?)?
        };
        if underflowed {
            let path = self.leaf_for(database.pool(), &self.encode_key(first))?.1;
            self.merge_if_small(database, log, page, &path)?;
        }
        Ok(taken)
    }

    /// Reads the rows of the leading keys one leaf holds, with one parse.
    ///
    /// Stops at the first key the leaf does not hold: that key may be in the
    /// next leaf, which is the caller's next run.
    ///
    /// @param pool - the buffer pool
    /// @param page - the leaf
    /// @param keys - the keys, in this tree's order
    /// @param shown - whether the caller is shown each row
    /// @param undo - whether the log keeps undo images
    fn read_run(
        &self,
        pool: &Pool,
        page: PageId,
        keys: &[Vec<Datum<'_>>],
        shown: bool,
        undo: bool,
    ) -> DbResult<Run> {
        let guard = pool.fetch(page)?;
        let leaf = LeafRef::parse(&guard)?
            .with_collations(self.collations())
            .with_directions(self.directions());
        // A row is copied out when the caller looks at it, or for an undo
        // image when the leaf holds an out-of-line value a copy of the page
        // would not. Otherwise the undo images read a copy of the page.
        let copied = shown || (undo && leaf.has_extents());
        let snapshot: Option<std::rc::Rc<[u8]>> =
            (undo && !copied).then(|| std::rc::Rc::from(leaf.bytes()));
        let mut doomed: Vec<Doomed> = Vec::new();
        let mut orphaned = Vec::new();
        // The keys are in the tree's order, so each is searched for from the
        // sorted row past the last one found.
        let mut from = 0usize;
        for key in keys {
            let located = leaf.locate_from(key, self.key_columns(), from)?;
            if let Located::Sorted(row) = located {
                from = row.saturating_add(1);
            }
            // A repeat would name the same row twice and log a record that
            // finds nothing on replay. The caller removes repeats; this stops
            // rather than trusting it.
            if located == Located::Absent || doomed.last().map(|held| held.located) == Some(located)
            {
                break;
            }
            let row = match copied {
                true => match self.row_in_leaf(pool, &leaf, located)? {
                    Some(row) => row,
                    None => break,
                },
                false => Vec::new(),
            };
            if let (Located::Delta(index), true) = (located, leaf.has_extents()) {
                for column in 0..leaf.column_count() {
                    if let Some(reference) = leaf.delta_extent_at(index, column)? {
                        orphaned.push(reference);
                    }
                }
            }
            doomed.push(Doomed { located, row });
        }
        Ok(Run {
            doomed,
            orphaned,
            has_tombstones: leaf.has_tombstones(),
            page: snapshot,
        })
    }

    /// Reports whether a leaf with no tombstone bitmap has room to make one.
    ///
    /// @param pool - the buffer pool
    /// @param page - the leaf
    fn tombstone_fits(&self, pool: &Pool, page: PageId) -> DbResult<bool> {
        pool.modify(page, |bytes| {
            LeafMut::new(bytes)?.has_room_for_a_tombstone()
        })
    }

    /// Writes each row's undo image, and one `DeleteRows` record for the run.
    ///
    /// Returns the record's LSN, which the leaf is stamped with. A run of one
    /// key is a `DeleteRow`, the record the row at a time path writes.
    ///
    /// @param log - where the records go
    /// @param page - the leaf
    /// @param keys - the keys, the first `doomed.len()` of which are being deleted
    /// @param doomed - the rows being deleted, which become the undo images
    /// @param snapshot - a copy of the leaf the undo images name places in,
    ///   when the rows were not copied out
    fn log_run(
        &self,
        log: &mut dyn TreeLog,
        page: PageId,
        keys: &[Vec<Datum<'_>>],
        doomed: Vec<Doomed>,
        snapshot: Option<&std::rc::Rc<[u8]>>,
    ) -> DbResult<u64> {
        let wants_undo = log.wants_undo();
        let run = doomed.len();
        // Tagged values, not the comparison encoding, for the reason
        // `delete_near` gives.
        let mut tagged: Vec<Vec<u8>> = Vec::with_capacity(run);
        for (key, held) in keys.iter().zip(doomed) {
            if wants_undo {
                match snapshot {
                    Some(copy) => log.undo_from_page(self.tree_id(), key, copy, held.located)?,
                    None => log.undo(self.tree_id(), key, Some(held.row))?,
                }
            }
            let mut bytes = Vec::new();
            for value in key.iter().take(self.key_columns()) {
                value.encode_tagged(&mut bytes);
            }
            tagged.push(bytes);
        }
        if let [only] = tagged.as_slice() {
            return log.log(Body::DeleteRow {
                tree: self.tree_id(),
                page: page.0,
                key: only,
            });
        }
        let mut list = Vec::new();
        inillucent_wal::record::put_key_list(&mut list, tagged.iter().map(Vec::as_slice));
        log.log(Body::DeleteRows {
            tree: self.tree_id(),
            page: page.0,
            keys: &list,
        })
    }

    /// Returns the row at a place in a leaf already parsed, copied out.
    ///
    /// Only the row's own out-of-line values are read, not the leaf's.
    ///
    /// @param pool - the buffer pool, for the row's out-of-line values
    /// @param leaf - the leaf
    /// @param located - where the row is
    pub(super) fn row_in_leaf(
        &self,
        pool: &Pool,
        leaf: &LeafRef<'_>,
        located: Located,
    ) -> DbResult<Option<Vec<OwnedDatum>>> {
        let (held, sorted, place) = match located {
            Located::Sorted(row) => (self.read_extents_row(pool, leaf, row)?, true, row),
            Located::Delta(index) => (self.read_extents_delta(pool, leaf, index)?, false, index),
            Located::Absent => return Ok(None),
        };
        let leaf = leaf.with_extents(&held);
        let mut values = Vec::with_capacity(leaf.column_count());
        for column in 0..leaf.column_count() {
            let value = match sorted {
                true => leaf.value(place, column)?,
                false => leaf.delta_value(place, column)?,
            };
            values.push(OwnedDatum::from_datum(&value));
        }
        Ok(Some(values))
    }
}

/// Removes a run's rows from its leaf and stamps the leaf with the last LSN.
///
/// The tombstones first, while the delta area is where the room check saw it,
/// then every delta row in one rewrite. See [`LeafMut::remove_deltas`] for why
/// the page comes out as the one row at a time path leaves.
///
/// @param bytes - the leaf's page
/// @param doomed - where the rows are, as `read_run` found them
/// @param lsn - the run's last record
fn apply_run(bytes: &mut [u8], doomed: &[Located], lsn: u64) -> DbResult<()> {
    let mut leaf = LeafMut::new(bytes)?;
    let mut deltas = Vec::new();
    for located in doomed {
        match *located {
            Located::Sorted(row) => {
                if matches!(leaf.set_tombstone(row)?, Applied::NoRoom) {
                    // Unreachable: the room check answered for this page.
                    return Err(corrupt("a leaf lost the room for its tombstones"));
                }
            }
            Located::Delta(index) => deltas.push(index),
            Located::Absent => return Err(corrupt("a row that was read could not be deleted")),
        }
    }
    leaf.remove_deltas(&deltas)?;
    leaf.set_lsn(lsn)
}

impl PagedTree {
    /// Rewrites one column of each leading key of a sorted list that one leaf holds.
    ///
    /// **The `UPDATE` half of the batch (task-2180).** `rewrite` is shown each
    /// row as it is and answers what to write. A new value for one column that
    /// fits where the old one lies is applied to a copy of the leaf, and when
    /// the run ends the copy replaces the leaf in one change, after one
    /// `UpdateInPlace` record and undo image per row. Writing one row at a time
    /// read the row with a descent from the root, then searched the leaf again,
    /// copied the row for the undo image and changed the page, per row.
    ///
    /// The copy is what keeps the log first. Each slot write is asked against
    /// the page as the writes before it left it, which is what recovery replays
    /// the records onto, so the leaf comes out as the records describe; and the
    /// change made after the records are written is a copy, which cannot fail.
    ///
    /// The run stops at the first key the leaf does not hold, and at the first
    /// row whose answer it cannot write: [`Rewrite::Row`], or a column that
    /// does not fit its slot or is in the delta area. That row is handed back
    /// in [`UpdateRun::pending`] for the caller to write the general way. When
    /// `rewrite` fails, the rows before it are written first, so what is in
    /// the tree is what one at a time would have left.
    ///
    /// @param database - the file
    /// @param log - where the records go
    /// @param keys - the keys, in this tree's order, without repeats
    /// @param near - the leaf to look in first; set to the leaf this run used
    /// @param rewrite - decides each row's new value from the row as it is
    /// @param run - set to what this call did, also when it fails
    pub fn update_sorted(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        keys: &[Vec<Datum<'_>>],
        near: &mut Option<PageId>,
        rewrite: &mut dyn FnMut(&[OwnedDatum]) -> DbResult<Rewrite>,
        run: &mut UpdateRun,
    ) -> DbResult<()> {
        *run = UpdateRun::default();
        let Some(first) = keys.first() else {
            return Ok(());
        };
        if let Some(page) = *near {
            if self.is_own_leaf(database.pool(), page)
                && self.update_run(database, log, page, keys, rewrite, run)?
            {
                return Ok(());
            }
        }
        let encoded = self.encode_key(first);
        let (page, _) = self.leaf_for(database.pool(), &encoded)?;
        *near = Some(page);
        if !self.update_run(database, log, page, keys, rewrite, run)? {
            // The first key's own leaf does not hold it, so the tree does not.
            run.taken = 1;
        }
        Ok(())
    }

    /// Rewrites the leading keys of a list that one known leaf holds.
    ///
    /// Returns false, having asked nothing, when the leaf does not hold the
    /// first key.
    ///
    /// @param database - the file
    /// @param log - where the records go
    /// @param page - the leaf
    /// @param keys - the keys, in this tree's order
    /// @param rewrite - decides each row's new value
    /// @param run - what this call did
    fn update_run(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        page: PageId,
        keys: &[Vec<Datum<'_>>],
        rewrite: &mut dyn FnMut(&[OwnedDatum]) -> DbResult<Rewrite>,
        run: &mut UpdateRun,
    ) -> DbResult<bool> {
        let mut asked = Asked::default();
        let outcome = self.ask_run(database.pool(), page, keys, rewrite, run, &mut asked);
        if matches!(outcome, Ok(false)) {
            return Ok(false);
        }
        if let Some(original) = asked.original.take() {
            let mut changes = std::mem::take(&mut asked.slotted);
            let started = changes.len();
            changes.append(&mut asked.repacked);
            if self.repack_run(database, log, page, keys, &original, &mut changes)? {
                return outcome.map(|_| true);
            }
            // The rows do not fit one page with their new values. The rows
            // before the first that did not fit its slot are written into
            // their slots, and that row goes back to the caller.
            let later = changes.split_off(started);
            asked.slotted = changes;
            if let (Some(first), Some(fallback)) = (later.into_iter().next(), asked.fallback) {
                run.taken = fallback.taken;
                run.counted = fallback.counted;
                run.pending_column = Some((first.column, first.value));
                run.pending = Some(first.before);
            }
        }
        if let (Some(image), false) = (asked.copy, asked.slotted.is_empty()) {
            let lsn = self.log_slots(log, page, keys, asked.slotted)?;
            database.pool().modify(page, |bytes| {
                if bytes.len() != image.len() {
                    return Err(corrupt("a leaf changed size during an update"));
                }
                bytes.copy_from_slice(&image);
                LeafMut::new(bytes)?.set_lsn(lsn)
            })?;
        }
        outcome.map(|_| true)
    }

    /// Asks the caller about each leading key the leaf holds and writes what fits into a copy.
    ///
    /// Returns false when the leaf does not hold the first key. A failure of
    /// `rewrite` is returned after `run` and `asked` describe the rows before
    /// it, so the caller can still write those.
    ///
    /// @param pool - the buffer pool
    /// @param page - the leaf
    /// @param keys - the keys, in this tree's order
    /// @param rewrite - decides each row's new value
    /// @param run - what this call did
    /// @param asked - the copy of the leaf and the slots written into it
    fn ask_run(
        &self,
        pool: &Pool,
        page: PageId,
        keys: &[Vec<Datum<'_>>],
        rewrite: &mut dyn FnMut(&[OwnedDatum]) -> DbResult<Rewrite>,
        run: &mut UpdateRun,
        asked: &mut Asked,
    ) -> DbResult<bool> {
        let guard = pool.fetch(page)?;
        let leaf = LeafRef::parse(&guard)?
            .with_collations(self.collations())
            .with_directions(self.directions());
        for (index, key) in keys.iter().enumerate() {
            let located = self.locate_in(&leaf, key)?;
            let Some(before) = self.row_in_leaf(pool, &leaf, located)? else {
                return Ok(index > 0);
            };
            let written = match rewrite(&before)? {
                Rewrite::Skip => false,
                Rewrite::Unchanged => true,
                Rewrite::Column(column, value) => {
                    let change = Slotted {
                        key: index,
                        column,
                        value,
                        before,
                        located,
                    };
                    // Once the leaf is to be repacked, every change goes with it.
                    if asked.original.is_some() {
                        asked.repacked.push(change);
                        run.taken = run.taken.saturating_add(1);
                        run.counted = run.counted.saturating_add(1);
                        continue;
                    }
                    if self.slot_into(
                        guard.bytes(),
                        &mut asked.copy,
                        located,
                        change.column,
                        &change.value,
                    )? {
                        asked.slotted.push(change);
                    } else if change.column >= self.key_columns() && !leaf.has_extents() {
                        // **The value does not fit where it lies, so the leaf
                        // is repacked with every change of the run (task-2183)**
                        // instead of this row and every later one going a row
                        // at a time, each a delta row that fills the leaf and
                        // compacts it again.
                        asked.original = Some(guard.bytes().to_vec());
                        asked.fallback = Some(Fallback {
                            taken: run.taken,
                            counted: run.counted,
                        });
                        asked.repacked.push(change);
                    } else {
                        run.pending = Some(change.before);
                        return Ok(true);
                    }
                    true
                }
                Rewrite::Row => {
                    run.pending = Some(before);
                    return Ok(true);
                }
            };
            run.taken = run.taken.saturating_add(1);
            if written {
                run.counted = run.counted.saturating_add(1);
            }
        }
        Ok(true)
    }

    /// Writes one value into its slot in a copy of a leaf, when it fits there.
    ///
    /// Returns false, having changed nothing, for a key column, a row in the
    /// delta area, or a value its slot cannot take; those are the cases
    /// `update_in_place` refuses too.
    ///
    /// **The copy is made at the first write that fits, not at the first
    /// question.** A leaf that has run out of room answers no for every row
    /// until the row at a time path makes room, and making the copy first cost
    /// a 32 KiB copy for each of those rows: `UPDATE copied SET label = label
    /// || '!'` over 100,000 rows took 430 ms where writing a row at a time took
    /// 209 ms, in `inillucent-hcprobe`'s `bulk.update.all`.
    ///
    /// @param page - the leaf as it is, asked while there is no copy yet
    /// @param copy - the copy of the leaf with the writes so far, made here
    /// @param located - where the row is
    /// @param column - which column
    /// @param value - the new value
    fn slot_into(
        &self,
        page: &[u8],
        copy: &mut Option<Vec<u8>>,
        located: Located,
        column: usize,
        value: &OwnedDatum,
    ) -> DbResult<bool> {
        let Located::Sorted(row) = located else {
            return Ok(false);
        };
        let borrowed = value.borrow();
        let asked = copy.as_deref().unwrap_or(page);
        if column < self.key_columns() || !would_update_slot(asked, column, row, &borrowed)? {
            return Ok(false);
        }
        let image = copy.get_or_insert_with(|| page.to_vec());
        match LeafMut::new(image)?.update_slot(column, row, &borrowed)? {
            Applied::Yes => Ok(true),
            Applied::NoRoom => Err(corrupt("a slot write that was costed did not apply")),
        }
    }

    /// Repacks a leaf with an update run's changes, splitting it when the rows no longer fit one page.
    ///
    /// Returns false, having written nothing, when they do not fit two.
    ///
    /// **One page image for the leaf, not a record per row (task-2183).** A
    /// change that does not fit its slot used to go a row at a time: a delta
    /// row, which the next such change found no room beside, so the leaf
    /// compacted and the next row filled it again. `UPDATE copied SET label =
    /// label || '!'` grows every row by a byte and went that way for most of
    /// its rows. Here the leaf's live rows are packed again with the new values
    /// in place and logged as one `CompactLeaf` carrying the page, which
    /// recovery copies; rows that have outgrown the page are split across two,
    /// logged as the split's three page images. Each row's undo image is still
    /// its own, so a rollback puts the rows back one at a time as before.
    ///
    /// @param database - the file
    /// @param log - where the records go
    /// @param page - the leaf
    /// @param keys - the keys the changes index into
    /// @param original - the leaf's bytes before the run
    /// @param changes - the changes, in key order; their before images are moved into the undo log
    fn repack_run(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        page: PageId,
        keys: &[Vec<Datum<'_>>],
        original: &[u8],
        changes: &mut [Slotted],
    ) -> DbResult<bool> {
        let collations = self.collations().to_vec();
        let directions = self.directions().to_vec();
        let leaf = LeafRef::parse(original)?
            .with_collations(&collations)
            .with_directions(&directions);
        if leaf.column_count() != self.columns().len() {
            return Ok(false);
        }
        let order = leaf.live_order()?;
        let positions: Vec<crate::leaf::LiveRow> = order.order().to_vec();
        let source = order.materialise()?;
        // The before images leave the changes first: the rows below borrow the
        // changes' new values for as long as the page is being built.
        let befores: Vec<(usize, Vec<OwnedDatum>)> = changes
            .iter_mut()
            .map(|change| (change.key, std::mem::take(&mut change.before)))
            .collect();
        let rows = with_changes(&positions, &source, changes)?;
        let builder = crate::leaf::LeafBuilder::new(
            self.page_size(),
            self.tree_id(),
            self.columns().to_vec(),
            self.key_columns(),
        )?;
        let packed =
            crate::write::compact_image(&builder, &crate::leaf::RowSlice(rows.as_slice()))?;
        let first_key = match changes.first().and_then(|change| keys.get(change.key)) {
            Some(key) => key.clone(),
            None => return Err(corrupt("a repack names a key the run was not given")),
        };
        let fits = match packed {
            Some(_) => true,
            None => {
                // Two pages, or the run goes back to a row at a time.
                let taken = super::rows_for_the_left_half(&builder, &rows, super::SPLIT_FILL)?;
                let right = rows.get(taken..).unwrap_or(&[]);
                rows.len() >= 2
                    && builder
                        .pack_all_rows(&crate::leaf::RowSlice(right), 1.0)?
                        .is_some()
            }
        };
        if !fits {
            // The caller hands the first change back, so its before image
            // goes back where it was taken from.
            drop(rows);
            for (change, (_, before)) in changes.iter_mut().zip(befores) {
                change.before = before;
            }
            return Ok(false);
        }
        let path = match packed {
            Some(_) => Vec::new(),
            None => {
                self.leaf_for(database.pool(), &self.encode_key(&first_key))?
                    .1
            }
        };
        let written = befores.len();
        if log.wants_undo() {
            for (at, before) in befores {
                let Some(key) = keys.get(at) else {
                    return Err(corrupt("a change names a key the run was not given"));
                };
                log.undo(self.tree_id(), key, Some(before))?;
            }
        }
        match packed {
            Some(mut image) => {
                crate::page::set_right(&mut image, leaf.right_sibling())?;
                crate::page::write_u64(
                    &mut image,
                    crate::leaf::leaf_header::MAX_CTS,
                    leaf.max_cts(),
                )?;
                let from_lsn = crate::page::read_u64(original, inillucent_pool::page::header::LSN)?;
                let lsn = log.log(Body::CompactLeaf {
                    tree: self.tree_id(),
                    page: page.0,
                    image: &image,
                    from_lsn,
                })?;
                database.pool().modify(page, |bytes| {
                    if bytes.len() != image.len() {
                        return Err(corrupt("a leaf changed size during an update"));
                    }
                    bytes.copy_from_slice(&image);
                    LeafMut::new(bytes)?.set_lsn(lsn)
                })?;
            }
            None => {
                // The rows are not the page's live rows, so the split is
                // logged as its page images: a replay copies them.
                let carried: &[Vec<Option<ExtentRef>>] = &[];
                self.split_carrying(
                    database,
                    log,
                    page,
                    &path,
                    &rows,
                    Some(carried),
                    super::SPLIT_FILL,
                )?;
            }
        }
        let mut stats = self.stats.get();
        stats.updated_in_place = stats.updated_in_place.saturating_add(written as u64);
        self.stats.set(stats);
        Ok(true)
    }

    /// Writes each slot's undo image and `UpdateInPlace` record, in key order.
    ///
    /// Returns the last record's LSN. The undo image is the row the run read,
    /// moved rather than copied: nothing else needs it.
    ///
    /// @param log - where the records go
    /// @param page - the leaf
    /// @param keys - the keys the slots index into
    /// @param slotted - the slots written, in key order
    fn log_slots(
        &self,
        log: &mut dyn TreeLog,
        page: PageId,
        keys: &[Vec<Datum<'_>>],
        slotted: Vec<Slotted>,
    ) -> DbResult<u64> {
        let mut lsn = 0;
        let mut tagged_key = Vec::new();
        let mut slot = Vec::new();
        let written = slotted.len();
        for held in slotted {
            let Some(key) = keys.get(held.key) else {
                return Err(corrupt("a slot names a key the run was not given"));
            };
            if log.wants_undo() {
                log.undo(self.tree_id(), key, Some(held.before))?;
            }
            tagged_key.clear();
            for value in key.iter().take(self.key_columns()) {
                value.encode_tagged(&mut tagged_key);
            }
            slot.clear();
            held.value.borrow().encode_tagged(&mut slot);
            lsn = log.log(Body::UpdateInPlace {
                tree: self.tree_id(),
                page: page.0,
                key: &tagged_key,
                column: held.column as u32,
                value: &slot,
            })?;
        }
        let mut stats = self.stats.get();
        stats.updated_in_place = stats.updated_in_place.saturating_add(written as u64);
        self.stats.set(stats);
        Ok(lsn)
    }
}

/// The leaf copy an update run writes into, and what it wrote.
#[derive(Default)]
struct Asked {
    /// The leaf with the slots written so far, made at the first.
    copy: Option<Vec<u8>>,
    /// The slots written into `copy`, in key order.
    slotted: Vec<Slotted>,
    /// The leaf as it was, once a value did not fit its slot and the leaf is
    /// to be repacked with every change of the run.
    original: Option<Vec<u8>>,
    /// The changes made after the repack began, in key order.
    repacked: Vec<Slotted>,
    /// How far the run had got when the repack began.
    fallback: Option<Fallback>,
}

/// How far an update run had got when it began to repack its leaf.
#[derive(Clone, Copy)]
struct Fallback {
    /// Keys used by then.
    taken: usize,
    /// Rows written or found unchanged by then.
    counted: usize,
}

/// Returns a leaf's live rows with an update run's changes in place.
///
/// The live rows and the changes are both in key order, so each change is
/// matched to its row by walking the two together.
///
/// @param positions - where each live row lies, in key order
/// @param source - the live rows' values
/// @param changes - the changes, in key order
fn with_changes<'p>(
    positions: &[crate::leaf::LiveRow],
    source: &'p crate::leaf::LiveSource<'p>,
    changes: &'p [Slotted],
) -> DbResult<Vec<Vec<Datum<'p>>>> {
    let mut rows: Vec<Vec<Datum<'p>>> = Vec::with_capacity(source.len());
    let mut next = changes.iter().peekable();
    for (at, position) in positions.iter().enumerate() {
        let mut row = source.row(at).to_vec();
        while let Some(change) = next.peek() {
            let here = match (change.located, *position) {
                (Located::Sorted(a), crate::leaf::LiveRow::Sorted(b)) => a == b as usize,
                (Located::Delta(a), crate::leaf::LiveRow::Delta(b)) => a == b as usize,
                _ => false,
            };
            if !here {
                break;
            }
            if let Some(cell) = row.get_mut(change.column) {
                *cell = change.value.borrow();
            }
            next.next();
        }
        rows.push(row);
    }
    if next.peek().is_some() {
        return Err(corrupt(
            "an update run changed a row the leaf does not hold",
        ));
    }
    Ok(rows)
}

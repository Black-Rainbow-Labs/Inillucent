//! Packing two adjacent leaves into one page, for a delete that emptied one of them.
//!
//! Invariant: **the merged image holds every live row of both leaves, in key
//! order, and fits `COMPACT_FILL` of a page, or there is no image.** The caller
//! logs the image whole in a `Structural::Merge` record, so recovery copies it
//! and never repacks, and which of the two routes below produced it is not a
//! question a replay asks.
//!
//! ## Two routes, and why the page route exists
//!
//! The owned route copies every live row of both leaves into owned values,
//! borrows them back, measures the pack and then encodes it: two passes and an
//! allocation per row and per text value. It is the only route for a leaf with
//! out-of-line values, because those are repacked through a spiller that hands
//! back the run a value is already in, and that needs the file.
//!
//! A leaf with no out-of-line value needs none of it. Its rows are read through
//! the page the way a compaction reads them ([`LeafRef::live_order`] and
//! [`crate::leaf::LiveOrder::materialise`]: one flat vector of borrowed values
//! per leaf), and [`LeafBuilder::pack_all_rows`] sizes and encodes in one call.
//! On the hillclimb plan's `churn.delete.half`, which merges fourteen times a
//! round, the owned route was a quarter of the delete; most of it was copying
//! rows, cloning the list of carried references and dropping both.

use inillucent_base::DbResult;
use inillucent_pool::extent::ExtentRef;
use inillucent_pool::{Database, PageId, Pool};

use super::{Measuring, TreeLog, COMPACT_FILL};
use crate::datum::{Datum, OwnedDatum};
use crate::leaf::{LeafBuilder, LeafRef, LiveSource, Packed, Rows};
use crate::paged::PagedTree;

/// The merged page, the runs the image kept, and the runs both leaves held.
type MergedImage = (Vec<u8>, Vec<ExtentRef>, Vec<ExtentRef>);

/// What packing two leaves straight from their pages produced.
enum FromPages {
    /// Both leaves fit one page, and this is it.
    Image(Vec<u8>),
    /// They do not fit one page together.
    TooBig,
    /// One of them holds an out-of-line value, so the owned route packs them.
    HasExtents,
}

/// Two leaves' live rows, the left one's first, as one [`Rows`].
struct Joined<'s, 'p> {
    /// The left leaf's rows.
    left: &'s LiveSource<'p>,
    /// The right leaf's rows, which all sort after the left leaf's.
    right: &'s LiveSource<'p>,
}

impl<'p> Rows<'p> for Joined<'_, 'p> {
    /// How many rows the two leaves hold together.
    fn len(&self) -> usize {
        self.left.len().saturating_add(self.right.len())
    }

    /// Returns one value, from the left leaf's rows and then the right's.
    ///
    /// @param row - which row of the two, in key order
    /// @param column - which column of it
    fn value(&self, row: usize, column: usize) -> Datum<'p> {
        match row.checked_sub(self.left.len()) {
            Some(at) => self.right.value(at, column),
            None => self.left.value(row, column),
        }
    }
}

/// Returns the fill a merge packs to.
///
/// **A leaf with no rows left merges at `TIGHT_FILL`.** `COMPACT_FILL` leaves
/// a merged leaf room for the writes that follow, but the merge of an emptied
/// leaf is only the right sibling's own rows, and a sibling an ascending insert
/// or a bulk build packed to `BULK_FILL` is fuller than `COMPACT_FILL` allows.
/// So the merge said "does not fit", the empty leaf stayed in the tree, and a
/// table deleted from the front never gave a page back. The merge is logged with
/// its whole image, so recovery installs the same bytes whatever fill made them.
///
/// @param emptied - whether the leaf the delete reached holds no live row
fn merge_fill(emptied: bool) -> f64 {
    match emptied {
        true => super::TIGHT_FILL,
        false => COMPACT_FILL,
    }
}

/// How many leaves the record of failed merges may hold before it is emptied.
const MOST_BACKED_OFF_LEAVES: usize = 4096;

impl PagedTree {
    /// Reports whether a leaf should skip a merge attempt because its last one failed and it
    /// has lost less than an eighth of its live rows since.
    ///
    /// **Why an attempt is not repeated on every delete.** A leaf below half full tries to merge
    /// on each delete, and an attempt packs both leaves. Deleting rows scattered over the
    /// table (the children of one parent, where the keys of a parent's children are far apart)
    /// leaves every leaf below half full for a long time, next to a sibling that is also part
    /// empty and still too full to take it. 8,569 attempts failed for 50,000 deletes, and they
    /// were most of the time. A merge that is delayed leaves a part empty leaf in place a
    /// little longer and changes no row.
    ///
    /// @param page - the leaf the delete reached
    /// @param live - how many live rows it holds now
    pub(super) fn merge_backed_off(&self, page: PageId, live: usize) -> bool {
        match self.merge_backoff.borrow().get(&page) {
            Some(failed_at) => live.saturating_mul(8) > failed_at.saturating_mul(7),
            None => false,
        }
    }

    /// Records that a leaf failed to merge while holding `live` live rows.
    ///
    /// @param page - the leaf
    /// @param live - how many live rows it held
    pub(super) fn note_failed_merge(&self, page: PageId, live: usize) {
        let mut held = self.merge_backoff.borrow_mut();
        if held.len() >= MOST_BACKED_OFF_LEAVES {
            held.clear();
        }
        held.insert(page, live);
    }

    /// Forgets a leaf's failed merge, because the leaf was rewritten or merged and is not the
    /// leaf the record was made for.
    ///
    /// @param page - the leaf
    pub(super) fn forget_failed_merge(&self, page: PageId) {
        self.merge_backoff.borrow_mut().remove(&page);
    }

    /// Returns the image a merge of a leaf and its right sibling installs, or
    /// nothing when their live rows do not fit one page.
    ///
    /// Also returns the out-of-line runs the image kept and the runs the two
    /// leaves held, so the caller frees exactly the ones the image dropped.
    /// Both lists are empty on the page route, which only runs when neither
    /// leaf holds such a value.
    ///
    /// @param database - the file, for the runs the owned route writes
    /// @param log - where the owned route records those runs
    /// @param page - the leaf the delete emptied
    /// @param right - its right sibling
    pub(super) fn merged_image(
        &self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        page: PageId,
        right: PageId,
    ) -> DbResult<Option<MergedImage>> {
        match self.merged_from_pages(database.pool(), page, right)? {
            FromPages::Image(image) => Ok(Some((image, Vec::new(), Vec::new()))),
            FromPages::TooBig => Ok(None),
            FromPages::HasExtents => self.merged_owned(database, log, page, right),
        }
    }

    /// Packs two leaves with no out-of-line values straight from their pages.
    ///
    /// The fill test is the owned route's - every live row within
    /// `COMPACT_FILL` of a page - asked without a spiller. With no value out of
    /// line, every value already fits inline, so a spiller would move nothing
    /// and both tests price the same bytes.
    ///
    /// @param pool - the buffer pool
    /// @param page - the leaf the delete emptied
    /// @param right - its right sibling
    fn merged_from_pages(&self, pool: &Pool, page: PageId, right: PageId) -> DbResult<FromPages> {
        let left_guard = pool.fetch(page)?;
        let right_guard = pool.fetch(right)?;
        let left = LeafRef::parse(&left_guard)?
            .with_collations(self.collations())
            .with_pad(&self.pad)
            .with_directions(self.directions());
        let right = LeafRef::parse(&right_guard)?
            .with_collations(self.collations())
            .with_pad(&self.pad)
            .with_directions(self.directions());
        if left.has_extents() || right.has_extents() {
            return Ok(FromPages::HasExtents);
        }
        let left_rows = left.live_order()?.materialise()?;
        let right_rows = right.live_order()?.materialise()?;
        let joined = Joined {
            left: &left_rows,
            right: &right_rows,
        };
        let builder = LeafBuilder::new(
            self.page_size(),
            self.tree_id(),
            self.columns().to_vec(),
            self.key_columns(),
        )?;
        Ok(
            match builder.pack_all_rows(&joined, merge_fill(left_rows.is_empty()))? {
                Some(image) => FromPages::Image(image),
                None => FromPages::TooBig,
            },
        )
    }

    /// Packs two leaves through owned copies of their rows, with a spiller that
    /// keeps each out-of-line value in the run it is already in.
    ///
    /// @param database - the file, for any run a value has to move into
    /// @param log - where those runs are recorded
    /// @param page - the leaf the delete emptied
    /// @param right - its right sibling
    fn merged_owned(
        &self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        page: PageId,
        right: PageId,
    ) -> DbResult<Option<MergedImage>> {
        let (mut rows, mut carried) = self.rows_to_repack(database.pool(), page)?;
        let emptied = rows.is_empty();
        let (right_rows, right_carried) = self.rows_to_repack(database.pool(), right)?;
        rows.extend(right_rows);
        carried.extend(right_carried);
        let mut doomed = self.extents_of(database.pool(), page)?;
        doomed.extend(self.extents_of(database.pool(), right)?);
        let builder = LeafBuilder::new(
            self.page_size(),
            self.tree_id(),
            self.columns().to_vec(),
            self.key_columns(),
        )?;
        let borrowed: Vec<Vec<Datum<'_>>> = rows
            .iter()
            .map(|row| row.iter().map(OwnedDatum::borrow).collect())
            .collect();
        // Measured without spilling and then encoded with it, for the reason
        // `split` gives: a spiller in the measure would write runs the encode
        // then writes again.
        let fits = matches!(
            builder.pack_with(&borrowed, merge_fill(emptied), Some(&mut Measuring))?,
            Packed::Filled { rows: packed, .. } if packed == borrowed.len()
        );
        if !fits {
            return Ok(None);
        }
        let owner_of = |row: usize| {
            let held = borrowed.get(row);
            held.map(|values| self.owner_key_of(values))
                .unwrap_or_default()
        };
        let mut spiller = crate::paged::Carrying {
            inner: crate::paged::Extender {
                key_columns: self.key_columns(),
                database,
                log,
                tree_id: self.tree_id(),
                written: Vec::new(),
                owner_of: &owner_of,
            },
            carried,
            used: Vec::new(),
        };
        let image = builder.encode_with(&borrowed, Some(&mut spiller))?;
        Ok(Some((image, spiller.used, doomed)))
    }
}

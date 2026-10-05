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

impl PagedTree {
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
            .with_directions(self.directions());
        let right = LeafRef::parse(&right_guard)?
            .with_collations(self.collations())
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
        Ok(match builder.pack_all_rows(&joined, COMPACT_FILL)? {
            Some(image) => FromPages::Image(image),
            None => FromPages::TooBig,
        })
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
            builder.pack_with(&borrowed, COMPACT_FILL, Some(&mut Measuring))?,
            Packed::Filled { rows: packed, .. } if packed == borrowed.len()
        );
        if !fits {
            return Ok(None);
        }
        let mut spiller = crate::paged::Carrying {
            inner: crate::paged::Extender {
                database,
                log,
                tree_id: self.tree_id(),
                written: Vec::new(),
            },
            carried,
            used: Vec::new(),
        };
        let image = builder.encode_with(&borrowed, Some(&mut spiller))?;
        Ok(Some((image, spiller.used, doomed)))
    }
}

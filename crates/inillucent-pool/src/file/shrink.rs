//! What a database file offers the step that lowers its end.
//!
//! Invariant: **the page count only falls through [`Database::shrink_to`] or a
//! replayed `PageCount` record, and every page numbered at or above it is free in
//! the map and absent from the pool.** The file itself is cut later, by the
//! checkpoint that writes the lower count.

use inillucent_base::error::misuse;
use inillucent_base::DbResult;

use super::Database;
use crate::meta::FORMAT_VERSION;
use crate::PageId;

/// What moving a free map page changed, for the caller that logs it.
#[derive(Clone, Debug)]
pub struct MapMove {
    /// The page before the moved one in the chain, or `None` for the first.
    pub previous: Option<PageId>,
    /// The moved page's image at its new number.
    pub image: Vec<u8>,
}

impl Database {
    /// Returns the highest page number that is the root of a tree, or zero.
    ///
    /// Zero means roots are not kept at the start of the file; see
    /// `Meta::largest_root`.
    pub fn largest_root(&self) -> u64 {
        self.meta.largest_root
    }

    /// Records the highest page number that is a root.
    ///
    /// It reaches the file at the next checkpoint, like every meta field.
    ///
    /// @param root - the page number, or zero to say roots are not kept low
    pub fn set_largest_root(&mut self, root: u64) {
        self.meta.largest_root = root;
    }

    /// Reports whether extent pages this file writes are in the owned layout.
    ///
    /// True for a file created by this build and for a file raised to format 3. A
    /// format 2 file keeps writing the older layout until it is raised.
    pub fn writes_owned_extents(&self) -> bool {
        self.meta.format >= FORMAT_VERSION
    }

    /// Returns the lowest free page below a limit.
    ///
    /// @param limit - only pages below this number are considered
    pub fn lowest_free_below(&self, limit: u64) -> Option<PageId> {
        self.free.lowest_free_below(limit)
    }

    /// Reports whether any page below a limit is free.
    ///
    /// @param limit - only pages below this number are considered
    pub fn has_free_below(&self, limit: u64) -> bool {
        self.free.has_free_below(limit)
    }

    /// Lowers the page count and drops every page at or above it from the pool.
    ///
    /// The pages must already be free in the map. The file keeps its length until
    /// the next checkpoint, which cuts it once the lower count is durable.
    ///
    /// @param pages - the new page count
    pub fn shrink_to(&mut self, pages: u64) -> DbResult<()> {
        if pages > self.pool.page_count() {
            return Err(misuse("the page count can only be lowered by a shrink"));
        }
        self.pool.lower_page_count(pages);
        self.pool.discard_from(pages)?;
        Ok(())
    }

    /// Lowers the page count to what a replayed `PageCount` record says.
    ///
    /// Frames above it are dropped once the whole replay is applied, by
    /// [`Database::discard_beyond_page_count`], because a later record in the same
    /// replay may write a page above this point again.
    ///
    /// @param pages - the page count the record carries
    pub fn lower_page_count(&mut self, pages: u64) {
        self.pool.lower_page_count(pages);
    }

    /// Drops every page at or above the page count from the pool.
    pub fn discard_beyond_page_count(&mut self) -> DbResult<()> {
        self.pool.discard_from(self.pool.page_count())?;
        Ok(())
    }

    /// Reports whether a page is one of the free map's own pages.
    ///
    /// The map keeps its pages in memory and writes them at a checkpoint, so the
    /// file may hold no readable copy of one.
    ///
    /// @param page - the page to ask about
    pub fn is_map_page(&self, page: PageId) -> bool {
        self.free.chain_index_of(page).is_some()
    }

    /// Moves a free map page to another page number.
    ///
    /// The map's bit for the new number is set and the bit for the old one cleared,
    /// the previous map page's `RIGHT` field is rewritten, and the meta record names
    /// the new page when the first map page moved. The caller logs the move and
    /// installs the returned image at the new number.
    ///
    /// @param from - the map page's current number
    /// @param to - the free page it moves to
    pub fn move_map_page(&mut self, from: PageId, to: PageId) -> DbResult<MapMove> {
        let previous = self
            .free
            .chain_index_of(from)
            .and_then(|index| self.free.chain_previous(index));
        let index = self.free.relink_map_page(from, to)?;
        self.free.allocate_at(to)?;
        self.free.free(from)?;
        if index == 0 {
            self.meta.free_map = to;
        }
        self.pool.forget_page(from)?;
        let image = self
            .free
            .pages()
            .nth(index)
            .map(|(_, bytes)| bytes.to_vec())
            .ok_or_else(|| misuse("a moved free map page is not in the chain"))?;
        Ok(MapMove { previous, image })
    }

    /// Records which page the free map chain starts at, for a replayed relink.
    ///
    /// @param page - the first map page
    pub fn set_free_map_head(&mut self, page: PageId) {
        self.meta.free_map = page;
    }
}

impl Database {
    /// Cuts the file to the pages the meta record just written counts, when the page
    /// count was lowered.
    ///
    /// **Called only by a checkpoint that moved the recovery point**, after the meta
    /// record is durable. A fold that left the recovery point where it was would
    /// cut pages that records still to be replayed name.
    pub fn cut_to_page_count(&mut self) -> DbResult<()> {
        self.pool.cut_the_file(self.meta.page_count)
    }

    /// Reports whether the page count was lowered and the file not yet cut.
    ///
    /// Until a checkpoint cuts it, the file is longer than the pages it counts, and
    /// the open must not trim it: the log may still name those pages.
    pub fn cut_is_pending(&self) -> bool {
        self.pool.cut_is_pending()
    }
}

impl Database {
    /// Asks the next checkpoint of this file to make its pages durable before it
    /// writes the meta record. See [`Pool::sync_pages_before_the_next_meta`].
    pub fn sync_pages_before_the_next_meta(&self) {
        self.pool.sync_pages_before_the_next_meta();
    }
}

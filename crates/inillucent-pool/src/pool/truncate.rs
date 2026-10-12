//! Moving a page out of the pool, lowering the page count, and cutting the file.
//!
//! Invariant: **after a page is forgotten here no frame, spilled copy or swizzled
//! swip names it, and the file is cut only once a meta record that does not count
//! the cut pages is durable.** The cut waits for the end of a checkpoint that
//! completed, so at no moment is the file shorter than the record on disk says.

use super::*;

impl Pool {
    /// Lowers the page count below its present value and remembers that the file
    /// is longer than the pages it counts.
    ///
    /// The checkpoint that writes the lower count is the one that cuts the file,
    /// through [`Pool::cut_the_file`]. Raising the count is [`Pool::set_page_count`].
    ///
    /// @param pages - the new page count
    pub fn lower_page_count(&self, pages: u64) {
        if pages < self.page_count.get() {
            self.cut_pending.set(true);
        }
        self.page_count.set(pages);
    }

    /// Cuts the file to the pages the record just written counts, when a lowered
    /// page count asked for it.
    ///
    /// **Called at the end of a checkpoint that completed**, with the file held
    /// exclusively, after both meta slots and the pages are durable. A crash before
    /// this leaves a file longer than its header, which the open path trims; a
    /// crash after leaves nothing to trim. Nothing here makes the file shorter than
    /// its header says.
    ///
    /// @param pages - the page count the durable record carries
    pub fn cut_the_file(&self, pages: u64) -> DbResult<()> {
        if !self.cut_pending.get() {
            return Ok(());
        }
        let wanted = pages.saturating_mul(self.page_size as u64);
        let there = self
            .file
            .file_size()
            .map_err(|error| error.into_db_error())?;
        if there > wanted {
            self.file
                .truncate(wanted)
                .map_err(|error| error.into_db_error())?;
        }
        self.cut_pending.set(false);
        Ok(())
    }

    /// Returns one page's bytes as they would be written to the file.
    ///
    /// A resident interior page may hold swizzled swips, which name frames and mean
    /// nothing at another page number, so each is written back as the page it
    /// names. A leaf and an extent page are returned as they are.
    ///
    /// @param page - the page to copy
    pub fn copy_for_move(&self, page: PageId) -> DbResult<Vec<u8>> {
        let mut image = self.fetch(page)?.bytes().to_vec();
        self.translate_swips(&mut image)?;
        Ok(image)
    }

    /// Drops a page from the pool without writing it, whatever it holds.
    ///
    /// **For a page that has just been copied to another number**, or that lies
    /// above a lowered page count. The parent's swizzled swip is put back to the
    /// page number first, so nothing names the frame afterwards. Returns whether a
    /// frame was dropped.
    ///
    /// @param page - the page to forget
    pub fn forget_page(&self, page: PageId) -> DbResult<bool> {
        let frame = self.state.borrow().table.get(&page).copied();
        let Some(frame) = frame else {
            self.forget_spilled(page);
            return Ok(false);
        };
        if self.pins_of(frame) > 0 {
            return Err(misuse(format!(
                "page {} is pinned and cannot be forgotten",
                page.0
            )));
        }
        if !self.unswizzle_from_parent(frame)? {
            return Err(misuse(format!(
                "page {} is named by a parent that is borrowed",
                page.0
            )));
        }
        self.forget_children_of(page);
        {
            let mut state = self.state.borrow_mut();
            state.cooling.retain(|held| *held != frame);
            state.table.remove(&page);
            state.amend(frame, |meta| *meta = FrameMeta::empty());
            state.free.push(frame);
        }
        if let Some(latch) = self.latch(frame) {
            if latch.try_exclusive() {
                latch.release_exclusive();
            }
        }
        self.forget_spilled(page);
        self.merged.borrow_mut().clear();
        Ok(true)
    }

    /// Drops every page numbered `pages` or more without writing any of it.
    ///
    /// Run when the page count has been lowered to `pages`: those pages are free,
    /// and a dirty frame left for one would be written back at the next
    /// checkpoint and grow the file again. Returns how many frames were dropped.
    ///
    /// @param pages - the lowest page number to drop
    pub fn discard_from(&self, pages: u64) -> DbResult<usize> {
        let above: Vec<PageId> = self
            .state
            .borrow()
            .table
            .keys()
            .copied()
            .filter(|page| page.0 >= pages)
            .collect();
        let mut dropped = 0usize;
        for page in above {
            if self.forget_page(page)? {
                dropped = dropped.saturating_add(1);
            }
        }
        Ok(dropped)
    }
}

impl Pool {
    /// Reports whether the page count was lowered and the file not yet cut.
    pub fn cut_is_pending(&self) -> bool {
        self.cut_pending.get()
    }
}

impl Pool {
    /// Asks the next checkpoint to sync the data file after its pages and before
    /// its meta record.
    ///
    /// **For a checkpoint that rewrites many pages at once**, as the one after a
    /// shrink does. The ordinary checkpoint writes the pages, then the shadow
    /// meta slot, then syncs once for both, so a power loss inside that sync can
    /// leave the new meta record durable beside a torn page that the log no
    /// longer covers, because the new record's recovery point is past the page
    /// images. The extra sync makes every page durable before any record that
    /// describes it can be.
    pub fn sync_pages_before_the_next_meta(&self) {
        self.sync_pages_first.set(true);
    }

    /// Syncs the data file when [`Pool::sync_pages_before_the_next_meta`] asked,
    /// and forgets the request.
    pub(super) fn sync_pages_if_asked(&self) -> DbResult<()> {
        if self.sync_pages_first.replace(false) {
            self.sync_data_file()?;
        }
        Ok(())
    }
}

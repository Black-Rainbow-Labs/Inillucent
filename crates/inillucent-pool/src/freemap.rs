//! The free map: one bit per page, spread over kind-`4` pages chained by the
//! common header's sibling field.
//!
//! Invariant: a page is allocated if and only if its bit is set, and the map
//! covers every page in the file. "Covers" is the load-bearing half: a file
//! that grew past the last map page has pages nobody can describe, so growing
//! the file and extending the chain are one operation ([`FreeMap::ensure`])
//! rather than two that could be interleaved.
//!
//! Bit `n` of the map is page `n` of the file, counting from page zero. The two
//! meta pages and the map pages themselves are allocated from the moment they
//! exist, so nothing can hand them out.
//!
//! ## Why a bitmap and not a free list
//!
//! A free list is one page write per allocation and gives no way to find a
//! *contiguous run*, which is what a blob extent wants. A bitmap answers both:
//! allocation is a scan from a hint, and a run of `k` pages is a run of `k`
//! clear bits. At 32 KiB pages one map page describes 261,888 pages, which is
//! 8 GiB of file - so the chain is one page long for every database this engine
//! is going to see, and the scan is over a resident page.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;

use crate::page::{self, PageKind, COMMON_HEADER};
use crate::PageId;

/// How many pages one map page can describe.
///
/// @param page_size - the database's page size in bytes
pub fn pages_per_map(page_size: usize) -> usize {
    page_size.saturating_sub(COMMON_HEADER).saturating_mul(8)
}

/// The free map over a run of map pages held as page images.
///
/// The map is read and written through the pool like any other page; this type
/// is the codec and the allocation policy, and it never touches a file.
pub struct FreeMap {
    /// The page size, which fixes how many pages one map page describes.
    page_size: usize,
    /// The map pages in chain order, with their page ids.
    pages: Vec<(PageId, Vec<u8>)>,
    /// Where the next allocation scan begins, as a global page index.
    hint: u64,
    /// Pages that may never be handed out, because the locking protocol locks
    /// bytes inside them. See `VfsFile::pages_under_the_lock_bytes`.
    reserved: Vec<u64>,
}

impl FreeMap {
    /// Returns an empty map with no pages yet.
    ///
    /// @param page_size - the database's page size in bytes
    pub fn new(page_size: usize) -> FreeMap {
        FreeMap {
            page_size,
            pages: Vec::new(),
            hint: 0,
            reserved: Vec::new(),
        }
    }

    /// Returns the same map, never handing out `pages`.
    ///
    /// **The pages under the lock bytes** (see
    /// `VfsFile::pages_under_the_lock_bytes`). They read as allocated whatever
    /// their bit says, so no allocation and no run ever includes one, and the
    /// file grows past them. A file an older release wrote may already hold data
    /// in one; that page keeps its data, and once it is freed it is never
    /// handed out again.
    ///
    /// @param pages - the page numbers to hold back
    pub fn reserving(mut self, pages: Vec<u64>) -> FreeMap {
        self.reserved = pages;
        self
    }

    /// Reports whether a page is held back because the lock bytes are in it.
    ///
    /// @param page - the page to test
    pub fn is_reserved(&self, page: PageId) -> bool {
        self.reserved.contains(&page.0)
    }

    /// Returns how many map pages the chain holds.
    pub fn chain_len(&self) -> usize {
        self.pages.len()
    }

    /// Returns the first map page's id, or [`PageId::NONE`] for an empty chain.
    pub fn first(&self) -> PageId {
        self.pages
            .first()
            .map(|(id, _)| *id)
            .unwrap_or(PageId::NONE)
    }

    /// Returns the map pages in chain order, for writing back.
    pub fn pages(&self) -> impl Iterator<Item = (PageId, &[u8])> {
        self.pages.iter().map(|(id, bytes)| (*id, bytes.as_slice()))
    }

    /// Adds a map page read from the file to the end of the chain.
    ///
    /// @param id - the map page's id
    /// @param bytes - the page image
    pub fn push_page(&mut self, id: PageId, bytes: Vec<u8>) -> DbResult<()> {
        if bytes.len() != self.page_size {
            return Err(corrupt(format!(
                "free-map page {} is {} bytes, not {}",
                id.0,
                bytes.len(),
                self.page_size
            )));
        }
        if page::kind_of(&bytes)? != PageKind::FreeMap {
            return Err(corrupt(format!("page {} is not a free-map page", id.0)));
        }
        self.pages.push((id, bytes));
        Ok(())
    }

    /// Returns the bit position of a page within the chain.
    ///
    /// @param page - the page to locate
    fn locate(&self, page: PageId) -> (usize, usize, u8) {
        let per = pages_per_map(self.page_size).max(1) as u64;
        let map = (page.0 / per) as usize;
        let within = (page.0 % per) as usize;
        (
            map,
            COMMON_HEADER.saturating_add(within / 8),
            1u8 << (within % 8),
        )
    }

    /// Reports whether a page is allocated.
    ///
    /// A page beyond the chain is reported allocated, because a page the map
    /// cannot describe is a page nothing may hand out.
    ///
    /// @param page - the page to test
    pub fn is_allocated(&self, page: PageId) -> bool {
        if self.is_reserved(page) {
            return true;
        }
        let (map, byte, mask) = self.locate(page);
        match self.pages.get(map).and_then(|(_, bytes)| bytes.get(byte)) {
            Some(value) => value & mask != 0,
            None => true,
        }
    }

    /// Marks a page allocated.
    ///
    /// @param page - the page to claim
    pub fn allocate_at(&mut self, page: PageId) -> DbResult<()> {
        self.set(page, true)
    }

    /// Marks a page free.
    ///
    /// @param page - the page to release
    pub fn free(&mut self, page: PageId) -> DbResult<()> {
        if page.0 < crate::meta::FIRST_DATA_PAGE.0 {
            return Err(corrupt(format!("page {} is a meta page", page.0)));
        }
        self.set(page, false)?;
        self.hint = self.hint.min(page.0);
        Ok(())
    }

    /// Sets or clears one bit.
    ///
    /// @param page - the page the bit describes
    /// @param allocated - what the bit should say
    fn set(&mut self, page: PageId, allocated: bool) -> DbResult<()> {
        let (map, byte, mask) = self.locate(page);
        let slot = self
            .pages
            .get_mut(map)
            .and_then(|(_, bytes)| bytes.get_mut(byte))
            .ok_or_else(|| corrupt(format!("page {} is outside the free map", page.0)))?;
        if allocated {
            *slot |= mask;
        } else {
            *slot &= !mask;
        }
        Ok(())
    }

    /// Grows the chain until it describes `pages` pages, taking new map pages
    /// from the run being described.
    ///
    /// Returns the ids of the map pages it created, which the caller writes.
    ///
    /// @param pages - how many pages the file will hold
    /// @param next_page - where the file currently ends, updated as it grows
    pub fn ensure(&mut self, pages: u64, next_page: &mut u64) -> DbResult<Vec<PageId>> {
        let per = pages_per_map(self.page_size).max(1) as u64;
        let mut created = Vec::new();
        while (self.pages.len() as u64).saturating_mul(per) < pages.max(*next_page) {
            // The map page is itself a page of the file, taken from the end -
            // but never a page the lock bytes are in.
            while self.reserved.contains(next_page) {
                *next_page = next_page.saturating_add(1);
            }
            let id = PageId(*next_page);
            *next_page = next_page.saturating_add(1);
            let mut bytes = vec![0u8; self.page_size];
            page::write_common(&mut bytes, PageKind::FreeMap, 0, 0)?;
            if let Some((_, previous)) = self.pages.last_mut() {
                page::set_right(previous, id)?;
            }
            self.pages.push((id, bytes));
            created.push(id);
            // Everything up to and including the new map page is allocated:
            // the meta pages, whatever the caller has already placed, and the
            // map pages themselves.
            for claimed in 0..*next_page {
                self.set(PageId(claimed), true).ok();
            }
        }
        Ok(created)
    }

    /// Returns a run of `count` contiguous free pages, marking them allocated.
    ///
    /// Returns `None` when the map has no run that long, which the caller
    /// answers by growing the file.
    ///
    /// @param count - how many contiguous pages are wanted
    pub fn allocate_run(&mut self, count: u64) -> DbResult<Option<PageId>> {
        if count == 0 {
            return Ok(None);
        }
        let total = (self.pages.len() as u64).saturating_mul(pages_per_map(self.page_size) as u64);
        let mut start = self.hint.max(crate::meta::FIRST_DATA_PAGE.0);
        while start.saturating_add(count) <= total {
            let mut run = 0u64;
            while run < count && !self.is_allocated(PageId(start.saturating_add(run))) {
                run = run.saturating_add(1);
            }
            if run == count {
                for offset in 0..count {
                    self.set(PageId(start.saturating_add(offset)), true)?;
                }
                self.hint = start.saturating_add(count);
                return Ok(Some(PageId(start)));
            }
            // Skip past the page that broke the run rather than retrying at
            // `start + 1`, which would rescan the same clear bits.
            start = start.saturating_add(run).saturating_add(1);
        }
        Ok(None)
    }

    /// Returns the lowest page below `limit` that is not allocated.
    ///
    /// The step that lowers the file's end asks for the page a moved page goes
    /// to, and the lowest one is the answer that fills the file from its front.
    /// A page held back because the lock bytes are in it is allocated, so it is
    /// never the answer. Whole bytes of set bits are skipped eight pages at a time.
    ///
    /// @param limit - only pages below this number are considered
    pub fn lowest_free_below(&self, limit: u64) -> Option<PageId> {
        let per = pages_per_map(self.page_size) as u64;
        for (index, (_, bytes)) in self.pages.iter().enumerate() {
            let base = (index as u64).saturating_mul(per);
            if base >= limit {
                break;
            }
            let body = bytes.get(COMMON_HEADER..).unwrap_or(&[]);
            let mut from = 0usize;
            while let Some(offset) = body
                .get(from..)
                .and_then(|rest| rest.iter().position(|byte| *byte != 0xFF))
            {
                let at = from.saturating_add(offset);
                let byte = body.get(at).copied().unwrap_or(0xFF);
                for bit in 0..8u64 {
                    let page = base
                        .saturating_add((at as u64).saturating_mul(8))
                        .saturating_add(bit);
                    if page >= limit {
                        return None;
                    }
                    let free = byte & (1u8 << bit) == 0;
                    if free
                        && page >= crate::meta::FIRST_DATA_PAGE.0
                        && !self.is_reserved(PageId(page))
                    {
                        return Some(PageId(page));
                    }
                }
                from = at.saturating_add(1);
            }
        }
        None
    }

    /// Reports whether any page below `limit` is not allocated.
    ///
    /// @param limit - only pages below this number are considered
    pub fn has_free_below(&self, limit: u64) -> bool {
        self.lowest_free_below(limit).is_some()
    }

    /// Returns where a map page sits in the chain, when `page` is one.
    ///
    /// @param page - the page to look for
    pub fn chain_index_of(&self, page: PageId) -> Option<usize> {
        self.pages.iter().position(|(id, _)| *id == page)
    }

    /// Returns the map page that precedes the one at a chain index.
    ///
    /// @param index - a position in the chain
    pub fn chain_previous(&self, index: usize) -> Option<PageId> {
        index
            .checked_sub(1)
            .and_then(|before| self.pages.get(before))
            .map(|(id, _)| *id)
    }

    /// Gives a map page a new page number, keeping every bit it holds.
    ///
    /// The previous map page's `RIGHT` field is rewritten to name the new number.
    /// The caller changes the bits for the two pages in the map itself, sets the
    /// meta record's head when the first page moved, and installs the page at its
    /// new number. Returns the index of the page in the chain.
    ///
    /// @param from - the map page's current number
    /// @param to - the number it moves to
    pub fn relink_map_page(&mut self, from: PageId, to: PageId) -> DbResult<usize> {
        let index = self
            .chain_index_of(from)
            .ok_or_else(|| corrupt(format!("page {} is not a free-map page", from.0)))?;
        if let Some(entry) = self.pages.get_mut(index) {
            entry.0 = to;
        }
        if let Some(before) = index.checked_sub(1) {
            if let Some((_, previous)) = self.pages.get_mut(before) {
                page::set_right(previous, to)?;
            }
        }
        Ok(index)
    }

    /// Returns how many pages the chain can describe at all.
    ///
    /// A file may never grow past this without the chain growing first, which
    /// is what [`FreeMap::ensure`] arranges.
    pub fn described_pages(&self) -> u64 {
        (self.pages.len() as u64).saturating_mul(pages_per_map(self.page_size) as u64)
    }

    /// Returns how many pages the map describes as free.
    pub fn free_count(&self) -> u64 {
        let total = (self.pages.len() as u64).saturating_mul(pages_per_map(self.page_size) as u64);
        self.free_count_below(total)
    }

    /// Returns how many pages *of the file* the map describes as free.
    ///
    /// **The number `PRAGMA freelist_count` wants, and the reason it answered
    /// 261,883 on a five-page database.** [`FreeMap::free_count`] counts every
    /// bit the map has room for, and one map page over a 4 KiB file describes
    /// 32,736 pages whether or not the file has them - so it was reporting the
    /// map's capacity, and a `DELETE` could not move it. Free means a page the
    /// file has and nothing is using; a page id past the end of the file is not
    /// free, it does not exist.
    ///
    /// @param page_count - how many pages the file holds
    pub fn free_count_below(&self, page_count: u64) -> u64 {
        let total = (self.pages.len() as u64).saturating_mul(pages_per_map(self.page_size) as u64);
        (0..total.min(page_count))
            .filter(|page| !self.is_allocated(PageId(*page)))
            .count() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a map over a file of `pages` pages with the meta pages claimed.
    fn map_over(page_size: usize, pages: u64) -> (FreeMap, u64) {
        let mut map = FreeMap::new(page_size);
        let mut next = crate::meta::FIRST_DATA_PAGE.0;
        map.ensure(pages, &mut next).unwrap();
        (map, next)
    }

    /// **A page under the lock bytes is never handed out**, alone or inside a
    /// run, and freeing it does not make it available. On Windows a byte range
    /// lock blocks other handles from reading or writing those bytes, so a page
    /// holding data there fails reads with `disk I/O error` while another
    /// process has the file open.
    #[test]
    fn a_page_under_the_lock_bytes_is_never_handed_out() {
        let mut map = FreeMap::new(512).reserving(vec![40]);
        let mut next = crate::meta::FIRST_DATA_PAGE.0;
        map.ensure(200, &mut next).unwrap();
        let mut handed = Vec::new();
        while let Some(page) = map.allocate_run(1).unwrap() {
            handed.push(page.0);
            if handed.len() > 400 {
                break;
            }
        }
        assert!(!handed.contains(&40), "page 40 was handed out: {handed:?}");
        assert!(map.is_reserved(PageId(40)));
        map.free(PageId(40)).unwrap();
        assert!(
            map.is_allocated(PageId(40)),
            "a freed reserved page still reads as taken"
        );

        let mut runs = FreeMap::new(512).reserving(vec![40]);
        let mut next = crate::meta::FIRST_DATA_PAGE.0;
        runs.ensure(200, &mut next).unwrap();
        while let Some(first) = runs.allocate_run(7).unwrap() {
            assert!(
                !(first.0..first.0 + 7).contains(&40),
                "a run starting at {} spans the reserved page",
                first.0
            );
            if first.0 > 300 {
                break;
            }
        }
    }

    /// A map page is never placed on a page under the lock bytes either.
    #[test]
    fn a_map_page_skips_the_page_under_the_lock_bytes() {
        let mut map = FreeMap::new(512).reserving(vec![2]);
        let mut next = crate::meta::FIRST_DATA_PAGE.0;
        map.ensure(100, &mut next).unwrap();
        assert_eq!(
            map.first(),
            PageId(3),
            "the map page moved past the reserved page"
        );
        assert!(map.is_allocated(PageId(2)));
    }

    /// One map page describes the whole page minus the header, eight pages per
    /// byte.
    #[test]
    fn a_map_page_describes_the_page_in_bits() {
        assert_eq!(pages_per_map(512), (512 - 32) * 8);
        assert_eq!(pages_per_map(32_768), (32_768 - 32) * 8);
        assert_eq!(pages_per_map(0), 0);
    }

    /// The meta pages and the map page itself are allocated from the start, so
    /// nothing can hand them out.
    #[test]
    fn the_meta_pages_and_the_map_are_never_free() {
        let (map, next) = map_over(512, 100);
        assert!(map.is_allocated(PageId(0)));
        assert!(map.is_allocated(PageId(1)));
        assert!(map.is_allocated(PageId(2)), "the map page itself");
        assert_eq!(next, 3);
        assert!(!map.is_allocated(PageId(3)));
        assert_eq!(map.chain_len(), 1);
        assert_eq!(map.first(), PageId(2));
    }

    /// Allocation hands out the first free page and freeing gives it back.
    #[test]
    fn allocation_and_freeing_round_trip() {
        let (mut map, _) = map_over(512, 100);
        let first = map.allocate_run(1).unwrap().unwrap();
        assert_eq!(first, PageId(3));
        let second = map.allocate_run(1).unwrap().unwrap();
        assert_eq!(second, PageId(4));
        assert!(map.is_allocated(first));
        map.free(first).unwrap();
        assert!(!map.is_allocated(first));
        assert_eq!(map.allocate_run(1).unwrap(), Some(first));
    }

    /// A contiguous run skips over an allocated page rather than straddling it.
    #[test]
    fn a_run_is_contiguous() {
        let (mut map, _) = map_over(512, 100);
        map.allocate_at(PageId(5)).unwrap();
        let run = map.allocate_run(4).unwrap().unwrap();
        assert_eq!(run, PageId(6), "3 and 4 are free but 5 is not");
        for offset in 0..4 {
            assert!(map.is_allocated(PageId(6 + offset)));
        }
    }

    /// A run longer than the map has room for is refused rather than
    /// half-allocated.
    #[test]
    fn a_run_that_does_not_fit_is_refused() {
        let (mut map, _) = map_over(512, 40);
        let total = pages_per_map(512) as u64;
        assert_eq!(map.allocate_run(total + 1).unwrap(), None);
        assert_eq!(map.allocate_run(0).unwrap(), None);
        // Nothing was claimed by the refusal.
        assert!(!map.is_allocated(PageId(3)));
    }

    /// A page beyond the chain reads as allocated, so an allocator can never
    /// hand out a page the map does not describe.
    #[test]
    fn a_page_outside_the_map_is_allocated() {
        let (map, _) = map_over(512, 10);
        let beyond = pages_per_map(512) as u64 + 1;
        assert!(map.is_allocated(PageId(beyond)));
        let mut map = map;
        assert!(map.free(PageId(beyond)).is_err());
        assert!(map.allocate_at(PageId(beyond)).is_err());
    }

    /// Freeing a meta page is refused.
    #[test]
    fn a_meta_page_cannot_be_freed() {
        let (mut map, _) = map_over(512, 10);
        assert!(map.free(PageId(0)).is_err());
        assert!(map.free(PageId(1)).is_err());
        assert!(map.free(PageId(2)).is_ok(), "the map page is ordinary");
    }

    /// The chain grows to cover a file bigger than one map page, and the new
    /// map page is linked from the one before it.
    #[test]
    fn the_chain_grows_and_links() {
        let per = pages_per_map(512) as u64;
        let (map, _) = map_over(512, per * 2 + 5);
        assert!(map.chain_len() >= 2, "chain is {}", map.chain_len());
        let ids: Vec<PageId> = map.pages().map(|(id, _)| id).collect();
        let images: Vec<&[u8]> = map.pages().map(|(_, bytes)| bytes).collect();
        for index in 0..ids.len().saturating_sub(1) {
            assert_eq!(
                page::right_of(images[index]).unwrap(),
                ids[index + 1],
                "map page {index} does not link to the next"
            );
        }
        assert_eq!(page::right_of(images[ids.len() - 1]).unwrap(), PageId::NONE);
    }

    /// A map page read back from bytes must be a map page.
    #[test]
    fn a_pushed_page_must_be_a_map_page() {
        let mut map = FreeMap::new(512);
        let mut wrong = vec![0u8; 512];
        page::write_common(&mut wrong, PageKind::Leaf, 0, 0).unwrap();
        assert!(map.push_page(PageId(2), wrong).is_err());
        let mut short = vec![0u8; 256];
        page::write_common(&mut short, PageKind::FreeMap, 0, 0).unwrap();
        assert!(map.push_page(PageId(2), short).is_err());
        let mut right = vec![0u8; 512];
        page::write_common(&mut right, PageKind::FreeMap, 0, 0).unwrap();
        assert!(map.push_page(PageId(2), right).is_ok());
    }

    /// The free count matches what allocation and freeing did to it.
    #[test]
    fn the_free_count_tracks_allocation() {
        let (mut map, _) = map_over(512, 20);
        let before = map.free_count();
        map.allocate_run(3).unwrap();
        assert_eq!(map.free_count(), before - 3);
        map.free(PageId(3)).unwrap();
        assert_eq!(map.free_count(), before - 2);
    }

    /// The lowest free page below a limit is found, and a dense file has none.
    #[test]
    fn the_lowest_free_page_is_found_below_a_limit() {
        let (mut map, _) = map_over(512, 100);
        for _ in 0..30 {
            map.allocate_run(1).unwrap();
        }
        // Pages 3 to 32 are taken. The lowest free page is 33.
        assert_eq!(map.lowest_free_below(100), Some(PageId(33)));
        assert!(map.has_free_below(34));
        assert!(!map.has_free_below(33));
        map.free(PageId(7)).unwrap();
        assert_eq!(map.lowest_free_below(100), Some(PageId(7)));
        assert_eq!(map.lowest_free_below(7), None);
    }

    /// A relinked map page keeps every bit, and the chain is rewritten around it.
    #[test]
    fn a_relinked_map_page_keeps_every_bit() {
        let per = pages_per_map(512) as u64;
        let (mut map, _) = map_over(512, per * 2 + 5);
        assert!(map.chain_len() >= 3);
        let before: Vec<bool> = (0..per * 2).map(|n| map.is_allocated(PageId(n))).collect();
        let ids: Vec<PageId> = map.pages().map(|(id, _)| id).collect();
        let middle = ids[1];
        let to = PageId(per * 2 + 50);
        let index = map.relink_map_page(middle, to).unwrap();
        assert_eq!(index, 1);
        assert_eq!(map.chain_previous(1), Some(ids[0]));
        let after: Vec<(PageId, &[u8])> = map.pages().collect();
        assert_eq!(after[1].0, to);
        assert_eq!(page::right_of(after[0].1).unwrap(), to);
        let after_bits: Vec<bool> = (0..per * 2).map(|n| map.is_allocated(PageId(n))).collect();
        assert_eq!(before, after_bits);
        assert!(map.relink_map_page(PageId(1), to).is_err());
    }
}

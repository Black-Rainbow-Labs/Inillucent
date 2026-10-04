//! Packed copies of leaves that took writes, kept for the readers that come back to them.
//!
//! Invariant: **a copy is handed out only for the page and generation it was
//! made from.** Every change to a frame's bytes gives the frame a new
//! generation (a load, an install, a `modify`), and the generation comes from
//! one counter the pool never winds back, so a page whose bytes moved since a
//! copy was made can never match that copy again. Nothing here is written to
//! the file or the log: a copy is a reader's private view of bytes the pool
//! already holds.
//!
//! ## Why it exists
//!
//! A leaf that took inserts holds its newest rows in a delta area, and every
//! read of it merges the delta area into the sorted rows: a full decode of
//! each delta row and a search for its place (task-2183). A table an
//! application has just filled is read that way by every query until the leaf
//! next compacts, which may be never. The hill climb's `ai.group.newest` read
//! the same four index leaves a thousand times and merged them a thousand
//! times. The tree packs a leaf into a copy the second time it is read at one
//! generation, and the reads after that take the path a clean leaf takes.
//!
//! ## What it costs
//!
//! At most [`MERGED_CAPACITY`] copies of one page each, 2 MiB at 32 KiB pages.
//! A copy is made only on a second sighting, so a scan that reads each leaf
//! once never builds one, and a full cache takes a new copy only into the room
//! of a copy whose page has changed.

use std::rc::Rc;

use crate::PageId;

/// The pool's side of the copies: the frame generations they are checked against.
///
/// A frame's generation is a new number from the pool's counter every time
/// its bytes are given a page or changed: a load, an install, a `modify`. Two
/// reads that see one generation saw the same bytes.
impl super::Pool {
    /// Returns a new frame generation, never handed out before.
    pub(super) fn next_generation(&self) -> u64 {
        let next = self.generations.get().wrapping_add(1);
        self.generations.set(next);
        next
    }

    /// Returns the generation of a resident page's bytes, or `None` when it is not resident.
    ///
    /// Does not load the page and does not warm it.
    ///
    /// @param page - the page
    pub fn generation_of(&self, page: PageId) -> Option<u64> {
        let state = self.state.borrow();
        let frame = *state.table.get(&page)?;
        state
            .frames
            .get(frame as usize)
            .filter(|meta| meta.page == page)
            .map(|meta| meta.generation)
    }

    /// Returns the packed copy of a written leaf made at its current generation, if one is kept.
    ///
    /// @param page - the leaf
    /// @param generation - the generation the caller read, from [`Pool::generation_of`]
    pub fn merged_copy(&self, page: PageId, generation: u64) -> Option<std::rc::Rc<[u8]>> {
        self.merged.borrow_mut().get(page, generation)
    }

    /// Records a read of a written leaf and says whether it was read before at this generation.
    ///
    /// A leaf read twice is worth packing a copy of; one read once is not.
    ///
    /// @param page - the leaf
    /// @param generation - the generation the caller read
    pub fn merged_sighting(&self, page: PageId, generation: u64) -> bool {
        self.merged.borrow_mut().sighted(page, generation)
    }

    /// Keeps a packed copy of a written leaf for the readers after this one.
    ///
    /// @param page - the leaf
    /// @param generation - the generation the copy was made from
    /// @param image - the packed page
    pub fn keep_merged_copy(&self, page: PageId, generation: u64, image: std::rc::Rc<[u8]>) {
        let current = |held: PageId| self.generation_of(held);
        self.merged
            .borrow_mut()
            .keep(page, generation, image, &current);
    }

    /// Gives a frame a new generation, before its bytes change.
    ///
    /// Before rather than after, so a change that fails half way still leaves
    /// a generation no copy was made from.
    ///
    /// @param frame - the frame about to change
    pub(super) fn renew_generation(&self, frame: u32) {
        let generation = self.next_generation();
        self.state
            .borrow_mut()
            .amend(frame, |meta| meta.generation = generation);
    }
}

/// The most packed copies kept at once.
pub(crate) const MERGED_CAPACITY: usize = 64;

/// How many recent first sightings are remembered.
const SIGHTINGS: usize = 64;

/// One packed copy.
struct Copy {
    /// The page it is a copy of.
    page: PageId,
    /// The generation the page had when the copy was made.
    generation: u64,
    /// The packed page.
    image: Rc<[u8]>,
}

/// The copies, and the pages seen once.
#[derive(Default)]
pub(super) struct MergedCache {
    /// The copies, at most [`MERGED_CAPACITY`].
    copies: Vec<Copy>,
    /// Pages and generations read once, oldest overwritten first.
    seen: Vec<(PageId, u64)>,
    /// Where the next sighting goes in `seen` once it is full.
    next_seen: usize,
}

impl MergedCache {
    /// Returns the copy of a page at a generation, if there is one.
    ///
    /// @param page - the page
    /// @param generation - the generation the frame holds now
    pub(super) fn get(&mut self, page: PageId, generation: u64) -> Option<Rc<[u8]>> {
        let at = self.copies.iter().position(|copy| copy.page == page)?;
        if self.copies.get(at)?.generation != generation {
            // The page has moved on; this copy will never match again.
            self.copies.swap_remove(at);
            return None;
        }
        self.copies.get(at).map(|copy| Rc::clone(&copy.image))
    }

    /// Records a read of a page at a generation, and says whether it was read before.
    ///
    /// @param page - the page
    /// @param generation - the generation the frame holds now
    pub(super) fn sighted(&mut self, page: PageId, generation: u64) -> bool {
        if self.seen.contains(&(page, generation)) {
            return true;
        }
        if self.seen.len() < SIGHTINGS {
            self.seen.push((page, generation));
        } else if let Some(slot) = self.seen.get_mut(self.next_seen) {
            *slot = (page, generation);
            self.next_seen = (self.next_seen + 1) % SIGHTINGS;
        }
        false
    }

    /// Keeps a copy, replacing the copy of the same page or a copy that has gone stale.
    ///
    /// **A full cache keeps what it has (task-2183).** A scan of a table larger
    /// than the cache sees each leaf once per pass, so replacing the copy used
    /// longest ago would replace every copy on every pass and keep none long
    /// enough to be read. A copy goes when its page changes; `current` says
    /// which copies have, and their room is reused.
    ///
    /// @param page - the page
    /// @param generation - the generation the copy was made from
    /// @param image - the packed page
    /// @param current - a page's generation now, or `None` when it is no longer resident
    pub(super) fn keep(
        &mut self,
        page: PageId,
        generation: u64,
        image: Rc<[u8]>,
        current: &dyn Fn(PageId) -> Option<u64>,
    ) {
        let copy = Copy {
            page,
            generation,
            image,
        };
        if let Some(at) = self.copies.iter().position(|held| held.page == page) {
            if let Some(slot) = self.copies.get_mut(at) {
                *slot = copy;
            }
            return;
        }
        if self.copies.len() >= MERGED_CAPACITY {
            self.copies
                .retain(|held| current(held.page) == Some(held.generation));
        }
        if self.copies.len() < MERGED_CAPACITY {
            self.copies.push(copy);
        }
    }

    /// Drops every copy and every sighting.
    pub(super) fn clear(&mut self) {
        self.copies.clear();
        self.seen.clear();
        self.next_seen = 0;
    }

    /// How many copies are held.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.copies.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A copy answers only for the generation it was made from, and goes when it is asked about another.
    #[test]
    fn a_copy_answers_only_for_its_generation() {
        let mut cache = MergedCache::default();
        let image: Rc<[u8]> = Rc::from(vec![1u8, 2, 3]);
        assert!(!cache.sighted(PageId(7), 4));
        assert!(cache.sighted(PageId(7), 4));
        assert!(
            !cache.sighted(PageId(7), 5),
            "a new generation is a new sighting"
        );
        cache.keep(PageId(7), 4, Rc::clone(&image), &|_| Some(4));
        assert_eq!(cache.get(PageId(7), 4).as_deref(), Some(&[1u8, 2, 3][..]));
        assert!(cache.get(PageId(7), 5).is_none());
        assert_eq!(
            cache.len(),
            0,
            "a stale copy is dropped when it is found stale"
        );
    }

    /// A full cache keeps its copies, and makes room only from copies whose page has changed.
    #[test]
    fn a_full_cache_keeps_what_it_has_until_a_page_changes() {
        let mut cache = MergedCache::default();
        let unchanged = |_: PageId| Some(1);
        for page in 0..MERGED_CAPACITY as u64 {
            cache.keep(PageId(page), 1, Rc::from(vec![page as u8]), &unchanged);
        }
        cache.keep(PageId(1_000), 1, Rc::from(vec![9u8]), &unchanged);
        assert_eq!(cache.len(), MERGED_CAPACITY);
        assert!(
            cache.get(PageId(1_000), 1).is_none(),
            "a scan does not push copies out"
        );
        assert!(cache.get(PageId(0), 1).is_some());
        // Page 3 changed, so its copy is stale and its room is reused.
        let changed = |page: PageId| if page == PageId(3) { Some(2) } else { Some(1) };
        cache.keep(PageId(1_000), 1, Rc::from(vec![9u8]), &changed);
        assert!(cache.get(PageId(1_000), 1).is_some());
        assert!(cache.get(PageId(3), 1).is_none());
    }
}

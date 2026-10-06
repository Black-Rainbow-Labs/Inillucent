//! Getting from the root of a b-tree to the leaf a key is in.
//!
//! Invariant: **a descent compares with the same function that wrote the
//! separators.** [`PagedTree::encode_key`] is used by the builder and by every
//! comparison here, so a tree cannot be written under one ordering and read
//! under another - which would route a descent to the wrong leaf and answer
//! *no row* rather than an error.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::interior::InteriorRef;
use inillucent_pool::page::{self, PageKind};
use inillucent_pool::{PageGuard, PageId, Pool, Swip};

use super::*;

impl PagedTree {
    /// Encodes a key tuple without allocating when it fits inline.
    ///
    /// The hot form. [`PagedTree::encode_key`] stays for the callers that want
    /// an owned `Vec` - the bulk builder's separators, which are kept - and
    /// this one is what every descent uses.
    ///
    /// @param values - the key tuple, in key-column order
    pub fn encode_key_small(&self, values: &[Datum<'_>]) -> KeyBytes {
        if let (KeyEncoding::Rowid, [Datum::Int(number)]) = (self.encoding, values) {
            let mut inline = [0u8; KeyBytes::INLINE];
            if let Some(slot) = inline.get_mut(..8) {
                slot.copy_from_slice(&key::order_preserving_int(*number));
            }
            return KeyBytes::Inline(inline, 8);
        }
        if self.encoding == KeyEncoding::General {
            let mut scratch = self.scratch.borrow_mut();
            scratch.clear();
            for (index, value) in values.iter().enumerate() {
                let collation = self.collations.get(index).copied().unwrap_or_default();
                key::encode_into_with(value, collation, &mut scratch);
            }
            return KeyBytes::from_slice(&scratch);
        }
        KeyBytes::from_slice(&self.encoding.encode_ordered(
            values,
            &self.collations,
            &self.directions,
        ))
    }

    /// Returns the leftmost leaf, where a full scan starts.
    ///
    /// @param pool - the buffer pool, read the first time it is asked
    pub fn first_leaf(&self, pool: &Pool) -> DbResult<PageId> {
        self.shape(pool).map(|(_, leaf)| leaf)
    }
    /// Descends to the leaf whose range holds a key, returning the pinned leaf.
    ///
    /// The guard comes back rather than the page id alone because every caller
    /// wants to read the leaf next, and re-fetching it would be a second page
    /// lookup for a page the descent is already holding. On `point.rowid` -
    /// the workload with the tightest budget in this phase's gate - that one
    /// lookup is a measurable share of the whole probe.
    ///
    /// @param pool - the buffer pool
    /// @param key - the encoded key being looked for
    pub fn descend_guard<'p>(
        &self,
        pool: &'p Pool,
        key: &[u8],
    ) -> DbResult<(PageGuard<'p>, PageId)> {
        for attempt in 0..=RESTARTS {
            match self.try_descend(pool, key, attempt == RESTARTS)? {
                Some(found) => return Ok(found),
                None => continue,
            }
        }
        Err(corrupt(
            "a descent restarted more times than the tree is deep",
        ))
    }
    /// Returns the leftmost leaf of a tree, taking the first child at each level.
    ///
    /// Reads the child pointer directly rather than going through
    /// [`PagedTree::take_child`], because that swizzles the parent's slot as a
    /// side effect and this walk happens once, at open, before any descent has
    /// a reason to want the pointer warm.
    ///
    /// @param pool - the buffer pool
    /// @param root - the root page
    pub(crate) fn leftmost_leaf(pool: &Pool, root: PageId) -> DbResult<PageId> {
        let mut page = root;
        for _ in 0..=64 {
            let next = {
                let guard = pool.fetch(page)?;
                match page::kind_of(&guard)? {
                    PageKind::Leaf => return Ok(page),
                    PageKind::Interior => {
                        let interior = InteriorRef::parse(&guard)?;
                        pool.page_of_swip(interior.swip(0)?)?
                    }
                    other => return Err(corrupt(format!("a tree cannot contain {other:?}"))),
                }
            };
            page = next;
        }
        Err(corrupt("a tree is deeper than 64 levels"))
    }
    /// Descends to the leaf whose range holds a key, recording the path taken.
    ///
    /// The path costs one `Vec` push per level and is only wanted by a reverse
    /// walk, which is why it is a separate entry point from
    /// [`PagedTree::descend_guard`] rather than something every point probe
    /// pays for.
    ///
    /// @param pool - the buffer pool
    /// @param key - the encoded key being looked for
    pub fn descend(&self, pool: &Pool, key: &[u8]) -> DbResult<Descent> {
        let mut steps =
            Vec::with_capacity(self.shape.get().map_or(4, |(height, _)| height as usize));
        let mut page = self.root;
        loop {
            let child = {
                let guard = pool.fetch(page)?;
                if page::kind_of(&guard)? == PageKind::Leaf {
                    return Ok(Descent { steps, leaf: page });
                }
                InteriorRef::parse(&guard)?.child_for(key)?.0
            };
            steps.push((page, child));
            page = self.take_child(pool, page, child)?;
            if steps.len() > 64 {
                return Err(corrupt("a descent walked more than 64 levels"));
            }
        }
    }
    /// [`PagedTree::descend`], also answering the leaf's fences: the separator
    /// below it, which is the lowest key that belongs in it, and the separator
    /// above it, which is the first key that does not. `None` is an edge of the
    /// tree.
    ///
    /// The descent already reads both, because they are what chose the child at
    /// every level. A separator is the first key of the child to its right, so
    /// child `c` holds the keys from separator `c - 1` up to but not including
    /// separator `c`, and a deeper level only narrows what a higher one said.
    ///
    /// @param pool - the buffer pool
    /// @param key - the encoded key being looked for
    pub(crate) fn descend_fenced(
        &self,
        pool: &Pool,
        key: &[u8],
    ) -> DbResult<(Descent, Option<Vec<u8>>, Option<Vec<u8>>)> {
        let mut steps =
            Vec::with_capacity(self.shape.get().map_or(4, |(height, _)| height as usize));
        let mut low: Option<Vec<u8>> = None;
        let mut high: Option<Vec<u8>> = None;
        let mut page = self.root;
        loop {
            let child = {
                let guard = pool.fetch(page)?;
                if page::kind_of(&guard)? == PageKind::Leaf {
                    return Ok((Descent { steps, leaf: page }, low, high));
                }
                let interior = InteriorRef::parse(&guard)?;
                let child = interior.child_for(key)?.0;
                if let Some(below) = child.checked_sub(1) {
                    low = Some(interior.key(below)?.to_vec());
                }
                if child < interior.count() {
                    high = Some(interior.key(child)?.to_vec());
                }
                child
            };
            steps.push((page, child));
            page = self.take_child(pool, page, child)?;
            if steps.len() > 64 {
                return Err(corrupt("a descent walked more than 64 levels"));
            }
        }
    }
    /// One attempt at a descent.
    ///
    /// Returns `None` when an optimistic read failed validation, which means
    /// the caller should start again. On the last attempt the reader takes
    /// shared latches instead, so a writer that keeps invalidating cannot
    /// starve it - the TDD's "after 4 restarts they descend with shared
    /// latches".
    ///
    /// The guard on the level above is held until the level below is pinned, so
    /// a pool too small to hold the path cannot evict a page this descent is
    /// still walking through. It is then dropped *before* the swizzle, because
    /// reading a frame and writing it are exclusive - and the swizzle re-checks
    /// that the frame still holds the page it was about to annotate.
    ///
    /// @param pool - the buffer pool
    /// @param key - the encoded key
    /// @param pessimistic - whether to take shared latches rather than observe
    fn try_descend<'p>(
        &self,
        pool: &'p Pool,
        key: &[u8],
        pessimistic: bool,
    ) -> DbResult<Option<(PageGuard<'p>, PageId)>> {
        let mut page = self.root;
        let mut guard = pool.fetch(page)?;
        loop {
            let frame = guard.frame();
            let observed = if pessimistic {
                None
            } else {
                pool.observe(frame)
            };
            if !pessimistic && observed.is_none() {
                return Ok(None);
            }
            if page::kind_of(&guard)? == PageKind::Leaf {
                if let Some(seen) = observed {
                    if !pool.validate(frame, seen) {
                        return Ok(None);
                    }
                }
                return Ok(Some((guard, page)));
            }
            let (swip, at) = {
                let interior = InteriorRef::parse(&guard)?;
                let (_, swip, at) = interior.child_for(key)?;
                (swip, at)
            };
            if let Some(seen) = observed {
                if !pool.validate(frame, seen) {
                    return Ok(None);
                }
            }
            // A swizzled swip names the frame directly, which is the whole
            // point of swizzling: no page-table lookup, no page id to resolve.
            //
            // `already` is why the write below is conditional. A slot that
            // already names this frame is a slot the swizzle would rewrite with
            // the bytes it already holds, and paying for that on every descent
            // is not free: it is a page-table lookup to re-check the parent, an
            // exclusive borrow of the parent's buffer, and a store into an
            // interior page that every later descent then has to re-read. The
            // first descent through a slot does the work; the millionth does
            // not need to repeat it.
            let (child_guard, target, already) = match swip.frame() {
                Some(child_frame) => {
                    let child_guard = pool.fetch_frame(child_frame)?;
                    let target = pool
                        .page_in_frame(child_frame)
                        .filter(|page| !page.is_none())
                        .ok_or_else(|| corrupt("a swizzled swip names an empty frame"))?;
                    (child_guard, target, true)
                }
                None => {
                    let target = pool.page_of_swip(swip)?;
                    let child_guard = pool.fetch(target)?;
                    pool.note_parent(child_guard.frame(), frame, page, at);
                    (child_guard, target, false)
                }
            };
            let child_frame = child_guard.frame();
            let parent_page = page;
            drop(guard);
            if !already {
                pool.swizzle_into(frame, parent_page, at, Swip::swizzled(child_frame))?;
            }
            guard = child_guard;
            page = target;
        }
    }
    /// Follows one child swip, swizzling the parent's slot.
    ///
    /// The shared half of [`PagedTree::try_descend`], used by the walks that
    /// take a fixed child rather than searching for one: the rightmost spine
    /// and the step-left of a reverse scan.
    ///
    /// @param pool - the buffer pool
    /// @param parent - the parent page
    /// @param child - which child to take
    fn take_child(&self, pool: &Pool, parent: PageId, child: usize) -> DbResult<PageId> {
        let (frame, at, target, child_frame) = {
            let guard = pool.fetch(parent)?;
            let frame = guard.frame();
            let interior = InteriorRef::parse(&guard)?;
            let swip = interior.swip(child)?;
            let at = interior.swip_offset(child)?;
            let target = pool.page_of_swip(swip)?;
            let child_guard = pool.fetch(target)?;
            let child_frame = child_guard.frame();
            pool.note_parent(child_frame, frame, parent, at);
            (frame, at, target, child_frame)
        };
        pool.swizzle_into(frame, parent, at, Swip::swizzled(child_frame))?;
        Ok(target)
    }
    /// Descends the rightmost spine.
    ///
    /// @param pool - the buffer pool
    pub(crate) fn descend_rightmost(&self, pool: &Pool) -> DbResult<Descent> {
        let mut steps = Vec::new();
        let mut page = self.root;
        loop {
            let child = {
                let guard = pool.fetch(page)?;
                if page::kind_of(&guard)? == PageKind::Leaf {
                    return Ok(Descent { steps, leaf: page });
                }
                InteriorRef::parse(&guard)?.count()
            };
            steps.push((page, child));
            page = self.take_child(pool, page, child)?;
            if steps.len() > 64 {
                return Err(corrupt("a rightmost descent walked more than 64 levels"));
            }
        }
    }
    /// Moves a descent to the leaf before the one it is on.
    ///
    /// Returns false when there is none, which is the left edge of the tree.
    ///
    /// @param pool - the buffer pool
    /// @param descent - the path, updated in place
    pub(crate) fn step_left(&self, pool: &Pool, descent: &mut Descent) -> DbResult<bool> {
        while let Some((page, index)) = descent.steps.pop() {
            if index == 0 {
                continue;
            }
            let child = index.saturating_sub(1);
            descent.steps.push((page, child));
            let mut next = self.take_child(pool, page, child)?;
            // Down the rightmost spine of that subtree.
            loop {
                let count = {
                    let guard = pool.fetch(next)?;
                    if page::kind_of(&guard)? == PageKind::Leaf {
                        descent.leaf = next;
                        break;
                    }
                    InteriorRef::parse(&guard)?.count()
                };
                descent.steps.push((next, count));
                next = self.take_child(pool, next, count)?;
                if descent.steps.len() > 64 {
                    return Err(corrupt("a left step walked more than 64 levels"));
                }
            }
            return Ok(true);
        }
        Ok(false)
    }
}

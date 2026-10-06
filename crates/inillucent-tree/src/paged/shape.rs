//! What a tree learns about itself from its pages and keeps until a page
//! says otherwise: its height and leftmost leaf, its largest integer key, and
//! the leaves its recent writes went to.
//!
//! Invariant: **a remembered fact is either read from the pages on first use
//! or checked against a page before it is used.** The height and the leftmost
//! leaf are read the first time something asks and moved by the splits that
//! move them. The largest key is used only while the leaf it was read from is
//! still the rightmost leaf of the tree and carries the log position it had.
//! Both were added by task-2191: an open used to read two pages for every tree,
//! and an insert with no rowid used to descend to the rightmost leaf every
//! time.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::page::{self, PageKind};
use inillucent_pool::{PageId, Pool};

use super::*;

/// How many leaves one connection remembers per tree for its next write.
///
/// Eight, because the miss path is what a larger set costs: every entry is two
/// comparisons against bytes the caller already has, and they are paid in full by a key
/// that is in none of the windows. Eight covers a secondary index whose rows arrive in
/// another index's order while leaving that miss at a handful of `memcmp`s.
pub(crate) const LEAF_HINTS: usize = 8;

/// The leaf a write is likely to want next, and the fences that prove it.
///
/// See [`PagedTree::leaf_hint`] for the argument. Held rather than derived because
/// the proof is two comparisons and deriving it is a descent.
#[derive(Clone, Debug)]
pub(crate) struct LeafHint {
    /// The leaf.
    pub(crate) page: PageId,
    /// Its low fence, the lowest key that belongs in it, in comparable bytes.
    /// Empty for the leftmost leaf, whose range starts at negative infinity.
    pub(crate) low: Vec<u8>,
    /// Its high fence, the first key that does not belong in it. Read only when
    /// `bounded` says there is one.
    pub(crate) high: Vec<u8>,
    /// Whether the leaf has a high fence, which every leaf but the rightmost does.
    pub(crate) bounded: bool,
    /// The right sibling it had when it was recorded.
    ///
    /// Two jobs. `PageId::NONE` means the leaf was the rightmost one, whose range runs
    /// to positive infinity, so the high end of the window is unnecessary for it - an
    /// append is above every probe seen so far and a window would reject it.
    ///
    /// And it is **how a split by anybody is detected**. A split of this leaf points it
    /// at the new page, and a merge changes it too, so a sibling that still matches is
    /// a leaf whose fence range has not moved since the window was proved. That covers
    /// the one case the window's own argument cannot: another process splitting this
    /// leaf between two of our statements. It costs nothing, because the hit path
    /// already reads this page's header to check the page is still a leaf of this tree.
    pub(crate) right: PageId,
}

/// What [`PagedTree::largest_hint`] remembers.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LargestHint {
    /// The rightmost leaf the key was read from.
    page: PageId,
    /// The leaf's log position when it was read.
    lsn: u64,
    /// The largest integer key the tree held.
    largest: i64,
}

impl PagedTree {
    /// Returns the height and the leftmost leaf, reading them from the pages
    /// the first time.
    ///
    /// The leftmost leaf is read out of the file, never taken on trust. It was
    /// once taken from the catalog, whose statistics are written at a
    /// checkpoint, and a root split moves it: a database closed without a
    /// checkpoint reopened with it naming the root, which was an interior page
    /// by then, and every read of it failed with "page is not a leaf".
    ///
    /// @param pool - the buffer pool
    pub(crate) fn shape(&self, pool: &Pool) -> DbResult<(u16, PageId)> {
        if let Some(shape) = self.shape.get() {
            return Ok(shape);
        }
        let guard = pool.fetch(self.root)?;
        let height = match page::kind_of(&guard)? {
            PageKind::Leaf => 0,
            PageKind::Interior => page::level_of(&guard)?,
            other => return Err(corrupt(format!("a tree root cannot be {other:?}"))),
        };
        drop(guard);
        let first_leaf = PagedTree::leftmost_leaf(pool, self.root)?;
        self.shape.set(Some((height, first_leaf)));
        Ok((height, first_leaf))
    }

    /// Returns the largest integer key, when the leaf it was read from is
    /// still the rightmost leaf of this tree and has not changed since.
    ///
    /// `None` means the caller reads the tree. See [`PagedTree::largest_hint`].
    ///
    /// @param pool - the buffer pool
    pub fn hinted_largest_key(&self, pool: &Pool) -> Option<i64> {
        let hint = self.largest_hint.get()?;
        let guard = pool.fetch(hint.page).ok()?;
        let holds = page::kind_of(&guard).ok() == Some(PageKind::Leaf)
            && page::tree_of(&guard).ok() == Some(self.tree_id)
            && page::right_of(&guard).ok() == Some(PageId::NONE)
            && page::lsn_of(&guard).ok() == Some(hint.lsn);
        if !holds {
            self.largest_hint.set(None);
            return None;
        }
        Some(hint.largest)
    }

    /// Remembers the largest integer key, read from a leaf the caller holds.
    ///
    /// Kept only when the leaf is the tree's rightmost one and carries a log
    /// position. A page with none, as a temporary table's may, could change
    /// without the position moving, so nothing is remembered for it.
    ///
    /// @param page - the leaf
    /// @param bytes - the leaf's bytes
    /// @param largest - the largest key it holds
    pub fn note_largest_key(&self, page: PageId, bytes: &[u8], largest: i64) {
        let lsn = page::lsn_of(bytes).unwrap_or(0);
        let rightmost = page::right_of(bytes).ok() == Some(PageId::NONE)
            && page::tree_of(bytes).ok() == Some(self.tree_id);
        self.largest_hint.set(match rightmost && lsn != 0 {
            true => Some(LargestHint { page, lsn, largest }),
            false => None,
        });
    }

    /// Moves the remembered largest key up to a key this tree just appended.
    ///
    /// Called right after the append, with nothing else written between, by
    /// a caller that saw the hint hold before it: the leaf then holds what it
    /// held plus this key, so the key is the largest.
    ///
    /// @param pool - the buffer pool
    /// @param key - the key appended
    pub(crate) fn note_appended_key(&self, pool: &Pool, key: i64) {
        let Some(hint) = self.largest_hint.get() else {
            return;
        };
        let refreshed = pool.fetch(hint.page).ok().and_then(|guard| {
            let still = page::kind_of(&guard).ok() == Some(PageKind::Leaf)
                && page::right_of(&guard).ok() == Some(PageId::NONE);
            let lsn = page::lsn_of(&guard).ok().filter(|lsn| *lsn != 0)?;
            still.then_some(LargestHint {
                page: hint.page,
                lsn,
                largest: key,
            })
        });
        self.largest_hint.set(refreshed);
    }

    /// Returns the leftmost leaf when it has been read, and `None` when the
    /// pages have not been asked yet.
    pub(crate) fn known_first_leaf(&self) -> Option<PageId> {
        self.shape.get().map(|(_, leaf)| leaf)
    }
}

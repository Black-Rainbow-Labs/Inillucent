//! Taking leaves a delete emptied out of the tree, without copying their right siblings into them.
//!
//! Invariant: **every row reachable before an unlink is reachable after it through the same
//! leaf, and the leaf chain skips exactly the pages that were freed.** An emptied leaf holds no
//! live row, so the only things that name it are its left neighbour's right link and one child
//! pointer in its parent. An unlink rewrites both and frees the page. The separator taken out of
//! the parent with each leaf is the one on its left, so the leaf before a run of emptied leaves
//! takes over their ranges, which held nothing.
//!
//! **Why not the merge** (task-2226). Since a delete from the front of a table was made to free
//! the pages it empties (task-2217), every emptied leaf was merged with its right sibling: the
//! sibling's rows were packed into the emptied page, and the merge was logged as three whole page
//! images. A range delete empties leaf after leaf, and the right sibling of each is the next leaf
//! the delete will empty, so `DELETE FROM copied WHERE id BETWEEN 20000 AND 59999` copied about
//! 165 full leaves and logged about 16 MB to remove 40,000 rows. That was 70% of the statement,
//! and the statement took 4.3 times as long as SQLite's.
//!
//! **And not one leaf at a time.** Unlinking each leaf as it emptied still logged the parent page
//! whole once per leaf, which was a third of the delete. A sorted delete therefore only notes the
//! leaves it empties, and the leaves under one parent are unlinked together, with one parent page
//! logged: when the delete moves on to a leaf under another parent, and when the caller says the
//! statement's deletes from this tree are done (`PagedTree::finish_deletes`). A leaf on the list
//! is still a valid empty leaf of the tree until then, so a statement that stops part way leaves
//! a tree that is whole, and the next unlink checks each leaf again before it acts on it.
//!
//! The left neighbour's right link is changed by a `Relink` record, which the replay applies to
//! that one field by the page LSN rule (`crate::relocate::apply`). The parent is logged whole as a
//! `WritePage` record and each freed page as a `FreePage` record, as a dissolve logs them. So the
//! log format does not change.
//!
//! An unlink needs the left neighbour under the same parent, so the first child of a parent, the
//! last leaf of the tree, a leaf that still holds an out of line value of a dead row, and a leaf
//! whose links do not say what the parent says are left to the merge or left in place.

use inillucent_base::DbResult;
use inillucent_pool::extent::ExtentRef;
use inillucent_pool::page;
use inillucent_pool::{Database, PageId};
use inillucent_wal::record::{Body, RelinkSite};

use super::TreeLog;
use crate::leaf::{LeafBuilder, LeafRef};
use crate::paged::PagedTree;
use crate::relocate::apply_relink;
use crate::split_log::StoredSlot;

impl PagedTree {
    /// Takes an emptied leaf out of the tree now when its left neighbour is under the same
    /// parent, and reports whether it did.
    ///
    /// For a delete that is not sorted, which empties a leaf and does not come back for the next.
    ///
    /// @param database - the file and its pool
    /// @param log - where the records go
    /// @param page - the leaf the delete emptied
    /// @param path - the interior pages above it, root first
    pub(super) fn unlink_emptied_leaf(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        page: PageId,
        path: &[PageId],
    ) -> DbResult<bool> {
        Ok(self.unlink_under_parent(database, log, &[page], path)? > 0)
    }

    /// Notes a leaf a sorted delete emptied, to be unlinked with the others under its parent, and
    /// reports whether it was noted; one that cannot be unlinked is left to the caller.
    ///
    /// Unlinks the leaves already noted first when this one is under another parent, because the
    /// delete has then left their parent behind.
    ///
    /// @param database - the file and its pool
    /// @param log - where the records go
    /// @param page - the leaf the delete emptied
    /// @param path - the interior pages above it, root first
    pub(super) fn defer_unlink(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        page: PageId,
        path: Vec<PageId>,
    ) -> DbResult<bool> {
        let Some(parent) = path.last().copied() else {
            return Ok(false);
        };
        let (_, children, _) = self.read_interior(database.pool(), parent)?;
        let first_child = children.first() == Some(&page);
        if first_child || !children.contains(&page) || !self.leaf_is_unlinkable(database, page)? {
            return Ok(false);
        }
        // **A noted leaf stays dirty in the pool until it is unlinked**, and a dirty page of an
        // uncommitted statement cannot be evicted, so the list is kept to an eighth of the pool.
        // Without the bound a delete of most of 2,000 rows through a 64 frame pool filled every
        // frame with emptied leaves and failed with "out of memory" (`vacuum_on_vfs`).
        let most = (database.pool().budget() / 8).max(1);
        let (elsewhere, full) = {
            let pending = self.pending_unlinks.borrow();
            (
                pending
                    .last()
                    .is_some_and(|(_, held)| held.last() != Some(&parent)),
                pending.len() >= most,
            )
        };
        if elsewhere || full {
            self.finish_deletes(database, log)?;
        }
        self.pending_unlinks.borrow_mut().push((page, path));
        Ok(true)
    }

    /// Unlinks every leaf the sorted deletes of this tree noted and has not unlinked yet.
    ///
    /// The caller calls it when it has no more keys to delete from this tree in this statement.
    /// A leaf left on the list is a valid empty leaf, so a caller that never calls it leaves the
    /// pages in the tree rather than breaking it, and the next sorted delete unlinks them.
    ///
    /// @param database - the file and its pool
    /// @param log - where the records go
    pub fn finish_deletes(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
    ) -> DbResult<()> {
        let pending = std::mem::take(&mut *self.pending_unlinks.borrow_mut());
        let mut at = 0usize;
        while let Some((_, path)) = pending.get(at) {
            let parent = path.last().copied();
            let pages: Vec<PageId> = pending
                .iter()
                .skip(at)
                .take_while(|(_, held)| held.last().copied() == parent)
                .map(|(page, _)| *page)
                .collect();
            at = at.saturating_add(pages.len().max(1));
            self.unlink_under_parent(database, log, &pages, path)?;
        }
        Ok(())
    }

    /// Unlinks the leaves of a list that are still emptied children of the parent at the end of
    /// `path`, with one write of the parent, and returns how many it unlinked.
    ///
    /// @param database - the file and its pool
    /// @param log - where the records go
    /// @param pages - the emptied leaves, all under one parent
    /// @param path - the interior pages above them, root first
    fn unlink_under_parent(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        pages: &[PageId],
        path: &[PageId],
    ) -> DbResult<usize> {
        let Some(parent) = path.last().copied() else {
            return Ok(0);
        };
        let (separators, children, level) = self.read_interior(database.pool(), parent)?;
        let mut doomed = vec![false; children.len()];
        for page in pages {
            if let Some(at) = children.iter().position(|held| held == page) {
                if at >= 1 && self.leaf_is_unlinkable(database, *page)? {
                    if let Some(slot) = doomed.get_mut(at) {
                        *slot = true;
                    }
                }
            }
        }
        let relinks = self.relinks_for(database, &children, &mut doomed)?;
        if relinks.is_empty() {
            return Ok(0);
        }
        for (left, from, to) in relinks {
            let site = RelinkSite::Sibling {
                tree: self.tree_id(),
                left: left.0,
            };
            let lsn = log.log(Body::Relink {
                from: from.0,
                to: to.0,
                site,
            })?;
            apply_relink(database.pool(), site, from, to, lsn)?;
        }
        let (kept_separators, kept_children, removed, freed) =
            without_doomed(separators, children, &doomed);
        self.install_interior(
            database,
            log,
            parent,
            level,
            &kept_separators,
            &kept_children,
        )?;
        for page in &freed {
            self.free_emptied_leaf(database, log, *page)?;
        }
        // The separators that bounded the unlinked leaves leave the tree, and the pages they were
        // stored on go with them, as the merge frees the one it takes out.
        for (out_of_line, bytes) in removed {
            if out_of_line {
                crate::paged::free_extent(database, log, ExtentRef::decode(&bytes)?)?;
            }
        }
        let mut stats = self.stats.get();
        stats.merges = stats.merges.saturating_add(freed.len() as u64);
        self.stats.set(stats);
        if self.path_holds(database, path)? {
            match kept_separators.is_empty() {
                true => self.dissolve_one_child_parents(database, log, path)?,
                false => self.dissolve_stranded_neighbour(database, log, path)?,
            }
        }
        Ok(freed.len())
    }

    /// Returns the relinks that take each run of doomed children out of the leaf chain, as
    /// (left neighbour, first leaf of the run, leaf after the run), and clears the runs whose
    /// links do not agree with the parent.
    ///
    /// @param database - the file and its pool
    /// @param children - the parent's children
    /// @param doomed - which of them to unlink, cleared for a run that cannot be
    fn relinks_for(
        &self,
        database: &Database,
        children: &[PageId],
        doomed: &mut [bool],
    ) -> DbResult<Vec<(PageId, PageId, PageId)>> {
        let mut relinks = Vec::new();
        let mut at = 1usize;
        while at < children.len() {
            if !doomed.get(at).copied().unwrap_or(false) {
                at = at.saturating_add(1);
                continue;
            }
            let start = at;
            while doomed.get(at).copied().unwrap_or(false) {
                at = at.saturating_add(1);
            }
            let mut chain = children.get(start.saturating_sub(1)..at).unwrap_or(&[]);
            let mut after = match chain.last() {
                Some(last) => self.right_of(database, *last)?,
                None => PageId(0),
            };
            // **A run that reaches the end of the tree keeps its last leaf, empty.** That leaf
            // is the tree's rightmost, which an append's hint remembers, so it is not freed; the
            // run's other leaves are, with the kept leaf as what follows them. Rejecting the
            // whole run freed nothing when a delete emptied a table from its second leaf on, and
            // `PRAGMA incremental_vacuum` then had no page to give back (`process_maintenance`).
            if after.0 == 0 && chain.len() > 2 {
                if let Some((last, rest)) = chain.split_last() {
                    if self.links_follow(database, chain)? {
                        if let Some(slot) = doomed.get_mut(at.saturating_sub(1)) {
                            *slot = false;
                        }
                        after = *last;
                        chain = rest;
                    }
                }
            }
            if after.0 != 0 && self.links_follow(database, chain)? {
                if let (Some(left), Some(first)) = (chain.first(), chain.get(1)) {
                    relinks.push((*left, *first, after));
                }
            } else {
                for slot in doomed.iter_mut().take(at).skip(start) {
                    *slot = false;
                }
            }
        }
        Ok(relinks)
    }

    /// Reports whether each leaf of a list names the next as its right sibling.
    ///
    /// @param database - the file and its pool
    /// @param chain - the leaves, left to right
    fn links_follow(&self, database: &Database, chain: &[PageId]) -> DbResult<bool> {
        for pair in chain.windows(2) {
            if let [left, right] = pair {
                if self.right_of(database, *left)? != *right {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Returns a leaf's right sibling.
    ///
    /// @param database - the file and its pool
    /// @param leaf - the leaf
    fn right_of(&self, database: &Database, leaf: PageId) -> DbResult<PageId> {
        let guard = database.pool().fetch(leaf)?;
        Ok(LeafRef::parse(&guard)?.right_sibling())
    }

    /// Reports whether a leaf holds no live row and names no out of line value.
    ///
    /// A leaf holding a run of a dead row's value is left to the merge, which frees the runs of
    /// both leaves it consumes; the unlink frees none.
    ///
    /// @param database - the file and its pool
    /// @param page - the leaf
    fn leaf_is_unlinkable(&self, database: &Database, page: PageId) -> DbResult<bool> {
        let guard = database.pool().fetch(page)?;
        let leaf = LeafRef::parse(&guard)?;
        Ok(!leaf.has_extents() && leaf.live_rows()? == 0)
    }

    /// Reports whether each interior page of a path is still the parent of the next, so the
    /// dissolves after an unlink may walk it.
    ///
    /// A deferred unlink acts on a path read when its leaf emptied, and a dissolve under another
    /// parent since then can have moved a page. A path that no longer holds skips the dissolve,
    /// which a later delete under that parent tries again.
    ///
    /// @param database - the file and its pool
    /// @param path - the interior pages, root first
    fn path_holds(&self, database: &Database, path: &[PageId]) -> DbResult<bool> {
        for pair in path.windows(2) {
            if let [upper, lower] = pair {
                let (_, children, _) = self.read_interior(database.pool(), *upper)?;
                if !children.contains(lower) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Gives an unlinked leaf back to the free map, leaving an empty leaf in the pool.
    ///
    /// **The page becomes the empty leaf a lookup can safely land in**, for the reason the merge
    /// installs one at the sibling it frees: a delete or an update holding the page as its nearby
    /// leaf would otherwise find the rows it held a moment ago in a page the free map owns.
    ///
    /// @param database - the file and its pool
    /// @param log - where the record goes
    /// @param page - the unlinked leaf
    fn free_emptied_leaf(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        page: PageId,
    ) -> DbResult<()> {
        let builder = LeafBuilder::new(
            self.page_size(),
            self.tree_id(),
            self.columns().to_vec(),
            self.key_columns(),
        )?;
        let mut emptied = builder.encode_empty()?;
        let lsn = log.log(Body::FreePage { page: page.0 })?;
        page::write_u64(&mut emptied, page::header::LSN, lsn)?;
        database.install(page, &emptied)?;
        database.release(page, 1)?;
        self.last_leaf.set(None);
        self.forget_failed_merge(page);
        self.note_leaves(-1);
        Ok(())
    }
}

/// What is left of an interior page once its doomed children are taken out.
///
/// The kept separators, the kept children, the separators that were taken out, and the children
/// that were taken out.
type Remaining = (Vec<StoredSlot>, Vec<PageId>, Vec<StoredSlot>, Vec<PageId>);

/// Takes the doomed children out of an interior page's lists, each with the separator on its left.
///
/// A child at position `n` lies between separators `n - 1` and `n`, so taking it out with
/// separator `n - 1` leaves its left neighbour bounded by separator `n`, which was its own upper
/// bound. Child 0 is never doomed, so every doomed child has a separator on its left.
///
/// @param separators - the parent's separators
/// @param children - the parent's children
/// @param doomed - which children go
fn without_doomed(
    separators: Vec<StoredSlot>,
    children: Vec<PageId>,
    doomed: &[bool],
) -> Remaining {
    let mut kept_separators = Vec::with_capacity(separators.len());
    let mut removed = Vec::new();
    for (at, separator) in separators.into_iter().enumerate() {
        match doomed.get(at.saturating_add(1)).copied().unwrap_or(false) {
            true => removed.push(separator),
            false => kept_separators.push(separator),
        }
    }
    let mut kept_children = Vec::with_capacity(children.len());
    let mut freed = Vec::new();
    for (at, child) in children.into_iter().enumerate() {
        match doomed.get(at).copied().unwrap_or(false) {
            true => freed.push(child),
            false => kept_children.push(child),
        }
    }
    (kept_separators, kept_children, removed, freed)
}

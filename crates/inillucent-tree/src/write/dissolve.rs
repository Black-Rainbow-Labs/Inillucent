//! Taking an interior page that is left with one child out of the tree.
//!
//! A table deleted from the front empties its leftmost leaves, and the leaf
//! merge folds each emptied leaf into its right sibling under the same parent.
//! Under every parent the last leaf has no sibling there to fold into, so the
//! parent ends with one child and no separator. At the default page an interior
//! page routes to hundreds of leaves and this costs nothing that shows, but at
//! 4,096 byte pages a table trimmed and refilled kept an interior page and a
//! leaf for every parent each round emptied, and the file grew by about six
//! pages a round.
//!
//! Such a parent is dissolved: its one child is handed to the parent's
//! neighbour after it under the grandparent (or the neighbour before it, when
//! the one after has no room), the grandparent drops the separator and the
//! pointer that named the parent, and the parent is freed. The child keeps its
//! page number, so no leaf moves, no sibling link changes and no leaf hint is
//! made wrong. The root is never dissolved, so a tree's height only grows,
//! which is what every cached height in this process and in another one
//! already assumes.
//!
//! Invariant: **every row reachable before a dissolve is reachable after it
//! through the same leaf, and every child stays strictly between the separators
//! around it.** The separator pulled down out of the grandparent is exactly the
//! bound the child had there, which is what `PagedTree::check_subtree` asserts.
//!
//! The two rewritten interior pages are logged whole as `WritePage` records and
//! the freed page as a `FreePage` record, which the replay already applies by
//! the page LSN rule, so the log format does not change.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::extent::ExtentRef;
use inillucent_pool::interior::InteriorBuilder;
use inillucent_pool::page;
use inillucent_pool::{Database, PageId, Swip};
use inillucent_wal::record::Body;

use super::{slots_of, TreeLog};
use crate::paged::PagedTree;
use crate::split_log::StoredSlot;

/// What one dissolve did.
enum Dissolved {
    /// Nothing: the page has more than one child, or neither neighbour has room.
    No,
    /// The page was freed and its child handed over.
    Yes,
    /// The page was freed and its child, a leaf, became the first child of the page
    /// after it, whose next child begins at this separator.
    LeafFirst(PageId, StoredSlot),
}

/// Where the one child goes and what the neighbour and the grandparent become.
struct Handover {
    /// The neighbour that takes the child.
    neighbour: PageId,
    /// The neighbour's separators after it takes the child.
    separators: Vec<StoredSlot>,
    /// The neighbour's children after it takes the child.
    children: Vec<PageId>,
    /// Which of the grandparent's separators is pulled down into the neighbour.
    pulled: usize,
}

impl PagedTree {
    /// Dissolves the lowest page of `path` while it is below the root and has one child,
    /// then the page above it, and so on up to the root.
    ///
    /// Stops at the root, at the first page that keeps two children or more, and at a
    /// page whose neighbours have no room for its child, which stays as it is until a
    /// later delete under it tries again.
    ///
    /// @param database - the file and its pool
    /// @param log - where the records go
    /// @param path - the interior pages from the root down to the page to try, root first
    pub(super) fn dissolve_one_child_parents(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        path: &[PageId],
    ) -> DbResult<()> {
        let mut path = path;
        let mut handed = None;
        while path.len() >= 2 {
            match self.dissolve_into_neighbour(database, log, path)? {
                Dissolved::No => break,
                Dissolved::Yes => {}
                Dissolved::LeafFirst(leaf, bound) => handed = handed.or(Some((leaf, bound))),
            }
            path = path.get(..path.len().saturating_sub(1)).unwrap_or(&[]);
        }
        // **The leaf handed over is usually empty, and nothing would merge it.** A trim
        // empties a parent's last leaf and then deletes on in the next parent, so no
        // delete lands in the handed leaf again, and it stayed in the tree empty: one
        // leaf for every parent a trim dissolved. Its right sibling is now under the
        // same parent, so the leaf merge runs on it here, through a path found again
        // from the root because the dissolves above may have changed the old one. Its
        // sibling holds keys from the separator up, which is the key the descent takes.
        let Some((leaf, bound)) = handed else {
            return Ok(());
        };
        let bound = match bound {
            (false, bytes) => bytes,
            (true, slot) => crate::paged::read_extent(database.pool(), ExtentRef::decode(&slot)?)?,
        };
        let (_, fresh) = self.leaf_for(database.pool(), &bound)?;
        self.merge_if_small(database, log, leaf, &fresh)
    }

    /// After a leaf merge made room in the last page of `path`, dissolves the page before
    /// it under the same grandparent when that page is down to one child.
    ///
    /// **A table trimmed from the front leaves such a page behind it.** The trim empties
    /// the page's last leaf while the page after it is full, so the dissolve waits, and
    /// no later delete lands under the emptied page to try it again. The first merge in
    /// the page after it is what makes the room, so that is when it is tried.
    ///
    /// @param database - the file and its pool
    /// @param log - where the records go
    /// @param path - the interior pages from the root down to the page that just lost a
    ///   child, root first
    pub(super) fn dissolve_stranded_neighbour(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        path: &[PageId],
    ) -> DbResult<()> {
        let (Some(&parent), Some(&grand)) = (path.last(), path.get(path.len().wrapping_sub(2)))
        else {
            return Ok(());
        };
        let (_, grand_children, _) = self.read_interior(database.pool(), grand)?;
        let before = grand_children
            .iter()
            .position(|held| *held == parent)
            .and_then(|at| at.checked_sub(1))
            .and_then(|at| grand_children.get(at).copied());
        let Some(before) = before else {
            return Ok(());
        };
        let mut stranded = path
            .get(..path.len().saturating_sub(1))
            .unwrap_or(&[])
            .to_vec();
        stranded.push(before);
        self.dissolve_one_child_parents(database, log, &stranded)
    }

    /// Hands the one child of the last page of `path` to a neighbour under the page above
    /// it, and frees the page.
    ///
    /// @param database - the file and its pool
    /// @param log - where the records go
    /// @param path - the interior pages from the root down to the page to dissolve; at
    ///   least two long, so the page has a parent
    /// @returns what was done, which is nothing when the page has more than one child or
    ///   when neither neighbour has room for its child
    fn dissolve_into_neighbour(
        &mut self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        path: &[PageId],
    ) -> DbResult<Dissolved> {
        let (Some(&parent), Some(&grand)) = (path.last(), path.get(path.len().wrapping_sub(2)))
        else {
            return Ok(Dissolved::No);
        };
        let (own_separators, own_children, level) = self.read_interior(database.pool(), parent)?;
        let (Some(&child), true) = (own_children.first(), own_separators.is_empty()) else {
            return Ok(Dissolved::No);
        };
        let (mut grand_separators, mut grand_children, grand_level) =
            self.read_interior(database.pool(), grand)?;
        let Some(position) = grand_children.iter().position(|held| *held == parent) else {
            return Err(corrupt(
                "an interior page is not among its parent's children",
            ));
        };
        let Some(handover) = self.choose_handover(
            database,
            &grand_separators,
            &grand_children,
            position,
            child,
            level,
        )?
        else {
            return Ok(Dissolved::No);
        };
        grand_separators.remove(handover.pulled);
        grand_children.remove(position);
        self.install_interior(
            database,
            log,
            handover.neighbour,
            level,
            &handover.separators,
            &handover.children,
        )?;
        self.install_interior(
            database,
            log,
            grand,
            grand_level,
            &grand_separators,
            &grand_children,
        )?;
        // Freed last, as the leaf merge frees its sibling after the installs, so a
        // failure between the steps leaks a page rather than freeing one still pointed
        // at. Its bytes stay: until the free map hands it out again it is an interior
        // page whose one child is `child`, so a stale path through it still arrives.
        log.log(Body::FreePage { page: parent.0 })?;
        database.release(parent, 1)?;
        let mut stats = self.stats.get();
        stats.parents_dissolved = stats.parents_dissolved.saturating_add(1);
        self.stats.set(stats);
        let first = handover.pulled == position && level == 1;
        match (first, handover.separators.first()) {
            (true, Some(bound)) => Ok(Dissolved::LeafFirst(child, bound.clone())),
            _ => Ok(Dissolved::Yes),
        }
    }

    /// Picks the neighbour that takes the child: the one after the parent when the
    /// separators fit it, otherwise the one before.
    ///
    /// @param database - the file and its pool
    /// @param separators - the grandparent's separators
    /// @param children - the grandparent's children
    /// @param position - the parent's place among them
    /// @param child - the parent's one child
    /// @param level - the parent's level, which the neighbour must share
    /// @returns the handover, or `None` when neither neighbour has room or there is none
    fn choose_handover(
        &self,
        database: &Database,
        separators: &[StoredSlot],
        children: &[PageId],
        position: usize,
        child: PageId,
        level: u16,
    ) -> DbResult<Option<Handover>> {
        let builder = InteriorBuilder::new(self.page_size(), self.tree_id(), level)?;
        let after = children.get(position.saturating_add(1)).copied();
        if let (Some(neighbour), Some(pulled)) = (after, separators.get(position)) {
            let (mut held, mut kids) = self.neighbour_at(database, neighbour, level)?;
            held.insert(0, pulled.clone());
            kids.insert(0, child);
            if fits(&builder, &held) {
                return Ok(Some(Handover {
                    neighbour,
                    separators: held,
                    children: kids,
                    pulled: position,
                }));
            }
        }
        let before = position.checked_sub(1);
        let neighbour = before.and_then(|at| children.get(at).copied());
        let pulled = before.and_then(|at| separators.get(at));
        if let (Some(neighbour), Some(pulled), Some(at)) = (neighbour, pulled, before) {
            let (mut held, mut kids) = self.neighbour_at(database, neighbour, level)?;
            held.push(pulled.clone());
            kids.push(child);
            if fits(&builder, &held) {
                return Ok(Some(Handover {
                    neighbour,
                    separators: held,
                    children: kids,
                    pulled: at,
                }));
            }
        }
        Ok(None)
    }

    /// Reads a neighbour's separators and children, refusing one at another level.
    ///
    /// @param database - the file and its pool
    /// @param neighbour - the page beside the parent under the grandparent
    /// @param level - the level the parent is at
    fn neighbour_at(
        &self,
        database: &Database,
        neighbour: PageId,
        level: u16,
    ) -> DbResult<(Vec<StoredSlot>, Vec<PageId>)> {
        let (separators, children, found) = self.read_interior(database.pool(), neighbour)?;
        if found != level {
            return Err(corrupt(format!(
                "interior page {} is at level {found} beside a page at level {level}",
                neighbour.0
            )));
        }
        Ok((separators, children))
    }

    /// Builds an interior page, logs its image whole and installs it.
    ///
    /// @param database - the file and its pool
    /// @param log - where the record goes
    /// @param at - the page to write
    /// @param level - its level
    /// @param separators - its separators
    /// @param children - its children, one more than the separators
    fn install_interior(
        &self,
        database: &mut Database,
        log: &mut dyn TreeLog,
        at: PageId,
        level: u16,
        separators: &[StoredSlot],
        children: &[PageId],
    ) -> DbResult<()> {
        let builder = InteriorBuilder::new(self.page_size(), self.tree_id(), level)?;
        let swips: Vec<Swip> = children
            .iter()
            .map(|held| Swip::unswizzled(*held))
            .collect();
        // An out of line separator moves between pages as its reference, so a
        // dissolve neither writes nor frees a separator extent.
        let mut image = builder.build_slots(&slots_of(separators), &swips)?;
        let lsn = log.log(Body::WritePage {
            page: at.0,
            image: &image,
        })?;
        page::write_u64(&mut image, page::header::LSN, lsn)?;
        database.install(at, &image)?;
        Ok(())
    }
}

/// Whether these separators fit one interior page.
///
/// @param builder - the builder for the page's size and level
/// @param separators - the separators the page would hold
fn fits(builder: &InteriorBuilder, separators: &[StoredSlot]) -> bool {
    builder.fits_slots(&slots_of(separators))
}

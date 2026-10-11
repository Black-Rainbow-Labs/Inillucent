//! Moving a page from one page number to another.
//!
//! Invariant: **a move is complete or it has not started.** Everything that can
//! make a page unmovable is found by reading, before the first record is written,
//! and a page that cannot move is reported as pinned with the reason. Once the
//! first record is written the move runs to its end: the new copy is logged, every
//! field that named the old page is rewritten under a relink record, and the old
//! page is freed.
//!
//! The page keeps its contents. Only the numbers that name it change: the parent's
//! child swip, the left sibling's right field, the previous and next fields of a
//! chain, the references in the leaf that owns a value, or the free map chain.
//! `PRAGMA auto_vacuum` moves pages this way to empty the end of the file.

mod apply;
mod find;
mod leafrefs;
mod plan;

pub use apply::apply_relink;
pub use leafrefs::patch_extent_refs;

use inillucent_base::DbResult;
use inillucent_pool::page;
use inillucent_pool::{Database, PageId};
use inillucent_wal::record::{Body, RelinkSite};

use crate::paged::PagedTree;
use crate::write::TreeLog;

/// Finds the tree handle a page's tree identifier names.
pub trait TreeLookup {
    /// Returns the tree with this identifier, or `None` when the file has none.
    ///
    /// @param tree_id - the identifier a page carries in its header
    fn tree(&self, tree_id: u64) -> Option<&PagedTree>;
}

/// Why a page cannot be moved.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Pinned {
    /// The page is the root of a tree. A root is named by a catalog row, which a
    /// move does not rewrite, so roots are kept at the start of the file.
    Root,
    /// No tree of this file owns the page, or it is allocated and holds nothing.
    Unowned,
    /// No interior page names the page.
    NoParent,
    /// The leaf before the page does not name it as its right sibling.
    SiblingMismatch,
    /// The page is an extent page written before extent pages recorded their owner.
    OlderLayout,
    /// The page was written with no owner key, because the key was too long or
    /// the value is a separator, so the reference to it cannot be found by a key.
    NoOwnerKey,
    /// The owner key leads to no leaf that names the page.
    NoReferrer,
}

impl Pinned {
    /// Returns the reason in words, for a report of why the file stopped shrinking.
    pub fn reason(self) -> &'static str {
        match self {
            Pinned::Root => "the page is the root of a tree",
            Pinned::Unowned => "no tree owns the page",
            Pinned::NoParent => "no interior page names the page",
            Pinned::SiblingMismatch => "the leaf before it does not name it as its sibling",
            Pinned::OlderLayout => "it is an extent page in the older layout",
            Pinned::NoOwnerKey => "it is an extent page written with no owner key",
            Pinned::NoReferrer => "no leaf holds a reference to it",
        }
    }
}

/// What a request to move a page came to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Relocated {
    /// The page now lives at the new number and the old number is free.
    Moved,
    /// Nothing changed, because the page cannot be moved.
    Pinned(Pinned),
}

/// Moves a page to a free page number, logging every step.
///
/// The records are, in order: the allocation of the new number, the whole page at
/// the new number, one relink for each field that named the old number, and the
/// release of the old number. The caller appends the commit.
///
/// @param database - the file
/// @param log - where the records go
/// @param trees - finds the tree a page belongs to
/// @param from - the page to move
/// @param to - a free page number, below `from`
pub fn relocate_page(
    database: &mut Database,
    log: &mut dyn TreeLog,
    trees: &dyn TreeLookup,
    from: PageId,
    to: PageId,
) -> DbResult<Relocated> {
    if database.is_map_page(from) {
        move_map_page(database, log, from, to)?;
        return Ok(Relocated::Moved);
    }
    // A page that cannot be read as a page of any kind is allocated and holds
    // nothing a tree owns: one a write let go of without saying so, or one taken
    // and never written. It stays where it is, which is the cost of the leak.
    let Ok(image) = database.pool().copy_for_move(from) else {
        return Ok(Relocated::Pinned(Pinned::Unowned));
    };
    if page::kind_of(&image).is_err() {
        return Ok(Relocated::Pinned(Pinned::Unowned));
    }
    let tree_id = page::tree_of(&image)?;
    let sites = match plan::plan_move(database.pool(), trees, from, &image)? {
        Ok(sites) => sites,
        Err(why) => return Ok(Relocated::Pinned(why)),
    };
    copy_page(database, log, from, to, image)?;
    for site in &sites {
        relink(database, log, *site, from, to)?;
    }
    log.log(Body::FreePage { page: from.0 })?;
    database.release(from, 1)?;
    if database.shared_extent() == Some(from) {
        database.set_shared_extent(Some(to));
    }
    if let Some(tree) = trees.tree(tree_id) {
        tree.forget_page_numbers();
    }
    Ok(Relocated::Moved)
}

/// Logs a page at its new number, installs it, and drops the old frame.
///
/// @param database - the file
/// @param log - where the records go
/// @param from - the old number
/// @param to - the new number
/// @param image - the page's bytes
fn copy_page(
    database: &mut Database,
    log: &mut dyn TreeLog,
    from: PageId,
    to: PageId,
    mut image: Vec<u8>,
) -> DbResult<()> {
    database.claim(to)?;
    log.log(Body::AllocPage { page: to.0 })?;
    let lsn = log.log(Body::WritePage {
        page: to.0,
        image: &image,
    })?;
    page::set_lsn(&mut image, lsn)?;
    database.install(to, &image)?;
    // The old frame goes before the parent is rewritten, so the parent's swizzled
    // swip is put back to a page number first and nothing names the frame after.
    database.pool().forget_page(from)?;
    Ok(())
}

/// Logs one relink and applies it.
///
/// @param database - the file
/// @param log - where the record goes
/// @param site - which field named the old number
/// @param from - the old number
/// @param to - the new number
fn relink(
    database: &mut Database,
    log: &mut dyn TreeLog,
    site: RelinkSite,
    from: PageId,
    to: PageId,
) -> DbResult<()> {
    let lsn = log.log(Body::Relink {
        from: from.0,
        to: to.0,
        site,
    })?;
    if !apply_relink(database.pool(), site, from, to, lsn)? {
        return Err(inillucent_base::error::corrupt(format!(
            "moving page {} to page {} found nothing to rewrite at {site:?}",
            from.0, to.0
        )));
    }
    Ok(())
}

/// Moves a free map page.
///
/// The map keeps its pages in memory and writes them at a checkpoint, so the move
/// changes the chain there and logs the new image and the one field that named the
/// page: the previous map page's right field, or the meta record's head.
///
/// @param database - the file
/// @param log - where the records go
/// @param from - the map page's number
/// @param to - the free page it moves to
fn move_map_page(
    database: &mut Database,
    log: &mut dyn TreeLog,
    from: PageId,
    to: PageId,
) -> DbResult<()> {
    let moved = database.move_map_page(from, to)?;
    let mut image = moved.image;
    log.log(Body::AllocPage { page: to.0 })?;
    let lsn = log.log(Body::WritePage {
        page: to.0,
        image: &image,
    })?;
    page::set_lsn(&mut image, lsn)?;
    database.install(to, &image)?;
    let site = match moved.previous {
        Some(previous) => RelinkSite::MapChain {
            previous: previous.0,
        },
        None => RelinkSite::MapHead,
    };
    log.log(Body::Relink {
        from: from.0,
        to: to.0,
        site,
    })?;
    log.log(Body::FreePage { page: from.0 })?;
    Ok(())
}

impl PagedTree {
    /// Forgets everything this handle remembers about which page holds what.
    ///
    /// A page that moved leaves the leaf hints, the last leaf, the largest key
    /// hint, the first leaf and the merge back off table naming a number that is
    /// now free or holds another page.
    pub fn forget_page_numbers(&self) {
        self.shape.set(None);
        self.leaf_hints.borrow_mut().clear();
        self.last_leaf.set(None);
        self.largest_hint.set(None);
        self.merge_backoff.borrow_mut().clear();
    }
}

/// Moves the page a tree's root is on to a free page number, and points the handle
/// at the new number.
///
/// **The caller rewrites whatever names the root**, because a root is named by a
/// catalog row and not by a page: either a row not written yet, for a tree just
/// built, or a row the caller rewrites under the same transaction. The old number
/// is freed.
///
/// @param database - the file
/// @param log - where the records go
/// @param tree - the tree whose root moves, updated to name the new page
/// @param to - a free page number
pub fn relocate_root(
    database: &mut Database,
    log: &mut dyn TreeLog,
    tree: &mut PagedTree,
    to: PageId,
) -> DbResult<()> {
    let from = tree.root();
    let image = database.pool().copy_for_move(from)?;
    copy_page(database, log, from, to, image)?;
    log.log(Body::FreePage { page: from.0 })?;
    database.release(from, 1)?;
    tree.root = to;
    tree.forget_page_numbers();
    Ok(())
}

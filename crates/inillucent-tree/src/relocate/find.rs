//! Finding the page that names a page, by walking the owning tree.
//!
//! Invariant: **nothing here changes a page.** A search answers where the
//! referrer is, or that there is none, and the move that follows is the only
//! thing that writes.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::interior::InteriorRef;
use inillucent_pool::page::{self, PageKind};
use inillucent_pool::{PageId, Pool};

use super::leafrefs::{names_page, names_slot};
use crate::paged::PagedTree;

/// The interior pages from a tree's root down to the parent of a page, each with
/// the index of the child taken. The last entry is the parent itself.
pub(super) type Path = Vec<(PageId, usize)>;

/// How many levels a tree may have before a walk gives up.
const MOST_LEVELS: usize = 64;

/// Returns the interior page that names `target`, and the path to it.
///
/// Walks interior pages only, the rightmost child first, because the pages the
/// step moves are the most recently written ones. `level` is the level of
/// `target`, 0 for a leaf, so only pages one level above it are searched.
///
/// @param pool - the buffer pool the file is open through
/// @param root - the tree's root page
/// @param target - the page whose parent is wanted
/// @param level - the level of `target`
pub(super) fn find_parent(
    pool: &Pool,
    root: PageId,
    target: PageId,
    level: u16,
) -> DbResult<Option<Path>> {
    let mut path = Vec::new();
    let found = search(pool, root, target, level, &mut path)?;
    Ok(found.then_some(path))
}

/// The recursive half of [`find_parent`].
///
/// @param pool - the buffer pool
/// @param page - the interior page being searched
/// @param target - the page whose parent is wanted
/// @param level - the level of `target`
/// @param path - the path to `page`, extended as the search goes down
fn search(
    pool: &Pool,
    page: PageId,
    target: PageId,
    level: u16,
    path: &mut Path,
) -> DbResult<bool> {
    if path.len() > MOST_LEVELS {
        return Err(corrupt("a tree is deeper than 64 levels"));
    }
    let (here, children) = {
        let guard = pool.fetch(page)?;
        if page::kind_of(&guard)? != PageKind::Interior {
            return Ok(false);
        }
        let interior = InteriorRef::parse(&guard)?;
        let mut children = Vec::with_capacity(interior.children());
        for child in 0..interior.children() {
            children.push(pool.page_of_swip(interior.swip(child)?)?);
        }
        (interior.level(), children)
    };
    if here <= level {
        return Ok(false);
    }
    if here == level.saturating_add(1) {
        return Ok(match children.iter().position(|child| *child == target) {
            Some(index) => {
                path.push((page, index));
                true
            }
            None => false,
        });
    }
    for (index, child) in children.iter().enumerate().rev() {
        path.push((page, index));
        if search(pool, *child, target, level, path)? {
            return Ok(true);
        }
        path.pop();
    }
    Ok(false)
}

/// Returns the leaf before the leaf a path ends at, or `None` for the first leaf.
///
/// The previous child of the deepest ancestor where the path is not on the first
/// child, then down its rightmost children to a leaf.
///
/// @param pool - the buffer pool
/// @param path - the path to the leaf's parent, from [`find_parent`]
pub(super) fn left_neighbour(pool: &Pool, path: &[(PageId, usize)]) -> DbResult<Option<PageId>> {
    for (page, index) in path.iter().rev() {
        let Some(before) = index.checked_sub(1) else {
            continue;
        };
        let mut current = child_of(pool, *page, before)?;
        for _ in 0..MOST_LEVELS {
            let rightmost = {
                let guard = pool.fetch(current)?;
                if page::kind_of(&guard)? == PageKind::Leaf {
                    return Ok(Some(current));
                }
                InteriorRef::parse(&guard)?.count()
            };
            current = child_of(pool, current, rightmost)?;
        }
        return Err(corrupt("a tree is deeper than 64 levels"));
    }
    Ok(None)
}

/// Returns the page one child of an interior page names.
///
/// @param pool - the buffer pool
/// @param page - the interior page
/// @param child - which child
fn child_of(pool: &Pool, page: PageId, child: usize) -> DbResult<PageId> {
    let guard = pool.fetch(page)?;
    let swip = InteriorRef::parse(&guard)?.swip(child)?;
    pool.page_of_swip(swip)
}

/// What a leaf must hold to be the referrer of a page.
#[derive(Clone, Copy)]
pub(super) enum Wanted {
    /// Any reference whose first page is this one.
    Page(PageId),
    /// A reference to this slot of this shared page.
    Slot(PageId, u16),
}

/// Reports whether a leaf holds what is wanted.
///
/// @param pool - the buffer pool
/// @param leaf - the leaf
/// @param wanted - the reference to look for
fn holds(pool: &Pool, leaf: PageId, wanted: Wanted) -> DbResult<bool> {
    let guard = pool.fetch(leaf)?;
    if page::kind_of(&guard)? != PageKind::Leaf {
        return Ok(false);
    }
    match wanted {
        Wanted::Page(first) => names_page(&guard, first),
        Wanted::Slot(first, slot) => names_slot(&guard, first, slot),
    }
}

/// Returns the leaves of a tree that hold what is wanted.
///
/// The leaf a descent by the owner key reaches is tried first, and is the whole
/// answer for a value. When it does not hold the reference, because the key
/// recorded is not the key the row has now, every leaf of the tree is searched.
///
/// @param pool - the buffer pool
/// @param tree - the tree that owns the value
/// @param key - the owner key recorded on the page
/// @param wanted - the reference to look for
pub(super) fn leaves_holding(
    pool: &Pool,
    tree: &PagedTree,
    key: &[u8],
    wanted: Wanted,
) -> DbResult<Vec<PageId>> {
    let leaf = tree.descend(pool, key)?.leaf;
    if holds(pool, leaf, wanted)? {
        return Ok(vec![leaf]);
    }
    let mut found = Vec::new();
    let mut current = tree.first_leaf(pool)?;
    let mut visited = 0u64;
    while !current.is_none() {
        if holds(pool, current, wanted)? {
            found.push(current);
        }
        current = {
            let guard = pool.fetch(current)?;
            page::right_of(&guard)?
        };
        visited = visited.saturating_add(1);
        if visited > pool.page_count() {
            return Err(corrupt("a leaf chain loops"));
        }
    }
    Ok(found)
}

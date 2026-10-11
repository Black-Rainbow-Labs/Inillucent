//! Working out, before anything changes, which fields name a page that will move.
//!
//! Invariant: **a plan is made from reading alone, and a page that cannot be
//! planned is reported as pinned with the reason.** The step that lowers the end
//! of the file stops at a pinned page and the file stays as long as that page
//! needs, which is the cost of a page it cannot safely move and never a failure.

use inillucent_base::DbResult;
use inillucent_pool::extent::{owned, shared};
use inillucent_pool::page::{self, PageKind};
use inillucent_pool::{PageId, Pool};
use inillucent_wal::record::RelinkSite;

use super::find::{find_parent, leaves_holding, left_neighbour, Wanted};
use super::{Pinned, TreeLookup};

/// What planning a page found: the fields to rewrite, or why it stays.
pub(super) type Planned = Result<Vec<RelinkSite>, Pinned>;

/// Plans the move of the page whose bytes are `image`.
///
/// @param pool - the buffer pool the file is open through
/// @param trees - finds a tree by the identifier a page carries
/// @param from - the page that would move
/// @param image - the page's bytes
pub(super) fn plan_move(
    pool: &Pool,
    trees: &dyn TreeLookup,
    from: PageId,
    image: &[u8],
) -> DbResult<Planned> {
    let tree_id = page::tree_of(image)?;
    match page::kind_of(image)? {
        PageKind::Leaf | PageKind::Interior => plan_tree_page(pool, trees, from, image, tree_id),
        PageKind::BlobExtent => plan_extent_page(pool, trees, from, image, tree_id),
        PageKind::BlobShared => plan_shared_page(pool, trees, from, image),
        PageKind::FreeMap | PageKind::Unused => Ok(Err(Pinned::Unowned)),
    }
}

/// Plans a leaf or an interior page: its parent's swip, and for a leaf the
/// right sibling field of the leaf before it.
///
/// @param pool - the buffer pool
/// @param trees - finds a tree by identifier
/// @param from - the page that would move
/// @param image - the page's bytes
/// @param tree_id - the tree the page declares
fn plan_tree_page(
    pool: &Pool,
    trees: &dyn TreeLookup,
    from: PageId,
    image: &[u8],
    tree_id: u64,
) -> DbResult<Planned> {
    let Some(tree) = trees.tree(tree_id) else {
        return Ok(Err(Pinned::Unowned));
    };
    if tree.root() == from {
        return Ok(Err(Pinned::Root));
    }
    let level = page::level_of(image)?;
    let Some(path) = find_parent(pool, tree.root(), from, level)? else {
        return Ok(Err(Pinned::NoParent));
    };
    let Some((parent, _)) = path.last().copied() else {
        return Ok(Err(Pinned::NoParent));
    };
    let mut sites = vec![RelinkSite::Child {
        tree: tree_id,
        parent: parent.0,
    }];
    if page::kind_of(image)? == PageKind::Leaf {
        if let Some(left) = left_neighbour(pool, &path)? {
            let held = pool.fetch(left)?;
            if page::right_of(&held)? != from {
                return Ok(Err(Pinned::SiblingMismatch));
            }
            sites.push(RelinkSite::Sibling {
                tree: tree_id,
                left: left.0,
            });
        }
    }
    Ok(Ok(sites))
}

/// Plans a page of an extent chain.
///
/// The first page is named by a leaf row, found by the owner key it carries. Any
/// other page is named by the `next` field of the page before it. Either way the
/// page after it, if there is one, names it in its `prev` field.
///
/// @param pool - the buffer pool
/// @param trees - finds a tree by identifier
/// @param from - the page that would move
/// @param image - the page's bytes
/// @param tree_id - the tree the page declares
fn plan_extent_page(
    pool: &Pool,
    trees: &dyn TreeLookup,
    from: PageId,
    image: &[u8],
    tree_id: u64,
) -> DbResult<Planned> {
    if !owned::is_owned(image) {
        return Ok(Err(Pinned::OlderLayout));
    }
    let links = owned::links(image)?;
    let mut sites = Vec::new();
    if links.head {
        let key = owned::owner_key(image)?;
        let (Some(tree), false) = (trees.tree(tree_id), key.is_empty()) else {
            return Ok(Err(Pinned::NoOwnerKey));
        };
        let leaves = leaves_holding(pool, tree, key, Wanted::Page(from))?;
        if leaves.is_empty() {
            return Ok(Err(Pinned::NoReferrer));
        }
        sites.extend(leaves.into_iter().map(|leaf| RelinkSite::Extent {
            tree: tree_id,
            leaf: leaf.0,
        }));
    } else {
        sites.push(RelinkSite::ChainNext { page: links.prev.0 });
    }
    if !links.next.is_none() {
        sites.push(RelinkSite::ChainPrev { page: links.next.0 });
    }
    Ok(Ok(sites))
}

/// Plans a shared page: one extent site for each leaf that holds a reference to a
/// slot of it.
///
/// @param pool - the buffer pool
/// @param trees - finds a tree by identifier
/// @param from - the page that would move
/// @param image - the page's bytes
fn plan_shared_page(
    pool: &Pool,
    trees: &dyn TreeLookup,
    from: PageId,
    image: &[u8],
) -> DbResult<Planned> {
    if !shared::is_owned(image) {
        return Ok(Err(Pinned::OlderLayout));
    }
    let mut sites: Vec<RelinkSite> = Vec::new();
    for slot in shared::live_slots(image)? {
        let Some(owner) = shared::owner_of(image, slot)? else {
            return Ok(Err(Pinned::OlderLayout));
        };
        let (Some(tree), false) = (trees.tree(owner.tree), owner.key.is_empty()) else {
            return Ok(Err(Pinned::NoOwnerKey));
        };
        let leaves = leaves_holding(pool, tree, &owner.key, Wanted::Slot(from, slot))?;
        if leaves.is_empty() {
            return Ok(Err(Pinned::NoReferrer));
        }
        for leaf in leaves {
            let site = RelinkSite::Extent {
                tree: owner.tree,
                leaf: leaf.0,
            };
            if !sites.contains(&site) {
                sites.push(site);
            }
        }
    }
    Ok(Ok(sites))
}

//! Applying one relink: rewriting the one field that named a page that moved.
//!
//! Invariant: **a relink writes nothing when the field no longer names the page
//! that moved.** The step that moves pages applies the same relinks a replay
//! does, from the same function, so a replay of a relink that a checkpoint already
//! made is a no-op, and a page is stamped with the record's LSN only when it
//! changed.

use inillucent_base::DbResult;
use inillucent_pool::extent::owned;
use inillucent_pool::interior::{swip_offsets_of, write_swip};
use inillucent_pool::page::{self, PageKind};
use inillucent_pool::{PageId, Pool, Swip};
use inillucent_wal::record::RelinkSite;

use super::leafrefs::patch_extent_refs;

/// Applies a relink to the page it names and reports whether anything changed.
///
/// A site that is the meta record, [`RelinkSite::MapHead`], names no page and
/// answers `false`: the caller changes the meta record.
///
/// @param pool - the buffer pool the file is open through
/// @param site - which field named the page
/// @param from - the page number the field named
/// @param to - the page number it names now
/// @param lsn - the record's LSN, stamped on the page when it changes
pub fn apply_relink(
    pool: &Pool,
    site: RelinkSite,
    from: PageId,
    to: PageId,
    lsn: u64,
) -> DbResult<bool> {
    let target = match site {
        RelinkSite::Child { parent: page, .. }
        | RelinkSite::Sibling { left: page, .. }
        | RelinkSite::Extent { leaf: page, .. }
        | RelinkSite::ChainNext { page }
        | RelinkSite::ChainPrev { page }
        | RelinkSite::MapChain { previous: page } => PageId(page),
        RelinkSite::MapHead => return Ok(false),
    };
    pool.modify(target, |bytes| {
        let changed = rewrite(bytes, site, from, to)?;
        if changed {
            page::set_lsn(bytes, lsn)?;
        }
        Ok(changed)
    })
}

/// Rewrites the field a site names inside a page image.
///
/// @param bytes - the page
/// @param site - which field
/// @param from - the page number the field named
/// @param to - the page number it names now
fn rewrite(bytes: &mut [u8], site: RelinkSite, from: PageId, to: PageId) -> DbResult<bool> {
    match site {
        RelinkSite::Child { .. } => rewrite_swip(bytes, from, to),
        RelinkSite::Sibling { .. } | RelinkSite::MapChain { .. } => {
            if page::right_of(bytes)? != from {
                return Ok(false);
            }
            page::set_right(bytes, to)?;
            Ok(true)
        }
        RelinkSite::Extent { .. } => Ok(patch_extent_refs(bytes, from, to)? > 0),
        RelinkSite::ChainNext { .. } => rewrite_link(bytes, from, to, true),
        RelinkSite::ChainPrev { .. } => rewrite_link(bytes, from, to, false),
        RelinkSite::MapHead => Ok(false),
    }
}

/// Rewrites the child swip of an interior page that names `from`.
///
/// @param bytes - the interior page
/// @param from - the page number the swip named
/// @param to - the page number it names now
fn rewrite_swip(bytes: &mut [u8], from: PageId, to: PageId) -> DbResult<bool> {
    if page::kind_of(bytes)? != PageKind::Interior {
        return Ok(false);
    }
    for at in swip_offsets_of(bytes)? {
        let swip = Swip::from_raw(page::read_u64(bytes, at)?);
        if swip.page() == Some(from) {
            write_swip(bytes, at, Swip::unswizzled(to))?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Rewrites the next or the previous link of an owned extent page.
///
/// @param bytes - the extent page
/// @param from - the page number the link named
/// @param to - the page number it names now
/// @param next - true for the next link, false for the previous one
fn rewrite_link(bytes: &mut [u8], from: PageId, to: PageId, next: bool) -> DbResult<bool> {
    if page::kind_of(bytes)? != PageKind::BlobExtent || !owned::is_owned(bytes) {
        return Ok(false);
    }
    let held = owned::links(bytes)?;
    let named = if next { held.next } else { held.prev };
    if named != from {
        return Ok(false);
    }
    match next {
        true => owned::set_next(bytes, to)?,
        false => owned::set_prev(bytes, to)?,
    }
    Ok(true)
}

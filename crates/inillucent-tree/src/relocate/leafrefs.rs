//! Finding, and rewriting in place, the extent references a leaf holds.
//!
//! Invariant: **a reference is rewritten at the byte offset the leaf's own reader
//! reads it from, never found by matching bytes.** A value stored in a leaf can
//! hold any bytes, including sixteen that look like a reference, so the sites are
//! listed from the leaf's structure: the references of the sorted rows' columns
//! and of the delta rows, key columns included, tombstoned rows included.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::extent::{ExtentRef, EXTENT_REF_BYTES};
use inillucent_pool::PageId;

use crate::datum::{tag, Datum};
use crate::leaf::LeafRef;
use crate::types::ValueClass;

/// Returns where `part` begins inside `page`, when `part` is a slice of it.
///
/// @param page - the whole page
/// @param part - a slice taken from it
fn offset_in(page: &[u8], part: &[u8]) -> usize {
    (part.as_ptr() as usize).saturating_sub(page.as_ptr() as usize)
}

/// Lists every extent reference in a leaf with the byte offset it sits at.
///
/// A dead reference is listed too: it names no page, and the caller that rewrites
/// references by page number never matches it.
///
/// @param page - the leaf's bytes
pub(crate) fn reference_sites(page: &[u8]) -> DbResult<Vec<(usize, ExtentRef)>> {
    let leaf = LeafRef::parse(page)?;
    let mut sites = Vec::new();
    if !leaf.has_extents() {
        return Ok(sites);
    }
    for row in 0..leaf.row_count() {
        for column in 0..leaf.column_count() {
            let mini = leaf.column(column)?;
            if mini.class_at(row)? == ValueClass::Extent {
                sites.push((mini.extent_offset(row)?, mini.extent(row)?));
            }
        }
    }
    for index in 0..leaf.delta_count() {
        delta_sites(&leaf, page, index, &mut sites)?;
    }
    Ok(sites)
}

/// Adds the references of one delta row to a list of sites.
///
/// @param leaf - the leaf
/// @param page - the leaf's bytes
/// @param index - the delta row
/// @param sites - where the sites go
fn delta_sites(
    leaf: &LeafRef<'_>,
    page: &[u8],
    index: usize,
    sites: &mut Vec<(usize, ExtentRef)>,
) -> DbResult<()> {
    let row = leaf.delta_row(index)?;
    let mut cursor = 0usize;
    for _ in 0..leaf.column_count() {
        let rest = row.get(cursor..).unwrap_or(&[]);
        let kind = Datum::tag_of(rest)?;
        if kind == tag::EXTENT || kind == tag::KEY_EXTENT {
            let body = rest.get(1..).unwrap_or(&[]);
            sites.push((offset_in(page, body), ExtentRef::decode(body)?));
        }
        cursor = cursor.saturating_add(Datum::tagged_span(rest)?);
    }
    Ok(())
}

/// Reports whether a leaf holds a reference whose first page is `first`.
///
/// @param page - the leaf's bytes
/// @param first - the page number
pub(crate) fn names_page(page: &[u8], first: PageId) -> DbResult<bool> {
    Ok(reference_sites(page)?
        .iter()
        .any(|(_, reference)| reference.first == first))
}

/// Reports whether a leaf holds a reference to one slot of a shared page.
///
/// @param page - the leaf's bytes
/// @param first - the shared page
/// @param slot - the slot
pub(crate) fn names_slot(page: &[u8], first: PageId, slot: u16) -> DbResult<bool> {
    Ok(reference_sites(page)?
        .iter()
        .any(|(_, reference)| reference.first == first && reference.slot == Some(slot)))
}

/// Rewrites every reference in a leaf whose first page is `from` to name `to`.
///
/// The length, slot and class a reference carries are kept. Returns how many
/// references changed.
///
/// @param page - the leaf's bytes, changed in place
/// @param from - the page that moved
/// @param to - the page it moved to
pub fn patch_extent_refs(page: &mut [u8], from: PageId, to: PageId) -> DbResult<usize> {
    let sites: Vec<(usize, ExtentRef)> = reference_sites(page)?
        .into_iter()
        .filter(|(_, reference)| reference.first == from)
        .collect();
    for (at, reference) in &sites {
        let moved = ExtentRef {
            first: to,
            ..*reference
        };
        page.get_mut(*at..at.saturating_add(EXTENT_REF_BYTES))
            .ok_or_else(|| corrupt("an extent reference runs past the page"))?
            .copy_from_slice(&moved.encode());
    }
    Ok(sites.len())
}

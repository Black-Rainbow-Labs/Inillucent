//! The owned layout of a blob extent page: a page that says who refers to it.
//!
//! Invariant: an extent page in this layout carries `page::EXTENT_OWNED` in its
//! flag byte, a previous page link beside the next page link, and on the first
//! page of a value the key of the row that owns the value, so a page that moves
//! can be found from its referrer without scanning the owning tree.
//!
//! | offset | size | field |
//! |---|---|---|
//! | 32 | 8 | `next`: the next page of the chain, 0 at the last page |
//! | 40 | 8 | `prev`: the previous page of the chain, 0 at the first page |
//! | 48 | 4 | `payload_len`: payload bytes in this page |
//! | 52 | 2 | `key_len`: on the first page, the length of the owner key, else 0 |
//! | 54 | 2 | `flags`: bit 0 `HEAD`, set on the first page |
//! | 56 | `key_len` | on the first page, the owner key |
//! | 56 + `key_len` | | the payload |
//!
//! The owner tree is the `TREE` field of the common header. The owner key is the
//! row's key as the owning tree encodes it for a descent. **A key longer than a
//! quarter of the page is not written.** The page is then owned with a key length
//! of 0, and the step that moves pages treats it as pinned, because the first page
//! of a value has to leave room for payload. A separator stored out of line in an
//! interior page is written the same way, with no owner key, and its page is
//! pinned for the same reason: the interior page that holds the reference is not
//! found by a key.

use inillucent_base::error::{corrupt, misuse};
use inillucent_base::DbResult;

use crate::page::{self, PageKind, EXTENT_OWNED};
use crate::PageId;

/// Byte offsets inside an owned extent page.
pub mod at {
    /// The next page of the chain, 8 bytes.
    pub const NEXT: usize = 32;
    /// The previous page of the chain, 8 bytes.
    pub const PREV: usize = 40;
    /// How many payload bytes this page holds, 4 bytes.
    pub const PAYLOAD: usize = 48;
    /// The owner key length on the first page, 2 bytes.
    pub const KEY_LEN: usize = 52;
    /// The page's own flags, 2 bytes.
    pub const FLAGS: usize = 54;
    /// Where the owner key begins, and where the payload begins when there is no key.
    pub const BODY: usize = 56;
}

/// The bit of the page's own flags that marks the first page of a value.
pub const HEAD: u16 = 1;

/// What an owned page says about its place in a chain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Links {
    /// The next page of the chain, or [`PageId::NONE`].
    pub next: PageId,
    /// The previous page of the chain, or [`PageId::NONE`].
    pub prev: PageId,
    /// Whether this is the first page of its value.
    pub head: bool,
}

/// Returns the longest owner key a page size stores.
///
/// @param page_size - the database's page size in bytes
pub fn key_limit(page_size: usize) -> usize {
    page_size / 4
}

/// Returns how many payload bytes the first page of a value holds.
///
/// @param page_size - the database's page size in bytes
/// @param key_len - the length of the owner key written on it
pub fn head_capacity(page_size: usize, key_len: usize) -> usize {
    page_size.saturating_sub(at::BODY).saturating_sub(key_len)
}

/// Returns how many payload bytes every page after the first holds.
///
/// @param page_size - the database's page size in bytes
pub fn body_capacity(page_size: usize) -> usize {
    page_size.saturating_sub(at::BODY)
}

/// Returns the owner key length a writer stores for a key.
///
/// A key past [`key_limit`] is not stored, and the answer is zero.
///
/// @param page_size - the database's page size in bytes
/// @param key - the encoded owner key
pub fn stored_key_len(page_size: usize, key: &[u8]) -> usize {
    match key.len() <= key_limit(page_size) {
        true => key.len(),
        false => 0,
    }
}

/// Returns how many pages a value of this length needs in this layout.
///
/// @param length - the value's length in bytes
/// @param page_size - the database's page size in bytes
/// @param key_len - the length of the owner key written on the first page
pub fn pages_needed(length: u64, page_size: usize, key_len: usize) -> u64 {
    let head = head_capacity(page_size, key_len) as u64;
    let body = body_capacity(page_size).max(1) as u64;
    match length.saturating_sub(head) {
        0 => 1,
        rest => 1u64.saturating_add(rest.saturating_add(body.saturating_sub(1)) / body),
    }
}

/// Returns the most pages a chain for a value of this length can have.
///
/// A reader uses it as the bound that stops a chain that loops, so it assumes
/// the longest owner key and the smallest payload any page holds.
///
/// @param length - the value's length in bytes
/// @param page_size - the database's page size in bytes
pub fn chain_bound(length: u64, page_size: usize) -> u64 {
    let smallest = head_capacity(page_size, key_limit(page_size)).max(1) as u64;
    length
        .saturating_add(smallest.saturating_sub(1))
        .saturating_div(smallest)
        .saturating_add(1)
}

/// Writes a value into a run of owned page images.
///
/// The caller has already allocated the run. Each page names its successor and
/// its predecessor by the page numbers of a contiguous run, and the first page
/// carries the owner key when it is short enough to store.
///
/// @param value - the bytes to store
/// @param first - the first page of the run
/// @param page_size - the database's page size in bytes
/// @param tree - the tree the value belongs to
/// @param owner_key - the key of the row that owns the value, or empty for none
pub fn encode_run(
    value: &[u8],
    first: PageId,
    page_size: usize,
    tree: u64,
    owner_key: &[u8],
) -> DbResult<Vec<(PageId, Vec<u8>)>> {
    if first.is_none() {
        return Err(misuse("an extent cannot start at page zero"));
    }
    let key_len = stored_key_len(page_size, owner_key);
    if head_capacity(page_size, key_len) == 0 || body_capacity(page_size) == 0 {
        return Err(misuse("the page size leaves no room for an extent payload"));
    }
    let count = pages_needed(value.len() as u64, page_size, key_len);
    let mut images = Vec::new();
    let mut offset = 0usize;
    for index in 0..count {
        let head = index == 0;
        let key = if head {
            owner_key.get(..key_len).unwrap_or(&[])
        } else {
            &[]
        };
        let room = if head {
            head_capacity(page_size, key_len)
        } else {
            body_capacity(page_size)
        };
        let take = room.min(value.len().saturating_sub(offset));
        let payload = value
            .get(offset..offset.saturating_add(take))
            .unwrap_or(&[]);
        let previous = if head {
            0
        } else {
            first.0.saturating_add(index - 1)
        };
        let next = if index + 1 < count {
            first.0.saturating_add(index + 1)
        } else {
            0
        };
        let image = write_page(page_size, tree, (previous, next), key, payload)?;
        images.push((PageId(first.0.saturating_add(index)), image));
        offset = offset.saturating_add(take);
    }
    Ok(images)
}

/// Builds one owned page image.
///
/// @param page_size - the database's page size in bytes
/// @param tree - the tree the value belongs to
/// @param links - the previous and next page numbers, zero for none
/// @param key - the owner key, empty on every page but the first
/// @param payload - the payload bytes this page holds
fn write_page(
    page_size: usize,
    tree: u64,
    links: (u64, u64),
    key: &[u8],
    payload: &[u8],
) -> DbResult<Vec<u8>> {
    let mut image = vec![0u8; page_size];
    page::write_common(&mut image, PageKind::BlobExtent, 0, tree)?;
    if let Some(flags) = image.get_mut(page::header::FLAGS) {
        *flags |= EXTENT_OWNED;
    }
    page::write_u64(&mut image, at::PREV, links.0)?;
    page::write_u64(&mut image, at::NEXT, links.1)?;
    page::write_u32(
        &mut image,
        at::PAYLOAD,
        u32::try_from(payload.len()).unwrap_or(0),
    )?;
    page::write_u16(
        &mut image,
        at::KEY_LEN,
        u16::try_from(key.len()).unwrap_or(0),
    )?;
    let head = if links.0 == 0 { HEAD } else { 0 };
    page::write_u16(&mut image, at::FLAGS, head)?;
    let start = at::BODY.saturating_add(key.len());
    image
        .get_mut(at::BODY..start)
        .ok_or_else(|| misuse("the owner key does not fit its page"))?
        .copy_from_slice(key);
    image
        .get_mut(start..start.saturating_add(payload.len()))
        .ok_or_else(|| misuse("the extent payload does not fit its page"))?
        .copy_from_slice(payload);
    Ok(image)
}

/// Reports whether a page image is an extent page in the owned layout.
///
/// @param image - the page bytes
pub fn is_owned(image: &[u8]) -> bool {
    page::flags_of(image).is_ok_and(|flags| flags & EXTENT_OWNED != 0)
}

/// Returns one owned page's payload and the page after it.
///
/// @param image - the page bytes
pub fn read_payload(image: &[u8]) -> DbResult<(&[u8], PageId)> {
    let length = page::read_u32(image, at::PAYLOAD)? as usize;
    let key_len = page::read_u16(image, at::KEY_LEN)? as usize;
    let start = at::BODY.saturating_add(key_len);
    if start.saturating_add(length) > image.len() {
        return Err(corrupt(format!(
            "an extent page claims {length} payload bytes after a {key_len} byte key, more than it holds"
        )));
    }
    let body = image
        .get(start..start.saturating_add(length))
        .ok_or_else(|| corrupt("an extent payload runs past its page"))?;
    Ok((body, PageId(page::read_u64(image, at::NEXT)?)))
}

/// Returns the links of an owned page.
///
/// @param image - the page bytes
pub fn links(image: &[u8]) -> DbResult<Links> {
    Ok(Links {
        next: PageId(page::read_u64(image, at::NEXT)?),
        prev: PageId(page::read_u64(image, at::PREV)?),
        head: page::read_u16(image, at::FLAGS)? & HEAD != 0,
    })
}

/// Returns the owner key of an owned page, empty when none was stored.
///
/// @param image - the page bytes
pub fn owner_key(image: &[u8]) -> DbResult<&[u8]> {
    let key_len = page::read_u16(image, at::KEY_LEN)? as usize;
    image
        .get(at::BODY..at::BODY.saturating_add(key_len))
        .ok_or_else(|| corrupt("an extent owner key runs past its page"))
}

/// Writes the next page link of an owned page.
///
/// @param image - the page bytes
/// @param next - the page that now follows
pub fn set_next(image: &mut [u8], next: PageId) -> DbResult<()> {
    page::write_u64(image, at::NEXT, next.0)
}

/// Writes the previous page link of an owned page.
///
/// @param image - the page bytes
/// @param prev - the page that now precedes
pub fn set_prev(image: &mut [u8], prev: PageId) -> DbResult<()> {
    page::write_u64(image, at::PREV, prev.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A value spanning several pages reads back, and each page links both ways.
    #[test]
    fn a_chain_links_both_ways_and_reads_back() {
        let value: Vec<u8> = (0..5_000u32).map(|n| (n % 251) as u8).collect();
        let images = encode_run(&value, PageId(20), 512, 3, b"key").unwrap();
        assert_eq!(images.len() as u64, pages_needed(5_000, 512, 3));
        let mut joined = Vec::new();
        for (index, (id, image)) in images.iter().enumerate() {
            assert!(is_owned(image));
            let held = links(image).unwrap();
            assert_eq!(*id, PageId(20 + index as u64));
            assert_eq!(held.head, index == 0);
            let before = if index == 0 {
                PageId::NONE
            } else {
                PageId(19 + index as u64)
            };
            assert_eq!(held.prev, before);
            let (body, next) = read_payload(image).unwrap();
            assert_eq!(next, held.next);
            joined.extend_from_slice(body);
        }
        assert_eq!(joined, value);
        assert_eq!(owner_key(&images[0].1).unwrap(), b"key");
        assert!(owner_key(&images[1].1).unwrap().is_empty());
    }

    /// A key longer than a quarter of the page is not stored.
    #[test]
    fn a_long_owner_key_is_not_stored() {
        let key = vec![9u8; 200];
        let images = encode_run(b"value", PageId(5), 512, 1, &key).unwrap();
        assert!(owner_key(&images[0].1).unwrap().is_empty());
        assert_eq!(stored_key_len(512, &key), 0);
        assert_eq!(stored_key_len(512, &key[..128]), 128);
    }

    /// The page count matches what the encoder produces at the boundary.
    #[test]
    fn the_page_count_is_exact_at_the_boundary() {
        let head = head_capacity(512, 8) as u64;
        assert_eq!(pages_needed(0, 512, 8), 1);
        assert_eq!(pages_needed(head, 512, 8), 1);
        assert_eq!(pages_needed(head + 1, 512, 8), 2);
        for length in [0u64, head, head + 1, head + body_capacity(512) as u64 + 1] {
            let value = vec![7u8; length as usize];
            let images = encode_run(&value, PageId(2), 512, 1, &[0u8; 8]).unwrap();
            assert_eq!(images.len() as u64, pages_needed(length, 512, 8));
            assert!(images.len() as u64 <= chain_bound(length, 512));
        }
    }
}

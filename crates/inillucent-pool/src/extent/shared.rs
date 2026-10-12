//! The shared page: several small out-of-line values in one page.
//!
//! Invariant: a slot's bytes never move while the page lives, and a page in the
//! owned layout (`page::EXTENT_OWNED` set) says for every live slot which tree
//! and which row key own it, so the step that moves pages can rewrite the
//! reference in the owner's leaf. A page in the older layout says nothing about
//! its owners, and every function here reads either layout from the flag byte.
//!
//! A slotted page. The directory grows forward from [`at::DIRECTORY`] and the
//! payload grows backward from the end, which is the arrangement that lets one
//! number, the payload's low water mark, say how much room is left without
//! walking anything.
//!
//! | offset | size | field |
//! |---|---|---|
//! | 32 | 4 | how many slots the directory holds |
//! | 36 | 4 | how many of them are live |
//! | 40 | 4 | where the payload starts; everything above it is in use |
//! | 44 | 4 | reserved, zero |
//! | 48 | 8 or 16 each | the directory |
//!
//! In the older layout an entry is `(u32 offset, u32 length)`. In the owned layout
//! it is `(u32 offset, u32 length, u64 owner_tree)`, and the slot's bytes begin with
//! a `u16` key length and the owner key, then the value. The entry's `length`
//! counts the key prefix and the value. A key longer than a quarter of the page is
//! not stored, so its slot has a key length of 0 and its page is pinned.

use super::{corrupt, misuse, DbResult, PageId, MAX_SLOTS};
use crate::page::{self, PageKind, EXTENT_OWNED};

/// Byte offsets inside a shared extent page's own header.
pub mod at {
    /// How many slots the directory holds, 4 bytes.
    pub const SLOTS: usize = 32;
    /// How many of those slots are live, 4 bytes.
    pub const LIVE: usize = 36;
    /// Where the payload begins; everything from here to the end is in use.
    pub const FLOOR: usize = 40;
    /// The directory; bytes 44..48 are reserved and zero.
    pub const DIRECTORY: usize = 48;
}

/// How many bytes one directory entry takes in the older layout.
const ENTRY: usize = 8;

/// How many bytes one directory entry takes in the owned layout.
const OWNED_ENTRY: usize = 16;

/// How many bytes the key length takes at the front of an owned slot.
const KEY_PREFIX: usize = 2;

/// Who owns one slot of an owned shared page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotOwner {
    /// The tree whose leaf holds the reference.
    pub tree: u64,
    /// The row key as the tree encodes it, empty when none was stored.
    pub key: Vec<u8>,
}

/// Returns how many bytes a directory entry takes on this page.
///
/// @param image - the page bytes
fn entry_size(image: &[u8]) -> usize {
    match page::flags_of(image).is_ok_and(|flags| flags & EXTENT_OWNED != 0) {
        true => OWNED_ENTRY,
        false => ENTRY,
    }
}

/// Reports whether a page is in the owned layout.
///
/// @param image - the page bytes
pub fn is_owned(image: &[u8]) -> bool {
    entry_size(image) == OWNED_ENTRY
}

/// Writes an empty shared page in the older layout into a buffer.
///
/// @param image - the page buffer, exactly one page long
/// @param tree - the tree the values belong to
pub fn initialise(image: &mut [u8], tree: u64) -> DbResult<()> {
    let size = image.len();
    if size <= at::DIRECTORY {
        return Err(misuse("the page size leaves no room for a shared extent"));
    }
    for byte in image.iter_mut() {
        *byte = 0;
    }
    page::write_common(image, PageKind::BlobShared, 0, tree)?;
    page::write_u32(image, at::SLOTS, 0)?;
    page::write_u32(image, at::LIVE, 0)?;
    page::write_u32(image, at::FLOOR, u32::try_from(size).unwrap_or(u32::MAX))?;
    Ok(())
}

/// Writes an empty shared page in the owned layout into a buffer.
///
/// @param image - the page buffer, exactly one page long
/// @param tree - the tree the first value belongs to
pub fn initialise_owned(image: &mut [u8], tree: u64) -> DbResult<()> {
    initialise(image, tree)?;
    if let Some(flags) = image.get_mut(page::header::FLAGS) {
        *flags |= EXTENT_OWNED;
    }
    Ok(())
}

/// Returns how many slots a page's directory holds.
///
/// @param image - the page bytes
pub fn slots(image: &[u8]) -> DbResult<u32> {
    page::read_u32(image, at::SLOTS)
}

/// Returns how many of a page's slots still hold a value.
///
/// @param image - the page bytes
pub fn live(image: &[u8]) -> DbResult<u32> {
    page::read_u32(image, at::LIVE)
}

/// Returns how many bytes a value of this length would need on a page in the
/// older layout.
///
/// The value itself plus the directory entry that finds it.
///
/// @param length - the value's length
pub fn cost(length: usize) -> usize {
    length.saturating_add(ENTRY)
}

/// Returns how many bytes a slot of an owned page holds for a value.
///
/// The key length, the key and the value.
///
/// @param key_len - the length of the owner key stored in the slot
/// @param length - the value's length
pub fn owned_slot_len(key_len: usize, length: usize) -> usize {
    KEY_PREFIX.saturating_add(key_len).saturating_add(length)
}

/// Returns whether `length` bytes fit in what is left of a page.
///
/// The directory entry for the new slot is counted, at the size this page's
/// layout uses.
///
/// @param image - the page bytes
/// @param length - the bytes the slot will hold
pub fn has_room(image: &[u8], length: usize) -> DbResult<bool> {
    let count = slots(image)? as usize;
    if count >= MAX_SLOTS as usize {
        return Ok(false);
    }
    let floor = page::read_u32(image, at::FLOOR)? as usize;
    if floor > image.len() {
        return Err(corrupt(
            "a shared extent page's payload starts past its end",
        ));
    }
    let entry = entry_size(image);
    let directory = at::DIRECTORY.saturating_add(count.saturating_add(1).saturating_mul(entry));
    Ok(floor.saturating_sub(directory) >= length)
}

/// Returns the longest value a *fresh* shared page in the older layout can hold.
///
/// **A shared page holds less than a dedicated one, and the difference is
/// where a bound TEXT of 32,700 bytes was lost.** [`super::payload_capacity`] is
/// what an extent page of its own holds, the page minus its header, and a shared
/// page spends another forty eight bytes on its own header and eight on each
/// directory entry. `write_extent` routed by the first number and then placed by
/// the second, so a value between the two reached [`place`] on a page freshly
/// made for it, was refused with `a shared extent page has no room for this
/// value`, and the caller read `bad parameter or other API misuse`. Measured on
/// the default 32,768 byte page: 32,000 bytes stored and 32,700 did not.
///
/// @param page_size - the database's page size in bytes
pub fn capacity(page_size: usize) -> usize {
    page_size.saturating_sub(at::DIRECTORY.saturating_add(ENTRY))
}

/// Returns the longest value a fresh shared page in the owned layout can hold.
///
/// @param page_size - the database's page size in bytes
/// @param key_len - the length of the owner key stored with the value
pub fn owned_capacity(page_size: usize, key_len: usize) -> usize {
    page_size
        .saturating_sub(at::DIRECTORY.saturating_add(OWNED_ENTRY))
        .saturating_sub(KEY_PREFIX)
        .saturating_sub(key_len)
}

/// Places bytes on a page and returns the slot they went into.
///
/// @param image - the page bytes
/// @param bytes - the slot's bytes, key prefix included on an owned page
/// @param owner_tree - the owner tree written in the entry of an owned page
fn place_bytes(image: &mut [u8], bytes: &[u8], owner_tree: u64) -> DbResult<u16> {
    if !has_room(image, bytes.len())? {
        return Err(misuse("a shared extent page has no room for this value"));
    }
    let count = slots(image)?;
    let floor = page::read_u32(image, at::FLOOR)? as usize;
    let start = floor.saturating_sub(bytes.len());
    image
        .get_mut(start..floor)
        .ok_or_else(|| corrupt("a shared extent payload runs past its page"))?
        .copy_from_slice(bytes);
    let entry = at::DIRECTORY.saturating_add((count as usize).saturating_mul(entry_size(image)));
    page::write_u32(image, entry, u32::try_from(start).unwrap_or(0))?;
    page::write_u32(
        image,
        entry.saturating_add(4),
        u32::try_from(bytes.len()).unwrap_or(0),
    )?;
    if is_owned(image) {
        page::write_u64(image, entry.saturating_add(8), owner_tree)?;
    }
    page::write_u32(image, at::SLOTS, count.saturating_add(1))?;
    page::write_u32(image, at::LIVE, live(image)?.saturating_add(1))?;
    page::write_u32(image, at::FLOOR, u32::try_from(start).unwrap_or(0))?;
    u16::try_from(count).map_err(|_| corrupt("a shared extent page has too many slots"))
}

/// Places a value on a page in the older layout and returns its slot.
///
/// @param image - the page bytes
/// @param value - the bytes to store
pub fn place(image: &mut [u8], value: &[u8]) -> DbResult<u16> {
    if is_owned(image) {
        return Err(misuse("a value with no owner was placed on an owned page"));
    }
    place_bytes(image, value, 0)
}

/// Places a value and its owner on a page in the owned layout and returns its slot.
///
/// @param image - the page bytes
/// @param tree - the tree whose leaf holds the reference
/// @param key - the row key as the tree encodes it, empty when too long to store
/// @param value - the bytes to store
pub fn place_owned(image: &mut [u8], tree: u64, key: &[u8], value: &[u8]) -> DbResult<u16> {
    if !is_owned(image) {
        return Err(misuse(
            "an owned value was placed on a page in the older layout",
        ));
    }
    let mut bytes = Vec::with_capacity(owned_slot_len(key.len(), value.len()));
    bytes.extend_from_slice(&u16::try_from(key.len()).unwrap_or(0).to_le_bytes());
    bytes.extend_from_slice(key);
    bytes.extend_from_slice(value);
    place_bytes(image, &bytes, tree)
}

/// Returns the offset of one directory entry.
///
/// @param image - the page bytes
/// @param slot - which slot
fn entry_at(image: &[u8], slot: u16) -> usize {
    at::DIRECTORY.saturating_add((slot as usize).saturating_mul(entry_size(image)))
}

/// Returns the bytes one live slot holds, key prefix included.
///
/// @param image - the page bytes
/// @param slot - which slot
fn slot_bytes(image: &[u8], slot: u16) -> DbResult<&[u8]> {
    if page::kind_of(image)? != PageKind::BlobShared {
        return Err(corrupt("page is not a shared blob extent"));
    }
    let count = slots(image)?;
    if u32::from(slot) >= count {
        return Err(corrupt(format!(
            "a reference names slot {slot} of a shared page that holds {count}"
        )));
    }
    let entry = entry_at(image, slot);
    let start = page::read_u32(image, entry)? as usize;
    let length = page::read_u32(image, entry.saturating_add(4))? as usize;
    if start == 0 && length == 0 {
        return Err(corrupt(format!(
            "a reference names slot {slot} of a shared page, which has been freed"
        )));
    }
    image
        .get(start..start.saturating_add(length))
        .ok_or_else(|| corrupt("a shared extent slot runs past its page"))
}

/// Splits an owned slot's bytes into its key and its value.
///
/// @param held - the slot's bytes
fn split_owned(held: &[u8]) -> DbResult<(&[u8], &[u8])> {
    let key_len = held
        .get(..KEY_PREFIX)
        .and_then(|raw| <[u8; 2]>::try_from(raw).ok())
        .map(|raw| u16::from_le_bytes(raw) as usize)
        .ok_or_else(|| corrupt("a shared extent slot is shorter than its key length"))?;
    let key_end = KEY_PREFIX.saturating_add(key_len);
    let key = held
        .get(KEY_PREFIX..key_end)
        .ok_or_else(|| corrupt("a shared extent slot's key runs past the slot"))?;
    Ok((key, held.get(key_end..).unwrap_or(&[])))
}

/// Returns one slot's value.
///
/// @param image - the page bytes
/// @param slot - which slot
pub fn read(image: &[u8], slot: u16) -> DbResult<&[u8]> {
    let held = slot_bytes(image, slot)?;
    match is_owned(image) {
        true => split_owned(held).map(|(_, value)| value),
        false => Ok(held),
    }
}

/// Returns who owns one live slot of an owned page.
///
/// `None` for a page in the older layout, which records no owner.
///
/// @param image - the page bytes
/// @param slot - which slot
pub fn owner_of(image: &[u8], slot: u16) -> DbResult<Option<SlotOwner>> {
    if !is_owned(image) {
        return Ok(None);
    }
    let held = slot_bytes(image, slot)?;
    let (key, _) = split_owned(held)?;
    let tree = page::read_u64(image, entry_at(image, slot).saturating_add(8))?;
    Ok(Some(SlotOwner {
        tree,
        key: key.to_vec(),
    }))
}

/// Returns the slots of a page that still hold a value, in slot order.
///
/// @param image - the page bytes
pub fn live_slots(image: &[u8]) -> DbResult<Vec<u16>> {
    let count = slots(image)?;
    let mut held = Vec::new();
    for slot in 0..count {
        let Ok(slot) = u16::try_from(slot) else {
            break;
        };
        let entry = entry_at(image, slot);
        let start = page::read_u32(image, entry)?;
        let length = page::read_u32(image, entry.saturating_add(4))?;
        if start != 0 || length != 0 {
            held.push(slot);
        }
    }
    Ok(held)
}

/// Clears one slot and returns how many are still live.
///
/// The bytes are left where they are: compacting a page would move a value
/// whose reference lives in a leaf this function cannot see. The page goes
/// back to the free map when nothing on it is live, which is the caller's
/// decision to make from the number this returns.
///
/// @param image - the page bytes
/// @param slot - which slot
pub fn clear(image: &mut [u8], slot: u16) -> DbResult<u32> {
    if page::kind_of(image)? != PageKind::BlobShared {
        return Err(corrupt("page is not a shared blob extent"));
    }
    let count = slots(image)?;
    if u32::from(slot) >= count {
        return Err(corrupt(format!(
            "a free names slot {slot} of a shared page that holds {count}"
        )));
    }
    let entry = entry_at(image, slot);
    let length = page::read_u32(image, entry.saturating_add(4))?;
    let start = page::read_u32(image, entry)?;
    if start == 0 && length == 0 {
        // Already cleared. Freeing twice is not damage - a repack that
        // carried a reference and a caller that freed it can arrive here in
        // either order - so the live count is left alone.
        return live(image);
    }
    page::write_u32(image, entry, 0)?;
    page::write_u32(image, entry.saturating_add(4), 0)?;
    if is_owned(image) {
        page::write_u64(image, entry.saturating_add(8), 0)?;
    }
    let remaining = live(image)?.saturating_sub(1);
    page::write_u32(image, at::LIVE, remaining)?;
    Ok(remaining)
}

/// Returns whether a page is a shared extent page.
///
/// @param image - the page bytes
pub fn is_shared(image: &[u8]) -> bool {
    matches!(page::kind_of(image), Ok(PageKind::BlobShared))
}

/// The page a shared reference names, for a caller reading one.
///
/// @param reference - the reference
pub fn page_of(reference: &super::ExtentRef) -> PageId {
    reference.first
}

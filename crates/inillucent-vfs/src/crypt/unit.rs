//! Units: the pieces of plaintext encrypted together, and where each one
//! sits in the file.
//!
//! Invariant: a write to one unit touches no 512 byte sector that holds any
//! part of another unit. That is what keeps a torn write inside the unit being
//! written, which is the guarantee a plaintext file gives the engine and the
//! one `inillucent_sim`'s disk model tests.
//!
//! Each unit is its plaintext, encrypted with XChaCha20-Poly1305, followed by
//! a 64 byte trailer: the nonce, the tag, the unit's generation and its
//! plaintext length. The associated data binds the unit to its file and to its
//! position, so a unit copied to another place fails its tag.

use inillucent_base::crypt::aead;

/// The bytes a unit's trailer takes.
pub const TRAILER: usize = 64;

/// The size of one slot of a double slot unit, and of a temporary file unit.
pub const SMALL_SLOT: u64 = 4_096;

/// The plaintext in one small slot.
pub const SMALL_UNIT: u32 = 4_032;

/// The sector a single slot database unit's trailer is padded to.
pub const TRAILER_SECTOR: u64 = 512;

/// How units are placed in a file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layout {
    /// One copy of each unit. The database file and temporary files.
    Single {
        /// Plaintext bytes per unit.
        unit: u32,
        /// Bytes the unit, its trailer and any padding take on disk.
        footprint: u64,
    },
    /// Two copies of each unit, written in turn. Logs and journals.
    Double,
}

impl Layout {
    /// The layout of a database file whose pages are `page` bytes.
    ///
    /// The trailer gets a whole sector of its own, so the next unit starts on
    /// a sector boundary.
    ///
    /// @param page - the page size, which is the unit size
    pub fn database(page: u32) -> Layout {
        Layout::Single {
            unit: page,
            footprint: u64::from(page).saturating_add(TRAILER_SECTOR),
        }
    }

    /// The layout of a temporary file: one small slot per unit.
    pub fn temporary() -> Layout {
        Layout::Single {
            unit: SMALL_UNIT,
            footprint: SMALL_SLOT,
        }
    }

    /// The layout code a header records.
    pub fn code(self) -> u32 {
        match self {
            Layout::Single { .. } => 1,
            Layout::Double => 2,
        }
    }

    /// Rebuilds a layout from what a header records.
    ///
    /// @param code - 1 or 2
    /// @param unit - the unit size the header records
    pub fn from_header(code: u32, unit: u32) -> Option<Layout> {
        match (code, unit) {
            (1, SMALL_UNIT) => Some(Layout::temporary()),
            // Any power of two a page can be. The engine's own API builds
            // 8 to 64 KiB, and its tests also build 4 KiB pages.
            (1, 512..=65_536) if unit.is_power_of_two() => Some(Layout::database(unit)),
            (2, SMALL_UNIT) => Some(Layout::Double),
            _ => None,
        }
    }

    /// Plaintext bytes per unit.
    pub fn unit(self) -> u32 {
        match self {
            Layout::Single { unit, .. } => unit,
            Layout::Double => SMALL_UNIT,
        }
    }

    /// Bytes one unit takes on disk, both slots counted.
    pub fn stride(self) -> u64 {
        match self {
            Layout::Single { footprint, .. } => footprint,
            Layout::Double => SMALL_SLOT.saturating_mul(2),
        }
    }

    /// Bytes one copy of a unit takes on disk.
    pub fn slot_bytes(self) -> usize {
        match self {
            Layout::Single { unit, .. } => (unit as usize).saturating_add(TRAILER),
            Layout::Double => SMALL_SLOT as usize,
        }
    }

    /// How many copies each unit has.
    pub fn slots(self) -> u64 {
        match self {
            Layout::Single { .. } => 1,
            Layout::Double => 2,
        }
    }
}

/// A unit's trailer, decoded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Trailer {
    /// The nonce the unit was sealed with.
    pub nonce: [u8; 24],
    /// The tag over it.
    pub tag: [u8; 16],
    /// Which write of this unit this is.
    pub generation: u64,
    /// Plaintext bytes that belong to the file.
    pub length: u32,
}

impl Trailer {
    /// Decodes a trailer from its 64 bytes.
    ///
    /// @param bytes - the trailer
    pub fn decode(bytes: &[u8]) -> Trailer {
        let mut trailer = Trailer::default();
        if let Some(source) = bytes.get(..24) {
            trailer.nonce.copy_from_slice(source);
        }
        if let Some(source) = bytes.get(24..40) {
            trailer.tag.copy_from_slice(source);
        }
        let mut eight = [0u8; 8];
        if let Some(source) = bytes.get(40..48) {
            eight.copy_from_slice(source);
        }
        trailer.generation = u64::from_le_bytes(eight);
        let mut four = [0u8; 4];
        if let Some(source) = bytes.get(48..52) {
            four.copy_from_slice(source);
        }
        trailer.length = u32::from_le_bytes(four);
        trailer
    }

    /// Encodes the trailer into its 64 bytes.
    pub fn encode(&self) -> [u8; TRAILER] {
        let mut out = [0u8; TRAILER];
        if let Some(target) = out.get_mut(..24) {
            target.copy_from_slice(&self.nonce);
        }
        if let Some(target) = out.get_mut(24..40) {
            target.copy_from_slice(&self.tag);
        }
        if let Some(target) = out.get_mut(40..48) {
            target.copy_from_slice(&self.generation.to_le_bytes());
        }
        if let Some(target) = out.get_mut(48..52) {
            target.copy_from_slice(&self.length.to_le_bytes());
        }
        out
    }
}

/// The associated data of one unit: which file, which unit, which write,
/// and how much of it is the file's.
///
/// @param file_id - the file's random id
/// @param index - the unit number
/// @param generation - the unit's generation
/// @param length - the unit's plaintext length
fn associated_data(file_id: &[u8; 16], index: u64, generation: u64, length: u32) -> [u8; 36] {
    let mut out = [0u8; 36];
    if let Some(target) = out.get_mut(..16) {
        target.copy_from_slice(file_id);
    }
    if let Some(target) = out.get_mut(16..24) {
        target.copy_from_slice(&index.to_le_bytes());
    }
    if let Some(target) = out.get_mut(24..32) {
        target.copy_from_slice(&generation.to_le_bytes());
    }
    if let Some(target) = out.get_mut(32..36) {
        target.copy_from_slice(&length.to_le_bytes());
    }
    out
}

/// What reading one copy of a unit found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Found {
    /// Never written: the trailer and the data are zero. Reads as zeros.
    Hole,
    /// Written and whole. The plaintext, and its trailer.
    Whole(Vec<u8>, Trailer),
    /// Written and not whole: torn, or changed by somebody without the key.
    Damaged,
}

/// Seals one unit: encrypts `plain` in place and returns its trailer bytes.
///
/// @param key - the data key
/// @param file_id - the file's random id
/// @param index - the unit number
/// @param generation - this write's generation
/// @param length - plaintext bytes that belong to the file
/// @param nonce - a nonce never used with this key
/// @param plain - the whole unit's plaintext, replaced by its ciphertext
pub fn seal(
    key: &[u8; 32],
    file_id: &[u8; 16],
    index: u64,
    generation: u64,
    length: u32,
    nonce: [u8; 24],
    plain: &mut [u8],
) -> [u8; TRAILER] {
    let aad = associated_data(file_id, index, generation, length);
    let tag = aead::seal(key, &nonce, &aad, plain);
    Trailer {
        nonce,
        tag,
        generation,
        length,
    }
    .encode()
}

/// Opens one copy of a unit: its ciphertext followed by its trailer.
///
/// @param key - the data key
/// @param file_id - the file's random id
/// @param index - the unit number
/// @param unit - plaintext bytes per unit
/// @param copy - the bytes of this copy as they are on disk
pub fn open(key: &[u8; 32], file_id: &[u8; 16], index: u64, unit: usize, copy: &[u8]) -> Found {
    let trailer_bytes = copy.get(unit..unit.saturating_add(TRAILER)).unwrap_or(&[]);
    let data = copy.get(..unit).unwrap_or(&[]);
    if trailer_bytes.iter().all(|byte| *byte == 0) {
        return match data.iter().all(|byte| *byte == 0) {
            true => Found::Hole,
            false => Found::Damaged,
        };
    }
    let trailer = Trailer::decode(trailer_bytes);
    if trailer.length as usize > unit || data.len() != unit {
        return Found::Damaged;
    }
    let aad = associated_data(file_id, index, trailer.generation, trailer.length);
    let mut plain = data.to_vec();
    match aead::open(key, &trailer.nonce, &aad, &mut plain, &trailer.tag) {
        true => Found::Whole(plain, trailer),
        false => Found::Damaged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sealed unit opens to its plaintext, and a copy moved to another
    /// index or file does not.
    #[test]
    fn a_unit_opens_only_where_it_was_written() {
        let key = [1u8; 32];
        let file = [2u8; 16];
        let mut data = vec![7u8; 4_032];
        let trailer = seal(&key, &file, 3, 1, 4_032, [5; 24], &mut data);
        let mut copy = data.clone();
        copy.extend_from_slice(&trailer);
        match open(&key, &file, 3, 4_032, &copy) {
            Found::Whole(plain, found) => {
                assert_eq!(plain, vec![7u8; 4_032]);
                assert_eq!(found.generation, 1);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(open(&key, &file, 4, 4_032, &copy), Found::Damaged);
        assert_eq!(open(&key, &[9u8; 16], 3, 4_032, &copy), Found::Damaged);
        assert_eq!(open(&[3u8; 32], &file, 3, 4_032, &copy), Found::Damaged);
    }

    /// Zeros are a hole, and zeros with data behind them are damage.
    #[test]
    fn a_zero_trailer_is_a_hole_only_over_zero_data() {
        let zeros = vec![0u8; 4_096];
        assert_eq!(open(&[1; 32], &[2; 16], 0, 4_032, &zeros), Found::Hole);
        let mut torn = zeros.clone();
        torn[10] = 1;
        assert_eq!(open(&[1; 32], &[2; 16], 0, 4_032, &torn), Found::Damaged);
    }

    /// Every database layout starts each unit on a sector boundary, and a
    /// small slot is exactly one plaintext unit plus its trailer.
    #[test]
    fn every_layout_keeps_units_on_sector_boundaries() {
        for page in [4_096u32, 8_192, 16_384, 32_768, 65_536] {
            let layout = Layout::database(page);
            assert_eq!(layout.stride() % 512, 0);
            assert!(layout.slot_bytes() as u64 <= layout.stride());
            assert_eq!(Layout::from_header(layout.code(), page), Some(layout));
        }
        assert_eq!(SMALL_UNIT as usize + TRAILER, SMALL_SLOT as usize);
        assert_eq!(Layout::Double.stride() % 512, 0);
        assert_eq!(Layout::from_header(2, SMALL_UNIT), Some(Layout::Double));
        assert_eq!(Layout::from_header(1, 12_345), None);
    }
}

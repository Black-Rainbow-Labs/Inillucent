//! The header at the front of every encrypted file.
//!
//! Invariant: a header is read from the newest of its two slots whose SHA-256
//! matches, and only from that slot. The SHA-256 needs no key and tells a torn
//! slot from a whole one; the AEAD tag on the wrapped data key is what tells a
//! right key from a wrong one, and it covers every field but the generation.
//! Rewriting the older slot and syncing before touching the newer one is what
//! lets `PRAGMA rekey` change the header without a moment where neither slot
//! is whole.

use inillucent_base::crypt::aead;
use inillucent_base::hash::sha256;

/// The bytes one header slot takes on disk.
pub const SLOT_BYTES: usize = 4_096;

/// The bytes the two header slots take together, before the first unit.
pub const HEADER_AREA: u64 = 8_192;

/// The first eight bytes of every header slot.
pub const MAGIC: [u8; 8] = *b"INLCRYPT";

/// The format version this build writes and reads.
pub const VERSION: u32 = 1;

/// The cipher code for XChaCha20-Poly1305.
pub const CIPHER_XCHACHA: u32 = 1;

/// The key derivation code for a raw 32 byte key.
pub const KDF_RAW: u32 = 0;

/// The key derivation code for PBKDF2-HMAC-SHA256.
pub const KDF_PBKDF2: u32 = 1;

/// How many bytes of a slot the associated data of the wrapped key covers.
const AAD_END: usize = 80;

/// Where the SHA-256 of the slot is kept, and how many bytes it covers.
const DIGEST_AT: usize = 160;

/// The fields of one header slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    /// How the key encryption key is made: [`KDF_RAW`] or [`KDF_PBKDF2`].
    pub kdf: u32,
    /// The PBKDF2 iteration count, zero for a raw key.
    pub iterations: u32,
    /// 1 for single slot units, 2 for double slot units.
    pub layout: u32,
    /// Plaintext bytes per unit.
    pub unit: u32,
    /// The PBKDF2 salt.
    pub salt: [u8; 16],
    /// Shared by a database and every file beside it.
    pub database_id: [u8; 16],
    /// Random per file, and part of every unit's associated data.
    pub file_id: [u8; 16],
    /// Which write of this header this is. The newest whole slot wins.
    pub generation: u64,
    /// The nonce the data key was wrapped with.
    pub wrap_nonce: [u8; 24],
    /// The wrapped data key: 32 bytes of ciphertext.
    pub wrapped: [u8; 32],
    /// The tag over the wrapped data key.
    pub wrap_tag: [u8; 16],
}

/// Copies `source` into `slot` at `at`, when it fits.
///
/// @param slot - the slot bytes
/// @param at - the offset
/// @param source - what to write
fn put(slot: &mut [u8], at: usize, source: &[u8]) {
    if let Some(target) = slot.get_mut(at..at.saturating_add(source.len())) {
        target.copy_from_slice(source);
    }
}

/// Reads `N` bytes of a slot at `at`, or zeros when the slot is short.
///
/// @param slot - the slot bytes
/// @param at - the offset
fn take<const N: usize>(slot: &[u8], at: usize) -> [u8; N] {
    let mut out = [0u8; N];
    if let Some(source) = slot.get(at..at.saturating_add(N)) {
        out.copy_from_slice(source);
    }
    out
}

/// Reads a little endian `u32` at `at`.
///
/// @param slot - the slot bytes
/// @param at - the offset
fn word(slot: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(take(slot, at))
}

impl Header {
    /// Encodes the fields that the wrapped key's associated data covers.
    ///
    /// Everything but the generation, the wrap and the digest: bytes 0 to 80.
    pub fn associated_data(&self) -> [u8; AAD_END] {
        let mut out = [0u8; AAD_END];
        put(&mut out, 0, &MAGIC);
        put(&mut out, 8, &VERSION.to_le_bytes());
        put(&mut out, 12, &CIPHER_XCHACHA.to_le_bytes());
        put(&mut out, 16, &self.kdf.to_le_bytes());
        put(&mut out, 20, &self.iterations.to_le_bytes());
        put(&mut out, 24, &self.layout.to_le_bytes());
        put(&mut out, 28, &self.unit.to_le_bytes());
        put(&mut out, 32, &self.salt);
        put(&mut out, 48, &self.database_id);
        put(&mut out, 64, &self.file_id);
        out
    }

    /// Encodes one whole slot, digest included.
    pub fn encode(&self) -> Vec<u8> {
        let mut slot = vec![0u8; SLOT_BYTES];
        put(&mut slot, 0, &self.associated_data());
        put(&mut slot, 80, &self.generation.to_le_bytes());
        put(&mut slot, 88, &self.wrap_nonce);
        put(&mut slot, 112, &self.wrapped);
        put(&mut slot, 144, &self.wrap_tag);
        let digest = sha256(slot.get(..DIGEST_AT).unwrap_or(&[]));
        put(&mut slot, DIGEST_AT, &digest);
        slot
    }

    /// Decodes one slot, or `None` when it is torn, blank, or from a format
    /// this build does not know.
    ///
    /// @param slot - the slot bytes
    pub fn decode(slot: &[u8]) -> Option<Header> {
        if take::<8>(slot, 0) != MAGIC {
            return None;
        }
        let digest: [u8; 32] = take(slot, DIGEST_AT);
        if sha256(slot.get(..DIGEST_AT)?) != digest {
            return None;
        }
        if word(slot, 8) != VERSION || word(slot, 12) != CIPHER_XCHACHA {
            return None;
        }
        Some(Header {
            kdf: word(slot, 16),
            iterations: word(slot, 20),
            layout: word(slot, 24),
            unit: word(slot, 28),
            salt: take(slot, 32),
            database_id: take(slot, 48),
            file_id: take(slot, 64),
            generation: u64::from_le_bytes(take(slot, 80)),
            wrap_nonce: take(slot, 88),
            wrapped: take(slot, 112),
            wrap_tag: take(slot, 144),
        })
    }

    /// Picks the header from the two slots: the newest whole one.
    ///
    /// @param area - the first [`HEADER_AREA`] bytes of the file
    pub fn newest(area: &[u8]) -> Option<(Header, usize)> {
        let first = area.get(..SLOT_BYTES).and_then(Header::decode);
        let second = area
            .get(SLOT_BYTES..SLOT_BYTES.saturating_mul(2))
            .and_then(Header::decode);
        match (first, second) {
            (Some(a), Some(b)) if b.generation > a.generation => Some((b, 1)),
            (Some(a), _) => Some((a, 0)),
            (None, Some(b)) => Some((b, 1)),
            (None, None) => None,
        }
    }

    /// Wraps a data key under a key encryption key, filling the three wrap
    /// fields.
    ///
    /// @param kek - the key encryption key
    /// @param data_key - the key being wrapped
    /// @param nonce - a nonce never used with this key encryption key
    pub fn wrap(&mut self, kek: &[u8; 32], data_key: &[u8; 32], nonce: [u8; 24]) {
        let mut sealed = *data_key;
        let tag = aead::seal(kek, &nonce, &self.associated_data(), &mut sealed);
        self.wrap_nonce = nonce;
        self.wrapped = sealed;
        self.wrap_tag = tag;
        sealed.fill(0);
    }

    /// Unwraps the data key, or `None` when the key encryption key is wrong
    /// or a field the tag covers was changed.
    ///
    /// @param kek - the key encryption key to try
    pub fn unwrap_key(&self, kek: &[u8; 32]) -> Option<[u8; 32]> {
        let mut opened = self.wrapped;
        if aead::open(
            kek,
            &self.wrap_nonce,
            &self.associated_data(),
            &mut opened,
            &self.wrap_tag,
        ) {
            Some(opened)
        } else {
            None
        }
    }
}

/// Reports whether the first bytes of a file begin like an encrypted file.
///
/// Used to tell a torn header from a file that was never encrypted: a torn
/// creation leaves zeros or a prefix of the magic.
///
/// @param head - the file's first bytes
pub fn starts_like_a_header(head: &[u8]) -> bool {
    head.iter().all(|byte| *byte == 0)
        || head
            .iter()
            .zip(MAGIC.iter())
            .all(|(byte, magic)| byte == magic)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A header built for these tests.
    fn sample() -> Header {
        Header {
            kdf: KDF_PBKDF2,
            iterations: 600_000,
            layout: 1,
            unit: 32_768,
            salt: [1; 16],
            database_id: [2; 16],
            file_id: [3; 16],
            generation: 7,
            wrap_nonce: [0; 24],
            wrapped: [0; 32],
            wrap_tag: [0; 16],
        }
    }

    /// A slot decodes to what was encoded, and the key it wraps comes back
    /// under the right key and not under another.
    #[test]
    fn a_slot_round_trips_and_wraps_its_key() {
        let mut header = sample();
        header.wrap(&[9; 32], &[5; 32], [4; 24]);
        let slot = header.encode();
        let decoded = Header::decode(&slot).expect("the slot decodes");
        assert_eq!(decoded, header);
        assert_eq!(decoded.unwrap_key(&[9; 32]), Some([5; 32]));
        assert_eq!(decoded.unwrap_key(&[8; 32]), None);
    }

    /// Any changed byte before the digest makes the slot torn.
    #[test]
    fn a_changed_byte_tears_the_slot() {
        let mut header = sample();
        header.wrap(&[9; 32], &[5; 32], [4; 24]);
        let slot = header.encode();
        for index in [0usize, 10, 20, 40, 70, 85, 100, 150] {
            let mut changed = slot.clone();
            changed[index] ^= 1;
            assert!(Header::decode(&changed).is_none(), "byte {index}");
        }
    }

    /// A changed field that the digest still covers, because the digest was
    /// recomputed, is caught by the wrapped key's tag.
    #[test]
    fn a_rewritten_field_fails_the_key_tag() {
        let mut header = sample();
        header.wrap(&[9; 32], &[5; 32], [4; 24]);
        let mut forged = header.clone();
        forged.unit = 4_096;
        let decoded = Header::decode(&forged.encode()).expect("the digest was recomputed");
        assert_eq!(decoded.unwrap_key(&[9; 32]), None);
        let mut regenerated = header.clone();
        regenerated.generation = 99;
        let decoded = Header::decode(&regenerated.encode()).expect("decodes");
        assert_eq!(
            decoded.unwrap_key(&[9; 32]),
            Some([5; 32]),
            "the generation is outside the tag on purpose"
        );
    }

    /// The newer whole slot wins, and a torn newer slot falls back.
    #[test]
    fn the_newest_whole_slot_is_the_header() {
        let mut older = sample();
        older.generation = 1;
        let mut newer = sample();
        newer.generation = 2;
        let mut area = older.encode();
        area.extend(newer.encode());
        assert_eq!(
            Header::newest(&area).map(|(h, slot)| (h.generation, slot)),
            Some((2, 1))
        );
        area[SLOT_BYTES + 3] ^= 0xff;
        assert_eq!(
            Header::newest(&area).map(|(h, slot)| (h.generation, slot)),
            Some((1, 0))
        );
        assert!(Header::newest(&[0u8; 8_192]).is_none());
    }
}

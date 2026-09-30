//! HMAC-SHA256 (RFC 2104) and PBKDF2-HMAC-SHA256 (RFC 8018).
//!
//! Invariant: both match their published test vectors, which are checked
//! below. PBKDF2 runs its inner loop on precomputed SHA-256 states and one
//! compression per hash, because a passphrase open runs 1.2 million of them
//! and the streaming hasher's byte by byte padding would triple that cost.

use crate::hash::{compress, Sha256, INITIAL};

/// The SHA-256 block size, in bytes.
const BLOCK: usize = 64;

/// The SHA-256 states after absorbing the key XOR the inner and outer pads.
#[derive(Clone)]
struct Keyed {
    /// After `key ^ 0x36`.
    inner: [u32; 8],
    /// After `key ^ 0x5c`.
    outer: [u32; 8],
}

impl Keyed {
    /// Prepares HMAC-SHA256 for a key of any length.
    ///
    /// @param key - the key; a key longer than a block is hashed first
    fn new(key: &[u8]) -> Keyed {
        let mut block = [0u8; BLOCK];
        if key.len() > BLOCK {
            let digest = crate::hash::sha256(key);
            if let Some(target) = block.get_mut(..32) {
                target.copy_from_slice(&digest);
            }
        } else if let Some(target) = block.get_mut(..key.len()) {
            target.copy_from_slice(key);
        }
        let mut inner = INITIAL;
        let mut outer = INITIAL;
        compress(&mut inner, &block.map(|byte| byte ^ 0x36));
        compress(&mut outer, &block.map(|byte| byte ^ 0x5c));
        block.fill(0);
        Keyed { inner, outer }
    }

    /// Finishes one hash whose input after the keyed block is exactly 32
    /// bytes, from a state that has absorbed that keyed block.
    ///
    /// @param state - the keyed state
    /// @param message - the 32 bytes
    fn one_block(state: &[u32; 8], message: &[u8; 32]) -> [u8; 32] {
        let mut block = [0u8; BLOCK];
        if let Some(target) = block.get_mut(..32) {
            target.copy_from_slice(message);
        }
        if let Some(marker) = block.get_mut(32) {
            *marker = 0x80;
        }
        // The length is the keyed block plus these 32 bytes: 96 bytes, 768 bits.
        if let Some(target) = block.get_mut(56..) {
            target.copy_from_slice(&768u64.to_be_bytes());
        }
        let mut next = *state;
        compress(&mut next, &block);
        digest_of(&next)
    }

    /// HMAC of a 32 byte message, which is every iteration after the first.
    ///
    /// @param message - the previous iteration's output
    fn mac32(&self, message: &[u8; 32]) -> [u8; 32] {
        let inner = Keyed::one_block(&self.inner, message);
        Keyed::one_block(&self.outer, &inner)
    }
}

/// Serialises a SHA-256 state as its big endian digest.
///
/// @param state - the eight words
fn digest_of(state: &[u32; 8]) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (chunk, word) in out.chunks_exact_mut(4).zip(state.iter()) {
        chunk.copy_from_slice(&word.to_be_bytes());
    }
    out
}

/// Returns HMAC-SHA256 of a message.
///
/// @param key - the key
/// @param message - the bytes to authenticate
pub fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = crate::hash::sha256(key);
        if let Some(target) = block.get_mut(..32) {
            target.copy_from_slice(&digest);
        }
    } else if let Some(target) = block.get_mut(..key.len()) {
        target.copy_from_slice(key);
    }
    let mut inner = Sha256::new();
    inner.update(&block.map(|byte| byte ^ 0x36));
    inner.update(message);
    let inner = inner.finish();
    let mut outer = Sha256::new();
    outer.update(&block.map(|byte| byte ^ 0x5c));
    outer.update(&inner);
    block.fill(0);
    outer.finish()
}

/// Derives key material from a passphrase with PBKDF2-HMAC-SHA256.
///
/// @param password - the passphrase bytes
/// @param salt - the salt stored beside what is being protected
/// @param iterations - the work factor; zero is treated as one
/// @param output - filled with the derived bytes
pub fn pbkdf2_sha256(password: &[u8], salt: &[u8], iterations: u32, output: &mut [u8]) {
    let keyed = Keyed::new(password);
    for (index, chunk) in output.chunks_mut(32).enumerate() {
        let block_number = u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1);
        let mut first = salt.to_vec();
        first.extend_from_slice(&block_number.to_be_bytes());
        let mut previous = hmac_sha256(password, &first);
        let mut total = previous;
        for _ in 1..iterations.max(1) {
            previous = keyed.mac32(&previous);
            for (sum, byte) in total.iter_mut().zip(previous.iter()) {
                *sum ^= byte;
            }
        }
        chunk.copy_from_slice(total.get(..chunk.len()).unwrap_or(&[]));
        total.fill(0);
        previous.fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypt::{from_hex, hex_bytes};

    /// RFC 4231 test cases 1, 2, 3, 4, 6 and 7.
    #[test]
    fn hmac_matches_rfc_4231() {
        let cases: [(Vec<u8>, Vec<u8>, &str); 6] = [
            (
                vec![0x0b; 20],
                b"Hi There".to_vec(),
                "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7",
            ),
            (
                b"Jefe".to_vec(),
                b"what do ya want for nothing?".to_vec(),
                "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843",
            ),
            (
                vec![0xaa; 20],
                vec![0xdd; 50],
                "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe",
            ),
            (
                hex_bytes("0102030405060708090a0b0c0d0e0f10111213141516171819"),
                vec![0xcd; 50],
                "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b",
            ),
            (
                vec![0xaa; 131],
                b"Test Using Larger Than Block-Size Key - Hash Key First".to_vec(),
                "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
            ),
            (
                vec![0xaa; 131],
                b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.".to_vec(),
                "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2",
            ),
        ];
        for (key, message, expected) in cases {
            assert_eq!(hmac_sha256(&key, &message), from_hex::<32>(expected));
        }
    }

    /// The precomputed path gives the same answer as the plain HMAC.
    #[test]
    fn the_fast_iteration_matches_the_plain_hmac() {
        let keyed = Keyed::new(b"passphrase");
        let message = [0x42u8; 32];
        assert_eq!(keyed.mac32(&message), hmac_sha256(b"passphrase", &message));
    }

    /// RFC 7914 section 11: PBKDF2-HMAC-SHA256 with one and 80,000 iterations.
    #[test]
    fn pbkdf2_matches_rfc_7914_section_11() {
        let mut out = [0u8; 64];
        pbkdf2_sha256(b"passwd", b"salt", 1, &mut out);
        assert_eq!(
            out.to_vec(),
            hex_bytes(
                "55ac046e56e3089fec1691c22544b605f94185216dde0465e68b9d57c20dacbc\
                 49ca9cccf179b645991664b39d77ef317c71b845b1e30bd509112041d3a19783"
            )
        );
        pbkdf2_sha256(b"Password", b"NaCl", 80_000, &mut out);
        assert_eq!(
            out.to_vec(),
            hex_bytes(
                "4ddcd8f60b98be21830cee5ef22701f9641a4418d04c0414aeff08876b34ab56\
                 a1d425a1225833549adb841b51c9b3176a272bdebba1d078478f62b397f33c8d"
            )
        );
    }
}

//! ChaCha20 (RFC 8439 section 2.4) and HChaCha20 (draft-irtf-cfrg-xchacha-03
//! section 2.2).
//!
//! Invariant: both functions match their published test vectors, which are
//! checked below. Every operation is an addition, a rotation or an exclusive
//! or on 32 bit words, with no table and no branch on a key or data byte, so
//! the time a call takes does not depend on the secret it is given.

/// The four words "expand 32-byte k" that start every ChaCha state.
const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

/// Applies one quarter round to four words of a state, by index.
///
/// A macro rather than a function so the indices stay constants: a
/// function taking four `usize` indices would have to index the array at run
/// time, which is exactly the slice access this crate denies.
macro_rules! quarter {
    ($s:ident, $a:literal, $b:literal, $c:literal, $d:literal) => {
        $s[$a] = $s[$a].wrapping_add($s[$b]);
        $s[$d] = ($s[$d] ^ $s[$a]).rotate_left(16);
        $s[$c] = $s[$c].wrapping_add($s[$d]);
        $s[$b] = ($s[$b] ^ $s[$c]).rotate_left(12);
        $s[$a] = $s[$a].wrapping_add($s[$b]);
        $s[$d] = ($s[$d] ^ $s[$a]).rotate_left(8);
        $s[$c] = $s[$c].wrapping_add($s[$d]);
        $s[$b] = ($s[$b] ^ $s[$c]).rotate_left(7);
    };
}

/// Runs the twenty ChaCha rounds over a state in place.
///
/// @param s - the sixteen word state
fn rounds(s: &mut [u32; 16]) {
    for _ in 0..10 {
        quarter!(s, 0, 4, 8, 12);
        quarter!(s, 1, 5, 9, 13);
        quarter!(s, 2, 6, 10, 14);
        quarter!(s, 3, 7, 11, 15);
        quarter!(s, 0, 5, 10, 15);
        quarter!(s, 1, 6, 11, 12);
        quarter!(s, 2, 7, 8, 13);
        quarter!(s, 3, 4, 9, 14);
    }
}

/// Reads `N` little endian words out of a byte array.
///
/// @param bytes - exactly four bytes per word
fn words<const B: usize, const N: usize>(bytes: &[u8; B]) -> [u32; N] {
    let mut out = [0u32; N];
    for (word, chunk) in out.iter_mut().zip(bytes.chunks_exact(4)) {
        let mut four = [0u8; 4];
        four.copy_from_slice(chunk);
        *word = u32::from_le_bytes(four);
    }
    out
}

/// Builds the initial state from a key, a block counter and a 96 bit nonce.
///
/// @param key - the 256 bit key
/// @param counter - the block counter
/// @param nonce - the 96 bit nonce
fn initial(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u32; 16] {
    let k: [u32; 8] = words(key);
    let n: [u32; 3] = words(nonce);
    [
        SIGMA[0], SIGMA[1], SIGMA[2], SIGMA[3], k[0], k[1], k[2], k[3], k[4], k[5], k[6], k[7],
        counter, n[0], n[1], n[2],
    ]
}

/// Returns one 64 byte block of ChaCha20 key stream.
///
/// @param key - the 256 bit key
/// @param counter - which block of the stream
/// @param nonce - the 96 bit nonce
pub fn block(key: &[u8; 32], counter: u32, nonce: &[u8; 12]) -> [u8; 64] {
    let start = initial(key, counter, nonce);
    let mut state = start;
    rounds(&mut state);
    let mut out = [0u8; 64];
    for ((chunk, word), first) in out.chunks_exact_mut(4).zip(state.iter()).zip(start.iter()) {
        chunk.copy_from_slice(&word.wrapping_add(*first).to_le_bytes());
    }
    out
}

/// Encrypts or decrypts `data` in place with the ChaCha20 stream.
///
/// The same call does both, because the stream is combined with the data by
/// exclusive or. The counter wraps after 2^32 blocks, which is 256 GiB; every
/// caller in this crate encrypts one database unit of at most 64 KiB.
///
/// @param key - the 256 bit key
/// @param counter - the block counter of the first 64 bytes
/// @param nonce - the 96 bit nonce
/// @param data - the bytes to transform
pub fn apply(key: &[u8; 32], counter: u32, nonce: &[u8; 12], data: &mut [u8]) {
    // **The state is built once and only the counter moves**, and the stream
    // is combined four bytes at a time. The first version built the state from
    // the key's bytes for every 64 byte block and combined a byte at a time.
    // With both changes and the Poly1305 one beside them, a cold full scan of
    // an encrypted 11 MiB table went from 130 ms to 97 ms in the release build,
    // against 72 ms in plaintext (docs/encryption.md).
    let mut start = initial(key, counter, nonce);
    let mut blocks = data.chunks_exact_mut(64);
    for chunk in &mut blocks {
        let mut state = start;
        rounds(&mut state);
        for ((four, word), first) in chunk
            .chunks_exact_mut(4)
            .zip(state.iter())
            .zip(start.iter())
        {
            let mut bytes = [0u8; 4];
            bytes.copy_from_slice(four);
            let mixed = u32::from_le_bytes(bytes) ^ word.wrapping_add(*first);
            four.copy_from_slice(&mixed.to_le_bytes());
        }
        start[12] = start[12].wrapping_add(1);
    }
    let rest = blocks.into_remainder();
    if !rest.is_empty() {
        let mut state = start;
        rounds(&mut state);
        let mut stream = [0u8; 64];
        for ((four, word), first) in stream
            .chunks_exact_mut(4)
            .zip(state.iter())
            .zip(start.iter())
        {
            four.copy_from_slice(&word.wrapping_add(*first).to_le_bytes());
        }
        for (byte, mask) in rest.iter_mut().zip(stream.iter()) {
            *byte ^= mask;
        }
    }
}

/// Derives a subkey from a key and the first 128 bits of an extended nonce.
///
/// This is what lets XChaCha20 take a 192 bit nonce: the first 16 bytes pick a
/// subkey, and the last 8 are the nonce ChaCha20 uses under it.
///
/// @param key - the 256 bit key
/// @param nonce - the first 16 bytes of the extended nonce
pub fn hchacha20(key: &[u8; 32], nonce: &[u8; 16]) -> [u8; 32] {
    let k: [u32; 8] = words(key);
    let n: [u32; 4] = words(nonce);
    let mut state = [
        SIGMA[0], SIGMA[1], SIGMA[2], SIGMA[3], k[0], k[1], k[2], k[3], k[4], k[5], k[6], k[7],
        n[0], n[1], n[2], n[3],
    ];
    rounds(&mut state);
    let picked = [
        state[0], state[1], state[2], state[3], state[12], state[13], state[14], state[15],
    ];
    let mut out = [0u8; 32];
    for (chunk, word) in out.chunks_exact_mut(4).zip(picked.iter()) {
        chunk.copy_from_slice(&word.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypt::from_hex;

    /// RFC 8439 section 2.3.2: the block function test vector.
    #[test]
    fn the_block_function_matches_rfc_8439_2_3_2() {
        let key: [u8; 32] =
            from_hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let nonce: [u8; 12] = from_hex("000000090000004a00000000");
        let expected: [u8; 64] = from_hex(
            "10f1e7e4d13b5915500fdd1fa32071c4c7d1f4c733c068030422aa9ac3d46c4e\
             d2826446079faa0914c2d705d98b02a2b5129cd1de164eb9cbd083e8a2503c4e",
        );
        assert_eq!(block(&key, 1, &nonce), expected);
    }

    /// RFC 8439 section 2.4.2: the "sunscreen" encryption.
    #[test]
    fn encryption_matches_rfc_8439_2_4_2() {
        let key: [u8; 32] =
            from_hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let nonce: [u8; 12] = from_hex("000000000000004a00000000");
        let mut text = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.".to_vec();
        apply(&key, 1, &nonce, &mut text);
        let expected: [u8; 114] = from_hex(
            "6e2e359a2568f98041ba0728dd0d6981e97e7aec1d4360c20a27afccfd9fae0b\
             f91b65c5524733ab8f593dabcd62b3571639d624e65152ab8f530c359f0861d8\
             07ca0dbf500d6a6156a38e088a22b65e52bc514d16ccf806818ce91ab7793736\
             5af90bbf74a35be6b40b8eedf2785e42874d",
        );
        assert_eq!(text, expected.to_vec());
    }

    /// RFC 8439 appendix A.1, test vector 1: the all zero key and nonce.
    #[test]
    fn the_all_zero_block_matches_rfc_8439_a_1() {
        let expected: [u8; 64] = from_hex(
            "76b8e0ada0f13d90405d6ae55386bd28bdd219b8a08ded1aa836efcc8b770dc7\
             da41597c5157488d7724e03fb8d84a376a43b8f41518a11cc387b669b2ee6586",
        );
        assert_eq!(block(&[0u8; 32], 0, &[0u8; 12]), expected);
    }

    /// draft-irtf-cfrg-xchacha-03 section 2.2.1: the HChaCha20 test vector.
    #[test]
    fn hchacha20_matches_the_xchacha_draft_2_2_1() {
        let key: [u8; 32] =
            from_hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f");
        let nonce: [u8; 16] = from_hex("000000090000004a0000000031415927");
        let expected: [u8; 32] =
            from_hex("82413b4227b27bfed30e42508a877d73a0f9e4d58a74a853c12ec41326d3ecdc");
        assert_eq!(hchacha20(&key, &nonce), expected);
    }

    /// Applying the stream twice gives the input back, across a block edge.
    #[test]
    fn the_stream_is_its_own_inverse() {
        let key = [7u8; 32];
        let nonce = [9u8; 12];
        let original: Vec<u8> = (0..200u32).map(|value| value as u8).collect();
        let mut data = original.clone();
        apply(&key, 5, &nonce, &mut data);
        assert_ne!(data, original);
        apply(&key, 5, &nonce, &mut data);
        assert_eq!(data, original);
    }
}

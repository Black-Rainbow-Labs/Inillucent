//! ChaCha20-Poly1305 (RFC 8439 section 2.8) and XChaCha20-Poly1305
//! (draft-irtf-cfrg-xchacha-03 section 2.3).
//!
//! Invariant: a message is decrypted only after its tag has been checked,
//! and the check is a comparison whose time does not depend on where the
//! tags differ. A caller who gets `false` from [`open`] has the ciphertext
//! still in its buffer, never a guess at the plaintext.

use super::chacha;
use super::poly1305::Poly1305;

/// Computes the RFC 8439 tag over associated data and ciphertext.
///
/// @param key - the ChaCha20 key the message is encrypted under
/// @param nonce - its 96 bit nonce
/// @param aad - the associated data
/// @param ciphertext - the encrypted bytes
fn compute_tag(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], ciphertext: &[u8]) -> [u8; 16] {
    let first = chacha::block(key, 0, nonce);
    let mut one_time = [0u8; 32];
    one_time.copy_from_slice(first.get(..32).unwrap_or(&[0u8; 32]));
    let mut mac = Poly1305::new(&one_time);
    mac.update(aad);
    mac.pad_to_block();
    mac.update(ciphertext);
    mac.pad_to_block();
    mac.update(&(aad.len() as u64).to_le_bytes());
    mac.update(&(ciphertext.len() as u64).to_le_bytes());
    mac.finish()
}

/// Reports whether two tags are equal, taking the same time wherever they
/// differ.
///
/// @param left - one tag
/// @param right - the other
pub fn tags_equal(left: &[u8; 16], right: &[u8; 16]) -> bool {
    let difference = left
        .iter()
        .zip(right.iter())
        .fold(0u8, |seen, (a, b)| seen | (a ^ b));
    std::hint::black_box(difference) == 0
}

/// Encrypts `data` in place with ChaCha20-Poly1305 and returns the tag.
///
/// @param key - the 256 bit key
/// @param nonce - a 96 bit nonce that is never used twice with this key
/// @param aad - bytes authenticated but not encrypted
/// @param data - the plaintext, replaced by the ciphertext
pub fn seal_ietf(key: &[u8; 32], nonce: &[u8; 12], aad: &[u8], data: &mut [u8]) -> [u8; 16] {
    chacha::apply(key, 1, nonce, data);
    compute_tag(key, nonce, aad, data)
}

/// Checks the tag and, only when it matches, decrypts `data` in place.
///
/// @param key - the 256 bit key
/// @param nonce - the nonce the message was sealed with
/// @param aad - the associated data it was sealed with
/// @param data - the ciphertext, replaced by the plaintext on success
/// @param tag - the tag that came with it
pub fn open_ietf(
    key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    data: &mut [u8],
    tag: &[u8; 16],
) -> bool {
    let expected = compute_tag(key, nonce, aad, data);
    if !tags_equal(&expected, tag) {
        return false;
    }
    chacha::apply(key, 1, nonce, data);
    true
}

/// Splits an extended nonce into the subkey and the 96 bit nonce ChaCha20
/// uses under it.
///
/// @param key - the 256 bit key
/// @param nonce - the 192 bit nonce
fn extend(key: &[u8; 32], nonce: &[u8; 24]) -> ([u8; 32], [u8; 12]) {
    let mut head = [0u8; 16];
    head.copy_from_slice(nonce.get(..16).unwrap_or(&[0u8; 16]));
    let subkey = chacha::hchacha20(key, &head);
    let mut short = [0u8; 12];
    if let (Some(target), Some(source)) = (short.get_mut(4..), nonce.get(16..)) {
        target.copy_from_slice(source);
    }
    (subkey, short)
}

/// Encrypts `data` in place with XChaCha20-Poly1305 and returns the tag.
///
/// The 192 bit nonce is large enough to pick at random for every message
/// under one key, which is what the encrypting file system does.
///
/// @param key - the 256 bit key
/// @param nonce - a 192 bit nonce that is never used twice with this key
/// @param aad - bytes authenticated but not encrypted
/// @param data - the plaintext, replaced by the ciphertext
pub fn seal(key: &[u8; 32], nonce: &[u8; 24], aad: &[u8], data: &mut [u8]) -> [u8; 16] {
    let (subkey, short) = extend(key, nonce);
    seal_ietf(&subkey, &short, aad, data)
}

/// Checks the tag and, only when it matches, decrypts `data` in place with
/// XChaCha20-Poly1305.
///
/// @param key - the 256 bit key
/// @param nonce - the nonce the message was sealed with
/// @param aad - the associated data it was sealed with
/// @param data - the ciphertext, replaced by the plaintext on success
/// @param tag - the tag that came with it
pub fn open(key: &[u8; 32], nonce: &[u8; 24], aad: &[u8], data: &mut [u8], tag: &[u8; 16]) -> bool {
    let (subkey, short) = extend(key, nonce);
    open_ietf(&subkey, &short, aad, data, tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypt::{from_hex, hex_bytes};

    /// The plaintext RFC 8439 and the XChaCha draft both use.
    const SUNSCREEN: &[u8] = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";

    /// RFC 8439 section 2.8.2.
    #[test]
    fn the_aead_matches_rfc_8439_2_8_2() {
        let key: [u8; 32] =
            from_hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
        let nonce: [u8; 12] = from_hex("070000004041424344454647");
        let aad = hex_bytes("50515253c0c1c2c3c4c5c6c7");
        let mut data = SUNSCREEN.to_vec();
        let tag = seal_ietf(&key, &nonce, &aad, &mut data);
        let expected = hex_bytes(
            "d31a8d34648e60db7b86afbc53ef7ec2a4aded51296e08fea9e2b5a736ee62d6\
             3dbea45e8ca9671282fafb69da92728b1a71de0a9e060b2905d6a5b67ecd3b36\
             92ddbd7f2d778b8c9803aee328091b58fab324e4fad675945585808b4831d7bc\
             3ff4def08e4b7a9de576d26586cec64b6116",
        );
        assert_eq!(data, expected);
        assert_eq!(tag, from_hex::<16>("1ae10b594f09e26a7e902ecbd0600691"));
        assert!(open_ietf(&key, &nonce, &aad, &mut data, &tag));
        assert_eq!(data, SUNSCREEN);
    }

    /// draft-irtf-cfrg-xchacha-03 appendix A.3.1.
    #[test]
    fn xchacha_matches_the_draft_a_3_1() {
        let key: [u8; 32] =
            from_hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f");
        let nonce: [u8; 24] = from_hex("404142434445464748494a4b4c4d4e4f5051525354555657");
        let aad = hex_bytes("50515253c0c1c2c3c4c5c6c7");
        let mut data = SUNSCREEN.to_vec();
        let tag = seal(&key, &nonce, &aad, &mut data);
        let expected = hex_bytes(
            "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb\
             731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452\
             2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9\
             21f9664c97637da9768812f615c68b13b52e",
        );
        assert_eq!(data, expected);
        assert_eq!(tag, from_hex::<16>("c0875924c1c7987947deafd8780acf49"));
        assert!(open(&key, &nonce, &aad, &mut data, &tag));
        assert_eq!(data, SUNSCREEN);
    }

    /// A changed byte anywhere - ciphertext, associated data or tag - is
    /// refused, and the buffer is left as ciphertext.
    #[test]
    fn any_changed_byte_is_refused() {
        let key = [1u8; 32];
        let nonce = [2u8; 24];
        let mut sealed = b"a row of the database".to_vec();
        let tag = seal(&key, &nonce, b"unit 7", &mut sealed);
        for index in 0..sealed.len() {
            let mut changed = sealed.clone();
            changed[index] ^= 1;
            let before = changed.clone();
            assert!(!open(&key, &nonce, b"unit 7", &mut changed, &tag));
            assert_eq!(changed, before, "a refused open leaves the ciphertext");
        }
        let mut copy = sealed.clone();
        assert!(!open(&key, &nonce, b"unit 8", &mut copy, &tag));
        let mut bad_tag = tag;
        bad_tag[15] ^= 0x80;
        let mut copy = sealed.clone();
        assert!(!open(&key, &nonce, b"unit 7", &mut copy, &bad_tag));
        assert!(!open(
            &[9u8; 32],
            &nonce,
            b"unit 7",
            &mut sealed.clone(),
            &tag
        ));
        assert!(open(&key, &nonce, b"unit 7", &mut sealed, &tag));
        assert_eq!(sealed, b"a row of the database");
    }
}

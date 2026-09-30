//! The cryptography encryption at rest is built from, written here.
//!
//! Invariant: every primitive in this module matches its published test
//! vectors, which each submodule checks, and none of them branches on or
//! indexes by a secret byte.
//!
//! They are first party for the reason SHA-256 and CRC-32 already are: an
//! encrypted database is a file format, and a file format must not change when
//! a dependency is upgraded. `docs/dependency-policy.md` lists them.
//!
//! | Module | What it is |
//! |---|---|
//! | [`chacha`] | ChaCha20 and HChaCha20 |
//! | [`poly1305`] | the Poly1305 one time authenticator |
//! | [`aead`] | ChaCha20-Poly1305 and XChaCha20-Poly1305 |
//! | [`kdf`] | HMAC-SHA256 and PBKDF2-HMAC-SHA256 |

pub mod aead;
pub mod chacha;
pub mod kdf;
pub mod poly1305;

/// A 256 bit key that is overwritten with zeros when it is dropped.
///
/// **Best effort, and the doc says so.** This crate forbids `unsafe`, so the
/// zeroing is a plain store followed by `black_box`, which stops the compiler
/// removing the store as dead. It cannot reach a copy the compiler made in a
/// register or on the stack, and nothing here can stop the operating system
/// paging the key out.
#[derive(Clone)]
pub struct SecretKey([u8; 32]);

impl SecretKey {
    /// Wraps 32 key bytes.
    ///
    /// @param bytes - the key
    pub fn new(bytes: [u8; 32]) -> SecretKey {
        SecretKey(bytes)
    }

    /// Returns the key bytes, for the primitive that uses them.
    pub fn bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl Drop for SecretKey {
    /// Overwrites the key with zeros.
    fn drop(&mut self) {
        self.0 = [0u8; 32];
        std::hint::black_box(&self.0);
    }
}

impl std::fmt::Debug for SecretKey {
    /// Prints that there is a key and nothing about it.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str("SecretKey(..)")
    }
}

/// Decodes hexadecimal text into bytes, reading nothing past the first
/// character that is not a hex digit pair. Whitespace is skipped.
///
/// @param text - the hexadecimal text
pub fn hex_bytes(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    digits
        .chunks_exact(2)
        .map_while(|pair| {
            let high = (*pair.first()? as char).to_digit(16)?;
            let low = (*pair.get(1)? as char).to_digit(16)?;
            u8::try_from(high.checked_mul(16)?.checked_add(low)?).ok()
        })
        .collect()
}

/// Decodes hexadecimal text into a fixed size array, leaving zeros where the
/// text is short. For test vectors.
///
/// @param text - the hexadecimal text
pub fn from_hex<const N: usize>(text: &str) -> [u8; N] {
    let mut out = [0u8; N];
    for (slot, byte) in out.iter_mut().zip(hex_bytes(text)) {
        *slot = byte;
    }
    out
}

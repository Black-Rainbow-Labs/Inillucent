//! Encryption at rest: a `Vfs` that encrypts every file it opens.
//!
//! Invariant: every byte this file system writes below its caller is either
//! a header, which holds no row, or ciphertext sealed with
//! XChaCha20-Poly1305 under the database's data key. The engine above sees
//! plaintext at the offsets it wrote, so no crate above `inillucent-vfs`
//! knows whether its database is encrypted.
//!
//! It wraps another `Vfs`, usually `OsVfs`, and passes locks, renames and
//! deletes straight through. `tasks/task-2142-encryption-at-rest-tdd.md` is the
//! design; the parts a reader of this module needs are:
//!
//! - **One data key per database**, random, stored in each file's header
//!   wrapped under a key encryption key made from the caller's key. A wrong
//!   key fails one tag check on the header, before any page is read.
//! - **Units.** The database file encrypts each page as one unit, with its
//!   trailer in a sector of its own. Logs and journals keep two copies of each
//!   4,032 byte unit and rewrite them in turn, so a torn rewrite never
//!   destroys what an earlier sync made durable. Temporary files use a key
//!   made for this file system and never stored.
//! - **A unit that fails its tag reads as random bytes**, which fail every
//!   page and log checksum the engine keeps, so recovery treats it as the torn
//!   write it usually is. [`CryptVfs::failures`] counts them.

pub mod file;
pub mod header;
pub mod open;
pub mod unit;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use inillucent_base::crypt::{kdf, SecretKey};

use crate::contract::Vfs;
use crate::error::VfsResult;
use crate::os::system_randomness;

use header::{KDF_PBKDF2, KDF_RAW};
pub use open::{probe, Probe};

/// The PBKDF2 iteration count a passphrase gets, which is OWASP's 2023
/// figure for PBKDF2-HMAC-SHA256.
pub const DEFAULT_ITERATIONS: u32 = 600_000;

/// The page size a new database file is created with when nobody says.
pub const DEFAULT_PAGE: u32 = 32_768;

/// What the caller gave as the key.
#[derive(Clone)]
enum Material {
    /// A passphrase, and the PBKDF2 iteration count new files get.
    Passphrase(Vec<u8>, u32),
    /// A 32 byte key, used as the key encryption key directly.
    Raw(SecretKey),
}

/// The key a database is encrypted with.
///
/// Either a passphrase, which PBKDF2 turns into a key, or 32 raw bytes. The
/// text form `x'` followed by 64 hex digits and `'` is a raw key, which is
/// SQLCipher's spelling; anything else is a passphrase.
#[derive(Clone)]
pub struct EncryptionKey {
    /// The key itself.
    material: Material,
}

impl std::fmt::Debug for EncryptionKey {
    /// Says what kind of key it is and nothing else.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.material {
            Material::Passphrase(_, iterations) => {
                write!(out, "EncryptionKey(passphrase, {iterations} iterations)")
            }
            Material::Raw(_) => out.write_str("EncryptionKey(raw)"),
        }
    }
}

impl Drop for Material {
    /// Overwrites a passphrase with zeros. A raw key zeroes itself.
    fn drop(&mut self) {
        if let Material::Passphrase(text, _) = self {
            text.fill(0);
            std::hint::black_box(&text);
        }
    }
}

impl EncryptionKey {
    /// A passphrase, stretched with PBKDF2 at [`DEFAULT_ITERATIONS`].
    ///
    /// @param text - the passphrase
    pub fn passphrase(text: &str) -> EncryptionKey {
        EncryptionKey::passphrase_with_iterations(text, DEFAULT_ITERATIONS)
    }

    /// A passphrase with a stated PBKDF2 iteration count for new files.
    ///
    /// An existing file keeps the count its header records. Tests use a small
    /// count; an application should not.
    ///
    /// @param text - the passphrase
    /// @param iterations - the work factor, at least one
    pub fn passphrase_with_iterations(text: &str, iterations: u32) -> EncryptionKey {
        EncryptionKey {
            material: Material::Passphrase(text.as_bytes().to_vec(), iterations.max(1)),
        }
    }

    /// A raw 32 byte key.
    ///
    /// @param bytes - the key
    pub fn raw(bytes: [u8; 32]) -> EncryptionKey {
        EncryptionKey {
            material: Material::Raw(SecretKey::new(bytes)),
        }
    }

    /// Reads a key from its text form: `x'<64 hex digits>'` is a raw key and
    /// anything else is a passphrase.
    ///
    /// @param text - the key as a caller typed it
    pub fn parse(text: &str) -> EncryptionKey {
        let trimmed = text.trim();
        let inner = trimmed
            .strip_prefix("x'")
            .or_else(|| trimmed.strip_prefix("X'"))
            .and_then(|rest| rest.strip_suffix('\''));
        if let Some(hex) = inner {
            let bytes = inillucent_base::crypt::hex_bytes(hex);
            if hex.len() == 64 && bytes.len() == 32 {
                return EncryptionKey::raw(inillucent_base::crypt::from_hex(hex));
            }
        }
        EncryptionKey::passphrase(text)
    }

    /// Reports whether this key is empty, which means "no encryption" to
    /// `ATTACH ... KEY ''` and `PRAGMA rekey = ''`.
    pub fn is_empty(&self) -> bool {
        matches!(&self.material, Material::Passphrase(text, _) if text.is_empty())
    }

    /// Makes the key encryption key for a header's derivation, or `None` when
    /// this key cannot be the one the header was made with.
    ///
    /// @param kdf - the header's key derivation code
    /// @param salt - its salt
    /// @param iterations - its iteration count
    fn key_encryption_key(&self, kdf: u32, salt: &[u8; 16], iterations: u32) -> Option<SecretKey> {
        match (&self.material, kdf) {
            (Material::Raw(key), KDF_RAW) => Some(key.clone()),
            (Material::Passphrase(text, _), KDF_PBKDF2) => {
                let mut out = [0u8; 32];
                kdf::pbkdf2_sha256(text, salt, iterations, &mut out);
                let key = SecretKey::new(out);
                out.fill(0);
                Some(key)
            }
            _ => None,
        }
    }

    /// The derivation code and iteration count a new file gets.
    fn derivation(&self) -> (u32, u32) {
        match &self.material {
            Material::Passphrase(_, iterations) => (KDF_PBKDF2, *iterations),
            Material::Raw(_) => (KDF_RAW, 0),
        }
    }
}

/// State every file of one `CryptVfs` shares: nonces, randomness, a count
/// of failed units, and the key temporary files use.
pub struct Shared {
    /// Random bytes that start every nonce this file system makes.
    prefix: [u8; 16],
    /// How many nonces this file system has made.
    counter: AtomicU64,
    /// How many unit reads failed their tag.
    failures: AtomicU64,
    /// The key for temporary files, made here and never stored.
    ephemeral: SecretKey,
}

impl Shared {
    /// Makes the shared state, taking its randomness from the operating system.
    fn new() -> VfsResult<Shared> {
        let mut prefix = [0u8; 16];
        system_randomness(&mut prefix)?;
        let mut key = [0u8; 32];
        system_randomness(&mut key)?;
        Ok(Shared {
            prefix,
            counter: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            ephemeral: SecretKey::new(key),
        })
    }

    /// Returns a nonce this file system has never returned before: the random
    /// prefix and a counter.
    ///
    /// **Not a system call per write.** 128 random bits make two file systems'
    /// prefixes collide with a probability of about 2^-64 after 2^32 of them,
    /// and the counter makes every nonce of one file system distinct.
    pub fn nonce(&self) -> [u8; 24] {
        let count = self.counter.fetch_add(1, Ordering::Relaxed);
        let mut nonce = [0u8; 24];
        if let Some(target) = nonce.get_mut(..16) {
            target.copy_from_slice(&self.prefix);
        }
        if let Some(target) = nonce.get_mut(16..) {
            target.copy_from_slice(&count.to_le_bytes());
        }
        nonce
    }

    /// Returns random bytes to stand in for a unit that failed its tag.
    ///
    /// @param length - how many
    pub fn filler(&self, length: usize) -> Vec<u8> {
        let mut bytes = vec![0u8; length];
        if system_randomness(&mut bytes).is_err() {
            // Without the operating system's generator, a stream under the
            // temporary key is still unpredictable to whoever changed the file.
            let nonce = self.nonce();
            let _ =
                inillucent_base::crypt::aead::seal(self.ephemeral.bytes(), &nonce, &[], &mut bytes);
        }
        bytes
    }

    /// Counts one unit that failed its tag.
    fn count_failure(&self) {
        self.failures.fetch_add(1, Ordering::Relaxed);
    }
}

/// The key material of one database, once its header has been read or made.
#[derive(Clone)]
struct Context {
    /// The key derivation code.
    kdf: u32,
    /// The iteration count.
    iterations: u32,
    /// The salt.
    salt: [u8; 16],
    /// The database id.
    database_id: [u8; 16],
    /// The key encryption key made from the caller's key and the salt.
    kek: SecretKey,
    /// The data key.
    data_key: SecretKey,
}

/// What a `CryptVfs` knows about keys, behind one lock.
struct Keys {
    /// The caller's key, which `rekey` replaces.
    key: EncryptionKey,
    /// The context new files are made with.
    context: Option<Context>,
    /// Data keys already unwrapped, by database id.
    known: Vec<([u8; 16], SecretKey)>,
}

/// A file system that encrypts every file it opens.
pub struct CryptVfs {
    /// The file system underneath, holding ciphertext.
    inner: Arc<dyn Vfs>,
    /// State shared with every open file.
    shared: Arc<Shared>,
    /// The keys, behind a lock because `rekey` replaces them.
    keys: Mutex<Keys>,
    /// The page size a new database file is created with.
    page: u32,
}

impl std::fmt::Debug for CryptVfs {
    /// Names the file system underneath and nothing secret.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("CryptVfs")
            .field("inner", &self.inner.name())
            .field("page", &self.page)
            .finish()
    }
}

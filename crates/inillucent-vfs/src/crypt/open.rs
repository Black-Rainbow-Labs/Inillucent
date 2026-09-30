//! Opening encrypted files: reading or writing their header, finding the
//! data key, and changing the key with `rekey`.
//!
//! Invariant: a file is opened as encrypted only when its header's wrapped
//! data key opens under the caller's key, or when the file is new and this
//! module has just written that header. A file with bytes and no header is
//! refused, never read as plaintext.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::SystemTime;

use inillucent_base::crypt::SecretKey;

use super::file::{not_a_database, CryptFile};
use super::header::{self, Header, HEADER_AREA};
use super::unit::Layout;
use super::{Context, CryptVfs, EncryptionKey, Keys, Shared, DEFAULT_PAGE};
use crate::contract::{AccessMode, FileKind, OpenOptions, SyncMode, Vfs, VfsFile};
use crate::error::{self, VfsResult};
use crate::os::system_randomness;
use crate::path::DbPath;

/// Returns 16 random bytes from the operating system.
fn random16() -> VfsResult<[u8; 16]> {
    let mut out = [0u8; 16];
    system_randomness(&mut out)?;
    Ok(out)
}

/// What the first bytes of a file say about it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Probe {
    /// There is no file, or it is empty.
    Empty,
    /// It starts with the encrypted file magic.
    Encrypted,
    /// It has bytes and they are not an encrypted header.
    Plain,
}

/// Looks at the start of a file without a key, to tell an encrypted file
/// from a plaintext one before opening it.
///
/// @param vfs - the file system to look through, unencrypted
/// @param path - the file
pub fn probe(vfs: &dyn Vfs, path: &DbPath) -> VfsResult<Probe> {
    if !vfs.access(path, AccessMode::Exists)? {
        return Ok(Probe::Empty);
    }
    let file = vfs.open(path, OpenOptions::of_kind(FileKind::MainDb).read_only())?;
    if file.file_size()? == 0 {
        return Ok(Probe::Empty);
    }
    let mut head = [0u8; 8];
    let _ = file.read_exact_at(0, &mut head);
    match head == header::MAGIC {
        true => Ok(Probe::Encrypted),
        false => Ok(Probe::Plain),
    }
}

impl CryptVfs {
    /// Wraps a file system, encrypting everything opened through it with
    /// `key`. New database files get [`DEFAULT_PAGE`] byte units.
    ///
    /// @param inner - the file system that holds the ciphertext
    /// @param key - the caller's key
    pub fn new(inner: Arc<dyn Vfs>, key: EncryptionKey) -> VfsResult<CryptVfs> {
        Ok(CryptVfs {
            inner,
            shared: Arc::new(Shared::new()?),
            keys: std::sync::Mutex::new(Keys {
                key,
                context: None,
                known: Vec::new(),
            }),
            page: DEFAULT_PAGE,
        })
    }

    /// Sets the page size a new database file is created with, which is its
    /// unit size. An existing file keeps the size its header records.
    ///
    /// @param page - the page size in bytes
    pub fn with_page_size(mut self, page: u32) -> CryptVfs {
        self.page = page;
        self
    }

    /// Returns the file system underneath.
    pub fn inner(&self) -> &Arc<dyn Vfs> {
        &self.inner
    }

    /// Returns a copy of the caller's key, for opening another database the
    /// same way: a backup, or an `ATTACH` with no `KEY`.
    pub fn key(&self) -> VfsResult<EncryptionKey> {
        Ok(self.keys()?.key.clone())
    }

    /// How many unit reads have failed their tag since this file system was
    /// made: torn writes, or bytes changed by somebody without the key.
    pub fn failures(&self) -> u64 {
        self.shared.failures.load(Ordering::Relaxed)
    }

    /// Takes the key lock.
    fn keys(&self) -> VfsResult<std::sync::MutexGuard<'_, Keys>> {
        self.keys
            .lock()
            .map_err(|_| error::misuse("the encryption key lock was poisoned"))
    }

    /// Returns the context new files are made with, making one when this file
    /// system has none yet.
    ///
    /// @param keys - the held key state
    fn context(keys: &mut Keys) -> VfsResult<Context> {
        if let Some(context) = &keys.context {
            return Ok(context.clone());
        }
        let (kdf, iterations) = keys.key.derivation();
        let salt = random16()?;
        let kek = keys
            .key
            .key_encryption_key(kdf, &salt, iterations)
            .ok_or_else(|| error::misuse("the key cannot derive a key encryption key"))?;
        let mut data = [0u8; 32];
        system_randomness(&mut data)?;
        let context = Context {
            kdf,
            iterations,
            salt,
            database_id: random16()?,
            kek,
            data_key: SecretKey::new(data),
        };
        data.fill(0);
        keys.known
            .push((context.database_id, context.data_key.clone()));
        keys.context = Some(context.clone());
        Ok(context)
    }

    /// Builds and writes the header of a new file.
    ///
    /// @param file - the new, empty file
    /// @param layout - how its units will be placed
    fn create_header(
        &self,
        file: &dyn VfsFile,
        layout: Layout,
    ) -> VfsResult<(SecretKey, [u8; 16])> {
        let context = CryptVfs::context(&mut *self.keys()?)?;
        let mut head = Header {
            kdf: context.kdf,
            iterations: context.iterations,
            layout: layout.code(),
            unit: layout.unit(),
            salt: context.salt,
            database_id: context.database_id,
            file_id: random16()?,
            generation: 1,
            wrap_nonce: [0; 24],
            wrapped: [0; 32],
            wrap_tag: [0; 16],
        };
        head.wrap(
            context.kek.bytes(),
            context.data_key.bytes(),
            self.shared.nonce(),
        );
        let mut area = head.encode();
        area.extend(head.encode());
        file.truncate(0)?;
        file.write_all_at(0, &area)?;
        Ok((context.data_key.clone(), head.file_id))
    }

    /// Finds the data key a header wraps: from a database already unlocked
    /// in this file system, or by unwrapping it with the caller's key.
    ///
    /// @param head - the file's header
    fn unlock(&self, head: &Header) -> VfsResult<SecretKey> {
        let mut keys = self.keys()?;
        if let Some((_, key)) = keys.known.iter().find(|(id, _)| *id == head.database_id) {
            return Ok(key.clone());
        }
        let kek = keys
            .key
            .key_encryption_key(head.kdf, &head.salt, head.iterations)
            .ok_or_else(|| {
                not_a_database("the key is not the kind this file was encrypted with")
            })?;
        let mut data = head
            .unwrap_key(kek.bytes())
            .ok_or_else(|| not_a_database("the key is wrong, or the file header was changed"))?;
        let data_key = SecretKey::new(data);
        data.fill(0);
        keys.known.push((head.database_id, data_key.clone()));
        if keys.context.is_none() {
            keys.context = Some(Context {
                kdf: head.kdf,
                iterations: head.iterations,
                salt: head.salt,
                database_id: head.database_id,
                kek,
                data_key: data_key.clone(),
            });
        }
        Ok(data_key)
    }

    /// The layout a new file of this kind gets.
    ///
    /// @param kind - what the file is for
    fn layout_for(&self, kind: FileKind) -> Layout {
        match kind {
            FileKind::MainDb => Layout::database(self.page),
            _ => Layout::Double,
        }
    }

    /// Opens a file that has, or will get, a header.
    ///
    /// @param path - the file
    /// @param options - how to open it
    fn open_headed(&self, path: &DbPath, options: OpenOptions) -> VfsResult<CryptFile> {
        let file = self.inner.open(path, options)?;
        let length = file.file_size()?;
        let mut area = vec![0u8; HEADER_AREA as usize];
        if length > 0 {
            let _ = file.read_exact_at(0, &mut area);
        }
        let shared = Arc::clone(&self.shared);
        if let Some((head, _)) = Header::newest(&area) {
            let layout = Layout::from_header(head.layout, head.unit).ok_or_else(|| {
                not_a_database("the header names a unit layout this build does not know")
            })?;
            let key = self.unlock(&head)?;
            return Ok(CryptFile::new(
                file,
                key,
                head.file_id,
                layout,
                HEADER_AREA,
                shared,
                options.read_only,
            ));
        }
        let torn_creation =
            length <= HEADER_AREA && header::starts_like_a_header(area.get(..8).unwrap_or(&[]));
        // **A log or journal whose header cannot be read holds nothing anybody
        // can read**, because the header is the only place its file id is kept.
        // The only way to get one is a crash before the file's first sync,
        // which is before anything in it was acknowledged, so it is opened as
        // the empty file it effectively is: a log scan ends its chain there,
        // and a journal with no header is not hot. That is what the plaintext
        // engine does with a segment or journal whose own header is garbage.
        // The database file itself gets no such reading: its header being
        // unreadable means the file is not an encrypted database.
        let damaged = length > 0 && !torn_creation;
        if damaged && options.kind == FileKind::MainDb {
            return Err(not_a_database(
                "the file is not encrypted, or its header is damaged",
            ));
        }
        let layout = self.layout_for(options.kind);
        if options.read_only {
            // Nothing to read and nothing may be written: an empty file.
            let nothing = SecretKey::new([0; 32]);
            return Ok(CryptFile::new(
                file,
                nothing,
                [0; 16],
                layout,
                super::file::NOTHING,
                shared,
                true,
            ));
        }
        let (key, file_id) = self.create_header(file.as_ref(), layout)?;
        Ok(CryptFile::new(
            file,
            key,
            file_id,
            layout,
            HEADER_AREA,
            shared,
            false,
        ))
    }

    /// Changes the key the named files are encrypted with.
    ///
    /// Each file's data key stays the same and only its header changes, so
    /// this rewrites one header per file. The older header slot is written
    /// and synced first, then the other, so a crash leaves one whole slot
    /// under one of the two keys. The caller names every file that holds the
    /// database: the database file and each log segment and journal beside it.
    ///
    /// @param paths - every file of the database
    /// @param new_key - the key they will be encrypted with
    pub fn rekey(&self, paths: &[DbPath], new_key: EncryptionKey) -> VfsResult<()> {
        let (kdf, iterations) = new_key.derivation();
        let salt = random16()?;
        let kek = new_key
            .key_encryption_key(kdf, &salt, iterations)
            .ok_or_else(|| error::misuse("the new key cannot derive a key encryption key"))?;
        for path in paths {
            self.rekey_file(path, kdf, iterations, &salt, &kek)?;
        }
        let mut keys = self.keys()?;
        keys.key = new_key;
        if let Some(context) = keys.context.as_mut() {
            context.kdf = kdf;
            context.iterations = iterations;
            context.salt = salt;
            context.kek = kek;
        }
        Ok(())
    }

    /// Rewrites one file's header under a new key encryption key.
    ///
    /// @param path - the file
    /// @param kdf - the new derivation code
    /// @param iterations - the new iteration count
    /// @param salt - the new salt
    /// @param kek - the new key encryption key
    fn rekey_file(
        &self,
        path: &DbPath,
        kdf: u32,
        iterations: u32,
        salt: &[u8; 16],
        kek: &SecretKey,
    ) -> VfsResult<()> {
        let mut options = OpenOptions::of_kind(FileKind::MainJournal);
        options.create = false;
        let file = self.inner.open(path, options)?;
        let mut area = vec![0u8; HEADER_AREA as usize];
        let _ = file.read_exact_at(0, &mut area);
        let Some((head, slot)) = Header::newest(&area) else {
            return Err(not_a_database(
                "a file of the database has no readable header",
            ));
        };
        let data_key = self.unlock(&head)?;
        let mut next = head.clone();
        next.kdf = kdf;
        next.iterations = iterations;
        next.salt = *salt;
        for (step, target) in [1usize.saturating_sub(slot), slot].into_iter().enumerate() {
            next.generation = head
                .generation
                .saturating_add(step as u64)
                .saturating_add(1);
            next.wrap(kek.bytes(), data_key.bytes(), self.shared.nonce());
            let offset = target.saturating_mul(header::SLOT_BYTES) as u64;
            file.write_all_at(offset, &next.encode())?;
            file.sync(SyncMode::Full)?;
        }
        Ok(())
    }
}

impl Vfs for CryptVfs {
    /// The name this file system is selected by.
    fn name(&self) -> &str {
        "crypt"
    }

    /// Opens a file, reading or writing its header, and returns a handle
    /// that encrypts and decrypts its units.
    fn open(&self, path: &DbPath, options: OpenOptions) -> VfsResult<Box<dyn VfsFile>> {
        match options.kind {
            FileKind::Transient | FileKind::TempDb => {
                let file = self.inner.open(path, options)?;
                let key = self.shared.ephemeral.clone();
                let shared = Arc::clone(&self.shared);
                let layout = Layout::temporary();
                Ok(Box::new(CryptFile::new(
                    file,
                    key,
                    random16()?,
                    layout,
                    0,
                    shared,
                    options.read_only,
                )))
            }
            _ => Ok(Box::new(self.open_headed(path, options)?)),
        }
    }

    /// Passes the delete through.
    fn delete(&self, path: &DbPath, sync_dir: bool) -> VfsResult<()> {
        self.inner.delete(path, sync_dir)
    }

    /// Passes the rename through; the header moves with the file.
    fn rename(&self, from: &DbPath, to: &DbPath) -> VfsResult<()> {
        self.inner.rename(from, to)
    }

    /// Passes the access check through.
    fn access(&self, path: &DbPath, mode: AccessMode) -> VfsResult<bool> {
        self.inner.access(path, mode)
    }

    /// Passes path resolution through.
    fn full_pathname(&self, path: &DbPath) -> VfsResult<DbPath> {
        self.inner.full_pathname(path)
    }

    /// Passes randomness through.
    fn randomness(&self, output: &mut [u8]) -> VfsResult<()> {
        self.inner.randomness(output)
    }

    /// Passes the clock through.
    fn current_time(&self) -> VfsResult<SystemTime> {
        self.inner.current_time()
    }

    /// Passes temporary paths through.
    fn temp_path(&self, prefix: &str) -> VfsResult<DbPath> {
        self.inner.temp_path(prefix)
    }

    /// Passes sleeping through.
    fn sleep(&self, micros: u64) -> VfsResult<()> {
        self.inner.sleep(micros)
    }

    /// Answers that this file system encrypts.
    fn encryption(&self) -> Option<&CryptVfs> {
        Some(self)
    }
}

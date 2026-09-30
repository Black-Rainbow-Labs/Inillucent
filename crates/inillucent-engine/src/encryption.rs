//! Encryption at rest, as the engine sees it: which file system a keyed
//! database is opened on, the refusals a key mismatch gets, and the three
//! pragmas `encryption`, `rekey` and `key`.
//!
//! Invariant: the engine never encrypts anything itself. It chooses the file
//! system - `CryptVfs` over `OsVfs` when there is a key, `OsVfs` when there is
//! not - and every file the database, its log and its journals open goes
//! through that choice. `inillucent_vfs::crypt` holds the cryptography.
//!
//! The one thing the engine adds is a look at the file before it is opened.
//! A plaintext file opened with a key and an encrypted file opened without
//! one both fail deep in the open with "file is not a database", which is
//! true and tells nobody what to do. Looking first turns them into a
//! sentence that names the fix.

use std::path::Path;
use std::sync::Arc;

use inillucent_base::error::{refusal, statement_refusal};
use inillucent_base::{DbError, DbResult, PrimaryCode};
use inillucent_sql::declare::argument_text;
use inillucent_sql::directive::PragmaArgument;
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::crypt::{probe, Probe};
use inillucent_vfs::{CryptVfs, DbPath, OsVfs, Vfs};

pub use inillucent_vfs::EncryptionKey;

use crate::{ImportedDatabase, Outcome};

/// What `PRAGMA encryption` answers for an encrypted database.
pub const CIPHER_NAME: &str = "xchacha20-poly1305";

/// Builds the error a file that cannot be read as this database gets.
///
/// `SQLITE_NOTADB`, which a driver reports as `corrupt`, with a sentence
/// that is safe to show: it names no path and no key.
///
/// @param said - the sentence
pub(crate) fn not_a_database(said: &str) -> DbError {
    DbError::primary(PrimaryCode::NotADb)
        .with_message(said.to_string())
        .with_detail(said.to_string())
}

/// Refuses a file whose encryption does not match whether a key was given.
///
/// A file that cannot be looked at is left for the open itself to report.
///
/// @param path - the database file
/// @param keyed - whether the caller gave a key
pub(crate) fn check_the_file_matches_the_key(path: &Path, keyed: bool) -> DbResult<()> {
    let looked = probe(&OsVfs::new(), &DbPath::new(path));
    match (looked, keyed) {
        (Ok(Probe::Encrypted), false) => Err(not_a_database(
            "this database is encrypted. Open it with its key: --key-file, the INILLUCENT_KEY \
             environment variable, or a driver's key option",
        )),
        (Ok(Probe::Plain), true) => Err(not_a_database(
            "this database is not encrypted, so it cannot be opened with a key. `inillucent \
             encrypt` writes an encrypted copy of it",
        )),
        _ => Ok(()),
    }
}

/// Returns the file system a database is opened on: encrypting when there is
/// a key, the operating system's when there is not.
///
/// @param key - the caller's key, if any
/// @param page_size - the page size a new database file is created with
pub(crate) fn file_system(key: Option<EncryptionKey>, page_size: usize) -> DbResult<Arc<dyn Vfs>> {
    match key {
        None => Ok(Arc::new(OsVfs::new())),
        Some(key) => {
            let page = u32::try_from(page_size).unwrap_or(inillucent_vfs::crypt::DEFAULT_PAGE);
            let crypt = CryptVfs::new(Arc::new(OsVfs::new()), key)
                .map_err(|error| error.into_db_error())?
                .with_page_size(page);
            Ok(Arc::new(crypt))
        }
    }
}

/// Rewords the error a keyed open fails with when the key is wrong.
///
/// The encrypting file system answers `SQLITE_NOTADB` for a header its key
/// cannot open. After [`check_the_file_matches_the_key`] has passed, that
/// means the key is wrong or the header was changed, and the sentence says so.
///
/// @param error - what the open returned
/// @param keyed - whether the caller gave a key
pub(crate) fn explain_a_wrong_key(error: DbError, keyed: bool) -> DbError {
    match keyed && error.code() == PrimaryCode::NotADb {
        true => not_a_database("file is not a database, or the key is wrong"),
        false => error,
    }
}

/// Returns every file of a database that exists: the database file, its
/// rollback journal, and each log segment.
///
/// Segments are numbered from one without gaps, and the walk stops after a
/// run of missing numbers, which is how `rebuild::remove_log_segments` finds
/// them too.
///
/// @param vfs - the file system the database lives on
/// @param database - the database file
fn files_of(vfs: &dyn Vfs, database: &Path) -> Vec<DbPath> {
    let main = DbPath::new(database);
    let mut paths = vec![main.clone()];
    let journal = main.journal();
    if vfs
        .access(&journal, inillucent_vfs::AccessMode::Exists)
        .unwrap_or(false)
    {
        paths.push(journal);
    }
    let Some(base) = database.file_name().and_then(|name| name.to_str()) else {
        return paths;
    };
    let mut missed = 0u64;
    let mut sequence = 0u64;
    while missed < 16 {
        let path = inillucent_wal::writer::segment_path(base, database.parent(), sequence);
        sequence = sequence.saturating_add(1);
        match vfs.access(&path, inillucent_vfs::AccessMode::Exists) {
            Ok(true) => {
                missed = 0;
                paths.push(path);
            }
            _ => missed = missed.saturating_add(1),
        }
    }
    paths
}

/// Returns a one row, one column text answer.
///
/// @param name - the column
/// @param word - the value
fn word_row(name: &str, word: &str) -> Outcome {
    Outcome {
        rows: vec![vec![OwnedDatum::Text(word.as_bytes().to_vec())]],
        names: std::rc::Rc::new(vec![name.into()]),
        changes: Default::default(),
    }
}

impl ImportedDatabase {
    /// Reports whether this connection's database is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.storage.vfs.encryption().is_some()
    }

    /// Returns the key this connection's database was opened with, if any.
    pub fn encryption_key(&self) -> DbResult<Option<EncryptionKey>> {
        match self.storage.vfs.encryption() {
            None => Ok(None),
            Some(crypt) => crypt.key().map(Some).map_err(|error| error.into_db_error()),
        }
    }

    /// `PRAGMA encryption`: which cipher the database is encrypted with, or
    /// `none`. It reports and cannot be set.
    ///
    /// @param argument - the value, which is refused
    pub(crate) fn pragma_encryption(&self, argument: Option<&PragmaArgument>) -> DbResult<Outcome> {
        if argument.is_some() {
            return Err(refusal(
                "PRAGMA encryption reports and cannot be set. `inillucent encrypt` and \
                 `inillucent decrypt` write a copy with or without encryption",
            ));
        }
        let word = match self.is_encrypted() {
            true => CIPHER_NAME,
            false => "none",
        };
        Ok(word_row("encryption", word))
    }

    /// `PRAGMA key`, which SQLCipher reads as the first statement of a
    /// connection. Here the key is given when the database is opened, because
    /// the open reads the file, so the pragma refuses and says where to put it.
    pub(crate) fn pragma_key(&self) -> DbResult<Outcome> {
        Err(refusal(
            "the key is given when the database is opened, not with PRAGMA key: pass \
             --key-file, set INILLUCENT_KEY, or use a driver's key option",
        ))
    }

    /// `PRAGMA rekey = 'new key'`: changes the key of an encrypted database.
    ///
    /// @param argument - the new key
    /// @param at - the attached database it was qualified with
    pub(crate) fn pragma_rekey(
        &mut self,
        argument: Option<&PragmaArgument>,
        at: Option<usize>,
    ) -> DbResult<Outcome> {
        if at.is_some_and(|index| index != crate::MAIN) {
            return Err(refusal("PRAGMA rekey changes the key of main only"));
        }
        let Some(argument) = argument else {
            return Err(refusal(
                "PRAGMA rekey needs the new key: PRAGMA rekey = 'new passphrase'",
            ));
        };
        let text = argument_text(argument);
        if text.is_empty() {
            return Err(refusal(
                "PRAGMA rekey = '' would remove the encryption, which rewrites every page.                  `inillucent decrypt` writes a plaintext copy",
            ));
        }
        self.rekey(EncryptionKey::parse(&text))?;
        Ok(Outcome::empty())
    }

    /// Changes the key this encrypted database is encrypted with.
    ///
    /// Everything is checkpointed first, then the header of every file of the
    /// database - the database file, its journal and each log segment - is
    /// rewritten under the new key. The data key does not change, so no page
    /// is rewritten, and the old key stops opening the file as soon as the
    /// first header write is on the disk.
    ///
    /// @param key - the new key
    pub fn rekey(&mut self, key: EncryptionKey) -> DbResult<()> {
        if self.storage.read_only {
            return Err(DbError::primary(PrimaryCode::ReadOnly)
                .with_message("a read only connection cannot change the key".to_string()));
        }
        if self.writing.batch().is_some() {
            return Err(statement_refusal(
                "cannot change the key within a transaction",
            ));
        }
        if !self.is_encrypted() {
            return Err(refusal(
                "this database is not encrypted. Changing the key needs an encrypted                  database, and `inillucent encrypt` writes an encrypted copy of this one",
            ));
        }
        self.checkpoint()?;
        let vfs = Arc::clone(&self.storage.vfs);
        let paths = files_of(vfs.as_ref(), &self.storage.path);
        if let Some(crypt) = vfs.encryption() {
            crypt
                .rekey(&paths, key)
                .map_err(|error| error.into_db_error())?;
        }
        Ok(())
    }

    /// Writes a copy of this database to `destination`, encrypted with `key`
    /// or in plaintext when there is none.
    ///
    /// The same rebuild `VACUUM INTO` does, onto a file system of the caller's
    /// choosing, which is what `inillucent encrypt` and `inillucent decrypt`
    /// are.
    ///
    /// @param destination - the file to write, which must not exist
    /// @param key - the key the copy is encrypted with, if any
    pub fn export_to(&mut self, destination: &Path, key: Option<EncryptionKey>) -> DbResult<()> {
        if destination.exists() {
            return Err(refusal("output file already exists"));
        }
        self.checkpoint()?;
        let vfs = file_system(key, self.storage.page_size)?;
        crate::rebuild::rebuild_into(
            vfs,
            self,
            destination,
            self.storage.page_size,
            self.storage.frames,
        )
    }
}

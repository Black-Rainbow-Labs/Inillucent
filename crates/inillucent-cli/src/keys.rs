//! The encryption key the three programs open databases with.
//!
//! Invariant: **a key reaches this process from a file or from the
//! environment, never from a word on the command line.** A command line is
//! readable by every user of the machine through the process list, and it is
//! saved in the shell's history, so `--key <text>` would publish the key it
//! was meant to protect. `--key-file <path>` names a file holding the key, and
//! `INILLUCENT_KEY` holds it in the environment, which only the process and
//! its owner can read.
//!
//! The key is installed once, when the program starts, and every database the
//! process opens after that is opened with it: the one `--db` names, one the
//! shell's `.open` names, and a backup's copy. A new database is created
//! encrypted. An existing plaintext database is refused rather than opened in
//! plaintext, because a caller who set a key believes the file is encrypted.
//! `inillucent encrypt` is the one command that opens its source without the
//! key, since its source is the plaintext file it encrypts.

use std::cell::Cell;
use std::sync::Mutex;

use inillucent_driver::EncryptionKey;

/// The environment variable that holds a key.
pub const KEY_VARIABLE: &str = "INILLUCENT_KEY";

/// The environment variable that names a file holding a key.
pub const KEY_FILE_VARIABLE: &str = "INILLUCENT_KEY_FILE";

/// The environment variable `inillucent rekey` reads the new key from, when
/// it is not given a file.
pub const NEW_KEY_VARIABLE: &str = "INILLUCENT_NEW_KEY";

/// The key this process opens databases with, once it is installed.
static CONFIGURED: Mutex<Option<EncryptionKey>> = Mutex::new(None);

thread_local! {
    /// Set while a command opens its source database without the key.
    static WITHOUT_KEY: Cell<bool> = const { Cell::new(false) };
}

/// Reads a key from a file: its whole contents, less a trailing line end.
///
/// A key file written by `echo` ends in a newline, and a newline is not part
/// of anybody's passphrase.
///
/// @param path - the file
pub fn read_key_file(path: &str) -> Result<EncryptionKey, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read the key file {path}: {error}"))?;
    let key = text.trim_end_matches(['\r', '\n']);
    if key.is_empty() {
        return Err(format!("the key file {path} is empty"));
    }
    Ok(EncryptionKey::parse(key))
}

/// Works out the key from `--key-file`, then `INILLUCENT_KEY_FILE`, then
/// `INILLUCENT_KEY`, and installs it for the rest of the process.
///
/// @param key_file - the `--key-file` the command line gave, if any
pub fn install_from(key_file: Option<&str>) -> Result<(), String> {
    let key = match key_file {
        Some(path) => Some(read_key_file(path)?),
        None => match std::env::var(KEY_FILE_VARIABLE) {
            Ok(path) if !path.is_empty() => Some(read_key_file(&path)?),
            _ => match std::env::var(KEY_VARIABLE) {
                Ok(text) if !text.is_empty() => Some(EncryptionKey::parse(&text)),
                _ => None,
            },
        },
    };
    install(key);
    Ok(())
}

/// Installs a key for the rest of the process, or removes it.
///
/// @param key - the key, or `None` for plaintext
pub fn install(key: Option<EncryptionKey>) {
    if let Ok(mut held) = CONFIGURED.lock() {
        *held = key;
    }
}

/// Returns the key this process was given, whatever is being opened.
pub fn configured() -> Option<EncryptionKey> {
    CONFIGURED.lock().ok().and_then(|held| held.clone())
}

/// Returns the key to open a database with now: the configured one, unless
/// the command running opens its source without it.
pub fn for_opening() -> Option<EncryptionKey> {
    match WITHOUT_KEY.with(Cell::get) {
        true => None,
        false => configured(),
    }
}

/// Runs `work` with databases opened without the key.
///
/// @param work - what opens the database
pub fn opening_without_key<T>(work: impl FnOnce() -> T) -> T {
    let before = WITHOUT_KEY.with(|flag| flag.replace(true));
    let result = work();
    WITHOUT_KEY.with(|flag| flag.set(before));
    result
}

/// Reports whether a command opens its source database without the key.
///
/// `encrypt` reads a plaintext database and writes an encrypted copy, so the
/// key is for the copy and the source has none.
///
/// @param command - the command's name
pub fn opens_source_without_key(command: &str) -> bool {
    command == "encrypt"
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key file's trailing line end is not part of the key, and an empty
    /// file is refused rather than read as an empty passphrase.
    #[test]
    fn a_key_file_is_read_without_its_line_end() {
        let directory =
            std::env::temp_dir().join(format!("inillucent-keys-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("key.txt");
        std::fs::write(&path, "hunter2\r\n").unwrap();
        let key = read_key_file(path.to_str().unwrap()).unwrap();
        assert!(format!("{key:?}").contains("passphrase"));
        std::fs::write(&path, "\n").unwrap();
        assert!(read_key_file(path.to_str().unwrap()).is_err());
        std::fs::write(&path, format!("x'{}'\n", "ab".repeat(32))).unwrap();
        let key = read_key_file(path.to_str().unwrap()).unwrap();
        assert!(format!("{key:?}").contains("raw"));
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Inside `opening_without_key` nothing is opened with the key, and
    /// outside it the key is back.
    #[test]
    fn opening_without_key_hides_the_key_only_inside() {
        install(Some(EncryptionKey::raw([1; 32])));
        assert!(for_opening().is_some());
        assert!(opening_without_key(for_opening).is_none());
        assert!(for_opening().is_some());
        install(None);
    }
}

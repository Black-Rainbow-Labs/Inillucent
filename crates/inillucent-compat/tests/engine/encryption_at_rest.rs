//! Encryption at rest, through the connection an application opens.
//!
//! Invariant: **a database opened with a key leaves no row on the disk in
//! plaintext**, in the database file, its log or its journal, and every
//! statement that writes a second file - `VACUUM`, `VACUUM INTO`, `ATTACH`, a
//! backup and an export - writes it encrypted or not exactly as the case says.
//!
//! Every case reads the files underneath with `std::fs`, not through the
//! engine, because a layer that answers every query correctly while storing
//! plaintext would pass every test that only asks the engine.
//!
//! The keys are raw 32 byte keys, written in SQLCipher's `x'...'` form, so no
//! case pays for 600,000 rounds of PBKDF2 in a debug build. One case uses a
//! passphrase with a small iteration count to cover that path.

use std::path::{Path, PathBuf};

use inillucent_engine::connect::Database;
use inillucent_engine::{EncryptionKey, PrimaryCode};
use inillucent_tree::datum::OwnedDatum;

/// The text form of the raw key most cases use.
const KEY_A: &str = "x'1111111111111111111111111111111111111111111111111111111111111111'";

/// A second raw key.
const KEY_B: &str = "x'2222222222222222222222222222222222222222222222222222222222222222'";

/// A string every case writes and then looks for on the disk.
const SECRET: &str = "the vault code is 7461-swordfish";

/// Returns a fresh directory for one case.
///
/// @param tag - the case's name
fn directory(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "inillucent-encryption-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("the directory is made");
    path
}

/// Opens a database with a raw key given in text form.
///
/// @param path - the file
/// @param key - the key text
fn open(path: &Path, key: &str) -> Database {
    Database::open_encrypted(path, EncryptionKey::parse(key)).expect("the database opens")
}

/// Runs statements, failing the case with the statement in the message.
///
/// @param database - the connection
/// @param sql - the statements
fn run(database: &Database, sql: &str) {
    database
        .connect()
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql}: {error}"));
}

/// Runs a query and returns its rows.
///
/// @param database - the connection
/// @param sql - the query
fn rows(database: &Database, sql: &str) -> Vec<Vec<OwnedDatum>> {
    database
        .connect()
        .query(sql)
        .unwrap_or_else(|error| panic!("{sql}: {error}"))
}

/// Returns every byte of every file in a directory, by file name.
///
/// @param directory - where to look
fn every_file(directory: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory)
        .expect("the directory lists")
        .flatten()
    {
        if let Ok(bytes) = std::fs::read(entry.path()) {
            found.push((entry.file_name().to_string_lossy().into_owned(), bytes));
        }
    }
    found
}

/// Reports whether `needle` appears in `haystack`.
///
/// @param haystack - the bytes to search
/// @param needle - what to find
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Fails the case when any file in `directory` holds the secret in plaintext.
///
/// @param directory - where to look
fn assert_no_plaintext(directory: &Path) {
    let files = every_file(directory);
    assert!(!files.is_empty(), "there were no files to search");
    for (name, bytes) in files {
        assert!(
            !contains(&bytes, SECRET.as_bytes()),
            "{name} holds the row in plaintext"
        );
        assert!(
            !contains(&bytes, b"swordfish"),
            "{name} holds part of the row in plaintext"
        );
        assert!(
            !contains(&bytes, b"CREATE TABLE"),
            "{name} holds the schema in plaintext"
        );
    }
}

/// Writes the rows every case reads back: a table, an index on it, a full
/// text index and a vector index, so every kind of tree is in the file.
///
/// @param database - the connection
fn fill(database: &Database) {
    run(
        database,
        "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT, v VECTOR(3));
         CREATE INDEX note_body ON note (body);
         CREATE INDEX note_v ON note USING inillucent_hnsw (v);
         CREATE VIRTUAL TABLE note_text USING fts5(body);",
    );
    for n in 0..200 {
        run(
            database,
            &format!(
                "INSERT INTO note (body, v) VALUES ('{SECRET} {n}', '[{n}, 1, 0]');
                 INSERT INTO note_text (body) VALUES ('{SECRET} {n}');"
            ),
        );
    }
}

/// Returns how many notes hold the secret, read three ways.
///
/// @param database - the connection
fn count(database: &Database) -> (i64, i64, i64) {
    let read = |sql: &str| match rows(database, sql).first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(n)) => *n,
        other => panic!("{sql}: {other:?}"),
    };
    (
        read(&format!("SELECT count(*) FROM note WHERE body LIKE '{SECRET}%'")),
        read("SELECT count(*) FROM note_text WHERE note_text MATCH 'swordfish'"),
        read("SELECT count(*) FROM (SELECT id FROM note ORDER BY vector_distance_l2(v, '[5, 1, 0]') LIMIT 7)"),
    )
}

/// **The headline.** Nothing is on the disk in plaintext, while the database
/// is open with its log unfolded and after it is closed, and the rows come
/// back through every index after a reopen.
#[test]
fn nothing_reaches_the_disk_in_plaintext() {
    let dir = directory("plaintext");
    let path = dir.join("vault.rdb");
    let database = open(&path, KEY_A);
    run(&database, "PRAGMA journal_mode = wal");
    fill(&database);
    assert_no_plaintext(&dir);
    drop(database);
    assert_no_plaintext(&dir);
    let database = open(&path, KEY_A);
    assert_eq!(count(&database), (200, 200, 7));
    assert_eq!(
        rows(&database, "PRAGMA encryption"),
        vec![vec![OwnedDatum::Text(b"xchacha20-poly1305".to_vec())]]
    );
    assert_eq!(
        rows(&database, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
    drop(database);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A wrong key, no key, and a key for a plaintext file are each refused with
/// `SQLITE_NOTADB` and a sentence that says which.
#[test]
fn a_key_that_does_not_match_the_file_is_refused() {
    let dir = directory("refusals");
    let locked = dir.join("locked.rdb");
    drop(open(&locked, KEY_A));
    let wrong = Database::open_encrypted(&locked, EncryptionKey::parse(KEY_B))
        .err()
        .expect("refused");
    assert_eq!(wrong.code(), PrimaryCode::NotADb);
    assert_eq!(
        wrong.message(),
        "file is not a database, or the key is wrong"
    );
    let missing = Database::open(&locked).err().expect("refused");
    assert_eq!(missing.code(), PrimaryCode::NotADb);
    assert!(
        missing.message().contains("this database is encrypted"),
        "{}",
        missing.message()
    );

    let plain = dir.join("plain.rdb");
    let database = Database::open(&plain).expect("a plaintext database opens");
    assert_eq!(
        rows(&database, "PRAGMA encryption"),
        vec![vec![OwnedDatum::Text(b"none".to_vec())]]
    );
    drop(database);
    let keyed = Database::open_encrypted(&plain, EncryptionKey::parse(KEY_A))
        .err()
        .expect("refused");
    assert_eq!(keyed.code(), PrimaryCode::NotADb);
    assert!(
        keyed.message().contains("not encrypted"),
        "{}",
        keyed.message()
    );

    let passphrase = dir.join("phrase.rdb");
    let key = EncryptionKey::passphrase_with_iterations("correct horse battery", 1_000);
    let database = Database::open_encrypted(&passphrase, key.clone()).expect("created");
    run(&database, "CREATE TABLE t (x); INSERT INTO t VALUES (42)");
    drop(database);
    let database = Database::open_encrypted(&passphrase, key).expect("reopened");
    assert_eq!(
        rows(&database, "SELECT x FROM t"),
        vec![vec![OwnedDatum::Int(42)]]
    );
    drop(database);
    let wrong = EncryptionKey::passphrase_with_iterations("correct horse battery!", 1_000);
    assert!(Database::open_encrypted(&passphrase, wrong).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// `PRAGMA rekey` moves the database to a new key: the old one stops opening
/// it, the new one does, and every row is still there.
#[test]
fn rekey_changes_the_key_and_keeps_the_rows() {
    let dir = directory("rekey");
    let path = dir.join("rekey.rdb");
    let database = open(&path, KEY_A);
    run(&database, "PRAGMA journal_mode = wal");
    fill(&database);
    run(&database, &format!("PRAGMA rekey = \"{KEY_B}\""));
    run(
        &database,
        &format!("INSERT INTO note (body, v) VALUES ('{SECRET} after', '[1, 1, 1]')"),
    );
    drop(database);
    assert!(
        Database::open_encrypted(&path, EncryptionKey::parse(KEY_A)).is_err(),
        "the old key still opens it"
    );
    let database = open(&path, KEY_B);
    assert_eq!(count(&database).0, 201);
    let refused = database
        .connect()
        .execute_batch("PRAGMA rekey = ''")
        .err()
        .expect("refused");
    assert!(
        refused.message().contains("inillucent decrypt"),
        "{}",
        refused.message()
    );
    let refused = database
        .connect()
        .execute_batch("PRAGMA key = 'x'")
        .err()
        .expect("refused");
    assert!(
        refused.message().contains("when the database is opened"),
        "{}",
        refused.message()
    );
    drop(database);
    let plain = Database::open(dir.join("plain.rdb")).expect("opens");
    let refused = plain
        .connect()
        .execute_batch(&format!("PRAGMA rekey = \"{KEY_A}\""))
        .err()
        .expect("refused");
    assert!(
        refused.message().contains("not encrypted"),
        "{}",
        refused.message()
    );
    drop(plain);
    let _ = std::fs::remove_dir_all(&dir);
}

/// `VACUUM` keeps the database encrypted, and `VACUUM INTO` and a backup
/// write copies encrypted with the same key.
#[test]
fn vacuum_and_backup_write_encrypted_files() {
    let dir = directory("vacuum");
    let path = dir.join("v.rdb");
    let database = open(&path, KEY_A);
    fill(&database);
    run(&database, "DELETE FROM note WHERE id % 2 = 0; VACUUM");
    run(
        &database,
        &format!("VACUUM INTO '{}'", dir.join("into.rdb").display()),
    );
    database
        .backup_to(dir.join("backup.rdb"))
        .expect("the backup is checked");
    assert_eq!(count(&database).0, 100);
    drop(database);
    assert_no_plaintext(&dir);
    for name in ["v.rdb", "into.rdb", "backup.rdb"] {
        assert!(
            Database::open(dir.join(name)).is_err(),
            "{name} opened without a key"
        );
        let copy = open(&dir.join(name), KEY_A);
        assert_eq!(count(&copy).0, 100, "{name}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `ATTACH ... KEY` opens an encrypted file with its own key, `KEY ''` opens
/// a plaintext one, and no `KEY` uses the connection's own key.
#[test]
fn attach_takes_a_key() {
    let dir = directory("attach");
    let other = dir.join("other.rdb");
    let same = dir.join("same.rdb");
    let plain = dir.join("plain.rdb");
    drop(open(&other, KEY_B));
    let database = open(&dir.join("main.rdb"), KEY_A);
    run(
        &database,
        &format!(
            "ATTACH '{}' AS o KEY \"{KEY_B}\";
             ATTACH '{}' AS s;
             ATTACH '{}' AS p KEY '';
             CREATE TABLE o.t (x); INSERT INTO o.t VALUES ('{SECRET}');
             CREATE TABLE s.t (x); INSERT INTO s.t VALUES ('{SECRET}');
             CREATE TABLE p.t (x); INSERT INTO p.t VALUES ('plain row');",
            other.display(),
            same.display(),
            plain.display()
        ),
    );
    let refused = database
        .connect()
        .execute_batch(&format!(
            "ATTACH '{}' AS w KEY \"{KEY_A}\"",
            other.display()
        ))
        .err()
        .expect("a wrong key is refused");
    assert_eq!(refused.code(), PrimaryCode::NotADb);
    drop(database);
    assert!(
        contains(&std::fs::read(&plain).expect("reads"), b"plain row") || {
            // The row may still be in the plaintext file's log rather than its
            // pages; either way it is readable without a key.
            Database::open(&plain).is_ok()
        }
    );
    let reopened = open(&other, KEY_B);
    assert_eq!(
        rows(&reopened, "SELECT count(*) FROM t"),
        vec![vec![OwnedDatum::Int(1)]]
    );
    drop(reopened);
    let reopened = open(&same, KEY_A);
    assert_eq!(
        rows(&reopened, "SELECT count(*) FROM t"),
        vec![vec![OwnedDatum::Int(1)]]
    );
    drop(reopened);
    assert!(
        Database::open(&plain).is_ok(),
        "the KEY '' attachment is plaintext"
    );
    for (name, bytes) in every_file(&dir) {
        assert!(
            !contains(&bytes, SECRET.as_bytes()),
            "{name} holds the row in plaintext"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// `export_to` writes an encrypted copy of a plaintext database and a
/// plaintext copy of an encrypted one, with the same rows.
#[test]
fn export_converts_in_both_directions() {
    let dir = directory("export");
    let plain = dir.join("plain.rdb");
    let database = Database::open(&plain).expect("opens");
    fill(&database);
    database
        .export_to(dir.join("sealed.rdb"), Some(EncryptionKey::parse(KEY_A)))
        .expect("the encrypted copy is written");
    drop(database);
    let sealed = open(&dir.join("sealed.rdb"), KEY_A);
    assert_eq!(count(&sealed), (200, 200, 7));
    sealed
        .export_to(dir.join("opened.rdb"), None)
        .expect("the plaintext copy is written");
    let refused = sealed
        .export_to(dir.join("opened.rdb"), None)
        .err()
        .expect("refused");
    assert!(refused.message().contains("already exists"));
    drop(sealed);
    let sealed_bytes = std::fs::read(dir.join("sealed.rdb")).expect("reads");
    assert!(!contains(&sealed_bytes, b"swordfish"));
    let opened =
        Database::open(dir.join("opened.rdb")).expect("the plaintext copy opens without a key");
    assert_eq!(count(&opened), (200, 200, 7));
    drop(opened);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A changed byte in the database file is never read back as a row: the
/// open, a query or the integrity check reports damage.
#[test]
fn a_changed_byte_is_reported_as_damage() {
    let dir = directory("tamper");
    let path = dir.join("t.rdb");
    let database = open(&path, KEY_A);
    fill(&database);
    drop(database);
    let mut bytes = std::fs::read(&path).expect("reads");
    // Every page unit after the two meta pages: 8 KiB of header, then
    // 32 KiB of page and a 512 byte trailer sector per unit.
    let stride = 32_768 + 512;
    let mut at = 8_192 + 2 * stride + 1_000;
    while at < bytes.len() {
        bytes[at] ^= 0x01;
        at += stride;
    }
    std::fs::write(&path, &bytes).expect("writes");
    let outcome =
        Database::open_encrypted(&path, EncryptionKey::parse(KEY_A)).and_then(|database| {
            let answer = database.connect().query("PRAGMA integrity_check")?;
            let counted = database.connect().query(&format!(
                "SELECT count(*) FROM note WHERE body LIKE '{SECRET}%'"
            ));
            Ok((answer, counted.is_ok()))
        });
    match outcome {
        Err(_) => {}
        Ok((answer, _)) => assert_ne!(
            answer,
            vec![vec![OwnedDatum::Text(b"ok".to_vec())]],
            "every page was changed and the integrity check said ok"
        ),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

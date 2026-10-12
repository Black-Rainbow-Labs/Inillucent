//! Which file format a file says, and which extent layout its pages are in.
//!
//! Invariant: **a file this build creates says format 3 and writes extent pages in
//! the owned layout; a format 2 file stays format 2 through every write that does
//! not store a key out of line, and writes extent pages in the older layout until a
//! key longer than `page_size / 8` or a `VACUUM` raises it to 3.** After that it
//! writes owned pages, and the file holds both. Every extent page says in its own
//! flag byte which layout it is in. A format 2 file is byte for byte a file every
//! release from 0.1.8 reads, so stamping 3 on it for nothing would turn a readable
//! file into a refused one.
//!
//! The built binary writes the file and the test reads the bytes, because the
//! number is a property of the file and a check through the library would ask the
//! process that raised it.

use std::path::{Path, PathBuf};

use inillucent_compat::cliproc::{number_field, program, run};
use inillucent_compat::workspace_root;

/// Returns an emptied directory of this case's own.
///
/// @param case - what to name it after
fn area(case: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/format-three")
        .join(case);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs one statement against a database with the built binary and asserts it worked.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
/// @param sql - the statement
fn exec(binary: &Path, database: &Path, sql: &str) {
    let path = database.to_string_lossy().to_string();
    let ran = run(binary, &["--db", path.as_str(), "exec", sql]);
    assert_eq!(ran.code, 0, "`{sql}` exited {}:\n{}", ran.code, ran.said());
}

/// Returns the format version both copies of the meta record carry.
///
/// The record lives on page 0 and its shadow on page 1, each with the version at
/// byte 8. They are written together, so a file that says one thing in one and
/// another in the other is damaged.
///
/// @param database - the database file
fn formats_of(database: &Path) -> (u32, u32) {
    let bytes = std::fs::read(database).expect("the database reads back");
    let read = |at: usize| -> u32 {
        let mut four = [0u8; 4];
        four.copy_from_slice(bytes.get(at..at + 4).expect("a header"));
        u32::from_le_bytes(four)
    };
    let page_size = read(12) as usize;
    (read(8), read(page_size + 8))
}

/// A database made through the binary, checkpointed, holding one table and one index.
///
/// @param binary - the built `inillucent`
/// @param case - what to name the directory after
/// @param value - the SQL expression the first indexed value is
fn made(binary: &Path, case: &str, value: &str) -> PathBuf {
    let database = area(case).join("app.rdb");
    let path = database.to_string_lossy().to_string();
    let created = run(binary, &["create", path.as_str(), "--output", "json"]);
    assert_eq!(created.code, 0, "create: {}", created.said());
    exec(
        binary,
        &database,
        "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
    );
    exec(binary, &database, "CREATE INDEX t_v ON t (v)");
    exec(
        binary,
        &database,
        &format!("INSERT INTO t VALUES (1, {value})"),
    );
    let checkpointed = run(binary, &["--db", path.as_str(), "checkpoint"]);
    assert_eq!(checkpointed.code, 0, "checkpoint: {}", checkpointed.said());
    database
}

/// A file this build creates says format 3 in both copies of its meta record, with
/// no key stored out of line in it.
#[test]
fn a_new_file_says_format_three_whatever_it_holds() {
    let binary = program("inillucent");
    let database = made(&binary, "short", "'a short key'");
    assert_eq!(formats_of(&database), (3, 3));
}

/// Counts the extent pages of a file in each layout, by the flag byte of their own
/// headers: `(older layout, owned layout)`.
///
/// @param database - the database file
fn extent_layouts(database: &Path) -> (usize, usize) {
    let bytes = std::fs::read(database).expect("the database reads back");
    let page_size = formats_page_size(&bytes);
    let mut counts = (0usize, 0usize);
    for page in bytes.chunks(page_size).skip(2) {
        let kind = page.get(12).copied().unwrap_or(0);
        let flags = page.get(13).copied().unwrap_or(0);
        if kind == 3 || kind == 6 {
            match flags & inillucent_pool::page::EXTENT_OWNED {
                0 => counts.0 += 1,
                _ => counts.1 += 1,
            }
        }
    }
    counts
}

/// Returns the page size a file's meta record carries.
///
/// @param bytes - the file's bytes
fn formats_page_size(bytes: &[u8]) -> usize {
    let mut four = [0u8; 4];
    four.copy_from_slice(bytes.get(12..16).expect("a header"));
    u32::from_le_bytes(four) as usize
}

/// Rewrites both copies of a file's meta record to say format 2, as a release
/// before extent pages were owned left it.
///
/// @param database - the database file
fn lowered_to_format_two(database: &Path) {
    let mut bytes = std::fs::read(database).expect("the database reads back");
    let page_size = formats_page_size(&bytes);
    for slot in 0..2 {
        let range = slot * page_size..(slot + 1) * page_size;
        let page = bytes.get_mut(range).expect("a meta page");
        let mut meta = inillucent_pool::meta::Meta::decode(page).expect("a meta record");
        meta.format = 2;
        meta.encode(page).expect("the record encodes");
    }
    std::fs::write(database, bytes).expect("the file is written back");
}

/// A format 2 file stays format 2 through a write that stores no key out of line
/// and writes its extent pages in the older layout, and a long key raises it to 3
/// after which new extent pages are owned and the file holds both layouts.
#[test]
fn a_format_two_file_keeps_the_older_layout_until_a_long_key_raises_it() {
    let binary = program("inillucent");
    let database = made(&binary, "older", "'a short key'");
    lowered_to_format_two(&database);
    assert_eq!(formats_of(&database), (2, 2));

    exec(
        &binary,
        &database,
        "CREATE TABLE blobs (id INTEGER PRIMARY KEY, body TEXT)",
    );
    exec(
        &binary,
        &database,
        "INSERT INTO blobs VALUES (1, printf('%.*c', 100000, 'x'))",
    );
    let path = database.to_string_lossy().to_string();
    let checkpointed = run(&binary, &["--db", path.as_str(), "checkpoint"]);
    assert_eq!(checkpointed.code, 0, "checkpoint: {}", checkpointed.said());
    assert_eq!(
        formats_of(&database),
        (2, 2),
        "a value out of line is not a key"
    );
    let (older, owned) = extent_layouts(&database);
    assert!(older > 0 && owned == 0, "{older} older, {owned} owned");

    exec(
        &binary,
        &database,
        "INSERT INTO t VALUES (2, replace(hex(zeroblob(25000)), '0', 'q'))",
    );
    exec(
        &binary,
        &database,
        "INSERT INTO blobs VALUES (2, printf('%.*c', 100000, 'y'))",
    );
    let checkpointed = run(&binary, &["--db", path.as_str(), "checkpoint"]);
    assert_eq!(checkpointed.code, 0, "checkpoint: {}", checkpointed.said());
    assert_eq!(formats_of(&database), (3, 3));
    let (older_now, owned_now) = extent_layouts(&database);
    assert!(
        older_now > 0 && owned_now > 0,
        "{older_now} older, {owned_now} owned"
    );
    let verified = run(&binary, &["--db", path.as_str(), "integrity-check"]);
    assert_eq!(verified.code, 0, "integrity-check: {}", verified.said());
}

/// A file with an indexed value over `page_size / 8` says 3 in both copies of the
/// meta record once it is checkpointed, and a build that reads it finds the row.
#[test]
fn a_file_holding_a_key_stored_out_of_line_says_format_three() {
    let binary = program("inillucent");
    let long = "replace(hex(zeroblob(25000)), '0', 'q')";
    let database = made(&binary, "long", long);
    assert_eq!(formats_of(&database), (3, 3));

    let path = database.to_string_lossy().to_string();
    let found = run(
        &binary,
        &[
            "--db",
            path.as_str(),
            "query",
            &format!("SELECT id FROM t WHERE v = {long}"),
            "--output",
            "json",
        ],
    );
    assert_eq!(found.code, 0, "query: {}", found.said());
    assert_eq!(number_field(&found.stdout, "total"), 1.0);
    let verified = run(&binary, &["--db", path.as_str(), "integrity-check"]);
    assert_eq!(verified.code, 0, "integrity-check: {}", verified.said());
    assert!(verified.said().contains("ok"), "{}", verified.said());
}

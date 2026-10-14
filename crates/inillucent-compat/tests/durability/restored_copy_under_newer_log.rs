//! An older copy of a database file, put back beside the log segments of the newer file, opens.
//!
//! Invariant: **a log written against a different state of the file than the one being opened is
//! set aside and not replayed, and a log that does belong to the file is still replayed.**
//!
//! ## Why this exists
//!
//! A log segment names its database by uuid and its place in the chain by sequence number, and
//! a copy of the file has the same uuid and the same checkpoint. So an older copy of the file put
//! back over the file it was copied from, with the newer file's segments still beside it, passes
//! every check the segment header offers. What the copy lacks is the pages a bulk build of the
//! newer file wrote straight into it, which the log does not carry. The open failed with
//! `replaying the log: record kind 18 ... read 0 of 32768 bytes` on a 1.1 GB file, and on a small
//! file replayed rows into a page that was never a leaf and left `sqlite_stat1` and the new table
//! failing `integrity_check` (bug hunt 10, found by the speed hunter restoring a copy after
//! `ANALYZE`).
//!
//! The open now reads a committed bulk build whose root page the file does not hold as the sign
//! that the log is not the file's, renames the segments with `.foreign` added to their names, and
//! opens the file as the copy was.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc::program;
use inillucent_compat::workspace_root;
use inillucent_engine::connect::Database;
use inillucent_tree::datum::OwnedDatum;

/// Returns an empty scratch directory for one test.
///
/// @param name - the test's name
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/restored-copy-under-newer-log")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Builds the base file: a table of 2,000 rows with an index, closed cleanly.
///
/// @param path - where the file goes
fn make_the_base(path: &Path) {
    let database = Database::open(path).expect("the base file opens");
    database
        .session()
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, x TEXT); CREATE INDEX tx ON t(x); \
             WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < 2000) \
             INSERT INTO t SELECT i, hex(randomblob(450)) FROM s",
        )
        .expect("the base rows are written");
}

/// Returns the names of the files beside a database whose names start with the database's.
///
/// @param path - the database file
fn files_beside(path: &Path) -> Vec<String> {
    let stem = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut names: Vec<String> = std::fs::read_dir(path.parent().unwrap_or(Path::new(".")))
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(&format!("{stem}-wal.")))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Returns the first value of a query's first row, as text.
///
/// @param database - the open database
/// @param sql - the query
fn first_value(database: &Database, sql: &str) -> String {
    let rows = database.session().query(sql).expect("the query runs");
    match rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(number)) => number.to_string(),
        Some(OwnedDatum::Text(text)) => String::from_utf8_lossy(text).into_owned(),
        other => format!("{other:?}"),
    }
}

/// An older copy of the file, beside the log of the newer file, opens as the copy was.
#[test]
fn an_older_copy_of_the_file_opens_beside_a_newer_files_log() {
    let directory = area("older-copy");
    let older = directory.join("older.rdb");
    let live = directory.join("live.rdb");
    make_the_base(&older);
    std::fs::copy(&older, &live).expect("the base is copied");
    {
        let database = Database::open(&live).expect("the copy opens");
        database
            .session()
            .execute_batch("CREATE TABLE u(a, b); INSERT INTO u VALUES (1, 'x'), (2, 'y'); ANALYZE")
            .expect("the newer file is written");
    }
    let segments = files_beside(&live);
    assert!(
        segments.iter().any(|name| !name.ends_with(".foreign")),
        "the newer file left no log segment, so there is nothing to be foreign: {segments:?}"
    );
    std::fs::copy(&older, &live).expect("the older copy is put back");

    let database = Database::open(&live).expect("the restored copy opens beside the newer log");
    assert_eq!(first_value(&database, "SELECT count(*) FROM t"), "2000");
    assert_eq!(
        first_value(
            &database,
            "SELECT count(*) FROM sqlite_schema WHERE name IN ('u', 'sqlite_stat1')"
        ),
        "0",
        "the copy was made before u and sqlite_stat1 existed"
    );
    assert_eq!(first_value(&database, "PRAGMA integrity_check"), "ok");
    assert!(
        database.recovery_report().set_aside >= 1,
        "the open did not report setting the newer log aside: {:?}",
        database.recovery_report()
    );
    drop(database);
    let after = files_beside(&live);
    assert!(
        after.iter().any(|name| name.ends_with(".foreign")),
        "the newer log was not kept under a name ending .foreign: {after:?}"
    );

    let again = Database::open(&live).expect("the file opens a second time");
    assert_eq!(first_value(&again, "SELECT count(*) FROM t"), "2000");
    assert_eq!(again.recovery_report().set_aside, 0);
}

/// A committed table the process never folded into the file is still replayed on the next open.
///
/// The writer is killed after its commit, so the file holds the bulk built pages and the log
/// holds everything else. This is the log that belongs to the file, and setting it aside would
/// lose two acknowledged rows.
#[test]
fn a_log_that_belongs_to_the_file_is_still_replayed() {
    let directory = area("own-log");
    let path = directory.join("live.rdb");
    make_the_base(&path);
    let mut child = Command::new(program("inillucent-shell"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the shell starts");
    child
        .stdin
        .as_mut()
        .expect("the shell has standard input")
        .write_all(
            b"CREATE TABLE u(a, b);\nINSERT INTO u VALUES (1, 'x'), (2, 'y');\nSELECT 'committed';\n",
        )
        .expect("the script is written");
    let mut seen = String::new();
    if let Some(output) = child.stdout.take() {
        for line in BufReader::new(output).lines().map_while(Result::ok) {
            seen.push_str(&line);
            if line.contains("committed") {
                break;
            }
        }
    }
    assert!(
        seen.contains("committed"),
        "the shell never answered: {seen:?}"
    );
    child.kill().expect("the shell is killed");
    let _ = child.wait();

    let database = Database::open(&path).expect("the file opens and replays its own log");
    assert_eq!(first_value(&database, "SELECT count(*) FROM u"), "2");
    assert_eq!(first_value(&database, "PRAGMA integrity_check"), "ok");
    assert_eq!(database.recovery_report().set_aside, 0);
    assert!(
        !files_beside(&path)
            .iter()
            .any(|name| name.ends_with(".foreign")),
        "a log that belongs to the file was set aside"
    );
}

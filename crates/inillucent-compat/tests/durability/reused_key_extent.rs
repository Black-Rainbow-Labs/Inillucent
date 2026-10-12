//! A database whose log names a long key whose pages a later write reused opens again.
//!
//! Invariant: **recovery replays a log in which a key stored out of line was deleted, its pages
//! freed and allocated to something else, and the file opens with the rows the log committed.**
//!
//! ## Why this exists
//!
//! A key over `page_size / 8` is stored on pages of its own, and its insert record names them.
//! Deleting the row frees those pages, and a later write in the same log window can be given them.
//! The page then holds a stamp newer than the insert record, so its extent records are skipped by
//! the page LSN rule, and the replay of the insert read the key from a page that was now a leaf of
//! another table. The open failed with `replaying the log: record kind 18 ... page is not a blob
//! extent`, and the database could not be opened at all (bug hunt 9, found by the random query
//! generator). A process that leaves a transaction open when it exits skips the checkpoint, so the
//! next open replays the whole log: `inillucent-shell` with a `BEGIN` and no `COMMIT` is enough.

use std::io::Write;
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
        .join("_agent_output/reused-key-extent")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs a script through `inillucent-shell` on a file and waits for it to exit.
///
/// @param path - the database file
/// @param script - the statements, fed on standard input
fn shell(path: &Path, script: &str) {
    let mut child = Command::new(program("inillucent-shell"))
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the shell starts");
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(script.as_bytes())
            .expect("the script is written");
    }
    drop(child.stdin.take());
    child.wait().expect("the shell exits");
}

/// Returns the rows of a query on a fresh open of the file, each row as text.
///
/// @param path - the file
/// @param sql - the query
fn rows_after_open(path: &Path, sql: &str) -> Result<Vec<String>, String> {
    let database = Database::open(path).map_err(|error| error.message().to_string())?;
    let rows = database
        .session()
        .query(sql)
        .map_err(|error| error.message().to_string())?;
    Ok(rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| match value {
                    OwnedDatum::Int(number) => number.to_string(),
                    OwnedDatum::Text(text) => String::from_utf8_lossy(text).into_owned(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect())
}

/// The pages of a deleted long key are given to a new table, and the log is replayed.
#[test]
fn a_reused_key_extent_does_not_stop_the_file_opening() {
    for (name, reuse) in [
        ("analyze", "ANALYZE;"),
        ("new-table", "CREATE TABLE z(q); INSERT INTO z VALUES (1);"),
        (
            "new-rows",
            "CREATE TABLE z(q); INSERT INTO z SELECT value FROM generate_series(1, 5000);",
        ),
    ] {
        let path = area(name).join("case.rdb");
        shell(
            &path,
            &format!(
                "CREATE TABLE t(a, b, PRIMARY KEY(a, b));\n\
                 INSERT INTO t VALUES (hex(zeroblob(20000)), 1), (hex(zeroblob(30000)), 2), ('x', 3);\n\
                 DELETE FROM t WHERE b < 3;\n\
                 {reuse}\n\
                 BEGIN;\n\
                 SELECT 1;\n"
            ),
        );
        let opened = rows_after_open(&path, "PRAGMA integrity_check");
        assert_eq!(
            opened,
            Ok(vec!["ok".to_string()]),
            "{name}: the file did not open cleanly"
        );
        assert_eq!(
            rows_after_open(&path, "SELECT count(*), max(b) FROM t"),
            Ok(vec!["1|3".to_string()]),
            "{name}: the committed rows did not come back"
        );
    }
}

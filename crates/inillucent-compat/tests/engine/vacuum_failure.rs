//! A `VACUUM` that fails leaves nothing beside the database.
//!
//! Invariant: **after a failed `VACUUM` or `VACUUM INTO`, the directory holds the database, its
//! log segments, and no file the rebuild made.** The rebuild writes the new file beside the
//! database as `<db>.vacuum-<stamp>`, and a copy that failed part way left it there with its log
//! segment: two files of 163 to 196 KB for every failed attempt, on a file that was already
//! failing.
//!
//! The failure is made with a collation registered on one connection. The rebuild replays the
//! stored `CREATE TABLE` in a new file that has no such collation, so the replay is refused after
//! the new file exists, which is the point the cleanup has to cover.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use inillucent_compat::facade::Database;

/// Returns an empty directory of this test's own under the gitignored root.
///
/// @param name - what to call it
fn area(name: &str) -> PathBuf {
    let directory = inillucent_compat::workspace_root()
        .join("_agent_output")
        .join("vacuum-failure")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    let _ = std::fs::create_dir_all(&directory);
    directory
}

/// Returns the names of the files in a directory, sorted.
///
/// @param directory - the directory to list
fn names(directory: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// Opens a database with a table whose column uses a collation only this connection has.
///
/// @param directory - where the database file goes
fn database_with_a_private_collation(directory: &Path) -> Database {
    let database = Database::open(directory.join("app.rdb")).expect("the database opens");
    let connection = database.session().expect("the connection opens");
    connection
        .create_collation(
            "REVERSED",
            Arc::new(|left: &[u8], right: &[u8]| right.cmp(left)),
        )
        .expect("the collation registers");
    connection
        .execute_batch("CREATE TABLE w(x TEXT COLLATE REVERSED); INSERT INTO w VALUES ('a'),('b')")
        .expect("the table is made");
    drop(connection);
    database
}

/// A `VACUUM` that is refused after the new file exists removes that file and its log.
#[test]
fn a_failed_vacuum_removes_the_files_its_rebuild_made() {
    let directory = area("in-place");
    let database = database_with_a_private_collation(&directory);
    let connection = database.session().expect("the connection opens");
    let refused = connection.execute_batch("VACUUM");
    assert!(
        refused.is_err(),
        "VACUUM of a table that uses a collation the rebuilt file lacks should be refused, so \
         this test is not exercising the cleanup"
    );
    let listed = names(&directory);
    let stray: Vec<&String> = listed
        .iter()
        .filter(|name| name.contains(".vacuum-"))
        .collect();
    assert!(
        stray.is_empty(),
        "the failed VACUUM left {stray:?} beside the database; the directory holds {listed:?}"
    );
}

/// A `VACUUM INTO` that is refused after the new file exists removes it and its log.
#[test]
fn a_failed_vacuum_into_removes_the_file_it_was_given() {
    let directory = area("into");
    let database = database_with_a_private_collation(&directory);
    let connection = database.session().expect("the connection opens");
    let copy = directory.join("copy.rdb");
    let sql = format!(
        "VACUUM INTO '{}'",
        copy.display().to_string().replace('\\', "/")
    );
    assert!(
        connection.execute_batch(&sql).is_err(),
        "VACUUM INTO of a table that uses a collation the copy lacks should be refused"
    );
    let listed = names(&directory);
    let stray: Vec<&String> = listed
        .iter()
        .filter(|name| name.starts_with("copy.rdb"))
        .collect();
    assert!(
        stray.is_empty(),
        "the failed VACUUM INTO left {stray:?}; the directory holds {listed:?}"
    );
}

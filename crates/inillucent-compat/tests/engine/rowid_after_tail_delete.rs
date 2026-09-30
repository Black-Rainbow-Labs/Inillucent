//! An insert after the newest rows were deleted takes the next rowid above the
//! largest one left.
//!
//! Invariant: **a table that is not `AUTOINCREMENT` gives a new row the largest
//! rowid it holds plus one**, which is SQLite's rule and what makes an insert
//! after a delete reuse the number. The largest rowid is read from the
//! rightmost leaf. When every row in that leaf has been deleted, the leaf holds
//! no live row, and the answer used to be zero: the insert then took rowid 1,
//! which the table already held, and failed with `UNIQUE constraint failed:
//! <table>.rowid`.
//!
//! That is what stopped Nikaya's embedding pass (task-2150). A sync tombstoned
//! the newest messages, the cascade deleted their `chunk_embedding` rows, which
//! were the last rows in the table, and every later insert into
//! `chunk_embedding` failed. The table here has the same shape: a composite
//! `PRIMARY KEY`, so the rowid is generated rather than given.

use std::path::PathBuf;

use inillucent_compat::workspace_root;
use inillucent_engine::connect::{Connection, Database};
use inillucent_engine::OwnedDatum;

/// How many rows the table starts with. About thirty fit in a leaf, so this is
/// dozens of leaves.
const ROWS: i64 = 2000;

/// How many of the newest rows are deleted: more than a leaf holds, so the
/// rightmost leaf is left with no live row.
const DELETED: i64 = 100;

/// Returns a fresh database file for one case.
///
/// @param name - the case's name, which is also its directory
fn fresh(name: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output/rowid-after-tail-delete")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("the case's directory was not made: {error}"));
    directory.join("rows.rdb")
}

/// Runs a statement and returns the first column of its last row as an integer.
///
/// @param session - the connection
/// @param sql - the statement
fn number(session: &Connection<'_>, sql: &str) -> i64 {
    let mut statement = session
        .prepare(sql)
        .unwrap_or_else(|error| panic!("{sql} did not prepare: {error}"));
    let mut found = None;
    while statement
        .step()
        .unwrap_or_else(|error| panic!("{sql} failed: {error}"))
    {
        found = match statement.row().first() {
            Some(OwnedDatum::Int(value)) => Some(*value),
            _ => None,
        };
    }
    found.unwrap_or_else(|| panic!("{sql} returned no number"))
}

/// Builds the table and deletes its newest rows.
///
/// @param session - the connection
fn table_with_its_tail_deleted(session: &Connection<'_>) {
    session
        .execute_batch(&format!(
            "CREATE TABLE embedding (chunk TEXT NOT NULL, model TEXT NOT NULL, body TEXT NOT NULL, \
             PRIMARY KEY (chunk, model));
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {ROWS})
             INSERT INTO embedding (chunk, model, body)
             SELECT printf('chunk-%05d', i), 'm', replace(hex(zeroblob(500)), '0', 'a') FROM n;
             DELETE FROM embedding WHERE rowid > {};",
            ROWS - DELETED
        ))
        .unwrap_or_else(|error| panic!("the table was not built: {error}"));
    assert_eq!(
        number(session, "SELECT max(rowid) FROM embedding"),
        ROWS - DELETED,
        "the newest rows were not deleted, so this case tests nothing"
    );
}

/// The insert after the delete succeeds, on the connection that deleted.
#[test]
fn an_insert_after_the_newest_rows_are_deleted_takes_the_next_rowid() {
    let path = fresh("same-connection");
    let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
    let session = database.session();
    table_with_its_tail_deleted(&session);
    session
        .execute_batch("INSERT INTO embedding (chunk, model, body) VALUES ('new', 'm', 'x')")
        .unwrap_or_else(|error| panic!("the insert after the delete failed: {error}"));
    assert_eq!(
        number(&session, "SELECT rowid FROM embedding WHERE chunk = 'new'"),
        ROWS - DELETED + 1
    );
}

/// The insert after the delete succeeds on a connection that opened the file
/// afterwards, which reads the rightmost leaf from the file.
#[test]
fn an_insert_after_a_reopen_takes_the_next_rowid() {
    let path = fresh("after-reopen");
    {
        let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
        let session = database.session();
        table_with_its_tail_deleted(&session);
        drop(session);
        database
            .checkpoint()
            .unwrap_or_else(|error| panic!("checkpoint: {error}"));
    }
    let database = Database::open(&path).unwrap_or_else(|error| panic!("reopen: {error}"));
    let session = database.session();
    session
        .execute_batch("INSERT INTO embedding (chunk, model, body) VALUES ('new', 'm', 'x')")
        .unwrap_or_else(|error| panic!("the insert after the reopen failed: {error}"));
    assert_eq!(
        number(&session, "SELECT rowid FROM embedding WHERE chunk = 'new'"),
        ROWS - DELETED + 1
    );
    assert_eq!(
        number(&session, "SELECT count(*) FROM embedding"),
        ROWS - DELETED + 1
    );
}

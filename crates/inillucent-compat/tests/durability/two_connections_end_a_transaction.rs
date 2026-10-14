//! A connection can always end its transaction, whatever another connection is doing to the file.
//!
//! Invariant: **`ROLLBACK`, a `COMMIT` of a transaction that wrote nothing, and a `PRAGMA` that
//! sets only this connection take no lock**, so they succeed while another connection holds the
//! file to write it. **A transaction that has read and written nothing reopens a file another
//! connection replaced with `VACUUM`** and goes on.
//!
//! ## The defects (bug hunt 10)
//!
//! - After another connection's `VACUUM`, a connection inside `BEGIN` that had not read yet was
//!   refused `busy` on its next statement, and its `ROLLBACK` was refused the same way. It could
//!   not leave the transaction until it was closed. SQLite runs the statement on the new file.
//! - `ROLLBACK` asked for the read lock and failed `busy` while another connection held the file
//!   under `locking_mode = exclusive`. The transaction stayed open, the next statement ran inside
//!   it and reported success, and it was gone when the connection closed.
//! - `PRAGMA busy_timeout` failed `busy` in the same state.

use std::path::PathBuf;

use inillucent_compat::workspace_root;
use inillucent_driver::{Database, OpenOptions, Value};

/// Returns a fresh database path for one test.
///
/// @param name - the test's name
fn fresh(name: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output/two-connections-end-a-transaction")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("the case's directory was not made: {error}"));
    directory.join("ends.rdb")
}

/// Opens a connection that waits for no lock, so a refusal shows at once.
///
/// @param path - the database file
fn open(path: &std::path::Path) -> Database {
    Database::open_with(
        path,
        OpenOptions {
            busy_timeout: Some(std::time::Duration::from_millis(0)),
            ..OpenOptions::default()
        },
    )
    .unwrap_or_else(|error| panic!("open: {error:?}"))
}

/// Returns `n` of the one row of `t`, read by a new connection after every other one closed.
///
/// @param path - the database file
fn stored_n(path: &std::path::Path) -> Option<Value> {
    let database = open(path);
    let session = database.session();
    let rows = session
        .query_all("SELECT n FROM t", &[])
        .unwrap_or_else(|error| panic!("read back: {error:?}"));
    rows.value(0, 0).cloned()
}

/// After another connection's `VACUUM`, a transaction that had touched nothing runs on the new file.
#[test]
fn a_transaction_that_touched_nothing_goes_on_after_another_connections_vacuum() {
    let path = fresh("vacuum");
    {
        let a = open(&path);
        let one = a.session();
        one.execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER); INSERT INTO t VALUES (1, 0)",
        )
        .unwrap_or_else(|error| panic!("setup: {error:?}"));
        let b = open(&path);
        let two = b.session();
        one.execute_batch("BEGIN")
            .unwrap_or_else(|error| panic!("BEGIN: {error:?}"));
        two.execute_batch("VACUUM")
            .unwrap_or_else(|error| panic!("VACUUM: {error:?}"));
        one.execute("UPDATE t SET n = 1", &[])
            .unwrap_or_else(|error| panic!("the UPDATE after the VACUUM was refused: {error:?}"));
        assert!(
            matches!(one.in_transaction(), Ok(true)),
            "the transaction is still open"
        );
        one.execute_batch("COMMIT")
            .unwrap_or_else(|error| panic!("COMMIT: {error:?}"));
        assert!(matches!(one.in_transaction(), Ok(false)));
    }
    assert_eq!(stored_n(&path), Some(Value::Integer(1)));
}

/// A `ROLLBACK` succeeds after another connection's `VACUUM`, and the connection is usable after it.
#[test]
fn a_rollback_succeeds_after_another_connections_vacuum() {
    let path = fresh("vacuum-rollback");
    {
        let a = open(&path);
        let one = a.session();
        one.execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER); INSERT INTO t VALUES (1, 0)",
        )
        .unwrap_or_else(|error| panic!("setup: {error:?}"));
        let b = open(&path);
        let two = b.session();
        one.execute_batch("BEGIN")
            .unwrap_or_else(|error| panic!("BEGIN: {error:?}"));
        two.execute_batch("VACUUM")
            .unwrap_or_else(|error| panic!("VACUUM: {error:?}"));
        one.execute_batch("ROLLBACK")
            .unwrap_or_else(|error| panic!("ROLLBACK was refused: {error:?}"));
        assert!(matches!(one.in_transaction(), Ok(false)));
        one.execute("UPDATE t SET n = 2", &[])
            .unwrap_or_else(|error| panic!("the UPDATE after the ROLLBACK: {error:?}"));
    }
    assert_eq!(stored_n(&path), Some(Value::Integer(2)));
}

/// `ROLLBACK` and `PRAGMA busy_timeout` succeed while another connection holds the file exclusively.
#[test]
fn a_rollback_succeeds_while_another_connection_holds_the_file() {
    let path = fresh("exclusive");
    {
        let a = open(&path);
        let one = a.session();
        one.execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, n INTEGER); INSERT INTO t VALUES (1, 0)",
        )
        .unwrap_or_else(|error| panic!("setup: {error:?}"));
        let b = open(&path);
        let two = b.session();
        one.execute_batch("BEGIN")
            .unwrap_or_else(|error| panic!("BEGIN: {error:?}"));
        two.execute_batch("PRAGMA locking_mode = exclusive; BEGIN IMMEDIATE; UPDATE t SET n = 5")
            .unwrap_or_else(|error| panic!("the other connection's write: {error:?}"));
        assert!(
            one.execute("UPDATE t SET n = 9", &[]).is_err(),
            "the write waits for the other connection, and with no timeout it is refused"
        );
        one.execute_batch("PRAGMA busy_timeout = 1000")
            .unwrap_or_else(|error| panic!("PRAGMA busy_timeout was refused: {error:?}"));
        one.execute_batch("PRAGMA busy_timeout = 0")
            .unwrap_or_else(|error| panic!("PRAGMA busy_timeout was refused: {error:?}"));
        one.execute_batch("ROLLBACK")
            .unwrap_or_else(|error| panic!("ROLLBACK was refused: {error:?}"));
        assert!(
            matches!(one.in_transaction(), Ok(false)),
            "the ROLLBACK ended the transaction"
        );
        two.execute_batch("COMMIT")
            .unwrap_or_else(|error| panic!("the other connection's COMMIT: {error:?}"));
        drop(b);
        one.execute("UPDATE t SET n = 7", &[])
            .unwrap_or_else(|error| panic!("the autocommit UPDATE: {error:?}"));
    }
    assert_eq!(
        stored_n(&path),
        Some(Value::Integer(7)),
        "an UPDATE that reported success after the ROLLBACK is in the file"
    );
}

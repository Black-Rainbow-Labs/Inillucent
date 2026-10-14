//! A rolled back `DROP` of a table that a module serves gives the table its module back.
//!
//! Invariant: **after `ROLLBACK` undoes a `DROP TABLE` or a `DROP INDEX`, every
//! virtual table and vector index the catalog names again is connected on this
//! connection**, so the statements after the rollback read and write it as
//! they did before the `DROP`.
//!
//! ## The defect
//!
//! The rollback put the catalog rows and the trees back, but the connection's
//! map of connected modules is memory, and nothing reconnected what the `DROP`
//! had disconnected. An FTS5 table then refused every statement with "no module
//! is connected for virtual table". A table with a vector index lost the index
//! from this connection: rows inserted after the rollback were written to the
//! table and not to the index, so after a reopen the index answered without
//! them, and `PRAGMA integrity_check` still said `ok`. Found by bug hunt 10.

use std::path::PathBuf;

use inillucent_compat::workspace_root;
use inillucent_engine::connect::{Connection, Database};
use inillucent_engine::OwnedDatum;

/// Returns a fresh database file for one case.
///
/// @param name - the case's name, which is also its directory
fn fresh(name: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output/rollback-restores-modules")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("the case's directory was not made: {error}"));
    directory.join("modules.rdb")
}

/// Runs a query and returns its first column as integers, one per row.
///
/// @param session - the connection
/// @param sql - the query
fn integers(session: &Connection<'_>, sql: &str) -> Vec<i64> {
    let mut statement = session
        .prepare(sql)
        .unwrap_or_else(|error| panic!("{sql} did not prepare: {error}"));
    let mut found = Vec::new();
    while statement
        .step()
        .unwrap_or_else(|error| panic!("{sql} failed: {error}"))
    {
        match statement.row().first() {
            Some(OwnedDatum::Int(value)) => found.push(*value),
            other => panic!("{sql} returned {other:?} where an integer was expected"),
        }
    }
    found
}

/// Runs statements that must succeed.
///
/// @param session - the connection
/// @param sql - the statements
fn exec(session: &Connection<'_>, sql: &str) {
    session
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql} failed: {error}"));
}

/// The nearest rows to `[0, 0, 1]`, by the vector index.
const NEAREST: &str = "SELECT id FROM d ORDER BY vector_distance_cos(e, '[0, 0, 1]') LIMIT 3";

/// A row inserted after a rolled back `DROP TABLE` reaches the table's vector index.
#[test]
fn a_vector_index_takes_rows_written_after_a_rolled_back_drop_table() {
    let path = fresh("drop-table");
    {
        let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
        let session = database.session();
        exec(
            &session,
            "CREATE TABLE d (id INTEGER PRIMARY KEY, e VECTOR(3));
             INSERT INTO d VALUES (1, '[1, 0, 0]'), (2, '[0, 1, 0]');
             CREATE INDEX di ON d USING inillucent_hnsw (e);
             BEGIN; DROP TABLE d; ROLLBACK;
             INSERT INTO d VALUES (3, '[0, 0, 1]');",
        );
        assert_eq!(integers(&session, NEAREST).first(), Some(&3));
    }
    let database = Database::open(&path).unwrap_or_else(|error| panic!("reopen: {error}"));
    let session = database.session();
    assert_eq!(
        integers(&session, NEAREST).first(),
        Some(&3),
        "the row written after the rollback is missing from the vector index"
    );
}

/// A row inserted after a rolled back `DROP INDEX` reaches the vector index.
#[test]
fn a_vector_index_takes_rows_written_after_a_rolled_back_drop_index() {
    let path = fresh("drop-index");
    {
        let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
        let session = database.session();
        exec(
            &session,
            "CREATE TABLE d (id INTEGER PRIMARY KEY, e VECTOR(3));
             INSERT INTO d VALUES (1, '[1, 0, 0]'), (2, '[0, 1, 0]');
             CREATE INDEX di ON d USING inillucent_hnsw (e);
             BEGIN; DROP INDEX di; ROLLBACK;
             INSERT INTO d VALUES (3, '[0, 0, 1]');",
        );
    }
    let database = Database::open(&path).unwrap_or_else(|error| panic!("reopen: {error}"));
    let session = database.session();
    assert_eq!(
        integers(&session, NEAREST).first(),
        Some(&3),
        "the row written after the rollback is missing from the vector index"
    );
}

/// An FTS5 table answers on the same connection after a rolled back `DROP TABLE`.
#[test]
fn a_full_text_table_answers_after_a_rolled_back_drop_table() {
    let path = fresh("fts5");
    let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
    let session = database.session();
    exec(
        &session,
        "CREATE VIRTUAL TABLE f USING fts5(x);
         INSERT INTO f VALUES ('hello world');
         BEGIN; DROP TABLE f; ROLLBACK;
         INSERT INTO f VALUES ('hello again');
         BEGIN; SAVEPOINT s; DROP TABLE f; ROLLBACK TO s; COMMIT;",
    );
    assert_eq!(
        integers(&session, "SELECT count(*) FROM f WHERE f MATCH 'hello'"),
        vec![2]
    );
}

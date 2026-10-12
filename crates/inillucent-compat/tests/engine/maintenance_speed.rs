//! Maintenance statements an application runs on a schedule finish in time
//! that grows with the data, at a rate near SQLite's.
//!
//! Invariant: **`VACUUM` and `VACUUM INTO` copy a table's rows in one
//! transaction, not one transaction per row.**
//!
//! ## Why this exists
//!
//! The rebuild behind `VACUUM` inserted each row as its own statement outside
//! any transaction, so each row was a commit with its own log syncs. 4,000 rows
//! took 12 seconds against SQLite's 24 milliseconds, 20,000 rows 72 seconds,
//! and 100,000 rows with two indexes had not finished after three minutes. A
//! maintenance job runs `VACUUM` after a large delete, which is when the table
//! is largest. Found by the bug hunt's speed and two process hunters.
//!
//! The limits are wide on purpose: they are about ten times what the fixed
//! code takes on a loaded machine, and several times less than the old code
//! took, so they tell the two apart without depending on the machine.

use std::time::{Duration, Instant};

use inillucent_compat::workspace_root;
use inillucent_driver::{Database, Value};

/// Opens a fresh database holding `rows` rows in a table with two indexes.
///
/// @param name - the case's name, which is also its directory's
/// @param rows - how many rows to insert
fn filled(name: &str, rows: i64) -> (std::path::PathBuf, Database) {
    let directory = workspace_root()
        .join("_agent_output/maintenance-speed")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let path = directory.join("case.rdb");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    session
        .execute_batch(&format!(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, b TEXT); \
             CREATE INDEX ta ON t(a); CREATE INDEX tb ON t(b); \
             INSERT INTO t SELECT value, value % 97, 'row ' || value FROM generate_series(1, {rows});"
        ))
        .expect("the table fills");
    (directory, database)
}

/// Returns one integer a query answers.
///
/// @param database - the database
/// @param sql - a query returning one integer
fn integer(database: &Database, sql: &str) -> i64 {
    let rows = database
        .session()
        .query_all(sql, &[])
        .expect("the query runs");
    match rows.rows.first().and_then(|row| row.first()) {
        Some(Value::Integer(n)) => *n,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// `VACUUM` of 20,000 rows with two indexes keeps every row and finishes in
/// seconds.
#[test]
fn vacuum_copies_rows_in_one_transaction() {
    let (_, database) = filled("vacuum", 20_000);
    database
        .session()
        .execute("DELETE FROM t WHERE id % 2 = 0", &[])
        .expect("half the rows are deleted");
    let started = Instant::now();
    database
        .session()
        .execute("VACUUM", &[])
        .expect("VACUUM runs");
    let took = started.elapsed();
    assert_eq!(integer(&database, "SELECT count(*) FROM t"), 10_000);
    assert_eq!(
        integer(&database, "SELECT count(*) FROM t WHERE a = 5"),
        104
    );
    assert!(
        took < Duration::from_secs(10),
        "VACUUM of 10,000 rows took {took:?}; each row was its own transaction"
    );
}

/// `VACUUM INTO` of 20,000 rows writes a complete copy in seconds.
#[test]
fn vacuum_into_copies_rows_in_one_transaction() {
    let (directory, database) = filled("vacuum-into", 20_000);
    let copy = directory.join("copy.rdb");
    let started = Instant::now();
    database
        .session()
        .execute(
            &format!(
                "VACUUM INTO '{}'",
                copy.to_string_lossy().replace('\\', "/")
            ),
            &[],
        )
        .expect("VACUUM INTO runs");
    let took = started.elapsed();
    let copied = Database::open(&copy).expect("the copy opens");
    assert_eq!(integer(&copied, "SELECT count(*) FROM t"), 20_000);
    assert_eq!(
        integer(&copied, "SELECT count(*) FROM t WHERE b = 'row 777'"),
        1
    );
    assert!(
        took < Duration::from_secs(10),
        "VACUUM INTO of 20,000 rows took {took:?}; each row was its own transaction"
    );
}

//! `PRAGMA max_page_count` stops a write that would grow the file past it.
//!
//! Invariant: **a statement that needs a page past the ceiling fails with
//! SQLite's `SQLITE_FULL` and its message, takes no page, and undoes itself
//! while the open transaction stays open; a write that fits in the pages the
//! file already holds still succeeds.**
//!
//! ## Why this exists
//!
//! The pragma was accepted and reported and changed nothing, so a write past it
//! succeeded where SQLite answers "database or disk is full". An application
//! that uses the ceiling as a quota or a cache size relies on the refusal.
//! Found by the bug hunt's SQLite fix mining.

use inillucent_compat::workspace_root;
use inillucent_driver::{Database, Status, Value};

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

/// Opens a fresh database in its own directory, with one small table of 10 rows.
///
/// @param name - the case's name, which is also its directory's
fn fresh(name: &str) -> Database {
    let directory = workspace_root()
        .join("_agent_output/max-page-count")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("quota.rdb")).expect("the database opens");
    database
        .session()
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, body TEXT); \
             INSERT INTO t(body) SELECT 'row ' || value FROM generate_series(1, 10);",
        )
        .expect("the table is made");
    database
}

/// Sets the ceiling to the pages the file holds now, and returns that count.
///
/// SQLite raises a ceiling below the page count to the page count, so asking
/// for 1 sets it to exactly what the file holds.
///
/// @param database - the database
fn hold_at_the_current_size(database: &Database) -> i64 {
    let held = integer(database, "PRAGMA page_count");
    assert_eq!(integer(database, "PRAGMA max_page_count = 1"), held);
    assert_eq!(integer(database, "PRAGMA max_page_count"), held);
    held
}

/// A large insert past the ceiling fails with `full` and SQLite's message, and
/// the table and the file are as they were.
#[test]
fn a_write_past_the_ceiling_fails_and_takes_nothing() {
    let database = fresh("refused");
    let held = hold_at_the_current_size(&database);
    let refused = database
        .session()
        .execute_batch(
            "INSERT INTO t(body) SELECT printf('%.*c', 2000, 'x') FROM generate_series(1, 5000);",
        )
        .expect_err("growing past the ceiling is refused");
    assert_eq!(refused.status, Status::Full, "{refused:?}");
    assert_eq!(refused.message, "database or disk is full");
    assert_eq!(integer(&database, "SELECT count(*) FROM t"), 10);
    assert_eq!(integer(&database, "PRAGMA page_count"), held);
    assert_eq!(integer(&database, "PRAGMA freelist_count"), 0);
}

/// Inside a transaction the refusal undoes the statement and keeps the
/// transaction and its earlier writes, as SQLite does for a statement of many
/// rows.
#[test]
fn a_refusal_inside_a_transaction_undoes_only_the_statement() {
    let database = fresh("transaction");
    hold_at_the_current_size(&database);
    let session = database.session();
    session
        .execute_batch("BEGIN; INSERT INTO t(body) VALUES('kept until the refusal');")
        .expect("the small insert fits");
    let refused = session
        .execute_batch(
            "INSERT INTO t(body) SELECT printf('%.*c', 2000, 'x') FROM generate_series(1, 5000);",
        )
        .expect_err("growing past the ceiling is refused");
    assert_eq!(refused.status, Status::Full, "{refused:?}");
    let still_open = session
        .execute_batch("BEGIN;")
        .expect_err("the transaction is still open");
    assert_eq!(
        still_open.message,
        "cannot start a transaction within a transaction"
    );
    session
        .execute_batch("INSERT INTO t(body) VALUES('after'); COMMIT;")
        .expect("the open transaction takes more and commits");
    assert_eq!(integer(&database, "SELECT count(*) FROM t"), 12);
}

/// A write that fits in the pages the file already holds succeeds at the
/// ceiling, and raising the ceiling lets the refused write through.
#[test]
fn a_write_that_fits_succeeds_and_a_raised_ceiling_lets_more_in() {
    let database = fresh("raised");
    hold_at_the_current_size(&database);
    database
        .session()
        .execute_batch("INSERT INTO t(body) VALUES('small');")
        .expect("a row that fits the last leaf is written");
    assert_eq!(
        integer(&database, "PRAGMA max_page_count = 100000"),
        100_000
    );
    database
        .session()
        .execute_batch(
            "INSERT INTO t(body) SELECT printf('%.*c', 2000, 'x') FROM generate_series(1, 5000);",
        )
        .expect("the raised ceiling lets the write through");
    assert_eq!(integer(&database, "SELECT count(*) FROM t"), 5_011);
    let check = database
        .session()
        .query_all("PRAGMA integrity_check", &[])
        .expect("the check runs");
    assert_eq!(check.rows, vec![vec![Value::Text("ok".to_string())]]);
}

/// A value that is not a positive integer leaves the ceiling where it was, as
/// in SQLite. It used to lower the ceiling to the page count.
#[test]
fn a_ceiling_of_zero_or_less_or_not_a_number_is_ignored() {
    let database = fresh("ignored");
    assert_eq!(integer(&database, "PRAGMA max_page_count = 1000"), 1_000);
    for asked in ["0", "-5", "'xyz'"] {
        let sql = format!("PRAGMA max_page_count = {asked}");
        assert_eq!(integer(&database, &sql), 1_000, "{sql}");
    }
    assert_eq!(integer(&database, "PRAGMA max_page_count"), 1_000);
}

/// An index build and a long value at the ceiling fail with `full`, and the
/// failed statements leave no page behind.
#[test]
fn a_ceiling_counts_an_index_build_and_a_long_value() {
    let database = fresh("index_and_chain");
    database
        .session()
        .execute_batch(
            "INSERT INTO t(body) SELECT printf('%.*c', 500, 'x') FROM generate_series(1, 3000);",
        )
        .expect("the table grows before the ceiling is set");
    let held = hold_at_the_current_size(&database);
    let free = integer(&database, "PRAGMA freelist_count");
    let index = database
        .session()
        .execute_batch("CREATE INDEX t_body ON t(body);")
        .expect_err("an index needs pages past the ceiling");
    assert_eq!(index.status, Status::Full, "{index:?}");
    let value = database
        .session()
        .execute_batch("INSERT INTO t(body) VALUES (printf('%.*c', 200000, 'y'));")
        .expect_err("a long value needs pages past the ceiling");
    assert_eq!(value.status, Status::Full, "{value:?}");
    assert_eq!(integer(&database, "PRAGMA page_count"), held);
    assert_eq!(integer(&database, "PRAGMA freelist_count"), free);
    assert_eq!(integer(&database, "SELECT count(*) FROM t"), 3_010);
}

/// A ceiling on an attached file does not bind the main file, a second
/// connection starts at the default, and a reopen is back to the default.
#[test]
fn a_ceiling_is_held_per_file_and_per_connection() {
    let database = fresh("per_file");
    let other = workspace_root().join("_agent_output/max-page-count/per_file/other.rdb");
    let text = other.to_string_lossy().replace('\'', "''");
    database
        .session()
        .execute_batch(&format!(
            "ATTACH '{text}' AS other; CREATE TABLE other.u(id INTEGER PRIMARY KEY, body TEXT);"
        ))
        .expect("the file attaches");
    let held = integer(&database, "PRAGMA other.page_count");
    assert_eq!(integer(&database, "PRAGMA other.max_page_count = 1"), held);
    assert_eq!(
        integer(&database, "PRAGMA main.max_page_count"),
        4_294_967_294
    );
    let refused = database
        .session()
        .execute_batch(
            "INSERT INTO other.u(body) SELECT printf('%.*c', 2000, 'x') FROM generate_series(1, 5000);",
        )
        .expect_err("the attached file is at its ceiling");
    assert_eq!(refused.status, Status::Full, "{refused:?}");
    database
        .session()
        .execute_batch(
            "INSERT INTO t(body) SELECT printf('%.*c', 2000, 'x') FROM generate_series(1, 500);",
        )
        .expect("the main file has no ceiling in force");
    let second =
        Database::open(workspace_root().join("_agent_output/max-page-count/per_file/quota.rdb"))
            .expect("a second connection opens");
    assert_eq!(integer(&second, "PRAGMA max_page_count"), 4_294_967_294);
}

/// A sort larger than the pool spills to its own temporary storage, so a ceiling
/// at the page count does not stop it.
#[test]
fn a_sort_that_spills_under_a_tight_limit_still_succeeds() {
    let database = fresh("sort_spill");
    database
        .session()
        .execute_batch(
            "INSERT INTO t(body) SELECT printf('%.*c', 300, 'x') || value FROM generate_series(1, 30000);",
        )
        .expect("the table grows before the ceiling is set");
    hold_at_the_current_size(&database);
    assert_eq!(
        integer(
            &database,
            "SELECT count(*) FROM (SELECT body FROM t ORDER BY body DESC)"
        ),
        30_010
    );
    let rows = database
        .session()
        .query_all("SELECT body FROM t ORDER BY body DESC", &[])
        .expect("the sort runs under the ceiling");
    assert_eq!(rows.rows.len(), 30_010);
}

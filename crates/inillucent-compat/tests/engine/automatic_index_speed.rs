//! Statements that compare two unindexed tables finish in time that grows with the rows.
//!
//! Invariant: **a correlated subquery or a join whose only way to reach the rows is a table
//! with no index on the compared column reads that table once, not once for every outer row.**
//!
//! ## Why this exists
//!
//! SQLite builds a transient index for an unindexed join column and for the table of an `EXISTS`
//! it rewrites into a loop, and this engine built one for neither shape below. At 10,000 rows
//! in each of two tables, measured against the pinned SQLite 3.53.4:
//!
//! | statement | before | SQLite |
//! |---|---|---|
//! | `SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k)` | 1.219 s | 0.0045 s |
//! | `SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k)) FROM a` | 2.185 s | 3.498 s |
//! | `... FROM a JOIN b ON a.k = b.k WHERE b.id % 2 = 0` | 2.836 s | 0.0034 s |
//! | `... FROM a JOIN b ON a.k = b.k + 1` | 2.207 s | 0.0044 s |
//! | `... FROM a JOIN b ON a.k = b.k AND a.id < b.id` | 2.001 s | 0.0056 s |
//!
//! The first reads `b` once for each of the 10,000 rows of `a`. The three joins are the shapes
//! the join's automatic index declined: a condition with something beside equalities in it, and
//! an equality with an expression on one side.
//!
//! The limits are wide on purpose: they are about ten times what the fixed code takes on a
//! loaded machine, and several times less than the old code took, so they tell the two apart
//! without depending on the machine.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use inillucent_base::budget::{self, Limits};
use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database holding `a` and `b`, unindexed on `k`.
///
/// `a.k` is the row number modulo 10,000 and `b.k` is seven times the row number modulo 10,000,
/// which visits every value once when there are 10,000 rows.
///
/// @param name - the case's name, which is also its directory's
/// @param rows - how many rows each table holds
fn tables(name: &str, rows: u32) -> Database {
    let directory = workspace_root()
        .join("_agent_output/automatic-index-speed")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    database
        .session()
        .execute_batch(&format!(
            "CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad TEXT); \
             CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, pad TEXT); \
             INSERT INTO a SELECT value, value % 10000, 'aaaaaaaaaaaaaaaaaaaa' \
                 FROM generate_series(1, {rows}); \
             INSERT INTO b SELECT value, (value * 7) % 10000, 'bbbbbbbbbbbbbbbbbbbb' \
                 FROM generate_series(1, {rows});"
        ))
        .expect("the tables are built");
    database
}

/// Returns the one integer a query answers.
///
/// @param database - the database
/// @param sql - a query answering one integer
fn integer(database: &Database, sql: &str) -> i64 {
    let rows = database.session().query(sql).expect("the query runs");
    match rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(number)) => *number,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Runs each query, checks its answer, and returns how long all of them took.
///
/// @param database - the database
/// @param cases - each query with the answer SQLite gives
fn timed(database: &Database, cases: &[(&str, i64)]) -> Duration {
    let started = Instant::now();
    let answers: Vec<i64> = cases
        .iter()
        .map(|(sql, _)| integer(database, sql))
        .collect();
    let took = started.elapsed();
    for ((sql, expected), answer) in cases.iter().zip(answers) {
        assert_eq!(answer, *expected, "{sql}");
    }
    took
}

/// A correlated `EXISTS` and a correlated `count(*)` read `b` once.
///
/// The old code scanned `b` for every row of `a`: 1.2 s and 2.2 s at 10,000 rows.
#[test]
fn correlated_subqueries_over_an_unindexed_column_finish_in_time() {
    let database = tables("correlated", 10_000);
    let took = timed(
        &database,
        &[
            (
                "SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k)",
                10_000,
            ),
            (
                "SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k)) FROM a",
                10_000,
            ),
        ],
    );
    assert!(
        took < Duration::from_millis(250),
        "a correlated EXISTS and count(*) over 10,000 rows took {took:?}; each outer row scanned b"
    );
}

/// A correlated block keyed on an expression of the outer row reads `b` once.
///
/// Only a bare outer column was a key, so `b.k = a.k + 1` scanned `b` for every row of `a`. At
/// 20,000 rows each the command line built with the workspace's optimised dev profile took
/// 11.0 s for the `EXISTS` and 41.2 s for `b.n = lower(a.name)`, against 77 ms for `b.k = a.k`.
#[test]
fn correlated_subqueries_keyed_on_an_outer_expression_finish_in_time() {
    let database = tables("correlated-expression", 10_000);
    let took = timed(
        &database,
        &[
            (
                "SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k + 1)",
                9_999,
            ),
            (
                "SELECT sum((SELECT count(*) FROM b WHERE b.k = (a.k * 3) % 10000)) FROM a",
                10_000,
            ),
        ],
    );
    assert!(
        took < Duration::from_millis(250),
        "two correlated blocks keyed on an outer expression over 10,000 rows took {took:?}; \
         each outer row scanned b"
    );
}

/// A correlated `NOT EXISTS` is answered from the table too.
///
/// SQLite keeps a subquery node for it and scans, taking 1.7 s at 10,000 rows.
#[test]
fn a_correlated_not_exists_finishes_in_time() {
    let database = tables("not-exists", 10_000);
    let took = timed(
        &database,
        &[(
            "SELECT count(*) FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k)",
            0,
        )],
    );
    assert!(
        took < Duration::from_millis(150),
        "a correlated NOT EXISTS over 10,000 rows took {took:?}; each outer row scanned b"
    );
}

/// With `PRAGMA automatic_index = OFF` the same statement scans, and gives the same answer.
///
/// 2,000 rows in each table keep the scanning arm short. SQLite answers 571 for both.
#[test]
fn the_scanning_arm_gives_the_same_answer() {
    let database = tables("scanning", 2_000);
    let session = database.session();
    let sql = "SELECT count(*) FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k)";
    assert_eq!(integer(&database, sql), 571);
    session
        .execute_batch("PRAGMA automatic_index = OFF;")
        .expect("the pragma runs");
    assert_eq!(integer(&database, sql), 571);
    session
        .execute_batch("PRAGMA automatic_index = ON;")
        .expect("the pragma runs");
    assert_eq!(integer(&database, sql), 571);
}

/// A byte budget too small for the table makes the block scan, and the statement succeeds.
///
/// Before this the block materialised nothing, so a statement that ran slowly and fitted a
/// served budget would start failing with `bytes` if the table's refusal were passed on.
#[test]
fn a_budget_too_small_for_the_table_makes_the_block_scan() {
    let database = tables("budget", 2_000);
    let guard = budget::arm(
        Limits::unbounded().with_bytes(Some(10_000)),
        Arc::new(AtomicBool::new(false)),
    );
    let sum = integer(
        &database,
        "SELECT sum((SELECT count(*) FROM b WHERE b.k = a.k)) FROM a",
    );
    let spent = budget::spent();
    drop(guard);
    assert_eq!(sum, 571);
    assert!(
        spent.1 < 10_000,
        "the refused table was charged to the budget: {} bytes",
        spent.1
    );
}

/// The three joins whose automatic index the key search used to decline finish in time.
///
/// The condition holds a predicate on `b` beside the equality, an expression on the `b` side, and
/// a comparison between the two tables beside the equality.
#[test]
fn joins_with_more_than_a_plain_equality_finish_in_time() {
    let database = tables("joins", 10_000);
    let took = timed(
        &database,
        &[
            (
                "SELECT count(*) FROM a JOIN b ON a.k = b.k WHERE b.id % 2 = 0",
                5_000,
            ),
            ("SELECT count(*) FROM a JOIN b ON a.k = b.k + 1", 9_999),
            (
                "SELECT count(*) FROM a JOIN b ON a.k = b.k AND a.id < b.id",
                4_999,
            ),
        ],
    );
    assert!(
        took < Duration::from_millis(350),
        "three joins over 10,000 rows took {took:?}; each outer row walked the whole inner table"
    );
}

//! Write statements finish in time that grows with the work they do.
//!
//! Invariant: **a write's cost depends on the rows it touches, not on how many
//! other rows share its leaf, how many other leaves are part empty, or how many
//! other tables the schema holds.**
//!
//! ## Why this exists
//!
//! The bug hunt's speed hunters found five statements that were 10 to 100 times
//! slower than SQLite, each for a cost that grew with something the statement did
//! not touch:
//!
//! - Updating a row that was written since its leaf was last packed rebuilt the
//!   whole delta area of the leaf. 40,000 upserts onto 1,000 keys took 952 ms
//!   against SQLite's 23 ms, while 10 keys and 40,000 keys were both fast.
//! - A leaf below half full tried to merge into its right sibling on every delete,
//!   and each attempt packed both leaves. A cascading delete of 100,000 children
//!   scattered over the table made 8,569 attempts for 50,000 deletes.
//! - Every `CREATE TABLE` synced the data file, and parsed and copied every table
//!   already in the schema. 1,600 tables in one transaction took 6.8 s against 0.1 s.
//! - A `WHERE` term that reads one table was tested after the join, once for every
//!   pair of rows, instead of before it.
//! - A line of 20,000 statements gave each statement a copy of the rest of the line.
//!
//! The limits are wide on purpose: they are about ten times what the fixed code
//! takes on a loaded machine, and several times less than the old code took, so
//! they tell the two apart without depending on the machine. Where a count says
//! the same thing it is used instead of a clock.

use std::time::{Duration, Instant};

use inillucent_compat::cliproc;
use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database in a directory named for the case.
///
/// @param name - the case's name, which is also its directory's
fn fresh(name: &str) -> (std::path::PathBuf, Database) {
    let directory = workspace_root()
        .join("_agent_output/write-speed")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    (directory, database)
}

/// Returns the integers in the first row a query answers.
///
/// @param database - the database
/// @param sql - a query returning integers
fn integers(database: &Database, sql: &str) -> Vec<i64> {
    let rows = database.session().query(sql).expect("the query runs");
    let first = rows.first().expect("the query answers a row");
    first
        .iter()
        .map(|value| match value {
            OwnedDatum::Int(n) => *n,
            other => panic!("{sql} answered {other:?}"),
        })
        .collect()
}

/// 40,000 upserts onto 1,000 keys leave 1,000 rows each updated 40 times, and do not copy the
/// leaf's delta area once per upsert.
///
/// The 1,000 rows are written after the table is created, so they sit in the delta area of the
/// leaf and nothing runs out of room. Each update of an integer to an integer is the same length
/// as the row it replaces, so it is written over it.
#[test]
fn upserts_onto_a_thousand_keys_finish_in_time() {
    let (_, database) = fresh("upsert");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE k(id INTEGER PRIMARY KEY, n INT);")
        .expect("the table is made");
    let started = Instant::now();
    session
        .execute_batch(
            "INSERT INTO k SELECT value % 1000, 1 FROM generate_series(1, 40000) WHERE true \
             ON CONFLICT(id) DO UPDATE SET n = n + 1;",
        )
        .expect("the upserts run");
    let took = started.elapsed();
    assert_eq!(
        integers(&database, "SELECT count(*), sum(n), min(n), max(n) FROM k"),
        vec![1000, 40_000, 40, 40]
    );
    assert!(
        took < Duration::from_millis(2_000),
        "40,000 upserts onto 1,000 keys took {took:?}; each one rebuilt the delta area of its leaf"
    );
}

/// 40,000 `INSERT OR REPLACE` statements onto 1,000 keys leave 1,000 rows with the last value
/// written to each.
#[test]
fn replaces_onto_a_thousand_keys_finish_in_time() {
    let (_, database) = fresh("replace");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE k(id INTEGER PRIMARY KEY, n INT);")
        .expect("the table is made");
    let started = Instant::now();
    session
        .execute_batch(
            "INSERT OR REPLACE INTO k SELECT value % 1000, value FROM generate_series(1, 40000);",
        )
        .expect("the replaces run");
    let took = started.elapsed();
    assert_eq!(
        integers(&database, "SELECT count(*), sum(n), min(n), max(n) FROM k"),
        vec![1000, 39_500_500, 39_001, 40_000]
    );
    assert!(
        took < Duration::from_millis(2_500),
        "40,000 replaces onto 1,000 keys took {took:?}; each one rebuilt the delta area of its leaf"
    );
}

/// A cascading delete whose children are scattered over the child table does not pack two
/// leaves together on every delete.
///
/// The children of one parent are 1,000 rowids apart, so each parent's delete takes a few rows
/// out of every leaf of the child table, and every leaf spends most of the statement below half
/// full next to a sibling that is also part empty. A merge attempt packs both leaves, so one per
/// delete cost more than the delete.
#[test]
fn cascading_delete_of_scattered_children_does_not_retry_merges() {
    let (_, database) = fresh("cascade");
    let session = database.session();
    session
        .execute_batch(
            "PRAGMA foreign_keys = ON; CREATE TABLE par(id INTEGER PRIMARY KEY); \
             CREATE TABLE ch(id INTEGER PRIMARY KEY, pid INT REFERENCES par(id) ON DELETE CASCADE); \
             CREATE INDEX chp ON ch(pid); \
             INSERT INTO par SELECT value FROM generate_series(1, 1000); \
             INSERT INTO ch SELECT value, value % 1000 + 1 FROM generate_series(1, 100000);",
        )
        .expect("the tables are built");
    let before = database.write_stats();
    session
        .execute_batch("DELETE FROM par WHERE id <= 250;")
        .expect("the cascade runs");
    let after = database.write_stats();
    assert_eq!(
        integers(
            &database,
            "SELECT (SELECT count(*) FROM par), (SELECT count(*) FROM ch)"
        ),
        vec![750, 75_000]
    );
    assert_eq!(after.deleted - before.deleted, 250 + 25_000 + 25_000);
    let attempts = after.merge_attempts - before.merge_attempts;
    assert!(
        attempts < 500,
        "{attempts} merge attempts for 50,250 deletes; the old code made 8,569"
    );
}

/// 1,200 `CREATE TABLE` statements in one transaction cost the same each, and the tables are
/// all there after the commit and after a reopen.
///
/// Each statement used to sync the data file and to parse and copy every table already in the
/// schema.
#[test]
fn many_create_tables_in_one_transaction_finish_in_time() {
    let (directory, database) = fresh("tables");
    let session = database.session();
    session
        .execute_batch("BEGIN;")
        .expect("the transaction opens");
    let started = Instant::now();
    for table in 0..1_200 {
        session
            .execute_batch(&format!(
                "CREATE TABLE m{table}(id INTEGER PRIMARY KEY, v INT, s TEXT);"
            ))
            .expect("the table is made");
    }
    session
        .execute_batch("INSERT INTO m0 VALUES (1, 10, 'first'); INSERT INTO m1199 VALUES (2, 20, 'last'); COMMIT;")
        .expect("the transaction commits");
    let took = started.elapsed();
    assert_eq!(
        integers(
            &database,
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table'"
        ),
        vec![1_200]
    );
    drop(database);
    let reopened = Database::open(directory.join("case.rdb")).expect("the file reopens");
    assert_eq!(
        integers(
            &reopened,
            "SELECT (SELECT count(*) FROM sqlite_schema), (SELECT v FROM m0), (SELECT v FROM m1199)"
        ),
        vec![1_200, 10, 20]
    );
    assert!(
        took < Duration::from_millis(3_000),
        "1,200 CREATE TABLE statements took {took:?}; each one touched every table before it"
    );
}

/// A `WHERE` term that reads one table of a join is tested before the join, so a join of two
/// 20,000 row tables with a term on each pairs 20 rows with 20,000, not 20,000 with 20,000.
///
/// `a` is `value % 1000`, so 20 rows of each table have `a = 1` and 20 have `a = 0`: 400 pairs.
#[test]
fn a_term_on_one_table_of_a_join_filters_before_the_join() {
    let (_, database) = fresh("join");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE t(a, b, c); \
             INSERT INTO t SELECT value % 1000, value % 77, value FROM generate_series(1, 20000);",
        )
        .expect("the table is built");
    let started = Instant::now();
    let both = integers(
        &database,
        "SELECT count(*) FROM t x, t y WHERE x.a = 1 AND y.a = 0",
    );
    let joined = integers(
        &database,
        "SELECT count(*) FROM t x JOIN t y ON x.c = y.c + 1 WHERE x.a = 1 AND y.a = 0",
    );
    let took = started.elapsed();
    assert_eq!(both, vec![400]);
    assert_eq!(joined, vec![19]);
    assert!(
        took < Duration::from_millis(3_000),
        "the two joins took {took:?}; the term on x was tested for every pair"
    );
}

/// A line of 20,000 statements runs in time proportional to its length.
///
/// The shell cut the line statement by statement and copied everything after each statement to
/// show in an error message, so the copies added up to the square of the line's length: 3.2 s
/// against 0.05 s when the statements are on separate lines.
#[test]
fn twenty_thousand_statements_on_one_line_finish_in_time() {
    let shell = cliproc::program("inillucent-shell");
    let mut script = String::from("CREATE TABLE one(a INTEGER, b TEXT);\nBEGIN;");
    for row in 0..20_000 {
        script.push_str(&format!("INSERT INTO one VALUES ({row}, 'row {row}');"));
    }
    script.push_str("COMMIT;\nSELECT count(*), sum(a), max(b) FROM one;\n");
    let started = Instant::now();
    let ran = cliproc::run_with_input(&shell, &[":memory:"], &script);
    let took = started.elapsed();
    assert_eq!(
        ran.stdout.trim(),
        "20000|199990000|row 9999",
        "{}",
        ran.said()
    );
    assert!(
        took < Duration::from_millis(1_500),
        "20,000 statements on one line took {took:?}; each one copied the rest of the line"
    );
}

//! An index over a column that holds values of 3,000 bytes, at a 4 KiB page size.
//!
//! Invariant: **an index key wider than a quarter of the page is stored out of
//! line, so an interior page always holds several separators, and an ordinary
//! insert, update or delete in the table never fails because the index tree grew
//! taller than the descent limit.** The failure this guards was reported as
//! "a descent walked more than 64 levels", status `corrupt`, from a script of 600
//! single row inserts and a `DELETE FROM` on a database with 4,096 byte pages. SQLite
//! stores such a key in overflow pages, so the same schema works there at any
//! page size.
//!
//! Every case builds the database on `MemoryVfs`, so the cases run in seconds and
//! reopen the file before they read the rows back.

use std::sync::Arc;

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::{MemoryVfs, Vfs};

/// The page size the report used, and the smallest the engine's tests commonly run at.
const PAGE_SIZE: usize = 4_096;

/// How many frames the pool holds, as in the report.
const FRAMES: usize = 4_096;

/// One case's database: the file system it lives on and where.
struct Case {
    vfs: Arc<dyn Vfs>,
    path: std::path::PathBuf,
}

impl Case {
    /// Returns a case with nothing created yet.
    ///
    /// @param tag - what to call the database
    fn new(tag: &str) -> Case {
        Case {
            vfs: Arc::new(MemoryVfs::new()),
            path: std::env::temp_dir().join(format!("inillucent-long-index-keys-{tag}.rdb")),
        }
    }

    /// Creates the database at the report's page size.
    fn create(&self) -> ImportedDatabase {
        ImportedDatabase::create_on(Arc::clone(&self.vfs), self.path.clone(), PAGE_SIZE, FRAMES)
            .expect("the database is created in memory")
    }

    /// Opens the database again, as another process would find it.
    fn reopen(&self) -> ImportedDatabase {
        ImportedDatabase::open_on(Arc::clone(&self.vfs), self.path.clone(), PAGE_SIZE, FRAMES)
            .expect("the database reopens")
    }
}

/// Runs a statement of any kind and returns its rows, failing with the statement.
///
/// @param engine - the open connection
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) -> Vec<Vec<OwnedDatum>> {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|why| panic!("{}: {}", &sql[..sql.len().min(60)], why.message()))
        .rows
}

/// Returns the one integer a query answers with.
///
/// @param engine - the open connection
/// @param sql - a query returning one row of one integer
fn count(engine: &mut ImportedDatabase, sql: &str) -> i64 {
    match run(engine, sql).first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(value)) => *value,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Returns the text a row's `b` holds: 3,000 bytes for every fifth row, 200 otherwise.
///
/// The rows differ from the first byte, so the index holds distinct keys of the
/// two widths in an order the inserts do not follow.
///
/// @param row - the row's number
fn value_of(row: i64) -> String {
    let width = if row % 5 == 0 { 3_000 } else { 200 };
    let lead = format!("{:05}", (row * 7_919) % 100_003);
    format!("{lead}{}", "z".repeat(width - lead.len()))
}

/// Asserts the table and its index agree, and the file passes its own check.
///
/// @param engine - the open connection
/// @param expected - how many rows there should be
fn assert_rows(engine: &mut ImportedDatabase, expected: i64) {
    assert_eq!(
        count(engine, "SELECT count(*) FROM copied"),
        expected,
        "by table"
    );
    assert_eq!(
        count(
            engine,
            "SELECT count(*) FROM copied INDEXED BY copied_b WHERE b >= ''"
        ),
        expected,
        "by index"
    );
    let check = run(engine, "PRAGMA integrity_check");
    assert_eq!(check, vec![vec![OwnedDatum::Text(b"ok".to_vec())]]);
}

/// Creates the report's table and index and inserts its 600 rows in one transaction.
///
/// @param engine - the open connection
fn fill(engine: &mut ImportedDatabase) {
    run(engine, "PRAGMA journal_mode=wal");
    run(engine, "CREATE TABLE copied(a INTEGER PRIMARY KEY, b TEXT)");
    run(engine, "CREATE INDEX copied_b ON copied(b)");
    run(engine, "BEGIN");
    for row in 1..=600 {
        run(
            engine,
            &format!("INSERT INTO copied VALUES({row}, '{}')", value_of(row)),
        );
    }
    run(engine, "COMMIT");
}

/// The report's script: 600 inserts, then `DELETE FROM copied`.
#[test]
fn deleting_every_row_of_a_table_with_wide_index_keys_works() {
    let case = Case::new("delete-all");
    let mut engine = case.create();
    fill(&mut engine);
    assert_rows(&mut engine, 600);
    run(&mut engine, "DELETE FROM copied");
    assert_rows(&mut engine, 0);
    drop(engine);
    let mut engine = case.reopen();
    assert_rows(&mut engine, 0);
}

/// Inserting one row at a time, outside a transaction, and then updating the wide keys.
#[test]
fn single_row_inserts_and_updates_of_wide_index_keys_work() {
    let case = Case::new("autocommit");
    let mut engine = case.create();
    run(
        &mut engine,
        "CREATE TABLE copied(a INTEGER PRIMARY KEY, b TEXT)",
    );
    run(&mut engine, "CREATE INDEX copied_b ON copied(b)");
    for row in 1..=300 {
        run(
            &mut engine,
            &format!("INSERT INTO copied VALUES({row}, '{}')", value_of(row)),
        );
    }
    assert_rows(&mut engine, 300);
    for row in (5..=300).step_by(5) {
        run(
            &mut engine,
            &format!(
                "UPDATE copied SET b = '{}' WHERE a = {row}",
                value_of(row + 1_000)
            ),
        );
    }
    assert_rows(&mut engine, 300);
    run(&mut engine, "DELETE FROM copied WHERE a % 2 = 0");
    assert_rows(&mut engine, 150);
    drop(engine);
    let mut engine = case.reopen();
    assert_rows(&mut engine, 150);
}

/// Every key 3,000 bytes wide, which is the worst case for the interior pages.
#[test]
fn an_index_where_every_key_is_three_thousand_bytes_works() {
    let case = Case::new("all-wide");
    let mut engine = case.create();
    run(
        &mut engine,
        "CREATE TABLE copied(a INTEGER PRIMARY KEY, b TEXT)",
    );
    run(&mut engine, "CREATE INDEX copied_b ON copied(b)");
    run(&mut engine, "BEGIN");
    for row in 1..=400 {
        let lead = format!("{:05}", (row * 7_919) % 100_003);
        let text = format!("{lead}{}", "y".repeat(3_000 - lead.len()));
        run(
            &mut engine,
            &format!("INSERT INTO copied VALUES({row}, '{text}')"),
        );
    }
    run(&mut engine, "COMMIT");
    assert_rows(&mut engine, 400);
    run(&mut engine, "DELETE FROM copied");
    assert_rows(&mut engine, 0);
}

/// Keys that share 2,990 bytes and differ in the last ten, mixed with short ones.
///
/// A separator between two such neighbours cannot be shortened below about 3,000
/// bytes, which is the case where an interior page used to hold one separator.
#[test]
fn wide_keys_that_share_a_long_prefix_work() {
    let case = Case::new("shared-prefix");
    let mut engine = case.create();
    run(
        &mut engine,
        "CREATE TABLE copied(a INTEGER PRIMARY KEY, b TEXT)",
    );
    run(&mut engine, "CREATE INDEX copied_b ON copied(b)");
    run(&mut engine, "BEGIN");
    for row in 1..=500 {
        let text = if row % 5 == 0 {
            format!("{}{:010}", "q".repeat(2_990), (row * 7_919) % 100_003)
        } else {
            value_of(row)
        };
        run(
            &mut engine,
            &format!("INSERT INTO copied VALUES({row}, '{text}')"),
        );
    }
    run(&mut engine, "COMMIT");
    assert_rows(&mut engine, 500);
    run(&mut engine, "DELETE FROM copied WHERE a % 3 <> 0");
    assert_rows(&mut engine, 166);
    run(&mut engine, "DELETE FROM copied");
    assert_rows(&mut engine, 0);
    drop(engine);
    let mut engine = case.reopen();
    assert_rows(&mut engine, 0);
}

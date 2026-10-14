//! A table loaded in key order takes about the pages a bulk build gives it.
//!
//! Invariant: **rows that arrive at the right edge of a tree one statement at
//! a time leave its leaves nearly as full as the bulk builder leaves them, and
//! `VACUUM`, which loads its rows by appends, does not give back a looser
//! file than it was given.**
//!
//! ## Why this exists
//!
//! A leaf takes appended rows into its delta area, about 30 bytes for a row of
//! two integers, and packs them into the sorted region, about 12 bytes, only
//! when it is full. An append that found the leaf full split it at once and
//! kept the left half as packed, so a leaf kept the 40% of a page its delta
//! rows packed to. Twenty thousand rows of `(a INTEGER, b INTEGER)` took 25
//! pages in 500 row `INSERT ... VALUES` statements, and 10 as one
//! `INSERT ... SELECT`, which builds the tree in one pass. SQLite's file for
//! the same rows was 2.3 to 2.5 times smaller. `VACUUM` reloads each table in
//! chunks of 20,000 rows and only the first chunk of a table is built in one
//! pass, so a table of 2,000,000 rows came out of `VACUUM` at 1,898 pages from
//! 1,910. The bug hunt found it by comparing file sizes with SQLite.

use inillucent_compat::workspace_root;
use inillucent_driver::{Database, Value};

/// How many pages a loaded file may take beyond the bulk build's, as the
/// bulk build packs a leaf at 0.9 and an appended leaf is left between 0.6 and
/// 0.95 full.
const SLACK_PAGES: i64 = 4;

/// Returns a fresh path for one test's database, with nothing at it.
///
/// @param name - the case's name, which is also the file's
fn fresh(name: &str) -> std::path::PathBuf {
    let area = workspace_root().join("target/scratch/append-load-density");
    let _ = std::fs::create_dir_all(&area);
    let path = area.join(format!("{name}.rdb"));
    inillucent_base::testing::remove_database(&path);
    path
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

/// Returns the pages a file uses, which is its page count less its free pages.
///
/// @param database - the database
fn used_pages(database: &Database) -> i64 {
    integer(database, "PRAGMA page_count") - integer(database, "PRAGMA freelist_count")
}

/// Checks that the file is intact and holds the rows that were loaded.
///
/// @param database - the database
/// @param table - the table that was loaded
/// @param rows - how many rows it must hold
fn assert_intact(database: &Database, table: &str, rows: i64) {
    assert_eq!(
        integer(database, &format!("SELECT count(*) FROM {table}")),
        rows
    );
    let check = database
        .session()
        .query_all("PRAGMA integrity_check", &[])
        .expect("the check runs");
    assert_eq!(check.rows, vec![vec![Value::Text("ok".to_string())]]);
}

/// Returns the text of `INSERT` statements that load `rows` rows of two
/// integers, `per_statement` rows to a statement, in the order of `key`.
///
/// @param table - the table to load
/// @param rows - how many rows
/// @param per_statement - how many rows each statement carries
/// @param key - the key of the nth row
fn value_statements(
    table: &str,
    rows: i64,
    per_statement: i64,
    key: impl Fn(i64) -> i64,
) -> String {
    let mut text = String::new();
    let mut at = 0;
    while at < rows {
        let last = (at + per_statement).min(rows);
        let tuples: Vec<String> = (at..last)
            .map(|nth| format!("({}, {})", key(nth), nth % 7))
            .collect();
        text.push_str(&format!(
            "INSERT INTO {table} VALUES {};\n",
            tuples.join(", ")
        ));
        at = last;
    }
    text
}

/// Returns the pages the same `rows` rows take when `INSERT ... SELECT` builds
/// the tree in one pass, which is the density the appended load is held to.
///
/// @param name - the case's name
/// @param rows - how many rows
fn bulk_pages(name: &str, rows: i64) -> i64 {
    let database = Database::open(fresh(&format!("{name}-bulk"))).expect("the database opens");
    database
        .session()
        .execute_batch(&format!(
            "CREATE TABLE n(a INTEGER, b INTEGER); \
             WITH RECURSIVE s(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM s WHERE i < {}) \
             INSERT INTO n SELECT i, i % 7 FROM s;",
            rows - 1
        ))
        .expect("the bulk load runs");
    assert_intact(&database, "n", rows);
    used_pages(&database)
}

/// Rows loaded by 500 row `INSERT ... VALUES` statements take the pages the
/// bulk build gives the same rows.
#[test]
fn multi_row_inserts_pack_like_a_bulk_build() {
    let rows = 20_000;
    let bulk = bulk_pages("values", rows);
    let database = Database::open(fresh("values")).expect("the database opens");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE n(a INTEGER, b INTEGER); BEGIN;")
        .expect("the table is made");
    session
        .execute_batch(&value_statements("n", rows, 500, |nth| nth))
        .expect("the rows are inserted");
    session.execute_batch("COMMIT").expect("the load commits");
    assert_intact(&database, "n", rows);
    let loaded = used_pages(&database);
    assert!(
        loaded <= bulk + SLACK_PAGES,
        "{rows} rows took {loaded} pages in 500 row inserts and {bulk} as one INSERT ... SELECT"
    );
}

/// Rows inserted one statement at a time, each its own call, take the pages
/// the bulk build gives them. The shell runs consecutive single row inserts as
/// one statement, so this goes through the driver, one call a row.
#[test]
fn single_row_inserts_pack_like_a_bulk_build() {
    let rows = 20_000;
    let bulk = bulk_pages("single", rows);
    let database = Database::open(fresh("single")).expect("the database opens");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE n(a INTEGER, b INTEGER); BEGIN;")
        .expect("the table is made");
    for nth in 0..rows {
        session
            .execute_batch(&format!("INSERT INTO n VALUES ({nth}, {})", nth % 7))
            .expect("the row is inserted");
    }
    session.execute_batch("COMMIT").expect("the load commits");
    assert_intact(&database, "n", rows);
    let loaded = used_pages(&database);
    assert!(
        loaded <= bulk + SLACK_PAGES,
        "{rows} rows took {loaded} pages one insert a row and {bulk} as one INSERT ... SELECT"
    );
}

/// `VACUUM` of a table loaded by appends gives no more pages than the file it
/// started from, and stays within a quarter of the bulk build.
#[test]
fn vacuum_does_not_leave_a_looser_file_than_a_bulk_build() {
    let rows = 90_000;
    let bulk = bulk_pages("vacuum", rows);
    let database = Database::open(fresh("vacuum")).expect("the database opens");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE n(a INTEGER, b INTEGER); BEGIN;")
        .expect("the table is made");
    session
        .execute_batch(&value_statements("n", rows, 500, |nth| nth))
        .expect("the rows are inserted");
    session.execute_batch("COMMIT").expect("the load commits");
    let loaded = used_pages(&database);
    session
        .execute_batch("VACUUM")
        .expect("the file is vacuumed");
    assert_intact(&database, "n", rows);
    let vacuumed = used_pages(&database);
    assert!(
        vacuumed <= loaded,
        "VACUUM took {loaded} pages to {vacuumed}, which is more"
    );
    // **`VACUUM` copies a table a page of rows at a time, so it packs as an
    // append does, not as one bulk build.** Reading the whole table to build it
    // in one pass is what made `VACUUM` of a 1.1 GB file peak at 937 MB, and
    // bug hunt 10 bounded that memory. An appended leaf is left between 0.6 and
    // 0.95 full where a bulk built one is 0.9, so the bound is a quarter above
    // the bulk build: 90,000 rows take 28 pages against 23.
    assert!(
        vacuumed <= bulk + bulk / 4 + SLACK_PAGES,
        "{rows} rows took {vacuumed} pages after VACUUM and {bulk} as one INSERT ... SELECT"
    );
}

/// Rows whose keys arrive in random order still leave leaves that are neither
/// nearly empty nor so full that every later insert splits them.
#[test]
fn random_order_rows_still_split_into_well_filled_leaves() {
    let rows = 30_000;
    let bulk = {
        let database = Database::open(fresh("random-bulk")).expect("the database opens");
        database
            .session()
            .execute_batch(&format!(
                "CREATE TABLE p(id INTEGER PRIMARY KEY, b INTEGER); \
                 WITH RECURSIVE s(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM s WHERE i < {}) \
                 INSERT INTO p SELECT i, i % 7 FROM s;",
                rows - 1
            ))
            .expect("the bulk load runs");
        used_pages(&database)
    };
    let database = Database::open(fresh("random")).expect("the database opens");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE p(id INTEGER PRIMARY KEY, b INTEGER); BEGIN;")
        .expect("the table is made");
    // 7919 is prime and does not divide 30,000, so the keys are a permutation.
    session
        .execute_batch(&value_statements("p", rows, 500, |nth| (nth * 7919) % rows))
        .expect("the rows are inserted");
    session.execute_batch("COMMIT").expect("the load commits");
    assert_intact(&database, "p", rows);
    let loaded = used_pages(&database);
    assert!(
        loaded <= bulk * 2 + SLACK_PAGES,
        "{rows} rows in random order took {loaded} pages and {bulk} as one INSERT ... SELECT"
    );
    assert!(
        loaded >= bulk,
        "{rows} rows in random order took {loaded} pages, fewer than the bulk build's {bulk}"
    );
}

/// Wide rows pack to about their delta size, so appending them still splits
/// the leaf, and the file is the size it was.
#[test]
fn wide_appended_rows_are_not_repacked_more_than_they_were() {
    let rows = 6_000;
    let database = Database::open(fresh("wide")).expect("the database opens");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE w(id INTEGER PRIMARY KEY, body TEXT); BEGIN;")
        .expect("the table is made");
    let body = "x".repeat(300);
    for first in (0..rows).step_by(100) {
        let tuples: Vec<String> = (first..first + 100)
            .map(|id| format!("({id}, '{body}{id}')"))
            .collect();
        session
            .execute_batch(&format!("INSERT INTO w VALUES {};", tuples.join(", ")))
            .expect("the rows are inserted");
    }
    session.execute_batch("COMMIT").expect("the load commits");
    assert_intact(&database, "w", rows);
    let loaded = used_pages(&database);
    session
        .execute_batch("VACUUM")
        .expect("the file is vacuumed");
    let vacuumed = used_pages(&database);
    // 300 byte rows put about 100 rows in a 32 KiB leaf. A bulk build packs
    // them at 0.9 and an appended leaf is left about 0.95 full by the split,
    // so the appended file is the smaller of the two.
    assert!(
        loaded <= vacuumed + SLACK_PAGES,
        "wide rows took {loaded} pages appended and {vacuumed} after VACUUM"
    );
}

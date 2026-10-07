//! `INSERT ... SELECT` into an empty table, over a real file that is closed and
//! opened again.
//!
//! Invariant: **the tree the bulk build writes is the table, on disk, after a
//! reopen, and a statement or transaction that is abandoned leaves the table
//! empty with every page accounted for.** The differential suite
//! `bulk_insert.rs` grades the answers against SQLite inside one session; this
//! file grades what reached the file, which is where the bulk build differs
//! most from the row by row insert. Its pages go straight into the data file
//! with no log record describing them, it replaces the table's tree under the
//! same handle, and it rewrites the catalog row to name the new root. A defect
//! in any of those reads back correctly in the session that wrote it, because
//! the pages are in the pool, and shows only after the file is opened again.
//!
//! The first test is the one that tells the two paths apart. The row by row
//! insert appends a log record for every row, and the bulk build appends one
//! for every page it allocates and a handful more, so the number of records a
//! 20,000 row copy appends says which path ran. Without it every other test
//! here would pass on the old insert too.

use std::path::PathBuf;

use inillucent_compat::workspace_root;
use inillucent_engine::connect::{Connection, Database};
use inillucent_tree::datum::OwnedDatum;

/// How many rows the source table holds.
const ROWS: i64 = 20_000;

/// The source table and its rows.
const SOURCE: &str = "CREATE TABLE src(id INTEGER PRIMARY KEY, k TEXT, n INTEGER, maybe TEXT); \
     WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 20000) \
     INSERT INTO src SELECT x, 'key ' || x, x * 7 % 1009, \
     CASE WHEN x = 15000 THEN NULL ELSE 'value ' || x || ' padding padding' END FROM c";

/// Returns a fresh path for one test's database, with nothing at it.
///
/// @param name - the test's name, which names the file
fn fresh(name: &str) -> PathBuf {
    let area = workspace_root().join("target/scratch/bulk-insert");
    let _ = std::fs::create_dir_all(&area);
    let path = area.join(format!("{name}.rdb"));
    inillucent_base::testing::remove_database(&path);
    path
}

/// Opens a database and builds the source table in it.
///
/// @param path - where the database goes
fn with_source(path: &PathBuf) -> Database {
    let database = Database::open(path).expect("a fresh database opens");
    database
        .session()
        .execute_batch(SOURCE)
        .expect("the source table fills");
    database
}

/// Returns the first value of a query's first row as an integer.
///
/// @param connection - the connection to ask
/// @param sql - a query answering one integer
fn integer(connection: &Connection<'_>, sql: &str) -> i64 {
    let rows = connection.query(sql).expect("the query runs");
    match rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(number)) => *number,
        other => panic!("{sql} answered {other:?} rather than an integer"),
    }
}

/// Returns what `PRAGMA integrity_check` says, joined into one line.
///
/// @param connection - the connection to ask
/// @param schema - which database to check
fn integrity(connection: &Connection<'_>, schema: &str) -> String {
    let rows = connection
        .query(&format!("PRAGMA {schema}.integrity_check"))
        .expect("the check runs");
    rows.iter()
        .filter_map(|row| match row.first() {
            Some(OwnedDatum::Text(text)) => Some(String::from_utf8_lossy(text).into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Returns a digest of a copied table: its row count and three sums that
/// change if any row is lost, duplicated or altered.
///
/// @param connection - the connection to ask
/// @param table - the table's name
fn digest(connection: &Connection<'_>, table: &str) -> (i64, i64, i64, i64) {
    (
        integer(connection, &format!("SELECT count(*) FROM {table}")),
        integer(connection, &format!("SELECT sum(id) FROM {table}")),
        integer(
            connection,
            &format!("SELECT sum(n * id % 9973) FROM {table}"),
        ),
        integer(connection, &format!("SELECT sum(length(k)) FROM {table}")),
    )
}

/// The bulk build appends nothing to the log per row, and the row by row insert
/// of the same rows does, so the log's growth says which path ran.
///
/// **Counted in bytes, not records** (task-2191). A transaction's inserts now
/// share one `InsertRows` record per 64 KiB, so the row by row path writes a
/// few hundred records for 20,000 rows, while every row still costs its entry:
/// at least 20 bytes of tree, page and length before the row.
#[test]
fn the_bulk_build_writes_no_log_record_per_row() {
    let path = fresh("records");
    let database = with_source(&path);
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE empty(id INTEGER PRIMARY KEY, k TEXT, n INTEGER); \
             CREATE INDEX empty_n ON empty(n); \
             CREATE TABLE held(id INTEGER PRIMARY KEY, k TEXT, n INTEGER); \
             CREATE INDEX held_n ON held(n); \
             INSERT INTO held VALUES (0, 'already here', 0)",
        )
        .expect("the targets are made");
    let before = database.log_stats().bytes;
    connection
        .execute_batch("INSERT INTO empty SELECT id, k, n FROM src")
        .expect("the copy into the empty table runs");
    let bulk = database.log_stats().bytes.saturating_sub(before);
    let before = database.log_stats().bytes;
    connection
        .execute_batch("INSERT INTO held SELECT id, k, n FROM src")
        .expect("the copy into the table that holds a row runs");
    let ordinary = database.log_stats().bytes.saturating_sub(before);
    assert!(
        ordinary >= ROWS as u64 * 20,
        "the row by row insert appended {ordinary} bytes for {ROWS} rows, so this \
         measure cannot tell the two paths apart"
    );
    assert!(
        bulk < ordinary / 20,
        "the copy into an empty table appended {bulk} bytes of log for {ROWS} rows, which is \
         the row by row insert's cost ({ordinary}) and not the bulk build's"
    );
    assert_eq!(digest(&connection, "empty").0, ROWS);
    assert_eq!(
        integer(&connection, "SELECT count(*) FROM empty WHERE n = 500"),
        integer(&connection, "SELECT count(*) FROM src WHERE n = 500")
    );
}

/// The copied rows and every index over them are on disk after the file is
/// closed and opened again, and rows written after the copy land in the tree
/// the copy built.
#[test]
fn the_rows_and_indexes_survive_a_reopen() {
    let path = fresh("reopen");
    let expected;
    {
        let database = with_source(&path);
        let connection = database.session();
        connection
            .execute_batch(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, n INTEGER, maybe TEXT); \
                 CREATE INDEX t_n ON t(n); \
                 CREATE UNIQUE INDEX t_k ON t(k); \
                 CREATE INDEX t_part ON t(maybe) WHERE n < 100; \
                 CREATE INDEX t_expr ON t(n % 13, k); \
                 INSERT INTO t SELECT id, k, n, maybe FROM src ORDER BY n, id",
            )
            .expect("the copy runs");
        expected = digest(&connection, "src");
        assert_eq!(digest(&connection, "t"), expected);
        assert_eq!(integrity(&connection, "main"), "ok");
    }
    {
        let database = Database::open(&path).expect("the database opens again");
        let connection = database.session();
        assert_eq!(
            digest(&connection, "t"),
            expected,
            "the copy after a reopen"
        );
        assert_eq!(integrity(&connection, "main"), "ok");
        assert_eq!(
            integer(&connection, "SELECT id FROM t WHERE k = 'key 12345'"),
            12_345
        );
        assert_eq!(
            integer(
                &connection,
                "SELECT count(*) FROM t WHERE n < 100 AND maybe > 'value 2'"
            ),
            integer(
                &connection,
                "SELECT count(*) FROM src WHERE n < 100 AND maybe > 'value 2'"
            )
        );
        assert_eq!(
            integer(&connection, "SELECT count(*) FROM t WHERE n % 13 = 4"),
            integer(&connection, "SELECT count(*) FROM src WHERE n % 13 = 4")
        );
        connection
            .execute_batch(
                "INSERT INTO t(k, n) VALUES ('after the copy', 5); \
                 UPDATE t SET n = n + 1 WHERE id % 1000 = 0; \
                 DELETE FROM t WHERE id % 997 = 0",
            )
            .expect("ordinary writes go into the built tree");
    }
    let database = Database::open(&path).expect("the database opens a third time");
    let connection = database.session();
    assert_eq!(integrity(&connection, "main"), "ok");
    assert_eq!(
        integer(&connection, "SELECT count(*) FROM t"),
        ROWS + 1 - ROWS / 997
    );
    assert_eq!(
        integer(&connection, "SELECT id FROM t WHERE k = 'after the copy'"),
        ROWS + 1
    );
}

/// A rolled back transaction takes the copied rows with it, gives every page
/// the build allocated back, and leaves the table able to take the copy again.
#[test]
fn a_rolled_back_copy_leaves_the_table_empty_and_every_page_accounted_for() {
    let path = fresh("rollback");
    {
        let database = with_source(&path);
        let connection = database.session();
        connection
            .execute_batch(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, n INTEGER); \
                 CREATE INDEX t_n ON t(n, k); \
                 BEGIN; \
                 INSERT INTO t SELECT id, k, n FROM src; \
                 SAVEPOINT inner_one; \
                 INSERT INTO src(k, n) VALUES ('inside', 1); \
                 ROLLBACK TO inner_one; \
                 RELEASE inner_one",
            )
            .expect("the transaction runs");
        assert_eq!(integer(&connection, "SELECT count(*) FROM t"), ROWS);
        connection
            .execute_batch("ROLLBACK")
            .expect("the transaction rolls back");
        assert_eq!(integer(&connection, "SELECT count(*) FROM t"), 0);
        assert_eq!(
            integer(&connection, "SELECT count(*) FROM t WHERE n = 3"),
            0,
            "the index still holds entries after the rollback"
        );
        // `integrity_check` reports a page no tree reaches and the free map
        // holds as `never used`, so a build whose pages were not given back
        // fails here.
        assert_eq!(integrity(&connection, "main"), "ok");
    }
    let database = Database::open(&path).expect("the database opens again");
    let connection = database.session();
    assert_eq!(integer(&connection, "SELECT count(*) FROM t"), 0);
    assert_eq!(integrity(&connection, "main"), "ok");
    connection
        .execute_batch("INSERT INTO t SELECT id, k, n FROM src")
        .expect("the copy runs after the rollback");
    assert_eq!(digest(&connection, "t"), digest(&connection, "src"));
    assert_eq!(integrity(&connection, "main"), "ok");
}

/// A copy that fails part way writes nothing, inside a transaction it leaves
/// the earlier statements alone, and the file agrees after a reopen.
#[test]
fn a_copy_that_fails_part_way_writes_nothing() {
    let path = fresh("fails");
    {
        let database = with_source(&path);
        let connection = database.session();
        connection
            .execute_batch(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, maybe TEXT NOT NULL); \
                 CREATE TABLE u(id INTEGER PRIMARY KEY, k TEXT UNIQUE, maybe TEXT)",
            )
            .expect("the targets are made");
        let failed = connection.execute_batch("INSERT INTO t SELECT id, k, maybe FROM src");
        assert!(failed.is_err(), "row 15000's NULL is refused");
        assert_eq!(integer(&connection, "SELECT count(*) FROM t"), 0);
        assert_eq!(integer(&connection, "SELECT last_insert_rowid()"), 14_999);
        // A unique index's duplicate is found after the sort and hands the
        // statement to the ordinary insert, which fails the same way.
        let failed = connection.execute_batch(
            "INSERT INTO u SELECT id, CASE WHEN id = 19000 THEN 'key 1' ELSE k END, maybe \
             FROM src",
        );
        assert!(failed.is_err(), "the repeated key is refused");
        assert_eq!(integer(&connection, "SELECT count(*) FROM u"), 0);
        assert_eq!(integrity(&connection, "main"), "ok");
        connection
            .execute_batch(
                "BEGIN; \
                 INSERT INTO u SELECT id, k, maybe FROM src WHERE id <= 100",
            )
            .expect("the transaction starts");
        let failed = connection.execute_batch("INSERT INTO t SELECT id, k, maybe FROM src");
        assert!(
            failed.is_err(),
            "the same NULL is refused inside a transaction"
        );
        connection
            .execute_batch("COMMIT")
            .expect("the transaction is still open and commits");
    }
    let database = Database::open(&path).expect("the database opens again");
    let connection = database.session();
    assert_eq!(integer(&connection, "SELECT count(*) FROM t"), 0);
    assert_eq!(integer(&connection, "SELECT count(*) FROM u"), 100);
    assert_eq!(integrity(&connection, "main"), "ok");
}

/// A table in an attached database and a temporary table take the bulk build
/// too, and their trees are read from their own files.
#[test]
fn an_attached_and_a_temporary_table_take_the_copy() {
    let path = fresh("attached-main");
    let side = fresh("attached-side");
    let attach = format!("ATTACH '{}' AS side", side.display());
    {
        let database = with_source(&path);
        let connection = database.session();
        connection
            .execute_batch(&attach)
            .expect("the side file attaches");
        connection
            .execute_batch(
                "CREATE TABLE side.t(id INTEGER PRIMARY KEY, k TEXT, n INTEGER); \
                 CREATE INDEX side.t_n ON t(n); \
                 INSERT INTO side.t SELECT id, k, n FROM src; \
                 CREATE TEMP TABLE scratch(id INTEGER PRIMARY KEY, k TEXT, n INTEGER); \
                 INSERT INTO scratch SELECT id, k, n FROM src WHERE id <= 5000",
            )
            .expect("the copies run");
        assert_eq!(digest(&connection, "side.t"), digest(&connection, "src"));
        assert_eq!(integer(&connection, "SELECT count(*) FROM scratch"), 5_000);
        assert_eq!(
            integer(
                &connection,
                "SELECT k = 'key 4321' FROM scratch WHERE id = 4321"
            ),
            1
        );
        assert_eq!(integrity(&connection, "side"), "ok");
        assert_eq!(integrity(&connection, "main"), "ok");
    }
    let database = Database::open(&path).expect("the database opens again");
    let connection = database.session();
    connection
        .execute_batch(&attach)
        .expect("the side file attaches again");
    assert_eq!(digest(&connection, "side.t"), digest(&connection, "src"));
    assert_eq!(
        integer(&connection, "SELECT count(*) FROM side.t WHERE n = 77"),
        integer(&connection, "SELECT count(*) FROM src WHERE n = 77")
    );
    assert_eq!(integrity(&connection, "side"), "ok");
}

//! The rows a statement produced before it failed.
//!
//! Invariant: **a query that fails on row N reports the failure and carries rows
//! 1 to N minus 1 in [`inillucent_driver::Error::partial`].** SQLite hands out
//! every row ahead of the one that raised the error, and a program that reads
//! rows until a failure and reports how far it got needs them. The call still
//! returns `Err`, so a caller that ignores the field behaves as it did.

use std::path::PathBuf;

use inillucent_driver::{Database, Status, Value};

/// Where this suite's scratch databases live.
///
/// @param name - the case's own file name
fn scratch(name: &str) -> PathBuf {
    let mut directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    directory.pop();
    directory.pop();
    let directory = directory.join("_agent_output/driver-partial-rows");
    let _ = std::fs::create_dir_all(&directory);
    let path = directory.join(format!("{name}.rdb"));
    inillucent_base::testing::remove_database(&path);
    path
}

/// The statement that fails on the row whose `a` is 3000 of 5000.
const FAILS_AT_3000: &str = "SELECT a, CASE WHEN a = 3000 THEN json('x') END FROM t";

/// Opens a database holding `t(a)` with the rows 1 to 5000.
///
/// @param name - the case's own file name
fn filled(name: &str) -> Database {
    let database = Database::open(scratch(name)).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE t(a); WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c \
             WHERE x<5000) INSERT INTO t SELECT x FROM c",
        )
        .expect("the table fills");
    database
}

/// A query that fails in its second batch hands back every row before the failure.
#[test]
fn a_failed_query_carries_the_rows_before_the_failure() {
    let database = filled("query");
    let connection = database.session();
    let failure = connection
        .query_all(FAILS_AT_3000, &[])
        .expect_err("row 3000 fails");
    assert_eq!(failure.message, "malformed JSON");
    let partial = failure
        .partial
        .expect("the rows before the failure come with it");
    assert_eq!(partial.rows.len(), 2999);
    assert_eq!(partial.total, 2999);
    assert_eq!(partial.value(0, 0), Some(&Value::Integer(1)));
    assert_eq!(partial.value(2998, 0), Some(&Value::Integer(2999)));
    assert_eq!(partial.value(2998, 1), Some(&Value::Null));
    assert_eq!(
        partial.columns.first().map(|column| column.name.as_str()),
        Some("a")
    );
}

/// The limit the caller asked for cuts the rows carried, as it cuts a success.
#[test]
fn the_limit_cuts_the_rows_carried_with_a_failure() {
    let database = filled("limit");
    let connection = database.session();
    let failure = connection
        .query(FAILS_AT_3000, &[], 10)
        .expect_err("row 3000 fails");
    let partial = failure
        .partial
        .expect("the rows before the failure come with it");
    assert_eq!(partial.rows.len(), 10);
    assert_eq!(partial.total, 2999);
    assert!(partial.more);
}

/// A prepared statement reports the rows before a failure too.
#[test]
fn a_prepared_statement_carries_the_rows_before_a_failure() {
    let database = filled("prepared");
    let connection = database.session();
    let mut statement = connection.prepare(FAILS_AT_3000).expect("it compiles");
    for _ in 0..2 {
        let failure = statement
            .query(&[], usize::MAX)
            .expect_err("row 3000 fails");
        assert_eq!(
            failure.partial.map(|rows| rows.rows.len()),
            Some(2999),
            "every execution of the prepared statement carries them"
        );
    }
}

/// A failure before the first row carries no rows, and a statement that is not a
/// query carries none either.
#[test]
fn a_failure_before_any_row_carries_none() {
    let database = filled("none");
    let connection = database.session();
    let first = connection
        .query_all("SELECT json('x') FROM t", &[])
        .expect_err("the first row fails");
    assert!(first.partial.is_none());
    let write = connection
        .execute("INSERT INTO nothing_of_the_sort VALUES (1)", &[])
        .expect_err("the table is not there");
    assert!(write.partial.is_none());
    assert_ne!(write.status, Status::Internal);
}

/// A window pass reports the rows it answered before it failed.
#[test]
fn a_window_pass_carries_the_rows_before_its_failure() {
    let database = Database::open(scratch("window")).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE m(id, v); INSERT INTO m VALUES (1, 9223372036854775807), \
             (2, 9223372036854775807), (3, 1), (4, 2)",
        )
        .expect("the table fills");
    let failure = connection
        .query_all(
            "SELECT id, sum(v) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM m",
            &[],
        )
        .expect_err("the sum overflows");
    assert_eq!(failure.message, "integer overflow");
    assert_eq!(failure.partial.map(|rows| rows.rows.len()), Some(1));
}

//! A process that keeps a database open sees the schema another process
//! changed, on the next statement it runs.
//!
//! Invariant: **a statement runs against the schema the file holds when the
//! statement takes the file lock, never the schema this process read before.**
//!
//! ## Why these cases exist
//!
//! An application server keeps its database open for days, and a migration is
//! run beside it by another program. SQLite answers that with its schema
//! cookie: `sqlite3_step` compares it once the lock is held, and recompiles the
//! statement when it moved.
//!
//! This engine compared the schema generation and then took the lock, and
//! taking the lock is what notices another process's commit and rereads the
//! catalog. So the statement ran with the plan it had compiled against the old
//! schema. After another process added a column, `SELECT *` here kept
//! returning the old columns. After another process dropped a column,
//! `SELECT *` failed with "the tree read for FROM term 0 does not carry column
//! 3", an internal error and not a SQL one. Found by the bug hunt that drove two
//! shells on one file.
//!
//! The other process is the `inillucent` command line, which is how a second
//! program reaches a file in production.

use std::path::{Path, PathBuf};

use inillucent_compat::cliproc::{program, rows, run};
use inillucent_compat::workspace_root;
use inillucent_driver::{Database, OpenOptions, Rows, Value};

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/process-schema-change")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs statements in another process, as one batch, and fails the case if
/// they fail.
///
/// @param database - the file
/// @param sql - the statements
fn other_process_exec(database: &Path, sql: &str) {
    let ran = run(
        &program("inillucent"),
        &["--db", &database.to_string_lossy(), "batch", sql],
    );
    assert_eq!(
        ran.code,
        0,
        "the other process failed on {sql}:\n{}",
        ran.said()
    );
}

/// Returns the rows a query prints, read by another process.
///
/// @param database - the file
/// @param sql - the query
fn other_process_rows(database: &Path, sql: &str) -> Vec<Vec<String>> {
    let ran = run(
        &program("inillucent"),
        &[
            "--db",
            &database.to_string_lossy(),
            "query",
            sql,
            "--output",
            "json",
        ],
    );
    assert_eq!(ran.code, 0, "the reading process failed:\n{}", ran.said());
    rows(&ran.stdout)
}

/// Opens the file in this process.
///
/// @param database - the file
fn this_process(database: &Path) -> Database {
    Database::open_with(database, OpenOptions::default())
        .unwrap_or_else(|error| panic!("this process could not open the file: {error:?}"))
}

/// Returns a result's column names and its rows, each value printed as SQL
/// would quote it, so a case can compare both in one assertion.
///
/// @param rows - what a query returned
fn shown(rows: &Rows) -> (Vec<String>, Vec<Vec<String>>) {
    let names = rows
        .columns
        .iter()
        .map(|column| column.name.clone())
        .collect();
    let values = rows
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| match value {
                    Value::Null => "NULL".to_string(),
                    Value::Integer(n) => n.to_string(),
                    Value::Real(r) => r.to_string(),
                    Value::Text(text) => format!("'{text}'"),
                    Value::Blob(bytes) => format!("x'{}'", bytes.len()),
                })
                .collect()
        })
        .collect();
    (names, values)
}

/// Turns string literals into the owned strings `shown` returns.
///
/// @param names - the expected column names
/// @param values - the expected rows
fn expect(names: &[&str], values: &[&[&str]]) -> (Vec<String>, Vec<Vec<String>>) {
    (
        names.iter().map(|name| (*name).to_string()).collect(),
        values
            .iter()
            .map(|row| row.iter().map(|value| (*value).to_string()).collect())
            .collect(),
    )
}

/// A query run by its text after another process added a column returns the
/// new column.
#[test]
fn a_query_after_another_process_adds_a_column_returns_it() {
    let directory = area("add-column");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch("CREATE TABLE t(a, b, c); INSERT INTO t VALUES(1, 2, 3);")
        .unwrap_or_else(|error| panic!("the table was not created: {error:?}"));
    let before = session
        .query_all("SELECT * FROM t", &[])
        .expect("the first read");
    assert_eq!(
        shown(&before),
        expect(&["a", "b", "c"], &[&["1", "2", "3"]])
    );

    other_process_exec(&database, "ALTER TABLE t ADD COLUMN d DEFAULT 9");

    let after = session
        .query_all("SELECT * FROM t", &[])
        .unwrap_or_else(|error| panic!("the read after the other process's ALTER: {error:?}"));
    assert_eq!(
        shown(&after),
        expect(&["a", "b", "c", "d"], &[&["1", "2", "3", "9"]]),
        "this process answered from the plan it compiled before the other process added `d`"
    );
}

/// A query after another process dropped a column returns the columns that
/// are left, with their own values.
#[test]
fn a_query_after_another_process_drops_a_column_reads_the_right_columns() {
    let directory = area("drop-column");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch("CREATE TABLE t(a, b, c, d); INSERT INTO t VALUES(1, 2, 3, 4);")
        .unwrap_or_else(|error| panic!("the table was not created: {error:?}"));
    session
        .query_all("SELECT * FROM t", &[])
        .expect("the first read");
    session
        .query_all("SELECT c FROM t", &[])
        .expect("the first read of c");

    other_process_exec(&database, "ALTER TABLE t DROP COLUMN b");

    let star = session
        .query_all("SELECT * FROM t", &[])
        .unwrap_or_else(|error| {
            panic!("SELECT * after the other process's DROP COLUMN: {error:?}")
        });
    assert_eq!(shown(&star), expect(&["a", "c", "d"], &[&["1", "3", "4"]]));
    let c = session
        .query_all("SELECT c FROM t", &[])
        .unwrap_or_else(|error| {
            panic!("SELECT c after the other process's DROP COLUMN: {error:?}")
        });
    assert_eq!(shown(&c), expect(&["c"], &[&["3"]]));
}

/// A statement this process prepared before another process changed the table
/// runs against the table as it is now, as `sqlite3_step` does.
#[test]
fn a_prepared_statement_follows_another_processes_schema_change() {
    let directory = area("prepared");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch("CREATE TABLE t(a, b, c); INSERT INTO t VALUES(1, 2, 3), (4, 5, 6);")
        .unwrap_or_else(|error| panic!("the table was not created: {error:?}"));
    let mut statement = session
        .prepare("SELECT * FROM t WHERE a = ?1")
        .expect("the statement prepares");
    let first = statement
        .query(&[Value::Integer(4)], usize::MAX)
        .expect("the first run");
    assert_eq!(shown(&first), expect(&["a", "b", "c"], &[&["4", "5", "6"]]));

    other_process_exec(
        &database,
        "ALTER TABLE t DROP COLUMN b; ALTER TABLE t ADD COLUMN e DEFAULT 'new'",
    );

    let second = statement
        .query(&[Value::Integer(4)], usize::MAX)
        .unwrap_or_else(|error| panic!("the prepared statement after the change: {error:?}"));
    assert_eq!(
        shown(&second),
        expect(&["a", "c", "e"], &[&["4", "6", "'new'"]])
    );
}

/// A write this process makes after another process dropped and recreated the
/// table goes into the new table, and the other process reads it back.
#[test]
fn a_write_after_another_process_recreates_the_table_lands_in_the_new_table() {
    let directory = area("recreate");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch("CREATE TABLE t(a, b, c); INSERT INTO t VALUES(1, 2, 3);")
        .unwrap_or_else(|error| panic!("the table was not created: {error:?}"));
    session
        .execute(
            "INSERT INTO t VALUES(?1, ?2, ?3)",
            &[Value::Integer(7), Value::Integer(8), Value::Integer(9)],
        )
        .expect("the first insert");

    other_process_exec(
        &database,
        "DROP TABLE t; CREATE TABLE t(x TEXT, y TEXT, z TEXT); INSERT INTO t VALUES('p', 'q', 'r')",
    );

    session
        .execute(
            "INSERT INTO t VALUES(?1, ?2, ?3)",
            &[Value::Integer(7), Value::Integer(8), Value::Integer(9)],
        )
        .unwrap_or_else(|error| {
            panic!("the insert after the other process recreated t: {error:?}")
        });
    let here = session
        .query_all("SELECT * FROM t ORDER BY x", &[])
        .expect("this process reads");
    assert_eq!(
        shown(&here),
        expect(&["x", "y", "z"], &[&["'7'", "'8'", "'9'"], &["'p'", "'q'", "'r'"]]),
        "the insert used the dropped table's plan, so its values missed the new table's TEXT affinity"
    );
    drop(ours);
    assert_eq!(
        other_process_rows(&database, "SELECT x, typeof(x) FROM t ORDER BY x"),
        vec![
            vec!["7".to_string(), "text".to_string()],
            vec!["p".to_string(), "text".to_string()],
        ]
    );
    let check = other_process_rows(&database, "PRAGMA integrity_check");
    assert_eq!(check, vec![vec!["ok".to_string()]]);
}

/// A statement naming a column another process renamed is refused with the
/// message SQLite gives, and the new name works.
#[test]
fn a_statement_naming_a_renamed_column_is_refused() {
    let directory = area("rename");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch("CREATE TABLE t(a, b); INSERT INTO t VALUES(1, 2);")
        .unwrap_or_else(|error| panic!("the table was not created: {error:?}"));
    session
        .query_all("SELECT b FROM t", &[])
        .expect("the first read");

    other_process_exec(&database, "ALTER TABLE t RENAME COLUMN b TO renamed");

    let refused = session.query_all("SELECT b FROM t", &[]);
    let message = match refused {
        Ok(rows) => panic!("SELECT b answered {:?} after b was renamed", shown(&rows)),
        Err(error) => error.to_string(),
    };
    assert!(
        message.contains("no such column: b"),
        "the refusal said {message}"
    );
    let renamed = session
        .query_all("SELECT renamed FROM t", &[])
        .expect("the new name reads");
    assert_eq!(shown(&renamed), expect(&["renamed"], &[&["2"]]));
}

/// A table, a view and a column another process creates after this process
/// opened the file are found by the first statement that names them.
///
/// **The statement failed to compile, so it never took the lock that rereads
/// the catalog.** Each one failed with "no such table" or "no such column",
/// and so did every later one, for as long as this process kept the file open:
/// an application server never saw the table a migration added beside it.
/// SQLite checks the schema cookie after a failed prepare and prepares again.
#[test]
fn objects_another_process_creates_are_found_by_a_process_already_open() {
    let directory = area("created-elsewhere");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch("CREATE TABLE seed(x); INSERT INTO seed VALUES(1);")
        .unwrap_or_else(|error| panic!("the seed table was not created: {error:?}"));
    session
        .query_all("SELECT x FROM seed", &[])
        .expect("the first read");

    other_process_exec(
        &database,
        "CREATE TABLE fresh(a, b); INSERT INTO fresh VALUES(1, 'one'); \
         CREATE VIEW fresh_view AS SELECT b FROM fresh; ALTER TABLE seed ADD COLUMN y DEFAULT 'y'",
    );

    let table = session
        .query_all("SELECT a, b FROM fresh", &[])
        .unwrap_or_else(|error| panic!("the table the other process created: {error:?}"));
    assert_eq!(shown(&table), expect(&["a", "b"], &[&["1", "'one'"]]));
    let view = session
        .query_all("SELECT b FROM fresh_view", &[])
        .unwrap_or_else(|error| panic!("the view the other process created: {error:?}"));
    assert_eq!(shown(&view), expect(&["b"], &[&["'one'"]]));
    session
        .execute("INSERT INTO seed(x, y) VALUES(2, 'mine')", &[])
        .unwrap_or_else(|error| {
            panic!("an insert naming the column the other process added: {error:?}")
        });
    let mut statement = session
        .prepare("SELECT y FROM seed ORDER BY x")
        .unwrap_or_else(|error| panic!("a prepare naming the added column: {error:?}"));
    let rows = statement.query(&[], usize::MAX).expect("the prepared read");
    assert_eq!(shown(&rows), expect(&["y"], &[&["'y'"], &["'mine'"]]));
}

/// A trigger another process creates fires on this process's next write.
///
/// The insert compiles fine against the old catalog, which has no trigger, so
/// this is the plan compiled before the lock that the generation check after
/// the lock throws away.
#[test]
fn a_trigger_another_process_creates_fires_on_this_processes_write() {
    let directory = area("trigger-elsewhere");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch("CREATE TABLE item(n); CREATE TABLE audit(n); INSERT INTO item VALUES(1);")
        .unwrap_or_else(|error| panic!("the tables were not created: {error:?}"));
    session
        .execute("INSERT INTO item VALUES(?1)", &[Value::Integer(2)])
        .expect("the first insert");

    other_process_exec(
        &database,
        "CREATE TRIGGER item_audit AFTER INSERT ON item BEGIN INSERT INTO audit VALUES(new.n); END",
    );

    session
        .execute("INSERT INTO item VALUES(?1)", &[Value::Integer(3)])
        .expect("the insert after the trigger was created");
    let audit = session
        .query_all("SELECT n FROM audit", &[])
        .expect("the audit read");
    assert_eq!(
        shown(&audit),
        expect(&["n"], &[&["3"]]),
        "the insert ran the plan compiled before the other process created the trigger"
    );
}

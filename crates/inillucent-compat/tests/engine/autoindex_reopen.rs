//! A table whose primary key is not an integer, reopened with its log unfolded.
//!
//! Invariant: **every tree the log names has a shape the replay can be told.**
//! Recovery refuses a record naming a tree it was not given the shape of, which
//! is the right refusal - replaying into a guessed shape is how a file is
//! corrupted quietly - so the shapes have to be derivable from the catalog for
//! *every* tree, not for most of them.
//!
//! One was not. A `PRIMARY KEY` that is not `INTEGER` implies an index, and
//! SQLite writes its `sqlite_schema` row with a **NULL statement**: the index is
//! declared by the table's own text, and the empty statement is what tells an
//! automatic index from a created one. The recovery's shape derivation parsed
//! that empty text as a `CREATE INDEX`, got nothing, and told the applier no
//! shape - so the first log record naming the index refused the open:
//!
//! ```text
//! bad parameter or other API misuse: the log names tree 2147483649,
//! which this recovery was not told the shape of
//! ```
//!
//! The visible consequence was as bad as it sounds: **a database with a
//! `TEXT PRIMARY KEY` could not be reopened after a write**. It reproduced on
//! three lines of SQL, and was found while building `.archive` - whose
//! `sqlar` table is keyed by `name TEXT PRIMARY KEY`.
//!
//! The test abandons the connection the way the crash tests do, so the log is
//! still there to replay. A tidy close checkpoints, which folds the log into the
//! file and never reaches the code this is about.

use std::path::{Path, PathBuf};

use inillucent_compat::facade::Database;
use inillucent_compat::workspace_root;
use inillucent_value::Value;

/// Returns a scratch directory for one scenario.
///
/// @param name - what to name it after
fn scratch(name: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output/autoindex-reopen")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    let _ = std::fs::create_dir_all(&directory);
    directory
}

/// Writes rows and abandons the connection with its log unfolded.
///
/// `locking_mode = NORMAL` lets the file go **without** checkpointing, and
/// forgetting the handle then leaves the pages where they are. That is the
/// state a crash leaves and the state this test needs: a log with records in it
/// that the next open has to replay.
///
/// @param path - the database file
/// @param sql - the statements to run
fn write_and_abandon(path: &Path, sql: &str) {
    let database = Database::open(path).expect("the database opens");
    let connection = database.session().expect("the connection opens");
    connection
        .execute_batch(&format!("{sql}\nPRAGMA locking_mode = NORMAL;"))
        .expect("the statements run");
    std::mem::forget(connection);
    std::mem::forget(database);
}

/// Returns the rows a query answers over a freshly opened database.
///
/// @param path - the database file
/// @param sql - the query
fn read_back(path: &Path, sql: &str) -> Vec<Vec<Value<'static>>> {
    let database = Database::open(path).expect("the database reopens");
    let connection = database.session().expect("the connection opens");
    connection.query(sql).expect("the query runs")
}

#[test]
fn a_text_primary_key_survives_a_reopen_with_the_log_unfolded() {
    let directory = scratch("text");
    let path = directory.join("archive.db");
    write_and_abandon(
        &path,
        "CREATE TABLE sqlar(name TEXT PRIMARY KEY, mode INT, sz INT, data BLOB);\n\
         INSERT INTO sqlar VALUES ('src/a.txt', 33206, 3, x'6f6e65');\n\
         INSERT INTO sqlar VALUES ('src', 16895, 0, NULL);",
    );
    let rows = read_back(&path, "SELECT name, sz FROM sqlar ORDER BY name");
    let names: Vec<String> = rows
        .iter()
        .filter_map(|row| row.first())
        .filter_map(|value| match value {
            Value::Text(text) => Some(String::from_utf8_lossy(&text.utf8_bytes()).into_owned()),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec!["src".to_string(), "src/a.txt".to_string()]);
    // The index the primary key implies has to answer too, or the table has
    // been reopened without the thing that made this fail.
    let found = read_back(&path, "SELECT sz FROM sqlar WHERE name = 'src/a.txt'");
    assert_eq!(found.len(), 1, "the primary key did not find the row");
}

#[test]
fn a_non_integer_primary_key_of_any_type_survives_a_reopen() {
    // `INT PRIMARY KEY` is not `INTEGER PRIMARY KEY`: it is an ordinary column
    // with an implied unique index, and it took the same path.
    let directory = scratch("int");
    let path = directory.join("keys.db");
    write_and_abandon(
        &path,
        "CREATE TABLE t(a INT PRIMARY KEY, b TEXT);\n\
         INSERT INTO t VALUES (1, 'one'), (2, 'two');",
    );
    let rows = read_back(&path, "SELECT count(*) FROM t");
    assert_eq!(
        rows.first()
            .and_then(|row| row.first())
            .and_then(Value::as_integer),
        Some(2)
    );
}

#[test]
fn a_compound_primary_key_survives_a_reopen() {
    let directory = scratch("compound");
    let path = directory.join("pairs.db");
    write_and_abandon(
        &path,
        "CREATE TABLE t(a TEXT, b TEXT, c INT, PRIMARY KEY(a, b));\n\
         INSERT INTO t VALUES ('x', 'y', 1), ('x', 'z', 2);",
    );
    let rows = read_back(&path, "SELECT c FROM t WHERE a = 'x' AND b = 'z'");
    assert_eq!(
        rows.first()
            .and_then(|row| row.first())
            .and_then(Value::as_integer),
        Some(2)
    );
}

#[test]
fn a_created_index_survives_the_same_reopen() {
    let directory = scratch("created");
    let path = directory.join("created.db");
    write_and_abandon(
        &path,
        "CREATE TABLE t(a TEXT, b INT);
         CREATE INDEX i ON t(a);
         INSERT INTO t VALUES ('x', 1), ('y', 2);",
    );
    let rows = read_back(&path, "SELECT b FROM t WHERE a = 'y'");
    assert_eq!(
        rows.first()
            .and_then(|row| row.first())
            .and_then(Value::as_integer),
        Some(2)
    );
}

#[test]
fn an_automatic_index_answers_a_query_after_a_tidy_close() {
    // **The same defect without a log to replay.** `load_schema` skipped an
    // index whose statement is empty, so the declaration the planner reads kept
    // the root of zero it was parsed with: `EXPLAIN QUERY PLAN` named
    // `sqlite_autoindex_t_1` and the statement answered `no layout imported for
    // root page 0`. A tidy close reaches it too, which is what makes this the
    // more serious half.
    let directory = scratch("tidy");
    let path = directory.join("tidy.db");
    {
        let database = Database::open(&path).expect("the database opens");
        let connection = database.session().expect("the connection opens");
        connection
            .execute_batch(
                "CREATE TABLE t(a TEXT PRIMARY KEY, b INT);
                 INSERT INTO t VALUES ('x', 1), ('y', 2);",
            )
            .expect("the statements run");
    }
    let rows = read_back(&path, "SELECT b FROM t WHERE a = 'y'");
    assert_eq!(
        rows.first()
            .and_then(|row| row.first())
            .and_then(Value::as_integer),
        Some(2),
        "the primary key's index did not answer after a reopen"
    );
}

/// Returns a query's first column of every row, as integers.
///
/// @param path - the database file, opened fresh
/// @param sql - the query
fn integers_after_reopen(path: &Path, sql: &str) -> Vec<i64> {
    read_back(path, sql)
        .iter()
        .filter_map(|row| row.first())
        .filter_map(Value::as_integer)
        .collect()
}

/// An `INSTEAD OF` trigger on a view still fires after the file is closed and
/// opened again.
///
/// The load pass that attaches a trigger to its table skipped every trigger
/// whose table it had no entry for, and a view has no entry there because it has
/// no tree. The trigger's catalog row was then missing from the rows the
/// connection keeps, so `INSERT`, `UPDATE` and `DELETE` on the view failed with
/// "cannot modify v0 because it is a view" in the second session while they
/// worked in the session that created the trigger. The usage corpus runs one
/// session per case and cannot see this, so it needs a reopen.
#[test]
fn an_instead_of_trigger_on_a_view_fires_after_a_reopen() {
    let directory = scratch("view-trigger");
    let path = directory.join("view.db");
    {
        let database = Database::open(&path).expect("the database opens");
        let connection = database.session().expect("the connection opens");
        connection
            .execute_batch(
                "CREATE TABLE t0(k INTEGER, a, b TEXT);
                 CREATE INDEX ie ON t0(coalesce(a, 0));
                 CREATE VIEW v0 AS SELECT k, a, b FROM t0;
                 CREATE TRIGGER vi INSTEAD OF INSERT ON v0 BEGIN
                   INSERT INTO t0(k, a, b) VALUES (NEW.k, NEW.a, NEW.b);
                 END;
                 CREATE TRIGGER vu INSTEAD OF UPDATE ON v0 BEGIN
                   UPDATE t0 SET a = NEW.a WHERE k = OLD.k;
                 END;
                 CREATE TRIGGER vd INSTEAD OF DELETE ON v0 BEGIN
                   DELETE FROM t0 WHERE k = OLD.k;
                 END;
                 INSERT INTO v0 VALUES (1, 2, 'x');",
            )
            .expect("the first session runs");
    }
    {
        let database = Database::open(&path).expect("the database reopens");
        let connection = database.session().expect("the connection opens");
        connection
            .execute_batch(
                "INSERT INTO v0 VALUES (3, 4, 'y');
                 INSERT INTO v0 VALUES (5, 6, 'z');
                 UPDATE v0 SET a = 40 WHERE k = 3;
                 DELETE FROM v0 WHERE k = 5;",
            )
            .expect("the second session writes through the triggers");
    }
    assert_eq!(
        integers_after_reopen(&path, "SELECT k FROM t0 ORDER BY k"),
        vec![1, 3]
    );
    assert_eq!(
        integers_after_reopen(&path, "SELECT a FROM t0 ORDER BY k"),
        vec![2, 40]
    );
}

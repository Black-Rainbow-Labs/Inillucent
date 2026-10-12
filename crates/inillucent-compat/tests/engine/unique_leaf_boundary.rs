//! A `UNIQUE` index refuses a duplicate of every key it holds, including the
//! first key of each leaf.
//!
//! Invariant: **after any number of attempts to insert or update a row onto an
//! existing unique key, the index holds each key once and `PRAGMA
//! integrity_check` says `ok`.**
//!
//! ## Why this exists
//!
//! The uniqueness check probes the index with the key's columns alone, without
//! the rowid every entry ends with. That prefix sorts before every entry that
//! begins with it, so the descent for `6529` went left of the separator
//! `(6529, 6529)` and landed on the leaf before it, found nothing there and
//! reported no conflict. So the first key of every leaf after the first could
//! be inserted twice: 6529, 13057 and 19585 in a 20,000 row integer column at
//! the default page size, and a few hundred keys into a text column. Plain
//! `INSERT`, `OR IGNORE`, `OR REPLACE`, both upsert forms and `UPDATE` all let
//! the duplicate through, and `PRAGMA integrity_check` then reported
//! "non-unique entry in index". The primary key of a table was not affected,
//! because it is probed with the whole key. Found by the bug hunt's ORM
//! schemas.
//!
//! Each case tries every existing key at once with `INSERT ... SELECT`, so it
//! covers every leaf boundary the data made, whatever the page size.

use std::path::PathBuf;

use inillucent_compat::workspace_root;
use inillucent_driver::{Connection, Database, Value};

/// Opens a fresh database in a directory of the case's own.
///
/// @param name - the case's name, which is also the directory's
fn fresh(name: &str) -> (PathBuf, Database) {
    let directory = workspace_root()
        .join("_agent_output/unique-leaf-boundary")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let path = directory.join("case.rdb");
    let database = Database::open(&path).expect("the database opens");
    (path, database)
}

/// Returns one integer a query answers.
///
/// @param session - the connection
/// @param sql - a query returning one integer
fn integer(session: &Connection<'_>, sql: &str) -> i64 {
    let rows = session
        .query_all(sql, &[])
        .unwrap_or_else(|error| panic!("{sql}: {error:?}"));
    match rows.rows.first().and_then(|row| row.first()) {
        Some(Value::Integer(n)) => *n,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Returns `PRAGMA integrity_check`'s answer as one line.
///
/// @param session - the connection
fn integrity(session: &Connection<'_>) -> String {
    let rows = session
        .query_all("PRAGMA integrity_check", &[])
        .expect("the integrity check runs");
    rows.rows
        .iter()
        .filter_map(|row| row.first().and_then(Value::text).map(str::to_string))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Tries every conflict path against every key the table already holds, and
/// checks the index still holds each key once.
///
/// @param session - the connection
/// @param table - the table
/// @param columns - the unique columns, as an `INSERT` column list
/// @param rows - how many rows the table holds
fn every_duplicate_is_refused(session: &Connection<'_>, table: &str, columns: &str, rows: i64) {
    let plain = session.execute(
        &format!("INSERT INTO {table}({columns}) SELECT {columns} FROM {table}"),
        &[],
    );
    let message = match plain {
        Ok(changed) => panic!("INSERT of every existing key into {table} wrote {changed} rows"),
        Err(error) => error.to_string(),
    };
    assert!(
        message.contains("UNIQUE constraint failed"),
        "{table}: {message}"
    );
    let ignored = session
        .execute(
            &format!("INSERT OR IGNORE INTO {table}({columns}) SELECT {columns} FROM {table}"),
            &[],
        )
        .expect("INSERT OR IGNORE runs");
    assert_eq!(ignored, 0, "INSERT OR IGNORE wrote duplicates into {table}");
    let upsert = session
        .execute(
            &format!(
                "INSERT INTO {table}({columns}) SELECT {columns} FROM {table} WHERE true \
                 ON CONFLICT DO NOTHING"
            ),
            &[],
        )
        .expect("the upsert runs");
    assert_eq!(
        upsert, 0,
        "ON CONFLICT DO NOTHING wrote duplicates into {table}"
    );
    let replaced = session
        .execute(
            &format!("INSERT OR REPLACE INTO {table}({columns}) SELECT {columns} FROM {table}"),
            &[],
        )
        .expect("INSERT OR REPLACE runs");
    assert_eq!(
        replaced, rows as u64,
        "every row of {table} is replaced once"
    );
    assert_eq!(
        integer(session, &format!("SELECT count(*) FROM {table}")),
        rows,
        "{table} holds a row twice"
    );
    assert_eq!(
        integrity(session),
        "ok",
        "{table}'s index holds a key twice"
    );
}

/// An integer `UNIQUE` column of 20,000 rows, past several leaves.
#[test]
fn an_integer_unique_column_refuses_a_duplicate_of_every_key() {
    let (_, database) = fresh("integer");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, k INT UNIQUE); \
             INSERT INTO t SELECT value, value FROM generate_series(1, 20000);",
        )
        .expect("the table fills");
    let first = session.execute("INSERT INTO t VALUES(999999, 6529)", &[]);
    assert!(
        first.is_err(),
        "the first key of the second leaf was taken twice"
    );
    every_duplicate_is_refused(&session, "t", "k", 20_000);
    let update = session.execute("UPDATE t SET k = 6529 WHERE k = 1", &[]);
    assert!(update.is_err(), "UPDATE moved a row onto an existing key");
    let upsert = session
        .execute(
            "INSERT INTO t(id, k) VALUES(888888, 13057) ON CONFLICT(k) DO UPDATE SET id = id",
            &[],
        )
        .expect("the upsert runs");
    assert_eq!(upsert, 1);
    assert_eq!(
        integer(&session, "SELECT count(*) FROM t WHERE k = 13057"),
        1
    );
    assert_eq!(
        integer(&session, "SELECT count(*) FROM t WHERE id = 888888"),
        0
    );
}

/// Long text keys, so each leaf holds few of them, at a small page size.
#[test]
fn a_text_unique_column_refuses_a_duplicate_of_every_key() {
    let (_, database) = fresh("text");
    let session = database.session();
    session
        .execute_batch(
            "PRAGMA page_size = 4096; \
             CREATE TABLE u(id INTEGER PRIMARY KEY, email TEXT UNIQUE, note TEXT); \
             INSERT INTO u(email, note) SELECT printf('user%05d@example.com%.*c', value, 300, 'x'), 'n' \
             FROM generate_series(1, 3000);",
        )
        .expect("the table fills");
    every_duplicate_is_refused(&session, "u", "email", 3_000);
}

/// A composite unique constraint, a partial unique index, and a secondary
/// unique index on a `WITHOUT ROWID` table.
#[test]
fn composite_partial_and_without_rowid_unique_indexes_refuse_every_duplicate() {
    let (path, database) = fresh("shapes");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE c(id INTEGER PRIMARY KEY, a INT, b TEXT, UNIQUE(a, b)); \
             INSERT INTO c(a, b) SELECT value / 10, printf('b%d', value % 10) FROM generate_series(1, 30000); \
             CREATE TABLE p(id INTEGER PRIMARY KEY, k INT, live INT); \
             CREATE UNIQUE INDEX p_live ON p(k) WHERE live = 1; \
             INSERT INTO p(k, live) SELECT value, 1 FROM generate_series(1, 20000); \
             CREATE TABLE w(code TEXT PRIMARY KEY, n INT UNIQUE) WITHOUT ROWID; \
             INSERT INTO w SELECT printf('c%06d', value), value FROM generate_series(1, 20000);",
        )
        .expect("the tables fill");
    every_duplicate_is_refused(&session, "c", "a, b", 30_000);
    let partial = session
        .execute("INSERT OR IGNORE INTO p(k, live) SELECT k, 1 FROM p", &[])
        .expect("the partial index insert runs");
    assert_eq!(partial, 0, "the partial unique index took duplicates");
    let outside = session
        .execute(
            "INSERT INTO p(k, live) SELECT k, 0 FROM p WHERE k <= 10",
            &[],
        )
        .expect("rows outside the partial index are not constrained");
    assert_eq!(outside, 10);
    let without_rowid = session
        .execute(
            "INSERT OR IGNORE INTO w SELECT 'd' || substr(code, 2), n FROM w",
            &[],
        )
        .expect("the WITHOUT ROWID insert runs");
    assert_eq!(
        without_rowid, 0,
        "the WITHOUT ROWID table's unique index took duplicates"
    );
    assert_eq!(integrity(&session), "ok");
    drop(database);
    let reopened = Database::open(&path).expect("the database reopens");
    let session = reopened.session();
    assert_eq!(integrity(&session), "ok");
    assert_eq!(integer(&session, "SELECT count(*) FROM w"), 20_000);
}

//! `PRAGMA data_version` and `PRAGMA schema_version` across two connections of one file.
//!
//! Invariant: **`data_version` changes when another connection commits and not when the
//! connection reads its own commit, and `schema_version` is the same number on both connections
//! after either one changes the schema.** `data_version` was the constant 1, so an application
//! that polls it to learn whether to refresh a cache never refreshed. `schema_version` on the
//! second connection stayed at the value it had when it opened, because the cookie a schema change
//! writes reaches the file with the writer's next fold and the second connection replays the
//! change from the log.
//!
//! The sessions of one `Database` are separate engines over the one file, the same as two
//! processes, so these tests exercise the path two processes take.

use inillucent_base::testing::ScratchDir;
use inillucent_engine::connect::{Connection, Database};
use inillucent_tree::datum::OwnedDatum;

/// A scratch directory of this test's own, removed when the returned guard drops.
///
/// @param name - what to call it
fn scratch(name: &str) -> ScratchDir {
    ScratchDir::new(&format!(
        "inillucent-pragma-versions-{}-{name}",
        std::process::id()
    ))
}

/// Reads the one integer a pragma answers.
///
/// @param connection - the connection to ask
/// @param pragma - the pragma statement
fn integer_of(connection: &Connection<'_>, pragma: &str) -> i64 {
    let rows = connection.query(pragma).expect("the pragma answers");
    match rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(value)) => *value,
        other => panic!("an integer was expected from {pragma}, and {other:?} came back"),
    }
}

/// `data_version` moves by one when the other connection commits, whatever that commit was, and
/// stays where it is for this connection's own commits and for repeated reads.
#[test]
fn data_version_changes_for_another_connections_commit_and_not_for_its_own() {
    let directory = scratch("data_version");
    let database = Database::open(directory.join("versions.rdb")).expect("the file opens");
    let first = database.session();
    let second = database.session();
    let start = integer_of(&first, "PRAGMA data_version");
    assert_eq!(
        integer_of(&second, "PRAGMA data_version"),
        start,
        "two connections that have only opened the file agree"
    );
    first
        .execute_batch("CREATE TABLE t (a INTEGER)")
        .expect("the table is made");
    assert_eq!(
        integer_of(&first, "PRAGMA data_version"),
        start,
        "a connection's own commit does not move its data_version"
    );
    assert_eq!(
        integer_of(&second, "PRAGMA data_version"),
        start + 1,
        "the other connection sees the commit"
    );
    assert_eq!(
        integer_of(&second, "PRAGMA data_version"),
        start + 1,
        "and a second look at the same commit changes nothing"
    );
    second
        .execute_batch("INSERT INTO t VALUES (1)")
        .expect("the insert");
    assert_eq!(
        integer_of(&second, "PRAGMA data_version"),
        start + 1,
        "the second connection's own insert does not move its data_version"
    );
    assert_eq!(
        integer_of(&first, "PRAGMA data_version"),
        start + 1,
        "the first connection sees the insert, which moves it from its own last value"
    );
}

/// A write that has not committed is not seen, and its commit is seen once.
#[test]
fn data_version_does_not_move_until_the_other_connection_commits() {
    let directory = scratch("data_version_commit");
    let database = Database::open(directory.join("versions.rdb")).expect("the file opens");
    let first = database.session();
    let second = database.session();
    first
        .execute_batch("CREATE TABLE t (a INTEGER)")
        .expect("the table is made");
    let seen = integer_of(&second, "PRAGMA data_version");
    first.execute_batch("BEGIN").expect("BEGIN");
    first
        .execute_batch("INSERT INTO t VALUES (1)")
        .expect("the insert");
    assert_eq!(
        integer_of(&second, "PRAGMA data_version"),
        seen,
        "an open transaction on the other connection has committed nothing"
    );
    first.execute_batch("COMMIT").expect("COMMIT");
    assert_eq!(
        integer_of(&second, "PRAGMA data_version"),
        seen + 1,
        "its commit moves data_version"
    );
    first
        .execute_batch("INSERT INTO t VALUES (2)")
        .expect("one more insert");
    assert_ne!(
        integer_of(&second, "PRAGMA data_version"),
        seen + 1,
        "a commit made since the last look moves it again"
    );
}

/// After either connection changes the schema both report the same `schema_version`, and it moves
/// by one for each change a connection looks at.
#[test]
fn schema_version_is_current_on_the_connection_that_did_not_change_the_schema() {
    let directory = scratch("schema_version");
    let database = Database::open(directory.join("versions.rdb")).expect("the file opens");
    let first = database.session();
    let second = database.session();
    let start = integer_of(&first, "PRAGMA schema_version");
    assert_eq!(integer_of(&second, "PRAGMA schema_version"), start);
    first
        .execute_batch("CREATE TABLE t (a INTEGER)")
        .expect("the table is made");
    assert_eq!(integer_of(&first, "PRAGMA schema_version"), start + 1);
    assert_eq!(
        integer_of(&second, "PRAGMA schema_version"),
        start + 1,
        "the second connection reads the first one's schema change"
    );
    first
        .execute_batch("ALTER TABLE t ADD COLUMN b INTEGER")
        .expect("the column is added");
    second
        .query("SELECT count(*) FROM t")
        .expect("the second connection reads the new table");
    assert_eq!(integer_of(&second, "PRAGMA schema_version"), start + 2);
    second
        .execute_batch("DROP TABLE t")
        .expect("the second connection drops it");
    assert_eq!(integer_of(&second, "PRAGMA schema_version"), start + 3);
    assert_eq!(
        integer_of(&first, "PRAGMA schema_version"),
        start + 3,
        "and the first connection reads the second one's change"
    );
}

/// A schema change in an attached database moves that database's `schema_version` and leaves
/// `main`'s alone, and `temp` has a counter of its own.
#[test]
fn schema_version_belongs_to_the_database_the_change_was_made_in() {
    let directory = scratch("schema_version_attached");
    let database = Database::open(directory.join("versions.rdb")).expect("the file opens");
    let connection = database.session();
    connection
        .execute_batch("ATTACH ':memory:' AS aux")
        .expect("attach");
    let main = integer_of(&connection, "PRAGMA main.schema_version");
    let aux = integer_of(&connection, "PRAGMA aux.schema_version");
    connection
        .execute_batch("CREATE TABLE aux.t (a INTEGER)")
        .expect("a table in aux");
    assert_eq!(
        integer_of(&connection, "PRAGMA aux.schema_version"),
        aux + 1
    );
    assert_eq!(integer_of(&connection, "PRAGMA main.schema_version"), main);
    connection
        .execute_batch("CREATE TEMP TABLE x (a INTEGER)")
        .expect("a table in temp");
    assert_eq!(integer_of(&connection, "PRAGMA main.schema_version"), main);
    assert_eq!(
        integer_of(&connection, "PRAGMA aux.schema_version"),
        aux + 1
    );
}

//! Two connections of one `Database` have a transaction each.
//!
//! Invariant: **a `BEGIN` on one connection is not joined by another, the
//! other connection reads only what is committed, a write through the other is
//! kept when the first connection rolls back, and a second writer is refused
//! with `busy` as a second process is.** Before this, every connection of a
//! `Database` borrowed one engine, so the second one read the first one's
//! uncommitted rows and its own write was undone by the first one's
//! `ROLLBACK`: an acknowledged write was lost with nothing reported. SQLite and
//! Python's `sqlite3` isolate two connections and refuse the second writer.

use inillucent_base::testing::ScratchDir;
use inillucent_base::PrimaryCode;
use inillucent_engine::connect::Database;
use inillucent_tree::datum::OwnedDatum;

/// A scratch directory of this test's own, removed when the returned guard drops.
///
/// @param name - what to call it
fn scratch(name: &str) -> ScratchDir {
    ScratchDir::new(&format!(
        "inillucent-connection-isolation-{}-{name}",
        std::process::id()
    ))
}

/// Reads the one number a `SELECT count(*)` answers.
///
/// @param rows - what the query returned
fn count_of(rows: Vec<Vec<OwnedDatum>>) -> i64 {
    match rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(count)) => *count,
        other => panic!("a count was expected, and {other:?} came back"),
    }
}

/// Makes a database holding an empty table `t`, and a second connection that does not wait for a
/// lock, so a refusal shows at once.
///
/// Returns the scratch directory guard with the database. The directory is
/// removed when the guard drops, so the caller binds the guard first and keeps
/// it for as long as the database is open.
///
/// @param name - the test's name
fn database_with_a_table(name: &str) -> (ScratchDir, Database) {
    let directory = scratch(name);
    let database = Database::open(directory.join("isolated.rdb")).expect("the file opens");
    // Session 1 is the one the test's first `session()` would get, so the first connection of
    // each test is the one the open made.
    database
        .session_as(1)
        .execute_batch("CREATE TABLE t (a INTEGER)")
        .expect("the table is made");
    (directory, database)
}

/// A `BEGIN` on one connection is not joined by the other, and the other reads nothing of it.
#[test]
fn a_second_connection_does_not_read_the_first_ones_uncommitted_rows() {
    let (_scratch, database) = database_with_a_table("dirty_read");
    let first = database.session();
    let second = database.session();
    first.execute_batch("BEGIN").expect("BEGIN");
    first
        .execute_batch("INSERT INTO t VALUES (1)")
        .expect("the insert");
    assert_eq!(
        count_of(first.query("SELECT count(*) FROM t").expect("first reads")),
        1,
        "the connection that wrote reads its own row"
    );
    assert_eq!(
        count_of(
            second
                .query("SELECT count(*) FROM t")
                .expect("second reads")
        ),
        0,
        "the other connection reads what is committed"
    );
    assert!(
        !first.autocommit().expect("first answers"),
        "the first connection is inside its transaction"
    );
    assert!(
        second.autocommit().expect("second answers"),
        "the second connection did not join it"
    );
    first.execute_batch("COMMIT").expect("COMMIT");
    assert_eq!(
        count_of(
            second
                .query("SELECT count(*) FROM t")
                .expect("second reads")
        ),
        1,
        "after the commit the other connection reads the row"
    );
}

/// A write through the second connection is not undone by the first one's `ROLLBACK`.
#[test]
fn a_rollback_on_one_connection_does_not_undo_a_write_through_another() {
    let (_scratch, database) = database_with_a_table("lost_write");
    let first = database.session();
    let second = database.session();
    first.execute_batch("BEGIN").expect("BEGIN");
    second
        .execute_batch("INSERT INTO t VALUES (7)")
        .expect("the second connection's write is acknowledged");
    first.execute_batch("ROLLBACK").expect("ROLLBACK");
    assert_eq!(
        count_of(first.query("SELECT count(*) FROM t").expect("first reads")),
        1,
        "the acknowledged write is still there for the connection that rolled back"
    );
    assert_eq!(
        count_of(
            second
                .query("SELECT count(*) FROM t")
                .expect("second reads")
        ),
        1,
        "and for the connection that made it"
    );
}

/// A second writer is refused with `busy`, as a writer in a second process is.
#[test]
fn a_second_writer_is_refused_busy_while_the_first_holds_the_file() {
    let (_scratch, database) = database_with_a_table("busy");
    let first = database.session();
    let second = database.session();
    second
        .execute_batch("PRAGMA busy_timeout = 0")
        .expect("the second connection does not wait");
    first.execute_batch("BEGIN").expect("BEGIN");
    first
        .execute_batch("INSERT INTO t VALUES (1)")
        .expect("the first writer holds the file");
    let refused = second
        .execute_batch("INSERT INTO t VALUES (2)")
        .expect_err("the second writer is refused");
    assert_eq!(refused.code(), PrimaryCode::Busy, "{refused:?}");
    first.execute_batch("COMMIT").expect("COMMIT");
    second
        .execute_batch("INSERT INTO t VALUES (2)")
        .expect("once the first has committed the second may write");
    assert_eq!(
        count_of(first.query("SELECT count(*) FROM t").expect("reads")),
        2
    );
}

/// `temp` tables, which were already scoped to a connection, stay scoped to it.
#[test]
fn a_temporary_table_belongs_to_the_connection_that_made_it() {
    let (_scratch, database) = database_with_a_table("temp");
    let first = database.session();
    let second = database.session();
    first
        .execute_batch("CREATE TEMP TABLE scratch (a INTEGER); INSERT INTO scratch VALUES (1)")
        .expect("the temp table is made");
    assert!(
        second.query("SELECT count(*) FROM scratch").is_err(),
        "the other connection has no such table"
    );
}

/// A session continued with `session_as` runs on the same engine, so its transaction goes on.
#[test]
fn a_continued_session_keeps_its_transaction() {
    let (_scratch, database) = database_with_a_table("continued");
    let _first = database.session();
    let number = database.session().session();
    database
        .session_as(number)
        .execute_batch("BEGIN; INSERT INTO t VALUES (1)")
        .expect("the session opens a transaction");
    let again = database.session_as(number);
    assert!(
        !again.autocommit().expect("answers"),
        "a connection that continues the session is inside the same transaction"
    );
    again.execute_batch("COMMIT").expect("COMMIT");
    assert_eq!(
        count_of(
            database
                .session()
                .query("SELECT count(*) FROM t")
                .expect("reads")
        ),
        1
    );
}

/// Releasing a session rolls its transaction back and gives the file to the next writer.
#[test]
fn releasing_a_session_ends_its_transaction() {
    let (_scratch, database) = database_with_a_table("released");
    let _first = database.session();
    let number = database.session().session();
    database
        .session_as(number)
        .execute_batch("BEGIN; INSERT INTO t VALUES (1)")
        .expect("the session holds the file");
    let writer = database.session();
    writer
        .execute_batch("PRAGMA busy_timeout = 0")
        .expect("the writer does not wait");
    assert!(
        writer.execute_batch("INSERT INTO t VALUES (2)").is_err(),
        "the file is held while the session lives"
    );
    database.release_session(number);
    writer
        .execute_batch("INSERT INTO t VALUES (2)")
        .expect("the released session no longer holds the file");
    assert_eq!(
        count_of(writer.query("SELECT count(*) FROM t").expect("reads")),
        1,
        "the released session's row was rolled back"
    );
}

/// A `:memory:` database has isolated connections too.
#[test]
fn two_connections_of_a_memory_database_have_a_transaction_each() {
    let database = Database::open(":memory:").expect("it opens");
    let first = database.session();
    first
        .execute_batch("CREATE TABLE t (a INTEGER)")
        .expect("the table is made");
    let second = database.session();
    second
        .execute_batch("PRAGMA busy_timeout = 0")
        .expect("the second connection does not wait");
    first
        .execute_batch("BEGIN; INSERT INTO t VALUES (1)")
        .expect("the first connection writes");
    assert_eq!(
        count_of(second.query("SELECT count(*) FROM t").expect("reads")),
        0
    );
    let refused = second
        .execute_batch("INSERT INTO t VALUES (2)")
        .expect_err("the second writer is refused");
    assert_eq!(refused.code(), PrimaryCode::Busy, "{refused:?}");
    first.execute_batch("ROLLBACK").expect("ROLLBACK");
}

/// Releasing the first session rolls its transaction back too, though its engine stays.
#[test]
fn releasing_the_first_session_ends_its_transaction() {
    let (_scratch, database) = database_with_a_table("released_first");
    let first = database.session();
    let number = first.session();
    first
        .execute_batch("BEGIN; INSERT INTO t VALUES (1)")
        .expect("the first session holds the file");
    let writer = database.session();
    writer
        .execute_batch("PRAGMA busy_timeout = 0")
        .expect("the writer does not wait");
    assert!(writer.execute_batch("INSERT INTO t VALUES (2)").is_err());
    database.release_session(number);
    writer
        .execute_batch("INSERT INTO t VALUES (2)")
        .expect("the file is free once the first session is released");
    assert_eq!(
        count_of(writer.query("SELECT count(*) FROM t").expect("reads")),
        1
    );
}

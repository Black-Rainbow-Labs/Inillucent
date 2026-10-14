//! A connection sees every commit another connection made, also when the other connection's log
//! rolled to a new segment without a fold.
//!
//! Invariant: **a statement that takes the file lock answers from every commit in the log, whether
//! the commit is in the segment this connection last saw or in one the log rolled to since, and a
//! write appends after the last record of the whole log and not after the last record of the
//! segment this connection had open.**
//!
//! ## What is tested
//!
//! `Wal::append` rolls to the next segment when the open segment holds `segment_bytes`, and a roll
//! writes no meta record. A connection in another process that asked only whether the meta record
//! moved and whether its own open segment grew would answer "nothing moved" and read its cached
//! pages without the commit. If it then wrote, it would append to the old segment at positions the
//! new segment already used. Two handles on one file take the lock against each other exactly as two
//! processes do, so these cases use two handles, with a segment size small enough that every
//! statement rolls. `inillucent_wal::with_segment_bytes` sets that size for the engine's own opens.
//!
//! A third handle holds a read lock in some cases, so the first connection's fold is put off.

use std::path::PathBuf;

use inillucent_compat::workspace_root;
use inillucent_driver::{Database, OpenOptions, Value};

/// The segment size every case runs with: the smallest the log accepts, so any record fills it.
const TINY_SEGMENT_BYTES: u64 = 128;

/// Returns a fresh database path for one case.
///
/// @param name - the case's name, which is also its directory's
fn fresh(name: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output/log-roll-between-connections")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("the case's directory was not made: {error}"));
    directory.join("rolls.rdb")
}

/// Opens one connection that waits for no lock, so a refusal shows at once.
///
/// @param path - the database file
fn open(path: &std::path::Path) -> Database {
    Database::open_with(
        path,
        OpenOptions {
            busy_timeout: Some(std::time::Duration::from_millis(0)),
            ..OpenOptions::default()
        },
    )
    .unwrap_or_else(|error| panic!("open: {error:?}"))
}

/// Returns how many rows a table holds as this connection sees them.
///
/// @param session - the connection to ask
/// @param table - the table, with its schema name when it is attached
fn count_in(session: &inillucent_driver::Connection<'_>, table: &str) -> i64 {
    let rows = session
        .query_all(&format!("SELECT count(*) FROM {table}"), &[])
        .unwrap_or_else(|error| panic!("count: {error:?}"));
    match rows.value(0, 0) {
        Some(Value::Integer(count)) => *count,
        other => panic!("count(*) answered {other:?}"),
    }
}

/// Returns how many rows `t` holds as this connection sees them.
///
/// @param session - the connection to ask
fn count_of(session: &inillucent_driver::Connection<'_>) -> i64 {
    count_in(session, "t")
}

/// Inserts one row into a table through one connection.
///
/// @param session - the connection to write through
/// @param table - the table, with its schema name when it is attached
/// @param n - the row's value
fn insert_into(session: &inillucent_driver::Connection<'_>, table: &str, n: i64) {
    session
        .execute(
            &format!("INSERT INTO {table} VALUES (?1)"),
            &[Value::Integer(n)],
        )
        .unwrap_or_else(|error| panic!("insert {n}: {error:?}"));
}

/// Inserts one row into `t` through one connection.
///
/// @param session - the connection to write through
/// @param n - the row's value
fn insert(session: &inillucent_driver::Connection<'_>, n: i64) {
    insert_into(session, "t", n);
}

/// Returns how many log segments sit beside the database.
///
/// @param path - the database file
fn segments_beside(path: &std::path::Path) -> usize {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    let directory = path.parent().map(PathBuf::from).unwrap_or_default();
    std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    let found = entry.file_name().to_string_lossy().to_string();
                    found.starts_with(&format!("{name}-wal."))
                })
                .count()
        })
        .unwrap_or(0)
}

/// Two connections take turns writing and reading while every statement rolls the log, and each
/// sees all of the other's rows.
#[test]
fn each_connection_sees_the_other_ones_commits_across_rolls() {
    let path = fresh("alternate");
    inillucent_wal::with_segment_bytes(TINY_SEGMENT_BYTES, || {
        let a = open(&path);
        let one = a.session();
        one.execute_batch("CREATE TABLE t (n INTEGER)")
            .unwrap_or_else(|error| panic!("setup: {error:?}"));
        let b = open(&path);
        let two = b.session();
        for round in 0..20i64 {
            insert(&one, round * 2);
            assert_eq!(
                count_of(&two),
                round * 2 + 1,
                "round {round}: the second connection missed a commit the first one made in a rolled segment"
            );
            insert(&two, round * 2 + 1);
            assert_eq!(
                count_of(&one),
                round * 2 + 2,
                "round {round}: the first connection missed a commit the second one made"
            );
        }
        assert!(
            segments_beside(&path) > 2,
            "the case rolled the log only {} times, so it did not exercise a roll",
            segments_beside(&path)
        );
    });
    inillucent_wal::with_segment_bytes(TINY_SEGMENT_BYTES, || {
        let reopened = open(&path);
        let session = reopened.session();
        assert_eq!(count_of(&session), 40, "a row was lost across the rolls");
        session
            .execute_batch("PRAGMA integrity_check")
            .unwrap_or_else(|error| panic!("integrity_check: {error:?}"));
    });
}

/// A connection that read while the log sat at the end of a full segment sees the commit the other
/// connection makes in the next segment, and its own write goes after that commit.
#[test]
fn a_write_after_a_roll_without_a_fold_appends_after_the_other_connections_commit() {
    let path = fresh("held-fold");
    inillucent_wal::with_segment_bytes(TINY_SEGMENT_BYTES, || {
        let a = open(&path);
        let one = a.session();
        one.execute_batch("CREATE TABLE t (n INTEGER)")
            .unwrap_or_else(|error| panic!("setup: {error:?}"));
        insert(&one, 0);
        let b = open(&path);
        let two = b.session();
        // The third connection keeps a read lock for the whole case, so a fold is put off.
        let c = open(&path);
        let three = c.session();
        three
            .execute_batch("BEGIN; SELECT count(*) FROM t")
            .unwrap_or_else(|error| panic!("the reader: {error:?}"));
        // The second connection reads while the first one's segment is full.
        assert_eq!(count_of(&two), 1);
        // The first connection's next append rolls, and its commit lands in the new segment.
        insert(&one, 1);
        // Nothing but the log tells the second connection, and it writes next.
        insert(&two, 2);
        assert_eq!(
            count_of(&two),
            3,
            "the second connection wrote from pages that lacked the first one's commit"
        );
        assert_eq!(count_of(&one), 3, "the first connection lost a row");
        three
            .execute_batch("ROLLBACK")
            .unwrap_or_else(|error| panic!("the reader's end: {error:?}"));
    });
    inillucent_wal::with_segment_bytes(TINY_SEGMENT_BYTES, || {
        let reopened = open(&path);
        let session = reopened.session();
        assert_eq!(count_of(&session), 3);
    });
}

/// A file attached to a connection shows the commits another connection makes to it while its log
/// rolls after every record, and the attaching connection's writes reach the other one.
#[test]
fn an_attached_file_sees_the_other_ones_commits_across_rolls() {
    let path = fresh("attached");
    let other = path.with_file_name("other.rdb");
    inillucent_wal::with_segment_bytes(TINY_SEGMENT_BYTES, || {
        let a = open(&other);
        let one = a.session();
        one.execute_batch("CREATE TABLE t (n INTEGER)")
            .unwrap_or_else(|error| panic!("setup: {error:?}"));
        let b = open(&path);
        let two = b.session();
        two.execute_batch(&format!(
            "ATTACH '{}' AS o",
            other.to_string_lossy().replace('\\', "/")
        ))
        .unwrap_or_else(|error| panic!("ATTACH: {error:?}"));
        for round in 0..10i64 {
            insert(&one, round * 2);
            assert_eq!(
                count_in(&two, "o.t"),
                round * 2 + 1,
                "round {round}: the attaching connection missed a commit made in a rolled segment"
            );
            insert_into(&two, "o.t", round * 2 + 1);
            assert_eq!(
                count_of(&one),
                round * 2 + 2,
                "round {round}: the owning connection missed a commit made through the attachment"
            );
        }
    });
}

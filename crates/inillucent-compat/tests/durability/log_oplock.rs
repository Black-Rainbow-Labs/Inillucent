//! A reader that keeps the log segment's length in memory still sees every commit of another
//! process.
//!
//! Invariant: **a statement that starts after another process's commit returned sees that
//! commit**, whether or not this process held the segment's oplock when the other process opened
//! the segment.
//!
//! ## Why this exists
//!
//! On Windows a process now holds a batch oplock on its open log segment (task-2209,
//! `inillucent_vfs::os::windows_log`). While it holds one, it answers "has the log grown" from a
//! length it keeps itself instead of asking the kernel before every statement, which made an
//! autocommit point read 3.5 times faster. That is only sound because another process cannot open
//! the segment until this process has acknowledged the oplock's break, and this process stops
//! trusting its own length before it acknowledges. The first test checks the property that
//! depends on that ordering, with a second process that is the shipped command line, so the
//! other side is a separate file object in a separate process exactly as an older build would be.
//! The second checks that the oplock is held at all, and given back, by counting the length
//! queries, so a change that quietly stopped sharing the segment would fail here too.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use inillucent_compat::cliproc::program;
use inillucent_compat::workspace_root;
use inillucent_engine::connect::{Connection, Database};

/// A scratch directory for one test, emptied first.
///
/// @param name - the test's name
fn area(name: &str) -> PathBuf {
    let path = workspace_root().join("_agent_output/log-oplock").join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Returns how many rows `note` holds.
///
/// @param connection - the connection to ask
fn count(connection: &Connection<'_>) -> i64 {
    let rows = connection
        .query("SELECT count(*) FROM note")
        .expect("the count runs");
    match rows.first().and_then(|row| row.first()) {
        Some(inillucent_tree::datum::OwnedDatum::Int(count)) => *count,
        other => panic!("a count of {other:?}"),
    }
}

/// Inserts one row through the command line, a process of its own, and waits for it to finish.
///
/// @param database - the file
/// @param n - the row
fn insert_elsewhere(database: &Path, n: i64) {
    let output = Command::new(program("inillucent"))
        .arg("--db")
        .arg(database)
        .arg("exec")
        .arg(format!("INSERT INTO note (n) VALUES ({n})"))
        .output()
        .expect("the command line starts");
    assert!(
        output.status.success(),
        "the insert of {n} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A commit made by another process is seen by the very next statement here, forty times, with
/// pauses between some of them long enough for this process to take the oplock back.
#[test]
fn a_commit_by_another_process_is_seen_by_the_next_read() {
    let directory = area("seen");
    let path = directory.join("shared.rdb");
    let database = Database::open(&path).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch("CREATE TABLE note (n INTEGER PRIMARY KEY)")
        .expect("the table is made");
    for n in 1..=40i64 {
        // Reads first, and on every other row a pause, so the oplock is held when the other
        // process opens the segment on some rows and already broken on others.
        for _ in 0..20 {
            count(&connection);
        }
        if n % 2 == 0 {
            std::thread::sleep(Duration::from_millis(250));
            count(&connection);
        }
        insert_elsewhere(&path, n);
        assert_eq!(
            count(&connection),
            n,
            "the read after another process committed row {n} did not see it"
        );
    }
}

/// Returns the newest log segment beside a database.
///
/// @param database - the file
fn newest_segment(database: &Path) -> PathBuf {
    let directory = database.parent().expect("a directory");
    let prefix = format!(
        "{}-wal.",
        database
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
    );
    let mut segments: Vec<PathBuf> = std::fs::read_dir(directory)
        .expect("the directory lists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect();
    segments.sort();
    segments.pop().expect("a log segment")
}

/// Runs a hundred point reads and answers how many log length queries they made.
///
/// @param statement - the prepared point read
fn queries_over_a_hundred_reads(statement: &mut inillucent_engine::connect::Statement<'_>) -> u64 {
    let before = inillucent_vfs::os::log_size_queries();
    for id in 0..100i64 {
        statement.reset();
        statement
            .bind_integer(1, id % 50 + 1)
            .expect("the id binds");
        assert!(statement.step().expect("the read runs"));
    }
    inillucent_vfs::os::log_size_queries().saturating_sub(before)
}

/// Waits up to two seconds for a hundred reads to make no log length query, and answers whether
/// they did.
///
/// @param statement - the prepared point read
fn becomes_solo(statement: &mut inillucent_engine::connect::Statement<'_>) -> bool {
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        if queries_over_a_hundred_reads(statement) == 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// The oplock takes the length query off a read while no other handle has the segment, gives it
/// back while one does, and is taken again once that handle closes.
#[cfg(windows)]
#[test]
fn the_oplock_takes_the_length_query_off_a_read_and_gives_it_back() {
    let directory = area("queries");
    let path = directory.join("shared.rdb");
    let database = Database::open(&path).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE note (n INTEGER PRIMARY KEY); \
             WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 50) \
             INSERT INTO note SELECT i FROM c;",
        )
        .expect("the table is filled");
    let mut statement = connection
        .prepare("SELECT n FROM note WHERE n = ?1")
        .expect("the read prepares");
    assert!(
        becomes_solo(&mut statement),
        "reads kept asking the log its length with no other handle open"
    );
    let foreign = std::fs::File::open(newest_segment(&path)).expect("a second handle opens");
    let asked = queries_over_a_hundred_reads(&mut statement);
    assert!(
        asked >= 90,
        "with another handle on the segment a hundred reads asked its length only {asked} times"
    );
    drop(foreign);
    assert!(
        becomes_solo(&mut statement),
        "the oplock was not taken back after the other handle closed"
    );
}

/// A database opened at a path whose files were deleted while an older database still holds
/// them writes its log to the new segment on disk, not to the deleted one.
///
/// `walperf`'s recovery measurement leaks a `Database`, deletes its files and opens the same path
/// again. The open found the leaked database's shared segment, which still pointed at the deleted
/// file, so every commit went to a file with no name, and a later open read no tables.
#[test]
fn a_segment_deleted_under_an_open_database_is_not_reused() {
    let directory = area("deleted");
    let path = directory.join("reused.rdb");
    let older = Database::open(&path).expect("the first database opens");
    let older_connection = older.session();
    older_connection
        .execute_batch("CREATE TABLE note (n INTEGER PRIMARY KEY); INSERT INTO note VALUES (1)")
        .expect("the first database writes");
    let mut names: Vec<PathBuf> = std::fs::read_dir(&directory)
        .expect("the directory lists")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    names.sort();
    for name in &names {
        std::fs::remove_file(name).expect("a file of the first database deletes");
    }
    let newer = Database::open(&path).expect("the second database opens");
    let newer_connection = newer.session();
    newer_connection
        .execute_batch(
            "PRAGMA wal_autocheckpoint = 0; CREATE TABLE note (n INTEGER PRIMARY KEY); \
             INSERT INTO note VALUES (1), (2), (3)",
        )
        .expect("the second database writes");
    let segment = newest_segment(&path);
    let on_disk = std::fs::metadata(&segment)
        .expect("the segment on disk reads")
        .len();
    assert!(
        on_disk > 0,
        "the second database's commits are not in {} on disk",
        segment.display()
    );
    // A third open reads the file on disk, and must find all three rows.
    let copy = directory.join("copy.rdb");
    std::fs::copy(&path, &copy).expect("the database file copies");
    let copied_segment = directory.join(format!(
        "copy.rdb-wal.{}",
        segment
            .extension()
            .and_then(|number| number.to_str())
            .unwrap_or_default()
    ));
    std::fs::copy(&segment, &copied_segment).expect("the segment copies");
    let reopened = Database::open(&copy).expect("the copy opens");
    assert_eq!(count(&reopened.session()), 3);
    // The first database stays open until here, as the leaked one in `walperf` does.
    drop(older);
}

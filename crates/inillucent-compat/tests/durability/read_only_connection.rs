//! A connection opened read only sees what other processes commit.
//!
//! Invariant: **a read only connection that stays open reads each later commit of another process,
//! the same as a connection opened for writing does.**
//!
//! ## Why this exists
//!
//! `inillucent-shell -readonly` began opening its connection read only in the engine during bug hunt
//! 9, and a long lived read only shell then answered every query from the state at its first
//! statement, so `durability::process_journal_handoff` failed. This holds the engine's read only
//! connection to the rule directly, through the driver, with the writes made by another process.
//!
//! A writer that ends folds its log into the file and moves the meta record, so the first case
//! sees each commit through the meta record. A writer that stays open leaves its commits in the
//! log, and the second case reads those.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc::program;
use inillucent_compat::workspace_root;
use inillucent_driver::{Database, OpenOptions, Value};

/// Returns an empty scratch directory for one test.
///
/// @param name - the test's name
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/read-only-connection")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs a script through `inillucent-shell` on a file, in its own process, and waits for it.
///
/// @param path - the database file
/// @param script - the statements, fed on standard input
fn shell(path: &Path, script: &str) {
    let mut child = Command::new(program("inillucent-shell"))
        .arg(path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("the shell starts");
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(script.as_bytes())
            .expect("the script is written");
    }
    drop(child.stdin.take());
    child.wait().expect("the shell exits");
}

/// Returns the one integer a query answers.
///
/// @param connection - the connection
/// @param sql - a query answering one integer
fn integer(connection: &inillucent_driver::Connection<'_>, sql: &str) -> i64 {
    let rows = connection.query(sql, &[], 1).expect("the query runs");
    match rows.value(0, 0) {
        Some(Value::Integer(number)) => *number,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Three commits by another process, each read by one read only connection that stays open.
#[test]
fn a_read_only_connection_reads_each_later_commit() {
    let path = area("later-commits").join("case.rdb");
    shell(
        &path,
        "CREATE TABLE t(id INTEGER PRIMARY KEY, v); \
         INSERT INTO t SELECT value, 1 FROM generate_series(1, 2000);\n",
    );
    let options = OpenOptions {
        read_only: true,
        create: false,
        ..OpenOptions::default()
    };
    let database = Database::open_with(&path, options).expect("the file opens read only");
    // One connection held across the commits, as a shell or a server holds it.
    let reader = database.session();
    assert_eq!(integer(&reader, "SELECT sum(v) FROM t"), 2000);
    for n in 1..=3i64 {
        shell(&path, "UPDATE t SET v = v + 1 WHERE id <= 100;\n");
        assert_eq!(
            integer(&reader, "SELECT sum(v) FROM t"),
            2000 + 100 * n,
            "the read only connection missed commit {n} of another process"
        );
    }
}

/// Three commits by another process that stays open, each read by one read only connection.
///
/// The writer does not fold while it is open, so its commits are only in the log. A read only
/// connection's own log is a scratch one in memory, and it used to measure that one to decide
/// whether the log had moved: it answered 2000 three times, and `inillucent-shell -readonly`
/// opened the engine for writing to get around it.
#[test]
fn a_read_only_connection_reads_what_a_live_writer_left_in_the_log() {
    let path = area("live-writer").join("case.rdb");
    shell(
        &path,
        "CREATE TABLE t(id INTEGER PRIMARY KEY, v); \
         INSERT INTO t SELECT value, 1 FROM generate_series(1, 2000);\n",
    );
    let options = OpenOptions {
        read_only: true,
        create: false,
        ..OpenOptions::default()
    };
    let database = Database::open_with(&path, options).expect("the file opens read only");
    let reader = database.session();
    assert_eq!(integer(&reader, "SELECT sum(v) FROM t"), 2000);
    let mut writer = Command::new(program("inillucent-shell"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the writer starts");
    let mut input = writer.stdin.take().expect("the writer's input");
    let mut output = BufReader::new(writer.stdout.take().expect("the writer's output"));
    for n in 1..=3i64 {
        input
            .write_all(b"UPDATE t SET v = v + 1 WHERE id <= 100;\nSELECT 'committed';\n")
            .and_then(|()| input.flush())
            .expect("the update is sent");
        let mut line = String::new();
        while line.trim() != "committed" {
            line.clear();
            let read = output.read_line(&mut line).expect("the writer answers");
            assert!(read > 0, "the writer ended before commit {n}");
        }
        assert_eq!(
            integer(&reader, "SELECT sum(v) FROM t"),
            2000 + 100 * n,
            "the read only connection missed commit {n} of a writer that is still open"
        );
    }
    drop(input);
    writer.wait().expect("the writer exits");
}

/// Returns the log segment with the highest sequence beside a database file.
///
/// @param path - the database file
fn last_segment(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| format!("{}-wal.", name.to_string_lossy()))
        .expect("the database has a file name");
    let directory = path.parent().expect("the database has a directory");
    let mut segments: Vec<PathBuf> = std::fs::read_dir(directory)
        .expect("the directory lists")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|candidate| {
            candidate
                .file_name()
                .is_some_and(|file| file.to_string_lossy().starts_with(&name))
        })
        .collect();
    segments.sort();
    segments.pop().expect("the database has a log segment")
}

/// A log whose last record was cut short, read by a read only connection that stays open.
///
/// A writer that dies in the middle of an append leaves bytes after the last whole record. A
/// writable open cuts them off, and a read only open cannot. The read only connection asks the
/// segment's length on disk whether another process appended, and the cut short bytes make that
/// length differ from where its replay ended, so every statement looked like a commit by another
/// process: `PRAGMA data_version` rose on each one and each one replayed the whole log again.
#[test]
fn a_read_only_connection_on_a_log_cut_short_sees_no_commit_that_did_not_happen() {
    let path = area("cut-short").join("case.rdb");
    shell(
        &path,
        "CREATE TABLE t(id INTEGER PRIMARY KEY, v); \
         INSERT INTO t SELECT value, 1 FROM generate_series(1, 2000);\n",
    );
    let segment = last_segment(&path);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&segment)
        .expect("the segment opens for appending");
    file.write_all(&[0xA5; 300])
        .expect("the cut short bytes are written");
    drop(file);
    let options = OpenOptions {
        read_only: true,
        create: false,
        ..OpenOptions::default()
    };
    let database = Database::open_with(&path, options).expect("the file opens read only");
    let reader = database.session();
    assert_eq!(integer(&reader, "SELECT sum(v) FROM t"), 2000);
    let before = integer(&reader, "PRAGMA data_version");
    for _ in 0..3 {
        assert_eq!(integer(&reader, "SELECT sum(v) FROM t"), 2000);
    }
    assert_eq!(
        integer(&reader, "PRAGMA data_version"),
        before,
        "no other process committed, so data_version stays where it was"
    );
    // A writer that stays open cuts the bytes off, appends its commit where they were and does
    // not fold, so the commit is only in the log and the reader has to find it there.
    let mut writer = Command::new(program("inillucent-shell"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the writer starts");
    let mut input = writer.stdin.take().expect("the writer's input");
    let mut output = BufReader::new(writer.stdout.take().expect("the writer's output"));
    input
        .write_all(b"UPDATE t SET v = v + 1 WHERE id <= 100;\nSELECT 'committed';\n")
        .and_then(|()| input.flush())
        .expect("the update is sent");
    let mut line = String::new();
    while line.trim() != "committed" {
        line.clear();
        let read = output.read_line(&mut line).expect("the writer answers");
        assert!(read > 0, "the writer ended before it committed");
    }
    assert_eq!(
        integer(&reader, "SELECT sum(v) FROM t"),
        2100,
        "the read only connection missed the commit of a writer that is still open"
    );
    assert!(
        integer(&reader, "PRAGMA data_version") > before,
        "another process committed, so data_version moved"
    );
    drop(input);
    writer.wait().expect("the writer exits");
}

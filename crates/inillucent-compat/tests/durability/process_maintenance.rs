//! A backup, a copy and a checkpoint run while another process holds a
//! transaction open, and each answers the way SQLite does.
//!
//! Invariant: **another process's open transaction does not stop a backup or
//! `VACUUM INTO`, and makes `PRAGMA wal_checkpoint` answer a row that says it
//! was busy rather than fail.**
//!
//! ## Why these cases exist
//!
//! A backup began with a checkpoint, and a checkpoint needs the file to
//! itself. So while another process held even a read transaction,
//! `inillucent backup`, `.backup`, `.clone` and `VACUUM INTO` waited out the
//! busy timeout and failed with "database is locked" or "readers are still
//! present", where SQLite copied in 40 milliseconds. `PRAGMA wal_checkpoint`
//! failed the same way in every mode, where SQLite returns its row with nothing
//! checkpointed. An application that backs up or checkpoints on a timer beside
//! its own readers met this every time. Found by the bug hunt's two process
//! hunter.
//!
//! The process holding the transaction is this one, through the driver; the
//! other is the `inillucent` command line.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use inillucent_compat::cliproc::{program, rows, run};
use inillucent_compat::workspace_root;
use inillucent_driver::{Database, Value};

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/process-maintenance")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Makes a database of 2,000 rows and opens a read transaction on it in this
/// process, which holds the file's shared lock until it ends.
///
/// @param database - the file
fn reading(database: &Path) -> Database {
    let opened = Database::open(database).expect("the database opens");
    let session = opened.session();
    session
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT); \
             INSERT INTO t SELECT value, 'row ' || value FROM generate_series(1, 2000);",
        )
        .expect("the table fills");
    session
        .execute("BEGIN", &[])
        .expect("the transaction opens");
    let counted = session
        .query_all("SELECT count(*) FROM t", &[])
        .expect("the read runs");
    assert_eq!(counted.rows, vec![vec![Value::Integer(2000)]]);
    opened
}

/// Runs a command line call against the database and returns it with how
/// long it took.
///
/// @param database - the file
/// @param arguments - the command and its arguments
fn other_process(
    database: &Path,
    arguments: &[&str],
) -> (inillucent_compat::cliproc::Ran, Duration) {
    let path = database.to_string_lossy().to_string();
    let mut all = vec!["--db", path.as_str()];
    all.extend_from_slice(arguments);
    let started = Instant::now();
    let ran = run(&program("inillucent"), &all);
    (ran, started.elapsed())
}

/// `inillucent backup` writes a complete copy while this process reads.
#[test]
fn a_backup_runs_beside_another_processes_read_transaction() {
    let directory = area("backup");
    let database = directory.join("app.rdb");
    let held = reading(&database);
    let copy = directory.join("copy.rdb");
    let (ran, took) = other_process(&database, &["backup", &copy.to_string_lossy()]);
    assert_eq!(
        ran.code,
        0,
        "the backup failed after {took:?}:\n{}",
        ran.said()
    );
    assert!(
        took < Duration::from_secs(4),
        "the backup waited {took:?} for the reader"
    );
    drop(held);
    let copied = other_process(
        &copy,
        &["query", "SELECT count(*) FROM t", "--output", "json"],
    )
    .0;
    assert_eq!(rows(&copied.stdout), vec![vec!["2000".to_string()]]);
    let check = other_process(
        &copy,
        &["query", "PRAGMA integrity_check", "--output", "json"],
    )
    .0;
    assert_eq!(rows(&check.stdout), vec![vec!["ok".to_string()]]);
}

/// `VACUUM INTO` writes a complete copy while this process reads.
#[test]
fn vacuum_into_runs_beside_another_processes_read_transaction() {
    let directory = area("vacuum-into");
    let database = directory.join("app.rdb");
    let held = reading(&database);
    let copy = directory.join("copy.rdb");
    let sql = format!(
        "VACUUM INTO '{}'",
        copy.to_string_lossy().replace('\\', "/")
    );
    let (ran, took) = other_process(&database, &["exec", &sql]);
    assert_eq!(
        ran.code,
        0,
        "VACUUM INTO failed after {took:?}:\n{}",
        ran.said()
    );
    drop(held);
    let copied = other_process(
        &copy,
        &["query", "SELECT count(*) FROM t", "--output", "json"],
    )
    .0;
    assert_eq!(rows(&copied.stdout), vec![vec!["2000".to_string()]]);
}

/// `PRAGMA wal_checkpoint` answers a row while this process reads: busy 0 for
/// PASSIVE, busy 1 for FULL, as SQLite does.
#[test]
fn a_checkpoint_beside_another_processes_read_transaction_answers_a_row() {
    let directory = area("checkpoint");
    let database = directory.join("app.rdb");
    let held = reading(&database);
    for (mode, busy) in [("PASSIVE", "0"), ("FULL", "1")] {
        let sql = format!("PRAGMA busy_timeout = 200; PRAGMA wal_checkpoint({mode})");
        let (ran, took) = other_process(&database, &["run", &sql, "--output", "json"]);
        assert_eq!(
            ran.code,
            0,
            "wal_checkpoint({mode}) failed after {took:?}:\n{}",
            ran.said()
        );
        assert!(
            ran.stdout.contains(&format!("\"busy\": {busy}"))
                || ran.stdout.contains(&format!("\"busy\":{busy}"))
                || ran.stdout.contains(&format!("{busy}|")),
            "wal_checkpoint({mode}) did not answer busy {busy}:\n{}",
            ran.stdout
        );
    }
    drop(held);
}

/// Pragmas that only read answer at once while this process holds a write
/// transaction open, as a query does.
///
/// They asked for the write lock, so each waited for this process's
/// transaction to end and failed after the busy timeout.
#[test]
fn reading_pragmas_answer_beside_another_processes_write_transaction() {
    let directory = area("reading-pragmas");
    let database = directory.join("app.rdb");
    let opened = Database::open(&database).expect("the database opens");
    let session = opened.session();
    session
        .execute_batch("CREATE TABLE t(a); INSERT INTO t VALUES(1);")
        .expect("the table is made");
    session
        .execute("BEGIN IMMEDIATE", &[])
        .expect("the write transaction opens");
    session
        .execute("INSERT INTO t VALUES(2)", &[])
        .expect("the write runs");
    for sql in [
        "PRAGMA quick_check",
        "PRAGMA journal_mode",
        "PRAGMA table_info(t)",
    ] {
        let (ran, took) = other_process(&database, &["query", sql, "--output", "json"]);
        assert_eq!(ran.code, 0, "{sql} failed after {took:?}:\n{}", ran.said());
        assert!(
            took < Duration::from_secs(3),
            "{sql} waited {took:?} for the writer"
        );
    }
    session
        .execute("COMMIT", &[])
        .expect("the transaction commits");
}

/// Inserts one numbered row at a time through this process's own connection
/// until `stop` is set, and returns the numbers whose insert was acknowledged.
///
/// An insert refused as busy, which a `VACUUM` in another process can cause
/// while it holds the file, is not acknowledged and is not counted.
///
/// @param database - the file
/// @param stop - set when the inserts should end
/// @param progress - raised by one for each acknowledged insert, so the
///   maintenance loop can see how far the writer has got
fn insert_until(database: &Path, stop: &AtomicBool, progress: &AtomicUsize) -> Vec<i64> {
    let opened = Database::open(database).expect("the database opens");
    let session = opened.session();
    let mut acknowledged = Vec::new();
    let mut next = 1i64;
    while !stop.load(Ordering::SeqCst) {
        if session
            .execute(
                "INSERT INTO t(id, v) VALUES (?1, 'x')",
                &[Value::Integer(next)],
            )
            .is_ok()
        {
            acknowledged.push(next);
            progress.fetch_add(1, Ordering::SeqCst);
        }
        next += 1;
        // An application leaves a moment between statements; a loop that
        // leaves none takes the write lock back before any other process can.
        std::thread::sleep(Duration::from_millis(2));
    }
    acknowledged
}

/// How many inserts the writer must have acknowledged beside the maintenance
/// steps before a case trusts its count. Fewer means the writer barely ran, and
/// a count of rows that were never at risk proves nothing.
const WRITER_FLOOR: usize = 50;

/// Runs one maintenance statement in another process, again and again, while
/// the writer inserts beside it, and returns how many steps ran.
///
/// The cases first ran a fixed number of steps and expected more than 50
/// inserts in that time. A quiet machine makes about 150 to 220. A two core CI
/// runner with three other suites beside it made 23 to 31, so the case failed
/// on the load of the machine and not on the engine. The loop now keeps
/// stepping until the writer is past the floor, so the inserts still happen
/// beside the steps however slow the machine is. It stops after two minutes,
/// and the case's own floor check then fails.
///
/// @param database - the file
/// @param statement - the maintenance statement, run through `inillucent exec`
/// @param minimum_steps - steps to run even when the writer is already past the floor
/// @param pause - the time left between steps for the writer
/// @param progress - the writer's count of acknowledged inserts
fn maintain_beside_writer(
    database: &Path,
    statement: &str,
    minimum_steps: usize,
    pause: Duration,
    progress: &AtomicUsize,
) -> usize {
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut steps = 0;
    while steps < minimum_steps
        || (progress.load(Ordering::SeqCst) <= WRITER_FLOOR && Instant::now() < deadline)
    {
        let (ran, _) = other_process(database, &["exec", statement]);
        assert_eq!(ran.code, 0, "{statement} runs: {}", ran.stderr);
        steps += 1;
        std::thread::sleep(pause);
    }
    steps
}

/// Starts the writer thread on a database and returns its stop flag, its
/// progress count and the thread.
///
/// @param database - the file
fn start_writer(
    database: &Path,
) -> (
    Arc<AtomicBool>,
    Arc<AtomicUsize>,
    std::thread::JoinHandle<Vec<i64>>,
) {
    let stop = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(AtomicUsize::new(0));
    let writer = {
        let database = database.to_path_buf();
        let stop = Arc::clone(&stop);
        let progress = Arc::clone(&progress);
        std::thread::spawn(move || insert_until(&database, &stop, &progress))
    };
    (stop, progress, writer)
}

/// A `VACUUM` in one process loses no row another process inserted beside it.
///
/// `VACUUM` writes a new file and renames it over the database. The process
/// inserting kept its handle on the old file and went on writing it, so its
/// commits after the rename, and any made during the rebuild, went to a file no
/// path named. The bug hunt's crash hunter measured 265 of 502 acknowledged
/// inserts missing, with no error and a clean `integrity_check`.
#[test]
fn vacuum_in_another_process_loses_no_acknowledged_insert() {
    let directory = area("vacuum-beside-writer");
    let database = directory.join("app.rdb");
    {
        let opened = Database::open(&database).expect("the database opens");
        opened
            .session()
            .execute_batch("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)")
            .expect("the table is made");
    }
    let (stop, progress, writer) = start_writer(&database);
    std::thread::sleep(Duration::from_millis(300));
    maintain_beside_writer(
        &database,
        "VACUUM",
        4,
        Duration::from_millis(150),
        &progress,
    );
    stop.store(true, Ordering::SeqCst);
    let acknowledged = writer.join().expect("the writer finishes");
    assert!(
        acknowledged.len() > WRITER_FLOOR,
        "the writer made progress: {}",
        acknowledged.len()
    );
    let (ran, _) = other_process(
        &database,
        &["query", "SELECT count(*) FROM t", "--output", "json"],
    );
    assert_eq!(ran.code, 0, "the count runs: {}", ran.stderr);
    let found = rows(&ran.stdout);
    assert_eq!(
        found,
        vec![vec![acknowledged.len().to_string()]],
        "every acknowledged insert is in the file"
    );
}

/// `PRAGMA incremental_vacuum` in one process loses no row another process
/// inserted beside it, and leaves a file that passes the integrity check.
///
/// Each step moves a page to a lower number and cuts the file. The process
/// inserting holds pages in its own pool, so it has to see each move through the
/// file lock and the log, and no insert it acknowledged may end up on a page
/// that was cut.
#[test]
fn incremental_vacuum_in_another_process_loses_no_acknowledged_row() {
    let directory = area("incremental-vacuum-beside-writer");
    let database = directory.join("app.rdb");
    {
        let opened = Database::open(&database).expect("the database opens");
        opened
            .session()
            .execute_batch(
                "PRAGMA auto_vacuum = incremental; \
                 CREATE TABLE junk(id INTEGER PRIMARY KEY, v TEXT); \
                 INSERT INTO junk SELECT value, hex(randomblob(400)) FROM generate_series(1, 3000); \
                 CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT); \
                 DELETE FROM junk WHERE id > 10;",
            )
            .expect("the tables are made and emptied");
    }
    let pages_of = |database: &Path| -> i64 {
        let (ran, _) = other_process(
            database,
            &["query", "PRAGMA page_count", "--output", "json"],
        );
        rows(&ran.stdout)[0][0].parse().expect("a page count")
    };
    let pages_before = pages_of(&database);
    let (stop, progress, writer) = start_writer(&database);
    std::thread::sleep(Duration::from_millis(300));
    maintain_beside_writer(
        &database,
        "PRAGMA incremental_vacuum(40)",
        6,
        Duration::from_millis(100),
        &progress,
    );
    stop.store(true, Ordering::SeqCst);
    let acknowledged = writer.join().expect("the writer finishes");
    let pages_after = pages_of(&database);
    assert!(
        pages_after < pages_before - 20,
        "the steps gave back no pages: {pages_before} before, {pages_after} after"
    );
    assert!(
        acknowledged.len() > WRITER_FLOOR,
        "the writer made progress: {}",
        acknowledged.len()
    );
    let (ran, _) = other_process(
        &database,
        &["query", "SELECT count(*) FROM t", "--output", "json"],
    );
    assert_eq!(ran.code, 0, "the count runs: {}", ran.stderr);
    assert_eq!(
        rows(&ran.stdout),
        vec![vec![acknowledged.len().to_string()]],
        "every acknowledged insert is in the file"
    );
    let (check, _) = other_process(
        &database,
        &["query", "PRAGMA integrity_check", "--output", "json"],
    );
    assert_eq!(rows(&check.stdout), vec![vec!["ok".to_string()]]);
}

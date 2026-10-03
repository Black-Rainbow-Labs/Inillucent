//! Two processes on one file, interleaved at a moment each case chooses, and
//! the file holds what both of them committed.
//!
//! Invariant: **an operation a program calls between statements (a checkpoint,
//! a backup, an export) sees every commit another process made before it, and
//! never writes this process's older copy of a page over that process's newer
//! one.**
//!
//! ## Why these cases exist
//!
//! A statement catches up with other processes on the way in: `enter` asks
//! whether the meta record or the log moved, and replays the log when either
//! did. The calls an application makes on the database handle rather than
//! through SQL did not all go through `enter`. `Database::checkpoint` took the
//! exclusive lock and folded this process's dirty pages into the file as they
//! stood, so a process that had committed, let another process commit on top,
//! and then asked for a checkpoint wrote its older pages over the other
//! process's commit. The multi process stress run in `process_storm.rs` found
//! it in its first minute: rows another process had deleted came back, and an
//! extent page was overwritten with a leaf, which `integrity-check` reported as
//! `a shared extent slot holds 5393 bytes where its reference says 7845`.
//!
//! Each case here is the smallest sequence that shows one such call, with the
//! other process played by the `inillucent` command line, which is how a second
//! program reaches a file in production.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc::{program, rows, run};
use inillucent_compat::workspace_root;
use inillucent_driver::{Database, OpenOptions, Value};

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/process-interleavings")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs one statement in another process and fails the case if it fails.
///
/// @param database - the file
/// @param sql - the statement
fn other_process_exec(database: &Path, sql: &str) {
    let ran = run(
        &program("inillucent"),
        &["--db", &database.to_string_lossy(), "exec", sql],
    );
    assert_eq!(
        ran.code,
        0,
        "the other process failed on {sql}:\n{}",
        ran.said()
    );
}

/// Returns the rows a query prints, read by a process that wrote none of them.
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

/// Runs `integrity-check` in another process and returns whether it passed,
/// with what it printed.
///
/// @param database - the file
fn integrity(database: &Path) -> (bool, String) {
    let ran = run(
        &program("inillucent"),
        &["--db", &database.to_string_lossy(), "integrity-check"],
    );
    (ran.code == 0, ran.said())
}

/// Opens the file in this process with a pool large enough to hold every
/// page, so a commit stays in the pool and the log until a fold.
///
/// @param database - the file
fn this_process(database: &Path) -> Database {
    Database::open_with(database, OpenOptions::default())
        .unwrap_or_else(|error| panic!("this process could not open the file: {error:?}"))
}

/// A checkpoint asked for through the driver, after another process committed
/// on top of this process's own unfolded commit, keeps the other commit.
///
/// The sequence: this process inserts ten rows and keeps the database open, so
/// the rows are in its pool and the log and not yet in the file. Another
/// process deletes five of them and exits, which folds its delete into the
/// file. This process then calls `Database::checkpoint`. Before the fix the
/// checkpoint wrote this process's pages, which still held all ten rows, over
/// the file, and the five deleted rows came back. With this case's
/// `UNIQUE(n)` the same overwrite also undid the other process's insert.
#[test]
fn a_checkpoint_called_between_statements_keeps_another_processes_commit() {
    let directory = area("checkpoint-after-another-commit");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch("CREATE TABLE note (n INTEGER NOT NULL UNIQUE, who TEXT NOT NULL)")
        .unwrap_or_else(|error| panic!("the table was not created: {error:?}"));
    for n in 1..=10 {
        session
            .execute(
                "INSERT INTO note (n, who) VALUES (?1, 'first')",
                &[Value::Integer(n)],
            )
            .unwrap_or_else(|error| panic!("insert {n} failed: {error:?}"));
    }

    other_process_exec(&database, "DELETE FROM note WHERE n > 5");
    other_process_exec(
        &database,
        "INSERT INTO note (n, who) VALUES (100, 'second')",
    );

    ours.checkpoint()
        .unwrap_or_else(|error| panic!("the checkpoint failed: {error:?}"));
    drop(ours);

    let found = other_process_rows(&database, "SELECT n, who FROM note ORDER BY n");
    let expected: Vec<Vec<String>> = (1..=5)
        .map(|n| vec![n.to_string(), "first".to_string()])
        .chain(std::iter::once(vec![
            "100".to_string(),
            "second".to_string(),
        ]))
        .collect();
    assert_eq!(
        found, expected,
        "the checkpoint wrote this process's older pages over the other process's delete and insert"
    );
    let (sound, said) = integrity(&database);
    assert!(sound, "the file failed its integrity check:\n{said}");
}

/// A process that opens the file while another holds it for writing reads the
/// other process's committed rows instead of failing with `busy`.
///
/// This process commits rows that grow the file, which stay in its pool and
/// the log, and then opens a second transaction and leaves it open, so it holds
/// RESERVED. A new process opening the file replays the committed rows, finds
/// the file has more pages than its header says, and used to try to fold them
/// into the file at once. That needs EXCLUSIVE, the RESERVED holder refused it
/// after a tenth of a second, and the open failed with `busy` whatever busy
/// timeout the caller had set. In the multi process stress run every reader
/// and every newly started writer died this way at open.
#[test]
fn an_open_beside_a_writer_holding_the_file_reads_its_committed_rows() {
    let directory = area("open-beside-a-reserved-writer");
    let database = directory.join("shared.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch(
            "CREATE TABLE note (n INTEGER PRIMARY KEY, body BLOB NOT NULL);
             WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < 400)
             INSERT INTO note (n, body) SELECT i, zeroblob(2000) FROM k;",
        )
        .unwrap_or_else(|error| panic!("the rows were not written: {error:?}"));
    let open = session
        .begin()
        .unwrap_or_else(|error| panic!("the second transaction did not start: {error:?}"));
    open.execute("INSERT INTO note (n, body) VALUES (401, x'00')", &[])
        .unwrap_or_else(|error| panic!("the held insert failed: {error:?}"));

    let ran = run(
        &program("inillucent"),
        &[
            "--db",
            &database.to_string_lossy(),
            "query",
            "SELECT count(*) FROM note",
            "--output",
            "json",
        ],
    );
    open.commit()
        .unwrap_or_else(|error| panic!("the held transaction did not commit: {error:?}"));
    assert_eq!(
        ran.code,
        0,
        "a process opening the file beside a writer failed instead of reading:\n{}",
        ran.said()
    );
    assert_eq!(rows(&ran.stdout), vec![vec!["400".to_string()]]);
    drop(ours);
    assert_eq!(
        other_process_rows(&database, "SELECT count(*) FROM note"),
        vec![vec!["401".to_string()]]
    );
    let (sound, said) = integrity(&database);
    assert!(sound, "the file failed its integrity check:\n{said}");
}

/// A shell process holding the file: it ran a script, printed a marker, and is
/// waiting for more input with its connection and any open transaction intact.
struct Holder {
    /// The shell.
    child: std::process::Child,
}

impl Holder {
    /// Starts `inillucent-shell` on a file and returns once it has run a
    /// script, which it proves by printing a marker placed after it.
    ///
    /// @param database - the file
    /// @param script - the statements, each on its own line
    fn after(database: &Path, script: &str) -> Holder {
        use std::io::{Read, Write};
        let mut child = std::process::Command::new(program("inillucent-shell"))
            .arg(database.to_string_lossy().replace('\\', "/"))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("the holder did not start: {error}"));
        if let Some(pipe) = child.stdin.as_mut() {
            let _ = pipe.write_all(format!(".bail on\n{script}\nSELECT 'holding';\n").as_bytes());
            let _ = pipe.flush();
        }
        let mut said = Vec::new();
        if let Some(out) = child.stdout.as_mut() {
            let mut byte = [0u8; 1];
            while !String::from_utf8_lossy(&said).contains("holding") {
                match out.read(&mut byte) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => said.push(byte[0]),
                }
            }
        }
        if !String::from_utf8_lossy(&said).contains("holding") {
            let _ = child.kill();
            let output = child.wait_with_output();
            panic!(
                "the holder stopped before its marker: {}{:?}",
                String::from_utf8_lossy(&said),
                output.map(|output| String::from_utf8_lossy(&output.stderr).to_string())
            );
        }
        Holder { child }
    }

    /// Ends the shell the way the operating system ends a process that dies:
    /// no destructor runs and nothing is folded.
    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Sends the last statements and waits for the shell to exit.
    ///
    /// @param script - what to run before the shell's input closes
    fn finish(mut self, script: &str) {
        use std::io::Write;
        if let Some(mut pipe) = self.child.stdin.take() {
            let _ = pipe.write_all(format!("{script}\n").as_bytes());
        }
        let output = self
            .child
            .wait_with_output()
            .unwrap_or_else(|error| panic!("the holder did not finish: {error}"));
        assert!(
            output.status.success(),
            "the holder failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// Opens the file in this process with a small pool, the way a reader that
/// keeps its memory small does.
///
/// @param database - the file
/// @param frames - how many pages the pool holds
fn small_pool(database: &Path, frames: usize) -> inillucent_driver::Result<Database> {
    Database::open_with(
        database,
        OpenOptions {
            cache_frames: frames,
            busy_timeout: Some(std::time::Duration::from_secs(3)),
            ..OpenOptions::default()
        },
    )
}

/// The script a holder runs: commit `rows` values of 24 KB, then open a second
/// transaction and leave it open, so the file is held RESERVED and the log
/// holds the committed rows unfolded.
///
/// @param rows - how many 24 KB values to commit first
fn commit_then_hold(rows: usize) -> String {
    format!(
        "CREATE TABLE big (id INTEGER PRIMARY KEY, b BLOB NOT NULL);
WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < {rows}) INSERT INTO big (id, b) SELECT i, randomblob(24000) FROM k;
BEGIN;
INSERT INTO big (id, b) VALUES (1000000, x'00');"
    )
}

/// Returns `count(*)` of `big` through a connection, or the error it gave.
///
/// @param database - the open database
fn count_big(database: &Database) -> Result<i64, String> {
    let rows = database
        .session()
        .query("SELECT count(*) FROM big", &[], 1)
        .map_err(|error| format!("{error:?}"))?;
    match rows.rows.first().and_then(|row| row.first()) {
        Some(Value::Integer(count)) => Ok(*count),
        other => Err(format!("the count was {other:?}")),
    }
}

/// A reader whose pool is smaller than what the log holds opens and reads
/// beside a writer that holds the file for writing.
///
/// The holder commits 60 values of 24 KB, which stay in the log, and holds
/// RESERVED. This process opens the file with a 64 page pool. Replaying 60
/// extent pages and the leaves above them needs more than 64 frames, so the
/// replay has to put pages somewhere. The design says such a replay spills to
/// the connection's own temporary file under SHARED. Before the fix the open
/// wrote them into the database file instead, which needs EXCLUSIVE, and every
/// statement the reader sent was refused with `busy` for as long as the writer
/// held the file. With 120 values the open itself failed with `every frame in
/// the buffer pool is pinned`.
#[test]
fn a_reader_with_a_small_pool_reads_beside_a_writer_holding_the_file() {
    for rows in [60usize, 120] {
        let directory = area(&format!("small-pool-beside-a-holder-{rows}"));
        let database = directory.join("shared.rdb");
        let holder = Holder::after(&database, &commit_then_hold(rows));
        let read = small_pool(&database, 64)
            .map_err(|error| format!("the open failed: {error:?}"))
            .and_then(|reader| count_big(&reader));
        holder.finish("COMMIT;");
        assert_eq!(
            read,
            Ok(rows as i64),
            "a 64 page reader beside a writer holding {rows} committed values in the log"
        );
        let (sound, said) = integrity(&database);
        assert!(sound, "the file failed its integrity check:\n{said}");
    }
}

/// A read transaction in one process reads beside a write transaction another
/// process holds open, in every form an application writes one.
///
/// **Every `BEGIN` took the write lock**, so `BEGIN; SELECT ...; COMMIT` in a
/// reader failed with `busy` for as long as any other process had a
/// transaction open, although the same `SELECT` on its own read fine. SQLite's
/// `BEGIN` is deferred: it takes no lock until a statement inside it needs one.
#[test]
fn a_read_transaction_reads_beside_a_writer_holding_one_open() {
    let directory = area("read-transaction-beside-a-writer");
    let database = directory.join("shared.rdb");
    let holder = Holder::after(
        &database,
        "CREATE TABLE t (x INTEGER);
INSERT INTO t VALUES (1);
BEGIN;
INSERT INTO t VALUES (2);",
    );
    let reader = Database::open_with(
        &database,
        OpenOptions {
            busy_timeout: Some(std::time::Duration::from_millis(1500)),
            ..OpenOptions::default()
        },
    )
    .unwrap_or_else(|error| panic!("the reader did not open: {error:?}"));
    let session = reader.session();
    let mut answers = Vec::new();
    for (open, close) in [
        ("BEGIN", "COMMIT"),
        ("BEGIN DEFERRED", "ROLLBACK"),
        ("SAVEPOINT look", "RELEASE look"),
    ] {
        let read = session
            .execute_batch(open)
            .and_then(|()| session.query("SELECT count(*) FROM t", &[], 1))
            .and_then(|rows| session.execute_batch(close).map(|()| rows.rows));
        answers.push((open, read.map_err(|error| format!("{error:?}"))));
    }
    holder.finish("COMMIT;");
    for (open, answer) in answers {
        assert_eq!(
            answer,
            Ok(vec![vec![Value::Integer(1)]]),
            "`{open}; SELECT; ...` beside a writer's open transaction"
        );
    }
}

/// A transaction never commits under the number of another process's
/// unfinished transaction, so that unfinished transaction stays unfinished.
///
/// This process and a shell open the file at the same moment, so their
/// transaction counters start at the same number. The shell begins a
/// transaction, writes enough for its records to reach the log, and is
/// killed. This process then runs `BEGIN; INSERT; COMMIT`. A deferred `BEGIN`
/// numbers the transaction before any lock is taken, so before the fix this
/// process committed under the shell's number, and recovery, which decides by
/// number what to replay, replayed the shell's rows as committed. The storm
/// saw readers find 31 and then 95 rows of a killed transaction this way.
#[test]
fn a_transaction_never_reuses_the_number_of_a_killed_processes_unfinished_one() {
    let directory = area("killed-transaction-number");
    let database = directory.join("shared.rdb");
    other_process_exec_create(&database);
    let ours = this_process(&database);
    let session = ours.session();
    session
        .query("SELECT count(*) FROM t", &[], 1)
        .unwrap_or_else(|error| panic!("{error:?}"));
    let killed = Holder::after(
        &database,
        "BEGIN;
WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < 400) INSERT INTO t (who, pad) SELECT 'unfinished', randomblob(3000) FROM k;",
    );
    killed.kill();
    session
        .execute_batch("BEGIN; INSERT INTO t (who, pad) VALUES ('committed', x'00'); COMMIT;")
        .unwrap_or_else(|error| panic!("this process's transaction failed: {error:?}"));
    let expected = vec![
        vec!["committed".to_string(), "1".to_string()],
        vec!["first".to_string(), "1".to_string()],
    ];
    // **Read by another process while this one still has the commit in its
    // log and not yet folded**, which is when a reader replays the log and
    // decides by number what committed. Once this process folds, the recovery
    // point moves past the number and nothing replays it again.
    assert_eq!(
        other_process_rows(
            &database,
            "SELECT who, count(*) FROM t GROUP BY who ORDER BY who"
        ),
        expected,
        "another process replaying the log saw rows of the killed process's unfinished transaction"
    );
    drop(ours);
    assert_eq!(
        other_process_rows(
            &database,
            "SELECT who, count(*) FROM t GROUP BY who ORDER BY who"
        ),
        expected,
        "rows of the killed process's unfinished transaction are in the file"
    );
    let (sound, said) = integrity(&database);
    assert!(sound, "the file failed its integrity check:\n{said}");
}

/// Creates the file the number case uses, in another process, with one row.
///
/// @param database - the file
fn other_process_exec_create(database: &Path) {
    let ran = run(
        &program("inillucent"),
        &["create", &database.to_string_lossy()],
    );
    assert_eq!(ran.code, 0, "{}", ran.said());
    other_process_exec(
        database,
        "CREATE TABLE t (who TEXT NOT NULL, pad BLOB NOT NULL)",
    );
    other_process_exec(database, "INSERT INTO t (who, pad) VALUES ('first', x'00')");
}

/// `inillucent batch` refused because another process holds the file reports
/// `busy`, so a script can tell it to retry.
///
/// Its `BEGIN` and `COMMIT` went through a path that kept only the error's
/// text, and every failure there was labelled `syntax` with exit code 1.
#[test]
fn a_batch_refused_by_a_held_file_reports_busy() {
    let directory = area("batch-refused");
    let database = directory.join("shared.rdb");
    let holder = Holder::after(
        &database,
        "CREATE TABLE t (x INTEGER);
BEGIN IMMEDIATE;
INSERT INTO t VALUES (1);",
    );
    let ran = Command::new(program("inillucent"))
        .args([
            "--db",
            &database.to_string_lossy(),
            "batch",
            "INSERT INTO t VALUES (2); INSERT INTO t VALUES (3)",
            "--output",
            "json",
        ])
        .env("INILLUCENT_BUSY_TIMEOUT", "300")
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|error| panic!("{error}"));
    holder.finish("COMMIT;");
    let said = String::from_utf8_lossy(&ran.stdout).to_string();
    assert_eq!(ran.status.code(), Some(1), "{said}");
    assert!(
        said.contains("\"status\": \"busy\"") || said.contains("\"status\":\"busy\""),
        "the refusal did not report busy:\n{said}"
    );
}

/// The same sequence through `Database::backup_to`, which checkpoints before it
/// copies: the copy and the original both hold the other process's commit.
#[test]
fn a_backup_called_between_statements_copies_another_processes_commit() {
    let directory = area("backup-after-another-commit");
    let database = directory.join("shared.rdb");
    let copy = directory.join("copy.rdb");
    let ours = this_process(&database);
    let session = ours.session();
    session
        .execute_batch(
            "CREATE TABLE note (n INTEGER NOT NULL UNIQUE);
             INSERT INTO note (n) VALUES (1), (2), (3);",
        )
        .unwrap_or_else(|error| panic!("the table was not created: {error:?}"));

    other_process_exec(&database, "UPDATE note SET n = n + 10");

    ours.backup_to(&copy)
        .unwrap_or_else(|error| panic!("the backup failed: {error:?}"));
    drop(ours);

    for file in [&database, &copy] {
        assert_eq!(
            other_process_rows(file, "SELECT n FROM note ORDER BY n"),
            vec![
                vec!["11".to_string()],
                vec!["12".to_string()],
                vec!["13".to_string()]
            ],
            "{} lost the other process's update",
            file.display()
        );
        let (sound, said) = integrity(file);
        assert!(
            sound,
            "{} failed its integrity check:\n{said}",
            file.display()
        );
    }
}

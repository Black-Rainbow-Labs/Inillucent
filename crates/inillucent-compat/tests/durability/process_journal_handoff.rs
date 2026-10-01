//! A rollback journal never outlives the lock of the connection that wrote it,
//! and no connection writes the database file under a shared lock.
//!
//! Invariant: **every update a process was told had committed is in the file
//! at the end, with three processes sharing it, and the file passes
//! `integrity-check`.** The writer adds one to `v` on a known set of rows in
//! every statement, so `sum(v)` at the end is a count of acknowledged updates,
//! and a smaller number is updates that were lost.
//!
//! ## What this caught
//!
//! A statement whose dirty pages outgrow the buffer pool writes some of them to
//! the file early, and saves each page's old image to `<database>-journal`
//! first. Until this suite existed the journal stayed on the disk after the
//! statement committed and the lock was released, and it was only removed by
//! the next fold. A second process that opened the file in that window found a
//! journal beside an unlocked file, took it for one a crashed process had left,
//! and wrote the old images back over pages the writer had committed. The
//! writer was never told: its next statement compared the meta record and the
//! log's tail, neither had moved, and it carried on from its own cache on top
//! of a file that was older than the cache said.
//!
//! What that leaves depends on the page that was put back:
//!
//! - a leaf: committed changes to its rows are gone, and `integrity-check`
//!   still says `ok`. The first case below measured 505 of 4,800 acknowledged
//!   updates missing on the build before the fix.
//! - an interior page: it no longer points at the leaves a split made, and the
//!   leaf chain reaches more leaves than the levels above it do. That is
//!   `the sibling chain visits 29361 leaves and the interior levels reach
//!   29358`, the report this suite was written for.
//! - a page that was a hole when it was first written early: its old image is
//!   zeros, so the page becomes zeros. That is the other report, `page 114714
//!   checksum 00000000`, four zero pages near the end of a 3.5 GB file.
//!
//! The second case is the reading side. A reader that finds the log has moved
//! replays it into its own pool. A replay larger than the pool has to write
//! pages to the file, and on the build before the fix it could not: the pool
//! still held the watermark of the reader's previous log, so the first page it
//! had to evict was refused as `writing it would put the data file ahead of the
//! log`, and every statement the reader sent failed that way until the writer
//! folded. Had the write been allowed, it would have gone to the file and to a
//! journal under the shared lock every reader holds, and the journal would have
//! been left behind for the next process to replay.
//!
//! The third case is a writer killed in the middle of a statement that had
//! already written pages early. An open puts the dead writer's journal back
//! before it reads anything; a reader that was already open replayed the log
//! over the file as it stood and left the journal for the next process to find.
//!
//! The fourth case is a read only reader, which may not write the file at all,
//! replaying more than its pool holds. It failed the same way as the second.
//!
//! Every case fails on the build before the fix and passes after it. On that
//! build the first case also left a file a new process could not open:
//! `database disk image is malformed: replaying the log: replaying InsertRow at
//! 283702864 found no room in leaf 99`.
//!
//! ## Why the shell and three processes
//!
//! The window exists only between two statements of a connection that stays
//! open, so a process per statement cannot reach it. A process that opens the
//! file and then closes folds what it replayed on the way out, which moves the
//! meta record and makes the writer reload, so the second process has to stay
//! open too.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use inillucent_compat::cliproc::{program, rows, run};
use inillucent_compat::workspace_root;

/// How many rows the table holds.
///
/// Twenty thousand rows of about three kilobytes are about two thousand leaves,
/// thirty times the smallest pool a connection can ask for.
///
/// **Measured, not chosen.** With eight thousand rows the first case passed on
/// the build it was written to catch: no journal was on the disk at any of the
/// moments a second process looked. With twenty thousand, the same build lost
/// 505 of the 4,800 updates on every run.
const ROWS: usize = 20_000;

/// How many rows one update statement changes.
///
/// The real writer this reproduces changed thirty two rows a statement. The
/// number only has to be small enough that a statement writes well under the
/// four mebibytes of log that start a fold, and large enough to touch more
/// leaves than the pool holds after a few statements.
const ROWS_A_STATEMENT: usize = 24;

/// How many update statements the writer sends.
const STATEMENTS: usize = 200;

/// The smallest pool a connection can have, in frames.
///
/// `PRAGMA cache_size` below sixty four is raised to it, so this is the
/// smallest number of dirty pages that makes a connection evict.
const SMALL_POOL: usize = 64;

/// How many processes open the file beside the writer in the first case.
///
/// Each open is one chance to find a journal and replay it. The first version
/// of this case opened one process at a fixed statement and passed on the
/// build it was written to catch, because no journal happened to be on the
/// disk at that statement.
const OPENERS: usize = 6;

/// A shell process kept open across statements.
struct Session {
    /// The process.
    child: Child,
    /// Where statements are written.
    input: ChildStdin,
    /// What the shell prints, read a line at a time.
    output: BufReader<ChildStdout>,
    /// The standard error, collected on a thread so a full pipe never stalls
    /// the shell.
    errors: std::thread::JoinHandle<String>,
    /// How many statements have been sent, which names each one's marker.
    sent: usize,
}

impl Session {
    /// Opens a shell on the database.
    ///
    /// @param database - the file to open
    /// @param pool - the pool size to ask for, in frames, or none for the default
    fn open(database: &Path, pool: Option<usize>) -> Session {
        Session::open_with(database, pool, &[])
    }

    /// Opens a shell on the database with extra options before the file name.
    ///
    /// @param database - the file to open
    /// @param pool - the pool size to ask for, in frames, or none for the default
    /// @param options - the shell's own options, such as `-readonly`
    fn open_with(database: &Path, pool: Option<usize>, options: &[&str]) -> Session {
        let mut child = Command::new(program("inillucent-shell"))
            .args(options)
            .arg(database.to_string_lossy().replace('\\', "/"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|error| panic!("a shell did not start: {error}"));
        let (Some(input), Some(output), Some(mut error)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            panic!("a shell has no pipes");
        };
        let errors = std::thread::spawn(move || {
            let mut said = String::new();
            let _ = error.read_to_string(&mut said);
            said
        });
        let mut session = Session {
            child,
            input,
            output: BufReader::new(output),
            errors,
            sent: 0,
        };
        // **`.bail on`, so a statement that fails ends the shell.** Without it
        // the shell reports the error and carries on, and the marker that
        // follows can fail the same way - which is how the first version of
        // the second case waited for ever on a reader whose every statement was
        // refused. A shell that ends is a panic that carries what it said.
        session.ask(".bail on");
        session.ask("PRAGMA busy_timeout = 60000;");
        if let Some(frames) = pool {
            session.ask(&format!("PRAGMA cache_size = {frames};"));
        }
        session
    }

    /// Sends statements and waits until the shell has run them.
    ///
    /// Returns what the shell printed for them. A `SELECT` of a marker follows
    /// the statements, and the shell runs its input in order, so the marker
    /// coming back means every statement before it has finished and released
    /// whatever lock it took.
    ///
    /// @param sql - one or more statements, each ending in a semicolon
    fn ask(&mut self, sql: &str) -> String {
        self.sent += 1;
        let marker = format!("statement-{}-done", self.sent);
        self.input
            .write_all(format!("{sql}\nSELECT '{marker}';\n").as_bytes())
            .and_then(|()| self.input.flush())
            .unwrap_or_else(|error| panic!("the shell stopped taking input: {error}"));
        let mut said = String::new();
        loop {
            let mut line = String::new();
            let read = self.output.read_line(&mut line).unwrap_or(0);
            if read == 0 {
                let _ = self.child.wait();
                let errors = std::mem::replace(&mut self.errors, std::thread::spawn(String::new));
                panic!(
                    "the shell ended before it ran `{sql}`; it had printed:\n{said}\nand on \
                     standard error:\n{}",
                    errors.join().unwrap_or_default()
                );
            }
            if line.contains(&marker) {
                return said;
            }
            said.push_str(&line);
        }
    }

    /// Sends statements without waiting for them to finish.
    ///
    /// For a statement that is still running when the caller acts, which is
    /// how a process gets killed in the middle of one.
    ///
    /// @param sql - one or more statements, each ending in a semicolon
    fn send(&mut self, sql: &str) {
        self.input
            .write_all(format!("{sql}\n").as_bytes())
            .and_then(|()| self.input.flush())
            .unwrap_or_else(|error| panic!("the shell stopped taking input: {error}"));
    }

    /// Ends the shell the way the operating system ends a process that is
    /// killed: no statement finishes, no destructor runs, and the locks go
    /// because the process does.
    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Closes the shell and returns what it wrote to standard error.
    fn close(mut self) -> String {
        let _ = self.input.write_all(b".quit\n");
        drop(self.input);
        let _ = self.child.wait();
        self.errors.join().unwrap_or_default()
    }
}

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/process-journal-handoff")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Builds the table, each row with a blob of about three kilobytes and `v`
/// starting at the row's own id.
///
/// @param directory - where to put the file
fn prepared(directory: &Path) -> PathBuf {
    let database = directory.join("shared.rdb");
    let path = database.to_string_lossy().to_string();
    let fill = format!(
        "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < {ROWS}) \
         INSERT INTO t (id, pad, v) SELECT i, randomblob(3072), i FROM c"
    );
    for arguments in [
        vec!["create", path.as_str()],
        vec![
            "--db",
            path.as_str(),
            "exec",
            "CREATE TABLE t (id INTEGER PRIMARY KEY, pad BLOB, v INTEGER NOT NULL)",
        ],
        vec!["--db", path.as_str(), "exec", fill.as_str()],
    ] {
        let ran = run(&program("inillucent"), &arguments);
        assert_eq!(
            ran.code,
            0,
            "preparing the database failed at {arguments:?}:\n{}",
            ran.said()
        );
    }
    database
}

/// Returns the update statement number `n` sends.
///
/// The rows are spread across the whole table, so successive statements touch
/// different leaves and the dirty pages pile up past the pool. The ids are
/// distinct within a statement because `811 * 23` is below [`ROWS`], so the
/// statement changes exactly [`ROWS_A_STATEMENT`] rows.
///
/// @param n - which statement, from one
fn update(n: usize) -> String {
    let ids: Vec<String> = (0..ROWS_A_STATEMENT)
        .map(|k| (1 + (n * 37 + k * 811) % ROWS).to_string())
        .collect();
    format!(
        "UPDATE t SET pad = randomblob(3072), v = v + 1 WHERE id IN ({});",
        ids.join(",")
    )
}

/// Returns `sum(v)` and the integrity check, read by a fresh process once
/// every other process has closed.
///
/// @param database - the file
fn settled(database: &Path) -> (i64, String) {
    let path = database.to_string_lossy().to_string();
    let summed = run(
        &program("inillucent"),
        &[
            "--db",
            &path,
            "query",
            "SELECT sum(v) FROM t",
            "--output",
            "json",
        ],
    );
    assert_eq!(summed.code, 0, "the sum did not run:\n{}", summed.said());
    let sum = rows(&summed.stdout)
        .first()
        .and_then(|row| row.first())
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or_else(|| panic!("the sum is not a number:\n{}", summed.said()));
    let checked = run(&program("inillucent"), &["--db", &path, "integrity-check"]);
    (sum, checked.said())
}

/// The value `sum(v)` starts at: each row's `v` is its own id.
fn starting_sum() -> i64 {
    (ROWS as i64) * (ROWS as i64 + 1) / 2
}

/// A writer with a small pool, and a second process that opens the file while
/// the writer is between statements and stays open.
///
/// Before the fix, the writer's early writes left a journal on the disk
/// between statements, the second process's open put the old images back,
/// and the writer's later statements were built on top of those old pages.
#[test]
fn a_second_process_opening_the_file_does_not_undo_a_live_writer() {
    let directory = area("open-between-statements");
    let database = prepared(&directory);
    let journal = PathBuf::from(format!("{}-journal", database.to_string_lossy()));
    let mut writer = Session::open(&database, Some(SMALL_POOL));
    let mut others: Vec<Session> = Vec::new();
    let mut journals_left = 0usize;
    for n in 1..=STATEMENTS {
        writer.ask(&update(n));
        // **Between two statements nothing may be beside the file that another
        // process would replay.** The writer holds no lock here, so a journal
        // on the disk now is one any process opening the file will treat as a
        // crashed writer's.
        let left = journal.exists();
        if left {
            journals_left += 1;
        }
        // A new process opens whenever there is a journal to find, which is
        // the window, up to [`OPENERS`] of them, and every twenty five
        // statements when there is none. Each one stays open: a process that
        // closed would fold what it replayed, and the fold moves the meta
        // record, which makes the writer reload and hides the loss.
        if (left || n % 25 == 0) && others.len() < OPENERS {
            let mut opened = Session::open(&database, None);
            opened.ask("SELECT count(*) FROM t;");
            others.push(opened);
        } else if let Some(open) = others.last_mut() {
            if n % 25 == 0 {
                open.ask("SELECT count(*), sum(v) FROM t;");
            }
        }
    }
    let writer_errors = writer.close();
    let other_errors: String = others.into_iter().map(Session::close).collect();
    let (sum, checked) = settled(&database);
    let expected = starting_sum() + (STATEMENTS * ROWS_A_STATEMENT) as i64;
    assert_eq!(
        sum,
        expected,
        "{} of {} acknowledged updates are missing. A journal was beside the file after \
         {journals_left} of {STATEMENTS} statements had released the lock.\nwriter's errors: \
         {writer_errors}\nsecond process's errors: {other_errors}\nintegrity check: {checked}",
        expected - sum,
        STATEMENTS * ROWS_A_STATEMENT
    );
    assert!(
        checked.contains("ok") && !checked.contains("corrupt"),
        "the file is damaged: {checked}"
    );
    assert_eq!(
        journals_left, 0,
        "a journal was left beside the file after {journals_left} of {STATEMENTS} statements \
         had released the lock"
    );
}

/// A writer with the default pool, a reader whose pool is smaller than what it
/// has to replay, and a third process that opens the file while the reader is
/// between statements. The reader then writes.
///
/// Before the fix every statement the reader sent between two of the writer's
/// folds failed with `writing it would put the data file ahead of the log`,
/// because its replay could not evict a page. The shells run with `.bail on`,
/// so a failed statement ends the reader and fails this case with what it said.
/// After the fix the replay raises the reader to EXCLUSIVE to write the pages
/// and folds before it lets go, so the third process finds no journal.
#[test]
fn a_reader_that_replays_more_than_its_pool_leaves_nothing_to_undo() {
    let directory = area("reader-replays-past-its-pool");
    let database = prepared(&directory);
    let mut writer = Session::open(&database, None);
    let mut reader = Session::open(&database, Some(SMALL_POOL));
    let mut third: Option<Session> = None;
    let mut reader_updates = 0i64;
    for n in 1..=STATEMENTS {
        writer.ask(&update(n));
        if n % 10 != 0 {
            continue;
        }
        // The reader catches up on everything the writer has logged since the
        // last fold, which is far more than sixty four pages.
        reader.ask("SELECT count(*), sum(length(pad)) FROM t;");
        match third.as_mut() {
            None => {
                let mut opened = Session::open(&database, None);
                opened.ask("SELECT count(*) FROM t;");
                third = Some(opened);
            }
            Some(open) => {
                open.ask("SELECT count(*) FROM t;");
            }
        }
        // And then writes, from whatever its pool and the file now hold.
        let id = 1 + (n * 53) % ROWS;
        reader.ask(&format!("UPDATE t SET v = v + 1 WHERE id = {id};"));
        reader_updates += 1;
    }
    let writer_errors = writer.close();
    let reader_errors = reader.close();
    let third_errors = third.map(Session::close).unwrap_or_default();
    let (sum, checked) = settled(&database);
    let expected = starting_sum() + (STATEMENTS * ROWS_A_STATEMENT) as i64 + reader_updates;
    assert_eq!(
        sum,
        expected,
        "{} acknowledged updates are missing.\nwriter's errors: {writer_errors}\nreader's \
         errors: {reader_errors}\nthird process's errors: {third_errors}\nintegrity check: \
         {checked}",
        expected - sum
    );
    assert!(
        checked.contains("ok") && !checked.contains("corrupt"),
        "the file is damaged: {checked}"
    );
}

/// A writer killed in the middle of a statement that had already written pages
/// early, and a reader that had the file open the whole time.
///
/// Two small statements leave pages dirty in the writer's pool without filling
/// it, so nothing is written early and no fold runs. A third statement touches
/// every row: it fills the pool, writes the dirty pages of the first two to the
/// file early and saves their old images to the journal. The writer is killed
/// the moment the journal appears.
///
/// A process that opens the file afterwards puts those images back before it
/// reads anything, which is what `open` has always done. A reader that was
/// already open did not: it saw the log had moved, replayed it over the file as
/// it stood and carried on, and the dead writer's journal stayed beside the
/// file for the next process to open to replay - behind the reader's back, the
/// same way as in the first case.
#[test]
fn a_reader_that_was_open_settles_what_a_killed_writer_left() {
    let directory = area("writer-killed-mid-statement");
    let database = prepared(&directory);
    let journal = PathBuf::from(format!("{}-journal", database.to_string_lossy()));
    let mut reader = Session::open(&database, None);
    reader.ask("SELECT count(*) FROM t;");
    let mut writer = Session::open(&database, Some(SMALL_POOL));
    writer.ask(&update(1));
    writer.ask(&update(2));
    let committed = starting_sum() + (2 * ROWS_A_STATEMENT) as i64;
    assert!(
        !journal.exists(),
        "two small statements already left a journal; the case needs dirty pages that have \
         not been written yet"
    );
    writer.send("UPDATE t SET pad = randomblob(3072), v = v + 1000;");
    let started = std::time::Instant::now();
    while !journal.exists() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(120),
            "the long statement never wrote a page early, so there is nothing for this case \
             to settle"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    writer.kill();
    assert!(
        journal.exists(),
        "the writer's journal is gone, so it finished its statement before it was killed"
    );
    // The reader takes the lock for the first time since the writer died.
    let seen = reader.ask("SELECT sum(v) FROM t;");
    assert_eq!(
        seen.trim().lines().last().unwrap_or_default().trim(),
        committed.to_string(),
        "the reader saw something other than the two committed statements:\n{seen}"
    );
    assert!(
        !journal.exists(),
        "the dead writer's journal is still beside the file after the reader's statement, so \
         the next process to open the file will put its old images back behind the reader"
    );
    let mut third = Session::open(&database, None);
    third.ask("SELECT count(*) FROM t;");
    reader.ask("UPDATE t SET v = v + 1 WHERE id = 1;");
    let reader_errors = reader.close();
    let third_errors = third.close();
    let (sum, checked) = settled(&database);
    assert_eq!(
        sum,
        committed + 1,
        "the file does not hold exactly the committed updates.\nreader's errors: \
         {reader_errors}\nthird process's errors: {third_errors}\nintegrity check: {checked}"
    );
    assert!(
        checked.contains("ok") && !checked.contains("corrupt"),
        "the file is damaged: {checked}"
    );
}

/// A read only reader whose pool is smaller than what it has to replay.
///
/// A read only connection replays into its own memory and may not write the
/// file at all, so a page its replay cannot keep has nowhere to go. Before the
/// fix it failed every statement with `writing it would put the data file ahead
/// of the log`, like the second case. Now its pool keeps replayed pages in the
/// frames it owns past `cache_size`, and it answers.
#[test]
fn a_read_only_reader_replays_more_than_its_pool() {
    let directory = area("read-only-reader-past-its-pool");
    let database = prepared(&directory);
    let mut writer = Session::open(&database, None);
    let mut reader = Session::open_with(&database, Some(SMALL_POOL), &["-readonly"]);
    for n in 1..=40 {
        writer.ask(&update(n));
        if n % 10 != 0 {
            continue;
        }
        let seen = reader.ask("SELECT sum(v) FROM t;");
        let expected = starting_sum() + (n * ROWS_A_STATEMENT) as i64;
        assert_eq!(
            seen.trim().lines().last().unwrap_or_default().trim(),
            expected.to_string(),
            "the read only reader answered something other than the {n} committed statements"
        );
    }
    let writer_errors = writer.close();
    let reader_errors = reader.close();
    let (sum, checked) = settled(&database);
    assert_eq!(
        sum,
        starting_sum() + (40 * ROWS_A_STATEMENT) as i64,
        "writer's errors: {writer_errors}\nreader's errors: {reader_errors}"
    );
    assert!(
        checked.contains("ok") && !checked.contains("corrupt"),
        "the file is damaged: {checked}"
    );
}

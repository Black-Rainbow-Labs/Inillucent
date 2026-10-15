//! Recovery that throws away an uncommitted tail happens at one open, not at every open after it.
//!
//! Invariant: **a session that opens a database whose log ends in a transaction that never
//! committed, and then only reads, leaves the file so that the next open recovers nothing.**
//!
//! ## What happened
//!
//! A writer killed in the middle of a large transaction leaves records in the log with no commit
//! record. The next open discards them and prints `recovered the log: ... 1 discarded.` Recovery
//! wrote nothing about having done so, and a session that only reads holds no dirty page and
//! appends nothing, so its close did not fold. The discarded records stayed below the recovery
//! point and every later open scanned them again and printed the same line, until some session
//! wrote and folded. A documentation session found a database in this state after running
//! statements that were refused inside a transaction.
//!
//! The writer here is a real process ended by the operating system, the same way
//! `process_crash.rs` does it, because a transaction bigger than the log's spill size is the one
//! case where uncommitted records reach the log file before the commit.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc::{program, rows, run};
use inillucent_compat::workspace_root;

/// How many rows of a kilobyte the transaction writes: enough to pass the log's spill size of
/// 512 KiB, so its records are in the log file when the writer is killed.
const ROWS: usize = 900;

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/recovery-discard-runs-once")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Builds a database with one committed row, then starts a writer that opens a transaction, writes
/// more than the spill size and is killed before it commits.
///
/// @param directory - where the database goes
fn database_with_an_uncommitted_tail(directory: &Path) -> PathBuf {
    let database = directory.join("tail.rdb");
    let text = database.to_string_lossy().to_string();
    for arguments in [
        vec!["create", text.as_str()],
        vec![
            "--db",
            text.as_str(),
            "exec",
            "CREATE TABLE note (body TEXT)",
        ],
        vec![
            "--db",
            text.as_str(),
            "exec",
            "INSERT INTO note VALUES ('committed')",
        ],
    ] {
        let ran = run(&program("inillucent"), &arguments);
        assert_eq!(ran.code, 0, "{arguments:?}:\n{}", ran.said());
    }
    let mut child = Command::new(program("inillucent-shell"))
        .arg(text.replace('\\', "/"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("the writer did not start: {error}"));
    let mut feed = String::from("BEGIN;\n");
    for _ in 0..ROWS {
        feed.push_str("INSERT INTO note VALUES (hex(randomblob(512)));\n");
    }
    feed.push_str("SELECT 'written';\n");
    let mut input = child.stdin.take().expect("the writer has an input");
    input
        .write_all(feed.as_bytes())
        .and_then(|()| input.flush())
        .unwrap_or_else(|error| panic!("feeding the writer: {error}"));
    let mut output = BufReader::new(child.stdout.take().expect("the writer has an output"));
    loop {
        let mut line = String::new();
        let got = output
            .read_line(&mut line)
            .unwrap_or_else(|error| panic!("reading the writer: {error}"));
        assert!(got > 0, "the writer ended before it wrote the transaction");
        if line.trim() == "written" {
            break;
        }
    }
    // Ended by the operating system with the transaction open, and the input still open, so the
    // shell cannot close tidily first.
    let _ = child.kill();
    let _ = child.wait();
    drop(input);
    database
}

/// Runs a read through the command line and returns what it printed.
///
/// @param database - the file
fn read_count(database: &Path) -> (i32, String) {
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
    (ran.code, ran.stdout)
}

/// The first open after the kill discards the tail and says so, and the second open recovers
/// nothing.
#[test]
fn a_session_that_only_reads_does_not_leave_the_discarded_tail_to_be_scanned_again() {
    let directory = area("reader");
    let database = database_with_an_uncommitted_tail(&directory);

    let (code, first) = read_count(&database);
    assert_eq!(code, 0, "{first}");
    assert!(
        first.contains("transactions_discarded") && first.contains("recovered"),
        "the first open after the kill did not report discarding the transaction, so this case \
         did not leave an uncommitted tail:\n{first}"
    );
    assert_eq!(
        rows(&first),
        vec![vec!["1".to_string()]],
        "the committed row survives and the uncommitted ones do not"
    );

    for round in 2..=3 {
        let (code, later) = read_count(&database);
        assert_eq!(code, 0, "{later}");
        assert!(
            !later.contains("recovered") && !later.contains("transactions_discarded"),
            "open {round} recovered the log again:\n{later}"
        );
    }
    let check = run(
        &program("inillucent"),
        &["--db", &database.to_string_lossy(), "integrity-check"],
    );
    assert_eq!(check.code, 0, "{}", check.said());
}

/// A transaction that holds statements refused inside it commits, and no later open recovers or
/// replays the log.
///
/// **The sequence a documentation session reported.** `DETACH` and `VACUUM` are refused inside a
/// transaction and `COMMIT` succeeds. Each later open was reported to replay the log, so the
/// refused statements were suspected of leaving the log in a state the close did not fold.
#[test]
fn refused_statements_inside_a_transaction_leave_no_recovery_for_later_opens() {
    let directory = area("refused");
    let database = directory.join("refused.rdb");
    let text = database.to_string_lossy().to_string();
    let created = run(&program("inillucent"), &["create", text.as_str()]);
    assert_eq!(created.code, 0, "{}", created.said());
    inillucent_compat::cliproc::run_with_input(
        &program("inillucent-shell"),
        &[text.as_str()],
        "CREATE TABLE t(a);\nBEGIN;\nINSERT INTO t VALUES (1);\nDETACH o;\nVACUUM;\nCOMMIT;\n",
    );
    for round in 1..=2 {
        let ran = run(
            &program("inillucent"),
            &[
                "--db",
                text.as_str(),
                "query",
                "SELECT count(*) FROM t",
                "--output",
                "json",
            ],
        );
        assert_eq!(ran.code, 0, "{}", ran.said());
        assert_eq!(rows(&ran.stdout), vec![vec!["1".to_string()]]);
        assert!(
            !ran.stdout.contains("recovered") && !ran.stdout.contains("replayed"),
            "open {round} replayed the log:\n{}",
            ran.stdout
        );
    }
}

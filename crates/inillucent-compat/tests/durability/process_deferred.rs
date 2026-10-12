//! Processes that each run a statement and then open a deferred transaction
//! take turns instead of waiting each other out.
//!
//! Invariant: **a deferred `BEGIN` holds no lock, even straight after a
//! statement that read.**
//!
//! ## Why this exists
//!
//! A statement that only reads keeps the shared lock for a moment after it
//! ends, so the next statement does not take it again. A deferred `BEGIN` run
//! inside that moment left the lock held, and the transaction then looked like
//! one that had read before its first write. Its first `INSERT` was refused
//! with "another process committed after this transaction began reading" when
//! another process had committed in between. Otherwise it waited for the write
//! lock while holding the shared one, beside a writer waiting for every shared
//! lock to go, until one of them ran out its busy timeout. Four workers, each
//! starting a new process per transaction that ran `PRAGMA busy_timeout` and
//! then `BEGIN; INSERT; COMMIT;`, finished 16 to 31 of 60 transactions, the
//! median one in 5.4 s. SQLite finished all 60, the median one in 30 ms. Found
//! by the bug hunt's two process hunter.
//!
//! The pattern is what a connection per request does: a setting or a check,
//! then a transaction to write.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use inillucent_compat::cliproc::{program, rows, run, run_with_input, Ran};
use inillucent_compat::workspace_root;

/// How many workers run at once.
const WORKERS: usize = 4;

/// How many transactions each worker runs, each in a process of its own.
const ROUNDS: usize = 12;

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/process-deferred")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Returns the script one process runs: a pragma, then a deferred transaction
/// that inserts one row.
///
/// @param worker - the worker's number, written into its row
/// @param round - the transaction's number, written into its row
fn script(worker: usize, round: usize) -> String {
    format!(
        ".bail on\nPRAGMA busy_timeout = 30000;\nBEGIN;\nINSERT INTO t(p, r) VALUES ({worker}, {round});\nCOMMIT;\n"
    )
}

/// Runs one worker's transactions, one process each, and returns how each
/// process ended.
///
/// @param path - the database file
/// @param worker - the worker's number
fn worker_runs(path: &str, worker: usize) -> Vec<Ran> {
    (0..ROUNDS)
        .map(|round| {
            run_with_input(
                &program("inillucent-shell"),
                &[path],
                &script(worker, round),
            )
        })
        .collect()
}

/// Four workers starting a process per deferred transaction commit every
/// transaction, in about the time SQLite takes.
#[test]
fn a_statement_then_a_deferred_transaction_takes_turns_with_other_processes() {
    let directory = area("statement-then-begin");
    let database = directory.join("app.rdb");
    let path = database.to_string_lossy().to_string();
    let made = run(
        &program("inillucent"),
        &[
            "--db",
            &path,
            "exec",
            "CREATE TABLE t(p INTEGER, r INTEGER, UNIQUE(p, r))",
        ],
    );
    assert_eq!(made.code, 0, "the table is made: {}", made.stderr);
    let started = Instant::now();
    let workers: Vec<_> = (0..WORKERS)
        .map(|worker| {
            let path = path.clone();
            std::thread::spawn(move || worker_runs(&path, worker))
        })
        .collect();
    let finished: Vec<Ran> = workers
        .into_iter()
        .flat_map(|worker| worker.join().expect("the worker finishes"))
        .collect();
    let took = started.elapsed();
    for ran in &finished {
        assert!(
            ran.stderr.trim().is_empty(),
            "no statement failed:\n{}",
            ran.stderr
        );
    }
    let counted = run(
        &program("inillucent"),
        &[
            "--db",
            &path,
            "query",
            "SELECT count(*) FROM t",
            "--output",
            "json",
        ],
    );
    assert_eq!(counted.code, 0, "the count runs: {}", counted.stderr);
    assert_eq!(
        rows(&counted.stdout),
        vec![vec![(WORKERS * ROUNDS).to_string()]],
        "every transaction committed"
    );
    // A transaction that waited out a busy timeout took five seconds alone.
    assert!(
        took < Duration::from_secs(20),
        "the workers took {took:?} for {} transactions",
        WORKERS * ROUNDS
    );
}

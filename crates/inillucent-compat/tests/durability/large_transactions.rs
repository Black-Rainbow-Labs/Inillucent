//! Transactions larger than the page cache, read beside and killed after.
//!
//! Invariant: **a reader in another process reads the last committed state
//! while a transaction larger than the writer's page cache is open, and a file
//! whose writer was killed after a rollback opens with every committed row.**
//!
//! ## The reader beside a large transaction
//!
//! A write holds RESERVED, which readers share. When a transaction's dirty pages
//! outgrew the writer's page cache, the writer evicted them into the database
//! file, which needs EXCLUSIVE, and kept EXCLUSIVE until it committed: every
//! reader in another process waited for the rest of the transaction and then
//! failed with busy. Measured on 2.0.4 with a 128 MiB cache: an `UPDATE` of
//! 20,000 rows of 3 KB values blocked a reader with a three second busy
//! timeout, every time. Such a page now goes to a spill file the writer owns,
//! so the database file is written only by a fold, under EXCLUSIVE. The test
//! sets a cache of 64 pages so that a few megabytes are enough.
//!
//! ## The rollback before a kill
//!
//! Recovery replays only transactions whose commit record it finds. A
//! `ROLLBACK` undoes rows through the tree, so the page splits the transaction
//! made stay in the tree, and the next transaction writes into those pages.
//! The undo was logged under the rolled back transaction with no commit record
//! after it, so the replay after a kill skipped both and did not know the pages
//! existed: on 2.0.4 the file did not open again, with `replaying InsertRow ...
//! found no room in leaf 7` or `read 0 of 32768 bytes at` the end of the file.
//! A finished rollback now ends with a commit record. `locking_mode =
//! exclusive` keeps the writer from folding, so the next open has to rebuild
//! everything from the log.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc::program;
use inillucent_compat::workspace_root;
use inillucent_engine::connect::Database;
use inillucent_tree::datum::OwnedDatum;

/// Returns an empty scratch directory for one test.
///
/// @param name - the test's name
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/large-transactions")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Creates a database holding one table of `rows` rows whose value is one
/// version byte and then zeros, with an index on the value.
///
/// @param path - the file
/// @param rows - how many rows
/// @param bytes - how long each value is
fn prepared(path: &Path, rows: usize, bytes: usize) {
    let database = Database::open(path).expect("a fresh database opens");
    let connection = database.session();
    connection
        .execute_batch(&format!(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v BLOB); \
             CREATE INDEX t_v ON t (v); \
             WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < {rows}) \
             INSERT INTO t SELECT i, x'01' || zeroblob({}) FROM n",
            bytes - 1
        ))
        .expect("the table loads");
}

/// Returns the rows of a query, run in this process on a fresh open of the
/// file, each row as text.
///
/// @param path - the file
/// @param sql - the query
fn rows_after_open(path: &Path, sql: &str) -> Result<Vec<String>, String> {
    let database = Database::open(path).map_err(|error| error.message().to_string())?;
    let connection = database.session();
    let rows = connection
        .query(sql)
        .map_err(|error| error.message().to_string())?;
    Ok(rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| match value {
                    OwnedDatum::Int(number) => number.to_string(),
                    OwnedDatum::Text(text) => String::from_utf8_lossy(text).into_owned(),
                    OwnedDatum::Blob(blob) => format!("x{}", blob.first().copied().unwrap_or(0)),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<String>>()
                .join("|")
        })
        .collect())
}

/// A shell with its input open, so it neither ends nor folds until told.
struct OpenShell {
    child: std::process::Child,
    input: std::process::ChildStdin,
    output: BufReader<std::process::ChildStdout>,
}

impl OpenShell {
    /// Starts a shell on a file.
    ///
    /// @param database - the file
    fn start(database: &Path) -> OpenShell {
        let mut child = Command::new(program("inillucent-shell"))
            .arg(database.to_string_lossy().replace('\\', "/"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|error| panic!("a shell did not start: {error}"));
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            panic!("a shell has no pipes");
        };
        OpenShell {
            child,
            input,
            output: BufReader::new(output),
        }
    }

    /// Sends statements and waits until the shell has printed `marker`.
    ///
    /// @param statements - what to run, ending with a `SELECT` of the marker
    /// @param marker - the text that says they have run
    fn run_until(&mut self, statements: &str, marker: &str) {
        self.input
            .write_all(statements.as_bytes())
            .expect("the shell takes its input");
        self.input.flush().expect("the shell takes its input");
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.output.read_line(&mut line).unwrap_or(0);
            assert!(read > 0, "the shell ended before it printed {marker}");
            if line.contains(marker) {
                return;
            }
        }
    }
}

/// Runs statements in a shell of their own and returns what it printed.
///
/// @param database - the file
/// @param statements - what to run
fn shell_output(database: &Path, statements: &str) -> String {
    let mut child = Command::new(program("inillucent-shell"))
        .arg(database.to_string_lossy().replace('\\', "/"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("a shell did not start: {error}"));
    if let Some(mut input) = child.stdin.take() {
        input
            .write_all(statements.as_bytes())
            .expect("the shell takes its input");
    }
    let output = child.wait_with_output().expect("the shell ends");
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// **A reader in another process reads through a transaction larger than the
/// writer's page cache, and sees the last committed state.**
#[test]
fn a_reader_reads_beside_a_transaction_larger_than_the_page_cache() {
    let directory = area("reader-beside");
    let database = directory.join("shared.rdb");
    prepared(&database, 3000, 3000);

    let mut writer = OpenShell::start(&database);
    writer.run_until(
        "PRAGMA cache_size = 64;\nBEGIN;\nUPDATE t SET v = x'02' || zeroblob(2999);\nSELECT 'updated';\n",
        "updated",
    );
    let during = shell_output(
        &database,
        "PRAGMA busy_timeout = 2000;\nSELECT count(*) FROM t WHERE v = x'01' || zeroblob(2999);\n",
    );
    assert!(
        during.lines().any(|line| line.trim() == "3000"),
        "the reader read the committed state while the transaction was open: {during}"
    );
    writer.run_until("COMMIT;\nSELECT 'committed';\n", "committed");
    drop(writer.input);
    let _ = writer.child.wait();

    let after = rows_after_open(
        &database,
        "SELECT hex(substr(v, 1, 1)), count(*) FROM t GROUP BY 1",
    )
    .expect("the file opens");
    assert_eq!(after, vec!["02|3000".to_string()]);
    assert_eq!(
        rows_after_open(&database, "PRAGMA integrity_check").expect("the file opens"),
        vec!["ok".to_string()]
    );
}

/// **A writer killed after a rollback and a later commit leaves a file that
/// opens with every committed row.**
#[test]
fn a_rollback_then_a_commit_survive_a_kill() {
    let directory = area("rollback-kill");
    let database = directory.join("killed.rdb");
    prepared(&database, 1000, 300);

    let mut writer = OpenShell::start(&database);
    writer.run_until(
        "PRAGMA locking_mode = exclusive;\n\
         BEGIN; UPDATE t SET v = x'02' || zeroblob(299); COMMIT;\n\
         BEGIN; UPDATE t SET v = x'03' || zeroblob(299); ROLLBACK;\n\
         BEGIN; UPDATE t SET v = x'04' || zeroblob(299); COMMIT;\n\
         SELECT 'written';\n",
        "written",
    );
    let _ = writer.child.kill();
    let _ = writer.child.wait();

    let after = rows_after_open(
        &database,
        "SELECT hex(substr(v, 1, 1)), count(*) FROM t GROUP BY 1",
    );
    assert_eq!(after, Ok(vec!["04|1000".to_string()]));
    assert_eq!(
        rows_after_open(&database, "PRAGMA integrity_check"),
        Ok(vec!["ok".to_string()])
    );
}

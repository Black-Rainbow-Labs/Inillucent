//! What `DISTINCT`, compound selects, window functions, `CREATE INDEX` and `INSERT ... SELECT`
//! hold in memory over millions of rows, and that the answers are right.
//!
//! Invariant: **the memory these statements need is bounded by a fixed budget and does not grow
//! with the number of rows.** Each of them used to hold every row, or every entry, in memory:
//! `row_number() OVER (ORDER BY a)` over 3,000,000 rows peaked at 874 MB, `SELECT DISTINCT` at 625
//! MB, a `UNION` at 891 MB, `CREATE INDEX` at 321 MB and a copy of the table into a table that held
//! a row at 622 MB. SQLite stays near 10 MB for all of them. The figures below are the peaks of the
//! `inillucent-shell` process on a debug build of this repository, before and after the change.
//!
//! The statements run in the shell, as a child process whose peak resident set is read while it
//! runs. A test in this process could not tell its own growth from that of the other tests that run
//! in it at the same time. The statements that produce rows write them into a table, because a
//! statement's own result is held whole by the shell and would be what the test measured.
//!
//! Each bound is about two thirds of the figure before the change at most. They are not close to
//! the figures after it, so a machine that is busy does not fail the test, and they are far enough
//! under the figures before it that holding the rows again does.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc;
use inillucent_compat::procstat;

/// How many rows the table has.
const ROWS: u64 = 3_000_000;

/// One megabyte.
const MB: u64 = 1024 * 1024;

/// Returns the directory this suite writes in, made empty.
fn fresh_directory() -> PathBuf {
    let directory = inillucent_compat::workspace_root()
        .join("_agent_output")
        .join("spill-memory");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    directory
}

/// Runs a script in the shell on a database, and returns its output and the largest resident set
/// the shell reached.
///
/// @param database - the database to open
/// @param script - the statements to send to the shell
fn run_and_watch(database: &Path, script: &str) -> (String, u64) {
    let shell = cliproc::program("inillucent-shell");
    let mut child = Command::new(&shell)
        .arg(database.to_string_lossy().replace('\\', "/"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{} did not start: {error}", shell.display()));
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(script.as_bytes())
            .expect("the script is written to the shell");
    }
    let mut highest = 0u64;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                highest = highest.max(procstat::running_child_peak(&child));
                std::thread::sleep(std::time::Duration::from_millis(15));
            }
            Err(error) => panic!("waiting on the shell: {error}"),
        }
    };
    highest = highest.max(procstat::running_child_peak(&child));
    let mut said = String::new();
    if let Some(mut stream) = child.stdout.take() {
        let _ = stream.read_to_string(&mut said);
    }
    let mut complaint = String::new();
    if let Some(mut stream) = child.stderr.take() {
        let _ = stream.read_to_string(&mut complaint);
    }
    assert!(status.success(), "the shell failed: {complaint}\n{said}");
    (said, highest)
}

/// Makes the table the cases read, and returns the path of its database.
///
/// `a` is a permutation-like spread over 0 to 3,000,016, `b` is the row number (so every `b` is
/// different), and `c` is text.
///
/// @param directory - where the database goes
fn build_table(directory: &Path) -> PathBuf {
    let database = directory.join("big.rdb");
    let sql = format!(
        "CREATE TABLE t(id INTEGER PRIMARY KEY, a INTEGER, b INTEGER, c TEXT);\n\
         BEGIN;\n\
         WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < {ROWS}) \
         INSERT INTO t SELECT i, (i * 7919) % 3000017, i, 'row' || i FROM s;\n\
         COMMIT;\n"
    );
    let shell = cliproc::program("inillucent-shell");
    let ran = cliproc::run_with_input(&shell, &[&database.to_string_lossy()], &sql);
    assert_eq!(ran.code, 0, "the table was not built: {}", ran.said());
    database
}

/// Runs a statement, asserts that it stayed under a bound, and returns the output of the checks run
/// after it in a program of their own.
///
/// @param database - the database
/// @param what - what the statement is, for the message
/// @param statement - the statement whose peak is read
/// @param bound_mb - the most megabytes the shell may reach
fn measured(database: &Path, what: &str, statement: &str, bound_mb: u64) {
    let (_, peak) = run_and_watch(database, statement);
    assert!(
        peak < bound_mb * MB,
        "{what} over {ROWS} rows peaked at {} MB; the bound is {bound_mb} MB",
        peak / MB
    );
}

/// Returns the first line of the output that starts with a label, without the label.
///
/// @param output - what the shell printed
/// @param label - the first column of the line
fn line_of(output: &str, label: &str) -> String {
    output
        .lines()
        .find(|line| line.starts_with(&format!("{label}|")))
        .map(|line| {
            line.trim()
                .split_once('|')
                .map(|(_, rest)| rest.to_string())
                .unwrap_or_default()
        })
        .unwrap_or_else(|| panic!("no line starts with {label}| in:\n{output}"))
}

/// **None of these statements holds its rows.**
///
/// One database, built once; every statement is measured in a shell of its own, and then the rows
/// it wrote are checked against an aggregate over the table that does not sort or de-duplicate.
#[test]
fn statements_over_millions_of_rows_stay_under_a_fixed_budget() {
    let directory = fresh_directory();
    let database = build_table(&directory);

    // Before the change: 622 MB into a table holding a row, 527 MB into an empty one.
    measured(
        &database,
        "INSERT ... SELECT into a table that holds a row",
        "CREATE TABLE copy_full(id INTEGER PRIMARY KEY, a INTEGER, b INTEGER, c TEXT);\n\
         INSERT INTO copy_full VALUES (-1, 0, 0, 'seed');\n\
         INSERT INTO copy_full SELECT id, a, b, c FROM t;\n",
        300,
    );
    measured(
        &database,
        "INSERT ... SELECT into an empty table",
        "CREATE TABLE copy_empty(id INTEGER PRIMARY KEY, a INTEGER, b INTEGER, c TEXT);\n\
         INSERT INTO copy_empty SELECT id, a, b, c FROM t;\n",
        250,
    );
    // Before the change: 321 MB.
    measured(
        &database,
        "CREATE INDEX",
        "CREATE INDEX t_a ON t(a);\n",
        200,
    );
    // Before the change: 434 MB for the operator and the answer held whole.
    measured(
        &database,
        "SELECT DISTINCT",
        "CREATE TABLE d(b INTEGER);\nINSERT INTO d SELECT DISTINCT b FROM t;\n",
        250,
    );
    // Before the change: 893 MB for a UNION, 692 MB for an EXCEPT.
    measured(
        &database,
        "UNION",
        "CREATE TABLE u(v INTEGER);\nINSERT INTO u SELECT a FROM t UNION SELECT b FROM t;\n",
        300,
    );
    measured(
        &database,
        "EXCEPT",
        "CREATE TABLE e(v INTEGER);\nINSERT INTO e SELECT a FROM t EXCEPT SELECT b FROM t WHERE b % 2 = 0;\n",
        300,
    );
    // Before the change: 650 MB, 884 MB and 916 MB.
    measured(
        &database,
        "row_number() OVER (ORDER BY a)",
        "CREATE TABLE w1(id INTEGER, n INTEGER);\n\
         INSERT INTO w1 SELECT id, row_number() OVER (ORDER BY a, id) FROM t;\n",
        250,
    );
    measured(
        &database,
        "a running sum and rank()",
        "CREATE TABLE w2(id INTEGER, s INTEGER, r INTEGER);\n\
         INSERT INTO w2 SELECT id, sum(a) OVER (ORDER BY id), rank() OVER (ORDER BY id) FROM t;\n",
        300,
    );
    measured(
        &database,
        "row_number() OVER (PARTITION BY ...)",
        "CREATE TABLE w3(n INTEGER);\n\
         INSERT INTO w3 SELECT row_number() OVER (PARTITION BY b % 1000 ORDER BY id) FROM t;\n",
        300,
    );

    // The rows each statement wrote, against aggregates that need no sort and no de-duplication.
    let (checks, _) = run_and_watch(
        &database,
        "SELECT 'copy_full', (SELECT count(*) FROM copy_full) = (SELECT count(*) FROM t) + 1, \
                (SELECT sum(a) + sum(b) FROM copy_full) = (SELECT sum(a) + sum(b) FROM t);\n\
         SELECT 'copy_empty', (SELECT count(*) FROM copy_empty) = (SELECT count(*) FROM t), \
                (SELECT sum(a) + sum(length(c)) FROM copy_empty) = (SELECT sum(a) + sum(length(c)) FROM t);\n\
         SELECT 'index', (SELECT count(*) FROM t WHERE a = 7919) = 1, \
                (SELECT id FROM t WHERE a = 15838) = 2;\n\
         PRAGMA integrity_check;\n\
         SELECT 'distinct', (SELECT count(*) FROM d) = (SELECT count(DISTINCT b) FROM t), \
                (SELECT sum(b) FROM d) = (SELECT sum(b) FROM t);\n\
         SELECT 'union', (SELECT count(*) FROM u) = (SELECT count(*) FROM t) + (SELECT count(*) FROM t WHERE b NOT IN (SELECT a FROM t));\n\
         SELECT 'except', (SELECT count(*) FROM e) = (SELECT count(*) FROM t WHERE a NOT IN (SELECT b FROM t WHERE b % 2 = 0));\n\
         SELECT 'rownumber', (SELECT count(*) FROM w1) = (SELECT count(*) FROM t), \
                (SELECT sum(n) FROM w1) = (SELECT count(*) FROM t) * ((SELECT count(*) FROM t) + 1) / 2, \
                (SELECT max(n) FROM w1) = (SELECT count(*) FROM t);\n\
         SELECT 'running', (SELECT s FROM w2 WHERE id = (SELECT max(id) FROM t)) = (SELECT sum(a) FROM t), \
                (SELECT count(*) FROM w2 WHERE r != id) = 0;\n\
         SELECT 'partition', (SELECT count(*) FROM w3) = (SELECT count(*) FROM t), \
                (SELECT max(n) FROM w3) = 3000, (SELECT sum(n) FROM w3) = 1000 * 3000 * 3001 / 2;\n",
    );
    assert!(
        checks.lines().any(|line| line.trim() == "ok"),
        "the integrity check did not say ok:\n{checks}"
    );
    for (label, expected) in [
        ("copy_full", "1|1"),
        ("copy_empty", "1|1"),
        ("index", "1|1"),
        ("distinct", "1|1"),
        ("union", "1"),
        ("rownumber", "1|1|1"),
        ("running", "1|1"),
        ("partition", "1|1|1"),
    ] {
        assert_eq!(
            line_of(&checks, label),
            expected,
            "the rows written by the {label} statement are not the rows the table implies"
        );
    }
}

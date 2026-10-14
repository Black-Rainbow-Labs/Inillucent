//! What a write transaction holds in memory while it is open, and that a rollback still works
//! once the oldest of it has been moved to a file.
//!
//! Invariant: **the memory a transaction needs for its undo records is bounded, and does not
//! grow with the number of rows the transaction changes; and a rollback of a transaction that
//! changed more rows than fit in that bound puts every row back.** A transaction used to keep
//! one record for each row it changed until it ended, and a record holds the row as it was, or a
//! copy of the leaf the row was in. `BEGIN; UPDATE big SET v = v + 1; COMMIT` over a 205 MB table
//! peaked at 388 MB and over a 410 MB table at 638 MB (bug hunt 10), and `PRAGMA cache_size` did
//! not change that, because the page cache already spills its pages and these records are not
//! pages of the cache.
//!
//! The statements run in the `inillucent-shell` program, as a child process whose peak resident
//! set is read while it runs. A test in this process could not tell its own growth from that of
//! the other tests that run in it at the same time.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc;
use inillucent_compat::procstat;

/// How many rows the table has. At 900 bytes a row the table is about 110 MB, seven times the
/// 16 MB the undo records may occupy in memory.
const ROWS: u64 = 120_000;

/// The bytes one row of the table occupies, to within a few percent.
const ROW_BYTES: u64 = 900;

/// Returns the directory this suite writes in, made empty.
///
/// @param name - the case's name
fn fresh_directory(name: &str) -> PathBuf {
    let directory = inillucent_compat::workspace_root()
        .join("_agent_output")
        .join("transaction-memory")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    directory
}

/// Makes a database holding the table the cases change, and returns its path.
///
/// The payload is text that reads as the number `id`, so `sum(payload + 0)` over the table is a
/// checksum of every payload: a row put back with another row's payload changes it.
///
/// @param directory - where the database goes
fn build_table(directory: &Path) -> PathBuf {
    let database = directory.join("big.rdb");
    let sql = format!(
        "CREATE TABLE big(id INTEGER PRIMARY KEY, v INTEGER, payload TEXT);\n\
         BEGIN;\n\
         WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < {ROWS}) \
         INSERT INTO big SELECT i, i % 1000, printf('%0{ROW_BYTES}d', i) FROM s;\n\
         COMMIT;\n"
    );
    let shell = cliproc::program("inillucent-shell");
    let ran = cliproc::run_with_input(&shell, &[&database.to_string_lossy()], &sql);
    assert_eq!(ran.code, 0, "the table was not built: {}", ran.said());
    database
}

/// Runs a script in the shell on a database, and returns its output and the largest resident set
/// the shell reached.
///
/// Sampled while the shell runs, because a process that has ended reports nothing about how
/// large it got on every platform.
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

/// Returns the line of the output that starts with a label.
///
/// @param output - what the shell printed
/// @param label - the first column of the line
fn line_of(output: &str, label: &str) -> String {
    output
        .lines()
        .find(|line| line.starts_with(&format!("{label}|")))
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| panic!("no line starts with {label}| in:\n{output}"))
}

/// Returns the line with the label replaced, so lines taken at different times can be compared.
///
/// @param line - a line from [`line_of`]
fn without_label(line: &str) -> String {
    line.split_once('|')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_default()
}

/// An update of a table seven times the size of its undo limit stays under a fixed budget.
///
/// The cache is set to 4 MiB, so the cache is not what the peak measures. The bound is 0.7 times
/// the size of the table, which is more than the 16 MB of undo records, the 4 MB cache, the
/// statement's list of keys and the program itself add up to. With the undo records kept in
/// memory the same update peaked at 166 MB for this table. Both updates are run: `SET v = v + 1`
/// changes one column and records a copy of each leaf, and the second changes two columns and
/// records each row.
#[test]
fn an_update_of_a_table_larger_than_its_undo_limit_stays_under_a_fixed_budget() {
    let directory = fresh_directory("budget");
    let database = build_table(&directory);
    let table_bytes = ROWS * ROW_BYTES;
    let bound = table_bytes * 7 / 10;
    let scripts = [
        (
            "one column",
            "PRAGMA cache_size = -4096;\nBEGIN;\nUPDATE big SET v = v + 1;\nCOMMIT;\n",
        ),
        (
            "two columns",
            "PRAGMA cache_size = -4096;\nBEGIN;\nUPDATE big SET v = v + 1, payload = printf('%0900d', id);\nCOMMIT;\n",
        ),
    ];
    for (what, script) in scripts {
        let (_, peak) = run_and_watch(&database, script);
        assert!(
            peak < bound,
            "an update of {what} over a table of {} MB peaked at {} MB; the bound is {} MB",
            table_bytes >> 20,
            peak >> 20,
            bound >> 20
        );
    }
}

/// `VACUUM` of a table of many pages keeps every row, and does not hold the whole table.
///
/// Rows are deleted first so the rowids have gaps, which is what a page that starts after the
/// last rowid of the one before has to cope with. The checksums are taken by the same statements
/// before and after. The bound is 1.8 times the table: the two page caches the rebuild opens are
/// 128 MB between them whatever `cache_size` says, and with the whole table read first the same
/// rebuild peaked at 258 MB for this table.
#[test]
fn vacuum_of_a_large_table_keeps_every_row_and_does_not_hold_the_table() {
    let directory = fresh_directory("vacuum");
    let database = build_table(&directory);
    let sum = |label: &str| {
        format!("SELECT '{label}', sum(v), sum(payload + 0), count(*), max(id) FROM big;\n")
    };
    // A program of its own for the deletes, so the cache that reading the table filled is not in
    // the process whose peak is read.
    let prepare = format!("DELETE FROM big WHERE id % 7 = 0;\n{}", sum("before"));
    let (prepared, _) = run_and_watch(&database, &prepare);
    let (_, peak) = run_and_watch(&database, "PRAGMA cache_size = -4096;\nVACUUM;\n");
    // Read in a third program, for the same reason.
    let mut script = sum("after");
    script.push_str("PRAGMA integrity_check;\n");
    let (output, _) = run_and_watch(&database, &script);
    let before = without_label(&line_of(&prepared, "before"));
    let after = without_label(&line_of(&output, "after"));
    assert_eq!(after, before, "VACUUM changed what the table holds");
    assert!(
        output.lines().any(|line| line.trim() == "ok"),
        "the integrity check did not say ok:\n{output}"
    );
    let bound = ROWS * ROW_BYTES * 18 / 10;
    assert!(
        peak < bound,
        "VACUUM of a table of {} MB peaked at {} MB; the bound is {} MB",
        (ROWS * ROW_BYTES) >> 20,
        peak >> 20,
        bound >> 20
    );
}

/// A rollback restores every row after the transaction changed more rows than fit in memory.
///
/// Three rollbacks, each past the undo limit: a whole transaction of updates, the second half of a
/// transaction rolled back to a savepoint, and a delete. The checksums are taken from the same
/// statements before and after, so a row put back wrong shows in the one that differs.
#[test]
fn a_rollback_restores_every_row_after_the_undo_records_were_moved_to_a_file() {
    let directory = fresh_directory("rollback");
    let database = build_table(&directory);
    let sum =
        |label: &str| format!("SELECT '{label}', sum(v), sum(payload + 0), count(*) FROM big;\n");
    let mut script = String::from("PRAGMA cache_size = -4096;\n");
    script.push_str(&sum("start"));
    script
        .push_str("BEGIN;\nUPDATE big SET v = v + 1, payload = printf('%0900d', id + 1000000);\n");
    script.push_str(&sum("inside"));
    script.push_str("ROLLBACK;\n");
    script.push_str(&sum("rolled"));
    script.push_str("BEGIN;\nUPDATE big SET v = v + 1;\nSAVEPOINT s;\n");
    script.push_str("UPDATE big SET v = v + 10, payload = printf('%0900d', id + 2000000);\n");
    script.push_str("ROLLBACK TO s;\nCOMMIT;\n");
    script.push_str(&sum("savepoint"));
    script.push_str("BEGIN;\nDELETE FROM big WHERE id % 2 = 0;\n");
    script.push_str(&sum("deleted"));
    script.push_str("ROLLBACK;\n");
    script.push_str(&sum("restored"));
    let (output, _) = run_and_watch(&database, &script);

    let start = without_label(&line_of(&output, "start"));
    let inside = without_label(&line_of(&output, "inside"));
    let rolled = without_label(&line_of(&output, "rolled"));
    assert_ne!(start, inside, "the update changed nothing");
    assert_eq!(rolled, start, "a rolled back update left rows changed");

    let fields: Vec<&str> = start.split('|').collect();
    let (v, payloads, count) = match fields.as_slice() {
        [v, payloads, count] => (
            v.parse::<i64>().expect("sum(v) is a number"),
            *payloads,
            *count,
        ),
        other => panic!("the start line has {} fields: {start}", other.len()),
    };
    let expected = format!("{}|{payloads}|{count}", v + ROWS as i64);
    let after_savepoint = without_label(&line_of(&output, "savepoint"));
    assert_eq!(
        after_savepoint, expected,
        "a rollback to a savepoint kept or lost rows of the first half"
    );
    let restored = without_label(&line_of(&output, "restored"));
    assert_eq!(
        restored, after_savepoint,
        "a rolled back delete did not put every row back"
    );
    let deleted = without_label(&line_of(&output, "deleted"));
    assert_ne!(deleted, after_savepoint, "the delete changed nothing");
}

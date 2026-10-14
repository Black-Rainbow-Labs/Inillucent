//! What the programs hold in memory while they print or load a large result.
//!
//! Invariant: **the memory `inillucent-shell`, `inillucent query` and `.import` need does not grow
//! with the number of rows they print or load.** The shell used to collect every row of a result
//! before it printed the first, and 3,000,000 rows of an integer and a short text peaked at 984
//! MB. `inillucent query` held every row to cut the list to its limit and peaked at 896 MB.
//! `sqlite3` stays near 10 MB for all three.
//!
//! Each case runs the program as a child process and reads its largest resident set while it runs.
//! A test in this process could not tell its own growth from that of the other tests that run in
//! it at the same time. The bounds are a small fraction of the sizes above, so a change that
//! collects the rows again fails here, and large enough for the program's own start up and its
//! page cache, which do not depend on the result.
//!
//! The shell modes that size their columns to the widest value (`column`, `table`, `box` and the
//! like) hold the result by definition, as `sqlite3` does, and are not measured here.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc;
use inillucent_compat::procstat;

/// How many rows the table the programs print has.
///
/// At 330 bytes a row, the figure the shell showed before it streamed, 2,000,000 rows peaked at
/// about 660 MB.
const ROWS: u64 = 2_000_000;

/// The most a program may hold while it prints the table in a mode that streams, in bytes.
///
/// The shell measured 70 MB for 3,000,000 rows, which is its start up, the pages it reads and a
/// chunk of rows. The bound is a fifth of what the same rows peaked at before.
const PRINT_BOUND: u64 = 130 << 20;

/// The most the shell may hold while it prints the table sorted, in bytes.
///
/// A sort holds what it sorts, which is the engine's own bound and not the shell's: the result is
/// not held on top of it. Before, the sorted table was held twice, and 3,000,000 rows peaked at
/// 998 MB.
const SORT_BOUND: u64 = 330 << 20;

/// What a program did: how it ended, how large it got, and what it printed.
pub(crate) struct Watched {
    /// The exit code.
    pub(crate) code: i32,
    /// The largest resident set it reached, in bytes.
    pub(crate) peak: u64,
    /// How many bytes it wrote to standard output.
    bytes: u64,
    /// How many lines it wrote.
    lines: u64,
    /// The first bytes it wrote.
    pub(crate) head: String,
    /// The last bytes it wrote.
    tail: String,
    /// Everything it wrote, when that is under a mebibyte.
    whole: String,
    /// What it wrote to standard error.
    pub(crate) stderr: String,
}

/// Returns the directory a case writes in, made empty.
///
/// @param name - the case's name
pub(crate) fn fresh_directory(name: &str) -> PathBuf {
    let directory = inillucent_compat::workspace_root()
        .join("_agent_output")
        .join("streaming-memory")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    directory
}

/// Runs a program with an input, reading standard output as it is written, and returns what
/// happened.
///
/// Standard output is read while the program runs: a result of this size does not fit in a pipe, so
/// a program whose output was read after it ended would wait for a reader forever.
///
/// @param program - the binary
/// @param arguments - the command line
/// @param input - what to write to standard input
pub(crate) fn watch(program: &Path, arguments: &[&str], input: &str) -> Watched {
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{} did not start: {error}", program.display()));
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(input.as_bytes())
            .expect("the input is written");
    }
    let mut out = child.stdout.take().expect("standard output is piped");
    let reader = std::thread::spawn(move || {
        let mut buffer = vec![0u8; 1 << 16];
        let (mut bytes, mut lines) = (0u64, 0u64);
        let mut head: Vec<u8> = Vec::new();
        let mut tail: Vec<u8> = Vec::new();
        let mut whole: Vec<u8> = Vec::new();
        loop {
            let read = match out.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(read) => read,
            };
            let piece = buffer.get(..read).unwrap_or_default();
            bytes += read as u64;
            lines += piece.iter().filter(|byte| **byte == b'\n').count() as u64;
            if whole.len() < (1 << 20) {
                whole.extend_from_slice(piece);
            }
            if head.len() < 200 {
                head.extend_from_slice(piece);
                head.truncate(200);
            }
            tail.extend_from_slice(piece);
            if tail.len() > 400 {
                tail.drain(..tail.len() - 200);
            }
        }
        (bytes, lines, head, tail, whole)
    });
    let mut err = child.stderr.take().expect("standard error is piped");
    let complaint = std::thread::spawn(move || {
        let mut said = String::new();
        let _ = err.read_to_string(&mut said);
        said
    });
    let mut peak = 0u64;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                peak = peak.max(procstat::running_child_peak(&child));
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => panic!("waiting on {}: {error}", program.display()),
        }
    };
    peak = peak.max(procstat::running_child_peak(&child));
    let (bytes, lines, head, tail, whole) = reader.join().expect("the reader finishes");
    Watched {
        code: status.code().unwrap_or(-1),
        peak,
        bytes,
        lines,
        head: String::from_utf8_lossy(&head).into_owned(),
        tail: String::from_utf8_lossy(&tail).into_owned(),
        whole: String::from_utf8_lossy(&whole).into_owned(),
        stderr: complaint.join().expect("the error reader finishes"),
    }
}

/// Makes a database holding the table the cases print, and returns its path.
///
/// @param directory - where the database goes
fn build_table(directory: &Path) -> PathBuf {
    let database = directory.join("big.rdb");
    let sql = format!(
        "CREATE TABLE t(a INTEGER, b TEXT);\n\
         BEGIN;\n\
         WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < {ROWS}) \
         INSERT INTO t SELECT i, 'row-' || i FROM s;\n\
         COMMIT;\n\
         PRAGMA wal_checkpoint;\n"
    );
    let shell = cliproc::program("inillucent-shell");
    let ran = cliproc::run_with_input(&shell, &[&database.to_string_lossy()], &sql);
    assert_eq!(ran.code, 0, "the table was not built: {}", ran.said());
    database
}

/// Printing the table in a mode that streams holds a chunk of rows and not the table, and prints
/// every row.
#[test]
fn the_shell_prints_a_large_table_in_bounded_memory() {
    let directory = fresh_directory("print");
    let database = build_table(&directory);
    let shell = cliproc::program("inillucent-shell");
    let path = database.to_string_lossy().replace('\\', "/");
    let last = ROWS.to_string();
    for (mode, first, end) in [
        ("list", "1|row-1".to_string(), format!("{last}|row-{last}")),
        ("csv", "1,row-1".to_string(), format!("{last},row-{last}")),
        (
            "json",
            "[{\"a\":1,\"b\":\"row-1\"},".to_string(),
            format!("{{\"a\":{last},\"b\":\"row-{last}\"}}]"),
        ),
        (
            "insert",
            "INSERT INTO tab VALUES(1,'row-1');".to_string(),
            format!("INSERT INTO tab VALUES({last},'row-{last}');"),
        ),
    ] {
        let ran = watch(
            &shell,
            &[&path],
            &format!(".mode {mode}\nSELECT a, b FROM t;\n"),
        );
        assert_eq!(ran.code, 0, "{mode}: {}", ran.stderr);
        assert_eq!(ran.lines, ROWS, "{mode}: one line for every row");
        assert!(ran.head.starts_with(&first), "{mode} began {:?}", ran.head);
        assert!(
            ran.tail.trim_end().ends_with(&end),
            "{mode} ended {:?}",
            ran.tail
        );
        assert!(
            ran.peak < PRINT_BOUND,
            "printing {ROWS} rows in {mode} mode peaked at {} MB; the bound is {} MB",
            ran.peak >> 20,
            PRINT_BOUND >> 20
        );
    }
}

/// A sorted result is held by the sort and not a second time by the shell.
#[test]
fn the_shell_prints_a_sorted_table_without_holding_the_result_as_well() {
    let directory = fresh_directory("sorted");
    let database = build_table(&directory);
    let shell = cliproc::program("inillucent-shell");
    let path = database.to_string_lossy().replace('\\', "/");
    let ran = watch(&shell, &[&path], "SELECT a, b FROM t ORDER BY b;\n");
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    assert_eq!(ran.lines, ROWS);
    assert!(
        ran.peak < SORT_BOUND,
        "printing {ROWS} sorted rows peaked at {} MB; the bound is {} MB",
        ran.peak >> 20,
        SORT_BOUND >> 20
    );
}

/// `inillucent query` holds the rows it hands back and counts the rest, and the count is exact.
#[test]
fn query_holds_only_the_rows_it_hands_back_and_counts_the_rest() {
    let directory = fresh_directory("query");
    let database = build_table(&directory);
    let program = cliproc::program("inillucent");
    let path = database.to_string_lossy().replace('\\', "/");
    let text = watch(
        &program,
        &["query", "SELECT a, b FROM t", "--db", &path],
        "",
    );
    assert_eq!(text.code, 0, "{}", text.stderr);
    assert!(
        text.tail.contains(&format!("(200 of {ROWS} rows)")),
        "the text reports the count: {:?}",
        text.tail
    );
    let json = watch(
        &program,
        &[
            "query",
            "SELECT a, b FROM t",
            "--db",
            &path,
            "--output",
            "json",
        ],
        "",
    );
    assert_eq!(json.code, 0, "{}", json.stderr);
    for expected in [format!("\"total\": {ROWS}"), "\"more\": true".to_string()] {
        assert!(
            json.whole.contains(&expected),
            "the JSON holds {expected}: {:?} ... {:?}",
            json.head,
            json.tail
        );
    }
    for ran in [&text, &json] {
        assert!(
            ran.peak < PRINT_BOUND,
            "query over {ROWS} rows peaked at {} MB; the bound is {} MB",
            ran.peak >> 20,
            PRINT_BOUND >> 20
        );
    }
}

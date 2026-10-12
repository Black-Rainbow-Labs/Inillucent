//! The shell's command line, exit status and bytes, driven as a separate process.
//!
//! Invariant: **what the shell writes and the status it ends with are what
//! `sqlite3` writes and ends with for the same input.** The cases here are the
//! ones the usage corpus cannot express, because it feeds a script on standard
//! input and compares text: arguments, exit codes, input and output that are not
//! UTF-8, and files that an option must not create.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use inillucent_compat::cliproc::program;
use inillucent_compat::workspace_root;

/// What a run produced.
struct Ran {
    /// The exit status.
    code: Option<i32>,
    /// Standard output, as bytes.
    out: Vec<u8>,
    /// Standard error, as text.
    err: String,
    /// The directory it ran in.
    dir: PathBuf,
}

/// Returns an empty directory of this file's own for one case.
///
/// @param name - the case
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/shell-command-line")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}

/// Runs the shell in a directory of its own, with the given input.
///
/// @param name - the case, which names the directory
/// @param arguments - the command line
/// @param input - what is written to standard input
/// @param files - files to create in the directory first
fn run(name: &str, arguments: &[&str], input: &[u8], files: &[(&str, &[u8])]) -> Ran {
    let dir = area(name);
    for (file, bytes) in files {
        std::fs::write(dir.join(file), bytes).expect("a fixture is written");
    }
    let mut child = Command::new(program("inillucent-shell"))
        .args(arguments)
        .current_dir(&dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the shell starts");
    if let Some(pipe) = child.stdin.as_mut() {
        let _ = pipe.write_all(input);
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the shell finishes");
    Ran {
        code: output.status.code(),
        out: output.stdout,
        err: String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
        dir,
    }
}

/// Returns standard output with the line endings Windows adds taken out.
///
/// @param ran - the run
fn text(ran: &Ran) -> String {
    String::from_utf8_lossy(&ran.out).replace("\r\n", "\n")
}

/// `.exit N` ends the process with N, and `.quit N` with 0.
#[test]
fn exit_ends_the_process_with_its_status() {
    let ran = run("exit", &[], b"select 1;\n.exit 3\nselect 2;\n", &[]);
    assert_eq!(ran.code, Some(3));
    assert_eq!(text(&ran), "1\n");
    assert_eq!(run("quit", &[], b".quit 3\n", &[]).code, Some(0));
    assert_eq!(run("exit-cmd", &[], b"select 5;\n", &[]).code, Some(0));
    let cmd = run("exit-cmd2", &["-cmd", ".exit 9"], b"select 5;\n", &[]);
    assert_eq!(cmd.code, Some(9));
    assert_eq!(text(&cmd), "");
    let read = run(
        "exit-read",
        &[],
        b".read r.sql\nselect 3;\n",
        &[("r.sql", b"select 1;\n.exit 5\nselect 2;\n")],
    );
    assert_eq!(read.code, Some(5));
    assert_eq!(text(&read), "1\n");
}

/// `.quit` inside a file ends that file and not the session.
#[test]
fn quit_in_a_read_file_ends_only_that_file() {
    let ran = run(
        "quit-read",
        &[],
        b".read r.sql\nselect 3;\n",
        &[("r.sql", b"select 1;\n.quit\nselect 2;\n")],
    );
    assert_eq!(text(&ran), "1\n3\n");
}

/// Statement arguments run one at a time and the first failure stops the rest.
#[test]
fn statement_arguments_run_separately() {
    let ran = run(
        "arguments",
        &[
            ":memory:",
            "create table t(a)",
            "insert into t values(1)",
            "select * from t",
        ],
        b"",
        &[],
    );
    assert_eq!((ran.code, text(&ran).as_str()), (Some(0), "1\n"));
    let failing = run(
        "arguments-fail",
        &[":memory:", "select 1;", "select * from nope;", "select 2;"],
        b"",
        &[],
    );
    assert_eq!(failing.code, Some(1));
    assert_eq!(text(&failing), "1\n");
    assert!(
        failing
            .err
            .contains("Parse error in 3rd command line argument: no such table: nope"),
        "{}",
        failing.err
    );
    let extra = run(
        "arguments-extra",
        &[":memory:", "select 1", "extra"],
        b"",
        &[],
    );
    assert_eq!(extra.code, Some(1));
    assert!(
        extra.err.contains("in 3rd command line argument"),
        "{}",
        extra.err
    );
}

/// An unknown option is status 1, names the option, and creates nothing.
#[test]
fn an_unknown_option_exits_with_one() {
    let ran = run("unknown", &["-bogus", "x.db"], b"", &[]);
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.err.contains("Error: unknown option: -bogus"),
        "{}",
        ran.err
    );
    assert!(!ran.dir.join("x.db").exists());
}

/// `-readonly` on a file that is not there fails and creates no file.
#[test]
fn readonly_on_a_missing_file_fails_without_creating_it() {
    let ran = run(
        "readonly",
        &["-readonly", "missing.db"],
        b"select 1;\n",
        &[],
    );
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.err.contains("unable to open database file"),
        "{}",
        ran.err
    );
    assert!(!ran.dir.join("missing.db").exists());
    let dot = run(
        "readonly-dot",
        &[],
        b".open --readonly missing.db\n.open --new fresh.db\n.open --bogus x\n",
        &[],
    );
    assert!(!dot.dir.join("missing.db").exists());
    assert!(!dot.dir.join("--readonly").exists());
    assert!(!dot.dir.join("--new").exists());
    assert!(!dot.dir.join("--bogus").exists());
    assert!(dot.err.contains("unknown option: --bogus"), "{}", dot.err);
}

/// A recursive `.read` stops at nesting depth 25 with status 1.
#[test]
fn a_recursive_read_stops_at_the_nesting_limit() {
    let ran = run(
        "recursion",
        &[],
        b".read s.sql\nselect 3;\n",
        &[("s.sql", b"select 1;\n.read s.sql\nselect 2;\n")],
    );
    assert_eq!(ran.code, Some(1));
    assert!(
        ran.err
            .contains("s.sql: Input nesting limit (25) reached at line 2. Check recursion."),
        "{}",
        ran.err
    );
    assert_eq!(text(&ran).matches("1\n").count(), 24 + 1 - 1);
    assert!(text(&ran).ends_with("3\n"));
}

/// Bytes that are not UTF-8 pass through input and output unchanged.
#[test]
fn bytes_that_are_not_utf8_pass_through() {
    let ran = run(
        "bytes",
        &[],
        b"select 'caf\xe9';\nselect x'ff41fe';\nselect 2;\n",
        &[],
    );
    assert_eq!(ran.code, Some(0));
    let want: Vec<u8> = [&b"caf\xe9\n"[..], b"\xffA\xfe\n", b"2\n"].concat();
    assert_eq!(
        ran.out
            .iter()
            .copied()
            .filter(|b| *b != b'\r')
            .collect::<Vec<u8>>(),
        want
    );
    let imported = run(
        "latin1",
        &[],
        b".mode csv\n.import l.csv t\nselect * from t;\n",
        &[("l.csv", b"a\ncaf\xe9\n")],
    );
    assert!(imported.out.windows(4).any(|w| w == b"caf\xe9"));
}

/// A byte order mark at the start of standard input is skipped.
#[test]
fn a_byte_order_mark_is_skipped() {
    let ran = run("bom", &[], b"\xef\xbb\xbfselect 1;\nselect 2;\n", &[]);
    assert_eq!((ran.code, text(&ran).as_str()), (Some(0), "1\n2\n"));
}

/// A setting command with the wrong arguments prints usage and fails.
#[test]
fn a_setting_without_its_argument_fails() {
    let ran = run("usage", &[], b".headers\n", &[]);
    assert_eq!(ran.code, Some(1));
    assert!(ran.err.contains("Usage: .headers on|off"), "{}", ran.err);
    let bad = run("boolean", &[], b".headers maybe\n", &[]);
    assert_eq!(bad.code, Some(0));
    assert!(
        bad.err.contains("Not a boolean value: \"maybe\""),
        "{}",
        bad.err
    );
}

/// `-newline` and `-ascii` set the row separator and the ASCII mode.
#[test]
fn newline_and_ascii_flags_set_the_separators() {
    let ran = run(
        "newline",
        &["-newline", "|", "-separator", ";"],
        b"select 1,2;select 3,4;\n",
        &[],
    );
    assert_eq!(text(&ran), "1;2|3;4|");
    let ascii = run("ascii", &["-ascii"], b"select 1,2;\n", &[]);
    assert_eq!(ascii.out, b"1\x1f2\x1e");
}

/// `.once FILE` covers one command, and `.output stderr` and `off` are places.
#[test]
fn once_covers_one_command() {
    let ran = run(
        "once",
        &[],
        b".once o.txt\n.print p\nselect 7;\n.output stderr\nselect 8;\n.output off\nselect 9;\n.output\nselect 10;\n",
        &[],
    );
    assert_eq!(text(&ran), "7\n10\n");
    assert_eq!(
        std::fs::read_to_string(ran.dir.join("o.txt")).expect("o.txt"),
        "p\n"
    );
    assert!(ran.err.contains('8'));
    assert!(!ran.dir.join("stderr").exists() && !ran.dir.join("off").exists());
}

/// A program that fails reports its status and the shell goes on.
#[test]
fn a_failing_system_command_is_reported_and_not_fatal() {
    let ran = run(
        "system",
        &[],
        b"select 1;\n.system exit 3\nselect 2;\n",
        &[],
    );
    assert_eq!(ran.code, Some(0));
    assert_eq!(text(&ran), "1\n2\n");
    assert!(ran.err.contains("System command returns 3"), "{}", ran.err);
}

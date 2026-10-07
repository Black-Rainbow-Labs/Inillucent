//! The shell's standard output, written in blocks when it is not a terminal.
//!
//! What this file checks (task-2197): rows and errors on one pipe arrive in
//! the order they were produced, a program that sends a statement and waits for
//! its answer gets the answer before the shell waits for the next one, and
//! output larger than one block arrives whole.
//!
//! Invariant: **every test drives the built shell as a separate process**, so
//! what is checked is what a pipe on the other end receives.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use inillucent_compat::cliproc::program;
use inillucent_compat::workspace_root;

/// Returns a fresh database path for one test, in a directory of its own.
///
/// @param name - the test's name
fn database(name: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output")
        .join("shell-output")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory.join("output.rdb")
}

/// Rows and errors written to one pipe come out in the order the script
/// produced them, as they did when every line was flushed.
#[test]
fn rows_and_errors_keep_their_order_on_one_pipe() {
    let path = database("order");
    let (mut reader, writer) = std::io::pipe().expect("a pipe");
    let mut child = Command::new(program("inillucent-shell"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(writer.try_clone().expect("a second writer"))
        .stderr(writer)
        .spawn()
        .expect("the shell started");
    if let Some(input) = child.stdin.as_mut() {
        input
            .write_all(
                b"SELECT 1;\nSELECT 2;\nSELEC 3;\nSELECT 4;\nSELECT * FROM missing;\nSELECT 6;\n",
            )
            .expect("the script written");
    }
    drop(child.stdin.take());
    let mut said = String::new();
    reader.read_to_string(&mut said).expect("the output read");
    let _ = child.wait();
    let lines: Vec<&str> = said.lines().collect();
    let place = |wanted: &str| {
        lines
            .iter()
            .position(|line| line.contains(wanted))
            .unwrap_or_else(|| panic!("{wanted:?} is missing from {said:?}"))
    };
    let order = [
        place("1"),
        place("2"),
        place("SELEC"),
        place("4"),
        place("missing"),
        place("6"),
    ];
    assert!(
        order.windows(2).all(|pair| pair[0] < pair[1]),
        "the output came out of order: {said:?}"
    );
}

/// A program that sends one statement at a time and waits for each answer is
/// answered, because the shell writes what it holds before it waits for input.
#[test]
fn a_program_that_waits_for_each_answer_gets_it() {
    let path = database("interactive");
    let mut child = Command::new(program("inillucent-shell"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the shell started");
    let stdout = child.stdout.take().expect("the shell's output");
    let (send, lines) = mpsc::channel();
    let reading = std::thread::spawn(move || {
        // Stops at the end of the output, or once the test has stopped listening.
        let mut lines = BufReader::new(stdout).lines().map_while(Result::ok);
        while lines.next().is_some_and(|line| send.send(line).is_ok()) {}
    });
    let mut input = child.stdin.take().expect("the shell's input");
    for number in [111, 222, 333] {
        writeln!(input, "SELECT {number};").expect("a statement written");
        input.flush().expect("the statement sent");
        let answer = lines
            .recv_timeout(Duration::from_secs(20))
            .unwrap_or_else(|_| {
                panic!("no answer to SELECT {number} while the shell waited for input")
            });
        assert_eq!(answer, number.to_string());
    }
    drop(input);
    let _ = child.wait();
    let _ = reading.join();
}

/// Output longer than one block arrives whole, the last block included.
#[test]
fn output_longer_than_a_block_arrives_whole() {
    let path = database("long");
    let mut child = Command::new(program("inillucent-shell"))
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("the shell started");
    if let Some(input) = child.stdin.as_mut() {
        input
            .write_all(
                b"WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 100000) SELECT i FROM n;\n",
            )
            .expect("the statement written");
    }
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("the shell finished");
    let said = String::from_utf8_lossy(&output.stdout);
    let numbers: Vec<&str> = said.lines().collect();
    assert_eq!(numbers.len(), 100_000, "the output was cut short");
    assert_eq!(numbers.first().copied(), Some("1"));
    assert_eq!(numbers.last().copied(), Some("100000"));
}

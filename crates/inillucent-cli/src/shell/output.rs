//! The shell's standard output, buffered when it is not a terminal.
//!
//! **Why** (task-2197). The shell printed through `std::io::stdout()`, which
//! flushes at every newline, so every row and every statement's answer was its
//! own write to the pipe. A script of 60,000 point queries spent 62% of its
//! time there. SQLite's shell writes through C's standard output, which holds a
//! block before writing when the output is a pipe or a file.
//!
//! Invariant: **nothing anybody can see changes order.** The buffer is written
//! out before anything goes to standard error, so a reader of both streams on
//! one pipe sees them in the order they were produced. It is written out
//! before the shell reads more input from standard input than it already has,
//! so a program that sends a statement and waits for its answer gets the answer
//! before the shell waits for the next statement. And it is written out when
//! the shell ends, by a normal return, an exit or a panic. A terminal gets
//! every line at once, as before.

use std::io::{IsTerminal, Read, Write};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// How much is held before it is written out.
const BLOCK: usize = 64 << 10;

/// What has been printed and not yet written out.
static HELD: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Whether standard output is a terminal, asked once.
fn is_terminal() -> bool {
    static TERMINAL: OnceLock<bool> = OnceLock::new();
    *TERMINAL.get_or_init(|| std::io::stdout().is_terminal())
}

/// Locks the held output, recovering it if a panic poisoned the lock.
fn held() -> MutexGuard<'static, Vec<u8>> {
    match HELD.lock() {
        Ok(held) => held,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Writes bytes to standard output now.
///
/// @param bytes - what to write
fn write_now(bytes: &[u8]) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(bytes);
    let _ = out.flush();
}

/// Prints one line and its ending.
///
/// @param line - the text, without its ending
/// @param ending - `\n`, or `\r\n` under `.crlf on`
pub fn write_line(line: &str, ending: &str) {
    if is_terminal() {
        let mut out = std::io::stdout().lock();
        let _ = write!(out, "{line}{ending}");
        return;
    }
    let mut held = held();
    held.extend_from_slice(line.as_bytes());
    held.extend_from_slice(ending.as_bytes());
    if held.len() >= BLOCK {
        write_now(&held);
        held.clear();
    }
}

/// Writes out everything held.
pub fn flush() {
    let mut held = held();
    if !held.is_empty() {
        write_now(&held);
        held.clear();
    }
}

/// Writes the held output out before a panic ends the process.
///
/// A release build aborts on a panic, which runs no destructors, so without
/// this the rows printed before the panic would be lost with it.
pub fn flush_on_panic() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        flush();
        previous(info);
    }));
}

/// Standard input that writes the held output out before it waits for more.
///
/// A `BufReader` around it reads from the operating system only once what it
/// already has is used up, which is the one moment the shell can wait for a
/// program that is waiting for this shell's output.
pub struct FlushingStdin;

impl Read for FlushingStdin {
    /// Writes the held output out, then reads standard input.
    ///
    /// @param buffer - where the bytes go
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        flush();
        std::io::stdin().lock().read(buffer)
    }
}

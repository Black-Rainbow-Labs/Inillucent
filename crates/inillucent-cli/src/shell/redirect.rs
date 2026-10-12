//! Where `.output` and `.once` send what the shell prints.
//!
//! Invariant: **a redirection owns what it writes to, and closing it finishes
//! the job.** A file is closed, a pipe has its input closed and its program
//! waited for, so a `.once |sort` has produced all of its output before the next
//! prompt. Standard output is flushed before a program that shares it is
//! started, because the shell holds its own output in a block (see
//! `shell::output`) and the program writes straight to the same stream.

use std::io::Write;

/// One place output can go besides standard output.
pub enum Destination {
    /// A file.
    File(std::fs::File),
    /// Standard error.
    Stderr,
    /// Nowhere: `.output off`.
    Discard,
    /// The input of a program.
    Pipe(std::process::Child),
    /// A buffer the shell keeps, shown by `.output --show`.
    Memory(Vec<u8>),
}

/// A redirection that is in force.
pub struct Redirection {
    /// Where the bytes go.
    pub destination: Destination,
    /// What `.show` calls it: the file name, `stderr`, `off`, `memory`, or the
    /// pipe text.
    pub name: String,
    /// Whether it ends after the next command, which is what `.once` asks for.
    pub once: bool,
    /// Whether the command that set it up is still running. A `.once` is not
    /// consumed by the `.once` that created it.
    pub arming: bool,
}

impl Redirection {
    /// Writes bytes to the destination.
    ///
    /// @param bytes - what to write
    pub fn write(&mut self, bytes: &[u8]) {
        match &mut self.destination {
            Destination::File(file) => {
                let _ = file.write_all(bytes);
            }
            Destination::Stderr => {
                let mut err = std::io::stderr().lock();
                let _ = err.write_all(bytes);
            }
            Destination::Discard => {}
            Destination::Pipe(child) => {
                if let Some(input) = child.stdin.as_mut() {
                    let _ = input.write_all(bytes);
                }
            }
            Destination::Memory(held) => held.extend_from_slice(bytes),
        }
    }

    /// Finishes the redirection: closes the file, or ends the program.
    pub fn close(self) {
        match self.destination {
            Destination::Pipe(mut child) => {
                drop(child.stdin.take());
                super::output::flush();
                let _ = child.wait();
            }
            Destination::File(file) => drop(file),
            Destination::Stderr | Destination::Discard | Destination::Memory(_) => {}
        }
    }
}

/// Starts a program whose input is the redirected output.
///
/// @param command - the command line, without the leading `|`
pub fn start_pipe(command: &str) -> std::io::Result<Destination> {
    super::output::flush();
    let mut builder = if cfg!(windows) {
        let mut builder = std::process::Command::new("cmd");
        builder.arg("/C").arg(command);
        builder
    } else {
        let mut builder = std::process::Command::new("/bin/sh");
        builder.arg("-c").arg(command);
        builder
    };
    let child = builder.stdin(std::process::Stdio::piped()).spawn()?;
    Ok(Destination::Pipe(child))
}

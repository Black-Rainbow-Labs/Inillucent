//! `.output` and `.once`: sending what the shell prints somewhere else.
//!
//! Invariant: **the name picks the destination, and no name is ever taken for a
//! file by accident.** `stdout`, `stderr`, `off` and `memory` are places, a name
//! that begins with `|` is a program that reads the output, and anything else is a
//! file. A word that begins with a dash is an option, so `.output --bom out.txt`
//! writes `out.txt` with a byte order mark and never a file called `--bom`.
//! `.once` is the same command for one command only; see `Shell::end_command`.

use crate::dot::{confine_path, word_error};
use crate::shell::redirect::{start_pipe, Destination};
use crate::shell::Shell;

/// What the words after `.output` or `.once` asked for.
#[derive(Default)]
struct Request<'a> {
    /// `--bom`: write a byte order mark first.
    bom: bool,
    /// `--keep`: do not end the redirection that is in force.
    keep: bool,
    /// `--show`: print what `memory` has kept.
    show: bool,
    /// The destination named, if one was.
    target: Option<&'a str>,
}

/// Reads the words after `.output` or `.once`.
///
/// **The options that start a program are refused by name.** `-e`, `-w` and `-x`
/// open a text editor, a browser and a spreadsheet on the output; this shell does
/// not start a program on its own.
///
/// @param shell - where a complaint goes
/// @param command - `.output` or `.once`
/// @param arguments - the words after the command
fn read_request<'a>(
    shell: &mut Shell,
    command: &str,
    arguments: &[&'a str],
) -> Option<Request<'a>> {
    let mut request = Request::default();
    let mut index = 0usize;
    while let Some(word) = arguments.get(index).copied() {
        index += 1;
        match word {
            "--bom" => request.bom = true,
            "--keep" => request.keep = true,
            "--show" => request.show = true,
            "--plain" => {}
            "--error-prefix" => {
                if arguments.get(index).is_none() {
                    word_error(shell, index, "missing argument");
                    return None;
                }
                index += 1;
            }
            "-e" | "-w" | "-x" => {
                shell.complain(&format!(
                    "Error: {command} {word} is not supported: it starts an editor, a browser or a spreadsheet on the output"
                ));
                return None;
            }
            other if other.starts_with('-') && other.len() > 1 => {
                word_error(shell, index, "unknown option");
                return None;
            }
            other => {
                if request.target.is_some() {
                    word_error(shell, index, "surplus argument");
                    return None;
                }
                request.target = Some(other);
            }
        }
    }
    Some(request)
}

/// Runs `.output` and `.once`.
///
/// @param shell - the shell
/// @param command - `.output` or `.once`
/// @param arguments - the words after the command
/// @param once - whether the redirection ends after the next command
pub fn output(shell: &mut Shell, command: &str, arguments: &[&str], once: bool) {
    let Some(request) = read_request(shell, command, arguments) else {
        return;
    };
    if request.show {
        // The text is taken first, because showing it while the memory
        // redirection is still in force would write it back into the buffer.
        let kept = shell.remembered_output();
        if !request.keep {
            shell.stop_redirection();
        }
        if let Some(text) = kept {
            shell.say(text.trim_end_matches('\n'));
        }
        if request.target.is_none() {
            return;
        }
    }
    let Some(target) = request.target else {
        if !request.keep {
            shell.stop_redirection();
        }
        return;
    };
    let opened = match target {
        "stdout" => {
            shell.stop_redirection();
            return;
        }
        "stderr" => Some(Destination::Stderr),
        "off" => Some(Destination::Discard),
        "memory" => Some(Destination::Memory(Vec::new())),
        piped if piped.starts_with('|') => open_pipe(shell, piped.get(1..).unwrap_or_default()),
        file => open_file(shell, file),
    };
    if let Some(destination) = opened {
        shell.start_redirection(destination, target, once, request.bom);
    }
}

/// Starts the program a `|COMMAND` names.
///
/// @param shell - where a complaint goes
/// @param command - the command line after the bar
fn open_pipe(shell: &mut Shell, command: &str) -> Option<Destination> {
    match start_pipe(command) {
        Ok(destination) => Some(destination),
        Err(_) => {
            shell.complain(&format!("Error: cannot open pipe \"{command}\""));
            None
        }
    }
}

/// Creates the file a name stands for.
///
/// A file that cannot be created is reported twice, as the reference does, and
/// output stays where it was.
///
/// @param shell - where a complaint goes
/// @param name - the file name as typed
fn open_file(shell: &mut Shell, name: &str) -> Option<Destination> {
    let path = confine_path(shell, name)?;
    match std::fs::File::create(&path) {
        Ok(file) => Some(Destination::File(file)),
        Err(_) => {
            shell.complain(&format!("Error: cannot open \"{name}\""));
            shell.complain(&format!("Error: cannot write to \"{name}\""));
            None
        }
    }
}

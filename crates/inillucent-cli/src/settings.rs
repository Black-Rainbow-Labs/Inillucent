//! The dot commands that set one value: `.headers`, `.bail`, `.echo` and the rest.
//!
//! Invariant: **a setting command with the wrong number of arguments prints the
//! reference's usage line and fails**, and a word that is not a boolean is
//! reported and read as "no". `.headers` with no argument used to turn headers on
//! and exit 0, and `.headers maybe` turned them off without saying so; a script
//! that mistyped either got no hint, and the reference gives both.

use crate::shell::Shell;

/// Reads a word as a boolean the way the reference's `booleanValue` does.
///
/// A number is true when it is not zero. `on` and `yes` are true and `off` and
/// `no` are false, in any case. Anything else prints
/// `ERROR: Not a boolean value: "WORD". Assuming "no".` and is false. **`true` and
/// `false` are not accepted**, which is the reference's choice: they are reported
/// like any other word.
///
/// @param shell - where the report of a word that is not a boolean goes
/// @param word - the argument
pub fn boolean(shell: &mut Shell, word: &str) -> bool {
    let digits = word
        .strip_prefix("0x")
        .map(|hex| (hex, 16))
        .unwrap_or((word, 10));
    let (text, radix) = digits;
    if !text.is_empty() && text.chars().all(|character| character.is_digit(radix)) {
        // The value is read as 32 bits, so 4294967296 is zero.
        return u64::from_str_radix(text, radix).map_or(true, |value| value & 0xffff_ffff != 0);
    }
    if word.eq_ignore_ascii_case("on") || word.eq_ignore_ascii_case("yes") {
        return true;
    }
    if word.eq_ignore_ascii_case("off") || word.eq_ignore_ascii_case("no") {
        return false;
    }
    shell.note(&format!(
        "ERROR: Not a boolean value: \"{word}\". Assuming \"no\"."
    ));
    false
}

/// Runs a setting command, and reports whether `name` was one.
///
/// @param shell - the shell
/// @param name - the command without its dot
/// @param arguments - the words after it
pub fn run(shell: &mut Shell, name: &str, arguments: &[&str]) -> bool {
    match name {
        "headers" => with_boolean(shell, ".headers on|off", arguments, |shell, on| {
            shell.headers_chosen = Some(on);
            shell.layout.headers = on;
        }),
        "bail" => with_boolean(shell, ".bail on|off", arguments, |shell, on| {
            shell.bail = on
        }),
        "changes" => with_boolean(shell, ".changes on|off", arguments, |shell, on| {
            shell.show_changes = on
        }),
        "echo" => with_boolean(shell, ".echo on|off", arguments, |shell, on| {
            shell.echo = on
        }),
        "timer" => timer(shell, arguments),
        "eqp" => eqp(shell, arguments),
        "nullvalue" => match arguments {
            [text] => shell.layout.null = (*text).to_string(),
            _ => shell.complain("Usage: .nullvalue STRING"),
        },
        "separator" => separator(shell, arguments),
        "log" => match arguments {
            [target] => shell.log_to = Some((*target).to_string()),
            _ => shell.complain("Usage: .log FILENAME"),
        },
        _ => return false,
    }
    true
}

/// Runs a command that takes exactly one boolean.
///
/// @param shell - the shell
/// @param usage - the usage line, without the word `Usage:`
/// @param arguments - the words after the command
/// @param apply - what to do with the value
fn with_boolean(
    shell: &mut Shell,
    usage: &str,
    arguments: &[&str],
    apply: impl FnOnce(&mut Shell, bool),
) {
    match arguments {
        [word] => {
            let value = boolean(shell, word);
            apply(shell, value);
        }
        _ => shell.complain(&format!("Usage: {usage}")),
    }
}

/// `.timer on|off|once`.
///
/// `once` times the next statement and then turns the timer off.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn timer(shell: &mut Shell, arguments: &[&str]) {
    match arguments {
        [word] if word.eq_ignore_ascii_case("once") => {
            shell.timer = true;
            shell.timer_once = true;
        }
        [word] => {
            shell.timer = boolean(shell, word);
            shell.timer_once = false;
        }
        _ => shell.complain("Usage: .timer on|off|once"),
    }
}

/// `.eqp off|on|trace|trigger|full`.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn eqp(shell: &mut Shell, arguments: &[&str]) {
    let [word] = arguments else {
        shell.complain("Usage: .eqp off|on|trace|trigger|full");
        return;
    };
    let lowered = word.to_ascii_lowercase();
    let level = match lowered.as_str() {
        "trigger" => "trigger",
        "full" => "full",
        "trace" => "on",
        _ => {
            if boolean(shell, word) {
                "on"
            } else {
                "off"
            }
        }
    };
    shell.eqp_name = level;
    shell.explain_plan = level != "off";
}

/// `.separator COL ?ROW?`.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn separator(shell: &mut Shell, arguments: &[&str]) {
    match arguments {
        [column] => shell.layout.separator = (*column).to_string(),
        [column, row] => {
            shell.layout.separator = (*column).to_string();
            shell.layout.row_separator = (*row).to_string();
        }
        _ => shell.complain("Usage: .separator COL ?ROW?"),
    }
}

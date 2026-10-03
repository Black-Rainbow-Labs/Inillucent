//! One process of a multi process run: a worker, a reader, or the checker.
//!
//! Invariant: **a worker prints `ACK <seq>` only after transaction `seq` has
//! committed, and prints nothing else that the parent counts.** The parent
//! kills workers at moments it chooses at random, so the acknowledgements a
//! worker printed before it died are the whole record of what it promised.
//!
//! The workload itself, and every check, is in `inillucent_compat::chaos`, so
//! a test can grade a file in its own process with the same code.
//!
//! Usage:
//!   inillucent-chaos setup  <db> [options]
//!   inillucent-chaos worker <db> --name N --id I --seed S --txns T [options]
//!   inillucent-chaos reader <db> --checks C [--hold-ms H] [options]
//!   inillucent-chaos verify <db> --incarnation name:seed:acked:finished ... [options]
//!
//! Options: --frames F, --key K, --search, --bulk-parts B, --busy-ms M.
//!
//! Exit codes: 0 done, 1 an operation failed, 2 bad arguments, 3 a reader saw
//! a snapshot that broke an invariant.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use inillucent_compat::chaos::{
    attempt, check_snapshot, open, setup, step, verify, Check, Incarnation, Outcome, Settings,
};

/// The parsed command line.
struct Arguments {
    /// `setup`, `worker`, `reader` or `verify`.
    verb: String,
    /// The database file.
    path: PathBuf,
    /// How to open it and what the workload includes.
    settings: Settings,
    /// The worker's name.
    name: String,
    /// The worker's number, used for its search rowids.
    id: i64,
    /// The worker's seed.
    seed: u64,
    /// How many transactions a worker commits, or how many snapshots a reader checks.
    count: i64,
    /// How long a reader keeps each snapshot open.
    hold_ms: u64,
    /// The workers `verify` grades.
    incarnations: Vec<Incarnation>,
}

/// Reads the command line.
///
/// @param words - the arguments after the program name
fn parse(words: &[String]) -> Result<Arguments, String> {
    let verb = words.first().cloned().ok_or("no verb")?;
    let path = PathBuf::from(words.get(1).ok_or("no database path")?);
    let mut arguments = Arguments {
        verb,
        path,
        settings: Settings {
            busy_ms: 30_000,
            ..Settings::default()
        },
        name: "w".to_string(),
        id: 1,
        seed: 1,
        count: 100,
        hold_ms: 0,
        incarnations: Vec::new(),
    };
    let mut index = 2;
    while let Some(flag) = words.get(index) {
        let value = words.get(index + 1).cloned().unwrap_or_default();
        let number = || {
            value
                .parse::<u64>()
                .map_err(|_| format!("{flag} needs a number, not {value:?}"))
        };
        index += 2;
        match flag.as_str() {
            "--frames" => arguments.settings.frames = number()? as usize,
            "--key" => arguments.settings.key = Some(value.clone()),
            "--bulk-parts" => arguments.settings.bulk_parts = number()? as usize,
            "--busy-ms" => arguments.settings.busy_ms = number()?,
            "--name" => arguments.name = value.clone(),
            "--id" => arguments.id = number()? as i64,
            "--seed" => arguments.seed = number()?,
            "--txns" | "--checks" => arguments.count = number()? as i64,
            "--hold-ms" => arguments.hold_ms = number()?,
            "--incarnation" => arguments.incarnations.push(incarnation(&value)?),
            "--search" => {
                arguments.settings.search = true;
                index -= 1;
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    Ok(arguments)
}

/// Reads `name:seed:acked:finished`.
///
/// @param text - the value of one `--incarnation`
fn incarnation(text: &str) -> Result<Incarnation, String> {
    let parts: Vec<&str> = text.split(':').collect();
    match parts.as_slice() {
        [name, seed, acked, finished] => Ok(Incarnation {
            name: name.to_string(),
            seed: seed.parse().map_err(|_| format!("bad seed in {text}"))?,
            acked: acked.parse().map_err(|_| format!("bad acked in {text}"))?,
            finished: *finished == "1",
        }),
        _ => Err(format!(
            "--incarnation wants name:seed:acked:finished, not {text}"
        )),
    }
}

/// Prints a line and flushes it, so a parent reading the pipe sees it before a kill.
///
/// @param line - what to print
fn say(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

/// Runs a worker: commits `count` seeded transactions, retrying any that were
/// refused as busy, and acknowledges each one after it commits.
///
/// @param arguments - the parsed command line
fn worker(arguments: &Arguments) -> ExitCode {
    let database = match open(&arguments.path, &arguments.settings) {
        Ok(database) => database,
        Err(why) => {
            say(&format!("ERROR open {why}"));
            return ExitCode::from(1);
        }
    };
    let connection = database.session();
    let mut busy = 0u64;
    for seq in 1..=arguments.count {
        let planned = step(arguments.seed, seq, &arguments.settings);
        loop {
            match attempt(
                &connection,
                &arguments.name,
                arguments.id,
                &planned,
                &arguments.settings,
            ) {
                Outcome::Committed => break,
                Outcome::Busy => {
                    busy += 1;
                    std::thread::sleep(std::time::Duration::from_millis(2 + busy % 7));
                }
                Outcome::Failed(why) => {
                    say(&format!("ERROR seq {seq}: {why}"));
                    return ExitCode::from(1);
                }
            }
        }
        say(&format!("ACK {seq}"));
        if planned.checkpoint_after {
            match database.checkpoint() {
                Ok(()) => say("CHECKPOINT ok"),
                Err(error) => say(&format!("CHECKPOINT {}", error.status.name())),
            }
        }
    }
    say(&format!("DONE busy={busy}"));
    ExitCode::SUCCESS
}

/// Runs a reader: checks `count` snapshots, each held open for `hold_ms`.
///
/// @param arguments - the parsed command line
fn reader(arguments: &Arguments) -> ExitCode {
    let database = match open(&arguments.path, &arguments.settings) {
        Ok(database) => database,
        Err(why) => {
            say(&format!("ERROR open {why}"));
            return ExitCode::from(1);
        }
    };
    let connection = database.session();
    let hold = std::time::Duration::from_millis(arguments.hold_ms);
    let (mut held, mut busy, mut seen) = (0i64, 0u64, 0i64);
    while held < arguments.count {
        match check_snapshot(&connection, &arguments.settings, hold, held) {
            Check::Held(transactions) => {
                held += 1;
                seen = seen.max(transactions);
                say(&format!("CHECKED {held} seen={transactions}"));
            }
            Check::Busy(why) => {
                busy += 1;
                // Said now and then, so a parent can tell a reader that is
                // being refused from one that has stopped.
                if busy % 50 == 1 {
                    say(&format!("BUSY {busy} {why}"));
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Check::Broken(why) => {
                say(&format!("VIOLATION {why}"));
                return ExitCode::from(3);
            }
        }
    }
    say(&format!("DONE checks={held} busy={busy} seen={seen}"));
    ExitCode::SUCCESS
}

/// Runs the chosen verb.
fn main() -> ExitCode {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let arguments = match parse(&words) {
        Ok(arguments) => arguments,
        Err(why) => {
            eprintln!("inillucent-chaos: {why}");
            return ExitCode::from(2);
        }
    };
    match arguments.verb.as_str() {
        "setup" => match setup(&arguments.path, &arguments.settings) {
            Ok(()) => ExitCode::SUCCESS,
            Err(why) => {
                say(&format!("ERROR setup {why}"));
                ExitCode::from(1)
            }
        },
        "worker" => worker(&arguments),
        "reader" => reader(&arguments),
        "verify" => match verify(
            &arguments.path,
            &arguments.settings,
            &arguments.incarnations,
        ) {
            Ok(summary) => {
                say(&format!("VERIFIED {summary}"));
                ExitCode::SUCCESS
            }
            Err(why) => {
                say(&format!("VIOLATION {why}"));
                ExitCode::from(3)
            }
        },
        other => {
            eprintln!("inillucent-chaos: unknown verb {other}");
            ExitCode::from(2)
        }
    }
}

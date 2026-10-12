//! A SQLite-like shell: `inillucent-shell`.
//!
//! Invariant: the shell is an adapter. It parses dot commands and formats
//! output; every statement it runs goes through the public `inillucent` facade, and
//! it never reaches past it. A shell that starts reading the schema directly
//! becomes a second, slightly different database, and the difference is only
//! ever found by somebody who trusted it.
//!
//! The command line is SQLite's, because a script that drives `sqlite3` has to
//! drive this: `inillucent-shell FILE "SELECT ..."` runs one statement and exits,
//! `inillucent-shell FILE` reads standard input, and the option flags before the
//! file name set the same things the dot commands do.
//!
//! The shell itself lives in the crate's library, because the
//! `inillucent` and `inillucent-mcp` binaries drive the same one. This file is
//! what is left: the `sqlite3`-shaped command line, and nothing else.

#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(clippy::indexing_slicing)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::panic)]
#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::panic,
        clippy::unwrap_used
    )
)]

/// The engine's own allocator, installed for this program.
///
/// **Part of the build, not of a workload.** SQLite ships its own memory
/// subsystem and is compiled as one translation unit; a Rust workspace measured
/// on the platform allocator is being measured on a build configuration rather
/// than on an engine, which is the same reasoning that fixed fat LTO and one
/// codegen unit in the release profile. The Windows C runtime heap was
/// measured at 59% of a trivial compile, and this size-classed free list at
/// 17% overall, which is why Phase 3's Part E names it the cheapest first move.
#[global_allocator]
static ALLOCATOR: inillucent_alloc::Carved = inillucent_alloc::Carved;

use inillucent_cli::shell::{drive, Shell};
use inillucent_cli::{commands, dot};

/// One thing the command line asks the shell to do before it reads input.
///
/// **In the order the words were written.** The reference runs each `-cmd` and each
/// option the moment it reaches it, so `-csv -cmd ".mode box"` ends in `box` and
/// `-cmd ".mode box" -csv` ends in `csv`.
enum Step {
    /// A dot command the shell builds for an option, such as `.mode csv`.
    Dot(String),
    /// The text of `-cmd`: a dot command or SQL, with the position it was given at.
    Command {
        /// What the person wrote.
        text: String,
        /// Where it was on the command line, counted from the program name.
        index: usize,
    },
    /// `-separator`, `-newline` or `-nullvalue`, whose text is used as written.
    ///
    /// The dot commands resolve a backslash escape and the options do not:
    /// `-separator '\t'` is a backslash and a `t`.
    Setting(Setting),
}

/// A setting an option gives literally.
enum Setting {
    /// What goes between columns.
    ColumnSeparator(String),
    /// What goes after a row.
    RowSeparator(String),
    /// What a NULL prints as.
    NullValue(String),
}

/// What the command line asked for.
struct Invocation {
    /// How the program was started, which an error names.
    program: String,
    /// The database to open.
    path: String,
    /// The database as it was typed, which an error names.
    typed: String,
    /// Whether the file has to be there already, from `mode=ro` or `mode=rw` in a URI.
    must_exist: bool,
    /// Statements and dot commands given as arguments, with their positions.
    statements: Vec<(usize, String)>,
    /// Settings and `-cmd`s to apply before anything else runs.
    steps: Vec<Step>,
    /// The file `-init` named.
    init: Option<String>,
    /// Whether `-noinit` was given, which cancels `-init` too.
    no_init: bool,
    /// Whether to print the version and stop.
    version: bool,
    /// Whether to print usage and stop.
    help: bool,
    /// Whether to stop at the first failing command.
    bail: bool,
    /// Whether every statement that changes something is refused.
    readonly: bool,
    /// Whether the commands that reach outside the database are refused.
    safe: bool,
    /// Whether the file has to exist already.
    if_exists: bool,
    /// Whether a symbolic link is refused rather than followed.
    no_follow: bool,
    /// The options this engine has no equivalent for, with the reason for each.
    refused: Vec<(String, &'static str)>,
    /// The words that are shaped like an option and are not one of them.
    unknown: Vec<String>,
    /// An option that needs a value and was the last word on the line.
    missing: Option<String>,
    /// The file `-key-file` named, holding the key databases are opened with.
    key_file: Option<String>,
}

/// The options that name a SQLite internal this engine does not have.
///
/// **Refused by name rather than ignored, and that is the decision.** A shell
/// that swallows `-mmap 268435456` and carries on has told its caller the
/// setting took effect. Every one of these names a mechanism the reference
/// shell exposes because SQLite has it - a lookaside allocator, a page cache
/// handed a caller's buffer, a memory-mapped read path, a VFS shim, the
/// serialize/deserialize interface, the appendvfs, the ZIP reader. This engine
/// has none of them, and the driver already carries a whole status for exactly
/// this distinction - `unsupported`, which `drivers/README.md` argues at length
/// must not be folded into "you mistyped something". Saying so out loud is the
/// same answer in a different medium.
///
/// The second field says whether the option consumes the word after it, so a
/// value is not then read as the file name.
const REFUSED: &[(&str, bool, &str)] = &[
    (
        "-append",
        false,
        "this engine does not write into the end of another file; there is no appendvfs.",
    ),
    (
        "-deserialize",
        false,
        "there is no sqlite3_deserialize: a database is a file, or it is :memory:.",
    ),
    (
        "-maxsize",
        true,
        "it sizes a deserialized database, and there are none.",
    ),
    ("-zip", false, "there is no ZIP virtual file system."),
    (
        "-vfs",
        true,
        "the VFS is not selectable at run time. .vfslist names the one there is.",
    ),
    (
        "-vfstrace",
        false,
        "there is no VFS shim layer to trace through.",
    ),
    (
        "-lookaside",
        true,
        "there is no lookaside allocator to budget. The allocator is inillucent-alloc, a \
         size-classed free list, and it takes no per-connection reservation.",
    ),
    (
        "-pagecache",
        true,
        "the buffer pool is not handed a caller's buffer. PRAGMA cache_size sizes it.",
    ),
    (
        "-pcachetrace",
        false,
        "there is no pluggable page cache to trace.",
    ),
    ("-memtrace", false, "there is no memory tracer to install."),
    (
        "-mmap",
        true,
        "the pool reads through the VFS. There is no memory-mapped read path to size.",
    ),
    (
        "-unsafe-testing",
        false,
        "there is no testing back door to open.",
    ),
    (
        "-no-rowid-in-view",
        false,
        "this engine has no legacy rowid-in-view behaviour to switch off.",
    ),
    (
        "-interactive",
        false,
        "this shell never prompts, so there is nothing to force on. It is always -batch.",
    ),
];

/// The options that take one word after them, which is never the file name.
const TAKES_A_VALUE: &[&str] = &[
    "-cmd",
    "-init",
    "-separator",
    "-nullvalue",
    "-newline",
    "-nonce",
    "-key-file",
    "-escape",
    "-screenwidth",
    "-heap",
];

/// The `-mode` style options and the dot command each one is.
const MODE_OPTIONS: &[(&str, &str)] = &[
    ("-ascii", ".mode ascii"),
    ("-box", ".mode box"),
    ("-column", ".mode column"),
    ("-csv", ".mode csv"),
    ("-html", ".mode html"),
    ("-json", ".mode json"),
    ("-line", ".mode line"),
    ("-list", ".mode list"),
    ("-markdown", ".mode markdown"),
    ("-quote", ".mode quote"),
    ("-table", ".mode table"),
    ("-tabs", ".mode tabs"),
];

/// Parses the command line the way `sqlite3` does.
///
/// Options come first, then the file, then any statements. An unrecognised word
/// that begins with a dash is an error, which is what `sqlite3` itself does:
/// version 3.53.4 answers `sqlite3 --db app.db "SELECT 1"` with `Error: unknown
/// option: -db`, opens nothing and exits non-zero. This shell used to take such
/// a word as the file name instead, so a mistyped option created a database
/// called `--db` in whatever directory the caller was standing in, and files
/// named `--`, `-d` and `--db` turned up in a repository from exactly that.
/// `--` still ends the options, so a file whose name begins with a dash can be
/// opened by writing it after one.
///
/// @param arguments - the words after the program name
fn parse(arguments: impl Iterator<Item = String>) -> Invocation {
    let mut invocation = Invocation {
        program: String::new(),
        path: ":memory:".to_string(),
        typed: ":memory:".to_string(),
        must_exist: false,
        statements: Vec::new(),
        steps: Vec::new(),
        init: None,
        no_init: false,
        version: false,
        help: false,
        bail: false,
        readonly: false,
        safe: false,
        if_exists: false,
        no_follow: false,
        refused: Vec::new(),
        unknown: Vec::new(),
        missing: None,
        key_file: None,
    };
    let mut named = false;
    let mut only_positional = false;
    let mut arguments = arguments.enumerate().map(|(at, word)| (at + 1, word));
    while let Some((index, argument)) = arguments.next() {
        if only_positional {
            take_positional(&mut invocation, index, argument, &mut named);
            continue;
        }
        if !argument.starts_with('-') || argument == "-" {
            take_positional(&mut invocation, index, argument, &mut named);
            continue;
        }
        // The reference reads `--name` as `-name`.
        let option = argument
            .strip_prefix("--")
            .map_or(argument.clone(), |rest| format!("-{rest}"));
        if option == "-" {
            only_positional = true;
            continue;
        }
        if let Some((name, takes_value, why)) = REFUSED.iter().find(|(name, _, _)| *name == option)
        {
            if *takes_value {
                let _ = arguments.next();
            }
            invocation.refused.push(((*name).to_string(), *why));
            continue;
        }
        if TAKES_A_VALUE.contains(&option.as_str()) {
            let Some((value_index, value)) = arguments.next() else {
                invocation.missing = Some(option);
                break;
            };
            apply_value_option(&mut invocation, &option, value_index, value);
            continue;
        }
        if let Some((_, dot)) = MODE_OPTIONS.iter().find(|(name, _)| *name == option) {
            invocation.steps.push(Step::Dot((*dot).to_string()));
            continue;
        }
        match option.as_str() {
            "-version" => invocation.version = true,
            "-help" => invocation.help = true,
            "-header" => invocation.steps.push(Step::Dot(".headers on".to_string())),
            "-noheader" => invocation.steps.push(Step::Dot(".headers off".to_string())),
            "-echo" => invocation.steps.push(Step::Dot(".echo on".to_string())),
            "-eqp" => invocation.steps.push(Step::Dot(".eqp on".to_string())),
            "-bail" => invocation.bail = true,
            "-stats" => invocation.steps.push(Step::Dot(".stats on".to_string())),
            "-batch" => {}
            "-noinit" => invocation.no_init = true,
            "-readonly" => invocation.readonly = true,
            "-safe" => invocation.safe = true,
            "-ifexists" => invocation.if_exists = true,
            "-nofollow" => invocation.no_follow = true,
            // Reached only when nothing matched, so a word still carrying a
            // leading dash is a mistyped option and not a file name.
            _ => invocation.unknown.push(option),
        }
    }
    invocation
}

/// Applies an option that carries a value.
///
/// @param invocation - what is being built
/// @param option - the option, with one leading dash
/// @param index - where the value was on the command line
/// @param value - the word after the option
fn apply_value_option(invocation: &mut Invocation, option: &str, index: usize, value: String) {
    match option {
        "-cmd" => invocation.steps.push(Step::Command { text: value, index }),
        "-init" => invocation.init = Some(value),
        "-separator" => invocation
            .steps
            .push(Step::Setting(Setting::ColumnSeparator(value))),
        "-newline" => invocation
            .steps
            .push(Step::Setting(Setting::RowSeparator(value))),
        "-nullvalue" => invocation
            .steps
            .push(Step::Setting(Setting::NullValue(value))),
        "-nonce" => invocation.steps.push(Step::Dot(format!(".nonce {value}"))),
        "-key-file" => invocation.key_file = Some(value),
        // `-escape` and `-screenwidth` set a mode option each.
        "-escape" => invocation
            .steps
            .push(Step::Dot(format!(".mode --escape {value}"))),
        "-screenwidth" => invocation
            .steps
            .push(Step::Dot(format!(".mode --screenwidth {value}"))),
        // `-heap` sizes SQLite's own allocator, which this engine does not have;
        // the reference accepts it and so does this.
        _ => {}
    }
}

/// Records a word that is not an option: the first is the file, the rest SQL.
///
/// @param invocation - what is being built
/// @param index - where the word was on the command line
/// @param argument - the word
/// @param named - whether the file has already been named
fn take_positional(invocation: &mut Invocation, index: usize, argument: String, named: &mut bool) {
    if *named {
        invocation.statements.push((index, argument));
    } else {
        invocation.typed = argument.clone();
        invocation.path = argument;
        *named = true;
    }
}

/// Refuses to open a file that does not satisfy `-ifexists`, `-nofollow` or `-readonly`.
///
/// A read only open of a file that is not there fails and creates nothing, as
/// the reference's does: opening a missing file read only must not leave an
/// empty database behind.
///
/// @param invocation - what the command line asked for
fn openable(invocation: &Invocation) -> Result<(), String> {
    if invocation.path == ":memory:" {
        return Ok(());
    }
    let path = std::path::Path::new(&invocation.path);
    let refused = format!(
        "Error: unable to open database \"{}\": unable to open database file",
        invocation.typed
    );
    if (invocation.if_exists || invocation.readonly || invocation.must_exist) && !path.exists() {
        return Err(refused);
    }
    // Asked of the link itself rather than of what it points at, which is the
    // whole of the check: `exists()` follows a link, so a check written that way
    // would answer about the target every time and never refuse anything.
    if invocation.no_follow {
        if let Ok(about) = std::fs::symlink_metadata(path) {
            if about.file_type().is_symlink() {
                return Err(refused);
            }
        }
    }
    Ok(())
}

/// Reads a `file:` name given on the command line as the URI it is.
///
/// `file:app.db?mode=ro` opens `app.db` read only, and has to find it there.
/// `mode=memory` is a database in memory. A name that is not a URI is left alone.
///
/// @param invocation - what the command line asked for
fn resolve_uri(invocation: &mut Invocation) {
    match inillucent_cli::opening::parse_uri(&invocation.path) {
        Some(Ok(uri)) => {
            invocation.readonly |= uri.read_only;
            invocation.must_exist |= uri.must_exist;
            invocation.path = if uri.path.is_empty() {
                ":memory:".to_string()
            } else {
                uri.path
            };
        }
        Some(Err(message)) => {
            eprintln!(
                "Error: unable to open database \"{}\": {message}",
                invocation.typed
            );
            std::process::exit(1);
        }
        None => {}
    }
}

/// Opens the database, applies the settings, and runs whatever was asked for.
fn main() {
    // See `inillucent_cli::STATEMENT_STACK`.
    inillucent_cli::on_a_sized_stack(run)
}

/// Prints an error line the way the reference prints a command line mistake.
///
/// @param program - how the program was started
/// @param message - what was wrong
fn command_line_error(program: &str, message: &str) -> ! {
    eprintln!("{program}: Error: {message}");
    std::process::exit(1)
}

/// Everything `main` does, on the sized thread.
fn run() {
    inillucent_cli::shell::output::flush_on_panic();
    let mut words = std::env::args_os().map(|word| word.to_string_lossy().into_owned());
    let program = words
        .next()
        .unwrap_or_else(|| "inillucent-shell".to_string());
    let mut invocation = parse(words);
    invocation.program = program;
    if let Some(option) = &invocation.missing {
        command_line_error(
            &invocation.program,
            &format!("missing argument to {option}"),
        );
    }
    if invocation.help {
        usage(&invocation.program);
        return;
    }
    // Before anything is opened, and before the refusals below: a word that is
    // shaped like an option and is not one means the rest of the line was
    // probably misread too. The file name is the word after it, so carrying on
    // would create a database under the typo's name and run the real file name
    // as a statement.
    if let Some(option) = invocation.unknown.first() {
        eprintln!("{}: Error: unknown option: {option}", invocation.program);
        eprintln!("Use -help for a list of options.");
        std::process::exit(1);
    }
    // Before anything is opened: an option nobody can honour means the command
    // line was misunderstood, and running most of it would be worse than
    // running none of it.
    if !invocation.refused.is_empty() {
        for (option, why) in &invocation.refused {
            eprintln!("Error: {option} is not supported by this engine.");
            eprintln!("  {why}");
        }
        std::process::exit(1);
    }
    resolve_uri(&mut invocation);
    if let Err(message) = openable(&invocation) {
        eprintln!("{message}");
        std::process::exit(1);
    }
    // Before the database is opened, so it is opened with the key. See
    // `inillucent_cli::keys`.
    if let Err(message) = inillucent_cli::keys::install_from(invocation.key_file.as_deref()) {
        eprintln!("Error: {message}");
        std::process::exit(1);
    }
    let mut shell = match Shell::open_following(&invocation.path, invocation.readonly) {
        Ok(shell) => shell,
        Err(message) => {
            eprintln!(
                "Error: unable to open database \"{}\": {message}",
                invocation.path
            );
            std::process::exit(1);
        }
    };
    // Ctrl+C ends the statement, not the shell (task-1932, H11). A second
    // press still ends the process: the operating system's default handler is
    // back once ours has fired.
    inillucent_cli::interrupt::stop_on_ctrl_c(shell.cancel_flag());
    if invocation.version {
        dot::run(&mut shell, ".version");
        inillucent_cli::shell::output::flush();
        return;
    }
    shell.bail = invocation.bail;
    let code = match run_setup(&mut shell, &invocation) {
        Some(code) => code,
        None => {
            shell.readonly = invocation.readonly;
            shell.safe = invocation.safe;
            run_input(&mut shell, &invocation)
        }
    };
    // `.testcase`/`.check` report their tally once, at the end, and only when
    // some ran - which is what makes the line invisible to an ordinary script.
    commands::report_tests(&mut shell);
    // The shell is dropped before the process ends, because the drop runs the
    // statements it is still holding and closes the database.
    drop(shell);
    inillucent_cli::shell::output::flush();
    if code != 0 {
        std::process::exit(code);
    }
}

/// Runs the start up file, then each option and `-cmd` in the order given.
///
/// The settings run before safe mode is armed, because `-init` names a script
/// the caller chose and `-safe` is about what that script may then do. Arming
/// first would refuse the caller's own `.read`.
///
/// **A failure here does not decide the exit status.** The reference reports the
/// error and goes on, and the status is that of the input read afterwards. With
/// `-bail`, the first failure ends the run with status 1 instead.
///
/// @param shell - the shell
/// @param invocation - what the command line asked for
fn run_setup(shell: &mut Shell, invocation: &Invocation) -> Option<i32> {
    if let (Some(file), false) = (&invocation.init, invocation.no_init) {
        run_init_file(shell, file);
        if let Some(code) = settle(shell, invocation.bail) {
            return Some(code);
        }
    }
    for step in &invocation.steps {
        match step {
            Step::Dot(text) => drive(shell, std::iter::once(text.clone())),
            Step::Command { text, index } => {
                shell.argument = Some(*index);
                drive(shell, std::iter::once(text.clone()));
                shell.argument = None;
            }
            Step::Setting(setting) => apply_setting(shell, setting),
        }
        if let Some(code) = settle(shell, invocation.bail) {
            return Some(code);
        }
    }
    None
}

/// Runs the file `-init` named, or says it could not be opened.
///
/// @param shell - the shell
/// @param file - the name as given
fn run_init_file(shell: &mut Shell, file: &str) {
    match std::fs::read(file) {
        Ok(bytes) => dot::run_script_bytes(shell, file, &bytes),
        Err(_) => eprintln!("cannot open: \"{file}\""),
    }
}

/// Applies a setting given as an option, without resolving escapes.
///
/// @param shell - the shell
/// @param setting - which setting and its text
fn apply_setting(shell: &mut Shell, setting: &Setting) {
    match setting {
        Setting::ColumnSeparator(text) => shell.layout.separator = text.clone(),
        Setting::RowSeparator(text) => shell.layout.row_separator = text.clone(),
        Setting::NullValue(text) => shell.layout.null = text.clone(),
    }
}

/// Decides what a finished step means for the run.
///
/// `.exit` ends the process with its status. `.quit` ends it with 0, without
/// reading any input. A failure with `-bail` ends it with 1. Otherwise the
/// failure is forgotten, because only the input read last sets the status.
/// Returns the status to end with, or none to go on.
///
/// @param shell - the shell
/// @param bail - whether `-bail` was given
fn settle(shell: &mut Shell, bail: bool) -> Option<i32> {
    if let Some(code) = shell.exit_code {
        return Some(code);
    }
    if shell.done {
        return Some(0);
    }
    if shell.failed && bail {
        return Some(1);
    }
    shell.failed = false;
    None
}

/// Runs the statements given as arguments, or reads standard input.
///
/// **Each argument is run on its own and the first failure ends the run.** An
/// argument is a statement or a dot command. A failing one is reported as
/// `in 3rd command line argument`, nothing after it runs, and the status is 1.
/// When any argument was given, standard input is not read. Returns the exit status.
///
/// @param shell - the shell
/// @param invocation - what the command line asked for
fn run_input(shell: &mut Shell, invocation: &Invocation) -> i32 {
    if invocation.statements.is_empty() {
        // Through `FlushingStdin`, so the answers printed so far are written
        // out before the shell waits for more input; see `shell::output`.
        let input = std::io::BufReader::with_capacity(
            64 << 10,
            inillucent_cli::shell::output::FlushingStdin,
        );
        // Bytes, not text: a line that is not UTF-8 is a line (see `raw`).
        let lines = inillucent_cli::raw::LineReader::new(input);
        drive(shell, lines);
    } else {
        for (index, text) in &invocation.statements {
            shell.argument = Some(*index);
            drive(shell, std::iter::once(text.clone()));
            shell.argument = None;
            if shell.exit_code.is_some() || shell.done || shell.failed {
                break;
            }
        }
    }
    match shell.exit_code {
        Some(code) => code,
        None if shell.failed => 1,
        None => 0,
    }
}

/// Prints what the command line accepts, to standard error as the reference does.
///
/// Every option the reference shell has appears here, including the ones this
/// engine refuses: an option missing from the usage text reads as an option that
/// was forgotten, and a caller who reads "not supported, and here is why" has
/// learned something a silent omission would not have told them.
///
/// @param program - how the program was started
fn usage(program: &str) {
    eprintln!("Usage: {program} [OPTIONS] [FILENAME [SQL...]]");
    eprintln!("FILENAME is the name of an SQLite database. A new database is created");
    eprintln!("if the file does not previously exist. Defaults to :memory:.");
    eprintln!("OPTIONS include:");
    for line in [
        "   --                   treat no subsequent arguments as options",
        "   -ascii               set output mode to 'ascii'",
        "   -bail                stop after hitting an error",
        "   -batch               force batch input (this shell is always batch)",
        "   -box                 set output mode to 'box'",
        "   -column              set output mode to 'column'",
        "   -cmd COMMAND         run \"COMMAND\" before reading stdin",
        "   -csv                 set output mode to 'csv'",
        "   -echo                print inputs before execution",
        "   -header              turn headers on",
        "   -noheader            turn headers off",
        "   -escape T            ctrl-char escape; T is one of: symbol, ascii, off",
        "   -screenwidth N       use N as the default screenwidth",
        "   -help                show this message",
        "   -html                set output mode to HTML",
        "   -ifexists            only open if database already exists",
        "   -init FILENAME       read/process named file",
        "   -json                set output mode to 'json'",
        "   -line                set output mode to 'line'",
        "   -list                set output mode to 'list'",
        "   -markdown            set output mode to 'markdown'",
        "   -newline SEP         set output row separator. Default: '\\n'",
        "   -nofollow            refuse to open symbolic links to database files",
        "   -noinit              Do not read the ~/.sqliterc file at startup",
        "   -nonce STRING        set the safe-mode escape nonce",
        "   -nullvalue TEXT      set text string for NULL values. Default ''",
        "   -quote               set output mode to 'quote'",
        "   -readonly            open the database read-only",
        "   -key-file FILE       open databases encrypted, with the key in FILE",
        "                        (or $INILLUCENT_KEY)",
        "   -safe                enable safe-mode",
        "   -separator SEP       set output column separator. Default: '|'",
        "   -stats               print memory stats before each finalize",
        "   -table               set output mode to 'table'",
        "   -tabs                set output mode to 'tabs'",
        "   -version             show SQLite version",
    ] {
        eprintln!("{line}");
    }
    eprintln!();
    eprintln!("Refused, because this engine has no equivalent - each says why when used:");
    let width = REFUSED
        .iter()
        .map(|(option, _, _)| option.len())
        .max()
        .unwrap_or(0);
    for (option, takes_value, _) in REFUSED {
        let value = if *takes_value { " N" } else { "" };
        let padding = " ".repeat(width.saturating_sub(option.len()));
        eprintln!("   {option}{padding}{value}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a list of words as a command line.
    fn parsed(words: &[&str]) -> Invocation {
        parse(words.iter().map(|word| (*word).to_string()))
    }

    /// The first bare word is the file and the rest are statements.
    #[test]
    fn the_first_word_is_the_file() {
        let parsed = parsed(&["db.sqlite", "SELECT 1", "SELECT 2"]);
        assert_eq!(parsed.path, "db.sqlite");
        let statements: Vec<(usize, &str)> = parsed
            .statements
            .iter()
            .map(|(at, text)| (*at, text.as_str()))
            .collect();
        assert_eq!(statements, vec![(2, "SELECT 1"), (3, "SELECT 2")]);
    }

    /// An option before the file sets a dot command.
    #[test]
    fn options_become_dot_commands() {
        let parsed = parsed(&["-csv", "-header", "db.sqlite"]);
        let dots: Vec<&str> = parsed
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Dot(text) => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(dots, vec![".mode csv", ".headers on"]);
        assert_eq!(parsed.path, "db.sqlite");
    }

    /// With no file at all, the database is in memory.
    #[test]
    fn no_file_means_memory() {
        let parsed = parsed(&[]);
        assert_eq!(parsed.path, ":memory:");
    }

    /// A word shaped like an option and matching none of them is not the file.
    #[test]
    fn an_unknown_option_is_not_the_file_name() {
        let parsed = parsed(&["--db", "app.rdb", "SELECT 1"]);
        assert_eq!(parsed.unknown, vec!["-db"]);
        assert_eq!(parsed.path, "app.rdb");
        assert_eq!(parsed.statements.len(), 1);
    }

    /// The short spellings that turned up on disk are caught the same way.
    #[test]
    fn every_dashed_word_that_is_not_an_option_is_reported() {
        let parsed = parsed(&["-d", "app.rdb", "--nonsense"]);
        assert_eq!(parsed.unknown, vec!["-d", "-nonsense"]);
    }

    /// After `--`, a name that begins with a dash is a file again.
    #[test]
    fn the_separator_still_opens_a_dashed_file() {
        let parsed = parsed(&["--", "-weird.rdb"]);
        assert!(parsed.unknown.is_empty());
        assert_eq!(parsed.path, "-weird.rdb");
    }

    /// A refused option is still a refusal rather than an unknown word.
    #[test]
    fn a_refused_option_is_not_reported_as_unknown() {
        let parsed = parsed(&["-mmap", "268435456", "app.rdb"]);
        assert!(parsed.unknown.is_empty());
        assert_eq!(parsed.refused.len(), 1);
        assert_eq!(parsed.path, "app.rdb");
    }

    /// An option that takes a value consumes the next word, and its text is kept
    /// as written.
    #[test]
    fn an_option_can_take_a_value() {
        let parsed = parsed(&["-separator", "\\t", "db.sqlite"]);
        assert!(matches!(
            parsed.steps.first(),
            Some(Step::Setting(Setting::ColumnSeparator(text))) if text == "\\t"
        ));
        assert_eq!(parsed.path, "db.sqlite");
    }

    /// A `-cmd` remembers which word of the command line it was.
    #[test]
    fn a_command_knows_its_position() {
        let parsed = parsed(&["-bail", "-cmd", "select 1", "db.sqlite"]);
        assert!(parsed.bail);
        assert!(matches!(
            parsed.steps.first(),
            Some(Step::Command { index: 3, .. })
        ));
    }

    /// An option that needs a value and is last on the line is reported.
    #[test]
    fn a_missing_value_is_reported() {
        assert_eq!(parsed(&["-cmd"]).missing.as_deref(), Some("-cmd"));
    }
}

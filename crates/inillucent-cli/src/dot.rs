//! The dot commands: everything a person types that is not SQL.
//!
//! Invariant: a dot command is sugar over SQL wherever it can be. `.tables`
//! queries `sqlite_master`, `.schema` reads the stored text, `.indexes` asks
//! `PRAGMA index_list` - so a dot command cannot disagree with the query a
//! person would have written instead, and there is no second implementation of
//! the catalog living in the shell. The commands that are *not* sugar - the
//! ones that manage files, output and settings - are the ones that could not
//! be, and they are the short list at the top of `run`.
//!
//! Arguments are split the way a shell splits them, with quotes honoured,
//! because `.output "my file.txt"` has to work and `.separator " | "` has to be
//! able to hold spaces.

use crate::render::{literal, Layout};
use crate::shell::{drive, Shell};
use inillucent_value::Value;

/// Refuses a path outside `--root`, and returns it when there is no root.
///
/// **The dot commands were the one way out of `--root` (task-1979, H1).** A
/// path a statement names goes through the confined VFS, which resolves it
/// through the file system and refuses what lands outside; `.output`, `.once`,
/// `.read` and `.import` open their path with plain `std::fs`, so none of that
/// applied to them. On an MCP server started `--root DIR` an agent could read
/// and write any file on the host.
///
/// The decision is `inillucent_vfs::confine`'s, which is the same one the VFS
/// makes, so a junction inside the root pointing out is refused here exactly as
/// it is there. This exists so that every dot command that opens a path by name
/// asks, and so that the next one somebody adds has an obvious thing to call.
///
/// @param shell - the shell, which is told when the path is refused
/// @param path - the path the caller typed
pub(crate) fn confine_path(shell: &mut Shell, path: &str) -> Option<String> {
    let Some(root) = inillucent_driver::vfs::confine::process_root() else {
        return Some(path.to_string());
    };
    match root.admit(path) {
        Ok(resolved) => Some(resolved.to_string_lossy().into_owned()),
        Err(refused) => {
            shell.complain(&format!("Error: {}", refused.message()));
            None
        }
    }
}

/// The dot commands safe mode refuses, measured against the pinned reference.
///
/// **Six of these were reachable over MCP and are the reason this list is
/// central rather than a check inside each command (task-1979, section 5.3).**
/// Safe mode was asked about in four places - `.cd`, `.system` and `.shell`,
/// `.excel` and `.www`, and `.load` - so a server that had turned it on still
/// let a caller write a file with `.output`, read one with `.read`, and copy
/// the database somewhere with `.backup`. Measured: `.output out.txt` through
/// `inillucent-mcp` created `out.txt` in the server's working directory and
/// reported no error.
///
/// The set is the reference's own, read off `sqlite3 -safe` at 3.53.4, which
/// answers `cannot run .output in safe mode` for every name here. `.clone` is
/// on it because this shell's `.clone` is its `.backup` under another name -
/// see the dispatch arm - and the reference has no `.clone` to ask.
const REFUSED_IN_SAFE_MODE: &[&str] = &[
    "ar", "archive", "backup", "cd", "clone", "excel", "import", "load", "once", "output", "read",
    "restore", "save", "shell", "system", "www",
];

/// Refuses a dot command that safe mode does not allow.
///
/// Returns whether the command was refused, in which case it must not run.
///
/// `.nonce` has already cleared safe mode by the time the command it covers
/// reaches here, which is what `.nonce` is for.
///
/// @param shell - the shell
/// @param name - the command's name, without its leading dot
fn refused_by_safe_mode(shell: &mut Shell, name: &str) -> bool {
    if !REFUSED_IN_SAFE_MODE.contains(&name) {
        return false;
    }
    shell.unsafe_refused(&format!(".{name}"))
}

/// Runs one dot command.
pub fn run(shell: &mut Shell, line: &str) {
    // What the shell is holding runs first, because a command can read or
    // change what it would have written (task-2191). A held statement that
    // failed under `.bail` stops the command, as it would have stopped the
    // input before the command was read.
    let failed_before = shell.failed;
    shell.run_held();
    if shell.bail && shell.failed && !failed_before {
        return;
    }
    let parsed = split(without_terminator(line));
    let Some(name) = parsed
        .first()
        .map(|(word, _)| word.trim_start_matches('.').to_string())
    else {
        return;
    };
    shell.dot_line = without_terminator(line).to_string();
    shell.dot_offsets = parsed.iter().map(|(_, at)| *at).collect();
    let arguments: Vec<&str> = parsed
        .iter()
        .skip(1)
        .map(|(word, _)| word.as_str())
        .collect();
    if refused_by_safe_mode(shell, &name) {
        return;
    }
    match name.as_str() {
        "headers" => settings(shell, &name, &arguments),
        "bail" => settings(shell, &name, &arguments),
        "changes" => settings(shell, &name, &arguments),
        "echo" => settings(shell, &name, &arguments),
        "timer" => settings(shell, &name, &arguments),
        "eqp" => settings(shell, &name, &arguments),
        "nullvalue" => settings(shell, &name, &arguments),
        "separator" => settings(shell, &name, &arguments),
        "log" => settings(shell, &name, &arguments),
        "quit" => shell.done = true,
        "exit" => {
            // **`.exit` ends the process with the status it was given, from any
            // depth** (a `.read` file, a `-cmd` or a statement argument). The
            // status is read the way C's `atoi` reads it: a leading number, and
            // zero when there is none, so `.exit abc` is 0 and `.exit 3x` is 3.
            // A second argument is ignored.
            shell.exit_code = Some(arguments.first().map_or(0, |word| c_atoi(word)));
            shell.done = true;
        }
        "help" => help(shell, &arguments),
        "open" => crate::opening::open(shell, &arguments),
        "databases" => databases(shell),
        "tables" => tables(shell, &arguments),
        "indexes" | "indices" => indexes(shell, &arguments),
        "schema" => schema(shell, &arguments),
        "fullschema" => full_schema(shell),
        "mode" => crate::modes::mode(shell, &arguments),
        "width" => width(shell, &arguments),
        "output" => crate::redirecting::output(shell, ".output", &arguments, false),
        "once" => crate::redirecting::output(shell, ".once", &arguments, true),
        "print" => {
            let text = arguments.join(" ");
            shell.say(&text);
        }
        "archive" | "ar" => crate::archive::archive(shell, &arguments),
        "stats" => crate::diagnose::stats(shell, &arguments),
        "vfslist" => crate::diagnose::vfs_list(shell),
        "vfsinfo" | "vfsname" => crate::diagnose::vfs_info(shell, name == "vfsname"),
        "read" => read(shell, &arguments),
        "dump" => dump(shell, &arguments),
        // `.import` reads its file with `std::fs`, so its path is confined
        // here rather than inside the reader - see `confine`.
        "import" => crate::import::import(shell, &arguments),
        // `.save` and `.clone` both write the database somewhere else, which is
        // what `.backup` does. SQLite's `.clone` rebuilds the target object by
        // object where `.backup` copies the file; the result is the same
        // database, and this engine has one way of producing a verified copy.
        "backup" | "save" => crate::copy::backup(shell, &format!(".{name}"), &arguments),
        "clone" => crate::copy::clone(shell, &arguments),
        "timeout" => timeout(shell, &arguments),
        "load" => load_extension(shell, &arguments),
        "progress" => progress(shell, &arguments),
        "restore" => crate::copy::restore(shell, &arguments),
        "parameter" => parameter(shell, &arguments),
        "recover" => crate::diagnose::recover(shell, &arguments),
        "sha3sum" => crate::diagnose::sha3sum(shell, &arguments),
        "limit" | "limits" => crate::diagnose::limit(shell, &arguments),
        "selftest" => crate::diagnose::selftest(shell, &arguments),
        "lint" => crate::diagnose::lint(shell, &arguments),
        "dbconfig" => crate::dbconfig::dbconfig(shell, &arguments),
        "auth" => crate::commands::auth(shell, &arguments),
        "connection" => crate::commands::connection(shell, &arguments),
        "imposter" => crate::commands::imposter(shell, &arguments),
        "cd" => crate::commands::cd(shell, &arguments),
        "shell" | "system" => crate::commands::system(shell, &arguments),
        "crlf" => crate::commands::crlf(shell, &arguments),
        "prompt" => crate::commands::prompt(shell, &arguments),
        "explain" => crate::commands::explain(shell, &arguments),
        "nonce" => crate::commands::nonce(shell, &arguments),
        "testcase" => crate::commands::testcase(shell, &arguments),
        "check" => crate::commands::check(shell, &arguments),
        "scanstats" => crate::commands::scanstats(shell, &arguments),
        "trace" => crate::commands::trace(shell, &arguments),
        "dbinfo" => crate::diagnose::dbinfo(shell, &arguments),
        "dbtotxt" => crate::diagnose::dbtotxt(shell),
        "intck" => crate::diagnose::intck(shell, &arguments),
        "filectrl" => crate::diagnose::filectrl(shell, &arguments),
        "excel" => crate::commands::viewer(shell, false),
        "www" => crate::commands::viewer(shell, true),
        "version" => version(shell),
        "show" => show(shell),
        "nullvalues" => shell.complain("Error: unknown command; try .help"),
        _ => shell.complain(&format!(
            // Two spaces, because the reference's format string is split
            // across two string literals and the second one opens with one.
            "Error: unknown command or invalid arguments:  \"{name}\". Enter \".help\" for help"
        )),
    }
}

/// Runs one of the commands that set a single value; see `crate::settings`.
///
/// @param shell - the shell
/// @param name - the command without its dot
/// @param arguments - the words after it
fn settings(shell: &mut Shell, name: &str, arguments: &[&str]) {
    crate::settings::run(shell, name, arguments);
}

/// Reads a number the way C's `atoi` does.
///
/// Leading white space, an optional sign, then digits up to the first character
/// that is not one. No digits is zero. A value outside the range of an `i32`
/// wraps, which is what a process exit status does with it.
///
/// @param word - the text after `.exit`
pub(crate) fn c_atoi(word: &str) -> i32 {
    let trimmed = word.trim_start();
    let (negative, digits) = match trimmed.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, trimmed.strip_prefix('+').unwrap_or(trimmed)),
    };
    let mut value: i64 = 0;
    for digit in digits.chars().map_while(|character| character.to_digit(10)) {
        value = value.wrapping_mul(10).wrapping_add(i64::from(digit));
    }
    let signed = if negative {
        value.wrapping_neg()
    } else {
        value
    };
    signed as i32
}

/// `.timeout MS`: how long a writer waits for the writer slot.
///
/// Sugar over `PRAGMA busy_timeout`, which is where the setting actually lives -
/// so the dot command and the pragma cannot disagree about it.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn timeout(shell: &mut Shell, arguments: &[&str]) {
    let milliseconds = arguments
        .first()
        .and_then(|word| word.parse::<i64>().ok())
        .unwrap_or(0)
        .max(0);
    let _ = shell.collect(&format!("PRAGMA busy_timeout = {milliseconds}"));
}

/// `.load FILE ?ENTRY?`: load a runtime extension.
///
/// **The command exists and answers; what it cannot do is find a library.**
/// This engine has no `sqlite3_load_extension` and no C entry point for one to
/// bind to - a shared library written against SQLite's C API would have nothing
/// here to call - so every `.load` reports the same failure the reference
/// reports for a library it cannot open, in the reference's own words.
///
/// That is a smaller difference than it looks and a much smaller one than the
/// command being absent: a script written for the reference now runs to the
/// same message rather than stopping at "unknown command", and `.help` and the
/// audit both see a shell with the same surface. The inability to load a
/// library is recorded in `docs/feature-comparison.md` rather than hidden.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn load_extension(shell: &mut Shell, arguments: &[&str]) {
    if arguments.is_empty() {
        shell.complain("Usage: .load FILE ?ENTRYPOINT?");
        return;
    }
    shell.complain("Error: The specified module could not be found.");
}

/// `.progress N`: how often the progress handler runs, in opcodes.
///
/// The reference's handler prints nothing unless `--limit` is given, and its
/// visible effect on an ordinary script is therefore none. This accepts the
/// same words and keeps the same state, so a script that sets it runs
/// identically; what it does not do is interrupt a statement part-way, because
/// this engine's VM has no per-opcode callback to hang one on.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn progress(shell: &mut Shell, arguments: &[&str]) {
    let mut interval = 0u64;
    for word in arguments {
        match *word {
            "--once" => shell.progress_once = true,
            "--quiet" | "-q" => shell.progress_quiet = true,
            "--reset" => {
                shell.progress_once = false;
                shell.progress_quiet = false;
                shell.progress_limit = 0;
            }
            "--limit" => {}
            other => match other.parse::<u64>() {
                Ok(number) if shell.progress_pending_limit => {
                    shell.progress_limit = number;
                    shell.progress_pending_limit = false;
                }
                Ok(number) => interval = number,
                Err(_) => {
                    shell.complain(&format!("Error: unknown option: \"{other}\""));
                    return;
                }
            },
        }
        if *word == "--limit" {
            shell.progress_pending_limit = true;
        }
    }
    shell.progress_interval = interval;
}

/// Works out the value `.parameter set NAME VALUE` binds.
///
/// **The word is tried as an SQL expression first and kept as text only when it does
/// not compile.** That is the reference's order: it runs `REPLACE INTO
/// temp.sqlite_parameters(key,value) VALUES('NAME',VALUE)`, and when that does not
/// prepare it stores the word itself as text. So `NULL` binds a NULL, `'42'` (whose
/// quotes the word splitter already removed) binds the integer 42, `1+1` binds 2, and
/// `hello`, which names no column, binds the text `hello`. This used to quote every
/// word that was not a number, which made `.parameter set ?4 NULL` the four letters.
///
/// @param shell - the shell, which evaluates the expression
/// @param word - the VALUE argument after the word splitter
fn parameter_value(shell: &Shell, word: &str) -> Result<Value<'static>, String> {
    let evaluated = match word.contains(';') {
        true => None,
        false => shell.collect(&format!("SELECT {word}")).ok(),
    };
    let rows = match evaluated {
        Some((_, rows)) => rows,
        None => {
            let quoted = format!("SELECT '{}'", word.replace('\'', "''"));
            shell.collect(&quoted).map_err(|failure| failure.message)?.1
        }
    };
    Ok(rows
        .first()
        .and_then(|row| row.first())
        .cloned()
        .unwrap_or(Value::Null))
}

/// Returns a bound value as `.parameter list` prints it, which is SQL's `quote()`.
///
/// `quote()` writes a blob as `X'..'` with an upper case `X` and upper case
/// digits, where the literal the rest of the shell writes is lower case.
///
/// @param value - the bound value
fn quoted_parameter(value: &Value<'static>) -> String {
    match value {
        Value::Blob(blob) => {
            let digits: String = blob
                .raw()
                .iter()
                .map(|byte| format!("{byte:02X}"))
                .collect();
            format!("X'{digits}'")
        }
        other => literal(other),
    }
}

/// `.parameter init | list | set NAME VALUE | unset NAME | clear`.
///
/// **The one dot command with no substitute.** A bound parameter could not be
/// exercised from this shell at all - `SELECT :x + 1` had nothing to bind `:x`
/// to - so the whole parameter surface was untestable from a script.
///
/// The value is a SQL expression, evaluated by the engine rather than parsed
/// here: SQLite runs `SELECT <value>` for it, so `.parameter set :n 1+1` binds
/// 2 and `.parameter set :s 'text'` binds text rather than the four characters
/// of the literal. `init` makes the store, which here is always there, and is a
/// no-op that exists so a script written for the reference runs unchanged.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn parameter(shell: &mut Shell, arguments: &[&str]) {
    // **The shape of the command is checked before it is read.** The reference
    // runs `clear`, `list` and `init` with nothing after them, `set` with a name
    // and one value, and `unset` with a name, and prints the help for the
    // command for anything else. A second value was dropped here, so
    // `.parameter set :f 'it''s'` bound `it` and said nothing about the `s`.
    let words = arguments.len();
    let shaped = match arguments.first().copied() {
        Some("clear" | "list" | "init") => words == 1,
        Some("set") => words == 3,
        Some("unset") => words == 2,
        _ => false,
    };
    if !shaped {
        help(shell, &["parameter"]);
        return;
    }
    match arguments.first().copied().unwrap_or("list") {
        "init" => {}
        "clear" => shell.parameters.clear(),
        "list" => {
            let listed: Vec<(String, String)> = shell
                .parameters
                .iter()
                .map(|(name, value)| (name.clone(), quoted_parameter(value)))
                .collect();
            // The reference pads the name column to the widest name, which is
            // what makes a list of several read as a table.
            let width = listed
                .iter()
                .map(|(name, _)| name.chars().count())
                .max()
                .unwrap_or(0);
            for (name, value) in listed {
                let padding = " ".repeat(width.saturating_sub(name.chars().count()));
                shell.say(&format!("{name}{padding} {value}"));
            }
        }
        "unset" => {
            let Some(name) = arguments.get(1) else {
                shell.complain("Error: .parameter unset needs a name");
                return;
            };
            shell.parameters.remove(*name);
        }
        "set" => {
            let (Some(name), Some(value)) = (arguments.get(1), arguments.get(2)) else {
                shell.complain("Error: .parameter set needs a name and a value");
                return;
            };
            match parameter_value(shell, value) {
                Ok(held) => {
                    shell.parameters.insert((*name).to_string(), held);
                }
                Err(message) => shell.complain(&format!("Error: {message}")),
            }
        }
        other => shell.complain(&format!(
            "Error: unknown .parameter subcommand: \"{other}\""
        )),
    }
}

/// Splits a command line into words, honouring quotes.
/// Drops the semicolon a dot command was typed with.
///
/// A dot command is not SQL and does not need a terminator, but it is typed at
/// the same prompt as SQL and so it gets one anyway. SQLite's shell drops a
/// single trailing `;` from the line before it splits it - `.print hello;`
/// prints `hello`, `.print hello;;` prints `hello;`, and `.separator ;` is left
/// with no argument at all and answers with its usage line. Everything inside
/// the line is untouched: `.print a;b` still prints `a;b`.
fn without_terminator(line: &str) -> &str {
    let trimmed = line.trim_end();
    trimmed.strip_suffix(';').unwrap_or(trimmed)
}

/// Splits a dot command into its words the way the reference does, and notes
/// where each word starts.
///
/// **A quote opens a word only at the start of that word.** A word that begins
/// with `'` or `"` runs to the next quote of the same kind, and a word that
/// begins with anything else runs to the next white space, quotes and all. The
/// first reading is what lets `.print "a b"` print `a b`; the second is what lets
/// `.parameter set :b x'0102'` hand `x'0102'` to the expression reader as one
/// word, where a quote in the middle of a word used to end it and start another,
/// and the parameter was bound to the text `x`.
///
/// **A backslash means something only inside double quotes.** Between double quotes
/// `\"`, `\`, `\n`, `\t`, `\x41` and `\101` are one character each. A word
/// outside quotes, or between single quotes, is taken exactly as written, so
/// `.separator \t` is a backslash and a `t`, and `.separator "\t"` is a tab. A
/// quote with no closing partner runs to the end of the line.
///
/// There is no doubled quote: `'it''s'` is the word `it` followed by the word
/// `s`, as it is in the reference, which then refuses the `.parameter set` for
/// having too many words.
///
/// The offset is where the word's text begins, counted in characters from the start
/// of the line, which is where an error points its caret.
///
/// @param line - the dot command, leading dot included
fn split(line: &str) -> Vec<(String, usize)> {
    let characters: Vec<char> = line.chars().collect();
    let mut words = Vec::new();
    let mut at = 0usize;
    loop {
        while characters
            .get(at)
            .is_some_and(|character| character.is_whitespace())
        {
            at += 1;
        }
        let Some(&first) = characters.get(at) else {
            break;
        };
        let mut word = String::new();
        let quoted = first == '\'' || first == '"';
        if quoted {
            at += 1;
        }
        let start = at;
        if quoted {
            while let Some(&character) = characters.get(at) {
                if character == first {
                    break;
                }
                if character == '\\' && first == '"' && characters.get(at + 1).is_some() {
                    word.push(character);
                    at += 1;
                }
                if let Some(&escaped) = characters.get(at) {
                    word.push(escaped);
                }
                at += 1;
            }
            // The closing quote.
            if characters.get(at) == Some(&first) {
                at += 1;
            }
            if first == '"' {
                word = resolve_backslashes(&word);
            }
        } else {
            while let Some(&character) = characters.get(at) {
                if character.is_whitespace() {
                    break;
                }
                word.push(character);
                at += 1;
            }
        }
        words.push((word, start));
    }
    words
}

/// Reports whether an argument means "on".
///
/// SQLite's shell takes `on`, `yes`, `true` and `1`, and treats a missing
/// argument as on - which is what makes a bare `.headers` do something.
pub fn truthy(argument: Option<&str>) -> bool {
    match argument {
        None => true,
        Some(text) => matches!(
            text.to_ascii_lowercase().as_str(),
            "on" | "yes" | "true" | "1"
        ),
    }
}

/// Prints the commands this shell knows.
fn help(shell: &mut Shell, arguments: &[&str]) {
    let mut lines: Vec<String> = Vec::new();
    let matched = crate::help::show_help(arguments.first().copied(), &mut |line| {
        lines.push(line.to_string())
    });
    for line in lines {
        shell.say(&line);
    }
    // **Nothing found is said rather than shown as an empty answer**, which is
    // the reference's behaviour and the reason `.help` is usable as a search:
    // a pattern that matched nothing has to say so, or it reads as a command
    // that exists and has no help. It goes to the output rather than to the
    // error stream, as the reference sends it.
    if matched == 0 {
        if let Some(asked) = arguments.first() {
            shell.say(&format!("Nothing matches '{asked}'"));
        }
    }
}

/// `.databases`: what is attached, and where each came from.
fn databases(shell: &mut Shell) {
    let Ok((_, rows)) = shell.collect("PRAGMA database_list") else {
        shell.complain("Error: could not read the database list");
        return;
    };
    for row in rows {
        let name = text_of(row.get(1));
        let file = text_of(row.get(2));
        // The reference writes `""` for a database with no file, and states what
        // it opened the file as: `r/w`, or `r/o` after `--readonly`.
        let shown = if file.is_empty() || file == ":memory:" {
            "\"\"".to_string()
        } else {
            // The full path, as the reference reports it, whatever the file was
            // opened as.
            std::path::absolute(&file)
                .map(|full| full.to_string_lossy().into_owned())
                .unwrap_or(file)
        };
        let mode = if name == "main" && shell.is_read_only() {
            "r/o"
        } else {
            "r/w"
        };
        shell.say(&format!("{name}: {shown} {mode}"));
    }
}

/// Returns a value as plain text.
fn text_of(value: Option<&Value<'static>>) -> String {
    match value {
        Some(Value::Text(text)) => String::from_utf8_lossy(text.raw()).into_owned(),
        Some(Value::Null) | None => String::new(),
        Some(other) => literal(other),
    }
}

/// `.tables`: the tables and views, in one space-separated block.
fn tables(shell: &mut Shell, arguments: &[&str]) {
    let mut condition = String::from("type IN ('table','view') AND name NOT LIKE 'sqlite_%'");
    if let Some(pattern) = arguments.first() {
        condition.push_str(&format!(" AND name LIKE {}", literal_text(pattern)));
    }
    let names = names_in_every_schema(shell, &condition);
    for line in columnise(&names) {
        shell.say(&line);
    }
}

/// Returns the quoted name of a schema's catalog table.
///
/// @param schema - `main`, `temp` or an attached name
fn catalog_of(schema: &str) -> String {
    format!("{}.sqlite_master", quote_identifier(schema))
}

/// Returns the names of the objects that meet a condition in every schema.
///
/// **Every schema the connection has, not only `main`.** `.tables` and `.indexes`
/// on a connection with a temporary table or an attached database listed
/// `main` alone, so `CREATE TEMP TABLE t(a)` followed by `.tables` answered
/// nothing. The reference lists the objects of `temp` and of every attached
/// database too, writes `schema.name` for the ones outside `main`, and sorts the
/// whole list by what it writes.
///
/// @param shell - the shell
/// @param condition - a `WHERE` condition over `name`, `type` and `tbl_name`
fn names_in_every_schema(shell: &Shell, condition: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for (schema, _) in shell.database_list() {
        let sql = format!("SELECT name FROM {} WHERE {condition}", catalog_of(&schema));
        for name in shell.column(&sql) {
            if schema.eq_ignore_ascii_case("main") {
                names.push(name);
            } else {
                names.push(format!("{schema}.{name}"));
            }
        }
    }
    names.sort();
    names
}

/// How wide the reference lays a name list out for.
const SCREEN: usize = 80;

/// The gap the reference leaves between one column and the next.
const GUTTER: usize = 5;

/// Lays a list of names out the way `.tables` and `.indexes` do.
///
/// **Column-major, padded per column, and as few rows as fit.** Measured
/// against the reference rather than guessed: fourteen tables of increasing
/// name length come back in five columns of three rows, each column padded to
/// its *own* longest entry plus five, with the last column unpadded. One row is
/// used whenever the whole list fits in eighty characters, which is every case
/// a script is likely to have.
///
/// @param names - the names, already sorted
fn columnise(names: &[String]) -> Vec<String> {
    if names.is_empty() {
        return Vec::new();
    }
    for rows in 1..=names.len() {
        let columns = names.len().div_ceil(rows);
        let widths: Vec<usize> = (0..columns)
            .map(|column| {
                names
                    .iter()
                    .skip(column.saturating_mul(rows))
                    .take(rows)
                    .map(|name| name.chars().count())
                    .max()
                    .unwrap_or(0)
            })
            .collect();
        let total: usize = widths
            .iter()
            .enumerate()
            .map(|(at, width)| {
                if at.saturating_add(1) == columns {
                    *width
                } else {
                    width.saturating_add(GUTTER)
                }
            })
            .sum();
        if total > SCREEN && rows < names.len() {
            continue;
        }
        let mut lines = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut line = String::new();
            for column in 0..columns {
                let Some(name) = names.get(column.saturating_mul(rows).saturating_add(row)) else {
                    continue;
                };
                if !line.is_empty() {
                    // The previous column was padded to its own width; the
                    // gutter is what separates them.
                    line.push_str(&" ".repeat(GUTTER));
                }
                line.push_str(name);
                let width = widths.get(column).copied().unwrap_or(0);
                let last = column.saturating_add(1) == columns
                    || names
                        .get(
                            column
                                .saturating_add(1)
                                .saturating_mul(rows)
                                .saturating_add(row),
                        )
                        .is_none();
                if !last {
                    line.push_str(&" ".repeat(width.saturating_sub(name.chars().count())));
                }
            }
            if !line.is_empty() {
                lines.push(line);
            }
        }
        return lines;
    }
    Vec::new()
}

/// `.indexes`: the indexes, optionally on one table.
fn indexes(shell: &mut Shell, arguments: &[&str]) {
    let mut condition = String::from("type = 'index'");
    if let Some(pattern) = arguments.first() {
        // **The argument is a substring of the *index* name, not the table's.**
        // Measured against the reference rather than read off the help text:
        // `.indexes people` answers `people_name`, `.indexes name` answers it
        // too, and `.indexes t` - the table an index called `ia` is on -
        // answers nothing. `.tables` is the other rule and is left alone:
        // `.tables people` answers `people` and not `peoples`.
        condition.push_str(&format!(
            " AND name LIKE {}",
            literal_text(&format!("%{pattern}%"))
        ));
    }
    condition.push_str(" AND name NOT LIKE 'sqlite_%'");
    let names = names_in_every_schema(shell, &condition);
    for line in columnise(&names) {
        shell.say(&line);
    }
}

/// What `.schema` was asked for.
#[derive(Default)]
struct SchemaOptions<'a> {
    /// `--indent`: put each column of a table on a line of its own.
    indent: bool,
    /// `--nosys`: leave out the objects whose names begin with `sqlite_`.
    nosys: bool,
    /// The `LIKE` pattern naming the objects to show, if one was given.
    pattern: Option<&'a str>,
}

/// Reads the options and the pattern after `.schema`.
///
/// **An option is read as an option.** The first word was taken as the pattern
/// whatever it was, so `.schema --indent` matched no object and printed nothing.
/// The reference accepts one or two dashes, refuses an option it does not know
/// by name, and refuses a second pattern with the usage line.
///
/// @param shell - where a complaint goes
/// @param arguments - the words after `.schema`
fn schema_options<'a>(shell: &mut Shell, arguments: &[&'a str]) -> Option<SchemaOptions<'a>> {
    let mut options = SchemaOptions::default();
    for argument in arguments {
        let flag = argument
            .strip_prefix('-')
            .map(|rest| rest.strip_prefix('-').unwrap_or(rest));
        match flag {
            Some("indent") => options.indent = true,
            Some("nosys") => options.nosys = true,
            Some(_) => {
                shell.complain(&format!("Unknown option: \"{argument}\""));
                return None;
            }
            None if options.pattern.is_none() => options.pattern = Some(argument),
            None => {
                shell.complain("Usage: .schema ?--indent? ?--nosys? ?LIKE-PATTERN?");
                return None;
            }
        }
    }
    Some(options)
}

/// `.schema`: the statements that would rebuild what is there.
///
/// **Every schema, in the order `PRAGMA database_list` gives them.** `main` first,
/// then `temp`, then the attached databases. An object outside `main` is written
/// with its schema name after the keywords, as `CREATE TABLE temp.tt(c);`, which is
/// what makes the statement rebuild it in the right place.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn schema(shell: &mut Shell, arguments: &[&str]) {
    let Some(options) = schema_options(shell, arguments) else {
        return;
    };
    let schemas: Vec<String> = shell
        .database_list()
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    list_schemas(shell, &options, &schemas);
}

/// Prints the `CREATE` statements of the objects in some schemas.
///
/// @param shell - the shell
/// @param options - what `.schema` was asked for
/// @param schemas - the schemas to list, in order
fn list_schemas(shell: &mut Shell, options: &SchemaOptions<'_>, schemas: &[String]) {
    for schema_name in schemas.iter().map(String::as_str) {
        let mut sql = format!(
            "SELECT type, name, sql FROM {} WHERE sql IS NOT NULL",
            catalog_of(schema_name)
        );
        if let Some(pattern) = options.pattern {
            let quoted = literal_text(pattern);
            sql.push_str(&format!(
                " AND (name LIKE {quoted} OR tbl_name LIKE {quoted})"
            ));
        }
        if options.nosys {
            sql.push_str(" AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'");
        }
        sql.push_str(" ORDER BY rowid");
        let Ok((_, rows)) = shell.collect(&sql) else {
            continue;
        };
        for row in rows {
            let kind = text_of(row.first());
            let name = text_of(row.get(1));
            let statement = qualified_statement(&text_of(row.get(2)), schema_name);
            if kind == "view" {
                // A view's text does not say what columns it produces, so the
                // columns go underneath in a comment - which is what makes
                // `.schema` on a view worth reading.
                let listed = view_columns(shell, schema_name, &name);
                let shown = shown_name(schema_name, &name);
                shell.say(&format!("{statement}\n/* {shown}({listed}) */;"));
                continue;
            }
            if options.indent && kind != "trigger" {
                shell.say(&crate::schema_indent::indented(&statement));
                continue;
            }
            shell.say(&format!("{statement};"));
        }
    }
}

/// Returns an object's name as the reference writes it: bare in `main`, and
/// after its schema's name anywhere else.
///
/// @param schema - the schema the object is in
/// @param name - the object's name
fn shown_name(schema: &str, name: &str) -> String {
    if schema.eq_ignore_ascii_case("main") {
        name.to_string()
    } else {
        format!("{schema}.{name}")
    }
}

/// Puts a schema's name after the keywords of a stored `CREATE` statement.
///
/// The catalog stores `CREATE TABLE tt(c)` for a table in `temp`, and the
/// reference prints `CREATE TABLE temp.tt(c)`. A statement in `main` is left as
/// it is.
///
/// @param statement - the text the catalog holds
/// @param schema - the schema the object is in
fn qualified_statement(statement: &str, schema: &str) -> String {
    if schema.eq_ignore_ascii_case("main") {
        return statement.to_string();
    }
    for prefix in [
        "CREATE VIRTUAL TABLE ",
        "CREATE UNIQUE INDEX ",
        "CREATE TABLE ",
        "CREATE INDEX ",
        "CREATE VIEW ",
        "CREATE TRIGGER ",
    ] {
        let Some(head) = statement.get(..prefix.len()) else {
            continue;
        };
        if head.eq_ignore_ascii_case(prefix) {
            let rest = statement.get(prefix.len()..).unwrap_or_default();
            return format!("{head}{schema}.{rest}");
        }
    }
    statement.to_string()
}

/// Returns a view's column names, comma-separated, each quoted when it needs to be.
///
/// @param shell - the shell
/// @param schema - the schema the view is in
/// @param name - the view
fn view_columns(shell: &Shell, schema: &str, name: &str) -> String {
    let sql = format!(
        "PRAGMA {}.table_info({})",
        quote_identifier(schema),
        quote_identifier(name)
    );
    let Ok((_, rows)) = shell.collect(&sql) else {
        return String::new();
    };
    rows.iter()
        .map(|row| crate::render::insert_column(&text_of(row.get(1))))
        .collect::<Vec<String>>()
        .join(",")
}

/// Returns an identifier quoted the way SQL wants it.
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// `.fullschema`: the schema, then whatever `ANALYZE` recorded.
fn full_schema(shell: &mut Shell) {
    list_schemas(shell, &SchemaOptions::default(), &["main".to_string()]);
    let exists = shell.scalar("SELECT count(*) FROM sqlite_master WHERE name = 'sqlite_stat1'");
    if exists.as_deref() == Some("0") || exists.is_none() {
        shell.say("/* No STAT tables available */");
        return;
    }
    let layout = core::mem::replace(
        &mut shell.layout,
        Layout {
            mode: crate::render::Mode::Insert,
            table: "sqlite_stat1".to_string(),
            ..Layout::default()
        },
    );
    shell.run("SELECT * FROM sqlite_stat1");
    shell.layout = layout;
}

/// Returns a string as an SQL literal.
fn literal_text(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

/// Resolves the backslash escapes inside a double quoted dot command argument.
///
/// **A port of `shell.c`'s `resolve_backslashes`.** `\a \b \f \n \r \t \v`, `\"`,
/// `\'` and a doubled backslash are one character each, `\x` and up to two
/// hexadecimal digits is a byte, and a backslash and up to three octal digits is
/// a byte. The
/// reference's rule for an unknown escape is to keep the character that followed
/// the backslash and drop the backslash, so a script that writes `\q` gets `q`
/// from both shells. A byte of 0x80 or more is carried as that byte, which is
/// what `.separator "\x1f"` and a Latin 1 `\xe9` ask for.
///
/// @param text - what the caller wrote, escapes and all
fn resolve_backslashes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '\\' {
            out.push(character);
            continue;
        }
        match characters.next() {
            Some('a') => out.push('\u{7}'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('v') => out.push('\u{b}'),
            Some('x') => push_numbered_byte(&mut out, &mut characters, 16, 2, 'x'),
            Some(digit @ '0'..='7') => {
                let mut value = digit.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match characters.peek().and_then(|next| next.to_digit(8)) {
                        Some(more) => {
                            value = value * 8 + more;
                            characters.next();
                        }
                        None => break,
                    }
                }
                push_byte(&mut out, (value & 0xff) as u8);
            }
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Reads up to `most` digits in a radix and pushes the byte they make.
///
/// With no digits at all the escape keeps the letter that introduced it.
///
/// @param out - the text being built
/// @param characters - the rest of the argument
/// @param radix - 16 for a hexadecimal escape
/// @param most - how many digits at most
/// @param letter - what to write when there are no digits
fn push_numbered_byte(
    out: &mut String,
    characters: &mut std::iter::Peekable<std::str::Chars<'_>>,
    radix: u32,
    most: usize,
    letter: char,
) {
    let mut value = 0u32;
    let mut count = 0usize;
    while count < most {
        match characters.peek().and_then(|next| next.to_digit(radix)) {
            Some(digit) => {
                value = value * radix + digit;
                characters.next();
                count += 1;
            }
            None => break,
        }
    }
    if count == 0 {
        out.push(letter);
    } else {
        push_byte(out, (value & 0xff) as u8);
    }
}

/// Pushes one byte: as itself when it is ASCII, and as the stand in for a byte
/// that is not text otherwise (see `crate::raw`).
///
/// @param out - the text being built
/// @param byte - the byte
fn push_byte(out: &mut String, byte: u8) {
    out.push_str(&crate::raw::decode(&[byte]));
}

/// `.width`: minimum widths for the columnar modes.
///
/// Each word is read the way C's `atoi` reads it, so a word that is not a number is
/// zero, which is as wide as the widest value. A negative width right aligns the
/// column. `.width` with nothing after it clears them.
///
/// @param shell - the shell
/// @param arguments - the words after the command
fn width(shell: &mut Shell, arguments: &[&str]) {
    shell.layout.widths = arguments.iter().map(|word| c_atoi(word)).collect();
}

/// `.read`: runs a script as though it had been typed.
fn read(shell: &mut Shell, arguments: &[&str]) {
    let [named] = arguments else {
        shell.complain("Usage: .read FILE");
        return;
    };
    let Some(bytes) = read_source(shell, named) else {
        return;
    };
    run_script_bytes(shell, named, &bytes);
}

/// How many files `.read` may have open inside one another.
///
/// The reference's limit is 25 and it refuses the 25th, so 24 run.
const MAX_INPUT_NESTING: usize = 25;

/// Returns the bytes `.read` reads for a name: a file, or what a command prints.
///
/// **A name that begins with `|` is a command**, as it is in the reference: its
/// standard output is the script. Anything else is a path, which goes through
/// `--root` confinement. A failure is reported here, in the reference's words.
///
/// @param shell - where a complaint goes
/// @param named - the file name, or `|` and a command
fn read_source(shell: &mut Shell, named: &str) -> Option<Vec<u8>> {
    if let Some(command) = named.strip_prefix('|') {
        return match crate::commands::capture_output(command) {
            Some(bytes) => Some(bytes),
            None => {
                shell.complain(&format!("Error: cannot open \"{named}\""));
                None
            }
        };
    }
    let path = confine_path(shell, named)?;
    match std::fs::read(&path) {
        Ok(bytes) => Some(bytes),
        Err(_) => {
            shell.complain(&format!("Error: cannot open \"{path}\""));
            None
        }
    }
}

/// Runs the lines of a script as though they had been typed.
///
/// **A file is one level of nesting, and 25 levels is an error** instead of a
/// stack overflow: a file that reads itself would otherwise run until the process
/// ended. The error names the file and the line that asked for it. `.quit` ends
/// this file only, so the `done` flag is cleared when the file ends, and `.exit`
/// ends everything. Errors in the file say `of FILE` after the line number.
///
/// @param shell - the shell
/// @param named - the file name as typed, which errors name
/// @param bytes - the file's contents
pub fn run_script_bytes(shell: &mut Shell, named: &str, bytes: &[u8]) {
    if shell.depth.saturating_add(1) >= MAX_INPUT_NESTING {
        let line = shell.line;
        shell.complain(&format!(
            "{named}: Input nesting limit ({MAX_INPUT_NESTING}) reached at line {line}. Check recursion."
        ));
        return;
    }
    let lines = crate::raw::split_lines(bytes);
    let outer_source = shell.source.replace(named.to_string());
    let outer_argument = shell.argument.take();
    let outer_line = shell.line;
    shell.depth = shell.depth.saturating_add(1);
    drive(shell, lines.into_iter());
    shell.depth = shell.depth.saturating_sub(1);
    shell.source = outer_source;
    shell.argument = outer_argument;
    shell.line = outer_line;
    if shell.exit_code.is_none() {
        shell.done = false;
    }
}

/// Reports a mistake in one word of the dot command being run, in the reference's form.
///
/// Two lines: the command as it was typed, and a caret under the first character
/// of the word with the reason after it, each starting `line N: `.
///
/// @param shell - where the complaint goes
/// @param word - which word, counting the command itself as word 0
/// @param reason - what is wrong with it, such as `unknown option`
pub(crate) fn word_error(shell: &mut Shell, word: usize, reason: &str) {
    let line = shell.line;
    let column = shell.dot_offsets.get(word).copied().unwrap_or(0);
    let typed = shell.dot_line.clone();
    shell.complain(&format!("line {line}: {typed}"));
    shell.complain(&format!("line {line}: {}^--- {reason}", " ".repeat(column)));
}

/// Reports an option a dot command does not know.
///
/// @param shell - where the complaint goes
/// @param command - the command with its dot, such as `.dump`
/// @param arguments - every word after the command
/// @param option - the word that was not understood
pub(crate) fn unknown_option(shell: &mut Shell, command: &str, arguments: &[&str], option: &str) {
    let _ = command;
    let position = arguments
        .iter()
        .position(|word| *word == option)
        .map_or(1, |at| at + 1);
    word_error(shell, position, "unknown option");
}

/// `.dump`: the SQL that would rebuild the database.
fn dump(shell: &mut Shell, arguments: &[&str]) {
    let mut options = crate::dump::DumpOptions::default();
    for argument in arguments {
        if let Some(flag) = argument.strip_prefix('-') {
            // **An option is read as an option.** The first word after `.dump`
            // was taken as the pattern whatever it was, so `.dump --nosys`
            // matched no table whose name looked like `--nosys` and dumped
            // nothing, and `inillucent dump --data-only` was an empty dump.
            // One or two dashes both work, as they do in the reference.
            match flag.strip_prefix('-').unwrap_or(flag) {
                "data-only" => options.data_only = true,
                "nosys" => options.nosys = true,
                "newlines" => {}
                "preserve-rowids" => {
                    shell.complain("Error: .dump --preserve-rowids is not built");
                    return;
                }
                _ => {
                    unknown_option(shell, ".dump", arguments, argument);
                    return;
                }
            }
            continue;
        }
        let Some(pattern) = confine_path(shell, argument) else {
            return;
        };
        options.patterns.push(pattern);
    }
    crate::dump::dump(shell, &options);
}

/// `.version`: what this is and what it implements.
fn version(shell: &mut Shell) {
    let library = shell
        .scalar("SELECT sqlite_version()")
        .unwrap_or_else(|| "unknown".to_string());
    shell.say(&format!("SQLite {library}"));
    shell.say("inillucent (a first-party engine implementing the SQLite ABI)");
}

/// `.show`: the current settings.
///
/// **Three of these twelve used to be literals** (task-2066 section 4.4.13):
/// `explain` was written as `auto`, `stats` as `off` and `output` as `stdout`,
/// so `.show` answered the same three values whatever `.explain on`,
/// `.stats on` or `.output FILE` had done. That is worse than not reporting
/// them. A person runs `.show` to find out where their rows went and what mode
/// they left the shell in, and the report told them confidently.
///
/// `dot_commands.rs` says in its own header that a setter is asserted through
/// `.show`, so every case that used this report to check a setting was checking
/// a constant. The three that were constants are now read off the shell.
fn show(shell: &mut Shell) {
    let lines = vec![
        format!("        echo: {}", on_off(shell.echo)),
        format!("         eqp: {}", shell.eqp_name),
        format!("     explain: {}", shell.explain_mode.name()),
        format!("     headers: {}", on_off(shell.layout.headers)),
        format!("        mode: {}", shell.layout.mode.name()),
        format!(
            "   nullvalue: {}",
            crate::modes::c_quoted(&shell.layout.null)
        ),
        format!("      output: {}", shell.output_target()),
        format!(
            "colseparator: {}",
            crate::modes::c_quoted(&shell.layout.separator)
        ),
        format!(
            "rowseparator: {}",
            crate::modes::c_quoted(&shell.layout.row_separator)
        ),
        format!("       stats: {}", on_off(shell.stats)),
        format!("       width: {}", widths_text(&shell.layout.widths)),
        format!("    filename: {}", shell.path().to_string()),
    ];
    for line in lines {
        shell.say(&line);
    }
}

/// Returns "on" or "off".
fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

/// Renders the fixed column widths, each followed by a space as the reference writes them.
fn widths_text(widths: &[i32]) -> String {
    widths.iter().map(|width| format!("{width} ")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Quotes hold a word together, and whitespace between words splits them.
    #[test]
    fn arguments_split_the_way_a_shell_splits_them() {
        let words =
            |line: &str| -> Vec<String> { split(line).into_iter().map(|(word, _)| word).collect() };
        assert_eq!(words(".mode csv"), vec![".mode", "csv"]);
        assert_eq!(
            words(".output \"my file.txt\""),
            vec![".output", "my file.txt"]
        );
        assert_eq!(words(".separator \" | \""), vec![".separator", " | "]);
        assert_eq!(words(".print"), vec![".print"]);
    }

    /// A backslash is an escape only between double quotes, and each word knows
    /// where it begins.
    #[test]
    fn a_backslash_means_something_only_inside_double_quotes() {
        let words =
            |line: &str| -> Vec<String> { split(line).into_iter().map(|(word, _)| word).collect() };
        assert_eq!(words(r#".print a\tb"#), vec![".print", r"a\tb"]);
        assert_eq!(words(r#".print 'a\tb'"#), vec![".print", r"a\tb"]);
        assert_eq!(words(r#".print "a\tb""#), vec![".print", "a\tb"]);
        assert_eq!(words(r#".print "a\"b""#), vec![".print", "a\"b"]);
        assert_eq!(words(r#".print "x\x41y\101""#), vec![".print", "xAyA"]);
        assert_eq!(words(r#".print "a\qb""#), vec![".print", "aqb"]);
        let at: Vec<usize> = split(".print \"ab\" c")
            .into_iter()
            .map(|(_, at)| at)
            .collect();
        assert_eq!(at, vec![0, 8, 12]);
    }

    /// `.exit` reads its status the way C's `atoi` does.
    #[test]
    fn an_exit_status_is_read_like_atoi() {
        assert_eq!(c_atoi("3"), 3);
        assert_eq!(c_atoi("3x"), 3);
        assert_eq!(c_atoi("abc"), 0);
        assert_eq!(c_atoi("-1"), -1);
        assert_eq!(c_atoi("  7"), 7);
    }

    /// A missing argument means on, which is what a bare `.headers` needs.
    #[test]
    fn a_missing_argument_means_on() {
        assert!(truthy(None));
        assert!(truthy(Some("on")));
        assert!(truthy(Some("YES")));
        assert!(!truthy(Some("off")));
        assert!(!truthy(Some("nonsense")));
    }
}

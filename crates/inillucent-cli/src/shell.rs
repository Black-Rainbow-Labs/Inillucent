//! The shell's state, and the loop that reads a line and decides what it is.
//!
//! Invariant: the shell is an adapter. It parses dot commands, formats output
//! and manages files; every statement it runs goes through the public `inillucent`
//! facade, and it never reaches past it. That is the rule that keeps the shell
//! from becoming a second, slightly different database - which is exactly what
//! happens to a shell that starts "just reading the schema directly".
//!
//! Input is accumulated until it is a complete statement. That is the one piece
//! of real logic here and it is not a nicety: a `CREATE TRIGGER` spans many
//! lines and holds semicolons inside its body, so "ends with a semicolon" is
//! wrong and the engine's own parser has to be the one that says when a
//! statement is finished.

use inillucent_engine::connect::{Connection, Database, Statement};
use inillucent_tree::datum::{owned_row_values, OwnedDatum};
use inillucent_value::Value;

use crate::render::{render, Layout};

mod deferred;
pub mod output;
pub mod redirect;
mod stream;

/// Everything the shell remembers between lines.
/// One open database and the session statements on it belong to.
pub struct Opened {
    /// The database.
    ///
    /// A connection is a borrow of it rather than a thing of its own, so one is
    /// made where it is used instead of being stored - storing it beside the
    /// database it borrows would be a self-referential struct for no gain.
    database: Database,
    /// The session every one of those borrows is a continuation of.
    ///
    /// **Because a shell is one connection, not one per statement.** Temporary
    /// objects belong to a session: `CREATE TEMP TABLE t(a)` puts `t` in the
    /// session's own database, and a `SELECT` on a *different* session cannot
    /// see it. Calling `Database::connect` per statement opened a new session
    /// each time, so the shell reported success on the `CREATE` and then
    /// `no such table: t` on the very next line.
    ///
    /// The engine was fixed to add `connect_as` for callers that hand out a
    /// connection per call over one logical connection; the shell is one of
    /// those and was not converted.
    session: u64,
    /// Where the database came from, for `.databases` and the prompt.
    path: String,
    /// Whether the file was opened read only, which `.databases` reports as `r/o`.
    read_only: bool,
    /// How `.connection` names this database when that is not its path.
    ///
    /// The reference prints `:memory:` for a database opened by that name, `(memory)`
    /// for one opened with no name, and `(temporary-file)` for an empty name, and
    /// all three are in memory here. `None` means the in-memory form with no name.
    label: Option<String>,
}

/// How many databases `.connection` can hold open at once.
///
/// Five, which is the reference's own array size. A slot that has never been
/// switched to is closed, and switching to one opens an in-memory database
/// there - which is what makes `.connection 1` a working command on a shell
/// that was started with one file.
pub const CONNECTIONS: usize = 5;

/// One shell session: the databases it can reach, and every setting a dot
/// command can change.
pub struct Shell {
    /// The flag that stops the statement this shell is running.
    ///
    /// **The shell does not go through `command::run`, so it arms its own
    /// budget (task-1932, H11).** Every statement a person types runs inside
    /// this, and `interrupt::stop_on_ctrl_c` is what a front end registers it
    /// with - so Ctrl+C ends the query rather than the program, and a second
    /// press still ends the program because the operating system's default
    /// handler comes back once ours has fired.
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The databases `.connection` switches between; slot 0 is the one the
    /// shell was started on.
    connections: Vec<Option<Opened>>,
    /// Which slot statements run on.
    active: usize,
    /// How results are laid out.
    pub layout: Layout,
    /// What `.headers` last set, or `None` when it has not been used.
    ///
    /// SQLite's shell keeps the choice apart from the mode: until `.headers`
    /// is used, `.mode` turns headers on for the tabular modes and off for
    /// the others, and once it is used the choice survives every later
    /// `.mode`. A single flag that `.mode box` set to on left headers on in
    /// `list` and `insert` mode afterwards.
    pub headers_chosen: Option<bool>,
    /// Whether a result with no rows still prints its header line.
    ///
    /// Off for everything a person or a script types, because `sqlite3`
    /// prints nothing for an empty result. `inillucent export` turns it on
    /// for its one statement, so an exported file always names its columns.
    pub header_when_empty: bool,
    /// Where output goes, when it is not standard output.
    ///
    /// It carries the name `.show` reports: a `File` does not carry the path it
    /// was opened with on any platform this builds for, and before the name was
    /// kept `.show` printed `output: stdout` while a `.output` redirect was
    /// open, which is the one line of that report a person reads when they
    /// cannot find where their rows went.
    redirection: Option<redirect::Redirection>,
    /// Whether a failing statement stops the script.
    pub bail: bool,
    /// Whether each statement is echoed before it runs.
    pub echo: bool,
    /// Whether to print how long each statement took.
    pub timer: bool,
    /// Whether `.timer once` was given, which turns the timer off after one statement.
    pub timer_once: bool,
    /// What `.eqp` was set to, as `.show` prints it: `off`, `on`, `trigger` or `full`.
    pub eqp_name: &'static str,
    /// The settings `.mode --once` puts back after the next statement.
    pub mode_once: Option<crate::modes::Snapshot>,
    /// The layouts `.mode --tag NAME` saved, by name.
    pub mode_tags: std::collections::HashMap<String, Layout>,
    /// Whether a statement has run since the last command ended.
    statement_ran: bool,
    /// The text of the dot command being run, which an error under it quotes.
    pub dot_line: String,
    /// Where each word of that command begins, counted in characters.
    pub dot_offsets: Vec<usize>,
    /// Whether the page cache's counters are printed after each statement.
    pub stats: bool,
    /// Whether to print the change count after each statement.
    pub show_changes: bool,
    /// Whether an `EXPLAIN QUERY PLAN` is printed before each statement.
    pub explain_plan: bool,
    /// Whether output lines end with a carriage return, which `.crlf` sets.
    pub crlf: bool,
    /// The prompt an interactive session prints for a new statement.
    pub prompt_main: String,
    /// The prompt it prints for a statement that is not finished.
    pub prompt_continue: String,
    /// When an `EXPLAIN` listing is laid out as a table.
    pub explain_mode: crate::commands::ExplainMode,
    /// The token `.nonce` set, which suspends safe mode for one command.
    pub nonce: Option<String>,
    /// The name of the `.testcase` that is capturing output, if one is.
    pub testcase: Option<String>,
    /// What has been printed since that `.testcase`.
    pub captured: String,
    /// How many `.check`s have run.
    pub tests_run: usize,
    /// How many of them failed.
    pub tests_failed: usize,
    /// Where a `.excel` or `.www` file is being written, if one is.
    pub viewer: Option<std::path::PathBuf>,
    /// Whether the authorizer's decisions are printed, which `.auth` sets.
    pub auth: bool,
    /// The decisions it has recorded since the last statement.
    pub authorized: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    /// Where `.trace` sends each statement, when it sends it anywhere.
    pub trace: Option<String>,
    /// What `.scanstats` was set to.
    pub scanstats: String,
    /// Whether `SQLITE_DBCONFIG_DEFENSIVE` is in force.
    ///
    /// **On, because the reference's shell turns it on.** It is the flag that
    /// makes `PRAGMA journal_mode = OFF` and `PRAGMA writable_schema = ON`
    /// refuse rather than take effect, and a shell that left it off answered
    /// those two differently from the reference on a fresh database.
    pub defensive: bool,
    /// Whether the shell should stop reading the input it is reading now.
    ///
    /// `.quit` sets it. A `.read` clears it when its file ends, because `.quit`
    /// inside a file stops that file and not the session; `.exit` sets
    /// `exit_code` as well, and that is never cleared.
    pub done: bool,
    /// The status `.exit` asked the process to end with.
    pub exit_code: Option<i32>,
    /// The file being read by `.read` or `-init`, which an error names.
    ///
    /// The reference ends an error heading with `of FILE` for a statement that
    /// came from a file, and not for one typed on standard input.
    pub source: Option<String>,
    /// Which command line argument is being run, counted from the program name.
    ///
    /// The reference says `in 3rd command line argument` instead of
    /// `near line N` for a statement given as an argument.
    pub argument: Option<usize>,
    /// How many `.read` files are open inside one another.
    pub depth: usize,
    /// Whether anything has failed, which decides the exit code.
    pub failed: bool,
    /// Whether a chunk of input is being run, so that the error of a statement that
    /// fails while running is held until the chunk ends. See `run_chunk`.
    pub holding: bool,
    /// The lines of the last runtime error of the chunk being run.
    pub held_error: Option<Vec<String>>,
    /// Whether a statement of the chunk failed to compile, which ends the chunk.
    pub chunk_stop: bool,
    /// The text of the chunk from the start of the statement being run.
    ///
    /// The reference draws the excerpt under an error from the rest of the line, not from
    /// the statement alone, so the statements that follow a failing one appear in it.
    pub excerpt: Option<Excerpt>,
    /// The engine's error for the first statement that failed since `failed`
    /// was last cleared, when the failure came from the engine.
    ///
    /// **Kept so a command can report the right status (task-2120).** The
    /// printed text says what went wrong; only the `DbError` says which class
    /// of failure it was. `inillucent run` used to report every failure in a
    /// script as `syntax` with exit code 1, so a statement the engine has not
    /// built - `exec` reports it as `unsupported` with exit code 3 - told the
    /// caller to look for a mistake in SQL that had none.
    pub first_error: Option<inillucent_base::DbError>,
    /// The line the statement being run started on.
    pub line: usize,
    /// Where `.log` was pointed, when it was pointed anywhere.
    ///
    /// Recorded and never written to: this engine emits no log messages, so the
    /// destination is a place nothing arrives. `.show` reports it, which is the
    /// only thing that reads it.
    pub log_to: Option<String>,
    /// How often `.progress` was asked to run a handler, in opcodes.
    ///
    /// Recorded and reported by `.show`, and never acted on: the reference's
    /// handler prints nothing unless `--limit` is given, and this engine's VM
    /// has no per-opcode callback to hang one on. Keeping the state means a
    /// script written for the reference sets it and runs on rather than
    /// stopping at "unknown command".
    pub progress_interval: u64,
    /// The `--limit` `.progress` was given.
    pub progress_limit: u64,
    /// Whether `.progress --once` was asked for.
    pub progress_once: bool,
    /// Whether `.progress --quiet` was asked for.
    pub progress_quiet: bool,
    /// Whether the next number belongs to a `--limit` that has just been read.
    pub progress_pending_limit: bool,
    /// Whether a statement that changes something is refused.
    ///
    /// `-readonly` on the command line, and `--readonly` on `inillucent` and
    /// `inillucent-mcp`. **The binder decides what writes, not a scan of the
    /// text**: `EXPLAIN QUERY PLAN` over the statement fails with "not a
    /// read-only statement" for anything that does, which cannot be talked past
    /// with whitespace, a comment or an unusual capitalisation. It is the same
    /// mechanism `inillucent-driver` uses and it is deliberately the same one -
    /// two classifiers would eventually disagree, and the one that let a write
    /// through would be the one nobody was watching.
    ///
    /// The *file* is still open for writing. The capability table says
    /// `readonly_open` is `partial` and says exactly this, which is why the row
    /// is worth reading before an application decides what it means by "open
    /// this read only".
    pub readonly: bool,
    /// Whether the commands that reach outside the database are refused.
    ///
    /// `-safe` on the command line. The set is the reference's: running a
    /// program (`.shell`, `.system`), loading a shared library (`.load`),
    /// changing the working directory (`.cd`), handing a file to whatever the
    /// system opens it with (`.excel`, `.www`), and writing output through a
    /// pipe (`.output |cmd`, `.once |cmd`). Every one of them is a way for a
    /// script that was only supposed to query a database to run code.
    ///
    /// `.nonce` lifts it for one command, which is the reference's own escape
    /// hatch and is why the token is a secret the script's author chose.
    pub safe: bool,
    /// Whether `.shell` and `.system` are refused because the surface was opened with
    /// `--readonly`.
    ///
    /// `inillucent --readonly run ".shell ..."` ran the program, which can write files
    /// anywhere the process can, so a surface that promised not to write did (bug hunt
    /// 10). Only those two commands are refused. `.output`, `.read` and the rest of the
    /// safe mode set are left alone, because a read only script reads files and writes
    /// reports. The shell program's own `-readonly` is the reference's option for opening
    /// the file read only and does not set this.
    pub no_programs: bool,
    /// Where output goes when a caller is collecting it rather than printing.
    ///
    /// **Not the same as `captured`, and deliberately outside it.** `captured`
    /// belongs to `.testcase`/`.check`, which compare one command's output
    /// against an expected digest; this belongs to a caller running the shell
    /// as a subroutine - the `run` command and the MCP server behind it
    /// - and has to still be collecting while a `.testcase` inside the script it
    /// was given is doing its own thing. So `say` checks the testcase first,
    /// and a script that uses both nests the way it reads.
    ///
    /// **A `.once` or `.output` redirect is checked before this** and takes the
    /// rows, which is what makes `export --out` write its file - see the
    /// comment in `say`. So a collected script that redirects hands its caller
    /// whatever was not redirected, which for an export is nothing.
    ///
    /// `complain` writes here whatever a redirect is doing, because a caller
    /// collecting output wants the error in the same stream a person would have
    /// seen it in rather than appended to the rows in the file. It still sets
    /// `failed`.
    pub sink: Option<String>,
    /// How many result rows have been rendered since output was last sent
    /// somewhere with `redirect`.
    ///
    /// **So a command that redirects can say what it wrote.** `export --out`
    /// sends its rows to a file, which leaves it nothing to report from the
    /// text it collected; counting the lines back out of the file would have
    /// to know which of the eight formats writes a header, a separator rule or
    /// several lines to the row. The number the renderer was handed is the
    /// answer, and it costs one addition.
    pub rows_since_redirect: usize,
    /// The values `.parameter set` bound, by the name they were given.
    ///
    /// **The shell's own table, not the engine's.** SQLite keeps them in a
    /// `temp.sqlite_parameters` table and binds from it before each step; the
    /// visible behaviour is the same and this needs no reserved table name.
    /// Ordered by key, which is the order `.parameter list` prints and the
    /// order the reference prints.
    pub parameters: std::collections::BTreeMap<String, Value<'static>>,
    /// The single row inserts held to run as one statement; see the
    /// `deferred` module.
    deferred: Option<deferred::Deferred>,
    /// Which line of input is being run, counted up by each one, so a held
    /// statement knows whether the line it came from has ended.
    chunk_number: u64,
    /// How far the session has got into a dump header; see `dump_header`.
    header_stage: crate::dump_header::Stage,
    /// Whether the dump header turned defensive mode off, so the `COMMIT` turns it on again.
    header_lifted: bool,
    /// The FTS5 tables of a SQLite dump this session is restoring; see `sqlite_dump`.
    restoring: crate::sqlite_dump::Restoring,
}

impl Drop for Shell {
    /// Runs whatever is held, so no statement the shell accepted is lost
    /// when it goes away without reaching the end of its input.
    fn drop(&mut self) {
        self.run_held();
    }
}

/// Returns a count as an English ordinal: 1st, 2nd, 3rd, 4th, 11th, 21st.
///
/// The reference's `%r` format, which it uses to name a command line argument.
///
/// @param number - the position, counted from one
pub fn ordinal(number: usize) -> String {
    let suffix = match (number % 100, number % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{number}{suffix}")
}

/// Returns an error as a sentence, with its detail when it carries one.
///
/// @param error - the failure
fn described(error: inillucent_base::DbError) -> String {
    match error.detail() {
        // A detail that only repeats the message is said once: opening an
        // encrypted file without its key printed the same sentence twice.
        Some(detail) if detail != error.message() => format!("{}: {detail}", error.message()),
        _ => error.message().to_string(),
    }
}

/// The first rows of a statement and how many it produced in all.
pub struct Limited {
    /// The result column names.
    pub columns: Vec<String>,
    /// The rows kept, at most as many as were asked for.
    pub rows: Vec<Vec<Value<'static>>>,
    /// How many rows the statement produced, exactly.
    pub total: usize,
}

/// Why a statement did not produce rows.
pub struct Failure {
    /// What went wrong.
    pub message: String,
    /// Where in the statement, when the failure knows.
    pub offset: Option<u32>,
    /// Whether it failed to compile rather than while running.
    pub compiling: bool,
    /// The engine's own error, kept so a caller can classify it.
    ///
    /// **The message is not the classification.** The command layer
    /// has to tell a caller whether a statement was refused because the engine
    /// has not built the construct - the driver's `unsupported` - or because it
    /// was mistyped, and `drivers/README.md` argues at length for why folding
    /// those two together throws the design away. Only the `DbError` knows:
    /// `unsupported()` is a field on it, and the primary code separates a
    /// constraint from a busy file from corruption. Rendering it to a sentence
    /// here and matching on the sentence there would be a second, worse
    /// classifier beside the driver's.
    pub error: Option<inillucent_base::DbError>,
    /// The column names and rows a query produced before it failed, which the
    /// reference shell prints ahead of the error.
    pub partial: Option<(Vec<String>, Vec<Vec<Value<'static>>>)>,
}

impl Shell {
    /// Opens one database, with the modules and the flags a shell gives it.
    ///
    /// @param path - the file, or an in-memory name
    pub fn open_one(path: &str) -> Result<Opened, String> {
        Shell::open_one_as(path, false)
    }

    /// Opens one database, read only when the surface asked for it.
    ///
    /// @param path - the file, or an in-memory name
    /// @param read_only - whether this connection may write the file
    pub fn open_one_as(path: &str, read_only: bool) -> Result<Opened, String> {
        Shell::open_one_reporting(path, read_only).map_err(described)
    }

    /// [`Shell::open_one_as`], handing back the engine's own error.
    ///
    /// @param path - the file, or an in-memory name
    /// @param read_only - whether this connection may write the file
    pub fn open_one_reporting(
        path: &str,
        read_only: bool,
    ) -> Result<Opened, inillucent_base::DbError> {
        Shell::open_one_with(path, read_only, read_only)
    }

    /// Opens one database for a person at the shell, which follows other processes' commits.
    ///
    /// **The engine connection is opened for writing even when the person asked
    /// for read only, and the shell refuses the writes itself.** A connection the
    /// engine opened read only does not see what another process commits after
    /// the first statement: a long lived `inillucent-shell -readonly` kept
    /// answering from the state it first read, even across `BEGIN; COMMIT;`.
    /// A shell stays open and is read again and again, which is the case where
    /// that matters. `-readonly` was a flag the shell enforced on a connection
    /// opened for writing until the open was changed to ask the engine for read
    /// only, and `process_journal_handoff` caught the change. Only when the file
    /// cannot be opened for writing at all (a read only file or folder) does the
    /// connection fall back to the engine's own read only mode.
    ///
    /// @param path - the file, or an in-memory name
    /// @param read_only - whether the shell refuses statements that write
    pub fn open_one_following(
        path: &str,
        read_only: bool,
    ) -> Result<Opened, inillucent_base::DbError> {
        match Shell::open_one_with(path, read_only, false) {
            Err(_) if read_only => Shell::open_one_with(path, read_only, true),
            other => other,
        }
    }

    /// Opens one database with the engine connection read only or not, whatever the shell reports.
    ///
    /// @param path - the file, or an in-memory name
    /// @param read_only - whether the shell reports and enforces read only
    /// @param engine_read_only - whether the engine connection itself is read only
    fn open_one_with(
        path: &str,
        read_only: bool,
        engine_read_only: bool,
    ) -> Result<Opened, inillucent_base::DbError> {
        // **The detail, not only the code.** An open that fails with "bad
        // parameter or other API misuse" and nothing else is an error nobody
        // can act on; the detail says which part of the file could not be read.
        // **With the process's key, when it has one.** See `crate::keys`: the
        // key comes from `--key-file` or the environment, and every database
        // this shell opens is opened with it.
        let database = Database::open_keyed(
            path,
            inillucent_driver::PAGE_SIZE,
            inillucent_driver::DEFAULT_FRAMES,
            engine_read_only,
            crate::keys::for_opening(),
        )?;
        // **The shell adds `fsdir`, and the library does not.** A table-valued
        // function over the file system belongs to a program that asked for
        // one; the reference draws the same line, with `fsdir` in `shell.c`.
        for module in [
            std::sync::Arc::new(inillucent_driver::vtab::fsdir::FsDirModule)
                as std::sync::Arc<dyn inillucent_driver::vtab::Module>,
            std::sync::Arc::new(inillucent_driver::vtab::zipfile::ZipFileModule),
        ] {
            database.register_module(module)?;
        }
        let session = database.session().session();
        // **The reference's shell turns this on and this one has to as well.**
        // It is a connection flag rather than a shell one, so setting the field
        // below is not enough: the engine has to be told, or
        // `PRAGMA journal_mode = OFF` is honoured here and refused there.
        let _ = database.session_as(session).set_defensive(true);
        // **And turns this one off, for the same reason (task-1972).** The
        // library's default is on, which is SQLite's, and the reference's shell
        // turns it off at startup - so `.dbconfig` on the reference prints
        // `trusted_schema off` on a connection whose library default was on.
        // A shell is a program that opens files it did not write, which is the
        // case the flag exists for.
        let _ = database.session_as(session).set_trusted_schema(false);
        Ok(Opened {
            database,
            session,
            path: path.to_string(),
            read_only,
            label: None,
        })
    }

    /// Opens a shell on a database file, or on an in-memory one.
    pub fn open(path: &str) -> Result<Shell, String> {
        Shell::open_as(path, false)
    }

    /// Opens a shell on a database file, read only when the surface asked.
    ///
    /// @param path - the file, or an in-memory name
    /// @param read_only - whether this connection may write the file
    pub fn open_as(path: &str, read_only: bool) -> Result<Shell, String> {
        Shell::open_reporting(path, read_only).map_err(described)
    }

    /// Opens a shell, handing back the engine's own error.
    ///
    /// **So a caller can report the status the engine gave (task-1979, C6).**
    /// `open_as` folds the failure into a sentence, and the command surface
    /// then reported every open failure as `io` - including a file another
    /// process holds, which is `busy` and is the one an agent or a script can
    /// act on by retrying.
    ///
    /// @param path - the file, or an in-memory name
    /// @param read_only - whether this connection may write the file
    pub fn open_reporting(path: &str, read_only: bool) -> Result<Shell, inillucent_base::DbError> {
        let first = Shell::open_one_reporting(path, read_only)?;
        Ok(Shell::around(first))
    }

    /// Opens a shell for a person at the program, which follows other processes' commits.
    ///
    /// See [`Shell::open_one_following`] for why `read_only` is enforced by the
    /// shell and not by the engine connection.
    ///
    /// @param path - the file, or an in-memory name
    /// @param read_only - whether statements that write are refused
    pub fn open_following(path: &str, read_only: bool) -> Result<Shell, String> {
        let first = Shell::open_one_following(path, read_only).map_err(described)?;
        Ok(Shell::around(first))
    }

    /// Builds a shell around its first open database.
    ///
    /// @param first - the database statements run on
    fn around(mut first: Opened) -> Shell {
        // The database the program started on was named, `:memory:` when none was.
        if first.path == ":memory:" {
            first.label = Some(":memory:".to_string());
        }
        let mut connections: Vec<Option<Opened>> = (0..CONNECTIONS).map(|_| None).collect();
        if let Some(slot) = connections.first_mut() {
            *slot = Some(first);
        }
        Shell {
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            connections,
            active: 0,
            layout: Layout::default(),
            headers_chosen: None,
            header_when_empty: false,
            redirection: None,
            bail: false,
            echo: false,
            timer: false,
            timer_once: false,
            eqp_name: "off",
            mode_once: None,
            mode_tags: std::collections::HashMap::new(),
            statement_ran: false,
            dot_line: String::new(),
            dot_offsets: Vec::new(),
            stats: false,
            show_changes: false,
            explain_plan: false,
            crlf: false,
            prompt_main: "sqlite> ".to_string(),
            prompt_continue: "   ...> ".to_string(),
            explain_mode: crate::commands::ExplainMode::Auto,
            nonce: None,
            testcase: None,
            captured: String::new(),
            tests_run: 0,
            tests_failed: 0,
            viewer: None,
            auth: false,
            authorized: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            trace: None,
            scanstats: "off".to_string(),
            defensive: true,
            done: false,
            exit_code: None,
            source: None,
            argument: None,
            depth: 0,
            failed: false,
            holding: false,
            held_error: None,
            chunk_stop: false,
            excerpt: None,
            first_error: None,
            log_to: None,
            progress_interval: 0,
            progress_limit: 0,
            progress_once: false,
            progress_quiet: false,
            progress_pending_limit: false,
            parameters: std::collections::BTreeMap::new(),
            readonly: false,
            safe: false,
            no_programs: false,
            sink: None,
            rows_since_redirect: 0,
            line: 1,
            deferred: None,
            chunk_number: 0,
            header_stage: crate::dump_header::Stage::Start,
            header_lifted: false,
            restoring: crate::sqlite_dump::Restoring::default(),
        }
    }

    /// Returns the connection statements run on.
    ///
    /// Always the same session, so a temporary object made by one statement is
    /// there for the next one.
    pub fn connection(&self) -> Connection<'_> {
        let held = self.open_slot();
        held.database.session_as(held.session)
    }

    /// Makes the open database's commits durable by the fold that follows each
    /// statement, for a program that runs one statement and exits.
    ///
    /// See the engine's `hold_commits_for_the_fold` on its `Database`.
    pub fn hold_commits_for_the_fold(&self) -> inillucent_base::DbResult<()> {
        self.open_slot().database.hold_commits_for_the_fold()
    }

    /// Returns what one run-time limit is set to on the open database.
    ///
    /// @param limit - which limit
    pub fn limit(&self, limit: inillucent_base::limits::Limit) -> i64 {
        self.open_slot().database.limit(limit)
    }

    /// Sets one run-time limit on the open database, returning its old value.
    ///
    /// @param limit - which limit
    /// @param requested - the value asked for
    pub fn set_limit(&mut self, limit: inillucent_base::limits::Limit, requested: i64) -> i64 {
        self.open_slot().database.set_limit(limit, requested)
    }

    /// Returns whether a boolean pragma reads on.
    ///
    /// @param name - the pragma's name
    pub fn boolean_pragma(&self, name: &str) -> bool {
        self.column(&format!("PRAGMA {name};"))
            .first()
            .is_some_and(|value| value == "1")
    }

    /// Sets a boolean pragma, reporting whether the engine took it.
    ///
    /// @param name - the pragma's name
    /// @param value - what to set it to
    pub fn set_boolean_pragma(&mut self, name: &str, value: bool) -> bool {
        let word = if value { "on" } else { "off" };
        self.collect(&format!("PRAGMA {name} = {word};")).is_ok()
            && self.boolean_pragma(name) == value
    }

    /// Installs or removes the authorizer that `.auth on` prints through.
    ///
    /// @param on - whether the decisions are watched
    pub fn set_authorizer(&mut self, on: bool) {
        let installed: Option<std::rc::Rc<dyn inillucent_driver::Authorizer>> = on.then(|| {
            std::rc::Rc::new(crate::commands::Watching {
                seen: std::rc::Rc::clone(&self.authorized),
            }) as std::rc::Rc<dyn inillucent_driver::Authorizer>
        });
        let _ = self.connection().set_authorizer(installed);
    }

    /// Prints and clears whatever the authorizer recorded.
    fn report_authorized(&mut self) {
        let lines: Vec<String> = self.authorized.borrow_mut().drain(..).collect();
        for line in lines {
            self.say(&line);
        }
    }

    /// Puts the connection into or out of defensive mode.
    ///
    /// @param on - whether the flag is in force
    pub fn set_defensive(&mut self, on: bool) -> bool {
        let _ = self.connection().set_defensive(on);
        true
    }

    /// Returns what the page cache has been asked to do.
    pub fn cache_stats(&self) -> inillucent_driver::CacheStats {
        self.open_slot().database.cache_stats()
    }

    /// Returns how many bytes the page cache is holding.
    pub fn pool_bytes(&self) -> usize {
        self.open_slot().database.pool_bytes()
    }

    /// Copies the open database into a file and checks the copy.
    ///
    /// @param path - where the copy goes
    pub fn backup_to(&self, path: &str) -> Result<(), inillucent_base::DbError> {
        self.open_slot().database.backup_to(path)
    }

    /// Copies a database file that statements are not running on.
    ///
    /// **For `.backup SCHEMA FILE` where `SCHEMA` is an attached file.** The engine's
    /// own backup copies the file the connection was opened on, which is `main`; an
    /// attached database is another file, and is opened here a second time to be
    /// checkpointed and copied the way any file is.
    ///
    /// @param source - the attached database's file
    /// @param destination - where the copy goes
    pub fn backup_file(source: &str, destination: &str) -> Result<(), inillucent_base::DbError> {
        let other = Database::open_keyed(
            source,
            inillucent_driver::PAGE_SIZE,
            inillucent_driver::DEFAULT_FRAMES,
            false,
            crate::keys::for_opening(),
        )?;
        other.backup_to(destination)
    }

    /// Returns every database the connection has, as its name and its file.
    ///
    /// The file is empty for `temp` and for a database held in memory. `main` of
    /// a shell opened on `:memory:` reports `:memory:`.
    pub fn database_list(&self) -> Vec<(String, String)> {
        let Ok((_, rows)) = self.collect("PRAGMA database_list") else {
            return Vec::new();
        };
        let text = |value: Option<&Value<'static>>| match value {
            Some(Value::Text(text)) => crate::raw::decode(text.raw()),
            _ => String::new(),
        };
        rows.iter()
            .map(|row| (text(row.get(1)), text(row.get(2))))
            .collect()
    }

    /// Writes a copy of the database, encrypted with `key` or in plaintext.
    ///
    /// @param path - where the copy goes
    /// @param key - the key the copy is encrypted with, if any
    pub fn export_to(
        &self,
        path: &str,
        key: Option<inillucent_driver::EncryptionKey>,
    ) -> Result<(), inillucent_base::DbError> {
        self.open_slot().database.export_to(path, key)
    }

    /// Reports whether the database is encrypted.
    pub fn is_encrypted(&self) -> bool {
        self.open_slot().database.is_encrypted()
    }

    /// Changes the key the database is encrypted with.
    ///
    /// @param key - the new key
    pub fn rekey(
        &self,
        key: inillucent_driver::EncryptionKey,
    ) -> Result<(), inillucent_base::DbError> {
        self.open_slot().database.rekey(key)
    }

    /// Returns where the database was opened from.
    pub fn path(&self) -> &str {
        &self.open_slot().path
    }

    /// Returns the database statements currently run on.
    ///
    /// The active slot is never closed: `.connection close` on it moves the
    /// shell back to slot zero, and slot zero is opened before the shell is.
    ///
    /// **The `expect` is the invariant, and the invariant is enforced twice.**
    /// `Shell::open` fills slot zero before the shell exists, and
    /// `.connection close` on the active slot moves back to slot zero rather
    /// than closing it. The crate denies `expect_used` because a shell that
    /// panics on a caller's SQL is unusable; this is not that - reaching it
    /// would mean the shell had been constructed without a database, which no
    /// path does.
    #[allow(clippy::expect_used)]
    fn open_slot(&self) -> &Opened {
        self.connections
            .get(self.active)
            .and_then(|held| held.as_ref())
            .or_else(|| self.connections.first().and_then(|held| held.as_ref()))
            .expect("the shell always holds one open database")
    }

    /// Returns what opening the active database did to it.
    ///
    /// See `inillucent_driver::Recovery`; the caller decides whether to report
    /// it, which for the command surface is "only when it says something
    /// happened".
    pub fn recovery(&self) -> inillucent_driver::Recovery {
        let report = self.open_slot().database.recovery_report();
        inillucent_driver::Recovery {
            recovered: report.recovered,
            scanned: report.scanned,
            applied: report.applied,
            dropped: report.dropped,
            committed: report.committed,
            losers: report.losers,
            last_sequence: report.last_sequence,
            last_lsn: report.last_lsn,
            set_aside: report.set_aside,
        }
    }

    /// Returns which segment of its log the active database is writing.
    pub fn log_sequence(&self) -> u64 {
        self.open_slot().database.log_sequence()
    }

    /// Returns which slot statements run on.
    pub fn active(&self) -> usize {
        self.active
    }

    /// Returns each slot and the name `.connection` shows for it.
    ///
    /// **The names are the reference's** (bug hunt 10): the path of a file, the
    /// label the database was opened under when it has one (`:memory:`,
    /// `(temporary-file)`), and `(memory)` for an in-memory database opened with
    /// no name, which `.connection` printed for all three.
    pub fn slots(&self) -> Vec<Option<String>> {
        self.connections
            .iter()
            .map(|held| {
                held.as_ref()
                    .map(|open| match (&open.label, open.path.as_str()) {
                        (Some(label), _) => label.clone(),
                        (None, ":memory:") => "(memory)".to_string(),
                        (None, path) => path.to_string(),
                    })
            })
            .collect()
    }

    /// Sets how `.connection` names the active database.
    ///
    /// @param label - the name to show, or `None` for the in-memory form with no name
    pub fn label_active(&mut self, label: Option<String>) {
        let active = self.active;
        if let Some(Some(open)) = self.connections.get_mut(active) {
            open.label = label;
        }
    }

    /// Switches to one slot, opening an in-memory database if it is closed.
    ///
    /// Out of range is ignored, which is what the reference does with it.
    ///
    /// @param slot - which connection to run statements on
    pub fn use_slot(&mut self, slot: usize) -> Result<(), String> {
        if slot >= CONNECTIONS {
            return Ok(());
        }
        if self.connections.get(slot).is_some_and(Option::is_none) {
            let opened = Shell::open_one(":memory:")?;
            if let Some(place) = self.connections.get_mut(slot) {
                *place = Some(opened);
            }
        }
        self.active = slot;
        Ok(())
    }

    /// Closes one slot, moving back to slot zero if it was the active one.
    ///
    /// Slot zero is never closed: it is the database the shell was started on,
    /// and a shell with nothing open has nothing to run a statement against.
    ///
    /// @param slot - which connection to close
    pub fn close_slot(&mut self, slot: usize) {
        if slot == 0 || slot >= CONNECTIONS {
            return;
        }
        if let Some(place) = self.connections.get_mut(slot) {
            *place = None;
        }
        if self.active == slot {
            self.active = 0;
        }
    }

    /// Closes the current database and opens another.
    pub fn reopen(&mut self, path: &str) -> Result<(), String> {
        self.reopen_as(path, false)
    }

    /// Closes the current database and opens another, read only when asked.
    ///
    /// @param path - the file, or an in-memory name
    /// @param read_only - whether the file is opened read only
    pub fn reopen_as(&mut self, path: &str, read_only: bool) -> Result<(), String> {
        let replacement = Shell::open_one_following(path, read_only).map_err(described)?;
        let active = self.active;
        if let Some(place) = self.connections.get_mut(active) {
            *place = Some(replacement);
        }
        Ok(())
    }

    /// Returns whether the database statements run on was opened read only.
    pub fn is_read_only(&self) -> bool {
        self.open_slot().read_only
    }

    /// Returns the layout to render with, told where its lines are going.
    ///
    /// The only caller is `run`, and it is a method rather than two lines
    /// there because the three destinations `say` chooses between are the
    /// three this has to agree with. They disagreeing is how `csv` came to
    /// write a carriage return too many into a file.
    fn rendering_layout(&self) -> crate::render::Layout {
        let mut layout = self.layout.clone();
        layout.to_stdout =
            self.redirection.is_none() && self.sink.is_none() && self.testcase.is_none();
        layout
    }

    /// Prints one line to wherever output is currently going.
    pub fn say(&mut self, line: &str) {
        self.emit(line, true);
    }

    /// Prints rows the renderer laid out, ending each as the layout says.
    ///
    /// **A row separator without a newline is the line's whole ending.** `-newline X`
    /// and `.mode ascii` write the separator after each row and nothing else, so
    /// a newline written here as well would be one the reference does not write.
    ///
    /// @param lines - the rendered lines
    /// @param layout - the settings they were rendered with
    fn print_rendered(&mut self, lines: Vec<String>, layout: &Layout) {
        let terminated = layout.lines_end_in_newline();
        for line in lines {
            self.emit(&line, terminated);
        }
    }

    /// Prints one line, with or without the newline after it.
    ///
    /// @param line - the text
    /// @param terminated - whether a newline follows it
    fn emit(&mut self, line: &str, terminated: bool) {
        let captured = self.testcase.is_some().then_some(&mut self.captured);
        stream::deliver(
            &mut self.redirection,
            &mut self.sink,
            captured,
            self.crlf,
            line,
            terminated,
        );
    }

    /// Prints an error, which always goes to standard error.
    ///
    /// Unless a caller is collecting output, in which case it goes there: a
    /// command run through the MCP server has no standard error anybody will
    /// ever read, and an error that vanished would be worse than one printed
    /// among the rows.
    pub fn complain(&mut self, message: &str) {
        match self.sink.as_mut() {
            Some(sink) => {
                sink.push_str(message);
                sink.push('\n');
            }
            None => {
                // Everything printed before the error reaches the terminal or
                // the pipe before it, as it did when every line was flushed.
                output::write_error_line(message);
            }
        }
        self.failed = true;
    }

    /// Prints a line to standard error without marking the run as failed.
    ///
    /// For what the reference prints and carries on after: a word that is not a
    /// boolean, the state `.crlf` reports, a warning about a row of a file.
    ///
    /// @param message - the line
    pub fn note(&mut self, message: &str) {
        let failed = self.failed;
        self.complain(message);
        self.failed = failed;
    }

    /// Refuses a command that safe mode does not allow, and says which it was.
    ///
    /// Returns whether the caller may go on. A matching `.nonce` has already
    /// cleared safe mode for this command by the time this is asked, because
    /// that is what `.nonce` does.
    ///
    /// @param command - the dot command being attempted, leading dot included
    pub fn unsafe_refused(&mut self, command: &str) -> bool {
        if !self.safe {
            return false;
        }
        self.complain(&format!("Error: {command} is prohibited in safe mode"));
        true
    }

    /// Sends output to a file, or back to standard output when `path` is none.
    ///
    /// @param path - the file to write, or none to go back to standard output
    /// @param once - whether the redirect ends after the next SQL statement
    pub fn redirect(&mut self, path: Option<&str>, once: bool) -> Result<(), String> {
        let Some(path) = path else {
            self.stop_redirection();
            return Ok(());
        };
        let file = std::fs::File::create(path).map_err(|error| error.to_string())?;
        self.start_redirection(redirect::Destination::File(file), path, once, false);
        Ok(())
    }

    /// Ends whatever `.output` or `.once` set up, and goes back to standard output.
    pub fn stop_redirection(&mut self) {
        self.rows_since_redirect = 0;
        if let Some(redirection) = self.redirection.take() {
            redirection.close();
        }
    }

    /// Sends output to a destination, ending the redirection that was in force.
    ///
    /// @param destination - where the bytes go
    /// @param name - what `.show` calls it
    /// @param once - whether it ends after the next command
    /// @param bom - whether a byte order mark is written first
    pub fn start_redirection(
        &mut self,
        destination: redirect::Destination,
        name: &str,
        once: bool,
        bom: bool,
    ) {
        self.stop_redirection();
        let mut redirection = redirect::Redirection {
            destination,
            name: name.to_string(),
            once,
            arming: once,
        };
        if bom {
            redirection.write(&[0xEF, 0xBB, 0xBF]);
        }
        self.redirection = Some(redirection);
    }

    /// Returns what a `.output --show` prints: the text kept for `memory`.
    pub fn remembered_output(&self) -> Option<String> {
        match self.redirection.as_ref().map(|held| &held.destination) {
            Some(redirect::Destination::Memory(bytes)) => Some(crate::raw::decode(bytes)),
            _ => None,
        }
    }

    /// Where output is going, as `.show` names it.
    ///
    /// `stdout` when nothing is redirecting, and the file name when `.output`
    /// or `.once` is.
    pub fn output_target(&self) -> &str {
        self.redirection
            .as_ref()
            .map_or("stdout", |held| held.name.as_str())
    }

    /// Called when a command, a dot command or a line of SQL, has finished.
    ///
    /// **A `.once` lasts for one command.** The reference ends it after the next
    /// line of input whatever the line was, so `.once out.txt` then `.mode csv`
    /// leaves the file empty and the statement after it prints to the terminal,
    /// and a line holding two statements sends both to the file. The `.once` that
    /// set the redirection is not the command it applies to.
    pub fn end_command(&mut self) {
        // **`.mode --once` lasts for one statement.** The settings it changed come
        // back once a statement has run, not after a dot command.
        if self.statement_ran {
            self.statement_ran = false;
            if let Some(saved) = self.mode_once.take() {
                self.layout = saved.layout;
                self.headers_chosen = saved.headers_chosen;
            }
        }
        let ending = match self.redirection.as_mut() {
            Some(held) if held.once && held.arming => {
                held.arming = false;
                false
            }
            Some(held) => held.once,
            None => false,
        };
        if ending {
            self.stop_redirection();
        }
        // `.excel` and `.www` hand the file to whatever the system opens that
        // kind with, and only once it is closed and complete.
        if self.redirection.is_none() {
            if let Some(path) = self.viewer.take() {
                crate::commands::open_viewer(&path);
            }
        }
    }

    /// Returns a handle to the flag that stops the running statement.
    ///
    /// A front end registers it with `interrupt::stop_on_ctrl_c`; a program
    /// embedding the shell can set it from any thread.
    pub fn cancel_flag(&self) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        std::sync::Arc::clone(&self.cancel)
    }

    /// Runs one complete statement and prints whatever it produced.
    ///
    /// **A single row `INSERT` may be held and run with the ones after it**
    /// (task-2191), as one statement, before anything that could see it; see
    /// the `deferred` module for when and why the output is the same.
    pub fn run(&mut self, sql: &str) {
        self.statement_ran = true;
        self.watch_dump_header(sql);
        let mut restoring = std::mem::take(&mut self.restoring);
        let rewrite = restoring.rewrite(self, sql);
        self.restoring = restoring;
        match rewrite {
            crate::sqlite_dump::Rewrite::Keep => self.run_one(sql),
            crate::sqlite_dump::Rewrite::Replace(statements) => {
                for statement in statements {
                    self.run_one(&statement);
                    if self.done || self.chunk_stop || (self.bail && self.failed) {
                        break;
                    }
                }
            }
        }
    }

    /// Runs one statement the shell has settled on, holding it if it can join the ones held.
    ///
    /// @param sql - the statement
    fn run_one(&mut self, sql: &str) {
        if self.hold_if_it_joins(sql) {
            return;
        }
        let failed_before = self.failed;
        self.run_held();
        // A held statement that failed stops what follows it the way it would
        // have stopped it had it run when it arrived.
        if self.chunk_stop || (self.bail && self.failed && !failed_before) {
            return;
        }
        self.run_as_written(sql);
    }

    /// Reports whether the open database holds no tables, indexes, views or triggers yet.
    ///
    /// The reference's shell lifts defensive mode for a dump header only when it loads into an
    /// empty database; measured on 3.53.4, the same header into a database that holds one table
    /// leaves `CREATE TABLE IF NOT EXISTS sqlite_sequence` refused.
    fn schema_is_empty(&self) -> bool {
        self.collect("SELECT 1 FROM sqlite_schema LIMIT 1")
            .is_ok_and(|(_, rows)| rows.is_empty())
    }

    /// Lifts defensive mode for the transaction a dump opens, as the reference's shell does.
    ///
    /// Called before a statement is held or run, so a held `INSERT` still counts as a statement
    /// and ends the chance of a header.
    ///
    /// @param sql - the statement that is about to run
    fn watch_dump_header(&mut self, sql: &str) {
        use crate::dump_header::{advance, Change};
        let (stage, change) = advance(self.header_stage, sql);
        self.header_stage = stage;
        match change {
            Change::Lift if self.defensive && self.schema_is_empty() => {
                self.header_lifted = true;
                self.defensive = false;
                self.set_defensive(false);
            }
            Change::Restore if self.header_lifted => {
                self.header_lifted = false;
                self.defensive = true;
                self.set_defensive(true);
            }
            _ => {}
        }
    }

    /// Runs one complete statement now and prints whatever it produced.
    ///
    /// @param sql - the statement
    fn run_as_written(&mut self, sql: &str) {
        if (self.readonly || self.is_read_only()) && self.writes(sql) {
            // Reported as the reference reports it, with the line the statement
            // began on, because that is the error a script's author searches for.
            let failure = Failure {
                message: "attempt to write a readonly database".to_string(),
                offset: None,
                compiling: false,
                error: None,
                partial: None,
            };
            self.report(sql, &failure);
            return;
        }
        if self.echo {
            let text = sql.to_string();
            self.say(&text);
        }
        if let Some(target) = self.trace.clone() {
            crate::commands::trace_statement(self, &target, sql);
        }
        let started = std::time::Instant::now();
        if self.explain_plan {
            self.print_plan(sql);
        }
        // Armed for this statement and dropped after it, so a Ctrl+C that
        // arrives between two statements belongs to the one that finished and
        // is cleared rather than applied to the one that has not started.
        let armed = inillucent_driver::arm(
            inillucent_driver::StatementLimits::unbounded(),
            std::sync::Arc::clone(&self.cancel),
        );
        if self.streams_rows(sql) {
            let outcome = self.print_streamed(sql);
            drop(armed);
            match outcome {
                Err(failure) => self.report(sql, &failure),
                Ok(printed) => {
                    self.rows_since_redirect = self.rows_since_redirect.saturating_add(printed);
                    self.print_changes();
                }
            }
            self.after_statement(started);
            return;
        }
        let outcome = self.collect(sql);
        drop(armed);
        // `.auth on` prints what the binder asked about, before the rows the
        // statement produced - which is the order the reference prints them in.
        if self.auth {
            self.report_authorized();
        }
        match outcome {
            Err(failure) => {
                if let Some((columns, rows)) = &failure.partial {
                    let layout = self.rendering_layout();
                    let lines = render(&layout, columns, rows);
                    self.print_rendered(lines, &layout);
                }
                self.report(sql, &failure);
            }
            Ok((columns, rows)) => {
                // Counted here rather than beside the one `render` call below,
                // because the two branches that follow print rows and return
                // without reaching it - and a count that is right for six of
                // the eight formats and silently zero for a plan is the kind
                // of number a caller stops checking.
                self.rows_since_redirect = self.rows_since_redirect.saturating_add(rows.len());
                // **A statement with no result columns has nothing to draw**
                // (task-2191): an `INSERT` asked whether it was a plan, an
                // `EXPLAIN` and a table layout, and built the layout, for every
                // statement of a script.
                let nothing_to_draw = columns.is_empty() && rows.is_empty();
                // **`EXPLAIN QUERY PLAN` is drawn, not listed.** Its four
                // columns are a tree, and the reference's shell renders them as
                // one; printing `0|0|0|SCAN t` is the raw result of a statement
                // nobody writes for the raw result.
                if !nothing_to_draw && is_query_plan(sql) {
                    for line in plan_tree(&rows) {
                        self.say(&line);
                    }
                    return;
                }
                // **And the bytecode form is a table with fixed columns.** The
                // reference's shell switches to its own `MODE_Explain` for an
                // `EXPLAIN` whatever `.mode` says, because eight columns of
                // opcode printed as `0|Init|0|1|0||0|Start at 1` is unreadable.
                // The widths are the reference's own.
                let as_table = !nothing_to_draw
                    && match self.explain_mode {
                        crate::commands::ExplainMode::Auto => is_bytecode_explain(sql),
                        crate::commands::ExplainMode::On => true,
                        crate::commands::ExplainMode::Off => false,
                    };
                if as_table && columns.len() == EXPLAIN_WIDTHS.len() {
                    for line in explain_table(&columns, &rows) {
                        self.say(&line);
                    }
                    return;
                }
                // A statement that returned no rows prints nothing, headers
                // included: SQLite's shell prints the header from its per-row
                // callback. Printing it here put an empty line after every
                // `CREATE` and `INSERT` once `.headers on` was set. `export`
                // renders through the same function and keeps its header.
                let layout = self.rendering_layout();
                let lines = if rows.is_empty() && !self.header_when_empty {
                    Vec::new()
                } else {
                    render(&layout, &columns, &rows)
                };
                self.print_rendered(lines, &layout);
                self.print_changes();
            }
        }
        self.after_statement(started);
    }

    /// Prints the change counters after a statement, for `.changes on`.
    fn print_changes(&mut self) {
        if self.show_changes {
            let changes = self.connection().changes().unwrap_or_default();
            // The reference prints both counters, aligned with three
            // spaces between them.
            let total = self.connection().total_changes().unwrap_or_default();
            self.say(&format!("changes: {changes}   total_changes: {total}"));
        }
    }

    /// Prints what `.stats` and `.timer` ask for after a statement.
    ///
    /// @param started - when the statement began
    fn after_statement(&mut self, started: std::time::Instant) {
        if self.stats {
            for line in crate::diagnose::statistics(self) {
                self.say(&line);
            }
        }
        if self.timer {
            let elapsed = started.elapsed();
            self.say(&format!(
                "Run Time: real {:.6} user 0.000000 sys 0.000000",
                elapsed.as_secs_f64()
            ));
            if self.timer_once {
                self.timer = false;
                self.timer_once = false;
            }
        }
    }

    /// Returns the location a dot command's usage error starts with.
    ///
    /// **A dot command given as a command line argument is `argv[N]:`, and one
    /// read from input is `line N:`** (bug hunt 10). The reference prints
    /// `argv[2]: .mode --bogus` for the first, and these messages said
    /// `line 1:` for both, so a script that matched the location found nothing.
    /// `argument` is the position counted from the program name, which is the
    /// number the reference prints.
    pub fn error_location(&self) -> String {
        match self.argument {
            Some(index) => format!("argv[{index}]:"),
            None => format!("line {}:", self.line),
        }
    }

    /// Prints a failure the way the reference prints it.
    ///
    /// The caret block only appears when the failure knows where it happened,
    /// which is the same rule the reference follows: `no such table` has no
    /// position and `no such column` does.
    fn report(&mut self, sql: &str, failure: &Failure) {
        if self.first_error.is_none() {
            self.first_error = failure.error.clone();
        }
        let kind = if failure.compiling {
            "Parse error"
        } else {
            "Error"
        };
        let place = match (&self.argument, &self.source) {
            (Some(index), _) => format!("in {} command line argument", ordinal(*index)),
            (None, Some(file)) => format!("near line {} of {file}", self.line),
            (None, None) => format!("near line {}", self.line),
        };
        let heading = format!("{kind} {place}: {}", failure.message);
        let mut lines = vec![heading];
        if let Some(offset) = failure.offset {
            let shown = self.excerpt.as_ref().map_or(sql, Excerpt::text);
            lines.extend(error_context(shown.as_bytes(), offset as usize));
        }
        self.print_or_hold(lines, failure.compiling);
    }

    /// Prints the lines of an error, or holds them until the chunk of input ends.
    ///
    /// **A statement that fails while running does not stop its chunk, and only the last
    /// of those errors is printed.** The reference runs every statement of a line, keeps
    /// the message of each failure in one slot that the next failure overwrites, and prints
    /// the slot when the line is done. A statement that fails to compile ends the line, and
    /// its message replaces whatever was held. So
    /// `INSERT ...(duplicate); INSERT ...(fine); SELECT ...;` on one line runs all three and
    /// prints one error, and a typo in the second statement hides the first's error and
    /// skips the third statement.
    ///
    /// @param lines - the heading and the statement excerpt lines
    /// @param compiling - whether the statement failed to compile
    fn print_or_hold(&mut self, lines: Vec<String>, compiling: bool) {
        if self.holding && !compiling {
            self.held_error = Some(lines);
            self.failed = true;
            return;
        }
        if self.holding {
            self.held_error = None;
            self.chunk_stop = true;
        }
        for line in lines {
            self.complain(&line);
        }
    }

    /// Runs a statement and collects its column names and rows.
    pub fn collect(&self, sql: &str) -> Result<(Vec<String>, Vec<Vec<Value<'static>>>), Failure> {
        self.collect_bound(sql, &[])
    }

    /// Returns the message for a value that could not be bound, which says how
    /// many values the statement takes when the reason is that it takes fewer.
    ///
    /// **The engine's own message for this is "column index out of range",
    /// which names neither number.** A caller who passed three values to a
    /// statement with two markers was told nothing about which of the two was
    /// wrong. The count of markers is asked of the statement itself. The first
    /// words are unchanged, so a program that matches on them still does.
    ///
    /// @param connection - the connection the statement was prepared on
    /// @param sql - the statement
    /// @param given - how many values the caller passed
    /// @param error - the engine's refusal
    fn too_many_parameters(
        &self,
        connection: &Connection<'_>,
        sql: &str,
        given: usize,
        error: &inillucent_base::DbError,
    ) -> String {
        let said = reason(error);
        if error.code() != inillucent_base::PrimaryCode::Range {
            return said;
        }
        match connection.parameter_count(sql) {
            Ok(takes) => format!(
                "{said}: the statement takes {takes} parameter{}, and {given} {} given",
                if takes == 1 { "" } else { "s" },
                if given == 1 { "was" } else { "were" }
            ),
            Err(_) => said,
        }
    }

    /// Compiles a statement and binds the values it takes: the positional ones first, then the
    /// ones `.parameter set` named.
    ///
    /// Shared by the call that collects a statement's rows and the call that prints them as they
    /// are produced, so both bind the same values the same way.
    ///
    /// @param connection - the connection to prepare the statement on
    /// @param sql - the statement
    /// @param bound - the values for `?1`, `?2`, ... in order
    fn prepare_bound<'c>(
        &self,
        connection: &'c Connection<'_>,
        sql: &str,
        bound: &[OwnedDatum],
    ) -> Result<Statement<'c>, Failure> {
        let mut statement = connection.prepare(sql).map_err(|error| Failure {
            message: reason(&error),
            offset: error.sql_offset(),
            compiling: true,
            error: Some(error),
            partial: None,
        })?;
        for (nth, value) in bound.iter().enumerate() {
            // The parser numbers markers from one, and a caller that passed
            // more values than the statement has markers is told so rather than
            // having the extras dropped: a query that silently ignored an
            // argument is a query answering a different question.
            statement
                .bind(nth as u32 + 1, value.clone())
                .map_err(|error| Failure {
                    message: self.too_many_parameters(connection, sql, bound.len(), &error),
                    offset: None,
                    compiling: true,
                    error: Some(error),
                    partial: None,
                })?;
        }
        // **What `.parameter set` bound, applied by name.** A statement that
        // names none of them binds nothing; a name the statement does not use
        // is not an error, which is what makes a set of parameters reusable
        // across a script.
        if !self.parameters.is_empty() {
            let names = connection.parameter_names(sql).unwrap_or_default();
            // A parameter number has one name, the first the statement wrote for it. In
            // `SELECT :a, ?1` both are number 1 and the name is `:a`, so a value set for `?1`
            // is never looked up, as in the reference shell.
            let mut named: Vec<u32> = Vec::new();
            for (name, index) in names {
                if named.contains(&index) {
                    continue;
                }
                named.push(index);
                let key = String::from_utf8_lossy(&name).into_owned();
                let Some(value) = self.parameters.get(&key) else {
                    continue;
                };
                let _ = statement.bind(index, OwnedDatum::from(value));
            }
        }
        Ok(statement)
    }

    /// Runs a statement with values bound by position, and collects its rows.
    ///
    /// **By position, because a caller that is a program has no names.** The
    /// shell's own `.parameter` table binds `:name` and `@name` markers, which
    /// is what a person typing a script wants; a command arriving over MCP or
    /// off a command line carries an ordered array and means `?1`, `?2`, ... .
    /// Going through the named table for those was the first thing tried,
    /// and it bound nothing at all: the engine reports a numbered marker
    /// under a name that is not the text `?1`, so every lookup missed and every
    /// value silently arrived as NULL. Binding by the index the parser assigned
    /// cannot miss.
    ///
    /// Both mechanisms apply: positional values are bound first and the named
    /// table after, so a script that sets `:limit` once and passes `?1` per
    /// call gets both.
    ///
    /// @param sql - the statement
    /// @param bound - the values for `?1`, `?2`, ... in order
    pub fn collect_bound(
        &self,
        sql: &str,
        bound: &[OwnedDatum],
    ) -> Result<(Vec<String>, Vec<Vec<Value<'static>>>), Failure> {
        let connection = self.connection();
        let mut statement = self.prepare_bound(&connection, sql, bound)?;
        let mut rows = Vec::new();
        loop {
            match statement.step() {
                Err(error) => {
                    // The rows that came before the failure are printed ahead
                    // of the error, as the reference shell does.
                    let partial = (!rows.is_empty()).then(|| (statement.columns().to_vec(), rows));
                    return Err(Failure {
                        message: reason(&error),
                        offset: None,
                        compiling: false,
                        error: Some(error),
                        partial,
                    });
                }
                Ok(false) => break,
                Ok(true) => match owned_row_values(statement.row()) {
                    Ok(row) => rows.push(row),
                    Err(error) => {
                        return Err(Failure {
                            message: reason(&error),
                            offset: None,
                            compiling: false,
                            error: Some(error),
                            partial: None,
                        })
                    }
                },
            }
        }
        // Read *after* stepping. The engine's statement materialises on its
        // first step, so it does not know its column names until it has run -
        // where `sqlite3_column_name` answers straight after a prepare. Asking
        // first returned an empty list, and `.headers on` printed nothing.
        let columns: Vec<String> = statement.columns().to_vec();
        Ok((columns, rows))
    }

    /// Runs a statement with values bound by position, keeps its first rows and counts the rest.
    ///
    /// **The statement is streamed, so the rows past `keep` are never held.** `total` is exact:
    /// it is counted as the rows go by, which is what a caller that shows `(200 of 3000000
    /// rows)` needs and what `collect_bound` gave by holding every row to count them.
    ///
    /// @param sql - the statement
    /// @param bound - the values for `?1`, `?2`, ... in order
    /// @param keep - how many rows to keep
    pub fn collect_limited(
        &self,
        sql: &str,
        bound: &[OwnedDatum],
        keep: usize,
    ) -> Result<Limited, Failure> {
        let connection = self.connection();
        let mut statement = self.prepare_bound(&connection, sql, bound)?;
        let limiter = std::rc::Rc::new(std::cell::RefCell::new(stream::Limiter::new(keep)));
        let ended = statement.stream(&limiter).map_err(|error| Failure {
            message: reason(&error),
            offset: None,
            compiling: false,
            error: Some(error),
            partial: None,
        })?;
        let rows = std::mem::take(&mut limiter.borrow_mut().rows);
        Ok(Limited {
            columns: ended.names.to_vec(),
            rows,
            total: ended.rows,
        })
    }

    /// Reports whether a statement's rows are printed as they are produced.
    ///
    /// **Only where nothing needs the whole result before the first line.** The modes that size
    /// their columns to the widest value need every row, and so do the two layouts that are drawn
    /// from the whole result: `EXPLAIN QUERY PLAN`, which is a tree, and the bytecode listing of
    /// `EXPLAIN`, which is a table. `.explain on` draws any statement with eight columns as that
    /// table, and the column count is not known until the statement is compiled, so it keeps
    /// every statement on the path that collects.
    ///
    /// @param sql - the statement
    fn streams_rows(&self, sql: &str) -> bool {
        // `.auth on` reports what the binder asked about before the rows, and a streamed
        // statement prints its rows while it runs.
        crate::render::stream::streams(self.layout.mode)
            && !self.auth
            && !is_query_plan(sql)
            && match self.explain_mode {
                crate::commands::ExplainMode::Auto => !is_bytecode_explain(sql),
                crate::commands::ExplainMode::On => false,
                crate::commands::ExplainMode::Off => true,
            }
    }

    /// Runs a statement and prints its rows as the engine produces them.
    ///
    /// **The rows before a failure are printed before the failure is reported**, because the
    /// printer has already written them, which is the order the reference shell prints them in.
    /// Returns how many rows were printed.
    ///
    /// @param sql - the statement
    pub fn print_streamed(&mut self, sql: &str) -> Result<usize, Failure> {
        let layout = self.rendering_layout();
        let sinks = stream::LineSinks {
            redirection: self.redirection.take(),
            sink: self.sink.take(),
            captured: self
                .testcase
                .is_some()
                .then(|| std::mem::take(&mut self.captured)),
            crlf: self.crlf,
        };
        let printer = std::rc::Rc::new(std::cell::RefCell::new(stream::RowPrinter::new(
            layout,
            self.header_when_empty,
            sinks,
        )));
        let ran = self.stream_into(sql, &printer);
        let (printed, sinks) = {
            let mut held = printer.borrow_mut();
            (held.finish(ran.is_ok()), held.release())
        };
        self.redirection = sinks.redirection;
        self.sink = sinks.sink;
        if let Some(captured) = sinks.captured {
            self.captured = captured;
        }
        ran.map(|()| printed)
    }

    /// Compiles a statement, binds its values and streams it into a printer.
    ///
    /// @param sql - the statement
    /// @param printer - receives the column names and each row
    fn stream_into(
        &self,
        sql: &str,
        printer: &std::rc::Rc<std::cell::RefCell<stream::RowPrinter>>,
    ) -> Result<(), Failure> {
        let connection = self.connection();
        let mut statement = self.prepare_bound(&connection, sql, &[])?;
        statement
            .stream(printer)
            .map(|_| ())
            .map_err(|error| Failure {
                message: reason(&error),
                offset: None,
                compiling: false,
                error: Some(error),
                partial: None,
            })
    }

    /// Prints the query plan for a statement, for `.eqp on`.
    fn print_plan(&mut self, sql: &str) {
        let plan = format!("EXPLAIN QUERY PLAN {sql}");
        let Ok((_, rows)) = self.collect(&plan) else {
            return;
        };
        for line in plan_tree(&rows) {
            self.say(&line);
        }
    }

    /// Returns the text left over after the first statement, when it holds another one.
    ///
    /// **The parser's own count, not a scan for semicolons.** A trigger body contains a semicolon,
    /// and a string literal can contain anything, so counting them is how a correct script gets
    /// refused and an incorrect one gets accepted. `prepare_with_tail` reports how many bytes the
    /// first statement used, and `leading_trivia` reports how much of what is left is not a
    /// statement at all - which is what makes a trailing semicolon and a trailing comment not count
    /// as a second statement. Both are the engine's own, so there is no second scanner here to
    /// disagree with the parser.
    ///
    /// A script that will not compile answers `None`: it is a syntax error, and it should be
    /// reported as the syntax error it is rather than as a script with too many statements in it.
    ///
    /// @param sql - the text a caller passed as one statement
    pub fn trailing_statement(&self, sql: &str) -> Option<String> {
        let connection = self.connection();
        let consumed = connection.prepare_with_tail(sql).ok()?.consumed;
        let left = sql.get(consumed..)?;
        let rest = left.get(inillucent_driver::leading_trivia(left)..)?.trim();
        if rest.is_empty() {
            return None;
        }
        Some(rest.chars().take(60).collect())
    }

    /// Runs a statement for its effect, reporting only a failure.
    ///
    /// @param sql - the statements, separated by semicolons
    pub fn execute(&mut self, sql: &str) -> Result<(), String> {
        self.connection()
            .execute_batch(sql)
            .map_err(|error| reason(&error))
    }

    /// Returns whether a statement changes something, by its class.
    ///
    /// **From `inillucent_driver::readonly`, the same answer the command
    /// surface and the driver use (task-1979, section 5.2).** It used to ask
    /// the engine to plan the statement and read the text of the failure, and
    /// `explain` answers `Ok` for an `INSERT`, a write pragma, an `ATTACH` and
    /// a `VACUUM INTO` - so this reported that none of them writes.
    ///
    /// @param sql - the statement
    pub fn writes(&self, sql: &str) -> bool {
        !inillucent_driver::readonly::admits(sql)
    }

    /// Returns one column of one row, as text.
    pub fn scalar(&self, sql: &str) -> Option<String> {
        let (_, rows) = self.collect(sql).ok()?;
        let value = rows.first().and_then(|row| row.first())?;
        Some(match value {
            Value::Null => String::new(),
            Value::Text(text) => String::from_utf8_lossy(text.raw()).into_owned(),
            other => crate::render::literal(other),
        })
    }

    /// Returns the first column of every row, as text.
    pub fn column(&self, sql: &str) -> Vec<String> {
        let Ok((_, rows)) = self.collect(sql) else {
            return Vec::new();
        };
        rows.iter()
            .filter_map(|row| row.first())
            .map(|value| match value {
                Value::Null => String::new(),
                Value::Text(text) => String::from_utf8_lossy(text.raw()).into_owned(),
                other => crate::render::literal(other),
            })
            .collect()
    }
}

/// The text an error report draws its excerpt from: a chunk of input from the start of one
/// statement to the end of the chunk.
///
/// It shares the chunk instead of copying the rest of it. A line of 20,000 statements gave each
/// statement its own copy of everything after it, which made the line quadratic in its length
/// (3.4 s against SQLite's 0.05 s). Only a failing statement ever reads the text.
#[derive(Clone)]
pub struct Excerpt {
    chunk: std::rc::Rc<str>,
    start: usize,
}

impl Excerpt {
    /// Returns the excerpt: the chunk from the statement's first character, without trailing
    /// whitespace.
    pub fn text(&self) -> &str {
        self.chunk.get(self.start..).unwrap_or_default().trim_end()
    }
}

/// Reads input line by line, running statements as they become complete.
///
/// A line beginning with a dot is a command, but only when nothing is
/// half-typed: `.` inside a `CREATE TRIGGER` body is part of the statement, and
/// treating it as a command there is the bug every naive shell has.
pub fn drive(shell: &mut Shell, input: impl Iterator<Item = String>) {
    drive_lines(shell, input);
    // Nothing held outlives the input it came from.
    shell.run_held();
}

/// The body of [`drive`], which runs what is still held when it returns.
///
/// @param shell - the shell
/// @param input - the lines
fn drive_lines(shell: &mut Shell, input: impl Iterator<Item = String>) {
    let mut pending = String::new();
    let mut number = 0usize;
    let mut started = 1usize;
    for line in input {
        number += 1;
        // The reference swallows a line that holds only whitespace or comments when no statement
        // is half read, so such a line neither starts a statement nor moves the line number an
        // error reports. `--what` on line 1 and a bad statement on line 2 is an error on line 2.
        // With `.echo on` the line is still passed through, which keeps the echoed text unchanged.
        if !shell.echo && pending.trim().is_empty() && only_whitespace_and_comments(&line) {
            continue;
        }
        if pending.trim().is_empty() {
            started = number;
        }
        shell.line = started;
        if pending.trim().is_empty() && line.starts_with('.') {
            if shell.echo {
                let text = line.trim().to_string();
                shell.say(&text);
            }
            crate::dot::run(shell, line.trim_start());
            shell.end_command();
            if shell.done || (shell.failed && shell.bail) {
                return;
            }
            continue;
        }
        pending.push_str(&line);
        pending.push('\n');
        // **Only a line that could end a statement is checked.** The check scans
        // everything accumulated, so asking it after every line was quadratic in
        // a statement's length: one `INSERT` of 20,000 rows written a row to a
        // line rescanned the whole statement 20,000 times (task-2191). A statement
        // ends at a semicolon, and text that held one already can only become
        // complete when a block comment after it closes, so a line with neither
        // cannot change the answer. SQLite's shell skips the same lines.
        if !(line.contains(';') || line.contains("*/")) {
            continue;
        }
        // **The first statement's end comes back from the check, and the buffer
        // keeps its capacity** (task-2191). The chunk was scanned here and again
        // by `run_chunk`, and `std::mem::take` handed the buffer away so the
        // next statement grew a new one from nothing.
        if let Some(first_end) = ends_in_complete_statement(shell, &pending) {
            run_chunk_from(shell, &pending, Some(first_end));
            shell.end_command();
            pending.clear();
            if shell.done || (shell.failed && shell.bail) {
                return;
            }
        }
    }
    if !pending.trim().is_empty() {
        // Whatever is left was never terminated. Running it is what SQLite's
        // shell does at end of input, and it is what makes `echo "SELECT 1" |
        // inillucent-shell` work without a semicolon.
        run_chunk(shell, &pending);
        shell.end_command();
    }
}

/// Reports whether the text read so far ends at the end of a statement, by
/// returning where its first statement ends, or `None` when it does not end.
///
/// This is `sqlite3_complete` applied to everything the shell has accumulated, which is
/// what decides when the reference's shell runs its input: when a line makes the whole
/// text end in a `;` that is not inside a string, a comment or a trigger body.
///
/// @param shell - the shell
/// @param text - the lines read since the last chunk ran
fn ends_in_complete_statement(shell: &Shell, text: &str) -> Option<usize> {
    let mut rest = text;
    let mut first: Option<usize> = None;
    while let Some(consumed) = complete_statement(shell, rest) {
        first.get_or_insert(consumed);
        rest = rest.get(consumed..).unwrap_or_default();
        if only_whitespace_and_comments(rest) {
            return first;
        }
    }
    None
}

/// Reports whether a script begins, commits or rolls back a transaction of its
/// own in any of its statements.
///
/// For `batch`, which wraps a script in a transaction: a script that has its
/// own `BEGIN` and `COMMIT`, such as `.dump` output, failed at its `BEGIN` with
/// "cannot start a transaction within a transaction". Statements are cut where
/// [`complete_statement`] cuts them, so a word inside a string, a comment or a
/// trigger body is not read as one.
///
/// @param shell - the shell
/// @param text - the script
pub(crate) fn controls_its_own_transaction(shell: &Shell, text: &str) -> bool {
    let mut rest = text;
    loop {
        let start = inillucent_driver::leading_trivia(rest);
        let statement = rest.get(start..).unwrap_or_default();
        let word: String = statement
            .chars()
            .take_while(char::is_ascii_alphabetic)
            .collect::<String>()
            .to_ascii_uppercase();
        if matches!(word.as_str(), "BEGIN" | "COMMIT" | "END" | "ROLLBACK") {
            return true;
        }
        match complete_statement(shell, rest) {
            Some(consumed) if consumed > 0 => rest = rest.get(consumed..).unwrap_or_default(),
            _ => return false,
        }
    }
}

/// Reports whether text holds nothing but whitespace and comments.
///
/// @param text - the text after a statement's semicolon
fn only_whitespace_and_comments(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        match bytes.get(at).copied() {
            Some(byte) if byte.is_ascii_whitespace() => at += 1,
            Some(b'-') if bytes.get(at + 1) == Some(&b'-') => at = skip_line_comment(bytes, at),
            Some(b'/') if bytes.get(at + 1) == Some(&b'*') => match skip_block_comment(bytes, at) {
                Some(end) => at = end,
                None => return false,
            },
            _ => return false,
        }
    }
    true
}

/// Runs the statements of one chunk of input the way the reference runs a line.
///
/// **The unit is the chunk, not the statement.** The reference's shell hands everything
/// it has accumulated to one call that prepares and runs statement after statement.
/// A statement that fails to compile ends the call and its error is printed. A statement
/// that fails while running does not end it: the next statement runs, and only the last
/// runtime error is printed, after the line. See [`Shell::print_or_hold`].
///
/// @param shell - the shell
/// @param chunk - the text, which may hold several statements
fn run_chunk(shell: &mut Shell, chunk: &str) {
    run_chunk_from(shell, chunk, None);
}

/// [`run_chunk`], told where the first statement ends when the caller has
/// already found it.
///
/// @param shell - the shell
/// @param chunk - the text, which may hold several statements
/// @param first_end - how many bytes the first statement takes, when known
fn run_chunk_from(shell: &mut Shell, chunk: &str, first_end: Option<usize>) {
    shell.chunk_number = shell.chunk_number.wrapping_add(1);
    shell.holding = true;
    shell.chunk_stop = false;
    shell.held_error = None;
    // **Slices of the chunk, not copies of it** (task-2191). Each statement
    // used to copy the rest of the chunk twice, once to split it and once to
    // build the excerpt, which was 2% of a script of single row inserts.
    let mut at = 0usize;
    let mut known = first_end;
    let mut whole: Option<std::rc::Rc<str>> = None;
    loop {
        let rest = chunk.get(at..).unwrap_or("");
        let consumed = match known.take() {
            Some(consumed) => consumed,
            None => complete_statement(shell, rest).unwrap_or(rest.len()),
        };
        let statement = rest.get(..consumed).unwrap_or(rest);
        let remainder = rest.get(consumed..).unwrap_or("");
        if !statement.trim().is_empty() {
            // The excerpt under an error runs on into the statements after this
            // one. With nothing after it, the excerpt is the statement itself,
            // which is what `report` shows when there is no excerpt.
            shell.excerpt = if remainder.trim().is_empty() {
                None
            } else {
                let lead = statement.len().saturating_sub(statement.trim_start().len());
                let shared = whole.get_or_insert_with(|| std::rc::Rc::<str>::from(chunk));
                Some(Excerpt {
                    chunk: shared.clone(),
                    start: at.saturating_add(lead),
                })
            };
            shell.run(statement.trim());
        }
        at = at.saturating_add(consumed);
        if shell.done
            || shell.chunk_stop
            || (shell.failed && shell.bail)
            || remainder.trim().is_empty()
        {
            break;
        }
    }
    shell.holding = false;
    shell.excerpt = None;
    if let Some(lines) = shell.held_error.take() {
        for line in lines {
            shell.complain(&line);
        }
    }
}

/// Returns how many bytes of `text` form one complete statement, if any.
///
/// This is `sqlite3_complete`, and it is lexical on purpose. Asking the parser
/// cannot work: a `CREATE TRIGGER` does not parse until its `END`, and a parse
/// failure does not distinguish "still typing" from "misspelt". What a shell
/// needs to know is narrower and decidable - has a semicolon been reached that
/// is not inside a trigger body - so that is what is computed.
fn complete_statement(_shell: &Shell, text: &str) -> Option<usize> {
    let mut state = State::Start;
    let bytes = text.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        // **In a plain statement only quotes, comments and semicolons matter**
        // (task-2191). No word changes `State::Plain` and nothing else does
        // either, so the bytes between those are skipped in one pass rather
        // than each word being upper cased and compared. The scan ran twice
        // per statement of a script of single row inserts and was 9% of it.
        if state == State::Plain {
            at = bytes
                .get(at..)
                .and_then(|rest| rest.iter().position(|byte| matters_in_plain(*byte)))
                .map_or(bytes.len(), |offset| at.saturating_add(offset));
        }
        let Some(byte) = bytes.get(at).copied() else {
            break;
        };
        match byte {
            b'-' if bytes.get(at + 1) == Some(&b'-') => {
                at = skip_line_comment(bytes, at);
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                // `?` rather than a `let ... else`: an unterminated block
                // comment is more input to come, which is what `None` means all
                // the way up this function.
                at = skip_block_comment(bytes, at)?;
            }
            b'\'' | b'"' | b'`' => {
                at = skip_quoted(bytes, at, byte)?;
            }
            b'[' => {
                at = skip_quoted(bytes, at, b']')?;
            }
            b';' => {
                at += 1;
                if state.ends_here() {
                    return Some(at);
                }
                state = state.after_semicolon();
            }
            _ if byte.is_ascii_alphabetic() || byte == b'_' => {
                let end = word_end(bytes, at);
                // Upper cased into a buffer on the stack. `to_ascii_uppercase`
                // allocated a vector for every word of every statement, which
                // was 2.1% of a script of single row inserts (task-2191). A word
                // longer than the buffer is no keyword `after_word` knows, so it
                // is passed as the empty word, which no keyword matches either.
                let mut folded = [0u8; KEYWORD_LONGEST];
                let word = bytes.get(at..end).unwrap_or(&[]);
                let word = match folded.get_mut(..word.len()) {
                    Some(into) => {
                        into.copy_from_slice(word);
                        into.make_ascii_uppercase();
                        &*into
                    }
                    None => &[],
                };
                state = state.after_word(word);
                at = end;
            }
            _ if byte.is_ascii_whitespace() => at += 1,
            _ => {
                state = state.after_other();
                at += 1;
            }
        }
    }
    None
}

/// Reports whether a byte can change what a semicolon means in a plain
/// statement: a quote, the start of a comment, or a semicolon.
///
/// @param byte - the byte
fn matters_in_plain(byte: u8) -> bool {
    matches!(byte, b';' | b'\'' | b'"' | b'`' | b'[' | b'-' | b'/')
}

/// The longest word `State::after_word` compares against: `TEMPORARY`.
const KEYWORD_LONGEST: usize = 9;

/// Where the scan is, in terms of what a semicolon would mean.
#[derive(Clone, Copy, PartialEq, Eq)]
enum State {
    /// Nothing has been read yet, or the last statement finished.
    Start,
    /// A statement is under way and a semicolon ends it.
    Plain,
    /// `CREATE` has been read, and the next words decide.
    Create,
    /// `CREATE ... TRIGGER` has been read; the body has not started.
    Trigger,
    /// Inside a trigger body, where a semicolon ends a nested statement.
    Body,
    /// A semicolon inside a body has just been read, so an `END` now would
    /// close the trigger.
    ///
    /// **Only an `END` straight after a semicolon closes a trigger**, which is
    /// `sqlite3_complete`'s table. Any `END` in the body used to count, so the
    /// `END` of a `CASE` did: `BEGIN SELECT CASE WHEN NEW.n < 0 THEN
    /// RAISE(ABORT, 'negative') END; END;` was cut after the first `END;` and
    /// sent to the parser as a trigger with no end, which is the usual way to
    /// write a guard trigger.
    Semi,
    /// `END` has been read after a semicolon inside a body, so a semicolon
    /// ends the whole thing.
    End,
}

impl State {
    /// Returns whether a semicolon here finishes the statement.
    fn ends_here(self) -> bool {
        !matches!(self, State::Trigger | State::Body | State::Semi)
    }

    /// Returns the state after a semicolon that did not finish anything.
    fn after_semicolon(self) -> State {
        match self {
            State::Trigger | State::Body | State::Semi => State::Semi,
            _ => State::Start,
        }
    }

    /// Returns the state after a word.
    fn after_word(self, word: &[u8]) -> State {
        match self {
            State::Start if word == b"CREATE" => State::Create,
            State::Start if word == b"EXPLAIN" => State::Start,
            State::Start => State::Plain,
            // `TEMP`, `TEMPORARY` and `IF NOT EXISTS` all sit between `CREATE`
            // and the thing being created, so they leave the state alone.
            State::Create
                if matches!(
                    word,
                    b"TEMP" | b"TEMPORARY" | b"IF" | b"NOT" | b"EXISTS" | b"OR" | b"REPLACE"
                ) =>
            {
                State::Create
            }
            State::Create if word == b"TRIGGER" => State::Trigger,
            State::Create => State::Plain,
            State::Trigger if word == b"BEGIN" => State::Body,
            State::Semi if word == b"END" => State::End,
            State::Semi | State::End => State::Body,
            other => other,
        }
    }

    /// Returns the state after anything that is not a word or a semicolon.
    fn after_other(self) -> State {
        match self {
            State::Start => State::Plain,
            State::Semi | State::End => State::Body,
            other => other,
        }
    }
}

/// Returns the offset just past a `--` comment.
fn skip_line_comment(bytes: &[u8], at: usize) -> usize {
    let mut scan = at + 2;
    while scan < bytes.len() {
        if bytes.get(scan) == Some(&b'\n') {
            return scan + 1;
        }
        scan += 1;
    }
    scan
}

/// Returns the offset just past a block comment, or `None` when it is open.
fn skip_block_comment(bytes: &[u8], at: usize) -> Option<usize> {
    let mut scan = at + 2;
    while scan + 1 < bytes.len() {
        if bytes.get(scan) == Some(&b'*') && bytes.get(scan + 1) == Some(&b'/') {
            return Some(scan + 2);
        }
        scan += 1;
    }
    None
}

/// Returns the offset just past a quoted run, or `None` when it is open.
///
/// A doubled quote inside a quoted run is one character and does not close it,
/// which is the case a naive scan gets wrong on `'it''s'`.
fn skip_quoted(bytes: &[u8], at: usize, close: u8) -> Option<usize> {
    let open = bytes.get(at).copied()?;
    let mut scan = at + 1;
    while scan < bytes.len() {
        let byte = bytes.get(scan).copied()?;
        if byte == close {
            if close == open && bytes.get(scan + 1) == Some(&close) {
                scan += 2;
                continue;
            }
            return Some(scan + 1);
        }
        scan += 1;
    }
    None
}

/// Returns the offset just past a word.
fn word_end(bytes: &[u8], at: usize) -> usize {
    let mut scan = at;
    while scan < bytes.len() {
        match bytes.get(scan) {
            Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'_' => scan += 1,
            _ => break,
        }
    }
    scan
}

/// Reports whether a statement is an `EXPLAIN QUERY PLAN`.
///
/// The words rather than the bound statement, because the shell decides how to
/// *print* before it knows what the engine made of it - and the two spellings
/// SQLite accepts are `EXPLAIN QUERY PLAN` and nothing else.
///
/// @param sql - the statement as typed
fn is_query_plan(sql: &str) -> bool {
    let mut words = sql.split_whitespace();
    words
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("explain"))
        && words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("query"))
        && words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("plan"))
}

/// The column widths the reference prints an `EXPLAIN` listing in.
///
/// `addr`, `opcode`, `p1`, `p2`, `p3`, `p4`, `p5`, `comment` - the same numbers
/// its shell carries, so a listing lines up under the same headings.
const EXPLAIN_WIDTHS: [usize; 8] = [4, 13, 4, 4, 4, 13, 2, 13];

/// Reports whether a statement is an `EXPLAIN` in its bytecode form.
///
/// The word `EXPLAIN` not followed by `QUERY`, which is the only other thing it
/// can be followed by.
///
/// @param sql - the statement as typed
fn is_bytecode_explain(sql: &str) -> bool {
    let mut words = sql.split_whitespace();
    words
        .next()
        .is_some_and(|word| word.eq_ignore_ascii_case("explain"))
        && !words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("query"))
}

/// Renders an `EXPLAIN` listing as the reference's fixed-width table.
///
/// A header, a rule of dashes, then one line per instruction, each column
/// left-aligned in its own width and separated by two spaces. A value wider
/// than its column is not truncated - the reference does not truncate either,
/// and a clipped opcode name would be worse than a ragged line.
///
/// @param columns - the column names, which are the reference's headings
/// @param rows - the instructions
fn explain_table(columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    /// What separates two columns.
    const GAP: &str = "  ";

    let mut lines = Vec::with_capacity(rows.len().saturating_add(2));
    lines.push(
        columns
            .iter()
            .enumerate()
            .map(|(at, name)| pad(name, EXPLAIN_WIDTHS.get(at).copied().unwrap_or(0)))
            .collect::<Vec<String>>()
            .join(GAP),
    );
    lines.push(
        EXPLAIN_WIDTHS
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<String>>()
            .join(GAP),
    );
    let last = EXPLAIN_WIDTHS.len().saturating_sub(1);
    for row in rows {
        let cells: Vec<String> = (0..EXPLAIN_WIDTHS.len())
            .map(|at| {
                let text = match row.get(at) {
                    Some(Value::Text(text)) => String::from_utf8_lossy(text.raw()).into_owned(),
                    Some(Value::Null) | None => String::new(),
                    Some(other) => crate::render::literal(other),
                };
                // **The last column of a row is written as it is.** The heading
                // is padded and the instruction's comment is not, which is what
                // leaves a `Halt` line ending in the separator rather than in
                // thirteen spaces. It is a small thing and it is two bytes of
                // difference per line against the reference.
                if at == last {
                    text
                } else {
                    pad(&text, EXPLAIN_WIDTHS.get(at).copied().unwrap_or(0))
                }
            })
            .collect();
        lines.push(cells.join(GAP));
    }
    lines
}

/// Left-aligns one cell in its column.
///
/// @param text - the cell
/// @param width - the column's width
fn pad(text: &str, width: usize) -> String {
    let mut out = text.to_string();
    while out.chars().count() < width {
        out.push(' ');
    }
    out
}

/// Renders `EXPLAIN QUERY PLAN`'s four columns as the tree the reference draws.
///
/// **The rows are a tree and were being printed as rows.** Each carries an id
/// and its parent's id, and the reference draws them under a `QUERY PLAN`
/// heading, with `|--` for a node that has a sibling after it and a backtick
/// arm for the last, indented three characters per level - which is how a
/// subquery under a step is told from a step beside it. Printing the raw four
/// columns left the shape for the reader to work out.
///
/// @param rows - the plan's rows: id, parent, notused, detail
pub fn plan_tree(rows: &[Vec<Value<'static>>]) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
    }
    let mut lines = vec!["QUERY PLAN".to_string()];
    plan_children(rows, 0, "", 0, &mut lines);
    lines
}

/// The arm the reference draws under the last child of a node.
const LAST_ARM: &str = "`--";

/// How deep a plan tree may be drawn before the walk gives up.
///
/// A plan that named itself as its own parent would otherwise not terminate,
/// and a malformed plan is not a reason for a shell to hang. The cap rather
/// than an id check, because this engine numbers its top-level rows from zero
/// and the root is asked for by parent zero - so a row whose id and parent are
/// both zero is the ordinary first line of every plan.
const PLAN_DEPTH: usize = 64;

/// Emits one parent's children, and theirs.
///
/// @param rows - every row of the plan
/// @param parent - the id whose children to emit
/// @param prefix - the indent the ancestors give
/// @param depth - how deep this call is
/// @param lines - where the rendered lines go
fn plan_children(
    rows: &[Vec<Value<'static>>],
    parent: i64,
    prefix: &str,
    depth: usize,
    lines: &mut Vec<String>,
) {
    if depth >= PLAN_DEPTH {
        return;
    }
    let field = |row: &Vec<Value<'static>>, at: usize| -> i64 {
        row.get(at).and_then(Value::as_integer).unwrap_or(0)
    };
    let children: Vec<&Vec<Value<'static>>> =
        rows.iter().filter(|row| field(row, 1) == parent).collect();
    for (at, row) in children.iter().enumerate() {
        let last = at.saturating_add(1) == children.len();
        let detail = row
            .last()
            .and_then(Value::as_text)
            .map(|text| String::from_utf8_lossy(text.raw()).into_owned())
            .unwrap_or_default();
        let arm = if last { LAST_ARM } else { "|--" };
        lines.push(format!("{prefix}{arm}{detail}"));
        // A node that still has siblings below it keeps a vertical bar in its
        // children's indent; the last one leaves a space.
        let carried = format!("{prefix}{}", if last { "   " } else { "|  " });
        // A row that names its own parent's id is its own child, which is what
        // a top-level row looks like on an engine that numbers from zero: it
        // has id 0 and parent 0. It is selected as a child of the root and must
        // not then be expanded as its own parent.
        if field(row, 0) != parent {
            plan_children(
                rows,
                field(row, 0),
                &carried,
                depth.saturating_add(1),
                lines,
            );
        }
    }
}

/// Returns the two lines that point at where a statement went wrong.
///
/// A port of the reference shell's `shell_error_context`, down to the two
/// arrangements of the marker and the number that chooses between them, because
/// this is one of the places a transcript is compared rather than read. The
/// reference slides a window along the statement so the offending token is never
/// off the left of the line, truncates at 78 bytes, flattens every space
/// character to a plain space so a tab cannot shift the marker, and then draws
/// the caret to the left of the token while it still fits and to the right of a
/// trailing rule once it does not.
///
/// Returns nothing when the position is not inside the statement, which is the
/// reference's answer for `no such table` and for everything that fails while
/// stepping rather than while parsing.
///
/// @param sql - the whole statement, as the shell was given it
/// @param offset - the byte the engine says the error is at
fn error_context(sql: &[u8], offset: usize) -> Vec<String> {
    if offset >= sql.len() {
        return Vec::new();
    }
    // Slide the window right until the marker is within 50 bytes of the start,
    // never stopping inside a UTF-8 sequence.
    let mut start = 0usize;
    let mut column = offset;
    while column > 50 {
        start += 1;
        column -= 1;
        while sql.get(start).is_some_and(|byte| byte & 0xc0 == 0x80) {
            start += 1;
            column -= 1;
        }
    }
    let window = sql.get(start..).unwrap_or_default();
    let mut length = window.len().min(78);
    while length > 0 && window.get(length).is_some_and(|byte| byte & 0xc0 == 0x80) {
        length -= 1;
    }
    let shown = String::from_utf8_lossy(window.get(..length).unwrap_or_default())
        .chars()
        .map(|character| {
            if character.is_ascii_whitespace() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let marker = if column < 25 {
        format!("  {}^--- error here", " ".repeat(column))
    } else {
        format!("  {}error here ---^", " ".repeat(column - 14))
    };
    vec![format!("  {shown}"), marker]
}

/// Returns what a failure should say to a person.
///
/// **The detail, when there is one, and the code's text otherwise.** A
/// `DbError`'s `message` is the text of its primary code - "bad parameter or
/// other API misuse" for everything the engine refuses - and the sentence a
/// person can act on is in `detail`: "no such table: nope". Printing the code's
/// text made every refusal look like the same failure, which is the opposite of
/// what a shell is for.
///
/// @param error - what went wrong
fn reason(error: &inillucent_base::DbError) -> String {
    error
        .detail()
        .unwrap_or_else(|| error.message())
        .to_string()
}

#[cfg(test)]
mod trailing_statement_tests {
    use super::Shell;

    /// Opens a scratch shell over a database that reaches no file.
    fn shell() -> Shell {
        Shell::open(":memory:").expect("a memory database opens")
    }

    /// One statement is one statement, however it is punctuated.
    ///
    /// `exec` and `query` are documented as taking one, and used to run the first of several and
    /// report success - which is how `inillucent exec "<twenty CREATE TABLEs>"` produced a database
    /// with one table in it and printed `ok. 0 rows changed.` These are the cases the
    /// refusal must not fire on, and the one it must.
    #[test]
    fn a_second_statement_is_recognised_and_punctuation_is_not() {
        let held = shell();
        for one in [
            "CREATE TABLE a (id INTEGER PRIMARY KEY)",
            "CREATE TABLE a (id INTEGER PRIMARY KEY);",
            "CREATE TABLE a (id INTEGER PRIMARY KEY);   ",
            "CREATE TABLE a (id INTEGER PRIMARY KEY); -- and that is all",
            "CREATE TABLE a (id INTEGER PRIMARY KEY); /* and that is all */",
            "CREATE TABLE a (id INTEGER PRIMARY KEY);;;",
            // A trigger body holds semicolons, which is why counting them is the wrong test.
            "CREATE TRIGGER t AFTER INSERT ON a FOR EACH ROW BEGIN UPDATE a SET id = id; END",
            // And the `END` of a `CASE` inside one is not the trigger's.
            "CREATE TRIGGER t AFTER INSERT ON a BEGIN SELECT CASE WHEN NEW.id < 0 THEN RAISE(ABORT, 'no') END; END",
        ] {
            assert_eq!(
                held.trailing_statement(one),
                None,
                "{one:?} is one statement"
            );
        }

        let two = held
            .trailing_statement(
                "CREATE TABLE a (id INTEGER PRIMARY KEY); CREATE TABLE b (id INTEGER PRIMARY KEY)",
            )
            .expect("two statements are two statements");
        assert!(
            two.starts_with("CREATE TABLE b"),
            "the refusal names what comes next, and said {two:?}"
        );

        // A comment between them does not hide the second one.
        let commented = held
            .trailing_statement(
                "CREATE TABLE a (id INTEGER PRIMARY KEY); -- next
CREATE TABLE b (id INTEGER PRIMARY KEY)",
            )
            .expect("a comment does not hide a statement");
        assert!(
            commented.starts_with("CREATE TABLE b"),
            "said {commented:?}"
        );
    }

    /// The shell cuts its input where `sqlite3_complete` would, and a `CASE` inside a trigger
    /// body does not end the trigger.
    ///
    /// Each case is the text the shell has read so far and how much of it is one complete
    /// statement. The last three are the guard triggers the coffee shop example writes, which
    /// were cut after the `CASE`'s `END;` and sent to the parser as a trigger with no end.
    #[test]
    fn a_statement_ends_where_sqlite3_complete_says() {
        let held = shell();
        let guard = "CREATE TRIGGER g BEFORE UPDATE ON o BEGIN SELECT CASE WHEN NEW.n < 0 THEN RAISE(ABORT, 'negative ' || NEW.n) END; END;";
        for (text, wanted) in [
            ("SELECT 1; SELECT 2;", Some(9)),
            ("CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE a SET id = id; END; SELECT 1;", Some(67)),
            ("CREATE TRIGGER t AFTER INSERT ON a BEGIN UPDATE a SET id = id;", None),
            (guard, Some(guard.len())),
            ("CREATE TRIGGER g BEFORE UPDATE ON o BEGIN SELECT CASE WHEN 1 THEN 2 END; SELECT 3; END;", Some(87)),
            ("CREATE TRIGGER g BEFORE UPDATE ON o BEGIN SELECT CASE WHEN 1 THEN 2 END;", None),
        ] {
            assert_eq!(
                super::complete_statement(&held, text),
                wanted,
                "how much of {text:?} is one statement"
            );
        }
    }

    /// Text that will not compile is a syntax error, not a script with too many statements in it.
    #[test]
    fn text_that_does_not_compile_is_left_to_the_parser() {
        let held = shell();
        assert_eq!(held.trailing_statement("SELEKT 1"), None);
        assert_eq!(held.trailing_statement(""), None);
        assert_eq!(held.trailing_statement("-- only a comment"), None);
    }

    /// Single row inserts inside a transaction are held and run together before
    /// the statement that ends the transaction, and none are held outside one.
    ///
    /// `held_inserts` in the e2e tier checks that what a script prints is the
    /// same either way; this checks that the inserts were in fact held, which
    /// that comparison alone cannot tell.
    #[test]
    fn inserts_in_a_transaction_are_held_until_the_next_statement() {
        let mut held = shell();
        held.run("CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT)");
        held.run("INSERT INTO t VALUES (1, 'outside')");
        assert_eq!(
            held.held_count(),
            0,
            "an insert outside a transaction was held"
        );
        held.run("BEGIN");
        for id in 2..=6 {
            held.run(&format!("INSERT INTO t VALUES ({id}, 'v{id}')"));
        }
        assert_eq!(held.held_count(), 5, "the inserts were not held");
        held.run("COMMIT");
        assert_eq!(held.held_count(), 0);
        assert_eq!(
            held.scalar("SELECT count(*) || ',' || max(id) FROM t")
                .as_deref(),
            Some("6,6")
        );
    }

    /// A statement spread over many lines runs once, when the line that ends it is read.
    ///
    /// `drive` asks whether the text read so far is complete only on a line holding a `;` or
    /// a `*/` (task-2191). Asking after every line rescanned the whole statement each time, and
    /// one `INSERT` of 20,000 rows written a row to a line took 5.8 s against 41 ms for
    /// SQLite's shell. These are the cases the narrower question must still answer: the rows
    /// all arrive, a block comment closed on a line with no semicolon still lets the statement
    /// before it run, and a trigger whose body spans lines ends at its own `END;`.
    #[test]
    fn a_statement_over_many_lines_runs_once_it_ends() {
        let mut held = shell();
        let mut lines = vec![
            "CREATE TABLE m (id INTEGER PRIMARY KEY, a TEXT);".to_string(),
            "INSERT INTO m VALUES".to_string(),
        ];
        for id in 1..=2_000 {
            let tail = if id == 2_000 { ";" } else { "," };
            lines.push(format!("({id}, 'row {id}'){tail}"));
        }
        lines.push("INSERT INTO m VALUES (2001, 'commented'); /* a note".to_string());
        lines.push("that ends here */".to_string());
        lines.push("CREATE TABLE seen (n INTEGER);".to_string());
        lines.push("CREATE TRIGGER count_m AFTER INSERT ON m BEGIN".to_string());
        lines.push("  INSERT INTO seen VALUES (NEW.id);".to_string());
        lines.push("END;".to_string());
        lines.push("INSERT INTO m VALUES (2002, 'after the trigger');".to_string());
        super::drive(&mut held, lines.into_iter());
        assert_eq!(
            held.scalar("SELECT count(*) FROM m").as_deref(),
            Some("2002"),
            "a row of the long statement, the commented one or the last one is missing"
        );
        assert_eq!(
            held.scalar("SELECT group_concat(n) FROM seen").as_deref(),
            Some("2002"),
            "the trigger did not end where its END; is, or ran for the wrong rows"
        );
    }
}

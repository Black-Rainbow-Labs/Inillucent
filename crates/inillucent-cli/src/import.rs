//! `.import`: reading a delimited file into a table.
//!
//! Invariant: a field is a string unless the table says otherwise. The import
//! does not guess types - it binds every field as text and lets the column's
//! affinity decide, which is exactly what an `INSERT` with a string literal
//! would do. Guessing here would make `.import` and `INSERT` disagree about the
//! same file, and the one that guessed would be the one that was wrong.
//!
//! The separator is whatever `.separator` is set to unless an option overrides
//! it. The quoting rules are CSV's in every mode except `--ascii`: a field may
//! be quoted, a doubled quote inside a quoted field is one quote, and a
//! separator inside quotes is data. SQLite reads `.mode tabs` and `.mode list`
//! files the same way, and `.mode tabs` writes a value that holds a tab between
//! quotes, so a file written by one can only be read back by the other.
//!
//! **A row that does not fit is reported and the import goes on.** SQLite pads
//! a short row with NULL, drops the fields after the last column, and prints a
//! warning for each. A row the table rejects is named by the line it began on,
//! counted from 1 at the top of the file with the header included, and the
//! rows around it are kept.
//!
//! **The options are parsed, not skipped.** They were not, until task-1907, and
//! the cost was that `inillucent import` - the verb, which builds the
//! line `.import --csv "file" "table"` - failed on every file anybody gave it
//! with `cannot open "--csv"`, because the first word was read as the file name.
//! The dot command was tested through a script that passed no options, so the
//! one caller that always passes one was the only caller that never worked.

use std::borrow::Cow;

use inillucent_engine::connect::{Connection, Statement};

use crate::dot::word_error;
use crate::render::Mode;
use crate::shell::Shell;

/// The unit and record separators `--ascii` reads, which are what `.mode ascii`
/// writes.
const UNIT_SEPARATOR: char = '\u{1f}';
const RECORD_SEPARATOR: char = '\u{1e}';

/// How one `.import` reads its file, after its options have been read.
struct Reading<'a> {
    /// The file to read.
    path: &'a str,
    /// The table to load into.
    table: &'a str,
    /// What separates two fields.
    separator: char,
    /// What ends a row.
    row_separator: char,
    /// Whether a field may be quoted, RFC 4180 style.
    quoted: bool,
    /// How many rows to drop off the front, header included.
    skip: usize,
    /// The schema a table is created in, when `--schema` named one.
    schema: Option<&'a str>,
    /// How much to say: `-v` once, and twice.
    verbose: u8,
}

/// One row of the file, with the line of the file it began on.
struct Row<'a> {
    /// The line the row began on. The first line of the file is 1.
    line: usize,
    /// The row's fields.
    fields: Vec<Cow<'a, str>>,
    /// What reading the row's quoted fields found wrong, in the order met.
    notes: Vec<Note>,
}

/// A fault found while reading a quoted field, which is reported and not fatal.
struct Note {
    /// The line of the file the reader had reached.
    line: usize,
    /// Which fault.
    kind: NoteKind,
}

/// The two faults `sqlite3` reports in a quoted field.
enum NoteKind {
    /// A quote inside a quoted field that is neither doubled nor the closing one.
    Unescaped,
    /// The file ended inside a quoted field.
    Unterminated,
}

/// One thing an import has to say about a row, in the order the rows were met.
struct Complaint {
    /// Whether the row was rejected. A warning about a short row is not an error.
    error: bool,
    /// The complete line to print.
    text: String,
    /// The engine's refusal, when the row was rejected, so a command that reports
    /// a status can name the cause.
    cause: Option<inillucent_base::DbError>,
}

/// Reads `.import`'s options and the two names that follow them.
///
/// Every option the command's help text documents is understood, and a word that
/// begins with a dash and is not one is reported with a caret under it. Reading an
/// unknown option as the file name is the failure this function exists to prevent:
/// it produces `cannot open "--esc"`, which sends a reader looking for a missing
/// file rather than at the flag they mistyped.
///
/// @param shell - what the defaults are read from, and what a complaint goes to
/// @param arguments - the words after `.import`
fn reading<'a>(shell: &mut Shell, arguments: &[&'a str]) -> Option<Reading<'a>> {
    let (mut separator, mut row_separator, mut quoted) = mode_defaults(shell);
    let mut skip = 0usize;
    let mut schema: Option<&'a str> = None;
    let mut verbose = 0u8;
    let mut positional: Vec<&'a str> = Vec::new();

    let mut index = 0;
    while let Some(word) = arguments.get(index).copied() {
        index += 1;
        let takes_value = matches!(
            word,
            "--colsep" | "--rowsep" | "--skip" | "--schema" | "--esc" | "--qesc"
        );
        let value = if takes_value {
            let Some(taken) = arguments.get(index).copied() else {
                word_error(shell, index, "missing argument");
                return None;
            };
            index += 1;
            taken
        } else {
            ""
        };
        match word {
            "--csv" => {
                separator = ',';
                quoted = true;
                row_separator = '\n';
            }
            "--ascii" => {
                separator = UNIT_SEPARATOR;
                quoted = false;
                row_separator = RECORD_SEPARATOR;
            }
            // SQLite takes the first character of a longer separator.
            "--colsep" => separator = value.chars().next().unwrap_or('\0'),
            "--rowsep" => row_separator = value.chars().next().unwrap_or('\0'),
            "--skip" => skip = usize::try_from(crate::dot::c_atoi(value)).unwrap_or(0),
            "--schema" => schema = Some(value),
            // The escape characters are in SQLite's help for this command and are not
            // built here. They are refused rather than ignored: an escape character
            // that is accepted and not applied loads a file whose fields are silently
            // wrong.
            "--esc" | "--qesc" => {
                shell.complain(&format!(
                    "Error: .import {word} is not supported by this engine"
                ));
                return None;
            }
            "-v" => verbose = verbose.saturating_add(1),
            other if other.starts_with('-') && other.len() > 1 => {
                word_error(shell, index, "unknown option");
                return None;
            }
            other => {
                if positional.len() == 2 {
                    word_error(shell, index, "unknown argument");
                    return None;
                }
                positional.push(other);
            }
        }
    }

    let place = shell.error_location();
    let Some(path) = positional.first().copied() else {
        shell.complain(&format!("{place} Missing FILE argument\n"));
        return None;
    };
    let Some(table) = positional.get(1).copied() else {
        shell.complain(&format!("{place} Missing TABLE argument\n"));
        return None;
    };
    // SQLite never ends a row at a bare carriage return outside `--ascii`.
    if row_separator == '\r' && quoted {
        row_separator = '\n';
    }
    Some(Reading {
        path,
        table,
        separator,
        row_separator,
        quoted,
        skip,
        schema,
        verbose,
    })
}

/// Returns the separators and quoting the current output mode asks `.import` to read with.
///
/// `ascii` reads the unit and record separators without quoting, `csv` reads
/// commas with RFC 4180 quoting, and any other mode reads its own column separator
/// with quoting.
///
/// @param shell - the shell
fn mode_defaults(shell: &Shell) -> (char, char, bool) {
    match shell.layout.mode {
        Mode::Csv => (',', '\n', true),
        Mode::Ascii => (UNIT_SEPARATOR, RECORD_SEPARATOR, false),
        _ => (
            shell.layout.separator.chars().next().unwrap_or('|'),
            '\n',
            true,
        ),
    }
}

/// How many bytes of the file are read at a time.
const BLOCK: usize = 1 << 20;

/// The most rows one `INSERT` statement takes.
///
/// A batch is converted to the engine's values and inserted as one statement, and that conversion
/// is what a batch costs in memory. 20,000 rows of a few fields each is a few megabytes.
const BATCH_ROWS: usize = 20_000;

/// A delimited file, read a block at a time into text that starts at the beginning of a row.
struct RowSource {
    /// Where the bytes come from.
    input: Box<dyn std::io::Read>,
    /// Bytes read that end in the middle of a character, kept until the rest of it is read.
    tail: Vec<u8>,
    /// The decoded text from the start of the first row that has not been reported.
    text: String,
    /// The line of the file `text` starts on.
    line: usize,
    /// Whether the last block reached the end of the file.
    ended: bool,
    /// Whether the first block has been read, which is when a byte order mark is dropped.
    started: bool,
    /// How many bytes the next read asks for. It doubles when a block held no complete row, so a
    /// row of any length is read in a number of reads that grows with its logarithm.
    block: usize,
}

impl RowSource {
    /// Returns a source over some bytes.
    ///
    /// @param input - the file, or the output of the command a name beginning with `|` runs
    fn new(input: Box<dyn std::io::Read>) -> RowSource {
        RowSource {
            input,
            tail: Vec::new(),
            text: String::new(),
            line: 1,
            ended: false,
            started: false,
            block: BLOCK,
        }
    }

    /// Reads the next block and adds its text to the text held.
    ///
    /// **A character cut by the end of the block waits for the rest of it.** Decoding the
    /// block's bytes as they are would turn the two halves of one character into stand ins for
    /// bytes that are not UTF-8, which is a different value from the file's. The bytes of the
    /// unfinished character are kept and read again with the next block. A UTF-8 byte order mark
    /// is dropped from the start of the file, as it was when the file was read whole.
    fn read_block(&mut self) -> std::io::Result<()> {
        use std::io::Read;
        let mut bytes = std::mem::take(&mut self.tail);
        let before = bytes.len();
        // The first read is at least three bytes, so a byte order mark is whole when it is looked
        // for.
        let wanted = if self.started {
            self.block
        } else {
            self.block.max(3)
        };
        (&mut self.input)
            .take(wanted as u64)
            .read_to_end(&mut bytes)?;
        if bytes.len().saturating_sub(before) < wanted {
            self.ended = true;
        }
        if !self.started {
            self.started = true;
            if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                bytes.drain(..3);
            }
        }
        let cut = if self.ended {
            bytes.len()
        } else {
            whole_characters(&bytes)
        };
        self.tail = bytes.split_off(cut);
        self.text.push_str(&crate::raw::decode(&bytes));
        Ok(())
    }
}

/// Returns how many leading bytes end on a character boundary, or at the end of a sequence that
/// cannot be finished.
///
/// A character is at most four bytes, so only the last three bytes can be the start of one that
/// the end of a block cut.
///
/// @param bytes - the bytes of a block
fn whole_characters(bytes: &[u8]) -> usize {
    let length = bytes.len();
    for back in 1..=3usize.min(length) {
        let at = length - back;
        let needed = match bytes.get(at).copied() {
            Some(0x80..=0xBF) => continue,
            Some(0xC0..=0xDF) => 2,
            Some(0xE0..=0xEF) => 3,
            Some(0xF0..=0xF7) => 4,
            _ => return length,
        };
        return if needed > back { at } else { length };
    }
    length
}

/// Reads a file window by window, giving the complete rows of each window to `take`.
///
/// **The whole file is never held.** A window is the text from the first row not yet reported to
/// the end of the block just read, so what is held is a block plus the part of a row that was
/// cut. `take` is told how many row separators the window's rows used, and answers whether to
/// go on.
///
/// @param source - the file
/// @param separators - what ends a field, and what ends a row
/// @param quoted - whether a field may be quoted
/// @param take - receives the rows of each window and the row separators they used
fn each_window(
    source: &mut RowSource,
    separators: (char, char),
    quoted: bool,
    mut take: impl FnMut(Vec<Row<'_>>, usize) -> bool,
) -> std::io::Result<()> {
    loop {
        if !source.ended {
            source.read_block()?;
        }
        let (consumed, line, going) = {
            let window = parse_window(&source.text, separators, quoted, source.line, source.ended);
            let used = source.text.get(..window.consumed).unwrap_or_default();
            let lines = used.matches(separators.1).count();
            (window.consumed, window.line, take(window.rows, lines))
        };
        source.text.drain(..consumed);
        source.line = line;
        if !going || source.ended {
            return Ok(());
        }
        if consumed == 0 {
            source.block = source.block.saturating_mul(2);
        }
    }
}

/// Opens the file, or the command whose output is the file.
///
/// A name that begins with `|` is a command, and its output is the file.
///
/// @param path - the file to read, or `|` and a command
fn open_input(path: &str) -> Result<Box<dyn std::io::Read>, String> {
    match path.strip_prefix('|') {
        Some(command) => {
            let bytes = crate::commands::capture_output(command)
                .ok_or_else(|| "the command could not be run".to_string())?;
            Ok(Box::new(std::io::Cursor::new(bytes)))
        }
        None => std::fs::File::open(path)
            .map(|file| Box::new(file) as Box<dyn std::io::Read>)
            .map_err(|error| error.to_string()),
    }
}

/// Where the rows of an import go, once the table is known.
struct Target {
    /// How many columns the table has, which is how many fields a row needs. Zero when there is
    /// nothing to insert into.
    columns: usize,
    /// The `INSERT` with one parameter for each column.
    sql: String,
    /// Whether the import opened the transaction the rows go in.
    began: bool,
}

/// What a `.import` has done so far, across the windows of the file.
struct Load<'a> {
    /// How the file is read.
    reading: &'a Reading<'a>,
    /// The file name as the person typed it, for the messages.
    named: &'a str,
    /// How many rows of `--skip` have still to be dropped.
    skip_left: usize,
    /// The table and statement, once the first row that is not skipped has been reached.
    target: Option<Target>,
    /// Whether the import was stopped by the table: a view, or a file with no header.
    refused: bool,
    /// How many rows went in.
    added: usize,
    /// How many rows the table refused.
    errors: usize,
    /// How many row separators the file held.
    lines: usize,
}

impl<'a> Load<'a> {
    /// Returns the state of an import that has read nothing.
    ///
    /// @param reading - how the file is read
    /// @param named - the file name as the person typed it
    fn new(reading: &'a Reading<'a>, named: &'a str) -> Load<'a> {
        Load {
            reading,
            named,
            skip_left: reading.skip,
            target: None,
            refused: false,
            added: 0,
            errors: 0,
            lines: 0,
        }
    }

    /// Takes the complete rows of one window: drops the rows `--skip` names, creates the table
    /// from the header when there is none, and inserts the rest in batches.
    ///
    /// **A fault in a row that is skipped is still reported:** the reference reads the row to skip
    /// it. The view check and the check for the table wait for the first row that is not
    /// skipped, so the warnings for the rows before it come first, as they did when the file was
    /// read whole. Answers whether to read on.
    ///
    /// @param shell - the shell whose database receives the rows
    /// @param rows - the window's complete rows
    /// @param lines - how many row separators those rows used
    fn take(&mut self, shell: &mut Shell, mut rows: Vec<Row<'_>>, lines: usize) -> bool {
        self.lines = self.lines.saturating_add(lines);
        let skipped = self.skip_left.min(rows.len());
        for row in rows.iter().take(skipped) {
            for note in &row.notes {
                warn(shell, &note_text(note, self.named));
            }
        }
        rows.drain(..skipped);
        self.skip_left -= skipped;
        if rows.is_empty() {
            return true;
        }
        if self.target.is_none() && !self.prepare(shell, &mut rows) {
            self.refused = true;
            return false;
        }
        for batch in rows.chunks(BATCH_ROWS) {
            self.insert_batch(shell, batch);
        }
        true
    }

    /// Checks the table the rows go into, creates it from the header when it is not there, and
    /// opens the transaction.
    ///
    /// @param shell - the shell whose database receives the rows
    /// @param rows - the rows that are not skipped, which lose the header when the table is made
    ///   from it
    fn prepare(&mut self, shell: &mut Shell, rows: &mut Vec<Row<'_>>) -> bool {
        let schema = self.reading.schema.unwrap_or("main");
        let table = self.reading.table;
        if is_view(shell, schema, table) {
            shell.complain(&format!(
                "Error: cannot modify {table} because it is a view"
            ));
            return false;
        }
        if !table_exists(shell, schema, table)
            && !create_from_header(shell, self.reading, self.named, rows)
        {
            return false;
        }
        self.target = Some(open_target(shell, self.reading));
        true
    }

    /// Inserts one batch of rows and counts what went in and what was refused.
    ///
    /// @param shell - the shell whose database receives the rows
    /// @param rows - the batch
    fn insert_batch(&mut self, shell: &mut Shell, rows: &[Row<'_>]) {
        let Some(target) = self.target.as_ref() else {
            return;
        };
        let (added, errors) = insert_batch(shell, target, rows, self.named);
        self.added = self.added.saturating_add(added);
        self.errors = self.errors.saturating_add(errors);
    }

    /// Ends the import: commits the transaction and says what was done when `-v` asked for it.
    ///
    /// A file with no rows after the skipped ones still has its table checked, so an empty file
    /// is reported and a table that exists is left as it is.
    ///
    /// @param shell - the shell whose database received the rows
    fn finish(mut self, shell: &mut Shell) {
        if self.target.is_none() && !self.refused {
            let mut none = Vec::new();
            self.refused = !self.prepare(shell, &mut none);
        }
        if let Some(target) = self.target.take() {
            if target.began {
                if let Err(message) = shell.execute("COMMIT") {
                    shell.complain(&format!("Error: {message}"));
                }
            }
        }
        if !self.refused && self.reading.verbose > 0 {
            shell.say(&format!(
                "Added {} rows with {} errors using {} lines of input",
                self.added, self.errors, self.lines
            ));
        }
    }
}

/// Reads a file into a table.
///
/// **The file is read a block at a time and inserted a batch at a time**, so the memory an import
/// needs is a block and a batch and does not grow with the file. It parsed every row of the file
/// before it inserted the first, and 1,000,000 rows peaked at 536 to 679 MB where `sqlite3`
/// stays near 10 MB.
///
/// @param shell - the shell whose database receives the rows
/// @param arguments - the words after `.import`
pub fn import(shell: &mut Shell, arguments: &[&str]) {
    let Some(reading) = reading(shell, arguments) else {
        return;
    };
    let named = reading.path;
    // **Through the same confinement function every other path-taking dot
    // command calls (task-1979, H1).** This reads its file with `std::fs`, so
    // the confined VFS never sees the path and `--root` did not apply to it.
    let path = if named.starts_with('|') {
        named.to_string()
    } else {
        match crate::dot::confine_path(shell, named) {
            Some(path) => path,
            None => return,
        }
    };
    let Ok(input) = open_input(&path) else {
        let place = shell.error_location();
        shell.complain(&format!("{place} cannot open \"{named}\""));
        return;
    };
    if reading.verbose > 0 {
        let note = format!(
            "Column separator {}, row separator {}",
            crate::modes::c_quoted(&reading.separator.to_string()),
            crate::modes::c_quoted(&reading.row_separator.to_string())
        );
        shell.say(&note);
    }
    let separators = (reading.separator, reading.row_separator);
    let mut source = RowSource::new(input);
    let mut load = Load::new(&reading, named);
    let read = each_window(&mut source, separators, reading.quoted, |rows, lines| {
        load.take(shell, rows, lines)
    });
    if read.is_err() {
        // A file that cannot be read at all is reported as one that cannot be opened, as it was
        // when the file was read whole; one that fails part way is reported where it stopped.
        let place = shell.error_location();
        let what = if source.started { "read" } else { "open" };
        shell.complain(&format!("{place} cannot {what} \"{named}\""));
    }
    load.finish(shell);
}

/// Creates a table from the first row of the file and removes that row.
///
/// With no table, the first line is the header and the columns have no type, which
/// is what SQLite's shell does and is the only thing it can do without being told
/// the types. A file with no rows at all has no header, and SQLite says so rather
/// than creating a table with no columns. **The statement is written as the
/// reference writes it** (the name, a parenthesis, a line break, then the columns
/// separated by a comma and a space), because it is stored and printed by
/// `.schema`, and with `-v` it is printed as it is run.
///
/// @param shell - the shell whose database receives the table
/// @param reading - how the import reads its file
/// @param named - the file name as the person typed it
/// @param rows - the file's rows, which lose the first
fn create_from_header(
    shell: &mut Shell,
    reading: &Reading<'_>,
    named: &str,
    rows: &mut Vec<Row<'_>>,
) -> bool {
    if rows.is_empty() {
        shell.complain(&format!("{named}: empty file"));
        return false;
    }
    let header = rows.remove(0);
    for note in &header.notes {
        warn(shell, &note_text(note, named));
    }
    let (names, renamed) = column_names(&header.fields);
    if let Some(list) = renamed {
        // The message goes to standard error even though the import succeeds,
        // so it does not mark the run as failed.
        warn(
            shell,
            &format!("Columns renamed during .import {named} due to duplicates:\n{list}"),
        );
    }
    let columns: Vec<String> = names.iter().map(|name| quote_identifier(name)).collect();
    let qualified = format!(
        "{}.{}",
        quote_identifier(reading.schema.unwrap_or("main")),
        quote_identifier(reading.table)
    );
    let create = format!("CREATE TABLE {qualified}(\n{})", columns.join(", "));
    if reading.verbose > 0 {
        shell.say(&format!("{create}\n"));
    }
    if let Err(message) = shell.execute(&create) {
        shell.complain(&format!("{create}\n failed:\n{message}"));
        return false;
    }
    true
}

/// Prints a line to standard error without marking the run as failed.
///
/// @param shell - where the line goes
/// @param message - the line
fn warn(shell: &mut Shell, message: &str) {
    let failed = shell.failed;
    shell.complain(message);
    shell.failed = failed;
}

/// Returns the column names a header row makes, and the renames it needed.
///
/// **Names that repeat are renamed the way SQLite's shell renames them.** An
/// empty name becomes `?`. When two names are the same ignoring ASCII case,
/// every name in the group gets `_` and its one based column number: `a,a,b`
/// gives `a_1`, `a_2`, `b`. The number is padded with zeros as far as it takes
/// for no new name to equal another name in the header, so a header that
/// already holds `a_1` gives `a_01` and `a_02`. Names that are not repeated are
/// left alone. The second value is the list SQLite prints under "Columns
/// renamed", one `"old" to "new"` per renamed column.
///
/// @param header - the fields of the file's first row
fn column_names(header: &[Cow<'_, str>]) -> (Vec<String>, Option<String>) {
    let names: Vec<String> = header
        .iter()
        .map(|name| {
            if name.is_empty() {
                "?".to_string()
            } else {
                name.to_string()
            }
        })
        .collect();
    let repeated: Vec<bool> = names
        .iter()
        .enumerate()
        .map(|(at, name)| {
            names
                .iter()
                .enumerate()
                .any(|(other, held)| other != at && held.eq_ignore_ascii_case(name))
        })
        .collect();
    if !repeated.iter().any(|flag| *flag) {
        return (names, None);
    }
    let digits = names.len().to_string().len();
    let zeros = (0..)
        .find(|zeros| !collides(&names, &repeated, *zeros, digits))
        .unwrap_or(0);
    let mut renamed = Vec::new();
    let mut result = Vec::with_capacity(names.len());
    for (at, name) in names.iter().enumerate() {
        if repeated.get(at).copied().unwrap_or(false) {
            let new = format!("{name}_{}{}", "0".repeat(zeros), at + 1);
            renamed.push(format!(
                "\"{}\" to \"{}\"",
                name.replace('"', "\"\""),
                new.replace('"', "\"\"")
            ));
            result.push(new);
        } else {
            result.push(name.clone());
        }
    }
    (result, Some(renamed.join(",\n")))
}

/// Reports whether renaming with this many extra zeros gives two columns the
/// same name, ignoring ASCII case.
///
/// @param names - the header's names, with `?` for an empty one
/// @param repeated - which of them are renamed
/// @param zeros - how many zeros go before the column number
/// @param digits - how wide the column number is padded when testing
fn collides(names: &[String], repeated: &[bool], zeros: usize, digits: usize) -> bool {
    let candidates: Vec<String> = names
        .iter()
        .enumerate()
        .map(|(at, name)| {
            if repeated.get(at).copied().unwrap_or(false) {
                format!("{name}_{}{:0>digits$}", "0".repeat(zeros), at + 1)
            } else {
                name.clone()
            }
        })
        .collect();
    candidates.iter().enumerate().any(|(at, name)| {
        candidates
            .iter()
            .enumerate()
            .any(|(other, held)| other != at && held.eq_ignore_ascii_case(name))
    })
}

/// Opens the transaction the rows go in, and returns the statement that inserts them.
///
/// One prepared statement for the whole file. Compiling an `INSERT` per row is
/// the obvious way to write this and it is the wrong one: the statement is
/// identical every time, and preparing it five thousand times is five thousand
/// compilations of the same text.
///
/// The rows are inserted in a transaction of their own when none is open, and
/// the transaction is committed even when some rows were rejected, because the
/// rows that went in are what the person asked for.
///
/// @param shell - the shell whose database receives the rows
/// @param reading - how the import reads its file
fn open_target(shell: &mut Shell, reading: &Reading<'_>) -> Target {
    let schema = reading.schema.unwrap_or("main");
    let table = reading.table;
    let none = Target {
        columns: 0,
        sql: String::new(),
        began: false,
    };
    let columns = column_count(shell, schema, table);
    if columns == 0 {
        return none;
    }
    let marks: Vec<&str> = std::iter::repeat_n("?", columns).collect();
    let target = match reading.schema {
        Some(schema) => format!("{}.{}", quote_identifier(schema), quote_identifier(table)),
        None => quote_identifier(table),
    };
    let sql = format!("INSERT INTO {target} VALUES({})", marks.join(","));
    if reading.verbose > 1 {
        shell.say(&format!(
            "Insert using: INSERT INTO {} VALUES({})",
            quote_identifier(table),
            marks.join(",")
        ));
    }
    let own_transaction = shell.connection().autocommit().unwrap_or(false);
    if own_transaction {
        if let Err(message) = shell.execute("BEGIN") {
            shell.complain(&format!("Error: {message}"));
            return none;
        }
    }
    Target {
        columns,
        sql,
        began: own_transaction,
    }
}

/// Inserts a batch of parsed rows, reporting each line that will not go in.
///
/// Returns how many rows went in and how many were refused.
///
/// @param shell - the shell whose database receives the rows
/// @param target - the statement and the table's width
/// @param rows - the batch of the file's rows after the header
/// @param named - the file name as the person typed it, for the messages
fn insert_batch(
    shell: &mut Shell,
    target: &Target,
    rows: &[Row<'_>],
    named: &str,
) -> (usize, usize) {
    if target.columns == 0 {
        return (0, 0);
    }
    let complaints = fill(
        &shell.connection(),
        &target.sql,
        rows,
        target.columns,
        named,
    );
    let mut errors = 0usize;
    for complaint in &complaints {
        if complaint.error {
            errors += 1;
            shell.complain(&complaint.text);
            // The first rejected row's refusal is what the status of the whole
            // import is read from: a duplicate key is `constraint`, and the
            // verb used to call every rejected row `syntax`.
            if shell.first_error.is_none() {
                shell.first_error = complaint.cause.clone();
            }
        } else {
            warn(shell, &complaint.text);
        }
    }
    (rows.len().saturating_sub(errors), errors)
}

/// Binds and runs every row, returning what there was to say about each.
///
/// It borrows the connection for the whole loop, which is why it reports rather
/// than complains: the shell cannot be borrowed mutably while a statement it
/// prepared is alive.
///
/// @param connection - the connection the import's transaction is on
/// @param sql - the `INSERT` with one parameter for each column
/// @param rows - the file's rows after the header
/// @param columns - how many columns the table has
/// @param named - the file name as the person typed it
fn fill(
    connection: &Connection<'_>,
    sql: &str,
    rows: &[Row<'_>],
    columns: usize,
    named: &str,
) -> Vec<Complaint> {
    let mut statement = match connection.prepare(sql) {
        Ok(statement) => statement,
        Err(error) => {
            return vec![Complaint {
                error: true,
                text: error.message().to_string(),
                cause: Some(error),
            }]
        }
    };
    if all_at_once(&mut statement, rows, columns) {
        return Vec::new();
    }
    let mut complaints = Vec::new();
    for row in rows {
        for note in &row.notes {
            complaints.push(Complaint {
                error: false,
                text: note_text(note, named),
                cause: None,
            });
        }
        if let Some(text) = width_warning(row, columns, named) {
            complaints.push(Complaint {
                error: false,
                text,
                cause: None,
            });
        }
        if let Err(error) = insert_row(&mut statement, row, columns) {
            let text = format!("{named}:{}: INSERT failed: {}", row.line, error.message());
            complaints.push(Complaint {
                error: true,
                text,
                cause: Some(error),
            });
        }
    }
    complaints
}

/// Returns the line `sqlite3` prints for a fault in a quoted field.
///
/// @param note - the fault
/// @param named - the file name as the person typed it
fn note_text(note: &Note, named: &str) -> String {
    match note.kind {
        NoteKind::Unescaped => format!("{named}:{}: unescaped \" character", note.line),
        NoteKind::Unterminated => format!("{named}:{}: unterminated \"-quoted field", note.line),
    }
}

/// Returns SQLite's warning for a row with too few or too many fields.
///
/// @param row - the row
/// @param columns - how many columns the table has
/// @param named - the file name as the person typed it
fn width_warning(row: &Row<'_>, columns: usize, named: &str) -> Option<String> {
    let found = row.fields.len();
    let what = match found.cmp(&columns) {
        std::cmp::Ordering::Less => "filling the rest with NULL",
        std::cmp::Ordering::Greater => "extras ignored",
        std::cmp::Ordering::Equal => return None,
    };
    Some(format!(
        "{named}:{}: expected {columns} columns but found {found} - {what}",
        row.line
    ))
}

/// Binds one row's fields and runs the insert.
///
/// A field past the last column is not bound, and a column the row has no field
/// for stays NULL because every row starts from nothing bound.
///
/// @param statement - the prepared insert
/// @param row - the row
/// @param columns - how many columns the table has
fn insert_row(
    statement: &mut Statement<'_>,
    row: &Row<'_>,
    columns: usize,
) -> Result<(), inillucent_base::DbError> {
    statement.reset();
    statement.clear_bindings();
    for (position, field) in row.fields.iter().take(columns).enumerate() {
        statement.bind_text(position as u32 + 1, field)?;
    }
    loop {
        match statement.step() {
            Ok(true) => continue,
            Ok(false) => return Ok(()),
            Err(error) => {
                statement.reset();
                return Err(error);
            }
        }
    }
}

/// Inserts every row as one statement, when the engine allows it, and reports
/// whether it did.
///
/// **One statement for the file instead of one a row** (task-2191). The rows
/// are what `insert_row` would bind: each field as text. When the one statement
/// fails it has written nothing, and the caller runs the rows one at a time to
/// find the lines that will not go in, as it always did. A file with a row of
/// another width is left to the rows one at a time too, because the warning for
/// that row is printed there.
///
/// @param statement - the prepared insert, inside the import's transaction
/// @param rows - the parsed rows
/// @param columns - how many columns the table has
fn all_at_once(statement: &mut Statement<'_>, rows: &[Row<'_>], columns: usize) -> bool {
    if rows.len() < 2
        || rows
            .iter()
            .any(|row| row.fields.len() != columns || !row.notes.is_empty())
    {
        return false;
    }
    if !statement.can_run_rows_at_once().unwrap_or(false) {
        return false;
    }
    let values: Vec<Vec<inillucent_tree::datum::OwnedDatum>> = rows
        .iter()
        .map(|row| {
            row.fields
                .iter()
                .map(|field| inillucent_tree::datum::OwnedDatum::Text(field.as_bytes().to_vec()))
                .collect()
        })
        .collect();
    matches!(statement.run_rows_at_once(values), Ok(Some(_)))
}

/// Reports whether a table is already there.
///
/// @param shell - the shell whose database is asked
/// @param schema - the schema to look in
/// @param table - the table or view name
fn table_exists(shell: &Shell, schema: &str, table: &str) -> bool {
    let sql = format!(
        "SELECT count(*) FROM pragma_table_info('{}', '{}')",
        table.replace('\'', "''"),
        schema.replace('\'', "''")
    );
    shell.scalar(&sql).is_some_and(|count| count != "0")
}

/// Reports whether a name is a view, which a row cannot be inserted into.
///
/// @param shell - the shell whose database is asked
/// @param schema - the schema to look in
/// @param table - the name
fn is_view(shell: &Shell, schema: &str, table: &str) -> bool {
    let sql = format!(
        "SELECT count(*) FROM {}.sqlite_master WHERE type = 'view' AND name = '{}'",
        quote_identifier(schema),
        table.replace('\'', "''")
    );
    shell.scalar(&sql).is_some_and(|count| count != "0")
}

/// Returns how many columns a table has, which is how many fields a row needs.
///
/// SQLite asks the table rather than the first row of the file, so a file
/// whose rows are all shorter than the table pads every one of them.
///
/// @param shell - the shell whose database is asked
/// @param schema - the schema to look in
/// @param table - the table or view name
fn column_count(shell: &Shell, schema: &str, table: &str) -> usize {
    let sql = format!(
        "SELECT count(*) FROM pragma_table_info('{}', '{}')",
        table.replace('\'', "''"),
        schema.replace('\'', "''")
    );
    shell
        .scalar(&sql)
        .and_then(|count| count.parse::<usize>().ok())
        .unwrap_or(0)
}

/// Splits a file into rows of fields.
///
/// A quoted field may hold newlines, so this walks the whole text rather than
/// splitting on lines first - which is the bug in every import that splits on
/// `\n` and then on the separator.
///
/// **Every row separator ends a row, so a blank line is a row with one empty
/// field.** SQLite reads it that way and warns that the row is short. Only the
/// end of the file ends the last row without a separator.
///
/// **A carriage return is dropped only when it is the last character before the
/// row separator.** That is the other half of a Windows line ending. A carriage
/// return anywhere else is data, and a file whose lines end in a bare carriage
/// return is one long row, as SQLite reads it. The drop is part of reading CSV,
/// so `--ascii` does not do it.
///
/// **Characters are copied a run at a time.** `run` is where the characters
/// that go into the field unchanged began, and they are copied in one piece when
/// a separator, a quote or the end of the row ends them. Pushing each
/// character grew every field's string from nothing, and the growing was 13% of
/// importing a 300,000 row file (task-2191).
///
/// @param text - the file
/// @param separator - what ends a field
/// @param quoted - whether a field may be quoted
/// @param row_separator - what ends a row
#[cfg(test)]
fn parse(text: &str, separator: char, quoted: bool, row_separator: char) -> Vec<Row<'_>> {
    parse_window(text, (separator, row_separator), quoted, 1, true).rows
}

/// The rows a window of the file held, and how much of the window they used.
struct Window<'a> {
    /// The rows that ended inside the window.
    rows: Vec<Row<'a>>,
    /// How many bytes of the window the rows used, which is where the next window starts.
    consumed: usize,
    /// The line of the file the next window starts on.
    line: usize,
}

/// Splits the start of a file into the rows that are complete, and says where they end.
///
/// **A row is complete when its row separator has been read outside a quoted field.** Unless
/// `last` says the window ends the file, a row that has not ended, and a quoted field that runs
/// to the end of the window, are left alone and not reported: the caller reads more of the file
/// and gives the same row again, from its first character. So a row of any length is read whole,
/// the fields are the ones [`parse`] gives for the whole file, and the line numbers continue from
/// `start_line`. For a window that is the whole file this is `parse`.
///
/// @param text - the window: the file from the start of a row
/// @param separators - what ends a field, and what ends a row
/// @param quoted - whether a field may be quoted
/// @param start_line - the line of the file the window starts on
/// @param last - whether the window ends the file
fn parse_window(
    text: &str,
    separators: (char, char),
    quoted: bool,
    start_line: usize,
    last: bool,
) -> Window<'_> {
    let (separator, row_separator) = separators;
    let mut rows = Vec::new();
    let mut row: Vec<Cow<'_, str>> = Vec::new();
    let mut notes: Vec<Note> = Vec::new();
    let mut field = String::new();
    let mut characters = text.char_indices().peekable();
    let mut anything = false;
    let mut run = 0usize;
    let mut line = start_line;
    let mut row_line = start_line;
    let mut consumed = 0usize;
    let mut cut_short = false;
    while let Some((at, character)) = characters.next() {
        // The line count follows every row separator, quoted or not, because
        // SQLite's does: a quoted field with a newline in it moves the next
        // row's line number down by one.
        if character == row_separator {
            line += 1;
        }
        match character {
            '"' if quoted && field.is_empty() && run == at => {
                let (value, term, ran_out) = read_quoted(
                    &mut characters,
                    (separator, row_separator),
                    &mut line,
                    &mut notes,
                    last,
                );
                if ran_out {
                    cut_short = true;
                    break;
                }
                row.push(Cow::Owned(value));
                anything = true;
                run = characters.peek().map_or(text.len(), |&(next, _)| next);
                match term {
                    Some(ended) if ended == separator => {}
                    Some(_) => {
                        let width = row.len();
                        let fields = core::mem::replace(&mut row, Vec::with_capacity(width));
                        rows.push(Row {
                            line: row_line,
                            fields,
                            notes: core::mem::take(&mut notes),
                        });
                        anything = false;
                        row_line = line;
                        consumed = run;
                    }
                    None => {
                        rows.push(Row {
                            line: row_line,
                            fields: core::mem::take(&mut row),
                            notes: core::mem::take(&mut notes),
                        });
                        anything = false;
                        consumed = text.len();
                    }
                }
            }
            _ if character == separator => {
                row.push(finished(&mut field, text, run, at));
                anything = true;
                run = at + character.len_utf8();
            }
            _ if character == row_separator => {
                let end = if quoted {
                    before_carriage_return(text, run, at)
                } else {
                    at
                };
                row.push(finished(&mut field, text, run, end));
                let width = row.len();
                let fields = core::mem::replace(&mut row, Vec::with_capacity(width));
                rows.push(Row {
                    line: row_line,
                    fields,
                    notes: core::mem::take(&mut notes),
                });
                anything = false;
                run = at + character.len_utf8();
                row_line = line;
                consumed = run;
            }
            _ => anything = true,
        }
    }
    if cut_short || !last {
        return Window {
            rows,
            consumed,
            line: row_line,
        };
    }
    let tail = finished(&mut field, text, run, text.len());
    if anything || !tail.is_empty() || !row.is_empty() {
        row.push(tail);
        rows.push(Row {
            line: row_line,
            fields: row,
            notes,
        });
    }
    Window {
        rows,
        consumed: text.len(),
        line: row_line,
    }
}

/// Reads the rest of a quoted field, the opening quote already taken.
///
/// **A port of `csv_read_one_field` in the reference shell, quoted branch.** A
/// doubled quote is one quote. A quote closes the field only when the next
/// character is the column separator, the row separator or the end of the file,
/// or is a carriage return before the row separator. Any other quote is data,
/// and is reported as unescaped. So `"x"y,1` is the field `x"y,1` and all that
/// follows it up to the next closing quote, and the end of the file inside a
/// quoted field is reported as unterminated. The characters after a stray quote
/// were previously joined to the text before it, which is a different value.
///
/// Returns the field and what ended it: none for the end of the file. The last value is true when
/// the text ended before the field did and the text is not the end of the file, which means the
/// field has to be read again from more of the file.
///
/// @param characters - the file after the opening quote
/// @param separators - the column separator and the row separator
/// @param line - the line the reader has reached, which a row separator advances
/// @param notes - where faults are recorded
/// @param last - whether the text ends the file
fn read_quoted(
    characters: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    separators: (char, char),
    line: &mut usize,
    notes: &mut Vec<Note>,
    last: bool,
) -> (String, Option<char>, bool) {
    let (separator, row_separator) = separators;
    let start_line = *line;
    let mut value = String::new();
    let mut previous: Option<char> = None;
    let mut before_previous: Option<char> = None;
    loop {
        let current = characters.next().map(|(_, character)| character);
        // A quote at the very end of a window may be the closing one or the first of a doubled
        // pair, and only the character after it says which, so the field is read again.
        if current.is_none() && !last {
            return (value, None, true);
        }
        if current == Some(row_separator) {
            *line += 1;
        }
        if current == Some('"') && previous == Some('"') {
            previous = None;
            continue;
        }
        let closes = (current == Some(separator) && previous == Some('"'))
            || (current == Some(row_separator) && previous == Some('"'))
            || (current == Some(row_separator)
                && previous == Some('\r')
                && before_previous == Some('"'))
            || (current.is_none() && previous == Some('"'));
        if closes {
            // Drop everything back to and including the quote that closed it.
            while let Some(removed) = value.pop() {
                if removed == '"' {
                    break;
                }
            }
            return (value, current, false);
        }
        if previous == Some('"') && current != Some('\r') {
            notes.push(Note {
                line: *line,
                kind: NoteKind::Unescaped,
            });
        }
        let Some(character) = current else {
            notes.push(Note {
                line: start_line,
                kind: NoteKind::Unterminated,
            });
            return (value, None, false);
        };
        value.push(character);
        before_previous = previous;
        previous = Some(character);
    }
}

/// Returns where a field ends when a row separator follows it: one byte
/// earlier if the field ends in a carriage return.
///
/// @param text - the file
/// @param run - where the field's uncopied characters began
/// @param at - where the row separator is
fn before_carriage_return(text: &str, run: usize, at: usize) -> usize {
    match text.get(run..at) {
        Some(tail) if tail.ends_with('\r') => at - 1,
        _ => at,
    }
}

/// Ends a field: the run of text it holds, borrowed from the file when nothing
/// was copied into the field before it, or the copied text with the run added.
///
/// **Most fields are borrowed.** Only a quoted field needs a string of its own;
/// giving every field one was an allocation a field, 7% of importing a 300,000
/// row file (task-2191).
///
/// @param field - the text copied so far, emptied by this
/// @param text - the whole file
/// @param run - where the run of characters not yet copied began
/// @param at - where the field ends
fn finished<'a>(field: &mut String, text: &'a str, run: usize, at: usize) -> Cow<'a, str> {
    let tail = text.get(run..at).unwrap_or_default();
    if field.is_empty() {
        return Cow::Borrowed(tail);
    }
    field.push_str(tail);
    Cow::Owned(core::mem::take(field))
}

/// Returns an identifier quoted the way SQL wants it.
fn quote_identifier(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns only the fields of each parsed row.
    fn fields<'a>(rows: &'a [Row<'_>]) -> Vec<Vec<&'a str>> {
        rows.iter()
            .map(|row| row.fields.iter().map(|field| field.as_ref()).collect())
            .collect()
    }

    /// Plain fields split on the separator.
    #[test]
    fn fields_split_on_the_separator() {
        let rows = parse("a|b\nc|d\n", '|', false, '\n');
        assert_eq!(fields(&rows), vec![vec!["a", "b"], vec!["c", "d"]]);
    }

    /// A quoted field may hold the separator and a newline.
    #[test]
    fn a_quoted_field_holds_anything() {
        let rows = parse("\"a,b\",c\n\"line\none\",d\n", ',', true, '\n');
        assert_eq!(
            fields(&rows),
            vec![vec!["a,b", "c"], vec!["line\none", "d"]]
        );
    }

    /// A doubled quote inside a quoted field is one quote.
    #[test]
    fn a_doubled_quote_is_one_quote() {
        let rows = parse("\"say \"\"hi\"\"\",x\n", ',', true, '\n');
        assert_eq!(fields(&rows), vec![vec!["say \"hi\"", "x"]]);
    }

    /// A carriage return is dropped only where it ends a row. A quote that is
    /// not followed by a separator is data, so `"q"x,` does not close its field
    /// and the field runs on to the end of the file, as the reference reads it.
    #[test]
    fn a_carriage_return_goes_only_at_the_end_of_a_row() {
        let rows = parse("a\r,b\r\n\"q\"x,\"\"\r\n\"open", ',', true, '\n');
        assert_eq!(
            fields(&rows),
            vec![vec!["a\r", "b"], vec!["q\"x,\"\r\n\"open"]]
        );
        assert_eq!(rows.last().map(|row| row.notes.len()), Some(3));
    }

    /// A quote after the closing quote is reported and kept, and the field goes on.
    #[test]
    fn a_stray_quote_is_kept_and_reported() {
        let rows = parse("a,b\n\"x\"y,1\n2,3\n", ',', true, '\n');
        assert_eq!(fields(&rows)[1], vec!["x\"y,1\n2,3\n"]);
        let kinds: Vec<usize> = rows[1].notes.iter().map(|note| note.line).collect();
        assert_eq!(kinds, vec![2, 2]);
    }

    /// A quote that closes a field only does so before a separator, a row
    /// separator or the end of the file, and a doubled quote is one quote.
    #[test]
    fn a_closing_quote_ends_the_field_at_a_separator() {
        let rows = parse("\"a\"\"b\",\"c\"\n", ',', true, '\n');
        assert_eq!(fields(&rows), vec![vec!["a\"b", "c"]]);
        assert!(rows[0].notes.is_empty());
    }

    /// Lines that end in a bare carriage return are one row.
    #[test]
    fn a_bare_carriage_return_does_not_end_a_row() {
        let rows = parse("a,b\r1,2\r3,4\r", ',', true, '\n');
        assert_eq!(fields(&rows), vec![vec!["a", "b\r1", "2\r3", "4\r"]]);
    }

    /// A last line with no newline is still a row.
    #[test]
    fn a_missing_final_newline_is_still_a_row() {
        let rows = parse("a|b", '|', false, '\n');
        assert_eq!(fields(&rows), vec![vec!["a", "b"]]);
    }

    /// A blank line is a row with one empty field, and the end of the file
    /// after the last newline is not a row.
    #[test]
    fn a_blank_line_is_a_row_of_one_empty_field() {
        let rows = parse("a,b\n\n1,2\n", ',', true, '\n');
        assert_eq!(
            fields(&rows),
            vec![vec!["a", "b"], vec![""], vec!["1", "2"]]
        );
    }

    /// Each row knows the line it began on, and a newline inside quotes counts.
    #[test]
    fn rows_are_numbered_by_the_line_they_begin_on() {
        let rows = parse("a,b\n\"x\ny\",1\n2,3\n", ',', true, '\n');
        let lines: Vec<usize> = rows.iter().map(|row| row.line).collect();
        assert_eq!(lines, vec![1, 2, 4]);
    }

    /// `--ascii` reads the unit and record separators `.mode ascii` writes.
    ///
    /// A newline is ordinary text there, which is the point of the mode: a
    /// field may hold one without being quoted.
    #[test]
    fn ascii_separators_leave_a_newline_as_data() {
        let text = "a\u{1f}line\none\u{1e}c\u{1f}d\u{1e}";
        let rows = parse(text, UNIT_SEPARATOR, false, RECORD_SEPARATOR);
        assert_eq!(fields(&rows), vec![vec!["a", "line\none"], vec!["c", "d"]]);
    }

    /// Returns every row of a file as a window at a time reads it: the line it began on, its
    /// fields, and how many faults it had.
    ///
    /// @param bytes - the file
    /// @param block - how many bytes each read asks for
    /// @param quoted - whether a field may be quoted
    fn windowed(bytes: &[u8], block: usize, quoted: bool) -> Vec<(usize, Vec<String>, usize)> {
        let mut source = RowSource::new(Box::new(std::io::Cursor::new(bytes.to_vec())));
        source.block = block;
        let mut seen = Vec::new();
        each_window(&mut source, (',', '\n'), quoted, |rows, _| {
            for row in rows {
                let fields = row.fields.iter().map(|field| field.to_string()).collect();
                seen.push((row.line, fields, row.notes.len()));
            }
            true
        })
        .expect("a cursor reads");
        seen
    }

    /// Reading a file a window at a time gives the rows, the line numbers and the faults that
    /// reading it whole gives, for every block size, including a block that ends inside a
    /// quoted field, between the two quotes of a doubled quote, between a carriage return and a
    /// line feed, and inside a character of more than one byte.
    #[test]
    fn windows_give_the_rows_of_the_whole_file() {
        let files = [
            "a,b\n1,2\n3,4\n",
            "a,b\r\n1,2\r\n3,4",
            "id,note\n1,\"line\none\"\n2,\"say \"\"hi\"\", ok\"\n3,plain\n",
            "x,\"unterminated\nstill going",
            "x,\"a\"b\",c\n1,2\n",
            "caf\u{e9},\u{1f600},\u{4e2d}\u{6587}\n\u{1f600},2\n",
            "\n\n,\n,,\n",
            "\"\",\"\"\"\",\"a\"\"\"\n1,2\n",
            "one\n",
            "",
        ];
        for text in files {
            let whole = parse(text, ',', true, '\n');
            let expected: Vec<(usize, Vec<String>, usize)> = whole
                .iter()
                .map(|row| {
                    (
                        row.line,
                        row.fields.iter().map(|field| field.to_string()).collect(),
                        row.notes.len(),
                    )
                })
                .collect();
            for block in [1usize, 2, 3, 5, 8, 13, 64, 1 << 20] {
                assert_eq!(
                    windowed(text.as_bytes(), block, true),
                    expected,
                    "{text:?} read {block} bytes at a time"
                );
            }
        }
    }

    /// A byte order mark is dropped from the start of the file, and bytes that are not UTF-8
    /// come through as the same stand ins whatever the block size.
    #[test]
    fn windows_drop_the_byte_order_mark_and_keep_bytes_that_are_not_utf8() {
        let mut file = vec![0xEF, 0xBB, 0xBF];
        file.extend_from_slice(b"a,caf\xE9\n\xE4\xB8,2\n");
        let whole = crate::raw::decode(file.get(3..).unwrap_or_default());
        let expected: Vec<(usize, Vec<String>, usize)> = parse(&whole, ',', true, '\n')
            .iter()
            .map(|row| {
                (
                    row.line,
                    row.fields.iter().map(|field| field.to_string()).collect(),
                    row.notes.len(),
                )
            })
            .collect();
        for block in [1usize, 2, 3, 4, 7, 1 << 20] {
            assert_eq!(windowed(&file, block, true), expected, "block {block}");
        }
    }

    /// A row longer than the block is read whole.
    #[test]
    fn a_row_longer_than_the_block_is_read_whole() {
        let field = "x".repeat(10_000);
        let text = format!("a,{field}\n1,2\n");
        let seen = windowed(text.as_bytes(), 16, true);
        assert_eq!(seen.len(), 2);
        assert_eq!(
            seen.first().map(|row| row.1.get(1).map(String::len)),
            Some(Some(10_000))
        );
    }

    /// Returns the names a header gives, as owned strings.
    fn names(header: &[&str]) -> (Vec<String>, Option<String>) {
        let fields: Vec<Cow<'_, str>> = header.iter().map(|name| Cow::Borrowed(*name)).collect();
        column_names(&fields)
    }

    /// Names that are not repeated are kept, and an empty one is `?`.
    #[test]
    fn unique_names_are_kept() {
        let (columns, renamed) = names(&["a", "", "c"]);
        assert_eq!(columns, vec!["a", "?", "c"]);
        assert_eq!(renamed, None);
    }

    /// A repeated name is renamed with its column number, ignoring case.
    #[test]
    fn repeated_names_get_their_column_number() {
        let (columns, renamed) = names(&["a", "A", "b"]);
        assert_eq!(columns, vec!["a_1", "A_2", "b"]);
        assert_eq!(
            renamed.as_deref(),
            Some("\"a\" to \"a_1\",\n\"A\" to \"A_2\"")
        );
    }

    /// A header that already holds the new name gets a zero in front of the number.
    #[test]
    fn a_new_name_never_equals_an_existing_one() {
        let (columns, _) = names(&["a", "a", "a_1", "", ""]);
        assert_eq!(columns, vec!["a_01", "a_02", "a_1", "?_04", "?_05"]);
    }
}

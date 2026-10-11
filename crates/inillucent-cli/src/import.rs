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
}

/// One row of the file, with the line of the file it began on.
struct Row<'a> {
    /// The line the row began on. The first line of the file is 1.
    line: usize,
    /// The row's fields.
    fields: Vec<Cow<'a, str>>,
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
/// Every option this command's own help text documents is understood here, and
/// an option it does not understand is refused by name. Reading an unknown
/// option as the file name is the failure this function exists to prevent: it
/// produces `cannot open "--esc"`, which sends a reader looking for a missing
/// file rather than at the flag they mistyped.
///
/// @param shell - what the defaults are read from, and what a complaint goes to
/// @param arguments - the words after `.import`
fn reading<'a>(shell: &mut Shell, arguments: &[&'a str]) -> Option<Reading<'a>> {
    let mut separator = if shell.layout.mode == Mode::Csv {
        ','
    } else {
        shell.layout.separator.chars().next().unwrap_or('|')
    };
    let mut quoted = true;
    let mut row_separator = '\n';
    let mut skip = 0usize;
    let mut positional: Vec<&'a str> = Vec::new();

    let mut index = 0;
    while index < arguments.len() {
        let Some(word) = arguments.get(index).copied() else {
            break;
        };
        index += 1;
        // An option that takes a value consumes the next word here, so that
        // word can never be mistaken for the file name further down.
        let value = if matches!(word, "--colsep" | "--rowsep" | "--skip") {
            let Some(taken) = arguments.get(index).copied() else {
                shell.complain(&format!("Error: {word} wants a value"));
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
            // SQLite takes the first character of a longer separator. An empty
            // one is refused rather than defaulted, because a separator that is
            // not the one asked for loads a file whose fields are all wrong and
            // reports nothing.
            "--colsep" | "--rowsep" => {
                let Some(character) = value.chars().next() else {
                    shell.complain(&format!("Error: {word} wants a character"));
                    return None;
                };
                if word == "--colsep" {
                    separator = character;
                } else {
                    row_separator = character;
                }
            }
            "--skip" => match value.parse::<usize>() {
                Ok(count) => skip = count,
                Err(_) => {
                    shell.complain("Error: --skip wants a number of rows");
                    return None;
                }
            },
            "-v" => {}
            // `--schema`, `--esc` and `--qesc` are in SQLite's help for this
            // command and are not built here. They are refused rather than
            // ignored: an escape character that is accepted and not applied
            // loads a file whose fields are silently wrong.
            other if other.starts_with('-') => {
                shell.complain(&format!("Error: .import does not take {other}"));
                return None;
            }
            other => positional.push(other),
        }
    }

    let (Some(path), Some(table)) = (positional.first(), positional.get(1)) else {
        shell.complain("Error: .import requires a file name and a table name");
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
    })
}

/// Reads a file as text, saying where it stopped being text.
///
/// **`read_to_string` answered "cannot open" for a file that opened fine**
/// (task-2066 section 4.2, item 24). A cp1252 CSV holding `1,Café` is a
/// file the reader can open, read and stat; what it cannot do is decode it,
/// and a message about opening sends the reader to check permissions and the
/// path. The same file with ASCII in it imports, which is what makes the
/// wrong message expensive: the difference between the two runs is one byte
/// of content and the message names neither content nor bytes.
///
/// The offset is the one `Utf8Error::valid_up_to` reports, which is the first
/// byte of the sequence that does not decode, so it points at the character a
/// person has to go and look at.
///
/// @param path - the file to read
fn read_text(path: &str) -> Result<String, String> {
    let mut bytes = std::fs::read(path).map_err(|error| error.to_string())?;
    // **A UTF-8 byte order mark is not data.** Excel and Notepad write one at
    // the start of a CSV file, and it was kept as the first three bytes of the
    // first field: the first column of a new table was named BOM plus `id`, so
    // `SELECT id` failed with "no such column", and into an existing table the
    // first value became the text BOM plus `1` instead of the integer 1.
    // SQLite's `.import` drops it.
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        bytes.drain(..3);
    }
    String::from_utf8(bytes).map_err(|error| {
        let at = error.utf8_error().valid_up_to();
        let byte = error
            .as_bytes()
            .get(at)
            .map(|held| format!("0x{held:02X}"))
            .unwrap_or_else(|| "the end of the file".to_string());
        format!(
            "it is not valid UTF-8; byte {at} is {byte}, which does not begin a character. This reads UTF-8 text only; convert the file first, with `iconv -f cp1252 -t utf-8` or the encoding it was written in."
        )
    })
}

/// Reads a file into a table.
///
/// @param shell - the shell whose database receives the rows
/// @param arguments - the words after `.import`
pub fn import(shell: &mut Shell, arguments: &[&str]) {
    let Some(reading) = reading(shell, arguments) else {
        return;
    };
    let (named, table) = (reading.path, reading.table);
    // **Through the same confinement function every other path-taking dot
    // command calls (task-1979, H1).** This reads its file with `std::fs`, so
    // the confined VFS never sees the path and `--root` did not apply to it.
    let Some(path) = crate::dot::confine_path(shell, named) else {
        return;
    };
    let text = match read_text(&path) {
        Ok(text) => text,
        Err(why) => {
            shell.complain(&format!("Error: cannot read \"{named}\": {why}"));
            return;
        }
    };
    let mut rows = parse(
        &text,
        reading.separator,
        reading.quoted,
        reading.row_separator,
    );
    rows.drain(..reading.skip.min(rows.len()));
    if !table_exists(shell, table) && !create_from_header(shell, table, named, &mut rows) {
        return;
    }
    insert(shell, table, &rows, named);
}

/// Creates a table from the first row of the file and removes that row.
///
/// With no table, the first line is the header and the columns are all text,
/// which is what SQLite's shell does and is the only thing it can do without
/// being told the types. A file with no rows at all has no header, and SQLite
/// says so rather than creating a table with no columns.
///
/// @param shell - the shell whose database receives the table
/// @param table - the table to create
/// @param named - the file name as the person typed it
/// @param rows - the file's rows, which lose the first
fn create_from_header(
    shell: &mut Shell,
    table: &str,
    named: &str,
    rows: &mut Vec<Row<'_>>,
) -> bool {
    if rows.is_empty() {
        shell.complain(&format!("{named}: empty file"));
        return false;
    }
    let header = rows.remove(0);
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
    let create = format!(
        "CREATE TABLE {} ({})",
        quote_identifier(table),
        columns.join(",")
    );
    if let Err(message) = shell.execute(&create) {
        shell.complain(&format!("Error: {message}"));
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

/// Inserts every parsed row, reporting each line that will not go in.
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
/// @param table - the table to insert into
/// @param rows - the file's rows after the header
/// @param named - the file name as the person typed it, for the messages
fn insert(shell: &mut Shell, table: &str, rows: &[Row<'_>], named: &str) {
    let columns = column_count(shell, table);
    if columns == 0 {
        return;
    }
    let marks: Vec<&str> = std::iter::repeat_n("?", columns).collect();
    let sql = format!(
        "INSERT INTO {} VALUES({})",
        quote_identifier(table),
        marks.join(",")
    );
    let own_transaction = shell.connection().autocommit().unwrap_or(false);
    if own_transaction {
        if let Err(message) = shell.execute("BEGIN") {
            shell.complain(&format!("Error: {message}"));
            return;
        }
    }
    let complaints = fill(&shell.connection(), &sql, rows, columns, named);
    for complaint in &complaints {
        if complaint.error {
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
    if own_transaction {
        if let Err(message) = shell.execute("COMMIT") {
            shell.complain(&format!("Error: {message}"));
        }
    }
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
    if rows.len() < 2 || rows.iter().any(|row| row.fields.len() != columns) {
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
/// @param table - the table or view name
fn table_exists(shell: &Shell, table: &str) -> bool {
    let sql = format!(
        "SELECT count(*) FROM sqlite_master WHERE type IN ('table','view') AND name = '{}'",
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
/// @param table - the table or view name
fn column_count(shell: &Shell, table: &str) -> usize {
    let sql = format!(
        "SELECT count(*) FROM pragma_table_info('{}')",
        table.replace('\'', "''")
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
fn parse(text: &str, separator: char, quoted: bool, row_separator: char) -> Vec<Row<'_>> {
    let mut rows = Vec::new();
    let mut row: Vec<Cow<'_, str>> = Vec::new();
    let mut field = String::new();
    let mut inside = false;
    let mut characters = text.char_indices().peekable();
    let mut anything = false;
    let mut run = 0usize;
    let mut line = 1usize;
    let mut row_line = 1usize;
    while let Some((at, character)) = characters.next() {
        // The line count follows every row separator, quoted or not, because
        // SQLite's does: a quoted field with a newline in it moves the next
        // row's line number down by one.
        if character == row_separator {
            line += 1;
        }
        if inside {
            if character == '"' {
                field.push_str(text.get(run..at).unwrap_or_default());
                if characters.peek().map(|&(_, next)| next) == Some('"') {
                    characters.next();
                    field.push('"');
                    run = at + 2;
                } else {
                    inside = false;
                    run = at + 1;
                }
            }
            continue;
        }
        match character {
            '"' if quoted && field.is_empty() && run == at => {
                inside = true;
                anything = true;
                run = at + 1;
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
                });
                anything = false;
                run = at + character.len_utf8();
                row_line = line;
            }
            _ => anything = true,
        }
    }
    let last = finished(&mut field, text, run, text.len());
    if anything || !last.is_empty() || !row.is_empty() {
        row.push(last);
        rows.push(Row {
            line: row_line,
            fields: row,
        });
    }
    rows
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

    /// A carriage return is dropped only where it ends a row, and text after a
    /// closing quote joins the field.
    #[test]
    fn a_carriage_return_goes_only_at_the_end_of_a_row() {
        let rows = parse("a\r,b\r\n\"q\"x,\"\"\r\n\"open", ',', true, '\n');
        assert_eq!(
            fields(&rows),
            vec![vec!["a\r", "b"], vec!["qx", ""], vec!["open"]]
        );
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

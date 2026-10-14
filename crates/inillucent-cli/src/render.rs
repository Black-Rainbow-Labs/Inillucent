//! The output modes: how a row becomes the text a person or a script reads.
//!
//! Invariant: a mode decides layout and nothing else. Every mode is handed the
//! same values, and none of them converts one - a blob prints as its bytes in
//! `list` and as an `x'...'` literal in `quote` because those are two ways of
//! writing the same value, not two values. The conversions themselves belong to
//! the engine, which is why nothing in this file parses or casts anything.
//!
//! The default is SQLite's: `list` mode, `|` between columns, headers off, and
//! NULL as the empty string. That last one is a genuinely bad default and it is
//! kept anyway, because a script written against `sqlite3` and pointed at this
//! shell has to see the same bytes.
//!
//! **A mode is a set of defaults for the same settings the `.mode` options
//! change.** `ascii`, `csv`, `tcl`, `quote` and `tabs` are all the `list` layout
//! with a different separator, row separator and way of writing a value, so one
//! function lays them out and the settings in [`Layout`] say how. The modes that
//! draw columns are in [`columnar`].

use inillucent_value::Value;

mod columnar;
pub mod stream;
pub mod text;

use text::escape_controls;

/// How rows are laid out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Columns separated by the separator, one row per line.
    #[default]
    List,
    /// Fixed-width columns, padded to the widest value.
    Column,
    /// One `name = value` line per column, a blank line between rows.
    Line,
    /// Comma-separated, quoted the way a spreadsheet expects.
    Csv,
    /// Columns separated by tabs.
    Tabs,
    /// Every value as an SQL literal.
    Quote,
    /// One `INSERT INTO` statement per row.
    Insert,
    /// A JSON array of objects.
    Json,
    /// A Markdown table.
    Markdown,
    /// A table drawn with `+` and `-`.
    Table,
    /// A table drawn with box-drawing characters.
    Box,
    /// An HTML table body.
    Html,
    /// Columns and rows separated by the ASCII unit and record separators.
    Ascii,
    /// A box whose values are written as SQL literals.
    Qbox,
    /// Values quoted for Tcl, separated by spaces.
    Tcl,
    /// Values quoted as C strings, separated by commas.
    C,
    /// Only the number of rows.
    Count,
    /// Nothing.
    Off,
    /// Values written as JSON, separated by commas.
    Jatom,
    /// One JSON object per row, with no array around them.
    Jobject,
    /// A table in the style of `psql`.
    Psql,
    /// Columns separated by two spaces, like `column`.
    Split,
}

impl Mode {
    /// Returns the mode a name selects.
    pub fn from_name(name: &str) -> Option<Mode> {
        match name.to_ascii_lowercase().as_str() {
            "list" => Some(Mode::List),
            "column" | "columns" => Some(Mode::Column),
            "line" | "lines" => Some(Mode::Line),
            "csv" => Some(Mode::Csv),
            "tabs" => Some(Mode::Tabs),
            "quote" => Some(Mode::Quote),
            "insert" => Some(Mode::Insert),
            "json" => Some(Mode::Json),
            "markdown" => Some(Mode::Markdown),
            "table" => Some(Mode::Table),
            "box" => Some(Mode::Box),
            "html" => Some(Mode::Html),
            "ascii" => Some(Mode::Ascii),
            "qbox" => Some(Mode::Qbox),
            "tcl" => Some(Mode::Tcl),
            "c" => Some(Mode::C),
            "count" => Some(Mode::Count),
            "off" => Some(Mode::Off),
            "jatom" => Some(Mode::Jatom),
            "jobject" => Some(Mode::Jobject),
            "psql" => Some(Mode::Psql),
            "split" => Some(Mode::Split),
            _ => None,
        }
    }

    /// Returns the name `.show` prints for this mode.
    pub fn name(self) -> &'static str {
        match self {
            Mode::List => "list",
            Mode::Column => "column",
            Mode::Line => "line",
            Mode::Csv => "csv",
            Mode::Tabs => "tabs",
            Mode::Quote => "quote",
            Mode::Insert => "insert",
            Mode::Json => "json",
            Mode::Markdown => "markdown",
            Mode::Table => "table",
            Mode::Box => "box",
            Mode::Html => "html",
            Mode::Ascii => "ascii",
            Mode::Qbox => "qbox",
            Mode::Tcl => "tcl",
            Mode::C => "c",
            Mode::Count => "count",
            Mode::Off => "off",
            Mode::Jatom => "jatom",
            Mode::Jobject => "jobject",
            Mode::Psql => "psql",
            Mode::Split => "split",
        }
    }

    /// Reports whether this mode draws columns.
    pub fn is_columnar(self) -> bool {
        matches!(
            self,
            Mode::Column
                | Mode::Markdown
                | Mode::Table
                | Mode::Box
                | Mode::Qbox
                | Mode::Psql
                | Mode::Split
        )
    }

    /// Reports whether this mode writes one line per row, joined by a separator.
    pub fn is_list(self) -> bool {
        matches!(
            self,
            Mode::List
                | Mode::Csv
                | Mode::Tabs
                | Mode::Quote
                | Mode::Ascii
                | Mode::Tcl
                | Mode::C
                | Mode::Jatom
        )
    }
}

/// How a text value is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Quote {
    /// As it is.
    #[default]
    Off,
    /// As an SQL literal.
    Sql,
    /// The way CSV writes a field.
    Csv,
    /// With the characters HTML needs escaped.
    Html,
    /// Between double quotes with Tcl escapes.
    Tcl,
    /// As a JSON string.
    Json,
    /// As an SQL literal, leaving out quotes the reader does not need.
    Relaxed,
}

impl Quote {
    /// Returns the style a `--quote` argument names.
    ///
    /// @param word - the argument
    pub fn from_name(word: &str) -> Option<Quote> {
        match word.to_ascii_lowercase().as_str() {
            "off" => Some(Quote::Off),
            "on" | "sql" => Some(Quote::Sql),
            "csv" => Some(Quote::Csv),
            "html" => Some(Quote::Html),
            "tcl" => Some(Quote::Tcl),
            "json" => Some(Quote::Json),
            "relaxed" => Some(Quote::Relaxed),
            _ => None,
        }
    }

    /// Returns the name `.mode` prints for a style.
    ///
    /// **`csv` prints as `hex` and `html` as `csv`**, because that is what the
    /// pinned reference prints: its table of names is one entry out of step with
    /// its list of styles from `csv` on. A script that reads the line back
    /// through `.mode` gets the style it had either way, and one that compares
    /// the text gets the same text.
    pub fn printed(self) -> &'static str {
        match self {
            Quote::Off => "off",
            Quote::Sql => "sql",
            Quote::Csv => "hex",
            Quote::Html => "csv",
            Quote::Tcl => "tcl",
            Quote::Json => "json",
            Quote::Relaxed => "relaxed",
        }
    }
}

/// How a blob is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlobQuote {
    /// The way the text quoting writes one.
    #[default]
    Auto,
    /// As the bytes.
    Text,
    /// As `x'..'`.
    Sql,
    /// As hexadecimal digits.
    Hex,
    /// As a Tcl string of octal escapes.
    Tcl,
    /// As a JSON string.
    Json,
    /// As its size.
    Size,
}

impl BlobQuote {
    /// Returns the style a `--blob-quote` argument names, and the name it prints as.
    ///
    /// @param word - the argument
    pub fn from_name(word: &str) -> Option<BlobQuote> {
        match word.to_ascii_lowercase().as_str() {
            "auto" => Some(BlobQuote::Auto),
            "text" => Some(BlobQuote::Text),
            "sql" => Some(BlobQuote::Sql),
            "hex" => Some(BlobQuote::Hex),
            "tcl" => Some(BlobQuote::Tcl),
            "json" => Some(BlobQuote::Json),
            "size" => Some(BlobQuote::Size),
            _ => None,
        }
    }

    /// Returns the name a style prints as.
    pub fn name(self) -> &'static str {
        match self {
            BlobQuote::Auto => "auto",
            BlobQuote::Text => "text",
            BlobQuote::Sql => "sql",
            BlobQuote::Hex => "hex",
            BlobQuote::Tcl => "tcl",
            BlobQuote::Json => "json",
            BlobQuote::Size => "size",
        }
    }
}

/// How a control character in a value is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Escape {
    /// Caret notation, which is what `ascii` is.
    #[default]
    Auto,
    /// As the byte.
    Off,
    /// Caret notation.
    Ascii,
    /// As the picture character of the control.
    Symbol,
}

/// Whether a long line is broken at word boundaries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WordWrap {
    /// At word boundaries, which is what a wrap does until it is told otherwise.
    #[default]
    Auto,
    /// At word boundaries, asked for.
    On,
    /// In the middle of whatever is there.
    Off,
}

/// Everything a mode needs that is not the rows themselves.
#[derive(Clone, Debug)]
pub struct Layout {
    /// Which mode.
    pub mode: Mode,
    /// What goes between columns, in the modes that use one.
    pub separator: String,
    /// What goes between rows.
    pub row_separator: String,
    /// What a NULL prints as.
    pub null: String,
    /// Whether to print a header line.
    pub headers: bool,
    /// The table name `insert` mode writes; empty means `tab`.
    pub table: String,
    /// The column widths `.width` fixed, if any. Negative aligns to the right and
    /// zero means as wide as the widest value.
    pub widths: Vec<i32>,
    /// Whether these lines are going to standard output.
    ///
    /// **Only `csv` reads it, and only on Windows**, where standard output is
    /// the one destination that translates a line feed on the way out. The
    /// bytes are in the comment at the end of [`separated`]. A file and a
    /// collecting caller both receive exactly what is written to them, so the
    /// second carriage return that makes standard output match the reference is
    /// wrong for both. The shell sets this from where its own output is currently
    /// going; the default is standard output, because that is where a `Layout`
    /// built by hand is printed.
    pub to_stdout: bool,
    /// How a text value is written.
    pub quote: Quote,
    /// How a title is written, when it is not written as the values are.
    pub title_quote: Option<Quote>,
    /// How a blob is written.
    pub blob_quote: BlobQuote,
    /// How a control character is written.
    pub escape: Escape,
    /// Wraps a column wider than this many characters; zero means never.
    pub wrap: usize,
    /// Whether a wrap prefers a word boundary.
    pub word_wrap: WordWrap,
    /// The screen width the output is for; `None` means as wide as the terminal.
    pub screen_width: Option<usize>,
    /// One letter per column: `L`, `C` or `R`.
    pub align: String,
    /// Whether `box` and `table` draw their outer edge.
    pub border: bool,
    /// The most characters shown of a value; zero means all.
    pub char_limit: usize,
    /// The most lines shown of a value; zero means all.
    pub line_limit: usize,
    /// The most characters shown of a title; zero means all.
    pub title_limit: usize,
    /// How many bytes of rows go into one `INSERT`; zero means one row each.
    pub multi_insert: usize,
    /// Whether JSONB text is shown as JSON text.
    pub text_jsonb: bool,
}

impl Default for Layout {
    /// Returns SQLite's defaults.
    fn default() -> Layout {
        Layout {
            mode: Mode::List,
            separator: "|".to_string(),
            row_separator: "\n".to_string(),
            null: String::new(),
            headers: false,
            table: String::new(),
            widths: Vec::new(),
            to_stdout: true,
            quote: Quote::Off,
            title_quote: None,
            blob_quote: BlobQuote::Auto,
            escape: Escape::Auto,
            wrap: 0,
            word_wrap: WordWrap::Auto,
            screen_width: Some(0),
            align: String::new(),
            border: true,
            char_limit: 0,
            line_limit: 0,
            title_limit: 0,
            multi_insert: 0,
            text_jsonb: false,
        }
    }
}

impl Layout {
    /// Returns the table name `insert` mode writes.
    pub fn table_name(&self) -> &str {
        if self.table.is_empty() {
            "tab"
        } else {
            &self.table
        }
    }

    /// Returns whether the lines `render` returns end a line when printed.
    ///
    /// **A row separator that does not end in a newline is the whole of the
    /// line's ending.** `render` hands back text with the separator on it, and the
    /// caller writes a newline after it only when this is true. `-newline '|'`
    /// and `.mode ascii` write no newline at all.
    pub fn lines_end_in_newline(&self) -> bool {
        !self.mode.is_list() || self.row_separator.ends_with('\n')
    }

    /// Returns what goes after a row of a list mode, short of the newline.
    fn row_end(&self) -> String {
        match self.row_separator.strip_suffix('\n') {
            Some(before) => before.to_string(),
            None => self.row_separator.clone(),
        }
    }
}

/// Renders a result set into the lines that should be printed.
///
/// It returns lines rather than writing, so the caller decides where they go -
/// which is what `.output` and `.once` need, and what makes this testable
/// without a file.
///
/// @param layout - the output settings
/// @param columns - the result's column names
/// @param rows - the result's rows
///
/// A row with no columns, which `PRAGMA incremental_vacuum` answers once per
/// page, prints as an empty line in the list, tabs, quote, csv, insert, json
/// and html modes, as the reference does. The column, line, markdown, table and
/// box modes have no cells to draw, so they print nothing, as the reference
/// does, and not an empty frame.
pub fn render(layout: &Layout, columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    let drawn_from_cells = layout.mode.is_columnar() || layout.mode == Mode::Line;
    if columns.is_empty() && drawn_from_cells {
        return Vec::new();
    }
    match layout.mode {
        mode if mode.is_list() => separated(layout, columns, rows),
        Mode::Line => lines(layout, columns, rows),
        Mode::Insert => inserts(layout, columns, rows),
        Mode::Json => json(layout, columns, rows),
        Mode::Jobject => objects(columns, rows),
        Mode::Html => html(layout, columns, rows),
        Mode::Count => vec![rows.len().to_string()],
        Mode::Off => Vec::new(),
        _ => columnar::render(layout, columns, rows),
    }
}

/// Returns a value as the settings write it, control characters made visible.
///
/// @param layout - the output settings
/// @param value - the cell
pub(crate) fn shown_cell(layout: &Layout, value: &Value<'static>) -> String {
    escape_controls(&cell_text(layout, value), layout.escape)
}

/// Returns a value as the settings write it, before control characters are made
/// visible.
///
/// A NULL is the `null` setting whatever the quoting. Text is written by `quote`,
/// numbers as the engine writes them, and a blob by `blob_quote` or, when that is
/// left to `auto`, in the style that goes with the text quoting.
///
/// @param layout - the output settings
/// @param value - the cell
fn cell_text(layout: &Layout, value: &Value<'static>) -> String {
    match value {
        Value::Null => layout.null.clone(),
        Value::Integer(number) => number.to_string(),
        Value::Real(_) => number_text(value),
        Value::Text(text) => quote_text(layout, text.raw()),
        Value::Blob(blob) => quote_blob(layout, blob.raw()),
    }
}

/// Writes a text value in the quoting style.
///
/// @param layout - the output settings
/// @param bytes - the text's bytes
fn quote_text(layout: &Layout, bytes: &[u8]) -> String {
    match layout.quote {
        Quote::Off => until_nul(bytes),
        Quote::Sql | Quote::Relaxed => sql_text(layout, bytes),
        Quote::Csv => csv_field(&until_nul(bytes), &layout.separator),
        Quote::Html => html_escape(&until_nul(bytes)),
        Quote::Tcl => tcl_string(&until_nul(bytes)),
        Quote::Json => format!(
            "\"{}\"",
            inillucent_base::json::escape_sql(&until_nul(bytes))
        ),
    }
}

/// Writes a text as an SQL literal.
///
/// With `--escape off` the text is written between plain quotes whatever it holds,
/// so a newline stays a newline instead of becoming a `unistr()` call.
///
/// @param layout - the output settings
/// @param bytes - the text's bytes
fn sql_text(layout: &Layout, bytes: &[u8]) -> String {
    if layout.escape == Escape::Off {
        return format!("'{}'", until_nul(bytes).replace('\'', "''"));
    }
    quote_literal(&Value::owned_text(bytes).unwrap_or(Value::Null))
}

/// Writes a blob in the blob quoting style.
///
/// @param layout - the output settings
/// @param bytes - the blob's bytes
fn quote_blob(layout: &Layout, bytes: &[u8]) -> String {
    let style = match layout.blob_quote {
        BlobQuote::Auto => match layout.quote {
            Quote::Sql | Quote::Relaxed => BlobQuote::Sql,
            Quote::Tcl => BlobQuote::Tcl,
            Quote::Json => BlobQuote::Json,
            _ => BlobQuote::Text,
        },
        chosen => chosen,
    };
    match style {
        BlobQuote::Sql => literal(&Value::owned_blob(bytes).unwrap_or(Value::Null)),
        BlobQuote::Hex => bytes.iter().map(|byte| format!("{byte:02X}")).collect(),
        BlobQuote::Tcl => tcl_octal(bytes),
        BlobQuote::Json => {
            let escaped: String = bytes.iter().map(|byte| format!("\\u{byte:04x}")).collect();
            format!("\"{escaped}\"")
        }
        BlobQuote::Size => format!("({}-byte blob)", bytes.len()),
        BlobQuote::Text | BlobQuote::Auto => {
            let text = until_nul(bytes);
            if layout.quote == Quote::Csv {
                csv_field(&text, &layout.separator)
            } else {
                text
            }
        }
    }
}

/// Writes a text as a Tcl string.
///
/// @param text - the text
fn tcl_string(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            control if u32::from(control) < 0x20 => {
                out.push_str(&format!("\\{:03o}", u32::from(control)));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// Writes bytes as a Tcl string of octal escapes.
///
/// @param bytes - the bytes
fn tcl_octal(bytes: &[u8]) -> String {
    let escapes: String = bytes.iter().map(|byte| format!("\\{byte:03o}")).collect();
    format!("\"{escapes}\"")
}

/// Returns the bytes up to the first NUL as text.
///
/// A value is printed as a C string, so a text or a blob stops at its first NUL.
///
/// @param bytes - the value's bytes
fn until_nul(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    crate::raw::decode(bytes.get(..end).unwrap_or(bytes))
}

/// Returns the text the engine writes a number as.
fn number_text(value: &Value<'static>) -> String {
    let cast = inillucent_value::cast::cast_value(
        value.clone(),
        inillucent_value::Affinity::Text,
        inillucent_value::TextEncoding::Utf8,
    );
    match cast {
        Ok(Value::Text(text)) => String::from_utf8_lossy(text.raw()).into_owned(),
        _ => String::new(),
    }
}

/// Returns the SQL literal a value would be written as.
pub fn literal(value: &Value<'static>) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::Integer(number) => number.to_string(),
        Value::Real(_) => number_text(value),
        Value::Text(text) => {
            let body = crate::raw::decode(text.raw()).replace('\'', "''");
            format!("'{body}'")
        }
        Value::Blob(blob) => {
            // Lower case, both the `x` and the digits: it is what the reference
            // writes, and a dump is compared against one.
            let mut out = String::from("x'");
            for byte in blob.raw() {
                out.push_str(&format!("{byte:02x}"));
            }
            out.push('\'');
            out
        }
    }
}

/// Returns a value as `.mode quote` and `.mode insert` write it.
///
/// **A text with a control character is written as `unistr('...')`.** The reference's
/// `output_quoted_string` counts the characters below U+0020, and when there is one
/// it cannot print the text between plain quotes without a newline, a tab or an escape
/// sequence landing in the output, so it writes a call to `unistr()` that rebuilds the
/// text: a quote is doubled, a backslash is doubled, and each control character is a
/// `\uXXXX` escape in lower case hex. Without a control character the text is written
/// between plain quotes, backslash and all, and DEL and the C1 range are not controls.
/// The text ends at its first NUL byte, because the reference reads it as a C string.
/// Everything that is not text is written as `literal` writes it.
///
/// `.dump` and the other callers of `literal` are unchanged: the reference builds those
/// with the SQL `quote()` function, which does not escape.
///
/// @param value - the value
pub fn quote_literal(value: &Value<'static>) -> String {
    let Value::Text(text) = value else {
        return literal(value);
    };
    let text = until_nul(text.raw());
    if !text.chars().any(|character| u32::from(character) < 0x20) {
        return format!("'{}'", text.replace('\'', "''"));
    }
    let mut out = String::from("unistr('");
    for character in text.chars() {
        match character {
            '\'' => out.push_str("''"),
            '\\' => out.push_str("\\\\"),
            control if u32::from(control) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", u32::from(control)));
            }
            other => out.push(other),
        }
    }
    out.push_str("')");
    out
}

/// Returns a column name as the layout writes it in a header line.
///
/// **The style a title is written in is set by the mode** (`csv` writes them as
/// CSV fields, `quote` as SQL strings) and is not changed by `--quote`. `--title`
/// sets it.
///
/// @param layout - the output settings
/// @param name - the column name
pub(crate) fn title_text(layout: &Layout, name: &str) -> String {
    let style = layout.title_quote.unwrap_or(Quote::Off);
    let written = match style {
        Quote::Off => name.to_string(),
        Quote::Sql | Quote::Relaxed => {
            quote_literal(&Value::owned_text(name.as_bytes()).unwrap_or(Value::Null))
        }
        Quote::Csv => csv_field(name, &layout.separator),
        Quote::Html => html_escape(name),
        Quote::Tcl => tcl_string(name),
        Quote::Json => format!("\"{}\"", inillucent_base::json::escape_sql(name)),
    };
    escape_controls(&written, layout.escape)
}

/// `list` and the modes laid out like it: values with a separator between them.
fn separated(layout: &Layout, columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    let separator = layout.separator.as_str();
    let end = layout.row_end();
    let mut out = Vec::with_capacity(rows.len() + 1);
    if layout.headers {
        let names: Vec<String> = columns
            .iter()
            .map(|name| title_text(layout, name))
            .collect();
        out.push(names.join(separator));
    }
    for row in rows {
        let cells: Vec<String> = row.iter().map(|value| list_cell(layout, value)).collect();
        out.push(cells.join(separator));
    }
    // **A carriage return on the end of the line itself when the row separator
    // is CR LF**, because the caller writes the newline after each line.
    //
    // **And a second one, but only on standard output, and only on Windows.**
    // The reference writes its CR LF through a text-mode C stream, which
    // translates the LF into CR LF again on the way out, and Rust's `write!`
    // does no such translation - so emitting two bytes where the reference
    // emits three would not be byte-compatible with the thing this replaces.
    //
    // The destination decides, because the reference's destinations differ.
    // Measured against the pinned 3.53.4 shell on Windows, one row of one
    // table, `.mode csv` with headers on:
    //
    // | what the reference was asked for | bytes at the end of a record |
    // |---|---|
    // | `sqlite3 -csv -header db "SELECT..."`, standard output | CR CR LF |
    // | the same rows through `.once out.csv`, in the file | CR LF |
    //
    // Its output file is not a text-mode stream, so the file gets RFC 4180's
    // CR LF and nothing more. This shell wrote CR CR LF into the file and into
    // the text a collecting caller reads, which matched neither. That is what
    // `to_stdout` is for.
    let doubled = cfg!(windows) && layout.to_stdout && layout.row_separator.ends_with("\r\n");
    for line in &mut out {
        line.push_str(&end);
        if doubled {
            line.push('\r');
        }
    }
    out
}

/// Returns one value as a list mode writes it.
///
/// A NULL with no text to show is nothing at all in a CSV record, which is how
/// it is told from an empty string: that one is written between quotes.
///
/// @param layout - the output settings
/// @param value - the cell
fn list_cell(layout: &Layout, value: &Value<'static>) -> String {
    escape_controls(&cell_text(layout, value), layout.escape)
}

/// `line`: one `name: value` per column, rows separated by a blank line.
fn lines(layout: &Layout, columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    let width = columns
        .iter()
        .map(|name| text::text_width(name))
        .max()
        .unwrap_or(0);
    let separator = if layout.separator.is_empty() {
        ": "
    } else {
        layout.separator.as_str()
    };
    let indent = " ".repeat(width + text::text_width(separator));
    let mut out = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            out.push(String::new());
        }
        for (position, value) in row.iter().enumerate() {
            let name = columns.get(position).cloned().unwrap_or_default();
            let shown = shown_cell(layout, value);
            let mut pieces = shown
                .split('\n')
                .map(|piece| piece.strip_suffix('\r').unwrap_or(piece));
            let first = pieces.next().unwrap_or_default();
            out.push(format!(
                "{}{name}{separator}{first}",
                " ".repeat(width.saturating_sub(text::text_width(&name)))
            ));
            for piece in pieces {
                out.push(format!("{indent}{piece}"));
            }
        }
    }
    out
}

/// `insert`: one statement per row, which is what a dump is made of.
///
/// With headers on, SQLite names the columns after the table, each quoted only
/// when its `quoteChar` says so: a name that is not a plain word, or that is a
/// keyword. The table is quoted by the same rule.
fn inserts(layout: &Layout, columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    let table = insert_column(layout.table_name());
    let target = if layout.headers {
        let names: Vec<String> = columns.iter().map(|name| insert_column(name)).collect();
        format!("{table}({})", names.join(","))
    } else {
        table
    };
    rows.iter()
        .map(|row| {
            let cells: Vec<String> = row
                .iter()
                .map(|value| insert_value(layout, value))
                .collect();
            format!("INSERT INTO {target} VALUES({});", cells.join(","))
        })
        .collect()
}

/// Returns a value as an `INSERT` writes it.
///
/// @param layout - the output settings
/// @param value - the cell
fn insert_value(layout: &Layout, value: &Value<'static>) -> String {
    match value {
        Value::Text(text) => sql_text(layout, text.raw()),
        Value::Blob(blob) if layout.quote == Quote::Off && layout.blob_quote == BlobQuote::Auto => {
            format!("'{}'", until_nul(blob.raw()).replace('\'', "''"))
        }
        other => quote_literal(other),
    }
}

/// Returns a name as SQLite's shell writes it in `.mode insert`.
///
/// The rule is `quoteChar` in the shell's source: quote when the first
/// character is not a letter or an underscore, when any character is not a
/// letter, digit or underscore, or when the word is a keyword.
///
/// @param name - the table or column name
pub(crate) fn insert_column(name: &str) -> String {
    let plain = name
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
        && inillucent_driver::keyword_lookup(name.as_bytes()).is_none();
    if plain {
        return name.to_string();
    }
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// `json`: an array of objects, one per row.
fn json(layout: &Layout, columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    if rows.is_empty() {
        // An empty result prints nothing, not an empty array: the reference
        // writes the brackets around rows and there are none.
        return Vec::new();
    }
    let mut out = object_lines(layout, columns, rows);
    let last = out.len().saturating_sub(1);
    for (index, line) in out.iter_mut().enumerate() {
        if index == 0 {
            line.insert(0, '[');
        }
        line.push(if index == last { ']' } else { ',' });
    }
    out
}

/// `jobject`: the objects, one to a line, with no array around them.
fn objects(columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    object_lines(
        &Layout {
            quote: Quote::Json,
            ..Layout::default()
        },
        columns,
        rows,
    )
}

/// Returns each row as a JSON object.
fn object_lines(layout: &Layout, columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    rows.iter()
        .map(|row| {
            let members: Vec<String> = row
                .iter()
                .enumerate()
                .map(|(position, value)| {
                    let name = columns.get(position).cloned().unwrap_or_default();
                    format!(
                        "\"{}\":{}",
                        inillucent_base::json::escape_sql(&name),
                        json_value(layout, value)
                    )
                })
                .collect();
            format!("{{{}}}", members.join(","))
        })
        .collect()
}

/// Returns a value as JSON.
fn json_value(layout: &Layout, value: &Value<'static>) -> String {
    match value {
        Value::Blob(blob) if layout.quote == Quote::Off && layout.blob_quote == BlobQuote::Auto => {
            format!(
                "\"{}\"",
                inillucent_base::json::escape_sql(&until_nul(blob.raw()))
            )
        }
        Value::Blob(blob)
            if matches!(layout.quote, Quote::Sql | Quote::Relaxed)
                && layout.blob_quote == BlobQuote::Auto =>
        {
            literal(&Value::owned_blob(blob.raw()).unwrap_or(Value::Null))
        }
        Value::Null => "null".to_string(),
        Value::Integer(number) => number.to_string(),
        Value::Real(_) => number_text(value),
        Value::Text(text) => format!(
            "\"{}\"",
            inillucent_base::json::escape_sql(&crate::raw::decode(text.raw()))
        ),
        // A blob's bytes, each as its own escape: the reference writes
        // `"«"` rather than the hex a reader might expect, and a consumer
        // of the JSON is reading whichever one it was given.
        Value::Blob(blob) => {
            let escaped: String = blob
                .raw()
                .iter()
                .map(|byte| format!("\\u{byte:04x}"))
                .collect();
            format!("\"{escaped}\"")
        }
    }
}

/// Quotes one CSV field, if it needs it.
///
/// The reference quotes text that is empty, that holds the mode's separator, or
/// that holds a character from a fixed list: every control character, the
/// space, the double quote, the single quote, DELETE and everything past ASCII.
/// A comma, a semicolon or a vertical bar does not force quoting in `tabs`.
/// The text is judged before control characters are made visible, so a
/// caret in the data is not mistaken for one.
///
/// @param text - the field's text
/// @param separator - the mode's separator
pub(crate) fn csv_field(text: &str, separator: &str) -> String {
    let needs = text.is_empty()
        || text.contains(separator)
        || text.chars().any(|character| {
            u32::from(character) <= 0x20
                || matches!(character, '"' | '\'')
                || u32::from(character) >= 0x7f
        });
    if !needs {
        return text.to_string();
    }
    format!("\"{}\"", text.replace('"', "\"\""))
}

/// `html`: a table body, which is what a caller pastes into a page.
///
/// One cell per line and no closing cell tags, which is what the reference
/// emits. It is valid HTML - a `<TD>` closes the one before it - and it is what
/// a diff against the reference has to produce.
fn html(layout: &Layout, columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    let mut out = Vec::new();
    if layout.headers {
        out.push("<TR>".to_string());
        for name in columns {
            out.push(format!("<TH>{}", html_escape(name)));
        }
        out.push("</TR>".to_string());
    }
    for row in rows {
        out.push("<TR>".to_string());
        for value in row {
            // **A NULL is `null` in `html`, whatever `.nullvalue` says**, as the
            // reference writes it.
            let text = match value {
                Value::Null => "null".to_string(),
                other => escape_controls(&until_value(other), layout.escape),
            };
            out.push(format!("<TD>{}", html_escape(&text)));
        }
        out.push("</TR>".to_string());
    }
    out
}

/// Returns a value as plain text, for the modes that escape it themselves.
fn until_value(value: &Value<'static>) -> String {
    match value {
        Value::Null => String::new(),
        Value::Integer(number) => number.to_string(),
        Value::Real(_) => number_text(value),
        Value::Text(text) => until_nul(text.raw()),
        Value::Blob(blob) => until_nul(blob.raw()),
    }
}

/// Escapes the five characters that mean something in HTML, as the reference does.
fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a one-row result for the tests below.
    fn sample() -> (Vec<String>, Vec<Vec<Value<'static>>>) {
        let columns = vec!["a".to_string(), "b".to_string()];
        let rows = vec![vec![
            Value::Integer(1),
            Value::owned_text(b"two").expect("owns"),
        ]];
        (columns, rows)
    }

    /// The default mode is SQLite's: pipes, no headers.
    #[test]
    fn the_default_is_a_pipe_separated_line() {
        let (columns, rows) = sample();
        let out = render(&Layout::default(), &columns, &rows);
        assert_eq!(out, vec!["1|two"]);
    }

    /// Headers are the column names, in the same layout as the rows.
    #[test]
    fn headers_use_the_same_layout() {
        let (columns, rows) = sample();
        let layout = Layout {
            headers: true,
            ..Layout::default()
        };
        let out = render(&layout, &columns, &rows);
        assert_eq!(out, vec!["a|b", "1|two"]);
    }

    /// A CSV record ends with one carriage return unless it is going to
    /// standard output on Windows, where it ends with two.
    ///
    /// The two counts are the reference's, measured at 3.53.4: `sqlite3 -csv`
    /// emits CR CR LF on standard output on this platform, because its row
    /// separator is CR LF and the C stream translates the LF again; the same
    /// rows through `.once out.csv` hold CR LF, because the file is not such a
    /// stream. Both destinations are checked here so that a change to either
    /// one has to be a deliberate change to this case.
    #[test]
    fn a_csv_record_ends_the_way_its_destination_expects() {
        let (columns, rows) = sample();
        let to_a_file = Layout {
            mode: Mode::Csv,
            separator: ",".to_string(),
            row_separator: "\r\n".to_string(),
            quote: Quote::Csv,
            to_stdout: false,
            ..Layout::default()
        };
        assert_eq!(render(&to_a_file, &columns, &rows), vec!["1,two\r"]);
        let to_the_terminal = Layout {
            to_stdout: true,
            ..to_a_file
        };
        let expected = if cfg!(windows) {
            "1,two\r\r"
        } else {
            "1,two\r"
        };
        assert_eq!(render(&to_the_terminal, &columns, &rows), vec![expected]);
    }

    /// A row separator with no newline in it is the whole ending of the line.
    #[test]
    fn a_row_separator_without_a_newline_ends_the_line_itself() {
        let (columns, rows) = sample();
        let layout = Layout {
            row_separator: "X".to_string(),
            ..Layout::default()
        };
        assert_eq!(render(&layout, &columns, &rows), vec!["1|twoX"]);
        assert!(!layout.lines_end_in_newline());
    }

    /// A CSV field is quoted only when it has to be.
    #[test]
    fn csv_quotes_only_what_it_must() {
        assert_eq!(csv_field("plain", ","), "plain");
        assert_eq!(csv_field("a,b", ","), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\"", ","), "\"say \"\"hi\"\"\"");
    }

    /// **An empty CSV field that is not NULL is quoted, and a NULL is not.**
    ///
    /// A blob is printed as a C string and stops at its first NUL, so a blob
    /// beginning with one rendered as nothing at all - and so does a NULL
    /// under the default `nullvalue`, which is the empty string. The reference
    /// writes two quotes for the first and nothing for the second, so an
    /// exported blob was indistinguishable from a NULL on the way back in
    /// (task-2066 section 4.2, item 27).
    #[test]
    fn csv_tells_an_empty_value_from_a_null() {
        let columns = vec!["x".to_string()];
        let rows = vec![
            vec![Value::Null],
            vec![Value::owned_blob(&[0, 1, 2]).expect("owns")],
            vec![Value::owned_text(b"").expect("owns")],
            vec![Value::owned_text(b"kept").expect("owns")],
        ];
        let layout = Layout {
            mode: Mode::Csv,
            separator: ",".to_string(),
            quote: Quote::Csv,
            ..Layout::default()
        };
        assert_eq!(
            render(&layout, &columns, &rows),
            vec!["", "\"\"", "\"\"", "kept"]
        );
    }

    /// A value is quoted for `tabs` and `csv` when it holds a character the
    /// reference lists: a control character, a space, a double or single quote,
    /// DELETE, anything past ASCII, or the mode's separator. Empty text is
    /// quoted too.
    #[test]
    fn csv_quotes_the_characters_the_reference_lists() {
        for quoted in [
            "a b",
            "q\"",
            "it's",
            "caf\u{e9}",
            "a\tb",
            "a\nb",
            "x\u{7f}",
            "",
        ] {
            assert!(csv_field(quoted, ",").starts_with('"'), "{quoted:?}");
        }
        for plain in ["abc", "a^b", "semi;", "a|b", "12", "1.5"] {
            assert_eq!(csv_field(plain, ","), plain);
        }
        assert_eq!(csv_field("a,b", ","), "\"a,b\"");
        assert_eq!(csv_field("a,b", "\t"), "a,b");
    }

    /// Quote mode writes values as SQL literals, blobs included.
    #[test]
    fn quote_mode_writes_literals() {
        let columns = vec!["x".to_string()];
        let rows = vec![
            vec![Value::Null],
            vec![Value::owned_blob(&[1, 255]).expect("owns")],
            vec![Value::owned_text(b"it's").expect("owns")],
        ];
        let layout = Layout {
            mode: Mode::Quote,
            quote: Quote::Sql,
            null: "NULL".to_string(),
            ..Layout::default()
        };
        let out = render(&layout, &columns, &rows);
        assert_eq!(out, vec!["NULL", "x'01ff'", "'it''s'"]);
    }

    /// Every mode name round-trips.
    #[test]
    fn every_mode_name_round_trips() {
        for mode in [
            Mode::List,
            Mode::Column,
            Mode::Line,
            Mode::Csv,
            Mode::Tabs,
            Mode::Quote,
            Mode::Insert,
            Mode::Json,
            Mode::Markdown,
            Mode::Table,
            Mode::Box,
            Mode::Html,
            Mode::Ascii,
            Mode::Qbox,
            Mode::Tcl,
            Mode::C,
            Mode::Count,
            Mode::Off,
            Mode::Jatom,
            Mode::Jobject,
            Mode::Psql,
            Mode::Split,
        ] {
            assert_eq!(Mode::from_name(mode.name()), Some(mode), "{}", mode.name());
        }
        assert_eq!(Mode::from_name("nonsense"), None);
    }

    /// A control character is escaped and a NUL ends the value.
    #[test]
    fn control_characters_are_escaped() {
        use super::text::escape_controls;
        assert_eq!(escape_controls("ab", Escape::Auto), "ab");
        assert_eq!(escape_controls("\u{1}\u{2}", Escape::Auto), "^A^B");
        assert_eq!(escape_controls("\tt", Escape::Auto), "\tt");
        assert_eq!(escape_controls("\rr", Escape::Auto), "^Mr");
        assert_eq!(escape_controls("a\r\nb", Escape::Auto), "a\r\nb");
        assert_eq!(escape_controls("\u{7f}", Escape::Auto), "\u{7f}");
        assert_eq!(until_nul(&[b'a', 0, b'b']), "a");
    }

    /// A drawn table has a rule above and below its rows.
    #[test]
    fn a_table_is_drawn_with_rules() {
        let (columns, rows) = sample();
        let layout = Layout {
            mode: Mode::Table,
            headers: true,
            ..Layout::default()
        };
        let out = render(&layout, &columns, &rows);
        assert_eq!(out.len(), 5, "{out:#?}");
        assert!(out.first().is_some_and(|line| line.starts_with('+')));
        assert!(out.last().is_some_and(|line| line.starts_with('+')));
    }

    /// JSON escapes what JSON has to escape.
    #[test]
    fn the_shell_escapes_through_the_base_crate() {
        assert_eq!(inillucent_base::json::escape("a\"b"), "a\\\"b");
        assert_eq!(inillucent_base::json::escape("a\nb"), "a\\nb");
        assert_eq!(inillucent_base::json::escape("a\u{1}b"), "a\\u0001b");
        // The two the private copy spelled as `\u0008` and `\u000c`, and the
        // one it left unescaped.
        assert_eq!(inillucent_base::json::escape("a\u{8}b"), "a\\bb");
        assert_eq!(inillucent_base::json::escape("a\u{c}b"), "a\\fb");
        assert_eq!(inillucent_base::json::escape("a\u{7f}b"), "a\\u007fb");
        assert_eq!(inillucent_base::json::escape_sql("a\u{7f}b"), "a\u{7f}b");
    }

    /// The table name of an `INSERT` is quoted when it has to be.
    #[test]
    fn an_insert_quotes_a_table_name_that_needs_it() {
        let (columns, rows) = sample();
        let layout = Layout {
            mode: Mode::Insert,
            table: "x y".to_string(),
            ..Layout::default()
        };
        assert_eq!(
            render(&layout, &columns, &rows),
            vec!["INSERT INTO \"x y\" VALUES(1,'two');"]
        );
    }
}

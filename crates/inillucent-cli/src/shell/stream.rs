//! Printing a statement's rows as the engine produces them.
//!
//! Invariant: **the shell holds one chunk of a result, not the result.** The statement hands each
//! row to a [`RowPrinter`], which converts it, keeps it until a chunk of rows is complete, renders
//! the chunk with [`RowRenderer`] and writes the lines where the shell's output is going. Printing
//! three million rows costs the memory of a few hundred, where the shell used to collect every row
//! before it printed the first.
//!
//! The statement runs inside the call and the printer has to be reachable from inside it, so the
//! places the shell writes to (a `.once` or `.output` redirection, a collecting caller, a
//! `.testcase`) are moved out of the shell into the printer for the length of the statement and
//! moved back afterwards. Nothing else prints while a statement runs.

use inillucent_driver::{Datum, Flow, RowConsumer};
use inillucent_value::Value;

use super::redirect::Redirection;
use crate::render::stream::RowRenderer;
use crate::render::Layout;

/// How many rows are kept before they are rendered and written.
///
/// Large enough that the renderer's work per chunk is not dominated by the call, and small enough
/// that a wide row of a few kilobytes still keeps the chunk under a few megabytes.
const CHUNK_ROWS: usize = 512;

/// The places the shell writes a line to, besides standard output.
pub(super) struct LineSinks {
    /// A `.once` or `.output` redirection, which outranks a collecting caller.
    pub redirection: Option<Redirection>,
    /// A caller that is collecting what the shell prints.
    pub sink: Option<String>,
    /// What has been printed since a `.testcase`, when one is capturing.
    pub captured: Option<String>,
    /// Whether lines end in a carriage return and a line feed.
    pub crlf: bool,
}

impl LineSinks {
    /// Writes one line where the shell's output is going.
    ///
    /// @param line - the text
    /// @param terminated - whether a newline follows it
    pub fn deliver(&mut self, line: &str, terminated: bool) {
        deliver(
            &mut self.redirection,
            &mut self.sink,
            self.captured.as_mut(),
            self.crlf,
            line,
            terminated,
        );
    }
}

/// Writes one line to a capture, a redirection, a collecting caller, or standard output, in that
/// order of precedence.
///
/// @param redirection - the `.once` or `.output` redirection, if one is open
/// @param sink - the text a collecting caller reads, if there is one
/// @param captured - what a `.testcase` has captured, when one is capturing
/// @param crlf - whether lines end in a carriage return and a line feed
/// @param line - the text
/// @param terminated - whether a newline follows it
pub(super) fn deliver(
    redirection: &mut Option<Redirection>,
    sink: &mut Option<String>,
    captured: Option<&mut String>,
    crlf: bool,
    line: &str,
    terminated: bool,
) {
    // **A `.testcase` captures instead of printing.** `.check` compares the
    // output of the commands between the two, and a passing case prints
    // nothing at all - which is what makes a test script's output the list
    // of the cases that failed.
    if let Some(captured) = captured {
        captured.push_str(line);
        if terminated {
            captured.push('\n');
        }
        return;
    }
    // **A redirect outranks a collecting caller, and used to lose to one
    // (task-2044).** `.once` and `.output` open their file and every line
    // then went into the sink instead, so the file existed and was zero
    // bytes while the rows came back in the caller's report. It was not a
    // corner: `export --out` built a `.once` and ran it through
    // `collect_output`, so the shipped command reported `"ok": true` with
    // `"wrote": "<path>"` over an empty file in all eight formats, and a
    // `.once` inside a script handed to `run` did the same. `export` asks
    // for its redirect directly now, but `run` still hands the shell a
    // script somebody else wrote, so this order is what makes that work.
    //
    // This order is what the two mean. A redirect is the caller of the
    // shell saying where output goes; a sink is a caller collecting what
    // was not redirected. `complain` is deliberately the other way round -
    // an error goes to the sink even while a redirect is open, because an
    // error belongs in the report rather than in the middle of the rows.
    let ending = match (terminated, crlf) {
        (false, _) => "",
        (true, true) => "\r\n",
        (true, false) => "\n",
    };
    if let Some(redirection) = redirection.as_mut() {
        let mut bytes = crate::raw::encode(line).into_owned();
        bytes.extend_from_slice(ending.as_bytes());
        redirection.write(&bytes);
        return;
    }
    if let Some(sink) = sink.as_mut() {
        sink.push_str(line);
        if terminated {
            sink.push('\n');
        }
        return;
    }
    super::output::write_line(line, ending);
}

/// Receives the rows of one statement and prints them a chunk at a time.
pub(super) struct RowPrinter {
    /// The settings the rows are drawn with.
    layout: Layout,
    /// Whether each rendered line ends a line when printed.
    terminated: bool,
    /// Whether a result with no rows still prints its header line.
    header_when_empty: bool,
    /// Draws the chunks, once the column names are known.
    renderer: Option<RowRenderer>,
    /// The rows received and not yet rendered.
    held: Vec<Vec<Value<'static>>>,
    /// Where the lines go.
    sinks: LineSinks,
}

impl RowPrinter {
    /// Returns a printer for one statement.
    ///
    /// @param layout - the settings the rows are drawn with
    /// @param header_when_empty - whether a result with no rows still prints its header
    /// @param sinks - where the lines go, moved out of the shell for the statement
    pub fn new(layout: Layout, header_when_empty: bool, sinks: LineSinks) -> RowPrinter {
        let terminated = layout.lines_end_in_newline();
        RowPrinter {
            layout,
            terminated,
            header_when_empty,
            renderer: None,
            held: Vec::new(),
            sinks,
        }
    }

    /// Writes the rows still held and the lines that end the result, and returns how many rows
    /// were printed.
    ///
    /// **A statement that failed prints the rows it had produced and nothing more**, as the
    /// reference shell does: the rows before the failing one, closed as a result is closed, and
    /// then the error. A result with no rows prints its header only when the statement
    /// succeeded and a header for an empty result was asked for.
    ///
    /// @param complete - whether the statement ran to its end
    pub fn finish(&mut self, complete: bool) -> usize {
        self.flush();
        let layout = self.layout.clone();
        let renderer = self
            .renderer
            .get_or_insert_with(|| RowRenderer::new(layout, Vec::new()));
        let rows = renderer.rows();
        if rows > 0 || (complete && self.header_when_empty) {
            let lines = renderer.finish();
            self.print(lines);
        }
        rows
    }

    /// Hands back the places the lines go, which the shell takes again.
    pub fn release(&mut self) -> LineSinks {
        std::mem::replace(
            &mut self.sinks,
            LineSinks {
                redirection: None,
                sink: None,
                captured: None,
                crlf: false,
            },
        )
    }

    /// Renders the rows held and writes them.
    fn flush(&mut self) {
        if self.held.is_empty() {
            return;
        }
        let held = std::mem::take(&mut self.held);
        let layout = self.layout.clone();
        let renderer = self
            .renderer
            .get_or_insert_with(|| RowRenderer::new(layout, Vec::new()));
        let lines = renderer.chunk(&held);
        self.print(lines);
    }

    /// Writes rendered lines, ending each as the layout says.
    ///
    /// @param lines - the rendered lines
    fn print(&mut self, lines: Vec<String>) {
        for line in lines {
            self.sinks.deliver(&line, self.terminated);
        }
    }
}

impl RowConsumer for RowPrinter {
    /// Learns the column names, which the renderer needs before the first row.
    ///
    /// @param names - the result columns
    fn columns(&mut self, names: &[String]) {
        self.renderer = Some(RowRenderer::new(self.layout.clone(), names.to_vec()));
    }

    /// Keeps one row, and prints the chunk when it is full.
    ///
    /// @param row - the row's values, borrowed from the page
    fn accept(&mut self, row: &[Datum<'_>]) -> inillucent_base::DbResult<Flow> {
        let mut owned = Vec::with_capacity(row.len());
        for value in row {
            owned.push(Value::from(value).into_owned()?);
        }
        self.held.push(owned);
        if self.held.len() >= CHUNK_ROWS {
            self.flush();
        }
        Ok(Flow::Continue)
    }
}

/// Keeps the first rows of a statement and counts the rest.
pub(super) struct Limiter {
    /// How many rows to keep.
    keep: usize,
    /// The rows kept.
    pub rows: Vec<Vec<Value<'static>>>,
}

impl Limiter {
    /// Returns a consumer that keeps this many rows.
    ///
    /// @param keep - how many rows to keep; the rest are counted by the statement and dropped
    pub fn new(keep: usize) -> Limiter {
        Limiter {
            keep,
            rows: Vec::new(),
        }
    }
}

impl RowConsumer for Limiter {
    /// Keeps the row when there is room for it.
    ///
    /// @param row - the row's values, borrowed from the page
    fn accept(&mut self, row: &[Datum<'_>]) -> inillucent_base::DbResult<Flow> {
        if self.rows.len() < self.keep {
            let mut owned = Vec::with_capacity(row.len());
            for value in row {
                owned.push(Value::from(value).into_owned()?);
            }
            self.rows.push(owned);
        }
        Ok(Flow::Continue)
    }
}

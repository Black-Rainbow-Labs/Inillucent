//! The modes that draw columns: `column`, `table`, `box`, `markdown`, `psql`.
//!
//! Invariant: **every cell is cut into lines before any width is chosen, and a
//! column is as wide as its widest line.** A value with a newline, a value wider
//! than `--wrap` and a value wider than a width set by `.width` are all several
//! lines of one row, and the columns after them are padded to line up. A width
//! from `.width` is exact: text longer than it is wrapped, shorter text is padded.
//! A negative width right aligns the column.

use inillucent_value::Value;

use super::text::{cell_lines, text_width, wrap_line};
use super::{shown_cell, Layout, Mode, WordWrap};

/// How a cell sits in its column.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Align {
    /// Against the left edge.
    Left,
    /// In the middle, leaning left when it cannot be even.
    Center,
    /// Against the right edge.
    Right,
}

/// One column once its cells are cut into lines.
struct Column {
    /// The title's lines.
    header: Vec<String>,
    /// How many columns the column takes.
    width: usize,
    /// The alignment `.width` or `--align` asked for, if either did.
    forced: Option<Align>,
}

/// The characters one drawn table is made of.
struct Frame {
    /// The left edge.
    left: &'static str,
    /// Between two columns.
    middle: &'static str,
    /// The right edge.
    right: &'static str,
    /// The rule.
    horizontal: &'static str,
    /// The wall of a text row.
    vertical: &'static str,
}

/// Returns the lines a drawn mode prints for a result.
///
/// @param layout - the output settings
/// @param columns - the result's column names
/// @param rows - the result's rows
pub fn render(layout: &Layout, columns: &[String], rows: &[Vec<Value<'static>>]) -> Vec<String> {
    let count = columns
        .len()
        .max(rows.iter().map(Vec::len).max().unwrap_or(0));
    let mut table: Vec<Column> = (0..count)
        .map(|at| column_for(layout, columns, at))
        .collect();
    let cells: Vec<Vec<(Vec<String>, bool)>> = rows
        .iter()
        .map(|row| {
            (0..count)
                .map(|at| body_cell(layout, &table, at, row.get(at)))
                .collect()
        })
        .collect();
    for (at, column) in table.iter_mut().enumerate() {
        if column.width == 0 {
            let widest = cells
                .iter()
                .filter_map(|row| row.get(at))
                .flat_map(|(lines, _)| lines.iter())
                .chain(column.header.iter())
                .map(|line| text_width(line))
                .max()
                .unwrap_or(0);
            // A column whose values are wider than `--wrap` is exactly that wide, even
            // when every line it was cut into is shorter.
            let natural = natural_width(layout, columns, rows, at);
            column.width = if layout.wrap > 0 && natural > layout.wrap {
                layout.wrap
            } else {
                widest
            };
        }
    }
    let tall = cells
        .iter()
        .any(|row| row.iter().any(|(lines, _)| lines.len() > 1));
    match layout.mode {
        Mode::Column | Mode::Split => column_lines(layout, &table, &cells, tall),
        Mode::Psql => psql_lines(layout, &table, &cells),
        _ if !layout.border => psql_lines(layout, &table, &cells),
        _ => drawn_lines(layout, &table, &cells, tall),
    }
}

/// Returns the widest line of a column before anything is wrapped.
///
/// @param layout - the output settings
/// @param columns - the column names
/// @param rows - the rows
/// @param at - which column
fn natural_width(
    layout: &Layout,
    columns: &[String],
    rows: &[Vec<Value<'static>>],
    at: usize,
) -> usize {
    let title = if layout.headers {
        columns.get(at).map(|name| shown_title(layout, name))
    } else {
        None
    };
    title
        .into_iter()
        .chain(
            rows.iter()
                .filter_map(|row| row.get(at))
                .map(|value| shown_cell(layout, value)),
        )
        .flat_map(|text| cell_lines(&text))
        .map(|line| text_width(&line))
        .max()
        .unwrap_or(0)
}

/// Builds a column from its title and the settings for its position.
///
/// A width of zero means the column is as wide as its widest line, which is worked
/// out once the cells are known.
///
/// @param layout - the output settings
/// @param columns - the column names
/// @param at - which column
fn column_for(layout: &Layout, columns: &[String], at: usize) -> Column {
    let setting = layout.widths.get(at).copied().unwrap_or(0);
    let fixed = setting.unsigned_abs() as usize;
    let forced = match layout.align.chars().nth(at) {
        Some('L' | 'l') => Some(Align::Left),
        Some('C' | 'c') => Some(Align::Center),
        Some('R' | 'r') => Some(Align::Right),
        _ if setting < 0 => Some(Align::Right),
        _ => None,
    };
    let title = columns.get(at).cloned().unwrap_or_default();
    let header = if layout.headers {
        lines_of(layout, &shown_title(layout, &title), fixed)
    } else {
        Vec::new()
    };
    Column {
        header,
        width: fixed,
        forced,
    }
}

/// Returns the title of a column as the settings write it.
///
/// @param layout - the output settings
/// @param title - the column's name
fn shown_title(layout: &Layout, title: &str) -> String {
    super::title_text(layout, title)
}

/// Cuts a text into the lines it is drawn on in a column.
///
/// @param layout - the output settings
/// @param text - the text
/// @param fixed - the width `.width` set for the column, zero when it set none
fn lines_of(layout: &Layout, text: &str, fixed: usize) -> Vec<String> {
    let limit = if fixed > 0 { fixed } else { layout.wrap };
    let words = layout.word_wrap != WordWrap::Off;
    cell_lines(text)
        .iter()
        .flat_map(|line| wrap_line(line, limit, words))
        .collect()
}

/// Returns a body cell's lines and whether its value is a number.
///
/// @param layout - the output settings
/// @param table - the columns, for their widths
/// @param at - which column
/// @param value - the value, when the row has one in this column
fn body_cell(
    layout: &Layout,
    table: &[Column],
    at: usize,
    value: Option<&Value<'static>>,
) -> (Vec<String>, bool) {
    let Some(value) = value else {
        return (vec![String::new()], false);
    };
    let fixed = table.get(at).map_or(0, |column| column.width);
    let numeric = matches!(value, Value::Integer(_) | Value::Real(_));
    (lines_of(layout, &shown_cell(layout, value), fixed), numeric)
}

/// Pads a line to a width, in the alignment asked for.
///
/// @param text - the line
/// @param width - the column's width
/// @param align - where the text sits
fn aligned(text: &str, width: usize, align: Align) -> String {
    let used = text_width(text);
    let spare = width.saturating_sub(used);
    match align {
        Align::Left => format!("{text}{}", " ".repeat(spare)),
        Align::Right => format!("{}{text}", " ".repeat(spare)),
        Align::Center => {
            let left = spare / 2;
            format!("{}{text}{}", " ".repeat(left), " ".repeat(spare - left))
        }
    }
}

/// Returns the line `at` of every cell of a row, padded to the columns.
///
/// @param table - the columns
/// @param row - the row's cells and whether each is a number
/// @param at - which line of the row
fn row_line(table: &[Column], row: &[(Vec<String>, bool)], at: usize) -> Vec<String> {
    table
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let (lines, numeric) = row.get(index).map_or((&[][..], false), |(lines, numeric)| {
                (lines.as_slice(), *numeric)
            });
            let text = lines.get(at).map_or("", String::as_str);
            let align = column
                .forced
                .unwrap_or(if numeric { Align::Right } else { Align::Left });
            aligned(text, column.width, align)
        })
        .collect()
}

/// Returns the line `at` of the header, each title centred in its column.
///
/// @param table - the columns
/// @param at - which line of the header
fn header_line(table: &[Column], at: usize) -> Vec<String> {
    table
        .iter()
        .map(|column| {
            let text = column.header.get(at).map_or("", String::as_str);
            aligned(text, column.width, Align::Center)
        })
        .collect()
}

/// How many lines the header takes.
///
/// @param table - the columns
fn header_height(table: &[Column]) -> usize {
    table
        .iter()
        .map(|column| column.header.len())
        .max()
        .unwrap_or(0)
}

/// How many lines a row takes.
///
/// @param row - the row's cells
fn row_height(row: &[(Vec<String>, bool)]) -> usize {
    row.iter()
        .map(|(lines, _)| lines.len())
        .max()
        .unwrap_or(1)
        .max(1)
}

/// `column`: two spaces between columns, a rule of dashes under the titles, and a
/// blank line between rows when any row takes more than one line.
///
/// @param layout - the output settings
/// @param table - the columns
/// @param cells - every row
/// @param tall - whether any cell takes more than one line
fn column_lines(
    layout: &Layout,
    table: &[Column],
    cells: &[Vec<(Vec<String>, bool)>],
    tall: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let joined = |parts: Vec<String>| parts.join("  ").trim_end().to_string();
    if layout.headers {
        for at in 0..header_height(table) {
            out.push(joined(header_line(table, at)));
        }
        out.push(
            table
                .iter()
                .map(|column| "-".repeat(column.width))
                .collect::<Vec<String>>()
                .join("  "),
        );
    }
    for (position, row) in cells.iter().enumerate() {
        if tall && position > 0 {
            out.push(String::new());
        }
        for at in 0..row_height(row) {
            out.push(joined(row_line(table, row, at)));
        }
    }
    out
}

/// `psql` (and `box` or `table` with the border off): cells between bars, no outer
/// edge, a rule of dashes and plus signs under the titles.
///
/// @param layout - the output settings
/// @param table - the columns
/// @param cells - every row
fn psql_lines(
    layout: &Layout,
    table: &[Column],
    cells: &[Vec<(Vec<String>, bool)>],
) -> Vec<String> {
    let mut out = Vec::new();
    let joined = |parts: Vec<String>| {
        parts
            .iter()
            .map(|part| format!(" {part} "))
            .collect::<Vec<String>>()
            .join("|")
            .trim_end()
            .to_string()
    };
    if layout.headers {
        for at in 0..header_height(table) {
            out.push(joined(header_line(table, at)));
        }
        out.push(
            table
                .iter()
                .map(|column| "-".repeat(column.width + 2))
                .collect::<Vec<String>>()
                .join("+"),
        );
    }
    for row in cells {
        for at in 0..row_height(row) {
            out.push(joined(row_line(table, row, at)));
        }
    }
    out
}

/// `markdown`, `table` and `box`: a header, a rule, and the rows.
///
/// @param layout - the output settings
/// @param table - the columns
/// @param cells - every row
/// @param tall - whether any cell takes more than one line, which puts a rule
///     between every pair of rows
fn drawn_lines(
    layout: &Layout,
    table: &[Column],
    cells: &[Vec<(Vec<String>, bool)>],
    tall: bool,
) -> Vec<String> {
    let frame = match layout.mode {
        Mode::Markdown => Frame {
            left: "|",
            middle: "|",
            right: "|",
            horizontal: "-",
            vertical: "|",
        },
        // The reference draws its box with rounded corners and a double
        // rule under the header. That is copied exactly rather than
        // approximated: the whole value of the mode is that a person
        // recognises the output.
        Mode::Box | Mode::Qbox => Frame {
            left: "\u{256d}",
            middle: "\u{252c}",
            right: "\u{256e}",
            horizontal: "\u{2500}",
            vertical: "\u{2502}",
        },
        _ => Frame {
            left: "+",
            middle: "+",
            right: "+",
            horizontal: "-",
            vertical: "|",
        },
    };
    let widths: Vec<usize> = table.iter().map(|column| column.width).collect();
    let mut out = Vec::new();
    let rule = rule_line(&frame, &widths);
    let markdown = layout.mode == Mode::Markdown;
    if !markdown {
        out.push(rule.clone());
    }
    if layout.headers {
        for at in 0..header_height(table) {
            out.push(drawn_row(&frame, &header_line(table, at)));
        }
        out.push(match layout.mode {
            Mode::Markdown => markdown_rule(&widths),
            Mode::Box | Mode::Qbox => rule_line(
                &Frame {
                    left: "\u{255e}",
                    middle: "\u{256a}",
                    right: "\u{2561}",
                    horizontal: "\u{2550}",
                    ..frame
                },
                &widths,
            ),
            _ => rule.clone(),
        });
    }
    let between = match layout.mode {
        Mode::Markdown => None,
        Mode::Box | Mode::Qbox => Some(rule_line(
            &Frame {
                left: "\u{251c}",
                middle: "\u{253c}",
                right: "\u{2524}",
                ..frame
            },
            &widths,
        )),
        _ => Some(rule.clone()),
    };
    for (position, row) in cells.iter().enumerate() {
        if let (true, true, Some(line)) = (tall, position > 0, between.as_ref()) {
            out.push(line.clone());
        }
        for at in 0..row_height(row) {
            out.push(drawn_row(&frame, &row_line(table, row, at)));
        }
    }
    match layout.mode {
        Mode::Box | Mode::Qbox => out.push(rule_line(
            &Frame {
                left: "\u{2570}",
                middle: "\u{2534}",
                right: "\u{256f}",
                ..frame
            },
            &widths,
        )),
        Mode::Markdown => {}
        _ => out.push(rule),
    }
    out
}

/// Returns one horizontal rule.
///
/// @param frame - the characters to draw with
/// @param widths - each column's width
fn rule_line(frame: &Frame, widths: &[usize]) -> String {
    let parts: Vec<String> = widths
        .iter()
        .map(|width| frame.horizontal.repeat(width + 2))
        .collect();
    format!("{}{}{}", frame.left, parts.join(frame.middle), frame.right)
}

/// Returns the `|---|---|` line Markdown wants under its header.
///
/// @param widths - each column's width
fn markdown_rule(widths: &[usize]) -> String {
    let parts: Vec<String> = widths.iter().map(|width| "-".repeat(width + 2)).collect();
    format!("|{}|", parts.join("|"))
}

/// Returns one drawn row from cells already padded to their widths.
///
/// @param frame - the characters to draw with
/// @param cells - the padded cells
fn drawn_row(frame: &Frame, cells: &[String]) -> String {
    let padded: Vec<String> = cells.iter().map(|cell| format!(" {cell} ")).collect();
    format!(
        "{}{}{}",
        frame.vertical,
        padded.join(frame.vertical),
        frame.vertical
    )
}

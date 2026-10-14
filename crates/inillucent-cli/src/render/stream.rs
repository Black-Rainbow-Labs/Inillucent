//! Rendering a result a chunk of rows at a time.
//!
//! Invariant: **the lines of every chunk followed by the lines `finish` returns are the lines
//! [`render`] returns for all of the rows at once, whatever the chunk sizes.** A test compares the
//! two for every mode that streams, over chunkings of one row, several rows and every row. That
//! equality is what lets the shell print a result as it is produced and keep the output that
//! `sqlite3` scripts and the differential tests expect, byte for byte.
//!
//! The modes that draw columns (`column`, `table`, `box`, `markdown`, `qbox`, `psql` and `split`)
//! do not stream: they size each column to its widest value, so no line can be written until every
//! row has been seen. SQLite's shell holds every cell for these modes too. They are drawn from the
//! rows collected, as before.

use inillucent_value::Value;

use super::{object_lines, render, Layout, Mode};

/// Reports whether a mode can be rendered a chunk of rows at a time.
///
/// @param mode - the output mode
pub fn streams(mode: Mode) -> bool {
    mode.is_list()
        || matches!(
            mode,
            Mode::Line
                | Mode::Insert
                | Mode::Json
                | Mode::Jobject
                | Mode::Html
                | Mode::Count
                | Mode::Off
        )
}

/// Renders the rows of one result as they arrive.
pub struct RowRenderer {
    /// The settings the rows are drawn with.
    layout: Layout,
    /// The result's column names.
    columns: Vec<String>,
    /// How many rows have been rendered, which decides what the first chunk adds.
    seen: usize,
    /// The last line of the `json` array so far, held until the line after it, or the end, says
    /// whether it is followed by a comma or by the closing bracket.
    pending: Option<String>,
}

impl RowRenderer {
    /// Returns a renderer for a result with these columns.
    ///
    /// @param layout - the settings the rows are drawn with; its mode has to stream
    /// @param columns - the result's column names
    pub fn new(layout: Layout, columns: Vec<String>) -> RowRenderer {
        RowRenderer {
            layout,
            columns,
            seen: 0,
            pending: None,
        }
    }

    /// Returns the lines for the next rows of the result.
    ///
    /// **What comes before the first row is written with the first chunk**: the header of a list
    /// mode and of `html`, the `[` of `json`. What comes between rows is written with the chunk
    /// that follows them: the blank line of `line` mode, the comma of `json`.
    ///
    /// @param rows - the next rows, in order
    pub fn chunk(&mut self, rows: &[Vec<Value<'static>>]) -> Vec<String> {
        if rows.is_empty() {
            return Vec::new();
        }
        let first = self.seen == 0;
        self.seen = self.seen.saturating_add(rows.len());
        match self.layout.mode {
            Mode::Json => self.json_chunk(rows, first),
            Mode::Count | Mode::Off => Vec::new(),
            Mode::Line => {
                let mut out = render(&self.layout, &self.columns, rows);
                if !first && !out.is_empty() {
                    out.insert(0, String::new());
                }
                out
            }
            Mode::Insert | Mode::Jobject => render(&self.layout, &self.columns, rows),
            _ if first => render(&self.layout, &self.columns, rows),
            _ => {
                let without_header = Layout {
                    headers: false,
                    ..self.layout.clone()
                };
                render(&without_header, &self.columns, rows)
            }
        }
    }

    /// Returns the lines that end the result.
    ///
    /// For a result with no rows these are the lines `render` returns for none, which the shell
    /// prints only when it prints a header for an empty result.
    pub fn finish(&mut self) -> Vec<String> {
        if self.seen == 0 {
            return render(&self.layout, &self.columns, &[]);
        }
        match self.layout.mode {
            Mode::Json => self
                .pending
                .take()
                .map(|last| format!("{last}]"))
                .into_iter()
                .collect(),
            Mode::Count => vec![self.seen.to_string()],
            _ => Vec::new(),
        }
    }

    /// Returns how many rows have been rendered.
    pub fn rows(&self) -> usize {
        self.seen
    }

    /// Returns the lines of a chunk of the `json` array, holding the last back.
    ///
    /// @param rows - the next rows
    /// @param first - whether these are the first rows of the result
    fn json_chunk(&mut self, rows: &[Vec<Value<'static>>], first: bool) -> Vec<String> {
        let mut out = Vec::with_capacity(rows.len());
        let mut lines = object_lines(&self.layout, &self.columns, rows);
        if first {
            if let Some(opening) = lines.first_mut() {
                opening.insert(0, '[');
            }
        }
        for line in lines {
            if let Some(held) = self.pending.replace(line) {
                out.push(format!("{held},"));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns rows that exercise quoting, a NULL, a number and text that holds the separator.
    fn sample_rows() -> Vec<Vec<Value<'static>>> {
        (0..23)
            .map(|nth| {
                vec![
                    Value::Integer(nth),
                    Value::owned_text(format!("row, {nth} \"quoted\"").as_bytes())
                        .unwrap_or(Value::Null),
                    if nth % 5 == 0 {
                        Value::Null
                    } else {
                        Value::Real(nth as f64 / 4.0)
                    },
                ]
            })
            .collect()
    }

    /// Returns every mode that streams, with the header settings that change what the first chunk
    /// adds.
    fn layouts() -> Vec<Layout> {
        let modes = [
            Mode::List,
            Mode::Csv,
            Mode::Tabs,
            Mode::Quote,
            Mode::Ascii,
            Mode::Tcl,
            Mode::C,
            Mode::Jatom,
            Mode::Line,
            Mode::Insert,
            Mode::Json,
            Mode::Jobject,
            Mode::Html,
            Mode::Count,
            Mode::Off,
        ];
        let mut layouts = Vec::new();
        for mode in modes {
            for headers in [false, true] {
                layouts.push(Layout {
                    mode,
                    headers,
                    ..Layout::default()
                });
            }
        }
        layouts
    }

    /// Chunks of one row, of several rows and of every row print what rendering the whole result
    /// at once prints, in every mode that streams.
    #[test]
    fn chunks_render_the_same_lines_as_the_whole_result() {
        let columns = vec!["id".to_string(), "label".to_string(), "ratio".to_string()];
        let rows = sample_rows();
        for layout in layouts() {
            let whole = render(&layout, &columns, &rows);
            for size in [1usize, 2, 7, rows.len(), rows.len() + 5] {
                let mut renderer = RowRenderer::new(layout.clone(), columns.clone());
                let mut streamed = Vec::new();
                for piece in rows.chunks(size) {
                    streamed.extend(renderer.chunk(piece));
                }
                streamed.extend(renderer.finish());
                assert_eq!(
                    streamed,
                    whole,
                    "mode {} with headers {} in chunks of {size}",
                    layout.mode.name(),
                    layout.headers
                );
            }
        }
    }

    /// A result with no rows renders what rendering no rows renders, which the shell prints only
    /// when it prints a header for an empty result.
    #[test]
    fn an_empty_result_renders_what_rendering_no_rows_renders() {
        let columns = vec!["id".to_string()];
        for layout in layouts() {
            let mut renderer = RowRenderer::new(layout.clone(), columns.clone());
            assert!(renderer.chunk(&[]).is_empty());
            assert_eq!(renderer.finish(), render(&layout, &columns, &[]));
        }
    }

    /// Only the modes that need every row to size a column are kept out of the streaming path.
    #[test]
    fn the_modes_that_draw_columns_do_not_stream() {
        for mode in [
            Mode::Column,
            Mode::Table,
            Mode::Box,
            Mode::Markdown,
            Mode::Qbox,
            Mode::Psql,
            Mode::Split,
        ] {
            assert!(!streams(mode), "{} sizes its columns", mode.name());
        }
        for layout in layouts() {
            assert!(streams(layout.mode), "{}", layout.mode.name());
        }
    }
}

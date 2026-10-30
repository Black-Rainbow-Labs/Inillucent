//! Rows too wide for the columnar region, kept in the delta area of a leaf with no sorted rows.
//!
//! Invariant: **the sizing pass and the encoder decide from the same rows whether they are kept in
//! the delta area, and a leaf built here holds as many consecutive such rows as the page takes.**
//! The columnar region costs a directory entry, a class array and an aligned slot array for every
//! column whatever the row holds, which is about 24 to 32 bytes a column, so a page of 32 KiB holds
//! one row of at most about a thousand columns there. The delta area costs the directory entry and
//! the row's own tagged values, so the same page holds a row of about two thousand small values, or
//! several rows of 800 to 1,000 columns. A leaf with no sorted rows and any number of delta rows is
//! an ordinary leaf (it is what a table holds after its first few inserts, before a compaction), so
//! no reader needs to know that this one was built rather than inserted into.

use super::*;
use crate::mutate::{Applied, LeafMut};

/// A table of at least this many columns is one whose columnar region may cost more than the
/// delta area per leaf; see `LeafBuilder::may_prefer_delta`.
pub(super) const DELTA_COMPARE_COLUMNS: usize = 128;

/// A leaf whose columnar region holds fewer rows than this is compared with the delta area, which
/// is tried for at most this many rows.
pub(super) const DELTA_COMPARE_ROWS: usize = 64;

/// `count` rows of a source from `at`, as a source of their own, so a sizing pass over it cannot
/// see the rows after the ones being encoded.
struct Window<'a, 'd> {
    /// The rows the window looks into.
    rows: &'a dyn Rows<'d>,
    /// The first row of the window, in `rows`.
    at: usize,
    /// How many rows the window holds.
    count: usize,
}

impl<'d> Rows<'d> for Window<'_, 'd> {
    /// Returns how many rows the window holds.
    fn len(&self) -> usize {
        self.count
    }

    /// Returns one value of the window's row.
    ///
    /// @param row - which row of the window
    /// @param column - which column of it
    fn value(&self, row: usize, column: usize) -> Datum<'d> {
        self.rows.value(self.at.saturating_add(row), column)
    }
}

impl LeafBuilder {
    /// Returns how many consecutive rows from `at` fit the delta area of an empty leaf.
    ///
    /// A row that holds a value long enough to go out of line ends the run, because the delta
    /// row built below keeps every value inline and would lose the reference the spiller has
    /// already recorded. The answer is 0 when the first row cannot be kept there at all.
    ///
    /// @param rows - the rows, sorted by key
    /// @param at - the first row to try
    /// @param limit - the most rows to try
    pub(super) fn delta_run_len<'d>(&self, rows: &dyn Rows<'d>, at: usize, limit: usize) -> usize {
        self.delta_only_page(rows, at, limit)
            .ok()
            .flatten()
            .map_or(0, |(_, placed)| placed)
    }

    /// Encodes rows that the columnar region cannot hold as one leaf with no sorted rows, or
    /// answers `None` when the ordinary encoder should run.
    ///
    /// **The layout the sizing pass handed over says which it is.** An empty layout is the
    /// sizing pass's answer for rows it placed in the delta area. With no layout the question is
    /// asked again of the first row, by the same sizing code at a full page.
    ///
    /// @param rows - the rows, sorted by key
    /// @param at - the first row
    /// @param count - how many rows are being encoded
    /// @param spilling - whether the encoder has a spiller
    /// @param layout - the widths the sizing pass derived, when the caller has them
    pub(super) fn encode_oversize_row<'d>(
        &self,
        rows: &dyn Rows<'d>,
        at: usize,
        count: usize,
        spilling: bool,
        layout: Option<&Layout>,
    ) -> DbResult<Option<Vec<u8>>> {
        if count == 0 || !spilling {
            return Ok(None);
        }
        let oversize = match layout {
            Some(held) => held.widths.is_empty(),
            None => self.is_delta_run(rows, at, count),
        };
        if !oversize {
            return Ok(None);
        }
        match self.delta_only_page(rows, at, count)? {
            Some((page, placed)) if placed == count => Ok(Some(page)),
            _ => Err(too_wide(self.columns.len(), self.page_size)),
        }
    }

    /// Reports whether the sizing pass would keep `count` rows from `at` in the delta area, for an
    /// encoder that was not handed the sizing pass's layout (a split's halves).
    ///
    /// **The same question the sizing pass answers, asked over the rows being encoded.** A table
    /// narrower than `DELTA_COMPARE_COLUMNS` never prefers the delta area to a columnar region
    /// that holds a row, so it asks only whether the first row fits the columnar region at all
    /// and pays no second pass.
    ///
    /// @param rows - the rows, sorted by key
    /// @param at - the first row
    /// @param count - how many rows are being encoded
    fn is_delta_run<'d>(&self, rows: &dyn Rows<'d>, at: usize, count: usize) -> bool {
        if self.columns.len() < DELTA_COMPARE_COLUMNS {
            return self.fit_columnar(rows, at, 1.0, true).0 == 0;
        }
        let window = Window { rows, at, count };
        self.fit_widths(&window, 0, 1.0, true).1.widths.is_empty()
    }

    /// Reports whether this table's rows are kept in the delta area when that holds more of them
    /// than the columnar region; see [`LeafBuilder::may_prefer_delta`].
    pub fn keeps_wide_rows_in_delta(&self) -> bool {
        self.columns.len() >= DELTA_COMPARE_COLUMNS
    }

    /// Returns the refusal for a row that fits neither the columnar region nor the delta area.
    ///
    /// The bulk build and the split used to answer this with "a row's keys and fixed-width
    /// columns alone are larger than a page", which names no column count and was reported by
    /// `inillucent migrate` as a malformed file. This one names the count and the page size.
    pub fn too_wide_refusal(&self) -> inillucent_base::DbError {
        too_wide(self.columns.len(), self.page_size)
    }

    /// Builds the leaf: no sorted rows, and up to `limit` consecutive rows as delta rows.
    ///
    /// Returns the page and how many rows it holds, or `None` when not even the first row can
    /// be kept. Rows arrive in key order, so each insert lands at the end of the directory.
    ///
    /// @param rows - the rows, sorted by key
    /// @param at - the first row to keep
    /// @param limit - the most rows to keep
    fn delta_only_page<'d>(
        &self,
        rows: &dyn Rows<'d>,
        at: usize,
        limit: usize,
    ) -> DbResult<Option<(Vec<u8>, usize)>> {
        let mut page = self.encode_empty()?;
        let mut placed = 0usize;
        while placed < limit {
            if !self.keeps_in_delta(&mut page, rows, at.saturating_add(placed))? {
                break;
            }
            placed = placed.saturating_add(1);
        }
        if placed == 0 {
            return Ok(None);
        }
        let first = rows.value(at, 0).as_int().unwrap_or(0);
        page::write_u64(&mut page, leaf_header::LOW_FENCE, first as u64)?;
        Ok(Some((page, placed)))
    }

    /// Inserts one row into the delta area of `page`, or answers false when the row has a value
    /// that would go out of line or the page has no room for it.
    ///
    /// @param page - the leaf being built
    /// @param rows - the rows, sorted by key
    /// @param at - the row to insert
    fn keeps_in_delta<'d>(
        &self,
        page: &mut [u8],
        rows: &dyn Rows<'d>,
        at: usize,
    ) -> DbResult<bool> {
        let values: Vec<Datum<'d>> = (0..self.columns.len())
            .map(|index| rows.value(at, index))
            .collect();
        let wide = self.row_threshold(rows, at, true);
        let spills = values.iter().enumerate().any(|(index, value)| {
            let column = self.columns.get(index);
            column.is_some_and(|column| {
                classify_at(
                    column.physical,
                    value,
                    self.column_threshold(index, wide, true),
                ) == ValueClass::Extent
            })
        });
        if spills {
            return Ok(false);
        }
        Ok(LeafMut::new(page)?.insert_delta(&self.columns, &values)? == Applied::Yes)
    }
}

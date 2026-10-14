//! A row too wide for the columnar region, kept alone in the delta area of its leaf.
//!
//! Invariant: **the sizing pass and the encoder decide from the same row whether it is kept in
//! the delta area, and a leaf built here holds exactly that one row.** The columnar region costs
//! a directory entry, a class array and an aligned slot array for every column whatever the row
//! holds, which is about 24 to 32 bytes a column, so a page of 32 KiB holds one row of at most
//! about a thousand columns there. The delta area costs the directory entry and the row's own
//! tagged values, so the same page holds a row of about two thousand small values. A leaf with
//! no sorted rows and one delta row is an ordinary leaf (it is what a table holds after its first
//! insert), so no reader needs to know that this one was built rather than inserted into.

use super::*;
use crate::mutate::{Applied, LeafMut};

impl LeafBuilder {
    /// Reports whether one row fits the delta area of an empty leaf.
    ///
    /// A row that holds a value long enough to go out of line is refused here, because the
    /// delta row built below keeps every value inline and would lose the reference the spiller
    /// has already recorded.
    ///
    /// @param rows - the rows, sorted by key
    /// @param at - the row to try
    pub(super) fn fits_in_delta_area_alone<'d>(&self, rows: &dyn Rows<'d>, at: usize) -> bool {
        self.delta_only_page(rows, at)
            .is_ok_and(|page| page.is_some())
    }

    /// Encodes a single row that the columnar region cannot hold as a leaf of its own, or
    /// answers `None` when the ordinary encoder should run.
    ///
    /// **The layout the sizing pass handed over says which it is.** An empty layout is the
    /// sizing pass's answer for a row it placed in the delta area. With no layout the question is
    /// asked again of the same row, by the same sizing code at a full page.
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
        if count != 1 || !spilling {
            return Ok(None);
        }
        let oversize = match layout {
            Some(held) => held.widths.is_empty(),
            None => self.fit_columnar(rows, at, 1.0, true).0 == 0,
        };
        if !oversize {
            return Ok(None);
        }
        match self.delta_only_page(rows, at)? {
            Some(page) => Ok(Some(page)),
            None => Err(too_wide(self.columns.len(), self.page_size)),
        }
    }

    /// Returns the refusal for a row that fits neither the columnar region nor the delta area.
    ///
    /// The bulk build and the split used to answer this with "a row's keys and fixed-width
    /// columns alone are larger than a page", which names no column count and was reported by
    /// `inillucent migrate` as a malformed file. This one names the count and the page size.
    pub fn too_wide_refusal(&self) -> inillucent_base::DbError {
        too_wide(self.columns.len(), self.page_size)
    }

    /// Builds the leaf: no sorted rows, and the row as the only delta row.
    ///
    /// @param rows - the rows, sorted by key
    /// @param at - the row to keep
    fn delta_only_page<'d>(&self, rows: &dyn Rows<'d>, at: usize) -> DbResult<Option<Vec<u8>>> {
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
            return Ok(None);
        }
        let mut page = self.encode_empty()?;
        if LeafMut::new(&mut page)?.insert_delta(&self.columns, &values)? != Applied::Yes {
            return Ok(None);
        }
        let first = values.first().and_then(Datum::as_int).unwrap_or(0);
        page::write_u64(&mut page, leaf_header::LOW_FENCE, first as u64)?;
        Ok(Some(page))
    }
}

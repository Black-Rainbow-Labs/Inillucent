//! Keys stored out of line: the prefix a leaf keeps, and the pool it reads the rest through.
//!
//! Invariant: **a leaf never guesses the order of a key it cannot read.** A key
//! value over the spill threshold is written on pages of its own, and the leaf
//! keeps its reference and a prefix. A comparison the prefix does not settle
//! reads the rest from the extents or the pool attached to the view, and a view
//! with neither refuses with an error.

use super::*;

/// How many leading bytes of a key value stored out of line stay in the leaf.
///
/// A key column value over the spill threshold is written on pages of its own,
/// and the leaf keeps the sixteen byte reference followed by these bytes. A
/// comparison reads the prefix first and fetches the rest only when the prefix
/// does not decide the order. 512 bytes is the most any page size keeps; see
/// [`key_prefix_bytes`].
pub const KEY_PREFIX_BYTES: usize = 512;

/// The bytes the leaf keeps in front of a key value stored out of line, beyond
/// its reference: a `u16` length for the prefix.
pub const KEY_PREFIX_LENGTH_BYTES: usize = 2;

/// Returns how long the prefix of an out of line key value is at a page size.
///
/// The smaller of [`KEY_PREFIX_BYTES`] and the spill threshold, so a value that
/// went out of line is always longer than its prefix. That is 512 at every page
/// size from 4,096 up and 64 at a page of 512 bytes.
///
/// @param page_size - the database's page size in bytes
pub fn key_prefix_bytes(page_size: usize) -> usize {
    KEY_PREFIX_BYTES.min(page_size / EXTENT_DIVISOR)
}

/// Returns the heap bytes a key value stored out of line costs at a page size:
/// the reference, the prefix length and the prefix.
///
/// @param page_size - the database's page size in bytes
pub fn key_extent_record_bytes(page_size: usize) -> usize {
    EXTENT_REF_BYTES
        .saturating_add(KEY_PREFIX_LENGTH_BYTES)
        .saturating_add(key_prefix_bytes(page_size))
}

/// A borrowed [`Pool`], held by a [`LeafRef`] so the view stays `Debug`.
#[derive(Clone, Copy)]
pub(crate) struct PoolHandle<'p>(pub(crate) &'p Pool);

impl std::fmt::Debug for PoolHandle<'_> {
    /// Writes a fixed name, because a pool has no useful summary to print.
    ///
    /// @param formatter - where the text goes
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Pool")
    }
}

impl<'p> LeafRef<'p> {
    /// Checks the prefix a key value stored out of line keeps in the leaf.
    ///
    /// Its length has to be the one this page size keeps for every value, and
    /// the bytes have to lie inside the page.
    ///
    /// @param row - the row's position in the sorted region
    /// @param column - which key column
    pub(super) fn check_key_prefix(&self, row: usize, column: usize) -> DbResult<()> {
        let prefix = self.column(column)?.key_prefix(row)?;
        if prefix.len() != key_prefix_bytes(self.page.len()) {
            return Err(corrupt(format!(
                "row {row} key column {column} keeps a prefix of {} bytes where this page keeps {}",
                prefix.len(),
                key_prefix_bytes(self.page.len())
            )));
        }
        Ok(())
    }

    /// Reads the key values this leaf stores out of line, sorted rows and delta rows.
    ///
    /// @param pool - the pool the extent pages are read through
    pub(crate) fn read_key_extents(&self, pool: &Pool) -> DbResult<Extents> {
        let mut held = Extents::default();
        for column in 0..self.key_columns {
            let mini = self.column(column)?;
            for row in 0..self.row_count {
                if mini.class_at(row)? == ValueClass::Extent {
                    let reference = mini.extent(row)?;
                    held.push(row, column, crate::paged::read_extent(pool, reference)?);
                }
            }
        }
        for index in 0..self.delta_count {
            for column in 0..self.key_columns {
                if let Some(reference) = self.delta_extent_at(index, column)? {
                    held.push_delta(index, column, crate::paged::read_extent(pool, reference)?);
                }
            }
        }
        Ok(held)
    }
}

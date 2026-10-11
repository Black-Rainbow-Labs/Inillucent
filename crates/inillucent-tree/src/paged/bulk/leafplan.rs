//! Pass one of a bulk build: how many rows each leaf takes, and the separator that opens it.
//!
//! Invariant: **pass one allocates and writes nothing.** It only asks the leaf builder how
//! many rows fit, so running it twice is safe and the leaf count is known before any page is
//! claimed.

use super::super::*;
use crate::separator::leaf_separator;

/// How many rows each leaf takes, and the separator that opens each one.
///
/// Pass one of the build. Nothing is encoded, allocated or written: this only asks
/// the leaf builder how many of the rows at a given offset fit, and encodes the
/// first key of each group as the separator the level above will carry.
///
/// An empty run still produces one boundary and one separator, because an empty
/// tree is still a tree: one empty leaf, so every reader has a page to land on and
/// nothing has to special-case a root that does not exist. `empty` is what tells
/// pass two to write that leaf rather than encode no rows into it.
///
/// A separator over what an interior page keeps whole is shortened here too, to
/// the shortest prefix of the leaf's first key that sorts above the previous
/// leaf's last key, exactly as a split shortens it.
///
/// @param builder - the leaf builder, which owns the page size and the layout
/// @param rows - the rows, already in key order
/// @param key_columns - how many leading columns are the key
/// @param encoding - how a key tuple becomes comparable bytes
/// @param collations - one per key column
/// @param directions - one per key column, true for descending
/// @param page_size - the file's page size, which decides how long a separator may be
pub(super) fn plan_leaves<'d>(
    builder: &LeafBuilder,
    rows: &dyn crate::leaf::Rows<'d>,
    key_columns: usize,
    encoding: KeyEncoding,
    collations: &[Collation],
    directions: &[bool],
    page_size: usize,
) -> DbResult<LeafPlan> {
    let mut boundaries: Vec<usize> = Vec::new();
    let mut separators: Vec<Vec<u8>> = Vec::new();
    let mut layouts: Vec<crate::leaf::Layout> = Vec::new();
    let mut head: Vec<Datum<'d>> = Vec::with_capacity(key_columns);
    let mut at = 0usize;
    let mut row_count = 0u64;
    let total = rows.len();
    while at < total {
        // The widths are kept for pass two, which encodes the same rows at the same widths;
        // working them out again was a pass over every value of every leaf, about 4% of an
        // `INSERT ... SELECT` of 100,000 rows (task-2226).
        let (placed, layout) = builder.fit_widths(rows, at, BULK_FILL, true);
        if placed == 0 {
            // Every oversized text and blob would have gone out of line, so what is
            // left is keys, fixed-width slots and sixteen bytes per reference. A row
            // that still does not fit is one whose *key* is most of a page.
            return Err(misuse(
                "a row's keys and fixed-width columns alone are larger than a page",
            ));
        }
        let separator = leaf_separator(
            &mut head,
            rows,
            at,
            key_columns,
            encoding,
            collations,
            directions,
            page_size,
        );
        separators.push(separator);
        boundaries.push(placed);
        layouts.push(layout);
        at = at.saturating_add(placed);
        row_count = row_count.saturating_add(placed as u64);
    }
    let empty = boundaries.is_empty();
    if empty {
        boundaries.push(0);
        separators.push(Vec::new());
    }
    Ok(LeafPlan {
        boundaries,
        separators,
        layouts,
        row_count,
        empty,
    })
}

/// What pass one worked out about the leaves.
pub(super) struct LeafPlan {
    /// How many rows each leaf holds, in order.
    pub(super) boundaries: Vec<usize>,
    /// Each leaf's first key, encoded, one per boundary.
    pub(super) separators: Vec<Vec<u8>>,
    /// The slot widths pass one priced each leaf at, one per boundary, and none for the
    /// empty leaf of an empty run.
    pub(super) layouts: Vec<crate::leaf::Layout>,
    /// How many rows the tree will hold.
    pub(super) row_count: u64,
    /// Whether the run was empty and the single leaf is the empty one.
    pub(super) empty: bool,
}

//! The columns a tree has that an older leaf of it does not.
//!
//! Invariant: **a column's default is a property of the tree, and a leaf that lacks the
//! column reads it from here.** The pad holds the tree's column directory and one fill per
//! column, so a leaf written before `ALTER TABLE ... ADD COLUMN` answers a read of the new
//! column with the default for every row it holds, and is never asked to store it.

use crate::datum::{Datum, OwnedDatum};
use crate::types::ColumnSpec;

/// The columns a tree has that an older leaf of it does not, and what they read as.
///
/// **`ALTER TABLE ... ADD COLUMN` writes no leaf.** The table's tree gains a
/// column in the catalog, and a leaf written before that holds one column fewer.
/// A reader asks the leaf for the column anyway, and this is the answer: the
/// column's default, which is the same for every row such a leaf holds. The leaf
/// is rewritten with the column in it the first time anything writes to it, see
/// `PagedTree::widen_short_leaf`.
///
/// `specs` and `fills` have one entry per column of the tree, the first columns
/// included, so a column is looked up by its position in the tree.
#[derive(Clone, Debug, Default)]
pub struct TreePad {
    /// The tree's column directory.
    specs: Vec<ColumnSpec>,
    /// What each column reads as in a leaf that does not hold it.
    fills: Vec<OwnedDatum>,
}

impl TreePad {
    /// Returns a pad over a tree's columns.
    ///
    /// @param specs - the tree's column directory
    /// @param fills - what each column reads as in a leaf that lacks it, one per column; short means NULL
    pub fn new(specs: Vec<ColumnSpec>, fills: Vec<OwnedDatum>) -> TreePad {
        TreePad { specs, fills }
    }

    /// How many columns the tree has.
    pub fn width(&self) -> usize {
        self.specs.len()
    }

    /// What one column reads as in a leaf that does not hold it.
    ///
    /// @param column - the column's position in the tree
    pub fn fill(&self, column: usize) -> Datum<'_> {
        self.fills
            .get(column)
            .map_or(Datum::Null, OwnedDatum::borrow)
    }

    /// The tree's directory entry for one column.
    ///
    /// @param column - the column's position in the tree
    pub fn spec(&self, column: usize) -> Option<ColumnSpec> {
        self.specs.get(column).copied()
    }
}

//! Attaching to a leaf view what a comparison needs: collations, directions,
//! out of line values and the pool.
//!
//! Invariant: **a view compares the way its tree is ordered.** The page carries
//! none of these and must not, so the tree attaches them wherever it opens a leaf.

use super::*;

impl<'p> LeafRef<'p> {
    /// Returns the same leaf, reading its key columns under these collations.
    ///
    /// @param collations - one per key column; short means BINARY for the rest
    pub fn with_collations(
        mut self,
        collations: &'p [inillucent_value::collation::Collation],
    ) -> LeafRef<'p> {
        self.collations = collations;
        self
    }

    /// Returns the same leaf, reading its key columns in these directions.
    ///
    /// @param directions - one per key column; short means ascending
    pub fn with_directions(mut self, directions: &'p [bool]) -> LeafRef<'p> {
        self.directions = directions;
        self
    }

    /// Returns whether one key column is stored descending.
    ///
    /// @param index - the key column's position
    pub fn descending_at(&self, index: usize) -> bool {
        self.directions.get(index).copied().unwrap_or(false)
    }

    /// Returns a comparison with the column's direction applied.
    ///
    /// **One place, so a comparison cannot be written that forgets.** Every
    /// order this leaf produces - the binary search, the bound tests, the row
    /// comparison - goes through here, so a descending column is descending for
    /// all of them or for none.
    ///
    /// @param order - the ascending comparison
    /// @param index - which key column it was made on
    pub(super) fn directed(&self, order: std::cmp::Ordering, index: usize) -> std::cmp::Ordering {
        if self.descending_at(index) {
            order.reverse()
        } else {
            order
        }
    }

    /// Attaches the out-of-line values a caller has read.
    ///
    /// Every accessor then answers for them exactly as it does for an inline
    /// value, which is what keeps the extent out of the readers: a scan does not
    /// branch on where a value is, only on whether the leaf has any.
    ///
    /// @param extents - the resolved values, which must outlive the view
    pub fn with_extents(self, extents: &'p Extents) -> LeafRef<'p> {
        LeafRef {
            extents: Some(extents),
            ..self
        }
    }

    /// Attaches the pool a comparison reads the rest of a key through.
    ///
    /// A key column value over the spill threshold is stored out of line with a
    /// prefix in the leaf. A comparison that the prefix decides never needs the
    /// pool. One that the prefix leaves open reads the whole value, and a leaf
    /// with neither this nor [`LeafRef::with_extents`] attached refuses with an
    /// error. It never guesses an order.
    ///
    /// @param pool - the pool the file is open through, which must outlive the view
    pub fn with_pool(self, pool: &'p Pool) -> LeafRef<'p> {
        LeafRef {
            pool: Some(PoolHandle(pool)),
            ..self
        }
    }

    /// Returns the collation of one key column.
    ///
    /// @param index - the key column's position
    pub fn collation_of(&self, index: usize) -> inillucent_value::collation::Collation {
        self.collations
            .get(index)
            .copied()
            .unwrap_or(inillucent_value::collation::Collation::Binary)
    }
}

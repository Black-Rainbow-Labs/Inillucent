//! The affinity and collation a comparison between two operands uses.
//!
//! Invariant: **a comparison applies the left operand's affinity first and an
//! explicit collation before an implicit one, as `sqlite3BinaryCompareCollSeq`
//! does, and this is the only place those two answers are combined.** The
//! functions moved here from `collation.rs` to keep that file under its
//! recorded size.

use inillucent_value::{Affinity, Collation};

use super::BoundExpr;

/// Returns the affinity and collation a comparison between two operands uses.
///
/// SQLite's rule, in order: if either side has a column affinity the comparison
/// applies it, with the left side winning. The collation is an explicit one on
/// the left operand, then an explicit one on the right, then the left
/// operand's implicit one, then the right's, and otherwise BINARY. "On an
/// operand" includes anywhere inside it: see [`BoundExpr::explicit_collation`].
///
/// @param left - the comparison's left operand
/// @param right - the comparison's right operand
pub fn comparison_rules(left: &BoundExpr, right: &BoundExpr) -> (Option<Affinity>, Collation) {
    comparison_rules_over(left, right, left.affinity(), right.affinity())
}

/// The rules of [`comparison_rules`] for operands whose affinities the caller
/// has already decided.
///
/// **A column of a derived table whose query gave it no affinity has none.** The
/// binder stores that as BLOB, which is also what a declared column with no
/// type has, and the two compare differently: a TEXT column meets the first with
/// TEXT affinity applied to it, and meets the second with nothing converted.
///
/// @param left - the comparison's left operand
/// @param right - the comparison's right operand
/// @param left_affinity - the left operand's affinity as the comparison sees it
/// @param right_affinity - the right operand's affinity as the comparison sees it
pub fn comparison_rules_over(
    left: &BoundExpr,
    right: &BoundExpr,
    left_affinity: Option<Affinity>,
    right_affinity: Option<Affinity>,
) -> (Option<Affinity>, Collation) {
    let affinity = match (left_affinity, right_affinity) {
        (Some(left), Some(right)) => inillucent_value::compare::comparison_affinity(left, right),
        (Some(left), None) => Some(left),
        (None, Some(right)) => Some(right),
        (None, None) => None,
    };
    let collation = left
        .explicit_collation()
        .or_else(|| right.explicit_collation())
        .or_else(|| left.collation())
        .or_else(|| right.collation())
        .unwrap_or(Collation::Binary);
    (affinity, collation)
}

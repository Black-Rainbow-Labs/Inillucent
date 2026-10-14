//! The affinity and collation a comparison between two operands uses.
//!
//! Invariant: **a comparison applies the left operand's affinity first and an
//! explicit collation before an implicit one, as `sqlite3BinaryCompareCollSeq`
//! does, and this is the only place those two answers are combined.** The
//! functions moved here from `collation.rs` to keep that file under its
//! recorded size.

use inillucent_value::{Affinity, Collation};

use super::{BoundExpr, BoundSource, SourceRows};

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

/// Returns the affinity an operand has in a comparison, reading a column of a
/// derived table, a view or a CTE that carries no affinity as having none.
///
/// A declared column with no type is stored as BLOB and the comparison applies
/// nothing to it. A derived column no arm gave an affinity is also stored as
/// BLOB, but SQLite treats it as having none, so the other operand's affinity
/// applies to it.
///
/// @param sources - the FROM terms the operand's column number is looked up in
/// @param expr - the operand
pub(super) fn seen_affinity_among(sources: &[BoundSource], expr: &BoundExpr) -> Option<Affinity> {
    if let BoundExpr::Column {
        source,
        column,
        affinity: Affinity::Blob,
        ..
    } = expr
    {
        if column_has_no_affinity(sources, *source, usize::from(*column)) {
            return None;
        }
    }
    expr.affinity()
}

/// Reports whether a column of a derived table, a CTE or a recursive CTE has no affinity.
///
/// **A derived table that only passes a column through keeps the answer of the table it reads.**
/// `SELECT x FROM (SELECT 1 AS x)` has a column that is a bare reference to the inner table's
/// `x`, and the inner `x` has no affinity. Asking only whether the outer block's own expression
/// has one found the reference's stored BLOB affinity, read as a declared column with no type,
/// so `n.parent = s.x` over two nested derived tables or a CTE read through another one applied
/// nothing to the other side and found a text `'1'` different from the integer 1 (bug hunt 10).
///
/// @param sources - the FROM terms the column number is looked up in
/// @param source - the number of the term the column belongs to
/// @param column - the column's position in that term
fn column_has_no_affinity(sources: &[BoundSource], source: usize, column: usize) -> bool {
    let Some(held) = sources.iter().find(|held| held.id == source) else {
        return false;
    };
    match &held.rows {
        SourceRows::Subquery(block) => {
            block.column_affinity_if_any(column).is_none()
                || passes_through_unaffined(sources, block, column)
        }
        // A recursive CTE's columns come from its seed, as SQLite reads
        // them from the leftmost arm: `SELECT 1 UNION ALL ...` has none.
        SourceRows::Recursive(body) => body
            .seeds
            .first()
            .is_some_and(|(_, seed)| seed.column_affinity_if_any(column).is_none()),
        _ => false,
    }
}

/// Reports whether a one arm block's result column is a bare reference to a column that has no
/// affinity.
///
/// @param sources - the FROM terms the reference is looked up in
/// @param block - the derived table's query
/// @param column - the result column's position
fn passes_through_unaffined(
    sources: &[BoundSource],
    block: &super::BoundSelect,
    column: usize,
) -> bool {
    if !block.compounds.is_empty() || !block.values.is_empty() {
        return false;
    }
    matches!(
        block.columns.get(column).map(|held| &held.expr),
        Some(BoundExpr::Column { source, column, affinity: Affinity::Blob, .. })
            if column_has_no_affinity(sources, *source, usize::from(*column))
    )
}

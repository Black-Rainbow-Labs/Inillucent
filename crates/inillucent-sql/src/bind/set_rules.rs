//! The affinity and collation rules for a value list and for a compound query.
//!
//! Invariant: **these two functions are SQLite's rules for `x IN (list)` and
//! for the columns of a compound, measured against the pinned 3.53.4 shell**,
//! and they differ from the rules for a comparison between two operands in
//! `collation.rs`: a list takes everything from its left operand, and a
//! compound takes its collation from the leftmost arm that has one.

use inillucent_value::{Affinity, Collation};

use super::{comparison_rules, result_collation, BoundExpr};

/// Returns the collation of a compound's result column, one expression per arm.
///
/// SQLite's `multiSelectCollSeq`: the leftmost arm whose expression has a
/// collation of its own decides, and an arm whose expression has none (a
/// literal, an arithmetic result) passes the question to the next arm on the
/// right. A plain column always has one, BINARY at least, so it ends the
/// search. Measured against 3.53.4: `SELECT 'A' UNION SELECT 'a' COLLATE
/// NOCASE` is one row, and `SELECT 'A' COLLATE NOCASE UNION SELECT 'a'` is
/// one row too.
///
/// @param exprs - the column's expression in each arm, leftmost first
pub fn compound_collation<'a>(exprs: impl IntoIterator<Item = &'a BoundExpr>) -> Collation {
    exprs
        .into_iter()
        .find_map(|expr| expr.explicit_collation().or_else(|| expr.collation()))
        .unwrap_or(Collation::Binary)
}

/// Returns the affinity and collation `operand IN (list)` compares with.
///
/// Measured against 3.53.4: the list items are stored under the LEFT operand's
/// affinity and looked up under the LEFT operand's collation, so an item's own
/// affinity (`1 IN (CAST('1' AS TEXT), 2)` is 0) and column collation
/// (`'A' IN (nocase_column, 'x')` is 0) do not count, and neither does an
/// explicit `COLLATE` on an item. The one exception is a list of a single
/// constant item, which SQLite's parser rewrites to `operand = +item`: then an
/// explicit `COLLATE` on the item counts as it does for `=`, and the unary plus
/// still hides the item's affinity.
///
/// @param operand - the left operand
/// @param list - the bound items between the parentheses
pub fn in_list_rules(operand: &BoundExpr, list: &[BoundExpr]) -> (Option<Affinity>, Collation) {
    let affinity = operand.affinity();
    let collation = match list {
        [only] if only.is_constant() => comparison_rules(operand, only).1,
        _ => result_collation(operand),
    };
    (affinity, collation)
}

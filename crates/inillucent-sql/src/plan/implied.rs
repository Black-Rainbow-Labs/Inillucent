//! Range bounds that a lexicographic comparison implies on its leading column.
//!
//! Invariant: **every term returned here is true of every row for which the `OR` it came from is
//! true, and the `OR` stays in the statement.** The returned term is a weaker condition that an
//! index can turn into a range. It is never a replacement: the planner keeps the original `OR` as a
//! residual that decides which of the rows in the range are kept, and drops the returned term when
//! no access path used it.
//!
//! **Why.** A row value comparison such as `(created_at, id) < (?, ?)` is bound as
//! `created_at < ? OR (created_at = ? AND id < ?)`. An `OR` is not an index range, so keyset
//! pagination over a `created_at` index that does not cover the query scanned the whole table and
//! sorted it: 73 ms warm against SQLite's 0 ms on 200,000 rows. Every row the `OR` accepts has
//! `created_at <= ?`, so that bound can drive the index, and the index walk is already in the order
//! the `ORDER BY` asks for.

use super::{split_conjunction, BoundExpr};
use crate::ast::BinaryOp;

/// Appends the implied bounds to the statement's terms and returns how many terms the statement
/// itself has.
///
/// The appended terms follow the statement's own, so the caller drops the ones no access path
/// used by testing only the first `stated` terms as residuals. A statement with a `RIGHT` or
/// `FULL` join gets none, because its terms are held back by owner and the owners list is
/// indexed by term.
///
/// @param terms - the statement's terms, extended in place
/// @param allowed - whether the statement may have implied terms
pub(super) fn add_leading_bounds(terms: &mut Vec<BoundExpr>, allowed: bool) -> usize {
    let stated = terms.len();
    if allowed {
        let implied = leading_bounds(terms);
        terms.extend(implied);
    }
    stated
}

/// Returns the bound on the leading column that each lexicographic `OR` among the terms implies.
///
/// An `OR` qualifies when its first arm compares an expression with `<`, `<=`, `>` or `>=`, and its
/// second arm is a conjunction that contains the equality of the same two operands. Then the `OR`
/// is true only when the operands are equal or the first arm holds, so `<` and `<=` imply `<=` and
/// `>` and `>=` imply `>=`. A comparison that reads a volatile function is left out, because the
/// extra term would evaluate it a second time.
///
/// @param terms - the statement's `WHERE` and `ON` conjuncts
pub(super) fn leading_bounds(terms: &[BoundExpr]) -> Vec<BoundExpr> {
    let mut bounds = Vec::new();
    for term in terms {
        let BoundExpr::Or(first, rest) = term else {
            continue;
        };
        let BoundExpr::Compare {
            op,
            left,
            right,
            affinity,
            collation,
        } = first.as_ref()
        else {
            continue;
        };
        let weaker = match op {
            BinaryOp::Less | BinaryOp::LessEqual => BinaryOp::LessEqual,
            BinaryOp::Greater | BinaryOp::GreaterEqual => BinaryOp::GreaterEqual,
            _ => continue,
        };
        if !ties_on_the_same_operands(rest, left, right) {
            continue;
        }
        let bound = BoundExpr::Compare {
            op: weaker,
            left: left.clone(),
            right: right.clone(),
            affinity: *affinity,
            collation: *collation,
        };
        if super::is_repeatable_condition(&bound) {
            bounds.push(bound);
        }
    }
    bounds
}

/// Reports whether a conjunction holds the equality of two given operands.
///
/// @param arm - the second arm of the `OR`
/// @param left - the left operand of the first arm's comparison
/// @param right - the right operand of the first arm's comparison
fn ties_on_the_same_operands(arm: &BoundExpr, left: &BoundExpr, right: &BoundExpr) -> bool {
    let mut conjuncts = Vec::new();
    split_conjunction(arm, &mut conjuncts);
    conjuncts.iter().any(|conjunct| {
        matches!(
            conjunct,
            BoundExpr::Compare { op: BinaryOp::Equal, left: l, right: r, .. }
                if l.as_ref() == left && r.as_ref() == right
        )
    })
}

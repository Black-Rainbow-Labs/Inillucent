//! `x IS TRUE`, `x IS NOT TRUE`, `x IS FALSE` and `x IS NOT FALSE`.
//!
//! Invariant: **a value is true when SQLite's boolean reading of it is
//! nonzero, never only when it is the integer 1.** `2 IS TRUE` and `0.5 IS
//! TRUE` are 1, and a text value is read as the number its leading characters
//! spell. `CASE WHEN x THEN ...` and `NOT x` already use that reading, so
//! these tests are written as a `CASE` over the operand and the comparison
//! with the integer 1 that was used before is gone. The test never yields NULL:
//! a NULL operand is neither true nor false.

use super::{comparison_rules, Binder, BoundExpr};
use crate::ast::{Expr, ExprId, Literal};
use crate::diagnostic::ParseError;
use crate::lexer::Span;

impl Binder<'_> {
    /// Binds `left IS [NOT] [DISTINCT FROM] right`.
    ///
    /// **`DISTINCT FROM` inverts the sense, and it was being dropped.** `a IS b`
    /// is already NULL-safe equality, so `a IS NOT DISTINCT FROM b` is `a IS b`
    /// and `a IS DISTINCT FROM b` is `a IS NOT b`. Binding the keyword away left
    /// `1 IS DISTINCT FROM NULL` meaning `1 IS NULL`: 0 where SQLite answers 1,
    /// and 0 again for `1 IS NOT DISTINCT FROM 1`, so both spellings answered the
    /// opposite of the truth.
    ///
    /// A written `TRUE` or `FALSE` on the right is a truth test under all four
    /// spellings, so `2 IS NOT DISTINCT FROM TRUE` is 1.
    ///
    /// @param negated - whether `NOT` was written
    /// @param distinct_from - whether `DISTINCT FROM` was written
    /// @param left - the left operand
    /// @param right - the right operand
    /// @param span - where the test was written
    pub(super) fn bind_is(
        &mut self,
        negated: bool,
        distinct_from: bool,
        left: ExprId,
        right: ExprId,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let inverted = negated != distinct_from;
        if let (Some(lefts), Some(rights)) =
            (self.row_value_parts(left), self.row_value_parts(right))
        {
            return self.bind_row_is(inverted, &lefts, &rights, span);
        }
        if let Some(select) = self.row_query(left) {
            return self.bind_row_query_is(inverted, select, right, span);
        }
        if let Some(truth) = self.truth_literal(right) {
            let operand = self.bind_expr(left)?;
            return Ok(truth_test(operand, inverted, truth));
        }
        let left = self.bind_expr(left)?;
        let right = self.bind_expr(right)?;
        let (affinity, collation) = comparison_rules(&left, &right);
        Ok(BoundExpr::Is {
            negated: inverted,
            left: Box::new(left),
            right: Box::new(right),
            affinity,
            collation,
        })
    }

    /// Returns which truth value a `TRUE` or `FALSE` written on the right of
    /// `IS` names, or `None` for any other expression.
    ///
    /// @param right - the expression after `IS`
    pub(super) fn truth_literal(&self, right: ExprId) -> Option<bool> {
        match self.ast.expr(right)? {
            Expr::Literal(Literal::Boolean(truth)) => Some(*truth),
            _ => None,
        }
    }
}

/// Builds `operand IS [NOT] TRUE` or `operand IS [NOT] FALSE`.
///
/// @param operand - the bound value being tested
/// @param negated - whether `IS NOT` was written
/// @param truth - whether the word after `IS [NOT]` was `TRUE`
pub(super) fn truth_test(operand: BoundExpr, negated: bool, truth: bool) -> BoundExpr {
    // `IS TRUE` is `CASE WHEN x THEN 1 ELSE 0 END`, and `IS FALSE` tests
    // `NOT x` instead. A NULL operand makes the `WHEN` unknown, which falls to
    // the `ELSE`, so the `ELSE` is what answers for NULL: 0 for `IS`, 1 for
    // `IS NOT`.
    let condition = if truth {
        operand
    } else {
        BoundExpr::Not(Box::new(operand))
    };
    let (matched, otherwise) = if negated { (0, 1) } else { (1, 0) };
    BoundExpr::Case {
        operand: None,
        branches: vec![(condition, BoundExpr::Integer(matched))],
        otherwise: Some(Box::new(BoundExpr::Integer(otherwise))),
        comparisons: Vec::new(),
    }
}

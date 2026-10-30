//! Binding `LIKE`, `GLOB`, `REGEXP`, `MATCH` and their `ESCAPE` clause.
//!
//! Invariant: **each pattern operator binds to the node or the call the reference builds for it.**
//! The arm lived inside `Binder::bind_expr`, which is recorded at 303 lines, and moved here when
//! the check for room on the stack went into that function.

use super::{Binder, BoundExpr, ScalarFunc};
use crate::ast::{ExprId, PatternOp};
use crate::diagnostic::ParseError;
use inillucent_value::Collation;

impl Binder<'_> {
    /// Binds one pattern operator.
    ///
    /// @param negated - whether the operator is written with `NOT`
    /// @param op - which operator it is
    /// @param operand - the expression on the left
    /// @param pattern - the expression on the right
    /// @param escape - the `ESCAPE` expression, when the statement gave one
    pub(super) fn bind_pattern_expr(
        &mut self,
        negated: bool,
        op: PatternOp,
        operand: ExprId,
        pattern: ExprId,
        escape: Option<ExprId>,
    ) -> Result<BoundExpr, ParseError> {
        if op == PatternOp::Regexp {
            // `X REGEXP Y` is sugar for `regexp(Y, X)` - the pattern
            // first - and the operator exists only because the function
            // does. The reference shell registers one, so this engine
            // registers one too, and the operator binds to it here
            // rather than refusing.
            let subject = self.bind_expr(operand)?;
            let pattern = self.bind_expr(pattern)?;
            let call = BoundExpr::Function {
                func: ScalarFunc::Regexp,
                arguments: vec![pattern, subject],
                collation: Collation::Binary,
            };
            return Ok(if negated {
                BoundExpr::Not(Box::new(call))
            } else {
                call
            });
        }
        if op == PatternOp::Match {
            // `x MATCH y` is a call to a function called `match`, which
            // does not exist - unless `x` is a column of a virtual
            // table, in which case it is a constraint the module is
            // offered and the module says what it means. That is the
            // whole of how `t MATCH 'word'` reaches FTS5.
            let left = self.bind_expr(operand)?;
            // Where `x` is not a virtual table column the `match` function
            // SQLite registers by default raises the error, and it raises
            // it when a row reaches the expression, not when the statement
            // is prepared. The physical pass compiles this node to that
            // runtime error.
            let pattern = Box::new(self.bind_expr(pattern)?);
            return Ok(BoundExpr::Pattern {
                negated,
                op: PatternOp::Match,
                operand: Box::new(left),
                pattern,
                escape: None,
            });
        }
        let operand = Box::new(self.bind_expr(operand)?);
        let pattern = Box::new(self.bind_expr(pattern)?);
        let escape = match escape {
            Some(expr) => Some(Box::new(self.bind_expr(expr)?)),
            None => None,
        };
        Ok(BoundExpr::Pattern {
            negated,
            op,
            operand,
            pattern,
            escape,
        })
    }
}

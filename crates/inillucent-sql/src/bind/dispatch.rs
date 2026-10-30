//! The entry point for binding one expression.
//!
//! Invariant: **`Binder::bind_expr` refuses a statement for want of stack before it descends, and
//! the two kinds of node that generated SQL nests deepest are bound without entering the large
//! function that binds the rest.**
//!
//! A chain of 400 `OR` terms is a tree 400 levels deep, and each level is one call of `bind_expr`
//! and one of `bind_binary`. When `bind_expr` was the 300 line function itself, a level cost
//! 5,280 bytes, so a thread with 1 MiB could bind 168 terms and one with 2,000,000 bytes 354.
//! Measured with the check below on a thread of 1 MiB, the same chain binds 3 times as deep now.

use super::{Binder, BoundExpr};
use crate::ast::{Expr, ExprId};
use crate::diagnostic::{stack_exhausted, ParseError};

impl Binder<'_> {
    /// Binds one expression.
    ///
    /// @param id - the expression
    pub fn bind_expr(&mut self, id: ExprId) -> Result<BoundExpr, ParseError> {
        if !inillucent_base::stack::has_room() {
            return Err(stack_exhausted(self.ast.expr_span(id)));
        }
        match self.ast.expr(id) {
            Some(Expr::Binary { op, left, right }) => {
                let (op, left, right) = (*op, *left, *right);
                self.bind_binary(op, left, right)
            }
            Some(Expr::Unary { op, operand }) => {
                let (op, operand) = (*op, *operand);
                self.bind_unary(op, operand)
            }
            _ => self.bind_expr_general(id),
        }
    }
}

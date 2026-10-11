//! The words `TRUE` and `FALSE`: a column of that name when one is in scope,
//! and otherwise the number each stands for.
//!
//! Invariant: **the word is read as an identifier first, as SQLite reads it.**
//! `CREATE TABLE t(true)` followed by `SELECT true FROM t` returns the column,
//! and only a missing column makes the word a number. Where the word is the
//! number, the fact that it was written as a word is kept for the one place
//! that tells the two spellings apart: whether a partial index holds the rows
//! a query wants.

use super::{Binder, BoundExpr};
use crate::ast::{self, Expr, ExprId, Literal, UnaryOp};
use crate::diagnostic::ParseError;
use crate::lexer::Span;

impl Binder<'_> {
    /// Keeps the fact that a comparison operand was written `TRUE` or `FALSE`.
    ///
    /// SQLite stores those keywords as their own kind of node, so `flag = false` is not the
    /// expression `flag = 0` when it decides whether a partial index declared `WHERE flag = 0`
    /// holds every row the query wants, and the plan scans the table. The value is the same
    /// integer. A unary plus around it keeps the two spellings apart for that comparison.
    ///
    /// @param written - the operand as the statement wrote it
    /// @param bound - the operand after binding
    pub(super) fn keep_written_boolean(&self, written: ExprId, bound: BoundExpr) -> BoundExpr {
        match (self.ast.expr(written), &bound) {
            (Some(Expr::Literal(Literal::Boolean(..))), BoundExpr::Integer(_)) => {
                BoundExpr::Unary {
                    op: UnaryOp::Identity,
                    operand: Box::new(bound),
                }
            }
            _ => bound,
        }
    }

    /// Binds the word `true` or `false`: a column of that name when one is in scope, and
    /// otherwise the number it stands for.
    ///
    /// SQLite reads these words as identifiers and turns one into 1 or 0 only when no column
    /// has the name, so `CREATE TABLE t(true)` followed by `SELECT true FROM t` returns the
    /// column. Any failure other than a missing column, such as an ambiguous name, is reported.
    ///
    /// @param name - the word as written
    /// @param span - where the word is
    pub(super) fn bind_true_or_false(
        &mut self,
        name: ast::NameId,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let value = self.ast.folded(name).eq_ignore_ascii_case(b"true");
        if self.sources.is_empty() && self.result_aliases.is_empty() {
            return Ok(BoundExpr::Integer(i64::from(value)));
        }
        match self.bind_column_reference(None, None, name, span) {
            Err(error) if is_missing_column(&error) => Ok(BoundExpr::Integer(i64::from(value))),
            other => other,
        }
    }
}

/// Reports whether a binding failure is SQLite's `no such column`, which is the only failure
/// that lets the word `true` or `false` mean a number.
///
/// @param error - the failure a column lookup returned
fn is_missing_column(error: &ParseError) -> bool {
    error.message().starts_with("no such column")
}

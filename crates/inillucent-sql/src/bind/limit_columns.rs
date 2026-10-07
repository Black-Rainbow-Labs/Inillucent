//! The columns a `LIMIT` or `OFFSET` may not read.
//!
//! Invariant: **a `LIMIT` or `OFFSET` that reads a column, outside a subquery,
//! is refused with SQLite's "no such column".** A column of an enclosing query
//! is named with its table, as SQLite names it when the reference was written
//! that way.
//! SQLite resolves both with an empty name context. Binding them in the
//! block's scope let `SELECT * FROM t LIMIT x` reach the executor, which
//! failed with an internal message about the tree it reads. Hiding every
//! scope while binding them instead broke an aggregate of an outer query that
//! a subquery's result column names, so the check is made on the bound
//! expression.

use super::{refusal, Binder, BoundExpr, BoundSelect};
use crate::diagnostic::ParseError;
use crate::lexer::Span;

impl Binder<'_> {
    /// Refuses a bound `LIMIT` or `OFFSET` that reads a column.
    ///
    /// @param bound - the block, with its `LIMIT` and `OFFSET` bound
    pub(super) fn refuse_limit_columns(&self, bound: &BoundSelect) -> Result<(), ParseError> {
        let own = self.scope();
        for expr in bound.limit.iter().chain(bound.offset.iter()) {
            if let Some(name) = self.own_column_name(expr, own) {
                return Err(refusal::no_such_column(&name, Span::default()));
            }
        }
        Ok(())
    }

    /// Returns the name of the first column an expression reads, without
    /// looking inside a subquery, which has names of its own.
    ///
    /// @param expr - the bound expression
    /// @param own - the ids of the block's own FROM terms
    fn own_column_name(&self, expr: &BoundExpr, own: &[usize]) -> Option<Vec<u8>> {
        if let BoundExpr::Column { source, column, .. } = expr {
            let term = self.sources.get(*source);
            let mut name = Vec::new();
            if let Some(term) = term.filter(|_| !own.contains(source)) {
                name.extend_from_slice(&term.alias);
                name.push(b'.');
            }
            if let Some(info) = term.and_then(|term| term.table.columns.get(usize::from(*column))) {
                name.extend_from_slice(&info.name);
            }
            return Some(name);
        }
        expr.children()
            .into_iter()
            .find_map(|child| self.own_column_name(child, own))
    }
}

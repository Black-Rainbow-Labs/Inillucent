//! Binding an expression whose value is used as it stands.
//!
//! Invariant: **a `COLLATE` that names no collation is an error only where
//! something compares or sorts with it.** SQLite looks a collation up when a
//! comparison, an `ORDER BY` or a `GROUP BY` asks for it, so `DELETE FROM t
//! WHERE ('2024-01-01' COLLATE nosuch)` runs and `WHERE a = 1 COLLATE nosuch`
//! fails. A filter, a join condition or a value being stored that is nothing
//! but such a `COLLATE` is not compared with anything, so its name is not
//! looked up. Everywhere else the binder refuses an unknown collation when the
//! expression is bound, which is the earlier of the two moments SQLite could
//! refuse it.

use super::collation::apply_collation;
use super::{Binder, BoundExpr};
use crate::ast::{Expr, ExprId};
use crate::diagnostic::ParseError;
use inillucent_value::Collation;

impl Binder<'_> {
    /// Binds a `WHERE`, `HAVING` or `ON` clause, or a value being stored.
    ///
    /// @param id - the expression
    pub(crate) fn bind_plain_value(&mut self, id: ExprId) -> Result<BoundExpr, ParseError> {
        let written = match self.ast.expr(id) {
            Some(Expr::Collate { operand, collation }) => Some((*operand, *collation)),
            _ => None,
        };
        if let Some((operand, name)) = written {
            if self.collation_named(self.ast.text(name)).is_none() {
                let bound = self.bind_plain_value(operand)?;
                return Ok(apply_collation(bound, Collation::Binary));
            }
        }
        self.bind_expr(id)
    }
}

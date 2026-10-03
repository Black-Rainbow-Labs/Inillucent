//! A result column alias used in a `WHERE` clause.
//!
//! Invariant: **an alias is looked at only after every column of every visible
//! FROM term has failed to match, and it stands for the expression it names.**
//! `SELECT a*2 AS d FROM t WHERE d > 2` is accepted by SQLite, which resolves
//! `d` to `a*2`. A real column of the same name wins, the first of two equal
//! aliases wins, and an alias that names an aggregate is the usual refusal
//! because a `WHERE` cannot hold one.

use super::{Binder, BoundExpr};
use crate::ast;
use crate::diagnostic::ParseError;

impl Binder<'_> {
    /// Makes a block's result column aliases visible to its `WHERE`.
    ///
    /// Returns whether a block was recorded, which the caller pops with
    /// [`Binder::forget_where_aliases`] once the `WHERE` is bound.
    ///
    /// @param columns - the block's result columns as written
    pub(super) fn offer_where_aliases(&mut self, columns: &[ast::ResultColumn]) -> bool {
        // Nearly no statement names an alias in its `WHERE`, and a compile has
        // an allocation budget, so a block with no alias at all is not recorded.
        if columns.iter().all(|column| column.alias.is_none()) {
            return false;
        }
        let block = columns
            .iter()
            .filter_map(|column| {
                column
                    .alias
                    .map(|alias| (self.ast.folded(alias).to_vec(), column.expr))
            })
            .collect();
        self.where_aliases.push(block);
        true
    }

    /// Takes the innermost block's aliases away again.
    pub(super) fn forget_where_aliases(&mut self) {
        self.where_aliases.pop();
    }

    /// Binds the expression a name stands for when it is a result column alias
    /// of the block being bound or of one around it.
    ///
    /// The expression is bound here, in the scope of the `WHERE` that named it,
    /// with no alias visible, so one alias cannot name another, and with
    /// aggregates refused.
    ///
    /// @param folded - the lowercase name that matched no column
    pub(super) fn bind_where_alias(
        &mut self,
        folded: &[u8],
    ) -> Result<Option<BoundExpr>, ParseError> {
        let named = self.where_aliases.iter().rev().find_map(|block| {
            block
                .iter()
                .find(|(name, _)| name.as_slice() == folded)
                .map(|(_, expr)| *expr)
        });
        let Some(expr) = named else {
            return Ok(None);
        };
        let aliases = core::mem::take(&mut self.where_aliases);
        let allowed = core::mem::replace(&mut self.allow_aggregates, false);
        let bound = self.bind_expr(expr);
        self.allow_aggregates = allowed;
        self.where_aliases = aliases;
        bound.map(Some)
    }
}

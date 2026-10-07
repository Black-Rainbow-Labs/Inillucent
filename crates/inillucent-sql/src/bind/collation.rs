//! Which collation a comparison, a sort or a grouping uses, and where an
//! explicit `COLLATE` comes from.
//!
//! Invariant: **every collation the binder decides comes from the rules in
//! this file, and they are SQLite's `sqlite3ExprCollSeq` and
//! `sqlite3BinaryCompareCollSeq`, graded against the pinned 3.53.4 shell.**
//! The executor's `expression_collation` used to be a second copy of them
//! that looked only at the top node, and the binder's own two helpers did the
//! same, so an explicit `COLLATE` inside an operand of `||` reached neither a
//! comparison nor a `GROUP BY` (task-2089). One copy, here, is what keeps a
//! comparison, an `ORDER BY`, a `DISTINCT` and a `PARTITION BY` agreeing
//! about which values are equal. The cases that grade these rules are
//! `crates/inillucent-compat/tests/corpora/differential-part8/task2089.cases`.

use inillucent_value::Collation;

use super::BoundExpr;
use crate::ast::UnaryOp;

impl BoundExpr {
    /// Returns the collation this expression carries, if it has one.
    ///
    /// SQLite's `sqlite3ExprCollSeq`, in its order: a column has its declared
    /// collation, a `CAST` and a unary `+` have their operand's, and any other
    /// expression has the explicit collation of an operand, if one has one.
    /// So `CAST(n AS TEXT) = 'A'` on a NOCASE column `n` compares with NOCASE,
    /// and `n || '' = 'A'` compares with BINARY. Measured against 3.53.4
    /// (task-2089): `CAST(n AS TEXT) = 'A'` and `+n = 'A'` answered 0 here
    /// where SQLite answers 1.
    pub fn collation(&self) -> Option<Collation> {
        match self {
            BoundExpr::Column { collation, .. } | BoundExpr::Generated { collation, .. } => {
                Some(*collation)
            }
            BoundExpr::Cast { operand, .. }
            | BoundExpr::Unary {
                op: UnaryOp::Identity,
                operand,
            } => operand.collation(),
            // One column of a subquery used as a row value reads the collation
            // of the column it stands for, as SQLite does for a `SELECT_COLUMN`.
            // A scalar subquery written alone carries Binary here and none.
            BoundExpr::Subquery {
                kind: super::SubqueryKind::Scalar,
                collation,
                ..
            } if *collation != Collation::Binary => Some(*collation),
            // The `coalesce` SQLite builds for a merged `USING` column of a
            // `FULL` join reads the collation of its first argument, so a
            // NOCASE column keeps comparing as NOCASE after the merge.
            BoundExpr::Function {
                func: crate::function::ScalarFunc::UsingCoalesce,
                arguments,
                ..
            } => arguments.first().and_then(BoundExpr::collation),
            other => other.explicit_collation(),
        }
    }

    /// Returns the collation an explicit `COLLATE` forced on this expression.
    ///
    /// This is *not* the same question as [`BoundExpr::collation`]. A column
    /// declared `COLLATE NOCASE` has an implicit collation; `x COLLATE BINARY`
    /// has an explicit one, and an explicit collation on either side of a
    /// comparison beats an implicit one on the other side.
    ///
    /// **An explicit collation reaches up through every operator and function
    /// argument (task-2089).** SQLite marks a node `EP_Collate` when any
    /// operand has the mark, and reads the collation from the first operand
    /// that has it, left first. This used to look only at the top node, so
    /// `('a' COLLATE NOCASE || 'x') = 'AX'` compared with BINARY and answered
    /// 0 where 3.53.4 answers 1, and `'a' COLLATE BINARY || 'b' COLLATE
    /// NOCASE` has to answer BINARY because the left operand is asked first.
    /// [`BoundExpr::children`] lists operands in SQLite's order for every node
    /// whose value is text. A scalar subquery has no children here, and SQLite
    /// does not carry a `COLLATE` out of one either. An aggregate or window
    /// call has no children here either, because its arguments live in the
    /// block's lists, so its reference carries the answer for them: see
    /// `explicit_argument_collation`, in `aggregate.rs`.
    pub fn explicit_collation(&self) -> Option<Collation> {
        match self {
            BoundExpr::Collate { collation, .. } => Some(*collation),
            BoundExpr::Generated { .. } => None,
            BoundExpr::Aggregate { collation, .. } | BoundExpr::WindowRef { collation, .. } => {
                *collation
            }
            other => other
                .children()
                .into_iter()
                .find_map(BoundExpr::explicit_collation),
        }
    }
}

/// Returns the collation a result column compares with.
///
/// `DISTINCT` and `GROUP BY` compare result values, and a NOCASE column makes
/// `blue` and `Blue` the same value for both. Comparing them with BINARY
/// instead returns more rows than SQLite does, which looks like a duplicate
/// rather than like a bug.
pub fn result_collation(expr: &BoundExpr) -> Collation {
    expr.explicit_collation()
        .or_else(|| expr.collation())
        .unwrap_or(Collation::Binary)
}

/// Wraps an expression in the collation an explicit `COLLATE` names.
///
/// **A `COLLATE` above a comparison does not reach the comparison (task-1979,
/// F5).** `a = b COLLATE NOCASE` parses as `a = (b COLLATE NOCASE)`, because
/// `COLLATE` binds tighter than `=`, and the comparison then reads NOCASE off
/// its own right operand through [`comparison_rules`]. `(a = b) COLLATE
/// NOCASE` is the other tree: the comparison is finished and NOCASE applies to
/// the integer it produced, where a text collation does nothing. This function
/// used to stamp the collation onto a `BoundExpr::Compare` it was handed, which
/// made the two trees answer the same and made the outer name win over the
/// inner one: measured against 3.53.4, `SELECT ('B'<'a') COLLATE NOCASE`
/// answered 0 where SQLite answers 1, and
/// `SELECT ('a' = 'A' COLLATE NOCASE) COLLATE BINARY` answered 0 where SQLite
/// answers 1 because the inner NOCASE is the comparison's and the outer BINARY
/// is the result's.
///
/// The wrapper is what carries the collation onward: [`comparison_rules`] asks
/// an operand for its [`BoundExpr::explicit_collation`], so a `COLLATE` on a
/// literal still reaches the comparison that uses it.
///
/// @param expr - the operand the `COLLATE` was written on
/// @param collation - the collation it names
pub(super) fn apply_collation(expr: BoundExpr, collation: Collation) -> BoundExpr {
    BoundExpr::Collate {
        operand: Box::new(expr),
        collation,
    }
}

#[cfg(test)]
mod tests;

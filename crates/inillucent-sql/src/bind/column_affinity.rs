//! The affinity a derived table's or a scalar subquery's column has when a
//! comparison reads it.
//!
//! Invariant: **a column of a compound `SELECT` has an affinity only when every
//! arm agrees about what the column holds, and an arm that disagrees takes the
//! affinity away from all of them.** The rules are measured against 3.53.4 with
//! a table of every pair of arm kinds, not read from its source:
//!
//! - one arm: the affinity of its expression, which is the declared affinity
//!   for a column, a `CAST` and the rowid, and none for anything else;
//! - several arms: every arm is classified as text, numeric, unknown or NULL.
//!   A column of `TEXT` affinity is text, one of `INTEGER`, `REAL` or `NUMERIC`
//!   affinity is numeric, one with no declared type is unknown. An expression
//!   with no affinity is classified by what it produces: a string literal and
//!   `||` are text, a number and arithmetic are numeric, a function call is
//!   unknown. NULL agrees with everything. When all arms have the same class and
//!   at least one of them has a real affinity, the column has that class's
//!   affinity. Otherwise it has none;
//! - a scalar subquery has the affinity of its first result column.
//!
//! So `SELECT a FROM t UNION ALL SELECT b FROM u` with `a TEXT` and `b INTEGER`
//! has no affinity, and `x = 1` over it is true for the integer row and false
//! for the text row `'1'`. Taking the first arm's affinity, which this file
//! replaces, made it true for both.

use inillucent_value::Affinity;

use super::{BoundExpr, BoundSelect, SubqueryKind};
use crate::ast::{BinaryOp, UnaryOp};

/// What an arm's column holds, as far as the compound's affinity is concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Held {
    /// Text values.
    Text,
    /// Integer or real values.
    Numeric,
    /// Values of no particular class, which disagree with every other class.
    Unknown,
    /// NULL, which agrees with every class.
    Null,
}

impl BoundSelect {
    /// Returns the affinity a reader of result column `index` applies, with
    /// "none" reported as `Affinity::Blob` the way a column with no declared
    /// type reports it.
    ///
    /// @param index - which result column
    pub fn column_affinity(&self, index: usize) -> Affinity {
        self.column_affinity_if_any(index).unwrap_or(Affinity::Blob)
    }

    /// Returns the affinity of result column `index`, or `None` when no arm of
    /// the block has an affinity of any kind.
    ///
    /// `None` and `Some(Affinity::Blob)` are different answers: the first is an
    /// expression with nothing declared, the second is a column or a clash.
    /// A comparison treats both as "apply nothing", but an enclosing compound
    /// does not.
    ///
    /// @param index - which result column
    pub fn column_affinity_if_any(&self, index: usize) -> Option<Affinity> {
        // **A `VALUES` list of several rows joined to another arm leaves the
        // column with no affinity**, whatever the other arm declares:
        // `SELECT n FROM t UNION VALUES (1), (2)` over an INTEGER `n` compares
        // as a column of no affinity, where `UNION VALUES (1)` keeps INTEGER.
        let several_rows = |block: &BoundSelect| block.values.len() > 1;
        if !self.compounds.is_empty()
            && (several_rows(self) || self.compounds.iter().any(|(_, arm)| several_rows(arm)))
        {
            return Some(Affinity::Blob);
        }
        let mut arms: Vec<&BoundExpr> = Vec::new();
        self.collect_arm_columns(index, &mut arms);
        compound_affinity(&arms)
    }

    /// Returns the affinity a scalar subquery has as an operand.
    ///
    /// **The last arm decides, not the combination of all of them.** SQLite
    /// reads the first result column of the statement it holds for the
    /// subquery, and for a compound that statement is the last arm. Measured
    /// with `(SELECT a FROM t UNION ALL SELECT b FROM u LIMIT 1) = 1.0` over a
    /// TEXT `a` and an INTEGER `b`: the text row `'1'` is equal to `1.0`,
    /// which only the INTEGER arm's affinity gives, and the same arms written
    /// in the other order and compared with `'1.0'` answer false.
    pub fn scalar_affinity(&self) -> Option<Affinity> {
        let last = self.compounds.last().map_or(self, |(_, arm)| arm);
        if last.values.is_empty() {
            last.columns
                .first()
                .and_then(|column| column.expr.affinity())
        } else {
            None
        }
    }

    /// Adds result column `index` of this block and of each later arm.
    ///
    /// A `VALUES` list contributes one expression per row, because SQLite reads
    /// each row as an arm of a compound.
    ///
    /// @param index - which result column
    /// @param into - the expressions found, first arm first
    fn collect_arm_columns<'a>(&'a self, index: usize, into: &mut Vec<&'a BoundExpr>) {
        if self.values.is_empty() {
            into.extend(self.columns.get(index).map(|column| &column.expr));
        } else {
            into.extend(self.values.iter().filter_map(|row| row.get(index)));
        }
        for (_, arm) in &self.compounds {
            arm.collect_arm_columns(index, into);
        }
    }
}

/// Combines the arms of a compound into the affinity of their column.
///
/// @param arms - the expression each arm puts in the column, first arm first
pub(super) fn compound_affinity(arms: &[&BoundExpr]) -> Option<Affinity> {
    if let [only] = arms {
        return only.affinity();
    }
    let bearing: Vec<Affinity> = arms.iter().filter_map(|arm| arm.affinity()).collect();
    let first = *bearing.first()?;
    let mut classes = arms
        .iter()
        .map(|arm| held_by(arm))
        .filter(|held| *held != Held::Null);
    let agreed = classes.next()?;
    if agreed == Held::Unknown || classes.any(|held| held != agreed) {
        return Some(Affinity::Blob);
    }
    Some(match agreed {
        Held::Text => Affinity::Text,
        _ if bearing.iter().all(|affinity| *affinity == first) => first,
        _ => Affinity::Numeric,
    })
}

/// Classifies what one arm's expression puts in the column.
///
/// @param expr - the arm's result expression
fn held_by(expr: &BoundExpr) -> Held {
    if let Some(affinity) = expr.affinity() {
        return match affinity {
            Affinity::Text => Held::Text,
            Affinity::Blob => Held::Unknown,
            numeric => {
                debug_assert!(numeric.is_numeric());
                Held::Numeric
            }
        };
    }
    match expr {
        BoundExpr::Null => Held::Null,
        BoundExpr::Integer(_) | BoundExpr::Real(_) => Held::Numeric,
        BoundExpr::Text(_) => Held::Text,
        BoundExpr::Arithmetic { op, .. } => held_by_operator(*op),
        BoundExpr::Compare { .. }
        | BoundExpr::Is { .. }
        | BoundExpr::IsNull { .. }
        | BoundExpr::And(_, _)
        | BoundExpr::Or(_, _)
        | BoundExpr::Not(_)
        | BoundExpr::Between { .. }
        | BoundExpr::InList { .. }
        | BoundExpr::Pattern { .. } => Held::Numeric,
        BoundExpr::Subquery {
            kind: SubqueryKind::Exists | SubqueryKind::In,
            ..
        } => Held::Numeric,
        BoundExpr::Unary {
            op: UnaryOp::Identity,
            operand,
        } => held_by(operand),
        BoundExpr::Unary { .. } => Held::Numeric,
        BoundExpr::Collate { operand, .. } => held_by(operand),
        BoundExpr::Case {
            branches,
            otherwise,
            ..
        } => held_by_case(branches, otherwise.as_deref()),
        _ => Held::Unknown,
    }
}

/// Classifies the result of a binary operator.
///
/// @param op - the operator
fn held_by_operator(op: BinaryOp) -> Held {
    match op {
        BinaryOp::Concat => Held::Text,
        BinaryOp::Add
        | BinaryOp::Subtract
        | BinaryOp::Multiply
        | BinaryOp::Divide
        | BinaryOp::Modulo
        | BinaryOp::BitAnd
        | BinaryOp::BitOr
        | BinaryOp::ShiftLeft
        | BinaryOp::ShiftRight => Held::Numeric,
        _ => Held::Unknown,
    }
}

/// Classifies a `CASE` by the values its branches and its `ELSE` produce.
///
/// @param branches - the `WHEN` and `THEN` pairs
/// @param otherwise - the `ELSE` arm, when written
fn held_by_case(branches: &[(BoundExpr, BoundExpr)], otherwise: Option<&BoundExpr>) -> Held {
    let mut classes = branches
        .iter()
        .map(|(_, then)| held_by(then))
        .chain(otherwise.map(held_by))
        .filter(|held| *held != Held::Null);
    let Some(first) = classes.next() else {
        return Held::Null;
    };
    if classes.all(|held| held == first) {
        first
    } else {
        Held::Unknown
    }
}

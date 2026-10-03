//! Which `ORDER BY` and `GROUP BY` terms name a result column by position.
//!
//! Invariant: **a term is an ordinal exactly when SQLite's
//! `sqlite3ExprIsInteger` says it is an integer**, graded against the pinned
//! 3.53.4 shell: a decimal or hexadecimal literal that fits a 32 bit integer,
//! under any number of unary plus and minus signs. A literal of 2147483648 or
//! more is an ordinary constant expression, and a negated one is an ordinal
//! below one, which is out of range.

use super::Binder;
use crate::ast::{Expr, ExprId, Literal, UnaryOp};

/// The largest literal SQLite takes as an ordinal.
const LARGEST_ORDINAL: usize = i32::MAX as usize;

impl Binder<'_> {
    /// Returns the one-based ordinal an expression is, if it is an integer.
    ///
    /// A negated literal answers zero, which stands for every ordinal below
    /// one: the caller reports it as out of range.
    ///
    /// @param id - the term as written
    pub(super) fn as_ordinal(&self, id: ExprId) -> Option<usize> {
        match self.ast.expr(id)? {
            Expr::Unary {
                op: UnaryOp::Identity,
                operand,
            } => self.as_ordinal(*operand),
            Expr::Unary {
                op: UnaryOp::Negate,
                operand,
            } => self.as_ordinal(*operand).map(|_| 0),
            Expr::Literal(Literal::Integer(text)) => literal_ordinal(text),
            _ => None,
        }
    }
}

/// Returns the value of an integer literal when it can be an ordinal.
///
/// @param text - the literal as written, decimal or `0x` hexadecimal
fn literal_ordinal(text: &[u8]) -> Option<usize> {
    let hex = text.len() > 2
        && text
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"0x"));
    let (digits, radix) = match hex {
        true => (text.get(2..)?, 16),
        false => (text, 10),
    };
    let mut value: usize = 0;
    // A separator inside a hexadecimal literal is not a digit; in a decimal one
    // it makes the text no integer SQLite's ordinal check accepts.
    for byte in digits.iter().filter(|byte| !(hex && **byte == b'_')) {
        let digit = (*byte as char).to_digit(radix)?;
        value = value
            .saturating_mul(radix as usize)
            .saturating_add(digit as usize);
    }
    (value <= LARGEST_ORDINAL).then_some(value)
}

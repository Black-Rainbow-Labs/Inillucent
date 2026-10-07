//! The operator precedence table, as data.
//!
//! Invariant: precedence lives in one table that tests read, not in the shape
//! of a hand-written descent. A table can be checked against the published
//! order in a loop; a nest of functions can only be checked by reading it.
//!
//! The order is SQLite's own, weakest binding first:
//!
//! ```text
//! OR
//! AND
//! NOT (unary, prefix)
//! = == <> != > >= < <= IS IS NOT IN LIKE GLOB MATCH REGEXP BETWEEN ISNULL NOTNULL
//! & | << >>
//! + -
//! * / %
//! ||  -> ->>
//! COLLATE (postfix)
//! ~ + - (unary, prefix)
//! ```
//!
//! SQLite gives every comparison and quasi-comparison the same precedence,
//! which is why `a = b IS NULL` parses as `(a = b) IS NULL` rather than as
//! `a = (b IS NULL)`. Splitting them into separate levels is the most common
//! way to get this wrong.

use crate::lexer::Punctuator;

/// A binding power: the precedence a parser compares against.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Power(pub u8);

/// Below every operator; where a fresh expression starts.
pub const LOWEST: Power = Power(0);
/// `OR`.
pub const OR: Power = Power(1);
/// `AND`.
pub const AND: Power = Power(2);
/// Prefix `NOT`.
pub const NOT: Power = Power(3);
/// `=`, `<>`, `IS`, `IN`, `LIKE`, `BETWEEN` and the other quasi-comparisons.
pub const COMPARISON: Power = Power(4);
/// `<`, `<=`, `>`, `>=`.
///
/// **One level tighter than `=`, as SQLite's grammar declares them.** Lemon's
/// table lists `IS MATCH LIKE BETWEEN IN ISNULL NOTNULL NE EQ` on one line and
/// `GT LE LT GE` on the next, so `2 = 1 < 3` is `2 = (1 < 3)`, which is false,
/// and `1 BETWEEN 0 AND x <= 3` takes `x <= 3` as its upper bound. Sharing one
/// level read both from the left and answered 1 for each.
pub const RELATIONAL: Power = Power(5);
/// `&`, `|`, `<<`, `>>`.
pub const BITWISE: Power = Power(6);
/// `<->`, `<=>`, `<#>`, `<+>`, `<~>`, `<%>`.
///
/// **The same level as the bitwise operators, which is where PostgreSQL puts
/// them.** pgvector's distances are ordinary user-defined operators there, and
/// PostgreSQL gives "any other operator" a slot that binds tighter than a
/// comparison and looser than `+`. That is the slot that makes
/// `WHERE v <=> q < 0.5` and `ORDER BY v <=> q` parse the way anybody writing
/// them means, and it is the only property of the level that matters: nothing
/// mixes a distance with a shift.
pub const DISTANCE: Power = Power(6);
/// `+` and `-`.
pub const ADDITIVE: Power = Power(7);
/// `*`, `/`, `%`.
pub const MULTIPLICATIVE: Power = Power(8);
/// `||`, `->`, `->>`.
pub const CONCAT: Power = Power(9);
/// Postfix `COLLATE`.
pub const COLLATE: Power = Power(10);
/// Prefix `~`, `+`, `-`.
pub const UNARY: Power = Power(11);

/// Returns the binding power of an infix punctuator, when it has one.
pub fn infix_power(punctuator: Punctuator) -> Option<Power> {
    let power = match punctuator {
        Punctuator::Equal | Punctuator::NotEqual => COMPARISON,
        Punctuator::Less
        | Punctuator::LessEqual
        | Punctuator::Greater
        | Punctuator::GreaterEqual => RELATIONAL,
        Punctuator::BitAnd | Punctuator::BitOr | Punctuator::ShiftLeft | Punctuator::ShiftRight => {
            BITWISE
        }
        Punctuator::L2Distance
        | Punctuator::CosineDistance
        | Punctuator::NegativeInnerProduct
        | Punctuator::L1Distance
        | Punctuator::HammingDistance
        | Punctuator::JaccardDistance => DISTANCE,
        Punctuator::Plus | Punctuator::Minus => ADDITIVE,
        Punctuator::Star | Punctuator::Slash | Punctuator::Percent => MULTIPLICATIVE,
        Punctuator::Concat | Punctuator::Arrow | Punctuator::DoubleArrow => CONCAT,
        _ => return None,
    };
    Some(power)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The published order, weakest first. A change to the table that does not
    /// change this list has changed how SQL parses.
    #[test]
    fn the_levels_are_in_the_published_order() {
        let levels = [
            LOWEST,
            OR,
            AND,
            NOT,
            COMPARISON,
            RELATIONAL,
            BITWISE,
            ADDITIVE,
            MULTIPLICATIVE,
            CONCAT,
            COLLATE,
            UNARY,
        ];
        for pair in levels.windows(2) {
            let (weaker, stronger) = (pair.first().copied(), pair.get(1).copied());
            assert!(weaker < stronger, "{weaker:?} !< {stronger:?}");
        }
    }

    /// `=` and `<>` share the level of `IS`, which decides how `a = b IS NULL`
    /// parses, and the ordering comparisons bind one level tighter, which
    /// decides how `a = b < c` parses. Both are SQLite's grammar.
    #[test]
    fn equality_and_ordering_comparisons_have_their_own_levels() {
        for punctuator in [Punctuator::Equal, Punctuator::NotEqual] {
            assert_eq!(infix_power(punctuator), Some(COMPARISON), "{punctuator:?}");
        }
        for punctuator in [
            Punctuator::Less,
            Punctuator::LessEqual,
            Punctuator::Greater,
            Punctuator::GreaterEqual,
        ] {
            assert_eq!(infix_power(punctuator), Some(RELATIONAL), "{punctuator:?}");
        }
    }

    /// Concatenation binds tighter than arithmetic, which is not the rule most
    /// languages use and is the rule SQLite uses.
    #[test]
    fn concatenation_binds_tighter_than_arithmetic() {
        assert!(infix_power(Punctuator::Concat) > infix_power(Punctuator::Star));
        assert!(infix_power(Punctuator::Star) > infix_power(Punctuator::Plus));
        assert!(infix_power(Punctuator::Plus) > infix_power(Punctuator::BitOr));
    }

    /// Punctuation that is not an operator has no power at all, so the Pratt
    /// loop stops on it rather than treating it as a weak operator.
    #[test]
    fn non_operators_have_no_power() {
        for punctuator in [
            Punctuator::LeftParen,
            Punctuator::RightParen,
            Punctuator::Comma,
            Punctuator::Semicolon,
            Punctuator::Dot,
            Punctuator::BitNot,
        ] {
            assert_eq!(infix_power(punctuator), None, "{punctuator:?}");
        }
    }
}

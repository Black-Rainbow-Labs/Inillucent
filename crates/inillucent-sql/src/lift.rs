//! Turning the literal values of an `INSERT ... VALUES` into parameters, so
//! statements that differ only in those values share one compiled plan.
//!
//! Invariant: **a statement is rewritten only where a literal and a bound
//! parameter mean the same thing.** That is a lone literal that is a whole
//! element of a `VALUES` row: a number, a negated number or a quoted string.
//! A literal inside an expression, a function call or a subquery stays where it
//! is, because there the binder may read the literal itself. Anything this
//! does not recognise answers `None`, and the statement is compiled as written.
//!
//! **Why** (task-2191). A script of single row inserts, which is what `.dump`
//! writes and what a person pastes into the shell, compiled every statement
//! from scratch. The compile was a third of each insert, and SQLite pays the
//! same. With the values lifted out, the second statement onwards is a lexer
//! pass and a lookup in the plan cache.

use std::fmt::Write as _;

use crate::bind::BoundExpr;
use crate::keyword::Keyword;
use crate::lexer::{Lexer, Punctuator, Span, Token, TokenKind};

/// The most values one statement may have lifted.
///
/// A multi row `INSERT` of thousands of rows is compiled once anyway, and
/// keying the plan cache by a text of thousands of parameters would hold a plan
/// nothing asks for twice.
const MOST_VALUES: usize = 64;

/// The longest string literal that is lifted, in bytes.
///
/// The parser checks a literal's length against the engine's limit while it
/// reads it, and a bound value is not checked the same way. Leaving long
/// strings in the text keeps that check where it was.
const LONGEST_TEXT: usize = 1 << 16;

/// One value taken out of a statement's text.
#[derive(Clone, Debug, PartialEq)]
pub enum LiftedValue {
    /// An integer literal, with its sign.
    Integer(i64),
    /// A real literal, or an integer literal too large for 64 bits.
    Real(f64),
    /// A string literal, with doubled quotes undoubled.
    Text(Vec<u8>),
}

/// A statement with its literal values replaced by `?1`, `?2` and so on.
#[derive(Clone, Debug, PartialEq)]
pub struct Lifted {
    /// The rewritten text.
    pub text: String,
    /// The values, `?1` first.
    pub values: Vec<LiftedValue>,
    /// How many values make one row, when the statement had several rows of
    /// lone literals and `text` is the one row template `VALUES (?1, ...)`.
    ///
    /// **Several rows as one template** (task-2191). An `INSERT` of twenty
    /// thousand rows built a syntax tree node, a bound expression and a
    /// compiled expression for every literal. With every element of every row
    /// a lone literal and every row the same width, the rows are values and
    /// the statement is the one row form run over all of them; `values` then
    /// holds every row in order. `None` means `values` binds `text` once.
    pub row_width: Option<usize>,
}

/// Rewrites an `INSERT ... VALUES` whose rows hold lone literals, or answers
/// `None` when the statement is anything else.
///
/// The statement must hold no parameter of its own, must end after its last
/// row (an upsert or a `RETURNING` clause answers `None`), and must lift at
/// least one value.
///
/// @param sql - one statement
pub fn lift_insert_literals(sql: &str) -> Option<Lifted> {
    let source = sql.as_bytes();
    let mut lexer = Lexer::new(source);
    let first = lexer.next_token().ok()?;
    if !matches!(
        first.keyword(),
        Some(Keyword::INSERT) | Some(Keyword::REPLACE)
    ) {
        return None;
    }
    let values_end = skip_to_values(&mut lexer)?;
    // Sized from the text, at about one token for every three bytes, so a
    // statement of twenty thousand rows does not grow it a dozen times over
    // (task-2191); a one row statement still asks for a few dozen.
    let mut rows = Vec::with_capacity(source.len().saturating_sub(values_end) / 3 + 8);
    loop {
        let token = lexer.next_token().ok()?;
        match token.kind {
            TokenKind::EndOfInput => break,
            TokenKind::Parameter => return None,
            _ => rows.push(token),
        }
    }
    let (lifts, widths) = lone_literals(source, &rows)?;
    if let Some(width) = one_width_of_lone_literals(&widths) {
        return Some(template_of(sql, values_end, width, lifts));
    }
    if lifts.is_empty() || lifts.len() > MOST_VALUES {
        return None;
    }
    Some(rewrite(sql, lifts))
}

/// Returns the width every row shares when there are several rows and every
/// element of every row was lifted, and `None` otherwise.
///
/// @param widths - each row's element count and how many of them were lifted
fn one_width_of_lone_literals(widths: &[(usize, usize)]) -> Option<usize> {
    let (width, _) = *widths.first()?;
    let shared = widths.len() >= 2
        && (1..=MOST_VALUES).contains(&width)
        && widths
            .iter()
            .all(|(elements, lifted)| *elements == width && *lifted == width);
    shared.then_some(width)
}

/// Builds the one row template `... VALUES (?1, ..., ?width)` and keeps every
/// row's values, in order.
///
/// @param sql - the statement
/// @param values_end - where the `VALUES` keyword ends
/// @param width - how many values make one row
/// @param lifts - every lifted value, row by row
fn template_of(
    sql: &str,
    values_end: usize,
    width: usize,
    lifts: Vec<(Span, LiftedValue)>,
) -> Lifted {
    let mut text = String::with_capacity(values_end.saturating_add(width.saturating_mul(5)));
    text.push_str(sql.get(..values_end).unwrap_or(""));
    text.push_str(" (");
    for number in 1..=width {
        if number > 1 {
            text.push_str(", ");
        }
        let _ = write!(text, "?{number}");
    }
    text.push(')');
    Lifted {
        text,
        values: lifts.into_iter().map(|(_, value)| value).collect(),
        row_width: Some(width),
    }
}

/// Moves the lexer past the `VALUES` keyword of the statement's own clause.
///
/// @param lexer - the lexer, just past `INSERT` or `REPLACE`
fn skip_to_values(lexer: &mut Lexer<'_>) -> Option<usize> {
    let mut depth = 0usize;
    loop {
        let token = lexer.next_token().ok()?;
        match token.kind {
            TokenKind::EndOfInput | TokenKind::Parameter => return None,
            TokenKind::Punctuator(Punctuator::LeftParen) => depth = depth.saturating_add(1),
            TokenKind::Punctuator(Punctuator::RightParen) => depth = depth.checked_sub(1)?,
            _ if depth == 0 && token.keyword() == Some(Keyword::VALUES) => {
                return Some(token.span.end as usize)
            }
            _ => {}
        }
    }
}

/// Finds every lone literal element of the rows after `VALUES`.
///
/// Answers `None` when the tokens are not rows followed by an optional
/// semicolon, or when a literal cannot be converted the way the binder would.
///
/// @param source - the statement's bytes
/// @param tokens - the tokens after `VALUES`
fn lone_literals(source: &[u8], tokens: &[Token]) -> Option<LoneLiterals> {
    // A lifted value takes at least two tokens, itself and a comma or the
    // closing parenthesis, and a row at least three.
    let mut lifts = Vec::with_capacity(tokens.len() / 2 + 1);
    let mut widths = Vec::with_capacity(tokens.len() / 3 + 1);
    let mut at = 0usize;
    loop {
        if !tokens.get(at)?.is(Punctuator::LeftParen) {
            return None;
        }
        let before = lifts.len();
        let (next, elements) = row_elements(source, tokens, at.saturating_add(1), &mut lifts)?;
        widths.push((elements, lifts.len().saturating_sub(before)));
        at = next;
        match tokens.get(at) {
            None => return Some((lifts, widths)),
            Some(token) if token.is(Punctuator::Comma) => at = at.saturating_add(1),
            Some(token)
                if token.is(Punctuator::Semicolon) && at.saturating_add(1) == tokens.len() =>
            {
                return Some((lifts, widths));
            }
            Some(_) => return None,
        }
    }
}

/// What [`lone_literals`] found: every lifted value with where it was, and
/// for each row how many elements it had and how many of them were lifted.
type LoneLiterals = (Vec<(Span, LiftedValue)>, Vec<(usize, usize)>);

/// Reads one row's elements and lifts the lone literals among them.
///
/// Returns the position just past the row's closing parenthesis.
///
/// @param source - the statement's bytes
/// @param tokens - the tokens after `VALUES`
/// @param start - the position just past the row's opening parenthesis
/// @param lifts - where lifted values are added
fn row_elements(
    source: &[u8],
    tokens: &[Token],
    start: usize,
    lifts: &mut Vec<(Span, LiftedValue)>,
) -> Option<(usize, usize)> {
    let mut depth = 1usize;
    let mut element = start;
    let mut at = start;
    let mut elements = 0usize;
    while depth > 0 {
        let token = *tokens.get(at)?;
        if token.is(Punctuator::LeftParen) {
            depth = depth.saturating_add(1);
        } else if token.is(Punctuator::RightParen) {
            depth = depth.saturating_sub(1);
        }
        if depth == 0 || (depth == 1 && token.is(Punctuator::Comma)) {
            if let Some(lifted) = lone_literal(source, tokens.get(element..at)?)? {
                lifts.push(lifted);
            }
            elements = elements.saturating_add(1);
            element = at.saturating_add(1);
        }
        at = at.saturating_add(1);
    }
    Some((at, elements))
}

/// Converts one row element when it is a lone literal.
///
/// Answers `Some(None)` for an element that is not one, which stays in the
/// text, and `None` for a literal the binder would refuse, so the statement is
/// compiled as written and reports the refusal itself.
///
/// @param source - the statement's bytes
/// @param element - the element's tokens
fn lone_literal(source: &[u8], element: &[Token]) -> Option<Option<(Span, LiftedValue)>> {
    let (negated, token) = match element {
        [token] => (false, *token),
        [minus, token] if minus.is(Punctuator::Minus) => (true, *token),
        _ => return Some(None),
    };
    let span = match negated {
        true => element.first()?.span.to(token.span),
        false => token.span,
    };
    let value = match token.kind {
        TokenKind::Integer => integer_value(token.text(source), negated, token.span)?,
        TokenKind::Float => {
            let value = crate::bind::literal::real_literal(token.text(source));
            LiftedValue::Real(if negated { -value } else { value })
        }
        TokenKind::String if !negated => {
            let text = crate::lexer::string_text(source, token);
            if text.len() > LONGEST_TEXT {
                return Some(None);
            }
            LiftedValue::Text(text.into_owned())
        }
        _ => return Some(None),
    };
    Some(Some((span, value)))
}

/// Converts an integer literal the way the binder does, sign folded in.
///
/// @param digits - the literal as written, without a sign
/// @param negated - whether a minus came before it
/// @param span - where the literal was written
fn integer_value(digits: &[u8], negated: bool, span: Span) -> Option<LiftedValue> {
    let mut text = Vec::with_capacity(digits.len().saturating_add(1));
    if negated {
        text.push(b'-');
    }
    text.extend_from_slice(digits);
    match crate::bind::literal::checked_integer_literal(&text, span).ok()? {
        BoundExpr::Integer(value) => Some(LiftedValue::Integer(value)),
        BoundExpr::Real(value) => Some(LiftedValue::Real(value)),
        _ => None,
    }
}

/// Builds the rewritten text, each lifted span replaced by its parameter.
///
/// @param sql - the statement
/// @param lifts - the spans and values, in the order they appear
fn rewrite(sql: &str, lifts: Vec<(Span, LiftedValue)>) -> Lifted {
    let mut text = String::with_capacity(sql.len());
    let mut values = Vec::with_capacity(lifts.len());
    let mut copied = 0usize;
    for (number, (span, value)) in lifts.into_iter().enumerate() {
        text.push_str(sql.get(copied..span.start as usize).unwrap_or(""));
        // Written into the text directly; `to_string` allocated a string per
        // value only to copy it in.
        let _ = write!(text, "?{}", number.saturating_add(1));
        copied = span.end as usize;
        values.push(value);
    }
    text.push_str(sql.get(copied..).unwrap_or(""));
    Lifted {
        text,
        values,
        row_width: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_row_of_lone_literals_becomes_parameters() {
        let lifted = lift_insert_literals("INSERT INTO t (a, b, c) VALUES ('it''s', -5, 2.5);")
            .expect("lifted");
        assert_eq!(lifted.text, "INSERT INTO t (a, b, c) VALUES (?1, ?2, ?3);");
        assert_eq!(
            lifted.values,
            vec![
                LiftedValue::Text(b"it's".to_vec()),
                LiftedValue::Integer(-5),
                LiftedValue::Real(2.5)
            ]
        );
    }

    #[test]
    fn literals_inside_expressions_stay_in_the_text() {
        let lifted =
            lift_insert_literals("INSERT INTO t VALUES (upper('a'), 1 + 2, (SELECT 3), 4)")
                .expect("lifted");
        assert_eq!(
            lifted.text,
            "INSERT INTO t VALUES (upper('a'), 1 + 2, (SELECT 3), ?1)"
        );
        assert_eq!(lifted.values, vec![LiftedValue::Integer(4)]);
    }

    #[test]
    fn the_extremes_convert_as_the_binder_converts_them() {
        let lifted = lift_insert_literals(
            "INSERT INTO t VALUES (-9223372036854775808, 9223372036854775808, 0x10, -0.0)",
        )
        .expect("lifted");
        assert_eq!(lifted.values.first(), Some(&LiftedValue::Integer(i64::MIN)));
        assert_eq!(
            lifted.values.get(1),
            Some(&LiftedValue::Real(9_223_372_036_854_775_808.0))
        );
        assert_eq!(lifted.values.get(2), Some(&LiftedValue::Integer(16)));
        match lifted.values.get(3) {
            Some(LiftedValue::Real(value)) => assert!(value.is_sign_negative()),
            other => panic!("expected a negative zero, got {other:?}"),
        }
    }

    #[test]
    fn statements_it_cannot_rewrite_safely_are_left_alone() {
        for sql in [
            "SELECT 1",
            "INSERT INTO t VALUES (?1, 2)",
            "INSERT INTO t VALUES (1) ON CONFLICT DO NOTHING",
            "INSERT INTO t VALUES (1) RETURNING a",
            "INSERT INTO t DEFAULT VALUES",
            "INSERT INTO t SELECT 1",
            "INSERT INTO t VALUES (upper('a'))",
            "INSERT INTO t VALUES (0x10000000000000000)",
            "WITH c AS (SELECT 1) INSERT INTO t VALUES (1)",
        ] {
            assert_eq!(lift_insert_literals(sql), None, "{sql}");
        }
    }

    #[test]
    fn several_rows_are_numbered_in_order() {
        let lifted =
            lift_insert_literals("REPLACE INTO t VALUES (1, 'a'), (2, NULL)").expect("lifted");
        assert_eq!(lifted.text, "REPLACE INTO t VALUES (?1, ?2), (?3, NULL)");
        assert_eq!(lifted.values.len(), 3);
        assert_eq!(lifted.row_width, None);
    }

    #[test]
    fn rows_of_lone_literals_become_one_row_template() {
        let lifted =
            lift_insert_literals("INSERT INTO t (a, b) VALUES (1, 'a'), (-2, 'b'), (3.5, 'c');")
                .expect("lifted");
        assert_eq!(lifted.text, "INSERT INTO t (a, b) VALUES (?1, ?2)");
        assert_eq!(lifted.row_width, Some(2));
        assert_eq!(
            lifted.values,
            vec![
                LiftedValue::Integer(1),
                LiftedValue::Text(b"a".to_vec()),
                LiftedValue::Integer(-2),
                LiftedValue::Text(b"b".to_vec()),
                LiftedValue::Real(3.5),
                LiftedValue::Text(b"c".to_vec()),
            ]
        );
        // Rows of different widths are the parser's to refuse, so they are
        // not made into a template.
        let uneven = lift_insert_literals("INSERT INTO t VALUES (1, 2), (3)").expect("lifted");
        assert_eq!(uneven.row_width, None);
    }
}

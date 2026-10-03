//! Turning a literal's source text into the value it denotes.
//!
//! Invariant: **the scanner is SQLite's, and the text is read where it lies.** A literal's
//! bytes are in the parse arena and stay there: nothing here copies them to decide what
//! number they are, because a compile that allocates per literal is a compile whose cost
//! is its allocator (task-2006). The conversions themselves are
//! `inillucent_value::numeric`'s, which reproduce `sqlite3Atoi64` and `sqlite3AtoF` digit
//! for digit.

use std::borrow::Cow;

use crate::bind::BoundExpr;

/// Converts an integer literal, refusing a hexadecimal one that cannot fit.
///
/// **SQLite refuses a hexadecimal literal of more than sixteen significant
/// digits while it parses**: `0x10000000000000000` is
/// `hex literal too big: 0x10000000000000000`, where a hexadecimal literal that
/// fits in sixteen digits wraps into a signed integer. Leading zeros do not
/// count. The message carries the literal with its sign, because the
/// parser folds a minus into the literal before it measures it.
///
/// @param text - the literal as written, with a leading `-` if it was negated
/// @param span - where the literal was written
pub(crate) fn checked_integer_literal(
    text: &[u8],
    span: crate::lexer::Span,
) -> Result<BoundExpr, crate::diagnostic::ParseError> {
    let digits = text.strip_prefix(b"-").unwrap_or(text);
    let hex = digits
        .get(..2)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"0x"));
    if hex {
        let significant = digits
            .iter()
            .skip(2)
            .filter(|byte| **byte != b'_')
            .skip_while(|byte| **byte == b'0')
            .count();
        if significant > 16 {
            return Err(crate::bind::refused(
                format!("hex literal too big: {}", String::from_utf8_lossy(text)),
                span,
            ));
        }
    }
    Ok(integer_literal(text))
}

/// Converts a real literal's text into its value, dropping `_` separators.
///
/// The lexer accepts a `_` between two digits in the integer part, the fraction
/// and the exponent (`1_0.5`, `1.5_5`, `1e1_0`), and the separators are not part
/// of the number. Like the integer path, nothing is copied when there is none.
///
/// @param text - the literal as written, without a sign
pub(crate) fn real_literal(text: &[u8]) -> f64 {
    let cleaned: Cow<'_, [u8]> = match text.contains(&b'_') {
        true => Cow::Owned(text.iter().copied().filter(|byte| *byte != b'_').collect()),
        false => Cow::Borrowed(text),
    };
    inillucent_value::numeric::atof(&cleaned, inillucent_value::TextEncoding::Utf8).value
}

/// Converts an integer literal's text into a bound value.
///
/// A decimal literal too large for `i64` becomes a real, which is what SQLite
/// does rather than failing, and a hexadecimal literal wraps into `i64`, which
/// is also what SQLite does.
///
/// @param text - the literal as it was written, with a leading `-` if it was negated
pub(crate) fn integer_literal(text: &[u8]) -> BoundExpr {
    // The sign is read off first, so `0x` is still recognised under one: the
    // binder folds a unary minus into the literal (see `Expr::Unary`), and
    // `-0x10` arrives here as `-0x10` rather than as an operator over `0x10`.
    let (negative, digits) = match text.first() {
        Some(b'-') => (true, text.get(1..).unwrap_or(&[])),
        _ => (false, text),
    };
    if digits.len() > 2
        && digits.first() == Some(&b'0')
        && digits
            .get(1)
            .is_some_and(|byte| byte.eq_ignore_ascii_case(&b'x'))
    {
        let mut value: u64 = 0;
        for byte in digits
            .get(2..)
            .unwrap_or(&[])
            .iter()
            .filter(|b| **b != b'_')
        {
            let digit = (*byte as char).to_digit(16).unwrap_or(0) as u64;
            value = value.wrapping_mul(16).wrapping_add(digit);
        }
        let value = value as i64;
        return BoundExpr::Integer(if negative {
            value.wrapping_neg()
        } else {
            value
        });
    }
    // **The separator is stripped only when there is one** (task-2006). This collected
    // every literal into a new `Vec<u8>` to remove the `_`s, and almost no literal has
    // one - so `SELECT 1` allocated eight bytes to copy the byte `1` unchanged. A
    // backtrace on each allocation the compile makes found this one here. Scanning for
    // the separator reads the same bytes the filter would have read, and allocates
    // nothing when it finds none.
    let cleaned: Cow<'_, [u8]> = match text.contains(&b'_') {
        true => Cow::Owned(text.iter().copied().filter(|byte| *byte != b'_').collect()),
        false => Cow::Borrowed(text),
    };
    let (value, syntax) =
        inillucent_value::numeric::atoi64(&cleaned, inillucent_value::TextEncoding::Utf8);
    if syntax.is_exact() {
        return BoundExpr::Integer(value);
    }
    let parsed = inillucent_value::numeric::atof(&cleaned, inillucent_value::TextEncoding::Utf8);
    BoundExpr::Real(parsed.value)
}

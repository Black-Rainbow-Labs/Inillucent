//! Reading a virtual table's declaration and a pragma's argument.
//!
//! Invariant: nothing here touches a database. These are four pure functions
//! over types this crate already owns, a `Declaration` and a `PragmaArgument`,
//! and they are here rather than in a connection crate because **both** engines
//! need them and neither should have to depend on the other to get them.
//!
//! They lived in `inillucent-session`, which is the old engine's connection, and
//! the new engine's statement path imported them from there. That was the last
//! thing tying the new engine to the old one that was not itself an engine: two
//! helpers that parse text. Moving them down removes the edge without changing
//! a caller, because `inillucent-session` re-exports both under their old paths.

use crate::bind::BoundExpr;
use crate::catalog_view::ColumnInfo;
use crate::directive::PragmaArgument;
use crate::vtab::Declaration;

/// Returns the columns a module's declaration provides.
///
/// A declared column carries a name, a declared type, an affinity, a collation
/// and whether it is hidden. Everything else a `ColumnInfo` can say - NOT NULL,
/// a default, a primary-key position, a generated expression - is something a
/// `CREATE TABLE` says and a module's declaration does not, so it is left at
/// the value that means "unsaid" rather than guessed at.
///
/// @param declaration - what the module answered when it was connected
pub fn declared_columns(declaration: &Declaration) -> Vec<ColumnInfo> {
    declaration
        .columns
        .iter()
        .map(|column| ColumnInfo {
            folded: column.name.to_ascii_lowercase(),
            name: column.name.clone(),
            declared_type: column.declared_type.clone(),
            affinity: column.affinity,
            collation: column.collation.clone(),
            not_null: false,
            not_null_conflict: None,
            primary_key_conflict: None,
            default_sql: None,
            primary_key_position: None,
            hidden: column.hidden,
            generated: false,
            stored: true,
            generated_sql: None,
        })
        .collect()
}

/// Reads a pragma argument as text.
///
/// @param argument - the argument as the parser produced it
pub fn argument_text(argument: &PragmaArgument) -> String {
    match argument {
        PragmaArgument::Name(name) => String::from_utf8_lossy(name).into_owned(),
        PragmaArgument::Value(expr) => expression_text(expr),
    }
}

/// Returns the text a bound pragma argument spells.
///
/// `PRAGMA cache_size = -4000` is a unary minus over a literal rather than a
/// negative literal, because that is what the grammar has. Reading only the
/// literal made every negative setting read as zero.
///
/// @param expr - the argument's expression
fn expression_text(expr: &BoundExpr) -> String {
    match expr {
        BoundExpr::Text(text) => String::from_utf8_lossy(text).into_owned(),
        BoundExpr::Integer(value) => value.to_string(),
        BoundExpr::Real(value) => value.to_string(),
        BoundExpr::Unary { op, operand } => match op {
            crate::ast::UnaryOp::Negate => format!("-{}", expression_text(operand)),
            crate::ast::UnaryOp::Identity => expression_text(operand),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

/// Reads a pragma argument as the boolean SQLite accepts.
///
/// SQLite reads `on`, `yes` and `true` as one, a number that starts with a
/// digit as itself, and everything else as zero, which is why `PRAGMA
/// foreign_keys = maybe` and `PRAGMA foreign_keys = -1` turn them off. See
/// [`sqlite_boolean`].
///
/// @param argument - the argument as the parser produced it
pub fn argument_boolean(argument: &PragmaArgument) -> bool {
    sqlite_boolean(&argument_text(argument), false)
}

/// Reads a pragma argument as an integer.
///
/// @param argument - the argument as the parser produced it
pub fn argument_integer(argument: &PragmaArgument) -> i64 {
    argument_text(argument).trim().parse().unwrap_or(0)
}

/// Reads a 32 bit integer the way SQLite's `sqlite3GetInt32` does.
///
/// An optional sign and decimal digits, or `0x` and up to eight hex digits.
/// Anything after the digits is ignored, so `1.5` reads as 1. Text that is not
/// a number, or a number that does not fit in 32 bits, is `None`. SQLite reads
/// 4294967295 as nothing rather than as -1, which is why
/// `PRAGMA user_version = 4294967295` reads back 0.
///
/// @param text - the argument's text
pub fn sqlite_int32(text: &str) -> Option<i32> {
    let bytes = text.as_bytes();
    let (negative, digits) = match bytes.first() {
        Some(b'-') => (true, bytes.get(1..).unwrap_or_default()),
        Some(b'+') => (false, bytes.get(1..).unwrap_or_default()),
        _ => (false, bytes),
    };
    if digits.first() == Some(&b'0')
        && matches!(digits.get(1), Some(b'x' | b'X'))
        && digits.get(2).is_some_and(u8::is_ascii_hexdigit)
        && !negative
    {
        return hex_int32(digits.get(2..).unwrap_or_default());
    }
    let significant: Vec<u8> = digits
        .iter()
        .copied()
        .skip_while(|byte| *byte == b'0')
        .take_while(u8::is_ascii_digit)
        .collect();
    if !digits.first().is_some_and(u8::is_ascii_digit) || significant.len() > 10 {
        return None;
    }
    let value = significant.iter().fold(0i64, |held, digit| {
        held.saturating_mul(10)
            .saturating_add(i64::from(digit.saturating_sub(b'0')))
    });
    let signed = if negative { -value } else { value };
    i32::try_from(signed).ok()
}

/// Reads the eight hex digits `sqlite_int32` allows after `0x`.
///
/// @param digits - the text after `0x`
fn hex_int32(digits: &[u8]) -> Option<i32> {
    let trimmed: Vec<u8> = digits
        .iter()
        .copied()
        .skip_while(|byte| *byte == b'0')
        .collect();
    let used: Vec<u8> = trimmed
        .iter()
        .copied()
        .take_while(u8::is_ascii_hexdigit)
        .collect();
    if used.len() > 8 || trimmed.get(used.len()).is_some_and(u8::is_ascii_hexdigit) {
        return None;
    }
    let value = used.iter().fold(0u32, |held, digit| {
        let nibble = char::from(*digit).to_digit(16).unwrap_or(0);
        held.wrapping_mul(16).wrapping_add(nibble)
    });
    if value & 0x8000_0000 != 0 {
        return None;
    }
    i32::try_from(value).ok()
}

/// Reads an integer the way SQLite's `sqlite3Atoi` does: 0 when there is none.
///
/// @param text - the argument's text
pub fn sqlite_atoi(text: &str) -> i32 {
    sqlite_int32(text).unwrap_or(0)
}

/// Reads a 64 bit integer the way SQLite's `sqlite3DecOrHexToI64` does.
///
/// Returns the number the text starts with, 0 when it starts with none, and
/// whether the whole text was one number. A pragma such as `mmap_size` takes the
/// number whether or not anything followed it; `threads` ignores an argument
/// for which the second part is false. A number too large for 64 bits is
/// limited to the largest or smallest.
///
/// @param text - the argument's text
pub fn sqlite_integer(text: &str) -> (i64, bool) {
    let bytes = text.as_bytes();
    if bytes.first() == Some(&b'0') && matches!(bytes.get(1), Some(b'x' | b'X')) {
        let digits: Vec<u8> = bytes
            .get(2..)
            .unwrap_or_default()
            .iter()
            .copied()
            .skip_while(|byte| *byte == b'0')
            .collect();
        let used: Vec<u8> = digits
            .iter()
            .copied()
            .take_while(u8::is_ascii_hexdigit)
            .collect();
        let value = used.iter().fold(0u64, |held, digit| {
            let nibble = char::from(*digit).to_digit(16).unwrap_or(0);
            held.wrapping_mul(16).wrapping_add(u64::from(nibble))
        });
        return (value as i64, used.len() == digits.len() && used.len() <= 16);
    }
    let rest = text.trim_start_matches([' ', '\t', '\n', '\r', '\u{b}', '\u{c}']);
    let (negative, unsigned) = match rest.as_bytes().first() {
        Some(b'-') => (true, rest.get(1..).unwrap_or_default()),
        Some(b'+') => (false, rest.get(1..).unwrap_or_default()),
        _ => (false, rest),
    };
    let digits: Vec<u8> = unsigned.bytes().take_while(u8::is_ascii_digit).collect();
    let magnitude = digits.iter().fold(0i128, |held, digit| {
        held.saturating_mul(10)
            .saturating_add(i128::from(digit.saturating_sub(b'0')))
            .min(i128::from(i64::MAX) + 1)
    });
    let after = unsigned.get(digits.len()..).unwrap_or_default();
    let whole = !digits.is_empty()
        && after
            .trim_start_matches([' ', '\t', '\n', '\r', '\u{b}', '\u{c}'])
            .is_empty();
    let signed = if negative { -magnitude } else { magnitude };
    let clamped = signed.clamp(i128::from(i64::MIN), i128::from(i64::MAX));
    (
        i64::try_from(clamped).unwrap_or(0),
        whole && clamped == signed,
    )
}

/// Reads a setting the way SQLite's `getSafetyLevel` does.
///
/// A number is read as a number, so `PRAGMA synchronous = 5` reads back 5.
/// `on`, `yes` and `true` are 1; `off`, `no` and `false` are 0; `full` is 2 and
/// `extra` is 3 unless `omit_full` is set, which a plain boolean pragma sets.
/// Any other word is `default`, and so is a negative number, because a sign is
/// not a digit.
///
/// @param text - the argument's text
/// @param omit_full - whether `full` and `extra` are refused as words
/// @param default - what any other text means
pub fn sqlite_safety_level(text: &str, omit_full: bool, default: u8) -> u8 {
    if text.as_bytes().first().is_some_and(u8::is_ascii_digit) {
        return u8::try_from(sqlite_atoi(text) & 0xff).unwrap_or(0);
    }
    let words: [(&str, u8); 8] = [
        ("on", 1),
        ("no", 0),
        ("off", 0),
        ("false", 0),
        ("yes", 1),
        ("true", 1),
        ("extra", 3),
        ("full", 2),
    ];
    for (word, value) in words {
        if text.eq_ignore_ascii_case(word) && (!omit_full || value <= 1) {
            return value;
        }
    }
    default
}

/// Reads a boolean setting the way SQLite's `sqlite3GetBoolean` does.
///
/// @param text - the argument's text
/// @param default - what text that is not a boolean means
pub fn sqlite_boolean(text: &str, default: bool) -> bool {
    sqlite_safety_level(text, true, u8::from(default)) != 0
}

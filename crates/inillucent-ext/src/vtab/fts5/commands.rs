//! The values the FTS5 settings commands accept.
//!
//! Invariant: `accepted` says yes for exactly the values SQLite's `sqlite3Fts5ConfigSetValue`
//! stores and no for the ones it reports as `SQL logic error`. A setting is an integer in the
//! sense of `sqlite3_value_numeric_type`: an integer, or text that reads as an integer. A real
//! number such as `1.5` is not one, and neither is text that is not a number.

use inillucent_value::Value;

/// Returns the integer a setting's value is, when SQLite reads it as one.
///
/// @param value - the value written in the `rank` column of the command
fn integer_of(value: &Value<'static>) -> Option<i64> {
    match value {
        Value::Integer(number) => Some(*number),
        Value::Text(text)
            if inillucent_value::numeric::looks_numeric(text.raw(), text.encoding()) =>
        {
            match inillucent_value::cast::numerify(value.clone()) {
                Value::Integer(number) => Some(number),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns the integer as C's `int`, which is what the checks compare.
///
/// @param value - the value written in the `rank` column of the command
fn int_of(value: &Value<'static>) -> Option<i64> {
    integer_of(value).map(|number| i64::from(number as i32))
}

/// Reports whether a settings command may be given this value.
///
/// A command that is not a setting is not asked about here, and a setting this answers nothing
/// for is accepted, because the caller checks only the names it knows.
///
/// @param command - the command, in lower case
/// @param value - the value written in the `rank` column
pub(super) fn accepted(command: &str, value: &Value<'static>) -> bool {
    let number = int_of(value);
    match command {
        "pgsz" => number.is_some_and(|number| (32..=65536).contains(&number)),
        "hashsize" => number.is_some_and(|number| number > 0),
        "automerge" => number.is_some_and(|number| (0..=64).contains(&number)),
        "usermerge" => number.is_some_and(|number| (2..=16).contains(&number)),
        "crisismerge" | "secure-delete" | "insttoken" => number.is_some_and(|number| number >= 0),
        // A value that is not an integer is refused, and any integer is accepted.
        "deletemerge" => number.is_some(),
        "rank" => rank_text(value).is_some_and(|text| rank_is_valid(&text)),
        _ => true,
    }
}

/// Returns the value a setting is stored as in `%_config`.
///
/// **Text that reads as a number is stored as the number**, as SQLite's
/// `sqlite3Fts5StorageConfigValue` binds it: `'4050'` for `pgsz` is kept as the integer 4050.
/// The `rank` setting is text.
///
/// @param command - the command, in lower case
/// @param value - the value written in the `rank` column, already accepted
pub(super) fn stored(command: &str, value: Value<'static>) -> Value<'static> {
    match &value {
        Value::Text(text)
            if command != "rank"
                && inillucent_value::numeric::looks_numeric(text.raw(), text.encoding()) =>
        {
            match inillucent_value::cast::numerify(value.clone()).into_owned() {
                Ok(number) => number,
                Err(_) => value,
            }
        }
        _ => value,
    }
}

/// Returns the text a `rank` value reads as, which is its own text or the text of a number.
///
/// @param value - the value written in the `rank` column
fn rank_text(value: &Value<'static>) -> Option<String> {
    match value {
        Value::Text(text) => Some(String::from_utf8_lossy(&text.utf8_bytes()).into_owned()),
        Value::Integer(number) => Some(number.to_string()),
        Value::Real(number) => Some(number.to_string()),
        Value::Blob(blob) => Some(String::from_utf8_lossy(blob.raw()).into_owned()),
        Value::Null => None,
    }
}

/// Reports whether a character can be part of a bareword, as FTS5 reads one.
///
/// @param byte - one byte of the text
fn is_bareword(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// Skips spaces, which are the only whitespace FTS5's configuration parser knows.
///
/// @param bytes - the text
/// @param at - where to start
fn skip_spaces(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at) == Some(&b' ') {
        at = at.saturating_add(1);
    }
    at
}

/// Skips one SQL literal, as `fts5ConfigSkipLiteral` does, and returns where it ends.
///
/// A literal is `NULL`, a number, a quoted string or a blob. Nothing here reads an exponent, so
/// `1e3` is the number `1` followed by text that is not a separator.
///
/// @param bytes - the text
/// @param at - where the literal starts
fn skip_literal(bytes: &[u8], at: usize) -> Option<usize> {
    let first = bytes.get(at).copied().unwrap_or(0);
    match first {
        b'n' | b'N' => bytes
            .get(at..at.saturating_add(4))
            .filter(|word| word.eq_ignore_ascii_case(b"null"))
            .map(|_| at.saturating_add(4)),
        b'x' | b'X' => {
            if bytes.get(at.saturating_add(1)) != Some(&b'\'') {
                return None;
            }
            let mut end = at.saturating_add(2);
            while bytes.get(end).is_some_and(u8::is_ascii_hexdigit) {
                end = end.saturating_add(1);
            }
            let even = end.saturating_sub(at).is_multiple_of(2);
            (bytes.get(end) == Some(&b'\'') && even).then_some(end.saturating_add(1))
        }
        b'\'' => {
            let mut end = at.saturating_add(1);
            loop {
                match bytes.get(end) {
                    None => return None,
                    Some(b'\'') => {
                        end = end.saturating_add(1);
                        if bytes.get(end) != Some(&b'\'') {
                            return Some(end);
                        }
                        end = end.saturating_add(1);
                    }
                    Some(_) => end = end.saturating_add(1),
                }
            }
        }
        _ => {
            let mut end = at;
            if matches!(bytes.get(end), Some(b'+' | b'-')) {
                end = end.saturating_add(1);
            }
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end = end.saturating_add(1);
            }
            if bytes.get(end) == Some(&b'.')
                && bytes
                    .get(end.saturating_add(1))
                    .is_some_and(u8::is_ascii_digit)
            {
                end = end.saturating_add(2);
                while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                    end = end.saturating_add(1);
                }
            }
            (end != at).then_some(end)
        }
    }
}

/// Reports whether text is a ranking function: a bareword, `(`, literals separated by commas,
/// and `)`, as `sqlite3Fts5ConfigParseRank` reads it.
///
/// @param text - the value written for the `rank` command
fn rank_is_valid(text: &str) -> bool {
    let bytes = text.as_bytes();
    let name_start = skip_spaces(bytes, 0);
    let mut at = name_start;
    while bytes.get(at).copied().is_some_and(is_bareword) {
        at = at.saturating_add(1);
    }
    if at == name_start {
        return false;
    }
    at = skip_spaces(bytes, at);
    if bytes.get(at) != Some(&b'(') {
        return false;
    }
    at = skip_spaces(bytes, at.saturating_add(1));
    if bytes.get(at) == Some(&b')') {
        return true;
    }
    loop {
        at = skip_spaces(bytes, at);
        let Some(after) = skip_literal(bytes, at) else {
            return false;
        };
        at = skip_spaces(bytes, after);
        match bytes.get(at) {
            Some(b')') => return true,
            Some(b',') => at = at.saturating_add(1),
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Settings take integers in the range SQLite allows and nothing else.
    #[test]
    fn a_setting_takes_the_integers_sqlite_allows() {
        assert!(accepted("automerge", &Value::Integer(64)));
        assert!(!accepted("automerge", &Value::Integer(65)));
        assert!(!accepted("usermerge", &Value::Integer(1)));
        assert!(!accepted("pgsz", &Value::Integer(31)));
        assert!(accepted("pgsz", &Value::Integer(65536)));
        assert!(!accepted("automerge", &Value::Real(1.5)));
    }

    /// A ranking function is a name and a list of literals in parentheses.
    #[test]
    fn a_rank_is_a_function_call_of_literals() {
        assert!(rank_is_valid("bm25()"));
        assert!(rank_is_valid("bm25(1.0, 2.0)"));
        assert!(rank_is_valid("foo('a', null, x'00')"));
        assert!(!rank_is_valid("bogus"));
        assert!(!rank_is_valid("bm25(a)"));
        assert!(!rank_is_valid(""));
    }
}

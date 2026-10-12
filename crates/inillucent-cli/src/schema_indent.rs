//! `.schema --indent`: a `CREATE` statement with one column on each line.
//!
//! Invariant: the output is the statement's own text with white space changed
//! and line breaks added between tokens. No token is added, removed or changed,
//! so the indented statement means what the stored one means. The rules are the
//! reference shell's `shell_format_schema`, because a script reads this output
//! and `.schema --indent` is compared with the reference's byte for byte.

/// A statement shorter than this, after white space is collapsed, stays on one line.
const WRAP_AT: usize = 79;

/// Returns a statement the way `.schema --indent` prints it, with its `;`.
///
/// White space is collapsed to single spaces first. A statement of 79
/// characters or more is then broken after the opening parenthesis and after
/// each comma at the top level, and the closing parenthesis goes on a line of its
/// own. An index with a `WHERE` clause is broken before each `AND` instead.
///
/// @param sql - the statement as the schema table stores it
pub fn indented(sql: &str) -> String {
    if sql.is_empty() {
        return String::new();
    }
    let collapsed = collapse_white_space(sql.as_bytes());
    let is_index = starts_with_ignoring_case(sql.as_bytes(), b"CREATE INDEX")
        || starts_with_ignoring_case(sql.as_bytes(), b"CREATE UNIQUE INDEX");
    let mut out = if collapsed.len() >= WRAP_AT {
        break_lines(&collapsed, is_index)
    } else {
        collapsed
    };
    out.push(b';');
    String::from_utf8_lossy(&out).into_owned()
}

/// Reports whether a byte is a C white space character.
///
/// @param byte - the byte
fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0b | 0x0c | b'\r')
}

/// Reports whether a byte is an ASCII letter or digit.
///
/// @param byte - the byte
fn is_alphanumeric(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
}

/// Reports whether `text` begins with `word`, ignoring ASCII case.
///
/// @param text - the bytes to look at
/// @param word - the word to look for
fn starts_with_ignoring_case(text: &[u8], word: &[u8]) -> bool {
    text.get(..word.len())
        .is_some_and(|start| start.eq_ignore_ascii_case(word))
}

/// Reports whether only white space or a comment follows on the current line.
///
/// @param rest - the bytes after the comma being considered
fn white_space_to_end_of_line(rest: &[u8]) -> bool {
    for (at, byte) in rest.iter().enumerate() {
        if *byte == b'\n' {
            return true;
        }
        if is_space(*byte) {
            continue;
        }
        return *byte == b'-' && rest.get(at + 1) == Some(&b'-');
    }
    true
}

/// Collapses runs of white space to one character and trims both ends.
///
/// A space after an opening parenthesis is dropped, a space before a
/// parenthesis is dropped, and a carriage return that a space follows becomes a
/// newline.
///
/// @param sql - the statement's bytes
fn collapse_white_space(sql: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(sql.len());
    let mut at = 0;
    while sql.get(at).is_some_and(|byte| is_space(*byte)) {
        at += 1;
    }
    for byte in sql.iter().skip(at) {
        if is_space(*byte) {
            if out.last() == Some(&b'\r') {
                out.pop();
                out.push(b'\n');
            }
            if out
                .last()
                .is_some_and(|last| is_space(*last) || *last == b'(')
            {
                continue;
            }
        } else if (*byte == b'(' || *byte == b')') && out.last().is_some_and(|last| is_space(*last))
        {
            out.pop();
        }
        out.push(*byte);
    }
    while out.last().is_some_and(|last| is_space(*last)) {
        out.pop();
    }
    out
}

/// Breaks a collapsed statement into lines.
///
/// @param src - the collapsed statement
/// @param is_index - whether it is `CREATE INDEX` or `CREATE UNIQUE INDEX`
fn break_lines(src: &[u8], is_index: bool) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(src.len() + 16);
    let mut line: Vec<u8> = Vec::new();
    let mut quote_end: u8 = 0;
    let mut depth: i32 = 0;
    let mut lines = 0usize;
    let mut in_where = false;
    let mut at = 0usize;
    while let Some(&byte) = src.get(at) {
        let ahead = src.get(at..).unwrap_or_default();
        if byte == quote_end {
            quote_end = 0;
        } else if quote_end != 0 {
            // Inside a quoted name or a comment nothing is a break.
        } else if matches!(byte, b'"' | b'\'' | b'`') {
            quote_end = byte;
        } else if byte == b'[' {
            quote_end = b']';
        } else if byte == b'-' && src.get(at + 1) == Some(&b'-') {
            quote_end = b'\n';
        } else if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            depth -= 1;
            if lines > 0 && depth == 0 && !line.is_empty() && !in_where {
                out.append(&mut line);
                out.push(b'\n');
            }
        } else if matches!(byte, b'w' | b'W') && depth == 0 && is_index && word_at(ahead, b"WHERE")
        {
            in_where = true;
        } else if in_where && matches!(byte, b'a' | b'A') && depth == 0 && word_at(ahead, b"AND") {
            out.append(&mut line);
            out.extend_from_slice(b"\n    ");
        }
        line.push(byte);
        let breaks_here = byte == b'('
            || byte == b'\n'
            || (byte == b',' && !white_space_to_end_of_line(src.get(at + 1..).unwrap_or_default()));
        if depth == 1 && quote_end == 0 && breaks_here && !in_where {
            if byte == b'\n' {
                line.pop();
            }
            out.append(&mut line);
            out.extend_from_slice(b"\n  ");
            lines += 1;
            while src.get(at + 1).is_some_and(|next| is_space(*next)) {
                at += 1;
            }
        }
        at += 1;
    }
    out.append(&mut line);
    out
}

/// Reports whether a whole word begins `text`, ignoring ASCII case.
///
/// The byte after the word may not be a letter, a digit or an underscore.
///
/// @param text - the bytes from the position being considered
/// @param word - the word
fn word_at(text: &[u8], word: &[u8]) -> bool {
    if !starts_with_ignoring_case(text, word) {
        return false;
    }
    match text.get(word.len()) {
        Some(next) => !is_alphanumeric(*next) && *next != b'_',
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A short statement is not broken into columns, and its white space is
    /// collapsed: a space before a parenthesis goes, runs of spaces become one,
    /// and a newline stays because it is white space of a kind a space does not
    /// replace.
    #[test]
    fn a_short_statement_is_not_broken_into_columns() {
        assert_eq!(
            indented("CREATE TABLE t (  a,   b )"),
            "CREATE TABLE t(a, b);"
        );
        assert_eq!(
            indented("CREATE TABLE t (  a,\n  b )"),
            "CREATE TABLE t(a,\nb);"
        );
    }

    /// A long table is broken after the parenthesis and each comma.
    #[test]
    fn a_long_table_has_one_column_on_each_line() {
        let sql = "CREATE TABLE people(id INTEGER PRIMARY KEY, first_name TEXT NOT NULL, last_name TEXT NOT NULL, born INTEGER)";
        assert_eq!(
            indented(sql),
            "CREATE TABLE people(\n  id INTEGER PRIMARY KEY,\n  first_name TEXT NOT NULL,\n  last_name TEXT NOT NULL,\n  born INTEGER\n);"
        );
    }
}

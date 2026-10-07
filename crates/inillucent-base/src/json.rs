//! Escaping a string into the body of a JSON string, once.
//!
//! Invariant: **every JSON string this workspace writes is escaped by this
//! function.** There is one correct answer and there were three implementations
//! of it (task-1946, M4): `inillucent-cli/src/json.rs`, which writes `--output
//! json`; `inillucent-scalar/src/json/node.rs`, which is what SQL's `json_quote`
//! and every other JSON function produce; and `inillucent-sim/src/trace.rs`,
//! which writes a crash campaign's event trace.
//!
//! The first two were character-for-character identical. The third was two
//! branches shorter: it escaped the five characters JSON names and the control
//! range, and did *not* escape `\u{8}` and `\u{c}` as `\b` and `\f`. Both
//! spellings are valid JSON - `` and `\b` are the same string - so nothing
//! was wrong with the trace, but a reader comparing the two files could not tell
//! whether the difference was a decision or an omission. That is the cost of
//! three copies, and it is why this is one.
//!
//! `inillucent-base` is where it goes because all three already depend on it and
//! `docs/invariants/layering.toml` already allows each of them that edge.

/// Appends `text` to `out` with the characters JSON forbids in a string escaped.
///
/// **Appends rather than returns**, because two of the three callers were
/// building a larger string and paid for an allocation per value to do it.
///
/// What is escaped, and why each:
///
/// - `"` and `\`, which end the string and begin an escape;
/// - `\n`, `\r`, `\t`, `\u{8}` and `\u{c}`, which JSON gives two-character
///   spellings and which are far more readable than their numeric form;
/// - every other character below `\u{20}`, which JSON forbids raw, as
///   `\uXXXX`;
/// - `\u{7f}`, DELETE, which JSON permits raw and which
///   `inillucent-scalar`'s copy escaped. It is kept escaped: a raw DELETE in a
///   result a terminal prints is a character that does something rather than
///   one that shows, and the escaped form is the same string.
///
/// Everything else is appended as itself, including every character above
/// ASCII: JSON is Unicode and `\u` escaping a character that does not need it
/// would make a result that is correct and unreadable.
///
/// @param out - the string being built, appended to
/// @param text - the text to escape into it
pub fn escape_into(out: &mut String, text: &str) {
    escape_with(out, text, needs_escape);
}

/// Appends `text` to `out` escaped the way SQLite's JSON functions escape it.
///
/// The same as [`escape_into`] except that DELETE is left raw. SQLite writes
/// it raw, so `json_quote(char(127))` is three characters long there, and a
/// program that compared or measured the result saw a different string here.
/// The command line's own JSON output keeps escaping it, for the terminal
/// reason [`escape_into`] gives.
///
/// @param out - the string being built, appended to
/// @param text - the SQL text to escape
pub fn escape_sql_into(out: &mut String, text: &str) {
    escape_with(out, text, |byte| byte != 0x7f && needs_escape(byte));
}

/// Returns `text` escaped the way SQLite's JSON functions escape it.
///
/// @param text - the SQL text to escape
pub fn escape_sql(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_sql_into(&mut out, text);
    out
}

/// Appends `text` with every byte a predicate names escaped.
///
/// @param out - the string being built, appended to
/// @param text - the text to escape
/// @param escaped - whether a byte is written escaped
fn escape_with(out: &mut String, text: &str, escaped: impl Fn(u8) -> bool) {
    out.reserve(text.len());
    // **Runs that need no escape are copied whole** (task-2191). Every byte
    // this function escapes is ASCII, and an ASCII byte never occurs inside a
    // multi-byte UTF-8 sequence, so a byte scan finds exactly the characters
    // the per-character loop below would change, and the text between them is
    // appended in one copy. Almost all text is such runs: it was a push per
    // character for every value of every JSON result.
    let mut rest = text;
    while let Some(at) = rest.bytes().position(&escaped) {
        let (plain, tail) = rest.split_at(at);
        out.push_str(plain);
        let Some(character) = tail.chars().next() else {
            break;
        };
        escape_char_into(out, character);
        rest = tail.get(character.len_utf8()..).unwrap_or_default();
    }
    out.push_str(rest);
}

/// Reports whether one byte is a character `escape_into` writes escaped.
///
/// @param byte - the byte
fn needs_escape(byte: u8) -> bool {
    byte == b'"' || byte == b'\\' || byte < 0x20 || byte == 0x7f
}

/// Appends one character the way `escape_into` writes it.
///
/// @param out - the string being built, appended to
/// @param character - the character
fn escape_char_into(out: &mut String, character: char) {
    match character {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        '\u{8}' => out.push_str("\\b"),
        '\u{c}' => out.push_str("\\f"),
        other if (other as u32) < 0x20 || other == '\u{7f}' => {
            out.push_str("\\u");
            // Four lower-case hexadecimal digits, written by hand rather
            // than through `format!`, because this is the inner loop of
            // every JSON result the engine produces and a `format!` here
            // allocates once per control character.
            let code = other as u32;
            for shift in [12u32, 8, 4, 0] {
                let digit = (code >> shift) & 0xf;
                out.push(char::from_digit(digit, 16).unwrap_or('0'));
            }
        }
        other => out.push(other),
    }
}

/// Returns `text` with the characters JSON forbids in a string escaped.
///
/// The allocating form, for a caller that wants the escaped string by itself.
///
/// @param text - the text to escape
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_into(&mut out, text);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every escape SQLite's `json_quote` produces, and the ones it leaves
    /// alone.
    ///
    /// The pairs are what the reference writes, which is what
    /// `crates/inillucent-compat/tests/differential/json.rs` compares this engine against
    /// case by case; this is the same claim at the level of the function.
    #[test]
    fn every_escape_json_quote_produces() {
        for (raw, escaped) in [
            ("", ""),
            ("plain", "plain"),
            ("a\"b", "a\\\"b"),
            ("a\\b", "a\\\\b"),
            ("a\nb", "a\\nb"),
            ("a\rb", "a\\rb"),
            ("a\tb", "a\\tb"),
            ("a\u{8}b", "a\\bb"),
            ("a\u{c}b", "a\\fb"),
            ("a\u{0}b", "a\\u0000b"),
            ("a\u{1}b", "a\\u0001b"),
            ("a\u{1f}b", "a\\u001fb"),
            ("a\u{7f}b", "a\\u007fb"),
        ] {
            assert_eq!(escape(raw), escaped, "escaping {raw:?}");
        }
    }

    /// A character above ASCII is written as itself, not as a `\u` escape.
    ///
    /// JSON is Unicode. Escaping these would produce a correct result nobody
    /// can read, and it is not what the reference does either.
    #[test]
    fn text_above_ascii_is_left_alone() {
        for text in ["café", "日本語", "🛟", "\u{a0}"] {
            assert_eq!(escape(text), text);
        }
    }

    /// The appending form and the allocating one agree.
    #[test]
    fn appending_and_allocating_agree() {
        let mut built = String::from("before:");
        escape_into(&mut built, "a\"b\nc");
        assert_eq!(built, format!("before:{}", escape("a\"b\nc")));
    }

    /// A character that needs no escape costs no escape, which is what makes
    /// this cheap enough to be on the result path.
    #[test]
    fn nothing_is_escaped_that_does_not_have_to_be() {
        let plain = "the quick brown fox, 0123456789 !@#$%^&*()_+-=[]{};':,./<>?";
        assert_eq!(escape(plain), plain);
    }
}

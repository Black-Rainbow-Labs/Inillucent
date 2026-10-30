//! The JSON value tree, in the shape the binary format stores it.
//!
//! Invariant: a node remembers the text it was parsed from. `json('{"a":1.50}')`
//! answers `{"a":1.50}` and not `{"a":1.5}`, and `jsonb('{"b":0x10}')` stores
//! the four bytes `0x10` rather than the two bytes `16`, because the pinned
//! release stores what it read and converts only where the output format
//! demands it. A tree of `f64` and `String` would be a different database:
//! every number would be renormalised on the way through, and every JSONB blob
//! would differ from the one SQLite writes for the same input.
//!
//! The variants are therefore the binary format's element types rather than
//! JSON's six value kinds, which is what makes the encoder a walk rather than a
//! translation.

/// One node of a parsed JSON document.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// `null`.
    Null,
    /// `true`.
    True,
    /// `false`.
    False,
    /// An integer in RFC-8259 spelling, stored as that spelling.
    Int(String),
    /// An integer only JSON5 accepts: hexadecimal, or with a leading `+`.
    Int5(String),
    /// A float in RFC-8259 spelling.
    Float(String),
    /// A float only JSON5 accepts: a bare leading or trailing point.
    Float5(String),
    /// String content that needs no escaping in either direction.
    Text(String),
    /// String content carrying JSON escapes, stored with the backslashes.
    TextJ(String),
    /// String content carrying escapes only JSON5 accepts.
    Text5(String),
    /// Raw text from SQL, which has to be escaped when it is rendered.
    TextRaw(String),
    /// An array.
    Array(Vec<Node>),
    /// An object, as label/value pairs. A label is always one of the text
    /// kinds; the parser refuses anything else.
    Object(Vec<(Node, Node)>),
}

impl Node {
    /// Returns the name `json_type()` gives this node.
    pub fn type_name(&self) -> &'static str {
        match self {
            Node::Null => "null",
            Node::True => "true",
            Node::False => "false",
            Node::Int(_) | Node::Int5(_) => "integer",
            Node::Float(_) | Node::Float5(_) => "real",
            Node::Text(_) | Node::TextJ(_) | Node::Text5(_) | Node::TextRaw(_) => "text",
            Node::Array(_) => "array",
            Node::Object(_) => "object",
        }
    }

    /// Returns whether the node is a text node of any spelling.
    pub fn is_text(&self) -> bool {
        matches!(
            self,
            Node::Text(_) | Node::TextJ(_) | Node::Text5(_) | Node::TextRaw(_)
        )
    }

    /// Returns whether the node holds children.
    pub fn is_container(&self) -> bool {
        matches!(self, Node::Array(_) | Node::Object(_))
    }

    /// Builds a text node for SQL text that is about to be rendered as JSON.
    ///
    /// `json_object`, `json_array` and the group aggregates escape what they
    /// are given on the way in, so the content they store is already JSON: it
    /// is `TEXT` when nothing needed escaping and `TEXTJ` when something did.
    pub fn text_escaped(content: &str) -> Node {
        if content.chars().all(needs_no_escape) {
            Node::Text(content.to_string())
        } else {
            Node::TextJ(escape(content))
        }
    }

    /// Builds a text node for SQL text stored into an existing document.
    ///
    /// `json_insert`, `json_set`, `json_replace` and `json_patch` store the
    /// bytes they were handed and mark them `TEXTRAW`, which is the format
    /// saying "this has not been escaped yet". The two spellings render
    /// identically; they differ in the blob, and the blob is what a cross-open
    /// fixture compares.
    pub fn text_raw(content: &str) -> Node {
        Node::TextRaw(content.to_string())
    }
}

/// The code point that stands for the byte 0x00 in text that is not valid UTF-8.
///
/// **SQLite copies bytes it cannot read as UTF-8 through a JSON document, and a `String` cannot
/// hold them.** `json_array(CAST(x'ff' AS TEXT))` answers `["` followed by the byte 0xff and
/// `"]`. Each such byte is held as one character of the last 128 code points of plane 16 (U+10FF80
/// to U+10FFFF, private use and two noncharacters) and turned back into the byte where text leaves
/// the JSON functions. The price is that a real character in that range, which no real text uses,
/// reads as a byte on its way out.
const RAW_BYTE_BASE: u32 = 0x10_FF00;

/// Returns the text of a byte string, holding each byte that is not valid UTF-8 as one character.
///
/// Valid UTF-8 is returned as it is, borrowed. See [`bytes_of_text`] for the way back.
///
/// @param bytes - the bytes of a SQL text value
pub fn text_of_bytes(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    let Err(first) = std::str::from_utf8(bytes) else {
        // `from_utf8` just succeeded, so the lossy conversion borrows and changes nothing.
        return String::from_utf8_lossy(bytes);
    };
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    let mut failure = Some(first);
    while let Some(error) = failure.take() {
        let (good, bad) = rest.split_at(error.valid_up_to());
        out.push_str(&String::from_utf8_lossy(good));
        let skipped = error.error_len().unwrap_or(bad.len());
        for byte in bad.iter().take(skipped) {
            out.push(char::from_u32(RAW_BYTE_BASE + u32::from(*byte)).unwrap_or('\u{fffd}'));
        }
        rest = bad.get(skipped..).unwrap_or(&[]);
        failure = std::str::from_utf8(rest).err();
    }
    out.push_str(&String::from_utf8_lossy(rest));
    std::borrow::Cow::Owned(out)
}

/// Returns the bytes of a text, turning each held byte back into itself.
///
/// @param text - text that may hold bytes put there by [`text_of_bytes`]
pub fn bytes_of_text(text: &str) -> std::borrow::Cow<'_, [u8]> {
    // The held bytes begin 0xf4 0x8f in UTF-8, so a text without that byte has none.
    if !text.as_bytes().contains(&0xf4) {
        return std::borrow::Cow::Borrowed(text.as_bytes());
    }
    let mut out = Vec::with_capacity(text.len());
    for character in text.chars() {
        let code = u32::from(character);
        match code.checked_sub(RAW_BYTE_BASE) {
            Some(byte) if (0x80..=0xff).contains(&byte) => out.push(byte as u8),
            _ => out.extend_from_slice(character.encode_utf8(&mut [0u8; 4]).as_bytes()),
        }
    }
    std::borrow::Cow::Owned(out)
}

/// Returns how many bytes a text takes once its held bytes are turned back.
///
/// @param text - the text
pub fn raw_len(text: &str) -> usize {
    if text.as_bytes().contains(&0xf4) {
        bytes_of_text(text).len()
    } else {
        text.len()
    }
}

/// Returns whether a character can sit in a JSON string with no escape.
fn needs_no_escape(character: char) -> bool {
    character != '"' && character != '\\' && character >= ' '
}

/// Escapes SQL text into the body of a JSON string.
///
/// One of three identical copies until task-1946's M4, and this was the one that
/// decided what `json_quote` and every other SQL JSON function produce. It is
/// `inillucent_base::json::escape_sql` now, which leaves DELETE raw as SQLite
/// does.
///
/// @param content - the text to escape
pub fn escape(content: &str) -> String {
    inillucent_base::json::escape_sql(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clean text needs no escaping and takes the smallest element type.
    #[test]
    fn clean_text_is_a_plain_text_node() {
        assert_eq!(Node::text_escaped("ab"), Node::Text("ab".to_string()));
    }

    /// A quote forces the escaped spelling, and the backslash is stored.
    #[test]
    fn a_quote_forces_the_escaped_spelling() {
        assert_eq!(
            Node::text_escaped("b\"c"),
            Node::TextJ("b\\\"c".to_string())
        );
    }

    /// A control character is escaped as its four-digit code point.
    #[test]
    fn control_characters_are_escaped_numerically() {
        assert_eq!(escape("a\u{1}b"), "a\\u0001b");
        assert_eq!(escape("a\nb"), "a\\nb");
    }

    /// The stored spelling of a value written by SQL is raw, whatever it holds.
    #[test]
    fn stored_sql_text_is_always_raw() {
        assert_eq!(Node::text_raw("v"), Node::TextRaw("v".to_string()));
    }
}

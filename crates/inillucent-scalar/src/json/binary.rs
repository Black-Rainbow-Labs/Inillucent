//! The JSONB binary format.
//!
//! Invariant: a blob this module writes is byte-identical to the one the pinned
//! release writes for the same input, and a blob it reads is understood the
//! same way. This is not an internal representation that happens to be
//! persisted - `jsonb()` hands the bytes to the application, applications store
//! them in columns, and a database written by one engine is opened by the
//! other. A format that were merely equivalent would fail the first cross-open
//! fixture.
//!
//! The encoding is one header per element. The header's low four bits are the
//! element type and its high four bits are the payload size when the size is
//! eleven or less; 12, 13, 14 and 15 mean the size follows as one, two, four or
//! eight big-endian bytes. Containers carry their children in their payload,
//! which is what makes the whole document one contiguous blob with no pointers
//! and no alignment.

use inillucent_base::DbResult;

use super::node::Node;

/// `null`.
pub const TYPE_NULL: u8 = 0;
/// `true`.
pub const TYPE_TRUE: u8 = 1;
/// `false`.
pub const TYPE_FALSE: u8 = 2;
/// An RFC-8259 integer, as its text.
pub const TYPE_INT: u8 = 3;
/// A JSON5 integer, as its text.
pub const TYPE_INT5: u8 = 4;
/// An RFC-8259 float, as its text.
pub const TYPE_FLOAT: u8 = 5;
/// A JSON5 float, as its text.
pub const TYPE_FLOAT5: u8 = 6;
/// String content that needs no translation in either direction.
pub const TYPE_TEXT: u8 = 7;
/// String content carrying JSON escapes.
pub const TYPE_TEXTJ: u8 = 8;
/// String content carrying JSON5 escapes.
pub const TYPE_TEXT5: u8 = 9;
/// String content that still has to be escaped to become JSON.
pub const TYPE_TEXTRAW: u8 = 10;
/// An array.
pub const TYPE_ARRAY: u8 = 11;
/// An object.
pub const TYPE_OBJECT: u8 = 12;

/// Appends one element to a blob.
pub fn encode(node: &Node, out: &mut Vec<u8>) {
    match node {
        Node::Null => header(TYPE_NULL, 0, out),
        Node::True => header(TYPE_TRUE, 0, out),
        Node::False => header(TYPE_FALSE, 0, out),
        Node::Int(text) => payload(TYPE_INT, text.as_bytes(), out),
        Node::Int5(text) => payload(TYPE_INT5, text.as_bytes(), out),
        Node::Float(text) => payload(TYPE_FLOAT, text.as_bytes(), out),
        Node::Float5(text) => payload(TYPE_FLOAT5, text.as_bytes(), out),
        Node::Text(text) => payload(TYPE_TEXT, &super::node::bytes_of_text(text), out),
        Node::TextJ(text) => payload(TYPE_TEXTJ, &super::node::bytes_of_text(text), out),
        Node::Text5(text) => payload(TYPE_TEXT5, &super::node::bytes_of_text(text), out),
        Node::TextRaw(text) => payload(TYPE_TEXTRAW, &super::node::bytes_of_text(text), out),
        Node::Array(items) => {
            let mut body = Vec::new();
            for item in items {
                encode(item, &mut body);
            }
            payload(TYPE_ARRAY, &body, out);
        }
        Node::Object(members) => {
            let mut body = Vec::new();
            for (label, value) in members {
                encode(label, &mut body);
                encode(value, &mut body);
            }
            payload(TYPE_OBJECT, &body, out);
        }
    }
}

/// Returns how many bytes one element takes in a blob, without building the blob.
///
/// **The same number `encode` writes, counted.** `json_each` and `json_tree` report each
/// element at the offset it would have in the document's blob, and found it by encoding the
/// element into a vector and measuring that, once for the element and again for every element
/// under it. For `json_each(payload, '$.tags')` over a document of three members that was about
/// seventy allocations per call, 1.9 us of the 2.8 us each output row cost.
///
/// @param node - the element
pub fn encoded_len(node: &Node) -> usize {
    match node {
        Node::Null | Node::True | Node::False => 1,
        Node::Int(text)
        | Node::Int5(text)
        | Node::Float(text)
        | Node::Float5(text)
        | Node::Text(text)
        | Node::TextJ(text)
        | Node::Text5(text)
        | Node::TextRaw(text) => {
            let length = super::node::raw_len(text);
            header_len(length).saturating_add(length)
        }
        Node::Array(items) => {
            let body: usize = items.iter().map(encoded_len).sum();
            header_len(body).saturating_add(body)
        }
        Node::Object(members) => {
            let body: usize = members
                .iter()
                .map(|(label, value)| encoded_len(label).saturating_add(encoded_len(value)))
                .sum();
            header_len(body).saturating_add(body)
        }
    }
}

/// Returns how many bytes the header of an element with this payload size takes.
///
/// The sizes `header` writes: one byte up to eleven, then one, two, four or eight bytes of size
/// after the first.
///
/// @param size - the payload size in bytes
pub fn header_len(size: usize) -> usize {
    if size <= 11 {
        1
    } else if size <= 0xff {
        2
    } else if size <= 0xffff {
        3
    } else if size <= 0xffff_ffff {
        5
    } else {
        9
    }
}

/// Returns the whole blob for one document.
pub fn to_blob(node: &Node) -> Vec<u8> {
    let mut out = Vec::new();
    encode(node, &mut out);
    out
}

/// Writes a header whose payload follows.
fn payload(kind: u8, body: &[u8], out: &mut Vec<u8>) {
    header(kind, body.len(), out);
    out.extend_from_slice(body);
}

/// Writes the smallest header that can describe a payload of this size.
fn header(kind: u8, size: usize, out: &mut Vec<u8>) {
    let kind = kind & 0x0f;
    if size <= 11 {
        out.push(((size as u8) << 4) | kind);
    } else if size <= 0xff {
        out.push(0xc0 | kind);
        out.push(size as u8);
    } else if size <= 0xffff {
        out.push(0xd0 | kind);
        out.extend_from_slice(&(size as u16).to_be_bytes());
    } else if size <= 0xffff_ffff {
        out.push(0xe0 | kind);
        out.extend_from_slice(&(size as u32).to_be_bytes());
    } else {
        out.push(0xf0 | kind);
        out.extend_from_slice(&(size as u64).to_be_bytes());
    }
}

/// One element's header, as read from a blob.
#[derive(Clone, Copy, Debug)]
pub struct ElementHeader {
    /// The element type in the low four bits of the first byte.
    pub kind: u8,
    /// How many bytes the header itself occupies.
    pub header_size: usize,
    /// How many payload bytes follow it.
    pub payload_size: usize,
}

impl ElementHeader {
    /// Returns the offset one past the whole element.
    pub fn end(&self, at: usize) -> usize {
        at.saturating_add(self.header_size)
            .saturating_add(self.payload_size)
    }
}

/// Reads one element header at an offset.
pub fn read_header(blob: &[u8], at: usize) -> DbResult<ElementHeader> {
    let first = *blob.get(at).ok_or_else(malformed)?;
    let kind = first & 0x0f;
    let marker = first >> 4;
    let (header_size, payload_size) = match marker {
        0..=11 => (1usize, usize::from(marker)),
        12 => (
            2usize,
            usize::from(*blob.get(at + 1).ok_or_else(malformed)?),
        ),
        13 => {
            // `try_from` on the slice rather than indexing it: `get` already
            // proved the length, and this is the form that says so to the
            // compiler as well as to a reader (task-1932, H9).
            let bytes = blob
                .get(at + 1..at + 3)
                .and_then(|head| <[u8; 2]>::try_from(head).ok())
                .ok_or_else(malformed)?;
            (3usize, usize::from(u16::from_be_bytes(bytes)))
        }
        14 => {
            let bytes = blob
                .get(at + 1..at + 5)
                .and_then(|head| <[u8; 4]>::try_from(head).ok())
                .ok_or_else(malformed)?;
            let size = u32::from_be_bytes(bytes);
            (5usize, size as usize)
        }
        15 => {
            let bytes = blob.get(at + 1..at + 9).ok_or_else(malformed)?;
            let mut size = [0u8; 8];
            size.copy_from_slice(bytes);
            let size = u64::from_be_bytes(size);
            if size > u64::from(u32::MAX) {
                return Err(malformed());
            }
            (9usize, size as usize)
        }
        _ => return Err(malformed()),
    };
    let header = ElementHeader {
        kind,
        header_size,
        payload_size,
    };
    if header.end(at) > blob.len() {
        return Err(malformed());
    }
    Ok(header)
}

/// Decodes one whole blob into a tree.
pub fn from_blob(blob: &[u8]) -> DbResult<Node> {
    let (node, end) = decode(blob, 0, 0)?;
    if end != blob.len() {
        return Err(malformed());
    }
    Ok(node)
}

/// Reports whether a blob's outer header says it could be JSONB.
///
/// This is SQLite's `jsonFuncArgMightBeBinary`, the test `json_quote` and the
/// functions that store a value into a document apply to a blob they were not
/// told is JSON. It reads only the first header: the type must be one of the
/// thirteen, the header and its payload must cover the blob exactly, and
/// `null`, `true` and `false` must have no payload. So `json_quote(x'00')` is
/// the document `null`, and `json_quote(x'ff')` is still the error a blob
/// with no JSON spelling gets.
///
/// @param blob - the blob
pub fn might_be_binary(blob: &[u8]) -> bool {
    let Some(first) = blob.first() else {
        return false;
    };
    if first & 0x0f > TYPE_OBJECT {
        return false;
    }
    let Ok(header) = read_header(blob, 0) else {
        return false;
    };
    if header.end(0) != blob.len() {
        return false;
    }
    !(header.kind <= TYPE_FALSE && header.payload_size > 0)
}

/// Reports whether a blob is a well-formed JSONB document, by SQLite's strict check.
///
/// **This is `jsonbValidityCheck`, and it is stricter than decoding.** A header that spends more
/// bytes than it needs on the size (`x'c000'` is a `null` with a two byte header), a number with
/// a character a number cannot hold, text with an unescaped quote and an object with an odd number
/// of members are all refused. Decoding accepts the first of these.
///
/// @param blob - the blob
pub fn is_valid(blob: &[u8]) -> bool {
    let Some((header, payload)) = sized(blob, 0) else {
        return false;
    };
    header.saturating_add(payload) == blob.len() && check(blob, 0, blob.len(), 1)
}

/// Reports whether a blob given to a JSON function is taken to be JSONB, as SQLite decides it.
///
/// This is `jsonArgIsJsonb`. The outer header must be one of the thirteen types, header and
/// payload must cover the blob exactly, `null`, `true` and `false` must have no payload, and a blob
/// of seven payload bytes or fewer that starts like JSON text (`{`, `[` or a digit) must also pass
/// the strict check, because it may be text that was cast to a blob.
///
/// @param blob - the blob
pub fn looks_like_jsonb(blob: &[u8]) -> bool {
    let Some(first) = blob.first().copied() else {
        return false;
    };
    let kind = first & 0x0f;
    let Some((header, payload)) = sized(blob, 0) else {
        return false;
    };
    kind <= TYPE_OBJECT
        && header.saturating_add(payload) == blob.len()
        && (kind > TYPE_FALSE || payload == 0)
        && (payload > 7
            || (first != 0x7b && first != 0x5b && !first.is_ascii_digit())
            || is_valid(blob))
}

/// Returns the header size and the payload size of the element at an offset, or nothing when the
/// header is cut off, an eight byte size is above four bytes, or the payload runs past the blob.
///
/// This is `jsonbPayloadSize`.
///
/// @param blob - the blob
/// @param at - where the element begins
fn sized(blob: &[u8], at: usize) -> Option<(usize, usize)> {
    let first = *blob.get(at)?;
    let marker = usize::from(first >> 4);
    let byte = |offset: usize| {
        blob.get(at.checked_add(offset)?)
            .map(|value| usize::from(*value))
    };
    let (header, payload) = match marker {
        0..=11 => (1, marker),
        12 => (2, byte(1)?),
        13 => (3, (byte(1)? << 8) + byte(2)?),
        14 => (
            5,
            (byte(1)? << 24) + (byte(2)? << 16) + (byte(3)? << 8) + byte(4)?,
        ),
        _ => {
            if byte(1)? != 0 || byte(2)? != 0 || byte(3)? != 0 || byte(4)? != 0 {
                return None;
            }
            (
                9,
                (byte(5)? << 24) + (byte(6)? << 16) + (byte(7)? << 8) + byte(8)?,
            )
        }
    };
    (at.checked_add(header)?.checked_add(payload)? <= blob.len()).then_some((header, payload))
}

/// Checks one element and everything inside it, the way `jsonbValidityCheck` does.
///
/// @param blob - the blob
/// @param at - where the element begins
/// @param end - one past the last byte the element may use
/// @param depth - how deep the element is nested
fn check(blob: &[u8], at: usize, end: usize, depth: usize) -> bool {
    if depth > 1000 {
        return false;
    }
    let Some((header, payload)) = sized(blob, at) else {
        return false;
    };
    if at + header + payload != end {
        return false;
    }
    let kind = blob.get(at).map_or(255, |first| first & 0x0f);
    let body = blob.get(at + header..end).unwrap_or(&[]);
    match kind {
        TYPE_NULL | TYPE_TRUE | TYPE_FALSE => header + payload == 1,
        TYPE_INT => integer_is_valid(body),
        TYPE_INT5 => hex_integer_is_valid(body),
        TYPE_FLOAT | TYPE_FLOAT5 => float_is_valid(body, kind == TYPE_FLOAT),
        TYPE_TEXT => body.iter().all(|byte| text_byte_is_plain(*byte)),
        TYPE_TEXTJ | TYPE_TEXT5 => escaped_text_is_valid(body, kind == TYPE_TEXT5),
        TYPE_TEXTRAW => true,
        TYPE_ARRAY => children_are_valid(blob, at + header, end, depth, false),
        TYPE_OBJECT => children_are_valid(blob, at + header, end, depth, true),
        _ => false,
    }
}

/// Checks the elements packed into a container's payload.
///
/// @param blob - the blob
/// @param from - where the first child begins
/// @param end - where the container ends
/// @param depth - the container's depth
/// @param object - whether the container is an object, whose odd children must be text labels
fn children_are_valid(blob: &[u8], from: usize, end: usize, depth: usize, object: bool) -> bool {
    let mut cursor = from;
    let mut count = 0usize;
    while cursor < end {
        let Some((header, payload)) = sized(blob, cursor) else {
            return false;
        };
        let next = cursor + header + payload;
        if next > end {
            return false;
        }
        if object && count % 2 == 0 {
            let kind = blob.get(cursor).map_or(255, |first| first & 0x0f);
            if !(TYPE_TEXT..=TYPE_TEXTRAW).contains(&kind) {
                return false;
            }
        }
        if !check(blob, cursor, next, depth + 1) {
            return false;
        }
        count += 1;
        cursor = next;
    }
    !(object && count % 2 == 1)
}

/// Reports whether a byte may sit in a JSON string with no escape.
///
/// @param byte - the byte
fn text_byte_is_plain(byte: u8) -> bool {
    (byte >= 0x20 && byte != b'"' && byte != b'\\') || byte == b'\''
}

/// Checks the payload of an integer element: an optional minus and digits.
///
/// @param body - the payload
fn integer_is_valid(body: &[u8]) -> bool {
    let digits = body.strip_prefix(b"-").unwrap_or(body);
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}

/// Checks the payload of a JSON5 integer element: an optional minus, `0x` and hex digits.
///
/// @param body - the payload
fn hex_integer_is_valid(body: &[u8]) -> bool {
    let unsigned = body.strip_prefix(b"-").unwrap_or(body);
    match unsigned {
        [b'0', b'x' | b'X', digits @ ..] => {
            !digits.is_empty() && digits.iter().all(u8::is_ascii_hexdigit)
        }
        _ => false,
    }
}

/// Checks the payload of a float element.
///
/// @param body - the payload
/// @param strict - true for the RFC-8259 type, which allows no leading or trailing point
fn float_is_valid(body: &[u8], strict: bool) -> bool {
    let size = body.len();
    if size < 2 {
        return false;
    }
    let mut at = 0usize;
    if body.first() == Some(&b'-') {
        if size < 3 {
            return false;
        }
        at = 1;
    }
    let mut seen = 0u8;
    if body.get(at) == Some(&b'.') {
        if strict || !body.get(at + 1).is_some_and(u8::is_ascii_digit) {
            return false;
        }
        at += 2;
        seen = 1;
    } else if body.get(at) == Some(&b'0') && strict {
        if at + 3 > size {
            return false;
        }
        if !matches!(body.get(at + 1), Some(b'.' | b'e' | b'E')) {
            return false;
        }
        at += 1;
    }
    while at < size {
        let byte = body.get(at).copied().unwrap_or(0);
        if byte.is_ascii_digit() {
            at += 1;
            continue;
        }
        if byte == b'.' {
            if seen > 0 {
                return false;
            }
            if strict && (at + 1 == size || !body.get(at + 1).is_some_and(u8::is_ascii_digit)) {
                return false;
            }
            seen = 1;
            at += 1;
            continue;
        }
        if byte == b'e' || byte == b'E' {
            if seen == 2 || at + 1 == size {
                return false;
            }
            if matches!(body.get(at + 1), Some(b'+' | b'-')) {
                at += 1;
                if at + 1 == size {
                    return false;
                }
            }
            seen = 2;
            at += 1;
            continue;
        }
        return false;
    }
    seen != 0
}

/// Checks the payload of a text element whose escapes are kept.
///
/// @param body - the payload
/// @param json5 - true for the JSON5 type, which also allows control characters and its own escapes
fn escaped_text_is_valid(body: &[u8], json5: bool) -> bool {
    let size = body.len();
    let mut at = 0usize;
    while at < size {
        let byte = body.get(at).copied().unwrap_or(0);
        if !text_byte_is_plain(byte) {
            if byte == b'"' || byte <= 0x1f {
                if !json5 {
                    return false;
                }
            } else if byte != b'\\' || at + 1 >= size {
                return false;
            } else {
                let next = body.get(at + 1).copied().unwrap_or(0);
                if b"\"\\/bfnrt".contains(&next) {
                    at += 1;
                } else if next == b'u' {
                    let hex = body.get(at + 2..at + 6);
                    if at + 5 >= size
                        || !hex.is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit))
                    {
                        return false;
                    }
                    at += 1;
                } else if !json5 {
                    return false;
                } else {
                    let length = json5_escape_length(body.get(at..).unwrap_or(&[]));
                    if length == 0 {
                        return false;
                    }
                    at += length - 1;
                }
            }
        }
        at += 1;
    }
    true
}

/// Returns how many bytes a JSON5 only escape takes, or 0 when it is not one.
///
/// @param rest - the text starting at the backslash
fn json5_escape_length(rest: &[u8]) -> usize {
    match rest {
        [b'\\', b'\'' | b'v' | b'\n', ..] => 2,
        [b'\\', b'0', next, ..] if !next.is_ascii_digit() => 2,
        [b'\\', b'0'] => 2,
        [b'\\', b'\r', b'\n', ..] => 3,
        [b'\\', b'\r', ..] => 2,
        [b'\\', b'x', a, b, ..] if a.is_ascii_hexdigit() && b.is_ascii_hexdigit() => 4,
        [b'\\', 0xe2, 0x80, 0xa8 | 0xa9, ..] => 4,
        _ => 0,
    }
}

/// Decodes one element, returning it and the offset past it.
fn decode(blob: &[u8], at: usize, depth: usize) -> DbResult<(Node, usize)> {
    if depth > 1000 {
        return Err(malformed());
    }
    let header = read_header(blob, at)?;
    let body_at = at.saturating_add(header.header_size);
    let body_end = body_at.saturating_add(header.payload_size);
    let body = blob.get(body_at..body_end).ok_or_else(malformed)?;
    let node = match header.kind {
        TYPE_NULL if body.is_empty() => Node::Null,
        TYPE_TRUE if body.is_empty() => Node::True,
        TYPE_FALSE if body.is_empty() => Node::False,
        // A number with no digits is malformed, as SQLite's own decoder
        // says: `json_quote(x'03')` is "malformed JSON" there. Reading it as
        // an empty number made such a blob the empty document.
        TYPE_INT | TYPE_INT5 | TYPE_FLOAT | TYPE_FLOAT5 if body.is_empty() => {
            return Err(malformed())
        }
        TYPE_INT => Node::Int(text(body)?),
        TYPE_INT5 => Node::Int5(text(body)?),
        TYPE_FLOAT => Node::Float(text(body)?),
        TYPE_FLOAT5 => Node::Float5(text(body)?),
        TYPE_TEXT => Node::Text(text(body)?),
        TYPE_TEXTJ => Node::TextJ(text(body)?),
        TYPE_TEXT5 => Node::Text5(text(body)?),
        TYPE_TEXTRAW => Node::TextRaw(text(body)?),
        TYPE_ARRAY => {
            let mut items = Vec::new();
            let mut cursor = body_at;
            while cursor < body_end {
                let (item, next) = decode(blob, cursor, depth + 1)?;
                if next <= cursor || next > body_end {
                    return Err(malformed());
                }
                items.push(item);
                cursor = next;
            }
            Node::Array(items)
        }
        TYPE_OBJECT => {
            let mut members = Vec::new();
            let mut cursor = body_at;
            while cursor < body_end {
                let (label, after_label) = decode(blob, cursor, depth + 1)?;
                if !label.is_text() || after_label >= body_end {
                    return Err(malformed());
                }
                let (value, next) = decode(blob, after_label, depth + 1)?;
                if next <= cursor || next > body_end {
                    return Err(malformed());
                }
                members.push((label, value));
                cursor = next;
            }
            Node::Object(members)
        }
        _ => return Err(malformed()),
    };
    Ok((node, body_end))
}

/// Returns a payload as text, holding any byte that is not UTF-8 as the document tree does.
fn text(body: &[u8]) -> DbResult<String> {
    Ok(super::node::text_of_bytes(body).into_owned())
}

/// Returns the error every malformed document reports.
fn malformed() -> inillucent_base::DbError {
    inillucent_base::DbError::primary(inillucent_base::PrimaryCode::Error)
        .with_message("malformed JSON")
        .with_detail("malformed JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    /// Encodes a document from its text.
    fn blob(text: &str) -> Vec<u8> {
        to_blob(&parse::parse(text).expect("parses").node)
    }

    /// Renders bytes as upper-case hexadecimal, as `hex()` does.
    fn hex(bytes: &[u8]) -> String {
        bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>()
    }

    /// The encodings the pinned release produces for the simple values.
    ///
    /// These are transcribed from `hex(jsonb(...))` run against the pinned
    /// 3.53.4 shell, which is the only thing that makes them a parity claim
    /// rather than a restatement of this file.
    #[test]
    fn simple_values_match_the_pinned_release() {
        assert_eq!(hex(&blob("null")), "00");
        assert_eq!(hex(&blob("true")), "01");
        assert_eq!(hex(&blob("false")), "02");
        assert_eq!(hex(&blob("1")), "1331");
        assert_eq!(hex(&blob("1.5")), "35312E35");
        assert_eq!(hex(&blob("\"ab\"")), "276162");
        assert_eq!(hex(&blob("[1,2]")), "4B13311332");
        assert_eq!(hex(&blob("{\"a\":1}")), "4C17611331");
        assert_eq!(hex(&blob("[]")), "0B");
        assert_eq!(hex(&blob("{}")), "0C");
    }

    /// The JSON5 element types, and the two spellings that are converted.
    #[test]
    fn json5_values_match_the_pinned_release() {
        assert_eq!(
            hex(&blob("{a:1, b:0x10, c:.5, d:+Infinity, e:NaN, f:'sq'}")),
            "CC2017611331176244307831301763262E35176455396539393917650017662773\
             71"
            .replace([' ', '\n'], "")
        );
    }

    /// A payload longer than eleven bytes takes the one-byte size header.
    #[test]
    fn a_long_payload_takes_a_wider_header() {
        let long = "\"".to_string() + &"x".repeat(20) + "\"";
        let encoded = blob(&long);
        assert_eq!(encoded[0], 0xc7);
        assert_eq!(encoded[1], 20);
    }

    /// Every encoding round-trips through the decoder unchanged.
    #[test]
    fn encodings_round_trip() {
        for text in [
            "null",
            "[1,2,[3,{\"a\":null}]]",
            "{\"a\":\"b\\\"c\",\"d\":[true,false]}",
            "{a:0x10, b:.5}",
        ] {
            let node = parse::parse(text).expect("parses").node;
            let bytes = to_blob(&node);
            assert_eq!(from_blob(&bytes).expect("decodes"), node, "{text}");
        }
    }

    /// A truncated or nonsensical blob is refused rather than misread.
    #[test]
    fn malformed_blobs_are_refused() {
        assert!(from_blob(&[]).is_err());
        assert!(from_blob(&[0x4b, 0x13]).is_err());
        assert!(from_blob(&[0x0d]).is_err());
        assert!(from_blob(&[0x1c, 0x00]).is_err());
        assert!(from_blob(&[0x11]).is_err());
    }

    /// An object whose label is not text is not a document.
    #[test]
    fn an_object_label_must_be_text() {
        assert!(from_blob(&[0x4c, 0x13, 0x31, 0x13, 0x32]).is_err());
    }
}

//! Carrying bytes that are not UTF-8 through the shell's `&str` plumbing.
//!
//! Invariant: **`encode(decode(bytes))` is `bytes` for every byte string, and
//! text that is valid UTF-8 and holds no character of the reserved range is
//! left exactly as it is.**
//!
//! `sqlite3` is a C program: a line of input, a text value and a blob are all
//! byte strings, and it writes them back out byte for byte. The shell here is
//! built on `String` and `&str` from the line reader through the renderer to the
//! output, and `String::from_utf8_lossy` at each end turned a Latin 1 `0xE9`
//! into U+FFFD, or ended the input at the first line it could not decode. The
//! engine takes `&str` for SQL text, so the statement cannot hold a byte that is
//! not UTF-8 either.
//!
//! **Each byte that cannot be decoded becomes one character of plane 16** (the
//! last 128 code points of the private use plane, U+10FF80 to U+10FFFF) and is
//! turned back into its byte when the text is written out. The character
//! travels through the engine as ordinary text, so `SELECT 'caf\xE9'` prints the
//! byte `E9` it was given. What the engine reports for `length()` and `hex()` of
//! such a value is the length and hex of the four byte encoding of that
//! character, which is not what `sqlite3` reports for the one byte it was given.
//! No input produced by a real text editor reaches the reserved range.
//!
//! **A character of the reserved range is left alone until a stand in has been made.** The
//! engine can produce any character itself: `SELECT char(1114111)` is the valid character
//! U+10FFFF, and writing it as the byte `FF` is wrong. A stand in and that character are the same
//! text once they are inside the engine, so no rule can tell them apart. What can be told is
//! whether the shell has made any stand in at all: it makes one only for a byte that is not UTF-8,
//! and when it has not, every character of the range came from the engine and is written as it
//! is. A process that has read invalid bytes and also prints a character of the range cannot be
//! answered correctly.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether `decode` has made a stand in in this process.
static STAND_INS_MADE: AtomicBool = AtomicBool::new(false);

/// The code point that stands for the byte `0x00`; only 0x80 to 0xFF are used.
const BASE: u32 = 0x10_FF00;

/// Returns the character that stands for a byte that is not valid UTF-8.
///
/// @param byte - the byte, 0x80 or more
fn stand_in(byte: u8) -> char {
    char::from_u32(BASE + u32::from(byte)).unwrap_or(char::REPLACEMENT_CHARACTER)
}

/// Returns the byte a reserved character stands for, if it is one.
///
/// @param character - any character
fn stood_for(character: char) -> Option<u8> {
    let code = u32::from(character);
    if (BASE + 0x80..=BASE + 0xFF).contains(&code) {
        u8::try_from(code - BASE).ok()
    } else {
        None
    }
}

/// Turns bytes into text, giving each byte that is not UTF-8 its stand in.
///
/// Making a stand in is recorded, because `encode` leaves the reserved range alone until one has
/// been made.
///
/// @param bytes - a value, a line of input or a file
pub fn decode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut rest = bytes;
    loop {
        match std::str::from_utf8(rest) {
            Ok(valid) => {
                out.push_str(valid);
                return out;
            }
            Err(error) => {
                let (good, bad) = rest.split_at(error.valid_up_to());
                out.push_str(&String::from_utf8_lossy(good));
                // `error_len` is None for a sequence cut short by the end, which
                // is every remaining byte.
                let length = error.error_len().unwrap_or(bad.len()).max(1);
                let (invalid, after) = bad.split_at(length.min(bad.len()));
                STAND_INS_MADE.store(true, Ordering::Relaxed);
                out.extend(invalid.iter().map(|byte| stand_in(*byte)));
                rest = after;
            }
        }
    }
}

/// Turns text back into the bytes it was decoded from.
///
/// @param text - text that may hold stand ins
pub fn encode(text: &str) -> Cow<'_, [u8]> {
    encode_given(text, STAND_INS_MADE.load(Ordering::Relaxed))
}

/// Turns text back into bytes, reading the reserved range as stand ins only when any were made.
///
/// @param text - text that may hold stand ins
/// @param stand_ins_made - whether `decode` has made a stand in in this process
fn encode_given(text: &str, stand_ins_made: bool) -> Cow<'_, [u8]> {
    if !stand_ins_made || !text.chars().any(|character| stood_for(character).is_some()) {
        return Cow::Borrowed(text.as_bytes());
    }
    let mut out = Vec::with_capacity(text.len());
    for character in text.chars() {
        match stood_for(character) {
            Some(byte) => out.push(byte),
            None => {
                let mut buffer = [0u8; 4];
                out.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    Cow::Owned(out)
}

/// Splits bytes into lines the way a line reader does, dropping a byte order
/// mark at the very start.
///
/// A line ends at `\n`, and a `\r` before it goes with it. The last line needs no
/// ending. **The mark is dropped from the first line only**: `sqlite3` skips it
/// at the start of its input, and a file written by Notepad starts with one.
///
/// @param bytes - a file, or all of standard input
pub fn split_lines(bytes: &[u8]) -> Vec<String> {
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    let mut lines: Vec<String> = Vec::new();
    for piece in body.split(|byte| *byte == b'\n') {
        let line = piece.strip_suffix(b"\r").unwrap_or(piece);
        lines.push(decode(line));
    }
    // `split` yields an empty last piece after a final newline, which is no line.
    if body.last() == Some(&b'\n') || body.is_empty() {
        lines.pop();
    }
    lines
}

/// Reads lines from a stream one at a time, as [`split_lines`] would cut them.
///
/// **One line at a time, because standard input is a conversation.** A program
/// that writes a statement and waits for its answer needs the shell to run the
/// statement before it reads again. A read of the whole stream would wait for an
/// end of input that never comes.
pub struct LineReader<R: std::io::BufRead> {
    /// Where the lines come from.
    reader: R,
    /// Whether the first line has been read, which is the only one that can begin
    /// with a byte order mark.
    started: bool,
}

impl<R: std::io::BufRead> LineReader<R> {
    /// Wraps a reader.
    ///
    /// @param reader - standard input or a file
    pub fn new(reader: R) -> LineReader<R> {
        LineReader {
            reader,
            started: false,
        }
    }
}

impl<R: std::io::BufRead> Iterator for LineReader<R> {
    type Item = String;

    /// Returns the next line, or none at the end of input or on a read failure.
    fn next(&mut self) -> Option<String> {
        let mut buffer: Vec<u8> = Vec::new();
        let read = self.reader.read_until(b'\n', &mut buffer).ok()?;
        if read == 0 {
            return None;
        }
        if buffer.last() == Some(&b'\n') {
            buffer.pop();
        }
        if buffer.last() == Some(&b'\r') {
            buffer.pop();
        }
        let mut line = buffer.as_slice();
        if !self.started {
            self.started = true;
            line = line.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(line);
        }
        Some(decode(line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every byte string comes back as it went in.
    #[test]
    fn bytes_round_trip() {
        let all: Vec<u8> = (0..=255u8).collect();
        assert_eq!(encode(&decode(&all)).as_ref(), all.as_slice());
        let mixed = b"caf\xE9 \xF0\x9F\x98\x80 \xFF\xFE end\xE2\x82";
        assert_eq!(encode(&decode(mixed)).as_ref(), mixed.as_slice());
    }

    /// A character of the reserved range the engine made is written as itself.
    ///
    /// `SELECT char(1114111)` is U+10FFFF, four bytes, and was written as the single byte `FF`.
    #[test]
    fn the_engines_own_reserved_character_is_written_whole() {
        let text = "\u{10FFFF}|\u{10FFFE}|\u{10FF80}";
        assert_eq!(encode_given(text, false).as_ref(), text.as_bytes());
        assert_eq!(encode_given("\u{10FFFF}", true).as_ref(), b"\xFF");
    }

    /// Valid UTF-8 is not touched.
    #[test]
    fn valid_text_is_unchanged() {
        assert_eq!(
            decode("caf\u{e9} \u{1F600}".as_bytes()),
            "caf\u{e9} \u{1F600}"
        );
        assert!(matches!(encode("plain"), Cow::Borrowed(_)));
    }

    /// A line reader drops one carriage return, the mark, and keeps the rest.
    #[test]
    fn lines_are_cut_like_a_line_reader() {
        let input = b"\xEF\xBB\xBFselect 1;\r\nselect '\xE9';\n\nlast";
        let lines = split_lines(input);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], "select 1;");
        assert_eq!(encode(&lines[1]).as_ref(), b"select '\xE9';");
        assert_eq!(lines[3], "last");
        let streamed: Vec<String> = LineReader::new(&input[..]).collect();
        assert_eq!(streamed, lines);
    }
}

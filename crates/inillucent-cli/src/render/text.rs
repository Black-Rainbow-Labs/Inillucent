//! Measuring, wrapping and escaping the text of a cell.
//!
//! Invariant: **a width is the number of terminal columns a text takes, and a
//! wrap never loses a character other than the white space it breaks at.** A
//! Japanese character is two columns and a combining accent is none, so a table
//! padded by counting characters was drawn crooked for any text that was not
//! ASCII. The reference measures columns, and so does this.

use super::Escape;

/// How many terminal columns a character takes.
///
/// Zero for the marks that combine with the character before them and for the
/// zero width controls, two for the East Asian wide and full width blocks and
/// the emoji planes, one for everything else.
///
/// @param character - the character
pub fn column_width(character: char) -> usize {
    match u32::from(character) {
        0x0300..=0x036F | 0x200B..=0x200F | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F => {
            0
        }
        0x1100..=0x115F
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7A3
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE6F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x1F300..=0x1F64F
        | 0x1F900..=0x1F9FF
        | 0x20000..=0x3FFFD => 2,
        _ => 1,
    }
}

/// How many terminal columns a text takes.
///
/// @param text - the text
pub fn text_width(text: &str) -> usize {
    text.chars().map(column_width).sum()
}

/// Replaces each tab with the spaces that reach the next multiple of eight.
///
/// @param line - one line of a cell
pub fn expand_tabs(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut column = 0usize;
    for character in line.chars() {
        if character == '\t' {
            let spaces = 8 - (column % 8);
            out.push_str(&" ".repeat(spaces));
            column += spaces;
        } else {
            out.push(character);
            column += column_width(character);
        }
    }
    out
}

/// Splits a cell into the lines a drawn table prints it on.
///
/// **A value with a newline is several lines of the table.** Printing it whole
/// put the newline inside the row, so the rest of the row started at the left
/// edge and the column rules no longer lined up. A carriage return before a
/// newline is part of the line ending. A tab expands to the next multiple of
/// eight columns, counted from the start of the line, because a tab would move
/// the closing rule by an amount nothing here can know.
///
/// @param text - the cell's text
pub fn cell_lines(text: &str) -> Vec<String> {
    text.split('\n')
        .map(|line| expand_tabs(line.strip_suffix('\r').unwrap_or(line)))
        .collect()
}

/// Reports whether a character counts as part of a word when choosing where to
/// break a line.
///
/// @param character - the character
fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Reports whether a line may be broken between two characters.
///
/// After white space, and wherever a word begins or ends. A break between two
/// characters of one word is never chosen while another break is possible.
///
/// @param before - the character before the break
/// @param after - the character after it
fn may_break_between(before: char, after: char) -> bool {
    before.is_whitespace() || after.is_whitespace() || is_word(before) != is_word(after)
}

/// Breaks one line into lines no wider than `width` columns.
///
/// **The break goes at the last place a word starts or ends that is in the second
/// half of the line, and in the middle of a word only when there is no such
/// place.** The white space at a break is dropped. With `words` off every break
/// is in the middle of whatever is there. The rule was measured against the
/// reference: `with "quote", comma` at ten columns breaks after the opening
/// quote, and at four columns breaks inside `"quote"` because the only word
/// boundary is in the first quarter of the line.
///
/// @param line - the text, with no newline in it
/// @param width - the most columns a line may take; zero means no limit
/// @param words - whether to prefer a word boundary to the middle of a word
pub fn wrap_line(line: &str, width: usize, words: bool) -> Vec<String> {
    if width == 0 || text_width(line) <= width {
        return vec![line.to_string()];
    }
    let mut remaining: Vec<char> = line.chars().collect();
    let mut out: Vec<String> = Vec::new();
    while text_width(&remaining.iter().collect::<String>()) > width {
        let mut taken = 0usize;
        let mut columns = 0usize;
        while let Some(character) = remaining.get(taken) {
            let next = column_width(*character);
            if columns + next > width {
                break;
            }
            columns += next;
            taken += 1;
        }
        if taken == 0 {
            taken = 1;
        }
        let mut cut = taken;
        if words {
            cut = (1..=taken)
                .rev()
                .find(|at| {
                    let before = remaining.get(at - 1).copied().unwrap_or(' ');
                    let after = remaining.get(*at).copied().unwrap_or(' ');
                    let head: String = remaining.iter().take(*at).collect();
                    may_break_between(before, after) && text_width(&head) * 2 >= width
                })
                .unwrap_or(taken);
        }
        let rest = remaining.split_off(cut);
        let head: String = remaining.iter().collect();
        out.push(head.trim_end().to_string());
        remaining = rest;
        while remaining
            .first()
            .is_some_and(|character| character.is_whitespace())
        {
            remaining.remove(0);
        }
    }
    out.push(remaining.iter().collect());
    out
}

/// Makes the control characters of a text visible, or leaves them, as asked.
///
/// `auto` and `ascii` write a control character in caret notation (`^A`,
/// `^[`), because a shell that emitted raw control bytes could be made to
/// drive a terminal by the contents of a database. A newline and a tab are left
/// alone, since a multi-line or tabbed value is meant to look like one, and so
/// is a carriage return that begins a line ending, which the reference leaves in
/// a Windows text. DELETE is written raw too. `symbol` writes the picture
/// character of the control (U+2401 for `^A`), and `off` writes the byte.
///
/// @param text - the text, with its quotes already added where a mode adds them
/// @param escape - how to write a control character
pub fn escape_controls(text: &str, escape: Escape) -> String {
    if escape == Escape::Off {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    while let Some(chunk) = characters.next() {
        let code = chunk as u32;
        let line_ending = chunk == '\r' && characters.peek() == Some(&'\n');
        if chunk == '\n' || chunk == '\t' || line_ending {
            out.push(chunk);
        } else if code < 0x20 {
            match escape {
                Escape::Symbol => out.push(char::from_u32(0x2400 + code).unwrap_or('?')),
                _ => {
                    out.push('^');
                    out.push(char::from_u32(code + 0x40).unwrap_or('?'));
                }
            }
        } else {
            out.push(chunk);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wide characters take two columns and accents none.
    #[test]
    fn width_counts_columns() {
        assert_eq!(text_width("abc"), 3);
        assert_eq!(text_width("\u{65e5}\u{672c}\u{8a9e}"), 6);
        assert_eq!(text_width("e\u{301}"), 1);
    }

    /// A line breaks where the reference breaks it.
    #[test]
    fn wrapping_prefers_a_word_boundary_in_the_second_half() {
        assert_eq!(
            wrap_line("with \"quote\", comma", 10, true),
            vec!["with \"", "quote\",", "comma"]
        );
        assert_eq!(
            wrap_line("with \"quote\", comma", 4, true),
            vec!["with", "\"quo", "te\",", "comm", "a"]
        );
        assert_eq!(
            wrap_line("tab     tab <b>&amp;</b> \\ back", 10, true),
            vec!["tab", "tab <b>&", "amp;</b> \\", "back"]
        );
        assert_eq!(
            wrap_line("12345678901234", 10, true),
            vec!["1234567890", "1234"]
        );
        assert_eq!(
            wrap_line("with quote, comma that is long", 10, false),
            vec!["with quote", ", comma th", "at is long"]
        );
    }

    /// Control characters are written the way the escape setting says.
    #[test]
    fn controls_are_escaped_as_asked() {
        assert_eq!(escape_controls("\u{1}\u{2}", Escape::Auto), "^A^B");
        assert_eq!(escape_controls("\u{1b}", Escape::Symbol), "\u{241b}");
        assert_eq!(escape_controls("\u{1}", Escape::Off), "\u{1}");
        assert_eq!(
            escape_controls("a\tb\nc\r\nd\u{7f}", Escape::Auto),
            "a\tb\nc\r\nd\u{7f}"
        );
        assert_eq!(escape_controls("\rr", Escape::Auto), "^Mr");
    }
}

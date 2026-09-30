//! `chunk_text`, the table function that cuts a document into windows for embedding.
//!
//! Invariant: the windows cover the whole text, in order, with no gap. Window number `n + 1` starts
//! at or before the character where window number `n` ended, so joining the windows and dropping
//! each overlap gives back the original text. Every window advances by at least one character, so
//! the function always ends, and no window splits a UTF-8 character because every position is a
//! character count and never a byte offset.
//!
//! ```sql
//! SELECT d.id, c.seq, c.chunk
//! FROM doc AS d, chunk_text(d.body, 900, 100, 'Subject: ' || d.subject) AS c;
//! ```
//!
//! | argument | meaning | default |
//! |---|---|---|
//! | `text` | the document | required |
//! | `size` | the largest window, in characters, not counting the heading | 900 |
//! | `overlap` | characters repeated from the end of one window at the start of the next | 0 |
//! | `heading` | text written, then a blank line, at the start of every chunk. `NULL` writes none | `NULL` |
//!
//! The output columns are `seq`, `chunk` (the heading, a blank line and the window), `start` (the
//! offset of the window in `text`, counted in characters from 0, so the window is
//! `substr(text, start + 1, length)`) and `length` (characters in the window).

use inillucent_base::error::unmet_requirement;
use inillucent_base::DbResult;
use inillucent_value::{cast, Value};

use super::{
    ConstraintOp, Context, Declaration, DeclaredColumn, FilterPlan, IndexQuery, Module,
    ModuleArguments, VirtualCursor, VirtualTable,
};

/// The visible column holding the position of the chunk among the chunks of one text.
const SEQ: usize = 0;
/// The visible column holding the heading, a blank line and the window.
const CHUNK: usize = 1;
/// The visible column holding the character offset of the window in the text.
const START: usize = 2;
/// The visible column holding the number of characters in the window.
const LENGTH: usize = 3;
/// The hidden column holding the document.
const TEXT: usize = 4;
/// The hidden column holding the largest window.
const SIZE: usize = 5;
/// The hidden column holding the overlap.
const OVERLAP: usize = 6;
/// The hidden column holding the heading.
const HEADING: usize = 7;

/// The window size when the caller names none.
pub const DEFAULT_SIZE: i64 = 900;

/// The share of a window, counted from its end, in which a paragraph break or a sentence end is
/// taken as the place to stop.
///
/// A break earlier than this would leave a short window, and a short window embeds as a weak
/// vector. The study that motivated this function cut at the last break in the final 30%.
const TAIL_PERCENT: usize = 30;

/// The `chunk_text` module.
pub struct ChunkTextModule;

impl Module for ChunkTextModule {
    /// Returns the module's name.
    fn name(&self) -> &str {
        "chunk_text"
    }

    /// It is a name rather than a table, so it needs no `CREATE`.
    fn eponymous(&self) -> bool {
        true
    }

    /// A `CREATE VIRTUAL TABLE` naming it would have no rows of its own, so it is refused.
    fn constructible(&self) -> bool {
        false
    }

    /// Connects, which is only declaring the eight columns.
    fn connect(
        &self,
        _arguments: &ModuleArguments,
        _creating: bool,
    ) -> DbResult<Box<dyn VirtualTable>> {
        Ok(Box::new(ChunkTextTable {
            declaration: Declaration {
                columns: vec![
                    DeclaredColumn::visible("seq").typed("INTEGER"),
                    DeclaredColumn::visible("chunk").typed("TEXT"),
                    DeclaredColumn::visible("start").typed("INTEGER"),
                    DeclaredColumn::visible("length").typed("INTEGER"),
                    DeclaredColumn::hidden("text").typed("TEXT"),
                    DeclaredColumn::hidden("size").typed("INTEGER"),
                    DeclaredColumn::hidden("overlap").typed("INTEGER"),
                    DeclaredColumn::hidden("heading").typed("TEXT"),
                ],
                without_rowid: false,
            },
        }))
    }
}

/// One connected `chunk_text`.
struct ChunkTextTable {
    declaration: Declaration,
}

impl VirtualTable for ChunkTextTable {
    /// Returns the eight columns.
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// Claims the four hidden columns, in column order, as arguments.
    ///
    /// The plan number has one bit per argument that was given. `filter` receives the arguments in
    /// the order they were claimed, and this claims them in column order, so a bit set for a
    /// column means the next argument belongs to it. A plan with no `text` is priced out of
    /// existence: `SELECT * FROM chunk_text` prepares and produces no rows.
    fn best_index(&self, info: &mut IndexQuery) -> DbResult<()> {
        let mut plan = 0i32;
        for (bit, column) in [TEXT, SIZE, OVERLAP, HEADING].into_iter().enumerate() {
            let found = (0..info.constraints.len()).find(|index| {
                info.constraints.get(*index).is_some_and(|constraint| {
                    constraint.usable
                        && constraint.op == ConstraintOp::Eq
                        && constraint.column as usize == column
                })
            });
            if let Some(index) = found {
                info.use_constraint(index, true);
                plan |= 1 << bit;
            }
        }
        info.index_number = plan;
        info.estimated_cost = if plan & 1 != 0 { 10.0 } else { 1.0e99 };
        info.estimated_rows = 20;
        Ok(())
    }

    /// Opens a cursor.
    fn open(&self) -> DbResult<Box<dyn VirtualCursor>> {
        Ok(Box::new(ChunkTextCursor::default()))
    }
}

/// A cursor over the chunks of one text.
#[derive(Default)]
struct ChunkTextCursor {
    /// The text, as characters, so a window is a slice and never splits a character.
    characters: Vec<char>,
    /// The start and length of each window, in characters.
    windows: Vec<(usize, usize)>,
    /// The heading and the blank line after it, or an empty string when there is no heading.
    prefix: String,
    /// The window the cursor is on.
    at: usize,
}

impl VirtualCursor for ChunkTextCursor {
    /// Reads the four arguments, checks them, and cuts the text into windows.
    fn filter(&mut self, _context: &mut Context<'_>, plan: &FilterPlan) -> DbResult<()> {
        self.characters.clear();
        self.windows.clear();
        self.prefix.clear();
        self.at = 0;
        let arguments = arguments_of(plan);
        let Some(text) = arguments.text else {
            return Ok(());
        };
        let (size, overlap) = checked_sizes(arguments.size, arguments.overlap)?;
        self.characters = text.chars().collect();
        self.windows = windows_of(&self.characters, size, overlap);
        if let Some(heading) = arguments.heading.filter(|heading| !heading.is_empty()) {
            self.prefix = format!("{heading}\n\n");
        }
        Ok(())
    }

    /// Moves to the next window.
    fn next(&mut self, _context: &mut Context<'_>) -> DbResult<()> {
        self.at = self.at.saturating_add(1);
        Ok(())
    }

    /// Returns whether every window has been produced.
    fn eof(&self) -> bool {
        self.at >= self.windows.len()
    }

    /// Returns one column of the current window.
    fn column(&mut self, _context: &mut Context<'_>, index: usize) -> DbResult<Value<'static>> {
        let Some((start, length)) = self.windows.get(self.at).copied() else {
            return Ok(Value::Null);
        };
        match index {
            SEQ => Ok(Value::Integer(self.at as i64)),
            CHUNK => {
                let window: String = self
                    .characters
                    .get(start..start.saturating_add(length))
                    .unwrap_or(&[])
                    .iter()
                    .collect();
                Value::owned_text(format!("{}{window}", self.prefix).as_bytes())
            }
            START => Ok(Value::Integer(start as i64)),
            LENGTH => Ok(Value::Integer(length as i64)),
            _ => Ok(Value::Null),
        }
    }

    /// Returns the window's position, which is its rowid.
    fn rowid(&self) -> DbResult<i64> {
        Ok(self.at as i64)
    }
}

/// The arguments one call carried, each `None` when it was not given or was `NULL`.
struct Arguments {
    text: Option<String>,
    size: Option<i64>,
    overlap: Option<i64>,
    heading: Option<String>,
}

/// Reads the arguments out of a plan, in the order `best_index` claimed them.
///
/// @param plan - the plan the engine hands the cursor
fn arguments_of(plan: &FilterPlan) -> Arguments {
    let mut given = plan.arguments.iter();
    let mut take = |bit: i32| -> Option<&Value<'static>> {
        if plan.index_number & (1 << bit) == 0 {
            return None;
        }
        given.next().filter(|value| !value.is_null())
    };
    let text = take(0).map(text_of);
    let size = take(1).map(cast::integer_value);
    let overlap = take(2).map(cast::integer_value);
    let heading = take(3).map(text_of);
    Arguments {
        text,
        size,
        overlap,
        heading,
    }
}

/// Returns a value's text, the way SQL would read it as a string.
///
/// @param value - a text, blob, integer or real value
fn text_of(value: &Value<'static>) -> String {
    match value {
        Value::Text(text) => String::from_utf8_lossy(&text.utf8_bytes()).into_owned(),
        Value::Blob(blob) => String::from_utf8_lossy(blob.raw()).into_owned(),
        Value::Integer(number) => number.to_string(),
        Value::Real(number) => {
            String::from_utf8_lossy(&inillucent_value::numeric::real_to_text(*number)).into_owned()
        }
        Value::Null => String::new(),
    }
}

/// Applies the defaults and refuses a size or an overlap that cannot produce windows.
///
/// A refusal is `invalid_state` and names the argument, because the SQL is valid and the value is
/// not.
///
/// @param size - the `size` argument, when given
/// @param overlap - the `overlap` argument, when given
fn checked_sizes(size: Option<i64>, overlap: Option<i64>) -> DbResult<(usize, usize)> {
    let size = size.unwrap_or(DEFAULT_SIZE);
    let overlap = overlap.unwrap_or(0);
    if size < 1 {
        return Err(refused(format!(
            "chunk_text: size must be at least 1, not {size}. It is the largest window in characters"
        )));
    }
    if overlap < 0 {
        return Err(refused(format!(
            "chunk_text: overlap must be 0 or more, not {overlap}"
        )));
    }
    if overlap >= size {
        return Err(refused(format!(
            "chunk_text: overlap must be smaller than size, and {overlap} is not smaller than {size}. \
             With no overlap smaller than the size a window could never advance"
        )));
    }
    Ok((size as usize, overlap as usize))
}

/// Builds the refusal for a bad argument.
///
/// @param said - the sentence, which names the argument
fn refused(said: String) -> inillucent_base::DbError {
    unmet_requirement("a valid chunk_text argument", said)
}

/// Reports whether a character is whitespace for the purpose of choosing where a window ends.
///
/// @param character - the character to test
fn is_space(character: char) -> bool {
    character.is_whitespace()
}

/// Cuts a text into windows of at most `size` characters.
///
/// @param text - the text as characters
/// @param size - the largest window, at least 1
/// @param overlap - how many characters the next window repeats, less than `size`
/// @returns the start and length of each window, in characters
pub fn windows_of(text: &[char], size: usize, overlap: usize) -> Vec<(usize, usize)> {
    let mut windows = Vec::new();
    let mut start = 0usize;
    while start < text.len() {
        let end = window_end(text, start, size);
        windows.push((start, end.saturating_sub(start)));
        if end >= text.len() {
            break;
        }
        start = next_start(text, start, end, overlap);
    }
    windows
}

/// Chooses where the window that begins at `start` ends.
///
/// In order: the end of the text when it fits, the last paragraph break in the final 30% of the
/// window, the last sentence end or line break in the final 30%, the last whitespace, and last
/// the cut at `size`. The result is always greater than `start`.
///
/// @param text - the text as characters
/// @param start - where the window begins
/// @param size - the largest window
fn window_end(text: &[char], start: usize, size: usize) -> usize {
    let limit = start.saturating_add(size).min(text.len());
    if limit >= text.len() {
        return text.len();
    }
    let tail_from = start.saturating_add(size.saturating_sub(size * TAIL_PERCENT / 100));
    last_paragraph_break(text, tail_from, limit)
        .or_else(|| last_sentence_end(text, tail_from, limit))
        .or_else(|| last_whitespace(text, start, limit))
        .unwrap_or(limit)
}

/// Finds the last paragraph break that ends inside `from..limit`, and returns the position after it.
///
/// @param text - the text as characters
/// @param from - the earliest position a break may start at
/// @param limit - the window's end; the break must be complete before it
fn last_paragraph_break(text: &[char], from: usize, limit: usize) -> Option<usize> {
    (from..limit.saturating_sub(1))
        .rev()
        .find(|at| text.get(*at) == Some(&'\n') && text.get(at.saturating_add(1)) == Some(&'\n'))
        .map(|at| at.saturating_add(2))
}

/// Finds the last sentence end or line break inside `from..limit`, and returns the position after it.
///
/// A sentence end is `.`, `?` or `!` followed by a space. The space is part of the window.
///
/// @param text - the text as characters
/// @param from - the earliest position an end may be at
/// @param limit - the window's end
fn last_sentence_end(text: &[char], from: usize, limit: usize) -> Option<usize> {
    (from..limit).rev().find_map(|at| {
        let here = *text.get(at)?;
        if here == '\n' {
            return Some(at.saturating_add(1));
        }
        let next_is_space = text.get(at.saturating_add(1)) == Some(&' ');
        let ends = matches!(here, '.' | '?' | '!');
        (ends && next_is_space && at.saturating_add(2) <= limit).then(|| at.saturating_add(2))
    })
}

/// Finds the last whitespace character in the window after its first character, and returns the position after it.
///
/// @param text - the text as characters
/// @param start - where the window begins
/// @param limit - the window's end
fn last_whitespace(text: &[char], start: usize, limit: usize) -> Option<usize> {
    (start.saturating_add(1)..limit)
        .rev()
        .find(|at| text.get(*at).is_some_and(|character| is_space(*character)))
        .map(|at| at.saturating_add(1))
}

/// Chooses where the window after one that ended at `end` begins.
///
/// It is `overlap` characters before `end`, moved forward past the rest of a word when that lands
/// inside one, and never before `start + 1` or after `end`, so the windows advance and leave no gap.
///
/// @param text - the text as characters
/// @param start - where the window that just ended began
/// @param end - where it ended
/// @param overlap - how many characters to repeat
fn next_start(text: &[char], start: usize, end: usize, overlap: usize) -> usize {
    if overlap == 0 {
        return end;
    }
    let mut next = end.saturating_sub(overlap).max(start.saturating_add(1));
    let inside_word = |at: usize| {
        at > 0
            && text
                .get(at.saturating_sub(1))
                .is_some_and(|c| !is_space(*c))
            && text.get(at).is_some_and(|c| !is_space(*c))
    };
    if inside_word(next) {
        while next < end && text.get(next).is_some_and(|c| !is_space(*c)) {
            next = next.saturating_add(1);
        }
    }
    while next < end && text.get(next).is_some_and(|c| is_space(*c)) {
        next = next.saturating_add(1);
    }
    next.min(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the text of each window.
    fn cut(text: &str, size: usize, overlap: usize) -> Vec<String> {
        let characters: Vec<char> = text.chars().collect();
        windows_of(&characters, size, overlap)
            .into_iter()
            .map(|(start, length)| characters[start..start + length].iter().collect())
            .collect()
    }

    /// A text no longer than the size is one window.
    #[test]
    fn a_short_text_is_one_window() {
        assert_eq!(cut("hello world", 900, 0), vec!["hello world"]);
        assert_eq!(cut("", 900, 0), Vec::<String>::new());
    }

    /// A paragraph break in the last 30% ends the window, and the break stays in it.
    #[test]
    fn a_paragraph_break_ends_a_window() {
        let text = format!("{}\n\n{}", "a".repeat(8), "b".repeat(20));
        assert_eq!(cut(&text, 10, 0)[0], format!("{}\n\n", "a".repeat(8)));
    }

    /// A sentence end ends a window when there is no paragraph break.
    #[test]
    fn a_sentence_end_ends_a_window() {
        let text = "aaaaaaa. bbbbbbbbbbbb";
        assert_eq!(cut(text, 10, 0)[0], "aaaaaaa. ");
    }

    /// With no break and no sentence end the window ends at the last whitespace.
    #[test]
    fn the_last_whitespace_ends_a_window() {
        assert_eq!(cut("aa bb cc dd ee ff", 10, 0)[0], "aa bb cc ");
    }

    /// With no whitespace at all the window is cut at the size.
    #[test]
    fn a_word_longer_than_the_size_is_cut() {
        assert_eq!(cut("abcdefghijkl", 5, 0), vec!["abcde", "fghij", "kl"]);
    }

    /// The next window repeats the overlap, moved forward so it starts at a word.
    #[test]
    fn the_overlap_starts_at_a_word() {
        let windows = cut("one two three four five six seven", 14, 6);
        assert!(windows.len() > 1);
        for window in &windows[1..] {
            assert!(!window.starts_with(char::is_whitespace), "{window:?}");
        }
        assert!(windows[1].starts_with("three") || windows[1].starts_with("four"));
    }

    /// Multibyte characters are counted as one and never split.
    #[test]
    fn multibyte_characters_are_never_split() {
        let text = "é😀é😀é😀é😀é😀é😀é😀";
        for window in cut(text, 5, 2) {
            assert!(window.chars().count() <= 5);
        }
        assert_eq!(cut(text, 5, 0).concat(), text);
    }

    /// Joining the windows and dropping each overlap gives back the text, for many sizes and overlaps.
    #[test]
    fn the_windows_tile_the_text() {
        let text =
            "The quick brown fox.  Jumps over\nthe lazy dog!\n\nA new paragraph starts here? \
                    Yes. It has several sentences. Some are short. Others run on and on without \
                    stopping for a good long while, so that a window has to end somewhere.";
        let characters: Vec<char> = text.chars().collect();
        for size in 1..60usize {
            for overlap in 0..size.min(20) {
                let windows = windows_of(&characters, size, overlap);
                let mut rebuilt = String::new();
                let mut covered = 0usize;
                for (start, length) in &windows {
                    assert!(*start <= covered, "gap at size {size} overlap {overlap}");
                    assert!(*length >= 1 && *length <= size);
                    rebuilt.extend(&characters[covered..start + length]);
                    covered = start + length;
                }
                assert_eq!(rebuilt, text, "size {size} overlap {overlap}");
                assert!(windows.windows(2).all(|pair| pair[1].0 > pair[0].0));
            }
        }
    }
}

//! The `trigram` tokenizer: every window of three characters is a token.
//!
//! Invariant: `spans` returns the tokens and byte offsets SQLite's `fts5TriTokenize` passes to its
//! callback. A text of fewer than three characters has no token. A character the fold turns into
//! zero (a combining mark when diacritics are removed) is skipped, and a token's end offset is the
//! start of the next character that was not skipped, so a token includes the marks that follow its
//! last character.

use super::super::fold::fold;

/// Returns the next character that is not folded away, with the byte offset it starts at.
///
/// @param characters - every character of the text with its byte offset
/// @param at - the index of the next character to read, advanced past the one returned
/// @param folding - whether characters are folded
/// @param fold_param - the `remove_diacritics` setting passed to the fold
fn next_character(
    characters: &[(usize, u32)],
    at: &mut usize,
    folding: bool,
    fold_param: u8,
) -> Option<(usize, u32)> {
    while let Some((offset, code)) = characters.get(*at).copied() {
        *at = at.saturating_add(1);
        let code = if folding {
            fold(code, fold_param)
        } else {
            code
        };
        if code != 0 {
            return Some((offset, code));
        }
    }
    None
}

/// Renders three code points as the token they make.
///
/// @param window - the code points of one trigram
fn token_of(window: &[u32; 3]) -> Vec<u8> {
    let mut token = String::new();
    for code in window {
        token.push(char::from_u32(*code).unwrap_or('\u{fffd}'));
    }
    token.into_bytes()
}

/// Returns the trigram tokens of a text with the byte range each one covers.
///
/// @param text - the text, as UTF-8
/// @param folding - whether characters are folded to lower case first
/// @param fold_param - the `remove_diacritics` setting passed to the fold, 0 or 2
pub(super) fn spans(text: &str, folding: bool, fold_param: u8) -> Vec<(Vec<u8>, usize, usize)> {
    let characters: Vec<(usize, u32)> = text
        .char_indices()
        .map(|(offset, character)| (offset, super::categories::decoded(u32::from(character))))
        .collect();
    let mut at = 0usize;
    let mut window = [0u32; 3];
    let mut starts = [0usize; 3];
    for slot in 0..3 {
        let Some((offset, code)) = next_character(&characters, &mut at, folding, fold_param) else {
            return Vec::new();
        };
        if let (Some(start), Some(held)) = (starts.get_mut(slot), window.get_mut(slot)) {
            *start = offset;
            *held = code;
        }
    }
    let mut found = Vec::new();
    loop {
        // The next character decides where this token ends, and whether there is another one.
        let (next_start, next) = match next_character(&characters, &mut at, folding, fold_param) {
            Some((offset, code)) => (offset, Some(code)),
            None => (text.len(), None),
        };
        let [first_start, second_start, third_start] = starts;
        let [first, second, third] = window;
        found.push((token_of(&window), first_start, next_start));
        let Some(code) = next else {
            return found;
        };
        let _ = (first, third_start);
        window = [second, third, code];
        starts = [second_start, third_start, next_start];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders spans as text for a comparison.
    fn shown(text: &str, folding: bool, param: u8) -> Vec<(String, usize, usize)> {
        spans(text, folding, param)
            .into_iter()
            .map(|(token, start, end)| (String::from_utf8_lossy(&token).into_owned(), start, end))
            .collect()
    }

    /// Fewer than three characters make no token and every later window makes one.
    #[test]
    fn a_text_makes_one_token_per_window_of_three() {
        assert!(shown("ab", true, 0).is_empty());
        assert_eq!(
            shown("abcd", true, 0),
            vec![("abc".to_string(), 0, 3), ("bcd".to_string(), 1, 4)]
        );
    }

    /// Folding lowers case and leaving it off keeps it.
    #[test]
    fn folding_is_optional() {
        assert_eq!(shown("ABC", true, 0), vec![("abc".to_string(), 0, 3)]);
        assert_eq!(shown("ABC", false, 0), vec![("ABC".to_string(), 0, 3)]);
    }
}

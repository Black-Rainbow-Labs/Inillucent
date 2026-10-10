//! Accent folding: a lowercase Latin letter with a mark on it becomes the plain
//! letter, so `notificação` and `notificacao` can meet.
//!
//! Invariant: **folding maps one character to one plain ASCII letter, or leaves it
//! alone.** A table of single characters cannot turn `ß` into `ss` by accident,
//! which keeps folding idempotent and keeps a word no longer than it was. The
//! letters left alone are the ones that are letters of their own rather than a
//! plain letter with a mark: `ß æ œ ð þ ı ĳ ĸ ŉ ŋ ſ`.
//!
//! The table covers the lowercase letters of Latin-1 Supplement and Latin
//! Extended-A, plus Romanian `ș` and `ț` from Latin Extended-B, which Romanian
//! writes in place of the cedilla forms `ş` and `ţ`. It is the single letter part
//! of PostgreSQL's `unaccent` rules for those blocks. It is a table searched by
//! binary search rather than a `match`, because rustfmt writes a `match` one arm
//! to a line and a function of ninety arms is over the length a new function may
//! have.

use std::borrow::Cow;

/// Every letter this module folds, and what it folds to, sorted by the letter.
///
/// `fold` searches it with `binary_search_by_key`, which finds nothing in a
/// table that is out of order. `the_table_is_sorted_with_no_repeats` checks the
/// order.
const FOLDS: &[(char, char)] = &[
    ('à', 'a'),
    ('á', 'a'),
    ('â', 'a'),
    ('ã', 'a'),
    ('ä', 'a'),
    ('å', 'a'),
    ('ç', 'c'),
    ('è', 'e'),
    ('é', 'e'),
    ('ê', 'e'),
    ('ë', 'e'),
    ('ì', 'i'),
    ('í', 'i'),
    ('î', 'i'),
    ('ï', 'i'),
    ('ñ', 'n'),
    ('ò', 'o'),
    ('ó', 'o'),
    ('ô', 'o'),
    ('õ', 'o'),
    ('ö', 'o'),
    ('ø', 'o'),
    ('ù', 'u'),
    ('ú', 'u'),
    ('û', 'u'),
    ('ü', 'u'),
    ('ý', 'y'),
    ('ÿ', 'y'),
    ('ā', 'a'),
    ('ă', 'a'),
    ('ą', 'a'),
    ('ć', 'c'),
    ('ĉ', 'c'),
    ('ċ', 'c'),
    ('č', 'c'),
    ('ď', 'd'),
    ('đ', 'd'),
    ('ē', 'e'),
    ('ĕ', 'e'),
    ('ė', 'e'),
    ('ę', 'e'),
    ('ě', 'e'),
    ('ĝ', 'g'),
    ('ğ', 'g'),
    ('ġ', 'g'),
    ('ģ', 'g'),
    ('ĥ', 'h'),
    ('ħ', 'h'),
    ('ĩ', 'i'),
    ('ī', 'i'),
    ('ĭ', 'i'),
    ('į', 'i'),
    ('ĵ', 'j'),
    ('ķ', 'k'),
    ('ĺ', 'l'),
    ('ļ', 'l'),
    ('ľ', 'l'),
    ('ŀ', 'l'),
    ('ł', 'l'),
    ('ń', 'n'),
    ('ņ', 'n'),
    ('ň', 'n'),
    ('ō', 'o'),
    ('ŏ', 'o'),
    ('ő', 'o'),
    ('ŕ', 'r'),
    ('ŗ', 'r'),
    ('ř', 'r'),
    ('ś', 's'),
    ('ŝ', 's'),
    ('ş', 's'),
    ('š', 's'),
    ('ţ', 't'),
    ('ť', 't'),
    ('ŧ', 't'),
    ('ũ', 'u'),
    ('ū', 'u'),
    ('ŭ', 'u'),
    ('ů', 'u'),
    ('ű', 'u'),
    ('ų', 'u'),
    ('ŵ', 'w'),
    ('ŷ', 'y'),
    ('ź', 'z'),
    ('ż', 'z'),
    ('ž', 'z'),
    ('ș', 's'),
    ('ț', 't'),
];

/// Returns the plain letter a marked letter folds to, or `None` when it is not
/// in the table.
///
/// @param letter - one character of lowercase text
fn folded(letter: char) -> Option<char> {
    let at = FOLDS
        .binary_search_by_key(&letter, |(from, _)| *from)
        .ok()?;
    FOLDS.get(at).map(|(_, to)| *to)
}

/// Returns the text with every marked letter replaced by its plain letter.
///
/// The text is borrowed back when nothing in it folds, which is every ASCII word
/// and every word in a script the table does not cover. Only a word that
/// changes pays for an allocation. The text is expected to be lowercase already:
/// the tokenizer lowercases before it folds, so the table holds no capitals.
///
/// @param text - lowercase text
pub fn fold(text: &str) -> Cow<'_, str> {
    if text.is_ascii() || !text.chars().any(|letter| folded(letter).is_some()) {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|letter| folded(letter).unwrap_or(letter))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every accented letter Portuguese writes folds to its plain letter.
    #[test]
    fn folding_removes_every_accent_portuguese_writes() {
        assert_eq!(fold("àáâãçéêíóôõúü"), "aaaaceeiooouu");
    }

    /// A letter of its own, a digit and another script are left as they are, and
    /// the text is borrowed rather than copied.
    #[test]
    fn folding_leaves_other_text_unchanged() {
        let text = "abc 123 ß æ œ ð þ ı 中文 ж";
        let folded = fold(text);
        assert!(matches!(folded, Cow::Borrowed(_)), "{folded:?} was copied");
        assert_eq!(folded, text);
    }

    /// The Latin Extended-A letters fold, and so do both Romanian spellings.
    #[test]
    fn folding_covers_latin_extended_a_and_romanian() {
        assert_eq!(fold("ăąćčęłńőśşșţțźżø"), "aaccelnosssttzzo");
    }

    /// Folding a second time changes nothing, because every letter the table
    /// produces is plain ASCII.
    #[test]
    fn folding_twice_is_folding_once() {
        for text in [
            "notificação",
            "àáâãçéêíóôõúü",
            "ăąćčęłńőśşșţțźżø",
            "straße",
            "plain",
        ] {
            let once = fold(text).into_owned();
            assert_eq!(fold(&once), once.as_str(), "{text}");
        }
        for (from, to) in FOLDS {
            assert!(to.is_ascii_lowercase(), "{from} folds to {to}");
        }
    }

    /// The table is strictly ascending, which is what `binary_search` needs. A
    /// row out of order would be silently missed, not refused.
    #[test]
    fn the_table_is_sorted_with_no_repeats() {
        for pair in FOLDS.windows(2) {
            let [(first, _), (second, _)] = pair else {
                continue;
            };
            assert!(first < second, "{first} is not before {second}");
        }
        assert_eq!(FOLDS.len(), 88);
    }
}

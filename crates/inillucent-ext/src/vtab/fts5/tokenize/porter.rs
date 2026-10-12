//! The Porter stemmer that SQLite's `porter` tokenizer runs.
//!
//! Invariant: **a token stems to the bytes SQLite stems it to.** A database
//! indexed by SQLite holds the stems in its index, so a query run here has to
//! produce the same stems or it finds nothing, or finds the wrong rows. The
//! rules are the original 1980 Porter algorithm as written in `fts5_tokenize.c`.
//! They are not Snowball English (Porter2), which stems `try`, `tries` and
//! `tried` to one word where SQLite keeps `tri` and `try` apart.
//!
//! Where SQLite's code and the paper differ, this follows the code. The
//! differences that show in a stem are: a token of one or two bytes, or of more
//! than 64 bytes, is not stemmed at all; the condition on a rule is checked
//! once, so a rule whose suffix matches but whose condition fails stops the step
//! instead of trying a shorter suffix; and a word that becomes empty cannot
//! happen because every rule needs a non empty stem.

/// The longest token the stemmer touches, in bytes.
///
/// SQLite passes a longer token through unchanged, which keeps a stem inside its
/// fixed buffer. A word that long is not English, so the stem would mean nothing
/// either way.
const MAX_TOKEN: usize = 64;

/// The condition a rule puts on the stem that is left when its suffix is cut.
#[derive(Clone, Copy)]
enum Condition {
    /// No condition.
    Always,
    /// The measure of the stem is above 0.
    MeasureAbove0,
    /// The measure of the stem is above 1.
    MeasureAbove1,
    /// The measure is above 1 and the stem ends in `s` or `t`.
    MeasureAbove1AfterSOrT,
}

/// One suffix rule: cut `suffix` when `condition` holds, and write `output`.
struct Rule {
    /// The suffix the word has to end in, with at least one byte before it.
    suffix: &'static [u8],
    /// What the stem has to satisfy.
    condition: Condition,
    /// What replaces the suffix.
    output: &'static [u8],
}

/// Builds a rule, so the tables below read as one line per rule.
const fn rule(suffix: &'static [u8], condition: Condition, output: &'static [u8]) -> Rule {
    Rule {
        suffix,
        condition,
        output,
    }
}

/// Step 2. Every rule needs a measure above 0.
const STEP_2: &[Rule] = &[
    rule(b"ational", Condition::MeasureAbove0, b"ate"),
    rule(b"tional", Condition::MeasureAbove0, b"tion"),
    rule(b"enci", Condition::MeasureAbove0, b"ence"),
    rule(b"anci", Condition::MeasureAbove0, b"ance"),
    rule(b"izer", Condition::MeasureAbove0, b"ize"),
    rule(b"logi", Condition::MeasureAbove0, b"log"),
    rule(b"bli", Condition::MeasureAbove0, b"ble"),
    rule(b"alli", Condition::MeasureAbove0, b"al"),
    rule(b"entli", Condition::MeasureAbove0, b"ent"),
    rule(b"eli", Condition::MeasureAbove0, b"e"),
    rule(b"ousli", Condition::MeasureAbove0, b"ous"),
    rule(b"ization", Condition::MeasureAbove0, b"ize"),
    rule(b"ation", Condition::MeasureAbove0, b"ate"),
    rule(b"ator", Condition::MeasureAbove0, b"ate"),
    rule(b"alism", Condition::MeasureAbove0, b"al"),
    rule(b"iveness", Condition::MeasureAbove0, b"ive"),
    rule(b"fulness", Condition::MeasureAbove0, b"ful"),
    rule(b"ousness", Condition::MeasureAbove0, b"ous"),
    rule(b"aliti", Condition::MeasureAbove0, b"al"),
    rule(b"iviti", Condition::MeasureAbove0, b"ive"),
    rule(b"biliti", Condition::MeasureAbove0, b"ble"),
];

/// Step 3. Every rule needs a measure above 0.
const STEP_3: &[Rule] = &[
    rule(b"ical", Condition::MeasureAbove0, b"ic"),
    rule(b"ness", Condition::MeasureAbove0, b""),
    rule(b"icate", Condition::MeasureAbove0, b"ic"),
    rule(b"iciti", Condition::MeasureAbove0, b"ic"),
    rule(b"ful", Condition::MeasureAbove0, b""),
    rule(b"ative", Condition::MeasureAbove0, b""),
    rule(b"alize", Condition::MeasureAbove0, b"al"),
];

/// Step 4. Every rule cuts its suffix and needs a measure above 1.
///
/// `ement` comes before `ment` and `ment` before `ent`, because SQLite tries
/// them in that order and the first suffix that matches decides.
const STEP_4: &[Rule] = &[
    rule(b"al", Condition::MeasureAbove1, b""),
    rule(b"ance", Condition::MeasureAbove1, b""),
    rule(b"ence", Condition::MeasureAbove1, b""),
    rule(b"er", Condition::MeasureAbove1, b""),
    rule(b"ic", Condition::MeasureAbove1, b""),
    rule(b"able", Condition::MeasureAbove1, b""),
    rule(b"ible", Condition::MeasureAbove1, b""),
    rule(b"ant", Condition::MeasureAbove1, b""),
    rule(b"ement", Condition::MeasureAbove1, b""),
    rule(b"ment", Condition::MeasureAbove1, b""),
    rule(b"ent", Condition::MeasureAbove1, b""),
    rule(b"ion", Condition::MeasureAbove1AfterSOrT, b""),
    rule(b"ou", Condition::MeasureAbove1, b""),
    rule(b"ism", Condition::MeasureAbove1, b""),
    rule(b"ate", Condition::MeasureAbove1, b""),
    rule(b"iti", Condition::MeasureAbove1, b""),
    rule(b"ous", Condition::MeasureAbove1, b""),
    rule(b"ive", Condition::MeasureAbove1, b""),
    rule(b"ize", Condition::MeasureAbove1, b""),
];

/// Step 1B part two: the rules that repair a stem after `ed` or `ing` was cut.
const STEP_1B_REPAIR: &[Rule] = &[
    rule(b"at", Condition::Always, b"ate"),
    rule(b"bl", Condition::Always, b"ble"),
    rule(b"iz", Condition::Always, b"ize"),
];

/// Returns the byte at a position, or zero when there is none.
///
/// SQLite reads one or two bytes before the start of its buffer for a very
/// short stem and compares what it finds, which no rule can match. Zero matches
/// no rule either, and keeps this code free of indexing that can panic.
///
/// @param bytes - the word
/// @param at - the position
fn byte_at(bytes: &[u8], at: usize) -> u8 {
    bytes.get(at).copied().unwrap_or(0)
}

/// Returns the byte a number of places back from the end, or zero.
///
/// @param bytes - the word
/// @param back - 1 is the last byte
fn from_end(bytes: &[u8], back: usize) -> u8 {
    match bytes.len().checked_sub(back) {
        Some(at) => byte_at(bytes, at),
        None => 0,
    }
}

/// Returns whether a byte is a vowel, where `y` counts after a consonant.
///
/// @param c - the byte
/// @param y_is_vowel - whether the byte before it was a consonant
fn is_vowel(c: u8, y_is_vowel: bool) -> bool {
    matches!(c, b'a' | b'e' | b'i' | b'o' | b'u') || (y_is_vowel && c == b'y')
}

/// Consumes one run of vowels and then consonants, returning how far it got.
///
/// Zero means the stem has no vowel followed by a consonant left in it. This is
/// SQLite's `fts5PorterGobbleVC`, and the measure of a stem is how many times it
/// can be called in a row.
///
/// @param stem - the bytes to scan
/// @param previous_was_consonant - whether the byte before the stem was one
fn gobble_vowel_consonant(stem: &[u8], previous_was_consonant: bool) -> usize {
    let mut consonant = previous_was_consonant;
    let mut at = 0usize;
    while at < stem.len() {
        consonant = !is_vowel(byte_at(stem, at), consonant);
        if !consonant {
            break;
        }
        at += 1;
    }
    at += 1;
    while at < stem.len() {
        consonant = !is_vowel(byte_at(stem, at), consonant);
        if consonant {
            return at + 1;
        }
        at += 1;
    }
    0
}

/// Returns the part of a stem after its first measure, for the second call.
///
/// @param stem - the stem
/// @param used - how many bytes the first measure consumed
fn after(stem: &[u8], used: usize) -> &[u8] {
    stem.get(used..).unwrap_or_default()
}

/// Returns whether the measure of the stem is above 1.
///
/// @param stem - the stem
fn measure_above_1(stem: &[u8]) -> bool {
    let used = gobble_vowel_consonant(stem, false);
    used > 0 && gobble_vowel_consonant(after(stem, used), true) > 0
}

/// Returns whether the measure of the stem is exactly 1.
///
/// @param stem - the stem
fn measure_is_1(stem: &[u8]) -> bool {
    let used = gobble_vowel_consonant(stem, false);
    used > 0 && gobble_vowel_consonant(after(stem, used), true) == 0
}

/// Returns whether the stem ends consonant, vowel, consonant with the last not
/// `w`, `x` or `y`. This is the paper's `*o` condition.
///
/// @param stem - the stem
fn ends_consonant_vowel_consonant(stem: &[u8]) -> bool {
    if stem.is_empty() || matches!(from_end(stem, 1), b'w' | b'x' | b'y') {
        return false;
    }
    let mut mask = 0u32;
    let mut consonant = false;
    for c in stem {
        consonant = !is_vowel(*c, consonant);
        mask = (mask << 1).wrapping_add(u32::from(consonant));
    }
    (mask & 0x7) == 0x5
}

/// Returns whether the stem holds a vowel, where `y` counts after a consonant.
///
/// @param stem - the stem
fn has_vowel(stem: &[u8]) -> bool {
    stem.iter().enumerate().any(|(at, c)| is_vowel(*c, at > 0))
}

/// Returns whether a stem satisfies a rule's condition.
///
/// @param condition - what the rule asks of the stem
/// @param stem - the bytes left when the suffix is cut
fn holds(condition: Condition, stem: &[u8]) -> bool {
    match condition {
        Condition::Always => true,
        Condition::MeasureAbove0 => gobble_vowel_consonant(stem, false) > 0,
        Condition::MeasureAbove1 => measure_above_1(stem),
        Condition::MeasureAbove1AfterSOrT => {
            matches!(from_end(stem, 1), b's' | b't') && measure_above_1(stem)
        }
    }
}

/// Applies the first rule whose suffix the word ends in.
///
/// **The first matching suffix decides, whether or not its condition holds.**
/// SQLite writes each step as a chain of `if` and `else if`, so a longer suffix
/// that matches and fails its condition does not fall through to a shorter one.
/// The word needs at least one byte before the suffix.
///
/// @param word - the word, changed in place
/// @param rules - the rules of one step, in SQLite's order
/// @returns whether a rule matched and its condition held
fn apply_first(word: &mut Vec<u8>, rules: &[Rule]) -> bool {
    for rule in rules {
        if word.len() <= rule.suffix.len() || !word.ends_with(rule.suffix) {
            continue;
        }
        let stem_len = word.len() - rule.suffix.len();
        let holds_here = holds(rule.condition, word.get(..stem_len).unwrap_or_default());
        if holds_here {
            word.truncate(stem_len);
            word.extend_from_slice(rule.output);
        }
        return holds_here;
    }
    false
}

/// Step 1A: plural endings, `sses` to `ss`, `ies` to `i`, `s` dropped.
///
/// @param word - the word, changed in place
fn step_1a(word: &mut Vec<u8>) {
    let length = word.len();
    if from_end(word, 1) != b's' {
        return;
    }
    if from_end(word, 2) == b'e' {
        let double_s = length > 4 && from_end(word, 4) == b's' && from_end(word, 3) == b's';
        let after_i = length > 3 && from_end(word, 3) == b'i';
        word.truncate(length - if double_s || after_i { 2 } else { 1 });
    } else if from_end(word, 2) != b's' {
        word.truncate(length - 1);
    }
}

/// Step 1B: `eed`, `ed` and `ing`.
///
/// @param word - the word, changed in place
/// @returns whether `ed` or `ing` was cut, which asks for the repair rules
fn step_1b(word: &mut Vec<u8>) -> bool {
    let length = word.len();
    if from_end(word, 2) == b'e' && length > 3 && word.ends_with(b"eed") {
        if gobble_vowel_consonant(word.get(..length - 3).unwrap_or_default(), false) > 0 {
            word.truncate(length - 1);
        }
        return false;
    }
    let suffix_len = if from_end(word, 2) == b'e' && length > 2 && word.ends_with(b"ed") {
        2
    } else if from_end(word, 2) == b'n' && length > 3 && word.ends_with(b"ing") {
        3
    } else {
        return false;
    };
    let stem = word.get(..length - suffix_len).unwrap_or_default();
    if !has_vowel(stem) {
        return false;
    }
    word.truncate(length - suffix_len);
    true
}

/// The rules after `ed` or `ing` was cut: restore an `e`, or drop a doubled
/// consonant.
///
/// @param word - the word, changed in place
fn repair_after_1b(word: &mut Vec<u8>) {
    if apply_first(word, STEP_1B_REPAIR) {
        return;
    }
    let last = from_end(word, 1);
    let doubled = !is_vowel(last, false)
        && last != b'l'
        && last != b's'
        && last != b'z'
        && last == from_end(word, 2);
    if doubled {
        word.pop();
    } else if measure_is_1(word) && ends_consonant_vowel_consonant(word) {
        word.push(b'e');
    }
}

/// Steps 5A and 5B: a final `e`, and a final double `l`.
///
/// @param word - the word, changed in place
fn step_5(word: &mut Vec<u8>) {
    if from_end(word, 1) == b'e' {
        let stem = word.get(..word.len() - 1).unwrap_or_default();
        if measure_above_1(stem) || (measure_is_1(stem) && !ends_consonant_vowel_consonant(stem)) {
            word.pop();
        }
    }
    let length = word.len();
    let stem = word.get(..length.saturating_sub(1)).unwrap_or_default();
    if length > 1 && from_end(word, 1) == b'l' && from_end(word, 2) == b'l' && measure_above_1(stem)
    {
        word.pop();
    }
}

/// Returns the stem SQLite's `porter` tokenizer gives a token.
///
/// A token of fewer than 3 bytes or more than 64 is returned as it came.
///
/// @param token - one token as the wrapped tokenizer produced it, already folded
pub(super) fn stem(token: &[u8]) -> Vec<u8> {
    if token.len() > MAX_TOKEN || token.len() < 3 {
        return token.to_vec();
    }
    let mut word = token.to_vec();
    step_1a(&mut word);
    if step_1b(&mut word) {
        repair_after_1b(&mut word);
    }
    // Step 1C: a final `y` becomes `i` when the rest holds a vowel.
    let length = word.len();
    if from_end(&word, 1) == b'y' && has_vowel(word.get(..length - 1).unwrap_or_default()) {
        if let Some(last) = word.last_mut() {
            *last = b'i';
        }
    }
    apply_first(&mut word, STEP_2);
    apply_first(&mut word, STEP_3);
    apply_first(&mut word, STEP_4);
    step_5(&mut word);
    word
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns the stem of a word as text.
    fn stemmed(word: &str) -> String {
        String::from_utf8_lossy(&stem(word.as_bytes())).into_owned()
    }

    /// The words the original algorithm is documented with.
    #[test]
    fn the_documented_examples_stem_as_published() {
        for (word, expected) in [
            ("caresses", "caress"),
            ("ponies", "poni"),
            ("cats", "cat"),
            ("feed", "feed"),
            ("agreed", "agre"),
            ("plastered", "plaster"),
            ("motoring", "motor"),
            ("hopping", "hop"),
            ("filing", "file"),
            ("happy", "happi"),
            ("relational", "relat"),
            ("conditional", "condit"),
            ("generalization", "gener"),
            ("probate", "probat"),
            ("controll", "control"),
        ] {
            assert_eq!(stemmed(word), expected, "the stem of {word}");
        }
    }

    /// The words that Snowball English stems together and SQLite keeps apart.
    #[test]
    fn the_words_snowball_joins_stay_apart() {
        assert_eq!(stemmed("try"), "try");
        assert_eq!(stemmed("tries"), "tri");
        assert_eq!(stemmed("trying"), "try");
        assert_eq!(stemmed("sky"), "sky");
        assert_eq!(stemmed("skies"), "ski");
        assert_eq!(stemmed("focus"), "focu");
        assert_eq!(stemmed("focuses"), "focus");
        assert_eq!(stemmed("releases"), "releas");
    }

    /// A token SQLite does not stem comes back as it was.
    #[test]
    fn a_short_or_long_token_is_left_alone() {
        assert_eq!(stemmed("is"), "is");
        assert_eq!(stemmed("as"), "as");
        let long = "s".repeat(65);
        assert_eq!(stemmed(&long), long);
    }
}

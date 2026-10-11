//! `LIKE` and `GLOB`.
//!
//! Invariant: both matchers are iterative with an explicit backtrack point, not
//! recursive. A pattern of a few hundred `%` characters against a long string
//! is an ordinary input, and a recursive matcher blows the stack on it; the
//! single-backtrack loop is linear in the common case and never grows a frame.
//!
//! `LIKE` is case-insensitive for ASCII and case-sensitive above it, which is
//! SQLite's documented behaviour and the reason its `LIKE` does not do what
//! most people expect on accented text. `GLOB` is case-sensitive everywhere and
//! has character classes; the two are genuinely different matchers and share
//! nothing but this module.

/// Matches a `LIKE` pattern against a subject.
///
/// `escape` is the bytes of the single character escape an `ESCAPE` clause named,
/// if any. An escaped `%`, `_` or escape character matches itself literally.
pub fn like(pattern: &[u8], subject: &[u8], escape: Option<&[u8]>) -> bool {
    matches(pattern, subject, escape, true, true)
}

/// Matches a `LIKE` pattern, saying whether ASCII case is folded.
///
/// **`PRAGMA case_sensitive_like = ON` turns the folding off**, which is the
/// whole content of that pragma: with it on, `'ABC' LIKE 'a%'` is 0. It was
/// accepted and dropped here, so the answer stayed 1 - a setting that changes
/// which rows a query returns, silently not applied.
///
/// @param pattern - the pattern
/// @param subject - the text being matched
/// @param escape - the `ESCAPE` character, when one was given
/// @param fold_case - whether ASCII letters match either case
pub fn like_folding(
    pattern: &[u8],
    subject: &[u8],
    escape: Option<&[u8]>,
    fold_case: bool,
) -> bool {
    matches(pattern, subject, escape, true, fold_case)
}

/// Matches a `GLOB` pattern against a subject.
pub fn glob(pattern: &[u8], subject: &[u8]) -> bool {
    // GLOB is case-sensitive whatever `case_sensitive_like` says; the pragma is
    // about LIKE alone.
    matches(pattern, subject, None, false, false)
}

/// The bytes before the first zero byte, or all of them when there is none.
///
/// @param bytes - a pattern or a subject
fn before_nul(bytes: &[u8]) -> &[u8] {
    match bytes.iter().position(|byte| *byte == 0) {
        Some(at) => bytes.get(..at).unwrap_or(bytes),
        None => bytes,
    }
}

/// The shared matcher, with one backtrack point.
fn matches(
    pattern: &[u8],
    subject: &[u8],
    escape: Option<&[u8]>,
    is_like: bool,
    fold_case: bool,
) -> bool {
    // **Both end at the first NUL, as SQLite's do.** SQLite matches the C
    // strings `sqlite3_value_text` returns, so a text value holding a zero
    // byte is matched only up to it: `('1' || x'00ff') LIKE '_'` is 1 and
    // `LIKE '___'` is 0, which is also what `length` says. This matcher read
    // every byte and answered the opposite of both. The nightly random matrix
    // found it on seed 20261002.
    let pattern = before_nul(pattern);
    let subject = before_nul(subject);
    let (mut any, mut one) = if is_like { (b'%', b'_') } else { (b'*', b'?') };
    // **A wildcard named as the escape character stops being a wildcard.**
    // SQLite clears `matchAll` or `matchOne` when `ESCAPE` names it, so with
    // `ESCAPE '%'` the pattern `'abc%%'` is the text `abc%` and not `abc`
    // followed by anything. The cleared byte is NUL, which `before_nul` has
    // already removed from the pattern, so it can never match again.
    if let Some([mark]) = escape {
        if *mark == any {
            any = 0;
        }
        if *mark == one {
            one = 0;
        }
    }
    let mut p = 0usize;
    let mut s = 0usize;
    let mut star_pattern: Option<usize> = None;
    let mut star_subject = 0usize;
    loop {
        let pattern_byte = pattern.get(p).copied();
        match pattern_byte {
            None => {
                if s >= subject.len() {
                    return true;
                }
            }
            Some(byte) if byte == any => {
                // Collapse a run of wildcards and remember where to come back
                // to if the rest of the pattern fails.
                star_pattern = Some(p);
                star_subject = s;
                p = p.saturating_add(1);
                continue;
            }
            Some(byte) if byte == one => {
                if let Some(width) = character_width(subject, s, is_like) {
                    p = p.saturating_add(1);
                    s = s.saturating_add(width);
                    continue;
                }
            }
            Some(b'[') if !is_like => {
                if let Some((width, length)) = class_match(pattern, p, subject, s) {
                    p = p.saturating_add(length);
                    s = s.saturating_add(width);
                    continue;
                }
            }
            Some(byte) => {
                // **The escape is a character, which may be more than one
                // byte** (task-2066 section 4.2, item 27). It used to be a
                // single `u8` taken from the first byte of whatever the caller
                // wrote, so an accented escape character escaped on 0xC3 - the
                // byte that begins half the accented characters there are -
                // and a pattern holding any of them lost a character.
                let mark = escape.filter(|mark| {
                    pattern
                        .get(p..)
                        .is_some_and(|rest| !mark.is_empty() && rest.starts_with(mark))
                });
                let skip = mark.map_or(0usize, <[u8]>::len);
                let literal = match mark {
                    Some(_) => pattern.get(p.saturating_add(skip)).copied(),
                    None => Some(byte),
                };
                let advance = skip.saturating_add(1);
                if let (Some(literal), Some(actual)) = (literal, subject.get(s).copied()) {
                    if literal == actual || (is_like && fold_case && folds_to(literal, actual)) {
                        p = p.saturating_add(advance);
                        s = s.saturating_add(1);
                        continue;
                    }
                }
            }
        }
        // The match failed here; go back to the last wildcard and let it eat
        // one more character.
        let Some(star) = star_pattern else {
            return false;
        };
        star_subject = star_subject.saturating_add(1);
        if star_subject > subject.len() {
            return false;
        }
        p = star.saturating_add(1);
        s = star_subject;
    }
}

/// Returns whether two bytes are the same character under LIKE's ASCII fold.
fn folds_to(left: u8, right: u8) -> bool {
    left.is_ascii_alphabetic() && right.is_ascii_alphabetic() && left.eq_ignore_ascii_case(&right)
}

/// Returns the byte width of the character at an offset.
///
/// `_` and `?` match one *character*, not one byte, so a multi-byte UTF-8
/// sequence has to be consumed whole or a pattern would match half of one.
fn character_width(subject: &[u8], offset: usize, _is_like: bool) -> Option<usize> {
    let first = subject.get(offset).copied()?;
    let width = if first < 0x80 {
        1
    } else if first >= 0xf0 {
        4
    } else if first >= 0xe0 {
        3
    } else if first >= 0xc0 {
        2
    } else {
        1
    };
    Some(width.min(subject.len().saturating_sub(offset)).max(1))
}

/// Reads one UTF-8 character the way SQLite's `sqlite3Utf8Read` does.
///
/// Returns the code point and the bytes it used. A surrogate and the two noncharacters U+FFFE and
/// U+FFFF read as U+FFFD, and a truncated sequence uses only the bytes present, so a malformed
/// subject still advances and a class compares the same code points SQLite compares.
///
/// @param bytes - the text being read
/// @param offset - where the character starts
fn read_character(bytes: &[u8], offset: usize) -> Option<(u32, usize)> {
    let first = u32::from(bytes.get(offset).copied()?);
    if first < 0xc0 {
        return Some((first, 1));
    }
    let mut code = if first < 0xe0 {
        first & 0x1f
    } else if first < 0xf0 {
        first & 0x0f
    } else if first < 0xf8 {
        first & 0x07
    } else {
        first & 0x03
    };
    let mut used = 1usize;
    while let Some(next) = bytes.get(offset.saturating_add(used)).copied() {
        if next & 0xc0 != 0x80 {
            break;
        }
        code = (code << 6) | u32::from(next & 0x3f);
        used = used.saturating_add(1);
    }
    if code < 0x80 || (code & 0xffff_f800) == 0xd800 || (code & 0xffff_fffe) == 0xfffe {
        code = 0xfffd;
    }
    Some((code, used))
}

/// Matches a `[...]` character class, returning the bytes consumed on each
/// side when it matches.
///
/// **The class compares characters, not bytes.** It compared single bytes, so a class holding
/// `é`, a range such as `à-ü`, or a negated class tested against a multi byte character never
/// matched: `'é' GLOB '[^a]'` was 0. The walk follows SQLite's `patternCompare`: a `^` directly
/// after the bracket negates, a `]` directly after the bracket or the `^` is a member, a `-` is a
/// range only when a previous member exists and the next character is not `]`, and a range cannot
/// start where another range ended.
///
/// @param pattern - the whole pattern
/// @param start - the index of the opening bracket
/// @param subject - the whole subject
/// @param offset - where the subject character starts
fn class_match(
    pattern: &[u8],
    start: usize,
    subject: &[u8],
    offset: usize,
) -> Option<(usize, usize)> {
    let (actual, width) = read_character(subject, offset)?;
    let mut index = start.saturating_add(1);
    let (mut member, used) = read_character(pattern, index)?;
    index = index.saturating_add(used);
    let negated = member == u32::from(b'^');
    if negated {
        let (next, used) = read_character(pattern, index)?;
        member = next;
        index = index.saturating_add(used);
    }
    let mut matched = false;
    if member == u32::from(b']') {
        matched = actual == u32::from(b']');
        let (next, used) = read_character(pattern, index)?;
        member = next;
        index = index.saturating_add(used);
    }
    let mut prior = 0u32;
    while member != u32::from(b']') {
        let upcoming = pattern.get(index).copied();
        if member == u32::from(b'-') && prior > 0 && upcoming.is_some_and(|byte| byte != b']') {
            let (high, used) = read_character(pattern, index)?;
            index = index.saturating_add(used);
            matched |= actual >= prior && actual <= high;
            prior = 0;
        } else {
            matched |= actual == member;
            prior = member;
        }
        let (next, used) = read_character(pattern, index)?;
        member = next;
        index = index.saturating_add(used);
    }
    if matched == negated {
        return None;
    }
    Some((width, index.saturating_sub(start)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wildcards, and the fact that `LIKE` is case-insensitive for ASCII.
    #[test]
    fn like_matches_the_documented_cases() {
        assert!(like(b"abc", b"abc", None));
        assert!(like(b"ABC", b"abc", None));
        assert!(like(b"a%", b"abcdef", None));
        assert!(like(b"%f", b"abcdef", None));
        assert!(like(b"a_c", b"abc", None));
        assert!(!like(b"a_c", b"abbc", None));
        assert!(like(b"%", b"", None));
        assert!(!like(b"a%", b"b", None));
    }

    /// An `ESCAPE` character makes a wildcard literal.
    #[test]
    fn like_honours_an_escape_character() {
        assert!(like(b"100\\%", b"100%", Some(b"\\".as_slice())));
        assert!(!like(b"100\\%", b"100x", Some(b"\\".as_slice())));
        assert!(like(b"a\\_c", b"a_c", Some(b"\\".as_slice())));
        assert!(!like(b"a\\_c", b"abc", Some(b"\\".as_slice())));
    }

    /// `GLOB` is case-sensitive and has its own wildcards and classes.
    #[test]
    fn glob_is_case_sensitive_and_has_classes() {
        assert!(glob(b"abc", b"abc"));
        assert!(!glob(b"ABC", b"abc"));
        assert!(glob(b"a*", b"abc"));
        assert!(glob(b"a?c", b"abc"));
        assert!(glob(b"[a-c]bc", b"abc"));
        assert!(!glob(b"[^a-c]bc", b"abc"));
        assert!(glob(b"[^d]bc", b"abc"));
    }

    /// A class compares characters, so a multi byte member, range or negation matches.
    #[test]
    fn a_glob_class_compares_characters_not_bytes() {
        assert!(glob("[é]".as_bytes(), "é".as_bytes()));
        assert!(glob("[^a]".as_bytes(), "é".as_bytes()));
        assert!(glob("[à-ü]".as_bytes(), "é".as_bytes()));
        assert!(glob("a[^b]b".as_bytes(), "a日b".as_bytes()));
        assert!(!glob("[^é]".as_bytes(), "é".as_bytes()));
        assert!(!glob("[a".as_bytes(), b"a"));
    }

    /// A pattern of many wildcards against a long subject must not recurse,
    /// and must still terminate. This is the input that kills a naive matcher.
    #[test]
    fn a_pathological_pattern_terminates() {
        let pattern = vec![b'%'; 400];
        let subject = vec![b'a'; 4000];
        assert!(like(&pattern, &subject, None));
        let mut mixed = Vec::new();
        for _ in 0..200 {
            mixed.push(b'%');
            mixed.push(b'a');
        }
        mixed.push(b'b');
        assert!(!like(&mixed, &subject, None));
    }

    /// `_` consumes one character, not one byte.
    #[test]
    fn one_character_wildcards_consume_a_whole_character() {
        assert!(like("_".as_bytes(), "é".as_bytes(), None));
        assert!(like("a_c".as_bytes(), "aéc".as_bytes(), None));
    }
}

//! Turning text into the tokens an index is built from.
//!
//! Invariant: the same text produces the same tokens on the way in and on the
//! way out. That sounds obvious and is the single most common way a full-text
//! index goes wrong: a document indexed with one rule and searched with another
//! finds nothing, and nothing about the failure says why. So indexing and
//! searching call this and only this.
//!
//! Two tokenizers, which are the two the pinned release has by default.
//! `ascii` treats every ASCII byte outside `A-Za-z0-9` as a separator, treats
//! every byte of 128 and above as part of a token, and folds `A-Z`; `unicode61` treats every character outside the Unicode letter and
//! number categories as a separator, folds case, and strips the combining marks
//! that make `é` and `e` the same word. `unicode61` is the default, as it is
//! there.

use inillucent_base::DbResult;

mod porter;

/// Which tokenizer an index uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tokenizer {
    /// `ascii`: ASCII bytes outside `A-Za-z0-9` separate, `A-Z` folds.
    ///
    /// Bytes of 128 and above are token characters and are not folded, which is
    /// how SQLite's `ascii` tokenizer treats them: `Caf\u{e9}` is one token.
    Ascii,
    /// `porter`: another tokenizer's tokens, each reduced to its stem.
    ///
    /// **A wrapper, which is what it is in SQLite too**: `tokenize='porter'`
    /// stems `unicode61`'s tokens and `tokenize='porter ascii'` stems `ascii`'s.
    /// It is what makes a search for `run` find `running`, and it was accepted
    /// and read as `unicode61` - a declaration the application trusts, doing
    /// nothing, which is the same shape `CHECK` and `STRICT` were in before
    /// each was enforced.
    ///
    /// The stemmer is the original 1980 Porter algorithm, ported from the code
    /// SQLite ships, so a token stems to the same bytes in both. Snowball
    /// English (Porter2) was used before, and it joined `try`, `tries` and
    /// `tried` where SQLite does not, so a database indexed by SQLite could not
    /// be searched here.
    Porter(Box<Tokenizer>),
    /// `unicode61`: Unicode letters and numbers are tokens.
    Unicode61 {
        /// What `remove_diacritics` asked for: 0 keeps diacritics, 1 removes them and 2 also
        /// removes the ranges SQLite only removes at that setting.
        remove_diacritics: u8,
        /// Characters that are part of a token although their category is not.
        extra_tokens: Vec<char>,
        /// Characters that separate although their category says otherwise.
        separators: Vec<char>,
    },
}

impl Tokenizer {
    /// Returns the tokenizer a specification names.
    ///
    /// **A name this build has not got is refused, and used to be read as
    /// `unicode61` (task-1932, M5).** The old comment called that "the safe
    /// direction: a schema naming a tokenizer this build has not got still
    /// opens, and its rows are still found by the words they contain". It is
    /// not the safe direction, because the tokenizer is what a query *means*.
    /// `tokenize='trigram'` in SQLite makes `MATCH` a substring search, and
    /// substituting `unicode61` made it whole-word search - so
    /// `CREATE VIRTUAL TABLE t USING fts5(body, tokenize='trigram')` succeeded
    /// and then answered a different question from the one it was asked, with
    /// no error anywhere. A refusal is a caller's problem to solve; a silently
    /// different search is a wrong answer nobody looks for.
    ///
    /// `trigram` itself is a feature to build rather than a name to add here.
    /// What this does is stop pretending it exists.
    ///
    /// @param specification - the `tokenize=` words, the name first
    pub fn named(specification: &[Vec<u8>]) -> DbResult<Tokenizer> {
        let name = specification
            .first()
            .map(|part| String::from_utf8_lossy(part).to_ascii_lowercase())
            .unwrap_or_default();
        if name == "porter" {
            // Whatever follows `porter` is the tokenizer it wraps, and nothing
            // following it means the default one.
            let inner = specification.get(1..).unwrap_or(&[]);
            // SQLite reports a wrapped tokenizer it cannot find with the wrapper's own words.
            let wrapped = Tokenizer::named(inner).map_err(|error| {
                if error.unsupported().is_some() {
                    return error;
                }
                let said = "error in tokenizer constructor".to_string();
                crate::vtab::failure(said.clone()).with_message(said)
            })?;
            return Ok(Tokenizer::Porter(Box::new(wrapped)));
        }
        if name == "ascii" {
            return Ok(Tokenizer::Ascii);
        }
        // An empty name is the default, which is `unicode61`; anything else has
        // to be a name this build implements.
        if !name.is_empty() && name != "unicode61" {
            // A name SQLite has never heard of is a mistake in the statement, which SQLite
            // reports as `no such tokenizer`. Only a name SQLite does have, and this build
            // does not, is `unsupported`.
            if name != "trigram" {
                let said = format!(
                    "no such tokenizer: {}",
                    String::from_utf8_lossy(
                        specification.first().map(Vec::as_slice).unwrap_or(&[])
                    )
                );
                return Err(crate::vtab::failure(said.clone()).with_message(said));
            }
            // **The same words the other fts5 refusals use** (bug hunt 10): the
            // construct, then `is not built`. This one named "the new engine's
            // physical pass", an internal stage a caller has no use for.
            let said = format!("the fts5 tokenizer {name}");
            return Err(
                inillucent_base::error::misuse(format!("{said} is not built"))
                    .with_message(format!("{said} is not built"))
                    .with_unsupported(said),
            );
        }
        let mut remove_diacritics = 1u8;
        let mut extra_tokens = Vec::new();
        let mut separators = Vec::new();
        let mut arguments = specification.iter().skip(1);
        while let Some(key) = arguments.next() {
            let key = String::from_utf8_lossy(key).to_ascii_lowercase();
            let Some(value) = arguments.next() else {
                break;
            };
            let value = String::from_utf8_lossy(value).into_owned();
            match key.as_str() {
                "remove_diacritics" => {
                    remove_diacritics = match value.trim() {
                        "0" => 0,
                        "2" => 2,
                        _ => 1,
                    }
                }
                "tokenchars" => extra_tokens.extend(value.chars()),
                "separators" => separators.extend(value.chars()),
                _ => {}
            }
        }
        Ok(Tokenizer::Unicode61 {
            remove_diacritics,
            extra_tokens,
            separators,
        })
    }

    /// Returns the tokens of one piece of text with where each one came from.
    ///
    /// **Byte offsets into the original text**, which is what `highlight()` and
    /// `snippet()` need and what `tokens` throws away: a token is folded, and
    /// case-folding, diacritic removal and stemming all change its length, so
    /// there is no way to find a token's bytes again by searching for it. The
    /// offsets are recorded while the characters are being read, which is the
    /// one place they are known.
    ///
    /// The Porter tokenizer stems, and a stem is not a substring of the word it
    /// came from - so its spans are the *unstemmed* tokenizer's spans and its
    /// tokens are the stems, which is the pairing a highlight wants: find the
    /// word by its span, decide whether it matched by its stem.
    ///
    /// @param text - the column's bytes
    pub fn spans(&self, text: &[u8]) -> Vec<(Vec<u8>, usize, usize)> {
        if let Tokenizer::Porter(inner) = self {
            return inner
                .spans(text)
                .into_iter()
                .map(|(token, start, end)| (porter::stem(&token), start, end))
                .collect();
        }
        let text = String::from_utf8_lossy(text);
        let mut spans = Vec::new();
        let mut current = String::new();
        let mut start = 0usize;
        let mut at = 0usize;
        for character in text.chars() {
            let width = character.len_utf8();
            if self.is_token_character(character, !current.is_empty()) {
                if current.is_empty() {
                    start = at;
                }
                self.fold_into(character, &mut current);
                at = at.saturating_add(width);
                continue;
            }
            if !current.is_empty() {
                spans.push((clipped(core::mem::take(&mut current)), start, at));
            }
            at = at.saturating_add(width);
        }
        if !current.is_empty() {
            spans.push((clipped(current), start, at));
        }
        spans
    }

    /// Returns the tokens of one piece of text, in the order they appear.
    pub fn tokens(&self, text: &[u8]) -> Vec<Vec<u8>> {
        if let Tokenizer::Porter(inner) = self {
            return inner
                .tokens(text)
                .into_iter()
                .map(|token| porter::stem(&token))
                .collect();
        }
        let text = String::from_utf8_lossy(text);
        let mut tokens = Vec::new();
        let mut current = String::new();
        for character in text.chars() {
            if self.is_token_character(character, !current.is_empty()) {
                self.fold_into(character, &mut current);
                continue;
            }
            if !current.is_empty() {
                tokens.push(clipped(core::mem::take(&mut current)));
            }
        }
        if !current.is_empty() {
            tokens.push(clipped(current));
        }
        tokens
    }

    /// Returns whether a character is part of a token.
    ///
    /// @param character - the character
    /// @param inside - whether a token is already being read, which is when a combining mark
    ///   belongs to it. A mark never starts a token.
    fn is_token_character(&self, character: char, inside: bool) -> bool {
        match self {
            // Unreachable: `tokens` answers a porter tokenizer from the
            // tokenizer it wraps and never walks the text itself.
            Tokenizer::Porter(inner) => inner.is_token_character(character, inside),
            Tokenizer::Ascii => !character.is_ascii() || character.is_ascii_alphanumeric(),
            Tokenizer::Unicode61 {
                extra_tokens,
                separators,
                ..
            } => {
                if separators.contains(&character) {
                    return false;
                }
                if extra_tokens.contains(&character) {
                    return true;
                }
                character.is_alphanumeric()
                    || (inside && super::fold::is_diacritic(u32::from(character)))
            }
        }
    }

    /// Folds one character onto the end of the token being built.
    ///
    /// **Written into rather than returned**, because a `Vec<char>` per
    /// character is an allocation per character: the folded form of `a` is one
    /// character, and describing it cost a heap allocation and a free. The gate
    /// tokenises fifty-five thousand characters per round of
    /// `extension.fts.build` and spent a fifth of the workload's module time
    /// here.
    ///
    /// A character folds to more than one - the German sharp s lowercases to
    /// `ss` - so the shape has to stay one-to-many; it is the collection that
    /// went, not the generality.
    ///
    /// @param character - the character to fold
    /// @param out - the token being built
    fn fold_into(&self, character: char, out: &mut String) {
        match self {
            // Unreachable for the same reason as `is_token_character`: a porter
            // tokenizer stems what the tokenizer it wraps produced.
            Tokenizer::Porter(inner) => inner.fold_into(character, out),
            Tokenizer::Ascii => out.push(character.to_ascii_lowercase()),
            Tokenizer::Unicode61 {
                remove_diacritics, ..
            } => {
                // SQLite's own table, which a code point of zero leaves out of the token.
                let folded = super::fold::fold(u32::from(character), *remove_diacritics);
                if let Some(folded) = char::from_u32(folded).filter(|_| folded != 0) {
                    out.push(folded);
                }
            }
        }
    }
}

/// The longest token, in bytes, that the index stores.
///
/// A token is the key of its `%_idx` row, and a key has to fit in a page. A
/// token longer than a page made the flush at the end of the statement fail
/// with "a row's keys and fixed-width columns alone are larger than a page"
/// after the row's text had already been written, so the insert reported an
/// error and left the row behind with no index entries. SQLite also cuts a
/// token at a fixed length (32768 bytes); this build cuts at a shorter one
/// because it keeps the whole token in a key. A token longer than this matches
/// on its first bytes, in the index and in a query alike. The text of the
/// column is stored whole.
pub const MAX_TOKEN_BYTES: usize = 1000;

/// Cuts a folded token to the longest the index stores, on a character boundary.
///
/// @param token - the folded token
fn clipped(mut token: String) -> Vec<u8> {
    if token.len() > MAX_TOKEN_BYTES {
        let mut end = MAX_TOKEN_BYTES;
        while end > 0 && !token.is_char_boundary(end) {
            end -= 1;
        }
        token.truncate(end);
    }
    token.into_bytes()
}

/// Reads a `tokenize = '...'` specification into its words.
///
/// The value is a quoted string holding the tokenizer's name and its arguments,
/// each of which may itself be quoted. `tokenize = "unicode61 remove_diacritics
/// 0"` is three words, and `tokenize = "unicode61 tokenchars '_-'"` is four.
pub fn parse_specification(value: &str) -> Vec<Vec<u8>> {
    let words = split_words(strip_quotes(value.trim()));
    if words.is_empty() {
        return vec![b"unicode61".to_vec()];
    }
    words
}

/// Splits text into words, honouring quotes and their doubling.
///
/// A column argument is quoted *per identifier* - `"a b" UNINDEXED` is two
/// words - while a `tokenize` value is one quoted string holding several. The
/// difference is only whether the caller strips the outer quotes first, so the
/// scan is shared and the stripping is not.
pub fn split_words(value: &str) -> Vec<Vec<u8>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut quote: Option<char> = None;
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match quote {
            Some(open) if character == open => {
                // A doubled quote is one quote, which is how SQL escapes it -
                // `'unicode61 tokenchars ''_-'''` is three words and the third
                // is `_-`.
                if characters.peek() == Some(&open) {
                    characters.next();
                    current.push(open);
                    continue;
                }
                quote = None;
                words.push(core::mem::take(&mut current).into_bytes());
                quoted = false;
            }
            Some(_) => current.push(character),
            None if character == '\'' || character == '"' || character == '`' => {
                if !current.is_empty() {
                    words.push(core::mem::take(&mut current).into_bytes());
                }
                quote = Some(character);
                quoted = true;
            }
            None if character.is_whitespace() => {
                if !current.is_empty() {
                    words.push(core::mem::take(&mut current).into_bytes());
                }
            }
            None => current.push(character),
        }
    }
    if !current.is_empty() || quoted {
        words.push(current.into_bytes());
    }
    words.retain(|word| !word.is_empty());
    words
}

/// Removes one layer of quotes from a value.
fn strip_quotes(text: &str) -> &str {
    let bytes = text.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return text;
    };
    let quoted = bytes.len() >= 2
        && matches!(
            (first, last),
            (b'"', b'"') | (b'\'', b'\'') | (b'`', b'`') | (b'[', b']')
        );
    if quoted {
        return text.get(1..text.len() - 1).unwrap_or(text);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Renders tokens as strings for a comparison.
    fn tokens(tokenizer: &Tokenizer, text: &str) -> Vec<String> {
        tokenizer
            .tokens(text.as_bytes())
            .into_iter()
            .map(|token| String::from_utf8_lossy(&token).into_owned())
            .collect()
    }

    /// The default splits on anything that is not a letter or a number.
    #[test]
    fn the_default_splits_on_punctuation() {
        let tokenizer = Tokenizer::named(&[]).expect("a known tokenizer");
        assert_eq!(
            tokens(&tokenizer, "The quick, brown fox!"),
            ["the", "quick", "brown", "fox"]
        );
        assert_eq!(tokens(&tokenizer, "a1 b-2"), ["a1", "b", "2"]);
    }

    /// `ascii` separates at ASCII punctuation, folds only `A-Z`, and keeps
    /// bytes of 128 and above inside the token.
    #[test]
    fn ascii_keeps_non_ascii_bytes_in_the_token() {
        let tokenizer = Tokenizer::named(&[b"ascii".to_vec()]).expect("a known tokenizer");
        assert_eq!(
            tokens(&tokenizer, "Caf\u{e9} au-lait \u{c9}T\u{c9}"),
            ["caf\u{e9}", "au", "lait", "\u{c9}t\u{c9}"]
        );
    }

    /// The default removes diacritics, so an accented word is the plain one.
    #[test]
    fn diacritics_are_removed_by_default() {
        let tokenizer = Tokenizer::named(&[]).expect("a known tokenizer");
        assert_eq!(tokens(&tokenizer, "Caf\u{e9}"), ["cafe"]);
        let kept = Tokenizer::named(&[
            b"unicode61".to_vec(),
            b"remove_diacritics".to_vec(),
            b"0".to_vec(),
        ])
        .expect("a known tokenizer");
        assert_eq!(tokens(&kept, "Caf\u{e9}"), ["caf\u{e9}"]);
    }

    /// `tokenchars` adds characters and `separators` removes them.
    #[test]
    fn the_character_lists_are_honoured() {
        let joined =
            Tokenizer::named(&[b"unicode61".to_vec(), b"tokenchars".to_vec(), b"-".to_vec()])
                .expect("a known tokenizer");
        assert_eq!(tokens(&joined, "well-known"), ["well-known"]);
        let split =
            Tokenizer::named(&[b"unicode61".to_vec(), b"separators".to_vec(), b"x".to_vec()])
                .expect("a known tokenizer");
        assert_eq!(tokens(&split, "axb"), ["a", "b"]);
    }

    /// A specification splits into words, quotes and all.
    #[test]
    fn a_specification_splits_into_words() {
        assert_eq!(parse_specification("'ascii'"), vec![b"ascii".to_vec()]);
        assert_eq!(
            parse_specification("\"unicode61 remove_diacritics 0\""),
            vec![
                b"unicode61".to_vec(),
                b"remove_diacritics".to_vec(),
                b"0".to_vec()
            ]
        );
        assert_eq!(
            parse_specification("'unicode61 tokenchars ''_-'''"),
            vec![
                b"unicode61".to_vec(),
                b"tokenchars".to_vec(),
                b"_-".to_vec(),
            ]
        );
    }

    /// An empty specification is the default tokenizer.
    #[test]
    fn an_empty_specification_is_the_default() {
        assert_eq!(parse_specification(""), vec![b"unicode61".to_vec()]);
        assert!(matches!(
            Tokenizer::named(&parse_specification("")).expect("a known tokenizer"),
            Tokenizer::Unicode61 { .. }
        ));
    }

    /// M5 (task-1920): a tokenizer name this build has not got is refused,
    /// and `trigram` is the one that matters.
    ///
    /// **A substituted tokenizer is a wrong answer, not a graceful
    /// fallback.** `named` read any unrecognised name as `unicode61` on the
    /// reasoning that "a schema naming a tokenizer this build has not got
    /// still opens, and its rows are still found by the words they contain".
    /// But the tokenizer decides what `MATCH` *means*: `tokenize='trigram'`
    /// makes it a substring search in SQLite, and substituting `unicode61`
    /// made it whole-word search. The table was created, the query ran, and
    /// it answered a different question with no error anywhere.
    #[test]
    fn a_tokenizer_this_build_has_not_got_is_refused() {
        // `trigram` is a tokenizer SQLite has and this build does not, so it is `unsupported`.
        let refused = Tokenizer::named(&[b"trigram".to_vec()])
            .err()
            .unwrap_or_else(|| panic!("trigram must be refused, not substituted"));
        assert_eq!(
            refused.unsupported(),
            Some("the fts5 tokenizer trigram"),
            "the refusal must name the tokenizer it refused"
        );
        assert_eq!(
            refused.message(),
            "the fts5 tokenizer trigram is not built",
            "the refusal says the feature is not built, as the other fts5 refusals do"
        );
        // A name SQLite has never heard of is a mistake, and is refused in SQLite's words.
        for name in ["icu", "porter2", "nonsense"] {
            let refused = Tokenizer::named(&[name.as_bytes().to_vec()])
                .err()
                .unwrap_or_else(|| panic!("{name} must be refused, not substituted"));
            assert_eq!(refused.unsupported(), None);
            assert_eq!(refused.message(), format!("no such tokenizer: {name}"));
        }
        // The names this build does have, and the wrapper, still work - which
        // is the half a refusal that turned everything away would break.
        for specification in [
            vec![],
            vec![b"unicode61".to_vec()],
            vec![b"ascii".to_vec()],
            vec![b"porter".to_vec()],
            vec![b"porter".to_vec(), b"ascii".to_vec()],
        ] {
            assert!(
                Tokenizer::named(&specification).is_ok(),
                "{specification:?} is a tokenizer this build has"
            );
        }
        // And an unknown name inside `porter` is refused too, rather than
        // being wrapped and silently substituted.
        assert!(Tokenizer::named(&[b"porter".to_vec(), b"trigram".to_vec()]).is_err());
    }
}

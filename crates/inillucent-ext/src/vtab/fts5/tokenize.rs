//! Turning text into the tokens an index is built from.
//!
//! Invariant: the same text produces the same tokens on the way in and on the
//! way out. That sounds obvious and is the single most common way a full-text
//! index goes wrong: a document indexed with one rule and searched with another
//! finds nothing, and nothing about the failure says why. So indexing and
//! searching call this and only this.
//!
//! Four tokenizers, which are the ones the pinned release has. `ascii` treats
//! every ASCII byte outside `A-Za-z0-9` as a separator, treats every byte of 128
//! and above as part of a token, and folds `A-Z`; `unicode61` treats every
//! character outside the selected Unicode categories as a separator, folds case,
//! and strips the combining marks that make `é` and `e` the same word, and is the
//! default, as it is there; `porter` stems the tokens of another one; and
//! `trigram` makes every window of three characters a token.

use inillucent_base::DbResult;

mod categories;
mod porter;
mod trigram;

/// Which tokenizer an index uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tokenizer {
    /// `ascii`: ASCII bytes outside `A-Za-z0-9` separate, `A-Z` folds.
    ///
    /// Bytes of 128 and above are token characters and are not folded, which is
    /// how SQLite's `ascii` tokenizer treats them: `Caf\u{e9}` is one token.
    /// `tokenchars` and `separators` change which ASCII characters are token characters.
    Ascii {
        /// Whether each ASCII character is part of a token.
        token_chars: [bool; 128],
    },
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
    /// `unicode61`: the characters of the selected Unicode categories are tokens.
    Unicode61 {
        /// What `remove_diacritics` asked for: 0 keeps diacritics, 1 removes them and 2 also
        /// removes the ranges SQLite only removes at that setting.
        remove_diacritics: u8,
        /// Which of SQLite's 32 category numbers are token characters. The default is `L* N* Co`.
        categories: [bool; 32],
        /// Code points above 127 whose class `tokenchars` or `separators` reversed, in order.
        exceptions: Vec<u32>,
        /// Whether each ASCII character is part of a token.
        token_chars: [bool; 128],
    },
    /// `trigram`: every run of three characters is a token, so `MATCH` finds substrings.
    Trigram {
        /// Whether characters are folded to lower case, which `case_sensitive 0` (the default) asks.
        fold: bool,
        /// The `remove_diacritics` setting passed to the fold: 0 or 2.
        fold_param: u8,
    },
}

/// Returns the failure SQLite gives a tokenizer whose arguments it refuses.
fn constructor_error() -> inillucent_base::DbError {
    let said = "error in tokenizer constructor".to_string();
    crate::vtab::failure(said.clone()).with_message(said)
}

/// Returns the lower case spelling of one tokenizer argument, which SQLite compares without case.
///
/// @param word - an option name as written
fn option_name(word: &[u8]) -> String {
    String::from_utf8_lossy(word).to_ascii_lowercase()
}

/// Returns the setting a `remove_diacritics` value names, or nothing for a value SQLite refuses.
///
/// SQLite accepts exactly one character, and only `0`, `1` or `2`.
///
/// @param value - the option's value as written
fn diacritics_setting(value: &[u8]) -> Option<u8> {
    match value {
        [digit @ (b'0' | b'1' | b'2')] => Some(digit.saturating_sub(b'0')),
        _ => None,
    }
}

/// Returns the characters of a tokenizer argument that fall in the ASCII range, as SQLite reads
/// them: a byte at a time, ignoring every byte with the high bit set.
///
/// @param value - the argument text
fn ascii_characters(value: &[u8]) -> impl Iterator<Item = usize> + '_ {
    value
        .iter()
        .filter(|byte| byte.is_ascii())
        .map(|byte| usize::from(*byte))
}

impl Tokenizer {
    /// Returns the tokenizer a specification names.
    ///
    /// **A name this build has not got is refused, and used to be read as
    /// `unicode61` (task-1932, M5).** The tokenizer is what a query *means*:
    /// `tokenize='trigram'` makes `MATCH` a substring search, and substituting
    /// `unicode61` made it whole-word search without an error anywhere. A name
    /// SQLite has never heard of is `no such tokenizer`, and an argument a
    /// tokenizer does not take, an odd number of arguments or a value out of range
    /// is `error in tokenizer constructor`, as SQLite reports them.
    ///
    /// @param specification - the `tokenize=` words, the name first
    pub fn named(specification: &[Vec<u8>]) -> DbResult<Tokenizer> {
        let name = specification
            .first()
            .map(|part| option_name(part))
            .unwrap_or_default();
        let arguments = specification.get(1..).unwrap_or(&[]);
        match name.as_str() {
            "porter" => {
                // Whatever follows `porter` is the tokenizer it wraps, and nothing
                // following it means the default one.
                // SQLite reports a wrapped tokenizer it cannot find with the wrapper's own words.
                let wrapped = Tokenizer::named(arguments).map_err(|error| {
                    if error.unsupported().is_some() {
                        return error;
                    }
                    constructor_error()
                })?;
                Ok(Tokenizer::Porter(Box::new(wrapped)))
            }
            "ascii" => Tokenizer::ascii(arguments),
            "" | "unicode61" => Tokenizer::unicode61(arguments),
            "trigram" => Tokenizer::trigram(arguments),
            _ => {
                let said = format!(
                    "no such tokenizer: {}",
                    String::from_utf8_lossy(
                        specification.first().map(Vec::as_slice).unwrap_or(&[])
                    )
                );
                Err(crate::vtab::failure(said.clone()).with_message(said))
            }
        }
    }

    /// Builds the `ascii` tokenizer, which takes `tokenchars` and `separators` and nothing else.
    ///
    /// @param arguments - the words after the name, in name and value pairs
    fn ascii(arguments: &[Vec<u8>]) -> DbResult<Tokenizer> {
        if arguments.len() % 2 == 1 {
            return Err(constructor_error());
        }
        let mut token_chars = [false; 128];
        for (at, flag) in token_chars.iter_mut().enumerate() {
            *flag = u8::try_from(at).is_ok_and(|byte| byte.is_ascii_alphanumeric());
        }
        for pair in arguments.chunks_exact(2) {
            let (Some(key), Some(value)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            let token = match option_name(key).as_str() {
                "tokenchars" => true,
                "separators" => false,
                _ => return Err(constructor_error()),
            };
            for at in ascii_characters(value) {
                if let Some(flag) = token_chars.get_mut(at) {
                    *flag = token;
                }
            }
        }
        Ok(Tokenizer::Ascii { token_chars })
    }

    /// Builds the `unicode61` tokenizer in the order SQLite's constructor does.
    ///
    /// The categories are chosen first, because a character's class decides whether `tokenchars`
    /// or `separators` has anything to change for it, and then the other options are applied in
    /// the order they were written.
    ///
    /// @param arguments - the words after the name, in name and value pairs
    fn unicode61(arguments: &[Vec<u8>]) -> DbResult<Tokenizer> {
        if arguments.len() % 2 == 1 {
            return Err(constructor_error());
        }
        let mut selected: &[u8] = b"L* N* Co";
        for pair in arguments.chunks_exact(2) {
            if let (Some(key), Some(value)) = (pair.first(), pair.get(1)) {
                if option_name(key) == "categories" {
                    selected = value;
                }
            }
        }
        let mut categories = [false; 32];
        for word in selected
            .split(|byte| *byte == b' ' || *byte == b'\t')
            .filter(|word| !word.is_empty())
        {
            if categories::parse_category(word, &mut categories) {
                return Err(constructor_error());
            }
        }
        let mut token_chars = [false; 128];
        for (code, flag) in token_chars.iter_mut().enumerate() {
            let class = categories::category(u32::try_from(code).unwrap_or(0));
            *flag = categories.get(class).copied().unwrap_or(false) && code != 0;
        }
        let mut remove_diacritics = 1u8;
        let mut exceptions: Vec<u32> = Vec::new();
        for pair in arguments.chunks_exact(2) {
            let (Some(key), Some(value)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            match option_name(key).as_str() {
                "remove_diacritics" => {
                    remove_diacritics = diacritics_setting(value).ok_or_else(constructor_error)?;
                }
                "tokenchars" => {
                    add_exceptions(&categories, &mut token_chars, &mut exceptions, value, true)
                }
                "separators" => {
                    add_exceptions(&categories, &mut token_chars, &mut exceptions, value, false)
                }
                "categories" => {}
                _ => return Err(constructor_error()),
            }
        }
        Ok(Tokenizer::Unicode61 {
            remove_diacritics,
            categories,
            exceptions,
            token_chars,
        })
    }

    /// Builds the `trigram` tokenizer, which takes `case_sensitive` and `remove_diacritics`.
    ///
    /// @param arguments - the words after the name, in name and value pairs
    fn trigram(arguments: &[Vec<u8>]) -> DbResult<Tokenizer> {
        if arguments.len() % 2 == 1 {
            return Err(constructor_error());
        }
        let mut fold = true;
        let mut fold_param = 0u8;
        for pair in arguments.chunks_exact(2) {
            let (Some(key), Some(value)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            match option_name(key).as_str() {
                "case_sensitive" => match value.as_slice() {
                    [b'0'] => fold = true,
                    [b'1'] => fold = false,
                    _ => return Err(constructor_error()),
                },
                "remove_diacritics" => {
                    let setting = diacritics_setting(value).ok_or_else(constructor_error)?;
                    fold_param = if setting == 0 { 0 } else { 2 };
                }
                _ => return Err(constructor_error()),
            }
        }
        // Removing diacritics folds case first, so it cannot be asked for beside `case_sensitive 1`.
        if fold_param != 0 && !fold {
            return Err(constructor_error());
        }
        Ok(Tokenizer::Trigram { fold, fold_param })
    }

    /// Returns whether a table's `LIKE` and `GLOB` can be answered from the trigram index, and
    /// for which of the two.
    ///
    /// `LIKE` is case insensitive and can only use a folding trigram index, while `GLOB` is case
    /// sensitive and uses either. A trigram index that removes diacritics answers neither,
    /// because the pattern's characters no longer say which rows hold them.
    pub fn pattern_support(&self) -> Option<PatternSupport> {
        match self {
            Tokenizer::Trigram {
                fold,
                fold_param: 0,
            } => Some(match fold {
                true => PatternSupport::LikeAndGlob,
                false => PatternSupport::GlobOnly,
            }),
            _ => None,
        }
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
        if let Tokenizer::Trigram { fold, fold_param } = self {
            return trigram::spans(&text, *fold, *fold_param);
        }
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
        if let Tokenizer::Trigram { fold, fold_param } = self {
            return trigram::spans(&text, *fold, *fold_param)
                .into_iter()
                .map(|(token, _, _)| token)
                .collect();
        }
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
            // Unreachable: a trigram tokenizer reads windows of characters, not runs.
            Tokenizer::Trigram { .. } => true,
            // Bytes of 128 and above are token characters, and so is every code point made of them.
            Tokenizer::Ascii { token_chars } => match usize::try_from(u32::from(character)) {
                Ok(code) if code < 128 => token_chars.get(code).copied().unwrap_or(false),
                _ => true,
            },
            Tokenizer::Unicode61 {
                categories,
                exceptions,
                token_chars,
                ..
            } => {
                let code = categories::decoded(u32::from(character));
                if let Some(flag) = usize::try_from(code)
                    .ok()
                    .filter(|code| *code < 128)
                    .and_then(|code| token_chars.get(code))
                {
                    return *flag;
                }
                let by_category = categories
                    .get(categories::category(code))
                    .copied()
                    .unwrap_or(false);
                by_category ^ exceptions.contains(&code)
                    || (inside && super::fold::is_diacritic(code))
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
            Tokenizer::Trigram { .. } => out.push(character),
            // Only `A-Z` fold, and a byte of 128 and above is kept as it is.
            Tokenizer::Ascii { .. } => out.push(character.to_ascii_lowercase()),
            Tokenizer::Unicode61 {
                remove_diacritics, ..
            } => {
                // SQLite's own table, which a code point of zero leaves out of the token.
                let folded = super::fold::fold(
                    categories::decoded(u32::from(character)),
                    *remove_diacritics,
                );
                if let Some(folded) = char::from_u32(folded).filter(|_| folded != 0) {
                    out.push(folded);
                }
            }
        }
    }
}

/// What a trigram index lets `LIKE` and `GLOB` use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatternSupport {
    /// A folding index: `LIKE` and `GLOB` both narrow the rows by trigrams.
    LikeAndGlob,
    /// A case sensitive index: only `GLOB` does.
    GlobOnly,
}

/// Applies a `tokenchars` or `separators` value, as SQLite's `fts5UnicodeAddExceptions` does.
///
/// An ASCII character is set in the table. Any other character is recorded as an exception only
/// when its category says the opposite and it is not a diacritic, so a character that is already
/// the way the option wants is not recorded. Nothing removes an exception once it is recorded.
///
/// @param categories - which categories are token characters
/// @param token_chars - the ASCII table, changed for the ASCII characters of the value
/// @param exceptions - the code points recorded so far
/// @param value - the characters the option names
/// @param token - whether they are made token characters or separators
fn add_exceptions(
    categories: &[bool; 32],
    token_chars: &mut [bool; 128],
    exceptions: &mut Vec<u32>,
    value: &[u8],
    token: bool,
) {
    for character in String::from_utf8_lossy(value).chars() {
        let code = categories::decoded(u32::from(character));
        if let Some(flag) = usize::try_from(code)
            .ok()
            .filter(|code| *code < 128)
            .and_then(|code| token_chars.get_mut(code))
        {
            *flag = token;
            continue;
        }
        let by_category = categories
            .get(categories::category(code))
            .copied()
            .unwrap_or(false);
        if by_category != token && !super::fold::is_diacritic(code) {
            exceptions.push(code);
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
    // An empty quoted word is an argument: `categories ''` selects no category at all.
    let words = split_words_keeping(&dequote(value.trim()), true);
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
    split_words_keeping(value, false)
}

/// Splits text into words, optionally keeping an empty word that was written in quotes.
///
/// @param value - the text to split
/// @param keep_empty - whether `''` is a word of no characters rather than nothing
fn split_words_keeping(value: &str, keep_empty: bool) -> Vec<Vec<u8>> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut quote: Option<char> = None;
    let mut characters = value.chars().peekable();
    while let Some(character) = characters.next() {
        match quote {
            Some(open) if character == closing_quote(open) => {
                // A doubled quote is one quote, which is how SQL escapes it -
                // `'unicode61 tokenchars ''_-'''` is three words and the third
                // is `_-`. A bracket has no such escape.
                if open != '[' && characters.peek() == Some(&open) {
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
            // **A word that starts with `[` runs to the next `]`**, which is how FTS5 reads
            // `fts5([x y], z)`: the column is called `x y`.
            None if character == '[' && current.is_empty() => {
                quote = Some(character);
                quoted = true;
            }
            None => current.push(character),
        }
    }
    if !current.is_empty() || quoted {
        words.push(current.into_bytes());
    }
    if !keep_empty {
        words.retain(|word| !word.is_empty());
    }
    words
}

/// Returns the character that closes a quote opened with `open`.
///
/// @param open - the quote character a word opened with
fn closing_quote(open: char) -> char {
    if open == '[' {
        ']'
    } else {
        open
    }
}

/// Removes one layer of quotes from a value and turns each doubled quote inside into one.
///
/// **SQL quoting is undone before the words are split.** `'porter unicode61 tokenchars ''-'''`
/// is the text `porter unicode61 tokenchars '-'`, and splitting it with its doubled quotes still
/// in place read each `''` as an empty word and only worked because empty words were dropped.
///
/// @param text - the value as written, quotes included
fn dequote(text: &str) -> String {
    let mut characters = text.chars();
    let (Some(first), Some(last)) = (characters.next(), text.chars().next_back()) else {
        return text.to_string();
    };
    if text.chars().count() < 2 || first != last || !matches!(first, '\'' | '"' | '`') {
        return strip_quotes(text).to_string();
    }
    let inner: String = text
        .chars()
        .skip(1)
        .take(text.chars().count().saturating_sub(2))
        .collect();
    let doubled: String = [first, first].iter().collect();
    inner.replace(&doubled, &first.to_string())
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
        // `trigram` is a tokenizer SQLite has, and this build has it too.
        assert!(matches!(
            Tokenizer::named(&[b"trigram".to_vec()]).expect("trigram is built"),
            Tokenizer::Trigram { .. }
        ));
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
        assert!(Tokenizer::named(&[b"porter".to_vec(), b"nonsense".to_vec()]).is_err());
    }
}

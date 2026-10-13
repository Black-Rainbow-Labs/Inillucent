//! A tokenizer pack: a small JSON file that says how one language's words become
//! terms. It names a stemmer, a stopword list, whether accents are folded, and a
//! list of suffix rewrites.
//!
//! Invariant: **a pack that parses is a pack every step can use as written.**
//! Everything a tokenizer would otherwise have to check per word is checked once
//! here: the stemmer exists, every stopword could match a word, and every suffix
//! rule could match a word. A pack file is text somebody else wrote, so nothing
//! in this module indexes, unwraps or panics on it, and a file over
//! [`MAX_PACK_BYTES`] is refused before it is parsed.
//!
//! A pack supplies only what happens to one lowercase word. Splitting text into
//! words, keeping identifiers and addresses whole, and the 64 byte limit stay in
//! [`super::Tokenizer::terms`], so no pack can change them.

use serde::{Deserialize, Serialize};

use super::fold::fold;

/// The pack format this build reads.
///
/// A stored pack in a later format is refused rather than read as this one,
/// because a field this build does not know about could change what a word
/// becomes.
pub const PACK_FORMAT: u64 = 1;

/// The name of the tokenizer built into the engine.
///
/// No pack may take it, so a table that says `porter` always means the built in
/// English tokenizer and never a file somebody installed.
pub const PORTER: &str = "porter";

/// The stemmers a pack may name, besides `none`.
///
/// These are the eighteen Snowball algorithms `rust-stemmers` 1.2.0 links. The
/// list is the one `stemmer_algorithm` matches on, and a test checks that each
/// name builds a stemmer.
pub const STEMMERS: &[&str] = &[
    "arabic",
    "danish",
    "dutch",
    "english",
    "finnish",
    "french",
    "german",
    "greek",
    "hungarian",
    "italian",
    "norwegian",
    "portuguese",
    "romanian",
    "russian",
    "spanish",
    "swedish",
    "tamil",
    "turkish",
];

/// The stemmer name that means no stemming.
pub const NO_STEMMER: &str = "none";

/// The largest pack file this build reads, in bytes.
///
/// A real pack is a few kilobytes: the Brazilian Portuguese example is under
/// five. The limit exists so a wrong file named `.json` is refused instead of
/// being read into memory.
pub const MAX_PACK_BYTES: usize = 1024 * 1024;

/// The most characters a pack's name may have.
pub const MAX_NAME_CHARS: usize = 32;

/// The most characters a pack's description may have.
pub const MAX_DESCRIPTION_CHARS: usize = 200;

/// The most stopwords a pack may list.
pub const MAX_STOPWORDS: usize = 10_000;

/// The most suffix rules a pack may list.
pub const MAX_SUFFIXES: usize = 256;

/// The longest a stopword, an ending or a replacement may be, in bytes.
///
/// The same 64 bytes past which the tokenizer drops a word: a longer stopword
/// or ending could never match a word the tokenizer keeps.
pub const MAX_WORD_BYTES: usize = 64;

/// One suffix rewrite: a word that ends in `ending` has it replaced by `becomes`
/// before the stemmer reads it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Suffix {
    /// The ending to look for, compared with the word after folding.
    pub ending: String,
    /// What the ending is replaced with. It may be empty, which removes it.
    pub becomes: String,
}

/// A tokenizer pack, as its file says it.
///
/// The fields serialize in declaration order and the lists in file order, so
/// [`Pack::to_json`] gives the same text for the same pack every time. That
/// text is what a table stores in its `%_config`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pack {
    /// The pack format, which must be [`PACK_FORMAT`].
    pub format: u64,
    /// The name a table declares with `tokenize = '<name>'`.
    pub name: String,
    /// One line on what the pack is for, shown by `inillucent tokenizer list`.
    #[serde(default)]
    pub description: String,
    /// One of [`STEMMERS`], or [`NO_STEMMER`].
    pub stemmer: String,
    /// Whether accents are folded before and after the stemmer.
    #[serde(default)]
    pub fold: bool,
    /// Words that produce no term. Compared after folding when `fold` is true.
    #[serde(default)]
    pub stopwords: Vec<String>,
    /// Suffix rewrites applied before the stemmer. The longest ending that
    /// matches wins, so their order in the file does not matter.
    #[serde(default)]
    pub suffixes: Vec<Suffix>,
}

/// Why a pack was refused.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PackError {
    /// The file is larger than [`MAX_PACK_BYTES`].
    #[error("a tokenizer pack is at most {MAX_PACK_BYTES} bytes, and this one is {0}")]
    TooLarge(u64),
    /// The file could not be read.
    #[error("the tokenizer pack could not be read: {0}")]
    Read(String),
    /// The text is not JSON, or not the shape of a pack.
    #[error("the tokenizer pack does not read: {0}")]
    Json(String),
    /// The pack is in a format this build does not read.
    #[error("the tokenizer pack is in format {0}, and this build reads format {PACK_FORMAT}")]
    Format(u64),
    /// The name breaks the naming rule.
    #[error(
        "a tokenizer pack name is 1 to {MAX_NAME_CHARS} lowercase letters, digits, - and _, \
         starts with a letter or a digit, and is not {PORTER}: {0:?} is not"
    )]
    Name(String),
    /// The file is installed under one name and the pack inside it says another.
    #[error("the file {file}.json holds the tokenizer pack named {pack}")]
    Misnamed {
        /// The name the file was looked up by.
        file: String,
        /// The name the pack inside says.
        pack: String,
    },
    /// The description is too long.
    #[error("a tokenizer pack description is at most {MAX_DESCRIPTION_CHARS} characters, and this one is {0}")]
    Description(usize),
    /// The stemmer is not one this build links.
    #[error("no stemmer is called {0}. The stemmers are none, {list}", list = STEMMERS.join(", "))]
    Stemmer(String),
    /// A stopword could never be compared with a word.
    #[error(
        "the stopword {0:?} could never match a word: a stopword is lowercase letters and \
         digits only, at most {MAX_WORD_BYTES} bytes"
    )]
    Stopword(String),
    /// A suffix rule could never match, or repeats another.
    #[error("the suffix rule for {ending:?} {why}")]
    Suffix {
        /// The rule's ending.
        ending: String,
        /// What is wrong with it.
        why: String,
    },
    /// A list is longer than its limit.
    #[error("a tokenizer pack lists at most {limit} {what}, and this one lists {count}")]
    TooMany {
        /// Which list.
        what: &'static str,
        /// Its limit.
        limit: usize,
        /// How long it is.
        count: usize,
    },
}

/// Reports whether a name may be a pack's name.
///
/// One to 32 characters: a lowercase ASCII letter or digit first, then lowercase
/// ASCII letters, digits, `-` and `_`. The rule keeps the name usable as a file
/// name on every platform, so `../x` or `pt/br` can never reach outside the
/// tokenizer folder, and `porter` is refused so it always means the built in
/// tokenizer.
///
/// @param name - the name to check
pub fn valid_name(name: &str) -> bool {
    let mut letters = name.chars();
    let Some(first) = letters.next() else {
        return false;
    };
    name != PORTER
        && name.chars().count() <= MAX_NAME_CHARS
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && letters.all(|letter| {
            letter.is_ascii_lowercase() || letter.is_ascii_digit() || matches!(letter, '-' | '_')
        })
}

/// Reports whether a word could be produced by the tokenizer: lowercase letters
/// and digits only, not empty, at most [`MAX_WORD_BYTES`] bytes.
///
/// @param word - a stopword or an ending
fn could_be_a_word(word: &str) -> bool {
    !word.is_empty()
        && word.len() <= MAX_WORD_BYTES
        && word.chars().all(char::is_alphanumeric)
        && word.to_lowercase() == word
}

impl Pack {
    /// Reads and checks a pack from its JSON text.
    ///
    /// The format is read first and on its own, so a pack in a later format is
    /// reported as that, even when it carries a field this build has never
    /// heard of.
    ///
    /// @param text - the file's contents
    pub fn parse(text: &str) -> Result<Pack, PackError> {
        if text.len() > MAX_PACK_BYTES {
            return Err(PackError::TooLarge(text.len() as u64));
        }
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|error| PackError::Json(error.to_string()))?;
        let format = value
            .get("format")
            .ok_or_else(|| PackError::Json("a pack names its format".to_string()))?;
        let format = format.as_u64().ok_or_else(|| {
            PackError::Json(format!("the format is a whole number, not {format}"))
        })?;
        if format != PACK_FORMAT {
            return Err(PackError::Format(format));
        }
        let pack: Pack =
            serde_json::from_value(value).map_err(|error| PackError::Json(error.to_string()))?;
        pack.check()?;
        Ok(pack)
    }

    /// Returns the pack's canonical JSON: no spaces, fields in declaration order,
    /// lists in file order.
    ///
    /// This is what a table stores and what `inillucent tokenizer install`
    /// writes, so two installs of the same pack compare equal as text.
    pub fn to_json(&self) -> String {
        // `serde_json::to_string` fails only for a map with keys that are not
        // strings or a `Serialize` that reports an error. A pack has neither, so
        // the empty fallback is unreachable, and an empty string would fail to
        // parse and be refused rather than read as some other pack.
        serde_json::to_string(self).unwrap_or_default()
    }

    /// Checks every rule a parsed pack must keep.
    fn check(&self) -> Result<(), PackError> {
        if !valid_name(&self.name) {
            return Err(PackError::Name(self.name.clone()));
        }
        let described = self.description.chars().count();
        if described > MAX_DESCRIPTION_CHARS {
            return Err(PackError::Description(described));
        }
        if self.stemmer != NO_STEMMER && !STEMMERS.contains(&self.stemmer.as_str()) {
            return Err(PackError::Stemmer(self.stemmer.clone()));
        }
        if self.stopwords.len() > MAX_STOPWORDS {
            return Err(PackError::TooMany {
                what: "stopwords",
                limit: MAX_STOPWORDS,
                count: self.stopwords.len(),
            });
        }
        if let Some(word) = self.stopwords.iter().find(|word| !could_be_a_word(word)) {
            return Err(PackError::Stopword(word.clone()));
        }
        self.check_suffixes()
    }

    /// Checks the suffix rules: each could match a word, and no ending repeats.
    fn check_suffixes(&self) -> Result<(), PackError> {
        if self.suffixes.len() > MAX_SUFFIXES {
            return Err(PackError::TooMany {
                what: "suffix rules",
                limit: MAX_SUFFIXES,
                count: self.suffixes.len(),
            });
        }
        let refuse = |rule: &Suffix, why: &str| PackError::Suffix {
            ending: rule.ending.clone(),
            why: why.to_string(),
        };
        let mut seen = std::collections::HashSet::new();
        for rule in &self.suffixes {
            if !could_be_a_word(&rule.ending) {
                return Err(refuse(
                    rule,
                    "could never match: an ending is lowercase letters and digits only",
                ));
            }
            // The word is folded before the rules are read, so an ending with an
            // accent in it can never be found in a folding pack.
            if self.fold && fold(&rule.ending) != rule.ending.as_str() {
                return Err(refuse(
                    rule,
                    "could never match: the pack folds accents before it reads an ending, so \
                     an ending is written without them",
                ));
            }
            let replacement_reads = rule.becomes.is_empty() || could_be_a_word(&rule.becomes);
            if !replacement_reads {
                return Err(refuse(
                    rule,
                    "has a replacement that is not lowercase letters and digits",
                ));
            }
            if !seen.insert(rule.ending.as_str()) {
                return Err(refuse(rule, "is listed twice"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example pack the repository ships, read where it is kept.
    const EXAMPLE: &str = include_str!("../../../../examples/tokenizers/pt-br.json");

    /// A pack that names only what it must reads with every default.
    #[test]
    fn a_minimal_pack_reads_with_its_defaults() {
        assert_eq!(
            Pack::parse(r#"{"format":1,"name":"plain","stemmer":"none"}"#),
            Ok(Pack {
                format: 1,
                name: "plain".to_string(),
                description: String::new(),
                stemmer: "none".to_string(),
                fold: false,
                stopwords: Vec::new(),
                suffixes: Vec::new(),
            })
        );
    }

    /// A later format is refused by its number, even with a field this build
    /// does not know.
    #[test]
    fn a_pack_in_a_later_format_is_refused_naming_it() {
        let refused = Pack::parse(r#"{"format":2,"name":"x","stemmer":"none","languages":[]}"#);
        assert_eq!(refused, Err(PackError::Format(2)));
        let text = refused
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(text.contains("format 2"), "{text}");
    }

    /// An unknown stemmer is refused, and the message lists every one there is.
    #[test]
    fn an_unknown_stemmer_is_refused_listing_every_stemmer() {
        let refused = Pack::parse(r#"{"format":1,"name":"x","stemmer":"klingon"}"#);
        assert_eq!(refused, Err(PackError::Stemmer("klingon".to_string())));
        let text = refused
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        for named in ["klingon", "none", "portuguese", "english", "turkish"] {
            assert!(text.contains(named), "{named} is missing from {text}");
        }
    }

    /// Every stemmer the list names builds one.
    #[test]
    fn every_listed_stemmer_builds() {
        for name in STEMMERS {
            assert!(super::super::stemmer_algorithm(name).is_some(), "{name}");
        }
        assert!(super::super::stemmer_algorithm(NO_STEMMER).is_none());
    }

    /// A name that is not a plain word is refused, and so is `porter`.
    #[test]
    fn a_name_that_is_not_a_plain_word_is_refused() {
        let long = "a".repeat(MAX_NAME_CHARS + 1);
        for name in ["", "PT", "pt/br", "../x", "-pt", "pt br", "porter", &long] {
            let text = format!(r#"{{"format":1,"name":{name:?},"stemmer":"none"}}"#);
            assert_eq!(
                Pack::parse(&text),
                Err(PackError::Name(name.to_string())),
                "{name:?}"
            );
        }
        for name in ["pt-br", "pt_br", "x", "2", &"a".repeat(MAX_NAME_CHARS)] {
            assert!(valid_name(name), "{name}");
        }
    }

    /// A misspelled field is refused, not ignored.
    #[test]
    fn an_unknown_field_is_refused() {
        let refused = Pack::parse(r#"{"format":1,"name":"x","stemmer":"none","stopword":["de"]}"#);
        let text = refused
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(text.contains("stopword"), "{text}");
        assert!(text.contains("unknown field"), "{text}");
    }

    /// A stopword with a space, a capital or a hyphen could never equal a word,
    /// because a word holds none of them.
    #[test]
    fn a_stopword_that_could_never_match_is_refused() {
        for word in ["não é", "Não", "e-mail", ""] {
            let text = format!(
                r#"{{"format":1,"name":"x","stemmer":"none","stopwords":["de",{word:?}]}}"#
            );
            assert_eq!(
                Pack::parse(&text),
                Err(PackError::Stopword(word.to_string())),
                "{word:?}"
            );
        }
    }

    /// An accented ending in a folding pack and a repeated ending are refused.
    #[test]
    fn a_suffix_rule_that_could_never_match_is_refused() {
        let accented = Pack::parse(
            r#"{"format":1,"name":"x","stemmer":"none","fold":true,
                "suffixes":[{"ending":"ção","becomes":"cao"}]}"#,
        );
        assert!(
            matches!(&accented, Err(PackError::Suffix { ending, .. }) if ending == "ção"),
            "{accented:?}"
        );
        let unfolded = Pack::parse(
            r#"{"format":1,"name":"x","stemmer":"none","fold":false,
                "suffixes":[{"ending":"ção","becomes":"cao"}]}"#,
        );
        assert!(unfolded.is_ok(), "{unfolded:?}");
        let repeated = Pack::parse(
            r#"{"format":1,"name":"x","stemmer":"none",
                "suffixes":[{"ending":"cao","becomes":"ção"},{"ending":"cao","becomes":"çao"}]}"#,
        );
        let text = repeated
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default();
        assert!(text.contains("cao") && text.contains("twice"), "{text}");
    }

    /// The example pack writes back to itself, and gives the same text twice.
    #[test]
    fn a_pack_writes_back_to_itself() {
        let pack = Pack::parse(EXAMPLE).expect("the example pack reads");
        assert_eq!(pack.name, "pt-br");
        assert_eq!(pack.stopwords.len(), 203);
        assert_eq!(pack.suffixes.len(), 12);
        let json = pack.to_json();
        assert_eq!(Pack::parse(&json), Ok(pack.clone()));
        assert_eq!(json, pack.to_json());
        assert!(json.starts_with(r#"{"format":1,"name":"pt-br","#), "{json}");
    }

    /// A file over the limit is refused before it is parsed.
    #[test]
    fn a_pack_over_the_size_limit_is_refused_unread() {
        let text = " ".repeat(MAX_PACK_BYTES + 1);
        assert_eq!(
            Pack::parse(&text),
            Err(PackError::TooLarge((MAX_PACK_BYTES + 1) as u64))
        );
    }
}

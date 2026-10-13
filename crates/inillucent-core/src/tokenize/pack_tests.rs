//! What a tokenizer built from a pack does, next to the built in `porter`.
//!
//! Invariant: **every claim here is checked under each tokenizer it is about.**
//! The three are `porter`, the Brazilian Portuguese example pack the repository
//! ships, and a `simple` pack that lowercases and folds and does nothing else.

use super::golden::GOLDEN;
use super::pack::Pack;
use super::Tokenizer;

/// The example pack, read where the repository keeps it.
const EXAMPLE: &str = include_str!("../../../../examples/tokenizers/pt-br.json");

/// A pack with no stemmer, no stopwords and no suffix rules, which folds.
const SIMPLE: &str = r#"{"format":1,"name":"simple","stemmer":"none","fold":true}"#;

/// Returns the tokenizer a pack's text describes.
///
/// @param text - the pack's JSON
fn from(text: &str) -> Tokenizer {
    Tokenizer::from_pack(&Pack::parse(text).expect("the pack reads"))
}

/// Returns the three tokenizers every shared property is checked under.
fn every_tokenizer() -> Vec<Tokenizer> {
    vec![Tokenizer::default(), from(EXAMPLE), from(SIMPLE)]
}

/// `porter` gives every line of the recorded corpus the terms it gave before
/// packs existed, one line at a time and all of them at once.
#[test]
fn the_default_tokenizer_matches_the_terms_it_produced_before_packs() {
    let porter = Tokenizer::default();
    assert_eq!(porter.name(), "porter");
    assert!(GOLDEN.len() >= 60, "{} lines", GOLDEN.len());
    for (line, expected) in GOLDEN {
        assert_eq!(porter.terms(line).join(" "), *expected, "{line:?}");
    }
    let text: Vec<&str> = GOLDEN.iter().map(|(line, _)| *line).collect();
    let expected: Vec<&str> = GOLDEN
        .iter()
        .flat_map(|(_, terms)| terms.split_whitespace())
        .collect();
    assert_eq!(porter.terms(&text.join("\n")), expected);
}

/// Every way of writing a Portuguese word with or without its accents reaches
/// one term, and a plural reaches its singular.
#[test]
fn every_accent_spelling_gives_one_term() {
    let pack = from(EXAMPLE);
    assert_eq!(pack.name(), "pt-br");
    for (spellings, term) in [
        (
            &["notificação", "notificaçao", "notificacao", "notificações"][..],
            "notific",
        ),
        (&["reembolsos", "reembolso"][..], "reembols"),
        (&["informações", "informacoes", "informação"][..], "inform"),
        (&["experiência", "experiencias"][..], "experient"),
        (&["disponível", "disponivel"][..], "dispon"),
    ] {
        for spelling in spellings {
            assert_eq!(pack.terms(spelling), vec![term], "{spelling}");
        }
    }
}

/// A stopword in the file matches the word with or without its accents, and
/// `porter` still keeps a Portuguese word it has never heard of.
#[test]
fn stopwords_are_compared_after_folding() {
    assert!(from(EXAMPLE).terms("não é você que está").is_empty());
    assert!(from(EXAMPLE).terms("nao e voce que esta").is_empty());
    assert_eq!(Tokenizer::default().terms("não"), vec!["não"]);
}

/// A compound identifier and an address are emitted whole, lowercased and
/// nothing else, whatever the tokenizer does to ordinary words.
#[test]
fn identifiers_and_addresses_are_kept_whole_under_every_tokenizer() {
    let text = "SC-1522 parse_headers ana.souza@exemplo.com.br notificação_v2";
    for tokenizer in every_tokenizer() {
        let terms = tokenizer.terms(text);
        for whole in [
            "sc-1522",
            "parse_headers",
            "ana.souza@exemplo.com.br",
            "notificação_v2",
        ] {
            assert!(
                terms.iter().any(|term| term == whole),
                "{} lost {whole}: {terms:?}",
                tokenizer.name()
            );
        }
    }
}

/// The longest ending wins whatever order the file lists the rules in.
#[test]
fn the_longest_suffix_wins() {
    let pack = from(
        r#"{"format":1,"name":"longest","stemmer":"none",
            "suffixes":[{"ending":"ao","becomes":"ão"},{"ending":"cao","becomes":"ção"}]}"#,
    );
    assert_eq!(pack.terms("notificacao"), vec!["notificação"]);
    assert_eq!(pack.terms("mao"), vec!["mão"]);
    // A word no longer than an ending is left alone by that rule: `cao` is not
    // rewritten by the `cao` rule, but it is longer than `ao`.
    assert_eq!(pack.terms("cao"), vec!["cão"]);
    assert_eq!(pack.terms("ao"), vec!["ao"]);
}

/// Text split in two produces the terms of the whole, which is what lets the
/// BM25 scorer count a heading's terms on their own.
#[test]
fn a_heading_and_a_body_tokenize_as_their_join() {
    let pairs = [
        ("Política de reembolso", "Notificações enviadas por e-mail"),
        ("SC-1522", "parse_headers falhou"),
        ("How it works", "The offers are redeemed"),
        ("", "ana.souza@exemplo.com.br"),
        // The heading ends with the term the body starts with, so a tokenizer
        // that merged repeats across the split would count one term short.
        ("Reembolso", "reembolsos aprovados reembolso"),
    ];
    for tokenizer in every_tokenizer() {
        for (heading, body) in pairs {
            let mut joined = tokenizer.terms(heading);
            joined.extend(tokenizer.terms(body));
            assert_eq!(
                tokenizer.terms(&format!("{heading} {body}")),
                joined,
                "{} on {heading:?}",
                tokenizer.name()
            );
        }
    }
}

/// A query produces exactly the terms the same text produced in the index.
#[test]
fn index_and_query_terms_agree_under_every_tokenizer() {
    let mut texts: Vec<&str> = GOLDEN.iter().map(|(line, _)| *line).collect();
    texts.extend([
        "Notificação de reembolso",
        "notificacoes reembolsos",
        "não é você",
    ]);
    for tokenizer in every_tokenizer() {
        for text in &texts {
            assert_eq!(
                tokenizer.query_terms(text),
                tokenizer.terms(text),
                "{} on {text:?}",
                tokenizer.name()
            );
        }
    }
}

/// A pack with no stemmer and no stopwords lowercases and folds, and stems
/// nothing.
#[test]
fn a_simple_pack_lowercases_and_folds_only() {
    assert_eq!(
        from(SIMPLE).terms("The Ação Running"),
        vec!["the", "acao", "running"]
    );
}

/// The 64 byte limit reads the word the pack compares, which is the folded
/// word when the pack folds, and the lowercase word under `porter`.
#[test]
fn the_length_limit_reads_the_word_the_pack_compares() {
    let accented = "é".repeat(40);
    assert!(Tokenizer::default().terms(&accented).is_empty());
    assert_eq!(from(SIMPLE).terms(&accented), vec!["e".repeat(40)]);
    assert!(from(SIMPLE).terms(&"e".repeat(65)).is_empty());
}

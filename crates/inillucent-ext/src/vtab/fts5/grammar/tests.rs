//! Tests of the query parser: precedence, column filters, the stack limit and the messages.
//!
//! Invariant: every expectation here was read off the pinned release's own answer to the same
//! query, so a change that breaks one has moved the parser away from SQLite's.

use super::*;

/// The columns every test table has.
fn columns() -> Vec<Vec<u8>> {
    vec![b"a".to_vec(), b"b".to_vec(), b"c".to_vec()]
}

/// Parses a query against the default tokenizer and three columns.
///
/// @param text - the query
fn parse(text: &str) -> Result<Query, String> {
    let tokenizer = Tokenizer::named(&[]).expect("a known tokenizer");
    Query::parse(text.as_bytes(), &tokenizer, &columns())
        .map_err(|error| error.message().to_string())
}

/// Returns the error a query is refused with.
///
/// @param text - the query
fn refused(text: &str) -> String {
    parse(text).expect_err(text)
}

/// Writes a tree as text, one word for each phrase.
///
/// @param node - the tree
fn show(node: &Match) -> String {
    let words = |phrases: &[Phrase]| {
        phrases
            .iter()
            .map(|phrase| {
                phrase
                    .terms
                    .iter()
                    .map(|term| String::from_utf8_lossy(&term.token).into_owned())
                    .collect::<Vec<_>>()
                    .join("_")
            })
            .collect::<Vec<_>>()
            .join(" ")
    };
    match node {
        Match::Near { phrases, .. } if phrases.len() == 1 => words(phrases),
        Match::Near { phrases, .. } => format!("NEAR({})", words(phrases)),
        Match::Nothing { .. } => "NOTHING".to_string(),
        Match::And(children) => format!(
            "AND({})",
            children.iter().map(show).collect::<Vec<_>>().join(", ")
        ),
        Match::Or(children) => format!(
            "OR({})",
            children.iter().map(show).collect::<Vec<_>>().join(", ")
        ),
        Match::Not(left, right) => format!("NOT({}, {})", show(left), show(right)),
    }
}

/// Returns the tree of a query as text.
///
/// @param text - the query
fn tree(text: &str) -> String {
    show(&parse(text).expect(text).root)
}

/// `NOT` takes everything after it up to the next `AND` or `OR`, and two phrases side by side
/// bind tighter than `NOT`.
#[test]
fn not_takes_the_phrases_after_it() {
    assert_eq!(tree("p NOT q r"), "NOT(p, AND(q, r))");
    assert_eq!(tree("p q NOT q r"), "NOT(AND(p, q), AND(q, r))");
    assert_eq!(tree("p NOT q AND r"), "AND(NOT(p, q), r)");
    assert_eq!(tree("p AND q NOT r"), "AND(p, NOT(q, r))");
    assert_eq!(tree("p OR q NOT r"), "OR(p, NOT(q, r))");
    assert_eq!(tree("p NOT q NOT r"), "NOT(NOT(p, q), r)");
    assert_eq!(tree("p AND q r"), "AND(p, q, r)");
    assert_eq!(tree("p q OR r s"), "OR(AND(p, q), AND(r, s))");
}

/// A group is a whole expression, so a phrase cannot follow it.
#[test]
fn a_group_cannot_be_followed_by_a_phrase() {
    assert_eq!(refused("(a) b"), "fts5: syntax error near \"b\"");
    assert_eq!(refused("a:(b) c"), "fts5: syntax error near \"c\"");
    assert_eq!(refused("a b a:(b)"), "fts5: syntax error near \"(\"");
    assert_eq!(tree("(a) AND b"), "AND(a, b)");
}

/// A filter inside another keeps only the columns both name.
#[test]
fn nested_column_filters_intersect() {
    let query = parse("{a b}:({b c}:x)").expect("parses");
    assert_eq!(query.phrases[0].columns, ColumnFilter::only(vec![1]));
    let query = parse("-a:(-b:x)").expect("parses");
    assert_eq!(query.phrases[0].columns, ColumnFilter::only(vec![2]));
    assert_eq!(tree("a:(b:x)"), "NOTHING");
    let query = Query::parse_in(
        b"a:x OR b:x",
        &Tokenizer::named(&[]).expect("a known tokenizer"),
        &columns(),
        Some(1),
    )
    .expect("parses");
    assert_eq!(show(&query.root), "OR(NOTHING, x)");
}

/// A string with no tokens is dropped from a run of phrases and kept beside an operator.
#[test]
fn a_string_with_no_tokens_is_dropped_from_a_run() {
    assert_eq!(tree("hello \"\""), "hello");
    assert_eq!(tree("\"\" hello"), "hello");
    assert_eq!(tree("hello AND \"\""), "AND(hello, NOTHING)");
    assert_eq!(parse("hello \"\"").expect("parses").phrases.len(), 1);
    assert_eq!(parse("hello AND \"\"").expect("parses").phrases.len(), 2);
}

/// The word before a parenthesis is the name of a `NEAR`, found only when the `)` arrives.
#[test]
fn a_word_before_a_parenthesis_is_a_near_that_is_checked_late() {
    assert_eq!(refused("one (three)"), "fts5: syntax error near \"one\"");
    assert_eq!(
        refused("one two (three)"),
        "fts5: syntax error near \"two\""
    );
    assert_eq!(refused("one (three"), "fts5: syntax error near \"\"");
    assert_eq!(refused("near(one two)"), "fts5: syntax error near \"near\"");
    assert_eq!(refused("NEAR(one two, x)"), "expected integer, got \"x\"");
    assert_eq!(
        refused("NEAR(one two, \"3\")"),
        "expected integer, got \"\"3\"\""
    );
}

/// The messages name the token the parser could not take, and the first error wins.
#[test]
fn the_messages_name_the_token() {
    assert_eq!(refused(""), "fts5: syntax error near \"\"");
    assert_eq!(refused("NOT one"), "fts5: syntax error near \"NOT\"");
    assert_eq!(refused("one NOT"), "fts5: syntax error near \"\"");
    assert_eq!(refused("one)"), "fts5: syntax error near \")\"");
    assert_eq!(refused("a:b:one"), "fts5: syntax error near \":\"");
    assert_eq!(refused("{}:one"), "fts5: syntax error near \"}\"");
    assert_eq!(refused("one;two"), "fts5: syntax error near \";\"");
    assert_eq!(refused("\"one"), "unterminated string");
    assert_eq!(refused("nosuch:one ("), "no such column: nosuch");
    assert_eq!(refused("one +"), "fts5: syntax error near \"\"");
    assert_eq!(refused("one\u{b}two"), "fts5: syntax error near \"\u{b}\"");
}

/// The parser's stack holds 99 entries, so a group nested 98 deep is refused and 97 is not.
#[test]
fn the_stack_has_the_size_sqlite_gives_it() {
    let nested = |depth: usize| format!("{}x{}", "(".repeat(depth), ")".repeat(depth));
    assert!(parse(&nested(97)).is_ok());
    assert_eq!(refused(&nested(98)), "fts5: parser stack overflow");
    assert_eq!(refused(&nested(60_000)), "fts5: parser stack overflow");
    let and = |depth: usize| format!("{}x{}", "x AND (".repeat(depth), ")".repeat(depth));
    assert_eq!(parse(&and(32)).err(), None);
    assert_eq!(refused(&and(33)), "fts5: parser stack overflow");
}

/// A chain of `NOT`s deeper than 256 is refused, and a long chain of `AND`s is not.
#[test]
fn the_tree_may_be_256_deep() {
    let nots = |count: usize| format!("x{}", " NOT y".repeat(count));
    assert!(parse(&nots(255)).is_ok());
    assert_eq!(
        refused(&nots(257)),
        "fts5 expression tree is too large (maximum depth 256)"
    );
    let ands = format!("x{}", " AND y".repeat(20_000));
    assert_eq!(parse(&ands).expect("parses").phrases.len(), 20_001);
}

/// A `NEAR` keeps its phrases, and its distance is ten when none is written.
#[test]
fn near_has_its_phrases_and_distance() {
    let query = parse("NEAR(a b c, 3)").expect("parses");
    assert_eq!(query.near_groups(), vec![(0, 3, 3)]);
    assert_eq!(
        parse("NEAR(a b)").expect("parses").near_groups(),
        vec![(0, 2, 10)]
    );
    assert_eq!(tree("NEAR(a \"\" b)"), "NEAR(a b)");
}

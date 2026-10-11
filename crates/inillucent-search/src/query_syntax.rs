//! The keyword query an `inillucent_search` table accepts in `MATCH`.
//!
//! Invariant: **a query is plain words, and a query written in FTS5 operator syntax is refused
//! instead of being read as plain words.** A row scores for any query word it holds, as
//! `docs/architecture.md` says. FTS5 has a query language with `NOT`, `AND`, column filters, quoted
//! phrases and prefix stars, and none of it is implemented here. Reading `alpha NOT beta` as the
//! three words `alpha`, `not` and `beta` returned the rows that hold `beta`, which is the opposite
//! of what the person wrote, so these forms fail with the status `unsupported` and the message says
//! what to use.
//!
//! Only forms that cannot be ordinary text are refused: a double quote, a word ending in `*`, an
//! upper case `AND`, `NOT` or `NEAR` between other words, and `column:` where `column` is one of the
//! table's own columns. A colon after any other word, a question mark and a lower case `not` are
//! words a question can hold, and `search_query: ...` is the prefix an embedding model expects, so
//! none of those is refused. `OR` is the combination the table already applies, so it is allowed.

use inillucent_base::{error, DbError};

/// Returns the refusal for a query that uses FTS5 operator syntax, or nothing when it is plain
/// words.
///
/// @param query - the text bound to `MATCH`
/// @param columns - the table's column names, in declaration order
pub fn refuse_operator_syntax(query: &str, columns: &[Vec<u8>]) -> Option<DbError> {
    let found = operator_in(query, columns)?;
    Some(
        error::refusal(format!(
            "MATCH on an inillucent_search table takes plain words, and {found} is FTS5 query \
             syntax, which this table does not implement. A row scores for any word it holds. \
             Remove the operator, or use an fts5 table for operator queries"
        ))
        .with_unsupported(format!(
            "FTS5 query syntax ({found}) in MATCH on inillucent_search"
        )),
    )
}

/// Names the first operator a query uses, as a short phrase for the message.
///
/// @param query - the text bound to `MATCH`
/// @param columns - the table's column names, in declaration order
fn operator_in(query: &str, columns: &[Vec<u8>]) -> Option<String> {
    if query.contains('"') {
        return Some("a quoted phrase".to_string());
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    for (at, word) in words.iter().enumerate() {
        if word.len() > 1 && word.ends_with('*') {
            return Some(format!("the prefix star in {word}"));
        }
        let between = at > 0 && at + 1 < words.len();
        if between && matches!(*word, "AND" | "NOT" | "NEAR") {
            return Some(format!("the operator {word}"));
        }
        if let Some((name, rest)) = word.split_once(':') {
            let names_a_column = columns
                .iter()
                .any(|column| column.eq_ignore_ascii_case(name.as_bytes()));
            if names_a_column && !rest.is_empty() {
                return Some(format!("the column filter {name}:"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn columns() -> Vec<Vec<u8>> {
        vec![b"title".to_vec(), b"body".to_vec()]
    }

    #[test]
    fn the_four_forms_in_the_report_are_named() {
        for query in ["alpha NOT beta", "body:alpha", "\"a b\"", "alp*", "a AND b"] {
            assert!(operator_in(query, &columns()).is_some(), "{query}");
        }
    }

    #[test]
    fn ordinary_text_is_not_refused() {
        for query in [
            "alpha beta",
            "alpha OR beta",
            "not beta",
            "alpha and beta",
            "search_query: alpha",
            "what is alpha?",
            "NOT",
            "a * b",
            "time: 5",
        ] {
            assert_eq!(operator_in(query, &columns()), None, "{query}");
        }
    }
}

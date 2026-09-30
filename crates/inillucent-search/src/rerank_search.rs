//! Reranking inside an `inillucent_search` query.
//!
//! Invariant: **a search that names `question` returns the rows its fused search found, reordered by
//! the reranker's score, and a search that does not name it is untouched.** The reranker reads the
//! question and each candidate together, so it can tell a passage that answers the question from one
//! that only shares its words. The candidates are the top `rerank_depth` rows of the fused list. A
//! row's passage is its text columns in declaration order, joined by a newline, so a table declared
//! as `(title, body)` gives the reranker `title\nbody` and the heading reaches it. Facet columns are
//! not text and are left out.
//!
//! What a caller sees changes in three places and no others. The rows are in reranker order, ties
//! keeping the fused order. `score(t)` is the reranker's score, from 0 to 1, and `rank` is its
//! negative, as before. `confidence(t)` and `origin(t)` are the fused search's, unchanged: a
//! confidence is a statement about the keyword and vector branches, and the reranker does not move
//! it.

use inillucent_base::DbResult;
use inillucent_ext::vtab::Context;

use crate::merge::Hit;
use crate::options::Options;
use crate::store::Store;

/// Checks that a reranker can be used, without loading it.
///
/// Called before the search runs, so a machine with no reranker refuses the query with the message
/// that names the installer, even when the search would have found nothing to score.
#[cfg(feature = "embed")]
pub(crate) fn available() -> DbResult<()> {
    crate::rerank::available()
}

/// Checks that a reranker can be used. A build without embedding support cannot.
#[cfg(not(feature = "embed"))]
pub(crate) fn available() -> DbResult<()> {
    Err(crate::embed_refusal::not_built(
        "inillucent_search: question",
    ))
}

/// Scores a question against passages with the installed reranker.
///
/// @param query - the question
/// @param passages - the passages to score
#[cfg(feature = "embed")]
fn score_passages(query: &str, passages: &[&str]) -> DbResult<Vec<f32>> {
    crate::rerank::score(query, passages)
}

/// Scores a question against passages. A build without embedding support cannot.
///
/// @param _query - the question
/// @param _passages - the passages to score
#[cfg(not(feature = "embed"))]
fn score_passages(_query: &str, _passages: &[&str]) -> DbResult<Vec<f32>> {
    Err(crate::embed_refusal::not_built(
        "inillucent_search: question",
    ))
}

/// Returns the passage the reranker reads for one row: its text columns, in declaration order,
/// joined by a newline.
///
/// A column with no text is left out, so an empty title does not put a blank line in front of the
/// body. Facet columns are left out, because a facet is a filter value and not text.
///
/// @param options - the table's declaration
/// @param columns - the row's columns, in declaration order
pub fn passage_of(options: &Options, columns: &[String]) -> String {
    columns
        .iter()
        .enumerate()
        .filter(|(position, text)| !options.is_facet(*position) && !text.is_empty())
        .map(|(_, text)| text.as_str())
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Reorders the top rows of a fused search by the reranker's score.
///
/// The first `rerank_depth` rows are scored in one call, sorted by score with the fused order
/// breaking ties, and the first `k` of them are returned. A `k` larger than the depth therefore
/// returns as many rows as the reranker scored, and no row that it did not score is mixed in with a
/// score on another scale.
///
/// @param context - the running statement
/// @param store - the table's shadow tables
/// @param options - the table's declaration
/// @param question - the question in plain words
/// @param hits - the fused search's rows, best first
/// @param k - how many rows the query asked for
pub fn rerank_hits(
    context: &mut Context<'_>,
    store: &Store,
    options: &Options,
    question: &str,
    hits: Vec<Hit>,
    k: usize,
) -> DbResult<Vec<Hit>> {
    available()?;
    let depth = options.rerank_depth().min(hits.len());
    let candidates: Vec<Hit> = hits.into_iter().take(depth).collect();
    if candidates.is_empty() {
        return Ok(candidates);
    }
    let mut passages: Vec<String> = Vec::with_capacity(candidates.len());
    for hit in &candidates {
        let row = store.read_row(context, hit.id)?.unwrap_or_default();
        passages.push(passage_of(options, &row.columns));
    }
    let borrowed: Vec<&str> = passages.iter().map(String::as_str).collect();
    let scores = score_passages(question, &borrowed)?;
    Ok(order_by_score(candidates, &scores, k))
}

/// Puts hits in order of a score, highest first, and keeps the first `k`.
///
/// The sort is stable, so hits with an equal score keep the order the fused search gave them.
///
/// @param candidates - the hits, in fused order
/// @param scores - one score per hit, in the same order
/// @param k - how many to keep
pub fn order_by_score(candidates: Vec<Hit>, scores: &[f32], k: usize) -> Vec<Hit> {
    let mut scored: Vec<Hit> = candidates
        .into_iter()
        .zip(scores.iter().copied())
        .map(|(hit, score)| Hit { score, ..hit })
        .collect();
    scored.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options;
    use inillucent_core::rank::HitOrigin;

    /// Builds a hit with an id and a fused score.
    fn hit(id: i64, score: f32) -> Hit {
        Hit {
            id,
            score,
            confidence: 0.5,
            origin: HitOrigin::Both,
        }
    }

    /// A table declared `(title, body)` hands the reranker `title`, a newline and `body`, and leaves a facet out.
    #[test]
    fn the_passage_is_the_text_columns_joined_by_a_newline() {
        let declared = options::parse(&[
            b"title".to_vec(),
            b"region FACET".to_vec(),
            b"body".to_vec(),
            b"dims = 3".to_vec(),
        ])
        .expect("parsed");
        let columns = vec![
            "Cats".to_string(),
            "eu".to_string(),
            "They sleep.".to_string(),
        ];
        assert_eq!(passage_of(&declared, &columns), "Cats\nThey sleep.");
        let no_title = vec![String::new(), "eu".to_string(), "They sleep.".to_string()];
        assert_eq!(passage_of(&declared, &no_title), "They sleep.");
    }

    /// Hits come back highest reranker score first, equal scores keep the fused order, and only `k` are kept.
    #[test]
    fn the_hits_are_ordered_by_the_reranker_and_ties_keep_the_fused_order() {
        let fused = vec![hit(1, 0.9), hit(2, 0.8), hit(3, 0.7), hit(4, 0.6)];
        let ordered = order_by_score(fused, &[0.2, 0.9, 0.9, 0.5], 3);
        let ids: Vec<i64> = ordered.iter().map(|hit| hit.id).collect();
        assert_eq!(ids, vec![2, 3, 4]);
        assert_eq!(ordered.first().map(|hit| hit.score), Some(0.9));
        assert_eq!(
            ordered.first().map(|hit| hit.confidence),
            Some(0.5),
            "the confidence stays the fused one"
        );
    }
}

//! FTS3 and FTS4 `snippet()`: choosing up to four fragments of a row and writing them.
//!
//! This follows the pinned release's `sqlite3Fts3Snippet`, `fts3BestSnippet`,
//! `fts3SnippetDetails`, `fts3SnippetShift` and `fts3SnippetText` in
//! `fts3_snippet.c`. FTS5 uses a different algorithm and lives in `snippet`.
//!
//! Invariant: **for the same row, query and arguments, the text this module
//! writes is the text the pinned SQLite writes**, which the FTS3 and FTS4 cases
//! of the usage corpus grade.
//!
//! - The snippet tries one fragment, then two, three and four, and stops at the
//!   first count whose fragments cover every phrase that occurs in the row.
//! - Each fragment holds `tokens / count` tokens, rounded up, or the number of
//!   tokens named when that number is negative.
//! - A candidate window is scored by the phrase occurrences it holds. A phrase
//!   no earlier fragment covers is worth 1000 the first time it appears in the
//!   window, and every other occurrence is worth 1.
//! - The text step moves the window start forward so the matched tokens are
//!   centred, then joins the fragments with the ellipsis.
//!
//! The functions take positions and counts, so the choice can be tested without
//! a table.

use super::snippet::{Instance, Span};

/// Where one phrase ends in a column, as the phrase's position list holds it.
#[derive(Clone, Debug, Default)]
pub(super) struct PhraseHits {
    /// The token each occurrence ends at, ascending.
    pub(super) ends: Vec<i64>,
    /// How many tokens the phrase covers.
    pub(super) tokens: usize,
}

/// The phrase occurrences of one column.
#[derive(Clone, Debug)]
pub(super) struct ColumnHits {
    /// Which declared column.
    pub(super) column: usize,
    /// One entry per phrase of the query, in query order.
    pub(super) hits: Vec<PhraseHits>,
}

/// One chosen fragment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Fragment {
    /// The column the fragment is cut from.
    pub(super) column: usize,
    /// The first token of the window.
    pub(super) start: i64,
    /// Which tokens of the window to mark, bit 0 being the first token.
    pub(super) highlight: u64,
    /// Which phrases the window holds.
    pub(super) covered: u64,
}

/// The marks and separator written around the fragments.
pub(super) struct Marks<'a> {
    /// Written before a matched token.
    pub(super) open: &'a [u8],
    /// Written after a matched token.
    pub(super) close: &'a [u8],
    /// Written between fragments and where text was cut.
    pub(super) ellipsis: &'a [u8],
}

/// Builds the per phrase end positions from the occurrences found in one column.
///
/// @param instances - every occurrence in the column
/// @param phrases - how many phrases the query has
pub(super) fn hits_of(instances: &[Instance], phrases: usize) -> Vec<PhraseHits> {
    let mut hits = vec![PhraseHits::default(); phrases];
    for instance in instances {
        let Some(slot) = hits.get_mut(instance.phrase) else {
            continue;
        };
        slot.tokens = instance.tokens;
        let end = (instance.first + instance.tokens.max(1) - 1) as i64;
        slot.ends.push(end);
    }
    for slot in &mut hits {
        slot.ends.sort_unstable();
        slot.ends.dedup();
    }
    hits
}

/// Moves a list position to the first entry at or after a token.
///
/// @param ends - the phrase's end positions
/// @param at - the current entry, or `None` once the list is used up
/// @param next - the token to reach
fn advance(ends: &[i64], at: Option<usize>, next: i64) -> Option<usize> {
    let mut index = at?;
    while ends.get(index).is_some_and(|end| *end < next) {
        index += 1;
    }
    (index < ends.len()).then_some(index)
}

/// The score and masks of one candidate window.
struct Details {
    /// The window's score.
    score: i64,
    /// The phrases the window holds.
    cover: u64,
    /// The tokens of the window to mark.
    highlight: u64,
}

/// Scores the window that starts at a token, as `fts3SnippetDetails` does.
///
/// @param hits - the column's phrase occurrences
/// @param tails - for each phrase, the first entry at or after the window start
/// @param window - the first token of the window and how many tokens it holds
/// @param covered - the phrases earlier fragments already hold
fn window_details(
    hits: &[PhraseHits],
    tails: &[Option<usize>],
    window: (i64, i64),
    covered: u64,
) -> Details {
    let (start, size) = window;
    let mut found = Details {
        score: 0,
        cover: 0,
        highlight: 0,
    };
    for (index, hit) in hits.iter().enumerate() {
        let Some(first) = tails.get(index).copied().flatten() else {
            continue;
        };
        let bit = 1u64 << (index % 64);
        for end in hit.ends.iter().skip(first) {
            if *end >= start + size || *end < start {
                break;
            }
            let seen = (found.cover | covered) & bit != 0;
            found.score += if seen { 1 } else { 1000 };
            found.cover |= bit;
            let mark = 1u64 << (*end - start);
            for back in 0..(hit.tokens as i64).min(size) {
                found.highlight |= mark >> back;
            }
        }
    }
    found
}

/// Finds the best window of one column, as `fts3BestSnippet` does.
///
/// Answers the fragment, its score, and the phrases that occur in the column.
///
/// @param column - the column's phrase occurrences
/// @param size - how many tokens a window holds
/// @param covered - the phrases earlier fragments already hold
fn best_fragment(column: &ColumnHits, size: i64, covered: u64) -> (Fragment, i64, u64) {
    let hits = &column.hits;
    let mut heads: Vec<Option<usize>> = hits
        .iter()
        .map(|hit| advance(&hit.ends, (!hit.ends.is_empty()).then_some(0), size))
        .collect();
    let mut tails: Vec<Option<usize>> = hits
        .iter()
        .map(|hit| (!hit.ends.is_empty()).then_some(0))
        .collect();
    let mut seen = 0u64;
    for (index, hit) in hits.iter().enumerate() {
        if !hit.ends.is_empty() {
            seen |= 1u64 << (index % 64);
        }
    }
    let mut best = Fragment {
        column: column.column,
        ..Fragment::default()
    };
    let mut best_score = -1i64;
    let mut start = 0i64;
    loop {
        let details = window_details(hits, &tails, (start, size), covered);
        if details.score > best_score {
            best_score = details.score;
            best.start = start;
            best.highlight = details.highlight;
            best.covered = details.cover;
        }
        let Some(end) = lowest_head(hits, &heads) else {
            break;
        };
        start = end - size + 1;
        for ((hit, head), tail) in hits.iter().zip(heads.iter_mut()).zip(tails.iter_mut()) {
            *head = advance(&hit.ends, *head, end + 1);
            *tail = advance(&hit.ends, *tail, start);
        }
    }
    (best, best_score, seen)
}

/// Returns the lowest position any phrase's head points at.
///
/// @param hits - the column's phrase occurrences
/// @param heads - for each phrase, the entry its head points at
fn lowest_head(hits: &[PhraseHits], heads: &[Option<usize>]) -> Option<i64> {
    hits.iter()
        .zip(heads)
        .filter_map(|(hit, head)| head.and_then(|index| hit.ends.get(index).copied()))
        .min()
}

/// Returns how many tokens a fragment holds when the snippet has `count` fragments.
///
/// @param tokens - the token count argument, already limited to 64 either way
/// @param count - how many fragments the snippet has
fn fragment_size(tokens: i64, count: i64) -> i64 {
    if tokens >= 0 {
        (tokens + count - 1) / count
    } else {
        -tokens
    }
}

/// Picks one fragment from the best of every column considered.
///
/// @param columns - the columns considered
/// @param size - how many tokens a window holds
/// @param covered - the phrases earlier fragments already hold
/// @param seen - the phrases found in any column, added to
fn pick_fragment(columns: &[ColumnHits], size: i64, covered: u64, seen: &mut u64) -> Fragment {
    let mut chosen = Fragment::default();
    let mut chosen_score = -1i64;
    for column in columns {
        let (fragment, score, held) = best_fragment(column, size, covered);
        *seen |= held;
        if score > chosen_score {
            chosen = fragment;
            chosen_score = score;
        }
    }
    chosen
}

/// Chooses the fragments of a snippet, as `sqlite3Fts3Snippet` does.
///
/// Answers the fragments and how many tokens each holds.
///
/// @param columns - the columns considered, one when a column was named
/// @param tokens - the token count argument, already limited to 64 either way
pub(super) fn choose_fragments(columns: &[ColumnHits], tokens: i64) -> (Vec<Fragment>, i64) {
    let mut count = 1i64;
    loop {
        let size = fragment_size(tokens, count);
        let mut covered = 0u64;
        let mut seen = 0u64;
        let mut fragments = Vec::new();
        for _ in 0..count {
            let fragment = pick_fragment(columns, size, covered, &mut seen);
            covered |= fragment.covered;
            fragments.push(fragment);
        }
        if seen == covered || count == 4 {
            return (fragments, size);
        }
        count += 1;
    }
}

/// The text written so far. It stays unset until something is appended, because
/// SQLite answers NULL when nothing was.
#[derive(Default)]
pub(super) struct Buffer {
    /// The bytes, once anything was appended.
    bytes: Option<Vec<u8>>,
}

impl Buffer {
    /// Appends bytes.
    ///
    /// @param more - the bytes to add
    fn push(&mut self, more: &[u8]) {
        self.bytes
            .get_or_insert_with(Vec::new)
            .extend_from_slice(more);
    }

    /// Returns the text, or `None` when nothing was appended.
    pub(super) fn finish(self) -> Option<Vec<u8>> {
        self.bytes
    }
}

/// The column text a fragment is written from.
pub(super) struct Source<'a> {
    /// The column's bytes.
    pub(super) text: &'a [u8],
    /// The column's tokens with their byte ranges.
    pub(super) spans: &'a [Span],
}

/// Where a fragment sits in the snippet.
pub(super) struct Placement {
    /// The fragment's number, from zero.
    pub(super) index: usize,
    /// Whether it is the final fragment.
    pub(super) last: bool,
    /// How many tokens a fragment holds.
    pub(super) size: i64,
}

/// Moves the window start forward so the marked tokens are centred.
///
/// Answers the new start and the shifted mask, as `fts3SnippetShift` does.
///
/// @param fragment - the fragment chosen
/// @param size - how many tokens a window holds
/// @param remaining - how many tokens the column has from the window start
fn shifted_window(fragment: &Fragment, size: i64, remaining: i64) -> (i64, u64) {
    let mask = fragment.highlight;
    if mask == 0 {
        return (fragment.start, mask);
    }
    let left = i64::from(mask.trailing_zeros());
    let right = size - 1 - i64::from(63 - mask.leading_zeros());
    let desired = (left - right) / 2;
    if desired <= 0 {
        return (fragment.start, mask);
    }
    let shift = desired.min(remaining - size);
    if shift > 0 {
        return (fragment.start + shift, mask >> shift);
    }
    (fragment.start, mask)
}

/// Appends one fragment's text, as `fts3SnippetText` does.
///
/// @param out - the text so far
/// @param source - the column text and tokens the fragment is cut from
/// @param fragment - the fragment to write
/// @param place - where the fragment sits in the snippet
/// @param marks - the marks and the ellipsis
pub(super) fn append_fragment(
    out: &mut Buffer,
    source: &Source<'_>,
    fragment: &Fragment,
    place: &Placement,
    marks: &Marks<'_>,
) {
    let mut start = fragment.start;
    let mut mask = fragment.highlight;
    let mut shifted = false;
    let mut written = 0usize;
    for (current, (_, begin, finish)) in source.spans.iter().enumerate() {
        let current = current as i64;
        if current < start {
            continue;
        }
        if !shifted {
            shifted = true;
            let remaining = source.spans.len() as i64 - current;
            (start, mask) = shifted_window(fragment, place.size, remaining);
            if start > 0 || place.index > 0 {
                out.push(marks.ellipsis);
            } else if *begin > 0 {
                out.push(source.text.get(..*begin).unwrap_or_default());
            }
            if current < start {
                continue;
            }
        }
        if current >= start + place.size {
            if place.last {
                out.push(marks.ellipsis);
            }
            return;
        }
        let marked = mask & (1u64 << (current - start)) != 0;
        if current > start {
            out.push(source.text.get(written..*begin).unwrap_or_default());
        }
        push_token(
            out,
            source.text.get(*begin..*finish).unwrap_or_default(),
            marked,
            marks,
        );
        written = *finish;
    }
    out.push(source.text.get(written..).unwrap_or_default());
}

/// Appends one token, wrapped in the marks when it matched.
///
/// @param out - the text so far
/// @param token - the token's bytes
/// @param marked - whether the token matched
/// @param marks - the marks
fn push_token(out: &mut Buffer, token: &[u8], marked: bool, marks: &Marks<'_>) {
    if marked {
        out.push(marks.open);
    }
    out.push(token);
    if marked {
        out.push(marks.close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the hits of one single token phrase.
    fn phrase(ends: &[i64]) -> PhraseHits {
        PhraseHits {
            ends: ends.to_vec(),
            tokens: 1,
        }
    }

    /// Two phrases far apart need two fragments of half the tokens each.
    #[test]
    fn far_apart_phrases_make_two_fragments() {
        let column = ColumnHits {
            column: 0,
            hits: vec![phrase(&[3]), phrase(&[40])],
        };
        let (fragments, size) = choose_fragments(&[column], 10);
        assert_eq!(fragments.len(), 2);
        assert_eq!(size, 5);
    }

    /// Phrases inside one window make one fragment.
    #[test]
    fn near_phrases_make_one_fragment() {
        let column = ColumnHits {
            column: 0,
            hits: vec![phrase(&[3]), phrase(&[5])],
        };
        let (fragments, size) = choose_fragments(&[column], 10);
        assert_eq!(fragments.len(), 1);
        assert_eq!(size, 10);
    }

    /// A negative token count is the size of every fragment.
    #[test]
    fn a_negative_count_is_the_fragment_size() {
        assert_eq!(fragment_size(-15, 3), 15);
        assert_eq!(fragment_size(10, 3), 4);
    }
}

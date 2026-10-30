//! `highlight()` and `snippet()`: marking the matches in a column and choosing
//! the part of it to show.
//!
//! Invariant: **both follow the pinned release's own functions step by step**,
//! `fts5HighlightCb`, `fts5SnippetScore` and `fts5SnippetFunction` in
//! `fts5_aux.c`, because the text they return is compared byte for byte. The
//! choice of window has four parts, and a snippet that gets one of them wrong
//! still looks plausible:
//!
//! - Every occurrence of a phrase starts a window, centred on the occurrence.
//! - A window is scored by the phrases it shows. The first time a phrase is
//!   counted in a window it is worth 1000, and each repeat is worth 1.
//! - Every occurrence also starts a window at the start of its sentence, when
//!   the column is longer than the snippet. That window scores the same way and
//!   is worth 100 more, or 120 more when it is the start of the column. This is
//!   why a match in the first words of a long column is shown from the first
//!   word and is not centred.
//! - A window has to score strictly more than the best so far to replace it,
//!   across every column when the column argument is negative.
//!
//! The functions take positions and counts and nothing else, so the choice can
//! be tested without a table.

use super::expr::Phrase;
use super::matches_a_term;

/// A token with the byte range it covers in the column's text.
pub(super) type Span = (Vec<u8>, usize, usize);

/// One place a phrase of the query occurs in a column's text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Instance {
    /// Which phrase of the query, in the order the query was parsed.
    pub(super) phrase: usize,
    /// The token the occurrence starts at.
    pub(super) first: usize,
    /// How many tokens the occurrence covers.
    pub(super) tokens: usize,
}

/// Returns every occurrence of the query's phrases in one column's tokens.
///
/// Sorted by first token and then by phrase. A phrase whose column filter does
/// not admit the column has no occurrences in it, and an anchored phrase
/// (`^word`) only occurs at the first token.
///
/// @param phrases - the phrases of the query
/// @param spans - the column's tokens with their byte ranges
/// @param column - which column the tokens are from
pub(super) fn instances_in(phrases: &[Phrase], spans: &[Span], column: usize) -> Vec<Instance> {
    let mut found = Vec::new();
    for (index, phrase) in phrases.iter().enumerate() {
        let width = phrase.terms.len();
        if width == 0 || !phrase.columns.admits(column) {
            continue;
        }
        let mut at = 0usize;
        while at.saturating_add(width) <= spans.len() {
            if phrase.anchored && at > 0 {
                break;
            }
            let fits = phrase.terms.iter().enumerate().all(|(offset, term)| {
                spans
                    .get(at.saturating_add(offset))
                    .is_some_and(|(token, _, _)| matches_a_term(token, std::slice::from_ref(term)))
            });
            if fits {
                found.push(Instance {
                    phrase: index,
                    first: at,
                    tokens: width,
                });
            }
            at = at.saturating_add(1);
        }
    }
    found.sort_by_key(|instance| (instance.first, instance.phrase));
    found
}

/// Returns the occurrences of the query's phrases in one column, from the positions the index holds.
///
/// **This is how SQLite finds them**: the instances `highlight` and `snippet` mark are the
/// positions of the phrases that count in the row, which `matcher` works out, so a phrase in a
/// branch that did not match the row and a position outside the window of a `NEAR` are not
/// marked. Sorted by first token and then by phrase.
///
/// @param phrases - the phrases of the query
/// @param live - the positions each phrase counts at in the row
/// @param column - which column to report
pub(super) fn instances_from_positions(
    phrases: &[Phrase],
    live: &[Vec<i64>],
    column: usize,
) -> Vec<Instance> {
    let mut found = Vec::new();
    for (index, positions) in live.iter().enumerate() {
        let tokens = phrases.get(index).map_or(1, |phrase| phrase.terms.len());
        for position in positions {
            if usize::try_from(position >> 32).ok() != Some(column) {
                continue;
            }
            found.push(Instance {
                phrase: index,
                first: usize::try_from(position & 0x7fff_ffff).unwrap_or(0),
                tokens,
            });
        }
    }
    found.sort_by_key(|instance| (instance.first, instance.phrase));
    found
}

/// The most combinations of occurrences a `NEAR` group is checked against.
///
/// A group over more than this keeps every occurrence, which marks too much and
/// is better than spending the time.
const NEAR_COMBINATION_LIMIT: usize = 1_000_000;

/// Drops the occurrences of a `NEAR` group that are not part of a match.
///
/// `NEAR(a b, 3)` matches a row when an `a` and a `b` have at most three tokens
/// between them, and only those occurrences are the match: the other `a`s and
/// `b`s in the row are not marked and do not score. An occurrence is kept when
/// it is in at least one combination of one occurrence per phrase that fits.
///
/// @param instances - every occurrence in the column, sorted
/// @param groups - the first phrase, size and distance of each `NEAR` group
/// @param phrases - the phrases of the query
pub(super) fn keep_near_matches(
    instances: Vec<Instance>,
    groups: &[(usize, usize, u32)],
    phrases: &[Phrase],
) -> Vec<Instance> {
    let mut dropped: Vec<(usize, usize)> = Vec::new();
    for (first, count, distance) in groups {
        let members: Vec<usize> = (*first..first.saturating_add(*count)).collect();
        let lists: Vec<Vec<usize>> = members
            .iter()
            .map(|member| {
                instances
                    .iter()
                    .filter(|instance| instance.phrase == *member)
                    .map(|instance| instance.first)
                    .collect()
            })
            .collect();
        let lengths: Vec<usize> = members
            .iter()
            .map(|member| phrases.get(*member).map(|p| p.terms.len()).unwrap_or(1))
            .collect();
        let product = lists
            .iter()
            .fold(1usize, |all, list| all.saturating_mul(list.len().max(1)));
        if product > NEAR_COMBINATION_LIMIT {
            continue;
        }
        let kept = near_participants(&lists, &lengths, *distance as usize);
        for (index, member) in members.iter().enumerate() {
            let keeps = kept.get(index);
            for position in lists.get(index).map(Vec::as_slice).unwrap_or_default() {
                if !keeps.is_some_and(|set| set.contains(position)) {
                    dropped.push((*member, *position));
                }
            }
        }
    }
    instances
        .into_iter()
        .filter(|instance| !dropped.contains(&(instance.phrase, instance.first)))
        .collect()
}

/// Returns, for each phrase, the positions that are in some matching combination.
///
/// @param lists - where each phrase starts, one list per phrase
/// @param lengths - how many tokens each phrase covers
/// @param distance - how many tokens may lie between the phrases
fn near_participants(lists: &[Vec<usize>], lengths: &[usize], distance: usize) -> Vec<Vec<usize>> {
    let mut kept: Vec<Vec<usize>> = vec![Vec::new(); lists.len()];
    let total: usize = lengths.iter().sum();
    let mut chosen: Vec<usize> = Vec::with_capacity(lists.len());
    near_walk(lists, lengths, (total, distance), &mut chosen, &mut kept);
    kept
}

/// Tries every combination of one position per phrase, keeping those that fit.
///
/// @param lists - where each phrase starts
/// @param lengths - how many tokens each phrase covers
/// @param limits - the tokens the phrases cover in all, and the distance allowed
/// @param chosen - the positions picked so far, one per phrase already decided
/// @param kept - the positions found in a matching combination, per phrase
fn near_walk(
    lists: &[Vec<usize>],
    lengths: &[usize],
    limits: (usize, usize),
    chosen: &mut Vec<usize>,
    kept: &mut [Vec<usize>],
) {
    let depth = chosen.len();
    let Some(list) = lists.get(depth) else {
        let low = chosen.iter().copied().min().unwrap_or(0);
        let high = chosen
            .iter()
            .zip(lengths)
            .map(|(start, length)| start.saturating_add(*length))
            .max()
            .unwrap_or(0);
        if high.saturating_sub(low) <= limits.0.saturating_add(limits.1) {
            for (index, position) in chosen.iter().enumerate() {
                if let Some(set) = kept.get_mut(index) {
                    if !set.contains(position) {
                        set.push(*position);
                    }
                }
            }
        }
        return;
    };
    for position in list {
        chosen.push(*position);
        near_walk(lists, lengths, limits, chosen, kept);
        chosen.pop();
    }
}

/// Joins occurrences that share a token into one range to mark.
///
/// Each range is the first and the last token, inclusive. Two occurrences that
/// overlap are one match, as in `quick OR "quick brown"`. Two that sit side by
/// side are two, so `quick AND brown` marks the words separately.
///
/// @param instances - every occurrence in the column, sorted
pub(super) fn coalesced(instances: &[Instance]) -> Vec<(i64, i64)> {
    let mut ranges: Vec<(i64, i64)> = Vec::new();
    for instance in instances {
        let first = instance.first as i64;
        let last = first
            .saturating_add(instance.tokens as i64)
            .saturating_sub(1);
        match ranges.last_mut() {
            Some((_, held)) if first <= *held => {
                if last > *held {
                    *held = last;
                }
            }
            _ => ranges.push((first, last)),
        }
    }
    ranges
}

/// Returns the tokens that start a sentence.
///
/// The first token does, and so does a token that follows a full stop or a
/// colon and at least one space, tab or line break.
///
/// @param text - the column's bytes
/// @param spans - the column's tokens with their byte ranges
pub(super) fn sentence_starts(text: &[u8], spans: &[Span]) -> Vec<usize> {
    let mut starts = Vec::new();
    for (position, (_, start, _)) in spans.iter().enumerate() {
        if position == 0 {
            starts.push(0);
            continue;
        }
        let mut at = *start;
        let mut byte = 0u8;
        while at > 0 {
            byte = text.get(at - 1).copied().unwrap_or(0);
            if !matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
                break;
            }
            at -= 1;
        }
        let skipped_space = at != *start;
        if skipped_space && at > 0 && (byte == b'.' || byte == b':') {
            starts.push(position);
        }
    }
    starts
}

/// The best window found so far, over every column considered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Best {
    /// The column the window is in.
    pub(super) column: usize,
    /// The first token to show.
    pub(super) start: i64,
    /// The score that chose it, which a later window has to beat.
    pub(super) score: i64,
    /// How many tokens its column has, or zero before any window was chosen.
    pub(super) size: i64,
}

/// What `snippet()` knows about one column when it looks for a window.
pub(super) struct ColumnWindows<'a> {
    /// The column.
    pub(super) column: usize,
    /// Every occurrence of a phrase in it, sorted.
    pub(super) instances: &'a [Instance],
    /// The tokens that start a sentence.
    pub(super) sentences: &'a [usize],
    /// How many tokens the column has.
    pub(super) tokens: i64,
}

/// Scores the window that starts at one token, as `fts5SnippetScore` does.
///
/// Answers the score, and where to show the window from when the caller wants
/// the occurrences centred. The score counts the occurrences that start inside
/// the window: the first of a phrase counts for 1000 and each repeat for 1.
///
/// @param instances - every occurrence in the column, sorted
/// @param phrases - how many phrases the query has
/// @param at - the token the window starts at
/// @param wanted - how many tokens the snippet shows
/// @param tokens - how many tokens the column has
fn score_window(
    instances: &[Instance],
    phrases: usize,
    at: i64,
    wanted: i64,
    tokens: i64,
) -> (i64, i64) {
    let end = at.saturating_add(wanted);
    let mut seen = vec![false; phrases];
    let mut score = 0i64;
    let mut first = -1i64;
    let mut last = 0i64;
    for instance in instances {
        let offset = instance.first as i64;
        if offset < at || offset >= end {
            continue;
        }
        let repeat = seen.get(instance.phrase).copied().unwrap_or(true);
        score = score.saturating_add(if repeat { 1 } else { 1000 });
        if let Some(slot) = seen.get_mut(instance.phrase) {
            *slot = true;
        }
        if first < 0 {
            first = offset;
        }
        last = offset.saturating_add(instance.tokens as i64);
    }
    // Integer division that truncates toward zero, as C does: a match longer
    // than the window gives a negative number to halve.
    let mut centred = first.saturating_sub(wanted.saturating_sub(last.saturating_sub(first)) / 2);
    if centred.saturating_add(wanted) > tokens {
        centred = tokens.saturating_sub(wanted);
    }
    (score, centred.max(0))
}

/// Looks for a better window in one column, as `fts5SnippetFunction` does.
///
/// @param best - the best window so far, replaced by a window that beats it
/// @param column - the column's occurrences, sentence starts and size
/// @param phrases - how many phrases the query has
/// @param wanted - how many tokens the snippet shows
pub(super) fn consider_column(
    best: &mut Best,
    column: &ColumnWindows<'_>,
    phrases: usize,
    wanted: i64,
) {
    for instance in column.instances {
        let offset = instance.first as i64;
        let (score, centred) =
            score_window(column.instances, phrases, offset, wanted, column.tokens);
        if score > best.score {
            *best = Best {
                column: column.column,
                start: centred,
                score,
                size: column.tokens,
            };
        }
        if column.sentences.is_empty() || column.tokens <= wanted {
            continue;
        }
        // The last sentence start at or before the occurrence.
        let mut which = 0usize;
        while column
            .sentences
            .get(which + 1)
            .is_some_and(|next| (*next as i64) <= offset)
        {
            which += 1;
        }
        let sentence = column.sentences.get(which).copied().unwrap_or(0) as i64;
        if sentence >= offset {
            continue;
        }
        let (score, _) = score_window(column.instances, phrases, sentence, wanted, column.tokens);
        let score = score.saturating_add(if sentence == 0 { 120 } else { 100 });
        if score > best.score {
            *best = Best {
                column: column.column,
                start: sentence,
                score,
                size: column.tokens,
            };
        }
    }
}

/// The state `fts5HighlightCb` keeps while it walks a column's tokens.
struct Marker<'a> {
    /// The column's bytes.
    text: &'a [u8],
    /// The opening mark.
    open: &'a [u8],
    /// The closing mark.
    close: &'a [u8],
    /// The first token to include, when there is a range.
    range_start: i64,
    /// The last token to include, or a negative number for no range.
    range_end: i64,
    /// The ranges to mark, first and last token inclusive.
    ranges: &'a [(i64, i64)],
    /// Which range is current.
    at_range: usize,
    /// The token being read.
    position: i64,
    /// How much of the text has been copied out.
    copied: usize,
    /// Whether a mark is open.
    is_open: bool,
    /// The text so far.
    out: Vec<u8>,
}

impl Marker<'_> {
    /// Returns the current range, or -1 and -1 when there are no more.
    fn current(&self) -> (i64, i64) {
        self.ranges.get(self.at_range).copied().unwrap_or((-1, -1))
    }

    /// Copies the text from the copied point up to a byte offset.
    ///
    /// @param to - the byte offset to copy to
    fn copy_to(&mut self, to: usize) {
        let from = self.copied.min(to);
        self.out
            .extend_from_slice(self.text.get(from..to).unwrap_or_default());
    }

    /// Reads one token, writing what `fts5HighlightCb` writes for it.
    ///
    /// @param start - the byte offset the token starts at
    /// @param end - the byte offset the token ends at
    fn token(&mut self, start: usize, end: usize) {
        let position = self.position;
        self.position += 1;
        if self.range_end >= 0 {
            if position < self.range_start || position > self.range_end {
                return;
            }
            if self.range_start != 0 && position == self.range_start {
                self.copied = start;
            }
        }
        let (first, last) = self.current();
        if self.is_open && (position <= first || first < 0) && start > self.copied {
            self.out.extend_from_slice(self.close);
            self.is_open = false;
        }
        if position == first && !self.is_open {
            self.copy_to(start);
            self.out.extend_from_slice(self.open);
            self.copied = start;
            self.is_open = true;
        }
        if position == last {
            if !self.is_open {
                self.out.extend_from_slice(self.open);
                self.is_open = true;
            }
            self.copy_to(end);
            self.copied = end;
            self.at_range += 1;
        }
        if position == self.range_end {
            if self.is_open {
                let (next_first, _) = self.current();
                if next_first >= 0 && position >= next_first {
                    self.copy_to(end);
                    self.copied = end;
                }
                self.out.extend_from_slice(self.close);
                self.is_open = false;
            }
            self.copy_to(end);
            self.copied = end;
        }
    }

    /// Closes a mark that was left open.
    fn finish(&mut self) {
        if self.is_open {
            self.out.extend_from_slice(self.close);
            self.is_open = false;
        }
    }
}

/// The marks and range for one call to [`render`].
struct Marking<'a> {
    /// The ranges to mark, first and last token inclusive.
    ranges: &'a [(i64, i64)],
    /// The opening mark.
    open: &'a [u8],
    /// The closing mark.
    close: &'a [u8],
    /// The first and last token to include, or `None` for the whole column.
    window: Option<(i64, i64)>,
}

/// Walks a column's tokens and writes the marked text.
///
/// Answers the text, and how much of the column's bytes it copied, so that the
/// caller decides what follows. With a window, tokens outside it are left out.
///
/// @param text - the column's bytes
/// @param spans - the column's tokens with their byte ranges
/// @param marking - what to mark and which tokens to include
fn render(text: &[u8], spans: &[Span], marking: &Marking<'_>) -> (Vec<u8>, usize) {
    let (range_start, range_end) = marking.window.unwrap_or((0, -1));
    let mut marker = Marker {
        text,
        open: marking.open,
        close: marking.close,
        range_start,
        range_end,
        ranges: marking.ranges,
        at_range: 0,
        position: 0,
        copied: 0,
        is_open: false,
        out: Vec::with_capacity(text.len().min(512)),
    };
    // A window that starts partway in begins at the first range that does.
    while marking.window.is_some() && marker.current().0 >= 0 && marker.current().0 < range_start {
        marker.at_range += 1;
    }
    for (_, start, end) in spans {
        marker.token(*start, *end);
    }
    marker.finish();
    (marker.out, marker.copied)
}

/// Writes a column with every match marked, which is `highlight()`.
///
/// @param text - the column's bytes
/// @param spans - the column's tokens with their byte ranges
/// @param ranges - the ranges to mark
/// @param words - the opening mark and the closing mark
pub(super) fn highlight_text(
    text: &[u8],
    spans: &[Span],
    ranges: &[(i64, i64)],
    words: (&[u8], &[u8]),
) -> Vec<u8> {
    let marking = Marking {
        ranges,
        open: words.0,
        close: words.1,
        window: None,
    };
    let (mut out, copied) = render(text, spans, &marking);
    out.extend_from_slice(text.get(copied..).unwrap_or_default());
    out
}

/// Writes the window `snippet()` shows.
///
/// A window that starts after the first token begins with the ellipsis. One
/// that reaches the end of the column is followed by the rest of the text, and
/// any other is followed by the ellipsis.
///
/// @param text - the column's bytes
/// @param spans - the column's tokens with their byte ranges
/// @param ranges - the ranges to mark
/// @param window - the first token to show, how many to show, and the tokens in the column
/// @param words - the opening mark, the closing mark and the ellipsis
pub(super) fn snippet_text(
    text: &[u8],
    spans: &[Span],
    ranges: &[(i64, i64)],
    window: (i64, i64, i64),
    words: (&[u8], &[u8], &[u8]),
) -> Vec<u8> {
    let (start, wanted, tokens) = window;
    let (open, close, ellipsis) = words;
    let range_end = start.saturating_add(wanted).saturating_sub(1);
    let marking = Marking {
        ranges,
        open,
        close,
        window: Some((start, range_end)),
    };
    let mut out = Vec::new();
    if start > 0 {
        out.extend_from_slice(ellipsis);
    }
    let (body, copied) = render(text, spans, &marking);
    out.extend_from_slice(&body);
    if range_end >= tokens.saturating_sub(1) {
        out.extend_from_slice(text.get(copied..).unwrap_or_default());
    } else {
        out.extend_from_slice(ellipsis);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds the tokens of a text split on spaces, with their byte ranges.
    fn spans_of(text: &str) -> Vec<Span> {
        let mut spans = Vec::new();
        let mut at = 0usize;
        for word in text.split(' ') {
            spans.push((word.as_bytes().to_vec(), at, at + word.len()));
            at += word.len() + 1;
        }
        spans
    }

    /// Builds one occurrence of one single token phrase.
    fn at(phrase: usize, first: usize) -> Instance {
        Instance {
            phrase,
            first,
            tokens: 1,
        }
    }

    /// Returns the first token shown for a list of occurrences in one column.
    fn start(found: &[Instance], phrases: usize, wanted: i64, tokens: i64) -> i64 {
        let mut best = Best {
            column: 0,
            start: 0,
            score: 0,
            size: 0,
        };
        let column = ColumnWindows {
            column: 0,
            instances: found,
            sentences: &[0],
            tokens,
        };
        consider_column(&mut best, &column, phrases, wanted);
        best.start
    }

    /// A match in the middle of a long column is centred in the window.
    #[test]
    fn a_match_is_centred() {
        let found = [at(0, 12)];
        let starts: Vec<i64> = (2..=10)
            .map(|wanted| start(&found, 1, wanted, 20))
            .collect();
        // The values SQLite shows for a match at the thirteenth of twenty words.
        assert_eq!(starts, vec![12, 11, 11, 10, 10, 9, 9, 8, 8]);
    }

    /// A match in the first words of a longer column is shown from the start.
    ///
    /// The window that starts at the first token scores 120 more than the
    /// window centred on the match, so the match is not centred.
    #[test]
    fn a_match_in_the_first_words_is_not_centred() {
        let found = [at(0, 3)];
        assert_eq!(start(&found, 1, 6, 20), 0);
        assert_eq!(start(&found, 1, 4, 20), 0);
        assert_eq!(start(&found, 1, 3, 20), 2);
    }

    /// A column no longer than the snippet has no sentence window.
    #[test]
    fn a_short_column_is_centred_then_clamped() {
        let found = [at(0, 3)];
        // Nine tokens and a window of ten: the centred start is below zero.
        assert_eq!(start(&found, 1, 10, 9), 0);
    }

    /// A match near the end moves the window back so it stays in the column.
    #[test]
    fn the_window_stays_inside_the_column() {
        let found = [at(0, 19)];
        assert_eq!(start(&found, 1, 6, 20), 14);
    }

    /// A window that shows a second phrase beats the first window.
    #[test]
    fn a_second_phrase_moves_the_window() {
        let found = [at(0, 9), at(1, 20)];
        assert_eq!(start(&found, 2, 15, 30), 8);
    }

    /// A phrase repeated inside one window counts for little.
    #[test]
    fn repeats_count_for_one() {
        let found = [at(0, 9), at(0, 12), at(0, 13), at(0, 14), at(0, 15)];
        assert_eq!(start(&found, 1, 10, 30), 0);
    }

    /// Overlapping occurrences are one range and neighbours are two.
    ///
    /// The phrase at tokens 2 and 3 and the word at token 3 share a token, so
    /// they are one range. The word at token 4 only sits beside them.
    #[test]
    fn overlapping_occurrences_are_joined() {
        let found = [
            Instance {
                phrase: 1,
                first: 2,
                tokens: 2,
            },
            at(0, 3),
            at(0, 4),
            at(0, 6),
        ];
        assert_eq!(coalesced(&found), vec![(2, 3), (4, 4), (6, 6)]);
    }

    /// A sentence starts after a full stop and a space, or a colon and a space.
    #[test]
    fn sentences_start_after_a_stop_and_a_space() {
        let text = "one two. three four:five six.seven eight: nine";
        let spans: Vec<Span> = vec![
            (b"one".to_vec(), 0, 3),
            (b"two".to_vec(), 4, 7),
            (b"three".to_vec(), 9, 14),
            (b"four".to_vec(), 15, 19),
            (b"five".to_vec(), 20, 24),
            (b"six".to_vec(), 25, 28),
            (b"seven".to_vec(), 29, 34),
            (b"eight".to_vec(), 35, 40),
            (b"nine".to_vec(), 42, 46),
        ];
        assert_eq!(sentence_starts(text.as_bytes(), &spans), vec![0, 2, 8]);
    }

    /// Highlight wraps each range and keeps the text between them.
    #[test]
    fn highlight_wraps_each_range() {
        let text = "the quick brown fox";
        let spans = spans_of(text);
        let out = highlight_text(text.as_bytes(), &spans, &[(1, 2)], (b"[", b"]"));
        assert_eq!(String::from_utf8_lossy(&out), "the [quick brown] fox");
    }

    /// A snippet that starts partway in begins with the ellipsis and ends with it.
    #[test]
    fn a_snippet_is_cut_with_ellipses() {
        let text = "a b c d e f g h i j";
        let spans = spans_of(text);
        let words: (&[u8], &[u8], &[u8]) = (b"[", b"]", b"..");
        let out = snippet_text(text.as_bytes(), &spans, &[(5, 5)], (3, 5, 10), words);
        assert_eq!(String::from_utf8_lossy(&out), "..d e [f] g h..");
        let tail = snippet_text(text.as_bytes(), &spans, &[(9, 9)], (5, 5, 10), words);
        assert_eq!(String::from_utf8_lossy(&tail), "..f g h i [j]");
    }
}

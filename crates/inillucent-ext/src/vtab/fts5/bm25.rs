//! `bm25()`: the score FTS5 ranks by.
//!
//! Invariant: the formula is the pinned release's, constants and sign included.
//! FTS5 returns a *negative* score, so that `ORDER BY rank` - which is
//! ascending - puts the best match first, and an application that sorted by the
//! absolute value would get the worst matches. That sign is the single most
//! surprising thing about the function and the reason it is written down here.
//!
//! The formula, from `fts5_aux.c`:
//!
//! ```text
//! idf(q)   = log( (N - n(q) + 0.5) / (n(q) + 0.5) )        -- clamped at 1e-6
//! f(q)     = SUM over columns c of w(c) * f(q,c)
//! score    = - SUM over phrases q of
//!              idf(q) * f(q) * (k1 + 1) / (f(q) + k1 * (1 - b + b * D/avgdl))
//! ```
//!
//! with `k1 = 1.2` and `b = 0.75`, `N` the number of rows, `n(q)` the number of
//! rows the phrase appears in, `f(q,c)` how often it appears in column `c` of
//! this row, `D` the row's length and `avgdl` the average length. `w(c)` is the
//! per-column weight an application may pass, and is one by default.
//!
//! **The columns are summed before the saturation, not after (task-1979, R3).**
//! This used to saturate each column separately and add the results, which is a
//! different function wherever a phrase is in more than one column of a row:
//! `fts5Bm25Function` accumulates `aFreq[phrase] += weight` once per *instance*
//! across every column and divides once. With every weight at one the two agree
//! only when each row holds the phrase in a single column, which is what the
//! four row corpus the old differential test used happened to be - so the test
//! claimed agreement "to the last digit" and could not have seen the
//! difference. Passing weights makes them disagree on every row, because a
//! weight inside a saturating quotient does not scale the result.

use inillucent_base::DbResult;
use inillucent_value::Value;

use super::expr::Query;
use super::matcher::{Evaluated, Hits, Position};
use super::{decode_sizes, Totals};
use crate::shadow::ShadowTables;
use crate::vtab::Context;

/// The term-frequency saturation constant.
const K1: f64 = 1.2;
/// The length-normalisation constant.
const B: f64 = 0.75;

/// Returns the column weights the `rank` column scores with.
///
/// **The `rank` setting is read, not only kept.** `INSERT INTO t(t, rank)
/// VALUES('rank', 'bm25(10.0, 1.0)')` is how an application weights its
/// columns for every query, and it was stored in `%_config` and then ignored:
/// `rank` kept scoring every column at one, so the order a search returned
/// was not the order the application asked for. A setting that names another
/// function, or that does not parse, leaves every weight at one.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param columns - how many columns the table declares
pub fn configured_weights(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    columns: usize,
) -> DbResult<Vec<f64>> {
    let key = [inillucent_value::Value::owned_text(b"rank")?];
    let Some(row) = shadows.read_keyed(context, b"config", &key, 2)? else {
        return Ok(vec![1.0f64; columns]);
    };
    let text = match row.get(1) {
        Some(inillucent_value::Value::Text(text)) => text.utf8_bytes().into_owned(),
        _ => return Ok(vec![1.0f64; columns]),
    };
    Ok(parse_rank(Some(&text))
        .map(|function| function.column_weights(columns))
        .unwrap_or_else(|_| vec![1.0f64; columns]))
}

/// A rank function as `rank MATCH '...'` or the `rank` setting names it: `bm25(10.0, 1.0)`.
#[derive(Clone, Debug, PartialEq)]
pub struct RankFunction {
    /// The function's name as written.
    pub name: String,
    /// Each argument as `sqlite3_value_double` reads it, which is a column's weight.
    pub weights: Vec<f64>,
}

impl RankFunction {
    /// Returns the weight of each of a table's columns, one for a column the call gave none.
    ///
    /// @param columns - how many columns the table declares
    pub fn column_weights(&self, columns: usize) -> Vec<f64> {
        (0..columns)
            .map(|column| self.weights.get(column).copied().unwrap_or(1.0))
            .collect()
    }
}

/// Reads a rank function specification the way `sqlite3Fts5ConfigParseRank` does.
///
/// The specification is a word, an opening parenthesis and a comma separated list of literals
/// closed by a parenthesis, and anything after that parenthesis is ignored. A literal is `NULL`, a
/// hex blob, a string in single quotes or a number written as digits with an optional sign and an
/// optional fraction, so `bm25(1e1)`, `bm25(a)` and `bm25(0x10)` are refused. The refusal is
/// `parse error in rank function: ` and the text. A NULL specification is refused the same way.
///
/// @param text - the specification, or `None` for NULL
pub fn parse_rank(text: Option<&[u8]>) -> Result<RankFunction, String> {
    let refused = |text: Option<&[u8]>| {
        format!(
            "parse error in rank function: {}",
            String::from_utf8_lossy(text.unwrap_or_default())
        )
    };
    let Some(bytes) = text else {
        return Err(refused(None));
    };
    read_rank(bytes).ok_or_else(|| refused(Some(bytes)))
}

/// Reads the name and the literals of a rank function, or `None` when they do not parse.
///
/// @param bytes - the specification
fn read_rank(bytes: &[u8]) -> Option<RankFunction> {
    let mut at = skip_spaces(bytes, 0);
    let start = at;
    while bytes.get(at).is_some_and(|byte| {
        *byte >= 0x80 || byte.is_ascii_alphanumeric() || matches!(byte, b'_' | 0x1A)
    }) {
        at += 1;
    }
    if at == start {
        return None;
    }
    let name = String::from_utf8_lossy(bytes.get(start..at)?).into_owned();
    at = skip_spaces(bytes, at);
    if bytes.get(at) != Some(&b'(') {
        return None;
    }
    at = skip_spaces(bytes, at + 1);
    let mut weights = Vec::new();
    if bytes.get(at) != Some(&b')') {
        read_literals(bytes, at, &mut weights)?;
    }
    Some(RankFunction { name, weights })
}

/// Skips spaces, the only white space a rank specification allows.
///
/// @param bytes - the specification
/// @param at - where to start
fn skip_spaces(bytes: &[u8], mut at: usize) -> usize {
    while bytes.get(at) == Some(&b' ') {
        at += 1;
    }
    at
}

/// Reads a comma separated list of literals that ends at a parenthesis.
///
/// @param bytes - the specification
/// @param from - where the first literal starts
/// @param weights - where each literal's value as a double is added
fn read_literals(bytes: &[u8], from: usize, weights: &mut Vec<f64>) -> Option<()> {
    let mut at = from;
    loop {
        at = skip_spaces(bytes, at);
        let (end, value) = read_literal(bytes, at)?;
        weights.push(value);
        at = skip_spaces(bytes, end);
        match bytes.get(at) {
            Some(b')') => return Some(()),
            Some(b',') => at += 1,
            _ => return None,
        }
    }
}

/// Reads one literal and returns where it ends and its value as a double.
///
/// @param bytes - the specification
/// @param at - where the literal starts
fn read_literal(bytes: &[u8], at: usize) -> Option<(usize, f64)> {
    match bytes.get(at)? {
        b'n' | b'N' => {
            let word = bytes.get(at..at + 4)?;
            word.eq_ignore_ascii_case(b"null").then_some((at + 4, 0.0))
        }
        b'x' | b'X' => read_blob_literal(bytes, at),
        b'\'' => read_string_literal(bytes, at),
        _ => read_number_literal(bytes, at),
    }
}

/// Reads `x'..'`, whose value as a double is that of the text its bytes spell.
///
/// @param bytes - the specification
/// @param at - the position of the `x`
fn read_blob_literal(bytes: &[u8], at: usize) -> Option<(usize, f64)> {
    if bytes.get(at + 1) != Some(&b'\'') {
        return None;
    }
    let mut end = at + 2;
    while bytes.get(end).is_some_and(u8::is_ascii_hexdigit) {
        end += 1;
    }
    if bytes.get(end) != Some(&b'\'') || !(end - at).is_multiple_of(2) {
        return None;
    }
    let digits = bytes.get(at + 2..end)?;
    let decoded: Vec<u8> = digits
        .chunks(2)
        .filter_map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
        .collect();
    Some((
        end + 1,
        super::query::leading_number(&String::from_utf8_lossy(&decoded)),
    ))
}

/// Reads a string in single quotes, where two quotes stand for one.
///
/// @param bytes - the specification
/// @param at - the position of the opening quote
fn read_string_literal(bytes: &[u8], at: usize) -> Option<(usize, f64)> {
    let mut end = at + 1;
    let mut content = Vec::new();
    loop {
        match bytes.get(end)? {
            b'\'' if bytes.get(end + 1) == Some(&b'\'') => {
                content.push(b'\'');
                end += 2;
            }
            b'\'' => break,
            byte => {
                content.push(*byte);
                end += 1;
            }
        }
    }
    Some((
        end + 1,
        super::query::leading_number(&String::from_utf8_lossy(&content)),
    ))
}

/// Reads a number written as a sign, digits, and a fraction that has digits.
///
/// @param bytes - the specification
/// @param at - where the number starts
fn read_number_literal(bytes: &[u8], at: usize) -> Option<(usize, f64)> {
    let mut end = at;
    if matches!(bytes.get(end), Some(b'+' | b'-')) {
        end += 1;
    }
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1).is_some_and(u8::is_ascii_digit) {
        end += 2;
        while bytes.get(end).is_some_and(u8::is_ascii_digit) {
            end += 1;
        }
    }
    if end == at {
        return None;
    }
    let text = String::from_utf8_lossy(bytes.get(at..end)?).into_owned();
    Some((end, text.parse::<f64>().unwrap_or(0.0)))
}

/// What a query's score needs from the whole table, worked out once for the query.
///
/// SQLite calculates these the first time `bm25` is called for a query and keeps them, so a row
/// costs the arithmetic and a read of its own size and nothing that grows with the rows the query
/// matched. `idf` has one entry per phrase.
#[derive(Clone, Debug, Default)]
pub struct Statistics {
    /// The inverse document frequency of each phrase.
    idf: Vec<f64>,
    /// How many tokens a row holds on average.
    average: f64,
}

/// Works out the numbers a query's score is relative to.
///
/// The frequency of a phrase is the number of rows it matches on its own, which is the size of
/// its hit map. An idf that is not above zero is `1e-6`, and one above zero is kept however small
/// it is, as `fts5Bm25GetData` does.
///
/// @param hits - where each phrase was found
/// @param totals - the collection totals
pub fn statistics(hits: &[Hits], totals: &Totals) -> Statistics {
    let rows = totals.rows as f64;
    let total_tokens: i64 = totals.tokens.iter().sum();
    let average = if totals.rows > 0 {
        total_tokens as f64 / totals.rows as f64
    } else {
        0.0
    };
    let idf = hits
        .iter()
        .map(|found| {
            let matching = found.len() as f64;
            let idf = ((rows - matching + 0.5) / (matching + 0.5)).ln();
            if idf <= 0.0 {
                1e-6
            } else {
                idf
            }
        })
        .collect();
    Statistics { idf, average }
}

/// Scores every matched row, returning `(rowid, score)` pairs.
///
/// @param evaluated - the rows the query matched and the hits of its phrases
/// @param query - the parsed query
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param totals - the collection totals
/// @param columns - how many columns the table declares
/// @param weights - the weight of each column
pub fn score(
    evaluated: &Evaluated,
    query: &Query,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    totals: &Totals,
    columns: usize,
    weights: &[f64],
) -> DbResult<Vec<(i64, f64)>> {
    let stats = statistics(&evaluated.hits, totals);
    let mut out = Vec::with_capacity(evaluated.rows.len());
    for rowid in &evaluated.rows {
        let sizes = row_sizes(context, shadows, *rowid, columns)?;
        let live = evaluated.live_positions(&query.root, *rowid);
        out.push((*rowid, score_row(&stats, &live, &sizes, weights)));
    }
    Ok(out)
}

/// Returns how many tokens each column of one row holds.
pub fn row_sizes(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    rowid: i64,
    columns: usize,
) -> DbResult<Vec<i64>> {
    let Some(row) = shadows.read_row(context, b"docsize", rowid)? else {
        return Ok(vec![0; columns]);
    };
    Ok(row
        .get(1)
        .and_then(Value::as_blob)
        .map(|blob| decode_sizes(blob.raw(), columns))
        .unwrap_or_else(|| vec![0; columns]))
}

/// Scores one row against every phrase of a query.
///
/// **The arithmetic is `fts5Bm25Function`'s, in the same order.** The length term is
/// `(1 - b) + b * D / avgdl`, which C evaluates as `b * D` first and then divides; writing it as
/// `b * (D / avgdl)` rounds differently and moved the last two or three digits of about one score
/// in six.
///
/// @param stats - the numbers the query's score is relative to
/// @param live - the positions each phrase counts at in this row
/// @param sizes - how many tokens each column of the row holds
/// @param weights - the weight of each column
pub fn score_row(
    stats: &Statistics,
    live: &[Vec<Position>],
    sizes: &[i64],
    weights: &[f64],
) -> f64 {
    let length = sizes.iter().sum::<i64>() as f64;
    let mut score = 0.0f64;
    for (index, idf) in stats.idf.iter().enumerate() {
        let mut frequency = 0.0f64;
        for position in live.get(index).map(Vec::as_slice).unwrap_or_default() {
            let column = usize::try_from(position >> 32).unwrap_or(usize::MAX);
            frequency += weights.get(column).copied().unwrap_or(1.0);
        }
        let normalised = (1.0 - B) + B * length / stats.average;
        score += idf * ((frequency * (K1 + 1.0)) / (frequency + K1 * normalised));
    }
    -score
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The numbers a score is relative to, for a table of `rows` rows and `tokens` tokens.
    fn statistics_of(rows: i64, tokens: i64, matching: usize) -> Statistics {
        let mut found = Hits::new();
        for row in 0..matching {
            found.insert(row as i64, vec![(0, vec![0])]);
        }
        statistics(
            &[found],
            &Totals {
                rows,
                tokens: vec![tokens],
            },
        )
    }

    /// The score is negative, so that ascending order is best first.
    #[test]
    fn the_score_is_negative() {
        let stats = statistics_of(10, 100, 1);
        assert!(score_row(&stats, &[vec![0]], &[10], &[1.0]) < 0.0);
    }

    /// A row where the phrase appears more often scores better.
    #[test]
    fn more_occurrences_score_better() {
        let stats = statistics_of(100, 1000, 3);
        let once = score_row(&stats, &[vec![0]], &[10], &[1.0]);
        let often = score_row(&stats, &[vec![0, 1, 2, 3, 4]], &[10], &[1.0]);
        assert!(often < once, "{often} should beat {once}");
    }

    /// A shorter row scores better for the same number of occurrences.
    #[test]
    fn a_shorter_row_scores_better() {
        let stats = statistics_of(100, 1000, 3);
        let long = score_row(&stats, &[vec![0, 1]], &[50], &[1.0]);
        let short = score_row(&stats, &[vec![0, 1]], &[5], &[1.0]);
        assert!(short < long, "{short} should beat {long}");
    }

    /// A row the phrase does not count in scores zero rather than something.
    #[test]
    fn an_unmatched_row_scores_nothing() {
        let stats = statistics_of(10, 100, 3);
        assert_eq!(score_row(&stats, &[Vec::new()], &[10], &[1.0]), 0.0);
    }

    /// The length term is `(1 - b) + b * D / avgdl`, and `b * (D / avgdl)` is a different number.
    #[test]
    fn the_length_term_is_evaluated_in_sqlites_order() {
        let stats = statistics_of(7, 100, 2);
        let length = 13i64;
        let expected = {
            let average = 100.0f64 / 7.0;
            let normalised = (1.0 - 0.75) + 0.75 * length as f64 / average;
            let idf = ((7.0f64 - 2.0 + 0.5) / (2.0 + 0.5)).ln();
            -(idf * ((2.0 * (1.2 + 1.0)) / (2.0 + 1.2 * normalised)))
        };
        assert_eq!(
            score_row(&stats, &[vec![0, 1]], &[length], &[1.0]).to_bits(),
            expected.to_bits()
        );
    }

    /// An idf that is not above zero is `1e-6`, and a tiny positive one is kept.
    #[test]
    fn a_common_phrase_has_the_smallest_idf() {
        let stats = statistics_of(10, 100, 9);
        assert_eq!(stats.idf, vec![1e-6]);
    }

    /// A rank function is a word and a list of literals, and the weights are what the literals read as.
    #[test]
    fn a_rank_function_is_read_like_sqlite_reads_it() {
        let parsed = parse_rank(Some(b"  bm25 ( 1 , 2.5, '3', null, x'31' )")).expect("parses");
        assert_eq!(parsed.name, "bm25");
        assert_eq!(parsed.weights, vec![1.0, 2.5, 3.0, 0.0, 1.0]);
        assert_eq!(
            parse_rank(Some(b"bm25(1) rest")).expect("parses").weights,
            vec![1.0]
        );
        for text in [
            "bm25(1e1)",
            "bm25(a)",
            "bm25(0x10)",
            "bm25(",
            "bm25(1,)",
            "(1)",
            "bm25 1",
        ] {
            let said = parse_rank(Some(text.as_bytes())).expect_err(text);
            assert_eq!(said, format!("parse error in rank function: {text}"));
        }
        assert_eq!(
            parse_rank(None).expect_err("NULL"),
            "parse error in rank function: "
        );
    }
}

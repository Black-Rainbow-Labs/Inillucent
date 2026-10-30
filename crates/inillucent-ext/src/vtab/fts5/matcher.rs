//! What a parsed query matches: the rows, and in each row the positions every phrase was found at.
//!
//! Invariant: **a row is a match, and a phrase counts in that row, exactly as SQLite's
//! `fts5_expr.c` decides them.** Three decisions are easy to get slightly wrong and each is
//! visible in a score or in a highlighted word:
//!
//! - `NOT` takes everything written after it up to the next `AND` or `OR` as its right side, and
//!   the right side's phrases never count in a row, because the row is a match only when the
//!   right side did not match it.
//! - A phrase counts in a row only while the node holding it is part of that row's match. A
//!   branch of an `OR` that did not match the row, and an `AND` that failed, contribute none of
//!   their phrases, even when one of the phrases is in the row.
//! - A `NEAR` keeps of each phrase only the positions that are inside a matching window, found
//!   by [`trim_near`], which is `fts5ExprNearIsMatch`'s sweep. It does not look for the closest
//!   pair. The sweep is also what decides a `NEAR` of three or more phrases.
//!
//! The rows of a query are found from the doclists once ([`evaluate`]). What a row holds is then
//! worked out for that row alone ([`Evaluated::live_positions`]), so scoring a row costs the
//! phrases of the query and not the rows the query matched.

use std::collections::BTreeMap;

use inillucent_base::DbResult;

use super::expr::{Match, Phrase, Query, Term};
use super::DocEntry;
use crate::shadow::ShadowTables;
use crate::vtab::Context;

/// Where one phrase was found: a row, and the positions per column.
pub type Hits = BTreeMap<i64, Vec<(usize, Vec<u32>)>>;

/// A position in the form the pinned release keeps it: the column in the high half, the offset in the low.
pub type Position = i64;

/// The rows a query matched, and where each phrase was found.
#[derive(Clone, Debug, Default)]
pub struct Evaluated {
    /// The rows the query matched, ascending.
    pub rows: Vec<i64>,
    /// Where each phrase was found, in the order the phrases were written.
    pub hits: Vec<Hits>,
    /// For each `NEAR` of more than one phrase, the number of its first phrase and the rows in
    /// which every one of its terms occurs. See [`Evaluated::live_positions`].
    pub cooccur: Vec<(usize, Vec<i64>)>,
}

/// Runs one query and returns the rows it matched with the phrase hits.
///
/// @param query - the parsed query
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param columns - how many columns the table declares
pub fn evaluate(
    query: &Query,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    columns: usize,
) -> DbResult<Evaluated> {
    // The segment list is read once for the query, so every term is looked up in the same set.
    let structure = super::segment::read_structure(context, shadows)?;
    let mut hits = Vec::with_capacity(query.phrases.len());
    for phrase in &query.phrases {
        hits.push(phrase_hits(
            phrase, context, shadows, buffer, &structure, columns,
        )?);
    }
    let rows = rows_of(&query.root, &|leaf| leaf_rows(leaf, &hits));
    let mut cooccur = Vec::new();
    cooccurring_rows(
        &query.root,
        context,
        shadows,
        buffer,
        &structure,
        columns,
        &mut cooccur,
    )?;
    Ok(Evaluated {
        rows,
        hits,
        cooccur,
    })
}

/// Runs one query and returns only the rows it matched.
///
/// **Positions are decoded when something needs them, and most queries do not.** A doclist
/// entry carries a position list per column, and building the [`Hits`] map allocates one vector
/// per row per column and inserts every row into a `BTreeMap` - work that
/// `SELECT count(*) FROM t WHERE t MATCH 'x'` throws away. Over five hundred documents that was
/// seventy per cent of the query.
///
/// Answers `None` for the queries whose *answer* depends on positions, and the caller runs
/// [`evaluate`] for those instead: a `NEAR`, and a phrase of more than one term, where adjacency
/// is the question, and an anchored phrase, which is decided by where its term sits. Scoring
/// needs positions too, so a ranked plan does not come here.
///
/// @param query - the parsed query
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param columns - how many columns the table declares
pub fn evaluate_rows(
    query: &Query,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    columns: usize,
) -> DbResult<Option<Vec<i64>>> {
    let needs_positions = query
        .phrases
        .iter()
        .any(|phrase| phrase.terms.len() != 1 || phrase.anchored)
        || has_near(&query.root);
    if needs_positions {
        return Ok(None);
    }
    let structure = super::segment::read_structure(context, shadows)?;
    let mut per_phrase = Vec::with_capacity(query.phrases.len());
    for phrase in &query.phrases {
        per_phrase.push(phrase_rows(
            phrase, context, shadows, buffer, &structure, columns,
        )?);
    }
    Ok(Some(rows_of(&query.root, &|leaf| match leaf {
        Match::Near { first, .. } => per_phrase.get(*first).cloned().unwrap_or_default(),
        _ => Vec::new(),
    })))
}

/// Returns whether a tree holds a `NEAR` of more than one phrase.
///
/// @param node - the tree
fn has_near(node: &Match) -> bool {
    match node {
        Match::Near { phrases, .. } => phrases.len() > 1,
        Match::Nothing { .. } => false,
        Match::And(children) | Match::Or(children) => children.iter().any(has_near),
        Match::Not(left, right) => has_near(left) || has_near(right),
    }
}

/// Returns the rows a tree matches, given how to find the rows of one leaf.
///
/// The rows of every node are ascending and name each row once, so the three operators are
/// merges of two sorted lists. They were searches of one list for each row of the other, which
/// made a query of two common words cost the square of the rows each matched.
///
/// @param node - the tree
/// @param leaf - the rows one leaf matches
fn rows_of(node: &Match, leaf: &dyn Fn(&Match) -> Vec<i64>) -> Vec<i64> {
    match node {
        Match::Near { .. } => leaf(node),
        Match::Nothing { .. } => Vec::new(),
        Match::And(children) => {
            let mut rows: Option<Vec<i64>> = None;
            for child in children {
                let next = rows_of(child, leaf);
                rows = Some(match rows {
                    None => next,
                    Some(held) => intersect_sorted(&held, &next),
                });
                if rows.as_ref().is_some_and(Vec::is_empty) {
                    break;
                }
            }
            rows.unwrap_or_default()
        }
        Match::Or(children) => children.iter().fold(Vec::new(), |held, child| {
            union_sorted(&held, &rows_of(child, leaf))
        }),
        Match::Not(left, right) => difference_sorted(&rows_of(left, leaf), &rows_of(right, leaf)),
    }
}

/// Returns the rows a leaf matches, from the hits of its phrases.
///
/// A plain phrase matches the rows it has hits in. A `NEAR` matches the rows where every phrase
/// has hits and [`trim_near`] finds a window.
///
/// @param leaf - a `Near` node
/// @param hits - where each phrase was found
fn leaf_rows(leaf: &Match, hits: &[Hits]) -> Vec<i64> {
    let Match::Near { first, phrases, .. } = leaf else {
        return Vec::new();
    };
    let Some(head) = hits.get(*first) else {
        return Vec::new();
    };
    let mut rows: Vec<i64> = head.keys().copied().collect();
    for offset in 1..phrases.len() {
        let Some(next) = hits.get(first + offset) else {
            return Vec::new();
        };
        rows.retain(|row| next.contains_key(row));
    }
    if phrases.len() > 1 {
        rows.retain(|row| near_lists(leaf, hits, *row).is_some());
    }
    rows
}

/// Returns the rows in both of two ascending lists.
///
/// @param left - one list
/// @param right - the other
fn intersect_sorted(left: &[i64], right: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(left.len().min(right.len()));
    let (mut a, mut b) = (0usize, 0usize);
    while let (Some(x), Some(y)) = (left.get(a), right.get(b)) {
        match x.cmp(y) {
            std::cmp::Ordering::Less => a += 1,
            std::cmp::Ordering::Greater => b += 1,
            std::cmp::Ordering::Equal => {
                out.push(*x);
                a += 1;
                b += 1;
            }
        }
    }
    out
}

/// Returns the rows in either of two ascending lists, once each.
///
/// @param left - one list
/// @param right - the other
fn union_sorted(left: &[i64], right: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(left.len() + right.len());
    let (mut a, mut b) = (0usize, 0usize);
    loop {
        match (left.get(a), right.get(b)) {
            (Some(x), Some(y)) if x < y => {
                out.push(*x);
                a += 1;
            }
            (Some(x), Some(y)) if x > y => {
                out.push(*y);
                b += 1;
            }
            (Some(x), Some(_)) => {
                out.push(*x);
                a += 1;
                b += 1;
            }
            (Some(x), None) => {
                out.push(*x);
                a += 1;
            }
            (None, Some(y)) => {
                out.push(*y);
                b += 1;
            }
            (None, None) => return out,
        }
    }
}

/// Returns the rows of one ascending list that are not in the other.
///
/// @param left - the rows to keep from
/// @param right - the rows to leave out
fn difference_sorted(left: &[i64], right: &[i64]) -> Vec<i64> {
    let mut out = Vec::with_capacity(left.len());
    let mut b = 0usize;
    for row in left {
        while right.get(b).is_some_and(|other| other < row) {
            b += 1;
        }
        if right.get(b) != Some(row) {
            out.push(*row);
        }
    }
    out
}

/// Flattens one row's hits of a phrase into positions, the column in the high half.
///
/// @param found - the row's hits: one list of offsets per column
fn flatten(found: &[(usize, Vec<u32>)]) -> Vec<Position> {
    let mut out: Vec<Position> = found
        .iter()
        .flat_map(|(column, offsets)| {
            offsets
                .iter()
                .map(move |offset| ((*column as i64) << 32) | i64::from(*offset))
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Returns, for a `NEAR` leaf and a row, the positions of each phrase that are inside a window.
///
/// `None` when a phrase has no hits in the row or no window covers every phrase.
///
/// @param leaf - a `Near` node
/// @param hits - where each phrase was found
/// @param row - the row
fn near_lists(leaf: &Match, hits: &[Hits], row: i64) -> Option<Vec<Vec<Position>>> {
    let Match::Near {
        first,
        phrases,
        distance,
    } = leaf
    else {
        return None;
    };
    let mut lists = Vec::with_capacity(phrases.len());
    for offset in 0..phrases.len() {
        let found = hits.get(first + offset)?.get(&row)?;
        lists.push(flatten(found));
    }
    let widths: Vec<i64> = phrases
        .iter()
        .map(|phrase| phrase.terms.len() as i64)
        .collect();
    trim_near(&lists, &widths, i64::from(*distance))
}

/// One phrase's list of positions, read with one position of lookahead.
struct Reader<'a> {
    list: &'a [Position],
    at: usize,
}

/// The position that stands for the end of a list.
const END: Position = 1 << 62;

impl Reader<'_> {
    /// Returns the position the reader is on.
    fn current(&self) -> Position {
        self.list.get(self.at).copied().unwrap_or(END)
    }

    /// Returns the position after the one the reader is on.
    fn lookahead(&self) -> Position {
        self.list.get(self.at + 1).copied().unwrap_or(END)
    }

    /// Moves on, and returns whether the list has ended.
    fn advance(&mut self) -> bool {
        self.at += 1;
        self.current() == END
    }
}

/// Finds the positions of each phrase that are part of a match of a `NEAR`, as
/// `fts5ExprNearIsMatch` does.
///
/// The sweep keeps `top`, the furthest position any phrase has been moved to, and moves every
/// phrase up to `top - width - distance`, where `width` is the number of terms of that phrase.
/// When every phrase is then within `top`, one position of each is part of a match, and the
/// phrase with the nearest next position moves on. The positions kept are the ones the sweep
/// stopped at, which are not every position inside some window.
///
/// @param lists - each phrase's positions in the row, ascending
/// @param widths - how many terms each phrase has
/// @param distance - how many tokens may lie between the phrases
pub(super) fn trim_near(
    lists: &[Vec<Position>],
    widths: &[i64],
    distance: i64,
) -> Option<Vec<Vec<Position>>> {
    let mut readers: Vec<Reader<'_>> = lists.iter().map(|list| Reader { list, at: 0 }).collect();
    let mut kept: Vec<Vec<Position>> = vec![Vec::new(); lists.len()];
    'sweep: loop {
        let mut top = readers.first()?.current();
        loop {
            let mut together = true;
            for (index, reader) in readers.iter_mut().enumerate() {
                let floor = top - widths.get(index).copied().unwrap_or(1) - distance;
                if reader.current() >= floor && reader.current() <= top {
                    continue;
                }
                together = false;
                while reader.current() < floor {
                    if reader.advance() {
                        break 'sweep;
                    }
                }
                top = top.max(reader.current());
            }
            if together {
                break;
            }
        }
        for (reader, out) in readers.iter().zip(kept.iter_mut()) {
            if out.last() != Some(&reader.current()) {
                out.push(reader.current());
            }
        }
        let mut next = 0usize;
        let mut nearest = readers.first()?.lookahead();
        for (index, reader) in readers.iter().enumerate() {
            if reader.lookahead() < nearest {
                nearest = reader.lookahead();
                next = index;
            }
        }
        if readers.get_mut(next)?.advance() {
            break;
        }
    }
    kept.first()
        .is_some_and(|first| !first.is_empty())
        .then_some(kept)
}

/// How a node stands at one row: whether SQLite's iterator for the node is on that row, and whether it
/// is a match there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Standing {
    /// The node is not on the row.
    Away,
    /// The node is on the row and matches it.
    Match,
    /// The node is on the row without matching it: every term of a `NEAR` is in the row but the
    /// phrases are not near enough, which SQLite calls `bNomatch`.
    Near,
}

impl Evaluated {
    /// Returns the rows in which every term of a `NEAR` leaf occurs, if the leaf is one.
    ///
    /// @param first - the number of the leaf's first phrase
    fn cooccurring(&self, first: usize) -> &[i64] {
        self.cooccur
            .iter()
            .find(|(at, _)| *at == first)
            .map(|(_, rows)| rows.as_slice())
            .unwrap_or_default()
    }

    /// Returns where a node stands at a row.
    ///
    /// **A `NEAR` is on a row when all its terms are in it, whether or not the phrases are near
    /// each other.** SQLite finds the rows of a `NEAR` by advancing one iterator per term to a row
    /// they share, and only then asks whether the phrases are close enough, so a `NEAR` that is
    /// not a match is still on the row. That decides which phrases are counted, see
    /// [`Evaluated::live_positions`].
    ///
    /// @param node - the node
    /// @param row - the row
    fn standing(&self, node: &Match, row: i64) -> Standing {
        match node {
            Match::Near { first, phrases, .. } if phrases.len() == 1 => {
                let found = self
                    .hits
                    .get(*first)
                    .is_some_and(|found| found.contains_key(&row));
                if found {
                    Standing::Match
                } else {
                    Standing::Away
                }
            }
            Match::Near { .. } if near_lists(node, &self.hits, row).is_some() => Standing::Match,
            Match::Near { first, .. } if self.cooccurring(*first).binary_search(&row).is_ok() => {
                Standing::Near
            }
            Match::Near { .. } | Match::Nothing { .. } => Standing::Away,
            Match::And(children) => self.standing_of_and(children, row),
            Match::Or(children) => {
                let all: Vec<Standing> = children
                    .iter()
                    .map(|child| self.standing(child, row))
                    .collect();
                if all.contains(&Standing::Match) {
                    Standing::Match
                } else if all.contains(&Standing::Near) {
                    Standing::Near
                } else {
                    Standing::Away
                }
            }
            Match::Not(left, right) => match self.standing(left, row) {
                Standing::Away => Standing::Away,
                // A right side that matches the row moves the left side on, however the left stood.
                _ if self.standing(right, row) == Standing::Match => Standing::Away,
                left_standing => left_standing,
            },
        }
    }

    /// Returns where an `AND` stands: on the row when every child is, and a match when each is.
    ///
    /// @param children - the children of the `AND`
    /// @param row - the row
    fn standing_of_and(&self, children: &[Match], row: i64) -> Standing {
        let mut result = Standing::Match;
        for child in children {
            match self.standing(child, row) {
                Standing::Away => return Standing::Away,
                Standing::Near => result = Standing::Near,
                Standing::Match => {}
            }
        }
        result
    }

    /// Returns where each phrase counts in a row, which is where `bm25`, `highlight` and
    /// `snippet` look for it.
    ///
    /// A phrase has positions here only while the node holding it is on the row, as in SQLite:
    /// every child of an `AND` that matches, the children of an `OR` that are on the row, the
    /// left side of a `NOT` and a right side that is on the row without matching it. A `NEAR`
    /// that is on the row without matching keeps the positions of the phrases before the first
    /// one that has none in the row, which is what SQLite's `fts5ExprNearTest` leaves behind when
    /// it stops there. The list of a phrase that does not count is empty.
    ///
    /// @param root - the query's tree
    /// @param row - the row
    pub fn live_positions(&self, root: &Match, row: i64) -> Vec<Vec<Position>> {
        let mut out = vec![Vec::new(); self.hits.len()];
        if self.standing(root, row) == Standing::Match {
            self.collect_live(root, row, &mut out);
        }
        out
    }

    /// Adds the positions of every phrase that counts under a node that is on the row.
    ///
    /// @param node - a node that is on the row
    /// @param row - the row
    /// @param out - one list per phrase of the query
    fn collect_live(&self, node: &Match, row: i64, out: &mut [Vec<Position>]) {
        let standing = self.standing(node, row);
        match node {
            Match::Near { first, phrases, .. } if phrases.len() == 1 => {
                let found = self.hits.get(*first).and_then(|hits| hits.get(&row));
                if let (Some(found), Some(slot)) = (found, out.get_mut(*first)) {
                    *slot = flatten(found);
                }
            }
            Match::Near { first, .. } if standing == Standing::Match => {
                for (offset, list) in near_lists(node, &self.hits, row)
                    .into_iter()
                    .flatten()
                    .enumerate()
                {
                    if let Some(slot) = out.get_mut(first + offset) {
                        *slot = list;
                    }
                }
            }
            Match::Near { first, phrases, .. } if standing == Standing::Near => {
                self.collect_left_over(node, (*first, phrases.len()), row, out);
            }
            Match::Near { .. } | Match::Nothing { .. } => {}
            Match::And(children) if standing == Standing::Match => {
                for child in children {
                    self.collect_live(child, row, out);
                }
            }
            Match::And(_) => {}
            Match::Or(children) => {
                for child in children {
                    if self.standing(child, row) != Standing::Away {
                        self.collect_live(child, row, out);
                    }
                }
            }
            Match::Not(left, right) => {
                self.collect_live(left, row, out);
                if self.standing(right, row) == Standing::Near {
                    self.collect_live(right, row, out);
                }
            }
        }
    }

    /// Returns the first phrase of a `NEAR` that has no hits in a row.
    ///
    /// @param first - the number of the first phrase
    /// @param count - how many phrases the `NEAR` has
    /// @param row - the row
    fn first_missing(&self, first: usize, count: usize, row: i64) -> Option<usize> {
        (0..count).find(|offset| {
            !self
                .hits
                .get(first + offset)
                .is_some_and(|hits| hits.contains_key(&row))
        })
    }

    /// Adds what a `NEAR` that is on the row without matching leaves in its phrases' lists.
    ///
    /// SQLite fills the phrases one by one and stops at the first that is not in the row. The
    /// phrases before it hold every position they have, it holds none, and the phrases after it
    /// are not touched, so they keep what the last test of this `NEAR` left in them: see
    /// [`Evaluated::left_in_phrase`]. When every phrase is in the row the sweep that decides the
    /// window clears them all.
    ///
    /// @param node - the `NEAR`
    /// @param first - the number of the first phrase
    /// @param count - how many phrases the `NEAR` has
    /// @param row - the row
    /// @param out - one list per phrase of the query
    fn collect_left_over(
        &self,
        node: &Match,
        (first, count): (usize, usize),
        row: i64,
        out: &mut [Vec<Position>],
    ) {
        let Some(missing) = self.first_missing(first, count, row) else {
            return;
        };
        for offset in 0..count {
            let list = match offset.cmp(&missing) {
                std::cmp::Ordering::Less => self
                    .hits
                    .get(first + offset)
                    .and_then(|hits| hits.get(&row))
                    .map(|found| flatten(found))
                    .unwrap_or_default(),
                std::cmp::Ordering::Equal => Vec::new(),
                std::cmp::Ordering::Greater => {
                    self.left_in_phrase(node, (first, count), offset, row)
                }
            };
            if let Some(slot) = out.get_mut(first + offset) {
                *slot = list;
            }
        }
    }

    /// Returns what an earlier test of a `NEAR` left in the list of a phrase it did not reach now.
    ///
    /// SQLite keeps one list per phrase and rewrites it only when a test gets as far as that
    /// phrase, so the list read for a row that stopped before it is from the last earlier row
    /// where the `NEAR` had all its terms and reached it. That list is what the phrase found
    /// there, or what the sweep left of it when every phrase was found, or nothing when the
    /// test stopped at the phrase. The `NEAR` is taken to have been tested at every row where
    /// all its terms occur, which is how an `OR` visits it.
    ///
    /// @param node - the `NEAR`
    /// @param first - the number of the first phrase
    /// @param count - how many phrases the `NEAR` has
    /// @param phrase - which phrase, counted from the first
    /// @param row - the row being read
    fn left_in_phrase(
        &self,
        node: &Match,
        (first, count): (usize, usize),
        phrase: usize,
        row: i64,
    ) -> Vec<Position> {
        let rows = self.cooccurring(first);
        let before = rows.partition_point(|held| *held < row);
        for earlier in rows.iter().take(before).rev() {
            match self.first_missing(first, count, *earlier) {
                None => {
                    let lists = near_lists(node, &self.hits, *earlier).unwrap_or_default();
                    return lists.into_iter().nth(phrase).unwrap_or_default();
                }
                Some(stopped) if stopped > phrase => {
                    let found = self
                        .hits
                        .get(first + phrase)
                        .and_then(|hits| hits.get(earlier));
                    return found.map(|found| flatten(found)).unwrap_or_default();
                }
                Some(stopped) if stopped == phrase => return Vec::new(),
                Some(_) => {}
            }
        }
        Vec::new()
    }
}

/// Returns the rows in which every term of a `NEAR` leaf occurs, whatever column and whatever
/// order, for each leaf of a tree.
///
/// @param node - the tree
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param structure - the segments the query reads
/// @param columns - how many columns the table declares
/// @param into - each leaf's first phrase and its rows
fn cooccurring_rows(
    node: &Match,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
    columns: usize,
    into: &mut Vec<(usize, Vec<i64>)>,
) -> DbResult<()> {
    match node {
        Match::Near { first, phrases, .. } if phrases.len() > 1 => {
            let mut rows: Option<Vec<i64>> = None;
            for term in phrases.iter().flat_map(|phrase| phrase.terms.iter()) {
                let mut found = Vec::new();
                let admit_all = super::expr::ColumnFilter::default();
                for bytes in term_doclists(term, context, shadows, buffer, structure)? {
                    super::doclist_rows(&bytes, &admit_all, columns, &mut found);
                }
                found.sort_unstable();
                found.dedup();
                rows = Some(match rows {
                    None => found,
                    Some(held) => intersect_sorted(&held, &found),
                });
            }
            into.push((*first, rows.unwrap_or_default()));
        }
        Match::Near { .. } | Match::Nothing { .. } => {}
        Match::And(children) | Match::Or(children) => {
            for child in children {
                cooccurring_rows(child, context, shadows, buffer, structure, columns, into)?;
            }
        }
        Match::Not(left, right) => {
            cooccurring_rows(left, context, shadows, buffer, structure, columns, into)?;
            cooccurring_rows(right, context, shadows, buffer, structure, columns, into)?;
        }
    }
    Ok(())
}

/// Returns the rows a single-term phrase appears in, ascending.
///
/// The same predicate [`phrase_hits`] applies at offset zero - a column the phrase asked for,
/// inside the table, with at least one position - decided without collecting the positions that
/// prove it.
///
/// @param phrase - the phrase, which has exactly one term
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param structure - the segments the query reads
/// @param columns - how many columns the table declares
fn phrase_rows(
    phrase: &Phrase,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
    columns: usize,
) -> DbResult<Vec<i64>> {
    let Some(term) = phrase.terms.first() else {
        return Ok(Vec::new());
    };
    let mut rows = Vec::new();
    for bytes in term_doclists(term, context, shadows, buffer, structure)? {
        super::doclist_rows(&bytes, &phrase.columns, columns, &mut rows);
    }
    rows.sort_unstable();
    rows.dedup();
    Ok(rows)
}

/// Returns where one phrase appears, by row and column.
pub(crate) fn phrase_hits(
    phrase: &Phrase,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
    columns: usize,
) -> DbResult<Hits> {
    let mut found: Option<Hits> = None;
    for (offset, term) in phrase.terms.iter().enumerate() {
        let entries = term_entries(term, context, shadows, buffer, structure)?;
        let mut current: Hits = BTreeMap::new();
        for entry in entries {
            for (column, positions) in &entry.columns {
                if !phrase.columns.admits(*column) {
                    continue;
                }
                if *column >= columns {
                    continue;
                }
                // A phrase's later terms have to sit one place further on, so
                // every position is shifted back by its place in the phrase and
                // the intersection is then over the phrase's *start*.
                let shifted: Vec<u32> = positions
                    .iter()
                    .filter(|position| **position >= offset as u32)
                    .map(|position| position - offset as u32)
                    .collect();
                if shifted.is_empty() {
                    continue;
                }
                current
                    .entry(entry.rowid)
                    .or_default()
                    .push((*column, shifted));
            }
        }
        found = Some(match found {
            None => current,
            Some(previous) => meet(&previous, &current),
        });
        if found.as_ref().is_some_and(BTreeMap::is_empty) {
            return Ok(BTreeMap::new());
        }
    }
    let mut found = found.unwrap_or_default();
    if phrase.anchored {
        anchor(&mut found);
    }
    Ok(found)
}

/// Keeps only the hits that start at the first token of their column.
///
/// **What `^` means, and what it did not do (task-1979, R4).** The parser
/// recognised the caret, stepped past it and recorded nothing, so `^alpha`
/// matched `alpha` anywhere in the column - 206 rows where SQLite answered 82.
/// The positions in `found` have already been shifted back by each term's place
/// in the phrase, so the phrase's own start is position zero and the anchor is
/// simply that position surviving.
///
/// @param found - the phrase's hits, narrowed in place
fn anchor(found: &mut Hits) {
    found.retain(|_, columns| {
        columns.retain(|(_, positions)| positions.first() == Some(&0));
        !columns.is_empty()
    });
    for columns in found.values_mut() {
        for (_, positions) in columns.iter_mut() {
            positions.retain(|position| *position == 0);
        }
    }
}

/// Returns the positions two terms share, per row and column.
fn meet(left: &Hits, right: &Hits) -> Hits {
    let mut out = BTreeMap::new();
    for (rowid, columns) in left {
        let Some(other) = right.get(rowid) else {
            continue;
        };
        let mut kept = Vec::new();
        for (column, positions) in columns {
            let Some((_, others)) = other.iter().find(|(index, _)| index == column) else {
                continue;
            };
            let shared: Vec<u32> = positions
                .iter()
                .filter(|position| others.contains(position))
                .copied()
                .collect();
            if !shared.is_empty() {
                kept.push((*column, shared));
            }
        }
        if !kept.is_empty() {
            out.insert(*rowid, kept);
        }
    }
    out
}

/// Returns the doclists one term matches, prefix included.
///
/// One doclist for an ordinary term, with the entries of every segment and of the running
/// transaction laid over one another; one for each term that starts with the prefix for
/// `word*`.
///
/// @param term - the term
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param structure - the segments the query reads
fn term_doclists(
    term: &Term,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
) -> DbResult<Vec<Vec<u8>>> {
    if !term.prefix {
        let bytes =
            super::segment::read_term_bytes(context, shadows, buffer, structure, &term.token)?;
        return Ok(bytes.into_iter().collect());
    }
    // **The staged postings are read from the buffer, not flushed.** A query may not write, so
    // a prefix query that follows a write in the same transaction cannot flush what the write
    // staged. They are laid over what is stored instead, term by term. A prefix reads every
    // term the dictionary holds that starts with it, and the scan starts at the prefix, so it
    // reads only those.
    let terms =
        super::segment::scan_terms(context, shadows, Some(buffer), structure, Some(&term.token))?;
    Ok(terms
        .into_values()
        .map(|entries| super::doclist::encode_doclist(&entries))
        .collect())
}

/// Returns the entries of the rows one term matches, prefix included, in rowid order.
///
/// @param term - the term
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the doclists this transaction has staged
/// @param structure - the segments the query reads
fn term_entries(
    term: &Term,
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &super::Buffer,
    structure: &super::segment::Structure,
) -> DbResult<Vec<DocEntry>> {
    // **One term that is not a prefix is already the answer.** Its entries are in rowid order
    // and name each row once, so there is nothing to merge, and running a merge anyway searched
    // a growing vector once per entry. That is quadratic in the documents a term appears in: a
    // term in five hundred of them cost a hundred and twenty-five thousand comparisons per
    // query, and `evaluate` was seventy per cent of `SELECT count(*) ... MATCH`.
    if !term.prefix {
        return super::segment::read_term_entries(context, shadows, buffer, structure, &term.token);
    }
    let found =
        super::segment::scan_terms(context, shadows, Some(buffer), structure, Some(&term.token))?;
    // A prefix reads several terms, and they can name the same row: merged by rowid
    // rather than searched for it.
    let mut entries: Vec<DocEntry> = found.into_values().flatten().collect();
    entries.sort_by_key(|entry| entry.rowid);
    let mut merged: Vec<DocEntry> = Vec::with_capacity(entries.len());
    for entry in entries {
        match merged.last_mut().filter(|last| last.rowid == entry.rowid) {
            Some(found) => {
                for (column, positions) in entry.columns {
                    match found.columns.iter_mut().find(|(index, _)| *index == column) {
                        Some((_, existing)) => {
                            existing.extend(positions);
                            existing.sort_unstable();
                            existing.dedup();
                        }
                        None => found.columns.push((column, positions)),
                    }
                }
            }
            None => merged.push(entry),
        }
    }
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a position the way the index keeps it: the column above the offset.
    fn at(column: i64, offset: i64) -> Position {
        (column << 32) | offset
    }

    /// The three set operations are merges of ascending lists.
    #[test]
    fn the_set_operations_merge_ascending_lists() {
        assert_eq!(intersect_sorted(&[1, 3, 5, 7], &[3, 4, 5, 8]), vec![3, 5]);
        assert_eq!(union_sorted(&[1, 3, 5], &[2, 3, 6]), vec![1, 2, 3, 5, 6]);
        assert_eq!(difference_sorted(&[1, 2, 3, 4], &[2, 4, 9]), vec![1, 3]);
        assert_eq!(difference_sorted(&[1, 2], &[]), vec![1, 2]);
        assert!(intersect_sorted(&[], &[1]).is_empty());
    }

    /// The sweep needs every phrase within the width and distance of the furthest one.
    #[test]
    fn the_near_sweep_decides_three_phrases() {
        let ones = [1i64, 1, 1];
        // Three words side by side are not within 0, because the first has to be within the width
        // and the distance of the last, and are within 1.
        assert!(trim_near(&[vec![0], vec![1], vec![2]], &ones, 0).is_none());
        assert!(trim_near(&[vec![0], vec![1], vec![2]], &ones, 1).is_some());
        // `a x b c` is not within 1 for three phrases, and is within 2.
        assert!(trim_near(&[vec![0], vec![2], vec![3]], &ones, 1).is_none());
        assert!(trim_near(&[vec![0], vec![2], vec![3]], &ones, 2).is_some());
        // A phrase in another column is never near.
        assert!(trim_near(&[vec![at(0, 0)], vec![at(1, 1)]], &[1, 1], 5).is_none());
    }

    /// Only the positions the sweep stopped at are kept, which is what the score counts.
    #[test]
    fn only_the_positions_inside_a_window_are_kept() {
        // `p q p z z z z z z z z z z p` against `NEAR(p q, 2)`: the first and last `p` are far
        // from the `q`, and only the first window counts.
        let kept = trim_near(&[vec![0, 2, 13], vec![1]], &[1, 1], 2).expect("a match");
        assert_eq!(kept, vec![vec![0, 2], vec![1]]);
    }
}

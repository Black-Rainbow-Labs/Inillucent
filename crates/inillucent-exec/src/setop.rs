//! `UNION`, `UNION ALL`, `EXCEPT` and `INTERSECT`.
//!
//! Invariant: three of the four are *distinct* operations and only `UNION ALL`
//! is not. `EXCEPT` and `INTERSECT` remove duplicates from their answer even
//! when neither input has any, which is the rule people get wrong most often
//! and the reason a set operation is never just a concatenation with a filter.
//!
//! ## Why the right branch is pushed first
//!
//! A push executor has one pipeline running at a time, and `EXCEPT` and
//! `INTERSECT` cannot decide about a left row until they know the whole right
//! side. So the right branch is pushed through [`SetOp`] first and is reduced
//! to its encoded keys, not its rows - a set operation compares whole rows, so
//! the key *is* the row and there is nothing to gain by keeping both.
//!
//! `UNION` and `UNION ALL` have no such dependency: both branches push through
//! the same operator one after the other, and the operator either dedupes or
//! does not.
//!
//! ## What happens when the keys and rows do not fit
//!
//! Past a byte limit the operator stops hashing. Every row from then on, and every key and row it
//! already held, goes to a sorter ordered by key, then by which arm the row came from, then by
//! arrival. The row each operation keeps for a key is the first one in that order, so one pass over
//! the merged runs finds the answer, in key order, which is also the order the in memory form
//! answers in. [`SetKeys`] is still here for the recursive query's queue, which asks a different
//! question: whether a row has been seen before.
//!
//! ## Why comparison is by encoded key rather than value by value
//!
//! Because the answer has to agree with the tree's ordering and with `DISTINCT`,
//! and all three go through `inillucent_tree::key`. A set operation compares its
//! columns under the collations the compound's terms settled on, which is why
//! the collations are a constructor argument rather than something read off the
//! batch.

use std::collections::HashMap;

use inillucent_base::DbResult;
use inillucent_tree::datum::OwnedDatum;
use inillucent_tree::key;
use inillucent_value::collation::Collation;

use crate::batch::Batch;
use crate::ops::{emit_rows, Flow, Sink};

/// Which set operation is being computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetKind {
    /// `UNION`: both sides, duplicates removed.
    Union,
    /// `UNION ALL`: both sides, duplicates kept.
    UnionAll,
    /// `EXCEPT`: the left side's distinct rows that the right side does not have.
    Except,
    /// `INTERSECT`: the distinct rows both sides have.
    Intersect,
}

impl SetKind {
    /// Reports whether the operation removes duplicates from its answer.
    pub fn is_distinct(self) -> bool {
        !matches!(self, SetKind::UnionAll)
    }

    /// Reports whether the operation has to see the right branch before it can
    /// decide about a left row.
    pub fn needs_right_first(self) -> bool {
        matches!(self, SetKind::Except | SetKind::Intersect)
    }
}

/// Encodes one row of a batch as a comparison key.
///
/// @param batch - the batch the row is in
/// @param nth - which live row
/// @param collations - the collation of each column
fn key_of(batch: &Batch<'_>, nth: usize, collations: &[Collation]) -> DbResult<Vec<u8>> {
    let mut encoded = Vec::new();
    for column in 0..batch.columns.len() {
        let value = batch.value(nth, column)?;
        key::encode_into_with(
            &value,
            collations.get(column).copied().unwrap_or(Collation::Binary),
            &mut encoded,
        );
    }
    Ok(encoded)
}

/// The right branch of an `EXCEPT` or `INTERSECT`, reduced to its keys.
///
/// Only the keys are kept. A set operation compares whole rows, so the encoded
/// key is a faithful stand-in for the row and holding both would double the
/// memory for nothing.
#[derive(Default)]
pub struct SetKeys {
    collations: Vec<Collation>,
    counts: HashMap<Vec<u8>, usize>,
}

impl SetKeys {
    /// Returns an empty key set.
    ///
    /// @param collations - the collation of each column
    pub fn new(collations: Vec<Collation>) -> SetKeys {
        SetKeys {
            collations,
            counts: HashMap::new(),
        }
    }

    /// Reports whether a key was seen.
    ///
    /// @param encoded - the key
    pub fn contains(&self, encoded: &[u8]) -> bool {
        self.counts.contains_key(encoded)
    }

    /// Records one already-materialised row, reporting whether it is new.
    ///
    /// **The same encoding the batch path uses**, so a caller holding rows and
    /// a caller pushing batches agree about what a duplicate is. The recursive
    /// CTE fill loop is the caller: it holds the rows a pass produced, and
    /// `UNION` means a row already in the answer is not queued again - which is
    /// the difference between a graph walk that terminates on a cycle and one
    /// that does not.
    ///
    /// @param row - the row, whole, because a set operation compares whole rows
    pub fn remember(&mut self, row: &[OwnedDatum]) -> bool {
        let mut encoded = Vec::new();
        for (column, value) in row.iter().enumerate() {
            key::encode_into_with(
                &value.borrow(),
                self.collations
                    .get(column)
                    .copied()
                    .unwrap_or(Collation::Binary),
                &mut encoded,
            );
        }
        let count = self.counts.entry(encoded).or_insert(0);
        *count = count.saturating_add(1);
        *count == 1
    }

    /// Returns how many distinct keys were seen.
    pub fn len(&self) -> usize {
        self.counts.len()
    }

    /// Reports whether nothing was seen.
    pub fn is_empty(&self) -> bool {
        self.counts.is_empty()
    }
}

impl Sink for SetKeys {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        for nth in 0..batch.live() {
            let encoded = key_of(batch, nth, &self.collations)?;
            // **The right branch of `EXCEPT` and `INTERSECT` is materialised
            // whole before the left one is read (task-1932, H6).** Only a new
            // key costs memory; a repeat increments a counter.
            if !self.counts.contains_key(&encoded) {
                inillucent_base::budget::materialise(encoded.len() as u64)?;
            }
            *self.counts.entry(encoded).or_insert(0) += 1;
        }
        Ok(Flow::Continue)
    }

    fn finish(&mut self) -> DbResult<()> {
        Ok(())
    }
    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.counts.clear();
        Ok(())
    }
}

/// How many bytes `SetOp` holds in its key sets and its rows before it sorts in runs.
///
/// Half of what `Sort` holds, for the reason `Distinct` gives: the keys and the rows are held at
/// once, and the sorter they move to holds as much again.
const SET_BYTES: u64 = 32 * 1024 * 1024;

/// How many rows `SetOp` hands downstream at once when it merges runs.
const SET_BATCH: usize = 1024;

/// What a `SetOp` that passed its limit is doing: writing every row to a sorter.
///
/// A row is written as its encoded key, a tag, an arrival number and then the row. The sorter orders
/// them by key, then tag, then arrival, which puts the row each set operation keeps first among the
/// rows that share its key:
///
/// - `UNION` tags a row `-arm`, so a row of a later arm sorts before one of an earlier arm and,
///   within an arm, the earlier arrival comes first. That is the rule `SetOp::hold` states;
/// - `EXCEPT` and `INTERSECT` tag a row of the right arm 0, with nothing after the arrival number,
///   and a row of the left arm 1. The right arm's row for a key therefore comes before every left
///   row for it, so the walk knows whether the key is in the right arm before it meets the first
///   left row, which is the one that stays.
struct ExternalSet {
    /// The rows, laid out as above.
    sorter: crate::spill::ExternalSorter,
    /// The arrival number of the next row.
    seq: i64,
}

/// Orders rows of the form (key, tag, arrival number, ...) by key, then tag, then arrival.
///
/// @param left - one row
/// @param right - the other
fn key_tag_arrival(left: &[OwnedDatum], right: &[OwnedDatum]) -> std::cmp::Ordering {
    key_bytes(left)
        .cmp(key_bytes(right))
        .then_with(|| number_at(left, 1).cmp(&number_at(right, 1)))
        .then_with(|| number_at(left, 2).cmp(&number_at(right, 2)))
}

/// Returns the encoded key at the front of a sorter row.
///
/// @param row - the row
fn key_bytes(row: &[OwnedDatum]) -> &[u8] {
    match row.first() {
        Some(OwnedDatum::Blob(bytes)) => bytes.as_slice(),
        _ => &[],
    }
}

/// Returns an integer a sorter row holds, or zero.
///
/// @param row - the row
/// @param at - which value of it
fn number_at(row: &[OwnedDatum], at: usize) -> i64 {
    match row.get(at) {
        Some(OwnedDatum::Int(held)) => *held,
        _ => 0,
    }
}

/// Reports whether two rows that sorted next to each other share a key and a tag.
///
/// The later one is never the row a set operation keeps, so the run need not hold it.
///
/// @param earlier - the row that sorted first
/// @param later - the row that sorted next
fn same_key_and_tag(earlier: &[OwnedDatum], later: &[OwnedDatum]) -> bool {
    matches!(
        (earlier.first(), later.first(), earlier.get(1), later.get(1)),
        (
            Some(OwnedDatum::Blob(a)),
            Some(OwnedDatum::Blob(b)),
            Some(OwnedDatum::Int(x)),
            Some(OwnedDatum::Int(y)),
        ) if a == b && x == y
    )
}

/// Lays a row out for the sorter.
///
/// @param key - the row's encoded key
/// @param tag - the row's tag
/// @param seq - the row's arrival number
/// @param row - the row, or nothing for a right arm's membership key
fn laid_out(key: Vec<u8>, tag: i64, seq: i64, row: Vec<OwnedDatum>) -> Vec<OwnedDatum> {
    let mut out = Vec::with_capacity(row.len().saturating_add(3));
    out.push(OwnedDatum::Blob(key));
    out.push(OwnedDatum::Int(tag));
    out.push(OwnedDatum::Int(seq));
    out.extend(row);
    out
}

/// Filters and forwards the rows of a set operation.
///
/// **Both arms go through this operator, one after the other.** For `UNION` and `UNION ALL` the
/// left arm is pushed, then [`SetOp::next_arm`] is called, then the right arm. For `EXCEPT` and
/// `INTERSECT` the right arm is pushed first, because a left row cannot be judged until the whole
/// right arm is known, then [`SetOp::next_arm`], then the left arm. The right arm is reduced to
/// its keys.
pub struct SetOp {
    kind: SetKind,
    collations: Vec<Collation>,
    /// The right arm's keys, for `EXCEPT` and `INTERSECT`.
    right: std::collections::HashSet<Vec<u8>>,
    /// Where each key already held sits in `kept`, and which arm it came from,
    /// for the three distinct operations.
    seen: HashMap<Vec<u8>, (usize, usize)>,
    /// Which arm is being pushed: 0 for the first, one more for each
    /// [`SetOp::next_arm`].
    arm: usize,
    /// The answer so far, for the three distinct operations, in the order the
    /// keys were first seen.
    kept: Vec<Vec<OwnedDatum>>,
    downstream: Box<dyn Sink>,
    /// Where files come from, when the caller gave this operator somewhere to put them.
    spill: Option<std::rc::Rc<dyn crate::spill::Spill>>,
    /// Roughly how many bytes the keys and the rows occupy.
    held: u64,
    /// How many bytes they may occupy before they move to runs.
    limit: u64,
    /// The rows once the limit was passed.
    external: Option<ExternalSet>,
}

impl SetOp {
    /// Returns a set operation.
    ///
    /// @param kind - which of the four
    /// @param collations - the collation of each column
    /// @param downstream - what to push the surviving rows into
    pub fn new(kind: SetKind, collations: Vec<Collation>, downstream: Box<dyn Sink>) -> SetOp {
        SetOp {
            kind,
            collations,
            right: std::collections::HashSet::new(),
            seen: HashMap::new(),
            arm: 0,
            kept: Vec::new(),
            downstream,
            spill: None,
            held: 0,
            limit: crate::spill::run_bytes(SET_BYTES),
            external: None,
        }
    }

    /// Returns this operator with somewhere to write runs.
    ///
    /// **Without one it behaves exactly as it did before runs existed**: every key and row is held,
    /// and the byte budget refuses the statement if one was armed.
    ///
    /// @param spill - where files come from
    pub fn spilling_to(mut self, spill: Option<std::rc::Rc<dyn crate::spill::Spill>>) -> SetOp {
        self.spill = spill;
        self
    }

    /// Says that the rows pushed from here on come from the next arm.
    ///
    /// See [`SetOp::hold`]: which of two equal rows is kept depends on whether
    /// they came from one arm or from two.
    pub fn next_arm(&mut self) {
        self.arm = self.arm.saturating_add(1);
    }

    /// Reports whether the arm being pushed is the right arm of an `EXCEPT` or `INTERSECT`.
    fn pushing_the_right_arm(&self) -> bool {
        self.kind.needs_right_first() && self.arm == 0
    }

    /// Reports whether one row belongs in the answer at all.
    ///
    /// This is the membership half of a set operation and says nothing about
    /// duplicates; [`SetOp::hold`] decides those.
    ///
    /// @param encoded - the row's key
    fn wanted(&self, encoded: &[u8]) -> bool {
        match self.kind {
            SetKind::Union | SetKind::UnionAll => true,
            SetKind::Except => !self.right.contains(encoded),
            SetKind::Intersect => self.right.contains(encoded),
        }
    }

    /// Puts one row of a distinct operation into the answer, replacing an
    /// earlier row with the same key from an earlier arm.
    ///
    /// **Of two rows that compare equal, SQLite keeps the one from the later
    /// arm (task-1979, F20), and the first one within an arm.** Its `UNION` fills an ephemeral index with
    /// `OP_IdxInsert`, and a b-tree insert whose key matches an existing entry
    /// overwrites that entry's payload. Two rows can carry different values
    /// and still match: 1 and 1.0 compare equal, and so do `'a'` and `'A'`
    /// under NOCASE. Measured against 3.53.4, `SELECT 1 AS a UNION SELECT 1.0`
    /// answers the real 1.0 and `SELECT 1.0 AS a UNION SELECT 1` answers the
    /// integer 1; this operator kept whichever arrived first and answered the
    /// integer for both.
    ///
    /// **Within one arm the first row stays.** `SELECT a FROM t UNION SELECT 7`
    /// over the rows 0 then 0.0 answers the integer 0 in SQLite, and over 0.0
    /// then 0 answers 0.0; the same two rows as two arms answer the later one.
    /// Measured on the pinned 3.53.4, and found by the statement matrix's
    /// random layer, where the later row of one arm was kept.
    ///
    /// The rows come out in key order; see [`SetOp::finish`].
    ///
    /// @param encoded - the row's key
    /// @param row - the row itself
    fn hold(&mut self, encoded: &[u8], row: Vec<OwnedDatum>) -> DbResult<()> {
        if let Some((at, arm)) = self.seen.get(encoded).copied() {
            if arm != self.arm {
                if let Some(held) = self.kept.get_mut(at) {
                    *held = row;
                }
                self.seen.insert(encoded.to_vec(), (at, self.arm));
            }
            return Ok(());
        }
        // **Both halves are charged (task-1932, H6).** A distinct set
        // operation holds the key of every row it has answered with, and now
        // the row too, because the row is what a later equal key replaces.
        inillucent_base::budget::materialise(
            crate::ops::owned_row_bytes(&row).saturating_add(encoded.len() as u64),
        )?;
        self.held = self
            .held
            .saturating_add(crate::ops::resident_row_bytes(&row))
            .saturating_add((encoded.len() as u64).saturating_add(96));
        self.seen
            .insert(encoded.to_vec(), (self.kept.len(), self.arm));
        self.kept.push(row);
        Ok(())
    }

    /// Records one key of the right arm.
    ///
    /// @param encoded - the key
    fn remember_right(&mut self, encoded: Vec<u8>) -> DbResult<()> {
        // Only a new key costs memory; a repeat is already there.
        if !self.right.contains(&encoded) {
            inillucent_base::budget::materialise(encoded.len() as u64)?;
            self.held = self
                .held
                .saturating_add((encoded.len() as u64).saturating_add(64));
            self.right.insert(encoded);
        }
        Ok(())
    }

    /// Moves every key and row held to a sorter, once they have passed the limit.
    ///
    /// The maps are dropped before the sorter fills, so their memory goes back.
    fn go_external(&mut self) -> DbResult<()> {
        let Some(spill) = self.spill.clone() else {
            return Ok(());
        };
        let mut sorter = crate::spill::ExternalSorter::new(
            std::rc::Rc::new(key_tag_arrival),
            Some(spill),
            self.limit,
        )
        .with_redundant(same_key_and_tag);
        let mut seq = i64::try_from(self.kept.len()).unwrap_or(i64::MAX);
        let right = std::mem::take(&mut self.right);
        let seen = std::mem::take(&mut self.seen);
        let mut kept: Vec<Option<Vec<OwnedDatum>>> = std::mem::take(&mut self.kept)
            .into_iter()
            .map(Some)
            .collect();
        for key in right {
            sorter.push(laid_out(key, 0, seq, Vec::new()))?;
            seq = seq.saturating_add(1);
        }
        let needs_right = self.kind.needs_right_first();
        for (key, (at, arm)) in seen {
            let Some(row) = kept.get_mut(at).and_then(Option::take) else {
                continue;
            };
            let tag = if needs_right {
                1
            } else {
                0i64.saturating_sub(i64::try_from(arm).unwrap_or(i64::MAX))
            };
            sorter.push(laid_out(key, tag, i64::try_from(at).unwrap_or(0), row))?;
        }
        self.held = 0;
        self.external = Some(ExternalSet { sorter, seq });
        Ok(())
    }

    /// Writes one batch's rows to the sorter, every one of them.
    ///
    /// @param batch - the batch
    fn push_external(&mut self, batch: &Batch<'_>) -> DbResult<()> {
        let width = batch.columns.len();
        let right_arm = self.pushing_the_right_arm();
        let needs_right = self.kind.needs_right_first();
        let arm = i64::try_from(self.arm).unwrap_or(i64::MAX);
        for nth in 0..batch.live() {
            let encoded = key_of(batch, nth, &self.collations)?;
            let (tag, row) = if right_arm {
                (0i64, Vec::new())
            } else {
                let mut row = Vec::with_capacity(width);
                for column in 0..width {
                    row.push(OwnedDatum::from_datum(&batch.value(nth, column)?));
                }
                (
                    if needs_right {
                        1
                    } else {
                        0i64.saturating_sub(arm)
                    },
                    row,
                )
            };
            if let Some(external) = self.external.as_mut() {
                external
                    .sorter
                    .push(laid_out(encoded, tag, external.seq, row))?;
                external.seq = external.seq.saturating_add(1);
            }
        }
        Ok(())
    }

    /// Emits the answer from the sorted rows, one row for each key that stays.
    ///
    /// @param external - the sorter the rows were written to
    fn finish_external(&mut self, external: ExternalSet) -> DbResult<()> {
        let kind = self.kind;
        let mut current: Option<Vec<u8>> = None;
        let mut present = false;
        let mut decided = false;
        let mut batch: Vec<Vec<OwnedDatum>> = Vec::new();
        let mut stopped = false;
        let downstream = self.downstream.as_mut();
        external.sorter.finish()?.for_each(|mut row| {
            let tag = match row.get(1) {
                Some(OwnedDatum::Int(held)) => *held,
                _ => 0,
            };
            let key = match row.first() {
                Some(OwnedDatum::Blob(bytes)) => Some(bytes.clone()),
                _ => None,
            };
            if current != key {
                current = key;
                present = false;
                decided = false;
            }
            let keep = match kind {
                SetKind::Union | SetKind::UnionAll => !std::mem::replace(&mut decided, true),
                SetKind::Except | SetKind::Intersect => {
                    if tag == 0 {
                        present = true;
                        false
                    } else if decided {
                        false
                    } else {
                        decided = true;
                        present == (kind == SetKind::Intersect)
                    }
                }
            };
            if !keep {
                return Ok(Flow::Continue);
            }
            row.drain(..3);
            batch.push(row);
            if batch.len() >= SET_BATCH {
                let flow = emit_rows(&batch, downstream)?;
                batch.clear();
                if flow == Flow::Stop {
                    stopped = true;
                    return Ok(Flow::Stop);
                }
            }
            Ok(Flow::Continue)
        })?;
        if !stopped && !batch.is_empty() {
            emit_rows(&batch, downstream)?;
        }
        self.downstream.finish()
    }
}

impl Sink for SetOp {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        // `UNION ALL` never filters, so it forwards the batch whole rather than
        // taking it apart and putting it back together a row at a time.
        if self.kind == SetKind::UnionAll {
            return self.downstream.push(batch);
        }
        if self.external.is_some() {
            self.push_external(batch)?;
            return Ok(Flow::Continue);
        }
        let width = batch.columns.len();
        let right_arm = self.pushing_the_right_arm();
        for nth in 0..batch.live() {
            let encoded = key_of(batch, nth, &self.collations)?;
            if right_arm {
                self.remember_right(encoded)?;
                continue;
            }
            if !self.wanted(&encoded) {
                continue;
            }
            let mut row = Vec::with_capacity(width);
            for column in 0..width {
                row.push(OwnedDatum::from_datum(&batch.value(nth, column)?));
            }
            self.hold(&encoded, row)?;
        }
        if self.spill.is_some() && self.held >= self.limit {
            self.go_external()?;
        }
        Ok(Flow::Continue)
    }

    /// Emits the answer.
    ///
    /// **A distinct set operation cannot answer while it is still reading
    /// (task-1979, F20).** A row it has already emitted can be replaced by a
    /// later row with an equal key, so the rows are held until the input ends.
    /// The cost is one copy of the distinct answer, until it passes the limit. Past it the
    /// rows are written to runs and this operator holds a bounded number of them.
    ///
    /// **In key order, which is the order SQLite answers in.** SQLite builds a
    /// distinct set operation in an ephemeral index keyed by the whole row and
    /// reads the index back, so `SELECT 3 UNION SELECT 1 UNION SELECT 2`
    /// answers 1, 2, 3, and `EXCEPT` and `INTERSECT` come out sorted the same
    /// way. This operator answered in the order the rows arrived. No `ORDER
    /// BY` means SQL promises no order, but applications read it anyway, and
    /// an `UPDATE ... FROM` over a `UNION` keeps whichever of two matching
    /// rows comes first - so the order decided which value was stored. The
    /// encoded key sorts the way the tree does, under each column's collation,
    /// so sorting by it is sorting the way SQLite's index does.
    fn finish(&mut self) -> DbResult<()> {
        if let Some(external) = self.external.take() {
            return self.finish_external(external);
        }
        let mut order: Vec<(Vec<u8>, usize)> =
            self.seen.drain().map(|(key, (at, _))| (key, at)).collect();
        order.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let mut kept: Vec<Option<Vec<OwnedDatum>>> = std::mem::take(&mut self.kept)
            .into_iter()
            .map(Some)
            .collect();
        let sorted: Vec<Vec<OwnedDatum>> = order
            .into_iter()
            .filter_map(|(_, at)| kept.get_mut(at).and_then(Option::take))
            .collect();
        emit_rows(&sorted, self.downstream.as_mut())?;
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.right.clear();
        self.seen.clear();
        self.arm = 0;
        self.kept.clear();
        self.held = 0;
        self.external = None;
        self.downstream.reset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::Vector;
    use crate::ops::CollectInto;
    use inillucent_tree::datum::Datum;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Pushes one batch of a single integer column through a sink.
    ///
    /// @param sink - the operator under test
    /// @param values - one row per value
    fn push_ints(sink: &mut dyn Sink, values: &[i64]) {
        let data: Vec<Datum<'_>> = values.iter().map(|value| Datum::Int(*value)).collect();
        let batch = Batch::new(values.len(), vec![Vector::Values(&data)]);
        sink.push(&batch).expect("the push succeeds");
    }

    /// Runs one set operation over two integer branches and returns the answer.
    ///
    /// The collector is held through a shared buffer rather than read back out
    /// of the operator, because a sink owns its downstream and does not hand it
    /// back - which is the same shape the pipeline builder uses.
    ///
    /// @param kind - which operation
    /// @param left - the left branch's rows
    /// @param right - the right branch's rows
    fn run(kind: SetKind, left: &[i64], right: &[i64]) -> Vec<i64> {
        let collations = vec![Collation::Binary];
        let collected = Rc::new(RefCell::new(Vec::new()));
        let mut op = SetOp::new(
            kind,
            collations,
            Box::new(CollectInto::new(Rc::clone(&collected))),
        );
        if kind.needs_right_first() {
            push_ints(&mut op, right);
            op.next_arm();
            push_ints(&mut op, left);
        } else {
            push_ints(&mut op, left);
            op.next_arm();
            push_ints(&mut op, right);
        }
        op.finish().expect("the finish succeeds");
        let rows = collected.borrow();
        rows.iter()
            .filter_map(|row| match row.first() {
                Some(OwnedDatum::Int(value)) => Some(*value),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn union_all_keeps_every_duplicate() {
        assert_eq!(
            run(SetKind::UnionAll, &[1, 2, 2], &[2, 3]),
            vec![1, 2, 2, 2, 3]
        );
    }

    #[test]
    fn union_removes_duplicates_across_both_branches() {
        assert_eq!(run(SetKind::Union, &[1, 2, 2], &[2, 3]), vec![1, 2, 3]);
    }

    #[test]
    fn except_is_distinct_even_when_nothing_is_removed() {
        // The left side's own duplicate goes, though the right side removed
        // nothing at all. This is the rule people get wrong most often.
        assert_eq!(run(SetKind::Except, &[1, 1, 2], &[3]), vec![1, 2]);
        assert_eq!(run(SetKind::Except, &[1, 2, 3], &[2]), vec![1, 3]);
    }

    #[test]
    fn intersect_is_distinct_and_keeps_only_what_both_have() {
        assert_eq!(
            run(SetKind::Intersect, &[1, 2, 2, 3], &[2, 3, 4]),
            vec![2, 3]
        );
        assert!(run(SetKind::Intersect, &[1], &[2]).is_empty());
    }

    #[test]
    fn an_empty_right_branch_leaves_except_alone_and_empties_intersect() {
        assert_eq!(run(SetKind::Except, &[1, 2], &[]), vec![1, 2]);
        assert!(run(SetKind::Intersect, &[1, 2], &[]).is_empty());
    }

    #[test]
    fn the_key_set_reports_what_it_holds() {
        let mut keys = SetKeys::new(vec![Collation::Binary]);
        assert!(keys.is_empty());
        push_ints(&mut keys, &[1, 2, 2]);
        assert_eq!(keys.len(), 2);
        assert!(!keys.is_empty());
        keys.finish().expect("the finish succeeds");
    }

    #[test]
    fn text_keys_compare_under_the_column_collation() {
        let rows: Vec<Datum<'_>> = vec![Datum::Text(b"blue"), Datum::Text(b"BLUE")];
        let folded = Rc::new(RefCell::new(Vec::new()));
        let mut op = SetOp::new(
            SetKind::Union,
            vec![Collation::NoCase],
            Box::new(CollectInto::new(Rc::clone(&folded))),
        );
        op.push(&Batch::new(2, vec![Vector::Values(&rows)]))
            .expect("the push succeeds");
        // Under NOCASE the two spellings are one row. Within one arm the first
        // spelling stays, as it does in SQLite.
        let later: Vec<Datum<'_>> = vec![Datum::Text(b"Blue")];
        op.next_arm();
        op.push(&Batch::new(1, vec![Vector::Values(&later)]))
            .expect("the push succeeds");
        op.finish().expect("the finish succeeds");
        // A later arm's equal row replaces it, which is the entry a b-tree
        // insert with an equal key leaves behind in SQLite (task-1979, F20).
        // Under BINARY the two spellings are two rows and neither replaces the
        // other.
        assert_eq!(text_column(&folded.borrow()), vec![b"Blue".to_vec()]);
        let exact = Rc::new(RefCell::new(Vec::new()));
        let mut binary = SetOp::new(
            SetKind::Union,
            vec![Collation::Binary],
            Box::new(CollectInto::new(Rc::clone(&exact))),
        );
        binary
            .push(&Batch::new(2, vec![Vector::Values(&rows)]))
            .expect("the push succeeds");
        binary.finish().expect("the finish succeeds");
        // In key order, as SQLite answers `SELECT 'blue' UNION SELECT 'BLUE'`:
        // under BINARY the capitals sort first.
        assert_eq!(
            text_column(&exact.borrow()),
            vec![b"BLUE".to_vec(), b"blue".to_vec()]
        );
    }

    /// Returns the first column of each row, as text.
    ///
    /// @param rows - what a collector gathered
    fn text_column(rows: &[Vec<OwnedDatum>]) -> Vec<Vec<u8>> {
        rows.iter()
            .filter_map(|row| match row.first() {
                Some(OwnedDatum::Text(text)) => Some(text.to_vec()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_distinct_flag_matches_the_four_operations() {
        assert!(SetKind::Union.is_distinct());
        assert!(!SetKind::UnionAll.is_distinct());
        assert!(SetKind::Except.is_distinct());
        assert!(SetKind::Intersect.is_distinct());
        assert!(!SetKind::Union.needs_right_first());
        assert!(!SetKind::UnionAll.needs_right_first());
        assert!(SetKind::Except.needs_right_first());
        assert!(SetKind::Intersect.needs_right_first());
    }

    /// A spill file held in memory.
    #[derive(Default)]
    struct InMemoryFile {
        bytes: Vec<u8>,
    }

    impl crate::spill::SpillFile for InMemoryFile {
        fn append(&mut self, bytes: &[u8]) -> DbResult<u64> {
            let at = self.bytes.len() as u64;
            self.bytes.extend_from_slice(bytes);
            Ok(at)
        }

        fn read_at(&self, offset: u64, out: &mut [u8]) -> DbResult<()> {
            let from = usize::try_from(offset).unwrap_or(usize::MAX);
            let slice = self
                .bytes
                .get(from..from.saturating_add(out.len()))
                .ok_or_else(|| inillucent_base::error::misuse("a spill read ran past the file"))?;
            out.copy_from_slice(slice);
            Ok(())
        }
    }

    /// Hands out in memory spill files.
    struct InMemory;

    impl crate::spill::Spill for InMemory {
        fn open(&self) -> DbResult<Box<dyn crate::spill::SpillFile>> {
            Ok(Box::new(InMemoryFile::default()))
        }
    }

    /// Runs a set operation over two arms of rows of (number, text), with or without somewhere to
    /// spill, and returns the answer and whether the operator went to runs.
    ///
    /// @param kind - which operation
    /// @param left - the left arm
    /// @param right - the right arm
    /// @param limit - how many bytes to hold before moving to runs, or `None` for nowhere to spill
    fn run_rows(
        kind: SetKind,
        left: &[Vec<OwnedDatum>],
        right: &[Vec<OwnedDatum>],
        limit: Option<u64>,
    ) -> (Vec<Vec<OwnedDatum>>, bool) {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let mut op = SetOp::new(
            kind,
            vec![Collation::Binary, Collation::NoCase],
            Box::new(CollectInto::new(Rc::clone(&collected))),
        );
        if let Some(bytes) = limit {
            op = op.spilling_to(Some(Rc::new(InMemory)));
            op.limit = bytes;
        }
        let push_arm = |op: &mut SetOp, rows: &[Vec<OwnedDatum>]| {
            for chunk in rows.chunks(200) {
                let columns: Vec<Vec<Datum<'_>>> = (0..2)
                    .map(|column| {
                        chunk
                            .iter()
                            .map(|row| row.get(column).map_or(Datum::Null, OwnedDatum::borrow))
                            .collect()
                    })
                    .collect();
                let vectors: Vec<Vector<'_>> = columns
                    .iter()
                    .map(|values| Vector::Values(values.as_slice()))
                    .collect();
                op.push(&Batch::new(chunk.len(), vectors))
                    .expect("a batch is pushed");
            }
        };
        if kind.needs_right_first() {
            push_arm(&mut op, right);
            op.next_arm();
            push_arm(&mut op, left);
        } else {
            push_arm(&mut op, left);
            op.next_arm();
            push_arm(&mut op, right);
        }
        let external = op.external.is_some();
        op.finish().expect("the finish succeeds");
        let answer = collected.borrow().clone();
        (answer, external)
    }

    /// **A set operation that sorts in runs answers what the one that hashes answers.**
    ///
    /// Every operation, over arms that overlap, with integers and reals that compare equal and texts
    /// a collation folds, so the row kept for a key (the later arm's, then the first of its arm) is
    /// tested as well as the set of keys.
    #[test]
    fn every_set_operation_answers_the_same_in_runs() {
        let row = |n: i64, side: i64| -> Vec<OwnedDatum> {
            let key = (n * 7919) % 900;
            vec![
                if (n + side) % 3 == 0 {
                    OwnedDatum::Real(key as f64)
                } else {
                    OwnedDatum::Int(key)
                },
                OwnedDatum::Text(
                    if (n + side) % 2 == 0 {
                        format!("Key {}", key % 120)
                    } else {
                        format!("key {}", key % 120)
                    }
                    .into_bytes(),
                ),
            ]
        };
        let left: Vec<Vec<OwnedDatum>> = (0..3_000).map(|n| row(n, 0)).collect();
        let right: Vec<Vec<OwnedDatum>> = (500..3_800).map(|n| row(n * 3, 1)).collect();
        for kind in [SetKind::Union, SetKind::Except, SetKind::Intersect] {
            let (in_memory, none) = run_rows(kind, &left, &right, None);
            let (in_runs, external) = run_rows(kind, &left, &right, Some(12 * 1024));
            assert!(!none, "{kind:?} with nowhere to spill went to runs");
            assert!(
                external,
                "{kind:?} never reached its limit, so this compared memory with memory"
            );
            assert!(!in_memory.is_empty() || kind == SetKind::Intersect);
            assert_eq!(in_runs, in_memory, "{kind:?} answers differently in runs");
        }
    }
}

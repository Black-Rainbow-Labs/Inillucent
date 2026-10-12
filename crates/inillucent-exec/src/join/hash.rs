//! The hash join, and the table it probes.
//!
//! Invariant: **the key is compared as bytes.** Both sides are encoded
//! through the memcmp key codec before anything is hashed, so two values
//! that are equal under the dialect's rules hash alike and two that are
//! not cannot collide into one group.

use inillucent_base::DbResult;
use inillucent_tree::datum::OwnedDatum;
use inillucent_tree::key;

use crate::autoindex::HashTable;
use crate::batch::Batch;
use crate::expr::Eval;
use crate::ops::{emit_rows, Flow, Sink};

use super::*;

/// How a join treats rows that find no partner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JoinKind {
    /// Only matching pairs.
    Inner,
    /// Every probe row, null-extended when it matches nothing.
    Left,
    /// Every *build* row, null-extended when it matches nothing.
    ///
    /// Only [`NestedLoopJoin`] answers this one, and the reason is the shape of
    /// that operator rather than a gap: it holds the build side as a
    /// materialised vector, so it can remember which of those rows matched and
    /// emit the rest when the input ends. An operator that sees the build side
    /// one tree descent at a time has nothing to keep that mark on.
    Right,
    /// Every probe row and every build row, each null-extended when it matched
    /// nothing.
    Full,
    /// One probe row per probe row that matched, with no build columns.
    Semi,
    /// One probe row per probe row that matched nothing, with no build columns.
    Anti,
}
impl JoinKind {
    /// Reports whether unmatched *probe* rows are kept, null-extended.
    pub fn keeps_probe(self) -> bool {
        matches!(self, JoinKind::Left | JoinKind::Full)
    }

    /// Reports whether unmatched *build* rows are kept, null-extended.
    pub fn keeps_build(self) -> bool {
        matches!(self, JoinKind::Right | JoinKind::Full)
    }
}
/// Builds a hash table from one side, then probes it with the other.
///
/// The build side is pushed in first through [`HashJoin::build`]; after that the
/// operator is an ordinary sink and every batch pushed into it is a probe.
pub struct HashJoin<'s> {
    kind: JoinKind,
    /// How wide the probe side is, from the plan and then from each batch.
    ///
    /// Read at `finish`, where a RIGHT or FULL join null-extends the build rows
    /// nothing matched and there is no batch left to ask. An empty probe side
    /// sends no batch at all, so the plan's width is what keeps the build
    /// values out of the probe side's columns.
    probe_width: usize,
    /// How wide the build side is, from the plan.
    ///
    /// An empty build side has no row to measure, and a LEFT join's unmatched
    /// probe rows still need that many NULLs.
    build_width: usize,
    /// The build side's key expressions, over the build row's columns.
    build_keys: Vec<Box<dyn Eval>>,
    /// The probe side's key expressions, over the probe batch's columns.
    probe_keys: Vec<Box<dyn Eval>>,
    table: HashTable,
    /// The conjuncts of the join condition the key did not take, over the
    /// joined row, tested for each candidate pair the key finds.
    condition: Option<Box<dyn Eval>>,
    downstream: Box<dyn Sink + 's>,
    /// Reused between probes so a join of a million rows makes no allocation
    /// per row.
    scratch: Vec<u8>,
}
impl<'s> HashJoin<'s> {
    /// Returns a hash join with an empty table.
    ///
    /// @param kind - how unmatched rows are treated
    /// @param build_keys - the build side's key expressions
    /// @param probe_keys - the probe side's key expressions
    /// @param downstream - what to push joined rows into
    pub fn new(
        kind: JoinKind,
        build_keys: Vec<Box<dyn Eval>>,
        probe_keys: Vec<Box<dyn Eval>>,
        downstream: Box<dyn Sink + 's>,
    ) -> HashJoin<'s> {
        HashJoin {
            kind,
            probe_width: 0,
            build_width: 0,
            build_keys,
            probe_keys,
            table: HashTable::new(),
            condition: None,
            downstream,
            scratch: Vec::new(),
        }
    }

    /// Returns the join with the width of each side taken from the plan.
    ///
    /// The widths are otherwise measured from the first probe batch and the
    /// first build row, and an empty side has neither.
    ///
    /// @param probe - how many columns a probe row has
    /// @param build - how many columns a build row has
    pub fn with_widths(mut self, probe: usize, build: usize) -> HashJoin<'s> {
        self.probe_width = probe;
        self.build_width = build;
        self
    }

    /// Returns the join with a condition tested for each pair the key finds.
    ///
    /// The key decides which pairs are candidates and the condition decides
    /// which candidates are partners. A probe row none of whose candidates pass
    /// is unmatched, and an outer join null extends it exactly as it does a row
    /// with an empty bucket.
    ///
    /// @param condition - the conjuncts the key did not take, over the probe row followed by the build row
    pub fn with_condition(mut self, condition: Box<dyn Eval>) -> HashJoin<'s> {
        self.condition = Some(condition);
        self
    }

    /// Adds one batch of build rows to the table.
    ///
    /// @param batch - a batch from the build side
    pub fn build(&mut self, batch: &Batch<'_>) -> DbResult<()> {
        // The build side is charged to the request budget inside `absorb`: it
        // is the whole of one input held in memory before a single output row
        // exists, and a join whose build side is the large table and whose
        // answer is one row must still count that table.
        self.table.absorb(&self.build_keys, batch)
    }

    /// Returns how many rows the build side put in the table.
    pub fn build_rows(&self) -> usize {
        self.table.rows.len()
    }

    /// Adds rows that were materialised rather than streamed.
    ///
    /// The automatic index's entry point: the inner side of a join has already
    /// been read into a vector by the time the operator is built, and turning
    /// it back into batches only so that `build` can take them apart again is
    /// work with no reader. The rows are fed a batch at a time even so, because
    /// the key expressions are compiled against a batch and evaluating them one
    /// row at a time would mean a second evaluator.
    ///
    /// @param rows - the inner side's rows, in the order they were read
    pub fn build_materialised(&mut self, rows: &[Vec<OwnedDatum>]) -> DbResult<()> {
        self.table.absorb_rows(&self.build_keys, rows)
    }
}
/// Returns the candidate build rows that pass a join condition with a probe row.
///
/// @param condition - the conjuncts the key did not take, over the probe row followed by the build row
/// @param probe - the probe row
/// @param rows - the build rows
/// @param candidates - the positions in `rows` the key found
fn passing_pairs(
    condition: &dyn Eval,
    probe: &[OwnedDatum],
    rows: &[Vec<OwnedDatum>],
    candidates: Vec<u32>,
) -> DbResult<Vec<u32>> {
    let mut joined: Vec<OwnedDatum> = Vec::new();
    let mut passing = Vec::with_capacity(candidates.len());
    for position in candidates {
        joined.clear();
        joined.extend(probe.iter().cloned());
        if let Some(build) = rows.get(position as usize) {
            joined.extend(build.iter().cloned());
        }
        if super::loops::keeps(Some(condition), &joined)? {
            passing.push(position);
        }
    }
    Ok(passing)
}
impl Sink for HashJoin<'_> {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        // **Every batch, because the scan leaves are not enough (task-1932,
        // H11).** A join reads its probe side once and then does work
        // proportional to the matches, so a cross join over a small table
        // checks a few hundred times at the start and then runs for as long as
        // the product takes with nothing reading the cancellation flag. One
        // atomic load per batch is what makes a cancel reach the statement it
        // is about rather than the one after it.
        inillucent_base::budget::check()?;
        let HashJoin {
            kind,
            probe_width,
            build_width,
            probe_keys,
            table,
            condition,
            downstream,
            scratch,
            ..
        } = self;
        let width = batch.columns.len();
        *probe_width = width;
        let build_width = table.rows.first().map(Vec::len).unwrap_or(*build_width);
        let mut produced: Vec<Vec<OwnedDatum>> = Vec::new();
        for nth in 0..batch.live() {
            scratch.clear();
            let mut has_null = false;
            for expression in probe_keys.iter() {
                let value = expression.value(batch, nth)?;
                has_null |= value.is_null();
                key::encode_into(&value.get(), scratch);
            }
            // **Copied once per probe row, including when it matched
            // nothing (task-1932, M7).** `probe` answers a borrowed slice and
            // this cloned it so the loop below could hold it across the
            // `&mut self` a push needs. Reading the length first means a row
            // that matches nothing - the common case on a selective join -
            // allocates nothing at all.
            let found = if has_null {
                &[][..]
            } else {
                table.probe(scratch)
            };
            let matches: Vec<u32> = match found.is_empty() {
                true => Vec::new(),
                false => found.to_vec(),
            };
            let matches = match condition.as_deref() {
                Some(test) if !matches.is_empty() => {
                    let probe = materialise(batch, nth, width)?;
                    passing_pairs(test, &probe, &table.rows, matches)?
                }
                _ => matches,
            };
            match kind {
                JoinKind::Semi => {
                    if !matches.is_empty() {
                        produced.push(materialise(batch, nth, width)?);
                    }
                }
                JoinKind::Anti => {
                    if matches.is_empty() {
                        produced.push(materialise(batch, nth, width)?);
                    }
                }
                JoinKind::Inner | JoinKind::Left | JoinKind::Right | JoinKind::Full => {
                    if matches.is_empty() {
                        if kind.keeps_probe() {
                            let mut row = materialise(batch, nth, width)?;
                            row.extend(std::iter::repeat_n(OwnedDatum::Null, build_width));
                            produced.push(row);
                        }
                    } else {
                        for position in matches {
                            if let Some(slot) = table.matched.get_mut(position as usize) {
                                *slot = true;
                            }
                            let mut row = materialise(batch, nth, width)?;
                            if let Some(build) = table.rows.get(position as usize) {
                                row.extend(build.iter().cloned());
                            }
                            produced.push(row);
                        }
                    }
                }
            }
        }
        if produced.is_empty() {
            return Ok(Flow::Continue);
        }
        emit_rows(&produced, downstream.as_mut())
    }

    fn finish(&mut self) -> DbResult<()> {
        // The build rows nothing matched, which only a RIGHT or FULL join
        // keeps and which only the end of the probe side can identify.
        if self.kind.keeps_build() {
            let mut produced: Vec<Vec<OwnedDatum>> = Vec::new();
            for (position, build) in self.table.rows.iter().enumerate() {
                if self.table.matched.get(position).copied().unwrap_or(false) {
                    continue;
                }
                let mut row: Vec<OwnedDatum> =
                    std::iter::repeat_n(OwnedDatum::Null, self.probe_width).collect();
                row.extend(build.iter().cloned());
                produced.push(row);
            }
            if !produced.is_empty() {
                emit_rows(&produced, self.downstream.as_mut())?;
            }
        }
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.table.clear();
        self.scratch.clear();
        self.downstream.reset()
    }
}

//! The rest of an index nested loop's outer rows, joined with one walk of the inner tree.
//!
//! Invariant: **a merge emits exactly the rows the probes it replaces would have emitted, in the
//! same order.** A buffered outer row is kept only when its key is one integer, and an inner entry
//! pairs with it only when the entry's first key column is that same integer, which is what an
//! index probe with that integer finds. The pairs are found in the inner tree's order and then
//! put back in the outer rows' order, each outer row's entries in the tree's order, which is the
//! order one probe per outer row produces. So what follows the join, a streaming `GROUP BY`
//! included, sees no difference. Only an inner join merges: it emits nothing for an outer row
//! that matches nothing, so there is no unmatched row to place.
//!
//! **Why (task-2226).** `app.dashboard` joins 12,500 `main_table` rows to `side_table` through
//! `side_owner`, and the probe keys are in no useful order, so each of the 12,500 probes
//! descended the index from its root: 2.4 ms a run against SQLite's 1.95. One walk of the index
//! over the range the keys cover reads each of its entries once, and the buffered keys are
//! sorted so the two meet in one pass.
//!
//! **When.** A join only learns how many outer rows it has by being handed them, so it probes the
//! first ones as it always did and starts buffering once it has seen a tenth as many as the inner
//! tree holds: past that many probes, one walk of the tree costs less than the probes still to
//! come. A join whose outer side stays small never buffers anything, which is `join.range`'s 201
//! rows a run. A key that is not one integer, arriving while rows are buffered, first has the
//! buffered rows emitted and is then probed, so the order holds there too.

use inillucent_base::DbResult;
use inillucent_pool::Pool;
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::PagedTree;

use crate::batch::{Batch, Vector, BATCH_ROWS};
use crate::ops::{Flow, Sink};
use crate::scan::Projection;

/// How many inner rows one outer row's probe is worth, in a walk of the tree.
///
/// A probe descends from the root and searches a leaf; a walk reads the next entry.
const PROBE_COST: u64 = 10;

/// The outer rows an index nested loop has buffered for a merge, and when it starts buffering.
#[derive(Debug, Default)]
pub(super) struct MergeBuffer {
    /// How many outer rows have arrived so far, buffered or not.
    seen: usize,
    /// How many outer rows are probed before the rest are buffered.
    threshold: usize,
    /// Each buffered row's key, in arrival order.
    keys: Vec<i64>,
    /// The buffered rows' values, `width` of them a row.
    values: Vec<OwnedDatum>,
    /// How many columns an outer row has.
    width: usize,
    /// How many outer rows were ever buffered, for a test to tell that a merge ran.
    merged: usize,
}

impl MergeBuffer {
    /// Returns a buffer for a join into a tree of `inner_rows` rows.
    ///
    /// @param inner_rows - how many rows the inner tree holds
    pub(super) fn for_inner(inner_rows: u64) -> MergeBuffer {
        let (keys, values) = SPARE.with(|spare| spare.take()).unwrap_or_default();
        MergeBuffer {
            seen: 0,
            threshold: usize::try_from(inner_rows / PROBE_COST).unwrap_or(usize::MAX),
            keys,
            values,
            width: 0,
            merged: 0,
        }
    }
}

thread_local! {
    /// The vectors of the last merge buffer dropped on this thread, emptied, for the next one.
    ///
    /// **A compiled statement builds its joins again on every execution** (see
    /// `Compiled::run_source`), so each execution's buffer grew its vectors from nothing, and
    /// the copies of that growth were 7% of `app.dashboard` (task-2226). One pair is kept, and
    /// only while it holds no more than [`SPARE_BYTES`].
    static SPARE: std::cell::Cell<Option<(Vec<i64>, Vec<OwnedDatum>)>> =
        const { std::cell::Cell::new(None) };
}

/// The most memory the kept vectors may hold between statements.
const SPARE_BYTES: usize = 4 << 20;

impl Drop for MergeBuffer {
    /// Keeps the emptied vectors for the next buffer on this thread, when they are not large.
    fn drop(&mut self) {
        let bytes = self
            .keys
            .capacity()
            .saturating_mul(std::mem::size_of::<i64>())
            .saturating_add(
                self.values
                    .capacity()
                    .saturating_mul(std::mem::size_of::<OwnedDatum>()),
            );
        if bytes > SPARE_BYTES {
            return;
        }
        let mut keys = std::mem::take(&mut self.keys);
        let mut values = std::mem::take(&mut self.values);
        keys.clear();
        values.clear();
        SPARE.with(|spare| spare.set(Some((keys, values))));
    }
}

impl MergeBuffer {
    /// Returns the first live row of a batch of `live` rows that is buffered rather than
    /// probed, and counts the batch as seen.
    ///
    /// @param live - how many live rows the batch has
    /// @param width - how many columns an outer row has
    pub(super) fn buffer_from(&mut self, live: usize, width: usize) -> usize {
        let from = self.threshold.saturating_sub(self.seen).min(live);
        self.seen = self.seen.saturating_add(live);
        // Room for the rows this batch buffers, at once: growing the two vectors a row at a
        // time was a tenth of buffering (task-2226).
        let buffered = live.saturating_sub(from);
        if buffered > 0 {
            self.keys.reserve(buffered);
            self.values.reserve(buffered.saturating_mul(width));
        }
        from
    }

    /// Reports whether any row is buffered.
    pub(super) fn holds_rows(&self) -> bool {
        !self.keys.is_empty()
    }

    /// Buffers one outer row whose key is one integer.
    ///
    /// @param key - the row's join key
    /// @param batch - the outer rows
    /// @param nth - which live row
    pub(super) fn keep(&mut self, key: i64, batch: &Batch<'_>, nth: usize) -> DbResult<()> {
        self.width = batch.columns.len();
        self.keys.push(key);
        self.merged = self.merged.saturating_add(1);
        for column in 0..self.width {
            self.values
                .push(OwnedDatum::from_datum(&batch.value(nth, column)?));
        }
        Ok(())
    }

    /// Returns how many outer rows were ever buffered, across every execution.
    pub(super) fn merged(&self) -> usize {
        self.merged
    }

    /// Forgets every buffered row and the count of rows seen, for the next execution.
    pub(super) fn clear(&mut self) {
        self.seen = 0;
        self.keys.clear();
        self.values.clear();
    }

    /// Joins the buffered rows with the inner tree in one walk, pushes the pairs downstream in the
    /// outer rows' order, and forgets the rows.
    ///
    /// @param inner - the inner tree, whose first key column the keys are compared with
    /// @param pool - the pool the inner tree's pages live in
    /// @param projection - which inner columns follow the outer ones
    /// @param needed - which of those the statement reads
    /// @param downstream - where the joined rows go
    pub(super) fn emit(
        &mut self,
        inner: &PagedTree,
        pool: &Pool,
        projection: &Projection,
        needed: &crate::paged::Needed,
        downstream: &mut dyn Sink,
    ) -> DbResult<Flow> {
        let pairs = self.pairs(inner, pool, projection, needed)?;
        let flow = emit_in_outer_order(&pairs, &self.values, self.width, downstream)?;
        self.keys.clear();
        self.values.clear();
        Ok(flow)
    }

    /// Walks the inner tree over the range the buffered keys cover and returns every pair.
    ///
    /// @param inner - the inner tree
    /// @param pool - the pool its pages live in
    /// @param projection - which inner columns a pair carries
    /// @param needed - which of those the statement reads; the rest are NULL
    fn pairs(
        &self,
        inner: &PagedTree,
        pool: &Pool,
        projection: &Projection,
        needed: &crate::paged::Needed,
    ) -> DbResult<Pairs> {
        // Each key and its row number packed into one integer that sorts as the pair does, so
        // the sort compares plain integers: the key with its sign bit flipped, which orders an
        // `i64` as an unsigned number, above the row number.
        let mut packed: Vec<u128> = self
            .keys
            .iter()
            .enumerate()
            .map(|(row, key)| {
                let ordered = u128::from((*key as u64) ^ (1u64 << 63));
                (ordered << 32) | u128::from(u32::try_from(row).unwrap_or(u32::MAX))
            })
            .collect();
        packed.sort_unstable();
        let order: Vec<(i64, u32)> = packed
            .iter()
            .map(|held| {
                let key = (((held >> 32) as u64) ^ (1u64 << 63)) as i64;
                (key, (held & u128::from(u32::MAX)) as u32)
            })
            .collect();
        let mut pairs = Pairs {
            width: projection.0.len(),
            ..Pairs::default()
        };
        let (Some(first), Some(last)) = (order.first().copied(), order.last().copied()) else {
            return Ok(pairs);
        };
        let low = [Datum::Int(first.0)];
        let high = [Datum::Int(last.0)];
        let mut at = 0usize;
        inner.visit_span(
            pool,
            Some(&low),
            true,
            Some(&high),
            true,
            &mut |leaf, start, end| {
                // A written leaf's live rows are not its sorted region, so they are merged and the
                // bounds applied again, as `SpanScan` does.
                let merged: Vec<Vec<Datum<'_>>> = match leaf.needs_materialising() {
                    true => leaf.live_between(Some(&low), true, Some(&high), true)?,
                    false => Vec::new(),
                };
                let count = match leaf.needs_materialising() {
                    true => merged.len(),
                    false => end.saturating_sub(start),
                };
                for row in 0..count {
                    let value = |column: usize| -> DbResult<Datum<'_>> {
                        match leaf.needs_materialising() {
                            true => Ok(merged
                                .get(row)
                                .and_then(|values| values.get(column))
                                .copied()
                                .unwrap_or(Datum::Null)),
                            false => leaf.value(start.saturating_add(row), column),
                        }
                    };
                    let Some(key) = integer_key(value(0)?) else {
                        continue;
                    };
                    while order.get(at).is_some_and(|held| held.0 < key) {
                        at = at.saturating_add(1);
                    }
                    let mut matching = at;
                    while let Some((held, outer)) = order.get(matching).copied() {
                        if held != key {
                            break;
                        }
                        pairs.outer.push(outer);
                        for (position, column) in projection.0.iter().enumerate() {
                            pairs
                                .values
                                .push(match crate::paged::is_needed(needed, position) {
                                    true => OwnedDatum::from_datum(&value(*column)?),
                                    false => OwnedDatum::Null,
                                });
                        }
                        matching = matching.saturating_add(1);
                    }
                }
                Ok(at < order.len())
            },
        )?;
        Ok(pairs)
    }
}

/// Returns the integer an inner key equals, or `None` when no integer probe would find it.
///
/// An integer, and a real with no fraction, which a column with no declared type keeps as a
/// real and which compares equal to the integer, as a probe compares them. Text, a blob, NULL and
/// a real with a fraction equal no integer.
///
/// @param key - the inner entry's first key value
fn integer_key(key: Datum<'_>) -> Option<i64> {
    /// Two to the sixty third: every real with no fraction below it in magnitude is an integer
    /// an `i64` holds exactly, and `-LIMIT` itself is `i64::MIN`.
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    match key {
        Datum::Int(value) => Some(value),
        Datum::Real(value) if value.fract() == 0.0 && (-LIMIT..LIMIT).contains(&value) => {
            Some(value as i64)
        }
        _ => None,
    }
}

/// Every pair a walk found, in the inner tree's order.
#[derive(Debug, Default)]
struct Pairs {
    /// Each pair's outer row, by its number among the buffered rows.
    outer: Vec<u32>,
    /// Each pair's inner values, `width` of them a pair.
    values: Vec<OwnedDatum>,
    /// How many inner values a pair carries.
    width: usize,
}

/// Pushes pairs downstream in the outer rows' order, a batch at a time.
///
/// A stable counting sort by outer row keeps each outer row's pairs in the inner tree's order,
/// which is the order a probe of that row would have found them in.
///
/// @param pairs - the pairs
/// @param outer - the buffered outer rows' values
/// @param width - how many values an outer row has
/// @param downstream - where the joined rows go
fn emit_in_outer_order(
    pairs: &Pairs,
    outer: &[OwnedDatum],
    width: usize,
    downstream: &mut dyn Sink,
) -> DbResult<Flow> {
    let rows = outer.len().checked_div(width).unwrap_or(0);
    let mut starts = vec![0usize; rows.saturating_add(1)];
    for row in &pairs.outer {
        if let Some(slot) = starts.get_mut((*row as usize).saturating_add(1)) {
            *slot = slot.saturating_add(1);
        }
    }
    for at in 1..starts.len() {
        let before = starts.get(at.saturating_sub(1)).copied().unwrap_or(0);
        if let Some(slot) = starts.get_mut(at) {
            *slot = slot.saturating_add(before);
        }
    }
    let mut placed = vec![0usize; pairs.outer.len()];
    for (pair, row) in pairs.outer.iter().enumerate() {
        if let Some(next) = starts.get_mut(*row as usize) {
            if let Some(slot) = placed.get_mut(*next) {
                *slot = pair;
            }
            *next = next.saturating_add(1);
        }
    }
    for chunk in placed.chunks(BATCH_ROWS) {
        let mut columns: Vec<Vec<Datum<'_>>> = vec![Vec::with_capacity(chunk.len()); width];
        let mut inner: Vec<Vec<Datum<'_>>> = vec![Vec::with_capacity(chunk.len()); pairs.width];
        for pair in chunk {
            let row = pairs.outer.get(*pair).copied().unwrap_or(0) as usize;
            for (column, values) in columns.iter_mut().enumerate() {
                let held = outer.get(row.saturating_mul(width).saturating_add(column));
                values.push(held.map(OwnedDatum::borrow).unwrap_or(Datum::Null));
            }
            for (column, values) in inner.iter_mut().enumerate() {
                let held = pairs
                    .values
                    .get(pair.saturating_mul(pairs.width).saturating_add(column));
                values.push(held.map(OwnedDatum::borrow).unwrap_or(Datum::Null));
            }
        }
        let vectors: Vec<Vector<'_>> = columns
            .iter()
            .chain(inner.iter())
            .map(|values| Vector::Values(values.as_slice()))
            .collect();
        if downstream.push(&Batch::over(chunk.len(), &vectors))? == Flow::Stop {
            return Ok(Flow::Stop);
        }
    }
    Ok(Flow::Continue)
}

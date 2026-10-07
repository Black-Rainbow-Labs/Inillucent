//! The vectorised executor's window operator.
//!
//! Invariant: this operator computes *values*, never frames. Which rows a
//! frame contains comes from [`inillucent_scalar::window`], which the bytecode VM
//! calls too - `EXCLUDE TIES` differing from `EXCLUDE GROUP` by one row is the
//! kind of rule that must have exactly one implementation in the workspace.
//! What is here is everything that needs a row's values: the comparisons that
//! decide peer groups, the arguments read out of their columns, and the
//! accumulator, which is operator state rather than a function.
//!
//! ## Why it is a pipeline breaker, and addressed by column number
//!
//! A window reads its whole partition in both directions - `last_value` looks
//! to the end of the frame, `EXCLUDE TIES` looks ahead an unbounded distance
//! inside a peer group - so there is no streaming form. The input arrives
//! already sorted by the partition keys and then by the window's own
//! `ORDER BY`, and this operator buffers it.
//!
//! Nothing here is an expression. Every value a call needs - its arguments, its
//! `FILTER`, its frame offsets, its ordering terms - is a column of the
//! buffered row, put there by the projection upstream. That is the same shape
//! the VM's window plan has, and it is what keeps the operator small enough to
//! read: the hard part is the frame arithmetic, not the plumbing.

use std::collections::HashSet;

use inillucent_base::DbResult;
use inillucent_scalar::window as frames;
use inillucent_sql::ast::{FrameExclude, FrameUnit};
use inillucent_sql::function::{AggregateFunc, WindowFunc};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::key;
use inillucent_tree::types::compare_under;
use inillucent_value::collation::Collation;
use inillucent_value::Value;

use crate::aggregate::{Accumulator, AggregateKind};
use crate::batch::Batch;
use crate::join::RowStore;
use crate::ops::{emit_rows, Flow, Sink};

/// Which family a window call belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WindowSlot {
    /// An aggregate computed over the frame.
    Aggregate(AggregateKind),
    /// One of the eleven functions that only exist in a window.
    Plain(WindowFunc),
}

/// One end of a frame as the plan writes it, before the offset is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameEnd {
    /// `UNBOUNDED PRECEDING`.
    UnboundedPreceding,
    /// `CURRENT ROW`.
    CurrentRow,
    /// `UNBOUNDED FOLLOWING`.
    UnboundedFollowing,
    /// `expr PRECEDING` or `expr FOLLOWING`, the offset in a buffered column.
    Offset {
        /// The column holding the offset, evaluated once per row.
        column: usize,
        /// Whether it counts backwards.
        preceding: bool,
    },
}

/// A frame specification, with its offsets still in their columns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowFrame {
    /// `ROWS`, `RANGE` or `GROUPS`.
    pub unit: FrameUnit,
    /// The start.
    pub start: FrameEnd,
    /// The end.
    pub end: FrameEnd,
    /// The `EXCLUDE` clause.
    pub exclude: FrameExclude,
}

impl Default for WindowFrame {
    /// The frame a window with an `ORDER BY` gets when none was written:
    /// `RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW`.
    fn default() -> WindowFrame {
        WindowFrame {
            unit: FrameUnit::Range,
            start: FrameEnd::UnboundedPreceding,
            end: FrameEnd::CurrentRow,
            exclude: FrameExclude::NoOthers,
        }
    }
}

/// One ordering term of a window's own `ORDER BY`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OrderTerm {
    /// The buffered column holding the value.
    pub column: usize,
    /// Whether it orders downwards.
    pub descending: bool,
    /// The collation its text compares under.
    pub collation: Collation,
}

/// One window call, addressed entirely by buffered column numbers.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowCall {
    /// What it computes.
    pub func: WindowSlot,
    /// Whether `DISTINCT` was written.
    pub distinct: bool,
    /// The collation its comparisons use.
    pub collation: Collation,
    /// The columns holding its arguments.
    pub arguments: Vec<usize>,
    /// Bit `n` set when argument `n` was produced by a JSON function, for the
    /// JSON group aggregates.
    pub json_marks: u32,
    /// The column holding its `FILTER (WHERE ...)` value.
    pub filter: Option<usize>,
    /// The window's own `ORDER BY`.
    pub order: Vec<OrderTerm>,
    /// The frame.
    pub frame: WindowFrame,
}

/// Everything one window pass needs.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WindowPlan {
    /// The columns holding the partition keys, with their collations.
    pub partition: Vec<(usize, Collation)>,
    /// The calls, in the order their values are appended to each row.
    pub calls: Vec<WindowCall>,
}

/// Buffers its sorted input and appends one column per window call.
///
/// Each row grows by one value per call, in the order the calls were bound, so
/// whatever reads the output finds them at fixed column numbers.
pub struct Window {
    plan: WindowPlan,
    store: RowStore,
    downstream: Box<dyn Sink>,
}

impl Window {
    /// Returns a window operator.
    ///
    /// @param plan - the partition keys and the calls
    /// @param downstream - what to push the widened rows into
    pub fn new(plan: WindowPlan, downstream: Box<dyn Sink>) -> Window {
        Window {
            plan,
            store: RowStore::new(),
            downstream,
        }
    }
}

impl Sink for Window {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        self.store.absorb(batch)?;
        Ok(Flow::Continue)
    }

    fn finish(&mut self) -> DbResult<()> {
        let rows = std::mem::take(&mut self.store).into_rows();
        let widened = compute(&rows, &self.plan)?;
        emit_rows(&widened, self.downstream.as_mut())?;
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.store.clear();
        self.downstream.reset()
    }
}

/// Computes every window value for every row and appends them.
///
/// @param rows - the buffered input, already sorted
/// @param plan - the partition keys and the calls
pub fn compute(rows: &[Vec<OwnedDatum>], plan: &WindowPlan) -> DbResult<Vec<Vec<OwnedDatum>>> {
    let calls = plan.calls.len();
    let extra = compute_values(rows, plan)?;
    Ok(rows
        .iter()
        .enumerate()
        .map(|(row, held)| {
            let mut whole = Vec::with_capacity(held.len().saturating_add(calls));
            whole.extend(held.iter().cloned());
            let from = row.saturating_mul(calls);
            whole.extend(
                extra
                    .get(from..from.saturating_add(calls))
                    .unwrap_or(&[])
                    .iter()
                    .cloned(),
            );
            whole
        })
        .collect())
}

/// Computes every window value for every row, without the rows.
///
/// One list, row by row: row `r`'s value for call `c` is at `r * calls + c`.
///
/// **The values alone (task-2183).** [`compute`] hands back a copy of every
/// row with its values appended, and a caller that owns the rows and only
/// wants the values appended to them paid for that copy and for one small
/// list per row on the way: `row_number() OVER (ORDER BY key, id)` over 1,560
/// rows made six thousand allocations.
///
/// @param rows - the buffered input, already sorted
/// @param plan - the partition keys and the calls
pub fn compute_values(rows: &[Vec<OwnedDatum>], plan: &WindowPlan) -> DbResult<Vec<OwnedDatum>> {
    let total = rows.len();
    let calls = plan.calls.len();
    let mut extra: Vec<OwnedDatum> = vec![OwnedDatum::Null; total.saturating_mul(calls)];
    for (nth, call) in plan.calls.iter().enumerate() {
        let slot_of = |row: usize| row.saturating_mul(calls).saturating_add(nth);
        // Every call shares the partition boundaries but brings its own
        // `ORDER BY`, so the peer groups are its own.
        let partitions = frames::partitions(
            total,
            |left, right| same_partition(rows, &plan.partition, left, right),
            |left, right| same_order(rows, call, left, right),
            !call.order.is_empty(),
        );
        for partition in &partitions {
            let streamed = match (slides_a_sum(call), grows_from_the_start(call)) {
                (Some(kind), _) => Some(sliding_sums(rows, call, partition, kind)?),
                (None, Some(kind)) => Some(growing_aggregate(rows, call, partition, kind)?),
                (None, None) => None,
            };
            if let Some(values) = streamed {
                for (row, value) in (partition.start..partition.end).zip(values) {
                    if let Some(slot) = extra.get_mut(slot_of(row)) {
                        *slot = value;
                    }
                }
                continue;
            }
            for row in partition.start..partition.end {
                let value = evaluate(rows, call, partition, row)?;
                if let Some(slot) = extra.get_mut(slot_of(row)) {
                    *slot = value;
                }
            }
        }
    }
    Ok(extra)
}

/// Returns the accumulator kind when a call is `sum()`, `total()` or `avg()`
/// over a frame whose start moves, and `None` for every other call.
///
/// **SQLite computes such a frame by adding the rows that enter it and
/// removing the rows that leave, with one accumulator for the partition.**
/// What the accumulator remembers outlives the rows that caused it. Once a
/// value that is not an integer was added the total stays a real, so `sum(a)
/// OVER (ORDER BY a RANGE BETWEEN 1 PRECEDING AND CURRENT ROW)` over the texts
/// `'0x10'` and `'10'` is 10.0 on the second row although its frame holds only
/// `'10'`. Once the total reached infinity it stays infinite, so `sum(v) OVER
/// (ROWS 1 PRECEDING)` over 1e308, 1e308, 3 is Inf on the third row, where the
/// frame alone sums to 1e308. A frame that starts at `UNBOUNDED PRECEDING`
/// only grows, so the frame itself already holds every value the accumulator
/// saw. A frame with an `EXCLUDE` clause, and a `DISTINCT` call, are summed
/// afresh for every row in SQLite, so there the frame's own values decide.
///
/// @param call - the window call
fn slides_a_sum(call: &WindowCall) -> Option<AggregateKind> {
    let WindowSlot::Aggregate(kind) = &call.func else {
        return None;
    };
    let summed = matches!(
        kind,
        AggregateKind::Sum | AggregateKind::Total | AggregateKind::Average
    );
    let slides = !matches!(call.frame.start, FrameEnd::UnboundedPreceding)
        && call.frame.exclude == FrameExclude::NoOthers
        && !call.distinct;
    (summed && slides).then(|| kind.clone())
}

/// Computes a sliding `sum()`, `total()` or `avg()` for every row of one
/// partition, the way SQLite steps its accumulator.
///
/// Between two rows SQLite first takes out the rows that left the frame and
/// then adds the rows that entered it, each in partition order, and reads the
/// value. Rows the call's `FILTER` drops are never added and so never taken
/// out. A frame that is empty for every row, such as `ROWS BETWEEN 1
/// PRECEDING AND 2 PRECEDING`, adds nothing and reads a fresh accumulator,
/// which is what SQLite's reset of its frame table gives.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param partition - the partition
/// @param kind - `Sum`, `Total` or `Average`
fn sliding_sums(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    kind: AggregateKind,
) -> DbResult<Vec<OwnedDatum>> {
    let Some(column) = call.arguments.first().copied() else {
        return Ok(Vec::new());
    };
    let mut accumulator = Accumulator::new(kind);
    // The rows the accumulator holds, as a half open range of the partition.
    // **Both ends only move forward**, because the partition is sorted by the
    // window's order and every bound is a fixed distance from the row. So the
    // rows that leave are a run at the front and the rows that enter a run at
    // the back, and the whole partition costs one pass. Listing each row's
    // frame and comparing it with the last one cost the frame's length per
    // row: `sum(v) OVER (ORDER BY id ROWS 1000 PRECEDING)` over 100,000 rows
    // was ten times SQLite's time.
    let (mut held_start, mut held_end) = (partition.start, partition.start);
    let mut values = Vec::with_capacity(partition.end.saturating_sub(partition.start));
    for row in partition.start..partition.end {
        let (start, end) = match frame_bounds(rows, call, partition, row)? {
            Some((low, high)) => (low, high.saturating_add(1)),
            None => (held_end, held_end),
        };
        if start < held_start || end < held_end {
            return sliding_sums_by_row(rows, call, partition, column, values);
        }
        for leaving in held_start..start.min(held_end) {
            if passes_filter(rows, call, leaving) {
                accumulator.pull(&value_at(rows, leaving, column));
            }
        }
        for entering in start.max(held_end)..end {
            if passes_filter(rows, call, entering) {
                accumulator.push(&value_at(rows, entering, column));
            }
        }
        held_start = start;
        held_end = end.max(start);
        values.push(accumulator.finish()?);
    }
    Ok(values)
}

/// Finishes a sliding sum by recomputing each remaining row's frame.
///
/// The bounds of a sorted partition only move forward, so this is not
/// expected to run. It is here so that a frame which ever moved backwards
/// would be answered correctly and slowly rather than wrongly.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param partition - the partition
/// @param column - the summed argument's column
/// @param values - the answers already computed, extended in place
fn sliding_sums_by_row(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    column: usize,
    mut values: Vec<OwnedDatum>,
) -> DbResult<Vec<OwnedDatum>> {
    let first = partition.start.saturating_add(values.len());
    for row in first..partition.end {
        let WindowSlot::Aggregate(kind) = &call.func else {
            return Ok(values);
        };
        let mut accumulator = Accumulator::new(kind.clone());
        for member in frame_of(rows, call, partition, row)? {
            if passes_filter(rows, call, member) {
                accumulator.push(&value_at(rows, member, column));
            }
        }
        values.push(accumulator.finish()?);
    }
    Ok(values)
}

/// Returns the accumulator kind when a call is an aggregate over a frame that
/// starts at `UNBOUNDED PRECEDING`, with no `EXCLUDE` and no `DISTINCT`.
///
/// Such a frame only grows from one row to the next, so one accumulator for
/// the partition answers every row by adding the rows that entered. That is
/// the default frame of every ordered window, `RANGE BETWEEN UNBOUNDED
/// PRECEDING AND CURRENT ROW`, so it is the running total, the running count
/// and the running `max` of every report that has one.
///
/// @param call - the window call
fn grows_from_the_start(call: &WindowCall) -> Option<AggregateKind> {
    let WindowSlot::Aggregate(kind) = &call.func else {
        return None;
    };
    let grows = matches!(call.frame.start, FrameEnd::UnboundedPreceding)
        && call.frame.exclude == FrameExclude::NoOthers
        && !call.distinct;
    grows.then(|| kind.clone())
}

/// Computes an aggregate over a growing frame for every row of one partition.
///
/// **One accumulator, fed the rows as the frame reaches them.** This ran the
/// whole frame through a fresh accumulator for every row, so a running total
/// cost the square of the partition: `sum(v) OVER (ORDER BY id)` over 100,000
/// rows did not finish in a minute.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param partition - the partition
/// @param kind - the aggregate
fn growing_aggregate(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    kind: AggregateKind,
) -> DbResult<Vec<OwnedDatum>> {
    let whole_rows = keeps_whole_rows(&kind);
    let mut accumulator = Accumulator::new(kind);
    accumulator.compare_under(call.collation);
    let mut added = partition.start;
    let mut values = Vec::with_capacity(partition.end.saturating_sub(partition.start));
    for row in partition.start..partition.end {
        let end = match frame_bounds(rows, call, partition, row)? {
            Some((_, high)) => high.saturating_add(1),
            None => partition.start,
        };
        if end < added {
            // A frame that shrank, which a sorted partition does not give.
            values.push(aggregate(rows, call, partition, row, call_kind(call))?);
            continue;
        }
        for member in added..end {
            if passes_filter(rows, call, member) {
                feed(&mut accumulator, rows, call, member, whole_rows)?;
            }
        }
        added = end;
        values.push(accumulator.finish()?);
    }
    Ok(values)
}

/// Returns a window call's aggregate kind, or `count(*)`'s when it has none.
///
/// @param call - the window call
fn call_kind(call: &WindowCall) -> AggregateKind {
    match &call.func {
        WindowSlot::Aggregate(kind) => kind.clone(),
        WindowSlot::Plain(_) => AggregateKind::CountStar,
    }
}

/// Reports whether an aggregate keeps each row's arguments whole rather than
/// folding the first one: a JSON group aggregate, whose NULL is a member
/// rather than a row to skip, a computed separator, and the percentile
/// family's fraction.
///
/// @param kind - the aggregate
fn keeps_whole_rows(kind: &AggregateKind) -> bool {
    matches!(
        kind,
        AggregateKind::JsonGroupArray(_)
            | AggregateKind::JsonGroupObject(_)
            | AggregateKind::GroupConcatComputed
            | AggregateKind::Percentile(_)
    )
}

/// Adds one frame member to an accumulator.
///
/// @param accumulator - the accumulator
/// @param rows - the buffered input
/// @param call - the window call
/// @param member - the row being added
/// @param whole_rows - whether the aggregate keeps every argument
fn feed(
    accumulator: &mut Accumulator,
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    member: usize,
    whole_rows: bool,
) -> DbResult<()> {
    if whole_rows {
        let values = call
            .arguments
            .iter()
            .map(|column| Value::from(&value_at(rows, member, *column)).into_owned())
            .collect::<DbResult<Vec<_>>>()?;
        accumulator.push_values_marked(values, call.json_marks);
        return Ok(());
    }
    let value = match call.arguments.first() {
        Some(column) => value_at(rows, member, *column),
        // `count(*)` has no argument and counts the row itself.
        None => Datum::Int(1),
    };
    accumulator.push(&value);
    Ok(())
}

/// Returns one value of one row, or NULL when the column is not there.
fn value_at<'r>(rows: &'r [Vec<OwnedDatum>], row: usize, column: usize) -> Datum<'r> {
    rows.get(row)
        .and_then(|values| values.get(column))
        .map(OwnedDatum::borrow)
        .unwrap_or(Datum::Null)
}

/// Returns whether two values are the same for peer purposes.
///
/// Two NULLs are peers. They are not equal, but an `ORDER BY` cannot separate
/// them, and a peer group is defined by what the ordering can distinguish.
fn peers(left: &Datum<'_>, right: &Datum<'_>, collation: Collation) -> bool {
    if matches!(left, Datum::Null) || matches!(right, Datum::Null) {
        return matches!(left, Datum::Null) && matches!(right, Datum::Null);
    }
    compare_under(left, right, collation) == std::cmp::Ordering::Equal
}

/// Returns whether two rows share every partition key.
fn same_partition(
    rows: &[Vec<OwnedDatum>],
    key: &[(usize, Collation)],
    left: usize,
    right: usize,
) -> bool {
    key.iter().all(|(column, collation)| {
        peers(
            &value_at(rows, left, *column),
            &value_at(rows, right, *column),
            *collation,
        )
    })
}

/// Returns whether the window's `ORDER BY` can tell two rows apart.
fn same_order(rows: &[Vec<OwnedDatum>], call: &WindowCall, left: usize, right: usize) -> bool {
    call.order.iter().all(|term| {
        peers(
            &value_at(rows, left, term.column),
            &value_at(rows, right, term.column),
            term.collation,
        )
    })
}

/// Returns whether a row passes the call's `FILTER (WHERE ...)`.
fn passes_filter(rows: &[Vec<OwnedDatum>], call: &WindowCall, row: usize) -> bool {
    let Some(column) = call.filter else {
        return true;
    };
    match value_at(rows, row, column) {
        Datum::Null => false,
        Datum::Int(value) => value != 0,
        Datum::Real(value) => value != 0.0,
        // A `FILTER` value the projection left as text or a blob is falsy the
        // way SQLite's truth test makes it falsy, via a numeric reading.
        other => integer_of(&other) != 0,
    }
}

/// Returns a value's integer reading, the way a truth test or an offset needs it.
fn integer_of(value: &Datum<'_>) -> i64 {
    match value {
        Datum::Int(held) => *held,
        Datum::Real(held) => *held as i64,
        Datum::Null => 0,
        other => inillucent_value::cast::integer_value(&Value::from(other)),
    }
}

/// Returns a value's double reading, for a `RANGE` offset.
fn real_of(value: &Datum<'_>) -> f64 {
    match value {
        Datum::Int(held) => *held as f64,
        Datum::Real(held) => *held,
        Datum::Null => 0.0,
        other => inillucent_value::cast::real_value(&Value::from(other)),
    }
}

/// Returns an ordering value's double reading, or `None` when it is NULL.
///
/// **NULL is not a number and a `RANGE` offset cannot measure a distance to
/// it (task-1913).** Reading it as `0.0` through `real_of` made every NULL row
/// sit one unit from zero, so `RANGE BETWEEN 1 PRECEDING AND 1 FOLLOWING` drew
/// the NULL rows into the frame of every row near zero and drew the numbers
/// beside them into the NULL rows' own frames. `frames::range_bound` takes the
/// `None` and resolves such a row to its peer group instead, which is what
/// SQLite answers.
///
/// A text or blob value is not a number either, but it is not NULL: SQLite
/// leaves it unoffset, so its frame is its peer group, and it sorts above
/// every number. It is passed as NaN, which `frames::range_bound` reads that
/// way. Reading `'9'` as the number 9 put it inside the frames of numbers and
/// of other text that SQLite keeps apart.
///
/// @param value - the row's ordering value
fn real_or_null(value: &Datum<'_>) -> Option<f64> {
    match value {
        Datum::Null => None,
        Datum::Text(_) | Datum::Blob(_) => Some(f64::NAN),
        other => Some(real_of(other)),
    }
}

/// Computes one window call for one row.
///
/// @param rows - the buffered input
/// @param call - the call
/// @param partition - the row's partition, with its peer groups
/// @param row - the row
fn evaluate(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    row: usize,
) -> DbResult<OwnedDatum> {
    Ok(match &call.func {
        WindowSlot::Plain(WindowFunc::RowNumber) => {
            OwnedDatum::Int(frames::row_number(partition, row))
        }
        WindowSlot::Plain(WindowFunc::Rank) => OwnedDatum::Int(frames::rank(partition, row)),
        WindowSlot::Plain(WindowFunc::DenseRank) => {
            OwnedDatum::Int(frames::dense_rank(partition, row))
        }
        WindowSlot::Plain(WindowFunc::PercentRank) => {
            OwnedDatum::Real(frames::percent_rank(partition, row))
        }
        WindowSlot::Plain(WindowFunc::CumeDist) => {
            OwnedDatum::Real(frames::cume_dist(partition, row))
        }
        WindowSlot::Plain(WindowFunc::Ntile) => ntile(rows, call, partition, row)?,
        WindowSlot::Plain(WindowFunc::Lag) => offset_row(rows, call, partition, row, true),
        WindowSlot::Plain(WindowFunc::Lead) => offset_row(rows, call, partition, row, false),
        WindowSlot::Plain(WindowFunc::FirstValue)
        | WindowSlot::Plain(WindowFunc::LastValue)
        | WindowSlot::Plain(WindowFunc::NthValue)
            if call.frame.exclude == FrameExclude::NoOthers =>
        {
            // The frame is one run of rows, so the member wanted is found from
            // its two ends. Listing the frame cost its length per row, which
            // made `last_value(x) OVER (ORDER BY y)`, whose default frame
            // grows with the partition, quadratic.
            let bounds = frame_bounds(rows, call, partition, row)?;
            let run: Vec<usize> = match bounds {
                Some((low, high)) => match &call.func {
                    WindowSlot::Plain(WindowFunc::FirstValue) => vec![low],
                    WindowSlot::Plain(WindowFunc::LastValue) => vec![high],
                    _ => (low..=high).take(nth_wanted(rows, call, row)?).collect(),
                },
                None => Vec::new(),
            };
            positional(rows, call, &run, row)?
        }
        WindowSlot::Plain(WindowFunc::FirstValue)
        | WindowSlot::Plain(WindowFunc::LastValue)
        | WindowSlot::Plain(WindowFunc::NthValue) => {
            let frame = frame_of(rows, call, partition, row)?;
            positional(rows, call, &frame, row)?
        }
        WindowSlot::Aggregate(kind) => aggregate(rows, call, partition, row, kind.clone())?,
    })
}

/// Computes `ntile(n)`.
///
/// **A bucket count of zero or less is an error, not a NULL (task-1979,
/// F17).** `frames::ntile` answers `None` for a count it cannot divide a
/// partition into, and this read that `None` as "no answer for this row" and
/// returned NULL for every row of the query. SQLite refuses the statement with
/// `argument of ntile must be a positive integer`, so a caller that passed a
/// count it computed hears about it rather than reading a column of NULLs as
/// data.
///
/// @param rows - the buffered input
/// @param call - the call, whose first argument is the bucket count
/// @param partition - the row's partition, with its peer groups
/// @param row - the row
fn ntile(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    row: usize,
) -> DbResult<OwnedDatum> {
    let Some(column) = call.arguments.first() else {
        return Ok(OwnedDatum::Null);
    };
    let buckets = integer_of(&value_at(rows, row, *column));
    match frames::ntile(partition, row, buckets) {
        Some(bucket) => Ok(OwnedDatum::Int(bucket)),
        None => Err(inillucent_base::error::statement_refusal(
            "argument of ntile must be a positive integer",
        )),
    }
}

/// Computes `lag` or `lead`, which read the partition and ignore the frame.
fn offset_row(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    row: usize,
    backwards: bool,
) -> OwnedDatum {
    let Some(value_column) = call.arguments.first() else {
        return OwnedDatum::Null;
    };
    let offset = match call.arguments.get(1) {
        Some(column) => integer_of(&value_at(rows, row, *column)),
        None => 1,
    };
    match frames::offset_row(partition, row, offset, backwards) {
        Some(target) => OwnedDatum::from_datum(&value_at(rows, target, *value_column)),
        // Off the end of the partition, so the default if one was written.
        None => match call.arguments.get(2) {
            Some(column) => OwnedDatum::from_datum(&value_at(rows, row, *column)),
            None => OwnedDatum::Null,
        },
    }
}

/// Returns how many leading frame members `nth_value` needs: its position.
///
/// @param rows - the buffered input
/// @param call - the `nth_value` call
/// @param row - the row
fn nth_wanted(rows: &[Vec<OwnedDatum>], call: &WindowCall, row: usize) -> DbResult<usize> {
    let Some(column) = call.arguments.get(1) else {
        return Ok(0);
    };
    Ok(checked_argument(&value_at(rows, row, *column), Argument::NthValue)? as usize)
}

/// Computes `first_value`, `last_value` or `nth_value` over a frame.
fn positional(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    frame: &[usize],
    row: usize,
) -> DbResult<OwnedDatum> {
    let Some(value_column) = call.arguments.first() else {
        return Ok(OwnedDatum::Null);
    };
    let picked = match &call.func {
        WindowSlot::Plain(WindowFunc::FirstValue) => frame.first().copied(),
        WindowSlot::Plain(WindowFunc::LastValue) => frame.last().copied(),
        WindowSlot::Plain(WindowFunc::NthValue) => {
            let Some(column) = call.arguments.get(1) else {
                return Ok(OwnedDatum::Null);
            };
            let nth = checked_argument(&value_at(rows, row, *column), Argument::NthValue)? as usize;
            frame.get(nth.saturating_sub(1)).copied()
        }
        _ => None,
    };
    Ok(match picked {
        Some(member) => OwnedDatum::from_datum(&value_at(rows, member, *value_column)),
        None => OwnedDatum::Null,
    })
}

/// Runs an aggregate over one row's frame.
///
/// `DISTINCT` is applied here rather than inside the accumulator, because a
/// window aggregate's distinctness is per frame: the same value can be counted
/// once in this row's frame and once again in the next row's.
fn aggregate(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    row: usize,
    kind: AggregateKind,
) -> DbResult<OwnedDatum> {
    let frame = frame_of(rows, call, partition, row)?;
    let whole_rows = keeps_whole_rows(&kind);
    let mut accumulator = Accumulator::new(kind);
    // `min` and `max` over a frame compare the way they do over a group.
    accumulator.compare_under(call.collation);
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    for member in frame {
        if !passes_filter(rows, call, member) {
            continue;
        }
        let value = match call.arguments.first() {
            Some(column) => value_at(rows, member, *column),
            // `count(*)` has no argument and counts the row itself.
            None => Datum::Int(1),
        };
        if call.distinct {
            let mut encoded = Vec::new();
            key::encode_into_with(&value, call.collation, &mut encoded);
            if !seen.insert(encoded) {
                continue;
            }
        }
        feed(&mut accumulator, rows, call, member, whole_rows)?;
    }
    accumulator.finish()
}

/// Returns the rows of one row's frame, in partition order.
///
/// Reads the offset expressions out of their columns and hands the resolved
/// frame to the shared arithmetic.
fn frame_of(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    row: usize,
) -> DbResult<Vec<usize>> {
    let spec = frame_spec(rows, call, row)?;
    let descending = call.order.first().is_some_and(|term| term.descending);
    Ok(frames::frame(
        partition,
        row,
        &spec,
        |member| order_value(rows, call, member),
        descending,
    ))
}

/// Returns the first and last row of one row's frame before `EXCLUDE`, or
/// `None` when the frame is empty. See `frames::frame_bounds`.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param partition - the row's partition
/// @param row - the row whose frame it is
fn frame_bounds(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    row: usize,
) -> DbResult<Option<(usize, usize)>> {
    let spec = frame_spec(rows, call, row)?;
    let descending = call.order.first().is_some_and(|term| term.descending);
    Ok(frames::frame_bounds(
        partition,
        row,
        &spec,
        |member| order_value(rows, call, member),
        descending,
    ))
}

/// Resolves one row's frame specification, evaluating its offsets.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param row - the row whose frame it is
fn frame_spec(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    row: usize,
) -> DbResult<frames::FrameSpec> {
    let unit = call.frame.unit;
    Ok(frames::FrameSpec {
        unit,
        start: resolve(rows, call.frame.start, row, unit, true)?,
        end: resolve(rows, call.frame.end, row, unit, false)?,
        exclude: call.frame.exclude,
    })
}

/// Returns a row's single `ORDER BY` value, for a `RANGE` offset.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param member - the row
fn order_value(rows: &[Vec<OwnedDatum>], call: &WindowCall, member: usize) -> Option<f64> {
    match call.order.first().map(|term| term.column) {
        Some(column) => real_or_null(&value_at(rows, member, column)),
        // No ordering term at all, so no `RANGE` offset can be resolved
        // against one. Every row reads alike, which is what the frame
        // arithmetic did before there was a NULL to tell apart.
        None => Some(0.0),
    }
}

/// Reads one end of a frame into the shared form, evaluating its offset.
///
/// @param rows - the buffered input
/// @param bound - the end as the binder left it
/// @param row - the row whose frame it is
/// @param unit - `ROWS`, `RANGE` or `GROUPS`
/// @param is_start - whether this is the frame's start
fn resolve(
    rows: &[Vec<OwnedDatum>],
    bound: FrameEnd,
    row: usize,
    unit: FrameUnit,
    is_start: bool,
) -> DbResult<frames::Bound> {
    let FrameEnd::Offset { column, preceding } = bound else {
        return Ok(match bound {
            FrameEnd::UnboundedPreceding => frames::Bound::UnboundedPreceding,
            FrameEnd::UnboundedFollowing => frames::Bound::UnboundedFollowing,
            _ => frames::Bound::CurrentRow,
        });
    };
    let value = value_at(rows, row, column);
    let range = unit == FrameUnit::Range;
    let argument = match (range, is_start) {
        (true, true) => Argument::RangeStart,
        (true, false) => Argument::RangeEnd,
        (false, true) => Argument::RowsStart,
        (false, false) => Argument::RowsEnd,
    };
    let distance = checked_argument(&value, argument)?;
    Ok(if range {
        frames::Bound::RangeOffset {
            distance,
            preceding,
        }
    } else {
        frames::Bound::Offset {
            distance: distance as i64,
            preceding,
        }
    })
}

/// A window argument SQLite checks before it uses it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Argument {
    /// A `ROWS` or `GROUPS` frame's starting offset.
    RowsStart,
    /// A `ROWS` or `GROUPS` frame's ending offset.
    RowsEnd,
    /// A `RANGE` frame's starting offset.
    RangeStart,
    /// A `RANGE` frame's ending offset.
    RangeEnd,
    /// The second argument of `nth_value`.
    NthValue,
}

/// Checks a frame offset or an `nth_value` position the way SQLite does.
///
/// **A bad value is an error, never a guess.** A negative, fractional, NULL or
/// non numeric offset was read as an integer here, so `ROWS BETWEEN 1.5
/// PRECEDING` was one row, `-1 FOLLOWING` an empty frame, and `nth_value(a, 0)`
/// a column of NULLs. SQLite refuses each with a message naming the argument.
/// A `ROWS` or `GROUPS` offset and the `nth_value` position must be integers
/// after numeric affinity, so `'2'` and `2.0` pass; a `RANGE` offset may be any
/// number.
///
/// @param value - the evaluated argument
/// @param argument - which argument it is
fn checked_argument(value: &Datum<'_>, argument: Argument) -> DbResult<f64> {
    let message = match argument {
        Argument::RowsStart => "frame starting offset must be a non-negative integer",
        Argument::RowsEnd => "frame ending offset must be a non-negative integer",
        Argument::RangeStart => "frame starting offset must be a non-negative number",
        Argument::RangeEnd => "frame ending offset must be a non-negative number",
        Argument::NthValue => "second argument to nth_value must be a positive integer",
    };
    let range = matches!(argument, Argument::RangeStart | Argument::RangeEnd);
    let number = match inillucent_value::affinity::apply_numeric_affinity(Value::from(value), true)
    {
        Value::Integer(held) => Some(held as f64),
        Value::Real(held) if range || (held.fract() == 0.0 && held.abs() < 9.2e18) => Some(held),
        _ => None,
    };
    let floor = if argument == Argument::NthValue {
        1.0
    } else {
        0.0
    };
    match number {
        Some(held) if held >= floor => Ok(held),
        _ => Err(inillucent_base::error::statement_refusal(message)),
    }
}

/// Returns the aggregate kind a bound aggregate function maps to.
///
/// The window binder names its aggregates with `inillucent_sql`'s enum and the
/// executor's accumulator takes its own, because the executor's carries the
/// separator `group_concat` was written with.
///
/// @param func - the bound function
/// @param separator - the separator, for `group_concat`
pub fn kind_of(func: AggregateFunc, separator: &str) -> Option<AggregateKind> {
    Some(match func {
        AggregateFunc::Count => AggregateKind::Count,
        AggregateFunc::Sum => AggregateKind::Sum,
        AggregateFunc::Total => AggregateKind::Total,
        AggregateFunc::Avg => AggregateKind::Average,
        AggregateFunc::Min => AggregateKind::Minimum,
        AggregateFunc::Max => AggregateKind::Maximum,
        AggregateFunc::GroupConcat => AggregateKind::GroupConcat(separator.to_string()),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plain window call over one partition with one ordering term.
    ///
    /// @param func - what it computes
    /// @param arguments - the columns its arguments are in
    fn call(func: WindowSlot, arguments: Vec<usize>) -> WindowCall {
        WindowCall {
            func,
            distinct: false,
            collation: Collation::Binary,
            arguments,
            json_marks: 0,
            filter: None,
            order: vec![OrderTerm {
                column: 0,
                descending: false,
                collation: Collation::Binary,
            }],
            frame: WindowFrame::default(),
        }
    }

    /// Builds rows of `(order key, payload)`.
    ///
    /// @param keys - the ordering key of each row
    fn rows_of(keys: &[i64]) -> Vec<Vec<OwnedDatum>> {
        keys.iter()
            .enumerate()
            .map(|(nth, key)| vec![OwnedDatum::Int(*key), OwnedDatum::Int(nth as i64 * 10)])
            .collect()
    }

    /// Runs one call and returns the appended column.
    ///
    /// @param rows - the buffered input
    /// @param plan - the plan
    fn appended(rows: &[Vec<OwnedDatum>], plan: &WindowPlan) -> Vec<OwnedDatum> {
        let widened = compute(rows, plan).expect("the pass succeeds");
        widened
            .iter()
            .map(|row| row.last().cloned().unwrap_or(OwnedDatum::Null))
            .collect()
    }

    /// Reads a column of integers out of an answer.
    fn ints(values: &[OwnedDatum]) -> Vec<i64> {
        values
            .iter()
            .map(|value| match value {
                OwnedDatum::Int(held) => *held,
                _ => i64::MIN,
            })
            .collect()
    }

    #[test]
    fn row_number_and_rank_differ_exactly_where_there_are_ties() {
        let rows = rows_of(&[1, 1, 2]);
        let numbers = WindowPlan {
            partition: Vec::new(),
            calls: vec![call(WindowSlot::Plain(WindowFunc::RowNumber), Vec::new())],
        };
        assert_eq!(ints(&appended(&rows, &numbers)), vec![1, 2, 3]);
        let ranks = WindowPlan {
            partition: Vec::new(),
            calls: vec![call(WindowSlot::Plain(WindowFunc::Rank), Vec::new())],
        };
        assert_eq!(ints(&appended(&rows, &ranks)), vec![1, 1, 3]);
    }

    #[test]
    fn a_running_sum_uses_the_default_range_frame() {
        let rows = rows_of(&[1, 1, 2]);
        let plan = WindowPlan {
            partition: Vec::new(),
            calls: vec![call(WindowSlot::Aggregate(AggregateKind::Sum), vec![1])],
        };
        // Rows 0 and 1 are peers, so the default RANGE frame gives both the
        // whole peer group: 0 + 10 = 10. Under ROWS row 0 would have seen 0.
        assert_eq!(ints(&appended(&rows, &plan)), vec![10, 10, 30]);
    }

    #[test]
    fn a_rows_frame_slides() {
        let rows = rows_of(&[1, 2, 3]);
        let mut spec = call(WindowSlot::Aggregate(AggregateKind::Sum), vec![1]);
        spec.frame = WindowFrame {
            unit: FrameUnit::Rows,
            start: FrameEnd::UnboundedPreceding,
            end: FrameEnd::CurrentRow,
            exclude: FrameExclude::NoOthers,
        };
        let plan = WindowPlan {
            partition: Vec::new(),
            calls: vec![spec],
        };
        assert_eq!(ints(&appended(&rows, &plan)), vec![0, 10, 30]);
    }

    #[test]
    fn a_partition_key_stops_the_window_reaching_across() {
        let rows = vec![
            vec![OwnedDatum::Int(1), OwnedDatum::Int(5), OwnedDatum::Int(1)],
            vec![OwnedDatum::Int(2), OwnedDatum::Int(5), OwnedDatum::Int(1)],
            vec![OwnedDatum::Int(3), OwnedDatum::Int(9), OwnedDatum::Int(1)],
        ];
        let mut spec = call(WindowSlot::Plain(WindowFunc::RowNumber), Vec::new());
        spec.order = vec![OrderTerm {
            column: 0,
            descending: false,
            collation: Collation::Binary,
        }];
        let plan = WindowPlan {
            // Column 1 is the partition key: rows 0 and 1 share it.
            partition: vec![(1, Collation::Binary)],
            calls: vec![spec],
        };
        assert_eq!(ints(&appended(&rows, &plan)), vec![1, 2, 1]);
    }

    #[test]
    fn lag_falls_back_to_its_default_at_the_partition_edge() {
        let rows = rows_of(&[1, 2, 3]);
        let mut spec = call(WindowSlot::Plain(WindowFunc::Lag), vec![1]);
        spec.arguments = vec![1];
        let plan = WindowPlan {
            partition: Vec::new(),
            calls: vec![spec],
        };
        let answer = appended(&rows, &plan);
        assert!(matches!(answer.first(), Some(OwnedDatum::Null)));
        assert_eq!(ints(&answer[1..]), vec![0, 10]);
    }

    #[test]
    fn a_filter_drops_rows_from_the_frame_and_not_from_the_output() {
        let rows = rows_of(&[1, 2, 3]);
        let mut spec = call(WindowSlot::Aggregate(AggregateKind::CountStar), Vec::new());
        spec.frame = WindowFrame {
            unit: FrameUnit::Rows,
            start: FrameEnd::UnboundedPreceding,
            end: FrameEnd::UnboundedFollowing,
            exclude: FrameExclude::NoOthers,
        };
        // The payload column is 0, 10, 20; filtering on it drops only row 0.
        spec.filter = Some(1);
        let plan = WindowPlan {
            partition: Vec::new(),
            calls: vec![spec],
        };
        let answer = appended(&rows, &plan);
        assert_eq!(answer.len(), 3, "every input row still produces a row");
        assert_eq!(ints(&answer), vec![2, 2, 2]);
    }

    #[test]
    fn distinct_is_per_frame_rather_than_per_partition() {
        let rows = vec![
            vec![OwnedDatum::Int(1), OwnedDatum::Int(7)],
            vec![OwnedDatum::Int(2), OwnedDatum::Int(7)],
            vec![OwnedDatum::Int(3), OwnedDatum::Int(8)],
        ];
        let mut spec = call(WindowSlot::Aggregate(AggregateKind::Count), vec![1]);
        spec.distinct = true;
        spec.frame = WindowFrame {
            unit: FrameUnit::Rows,
            start: FrameEnd::UnboundedPreceding,
            end: FrameEnd::CurrentRow,
            exclude: FrameExclude::NoOthers,
        };
        let plan = WindowPlan {
            partition: Vec::new(),
            calls: vec![spec],
        };
        // The 7 repeats, so the second frame still has one distinct value.
        assert_eq!(ints(&appended(&rows, &plan)), vec![1, 1, 2]);
    }

    #[test]
    fn two_calls_append_in_the_order_they_were_bound() {
        let rows = rows_of(&[1, 2]);
        let plan = WindowPlan {
            partition: Vec::new(),
            calls: vec![
                call(WindowSlot::Plain(WindowFunc::RowNumber), Vec::new()),
                call(WindowSlot::Aggregate(AggregateKind::Sum), vec![1]),
            ],
        };
        let widened = compute(&rows, &plan).expect("the pass succeeds");
        assert_eq!(widened[1].len(), 4, "two input columns plus two calls");
        assert_eq!(ints(&widened[1][2..]), vec![2, 10]);
    }

    #[test]
    fn an_empty_input_produces_no_rows_rather_than_a_panic() {
        let plan = WindowPlan {
            partition: Vec::new(),
            calls: vec![call(WindowSlot::Plain(WindowFunc::Rank), Vec::new())],
        };
        assert!(compute(&[], &plan).expect("the pass succeeds").is_empty());
    }

    #[test]
    fn nulls_are_peers_of_each_other_and_of_nothing_else() {
        let rows = vec![
            vec![OwnedDatum::Null, OwnedDatum::Int(0)],
            vec![OwnedDatum::Null, OwnedDatum::Int(10)],
            vec![OwnedDatum::Int(1), OwnedDatum::Int(20)],
        ];
        let plan = WindowPlan {
            partition: Vec::new(),
            calls: vec![call(WindowSlot::Plain(WindowFunc::Rank), Vec::new())],
        };
        assert_eq!(ints(&appended(&rows, &plan)), vec![1, 1, 3]);
    }

    #[test]
    fn the_aggregate_names_map_onto_the_executors_kinds() {
        assert_eq!(kind_of(AggregateFunc::Sum, ""), Some(AggregateKind::Sum));
        assert_eq!(
            kind_of(AggregateFunc::GroupConcat, "-"),
            Some(AggregateKind::GroupConcat("-".to_string()))
        );
    }
}

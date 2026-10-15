//! The window pass as an operator that computes one partition at a time.
//!
//! Invariant: **a window value is computed from exactly the rows [`compute_values_until_failure`]
//! would compute it from, and a partition's rows are emitted before the next partition's are
//! read.** The pass used to buffer every row of the input, compute every value, and only then hand
//! a row on. A partition is the unit a window function reads (it never looks at another
//! partition's rows), so the operator holds one partition, computes it with the same functions
//! that computed the whole input, and emits it. Memory is the largest partition, not the table.
//!
//! ## A partition that does not fit
//!
//! Some calls read nothing but the rows before them and the peer group they sit in: `row_number`,
//! `rank`, `dense_rank`, and an aggregate over the frame every ordered window gets by default (from
//! the start of the partition to the current row). When every call of the window is one of those,
//! a partition past the byte limit is computed in pieces. Each piece is a run of whole peer groups;
//! the operator keeps a count of the rows and groups before it and an accumulator for each
//! aggregate, so the next piece carries on from where this one stopped. `row_number() OVER (ORDER
//! BY a)` over three million rows holds one peer group at a time.
//!
//! Any other call (`ntile`, `lead`, a frame with a following offset, `last_value` over a whole
//! partition, a `DISTINCT` aggregate) reads rows after the current one or counts the whole
//! partition, and a partition of such a window is held whole.

use super::*;
use crate::ops::resident_row_bytes;

/// How many bytes of a partition are held before it is computed in pieces, when it can be.
///
/// 32 MiB, which leaves the operators below a window (the sort that orders its input) and above it
/// (the projection and the collector) room in the 64 MiB a sort holds.
const WINDOW_BYTES: u64 = 32 * 1024 * 1024;

/// How many pieces of a partition have been computed in this process.
///
/// So a test can say that a window pass reached the code that computes a partition in pieces.
static PIECES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Returns how many pieces of a partition have been computed since the process started.
pub fn pieces_computed() -> u64 {
    PIECES.load(std::sync::atomic::Ordering::Relaxed)
}

/// Where a window pass's rows sit in the row it handles.
///
/// A row is the values the pass reads, then one slot for every window call of the statement (NULL
/// until the pass of its window fills it), then, when the statement has windows that need different
/// orders, the number the row arrived with.
#[derive(Clone, Copy, Debug)]
pub struct StreamShape {
    /// How many values the pass reads.
    pub width: usize,
    /// How many window calls the statement has, in all its windows.
    pub total_slots: usize,
    /// Whether this is the first pass, which makes the slots and the arrival number.
    pub first: bool,
    /// Whether rows carry an arrival number after their slots.
    pub carries_seq: bool,
    /// Whether the rows before a failing row are handed on before the failure is reported.
    pub partial_rows_on_failure: bool,
}

/// What a piece of a partition leaves for the next piece.
struct Carry {
    /// One entry for each call of the window.
    calls: Vec<CallCarry>,
}

/// What one call carries from a piece to the next.
enum CallCarry {
    /// `row_number`: how many rows came before.
    RowNumber(i64),
    /// `rank`: how many rows came before.
    Rank(i64),
    /// `dense_rank`: how many peer groups came before.
    DenseRank(i64),
    /// An aggregate over a growing frame: the accumulator, holding every row before.
    Growing(Box<Accumulator>),
}

/// Reports whether every call of a window can be computed in pieces.
///
/// @param plan - the partition keys and the calls
fn computes_in_pieces(plan: &WindowPlan) -> bool {
    !plan.calls.is_empty() && plan.calls.iter().all(call_computes_in_pieces)
}

/// Reports whether one call can be computed in pieces.
///
/// @param call - the call
fn call_computes_in_pieces(call: &WindowCall) -> bool {
    match &call.func {
        WindowSlot::Plain(WindowFunc::RowNumber | WindowFunc::Rank | WindowFunc::DenseRank) => true,
        WindowSlot::Plain(_) => false,
        WindowSlot::Aggregate(_) => {
            grows_from_the_start(call).is_some() && call.frame.end == FrameEnd::CurrentRow
        }
    }
}

/// Reports whether a window needs no peer groups to compute in pieces: it has no `ORDER BY` and
/// only numbers rows.
///
/// @param plan - the partition keys and the calls
fn pieces_need_no_peers(plan: &WindowPlan) -> bool {
    plan.calls.iter().all(|call| {
        call.order.is_empty() && matches!(call.func, WindowSlot::Plain(WindowFunc::RowNumber))
    })
}

/// Returns what a piece carries before the first piece.
///
/// @param plan - the partition keys and the calls
fn fresh_carry(plan: &WindowPlan) -> Carry {
    Carry {
        calls: plan
            .calls
            .iter()
            .map(|call| match &call.func {
                WindowSlot::Plain(WindowFunc::RowNumber) => CallCarry::RowNumber(0),
                WindowSlot::Plain(WindowFunc::Rank) => CallCarry::Rank(0),
                WindowSlot::Plain(WindowFunc::DenseRank) => CallCarry::DenseRank(0),
                WindowSlot::Aggregate(kind) => {
                    let mut accumulator = Accumulator::new(kind.clone());
                    accumulator.compare_under(call.collation);
                    CallCarry::Growing(Box::new(accumulator))
                }
                WindowSlot::Plain(_) => CallCarry::RowNumber(0),
            })
            .collect(),
    }
}

/// Computes every window value of one piece of a partition, and what the next piece carries.
///
/// **The same values the whole partition would get.** The piece is run of whole peer groups, so the
/// peer groups and the frame ends inside it are the ones the whole partition has; what lies before
/// it is in `carry`.
///
/// @param rows - the piece
/// @param plan - the partition keys and the calls
/// @param carry - what the pieces before this one left
fn compute_piece(
    rows: &[Vec<OwnedDatum>],
    plan: &WindowPlan,
    carry: &mut Carry,
) -> (Vec<OwnedDatum>, Option<WindowFailure>) {
    let total = rows.len();
    let calls = plan.calls.len();
    let mut extra: Vec<OwnedDatum> = vec![OwnedDatum::Null; total.saturating_mul(calls)];
    let mut failure: Option<WindowFailure> = None;
    for (nth, (call, carried)) in plan.calls.iter().zip(carry.calls.iter_mut()).enumerate() {
        let partition = frames::partitions(
            total,
            |_, _| true,
            |left, right| same_order(rows, call, left, right),
            !call.order.is_empty(),
        );
        let Some(partition) = partition.first() else {
            continue;
        };
        let mut values: Vec<OwnedDatum> = Vec::with_capacity(total);
        let mut failed: Option<inillucent_base::DbError> = None;
        match carried {
            CallCarry::RowNumber(before) => {
                for row in partition.start..partition.end {
                    values.push(OwnedDatum::Int(
                        frames::row_number(partition, row).saturating_add(*before),
                    ));
                }
                *before = before.saturating_add(total as i64);
            }
            CallCarry::Rank(before) => {
                for row in partition.start..partition.end {
                    values.push(OwnedDatum::Int(
                        frames::rank(partition, row).saturating_add(*before),
                    ));
                }
                *before = before.saturating_add(total as i64);
            }
            CallCarry::DenseRank(before) => {
                for row in partition.start..partition.end {
                    values.push(OwnedDatum::Int(
                        frames::dense_rank(partition, row).saturating_add(*before),
                    ));
                }
                *before = before.saturating_add(partition.group_starts.len() as i64);
            }
            CallCarry::Growing(accumulator) => {
                failed =
                    growing_aggregate_carrying(rows, call, partition, accumulator, &mut values)
                        .err();
            }
        }
        let computed = values.len();
        for (row, value) in (partition.start..partition.end).zip(values) {
            if let Some(slot) = extra.get_mut(row.saturating_mul(calls).saturating_add(nth)) {
                *slot = value;
            }
        }
        if let Some(error) = failed {
            if failure.as_ref().is_none_or(|held| computed < held.before) {
                failure = Some(WindowFailure {
                    before: computed,
                    error,
                });
            }
        }
    }
    (extra, failure)
}

/// A window pass that computes one partition at a time.
///
/// Rows must arrive sorted by the partition keys and then by the window's `ORDER BY`. Each
/// partition is computed when the next one starts, or when the input ends, and its rows are pushed
/// downstream with the pass's values in their slots.
pub struct WindowStream {
    /// The partition keys and the calls of this pass.
    plan: WindowPlan,
    /// The slot each call's value goes to.
    slots: Vec<usize>,
    /// Where the rows' parts are.
    shape: StreamShape,
    /// The rows of the partition being read.
    rows: Vec<Vec<OwnedDatum>>,
    /// Roughly how many bytes they occupy.
    held: u64,
    /// How many bytes a partition may occupy before it is computed in pieces.
    limit: u64,
    /// The partition key of the last row read, which says whether the next row starts a partition.
    last_key: Option<Vec<OwnedDatum>>,
    /// What the pieces of the partition computed so far left, once there has been one.
    carry: Option<Carry>,
    /// The number the next row arrives with.
    next_seq: i64,
    /// Whether the downstream operators asked for no more rows.
    stopped: bool,
    /// How many pieces of a partition were computed, so a test can say that any were.
    pieces: usize,
    downstream: Box<dyn Sink>,
}

impl WindowStream {
    /// Returns a pass.
    ///
    /// @param plan - the partition keys and the calls
    /// @param slots - the slot each call's value goes to
    /// @param shape - where the rows' parts are
    /// @param downstream - what to push the rows into
    pub fn new(
        plan: WindowPlan,
        slots: Vec<usize>,
        shape: StreamShape,
        downstream: Box<dyn Sink>,
    ) -> WindowStream {
        WindowStream {
            plan,
            slots,
            shape,
            rows: Vec::new(),
            held: 0,
            limit: crate::spill::run_bytes(WINDOW_BYTES),
            last_key: None,
            carry: None,
            next_seq: 0,
            stopped: false,
            pieces: 0,
            downstream,
        }
    }

    /// Returns the pass with a different limit, for a test that has to reach the pieces.
    ///
    /// @param bytes - how many bytes a partition may occupy before it is computed in pieces
    #[cfg(test)]
    pub(crate) fn pieces_at(mut self, bytes: u64) -> WindowStream {
        self.limit = bytes;
        self
    }

    /// Computes the rows held, hands them on, and empties the buffer.
    ///
    /// @param whole - the rows are the end of their partition, so the carry ends with them
    fn flush(&mut self, whole: bool) -> DbResult<Flow> {
        let rows = std::mem::take(&mut self.rows);
        self.held = 0;
        self.compute_and_emit(rows, whole)
    }

    /// Computes a run of rows, which may be a whole partition or a piece of one, and emits it.
    ///
    /// @param rows - the rows
    /// @param ends_partition - whether the rows are the end of their partition
    fn compute_and_emit(
        &mut self,
        mut rows: Vec<Vec<OwnedDatum>>,
        ends_partition: bool,
    ) -> DbResult<Flow> {
        if rows.is_empty() {
            return Ok(Flow::Continue);
        }
        let in_pieces = self.carry.is_some() || !ends_partition;
        let (mut values, failure) = if in_pieces {
            let carry = self.carry.get_or_insert_with(|| fresh_carry(&self.plan));
            compute_piece(&rows, &self.plan, carry)
        } else {
            compute_values_until_failure(&rows, &self.plan)
        };
        if ends_partition {
            self.carry = None;
        }
        let calls = self.plan.calls.len();
        let usable = failure
            .as_ref()
            .map_or(rows.len(), |failed| failed.before.min(rows.len()));
        let (width, slots) = (self.shape.width, &self.slots);
        for (at, row) in rows.iter_mut().enumerate().take(usable) {
            for (nth, slot) in slots.iter().enumerate() {
                let value = values
                    .get_mut(at.saturating_mul(calls).saturating_add(nth))
                    .map(|value| std::mem::replace(value, OwnedDatum::Null))
                    .unwrap_or(OwnedDatum::Null);
                if let Some(cell) = row.get_mut(width.saturating_add(*slot)) {
                    *cell = value;
                }
            }
        }
        match failure {
            None => emit_rows(&rows, self.downstream.as_mut()),
            Some(failed) => {
                // SQLite hands out the rows before the one that raised the error. Only a pass whose
                // rows go straight to the answer can do the same; one that feeds another pass
                // has nothing to hand out.
                if self.shape.partial_rows_on_failure && usable > 0 {
                    rows.truncate(usable);
                    if emit_rows(&rows, self.downstream.as_mut())? == Flow::Stop {
                        return Ok(Flow::Stop);
                    }
                }
                Err(failed.error)
            }
        }
    }

    /// Computes the complete peer groups held, as a piece of the partition, and keeps the rest.
    ///
    /// Nothing is computed while the rows held are one peer group, because the group may continue
    /// in the next batch.
    fn compute_a_piece(&mut self) -> DbResult<Flow> {
        let boundary = if pieces_need_no_peers(&self.plan) {
            self.rows.len()
        } else {
            let Some(call) = self.plan.calls.first() else {
                return Ok(Flow::Continue);
            };
            let mut found = 0usize;
            for at in (1..self.rows.len()).rev() {
                let (Some(left), Some(right)) = (self.rows.get(at - 1), self.rows.get(at)) else {
                    continue;
                };
                if !rows_are_peers(left, right, call) {
                    found = at;
                    break;
                }
            }
            found
        };
        if boundary == 0 {
            return Ok(Flow::Continue);
        }
        self.pieces = self.pieces.saturating_add(1);
        PIECES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let rest = self.rows.split_off(boundary);
        let piece = std::mem::replace(&mut self.rows, rest);
        self.held = self.rows.iter().map(|row| resident_row_bytes(row)).sum();
        self.compute_and_emit(piece, false)
    }
}

/// Returns whether the window's `ORDER BY` cannot tell two adjacent rows apart.
///
/// @param left - one row
/// @param right - the next
/// @param call - a call of the window, which carries its ordering terms
fn rows_are_peers(left: &[OwnedDatum], right: &[OwnedDatum], call: &WindowCall) -> bool {
    call.order.iter().all(|term| {
        peers(
            &left
                .get(term.column)
                .map_or(Datum::Null, OwnedDatum::borrow),
            &right
                .get(term.column)
                .map_or(Datum::Null, OwnedDatum::borrow),
            term.collation,
        )
    })
}

impl Sink for WindowStream {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        if self.stopped {
            return Ok(Flow::Stop);
        }
        let in_pieces = computes_in_pieces(&self.plan);
        for nth in 0..batch.live() {
            let mut row: Vec<OwnedDatum> = Vec::with_capacity(
                batch
                    .columns
                    .len()
                    .saturating_add(self.shape.total_slots)
                    .saturating_add(1),
            );
            for column in 0..batch.columns.len() {
                row.push(OwnedDatum::from_datum(&batch.value(nth, column)?));
            }
            if self.shape.first {
                row.extend(std::iter::repeat_n(
                    OwnedDatum::Null,
                    self.shape.total_slots,
                ));
                if self.shape.carries_seq {
                    row.push(OwnedDatum::Int(self.next_seq));
                    self.next_seq = self.next_seq.saturating_add(1);
                }
            }
            let key: Vec<OwnedDatum> = self
                .plan
                .partition
                .iter()
                .map(|(column, _)| row.get(*column).cloned().unwrap_or(OwnedDatum::Null))
                .collect();
            let starts_partition = match &self.last_key {
                Some(last) => !same_key_values(last, &key, &self.plan.partition),
                None => false,
            };
            if starts_partition && self.flush(true)? == Flow::Stop {
                self.stopped = true;
                return Ok(Flow::Stop);
            }
            self.last_key = Some(key);
            // A partition that is not computed in pieces is held whole, so it is charged: the
            // request's byte budget is the only bound on it.
            if !in_pieces {
                inillucent_base::budget::materialise(crate::ops::owned_row_bytes(&row))?;
            }
            self.held = self.held.saturating_add(resident_row_bytes(&row));
            self.rows.push(row);
        }
        if in_pieces && self.held >= self.limit && self.compute_a_piece()? == Flow::Stop {
            self.stopped = true;
            return Ok(Flow::Stop);
        }
        Ok(Flow::Continue)
    }

    fn finish(&mut self) -> DbResult<()> {
        if !self.stopped {
            self.flush(true)?;
        }
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.rows.clear();
        self.held = 0;
        self.last_key = None;
        self.carry = None;
        self.next_seq = 0;
        self.stopped = false;
        self.downstream.reset()
    }
}

/// Returns whether two partition keys, already read out of their rows, are the same.
///
/// @param left - one key's values
/// @param right - the other's
/// @param key - the partition key columns and their collations
fn same_key_values(left: &[OwnedDatum], right: &[OwnedDatum], key: &[(usize, Collation)]) -> bool {
    key.iter().enumerate().all(|(at, (_, collation))| {
        peers(
            &left.get(at).map_or(Datum::Null, OwnedDatum::borrow),
            &right.get(at).map_or(Datum::Null, OwnedDatum::borrow),
            *collation,
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::Vector;
    use crate::ops::CollectInto;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// Returns a plan of the calls that can be computed in pieces, over a row of a partition key, an
    /// ordering value and a number: `row_number`, `rank`, `dense_rank`, a running `sum` and a running
    /// `max`.
    fn plan() -> WindowPlan {
        let order = vec![OrderTerm {
            column: 1,
            descending: false,
            collation: Collation::Binary,
        }];
        let call = |func: WindowSlot, arguments: Vec<usize>| WindowCall {
            func,
            distinct: false,
            collation: Collation::Binary,
            arguments,
            json_marks: 0,
            filter: None,
            order: order.clone(),
            frame: WindowFrame::default(),
        };
        WindowPlan {
            partition: vec![(0, Collation::Binary)],
            calls: vec![
                call(WindowSlot::Plain(WindowFunc::RowNumber), vec![]),
                call(WindowSlot::Plain(WindowFunc::Rank), vec![]),
                call(WindowSlot::Plain(WindowFunc::DenseRank), vec![]),
                call(WindowSlot::Aggregate(AggregateKind::Sum), vec![2]),
                call(WindowSlot::Aggregate(AggregateKind::Maximum), vec![2]),
            ],
        }
    }

    /// Runs the rows through a pass and returns what came out.
    ///
    /// @param rows - the input, sorted by partition key and then ordering value
    /// @param limit - how many bytes a partition may occupy before it is computed in pieces
    fn run(rows: &[Vec<OwnedDatum>], limit: u64) -> (Vec<Vec<OwnedDatum>>, usize) {
        let collected = Rc::new(RefCell::new(Vec::new()));
        let shape = StreamShape {
            width: 3,
            total_slots: 5,
            first: true,
            carries_seq: false,
            partial_rows_on_failure: true,
        };
        let mut stream = WindowStream::new(
            plan(),
            (0..5).collect(),
            shape,
            Box::new(CollectInto::new(Rc::clone(&collected))),
        )
        .pieces_at(limit);
        for chunk in rows.chunks(37) {
            let columns: Vec<Vec<Datum<'_>>> = (0..3)
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
            stream
                .push(&Batch::new(chunk.len(), vectors))
                .expect("a batch is pushed");
        }
        let pieces = stream.pieces;
        stream.finish().expect("the pass finishes");
        let answer = collected.borrow().clone();
        (answer, pieces)
    }

    /// **A partition computed in pieces gets the values the whole partition gets.**
    ///
    /// The ordering value repeats, so peer groups of several rows fall on the edge of a piece, and
    /// one partition is much larger than the limit while others are smaller, so both paths run.
    #[test]
    fn pieces_of_a_partition_get_the_values_of_the_whole() {
        let mut rows: Vec<Vec<OwnedDatum>> = Vec::new();
        for (partition, count) in [(1i64, 3_000i64), (2, 40), (3, 1_500), (4, 1)] {
            for n in 0..count {
                rows.push(vec![
                    OwnedDatum::Int(partition),
                    OwnedDatum::Int(n / 7),
                    if n % 11 == 0 {
                        OwnedDatum::Null
                    } else {
                        OwnedDatum::Int((n * 31) % 101 - 40)
                    },
                ]);
            }
        }
        let (whole, none) = run(&rows, u64::MAX);
        let (in_pieces, pieces) = run(&rows, 8 * 1024);
        assert_eq!(none, 0);
        assert!(
            pieces > 20,
            "{pieces} pieces: the limit was never reached, so this compared whole with whole"
        );
        assert_eq!(whole.len(), rows.len());
        assert_eq!(in_pieces, whole, "computing in pieces changed a value");
        assert!(computes_in_pieces(&plan()));
    }
}

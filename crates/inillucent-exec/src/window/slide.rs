//! Aggregates over a frame whose start moves, computed by taking rows in and out.
//!
//! Invariant: **a window aggregate over a frame that starts after the start of the partition
//! reads each row of the partition once when it enters the frame and once when it leaves, and
//! gives the answer the frame's own rows give.** The calls here used to rebuild the frame for
//! every row, so `max(v) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING)` over
//! 100,000 rows took 64.6 seconds where SQLite takes 0.13.
//!
//! ## Why the bounds can be walked
//!
//! The partition is sorted by the window's order and every bound is a fixed distance from the row,
//! so both ends of the frame only move forward. [`super::sliding_sums`] relies on the same fact.
//! A frame that ever moved backwards is computed row by row for the rest of the partition.
//!
//! ## Three ways of holding the frame
//!
//! - `count` keeps a number. A row that counts adds one when it enters and takes one when it leaves.
//! - `min` and `max` keep the rows that could still be the answer, in order, so the first of them is
//!   the answer. A new row removes every row before it that it beats or equals, because those can
//!   never be the answer again: the new row leaves the frame later than they do. Keeping the
//!   latest of equal values is what SQLite answers for a sliding frame, because it rebuilds such a
//!   frame from its end; over 0.0 and 0 the answer is the integer 0.
//! - Everything else (`median` and the other percentiles, `json_group_array`, `json_group_object`
//!   and `group_concat`) keeps the accumulator that a plain aggregate uses and takes the oldest row
//!   out of it with [`Accumulator::pull_oldest`]. Their answer is built from the whole frame, so
//!   each row still pays for that, but the frame's rows are no longer fed in again for every row.

use std::collections::VecDeque;

use super::*;
use inillucent_scalar::json;

use crate::aggregate::{percentile_of_sorted, refuse_past_value_limit, Percentile};

/// Returns whether a call is computed by [`sliding_aggregate`].
///
/// A frame that starts at the start of the partition only grows and has its own function, the
/// sums have theirs, and an `EXCLUDE` clause or `DISTINCT` makes the members of one row's frame
/// depend on that row, so those are computed row by row.
///
/// @param call - the window call
pub(super) fn slides_by_entering_and_leaving(call: &WindowCall) -> bool {
    let WindowSlot::Aggregate(kind) = &call.func else {
        return false;
    };
    let slides = !matches!(call.frame.start, FrameEnd::UnboundedPreceding)
        && call.frame.exclude == FrameExclude::NoOthers
        && !call.distinct;
    slides
        && matches!(
            kind,
            AggregateKind::CountStar
                | AggregateKind::Count
                | AggregateKind::Minimum
                | AggregateKind::Maximum
                | AggregateKind::GroupConcat(_)
                | AggregateKind::GroupConcatComputed
                | AggregateKind::JsonGroupArray(_)
                | AggregateKind::JsonGroupObject(_)
                | AggregateKind::Percentile(_)
        )
}

/// What a sliding aggregate holds between two rows.
enum Held {
    /// How many rows count, for `count`.
    Counter(i64),
    /// The rows that could still be the answer, oldest first, for `min` and `max`.
    Extreme {
        /// The partition's row numbers.
        candidates: VecDeque<usize>,
        /// Whether the answer is the largest value, which is `max`.
        maximum: bool,
    },
    /// The values of the frame, sorted, for a percentile over a partition whose every row is valid.
    Sorted {
        /// Which of the four percentile aggregates.
        which: Percentile,
        /// The fraction every row of the partition asks for.
        fraction: f64,
        /// The numeric values of the frame, lowest first.
        values: Vec<f64>,
    },
    /// The rendered members of a JSON group aggregate, oldest first.
    Json {
        /// Whether the call is `json_group_object`.
        object: bool,
        /// The text of each member in the frame.
        pieces: VecDeque<String>,
    },
    /// An accumulator holding the rows of the frame.
    Collected {
        /// The accumulator.
        accumulator: Accumulator,
        /// Whether it keeps each row's arguments whole, so a NULL is a member.
        whole_rows: bool,
    },
}

impl Held {
    /// Returns what a call holds before any row has entered its frame.
    ///
    /// @param call - the window call, which `slides_by_entering_and_leaving` accepted
    /// @param checked - what the rows of a percentile call hold, from [`check_percentile`]
    fn new(call: &WindowCall, checked: &PercentileCheck) -> Held {
        let kind = call_kind(call);
        if let (AggregateKind::Percentile(which), Some(fraction), None) =
            (&kind, checked.fraction, &checked.bad)
        {
            return Held::Sorted {
                which: *which,
                fraction,
                values: Vec::new(),
            };
        }
        match kind {
            AggregateKind::JsonGroupArray(false) | AggregateKind::JsonGroupObject(false) => {
                Held::Json {
                    object: matches!(kind, AggregateKind::JsonGroupObject(_)),
                    pieces: VecDeque::new(),
                }
            }
            AggregateKind::CountStar | AggregateKind::Count => Held::Counter(0),
            AggregateKind::Minimum | AggregateKind::Maximum => Held::Extreme {
                candidates: VecDeque::new(),
                maximum: kind == AggregateKind::Maximum,
            },
            _ => {
                let whole_rows = keeps_whole_rows(&kind);
                let mut accumulator = Accumulator::new(kind);
                accumulator.compare_under(call.collation);
                accumulator.track_pieces();
                Held::Collected {
                    accumulator,
                    whole_rows,
                }
            }
        }
    }

    /// Adds a row that entered the frame and passed the call's `FILTER`.
    ///
    /// @param rows - the buffered input
    /// @param call - the window call
    /// @param row - the row that entered
    fn enter(&mut self, rows: &[Vec<OwnedDatum>], call: &WindowCall, row: usize) -> DbResult<()> {
        count_frame_work(1);
        match self {
            Held::Counter(count) => {
                if counts(rows, call, row) {
                    *count = count.saturating_add(1);
                }
                Ok(())
            }
            Held::Extreme {
                candidates,
                maximum,
            } => {
                enter_extreme(rows, call, candidates, *maximum, row);
                Ok(())
            }
            Held::Json { object, pieces } => {
                if let Some(piece) = json_piece(rows, call, row, *object)? {
                    pieces.push_back(piece);
                }
                Ok(())
            }
            Held::Sorted { values, .. } => {
                if let Some(number) = number_of(&argument_of(rows, call, row)) {
                    let at = values.partition_point(|held| held.total_cmp(&number).is_lt());
                    values.insert(at, number);
                }
                Ok(())
            }
            Held::Collected {
                accumulator,
                whole_rows,
            } => feed(accumulator, rows, call, row, *whole_rows),
        }
    }

    /// Takes out a row that left the frame and had passed the call's `FILTER`.
    ///
    /// @param rows - the buffered input
    /// @param call - the window call
    /// @param row - the row that left
    fn leave(&mut self, rows: &[Vec<OwnedDatum>], call: &WindowCall, row: usize) {
        count_frame_work(1);
        match self {
            Held::Counter(count) => {
                if counts(rows, call, row) {
                    *count = count.saturating_sub(1);
                }
            }
            Held::Extreme { candidates, .. } => {
                if candidates.front() == Some(&row) {
                    candidates.pop_front();
                }
            }
            Held::Json { object, pieces } => {
                // A row whose label is NULL has no member in an object, so nothing of it is held.
                if !*object || !argument_of(rows, call, row).is_null() {
                    pieces.pop_front();
                }
            }
            Held::Sorted { values, .. } => {
                if let Some(number) = number_of(&argument_of(rows, call, row)) {
                    let at = values.partition_point(|held| held.total_cmp(&number).is_lt());
                    if values
                        .get(at)
                        .is_some_and(|held| held.total_cmp(&number).is_eq())
                    {
                        values.remove(at);
                    }
                }
            }
            Held::Collected {
                accumulator,
                whole_rows,
            } => {
                // A value the accumulator skipped, a NULL for a call that folds one argument, was
                // never added, so there is nothing of it to take out.
                if *whole_rows || !argument_of(rows, call, row).is_null() {
                    accumulator.pull_oldest();
                }
            }
        }
    }

    /// Returns the answer for the rows held now.
    ///
    /// @param rows - the buffered input
    /// @param call - the window call
    fn answer(&self, rows: &[Vec<OwnedDatum>], call: &WindowCall) -> DbResult<OwnedDatum> {
        match self {
            Held::Counter(count) => Ok(OwnedDatum::Int(*count)),
            Held::Extreme { candidates, .. } => Ok(candidates
                .front()
                .map(|row| OwnedDatum::from_datum(&argument_of(rows, call, *row)))
                .unwrap_or(OwnedDatum::Null)),
            Held::Json { object, pieces } => {
                let (open, close) = if *object { ('{', '}') } else { ('[', ']') };
                let joined =
                    json::group_text_answer(open, pieces.iter().map(String::as_str), close)?;
                let answer = OwnedDatum::from(joined.value);
                refuse_past_value_limit(&answer)?;
                Ok(answer)
            }
            Held::Sorted {
                which,
                fraction,
                values,
            } => Ok(percentile_of_sorted(*which, *fraction, values)),
            Held::Collected { accumulator, .. } => accumulator.finish(),
        }
    }

    /// Reports whether the rows held can no longer be taken out one at a time.
    fn is_overfull(&self) -> bool {
        matches!(self, Held::Collected { accumulator, .. } if accumulator.is_overfull())
    }
}

/// Renders the member a row adds to a JSON group aggregate, or `None` when the row adds none.
///
/// The JSON subtype of each argument is the mark the call carries for it, as the accumulator
/// reads it, so a value a JSON function produced is nested and not quoted.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param row - the row entering the frame
/// @param object - whether the call is `json_group_object`
fn json_piece(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    row: usize,
    object: bool,
) -> DbResult<Option<String>> {
    let mut held = Vec::with_capacity(call.arguments.len());
    for column in &call.arguments {
        held.push(Value::from(&value_at(rows, row, *column)).into_owned()?);
    }
    let marked = |position: u32| {
        call.json_marks
            .checked_shr(position)
            .is_some_and(|bits| bits & 1 == 1)
    };
    let argument = |position: usize| {
        held.get(position).map(|value| json::Argument {
            value,
            json: marked(position as u32),
        })
    };
    if object {
        let (Some(label), Some(value)) = (argument(0), argument(1)) else {
            return Ok(None);
        };
        return json::group_object_piece(&label, &value);
    }
    match argument(0) {
        Some(value) => json::group_array_piece(&value).map(Some),
        None => Ok(None),
    }
}

/// Returns a value as a number when it is an integer or a real, and `None` for NULL.
///
/// @param value - a value `clean_fraction` has accepted
fn number_of(value: &Datum<'_>) -> Option<f64> {
    match value {
        Datum::Int(held) => Some(*held as f64),
        Datum::Real(held) => Some(*held),
        _ => None,
    }
}

/// What the rows of a percentile call hold, read in the order SQLite adds them to the aggregate.
struct PercentileCheck {
    /// The fraction the first row asks for, when that is acceptable.
    fraction: Option<f64>,
    /// The first row SQLite refuses, and the error it raises for it.
    bad: Option<(usize, inillucent_base::DbError)>,
}

/// Reads the rows of a percentile call in partition order and finds the first one SQLite refuses.
///
/// **SQLite checks a row when it adds the row to the aggregate, and compares its fraction with the
/// fraction of the first row added to the partition, a row that may have left the frame since.** A
/// row is refused when its fraction is not acceptable, when its fraction differs from the first
/// row's, or when its value is not NULL and not an integer or a real. So `percentile(v, id) OVER
/// (ORDER BY id ROWS CURRENT ROW)` fails on the second row, where each frame holds one row and
/// agrees with itself. A call over a partition with no refused row is computed from sorted values.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param partition - the partition
fn check_percentile(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
) -> PercentileCheck {
    let AggregateKind::Percentile(which) = call_kind(call) else {
        return PercentileCheck {
            fraction: None,
            bad: None,
        };
    };
    let mut fraction: Option<f64> = None;
    for row in partition.start..partition.end {
        // A row the call's `FILTER` drops is never added to the aggregate, so it is never checked.
        if !passes_filter(rows, call, row) {
            continue;
        }
        let given = call
            .arguments
            .get(1)
            .and_then(|column| Value::from(&value_at(rows, row, *column)).into_owned().ok());
        let wanted = match which.fraction(given.as_ref()) {
            Ok(wanted) => wanted,
            Err(error) => {
                return PercentileCheck {
                    fraction,
                    bad: Some((row, error)),
                }
            }
        };
        if fraction.is_some_and(|held| held != wanted) {
            let message = format!(
                "the fraction argument to {}() is not the same for all input rows",
                which.name()
            );
            return PercentileCheck {
                fraction,
                bad: Some((row, inillucent_base::error::statement_refusal(message))),
            };
        }
        fraction = Some(wanted);
        if !matches!(
            argument_of(rows, call, row),
            Datum::Null | Datum::Int(_) | Datum::Real(_)
        ) {
            let message = format!("input to {}() is not numeric", which.name());
            return PercentileCheck {
                fraction,
                bad: Some((row, inillucent_base::error::statement_refusal(message))),
            };
        }
    }
    PercentileCheck {
        fraction,
        bad: None,
    }
}

/// Reports whether a row's `ROWS` or `GROUPS` frame starts after it ends whatever the data is.
/// See `frames::starts_after_end`.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param row - the row whose frame it is
fn empty_by_offsets(rows: &[Vec<OwnedDatum>], call: &WindowCall, row: usize) -> DbResult<bool> {
    Ok(frames::starts_after_end(&frame_spec(rows, call, row)?))
}

/// Returns a row's first argument, or the integer 1 for `count(*)`, which has none.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param row - the row
fn argument_of<'r>(rows: &'r [Vec<OwnedDatum>], call: &WindowCall, row: usize) -> Datum<'r> {
    match call.arguments.first() {
        Some(column) => value_at(rows, row, *column),
        None => Datum::Int(1),
    }
}

/// Returns whether a row adds to a `count`: every row for `count(*)`, a non NULL argument otherwise.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param row - the row
fn counts(rows: &[Vec<OwnedDatum>], call: &WindowCall, row: usize) -> bool {
    !argument_of(rows, call, row).is_null()
}

/// Adds a row to the candidates for `min` or `max`, first removing those it makes useless.
///
/// A candidate the new row beats or equals leaves the frame before the new row does, so it can
/// never be the answer again. An equal value is removed too, which keeps the latest of equal
/// values. A NULL is never an answer.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param candidates - the rows that could still be the answer, oldest first
/// @param maximum - whether the largest value is wanted
/// @param row - the row that entered
fn enter_extreme(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    candidates: &mut VecDeque<usize>,
    maximum: bool,
    row: usize,
) {
    let value = argument_of(rows, call, row);
    if value.is_null() {
        return;
    }
    while let Some(last) = candidates.back() {
        let order = compare_under(&argument_of(rows, call, *last), &value, call.collation);
        let beaten = if maximum {
            order != std::cmp::Ordering::Greater
        } else {
            order != std::cmp::Ordering::Less
        };
        if !beaten {
            break;
        }
        candidates.pop_back();
    }
    candidates.push_back(row);
}

/// Computes an aggregate for every row of one partition by taking rows in and out of the frame.
///
/// Between two rows the rows that left the frame are taken out first and then the rows that
/// entered are added, each in partition order. A frame that is empty for a row reads whatever is
/// held after the rows before it left, which is nothing.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param partition - the partition
/// @param values - where the answers go, so a failure keeps the ones before it
pub(super) fn sliding_aggregate(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    values: &mut Vec<OwnedDatum>,
) -> DbResult<()> {
    let checked = check_percentile(rows, call, partition);
    let mut held = Held::new(call, &checked);
    // The last row the end of a frame has reached so far, which is how many rows have been added.
    let mut stepped: Option<usize> = None;
    // The rows the frame holds, as a half open range of the partition. Both ends only move forward.
    let (mut held_start, mut held_end) = (partition.start, partition.start);
    values.reserve(partition.end.saturating_sub(partition.start));
    for row in partition.start..partition.end {
        let (start, end) = match frame_bounds(rows, call, partition, row)? {
            Some((low, high)) => (low, high.saturating_add(1)),
            None => (held_end, held_end),
        };
        if start < held_start || end < held_end || held.is_overfull() {
            return sliding_aggregate_by_row(rows, call, partition, values);
        }
        refuse_a_row_already_added(rows, call, partition, row, &checked, &mut stepped)?;
        for leaving in held_start..start.min(held_end) {
            if passes_filter(rows, call, leaving) {
                held.leave(rows, call, leaving);
            }
        }
        for entering in start.max(held_end)..end {
            if passes_filter(rows, call, entering) {
                held.enter(rows, call, entering)?;
            }
        }
        held_start = start;
        held_end = end.max(start);
        values.push(held.answer(rows, call)?);
    }
    Ok(())
}

/// Fails a row whose frame end has reached a row that SQLite refuses when it adds the row.
///
/// **The row that fails is the first one whose frame end reaches the refused row, even when that
/// row's frame is empty,** because SQLite adds a row when the end of a frame passes it and takes
/// it out when the start does. A `ROWS` or `GROUPS` frame that starts after it ends adds nothing.
/// Does nothing for a call whose rows are all acceptable.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param partition - the partition
/// @param row - the row whose answer is about to be computed
/// @param checked - what the rows of the call hold, from [`check_percentile`]
/// @param stepped - the last row any frame end has reached so far, updated here
fn refuse_a_row_already_added(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    row: usize,
    checked: &PercentileCheck,
    stepped: &mut Option<usize>,
) -> DbResult<()> {
    let Some((bad, error)) = &checked.bad else {
        return Ok(());
    };
    if !empty_by_offsets(rows, call, row)? {
        *stepped = (*stepped).max(frame_end(rows, call, partition, row)?);
    }
    match stepped {
        Some(reached) if bad <= reached => Err(error.clone()),
        _ => Ok(()),
    }
}

/// Finishes a partition by computing each remaining row's frame from its own rows.
///
/// Reached when a bound moved backwards, which a sorted partition does not give, and when a
/// `group_concat` grew to the value limit, after which the rows can no longer be taken out one at
/// a time. It answers correctly and slowly.
///
/// @param rows - the buffered input
/// @param call - the window call
/// @param partition - the partition
/// @param values - the answers already computed, extended in place
fn sliding_aggregate_by_row(
    rows: &[Vec<OwnedDatum>],
    call: &WindowCall,
    partition: &frames::Partition,
    values: &mut Vec<OwnedDatum>,
) -> DbResult<()> {
    let first = partition.start.saturating_add(values.len());
    for row in first..partition.end {
        values.push(aggregate(rows, call, partition, row, call_kind(call))?);
    }
    Ok(())
}

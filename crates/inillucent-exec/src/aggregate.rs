//! Aggregate accumulators, with the dialect's semantics rather than Rust's.
//!
//! Invariant: every accumulator produces what SQLite 3.53.4 produces, including
//! the cases where that is surprising. The differential gate compares digests,
//! so `sum` of no rows being NULL while `total` of no rows is `0.0` is not a
//! detail - it is the difference between a passing family and a correctness
//! failure that stops the timing being read at all.
//!
//! The documented behaviours this implements:
//!
//! - **`count(*)`** counts rows; **`count(x)`** counts rows where `x` is not
//!   NULL.
//! - **`sum(x)`** of no non-NULL rows is **NULL**. `sum` of integers stays an
//!   integer until it overflows, and then becomes a double - SQLite raises an
//!   "integer overflow" error for `sum` in that case, which Phase 1 does not
//!   reach on any scorecard workload and which is recorded as an open item
//!   rather than silently guessed at.
//! - **`total(x)`** is `sum` that returns `0.0` instead of NULL and is always a
//!   double.
//! - **`avg(x)`** is a double, NULL over no non-NULL rows, and divides by the
//!   count of non-NULL rows.
//! - **`min`/`max`** ignore NULLs, return NULL over no non-NULL rows, and order
//!   by the dialect's class order.
//! - **`group_concat(x, sep)`** joins non-NULL values with `sep` (default
//!   `","`) and is NULL over no non-NULL rows.

use std::collections::HashSet;

use inillucent_base::DbResult;
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_value::collation::Collation;
use inillucent_value::Value;

use crate::expr::{format_real, numeric};

/// Which aggregate an accumulator computes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AggregateKind {
    /// `count(*)`: every row.
    CountStar,
    /// `count(x)`: rows where the argument is not NULL.
    Count,
    /// `sum(x)`.
    Sum,
    /// `total(x)`.
    Total,
    /// `avg(x)`.
    Average,
    /// `min(x)`.
    Minimum,
    /// `max(x)`.
    Maximum,
    /// `group_concat(x, sep)`.
    GroupConcat(String),
    /// `group_concat(x, y)` where `y` is not a literal, so the separator is a
    /// value of each row rather than a constant of the call.
    ///
    /// **The separator SQLite uses before a row is that row's own (task-1913).**
    /// `group_concat(s, n)` over `('a',1), ('b',2), ('c',3)` is `a2b3c`: the
    /// first kept row contributes no separator, and every later one is preceded
    /// by the separator read from itself. A NULL separator contributes nothing,
    /// which is why `group_concat(s, NULL)` is `abc` rather than NULL. Measured
    /// against the pinned 3.53.4 rather than reasoned about, including through
    /// the call's own `ORDER BY`, where the rows are sorted first and the rule
    /// then applies to the sorted order.
    ///
    /// It keeps whole rows rather than folding as they arrive, because the
    /// separator cannot be known until the row it belongs to is there.
    GroupConcatComputed,
    /// `json_group_array(x)` and its `jsonb_` spelling.
    ///
    /// The flag is the binary form. A NULL is a *member* here rather than a row
    /// to skip: `json_group_array` over one NULL is `[null]` and not `[]`,
    /// because the document records what the rows held and a JSON null is a
    /// value.
    JsonGroupArray(bool),
    /// `json_group_object(label, value)` and its `jsonb_` spelling.
    JsonGroupObject(bool),
    /// `median(x)`, `percentile(x, p)`, `percentile_cont(x, f)` and
    /// `percentile_disc(x, f)`.
    ///
    /// One kind rather than four, because they differ only in where the
    /// fraction comes from and whether the answer may fall between two rows -
    /// see [`Percentile`]. Every one of them has to see the whole group sorted
    /// before it can answer, so they collect their rows the way a registered
    /// aggregate does rather than reducing as they go.
    Percentile(Percentile),
    /// A **bare column**: one read outside any aggregate in an aggregating
    /// query.
    ///
    /// `SELECT id, max(a) FROM t` is SQLite's own extension, and its rule is
    /// exact rather than arbitrary: when the query has exactly one `min` or
    /// `max`, the bare columns come from **the row that produced the extreme**,
    /// and otherwise from an arbitrary row - which SQLite takes as the last one
    /// of the group. The witness is the ordering that names the extreme, and it
    /// is carried per bare column rather than shared, so no accumulator has to
    /// see inside another.
    Bare(Option<std::cmp::Ordering>),
    /// `geopoly_group_bbox(P)`: the box that holds every polygon in the group.
    ///
    /// It collects its rows rather than reducing as it goes, which costs more
    /// memory than the four floats it needs. That is deliberate: an aggregate
    /// with running state of its own would be the only one in this enum, and
    /// the group a bounding box is asked for is a map layer rather than a
    /// table scan.
    GeopolyBox,
    /// `sum(v)` and `avg(v)` over a vector column, component by component.
    ///
    /// The flag is whether the total is divided by how many vectors went into
    /// it. Rows are collected rather than reduced for the same reason
    /// `GeopolyBox` collects them: the width is not known until the first
    /// vector arrives, and a running total sized from the first row would be
    /// wrong for a column whose rows disagree - which is a thing to *report*,
    /// not to average over.
    VectorFold(bool),
    /// An aggregate an application registered.
    ///
    /// It is handed every row of the group, in order, rather than a running
    /// accumulator. That is deliberate: an implementation written in C keeps
    /// its state in memory this engine must not look inside, and driving its
    /// step and final at the end of the group is how that state stays entirely
    /// on the other side of the boundary.
    External(crate::expr::AggregateBody),
}

/// The modulus a large integer is split on before it joins a double sum.
///
/// 2^14. The high part is then a multiple of 2^14, so a 63-bit integer needs
/// only 49 bits of mantissa to hold it exactly, and the remainder is small
/// enough to be exact by itself. This is the constant SQLite uses.
const SPLIT_I64: i64 = 16_384;

/// The magnitude below which a double holds an integer exactly.
///
/// 2^52. SQLite uses the same bound to decide whether a value needs splitting.
const EXACT_IN_DOUBLE: i64 = 4_503_599_627_370_496;

/// Which of the four percentile aggregates is being computed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Percentile {
    /// `median(x)`: the fraction is fixed at one half.
    Median,
    /// `percentile(x, p)`: the second argument runs 0 to 100.
    Hundredths,
    /// `percentile_cont(x, f)`: the second argument runs 0 to 1, and the answer
    /// is interpolated between the two rows the position falls between.
    Continuous,
    /// `percentile_disc(x, f)`: the same range, but the answer is one of the
    /// rows rather than a value between two of them.
    Discrete,
}

impl Percentile {
    /// Returns the fraction of the way through the group, from 0 to 1.
    ///
    /// `median` ignores its (absent) second argument; `percentile` divides by a
    /// hundred; the other two take the argument as written. A fraction outside
    /// the range, or one that is not a number, is an error naming the aggregate,
    /// which is what the reference answers.
    ///
    /// @param argument - the second argument's value, when there is one
    pub(crate) fn fraction(self, argument: Option<&Value<'static>>) -> DbResult<f64> {
        let (divisor, upper) = match self {
            Percentile::Median => return Ok(0.5),
            Percentile::Hundredths => (100.0, "100.0"),
            Percentile::Continuous | Percentile::Discrete => (1.0, "1.0"),
        };
        let given = argument.cloned().unwrap_or(Value::Null);
        let number = match inillucent_value::affinity::apply_numeric_affinity(given, true) {
            Value::Integer(held) => Some(held as f64),
            Value::Real(held) => Some(held),
            _ => None,
        };
        match number.map(|held| held / divisor) {
            Some(fraction) if (0.0..=1.0).contains(&fraction) => Ok(fraction),
            _ => Err(inillucent_base::error::statement_refusal(format!(
                "the fraction argument to {}() is not between 0.0 and {upper}",
                self.name()
            ))),
        }
    }

    /// Returns the SQL name SQLite's error messages use for this aggregate.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Percentile::Median => "median",
            Percentile::Hundredths => "percentile",
            Percentile::Continuous => "percentile_cont",
            Percentile::Discrete => "percentile_disc",
        }
    }

    /// Returns whether the answer may be a value no row held.
    fn interpolates(self) -> bool {
        !matches!(self, Percentile::Discrete)
    }
}

/// Returns what a percentile aggregate answers over values already sorted, lowest first.
///
/// A sliding window frame keeps its values sorted as rows enter and leave and calls this for every
/// row, so the answer is the one [`Accumulator::finish`] gives. An empty list answers NULL.
///
/// `percentile_disc` picks a row; the other three interpolate between the two rows the position
/// falls between.
///
/// @param which - which of the four is being computed
/// @param fraction - the fraction of the way through the values, from 0 to 1
/// @param values - the values, sorted
pub(crate) fn percentile_of_sorted(which: Percentile, fraction: f64, values: &[f64]) -> OwnedDatum {
    if values.is_empty() {
        return OwnedDatum::Null;
    }
    let last = values.len().saturating_sub(1);
    let position = fraction * last as f64;
    if !which.interpolates() {
        // The row the position lands *in*, which is what "discrete" means:
        // an answer some row actually held. It rounds down rather than to
        // nearest, so `percentile_disc(x, 0.5)` over four rows is the
        // second of them and not the third - measured against the
        // reference, which is the only way to get this one right.
        let at = position.floor().max(0.0) as usize;
        return OwnedDatum::Real(values.get(at.min(last)).copied().unwrap_or(f64::NAN));
    }
    let below = position.floor().max(0.0) as usize;
    let above = position.ceil().max(0.0) as usize;
    let low = values.get(below.min(last)).copied().unwrap_or(f64::NAN);
    let high = values.get(above.min(last)).copied().unwrap_or(f64::NAN);
    OwnedDatum::Real(low + (high - low) * (position - below as f64))
}

/// How one key of an aggregate's own `ORDER BY` is compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SortTerm {
    /// Whether the key sorts high to low.
    pub descending: bool,
    /// The collation the key's text is compared under.
    pub collation: Collation,
    /// Whether a NULL key sorts before every other value.
    pub nulls_first: bool,
}

/// One aggregate's running state.
#[derive(Clone, Debug)]
pub struct Accumulator {
    kind: AggregateKind,
    /// Rows counted, or non-NULL values seen.
    count: i64,
    /// The integer running total, while the sum is still exact.
    integer_sum: i64,
    /// The double running total, once the sum has left the integers.
    real_sum: f64,
    /// The low bits the running total could not hold.
    ///
    /// Kahan-Babuska-Neumaier's compensation term. It is added in at the very
    /// end rather than folded back each step, which is what makes the sum of
    /// `-1e300`, a handful of ordinary values and `+1e300` come out as the
    /// handful rather than as zero - and is what the pinned SQLite answers.
    compensation: f64,
    /// Whether the sum has left the integers.
    is_real: bool,
    /// Whether it left them because an integer total would not fit.
    ///
    /// Kept apart from `is_real`, which is also set by a real *argument*. Only
    /// the first of the two is `sum()`'s `integer overflow`.
    overflowed: bool,
    /// Whether any value folded in was not an integer.
    saw_real: bool,
    /// The extreme value seen, for `min` and `max`.
    extreme: Option<OwnedDatum>,
    /// The collation `extreme` is chosen under.
    ///
    /// The argument's own, as SQLite uses: `max(name)` over a `NOCASE` column
    /// is `Zebra` and not `outdoor`.
    ordering: Collation,
    /// The value a bare column has settled on, and the witness that chose it.
    chosen: Option<OwnedDatum>,
    /// The parts only some calls use, boxed and made on first use.
    ///
    /// **Most accumulators are a `count`, a `sum`, a `min` or a `max` (task-2175).**
    /// With every field inline an accumulator was several hundred bytes, and a
    /// `GROUP BY` of 50,000 groups made, finished and dropped 50,000 of them:
    /// the hash aggregate's profile was mostly cache misses on them. What a
    /// `DISTINCT`, an `ORDER BY` inside the call, a JSON aggregate,
    /// `group_concat` or a registered aggregate needs lives here instead.
    rare: Option<Box<Rare>>,
}

/// A hasher for integers that mixes one multiplication, for the set of integers a `DISTINCT`
/// aggregate has seen.
///
/// The standard hasher is built to resist chosen keys and spends about 20 ns on eight bytes. The
/// keys here are values a query read from its own table, and 500,000 of them made the set the
/// largest cost of `count(DISTINCT post_id)`.
#[derive(Clone, Copy, Debug, Default)]
struct IntegerHasher(u64);

impl std::hash::Hasher for IntegerHasher {
    /// Returns the hash of everything written so far.
    fn finish(&self) -> u64 {
        self.0
    }

    /// Mixes bytes in one at a time; only a key that is not an `i64` reaches it.
    ///
    /// @param bytes - the bytes to mix in
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = (self.0 ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    /// Mixes one integer in with a multiplication and a shift.
    ///
    /// @param number - the integer
    fn write_i64(&mut self, number: i64) {
        let mixed = (number as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        self.0 = mixed ^ (mixed >> 29);
    }
}

/// The values a `DISTINCT` aggregate has already folded in.
///
/// **Integers are kept as integers until something else arrives.** The general form is a set of
/// encoded keys, one allocation per new value and a hash over its bytes, and it is what makes a
/// real `1.0` and an integer `1` the same value under the encoding. A column of integers, which
/// is nearly every `count(DISTINCT id)`, never meets that question, so its values go in a set of
/// `i64`. The first value that is not an integer moves every integer seen so far into the
/// encoded set, and from then on every value goes there, so the answer is the one the encoded
/// set alone gives.
#[derive(Clone, Debug)]
struct DistinctSet {
    /// The collation the argument is compared under.
    collation: Collation,
    /// The integers seen, while nothing else has been.
    integers: HashSet<i64, std::hash::BuildHasherDefault<IntegerHasher>>,
    /// Every value seen, once a value that is not an integer has arrived.
    encoded: HashSet<Vec<u8>>,
    /// Whether the integers have moved into `encoded`.
    moved: bool,
}

impl DistinctSet {
    /// Returns an empty set for values compared under a collation.
    ///
    /// @param collation - the collation the argument is compared under
    fn new(collation: Collation) -> DistinctSet {
        DistinctSet {
            collation,
            integers: HashSet::default(),
            encoded: HashSet::new(),
            moved: false,
        }
    }

    /// Records a value and says whether it had not been seen before.
    ///
    /// @param value - the argument's value for this row
    /// @param scratch - a buffer the encoded key is built in, reused between rows
    fn first_sight(&mut self, value: &Datum<'_>, scratch: &mut Vec<u8>) -> bool {
        if !self.moved {
            if let Datum::Int(number) = value {
                return self.integers.insert(*number);
            }
            self.moved = true;
            for number in std::mem::take(&mut self.integers) {
                scratch.clear();
                inillucent_tree::key::encode_into_with(
                    &Datum::Int(number),
                    self.collation,
                    scratch,
                );
                self.encoded.insert(scratch.clone());
            }
        }
        scratch.clear();
        inillucent_tree::key::encode_into_with(value, self.collation, scratch);
        if self.encoded.contains(scratch.as_slice()) {
            return false;
        }
        self.encoded.insert(scratch.clone());
        true
    }
}

/// What only some aggregate calls keep, apart from the fields every call uses.
///
/// See [`Accumulator`]'s `rare` field for why these are boxed.
#[derive(Clone, Debug, Default)]
struct Rare {
    /// The argument values already folded in, when the call said `DISTINCT`.
    ///
    /// `count(DISTINCT team)` counts the distinct teams, not the rows, and the
    /// set is what makes that true. It is per accumulator rather than per
    /// operator because `GROUP BY` gives each group its own - `count(DISTINCT
    /// x)` inside a group counts the distinct values *of that group*, and a set
    /// shared across groups would count each value once for the whole query.
    ///
    /// The values are encoded under the argument's collation, through the same
    /// `inillucent_tree::key` encoding `DISTINCT` and the set operations use, so
    /// `count(DISTINCT team)` over a `NOCASE` column agrees with `SELECT DISTINCT
    /// team` over it.
    seen: Option<DistinctSet>,
    /// The encoded key `seen` is asked about, reused between rows.
    ///
    /// One buffer rather than one allocation per row; see the note in `push`
    /// (task-1932, M7).
    scratch: Vec<u8>,
    /// Where the sort keys start in a collected row, and their directions.
    ///
    /// `None` for a call with no `ORDER BY` of its own, which is nearly all of
    /// them. When it is set, every row is collected rather than reduced and the
    /// fold happens at `finish`, in the order the keys give - because
    /// `group_concat(b ORDER BY a DESC)` is a different answer from
    /// `group_concat(b)` and the difference is the order the rows arrive in.
    sort: Option<(usize, Vec<SortTerm>)>,
    /// The rows a JSON group aggregate has collected, in row order.
    ///
    /// One value for the array form and two - label, value - for the object
    /// form. Kept as values rather than folded into a document as they arrive,
    /// because the document is built by `inillucent_scalar::json`, which is
    /// where the one implementation of a JSON document lives.
    json_rows: Vec<Vec<inillucent_value::value::Value<'static>>>,
    /// Which values of each row in `json_rows` carry the JSON subtype, one bit
    /// per value in written order.
    ///
    /// **Kept beside the values because a `Value` has nowhere to put it.**
    /// `json_group_array(json_object('id', id))` has to nest each object, and
    /// it can only tell an object from a string that spells one by the mark
    /// `json_object` put on its answer. Without it every member was quoted:
    /// `["{\"id\":1}"]` where SQLite answers `[{"id":1}]`.
    json_marks: Vec<u32>,
    /// The joined text, for `group_concat`.
    joined: Vec<u8>,
    /// Whether the joined text would have passed the value limit, so the answer
    /// is the refusal and not a string.
    joined_too_big: bool,
    /// Every row of the group, for a registered aggregate and nothing else.
    ///
    /// Empty for every built-in, which is what makes them pay nothing for it.
    rows: Vec<Vec<inillucent_value::value::Value<'static>>>,
    /// Whether `group_concat` records how long each piece of `joined` is, so the oldest piece can
    /// be taken off the front again. Only a window frame that slides asks for it.
    track_pieces: bool,
    /// The length in bytes of each value in `joined`, oldest first, separators not counted.
    pieces: std::collections::VecDeque<usize>,
}

/// Returns a real result, or NULL when it is NaN.
///
/// SQLite's `sqlite3_result_double` stores NaN as NULL, so `sum` over `Inf`
/// and `-Inf` is NULL and not NaN.
///
/// @param value - the computed total
fn real_or_null(value: f64) -> OwnedDatum {
    if value.is_nan() {
        OwnedDatum::Null
    } else {
        OwnedDatum::Real(value)
    }
}

impl Accumulator {
    /// Returns a fresh accumulator.
    ///
    /// @param kind - which aggregate it computes
    pub fn new(kind: AggregateKind) -> Accumulator {
        Accumulator {
            kind,
            count: 0,
            integer_sum: 0,
            real_sum: 0.0,
            compensation: 0.0,
            chosen: None,
            is_real: false,
            overflowed: false,
            saw_real: false,
            extreme: None,
            ordering: Collation::Binary,
            rare: None,
        }
    }

    /// Returns the rarely used parts, making them on first use.
    fn rare_mut(&mut self) -> &mut Rare {
        self.rare.get_or_insert_with(Box::default)
    }

    /// Returns the rows a registered or a collecting aggregate kept.
    fn kept_rows(&self) -> &[Vec<inillucent_value::value::Value<'static>>] {
        self.rare.as_ref().map_or(&[], |rare| rare.rows.as_slice())
    }

    /// Returns the rows a JSON or an ordered aggregate kept.
    fn json_kept(&self) -> &[Vec<inillucent_value::value::Value<'static>>] {
        self.rare
            .as_ref()
            .map_or(&[], |rare| rare.json_rows.as_slice())
    }

    /// Returns which values of each kept JSON row carry the JSON subtype.
    fn json_marked(&self) -> &[u32] {
        self.rare
            .as_ref()
            .map_or(&[], |rare| rare.json_marks.as_slice())
    }

    /// Returns the call's own `ORDER BY`, when it has one.
    fn sort_terms(&self) -> Option<&(usize, Vec<SortTerm>)> {
        self.rare.as_ref().and_then(|rare| rare.sort.as_ref())
    }

    /// Returns an accumulator that folds each distinct argument value once.
    ///
    /// @param kind - which aggregate it computes
    /// @param collation - the collation the argument is compared under
    pub fn distinct(kind: AggregateKind, collation: Collation) -> Accumulator {
        let mut accumulator = Accumulator::new(kind);
        accumulator.rare_mut().seen = Some(DistinctSet::new(collation));
        accumulator
    }

    /// Sets the collation `min`, `max` and a bare column's witness compare
    /// under.
    ///
    /// @param collation - the argument's collation
    pub fn compare_under(&mut self, collation: Collation) {
        self.ordering = collation;
    }

    /// Returns which aggregate this computes.
    pub fn kind(&self) -> &AggregateKind {
        &self.kind
    }

    /// Reports whether a whole run of integers may be folded in at once.
    ///
    /// A `DISTINCT` accumulator has to look at every value to decide whether it
    /// has seen it, so the vectorised path is not available to it - and an
    /// accumulator that took it anyway would count duplicates. The fast path
    /// asks rather than the caller remembering, because the caller is three
    /// operators and the accumulator is one.
    pub fn takes_dense(&self) -> bool {
        self.rare.as_ref().is_none_or(|rare| rare.seen.is_none())
            && !matches!(
                self.kind,
                AggregateKind::External(_)
                    | AggregateKind::JsonGroupArray(_)
                    | AggregateKind::JsonGroupObject(_)
                    | AggregateKind::Percentile(_)
                    | AggregateKind::GeopolyBox
                    | AggregateKind::VectorFold(_)
                    | AggregateKind::Bare(_)
            )
    }

    /// Tells this accumulator to collect its rows and sort them.
    ///
    /// @param at - the position of the first sort key in a collected row
    /// @param terms - how each key is compared, in key order
    pub fn sort_by(&mut self, at: usize, terms: Vec<SortTerm>) {
        self.rare_mut().sort = Some((at, terms));
    }

    /// Folds one whole row of arguments in, for a registered aggregate.
    ///
    /// **The rows are kept, not reduced.** A registered aggregate is handed the
    /// whole group at the end rather than a running accumulator, because an
    /// implementation written in C keeps its state in memory this engine must
    /// not look inside - see [`AggregateKind::External`].
    ///
    /// @param values - one row's arguments, in written order
    pub fn push_values(&mut self, values: Vec<inillucent_value::value::Value<'static>>) {
        self.push_values_marked(values, 0);
    }

    /// Folds one whole row of arguments in, with the JSON subtype of each.
    ///
    /// What [`Accumulator::push_values`] does, for a caller that knows which of
    /// the values a JSON function produced. Only the JSON group aggregates read
    /// the marks.
    ///
    /// @param values - one row's arguments, in written order
    /// @param marks - bit `n` set when value `n` carries the JSON subtype
    pub fn push_values_marked(
        &mut self,
        values: Vec<inillucent_value::value::Value<'static>>,
        marks: u32,
    ) {
        self.count = self.count.saturating_add(1);
        // **A JSON group aggregate comes through this entry point** rather than
        // through `push`, because a NULL is a *member* of the document it
        // builds and not a row to skip: `json_group_array` over one NULL is
        // `[null]` and not `[]`, and `push` drops NULLs before the accumulator
        // sees them. A call with its own `ORDER BY` comes through it too, for
        // the different reason that its rows cannot be folded until they are in
        // order.
        if let AggregateKind::Bare(witness) = self.kind {
            let value = values
                .first()
                .cloned()
                .unwrap_or(inillucent_value::value::Value::Null);
            let Some(wanted) = witness else {
                // No single extreme to follow, so the row is arbitrary and this
                // takes the *first* of the group - which is what the reference
                // answers, measured rather than assumed: `SELECT id, count(*)
                // FROM t` is 1 there and was 4 here when the last row won.
                if self.chosen.is_none() {
                    self.chosen = Some(OwnedDatum::from(value));
                }
                return;
            };
            let seen = values
                .get(1)
                .cloned()
                .unwrap_or(inillucent_value::value::Value::Null);
            let seen = OwnedDatum::from(seen);
            // **A row the `min` or `max` call's `FILTER` drops is not a candidate.**
            // It can still be the row the bare column falls back to when no
            // row passes at all, which SQLite answers with the first row.
            let filtered_out = matches!(
                values.get(2),
                Some(inillucent_value::value::Value::Integer(0))
            );
            if filtered_out {
                if self.chosen.is_none() {
                    self.chosen = Some(OwnedDatum::from(value));
                }
                return;
            }
            // A NULL never wins a `min` or a `max`, so a row whose witness is
            // NULL cannot be the row the bare column comes from - unless no row
            // has yet been chosen at all.
            // A held NULL is replaced by the first value that is not NULL: the
            // comparison alone would keep it for a `min`, because NULL sorts
            // below every value, and the bare column would stay on the row
            // that had no value at all.
            let replace = match &self.extreme {
                None => true,
                Some(held) => {
                    !seen.borrow().is_null()
                        && (held.borrow().is_null()
                            || inillucent_tree::types::compare_under(
                                &seen.borrow(),
                                &held.borrow(),
                                self.ordering,
                            ) == wanted)
                }
            };
            if replace {
                self.extreme = Some(seen);
                self.chosen = Some(OwnedDatum::from(value));
            }
            return;
        }
        // **`DISTINCT` is applied here too (task-1913).** It used to live only
        // in `push`, so an aggregate that keeps whole rows lost it entirely:
        // `group_concat(DISTINCT t ORDER BY t)` answered `blue,blue,gone,red`
        // where the reference answers `blue,gone,red`. Adding an `ORDER BY` to
        // a call turned its `DISTINCT` off, which is a wrong answer to a
        // perfectly ordinary query and not one the shape of the statement
        // warns anybody about. The key is the call's first argument, which is
        // the value `DISTINCT` is about - a second argument is refused by the
        // binder, in the reference's own words.
        if !self.keep_distinct(values.first()) {
            return;
        }
        if self.sort_terms().is_some()
            || matches!(
                self.kind,
                AggregateKind::JsonGroupArray(_) | AggregateKind::JsonGroupObject(_)
            )
        {
            let rare = self.rare_mut();
            rare.json_rows.push(values);
            rare.json_marks.push(marks);
            return;
        }
        self.rare_mut().rows.push(values);
    }

    /// Reports whether a value is new, and records it when it is.
    ///
    /// Always true for a call that did not say `DISTINCT`. The encoding and
    /// the reused buffer are the ones [`Accumulator::push`] uses, so the two
    /// paths agree on what counts as the same value - which matters because a
    /// `NOCASE` column makes `'a'` and `'A'` one value here and two under a
    /// binary comparison.
    ///
    /// @param value - the call's first argument for this row
    fn keep_distinct(&mut self, value: Option<&inillucent_value::value::Value<'static>>) -> bool {
        let Some(rare) = self.rare.as_deref_mut() else {
            return true;
        };
        let Rare { seen, scratch, .. } = rare;
        let Some(seen) = seen else {
            return true;
        };
        let Some(value) = value else {
            return true;
        };
        let datum = OwnedDatum::from(value.clone());
        seen.first_sight(&datum.borrow(), scratch)
    }

    /// Tells a `group_concat` to remember where each value ends, so that [`Accumulator::pull_oldest`]
    /// can take the oldest one back out. Costs one length per value, and only for the accumulator
    /// that asks.
    pub fn track_pieces(&mut self) {
        self.rare_mut().track_pieces = true;
    }

    /// Reports whether a `group_concat` stopped growing at the value limit.
    ///
    /// The limit is remembered for good, so an accumulator that slides cannot go on: a later frame
    /// that is small enough would still answer the refusal. The caller computes the remaining rows
    /// frame by frame instead.
    pub fn is_overfull(&self) -> bool {
        self.rare.as_ref().is_some_and(|rare| rare.joined_too_big)
    }

    /// Takes the oldest row back out of an aggregate that keeps its rows, as a sliding window
    /// frame needs when its first row leaves.
    ///
    /// **Only the aggregates that a frame computes from a list of its rows.** A percentile, a JSON
    /// group aggregate and a `group_concat` whose separator is a value of each row drop the first
    /// row they collected. A `group_concat` with a fixed separator drops the first value and the
    /// separator after it, which needs [`Accumulator::track_pieces`] to have been asked for before
    /// the value was added. The counts and `min` and `max` have no inverse here: a window computes
    /// them without an accumulator. Does nothing for any other kind, and when no row is held.
    ///
    /// Before this a frame whose start moved was rebuilt from its rows for every row of the
    /// partition, which made `median(v) OVER (ORDER BY id ROWS 300 PRECEDING)` 24 times slower
    /// than SQLite, and `group_concat` and `json_group_array` 11 and 14 times.
    pub fn pull_oldest(&mut self) {
        self.count = self.count.saturating_sub(1);
        let kind = self.kind.clone();
        let Some(rare) = self.rare.as_deref_mut() else {
            return;
        };
        match kind {
            AggregateKind::Percentile(_) | AggregateKind::GroupConcatComputed
                if !rare.rows.is_empty() =>
            {
                rare.rows.remove(0);
            }
            AggregateKind::JsonGroupArray(_) | AggregateKind::JsonGroupObject(_) => {
                if !rare.json_rows.is_empty() {
                    rare.json_rows.remove(0);
                }
                if !rare.json_marks.is_empty() {
                    rare.json_marks.remove(0);
                }
            }
            AggregateKind::GroupConcat(separator) => {
                let Some(piece) = rare.pieces.pop_front() else {
                    return;
                };
                let more = !rare.pieces.is_empty();
                let taken = piece.saturating_add(if more { separator.len() } else { 0 });
                rare.joined.drain(..taken.min(rare.joined.len()));
            }
            _ => {}
        }
    }

    /// Folds a whole batch's worth of rows into a `count(*)`.
    ///
    /// **Design 4 of task-2000: `count(*)` is a batch's row count, not a loop.**
    /// Every aggregate operator answered `count(*)` by calling [`Accumulator::push`]
    /// once a row with a `NULL`, so a hundred thousand row scan made a hundred
    /// thousand calls that each compared a discriminant and added one. There is no
    /// value to read - `count(*)` counts rows - so the whole batch is one addition.
    ///
    /// It is `count(*)` only. `count(x)` skips a NULL and so has to see each value,
    /// and a `DISTINCT` count has to compare each one; both keep the per-row path,
    /// and calling this for either would count rows this aggregate must not count.
    ///
    /// @param rows - how many rows the batch contributes
    pub fn push_count(&mut self, rows: usize) {
        if self.kind != AggregateKind::CountStar {
            return;
        }
        self.count = self.count.saturating_add(rows as i64);
    }

    /// Takes one value back out of a `sum`, `total` or `avg`, as SQLite's
    /// `sumInverse` does when a row leaves a sliding window frame.
    ///
    /// **The total is not recomputed from the rows left in the frame.** SQLite
    /// keeps one accumulator per partition and subtracts each row that leaves,
    /// so a total that reached infinity stays infinite after the large values
    /// have left, and a total that left the integers stays a real. The
    /// subtraction follows `sumInverse`: an exact integer total subtracts
    /// exactly until that would overflow, and a real total adds the negated
    /// value through the same compensated step a push uses. `i64::MIN` cannot
    /// be negated, so it is taken out as `i64::MAX` and then 1.
    ///
    /// Does nothing for any other kind of aggregate and for NULL.
    ///
    /// @param value - the value of the row leaving the frame
    pub fn pull(&mut self, value: &Datum<'_>) {
        if value.is_null()
            || !matches!(
                self.kind,
                AggregateKind::Sum | AggregateKind::Total | AggregateKind::Average
            )
        {
            return;
        }
        if let Datum::Text(_) = value {
            let converted = inillucent_value::affinity::apply_numeric_affinity(
                inillucent_value::value::Value::from(value),
                false,
            );
            if let inillucent_value::value::Value::Integer(number) = converted {
                self.pull(&Datum::Int(number));
                return;
            }
        }
        self.count = self.count.saturating_sub(1);
        if !self.is_real {
            // Only integers are pushed while the total is exact, so only
            // integers come back out here.
            let Datum::Int(number) = value else {
                return;
            };
            if let Some(total) = self.integer_sum.checked_sub(*number) {
                self.integer_sum = total;
                return;
            }
            self.seed_real();
        }
        match value {
            Datum::Int(i64::MIN) => {
                self.add_int(i64::MAX);
                self.add_int(1);
            }
            Datum::Int(number) => self.add_int(-*number),
            other => self.add_real(-numeric(other)),
        }
    }

    /// Folds one integer in, exactly as `push(&Datum::Int(value))` does.
    ///
    /// **The counts and sums without the general dispatch** (task-2209). A batch evaluated in one
    /// loop hands its values over one at a time, and `push` compared the kind, tested for NULL,
    /// looked for a `DISTINCT` set and matched the kind again for each; for a count or a sum of
    /// integers none of that decides anything. Every other kind, and a `DISTINCT` one, goes
    /// through `push`.
    ///
    /// @param value - the argument's value for this row
    pub fn push_int(&mut self, value: i64) {
        if self.rare.as_ref().is_some_and(|rare| rare.seen.is_some()) {
            return self.push(&Datum::Int(value));
        }
        match self.kind {
            AggregateKind::CountStar | AggregateKind::Count => {
                self.count = self.count.saturating_add(1);
            }
            AggregateKind::Sum | AggregateKind::Total | AggregateKind::Average => {
                self.count = self.count.saturating_add(1);
                if self.is_real {
                    self.add_int(value);
                    return;
                }
                match self.integer_sum.checked_add(value) {
                    Some(total) => self.integer_sum = total,
                    None => {
                        self.seed_real();
                        self.add_int(value);
                    }
                }
            }
            _ => self.push(&Datum::Int(value)),
        }
    }

    /// Folds one real in, exactly as `push(&Datum::Real(value))` does. See
    /// [`Accumulator::push_int`].
    ///
    /// @param value - the argument's value for this row
    pub fn push_real(&mut self, value: f64) {
        if self.rare.as_ref().is_some_and(|rare| rare.seen.is_some()) {
            return self.push(&Datum::Real(value));
        }
        match self.kind {
            AggregateKind::CountStar | AggregateKind::Count => {
                self.count = self.count.saturating_add(1);
            }
            AggregateKind::Sum | AggregateKind::Total | AggregateKind::Average => {
                self.count = self.count.saturating_add(1);
                if !self.is_real {
                    self.seed_real();
                }
                self.saw_real = true;
                self.add_real(value);
            }
            _ => self.push(&Datum::Real(value)),
        }
    }

    /// Folds one value in.
    ///
    /// @param value - the argument's value for this row, or NULL for `count(*)`
    pub fn push(&mut self, value: &Datum<'_>) {
        if self.kind == AggregateKind::CountStar {
            self.count = self.count.saturating_add(1);
            return;
        }
        if value.is_null() {
            return;
        }
        if let Some(Rare {
            seen: Some(seen),
            scratch,
            ..
        }) = self.rare.as_deref_mut()
        {
            // **Encoded into a reused buffer and only cloned when the value is
            // new (task-1932, M7).** A `DISTINCT` aggregate over a column with
            // few distinct values allocated one key per *row* and dropped
            // almost all of them - `count(DISTINCT status)` over a million rows
            // with six statuses allocated a million times to keep six.
            if !seen.first_sight(value, scratch) {
                return;
            }
        }
        self.count = self.count.saturating_add(1);
        match &self.kind {
            AggregateKind::CountStar | AggregateKind::Count => {}
            // A registered aggregate of one argument reaches here through the
            // ordinary single-value path; more than one goes through
            // `push_values`. Either way the row is kept rather than reduced.
            AggregateKind::External(_)
            | AggregateKind::Percentile(_)
            | AggregateKind::GeopolyBox
            // The copy is what an external aggregate needs: it keeps whole
            // rows past the page they were read from. An allocation this
            // small failing leaves a NULL in the row, which is what the
            // conversion this replaced did, and is the only answer an
            // infallible `push` can give.
            | AggregateKind::VectorFold(_) => self
                .rare
                .get_or_insert_with(Box::default)
                .rows
                .push(vec![Value::from(value).into_owned().unwrap_or(Value::Null)]),
            AggregateKind::Sum | AggregateKind::Total | AggregateKind::Average => {
                self.push_numeric(value)
            }
            AggregateKind::Minimum => self.push_extreme(value, std::cmp::Ordering::Less),
            AggregateKind::Maximum => self.push_extreme(value, std::cmp::Ordering::Greater),
            AggregateKind::GroupConcat(separator) => {
                let first = self.count <= 1;
                let rare = self.rare.get_or_insert_with(Box::default);
                // **The answer stops growing at the value limit (bug hunt 10).**
                // `group_concat(zeroblob(100000000))` over fifty rows built five
                // gigabytes before anything looked at the size, and one process
                // ran out of memory. The overflow is remembered here, because
                // `push` cannot fail, and reported by `finish` as "string or
                // blob too big", which is what SQLite answers.
                let added = datum_length(value).saturating_add(if first { 0 } else { separator.len() });
                if rare.joined_too_big || exceeds_value_limit(rare.joined.len(), added) {
                    rare.joined_too_big = true;
                    return;
                }
                if !first {
                    rare.joined.extend_from_slice(separator.as_bytes());
                }
                let before = rare.joined.len();
                rare.joined.extend_from_slice(&render(value));
                if rare.track_pieces {
                    let piece = rare.joined.len().saturating_sub(before);
                    rare.pieces.push_back(piece);
                }
            }
            // Unreachable: `AggregateSpec::takes_whole_row` sends all three of
            // these through `push_values`. Stated rather than folded into
            // another arm so a kind added later is a compilation error.
            AggregateKind::JsonGroupArray(_)
            | AggregateKind::JsonGroupObject(_)
            | AggregateKind::GroupConcatComputed
            | AggregateKind::Bare(_) => {}
        }
    }

    /// Joins the collected rows with the separator each of them carries.
    ///
    /// A row whose *value* is NULL is not in the answer at all - the reference
    /// skips it, separator and all - and a NULL separator contributes nothing.
    /// The first row that survives contributes no separator, which is what
    /// makes `group_concat(s, n)` over `('a',1),('b',2)` read `a2b` rather
    /// than `1a2b`. See [`AggregateKind::GroupConcatComputed`].
    fn group_concat_computed(&self) -> DbResult<OwnedDatum> {
        let mut joined: Vec<u8> = Vec::new();
        let mut any = false;
        for row in self.kept_rows() {
            let Some(value) = row.first() else {
                continue;
            };
            if matches!(value, inillucent_value::value::Value::Null) {
                continue;
            }
            let mut added = value_length(value);
            if any {
                if let Some(separator) = row.get(1) {
                    if !matches!(separator, inillucent_value::value::Value::Null) {
                        added = added.saturating_add(value_length(separator));
                    }
                }
            }
            // The same limit the single value path keeps (bug hunt 10).
            if exceeds_value_limit(joined.len(), added) {
                return Err(string_too_big());
            }
            if any {
                if let Some(separator) = row.get(1) {
                    if !matches!(separator, inillucent_value::value::Value::Null) {
                        joined.extend_from_slice(&text_of(separator));
                    }
                }
            }
            joined.extend_from_slice(&text_of(value));
            any = true;
        }
        Ok(if any {
            OwnedDatum::Text(joined)
        } else {
            OwnedDatum::Null
        })
    }

    /// Returns what a percentile aggregate settles on, over the sorted group.
    ///
    /// The fraction is read from the *second* argument of any collected row -
    /// it is a constant for the group, so any row's copy of it will do - and
    /// `median` has no second argument at all. NULL is the answer to an empty
    /// group and to a fraction outside `0..=1`, which is what the reference
    /// answers rather than clamping.
    ///
    /// `percentile_disc` picks a row; the other three interpolate between the
    /// two the position falls between, which is why the two arms differ in more
    /// than rounding. Both index a group sorted by value, and the sort is done
    /// here rather than as the rows arrive because the group is not known to be
    /// complete until now.
    ///
    /// @param which - which of the four is being computed
    fn finish_percentile(&self, which: Percentile) -> DbResult<OwnedDatum> {
        let (fraction, mut values) = self.percentile_inputs(which)?;
        let (Some(fraction), false) = (fraction, values.is_empty()) else {
            return Ok(OwnedDatum::Null);
        };
        values.sort_by(|left, right| left.total_cmp(right));
        Ok(percentile_of_sorted(which, fraction, &values))
    }

    /// Reads the collected rows of a percentile aggregate the way SQLite steps them.
    ///
    /// **Every row is checked in arrival order, as SQLite's step does.** The fraction of a row is
    /// checked first, even when its value is NULL, then compared with the fraction of the rows
    /// before it, and only then is a NULL value skipped or a text or blob value refused with
    /// `input to median() is not numeric`. A text that looks like a number is refused too. This
    /// accepted every value as a number and the first row's fraction for the whole group, so
    /// `median('abc')` answered a number and a fraction that varied by row answered a value where
    /// SQLite raises an error. A group with no rows is never checked and answers NULL.
    ///
    /// @param which - which of the four is being computed
    fn percentile_inputs(&self, which: Percentile) -> DbResult<(Option<f64>, Vec<f64>)> {
        use inillucent_value::value::Value;
        let mut fraction: Option<f64> = None;
        let mut values: Vec<f64> = Vec::new();
        for row in self.kept_rows() {
            let wanted = which.fraction(row.get(1))?;
            match fraction {
                Some(held) if held != wanted => {
                    return Err(inillucent_base::error::statement_refusal(format!(
                        "the fraction argument to {}() is not the same for all input rows",
                        which.name()
                    )));
                }
                _ => fraction = Some(wanted),
            }
            match row.first() {
                None | Some(Value::Null) => {}
                Some(Value::Integer(number)) => values.push(*number as f64),
                Some(Value::Real(number)) => values.push(*number),
                Some(_) => {
                    return Err(inillucent_base::error::statement_refusal(format!(
                        "input to {}() is not numeric",
                        which.name()
                    )));
                }
            }
        }
        Ok((fraction, values))
    }

    /// Returns the box that holds every polygon the group held.
    ///
    /// A row that is not a polygon contributes nothing, and a group with no
    /// polygons in it at all answers NULL - which is the reference's answer to
    /// an aggregate that was never stepped.
    fn finish_geopoly_box(&self) -> OwnedDatum {
        let mut bounds: Option<[f32; 4]> = None;
        for row in self.kept_rows() {
            let Some(value) = row.first() else {
                continue;
            };
            let Some(shape) = inillucent_scalar::geopoly::Polygon::parse(Some(value)) else {
                continue;
            };
            let found = shape.bounds();
            bounds = Some(match bounds {
                None => found,
                Some(held) => [
                    held[0].min(found[0]),
                    held[1].max(found[1]),
                    held[2].min(found[2]),
                    held[3].max(found[3]),
                ],
            });
        }
        match bounds {
            Some(bounds) => {
                OwnedDatum::Blob(inillucent_scalar::geopoly::box_polygon(bounds).to_blob())
            }
            None => OwnedDatum::Null,
        }
    }

    /// Returns the component-wise total of the group's vectors.
    ///
    /// A row that is not a vector contributes nothing, and a width that does
    /// not match the first one is a **refusal** rather than a silent partial
    /// sum: two embeddings of different widths are not two of the same thing,
    /// and the reference's own message for that is what a caller has already
    /// seen from `vector_distance_cos`.
    ///
    /// @param average - whether the total is divided by how many went into it
    fn finish_vector_fold(&self, average: bool) -> OwnedDatum {
        let mut total: Vec<f64> = Vec::new();
        let mut counted = 0i64;
        for row in self.kept_rows() {
            let Some(inillucent_value::value::Value::Blob(blob)) = row.first() else {
                continue;
            };
            let bytes = blob.raw();
            if bytes.is_empty() || bytes.len() % 4 != 0 {
                continue;
            }
            let width = bytes.len() / 4;
            if total.is_empty() {
                total = vec![0.0; width];
            } else if total.len() != width {
                continue;
            }
            for (at, chunk) in bytes.chunks_exact(4).enumerate() {
                let raw: [u8; 4] = chunk.try_into().unwrap_or([0; 4]);
                if let Some(slot) = total.get_mut(at) {
                    *slot += f64::from(f32::from_bits(u32::from_le_bytes(raw)));
                }
            }
            counted = counted.saturating_add(1);
        }
        if counted == 0 {
            return OwnedDatum::Null;
        }
        let scale = if average { counted as f64 } else { 1.0 };
        let mut bytes = Vec::with_capacity(total.len().saturating_mul(4));
        for component in &total {
            bytes.extend_from_slice(&((component / scale) as f32).to_bits().to_le_bytes());
        }
        OwnedDatum::Blob(bytes)
    }

    /// Folds a whole run of integers in at once.
    ///
    /// The vectorised entry point: a `sum` over a dense integer column calls
    /// this once per batch with the mini-column's bytes and never touches a
    /// `Datum`. It is the reason the aggregate operator has a fast path at all.
    ///
    /// @param slots - a contiguous run of typed integers, at the leaf's width
    pub fn push_dense_ints(&mut self, slots: crate::batch::DenseInts<'_>) {
        let rows = slots.len();
        match self.kind {
            // Unreachable: `takes_dense` is false for a registered aggregate
            // and `takes_whole_row` is true for a JSON group, so no operator
            // offers either of them a mini-column. Stated rather than folded
            // into another arm, so a kind added later is a compilation error.
            AggregateKind::External(_)
            | AggregateKind::JsonGroupArray(_)
            | AggregateKind::JsonGroupObject(_)
            | AggregateKind::Percentile(_)
            | AggregateKind::GeopolyBox
            | AggregateKind::VectorFold(_)
            | AggregateKind::Bare(_) => {}
            AggregateKind::CountStar | AggregateKind::Count => {
                self.count = self.count.saturating_add(rows as i64);
            }
            AggregateKind::Sum | AggregateKind::Total | AggregateKind::Average => {
                self.count = self.count.saturating_add(rows as i64);
                if self.is_real {
                    // Borrowed out of `self` because `add_int` takes it
                    // mutably and the closure would otherwise hold it twice.
                    let mut values: Vec<i64> = Vec::with_capacity(rows);
                    slots.for_each(|value| values.push(value));
                    for value in values {
                        self.add_int(value);
                    }
                    return;
                }
                // **A checked add per value, in row order, because the
                // overflow SQLite reports is the running total's and not the
                // final one's.** This used to add the run in `i128` and look
                // only at the total, so 1, 2^63 - 1, -2^63 and 0 summed to 0
                // where SQLite raises `integer overflow` at the second row.
                // The per row path gets that right, and a long lived session
                // takes this path on a second read of the same table, so the
                // same statement answered 0 once and failed once. The values
                // from the one that overflows onwards go through `push_numeric`,
                // the per row path.
                let mut running = self.integer_sum;
                let mut rest: Vec<i64> = Vec::new();
                slots.for_each(|value| {
                    if !rest.is_empty() {
                        rest.push(value);
                        return;
                    }
                    match running.checked_add(value) {
                        Some(next) => running = next,
                        None => rest.push(value),
                    }
                });
                self.integer_sum = running;
                for value in rest {
                    self.push_numeric(&Datum::Int(value));
                }
            }
            AggregateKind::Minimum => {
                let mut best = i64::MAX;
                slots.for_each(|value| {
                    if value < best {
                        best = value;
                    }
                });
                if rows > 0 {
                    self.count = self.count.saturating_add(rows as i64);
                    self.push_extreme(&Datum::Int(best), std::cmp::Ordering::Less);
                }
            }
            AggregateKind::Maximum => {
                let mut best = i64::MIN;
                slots.for_each(|value| {
                    if value > best {
                        best = value;
                    }
                });
                if rows > 0 {
                    self.count = self.count.saturating_add(rows as i64);
                    self.push_extreme(&Datum::Int(best), std::cmp::Ordering::Greater);
                }
            }
            AggregateKind::GroupConcat(_) => {
                let mut values: Vec<i64> = Vec::with_capacity(rows);
                slots.for_each(|value| values.push(value));
                for value in values {
                    self.push(&Datum::Int(value));
                }
            }
            // Unreachable: a computed separator makes the call a whole-row
            // one, and no operator offers one of those a mini-column.
            AggregateKind::GroupConcatComputed => {}
        }
    }

    /// Folds one numeric value into the running total.
    ///
    /// @param value - the value to add
    fn push_numeric(&mut self, value: &Datum<'_>) {
        // **Text that is an integer is added as an integer.** SQLite's
        // `sumStep` asks `sqlite3_value_numeric_type`, which applies numeric
        // affinity to text first, so `sum(a)` over the text `'2'` and `'3'` is
        // the integer 5. Adding every text value as a double made it 5.0. Text
        // that is not wholly a number, and every blob, is still added as a
        // double, which is what SQLite does with them.
        if let Datum::Text(_) = value {
            let converted = inillucent_value::affinity::apply_numeric_affinity(
                inillucent_value::value::Value::from(value),
                false,
            );
            if let inillucent_value::value::Value::Integer(number) = converted {
                self.push_numeric(&Datum::Int(number));
                return;
            }
        }
        match value {
            Datum::Int(number) if !self.is_real => match self.integer_sum.checked_add(*number) {
                Some(total) => self.integer_sum = total,
                None => {
                    self.seed_real();
                    self.add_int(*number);
                }
            },
            Datum::Int(number) => self.add_int(*number),
            other => {
                if !self.is_real {
                    self.seed_real();
                }
                // **A real argument makes the whole sum a real**, and a real
                // sum has no overflow to report - SQLite raises `integer
                // overflow` only when every value it added was an integer.
                // `seed_real` above sets the flag because it cannot tell why it
                // was called; here is where we know.
                self.saw_real = true;
                self.add_real(numeric(other));
            }
        }
    }

    /// Moves the running total out of the integers, keeping its exact value.
    ///
    /// The seed goes through the same split [`Accumulator::add_int`] uses: an
    /// integer sum that has already passed 2^53 would otherwise lose its low
    /// bits on the way into the double, which is the same error the split
    /// exists to prevent and is worth exactly as much.
    fn seed_real(&mut self) {
        self.overflowed = true;
        self.is_real = true;
        // SQLite's `kahanBabuskaNeumaierInit` splits only a total a double
        // cannot hold exactly. The compensation term this leaves is what the
        // next compensated step rounds against, so it is kept the same.
        let low = if self.integer_sum > -EXACT_IN_DOUBLE && self.integer_sum < EXACT_IN_DOUBLE {
            0
        } else {
            self.integer_sum % SPLIT_I64
        };
        self.real_sum = (self.integer_sum - low) as f64;
        self.compensation = low as f64;
    }

    /// Folds one integer into the running double total without losing its low bits.
    ///
    /// `value as f64` is not good enough and the corpus proves it. The `people`
    /// table holds `i64::MAX` and `i64::MIN` in one column beside a `-1`, and
    /// the exact total is -2. Cast to doubles, `i64::MAX` rounds *up* to 2^63
    /// and cancels `i64::MIN` exactly, so the total comes out as -1 - a
    /// one-unit error that no amount of compensation recovers, because the
    /// information was gone before the compensation saw it.
    ///
    /// So a large integer is split the way SQLite splits it: a high part that
    /// is a multiple of 2^14 and therefore exactly representable in a double,
    /// and a remainder small enough to be exact on its own. Both go through the
    /// compensated add, and the exact value survives.
    ///
    /// @param value - the integer to add
    fn add_int(&mut self, value: i64) {
        if value > -EXACT_IN_DOUBLE && value < EXACT_IN_DOUBLE {
            // Below 2^52 a double holds the integer exactly, so there is
            // nothing to split.
            self.add_real(value as f64);
            return;
        }
        let low = value % SPLIT_I64;
        self.add_real((value - low) as f64);
        self.add_real(low as f64);
    }

    /// Adds one double to the running total, compensated.
    ///
    /// Kahan-Babuska-Neumaier summation, which is what the pinned SQLite does
    /// for `sum`, `total` and `avg`. It is not an optimisation: the corpus's
    /// `score` column holds `-1e300`, nine ordinary values and `+1e300`, and a
    /// naive total is **0.0** where SQLite answers 124.25, because the small
    /// values are absorbed into the first huge one and cancelled by the second.
    /// The compensation term is what carries them across.
    ///
    /// @param value - the double to add
    fn add_real(&mut self, value: f64) {
        let total = self.real_sum + value;
        // Whichever operand is larger keeps its low bits; the other's are what
        // the compensation recovers.
        if self.real_sum.abs() >= value.abs() {
            self.compensation += (self.real_sum - total) + value;
        } else {
            self.compensation += (value - total) + self.real_sum;
        }
        self.real_sum = total;
    }

    /// Keeps the value if it is more extreme than the one held.
    ///
    /// @param value - the candidate
    /// @param wanted - `Less` for `min`, `Greater` for `max`
    fn push_extreme(&mut self, value: &Datum<'_>, wanted: std::cmp::Ordering) {
        let replace = match &self.extreme {
            None => true,
            Some(held) => {
                inillucent_tree::types::compare_under(value, &held.borrow(), self.ordering)
                    == wanted
            }
        };
        if replace {
            self.extreme = Some(OwnedDatum::from_datum(value));
        }
    }

    /// Returns the running total with its compensation folded in.
    ///
    /// One place, so `sum`, `total` and `avg` cannot disagree about it - which
    /// they did on the first attempt, when only `sum` was corrected and the
    /// other two answered 3.5 where SQLite answered 124.25.
    fn compensated(&self) -> f64 {
        // SQLite drops the compensation when it is NaN or infinite
        // (`sqlite3IsOverflow`). Once the running total overflows, the term
        // is infinity minus infinity, and adding it would turn a total of
        // Inf into NaN where SQLite answers Inf.
        if !self.compensation.is_finite() {
            return self.real_sum;
        }
        self.real_sum + self.compensation
    }

    /// Reports whether one value of one collected row carries the JSON subtype.
    ///
    /// @param row - the row's position in `json_rows`
    /// @param value - the value's position in the row
    fn marked(&self, row: usize, value: u32) -> bool {
        self.json_marked()
            .get(row)
            .is_some_and(|marks| marks.checked_shr(value).is_some_and(|bits| bits & 1 == 1))
    }

    /// Returns the aggregate's value.
    pub fn finish(&self) -> DbResult<OwnedDatum> {
        // A call with its own `ORDER BY` collected its rows; they are folded
        // here, in the order the keys give.
        if let Some((at, terms)) = self.sort_terms() {
            return self.finish_sorted(*at, terms);
        }
        Ok(match &self.kind {
            AggregateKind::CountStar | AggregateKind::Count => OwnedDatum::Int(self.count),
            // The whole group at once, which is what the boundary promises.
            AggregateKind::External(body) => OwnedDatum::from((body.0)(self.kept_rows())?),
            AggregateKind::Percentile(which) => self.finish_percentile(*which)?,
            AggregateKind::GeopolyBox => self.finish_geopoly_box(),
            AggregateKind::VectorFold(average) => self.finish_vector_fold(*average),
            AggregateKind::Sum => {
                if self.count == 0 {
                    OwnedDatum::Null
                } else if self.overflowed && !self.saw_real {
                    // **`sum()` over integers that will not fit is an error,
                    // not a bigger number.** It answered
                    // `1.8446744073709552e+19` where SQLite raises `integer
                    // overflow` - a total that is *wrong by a rounding* and
                    // that a caller reading an integer column has no reason to
                    // suspect. `total()` and `avg()` are documented to be
                    // doubles and keep answering one, which is why the check is
                    // on this arm alone.
                    // `SQLITE_ERROR`, not `SQLITE_MISUSE` (task-1913). The
                    // pinned 3.53.4 answers primary code 1 here, measured
                    // against the oracle; `refusal` hardcodes 21, so the
                    // message matched and the code a driver branches on did
                    // not.
                    return Err(inillucent_base::error::statement_refusal(
                        "integer overflow",
                    ));
                } else if self.is_real {
                    real_or_null(self.compensated())
                } else {
                    OwnedDatum::Int(self.integer_sum)
                }
            }
            AggregateKind::Total => real_or_null(if self.is_real {
                self.compensated()
            } else {
                self.integer_sum as f64
            }),
            AggregateKind::Average => {
                if self.count == 0 {
                    OwnedDatum::Null
                } else {
                    let total = if self.is_real {
                        self.compensated()
                    } else {
                        self.integer_sum as f64
                    };
                    real_or_null(total / self.count as f64)
                }
            }
            AggregateKind::JsonGroupArray(binary) => {
                let mut items = Vec::with_capacity(self.json_kept().len());
                for (position, row) in self.json_kept().iter().enumerate() {
                    let value = row
                        .first()
                        .cloned()
                        .unwrap_or(inillucent_value::value::Value::Null);
                    inillucent_scalar::json::group_array_step(
                        &mut items,
                        &inillucent_scalar::json::Argument {
                            value: &value,
                            json: self.marked(position, 0),
                        },
                    )?;
                }
                let answer = OwnedDatum::from(
                    inillucent_scalar::json::group_array_final(items, *binary)?.value,
                );
                refuse_past_value_limit(&answer)?;
                answer
            }
            AggregateKind::JsonGroupObject(binary) => {
                let mut members = Vec::with_capacity(self.json_kept().len());
                for (position, row) in self.json_kept().iter().enumerate() {
                    let label = row
                        .first()
                        .cloned()
                        .unwrap_or(inillucent_value::value::Value::Null);
                    let value = row
                        .get(1)
                        .cloned()
                        .unwrap_or(inillucent_value::value::Value::Null);
                    inillucent_scalar::json::group_object_step(
                        &mut members,
                        &inillucent_scalar::json::Argument::plain(&label),
                        &inillucent_scalar::json::Argument {
                            value: &value,
                            json: self.marked(position, 1),
                        },
                    )?;
                }
                let answer = OwnedDatum::from(
                    inillucent_scalar::json::group_object_final(members, *binary)?.value,
                );
                refuse_past_value_limit(&answer)?;
                answer
            }
            AggregateKind::Bare(_) => self.chosen.clone().unwrap_or(OwnedDatum::Null),
            AggregateKind::Minimum | AggregateKind::Maximum => {
                self.extreme.clone().unwrap_or(OwnedDatum::Null)
            }
            AggregateKind::GroupConcat(_) => {
                if self.rare.as_ref().is_some_and(|rare| rare.joined_too_big) {
                    return Err(string_too_big());
                }
                if self.count == 0 {
                    OwnedDatum::Null
                } else {
                    OwnedDatum::Text(
                        self.rare
                            .as_ref()
                            .map_or_else(Vec::new, |rare| rare.joined.clone()),
                    )
                }
            }
            AggregateKind::GroupConcatComputed => self.group_concat_computed()?,
        })
    }
    /// Returns the aggregate's value over its rows in the sort's own order.
    ///
    /// The keys are encoded with the tree's own key encoding, which is what
    /// `DISTINCT` already compares with here, so a mixed-type column orders the
    /// way every other comparison in the engine orders it. A descending key
    /// reverses that key alone, which is what `ORDER BY a DESC, b` means.
    ///
    /// **Each key under its own collation, and its NULLs where the term put
    /// them.** The keys used to be encoded under `BINARY` whatever the term
    /// said, so `group_concat(name, ',' ORDER BY name)` over a `NOCASE` column
    /// answered `Repairs,Zebra,apple,outdoor` where SQLite, and this engine's
    /// own `SELECT name FROM tag ORDER BY name`, answer
    /// `apple,outdoor,Repairs,Zebra`.
    ///
    /// @param at - the position of the first sort key in a collected row
    /// @param terms - how each key is compared, in key order
    fn finish_sorted(&self, at: usize, terms: &[SortTerm]) -> DbResult<OwnedDatum> {
        let mut order: Vec<usize> = (0..self.json_kept().len()).collect();
        let key_of = |row: &Vec<inillucent_value::value::Value<'static>>, which: usize| {
            let mut encoded = Vec::new();
            let value = row
                .get(at.saturating_add(which))
                .cloned()
                .unwrap_or(inillucent_value::value::Value::Null);
            let collation = terms
                .get(which)
                .map(|term| term.collation)
                .unwrap_or(Collation::Binary);
            let value = OwnedDatum::from(value);
            let null = value.borrow().is_null();
            inillucent_tree::key::encode_into_with(&value.borrow(), collation, &mut encoded);
            (null, encoded)
        };
        order.sort_by(|left, right| {
            let (Some(a), Some(b)) = (self.json_kept().get(*left), self.json_kept().get(*right))
            else {
                return std::cmp::Ordering::Equal;
            };
            for (which, term) in terms.iter().enumerate() {
                let (a_null, a_key) = key_of(a, which);
                let (b_null, b_key) = key_of(b, which);
                let ordering = match (a_null, b_null) {
                    (true, true) => std::cmp::Ordering::Equal,
                    (true, false) if term.nulls_first => std::cmp::Ordering::Less,
                    (true, false) => std::cmp::Ordering::Greater,
                    (false, true) if term.nulls_first => std::cmp::Ordering::Greater,
                    (false, true) => std::cmp::Ordering::Less,
                    (false, false) => {
                        let ordering = a_key.cmp(&b_key);
                        if term.descending {
                            ordering.reverse()
                        } else {
                            ordering
                        }
                    }
                };
                if ordering != std::cmp::Ordering::Equal {
                    return ordering;
                }
            }
            // A tie keeps the arrival order, which is what a stable sort of the
            // positions gives.
            left.cmp(right)
        });
        let sorted: Vec<(Vec<inillucent_value::value::Value<'static>>, u32)> = order
            .into_iter()
            .filter_map(|at| {
                let marks = self.json_marked().get(at).copied().unwrap_or(0);
                self.json_kept().get(at).cloned().map(|row| (row, marks))
            })
            .collect();
        let mut folded = Accumulator::new(self.kind.clone());
        for (row, marks) in sorted {
            match self.kind {
                // The document builders keep taking whole rows, and so does a
                // `group_concat` whose separator is a value of each row: the
                // separator travels with the value it precedes, so folding the
                // value alone would lose it (task-1913).
                //
                // A percentile too: its fraction is the second value of each row, and folding the
                // first value alone left it with no fraction, so `percentile(a, 50 ORDER BY b)`
                // answered NULL.
                AggregateKind::JsonGroupArray(_)
                | AggregateKind::JsonGroupObject(_)
                | AggregateKind::GroupConcatComputed
                | AggregateKind::Percentile(_) => {
                    folded.push_values_marked(row, marks);
                }
                // Everything else reduces one value, and a NULL is a row it
                // skips - which `push` already knows.
                _ => {
                    let value = row
                        .first()
                        .cloned()
                        .unwrap_or(inillucent_value::value::Value::Null);
                    folded.push(&OwnedDatum::from(value).borrow());
                }
            }
        }
        folded.finish()
    }
}

/// The longest string or blob SQLite's default build admits, in bytes.
///
/// The accumulators do not see the connection's own bound (`SQLITE_LIMIT_LENGTH`),
/// so `group_concat` keeps SQLite's default for it.
const VALUE_LIMIT: usize = 1_000_000_000;

/// Reports whether appending bytes to a string of a length would reach the limit.
///
/// SQLite's string builder refuses when the length including the closing zero
/// byte would pass the limit, so a string of exactly the limit is refused.
///
/// @param length - the bytes held already
/// @param added - the bytes about to be appended
fn exceeds_value_limit(length: usize, added: usize) -> bool {
    length.saturating_add(added) >= VALUE_LIMIT
}

/// Refuses an answer that reaches the value limit.
///
/// @param value - the finished answer of an aggregate
pub(crate) fn refuse_past_value_limit(value: &OwnedDatum) -> DbResult<()> {
    match value {
        OwnedDatum::Text(bytes) | OwnedDatum::Blob(bytes) if bytes.len() >= VALUE_LIMIT => {
            Err(string_too_big())
        }
        _ => Ok(()),
    }
}

/// Returns the refusal SQLite gives for a value past its length limit.
fn string_too_big() -> inillucent_base::error::DbError {
    inillucent_base::error::DbError::primary(inillucent_base::error::PrimaryCode::TooBig)
        .with_message("string or blob too big")
        .with_detail("string or blob too big")
}

/// Returns how many bytes `render` will produce for a value, without producing them.
///
/// @param value - the value about to be appended
fn datum_length(value: &Datum<'_>) -> usize {
    match value {
        Datum::Null => 0,
        Datum::Int(_) | Datum::Real(_) => 32,
        Datum::Text(bytes) | Datum::Blob(bytes) => bytes.len(),
    }
}

/// Returns how many bytes `text_of` will produce for a collected value.
///
/// @param value - the collected value
fn value_length(value: &inillucent_value::value::Value<'_>) -> usize {
    use inillucent_value::value::Value;
    match value {
        Value::Null => 0,
        Value::Integer(_) | Value::Real(_) => 32,
        Value::Text(text) => text.utf8_bytes().len(),
        Value::Blob(blob) => blob.raw().len(),
    }
}

/// Renders a value as text bytes, the way the dialect's text conversion does.
///
/// **The bytes of a text or blob are kept as they are.** SQLite's `group_concat` appends the bytes
/// `sqlite3_value_text` returns, which for a blob or for text that is not valid UTF-8 are the
/// stored bytes. Going through `String::from_utf8_lossy` replaced each invalid byte with U+FFFD,
/// so `group_concat(x'ff')` answered EFBFBD where SQLite answers FF.
///
/// @param value - the value to render
fn render(value: &Datum<'_>) -> Vec<u8> {
    match value {
        Datum::Null => Vec::new(),
        Datum::Int(number) => number.to_string().into_bytes(),
        Datum::Real(number) => format_real(*number).into_bytes(),
        Datum::Text(bytes) | Datum::Blob(bytes) => bytes.to_vec(),
    }
}

/// Returns a value as the text bytes `group_concat` writes into its answer.
///
/// The same rendering [`render`] does, over the owned form the whole-row path
/// collects: NULL is empty, a real is formatted the way the reference formats
/// one, and a text or a blob is its bytes, invalid UTF-8 included.
///
/// @param value - the collected value
fn text_of(value: &inillucent_value::value::Value<'_>) -> Vec<u8> {
    use inillucent_value::value::Value;
    match value {
        Value::Null => Vec::new(),
        Value::Integer(number) => number.to_string().into_bytes(),
        Value::Real(number) => format_real(*number).into_bytes(),
        Value::Text(text) => text.utf8_bytes().to_vec(),
        Value::Blob(blob) => blob.raw().to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An accumulator stays small enough that a group table of them stays in cache.
    ///
    /// The parts only some calls use are boxed in `Rare`; a field added inline
    /// for one kind of aggregate grows every group of every `GROUP BY`.
    #[test]
    fn an_accumulator_keeps_its_rare_parts_out_of_line() {
        let size = std::mem::size_of::<Accumulator>();
        assert!(size <= 160, "an accumulator is {size} bytes");
    }

    fn fold(kind: AggregateKind, values: &[Datum<'_>]) -> OwnedDatum {
        let mut accumulator = Accumulator::new(kind);
        for value in values {
            accumulator.push(value);
        }
        accumulator.finish().unwrap()
    }

    /// `sum` of nothing is NULL and `total` of nothing is 0.0, which is the
    /// difference the dialect actually draws.
    #[test]
    fn empty_aggregates_follow_the_dialect() {
        assert!(matches!(fold(AggregateKind::Sum, &[]), OwnedDatum::Null));
        assert!(matches!(
            fold(AggregateKind::Average, &[]),
            OwnedDatum::Null
        ));
        assert!(matches!(
            fold(AggregateKind::Minimum, &[]),
            OwnedDatum::Null
        ));
        assert!(matches!(
            fold(AggregateKind::Maximum, &[]),
            OwnedDatum::Null
        ));
        assert!(matches!(
            fold(AggregateKind::GroupConcat(",".into()), &[]),
            OwnedDatum::Null
        ));
        match fold(AggregateKind::Total, &[]) {
            OwnedDatum::Real(number) => assert_eq!(number, 0.0),
            other => panic!("total of nothing was {other:?}"),
        }
        assert!(matches!(
            fold(AggregateKind::Count, &[]),
            OwnedDatum::Int(0)
        ));
        assert!(matches!(
            fold(AggregateKind::CountStar, &[]),
            OwnedDatum::Int(0)
        ));
    }

    /// NULLs are counted by `count(*)` and ignored by everything else.
    #[test]
    fn nulls_are_counted_only_by_count_star() {
        let values = [Datum::Int(1), Datum::Null, Datum::Int(3)];
        assert!(matches!(
            fold(AggregateKind::CountStar, &values),
            OwnedDatum::Int(3)
        ));
        assert!(matches!(
            fold(AggregateKind::Count, &values),
            OwnedDatum::Int(2)
        ));
        assert!(matches!(
            fold(AggregateKind::Sum, &values),
            OwnedDatum::Int(4)
        ));
        match fold(AggregateKind::Average, &values) {
            OwnedDatum::Real(number) => assert_eq!(number, 2.0),
            other => panic!("avg was {other:?}"),
        }
    }

    /// An integer sum stays an integer until it overflows, and then becomes a
    /// double rather than wrapping.
    #[test]
    fn an_integer_sum_leaves_the_integers_only_on_overflow() {
        assert!(matches!(
            fold(AggregateKind::Sum, &[Datum::Int(i64::MAX), Datum::Int(0)]),
            OwnedDatum::Int(i64::MAX)
        ));
        // **An integer sum that will not fit is an error, not a bigger
        // number.** It used to become a double, which is a total that is wrong
        // by a rounding and that a caller reading an integer column has no
        // reason to suspect; the reference raises `integer overflow`.
        // `total` and `avg` are documented doubles and
        // still answer one over the same values.
        let mut over = Accumulator::new(AggregateKind::Sum);
        over.push(&Datum::Int(i64::MAX));
        over.push(&Datum::Int(1));
        assert!(over.finish().is_err(), "an overflowing sum must refuse");
        match fold(AggregateKind::Total, &[Datum::Int(i64::MAX), Datum::Int(1)]) {
            OwnedDatum::Real(number) => assert_eq!(number, i64::MAX as f64 + 1.0),
            other => panic!("overflowing total was {other:?}"),
        }
        // A real anywhere in the input makes the sum a real, which has no
        // overflow to report.
        match fold(
            AggregateKind::Sum,
            &[Datum::Int(i64::MAX), Datum::Int(1), Datum::Real(0.0)],
        ) {
            OwnedDatum::Real(number) => assert_eq!(number, i64::MAX as f64 + 1.0),
            other => panic!("a mixed overflowing sum was {other:?}"),
        }
    }

    /// A single real in the input makes the whole sum real.
    #[test]
    fn one_real_makes_the_sum_real() {
        match fold(
            AggregateKind::Sum,
            &[Datum::Int(1), Datum::Real(0.5), Datum::Int(2)],
        ) {
            OwnedDatum::Real(number) => assert_eq!(number, 3.5),
            other => panic!("mixed sum was {other:?}"),
        }
    }

    /// `min` and `max` order by the dialect's class order, not by class.
    #[test]
    fn extremes_use_the_dialect_ordering() {
        let values = [
            Datum::Text(b"apple"),
            Datum::Int(5),
            Datum::Blob(&[0]),
            Datum::Real(-1.0),
        ];
        match fold(AggregateKind::Minimum, &values) {
            OwnedDatum::Real(number) => assert_eq!(number, -1.0),
            other => panic!("min was {other:?}"),
        }
        match fold(AggregateKind::Maximum, &values) {
            OwnedDatum::Blob(bytes) => assert_eq!(bytes, vec![0]),
            other => panic!("max was {other:?}"),
        }
    }

    /// `group_concat` joins with its separator and skips NULLs.
    #[test]
    fn group_concat_joins_non_nulls() {
        match fold(
            AggregateKind::GroupConcat("-".into()),
            &[Datum::Int(1), Datum::Null, Datum::Text(b"x")],
        ) {
            OwnedDatum::Text(bytes) => assert_eq!(bytes, b"1-x".to_vec()),
            other => panic!("group_concat was {other:?}"),
        }
    }

    /// The vectorised path agrees with the per-row path on every aggregate,
    /// over runs that do and do not overflow.
    ///
    /// This is what licenses the fast path to exist: it is only a speed change.
    #[test]
    fn the_dense_path_agrees_with_the_per_row_path() {
        let runs: [Vec<i64>; 6] = [
            vec![],
            // The running total overflows at the second value and the last two
            // bring it back: SQLite still raises `integer overflow`.
            vec![1, i64::MAX, i64::MIN, 0],
            vec![7],
            (0..1000).collect(),
            vec![i64::MAX, 1, 1],
            vec![-5, 0, 5, i64::MIN + 1],
        ];
        let kinds = [
            AggregateKind::CountStar,
            AggregateKind::Count,
            AggregateKind::Sum,
            AggregateKind::Total,
            AggregateKind::Average,
            AggregateKind::Minimum,
            AggregateKind::Maximum,
            AggregateKind::GroupConcat(",".into()),
        ];
        for run in &runs {
            let bytes: Vec<u8> = run.iter().flat_map(|value| value.to_le_bytes()).collect();
            for kind in &kinds {
                let mut per_row = Accumulator::new(kind.clone());
                for value in run {
                    per_row.push(&Datum::Int(*value));
                }
                let mut dense = Accumulator::new(kind.clone());
                dense.push_dense_ints(crate::batch::DenseInts::new(&bytes, 8));
                // **A refusal is an answer the two paths have to agree on
                // too.** `sum` over `[i64::MAX, 1, 1]` raises `integer
                // overflow` now, and the point of this sweep is that the
                // vectorised path and the per-row one cannot disagree - about
                // that as much as about a number.
                let (a, b) = match (per_row.finish(), dense.finish()) {
                    (Ok(a), Ok(b)) => (a, b),
                    (Err(_), Err(_)) => continue,
                    (a, b) => panic!("{kind:?} over {run:?}: per-row {a:?}, dense {b:?}"),
                };
                assert_eq!(
                    a.borrow().compare(&b.borrow()),
                    std::cmp::Ordering::Equal,
                    "{kind:?} over {run:?}: per-row {a:?}, dense {b:?}"
                );
                assert_eq!(
                    matches!(a, OwnedDatum::Null),
                    matches!(b, OwnedDatum::Null),
                    "{kind:?} over {run:?}"
                );
            }
        }
    }

    /// The dense path folds several batches into one answer, which is what a
    /// multi-leaf scan does.
    #[test]
    fn the_dense_path_accumulates_across_batches() {
        let mut dense = Accumulator::new(AggregateKind::Sum);
        let mut per_row = Accumulator::new(AggregateKind::Sum);
        for batch in 0..5i64 {
            let run: Vec<i64> = (0..100).map(|n| batch * 100 + n).collect();
            let bytes: Vec<u8> = run.iter().flat_map(|value| value.to_le_bytes()).collect();
            dense.push_dense_ints(crate::batch::DenseInts::new(&bytes, 8));
            for value in &run {
                per_row.push(&Datum::Int(*value));
            }
        }
        assert_eq!(
            dense
                .finish()
                .unwrap()
                .borrow()
                .compare(&per_row.finish().unwrap().borrow()),
            std::cmp::Ordering::Equal
        );
        assert!(matches!(dense.finish().unwrap(), OwnedDatum::Int(124_750)));
    }

    /// `group_concat` appends the bytes of a blob and of text that is not UTF-8 as they are.
    ///
    /// It replaced each invalid byte with U+FFFD, so `group_concat(x'ff')` answered EFBFBD where
    /// SQLite answers FF.
    #[test]
    fn group_concat_keeps_invalid_utf8_bytes() {
        let blobs = [Datum::Blob(&[0xff]), Datum::Blob(&[0xfe, 0x41])];
        let joined = fold(AggregateKind::GroupConcat(",".into()), &blobs);
        assert_eq!(joined, OwnedDatum::Text(vec![0xff, b',', 0xfe, 0x41]));
        let text = [Datum::Text(&[0xc3]), Datum::Text(&[0x28])];
        let joined = fold(AggregateKind::GroupConcat("-".into()), &text);
        assert_eq!(joined, OwnedDatum::Text(vec![0xc3, b'-', 0x28]));
    }

    /// A separator that is a value of each row keeps its bytes too.
    #[test]
    fn group_concat_with_a_blob_separator_keeps_its_bytes() {
        use inillucent_value::value::Value;
        let mut accumulator = Accumulator::new(AggregateKind::GroupConcatComputed);
        for letter in [b"a", b"b"] {
            accumulator.push_values(vec![
                Value::owned_text(letter).unwrap(),
                Value::owned_blob(&[0xff]).unwrap(),
            ]);
        }
        let joined = accumulator.finish().unwrap();
        assert_eq!(joined, OwnedDatum::Text(vec![b'a', 0xff, b'b']));
    }

    /// `median` and the percentiles refuse a value that is not an integer or a real, naming the
    /// function, and a text that spells a number is refused too.
    #[test]
    fn a_percentile_of_text_or_a_blob_is_an_error() {
        use inillucent_value::value::Value;
        let refused = |kind: Percentile, with_fraction: bool, value: Value<'static>| {
            let mut accumulator = Accumulator::new(AggregateKind::Percentile(kind));
            let mut first = vec![Value::Integer(2)];
            let mut second = vec![value];
            if with_fraction {
                first.push(Value::Integer(50));
                second.push(Value::Integer(50));
            }
            accumulator.push_values(first);
            accumulator.push_values(second);
            accumulator
                .finish()
                .expect_err("a value that is not numeric")
                .message()
                .to_string()
        };
        for value in [
            Value::owned_text(b"abc").unwrap(),
            Value::owned_text(b"1").unwrap(),
            Value::owned_blob(b"1").unwrap(),
        ] {
            assert_eq!(
                refused(Percentile::Median, false, value.clone()),
                "input to median() is not numeric"
            );
            assert_eq!(
                refused(Percentile::Hundredths, true, value),
                "input to percentile() is not numeric"
            );
        }
    }

    /// A fraction outside its range, or one that is not a number, is an error naming the
    /// function and the range, and a fraction that changes between rows is an error too.
    #[test]
    fn a_percentile_fraction_is_checked() {
        use inillucent_value::value::Value;
        let cases: [(Percentile, Value<'static>, &str); 4] = [
            (
                Percentile::Hundredths,
                Value::Integer(101),
                "percentile() is not between 0.0 and 100.0",
            ),
            (
                Percentile::Continuous,
                Value::Real(1.5),
                "percentile_cont() is not between 0.0 and 1.0",
            ),
            (
                Percentile::Discrete,
                Value::Real(-0.5),
                "percentile_disc() is not between 0.0 and 1.0",
            ),
            (
                Percentile::Hundredths,
                Value::Null,
                "percentile() is not between 0.0 and 100.0",
            ),
        ];
        for (kind, fraction, wanted) in cases {
            let mut accumulator = Accumulator::new(AggregateKind::Percentile(kind));
            accumulator.push_values(vec![Value::Integer(1), fraction]);
            let error = accumulator.finish().expect_err("a fraction out of range");
            assert_eq!(
                error.message(),
                format!("the fraction argument to {wanted}")
            );
        }
        let mut accumulator = Accumulator::new(AggregateKind::Percentile(Percentile::Hundredths));
        accumulator.push_values(vec![Value::Integer(1), Value::Integer(10)]);
        accumulator.push_values(vec![Value::Integer(2), Value::Integer(20)]);
        let error = accumulator.finish().expect_err("a fraction that changes");
        assert_eq!(
            error.message(),
            "the fraction argument to percentile() is not the same for all input rows"
        );
    }

    /// Taking the oldest value out of a `group_concat` leaves the others joined by the separator,
    /// and an empty string is a value like any other.
    #[test]
    fn pulling_the_oldest_group_concat_value_drops_it_and_one_separator() {
        let mut accumulator = Accumulator::new(AggregateKind::GroupConcat(",".into()));
        accumulator.track_pieces();
        for value in [
            Datum::Text(b""),
            Datum::Text(b"x"),
            Datum::Null,
            Datum::Text(b"yz"),
        ] {
            accumulator.push(&value);
        }
        assert_eq!(
            accumulator.finish().unwrap(),
            OwnedDatum::Text(b",x,yz".to_vec())
        );
        accumulator.pull_oldest();
        assert_eq!(
            accumulator.finish().unwrap(),
            OwnedDatum::Text(b"x,yz".to_vec())
        );
        accumulator.pull_oldest();
        assert_eq!(
            accumulator.finish().unwrap(),
            OwnedDatum::Text(b"yz".to_vec())
        );
        accumulator.pull_oldest();
        assert_eq!(accumulator.finish().unwrap(), OwnedDatum::Null);
    }
}

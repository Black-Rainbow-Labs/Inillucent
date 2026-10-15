//! Ordering two rows, and searching a page with that order.
//!
//! Invariant: **one comparison function, used by the search and by the
//! builder.** A page whose rows were ordered by one rule and searched by
//! another answers *no row* rather than an error, which is the failure this
//! file is arranged to make impossible.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;

use crate::datum::Datum;
use crate::types::PhysicalType;

use super::layout::*;
use super::read::*;
use super::*;

impl<'p> LeafRef<'p> {
    /// Returns a reusable view of the leaf's key columns.
    ///
    /// A binary search over a leaf makes about eleven comparisons, and each one
    /// re-derived the key columns' directory entries and slice bounds from the
    /// page. That is a dozen bounds-checked reads per comparison to rediscover
    /// something that does not change, and on a skip scan - which searches a
    /// leaf per seek per distinct value - it was the whole cost of the query.
    /// Deriving it once per search and passing it down makes the comparison
    /// two slice reads.
    pub fn key_view(&self) -> DbResult<KeyView<'p>> {
        let mut columns: [Option<MiniColumn<'p>>; KEY_VIEW_INLINE] = Default::default();
        let width = self.key_columns.min(KEY_VIEW_INLINE);
        for index in 0..width {
            if let Some(slot) = columns.get_mut(index) {
                *slot = Some(self.column(index)?);
            }
        }
        Ok(KeyView {
            columns,
            width: self.key_columns,
        })
    }
    /// Compares one sorted-region row's key against a probe key.
    ///
    /// @param row - the row's position in the sorted region
    /// @param probe - the key to compare against, one value per key column
    pub fn compare_key(&self, row: usize, probe: &[Datum<'_>]) -> DbResult<std::cmp::Ordering> {
        self.compare_key_with(&self.key_view()?, row, probe)
    }
    /// Compares one row's key against a probe, through a prepared key view.
    ///
    /// @param view - the leaf's key columns, from [`LeafRef::key_view`]
    /// @param row - the row's position in the sorted region
    /// @param probe - the key to compare against, one value per key column
    pub fn compare_key_with(
        &self,
        view: &KeyView<'p>,
        row: usize,
        probe: &[Datum<'_>],
    ) -> DbResult<std::cmp::Ordering> {
        for (index, wanted) in probe.iter().enumerate().take(view.width) {
            // Past the inline capacity the column is read from the directory:
            // correct for every key width and only slower.
            let derived;
            let column = match view.columns.get(index).and_then(|held| held.as_ref()) {
                Some(column) => column,
                None => {
                    derived = self.column(index)?;
                    &derived
                }
            };
            if let Some(spilled) = column.spilled_key(row)? {
                let whole = self.extents.and_then(|held| held.get(row, index));
                let order = self.compare_spilled(&spilled, wanted, index, whole)?;
                if order != std::cmp::Ordering::Equal {
                    return Ok(order);
                }
                continue;
            }
            let held = column.value(row)?;
            let order = self.directed(
                crate::types::compare_under(&held, wanted, self.collation_of(index)),
                index,
            );
            if order != std::cmp::Ordering::Equal {
                return Ok(order);
            }
        }
        Ok(std::cmp::Ordering::Equal)
    }
    /// Compares a key value stored out of line against a probe value.
    ///
    /// **Three steps, and the last is the only one that can need the pool.**
    /// The class decides when the probe is NULL, a number, or the other of text
    /// and blob. The prefix the leaf kept decides when a byte differs inside it,
    /// under a rule the collation allows. Otherwise the whole value is read:
    /// from the extents a caller attached, else through the pool, else the
    /// comparison refuses. It never guesses an order.
    ///
    /// The direction of the column is applied to the answer, as it is for a
    /// value held in the leaf.
    ///
    /// @param spilled - the reference and prefix the leaf holds
    /// @param wanted - the probe's value for this key column
    /// @param index - which key column this is
    /// @param whole - the whole value, when the caller already read it
    pub(super) fn compare_spilled(
        &self,
        spilled: &SpilledKey<'_>,
        wanted: &Datum<'_>,
        index: usize,
        whole: Option<&[u8]>,
    ) -> DbResult<std::cmp::Ordering> {
        let held_rank = extent_datum(spilled.reference.class, spilled.physical, &[]).sort_rank();
        if held_rank != wanted.sort_rank() {
            return Ok(self.directed(held_rank.cmp(&wanted.sort_rank()), index));
        }
        let collation = self.collation_of(index);
        let probe = wanted.as_bytes().unwrap_or(&[]);
        if let Some(order) = decide_by_prefix(spilled.prefix, probe, held_rank == 2, collation) {
            return Ok(self.directed(order, index));
        }
        let fetched;
        let bytes = match (whole, self.pool) {
            (Some(bytes), _) => bytes,
            (None, Some(pool)) => {
                fetched = crate::paged::read_extent(pool.0, spilled.reference)?;
                fetched.as_slice()
            }
            (None, None) => {
                return Err(misuse(concat!(
                    "this key is stored out of line; attach the tree's pool to the ",
                    "leaf before searching it"
                )))
            }
        };
        let held = extent_datum(spilled.reference.class, spilled.physical, bytes);
        Ok(self.directed(crate::types::compare_under(&held, wanted, collation), index))
    }
    /// Returns the run of sorted rows whose key begins with a prefix.
    ///
    /// The half-open range `begin..end`, empty when the prefix is not present.
    ///
    /// An index nested loop asks this once per outer row, so the shape matters:
    /// the lower bound is a search, and the *upper* bound is a short forward
    /// walk rather than a second search, because the run a join probe finds is
    /// usually one entry long. Past `scan_cap` matching rows it stops walking
    /// and bisects, so a prefix that matches a whole leaf still costs a search.
    ///
    /// The integer fast path reads the leading key column's raw values, which
    /// is what [`LeafRef::lower_bound`] already does for the search; doing it
    /// for the walk as well is what removes the key view and the two
    /// `Datum` comparisons that a probe finding one entry was paying.
    ///
    /// @param prefix - the prefix to match, one value per compared column
    /// @param scan_cap - how far the run is walked before the end is bisected
    pub fn equal_run(&self, prefix: &[Datum<'_>], scan_cap: usize) -> DbResult<(usize, usize)> {
        // The guide is built once and used for both ends. Asking for it again
        // is not free: `all_typed` walks the column's class array, which on a
        // leaf holding 1,667 index entries is 417 bytes, and a probe that
        // called `lower_bound` and then re-derived the guide walked it twice.
        if let [Datum::Int(target)] = prefix {
            if let Some(guide) = self.integer_guide(prefix)? {
                let begin = self.partition_integer(&guide, *target, false)?;
                if begin >= self.row_count || guide.read(begin)? != *target {
                    return Ok((begin, begin));
                }
                let mut end = begin.saturating_add(1);
                while end < self.row_count
                    && end.saturating_sub(begin) < scan_cap
                    && guide.read(end)? == *target
                {
                    end = end.saturating_add(1);
                }
                if end.saturating_sub(begin) >= scan_cap {
                    end = self
                        .partition_integer(&guide, *target, true)?
                        .min(self.row_count);
                }
                return Ok((begin, end));
            }
        }
        let begin = self.lower_bound(prefix)?;
        if begin >= self.row_count {
            return Ok((begin, begin));
        }
        let view = self.key_view()?;
        if self.compare_key_with(&view, begin, prefix)? != std::cmp::Ordering::Equal {
            return Ok((begin, begin));
        }
        let mut end = begin.saturating_add(1);
        while end < self.row_count
            && end.saturating_sub(begin) < scan_cap
            && self.compare_key_with(&view, end, prefix)? == std::cmp::Ordering::Equal
        {
            end = end.saturating_add(1);
        }
        if end.saturating_sub(begin) >= scan_cap {
            end = self.upper_bound(prefix)?.min(self.row_count);
        }
        Ok((begin, end))
    }
    /// Finds the position of a key in the sorted region.
    ///
    /// Returns `Ok(row)` when the key is present and `Err(insertion point)` when
    /// it is not, which is `slice::binary_search`'s contract and the shape both
    /// the point probe and the insert path want.
    ///
    /// @param probe - the key to look for, one value per key column
    pub fn search(&self, probe: &[Datum<'_>]) -> DbResult<Result<usize, usize>> {
        if self.key_columns == 1 {
            // **The column comes back with the target.** Both halves need the
            // key mini-column - one to decide the probe is an integer one, the
            // other to interpolate over its values - and parsing the directory
            // entry twice to hand the same four fields back twice is a
            // measurable part of a 40 ns leaf search on a probe that does two
            // or three value reads in total.
            if let Some((target, column)) = self.integer_key_probe(probe)? {
                return self.search_integer_key(target, &column);
            }
        }
        let view = self.key_view()?;
        self.search_between(&view, probe, 0, self.row_count)
    }
    /// Binary-searches a window of the sorted region.
    ///
    /// @param view - the leaf's key columns
    /// @param probe - the key to look for
    /// @param from - the first row of the window
    /// @param to - one past the last row of the window
    pub(super) fn search_between(
        &self,
        view: &KeyView<'p>,
        probe: &[Datum<'_>],
        from: usize,
        to: usize,
    ) -> DbResult<Result<usize, usize>> {
        let mut low = from;
        let mut high = to;
        while low < high {
            let middle = low.saturating_add(high.saturating_sub(low) / 2);
            match self.compare_key_with(view, middle, probe)? {
                std::cmp::Ordering::Less => low = middle.saturating_add(1),
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Ok(Ok(middle)),
            }
        }
        Ok(Err(low))
    }
    /// Returns the integer a probe is looking for, when this leaf is one a
    /// rowid tree would have.
    ///
    /// The conditions are narrow on purpose: one key column, physically
    /// `Int64`, every row typed, and an integer probe. Anything else - a
    /// compound key, a NULL, an exception row, a text probe against an integer
    /// column - falls through to the general comparison, which is correct for
    /// all of them.
    ///
    /// @param probe - the key being looked for
    fn integer_key_probe(&self, probe: &[Datum<'_>]) -> DbResult<Option<(i64, MiniColumn<'p>)>> {
        if probe.len() != 1 || self.row_count < 8 {
            return Ok(None);
        }
        let Some(Datum::Int(target)) = probe.first() else {
            return Ok(None);
        };
        let column = self.column(0)?;
        if column.physical != PhysicalType::Int64 || !column.all_typed() {
            return Ok(None);
        }
        Ok(Some((*target, column)))
    }
    /// Finds an integer key by interpolation, falling back to a binary search.
    ///
    /// **This is a measurement, not a preference.** A 32 KiB leaf holds a few
    /// hundred rows, and a binary search over them touches a scattered cache
    /// line per step: measured at **120 ns** against a 100 ns descent and a
    /// 423 ns point probe, so the search was the largest single cost in three
    /// of the four read families - `read.point`, `read.range` through a rowid
    /// lookup, and `read.analytical` through the skip scan's per-seek search.
    ///
    /// Interpolation converges in one or two steps on keys that are anywhere
    /// near uniform, which a rowid is by construction: `INTEGER PRIMARY KEY`
    /// values are handed out in order. It is *not* a promise about arbitrary
    /// data, so the step count is capped and what is left of the window is
    /// binary searched. The worst case is therefore a binary search plus four
    /// probes, and the ordinary case is two.
    ///
    /// @param target - the integer being looked for
    fn search_integer_key(
        &self,
        target: i64,
        column: &MiniColumn<'p>,
    ) -> DbResult<Result<usize, usize>> {
        /// How many interpolation steps before giving up and bisecting.
        ///
        /// Four, because a distribution that has not converged in four steps is
        /// not one interpolation is going to help with, and because bounding it
        /// is what makes the worst case no worse than the search it replaces.
        const STEPS: usize = 4;

        let values = column.inline_bytes();
        let width = column.width;
        let base = column.base;
        let read = |row: usize| -> DbResult<i64> {
            let at = row.saturating_mul(width);
            let slice = values
                .get(at..at.saturating_add(width))
                .ok_or_else(|| corrupt(format!("row {row} is past the key column")))?;
            Ok(from_frame(base, width, slice))
        };

        let mut low = 0usize;
        let mut high = self.row_count.saturating_sub(1);
        let mut low_value = read(low)?;
        let mut high_value = read(high)?;
        if target < low_value {
            return Ok(Err(0));
        }
        if target > high_value {
            return Ok(Err(self.row_count));
        }
        for _ in 0..STEPS {
            if low > high {
                break;
            }
            if low_value == high_value {
                return Ok(if low_value == target {
                    Ok(low)
                } else {
                    Err(low)
                });
            }
            // The guess, in 64 bits unless the product needs more, which is
            // what the bound search does too (task-2183): the 128 bit form
            // called the compiler's division routine on every guess. Here
            // `low_value <= target <= high_value` and the two values differ,
            // so both differences are non-negative and fit a `u64`.
            let span = high_value.wrapping_sub(low_value) as u64;
            let into = target.wrapping_sub(low_value) as u64;
            let width = high.saturating_sub(low) as u64;
            let offset = match into.checked_mul(width) {
                Some(product) => product / span.max(1),
                None => (u128::from(into) * u128::from(width) / u128::from(span.max(1))) as u64,
            };
            let guess = low.saturating_add(offset.min(width) as usize);
            let seen = read(guess)?;
            match seen.cmp(&target) {
                std::cmp::Ordering::Equal => return Ok(Ok(guess)),
                std::cmp::Ordering::Less => {
                    low = guess.saturating_add(1);
                    if low > high {
                        return Ok(Err(low));
                    }
                    low_value = read(low)?;
                    if target < low_value {
                        return Ok(Err(low));
                    }
                }
                std::cmp::Ordering::Greater => {
                    if guess == 0 {
                        return Ok(Err(0));
                    }
                    high = guess.saturating_sub(1);
                    if low > high {
                        return Ok(Err(low));
                    }
                    high_value = read(high)?;
                    if target > high_value {
                        return Ok(Err(high.saturating_add(1)));
                    }
                }
            }
        }
        // Whatever window is left, bisected. The general comparison is used so
        // that this path and the one above cannot disagree about ordering.
        let view = self.key_view()?;
        self.search_between(&view, &[Datum::Int(target)], low, high.saturating_add(1))
    }
    /// Returns the first row whose key is at or above a probe.
    ///
    /// The bound an index range and an index nested loop each need once per
    /// leaf. It is an ordinary bound search with an **interpolated midpoint**
    /// for its first few steps: when the leading key column is an all-typed
    /// `Int64` - which every index on an integer column is - a guess placed by
    /// proportion lands far closer than the middle, and the loop's invariant is
    /// unchanged by where the midpoint came from.
    ///
    /// That last sentence is the whole correctness argument, and it is why the
    /// interpolation is here rather than in a separate narrowing pass: a pass
    /// that returned a *window* would have to be right about the window, and a
    /// run longer than it would put the answer outside. A midpoint cannot be
    /// wrong; it can only be a poor guess, and after a few of those the loop
    /// falls back to bisection.
    ///
    /// @param probe - the bound, one value per compared column
    pub fn lower_bound(&self, probe: &[Datum<'_>]) -> DbResult<usize> {
        self.bounded(probe, false)
    }
    /// Returns the first row whose key is above a probe.
    ///
    /// @param probe - the bound, one value per compared column
    pub fn upper_bound(&self, probe: &[Datum<'_>]) -> DbResult<usize> {
        self.bounded(probe, true)
    }
    /// The shared bound search.
    ///
    /// @param probe - the bound
    /// @param past_equal - whether a row equal to the probe is below the bound
    fn bounded(&self, probe: &[Datum<'_>], past_equal: bool) -> DbResult<usize> {
        /// How many interpolated midpoints before falling back to bisection.
        const GUESSES: u32 = 4;

        // The integer fast path, and it is a measurement rather than a
        // preference. A bound over one integer column is what a skip scan seeks
        // with and what an index nested loop probes with, and the generic
        // search reads a `Datum` out of the mini-column and compares it under a
        // collation at every step. `inillucent-probeprofile` measured a prefix
        // probe into `side_owner` - two integer key columns, 25,000 entries -
        // at 209 ns for the descent plus this search, of which the descent was
        // 62 ns. `join.range` pays it 201 times per execution.
        //
        // When the probe is one integer and the leading key column is a fully
        // typed `Int64` mini-column, the same partition point is found over a
        // contiguous run of eight-byte values, with the interpolation guide
        // still choosing the midpoints. Comparing only column zero is exactly
        // what the generic path does for a one-column probe, so this is the
        // same answer by a shorter route rather than a different one.
        if let [Datum::Int(target)] = probe {
            if let Some(guide) = self.integer_guide(probe)? {
                return self.partition_integer(&guide, *target, past_equal);
            }
        }

        let view = self.key_view()?;
        let guide = self.integer_guide(probe)?;
        let mut low = 0usize;
        let mut high = self.row_count;
        let mut guesses = 0u32;
        while low < high {
            let middle = match &guide {
                Some(guide) if guesses < GUESSES => {
                    guesses = guesses.saturating_add(1);
                    guide.between(low, high)?
                }
                _ => low.saturating_add(high.saturating_sub(low) / 2),
            };
            let order = self.compare_key_with(&view, middle, probe)?;
            let below = match order {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Equal => past_equal,
                std::cmp::Ordering::Greater => false,
            };
            if below {
                low = middle.saturating_add(1);
            } else {
                high = middle;
            }
        }
        Ok(low)
    }
    /// Returns the partition point of an integer bound over column zero.
    ///
    /// @param guide - the interpolation guide over the leading key column
    /// @param target - the integer being bounded
    /// @param past_equal - whether a row equal to the target is below the bound
    fn partition_integer(
        &self,
        guide: &IntegerGuide<'p>,
        target: i64,
        past_equal: bool,
    ) -> DbResult<usize> {
        /// How many interpolated midpoints before falling back to bisection.
        const GUESSES: u32 = 4;

        let mut low = 0usize;
        let mut high = self.row_count;
        let mut guesses = 0u32;
        while low < high {
            let middle = if guesses < GUESSES {
                guesses = guesses.saturating_add(1);
                guide.between(low, high)?
            } else {
                low.saturating_add(high.saturating_sub(low) / 2)
            };
            let held = guide.read(middle)?;
            let below = if past_equal {
                held <= target
            } else {
                held < target
            };
            if below {
                low = middle.saturating_add(1);
            } else {
                high = middle;
            }
        }
        Ok(low)
    }
    /// Returns what is needed to interpolate on the leading key column.
    ///
    /// `None` whenever interpolation does not apply, which leaves the bound
    /// search an ordinary bisection.
    ///
    /// @param probe - the bound
    fn integer_guide(&self, probe: &[Datum<'_>]) -> DbResult<Option<IntegerGuide<'p>>> {
        if probe.is_empty() || self.row_count < 8 {
            return Ok(None);
        }
        let Some(Datum::Int(target)) = probe.first() else {
            return Ok(None);
        };
        let column = self.column(0)?;
        if column.physical != PhysicalType::Int64 || !column.all_typed() {
            return Ok(None);
        }
        // A collation is an order over text and never changes where an integer
        // sits, so interpolation on an integer column is valid under any of
        // them. The guard is here so that a collation that *did* reorder
        // numbers would turn it off rather than silently mis-guess.
        if self.collation_of(0) != inillucent_value::collation::Collation::Binary {
            return Ok(None);
        }
        // **And off entirely for a descending column.** Interpolation assumes
        // the values rise across the leaf; in a descending tree they fall, and
        // a guess made on the wrong slope is a guess that lands past the row it
        // was looking for.
        if self.descending_at(0) {
            return Ok(None);
        }
        Ok(Some(IntegerGuide {
            values: column.inline_bytes(),
            width: column.width,
            base: column.base,
            target: *target,
        }))
    }
    /// Compares two materialised rows on their key columns, under the leaf's
    /// collations.
    ///
    /// @param left - one row
    /// @param right - the other row
    pub(crate) fn compare_keys(
        &self,
        left: &[Datum<'_>],
        right: &[Datum<'_>],
    ) -> std::cmp::Ordering {
        for index in 0..self.key_columns {
            let (Some(a), Some(b)) = (left.get(index), right.get(index)) else {
                return std::cmp::Ordering::Equal;
            };
            let order = self.directed(
                crate::types::compare_under(a, b, self.collation_of(index)),
                index,
            );
            if order != std::cmp::Ordering::Equal {
                return order;
            }
        }
        std::cmp::Ordering::Equal
    }
    /// Compares a row against a probe that may be shorter than the key.
    ///
    /// A probe of two columns against a three-column key matches a *run*, so
    /// only the columns the probe names are compared - which is the same rule
    /// `lower_bound` and `upper_bound` follow over the sorted region, and the
    /// reason a prefix bound returns three rows rather than one.
    ///
    /// @param row - the row
    /// @param probe - the bound, one value per column it names
    pub(crate) fn compare_prefix(
        &self,
        row: &[Datum<'_>],
        probe: &[Datum<'_>],
    ) -> std::cmp::Ordering {
        for (index, wanted) in probe.iter().enumerate() {
            let Some(held) = row.get(index) else {
                return std::cmp::Ordering::Less;
            };
            let order = self.directed(
                crate::types::compare_under(held, wanted, self.collation_of(index)),
                index,
            );
            if order != std::cmp::Ordering::Equal {
                return order;
            }
        }
        std::cmp::Ordering::Equal
    }
}

/// Decides the order of a long text or blob from its leading bytes alone, when
/// those bytes are enough.
///
/// The held value is longer than `prefix`. BINARY and every blob compare bytes
/// from the front and stop at the first difference, so a difference inside the
/// bytes both sides have decides the order. NOCASE folds each byte and stops
/// the same way, except that two NUL bytes at one position end the walk and
/// leave the lengths to decide, which the prefix cannot do. RTRIM, DECIMAL,
/// UINT and a custom collation are never decided here. A probe that matches the
/// prefix for its whole length is left to the full comparison too, so the rule
/// stays the same for every case.
///
/// @param prefix - the leading bytes of the held value
/// @param probe - the probe's bytes
/// @param is_text - whether both sides are text rather than blobs
/// @param collation - the key column's collation
fn decide_by_prefix(
    prefix: &[u8],
    probe: &[u8],
    is_text: bool,
    collation: inillucent_value::collation::Collation,
) -> Option<std::cmp::Ordering> {
    use inillucent_value::collation::Collation;
    let shared = prefix.len().min(probe.len());
    let (held, wanted) = (prefix.get(..shared)?, probe.get(..shared)?);
    let collation = if is_text {
        collation
    } else {
        Collation::Binary
    };
    match collation {
        Collation::Binary => Some(held.cmp(wanted)).filter(|order| order.is_ne()),
        Collation::NoCase => decide_nocase(held, wanted),
        _ => None,
    }
}
/// The NOCASE rule of [`decide_by_prefix`] over two slices of equal length.
///
/// @param held - the held value's leading bytes
/// @param wanted - the probe's leading bytes, as many
fn decide_nocase(held: &[u8], wanted: &[u8]) -> Option<std::cmp::Ordering> {
    for (left, right) in held.iter().zip(wanted.iter()) {
        if *left == 0 && *right == 0 {
            return None;
        }
        let order = left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase());
        if order.is_ne() {
            return Some(order);
        }
    }
    None
}

/// A leaf's key columns, derived once and reused across a binary search.
#[derive(Clone, Copy, Debug)]
pub struct KeyView<'p> {
    columns: [Option<MiniColumn<'p>>; KEY_VIEW_INLINE],
    width: usize,
}
/// A validated read view over one leaf page.
///
/// Holds no allocation: everything is derived from the page bytes on demand,
/// because a scan visits hundreds of leaves and asks each for two or three of
/// its columns.
/// Where to place a bound search's next probe, by proportion.
///
/// It reads the leading key column's raw bytes, which for an all-typed `Int64`
/// mini-column is a contiguous run of eight-byte little-endian values.
pub(crate) struct IntegerGuide<'p> {
    /// The leading key column's value array.
    values: &'p [u8],
    /// How many bytes one of its slots occupies.
    width: usize,
    /// What those slots are measured from.
    base: i64,
    /// The value being looked for.
    target: i64,
}
impl IntegerGuide<'_> {
    /// Returns a row in `low..high` to probe next.
    ///
    /// Always inside the window, so the loop that calls it terminates whatever
    /// the data looks like: a degenerate guess is a slow search, never a wrong
    /// one or a hanging one.
    ///
    /// @param low - the first row still in the window
    /// @param high - one past the last row still in the window
    fn between(&self, low: usize, high: usize) -> DbResult<usize> {
        let last = high.saturating_sub(1);
        let low_value = self.read(low)?;
        let high_value = self.read(last)?;
        if high_value <= low_value {
            return Ok(low.saturating_add(high.saturating_sub(low) / 2));
        }
        if self.target <= low_value {
            return Ok(low);
        }
        if self.target >= high_value {
            return Ok(last);
        }
        // **In 64 bits unless the product needs more (task-2183).** Here
        // `low_value < target < high_value`, so both differences are positive
        // and fit a `u64`, and `into < span`. The `i128` form called the
        // compiler's 128 bit division routine on every guess, which was 4% of
        // an index probe per outer row.
        let span = high_value.wrapping_sub(low_value) as u64;
        let into = self.target.wrapping_sub(low_value) as u64;
        let width = last.saturating_sub(low) as u64;
        let offset = match into.checked_mul(width) {
            Some(product) => product / span.max(1),
            None => (u128::from(into) * u128::from(width) / u128::from(span.max(1))) as u64,
        };
        Ok(low.saturating_add(offset.min(width) as usize))
    }

    /// Reads one row's value from the leading key column.
    ///
    /// @param row - the row to read
    fn read(&self, row: usize) -> DbResult<i64> {
        let at = row.saturating_mul(self.width);
        let slice = self
            .values
            .get(at..at.saturating_add(self.width))
            .ok_or_else(|| corrupt(format!("row {row} is past the key column")))?;
        Ok(from_frame(self.base, self.width, slice))
    }
}
/// Where a row a probe found actually lives.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Hit {
    /// In the sorted region, at this row.
    Sorted(usize),
    /// In the delta area, at this index.
    Delta(usize),
}
/// Compares two materialised rows on their leading key columns.
///
/// @param left - one row
/// @param right - the other row
/// @param key_columns - how many leading columns form the key
pub fn compare_rows(
    left: &[Datum<'_>],
    right: &[Datum<'_>],
    key_columns: usize,
) -> std::cmp::Ordering {
    compare_rows_under(left, right, key_columns, &[], &[])
}
/// Compares two rows' keys the way the tree they came from is ordered.
///
/// **The same comparison the tree's own searches make, which is the point.** A
/// caller that checks a tree's ordering with `BINARY` while the tree is ordered
/// by `NOCASE` is not checking the tree - it is checking a different tree. The
/// integrity check did exactly that, and `REINDEX` over
/// `CREATE INDEX ix ON t(team COLLATE NOCASE)` holding `'Blue'` and `'BLUE'`
/// reported `a key does not increase across the leaf chain` about a tree whose
/// every read was correct.
///
/// Short slices default the rest: no collation is `BINARY` and no direction is
/// ascending, which is what every rowid tree is.
///
/// @param left - one row
/// @param right - the other
/// @param key_columns - how many leading columns form the key
/// @param collations - the collation of each key column
/// @param directions - whether each key column is stored descending
pub fn compare_rows_under(
    left: &[Datum<'_>],
    right: &[Datum<'_>],
    key_columns: usize,
    collations: &[inillucent_value::collation::Collation],
    directions: &[bool],
) -> std::cmp::Ordering {
    for index in 0..key_columns {
        let (Some(a), Some(b)) = (left.get(index), right.get(index)) else {
            return std::cmp::Ordering::Equal;
        };
        let collation = collations
            .get(index)
            .copied()
            .unwrap_or(inillucent_value::collation::Collation::Binary);
        let order = crate::types::compare_under(a, b, collation);
        let order = if directions.get(index).copied().unwrap_or(false) {
            order.reverse()
        } else {
            order
        };
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ColumnSpec;
    use inillucent_pool::extent::ExtentRef;
    use inillucent_value::collation::Collation;

    /// A spiller that hands out one reference and writes nothing.
    struct Nowhere;

    impl Spill for Nowhere {
        fn spill(&mut self, _row: usize, _column: usize, value: &[u8]) -> DbResult<ExtentRef> {
            Ok(ExtentRef::run(
                inillucent_pool::PageId(9),
                value.len() as u64,
            ))
        }
    }

    /// Builds a one row leaf at a 4,096 byte page, whose one key is stored out of
    /// line because it is over 512 bytes.
    ///
    /// @param key - the key's bytes
    fn leaf_page(key: &[u8]) -> Vec<u8> {
        let builder = LeafBuilder::new(4_096, 1, vec![ColumnSpec::key(PhysicalType::Text)], 1)
            .expect("a builder");
        let rows = vec![vec![Datum::Text(key)]];
        match builder
            .pack_with(&rows, 1.0, Some(&mut Nowhere))
            .expect("a pack")
        {
            Packed::Filled { page, .. } => page,
            Packed::RowTooLarge => panic!("the key was refused"),
        }
    }

    /// Returns a long key that starts with `head`, `length` bytes long in all.
    ///
    /// @param head - the leading bytes
    /// @param length - the key's whole length
    fn long_key(head: &[u8], length: usize) -> Vec<u8> {
        let mut key = head.to_vec();
        key.resize(length, b'x');
        key
    }

    /// Compares a stored long key against a probe, with or without the whole
    /// value attached, and returns both answers.
    ///
    /// @param key - the key stored out of line
    /// @param probe - the probe's bytes
    /// @param collation - the key column's collation
    fn answers(
        key: &[u8],
        probe: &[u8],
        collation: Collation,
    ) -> (DbResult<std::cmp::Ordering>, DbResult<std::cmp::Ordering>) {
        let page = leaf_page(key);
        let collations = [collation];
        let bare = LeafRef::parse(&page)
            .expect("a leaf")
            .with_collations(&collations);
        let mut held = Extents::default();
        held.push(0, 0, key.to_vec());
        let with_value = LeafRef::parse(&page)
            .expect("a leaf")
            .with_collations(&collations)
            .with_extents(&held);
        let wanted = [Datum::Text(probe)];
        (
            bare.compare_key(0, &wanted),
            with_value.compare_key(0, &wanted),
        )
    }

    /// A key stored out of line compares right under every collation, and reads
    /// the rest of the value only when the prefix does not decide.
    ///
    /// **An error rather than a guess when the value is needed and not
    /// attached.** The first case in each group differs inside the prefix, so a
    /// leaf with nothing attached still answers; the others tie over the prefix,
    /// or the collation never lets a prefix decide, and then the leaf with
    /// nothing attached names the pool in its refusal.
    #[test]
    fn a_long_key_is_decided_by_its_prefix_when_the_collation_allows_it() {
        let held = long_key(b"hello", 5_000);
        let upper = long_key(b"HELLO", 5_000);
        let nul_held = long_key(b"ab\0cd", 5_000);
        let nul_probe = long_key(b"AB\0zz", 4_000);
        let nul_apart = long_key(b"ac", 4_000);
        for collation in [Collation::Binary, Collation::NoCase, Collation::RTrim] {
            let decided_early = collation != Collation::RTrim;
            let help = long_key(b"help", 4_000);
            let same = long_key(b"hello", 4_000);
            let cases: [(&[u8], &[u8], bool); 5] = [
                (&held, &help, decided_early),
                (&held, &same, false),
                (&held, &upper, collation == Collation::Binary),
                // BINARY sees `a` against `A`; NOCASE reaches the NUL both hold.
                (&nul_held, &nul_probe, collation == Collation::Binary),
                (&nul_held, &nul_apart, decided_early),
            ];
            for (key, probe, bare_answers) in cases {
                let (bare, with_value) = answers(key, probe, collation);
                let wanted =
                    crate::types::compare_under(&Datum::Text(key), &Datum::Text(probe), collation);
                assert_eq!(with_value.unwrap(), wanted, "{collation:?}");
                match (bare_answers, bare) {
                    (true, answer) => assert_eq!(answer.unwrap(), wanted, "{collation:?}"),
                    (false, answer) => {
                        let refusal = answer.expect_err("a tie needs the whole value");
                        assert!(
                            refusal
                                .detail()
                                .is_some_and(|said| said.contains("attach the tree's pool")),
                            "{collation:?}: {refusal:?}"
                        );
                    }
                }
            }
        }
    }

    /// Searching a leaf of integer keys never reads a key column's class array, and
    /// searching one with a key stored out of line does.
    ///
    /// **The count of those reads is the work the check on the first line of a
    /// comparison is there to avoid.** An indexed insert makes about fifteen
    /// comparisons, and reading the class array for each was part of what made
    /// `write.insert.batch` slower once keys could be stored out of line. The second leaf
    /// proves the counter moves, so the first count of zero is not a counter that never counts.
    #[test]
    fn ordinary_keys_are_compared_without_reading_the_stored_key_class() {
        let builder = LeafBuilder::new(4_096, 1, vec![ColumnSpec::key(PhysicalType::Int64)], 1)
            .expect("a builder");
        let rows: Vec<Vec<Datum<'_>>> = (0..200).map(|number| vec![Datum::Int(number)]).collect();
        let page = match builder
            .pack_with(&rows, 1.0, Some(&mut Nowhere))
            .expect("a pack")
        {
            Packed::Filled { page, .. } => page,
            Packed::RowTooLarge => panic!("the rows were refused"),
        };
        let leaf = LeafRef::parse(&page).expect("a leaf");
        STORED_KEY_READS.with(|count| count.set(0));
        for row in 0..200 {
            let order = leaf
                .compare_key(row, &[Datum::Int(77)])
                .expect("a comparison");
            assert_eq!(order, (row as i64).cmp(&77));
        }
        assert_eq!(STORED_KEY_READS.with(std::cell::Cell::get), 0);

        let long = long_key(b"hello", 5_000);
        let spilled = leaf_page(&long);
        let leaf = LeafRef::parse(&spilled).expect("a leaf");
        STORED_KEY_READS.with(|count| count.set(0));
        let _ = leaf.compare_key(0, &[Datum::Null]);
        assert_eq!(STORED_KEY_READS.with(std::cell::Cell::get), 1);
    }

    /// A probe of another class and a descending column are decided without
    /// reading the value.
    #[test]
    fn a_long_key_orders_by_class_and_direction_before_its_bytes() {
        let key = long_key(b"abc", 3_000);
        let page = leaf_page(&key);
        let leaf = LeafRef::parse(&page).expect("a leaf");
        // A text key sorts above NULL and numbers and below a blob, with the
        // pool absent: the class decides without reading the value.
        assert!(leaf.compare_key(0, &[Datum::Null]).unwrap().is_gt());
        assert!(leaf.compare_key(0, &[Datum::Int(5)]).unwrap().is_gt());
        assert!(leaf.compare_key(0, &[Datum::Blob(b"abc")]).unwrap().is_lt());
        let down = [true];
        let reversed = LeafRef::parse(&page)
            .expect("a leaf")
            .with_directions(&down);
        assert!(reversed.compare_key(0, &[Datum::Null]).unwrap().is_lt());
        assert!(reversed
            .compare_key(0, &[Datum::Text(b"abd")])
            .unwrap()
            .is_gt());
    }
}

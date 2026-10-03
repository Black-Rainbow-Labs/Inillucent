//! `IN` and `NOT IN` over a long list of constants, answered by a binary search.
//!
//! Invariant: **the answer is the one `scalar::InList` gives for the same
//! operand, list, affinity and collation.** `InList` compares the operand with
//! each entry in turn through `eval::comparison`, which applies the affinity to
//! both sides and then calls `compare::compare_values`. This applies the same
//! affinity to each entry once, sorts the entries with the same
//! `compare_values` and the same collation, and searches. `compare_values` is a
//! total order (numbers by value, an integer against a real exactly, then
//! text under the collation, then blobs), so an entry the search finds equal is
//! one the linear walk finds equal, and the NULL rules are copied from
//! `InList` line for line.
//!
//! The list this serves is mostly a folded `NOT IN (SELECT ...)`: the
//! subquery runs once per execution and its column arrives as literals.
//! `owner NOT IN (SELECT id FROM main_table WHERE category = 1)` walked about
//! 1,600 entries for each of 25,000 outer rows, at a dyn call, a copy and two
//! affinity applications per step, and took 1 s where SQLite took 4.6 ms
//! (task-2175).

use inillucent_base::DbResult;
use inillucent_tree::datum::OwnedDatum;
use inillucent_value::affinity::{apply_affinity, Affinity};
use inillucent_value::collation::Collation;
use inillucent_value::compare::compare_values;
use inillucent_value::encoding::TextEncoding;
use inillucent_value::value::Value;

use crate::batch::Batch;
use crate::expr::{Computed, Eval};

/// The text encoding every value in a pipeline is in.
const ENCODING: TextEncoding = TextEncoding::Utf8;

/// The shortest list a sorted set is built for.
///
/// Below this the linear walk is as quick as the sort it saves, and a short
/// written list, `IN (1, 2, 3)`, is the common case.
pub const SET_THRESHOLD: usize = 8;

/// `x IN (list)` and `x NOT IN (list)` over constants, by binary search.
pub struct InSet {
    /// Whether `NOT` was written.
    negated: bool,
    /// The value being tested.
    operand: Box<dyn Eval>,
    /// The entries that are not NULL, with the affinity applied, sorted.
    sorted: Vec<Value<'static>>,
    /// Whether the list held a NULL.
    has_null: bool,
    /// Whether the list was empty.
    empty: bool,
    /// The affinity applied to the operand before comparing.
    affinity: Option<Affinity>,
    /// The collation the comparison uses.
    collation: Collation,
}

impl InSet {
    /// Builds the set from the list's constants.
    ///
    /// @param negated - whether `NOT` was written
    /// @param operand - the compiled value being tested
    /// @param list - the constants
    /// @param affinity - the affinity the comparison applies to both sides
    /// @param collation - the collation the comparison uses
    pub fn new(
        negated: bool,
        operand: Box<dyn Eval>,
        list: &[OwnedDatum],
        affinity: Option<Affinity>,
        collation: Collation,
    ) -> DbResult<InSet> {
        let mut sorted = Vec::with_capacity(list.len());
        let mut has_null = false;
        for entry in list {
            let value = Value::from(&entry.borrow()).into_owned()?;
            if value.is_null() {
                has_null = true;
                continue;
            }
            sorted.push(with_affinity(value, affinity));
        }
        sorted.sort_by(|left, right| compare_values(left, right, collation));
        Ok(InSet {
            negated,
            operand,
            sorted,
            has_null,
            empty: list.is_empty(),
            affinity,
            collation,
        })
    }
}

/// Applies a comparison's affinity, keeping the value as it was when it does not apply.
///
/// This is what `eval::ordered` does to each side, so the two paths compare
/// the same values.
///
/// @param value - the value
/// @param affinity - the affinity, when the comparison has one
fn with_affinity(value: Value<'static>, affinity: Option<Affinity>) -> Value<'static> {
    match affinity {
        Some(affinity) => apply_affinity(value.clone(), affinity, ENCODING).unwrap_or(value),
        None => value,
    }
}

impl Eval for InSet {
    fn value<'p>(&self, batch: &Batch<'p>, nth: usize) -> DbResult<Computed<'p>> {
        let operand = self.operand.value(batch, nth)?;
        let operand = Value::from(&operand.get()).into_owned()?;
        if operand.is_null() {
            if self.empty {
                return Ok(Computed::Owned(OwnedDatum::Int(i64::from(self.negated))));
            }
            return Ok(Computed::Owned(OwnedDatum::Null));
        }
        let operand = with_affinity(operand, self.affinity);
        let found = self
            .sorted
            .binary_search_by(|entry| compare_values(entry, &operand, self.collation))
            .is_ok();
        if found {
            return Ok(Computed::Owned(OwnedDatum::Int(i64::from(!self.negated))));
        }
        if self.has_null {
            return Ok(Computed::Owned(OwnedDatum::Null));
        }
        Ok(Computed::Owned(OwnedDatum::Int(i64::from(self.negated))))
    }
}

/// Returns the list's constants when every entry is one and the list is long enough for a set.
///
/// @param list - the compiled list's entries, as expressions
/// @param literal - returns an entry's constant, or `None` when it is not one
pub fn constants_of<'e, E>(
    list: &'e [E],
    literal: impl Fn(&'e E) -> Option<&'e OwnedDatum>,
) -> Option<Vec<OwnedDatum>> {
    if list.len() < SET_THRESHOLD {
        return None;
    }
    list.iter()
        .map(|entry| literal(entry).cloned())
        .collect::<Option<Vec<OwnedDatum>>>()
}

/// Reports whether two values compare equal under a collation; for the tests.
///
/// @param left - one value
/// @param right - the other
/// @param collation - the collation
#[cfg(test)]
fn equal(left: &Value<'_>, right: &Value<'_>, collation: Collation) -> bool {
    compare_values(left, right, collation) == std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An integer and the real with the same value are one entry, and a big integer is not its rounded real.
    #[test]
    fn numbers_compare_by_value_and_exactly() {
        let one = Value::Integer(1);
        let one_real = Value::Real(1.0);
        assert!(equal(&one, &one_real, Collation::Binary));
        let big = Value::Integer((1 << 53) + 1);
        let rounded = Value::Real((1u64 << 53) as f64);
        assert!(!equal(&big, &rounded, Collation::Binary));
    }
}

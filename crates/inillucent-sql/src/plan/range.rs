//! Reading the `WHERE` terms that bound one index key column as a range.
//!
//! Invariant: **`low` and `high` are the two ends of the walk, not of the
//! value.** A column the index holds descending runs the other way, so `k > 5`
//! is where its walk starts rather than where it stops, and every bound read
//! here is placed by the column's direction before it is placed by the
//! operator.
//!
//! Here rather than in [`super`] because `plan.rs` is at its recorded size and
//! this loop is one question `index_candidate` asks of the key column after
//! its equality prefix (task-2078).

use super::terms::{compares_with_null, mirror};
use super::*;

/// The range one index key column takes from the statement's terms.
pub(super) struct KeyRange {
    /// Where the walk starts, when a term bounds it.
    pub(super) low: Option<RangeBound>,
    /// Where the walk stops, when a term bounds it.
    pub(super) high: Option<RangeBound>,
    /// The collation the column is compared in.
    pub(super) collation: Collation,
    /// Whether the index holds the column descending.
    pub(super) descending: bool,
    /// The table column the key column is, `None` for a key the index computes.
    pub(super) column: Option<u16>,
    /// The terms a computed key's range answered, which are not tested again.
    pub(super) answered: Vec<usize>,
}

/// Returns the range the terms put on the key column after an index candidate's equality prefix.
///
/// A range is an outermost term path only, so an inner term has none. A range on a computed key
/// answers its terms the way an equality on one does, so the columns they read are not read again,
/// and they are added to `expression_terms`; see `covering::needed_beyond_expressions`.
///
/// @param context - the term being planned and its statement
/// @param index - the index being considered
/// @param computed - the bound key expressions of the index, when it has any
/// @param key - the position of the key column after the equality prefix
/// @param used - the terms the candidate has used
pub(super) fn range_after_prefix(
    context: &CandidateContext<'_>,
    index: &IndexInfo,
    computed: Option<&crate::dml::BoundIndexExprs>,
    key: usize,
    used: &mut Vec<usize>,
) -> Option<KeyRange> {
    let key_column = index.columns.get(key).filter(|_| context.position == 0)?;
    let wanted = computed
        .and_then(|held| held.keys.get(key))
        .and_then(Option::as_ref);
    let starts_at = used.len();
    let mut found = key_range(context, key_column, wanted, used)?;
    if key_column.column.is_none() {
        found.answered = used.iter().skip(starts_at).copied().collect();
    }
    Some(found)
}

/// Returns the range the terms put on one key column, or `None` when nothing
/// bounds it.
///
/// Each term used is pushed onto `used`, so the caller can mark it consumed if
/// this candidate is chosen and leave it as a residual if not.
/// @param context - the term being planned and its statement
/// @param key_column - the index key column after the equality prefix
/// @param computed - the bound expression the key computes, when it is not a bare column
/// @param used - the terms this candidate has taken so far
pub(super) fn key_range(
    context: &CandidateContext<'_>,
    key_column: &crate::catalog_view::IndexColumnInfo,
    computed: Option<&BoundExpr>,
    used: &mut Vec<usize>,
) -> Option<KeyRange> {
    let collation = collation_of(&key_column.collation);
    let column = key_column.plain_column();
    if column.is_none() {
        computed?;
    }
    let mut low = None;
    let mut high = None;
    for (term_index, term) in context.terms.iter().enumerate() {
        let taken = context.consumed.get(term_index).copied().unwrap_or(false);
        if taken || used.contains(&term_index) {
            continue;
        }
        // An anchored pattern is a range; see `plan::pattern`, which also says
        // why the term is left as a residual (task-1932, M7).
        if let Some((low_bound, high_bound)) = column.and_then(|column| {
            pattern::pattern_range(context.id, column, term, collation, key_column.descending)
        }) {
            if low.is_none() && high.is_none() {
                low = low_bound;
                high = high_bound;
            }
            continue;
        }
        // `expr BETWEEN a AND b` is both bounds at once. It is not split into two comparisons
        // ahead of planning (the operand would be evaluated twice by a scan), so a computed key
        // reads it here.
        if let (None, Some(wanted)) = (column, computed) {
            if let Some(halves) = between_halves(context, wanted, term, collation) {
                let placed = halves.map(|(op, value, unconverted)| {
                    walk_bound(op, key_column.descending).map(|(kind, at_low)| {
                        (
                            at_low,
                            RangeBound {
                                kind,
                                value,
                                unconverted,
                            },
                        )
                    })
                });
                if let [Some((first_low, first)), Some((second_low, second))] = placed {
                    if first_low != second_low && low.is_none() && high.is_none() {
                        for (at_low, bound) in [(first_low, first), (second_low, second)] {
                            if at_low {
                                low = Some(bound);
                            } else {
                                high = Some(bound);
                            }
                        }
                        used.push(term_index);
                    }
                }
                continue;
            }
        }
        // **A computed key is matched by its expression** and, like the equality prefix, takes
        // no affinity check: the entries hold the expression's value as it was computed.
        let found = match (column, computed) {
            (Some(column), _) => indexable_comparison(context.id, column, term),
            (None, Some(wanted)) => comparison_against_expr(wanted, term),
            (None, None) => None,
        };
        let Some((op, value)) = found else {
            continue;
        };
        if !is_available(context.position, context.ids, &value)
            || comparison_collation(term) != collation
        {
            continue;
        }
        // Reading `k > 5` on a descending column as a low bound seeks past
        // every row it was meant to return. It did: `WHERE k > 5` on a
        // descending index returned nothing at all, silently, with no ORDER BY
        // anywhere near it.
        let Some((kind, at_low)) = walk_bound(op, key_column.descending) else {
            continue;
        };
        let slot = if at_low { &mut low } else { &mut high };
        if slot.is_none() {
            *slot = Some(RangeBound {
                kind,
                value,
                unconverted: compares_unconverted(term),
            });
            used.push(term_index);
        }
    }
    if low.is_none() && high.is_none() {
        return None;
    }
    Some(KeyRange {
        low,
        high,
        collation,
        descending: key_column.descending,
        column,
        answered: Vec::new(),
    })
}

/// Returns the two comparisons a `BETWEEN` on an index's computed key makes, as operator, value
/// and whether the seek compares the value without converting it.
///
/// `None` when the term is not a plain `BETWEEN` on that expression, when either bound cannot be
/// computed before the loop, or when a bound compares in a different collation from the key's.
///
/// @param context - the term being planned and its statement
/// @param wanted - the bound expression the index key computes
/// @param term - one `WHERE` conjunct
/// @param collation - the collation the key is ordered by
fn between_halves(
    context: &CandidateContext<'_>,
    wanted: &BoundExpr,
    term: &BoundExpr,
    collation: Collation,
) -> Option<[(BinaryOp, BoundExpr, bool); 2]> {
    let BoundExpr::Between {
        negated: false,
        operand,
        low,
        high,
        low_affinity,
        low_collation,
        high_affinity,
        high_collation,
    } = term
    else {
        return None;
    };
    let available = |value: &BoundExpr| is_available(context.position, context.ids, value);
    if operand.as_ref() != wanted
        || *low_collation != collation
        || *high_collation != collation
        || !available(low)
        || !available(high)
        || compares_with_null(operand, low)
        || compares_with_null(operand, high)
    {
        return None;
    }
    Some([
        (
            BinaryOp::GreaterEqual,
            low.as_ref().clone(),
            low_affinity.is_none(),
        ),
        (
            BinaryOp::LessEqual,
            high.as_ref().clone(),
            high_affinity.is_none(),
        ),
    ])
}

/// Returns the operator and the other side when a term compares an index's computed key with
/// something else, the key's expression on either side.
///
/// A comparison against a NULL literal is not one, for the reason
/// `comparison_against_column` gives.
///
/// @param wanted - the bound expression the index key computes
/// @param term - one `WHERE` conjunct
fn comparison_against_expr(wanted: &BoundExpr, term: &BoundExpr) -> Option<(BinaryOp, BoundExpr)> {
    let BoundExpr::Compare {
        op, left, right, ..
    } = term
    else {
        return None;
    };
    if compares_with_null(left, right) {
        return None;
    }
    if left.as_ref() == wanted {
        return Some((*op, right.as_ref().clone()));
    }
    if right.as_ref() == wanted {
        return Some((mirror(*op), left.as_ref().clone()));
    }
    None
}

/// Returns the bound a comparison puts on the walk, and whether it is the
/// walk's start, for a column held in the given direction.
/// @param op - the comparison, with the column on its left
/// @param descending - whether the index holds the column descending
fn walk_bound(op: BinaryOp, descending: bool) -> Option<(BoundKind, bool)> {
    Some(match (op, descending) {
        (BinaryOp::Greater, false) => (BoundKind::Greater, true),
        (BinaryOp::GreaterEqual, false) => (BoundKind::GreaterEqual, true),
        (BinaryOp::Less, false) => (BoundKind::Less, false),
        (BinaryOp::LessEqual, false) => (BoundKind::LessEqual, false),
        (BinaryOp::Greater, true) => (BoundKind::Less, false),
        (BinaryOp::GreaterEqual, true) => (BoundKind::LessEqual, false),
        (BinaryOp::Less, true) => (BoundKind::Greater, true),
        (BinaryOp::LessEqual, true) => (BoundKind::GreaterEqual, true),
        _ => return None,
    })
}

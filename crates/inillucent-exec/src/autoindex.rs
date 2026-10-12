//! The automatic index: what `PRAGMA automatic_index` turns on and off.
//!
//! Invariant: an automatic index changes how fast a join or a correlated
//! subquery runs and never what it answers. The key rule below takes the
//! equalities of a condition that have one side reading only outer columns and
//! the other only inner ones. The conjuncts it does not take are handed back as
//! `rest`, and the join tests `rest` for each candidate pair, so a predicate
//! the hash key did not capture is still applied to every pair the key finds.
//! A correlated subquery's built table keeps the whole residual in the chain
//! for the same reason.
//!
//! # Why a hash table rather than a b-tree
//!
//! SQLite calls this an *automatic index* because that is what it builds: a
//! transient b-tree over the inner table, keyed on the join column, thrown away
//! when the statement ends. It builds a tree because its executor has one join
//! operator - a nested loop - and the only way to make the inner side cheap is
//! to give it something to seek in.
//!
//! This engine has a hash join, so the same idea costs less here: the inner
//! side is read once into a hash table keyed on the join expression, and each
//! outer row probes it. Both are "build a throwaway structure over the inner
//! side so the join stops being quadratic", both are under one switch, and the
//! switch means the same thing to an application - which is what
//! `PRAGMA automatic_index = off` is asking about.
//!
//! Without it, an unindexed inner side is read once into a vector and every
//! outer row walks the whole vector, which is what this engine used to always
//! do and is what `off` still selects.
//!
//! A correlated subquery whose table has no index for its equality against the
//! outer row uses the same table: [`HashTable`] holds the block's table, read
//! once at the first outer row, and `crate::correlate` looks the outer row's
//! value up in it. A key side may be any expression that reads one side of the
//! join and gives the same answer each time it is evaluated, so `a.k = b.k + 1`
//! is keyed on `b.k + 1`.

use std::collections::HashMap;

use inillucent_base::DbResult;
use inillucent_tree::datum::{Datum, OwnedDatum};

use crate::batch::{Batch, Vector};
use crate::expr::{Eval, Expr};

/// A hash table over encoded join keys.
///
/// The key is the memcmp encoding rather than the values, for the same reason
/// an interior page's separator is: one `Vec<u8>` hashes and compares in one
/// call where a tuple of tagged values dispatches per column. The encoding is
/// exact - `inillucent-tree`'s key module documents how - so two rows hash together
/// exactly when SQL says their keys are equal.
pub(crate) struct HashTable {
    /// Encoded key to the positions in `rows` that carry it.
    index: HashMap<Vec<u8>, Vec<u32>>,
    /// The build side's rows.
    pub(crate) rows: Vec<Vec<OwnedDatum>>,
    /// Which build rows have been matched, for a right/full outer join.
    pub(crate) matched: Vec<bool>,
}
impl HashTable {
    /// Returns an empty table.
    pub(crate) fn new() -> HashTable {
        HashTable {
            index: HashMap::new(),
            rows: Vec::new(),
            matched: Vec::new(),
        }
    }

    /// Forgets every build row, so the join can be run again.
    pub(crate) fn clear(&mut self) {
        self.index.clear();
        self.rows.clear();
        self.matched.clear();
    }

    /// Adds one build row under an encoded key.
    ///
    /// A NULL in the key is dropped rather than indexed: SQL equality is never
    /// true for NULL, so a NULL-keyed build row can never match a probe, and
    /// keeping it in the table would only make it findable.
    ///
    /// @param key - the encoded join key
    /// @param row - the build row
    /// @param has_null - whether any key column was NULL
    pub(crate) fn insert(&mut self, key: Vec<u8>, row: Vec<OwnedDatum>, has_null: bool) {
        let position = self.rows.len() as u32;
        self.rows.push(row);
        self.matched.push(false);
        if has_null {
            return;
        }
        self.index.entry(key).or_default().push(position);
    }

    /// Returns the build rows carrying an encoded key.
    ///
    /// @param key - the encoded probe key
    pub(crate) fn probe(&self, key: &[u8]) -> &[u32] {
        self.index.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Adds every live row of one batch, charging the request budget for it.
    ///
    /// The build side is charged here because it is the whole of one input held
    /// in memory before a single output row exists.
    ///
    /// @param keys - the key expressions over the batch's columns
    /// @param batch - a batch of build rows
    pub(crate) fn absorb(&mut self, keys: &[Box<dyn Eval>], batch: &Batch<'_>) -> DbResult<()> {
        inillucent_base::budget::materialise(crate::ops::batch_bytes(batch))?;
        let width = batch.columns.len();
        for nth in 0..batch.live() {
            let mut row = Vec::with_capacity(width);
            for column in 0..width {
                row.push(OwnedDatum::from_datum(&batch.value(nth, column)?));
            }
            let (encoded, has_null) = crate::join::encode_row_key(keys, batch, nth)?;
            self.insert(encoded, row, has_null);
        }
        Ok(())
    }

    /// Adds rows that were materialised rather than streamed, copying them.
    ///
    /// @param keys - the key expressions over the rows' columns
    /// @param rows - the build side's rows, in the order they were read
    pub(crate) fn absorb_rows(
        &mut self,
        keys: &[Box<dyn Eval>],
        rows: &[Vec<OwnedDatum>],
    ) -> DbResult<()> {
        in_batches(rows, &mut |batch| self.absorb(keys, batch))
    }

    /// Adds rows that were materialised, taking them without a copy.
    ///
    /// The request budget is charged for the rows before any is added, so a
    /// refusal leaves the table empty. The caller that can read its table
    /// another way asks [`inillucent_base::budget::fits`] with
    /// [`owned_bytes`] first and does not call this when the answer is no.
    ///
    /// @param keys - the key expressions over the rows' columns
    /// @param rows - the build side's rows, in the order they were read
    pub(crate) fn absorb_owned(
        &mut self,
        keys: &[Box<dyn Eval>],
        rows: Vec<Vec<OwnedDatum>>,
    ) -> DbResult<()> {
        inillucent_base::budget::materialise(owned_bytes(&rows))?;
        let mut encoded: Vec<(Vec<u8>, bool)> = Vec::with_capacity(rows.len());
        in_batches(&rows, &mut |batch| {
            for nth in 0..batch.live() {
                encoded.push(crate::join::encode_row_key(keys, batch, nth)?);
            }
            Ok(())
        })?;
        for (row, (key, has_null)) in rows.into_iter().zip(encoded) {
            self.insert(key, row, has_null);
        }
        Ok(())
    }
}

/// Returns roughly how many bytes a list of rows holds, as the request budget counts them.
///
/// @param rows - the rows to measure
pub(crate) fn owned_bytes(rows: &[Vec<OwnedDatum>]) -> u64 {
    rows.iter()
        .map(|row| crate::ops::owned_row_bytes(row))
        .fold(0u64, u64::saturating_add)
}

/// Hands rows to a function a batch at a time.
///
/// The key expressions are compiled against a batch, and evaluating them one
/// row at a time would mean a second evaluator.
///
/// @param rows - the rows, in order
/// @param work - what to do with each batch
fn in_batches(
    rows: &[Vec<OwnedDatum>],
    work: &mut dyn FnMut(&Batch<'_>) -> DbResult<()>,
) -> DbResult<()> {
    let width = rows.first().map(Vec::len).unwrap_or(0);
    if width == 0 {
        return Ok(());
    }
    let mut start = 0usize;
    while start < rows.len() {
        let end = start
            .saturating_add(crate::batch::BATCH_ROWS)
            .min(rows.len());
        let chunk = rows.get(start..end).unwrap_or(&[]);
        let mut held: Vec<Vec<Datum<'_>>> = Vec::with_capacity(width);
        for column in 0..width {
            held.push(
                chunk
                    .iter()
                    .map(|row| {
                        row.get(column)
                            .map(OwnedDatum::borrow)
                            .unwrap_or(Datum::Null)
                    })
                    .collect(),
            );
        }
        let columns: Vec<Vector<'_>> = held
            .iter()
            .map(|values| Vector::Values(values.as_slice()))
            .collect();
        work(&Batch::new(chunk.len(), columns))?;
        start = end;
    }
    Ok(())
}

/// The halves of an equi-join condition, ready to compile, and what is left.
pub struct EquiKeys {
    /// The key expressions over the outer row, which probe the table.
    pub probe: Vec<Expr>,
    /// The key expressions over the inner row, which build it.
    ///
    /// Already rebased: an inner column that is column `offset + n` of the
    /// joined row is column `n` here, because the build side is pushed in as
    /// batches of the inner stage alone.
    pub build: Vec<Expr>,
    /// The conjuncts that are not key equalities, joined again with `And`, over
    /// the joined row. `None` when the keys took every conjunct.
    pub rest: Option<Expr>,
}

/// Returns the equi-join keys a condition offers, or nothing if it offers none.
///
/// A conjunct is a key when it is an equality whose two sides each read one
/// side of the join, and either side may be any deterministic expression. Every
/// other conjunct goes into [`EquiKeys::rest`], which the join tests for each
/// candidate pair. Nothing is returned when no conjunct is a key, and the
/// caller then builds the nested loop it would have built anyway.
///
/// @param condition - the `ON` condition, over the joined row's columns
/// @param offset - the first column of the inner stage in the joined row
/// @param width - how many columns the inner stage has
pub fn equi_keys(condition: &Expr, offset: usize, width: usize) -> Option<EquiKeys> {
    let mut terms = Vec::new();
    flatten_and(condition, &mut terms);
    let mut keys = EquiKeys {
        probe: Vec::new(),
        build: Vec::new(),
        rest: None,
    };
    let mut left_over: Vec<Expr> = Vec::new();
    for term in terms {
        if !take_key(term, offset, width, &mut keys) {
            left_over.push(term.clone());
        }
    }
    keys.rest = left_over
        .into_iter()
        .reduce(|left, right| Expr::And(Box::new(left), Box::new(right)));
    (!keys.probe.is_empty()).then_some(keys)
}

/// Adds one conjunct to the keys when it is a key equality, and says whether it was.
///
/// Both comparison shapes are accepted: the plain one, and the one that
/// carries an affinity. The affinity is applied to both key expressions, so
/// the two sides of the hash table are hashed after the same conversion the
/// comparison would have made. A collation other than `BINARY` is not a key:
/// the keys are byte encoded, and `NOCASE` equality is not byte equality.
///
/// @param term - one conjunct of the condition
/// @param offset - the first column of the inner stage in the joined row
/// @param width - how many columns the inner stage has
/// @param keys - where a key equality's two halves go
fn take_key(term: &Expr, offset: usize, width: usize, keys: &mut EquiKeys) -> bool {
    let (left, right, affinity) = match term {
        Expr::Compare(crate::expr::CompareOp::Equal, left, right) => (left, right, None),
        Expr::CompareWith {
            op: crate::expr::CompareOp::Equal,
            affinity,
            collation: inillucent_value::Collation::Binary,
            left,
            right,
        } => (left, right, *affinity),
        _ => return false,
    };
    let (Some(left_side), Some(right_side)) =
        (side_of(left, offset, width), side_of(right, offset, width))
    else {
        return false;
    };
    let (outer, inner) = match (left_side, right_side) {
        (Side::Outer, Side::Inner) => (left, right),
        (Side::Inner, Side::Outer) => (right, left),
        // A term reading one side only is a filter rather than a join key, and
        // a term reading neither is a constant.
        _ => return false,
    };
    let Some(rebased_inner) = rebased(inner, offset) else {
        return false;
    };
    keys.probe.push(converted((**outer).clone(), affinity));
    keys.build.push(converted(rebased_inner, affinity));
    true
}

/// Wraps a key expression in the conversion the comparison would have applied.
///
/// @param expr - the key expression
/// @param affinity - the comparison's affinity, when it has one
pub(crate) fn converted(expr: Expr, affinity: Option<inillucent_value::Affinity>) -> Expr {
    match affinity {
        None => expr,
        Some(affinity) => Expr::Affinity {
            operand: Box::new(expr),
            affinity,
            widen: false,
        },
    }
}

/// Which side of the join an expression reads.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    /// Only columns the stages before this one produced.
    Outer,
    /// Only this stage's own columns.
    Inner,
}

/// Returns which side an expression reads, or nothing when it reads both.
///
/// An expression reading no column at all is a constant, and a constant is not
/// a join key - reporting it as one would put every row in one hash bucket.
///
/// @param expr - the expression
/// @param offset - the first column of the inner stage
/// @param width - how many columns the inner stage has
fn side_of(expr: &Expr, offset: usize, width: usize) -> Option<Side> {
    let mut columns = Vec::new();
    collect_columns(expr, &mut columns)?;
    if columns.is_empty() {
        return None;
    }
    let outer = columns.iter().all(|at| *at < offset);
    let inner = columns
        .iter()
        .all(|at| *at >= offset && *at < offset.saturating_add(width));
    match (outer, inner) {
        (true, _) => Some(Side::Outer),
        (_, true) => Some(Side::Inner),
        _ => None,
    }
}

/// Returns the expression with its inner column numbers rebased from zero.
///
/// @param expr - an expression reading only inner columns
/// @param offset - the first column of the inner stage
fn rebased(expr: &Expr, offset: usize) -> Option<Expr> {
    let mut copy = expr.clone();
    shift_columns(&mut copy, offset)?;
    Some(copy)
}

/// Moves every column number in an expression down by `offset`.
///
/// @param expr - the expression to rewrite in place
/// @param offset - how far down each column number moves
fn shift_columns(expr: &mut Expr, offset: usize) -> Option<()> {
    if let Expr::Column(at) = expr {
        *at = at.checked_sub(offset)?;
        return Some(());
    }
    for child in children_mut(expr)? {
        shift_columns(child, offset)?;
    }
    Some(())
}

/// Collects every column an expression reads, or reports it cannot be walked.
///
/// A node this walk does not understand, or one whose value changes between
/// two evaluations in one statement, returns `None`, which declines the
/// optimisation. Evaluating such a key once per row instead of once per pair
/// would change the answer.
///
/// @param expr - the expression
/// @param out - where the column numbers go
pub(crate) fn collect_columns(expr: &Expr, out: &mut Vec<usize>) -> Option<()> {
    let mut copy = expr.clone();
    collect_from(&mut copy, out)
}

/// The walk behind [`collect_columns`], over a copy it may borrow mutably.
///
/// @param expr - the expression
/// @param out - where the column numbers go
fn collect_from(expr: &mut Expr, out: &mut Vec<usize>) -> Option<()> {
    if let Expr::Column(at) = expr {
        out.push(*at);
        return Some(());
    }
    for child in children_mut(expr)? {
        collect_from(child, out)?;
    }
    Some(())
}

/// Returns the direct sub expressions of a node, or nothing when the node
/// cannot be keyed on.
///
/// A registered application function, `RAISE`, and the built in functions whose
/// answer differs between calls (`random`, `randomblob`, `changes`,
/// `total_changes`, `last_insert_rowid`) return `None`.
///
/// @param expr - the node
fn children_mut(expr: &mut Expr) -> Option<Vec<&mut Expr>> {
    Some(match expr {
        Expr::Column(_) | Expr::Literal(_) | Expr::Parameter { .. } => Vec::new(),
        Expr::External { .. } | Expr::Raise { .. } => return None,
        Expr::RowOffset { rowid, .. } => vec![&mut **rowid],
        Expr::Arith(_, left, right)
        | Expr::Compare(_, left, right)
        | Expr::And(left, right)
        | Expr::Or(left, right)
        | Expr::CompareWith { left, right, .. }
        | Expr::General { left, right, .. }
        | Expr::Is { left, right, .. } => vec![&mut **left, &mut **right],
        Expr::Not(operand)
        | Expr::IsNull(operand)
        | Expr::IsNotNull(operand)
        | Expr::Length(operand)
        | Expr::Unary { operand, .. }
        | Expr::Cast { operand, .. }
        | Expr::Affinity { operand, .. } => vec![&mut **operand],
        Expr::Call {
            func, arguments, ..
        } => {
            if !is_repeatable(*func) {
                return None;
            }
            arguments.iter_mut().collect()
        }
        Expr::Math { arguments, .. }
        | Expr::Time { arguments, .. }
        | Expr::Json { arguments, .. } => arguments.iter_mut().collect(),
        Expr::Between {
            operand, low, high, ..
        } => vec![&mut **operand, &mut **low, &mut **high],
        Expr::InList { operand, list, .. } => {
            let mut all = vec![&mut **operand];
            all.extend(list.iter_mut());
            all
        }
        Expr::Case {
            operand,
            branches,
            otherwise,
            ..
        } => {
            let mut all: Vec<&mut Expr> = Vec::new();
            all.extend(operand.iter_mut().map(|held| &mut **held));
            for (when, then) in branches.iter_mut() {
                all.push(when);
                all.push(then);
            }
            all.extend(otherwise.iter_mut().map(|held| &mut **held));
            all
        }
        Expr::Pattern {
            operand,
            pattern,
            escape,
            ..
        } => {
            let mut all = vec![&mut **operand, &mut **pattern];
            all.extend(escape.iter_mut().map(|held| &mut **held));
            all
        }
    })
}

/// Reports whether a built in function answers the same for the same arguments
/// throughout one statement.
///
/// @param func - the function
fn is_repeatable(func: inillucent_sql::function::ScalarFunc) -> bool {
    use inillucent_sql::function::ScalarFunc;
    !matches!(
        func,
        ScalarFunc::Random
            | ScalarFunc::RandomBlob
            | ScalarFunc::Changes
            | ScalarFunc::TotalChanges
            | ScalarFunc::LastInsertRowid
    )
}

/// Splits a conjunction into its terms.
///
/// @param expr - the condition
/// @param out - where the terms go, in written order
fn flatten_and<'e>(expr: &'e Expr, out: &mut Vec<&'e Expr>) {
    if let Expr::And(left, right) = expr {
        flatten_and(left, out);
        flatten_and(right, out);
        return;
    }
    out.push(expr);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::CompareOp;

    /// Returns `left = right` as the planner would build it.
    fn equal(left: Expr, right: Expr) -> Expr {
        Expr::Compare(CompareOp::Equal, Box::new(left), Box::new(right))
    }

    /// The plain case: one equality with a column on each side.
    #[test]
    fn one_column_each_side_is_a_key() {
        let condition = equal(Expr::Column(0), Expr::Column(3));
        let keys = equi_keys(&condition, 3, 2).expect("an equi-join");
        assert_eq!(keys.probe.len(), 1);
        assert_eq!(keys.build.len(), 1);
        // The build key is rebased to the inner stage's own numbering.
        assert!(matches!(keys.build.first(), Some(Expr::Column(0))));
    }

    /// Written the other way round, which is the same join.
    #[test]
    fn the_sides_may_be_written_either_way_round() {
        let condition = equal(Expr::Column(3), Expr::Column(0));
        let keys = equi_keys(&condition, 3, 2).expect("an equi-join");
        assert!(matches!(keys.probe.first(), Some(Expr::Column(0))));
        assert!(matches!(keys.build.first(), Some(Expr::Column(0))));
    }

    /// Two equalities are a composite key rather than a reason to decline.
    #[test]
    fn a_conjunction_of_equalities_is_a_composite_key() {
        let condition = Expr::And(
            Box::new(equal(Expr::Column(0), Expr::Column(3))),
            Box::new(equal(Expr::Column(1), Expr::Column(4))),
        );
        let keys = equi_keys(&condition, 3, 2).expect("an equi-join");
        assert_eq!(keys.probe.len(), 2);
    }

    /// A condition with no key equality declines, so the caller builds the
    /// nested loop it always did.
    #[test]
    fn a_condition_without_a_key_equality_declines() {
        // A term reading only the outer side is a filter, not a key.
        let filter = equal(Expr::Column(0), Expr::Column(1));
        assert!(equi_keys(&filter, 3, 2).is_none());
        // A range is not an equality.
        assert!(equi_keys(&range(Expr::Column(0), Expr::Column(3)), 3, 2).is_none());
        // An equality under a NOCASE collation is not byte equality.
        let nocase = Expr::CompareWith {
            op: CompareOp::Equal,
            affinity: None,
            collation: inillucent_value::Collation::NoCase,
            left: Box::new(Expr::Column(0)),
            right: Box::new(Expr::Column(3)),
        };
        assert!(equi_keys(&nocase, 3, 2).is_none());
    }

    /// Returns `left < right`.
    fn range(left: Expr, right: Expr) -> Expr {
        Expr::Compare(CompareOp::Less, Box::new(left), Box::new(right))
    }

    /// A key equality beside a range is a key, and the range is handed back.
    #[test]
    fn a_range_beside_a_key_is_returned_as_the_rest() {
        let mixed = Expr::And(
            Box::new(equal(Expr::Column(0), Expr::Column(3))),
            Box::new(range(Expr::Column(1), Expr::Column(4))),
        );
        let keys = equi_keys(&mixed, 3, 2).expect("one key");
        assert_eq!(keys.probe.len(), 1);
        assert!(matches!(
            keys.rest,
            Some(Expr::Compare(CompareOp::Less, _, _))
        ));
        let plain = equi_keys(&equal(Expr::Column(0), Expr::Column(3)), 3, 2).expect("a key");
        assert!(plain.rest.is_none());
    }

    /// An expression on the inner side is a key, with its columns rebased.
    #[test]
    fn an_expression_side_is_a_key_over_the_rebased_columns() {
        let sum = Expr::Arith(
            crate::expr::ArithOp::Add,
            Box::new(Expr::Column(3)),
            Box::new(Expr::Literal(OwnedDatum::Int(1))),
        );
        let keys = equi_keys(&equal(Expr::Column(0), sum), 3, 2).expect("a key");
        match keys.build.first() {
            Some(Expr::Arith(_, left, _)) => assert!(matches!(**left, Expr::Column(0))),
            other => panic!("expected the rebased sum, got {other:?}"),
        }
    }

    /// An expression whose answer changes between calls is not a key.
    #[test]
    fn a_random_side_is_not_a_key() {
        let random = Expr::Call {
            func: inillucent_sql::function::ScalarFunc::Random,
            arguments: Vec::new(),
            collation: inillucent_value::Collation::Binary,
            context: Default::default(),
        };
        let sum = Expr::Arith(
            crate::expr::ArithOp::Add,
            Box::new(Expr::Column(3)),
            Box::new(random),
        );
        assert!(equi_keys(&equal(Expr::Column(0), sum), 3, 2).is_none());
    }
}

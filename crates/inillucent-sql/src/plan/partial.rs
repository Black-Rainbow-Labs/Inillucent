//! Whether a query's `WHERE` proves a partial index's predicate.
//!
//! Invariant: **an index is chosen only when every row the query wants is in
//! it.** A partial index holds the rows its predicate accepted and no others,
//! so reading one for a query whose rows it does not all hold answers the
//! question with rows missing - and the rows missing are exactly the ones the
//! predicate excludes, which is the hardest kind of wrong answer to notice.
//! Every rule here is therefore sound on its own rather than likely, and a
//! rule nobody could write is an index left unchosen.
//!
//! Here rather than in [`super`] because `plan.rs` is at its recorded size and
//! these two functions are one question its caller asks once.

use super::*;

/// Reports whether a query's `WHERE` implies a partial index's predicate.
///
/// **Three rules, all sound, and nothing that needs a theorem prover.**
///
/// The first is each conjunct of the predicate appearing, unchanged, as a conjunct of the
/// statement's `WHERE`. So an index declared `WHERE b > 5` answers
/// `WHERE b > 5 AND a = 1` and does not answer `WHERE b > 6`, even though the
/// second implies the first. Proving the general implication is a theorem
/// prover in the planner, and every case it got wrong would be a query
/// silently missing exactly the rows the predicate excludes. A predicate that is an `AND`
/// needs every one of its conjuncts proved, and one that is an `OR` is proved when either side
/// is, as `sqlite3ExprImpliesExpr` does it.
///
/// The second is `IS NOT NULL`, and it was measured rather than reasoned about
/// (task-1913). This function's comment used to call the verbatim rule
/// "SQLite's rule"; the pinned 3.53.4 answers `SELECT n FROM t WHERE n = 1`
/// with `SEARCH t USING COVERING INDEX t_n (n=?)` over an index declared
/// `WHERE n IS NOT NULL`, and this engine scanned the table. That index is how
/// SQLite spells "unique among the rows that have one", so the shape is
/// common and the whole point of declaring it was to be searched.
///
/// The rule added is the narrowest one that answers it: a comparison is three
/// valued, so `n = 1` is *true* only when `n` is not NULL - and the same holds
/// for `<`, `<=`, `>`, `>=` and `<>`. A conjunct comparing the operand the
/// predicate asks about therefore proves the predicate. `IS` and `IS NOT` are
/// deliberately not comparisons here: `n IS NULL` is true precisely when `n`
/// is NULL, so reading it as proof of the opposite would choose an index that
/// holds none of the rows the query wants.
///
/// `false` when the index's own predicate could not be bound, which is what
/// leaves an index the planner cannot reason about unchosen rather than chosen
/// on a guess.
///
/// @param computed - the index's bound expressions, when it has them
/// @param terms - the statement's `WHERE` conjuncts
pub(super) fn implies(computed: Option<&crate::dml::BoundIndexExprs>, terms: &[BoundExpr]) -> bool {
    let Some(held) = computed else {
        return false;
    };
    let Some(predicate) = held.predicate.as_ref() else {
        return false;
    };
    // SQLite proves the predicate one conjunct at a time (`whereUsablePartialIndex`), and each
    // conjunct needs one term of the query that implies it. An index declared
    // `WHERE s NOT IN ('p') AND s IS NOT 'z'` is therefore usable by a query that repeats
    // both, which comparing the whole predicate against single terms never found.
    conjunction(predicate)
        .iter()
        .all(|wanted| terms.iter().any(|term| implies_expression(term, wanted)))
}

/// Reports whether one conjunct of a query's `WHERE` implies one conjunct of an index's
/// predicate, by the three rules of SQLite's `sqlite3ExprImpliesExpr`.
///
/// The term is the predicate; or the predicate is an `OR` and the term implies either side
/// (so `s = 'x'` implies `s = 'x' OR s = 'y'`); or the predicate is `x IS NOT NULL` and the
/// term cannot be true when `x` is NULL. Nothing else is tried: a rule that proves more would
/// need a theorem prover, and a rule that is wrong leaves rows out of the answer.
///
/// @param term - one conjunct of the statement's `WHERE`
/// @param wanted - one conjunct of the index's predicate
fn implies_expression(term: &BoundExpr, wanted: &BoundExpr) -> bool {
    if term == wanted {
        return true;
    }
    match wanted {
        BoundExpr::Or(left, right) => {
            implies_expression(term, left) || implies_expression(term, right)
        }
        BoundExpr::IsNull {
            negated: true,
            operand,
        } => cannot_be_true_when_null(term, operand.as_ref()),
        _ => false,
    }
}

/// Reports whether a conjunct can only be true when `operand` is not NULL.
///
/// A comparison, a `BETWEEN`, an `IN` over a list that has a value in it, and `IS NOT NULL`
/// itself are all NULL or false when the operand is NULL. `IS` and `IS NOT` are not: they never
/// yield NULL, and `x IS NOT 5` is true when `x` is NULL.
///
/// @param term - one conjunct of the statement's `WHERE`
/// @param operand - the expression the index's predicate asks about
fn cannot_be_true_when_null(term: &BoundExpr, operand: &BoundExpr) -> bool {
    match term {
        BoundExpr::IsNull {
            negated: true,
            operand: tested,
        } => tested.as_ref() == operand,
        BoundExpr::Between {
            operand: tested, ..
        } => tested.as_ref() == operand,
        BoundExpr::InList {
            operand: tested,
            list,
            ..
        } => tested.as_ref() == operand && !list.is_empty(),
        _ => compares_operand(term, operand),
    }
}

/// Returns what a partial index has to hold to cover a query that repeats its predicate.
///
/// Every entry of a partial index satisfies the predicate, so a conjunct that is the predicate
/// itself reads nothing from the row: the pinned release plans `SELECT 1 FROM t WHERE a = ? AND
/// flag = 0` over an index on `(a)` declared `WHERE flag = 0` as a `COVERING INDEX` although
/// `flag` is in no entry. The answer is the columns read outside the `WHERE`, plus the columns
/// of every other conjunct, and the positions of the conjuncts that are the predicate, which the
/// path then does not have to test again. `None` when no conjunct is the predicate.
///
/// @param id - the statement-wide number of the FROM term
/// @param needed - what the whole query reads of the term
/// @param terms - the statement's `WHERE` conjuncts
/// @param computed - the index's bound expressions
pub(super) fn needed_beyond_predicate(
    id: usize,
    needed: &ColumnUse,
    terms: &[BoundExpr],
    computed: Option<&crate::dml::BoundIndexExprs>,
) -> Option<(ColumnUse, Vec<usize>)> {
    let predicate = computed?.predicate.as_ref()?;
    let implied: Vec<usize> = terms
        .iter()
        .enumerate()
        .filter(|(_, term)| *term == predicate)
        .map(|(position, _)| position)
        .collect();
    if implied.is_empty() {
        return None;
    }
    let mut reduced = ColumnUse {
        columns: needed.outside_filter.clone(),
        rowid: needed.rowid,
        opaque: needed.opaque,
        functions: needed.functions.clone(),
        outside_filter: needed.outside_filter.clone(),
    };
    for (position, term) in terms.iter().enumerate() {
        if !implied.contains(&position) {
            term.columns_read(id, &mut reduced);
        }
    }
    Some((reduced, implied))
}

/// Reports whether a conjunct is a comparison that one operand has to be
/// non-NULL to satisfy.
///
/// Only [`BoundExpr::Compare`], and only its six comparison operators. A
/// `Compare` holding an arithmetic operator is not a conjunct the binder
/// produces, and the match is written out rather than defaulted so that an
/// operator added later is a compilation error rather than a wrong answer.
///
/// @param term - one conjunct of the statement's `WHERE`
/// @param operand - the expression the index's predicate asks about
pub(super) fn compares_operand(term: &BoundExpr, operand: &BoundExpr) -> bool {
    let BoundExpr::Compare {
        op, left, right, ..
    } = term
    else {
        return false;
    };
    if !matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    ) {
        return false;
    }
    left.as_ref() == operand || right.as_ref() == operand
}

//! Which of a statement's constant `WHERE` terms are ever tested.
//!
//! Invariant: **a constant term after one that is certainly not true is never evaluated.** SQLite
//! tests each term of a `WHERE` that reads no table once, in the order written, before its loop,
//! and leaves the loop at the first one that fails, so a term after it that would raise an error
//! raises nothing.
//!
//! Here rather than in `chain.rs` because that file holds the operators and this is one rule about
//! the terms they are built from.

use inillucent_sql::bind::BoundExpr;
use inillucent_tree::datum::OwnedDatum;

use crate::physical::Params;

/// Returns the constant filter cut off after its first term that is certainly not true.
///
/// A term is certainly not true when it folds to NULL, integer 0 or real 0.0. A term that calls a
/// function whose answer changes, or that cannot be folded, is never taken for one, so it and every
/// term after it stay: they are tested by the filter, as before.
///
/// `WHERE NULL AND (SELECT sum(a) FROM t)` over a table whose sum overflows is an empty answer in
/// SQLite and was "integer overflow" here, because the whole filter was one expression and the
/// failing term was evaluated beside the NULL one (bug hunt 11).
///
/// @param constant - the terms of the `WHERE` that read no table, joined by `AND`
/// @param params - the bound parameters, with every uncorrelated subquery already answered
pub(crate) fn tested_terms(constant: &BoundExpr, params: &Params) -> BoundExpr {
    let mut kept: Vec<BoundExpr> = Vec::new();
    for term in inillucent_sql::plan::conjunction(constant) {
        let stops = !inillucent_sql::plan::calls_a_volatile_function(&term)
            && match crate::constant::literal_value(&term, params) {
                Ok(OwnedDatum::Null) | Ok(OwnedDatum::Int(0)) => true,
                Ok(OwnedDatum::Real(number)) => number == 0.0,
                _ => false,
            };
        kept.push(term);
        if stops {
            break;
        }
    }
    let mut terms = kept.into_iter();
    match terms.next() {
        Some(first) => terms.fold(first, |left, right| {
            BoundExpr::And(Box::new(left), Box::new(right))
        }),
        None => constant.clone(),
    }
}

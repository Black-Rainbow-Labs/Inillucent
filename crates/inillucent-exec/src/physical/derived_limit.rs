//! Carrying a `LIMIT` from the statement above into the query a derived table runs.
//!
//! Invariant: **the plan returned from here produces the first rows of the plan it was made from,
//! in the same order, and no others past the bound.** It is made only when the rows above need no
//! more than the bound and nothing below could reorder them, so it changes how many rows are
//! generated and never which rows come first.
//!
//! **Why.** `SELECT term FROM v1 LIMIT 5` over a view of an unbounded recursive CTE never
//! returned: the view is a derived table, which is run to completion before the `LIMIT` above it
//! is applied, and the same CTE read directly stops after five rows because its `LIMIT` reaches the
//! recursion. A derived table, a CTE chain and a view all reach the executor as the same thing.

use inillucent_sql::bind::BoundExpr;
use inillucent_sql::plan::PhysicalPlan;

/// Returns the plan of a derived table with the rows above's bound as its own `LIMIT`, or `None`
/// when the plan should run as it is.
///
/// A plan is left alone when there is no bound, when the bound is no smaller than its own `LIMIT`,
/// when its `LIMIT` is not a number the planner can read, when it is a compound or a window query
/// (their rows come out of other operators), or when it is shared by several references (the
/// first run's rows are kept for the others, so a cut run would hand them too few).
///
/// @param inner - the derived table's plan
/// @param bound - how many rows the statement above keeps, offset included, when it says
pub(crate) fn bounded_inner_plan(
    inner: &PhysicalPlan,
    bound: Option<usize>,
) -> Option<PhysicalPlan> {
    let bound = i64::try_from(bound?).ok()?;
    if !inner.compounds.is_empty()
        || !inner.select.windows.is_empty()
        || inner.select.shared.is_some()
    {
        return None;
    }
    let own = match inner.select.limit.as_ref() {
        None => None,
        Some(BoundExpr::Integer(count)) if *count >= 0 => Some(*count),
        Some(_) => return None,
    };
    if own.is_some_and(|count| count <= bound) {
        return None;
    }
    let mut bounded = inner.clone();
    bounded.select.limit = Some(BoundExpr::Integer(bound));
    Some(bounded)
}

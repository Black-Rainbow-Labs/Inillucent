//! A derived table over a recursion that is read a few rows at a time.
//!
//! Invariant: **the rows handed to the query above are the first rows of the derived table's own
//! answer, in the order that answer has, and no row is handed twice.** The derived table is run
//! with a bound on its rows, the rows past the ones already sent are handed on, and when the query
//! above has not stopped and the bound was reached the table is run again with a larger bound.
//!
//! **Why.** `SELECT term FROM v1 WHERE term > 3 LIMIT 3` over a view of an unbounded recursion
//! never returned: the view is a derived table, which is run to completion before the filter above
//! it sees a row, and the `LIMIT` is above the filter, so it reaches no bound the table could
//! stop at. SQLite runs the view as a coroutine the outer query pulls from. This engine's derived
//! table is a buffer, so the bound grows by four each time, which reads the first rows again and
//! costs a little more than the rows asked for in all.

use inillucent_base::DbResult;
use inillucent_sql::plan::{AccessPath, PhysicalPlan};
use inillucent_tree::datum::OwnedDatum;

use crate::ops::{emit_rows, Flow, Sink};
use crate::physical::{prepare, run_prepared, ForcePlan, Params, TreeCatalog};

/// How many rows the first run of the derived table is asked for.
const FIRST_BOUND: usize = 64;

/// How much larger each run's bound is than the one before it.
const GROWTH: usize = 4;

/// A derived table run in growing bounds, kept so it can be driven when the query above reads it.
///
/// Boxed inside `Source::Growing` for the reason `RecursiveSource` is.
pub struct GrowingSource<'t> {
    /// Where the trees and layouts come from.
    pub catalog: &'t dyn TreeCatalog,
    /// The derived table's plan.
    pub plan: PhysicalPlan,
    /// The values this execution bound.
    pub params: Params,
    /// The affinity each column of the rows is stored with.
    pub affinities: Vec<inillucent_value::Affinity>,
}

impl GrowingSource<'_> {
    /// Hands the derived table's rows to the query above, running it again with a larger bound
    /// while that query keeps reading.
    ///
    /// @param downstream - the query above
    pub(crate) fn run(&self, downstream: &mut dyn Sink) -> DbResult<()> {
        let mut bound = FIRST_BOUND;
        let mut sent = 0usize;
        loop {
            let capped =
                crate::physical::derived_limit::bounded_inner_plan(&self.plan, Some(bound));
            let plan = capped.as_ref().unwrap_or(&self.plan);
            let prepared = prepare(plan, self.catalog, ForcePlan::default())?;
            let rows: Vec<Vec<OwnedDatum>> =
                run_prepared(plan, self.catalog, &prepared, &self.params)?.0;
            let rows = super::joins::store_with_affinity(rows, &self.affinities);
            let fresh = rows.get(sent..).unwrap_or(&[]);
            let flow = emit_rows(fresh, downstream)?;
            sent = rows.len();
            if flow == Flow::Stop || capped.is_none() || rows.len() < bound {
                return Ok(());
            }
            bound = bound.saturating_mul(GROWTH);
        }
    }
}

/// Reports whether a derived table can be run in growing bounds.
///
/// It must be a plain `SELECT` over a recursion: no compound, window, aggregate, `DISTINCT` or
/// `ORDER BY`, which need every row before the first, no subquery shared with another reference,
/// and nothing whose answer changes from one run to the next. Anything else is run once as it
/// always was.
///
/// @param plan - the derived table's plan
pub(crate) fn can_grow(plan: &PhysicalPlan) -> bool {
    let select = &plan.select;
    plan.compounds.is_empty()
        && select.windows.is_empty()
        && select.shared.is_none()
        && !select.distinct
        && select.order_by.is_empty()
        && plan.aggregation == inillucent_sql::plan::AggregationMode::None
        && reads_a_recursion(plan)
        && select
            .columns
            .iter()
            .all(|column| inillucent_sql::plan::is_repeatable_condition(&column.expr))
        && select
            .filter
            .iter()
            .all(inillucent_sql::plan::is_repeatable_condition)
}

/// Reports whether a plan reads a recursive CTE, at any depth of derived tables.
///
/// @param plan - the plan
fn reads_a_recursion(plan: &PhysicalPlan) -> bool {
    plan.sources.iter().any(|source| match &source.path {
        AccessPath::Recursive { .. } => true,
        AccessPath::Subquery { plan, .. } => reads_a_recursion(plan),
        _ => false,
    })
}

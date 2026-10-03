//! Running a recursive CTE to a fixed point.
//!
//! Invariant: **the consumer above a recursion can stop it.** A recursion with
//! no base case is ordinary SQL - every counter and every series generator is
//! written as one and bounded by a `LIMIT` outside it - so a producer that ran
//! to completion before anything above it saw a row would refuse queries
//! SQLite answers. What guards against a recursion that settles neither way is
//! the request's budget, charged per row as the rows are produced and checked
//! between passes; it is not the mechanism that stops an ordinary one.
//!
//! Extracted from `physical.rs`, whose recorded size this pushed past. Nothing
//! here changed in the move.

use inillucent_base::DbResult;
use inillucent_sql::ast::{NullOrder, SortOrder};
use inillucent_sql::bind::BoundExpr;
use inillucent_sql::plan::{PhysicalPlan, RecursiveQueue};
use inillucent_tree::datum::OwnedDatum;
use inillucent_value::collation::Collation;

use crate::ops::{compare_by, SortKey};
use crate::physical::{
    constant_count, prepare_any, run_any, run_any_prepared, Negative, Params, Prepared,
    TreeCatalog, WithQueue,
};
use crate::setop::SetKeys;

/// What a recursive CTE is filled from.
///
/// A struct because the loop needs eight things and every function that takes
/// part in it needs the same ones.
#[derive(Clone, Copy)]
pub(crate) struct Recursion<'a> {
    /// The FROM term whose queue this is.
    pub(crate) cte: usize,
    /// The arms that do not reference the CTE.
    pub(crate) seeds: &'a [(inillucent_sql::ast::CompoundOp, PhysicalPlan)],
    /// The arms that do.
    pub(crate) steps: &'a [(inillucent_sql::ast::CompoundOp, PhysicalPlan)],
    /// How many columns a row holds.
    pub(crate) width: usize,
    /// The CTE's own `ORDER BY`, `LIMIT` and `OFFSET`.
    pub(crate) queue: &'a RecursiveQueue,
    /// Where the trees and layouts come from.
    pub(crate) catalog: &'a dyn TreeCatalog,
    /// The bound parameters.
    pub(crate) params: &'a Params,
}

/// Fills a recursive CTE and returns every row it produced.
///
/// **The seed arms once, then the step arms until a pass produces nothing.**
/// Each pass runs the step arms over the rows the *previous* pass produced -
/// not over every row so far - which is what makes the work proportional to the
/// rows rather than to their square, and it is SQLite's own rule.
///
/// `UNION` de-duplicates against everything already produced and `UNION ALL`
/// does not, which is also the difference between a graph walk that terminates
/// on a cycle and one that does not.
///
/// @param recursion - what the CTE is filled from
/// @param limit - the rows the statement above will keep, when it says
pub(crate) fn run_recursive(
    recursion: &Recursion<'_>,
    limit: Option<usize>,
) -> DbResult<Vec<Vec<OwnedDatum>>> {
    let Recursion { queue, params, .. } = *recursion;
    let own_limit = constant_count(queue.limit.as_ref(), params, Negative::NoLimit)?;
    let own_offset = constant_count(queue.offset.as_ref(), params, Negative::Zero)?.unwrap_or(0);
    // The rows the loop has to produce before nothing more is needed: what the
    // query's own LIMIT keeps after its OFFSET, or what the statement above
    // keeps, whichever is fewer.
    let wanted = [
        own_limit.map(|own| own.saturating_add(own_offset)),
        limit.map(|above| above.saturating_add(own_offset)),
    ]
    .into_iter()
    .flatten()
    .min();
    let produced = if queue.order_by.is_empty() {
        fill_in_passes(recursion, wanted)?
    } else {
        fill_in_order(recursion, wanted)?
    };
    let kept = produced.into_iter().skip(own_offset);
    Ok(match own_limit {
        Some(own) => kept.take(own).collect(),
        None => kept.collect(),
    })
}

/// Fills a recursive CTE whose queue is taken in the order rows were added.
///
/// @param recursion - what the CTE is filled from
/// @param limit - how many rows are needed, counting any offset, when known
fn fill_in_passes(
    recursion: &Recursion<'_>,
    limit: Option<usize>,
) -> DbResult<Vec<Vec<OwnedDatum>>> {
    let Recursion {
        cte,
        seeds,
        steps,
        width,
        catalog,
        params,
        ..
    } = *recursion;
    let distinct = seeds
        .iter()
        .chain(steps.iter())
        .any(|(op, _)| *op == inillucent_sql::ast::CompoundOp::Union);
    let collations = vec![Collation::Binary; width.max(1)];
    let mut answer: Vec<Vec<OwnedDatum>> = Vec::new();
    for (_, arm) in seeds {
        let (rows, _) = run_any(arm, catalog, params)?;
        answer.extend(rows);
    }
    // **One set of the rows a `UNION` has kept, for the whole fill (task-2175).**
    // Each pass used to copy the whole answer and build a new set from it to
    // drop the pass's repeats, so the work grew with the square of the passes.
    let mut seen = SetKeys::new(collations);
    if distinct {
        answer.retain(|row| seen.remember(row));
    }
    // **Each step arm is prepared once, not once a pass (task-2175).** The
    // structural choice reads the trees the arm names and not the queue,
    // which `WithQueue` hands over when the pipeline runs, so it is the same
    // choice on every pass. Preparing it per pass was the whole of the gap
    // on a 10,000 pass counter: 12 ms against SQLite's 1.5 ms.
    let preparing = WithQueue {
        inner: catalog,
        cte,
        rows: &[],
    };
    let mut arms = steps
        .iter()
        .map(|(_, arm)| prepare_any(arm, &preparing).map(StepArm::new))
        .collect::<DbResult<Vec<StepArm>>>()?;
    // The rows the next pass reads are the last ones added to the answer, so
    // they are a range of it rather than a copy.
    let mut working = 0..answer.len();
    // **A recursion with no base case is stopped by the `LIMIT` above it.**
    // `WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM n)
    //  SELECT ... FROM (SELECT x FROM n LIMIT 1000)` is how every counter and
    // every series generator is written, and it never terminates on its own -
    // the step arm always produces a row. This ran it to the million-pass guard
    // and then refused a query SQLite answers, which is the same shape
    // fixed for a virtual table's scan: a producer has to be
    // stoppable by the consumer above it rather than run to completion first.
    let enough = |answer: &Vec<Vec<OwnedDatum>>| limit.is_some_and(|want| answer.len() >= want);
    // **There is no pass count here, and there used to be one.** A million
    // passes refused `WITH RECURSIVE n(x) AS (SELECT 1 UNION ALL SELECT x + 1
    // FROM n) SELECT x FROM n LIMIT 1 OFFSET 2000000`, which SQLite answers in
    // thirteen seconds and which a series generator past a million rows is
    // written as; the audit measured the refusal at seventeen seconds with
    // "a recursive CTE did not settle" (task-2066 section 4.2, item 22). A
    // constant that refuses a query the reference answers is a compatibility
    // defect, and the number it refuses at was not derived from anything.
    //
    // What stops a recursion that settles neither way is the request's own
    // budget, which is a better guard than a pass count on both sides: it
    // counts the bytes the recursion is actually holding rather than how many
    // times round it has been, and it is armed on every served path - the MCP
    // server, the command line and the driver all call `budget::arm`. The
    // `materialise` call below charges each row as it is produced, and the
    // `check` here answers a cancellation and the wall clock limit between
    // passes, so a runaway stops at the first of memory, time or the caller
    // hanging up. A library embedder that armed no budget gets what SQLite
    // gives one, which is a query that runs until it is stopped.
    loop {
        inillucent_base::budget::check()?;
        if working.is_empty() || enough(&answer) {
            return Ok(answer);
        }
        let queued = WithQueue {
            inner: catalog,
            cte,
            rows: answer.get(working.clone()).unwrap_or_default(),
        };
        let mut fresh: Vec<Vec<OwnedDatum>> = Vec::new();
        for ((_, arm), step) in steps.iter().zip(arms.iter_mut()) {
            fresh.extend(step.run(arm, &queued, params)?);
        }
        if distinct {
            fresh.retain(|row| seen.remember(row));
        }
        if fresh.is_empty() {
            return Ok(answer);
        }
        // **The answer accumulates across passes (task-1932, H6).** A
        // recursive CTE holds every row it has produced, and nothing bounds
        // the number of passes, so a walk over a graph with a cycle the
        // `UNION` does not close builds until memory runs out rather than
        // until a budget says stop.
        for row in &fresh {
            inillucent_base::budget::materialise(crate::ops::owned_row_bytes(row))?;
        }
        let start = answer.len();
        answer.extend(fresh);
        if enough(&answer) {
            return Ok(answer);
        }
        working = start..answer.len();
    }
}

/// One recursive arm, prepared once and compiled into a reusable chain when it can be.
///
/// **The chain is built once, not once a pass (task-2175).** Preparing once
/// still left `run_any_prepared` building the operator chain on every pass -
/// translating and compiling the filter and the projection again - which was
/// most of what a pass cost on a 10,000 pass counter. A `Compiled` chain keeps
/// those and builds only its source per run, and the source is what reads the
/// queue through `WithQueue`, so each pass sees its own rows. The rules for
/// trying and keeping one are the engine's own `Slot`: tried on the first
/// pass, kept only when it is reusable, and not used when the settings it was
/// built under have changed.
struct StepArm {
    /// The arm's structural choice.
    prepared: Prepared,
    /// The arm's compiled chain, when it has one that can be run again.
    compiled: Option<crate::compiled::Compiled>,
    /// Whether a compile has been tried.
    tried: bool,
}

impl StepArm {
    /// Returns an arm that has not been compiled yet.
    ///
    /// @param prepared - the arm's structural choice
    fn new(prepared: Prepared) -> StepArm {
        StepArm {
            prepared,
            compiled: None,
            tried: false,
        }
    }

    /// Runs the arm over one pass's queue and returns the rows it produced.
    ///
    /// @param plan - the arm's plan
    /// @param catalog - the catalog with this pass's queue
    /// @param params - the statement's bound parameters
    fn run(
        &mut self,
        plan: &PhysicalPlan,
        catalog: &dyn TreeCatalog,
        params: &Params,
    ) -> DbResult<Vec<Vec<OwnedDatum>>> {
        if !self.tried {
            self.tried = true;
            if let Some(mut compiled) =
                crate::compiled::try_compile(plan, catalog, &self.prepared, params)?
            {
                compiled.run(plan, catalog, params)?;
                let rows = compiled.take_rows();
                if compiled.rebindable() {
                    self.compiled = Some(compiled);
                }
                return Ok(rows);
            }
        }
        if let Some(compiled) = self.compiled.as_mut() {
            if compiled.built_under(params) {
                compiled.run(plan, catalog, params)?;
                return Ok(compiled.take_rows());
            }
        }
        Ok(run_any_prepared(plan, catalog, &self.prepared, params)?.0)
    }
}

/// Fills a recursive CTE whose queue is ordered by the query's own `ORDER BY`.
///
/// **One row at a time, because the order is decided between rows.** SQLite
/// takes the first row of the ordered queue, produces it, runs the recursive
/// arms over that one row and queues what they return, so a row that sorts
/// first jumps ahead of rows queued earlier: `ORDER BY depth DESC` walks a tree
/// depth first and `ORDER BY depth` breadth first. Rows that compare equal
/// leave in the order they were queued. The loop stops as soon as `wanted` rows
/// have been produced, without running the arms over the last one.
///
/// @param recursion - what the CTE is filled from
/// @param wanted - how many rows are needed, counting any offset, when known
fn fill_in_order(
    recursion: &Recursion<'_>,
    wanted: Option<usize>,
) -> DbResult<Vec<Vec<OwnedDatum>>> {
    let Recursion {
        cte,
        seeds,
        steps,
        width,
        queue,
        catalog,
        params,
    } = *recursion;
    let keys = queue_keys(&queue.order_by)?;
    let distinct = seeds
        .iter()
        .chain(steps.iter())
        .any(|(op, _)| *op == inillucent_sql::ast::CompoundOp::Union);
    let mut seen = SetKeys::new(vec![Collation::Binary; width.max(1)]);
    let mut waiting: Vec<Vec<OwnedDatum>> = Vec::new();
    for (_, arm) in seeds {
        let (rows, _) = run_any(arm, catalog, params)?;
        queue_new_rows(rows, distinct, &mut seen, &mut waiting)?;
    }
    // Prepared once, for the reason `fill_in_passes` gives (task-2175).
    let preparing = WithQueue {
        inner: catalog,
        cte,
        rows: &[],
    };
    let prepared = steps
        .iter()
        .map(|(_, arm)| prepare_any(arm, &preparing))
        .collect::<DbResult<Vec<Prepared>>>()?;
    let mut answer: Vec<Vec<OwnedDatum>> = Vec::new();
    let reached = |answer: &Vec<Vec<OwnedDatum>>| wanted.is_some_and(|want| answer.len() >= want);
    while !reached(&answer) {
        inillucent_base::budget::check()?;
        let Some(next) = first_in_order(&waiting, &keys) else {
            break;
        };
        let row = waiting.remove(next);
        answer.push(row.clone());
        if reached(&answer) {
            break;
        }
        let current = vec![row];
        let queued = WithQueue {
            inner: catalog,
            cte,
            rows: &current,
        };
        for ((_, arm), prepared) in steps.iter().zip(&prepared) {
            let (rows, _) = run_any_prepared(arm, &queued, prepared, params)?;
            queue_new_rows(rows, distinct, &mut seen, &mut waiting)?;
        }
    }
    Ok(answer)
}

/// Adds the rows a pass produced to the queue, dropping the ones a `UNION` has
/// seen before.
///
/// @param rows - the rows produced
/// @param distinct - whether a row already queued once is dropped
/// @param seen - every row queued so far
/// @param waiting - the queue
fn queue_new_rows(
    rows: Vec<Vec<OwnedDatum>>,
    distinct: bool,
    seen: &mut SetKeys,
    waiting: &mut Vec<Vec<OwnedDatum>>,
) -> DbResult<()> {
    for row in rows {
        if distinct && !seen.remember(&row) {
            continue;
        }
        inillucent_base::budget::materialise(crate::ops::owned_row_bytes(&row))?;
        waiting.push(row);
    }
    Ok(())
}

/// Returns the position of the row that leaves the queue next.
///
/// The first of the smallest, so rows that compare equal leave in the order
/// they arrived.
///
/// @param waiting - the queue, oldest first
/// @param keys - the ordering
fn first_in_order(waiting: &[Vec<OwnedDatum>], keys: &[SortKey]) -> Option<usize> {
    waiting
        .iter()
        .enumerate()
        .reduce(|best, candidate| {
            if compare_by(candidate.1, best.1, keys) == std::cmp::Ordering::Less {
                candidate
            } else {
                best
            }
        })
        .map(|(position, _)| position)
}

/// Turns the bound `ORDER BY` terms of a recursive query into sort keys.
///
/// @param terms - the terms, each naming a result column
fn queue_keys(terms: &[inillucent_sql::bind::BoundOrderTerm]) -> DbResult<Vec<SortKey>> {
    terms
        .iter()
        .map(|term| {
            let BoundExpr::SorterColumn { column } = &term.expr else {
                return Err(inillucent_base::error::misuse(
                    "a recursive query ordered by an expression",
                ));
            };
            Ok(SortKey {
                column: usize::from(*column),
                descending: term.order == SortOrder::Descending,
                collation: term.collation,
                nulls_first: term.nulls == NullOrder::First,
            })
        })
        .collect()
}

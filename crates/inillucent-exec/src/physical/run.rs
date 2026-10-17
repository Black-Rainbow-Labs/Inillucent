//! Running a prepared statement, and combining the arms of a compound one.
//!
//! Invariant: **a compound statement's arms are run in the order the text
//! gives and combined afterwards.** `UNION` and `EXCEPT` are set operations
//! over the whole of each arm, so an implementation that streamed one into the
//! other would answer differently for a `LIMIT`. Each arm is pushed into the set operation by its
//! own pipeline, so an arm is never held as a vector of rows; what the set operation holds is its
//! own business, and it sorts in runs when that is too much (`crate::setop`).

use inillucent_base::DbResult;
// `literal_value` is named by path from a dozen call sites in
// `inillucent-engine`, so it stays reachable here after the move to `constant`.
use crate::constant::constant_value;
// `Compiled`, `Slot` and `try_compile` moved to `crate::compiled` to keep this
// file under its recorded ceiling; re-exported here so every existing
// `physical::Slot` / `physical::Compiled` / `physical::try_compile` reference
// - `inillucent-engine`'s `Cached::Select` among them - did not have to move
// with them.
use inillucent_pool::Pool;
use inillucent_sql::ast::{NullOrder, SortOrder};
use inillucent_sql::bind::{BoundExpr, BoundSelect};
use inillucent_sql::plan::{AccessPath, PhysicalPlan};
use inillucent_tree::datum::OwnedDatum;
use inillucent_tree::PagedTree;
use inillucent_value::affinity::Affinity;
use inillucent_value::collation::Collation;

use crate::expr::Expr;
use crate::gather::{Gathered, Gathering};

use crate::ops::{CollectInto, Limit, Sink, Skip, Sort, SortKey};
use crate::setop::{SetKind, SetOp};

/// A catalog that also answers one recursive CTE's queue.
///
/// Everything else is delegated, so the step arm sees exactly the trees, the
/// layouts and the modules the statement sees. Wrapping rather than threading a
/// parameter through every builder is what keeps a recursive query from
/// changing the shape of a signature nothing else uses.
use super::*;

/// Builds a plan and runs it, returning the rows.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param params - the values bound to `?1`, `?2`, ...
pub fn run(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    params: &Params,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    let prepared = prepare(plan, catalog, ForcePlan::default())?;
    run_prepared(plan, catalog, &prepared, params)
}
/// Runs an already-prepared statement and returns the rows.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param prepared - the structural choices [`prepare`] made
/// @param params - the values bound to `?1`, `?2`, ...
pub fn run_prepared(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    prepared: &Prepared,
    params: &Params,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    run_prepared_limited(plan, catalog, prepared, params, None)
}

thread_local! {
    /// The rows the last pipeline to fail had already delivered.
    static PARTIAL_ROWS: std::cell::RefCell<Vec<Vec<OwnedDatum>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Keeps the rows a failed pipeline had delivered, replacing any kept before.
///
/// The outermost pipeline fails last, so its rows are the ones left.
///
/// @param rows - what the failed pipeline's sink held
pub fn keep_partial_rows(rows: Vec<Vec<OwnedDatum>>) {
    PARTIAL_ROWS.with(|held| *held.borrow_mut() = rows);
}

/// Takes the rows the last failed pipeline had delivered before it failed.
pub fn take_partial_rows() -> Vec<Vec<OwnedDatum>> {
    PARTIAL_ROWS.with(|held| std::mem::take(&mut *held.borrow_mut()))
}

/// Runs a prepared pipeline, stopping after `limit` rows when one is given.
///
/// **For a caller that needs one row and not the answer** (task-2066 §4.3.1).
/// A correlated `EXISTS` asks whether the inner query produces anything and an
/// unlimited run reads every row of it to answer that; a scalar block takes
/// the first row and drops the rest. `CollectInto::with_limit` already answers
/// `Flow::Stop`, so the pipeline stops rather than the rows being thrown away
/// afterwards.
///
/// A block's own `ORDER BY` is inside the pipeline, so the limit stops after
/// the sort has emitted its first row - which is the row the unlimited path
/// would have picked.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param prepared - the structural choice
/// @param params - the values bound to `?1`, `?2`, ...
/// @param limit - how many rows to keep, or `None` for all of them
pub fn run_prepared_limited(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    prepared: &Prepared,
    params: &Params,
    limit: Option<usize>,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    let rows = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink: Box<CollectInto> = match limit {
        Some(limit) => Box::new(CollectInto::with_limit(std::rc::Rc::clone(&rows), limit)),
        None => Box::new(CollectInto::new(std::rc::Rc::clone(&rows))),
    };
    let (mut pipeline, shape) = build_prepared(plan, catalog, prepared, params, sink)?;
    if let Err(error) = pipeline.run() {
        // The rows delivered before the failure are kept for whoever reports it:
        // SQLite hands out the rows of `SELECT ... ` that came before the row
        // that raised the error, and a caller that wants them asks for them.
        keep_partial_rows(std::mem::take(&mut *rows.borrow_mut()));
        return Err(error);
    }
    // **Taken, not cloned.** The sink is dropped with the pipeline and nothing
    // reads the buffer again, so cloning it copied every row of every answer to
    // hand back a second copy of what was about to be freed. On a one-row answer
    // that is two allocations of the twenty-two an already-prepared `SELECT 1`
    // makes; on a scan it is the whole result set, twice.
    let collected = std::mem::take(&mut *rows.borrow_mut());
    Ok((collected, shape))
}
/// Runs any planned query with every row handed to a consumer as it is produced.
///
/// **A query that is one pipeline streams; a compound or a windowed query does not.** Neither is
/// one pipeline (see [`run_any_prepared_limited`]), so their rows are collected and then handed
/// over, which is what they cost before and is not worse. The consumer is returned whatever the
/// outcome, with the result of the run beside it.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param prepared - the structural choice
/// @param params - the values bound to `?1`, `?2`, ...
/// @param consumer - what takes each row
pub fn run_any_prepared_streaming(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    prepared: &Prepared,
    params: &Params,
    mut consumer: Box<dyn crate::ops::RowConsumer>,
) -> (DbResult<()>, Box<dyn crate::ops::RowConsumer>) {
    if !plan.compounds.is_empty() || !plan.select.windows.is_empty() {
        let handed = match run_any_prepared(plan, catalog, prepared, params) {
            Ok((rows, _)) => crate::ops::hand_over(&rows, consumer.as_mut()),
            // The rows a failing query had produced come out before the failure, as they do
            // when the rows are collected and the statement is stepped.
            Err(error) => {
                let delivered = take_partial_rows();
                match crate::ops::hand_over(&delivered, consumer.as_mut()) {
                    Ok(()) => Err(error),
                    Err(refused) => Err(refused),
                }
            }
        };
        return (handed, consumer);
    }
    let rows = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let cell: crate::ops::Streaming = std::rc::Rc::new(std::cell::RefCell::new(Some(consumer)));
    let sink = Box::new(CollectInto::new(rows).with_streaming(std::rc::Rc::clone(&cell)));
    let ran = build_prepared(plan, catalog, prepared, params, sink)
        .and_then(|(mut pipeline, _)| pipeline.run());
    let consumer = cell.borrow_mut().take();
    match consumer {
        Some(consumer) => (ran, consumer),
        None => (
            Err(inillucent_base::error::misuse(
                "a streamed statement lost its consumer",
            )),
            Box::new(crate::ops::NoConsumer),
        ),
    }
}
/// Runs a compound query: two or more arms joined by a set operator.
///
/// **Each arm is an ordinary plan and is run as one.** The set operation is
/// [`crate::setop::SetOp`], the same operator a hand-built pipeline would use,
/// so there is one implementation of what `EXCEPT` means rather than two. What
/// this function adds is the plumbing the push executor cannot express on its
/// own: several sources feeding one sink, and - for `EXCEPT` and `INTERSECT` -
/// the right arm having to be complete before the left arm's first row can be
/// judged.
///
/// The arms are materialised between steps. A compound is a pipeline breaker in
/// every engine that has one, because three of the four operators are set
/// operations over whole rows and a set operation cannot stream; `UNION ALL`
/// could, and running it the same way costs a buffer and keeps the four arms of
/// this function from being four different shapes.
///
/// ## Where the ORDER BY lives
///
/// On the **head** arm's `select`, not on the compound. That is the binder's
/// doing and it is right - SQL does not let an arm of a compound carry its own
/// `ORDER BY`, so the one that is written belongs to the whole - but it means
/// the head arm has to be run with its ordering and its limit *removed*, or
/// `SELECT a FROM t UNION SELECT b FROM u LIMIT 3` would take three rows from
/// the first arm and then union them.
///
/// @param plan - the head arm, carrying the rest in `compounds`
/// @param catalog - where the trees and layouts come from
/// @param params - the values bound to `?1`, `?2`, ...
pub fn run_compound(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    params: &Params,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    let rows = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = Box::new(CollectInto::new(std::rc::Rc::clone(&rows)));
    let shape = run_compound_into(plan, catalog, params, sink)?;
    let collected = std::mem::take(&mut *rows.borrow_mut());
    Ok((collected, shape))
}
/// Runs a compound query into a sink.
///
/// The body of [`run_compound`]: the answer is pushed to `sink` after the compound's own `ORDER
/// BY`, `LIMIT` and `OFFSET`, so a caller that writes the rows somewhere does not hold them.
///
/// @param plan - the head arm, carrying the rest in `compounds`
/// @param catalog - where the trees and layouts come from
/// @param params - the values bound to `?1`, `?2`, ...
/// @param sink - where the answer goes; it is finished when the answer ends
pub fn run_compound_into(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    params: &Params,
    sink: Box<dyn Sink>,
) -> DbResult<Shape> {
    // Every uncorrelated subquery is answered once, here, before anything is
    // built over it. See `crate::subquery` for why it is per execution.
    let folded = crate::subquery::fold(plan, catalog, params)?;
    let params = folded.as_ref().unwrap_or(params);
    let collations = compound_collations(plan);
    let spill = catalog.spill();
    let streams = compound_streams(plan);
    let budget = compound_skip(plan, params, streams)?;
    let skipped = budget.is_some();
    let gathering = new_gathering(&spill);
    let shape = match run_arm_into(
        plan,
        catalog,
        params,
        collecting(&gathering),
        budget.as_ref(),
    ) {
        Ok(shape) => shape,
        Err(error) => {
            stash_partial(&gathering);
            return compound_failure_into(plan, params, (streams, skipped), error, sink);
        }
    };
    let mut rows = take_gathered(&gathering)?;
    for (op, arm) in &plan.compounds {
        if !arm.compounds.is_empty() {
            // The binder flattens a chain of compounds onto the head, so an arm
            // carrying its own is a shape this has never been handed. Refusing
            // is what the physical pass does with a shape it has not seen.
            return unsupported("a compound query nested inside a compound arm");
        }
        let fold = Folding {
            kind: kind_of(*op),
            collations: &collations,
            budget: budget.as_ref(),
        };
        rows = match combine(&fold, rows, arm, catalog, params) {
            Ok(combined) => combined,
            Err(error) => {
                return compound_failure_into(plan, params, (streams, skipped), error, sink)
            }
        };
    }
    let mut tail = compound_tail(&plan.select, params, catalog.spill(), sink, skipped)?;
    feed_rows(rows, tail.as_mut())?;
    tail.finish()?;
    Ok(shape)
}
/// Says whether a compound hands its rows out as each arm produces them.
///
/// **Only `UNION ALL` with no `ORDER BY` does.** SQLite sends the rows of each
/// arm of such a compound to the caller as they are found, so an arm that fails
/// has already returned the rows before it. `UNION`, `EXCEPT` and `INTERSECT`
/// build a table of the whole answer first, and a sorted compound sorts it first,
/// so a failure there returns no rows at all.
///
/// @param plan - the head arm, carrying the rest in `compounds`
fn compound_streams(plan: &PhysicalPlan) -> bool {
    plan.select.order_by.is_empty()
        && plan
            .compounds
            .iter()
            .all(|(op, _)| *op == inillucent_sql::ast::CompoundOp::UnionAll)
}
/// Returns the count of rows a compound's arms skip before their projections, when they do.
///
/// **SQLite does not evaluate the select list of a row a `UNION ALL` skips.** It hands the
/// compound's `OFFSET` to each arm, and an arm counts a row off before it computes the result
/// columns, so in `SELECT a FROM t UNION ALL SELECT 1 / 0 FROM u LIMIT 1 OFFSET 5` a skipped row
/// of the second arm never divides. The compound used to apply its `OFFSET` to the rows the arms
/// had already projected, so a skipped row's expression ran and could fail the query. Only a
/// compound that streams has this order; `UNION`, `INTERSECT`, `EXCEPT` and a sorted compound
/// count the offset over the finished answer and keep applying it there.
///
/// @param plan - the head arm, carrying the rest in `compounds`
/// @param params - the bound parameters
/// @param streams - whether the compound hands out rows as each arm produces them
fn compound_skip(
    plan: &PhysicalPlan,
    params: &Params,
    streams: bool,
) -> DbResult<Option<crate::ops::SkipBudget>> {
    if !streams {
        return Ok(None);
    }
    let offset = constant_count(plan.select.offset.as_ref(), params, Negative::Zero)?;
    Ok(offset
        .filter(|offset| *offset > 0)
        .map(|offset| std::rc::Rc::new(std::cell::Cell::new(offset))))
}
/// Reports an arm of a compound that failed, with the rows SQLite would have returned first.
///
/// For a compound that streams, those are the rows of the arms before the one
/// that failed and the rows that arm produced before it failed, with the
/// compound's `OFFSET` and `LIMIT` applied. When the `LIMIT` is already met by
/// them, SQLite never reaches the failing row and the answer is those rows.
/// Otherwise they are kept for [`take_partial_rows`] and the failure is returned.
///
/// @param plan - the head arm, carrying the rest in `compounds`
/// @param params - the bound parameters
/// @param streaming - whether the compound hands out rows as each arm produces them, and whether
///   the arms already threw the `OFFSET` rows away
/// @param earlier - the rows of the arms that finished
/// @param error - what the failing arm said
fn compound_failure(
    plan: &PhysicalPlan,
    params: &Params,
    (streams, skipped): (bool, bool),
    mut earlier: Vec<Vec<OwnedDatum>>,
    error: inillucent_base::DbError,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    let tail = take_partial_rows();
    if !streams {
        keep_partial_rows(Vec::new());
        return Err(error);
    }
    earlier.extend(tail);
    let wanted = constant_count(plan.select.limit.as_ref(), params, Negative::NoLimit)?;
    let offset = match skipped {
        true => None,
        false => constant_count(plan.select.offset.as_ref(), params, Negative::Zero)?,
    };
    let enough =
        wanted.is_some_and(|wanted| earlier.len() >= offset.unwrap_or(0).saturating_add(wanted));
    let kept = order_compound(&plan.select, earlier, params, skipped)?;
    if enough {
        let names = plan.select.columns.iter().map(|column| column.name.clone());
        return Ok((
            kept,
            Shape {
                names: names.collect(),
                operators: Vec::new(),
            },
        ));
    }
    keep_partial_rows(kept);
    Err(error)
}
/// Returns the collation each result column of a compound is compared under.
///
/// The leftmost arm whose column has a collation of its own decides, as in
/// SQLite's `multiSelectCollSeq`, so an explicit `COLLATE` on a later arm counts
/// when the earlier arms have only literals in that column.
///
/// @param plan - the head arm, carrying the rest in `compounds`
fn compound_collations(plan: &PhysicalPlan) -> Vec<Collation> {
    (0..plan.select.columns.len())
        .map(|at| {
            let arms = std::iter::once(&plan.select)
                .chain(plan.compounds.iter().map(|(_, arm)| &arm.select))
                .filter_map(|select| select.columns.get(at))
                .map(|column| &column.expr);
            inillucent_sql::bind::compound_collation(arms)
        })
        .collect()
}
/// Runs one arm of a compound into a sink, without the compound's own ordering or limit.
///
/// @param plan - the arm
/// @param catalog - where the trees and layouts come from
/// @param params - the bound parameters
/// @param sink - where the arm's rows go; it is finished when the arm ends
/// @param budget - the compound's `OFFSET` still to skip, when the arms skip it themselves
fn run_arm_into(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    params: &Params,
    sink: Box<dyn Sink>,
    budget: Option<&crate::ops::SkipBudget>,
) -> DbResult<Shape> {
    let mut arm = plan.clone();
    arm.compounds.clear();
    arm.select.order_by.clear();
    arm.select.limit = None;
    arm.select.offset = None;
    arm.needs_sort = false;
    arm.reverse = false;
    if !arm.select.windows.is_empty() {
        // A window sees every row of its partition, so the rows skipped are the ones it answers.
        let sink: Box<dyn Sink> = match budget {
            Some(budget) => Box::new(Skip::shared(std::rc::Rc::clone(budget), sink)),
            None => sink,
        };
        return crate::windowpass::run_windowed_into(&arm, catalog, params, sink);
    }
    let mut prepared = prepare(&arm, catalog, ForcePlan::default())?;
    prepared.skip = budget.cloned();
    let (mut pipeline, shape) = build_prepared(&arm, catalog, &prepared, params, sink)?;
    pipeline.run()?;
    Ok(shape)
}
/// Returns a collection that holds an arm's rows, moving them to a file when there are too many.
///
/// @param spill - where files come from, when the connection offers them
fn new_gathering(
    spill: &Option<std::rc::Rc<dyn crate::spill::Spill>>,
) -> std::rc::Rc<std::cell::RefCell<Option<Gathering>>> {
    std::rc::Rc::new(std::cell::RefCell::new(Some(Gathering::rows(
        spill.clone(),
    ))))
}
/// Returns a sink that collects into a collection.
///
/// @param gathering - the collection
fn collecting(gathering: &std::rc::Rc<std::cell::RefCell<Option<Gathering>>>) -> Box<dyn Sink> {
    let unused = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    Box::new(CollectInto::new(unused).with_gathering(std::rc::Rc::clone(gathering)))
}
/// Ends a collection and returns what it holds.
///
/// @param gathering - the collection
fn take_gathered(
    gathering: &std::rc::Rc<std::cell::RefCell<Option<Gathering>>>,
) -> DbResult<Gathered> {
    match gathering.borrow_mut().take() {
        Some(held) => held.finish(),
        None => Ok(Gathered::Rows(Vec::new())),
    }
}
/// A sink that pushes into a set operation shared by several arms and never ends it.
///
/// Each arm of a compound is a pipeline of its own and a pipeline finishes its sink when it ends.
/// The set operation must see both arms before it finishes, so each arm gets one of these, and the
/// set operation is finished once, by the caller, when the last arm is done.
struct ArmSink {
    /// The set operation every arm pushes into.
    shared: std::rc::Rc<std::cell::RefCell<SetOp>>,
}
impl Sink for ArmSink {
    fn push(&mut self, batch: &crate::batch::Batch<'_>) -> DbResult<crate::ops::Flow> {
        self.shared.borrow_mut().push(batch)
    }

    fn finish(&mut self) -> DbResult<()> {
        Ok(())
    }

    fn reset(&mut self) -> DbResult<()> {
        Ok(())
    }
}
/// Runs any planned query, whichever of the three shapes it is.
///
/// The one entry point that knows a compound is several plans and a window
/// query is a plan with a pass on top. Every caller that just wants an answer
/// goes through here; `prepare` and `run_prepared` stay the single-pipeline
/// pair they were, because a `Prepared` is the structural choice for *one*
/// pipeline and neither of the other two shapes has only one.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param params - the values bound to `?1`, `?2`, ...
pub fn run_any(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    params: &Params,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    let prepared = prepare_any(plan, catalog)?;
    run_any_prepared(plan, catalog, &prepared, params)
}
/// Returns the one key a plan looks up, when the whole plan is a rowid seek.
///
/// **A write whose `WHERE` is a rowid equality does not need a pipeline to find
/// one row.** `UPDATE t SET ... WHERE id = ?1` plans to a single-stage
/// `RowidSeek`, and running it as a query builds a source, a projection and a
/// collecting sink to hand back one integer the plan already contains. The write
/// gate measured that at 0.89 us of a 1.85 us update, and 17 us of a 47 us one -
/// about half of each, spent deciding something already decided.
///
/// `None` for every other shape, and the caller runs the query. The conditions
/// are all of them: one FROM term, a rowid seek, no residual predicate, no
/// constant filter, no limit and no offset. A plan with any of those does more
/// than name a row, and answering it from the key alone would be answering a
/// different question.
///
/// @param plan - the planner's output
/// @param params - the values bound to `?1`, `?2`, ...
pub fn rowid_seek_key(plan: &PhysicalPlan, params: &Params) -> DbResult<Option<OwnedDatum>> {
    if plan.sources.len() != 1
        || plan.constant_filter.is_some()
        || plan.select.limit.is_some()
        || plan.select.offset.is_some()
        || !plan.compounds.is_empty()
        || plan.residuals.iter().any(Option::is_some)
    {
        return Ok(None);
    }
    let Some(source) = plan.sources.first() else {
        return Ok(None);
    };
    let AccessPath::RowidSeek { key, .. } = &source.path else {
        return Ok(None);
    };
    let held = HeldSpace {
        layouts: Vec::new(),
        types: Vec::new(),
        order: Vec::new(),
    };
    let space = held.view(&[]);
    // Integer affinity, because that is what a rowid comparison applies -
    // `WHERE id = '4'` finds row 4 - and the seek path already applies it. A
    // shortcut that skipped it would answer a question the pipeline would not.
    match constant_value(key, &space, params, Some(Affinity::Integer)) {
        Ok(value) => Ok(Some(value)),
        // The key reads a column, which a rowid seek's should not; the pipeline
        // is the honest answer rather than a guess about what it meant.
        Err(_) => Ok(None),
    }
}
/// Makes the structural choice for any planned query, once.
///
/// **Everything that does not depend on the bound parameters belongs here**, so
/// that a caller re-running a statement pays for the parameters and the work and
/// not for the decision. A compound and a windowed query have no single set of
/// stages - a compound has one per arm, and a window pass plans an inner query
/// of its own - so they get an empty `Prepared` and are re-decided per
/// execution. That is honest rather than tidy: the alternative is a `Prepared`
/// that describes one of several pipelines and is silently wrong about the rest.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
pub fn prepare_any(plan: &PhysicalPlan, catalog: &dyn TreeCatalog) -> DbResult<Prepared> {
    if !plan.compounds.is_empty() || !plan.select.windows.is_empty() {
        return Ok(Prepared {
            stages: Vec::new(),
            forced: ForcePlan::default(),
            blocks: Default::default(),
            skip: None,
        });
    }
    prepare(plan, catalog, ForcePlan::default())
}
/// Runs any planned query against a choice [`prepare_any`] already made.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param prepared - the structural choice
/// @param params - the values bound to `?1`, `?2`, ...
pub fn run_any_prepared(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    prepared: &Prepared,
    params: &Params,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    run_any_prepared_limited(plan, catalog, prepared, params, None)
}

/// Runs any planned query against a prepared choice, stopping after `limit` rows.
///
/// A compound and a windowed query take the unlimited path: neither is one
/// pipeline, so there is no single sink to put the limit in, and the caller
/// that asks for a limit here - a correlated block - reads the first row
/// either way.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param prepared - the structural choice
/// @param params - the values bound to `?1`, `?2`, ...
/// @param limit - how many rows to keep, or `None` for all of them
pub fn run_any_prepared_limited(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    prepared: &Prepared,
    params: &Params,
    limit: Option<usize>,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    if !plan.compounds.is_empty() {
        return run_compound(plan, catalog, params);
    }
    if !plan.select.windows.is_empty() {
        return crate::windowpass::run_windowed(plan, catalog, params);
    }
    run_prepared_limited(plan, catalog, prepared, params, limit)
}
/// Runs any planned query into a sink, whichever of the three shapes it is.
///
/// **For a caller that writes the rows somewhere and has no use for them as a vector**: an
/// `INSERT ... SELECT` source. A compound and a windowed query push their answers as they produce
/// them, so none of the three holds the answer.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param prepared - the structural choice
/// @param params - the values bound to `?1`, `?2`, ...
/// @param sink - where the answer goes; it is finished when the answer ends
pub fn run_any_into(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    prepared: &Prepared,
    params: &Params,
    sink: Box<dyn Sink>,
) -> DbResult<Shape> {
    if !plan.compounds.is_empty() {
        return run_compound_into(plan, catalog, params, sink);
    }
    if !plan.select.windows.is_empty() {
        return crate::windowpass::run_windowed_into(plan, catalog, params, sink);
    }
    let (mut pipeline, shape) = build_prepared(plan, catalog, prepared, params, sink)?;
    pipeline.run()?;
    Ok(shape)
}
/// Runs any planned query and collects its answer, in memory or in a file when it is large.
///
/// For the source of an `INSERT ... SELECT` that has no compiled chain: a compound, a window, or a
/// query whose chain is in use.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param prepared - the structural choice
/// @param params - the values bound to `?1`, `?2`, ...
/// @param packed - whether to pack the rows, for the bulk build, or keep one vector a row
pub fn run_any_gathered(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    prepared: &Prepared,
    params: &Params,
    packed: bool,
) -> DbResult<Gathered> {
    let spill = catalog.spill();
    let gathering = std::rc::Rc::new(std::cell::RefCell::new(Some(if packed {
        Gathering::packed(spill)
    } else {
        Gathering::rows(spill)
    })));
    run_any_into(plan, catalog, prepared, params, collecting(&gathering))?;
    take_gathered(&gathering)
}
/// Returns the set operation a compound operator names.
///
/// @param op - the binder's operator
fn kind_of(op: inillucent_sql::ast::CompoundOp) -> SetKind {
    match op {
        inillucent_sql::ast::CompoundOp::Union => SetKind::Union,
        inillucent_sql::ast::CompoundOp::UnionAll => SetKind::UnionAll,
        inillucent_sql::ast::CompoundOp::Except => SetKind::Except,
        inillucent_sql::ast::CompoundOp::Intersect => SetKind::Intersect,
    }
}
/// What one fold of a compound needs besides its rows: the operator, its collations and its skip.
struct Folding<'a> {
    /// Which of the four set operations.
    kind: SetKind,
    /// The collation of each result column.
    collations: &'a [Collation],
    /// The compound's `OFFSET` still to skip, when the arms skip it themselves.
    budget: Option<&'a crate::ops::SkipBudget>,
}
/// Applies one set operation to the rows so far and the next arm.
///
/// **The next arm is run into the set operation, not collected first.** The rows so far are read
/// from where they are held and pushed in, the arm's pipeline pushes the rest, and the answer is
/// collected, in memory or in a file when it is large. For `EXCEPT` and `INTERSECT` the right arm
/// goes first, because a left row cannot be judged until the whole right arm is known.
///
/// @param fold - the operator, the collations and the skip
/// @param left - the rows so far
/// @param arm - the arm being folded in
/// @param catalog - where the trees and layouts come from
/// @param params - the bound parameters
fn combine(
    fold: &Folding<'_>,
    left: Gathered,
    arm: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    params: &Params,
) -> DbResult<Gathered> {
    let (kind, collations) = (fold.kind, fold.collations);
    let spill = catalog.spill();
    let gathering = new_gathering(&spill);
    let operation = std::rc::Rc::new(std::cell::RefCell::new(
        SetOp::new(kind, collations.to_vec(), collecting(&gathering)).spilling_to(spill),
    ));
    let arm_sink = || -> Box<dyn Sink> {
        Box::new(ArmSink {
            shared: std::rc::Rc::clone(&operation),
        })
    };
    if kind.needs_right_first() {
        run_arm_into(arm, catalog, params, arm_sink(), fold.budget)
            .inspect_err(|_| stash_partial(&gathering))?;
        operation.borrow_mut().next_arm();
        feed_rows(left, &mut *operation.borrow_mut())?;
    } else {
        feed_rows(left, &mut *operation.borrow_mut())?;
        operation.borrow_mut().next_arm();
        run_arm_into(arm, catalog, params, arm_sink(), fold.budget)
            .inspect_err(|_| stash_partial(&gathering))?;
    }
    operation.borrow_mut().finish()?;
    take_gathered(&gathering)
}
/// Keeps the rows a collection holds for [`take_partial_rows`], after the arm feeding it failed.
///
/// A failing arm of a compound that streams has delivered the rows before the failure, and SQLite
/// returns them ahead of the error. They are read back whole, which only a failure pays for.
///
/// @param gathering - the collection the failed run was filling
fn stash_partial(gathering: &std::rc::Rc<std::cell::RefCell<Option<Gathering>>>) {
    let held = take_gathered(gathering).and_then(Gathered::into_rows);
    keep_partial_rows(held.unwrap_or_default());
}
/// Reports an arm of a compound that failed, into a sink, with the rows SQLite returns first.
///
/// The rows the arms delivered are in [`take_partial_rows`] when this is called; see
/// [`compound_failure`] for what is done with them. When the compound's `LIMIT` is met by
/// them the answer is those rows and no failure.
///
/// @param plan - the head arm, carrying the rest in `compounds`
/// @param params - the bound parameters
/// @param streaming - whether the compound hands out rows as each arm produces them, and whether
///   the arms already threw the `OFFSET` rows away
/// @param error - what the failing arm said
/// @param sink - where the answer goes
fn compound_failure_into(
    plan: &PhysicalPlan,
    params: &Params,
    streaming: (bool, bool),
    error: inillucent_base::DbError,
    mut sink: Box<dyn Sink>,
) -> DbResult<Shape> {
    let (rows, shape) = compound_failure(plan, params, streaming, Vec::new(), error)?;
    crate::ops::emit_rows(&rows, sink.as_mut())?;
    sink.finish()?;
    Ok(shape)
}
/// Pushes rows that were collected earlier into a sink, without finishing it.
///
/// @param rows - the rows
/// @param sink - where they go
fn feed_rows(rows: Gathered, sink: &mut dyn Sink) -> DbResult<()> {
    match rows {
        Gathered::Rows(rows) => {
            crate::ops::emit_rows(&rows, sink)?;
        }
        Gathered::Packed(packed) => {
            crate::ops::emit_rows(&packed.into_rows(), sink)?;
        }
        Gathered::Spooled(spooled) => {
            let mut batch: Vec<Vec<OwnedDatum>> = Vec::with_capacity(crate::batch::BATCH_ROWS);
            for row in spooled {
                batch.push(row?);
                if batch.len() >= crate::batch::BATCH_ROWS {
                    let flow = crate::ops::emit_rows(&batch, sink)?;
                    batch.clear();
                    if flow == crate::ops::Flow::Stop {
                        return Ok(());
                    }
                }
            }
            if !batch.is_empty() {
                crate::ops::emit_rows(&batch, sink)?;
            }
        }
    }
    Ok(())
}
/// Applies a compound's own `ORDER BY`, `LIMIT` and `OFFSET` to rows already in hand.
///
/// @param select - the head arm's bound statement, which carries them
/// @param rows - the rows
/// @param params - the bound parameters
/// @param skipped - whether the arms already threw the `OFFSET` rows away
fn order_compound(
    select: &BoundSelect,
    rows: Vec<Vec<OwnedDatum>>,
    params: &Params,
    skipped: bool,
) -> DbResult<Vec<Vec<OwnedDatum>>> {
    let collected = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = Box::new(CollectInto::new(std::rc::Rc::clone(&collected)));
    let mut tail = compound_tail(select, params, None, sink, skipped)?;
    feed_rows(Gathered::Rows(rows), tail.as_mut())?;
    tail.finish()?;
    let answer = std::mem::take(&mut *collected.borrow_mut());
    Ok(answer)
}
/// Returns the operators that apply a compound's own `ORDER BY`, `LIMIT` and `OFFSET` to its rows.
///
/// @param select - the head arm's bound statement, which carries them
/// @param params - the bound parameters
/// @param catalog - where a sort that does not fit in memory writes its runs
/// @param sink - where the ordered and limited rows go
/// @param skipped - whether the arms already threw the `OFFSET` rows away
fn compound_tail(
    select: &BoundSelect,
    params: &Params,
    spill: Option<std::rc::Rc<dyn crate::spill::Spill>>,
    collect: Box<dyn Sink>,
    skipped: bool,
) -> DbResult<Box<dyn Sink>> {
    let offset_left = select.offset.is_some() && !skipped;
    if select.order_by.is_empty() && select.limit.is_none() && !offset_left {
        return Ok(collect);
    }
    let mut keys = Vec::with_capacity(select.order_by.len());
    for term in &select.order_by {
        // A compound's ordering terms name *output* columns - SQL does not let
        // one reach into an arm's FROM clause - so the binder has already
        // resolved each to an ordinal. Anything else is a shape this cannot
        // order and is refused rather than approximated.
        let BoundExpr::SorterColumn { column } = &term.expr else {
            return unsupported("a compound query ordered by an expression");
        };
        let descending = term.order == SortOrder::Descending;
        keys.push(SortKey {
            column: usize::from(*column),
            descending,
            // **The collation the term resolved to, not the result column's
            // (task-1979, F15).** Reading it off the column threw away a
            // `COLLATE` written on the term, so
            // `SELECT 'B' AS a UNION SELECT 'a' ORDER BY a COLLATE NOCASE`
            // sorted `B` before `a` where SQLite sorts `a` first. The binder
            // has already chosen between the two: the name on the term when
            // there is one, and the result column's own collation when there is
            // not, which is the value this line used to read.
            collation: term.collation,
            // The binder has already resolved the default, which is NULLS
            // FIRST ascending and NULLS LAST descending.
            nulls_first: match term.nulls {
                NullOrder::First => true,
                NullOrder::Last => false,
            },
        });
    }
    let limit = constant_count(select.limit.as_ref(), params, Negative::NoLimit)?;
    let offset = match offset_left {
        true => constant_count(select.offset.as_ref(), params, Negative::Zero)?,
        false => None,
    };
    let sink: Box<dyn Sink> = match (limit, offset) {
        (None, None) => collect,
        (limit, offset) => Box::new(Limit::new(
            limit.unwrap_or(usize::MAX),
            offset.unwrap_or(0),
            collect,
        )),
    };
    let head: Box<dyn Sink> = if keys.is_empty() {
        sink
    } else {
        Box::new(Sort::new(keys, sink).spilling_to(spill))
    };
    Ok(head)
}
/// Returns the collation each result column is compared under by `DISTINCT`.
///
/// @param select - the bound statement
pub(crate) fn distinct_collations(select: &BoundSelect) -> Vec<Collation> {
    select
        .columns
        .iter()
        .map(|column| inillucent_sql::bind::result_collation(&column.expr))
        .collect()
}
/// Translates a bound expression that reads the scan's columns.
///
/// @param expr - the bound expression
/// @param space - the joined column space
/// @param params - the bound parameters
/// Returns the expression `sqlite_offset(X)` compiles to.
///
/// The argument has to be a column of a table the statement reads, which is
/// SQLite's own rule - anything else answers NULL there and answers NULL here.
/// What is compiled is a lookup of the row's key in a table of leaf boundaries:
/// one entry per leaf, holding the lowest key on it and the offset of its page
/// in the file. The boundaries are read once, while the statement is being
/// prepared, and the cost is one page read per leaf rather than one per row.
///
/// @param arguments - the call's single argument
/// @param space - the joined column space
/// @param params - the statement's bound parameters
/// @param frame - which of the two row shapes the expression is over
pub(crate) fn row_offset(
    arguments: &[BoundExpr],
    space: &Space<'_>,
    params: &Params,
    frame: Frame<'_>,
) -> DbResult<Expr> {
    let Some(BoundExpr::Column { source, .. }) = arguments.first() else {
        return Ok(Expr::Literal(OwnedDatum::Null));
    };
    let source = *source;
    let Some(catalog) = space.catalog else {
        return Ok(Expr::Literal(OwnedDatum::Null));
    };
    let Some(stage) = space.stages.iter().find(|stage| stage.term == source) else {
        return Ok(Expr::Literal(OwnedDatum::Null));
    };
    let Some(layout) = space.layouts.get(source) else {
        return Ok(Expr::Literal(OwnedDatum::Null));
    };
    let Some(rowid_column) = layout.rowid else {
        return Ok(Expr::Literal(OwnedDatum::Null));
    };
    let (Some(tree), Some(pool)) = (catalog.tree(stage.root), catalog.pool_for(stage.root)) else {
        return Ok(Expr::Literal(OwnedDatum::Null));
    };
    let boundaries = leaf_boundaries(tree, pool, rowid_column)?;
    let rowid = translate(&BoundExpr::Rowid { source }, space, params, frame)?;
    Ok(Expr::RowOffset {
        boundaries: std::sync::Arc::new(boundaries),
        rowid: Box::new(rowid),
    })
}
/// Returns the lowest key on each leaf and where that leaf sits in the file.
///
/// Sorted by key, so a lookup is a binary search. A leaf covers a contiguous
/// run of keys, so the boundary is the whole of what a lookup needs.
///
/// @param tree - the table's tree
/// @param pool - the pool its pages live in
/// @param rowid_column - which tree column holds the row's key
fn leaf_boundaries(
    tree: &PagedTree,
    pool: &Pool,
    rowid_column: usize,
) -> DbResult<Vec<(i64, i64)>> {
    let page_size = tree.page_size() as i64;
    let mut boundaries: Vec<(i64, i64)> = Vec::new();
    let mut page = tree.first_leaf(pool)?;
    let mut seen = 0u64;
    while !page.is_none() {
        let guard = pool.fetch(page)?;
        let leaf = inillucent_tree::leaf::LeafRef::parse(&guard)?;
        let mut lowest: Option<i64> = None;
        for row in 0..leaf.row_count() {
            if let Ok(inillucent_tree::datum::Datum::Int(key)) =
                leaf.value_at(inillucent_tree::leaf::Hit::Sorted(row), rowid_column)
            {
                lowest = Some(lowest.map_or(key, |held: i64| held.min(key)));
            }
        }
        for row in 0..leaf.delta_count() {
            if let Ok(inillucent_tree::datum::Datum::Int(key)) =
                leaf.value_at(inillucent_tree::leaf::Hit::Delta(row), rowid_column)
            {
                lowest = Some(lowest.map_or(key, |held: i64| held.min(key)));
            }
        }
        boundaries.push((
            lowest.unwrap_or(i64::MIN),
            (page.0 as i64).saturating_mul(page_size),
        ));
        let next = leaf.right_sibling();
        drop(guard);
        page = next;
        seen = seen.saturating_add(1);
        // The same guard every walk in the tree crate carries: a sibling chain
        // that pointed at itself would otherwise not come back.
        if seen > tree.leaf_count().saturating_add(2) {
            break;
        }
    }
    boundaries.sort_by_key(|(key, _)| *key);
    Ok(boundaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each of SQL's four compound operators maps onto its own set operation.
    ///
    /// **Four values and not three (T3, task-1962).** `UNION` deduplicates and
    /// `UNION ALL` does not, and collapsing them would silently drop every
    /// duplicate row a `UNION ALL` was written to keep.
    #[test]
    fn the_four_compound_operators_are_four_set_operations() {
        use inillucent_sql::ast::CompoundOp;
        assert_eq!(kind_of(CompoundOp::Union), SetKind::Union);
        assert_eq!(kind_of(CompoundOp::UnionAll), SetKind::UnionAll);
        assert_eq!(kind_of(CompoundOp::Except), SetKind::Except);
        assert_eq!(kind_of(CompoundOp::Intersect), SetKind::Intersect);
        assert_ne!(
            kind_of(CompoundOp::Union),
            kind_of(CompoundOp::UnionAll),
            "`UNION` removes duplicates and `UNION ALL` keeps them"
        );
    }
}

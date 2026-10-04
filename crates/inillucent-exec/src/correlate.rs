//! Correlated subqueries, answered per row without a second execution engine.
//!
//! Invariant: a correlated block is planned **once** and run per row against
//! bound parameters. It is never re-planned, never re-bound, and never
//! evaluated by anything but the ordinary planner and the ordinary pipeline -
//! so `EXISTS (SELECT 1 FROM b WHERE b.team = a.team)` reaches the same index
//! probe `SELECT 1 FROM b WHERE b.team = ?1` reaches.
//!
//! ## Why an operator and not an expression
//!
//! [`crate::expr::Eval`] is `Send + Sync`, deliberately: it is what lets a
//! compiled expression be held by a prepared statement and shared. A catalog is
//! neither, so an expression node cannot reach the trees and a subquery cannot
//! be evaluated from inside one. That bound is not relaxed here;
//! instead the value is computed *beside* the row, one column per
//! correlated block, and the expression reads that column - which is exactly
//! how a virtual table's auxiliary functions (`bm25(t)`, `score(t)`) already
//! reach an expression that cannot call a module.
//!
//! ## How the outer row reaches the block
//!
//! The block reads columns of FROM terms it does not own. Each such reference
//! is replaced, once, by a parameter numbered past everything a statement can
//! write, and the joined-row column that feeds it is recorded. Per row the
//! operator binds those parameters and runs the plan.
//!
//! This is decorrelation's alternative and it is chosen for one reason:
//! decorrelation into a semi-join, an anti-join or a left join answers `EXISTS`
//! and `NOT EXISTS`, and does not answer a scalar block in a result column -
//! `SELECT a.name, (SELECT b.region FROM b WHERE b.team = a.team) FROM a` has
//! no join that produces it without also changing what the projection means.
//! One mechanism that answers all three shapes is worth more than two that
//! answer some of them, and what it costs is a plan *execution* per row rather
//! than a plan *compilation* per row.
//!
//! ## What a statement with none pays
//!
//! Nothing. [`correlations_of`] returns an empty list the moment the planner's
//! own `subqueries` flag is clear, and the pipeline then never builds this
//! operator.

use inillucent_base::DbResult;
use inillucent_sql::bind::{BoundExpr, BoundSelect, SourceRows, SubqueryKind};
use inillucent_sql::plan::{plan_select_with, Levers, PhysicalPlan};
use inillucent_tree::datum::OwnedDatum;

use crate::batch::{Batch, Vector};
use crate::expr::Eval;
use crate::ops::{Flow, Sink};
use crate::physical::{prepare_any, run_any_prepared_limited, Params, Prepared, TreeCatalog};

/// The first parameter number a correlation may use.
///
/// SQLite's `SQLITE_MAX_VARIABLE_NUMBER` is 32,766 and this engine's limits
/// agree, so a number past it cannot collide with one a statement wrote.
/// Colliding would be a wrong answer rather than an error - the block would
/// read the application's value instead of the outer row's - which is why the
/// separation is a stated constant rather than "one past the highest we saw".
const FIRST_CORRELATION_PARAMETER: u32 = crate::physical::ENGINE_PARAMETER_BASE;

/// A subquery found in an expression: its number, its kind, whether `NOT` was
/// written, its block, and for an `IN` the whole expression, operand included.
type Found = (usize, SubqueryKind, bool, BoundSelect, Option<BoundExpr>);

/// One correlated block, planned once.
pub struct Correlation {
    /// The binder's statement-wide number for the subquery.
    pub id: usize,
    /// Which of the three forms the block is used as.
    kind: SubqueryKind,
    /// Whether `NOT` was written.
    negated: bool,
    /// The block, with its outer references replaced by parameters.
    plan: PhysicalPlan,
    /// The structural choice for that plan, made once.
    ///
    /// **`answer` used to call `run_any`, which is `prepare_any` and then
    /// `run_any_prepared`, on every outer row** (task-2066 §4.3.1).
    /// `prepare_any` runs the covering candidate trial - a speculative
    /// pipeline build per candidate tree - so an `EXISTS` over five thousand
    /// outer rows made five thousand structural decisions about the same inner
    /// query against the same schema.
    ///
    /// The module comment above says a correlated block is planned once and
    /// run per row. That was true of the plan and not of the prepare.
    prepared: Prepared,
    /// For each replaced reference: the joined-row column that feeds it, and
    /// the parameter number it was given.
    feeds: Vec<(usize, u32)>,
    /// Whether the block reads nothing of the row and nothing the statement
    /// writes, so its first answer holds for the rest of the statement.
    ///
    /// SQLite keeps the answer of a subquery in `RETURNING` that does not read
    /// the table being changed: `(SELECT count(*) FROM log)` is the same for
    /// every row returned, even when a trigger writes `log` between rows.
    once: bool,
    /// The answer kept for a block that is `once`.
    kept: std::cell::RefCell<Option<OwnedDatum>>,
    /// The block's chain, built at its first outer row and run again for each
    /// one after (task-2183).
    ///
    /// **The block was built for every outer row**: `run_any_prepared_limited`
    /// translated and compiled its filter and projection again each time,
    /// which was 39% of `wide.id IN (SELECT owner FROM side_table WHERE owner
    /// = wide.id)` over 400 rows. The outer row reaches the block as
    /// parameters, and a chain whose build read no parameter answers a fresh
    /// set of them without being built again, which is the rule the engine's
    /// statement cache keeps.
    chain: std::cell::RefCell<Option<crate::compiled::Compiled>>,
    /// Whether a chain has been tried for the block.
    tried: std::cell::Cell<bool>,
    /// The index probe an `EXISTS` block is answered by, once its chain has
    /// shown it has one; see [`Correlation::probe_directly`].
    direct: std::cell::RefCell<Option<DirectProbe>>,
}

/// An `EXISTS` block answered by probing one index with the outer row's values.
///
/// **For the commonest correlated block there is (task-2183).** `EXISTS
/// (SELECT 1 FROM side_table b WHERE b.owner = a.id)` over an index on
/// `owner` is one equality probe per outer row. Through the block's chain it
/// wrote the outer value into the parameter set, built the source's bounds
/// back out of it, built a span scan and ran it into a sink: 330 ns an outer
/// row, against 395 ns for SQLite's whole statement.
struct DirectProbe {
    /// The index's root page.
    root: u32,
    /// For each equality of the key, in index order: the joined-row column
    /// that feeds it, and the affinity the seek converts it to.
    keys: Vec<(usize, Option<inillucent_value::Affinity>)>,
}

/// Appends one column per correlated subquery to every row that passes.
///
/// It sits above every join and below every filter that reads what it
/// computes, because the value is read by the `WHERE` and by the projection
/// alike, and both of those run after the last join has widened the row.
///
/// ## The conjuncts that do not read a block are tested first
///
/// **This operator used to answer every block for every row the joins
/// produced, and the `WHERE` threw most of those rows away afterwards**
/// (task-2076). `SELECT a FROM t WHERE t.v % 100 = 0 AND EXISTS (...)` ran the
/// inner query once per row of `t` rather than once per row that survives
/// `t.v % 100 = 0`. A block's prepare was hoisted out of the row loop in
/// task-2066 section 4.3.1; its run cannot be, and that run was paid on rows
/// nobody wanted.
///
/// So the chain hands this operator the `WHERE` conjuncts that hold no
/// subquery at all, and a row that fails one of them is dropped before any
/// block is answered for it. That is sound for one reason: this operator is a
/// map. It emits exactly one row for every row it receives and only appends
/// columns, so a predicate that reads none of the appended columns gives the
/// same verdict on either side of it. A conjunct that names any subquery,
/// correlated or not, stays in the filters above, which is the conservative
/// half of the rule and the one that needs no argument.
///
/// SQLite 3.53.4 answers the same way: a block that fails on a row the cheap
/// conjunct rejects does not fail the query, whichever order the two terms are
/// written in. The test that checks it here is in `new_engine_subquery.rs`,
/// and it failed on this engine before the change.
pub struct Correlated<'t> {
    /// The blocks, in the order their columns are appended.
    correlations: std::rc::Rc<[Correlation]>,
    /// The `WHERE` conjuncts that read no block, compiled against the row as
    /// the joins produce it. A row answers a block only when all of them are
    /// true, which is exactly when the filter above would have kept it.
    gate: std::rc::Rc<[Box<dyn Eval>]>,
    /// Where the trees and layouts come from.
    catalog: &'t dyn TreeCatalog,
    /// The statement's own bound parameters, which a block may also read.
    params: Params,
    /// The copy of `params` the blocks write their outer values into, made at
    /// the first row that passes the gate and kept for the execution.
    ///
    /// **Not one copy per batch (task-2183).** A scan of a table with wide rows
    /// pushes a batch per leaf, and `WHERE a.id % 100 = 0 AND EXISTS (...)`
    /// copied the parameter set forty times to answer four blocks: 7% of the
    /// statement. Each block writes only its own numbers, so one copy serves
    /// every batch the way it already served every row of one.
    bound: Option<Params>,
    downstream: Box<dyn Sink + 't>,
}

impl<'t> Correlated<'t> {
    /// Returns the operator over blocks [`correlations_of`] has prepared.
    ///
    /// @param correlations - the prepared blocks
    /// @param gate - the conjuncts a row must pass before any block is answered
    /// @param catalog - where the trees and layouts come from
    /// @param params - the statement's bound parameters
    /// @param downstream - what to push widened rows into
    pub fn new(
        correlations: std::rc::Rc<[Correlation]>,
        gate: std::rc::Rc<[Box<dyn Eval>]>,
        catalog: &'t dyn TreeCatalog,
        params: &Params,
        downstream: Box<dyn Sink + 't>,
    ) -> Correlated<'t> {
        Correlated {
            correlations,
            gate,
            catalog,
            // **Without the outer statement's folded subqueries.** A block run
            // from here is a statement of its own and folds its own; carrying
            // the outer table in would tell it the fold had already happened
            // and leave its slots empty, which reads from inside `translate` as
            // "a correlated subquery" - a true sentence about the slot and a
            // false one about the query.
            params: params.without_subqueries(),
            bound: None,
            downstream,
        }
    }
}

impl Sink for Correlated<'_> {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        let width = batch.columns.len();
        // **Once per execution, not once per row** (task-2066 §4.3.1). See
        // `Correlation::answer` for what that clone cost, and `bound`.
        // **The outer row is not copied (task-2183).** Each row used to be
        // copied into owned values, every column of it, and then into a
        // second list of borrowed ones, to push a batch of one row of
        // constants. A block reads only the columns it is fed, so those are
        // read where they lie, and the row goes downstream as the outer
        // batch's own vectors under a one row selection, with the answers
        // appended as constants.
        let mut answers: Vec<OwnedDatum> = Vec::with_capacity(self.correlations.len());
        for nth in 0..batch.live() {
            if !passes(&self.gate, batch, nth)? {
                continue;
            }
            answers.clear();
            let bound = self.bound.get_or_insert_with(|| self.params.clone());
            for correlation in self.correlations.iter() {
                let feed = |column: usize| -> DbResult<OwnedDatum> {
                    match column.checked_sub(width) {
                        None => Ok(OwnedDatum::from_datum(&batch.value(nth, column)?)),
                        Some(earlier) => {
                            Ok(answers.get(earlier).cloned().unwrap_or(OwnedDatum::Null))
                        }
                    }
                };
                let answer = correlation.answer_from(self.catalog, bound, &feed)?;
                answers.push(answer);
            }
            // On the stack when the row is narrow, as an index nested loop's
            // joined row is; see `crate::join::IndexNestedLoopJoin`.
            const INLINE: usize = 8;
            let total = width.saturating_add(answers.len());
            let mut inline = [Vector::Const(inillucent_tree::datum::Datum::Null); INLINE];
            let mut spilled: Vec<Vector<'_>> = Vec::new();
            let every = batch
                .columns
                .iter()
                .copied()
                .chain(answers.iter().map(|value| Vector::Const(value.borrow())));
            let columns: &[Vector<'_>] = if total <= INLINE {
                for (slot, vector) in inline.iter_mut().zip(every) {
                    *slot = vector;
                }
                inline.get(..total).unwrap_or(&[])
            } else {
                spilled.extend(every);
                &spilled
            };
            let selected = [batch.row_at(nth) as u32];
            let one = Batch {
                rows: batch.rows,
                selection: Some(&selected),
                columns: crate::batch::Columns::Borrowed(columns),
            };
            if self.downstream.push(&one)? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(Flow::Continue)
    }

    fn finish(&mut self) -> DbResult<()> {
        self.downstream.finish()
    }

    fn reset(&mut self) -> DbResult<()> {
        self.bound = None;
        self.downstream.reset()
    }
}

/// Reports whether a tree holds a live row whose key begins with a prefix.
///
/// @param tree - the index
/// @param pool - the pool its pages live in
/// @param key - the prefix, already converted to the seek's affinity
fn holds_key(
    tree: &inillucent_tree::paged::PagedTree,
    pool: &inillucent_pool::Pool,
    key: &[inillucent_tree::datum::Datum<'_>],
) -> DbResult<bool> {
    let mut found = false;
    tree.visit_equal(pool, key, &mut |leaf, start, end| {
        // A written leaf's live rows are not its sorted run, so they are
        // matched again, the way an index nested loop's range probe does.
        found = if leaf.needs_materialising() {
            !leaf.live_matching(key, 8)?.is_empty()
        } else {
            end > start
        };
        Ok(!found)
    })?;
    Ok(found)
}

/// Whether one row passes every conjunct of a gate.
///
/// The same test [`crate::ops::Filter`] applies: a row is kept only when the
/// predicate is definitely true, so a NULL rejects it exactly as it would in
/// the `WHERE` the conjunct came from.
///
/// @param gate - the compiled conjuncts
/// @param batch - the rows as the joins produced them
/// @param nth - which live row of the batch to test
fn passes(gate: &[Box<dyn Eval>], batch: &Batch<'_>, nth: usize) -> DbResult<bool> {
    for predicate in gate {
        let verdict = predicate.value(batch, nth)?;
        if crate::expr::truth(&verdict.get()) != Some(true) {
            return Ok(false);
        }
    }
    Ok(true)
}

impl Correlation {
    /// Returns what this block answers for one outer row.
    ///
    /// **The parameter set is the caller's and is written into** (task-2066
    /// §4.3.1). This used to clone it per row, and
    /// [`FIRST_CORRELATION_PARAMETER`] is 100,000, so that clone copied a
    /// hundred thousand `OwnedDatum` slots to write one of them. Measured on
    /// 5,000 outer rows with an `EXISTS` whose probe matches nothing - so
    /// nothing but the setup runs - it was **1.47 milliseconds an outer row**.
    ///
    /// Each block writes only its own numbers, and they are past anything a
    /// statement can write, so sharing one set between the blocks of one batch
    /// cannot let one of them read another's value.
    ///
    /// A block that is `once` answers from the first run for the rest of the
    /// statement; `forget` is what starts the statement again.
    ///
    /// @param catalog - where the trees and layouts come from
    /// @param bound - the statement's parameters, to write this row's feeds into
    /// @param row - the joined row so far
    pub fn answer(
        &self,
        catalog: &dyn TreeCatalog,
        bound: &mut Params,
        row: &[OwnedDatum],
    ) -> DbResult<OwnedDatum> {
        self.answer_from(catalog, bound, &|column| {
            Ok(row.get(column).cloned().unwrap_or(OwnedDatum::Null))
        })
    }

    /// [`Correlation::answer`], reading each fed column through a function.
    ///
    /// @param catalog - where the trees and layouts come from
    /// @param bound - the statement's parameters, to write this row's feeds into
    /// @param feed - returns the joined row's value in one column
    pub fn answer_from(
        &self,
        catalog: &dyn TreeCatalog,
        bound: &mut Params,
        feed: &dyn Fn(usize) -> DbResult<OwnedDatum>,
    ) -> DbResult<OwnedDatum> {
        if self.once {
            if let Some(kept) = self.kept.borrow().clone() {
                return Ok(kept);
            }
        }
        let answer = self.run(catalog, bound, feed)?;
        if self.once {
            *self.kept.borrow_mut() = Some(answer.clone());
        }
        Ok(answer)
    }

    /// Runs the block's kept chain for one outer row, or builds it at the first.
    ///
    /// `None` when the block has no chain that can be run again - a shape the
    /// chain builder refuses, a build that read a parameter, settings that
    /// have changed since - and the caller builds and runs it the old way.
    ///
    /// @param catalog - where the trees and layouts come from
    /// @param bound - the statement's parameters, with this row's feeds in them
    fn run_chain(
        &self,
        catalog: &dyn TreeCatalog,
        bound: &Params,
    ) -> DbResult<Option<Vec<Vec<OwnedDatum>>>> {
        self.with_chain(catalog, bound, &mut |compiled| {
            compiled.run(&self.plan, catalog, bound)?;
            Ok(compiled.take_rows())
        })
    }

    /// Answers whether an `EXISTS` block produces a row, through its kept chain.
    ///
    /// A block whose access path answers its whole `WHERE` reads at most one
    /// row of its source and nothing above it; see `Compiled::any_row`.
    /// `None` when the block has no chain, as for [`Correlation::run_chain`].
    ///
    /// @param catalog - where the trees and layouts come from
    /// @param bound - the statement's parameters, with this row's feeds in them
    fn exists_by_chain(&self, catalog: &dyn TreeCatalog, bound: &Params) -> DbResult<Option<bool>> {
        self.with_chain(catalog, bound, &mut |compiled| {
            if compiled.answers_existence(&self.plan) {
                if let Ok(mut direct) = self.direct.try_borrow_mut() {
                    if direct.is_none() && compiled.rebindable() {
                        *direct = self.direct_probe();
                    }
                }
                return compiled.any_row(&self.plan, catalog, bound);
            }
            compiled.run(&self.plan, catalog, bound)?;
            Ok(!compiled.take_rows().is_empty())
        })
    }

    /// Returns the index probe this `EXISTS` block reduces to, when it does.
    ///
    /// One stage reading an index by an equality prefix and nothing else,
    /// every value of which is a parameter the outer row feeds. The caller has
    /// already checked that the access path consumes the whole `WHERE`.
    fn direct_probe(&self) -> Option<DirectProbe> {
        let [stage] = self.prepared.stages.as_slice() else {
            return None;
        };
        if stage.kind != crate::physical::AccessKind::Span {
            return None;
        }
        let term = self.plan.sources.get(stage.term)?;
        let inillucent_sql::plan::AccessPath::IndexSeek {
            equalities,
            unconverted,
            low: None,
            high: None,
            columns,
            ..
        } = &term.path
        else {
            return None;
        };
        if equalities.is_empty() {
            return None;
        }
        let mut keys = Vec::with_capacity(equalities.len());
        for (position, expr) in equalities.iter().enumerate() {
            let BoundExpr::Parameter(number) = expr else {
                return None;
            };
            let (column, _) = self.feeds.iter().find(|(_, fed)| fed == number)?;
            let affinity = crate::physical::probe_affinity(
                unconverted.contains(&position),
                crate::physical::index_affinity(&term.table, columns, position),
            );
            keys.push((*column, affinity));
        }
        Some(DirectProbe {
            root: stage.root,
            keys,
        })
    }

    /// Answers an `EXISTS` block by its index probe, when it has one.
    ///
    /// `None` when the block has no [`DirectProbe`] yet; the first outer row
    /// runs the chain, which is what finds out whether it has one.
    ///
    /// @param catalog - where the trees come from
    /// @param feed - returns the joined row's value in one column
    fn probe_directly(
        &self,
        catalog: &dyn TreeCatalog,
        feed: &dyn Fn(usize) -> DbResult<OwnedDatum>,
    ) -> DbResult<Option<bool>> {
        let Ok(held) = self.direct.try_borrow() else {
            return Ok(None);
        };
        let Some(direct) = held.as_ref() else {
            return Ok(None);
        };
        let (Some(tree), Some(pool)) = (catalog.tree(direct.root), catalog.pool_for(direct.root))
        else {
            return Ok(None);
        };
        // One key value is the common case, and it needs no list.
        if let [(column, affinity)] = direct.keys.as_slice() {
            let value = crate::constant::seek_value(feed(*column)?, *affinity)?;
            // `x = NULL` is never true, which is the span the seek would build.
            if value == OwnedDatum::Null {
                return Ok(Some(false));
            }
            return holds_key(tree, pool, &[value.borrow()]).map(Some);
        }
        let mut values = Vec::with_capacity(direct.keys.len());
        for (column, affinity) in &direct.keys {
            let value = crate::constant::seek_value(feed(*column)?, *affinity)?;
            if value == OwnedDatum::Null {
                return Ok(Some(false));
            }
            values.push(value);
        }
        let key: Vec<inillucent_tree::datum::Datum<'_>> =
            values.iter().map(OwnedDatum::borrow).collect();
        holds_key(tree, pool, &key).map(Some)
    }

    /// Hands the block's kept chain to `work`, building it at the first call.
    ///
    /// `None` when the block has no chain that can be run again - a shape the
    /// chain builder refuses, a build that read a parameter, settings that
    /// have changed since - and the caller builds and runs it the old way.
    ///
    /// @param catalog - where the trees and layouts come from
    /// @param bound - the statement's parameters, with this row's feeds in them
    /// @param work - what to do with the chain
    fn with_chain<R>(
        &self,
        catalog: &dyn TreeCatalog,
        bound: &Params,
        work: &mut dyn FnMut(&mut crate::compiled::Compiled) -> DbResult<R>,
    ) -> DbResult<Option<R>> {
        let Ok(mut held) = self.chain.try_borrow_mut() else {
            return Ok(None);
        };
        if !self.tried.get() {
            self.tried.set(true);
            let Some(mut compiled) = crate::compiled::try_compile_limited(
                &self.plan,
                catalog,
                &self.prepared,
                bound,
                Some(1),
            )?
            else {
                return Ok(None);
            };
            let answer = work(&mut compiled)?;
            if compiled.rebindable() {
                *held = Some(compiled);
            }
            return Ok(Some(answer));
        }
        match held.as_mut() {
            Some(compiled) if compiled.built_under(bound) => Ok(Some(work(compiled)?)),
            _ => Ok(None),
        }
    }

    /// Forgets the answer a `once` block kept, so the next statement runs it.
    pub fn forget(&self) {
        *self.kept.borrow_mut() = None;
    }

    /// Runs the block for one outer row.
    ///
    /// @param catalog - where the trees and layouts come from
    /// @param bound - the statement's parameters, to write this row's feeds into
    /// @param feed - returns the joined row's value in one column
    fn run(
        &self,
        catalog: &dyn TreeCatalog,
        bound: &mut Params,
        feed: &dyn Fn(usize) -> DbResult<OwnedDatum>,
    ) -> DbResult<OwnedDatum> {
        if self.kind == SubqueryKind::Exists {
            if let Some(found) = self.probe_directly(catalog, feed)? {
                return Ok(OwnedDatum::Int(i64::from(found != self.negated)));
            }
        }
        for (column, number) in &self.feeds {
            bound.set(*number, feed(*column)?);
        }
        if self.kind == SubqueryKind::Exists {
            if let Some(found) = self.exists_by_chain(catalog, bound)? {
                return Ok(OwnedDatum::Int(i64::from(found != self.negated)));
            }
        }
        // **One row is all any of the three forms reads.** `Exists` asks
        // whether the block produced anything and `Scalar` takes the first row
        // and drops the rest, so the unlimited run was reading an inner result
        // set to throw it away. `In` is refused in `correlations_of` and is
        // stated below so a variant added later is a compilation error.
        let rows = match self.run_chain(catalog, bound)? {
            Some(rows) => rows,
            None => {
                run_any_prepared_limited(&self.plan, catalog, &self.prepared, bound, Some(1))?.0
            }
        };
        let mut column = rows
            .into_iter()
            .map(|row| row.into_iter().next().unwrap_or(OwnedDatum::Null));
        Ok(match self.kind {
            SubqueryKind::Exists => {
                OwnedDatum::Int(i64::from(column.next().is_some() != self.negated))
            }
            // SQLite's rule for a block that produced nothing is NULL, which is
            // what a missing row already reads as.
            SubqueryKind::Scalar => column.next().unwrap_or(OwnedDatum::Null),
            // Refused in `correlations_of`; stated here so a variant added
            // later is a compilation error rather than a NULL.
            SubqueryKind::In => OwnedDatum::Null,
        })
    }
}

/// A statement's prepared correlated blocks, kept for its next execution.
///
/// **Prepared once per statement, not once per execution (task-2183).**
/// Every execution planned and prepared each block again, and its first outer
/// row compiled the block's chain again: a fifth of `WHERE a.id % 100 = 0 AND
/// EXISTS (...)`, which answers four blocks. A block's plan, its structural
/// choices and its chain depend on the statement and the schema, as the
/// `Prepared` that holds this does, and a kept chain is checked against the
/// connection's settings before each run. A `once` block's kept answer is
/// forgotten at each execution, because it depends on the parameters.
///
/// A cell rather than a shared one: a statement that has no block allocates
/// nothing for it, which the compile allocation bound for `SELECT 1` counts,
/// and a clone made after the blocks were built shares them.
#[derive(Clone, Default)]
pub struct KeptBlocks(std::cell::RefCell<Option<std::rc::Rc<[Correlation]>>>);

impl KeptBlocks {
    /// Returns the kept blocks, ready for a new execution, or builds and keeps them.
    ///
    /// @param build - prepares the blocks when none are kept
    pub fn get_or_build(
        &self,
        build: impl FnOnce() -> DbResult<Vec<Correlation>>,
    ) -> DbResult<std::rc::Rc<[Correlation]>> {
        if let Some(kept) = self.0.borrow().as_ref() {
            for correlation in kept.iter() {
                correlation.forget();
            }
            return Ok(std::rc::Rc::clone(kept));
        }
        let built: std::rc::Rc<[Correlation]> = build()?.into();
        *self.0.borrow_mut() = Some(std::rc::Rc::clone(&built));
        Ok(built)
    }
}

impl std::fmt::Debug for KeptBlocks {
    /// Says how many blocks are kept; a block itself has nothing to print.
    ///
    /// @param formatter - where the text goes
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let held = self.0.borrow().as_ref().map(|kept| kept.len());
        write!(formatter, "KeptBlocks({held:?})")
    }
}

/// Finds every correlated block a list of expressions holds, and prepares it.
///
/// The companion to [`correlations_of`], for the statements that have
/// expressions but no plan to hang them on: an `UPDATE`'s assignments and a
/// `RETURNING` clause are evaluated by the write path directly, so nothing ever
/// built a `PhysicalPlan` for them.
///
/// @param exprs - the expressions to look through
/// @param resolve - which row column an outer reference reads
pub fn correlations_in(
    exprs: &[&BoundExpr],
    catalog: &dyn TreeCatalog,
    resolve: &dyn Fn(&BoundExpr) -> Option<usize>,
) -> DbResult<Vec<Correlation>> {
    let mut found: Vec<Found> = Vec::new();
    for expr in exprs {
        gather_expression(expr, &mut found);
    }
    prepare_blocks(found, catalog, resolve, None, Levers::default())
}

/// Finds every subquery a list of expressions holds, correlated or not, and
/// prepares it to be answered for each row.
///
/// For a `RETURNING` clause: SQLite evaluates its subqueries for each row it
/// returns, after the row is written, so one that reads a table the statement
/// changes sees the rows written so far.
///
/// @param exprs - the expressions to look through
/// @param resolve - which row column an outer reference reads
/// @param target_root - the root page of the table the statement changes
pub fn correlations_in_every(
    exprs: &[&BoundExpr],
    catalog: &dyn TreeCatalog,
    resolve: &dyn Fn(&BoundExpr) -> Option<usize>,
    target_root: u32,
) -> DbResult<Vec<Correlation>> {
    let mut found: Vec<Found> = Vec::new();
    for expr in exprs {
        gather_every(expr, &mut found);
    }
    prepare_blocks(
        found,
        catalog,
        resolve,
        Some(target_root),
        Levers::default(),
    )
}

/// Finds every correlated block in a plan and prepares it.
///
/// Returns an empty list when there is none, which is the common case and the
/// one that must cost nothing.
///
/// @param plan - the planner's output
/// @param resolve - which joined-row column an outer reference reads
pub fn correlations_of(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    resolve: &dyn Fn(&BoundExpr) -> Option<usize>,
) -> DbResult<Vec<Correlation>> {
    if !plan.subqueries {
        return Ok(Vec::new());
    }
    let mut found: Vec<Found> = Vec::new();
    gather_plan(plan, &mut found);
    prepare_blocks(found, catalog, resolve, None, plan.levers)
}

/// Rewrites each gathered block's outer references into parameters, and plans it.
///
/// @param found - the correlated blocks
/// @param resolve - which row column an outer reference reads
/// @param levers - the outer statement's optimizations, which each block is
///   planned with, so `PRAGMA reverse_unordered_selects` reaches it too
fn prepare_blocks(
    found: Vec<Found>,
    catalog: &dyn TreeCatalog,
    resolve: &dyn Fn(&BoundExpr) -> Option<usize>,
    target_root: Option<u32>,
    levers: Levers,
) -> DbResult<Vec<Correlation>> {
    let mut prepared = Vec::with_capacity(found.len());
    for (id, kind, negated, block, membership) in found {
        // **A correlated `IN` is answered as a scalar block that holds the whole
        // test.** The operator carries one value per block, and the answer of
        // `x IN (SELECT ...)` is one value: 1, 0 or NULL. Wrapped like that, the
        // operand's outer references become parameters with the block's own, and
        // the `IN` inside is an ordinary uncorrelated one for the run.
        let (kind, negated, block) = match (kind, membership) {
            (SubqueryKind::In, Some(node)) => (SubqueryKind::Scalar, false, membership_block(node)),
            (SubqueryKind::In, None) => {
                return crate::physical::unsupported("a correlated IN subquery");
            }
            (other, _) => (other, negated, block),
        };
        let mut block = block;
        let once = block.correlations.is_empty()
            && target_root.is_some_and(|root| !reads_table(&block, root));
        let owned = owned_sources(&block);
        let mut feeds = Vec::new();
        let mut next = FIRST_CORRELATION_PARAMETER;
        let mut unresolved = false;
        inillucent_sql::rewrite::rewrite_select(&mut block, &mut |expr: &mut BoundExpr| {
            let outer = match expr {
                BoundExpr::Column { source, .. } | BoundExpr::Rowid { source } => *source,
                _ => return,
            };
            if owned.contains(&outer) {
                return;
            }
            match resolve(expr) {
                Some(column) => {
                    feeds.push((column, next));
                    *expr = BoundExpr::Parameter(next);
                    next = next.saturating_add(1);
                }
                None => unresolved = true,
            }
        });
        if unresolved {
            return crate::physical::unsupported(
                "a correlated subquery reading a column the joined row does not carry",
            );
        }
        forget_parameterised(&mut block, &owned);
        inillucent_sql::rewrite::rewrite_select(&mut block, &mut |expr: &mut BoundExpr| {
            if let Some(inner) = expr.block_mut() {
                inner.correlations.retain(|held| owned.contains(held));
            }
        });
        // Prepared here, once, which is the whole of section 4.3.1: the plan
        // and the schema decide the structural choice and neither depends on
        // the outer row.
        let plan = plan_select_with(block, levers);
        let choice = prepare_any(&plan, catalog)?;
        prepared.push(Correlation {
            id,
            kind,
            negated,
            plan,
            prepared: choice,
            feeds,
            once,
            kept: std::cell::RefCell::new(None),
            chain: std::cell::RefCell::new(None),
            tried: std::cell::Cell::new(false),
            direct: std::cell::RefCell::new(None),
        });
    }
    Ok(prepared)
}

/// Reports whether a block reads a table, at any depth.
///
/// @param block - the query
/// @param root - the root page of the table
fn reads_table(block: &BoundSelect, root: u32) -> bool {
    if reads_in_from(block, root) {
        return true;
    }
    let mut nested: Vec<BoundSelect> = Vec::new();
    let mut copy = block.clone();
    inillucent_sql::rewrite::rewrite_select(&mut copy, &mut |expr: &mut BoundExpr| {
        if let Some(inner) = expr.block_mut() {
            nested.push(inner.clone());
        }
    });
    nested.iter().any(|inner| reads_in_from(inner, root))
}

/// Reports whether the FROM terms of a block, its derived tables and its
/// compound arms read a table.
///
/// @param block - the query
/// @param root - the root page of the table
fn reads_in_from(block: &BoundSelect, root: u32) -> bool {
    block.sources.iter().any(|source| match &source.rows {
        SourceRows::Table => source.table.root == root,
        SourceRows::Subquery(inner) => reads_in_from(inner, root),
        _ => false,
    }) || block
        .compounds
        .iter()
        .any(|(_, arm)| reads_in_from(arm, root))
}
/// Builds the one row block that answers a whole `x IN (SELECT ...)` test.
///
/// @param node - the `IN` subquery expression, operand and block together
fn membership_block(node: BoundExpr) -> BoundSelect {
    let mut correlations = Vec::new();
    node.sources_used(&mut correlations);
    BoundSelect {
        sources: Vec::new(),
        filter: None,
        group_by: Vec::new(),
        having: None,
        columns: vec![inillucent_sql::bind::BoundResultColumn {
            expr: node,
            name: b"in".to_vec(),
            origin: None,
            declared_type: Vec::new(),
            written: None,
        }],
        distinct: false,
        order_by: Vec::new(),
        limit: None,
        offset: None,
        aggregates: Vec::new(),
        values: Vec::new(),
        compounds: Vec::new(),
        windows: Vec::new(),
        correlations,
        shared: None,
        serial: 0,
    }
}

/// Takes the outer terms a block no longer reads off every correlation list in
/// it.
///
/// **The rewrite above has made them parameters, at every depth.** A derived
/// table inside the block still listed the outer term it read, so the planner
/// marked it correlated, and a correlated derived table as the block's first
/// term was refused: `SELECT (SELECT json_group_array(name) FROM (SELECT g.name
/// FROM todo_tag tt JOIN tag g ON g.id = tt.tag_id WHERE tt.todo_id = t.id
/// ORDER BY g.name)) FROM todo t` answered "a correlated subquery as the
/// outermost term". Once `t.id` is a parameter the derived table reads nothing
/// outside the block, and it runs once per outer row like the rest of it.
///
/// @param block - the rewritten block
/// @param owned - the source numbers the block and everything in it own
fn forget_parameterised(block: &mut BoundSelect, owned: &[usize]) {
    block.correlations.retain(|id| owned.contains(id));
    for source in &mut block.sources {
        match &mut source.rows {
            SourceRows::Subquery(inner) => forget_parameterised(inner, owned),
            SourceRows::Recursive(body) => {
                for (_, arm) in body.seeds.iter_mut().chain(body.steps.iter_mut()) {
                    forget_parameterised(arm, owned);
                }
            }
            SourceRows::Table | SourceRows::RecursiveSelf { .. } => {}
        }
    }
    for (_, arm) in &mut block.compounds {
        forget_parameterised(arm, owned);
    }
}

/// Returns the statement-wide source numbers a block's own FROM terms have.
///
/// Everything else a column reference names belongs to an enclosing block, and
/// that is precisely what makes the subquery correlated.
///
/// @param block - the nested query
fn owned_sources(block: &BoundSelect) -> Vec<usize> {
    let mut owned = Vec::new();
    collect_sources(block, &mut owned);
    // **A subquery inside one of the block's expressions owns its sources
    // too.** The rewrite in `prepare_blocks` walks into every nested block -
    // a scalar subquery in a table function's argument, in a derived table's
    // `WHERE`, in a result column - and a column of a table that only such a
    // subquery reads was taken for an outer reference. It could not be fed,
    // so `(SELECT count(*) FROM json_each((SELECT json_group_array(a) FROM
    // t0)) AS s WHERE s.value = u.k)` was refused as "a correlated subquery
    // reading a column the joined row does not carry", and SQLite answers it.
    // The walk is over a copy because `rewrite_select` is the one walker that
    // reaches every expression, and it takes the block mutably.
    let mut copy = block.clone();
    inillucent_sql::rewrite::rewrite_select(&mut copy, &mut |expr: &mut BoundExpr| {
        if let Some(inner) = expr.block_mut() {
            collect_sources(inner, &mut owned);
        }
    });
    owned
}

/// Collects the source numbers of a block and of everything nested in it.
///
/// @param block - the query to walk
/// @param into - the numbers found so far
fn collect_sources(block: &BoundSelect, into: &mut Vec<usize>) {
    for source in &block.sources {
        into.push(source.id);
        match &source.rows {
            SourceRows::Subquery(inner) => collect_sources(inner, into),
            SourceRows::Recursive(body) => {
                for (_, arm) in body.seeds.iter().chain(body.steps.iter()) {
                    collect_sources(arm, into);
                }
            }
            SourceRows::Table | SourceRows::RecursiveSelf { .. } => {}
        }
    }
    for (_, arm) in &block.compounds {
        collect_sources(arm, into);
    }
}

/// Collects every correlated block of a planned statement.
///
/// @param plan - the planner's output
/// @param into - the blocks found so far
fn gather_plan(plan: &PhysicalPlan, into: &mut Vec<Found>) {
    gather_select(&plan.select, into);
    for residual in plan.residuals.iter().flatten() {
        gather_expression(residual, into);
    }
    if let Some(filter) = &plan.constant_filter {
        gather_expression(filter, into);
    }
}

/// Collects every correlated block of a query.
///
/// @param select - the query to walk
/// @param into - the blocks found so far
fn gather_select(select: &BoundSelect, into: &mut Vec<Found>) {
    for column in &select.columns {
        gather_expression(&column.expr, into);
    }
    if let Some(filter) = &select.filter {
        gather_expression(filter, into);
    }
    if let Some(having) = &select.having {
        gather_expression(having, into);
    }
    for term in &select.group_by {
        gather_expression(term, into);
    }
    for term in &select.order_by {
        gather_expression(&term.expr, into);
    }
    for source in &select.sources {
        if let Some(constraint) = &source.constraint {
            gather_expression(constraint, into);
        }
    }
    // **The aggregates and the windows, which this walk did not have
    // (task-1932, M6).** `select.aggregates` is a list of its own, beside
    // `select.columns` rather than inside it, so a subquery written as an
    // aggregate's argument was never seen here. It was therefore never
    // recognised as a correlated block, its slot was never filled, and
    // `translate` reported the empty slot as `unsupported("a correlated
    // subquery used as a value")` - a true statement about the slot and a false
    // one about the query. `SELECT team, SUM((SELECT b.amount FROM b WHERE
    // b.team = a.team)) FROM a GROUP BY team` was refused outright.
    //
    // The `FILTER` and the inner `ORDER BY` are walked for the same reason
    // `bind::gather_columns` walks them: they read the row, so a subquery in
    // one of them is correlated in exactly the way an argument's is.
    for aggregate in &select.aggregates {
        for argument in &aggregate.arguments {
            gather_expression(argument, into);
        }
        if let Some(filter) = &aggregate.filter {
            gather_expression(filter, into);
        }
        for term in &aggregate.order_by {
            gather_expression(&term.expr, into);
        }
    }
    for window in &select.windows {
        for argument in &window.arguments {
            gather_expression(argument, into);
        }
        if let Some(filter) = &window.filter {
            gather_expression(filter, into);
        }
        for term in &window.partition_by {
            gather_expression(term, into);
        }
        for term in &window.order_by {
            gather_expression(&term.expr, into);
        }
    }
}

/// Collects every correlated block one expression holds.
///
/// @param expr - the expression to walk
/// @param into - the blocks found so far
fn gather_expression(expr: &BoundExpr, into: &mut Vec<Found>) {
    gather_expression_with(expr, into, false);
}

/// Collects the subqueries one expression holds, correlated or not.
///
/// For a `RETURNING` clause, whose subqueries SQLite evaluates for each row it
/// returns, after that row is written. An uncorrelated one is folded once for
/// the statement everywhere else, and the state of the table when it is folded
/// is not the state the row sees.
///
/// @param expr - the expression to walk
/// @param into - the blocks found so far
fn gather_every(expr: &BoundExpr, into: &mut Vec<Found>) {
    gather_expression_with(expr, into, true);
}

/// The walk behind [`gather_expression`] and [`gather_every`].
///
/// @param expr - the expression to walk
/// @param into - the blocks found so far
/// @param every - whether an uncorrelated subquery is collected too
fn gather_expression_with(expr: &BoundExpr, into: &mut Vec<Found>, every: bool) {
    if let BoundExpr::Subquery {
        id,
        kind,
        negated,
        block,
        ..
    } = expr
    {
        let wanted = !block.correlations.is_empty() || every;
        if wanted && !into.iter().any(|(held, ..)| *held == *id) {
            let membership = (*kind == SubqueryKind::In).then(|| expr.clone());
            into.push((*id, *kind, *negated, (**block).clone(), membership));
        }
    }
    for child in expr.children() {
        gather_expression_with(child, into, every);
    }
}

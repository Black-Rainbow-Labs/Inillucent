//! Filtering and projecting: the two operators that change a row's shape.
//!
//! Invariant: **neither copies a value.** A filter hands on a selection
//! vector rather than moving rows, and a projection that permutes columns
//! rebuilds the batch out of the same borrowed vectors.

use inillucent_base::DbResult;
use inillucent_tree::datum::Datum;

use crate::batch::{Batch, Vector};
use crate::expr::Eval;

use super::*;

/// Applies a predicate, producing a selection vector rather than moving rows.
pub struct Filter {
    predicate: Box<dyn Eval>,
    downstream: Box<dyn Sink>,
    pub(crate) selection: Vec<u32>,
    /// Whether the predicate reads no column of the rows it filters, and so
    /// is evaluated once at the end when no row arrived.
    constant: bool,
    /// Whether any batch has arrived since the last reset.
    saw_rows: bool,
}
impl Filter {
    /// Returns a filter over a compiled predicate.
    ///
    /// @param predicate - the compiled predicate
    /// @param downstream - what to push the surviving rows into
    pub fn new(predicate: Box<dyn Eval>, downstream: Box<dyn Sink>) -> Filter {
        Filter {
            predicate,
            downstream,
            selection: Vec::with_capacity(crate::batch::BATCH_ROWS),
            constant: false,
            saw_rows: false,
        }
    }

    /// Returns a filter over a predicate that reads no column of the rows.
    ///
    /// **Such a predicate is evaluated even when no row arrives.** SQLite
    /// tests a `WHERE` term that reads no table of its query once, before the
    /// loop, so a term that fails fails the statement over an empty table:
    /// `SELECT count(*) FROM e WHERE abs(-9223372036854775808) IS NULL` is
    /// "integer overflow" there. This filter tested it per row, and with no
    /// row it answered 0. It is still tested per row when rows arrive, which
    /// keeps a term such as `random() > 0` a per row test, as SQLite keeps it;
    /// when none did, `finish` evaluates it once and reports its failure.
    ///
    /// @param predicate - the compiled predicate
    /// @param downstream - what to push the surviving rows into
    pub fn constant(predicate: Box<dyn Eval>, downstream: Box<dyn Sink>) -> Filter {
        Filter {
            constant: true,
            ..Filter::new(predicate, downstream)
        }
    }
}
impl Sink for Filter {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        self.saw_rows = true;
        self.selection.clear();
        // A predicate that can judge the whole batch in one loop does, and
        // gives the verdicts `value` would; see `Eval::truths_over`.
        let mut truths = Vec::new();
        let mut failed: Option<inillucent_base::DbError> = None;
        if self.predicate.truths_over(batch, &mut truths) && truths.len() == batch.live() {
            for (nth, kept) in truths.iter().enumerate() {
                if *kept {
                    self.selection.push(batch.row_at(nth) as u32);
                }
            }
        } else {
            for nth in 0..batch.live() {
                // SQL's WHERE keeps a row only when the predicate is definitely
                // true: NULL is not true, and the three-valued logic in `expr`
                // produces NULL rather than false so the distinction survives to
                // here.
                let verdict = match self.predicate.value(batch, nth) {
                    Ok(verdict) => verdict,
                    Err(error) => {
                        failed = Some(error);
                        break;
                    }
                };
                if crate::expr::truth(&verdict.get()) == Some(true) {
                    self.selection.push(batch.row_at(nth) as u32);
                }
            }
        }
        if self.selection.is_empty() {
            return match failed {
                Some(error) => Err(error),
                None => Ok(Flow::Continue),
            };
        }
        let filtered = Batch {
            rows: batch.rows,
            selection: Some(&self.selection),
            columns: batch.columns.clone(),
        };
        let flow = self.downstream.push(&filtered)?;
        // The rows that passed ahead of the failing one went on first; SQLite
        // returns them and then reports the error (bug 28).
        match (failed, flow) {
            (Some(error), Flow::Continue) => Err(error),
            _ => Ok(flow),
        }
    }

    fn finish(&mut self) -> DbResult<()> {
        if self.constant && !self.saw_rows {
            // One row with no columns: the predicate reads none.
            let empty = Batch::new(1, Vec::new());
            self.predicate.value(&empty, 0)?;
        }
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.selection.clear();
        self.saw_rows = false;
        self.downstream.reset()
    }
}
/// Evaluates the constant operands of `AND` and `OR` once, before the first row goes on.
///
/// **SQLite evaluates a constant once, the first time a row reaches it, and not
/// once per row.** `SELECT a, a AND abs(-9223372036854775808) FROM t` fails
/// before it returns any row, even when the first row's `a` is 0 and the `AND`
/// would not look at its right side. Evaluating the operand per row, with the
/// short circuit, raised the error on the first row that needed it, and now that
/// the rows before a failing one are returned, the rows before that one came out
/// first. The guards are compiled from the operands and run on the first batch
/// that reaches this operator.
///
/// **Only the result columns are guarded.** A `WHERE` term is different in
/// SQLite: `SELECT a FROM t WHERE (a = 1) OR abs(-9223372036854775808)` returns
/// the first row when its `a` is 1 and fails on the next, so the constant there
/// is evaluated when a row first needs it and the filters are left as they are.
pub struct Hoisted {
    guards: Vec<Box<dyn Eval>>,
    ran: bool,
    downstream: Box<dyn Sink>,
}
impl Hoisted {
    /// Returns an operator that runs guards before its first batch.
    ///
    /// @param guards - the compiled constant operands, which read no column
    /// @param downstream - what to push each batch into afterwards
    pub fn new(guards: Vec<Box<dyn Eval>>, downstream: Box<dyn Sink>) -> Hoisted {
        Hoisted {
            guards,
            ran: false,
            downstream,
        }
    }
}
impl Sink for Hoisted {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        if !self.ran && batch.live() > 0 {
            self.ran = true;
            for guard in &self.guards {
                guard.value(batch, 0)?;
            }
        }
        self.downstream.push(batch)
    }

    fn finish(&mut self) -> DbResult<()> {
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.ran = false;
        self.downstream.reset()
    }
}
/// Evaluates a list of expressions into a new batch.
pub struct Project {
    expressions: Vec<Box<dyn Eval>>,
    downstream: Box<dyn Sink>,
}
impl Project {
    /// Returns a projection.
    ///
    /// @param expressions - one compiled expression per output column
    /// @param downstream - what to push the projected batch into
    pub fn new(expressions: Vec<Box<dyn Eval>>, downstream: Box<dyn Sink>) -> Project {
        Project {
            expressions,
            downstream,
        }
    }
}
impl Sink for Project {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        let live = batch.live();
        // The permutation fast path. A projection that only reorders the
        // input's columns produces no values of its own, so the general path
        // below builds four `Vec`s to arrive at a list of vectors it already
        // had - once per batch, and a point probe's batch is one row.
        // `inillucent-probeprofile` measured `SELECT count(*) ... WHERE id = ?1`
        // at 0.87 us against a bare probe of about 0.24 us, and this is one of
        // the allocations in between.
        if self.expressions.len() <= INLINE_PROJECT {
            let mut inline: [Vector<'_>; INLINE_PROJECT] =
                [Vector::Const(Datum::Null); INLINE_PROJECT];
            let mut permutation = true;
            for (at, expression) in self.expressions.iter().enumerate() {
                match expression
                    .column()
                    .and_then(|column| batch.columns.get(column))
                {
                    Some(vector) => {
                        if let Some(slot) = inline.get_mut(at) {
                            *slot = *vector;
                        }
                    }
                    None => {
                        permutation = false;
                        break;
                    }
                }
            }
            if permutation {
                // The selection is carried through rather than applied: the
                // vectors are indexed by the input's row numbers, so a batch
                // that arrives selected has to leave selected. The first
                // version required a dense batch for exactly this reason, and
                // then a point probe handing one row of a leaf downstream could
                // not use it.
                let mut projected = Batch::over(
                    batch.rows,
                    inline.get(..self.expressions.len()).unwrap_or(&[]),
                );
                projected.selection = batch.selection;
                return self.downstream.push(&projected);
            }
        }
        // A projection that is a permutation of the input columns rebuilds the
        // batch out of the same borrowed vectors and copies nothing. Anything
        // computed is materialised into a scratch buffer whose lifetime is this
        // call, which is what the borrow below relies on.
        // An expression may *build* its answer - a scalar function returning
        // text owns bytes that were not in the page - so the computed columns
        // are held as `Computed` and borrowed in a second pass. The two passes
        // are not a cost: the first would have had to exist anyway, and the
        // second is a pointer per value over storage that does not move.
        let mut computed: Vec<Vec<crate::expr::Computed<'_>>> =
            Vec::with_capacity(self.expressions.len());
        let mut passthrough: Vec<Option<usize>> = Vec::with_capacity(self.expressions.len());
        // The first row an expression failed on, and what it said. Rows before
        // it are still good for every column, and are delivered (bug 28).
        let mut failed: Option<(usize, inillucent_base::DbError)> = None;
        for expression in &self.expressions {
            match expression.column() {
                Some(index) if batch.is_dense() => {
                    passthrough.push(Some(index));
                    computed.push(Vec::new());
                }
                _ => {
                    passthrough.push(None);
                    let reach = failed.as_ref().map_or(live, |(row, _)| *row);
                    let mut values = Vec::with_capacity(reach);
                    for nth in 0..reach {
                        match expression.value(batch, nth) {
                            Ok(value) => values.push(value),
                            Err(error) => {
                                failed = Some((nth, error));
                                break;
                            }
                        }
                    }
                    computed.push(values);
                }
            }
        }
        let live = failed.as_ref().map_or(live, |(row, _)| *row);
        let borrowed: Vec<Vec<Datum<'_>>> = computed
            .iter()
            .map(|column| column.iter().map(crate::expr::Computed::get).collect())
            .collect();
        let mut columns = Vec::with_capacity(self.expressions.len());
        for (index, source) in passthrough.iter().enumerate() {
            columns.push(match source {
                Some(column) => batch
                    .columns
                    .get(*column)
                    .copied()
                    .unwrap_or(Vector::Const(Datum::Null)),
                None => Vector::Values(borrowed.get(index).map(|v| v.as_slice()).unwrap_or(&[])),
            });
        }
        let projected = Batch::new(live, columns);
        let Some((_, error)) = failed else {
            return self.downstream.push(&projected);
        };
        // SQLite evaluates a row's columns before it starts the next row, so the
        // rows ahead of the one that failed are answered and the failure comes
        // after them. A failure further down the pipeline belongs to an earlier
        // row than this one, so it is the one reported.
        // A downstream that already has all the rows it wants (a `LIMIT`) never
        // reaches the failing row, as in SQLite.
        if live > 0 && self.downstream.push(&projected)? == Flow::Stop {
            return Ok(Flow::Stop);
        }
        Err(error)
    }

    fn finish(&mut self) -> DbResult<()> {
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.downstream.reset()
    }
}
/// Passes at most `limit` rows through, after skipping `offset`.
pub struct Limit {
    limit: usize,
    offset: usize,
    pub(crate) seen: usize,
    pub(crate) emitted: usize,
    downstream: Box<dyn Sink>,
}
impl Limit {
    /// Returns a limit.
    ///
    /// @param limit - how many rows to pass
    /// @param offset - how many to skip first
    /// @param downstream - what to push the surviving rows into
    pub fn new(limit: usize, offset: usize, downstream: Box<dyn Sink>) -> Limit {
        Limit {
            limit,
            offset,
            seen: 0,
            emitted: 0,
            downstream,
        }
    }
}
impl Sink for Limit {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        let mut selection: Vec<u32> = Vec::new();
        for nth in 0..batch.live() {
            // The limit is checked before the row is counted, so `seen` reports
            // how many rows the operator actually consumed rather than how many
            // it looked at on its way to stopping.
            if self.emitted >= self.limit {
                break;
            }
            self.seen = self.seen.saturating_add(1);
            if self.seen <= self.offset {
                continue;
            }
            self.emitted = self.emitted.saturating_add(1);
            selection.push(batch.row_at(nth) as u32);
        }
        if !selection.is_empty() {
            let limited = Batch {
                rows: batch.rows,
                selection: Some(&selection),
                columns: batch.columns.clone(),
            };
            self.downstream.push(&limited)?;
        }
        Ok(if self.emitted >= self.limit {
            Flow::Stop
        } else {
            Flow::Continue
        })
    }

    fn finish(&mut self) -> DbResult<()> {
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.seen = 0;
        self.emitted = 0;
        self.downstream.reset()
    }
}

/// Throws away the first `offset` rows and passes every later row on.
///
/// **It answers what the operator below it answers.** [`Limit`] answers `Stop`
/// from its own count and drops whatever its downstream said, which is right for
/// the last operator in a chain. A `LIMIT` that sits below this operator decides
/// when the source can stop, and a recursive query with no end of its own relies
/// on it: an operator here that answered `Continue` for ever made
/// `WITH RECURSIVE r(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM r) SELECT n FROM
/// r WHERE n%7=0 LIMIT 3 OFFSET 2` run until it ran out of memory.
pub struct Skip {
    /// What a reset puts back, or `None` when the count is shared and belongs to the caller.
    offset: Option<usize>,
    remaining: SkipBudget,
    downstream: Box<dyn Sink>,
}

/// How many rows are still to be thrown away, shared by the operators that draw on it.
///
/// **The arms of a `UNION ALL` share one `OFFSET`.** Each arm is a pipeline of its own, and the
/// rows the first arm did not have are skipped from the second, so each arm's [`Skip`] takes from
/// the same count.
pub type SkipBudget = std::rc::Rc<std::cell::Cell<usize>>;

impl Skip {
    /// Returns an operator that skips rows.
    ///
    /// @param offset - how many rows to throw away first
    /// @param downstream - what to push the later rows into
    pub fn new(offset: usize, downstream: Box<dyn Sink>) -> Skip {
        Skip {
            offset: Some(offset),
            remaining: std::rc::Rc::new(std::cell::Cell::new(offset)),
            downstream,
        }
    }

    /// Returns an operator that skips rows from a count other operators draw on too.
    ///
    /// A reset leaves the count alone: whoever made it decides when it starts again.
    ///
    /// @param budget - the shared count of rows still to throw away
    /// @param downstream - what to push the later rows into
    pub fn shared(budget: SkipBudget, downstream: Box<dyn Sink>) -> Skip {
        Skip {
            offset: None,
            remaining: budget,
            downstream,
        }
    }
}
impl Sink for Skip {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        let mut remaining = self.remaining.get();
        if remaining == 0 {
            return self.downstream.push(batch);
        }
        let mut selection: Vec<u32> = Vec::new();
        for nth in 0..batch.live() {
            if remaining > 0 {
                remaining -= 1;
                continue;
            }
            selection.push(batch.row_at(nth) as u32);
        }
        self.remaining.set(remaining);
        if selection.is_empty() {
            return Ok(Flow::Continue);
        }
        let kept = Batch {
            rows: batch.rows,
            selection: Some(&selection),
            columns: batch.columns.clone(),
        };
        self.downstream.push(&kept)
    }

    fn finish(&mut self) -> DbResult<()> {
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        if let Some(offset) = self.offset {
            self.remaining.set(offset);
        }
        self.downstream.reset()
    }
}

//! The lateral join: a table-valued function driven once per outer row.
//!
//! Invariant: the module is re-run for every outer row, and the arguments it is
//! run with are evaluated against *that* row. That is the whole of what makes
//! `FROM t, json_each(t.d)` mean what it says, and it is why this is an
//! operator rather than a plan shape: every other inner term in this executor
//! is either a tree it seeks into or a set of rows read once, and a table
//! whose *arguments* change per outer row is neither.
//!
//! # Why it is not a materialised stage
//!
//! An ordinary virtual table on the inner side is materialised: its rows are
//! read once and paired with every outer row, because nothing about it depends
//! on which outer row is being paired. A table-valued function whose argument
//! reads an outer column has a different answer per outer row, so materialising
//! it once would pair every outer row with the rows belonging to whichever row
//! happened to be read first - which is not a slow answer, it is a wrong one.
//!
//! # Why the rows are collected rather than streamed
//!
//! One outer row produces one call, and the call's rows are collected before
//! they are emitted. The module's cursor protocol is `filter`/`next`/`column`
//! against a `Context` that borrows the connection, and threading that borrow
//! through a sink that is itself being pushed into would mean holding the
//! connection mutably across a downstream call. The collection is per outer
//! row, so what is held is one row's worth of expansion rather than the whole
//! join.

use inillucent_base::{error::misuse, DbResult};
use inillucent_sql::bind::ColumnUse;
use inillucent_sql::catalog_view::TableInfo;
use inillucent_sql::plan::AccessPath;
use inillucent_tree::datum::OwnedDatum;

use crate::batch::Batch;
use crate::expr::Eval;
use crate::ops::{emit_rows, Flow, Sink};
use crate::physical::{Params, TreeCatalog};

/// What a lateral join evaluates against each outer row.
pub struct OuterRowTests {
    /// One expression per offered constraint, over the outer row.
    pub arguments: Vec<Box<dyn Eval>>,
    /// Conditions on the outer row that must hold before the module is called.
    ///
    /// **Without them the module is called for rows the `WHERE` removes.**
    /// `FROM sqlite_master m, pragma_foreign_key_check(m.name) WHERE m.type =
    /// 'table'` called the function for index rows too, and it fails for a
    /// name that is not a table.
    pub gate: Vec<Box<dyn Eval>>,
    /// Whether the join is a `LEFT JOIN`, so an outer row that no module row
    /// matches is still produced, extended with NULLs.
    pub left: bool,
    /// The `ON` condition of a `LEFT JOIN`, over the outer row followed by the
    /// module's row. A module row that fails it does not match.
    pub on: Option<Box<dyn Eval>>,
}

/// Joins each outer row to the rows a module answers for it.
pub struct LateralModule<'t> {
    /// The module's instance, which names what to run.
    table: TableInfo,
    /// The access path the planner chose, carrying the module's own choice.
    path: AccessPath,
    /// The values bound to `?1`, `?2`, ...
    params: Params,
    /// Which of the module's columns the query reads.
    needed: ColumnUse,
    /// One expression per offered constraint, over the outer row.
    arguments: Vec<Box<dyn Eval>>,
    /// Conditions on the outer row that must hold before the module is called.
    gate: Vec<Box<dyn Eval>>,
    /// Whether an outer row with no matching module row is still produced.
    left: bool,
    /// The `ON` condition of a `LEFT JOIN`.
    on: Option<Box<dyn Eval>>,
    /// Where the module's rows come from.
    catalog: &'t dyn TreeCatalog,
    /// How wide the module's rows are, so a call answering nothing still pads.
    width: usize,
    /// What to push joined rows into.
    downstream: Box<dyn Sink + 't>,
}

impl<'t> LateralModule<'t> {
    /// Returns a lateral join over one module.
    ///
    /// @param table - the FROM term's table, which names the module's instance
    /// @param path - the access path the planner chose
    /// @param params - the values bound to `?1`, `?2`, ...
    /// @param needed - which of the term's columns the query reads
    /// @param tests - the arguments and the outer row conditions to evaluate for each outer row
    /// @param catalog - where the module's rows come from
    /// @param width - how many columns the module declares
    /// @param downstream - what to push joined rows into
    pub fn new(
        table: TableInfo,
        path: AccessPath,
        params: Params,
        needed: ColumnUse,
        tests: OuterRowTests,
        catalog: &'t dyn TreeCatalog,
        width: usize,
        downstream: Box<dyn Sink + 't>,
    ) -> LateralModule<'t> {
        LateralModule {
            table,
            path,
            params,
            needed,
            arguments: tests.arguments,
            gate: tests.gate,
            left: tests.left,
            on: tests.on,
            catalog,
            width,
            downstream,
        }
    }
}

impl LateralModule<'_> {
    /// Reports whether an outer row satisfies every condition on the outer terms.
    ///
    /// A condition that is NULL rejects the row, as it does in a `WHERE`.
    ///
    /// @param batch - the outer rows
    /// @param nth - which live row of the batch to test
    fn outer_row_passes(&self, batch: &Batch<'_>, nth: usize) -> DbResult<bool> {
        for condition in &self.gate {
            let verdict = condition.value(batch, nth)?;
            if crate::expr::truth(&verdict.get()) != Some(true) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl LateralModule<'_> {
    /// Reports whether a joined row meets the `ON` condition of a `LEFT JOIN`.
    ///
    /// A join with no condition keeps every row. A NULL condition rejects the
    /// row, as it does in a `WHERE`.
    ///
    /// @param joined - the outer row followed by the module's row
    fn on_holds(&self, joined: &[OwnedDatum]) -> DbResult<bool> {
        let Some(condition) = &self.on else {
            return Ok(true);
        };
        let borrowed: Vec<inillucent_tree::datum::Datum<'_>> =
            joined.iter().map(OwnedDatum::borrow).collect();
        let columns: Vec<crate::batch::Vector<'_>> = borrowed
            .iter()
            .map(|value| crate::batch::Vector::Const(*value))
            .collect();
        let batch = Batch::new(1, columns);
        let verdict = condition.value(&batch, 0)?;
        Ok(crate::expr::truth(&verdict.get()) == Some(true))
    }
}

impl Sink for LateralModule<'_> {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        let outer_width = batch.columns.len();
        for nth in 0..batch.live() {
            if !self.outer_row_passes(batch, nth)? {
                continue;
            }
            let mut supplied = Vec::with_capacity(self.arguments.len());
            for expression in &self.arguments {
                supplied.push(OwnedDatum::from_datum(&expression.value(batch, nth)?.get()));
            }
            let Some(rows) = self.catalog.virtual_rows_supplied(
                &self.table,
                &self.path,
                &self.params,
                &self.needed,
                &supplied,
            )?
            else {
                return Err(misuse("a virtual table the caller does not have"));
            };
            if rows.is_empty() && !self.left {
                continue;
            }
            // The outer row is copied once and reused for every row the module
            // answered, which is where the expansion is paid for.
            let mut outer = Vec::with_capacity(outer_width);
            for column in 0..outer_width {
                outer.push(OwnedDatum::from_datum(&batch.value(nth, column)?));
            }
            let mut produced: Vec<Vec<OwnedDatum>> = Vec::with_capacity(rows.len());
            for row in rows {
                let mut joined = outer.clone();
                joined.extend(row);
                joined.resize(outer_width.saturating_add(self.width), OwnedDatum::Null);
                if self.on_holds(&joined)? {
                    produced.push(joined);
                }
            }
            if produced.is_empty() && self.left {
                let mut padded = outer;
                padded.resize(outer_width.saturating_add(self.width), OwnedDatum::Null);
                produced.push(padded);
            }
            if produced.is_empty() {
                continue;
            }
            if emit_rows(&produced, self.downstream.as_mut())? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        Ok(Flow::Continue)
    }

    fn finish(&mut self) -> DbResult<()> {
        self.downstream.finish()
    }

    /// Forgets nothing, because the operator holds nothing between rows.
    ///
    /// The module's rows are collected per outer row and emitted before the
    /// next one is read, so there is no accumulated state a re-run would have
    /// to clear - only the chain below.
    fn reset(&mut self) -> DbResult<()> {
        self.downstream.reset()
    }
}

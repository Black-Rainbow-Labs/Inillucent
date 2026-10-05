//! Keeping what an `INSERT` compiles between executions of one prepared statement.
//!
//! Invariant: **a kept setup is used only while it is what compiling the
//! statement now would build**: the table's layout is the same object the
//! catalog holds, the connection's settings and the supplied types switch are
//! the ones it was built under, and building it read nothing but parameters.
//! Those are the three tests [`crate::dml::UpdateSetup`] reuses on, for the
//! same reasons; see `update_setup`.
//!
//! ## What it saves
//!
//! An insert compiled its plan, its table's declarations and every `VALUES`
//! expression on each execution, and a prepared single row insert executes
//! many times: on the hillclimb plan's `churn.refill`, 2,000 inserts in one
//! transaction, compiling the `VALUES` row was 10% of the statement and the
//! rest of the compile another 4%. A parameter is read when an expression is
//! evaluated, through the cell every `Expr::Parameter` holds, so a kept
//! expression answers the next execution's values once that cell is pointed
//! at them.

use std::rc::Rc;

use super::*;
use crate::expr::Eval;
use crate::physical::Bindings;

/// What an `INSERT` builds before it writes a row, kept between executions.
pub struct InsertSetup {
    /// The plan, the row space, the layout and the declarations.
    compiled: CompiledInsert,
    /// Each `VALUES` row's compiled expressions, or `None` for a `SELECT`
    /// source, whose rows the caller supplies.
    values: Option<Vec<Vec<Box<dyn Eval>>>>,
    /// The cell every `Expr::Parameter` in the compiled pieces reads.
    bindings: Bindings,
    /// Whether nothing but a parameter was read while this was built.
    reusable: bool,
    /// The connection's settings when this was built.
    settings: crate::scalar::Context,
    /// Whether supplied values kept their types when this was built, which
    /// decides whether the declarations apply affinities.
    keeps_supplied_types: bool,
}

/// Where a prepared insert keeps its setup between executions.
pub type InsertCache = std::cell::RefCell<Option<Rc<InsertSetup>>>;

/// Applies an `INSERT`, reusing the setup a previous execution built.
///
/// A view is written through its trigger, which compiles nothing to keep, so
/// it goes the way [`insert`] sends it.
///
/// @param statement - the bound insert
/// @param target - the file and its trees
/// @param params - the bound parameters
/// @param supplied - the rows a `SELECT` source produced, empty for `VALUES`
/// @param cache - where the setup is kept between executions
pub fn insert_cached(
    statement: &BoundInsert,
    target: &mut dyn WriteTarget,
    params: &Params,
    supplied: &[Row],
    cache: &InsertCache,
) -> DbResult<Changes> {
    if statement.table.kind == TableKind::View {
        return insert(statement, target, params, supplied);
    }
    let depth = Depth::outermost(statement.on_conflict);
    note_single_row(statement, target, depth);
    let written = insert_setup(cache, statement, target, params).and_then(|setup| {
        let rows: std::borrow::Cow<'_, [Row]> = match &setup.values {
            Some(values) => std::borrow::Cow::Owned(evaluate_values(values, &setup)?),
            None => std::borrow::Cow::Borrowed(supplied),
        };
        insert_rows(statement, target, params, &rows, depth, &setup.compiled)
    });
    written.map_err(|error| outer_unwind(error, statement.on_conflict))
}

/// Returns the setup kept for this statement when it is still valid, and
/// builds and keeps a new one when it is not.
///
/// @param cache - where the setup is kept between executions
/// @param statement - the bound insert
/// @param target - the file and its trees
/// @param params - the bound parameters
fn insert_setup(
    cache: &InsertCache,
    statement: &BoundInsert,
    target: &dyn WriteTarget,
    params: &Params,
) -> DbResult<Rc<InsertSetup>> {
    let layout = layout_of(target, &statement.table)?;
    if let Some(held) = cache.borrow().as_ref() {
        if held.reusable
            && Rc::ptr_eq(&held.compiled.layout, &layout)
            && held.settings == params.settings()
            && held.keeps_supplied_types == params.keeps_supplied_types()
        {
            crate::dml::update::adopt_bindings(&held.bindings, params);
            return Ok(Rc::clone(held));
        }
    }
    let built = Rc::new(build_insert_setup(statement, target, params)?);
    if built.reusable {
        *cache.borrow_mut() = Some(Rc::clone(&built));
    }
    Ok(built)
}

/// Compiles an insert's plan, declarations and `VALUES` expressions.
///
/// @param statement - the bound insert
/// @param target - the file and its trees
/// @param params - the bound parameters
fn build_insert_setup(
    statement: &BoundInsert,
    target: &dyn WriteTarget,
    params: &Params,
) -> DbResult<InsertSetup> {
    let before = params.reads();
    let compiled = compile_insert(statement, target, params)?;
    let values = match &statement.source {
        BoundInsertSource::Values(rows) => {
            let catalog = target.catalog();
            let mut compiled_rows = Vec::with_capacity(rows.len());
            for row in rows {
                let mut cells: Vec<Box<dyn Eval>> = Vec::with_capacity(row.len());
                for expr in row {
                    cells.push(compiled.space.compile(expr, params, catalog)?);
                }
                compiled_rows.push(cells);
            }
            Some(compiled_rows)
        }
        _ => None,
    };
    Ok(InsertSetup {
        compiled,
        values,
        bindings: params.bindings(),
        reusable: params.reads() == before,
        settings: params.settings(),
        keeps_supplied_types: params.keeps_supplied_types(),
    })
}

/// Evaluates a kept `VALUES` list into the rows it supplies.
///
/// The same evaluation `rows_to_insert` makes after compiling the list.
///
/// @param values - each row's compiled expressions
/// @param setup - the setup they were compiled in
fn evaluate_values(values: &[Vec<Box<dyn Eval>>], setup: &InsertSetup) -> DbResult<Vec<Row>> {
    let mut built = Vec::with_capacity(values.len());
    for row in values {
        let mut cells = Vec::with_capacity(row.len());
        for eval in row {
            cells.push(setup.compiled.space.evaluate(eval.as_ref(), &[])?);
        }
        built.push(cells);
    }
    Ok(built)
}

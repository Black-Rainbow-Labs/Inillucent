//! The subqueries a write evaluates for each row: `RETURNING`, an `UPDATE`'s
//! `SET`, and an upsert's `DO UPDATE`.
//!
//! Invariant: **a subquery in one of these runs against the table as it is when
//! the row is reached, and a row's `RETURNING` sees the row after it is
//! written.** SQLite evaluates `RETURNING` for each row straight after writing
//! it, so `UPDATE t SET b = b + 10 RETURNING b, (SELECT sum(b) FROM t)` reports
//! 610, 620 and 630 where a subquery answered once before the statement says 600
//! for every row. The three share the operator `crate::correlate` that a
//! `SELECT` uses, so there is one implementation of what a correlated block
//! means.

use inillucent_base::DbResult;
use inillucent_sql::bind::{BoundExpr, EXCLUDED_SOURCE};
use inillucent_tree::datum::OwnedDatum;

use super::{RowSpace, WriteTarget};
use crate::correlate::Correlation;
use crate::expr::Eval;
use crate::insert_plan::InsertPlan;
use crate::physical::{Params, SourceLayout, TreeCatalog};

/// Answers every prepared block against one row image.
///
/// @param correlated - the prepared blocks
/// @param target - the file and its trees, as they are now
/// @param params - the bound parameters
/// @param row - the row image, in tree-column order
pub(crate) fn answer_correlations(
    correlated: &[Correlation],
    target: &dyn WriteTarget,
    params: &Params,
    row: &[OwnedDatum],
) -> DbResult<Vec<OwnedDatum>> {
    if correlated.is_empty() {
        return Ok(Vec::new());
    }
    let catalog = target.catalog();
    // One set for every block of this row, written into rather than cloned per
    // block: `without_subqueries` copies the whole parameter vector, and the
    // blocks write only their own numbers, which are past anything a statement
    // can write.
    let mut bare = params.without_subqueries();
    let mut answers = Vec::with_capacity(correlated.len());
    for correlation in correlated {
        answers.push(correlation.answer(catalog, &mut bare, row)?);
    }
    Ok(answers)
}

/// Returns how an outer reference maps onto the cells of a row.
///
/// The target's own columns are the tree columns of the first image. When the
/// statement is an upsert the `excluded` row follows it, so its columns are the
/// same tree columns one image further along.
///
/// @param source - the statement-wide number of the target's FROM term
/// @param excluded - whether `excluded` is an image after the target's
/// @param layout - the table tree's layout
pub(crate) fn image_resolver(
    source: usize,
    excluded: bool,
    layout: &SourceLayout,
) -> impl Fn(&BoundExpr) -> Option<usize> + '_ {
    move |expr: &BoundExpr| match expr {
        BoundExpr::Column {
            source: held,
            column,
            ..
        } if *held == source => layout.slots.get(usize::from(*column)).copied().flatten(),
        BoundExpr::Rowid { source: held } if *held == source => layout.rowid,
        BoundExpr::Column {
            source: held,
            column,
            ..
        } if excluded && *held == EXCLUDED_SOURCE => layout
            .slots
            .get(usize::from(*column))
            .copied()
            .flatten()
            .map(|slot| slot.saturating_add(layout.width)),
        BoundExpr::Rowid { source: held } if excluded && *held == EXCLUDED_SOURCE => {
            layout.rowid.map(|slot| slot.saturating_add(layout.width))
        }
        _ => None,
    }
}

/// Prepares the subqueries of a `RETURNING` clause, every one of them.
///
/// @param returning - the expressions the clause returns
/// @param source - the statement-wide number of the target's FROM term
/// @param layout - the table tree's layout
/// @param target_root - the root page of the table the statement changes
/// @param catalog - where the subqueries' trees are read
pub(crate) fn returning_correlations(
    returning: &[&BoundExpr],
    source: usize,
    layout: &SourceLayout,
    target_root: u32,
    catalog: &dyn TreeCatalog,
) -> DbResult<Vec<Correlation>> {
    crate::correlate::correlations_in_every(
        returning,
        catalog,
        &image_resolver(source, false, layout),
        target_root,
    )
}

/// Pads a list of answers so it holds one cell for every prepared block.
///
/// The row space has a cell per block, in the order the blocks were prepared,
/// and an expression reads the cell of its own block only.
///
/// @param answers - the answers to the blocks that were run
/// @param before - how many blocks come before the ones answered
/// @param total - how many blocks there are
pub(crate) fn placed(answers: Vec<OwnedDatum>, before: usize, total: usize) -> Vec<OwnedDatum> {
    let mut cells = vec![OwnedDatum::Null; before];
    cells.extend(answers);
    cells.resize(total, OwnedDatum::Null);
    cells
}

/// Evaluates a statement's `RETURNING` columns for one written row.
///
/// @param space - the statement's row space
/// @param evals - the compiled `RETURNING` columns
/// @param row - the row as it was written
/// @param answers - the row's subquery answers, placed by number
pub(super) fn evaluate_returning(
    space: &RowSpace,
    evals: &[Box<dyn Eval>],
    row: &[OwnedDatum],
    answers: &[OwnedDatum],
) -> DbResult<Vec<OwnedDatum>> {
    let mut out = Vec::with_capacity(evals.len());
    for eval in evals {
        out.push(space.evaluate_with(eval.as_ref(), &[row], answers)?);
    }
    Ok(out)
}

/// Builds the row space of a statement whose subqueries are answered per row.
///
/// @param sources - the statement-wide numbers of the row images
/// @param layout - the target table's layout
/// @param subqueries - the prepared subqueries, whose numbers get answer cells
pub(super) fn space_with_subqueries(
    sources: &[usize],
    layout: &std::rc::Rc<SourceLayout>,
    subqueries: &[Correlation],
) -> RowSpace {
    RowSpace::new(sources, layout).with_correlations(
        &subqueries
            .iter()
            .map(|held| held.id)
            .collect::<Vec<usize>>(),
    )
}

/// Evaluates an `INSERT`'s `RETURNING` columns for the row just written.
///
/// **`RETURNING` is evaluated for the row straight after it is written**, before
/// the `AFTER` triggers: a subquery that keeps its first answer keeps the one
/// from before they ran.
///
/// @param plan - the compiled insert
/// @param space - the statement's row space
/// @param target - the file and its trees, as they are now
/// @param params - the bound parameters
/// @param row - the row as it was written
pub(super) fn returned_for_insert(
    plan: &InsertPlan,
    space: &RowSpace,
    target: &dyn WriteTarget,
    params: &Params,
    row: &[OwnedDatum],
) -> DbResult<Option<Vec<OwnedDatum>>> {
    if plan.returning.is_empty() {
        return Ok(None);
    }
    let answers = plan.returning_answers(target, params, row)?;
    Ok(Some(evaluate_returning(
        space,
        &plan.returning,
        row,
        &answers,
    )?))
}

/// Answers the subqueries of an `UPDATE`'s assignments for the row about to change.
///
/// The answers are placed in the cells numbered for them, ahead of the cells of
/// the `RETURNING` subqueries.
///
/// @param correlated - every prepared subquery, the assignments' first
/// @param assigned_count - how many of `correlated` belong to the assignments
/// @param target - the file and its trees, as they are now
/// @param params - the bound parameters
/// @param before - the row as it was read
pub(super) fn assigned_answers(
    correlated: &[Correlation],
    assigned_count: usize,
    target: &dyn WriteTarget,
    params: &Params,
    before: &[OwnedDatum],
) -> DbResult<Vec<OwnedDatum>> {
    let blocks = correlated.get(..assigned_count).unwrap_or(&[]);
    Ok(placed(
        answer_correlations(blocks, target, params, before)?,
        0,
        correlated.len(),
    ))
}

/// Evaluates an `UPDATE`'s `RETURNING` columns for the row just written.
///
/// **`RETURNING` is evaluated for the row straight after it is written**,
/// against the table as it is then, before the `AFTER` triggers run.
///
/// @param space - the statement's row space
/// @param projected - the compiled `RETURNING` columns
/// @param held - every prepared subquery and how many of them belong to the assignments
/// @param target - the file and its trees, as they are now
/// @param params - the bound parameters
/// @param after - the row as it was written
pub(super) fn returned_for_update(
    space: &RowSpace,
    projected: &[Box<dyn Eval>],
    held: (&[Correlation], usize),
    target: &dyn WriteTarget,
    params: &Params,
    after: &[OwnedDatum],
) -> DbResult<Option<Vec<OwnedDatum>>> {
    if projected.is_empty() {
        return Ok(None);
    }
    let (correlated, assigned_count) = held;
    let blocks = correlated.get(assigned_count..).unwrap_or(&[]);
    let answers = placed(
        answer_correlations(blocks, target, params, after)?,
        assigned_count,
        correlated.len(),
    );
    Ok(Some(evaluate_returning(space, projected, after, &answers)?))
}

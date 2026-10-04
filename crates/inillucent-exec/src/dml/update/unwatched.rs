//! An `UPDATE` of many rows that nothing can watch, a leaf at a time.
//!
//! Invariant: **each row is decided exactly as the row at a time path decides
//! it, and written with the same records.** Only how the rows reach the tree
//! changes: in the table's order, through `PagedTree::update_sorted`, which
//! reads each row out of a leaf it has already parsed and writes the leaf once
//! for every row whose one changed column fits where it lies (task-2180). A row
//! that needs anything else - a moved key, a changed index entry, more than one
//! changed column, a value that does not fit its slot - is written by the row
//! at a time path, `replace_row`, as before.
//!
//! ## What "nothing can watch" means
//!
//! No trigger, no foreign key action, no `RETURNING`, no captured changes, no
//! `FROM`, and no correlated subquery in an assignment. Each of those can see
//! the table part way through the statement or depends on the order the rows
//! are written in, and with none of them neither is observable: a row's new
//! value is computed from that row alone, and the uniqueness of the keys is
//! still checked for every row whose key or index entries change, because
//! those rows take the row at a time path. A row that fails part way stops the
//! statement after the rows before it are written, as the row at a time path
//! does.
//!
//! ## When the rows may be put in the table's order
//!
//! The row at a time path writes the rows in the order the query found them,
//! which is often an index's order: `UPDATE t SET body = ...` scans the
//! smallest index that covers the key. Writing them in the table's order
//! instead changes what the statement leaves whenever a failure keeps the rows
//! before it, `OR FAIL`, or a unique collision keeps the first row to arrive,
//! `OR IGNORE` and `OR REPLACE`. The first version of this path sorted the keys
//! regardless, and `UPDATE OR FAIL` over a table read in index order kept a
//! different set of rows than the row at a time path did. So the keys are put
//! in the table's order only when every conflict the statement can meet undoes
//! the whole statement; otherwise the run is used only when the query already
//! found them in the table's order, and the row at a time path runs when it
//! did not.

use inillucent_base::error::misuse;
use inillucent_sql::ast::ConflictAction;
use inillucent_sql::catalog_view::TableInfo;
use inillucent_tree::datum::Datum;
use inillucent_tree::write::{Rewrite, UpdateRun};

use super::*;
use crate::dml::index::{index_entry, maintained};

/// What the decision for the last row asked about holds back for the row at a time path.
enum Held {
    /// Nothing: the row was written by the run, or skipped.
    Nothing,
    /// One column's new value, which the run could not fit in its slot.
    Column(usize, OwnedDatum),
    /// The whole new row.
    Row(Vec<OwnedDatum>),
}

/// Runs an `UPDATE` a leaf at a time, or returns `None` when something can watch it.
///
/// @param statement - the bound update
/// @param target - the file and its trees
/// @param params - the bound parameters
/// @param keys - the rows the plan found
/// @param depth - how many triggers deep this write already is
/// @param setup - the statement's compiled pieces
/// @param captured - whether the target records the rows a statement changes
pub(super) fn update_unwatched(
    statement: &BoundUpdate,
    target: &mut dyn WriteTarget,
    params: &Params,
    keys: &[Row],
    depth: Depth,
    setup: &UpdateSetup,
    captured: bool,
) -> DbResult<Option<Changes>> {
    if !unwatched(statement, setup, captured, keys.len()) {
        return Ok(None);
    }
    let table = &statement.table;
    let width = setup.layout.key_columns.len();
    let prefixes: Vec<&[OwnedDatum]> = keys
        .iter()
        .map(|row| row.get(..width).unwrap_or(row))
        .collect();
    let Some(ordered) = in_tree_order(statement, target, prefixes)? else {
        return Ok(None);
    };
    let borrowed: Vec<Vec<Datum<'_>>> = ordered
        .into_iter()
        .map(|key| key.iter().map(OwnedDatum::borrow).collect())
        .collect();
    let mut changes = Changes::default();
    let mut near = None;
    let mut at = 0usize;
    let mut run = UpdateRun::default();
    while let Some(rest) = borrowed.get(at..).filter(|rest| !rest.is_empty()) {
        let mut held = Held::Nothing;
        let outcome = {
            let (database, trees, log) = target.parts_for(table.root)?;
            let tree = trees
                .get_mut(table.root)
                .ok_or_else(|| missing_tree(table))?;
            tree.update_sorted(
                database,
                log,
                rest,
                &mut near,
                &mut |before| decide(statement, setup, before, &mut held),
                &mut run,
            )
        };
        for _ in 0..run.counted {
            count_row(&mut changes, target, depth);
        }
        outcome?;
        at = at.saturating_add(run.taken);
        if let Some(before) = run.pending.take() {
            // A run that asked about later rows before it handed this one back
            // says what was decided for it; `held` is the last row's.
            if let Some((column, value)) = run.pending_column.take() {
                held = Held::Column(column, value);
            }
            if write_held(statement, setup, target, params, depth, &before, held)? {
                count_row(&mut changes, target, depth);
            }
            at = at.saturating_add(1);
        }
    }
    Ok(Some(changes))
}

/// Returns the keys in the table's order, or `None` when the run cannot be used.
///
/// See the module's note on when the order may be changed. A key the query
/// returned twice would be written twice by the row at a time path, reading
/// its own first write the second time, so a list with a repeat is left to
/// that path as well.
///
/// @param statement - the bound update
/// @param target - the file and its trees
/// @param keys - each row's key, in the order the query found them
fn in_tree_order<'k>(
    statement: &BoundUpdate,
    target: &mut dyn WriteTarget,
    keys: Vec<&'k [OwnedDatum]>,
) -> DbResult<Option<Vec<&'k [OwnedDatum]>>> {
    let found = keys.clone();
    let sorted = super::super::delete::sorted_by_tree(target, statement.table.root, keys)?;
    if sorted.len() != found.len() {
        return Ok(None);
    }
    if order_matters(statement) && sorted != found {
        return Ok(None);
    }
    Ok(Some(sorted))
}

/// Reports whether the order rows are written in can change what the statement leaves.
///
/// It can when a conflict keeps what was written before it: `FAIL`, `IGNORE`
/// or `REPLACE`, written on the statement or, when the statement names none,
/// on a constraint. The statement's own clause replaces every constraint's,
/// as SQLite applies it.
///
/// @param statement - the bound update
fn order_matters(statement: &BoundUpdate) -> bool {
    let keeps = |action: Option<ConflictAction>| {
        !matches!(
            action,
            None | Some(ConflictAction::Abort) | Some(ConflictAction::Rollback)
        )
    };
    if statement.on_conflict.is_some() {
        return keeps(statement.on_conflict);
    }
    let table = &statement.table;
    table
        .columns
        .iter()
        .any(|column| keeps(column.not_null_conflict))
        || table.indexes.iter().any(|index| keeps(index.conflict))
        || keeps(rowid_conflict(table))
}

/// Reports whether nothing can watch this statement's rows change.
///
/// See the module's note for why each of these is a reason to go a row at a
/// time. Fewer than two rows has nothing to batch.
///
/// @param statement - the bound update
/// @param setup - the statement's compiled pieces
/// @param captured - whether the target records the rows a statement changes
/// @param rows - how many rows the plan found
fn unwatched(statement: &BoundUpdate, setup: &UpdateSetup, captured: bool, rows: usize) -> bool {
    statement.triggers.is_empty()
        && statement.table.foreign_key_triggers.is_empty()
        && statement.returning.is_empty()
        && !captured
        && !setup.joined
        && setup.correlated.is_empty()
        && rows >= 2
}

/// Decides one row's new value, the way `update_at_cached` does for a row nothing watches.
///
/// The assignments read the row as it is, then the stored generated columns
/// and the affinities, then `NOT NULL`, `STRICT` types and `CHECK` in SQLite's
/// order. A row the run cannot write keeps its new value in `held`.
///
/// @param statement - the bound update
/// @param setup - the statement's compiled pieces
/// @param before - the row as it is
/// @param held - set to what the row at a time path needs, when it needs it
fn decide(
    statement: &BoundUpdate,
    setup: &UpdateSetup,
    before: &[OwnedDatum],
    held: &mut Held,
) -> DbResult<Rewrite> {
    *held = Held::Nothing;
    let table = &statement.table;
    let layout = &*setup.layout;
    let mut after = before.to_vec();
    for (slot, eval) in &setup.assignments {
        // Every assignment reads the before image, as the row at a time path.
        let value = setup.space.evaluate_with(eval.as_ref(), &[before], &[])?;
        if let Some(cell) = after.get_mut(*slot) {
            *cell = value;
        }
    }
    convert_after(
        &setup.space,
        &setup.generated,
        &setup.declarations,
        &[],
        &mut after,
    )?;
    check_key_is_integer(layout, &mut after)?;
    let resolution = resolution_of(statement.on_conflict);
    let declarations = &setup.declarations;
    if !declarations_are_met(
        table,
        layout,
        declarations,
        &setup.space,
        &mut after,
        resolution,
    )? {
        return Ok(Rewrite::Skip);
    }
    declarations.types_are_met(table, &setup.space, &after)?;
    if !declarations.checks_are_met(&setup.space, &after, resolution == Resolution::Skip)? {
        return Ok(Rewrite::Skip);
    }
    if !same_key(layout, before, &after) || entries_move(table, layout, setup, before, &after)? {
        *held = Held::Row(after);
        return Ok(Rewrite::Row);
    }
    match difference(before, &after) {
        Difference::Nothing => Ok(Rewrite::Unchanged),
        Difference::One(column) if column >= layout.key_columns.len() => {
            let Some(value) = after.get(column).cloned() else {
                return Err(misuse("a changed column is not in the row"));
            };
            *held = Held::Column(column, value.clone());
            Ok(Rewrite::Column(column, value))
        }
        _ => {
            *held = Held::Row(after);
            Ok(Rewrite::Row)
        }
    }
}

/// Reports whether any index entry of the row differs between its two images.
///
/// A row whose entries all stay where they are cannot collide with another
/// row in a unique index and needs no index write, which is what lets the run
/// write it. The same comparison `place_row` makes.
///
/// @param table - the table being written
/// @param layout - the table tree's layout
/// @param setup - the statement's compiled pieces
/// @param before - the row as it is
/// @param after - the row as it will be
fn entries_move(
    table: &TableInfo,
    layout: &SourceLayout,
    setup: &UpdateSetup,
    before: &[OwnedDatum],
    after: &[OwnedDatum],
) -> DbResult<bool> {
    let indexes = IndexExprs::new(&setup.declarations, &setup.space);
    for (position, index) in maintained(table) {
        let entry = |row: &[OwnedDatum]| -> DbResult<Option<Row>> {
            match indexes.holds(position, row)? {
                true => Ok(Some(index_entry(position, index, layout, row, indexes)?)),
                false => Ok(None),
            }
        };
        if entry(before)? != entry(after)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Writes the row a run handed back, the row at a time way.
///
/// Returns whether it was written: a conflict the statement resolves by
/// ignoring skips it.
///
/// @param statement - the bound update
/// @param setup - the statement's compiled pieces
/// @param target - the file and its trees
/// @param params - the bound parameters
/// @param depth - how many triggers deep this write already is
/// @param before - the row as it is
/// @param held - its new value, as `decide` kept it
fn write_held(
    statement: &BoundUpdate,
    setup: &UpdateSetup,
    target: &mut dyn WriteTarget,
    params: &Params,
    depth: Depth,
    before: &[OwnedDatum],
    held: Held,
) -> DbResult<bool> {
    let after = match held {
        Held::Row(after) => after,
        Held::Column(column, value) => {
            let mut after = before.to_vec();
            if let Some(cell) = after.get_mut(column) {
                *cell = value;
            }
            after
        }
        Held::Nothing => return Err(misuse("a run handed back a row nothing was decided for")),
    };
    let layout = &*setup.layout;
    let request = WriteRequest {
        layout,
        params,
        depth,
        indexes: IndexExprs::new(&setup.declarations, &setup.space),
        declarations: &setup.declarations,
    };
    if !resolve_key_conflicts(statement, target, &after, before, request)? {
        return Ok(false);
    }
    let indexes = IndexExprs::new(&setup.declarations, &setup.space);
    replace_row(&statement.table, layout, target, before, &after, indexes)?;
    Ok(true)
}

//! `DELETE`, and the triggers it fires on the way.
//!
//! Invariant: **a statement that removes a row removes its index entries
//! too.** An entry left behind is a seek that finds a row that has been
//! deleted, which is a wrong answer rather than a leak. When a trigger, a
//! foreign key action or `RETURNING` can watch, a row's entries go before the
//! row does, one row at a time. When nothing can watch, the rows go first in
//! the table's order and the entries after in each index's order (task-2077,
//! see `delete_unwatched`), and a statement that fails part way is undone
//! whole.

use index::{index_entry, maintained, plain_index_entry, write_index_entry};

use inillucent_base::DbResult;
use inillucent_sql::ast::TriggerTime;
use inillucent_sql::catalog_view::{TableInfo, TableKind};
use inillucent_sql::dml::BoundDelete;
use inillucent_tree::datum::{Datum, OwnedDatum};

use super::*;
use crate::declared::{BoundDeclarations, IndexExprs, WriteDeclarations};
use crate::physical::{Params, SourceLayout};
use crate::trigger::{self, Depth};

/// Applies a `DELETE` to rows a query has already selected.
///
/// @param statement - the bound delete
/// @param target - the file and its trees
/// @param params - the bound parameters
/// @param keys - the key of each row the `WHERE` selected
pub fn delete(
    statement: &BoundDelete,
    target: &mut dyn WriteTarget,
    params: &Params,
    keys: &[Row],
) -> DbResult<Changes> {
    delete_at(statement, target, params, keys, Depth::default())
}
/// Applies a `DELETE` that is already some triggers deep.
///
/// @param statement - the bound delete
/// @param target - the file and its trees
/// @param params - the bound parameters
/// @param keys - the key of each row the `WHERE` selected
/// @param depth - how many triggers deep this write already is
pub fn delete_at(
    statement: &BoundDelete,
    target: &mut dyn WriteTarget,
    params: &Params,
    keys: &[Row],
    depth: Depth,
) -> DbResult<Changes> {
    let table = &statement.table;
    if table.kind == TableKind::View {
        return delete_view(statement, target, params, keys, depth);
    }
    let layout = layout_of(target, table)?;
    let catalog = target.catalog();
    let returned: Vec<&inillucent_sql::bind::BoundExpr> = statement
        .returning
        .iter()
        .map(|column| &column.expr)
        .collect();
    let correlated =
        returning_correlations(&returned, statement.source, &layout, table.root, catalog)?;
    let space = RowSpace::new(&sources_for(statement.source, &statement.triggers), &layout)
        .with_correlations(
            &correlated
                .iter()
                .map(|held| held.id)
                .collect::<Vec<usize>>(),
        );
    // A delete declares no `CHECK` to meet, but it does have to know which of
    // the table's indexes hold the row it is removing: an entry only comes out
    // of a partial index if the predicate accepted the row, and an index key the
    // table does not carry has to be recomputed to be found.
    let declarations = WriteDeclarations::compile(
        table,
        &layout,
        // A `DELETE` writes no value, so no `CHECK` applies, no default can
        // stand in for one, and no virtual column has a value to check.
        BoundDeclarations {
            checks: &[],
            defaults: &[],
            virtual_columns: &[],
            index_exprs: &statement.index_exprs,
        },
        &space,
        params,
        catalog,
    )?;
    let mut projected = Vec::with_capacity(statement.returning.len());
    for column in &statement.returning {
        projected.push(space.compile(&column.expr, params, catalog)?);
    }

    let captured = target.captures(table.root);
    if let Some(ordered) = in_tree_order(statement, target, keys, captured)? {
        let indexes = IndexExprs::new(&declarations, &space);
        return delete_unwatched(table, &layout, target, ordered, indexes, depth);
    }
    let mut changes = Changes::default();
    for key in keys {
        let Some(row) = read_row(table, target, key)? else {
            continue;
        };
        // The row was read a moment ago for `RETURNING` and for the index
        // entries; reading it again inside the removal was a second descent per
        // delete, on the workload the gate measures two thousand of.
        // **`RETURNING` names the row that went, and is evaluated after it has
        // gone**, before the `AFTER` triggers. A subquery in it sees the table
        // without this row and without the rows before it, as SQLite's does; a
        // row a `BEFORE` trigger skipped is not returned.
        let mut returned: Option<Vec<OwnedDatum>> = None;
        if remove_then(
            table,
            target,
            key,
            &row,
            &statement.triggers,
            WriteRequest {
                layout: &layout,
                params,
                depth,
                indexes: IndexExprs::new(&declarations, &space),
                declarations: &declarations,
            },
            false,
            &mut |gone| {
                if projected.is_empty() {
                    return Ok(());
                }
                let answers = answer_correlations(&correlated, gone, params, &row)?;
                let mut out = Vec::with_capacity(projected.len());
                for eval in &projected {
                    out.push(space.evaluate_with(eval.as_ref(), &[row.as_slice()], &answers)?);
                }
                returned = Some(out);
                Ok(())
            },
        )? {
            count_row(&mut changes, target, depth);
            if let Some(out) = returned {
                changes.returned.push(out);
            }
            if captured {
                changes.removed.push(row);
            }
        }
    }
    Ok(changes)
}
/// Returns the keys a delete removes in the order the table's tree holds them,
/// or `None` when something can observe the order they go in.
///
/// **The query that chose the keys decides their order, and the order decides
/// how many pages the delete reads** (task-2077). `DELETE FROM big WHERE a % 2
/// = 0` over `big(a INTEGER PRIMARY KEY, b, d)` with `big_d ON big(d)` is
/// planned as `SCAN big USING COVERING INDEX big_d`, because the index is the
/// smaller tree and holds every rowid. So the keys come back in `(d, a)`
/// order, and each key is 1,013 rows past the one before it. At a 32,768 byte
/// page a leaf holds about 890 rows, so every key is in a different leaf and
/// one value of `d` visits almost every leaf of the table. Past the pool that
/// is one read per deleted row: 176,503 reads and 175,507 writes for 250,000
/// deletes on a table of 558 leaves (`inillucent-writeprofile --spread 500000
/// 8`, release build). At 4,096 bytes one value of `d` visits about 494 leaves
/// and the next 112 values visit the same ones, which the pool holds, so the
/// same delete read 9,875 pages. That difference is why the story measured the
/// 32 KiB delete at 5.4 times the cost per row.
///
/// SQLite's two pass delete orders its rowids the same way: it collects them
/// into a `RowSet`, which hands them back sorted.
///
/// **Only when the order is invisible.** A trigger or a foreign key action
/// (which is a trigger here) sees the rows one at a time, `RETURNING` lists
/// them in the order they went, and a captured table reports them in that
/// order too. With any of those the keys keep the order the query produced.
///
/// @param statement - the bound delete
/// @param target - the file and its trees
/// @param keys - the key of each row the `WHERE` selected
/// @param captured - whether the removed rows are reported to the caller
fn in_tree_order<'k>(
    statement: &BoundDelete,
    target: &mut dyn WriteTarget,
    keys: &'k [Row],
    captured: bool,
) -> DbResult<Option<Vec<&'k [OwnedDatum]>>> {
    let table = &statement.table;
    let observable = !statement.triggers.is_empty()
        || !table.foreign_key_triggers.is_empty()
        || !statement.returning.is_empty()
        || captured;
    if observable || keys.len() < 2 {
        return Ok(None);
    }
    Ok(Some(sorted_by_tree(
        target,
        table.root,
        keys.iter().map(Vec::as_slice).collect(),
    )?))
}
/// Sorts tuples into the order one tree holds them, dropping repeats.
///
/// By the tree's own key encoding, so a collation, a descending column or a
/// `WITHOUT ROWID` key sorts the way the tree does. A repeat is dropped because
/// the caller counts every tuple it is handed as a row removed, and a key the
/// query returned twice is still one row.
///
/// @param target - the file and its trees
/// @param root - the tree whose order to use
/// @param tuples - the key tuples, or the index entries
pub(super) fn sorted_by_tree<'k>(
    target: &mut dyn WriteTarget,
    root: u32,
    tuples: Vec<&'k [OwnedDatum]>,
) -> DbResult<Vec<&'k [OwnedDatum]>> {
    let (_, trees, _) = target.parts_for(root)?;
    let Some(tree) = trees.get(root) else {
        return Ok(tuples);
    };
    // **A rowid is an integer, and integers are compared as integers
    // (task-2183).** Encoding each key to bytes to sort by was an allocation per
    // key, 5.5% of a 40,000 row range delete whose keys came from a scan of the
    // same tree and so were in order already. An ascending integer key sorts in
    // integer order, and keys that are in order are checked, not sorted.
    let ascending_integer_key = tree.key_columns() == 1
        && !tree.directions().first().copied().unwrap_or(false)
        && tuples
            .iter()
            .all(|tuple| matches!(tuple, [OwnedDatum::Int(_)]));
    if ascending_integer_key {
        let integer = |tuple: &[OwnedDatum]| match tuple.first() {
            Some(OwnedDatum::Int(value)) => *value,
            _ => i64::MIN,
        };
        let mut tuples = tuples;
        let ordered = tuples
            .windows(2)
            .all(|pair| match (pair.first(), pair.get(1)) {
                (Some(left), Some(right)) => integer(left) < integer(right),
                _ => true,
            });
        if !ordered {
            tuples.sort_by_key(|tuple| integer(tuple));
            tuples.dedup_by_key(|tuple| integer(tuple));
        }
        return Ok(tuples);
    }
    // **Every key encoded into one buffer** (task-2185), each named by its
    // range. A vector per key and a second one for its borrowed values were
    // three allocations a key and their regrowth, 3.4% of the hillclimb plan's
    // `churn.delete.half`, whose two index trees take this branch.
    let encoding = tree.key_encoding();
    let mut bytes: Vec<u8> = Vec::with_capacity(tuples.len().saturating_mul(24));
    let mut borrowed: Vec<Datum<'_>> = Vec::new();
    let mut encoded: Vec<(std::ops::Range<usize>, &'k [OwnedDatum])> =
        Vec::with_capacity(tuples.len());
    for tuple in tuples {
        borrowed.clear();
        borrowed.extend(tuple.iter().map(OwnedDatum::borrow));
        let start = bytes.len();
        encoding.encode_into(&borrowed, tree.collations(), tree.directions(), &mut bytes);
        encoded.push((start..bytes.len(), tuple));
    }
    let key = |range: &std::ops::Range<usize>| bytes.get(range.clone()).unwrap_or(&[]);
    encoded.sort_by(|left, right| key(&left.0).cmp(key(&right.0)));
    encoded.dedup_by(|later, earlier| key(&later.0) == key(&earlier.0));
    Ok(encoded.into_iter().map(|(_, tuple)| tuple).collect())
}
/// Deletes rows nothing can watch go, visiting each tree in its own order.
///
/// **Ordering the table's keys is half of it** (task-2077). The first version
/// of this fix ordered the keys and still removed each row together with its
/// index entries, and `story_large_table_nightly` failed its read check at
/// 32,768 bytes: 285,098 reads of a 2,221 page file, 280,611 of them leaves of
/// `big_d`. With the keys in `a` order a row's entry is `(a % 1013, a)`, so
/// consecutive rows land in different leaves of the index, and 506 rows visit
/// all 388 of them against a pool budget of 256 frames.
///
/// So the rows go first, in the table's order, and each row's index entries
/// are collected from the row the table's own delete hands back. Then every
/// index's entries are sorted into that index's order and removed. Each tree
/// is walked from one end to the other once, and each index leaf is visited
/// once whatever the pool can keep of the index.
///
/// **Why the rows go first and are not read first.** A version that read every
/// row, then removed the entries, then the rows, failed `vacuum_on_vfs`: 2,000
/// rows in about 110 leaves under one root, a 64 frame pool, and "every frame
/// in the buffer pool is pinned". A descent holds its root pinned while it
/// loads a leaf, and a leaf whose parent is pinned cannot be cooled, so reading
/// more leaves of one root than the pool has frames stops the pool. Removing
/// each row as it is reached empties leaves and lets them merge, which is what
/// the per row delete always did. The same effect let the read first version
/// pass the story only by taking 132 frames past the pool's budget.
///
/// **The module invariant holds at the scale of the statement, not the row.**
/// Here a row goes before its index entries, so part way through the statement
/// an index names rows that are gone. Nothing can look: this path runs only
/// with no trigger, no foreign key action and no `RETURNING`, and a statement
/// that fails part way is undone whole - the write path keeps an undo buffer
/// per statement, which is SQLite's `ABORT`. The entries are held in memory,
/// one per row per index, beside the keys the caller already holds.
///
/// @param table - the table being written
/// @param layout - the table tree's layout
/// @param target - the file and its trees
/// @param keys - the rows' keys, in the table's order
/// @param indexes - the compiled keys and predicates of the table's indexes
/// @param depth - how many triggers deep this write already is
fn delete_unwatched(
    table: &TableInfo,
    layout: &SourceLayout,
    target: &mut dyn WriteTarget,
    keys: Vec<&[OwnedDatum]>,
    indexes: IndexExprs<'_>,
    depth: Depth,
) -> DbResult<Changes> {
    let mut entries: Vec<Vec<Row>> = maintained(table).map(|_| Vec::new()).collect();
    let has_maintained = !entries.is_empty();
    let mut changes = Changes::default();
    // **One buffer of values, and a slice of it per key** (task-2185). A vector
    // per key was 40,000 allocations for `edge.delete.range`, and dropping
    // them was 8% of the statement.
    let values: Vec<Datum<'_>> = keys
        .iter()
        .flat_map(|key| key.iter().map(OwnedDatum::borrow))
        .collect();
    let borrowed = key_slices(&values, keys.iter().map(|key| key.len()));
    // The leaf the last run used, per tree: the keys arrive in each tree's
    // order, so the next run nearly always starts there (task-2175). Each call
    // deletes the keys one leaf holds with one change to it (task-2180), and
    // the rows are counted after each, so a failure part way leaves the count
    // at the rows that went. See `PagedTree::delete_sorted`.
    // One undo image per row for the table and one for each index entry, so
    // a log that keeps them sizes its buffer once (task-2185).
    let images = keys.len().saturating_mul(entries.len().saturating_add(1));
    target.parts_for(table.root)?.2.expect_undo(images);
    let mut near = None;
    let mut at = 0usize;
    while let Some(rest) = borrowed.get(at..).filter(|rest| !rest.is_empty()) {
        let mut removed = 0usize;
        let (database, trees, log) = target.parts_for(table.root)?;
        let tree = trees
            .get_mut(table.root)
            .ok_or_else(|| missing_tree(table))?;
        // A table with no index to keep is not shown its rows, so the run
        // copies none out (task-2183).
        at = at.saturating_add(tree.delete_sorted(
            database,
            log,
            rest,
            &mut near,
            &mut |row| {
                removed = removed.saturating_add(1);
                // The row borrows from a copy of its leaf (task-2185). An
                // index that computes nothing takes the values it keeps; one
                // with a predicate or a computed key is evaluated over an owned
                // copy of the row, which is what its expressions read.
                if indexes.computes_nothing() {
                    for ((_, index), held) in maintained(table).zip(entries.iter_mut()) {
                        held.push(plain_index_entry(index, layout, row)?);
                    }
                    return Ok(());
                }
                let owned: Vec<OwnedDatum> = row.iter().map(OwnedDatum::from_datum).collect();
                for ((position, index), held) in maintained(table).zip(entries.iter_mut()) {
                    if indexes.holds(position, &owned)? {
                        held.push(index_entry(position, index, layout, &owned, indexes)?);
                    }
                }
                Ok(())
            },
            has_maintained,
        )?);
        for _ in 0..removed {
            count_row(&mut changes, target, depth);
        }
    }
    for ((_, index), held) in maintained(table).zip(entries.iter()) {
        let ordered = sorted_by_tree(target, index.root, held.iter().map(Vec::as_slice).collect())?;
        let values: Vec<Datum<'_>> = ordered
            .iter()
            .flat_map(|entry| entry.iter().map(OwnedDatum::borrow))
            .collect();
        let sorted = key_slices(&values, ordered.iter().map(|entry| entry.len()));
        let mut near = None;
        let mut at = 0usize;
        while let Some(rest) = sorted.get(at..).filter(|rest| !rest.is_empty()) {
            let (database, trees, log) = target.parts_for(index.root)?;
            let Some(tree) = trees.get_mut(index.root) else {
                break;
            };
            at = at.saturating_add(tree.delete_sorted(
                database,
                log,
                rest,
                &mut near,
                &mut |_| Ok(()),
                false,
            )?);
        }
    }
    Ok(changes)
}
/// Cuts a buffer of key values into one slice per key.
///
/// @param values - every key's values, one key after another
/// @param lengths - how many values each key has, in the same order
pub(super) fn key_slices<'v, 'd>(
    values: &'v [Datum<'d>],
    lengths: impl Iterator<Item = usize>,
) -> Vec<&'v [Datum<'d>]> {
    let mut start = 0usize;
    lengths
        .map(|length| {
            let end = start.saturating_add(length);
            let key = values.get(start..end).unwrap_or(&[]);
            start = end;
            key
        })
        .collect()
}
/// Removes one row, firing the `BEFORE` and `AFTER` triggers around it.
///
/// Returns whether the row was actually removed: a `RAISE(IGNORE)` in a
/// `BEFORE` body abandons it, which is not a failure and not a change.
///
/// The one place a delete happens with triggers around it, so a `DELETE`
/// statement and the delete a `REPLACE` performs to make room cannot fire
/// different things - which they would the moment there were two copies of
/// this.
///
/// @param table - the table being written
/// @param layout - the table tree's layout
/// @param target - the file and its trees
/// @param key - the row's key
/// @param row - the row as it is
/// @param triggers - the triggers this delete fires
/// @param request - the layout, parameters and depth of the write
/// @param replacing - whether a conflict is removing the row, which fires only
///   the foreign key actions unless `recursive_triggers` is on
pub(crate) fn remove_with_triggers(
    table: &TableInfo,
    target: &mut dyn WriteTarget,
    key: &[OwnedDatum],
    row: &[OwnedDatum],
    triggers: &[inillucent_sql::dml::BoundTrigger],
    request: WriteRequest<'_>,
    replacing: bool,
) -> DbResult<bool> {
    remove_then(
        table,
        target,
        key,
        row,
        triggers,
        request,
        replacing,
        &mut |_| Ok(()),
    )
}
/// Removes one row like [`remove_with_triggers`], running a step between the
/// removal and the `AFTER` triggers.
///
/// `RETURNING` is evaluated there: after the row is gone and before anything the
/// triggers write.
///
/// @param table - the table being written
/// @param target - the file and its trees
/// @param key - the row's key
/// @param row - the row as it is
/// @param triggers - the triggers this delete fires
/// @param request - the layout, parameters and depth of the write
/// @param replacing - whether a conflict is removing the row
/// @param between - what to do once the row has been removed
fn remove_then(
    table: &TableInfo,
    target: &mut dyn WriteTarget,
    key: &[OwnedDatum],
    row: &[OwnedDatum],
    triggers: &[inillucent_sql::dml::BoundTrigger],
    request: WriteRequest<'_>,
    replacing: bool,
    between: &mut dyn FnMut(&mut dyn WriteTarget) -> DbResult<()>,
) -> DbResult<bool> {
    let WriteRequest {
        layout,
        params,
        depth,
        indexes,
        ..
    } = request;
    let fire = if replacing {
        trigger::fire_replaced
    } else {
        trigger::fire
    };
    if fire(
        triggers,
        TriggerTime::Before,
        target,
        &trigger::TriggerFiring {
            rows: trigger::TriggerRows {
                old: Some(row),
                new: None,
            },
            slots: &layout.slots,
            rowid: layout.rowid,
            params,
            depth,
        },
    )? == trigger::Fired::SkipRow
    {
        return Ok(false);
    }
    // A `BEFORE` body may have removed the row itself - `ON DELETE CASCADE` on
    // a self-referencing key does exactly that - so the removal is skipped
    // rather than repeated when it is already gone.
    if !row_exists(table, target, key)? {
        return Ok(false);
    }
    remove_row(table, layout, target, key, row, indexes)?;
    between(target)?;
    fire(
        triggers,
        TriggerTime::After,
        target,
        &trigger::TriggerFiring {
            rows: trigger::TriggerRows {
                old: Some(row),
                new: None,
            },
            slots: &layout.slots,
            rowid: layout.rowid,
            params,
            depth,
        },
    )?;
    Ok(true)
}
/// Removes one row and every index entry that named it.
///
/// @param table - the table being written
/// @param layout - the table tree's layout
/// @param target - the file and its trees
/// @param key - the row's key
/// @param row - the row as it stands, which the caller has already read
pub(crate) fn remove_row(
    table: &TableInfo,
    layout: &SourceLayout,
    target: &mut dyn WriteTarget,
    key: &[OwnedDatum],
    row: &[OwnedDatum],
    indexes: IndexExprs<'_>,
) -> DbResult<()> {
    for (position, index) in maintained(table) {
        if !indexes.holds(position, row)? {
            continue;
        }
        let entry = index_entry(position, index, layout, row, indexes)?;
        write_index_entry(index, target, &entry, false)?;
    }
    let (database, trees, log) = target.parts_for(table.root)?;
    let tree = trees
        .get_mut(table.root)
        .ok_or_else(|| missing_tree(table))?;
    let borrowed: Vec<Datum<'_>> = key.iter().map(OwnedDatum::borrow).collect();
    tree.delete(database, log, &borrowed)?;
    Ok(())
}
/// Fires a view's `INSTEAD OF DELETE` triggers, storing nothing.
///
/// @param statement - the bound delete, whose target is the view
/// @param target - the file and its trees
/// @param params - the bound parameters
/// @param rows - the view's rows, as its own query produced them
/// @param depth - how many triggers deep this write already is
fn delete_view(
    statement: &BoundDelete,
    target: &mut dyn WriteTarget,
    params: &Params,
    rows: &[Row],
    depth: Depth,
) -> DbResult<Changes> {
    let layout = view_layout(&statement.table);
    let space = RowSpace::new(&sources_for(statement.source, &statement.triggers), &layout);
    let catalog = target.catalog();
    let mut projected = Vec::with_capacity(statement.returning.len());
    for column in &statement.returning {
        projected.push(space.compile(&column.expr, params, catalog)?);
    }
    let mut changes = Changes::default();
    for before in rows {
        if trigger::fire(
            &statement.triggers,
            TriggerTime::InsteadOf,
            target,
            &trigger::TriggerFiring {
                rows: trigger::TriggerRows {
                    old: Some(before.as_slice()),
                    new: None,
                },
                slots: &layout.slots,
                rowid: layout.rowid,
                params,
                depth,
            },
        )? == trigger::Fired::SkipRow
        {
            continue;
        }
        count_view_row(&mut changes);
        if !projected.is_empty() {
            let mut out = Vec::with_capacity(projected.len());
            for eval in &projected {
                out.push(space.evaluate(eval.as_ref(), &[before.as_slice()])?);
            }
            changes.returned.push(out);
        }
    }
    Ok(changes)
}

//! A tree that gained a column after its leaves were written.
//!
//! Invariant: **a leaf with fewer columns than its tree reads as if it had them, holding
//! the tree's default for each, and the first write that lands in it rewrites it with all
//! of them.** `ALTER TABLE ... ADD COLUMN` changes no leaf, so every read of a table that
//! was altered goes through a leaf of the old width until something writes to that leaf.
//! The cases compare the tree with a model after each step, and count leaves, so a
//! widening that wrote far more pages than the rows need fails by name.

use std::collections::BTreeMap;

use inillucent_pool::{Database, Options};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::types::{ColumnSpec, PhysicalType};
use inillucent_tree::write::NoLog;
use inillucent_tree::PagedTree;
use inillucent_vfs::{DbPath, MemoryVfs};

/// The columns the tree was built with: a key and a label.
fn narrow() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::key(PhysicalType::Int64),
        ColumnSpec::new(PhysicalType::Text),
    ]
}

/// The columns after `ALTER TABLE ... ADD COLUMN` three times: an integer, a text and a real.
fn wide() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::key(PhysicalType::Int64),
        ColumnSpec::new(PhysicalType::Text),
        ColumnSpec::new(PhysicalType::Int64),
        ColumnSpec::new(PhysicalType::Text),
        ColumnSpec::new(PhysicalType::Float64),
    ]
}

/// What the three added columns read as in a leaf written without them.
fn fills() -> Vec<OwnedDatum> {
    vec![
        OwnedDatum::Null,
        OwnedDatum::Null,
        OwnedDatum::Int(5),
        OwnedDatum::Text(b"a default of some length, so a widened row is larger".to_vec()),
        OwnedDatum::Null,
    ]
}

/// Builds a tree of `rows` narrow rows at a small page size, so it has many leaves.
///
/// @param rows - how many rows
fn fixture(rows: i64) -> (Database, PagedTree) {
    let vfs = MemoryVfs::new();
    let path = DbPath::new("short-leaves.rdb");
    let mut database = Database::create(
        &vfs,
        &path,
        Options::default().with_page_size(4_096).with_frames(512),
    )
    .expect("a database");
    let owned: Vec<Vec<OwnedDatum>> = (0..rows)
        .map(|key| {
            vec![
                OwnedDatum::Int(key),
                OwnedDatum::Text(format!("label-{key:06}").into_bytes()),
            ]
        })
        .collect();
    let borrowed: Vec<Vec<Datum<'_>>> = owned
        .iter()
        .map(|row| row.iter().map(OwnedDatum::borrow).collect())
        .collect();
    let tree = PagedTree::bulk_build(&mut database, 11, narrow(), 1, &borrowed).expect("a build");
    (database, tree)
}

/// What every row of the model holds: the label, then the three defaults.
///
/// @param key - the row's key
fn defaulted(key: i64) -> Vec<OwnedDatum> {
    let mut row = vec![
        OwnedDatum::Int(key),
        OwnedDatum::Text(format!("label-{key:06}").into_bytes()),
    ];
    row.extend(fills().into_iter().skip(2));
    row
}

/// Asserts the tree holds exactly the model's rows, scanned and probed.
///
/// @param database - the file
/// @param tree - the tree
/// @param model - the rows it should hold, by key
/// @param context - what to say when it fails
fn assert_agrees(
    database: &Database,
    tree: &PagedTree,
    model: &BTreeMap<i64, Vec<OwnedDatum>>,
    context: &str,
) {
    let scanned = tree.rows(database.pool()).expect("the tree scans");
    let expected: Vec<Vec<OwnedDatum>> = model.values().cloned().collect();
    assert_eq!(scanned.len(), expected.len(), "{context}: row count");
    assert_eq!(scanned, expected, "{context}: the rows differ");
    for key in model.keys().step_by(97) {
        let probe = [Datum::Int(*key)];
        let found = tree
            .point(database.pool(), &probe)
            .expect("the probe runs")
            .expect("the key is there");
        assert_eq!(Some(&found), model.get(key), "{context}: probe of {key}");
    }
    tree.check(database.pool())
        .unwrap_or_else(|error| panic!("{context}: integrity failed: {error:?}"));
}

/// A tree given a longer column list reads every old row with the defaults and writes nothing.
#[test]
fn old_leaves_read_with_the_defaults_and_nothing_is_written() {
    let (database, mut tree) = fixture(3_000);
    let leaves = tree.leaf_count();
    let written = database.pool().stats().writes;
    tree.set_shape(wide(), fills());
    let model: BTreeMap<i64, Vec<OwnedDatum>> =
        (0..3_000).map(|key| (key, defaulted(key))).collect();
    assert_agrees(&database, &tree, &model, "after set_shape");
    assert_eq!(
        tree.leaf_count(),
        leaves,
        "set_shape changed the leaf count"
    );
    assert_eq!(
        database.pool().stats().writes,
        written,
        "reading a short leaf wrote a page"
    );
}

/// The first insert into a short leaf widens it, and the leaf count grows by a small number.
#[test]
fn the_first_write_into_a_leaf_widens_that_leaf_only() {
    let (mut database, mut tree) = fixture(3_000);
    let leaves = tree.leaf_count();
    tree.set_shape(wide(), fills());
    let mut model: BTreeMap<i64, Vec<OwnedDatum>> =
        (0..3_000).map(|key| (key, defaulted(key))).collect();
    let mut log = NoLog::default();
    let row = [
        Datum::Int(1_500),
        Datum::Text(b"changed"),
        Datum::Int(9),
        Datum::Text(b"text"),
        Datum::Real(2.5),
    ];
    tree.insert(&mut database, &mut log, &row)
        .expect("the insert");
    model.insert(
        1_500,
        vec![
            OwnedDatum::Int(1_500),
            OwnedDatum::Text(b"changed".to_vec()),
            OwnedDatum::Int(9),
            OwnedDatum::Text(b"text".to_vec()),
            OwnedDatum::Real(2.5),
        ],
    );
    assert_agrees(&database, &tree, &model, "after one insert");
    // Rows of 14 bytes widened by about 70 take a leaf from one page to about six, so one
    // leaf turns into a handful and not into one page per row.
    let grown = tree.leaf_count().saturating_sub(leaves);
    assert!(
        (1..=12).contains(&grown),
        "one widened leaf became {grown} more leaves"
    );
}

/// Every leaf is widened by an insert into it, in any order, and the tree stays whole.
#[test]
fn inserts_across_the_tree_widen_every_leaf() {
    let (mut database, mut tree) = fixture(2_000);
    tree.set_shape(wide(), fills());
    let mut model: BTreeMap<i64, Vec<OwnedDatum>> =
        (0..2_000).map(|key| (key, defaulted(key))).collect();
    let mut log = NoLog::default();
    for key in (0..2_000).step_by(37).chain([2_500, 2_501, -3]) {
        let row = [
            Datum::Int(key),
            Datum::Text(b"new"),
            Datum::Int(key),
            Datum::Null,
            Datum::Null,
        ];
        tree.insert(&mut database, &mut log, &row)
            .expect("the insert");
        model.insert(
            key,
            vec![
                OwnedDatum::Int(key),
                OwnedDatum::Text(b"new".to_vec()),
                OwnedDatum::Int(key),
                OwnedDatum::Null,
                OwnedDatum::Null,
            ],
        );
    }
    assert_agrees(&database, &tree, &model, "after inserts");
}

/// A delete from a short leaf works without widening it, and the rows left read right.
#[test]
fn a_delete_from_a_short_leaf_leaves_the_other_rows_reading_the_defaults() {
    let (mut database, mut tree) = fixture(2_000);
    tree.set_shape(wide(), fills());
    let mut model: BTreeMap<i64, Vec<OwnedDatum>> =
        (0..2_000).map(|key| (key, defaulted(key))).collect();
    let mut log = NoLog::default();
    for key in (0..2_000).step_by(3) {
        let removed = tree
            .delete(&mut database, &mut log, &[Datum::Int(key)])
            .expect("the delete");
        assert_eq!(
            removed.as_ref(),
            model.get(&key),
            "the row a delete returns"
        );
        model.remove(&key);
    }
    assert_agrees(&database, &tree, &model, "after deletes");
}

/// A leaf whose rows were all deleted takes the next write, and is replaced by an empty leaf
/// with every column.
#[test]
fn a_short_leaf_with_no_live_row_is_replaced_by_an_empty_full_leaf() {
    let (mut database, mut tree) = fixture(2_000);
    tree.set_shape(wide(), fills());
    let mut model: BTreeMap<i64, Vec<OwnedDatum>> =
        (0..2_000).map(|key| (key, defaulted(key))).collect();
    let mut log = NoLog::default();
    // Every row of the first few leaves goes.
    for key in 0..400 {
        tree.delete(&mut database, &mut log, &[Datum::Int(key)])
            .expect("the delete");
        model.remove(&key);
    }
    let row = [
        Datum::Int(10),
        Datum::Text(b"back"),
        Datum::Int(1),
        Datum::Null,
        Datum::Real(1.5),
    ];
    tree.insert(&mut database, &mut log, &row)
        .expect("the insert");
    model.insert(
        10,
        vec![
            OwnedDatum::Int(10),
            OwnedDatum::Text(b"back".to_vec()),
            OwnedDatum::Int(1),
            OwnedDatum::Null,
            OwnedDatum::Real(1.5),
        ],
    );
    assert_agrees(
        &database,
        &tree,
        &model,
        "after the insert into an emptied leaf",
    );
}

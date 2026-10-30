//! Rows of several hundred integer columns, appended one at a time, share leaves.
//!
//! Invariant: **a table of 450 to 790 integer columns loaded by appends takes a small multiple of
//! the leaves a bulk build of the same rows takes.** The leaves of such a table spend most of their
//! page on the column directory and the class arrays, and a row waiting in the delta area is several
//! kilobytes, so the rule that asks a packed leaf for a third of the page free on top of the arriving
//! row could never be met and every full leaf split.
//!
//! ## Why this exists
//!
//! 300 rows of 600 integer columns took 146 leaves appended against 13 built in bulk (bug hunt 11),
//! and a scan of the table read 11 times what SQLite's read. The leaf count is asserted, because it
//! is what decides how many pages a scan, an update and a `VACUUM` touch, and it does not depend on
//! how loaded the machine is.

use std::collections::BTreeMap;

use inillucent_pool::{Database, Options};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::types::{ColumnSpec, PhysicalType};
use inillucent_tree::write::NoLog;
use inillucent_tree::PagedTree;
use inillucent_vfs::{DbPath, MemoryVfs};

/// Returns a key column and `width` integer columns.
///
/// @param width - how many integer columns follow the key
fn columns_of(width: usize) -> Vec<ColumnSpec> {
    let mut columns = vec![ColumnSpec::key(PhysicalType::Int64)];
    columns.extend((0..width).map(|_| ColumnSpec::new(PhysicalType::Int64)));
    columns
}

/// Returns the leaves of a bulk build and of 300 appended rows of `width` integer columns.
///
/// @param width - how many integer columns follow the key
fn leaves_for(width: usize) -> (u64, u64) {
    let rows: Vec<Vec<Datum<'static>>> = (0..300i64)
        .map(|row| {
            let mut values = vec![Datum::Int(row + 1)];
            values.extend((0..width).map(|column| Datum::Int((row * 7 + column as i64) % 100)));
            values
        })
        .collect();
    let vfs = MemoryVfs::new();
    let path = DbPath::new("wide-rows.rdb");
    let mut database = Database::create(
        &vfs,
        &path,
        Options::default().with_page_size(32_768).with_frames(512),
    )
    .expect("a database");
    let bulk = PagedTree::bulk_build(&mut database, 11, columns_of(width), 1, &rows)
        .expect("a bulk build");
    let mut appended = PagedTree::bulk_build(&mut database, 12, columns_of(width), 1, &rows[..0])
        .expect("an empty tree");
    let mut log = NoLog::default();
    for row in &rows {
        appended
            .insert(&mut database, &mut log, row)
            .expect("the insert");
    }
    (bulk.leaf_count(), appended.leaf_count())
}

/// Appended rows of 540 and 600 columns take no more than four times the leaves of a bulk build.
#[test]
fn appended_wide_rows_share_leaves() {
    for width in [540usize, 600] {
        let (bulk, appended) = leaves_for(width);
        assert!(
            appended <= bulk.saturating_mul(4),
            "{width} columns: {appended} leaves appended, {bulk} built in bulk"
        );
    }
}

/// Returns `count` rows of a key and `width` integers below `modulus`.
///
/// @param width - how many integer columns follow the key
/// @param count - how many rows
/// @param modulus - the values are below this
fn small_int_rows(width: usize, count: i64, modulus: i64) -> Vec<Vec<Datum<'static>>> {
    (0..count)
        .map(|row| {
            let mut values = vec![Datum::Int(row + 1)];
            values.extend(
                (0..width)
                    .map(|column| Datum::Int((row * 7919 + column as i64 * 104_729) % modulus)),
            );
            values
        })
        .collect()
}

/// Returns the leaves of 1,000 rows of `width` columns bulk built and appended one at a time.
///
/// @param width - how many integer columns follow the key
/// @param modulus - the values are below this
fn leaves_of_a_thousand(width: usize, modulus: i64) -> (u64, u64) {
    let rows = small_int_rows(width, 1_000, modulus);
    let vfs = MemoryVfs::new();
    let path = DbPath::new("wide-rows-thousand.rdb");
    let mut database = Database::create(
        &vfs,
        &path,
        Options::default().with_page_size(32_768).with_frames(512),
    )
    .expect("a database");
    let bulk = PagedTree::bulk_build(&mut database, 11, columns_of(width), 1, &rows)
        .expect("a bulk build");
    let mut appended = PagedTree::bulk_build(&mut database, 12, columns_of(width), 1, &rows[..0])
        .expect("an empty tree");
    let mut log = NoLog::default();
    for row in &rows {
        appended
            .insert(&mut database, &mut log, row)
            .expect("the insert");
    }
    for (index, row) in rows.iter().enumerate().step_by(97) {
        let key = [row[0]];
        let found = appended
            .point(database.pool(), &key)
            .expect("a lookup")
            .expect("the row");
        assert_eq!(found.len(), width + 1, "row {index} read back");
    }
    (bulk.leaf_count(), appended.leaf_count())
}

/// Rows of 800, 1,200 and 1,900 integer columns take about their own bytes of leaves whether bulk
/// built or appended, and not one leaf to a row.
///
/// The leaf of such a table spends 22 to 50 KiB of a 32 KiB page on the column directory and the
/// class arrays when the rows are in the columnar region, so the rows are kept in the delta area,
/// which costs only their own tagged bytes: 9 for an integer, or 2 for one below 128 in a row of
/// more than half a page. Before the delta area took more than one such row a leaf, 1,000 rows
/// took 1,000 leaves, 32 MB where SQLite takes 2 MB (bug hunt 11).
#[test]
fn rows_of_a_thousand_columns_pack_several_to_a_leaf() {
    for (width, modulus, bytes_a_value) in [
        (800usize, 1i64 << 40, 9u64),
        (1_200, 1 << 40, 9),
        (1_900, 100, 2),
    ] {
        let (bulk, appended) = leaves_of_a_thousand(width, modulus);
        let row_bytes = width as u64 * bytes_a_value + 8;
        let ideal = (1_000 * row_bytes).div_ceil(32_768);
        assert!(
            bulk <= ideal * 3,
            "{width} columns: {bulk} leaves bulk built, {ideal} is the row bytes over the page"
        );
        assert!(
            appended <= ideal * 3,
            "{width} columns: {appended} leaves appended, {ideal} is the row bytes over the page"
        );
    }
}

/// Returns the model row for `key`: the key and 900 integers that depend on it and on `version`.
///
/// @param key - the row's key
/// @param version - 0 for the first write, 1 for an update
fn model_row(key: i64, version: i64) -> Vec<OwnedDatum> {
    let mut row = vec![OwnedDatum::Int(key)];
    row.extend(
        (0..900).map(|column| {
            OwnedDatum::Int((key * 31 + column * 7 + version * 1_000_003) % (1 << 33))
        }),
    );
    row
}

/// Asserts the tree holds exactly the model's rows, scanned, probed and integrity checked.
///
/// @param database - the file
/// @param tree - the tree
/// @param model - the rows it should hold, by key
/// @param context - what to say when it fails
fn assert_tree_matches(
    database: &Database,
    tree: &PagedTree,
    model: &BTreeMap<i64, Vec<OwnedDatum>>,
    context: &str,
) {
    let scanned = tree.rows(database.pool()).expect("the tree scans");
    let expected: Vec<Vec<OwnedDatum>> = model.values().cloned().collect();
    assert_eq!(scanned, expected, "{context}: the scan differs");
    for (key, row) in model {
        let found = tree
            .point(database.pool(), &[Datum::Int(*key)])
            .expect("a lookup");
        assert_eq!(found.as_ref(), Some(row), "{context}: lookup of {key}");
    }
    assert!(tree
        .point(database.pool(), &[Datum::Int(100_000)])
        .expect("a lookup")
        .is_none());
    tree.check(database.pool())
        .unwrap_or_else(|error| panic!("{context}: integrity failed: {error:?}"));
}

/// Leaves of delta rows alone read, update, delete and reopen like any other leaf.
///
/// A leaf built from rows too wide for the columnar region holds several delta rows and no sorted
/// rows, which no leaf had before a leaf kept more than one such row. Every read and write goes
/// through the same code as the leaf of a table's first few inserts, and this checks that it does,
/// at 900 columns, on a tree of about 40 leaves.
#[test]
fn leaves_of_several_delta_rows_read_and_write_like_any_leaf() {
    let vfs = MemoryVfs::new();
    let path = DbPath::new("wide-rows-delta-leaves.rdb");
    let mut database = Database::create(
        &vfs,
        &path,
        Options::default().with_page_size(32_768).with_frames(512),
    )
    .expect("a database");
    let mut tree = PagedTree::bulk_build(
        &mut database,
        11,
        columns_of(900),
        1,
        &Vec::<Vec<Datum<'static>>>::new(),
    )
    .expect("an empty tree");
    let mut log = NoLog::default();
    let mut model: BTreeMap<i64, Vec<OwnedDatum>> = BTreeMap::new();
    for key in 1..=120i64 {
        let row = model_row(key, 0);
        let borrowed: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
        tree.insert(&mut database, &mut log, &borrowed)
            .expect("the insert");
        model.insert(key, row);
    }
    assert!(
        tree.leaf_count() <= 60,
        "{} leaves for 120 rows",
        tree.leaf_count()
    );
    assert_tree_matches(&database, &tree, &model, "after the appends");
    for key in [3i64, 40, 41, 77, 120] {
        let row = model_row(key, 1);
        let borrowed: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
        tree.insert(&mut database, &mut log, &borrowed)
            .expect("an update");
        model.insert(key, row);
    }
    assert_tree_matches(&database, &tree, &model, "after updates");
    for key in [2i64, 41, 42, 43, 44, 45, 46, 90] {
        let key_row = [Datum::Int(key)];
        assert!(tree
            .delete(&mut database, &mut log, &key_row)
            .expect("a delete")
            .is_some());
        model.remove(&key);
    }
    assert_tree_matches(&database, &tree, &model, "after deletes");
    for key in [42i64, 200, 0] {
        let row = model_row(key, 0);
        let borrowed: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
        tree.insert(&mut database, &mut log, &borrowed)
            .expect("an insert between rows");
        model.insert(key, row);
    }
    assert_tree_matches(&database, &tree, &model, "after inserts between rows");
    database.checkpoint().expect("a checkpoint");
    let reopened = Database::open(&vfs, &path, 64).expect("a reopen");
    let attached = PagedTree::attach(
        reopened.pool(),
        11,
        tree.root(),
        columns_of(900),
        1,
        tree.leaf_count(),
        tree.row_count(),
    )
    .expect("the tree attaches");
    assert_tree_matches(&reopened, &attached, &model, "after a reopen");
}

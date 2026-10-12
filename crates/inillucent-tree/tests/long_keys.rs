//! Keys longer than the spill threshold, against a `BTreeMap`.
//!
//! Invariant: **a key of any length up to the key limit is stored, found and
//! ordered the way a short one is, and the tree stays sound after every phase.**
//! A key over `page_size / 8` is written on pages of its own with its first bytes
//! kept in the leaf. A separator over that is shortened at a split, and one that
//! is still over it after shortening is stored out of line in the interior page.
//! Keys that share a prefix longer than the limit are what force the second, so
//! these keys share 2,000 bytes at a 4,096 byte page.

use std::collections::BTreeMap;

use inillucent_pool::{Database, Options};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::types::{ColumnSpec, PhysicalType};
use inillucent_tree::write::NoLog;
use inillucent_tree::PagedTree;
use inillucent_vfs::{DbPath, MemoryVfs};

/// The page size, which makes a key over 512 bytes a key stored out of line.
const PAGE_SIZE: usize = 4_096;

/// Returns a key of `shared` bytes that every key has, then its number.
///
/// @param shared - how many leading bytes every key has in common
/// @param number - which key
fn key_of(shared: usize, number: i64) -> Vec<u8> {
    let mut key = vec![b'p'; shared];
    key.extend_from_slice(format!("{number:05}").as_bytes());
    key
}

/// Returns a row: the key and a short value.
///
/// @param key - the key's bytes
/// @param number - the number the value carries
fn row_of(key: &[u8], number: i64) -> Vec<OwnedDatum> {
    vec![
        OwnedDatum::Text(key.to_vec()),
        OwnedDatum::Text(format!("value {number}").into_bytes()),
    ]
}

/// Opens an empty database over memory.
fn database() -> Database {
    let vfs = MemoryVfs::new();
    Database::create(
        &vfs,
        &DbPath::new("long-keys.rdb"),
        Options::default()
            .with_page_size(PAGE_SIZE)
            .with_frames(512),
    )
    .expect("a database")
}

/// Asserts the tree holds exactly what the model does, by scan and by point.
///
/// @param database - the file
/// @param tree - the tree
/// @param model - what it should hold
/// @param context - what to say when it fails
fn assert_agrees(
    database: &Database,
    tree: &PagedTree,
    model: &BTreeMap<Vec<u8>, i64>,
    context: &str,
) {
    let pool = database.pool();
    tree.check(pool)
        .unwrap_or_else(|error| panic!("{context}: not sound: {:?}", error.detail()));
    let scanned = tree.rows(pool).expect("a scan");
    let keys: Vec<Vec<u8>> = scanned
        .iter()
        .map(|row| match row.first() {
            Some(OwnedDatum::Text(key)) => key.clone(),
            other => panic!("{context}: a key is {other:?}"),
        })
        .collect();
    let wanted: Vec<Vec<u8>> = model.keys().cloned().collect();
    assert_eq!(keys, wanted, "{context}: the scan");
    for (key, number) in model {
        let found = tree
            .point(pool, &[Datum::Text(key)])
            .expect("a point read")
            .unwrap_or_else(|| panic!("{context}: key {number} was not found"));
        assert_eq!(
            found.get(1),
            Some(&OwnedDatum::Text(format!("value {number}").into_bytes())),
            "{context}: the value of key {number}"
        );
    }
}

/// Reports whether the root interior page stores any separator out of line.
///
/// @param database - the file
/// @param tree - the tree, which must have an interior root
fn root_stores_a_separator_out_of_line(database: &Database, tree: &PagedTree) -> bool {
    let guard = database.pool().fetch(tree.root()).expect("the root");
    inillucent_pool::interior::InteriorRef::parse(&guard)
        .expect("an interior root")
        .has_extents()
}

/// Builds a tree in bulk, writes into it, and deletes from it, with keys that
/// share a prefix longer than an interior page keeps whole.
#[test]
fn a_tree_of_keys_sharing_a_long_prefix_stays_sound() {
    let mut database = database();
    let columns = vec![
        ColumnSpec::key(PhysicalType::Text),
        ColumnSpec::new(PhysicalType::Text),
    ];
    let mut model: BTreeMap<Vec<u8>, i64> = BTreeMap::new();
    let mut owned: Vec<Vec<OwnedDatum>> = Vec::new();
    for number in (0..120i64).step_by(2) {
        let key = key_of(2_000, number);
        owned.push(row_of(&key, number));
        model.insert(key, number);
    }
    let borrowed: Vec<Vec<Datum<'_>>> = owned
        .iter()
        .map(|row| row.iter().map(OwnedDatum::borrow).collect())
        .collect();
    let mut tree =
        PagedTree::bulk_build(&mut database, 21, columns, 1, &borrowed).expect("a bulk build");
    assert!(
        tree.height(database.pool()).expect("a height") >= 1,
        "sixty keys of 2 KB fill more than one leaf"
    );
    assert_agrees(&database, &tree, &model, "after the bulk build");
    assert!(
        root_stores_a_separator_out_of_line(&database, &tree),
        "keys that share 2,000 bytes need separators longer than 512"
    );

    let mut log = NoLog::default();
    // The odd numbers arrive between the keys the bulk build placed, in an
    // order that is neither ascending nor descending.
    let arrivals: Vec<i64> = (0..60i64).map(|step| (step * 37 % 60) * 2 + 1).collect();
    for number in arrivals {
        let key = key_of(2_000, number);
        let row = row_of(&key, number);
        let borrowed: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
        tree.put(&mut database, &mut log, &borrowed)
            .unwrap_or_else(|error| panic!("key {number} was refused: {:?}", error.detail()));
        model.insert(key, number);
    }
    assert_agrees(&database, &tree, &model, "after the inserts");

    for number in (0..120i64).filter(|number| number % 3 == 0) {
        let key = key_of(2_000, number);
        let gone = tree
            .delete(&mut database, &mut log, &[Datum::Text(&key)])
            .expect("a delete");
        assert!(gone.is_some(), "key {number} was present");
        model.remove(&key);
    }
    assert_agrees(&database, &tree, &model, "after the deletes");

    // Every page the tree names is named once.
    let pages = tree.pages_occupied(database.pool()).expect("the pages");
    let mut seen = std::collections::HashSet::new();
    for (page, share) in pages {
        if share == inillucent_tree::PageShare::Owned {
            assert!(seen.insert(page), "page {} is held twice", page.0);
        }
    }
}

/// A tree of long keys trimmed from the front hands emptied interior pages to
/// their neighbours, moving separators stored out of line as they are, and stays
/// sound with every page named once.
///
/// The keys share 2,000 bytes, so nearly every separator is out of line, and a
/// dissolve pulls one down from the grandparent into the neighbour. It has to
/// move as the same reference: writing it again would leak the old extent, and
/// freeing it would leave the neighbour naming free pages.
#[test]
fn trimming_long_keys_from_the_front_dissolves_parents_and_keeps_separators() {
    let mut database = database();
    let columns = vec![
        ColumnSpec::key(PhysicalType::Text),
        ColumnSpec::new(PhysicalType::Text),
    ];
    let mut model: BTreeMap<Vec<u8>, i64> = BTreeMap::new();
    let mut owned: Vec<Vec<OwnedDatum>> = Vec::new();
    for number in 0..600i64 {
        let key = key_of(2_000, number);
        owned.push(row_of(&key, number));
        model.insert(key, number);
    }
    let borrowed: Vec<Vec<Datum<'_>>> = owned
        .iter()
        .map(|row| row.iter().map(OwnedDatum::borrow).collect())
        .collect();
    let mut tree =
        PagedTree::bulk_build(&mut database, 22, columns, 1, &borrowed).expect("a bulk build");
    assert!(
        tree.height(database.pool()).expect("a height") >= 2,
        "600 long keys need two interior levels"
    );
    let mut log = NoLog::default();
    let mut near = None;
    for number in 0..560i64 {
        let key = key_of(2_000, number);
        tree.delete_near(&mut database, &mut log, &[Datum::Text(&key)], &mut near)
            .unwrap_or_else(|error| panic!("deleting {number}: {:?}", error.detail()));
        model.remove(&key);
        if number % 80 == 79 {
            assert_agrees(
                &database,
                &tree,
                &model,
                &format!("after deleting {number}"),
            );
        }
    }
    assert_agrees(&database, &tree, &model, "after the trim");
    assert!(
        tree.write_stats().parents_dissolved > 0,
        "no parent was dissolved: {:?}",
        tree.write_stats()
    );
    let pages = tree.pages_occupied(database.pool()).expect("the pages");
    let mut seen = std::collections::HashSet::new();
    for (page, share) in pages {
        if share == inillucent_tree::PageShare::Owned {
            assert!(seen.insert(page), "page {} is held twice", page.0);
        }
    }
    assert!(
        seen.len() < 120,
        "forty keys still hold {} pages",
        seen.len()
    );
}

//! Moving one page of each kind to another page number.
//!
//! Invariant: **after a page moves, the tree holds exactly the rows it held, in
//! the same order, every value reads back byte for byte, the tree passes its own
//! integrity check, and the old page number is free.** The move rewrites the one
//! field that named the page and nothing else, so each test moves a page that has
//! a different kind of referrer: a leaf (a parent swip and a sibling link), an
//! interior page (a parent swip), a chain page (its neighbours' links and the
//! leaf row that owns the value), a shared page (every owner's leaf), and a free
//! map page (the previous map page and the meta record).

use inillucent_pool::page::{self, PageKind};
use inillucent_pool::{Database, Options, PageId};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::relocate::{relocate_page, Pinned, Relocated, TreeLookup};
use inillucent_tree::types::{ColumnSpec, PhysicalType};
use inillucent_tree::write::{NoLog, TreeLog};
use inillucent_tree::PagedTree;
use inillucent_vfs::{DbPath, MemoryVfs};
use inillucent_wal::record::Body;

/// The page size, small so a few thousand rows make a tree three levels deep.
const PAGE_SIZE: usize = 4_096;

/// The tree identifier every test tree carries.
const TREE: u64 = 31;

/// A lookup that knows one tree.
struct One<'t>(&'t PagedTree);

impl TreeLookup for One<'_> {
    /// Answers the one tree, whatever it is asked for by the right identifier.
    ///
    /// @param tree_id - the identifier a page carries
    fn tree(&self, tree_id: u64) -> Option<&PagedTree> {
        (tree_id == self.0.tree_id()).then_some(self.0)
    }
}

/// A log that keeps the kind of each record it is given.
#[derive(Default)]
struct Recording {
    inner: NoLog,
    kinds: Vec<u8>,
}

impl TreeLog for Recording {
    /// Keeps the record's kind, then hands out the next LSN.
    ///
    /// @param body - the record
    fn log(&mut self, body: Body<'_>) -> inillucent_base::DbResult<u64> {
        self.kinds.push(body.kind());
        self.inner.log(body)
    }
}

/// Opens an empty database over memory.
fn database() -> Database {
    let vfs = MemoryVfs::new();
    Database::create(
        &vfs,
        &DbPath::new("relocate.rdb"),
        Options::default()
            .with_page_size(PAGE_SIZE)
            .with_frames(512),
    )
    .expect("a database")
}

/// Returns the columns of a table keyed by an integer holding one text column.
fn columns() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::key(PhysicalType::Int64),
        ColumnSpec::new(PhysicalType::Text),
    ]
}

/// Returns a row: the key and a text value of the given length.
///
/// @param key - the integer key
/// @param length - how long the text is
fn row_of(key: i64, length: usize) -> Vec<OwnedDatum> {
    let byte = b'a' + u8::try_from(key.rem_euclid(26)).unwrap_or(0);
    vec![OwnedDatum::Int(key), OwnedDatum::Text(vec![byte; length])]
}

/// Builds a tree of rows by a bulk build and returns it with the rows.
///
/// @param database - the file
/// @param count - how many rows
/// @param length - how long each row's text is
fn bulk_tree(
    database: &mut Database,
    count: i64,
    length: usize,
) -> (PagedTree, Vec<Vec<OwnedDatum>>) {
    let owned: Vec<Vec<OwnedDatum>> = (0..count).map(|key| row_of(key, length)).collect();
    let borrowed: Vec<Vec<Datum<'_>>> = owned
        .iter()
        .map(|row| row.iter().map(OwnedDatum::borrow).collect())
        .collect();
    let tree =
        PagedTree::bulk_build(database, TREE, columns(), 1, &borrowed).expect("a bulk build");
    (tree, owned)
}

/// Asserts the tree reads back every row, by scan and by point, and is sound.
///
/// @param database - the file
/// @param tree - the tree
/// @param rows - what it should hold
fn assert_holds(database: &Database, tree: &PagedTree, rows: &[Vec<OwnedDatum>]) {
    tree.check(database.pool()).expect("the tree is sound");
    assert_eq!(tree.rows(database.pool()).expect("a scan"), rows);
    for row in rows {
        let key = row.first().map(OwnedDatum::borrow).expect("a key");
        let found = tree.point(database.pool(), &[key]).expect("a point read");
        assert_eq!(found.as_ref(), Some(row), "the row of key {row:?}");
    }
}

/// Moves a page to a freshly allocated number and asserts the move happened.
///
/// @param database - the file
/// @param tree - the tree that owns the page
/// @param log - where the records go
/// @param from - the page to move
fn move_to_a_new_page(
    database: &mut Database,
    tree: &PagedTree,
    log: &mut Recording,
    from: PageId,
) -> PageId {
    let to = database.allocate(1).expect("a page");
    let moved = relocate_page(database, log, &One(tree), from, to).expect("a move");
    assert_eq!(moved, Relocated::Moved);
    assert!(!database.page_is_allocated(from), "the old number is free");
    assert!(database.page_is_allocated(to), "the new number is in use");
    to
}

/// Returns the pages of a tree at one kind, in page order.
///
/// @param database - the file
/// @param tree - the tree
/// @param kind - the kind of page wanted
fn pages_of_kind(database: &Database, tree: &PagedTree, kind: PageKind) -> Vec<PageId> {
    let mut found: Vec<PageId> = tree
        .pages_occupied(database.pool())
        .expect("the pages")
        .into_iter()
        .map(|(page, _)| page)
        .filter(|page| {
            let guard = database.pool().fetch(*page).expect("a page");
            page::kind_of(&guard).expect("a kind") == kind
        })
        .collect();
    found.sort();
    found
}

/// A leaf moves: its parent names the new number, the leaf before it points at
/// the new number, and a scan still visits every row once.
#[test]
fn a_leaf_moves_and_keeps_its_place_in_the_sibling_chain() {
    let mut database = database();
    let (tree, rows) = bulk_tree(&mut database, 3_000, 300);
    let leaves = pages_of_kind(&database, &tree, PageKind::Leaf);
    assert!(leaves.len() > 20, "{} leaves", leaves.len());
    let mut log = Recording::default();
    for index in [leaves.len() / 2, leaves.len() - 1, 0] {
        move_to_a_new_page(&mut database, &tree, &mut log, leaves[index]);
        assert_holds(&database, &tree, &rows);
    }
    // A move logs the allocation, the copy, a relink for each field that named
    // the page, and the release.
    assert!(log.kinds.contains(&20), "no relink record was logged");
    assert!(log.kinds.contains(&7) && log.kinds.contains(&8) && log.kinds.contains(&9));
}

/// An interior page moves: its parent names the new number, and every key is
/// still found through it.
#[test]
fn an_interior_page_moves_and_still_routes_every_key() {
    let mut database = database();
    let (tree, rows) = bulk_tree(&mut database, 6_000, 300);
    assert!(tree.height(database.pool()).expect("a height") >= 2);
    let interiors = pages_of_kind(&database, &tree, PageKind::Interior);
    let movable: Vec<PageId> = interiors
        .into_iter()
        .filter(|page| *page != tree.root())
        .collect();
    assert!(
        !movable.is_empty(),
        "the tree has an interior page below its root"
    );
    let mut log = Recording::default();
    for page in movable {
        move_to_a_new_page(&mut database, &tree, &mut log, page);
        assert_holds(&database, &tree, &rows);
    }
}

/// A root is never moved, whatever it is asked.
#[test]
fn a_root_is_pinned() {
    let mut database = database();
    let (tree, rows) = bulk_tree(&mut database, 3_000, 300);
    let to = database.allocate(1).expect("a page");
    let mut log = Recording::default();
    let moved =
        relocate_page(&mut database, &mut log, &One(&tree), tree.root(), to).expect("a move");
    assert_eq!(moved, Relocated::Pinned(Pinned::Root));
    assert!(log.kinds.is_empty(), "a pinned page logs nothing");
    assert_holds(&database, &tree, &rows);
}

/// Returns a tree whose rows hold values of the given length, written by `put`.
///
/// @param database - the file
/// @param count - how many rows
/// @param length - how long each value is
fn put_tree(
    database: &mut Database,
    count: i64,
    length: usize,
) -> (PagedTree, Vec<Vec<OwnedDatum>>) {
    let (mut tree, _) = bulk_tree(database, 0, 0);
    let rows: Vec<Vec<OwnedDatum>> = (0..count).map(|key| row_of(key, length)).collect();
    let mut log = NoLog::default();
    for row in &rows {
        let borrowed: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
        tree.put(database, &mut log, &borrowed).expect("a put");
    }
    (tree, rows)
}

/// Returns the pages the tree's values sit on, of the given kind, in page order.
///
/// @param database - the file
/// @param tree - the tree
/// @param kind - the kind of page wanted
fn value_pages(database: &Database, tree: &PagedTree, kind: PageKind) -> Vec<PageId> {
    pages_of_kind(database, tree, kind)
}

/// Every page of a chain moves, the first, the middle and the last, and the
/// value reads back byte for byte after each.
#[test]
fn a_chain_page_moves_and_the_value_reads_back_byte_for_byte() {
    let mut database = database();
    let (tree, rows) = put_tree(&mut database, 6, 60_000);
    let pages = value_pages(&database, &tree, PageKind::BlobExtent);
    assert!(pages.len() > 60, "{} chain pages", pages.len());
    let mut log = Recording::default();
    for index in [0, pages.len() / 2, pages.len() - 1, 1] {
        move_to_a_new_page(&mut database, &tree, &mut log, pages[index]);
        assert_holds(&database, &tree, &rows);
    }
}

/// A shared page moves and every value that sat on it still reads.
#[test]
fn a_shared_page_moves_and_every_owner_still_reads() {
    let mut database = database();
    let (tree, rows) = put_tree(&mut database, 40, 1_500);
    let pages = value_pages(&database, &tree, PageKind::BlobShared);
    assert!(pages.len() > 4, "{} shared pages", pages.len());
    let mut log = Recording::default();
    for index in [0, pages.len() / 2, pages.len() - 1] {
        move_to_a_new_page(&mut database, &tree, &mut log, pages[index]);
        assert_holds(&database, &tree, &rows);
    }
}

/// A value written with no owner key is read as usual and never moved.
#[test]
fn a_value_with_no_owner_key_pins_its_page() {
    let mut database = database();
    let mut log = NoLog::default();
    let value = vec![7u8; 30_000];
    let reference = inillucent_tree::paged::write_extent(&mut database, &mut log, TREE, &value)
        .expect("a write");
    let (tree, _) = bulk_tree(&mut database, 1, 1);
    let to = database.allocate(1).expect("a page");
    let mut recording = Recording::default();
    let moved = relocate_page(
        &mut database,
        &mut recording,
        &One(&tree),
        reference.first,
        to,
    )
    .expect("a move");
    assert_eq!(moved, Relocated::Pinned(Pinned::NoOwnerKey));
    let read = inillucent_tree::paged::read_extent(database.pool(), reference).expect("a read");
    assert_eq!(read, value);
}

/// A free map page moves: the chain names the new number, every bit is kept, and
/// the file reopens with the same pages free.
#[test]
fn a_free_map_page_moves_and_the_file_reopens() {
    let vfs = MemoryVfs::new();
    let path = DbPath::new("relocate-map.rdb");
    let options = Options::default().with_page_size(512).with_frames(64);
    let mut database = Database::create(&vfs, &path, options).expect("a database");
    let held = (0..4_100)
        .map(|_| database.allocate(1).expect("a page"))
        .count();
    assert_eq!(held, 4_100);
    let maps = database.free_map_pages();
    assert!(maps.len() >= 2, "{} map pages", maps.len());
    let second = maps[1].0;
    let to = database.allocate(1).expect("a page");
    let before: Vec<bool> = (0..4_200)
        .map(|n| database.page_is_allocated(PageId(n)))
        .collect();
    let (tree, _) = (None::<PagedTree>, ());
    let mut log = Recording::default();
    let lookup = NoTrees;
    let moved = relocate_page(&mut database, &mut log, &lookup, second, to).expect("a move");
    assert_eq!(moved, Relocated::Moved);
    drop(tree);
    assert!(database.page_is_allocated(to));
    assert!(!database.page_is_allocated(second));
    database.checkpoint().expect("a checkpoint");
    drop(database);
    let reopened = Database::open(&vfs, &path, 64).expect("a reopen");
    let after: Vec<bool> = (0..4_200)
        .map(|n| reopened.page_is_allocated(PageId(n)))
        .collect();
    for (page, (was, is)) in before.iter().zip(after.iter()).enumerate() {
        let moved_pair = page as u64 == second.0 || page as u64 == to.0;
        if !moved_pair {
            assert_eq!(was, is, "page {page} changed");
        }
    }
    assert_eq!(reopened.free_map_pages()[1].0, to);
}

/// A lookup that knows no tree.
struct NoTrees;

impl TreeLookup for NoTrees {
    /// Answers nothing.
    ///
    /// @param _tree_id - the identifier asked for
    fn tree(&self, _tree_id: u64) -> Option<&PagedTree> {
        None
    }
}

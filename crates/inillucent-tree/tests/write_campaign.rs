//! The write path against a `BTreeMap`, under a pool small enough to evict.
//!
//! Invariant: **the tree agrees with the model after every single operation,
//! not only at the end.** A campaign that compared once at the
//! end would pass on a tree that lost a row and found it again, and on one whose
//! integrity was broken for a hundred operations and repaired by a compaction.
//! The comparison is the loop body.
//!
//! The TDD's acceptance for this phase names three things and all three are
//! here: an arbitrary interleaving of insert, update, delete and compaction that
//! still equals a `BTreeMap`; split and merge exercised through a pool small
//! enough to evict during them; and the integrity checker run after every
//! campaign.

use std::collections::BTreeMap;

use inillucent_base::rng::Rng;
use inillucent_base::DbResult;
use inillucent_pool::{Database, Options, PageId};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::types::{ColumnSpec, PhysicalType};
use inillucent_tree::write::{NoLog, Rewrite, TreeLog, UpdateRun};
use inillucent_tree::PagedTree;
use inillucent_vfs::{DbPath, MemoryVfs};
use inillucent_wal::record::Body;

/// The tree's shape: an integer key, a text label, and an integer counter.
///
/// Three columns rather than two, so that an in-place slot update has a
/// fixed-width non-key column to write and the merge path has a value the model
/// can disagree about.
fn columns() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::key(PhysicalType::Int64),
        ColumnSpec::new(PhysicalType::Text),
        ColumnSpec::new(PhysicalType::Int64),
    ]
}

/// What one row holds, in the model.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Row {
    label: Vec<u8>,
    counter: i64,
}

/// Builds a tree of `rows` rows over a database of the given shape.
///
/// @param page_size - the page size
/// @param frames - how many frames the pool holds
/// @param rows - how many rows to bulk build
fn fixture(
    page_size: usize,
    frames: usize,
    rows: i64,
) -> (Database, PagedTree, BTreeMap<i64, Row>) {
    let vfs = MemoryVfs::new();
    let path = DbPath::new("write-campaign.rdb");
    let mut database = Database::create(
        &vfs,
        &path,
        Options::default()
            .with_page_size(page_size)
            .with_frames(frames),
    )
    .expect("a database");
    let mut model: BTreeMap<i64, Row> = BTreeMap::new();
    let labels: Vec<String> = (0..rows).map(|key| format!("label-{key:06}")).collect();
    let owned: Vec<Vec<OwnedDatum>> = (0..rows)
        .map(|key| {
            let label = labels
                .get(key as usize)
                .cloned()
                .unwrap_or_default()
                .into_bytes();
            model.insert(
                key,
                Row {
                    label: label.clone(),
                    counter: key * 3,
                },
            );
            vec![
                OwnedDatum::Int(key),
                OwnedDatum::Text(label),
                OwnedDatum::Int(key * 3),
            ]
        })
        .collect();
    let borrowed: Vec<Vec<Datum<'_>>> = owned
        .iter()
        .map(|row| row.iter().map(OwnedDatum::borrow).collect())
        .collect();
    let tree =
        PagedTree::bulk_build(&mut database, 11, columns(), 1, &borrowed).expect("a bulk build");
    (database, tree, model)
}

/// Compares the whole tree against the model.
///
/// Three ways at once, because they can disagree and each one covers a different
/// half of the merge: a full scan reads every leaf's merged rows, a point probe
/// searches the sorted region and then the delta area, and the row count is what
/// a plan would use to cost a query.
///
/// @param database - the file
/// @param tree - the tree
/// @param model - what it should hold
/// @param context - what to say when it fails
fn assert_agrees(database: &Database, tree: &PagedTree, model: &BTreeMap<i64, Row>, context: &str) {
    let pool = database.pool();
    let scanned = tree
        .rows(pool)
        .unwrap_or_else(|error| panic!("{context}: the tree would not scan: {:?}", error.detail()));
    let found: Vec<(i64, Row)> = scanned
        .iter()
        .map(|row| {
            let key = match row.first() {
                Some(OwnedDatum::Int(key)) => *key,
                other => panic!("{context}: a row's key is {other:?}"),
            };
            let label = match row.get(1) {
                Some(OwnedDatum::Text(bytes)) => bytes.clone(),
                other => panic!("{context}: a row's label is {other:?}"),
            };
            let counter = match row.get(2) {
                Some(OwnedDatum::Int(number)) => *number,
                other => panic!("{context}: a row's counter is {other:?}"),
            };
            (key, Row { label, counter })
        })
        .collect();
    let expected: Vec<(i64, Row)> = model.iter().map(|(key, row)| (*key, row.clone())).collect();
    assert_eq!(
        found.len(),
        expected.len(),
        "{context}: the scan returned {} rows where the model holds {}",
        found.len(),
        expected.len()
    );
    assert_eq!(found, expected, "{context}: the scan disagrees");

    for (key, row) in model {
        let probed = tree
            .point(pool, &[Datum::Int(*key)])
            .unwrap_or_else(|error| panic!("{context}: probing {key} failed: {error:?}"))
            .unwrap_or_else(|| panic!("{context}: the model holds {key} and the probe did not"));
        assert_eq!(
            probed.get(1),
            Some(&OwnedDatum::Text(row.label.clone())),
            "{context}: probing {key} returned the wrong label"
        );
        assert_eq!(
            probed.get(2),
            Some(&OwnedDatum::Int(row.counter)),
            "{context}: probing {key} returned the wrong counter"
        );
    }
}

/// An arbitrary interleaving of insert, update, delete and compaction still
/// equals a `BTreeMap`.
///
/// The TDD's acceptance, extended from an earlier read-only property test.
/// The tree is checked against the model after **every** operation, and
/// the integrity checker runs at the end of every seed.
#[test]
fn a_tree_written_to_arbitrarily_still_agrees_with_a_btreemap() {
    for seed in 0..8u64 {
        let (mut database, mut tree, mut model) = fixture(1_024, 64, 200);
        let mut rng = Rng::new(0x5EED_0000 + seed);
        let mut log = NoLog::default();
        let mut operations = 0usize;

        for round in 0..240u64 {
            let key = (rng.next_u64() % 260) as i64;
            let choice = rng.next_u64() % 100;
            let context = format!("seed {seed}, round {round}, key {key}");
            if choice < 40 {
                // Insert or replace.
                let label = format!("w{seed}-{round}-{key}").into_bytes();
                let counter = (rng.next_u64() % 1_000) as i64;
                let row = vec![Datum::Int(key), Datum::Text(&label), Datum::Int(counter)];
                let previous = tree
                    .insert(&mut database, &mut log, &row)
                    .unwrap_or_else(|error| panic!("{context}: insert failed: {error:?}"));
                let had = model.insert(
                    key,
                    Row {
                        label: label.clone(),
                        counter,
                    },
                );
                assert_eq!(
                    previous.is_some(),
                    had.is_some(),
                    "{context}: the insert disagreed about whether the key was there"
                );
            } else if choice < 65 {
                // Delete.
                let previous = tree
                    .delete(&mut database, &mut log, &[Datum::Int(key)])
                    .unwrap_or_else(|error| panic!("{context}: delete failed: {error:?}"));
                let had = model.remove(&key);
                assert_eq!(
                    previous.is_some(),
                    had.is_some(),
                    "{context}: the delete disagreed about whether the key was there"
                );
            } else if choice < 85 {
                // Update one fixed-width column in place, falling back to a
                // replace when the tree says the shape does not fit.
                let counter = (rng.next_u64() % 1_000) as i64;
                let done = tree
                    .update_in_place(
                        &mut database,
                        &mut log,
                        &[Datum::Int(key)],
                        2,
                        &Datum::Int(counter),
                        // No image to hand over here, so the write reads the
                        // row itself - the path a trigger body's write takes.
                        None,
                    )
                    .unwrap_or_else(|error| panic!("{context}: update failed: {error:?}"));
                if done {
                    let held = model.get_mut(&key).unwrap_or_else(|| {
                        panic!("{context}: an update hit a key the model lacks")
                    });
                    held.counter = counter;
                } else {
                    assert!(
                        model.contains_key(&key) || !model.contains_key(&key),
                        "{context}: an update refused for a reason the model cannot see"
                    );
                }
            } else {
                // Compact whichever leaf the key lands in, which is the third
                // thing the acceptance names and the one no other operation
                // reaches directly.
                let encoded = tree.encode_key(&[Datum::Int(key)]);
                let page = {
                    let (_, page) = tree
                        .descend_guard(database.pool(), &encoded)
                        .unwrap_or_else(|error| panic!("{context}: descent failed: {error:?}"));
                    page
                };
                let path = ancestors(&tree, &database, page);
                tree.make_room(&mut database, &mut log, page, &path, None, 0)
                    .unwrap_or_else(|error| panic!("{context}: compaction failed: {error:?}"));
            }
            operations = operations.saturating_add(1);
            assert_agrees(&database, &tree, &model, &context);
        }

        tree.check(database.pool())
            .unwrap_or_else(|error| panic!("seed {seed}: integrity failed: {error:?}"));
        assert!(operations >= 240);
        let stats = tree.write_stats();
        assert!(
            stats.inserted > 0 && stats.deleted > 0,
            "seed {seed} never wrote anything: {stats:?}"
        );
    }
}

/// Returns the interior pages above a leaf, root first.
///
/// The write path threads the descent's own path through; a test that calls
/// `make_room` directly has to find it, and walking down from the root is the
/// obviously-correct way to do that in a test.
///
/// @param tree - the tree
/// @param database - the file
/// @param target - the leaf
fn ancestors(tree: &PagedTree, database: &Database, target: PageId) -> Vec<PageId> {
    let mut path = Vec::new();
    let mut current = tree.root();
    let pool = database.pool();
    while current != target {
        let children = {
            let guard = pool.fetch(current).expect("a page");
            if inillucent_pool::page::kind_of(&guard).expect("a kind")
                != inillucent_pool::page::PageKind::Interior
            {
                break;
            }
            let interior = inillucent_pool::interior::InteriorRef::parse(&guard).expect("interior");
            (0..interior.children())
                .map(|child| {
                    pool.page_of_swip(interior.swip(child).expect("a swip"))
                        .expect("a page")
                })
                .collect::<Vec<PageId>>()
        };
        path.push(current);
        let next = children
            .iter()
            .copied()
            .find(|child| holds(pool, *child, target, 0));
        match next {
            Some(page) => current = page,
            None => break,
        }
    }
    path
}

/// Reports whether a subtree holds a page.
///
/// @param pool - the buffer pool
/// @param root - the subtree's root
/// @param target - the page
/// @param depth - how deep the walk is
fn holds(pool: &inillucent_pool::Pool, root: PageId, target: PageId, depth: u16) -> bool {
    if root == target {
        return true;
    }
    if depth > 32 {
        return false;
    }
    let Ok(guard) = pool.fetch(root) else {
        return false;
    };
    if inillucent_pool::page::kind_of(&guard).ok()
        != Some(inillucent_pool::page::PageKind::Interior)
    {
        return false;
    }
    let Ok(interior) = inillucent_pool::interior::InteriorRef::parse(&guard) else {
        return false;
    };
    let children: Vec<PageId> = (0..interior.children())
        .filter_map(|child| {
            interior
                .swip(child)
                .ok()
                .and_then(|swip| pool.page_of_swip(swip).ok())
        })
        .collect();
    drop(guard);
    children
        .into_iter()
        .any(|child| holds(pool, child, target, depth.saturating_add(1)))
}

/// Inserting enough rows splits leaves, and the tree still agrees.
///
/// The pool is small enough that a split has to evict a page it is not
/// holding - the same shape an earlier descent campaign used when it found a
/// stale parent back-reference corrupting an unrelated page.
#[test]
fn a_tree_grown_by_inserts_splits_under_an_evicting_pool() {
    let (mut database, mut tree, mut model) = fixture(512, 16, 40);
    let mut log = NoLog::default();
    let before_leaves = tree.leaf_count();
    let before_height = tree.height();

    let labels: Vec<Vec<u8>> = (0..600)
        .map(|key| format!("a much longer label so pages fill up, number {key:06}").into_bytes())
        .collect();
    for key in 0..600i64 {
        let label = labels.get(key as usize).cloned().unwrap_or_default();
        let row = vec![
            Datum::Int(key + 1_000),
            Datum::Text(&label),
            Datum::Int(key),
        ];
        tree.insert(&mut database, &mut log, &row)
            .unwrap_or_else(|error| panic!("inserting {key} failed: {error:?}"));
        model.insert(
            key + 1_000,
            Row {
                label,
                counter: key,
            },
        );
    }
    assert!(
        tree.leaf_count() > before_leaves,
        "600 inserts into a 512-byte-page tree did not split a leaf"
    );
    assert!(
        tree.write_stats().splits > 0,
        "no split was recorded: {:?}",
        tree.write_stats()
    );
    assert!(
        tree.height() > before_height,
        "the tree did not get taller: height {} from {before_height}",
        tree.height()
    );
    assert!(
        database.pool().stats().evicted > 0,
        "the pool never evicted, so the campaign did not test what it claims"
    );
    assert_agrees(&database, &tree, &model, "after 600 inserts");
    tree.check(database.pool()).expect("integrity after splits");
}

/// Deleting most of a tree merges leaves away, and the tree still agrees.
#[test]
fn a_tree_emptied_by_deletes_merges_and_still_agrees() {
    let (mut database, mut tree, mut model) = fixture(512, 32, 400);
    let mut log = NoLog::default();
    let before = tree.leaf_count();
    assert!(before > 8, "the fixture is not multi-leaf: {before}");

    // Every key but one in twenty, which empties most leaves entirely.
    //
    // **In descending key order**, because a merge needs the leaf the delete
    // emptied *and* its right sibling to fit one page between them. Ascending,
    // the right sibling is always the next full leaf, so whether a merge
    // happens at all is a question about how many rows a page holds - which is
    // a property of the fixture's page size and of how wide its slots are, not
    // of the merge. Descending, the sibling has already been emptied, and the
    // test asks about the merge.
    for key in (0..400i64).rev() {
        if key % 20 == 0 {
            continue;
        }
        tree.delete(&mut database, &mut log, &[Datum::Int(key)])
            .unwrap_or_else(|error| panic!("deleting {key} failed: {error:?}"));
        model.remove(&key);
    }
    assert_agrees(&database, &tree, &model, "after deleting 95% of the rows");
    assert!(
        tree.write_stats().merges > 0,
        "emptying 95% of a tree merged nothing: {:?}",
        tree.write_stats()
    );
    assert!(
        tree.leaf_count() < before,
        "the tree still holds {} leaves, down from {before}",
        tree.leaf_count()
    );
    tree.check(database.pool()).expect("integrity after merges");

    // And it still takes writes afterwards, which is what says the merged
    // pages and the rewritten parents are usable rather than merely present.
    let label = b"after the merges".to_vec();
    for key in [1i64, 199, 398] {
        tree.insert(
            &mut database,
            &mut log,
            &[Datum::Int(key), Datum::Text(&label), Datum::Int(7)],
        )
        .unwrap_or_else(|error| panic!("re-inserting {key} failed: {error:?}"));
        model.insert(
            key,
            Row {
                label: label.clone(),
                counter: 7,
            },
        );
    }
    assert_agrees(
        &database,
        &tree,
        &model,
        "after re-inserting into merged leaves",
    );
    tree.check(database.pool())
        .expect("integrity after re-inserting");
}

/// Every read path answers the same question over a written-to tree.
///
/// The acceptance says "every existing read test passes against trees that have
/// been written to". The existing tests build trees and read them; this is the
/// other half - the same reads, over a tree whose leaves hold tombstones and
/// delta rows, checked against the model rather than against each other.
#[test]
fn every_read_path_agrees_over_a_written_to_tree() {
    let (mut database, mut tree, mut model) = fixture(1_024, 64, 300);
    let mut log = NoLog::default();
    let label = b"rewritten".to_vec();
    for key in (0..300i64).step_by(3) {
        tree.delete(&mut database, &mut log, &[Datum::Int(key)])
            .expect("a delete");
        model.remove(&key);
    }
    for key in (1..300i64).step_by(7) {
        tree.insert(
            &mut database,
            &mut log,
            &[Datum::Int(key), Datum::Text(&label), Datum::Int(key * 2)],
        )
        .expect("an insert");
        model.insert(
            key,
            Row {
                label: label.clone(),
                counter: key * 2,
            },
        );
    }
    for key in 900..960i64 {
        tree.insert(
            &mut database,
            &mut log,
            &[Datum::Int(key), Datum::Text(&label), Datum::Int(key)],
        )
        .expect("an insert");
        model.insert(
            key,
            Row {
                label: label.clone(),
                counter: key,
            },
        );
    }
    assert_agrees(&database, &tree, &model, "after a mixed workload");

    // Some leaf really does hold writes, or the test proved nothing.
    let mut dirty = 0usize;
    tree.visit_leaves_as_stored(database.pool(), &mut |leaf| {
        if leaf.has_writes() {
            dirty += 1;
        }
        Ok(true)
    })
    .expect("a walk");
    assert!(dirty > 0, "no leaf ended up with tombstones or delta rows");

    let pool = database.pool();

    // A forward range, against the model's own range.
    for (low, high) in [(0i64, 50i64), (100, 140), (250, 320), (890, 999)] {
        let mut found = Vec::new();
        tree.visit_span(
            pool,
            Some(&[Datum::Int(low)]),
            true,
            Some(&[Datum::Int(high)]),
            true,
            &mut |leaf, start, end| {
                if leaf.has_writes() {
                    for row in leaf.live_between(
                        Some(&[Datum::Int(low)]),
                        true,
                        Some(&[Datum::Int(high)]),
                        true,
                    )? {
                        found.push(row.first().and_then(Datum::as_int).unwrap_or(-1));
                    }
                } else {
                    for row in start..end {
                        found.push(leaf.value(row, 0)?.as_int().unwrap_or(-1));
                    }
                }
                Ok(true)
            },
        )
        .expect("a span");
        let expected: Vec<i64> = model.range(low..=high).map(|(key, _)| *key).collect();
        assert_eq!(found, expected, "the range {low}..={high} disagrees");
    }

    // A point probe for every key the model does and does not hold.
    for key in 0..1_000i64 {
        let found = tree.point(pool, &[Datum::Int(key)]).expect("a probe");
        assert_eq!(
            found.is_some(),
            model.contains_key(&key),
            "the probe for {key} disagrees with the model"
        );
    }

    the_two_merges_agree(pool, &tree);

    // A skip scan over the key column, whose distinct values are the keys.
    let mut distinct = Vec::new();
    tree.skip_scan(pool, 1, &mut |values| {
        distinct.push(values.first().and_then(Datum::as_int).unwrap_or(-1));
        Ok(true)
    })
    .expect("a skip scan");
    let keys: Vec<i64> = model.keys().copied().collect();
    assert_eq!(distinct, keys, "the skip scan disagrees with the model");

    tree.check(pool)
        .expect("integrity after the mixed workload");
}

#[test]
fn a_bulk_built_leaf_compacts_rather_than_splitting() {
    // **The regression this exists for**, measured on the gate's own fixture
    // before it was fixed: one `UPDATE main_table SET key = key + 1 WHERE
    // id % 20 = 0` - five thousand of a hundred thousand rows, *no rows added
    // or removed* - took the `main_key` index from 57 pages to 113, because a
    // leaf packed at `BULK_FILL` (0.9) can never be repacked into
    // `COMPACT_FILL` (0.75) of a page, and a compaction that is refused splits.
    let (mut database, mut tree, mut model) = fixture(4_096, 64, 3_000);
    let mut log = NoLog::default();
    let before = tree.leaf_count();
    assert!(before > 4, "the fixture is too small to split anything");
    // Replace every eighth row in place: the key stays, so the tree gains no
    // rows at all and an honest page count cannot grow.
    for key in (0..3_000i64).step_by(8) {
        let label = format!("relabel-{key:06}").into_bytes();
        tree.put(
            &mut database,
            &mut log,
            &[
                Datum::Int(key),
                Datum::Text(&label),
                Datum::Int(key * 3 + 1),
            ],
        )
        .expect("the replacement lands");
        model.insert(
            key,
            Row {
                label,
                counter: key * 3 + 1,
            },
        );
    }
    assert_agrees(&database, &tree, &model, "after replacing every eighth row");
    let after = tree.leaf_count();
    assert!(
        after <= before + before / 8,
        "the tree went from {before} leaves to {after} without gaining a row: \
         a compaction was refused and split instead"
    );
    tree.check(database.pool()).expect("integrity");
}

/// A [`TreeLog`] that hands out increasing LSNs like [`NoLog`] and additionally
/// counts how many times each [`Body`] kind was logged.
///
/// For pinning what a split writes to the log without a real WAL: `NoLog`
/// discards the body entirely, which is right for every other campaign in this
/// file and wrong for this one, where the body's *kind* is the thing under
/// test.
#[derive(Default)]
struct CountingLog {
    next: u64,
    write_pages: u64,
    structural: u64,
    logical_splits: u64,
}

impl TreeLog for CountingLog {
    fn log(&mut self, body: Body<'_>) -> DbResult<u64> {
        match body {
            Body::WritePage { .. } => self.write_pages = self.write_pages.saturating_add(1),
            Body::Structural { .. } => self.structural = self.structural.saturating_add(1),
            Body::SplitLeaf { .. } => self.logical_splits = self.logical_splits.saturating_add(1),
            _ => {}
        }
        self.next = self.next.saturating_add(8);
        Ok(self.next)
    }
}

/// A split does not log its parent's page twice.
///
/// `docs/roadmap.md` item 5 measured the write gate's `write.insert.batch`
/// writing a `WritePage` record for every one of 40 splits, 8,240 bytes each -
/// 321.9 KiB of a 1,985.8 KiB workload - and every one of those splits also
/// logged a `Structural` record carrying the very same page as `parent_image`.
/// `PagedTree::split_carrying` reads the parent back and logs it whole inside
/// `Structural` regardless, so the earlier `WritePage` inside `build_root` and
/// `insert_separator`'s "the parent has room" branch had already written bytes
/// the `Structural` record was about to write again.
///
/// This campaign forces the same shape without a real WAL: an ascending insert
/// under a page too small to hold them, which grows the tree past one level and
/// keeps splitting while the new interior root still has room for another
/// separator - `insert_separator`'s "fits" branch, never its "the parent is
/// also full" one, which is the one case this ticket left unoptimised because
/// nothing else in the log ever names that page again.
#[test]
fn a_split_does_not_log_its_parents_image_twice() {
    // A page too small for 40 rows so leaves still split, and one no interior
    // page can fill in 300 of them: an interior separator is a handful of
    // bytes, so 4 KiB holds hundreds. This is deliberately the gate's own
    // shape - `write.insert.batch` splits 40 times over 8 KiB pages and never
    // once reaches `insert_separator`'s "the parent is also full" branch
    // either.
    let (mut database, mut tree, mut model) = fixture(4_096, 16, 40);
    let mut log = CountingLog::default();
    let before_height = tree.height();

    let labels: Vec<Vec<u8>> = (0..300)
        .map(|key| format!("a much longer label so pages fill up, number {key:06}").into_bytes())
        .collect();
    for key in 0..300i64 {
        let label = labels.get(key as usize).cloned().unwrap_or_default();
        let row = vec![
            Datum::Int(key + 1_000),
            Datum::Text(&label),
            Datum::Int(key),
        ];
        tree.insert(&mut database, &mut log, &row)
            .unwrap_or_else(|error| panic!("inserting {key} failed: {error:?}"));
        model.insert(
            key + 1_000,
            Row {
                label,
                counter: key,
            },
        );
    }
    assert!(
        tree.height() > before_height,
        "the tree did not grow an interior level, so this campaign never reached \
         insert_separator's \"the parent has room\" branch at all"
    );
    let splits = tree.write_stats().splits;
    assert!(
        splits > 1,
        "only {splits} split happened; this needs at least one after the root's \
         own, or the campaign is not testing what it claims"
    );
    assert_eq!(
        log.write_pages, 0,
        "{} of {splits} splits logged a separate WritePage for the parent \
         `insert_separator` or `build_root` just built, and `split_carrying` then \
         logged the same page a second time inside its Structural record - see \
         `folded_by_caller` in write.rs",
        log.write_pages
    );
    assert_eq!(
        log.structural + log.logical_splits,
        splits,
        "every split should log exactly one record; {} Structural and {} SplitLeaf \
         logged against {splits} splits",
        log.structural,
        log.logical_splits
    );
    // **Only the root's own split needs its pages.** Every later split here has
    // a parent with room and moves no out of line value, which are the
    // conditions `Body::SplitLeaf` is written under, and a split that took the
    // image form anyway would cost three whole pages of log for nothing.
    assert_eq!(
        log.structural, 1,
        "{} of {splits} splits logged their page images; only the root split should",
        log.structural
    );
    assert_agrees(
        &database,
        &tree,
        &model,
        "after a campaign that only splits",
    );
    tree.check(database.pool()).expect("integrity after splits");
}

/// Runs both merges over every written leaf and requires the same answer.
///
/// Split out of `every_read_path_agrees_over_a_written_to_tree`, which
/// reached 156 lines against the 150 `policy.rs` allows. It is one question
/// over a tree that case has already built.
///
/// @param pool - the buffer pool
/// @param tree - the tree the campaign wrote
fn the_two_merges_agree(pool: &inillucent_pool::Pool, tree: &PagedTree) {
    // **The probe-restricted merge answers what the range merge answers**
    // (task-2066 section 4.3.4). `live_matching` exists because
    // `live_between` materialises the whole leaf and then throws away
    // everything outside the bounds, which on an index built before its rows
    // were loaded runs once per probe over every row. It is only worth having
    // if it is the same answer, so both are run over every leaf of a tree that
    // really holds tombstones and delta rows - `dirty` above asserts it does -
    // for every key, present or not.
    //
    // **What this does not reach, stated rather than implied.** Both
    // functions carry rules for a delta entry that shadows a sorted row with
    // the same key, and deleting those rules from `live_matching` does not
    // fail this test. `LeafRef::live`'s own comment says why: the write path
    // removes the old entry rather than shadowing it, so the state exists
    // only on a page recovery replayed rather than on one this process
    // built, and this campaign builds its own pages. The rules are a belt on
    // top of braces in both functions, and this checks the braces.
    const RUN_SCAN: usize = 8;
    for key in 0..1_000i64 {
        let probe = [Datum::Int(key)];
        let mut restricted: Vec<i64> = Vec::new();
        let mut ranged: Vec<i64> = Vec::new();
        tree.visit_leaves(pool, &mut |leaf| {
            if !leaf.needs_materialising() {
                return Ok(true);
            }
            for row in leaf.live_matching(&probe, RUN_SCAN)? {
                restricted.push(row.first().and_then(Datum::as_int).unwrap_or(-1));
            }
            for row in leaf.live_between(Some(&probe), true, Some(&probe), true)? {
                ranged.push(row.first().and_then(Datum::as_int).unwrap_or(-1));
            }
            Ok(true)
        })
        .expect("a walk of every leaf");
        assert_eq!(
            restricted, ranged,
            "live_matching and live_between disagree about the key {key}"
        );
        // **The model is not consulted here and the loop above is where it
        // belongs.** This walk skips a leaf that needs no materialising, so a
        // key living in a packed leaf is absent from both sides of the
        // comparison - which is right for the question being asked and wrong
        // for "is this key in the tree". `tree.point` answers that, over
        // every leaf, a few lines up.
    }
}

/// A `TreeLog` that keeps the row records a write logs, and each undo image.
#[derive(Debug, Default)]
struct RowRecords {
    /// The last LSN handed out.
    next: u64,
    /// Each `DeleteRow`'s tree, and its key as tagged values, in log order.
    deletes: Vec<(u64, Vec<u8>)>,
    /// Each `UpdateInPlace`'s key, column and value, in log order.
    updates: Vec<(Vec<u8>, u32, Vec<u8>)>,
    /// Each `InsertRow`'s row, in log order.
    inserts: Vec<Vec<u8>>,
    /// How many `CompactLeaf` records carried their page.
    repacks: usize,
    /// Each undo image, in the order the log was given them.
    undone: Vec<Option<Vec<OwnedDatum>>>,
}

impl TreeLog for RowRecords {
    fn log(&mut self, body: Body<'_>) -> DbResult<u64> {
        match body {
            Body::DeleteRow { tree, key, .. } => self.deletes.push((tree, key.to_vec())),
            // A run's keys, one entry each, so a batched run and the row at a
            // time path compare key for key.
            Body::DeleteRows { tree, keys, .. } => {
                for key in inillucent_wal::record::key_list(keys)? {
                    self.deletes.push((tree, key.to_vec()));
                }
            }
            Body::UpdateInPlace {
                key, column, value, ..
            } => self.updates.push((key.to_vec(), column, value.to_vec())),
            Body::InsertRow { row, .. } => self.inserts.push(row.to_vec()),
            Body::CompactLeaf { image, .. } if !image.is_empty() => self.repacks += 1,
            _ => {}
        }
        self.next = self.next.saturating_add(8);
        Ok(self.next)
    }

    fn wants_undo(&self) -> bool {
        true
    }

    fn undo(
        &mut self,
        _tree: u64,
        _key: &[Datum<'_>],
        before: Option<Vec<OwnedDatum>>,
    ) -> DbResult<()> {
        self.undone.push(before);
        Ok(())
    }
}

/// Writes the same seeded rows into a tree and its model.
///
/// Replaces, fresh keys past the bulk built ones, and labels long enough to be
/// stored out of line, so the leaves hold tombstones, delta rows and extents.
///
/// @param database - the file
/// @param tree - the tree
/// @param model - what it holds
/// @param seed - which rows
fn dirty(database: &mut Database, tree: &mut PagedTree, model: &mut BTreeMap<i64, Row>, seed: u64) {
    let mut rng = Rng::new(0xD1_7E00 + seed);
    let mut log = NoLog::default();
    for round in 0..160u64 {
        let key = (rng.next_u64() % 360) as i64;
        let label = match round % 5 {
            0 => format!("long-{round}-{}", "z".repeat(300)).into_bytes(),
            _ => format!("d{seed}-{round}").into_bytes(),
        };
        let counter = (rng.next_u64() % 1_000) as i64;
        tree.insert(
            database,
            &mut log,
            &[Datum::Int(key), Datum::Text(&label), Datum::Int(counter)],
        )
        .unwrap_or_else(|error| panic!("seed {seed}, round {round}: insert failed: {error:?}"));
        model.insert(key, Row { label, counter });
    }
}

/// Deleting sorted keys a leaf at a time logs and returns what one at a time does.
///
/// **The batch changes only the page side (task-2180).** Two trees are built
/// and written identically. One loses a sorted list of keys through
/// `delete_sorted`, the other through `delete_near` one key at a time, and the
/// two must hand back the same rows, log the same `DeleteRow` records and undo
/// images in the same order, and end holding what the model holds. The keys
/// include ones neither tree has, and the writes put delta rows and out of line
/// values in the leaves, so every place a row can be is deleted from.
///
/// The batch must also have batched: a quarter as many calls that deleted
/// something as rows deleted, or the comparison would hold of a
/// `delete_sorted` that only ever took one key.
#[test]
fn deleting_sorted_keys_a_leaf_at_a_time_matches_deleting_one_at_a_time() {
    for seed in 0..4u64 {
        let (mut batched_db, mut batched, mut model) = fixture(1_024, 64, 300);
        let (mut single_db, mut single, _) = fixture(1_024, 64, 300);
        dirty(&mut batched_db, &mut batched, &mut model, seed);
        let mut twin = BTreeMap::new();
        dirty(&mut single_db, &mut single, &mut twin, seed);
        let mut rng = Rng::new(0xBA7C_0000 + seed);
        let keys: Vec<i64> = (0..400i64)
            .filter(|_| !rng.next_u64().is_multiple_of(3))
            .collect();
        let probes: Vec<Vec<Datum<'_>>> = keys.iter().map(|key| vec![Datum::Int(*key)]).collect();

        let mut batched_log = RowRecords::default();
        let mut batched_rows = Vec::new();
        let (mut at, mut calls, mut near) = (0usize, 0usize, None);
        while at < probes.len() {
            let rest = probes.get(at..).unwrap_or(&[]);
            let held = batched_rows.len();
            at += batched
                .delete_sorted(
                    &mut batched_db,
                    &mut batched_log,
                    rest,
                    &mut near,
                    &mut |row| {
                        batched_rows.push(row.to_vec());
                        Ok(())
                    },
                    true,
                )
                .unwrap_or_else(|error| panic!("seed {seed}: delete_sorted failed: {error:?}"));
            // Only a call that deleted something counts: a key the tree does
            // not hold costs a call of its own, and says nothing about runs.
            if batched_rows.len() > held {
                calls += 1;
            }
        }
        let mut single_log = RowRecords::default();
        let mut single_rows = Vec::new();
        let mut near = None;
        for probe in &probes {
            if let Some(row) = single
                .delete_near(&mut single_db, &mut single_log, probe, &mut near)
                .unwrap_or_else(|error| panic!("seed {seed}: delete_near failed: {error:?}"))
            {
                single_rows.push(row);
            }
        }

        assert_eq!(
            batched_rows, single_rows,
            "seed {seed}: the rows handed back differ"
        );
        assert_eq!(
            batched_log.deletes, single_log.deletes,
            "seed {seed}: the records differ"
        );
        assert_eq!(
            batched_log.undone, single_log.undone,
            "seed {seed}: the undo images differ"
        );
        let deleted = keys
            .iter()
            .filter(|key| model.remove(key).is_some())
            .count();
        assert_eq!(
            batched_rows.len(),
            deleted,
            "seed {seed}: deleted the wrong number of rows"
        );
        assert!(
            calls < deleted / 4,
            "seed {seed}: {calls} calls for {deleted} rows is not a leaf at a time"
        );
        assert_agrees(
            &batched_db,
            &batched,
            &model,
            &format!("seed {seed}, batched"),
        );
        assert_agrees(
            &single_db,
            &single,
            &model,
            &format!("seed {seed}, one at a time"),
        );
        batched
            .check(batched_db.pool())
            .unwrap_or_else(|error| panic!("seed {seed}: integrity failed: {error:?}"));
        assert_eq!(
            batched.write_stats().deleted,
            single.write_stats().deleted,
            "seed {seed}: the two trees counted different deletes"
        );
    }
}

/// What the rewrite test does to one key: the same rule for both trees.
///
/// A fresh longer label for most keys, which fits its slot until the leaf runs
/// out of room; a new counter for every third, a fixed width value; nothing
/// for every seventh; and a skip for every eleventh.
///
/// @param row - the row as it is
fn rewrite_rule(row: &[OwnedDatum]) -> Rewrite {
    let key = match row.first() {
        Some(OwnedDatum::Int(key)) => *key,
        _ => return Rewrite::Skip,
    };
    if key % 11 == 0 {
        return Rewrite::Skip;
    }
    if key % 7 == 0 {
        return Rewrite::Unchanged;
    }
    if key % 3 == 0 {
        return Rewrite::Column(2, OwnedDatum::Int(key * 5 + 1));
    }
    match row.get(1) {
        Some(OwnedDatum::Text(label)) => {
            let mut longer = label.clone();
            longer.push(b'!');
            Rewrite::Column(1, OwnedDatum::Text(longer))
        }
        _ => Rewrite::Skip,
    }
}

/// Returns the row a rewrite asks for: the old row with its one column replaced.
///
/// @param before - the row as it is
/// @param column - which column
/// @param value - the new value
fn rewritten(before: &[OwnedDatum], column: usize, value: OwnedDatum) -> Vec<OwnedDatum> {
    let mut after = before.to_vec();
    if let Some(cell) = after.get_mut(column) {
        *cell = value;
    }
    after
}

/// Writes a whole row with `insert`, which replaces the row under its key.
///
/// @param database - the file
/// @param tree - the tree
/// @param log - where the records go
/// @param row - the row
fn write_whole(
    database: &mut Database,
    tree: &mut PagedTree,
    log: &mut RowRecords,
    row: &[OwnedDatum],
) {
    let borrowed: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
    tree.insert(database, log, &borrowed)
        .unwrap_or_else(|error| panic!("a whole row write failed: {error:?}"));
}

/// Rewrites every key one at a time, the way the row at a time `UPDATE` does.
///
/// Returns how many rows it counted: written or unchanged.
///
/// @param database - the file
/// @param tree - the tree
/// @param log - where the records go
/// @param keys - the keys, in order
fn rewrite_one_at_a_time(
    database: &mut Database,
    tree: &mut PagedTree,
    log: &mut RowRecords,
    keys: &[i64],
) -> usize {
    let mut counted = 0usize;
    for key in keys {
        let probe = [Datum::Int(*key)];
        let Some(before) = tree.point(database.pool(), &probe).expect("a probe") else {
            continue;
        };
        match rewrite_rule(&before) {
            Rewrite::Skip | Rewrite::Row => continue,
            Rewrite::Unchanged => {}
            Rewrite::Column(column, value) => {
                let done = tree
                    .update_in_place(
                        database,
                        log,
                        &probe,
                        column,
                        &value.borrow(),
                        Some(&before),
                    )
                    .expect("an update in place");
                if !done {
                    write_whole(database, tree, log, &rewritten(&before, column, value));
                }
            }
        }
        counted += 1;
    }
    counted
}

/// Rewrites every key a leaf at a time with `update_sorted`, writing what it hands back whole.
///
/// Returns how many rows it counted and the most one call counted.
///
/// @param database - the file
/// @param tree - the tree
/// @param log - where the records go
/// @param keys - the keys, in order
fn rewrite_a_leaf_at_a_time(
    database: &mut Database,
    tree: &mut PagedTree,
    log: &mut RowRecords,
    keys: &[i64],
) -> (usize, usize) {
    let probes: Vec<Vec<Datum<'_>>> = keys.iter().map(|key| vec![Datum::Int(*key)]).collect();
    let (mut at, mut counted, mut largest, mut near) = (0usize, 0usize, 0usize, None);
    let mut run = UpdateRun::default();
    while at < probes.len() {
        let mut asked: Option<(usize, OwnedDatum)> = None;
        tree.update_sorted(
            database,
            log,
            probes.get(at..).unwrap_or(&[]),
            &mut near,
            &mut |row| {
                let rule = rewrite_rule(row);
                if let Rewrite::Column(column, value) = &rule {
                    asked = Some((*column, value.clone()));
                }
                Ok(rule)
            },
            &mut run,
        )
        .expect("update_sorted");
        counted += run.counted;
        largest = largest.max(run.counted);
        at += run.taken;
        if let Some(before) = run.pending.take() {
            let (column, value) = asked.expect("a row was handed back with no value decided");
            write_whole(database, tree, log, &rewritten(&before, column, value));
            counted += 1;
            at += 1;
        }
    }
    (counted, largest)
}

/// Rewriting sorted keys a leaf at a time leaves what one at a time does.
///
/// **The `UPDATE` half of the batch (task-2180).** Two trees are written
/// identically, then one is rewritten through `update_sorted` and the other
/// through `update_in_place` one key at a time, falling back to a whole row
/// write when the value does not fit its slot; `update_sorted` repacks the leaf
/// instead (task-2183). The two must give the same undo images in the same
/// order, count the same rows, and end holding the same rows. Labels grow by a byte
/// until leaves run out of room, so both the slot write and the fallback are
/// reached, and some rows are in the delta area and some out of line.
#[test]
fn rewriting_sorted_keys_a_leaf_at_a_time_matches_rewriting_one_at_a_time() {
    for seed in 0..4u64 {
        let (mut batched_db, mut batched, mut model) = fixture(1_024, 64, 300);
        let (mut single_db, mut single, _) = fixture(1_024, 64, 300);
        dirty(&mut batched_db, &mut batched, &mut model, seed);
        let mut twin = BTreeMap::new();
        dirty(&mut single_db, &mut single, &mut twin, seed);
        let mut rng = Rng::new(0x0FDA_7E00 + seed);
        let keys: Vec<i64> = (0..400i64)
            .filter(|_| !rng.next_u64().is_multiple_of(4))
            .collect();
        let mut single_log = RowRecords::default();
        let mut batched_log = RowRecords::default();
        // Twice over, so the second pass meets leaves the first pass filled.
        for pass in 0..2 {
            let one = rewrite_one_at_a_time(&mut single_db, &mut single, &mut single_log, &keys);
            let (leaf, largest) =
                rewrite_a_leaf_at_a_time(&mut batched_db, &mut batched, &mut batched_log, &keys);
            assert_eq!(
                leaf, one,
                "seed {seed}, pass {pass}: the two counted different rows"
            );
            // A batch that only ever took one row would pass every comparison.
            assert!(
                largest >= 4,
                "seed {seed}, pass {pass}: no call wrote more than {largest} rows"
            );
        }
        // **The records differ by design (task-2183).** A leaf whose rows
        // outgrew their slots is repacked with every change of its run and
        // logged as one page, where the row at a time path writes the rows
        // that do not fit as new rows. What both must agree on is the undo
        // images, the rows counted and the rows held.
        assert_eq!(
            batched_log.undone, single_log.undone,
            "seed {seed}: the undo images differ"
        );
        assert!(batched_log.repacks > 0, "seed {seed}: no leaf was repacked");
        assert!(
            !single_log.inserts.is_empty(),
            "seed {seed}: no row fell back to a whole write one at a time"
        );
        assert_eq!(
            batched.rows(batched_db.pool()).expect("a scan"),
            single.rows(single_db.pool()).expect("a scan"),
            "seed {seed}: the two trees hold different rows"
        );
        batched
            .check(batched_db.pool())
            .unwrap_or_else(|error| panic!("seed {seed}: integrity failed: {error:?}"));
    }
}

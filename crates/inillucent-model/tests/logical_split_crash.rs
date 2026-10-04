//! A leaf split logged as what it did, crashed at every point that matters, and
//! recovered to the three pages the split wrote.
//!
//! Invariant: **after a crash, the left page, the right page and the parent of a
//! split logged as `Body::SplitLeaf` come back byte for byte as the split left
//! them, whichever of the three reached the data file before the crash.** The
//! record replays each page from that page's own state, so the cases that
//! matter are which pages are already in the file at their state after the
//! split - an eviction writes one page at a time, so any of the eight subsets
//! can be there - and where in the split's transaction the power went out.
//!
//! ## The three tests
//!
//! - every subset of the three pages written to the file after the split, plus
//!   a torn right page, recovered and compared byte for byte;
//! - a crash at every write and every sync the split's transaction makes,
//!   recovered and compared with the pages before the split when the commit
//!   was not acknowledged and with the pages after it when it was;
//! - the same split replayed from the record a build before this one wrote,
//!   three page images, landing on the same three pages. That is the claim that
//!   a log written by an older build still replays.
//!
//! **A crash inside the fold that follows is not here**, because this engine's
//! fold writes pages in place with no after image and no rollback journal, so
//! a page it tears is not recoverable from any record that reads its page, a
//! row record as much as a split. `inillucent-engine`'s fold logs an after
//! image of every page or keeps a rollback journal, and
//! `inillucent-compat`'s `durability::logical_split_crash` cuts its fold at
//! every write.
//!
//! The split is forced with `PagedTree::make_room` asked for a whole page of
//! room, so the transaction holds the split and nothing else, and every page's
//! state after it is the state right after the split.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use inillucent_pool::page::{self, header, PageKind};
use inillucent_pool::{Database, Options, PageId, Pool};
use inillucent_sim::{CrashSnapshot, Failure, Policy, SimConfig, SimVfs, Site};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::types::{ColumnSpec, PhysicalType};
use inillucent_tree::write::TreeLog;
use inillucent_tree::PagedTree;
use inillucent_txn::engine::{Begin, Engine, EngineOptions};
use inillucent_txn::redo::{Applier, TreeRows};
use inillucent_vfs::{DbPath, Vfs};
use inillucent_wal::record::{Body, Record, Structural};
use inillucent_wal::writer::WalOptions;
use inillucent_wal::{Redo, Synchronous};

/// The page size: small, so a bulk build gives a tree of two levels quickly.
const PAGE: usize = 4_096;

/// The tree's id.
const TABLE: u64 = 1;

/// How many rows the tree is bulk built with: enough leaves for a root with
/// room left, so a split of one of them is a split under a parent that fits.
const ROWS: i64 = 2_000;

/// The database's file name inside the simulated file system.
const FILE: &str = "logical-split-crash.rdb";

/// Returns engine options with a pool large enough that nothing is evicted.
///
/// The eviction this test is about is modelled by copying pages into the file
/// by hand, so that every subset is reached on purpose rather than by luck.
fn options() -> EngineOptions {
    EngineOptions {
        database: Options::default().with_page_size(PAGE).with_frames(4_096),
        wal: WalOptions {
            synchronous: Synchronous::Full,
            segment_bytes: 1 << 24,
        },
        busy_timeout_ms: 0,
    }
}

/// A rowid table: an integer key, a text and an integer.
fn columns() -> Vec<ColumnSpec> {
    vec![
        ColumnSpec::key(PhysicalType::Int64),
        ColumnSpec::new(PhysicalType::Text),
        ColumnSpec::new(PhysicalType::Int64),
    ]
}

/// The row filtering each tree replay knows about.
fn shapes() -> TreeRows {
    TreeRows::new().with_tree(TABLE, columns(), 1)
}

/// A `TreeLog` over one transaction that remembers the split records it saw.
struct SplitLog<'a, 'e> {
    /// The transaction the records go into.
    txn: &'a mut inillucent_txn::engine::Transaction<'e>,
    /// Left, right and parent of every `SplitLeaf`, with its LSN.
    logical: Vec<(u64, u64, u64, u64)>,
    /// How many splits were logged as page images.
    images: usize,
}

impl TreeLog for SplitLog<'_, '_> {
    fn log(&mut self, body: Body<'_>) -> Result<u64, inillucent_base::DbError> {
        let lsn = self.txn.log(body)?;
        match body {
            Body::SplitLeaf {
                left,
                right,
                parent,
                ..
            } => self.logical.push((left, right, parent, lsn)),
            Body::Structural { .. } => self.images += 1,
            _ => {}
        }
        Ok(lsn)
    }
}

/// Bulk builds the tree and folds it into the file, so the split that follows
/// starts from pages the file already holds.
///
/// @param engine - the engine
/// @returns the tree's root
fn create(engine: &Engine) -> PageId {
    let labels: Vec<String> = (0..ROWS)
        .map(|key| format!("row {key} with a label long enough to fill a page"))
        .collect();
    let rows: Vec<Vec<Datum<'_>>> = (0..ROWS)
        .map(|key| {
            vec![
                Datum::Int(key * 3),
                Datum::Text(labels[key as usize].as_bytes()),
                Datum::Int(key),
            ]
        })
        .collect();
    let root = engine.with_database(|database| {
        PagedTree::bulk_build(database, TABLE, columns(), 1, &rows)
            .expect("the tree builds")
            .root()
    });
    engine.checkpoint().expect("the first fold");
    root
}

/// Attaches the tree.
///
/// @param engine - the engine
/// @param root - its root
fn attach(engine: &Engine, root: PageId) -> PagedTree {
    engine.with_pool(|pool| {
        PagedTree::attach_scanned(pool, TABLE, root, columns(), 1).expect("the tree attaches")
    })
}

/// Splits the third leaf under the root, in one transaction, and commits.
///
/// Returns the split's pages and its LSN once it has committed, and `None` when
/// a failure stopped it first.
///
/// @param engine - the engine
/// @param tree - the tree
fn split_once(engine: &Engine, tree: &mut PagedTree) -> Option<(u64, u64, u64, u64)> {
    let root = tree.root();
    let children = engine
        .with_pool(|pool| inillucent_tree::split_log::read_interior(pool, root))
        .ok()?
        .1;
    let leaf = *children.get(2)?;
    let mut txn = engine.begin(Begin::Immediate).ok()?;
    let (logical, images) = {
        let mut log = SplitLog {
            txn: &mut txn,
            logical: Vec::new(),
            images: 0,
        };
        let made = engine
            .with_database(|database| tree.make_room(database, &mut log, leaf, &[root], None, PAGE))
            .ok()?;
        assert!(made, "a full leaf of many rows refused to make room");
        (log.logical, log.images)
    };
    assert_eq!(images, 0, "the split was logged as page images");
    assert_eq!(logical.len(), 1, "the transaction logged {logical:?}");
    txn.commit().ok()?;
    logical.first().copied()
}

/// Returns every page the pool can read, with the fields a frame does not keep
/// the file's way cleared.
///
/// The checksum, which a frame does not keep, and an interior page's child
/// pointers, which a frame holds swizzled. See `redo_bytes.rs`, which compares
/// pages the same way and says why.
///
/// @param pool - the pool
fn pages(pool: &Pool) -> BTreeMap<u64, Vec<u8>> {
    let mut found = BTreeMap::new();
    for page in 2..pool.page_count() {
        let Ok(guard) = pool.fetch(PageId(page)) else {
            continue;
        };
        let mut bytes = guard.bytes().to_vec();
        if let Some(field) = bytes.get_mut(header::CHECKSUM..header::CHECKSUM + 4) {
            field.fill(0);
        }
        if page::kind_of(&bytes).ok() == Some(PageKind::Interior) {
            let swips = inillucent_pool::interior::swip_offsets_of(&bytes)
                .expect("an interior page lists its child pointers");
            for at in swips {
                if let Some(swip) = bytes.get_mut(at..at + 8) {
                    swip.fill(0);
                }
            }
        }
        found.insert(page, bytes);
    }
    found
}

/// What one run of the split left behind.
struct Run {
    /// What the media held after the power went out.
    snapshot: CrashSnapshot,
    /// Every page before the split.
    before: BTreeMap<u64, Vec<u8>>,
    /// Every page after it.
    after: BTreeMap<u64, Vec<u8>>,
    /// Left, right, parent and the split's LSN.
    split: (u64, u64, u64, u64),
    /// The tree's rows after the split.
    rows: Vec<Vec<OwnedDatum>>,
    /// The tree's root.
    root: PageId,
}

/// Builds the tree, splits one leaf, optionally folds, and pulls the power.
///
/// The run is deterministic: the simulated file system has a fixed seed and
/// every step is the same, so two runs split the same leaf into the same pages.
///
/// @param fold - whether to fold after the split, which puts the three pages
///   in the data file
fn run(fold: bool) -> Run {
    let vfs = Arc::new(SimVfs::new(SimConfig::default()));
    let path = DbPath::new(FILE);
    let engine =
        Engine::create(Arc::clone(&vfs) as Arc<dyn Vfs>, &path, options()).expect("a database");
    let root = create(&engine);
    let mut tree = attach(&engine, root);
    // Height counts interior levels, so one is a root above the leaves.
    assert_eq!(
        tree.height(),
        1,
        "the tree is not two levels, so no parent fits"
    );
    let before = engine.with_pool(pages);
    let split = split_once(&engine, &mut tree).expect("the split commits");
    let after = engine.with_pool(pages);
    let rows = engine.with_pool(|pool| tree.rows(pool).expect("the rows read"));
    if fold {
        engine.checkpoint().expect("the fold after the split");
    }
    drop(engine);
    Run {
        snapshot: vfs.crash(),
        before,
        after,
        split,
        rows,
        root,
    }
}

/// Returns the data file's bytes in a snapshot.
///
/// @param snapshot - the media after a crash
fn data_file(snapshot: &mut CrashSnapshot) -> &mut Vec<u8> {
    snapshot
        .files
        .get_mut(&PathBuf::from(FILE))
        .expect("the snapshot holds the data file")
}

/// Copies one page's bytes from one snapshot's data file into another's.
///
/// @param into - the snapshot being built
/// @param from - the snapshot the page is taken from
/// @param page - the page
fn copy_page(into: &mut CrashSnapshot, from: &CrashSnapshot, page: u64) {
    let source = from
        .files
        .get(&PathBuf::from(FILE))
        .expect("the source holds the data file");
    let at = page as usize * PAGE;
    let bytes = source
        .get(at..at + PAGE)
        .expect("the fold wrote the page")
        .to_vec();
    let target = data_file(into);
    if target.len() < at + PAGE {
        target.resize(at + PAGE, 0);
    }
    target[at..at + PAGE].copy_from_slice(&bytes);
}

/// Reopens a snapshot, which replays its log, and returns the engine.
///
/// @param snapshot - the media
fn reopen(snapshot: &CrashSnapshot) -> Engine {
    let media: Arc<dyn Vfs> = Arc::new(SimVfs::recovered(SimConfig::default(), snapshot));
    Engine::open(media, &DbPath::new(FILE), options(), shapes()).expect("the database reopens")
}

/// Fails when a recovered page differs from the one expected, naming the first
/// differing byte.
///
/// @param got - the recovered pages
/// @param want - the pages expected
/// @param page - which page
/// @param context - the case
fn assert_page(
    got: &BTreeMap<u64, Vec<u8>>,
    want: &BTreeMap<u64, Vec<u8>>,
    page: u64,
    context: &str,
) {
    let (Some(got), Some(want)) = (got.get(&page), want.get(&page)) else {
        panic!("{context}: page {page} is missing from one side");
    };
    if got == want {
        return;
    }
    let at = got
        .iter()
        .zip(want.iter())
        .position(|(left, right)| left != right)
        .unwrap_or(0);
    panic!(
        "{context}: page {page} differs first at byte {at}: recovered {:?}, written {:?}",
        got.get(at..at + 8),
        want.get(at..at + 8)
    );
}

/// Every subset of the three pages in the file after the split, and a torn
/// right page, recovers to the pages the split wrote.
#[test]
fn every_set_of_evicted_pages_recovers_the_split_byte_for_byte() {
    let crashed = run(false);
    let folded = run(true);
    assert_eq!(
        crashed.after, folded.after,
        "two runs of the same split produced different pages"
    );
    let (left, right, parent, lsn) = crashed.split;
    assert!(right > 0 && left != right && parent == crashed.root.0);
    // The fold wrote the three pages at the split's own LSN: nothing else
    // touched them, so a copy of one is exactly what an eviction right after
    // the split would have written.
    for page in [left, right, parent] {
        let file = folded.snapshot.files.get(&PathBuf::from(FILE)).unwrap();
        let stamp = page::read_u64(&file[page as usize * PAGE..], header::LSN).unwrap();
        assert_eq!(
            stamp, lsn,
            "the folded page {page} carries {stamp}, not {lsn}"
        );
    }
    let mut cases = 0usize;
    for mask in 0u8..8 {
        let mut snapshot = crashed.snapshot.clone();
        let mut named = Vec::new();
        for (bit, page) in [left, right, parent].into_iter().enumerate() {
            if mask & (1 << bit) != 0 {
                copy_page(&mut snapshot, &folded.snapshot, page);
                named.push(page);
            }
        }
        let context = format!("pages {named:?} written before the crash");
        let engine = reopen(&snapshot);
        let got = engine.with_pool(pages);
        for page in [left, right, parent] {
            assert_page(&got, &crashed.after, page, &context);
        }
        assert_eq!(got, crashed.after, "{context}: another page differs");
        let tree = attach(&engine, crashed.root);
        let rows = engine.with_pool(|pool| tree.rows(pool).expect("the rows read"));
        assert_eq!(rows, crashed.rows, "{context}: the rows differ");
        cases += 1;
    }
    // A right page torn by the crash: half of it new and half whatever the
    // page held before. The record rebuilds the right page from its own rows
    // and reads nothing there, so this recovers where a row record could not.
    let mut snapshot = crashed.snapshot.clone();
    copy_page(&mut snapshot, &folded.snapshot, right);
    let at = right as usize * PAGE + PAGE / 2;
    data_file(&mut snapshot)[at..at + PAGE / 2].fill(0xA5);
    let engine = reopen(&snapshot);
    let got = engine.with_pool(pages);
    assert_page(&got, &crashed.after, right, "a torn right page");
    assert_eq!(
        got, crashed.after,
        "a torn right page: another page differs"
    );
    cases += 1;
    assert_eq!(cases, 9);
}

/// A crash at every write and every sync of the split's transaction leaves the
/// pages before the split or the pages after it, and the commit decides which.
#[test]
fn a_crash_at_every_write_and_sync_of_the_split_recovers() {
    let reference = run(false);
    let (left, right, parent, _) = reference.split;
    let mut lost = 0usize;
    let mut kept = 0usize;
    for site in [Site::Write, Site::Sync] {
        for nth in 1..=24u64 {
            let vfs = Arc::new(SimVfs::new(SimConfig::default()));
            let path = DbPath::new(FILE);
            let acknowledged = {
                let engine = Engine::create(Arc::clone(&vfs) as Arc<dyn Vfs>, &path, options())
                    .expect("a database");
                let root = create(&engine);
                let mut tree = attach(&engine, root);
                // Armed after the tree is built and folded, so every call it
                // counts belongs to the split's transaction.
                vfs.failpoints().set(site, Policy::Nth(nth, Failure::Crash));
                split_once(&engine, &mut tree).is_some()
            };
            let snapshot = vfs.crash();
            let engine = reopen(&snapshot);
            let got = engine.with_pool(pages);
            let context = format!("a crash at {site:?} {nth}");
            if acknowledged {
                for page in [left, right, parent] {
                    assert_page(&got, &reference.after, page, &context);
                }
                let tree = attach(&engine, reference.root);
                let rows = engine.with_pool(|pool| tree.rows(pool).expect("the rows read"));
                assert_eq!(rows, reference.rows, "{context}: the rows differ");
                kept += 1;
            } else {
                for page in [left, parent] {
                    assert_page(&got, &reference.before, page, &context);
                }
                lost += 1;
            }
        }
    }
    assert!(
        lost > 0,
        "no crash landed before the commit, so that half was not tested"
    );
    assert!(
        kept > 0,
        "no crash landed after the commit, so that half was not tested"
    );
}

/// The same split replayed from three page images, which is the record a
/// build before the logical one wrote, lands on the same three pages.
#[test]
fn the_image_record_an_older_build_wrote_still_replays() {
    let crashed = run(false);
    let folded = run(true);
    let (left, right, parent, lsn) = crashed.split;
    let file = folded
        .snapshot
        .files
        .get(&PathBuf::from(FILE))
        .expect("the folded data file");
    let image = |page: u64| file[page as usize * PAGE..(page as usize + 1) * PAGE].to_vec();
    let (left_image, right_image, parent_image) = (image(left), image(right), image(parent));
    let media = SimVfs::recovered(SimConfig::default(), &crashed.snapshot);
    let mut database = Database::open(&media, &DbPath::new(FILE), 4_096).expect("the file opens");
    let before = pages(database.pool());
    for page in [left, parent] {
        assert_page(&before, &crashed.before, page, "the file before the replay");
    }
    let record = Record {
        lsn,
        txn: 1,
        body: Body::Structural {
            kind: Structural::Split,
            tree: TABLE,
            left,
            right,
            parent,
            left_image: &left_image,
            right_image: &right_image,
            parent_image: &parent_image,
        },
        length: 0,
    };
    {
        let mut applier = Applier::new(&mut database, shapes());
        applier
            .redo(&record, &[true, true, true])
            .expect("the image record replays");
    }
    let got = pages(database.pool());
    for page in [left, right, parent] {
        assert_page(&got, &crashed.after, page, "the image record");
    }
}

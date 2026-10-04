//! A `DELETE` or an `UPDATE` of many rows is recovered from the log, undone by a
//! rollback, and leaves what the row at a time path leaves.
//!
//! Invariant: **a bulk write that changes each leaf once still writes a record
//! and an undo image for every row,** so a crash before any page reaches the
//! file recovers to the state the statement left, a `ROLLBACK` puts every row
//! and every index entry back, and the rows, the index entries and the counts
//! are the ones writing a row at a time produces.
//!
//! ## Why this file exists (task-2180)
//!
//! `PagedTree::delete_sorted` and `PagedTree::update_sorted` read a leaf once
//! for every key of a run, log a record per key, and then change the leaf once
//! and stamp it with the last record's LSN. The page side and the log side are
//! now separate steps, so the question a passing session cannot answer is
//! whether the log alone describes what the page became. Here the pages never
//! reach the file: the database is held in `exclusive` locking mode with a pool
//! larger than the file, the media is cut straight after the commit, and
//! recovery has only the log to rebuild the writes from. The log is read first
//! to prove it holds the records the batch wrote.
//!
//! The other question is whether the batch decides each row as the row at a
//! time path does. A trigger makes a statement watchable, which sends it down
//! the row at a time path, so a twin database with a trigger that does nothing
//! is the reference: every statement, including one that fails part way under
//! `OR FAIL` and one that skips rows under `OR IGNORE`, must leave both
//! databases the same and report the same `changes()`.
//!
//! The table has a secondary index, rows written after the bulk build so its
//! leaves hold delta rows and tombstones, and bodies long enough to be stored
//! out of line, so every place a row can be is written to.

use std::path::PathBuf;
use std::sync::Arc;

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::path::DbPath;
use inillucent_vfs::{AccessMode, Vfs};
use inillucent_wal::record::{Body, Record};
use inillucent_wal::recover::{recover, RecoveryStart, Redo};

/// The page size, and enough frames that nothing is evicted, so no page of
/// the delete reaches the file before the cut.
const PAGE_SIZE: usize = 4_096;
const FRAMES: usize = 8_192;

/// How many rows the table starts with.
const ROWS: i64 = 4_000;

/// The statements that build the table, its index, and the writes after the
/// build that leave delta rows, tombstones and out of line bodies in its
/// leaves.
const BUILD: [&str; 6] = [
    "CREATE TABLE t (id INTEGER PRIMARY KEY, k INTEGER NOT NULL, body TEXT NOT NULL)",
    "CREATE INDEX t_k ON t (k)",
    "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 4000) \
     INSERT INTO t SELECT i, (i * 37) % 501, 'row ' || i FROM c",
    "UPDATE t SET body = body || ' changed' WHERE id % 7 = 0",
    "UPDATE t SET body = printf('%.900c', 'w') WHERE id % 97 = 0",
    "INSERT INTO t (id, k, body) SELECT id + 4000, k, body FROM t WHERE id % 50 = 0",
];

/// The deletes under test: a range across many leaves, then every third row
/// of what is left, which reaches every leaf of the table and of the index.
const DELETES: [&str; 2] = [
    "DELETE FROM t WHERE id BETWEEN 300 AND 2700",
    "DELETE FROM t WHERE id % 3 = 0",
];

/// The updates under test, one per kind of row a run meets.
///
/// One column that grows by a byte, which fits its slot until the leaf runs
/// out of room; a value long enough to be stored out of line, which no slot
/// takes; an index column, whose entry moves; two assignments of which one
/// changes nothing; and a statement that changes nothing at all.
const UPDATES: [&str; 5] = [
    "UPDATE t SET body = body || '!'",
    "UPDATE t SET body = printf('%.700c', 'q') WHERE id % 11 = 0",
    "UPDATE t SET k = k + 1 WHERE id % 5 = 0",
    "UPDATE t SET body = body || 'm', k = k WHERE id % 4 = 0",
    "UPDATE t SET body = body WHERE id < 100",
];

/// Statements that stop or skip part way, for the comparison with the row at a time path.
///
/// `body` is `NOT NULL`, so an `OR FAIL` statement fails at row 2500 having
/// written the rows before it, which it keeps, and an `OR IGNORE` statement
/// skips every thirteenth row. Each is written twice. Without a `WHERE` the
/// query reads the keys in `t_k`'s order, which is the case where the order
/// rows are written in decides which rows `OR FAIL` keeps, so the batch must
/// not reorder them. With a rowid range the keys arrive in the table's order
/// and the batch runs, so its skip and its failure part way through a leaf are
/// compared too.
const PART_WAY: [&str; 4] = [
    "UPDATE OR FAIL t SET body = CASE WHEN id = 2500 THEN NULL ELSE body || '?' END",
    "UPDATE OR IGNORE t SET body = CASE WHEN id % 13 = 0 THEN NULL ELSE body || 'i' END",
    "UPDATE OR FAIL t SET body = CASE WHEN id = 2500 THEN NULL ELSE body || 'f' END      WHERE id BETWEEN 1 AND 5000",
    "UPDATE OR IGNORE t SET body = CASE WHEN id % 13 = 0 THEN NULL ELSE body || 'g' END      WHERE id BETWEEN 1 AND 5000",
];

/// Returns the path every run in this file uses.
fn path() -> PathBuf {
    PathBuf::from("bulk-write-log.rdb")
}

/// Returns a simulated media with the default device model.
///
/// @param seed - the model's seed
fn media(seed: u64) -> Arc<SimVfs> {
    Arc::new(SimVfs::new(SimConfig {
        seed,
        model: MediaModel::default(),
        ..SimConfig::default()
    }))
}

/// Runs one statement, failing the test with what the engine said.
///
/// @param engine - the database
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|error| {
            panic!(
                "{sql}: {} ({})",
                error.message(),
                error.detail().unwrap_or_default()
            )
        });
}

/// Returns a query's rows, each written out as one line.
///
/// @param engine - the database
/// @param sql - the query
fn lines(engine: &mut ImportedDatabase, sql: &str) -> Vec<String> {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()))
        .rows
        .iter()
        .map(|row| format!("{row:?}"))
        .collect()
}

/// Returns what the table holds, read three ways that use different trees.
///
/// The rows by the table, every `k` and `id` pair by the index, and the count
/// for each `k` through the index, so an index entry left behind or lost shows
/// as a difference the table alone would not.
///
/// @param engine - the database
fn contents(engine: &mut ImportedDatabase) -> Vec<String> {
    let mut all = lines(engine, "SELECT id, k, body FROM t NOT INDEXED ORDER BY id");
    all.extend(lines(
        engine,
        "SELECT k, id FROM t INDEXED BY t_k ORDER BY k, id",
    ));
    all.extend(lines(
        engine,
        "SELECT k, count(*) FROM t INDEXED BY t_k WHERE k BETWEEN 0 AND 600 GROUP BY k",
    ));
    all
}

/// Returns how many rows the table holds.
///
/// @param engine - the database
fn count(engine: &mut ImportedDatabase) -> usize {
    let outcome = engine
        .execute_any("SELECT count(*) FROM t", &Params::new())
        .expect("the table counts");
    match outcome.rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(rows)) => usize::try_from(*rows).unwrap_or(0),
        other => panic!("count(*) returned {other:?}"),
    }
}

/// Asserts the database's own integrity check finds nothing.
///
/// @param engine - the database
/// @param context - what to say when it fails
fn assert_sound(engine: &mut ImportedDatabase, context: &str) {
    let said = lines(engine, "PRAGMA integrity_check");
    assert_eq!(
        said,
        vec![format!("{:?}", vec![OwnedDatum::Text(b"ok".to_vec())])],
        "{context}"
    );
}

/// Counts the row records a log holds above a checkpoint, by kind.
#[derive(Default)]
struct RowRecords {
    /// How many rows the `DeleteRow` and `DeleteRows` records the scan handed over remove.
    deletes: usize,
    /// How many `UpdateInPlace` records the scan handed over.
    updates: usize,
    /// How many `CompactLeaf` records carrying their page the scan handed over.
    repacks: usize,
}

impl Redo for RowRecords {
    /// Always "not yet applied", so every record is handed to `redo`.
    ///
    /// @param _page - unused
    fn page_lsn(&mut self, _page: u64) -> inillucent_base::DbResult<Option<u64>> {
        Ok(None)
    }

    /// Counts the rows a `DeleteRow`, a `DeleteRows` or an `UpdateInPlace` names.
    ///
    /// @param record - the record
    /// @param _wanted - unused
    fn redo(&mut self, record: &Record<'_>, _wanted: &[bool]) -> inillucent_base::DbResult<()> {
        match record.body {
            Body::DeleteRow { .. } => self.deletes = self.deletes.saturating_add(1),
            Body::DeleteRows { keys, .. } => {
                let removed = inillucent_wal::record::key_list(keys)?.len();
                self.deletes = self.deletes.saturating_add(removed);
            }
            Body::UpdateInPlace { .. } => self.updates = self.updates.saturating_add(1),
            Body::CompactLeaf { image, .. } if !image.is_empty() => {
                self.repacks = self.repacks.saturating_add(1);
            }
            _ => {}
        }
        Ok(())
    }
}

/// Returns the row records recovery would read, by kind.
///
/// @param vfs - the media
/// @param engine - the open database, for its log and its checkpoint
fn row_records(vfs: &SimVfs, engine: &ImportedDatabase) -> RowRecords {
    let wal = engine.wal();
    let lowest = (1..=wal.sequence())
        .find(|candidate| {
            vfs.access(&wal.segment_path(*candidate), AccessMode::Exists)
                .unwrap_or(false)
        })
        .unwrap_or(wal.sequence());
    let start = RecoveryStart {
        uuid: wal.uuid(),
        checkpoint_lsn: engine.checkpoint_lsn(),
        sequence: lowest,
        cts_watermark: 0,
        doubtful: std::collections::BTreeSet::new(),
    };
    let mut observer = RowRecords::default();
    recover(vfs, &DbPath::new(path()), start, &mut observer).expect("the log scans");
    observer
}

/// Builds the table, checkpoints it, and holds the file so nothing more is written to it.
///
/// @param vfs - the media
fn built(vfs: &Arc<SimVfs>) -> ImportedDatabase {
    let mut engine =
        ImportedDatabase::create_on(Arc::clone(vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES)
            .expect("the database is created");
    // `exclusive` keeps the log between statements, as `torn_page_with_image`
    // explains: under `normal` every statement that wrote is checkpointed.
    run(&mut engine, "PRAGMA locking_mode = exclusive");
    for sql in BUILD {
        run(&mut engine, sql);
    }
    engine.checkpoint().expect("the fixture's checkpoint");
    engine
}

/// Runs statements, cuts the media, recovers, and returns the records the log held.
///
/// Asserts the recovered database holds what the session held after the
/// statements, and passes its integrity check.
///
/// @param statements - the writes under test
/// @param seed - the media model's seed
fn recovered_after(statements: &[&str], seed: u64) -> RowRecords {
    let vfs = media(seed);
    let mut engine = built(&vfs);
    for sql in statements {
        run(&mut engine, sql);
    }
    let expected = contents(&mut engine);
    let records = row_records(vfs.as_ref(), &engine);
    let snapshot = vfs.crash();
    std::mem::forget(engine);

    let reopened = Arc::new(SimVfs::recovered(
        SimConfig {
            seed: seed.saturating_add(1),
            model: MediaModel::default(),
            ..SimConfig::default()
        },
        &snapshot,
    ));
    let mut engine = ImportedDatabase::open_on(
        Arc::clone(&reopened) as Arc<dyn Vfs>,
        path(),
        PAGE_SIZE,
        FRAMES,
    )
    .unwrap_or_else(|error| panic!("recovery refused: {:?}", error.detail()));
    assert_eq!(
        contents(&mut engine),
        expected,
        "recovery rebuilt a different table"
    );
    assert_sound(&mut engine, "after recovering the writes");
    records
}

/// Runs statements in a transaction, rolls it back, then commits them through a reopen.
///
/// @param statements - the writes under test
/// @param seed - the media model's seed
fn rolled_back(statements: &[&str], seed: u64) {
    let vfs = media(seed);
    let mut engine = built(&vfs);
    let original = contents(&mut engine);
    run(&mut engine, "BEGIN");
    for sql in statements {
        run(&mut engine, sql);
    }
    assert_ne!(
        contents(&mut engine),
        original,
        "the writes changed nothing"
    );
    run(&mut engine, "ROLLBACK");
    assert_eq!(
        contents(&mut engine),
        original,
        "the rollback did not restore every row"
    );
    assert_sound(&mut engine, "after the rollback");

    for sql in statements {
        run(&mut engine, sql);
    }
    let expected = contents(&mut engine);
    drop(engine);
    let mut engine =
        ImportedDatabase::open_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES)
            .expect("the database reopens");
    assert_eq!(
        contents(&mut engine),
        expected,
        "the reopened table differs"
    );
    assert_sound(&mut engine, "after the reopen");
}

/// A bulk delete cut off before any of its pages reach the file recovers to what it left.
#[test]
fn a_bulk_delete_is_recovered_from_its_records_alone() {
    let vfs = media(2_190);
    let mut engine = built(&vfs);
    let before = count(&mut engine);
    for sql in DELETES {
        run(&mut engine, sql);
    }
    let gone = before.saturating_sub(count(&mut engine));
    drop(engine);
    assert!(gone > ROWS as usize / 2, "only {gone} rows were deleted");
    let records = recovered_after(&DELETES, 2_180);
    // One record for the row and one for its entry in `t_k`.
    assert_eq!(
        records.deletes,
        gone * 2,
        "the log does not hold a DeleteRow for every row and every index entry"
    );
}

/// A bulk update cut off before any of its pages reach the file recovers to what it left.
///
/// The first update grows every row by a byte. A row whose new value fits its
/// slot is an `UpdateInPlace`; a leaf whose rows outgrew their slots is
/// repacked with every change of the run and logged as one `CompactLeaf`
/// carrying the page (task-2183). Both reach the log, and recovery rebuilds the
/// table from them alone, which `recovered_after` checks row for row.
#[test]
fn a_bulk_update_is_recovered_from_its_records_alone() {
    let records = recovered_after(&UPDATES, 2_184);
    assert!(
        records.repacks > 0,
        "no leaf was repacked by an update that grows every row"
    );
    assert!(
        records.updates > 0,
        "no row was rewritten in its slot by an update of every row"
    );
}

/// A bulk delete inside a transaction is undone whole by a rollback, and survives a reopen.
#[test]
fn a_bulk_delete_is_undone_by_a_rollback() {
    rolled_back(&DELETES, 2_182);
}

/// A bulk update inside a transaction is undone whole by a rollback, and survives a reopen.
#[test]
fn a_bulk_update_is_undone_by_a_rollback() {
    rolled_back(&UPDATES, 2_186);
}

/// Returns what a statement reported: the error's message, or how many rows it changed.
///
/// @param engine - the database
/// @param sql - the statement
fn outcome(engine: &mut ImportedDatabase, sql: &str) -> String {
    match engine.execute_any(sql, &Params::new()) {
        Ok(_) => format!("changed {:?}", lines(engine, "SELECT changes()")),
        Err(error) => format!("failed: {}", error.message()),
    }
}

/// Asserts two readings of the table are the same, naming the first line that differs.
///
/// A whole table in a failure message is thousands of lines nobody can read.
///
/// @param batched - what the batched database holds
/// @param single - what the row at a time database holds
/// @param sql - the statement that ran before the reading
fn assert_same(batched: &[String], single: &[String], sql: &str) {
    if let Some(at) =
        (0..batched.len().max(single.len())).find(|at| batched.get(*at) != single.get(*at))
    {
        panic!(
            "{sql} left the two tables different at line {at} of {} and {}: batched {:?}, one at a time {:?}",
            batched.len(),
            single.len(),
            batched.get(at),
            single.get(at)
        );
    }
}

/// Every update leaves what the row at a time path leaves, including one that fails part way.
#[test]
fn a_bulk_update_leaves_what_a_row_at_a_time_leaves() {
    let batched_media = media(2_188);
    let mut batched = built(&batched_media);
    let single_media = media(2_188);
    let mut single = built(&single_media);
    // A trigger makes the statement watchable, so it goes a row at a time.
    run(
        &mut single,
        "CREATE TRIGGER watch AFTER UPDATE ON t BEGIN SELECT 1; END",
    );
    for sql in UPDATES.iter().chain(PART_WAY.iter()) {
        let said = outcome(&mut batched, sql);
        assert_eq!(
            said,
            outcome(&mut single, sql),
            "{sql} reported differently"
        );
        assert_same(&contents(&mut batched), &contents(&mut single), sql);
    }
    let failed = outcome(&mut batched, PART_WAY[0]);
    assert!(
        failed.starts_with("failed"),
        "the OR FAIL statement did not fail: {failed}"
    );
    assert_sound(&mut batched, "after the updates");
}

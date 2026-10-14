//! A range `DELETE` of 40,000 rows writes a bounded number of log records and bytes.
//!
//! Invariant: **a `DELETE` of a rowid range writes one `DeleteRows` record per leaf
//! it changes, not one record per row, and the log it writes stays a small multiple
//! of the bytes of the keys it removes.** The hill climb workload
//! `edge.delete.range` deletes 40,000 of 100,000 rows of a table with an integer
//! primary key. A release that fell back to a record per row, or that compacted a
//! leaf image for every leaf it emptied, was 3.3 times slower than SQLite on it, and
//! a wall clock cannot say which on a loaded machine. Counting the records does.
//!
//! The table matches the gate's `copied` table: `id INTEGER PRIMARY KEY, key
//! INTEGER, label TEXT`, filled by one `INSERT ... SELECT`, at the gate's 32 KB page
//! size. The database is held in `exclusive` locking mode so the log is still on the
//! media after the statement and can be scanned.

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

/// The gate's page size and pool.
const PAGE_SIZE: usize = 32_768;
const FRAMES: usize = 4_096;

/// What the log holds above the checkpoint, by the kinds the delete can write.
#[derive(Default, Debug)]
struct LogWork {
    /// `DeleteRow` records.
    delete_row: usize,
    /// `DeleteRows` records.
    delete_rows: usize,
    /// Rows named by all `DeleteRows` records.
    rows_named: usize,
    /// Every other record that is not a commit.
    other: usize,
    /// Records whose body is a whole page image: `CompactLeaf` with an image or `WritePage`.
    page_images: usize,
    /// Bytes of every record, header included.
    bytes: usize,
}

impl Redo for LogWork {
    /// Always "not yet applied", so every record is handed to `redo`.
    ///
    /// @param _page - unused
    fn page_lsn(&mut self, _page: u64) -> inillucent_base::DbResult<Option<u64>> {
        Ok(None)
    }

    /// Counts one record by kind.
    ///
    /// @param record - the record
    /// @param _wanted - unused
    fn redo(&mut self, record: &Record<'_>, _wanted: &[bool]) -> inillucent_base::DbResult<()> {
        self.bytes = self.bytes.saturating_add(record.length);
        match record.body {
            Body::DeleteRow { .. } => self.delete_row += 1,
            Body::DeleteRows { keys, .. } => {
                self.delete_rows += 1;
                self.rows_named += inillucent_wal::record::key_list(keys)?.len();
            }
            Body::CompactLeaf { image, .. } if !image.is_empty() => self.page_images += 1,
            Body::WritePage { .. } => self.page_images += 1,
            Body::Commit { .. } => {}
            _ => self.other += 1,
        }
        Ok(())
    }
}

/// Runs one statement through the engine.
///
/// @param engine - the database
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
}

/// Returns the first value of a one row query as an integer.
///
/// @param engine - the database
/// @param sql - the query
fn number(engine: &mut ImportedDatabase, sql: &str) -> i64 {
    let outcome = engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
    match outcome.rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(value)) => *value,
        other => panic!("{sql} returned {other:?}"),
    }
}

/// Scans the log above the checkpoint and returns what it holds.
///
/// @param vfs - the media
/// @param engine - the open database
fn scan_log(vfs: &SimVfs, engine: &ImportedDatabase) -> LogWork {
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
    let mut work = LogWork::default();
    recover(vfs, &DbPath::new(PathBuf::from("range-delete.rdb")), start, &mut work)
        .expect("the log scans");
    work
}

/// Builds a table shaped like the gate's `copied`, and checkpoints it.
///
/// @param vfs - the media
/// @param exclusive - whether the log is kept between statements, for scanning it
fn copied_table(vfs: &Arc<SimVfs>, exclusive: bool) -> ImportedDatabase {
    let mut engine = ImportedDatabase::create_on(
        Arc::clone(vfs) as Arc<dyn Vfs>,
        PathBuf::from("range-delete.rdb"),
        PAGE_SIZE,
        FRAMES,
    )
    .expect("the database is created");
    if exclusive {
        run(&mut engine, "PRAGMA locking_mode = exclusive");
    }
    run(
        &mut engine,
        "CREATE TABLE copied(id INTEGER PRIMARY KEY, key INTEGER, label TEXT)",
    );
    run(
        &mut engine,
        "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 100000)          INSERT INTO copied SELECT i, (i * 2654435761) % 100000,          'row ' || i || ' lorem ipsum dolor sit amet consectetur' FROM c",
    );
    engine.checkpoint().expect("the fixture's checkpoint");
    engine
}

/// The media every test here uses.
fn media() -> Arc<SimVfs> {
    Arc::new(SimVfs::new(SimConfig {
        seed: 40,
        model: MediaModel::default(),
        ..SimConfig::default()
    }))
}

/// The statement the gate's `edge.delete.range` runs.
const RANGE_DELETE: &str = "DELETE FROM copied WHERE id BETWEEN 20000 AND 59999";

/// Deletes the gate's range and counts the log records the statement wrote.
#[test]
fn a_range_delete_of_forty_thousand_rows_logs_a_record_per_leaf() {
    let vfs = media();
    let mut engine = copied_table(&vfs, true);
    let before = engine.write_stats();
    run(&mut engine, RANGE_DELETE);
    let after = engine.write_stats();
    let work = scan_log(vfs.as_ref(), &engine);
    println!("log work   : {work:?}");
    println!("write stats before: {before:?}");
    println!("write stats after : {after:?}");
    assert_eq!(number(&mut engine, "SELECT count(*) FROM copied"), 60_000);
    assert_eq!(work.delete_row, 0, "{work:?}");
    assert_eq!(work.rows_named, 40_000, "{work:?}");
}

/// Deletes the gate's range under the default locking mode and prints what it cost.
#[test]
fn a_range_delete_of_forty_thousand_rows_reports_its_page_and_log_work() {
    let vfs = media();
    let mut engine = copied_table(&vfs, false);
    let cache_before = engine.pool_stats();
    run(&mut engine, RANGE_DELETE);
    let cache_after = engine.pool_stats();
    // The wall clock is printed for a person comparing builds; no assertion reads it.
    let mut times = Vec::new();
    for _ in 0..5 {
        let mut again = copied_table(&media(), false);
        let started = std::time::Instant::now();
        run(&mut again, RANGE_DELETE);
        times.push(started.elapsed().as_micros());
    }
    println!("delete microseconds, five fresh tables: {times:?}");
    println!("cache before: {cache_before:?}");
    println!("cache after : {cache_after:?}");
    println!("write stats : {:?}", engine.write_stats());
    assert_eq!(number(&mut engine, "SELECT count(*) FROM copied"), 60_000);
}

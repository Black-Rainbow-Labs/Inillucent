//! Power loss at every sync of an ordinary checkpoint.
//!
//! Invariant: **a checkpoint makes every data page durable before any meta
//! record that moves the recovery point past the log records for those pages.**
//! A power loss inside the sync that carried both could leave the new meta
//! record on the media beside a page that was torn, and the next open would read
//! that page with no log record left to repair it. The checkpoint under test is
//! the one `PRAGMA wal_checkpoint` makes after a `DELETE`, on a file with
//! `auto_vacuum` off, so no shrink is involved.
//!
//! Each sync of the run is cut in turn, under several seeds of the device model,
//! because which sectors a power loss tears is random.

use std::sync::Arc;

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::failpoint::{Failure, Policy, Site};
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{CrashSnapshot, SimConfig, SimVfs};
use inillucent_vfs::Vfs;

/// The page size every case builds at.
const PAGE_SIZE: usize = 1_024;

/// How many frames the pool holds.
const FRAMES: usize = 4_096;

/// How many device seeds each sync is cut under.
const SEEDS: u64 = 24;

/// The database every run starts from: a table three levels deep, an index, and
/// values that live on shared pages and on chains of pages.
const SCHEMA: &str = "CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
     CREATE INDEX tb ON t(b);
     CREATE TABLE v(a INTEGER PRIMARY KEY, b TEXT);
     CREATE TABLE s(a INTEGER PRIMARY KEY, b TEXT);
     INSERT INTO t SELECT value, printf('%06d', value) || printf('%.*c', 100, 'x')
       FROM generate_series(1, 500);
     INSERT INTO v SELECT value, hex(randomblob(10000)) FROM generate_series(1, 4);
     INSERT INTO s SELECT value, hex(randomblob(200)) FROM generate_series(1, 100);";

/// The statements the cut lands inside.
const WORKLOAD: &str =
    "DELETE FROM t WHERE a > 50; DELETE FROM s WHERE a <= 70; PRAGMA wal_checkpoint";

/// Returns a simulator with the campaign's device model.
///
/// @param seed - what the device model's randomness starts from
fn simulator(seed: u64) -> Arc<SimVfs> {
    Arc::new(SimVfs::new(SimConfig {
        seed,
        model: MediaModel::default(),
        ..SimConfig::default()
    }))
}

/// Runs a script, reporting rather than panicking.
///
/// @param engine - the open connection
/// @param script - the statements, separated by semicolons
fn script(engine: &mut ImportedDatabase, script: &str) -> Result<(), inillucent_base::DbError> {
    for statement in script.split(';') {
        if statement.trim().is_empty() {
            continue;
        }
        engine.execute_any(statement, &Params::new())?;
    }
    Ok(())
}

/// Returns the one integer a query answers with.
///
/// @param engine - the open connection
/// @param sql - a query returning one row of one integer
fn integer(engine: &mut ImportedDatabase, sql: &str) -> i64 {
    let outcome = engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|why| panic!("{sql}: {} {:?}", why.message(), why.detail()));
    match outcome.rows.first().and_then(|row| row.first()) {
        Some(inillucent_tree::datum::OwnedDatum::Int(value)) => *value,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Builds the schema in a simulator and closes the connection.
///
/// @param seed - the simulator's seed
/// @param path - where the database lives inside it
fn built(seed: u64, path: &std::path::Path) -> Arc<SimVfs> {
    let vfs = simulator(seed);
    let mut engine = ImportedDatabase::create_on(
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        path.to_path_buf(),
        PAGE_SIZE,
        FRAMES,
    )
    .expect("the database is created in the simulator");
    script(&mut engine, "PRAGMA journal_mode=wal").expect("the journal mode applies");
    script(&mut engine, SCHEMA).expect("the schema builds");
    drop(engine);
    vfs
}

/// Opens the built database for the workload.
///
/// @param vfs - the simulator
/// @param path - the database file
fn reopened(vfs: &Arc<SimVfs>, path: &std::path::Path) -> ImportedDatabase {
    let mut engine = ImportedDatabase::open_on(
        Arc::clone(vfs) as Arc<dyn Vfs>,
        path.to_path_buf(),
        PAGE_SIZE,
        FRAMES,
    )
    .expect("the database reopens");
    script(&mut engine, "PRAGMA journal_mode=wal").expect("the journal mode applies");
    engine
}

/// Opens what a crash left behind, or reports why it would not open.
///
/// @param snapshot - the file as the device had it at the cut
/// @param seed - what the recovering simulator's randomness starts from
/// @param path - the database file
fn recovered_from(
    snapshot: &CrashSnapshot,
    seed: u64,
    path: &std::path::Path,
) -> Result<ImportedDatabase, inillucent_base::DbError> {
    let vfs = Arc::new(SimVfs::recovered(
        SimConfig {
            seed,
            model: MediaModel::default(),
            ..SimConfig::default()
        },
        snapshot,
    ));
    ImportedDatabase::open_on(vfs as Arc<dyn Vfs>, path.to_path_buf(), PAGE_SIZE, FRAMES)
}

/// How many syncs the workload makes on a clean run.
///
/// @param path - the database file
fn syncs_in_the_workload(path: &std::path::Path) -> u64 {
    let vfs = built(71_001, path);
    let mut engine = reopened(&vfs, path);
    let count = |vfs: &Arc<SimVfs>| {
        vfs.failpoints()
            .counts()
            .get(&Site::Sync)
            .copied()
            .unwrap_or(0)
    };
    let base = count(&vfs);
    script(&mut engine, WORKLOAD).expect("the workload commits");
    count(&vfs) - base
}

/// Grades a recovered database: the rows are one of the two committed states,
/// the other tables are whole, and `integrity_check` answers `ok`.
///
/// @param recovered - the database that came back
/// @param label - which sync and seed this is, for the message
fn grade(recovered: &mut ImportedDatabase, label: &str) {
    let rows = integer(recovered, "SELECT count(*) FROM t");
    assert!(rows == 500 || rows == 50, "{label}: {rows} rows in t");
    assert_eq!(
        integer(recovered, "SELECT count(*) FROM v"),
        4,
        "{label}: rows of v"
    );
    let shared = integer(recovered, "SELECT count(*) FROM s");
    assert!(shared == 100 || shared == 30, "{label}: {shared} rows in s");
    let check = recovered
        .execute_any("PRAGMA integrity_check", &Params::new())
        .unwrap_or_else(|why| panic!("{label}: the check failed: {}", why.message()))
        .rows;
    assert_eq!(
        check.len(),
        1,
        "{label}: integrity_check answered {check:?}"
    );
}

/// Power is lost at every sync of a `DELETE` followed by `PRAGMA wal_checkpoint`,
/// under many device seeds, and every file that comes back opens and is whole.
///
/// **This is the case where the new meta record and a page share one sync.** The
/// record's recovery point is past the log's images of the pages, so a page torn
/// in that sync has nothing left to repair it.
#[test]
fn a_crash_at_any_sync_of_an_ordinary_checkpoint_leaves_every_page_whole() {
    let path = std::path::PathBuf::from("/sim/checkpoint_sync.db");
    let total = syncs_in_the_workload(&path);
    assert!(total >= 3, "the workload made {total} syncs");
    let mut failures = Vec::new();
    for nth in 1..=total {
        for seed in 0..SEEDS {
            let vfs = built(71_100 + nth * 1_000 + seed, &path);
            let mut engine = reopened(&vfs, &path);
            let base = vfs
                .failpoints()
                .counts()
                .get(&Site::Sync)
                .copied()
                .unwrap_or(0);
            vfs.failpoints()
                .set(Site::Sync, Policy::Nth(base + nth, Failure::Crash));
            let _ = script(&mut engine, WORKLOAD);
            let snapshot = vfs.crash();
            drop(engine);
            let label = format!("sync {nth} seed {seed}");
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut recovered = recovered_from(&snapshot, 72_000 + seed, &path)
                    .unwrap_or_else(|why| panic!("{label}: did not open: {}", why.message()));
                grade(&mut recovered, &label);
            }));
            if let Err(cause) = outcome {
                let text = cause
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| cause.downcast_ref::<&str>().map(|text| text.to_string()))
                    .unwrap_or_default();
                failures.push(text);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cuts left a damaged file; first: {:?}",
        failures.len(),
        total * SEEDS,
        failures.first()
    );
}

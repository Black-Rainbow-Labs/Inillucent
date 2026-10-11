//! Power loss while a commit gives pages back at the end of the file.
//!
//! Invariant: **every state a crash can leave a commit that moves pages in, and
//! then lowers the page count and cuts the file, reads back as the database
//! before the commit or the database after it, and a file that comes back is
//! never shorter than its header says.** The commit moves live pages to free
//! pages below the end, rewrites each field that named a moved page, records the
//! lower page count and, in a later step, cuts the file. Each of those has to be
//! either wholly there or wholly absent after recovery.
//!
//! ## What the schema holds
//!
//! Every page kind the step can move is in the file: a table three levels deep,
//! chains of pages for 20 KB values, shared pages for values of a few hundred
//! bytes, and an index. The page size is 1,024 bytes so that the tree is three
//! levels deep after five hundred rows and one cut costs a fraction of a second.
//!
//! The cuts are the method of `crashcampaign.rs`: number every injectable call of
//! one run, repeat the run once per number with the failure armed at that call,
//! and grade what recovery gives back.

use std::sync::Arc;

use inillucent_compat::crashcampaign::{record, Campaign};
use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::failpoint::{Failure, Policy, Site};
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{CrashSnapshot, SimConfig, SimVfs};
use inillucent_vfs::{DbPath, OpenOptions, Vfs};

/// The page size every case builds at.
const PAGE_SIZE: usize = 1_024;

/// How many frames the pool holds.
const FRAMES: usize = 4_096;

/// How many cut points each campaign tries. A campaign stops at the last call a
/// run makes, so this is a bound and not a target.
const CUTS: u64 = 400;

/// The database every run starts from, in FULL mode.
const SCHEMA: &str = "PRAGMA auto_vacuum = FULL;
     CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
     CREATE INDEX tb ON t(b);
     CREATE TABLE v(a INTEGER PRIMARY KEY, b TEXT);
     CREATE TABLE s(a INTEGER PRIMARY KEY, b TEXT);
     INSERT INTO t SELECT value, printf('%06d', value) || printf('%.*c', 100, 'x')
       FROM generate_series(1, 500);
     INSERT INTO v SELECT value, hex(randomblob(10000)) FROM generate_series(1, 4);
     INSERT INTO s SELECT value, hex(randomblob(200)) FROM generate_series(1, 100);";

/// The same, in INCREMENTAL mode.
const INCREMENTAL_SCHEMA: &str = "PRAGMA auto_vacuum = INCREMENTAL;
     CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
     CREATE INDEX tb ON t(b);
     CREATE TABLE v(a INTEGER PRIMARY KEY, b TEXT);
     CREATE TABLE s(a INTEGER PRIMARY KEY, b TEXT);
     INSERT INTO t SELECT value, printf('%06d', value) || printf('%.*c', 100, 'x')
       FROM generate_series(1, 500);
     INSERT INTO v SELECT value, hex(randomblob(10000)) FROM generate_series(1, 4);
     INSERT INTO s SELECT value, hex(randomblob(200)) FROM generate_series(1, 100);";

/// What a run is graded on: the rows, an index answer, the integrity check, and
/// the two numbers the step changes.
const PROBES: &[&str] = &[
    "SELECT count(*), sum(a), min(a), max(a) FROM t",
    "SELECT a FROM t INDEXED BY tb WHERE b >= '000040' AND b < '000045' ORDER BY b",
    "SELECT count(*), sum(length(b)) FROM v",
    "SELECT count(*), sum(length(b)) FROM s",
    "PRAGMA integrity_check",
    "PRAGMA freelist_count",
    "PRAGMA page_count",
];

/// The statements after the workload, so a cut can land past its commit.
const TAIL: &str = "SELECT count(*) FROM t; PRAGMA wal_checkpoint;";

/// One transaction that frees pages in every table.
const FREEING: &str = "BEGIN; DELETE FROM t WHERE a > 50; DELETE FROM v WHERE a <= 3; \
                       DELETE FROM s WHERE a <= 70; COMMIT;";

/// Power loss at any call of a FULL mode commit that frees and moves pages, under
/// a write ahead log.
#[test]
fn a_crash_at_any_call_during_a_full_commit_loses_nothing() {
    let report = Campaign {
        name: "auto-vacuum-full-wal",
        mode: "wal",
        schema: SCHEMA,
        workload: FREEING,
        tail: TAIL,
        probes: PROBES,
        failure: Failure::Crash,
        cuts: CUTS,
    }
    .run_at(PAGE_SIZE);
    record("auto-vacuum-full-wal", &report);
}

/// The same under a rollback journal.
#[test]
fn a_crash_at_any_call_during_a_full_commit_under_a_journal_loses_nothing() {
    let report = Campaign {
        name: "auto-vacuum-full-journal",
        mode: "delete",
        schema: SCHEMA,
        workload: FREEING,
        tail: TAIL,
        probes: PROBES,
        failure: Failure::Crash,
        cuts: CUTS,
    }
    .run_at(PAGE_SIZE);
    record("auto-vacuum-full-journal", &report);
}

/// Power loss at any call of a commit that deletes and then asks for 20 pages
/// back: the page count is unchanged or lower by the pages asked for.
#[test]
fn a_crash_at_any_call_during_incremental_vacuum_loses_nothing() {
    let report = Campaign {
        name: "auto-vacuum-incremental",
        mode: "wal",
        schema: INCREMENTAL_SCHEMA,
        workload: "BEGIN; DELETE FROM t WHERE a > 50; DELETE FROM v WHERE a <= 3; \
                   PRAGMA incremental_vacuum(20); COMMIT;",
        tail: TAIL,
        probes: PROBES,
        failure: Failure::Crash,
        cuts: CUTS,
    }
    .run_at(PAGE_SIZE);
    record("auto-vacuum-incremental", &report);
}

/// A device that reports the disk full at any call of a commit that moves pages
/// either keeps the old layout or has the new one, never a mixture.
#[test]
fn a_disk_full_during_a_move_fails_the_commit_and_keeps_the_old_layout() {
    let report = Campaign {
        name: "auto-vacuum-disk-full",
        mode: "wal",
        schema: SCHEMA,
        workload: FREEING,
        tail: TAIL,
        probes: PROBES,
        failure: Failure::DiskFull,
        cuts: CUTS,
    }
    .run_at(PAGE_SIZE);
    record("auto-vacuum-disk-full", &report);
}

/// Power loss at any call of a `DROP TABLE` that moves the highest root into the
/// page the dropped root leaves: the catalog and the tree stay in step.
#[test]
fn a_crash_during_a_root_move_at_drop_table_keeps_the_catalog_and_the_tree_in_step() {
    let report = Campaign {
        name: "auto-vacuum-drop-root",
        mode: "wal",
        schema: "PRAGMA auto_vacuum = FULL;
             CREATE TABLE a1(a INTEGER PRIMARY KEY, b TEXT);
             CREATE TABLE a2(a INTEGER PRIMARY KEY, b TEXT);
             CREATE TABLE a3(a INTEGER PRIMARY KEY, b TEXT);
             INSERT INTO a1 SELECT value, 'one' FROM generate_series(1, 400);
             INSERT INTO a2 SELECT value, 'two' FROM generate_series(1, 400);
             INSERT INTO a3 SELECT value, 'three' FROM generate_series(1, 400);",
        workload: "DROP TABLE a2;",
        tail: "SELECT count(*) FROM a3; PRAGMA wal_checkpoint;",
        probes: &[
            "SELECT name FROM sqlite_schema ORDER BY name",
            "SELECT count(*), min(b) FROM a3",
            "SELECT count(*), min(b) FROM a1",
            "PRAGMA integrity_check",
            "PRAGMA page_count",
        ],
        failure: Failure::Crash,
        cuts: CUTS,
    }
    .run_at(PAGE_SIZE);
    record("auto-vacuum-drop-root", &report);
}

/// Power loss at any call of a `CREATE TABLE` whose root slot is taken by a leaf
/// of another table: the occupant's rows read after every cut.
#[test]
fn a_crash_while_a_root_slot_is_cleared_for_a_new_table() {
    let report = Campaign {
        name: "auto-vacuum-new-root",
        mode: "wal",
        schema: "PRAGMA auto_vacuum = FULL;
             CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
             INSERT INTO t SELECT value, printf('%.*c', 100, 'x') FROM generate_series(1, 300);",
        workload: "BEGIN; CREATE TABLE u(a INTEGER PRIMARY KEY, b TEXT);
                   INSERT INTO u VALUES (1, 'first'); COMMIT;",
        tail: "SELECT count(*) FROM t; PRAGMA wal_checkpoint;",
        probes: &[
            "SELECT name FROM sqlite_schema ORDER BY name",
            "SELECT count(*), sum(a) FROM t",
            "PRAGMA integrity_check",
        ],
        failure: Failure::Crash,
        cuts: CUTS,
    }
    .run_at(PAGE_SIZE);
    record("auto-vacuum-new-root", &report);
}

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

/// Builds the schema in a simulator.
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

/// Opens the schema's database in a simulator for a workload.
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

/// Opens what a crash left behind.
///
/// @param snapshot - the file as the device had it at the cut
/// @param seed - what the recovering simulator's randomness starts from
/// @param path - the database file
fn recovered_from(
    snapshot: &CrashSnapshot,
    seed: u64,
    path: &std::path::Path,
) -> (Arc<SimVfs>, ImportedDatabase) {
    let vfs = Arc::new(SimVfs::recovered(
        SimConfig {
            seed,
            model: MediaModel::default(),
            ..SimConfig::default()
        },
        snapshot,
    ));
    let engine = ImportedDatabase::open_on(
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        path.to_path_buf(),
        PAGE_SIZE,
        FRAMES,
    )
    .unwrap_or_else(|error| panic!("the file opens after the crash (seed {seed}): {error:?}"));
    (vfs, engine)
}

/// Returns the length of the database file inside a simulator.
///
/// @param vfs - the simulator
/// @param path - the database file
fn length_of(vfs: &Arc<SimVfs>, path: &std::path::Path) -> u64 {
    let db_path = DbPath::new(path.to_string_lossy().as_ref());
    let file = vfs
        .open(&db_path, OpenOptions::main_db())
        .expect("the file opens");
    file.file_size().expect("the file has a length")
}

/// Power is lost every time the file is made shorter, and the file that comes back
/// opens, reads, and is no longer than its own header says.
///
/// **The cut is the last step of the commit**, after the checkpoint that carries the
/// lower page count is durable, so what recovery finds is a file longer than the
/// pages it counts. The open trims it; it is never shorter than its header.
#[test]
fn a_crash_inside_the_cut_leaves_a_file_no_longer_than_its_header() {
    let path = std::path::PathBuf::from("/sim/auto_vacuum_cut.db");
    let vfs = built(61_001, &path);
    vfs.failpoints()
        .set(Site::Truncate, Policy::Always(Failure::Crash));
    let mut engine = reopened(&vfs, &path);
    let _ = script(&mut engine, "DELETE FROM t WHERE a > 50");
    let truncations = vfs
        .failpoints()
        .counts()
        .get(&Site::Truncate)
        .copied()
        .unwrap_or(0);
    assert!(
        truncations > 0,
        "the cut was never reached, so this test cut nothing"
    );
    let snapshot = vfs.crash();
    drop(engine);

    let (recovered_vfs, mut recovered) = recovered_from(&snapshot, 61_002, &path);
    let pages = integer(&mut recovered, "PRAGMA page_count") as u64;
    assert!(length_of(&recovered_vfs, &path) <= pages * PAGE_SIZE as u64 + PAGE_SIZE as u64);
    assert_eq!(integer(&mut recovered, "SELECT count(*) FROM v"), 4);
    let rows = integer(&mut recovered, "SELECT count(*) FROM t");
    assert!(rows == 500 || rows == 50, "{rows} rows");
    let check = recovered
        .execute_any("PRAGMA integrity_check", &Params::new())
        .expect("the check runs")
        .rows;
    assert_eq!(check.len(), 1);
}

/// Power is lost at every sync a FULL commit makes, and recovery replays every
/// move: the rows read through their new parents, the page count is the lower one,
/// and the file is no longer than its header.
///
/// **The window between the commit and the checkpoint is the one that needs the
/// replay of `WritePage`, `Relink` and `PageCount`.** A crash at one of the syncs
/// before the commit record leaves the old state; from the first sync after it,
/// every crash leaves the new one, and the first of those happens before a single
/// data page has reached the file.
#[test]
fn a_crash_between_the_commit_and_the_checkpoint_replays_every_move() {
    let path = std::path::PathBuf::from("/sim/auto_vacuum_replay.db");
    let total = {
        let vfs = built(62_001, &path);
        let mut engine = reopened(&vfs, &path);
        let base = vfs
            .failpoints()
            .counts()
            .get(&Site::Sync)
            .copied()
            .unwrap_or(0);
        script(&mut engine, "DELETE FROM t WHERE a > 50").expect("the delete commits");
        vfs.failpoints()
            .counts()
            .get(&Site::Sync)
            .copied()
            .unwrap_or(0)
            - base
    };
    assert!(total >= 3, "the delete made {total} syncs");
    let mut verdicts = Vec::new();
    for nth in 1..=total {
        let vfs = built(62_100 + nth, &path);
        let mut engine = reopened(&vfs, &path);
        let base = vfs
            .failpoints()
            .counts()
            .get(&Site::Sync)
            .copied()
            .unwrap_or(0);
        vfs.failpoints()
            .set(Site::Sync, Policy::Nth(base + nth, Failure::Crash));
        let _ = script(&mut engine, "DELETE FROM t WHERE a > 50");
        let snapshot = vfs.crash();
        drop(engine);
        let (recovered_vfs, mut recovered) = recovered_from(&snapshot, 62_200 + nth, &path);
        let rows = integer(&mut recovered, "SELECT count(*) FROM t");
        let pages = integer(&mut recovered, "PRAGMA page_count") as u64;
        assert!(rows == 500 || rows == 50, "sync {nth}: {rows} rows");
        if rows == 50 {
            assert_eq!(
                integer(&mut recovered, "PRAGMA freelist_count"),
                0,
                "sync {nth}"
            );
            // Replay lowers the page count; the next checkpoint cuts the file to it.
            script(&mut recovered, "PRAGMA wal_checkpoint").expect("the checkpoint runs");
            assert_eq!(
                length_of(&recovered_vfs, &path),
                pages * PAGE_SIZE as u64,
                "sync {nth}"
            );
        }
        assert_eq!(
            integer(&mut recovered, "SELECT count(*) FROM v"),
            4,
            "sync {nth}"
        );
        let check = recovered
            .execute_any("PRAGMA integrity_check", &Params::new())
            .expect("the check runs")
            .rows;
        assert_eq!(check.len(), 1, "sync {nth}");
        verdicts.push(rows);
    }
    let Some(first_new) = verdicts.iter().position(|rows| *rows == 50) else {
        panic!("no crash at a sync left the new state: {verdicts:?}");
    };
    assert!(
        first_new > 0,
        "the first sync already left the new state: {verdicts:?}"
    );
    assert!(
        verdicts[first_new..].iter().all(|rows| *rows == 50),
        "a later sync lost the committed delete: {verdicts:?}"
    );
}

/// An I/O error at a call of a FULL commit, then a clean reopen, and the next FULL
/// commit gives the file back down to no free page.
#[test]
fn an_io_error_inside_a_relink_pins_nothing_permanently() {
    let path = std::path::PathBuf::from("/sim/auto_vacuum_io.db");
    for nth in (1..=150).step_by(5) {
        let vfs = built(63_000 + nth, &path);
        let mut engine = reopened(&vfs, &path);
        let base = vfs.failpoints().sites_reached();
        vfs.failpoints()
            .fail_nth_call(base.saturating_add(nth), Failure::IoError);
        let _ = script(&mut engine, "DELETE FROM t WHERE a > 50");
        let snapshot = vfs.crash();
        drop(engine);
        let (_, mut recovered) = recovered_from(&snapshot, 63_500 + nth, &path);
        script(&mut recovered, "PRAGMA journal_mode=wal").expect("the journal mode applies");
        script(&mut recovered, "DELETE FROM t WHERE a > 50").expect("the next commit runs");
        assert_eq!(
            integer(&mut recovered, "PRAGMA freelist_count"),
            0,
            "error at call {nth}"
        );
        assert_eq!(
            integer(&mut recovered, "SELECT count(*) FROM t"),
            50,
            "error at call {nth}"
        );
        assert_eq!(
            integer(&mut recovered, "SELECT count(*) FROM v"),
            4,
            "error at call {nth}"
        );
    }
}

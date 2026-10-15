//! A writable open beside another handle that holds SHARED.
//!
//! Invariant: **an open that cannot have the file exclusively changes no byte
//! of the database, its log or its journal, and creates, deletes and renames no
//! file.** SHARED is the lock a reader holds across a statement, so a writable
//! open that trimmed the log, folded or gave back the file's tail under SHARED
//! would change the file beneath that reader. The open either skips those writes
//! or refuses with `busy`; once the other handle lets go, the next open does
//! them.
//!
//! The file in these cases has an unfolded log that moves the page count and a
//! torn tail after the last valid record, which are the two things an open
//! writes for: a fold of the replayed pages and a cut of the log.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{CrashSnapshot, SimConfig, SimVfs};
use inillucent_vfs::{DbPath, FileKind, FileLock, OpenOptions, Vfs};

/// The page size every case builds at.
const PAGE_SIZE: usize = 1_024;

/// How many frames the pool holds.
const FRAMES: usize = 4_096;

/// How many log segments a case looks for.
const SEGMENTS: u64 = 12;

/// Where the database lives in the simulator.
const DATABASE: &str = "/sim/beside_a_reader.db";

/// Trace kinds that change a file or the directory around it.
const CHANGES: &[&str] = &["write", "truncate", "delete", "rename", "allocate"];

/// Returns a simulator with the pessimistic device model.
///
/// @param seed - what the device model's randomness starts from
fn simulator(seed: u64) -> Arc<SimVfs> {
    Arc::new(SimVfs::new(SimConfig {
        seed,
        model: MediaModel::default(),
        ..SimConfig::default()
    }))
}

/// Runs one statement and reports its error.
///
/// @param engine - the open connection
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) -> Result<(), inillucent_base::DbError> {
    engine.execute_any(sql, &Params::new()).map(|_| ())
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

/// Opens the database in a simulator.
///
/// @param vfs - the simulator
fn open_on(vfs: &Arc<SimVfs>) -> Result<ImportedDatabase, inillucent_base::DbError> {
    ImportedDatabase::open_on(
        Arc::clone(vfs) as Arc<dyn Vfs>,
        PathBuf::from(DATABASE),
        PAGE_SIZE,
        FRAMES,
    )
}

/// Builds a file whose log holds rows the data file does not, and returns what
/// the device holds when the power goes with the writer still open.
fn crashed_with_an_unfolded_log() -> CrashSnapshot {
    let vfs = simulator(81_001);
    let mut engine = ImportedDatabase::create_on(
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        PathBuf::from(DATABASE),
        PAGE_SIZE,
        FRAMES,
    )
    .expect("the database is created");
    run(&mut engine, "PRAGMA journal_mode=wal").expect("the journal mode applies");
    run(&mut engine, "CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT)").expect("the table");
    drop(engine);
    let mut engine = open_on(&vfs).expect("the database reopens");
    run(&mut engine, "PRAGMA journal_mode=wal").expect("the journal mode applies");
    run(
        &mut engine,
        "INSERT INTO t SELECT value, printf('%06d', value) || printf('%.*c', 200, 'x') \
         FROM generate_series(1, 2000)",
    )
    .expect("the rows commit");
    let snapshot = vfs.crash();
    drop(engine);
    snapshot
}

/// Returns the segment path for a sequence number.
///
/// @param sequence - which segment
fn segment(sequence: u64) -> DbPath {
    inillucent_wal::writer::segment_path(DATABASE, Some(Path::new("/sim")), sequence)
}

/// Appends bytes that are not a record to the last log segment, which is what a
/// writer killed in the middle of an append leaves.
///
/// @param vfs - the simulator holding the crashed file
fn tear_the_log_tail(vfs: &Arc<SimVfs>) {
    let mut last = None;
    for sequence in 1..=SEGMENTS {
        if vfs
            .size_by_name(&segment(sequence))
            .expect("the size is asked")
            .is_some()
        {
            last = Some(sequence);
        }
    }
    let last = last.expect("the crashed file has a log segment");
    let file = vfs
        .open(
            &segment(last),
            OpenOptions::of_kind(FileKind::Wal).existing(),
        )
        .expect("the segment opens");
    let end = file.file_size().expect("the segment has a length");
    file.write_all_at(end, &[0xAB; 300])
        .expect("the tail is written");
    file.sync(inillucent_vfs::SyncMode::Normal)
        .expect("the tail is synced");
}

/// The length of every file an open could write, by name, in a fixed order.
///
/// @param vfs - the simulator
fn lengths(vfs: &Arc<SimVfs>) -> Vec<(String, Option<u64>)> {
    let mut names = vec![
        DATABASE.to_string(),
        format!("{DATABASE}-journal"),
        format!("{DATABASE}-wal"),
    ];
    for sequence in 1..=SEGMENTS {
        names.push(segment(sequence).display().to_string());
    }
    names
        .into_iter()
        .map(|name| {
            let length = vfs.size_by_name(&DbPath::new(&name)).expect("a size");
            (name, length)
        })
        .collect()
}

/// How many events since `from` changed a file.
///
/// @param vfs - the simulator
/// @param from - how many events the trace held before
fn changes_since(vfs: &Arc<SimVfs>, from: usize) -> Vec<String> {
    vfs.trace()
        .events()
        .into_iter()
        .skip(from)
        .filter(|event| CHANGES.contains(&event.kind))
        .map(|event| format!("{} {} at {}", event.kind, event.path, event.offset))
        .collect()
}

/// Returns a simulator holding the crashed file, with a torn log tail or without one.
///
/// @param torn - whether bytes that are not a record follow the log's last record
fn crashed_file(torn: bool) -> Arc<SimVfs> {
    let snapshot = crashed_with_an_unfolded_log();
    let vfs = Arc::new(SimVfs::recovered(
        SimConfig {
            seed: 81_002,
            model: MediaModel::default(),
            ..SimConfig::default()
        },
        &snapshot,
    ));
    if torn {
        tear_the_log_tail(&vfs);
    }
    vfs
}

/// Opens the file beside a handle holding SHARED and grades what the open wrote.
///
/// @param torn - whether the log has a torn tail
fn open_beside_a_shared_holder(torn: bool) {
    let vfs = crashed_file(torn);
    let holder = vfs
        .open(&DbPath::new(DATABASE), OpenOptions::main_db().existing())
        .expect("the holder opens the file");
    holder
        .lock(FileLock::Shared)
        .expect("the holder takes SHARED");
    let before = lengths(&vfs);
    let from = vfs.trace().events().len();
    let opened = open_on(&vfs);
    let written = changes_since(&vfs, from);
    assert!(
        written.is_empty(),
        "an open beside a SHARED holder changed the files: {written:?} (torn tail {torn})"
    );
    assert_eq!(
        before,
        lengths(&vfs),
        "an open beside a SHARED holder changed a file's length (torn tail {torn})"
    );
    let mut beside = opened.expect("an open beside a reader skips its writes and succeeds");
    assert_eq!(integer(&mut beside, "SELECT count(*) FROM t"), 2_000);
    drop(beside);
    holder.unlock(FileLock::None).expect("the holder lets go");
    let mut after = open_on(&vfs).expect("the open after the holder lets go succeeds");
    assert_eq!(integer(&mut after, "SELECT count(*) FROM t"), 2_000);
    let check = after
        .execute_any("PRAGMA integrity_check", &Params::new())
        .expect("the check runs")
        .rows;
    assert_eq!(check.len(), 1, "integrity_check answered {check:?}");
}

/// A writable open beside a handle that holds SHARED writes nothing when the log
/// ends cleanly, and the open after that handle lets go reads every row.
#[test]
fn an_open_beside_a_shared_holder_changes_no_byte() {
    open_beside_a_shared_holder(false);
}

/// The same when the log has a torn tail the open would cut.
#[test]
fn an_open_beside_a_shared_holder_does_not_cut_a_torn_log_tail() {
    open_beside_a_shared_holder(true);
}

/// The same file with nobody holding it is folded by the open, so the case above
/// is graded on an open that has a write to skip.
///
/// **A control for the case above.** Without it, that case would also pass if the
/// file were built so that an open had nothing to write.
#[test]
fn the_same_open_with_no_holder_folds_into_the_data_file() {
    let vfs = crashed_file(false);
    let from = vfs.trace().events().len();
    let opened = open_on(&vfs).expect("the open succeeds");
    let written = changes_since(&vfs, from);
    drop(opened);
    assert!(
        written.iter().any(|change| change.contains(".db at")),
        "the open folded nothing into the data file, so the case above skipped nothing: {written:?}"
    );
}

/// The torn tail is cut by the first writing statement once nothing else holds
/// the file, so the case above skips a cut that does happen later.
///
/// **A control for the torn tail case.**
#[test]
fn the_same_torn_tail_is_cut_when_nothing_holds_the_file() {
    let vfs = crashed_file(true);
    let mut opened = open_on(&vfs).expect("the open succeeds");
    let from = vfs.trace().events().len();
    run(&mut opened, "INSERT INTO t VALUES (100000, 'after')").expect("the insert commits");
    let cut = vfs
        .trace()
        .events()
        .into_iter()
        .skip(from)
        .any(|event| event.kind == "truncate" && event.path.contains("-wal"));
    assert!(
        cut,
        "the writing statement cut no log tail, so the torn tail case skipped nothing"
    );
}

//! Power loss at every write of a transaction whose leaf splits are logged as
//! what they did, and of the fold that follows it, in both journal modes.
//!
//! Invariant: **a transaction that splits leaves under a parent with room, and
//! the fold that writes those pages into the file, recover after a crash at
//! any call to exactly the state before the transaction or exactly the state
//! after it.** Those splits are logged as `Body::SplitLeaf`, which replays the
//! left page and the parent by reading them, so a fold that tears either one
//! has to be repaired by something else first: the after image the fold logs
//! under `journal_mode = wal`, or the pre image the rollback journal keeps
//! under `journal_mode = delete`. This file cuts the fold at every write and
//! every sync in both modes, which is where a record that reads its page meets
//! a torn page.
//!
//! The workload is checked to have logged at least two such records, read off
//! the log itself, so a change that stopped logging splits this way could not
//! leave this file passing on image records alone.
//! `inillucent-model`'s `logical_split_crash.rs` compares the three pages of
//! one split byte for byte; this file grades the rows a connection reads back
//! and `PRAGMA integrity_check`.

use std::path::PathBuf;
use std::sync::Arc;

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::failpoint::Failure;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{CrashSnapshot, SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::Vfs;
use inillucent_wal::record::{kind, Record};

/// The page size: small, so a few hundred rows make a tree of two levels.
const PAGE_SIZE: usize = 4_096;

/// How many frames the pool holds: enough that nothing evicts during a run.
const FRAMES: usize = 4_096;

/// How many rows the table starts with, inserted in key order so the leaves
/// are packed full the way an append packs them.
const SEEDED: i64 = 300;

/// Returns the text every row carries: long enough that a leaf holds about
/// twenty of them.
///
/// @param key - the row's key
fn label(key: i64) -> String {
    format!("{key:>6} {}", "x".repeat(180))
}

/// Returns a simulator with the pessimistic device model.
///
/// @param seed - the run's seed
fn simulator(seed: u64) -> Arc<SimVfs> {
    Arc::new(SimVfs::new(SimConfig {
        seed,
        model: MediaModel::default(),
        ..SimConfig::default()
    }))
}

/// The path every run uses.
fn path() -> PathBuf {
    PathBuf::from("/sim/logical-split.db")
}

/// Reopens a database a prior connection built.
///
/// @param vfs - the file system holding it
fn reopen(vfs: Arc<dyn Vfs>) -> Result<ImportedDatabase, inillucent_base::DbError> {
    ImportedDatabase::open_on(vfs, path(), PAGE_SIZE, FRAMES)
}

/// Runs one statement.
///
/// @param engine - the connection
/// @param sql - the statement
fn exec(engine: &mut ImportedDatabase, sql: &str) -> Result<(), inillucent_base::DbError> {
    engine.execute_any(sql, &Params::new()).map(|_| ())
}

/// Builds the table in a journal mode and returns the simulator holding it.
///
/// @param seed - the run's seed
/// @param mode - the journal mode
fn built(seed: u64, mode: &str) -> Arc<SimVfs> {
    let vfs = simulator(seed);
    let mut engine =
        ImportedDatabase::create_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES)
            .expect("the connection opens");
    exec(&mut engine, &format!("PRAGMA journal_mode={mode}")).expect("the journal mode");
    exec(&mut engine, "PRAGMA synchronous=full").expect("the sync mode");
    exec(&mut engine, "CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT)").expect("the table");
    exec(&mut engine, "BEGIN").expect("a transaction");
    for row in 0..SEEDED {
        let key = row * 10;
        exec(
            &mut engine,
            &format!("INSERT INTO t VALUES({key}, '{}')", label(key)),
        )
        .expect("a seeded row");
    }
    exec(&mut engine, "COMMIT").expect("the seed commits");
    drop(engine);
    vfs
}

/// Runs the workload: forty rows into the middle of the key range, in one
/// transaction, which splits the full leaves they land in.
///
/// @param engine - the connection
fn workload(engine: &mut ImportedDatabase) -> Result<(), inillucent_base::DbError> {
    exec(engine, "BEGIN")?;
    for row in 100..140 {
        let key = row * 10 + 5;
        exec(
            engine,
            &format!("INSERT INTO t VALUES({key}, '{}')", label(key)),
        )?;
    }
    exec(engine, "COMMIT")
}

/// The rows a database holds, in key order.
///
/// @param engine - the connection
fn contents(engine: &mut ImportedDatabase) -> Result<Vec<(i64, usize)>, inillucent_base::DbError> {
    let outcome = engine.execute_any("SELECT a, b FROM t ORDER BY a", &Params::new())?;
    Ok(outcome
        .rows
        .iter()
        .map(|row| {
            let key = match row.first() {
                Some(OwnedDatum::Int(key)) => *key,
                _ => -1,
            };
            let length = match row.get(1) {
                Some(OwnedDatum::Text(bytes)) if *bytes == label(key).into_bytes() => bytes.len(),
                _ => 0,
            };
            (key, length)
        })
        .collect())
}

/// Returns what `PRAGMA integrity_check` says.
///
/// @param engine - the connection
fn integrity(engine: &mut ImportedDatabase) -> Result<String, inillucent_base::DbError> {
    let outcome = engine.execute_any("PRAGMA integrity_check", &Params::new())?;
    Ok(outcome
        .rows
        .iter()
        .filter_map(|row| match row.first() {
            Some(OwnedDatum::Text(bytes)) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("; "))
}

/// Counts the `SplitLeaf` records the connection's log holds.
///
/// @param engine - the connection, with the workload's records not yet folded
/// @param vfs - the simulator the log is on
fn logical_splits(engine: &ImportedDatabase, vfs: &SimVfs) -> usize {
    let wal = engine.wal();
    let mut found = 0usize;
    for sequence in 1..=wal.sequence() {
        let Some(bytes) = vfs.visible_bytes(&wal.segment_path(sequence)) else {
            continue;
        };
        let mut at = inillucent_wal::segment::HEADER_BYTES;
        while let Ok(Some(record)) = Record::decode(bytes.get(at..).unwrap_or(&[])) {
            if record.body.kind() == kind::SPLIT_LOGICAL {
                found += 1;
            }
            at += record.length;
        }
    }
    found
}

/// The two states a run may end in.
///
/// Also fails unless the workload logged at least two logical splits, read
/// off the log before anything folds it.
///
/// @param mode - the journal mode
fn expected_states(mode: &str) -> (Vec<(i64, usize)>, Vec<(i64, usize)>) {
    let vfs = built(4242, mode);
    let mut engine = reopen(Arc::clone(&vfs) as Arc<dyn Vfs>).expect("the connection opens");
    let before = contents(&mut engine).expect("the rows read");
    workload(&mut engine).expect("the workload commits");
    let after = contents(&mut engine).expect("the rows read");
    let splits = logical_splits(&engine, &vfs);
    assert!(
        splits >= 2,
        "{mode}: the workload logged {splits} logical splits, so this campaign would grade \
         image records only"
    );
    (before, after)
}

/// Reopens what a crash left and reads it back.
///
/// @param snapshot - the media
/// @param seed - the recovery's seed
fn recovered(
    snapshot: &CrashSnapshot,
    seed: u64,
) -> Result<(Vec<(i64, usize)>, String), inillucent_base::DbError> {
    let vfs = Arc::new(SimVfs::recovered(
        SimConfig {
            seed,
            model: MediaModel::default(),
            ..SimConfig::default()
        },
        snapshot,
    ));
    let mut engine = reopen(Arc::clone(&vfs) as Arc<dyn Vfs>)?;
    Ok((contents(&mut engine)?, integrity(&mut engine)?))
}

/// Runs the campaign in one journal mode and returns how many cuts left the
/// old state and how many the new one.
///
/// @param mode - the journal mode
fn campaign(mode: &str) -> (u64, u64) {
    let (before, after) = expected_states(mode);
    assert_eq!(
        after.len(),
        before.len() + 40,
        "{mode}: the workload added nothing"
    );
    let mut old = 0u64;
    let mut new = 0u64;
    let mut cuts = 0u64;
    let mut every_call = false;
    for nth in 1..=600u64 {
        let vfs = built(7000 + nth, mode);
        let base = vfs.failpoints().sites_reached();
        vfs.failpoints()
            .fail_nth_call(base.saturating_add(nth), Failure::Crash);
        let mut connection = reopen(Arc::clone(&vfs) as Arc<dyn Vfs>);
        let committed = match &mut connection {
            Ok(engine) => {
                let committed = workload(engine).is_ok();
                let _ = exec(engine, "PRAGMA wal_checkpoint(TRUNCATE)");
                committed
            }
            Err(_) => false,
        };
        let reached = vfs.failpoints().sites_reached().saturating_sub(base);
        let snapshot = vfs.crash();
        drop(connection);
        if reached < nth {
            every_call = true;
            break;
        }
        cuts += 1;
        let (rows, check) = recovered(&snapshot, 8000 + nth).unwrap_or_else(|error| {
            panic!(
                "{mode} cut {nth}: the database came back unreadable: {error}: {:?}",
                error.detail()
            )
        });
        assert_eq!(
            check, "ok",
            "{mode} cut {nth}: integrity_check said {check}"
        );
        if rows == after {
            new += 1;
        } else if rows == before {
            assert!(
                !committed,
                "{mode} cut {nth}: the commit reported success and the database does not hold it"
            );
            old += 1;
        } else {
            panic!(
                "{mode} cut {nth}: neither state: {} rows, before {} and after {}",
                rows.len(),
                before.len(),
                after.len()
            );
        }
    }
    // **Every call, not the first six hundred.** The fold comes last, so a
    // sweep that stopped early would have cut the commit and never the fold.
    assert!(
        every_call,
        "{mode}: the run makes more than 600 calls, so the fold was not reached"
    );
    assert!(cuts > 40, "{mode}: only {cuts} cut points were reached");
    assert!(old > 0, "{mode}: no cut left the old state");
    assert!(new > 0, "{mode}: no cut left the new state");
    (old, new)
}

/// Every cut of a splitting transaction and its fold, under the write ahead
/// log, recovers to one state or the other.
#[test]
fn every_cut_under_the_write_ahead_log_is_recoverable() {
    campaign("wal");
}

/// The same under the rollback journal.
#[test]
fn every_cut_under_the_rollback_journal_is_recoverable() {
    campaign("delete");
}

//! Crash campaigns over the encrypting file system.
//!
//! Invariant: **an encrypted database keeps every durability promise a
//! plaintext one makes**, on the same pessimistic disk model: a crash at any
//! cut point of a commit leaves the old state or the new one, and a crash at
//! any cut point of a checkpoint after an acknowledged commit leaves the new
//! one.
//!
//! This is the test the encrypted file layout was designed against. An AEAD
//! unit rewrites all its bytes whenever any byte of it changes, so a torn
//! rewrite damages bytes the engine never asked to change - the log appends
//! records into units an earlier synced record ends in, and the rollback
//! journal rewrites its header over the unit its first record starts in. The
//! double slot layout is what keeps the synced copy readable. Every call the
//! encrypting layer makes to the simulator is a cut point here, so the
//! campaign reaches the cut between a unit's two slot writes as well as every
//! cut the plaintext campaign reaches.
//!
//! `durability.rs` runs the same shape of campaign in plaintext; the two files
//! are kept apart because this one wraps every simulator in a `CryptVfs`, and
//! threading an optional wrapper through that file's thirty helpers would make
//! both harder to read.

use std::sync::Arc;

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::failpoint::Failure;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{CrashSnapshot, SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::crypt::{probe, Probe};
use inillucent_vfs::path::DbPath;
use inillucent_vfs::{CryptVfs, EncryptionKey, Vfs};

/// The page size these tests build at, and the frames the pool holds.
const PAGE_SIZE: usize = 4_096;
const FRAMES: usize = 8_192;

/// The database every run builds.
const SCHEMA: [&str; 3] = [
    "CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT, c INTEGER)",
    "CREATE INDEX t_b ON t(b)",
    "INSERT INTO t VALUES(1, 'one', 10), (2, 'two', 20), (3, 'three', 30)",
];

/// The transaction each run tries to commit on top of it.
const WORKLOAD: [&str; 5] = [
    "BEGIN",
    "INSERT INTO t VALUES(4, 'four', 40)",
    "UPDATE t SET c = c + 1 WHERE a <= 2",
    "DELETE FROM t WHERE a = 3",
    "COMMIT",
];

/// The journal mode and durability level a campaign runs under.
#[derive(Clone, Copy, Debug)]
struct Journal {
    /// `PRAGMA journal_mode`.
    mode: &'static str,
    /// `PRAGMA synchronous`.
    synchronous: &'static str,
}

/// The file every run uses, as the simulator names it.
fn path() -> std::path::PathBuf {
    std::path::PathBuf::from("vault.db")
}

/// A simulator with the pessimistic device model.
///
/// @param seed - the device model's seed
fn simulator(seed: u64) -> Arc<SimVfs> {
    Arc::new(SimVfs::new(SimConfig {
        seed,
        model: MediaModel::default(),
        ..SimConfig::default()
    }))
}

/// Wraps a simulator in the encrypting file system, with the key every run
/// shares. A fresh wrapper per open, as a new process would make.
///
/// @param sim - the simulator
fn encrypted(sim: &Arc<SimVfs>) -> Arc<dyn Vfs> {
    let key = EncryptionKey::raw([0x42; 32]);
    let crypt = CryptVfs::new(Arc::clone(sim) as Arc<dyn Vfs>, key)
        .expect("the encrypting file system is made")
        .with_page_size(PAGE_SIZE as u32);
    Arc::new(crypt)
}

/// Runs every statement of a script, stopping at the first failure.
///
/// @param engine - the database
/// @param script - the statements
fn run_script(
    engine: &mut ImportedDatabase,
    script: &[&str],
) -> Result<(), inillucent_base::DbError> {
    for statement in script {
        engine.execute_any(statement, &Params::new())?;
    }
    Ok(())
}

/// Opens the database with the campaign's journal settings.
///
/// @param vfs - the encrypting file system
/// @param journal - the settings
fn open(vfs: Arc<dyn Vfs>, journal: Journal) -> Result<ImportedDatabase, inillucent_base::DbError> {
    let mut engine = ImportedDatabase::open_on(vfs, path(), PAGE_SIZE, FRAMES)?;
    run_script(
        &mut engine,
        &[
            &format!("PRAGMA journal_mode = {}", journal.mode),
            &format!("PRAGMA synchronous = {}", journal.synchronous),
        ],
    )?;
    Ok(engine)
}

/// Reads the rows, reporting a failure rather than panicking.
///
/// @param engine - the database
fn try_contents(engine: &mut ImportedDatabase) -> Result<Vec<String>, inillucent_base::DbError> {
    let outcome = engine.execute_any("SELECT a, b, c FROM t ORDER BY a", &Params::new())?;
    Ok(outcome
        .rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| match value {
                    OwnedDatum::Int(n) => n.to_string(),
                    OwnedDatum::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect())
}

/// Builds the schema on a fresh simulator and returns it.
///
/// @param journal - the settings
/// @param seed - the device model's seed
fn built(journal: Journal, seed: u64) -> Arc<SimVfs> {
    let sim = simulator(seed);
    let mut engine = ImportedDatabase::create_on(encrypted(&sim), path(), PAGE_SIZE, FRAMES)
        .expect("the database is created");
    run_script(
        &mut engine,
        &[
            &format!("PRAGMA journal_mode = {}", journal.mode),
            &format!("PRAGMA synchronous = {}", journal.synchronous),
        ],
    )
    .expect("the settings apply");
    run_script(&mut engine, &SCHEMA).expect("the schema builds");
    drop(engine);
    sim
}

/// The rows before and after the workload, with no failure.
///
/// @param journal - the settings
fn expected_states(journal: Journal) -> (Vec<String>, Vec<String>) {
    let sim = built(journal, 7);
    let mut engine = open(encrypted(&sim), journal).expect("it reopens");
    let before = try_contents(&mut engine).expect("reads");
    run_script(&mut engine, &WORKLOAD).expect("the workload commits");
    let after = try_contents(&mut engine).expect("reads");
    (before, after)
}

/// What reopening a crashed database produced.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Recovery {
    /// It opened and held these rows.
    Rows(Vec<String>),
    /// It refused to be read, naming the damage.
    Corrupt(String),
}

/// Reopens what a crash left, through a fresh encrypting file system.
///
/// @param journal - the settings
/// @param snapshot - the media after the power loss
/// @param seed - the device model's seed
fn recovered(journal: Journal, snapshot: &CrashSnapshot, seed: u64) -> Recovery {
    let sim = Arc::new(SimVfs::recovered(
        SimConfig {
            seed,
            model: MediaModel::default(),
            ..SimConfig::default()
        },
        snapshot,
    ));
    match open(encrypted(&sim), journal).and_then(|mut engine| try_contents(&mut engine)) {
        Ok(rows) => Recovery::Rows(rows),
        Err(failure) => Recovery::Corrupt(format!("{failure} / {:?}", failure.detail())),
    }
}

/// Runs the commit campaign: a failure at every call of the workload.
///
/// @param journal - the settings
/// @param failure - what the armed call does
/// @param limit - the most cut points to try
fn commit_campaign(journal: Journal, failure: Failure, limit: u64) -> u64 {
    let (before, after) = expected_states(journal);
    assert_ne!(before, after);
    let mut cut_points = 0u64;
    for nth in 1..=limit {
        let sim = built(journal, 1_786 + nth);
        let base = sim.failpoints().sites_reached();
        sim.failpoints().fail_nth_call(base + nth, failure);
        let committed = match open(encrypted(&sim), journal) {
            Ok(mut engine) => run_script(&mut engine, &WORKLOAD).is_ok(),
            Err(_) => false,
        };
        let reached = sim.failpoints().sites_reached() - base;
        let snapshot = sim.crash();
        let recovery = recovered(journal, &snapshot, 4_242 + nth);
        if reached < nth {
            assert!(committed, "an unarmed run must commit");
            assert_eq!(
                recovery,
                Recovery::Rows(after.clone()),
                "an acknowledged commit was lost"
            );
            break;
        }
        cut_points += 1;
        match (&recovery, committed) {
            (Recovery::Rows(rows), true) => assert_eq!(
                *rows, after,
                "{journal:?} call {nth}: an acknowledged commit came back as {rows:?}"
            ),
            (Recovery::Rows(rows), false) => assert!(
                *rows == before || *rows == after,
                "{journal:?} call {nth}: recovery produced neither state: {rows:?}"
            ),
            (Recovery::Corrupt(detail), _) => {
                panic!("{journal:?} call {nth}: the database could not be read: {detail}")
            }
        }
    }
    assert!(
        cut_points >= 10,
        "a campaign of {cut_points} cut points is not a campaign"
    );
    cut_points
}

/// Runs the checkpoint campaign: the workload commits, then a failure at
/// every call of the checkpoint after it.
///
/// @param journal - the settings
/// @param limit - the most cut points to try
fn checkpoint_campaign(journal: Journal, limit: u64) -> u64 {
    let (_, after) = expected_states(journal);
    let mut cut_points = 0u64;
    for nth in 1..=limit {
        let sim = built(journal, 5_100 + nth);
        let mut engine = open(encrypted(&sim), journal).expect("it opens");
        run_script(&mut engine, &WORKLOAD).expect("the workload commits");
        let base = sim.failpoints().sites_reached();
        sim.failpoints().fail_nth_call(base + nth, Failure::Crash);
        let _ = engine.execute_any("PRAGMA wal_checkpoint", &Params::new());
        let reached = sim.failpoints().sites_reached() - base;
        let snapshot = sim.crash();
        drop(engine);
        let recovery = recovered(journal, &snapshot, 7_400 + nth);
        assert_eq!(
            recovery,
            Recovery::Rows(after.clone()),
            "{journal:?} checkpoint call {nth}: the acknowledged commit was lost"
        );
        if reached < nth {
            break;
        }
        cut_points += 1;
    }
    assert!(
        cut_points >= 10,
        "a campaign of {cut_points} cut points is not a campaign"
    );
    cut_points
}

/// A power loss at every cut point of a `delete` mode commit.
#[test]
fn power_loss_at_every_cut_point_of_an_encrypted_delete_commit() {
    let journal = Journal {
        mode: "delete",
        synchronous: "full",
    };
    let cuts = commit_campaign(journal, Failure::Crash, 1_500);
    eprintln!("delete commit: {cuts} cut points");
}

/// A power loss at every cut point of a `wal` mode commit.
#[test]
fn power_loss_at_every_cut_point_of_an_encrypted_wal_commit() {
    let journal = Journal {
        mode: "wal",
        synchronous: "full",
    };
    let cuts = commit_campaign(journal, Failure::Crash, 1_500);
    eprintln!("wal commit: {cuts} cut points");
}

/// An I/O error at every cut point of a commit is recoverable.
#[test]
fn an_io_error_at_every_cut_point_of_an_encrypted_commit_is_recoverable() {
    let journal = Journal {
        mode: "delete",
        synchronous: "full",
    };
    let cuts = commit_campaign(journal, Failure::IoError, 1_500);
    eprintln!("io error commit: {cuts} cut points");
}

/// A power loss at every cut point of a checkpoint keeps the commit before it,
/// in both journal modes.
#[test]
fn power_loss_at_every_cut_point_of_an_encrypted_checkpoint() {
    for mode in ["delete", "wal"] {
        let cuts = checkpoint_campaign(
            Journal {
                mode,
                synchronous: "full",
            },
            1_500,
        );
        eprintln!("{mode} checkpoint: {cuts} cut points");
    }
}

/// The campaign is over an encrypted file: the simulator holds a header and
/// no plaintext row.
#[test]
fn the_campaign_file_is_encrypted() {
    let journal = Journal {
        mode: "delete",
        synchronous: "full",
    };
    let sim = built(journal, 11);
    assert_eq!(
        probe(
            sim.as_ref(),
            &DbPath::new(path().to_string_lossy().as_ref())
        )
        .expect("probes"),
        Probe::Encrypted
    );
}

//! Several writer and reader processes on one file, killed at random, and the
//! file graded by a process that wrote none of it.
//!
//! Invariant: **however the operating system ends the processes, every
//! transaction a worker acknowledged is in the file whole, a reader never sees
//! half a transaction, a rolled back row or a value that does not match its
//! digest, and the file passes its integrity check.** The workload and the
//! checks are `inillucent_compat::chaos`; the run is `inillucent_compat::storm`.
//!
//! ## Why a storm and not another sequence
//!
//! Every defect users reported this sprint was a sequence of processes nobody
//! had written a test for. A storm writes the sequences itself: four workers
//! and two readers under `locking_mode = normal`, a kill every 50 to 450 ms,
//! transactions that spill, roll back, checkpoint and free pages. Its first
//! minute on 2.0.7 found four defects that thirty durability suites had not:
//! a checkpoint called between statements that undid another process's commit,
//! an open that failed with `busy` beside any writer, an open that failed with
//! `every frame in the buffer pool is pinned` beside a writer, and a `BEGIN`
//! that blocked every reader. `process_interleavings.rs` holds the smallest
//! sequence for each, so a failure here that matches one of them is that
//! defect come back.
//!
//! ## Reproducing a failure
//!
//! The message names the seed. The run's files stay under
//! `_agent_output/process-storm/<case>`, and the kill schedule and every
//! worker's transactions come from the seed, so the same case with the same
//! seed runs the same workload; the moments the kills land are the operating
//! system's.

use std::path::{Path, PathBuf};
use std::time::Duration;

use inillucent_compat::chaos::Settings;
use inillucent_compat::storm::{run, Storm};
use inillucent_compat::workspace_root;

/// Returns `inillucent-chaos`, which cargo builds for this package's tests.
fn chaos() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_inillucent-chaos"))
}

/// Returns a directory of this case's own, emptied first.
///
/// `STORM_DIR` names another parent directory, so several copies of one case
/// can run at once. The intermittent failures here show up when several
/// storms run side by side, and that is how they are reproduced.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let parent = std::env::var_os("STORM_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace_root().join("_agent_output/process-storm"));
    let path = parent.join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// The storm every case starts from: four workers, two readers, kills on,
/// fifteen seconds.
///
/// **The floors are low on purpose and still able to fail.** These are debug
/// builds sharing the machine with the rest of the tier, so a worker commits a
/// few transactions a second and a reader that is killed every second or so
/// may finish one snapshot check in its life. Twenty acknowledged transactions
/// and one reader check is what an idle run passes many times over, and what a
/// run whose processes all failed at open, or whose readers were refused for
/// its whole length, does not reach. Both of those happened on 2.0.7.
///
/// @param name - the case's name
/// @param seed - the seed
fn storm(name: &str, seed: u64) -> Storm {
    Storm {
        name: name.to_string(),
        workers: 4,
        readers: 2,
        duration: Duration::from_secs(15),
        kills: true,
        seed,
        transactions: 60,
        settings: Settings {
            busy_ms: 30_000,
            ..Settings::default()
        },
        least_acknowledged: 20,
        least_checks: 1,
        kill_all_at_end: false,
    }
}

/// Runs a storm and fails the case with the run's own words if it fails.
///
/// @param storm - the storm
/// @param directory - where its files go
fn passes(storm: &Storm, directory: &Path) {
    match run(storm, &chaos(), directory) {
        Ok(report) => {
            eprintln!(
                "{}: {} acknowledged, {} reader checks, {} kills, {} busy lines; {}",
                storm.name,
                report.acknowledged,
                report.checks,
                report.kills,
                report.busy_lines,
                report.verdict
            );
        }
        Err(why) => panic!(
            "storm {} failed (files kept in {}): {why}",
            storm.name,
            directory.display()
        ),
    }
}

/// Four workers and two readers with the default pool, killed at random.
#[test]
fn a_storm_of_kills_loses_nothing() {
    let storm = storm("kills", 0x2173_0001);
    passes(&storm, &area("kills"));
}

/// The same workload with nothing killed, so a failure here is a concurrency
/// defect and not a recovery one.
#[test]
fn a_storm_without_kills_loses_nothing() {
    let mut storm = storm("no-kills", 0x2173_0002);
    storm.kills = false;
    passes(&storm, &area("no-kills"));
}

/// A 64 page pool and bulk transactions of 120 values of 24 KiB, so every
/// bulk transaction is larger than the cache: the spill path, the rollback
/// journal and a replay larger than the pool, with kills landing in all of
/// them.
#[test]
fn a_storm_with_a_pool_smaller_than_a_transaction_loses_nothing() {
    let mut storm = storm("small-pool", 0x2173_0003);
    storm.settings.frames = 64;
    storm.settings.bulk_parts = 120;
    storm.least_acknowledged = 10;
    passes(&storm, &area("small-pool"));
}

/// The search table written by every worker and searched by every reader.
#[test]
fn a_storm_writing_a_search_table_loses_nothing() {
    let mut storm = storm("search", 0x2173_0004);
    storm.settings.search = true;
    passes(&storm, &area("search"));
}

/// An encrypted database: the log, the journal and the spill file are all
/// encrypted, and every process opens with the key.
#[test]
fn a_storm_on_an_encrypted_file_loses_nothing() {
    let mut storm = storm("encrypted", 0x2173_0005);
    storm.workers = 3;
    storm.least_acknowledged = 10;
    storm.settings.key = Some("storm passphrase".to_string());
    passes(&storm, &area("encrypted"));
}

/// The shape of the 2.0.7 damage report: every worker rewrites a value on a
/// shared extent page in an autocommit statement before each transaction, the
/// search table is written and searched, readers write a small row after each
/// check, and the run ends with every process killed at the same moment.
///
/// The report's file failed `integrity_check` with a reference to a slot of a
/// shared page that the page did not hold, and only when the reader still had
/// the file open as the writer died. Nothing is killed before the end here, so
/// the log is as long as two live processes let it grow and the first replay
/// of it is the grading process's open.
#[test]
fn a_storm_in_the_shape_of_the_2_0_7_damage_report_loses_nothing() {
    let mut storm = storm("report-shape", 0x2177_0001);
    storm.kills = false;
    storm.kill_all_at_end = true;
    storm.settings.search = true;
    storm.settings.rewrite = true;
    passes(&storm, &area("report-shape"));
}

/// The same shape with kills during the run as well, so some workers die with
/// a rewrite committed and its transaction not.
#[test]
fn a_storm_in_the_shape_of_the_2_0_7_damage_report_with_kills_loses_nothing() {
    let mut storm = storm("report-shape-kills", 0x2177_0002);
    storm.kill_all_at_end = true;
    storm.settings.search = true;
    storm.settings.rewrite = true;
    passes(&storm, &area("report-shape-kills"));
}

//! The long form of `durability::process_storm`: more processes, minutes
//! rather than seconds, and the combinations of settings that only fail
//! together.
//!
//! Invariant: the same as the short form's. **However the operating system ends
//! the processes, every transaction a worker acknowledged is in the file whole,
//! no reader ever sees a broken snapshot, and the file passes its integrity
//! check.**
//!
//! ## Why a long form
//!
//! The short form runs four workers for fifteen seconds and finds a defect
//! that happens in most runs. The defects task-2173 found under load happened
//! in one storm of six, and one of them only when a 64 page pool, bulk
//! transactions and the search table were all in the same run: none of the
//! three alone, nor any two, failed in 144 storms. A nightly run has the time
//! to run each combination long enough to see a defect that rare.

use std::path::PathBuf;
use std::time::Duration;

use inillucent_compat::chaos::Settings;
use inillucent_compat::storm::{run, Storm};
use inillucent_compat::workspace_root;

/// Returns `inillucent-chaos`, which cargo builds for this package's tests.
fn chaos() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_inillucent-chaos"))
}

/// Runs one long storm and fails the case with the run's own words.
///
/// @param name - the case's name, which is also its directory's
/// @param seed - the seed
/// @param settings - what the workload includes
fn long_storm(name: &str, seed: u64, settings: Settings) {
    let directory = workspace_root()
        .join("_agent_output/storm-nightly")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    let _ = std::fs::create_dir_all(&directory);
    let storm = Storm {
        name: name.to_string(),
        workers: 8,
        readers: 4,
        duration: Duration::from_secs(180),
        kills: true,
        seed,
        transactions: 120,
        settings: Settings {
            busy_ms: 30_000,
            ..settings
        },
        least_acknowledged: 200,
        least_checks: 20,
    };
    match run(&storm, &chaos(), &directory) {
        Ok(report) => eprintln!(
            "{name}: {} acknowledged, {} reader checks, {} kills; {}",
            report.acknowledged, report.checks, report.kills, report.verdict
        ),
        Err(why) => panic!(
            "storm {name} failed (files kept in {}): {why}",
            directory.display()
        ),
    }
}

/// Eight workers and four readers with the default pool.
#[test]
fn a_long_storm_loses_nothing() {
    long_storm("default", 0x2173_1001, Settings::default());
}

/// A 64 page pool, bulk transactions and the search table, together.
#[test]
fn a_long_storm_with_a_small_pool_bulk_and_search_loses_nothing() {
    long_storm(
        "small-pool-bulk-search",
        0x2173_1002,
        Settings {
            frames: 64,
            bulk_parts: 60,
            search: true,
            ..Settings::default()
        },
    );
}

/// An encrypted file with a 64 page pool, so the spill file and the log are
/// encrypted and the spill path runs all the time.
#[test]
fn a_long_storm_on_an_encrypted_file_with_a_small_pool_loses_nothing() {
    long_storm(
        "encrypted-small-pool",
        0x2173_1003,
        Settings {
            frames: 64,
            bulk_parts: 30,
            key: Some("nightly storm".to_string()),
            ..Settings::default()
        },
    );
}

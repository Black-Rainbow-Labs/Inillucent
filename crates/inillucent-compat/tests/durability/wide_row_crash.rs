//! Power loss at every cut point of writes to a table of 2,000 integer columns.
//!
//! Invariant: **after a crash at any point, a table of wide rows holds exactly the rows the
//! statements that returned wrote, every cell of each as it was written, and passes its
//! integrity check.** A row of 2,000 integers is written in the compact encoding, which only a
//! file at format 4 reads, so the file moves to 4 with a log record of its own before the
//! first such row. Recovery has to replay that record, and then replay the row's bytes onto a
//! leaf that holds nothing else.
//!
//! **A campaign rather than a case**, for the reason `bulk_insert_crash.rs` gives.

use std::path::PathBuf;
use std::sync::Arc;

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::failpoint::Failure;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{CrashSnapshot, SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::Vfs;

/// The default page size, the one a table of this width needs.
const PAGE_SIZE: usize = 32_768;

/// How many frames the pool holds.
const FRAMES: usize = 512;

/// How many columns the table has besides the key.
const COLUMNS: usize = 1_999;

/// Returns what row `row` holds in column `column`: a number of every width the encoding has.
///
/// @param row - the row's number
/// @param column - the column's position, from 1
fn cell(row: i64, column: usize) -> i64 {
    let spread = (column as i64)
        .wrapping_mul(2_654_435_761)
        .wrapping_add(row * 40_503);
    match column % 3 {
        0 => spread.rem_euclid(120),
        1 => -spread.rem_euclid(30_000),
        _ => spread.rem_euclid(2_000_000_000),
    }
}

/// Returns the statement that inserts row `row`.
///
/// @param row - the row's number
fn insert_statement(row: i64) -> String {
    let values: Vec<String> = (1..=COLUMNS).map(|at| cell(row, at).to_string()).collect();
    format!("INSERT INTO w VALUES ({row}, {})", values.join(", "))
}

/// The statements of the workload, each its own transaction, in order.
fn workload() -> Vec<String> {
    vec![
        insert_statement(1),
        insert_statement(2),
        "UPDATE w SET c5 = 99 WHERE k = 1".to_string(),
        "DELETE FROM w WHERE k = 2".to_string(),
        insert_statement(3),
    ]
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
    PathBuf::from("/sim/wide-rows.db")
}

/// Runs one statement.
///
/// @param engine - the connection
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) -> Result<(), inillucent_base::DbError> {
    engine.execute_any(sql, &Params::new()).map(|_| ())
}

/// Builds the empty table and folds the log, and returns the simulator holding it.
///
/// @param seed - the run's seed
fn built(seed: u64) -> Arc<SimVfs> {
    let vfs = simulator(seed);
    let mut engine =
        ImportedDatabase::create_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES)
            .expect("the connection opens");
    run(&mut engine, "PRAGMA journal_mode=wal").expect("wal mode");
    run(&mut engine, "PRAGMA synchronous=full").expect("synchronous");
    let names: Vec<String> = (1..=COLUMNS).map(|at| format!("c{at} INTEGER")).collect();
    run(
        &mut engine,
        &format!(
            "CREATE TABLE w(k INTEGER PRIMARY KEY, {})",
            names.join(", ")
        ),
    )
    .expect("the table");
    engine.checkpoint().expect("the fold runs");
    drop(engine);
    vfs
}

/// Asks one integer question of a recovered database.
///
/// @param engine - the recovered connection
/// @param sql - a query answering one integer
fn ask(engine: &mut ImportedDatabase, sql: &str) -> Result<i64, String> {
    let answer = engine
        .execute_any(sql, &Params::new())
        .map_err(|failure| format!("{sql}: {failure}"))?;
    match answer.rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(number)) => Ok(*number),
        other => Err(format!("{sql} answered {other:?}")),
    }
}

/// Checks one row of a recovered table against the cells it was written with.
///
/// @param engine - the recovered connection
/// @param row - the row's number
/// @param bumped - whether the update of `c5` took effect
fn check_row(engine: &mut ImportedDatabase, row: i64, bumped: bool) -> Result<(), String> {
    for first in (1..=COLUMNS).step_by(200) {
        let last = (first + 199).min(COLUMNS);
        let mut expected: i64 = (first..=last).map(|at| cell(row, at)).sum();
        if bumped && (first..=last).contains(&5) {
            expected += 99 - cell(row, 5);
        }
        let terms: Vec<String> = (first..=last).map(|at| format!("c{at}")).collect();
        let total = ask(
            engine,
            &format!("SELECT {} FROM w WHERE k = {row}", terms.join(" + ")),
        )?;
        if total != expected {
            return Err(format!(
                "row {row}, columns {first} to {last}: {total} and not {expected}"
            ));
        }
    }
    Ok(())
}

/// Reopens what a crash left and checks it against what the workload committed.
///
/// @param snapshot - what the crash left
/// @param committed - how many statements returned success before the crash
/// @param seed - the recovery's own seed
fn check_recovered(snapshot: &CrashSnapshot, committed: usize, seed: u64) -> Result<(), String> {
    let vfs = Arc::new(SimVfs::recovered(
        SimConfig {
            seed,
            model: MediaModel::default(),
            ..SimConfig::default()
        },
        snapshot,
    ));
    let mut engine =
        ImportedDatabase::open_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES)
            .map_err(|failure| format!("the database would not open: {failure}"))?;
    let integrity = engine
        .execute_any("PRAGMA integrity_check", &Params::new())
        .map_err(|failure| format!("integrity_check: {failure}"))?;
    if integrity.rows != vec![vec![OwnedDatum::Text(b"ok".to_vec())]] {
        return Err(format!("integrity_check said {:?}", integrity.rows));
    }
    let has = |engine: &mut ImportedDatabase, key: i64| {
        ask(engine, &format!("SELECT count(*) FROM w WHERE k = {key}"))
    };
    let (one, two, three) = (
        has(&mut engine, 1)? == 1,
        has(&mut engine, 2)? == 1,
        has(&mut engine, 3)? == 1,
    );
    // The statements ran in order: insert 1, insert 2, update 1, delete 2, insert 3. What is
    // there is the state after some prefix of them, and at least the ones that returned.
    let rows = ask(&mut engine, "SELECT count(*) FROM w")?;
    if rows != i64::from(one) + i64::from(two) + i64::from(three) {
        return Err(format!("{rows} rows and keys {one} {two} {three}"));
    }
    if three && two {
        return Err("row 3 is there and row 2, which was deleted before it, is too".to_string());
    }
    if committed >= 1 && !one {
        return Err(format!(
            "{committed} statements returned and row 1 is missing"
        ));
    }
    if (2..4).contains(&committed) && !two {
        return Err("the second insert returned and is not there".to_string());
    }
    if committed >= 4 && two {
        return Err("the delete returned and row 2 is still there".to_string());
    }
    if committed >= 5 && !three {
        return Err("every statement returned and row 3 is missing".to_string());
    }
    let bumped = one && ask(&mut engine, "SELECT c5 FROM w WHERE k = 1")? == 99;
    if committed >= 3 && !bumped {
        return Err("the update returned and is not there".to_string());
    }
    if one {
        check_row(&mut engine, 1, bumped)?;
    }
    if two {
        check_row(&mut engine, 2, false)?;
    }
    if three {
        check_row(&mut engine, 3, false)?;
    }
    Ok(())
}

/// A crash at every cut point leaves the wide rows that were written, whole.
#[test]
fn every_cut_of_a_write_to_a_wide_table_is_recoverable() {
    let statements = workload();
    let mut cuts = 0u64;
    let mut report = String::new();
    for nth in 1..=3_000u64 {
        let vfs = built(11_000 + nth);
        let base = vfs.failpoints().sites_reached();
        vfs.failpoints()
            .fail_nth_call(base.saturating_add(nth), Failure::Crash);
        let mut connection =
            ImportedDatabase::open_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES);
        let mut committed = 0usize;
        if let Ok(engine) = &mut connection {
            for statement in &statements {
                match run(engine, statement) {
                    Ok(()) => committed += 1,
                    Err(_) => break,
                }
            }
        }
        let reached = vfs.failpoints().sites_reached().saturating_sub(base);
        let snapshot = vfs.crash();
        drop(connection);
        cuts = cuts.saturating_add(1);
        if let Err(said) = check_recovered(&snapshot, committed, 12_000 + nth) {
            panic!("cut {nth} after {committed} commits: {said}");
        }
        report.push_str(&format!("{nth}\t{committed}\n"));
        if reached < nth {
            assert_eq!(
                committed,
                statements.len(),
                "an unarmed run must commit every statement"
            );
            break;
        }
    }
    assert!(cuts > 20, "only {cuts} cut points were reached");
    let directory = inillucent_compat::workspace_root().join("_agent_output/wide-row-crash");
    let _ = std::fs::create_dir_all(&directory);
    let _ = std::fs::write(directory.join("cuts.tsv"), report);
}

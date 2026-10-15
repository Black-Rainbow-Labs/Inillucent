//! Power loss at every cut point of `ADD COLUMN` and of the writes that widen a short leaf.
//!
//! Invariant: **after a crash at any point, a table that was given a column reads every
//! row with that column's default or the value written into it, each statement that
//! returned is there, none is half there, and the database passes its integrity check.**
//! `ADD COLUMN` writes the catalog row and no leaf, so what a reader sees after a crash is a
//! catalog that names a column and leaves that were written without it. The first write
//! into such a leaf rewrites it as a page image or as the images of its halves, and that
//! rewrite is what a replay has to copy. A replay that re-derived the wider rows from
//! the leaf would not know the default, which is why the record carries the image.
//!
//! **A campaign rather than a case**, for the reason `bulk_insert_crash.rs` gives: every
//! injectable call of the run is numbered, and the run is repeated once per number with the
//! failure armed at exactly that call.

use std::path::PathBuf;
use std::sync::Arc;

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::failpoint::Failure;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{CrashSnapshot, SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::Vfs;

/// The page size these runs build at, small so the table takes many leaves.
const PAGE_SIZE: usize = 4_096;

/// How many frames the pool holds.
const FRAMES: usize = 4_096;

/// How many rows the table holds before the workload.
const ROWS: i64 = 1_500;

/// The text the second added column defaults to, long enough that a widened row is larger.
const DEFAULT_TEXT: &str = "a default of some length so widening a leaf makes it hold fewer rows";

/// The statements of the workload, each its own transaction, in the order they run.
const WORKLOAD: [&str; 4] = [
    "ALTER TABLE t ADD COLUMN c INTEGER DEFAULT 7",
    "ALTER TABLE t ADD COLUMN d TEXT DEFAULT 'a default of some length so widening a leaf makes it hold fewer rows'",
    "UPDATE t SET c = c + 1 WHERE a % 50 = 0",
    "INSERT INTO t(a, b) VALUES (100000, 'late')",
];

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
    PathBuf::from("/sim/add-column.db")
}

/// Runs one statement.
///
/// @param engine - the connection
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) -> Result<(), inillucent_base::DbError> {
    engine.execute_any(sql, &Params::new()).map(|_| ())
}

/// Builds the table and folds the log, and returns the simulator holding it.
///
/// @param seed - the run's seed
fn built(seed: u64) -> Arc<SimVfs> {
    let vfs = simulator(seed);
    let mut engine =
        ImportedDatabase::create_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES)
            .expect("the connection opens");
    run(&mut engine, "PRAGMA journal_mode=wal").expect("wal mode");
    run(&mut engine, "PRAGMA synchronous=full").expect("synchronous");
    run(&mut engine, "CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT)").expect("the table");
    run(
        &mut engine,
        &format!(
            "INSERT INTO t SELECT value, printf('label-%08d', value) FROM generate_series(1, {ROWS})"
        ),
    )
    .expect("the rows");
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

/// Reopens what a crash left and checks it against what the workload committed.
///
/// @param snapshot - what the crash left
/// @param committed - how many statements of the workload returned success before the crash
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
    let has = |engine: &mut ImportedDatabase, name: &str| {
        ask(
            engine,
            &format!("SELECT count(*) FROM pragma_table_info('t') WHERE name = '{name}'"),
        )
    };
    let has_c = has(&mut engine, "c")? == 1;
    let has_d = has(&mut engine, "d")? == 1;
    if has_d && !has_c {
        return Err("column d exists and c does not".to_string());
    }
    if committed >= 1 && !has_c || committed >= 2 && !has_d {
        return Err(format!(
            "{committed} statements returned and a column is missing"
        ));
    }
    let rows = ask(&mut engine, "SELECT count(*) FROM t")?;
    let late = ask(&mut engine, "SELECT count(*) FROM t WHERE a = 100000")?;
    if rows != ROWS + late || late > 1 || committed >= 4 && late != 1 {
        return Err(format!(
            "{rows} rows and {late} late rows after {committed} commits"
        ));
    }
    if has_c {
        let bumped = ask(&mut engine, "SELECT count(*) FROM t WHERE c = 8")?;
        let plain = ask(&mut engine, "SELECT count(*) FROM t WHERE c = 7")?;
        if bumped + plain != rows {
            return Err(format!(
                "c holds {bumped} eights and {plain} sevens in {rows} rows"
            ));
        }
        let every_fiftieth = ROWS / 50;
        if bumped != 0 && bumped != every_fiftieth {
            return Err(format!(
                "the update is half there: {bumped} of {every_fiftieth}"
            ));
        }
        if committed >= 3 && bumped != every_fiftieth {
            return Err("the update returned and is not there".to_string());
        }
    }
    if has_d {
        let defaulted = ask(
            &mut engine,
            &format!("SELECT count(*) FROM t WHERE d = '{DEFAULT_TEXT}'"),
        )?;
        if defaulted != rows {
            return Err(format!("d reads its default in {defaulted} of {rows} rows"));
        }
    }
    Ok(())
}

/// A crash at every cut point leaves a table that reads right, whatever statement it cut.
#[test]
fn every_cut_of_add_column_and_the_writes_after_it_is_recoverable() {
    let mut cuts = 0u64;
    let mut report = String::new();
    for nth in 1..=4_000u64 {
        let vfs = built(8_000 + nth);
        let base = vfs.failpoints().sites_reached();
        vfs.failpoints()
            .fail_nth_call(base.saturating_add(nth), Failure::Crash);
        let mut connection =
            ImportedDatabase::open_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES);
        let mut committed = 0usize;
        if let Ok(engine) = &mut connection {
            for statement in WORKLOAD {
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
        if let Err(said) = check_recovered(&snapshot, committed, 9_000 + nth) {
            panic!("cut {nth} after {committed} commits: {said}");
        }
        report.push_str(&format!("{nth}\t{committed}\n"));
        if reached < nth {
            assert_eq!(
                committed,
                WORKLOAD.len(),
                "an unarmed run must commit every statement"
            );
            break;
        }
    }
    assert!(cuts > 40, "only {cuts} cut points were reached");
    let directory = inillucent_compat::workspace_root().join("_agent_output/add-column-crash");
    let _ = std::fs::create_dir_all(&directory);
    let _ = std::fs::write(directory.join("cuts.tsv"), report);
}

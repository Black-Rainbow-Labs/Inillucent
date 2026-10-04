//! Power loss at every cut point of an `INSERT ... SELECT` into an empty table.
//!
//! Invariant: **a copy into an empty table is all there after a crash or not
//! there at all, and the database opens, reads and passes its integrity check
//! either way.** The copy takes the bulk build: the table's empty tree is
//! released, a new tree is written straight into the data file with no log
//! record describing its pages, the index is built the same way, and the
//! catalog rows are rewritten to name the new roots. That is three things the
//! row by row insert never does, and each has a crash window of its own:
//!
//! - **before the commit**, the catalog rows on disk still name the empty
//!   trees, and the `AllocPage` records belong to a transaction with no
//!   `Commit`, so recovery replays none of them. The table comes back empty.
//! - **after the commit**, the data file was synced before the log was, so
//!   every page the catalog rows name is on the media. The table comes back
//!   whole, and its index agrees with it.
//!
//! A cut that leaves part of the rows, a catalog row naming a page nobody
//! wrote, or an index that disagrees with its table fails this file.
//!
//! **A campaign rather than a case**, for the reason `bulk_build_crash.rs`
//! gives: every injectable call of the run is numbered, and the run is repeated
//! once per number with the failure armed at exactly that call.
//!
//! The two cases at the end build onto pages that held something a statement
//! ago, which is what made the same bulk builder lose an index in the change
//! that stamped built pages with their own record's position: the log and the
//! rollback journal can each put a page's previous life back, and a page the
//! build wrote has no record of its own to replay forward again.

use std::path::PathBuf;
use std::sync::Arc;

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::failpoint::Failure;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{CrashSnapshot, SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::Vfs;

/// The page size these runs build at.
///
/// Four kilobytes, so the padded rows need several leaves and an interior
/// level, which is the shape that makes the run of leaves, the level above
/// them and the `BulkBuilt` record all present.
const PAGE_SIZE: usize = 4_096;

/// How many frames the pool holds.
const FRAMES: usize = 4_096;

/// A pool small enough that an ordinary statement evicts, and so fills a
/// rollback journal with pre-images.
const SMALL_FRAMES: usize = 64;

/// How many rows the source table holds, and so how many the copy writes.
///
/// Over the 1,024 rows the bulk build starts at, so the copy takes it.
const ROWS: usize = 1_100;

/// The statement each run of the campaign tries to commit.
const WORKLOAD: &str = "INSERT INTO copied SELECT a, b FROM t;";

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
    PathBuf::from("/sim/bulk-insert.db")
}

/// Runs a script of one or more statements, stopping at the first failure.
///
/// @param engine - the connection
/// @param sql - one or more statements
fn run(engine: &mut ImportedDatabase, sql: &str) -> Result<(), inillucent_base::DbError> {
    let mut rest = sql;
    loop {
        let trimmed = rest.trim_start();
        if trimmed.is_empty() {
            return Ok(());
        }
        let consumed = engine.statement_length(trimmed)?;
        let Some(head) = trimmed.get(..consumed) else {
            return Ok(());
        };
        if head.trim().is_empty() {
            return Ok(());
        }
        engine.execute_any(head, &Params::new())?;
        rest = trimmed.get(consumed..).unwrap_or("");
    }
}

/// Returns the statement that fills the source table.
///
/// One statement rather than a row at a time, because the campaign builds the
/// source again for every cut point. The source table is empty when it runs,
/// so it takes the bulk build as well, before any failure is armed.
fn source_rows() -> String {
    format!(
        "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < {ROWS}) \
         INSERT INTO t SELECT x, printf('label-%08d-padding-padding-padding', x) FROM c;"
    )
}

/// Builds the source table and the empty indexed target, folds the log, and
/// returns the simulator holding them.
///
/// @param seed - the run's seed
fn built(seed: u64) -> Arc<SimVfs> {
    let vfs = simulator(seed);
    let mut engine =
        ImportedDatabase::create_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES)
            .expect("the connection opens");
    run(
        &mut engine,
        "PRAGMA journal_mode=wal;
         PRAGMA synchronous=full;
         CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
         CREATE TABLE copied(a INTEGER PRIMARY KEY, b TEXT);
         CREATE INDEX copied_b ON copied(b);",
    )
    .expect("the schema builds");
    run(&mut engine, &source_rows()).expect("the rows insert");
    // Folded, so the cuts below are cuts of the copy and not of the source's
    // log still being folded in.
    engine.checkpoint().expect("the fold runs");
    drop(engine);
    vfs
}

/// What reopening a crashed database produced.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Recovery {
    /// It opened, and the copy was all there, through the table and the index.
    Whole,
    /// It opened, and the copied table was empty.
    Empty,
    /// It refused to be read, or came back in a state no commit describes.
    Broken(String),
}

/// Asks one integer question of a recovered database.
///
/// @param engine - the recovered connection
/// @param sql - a query answering one integer
fn ask(engine: &mut ImportedDatabase, sql: &str) -> Result<i64, String> {
    let answer = engine
        .execute_any(sql, &Params::new())
        .map_err(|failure| detail(&failure))?;
    first_integer(&answer.rows).ok_or_else(|| format!("{sql} answered no integer"))
}

/// Reopens what a crash left behind and says which of the two states it is in.
///
/// **The table, the index and the integrity check are all asked**, because a
/// copy can go wrong in each separately: a catalog row naming a root whose
/// pages were never written refuses the table read, an index left behind by a
/// rolled back build disagrees with the table, and a page given to the build
/// and never given back is `never used`.
///
/// @param snapshot - what the crash left
/// @param frames - how many frames the reopened pool holds
/// @param seed - the recovery's own seed
fn recovered(snapshot: &CrashSnapshot, frames: usize, seed: u64) -> Recovery {
    let vfs = Arc::new(SimVfs::recovered(
        SimConfig {
            seed,
            model: MediaModel::default(),
            ..SimConfig::default()
        },
        snapshot,
    ));
    let mut engine = match ImportedDatabase::open_on(
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        path(),
        PAGE_SIZE,
        frames,
    ) {
        Ok(engine) => engine,
        Err(failure) => return Recovery::Broken(detail(&failure)),
    };
    match classify(&mut engine) {
        Ok(state) => state,
        Err(said) => Recovery::Broken(said),
    }
}

/// Reads a recovered database and names its state.
///
/// @param engine - the recovered connection
fn classify(engine: &mut ImportedDatabase) -> Result<Recovery, String> {
    let source = ask(engine, "SELECT count(*) FROM t")?;
    if source != ROWS as i64 {
        return Err(format!(
            "the source came back with {source} rows rather than {ROWS}"
        ));
    }
    let copied = ask(engine, "SELECT count(*) FROM copied")?;
    let through_the_index = ask(
        engine,
        "SELECT count(*) FROM copied WHERE b >= 'label-00000000'",
    )?;
    if through_the_index != copied {
        return Err(format!(
            "the index answered {through_the_index} rows and the table {copied}"
        ));
    }
    let integrity = engine
        .execute_any("PRAGMA integrity_check", &Params::new())
        .map_err(|failure| detail(&failure))?;
    let said: Vec<String> = integrity
        .rows
        .iter()
        .filter_map(|row| match row.first() {
            Some(OwnedDatum::Text(text)) => Some(String::from_utf8_lossy(text).into_owned()),
            _ => None,
        })
        .collect();
    if said != ["ok"] {
        return Err(format!("integrity_check said {said:?}"));
    }
    match copied {
        0 => Ok(Recovery::Empty),
        whole if whole == ROWS as i64 => {
            let same = ask(
                engine,
                "SELECT count(*) FROM copied JOIN t USING (a) WHERE copied.b = t.b",
            )?;
            if same != ROWS as i64 {
                return Err(format!("{same} of {ROWS} copied rows match their source"));
            }
            Ok(Recovery::Whole)
        }
        part => Err(format!("the copy came back with {part} of {ROWS} rows")),
    }
}

/// Renders a failure with its detail, which is where the page and the
/// checksums are.
///
/// @param failure - what the engine reported
fn detail(failure: &inillucent_base::DbError) -> String {
    match failure.detail() {
        Some(said) => format!("{failure}: {said}"),
        None => format!("{failure}"),
    }
}

/// Returns the first value of the first row, as an integer.
///
/// @param rows - the outcome's rows
fn first_integer(rows: &[Vec<OwnedDatum>]) -> Option<i64> {
    match rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(number)) => Some(*number),
        _ => None,
    }
}

/// Proves the campaign exercises the bulk build and not the row by row insert.
///
/// The row by row insert appends a record per row, so a copy of 1,100 rows
/// that appends fewer than a quarter as many records took the bulk build. Without this the
/// campaign below would pass on the old insert, which is atomic for the same
/// reason any statement is.
#[test]
fn the_campaign_copy_takes_the_bulk_build() {
    let vfs = built(4_900);
    let mut engine =
        ImportedDatabase::open_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES)
            .expect("the connection opens");
    let before = engine.wal().stats().records;
    run(&mut engine, WORKLOAD).expect("the copy runs");
    let appended = engine.wal().stats().records.saturating_sub(before);
    assert!(
        appended < (ROWS / 4) as u64,
        "the copy appended {appended} log records for {ROWS} rows, which is the row by row \
         insert and not the bulk build"
    );
}

/// A crash at every cut point of a bulk copy leaves the copy whole or absent,
/// and never part of it.
#[test]
fn every_cut_of_a_bulk_copy_is_recoverable() {
    let mut cuts = 0u64;
    let mut whole = 0u64;
    let mut empty = 0u64;
    let mut report = String::new();
    // The `break` decides the real bound, as in `bulk_build_crash.rs`: it
    // stops the first time a run reaches fewer sites than the cut being armed,
    // which is the run that committed and then lost power.
    for nth in 1..=2_000u64 {
        let vfs = built(5_000 + nth);
        let base = vfs.failpoints().sites_reached();
        vfs.failpoints()
            .fail_nth_call(base.saturating_add(nth), Failure::Crash);
        let mut connection =
            ImportedDatabase::open_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, FRAMES);
        let committed = match &mut connection {
            Ok(engine) => run(engine, WORKLOAD).is_ok(),
            Err(_) => false,
        };
        let reached = vfs.failpoints().sites_reached().saturating_sub(base);
        let snapshot = vfs.crash();
        drop(connection);
        cuts = cuts.saturating_add(1);
        let state = recovered(&snapshot, FRAMES, 6_000 + nth);
        let verdict = match &state {
            Recovery::Whole => {
                whole = whole.saturating_add(1);
                "whole"
            }
            Recovery::Empty => {
                empty = empty.saturating_add(1);
                "empty"
            }
            Recovery::Broken(said) => panic!("cut {nth}: the database came back wrong: {said}"),
        };
        assert!(
            !committed || verdict == "whole",
            "cut {nth}: the copy reported success and is not all there after recovery"
        );
        report.push_str(&format!("{nth}\t{verdict}\t{committed}\n"));
        if reached < nth {
            assert!(
                committed,
                "an unarmed run must commit; the workload is not deterministic"
            );
            break;
        }
    }
    assert!(cuts > 20, "only {cuts} cut points were reached");
    assert!(
        empty > 0,
        "no cut left the copy absent, so the state before the commit was never tested"
    );
    assert!(
        whole > 0,
        "no cut left the copy whole, so the state after the commit was never tested"
    );
    let directory = inillucent_compat::workspace_root().join("_agent_output/bulk-insert-crash");
    let _ = std::fs::create_dir_all(&directory);
    let _ = std::fs::write(
        directory.join("cuts.tsv"),
        format!("# {cuts} cuts: {whole} whole, {empty} empty\ncut\tstate\tcommitted\n{report}"),
    );
}

/// Fills `copied`, empties it, and copies into it again, with nothing folded
/// since the first fill, so the build is handed pages whose previous life is
/// still in the log, and at a small pool in the rollback journal too.
///
/// Returns the media as it stands after the copy committed, with the
/// connection still open: a close would fold the log and hide the case.
///
/// @param mode - the `journal_mode` to run under
/// @param frames - how many frames the pool holds
/// @param seed - the run's seed
fn copied_onto_freed_pages(mode: &str, frames: usize, seed: u64) -> CrashSnapshot {
    let vfs = simulator(seed);
    let mut engine =
        ImportedDatabase::create_on(Arc::clone(&vfs) as Arc<dyn Vfs>, path(), PAGE_SIZE, frames)
            .expect("the connection opens");
    run(
        &mut engine,
        &format!(
            "PRAGMA journal_mode={mode};
             PRAGMA synchronous=full;
             CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT);
             CREATE TABLE copied(a INTEGER PRIMARY KEY, b TEXT, c TEXT);
             CREATE INDEX copied_b ON copied(b);"
        ),
    )
    .expect("the schema builds");
    run(&mut engine, &source_rows()).expect("the rows insert");
    // A previous life for the pages: wide values in a column no index covers,
    // so the table holds extent pages as well as leaves, written row by row
    // and then deleted.
    let mut earlier = String::from("BEGIN;\n");
    for nth in 1..=600 {
        let width = if nth % 5 == 0 { 3_000 } else { 200 };
        earlier.push_str(&format!(
            "INSERT INTO copied VALUES({nth}, 'old-{nth:08}', '{}');\n",
            "z".repeat(width)
        ));
    }
    earlier.push_str("COMMIT;\nDELETE FROM copied;\n");
    run(&mut engine, &earlier).expect("the earlier rows come and go");
    run(&mut engine, "INSERT INTO copied(a, b) SELECT a, b FROM t;").expect("the copy runs");
    // Read everything back, which at a small pool writes every dirty frame out
    // and fills the rollback journal with their pre-images.
    run(&mut engine, "SELECT a, b FROM copied ORDER BY a;").expect("the read runs");
    let snapshot = vfs.crash();
    drop(engine);
    snapshot
}

/// A crash after a copy onto freed pages keeps the copy, in write ahead log
/// mode, where only the log can put the pages' previous life back.
#[test]
fn a_crash_after_a_copy_onto_freed_pages_keeps_it() {
    let crashed = copied_onto_freed_pages("wal", FRAMES, 7_300);
    assert_eq!(recovered(&crashed, FRAMES, 7_301), Recovery::Whole);
}

/// A hot rollback journal does not put back the pages a copy was built on.
#[test]
fn a_hot_journal_does_not_put_back_a_copy_built_over_it() {
    let crashed = copied_onto_freed_pages("delete", SMALL_FRAMES, 7_400);
    assert_eq!(recovered(&crashed, SMALL_FRAMES, 7_401), Recovery::Whole);
}

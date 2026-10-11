//! A free map page that sits at the end of a large file moves when the file shrinks.
//!
//! Invariant: **after a `FULL` commit gives back the pages at the end of a file
//! larger than one free map page covers, every remaining row reads and
//! `integrity_check` answers `ok`, before and after a reopen.**
//!
//! ## Why this exists
//!
//! At 8,192 byte pages one free map page covers 65,280 pages, so a file past that
//! size has a second map page. That page is allocated near the end, and the
//! shrink step has to move it like any other page: the bits it holds go to a lower
//! page and the chain of map pages is relinked. The unit test
//! `a_relinked_map_page_keeps_every_bit` in `freemap.rs` covers the chain rewrite
//! cheaply. This case runs it through the engine on a file of about 510 MB, which
//! is why it is a nightly case.

use std::sync::Arc;

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::{MemoryVfs, Vfs};

/// The page size the case runs at.
const PAGE_SIZE: usize = 8_192;

/// How many frames the pool holds.
const FRAMES: usize = 4_096;

/// Runs a statement of any kind and returns its rows.
///
/// @param engine - the open connection
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) -> Vec<Vec<OwnedDatum>> {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|why| panic!("{sql}: {}", why.message()))
        .rows
}

/// Returns the one integer a query answers with.
///
/// @param engine - the open connection
/// @param sql - a query returning one row of one integer
fn count(engine: &mut ImportedDatabase, sql: &str) -> i64 {
    match run(engine, sql).first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(value)) => *value,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Asserts the integrity check answers `ok`.
///
/// @param engine - the open connection
fn assert_whole(engine: &mut ImportedDatabase) {
    let rows = run(engine, "PRAGMA integrity_check");
    assert_eq!(rows.len(), 1, "{rows:?}");
}

/// Fills a file past one map page, deletes the front, and checks the file.
#[test]
fn a_free_map_page_at_the_end_moves_too() {
    let vfs: Arc<dyn Vfs> = Arc::new(MemoryVfs::new());
    let path = std::env::temp_dir().join("inillucent-free-map-end.rdb");
    let mut engine = ImportedDatabase::create_on(Arc::clone(&vfs), path.clone(), PAGE_SIZE, FRAMES)
        .expect("the database is created in memory");
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    run(&mut engine, "CREATE TABLE t(a INTEGER PRIMARY KEY, b BLOB)");
    run(
        &mut engine,
        "INSERT INTO t SELECT value, randomblob(3000) FROM generate_series(1, 200000)",
    );
    let grown = count(&mut engine, "PRAGMA page_count");
    assert!(
        grown > 65_280,
        "the file is {grown} pages, which one map page covers"
    );
    run(&mut engine, "DELETE FROM t WHERE a <= 150000");
    let shrunk = count(&mut engine, "PRAGMA page_count");
    assert!(shrunk < grown / 2, "{grown} pages, then {shrunk}");
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0);
    assert_eq!(count(&mut engine, "SELECT count(*) FROM t"), 50_000);
    assert_whole(&mut engine);
    drop(engine);
    let mut reopened =
        ImportedDatabase::open_on(vfs, path, PAGE_SIZE, FRAMES).expect("the database reopens");
    assert_eq!(count(&mut reopened, "SELECT count(*) FROM t"), 50_000);
    assert_whole(&mut reopened);
}

//! A correlated lookup into a grouped derived table or view reads the table behind it once.
//!
//! Invariant: **a correlated block whose one FROM term is a derived table or a view that reads
//! nothing from the outer row builds a hash table on the materialised rows the first time it is
//! asked, and every later outer row probes that table, so the pages fetched for the statement grow
//! with the rows of the outer table by that table's own pages and not by the pages of the table
//! behind the view.**
//!
//! ## Why this exists
//!
//! `EXPLAIN` named an automatic index for `SELECT (SELECT g FROM agg WHERE agg.s = o.v) FROM o`,
//! where `agg` is `SELECT g, sum(x) AS s FROM d GROUP BY g`, and the executor then ran the view's
//! query again for each row of `o`: a scan of every page of `d` and a grouping, per outer row.
//! SQLite builds the index on the materialised rows. The block's table was only built for a plain
//! table scan, and a derived table is a different kind of term.
//!
//! The count is the pages the pool was asked for, a number that does not depend on how loaded the
//! machine is. With the table behind the view spread over many pages, a statement that rereads it
//! for each outer row asks for tens of thousands of pages, and one that reads it once asks for a
//! few hundred.

use std::path::PathBuf;
use std::sync::Arc;

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::Vfs;

/// A small page, so that the table behind the view has many pages.
const PAGE_SIZE: usize = 4_096;
const FRAMES: usize = 8_192;

/// Rows of the table the view groups, and how many groups it has.
const DETAIL_ROWS: u32 = 20_000;
const GROUPS: u32 = 500;

/// Runs one statement and returns its rows.
///
/// @param engine - the database
/// @param sql - the statement
fn rows(engine: &mut ImportedDatabase, sql: &str) -> Vec<Vec<OwnedDatum>> {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()))
        .rows
}

/// Returns the one integer a query answers.
///
/// @param engine - the database
/// @param sql - a query answering one integer
fn integer(engine: &mut ImportedDatabase, sql: &str) -> i64 {
    match rows(engine, sql).first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(number)) => *number,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Opens a database holding `o` with `outer` rows, whose `v` equals the sum of one group and whose `w` is 40 or 41, `d` with the detail rows, and the view `agg`.
///
/// @param outer - how many rows `o` holds
fn database(outer: u32) -> ImportedDatabase {
    let vfs = Arc::new(SimVfs::new(SimConfig {
        seed: 39,
        model: MediaModel::default(),
        ..SimConfig::default()
    }));
    let mut engine = ImportedDatabase::create_on(
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        PathBuf::from("derived-lookup.rdb"),
        PAGE_SIZE,
        FRAMES,
    )
    .expect("the database is created");
    for sql in [
        "CREATE TABLE o(id INTEGER PRIMARY KEY, v INTEGER, w INTEGER)".to_string(),
        "CREATE TABLE d(id INTEGER PRIMARY KEY, g INTEGER, x INTEGER, pad TEXT)".to_string(),
        format!(
            "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < {outer}) \
             INSERT INTO o SELECT i, 390000 + 40 * (i % {GROUPS}), 40 + i % 2 FROM c"
        ),
        format!(
            "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < {DETAIL_ROWS}) \
             INSERT INTO d SELECT i, i % {GROUPS}, i, 'padding padding padding padding' FROM c"
        ),
        "CREATE VIEW agg AS SELECT g, sum(x) AS s, count(*) AS n FROM d GROUP BY g".to_string(),
    ] {
        rows(&mut engine, &sql);
    }
    engine
}

/// Returns how many pages the pool was asked for while a statement ran, and its first row.
///
/// @param engine - the database
/// @param sql - the statement
fn fetches(engine: &mut ImportedDatabase, sql: &str) -> (u64, i64) {
    let before = engine.pool_stats();
    let answer = integer(engine, sql);
    let after = engine.pool_stats();
    (
        (after.hits + after.misses).saturating_sub(before.hits + before.misses),
        answer,
    )
}

/// The three ways a statement looks a grouped view up once per outer row.
const LOOKUPS: [&str; 3] = [
    "SELECT sum((SELECT g FROM agg WHERE agg.s = o.v)) FROM o",
    "SELECT count(*) FROM o WHERE EXISTS (SELECT 1 FROM agg WHERE agg.n = o.w)",
    "SELECT count(*) FROM o WHERE NOT EXISTS \
     (SELECT 1 FROM (SELECT g, max(x) AS m FROM d GROUP BY g) q WHERE q.m = o.v)",
];

/// The pages fetched stay flat as the outer table doubles.
#[test]
fn the_pages_fetched_do_not_grow_with_the_outer_rows_by_the_pages_behind_the_view() {
    let mut small = database(400);
    let mut large = database(800);
    let behind_the_view = integer(&mut large, "SELECT count(*) FROM d") as u64;
    assert_eq!(behind_the_view, u64::from(DETAIL_ROWS));
    for sql in LOOKUPS {
        let (few, small_answer) = fetches(&mut small, sql);
        let (many, large_answer) = fetches(&mut large, sql);
        println!("{sql}\n  400 outer rows: {few} fetches, 800 outer rows: {many} fetches");
        // Reading `d` once is about 250 pages at 4 KB; reading it for each of 400 outer rows
        // would be 100,000 fetches. The bound sits far from both.
        assert!(
            many < 5_000,
            "{sql}: {many} page fetches for 800 outer rows; the view is read again per outer row"
        );
        assert!(
            many < few * 2,
            "{sql}: {few} fetches for 400 outer rows and {many} for 800 is growth by the pages behind the view"
        );
        assert!(small_answer >= 0 && large_answer >= small_answer, "{sql}");
    }
}

/// The keyed lookup answers what a join against the same view answers.
#[test]
fn the_keyed_lookup_answers_what_a_join_answers() {
    let mut engine = database(600);
    let by_lookup = integer(
        &mut engine,
        "SELECT count(*) FROM o WHERE EXISTS (SELECT 1 FROM agg WHERE agg.n = o.w)",
    );
    let by_join = integer(
        &mut engine,
        "SELECT count(*) FROM o WHERE id IN \
         (SELECT o.id FROM o JOIN agg ON agg.n = o.w)",
    );
    assert_eq!(by_lookup, by_join);
    let sum = integer(
        &mut engine,
        "SELECT sum((SELECT g FROM agg WHERE agg.s = o.v)) FROM o",
    );
    let expected = integer(
        &mut engine,
        "SELECT coalesce(sum(a.g), 0) FROM o JOIN agg a ON a.s = o.v",
    );
    assert_eq!(sum, expected);
}

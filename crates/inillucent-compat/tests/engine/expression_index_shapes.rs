//! An index on an expression answers every shape of query an index on a column answers.
//!
//! Invariant: **a comparison range, a `BETWEEN`, an `IN` list, an `ORDER BY`, a `min()` or `max()`
//! and a `GROUP BY` over the expression an index computes are answered from that index, and return
//! what a scan of the table returns.**
//!
//! ## Why this exists
//!
//! The planner matched an expression key only against an equality. On 200,000 rows
//! `ORDER BY json_extract(p, '$.k') LIMIT 10` took 65 ms against SQLite's 0.01 ms, and
//! `WHERE json_extract(p, '$.k') > 997` took 32 ms against 0.01 ms (bug hunt 11). The plan is
//! asserted, because the plan is what changed, and every answer is compared with a scan of the same
//! table through `NOT INDEXED`.
//!
//! `SELECT min(json_extract(p, '$.k')) FROM t` took 42 ms on 200,000 rows against SQLite's 0.06 ms,
//! and the same query with `GROUP BY json_extract(p, '$.k')` took 44 ms against SQLite's 5 ms. The
//! `min()` and `max()` statements read one entry of the index now, which a timing over 100,000 rows
//! tells apart from a scan with a limit several times wider than the time it takes.

use std::time::{Duration, Instant};

use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database holding 5,000 rows with a JSON column and an index on the expression,
/// and `ixn`, a copy of the rows with no index, which a scan is checked against.
///
/// @param name - the case's name, which is also its directory's
fn shaped(name: &str) -> Database {
    let directory = workspace_root()
        .join("_agent_output/expression-index-shapes")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    database
        .session()
        .execute_batch(
            "CREATE TABLE ix(id INTEGER PRIMARY KEY, p TEXT, a INT); \
             INSERT INTO ix SELECT value, json_object('k', value % 500), value % 3 \
                 FROM generate_series(1, 5000); \
             CREATE INDEX ixe ON ix(json_extract(p, '$.k')); \
             CREATE INDEX ixae ON ix(a, json_extract(p, '$.k')); \
             CREATE TABLE ixn AS SELECT * FROM ix; \
             ANALYZE;",
        )
        .expect("the table is built");
    database
}

/// Returns the plan's lines, joined.
///
/// @param database - the database
/// @param sql - the query
fn plan(database: &Database, sql: &str) -> String {
    let rows = database
        .session()
        .query(&format!("EXPLAIN QUERY PLAN {sql}"))
        .expect("the plan is described");
    rows.iter()
        .filter_map(|row| match row.last() {
            Some(OwnedDatum::Text(text)) => Some(String::from_utf8_lossy(text).into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

/// Returns every row a query answers, as text.
///
/// @param database - the database
/// @param sql - the query
fn rows(database: &Database, sql: &str) -> Vec<String> {
    database
        .session()
        .query(sql)
        .expect("the query runs")
        .iter()
        .map(|row| format!("{row:?}"))
        .collect()
}

/// A range, a `BETWEEN`, an `IN` list and an `ORDER BY ... LIMIT` seek or walk the expression index.
#[test]
fn every_shape_over_the_expression_uses_its_index() {
    let database = shaped("plans");
    for (sql, expected) in [
        (
            "SELECT id FROM ix WHERE json_extract(p, '$.k') > 497",
            "SEARCH ix USING COVERING INDEX ixe (<expr>>?)",
        ),
        (
            "SELECT id FROM ix WHERE json_extract(p, '$.k') < 3",
            "SEARCH ix USING COVERING INDEX ixe (<expr><?)",
        ),
        (
            "SELECT id FROM ix WHERE json_extract(p, '$.k') BETWEEN 5 AND 6",
            "SEARCH ix USING COVERING INDEX ixe (<expr>>? AND <expr><?)",
        ),
        (
            "SELECT id FROM ix WHERE json_extract(p, '$.k') IN (1, 2, 3)",
            "SEARCH ix USING COVERING INDEX ixe (<expr>=?)",
        ),
        (
            "SELECT id FROM ix ORDER BY json_extract(p, '$.k') LIMIT 10",
            "SCAN ix USING COVERING INDEX ixe",
        ),
        (
            "SELECT id FROM ix WHERE a = 1 AND json_extract(p, '$.k') > 490",
            "SEARCH ix USING COVERING INDEX ixae (a=? AND <expr>>?)",
        ),
        (
            "SELECT id FROM ix WHERE a = 1 ORDER BY json_extract(p, '$.k') LIMIT 5",
            "SEARCH ix USING COVERING INDEX ixae (a=?)",
        ),
    ] {
        let described = plan(&database, sql);
        assert!(
            described.contains(expected) && !described.contains("TEMP B-TREE"),
            "{sql} was planned as {described}"
        );
    }
}

/// Every shape answers what a scan of the table answers, in the same order.
#[test]
fn every_shape_answers_what_the_table_answers() {
    let database = shaped("answers");
    for (indexed, scanned) in [
        (
            "SELECT id FROM ix WHERE json_extract(p, '$.k') > 497 ORDER BY id",
            "SELECT id FROM ix NOT INDEXED WHERE json_extract(p, '$.k') > 497 ORDER BY id",
        ),
        (
            "SELECT id FROM ix WHERE json_extract(p, '$.k') BETWEEN 5 AND 6 ORDER BY id",
            "SELECT id FROM ix NOT INDEXED WHERE json_extract(p, '$.k') BETWEEN 5 AND 6 ORDER BY id",
        ),
        (
            "SELECT id FROM ix WHERE json_extract(p, '$.k') IN (3, 1, 2, 1) ORDER BY id",
            "SELECT id FROM ix NOT INDEXED WHERE json_extract(p, '$.k') IN (3, 1, 2, 1) ORDER BY id",
        ),
        (
            "SELECT id FROM ix ORDER BY json_extract(p, '$.k') DESC, id DESC LIMIT 25",
            "SELECT id FROM ix NOT INDEXED ORDER BY json_extract(p, '$.k') DESC, id DESC LIMIT 25",
        ),
        (
            "SELECT id, p FROM ix ORDER BY json_extract(p, '$.k'), id LIMIT 25",
            "SELECT id, p FROM ix NOT INDEXED ORDER BY json_extract(p, '$.k'), id LIMIT 25",
        ),
        (
            "SELECT id FROM ix WHERE a = 2 AND json_extract(p, '$.k') >= 300 ORDER BY id",
            "SELECT id FROM ix NOT INDEXED WHERE a = 2 AND json_extract(p, '$.k') >= 300 ORDER BY id",
        ),
        (
            "SELECT id FROM ix WHERE a = 2 ORDER BY json_extract(p, '$.k') DESC, id DESC LIMIT 7",
            "SELECT id FROM ix NOT INDEXED WHERE a = 2 \
             ORDER BY json_extract(p, '$.k') DESC, id DESC LIMIT 7",
        ),
    ] {
        assert_eq!(rows(&database, indexed), rows(&database, scanned), "{indexed}");
    }
}

/// `min()` and `max()` over the indexed expression are planned as a seek to one end of the index.
#[test]
fn min_and_max_over_the_expression_use_its_index() {
    let database = shaped("extremes_plan");
    for (sql, expected) in [
        (
            "SELECT min(json_extract(p, '$.k')) FROM ix",
            "SEARCH ix USING COVERING INDEX ixe",
        ),
        (
            "SELECT max(json_extract(p, '$.k')) FROM ix",
            "SEARCH ix USING COVERING INDEX ixe",
        ),
        (
            "SELECT max(json_extract(p, '$.k')) FROM ix WHERE json_extract(p, '$.k') < 400",
            "SEARCH ix USING COVERING INDEX ixe (<expr><?)",
        ),
        (
            "SELECT min(json_extract(p, '$.k')) FROM ix WHERE a = 1",
            "SEARCH ix USING COVERING INDEX ixae (a=?)",
        ),
    ] {
        let described = plan(&database, sql);
        assert!(
            described.contains(expected) && !described.contains("TEMP B-TREE"),
            "{sql} was planned as {described}"
        );
    }
}

/// `min()` and `max()` answer what a scan of the table answers, with NULLs, ties and a `WHERE`.
#[test]
fn min_and_max_answer_what_the_table_answers() {
    let database = shaped("extremes_answers");
    database
        .session()
        .execute_batch(
            "INSERT INTO ix(p, a) VALUES ('{\"s\":1}', 1), ('{\"k\":null}', 2), \
                 ('{\"k\":\"z\"}', 0), ('{\"k\":-4.5}', 0); \
             INSERT INTO ixn SELECT * FROM ix WHERE id > 5000;",
        )
        .expect("rows with a NULL, a text and a negative value are added");
    for shape in [
        "SELECT min(json_extract(p, '$.k')) FROM ix",
        "SELECT max(json_extract(p, '$.k')) FROM ix",
        "SELECT min(json_extract(p, '$.k')), max(json_extract(p, '$.k')) FROM ix",
        "SELECT min(json_extract(p, '$.k')) FROM ix WHERE json_extract(p, '$.k') > 497",
        "SELECT max(json_extract(p, '$.k')) FROM ix WHERE json_extract(p, '$.k') < 3",
        "SELECT min(json_extract(p, '$.k')) FROM ix WHERE a = 2",
        "SELECT max(json_extract(p, '$.k')) FROM ix WHERE a = 0",
        "SELECT max(json_extract(p, '$.k')) FROM ix WHERE id < 0",
    ] {
        let scanned = shape.replace(" FROM ix", " FROM ixn");
        assert_eq!(rows(&database, shape), rows(&database, &scanned), "{shape}");
    }
}

/// `GROUP BY` the indexed expression walks the index in order and sorts nothing.
#[test]
fn group_by_the_expression_streams_its_index() {
    let database = shaped("group_plan");
    for sql in [
        "SELECT json_extract(p, '$.k'), count(*) FROM ix GROUP BY json_extract(p, '$.k')",
        "SELECT json_extract(p, '$.k') AS k, count(*) FROM ix GROUP BY json_extract(p, '$.k') ORDER BY k",
        "SELECT count(*) FROM ix GROUP BY json_extract(p, '$.k')",
    ] {
        let described = plan(&database, sql);
        assert!(
            described.contains("SCAN ix USING COVERING INDEX ixe") && !described.contains("TEMP B-TREE"),
            "{sql} was planned as {described}"
        );
    }
    for shape in [
        "SELECT json_extract(p, '$.k'), count(*), sum(a) FROM ix GROUP BY json_extract(p, '$.k')",
        "SELECT json_extract(p, '$.k') AS k, count(*) FROM ix GROUP BY json_extract(p, '$.k') ORDER BY k DESC",
        "SELECT json_extract(p, '$.k'), count(*) FROM ix GROUP BY json_extract(p, '$.k') HAVING count(*) > 9",
    ] {
        let scanned = shape.replace(" FROM ix", " FROM ixn");
        assert_eq!(rows(&database, shape), rows(&database, &scanned), "{shape}");
    }
}

/// Returns the shortest time any of several runs of a query took.
///
/// The shortest run is the one least disturbed by other work on the machine, which is what a
/// comparison of two plans needs.
///
/// @param database - the database
/// @param sql - the query
/// @param runs - how many times to run it
fn fastest_run(database: &Database, sql: &str, runs: u32) -> Duration {
    let session = database.session();
    (0..runs)
        .map(|_| {
            let started = Instant::now();
            session.query(sql).expect("the query runs");
            started.elapsed()
        })
        .min()
        .unwrap_or_default()
}

/// Over 100,000 rows, `min()` and `max()` read one entry, so a run takes a small part of what a
/// scan of the table takes.
#[test]
fn min_and_max_read_one_entry_of_a_large_table() {
    let directory = workspace_root().join("_agent_output/expression-index-shapes/extremes_time");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    database
        .session()
        .execute_batch(
            "CREATE TABLE big(id INTEGER PRIMARY KEY, p TEXT); \
             INSERT INTO big SELECT value, \
                 json_object('k', (value * 7919) % 100003, 'z', 'padding padding padding padding') \
                 FROM generate_series(1, 100000); \
             CREATE TABLE bign AS SELECT * FROM big; \
             CREATE INDEX bige ON big(json_extract(p, '$.k')); ANALYZE;",
        )
        .expect("the table is built");
    let scan = fastest_run(&database, "SELECT min(json_extract(p, '$.k')) FROM bign", 5);
    for sql in [
        "SELECT min(json_extract(p, '$.k')) FROM big",
        "SELECT max(json_extract(p, '$.k')) FROM big",
    ] {
        let indexed = fastest_run(&database, sql, 20);
        assert!(
            indexed * 5 < scan,
            "{sql} took {indexed:?} at best, against {scan:?} for a scan of the table"
        );
    }
}

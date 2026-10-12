//! An index on an expression covers a query whose only use of a column is inside that expression.
//!
//! Invariant: **a seek on an index over `json_extract(payload, '$.type')` reads only the index
//! when nothing else of the row is asked for, and answers what a scan of the table answers.**
//!
//! ## Why this exists
//!
//! `SELECT count(*) FROM events WHERE json_extract(payload, '$.type') = 'buy'` read `payload` as a
//! needed column, found the index did not hold it, and scanned the table: 129 ms against SQLite's
//! 11 ms on 300,000 events (bug hunt 9). The term the seek answers is not tested again, so the
//! column it reads is not read at all. The plan is asserted, because the plan is what changed.

use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database holding 20,000 events with a JSON payload and two expression indexes.
///
/// @param name - the case's name, which is also its directory's
fn events(name: &str) -> Database {
    let directory = workspace_root()
        .join("_agent_output/expression-index-covering")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    database
        .session()
        .execute_batch(
            "CREATE TABLE events(id INTEGER PRIMARY KEY, payload TEXT); \
             INSERT INTO events SELECT value, json_object('type', \
                 CASE value % 4 WHEN 0 THEN 'buy' ELSE 'view' END, 'n', value) \
                 FROM generate_series(1, 20000); \
             CREATE INDEX events_type ON events(json_extract(payload, '$.type')); \
             CREATE INDEX events_lower ON events(lower(payload)); \
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

/// A count, a rowid aggregate and a rowid list over the expression read only the index.
#[test]
fn an_expression_index_covers_what_reads_only_the_expression() {
    let database = events("covers");
    for (sql, index) in [
        (
            "SELECT count(*) FROM events WHERE json_extract(payload, '$.type') = 'buy'",
            "events_type",
        ),
        (
            "SELECT max(id), count(*) FROM events WHERE json_extract(payload, '$.type') = 'buy'",
            "events_type",
        ),
        (
            "SELECT count(*) FROM events WHERE lower(payload) = 'x'",
            "events_lower",
        ),
    ] {
        let described = plan(&database, sql);
        assert!(
            described.contains(&format!("USING COVERING INDEX {index}")),
            "{sql} was planned as {described}"
        );
    }
}

/// A column read outside the expression still fetches the row, and every form answers what the
/// table answers.
#[test]
fn the_index_answers_what_the_table_answers() {
    let database = events("answers");
    let described = plan(
        &database,
        "SELECT payload FROM events WHERE json_extract(payload, '$.type') = 'buy'",
    );
    assert!(
        !described.contains("COVERING"),
        "a query reading payload was planned as {described}"
    );
    for (indexed, scanned) in [
        (
            "SELECT count(*), max(id), min(id) FROM events \
             WHERE json_extract(payload, '$.type') = 'buy'",
            "SELECT count(*), max(id), min(id) FROM events NOT INDEXED \
             WHERE json_extract(payload, '$.type') = 'buy'",
        ),
        (
            "SELECT count(*) FROM events WHERE json_extract(payload, '$.type') = 'buy' \
             AND payload LIKE '%\"n\":1%'",
            "SELECT count(*) FROM events NOT INDEXED WHERE json_extract(payload, '$.type') = 'buy' \
             AND payload LIKE '%\"n\":1%'",
        ),
    ] {
        assert_eq!(rows(&database, indexed), rows(&database, scanned), "{indexed}");
    }
}

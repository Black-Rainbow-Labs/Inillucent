//! An index on an expression answers every shape of query an index on a column answers.
//!
//! Invariant: **a comparison range, a `BETWEEN`, an `IN` list and an `ORDER BY` over the expression
//! an index computes are answered from that index, and return what a scan of the table returns.**
//!
//! ## Why this exists
//!
//! The planner matched an expression key only against an equality. On 200,000 rows
//! `ORDER BY json_extract(p, '$.k') LIMIT 10` took 65 ms against SQLite's 0.01 ms, and
//! `WHERE json_extract(p, '$.k') > 997` took 32 ms against 0.01 ms (bug hunt 11). The plan is
//! asserted, because the plan is what changed, and every answer is compared with a scan of the same
//! table through `NOT INDEXED`.

use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database holding 5,000 rows with a JSON column and an index on the expression.
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

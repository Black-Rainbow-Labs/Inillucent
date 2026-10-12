//! `ORDER BY` an indexed column with a small `LIMIT` walks the index instead of sorting the table.
//!
//! Invariant: **a single table query ordered by an index's leading column, with a constant
//! `LIMIT`, walks that index and stops, whether or not the index covers the query, and returns
//! the rows the sort would have returned.**
//!
//! ## Why this exists
//!
//! `SELECT id, title FROM posts ORDER BY created_at DESC LIMIT 20` is the page of a feed, and it
//! scanned all 200,000 posts and sorted them: 19 to 116 ms where SQLite takes 0.1 ms walking
//! the index. An index that does not hold `title` was never offered as a path, and had it been,
//! it would have been priced as a row fetch for every row of the table (bug hunt 9). The plan is
//! asserted rather than a time, because the plan is what changed and it does not depend on the
//! machine.

use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database holding `posts` with an index on `created_at` and 20,000 rows.
///
/// @param name - the case's name, which is also its directory's
fn posts(name: &str) -> Database {
    let directory = workspace_root()
        .join("_agent_output/order-limit-walk")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    database
        .session()
        .execute_batch(
            "CREATE TABLE posts(id INTEGER PRIMARY KEY, user_id INT, title TEXT, \
                 status TEXT, created_at INT); \
             CREATE INDEX posts_created ON posts(created_at); \
             CREATE INDEX posts_user ON posts(user_id, created_at); \
             INSERT INTO posts SELECT value, value % 100, 'title ' || value, \
                 CASE WHEN value % 3 = 0 THEN 'draft' ELSE 'live' END, \
                 (value * 7919) % 5000 FROM generate_series(1, 20000); \
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

/// A page of a feed walks the index in both directions, with and without an `OFFSET`.
#[test]
fn a_limited_order_by_walks_the_index_that_does_not_cover_it() {
    let database = posts("walks");
    for sql in [
        "SELECT id, title FROM posts ORDER BY created_at DESC LIMIT 20",
        "SELECT id, title FROM posts ORDER BY created_at LIMIT 20",
        "SELECT id, title FROM posts ORDER BY created_at DESC LIMIT 20 OFFSET 100",
    ] {
        let described = plan(&database, sql);
        assert!(
            described.contains("USING INDEX posts_created") && !described.contains("TEMP B-TREE"),
            "{sql} was planned as {described}"
        );
    }
}

/// The walk returns what the sort returns, including ties, filters, and a `LIMIT` past the end.
///
/// `created_at` repeats every 5,000 rows, so ties are broken by `id` in both forms.
#[test]
fn the_walk_answers_what_the_sort_answers() {
    let database = posts("answers");
    for (walked, sorted) in [
        (
            "SELECT id, title FROM posts ORDER BY created_at DESC, id DESC LIMIT 25",
            "SELECT id, title FROM posts NOT INDEXED ORDER BY created_at DESC, id DESC LIMIT 25",
        ),
        (
            "SELECT id FROM posts WHERE status = 'draft' ORDER BY created_at, id LIMIT 30 OFFSET 7",
            "SELECT id FROM posts NOT INDEXED WHERE status = 'draft' \
             ORDER BY created_at, id LIMIT 30 OFFSET 7",
        ),
        (
            "SELECT id FROM posts WHERE user_id = 7 ORDER BY created_at DESC, id DESC LIMIT 5",
            "SELECT id FROM posts NOT INDEXED WHERE user_id = 7 \
             ORDER BY created_at DESC, id DESC LIMIT 5",
        ),
        (
            "SELECT count(*) FROM (SELECT id FROM posts ORDER BY created_at LIMIT 30000)",
            "SELECT count(*) FROM (SELECT id FROM posts NOT INDEXED ORDER BY created_at LIMIT 30000)",
        ),
    ] {
        assert_eq!(rows(&database, walked), rows(&database, sorted), "{walked}");
    }
}

/// Without a `LIMIT` the statement reads every row, and the scan with a sort is kept.
#[test]
fn an_unlimited_order_by_still_sorts_a_scan() {
    let database = posts("unlimited");
    let described = plan(
        &database,
        "SELECT id, title FROM posts ORDER BY created_at DESC",
    );
    assert!(
        described.contains("TEMP B-TREE"),
        "an unlimited ORDER BY was planned as {described}"
    );
}

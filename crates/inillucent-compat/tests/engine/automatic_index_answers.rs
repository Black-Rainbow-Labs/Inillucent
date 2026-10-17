//! An automatic index changes how fast a statement runs and never what it answers.
//!
//! Invariant: **every statement here answers what the pinned SQLite 3.53.4 answers, with
//! `PRAGMA automatic_index` on and off.** Two paths read a table once into a hash table: a
//! correlated subquery whose table has no index for its equality against the enclosing query,
//! and a join whose inner table has no index. Each case is chosen for a way the hash table could
//! answer differently from a scan: a key that is NULL, a count of nothing that is 0 and a
//! maximum of nothing that is NULL, a residual predicate that reads a second column of the
//! enclosing query, a key compared under an affinity, a column with a NOCASE collation, and an
//! outer join whose partners all fail the rest of the condition.
//!
//! The expected values are the pinned shell's. They are written out rather than compared with
//! the engine's own other path, because a test that asks one engine to check itself agrees with
//! whatever it does.

use inillucent_compat::workspace_root;
use inillucent_engine::connect::{Connection, Database};
use inillucent_tree::datum::OwnedDatum;

/// The two tables every case reads.
///
/// `a.k` is NULL for one row. `b.k` is NULL for one row, and `b` holds keys with several rows,
/// keys whose rows are all inactive, and a NOCASE text column.
const FIXTURE: &str = "\
CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, g INT, s TEXT, nm TEXT, pad TEXT); \
CREATE TABLE b(id INTEGER PRIMARY KEY, k INT, g INT, active INT, x INT, name TEXT COLLATE NOCASE); \
INSERT INTO a VALUES (1, 1, 1, '1', 'Apple', ''), (2, 2, 1, '2', 'pear', ''), \
 (3, 3, 2, '3', 'FIG', ''), (4, NULL, 1, NULL, NULL, ''), (5, 5, 2, '7', 'fig', ''), \
 (6, 6, 1, '6', 'kiwi', ''), (7, 1, 2, '1', 'APPLE', ''), (8, 9, 3, '9', 'lime', ''), \
 (9, 7, 1, '7', 'plum', ''), (10, 4, 1, '4', 'Pear', ''); \
INSERT INTO b VALUES (1, 1, 1, 1, 10, 'apple'), (2, 1, 2, 0, 20, 'Apple'), \
 (3, 2, 1, 1, 30, 'PEAR'), (4, 2, 1, 0, 40, 'pear'), (5, 3, 2, 1, 50, 'fig'), \
 (6, NULL, 1, 1, 60, 'plum'), (7, 7, 1, 1, 70, 'plum'), (8, 7, 2, 0, 80, 'fig'), \
 (9, 1, 2, 1, 90, 'kiwi'), (10, 6, 3, 1, 100, NULL), (11, 4, 1, 0, 110, 'pear');";

/// Returns a connection to a database holding the fixture.
///
/// @param name - the test's name, which names its file
fn fixture(name: &str) -> Database {
    let area = workspace_root().join("target/scratch/automatic-index-answers");
    let _ = std::fs::create_dir_all(&area);
    let path = area.join(format!("{name}.rdb"));
    inillucent_base::testing::remove_database(&path);
    let database = Database::open(&path).expect("a fresh database opens");
    database
        .session()
        .execute_batch(FIXTURE)
        .expect("the fixture loads");
    database
}

/// Renders a query's rows: the cells of a row joined by `/`, the rows joined by a space.
///
/// @param connection - the connection to ask
/// @param sql - the query
fn rendered(connection: &Connection<'_>, sql: &str) -> String {
    let rows = connection
        .query(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|cell| match cell {
                    OwnedDatum::Null => "NULL".to_string(),
                    OwnedDatum::Int(number) => number.to_string(),
                    OwnedDatum::Text(text) => String::from_utf8_lossy(text).into_owned(),
                    other => panic!("{sql} answered {other:?}"),
                })
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Asserts that each query answers the expected rows with the pragma on and with it off.
///
/// @param connection - the connection to ask
/// @param cases - each query with the rows the pinned shell answers
fn assert_both_ways(connection: &Connection<'_>, cases: &[(&str, &str)]) {
    for setting in ["ON", "OFF"] {
        connection
            .execute_batch(&format!("PRAGMA automatic_index = {setting};"))
            .expect("the pragma runs");
        for (sql, expected) in cases {
            assert_eq!(
                rendered(connection, sql),
                *expected,
                "automatic_index = {setting}: {sql}"
            );
        }
    }
}

/// A correlated block answers the same from a hash table as from a scan.
///
/// Rows of `a` whose key matches nothing get 0 from `count(*)` and NULL from `max(x)`, the row
/// whose key is NULL matches nothing, a filter on `b` alone narrows each bucket, a comparison
/// with a second column of `a` is left to the block, and a composite key needs both equalities.
#[test]
fn a_correlated_block_answers_what_a_scan_answers() {
    let database = fixture("blocks");
    let connection = database.session();
    assert_both_ways(
        &connection,
        &[
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.k) FROM a ORDER BY id",
                "1/3 2/2 3/1 4/0 5/0 6/1 7/3 8/0 9/2 10/1",
            ),
            (
                "SELECT id, (SELECT max(x) FROM b WHERE b.k = a.k) FROM a ORDER BY id",
                "1/90 2/40 3/50 4/NULL 5/NULL 6/100 7/90 8/NULL 9/80 10/110",
            ),
            (
                "SELECT id FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k) ORDER BY id",
                "1 2 3 6 7 9 10",
            ),
            (
                "SELECT id FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k) ORDER BY id",
                "4 5 8",
            ),
            (
                "SELECT id FROM a WHERE EXISTS \
                 (SELECT 1 FROM b WHERE b.k = a.k AND b.active = 1) ORDER BY id",
                "1 2 3 6 7 9",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.k AND b.active = 1) \
                 FROM a ORDER BY id",
                "1/2 2/1 3/1 4/0 5/0 6/1 7/2 8/0 9/1 10/0",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.k AND b.id > a.id) \
                 FROM a ORDER BY id",
                "1/2 2/2 3/1 4/0 5/0 6/1 7/1 8/0 9/0 10/1",
            ),
            (
                "SELECT id FROM a WHERE EXISTS \
                 (SELECT 1 FROM b WHERE b.k = a.k AND b.g = a.g) ORDER BY id",
                "1 2 3 7 9 10",
            ),
            (
                "SELECT id, (SELECT x FROM b WHERE b.k = a.k ORDER BY x DESC LIMIT 1) \
                 FROM a ORDER BY id",
                "1/90 2/40 3/50 4/NULL 5/NULL 6/100 7/90 8/NULL 9/80 10/110",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.k) + \
                 (SELECT count(*) FROM b WHERE b.g = a.g AND b.active = 1) FROM a ORDER BY id",
                "1/7 2/6 3/3 4/4 5/2 6/5 7/5 8/1 9/6 10/5",
            ),
            (
                "SELECT id FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.k AND \
                 EXISTS (SELECT 1 FROM a y WHERE y.k = b.k AND y.id > a.id)) ORDER BY id",
                "1",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.k AND \
                 (SELECT count(*) FROM a y WHERE y.k = b.k AND y.g = b.g) > 0) \
                 FROM a ORDER BY id",
                "1/3 2/2 3/1 4/0 5/0 6/0 7/3 8/0 9/1 10/1",
            ),
            (
                "SELECT id FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k AND \
                 NOT EXISTS (SELECT 1 FROM a y WHERE y.k = b.k AND y.g = a.g)) ORDER BY id",
                "1 2 3 4 5 6 7 8 9 10",
            ),
            (
                "SELECT id, (SELECT count(*) FROM \
                 (SELECT k AS c1, x * 0.5 AS c2 FROM b WHERE x * 0.5) AS p WHERE p.c1 = a.k) \
                 FROM a ORDER BY id",
                "1/3 2/2 3/1 4/0 5/0 6/1 7/3 8/0 9/2 10/1",
            ),
        ],
    );
}

/// A key compared under an affinity, and a column with a collation, give a scan's answers.
///
/// `b.k` is an `INT` column and `a.s` holds text, so `'7'` equals 7 under the integer affinity
/// the comparison applies. `b.name` is `NOCASE`, so its equality is not one of bytes and the
/// block is read by scanning.
#[test]
fn a_key_under_an_affinity_or_a_collation_answers_what_a_scan_answers() {
    let database = fixture("affinity");
    let connection = database.session();
    assert_both_ways(
        &connection,
        &[
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.s) FROM a ORDER BY id",
                "1/3 2/2 3/1 4/0 5/2 6/1 7/3 8/0 9/2 10/1",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.name = a.nm) FROM a ORDER BY id",
                "1/2 2/3 3/2 4/0 5/2 6/1 7/2 8/0 9/2 10/3",
            ),
        ],
    );
}

/// A block over the table an `UPDATE` changes sees the rows as they are when it is asked.
#[test]
fn a_block_over_the_table_an_update_changes_answers_what_a_scan_answers() {
    for setting in ["ON", "OFF"] {
        let database = fixture(&format!("update-{setting}"));
        let connection = database.session();
        connection
            .execute_batch(&format!("PRAGMA automatic_index = {setting};"))
            .expect("the pragma runs");
        connection
            .execute_batch("UPDATE a SET pad = (SELECT count(*) FROM a y WHERE y.k = a.k);")
            .expect("the update runs");
        assert_eq!(
            rendered(&connection, "SELECT id, pad FROM a ORDER BY id"),
            "1/2 2/1 3/1 4/0 5/1 6/1 7/2 8/1 9/1 10/1",
            "automatic_index = {setting}"
        );
    }
}

/// Rows a trigger writes during an `UPDATE` are seen by the next row's subquery.
///
/// `b` holds three rows for each of the ten keys, and the trigger adds one row to `b` for each
/// row of `a` updated. SQLite sets `pad` to 3 for ids 1 to 10, 4 for 11 to 20 and 5 for 21 to
/// 30, because the ids 11 to 20 are updated after one more row of each key exists. A table
/// read before the first row would answer 3 for all of them.
#[test]
fn a_trigger_s_writes_are_seen_by_the_rows_after_them() {
    let expected: String = (1..=30)
        .map(|id| format!("{id}/{}", 3 + (id - 1) / 10))
        .collect::<Vec<_>>()
        .join(" ");
    for setting in ["ON", "OFF"] {
        let database = trigger_fixture(&format!("trigger-{setting}"), true);
        let connection = database.session();
        connection
            .execute_batch(&format!("PRAGMA automatic_index = {setting};"))
            .expect("the pragma runs");
        connection
            .execute_batch("UPDATE a SET pad = (SELECT count(*) FROM b WHERE b.k = a.k);")
            .expect("the update runs");
        assert_eq!(
            rendered(&connection, "SELECT id, pad FROM a ORDER BY id"),
            expected,
            "automatic_index = {setting}, with the trigger"
        );
    }
    let flat: String = (1..=30)
        .map(|id| format!("{id}/3"))
        .collect::<Vec<_>>()
        .join(" ");
    let database = trigger_fixture("no-trigger", false);
    let connection = database.session();
    connection
        .execute_batch("UPDATE a SET pad = (SELECT count(*) FROM b WHERE b.k = a.k);")
        .expect("the update runs");
    assert_eq!(
        rendered(&connection, "SELECT id, pad FROM a ORDER BY id"),
        flat,
        "without the trigger"
    );
}

/// Builds `a` and `b` for the trigger case.
///
/// @param name - the test's name, which names its file
/// @param with_trigger - whether `AFTER UPDATE ON a` inserts `new.k` into `b`
fn trigger_fixture(name: &str, with_trigger: bool) -> Database {
    let area = workspace_root().join("target/scratch/automatic-index-answers");
    let _ = std::fs::create_dir_all(&area);
    let path = area.join(format!("{name}.rdb"));
    inillucent_base::testing::remove_database(&path);
    let database = Database::open(&path).expect("a fresh database opens");
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE a(id INTEGER PRIMARY KEY, k INT, pad INT); \
             CREATE TABLE b(id INTEGER PRIMARY KEY, k INT); \
             INSERT INTO a SELECT value, value % 10, 0 FROM generate_series(1, 30); \
             INSERT INTO b(k) SELECT value % 10 FROM generate_series(1, 30);",
        )
        .expect("the tables load");
    if with_trigger {
        connection
            .execute_batch(
                "CREATE TRIGGER grow AFTER UPDATE ON a BEGIN INSERT INTO b(k) VALUES (new.k); END;",
            )
            .expect("the trigger is made");
    }
    database
}

/// An outer join whose condition has more than the key gives a scan's rows.
///
/// A row of `a` whose partners all fail the rest of the condition is null extended, a `RIGHT`
/// join keeps the rows of `b` nothing matched, and a `FULL` join keeps both.
#[test]
fn an_outer_join_with_more_than_a_key_answers_what_a_scan_answers() {
    let database = fixture("outer");
    let connection = database.session();
    assert_both_ways(
        &connection,
        &[
            (
                "SELECT ifnull(a.id, '-'), ifnull(b.id, '-') FROM a \
                 LEFT JOIN b ON a.k = b.k AND b.active = 1 ORDER BY a.id, b.id",
                "1/1 1/9 2/3 3/5 4/- 5/- 6/10 7/1 7/9 8/- 9/7 10/-",
            ),
            (
                "SELECT a.id FROM a LEFT JOIN b ON a.k = b.k AND a.id < b.id \
                 WHERE b.id IS NULL ORDER BY a.id",
                "4 5 8 9",
            ),
            (
                "SELECT ifnull(a.id, '-'), ifnull(b.id, '-') FROM a \
                 LEFT JOIN b ON a.k = b.k AND a.id < b.id ORDER BY a.id, b.id",
                "1/2 1/9 2/3 2/4 3/5 4/- 5/- 6/10 7/9 8/- 9/- 10/11",
            ),
            (
                "SELECT ifnull(a.id, '-'), ifnull(b.id, '-') FROM a \
                 RIGHT JOIN b ON a.k = b.k AND b.active = 1 ORDER BY b.id, a.id",
                "1/1 7/1 -/2 2/3 -/4 3/5 -/6 9/7 -/8 1/9 7/9 6/10 -/11",
            ),
            (
                "SELECT ifnull(a.id, '-'), ifnull(b.id, '-') FROM a \
                 FULL JOIN b ON a.k = b.k AND b.active = 1 ORDER BY a.id, b.id",
                "-/2 -/4 -/6 -/8 -/11 1/1 1/9 2/3 3/5 4/- 5/- 6/10 7/1 7/9 8/- 9/7 10/-",
            ),
            (
                "SELECT ifnull(a.id, '-'), ifnull(b.id, '-') FROM a \
                 FULL JOIN b ON a.k = b.k AND a.id < b.id ORDER BY a.id, b.id",
                "-/1 -/6 -/7 -/8 1/2 1/9 2/3 2/4 3/5 4/- 5/- 6/10 7/9 8/- 9/- 10/11",
            ),
            (
                "SELECT a.id, b.id FROM a JOIN b ON a.k = b.k AND a.id < b.id \
                 ORDER BY a.id, b.id",
                "1/2 1/9 2/3 2/4 3/5 6/10 7/9 10/11",
            ),
        ],
    );
}

/// A correlated block keyed on an expression of the enclosing row answers what a scan answers.
///
/// The block computes the expression once for the outer row and looks its value up in the table.
/// The cases are an arithmetic key, a key that is NULL because the outer column is, a function,
/// a key compared under the integer affinity of `b.k`, a composite key, a `CASE`, a bucket that a
/// filter on `b` narrows, and a NOCASE column, which is not keyed. An expression that fails is
/// left to the scan, which fails on the same row.
#[test]
fn a_correlated_block_keyed_on_an_outer_expression_answers_what_a_scan_answers() {
    let database = fixture("outer-expression");
    let connection = database.session();
    assert_both_ways(
        &connection,
        &[
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.k + 1) FROM a ORDER BY id",
                "1/2 2/1 3/1 4/0 5/1 6/2 7/2 8/0 9/0 10/0",
            ),
            (
                "SELECT id FROM a WHERE EXISTS (SELECT 1 FROM b WHERE b.k = a.g * 2) ORDER BY id",
                "1 2 3 4 5 6 7 8 9 10",
            ),
            (
                "SELECT id FROM a WHERE NOT EXISTS (SELECT 1 FROM b WHERE b.k = a.k - 1) \
                 ORDER BY id",
                "1 4 6 7 8",
            ),
            (
                "SELECT id, (SELECT max(x) FROM b WHERE b.k = abs(a.k) + 0) FROM a ORDER BY id",
                "1/90 2/40 3/50 4/NULL 5/NULL 6/100 7/90 8/NULL 9/80 10/110",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.s || '') FROM a ORDER BY id",
                "1/3 2/2 3/1 4/0 5/2 6/1 7/3 8/0 9/2 10/1",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.k + 0 AND b.g = a.g) \
                 FROM a ORDER BY id",
                "1/1 2/2 3/1 4/0 5/0 6/0 7/2 8/0 9/1 10/1",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.x = a.k * 10) FROM a ORDER BY id",
                "1/1 2/1 3/1 4/0 5/1 6/1 7/1 8/1 9/1 10/1",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.name = lower(a.nm)) \
                 FROM a ORDER BY id",
                "1/2 2/3 3/2 4/0 5/2 6/1 7/2 8/0 9/2 10/3",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE \
                 b.k = CASE WHEN a.g = 2 THEN a.k + 1 ELSE a.k END) FROM a ORDER BY id",
                "1/3 2/2 3/1 4/0 5/1 6/1 7/2 8/0 9/2 10/1",
            ),
            (
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = a.k + 1 AND b.active = 1) \
                 FROM a ORDER BY id",
                "1/1 2/1 3/0 4/0 5/1 6/1 7/1 8/0 9/0 10/0",
            ),
        ],
    );
    for setting in ["ON", "OFF"] {
        connection
            .execute_batch(&format!("PRAGMA automatic_index = {setting};"))
            .expect("the pragma runs");
        let failed = connection
            .query(
                "SELECT id, (SELECT count(*) FROM b WHERE b.k = \
                 CASE WHEN a.id = 4 THEN abs(-9223372036854775808) ELSE a.k END) \
                 FROM a ORDER BY id",
            )
            .expect_err("the fourth row's key overflows");
        assert_eq!(
            failed.message(),
            "integer overflow",
            "automatic_index = {setting}"
        );
    }
}

/// A key side that is an expression gives a scan's rows.
#[test]
fn an_expression_key_answers_what_a_scan_answers() {
    let database = fixture("expression");
    let connection = database.session();
    assert_both_ways(
        &connection,
        &[
            (
                "SELECT a.id, b.id FROM a JOIN b ON a.k = b.k + 1 ORDER BY a.id, b.id",
                "2/1 2/2 2/9 3/3 3/4 5/11 9/10 10/5",
            ),
            (
                "SELECT a.id, b.id FROM a JOIN b ON a.k = abs(b.k) ORDER BY a.id, b.id",
                "1/1 1/2 1/9 2/3 2/4 3/5 6/10 7/1 7/2 7/9 9/7 9/8 10/11",
            ),
            (
                "SELECT a.id, b.id FROM a JOIN b ON lower(a.nm) = lower(b.name) \
                 ORDER BY a.id, b.id",
                "1/1 1/2 2/3 2/4 2/11 3/5 3/8 5/5 5/8 6/9 7/1 7/2 9/6 9/7 10/3 10/4 10/11",
            ),
            (
                "SELECT ifnull(a.id, '-'), ifnull(b.id, '-') FROM a \
                 LEFT JOIN b ON a.k = b.k + 1 ORDER BY a.id, b.id",
                "1/- 2/1 2/2 2/9 3/3 3/4 4/- 5/11 6/- 7/- 8/- 9/10 10/5",
            ),
        ],
    );
}

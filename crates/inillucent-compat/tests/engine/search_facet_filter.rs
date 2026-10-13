//! Facet filters applied inside a search, before its `k` rows are chosen.
//!
//! Invariant: on an `inillucent_search` table, a search whose `WHERE` clause
//! tests a facet with `=`, `IN`, `>`, `>=`, `<`, `<=` or `BETWEEN` returns
//! `k` rows that pass the test, or every passing row when fewer than `k` exist.
//!
//! Only `=` used to be applied inside the search. `IN` and the range operators
//! were applied by the engine to the `k` rows the search had already chosen,
//! so `day >= '2026-06-01' AND k = 1` returned nothing whenever the single best
//! row was older, while newer matching rows existed. The report came from an
//! application filtering a Slack archive by date and by several channels.

use std::collections::BTreeSet;

use inillucent_compat::differential::start_inillucent;
use inillucent_compat::rendering::datum_text as render;
use inillucent_engine::connect::Connection;

/// Where this suite's scratch databases live.
const AREA: &str = "search-facet-filter";

/// How many rows the corpus holds.
const ROWS: i64 = 60;

/// Runs a statement for its effect.
///
/// @param connection - the database
/// @param sql - one or more statements
fn exec(connection: &Connection<'_>, sql: &str) {
    connection
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
}

/// Returns the first column of every row, rendered as text.
///
/// @param connection - the database
/// @param sql - the query
fn column(connection: &Connection<'_>, sql: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
    let mut out = Vec::new();
    while statement
        .step()
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()))
    {
        if let Some(first) = statement.row().first() {
            out.push(render(first));
        }
    }
    out
}

/// Creates the three row table from the report, with a vector width of four.
///
/// @param connection - the database
fn seed_report(connection: &Connection<'_>) {
    exec(
        connection,
        "CREATE VIRTUAL TABLE s USING inillucent_search(text, day FACET, ch FACET, dims = 4, \
         fusion = 'rrf');
         INSERT INTO s (text, day, ch) VALUES ('budget approved', '2026-01-05', 'C1');
         INSERT INTO s (text, day, ch) VALUES ('budget denied', '2026-09-05', 'C2');
         INSERT INTO s (text, day, ch) VALUES ('budget pending', '2026-10-01', 'C1');",
    );
}

/// Creates a corpus where every row matches the word `report` and carries a
/// day, a channel and a vector, so every facet test has rows on both sides.
///
/// Row `n` is on day `2026-01-01` plus `n` days, written as text, and on
/// channel `C` followed by `n % 7`. The rows that rank first lexically are the
/// ones with the most repetitions of `report`, which are spread across every
/// day and channel, so a filter applied after the ranking would lose rows.
///
/// @param connection - the database
fn seed_corpus(connection: &Connection<'_>) {
    exec(
        connection,
        "CREATE VIRTUAL TABLE c USING inillucent_search(text, day FACET, ch FACET, dims = 4)",
    );
    for n in 0..ROWS {
        let repeats = "report ".repeat(1 + (n * 13 % 5) as usize);
        let day = format!("2026-{:02}-{:02}", 1 + n / 28, 1 + n % 28);
        let vector = format!("[{}, {}, 1, 0]", (n % 5) as f64 / 5.0, (n % 3) as f64 / 3.0);
        exec(
            connection,
            &format!(
                "INSERT INTO c (rowid, text, day, ch, vector) VALUES \
                 ({n}, '{repeats} number {n}', '{day}', 'C{}', '{vector}')",
                n % 7
            ),
        );
    }
}

/// The report's six queries, and the rows each has to return.
#[test]
fn the_reported_queries_return_matching_rows() {
    let connection = start_inillucent(AREA, "report");
    seed_report(&connection);
    let cases: [(&str, &[&str]); 9] = [
        ("ch = 'C2' AND k = 1", &["2"]),
        ("day = '2026-10-01' AND k = 1", &["3"]),
        ("ch IN ('C2') AND k = 1", &["2"]),
        ("day >= '2026-06-01' AND k = 5", &["2", "3"]),
        ("day < '2026-06-01' AND k = 1", &["1"]),
        ("day <= '2026-01-05' AND k = 1", &["1"]),
        ("ch IN ('zz') AND k = 1", &[]),
        ("day >= '2026-06-01' AND ch IN ('C1') AND k = 1", &["3"]),
        (
            "day BETWEEN '2026-06-01' AND '2026-09-30' AND k = 1",
            &["2"],
        ),
    ];
    for (predicate, expected) in cases {
        let sql =
            format!("SELECT rowid FROM s WHERE s MATCH 'budget' AND {predicate} ORDER BY rowid");
        assert_eq!(column(&connection, &sql), expected, "{predicate}");
    }
    // The report said "row 2 or 3" for these two: one row, from the range.
    for predicate in [
        "day >= '2026-06-01' AND k = 1",
        "day > '2026-06-01' AND k = 1",
    ] {
        let sql = format!("SELECT day FROM s WHERE s MATCH 'budget' AND {predicate}");
        let days = column(&connection, &sql);
        assert_eq!(days.len(), 1, "{predicate} returned {days:?}");
        assert!(
            days.iter().all(|day| day.as_str() >= "2026-06-01"),
            "{predicate} returned {days:?}"
        );
    }
}

/// Every facet test, at several `k`, returns `k` passing rows or every
/// passing row, by keyword, by vector and by both.
///
/// The rows that pass are found by a scan of the table, which the engine
/// evaluates itself, so the search is graded against SQL's own answer.
#[test]
fn every_facet_test_returns_k_passing_rows() {
    let connection = start_inillucent(AREA, "corpus");
    seed_corpus(&connection);
    let predicates = [
        "ch = 'C3'",
        "ch IN ('C1', 'C5')",
        "ch IN ('C6', 'nowhere')",
        "day >= '2026-02-10'",
        "day > '2026-02-10'",
        "day < '2026-01-09'",
        "day <= '2026-01-09'",
        "day BETWEEN '2026-01-20' AND '2026-02-03'",
        "day >= '2026-01-20' AND ch IN ('C0', 'C2', 'C4')",
        "ch IN (SELECT 'C' || 2)",
    ];
    let searches = [
        "c MATCH 'report'",
        "vector = '[0, 1, 1, 0]'",
        "c MATCH 'report' AND vector = '[0.8, 0, 1, 0]'",
    ];
    for predicate in predicates {
        let passing: BTreeSet<String> = column(
            &connection,
            &format!("SELECT rowid FROM c WHERE {predicate}"),
        )
        .into_iter()
        .collect();
        assert!(!passing.is_empty(), "{predicate} passes no row at all");
        for search in searches {
            for k in [1usize, 3, 10] {
                let sql = format!("SELECT rowid FROM c WHERE {search} AND {predicate} AND k = {k}");
                let found = column(&connection, &sql);
                assert_eq!(
                    found.len(),
                    k.min(passing.len()),
                    "{sql} returned {found:?}, and {} rows pass",
                    passing.len()
                );
                for row in &found {
                    assert!(passing.contains(row), "{sql} returned row {row}");
                }
            }
        }
    }
}

/// An `IN` list in a search runs as one search, so `k` counts the whole
/// answer and `ORDER BY rank` holds across every value of the list.
#[test]
fn an_in_list_is_one_search_in_rank_order() {
    let connection = start_inillucent(AREA, "ranked");
    seed_corpus(&connection);
    let ranked = column(
        &connection,
        "SELECT rank FROM c WHERE c MATCH 'report' AND ch IN ('C1', 'C2', 'C3') AND k = 6 \
         ORDER BY rank",
    );
    assert_eq!(ranked.len(), 6, "k counts the whole answer: {ranked:?}");
    let ranks: Vec<f64> = ranked
        .iter()
        .map(|rank| rank.parse::<f64>().expect("a rank is a number"))
        .collect();
    assert!(
        ranks.windows(2).all(|pair| pair[0] <= pair[1]),
        "not in rank order: {ranks:?}"
    );
}

/// A bound parameter, an integer, a NULL and an empty list each select what
/// SQL selects for a `TEXT` column.
#[test]
fn a_facet_test_reads_its_values_as_sql_does() {
    let connection = start_inillucent(AREA, "values");
    exec(
        &connection,
        "CREATE VIRTUAL TABLE n USING inillucent_search(text, num FACET, dims = 2);
         INSERT INTO n (text, num) VALUES ('alpha one', 1), ('alpha two', 2), \
         ('alpha ten', 10);",
    );
    let found = |predicate: &str| {
        column(
            &connection,
            &format!(
                "SELECT num FROM n WHERE n MATCH 'alpha' AND {predicate} AND k = 5 ORDER BY num"
            ),
        )
    };
    assert_eq!(found("num IN (1, 10)"), vec!["1", "10"]);
    assert_eq!(found("num IN ('2')"), vec!["2"]);
    // Text affinity compares '10' and '2' as text, where '10' sorts first.
    assert_eq!(found("num > 2"), Vec::<String>::new());
    assert_eq!(found("num < '2'"), vec!["1", "10"]);
    assert_eq!(found("num IN (NULL)"), Vec::<String>::new());
    assert_eq!(found("num >= NULL"), Vec::<String>::new());

    let mut statement = connection
        .prepare("SELECT num FROM n WHERE n MATCH 'alpha' AND num IN (?1, ?2) AND k = 1")
        .unwrap_or_else(|error| panic!("{}", error.message()));
    statement
        .bind_integer(1, 2)
        .and_then(|()| statement.bind_null(2))
        .unwrap_or_else(|error| panic!("{}", error.message()));
    let mut rows = Vec::new();
    while statement
        .step()
        .unwrap_or_else(|error| panic!("{}", error.message()))
    {
        rows.extend(statement.row().first().map(render));
    }
    assert_eq!(rows, vec!["2"], "a bound list ran as one search");
}

/// The same tests outside a search are evaluated by the engine, and select
/// the same rows they always did.
#[test]
fn a_facet_test_outside_a_search_is_unchanged() {
    let connection = start_inillucent(AREA, "scan");
    seed_report(&connection);
    let cases: [(&str, &[&str]); 5] = [
        ("ch IN ('C1')", &["1", "3"]),
        ("ch NOT IN ('C1')", &["2"]),
        ("day >= '2026-06-01'", &["2", "3"]),
        ("ch IN (1, 'C2')", &["2"]),
        ("rowid IN (1, 3)", &["1", "3"]),
    ];
    for (predicate, expected) in cases {
        let sql = format!("SELECT rowid FROM s WHERE {predicate} ORDER BY rowid");
        assert_eq!(column(&connection, &sql), expected, "{predicate}");
    }
    exec(&connection, "DELETE FROM s WHERE ch IN ('C2')");
    assert_eq!(
        column(&connection, "SELECT rowid FROM s ORDER BY rowid"),
        vec!["1", "3"]
    );
}

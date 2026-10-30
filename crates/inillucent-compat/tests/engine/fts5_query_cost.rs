//! What an FTS5 search costs for each row it matches, and where the auxiliary functions may be used.
//!
//! Invariant: **the time of a search grows in proportion to the rows it matches, and scoring a row
//! does not look at the other rows.** `bm25` used to copy the position maps of every phrase of the
//! query for each row it scored, so scoring 20,000 matching rows took 40 s in a debug build and 7 s
//! in a release build. `OR`, `AND` and `NOT` looked for each row of one side in a list of the other,
//! which cost the product of the two lists.
//!
//! The bounds are generous, because the check is of the growth and not of the speed: they are many
//! times what the linear cost takes in a debug build, and well below what the quadratic cost took.

use std::time::{Duration, Instant};

use inillucent_compat::facade::{Connection, Database};

/// Where this suite's scratch databases live.
const AREA: &str = "fts5-query-cost";

/// How many rows the table holds for the `bm25` check.
const ROWS: usize = 20_000;

/// How many rows the table holds for the checks of `AND`, `OR` and `NOT`, whose old cost was the
/// product of the two lists and so needs a longer table to show.
const MANY_ROWS: usize = 60_000;

/// The longest any of the searches here may take.
const BOUND: Duration = Duration::from_secs(8);

/// Returns a database file of this test's own, under the gitignored root.
fn scratch() -> std::path::PathBuf {
    let root = inillucent_compat::workspace_root()
        .join("_agent_output")
        .join(AREA);
    let _ = std::fs::create_dir_all(&root);
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = root.join(format!("{}-{serial}.rdb", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

/// Creates the table and loads it: every row holds `common`, and `w5` is in about one in 97.
///
/// @param connection - a connection to an empty database
/// @param rows - how many rows to load
fn load(connection: &Connection, rows: usize) {
    connection
        .execute("CREATE VIRTUAL TABLE f USING fts5(a, b)")
        .expect("creates the table");
    connection
        .execute(&format!(
            "WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n+1 FROM c WHERE n<{rows}) \
             INSERT INTO f(rowid, a, b) SELECT n, 'w'||(n%97)||' w'||(n%1013)||' common word'||(n%5), \
             'title '||(n%53) FROM c"
        ))
        .expect("fills the table");
}

/// Runs a query and returns how many rows it gave and how long it took.
///
/// @param connection - the connection
/// @param sql - the query
fn timed(connection: &Connection, sql: &str) -> (usize, Duration) {
    let started = Instant::now();
    let rows = connection.query(sql).expect("the query runs");
    (rows.len(), started.elapsed())
}

/// `bm25` over every matching row costs the arithmetic and the row's own size, not the other rows.
#[test]
fn bm25_over_every_matching_row_is_linear() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    load(&connection, ROWS);
    let (rows, took) = timed(
        &connection,
        "SELECT rowid, bm25(f) FROM f WHERE f MATCH 'common'",
    );
    assert_eq!(rows, ROWS);
    assert!(
        took < BOUND,
        "bm25 over {ROWS} matching rows took {took:?}; copying every phrase's positions for each \
         row took 40 s at this size, and scoring a row from its own positions takes a fraction of one"
    );
}

/// A ranked search with a limit scores each matching row once and combines the lists by merging.
#[test]
fn a_ranked_search_over_an_or_is_linear() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    load(&connection, MANY_ROWS);
    let (rows, took) = timed(
        &connection,
        "SELECT rowid FROM f WHERE f MATCH 'common OR word1 OR word2 OR w5' ORDER BY rank LIMIT 10",
    );
    assert_eq!(rows, 10);
    assert!(
        took < BOUND,
        "a ranked search over an OR of four words and {MANY_ROWS} rows took {took:?}; looking for \
         each row of one side in a list of the other took the product of the two lists"
    );
}

/// `AND` and `NOT` over two common words combine by merging the two lists of rows.
#[test]
fn and_and_not_over_common_words_are_linear() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    load(&connection, MANY_ROWS);
    for sql in [
        "SELECT rowid FROM f WHERE f MATCH 'common AND word1'",
        "SELECT rowid FROM f WHERE f MATCH 'common NOT word1'",
        "SELECT rowid FROM f WHERE f MATCH 'common OR word1 OR word2 OR word3'",
    ] {
        let (rows, took) = timed(&connection, sql);
        assert!(rows > 0, "{sql} found nothing");
        assert!(took < BOUND, "{sql} took {took:?} over {MANY_ROWS} rows");
    }
}

/// `highlight`, `snippet` and `bm25` need a row of the table to read, so an aggregate refuses them.
///
/// SQLite answers `unable to use function highlight in the requested context` for
/// `max(highlight(f, ...))`, because inside an aggregate there is no row of the virtual table. The
/// call used to be accepted and answered with the text of whichever row came last.
#[test]
fn an_auxiliary_function_inside_an_aggregate_is_refused() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE f USING fts5(a)")
        .expect("creates the table");
    connection
        .execute("INSERT INTO f(rowid, a) VALUES (1, 'one two')")
        .expect("inserts");
    for (name, sql) in [
        (
            "highlight",
            "SELECT max(highlight(f, 0, '[', ']')) FROM f WHERE f MATCH 'one'",
        ),
        (
            "snippet",
            "SELECT group_concat(snippet(f, 0, '[', ']', '..', 3)) FROM f WHERE f MATCH 'one'",
        ),
        ("bm25", "SELECT sum(bm25(f)) FROM f WHERE f MATCH 'one'"),
    ] {
        let said = connection.query(sql).expect_err(sql).message().to_string();
        assert_eq!(
            said,
            format!("unable to use function {name} in the requested context")
        );
    }
    let plain = connection
        .query("SELECT highlight(f, 0, '[', ']') FROM f WHERE f MATCH 'one'")
        .expect("a call on its own row still works");
    assert_eq!(plain.len(), 1);
}

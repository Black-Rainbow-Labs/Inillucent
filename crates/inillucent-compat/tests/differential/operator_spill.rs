//! `DISTINCT`, compound selects and window functions over more rows than their operators hold,
//! graded against pinned SQLite 3.53.4.
//!
//! Invariant: **an operator that sorts in runs, or computes a partition in pieces, answers exactly
//! what it answers when it holds everything: the same rows, in the same order, with the same
//! values.** Each of these operators held every row in a vector and now writes sorted runs to a
//! temporary file (`DISTINCT`, `UNION`, `INTERSECT`, `EXCEPT`), or computes a partition a run of
//! peer groups at a time (a window), once it holds 32 MiB. The cases lower that to 24 KiB, so
//! 20,000 rows reach the runs and the pieces, and check that they did. Without the lowered limit
//! every case here would be graded in memory and prove nothing about them.
//!
//! The rows are built to be hard for a sort in runs: keys that repeat across runs, integers and
//! reals that compare equal, text that a collation folds, NULLs, and an ordering value that repeats
//! so peer groups fall on the edge of a piece.

use inillucent_compat::differential::{self, Step};

/// How many bytes an operator holds before it moves to runs.
const LIMIT: u64 = 24 * 1024;

/// Serialises the cases, which share process wide counters.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The table the cases read.
const TABLE: &str =
    "CREATE TABLE t(id INTEGER PRIMARY KEY, g INTEGER, a INTEGER, b TEXT COLLATE NOCASE, c REAL)";

/// Fills [`TABLE`] with 20,000 rows.
///
/// `g` has 7 values, `a` repeats about 12 times, `b` mixes cases over 600 values, and `c` is
/// NULL on every 13th row and otherwise a real that sometimes equals an integer in `a`.
const FILL: &str =
    "WITH RECURSIVE s(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM s WHERE x < 20000) \
     INSERT INTO t SELECT x, x % 7, (x * 7919) % 1700, \
     CASE WHEN x % 2 = 0 THEN 'Key ' ELSE 'key ' END || ((x * 31) % 600), \
     CASE WHEN x % 13 = 0 THEN NULL ELSE ((x * 17) % 1700) * 1.0 END FROM s";

/// Runs a scenario against both engines and compares every reply.
///
/// @param name - the scenario's name, which names its database files
/// @param steps - the statements, after the table is built
/// @param runs - how many runs at least must have been written
/// @param pieces - how many pieces of a window partition at least must have been computed
fn compare(name: &str, steps: &[Step], runs: u64, pieces: u64) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inillucent_exec::spill::set_run_bytes_override(LIMIT);
    let (runs_before, pieces_before) = (
        inillucent_exec::spill::runs_written(),
        inillucent_exec::window::pieces_computed(),
    );
    let mut all = vec![Step::Exec(TABLE), Step::Exec(FILL)];
    all.extend_from_slice(steps);
    let compared = differential::compare("operator_spill", name, &all);
    assert!(
        compared == 0 || compared == all.len(),
        "compared {compared} of {} steps",
        all.len()
    );
    if compared == 0 {
        return;
    }
    let wrote = inillucent_exec::spill::runs_written().saturating_sub(runs_before);
    let computed = inillucent_exec::window::pieces_computed().saturating_sub(pieces_before);
    assert!(
        wrote >= runs,
        "{wrote} runs were written, and this case needs {runs}"
    );
    assert!(
        computed >= pieces,
        "{computed} pieces were computed, and this case needs {pieces}"
    );
}

/// `DISTINCT` keeps the first row of each key, in the order the keys first arrive.
#[test]
fn distinct_in_runs_matches_sqlite() {
    compare(
        "distinct",
        &[
            Step::Query(
                "SELECT count(*), sum(a), sum(length(b)) FROM (SELECT DISTINCT a, b FROM t)",
            ),
            Step::Query("SELECT DISTINCT a, b FROM t ORDER BY a, b LIMIT 25 OFFSET 3000"),
            Step::Query("SELECT DISTINCT b FROM t LIMIT 40"),
            Step::Query("SELECT DISTINCT a, c FROM t LIMIT 60"),
            Step::Query("SELECT count(*) FROM (SELECT DISTINCT id % 5000, b, c FROM t)"),
        ],
        2,
        0,
    );
}

/// `UNION`, `INTERSECT`, `EXCEPT` and `UNION ALL`, alone and chained, in runs.
#[test]
fn compound_selects_in_runs_match_sqlite() {
    compare(
        "compound",
        &[
            Step::Query("SELECT count(*), sum(a) FROM (SELECT a, b FROM t WHERE id <= 12000 UNION SELECT a, b FROM t WHERE id > 8000)"),
            Step::Query("SELECT a, b FROM t WHERE id <= 12000 UNION SELECT a, b FROM t WHERE id > 8000 ORDER BY 1, 2 LIMIT 30 OFFSET 900"),
            Step::Query("SELECT a, b FROM t WHERE id <= 12000 INTERSECT SELECT a, b FROM t WHERE id > 8000 ORDER BY 1, 2 LIMIT 30"),
            Step::Query("SELECT a, b FROM t WHERE id <= 12000 EXCEPT SELECT a, b FROM t WHERE id > 8000 ORDER BY 1, 2 LIMIT 30"),
            Step::Query("SELECT count(*) FROM (SELECT a, b FROM t EXCEPT SELECT a, b FROM t WHERE g = 3)"),
            Step::Query("SELECT count(*) FROM (SELECT a, b FROM t INTERSECT SELECT a, b FROM t WHERE g IN (1, 2))"),
            Step::Query("SELECT count(*) FROM (SELECT a FROM t UNION ALL SELECT a FROM t WHERE g = 1)"),
            // The unordered answer comes out in key order.
            Step::Query("SELECT a, c FROM t WHERE id % 40 = 0 UNION SELECT a, c FROM t WHERE id % 60 = 0 LIMIT 50"),
            // The row kept for a key is the later arm's, so a real beats the integer it equals.
            Step::Query("SELECT typeof(a), count(*) FROM (SELECT a FROM t WHERE id <= 6000 UNION SELECT c FROM t WHERE id > 3000) GROUP BY 1"),
            Step::Query("SELECT typeof(x), count(*) FROM (SELECT c AS x FROM t WHERE id <= 6000 UNION SELECT a FROM t WHERE id > 3000) GROUP BY 1"),
            // Three arms, folded left to right.
            Step::Query("SELECT count(*), sum(a) FROM (SELECT a, b FROM t WHERE g < 3 UNION SELECT a, b FROM t WHERE g > 4 EXCEPT SELECT a, b FROM t WHERE id % 3 = 0)"),
            Step::Query("SELECT count(*) FROM (SELECT a, b FROM t WHERE g < 4 INTERSECT SELECT a, b FROM t WHERE g > 1 UNION SELECT a, b FROM t WHERE g = 6)"),
        ],
        6,
        0,
    );
}

/// Running and ranking window functions over one partition larger than the limit, and over many.
#[test]
fn windows_computed_in_pieces_match_sqlite() {
    compare(
        "window-pieces",
        &[
            Step::Query("SELECT id, row_number() OVER (ORDER BY a, id) FROM t ORDER BY id LIMIT 12 OFFSET 9000"),
            Step::Query("SELECT max(n), sum(n) FROM (SELECT row_number() OVER (ORDER BY a, id) AS n FROM t)"),
            Step::Query("SELECT id, a, rank() OVER (ORDER BY a), dense_rank() OVER (ORDER BY a) FROM t ORDER BY id LIMIT 20 OFFSET 15000"),
            Step::Query("SELECT id, sum(c) OVER (ORDER BY a), count(c) OVER (ORDER BY a), max(c) OVER (ORDER BY a), min(b) OVER (ORDER BY a) FROM t ORDER BY id LIMIT 20 OFFSET 7000"),
            Step::Query("SELECT id, sum(a) OVER (ORDER BY id ROWS UNBOUNDED PRECEDING), avg(c) OVER (ORDER BY id ROWS BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM t ORDER BY id LIMIT 12 OFFSET 12000"),
            Step::Query("SELECT id, group_concat(g) FILTER (WHERE a % 2 = 0) OVER (ORDER BY id RANGE UNBOUNDED PRECEDING) FROM t WHERE id <= 300 ORDER BY id LIMIT 8 OFFSET 291"),
            Step::Query("SELECT id, g, row_number() OVER (PARTITION BY g ORDER BY a, id), sum(a) OVER (PARTITION BY g ORDER BY a) FROM t ORDER BY g, a, id LIMIT 20 OFFSET 11000"),
            Step::Query("SELECT id, row_number() OVER () FROM t ORDER BY id LIMIT 5 OFFSET 19000"),
            Step::Query("SELECT id, row_number() OVER (), rank() OVER () FROM t ORDER BY id LIMIT 5 OFFSET 100"),
            Step::Query("SELECT count(*) FROM (SELECT id, rank() OVER (ORDER BY b COLLATE NOCASE) AS r FROM t) WHERE r < 40"),
            Step::Query("SELECT id, sum(a) OVER (ORDER BY c DESC NULLS LAST) FROM t ORDER BY id LIMIT 10 OFFSET 5000"),
        ],
        0,
        10,
    );
}

/// Windows that need the whole partition, or the rows after the current one, still match SQLite
/// whatever the limit is, and a statement with several frames puts its rows back in order.
#[test]
fn windows_that_read_the_whole_partition_and_several_frames_match_sqlite() {
    compare(
        "window-whole",
        &[
            Step::Query("SELECT id, ntile(4) OVER (ORDER BY a, id), percent_rank() OVER (ORDER BY a), cume_dist() OVER (ORDER BY a) FROM t ORDER BY id LIMIT 15 OFFSET 8000"),
            Step::Query("SELECT id, lag(a) OVER (ORDER BY id), lead(a, 2, -1) OVER (ORDER BY id), first_value(a) OVER (ORDER BY id), last_value(a) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING) FROM t ORDER BY id LIMIT 10 OFFSET 19990"),
            Step::Query("SELECT id, sum(a) OVER (PARTITION BY g), count(*) OVER (PARTITION BY g, a % 3) FROM t ORDER BY id LIMIT 12 OFFSET 4000"),
            Step::Query("SELECT id, sum(a) OVER (ORDER BY id ROWS BETWEEN 2 PRECEDING AND 3 FOLLOWING) FROM t ORDER BY id LIMIT 10 OFFSET 10000"),
            Step::Query("SELECT id, count(DISTINCT a) OVER (ORDER BY id ROWS BETWEEN 50 PRECEDING AND CURRENT ROW) FROM t ORDER BY id LIMIT 8 OFFSET 777"),
            // Two frames, so the rows go through a sort by the second frame and a sort back.
            Step::Query("SELECT id, row_number() OVER (ORDER BY a, id), row_number() OVER (PARTITION BY g ORDER BY b, id DESC), sum(a) OVER (ORDER BY id) FROM t ORDER BY id LIMIT 15 OFFSET 13000"),
            Step::Query("SELECT id, row_number() OVER (ORDER BY a, id), rank() OVER (ORDER BY b) FROM t LIMIT 25 OFFSET 6000"),
            Step::Query("SELECT DISTINCT rank() OVER (PARTITION BY g ORDER BY a) FROM t"),
            Step::Query("SELECT count(*), sum(r) FROM (SELECT row_number() OVER (PARTITION BY a ORDER BY id) AS r FROM t)"),
        ],
        6,
        0,
    );
}

/// A window inside a compound arm, a window feeding an `INSERT ... SELECT`, and one that fails
/// part way.
#[test]
fn windows_in_other_statements_and_failing_windows_match_sqlite() {
    compare(
        "window-contexts",
        &[
            Step::Query("SELECT count(*), sum(n) FROM (SELECT row_number() OVER (ORDER BY id) AS n FROM t UNION SELECT a FROM t WHERE g = 2)"),
            Step::Exec("CREATE TABLE copy(id INTEGER PRIMARY KEY, n INTEGER, s INTEGER)"),
            Step::Exec("INSERT INTO copy SELECT id, row_number() OVER (ORDER BY a, id), sum(a) OVER (ORDER BY id) FROM t"),
            Step::Query("SELECT changes(), count(*), sum(n), sum(s) FROM copy"),
            // An overflow part way: SQLite answers the rows before it, then the error.
            Step::Exec("CREATE TABLE big(id INTEGER PRIMARY KEY, v INTEGER)"),
            Step::Exec("INSERT INTO big SELECT id, CASE WHEN id = 1500 THEN 9223372036854775807 ELSE id * 1000000000 END FROM t WHERE id <= 2000"),
            Step::Query("SELECT id, sum(v) OVER (ORDER BY id) FROM big LIMIT 3"),
            Step::Query("SELECT count(*) FROM (SELECT sum(v) OVER (ORDER BY id) FROM big WHERE id < 1000)"),
            Step::Query("PRAGMA integrity_check"),
        ],
        0,
        0,
    );
}

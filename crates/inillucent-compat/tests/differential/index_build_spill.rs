//! `CREATE INDEX` and `REINDEX` over entries that do not fit in the sort's memory, graded against
//! pinned SQLite 3.53.4.
//!
//! Invariant: **an index built from sorted runs written to a transient file holds exactly the
//! entries, in exactly the order, that an index sorted in memory holds, and a `UNIQUE` index that
//! cannot be built fails with SQLite's message.** The shipped run size is 32 MiB, far above any table
//! a differential case builds, so each case lowers it to 128 KiB. A table of 40,000 rows then writes
//! dozens of runs, and the merge, the chunked tree build and the duplicate check over the merge are
//! all reached. Without the lowered size every case here would sort in memory and prove nothing about
//! them.
//!
//! The cases use keys built to defeat the sort's 16 byte prefix (a long text prefix shared by every
//! row), NULLs, a descending column, a case insensitive collation, an expression, a partial index and
//! a `WITHOUT ROWID` table, because each of those takes a different path through the comparison the
//! merge makes.

use inillucent_compat::differential::{self, Step};

/// The run size the cases sort with, in bytes.
///
/// A run written at this size holds a few thousand entries.
const RUN_BYTES: u64 = 128 * 1024;

/// Lowers the sort's run size for every connection in this process.
///
/// Never raised again: a case that runs beside this one still gets right answers from a lower
/// size, only slower ones.
fn lowered() {
    inillucent_exec::spill::set_run_bytes_override(RUN_BYTES);
    // And the rows handed to the tree builder at a time, so that 40,000 entries cross a dozen chunk
    // boundaries and the leaf that ends each chunk is chained to the first leaf of the next.
    inillucent_exec::spill::set_chunk_rows_override(3_000);
}

/// The table every case indexes: 40,000 rows.
const TABLE: &str =
    "CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, b INTEGER, c REAL, d TEXT COLLATE NOCASE)";

/// Fills [`TABLE`].
///
/// `a` is unique and shares its first 24 bytes with every other row, so the sort's prefix ties on
/// every comparison and the values decide. `b` repeats and is NULL on every 13th row. `d` mixes
/// cases, so `NOCASE` merges `Key` and `key`.
const FILL: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 40000) \
     INSERT INTO t SELECT x, 'shared-prefix-of-24-by-' || (x * 7919 % 40009), \
     CASE WHEN x % 13 = 0 THEN NULL ELSE x * 31 % 977 END, x * 0.5, \
     CASE WHEN x % 2 = 0 THEN 'Key' ELSE 'key' END || (x % 3001) FROM c";

/// Runs a scenario against both engines and compares every reply.
///
/// @param name - the scenario's name, which names its database files
/// @param steps - the statements, after the table is built and filled
fn compare(name: &str, steps: &[Step]) {
    // One case at a time, so the count of runs written is this case's own.
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    lowered();
    let before = inillucent_exec::spill::runs_written();
    let mut all = vec![Step::Exec(TABLE), Step::Exec(FILL)];
    all.extend_from_slice(steps);
    let compared = differential::compare("index_build_spill", name, &all);
    assert!(
        compared == 0 || compared == all.len(),
        "compared {compared} of {} steps",
        all.len()
    );
    // **A case that did not spill proves nothing about the spill.** 40,000 rows of these entries
    // are several megabytes and the run size is 128 KiB.
    let written = inillucent_exec::spill::runs_written().saturating_sub(before);
    assert!(
        compared == 0 || written >= 4,
        "the case wrote {written} runs, so it did not reach the merge it is meant to grade"
    );
}

/// Serialises the cases, which share one process-wide run counter.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Plain, descending, collated and multi column indexes answer every question the way SQLite does.
#[test]
fn indexes_built_from_runs_answer_like_sqlite() {
    compare(
        "plain",
        &[
            Step::Exec("CREATE INDEX ia ON t(a)"),
            Step::Exec("CREATE INDEX ib ON t(b DESC, a)"),
            Step::Exec("CREATE INDEX id_nocase ON t(d)"),
            Step::Exec("CREATE INDEX ic ON t(c, d DESC, b)"),
            Step::Query("SELECT id FROM t ORDER BY a LIMIT 7 OFFSET 20000"),
            Step::Query("SELECT id, a FROM t WHERE a >= 'shared-prefix-of-24-by-3999' AND a < 'shared-prefix-of-24-by-4005' ORDER BY a"),
            Step::Query("SELECT id FROM t WHERE b = 500 ORDER BY a LIMIT 9"),
            Step::Query("SELECT b, count(*), min(a), max(a) FROM t WHERE b IS NULL OR b > 970 GROUP BY b ORDER BY b"),
            Step::Query("SELECT d, count(*) FROM t WHERE d = 'KEY17' GROUP BY d"),
            Step::Query("SELECT id FROM t WHERE c > 19990 AND c < 20000 ORDER BY c, d DESC LIMIT 12"),
            Step::Query("PRAGMA integrity_check"),
            // Rows written after the build go into the tree it made, on both sides of a chunk
            // boundary, and the index still agrees with the table.
            Step::Exec("INSERT INTO t VALUES (50001, 'shared-prefix-of-24-by-0', 1, 1.5, 'Key')"),
            Step::Exec("UPDATE t SET a = a || 'x' WHERE id % 997 = 0"),
            Step::Exec("DELETE FROM t WHERE id % 1009 = 0"),
            Step::Query("SELECT count(*), sum(id) FROM t WHERE a > 'shared-prefix-of-24-by-2'"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

/// An index on an expression and a partial index take the query path to their entries.
#[test]
fn expression_and_partial_indexes_built_from_runs_match_sqlite() {
    compare(
        "computed",
        &[
            Step::Exec("CREATE INDEX ie ON t(lower(d), b)"),
            Step::Exec("CREATE INDEX ip ON t(a) WHERE b > 900"),
            Step::Query("SELECT count(*) FROM t WHERE lower(d) = 'key5' AND b = 7"),
            Step::Query("SELECT id FROM t WHERE b > 900 AND a > 'shared-prefix-of-24-by-5' ORDER BY a LIMIT 10"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

/// A `UNIQUE` index over unique keys builds, and one over duplicates fails with SQLite's message
/// and leaves no index behind.
#[test]
fn a_unique_index_over_runs_is_checked_like_sqlite() {
    compare(
        "unique",
        &[
            Step::Exec("CREATE UNIQUE INDEX iu ON t(a)"),
            Step::Query("SELECT count(*) FROM t WHERE a = 'shared-prefix-of-24-by-123'"),
            // 977 values over 40,000 rows: duplicates in the first run and across runs.
            Step::Exec("CREATE UNIQUE INDEX ibad ON t(b)"),
            Step::Query("SELECT count(*) FROM sqlite_schema WHERE name IN ('iu', 'ibad')"),
            // NULLs are distinct, so an index whose only repeats are NULLs builds.
            Step::Exec("CREATE TABLE n(id INTEGER PRIMARY KEY, k INTEGER)"),
            Step::Exec(
                "INSERT INTO n SELECT id, CASE WHEN id % 2 = 0 THEN NULL ELSE id END FROM t",
            ),
            Step::Exec("CREATE UNIQUE INDEX nk ON n(k)"),
            Step::Query("SELECT count(*), count(k) FROM n"),
            // A duplicate that is only one entry apart in a table of many runs.
            Step::Exec("UPDATE n SET k = 3 WHERE id = 39999"),
            Step::Exec("DROP INDEX nk"),
            Step::Exec("CREATE UNIQUE INDEX nk ON n(k)"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

/// `REINDEX` of indexes whose entries span many runs rebuilds them to the same answers.
#[test]
fn reindex_over_runs_matches_sqlite() {
    compare(
        "reindex",
        &[
            Step::Exec("CREATE INDEX ia ON t(a)"),
            Step::Exec("CREATE INDEX ib ON t(b DESC, a)"),
            Step::Exec("DELETE FROM t WHERE id % 5 = 0"),
            Step::Exec("REINDEX"),
            Step::Query("SELECT count(*), sum(id) FROM t WHERE a > 'shared-prefix-of-24-by-2'"),
            Step::Query("SELECT id FROM t ORDER BY b DESC, a LIMIT 15 OFFSET 8000"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

/// A `WITHOUT ROWID` table's secondary index, whose entries end in the primary key columns.
#[test]
fn an_index_on_a_without_rowid_table_built_from_runs_matches_sqlite() {
    compare(
        "without-rowid",
        &[
            Step::Exec(
                "CREATE TABLE w(k1 TEXT, k2 INTEGER, v TEXT, PRIMARY KEY (k1, k2)) WITHOUT ROWID",
            ),
            Step::Exec("INSERT INTO w SELECT 'g' || (id % 211), id, a FROM t"),
            Step::Exec("CREATE INDEX wv ON w(v)"),
            Step::Exec("CREATE UNIQUE INDEX wu ON w(v, k2)"),
            Step::Query("SELECT k1, k2 FROM w WHERE v = 'shared-prefix-of-24-by-77'"),
            Step::Query("SELECT count(*) FROM w WHERE v > 'shared-prefix-of-24-by-9'"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

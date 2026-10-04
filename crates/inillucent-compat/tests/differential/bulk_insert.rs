//! `INSERT ... SELECT` into an empty table, graded against pinned SQLite
//! 3.53.4.
//!
//! Invariant: **a statement the bulk build takes answers exactly what the row
//! by row insert answers**: the same rows, the same error for a statement that
//! fails, and the same `changes()`, `total_changes()` and
//! `last_insert_rowid()` afterwards. Every claim is a comparison with a live
//! SQLite process, statement by statement.
//!
//! The bulk build checks every row before it writes any, so what it has to get
//! right that the ordinary insert gets for free is order: a statement that
//! breaks two constraints must name the one SQLite names, which is the first
//! row in input order that breaks anything, and `last_insert_rowid()` must be
//! left on the row before it. Most cases below are built to tell those orders
//! apart: the failing row is not the first row, the rows arrive in descending
//! order, or two different constraints fail on two different rows.
//!
//! Each source table holds 1,100 rows, which is over the 1,024 rows the bulk
//! build starts at, so every insert into an empty table here takes it unless its
//! shape is one the bulk build refuses. The cases with a foreign key, a
//! trigger and `RETURNING` are those shapes, and they are here so the hand
//! back to the ordinary insert is graded too.

use inillucent_compat::differential::{self, Step};

/// The source rows every case copies from: 1,100 rows with a text key, a
/// number, and a value that is NULL on every 50th row.
const SOURCE: &str = "CREATE TABLE src(id INTEGER PRIMARY KEY, k TEXT, n INTEGER, maybe TEXT)";

/// Fills [`SOURCE`].
const FILL: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 1100) \
     INSERT INTO src SELECT x, 'key ' || x, x * 7 % 101, \
     CASE WHEN x % 50 = 0 THEN NULL ELSE 'v' || x END FROM c";

/// Runs a scenario against both engines and compares every reply.
///
/// @param name - the scenario's name, which names its database files
/// @param steps - the statements, after the source table is built
fn compare(name: &str, steps: &[Step]) {
    let mut all = vec![Step::Exec(SOURCE), Step::Exec(FILL)];
    all.extend_from_slice(steps);
    let compared = differential::compare("bulk_insert", name, &all);
    assert!(
        compared == 0 || compared == all.len(),
        "compared {compared} of {} steps",
        all.len()
    );
}

/// A copy in rowid order, a copy in reverse order, and a copy whose rowids are
/// all allocated, each into an empty table.
#[test]
fn copies_match_sqlite() {
    compare(
        "copies",
        &[
            Step::Exec("CREATE TABLE a(id INTEGER PRIMARY KEY, k TEXT, n INTEGER)"),
            Step::Exec("INSERT INTO a SELECT id, k, n FROM src"),
            Step::Query("SELECT changes(), total_changes(), last_insert_rowid()"),
            Step::Query("SELECT count(*), sum(id), sum(n), min(k), max(k) FROM a"),
            Step::Exec("CREATE TABLE b(id INTEGER PRIMARY KEY, k TEXT, n INTEGER)"),
            Step::Exec("INSERT INTO b SELECT id, k, n FROM src ORDER BY id DESC"),
            Step::Query("SELECT changes(), last_insert_rowid()"),
            Step::Query("SELECT id, k FROM b WHERE id IN (1, 550, 1100) ORDER BY id"),
            Step::Exec("CREATE TABLE c(k TEXT, n INTEGER)"),
            Step::Exec("INSERT INTO c SELECT k, n FROM src ORDER BY n, id"),
            Step::Query("SELECT changes(), last_insert_rowid()"),
            Step::Query("SELECT rowid, k, n FROM c WHERE rowid % 37 = 1 ORDER BY rowid"),
            // Rows written after the bulk build go into the tree it built.
            Step::Exec("INSERT INTO c(k, n) VALUES ('after', 1)"),
            Step::Exec("UPDATE c SET n = n + 1 WHERE rowid % 10 = 0"),
            Step::Exec("DELETE FROM c WHERE rowid % 7 = 0"),
            Step::Query("SELECT count(*), sum(n), max(rowid) FROM c"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

/// Rowids the statement supplies, mixed with rowids it leaves out, so the
/// allocation has to count up from the largest rowid an earlier row of the
/// same statement took.
#[test]
fn allocated_rowids_follow_supplied_ones() {
    compare(
        "allocated-rowids",
        &[
            Step::Exec("CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec(
                "INSERT INTO t SELECT CASE WHEN id % 3 = 0 THEN NULL \
                 WHEN id % 3 = 1 THEN id * 10 ELSE -id END, k FROM src",
            ),
            Step::Query("SELECT changes(), last_insert_rowid()"),
            Step::Query("SELECT count(*), sum(id), min(id), max(id) FROM t"),
            Step::Query(
                "SELECT id, k FROM t WHERE k IN ('key 1', 'key 3', 'key 1100') ORDER BY id",
            ),
            Step::Exec("CREATE TABLE u(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO u SELECT -id, k FROM src"),
            Step::Exec("INSERT INTO u(k) VALUES ('next')"),
            Step::Query("SELECT id FROM u WHERE k = 'next'"),
        ],
    );
}

/// A `NOT NULL` that fails part way, a rowid that an earlier row of the
/// statement took, and a `CHECK` that fails part way: each leaves the table
/// empty and `last_insert_rowid()` on the row before the failure.
#[test]
fn a_constraint_that_fails_part_way_matches_sqlite() {
    compare(
        "constraint-part-way",
        &[
            Step::Exec("CREATE TABLE nn(id INTEGER PRIMARY KEY, maybe TEXT NOT NULL)"),
            Step::Exec("INSERT INTO nn SELECT id, maybe FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM nn"),
            Step::Exec("CREATE TABLE dup(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO dup SELECT id % 120, k FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM dup"),
            Step::Exec("CREATE TABLE ck(id INTEGER PRIMARY KEY, n INTEGER CHECK (n < 95))"),
            Step::Exec("INSERT INTO ck SELECT id, n FROM src ORDER BY id DESC"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM ck"),
            Step::Exec("CREATE TABLE st(id INTEGER PRIMARY KEY, n INTEGER) STRICT"),
            Step::Exec(
                "INSERT INTO st SELECT id, CASE WHEN id = 222 THEN 'text' ELSE n END FROM src",
            ),
            Step::Query("SELECT count(*), last_insert_rowid() FROM st"),
            Step::Exec("CREATE TABLE mm(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO mm SELECT CASE WHEN id = 99 THEN 'x' ELSE id END, k FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM mm"),
            // The same table takes the copy once the data is right, so the failed
            // statement left nothing behind that a later one trips over.
            Step::Exec("INSERT INTO nn SELECT id, coalesce(maybe, 'filled') FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM nn"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

/// Two different constraints fail on two different rows, and the one named is
/// the one on the earlier row.
#[test]
fn the_earlier_of_two_failures_is_the_one_named() {
    compare(
        "earlier-failure",
        &[
            Step::Exec("CREATE TABLE t(id INTEGER PRIMARY KEY, maybe TEXT NOT NULL, n INTEGER CHECK (n <> 3))"),
            // Row 49 fails the CHECK (49 * 7 % 101 = 3) before row 50's NULL.
            Step::Exec("INSERT INTO t SELECT id, maybe, n FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM t"),
            // Descending, the NULL on row 1100 comes first.
            Step::Exec("INSERT INTO t SELECT id, maybe, n FROM src ORDER BY id DESC"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM t"),
            Step::Exec("CREATE TABLE u(id INTEGER PRIMARY KEY, k TEXT UNIQUE, maybe TEXT NOT NULL)"),
            // A UNIQUE key repeats on row 12, before row 50's NULL.
            Step::Exec("INSERT INTO u SELECT id, CASE WHEN id = 12 THEN 'key 3' ELSE k END, maybe FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM u"),
            // And the NULL comes first when the repeat is on row 240.
            Step::Exec("INSERT INTO u SELECT id, CASE WHEN id = 240 THEN 'key 3' ELSE k END, maybe FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM u"),
        ],
    );
}

/// A column's own conflict clause skips a row or fills its default, which the
/// bulk build decides from the row alone.
#[test]
fn a_column_clause_that_skips_or_fills_matches_sqlite() {
    compare(
        "column-clause",
        &[
            Step::Exec("CREATE TABLE ig(id INTEGER PRIMARY KEY, maybe TEXT NOT NULL ON CONFLICT IGNORE)"),
            Step::Exec("INSERT INTO ig SELECT id, maybe FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM ig"),
            Step::Exec("CREATE TABLE ig2(maybe TEXT NOT NULL ON CONFLICT IGNORE)"),
            Step::Exec("INSERT INTO ig2 SELECT maybe FROM src"),
            Step::Query("SELECT count(*), max(rowid), last_insert_rowid() FROM ig2"),
            Step::Exec("CREATE TABLE rp(id INTEGER PRIMARY KEY, maybe TEXT NOT NULL ON CONFLICT REPLACE DEFAULT 'dflt')"),
            Step::Exec("INSERT INTO rp SELECT id, maybe FROM src"),
            Step::Query("SELECT count(*), sum(maybe = 'dflt') FROM rp"),
            Step::Exec("CREATE TABLE ab(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT OR ABORT INTO ab SELECT id, k FROM src"),
            Step::Query("SELECT count(*), changes() FROM ab"),
        ],
    );
}

/// Defaults, affinity and generated columns are applied to every row the bulk
/// build writes, as they are to a row written alone.
#[test]
fn defaults_affinity_and_generated_columns_match_sqlite() {
    compare(
        "defaults-affinity",
        &[
            Step::Exec(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, n REAL, s TEXT DEFAULT 'dflt', \
                 i INTEGER, g TEXT GENERATED ALWAYS AS (s || '-' || id) STORED, \
                 v INTEGER GENERATED ALWAYS AS (i * 2) VIRTUAL)",
            ),
            Step::Exec("INSERT INTO t(id, n, i) SELECT id, n, CAST(n AS TEXT) FROM src"),
            Step::Query(
                "SELECT typeof(n), typeof(i), s, g, v FROM t WHERE id IN (1, 77, 1100) ORDER BY id",
            ),
            Step::Query("SELECT count(*), sum(v), sum(length(g)) FROM t"),
        ],
    );
}

/// Indexes are built with the table: a plain index, a unique one, a partial
/// one, an expression, a descending key and a collation, each read back
/// through a query the planner answers from the index.
#[test]
fn indexes_built_with_the_table_match_sqlite() {
    compare(
        "indexes",
        &[
            Step::Exec("CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, n INTEGER, maybe TEXT)"),
            Step::Exec("CREATE INDEX t_n ON t(n, k)"),
            Step::Exec("CREATE UNIQUE INDEX t_k ON t(k)"),
            Step::Exec("CREATE INDEX t_part ON t(maybe) WHERE n > 50"),
            Step::Exec("CREATE INDEX t_expr ON t(lower(k) || 'x')"),
            Step::Exec("CREATE INDEX t_desc ON t(n DESC, id)"),
            Step::Exec("CREATE INDEX t_nocase ON t(maybe COLLATE NOCASE)"),
            Step::Exec("INSERT INTO t SELECT id, upper(k), n, maybe FROM src ORDER BY n, id DESC"),
            Step::Query("SELECT changes(), last_insert_rowid()"),
            Step::Query("SELECT id FROM t WHERE n = 42 ORDER BY k"),
            Step::Query("SELECT id FROM t WHERE k = 'KEY 123'"),
            Step::Query("SELECT count(*) FROM t WHERE maybe > 'v2' AND n > 50"),
            Step::Query("SELECT id FROM t WHERE lower(k) || 'x' = 'key 7x'"),
            Step::Query("SELECT n, id FROM t ORDER BY n DESC, id LIMIT 5"),
            Step::Query("SELECT id FROM t WHERE maybe = 'V9' COLLATE NOCASE"),
            Step::Query("SELECT count(*), sum(n) FROM t INDEXED BY t_n WHERE n >= 0"),
            Step::Exec("INSERT INTO t(k, n) VALUES ('KEY 1', 1)"),
            Step::Exec("INSERT INTO t(k, n) VALUES ('fresh', 1)"),
            Step::Query("SELECT count(*) FROM t WHERE n = 1"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

/// A duplicate in a unique index, in a table whose other rows are fine, is
/// named exactly as SQLite names it, and so is a unique index on a column
/// that holds several NULLs, which is no duplicate at all.
#[test]
fn a_unique_index_duplicate_matches_sqlite() {
    compare(
        "unique-duplicate",
        &[
            Step::Exec("CREATE TABLE t(id INTEGER PRIMARY KEY, n INTEGER UNIQUE, k TEXT)"),
            Step::Exec("INSERT INTO t SELECT id, n, k FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM t"),
            Step::Exec("CREATE TABLE nulls(id INTEGER PRIMARY KEY, maybe TEXT UNIQUE)"),
            Step::Exec("INSERT INTO nulls SELECT id, CASE WHEN id % 10 = 0 THEN NULL ELSE maybe END FROM src"),
            Step::Query("SELECT count(*), count(maybe), last_insert_rowid() FROM nulls"),
            Step::Exec("CREATE TABLE two(id INTEGER PRIMARY KEY, a TEXT, b TEXT)"),
            Step::Exec("CREATE UNIQUE INDEX two_a ON two(a)"),
            Step::Exec("CREATE UNIQUE INDEX two_b ON two(b)"),
            Step::Exec("INSERT INTO two SELECT id, CASE WHEN id = 200 THEN 'key 5' ELSE k END, \
                 CASE WHEN id = 100 THEN 'key 9' ELSE k END FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM two"),
        ],
    );
}

/// The statement is atomic inside a transaction: a rolled back transaction
/// takes the rows with it, a failed statement leaves the transaction open with
/// the earlier statements kept, and a savepoint rolled back takes only what
/// came after it.
#[test]
fn transactions_and_savepoints_match_sqlite() {
    compare(
        "transactions",
        &[
            Step::Exec("CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, maybe TEXT NOT NULL)"),
            Step::Exec("CREATE INDEX t_k ON t(k)"),
            Step::Exec("BEGIN"),
            Step::Exec("INSERT INTO t SELECT id, k, coalesce(maybe, '-') FROM src"),
            Step::Query("SELECT count(*) FROM t"),
            Step::Exec("ROLLBACK"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM t"),
            Step::Query("SELECT count(*) FROM t WHERE k > 'key'"),
            Step::Exec("CREATE TABLE side(x)"),
            Step::Exec("BEGIN"),
            Step::Exec("INSERT INTO side VALUES (1)"),
            Step::Exec("INSERT INTO t SELECT id, k, maybe FROM src"),
            Step::Query("SELECT count(*) FROM t"),
            Step::Exec("INSERT INTO t SELECT id, k, coalesce(maybe, '-') FROM src WHERE id > 50"),
            Step::Exec("COMMIT"),
            Step::Query("SELECT count(*), min(id), (SELECT count(*) FROM side) FROM t"),
            Step::Exec("CREATE TABLE s(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("BEGIN"),
            Step::Exec("SAVEPOINT one"),
            Step::Exec("INSERT INTO s SELECT id, k FROM src"),
            Step::Exec("ROLLBACK TO one"),
            Step::Query("SELECT count(*) FROM s"),
            Step::Exec("INSERT INTO s SELECT id, k FROM src WHERE id <= 1050"),
            Step::Exec("RELEASE one"),
            Step::Exec("COMMIT"),
            Step::Query("SELECT count(*), sum(id) FROM s"),
            Step::Query("PRAGMA integrity_check"),
        ],
    );
}

/// `CREATE TABLE ... AS SELECT` fills its table through the same statement,
/// and still moves none of the connection's counters.
#[test]
fn create_table_as_select_matches_sqlite() {
    compare(
        "ctas",
        &[
            Step::Exec("INSERT INTO src(k) VALUES ('one more')"),
            Step::Exec("CREATE TABLE copy AS SELECT id, k, n * 1.5 AS r FROM src"),
            Step::Query("SELECT changes(), total_changes(), last_insert_rowid()"),
            Step::Query("SELECT count(*), sum(r), typeof(r) FROM copy"),
        ],
    );
}

/// The shapes the bulk build refuses still insert correctly: a table that
/// already holds a row, a foreign key, a trigger, `RETURNING` and
/// `AUTOINCREMENT`.
#[test]
fn the_shapes_handed_back_match_sqlite() {
    compare(
        "handed-back",
        &[
            Step::Exec("CREATE TABLE held(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO held VALUES (5000, 'already')"),
            Step::Exec("INSERT INTO held SELECT id, k FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM held"),
            Step::Exec("CREATE TABLE child(id INTEGER PRIMARY KEY, parent INTEGER REFERENCES src(id))"),
            Step::Exec("INSERT INTO child SELECT id, id FROM src"),
            Step::Query("SELECT count(*) FROM child"),
            Step::Exec("CREATE TABLE trig(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("CREATE TABLE log(n)"),
            Step::Exec("CREATE TRIGGER trig_ai AFTER INSERT ON trig BEGIN INSERT INTO log VALUES (NEW.id); END"),
            Step::Exec("INSERT INTO trig SELECT id, k FROM src"),
            Step::Query("SELECT count(*), sum(n), changes(), total_changes() FROM log"),
            Step::Exec("CREATE TABLE auto(id INTEGER PRIMARY KEY AUTOINCREMENT, k TEXT)"),
            Step::Exec("INSERT INTO auto(k) SELECT k FROM src"),
            Step::Query("SELECT count(*), max(id), last_insert_rowid() FROM auto"),
            Step::Query("SELECT seq FROM sqlite_sequence WHERE name = 'auto'"),
            Step::Exec("CREATE TABLE ret(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Query("INSERT INTO ret SELECT id, k FROM src WHERE id <= 70 RETURNING id"),
        ],
    );
}

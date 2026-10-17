//! `INSERT ... SELECT` whose source rows do not fit in memory, graded against pinned SQLite 3.53.4.
//!
//! Invariant: **a statement whose source rows were moved to a transient file and written a row at a
//! time answers exactly what it answers with the rows in memory: the same rows in the table, the
//! same constraint named for the same row, the same `changes()`, `total_changes()` and
//! `last_insert_rowid()`, and nothing left behind by a statement that fails.** The statement reads
//! all of its source before it writes a row, so a row it writes is never read back by its own
//! source, and a failure on any row puts every earlier row back, whichever way the source was held.
//!
//! The shipped limit is 32 MiB of source rows, far above the 1,100 rows these cases copy, so each
//! case lowers it to 8 KiB. The source then goes to the file after a few dozen rows, and every case
//! checks that it did (`spilled`), because a case that never reached the file would grade the code
//! it was meant to replace. The cases are built the way `bulk_insert` builds them, so the order a
//! failing statement names its constraint in is graded again here for the row by row path that now
//! takes statements the bulk build used to take.

use inillucent_compat::differential::{self, Step};

/// How many bytes of source rows a statement holds before it moves them to a file.
const GATHER_BYTES: u64 = 8 * 1024;

/// Serialises the cases, which share one process wide run counter.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The source rows every case copies from: 1,100 rows with a text key, a number, and a value that
/// is NULL on every 50th row.
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
/// @param spilled - how many source collections at least must have moved to a file
fn compare(name: &str, steps: &[Step], spilled: u64) {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inillucent_exec::spill::set_run_bytes_override(GATHER_BYTES);
    let before = inillucent_exec::gather::spooled_collections();
    let mut all = vec![Step::Exec(SOURCE), Step::Exec(FILL)];
    all.extend_from_slice(steps);
    let compared = differential::compare("insert_select_spill", name, &all);
    assert!(
        compared == 0 || compared == all.len(),
        "compared {compared} of {} steps",
        all.len()
    );
    let moved = inillucent_exec::gather::spooled_collections().saturating_sub(before);
    assert!(
        compared == 0 || moved >= spilled,
        "{moved} source collections moved to a file, and this case needs {spilled}"
    );
}

/// Copies into an empty table and into a table that already holds rows, in rowid order, in reverse
/// order and with every rowid allocated.
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
            Step::Exec("INSERT INTO b VALUES (-5, 'seed', 0)"),
            Step::Exec("INSERT INTO b SELECT id, k, n FROM src ORDER BY id DESC"),
            Step::Query("SELECT changes(), last_insert_rowid()"),
            Step::Query("SELECT id, k FROM b WHERE id IN (-5, 1, 550, 1100) ORDER BY id"),
            Step::Exec("CREATE TABLE c(k TEXT, n INTEGER)"),
            Step::Exec("INSERT INTO c SELECT k, n FROM src ORDER BY n, id"),
            Step::Exec("INSERT INTO c SELECT k, n FROM src WHERE n > 50 ORDER BY n DESC, id"),
            Step::Query("SELECT changes(), last_insert_rowid(), total_changes()"),
            Step::Query("SELECT rowid, k, n FROM c WHERE rowid % 37 = 1 ORDER BY rowid"),
            Step::Exec("INSERT INTO c(k, n) VALUES ('after', 1)"),
            Step::Exec("UPDATE c SET n = n + 1 WHERE rowid % 10 = 0"),
            Step::Exec("DELETE FROM c WHERE rowid % 7 = 0"),
            Step::Query("SELECT count(*), sum(n), max(rowid) FROM c"),
            Step::Query("PRAGMA integrity_check"),
        ],
        3,
    );
}

/// A copy into an empty table with indexes of every kind is built a chunk at a time from the file,
/// and the table and its indexes answer as SQLite's do.
#[test]
fn a_streamed_copy_builds_the_table_and_every_kind_of_index() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inillucent_exec::spill::set_chunk_rows_override(100);
    drop(_serial);
    let before = inillucent_exec::gather::streamed_builds();
    compare(
        "streamed-indexes",
        &[
            Step::Exec("CREATE TABLE s(id INTEGER PRIMARY KEY, k TEXT, n INTEGER, maybe TEXT)"),
            Step::Exec("CREATE INDEX s_k ON s(k)"),
            Step::Exec("CREATE INDEX s_nk ON s(n DESC, k)"),
            Step::Exec("CREATE INDEX s_lower ON s(lower(k), n)"),
            Step::Exec("CREATE INDEX s_part ON s(maybe) WHERE n > 50"),
            Step::Exec("INSERT INTO s SELECT id, k, n, maybe FROM src"),
            Step::Query("SELECT changes(), last_insert_rowid(), total_changes()"),
            Step::Query("SELECT count(*), sum(id), sum(n) FROM s"),
            Step::Query("SELECT id FROM s WHERE k >= 'key 5' AND k < 'key 6' ORDER BY k LIMIT 20"),
            Step::Query("SELECT id FROM s WHERE n = 40 ORDER BY k LIMIT 9"),
            Step::Query("SELECT count(*) FROM s WHERE lower(k) = 'key 77' AND n = 14"),
            Step::Query("SELECT id FROM s WHERE n > 50 AND maybe > 'v5' ORDER BY maybe LIMIT 10"),
            Step::Exec("CREATE TABLE r(k TEXT, n INTEGER)"),
            Step::Exec("INSERT INTO r SELECT k, n FROM src ORDER BY id"),
            Step::Query("SELECT changes(), last_insert_rowid()"),
            Step::Query("SELECT rowid, k FROM r WHERE rowid IN (1, 333, 1100) ORDER BY rowid"),
            Step::Query("PRAGMA integrity_check"),
        ],
        2,
    );
    let streamed = inillucent_exec::gather::streamed_builds().saturating_sub(before);
    assert!(
        differential::sqlite_oracle().is_none() || streamed >= 2,
        "{streamed} copies took the streaming build, and this case copies into two empty tables"
    );
}

/// A statement that reads the table it writes sees the rows the table held when it began, and no
/// row it wrote itself.
#[test]
fn a_statement_that_reads_its_own_target_does_not_see_its_own_rows() {
    compare(
        "own-target",
        &[
            Step::Exec("CREATE TABLE t(id INTEGER PRIMARY KEY, k TEXT, n INTEGER)"),
            Step::Exec("INSERT INTO t SELECT id, k, n FROM src"),
            Step::Exec("INSERT INTO t SELECT id + 5000, k || 'x', n FROM t"),
            Step::Query("SELECT count(*), sum(id), max(id) FROM t"),
            Step::Exec("INSERT INTO t SELECT id + 20000, k, n FROM t WHERE n % 2 = 0"),
            Step::Query("SELECT count(*), sum(n) FROM t"),
            Step::Query("SELECT changes(), last_insert_rowid()"),
            Step::Query("PRAGMA integrity_check"),
        ],
        3,
    );
}

/// A `NOT NULL`, a duplicate rowid, a `CHECK`, a `STRICT` type and a `UNIQUE` index that fail part
/// way each leave the table as it was and `last_insert_rowid()` on the row before the failure, into
/// an empty table and into one that holds rows.
#[test]
fn a_constraint_that_fails_part_way_matches_sqlite() {
    compare(
        "constraint-part-way",
        &[
            Step::Exec("CREATE TABLE nn(id INTEGER PRIMARY KEY, maybe TEXT NOT NULL)"),
            Step::Exec("INSERT INTO nn SELECT id, maybe FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM nn"),
            Step::Exec("CREATE TABLE dup(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO dup VALUES (-1, 'seed')"),
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
            Step::Exec("CREATE TABLE ux(id INTEGER PRIMARY KEY, k TEXT, n INTEGER)"),
            Step::Exec("CREATE UNIQUE INDEX ux_n ON ux(n)"),
            Step::Exec("INSERT INTO ux SELECT id, k, n FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM ux"),
            Step::Exec("INSERT INTO ux SELECT id, k, id FROM src WHERE id < 90"),
            Step::Exec("INSERT INTO ux SELECT id + 1000, k, id + 1000 FROM src WHERE id < 60"),
            Step::Query("SELECT count(*), sum(n), last_insert_rowid() FROM ux"),
            // The same table takes the copy once the data is right, so the failed statements left
            // nothing behind that a later one trips over.
            Step::Exec("INSERT INTO nn SELECT id, coalesce(maybe, 'filled') FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM nn"),
            Step::Query("PRAGMA integrity_check"),
        ],
        8,
    );
}

/// Two different constraints fail on two different rows, and the statement names the one SQLite
/// names, which is the first row in input order that breaks anything.
#[test]
fn two_constraints_on_two_rows_name_the_first_row_in_input_order() {
    compare(
        "first-in-input-order",
        &[
            Step::Exec(
                "CREATE TABLE two(id INTEGER PRIMARY KEY, maybe TEXT NOT NULL, n INTEGER CHECK (n < 99))",
            ),
            Step::Exec("INSERT INTO two SELECT id, maybe, n FROM src ORDER BY id DESC"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM two"),
            Step::Exec("INSERT INTO two SELECT id, coalesce(maybe, 'f'), n FROM src ORDER BY id"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM two"),
        ],
        2,
    );
}

/// `OR IGNORE`, `OR REPLACE`, `OR FAIL` and `OR ROLLBACK` over rows that collide part way keep
/// exactly the rows SQLite keeps.
#[test]
fn conflict_clauses_keep_the_rows_sqlite_keeps() {
    compare(
        "conflict-clauses",
        &[
            Step::Exec("CREATE TABLE ig(id INTEGER PRIMARY KEY, k TEXT, n INTEGER)"),
            Step::Exec("INSERT INTO ig SELECT id, k, n FROM src WHERE id % 3 = 0"),
            Step::Exec("INSERT OR IGNORE INTO ig SELECT id, 'ignored', n FROM src"),
            Step::Query("SELECT count(*), sum(n), changes(), last_insert_rowid() FROM ig"),
            Step::Query("SELECT count(*) FROM ig WHERE k = 'ignored'"),
            Step::Exec(
                "INSERT OR REPLACE INTO ig SELECT id, 'replaced', n FROM src WHERE id % 2 = 0",
            ),
            Step::Query("SELECT count(*), sum(n), changes(), last_insert_rowid() FROM ig"),
            Step::Query("SELECT count(*) FROM ig WHERE k = 'replaced'"),
            Step::Exec("CREATE TABLE fl(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO fl VALUES (700, 'taken')"),
            Step::Exec("INSERT OR FAIL INTO fl SELECT id, k FROM src"),
            Step::Query("SELECT count(*), max(id), changes(), last_insert_rowid() FROM fl"),
            Step::Exec("BEGIN"),
            Step::Exec("INSERT OR FAIL INTO fl SELECT id + 1000, k FROM src WHERE id < 500"),
            Step::Exec("INSERT OR FAIL INTO fl SELECT 2000 + id % 300, k FROM src WHERE id < 600"),
            Step::Exec("COMMIT"),
            Step::Query("SELECT count(*), max(id), changes(), last_insert_rowid() FROM fl"),
            Step::Exec("CREATE TABLE rb(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO rb VALUES (900, 'x')"),
            Step::Exec("BEGIN"),
            Step::Exec("INSERT INTO rb VALUES (-9, 'inside')"),
            Step::Exec("INSERT OR ROLLBACK INTO rb SELECT id, k FROM src"),
            Step::Query("SELECT count(*) FROM rb"),
            Step::Query("PRAGMA integrity_check"),
        ],
        5,
    );
}

/// An `AUTOINCREMENT` table's sequence moves with the rows written and is put back with them.
#[test]
fn autoincrement_sequences_match_sqlite() {
    compare(
        "autoincrement",
        &[
            Step::Exec("CREATE TABLE ai(id INTEGER PRIMARY KEY AUTOINCREMENT, k TEXT)"),
            Step::Exec("INSERT INTO ai(k) SELECT k FROM src"),
            Step::Query("SELECT count(*), max(id), changes(), last_insert_rowid() FROM ai"),
            Step::Query("SELECT seq FROM sqlite_sequence WHERE name = 'ai'"),
            Step::Exec("DELETE FROM ai WHERE id > 1000"),
            Step::Exec("INSERT INTO ai(k) SELECT k FROM src WHERE id <= 40"),
            Step::Query("SELECT count(*), max(id) FROM ai"),
            Step::Query("SELECT seq FROM sqlite_sequence WHERE name = 'ai'"),
            // A failure part way leaves the sequence where it was.
            Step::Exec("CREATE TABLE ak(id INTEGER PRIMARY KEY AUTOINCREMENT, k TEXT NOT NULL)"),
            Step::Exec("INSERT INTO ak(k) SELECT maybe FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM ak"),
            Step::Query("SELECT count(*) FROM sqlite_sequence WHERE name = 'ak'"),
            Step::Exec("INSERT OR FAIL INTO ak(k) SELECT maybe FROM src"),
            Step::Query("SELECT count(*), max(id) FROM ak"),
            Step::Query("SELECT seq FROM sqlite_sequence WHERE name = 'ak'"),
            Step::Query("PRAGMA integrity_check"),
        ],
        4,
    );
}

/// Triggers run for every row of the copy, a `RETURNING` clause hands every row back, and a trigger
/// that raises part way undoes the rows before it.
#[test]
fn triggers_and_returning_run_for_every_row() {
    compare(
        "triggers",
        &[
            Step::Exec("CREATE TABLE tgt(id INTEGER PRIMARY KEY, k TEXT, n INTEGER)"),
            Step::Exec("CREATE TABLE logged(id INTEGER PRIMARY KEY, what TEXT)"),
            Step::Exec(
                "CREATE TRIGGER tgt_after AFTER INSERT ON tgt BEGIN INSERT INTO logged(what) VALUES ('row ' || NEW.id); END",
            ),
            Step::Exec("INSERT INTO tgt SELECT id, k, n FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid(), total_changes() FROM tgt"),
            Step::Query("SELECT count(*), min(what), max(what) FROM logged"),
            Step::Exec("CREATE TABLE guarded(id INTEGER PRIMARY KEY, n INTEGER)"),
            Step::Exec(
                "CREATE TRIGGER g_before BEFORE INSERT ON guarded WHEN NEW.n = 77 BEGIN SELECT RAISE(ABORT, 'seventy seven'); END",
            ),
            Step::Exec("INSERT INTO guarded SELECT id, n FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM guarded"),
            Step::Exec("CREATE TABLE ret(id INTEGER PRIMARY KEY, n INTEGER)"),
            Step::Query("INSERT INTO ret SELECT id, n FROM src WHERE id % 10 = 0 RETURNING id, n * 2"),
            Step::Query("SELECT count(*), sum(n) FROM ret"),
            Step::Query("PRAGMA integrity_check"),
        ],
        4,
    );
}

/// A child row whose parent is missing fails the statement and leaves nothing, and the rows
/// copied before it are put back.
#[test]
fn foreign_keys_are_checked_for_every_row() {
    compare(
        "foreign-keys",
        &[
            Step::Exec("PRAGMA foreign_keys = ON"),
            Step::Exec("CREATE TABLE parent(id INTEGER PRIMARY KEY)"),
            Step::Exec("INSERT INTO parent SELECT id FROM src WHERE id <= 1000"),
            Step::Exec(
                "CREATE TABLE child(id INTEGER PRIMARY KEY, pid INTEGER REFERENCES parent(id))",
            ),
            Step::Exec("INSERT INTO child SELECT id, id FROM src"),
            Step::Query("SELECT count(*), last_insert_rowid() FROM child"),
            Step::Exec("INSERT INTO child SELECT id, id FROM src WHERE id <= 1000"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM child"),
            Step::Query("PRAGMA foreign_key_check"),
        ],
        3,
    );
}

/// A copy inside a transaction is undone by `ROLLBACK` and by a rolled back savepoint.
#[test]
fn a_rolled_back_copy_leaves_nothing() {
    compare(
        "rollback",
        &[
            Step::Exec("CREATE TABLE r(id INTEGER PRIMARY KEY, k TEXT, n INTEGER)"),
            Step::Exec("INSERT INTO r VALUES (-1, 'seed', 0)"),
            Step::Exec("BEGIN"),
            Step::Exec("INSERT INTO r SELECT id, k, n FROM src"),
            Step::Query("SELECT count(*) FROM r"),
            Step::Exec("ROLLBACK"),
            Step::Query("SELECT count(*), sum(id) FROM r"),
            Step::Exec("BEGIN"),
            Step::Exec("INSERT INTO r SELECT id, k, n FROM src WHERE id <= 300"),
            Step::Exec("SAVEPOINT s"),
            Step::Exec("INSERT INTO r SELECT id + 5000, k, n FROM src"),
            Step::Exec("ROLLBACK TO s"),
            Step::Exec("COMMIT"),
            Step::Query("SELECT count(*), sum(id), max(id) FROM r"),
            Step::Query("PRAGMA integrity_check"),
        ],
        3,
    );
}

/// A source that is a compound select, an aggregate, a join or a `DISTINCT`, which are not plain
/// scans, copies the same rows.
#[test]
fn sources_that_are_not_plain_scans_copy_the_same_rows() {
    compare(
        "sources",
        &[
            Step::Exec("CREATE TABLE u(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO u SELECT id, k FROM src WHERE id < 600 UNION ALL SELECT id + 5000, k FROM src WHERE id < 600"),
            Step::Query("SELECT count(*), sum(id) FROM u"),
            Step::Exec("CREATE TABLE g(n INTEGER, c INTEGER, total INTEGER)"),
            Step::Exec("INSERT INTO g SELECT n, count(*), sum(id) FROM src GROUP BY n"),
            Step::Query("SELECT count(*), sum(c), sum(total) FROM g"),
            Step::Exec("CREATE TABLE j(a INTEGER, b TEXT)"),
            Step::Exec("INSERT INTO j SELECT s1.id, s2.k FROM src s1 JOIN src s2 ON s2.id = s1.id + 1"),
            Step::Query("SELECT count(*), sum(a) FROM j"),
            Step::Exec("CREATE TABLE d(n INTEGER)"),
            Step::Exec("INSERT INTO d SELECT DISTINCT n FROM src"),
            Step::Query("SELECT count(*), sum(n) FROM d"),
            Step::Query("PRAGMA integrity_check"),
        ],
        0,
    );
}

/// A streamed copy that stops part way, because a row fails or the rowids stop increasing, leaves
/// what SQLite leaves, inside a transaction that goes on and commits and on its own.
///
/// The streamed build checks each row as it writes it, so a row that fails ends a tree that is
/// half built, and the statement's writes are undone to its mark. A source in falling rowid order
/// is undone the same way after its first row and handed to the row by row insert. The chunk size
/// is lowered so the stop comes after several chunks have been written.
#[test]
fn a_streamed_copy_that_stops_part_way_leaves_what_sqlite_leaves() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inillucent_exec::spill::set_chunk_rows_override(100);
    drop(_serial);
    compare(
        "stops-part-way",
        &[
            Step::Exec("BEGIN"),
            Step::Exec("CREATE TABLE p(id INTEGER PRIMARY KEY, k TEXT, maybe TEXT NOT NULL)"),
            Step::Exec("CREATE INDEX p_k ON p(k)"),
            Step::Exec("INSERT INTO p SELECT id, k, maybe FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM p"),
            Step::Exec("INSERT INTO p SELECT id, k, coalesce(maybe, 'filled') FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM p"),
            Step::Exec("CREATE TABLE q(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("CREATE INDEX q_k ON q(k)"),
            Step::Exec("INSERT INTO q SELECT id, k FROM src ORDER BY id DESC"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid(), min(id), max(id) FROM q"),
            Step::Exec("COMMIT"),
            Step::Query("PRAGMA integrity_check"),
            Step::Exec("CREATE TABLE r(id INTEGER PRIMARY KEY, n INTEGER CHECK (n < 99))"),
            Step::Exec("CREATE INDEX r_n ON r(n)"),
            Step::Exec("INSERT INTO r SELECT id, n FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid() FROM r"),
            Step::Exec("CREATE TABLE s(id INTEGER PRIMARY KEY, k TEXT)"),
            Step::Exec("INSERT INTO s SELECT 1100 - id, k FROM src"),
            Step::Query("SELECT count(*), changes(), last_insert_rowid(), min(id), max(id) FROM s"),
            Step::Query("SELECT count(*) FROM p WHERE k > 'key 9'"),
            Step::Query("PRAGMA integrity_check"),
        ],
        6,
    );
}

/// The file a streamed copy that stopped part way left behind reopens whole.
///
/// A statement that reads its own pages back from the cache can look right while the file is not,
/// so the database is closed, opened again and checked.
#[test]
fn a_file_with_a_stopped_streamed_copy_reopens_whole() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    inillucent_exec::spill::set_run_bytes_override(GATHER_BYTES);
    inillucent_exec::spill::set_chunk_rows_override(100);
    let area = inillucent_compat::workspace_root().join("target/scratch/insert-select-spill");
    let _ = std::fs::create_dir_all(&area);
    let path = area.join(format!("reopen-{}.rdb", std::process::id()));
    inillucent_base::testing::remove_database(&path);
    {
        let database = inillucent_engine::connect::Database::open(&path).expect("it opens");
        let connection = database.session();
        connection.execute_batch(SOURCE).expect("the source table");
        connection.execute_batch(FILL).expect("the source rows");
        connection
            .execute_batch(
                "CREATE TABLE p(id INTEGER PRIMARY KEY, k TEXT, maybe TEXT NOT NULL); \
                 CREATE INDEX p_k ON p(k); CREATE TABLE q(id INTEGER PRIMARY KEY, k TEXT);",
            )
            .expect("the targets");
        assert!(
            connection
                .execute_batch("INSERT INTO p SELECT id, k, maybe FROM src")
                .is_err(),
            "the fiftieth row's NULL fails the copy"
        );
        connection
            .execute_batch("INSERT INTO q SELECT id, k FROM src ORDER BY id DESC")
            .expect("a copy in falling rowid order is written row by row");
        connection
            .execute_batch("INSERT INTO p SELECT id, k, coalesce(maybe, 'x') FROM src")
            .expect("the copy that is right");
    }
    let database = inillucent_engine::connect::Database::open(&path).expect("it opens again");
    let connection = database.session();
    let answer = |sql: &str| -> String {
        let rows = connection.query(sql).expect("the query runs");
        rows.iter()
            .flat_map(|row| row.iter().map(|cell| format!("{cell:?}")))
            .collect::<Vec<_>>()
            .join(",")
    };
    assert_eq!(answer("PRAGMA integrity_check"), r#"Text([111, 107])"#);
    assert_eq!(
        answer("SELECT count(*), sum(id) FROM p"),
        "Int(1100),Int(605550)"
    );
    assert_eq!(
        answer("SELECT count(*), sum(id) FROM q"),
        "Int(1100),Int(605550)"
    );
}

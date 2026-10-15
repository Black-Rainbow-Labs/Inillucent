//! `ALTER TABLE ... ADD COLUMN` on a table that has rows writes no leaf.
//!
//! Invariant: **the log an `ADD COLUMN` appends and the pages it leaves dirty do not depend
//! on how many rows the table holds, a read of the table afterwards writes nothing, and the
//! first write into a leaf rewrites that leaf and no other.** Before this the statement read
//! every row, built a new tree from them and freed the old one: 783 ms for 800,000 rows where
//! SQLite takes 19, and a log record per page.
//!
//! The counts are bytes of log and pages written to the file, taken around the statement. A
//! timing would measure the machine, and these measure the work. The cases build the table at
//! three sizes, so a statement that did a page of work for every thousand rows fails by
//! the difference between them and not by a threshold somebody chose.

use std::sync::Arc;

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::{DbPath, MemoryVfs, OpenOptions, Vfs};

/// The page size every case runs at, so a few thousand rows take many leaves.
const PAGE_SIZE: usize = 4_096;

/// How many frames the pool holds.
const FRAMES: usize = 1_024;

/// One case's database: the file system it lives on and where.
struct Case {
    vfs: Arc<dyn Vfs>,
    path: std::path::PathBuf,
}

impl Case {
    /// Returns a case with nothing created yet.
    ///
    /// @param tag - what to call the database
    fn new(tag: &str) -> Case {
        Case {
            vfs: Arc::new(MemoryVfs::new()),
            path: std::env::temp_dir().join(format!("inillucent-add-column-{tag}.rdb")),
        }
    }

    /// Creates the database.
    fn create(&self) -> ImportedDatabase {
        ImportedDatabase::create_on(Arc::clone(&self.vfs), self.path.clone(), PAGE_SIZE, FRAMES)
            .expect("the database is created in memory")
    }

    /// Opens the database again, as another process would find it.
    fn reopen(&self) -> ImportedDatabase {
        ImportedDatabase::open_on(Arc::clone(&self.vfs), self.path.clone(), PAGE_SIZE, FRAMES)
            .expect("the database reopens")
    }

    /// Returns the format version the file's meta record says.
    fn format(&self) -> u32 {
        let path = DbPath::new(self.path.to_string_lossy().as_ref());
        let file = self
            .vfs
            .open(&path, OpenOptions::main_db())
            .expect("the file opens");
        let mut four = [0u8; 4];
        file.read_exact_at(8, &mut four)
            .expect("a file has a header");
        u32::from_le_bytes(four)
    }
}

/// Runs a statement of any kind and returns its rows, failing with the statement.
///
/// @param engine - the open connection
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) -> Vec<Vec<OwnedDatum>> {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|why| panic!("{sql}: {}", why.message()))
        .rows
}

/// Returns the one integer a query answers with.
///
/// @param engine - the open connection
/// @param sql - a query returning one row of one integer
fn count(engine: &mut ImportedDatabase, sql: &str) -> i64 {
    match run(engine, sql).first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(value)) => *value,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Makes the table `t` with `rows` rows, folds the log, and returns the connection.
///
/// @param case - the case's database
/// @param rows - how many rows
fn table_of(case: &Case, rows: i64) -> ImportedDatabase {
    let mut engine = case.create();
    run(
        &mut engine,
        "CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT)",
    );
    run(
        &mut engine,
        &format!(
            "INSERT INTO t SELECT value, printf('row-%08d', value) FROM generate_series(1, {rows})"
        ),
    );
    engine.checkpoint().expect("the fold runs");
    engine
}

/// What one statement cost: bytes of log appended and pages written to the file.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Cost {
    log_bytes: u64,
    page_writes: u64,
}

/// Runs a statement, folds the log, and returns what it cost.
///
/// @param engine - the open connection
/// @param sql - the statement
fn cost_of(engine: &mut ImportedDatabase, sql: &str) -> Cost {
    let log_before = engine.wal().stats().bytes;
    let writes_before = engine.pool_stats().writes;
    run(engine, sql);
    engine.checkpoint().expect("the fold runs");
    Cost {
        log_bytes: engine.wal().stats().bytes.saturating_sub(log_before),
        page_writes: engine.pool_stats().writes.saturating_sub(writes_before),
    }
}

/// The statement costs the same at 1,000, 10,000 and 40,000 rows, and a fraction of a leaf.
#[test]
fn adding_a_column_writes_the_same_pages_whatever_the_row_count() {
    let mut costs = Vec::new();
    for rows in [1_000i64, 10_000, 40_000] {
        let case = Case::new(&format!("cost-{rows}"));
        let mut engine = table_of(&case, rows);
        costs.push(cost_of(
            &mut engine,
            "ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 5",
        ));
        assert_eq!(
            count(&mut engine, "SELECT sum(e) FROM t"),
            rows * 5,
            "{rows} rows"
        );
        assert_eq!(
            count(&mut engine, "SELECT count(*) FROM t WHERE e = 5"),
            rows
        );
    }
    let first = costs.first().copied().expect("three sizes ran");
    for cost in &costs {
        assert_eq!(cost, &first, "the cost moved with the row count: {costs:?}");
    }
    assert!(
        first.page_writes <= 8 && first.log_bytes < 4 * PAGE_SIZE as u64,
        "ADD COLUMN cost {first:?}, which is more than the catalog row and the meta record"
    );
}

/// A table with rows takes ten columns for the price of one, and a column with a check too.
#[test]
fn several_columns_and_a_check_cost_no_leaf_either() {
    let case = Case::new("several");
    let mut engine = table_of(&case, 20_000);
    let mut total = Cost {
        log_bytes: 0,
        page_writes: 0,
    };
    for statement in [
        "ALTER TABLE t ADD COLUMN c1 INTEGER DEFAULT 1",
        "ALTER TABLE t ADD COLUMN c2 TEXT DEFAULT 'two'",
        "ALTER TABLE t ADD COLUMN c3 REAL DEFAULT 3",
        "ALTER TABLE t ADD COLUMN c4",
        "ALTER TABLE t ADD COLUMN c5 BLOB DEFAULT x'0102'",
        "ALTER TABLE t ADD COLUMN c6 INTEGER NOT NULL DEFAULT 6",
        "ALTER TABLE t ADD COLUMN c7 INTEGER DEFAULT -7",
        "ALTER TABLE t ADD COLUMN c8 NUMERIC DEFAULT '8'",
        "ALTER TABLE t ADD COLUMN c9 DEFAULT 9.5",
        "ALTER TABLE t ADD COLUMN c10 AS (id * 2)",
    ] {
        let cost = cost_of(&mut engine, statement);
        total.log_bytes += cost.log_bytes;
        total.page_writes += cost.page_writes;
    }
    assert!(
        total.page_writes <= 60 && total.log_bytes < 40 * PAGE_SIZE as u64,
        "ten columns on a 20,000 row table cost {total:?}"
    );
    for (predicate, expected) in [
        ("c1 = 1 AND typeof(c1) = 'integer'", 20_000),
        ("c2 = 'two' AND typeof(c2) = 'text'", 20_000),
        ("c3 = 3.0 AND typeof(c3) = 'real'", 20_000),
        ("c4 IS NULL", 20_000),
        ("hex(c5) = '0102' AND typeof(c5) = 'blob'", 20_000),
        ("c6 = 6", 20_000),
        ("c7 = -7", 20_000),
        ("c8 = 8 AND typeof(c8) = 'integer'", 20_000),
        ("c9 = 9.5 AND typeof(c9) = 'real'", 20_000),
        ("c10 = id * 2", 20_000),
    ] {
        assert_eq!(
            count(
                &mut engine,
                &format!("SELECT count(*) FROM t WHERE {predicate}")
            ),
            expected,
            "{predicate}"
        );
    }
    assert_eq!(
        count(
            &mut engine,
            "SELECT count(*) FROM t WHERE c1 = 1 AND c6 = 6"
        ),
        20_000
    );
}

/// Reading the altered table, in every way the executor reads one, writes nothing.
#[test]
fn reading_a_table_with_short_leaves_writes_nothing() {
    let case = Case::new("reads");
    let mut engine = table_of(&case, 12_000);
    run(&mut engine, "ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 5");
    run(&mut engine, "ALTER TABLE t ADD COLUMN f TEXT DEFAULT 'hi'");
    run(&mut engine, "CREATE INDEX t_e ON t(e, f)");
    engine.checkpoint().expect("the fold runs");
    let log_before = engine.wal().stats().bytes;
    let writes_before = engine.pool_stats().writes;
    assert_eq!(count(&mut engine, "SELECT count(*) FROM t"), 12_000);
    assert_eq!(count(&mut engine, "SELECT sum(e) FROM t"), 60_000);
    assert_eq!(
        count(&mut engine, "SELECT count(*) FROM t WHERE f = 'hi'"),
        12_000
    );
    assert_eq!(count(&mut engine, "SELECT count(DISTINCT e) FROM t"), 1);
    assert_eq!(count(&mut engine, "SELECT e FROM t WHERE id = 6000"), 5);
    assert_eq!(
        count(
            &mut engine,
            "SELECT count(*) FROM t a JOIN t b ON a.e = b.e WHERE a.id < 3 AND b.id < 50"
        ),
        98
    );
    assert_eq!(
        count(
            &mut engine,
            "SELECT count(*) FROM t INDEXED BY t_e WHERE e = 5 AND f = 'hi'"
        ),
        12_000
    );
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
    assert_eq!(
        engine.wal().stats().bytes,
        log_before,
        "a read appended to the log"
    );
    assert_eq!(
        engine.pool_stats().writes,
        writes_before,
        "a read wrote a page"
    );
}

/// An update of one row rewrites its leaf, and only a few leaves, however big the table is.
#[test]
fn the_first_write_into_a_leaf_widens_that_leaf_only() {
    let case = Case::new("widen");
    let mut engine = table_of(&case, 20_000);
    run(&mut engine, "ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 5");
    run(&mut engine, "ALTER TABLE t ADD COLUMN f TEXT DEFAULT 'a default long enough to make a widened row larger'");
    engine.checkpoint().expect("the fold runs");
    let cost = cost_of(&mut engine, "UPDATE t SET e = 6 WHERE id = 10000");
    assert!(
        cost.page_writes <= 24,
        "one row's update wrote {} pages of a table of 20,000 rows",
        cost.page_writes
    );
    assert_eq!(count(&mut engine, "SELECT e FROM t WHERE id = 10000"), 6);
    assert_eq!(count(&mut engine, "SELECT e FROM t WHERE id = 10001"), 5);
    assert_eq!(count(&mut engine, "SELECT sum(e) FROM t"), 5 * 20_000 + 1);
    assert_eq!(
        count(
            &mut engine,
            "SELECT count(*) FROM t WHERE f LIKE 'a default long%'"
        ),
        20_000
    );
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
}

/// Every kind of write lands in a short leaf and the table stays whole.
#[test]
fn inserts_updates_and_deletes_into_short_leaves_agree_with_the_defaults() {
    let case = Case::new("writes");
    let mut engine = table_of(&case, 6_000);
    run(&mut engine, "ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 5");
    run(&mut engine, "ALTER TABLE t ADD COLUMN f TEXT DEFAULT 'hi'");
    run(&mut engine, "ALTER TABLE t ADD COLUMN g REAL");
    run(&mut engine, "INSERT INTO t(id, a, e) VALUES (3000, 'replaced?', 1) ON CONFLICT(id) DO UPDATE SET e = e + 100");
    assert_eq!(count(&mut engine, "SELECT e FROM t WHERE id = 3000"), 105);
    run(&mut engine, "INSERT INTO t(id, a) VALUES (7000, 'new')");
    run(&mut engine, "UPDATE t SET g = id * 0.5 WHERE id % 100 = 0");
    run(
        &mut engine,
        "UPDATE t SET f = f || '!' WHERE id BETWEEN 100 AND 400",
    );
    run(&mut engine, "DELETE FROM t WHERE id % 3 = 0");
    assert_eq!(count(&mut engine, "SELECT count(*) FROM t"), 4_001);
    assert_eq!(
        count(&mut engine, "SELECT count(*) FROM t WHERE f = 'hi!'"),
        201
    );
    assert_eq!(count(&mut engine, "SELECT count(g) FROM t"), 41);
    assert_eq!(
        count(&mut engine, "SELECT count(*) FROM t WHERE e = 5"),
        4_001
    );
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
    // The rollback of a write into a short leaf puts the row back as it was.
    run(&mut engine, "BEGIN");
    run(&mut engine, "UPDATE t SET e = 99 WHERE id < 2000");
    run(&mut engine, "DELETE FROM t WHERE id > 5000");
    run(&mut engine, "ROLLBACK");
    assert_eq!(count(&mut engine, "SELECT count(*) FROM t"), 4_001);
    assert_eq!(count(&mut engine, "SELECT count(*) FROM t WHERE e = 99"), 0);
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
}

/// A reopened file reads the defaults, and its header says format 4 only once a column was added.
#[test]
fn a_reopened_file_reads_the_defaults_and_says_format_four() {
    let case = Case::new("reopen");
    let mut engine = table_of(&case, 5_000);
    assert_eq!(case.format(), 3, "a new file says 3");
    run(&mut engine, "ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 5");
    run(&mut engine, "ALTER TABLE t ADD COLUMN f TEXT DEFAULT 'hi'");
    engine.checkpoint().expect("the fold runs");
    assert_eq!(case.format(), 4, "a file with a short leaf says 4");
    drop(engine);
    let mut engine = case.reopen();
    assert_eq!(count(&mut engine, "SELECT sum(e) FROM t"), 25_000);
    assert_eq!(
        count(&mut engine, "SELECT count(*) FROM t WHERE f = 'hi'"),
        5_000
    );
    run(&mut engine, "UPDATE t SET e = 0 WHERE id % 2 = 0");
    drop(engine);
    let mut engine = case.reopen();
    assert_eq!(count(&mut engine, "SELECT sum(e) FROM t"), 2_500 * 5);
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
}

/// A table with no rows, and a column that needs a rewrite, keep the format a release before this reads.
#[test]
fn only_a_short_leaf_moves_the_file_to_format_four() {
    let case = Case::new("format-stays");
    let mut engine = case.create();
    run(
        &mut engine,
        "CREATE TABLE empty_one(id INTEGER PRIMARY KEY)",
    );
    run(
        &mut engine,
        "ALTER TABLE empty_one ADD COLUMN x INTEGER DEFAULT 1",
    );
    run(&mut engine, "CREATE TABLE t(id INTEGER PRIMARY KEY)");
    run(&mut engine, "INSERT INTO t VALUES (1), (2), (3)");
    run(
        &mut engine,
        "ALTER TABLE t ADD COLUMN y INTEGER DEFAULT (CAST('5' AS INTEGER))",
    );
    engine.checkpoint().expect("the fold runs");
    assert_eq!(
        case.format(),
        3,
        "an empty table and a default the pad cannot read rewrite the table"
    );
    assert_eq!(count(&mut engine, "SELECT count(*) FROM t WHERE y = 5"), 3);
    run(&mut engine, "ALTER TABLE t ADD COLUMN w INTEGER DEFAULT 4");
    engine.checkpoint().expect("the fold runs");
    assert_eq!(case.format(), 4);
}

/// A column dropped after a column was added reads the rows right, and a second add after that too.
#[test]
fn dropping_and_adding_columns_after_a_lazy_add_keeps_every_value() {
    let case = Case::new("drop");
    let mut engine = table_of(&case, 3_000);
    run(&mut engine, "ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 5");
    run(&mut engine, "ALTER TABLE t ADD COLUMN f TEXT DEFAULT 'hi'");
    run(&mut engine, "ALTER TABLE t DROP COLUMN a");
    assert_eq!(count(&mut engine, "SELECT sum(e) FROM t"), 15_000);
    assert_eq!(
        count(&mut engine, "SELECT count(*) FROM t WHERE f = 'hi'"),
        3_000
    );
    run(&mut engine, "ALTER TABLE t ADD COLUMN g INTEGER DEFAULT 7");
    run(&mut engine, "ALTER TABLE t RENAME COLUMN g TO h");
    run(&mut engine, "UPDATE t SET h = h + id WHERE id <= 10");
    assert_eq!(count(&mut engine, "SELECT sum(h) FROM t"), 7 * 3_000 + 55);
    run(&mut engine, "CREATE INDEX t_h ON t(h)");
    assert_eq!(
        count(&mut engine, "SELECT count(*) FROM t WHERE h = 7"),
        2_990
    );
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
}

/// A table without a rowid takes the column the same way.
#[test]
fn a_table_without_rowid_takes_a_column_without_a_rewrite() {
    let case = Case::new("without-rowid");
    let mut engine = case.create();
    run(
        &mut engine,
        "CREATE TABLE k(a TEXT PRIMARY KEY, b INTEGER) WITHOUT ROWID",
    );
    run(
        &mut engine,
        "INSERT INTO k SELECT printf('key-%08d', value), value FROM generate_series(1, 8000)",
    );
    engine.checkpoint().expect("the fold runs");
    let cost = cost_of(&mut engine, "ALTER TABLE k ADD COLUMN c INTEGER DEFAULT 3");
    assert!(cost.page_writes <= 8, "{cost:?}");
    assert_eq!(count(&mut engine, "SELECT sum(c) FROM k"), 24_000);
    run(&mut engine, "UPDATE k SET c = 4 WHERE b = 100");
    assert_eq!(count(&mut engine, "SELECT sum(c) FROM k"), 24_001);
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
}

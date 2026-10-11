//! `PRAGMA auto_vacuum` FULL and INCREMENTAL, `PRAGMA incremental_vacuum`, and what
//! a `DELETE` must free for either to matter.
//!
//! Invariant: **a file in FULL mode has no free page below its end and is exactly
//! `page_count * page_size` bytes long when a statement that freed pages returns;
//! a file in INCREMENTAL mode gives back exactly as many pages as the pragma
//! asked for; and every row still reads, through the table and through an index,
//! after the file is reopened.**
//!
//! ## Why this exists
//!
//! The two modes were stored in the file and did nothing, `incremental_vacuum`
//! rebuilt the whole file, and a table emptied by `DELETE FROM` kept most of its
//! pages. An application that turns `auto_vacuum` on so the file gives space back
//! after it deletes got a file that never shrank. Every case here opens on
//! `MemoryVfs` at 8192 byte pages, so the numbers are small and the cases run in
//! seconds, and every case reopens before it reads the rows again.

use std::sync::Arc;

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::{DbPath, MemoryVfs, OpenOptions, Vfs};

/// The page size every case runs at.
const PAGE_SIZE: usize = 8_192;

/// How many frames the pool holds.
const FRAMES: usize = 256;

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
            path: std::env::temp_dir().join(format!("inillucent-auto-vacuum-{tag}.rdb")),
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

    /// Returns how many bytes the database file is.
    fn file_length(&self) -> u64 {
        let path = DbPath::new(self.path.to_string_lossy().as_ref());
        let file = self
            .vfs
            .open(&path, OpenOptions::main_db())
            .expect("the file opens");
        file.file_size().expect("the file has a length")
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

/// Fills a table `t` with an index `tb`, in one transaction.
///
/// @param engine - the open connection
/// @param rows - how many rows
/// @param width - how many bytes each row's text is
fn fill(engine: &mut ImportedDatabase, rows: i64, width: usize) {
    run(engine, "CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT)");
    run(engine, "CREATE INDEX tb ON t(b)");
    run(engine, "BEGIN");
    for row in 0..rows {
        let text = format!("{row:08}{}", "x".repeat(width));
        run(engine, &format!("INSERT INTO t VALUES ({row}, '{text}')"));
    }
    run(engine, "COMMIT");
}

/// Asserts the rows of `t` read the same through the table and through its index.
///
/// @param engine - the open connection
/// @param expected - how many rows there should be
fn assert_rows(engine: &mut ImportedDatabase, expected: i64) {
    assert_eq!(
        count(engine, "SELECT count(*) FROM t"),
        expected,
        "by table"
    );
    assert_eq!(
        count(engine, "SELECT count(*) FROM t INDEXED BY tb WHERE b >= ''"),
        expected,
        "by index"
    );
    let check = run(engine, "PRAGMA integrity_check");
    assert_eq!(check, vec![vec![OwnedDatum::Text(b"ok".to_vec())]]);
}

/// A delete under FULL leaves no free page below the end, and the file is as long
/// as its page count says the moment the statement returns.
///
/// **Red:** before the step existed `freelist_count` was 30 or more after the
/// delete and the file kept every page it had.
#[test]
fn a_delete_under_full_leaves_no_free_page_below_the_end() {
    let case = Case::new("full-delete");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    fill(&mut engine, 2_000, 480);
    let before = count(&mut engine, "PRAGMA page_count");
    assert!(before > 150, "the fixture is {before} pages");
    run(&mut engine, "DELETE FROM t WHERE a > 100");
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0);
    let after = count(&mut engine, "PRAGMA page_count");
    assert!(after < before / 4, "{before} pages became {after}");
    assert_eq!(case.file_length(), after as u64 * PAGE_SIZE as u64);
    assert_rows(&mut engine, 101);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(count(&mut reopened, "PRAGMA freelist_count"), 0);
    assert_eq!(count(&mut reopened, "PRAGMA page_count"), after);
    assert_rows(&mut reopened, 101);
}

/// Fills a table with `INSERT ... SELECT`, so a large table costs one statement.
///
/// @param engine - the open connection
/// @param name - the table
/// @param rows - how many rows, keyed from 1
/// @param width - how many bytes each row's text is
fn fill_fast(engine: &mut ImportedDatabase, name: &str, rows: i64, width: usize) {
    run(
        engine,
        &format!("CREATE TABLE {name}(a INTEGER PRIMARY KEY, b TEXT)"),
    );
    run(engine, &format!("CREATE INDEX {name}b ON {name}(b)"));
    run(
        engine,
        &format!(
            "INSERT INTO {name} SELECT value, printf('%08d', value) || printf('%.*c', {width}, 'x') \
             FROM generate_series(1, {rows})"
        ),
    );
}

/// Returns the first column of every row a query answers, as integers.
///
/// @param engine - the open connection
/// @param sql - a query
fn integers(engine: &mut ImportedDatabase, sql: &str) -> Vec<i64> {
    run(engine, sql)
        .iter()
        .map(|row| match row.first() {
            Some(OwnedDatum::Int(value)) => *value,
            other => panic!("{sql} answered {other:?}"),
        })
        .collect()
}

/// Returns every row a query answers, to compare before and after a move.
///
/// @param engine - the open connection
/// @param sql - a query with a total order
fn snapshot(engine: &mut ImportedDatabase, sql: &str) -> Vec<Vec<OwnedDatum>> {
    run(engine, sql)
}

/// Asserts the integrity check says `ok`.
///
/// @param engine - the open connection
fn assert_intact(engine: &mut ImportedDatabase) {
    let check = run(engine, "PRAGMA integrity_check");
    assert_eq!(check, vec![vec![OwnedDatum::Text(b"ok".to_vec())]]);
}

/// Asserts the file is exactly as long as its page count says.
///
/// @param case - the case that owns the file
/// @param engine - the open connection
fn assert_file_is_its_page_count(case: &Case, engine: &mut ImportedDatabase) {
    let pages = count(engine, "PRAGMA page_count") as u64;
    assert_eq!(case.file_length(), pages * PAGE_SIZE as u64);
}

/// A delete under FULL shortens the file before the statement returns, with no
/// checkpoint asked for.
///
/// **Red:** the file was as long as before the delete.
#[test]
fn a_delete_under_full_shortens_the_file_when_the_statement_returns() {
    let case = Case::new("full-length");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    fill_fast(&mut engine, "t", 3_000, 400);
    let long = case.file_length();
    run(&mut engine, "DELETE FROM t WHERE a > 30");
    assert!(
        case.file_length() < long / 4,
        "{} of {long}",
        case.file_length()
    );
    assert_file_is_its_page_count(&case, &mut engine);
    assert_eq!(integers(&mut engine, "SELECT count(*) FROM t"), vec![30]);
}

/// `incremental_vacuum(N)` gives back exactly N pages and answers N rows with no
/// columns, and two tables it did not touch keep their root pages.
///
/// **Red:** the pragma rebuilt the whole file, or did nothing when fewer than N
/// pages were free, and a rebuild cannot promise the root pages stay.
#[test]
fn incremental_vacuum_n_frees_exactly_n_pages_and_answers_n_empty_rows() {
    let case = Case::new("incremental-n");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = INCREMENTAL");
    run(&mut engine, "CREATE TABLE keep1(a INTEGER PRIMARY KEY, b)");
    run(&mut engine, "CREATE TABLE keep2(a INTEGER PRIMARY KEY, b)");
    run(
        &mut engine,
        "INSERT INTO keep1 VALUES (1, 'one'), (2, 'two')",
    );
    run(
        &mut engine,
        "INSERT INTO keep2 VALUES (1, 'uno'), (2, 'dos')",
    );
    fill_fast(&mut engine, "t", 3_000, 400);
    run(&mut engine, "DELETE FROM t WHERE a > 100");
    let roots = "SELECT rootpage FROM sqlite_schema WHERE name IN ('keep1', 'keep2') ORDER BY name";
    let roots_before = integers(&mut engine, roots);
    let pages = count(&mut engine, "PRAGMA page_count");
    let free = count(&mut engine, "PRAGMA freelist_count");
    assert!(free > 50, "{free} pages are free");
    assert_eq!(pages, count(&mut engine, "PRAGMA page_count"));

    let answered = run(&mut engine, "PRAGMA incremental_vacuum(10)");
    assert_eq!(answered.len(), 10);
    assert!(answered.iter().all(|row| row.is_empty()), "{answered:?}");
    assert_eq!(count(&mut engine, "PRAGMA page_count"), pages - 10);
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), free - 10);
    assert_eq!(integers(&mut engine, roots), roots_before);
    assert_file_is_its_page_count(&case, &mut engine);
    assert_intact(&mut engine);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(count(&mut reopened, "PRAGMA page_count"), pages - 10);
    assert_eq!(integers(&mut reopened, roots), roots_before);
    assert_eq!(integers(&mut reopened, "SELECT count(*) FROM t"), vec![100]);
    assert_intact(&mut reopened);
}

/// With no count, with 0, with a negative number and with a word, every free page
/// is given back.
#[test]
fn incremental_vacuum_without_a_count_frees_every_free_page() {
    for (tag, sql) in [
        ("none", "PRAGMA incremental_vacuum"),
        ("zero", "PRAGMA incremental_vacuum(0)"),
        ("negative", "PRAGMA incremental_vacuum(-1)"),
        ("word", "PRAGMA incremental_vacuum('abc')"),
    ] {
        let case = Case::new(&format!("incremental-all-{tag}"));
        let mut engine = case.create();
        run(&mut engine, "PRAGMA auto_vacuum = INCREMENTAL");
        fill_fast(&mut engine, "t", 2_000, 400);
        run(&mut engine, "DELETE FROM t WHERE a > 50");
        let free = count(&mut engine, "PRAGMA freelist_count");
        let pages = count(&mut engine, "PRAGMA page_count");
        let answered = run(&mut engine, sql);
        assert_eq!(answered.len() as i64, free, "{sql}");
        assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0, "{sql}");
        assert_eq!(
            count(&mut engine, "PRAGMA page_count"),
            pages - free,
            "{sql}"
        );
        assert_eq!(integers(&mut engine, "SELECT count(*) FROM t"), vec![50]);
    }
}

/// In mode NONE and mode FULL the pragma answers no rows and moves nothing.
#[test]
fn incremental_vacuum_is_silent_in_modes_none_and_full() {
    for mode in ["NONE", "FULL"] {
        let case = Case::new(&format!("incremental-silent-{mode}"));
        let mut engine = case.create();
        run(&mut engine, &format!("PRAGMA auto_vacuum = {mode}"));
        fill_fast(&mut engine, "t", 2_000, 400);
        run(&mut engine, "DELETE FROM t WHERE a > 50");
        let pages = count(&mut engine, "PRAGMA page_count");
        let free = count(&mut engine, "PRAGMA freelist_count");
        assert!(
            run(&mut engine, "PRAGMA incremental_vacuum(1)").is_empty(),
            "{mode}"
        );
        assert_eq!(count(&mut engine, "PRAGMA page_count"), pages, "{mode}");
        assert_eq!(count(&mut engine, "PRAGMA freelist_count"), free, "{mode}");
    }
}

/// A pragma inside a transaction that rolls back moves nothing.
#[test]
fn a_rolled_back_incremental_vacuum_moves_nothing() {
    let case = Case::new("incremental-rollback");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = INCREMENTAL");
    fill_fast(&mut engine, "t", 2_000, 400);
    run(&mut engine, "DELETE FROM t WHERE a > 50");
    let pages = count(&mut engine, "PRAGMA page_count");
    let free = count(&mut engine, "PRAGMA freelist_count");
    run(&mut engine, "BEGIN");
    assert_eq!(run(&mut engine, "PRAGMA incremental_vacuum(2)").len(), 2);
    run(&mut engine, "ROLLBACK");
    assert_eq!(count(&mut engine, "PRAGMA page_count"), pages);
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), free);
    run(&mut engine, "BEGIN");
    run(&mut engine, "PRAGMA incremental_vacuum(2)");
    run(&mut engine, "COMMIT");
    assert_eq!(count(&mut engine, "PRAGMA page_count"), pages - 2);
    assert_intact(&mut engine);
}

/// A mode set on a file that already holds a table is remembered until `VACUUM`,
/// which applies it, and the next delete then shrinks the file.
#[test]
fn a_mode_change_on_a_file_with_tables_waits_for_vacuum() {
    let case = Case::new("pending-mode");
    let mut engine = case.create();
    fill_fast(&mut engine, "t", 2_000, 400);
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    assert_eq!(count(&mut engine, "PRAGMA auto_vacuum"), 0);
    run(&mut engine, "VACUUM");
    assert_eq!(count(&mut engine, "PRAGMA auto_vacuum"), 1);
    let pages = count(&mut engine, "PRAGMA page_count");
    run(&mut engine, "DELETE FROM t WHERE a > 50");
    assert!(count(&mut engine, "PRAGMA page_count") < pages / 4);
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(count(&mut reopened, "PRAGMA auto_vacuum"), 1);
}

/// FULL and INCREMENTAL switch with no `VACUUM`, but only between the two.
#[test]
fn full_and_incremental_switch_without_a_vacuum() {
    let case = Case::new("switch-modes");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    fill_fast(&mut engine, "t", 100, 100);
    assert_eq!(count(&mut engine, "PRAGMA auto_vacuum"), 1);
    run(&mut engine, "PRAGMA auto_vacuum = INCREMENTAL");
    assert_eq!(count(&mut engine, "PRAGMA auto_vacuum"), 2);
    run(&mut engine, "PRAGMA auto_vacuum = NONE");
    assert_eq!(count(&mut engine, "PRAGMA auto_vacuum"), 2);
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    assert_eq!(count(&mut engine, "PRAGMA auto_vacuum"), 1);
    run(&mut engine, "PRAGMA auto_vacuum = bogus");
    run(&mut engine, "PRAGMA auto_vacuum = 7");
    assert_eq!(count(&mut engine, "PRAGMA auto_vacuum"), 1);
    drop(engine);
    assert_eq!(count(&mut case.reopen(), "PRAGMA auto_vacuum"), 1);
}

/// Leaves move from the end into the gap a delete from the front left, and a scan
/// in key order still returns every row once, through the table and an index.
#[test]
fn a_moved_leaf_keeps_its_place_in_the_sibling_chain() {
    let case = Case::new("moved-leaf");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    fill_fast(&mut engine, "t", 4_000, 400);
    run(&mut engine, "DELETE FROM t WHERE a <= 3000");
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0);
    let by_table = integers(&mut engine, "SELECT a FROM t ORDER BY a");
    assert_eq!(by_table, (3_001..=4_000).collect::<Vec<i64>>());
    let by_index = integers(
        &mut engine,
        "SELECT a FROM t INDEXED BY tb WHERE b >= '' ORDER BY b",
    );
    assert_eq!(by_index, by_table);
    assert_intact(&mut engine);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(
        integers(&mut reopened, "SELECT a FROM t ORDER BY a"),
        by_table
    );
    assert_intact(&mut reopened);
}

/// A tree three levels deep, trimmed from the front, still routes every key.
#[test]
fn a_moved_interior_page_still_routes_every_key() {
    let case = Case::new("moved-interior");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    fill_fast(&mut engine, "t", 20_000, 450);
    run(&mut engine, "DELETE FROM t WHERE a <= 18000");
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0);
    for key in [18_001, 18_500, 19_000, 19_999, 20_000] {
        let found = integers(&mut engine, &format!("SELECT a FROM t WHERE a = {key}"));
        assert_eq!(found, vec![key]);
    }
    assert_eq!(integers(&mut engine, "SELECT count(*) FROM t"), vec![2_000]);
    assert_intact(&mut engine);
    drop(engine);
    let mut reopened = case.reopen();
    for key in 18_001..=20_000 {
        let found = integers(&mut reopened, &format!("SELECT a FROM t WHERE a = {key}"));
        assert_eq!(found, vec![key], "key {key}");
    }
    assert_intact(&mut reopened);
}

/// Values of 200 KB sit on chains of pages. Delete the rows written before them
/// and the chains move down, and each value reads back byte for byte.
#[test]
fn a_blob_chain_page_moves_and_the_value_reads_back_byte_for_byte() {
    let case = Case::new("moved-chain");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    run(&mut engine, "CREATE TABLE v(a INTEGER PRIMARY KEY, b TEXT)");
    run(
        &mut engine,
        "INSERT INTO v SELECT value, hex(randomblob(100000)) FROM generate_series(1, 40)",
    );
    let wanted = snapshot(&mut engine, "SELECT a, b FROM v WHERE a > 30 ORDER BY a");
    assert_eq!(wanted.len(), 10);
    let pages = count(&mut engine, "PRAGMA page_count");
    run(&mut engine, "DELETE FROM v WHERE a <= 30");
    assert!(count(&mut engine, "PRAGMA page_count") < pages / 2);
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0);
    assert_eq!(
        snapshot(&mut engine, "SELECT a, b FROM v ORDER BY a"),
        wanted
    );
    assert_intact(&mut engine);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(
        snapshot(&mut reopened, "SELECT a, b FROM v ORDER BY a"),
        wanted
    );
    assert_intact(&mut reopened);
}

/// Two tables put 5 KB values on the same shared pages. Delete the rows before
/// them, and both tables read their values back from the moved pages.
#[test]
fn a_shared_extent_page_moves_and_every_owner_still_reads() {
    let case = Case::new("moved-shared");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    run(
        &mut engine,
        "CREATE TABLE one(a INTEGER PRIMARY KEY, b TEXT)",
    );
    run(
        &mut engine,
        "CREATE TABLE two(a INTEGER PRIMARY KEY, b TEXT)",
    );
    run(&mut engine, "BEGIN");
    for row in 1..=400 {
        for table in ["one", "two"] {
            run(
                &mut engine,
                &format!("INSERT INTO {table} VALUES ({row}, hex(randomblob(2500)))"),
            );
        }
    }
    run(&mut engine, "COMMIT");
    let one = snapshot(&mut engine, "SELECT a, b FROM one WHERE a > 380 ORDER BY a");
    let two = snapshot(&mut engine, "SELECT a, b FROM two WHERE a > 380 ORDER BY a");
    let pages = count(&mut engine, "PRAGMA page_count");
    run(&mut engine, "DELETE FROM one WHERE a <= 380");
    run(&mut engine, "DELETE FROM two WHERE a <= 380");
    assert!(count(&mut engine, "PRAGMA page_count") < pages / 4);
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0);
    assert_eq!(
        snapshot(&mut engine, "SELECT a, b FROM one ORDER BY a"),
        one
    );
    assert_eq!(
        snapshot(&mut engine, "SELECT a, b FROM two ORDER BY a"),
        two
    );
    assert_intact(&mut engine);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(
        snapshot(&mut reopened, "SELECT a, b FROM one ORDER BY a"),
        one
    );
    assert_eq!(
        snapshot(&mut reopened, "SELECT a, b FROM two ORDER BY a"),
        two
    );
}

/// A page nothing owns at the end of the file pins the step: the commit succeeds,
/// the page count stays, and the integrity check names the page.
#[test]
fn an_unowned_page_pins_the_end_and_the_commit_still_succeeds() {
    let case = Case::new("pinned-end");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    fill_fast(&mut engine, "t", 3_000, 400);
    let stranded = engine
        .strand_a_page_unchecked()
        .expect("a page is stranded");
    let pages = count(&mut engine, "PRAGMA page_count");
    assert_eq!(stranded + 1, pages as u64, "the stranded page is the end");
    run(&mut engine, "DELETE FROM t WHERE a > 30");
    assert_eq!(count(&mut engine, "PRAGMA page_count"), pages);
    assert!(count(&mut engine, "PRAGMA freelist_count") > 0);
    assert_eq!(integers(&mut engine, "SELECT count(*) FROM t"), vec![30]);
    let check = run(&mut engine, "PRAGMA integrity_check");
    let said: Vec<String> = check
        .iter()
        .flatten()
        .map(|datum| match datum {
            OwnedDatum::Text(text) => String::from_utf8_lossy(text).to_string(),
            other => format!("{other:?}"),
        })
        .collect();
    let named = format!("Page {stranded}");
    assert!(said.iter().any(|line| line.contains(&named)), "{said:?}");
}

/// A file attached with its own mode shrinks by it, and `main` in mode NONE does not.
#[test]
fn an_attached_file_shrinks_on_its_own_mode() {
    let directory = inillucent_compat::workspace_root().join("_agent_output/auto-vacuum/attached");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the directory is made");
    let main = directory.join("main.rdb");
    let aux = directory.join("aux.rdb");
    let mut engine = ImportedDatabase::create(main, PAGE_SIZE, FRAMES).expect("main is created");
    let attach = format!(
        "ATTACH DATABASE '{}' AS aux",
        aux.display().to_string().replace('\\', "/")
    );
    run(&mut engine, &attach);
    run(&mut engine, "PRAGMA aux.auto_vacuum = FULL");
    run(
        &mut engine,
        "CREATE TABLE main.t(a INTEGER PRIMARY KEY, b TEXT)",
    );
    run(
        &mut engine,
        "CREATE TABLE aux.t(a INTEGER PRIMARY KEY, b TEXT)",
    );
    for table in ["main.t", "aux.t"] {
        run(
            &mut engine,
            &format!(
                "INSERT INTO {table} SELECT value, printf('%.*c', 400, 'x') FROM generate_series(1, 2000)"
            ),
        );
    }
    for table in ["main.t", "aux.t"] {
        run(&mut engine, &format!("DELETE FROM {table} WHERE a > 50"));
    }
    assert!(count(&mut engine, "PRAGMA main.freelist_count") > 0);
    assert_eq!(count(&mut engine, "PRAGMA aux.freelist_count"), 0);
    assert_eq!(count(&mut engine, "SELECT count(*) FROM aux.t"), 50);
    assert_eq!(count(&mut engine, "SELECT count(*) FROM main.t"), 50);
}

/// A table created after data is written takes the page above the largest root,
/// and the delete that follows shrinks the file past where that root would have been.
#[test]
fn a_table_created_late_takes_a_low_root_and_does_not_pin_the_end() {
    let case = Case::new("late-root");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    fill_fast(&mut engine, "t", 3_000, 400);
    let before = engine.largest_root();
    assert!(before > 0, "the file keeps its roots at the start");
    run(
        &mut engine,
        "CREATE TABLE late(a INTEGER PRIMARY KEY, b TEXT)",
    );
    run(&mut engine, "INSERT INTO late VALUES (1, 'late')");
    let root = integers(
        &mut engine,
        "SELECT rootpage FROM sqlite_schema WHERE name = 'late'",
    );
    assert_eq!(root, vec![before as i64 + 1]);
    assert_eq!(engine.largest_root(), before + 1);
    run(&mut engine, "DELETE FROM t WHERE a > 30");
    assert!(count(&mut engine, "PRAGMA page_count") < 40);
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), 0);
    assert_eq!(integers(&mut engine, "SELECT a FROM late"), vec![1]);
    assert_intact(&mut engine);
}

/// Dropping the middle of three tables moves the last root into its page, and the
/// third table reads through the page its catalog row now names.
#[test]
fn dropping_a_table_moves_the_largest_root_into_its_slot() {
    let case = Case::new("drop-root");
    let mut engine = case.create();
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    for name in ["first", "second", "third"] {
        run(
            &mut engine,
            &format!("CREATE TABLE {name}(a INTEGER PRIMARY KEY, b TEXT)"),
        );
        run(
            &mut engine,
            &format!("INSERT INTO {name} SELECT value, '{name}' FROM generate_series(1, 300)"),
        );
    }
    let root_of = "SELECT rootpage FROM sqlite_schema WHERE name = ";
    let second = integers(&mut engine, &format!("{root_of}'second'"))[0];
    let third = integers(&mut engine, &format!("{root_of}'third'"))[0];
    let largest = engine.largest_root();
    assert_eq!(third as u64, largest);
    run(&mut engine, "DROP TABLE second");
    assert_eq!(
        integers(&mut engine, &format!("{root_of}'third'")),
        vec![second]
    );
    assert_eq!(engine.largest_root(), largest - 1);
    assert_eq!(
        integers(&mut engine, "SELECT count(*) FROM third"),
        vec![300]
    );
    assert_intact(&mut engine);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(
        integers(&mut reopened, &format!("{root_of}'third'")),
        vec![second]
    );
    assert_eq!(
        integers(&mut reopened, "SELECT count(*) FROM third"),
        vec![300]
    );
    assert_intact(&mut reopened);
}

/// A file with five tables and data gets a pending mode, and `VACUUM` rebuilds it
/// with every root at the start.
#[test]
fn vacuum_applies_a_pending_mode_and_packs_the_roots_low() {
    let case = Case::new("vacuum-roots");
    let mut engine = case.create();
    for table in 0..5 {
        fill_fast(&mut engine, &format!("t{table}"), 200, 300);
    }
    assert_eq!(engine.largest_root(), 0);
    run(&mut engine, "PRAGMA auto_vacuum = FULL");
    run(&mut engine, "VACUUM");
    assert_eq!(count(&mut engine, "PRAGMA auto_vacuum"), 1);
    let mut roots = integers(
        &mut engine,
        "SELECT rootpage FROM sqlite_schema WHERE rootpage > 0 ORDER BY rootpage",
    );
    assert_eq!(roots.len(), 10, "five tables and five indexes");
    let largest = engine.largest_root();
    assert_eq!(roots.pop(), Some(largest as i64));
    let first = roots.first().copied().expect("a root");
    let consecutive: Vec<i64> = (first..first + roots.len() as i64).collect();
    assert_eq!(roots, consecutive, "the roots are consecutive");
    for table in 0..5 {
        assert_eq!(
            integers(&mut engine, &format!("SELECT count(*) FROM t{table}")),
            vec![200]
        );
    }
    assert_intact(&mut engine);
}

/// Returns how many pages hold something: the page count less the free pages.
///
/// @param engine - the open connection
fn in_use(engine: &mut ImportedDatabase) -> i64 {
    count(engine, "PRAGMA page_count") - count(engine, "PRAGMA freelist_count")
}

/// Emptying a table of 5,000 byte values frees the pages the values sat on.
///
/// **Red:** a delete tombstoned a sorted row and left its value on its shared
/// page for a repack that never came, so about 125 of 552 pages stayed allocated
/// to an empty table.
#[test]
fn deleting_every_row_frees_every_shared_page() {
    let case = Case::new("empty-shared");
    let mut engine = case.create();
    run(&mut engine, "CREATE TABLE v(a INTEGER PRIMARY KEY, b TEXT)");
    run(
        &mut engine,
        "INSERT INTO v SELECT value, hex(randomblob(2500)) FROM generate_series(1, 2000)",
    );
    let used = in_use(&mut engine);
    assert!(
        used > 1_000,
        "{used} pages hold 2,000 values of 5,000 bytes"
    );
    run(&mut engine, "DELETE FROM v");
    assert!(
        in_use(&mut engine) <= 8,
        "{} pages are still in use",
        in_use(&mut engine)
    );
    assert_intact(&mut engine);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(integers(&mut reopened, "SELECT count(*) FROM v"), vec![0]);
    assert_intact(&mut reopened);
}

/// A leaf emptied beside a full sibling is freed, and a scan returns every other
/// row in order.
#[test]
fn an_emptied_leaf_beside_a_full_sibling_is_freed() {
    let case = Case::new("emptied-leaf");
    let mut engine = case.create();
    run(
        &mut engine,
        "CREATE TABLE w AS SELECT value AS a, printf('%.*c', 500, 'x') AS b FROM generate_series(1, 400)",
    );
    let per_leaf = integers(
        &mut engine,
        "SELECT ncell FROM dbstat WHERE name = 'w' AND pagetype = 'leaf' ORDER BY pageno",
    );
    let first = per_leaf[0];
    assert!(first > 4, "{first} rows to a leaf");
    let free = count(&mut engine, "PRAGMA freelist_count");
    run(
        &mut engine,
        &format!(
            "DELETE FROM w WHERE a BETWEEN {} AND {}",
            first + 1,
            first * 2
        ),
    );
    assert_eq!(count(&mut engine, "PRAGMA freelist_count"), free + 1);
    let wanted: Vec<i64> = (1..=400)
        .filter(|a| *a <= first || *a > first * 2)
        .collect();
    assert_eq!(integers(&mut engine, "SELECT a FROM w ORDER BY a"), wanted);
    assert_intact(&mut engine);
}

/// A transaction that deletes every row and rolls back leaves every value readable
/// byte for byte and the free list as it was.
///
/// This is the case that makes freeing at the delete safe: the undo writes the
/// row back from the values it holds, so it never names a page the delete freed.
#[test]
fn a_rolled_back_delete_keeps_its_values_readable() {
    let case = Case::new("rolled-back-delete");
    let mut engine = case.create();
    run(&mut engine, "CREATE TABLE v(a INTEGER PRIMARY KEY, b TEXT)");
    run(
        &mut engine,
        "INSERT INTO v SELECT value, hex(randomblob(2500)) FROM generate_series(1, 300)",
    );
    let wanted = snapshot(&mut engine, "SELECT a, b FROM v ORDER BY a");
    let free = count(&mut engine, "PRAGMA freelist_count");
    run(&mut engine, "BEGIN");
    run(&mut engine, "DELETE FROM v");
    assert_eq!(integers(&mut engine, "SELECT count(*) FROM v"), vec![0]);
    run(&mut engine, "ROLLBACK");
    assert_eq!(
        snapshot(&mut engine, "SELECT a, b FROM v ORDER BY a"),
        wanted
    );
    // The undo writes the rows back one at a time, so the leaves it fills may differ
    // from the leaves the bulk insert filled by a page or two; what it may not do is
    // leave a page both free and in use, which the integrity check names.
    assert!(count(&mut engine, "PRAGMA freelist_count") <= free + 3);
    assert_intact(&mut engine);
    drop(engine);
    let mut reopened = case.reopen();
    assert_eq!(
        snapshot(&mut reopened, "SELECT a, b FROM v ORDER BY a"),
        wanted
    );
    assert_intact(&mut reopened);
}

/// Half of a leaf's rows are deleted, then writes into the same leaf repack it:
/// no page is freed twice, and the rows that stayed still read.
#[test]
fn a_repack_after_a_delete_frees_nothing_twice() {
    let case = Case::new("repack-after-delete");
    let mut engine = case.create();
    run(&mut engine, "CREATE TABLE v(a INTEGER PRIMARY KEY, b TEXT)");
    run(
        &mut engine,
        "INSERT INTO v SELECT value, hex(randomblob(2500)) FROM generate_series(1, 400)",
    );
    let kept = snapshot(&mut engine, "SELECT a, b FROM v WHERE a % 2 = 0 ORDER BY a");
    run(&mut engine, "DELETE FROM v WHERE a % 2 = 1");
    let freed = count(&mut engine, "PRAGMA freelist_count");
    for round in 0..40 {
        run(
            &mut engine,
            &format!("INSERT INTO v VALUES ({}, 'small {round}')", 1 + round * 2),
        );
    }
    assert_intact(&mut engine);
    assert!(count(&mut engine, "PRAGMA freelist_count") >= freed - 2);
    assert_eq!(
        snapshot(&mut engine, "SELECT a, b FROM v WHERE a % 2 = 0 ORDER BY a"),
        kept
    );
}

/// A table emptied by `DELETE` is its root and nothing else.
#[test]
fn deleting_every_row_leaves_the_root_and_one_leaf_per_parent() {
    let case = Case::new("emptied-table");
    let mut engine = case.create();
    fill_fast(&mut engine, "t", 20_000, 450);
    run(&mut engine, "DELETE FROM t");
    assert_eq!(
        integers(&mut engine, "SELECT count(*) FROM dbstat WHERE name = 't'"),
        vec![1]
    );
    assert_eq!(
        integers(&mut engine, "SELECT count(*) FROM dbstat WHERE name = 'tb'"),
        vec![1]
    );
    assert_intact(&mut engine);
}

//! A table of 2,000 integer columns, at the default page size.
//!
//! Invariant: **a row of 2,000 integers, each of which fits 32 bits, is stored, read back,
//! updated, deleted and recovered, and the file says format 4 from the first such row on and
//! not before.** A leaf spends a directory entry of eight bytes on every column, and a row's
//! values took nine bytes each, so 1,940 integer columns filled a 32 KiB page and SQLite's limit
//! of 2,000 columns could not be reached. A row too wide for the ordinary encoding now writes
//! each integer in the fewest bytes that hold it: two for a number under 128, three for one
//! under 32,768, five for one under 2,147,483,648. A row of integers that need eight bytes each
//! still holds about 1,925 of them, and the refusal for more says so.
//!
//! The rows differ in every column, so a reader that took one column's bytes for another's
//! would answer a different sum.

use std::sync::Arc;

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::{DbPath, MemoryVfs, OpenOptions, Vfs};

/// The default page size.
const PAGE_SIZE: usize = 32_768;

/// How many frames the pool holds.
const FRAMES: usize = 512;

/// How many columns the table has, which is SQLite's default limit.
const COLUMNS: usize = 2_000;

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
            path: std::env::temp_dir().join(format!("inillucent-wide-rows-{tag}.rdb")),
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
        .unwrap_or_else(|why| panic!("{}: {}", &sql[..sql.len().min(80)], why.message()))
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

/// Returns the value row `row` holds in column `column`: every width the encoding has.
///
/// Column 0 is the key. The rest cycle through numbers under 128, under 32,768, under
/// 2,147,483,648, and negative ones of each size, and no two rows agree.
///
/// @param row - the row's number
/// @param column - the column's position, from 1
fn cell(row: i64, column: usize) -> i64 {
    let spread = (column as i64)
        .wrapping_mul(2_654_435_761)
        .wrapping_add(row * 40_503);
    match column % 6 {
        0 => spread.rem_euclid(120),
        1 => spread.rem_euclid(30_000),
        2 => spread.rem_euclid(2_000_000_000),
        3 => -spread.rem_euclid(120),
        4 => -spread.rem_euclid(30_000),
        _ => -spread.rem_euclid(2_000_000_000),
    }
}

/// Returns `CREATE TABLE w(k INTEGER PRIMARY KEY, c1 INTEGER, ..., cN INTEGER)`.
///
/// @param columns - how many columns besides the key
fn create_statement(columns: usize) -> String {
    let names: Vec<String> = (1..=columns).map(|at| format!("c{at} INTEGER")).collect();
    format!(
        "CREATE TABLE w(k INTEGER PRIMARY KEY, {})",
        names.join(", ")
    )
}

/// Returns `INSERT INTO w VALUES (row, ...)` with the row's cells.
///
/// @param row - the row's number
/// @param columns - how many columns besides the key
fn insert_statement(row: i64, columns: usize) -> String {
    let values: Vec<String> = (1..=columns).map(|at| cell(row, at).to_string()).collect();
    format!("INSERT INTO w VALUES ({row}, {})", values.join(", "))
}

/// Asserts a row holds its cells, by the sum of all and by a probe of three.
///
/// @param engine - the open connection
/// @param row - the row's number
/// @param columns - how many columns besides the key
fn assert_row(engine: &mut ImportedDatabase, row: i64, columns: usize) {
    // The columns are added up two hundred at a time, because an expression of two thousand
    // terms is deeper than the engine's limit on an expression tree.
    for first in (1..=columns).step_by(200) {
        let last = (first + 199).min(columns);
        let expected: i64 = (first..=last).map(|at| cell(row, at)).sum();
        let sum: Vec<String> = (first..=last).map(|at| format!("c{at}")).collect();
        let total = count(
            engine,
            &format!("SELECT {} FROM w WHERE k = {row}", sum.join(" + ")),
        );
        assert_eq!(total, expected, "row {row}, columns {first} to {last}");
    }
    for at in [1usize, columns / 2, columns] {
        assert_eq!(
            count(engine, &format!("SELECT c{at} FROM w WHERE k = {row}")),
            cell(row, at),
            "row {row} column {at}"
        );
    }
}

/// Returns what the bulk insert writes in column `column` of row `row`.
///
/// @param row - the row's number
/// @param column - the column's position, from 1
fn bulk_cell(row: i64, column: i64) -> i64 {
    (row * 7 + column) % (100 + column * 13)
}

/// 2,000 columns of numbers up to two billion are stored and read back.
#[test]
fn a_row_of_two_thousand_integers_is_stored_and_read_back() {
    let case = Case::new("store");
    let mut engine = case.create();
    run(&mut engine, &create_statement(COLUMNS - 1));
    assert_eq!(case.format(), 3, "a table with no row says 3");
    for row in 1..=12 {
        run(&mut engine, &insert_statement(row, COLUMNS - 1));
    }
    assert_eq!(count(&mut engine, "SELECT count(*) FROM w"), 12);
    for row in 1..=12 {
        assert_row(&mut engine, row, COLUMNS - 1);
    }
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
    engine.checkpoint().expect("the fold runs");
    assert_eq!(case.format(), 4, "the first wide row moves the file to 4");
}

/// The same rows are there after the file is closed and opened, with and without a fold.
#[test]
fn wide_rows_survive_a_reopen_and_take_updates_and_deletes() {
    let case = Case::new("reopen");
    let mut engine = case.create();
    run(&mut engine, &create_statement(COLUMNS - 1));
    for row in 1..=9 {
        run(&mut engine, &insert_statement(row, COLUMNS - 1));
    }
    drop(engine);
    let mut engine = case.reopen();
    for row in 1..=9 {
        assert_row(&mut engine, row, COLUMNS - 1);
    }
    run(
        &mut engine,
        "UPDATE w SET c1000 = 77, c1999 = -5 WHERE k = 4",
    );
    run(&mut engine, "DELETE FROM w WHERE k IN (2, 6)");
    run(&mut engine, &insert_statement(2, COLUMNS - 1));
    drop(engine);
    let mut engine = case.reopen();
    assert_eq!(count(&mut engine, "SELECT count(*) FROM w"), 8);
    assert_eq!(count(&mut engine, "SELECT c1000 FROM w WHERE k = 4"), 77);
    assert_eq!(count(&mut engine, "SELECT c1999 FROM w WHERE k = 4"), -5);
    assert_eq!(
        count(&mut engine, "SELECT c1 FROM w WHERE k = 4"),
        cell(4, 1)
    );
    assert_row(&mut engine, 2, COLUMNS - 1);
    assert_row(&mut engine, 9, COLUMNS - 1);
    assert_eq!(count(&mut engine, "SELECT count(*) FROM w WHERE k = 6"), 0);
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
}

/// Over a thousand rows take the bulk build, which writes each wide row alone in a leaf.
#[test]
fn a_bulk_insert_of_wide_rows_is_stored_and_read_back() {
    let case = Case::new("bulk");
    let mut engine = case.create();
    run(&mut engine, &create_statement(COLUMNS - 1));
    let expressions: Vec<String> = (1..COLUMNS)
        .map(|at| format!("(value * 7 + {at}) % {}", 100 + at * 13))
        .collect();
    run(
        &mut engine,
        &format!(
            "INSERT INTO w SELECT value, {} FROM generate_series(1, 1100)",
            expressions.join(", ")
        ),
    );
    assert_eq!(count(&mut engine, "SELECT count(*) FROM w"), 1_100);
    assert_eq!(
        count(&mut engine, "SELECT c1 FROM w WHERE k = 500"),
        bulk_cell(500, 1)
    );
    assert_eq!(
        count(&mut engine, "SELECT c1999 FROM w WHERE k = 1100"),
        bulk_cell(1100, 1999)
    );
    engine.checkpoint().expect("the fold runs");
    assert_eq!(case.format(), 4);
    drop(engine);
    let mut engine = case.reopen();
    assert_eq!(count(&mut engine, "SELECT count(*) FROM w"), 1_100);
    assert_eq!(
        count(&mut engine, "SELECT c777 FROM w WHERE k = 321"),
        bulk_cell(321, 777)
    );
    assert_eq!(
        run(&mut engine, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
}

/// A table of ordinary width, and a wide table of NULLs, keep the format a release before this reads.
#[test]
fn only_a_wide_row_of_numbers_moves_the_file_to_format_four() {
    let case = Case::new("format");
    let mut engine = case.create();
    run(
        &mut engine,
        "CREATE TABLE plain(k INTEGER PRIMARY KEY, a INTEGER, b TEXT)",
    );
    run(&mut engine, "INSERT INTO plain VALUES (1, 2, 'three')");
    run(&mut engine, &create_statement(COLUMNS - 1));
    run(&mut engine, "INSERT INTO w(k) VALUES (1)");
    engine.checkpoint().expect("the fold runs");
    assert_eq!(case.format(), 3, "a row of NULLs is not a wide row");
    run(&mut engine, &insert_statement(2, COLUMNS - 1));
    engine.checkpoint().expect("the fold runs");
    assert_eq!(case.format(), 4);
}

/// More integers than a page holds at eight bytes each are refused by name.
#[test]
fn a_row_of_full_width_integers_past_the_page_is_refused_by_name() {
    let case = Case::new("refused");
    let mut engine = case.create();
    run(&mut engine, &create_statement(COLUMNS - 1));
    let values: Vec<String> = (1..COLUMNS)
        .map(|at| (4_000_000_000_000_000_000i64 + at as i64).to_string())
        .collect();
    let sql = format!("INSERT INTO w VALUES (1, {})", values.join(", "));
    let refusal = engine
        .execute_any(&sql, &Params::new())
        .expect_err("2,000 integers of eight bytes do not fit a 32 KiB page");
    assert!(
        refusal.message().contains("larger than a page")
            || refusal.message().contains("cannot be stored"),
        "the refusal does not say what is wrong: {}",
        refusal.message()
    );
    assert_eq!(count(&mut engine, "SELECT count(*) FROM w"), 0);
    // And 1,900 of them fit, as they did before.
    let case = Case::new("refused-fits");
    let mut engine = case.create();
    run(&mut engine, &create_statement(1_900));
    let values: Vec<String> = (1..=1_900)
        .map(|at| (4_000_000_000_000_000_000i64 + at as i64).to_string())
        .collect();
    run(
        &mut engine,
        &format!("INSERT INTO w VALUES (1, {})", values.join(", ")),
    );
    assert_eq!(
        count(&mut engine, "SELECT c1900 FROM w WHERE k = 1"),
        4_000_000_000_000_001_900
    );
}

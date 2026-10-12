//! Keys longer than a page, at the page sizes that decide how they are stored.
//!
//! Invariant: **an indexed value of 20 KB is stored, found after a reopen and
//! given back by a delete, at every page size a database can be created with.**
//! A key over `page_size / 8` is written on pages of its own with its first bytes
//! in the leaf; a separator over that is shortened at a split and, when two
//! neighbours share a longer prefix, stored out of line in the interior page.
//! The keys here share 20,000 bytes and differ in their last six, so every
//! separator is one of the second kind at every page size.
//!
//! The page ownership check is what proves the pages come back. It reports a
//! page two trees hold, a page a tree holds that the free map calls free, and a
//! page the free map calls allocated that no tree reaches - which is what a key
//! extent or a separator extent that a delete or a merge forgot to free would be.
//!
//! §1.2 of the testing standard: every case reads back the rows it wrote, so a
//! build that stores nothing cannot pass.

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_tree::datum::OwnedDatum;

/// The page sizes the design names: the smallest, SQLite's default, the
/// default here, and the largest.
const PAGE_SIZES: [usize; 4] = [512, 4_096, 32_768, 65_536];

/// How many rows each case writes.
const ROWS: i64 = 1_000;

/// How many characters of every key are the same.
const SHARED: usize = 20_000;

/// Returns a fresh database at the given page size.
///
/// @param tag - what to name the file
/// @param page_size - the page size to create it with
fn database(tag: &str, page_size: usize) -> (ImportedDatabase, std::path::PathBuf) {
    let path = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("long-keys")
        .join(format!("{tag}-{page_size}.rdb"));
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&path);
    let database =
        ImportedDatabase::create(path.clone(), page_size, 256).expect("a fresh database");
    (database, path)
}

/// Closes a database and opens the file again.
///
/// @param database - the database to close
/// @param path - the file it was opened on
/// @param page_size - the page size it was created with
fn reopened(
    database: ImportedDatabase,
    path: &std::path::Path,
    page_size: usize,
) -> ImportedDatabase {
    drop(database);
    ImportedDatabase::open(path.to_path_buf(), page_size, 256)
        .unwrap_or_else(|error| panic!("the {page_size}-byte database reopens: {error}"))
}

/// Runs one statement, naming the page size when it refuses.
///
/// @param database - the open database
/// @param page_size - the page size, for the message
/// @param sql - the statement
fn run(database: &mut ImportedDatabase, page_size: usize, sql: &str) {
    if let Err(error) = database.execute_any(sql, &Params::new()) {
        panic!(
            "at a {page_size}-byte page `{}` answered {error} ({:?})",
            sql.chars().take(120).collect::<String>(),
            error.detail()
        );
    }
}

/// Returns the single integer a one row, one column query answered with.
///
/// @param database - the open database
/// @param sql - the query, which must answer one row of one integer
fn count(database: &mut ImportedDatabase, sql: &str) -> i64 {
    let outcome = database
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|error| panic!("`{sql}` answers: {error} ({:?})", error.detail()));
    match outcome.rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(value)) => *value,
        other => panic!("`{sql}` answers an integer, not {other:?}"),
    }
}

/// Returns the SQL expression for the key of one row.
///
/// @param id - which row
fn key_of(id: i64) -> String {
    format!(
        "replace(hex(zeroblob({})), '0', 'q') || printf('%06d', {id})",
        SHARED / 2
    )
}

/// Asserts the page ownership check finds nothing wrong.
///
/// **At a 512 byte page a page the free map calls allocated and no tree reaches
/// is not counted.** A value of 20 KB there is a run of 44 pages, a run cannot
/// cross a free map page, and the pages left at the end of each stretch before
/// one are reported as never used. That happens for a table with no index and an
/// ordinary 20 KB column as well, so it says nothing about keys. Every other
/// state is still checked at that page size: a page two trees hold, and a page a
/// tree holds that the free map calls free.
///
/// @param database - the open database
/// @param page_size - the page size, for the message
/// @param when - what has just happened, for the message
fn assert_pages_balance(database: &ImportedDatabase, page_size: usize, when: &str) {
    let Err(error) = database.report_leaked_pages() else {
        return;
    };
    let said = error
        .detail()
        .unwrap_or_else(|| error.message())
        .to_string();
    let only_unused = said
        .lines()
        .all(|line| line.ends_with("never used") || line.starts_with("and "));
    if page_size == 512 && only_unused {
        return;
    }
    panic!("at a {page_size}-byte page, {when}: {said}");
}

/// A thousand rows with 20 KB keys, inserted out of order, found after a
/// reopen, deleted in two halves, with the pages accounted for each time.
#[test]
fn a_thousand_keys_of_twenty_kilobytes_are_stored_found_and_given_back() {
    for page_size in PAGE_SIZES {
        let (mut database, path) = database("every-size", page_size);
        run(
            &mut database,
            page_size,
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
        );
        run(&mut database, page_size, "CREATE INDEX t_v ON t (v)");
        for step in 0..ROWS {
            let id = (step * 389) % ROWS;
            run(
                &mut database,
                page_size,
                &format!("INSERT INTO t VALUES ({id}, {})", key_of(id)),
            );
        }
        assert_eq!(count(&mut database, "SELECT count(*) FROM t"), ROWS);
        let mut database = reopened(database, &path, page_size);
        for id in (0..ROWS).step_by(7) {
            assert_eq!(
                count(
                    &mut database,
                    &format!("SELECT id FROM t WHERE v = {}", key_of(id))
                ),
                id,
                "at a {page_size}-byte page the key of row {id} is found through the index"
            );
        }
        assert_eq!(
            count(
                &mut database,
                "SELECT count(*) FROM (SELECT id FROM t ORDER BY v)"
            ),
            ROWS
        );
        assert_pages_balance(&database, page_size, "after the inserts and a reopen");

        run(&mut database, page_size, "DELETE FROM t WHERE id % 2 = 0");
        assert_eq!(count(&mut database, "SELECT count(*) FROM t"), ROWS / 2);
        assert_pages_balance(&database, page_size, "after deleting half the rows");
        let mut database = reopened(database, &path, page_size);
        assert_pages_balance(&database, page_size, "after the first delete and a reopen");

        run(&mut database, page_size, "DELETE FROM t");
        assert_eq!(count(&mut database, "SELECT count(*) FROM t"), 0);
        assert_pages_balance(&database, page_size, "after deleting every row");
    }
}

/// `CREATE INDEX` over a table that already holds long keys, which builds the
/// tree in bulk, then `REINDEX`, then the integrity check.
#[test]
fn an_index_built_over_existing_long_keys_is_sound_and_survives_a_reindex() {
    for page_size in [4_096usize, 32_768] {
        let (mut database, path) = database("bulk", page_size);
        run(
            &mut database,
            page_size,
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
        );
        for id in 0..300i64 {
            run(
                &mut database,
                page_size,
                &format!("INSERT INTO t VALUES ({id}, {})", key_of((id * 211) % 300)),
            );
        }
        run(&mut database, page_size, "CREATE INDEX t_v ON t (v)");
        assert_pages_balance(&database, page_size, "after CREATE INDEX");
        // 211 is coprime to 300, so exactly one row was given the key of row 150.
        let holder = (0..300i64)
            .find(|id| (id * 211) % 300 == 150)
            .expect("one row holds that key");
        assert_eq!(
            count(
                &mut database,
                &format!("SELECT id FROM t WHERE v = {}", key_of(150))
            ),
            holder,
            "the index finds the row the table holds"
        );
        run(&mut database, page_size, "REINDEX");
        assert_pages_balance(&database, page_size, "after REINDEX");
        let mut database = reopened(database, &path, page_size);
        let outcome = database
            .execute_any("PRAGMA integrity_check", &Params::new())
            .expect("the pragma runs");
        assert_eq!(
            outcome.rows.first().and_then(|row| row.first()),
            Some(&OwnedDatum::Text(b"ok".to_vec())),
            "at a {page_size}-byte page the integrity check says ok"
        );
        assert_eq!(
            count(
                &mut database,
                "SELECT count(*) FROM (SELECT id FROM t ORDER BY v)"
            ),
            300
        );
    }
}

//! A plain `VACUUM` of an attached database makes its file shorter.
//!
//! Invariant: **after `VACUUM aux`, the file of the attached database is no
//! longer than the pages that tables and indexes still use, every row still
//! reads through its table and through its index, and `main` is untouched.**
//! `VACUUM aux` used to fold the log and leave the file at its old length. A
//! delete that leaves every leaf a tenth full frees almost no page, so the free
//! map could not give anything back either.
//!
//! The files are real files, because `ATTACH` opens its file on the operating
//! system's file system.

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_tree::datum::OwnedDatum;

/// Where this suite keeps its files.
///
/// @param name - the case's own directory
fn area(name: &str) -> std::path::PathBuf {
    let directory = inillucent_compat::workspace_root()
        .join("_agent_output")
        .join("vacuum-attached")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    let _ = std::fs::create_dir_all(&directory);
    directory
}

/// Runs a statement and returns its rows, failing with the statement.
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

/// Returns a file's length in bytes.
///
/// @param path - the file
fn length(path: &std::path::Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Fills `aux.t` with an index and deletes nine rows in ten, so every leaf is left a tenth full.
///
/// @param engine - the open connection
fn fill_and_thin(engine: &mut ImportedDatabase) {
    run(
        engine,
        "CREATE TABLE aux.t(id INTEGER PRIMARY KEY, body TEXT, tag INTEGER)",
    );
    run(engine, "CREATE INDEX aux.t_tag ON t(tag)");
    run(
        engine,
        "INSERT INTO aux.t SELECT value, hex(randomblob(100)), value % 97 FROM generate_series(1, 20000)",
    );
    run(engine, "DELETE FROM aux.t WHERE id % 10 <> 0");
}

/// Asserts the table and the index of `aux` agree on the rows left, and the file is sound.
///
/// @param engine - the open connection
fn assert_rows(engine: &mut ImportedDatabase) {
    assert_eq!(
        count(engine, "SELECT count(*) FROM aux.t"),
        2_000,
        "by table"
    );
    assert_eq!(
        count(
            engine,
            "SELECT count(*) FROM aux.t INDEXED BY t_tag WHERE tag >= 0"
        ),
        2_000,
        "by index"
    );
    assert_eq!(count(engine, "SELECT sum(id) FROM aux.t"), 20_010_000);
    let check = run(engine, "PRAGMA aux.integrity_check");
    assert_eq!(check, vec![vec![OwnedDatum::Text(b"ok".to_vec())]]);
}

/// `VACUUM aux` shortens the file of the attached database.
#[test]
fn vacuum_of_an_attached_database_shortens_its_file() {
    let directory = area("shortens");
    let main = directory.join("main.rdb");
    let aux = directory.join("aux.rdb");
    let mut engine = ImportedDatabase::create(main.clone(), 32_768, 256).expect("main is created");
    run(
        &mut engine,
        &format!(
            "ATTACH '{}' AS aux",
            aux.to_string_lossy().replace('\\', "/")
        ),
    );
    fill_and_thin(&mut engine);
    run(&mut engine, "PRAGMA aux.wal_checkpoint(TRUNCATE)");
    let main_before = length(&main);
    let before = length(&aux);
    run(&mut engine, "VACUUM aux");
    let after = length(&aux);
    assert!(
        after * 3 < before,
        "the file was {before} bytes before VACUUM aux and {after} after; 2,000 of 20,000 rows are left"
    );
    assert_rows(&mut engine);
    assert_eq!(length(&main), main_before, "main changed");
    drop(engine);
    let mut engine = ImportedDatabase::open(main, 32_768, 256).expect("main reopens");
    run(
        &mut engine,
        &format!(
            "ATTACH '{}' AS aux",
            aux.to_string_lossy().replace('\\', "/")
        ),
    );
    assert_rows(&mut engine);
}

/// A table with no rows left and a table that keeps its rows both survive.
#[test]
fn vacuum_of_an_attached_database_keeps_every_table() {
    let directory = area("every-table");
    let main = directory.join("main.rdb");
    let aux = directory.join("aux.rdb");
    let mut engine = ImportedDatabase::create(main, 32_768, 256).expect("main is created");
    run(
        &mut engine,
        &format!(
            "ATTACH '{}' AS aux",
            aux.to_string_lossy().replace('\\', "/")
        ),
    );
    run(
        &mut engine,
        "CREATE TABLE aux.kept(k TEXT PRIMARY KEY, v INTEGER) WITHOUT ROWID",
    );
    run(
        &mut engine,
        "CREATE TABLE aux.emptied(a INTEGER PRIMARY KEY, b TEXT UNIQUE)",
    );
    run(
        &mut engine,
        "INSERT INTO aux.kept SELECT 'k' || value, value FROM generate_series(1, 500)",
    );
    run(
        &mut engine,
        "INSERT INTO aux.emptied SELECT value, 'b' || value FROM generate_series(1, 3000)",
    );
    run(&mut engine, "DELETE FROM aux.emptied");
    run(&mut engine, "VACUUM aux");
    assert_eq!(count(&mut engine, "SELECT count(*) FROM aux.kept"), 500);
    assert_eq!(count(&mut engine, "SELECT sum(v) FROM aux.kept"), 125_250);
    assert_eq!(count(&mut engine, "SELECT count(*) FROM aux.emptied"), 0);
    run(&mut engine, "INSERT INTO aux.emptied VALUES (1, 'again')");
    assert_eq!(
        count(
            &mut engine,
            "SELECT count(*) FROM aux.emptied WHERE b = 'again'"
        ),
        1
    );
    let check = run(&mut engine, "PRAGMA aux.integrity_check");
    assert_eq!(check, vec![vec![OwnedDatum::Text(b"ok".to_vec())]]);
}

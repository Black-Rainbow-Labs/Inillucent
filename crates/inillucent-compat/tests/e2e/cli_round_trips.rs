//! Data that goes out through the command line comes back in through it.
//!
//! Invariant: **a backup, an export, a parameters file and a script written by
//! a common tool each load back into what the caller started with, and a
//! command that cannot do what it was asked leaves the database as it was.**
//!
//! ## Why these cases exist
//!
//! The bug hunt drove `inillucent` the way a person and a script do and found
//! five round trips that broke:
//!
//! - `inillucent backup app.rdb --db app.rdb` copied the file onto itself,
//!   which truncates it, and left no database at all.
//! - A backup over an existing database left that database's log segments
//!   beside the copy, where recovery reads segments by name.
//! - `batch` wrapped every script in a transaction, so a script with its own
//!   `BEGIN` and `COMMIT`, such as `.dump` output, failed at its `BEGIN`.
//! - `export <table> --format insert` wrote `INSERT INTO tab(...)`, which loads
//!   into no table.
//! - A parameters file saved by Notepad or PowerShell starts with a byte order
//!   mark and was refused as "not JSON".

use std::path::{Path, PathBuf};

use inillucent_compat::cliproc::{program, rows, run, text_field, Ran};
use inillucent_compat::workspace_root;

/// Returns a directory of this case's own, emptied first.
///
/// @param case - what to name the directory after
fn area(case: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/cli-round-trips")
        .join(case);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs `inillucent` against a database with the given arguments.
///
/// @param database - the database file
/// @param arguments - the command and its arguments
fn on(database: &Path, arguments: &[&str]) -> Ran {
    let path = database.to_string_lossy().to_string();
    let mut all = vec!["--db", path.as_str()];
    all.extend_from_slice(arguments);
    run(&program("inillucent"), &all)
}

/// Runs a query and returns its rows, failing the case if the query fails.
///
/// @param database - the database file
/// @param sql - the query
fn query(database: &Path, sql: &str) -> Vec<Vec<String>> {
    let ran = on(database, &["query", sql, "--output", "json"]);
    assert_eq!(ran.code, 0, "{sql} failed:\n{}", ran.said());
    rows(&ran.stdout)
}

/// Makes a database holding one table with two rows.
///
/// @param database - where to make it
fn two_rows(database: &Path) {
    let made = on(
        database,
        &["batch", "CREATE TABLE note(id INTEGER PRIMARY KEY, body TEXT); INSERT INTO note VALUES(1, 'one'), (2, 'two');"],
    );
    assert_eq!(made.code, 0, "the fixture was not made:\n{}", made.said());
}

/// Returns the names of the files in a directory that begin with a prefix.
///
/// @param directory - the directory
/// @param prefix - the start of the names
fn files_starting(directory: &Path, prefix: &str) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(directory)
        .map(|listing| {
            listing
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(prefix))
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// A backup onto the database it copies is refused, and the database keeps
/// every row.
#[test]
fn a_backup_onto_its_own_database_is_refused_and_destroys_nothing() {
    let directory = area("backup-onto-itself");
    let database = directory.join("app.rdb");
    two_rows(&database);
    for target in [database.clone(), directory.join(".").join("app.rdb")] {
        let refused = on(&database, &["backup", &target.to_string_lossy()]);
        assert_eq!(
            refused.code,
            1,
            "a backup onto itself was not refused:\n{}",
            refused.said()
        );
        assert!(refused.said().contains("onto itself"), "{}", refused.said());
    }
    assert_eq!(
        query(&database, "SELECT id, body FROM note ORDER BY id"),
        vec![
            vec!["1".to_string(), "one".to_string()],
            vec!["2".to_string(), "two".to_string()]
        ]
    );
}

/// A backup over an existing database replaces it, and leaves none of the
/// replaced database's log segments behind.
#[test]
fn a_backup_over_an_existing_database_leaves_none_of_its_log() {
    let directory = area("backup-over-existing");
    let source = directory.join("source.rdb");
    two_rows(&source);
    let destination = directory.join("old.rdb");
    let old = on(
        &destination,
        &["batch", "CREATE TABLE stale(x); INSERT INTO stale VALUES('old'); INSERT INTO stale VALUES('older');"],
    );
    assert_eq!(old.code, 0, "{}", old.said());
    assert!(
        !files_starting(&directory, "old.rdb-wal.").is_empty(),
        "the fixture needs the old database to have a log beside it"
    );
    let copied = on(&source, &["backup", &destination.to_string_lossy()]);
    assert_eq!(copied.code, 0, "the backup failed:\n{}", copied.said());
    assert_eq!(
        files_starting(&directory, "old.rdb"),
        vec!["old.rdb".to_string()],
        "the replaced database's log segments, or the scratch copy, were left beside the backup"
    );
    assert_eq!(
        query(&destination, "SELECT name FROM sqlite_schema ORDER BY name"),
        vec![vec!["note".to_string()]]
    );
    assert_eq!(
        query(&destination, "PRAGMA integrity_check"),
        vec![vec!["ok".to_string()]]
    );
}

/// A script with its own `BEGIN` and `COMMIT` runs through `batch`, and one
/// that fails inside its own transaction leaves nothing of it.
#[test]
fn a_batch_with_its_own_transaction_runs_and_rolls_back_whole() {
    let directory = area("batch-own-transaction");
    let database = directory.join("app.rdb");
    let ran = on(
        &database,
        &[
            "batch",
            "BEGIN TRANSACTION; CREATE TABLE t(a); INSERT INTO t VALUES(1); COMMIT;",
        ],
    );
    assert_eq!(
        ran.code,
        0,
        "a script with its own transaction was refused:\n{}",
        ran.said()
    );
    let failed = on(
        &database,
        &[
            "batch",
            "BEGIN; INSERT INTO t VALUES(2); INSERT INTO missing VALUES(3); COMMIT;",
        ],
    );
    assert_eq!(failed.code, 1, "{}", failed.said());
    assert_eq!(
        query(&database, "SELECT a FROM t"),
        vec![vec!["1".to_string()]]
    );
    let wrapped = on(
        &database,
        &[
            "batch",
            "INSERT INTO t VALUES(4); INSERT INTO missing VALUES(5);",
        ],
    );
    assert_eq!(wrapped.code, 1, "{}", wrapped.said());
    assert_eq!(
        query(&database, "SELECT a FROM t"),
        vec![vec!["1".to_string()]]
    );
}

/// `export <table> --format insert` writes statements that load back into
/// a table of that name.
#[test]
fn an_insert_export_of_a_table_loads_back_into_that_table() {
    let directory = area("export-insert");
    let database = directory.join("app.rdb");
    two_rows(&database);
    let exported = on(&database, &["export", "note", "--format", "insert"]);
    assert_eq!(exported.code, 0, "{}", exported.said());
    assert!(
        exported.stdout.contains("INSERT INTO note("),
        "the export did not name the table:\n{}",
        exported.stdout
    );
    let copy = directory.join("copy.rdb");
    let schema = on(
        &copy,
        &[
            "exec",
            "CREATE TABLE note(id INTEGER PRIMARY KEY, body TEXT)",
        ],
    );
    assert_eq!(schema.code, 0, "{}", schema.said());
    let loaded = on(&copy, &["batch", exported.stdout.trim()]);
    assert_eq!(
        loaded.code,
        0,
        "the export did not load back:\n{}",
        loaded.said()
    );
    assert_eq!(
        query(&copy, "SELECT count(*) FROM note"),
        vec![vec!["2".to_string()]]
    );
}

/// A parameters file that starts with a byte order mark is read.
#[test]
fn a_parameters_file_with_a_byte_order_mark_is_read() {
    let directory = area("params-file-bom");
    let database = directory.join("app.rdb");
    two_rows(&database);
    let parameters = directory.join("params.json");
    std::fs::write(&parameters, "\u{FEFF}[2]").expect("the parameters file is written");
    let ran = on(
        &database,
        &[
            "query",
            "SELECT body FROM note WHERE id = ?1",
            "--params-file",
            &parameters.to_string_lossy(),
            "--output",
            "json",
        ],
    );
    assert_eq!(
        ran.code,
        0,
        "the parameters file was refused:\n{}",
        ran.said()
    );
    assert_eq!(rows(&ran.stdout), vec![vec!["two".to_string()]]);
}

/// Imports a file through the `import` command into a new table named `t`.
///
/// @param database - the database file, created by the import
/// @param file - the file to read
fn import_into_t(database: &Path, file: &Path) -> Ran {
    on(
        database,
        &[
            "import",
            &file.to_string_lossy(),
            "--table",
            "t",
            "--output",
            "json",
        ],
    )
}

/// A file whose lines end in a bare carriage return is one row, which is how
/// SQLite reads it, and a carriage return in the middle of a field is data.
#[test]
fn an_import_keeps_a_carriage_return_that_does_not_end_a_row() {
    let directory = area("import-bare-carriage-return");
    let database = directory.join("app.rdb");
    let file = directory.join("cr.csv");
    std::fs::write(&file, b"a,b\r1,2\r3,4\r").expect("the file is written");
    let ran = import_into_t(&database, &file);
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(
        query(
            &database,
            "SELECT name FROM pragma_table_info('t') ORDER BY cid"
        ),
        vec![
            vec!["a".to_string()],
            vec!["b\r1".to_string()],
            vec!["2\r3".to_string()],
            vec!["4\r".to_string()]
        ]
    );
    assert_eq!(
        query(&database, "SELECT count(*) FROM t"),
        vec![vec!["0".to_string()]]
    );
}

/// A header that ends in a carriage return before its newline names the column
/// without the carriage return.
#[test]
fn an_import_of_a_crlf_file_names_its_columns_without_the_carriage_return() {
    let directory = area("import-crlf-header");
    let database = directory.join("app.rdb");
    let file = directory.join("crlf.csv");
    std::fs::write(&file, b"a,b\r\n1,2\r\n3,\"4\"\r\n").expect("the file is written");
    let ran = import_into_t(&database, &file);
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(
        query(
            &database,
            "SELECT name FROM pragma_table_info('t') ORDER BY cid"
        ),
        vec![vec!["a".to_string()], vec!["b".to_string()]]
    );
    assert_eq!(
        query(&database, "SELECT a, b FROM t ORDER BY rowid"),
        vec![
            vec!["1".to_string(), "2".to_string()],
            vec!["3".to_string(), "4".to_string()]
        ]
    );
}

/// A row with fewer fields than the table has columns is padded with NULL, a
/// row with more has its extra fields dropped, every row is kept, and the
/// warnings name the line each one began on.
#[test]
fn an_import_pads_short_rows_and_names_the_lines_it_warned_about() {
    let directory = area("import-ragged");
    let database = directory.join("app.rdb");
    let file = directory.join("ragged.csv");
    std::fs::write(&file, "a,b,c\n1,2\n3\n4,5,6,7,8\n\n9,9,9\n").expect("the file is written");
    let ran = import_into_t(&database, &file);
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(
        query(&database, "SELECT a, b, c FROM t ORDER BY rowid"),
        vec![
            vec!["1".to_string(), "2".to_string(), "null".to_string()],
            vec!["3".to_string(), "null".to_string(), "null".to_string()],
            vec!["4".to_string(), "5".to_string(), "6".to_string()],
            vec![String::new(), "null".to_string(), "null".to_string()],
            vec!["9".to_string(), "9".to_string(), "9".to_string()]
        ]
    );
    let said = text_field(&ran.stdout, "text");
    for line in [
        "ragged.csv:2: expected 3 columns but found 2 - filling the rest with NULL",
        "ragged.csv:3: expected 3 columns but found 1 - filling the rest with NULL",
        "ragged.csv:4: expected 3 columns but found 5 - extras ignored",
        "ragged.csv:5: expected 3 columns but found 1 - filling the rest with NULL",
    ] {
        assert!(said.contains(line), "missing `{line}` in:\n{said}");
    }
    assert!(said.contains("imported 5 rows into t"), "{said}");
}

/// A row the table rejects is named by its line and does not stop the rows
/// around it, and the status names the cause.
#[test]
fn an_import_keeps_the_good_rows_around_a_rejected_one() {
    let directory = area("import-rejected-row");
    let database = directory.join("app.rdb");
    let made = on(
        &database,
        &["exec", "CREATE TABLE t(id INTEGER PRIMARY KEY, name TEXT)"],
    );
    assert_eq!(made.code, 0, "{}", made.said());
    let file = directory.join("rows.csv");
    std::fs::write(&file, "id,name\n1,x\n2,y\n2,dup\n3,z\n").expect("the file is written");
    let ran = import_into_t(&database, &file);
    assert_eq!(ran.code, 1, "{}", ran.said());
    let said = text_field(&ran.stdout, "message");
    assert!(
        said.contains("rows.csv:1: INSERT failed: datatype mismatch"),
        "{said}"
    );
    assert!(
        said.contains("rows.csv:4: INSERT failed: UNIQUE constraint failed: t.id"),
        "{said}"
    );
    assert_eq!(
        query(&database, "SELECT id, name FROM t ORDER BY id"),
        vec![
            vec!["1".to_string(), "x".to_string()],
            vec!["2".to_string(), "y".to_string()],
            vec!["3".to_string(), "z".to_string()]
        ]
    );
}

/// Header names that repeat are renamed with their column number, as SQLite
/// renames them, and a name that is empty becomes `?`.
#[test]
fn an_import_renames_header_names_that_repeat() {
    let directory = area("import-duplicate-header");
    let database = directory.join("app.rdb");
    let file = directory.join("dup.csv");
    std::fs::write(&file, "a,A,b,a_1,,\n1,2,3,4,5,6\n").expect("the file is written");
    let ran = import_into_t(&database, &file);
    assert_eq!(ran.code, 0, "{}", ran.said());
    let names: Vec<String> = query(
        &database,
        "SELECT name FROM pragma_table_info('t') ORDER BY cid",
    )
    .into_iter()
    .map(|row| row.join(""))
    .collect();
    assert_eq!(names, vec!["a_01", "A_02", "b", "a_1", "?_05", "?_06"]);
}

/// The tabs format quotes a value that holds a tab and the import reads the
/// quotes, so the value survives being exported and loaded again.
#[test]
fn a_value_with_a_tab_survives_a_tabs_export_and_import() {
    let directory = area("tabs-round-trip");
    let database = directory.join("app.rdb");
    let made = on(
        &database,
        &["batch", "CREATE TABLE note(id INTEGER PRIMARY KEY, body TEXT); INSERT INTO note VALUES(1, 'one' || char(9) || 'two'), (2, 'say \"hi\"'), (3, 'line' || char(10) || 'break');"],
    );
    assert_eq!(made.code, 0, "{}", made.said());
    let exported = on(&database, &["export", "note", "--format", "tabs"]);
    assert_eq!(exported.code, 0, "{}", exported.said());
    let file = directory.join("note.tsv");
    std::fs::write(&file, exported.stdout.as_bytes()).expect("the export is saved");
    let copy = directory.join("copy.rdb");
    let schema = on(
        &copy,
        &[
            "exec",
            "CREATE TABLE note(id INTEGER PRIMARY KEY, body TEXT)",
        ],
    );
    assert_eq!(schema.code, 0, "{}", schema.said());
    let loaded = on(
        &copy,
        &[
            "import",
            &file.to_string_lossy(),
            "--table",
            "note",
            "--format",
            "tabs",
            "--skip",
            "1",
            "--output",
            "json",
        ],
    );
    assert_eq!(loaded.code, 0, "{}", loaded.said());
    assert_eq!(
        query(&copy, "SELECT id, body FROM note ORDER BY id"),
        query(&database, "SELECT id, body FROM note ORDER BY id")
    );
}

/// A double that JSON cannot hold is written as a number a reader parses, and a
/// very large or very small one is not written as hundreds of digits.
#[test]
fn json_output_writes_extreme_doubles_compactly_and_infinity_as_a_number() {
    let directory = area("json-extreme-doubles");
    let database = directory.join("app.rdb");
    let created = run(
        &program("inillucent"),
        &["create", &database.to_string_lossy()],
    );
    assert_eq!(created.code, 0, "{}", created.said());
    let ran = on(
        &database,
        &[
            "query",
            "SELECT 1e100 AS a, 5e-324 AS b, 9e999 AS c, -9e999 AS d, 1.5 AS e",
            "--output",
            "json",
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.said());
    let compact: String = ran.stdout.split_whitespace().collect();
    assert!(
        compact.contains("[1e100,5e-324,1e999,-1e999,1.5]"),
        "{}",
        ran.stdout
    );
}

/// A database path that is a directory is `io`, and a file to import that is not
/// there is `not_found`. Neither is `syntax`, which says the SQL is wrong.
#[test]
fn a_path_that_cannot_be_opened_reports_the_status_of_its_cause() {
    let directory = area("status-of-paths");
    let folder = directory.join("folder");
    std::fs::create_dir_all(&folder).expect("the folder is made");
    let ran = on(&folder, &["tables", "--output", "json"]);
    assert_eq!(ran.code, 1, "{}", ran.said());
    assert_eq!(text_field(&ran.stdout, "status"), "io", "{}", ran.stdout);

    let database = directory.join("app.rdb");
    two_rows(&database);
    let missing = directory.join("nothing.csv");
    let ran = on(
        &database,
        &[
            "import",
            &missing.to_string_lossy(),
            "--table",
            "t",
            "--output",
            "json",
        ],
    );
    assert_eq!(ran.code, 1, "{}", ran.said());
    assert_eq!(
        text_field(&ran.stdout, "status"),
        "not_found",
        "{}",
        ran.stdout
    );

    let source = directory.join("source.rdb");
    two_rows(&source);
    let target = directory.join("nofolder").join("out.rdb");
    let ran = run(
        &program("inillucent"),
        &[
            "migrate",
            &source.to_string_lossy(),
            "--destination",
            &target.to_string_lossy(),
            "--kind",
            "sqlite",
            "--output",
            "json",
        ],
    );
    assert_eq!(ran.code, 1, "{}", ran.said());
    assert_eq!(
        text_field(&ran.stdout, "status"),
        "not_found",
        "{}",
        ran.stdout
    );
}

/// A command that works on a database that has to exist does not make one, as a
/// read does not, while a command that sets a database up still does.
#[test]
fn only_a_command_that_sets_a_database_up_creates_the_file() {
    let directory = area("which-commands-create");
    for verb in ["analyze", "checkpoint", "tables"] {
        let database = directory.join(format!("{verb}.rdb"));
        let ran = on(&database, &[verb, "--output", "json"]);
        assert_eq!(ran.code, 1, "{verb}: {}", ran.said());
        assert_eq!(
            text_field(&ran.stdout, "status"),
            "not_found",
            "{verb}: {}",
            ran.stdout
        );
        assert!(!database.exists(), "{verb} made {}", database.display());
    }
    for (verb, argument) in [
        ("exec", "CREATE TABLE a(x)"),
        ("batch", "CREATE TABLE a(x);"),
    ] {
        let database = directory.join(format!("{verb}.rdb"));
        let ran = on(&database, &[verb, argument]);
        assert_eq!(ran.code, 0, "{verb}: {}", ran.said());
        assert!(
            database.exists(),
            "{verb} did not make {}",
            database.display()
        );
    }
}

/// With `--output json` a command line that cannot run answers in JSON.
#[test]
fn a_malformed_command_line_answers_in_json_when_json_was_asked_for() {
    let directory = area("usage-failures-in-json");
    let database = directory.join("app.rdb");
    two_rows(&database);
    for arguments in [
        vec!["query", "SELECT ?1", "--params", "[1,", "--output", "json"],
        vec!["query", "SELECT 1", "--nope", "x", "--output", "json"],
        vec!["query", "SELECT 1", "--limit", "x", "--output", "json"],
        vec!["frobnicate", "--output", "json"],
    ] {
        let ran = on(&database, &arguments);
        assert_eq!(ran.code, 2, "{arguments:?}: {}", ran.said());
        assert_eq!(
            text_field(&ran.stdout, "status"),
            "invalid_state",
            "{arguments:?}: {}",
            ran.stdout
        );
        assert!(
            ran.stderr.is_empty(),
            "{arguments:?} wrote to standard error: {}",
            ran.stderr
        );
    }
}

/// An empty blob is a blob, and a parameter the statement has no place for is
/// refused with the number the statement takes.
#[test]
fn parameters_accept_an_empty_blob_and_name_how_many_a_statement_takes() {
    let directory = area("params-blob-and-count");
    let database = directory.join("app.rdb");
    two_rows(&database);
    let ran = on(
        &database,
        &[
            "query",
            "SELECT typeof(?1), length(?1)",
            "--params",
            "[{\"blob\":\"\"}]",
            "--output",
            "json",
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(
        rows(&ran.stdout),
        vec![vec!["blob".to_string(), "0".to_string()]]
    );

    let ran = on(
        &database,
        &[
            "query",
            "SELECT ?1",
            "--params",
            "[1, 2, 3]",
            "--output",
            "json",
        ],
    );
    assert_eq!(ran.code, 1, "{}", ran.said());
    let said = text_field(&ran.stdout, "message");
    assert!(
        said.contains("the statement takes 1 parameter, and 3 were given"),
        "{said}"
    );
}

/// A dump of a database that cannot be read reports why, not a bad statement.
#[test]
fn a_dump_of_a_damaged_database_reports_a_status_that_names_the_damage() {
    let directory = area("dump-damaged");
    let database = directory.join("app.rdb");
    let made = on(
        &database,
        &["batch", "CREATE TABLE t(a, b); WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 4000) INSERT INTO t SELECT i, printf('%0500d', i) FROM n;"],
    );
    assert_eq!(made.code, 0, "{}", made.said());
    let folded = on(&database, &["checkpoint"]);
    assert_eq!(folded.code, 0, "{}", folded.said());
    let size = std::fs::metadata(&database)
        .expect("the database exists")
        .len();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(&database)
        .expect("the database opens");
    file.set_len(size / 2).expect("the file is cut");
    drop(file);
    let ran = on(&database, &["dump", "--output", "json"]);
    assert_eq!(ran.code, 1, "{}", ran.said());
    let status = text_field(&ran.stdout, "status");
    assert!(
        status == "corrupt" || status == "io",
        "a damaged file was reported as {status}:\n{}",
        ran.stdout
    );
}

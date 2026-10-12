//! Several processes opening a database that does not exist yet, or that is
//! an empty file, all end up on one database that holds what each of them
//! wrote.
//!
//! Invariant: **creating a database never removes or truncates a file another
//! process has already made a database of, and an empty file opens as an
//! empty database.**
//!
//! ## Why these cases exist
//!
//! An open of a path with nothing at it created the database by deleting
//! whatever was at the path, then opening it, taking the exclusive lock and
//! truncating it to nothing. Two copies of an application started together
//! both saw no file, and the second one's delete and truncate could land after
//! the first had created the file: in one run of ten with eight shells, a shell
//! failed to open with "The system cannot find the file specified", and a
//! process that had already committed rows by then would have lost them.
//!
//! An empty file was refused as "database disk image is malformed". SQLite
//! opens one as an empty database, and a temporary file from `mkstemp` or a
//! test fixture is exactly such a file. Found by the bug hunt's command line
//! round trips.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::cliproc::program;
use inillucent_compat::workspace_root;
use inillucent_driver::{Database, Value};

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/process-create")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Returns the rows of a query, read through the driver in this process, each
/// row's values joined by `|`.
///
/// @param database - the file
/// @param sql - the query
fn rows_of(database: &Path, sql: &str) -> Vec<String> {
    let opened = Database::open(database).expect("the database opens");
    let session = opened.session();
    let rows = session
        .query_all(sql, &[])
        .unwrap_or_else(|error| panic!("{sql}: {error:?}"));
    rows.rows
        .iter()
        .map(|row| {
            row.iter()
                .map(|value| match value {
                    Value::Integer(n) => n.to_string(),
                    Value::Text(text) => text.clone(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

/// An empty file opens as an empty database, through the driver and through
/// the command line, and keeps what is written to it.
#[test]
fn an_empty_file_opens_as_an_empty_database() {
    let directory = area("empty-file");
    let database = directory.join("empty.rdb");
    std::fs::write(&database, b"").expect("the empty file is made");
    {
        let opened = Database::open(&database)
            .unwrap_or_else(|error| panic!("an empty file did not open: {error:?}"));
        let session = opened.session();
        session
            .execute_batch("CREATE TABLE t(a); INSERT INTO t VALUES('kept');")
            .unwrap_or_else(|error| panic!("the empty database took no table: {error:?}"));
    }
    assert_eq!(
        rows_of(&database, "SELECT a FROM t"),
        vec!["kept".to_string()]
    );

    let second = directory.join("empty-cli.rdb");
    std::fs::write(&second, b"").expect("the second empty file is made");
    let ran = Command::new(program("inillucent"))
        .args([
            "--db",
            &second.to_string_lossy(),
            "query",
            "SELECT count(*) FROM sqlite_schema",
        ])
        .output()
        .expect("the command line runs");
    assert!(
        ran.status.success(),
        "the command line refused an empty file: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

/// Eight shells started at the same moment on a path with nothing at it all
/// open, and every row each of them wrote is in the one database afterwards.
#[test]
fn processes_that_create_one_database_together_all_open_and_keep_their_rows() {
    let shell = program("inillucent-shell");
    for iteration in 0..20 {
        let directory = area(&format!("together-{iteration}"));
        let database = directory.join("shared.rdb");
        let children: Vec<_> = (0..8)
            .map(|who| {
                let mut child = Command::new(&shell)
                    .arg(&database)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
                    .expect("a shell starts");
                if let Some(mut input) = child.stdin.take() {
                    use std::io::Write;
                    let script = format!(
                        ".timeout 20000\nCREATE TABLE IF NOT EXISTS t(who, n);\n\
                         INSERT INTO t VALUES({who}, 1);\nINSERT INTO t VALUES({who}, 2);\n"
                    );
                    let _ = input.write_all(script.as_bytes());
                }
                child
            })
            .collect();
        let mut failures = Vec::new();
        for (who, child) in children.into_iter().enumerate() {
            let output = child.wait_with_output().expect("a shell finishes");
            let said = format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if !said.trim().is_empty() {
                failures.push(format!("shell {who}: {}", said.trim()));
            }
        }
        assert!(
            failures.is_empty(),
            "iteration {iteration}: shells failed while creating one database together:\n{}",
            failures.join("\n")
        );
        let expected: Vec<String> = (0..8)
            .flat_map(|who| [format!("{who}|1"), format!("{who}|2")])
            .collect();
        assert_eq!(
            rows_of(&database, "SELECT who, n FROM t ORDER BY who, n"),
            expected,
            "iteration {iteration}: a row a shell wrote is missing"
        );
        assert_eq!(
            rows_of(&database, "PRAGMA integrity_check"),
            vec!["ok".to_string()]
        );
    }
}

/// A database created where a deleted one stood, with the deleted one's log
/// still beside it, holds only its own rows.
///
/// The old log's segments carry the old database's uuid, so recovery of the new
/// file must refuse them rather than replay them, and the new file's own log
/// must not append behind them. The old database here dies before it folds,
/// so its segments hold committed rows its file never received.
#[test]
fn a_new_database_over_a_deleted_ones_log_holds_only_its_own_rows() {
    let directory = area("over-a-deleted-log");
    let database = directory.join("reused.rdb");
    let said = inillucent_compat::cliproc::write_and_crash(
        &program("inillucent-shell"),
        &database,
        "CREATE TABLE old(a); INSERT INTO old SELECT value FROM generate_series(1, 300);",
    );
    assert!(
        said.contains("written"),
        "the old database's rows were written: {said}"
    );
    let segments_left = std::fs::read_dir(&directory)
        .expect("the directory lists")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().contains("-wal."))
        .count();
    assert!(
        segments_left > 0,
        "the crash left log segments beside the file"
    );
    std::fs::remove_file(&database).expect("the old database file is deleted");

    {
        let fresh = Database::open(&database).expect("a new database is made at the path");
        let session = fresh.session();
        session
            .execute_batch("CREATE TABLE new(b); INSERT INTO new VALUES (1), (2), (3);")
            .expect("the new rows are written");
    }
    let reopened = Database::open(&database).expect("the new database reopens");
    let session = reopened.session();
    let tables = session
        .query_all("SELECT name FROM sqlite_schema ORDER BY name", &[])
        .expect("the schema reads");
    assert_eq!(
        tables.rows,
        vec![vec![Value::Text("new".into())]],
        "only the new database's table is there"
    );
    let counted = session
        .query_all("SELECT count(*) FROM new", &[])
        .expect("the count runs");
    assert_eq!(counted.rows, vec![vec![Value::Integer(3)]]);
    let checked = session
        .query_all("PRAGMA integrity_check", &[])
        .expect("the check runs");
    assert_eq!(checked.rows, vec![vec![Value::Text("ok".into())]]);
}

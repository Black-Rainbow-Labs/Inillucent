//! Renaming a table or a column that a vector index is built over, and renaming and protecting the
//! tables a module keeps.
//!
//! Invariant: **an index follows its table through `ALTER TABLE ... RENAME`.** The store behind an
//! `inillucent_hnsw` index records the table and column it indexes in its own definition
//! (`source=`, `source_column=`), and the engine finds a table's indexes by reading them back. A
//! rename that rewrote only the table left the store pointing at a name nothing had, so inserts
//! stopped reaching the index, `DROP INDEX` said `no such index`, and six `pv_*` tables stayed
//! behind. SQLite has no vector index, so the expected answers are written out by hand.

use std::path::PathBuf;

use inillucent_compat::workspace_root;
use inillucent_engine::connect::{Connection, Database};
use inillucent_engine::OwnedDatum;

/// Returns a fresh database file for one case.
///
/// @param name - the case's name, which is also its directory
fn fresh(name: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output/vector-rename")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("the case's directory was not made: {error}"));
    directory.join("rename.rdb")
}

/// Runs statements that must succeed.
///
/// @param session - the connection
/// @param sql - the statements
fn exec(session: &Connection<'_>, sql: &str) {
    session
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql} failed: {}", error.message()));
}

/// Runs a statement that must be refused, and returns the message.
///
/// @param session - the connection
/// @param sql - the statement
fn refusal(session: &Connection<'_>, sql: &str) -> String {
    match session.execute_batch(sql) {
        Ok(()) => panic!("{sql} was accepted"),
        Err(error) => error.message().to_string(),
    }
}

/// Runs a query and returns its first column rendered as text, one per row.
///
/// @param session - the connection
/// @param sql - the query
fn texts(session: &Connection<'_>, sql: &str) -> Vec<String> {
    let mut statement = session
        .prepare(sql)
        .unwrap_or_else(|error| panic!("{sql} did not prepare: {}", error.message()));
    let mut found = Vec::new();
    while statement
        .step()
        .unwrap_or_else(|error| panic!("{sql} failed: {}", error.message()))
    {
        found.push(match statement.row().first() {
            Some(OwnedDatum::Int(value)) => value.to_string(),
            Some(OwnedDatum::Text(value)) => String::from_utf8_lossy(value).into_owned(),
            other => panic!("{sql} returned {other:?}"),
        });
    }
    found
}

/// The table with an index, as every case starts it.
const START: &str = "CREATE TABLE p (id INTEGER PRIMARY KEY, v VECTOR(3));
     INSERT INTO p VALUES (1, '[1, 0, 0]'), (2, '[0, 1, 0]'), (3, '[0, 0, 1]');
     CREATE INDEX pv ON p USING inillucent_hnsw (v);";

/// The rows the index reports in its delta log, which grows with every row the index is given.
const DELTA_ROWS: &str = "SELECT count(*) FROM pv_delta";

/// The nearest row to a vector from the table with this name and column.
///
/// @param table - the table
/// @param column - its vector column
/// @param vector - the query vector, as text
fn nearest(table: &str, column: &str, vector: &str) -> String {
    format!("SELECT id FROM {table} ORDER BY vector_distance_cos({column}, '{vector}') LIMIT 1")
}

/// Asserts that the index was used for the query and that a new row reaches it.
///
/// @param session - the connection
/// @param table - the table's current name
/// @param column - the vector column's current name
/// @param id - the id of the row to insert, which also picks its vector
fn assert_the_index_takes_a_row(session: &Connection<'_>, table: &str, column: &str, id: i64) {
    let vector = format!("[1, {id}, 0]");
    let before: i64 = texts(session, DELTA_ROWS)[0].parse().unwrap_or(-1);
    exec(
        session,
        &format!("INSERT INTO {table} (id, {column}) VALUES ({id}, '{vector}')"),
    );
    let after: i64 = texts(session, DELTA_ROWS)[0].parse().unwrap_or(-1);
    assert_eq!(
        after,
        before + 1,
        "the row inserted into {table} did not reach the index"
    );
    assert_eq!(
        texts(session, &nearest(table, column, &vector)),
        vec![id.to_string()]
    );
}

/// Renaming the table keeps the index, before and after a reopen, and the index can be dropped.
#[test]
fn renaming_a_table_carries_its_vector_index() {
    let path = fresh("rename-table");
    {
        let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
        let session = database.session();
        exec(&session, START);
        exec(&session, "ALTER TABLE p RENAME TO q");
        assert_the_index_takes_a_row(&session, "q", "v", 4);
        assert_eq!(
            texts(&session, "SELECT name FROM pragma_index_list('q')"),
            vec!["pv"]
        );
    }
    let database = Database::open(&path).unwrap_or_else(|error| panic!("reopen: {error}"));
    let session = database.session();
    assert_the_index_takes_a_row(&session, "q", "v", 5);
    exec(&session, "DROP INDEX pv");
    assert_eq!(
        texts(
            &session,
            "SELECT name FROM sqlite_schema WHERE name LIKE 'pv%'"
        ),
        Vec::<String>::new(),
        "DROP INDEX left the store's tables behind"
    );
}

/// Renaming the indexed column keeps the index, before and after a reopen.
#[test]
fn renaming_the_indexed_column_carries_its_vector_index() {
    let path = fresh("rename-column");
    {
        let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
        let session = database.session();
        exec(&session, START);
        exec(&session, "ALTER TABLE p RENAME COLUMN v TO emb");
        assert_the_index_takes_a_row(&session, "p", "emb", 4);
    }
    let database = Database::open(&path).unwrap_or_else(|error| panic!("reopen: {error}"));
    let session = database.session();
    assert_the_index_takes_a_row(&session, "p", "emb", 5);
    exec(&session, "DROP INDEX pv");
    assert_eq!(
        texts(
            &session,
            "SELECT count(*) FROM sqlite_schema WHERE name LIKE 'pv%'"
        ),
        vec!["0"]
    );
}

/// A rename inside a transaction that is rolled back leaves the index with the old names.
#[test]
fn a_rolled_back_rename_leaves_the_vector_index_on_the_old_table() {
    let path = fresh("rolled-back");
    let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
    let session = database.session();
    exec(&session, START);
    exec(&session, "BEGIN; ALTER TABLE p RENAME TO q; ROLLBACK;");
    assert_the_index_takes_a_row(&session, "p", "v", 4);
}

/// An `inillucent_search` table is renamed with its shadow tables, and keeps its rows.
#[test]
fn renaming_an_inillucent_search_table_renames_its_shadow_tables() {
    let path = fresh("search-table");
    {
        let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
        let session = database.session();
        exec(
            &session,
            "CREATE VIRTUAL TABLE st USING inillucent_search(body, dims=3);
             INSERT INTO st(rowid, body, vector) VALUES (1, 'alpha beta', '[1,0,0]');
             ALTER TABLE st RENAME TO su;",
        );
        assert_eq!(
            texts(
                &session,
                "SELECT name FROM sqlite_schema WHERE name LIKE 'st%'"
            ),
            Vec::<String>::new()
        );
        exec(
            &session,
            "INSERT INTO su(rowid, body, vector) VALUES (2, 'alpha gamma', '[0,1,0]')",
        );
    }
    let database = Database::open(&path).unwrap_or_else(|error| panic!("reopen: {error}"));
    let session = database.session();
    assert_eq!(
        texts(
            &session,
            "SELECT rowid FROM su WHERE su MATCH 'alpha' AND k=10 ORDER BY rowid"
        ),
        vec!["1", "2"]
    );
}

/// The shadow tables of a vector index cannot be dropped, altered or given triggers.
#[test]
fn the_tables_of_a_vector_index_are_protected_on_a_defensive_connection() {
    let path = fresh("protected");
    let database = Database::open(&path).unwrap_or_else(|error| panic!("open: {error}"));
    let session = database.session();
    exec(&session, START);
    session
        .set_defensive(true)
        .unwrap_or_else(|error| panic!("defensive: {}", error.message()));
    for shadow in ["pv_config", "pv_content", "pv_delta", "pv_gen", "pv_state"] {
        assert_eq!(
            refusal(&session, &format!("DROP TABLE {shadow}")),
            format!("table {shadow} may not be dropped")
        );
        assert_eq!(
            refusal(&session, &format!("ALTER TABLE {shadow} RENAME TO other")),
            format!("table {shadow} may not be altered")
        );
        assert_eq!(
            refusal(
                &session,
                &format!("CREATE TRIGGER t_{shadow} AFTER INSERT ON {shadow} BEGIN SELECT 1; END")
            ),
            "cannot create triggers on shadow tables"
        );
    }
    assert_eq!(texts(&session, &nearest("p", "v", "[1, 0, 0]")), vec!["1"]);
}

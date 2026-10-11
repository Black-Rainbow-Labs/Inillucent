//! Edge cases of vector search that SQLite has no equivalent for: a nearest neighbour query on an
//! index that holds no vectors, a failed multi row write to an `inillucent_search` table, a
//! `rowid` assignment on one, the keyword syntax of its `MATCH`, and the names a metric accepts.
//!
//! Invariant: **an index never turns a valid query into an error, and a failed statement leaves
//! the table as it was.** SQLite has no vector functions, so the expected answers are written out
//! by hand in each test.

use inillucent_compat::differential::start_inillucent;
use inillucent_compat::rendering::datum_text as render;
use inillucent_engine::connect::Connection;

/// Where this suite's scratch databases live.
const AREA: &str = "vector_edges";

/// Runs a statement for its effect.
///
/// @param connection - the connection to run it on
/// @param sql - the statement
fn exec(connection: &Connection<'_>, sql: &str) {
    connection
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
}

/// Runs a statement, returning the error message it produced.
///
/// @param connection - the connection to run it on
/// @param sql - the statement
fn refusal(connection: &Connection<'_>, sql: &str) -> String {
    match connection.execute_batch(sql) {
        Ok(()) => panic!("{sql} was accepted"),
        Err(error) => error.message().to_string(),
    }
}

/// Returns the first column of every row, rendered as text.
///
/// @param connection - the connection to ask
/// @param sql - the query
fn column(connection: &Connection<'_>, sql: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
    let mut out = Vec::new();
    while statement
        .step()
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()))
    {
        out.push(statement.row().first().map(render).unwrap_or_default());
    }
    out
}

/// The index declarations whose nearest neighbour query must survive an empty index.
const INDEXES: &[&str] = &[
    "USING inillucent_hnsw (v)",
    "USING inillucent_hnsw (v) WITH (mode = 'exact')",
    "USING inillucent_hnsw (v) WITH (metric = 'l2')",
    "USING ivfflat (v)",
];

/// The nearest neighbour query every case in this file asks.
const NEAREST: &str = "SELECT id FROM vt ORDER BY vector_distance_cos(v,'[1,0,0]') LIMIT 10";

/// A nearest neighbour query on an index over a table with no rows answers no rows.
#[test]
fn an_empty_table_answers_no_rows_for_every_index_kind() {
    for (at, declaration) in INDEXES.iter().enumerate() {
        let connection = start_inillucent(AREA, &format!("empty-{at}"));
        exec(
            &connection,
            "CREATE TABLE vt (id INTEGER PRIMARY KEY, v VECTOR(3))",
        );
        exec(
            &connection,
            &format!("CREATE INDEX vt_v ON vt {declaration}"),
        );
        assert_eq!(
            column(&connection, NEAREST),
            Vec::<String>::new(),
            "{declaration}"
        );
    }
}

/// An index whose rows were all deleted answers no rows, and answers rows again after an insert.
#[test]
fn a_table_emptied_by_delete_answers_no_rows() {
    for (at, declaration) in INDEXES.iter().enumerate() {
        let connection = start_inillucent(AREA, &format!("deleted-{at}"));
        exec(
            &connection,
            "CREATE TABLE vt (id INTEGER PRIMARY KEY, v VECTOR(3))",
        );
        exec(
            &connection,
            &format!("CREATE INDEX vt_v ON vt {declaration}"),
        );
        exec(&connection, "INSERT INTO vt VALUES (1,'[1,0,0]')");
        assert_eq!(column(&connection, NEAREST), vec!["1"], "{declaration}");
        exec(&connection, "DELETE FROM vt");
        assert_eq!(
            column(&connection, NEAREST),
            Vec::<String>::new(),
            "{declaration}"
        );
        exec(&connection, "INSERT INTO vt VALUES (7,'[1,0,0]')");
        assert_eq!(column(&connection, NEAREST), vec!["7"], "{declaration}");
    }
}

/// An index over a table whose vectors are all NULL answers no rows.
#[test]
fn a_table_of_null_vectors_answers_no_rows() {
    let connection = start_inillucent(AREA, "all-null");
    exec(
        &connection,
        "CREATE TABLE vt (id INTEGER PRIMARY KEY, v VECTOR(3))",
    );
    exec(&connection, "INSERT INTO vt VALUES (1,NULL),(2,NULL)");
    exec(
        &connection,
        "CREATE INDEX vt_v ON vt USING inillucent_hnsw (v)",
    );
    assert_eq!(column(&connection, NEAREST), Vec::<String>::new());
}

/// A `WHERE` that removes every row answers no rows, and one that keeps some keeps them.
#[test]
fn a_where_that_removes_every_row_answers_no_rows() {
    let connection = start_inillucent(AREA, "where-none");
    exec(
        &connection,
        "CREATE TABLE vt (id INTEGER PRIMARY KEY, v VECTOR(3))",
    );
    exec(
        &connection,
        "INSERT INTO vt VALUES (1,'[1,0,0]'),(2,'[0,1,0]')",
    );
    exec(
        &connection,
        "CREATE INDEX vt_v ON vt USING inillucent_hnsw (v)",
    );
    let none =
        "SELECT id FROM vt WHERE id > 100 ORDER BY vector_distance_cos(v,'[1,0,0]') LIMIT 10";
    assert_eq!(column(&connection, none), Vec::<String>::new());
    let one = "SELECT id FROM vt WHERE id > 1 ORDER BY vector_distance_cos(v,'[1,0,0]') LIMIT 10";
    assert_eq!(column(&connection, one), vec!["2"]);
}

/// A row whose distance is NULL is left out by the index, and the documented form that gives the
/// same answer with and without the index is `WHERE v IS NOT NULL`.
#[test]
fn null_distance_rows_are_left_out_by_the_index_and_documented() {
    let connection = start_inillucent(AREA, "null-distance");
    exec(
        &connection,
        "CREATE TABLE vt (id INTEGER PRIMARY KEY, v VECTOR(3))",
    );
    exec(
        &connection,
        "INSERT INTO vt VALUES (1,'[1,0,0]'),(2,'[0,1,0]'),(3,NULL),(4,'[0,0,0]')",
    );
    let guarded = "SELECT id FROM vt WHERE v IS NOT NULL AND vector_distance_cos(v,'[1,0,0]') IS NOT NULL ORDER BY vector_distance_cos(v,'[1,0,0]') LIMIT 10";
    let before = column(&connection, guarded);
    assert_eq!(before, vec!["1", "2"]);
    assert_eq!(
        column(&connection, NEAREST),
        vec!["3", "4", "1", "2"],
        "without an index a NULL distance sorts first"
    );
    exec(
        &connection,
        "CREATE INDEX vt_v ON vt USING inillucent_hnsw (v)",
    );
    assert_eq!(column(&connection, guarded), before);
    assert_eq!(
        column(&connection, NEAREST),
        vec!["1", "2"],
        "with an index a NULL distance is not a neighbour"
    );
}

/// The metric `cos` is a spelling of `cosine`, and a name that is neither is refused with a
/// message that names the index and lists the accepted names.
#[test]
fn the_metric_names_are_listed_when_one_is_wrong() {
    let connection = start_inillucent(AREA, "metric-names");
    exec(
        &connection,
        "CREATE TABLE vt (id INTEGER PRIMARY KEY, v VECTOR(3))",
    );
    exec(
        &connection,
        "INSERT INTO vt VALUES (1,'[1,0,0]'),(2,'[0,1,0]')",
    );
    exec(
        &connection,
        "CREATE INDEX vt_cos ON vt USING inillucent_hnsw (v) WITH (metric = 'cos')",
    );
    assert_eq!(column(&connection, NEAREST), vec!["1", "2"]);
    let said = refusal(
        &connection,
        "CREATE INDEX vt_bad ON vt USING inillucent_hnsw (v) WITH (metric = 'manhattan')",
    );
    assert!(said.contains("vt_bad"), "{said}");
    assert!(said.contains("manhattan"), "{said}");
    assert!(
        said.contains("cosine") && said.contains("cos") && said.contains("l2"),
        "{said}"
    );
    assert!(!said.starts_with("inillucent_search:"), "{said}");
}

/// Creates a vector and text search table holding two rows.
///
/// @param connection - the connection to create it on
fn two_row_table(connection: &Connection<'_>) {
    exec(
        connection,
        "CREATE VIRTUAL TABLE st USING inillucent_search(body, dims=3)",
    );
    exec(
        connection,
        "INSERT INTO st(rowid,body,vector) VALUES (1,'one','[1,0,0]')",
    );
}

/// A multi row insert that fails on a later row leaves none of its rows.
#[test]
fn a_failed_multi_row_insert_into_a_search_table_leaves_nothing() {
    let connection = start_inillucent(AREA, "atomic-dup");
    two_row_table(&connection);
    let said = refusal(
        &connection,
        "INSERT INTO st(rowid,body,vector) VALUES (2,'two','[0,1,0]'),(1,'dup','[0,0,1]')",
    );
    assert_eq!(said, "constraint failed");
    assert_eq!(
        column(&connection, "SELECT rowid FROM st ORDER BY rowid"),
        vec!["1"]
    );
    let wide = refusal(
        &connection,
        "INSERT INTO st(rowid,body,vector) VALUES (3,'three','[0,1,0]'),(4,'four','[0,1]')",
    );
    assert!(!wide.is_empty());
    assert_eq!(
        column(&connection, "SELECT rowid FROM st ORDER BY rowid"),
        vec!["1"]
    );
    exec(
        &connection,
        "INSERT INTO st(rowid,body,vector) VALUES (5,'five','[0,1,0]')",
    );
    assert_eq!(
        column(&connection, "SELECT rowid FROM st ORDER BY rowid"),
        vec!["1", "5"]
    );
}

/// The same failure inside a transaction leaves the earlier statements of the transaction alone.
#[test]
fn a_failed_insert_inside_a_transaction_keeps_the_earlier_statements() {
    let connection = start_inillucent(AREA, "atomic-txn");
    two_row_table(&connection);
    exec(&connection, "BEGIN");
    exec(
        &connection,
        "INSERT INTO st(rowid,body,vector) VALUES (2,'two','[0,1,0]')",
    );
    refusal(
        &connection,
        "INSERT INTO st(rowid,body,vector) VALUES (3,'three','[0,1,0]'),(1,'dup','[0,0,1]')",
    );
    exec(&connection, "COMMIT");
    assert_eq!(
        column(&connection, "SELECT rowid FROM st ORDER BY rowid"),
        vec!["1", "2"]
    );
}

/// `UPDATE ... SET rowid` moves the row and keeps its text.
#[test]
fn an_update_of_the_rowid_changes_the_rowid_and_not_the_text() {
    let connection = start_inillucent(AREA, "update-rowid");
    two_row_table(&connection);
    exec(&connection, "UPDATE st SET rowid = 50 WHERE rowid = 1");
    assert_eq!(column(&connection, "SELECT rowid FROM st"), vec!["50"]);
    assert_eq!(column(&connection, "SELECT body FROM st"), vec!["one"]);
    assert_eq!(
        column(
            &connection,
            "SELECT rowid FROM st WHERE st MATCH 'one' AND k = 5"
        ),
        vec!["50"]
    );
}

/// `UPDATE ... SET rowid` onto a rowid that exists is refused and changes nothing.
#[test]
fn an_update_onto_an_existing_rowid_is_refused() {
    let connection = start_inillucent(AREA, "update-rowid-taken");
    two_row_table(&connection);
    exec(
        &connection,
        "INSERT INTO st(rowid,body,vector) VALUES (2,'two','[0,1,0]')",
    );
    let said = refusal(&connection, "UPDATE st SET rowid = 2 WHERE rowid = 1");
    assert_eq!(said, "constraint failed");
    assert_eq!(
        column(
            &connection,
            "SELECT rowid || ':' || body FROM st ORDER BY rowid"
        ),
        vec!["1:one", "2:two"]
    );
}

/// Creates the two column table the MATCH syntax cases search.
///
/// @param connection - the connection to create it on
fn syntax_table(connection: &Connection<'_>) {
    exec(
        connection,
        "CREATE VIRTUAL TABLE st USING inillucent_search(title, body, dims=3)",
    );
    exec(
        connection,
        "INSERT INTO st(rowid,title,body,vector) VALUES (1,'alpha','beta','[1,0,0]'),(2,'alpha','gamma','[0,1,0]'),(3,'delta','beta','[0,0,1]')",
    );
}

/// Plain words are an OR of the words, as `docs/architecture.md` says.
#[test]
fn plain_words_score_a_row_that_holds_any_of_them() {
    let connection = start_inillucent(AREA, "match-plain");
    syntax_table(&connection);
    let found = column(
        &connection,
        "SELECT rowid FROM st WHERE st MATCH 'alpha beta' AND k=10 ORDER BY rowid",
    );
    assert_eq!(found, vec!["1", "2", "3"]);
}

/// FTS5 operators are refused with the status `unsupported` instead of being read as plain words.
#[test]
fn fts5_operators_are_refused_instead_of_read_as_words() {
    let connection = start_inillucent(AREA, "match-operators");
    syntax_table(&connection);
    for query in [
        "alpha NOT beta",
        "body:alpha",
        "\"alpha beta\"",
        "alp*",
        "alpha AND beta",
    ] {
        let sql = format!(
            "SELECT rowid FROM st WHERE st MATCH '{}' AND k=10",
            query.replace('\'', "''")
        );
        let error = connection
            .execute_batch(&sql)
            .expect_err(&format!("{query} was accepted"));
        assert!(
            error.unsupported().is_some(),
            "{query}: {}",
            error.message()
        );
        assert!(
            error.message().contains("MATCH"),
            "{query}: {}",
            error.message()
        );
    }
    for query in [
        "search_query: alpha",
        "what is alpha?",
        "not beta",
        "alpha and beta",
        "alpha OR beta",
    ] {
        let sql = format!("SELECT rowid FROM st WHERE st MATCH '{query}' AND k=10");
        let found = column(&connection, &sql);
        assert!(!found.is_empty(), "{query}");
    }
}

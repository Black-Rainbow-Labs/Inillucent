//! A vector index follows the table it is on, and `integrity_check` compares the two.
//!
//! Invariant: **each statement is applied to the vector indexes of the table it wrote and to no
//! other table's, `confidence()` on a cosine table does not depend on the length of the query
//! vector, and `PRAGMA integrity_check` reports an index whose vector count differs from the
//! rows it indexes.**
//!
//! ## The defects
//!
//! A statement's row images carry no table name, and the engine applied them to every vector
//! index in the file. With vector indexes on two tables, an insert into the first table read the
//! second table's column position out of its own row. A text column sat at that position, so the
//! insert failed with "it is not a vector of the declared width", and `integrity_check` said `ok`.
//!
//! The cosine query vector was used at the length the caller passed, so the same direction at a
//! shorter length gave a lower `confidence()`.
//!
//! `integrity_check` walked trees and the free map only, so a row missing from an index was not
//! reported.

use inillucent_compat::differential::start_inillucent;
use inillucent_compat::rendering::datum_text as render;
use inillucent_engine::connect::Connection;

/// Where this suite's scratch databases live.
const AREA: &str = "vector_index_agreement";

/// Runs statements that must succeed.
///
/// @param connection - the connection
/// @param sql - the statements
fn exec(connection: &Connection<'_>, sql: &str) {
    connection
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
}

/// Returns the first column of every row of a query, as text.
///
/// @param connection - the connection
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

/// Returns a little endian `f32` blob as hexadecimal, for an `x'...'` literal.
///
/// @param vector - the values
fn hex(vector: &[f32]) -> String {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// The same direction at lengths 1, 0.9 and 0.45 gives the same `confidence()` on a cosine table.
#[test]
fn confidence_does_not_change_with_the_length_of_the_query_vector() {
    let connection = start_inillucent(AREA, "cosine_confidence");
    exec(
        &connection,
        "CREATE VIRTUAL TABLE points USING inillucent_search(label, dims = 3);
         INSERT INTO points (rowid, label, vector) VALUES (1, 'a', x'0000803f0000000000000000');
         INSERT INTO points (rowid, label, vector) VALUES (2, 'b', x'000000000000803f00000000');
         INSERT INTO points (rowid, label, vector) VALUES (3, 'c', x'00000000000000000000803f');",
    );
    let mut seen = Vec::new();
    for length in [1.0f32, 0.9, 0.45] {
        let sql = format!(
            "SELECT confidence(points) FROM points WHERE vector = x'{}' AND k = 1",
            hex(&[length * 0.6, length * 0.8, 0.0])
        );
        seen.push(column(&connection, &sql));
    }
    assert!(!seen[0].is_empty(), "the query returned no row");
    assert_eq!(seen[0], seen[1], "length 1 against length 0.9");
    assert_eq!(seen[0], seen[2], "length 1 against length 0.45");
}

/// An insert into one table is not applied to the vector index of another table.
#[test]
fn an_insert_is_applied_to_the_indexes_of_its_own_table() {
    let connection = start_inillucent(AREA, "two_tables");
    exec(
        &connection,
        "CREATE TABLE embedding (id INTEGER PRIMARY KEY, source TEXT, v VECTOR(3));
         CREATE INDEX ix ON embedding USING inillucent_hnsw (v);
         CREATE TABLE e2 (id INTEGER PRIMARY KEY, v VECTOR(3));
         CREATE INDEX ix2 ON e2 USING inillucent_hnsw (v);
         INSERT INTO embedding VALUES (1, 'a', '[1, 0, 0]');
         INSERT INTO e2 VALUES (1, '[0, 1, 0]');
         UPDATE embedding SET source = 'b' WHERE id = 1;
         INSERT INTO embedding VALUES (2, 'c', '[0, 0, 1]');
         DELETE FROM embedding WHERE id = 1;",
    );
    assert_eq!(
        column(
            &connection,
            "SELECT id FROM embedding ORDER BY vector_distance_cos(v, '[0, 0, 1]') LIMIT 3"
        ),
        vec!["2".to_string()]
    );
    assert_eq!(column(&connection, "PRAGMA integrity_check"), vec!["ok"]);
}

/// `integrity_check` reports a vector index that lost a row its table still has.
#[test]
fn integrity_check_reports_a_vector_index_missing_a_row() {
    let connection = start_inillucent(AREA, "hnsw_missing_row");
    exec(
        &connection,
        "CREATE TABLE d (id INTEGER PRIMARY KEY, e VECTOR(3));
         INSERT INTO d VALUES (1, '[1, 0, 0]'), (2, '[0, 1, 0]'), (3, NULL);
         CREATE INDEX di ON d USING inillucent_hnsw (e);",
    );
    assert_eq!(column(&connection, "PRAGMA integrity_check"), vec!["ok"]);
    exec(&connection, "DELETE FROM di_content WHERE id = 2");
    let said = column(&connection, "PRAGMA integrity_check");
    assert_ne!(
        said,
        vec!["ok"],
        "a row missing from the index was not reported"
    );
    assert!(said[0].contains("di"), "{said:?}");
}

/// `integrity_check` reports an `inillucent_search` table whose index lost a row.
#[test]
fn integrity_check_reports_a_search_table_whose_index_lost_a_row() {
    let connection = start_inillucent(AREA, "search_missing_row");
    exec(
        &connection,
        "CREATE VIRTUAL TABLE s USING inillucent_search(body, dims = 3);
         INSERT INTO s (rowid, body, vector) VALUES (1, 'one', x'0000803f0000000000000000');
         INSERT INTO s (rowid, body, vector) VALUES (2, 'two', x'000000000000803f00000000');
         INSERT INTO s (rowid, body, vector) VALUES (3, 'three', x'00000000000000000000803f');",
    );
    assert_eq!(column(&connection, "PRAGMA integrity_check"), vec!["ok"]);
    exec(&connection, "DELETE FROM s_delta WHERE id = 2");
    let said = column(&connection, "PRAGMA integrity_check");
    assert_ne!(
        said,
        vec!["ok"],
        "a row missing from the index was not reported"
    );
    assert!(said[0].contains("search table s"), "{said:?}");
}

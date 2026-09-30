//! The `fusion` option of `inillucent_search`.
//!
//! Invariant: a table's `fusion` option decides how its keyword list and its vector list are
//! combined, it survives closing and opening the database, and a declaration that contradicts
//! itself is refused when the table is created instead of one half being ignored.
//!
//! The worked example is the one in `docs/rag-explained.md`: keyword search returns A, B, C and
//! vector search returns C, D, A. Under reciprocal rank fusion A and C come first with the score
//! `1/61 + 1/63`, and B and D follow with `1/62`.

use std::sync::Arc;

use inillucent_compat::facade::{Connection, Database};
use inillucent_ext::registry::FunctionFlags;
use inillucent_value::Value;

/// Returns a database file of this test's own, under the gitignored root.
fn scratch() -> std::path::PathBuf {
    let root = inillucent_compat::workspace_root().join("_agent_output/search_fusion");
    let _ = std::fs::create_dir_all(&root);
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = root.join(format!("{}-{serial}.rdb", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

/// Registers `v3(a, b, c)`, which makes a three dimensional vector from three numbers.
///
/// @param connection - the connection to register it on
fn register_v3(connection: &Connection) {
    connection
        .create_scalar_function(
            "v3",
            3,
            FunctionFlags::external(),
            Arc::new(|arguments: &[Value<'static>]| {
                let mut bytes = Vec::with_capacity(12);
                for argument in arguments.iter().take(3) {
                    let component = argument
                        .as_real()
                        .or_else(|| argument.as_integer().map(|whole| whole as f64))
                        .unwrap_or(0.0) as f32;
                    bytes.extend_from_slice(&component.to_le_bytes());
                }
                Value::owned_blob(&bytes)
            }),
        )
        .expect("registers");
}

/// Creates the four row table of the worked example and fills it.
///
/// Rows 1 to 4 are A, B, C and D. B has no vector, so it is in the keyword list only. The keyword `apple` is in A, B and C, most often in A and least
/// often in C, so keyword search ranks them A, B, C. The vector `v3(1, 0, 0)` is closest to C, then
/// D, then A, and B has no vector, so vector search ranks them C, D, A.
///
/// @param connection - the connection, with `v3` registered
/// @param declared - the options after the columns, such as `fusion = 'rrf'`
fn build(connection: &Connection, declared: &str) {
    connection
        .execute(&format!(
            "CREATE VIRTUAL TABLE t USING inillucent_search(body, dims = 3{declared})"
        ))
        .expect("creates the search table");
    connection
        .execute(
            "INSERT INTO t (rowid, body, vector) VALUES \
             (1, 'apple apple apple', v3(0.5, 0.866, 0)), \
             (2, 'apple apple pear pear pear pear', NULL), \
             (3, 'apple pear pear pear pear pear pear pear pear', v3(1, 0, 0)), \
             (4, 'pear', v3(0.9, 0.436, 0))",
        )
        .expect("fills the search table");
}

/// Returns each row's rowid and score for a query, in the order the query returns them.
///
/// @param connection - the connection to ask
/// @param sql - the query, selecting `rowid` and `score(t)`
fn scored(connection: &Connection, sql: &str) -> Vec<(i64, f64)> {
    connection
        .query(sql)
        .unwrap_or_else(|error| panic!("{sql} failed: {}", error.message()))
        .iter()
        .map(|row| {
            (
                row.first().and_then(Value::as_integer).unwrap_or(-1),
                row.get(1).and_then(Value::as_real).unwrap_or(f64::NAN),
            )
        })
        .collect()
}

/// Returns the `fusion` value stored in the table's `%_config` shadow table.
///
/// @param connection - the connection to ask
fn stored_fusion(connection: &Connection) -> String {
    let rows = connection
        .query("SELECT v FROM t_config WHERE k = 'fusion'")
        .expect("reads the config");
    rows.first()
        .and_then(|row| row.first())
        .and_then(Value::as_text)
        .map(|text| String::from_utf8_lossy(text.raw()).into_owned())
        .unwrap_or_default()
}

/// Returns the refusal a statement gives, as one string.
///
/// @param connection - the connection to run it on
/// @param sql - a statement that must fail
fn refusal(connection: &Connection, sql: &str) -> String {
    let error = connection
        .execute(sql)
        .expect_err("the statement must be refused");
    format!("{} {}", error.message(), error.detail().unwrap_or_default())
}

/// The two lists the worked example starts from are the ones the example says they are.
///
/// Without this the RRF assertions below could pass for the wrong reason: a keyword list of A, C, B
/// would change which rows come first without the fusion being wrong.
#[test]
fn the_two_input_lists_are_a_b_c_and_c_d_a() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    register_v3(&connection);
    build(&connection, "");
    let keyword: Vec<i64> = scored(
        &connection,
        "SELECT rowid, score(t) FROM t WHERE t MATCH 'apple' AND k = 10 ORDER BY rank",
    )
    .iter()
    .map(|(rowid, _)| *rowid)
    .collect();
    assert_eq!(keyword, vec![1, 2, 3], "the keyword list");
    let vector: Vec<i64> = scored(
        &connection,
        "SELECT rowid, score(t) FROM t WHERE vector = v3(1, 0, 0) AND k = 3 ORDER BY rank",
    )
    .iter()
    .map(|(rowid, _)| *rowid)
    .collect();
    assert_eq!(vector, vec![3, 4, 1], "the vector list");
}

/// Runs the fused query of the worked example and returns every row it ranks, best first.
///
/// @param connection - the connection, with the table built
fn fused(connection: &Connection) -> Vec<(i64, f64)> {
    scored(
        connection,
        "SELECT rowid, score(t) FROM t WHERE t MATCH 'apple' AND vector = v3(1, 0, 0) \
         AND k = 4 AND recall = 1.0 ORDER BY rank",
    )
}

/// Checks the four scores the worked example gives under reciprocal rank fusion.
///
/// A and C are in both lists, so each scores `1/61 + 1/63`. B is in the keyword list only and D in
/// the vector list only, so each scores `1/62`. The score is a 32 bit float inside the engine, so
/// the expected values are computed as 32 bit floats too and compared to 1e-9. A fusion that scaled
/// the lists and added them, or that used another damping constant, fails by far more than that.
///
/// @param rows - the rowid and score of each row the query returned
fn assert_worked_example(rows: &[(i64, f64)]) {
    let both = f64::from(1.0f32 / 61.0 + 1.0f32 / 63.0);
    let one = f64::from(1.0f32 / 62.0);
    assert_eq!(
        rows.len(),
        4,
        "every row is in one list or the other: {rows:?}"
    );
    for (rowid, score) in rows {
        let expected = if matches!(rowid, 1 | 3) { both } else { one };
        assert!(
            (score - expected).abs() < 1e-9,
            "row {rowid} scored {score}, expected {expected}: {rows:?}"
        );
    }
    let first_two: Vec<i64> = rows.iter().take(2).map(|(rowid, _)| *rowid).collect();
    assert!(
        first_two == vec![1, 3] || first_two == vec![3, 1],
        "A and C come first: {rows:?}"
    );
}

/// `fusion = 'rrf'` scores the worked example the way `docs/rag-explained.md` computes it by hand.
#[test]
fn reciprocal_rank_fusion_scores_the_worked_example() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    register_v3(&connection);
    build(&connection, ", fusion = 'rrf'");
    assert_worked_example(&fused(&connection));
}

/// A table that declares `fusion = 'rrf'` still declares it after the database is closed and opened.
///
/// `%_config` holds the value. Reading it back is the path every later session takes.
#[test]
fn a_reopened_table_keeps_its_fusion() {
    let path = scratch();
    {
        let database = Database::open(&path).expect("opens");
        let connection = database.session().expect("connects");
        register_v3(&connection);
        build(&connection, ", fusion = 'rrf'");
    }
    let database = Database::open(&path).expect("opens again");
    let connection = database.session().expect("connects again");
    register_v3(&connection);
    assert_eq!(stored_fusion(&connection), "rrf");
    assert_worked_example(&fused(&connection));
}

/// With no `fusion` and no `vector_weight` a table keeps the adaptive fusion, and its score is not a reciprocal rank score.
#[test]
fn the_default_fusion_is_still_adaptive() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    register_v3(&connection);
    build(&connection, "");
    assert_eq!(stored_fusion(&connection), "adaptive");
    let rows = scored(
        &connection,
        "SELECT rowid, score(t) FROM t WHERE t MATCH 'apple' AND vector = v3(1, 0, 0) \
         AND k = 4 AND recall = 1.0 ORDER BY rank",
    );
    let top = rows.first().map(|(_, score)| *score).unwrap_or(0.0);
    assert!(top > 2.0 / 61.0 + 1e-6, "the adaptive score was {top}");
}

/// `vector_weight` alone still means a fixed weight, and it is stored as `weighted`.
#[test]
fn a_vector_weight_alone_is_weighted() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    register_v3(&connection);
    build(&connection, ", vector_weight = 0.5");
    assert_eq!(stored_fusion(&connection), "weighted");
}

/// Every contradiction and every unknown value is refused when the table is created, and the message names the option.
#[test]
fn a_contradictory_fusion_is_refused_at_create() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    let cases = [
        (", fusion = 'rrf', vector_weight = 0.5", "has no weight"),
        (", fusion = 'weighted'", "needs vector_weight"),
        (
            ", fusion = 'adaptive', vector_weight = 0.5",
            "has no weight",
        ),
        (", fusion = 'borda'", "fusion must be"),
    ];
    for (declared, expected) in cases {
        let said = refusal(
            &connection,
            &format!("CREATE VIRTUAL TABLE bad USING inillucent_search(body, dims = 3{declared})"),
        );
        assert!(
            said.contains(expected) && said.contains("fusion"),
            "{declared} was refused with: {said}"
        );
    }
}

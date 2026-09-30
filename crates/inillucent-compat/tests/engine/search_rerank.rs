//! The `rerank_depth` option and the `question` column of `inillucent_search`, and the model
//! functions in a build with no embedding support. None of these cases loads a model.
//!
//! Invariant: `rerank_depth` is a whole number from 1 to 1,000, refused when the table is created
//! and read back after a reopen; a search that names `question` and has no candidates to reorder is
//! refused by name; and `rerank`, `embed_tokens` and `question` either resolve, in a build with
//! embedding support, or answer `unsupported` with a message that says the build lacks it, in a build
//! without. They are never "no such function".

use inillucent_compat::facade::{Connection, Database};
use inillucent_value::Value;

/// Returns a database file of this test's own, under the gitignored root.
fn scratch() -> std::path::PathBuf {
    let root = inillucent_compat::workspace_root().join("_agent_output/search_rerank");
    let _ = std::fs::create_dir_all(&root);
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = root.join(format!("{}-{serial}.rdb", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

/// Opens a connection to a database of this test's own.
fn connect() -> Connection {
    let database = Database::open(scratch()).expect("opens");
    Box::leak(Box::new(database)).session().expect("connects")
}

/// Returns a refusal as one string: the message, then the detail.
///
/// @param error - what the engine refused with
fn said(error: &inillucent_base::DbError) -> String {
    format!("{} {}", error.message(), error.detail().unwrap_or_default())
}

/// Returns the value stored under a key in a table's `%_config`.
///
/// @param connection - the connection to ask
/// @param table - the search table's name
/// @param key - the config key
fn stored(connection: &Connection, table: &str, key: &str) -> Option<String> {
    let rows = connection
        .query(&format!("SELECT v FROM {table}_config WHERE k = '{key}'"))
        .expect("reads the config");
    rows.first()
        .and_then(|row| row.first())
        .and_then(Value::as_text)
        .map(|text| String::from_utf8_lossy(text.raw()).into_owned())
}

/// A whole number from 1 to 1,000 is accepted, stored, and read back after the database is closed and opened again.
#[test]
fn a_rerank_depth_is_stored_and_survives_a_reopen() {
    let path = scratch();
    {
        let database = Database::open(&path).expect("opens");
        let connection = database.session().expect("connects");
        connection
            .execute("CREATE VIRTUAL TABLE deep USING inillucent_search(body, dims = 3, rerank_depth = 25)")
            .expect("creates");
        connection
            .execute("CREATE VIRTUAL TABLE plain USING inillucent_search(body, dims = 3)")
            .expect("creates");
        assert_eq!(
            stored(&connection, "deep", "rerank_depth").as_deref(),
            Some("25")
        );
        assert_eq!(
            stored(&connection, "plain", "rerank_depth").as_deref(),
            Some("")
        );
        for value in ["1", "1000"] {
            connection
                .execute(&format!(
                    "CREATE VIRTUAL TABLE edge{value} USING inillucent_search(body, rerank_depth = {value})"
                ))
                .expect("the ends of the range are accepted");
        }
    }
    let database = Database::open(&path).expect("opens again");
    let connection = database.session().expect("connects again");
    connection
        .execute("INSERT INTO deep (rowid, body) VALUES (1, 'a row so the table is read')")
        .expect("writes after a reopen, which reads the stored options");
    assert_eq!(
        stored(&connection, "deep", "rerank_depth").as_deref(),
        Some("25")
    );
}

/// A depth outside 1 to 1,000, or one that is not a whole number, is refused when the table is created, and the message names the option.
#[test]
fn a_bad_rerank_depth_is_refused_at_create() {
    let connection = connect();
    for value in ["0", "1001", "-3", "60.5", "'sixty'", "1e3"] {
        let error = connection
            .execute(&format!(
                "CREATE VIRTUAL TABLE bad USING inillucent_search(body, rerank_depth = {value})"
            ))
            .expect_err("the depth must be refused");
        let text = said(&error);
        assert!(
            text.contains("rerank_depth") && text.contains("1000"),
            "{value} was refused with: {text}"
        );
    }
}

/// A search that names `question` with neither `MATCH` nor `vector` has nothing to reorder, and is refused by name.
///
/// The refusal is the same in every build, because it is decided before the reranker is looked for.
#[test]
fn a_question_with_no_search_is_refused() {
    let connection = connect();
    connection
        .execute("CREATE VIRTUAL TABLE docs USING inillucent_search(body, dims = 3)")
        .expect("creates");
    connection
        .execute("INSERT INTO docs (rowid, body) VALUES (1, 'apples in the orchard')")
        .expect("fills");
    let error = connection
        .query("SELECT rowid FROM docs WHERE question = 'which fruit' AND k = 5")
        .expect_err("a question with no search must be refused");
    let text = said(&error);
    assert!(
        text.contains("question") && text.contains("MATCH"),
        "the refusal must say what to add: {text}"
    );
    assert!(
        error.requirement().is_some(),
        "the refusal must carry the marker the driver reports as invalid_state: {error:?}"
    );
}

/// `question` is the fifth argument of the table function form, after the recall, and a plain search still works beside it.
#[test]
fn question_is_the_fifth_argument_of_the_table_function() {
    let connection = connect();
    connection
        .execute("CREATE VIRTUAL TABLE docs USING inillucent_search(body)")
        .expect("creates");
    connection
        .execute("INSERT INTO docs (rowid, body) VALUES (1, 'apples in the orchard'), (2, 'pears on the shelf')")
        .expect("fills");
    let plain = connection
        .query("SELECT rowid FROM docs('apples', 5) ORDER BY rank")
        .expect("the ordinary table function form still works");
    assert_eq!(plain.len(), 1);
    // Five arguments name the hidden columns up to and including `question`: query, k, vector,
    // recall and question. The statement is only prepared, so no reranker is looked for and none is
    // loaded. The fifth hidden column is `question`, so the statement is accepted, and `rank` moved
    // to the sixth place, behind it.
    connection
        .prepare(
            "SELECT rowid FROM docs('apples', 5, NULL, NULL, 'which fruit grows in an orchard')",
        )
        .expect("question is accepted as the fifth argument");
}

/// `rerank` and `embed_tokens` resolve in a build with embedding support and answer `unsupported` in one without.
///
/// They are never "no such function". The probe prepares and does not run, so no model is loaded.
/// Under the runner this suite's build has the feature, which `embed_direct_only` also asks for;
/// under a plain `cargo test` it does not, and this is the case that checks the second half.
#[test]
fn the_model_functions_resolve_or_answer_unsupported_and_never_no_such_function() {
    let connection = connect();
    for sql in [
        "SELECT rerank('a question', 'a passage')",
        "SELECT embed_tokens('some text')",
    ] {
        match connection.prepare(sql) {
            Ok(_) => {}
            Err(error) => {
                let text = said(&error);
                assert!(
                    text.contains("no embedding support compiled in"),
                    "{sql} was refused for the wrong reason: {text}"
                );
                assert!(
                    error.unsupported().is_some(),
                    "{sql} must be marked unsupported so the command line exits with 3: {error:?}"
                );
                assert!(!text.contains("no such function"), "{text}");
            }
        }
    }
}

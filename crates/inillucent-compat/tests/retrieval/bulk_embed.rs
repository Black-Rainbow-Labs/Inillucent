//! `inillucent embed`, the bulk embedding command, on the processor.
//!
//! Invariant: a vector the command writes is the vector `embed(prefix || text)` returns for the
//! same row, byte for byte on the processor; a run continues at the first row whose vector is
//! still `NULL`; every row that was skipped or cut at the token limit is counted and, for a cut
//! row, named; and a device that will not start is an error and not a run on the processor.
//!
//! Every case drives the built `inillucent` program against a scratch database. The suite skips
//! when the build has no `embed` function or the machine has no model. The graphics card cases are
//! in `bulk_embed_cuda.rs`, because a machine without a card declares that suite absent and the
//! processor cases should still run there.

use std::path::{Path, PathBuf};
use std::process::Command;

use inillucent_compat::cliproc::{document, field, items, number_of, program, text_of};

/// What one run of the program produced.
struct Ran {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Ran {
    /// Returns both streams, for a failure message.
    fn said(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

/// Runs the program with arguments and environment variables set for this one process.
///
/// The settings variables are cleared first, so a developer's shell cannot change what a case checks.
///
/// @param binary - the built `inillucent`
/// @param arguments - everything after the program name
/// @param variables - environment variables to set
fn run(binary: &PathBuf, arguments: &[&str], variables: &[(&str, &str)]) -> Ran {
    let output = Command::new(binary)
        .args(arguments)
        .env_remove("INILLUCENT_EMBED_THREADS")
        .env_remove("INILLUCENT_EMBED_DEVICE")
        .env_remove("INILLUCENT_EMBED_RESIDENCY")
        .envs(variables.iter().copied())
        .output()
        .unwrap_or_else(|error| panic!("inillucent did not start: {error}"));
    Ran {
        code: output.status.code().unwrap_or(130),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Returns a scratch database path of this suite's own.
///
/// @param name - the case's name
fn scratch(name: &str) -> PathBuf {
    let root = inillucent_compat::workspace_root().join("_agent_output/bulk-embed");
    let _ = std::fs::create_dir_all(&root);
    let path = root.join(format!("{}-{name}.rdb", std::process::id()));
    inillucent_base::testing::remove_database(&path);
    path
}

/// Returns the program when this build can embed and the machine has the model, and says why not otherwise.
fn embedder_or_skip() -> Option<PathBuf> {
    let binary = program("inillucent");
    let probe = run(
        &binary,
        &[
            "--db",
            ":memory:",
            "query",
            "SELECT length(embed('a word'))",
        ],
        &[],
    );
    if probe.code != 0 {
        inillucent_compat::differential::skipping(&format!(
            "this build cannot answer embed(TEXT): {}. Run `inillucent setup-embeddings all` and \
             build with --features inillucent-cli/embed",
            probe.said().lines().next().unwrap_or("")
        ));
        return None;
    }
    Some(binary)
}

/// Runs a statement against a database and panics with what the program said when it fails.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
/// @param verb - `exec` or `query`
/// @param sql - the statement
fn sql(binary: &PathBuf, database: &Path, verb: &str, sql: &str) -> Ran {
    let path = database.to_string_lossy().into_owned();
    let ran = run(binary, &["--db", &path, verb, sql, "--output", "json"], &[]);
    assert_eq!(ran.code, 0, "{sql} failed: {}", ran.said());
    ran
}

/// Returns the first cell of the first row of a query, as an integer.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
/// @param query - a query that returns one integer
fn count(binary: &PathBuf, database: &Path, query: &str) -> i64 {
    let ran = sql(binary, database, "query", query);
    inillucent_compat::cliproc::rows(&ran.stdout)
        .first()
        .and_then(|row| row.first())
        .and_then(|cell| cell.parse().ok())
        .unwrap_or_else(|| panic!("{query} did not answer an integer: {}", ran.said()))
}

/// Creates the table this suite embeds: 40 rows of growing length, a `NULL`, an empty text and one text over the token limit.
///
/// Row `i` from 1 to 40 holds `i` repetitions of a phrase of eight tokens, so the token counts run
/// from 10 to about 330 and a batch of them pads. Row 41 is `NULL`, row 42 is empty and row 43 is 2,000 words, which is
/// over the 1,900 token limit.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
fn build_corpus(binary: &PathBuf, database: &Path) {
    let path = database.to_string_lossy().into_owned();
    let created = run(binary, &["create", &path], &[]);
    assert_eq!(created.code, 0, "{}", created.said());
    sql(
        binary,
        database,
        "exec",
        "CREATE TABLE chunk (id INTEGER PRIMARY KEY, body TEXT, v VECTOR(768))",
    );
    sql(
        binary,
        database,
        "exec",
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 40) \
         INSERT INTO chunk (id, body) SELECT i, 'note ' || i || ' ' || \
         replace(hex(zeroblob(i)), '00', 'cats sleep on warm mats. ') FROM n",
    );
    sql(
        binary,
        database,
        "exec",
        "INSERT INTO chunk (id, body) VALUES (41, NULL), (42, ''), \
         (43, replace(hex(zeroblob(1000)), '0', 'word '))",
    );
}

/// Reads one number out of the command's JSON result.
///
/// @param stdout - what the command printed
/// @param name - the member's name
fn number(stdout: &str, name: &str) -> f64 {
    let node = document(stdout);
    field(&node, name)
        .and_then(number_of)
        .unwrap_or_else(|| panic!("no number {name} in {stdout}"))
}

/// The command's arguments for the corpus, without a device.
///
/// @param database - the database file, as text
fn embed_arguments(database: &str) -> Vec<String> {
    [
        "--db",
        database,
        "embed",
        "--table",
        "chunk",
        "--text",
        "body",
        "--vector",
        "v",
        "--prefix",
        "search_document: ",
        "--output",
        "json",
    ]
    .iter()
    .map(|word| word.to_string())
    .collect()
}

/// Runs `inillucent embed` over the corpus and returns the run.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
/// @param extra - more arguments, such as `--all`
fn embed(binary: &PathBuf, database: &Path, extra: &[&str]) -> Ran {
    let path = database.to_string_lossy().into_owned();
    let mut arguments = embed_arguments(&path);
    if !extra.contains(&"--device") {
        arguments.extend(["--device".to_string(), "cpu".to_string()]);
    }
    arguments.extend(extra.iter().map(|word| word.to_string()));
    let borrowed: Vec<&str> = arguments.iter().map(String::as_str).collect();
    run(binary, &borrowed, &[])
}

/// A vector the command wrote is the vector `embed()` returns for the same text, byte for byte.
///
/// The batch the command runs pads short texts to the longest in it, and `embed()` runs one text
/// at a time, so this is also the check that padding does not move a vector.
#[test]
fn a_bulk_vector_equals_embed_byte_for_byte_on_the_processor() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let database = scratch("equal");
    build_corpus(&binary, &database);
    let ran = embed(&binary, &database, &[]);
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(number(&ran.stdout, "embedded"), 41.0, "{}", ran.stdout);
    let different = count(
        &binary,
        &database,
        "SELECT count(*) FROM chunk WHERE body IS NOT NULL AND body != '' \
         AND v IS NOT embed('search_document: ' || body)",
    );
    assert_eq!(
        different, 0,
        "vectors that differ from embed(): {}",
        ran.stdout
    );
    assert_eq!(
        count(
            &binary,
            &database,
            "SELECT count(*) FROM chunk WHERE v IS NULL"
        ),
        2
    );
}

/// The report counts the skipped rows and the row cut at the token limit, and names the cut row.
#[test]
fn the_report_counts_skipped_and_truncated_rows_and_names_the_cut_one() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let database = scratch("report");
    build_corpus(&binary, &database);
    let ran = embed(&binary, &database, &[]);
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(number(&ran.stdout, "embedded"), 41.0, "{}", ran.stdout);
    assert_eq!(number(&ran.stdout, "skipped"), 2.0, "{}", ran.stdout);
    assert_eq!(number(&ran.stdout, "truncated"), 1.0, "{}", ran.stdout);
    let node = document(&ran.stdout);
    let named: Vec<f64> = field(&node, "truncated_rowids")
        .and_then(items)
        .unwrap_or_default()
        .iter()
        .filter_map(number_of)
        .collect();
    assert_eq!(named, vec![43.0], "{}", ran.stdout);
    assert_eq!(
        field(&node, "device").and_then(text_of).as_deref(),
        Some("cpu")
    );
    assert!(number(&ran.stdout, "rows_per_second") > 0.0);
}

/// A rerun embeds only the rows whose vector is `NULL`, and leaves the others as they were.
///
/// A run that stopped partway leaves exactly this state: some rows filled and the rest `NULL`. The
/// rows given a `NULL` here are the ones a stopped run would not have reached.
#[test]
fn a_rerun_continues_where_the_last_run_ended() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let database = scratch("resume");
    build_corpus(&binary, &database);
    assert_eq!(embed(&binary, &database, &[]).code, 0);
    let before = count(
        &binary,
        &database,
        "SELECT sum(length(hex(v))) FROM chunk WHERE id % 2 = 1",
    );
    sql(
        &binary,
        &database,
        "exec",
        "UPDATE chunk SET v = NULL WHERE id % 2 = 0",
    );
    let ran = embed(&binary, &database, &["--commit-every", "5"]);
    assert_eq!(ran.code, 0, "{}", ran.said());
    // 20 even rows from 2 to 40 and 42, of which 42 is empty, and 41 is NULL and odd.
    assert_eq!(number(&ran.stdout, "embedded"), 20.0, "{}", ran.stdout);
    let after = count(
        &binary,
        &database,
        "SELECT sum(length(hex(v))) FROM chunk WHERE id % 2 = 1",
    );
    assert_eq!(
        before, after,
        "the rows that already had a vector were touched"
    );
    let different = count(
        &binary,
        &database,
        "SELECT count(*) FROM chunk WHERE body IS NOT NULL AND body != '' \
         AND v IS NOT embed('search_document: ' || body)",
    );
    assert_eq!(different, 0);
}

/// `--all` embeds every row again, including the rows that already have a vector.
#[test]
fn all_embeds_every_row_again() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let database = scratch("all");
    build_corpus(&binary, &database);
    assert_eq!(embed(&binary, &database, &[]).code, 0);
    let ran = embed(&binary, &database, &["--all"]);
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(number(&ran.stdout, "embedded"), 41.0, "{}", ran.stdout);
    let again = embed(&binary, &database, &[]);
    assert_eq!(number(&again.stdout, "embedded"), 0.0, "{}", again.stdout);
}

/// Two sessions and a thread count give the same vectors as one session.
#[test]
fn two_sessions_write_the_same_vectors_as_one() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let database = scratch("sessions");
    build_corpus(&binary, &database);
    let ran = embed(&binary, &database, &["--sessions", "2", "--threads", "2"]);
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(number(&ran.stdout, "sessions"), 2.0, "{}", ran.stdout);
    let different = count(
        &binary,
        &database,
        "SELECT count(*) FROM chunk WHERE body IS NOT NULL AND body != '' \
         AND vector_distance_cos(v, embed('search_document: ' || body)) > 0.00001",
    );
    assert_eq!(different, 0, "{}", ran.stdout);
}

/// An `inillucent_search` table is a target, and its vector column can be searched afterwards.
///
/// The rows are inserted with no vector, embedded by the command, and then found by a vector query.
#[test]
fn a_search_table_can_be_the_target() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let database = scratch("search-table");
    let path = database.to_string_lossy().into_owned();
    assert_eq!(run(&binary, &["create", &path], &[]).code, 0);
    sql(
        &binary,
        &database,
        "exec",
        "CREATE VIRTUAL TABLE note_search USING inillucent_search(body, dims = 768)",
    );
    sql(
        &binary,
        &database,
        "exec",
        "INSERT INTO note_search (rowid, body) VALUES \
         (1, 'A cat sleeps on the warm mat by the window.'), \
         (2, 'The compiler rejected the program because of a type error.'), \
         (3, 'Bananas and apples are sold at the fruit market.')",
    );
    let ran = run(
        &binary,
        &[
            "--db",
            &path,
            "embed",
            "--table",
            "note_search",
            "--text",
            "body",
            "--vector",
            "vector",
            "--prefix",
            "search_document: ",
            "--device",
            "cpu",
            "--output",
            "json",
        ],
        &[],
    );
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert_eq!(number(&ran.stdout, "embedded"), 3.0, "{}", ran.stdout);
    let first = count(
        &binary,
        &database,
        "SELECT rowid FROM note_search WHERE vector = embed('search_query: a kitten resting on a rug') \
         AND k = 3 ORDER BY rank LIMIT 1",
    );
    assert_eq!(first, 1);
}

/// A table or a column that is not there is refused by name before a model is opened.
#[test]
fn a_missing_table_or_column_is_refused_by_name() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let database = scratch("refuse");
    build_corpus(&binary, &database);
    let path = database.to_string_lossy().into_owned();
    for (table, text, vector, named) in [
        ("nothing", "body", "v", "nothing"),
        ("chunk", "nothing", "v", "nothing"),
        ("chunk", "body", "nothing", "nothing"),
    ] {
        let ran = run(
            &binary,
            &[
                "--db", &path, "embed", "--table", table, "--text", text, "--vector", vector,
            ],
            &[],
        );
        assert_ne!(
            ran.code,
            0,
            "{table}.{text}.{vector} should be refused: {}",
            ran.said()
        );
        assert!(ran.said().contains(named), "{}", ran.said());
    }
}

/// A card that is not there is an error that names the CUDA runtime install, and nothing is written.
///
/// Card 99 exists on no machine, so this checks the error path on a machine with a working CUDA
/// install as well as on one without. It must fail, and it must not fall back to the processor.
#[test]
fn a_cuda_device_that_will_not_start_fails_and_writes_nothing() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let database = scratch("nocuda");
    build_corpus(&binary, &database);
    let ran = embed(&binary, &database, &["--device", "cuda:99"]);
    assert_ne!(ran.code, 0, "{}", ran.said());
    assert!(
        ran.said().contains("setup-embeddings runtime --gpu"),
        "{}",
        ran.said()
    );
    assert_eq!(count(&binary, &database, "SELECT count(v) FROM chunk"), 0);
}

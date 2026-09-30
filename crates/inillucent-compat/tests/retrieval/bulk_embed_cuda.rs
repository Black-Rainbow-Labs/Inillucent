//! `inillucent embed` on a graphics card.
//!
//! Invariant: a vector written on `cuda:0` agrees with `embed()` on the processor to a cosine
//! similarity of at least 0.99999 for every row, and a run on two sessions on the one card writes
//! vectors that agree as closely as a run on one. The card computes in a different order from the
//! processor, so the vectors are close and not equal, which is why the processor cases in
//! `bulk_embed.rs` compare bytes and these compare cosine.
//!
//! The suite skips when the build has no `embed` function, the machine has no model, or the CUDA
//! runtime will not start on card 0. A machine with no card declares `cuda` absent in
//! `tests/prerequisites.local.toml`. Only card 0 is used: card 1 holds the local language model.

use std::path::{Path, PathBuf};
use std::process::Command;

use inillucent_compat::cliproc::{document, field, number_of, program, text_of};

/// The largest cosine distance a card's vector may be from the processor's, which is a similarity of 0.99999.
const MAX_DISTANCE: &str = "0.00001";

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

/// Runs the program with arguments, with the settings variables cleared.
///
/// @param binary - the built `inillucent`
/// @param arguments - everything after the program name
fn run(binary: &PathBuf, arguments: &[&str]) -> Ran {
    let output = Command::new(binary)
        .args(arguments)
        .env_remove("INILLUCENT_EMBED_THREADS")
        .env_remove("INILLUCENT_EMBED_DEVICE")
        .env_remove("INILLUCENT_EMBED_RESIDENCY")
        .output()
        .unwrap_or_else(|error| panic!("inillucent did not start: {error}"));
    Ran {
        code: output.status.code().unwrap_or(130),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// Runs a statement against a database and panics with what the program said when it fails.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
/// @param verb - `exec` or `query`
/// @param statement - the SQL
fn sql(binary: &PathBuf, database: &Path, verb: &str, statement: &str) -> Ran {
    let path = database.to_string_lossy().into_owned();
    let ran = run(
        binary,
        &["--db", &path, verb, statement, "--output", "json"],
    );
    assert_eq!(ran.code, 0, "{statement} failed: {}", ran.said());
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

/// Builds a scratch database with 30 rows of growing length and one over the token limit.
///
/// @param binary - the built `inillucent`
/// @param name - the case's name
fn corpus(binary: &PathBuf, name: &str) -> PathBuf {
    let root = inillucent_compat::workspace_root().join("_agent_output/bulk-embed");
    let _ = std::fs::create_dir_all(&root);
    let database = root.join(format!("{}-cuda-{name}.rdb", std::process::id()));
    inillucent_base::testing::remove_database(&database);
    let path = database.to_string_lossy().into_owned();
    assert_eq!(run(binary, &["create", &path]).code, 0);
    sql(
        binary,
        &database,
        "exec",
        "CREATE TABLE chunk (id INTEGER PRIMARY KEY, body TEXT, v VECTOR(768))",
    );
    sql(
        binary,
        &database,
        "exec",
        "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 30) \
         INSERT INTO chunk (id, body) SELECT i, 'note ' || i || ' ' || \
         replace(hex(zeroblob(i)), '00', 'dogs bark at the mail carrier. ') FROM n",
    );
    sql(
        binary,
        &database,
        "exec",
        "INSERT INTO chunk (id, body) VALUES (31, replace(hex(zeroblob(1000)), '0', 'word '))",
    );
    database
}

/// Returns the program when card 0 can run the model, and says why not otherwise.
fn card_or_skip() -> Option<PathBuf> {
    let binary = program("inillucent");
    let probe = run(
        &binary,
        &[
            "--db",
            ":memory:",
            "query",
            "SELECT length(embed('a word'))",
        ],
    );
    if probe.code != 0 {
        inillucent_compat::differential::skipping(&format!(
            "this build cannot answer embed(TEXT): {}. Run `inillucent setup-embeddings all`",
            probe.said().lines().next().unwrap_or("")
        ));
        return None;
    }
    let database = corpus(&binary, &format!("probe-{:?}", std::thread::current().id()));
    let path = database.to_string_lossy().into_owned();
    let ran = run(
        &binary,
        &[
            "--db", &path, "embed", "--table", "chunk", "--text", "body", "--vector", "v",
            "--device", "cuda:0",
        ],
    );
    if ran.code != 0 {
        inillucent_compat::differential::skipping(&format!(
            "card 0 cannot run the model: {}. Run `inillucent setup-embeddings runtime --gpu`",
            ran.said().lines().next().unwrap_or("")
        ));
        return None;
    }
    Some(binary)
}

/// Runs `inillucent embed` on card 0 with the given number of sessions.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
/// @param sessions - how many sessions to open on the card
fn embed_on_card(binary: &PathBuf, database: &Path, sessions: &str) -> Ran {
    let path = database.to_string_lossy().into_owned();
    run(
        binary,
        &[
            "--db",
            &path,
            "embed",
            "--table",
            "chunk",
            "--text",
            "body",
            "--vector",
            "v",
            "--prefix",
            "search_document: ",
            "--device",
            "cuda:0",
            "--sessions",
            sessions,
            "--output",
            "json",
        ],
    )
}

/// Counts the rows whose vector is farther from `embed()` on the processor than the limit allows.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
fn rows_too_far_from_embed(binary: &PathBuf, database: &Path) -> i64 {
    count(
        binary,
        database,
        &format!(
            "SELECT count(*) FROM chunk WHERE \
             vector_distance_cos(v, embed('search_document: ' || body)) > {MAX_DISTANCE}"
        ),
    )
}

/// Every vector written on card 0 has a cosine similarity of at least 0.99999 with `embed()` on the processor.
#[test]
fn card_vectors_agree_with_embed_to_a_cosine_of_0_99999() {
    let Some(binary) = card_or_skip() else {
        return;
    };
    let database = corpus(&binary, "cosine");
    let ran = embed_on_card(&binary, &database, "1");
    assert_eq!(ran.code, 0, "{}", ran.said());
    let node = document(&ran.stdout);
    assert_eq!(
        field(&node, "device").and_then(text_of).as_deref(),
        Some("cuda:0")
    );
    assert_eq!(
        field(&node, "embedded").and_then(number_of),
        Some(31.0),
        "{}",
        ran.stdout
    );
    assert_eq!(
        rows_too_far_from_embed(&binary, &database),
        0,
        "{}",
        ran.stdout
    );
}

/// Two sessions on the one card write vectors that agree with the processor as closely as one session does.
#[test]
fn two_sessions_on_one_card_agree_with_embed() {
    let Some(binary) = card_or_skip() else {
        return;
    };
    let database = corpus(&binary, "sessions");
    let ran = embed_on_card(&binary, &database, "2");
    assert_eq!(ran.code, 0, "{}", ran.said());
    let node = document(&ran.stdout);
    assert_eq!(
        field(&node, "sessions").and_then(number_of),
        Some(2.0),
        "{}",
        ran.stdout
    );
    assert_eq!(
        rows_too_far_from_embed(&binary, &database),
        0,
        "{}",
        ran.stdout
    );
}

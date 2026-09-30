//! What a machine with no reranker sees.
//!
//! Invariant: `rerank()`, a search that names `question`, and `inillucent search --rerank` fail
//! with the status `invalid_state`, exit code 1, and a message that names
//! `inillucent setup-embeddings reranker`, and they never answer a score of zero, a score from
//! another model, or a quiet fallback to the fused order. `embed_tokens` and the rest of the SQL are
//! not affected.
//!
//! The machine with no reranker is made here: every folder the program searches for a model is
//! pointed at an empty folder of this suite's own, so the result is the same on a developer machine
//! that has the reranker installed. The suite needs only a build with the `embed` function.

use std::path::{Path, PathBuf};
use std::process::Command;

use inillucent_compat::cliproc::program;

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

/// Returns an empty folder of this suite's own, which stands in for every place a model could be.
///
/// @param name - the case's name
fn empty_home(name: &str) -> PathBuf {
    let path = inillucent_compat::workspace_root()
        .join("_agent_output/rerank-absent")
        .join(format!("{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("makes the empty folder");
    path
}

/// Runs the program with every model location pointed at an empty folder.
///
/// @param binary - the built `inillucent`
/// @param home - the empty folder
/// @param arguments - everything after the program name
fn run(binary: &PathBuf, home: &Path, arguments: &[&str]) -> Ran {
    let output = Command::new(binary)
        .args(arguments)
        .env("INILLUCENT_HOME", home)
        .env("INILLUCENT_MODEL_ROOTS", home)
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env_remove("INILLUCENT_ONNX_DIR")
        .env_remove("ORT_DYLIB_PATH")
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

/// Returns the program when this build has the model functions, and says why not otherwise.
fn build_or_skip() -> Option<PathBuf> {
    let binary = program("inillucent");
    let probe = run(
        &binary,
        &empty_home("probe"),
        &["--db", ":memory:", "query", "SELECT rerank(NULL, NULL)"],
    );
    if probe.code != 0 {
        inillucent_compat::differential::skipping(&format!(
            "this build has no embedding support compiled in: {}. Build with --features \
             inillucent-cli/embed",
            probe.said().lines().next().unwrap_or("")
        ));
        return None;
    }
    Some(binary)
}

/// Asserts that a run failed the way a machine with no reranker fails.
///
/// @param ran - the run
/// @param what - what was called, for the failure message
fn assert_no_reranker(ran: &Ran, what: &str) {
    assert_eq!(
        ran.code,
        1,
        "{what} must exit with 1, not 3: {}",
        ran.said()
    );
    let said = ran.said();
    assert!(said.contains("invalid_state"), "{what}: {said}");
    assert!(
        said.contains("inillucent setup-embeddings reranker"),
        "{what}: {said}"
    );
    assert!(
        said.contains("gte-reranker-modernbert-base"),
        "{what}: {said}"
    );
}

/// `rerank()` with two texts fails with the installer's name, and with `NULL` it still answers `NULL`.
#[test]
fn the_function_names_the_installer_when_no_reranker_is_installed() {
    let Some(binary) = build_or_skip() else {
        return;
    };
    let home = empty_home("function");
    let ran = run(
        &binary,
        &home,
        &[
            "--db",
            ":memory:",
            "query",
            "SELECT rerank('a question', 'a passage')",
        ],
    );
    assert_no_reranker(&ran, "rerank()");
    let nulls = run(
        &binary,
        &home,
        &[
            "--db",
            ":memory:",
            "query",
            "SELECT rerank(NULL, 'a passage') IS NULL",
        ],
    );
    assert_eq!(nulls.code, 0, "{}", nulls.said());
}

/// A search that names `question` fails the same way, and one that does not is unaffected.
#[test]
fn a_search_with_a_question_names_the_installer() {
    let Some(binary) = build_or_skip() else {
        return;
    };
    let home = empty_home("search");
    let database = home.join("d.rdb").to_string_lossy().into_owned();
    assert_eq!(run(&binary, &home, &["create", &database]).code, 0);
    for statement in [
        "CREATE VIRTUAL TABLE docs USING inillucent_search(body)",
        "INSERT INTO docs (rowid, body) VALUES (1, 'apples in the orchard'), (2, 'pears on the shelf')",
    ] {
        let ran = run(&binary, &home, &["--db", &database, "exec", statement]);
        assert_eq!(ran.code, 0, "{statement}: {}", ran.said());
    }
    let plain = run(
        &binary,
        &home,
        &[
            "--db",
            &database,
            "query",
            "SELECT rowid FROM docs WHERE docs MATCH 'apples' AND k = 3 ORDER BY rank",
        ],
    );
    assert_eq!(
        plain.code,
        0,
        "a search with no question is unaffected: {}",
        plain.said()
    );
    let reranked = run(
        &binary,
        &home,
        &[
            "--db",
            &database,
            "query",
            "SELECT rowid FROM docs WHERE docs MATCH 'apples' AND question = 'which fruit' AND k = 3 ORDER BY rank",
        ],
    );
    assert_no_reranker(&reranked, "a search that names question");
    let command = run(
        &binary,
        &home,
        &[
            "--db", &database, "search", "apples", "--table", "docs", "--rerank",
        ],
    );
    assert_no_reranker(&command, "inillucent search --rerank");
}

/// `setup-embeddings --status` reports the reranker as not installed, with the command that installs it and its size.
#[test]
fn status_says_how_to_install_the_reranker() {
    let Some(binary) = build_or_skip() else {
        return;
    };
    let home = empty_home("status");
    let ran = run(&binary, &home, &["setup-embeddings", "--status"]);
    assert_eq!(ran.code, 0, "{}", ran.said());
    assert!(
        ran.stdout
            .contains("gte-reranker-modernbert-base: not installed"),
        "{}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains("setup-embeddings reranker"),
        "{}",
        ran.stdout
    );
    assert!(ran.stdout.contains("600 MB"), "{}", ran.stdout);
}

//! The thread and device settings of `embed()`, and `embed_tokens(text)`.
//!
//! Invariant: a device that will not start is an error that names the command that installs the
//! runtime, and never a quiet run on the processor; `embed_tokens` counts the tokens the embedding
//! model's tokenizer produces before the model's 1,900 token limit is applied, without loading
//! the model.
//!
//! Every case drives the built `inillucent` program with the settings in its environment,
//! because the embedder is one static per process and a setting is read once, so two cases with
//! different settings cannot share a process. The suite skips when the build has no `embed`
//! function or the machine has no model.

use std::path::PathBuf;
use std::process::Command;

use inillucent_compat::cliproc::program;

/// What one run of the program produced.
struct Ran {
    ok: bool,
    stdout: String,
    said: String,
}

/// Runs `inillucent --db :memory: query <sql>` with extra environment variables set.
///
/// The three settings variables are cleared first, so a developer's shell cannot change what a
/// case checks.
///
/// @param binary - the built `inillucent`
/// @param sql - the query
/// @param variables - environment variables to set for this one process
fn ask(binary: &PathBuf, sql: &str, variables: &[(&str, &str)]) -> Ran {
    let output = Command::new(binary)
        .args(["--db", ":memory:", "query", sql, "--output", "json"])
        .env_remove("INILLUCENT_EMBED_THREADS")
        .env_remove("INILLUCENT_EMBED_DEVICE")
        .env_remove("INILLUCENT_EMBED_RESIDENCY")
        .envs(variables.iter().copied())
        .output()
        .unwrap_or_else(|error| panic!("inillucent did not start: {error}"));
    Ran {
        ok: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        said: format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ),
    }
}

/// Returns the binary when this build can embed and the machine has the model, and says why not otherwise.
fn embedder_or_skip() -> Option<PathBuf> {
    let binary = program("inillucent");
    let probe = ask(&binary, "SELECT length(embed('a word'))", &[]);
    if !probe.ok {
        inillucent_compat::differential::skipping(&format!(
            "this build cannot answer embed(TEXT): {}. Run `inillucent setup-embeddings all` and \
             build with --features inillucent-cli/embed",
            probe.said.lines().next().unwrap_or("")
        ));
        return None;
    }
    Some(binary)
}

/// Returns the integer in one column of the first row of a JSON result.
///
/// @param ran - a run that answered with `--output json`
/// @param column - the column's position, from 0
fn integer_of(ran: &Ran, column: usize) -> i64 {
    let rows = inillucent_compat::cliproc::rows(&ran.stdout);
    rows.first()
        .and_then(|row| row.get(column))
        .and_then(|cell| cell.parse().ok())
        .unwrap_or_else(|| panic!("column {column} is not an integer in {}", ran.said))
}

/// `embed_tokens` counts what the tokenizer produces, special tokens included, with no limit applied.
///
/// The model is a BERT style tokenizer, so `[CLS]` and `[SEP]` are counted and punctuation splits
/// into its own tokens: `hello world` is four tokens and `search_document: hello world` is eight.
/// A text of 2,000 words is over the 1,900 token limit and the count says so, where the embedding
/// itself would silently use the first 1,900.
#[test]
fn embed_tokens_counts_what_the_tokenizer_produces() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let ran = ask(
        &binary,
        "SELECT embed_tokens('hello world') AS a, embed_tokens('search_document: hello world') AS b, \
         embed_tokens('') AS c, embed_tokens(replace(hex(zeroblob(1000)), '0', 'word ')) AS d",
        &[],
    );
    assert!(ran.ok, "{}", ran.said);
    assert_eq!(integer_of(&ran, 0), 4, "{}", ran.said);
    assert_eq!(integer_of(&ran, 1), 8, "{}", ran.said);
    assert_eq!(integer_of(&ran, 2), 2, "{}", ran.said);
    assert!(integer_of(&ran, 3) > 1900, "{}", ran.said);
}

/// `NULL` in gives `NULL` out, and a query that only counts tokens does not load the model.
///
/// The second half is measured by asking with a device that cannot start: if the count loaded a
/// session it would fail on the card that is not there, and it succeeds.
#[test]
fn embed_tokens_of_null_is_null_and_needs_no_session() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let ran = ask(
        &binary,
        "SELECT embed_tokens(NULL) IS NULL AS n, embed_tokens('a b c') AS c",
        &[("INILLUCENT_EMBED_DEVICE", "cuda:99")],
    );
    assert!(ran.ok, "{}", ran.said);
    assert_eq!(integer_of(&ran, 0), 1, "{}", ran.said);
    assert_eq!(integer_of(&ran, 1), 5, "{}", ran.said);
}

/// A card that is not there fails the call, names the command that installs the CUDA runtime, and does not run on the processor.
///
/// Card 99 does not exist on any machine, so this checks the error path on a machine that has a
/// working CUDA install as well as on one that has none.
#[test]
fn a_cuda_device_that_will_not_start_is_an_error_and_not_a_quiet_fallback() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let ran = ask(
        &binary,
        "SELECT length(embed('a word'))",
        &[("INILLUCENT_EMBED_DEVICE", "cuda:99")],
    );
    assert!(
        !ran.ok,
        "embed on a card that is not there must fail, and it answered: {}",
        ran.said
    );
    assert!(
        ran.said.contains("setup-embeddings runtime --gpu"),
        "the message must name the installer command: {}",
        ran.said
    );
    assert!(
        ran.said.contains("invalid_state"),
        "the status is invalid_state: {}",
        ran.said
    );
}

/// A thread count is accepted and the vector has the same width, on the processor.
#[test]
fn a_thread_count_is_applied_and_the_vector_keeps_its_width() {
    let Some(binary) = embedder_or_skip() else {
        return;
    };
    let ran = ask(
        &binary,
        "SELECT length(embed('a word')) AS bytes",
        &[("INILLUCENT_EMBED_THREADS", "2")],
    );
    assert!(ran.ok, "{}", ran.said);
    assert_eq!(integer_of(&ran, 0), 3072, "{}", ran.said);
}

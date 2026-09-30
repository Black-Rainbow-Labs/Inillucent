//! `rerank(query, passage)`, a search that names `question`, and `inillucent search --rerank`.
//!
//! Invariant: a score is a number from 0 to 1 and a relevant passage scores above an unrelated one;
//! `NULL` in gives `NULL` out; a search that names `question` returns `k` rows in the order of the
//! reranker's score with a `score` from 0 to 1, and one that does not name it is unchanged; and the
//! reranker moves a row that the fused search ranked low to the top when that row answers the
//! question.
//!
//! Every case drives the built `inillucent` program. The suite skips when the build has no `embed`
//! function, or the machine has no embedding model or no reranker. On this machine both are
//! installed. `rerank_absent.rs` covers the machine that has neither.

use std::path::{Path, PathBuf};
use std::process::Command;

use inillucent_compat::cliproc::{document, field, items, number_of, program, rows, text_of};

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

/// Returns the program when this build can rerank and the machine has the models, and says why not otherwise.
fn reranker_or_skip() -> Option<PathBuf> {
    let binary = program("inillucent");
    let probe = run(
        &binary,
        &[
            "--db",
            ":memory:",
            "query",
            "SELECT rerank('a question', 'an answer')",
        ],
    );
    if probe.code != 0 {
        inillucent_compat::differential::skipping(&format!(
            "this machine cannot answer rerank(): {}. Run `inillucent setup-embeddings all` and \
             `inillucent setup-embeddings reranker`, and build with --features inillucent-cli/embed",
            probe.said().lines().next().unwrap_or("")
        ));
        return None;
    }
    Some(binary)
}

/// Runs a statement against a database and panics with what the program said when it fails.
///
/// @param binary - the built `inillucent`
/// @param database - the database file, or `:memory:`
/// @param verb - `exec` or `query`
/// @param statement - the SQL
fn sql(binary: &PathBuf, database: &str, verb: &str, statement: &str) -> Ran {
    let ran = run(
        binary,
        &["--db", database, verb, statement, "--output", "json"],
    );
    assert_eq!(ran.code, 0, "{statement} failed: {}", ran.said());
    ran
}

/// Returns the cells of every row of a JSON result, as text.
///
/// @param ran - a run that answered with `--output json`
fn cells(ran: &Ran) -> Vec<Vec<String>> {
    rows(&ran.stdout)
}

/// A relevant passage scores above an unrelated one, both are from 0 to 1, and `NULL` gives `NULL`.
#[test]
fn a_relevant_passage_scores_above_an_unrelated_one() {
    let Some(binary) = reranker_or_skip() else {
        return;
    };
    let ran = sql(
        &binary,
        ":memory:",
        "query",
        "SELECT \
         rerank('Which philosopher said that everything is made of water?', \
                'Thales of Miletus taught that water is the first principle from which all things come.') AS relevant, \
         rerank('Which philosopher said that everything is made of water?', \
                'The compiler rejected the program because a type did not match the declared signature.') AS unrelated, \
         rerank(NULL, 'a passage') IS NULL AS null_query, \
         rerank('a question', NULL) IS NULL AS null_passage",
    );
    let row = cells(&ran).into_iter().next().unwrap_or_default();
    let relevant: f64 = row
        .first()
        .and_then(|cell| cell.parse().ok())
        .expect("relevant");
    let unrelated: f64 = row
        .get(1)
        .and_then(|cell| cell.parse().ok())
        .expect("unrelated");
    assert!(relevant > unrelated, "{relevant} against {unrelated}");
    assert!(
        relevant > 0.5 && relevant <= 1.0,
        "a relevant passage scored {relevant}"
    );
    assert!(
        (0.0..0.5).contains(&unrelated),
        "an unrelated passage scored {unrelated}"
    );
    assert_eq!(row.get(2).map(String::as_str), Some("1"), "{}", ran.stdout);
    assert_eq!(row.get(3).map(String::as_str), Some("1"), "{}", ran.stdout);
}

/// Builds a table of passages and a search table over them with the vectors written by `inillucent embed`.
///
/// The passage that answers the question, row 1, is written without the words of the question. Rows
/// 2 to 4 use the question's words and answer nothing. The keyword branch therefore favours the
/// decoys and the vector branch is the only one that can find row 1, which is the case a reranker
/// exists to fix.
///
/// @param binary - the built `inillucent`
/// @param name - the case's name
fn build_search(binary: &PathBuf, name: &str) -> PathBuf {
    let root = inillucent_compat::workspace_root().join("_agent_output/rerank");
    let _ = std::fs::create_dir_all(&root);
    let database = root.join(format!("{}-{name}.rdb", std::process::id()));
    inillucent_base::testing::remove_database(&database);
    let path = database.to_string_lossy().into_owned();
    assert_eq!(run(binary, &["create", &path]).code, 0);
    sql(
        binary,
        &path,
        "exec",
        "CREATE TABLE passage (id INTEGER PRIMARY KEY, title TEXT, body TEXT, v VECTOR(768))",
    );
    sql(
        binary,
        &path,
        "exec",
        "INSERT INTO passage (id, title, body) VALUES \
         (1, 'Thales', 'The Milesian thinker held that the first principle behind every substance was the sea, the ocean and rain, the wet element itself.'), \
         (2, 'Water bills', 'Water bills rise every year, and everything in the house that uses water, from the kettle to the washing machine, adds to the cost of the bill.'), \
         (3, 'Lockers', 'Every philosopher in the department was given a locker, and everything in the lockers was made of steel, so nobody could hang a coat.'), \
         (4, 'The cat', 'Everything was quiet, and the cat of the philosopher drank water from a bowl by the door, then went back to sleep on the sofa.'), \
         (5, 'Heraclitus', 'The Ephesian thinker taught that fire is the origin of the world and that all things are an exchange for fire.'), \
         (6, 'Compilers', 'A compiler turns source text into machine code and reports a type error when the types of two expressions do not match.'), \
         (7, 'Bread', 'Sourdough bread rises slowly because wild yeast and bacteria in the starter produce gas and acid over many hours.'), \
         (8, 'Football', 'The 1994 World Cup final was played in Pasadena and decided on penalties after a goalless draw.')",
    );
    let embed = run(
        binary,
        &[
            "--db",
            &path,
            "embed",
            "--table",
            "passage",
            "--text",
            "body",
            "--vector",
            "v",
            "--prefix",
            "search_document: ",
            "--device",
            "cpu",
            "--output",
            "json",
        ],
    );
    assert_eq!(embed.code, 0, "{}", embed.said());
    sql(
        binary,
        &path,
        "exec",
        "CREATE VIRTUAL TABLE passage_search USING inillucent_search(title, body, dims = 768, fusion = 'rrf', rerank_depth = 8)",
    );
    sql(
        binary,
        &path,
        "exec",
        "INSERT INTO passage_search (rowid, title, body, vector) SELECT id, title, body, v FROM passage",
    );
    database
}

/// The question every search case asks, in plain words.
const QUESTION: &str = "Which philosopher said that everything is made of water?";

/// Returns the rowid and score of each row a search returned, in the order it returned them.
///
/// @param binary - the built `inillucent`
/// @param database - the database file
/// @param extra - the conditions to add after the `MATCH` and the vector, such as `AND question = ?`
fn search(binary: &PathBuf, database: &Path, extra: &str) -> Vec<(i64, f64)> {
    let path = database.to_string_lossy().into_owned();
    let statement = format!(
        "SELECT rowid, score(passage_search) AS relevance FROM passage_search \
         WHERE passage_search MATCH 'philosopher OR water OR everything' \
         AND vector = embed('search_query: {QUESTION}') {extra} ORDER BY rank"
    );
    let ran = sql(binary, &path, "query", &statement);
    cells(&ran)
        .iter()
        .map(|row| {
            (
                row.first().and_then(|cell| cell.parse().ok()).unwrap_or(-1),
                row.get(1)
                    .and_then(|cell| cell.parse().ok())
                    .unwrap_or(f64::NAN),
            )
        })
        .collect()
}

/// A search that names `question` returns `k` rows ordered by the reranker's score, with scores from 0 to 1.
#[test]
fn a_search_with_a_question_is_ordered_by_the_reranker() {
    let Some(binary) = reranker_or_skip() else {
        return;
    };
    let database = build_search(&binary, "order");
    let reranked = search(
        &binary,
        &database,
        &format!("AND question = '{QUESTION}' AND k = 4"),
    );
    assert_eq!(reranked.len(), 4, "{reranked:?}");
    for pair in reranked.windows(2) {
        assert!(
            pair[0].1 >= pair[1].1,
            "the scores must not rise down the list: {reranked:?}"
        );
    }
    assert!(
        reranked
            .iter()
            .all(|(_, score)| (0.0..=1.0).contains(score)),
        "a reranker score is from 0 to 1: {reranked:?}"
    );
    assert_eq!(
        reranked.first().map(|(rowid, _)| *rowid),
        Some(1),
        "the passage that answers the question, with none of its words, must come first: {reranked:?}"
    );
}

/// The same search without `question` is not reranked: its scores are reciprocal rank scores, at most 2/61.
#[test]
fn a_search_without_a_question_is_not_reranked() {
    let Some(binary) = reranker_or_skip() else {
        return;
    };
    let database = build_search(&binary, "plain");
    let plain = search(&binary, &database, "AND k = 4");
    assert_eq!(plain.len(), 4, "{plain:?}");
    assert!(
        plain.iter().all(|(_, score)| *score <= 2.0 / 61.0 + 1e-6),
        "a reciprocal rank fusion score is at most 2/61: {plain:?}"
    );
}

/// `inillucent search --rerank` passes the query as the question and returns rows.
#[test]
fn the_search_command_can_rerank() {
    let Some(binary) = reranker_or_skip() else {
        return;
    };
    let database = build_search(&binary, "command");
    let path = database.to_string_lossy().into_owned();
    let ran = run(
        &binary,
        &[
            "--db",
            &path,
            "search",
            "philosopher OR water OR everything",
            "--table",
            "passage_search",
            "--rerank",
            "--k",
            "3",
            "--output",
            "json",
        ],
    );
    assert_eq!(ran.code, 0, "{}", ran.said());
    let node = document(&ran.stdout);
    let count = field(&node, "row_count").and_then(number_of);
    assert_eq!(count, Some(3.0), "{}", ran.stdout);
    assert!(field(&node, "rows").and_then(items).is_some());
    assert_eq!(
        field(&node, "command").and_then(text_of).as_deref(),
        Some("search")
    );
}

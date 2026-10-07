//! Cases gathered from other engines' bug reports, run through `sqlite3` and
//! `inillucent-shell` and compared byte for byte.
//!
//! Invariant: every case in `compat/corpus/research/*.sql` prints exactly what
//! the pinned SQLite 3.53.4 shell prints, or it is listed in
//! `compat/corpus/research/known.toml` with the reason it does not. A case that
//! differs and is not listed fails the suite, and so does a listed case that
//! now matches.
//!
//! The corpus was gathered by the bug hunt in
//! `tasks/task-2201-bug-hunt-tdd.md`: about 1,200 cases from Turso's issue
//! tracker, SQLite's release notes and forum, the SQLancer bug lists and hand
//! written probes. Each one is a sequence that broke some engine that speaks
//! SQLite's dialect. The cases that found a defect here are also in the usage
//! corpus, which runs on every change; this suite runs the whole set every
//! night, because it is several minutes of shell processes.
//!
//! Setting `INILLUCENT_RESEARCH_REPORT` to a path writes every difference as
//! JSON to that path.

#[path = "../support/shell_corpus.rs"]
mod shell_corpus;

use inillucent_compat::workspace_root;
use shell_corpus::{assert_well_formed, corpus_problems};

/// Returns the corpus directory.
fn corpus_directory() -> std::path::PathBuf {
    workspace_root().join("compat/corpus/research")
}

/// Every research case matches the pinned shell, or is listed in
/// `known.toml`, and no listed case matches.
#[test]
fn every_research_case_matches_sqlite_or_is_listed() {
    let Some(problems) = corpus_problems(
        &corpus_directory(),
        "research corpus",
        "INILLUCENT_RESEARCH_REPORT",
    ) else {
        inillucent_compat::differential::skipping("the pinned SQLite shell is not built");
        return;
    };
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// The corpus is well formed and has not lost its cases.
#[test]
fn the_research_corpus_is_well_formed() {
    let cases = assert_well_formed(&corpus_directory());
    assert!(
        cases.len() >= 1_000,
        "the research corpus shrank to {} cases",
        cases.len()
    );
}

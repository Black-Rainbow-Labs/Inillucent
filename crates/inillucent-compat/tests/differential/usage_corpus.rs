//! Application usage scripts, run through `sqlite3` and `inillucent-shell`
//! and compared byte for byte.
//!
//! Invariant: every case in `compat/corpus/usage/*.sql` prints exactly what the
//! pinned SQLite 3.53.4 shell prints, or it is listed in
//! `compat/corpus/usage/known.toml` with the reason it does not. A case that
//! differs and is not listed fails the suite, and so does a listed case that
//! now matches, so the list can only shrink by fixing something and can only
//! grow by somebody writing down why.
//!
//! The cases are the statements applications send: the PRAGMAs a driver runs
//! when it connects, the catalog queries an ORM runs to read a schema, the
//! table rebuild a migration tool runs, the upserts and RETURNING clauses of
//! ordinary CRUD code, and the error messages those tools parse. The engine
//! suites check one statement at a time against the oracle; these check the
//! sequences, because the defects they were written to find appeared only
//! after a rename, a rebuild or a catalog join.
//!
//! The comparison goes through the shell, so every value is printed with
//! `.mode quote` and `.headers on`: a storage class, a column name and an error
//! message are all part of the bytes compared. A script cannot see the
//! extended result code, which the engine suites compare instead.
//!
//! Setting `INILLUCENT_USAGE_REPORT` to a path writes every difference as JSON
//! to that path, which is how a fix is measured against the whole corpus
//! rather than against the one case that prompted it.

#[path = "../support/shell_corpus.rs"]
mod shell_corpus;

use inillucent_compat::workspace_root;
use shell_corpus::{assert_well_formed, corpus_problems, parse_cases, without_carets};

/// Returns the corpus directory.
fn corpus_directory() -> std::path::PathBuf {
    workspace_root().join("compat/corpus/usage")
}

/// Every case matches the pinned shell, or is listed in `known.toml`, and no
/// listed case matches.
#[test]
fn every_usage_case_matches_sqlite_or_is_listed() {
    let Some(problems) = corpus_problems(
        &corpus_directory(),
        "usage corpus",
        "INILLUCENT_USAGE_REPORT",
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

/// The corpus is well formed: names are unique, every case has SQL, and
/// every row of `known.toml` names a case that exists, with a kind that is
/// one of the two.
#[test]
fn the_corpus_and_its_known_list_are_well_formed() {
    let cases = assert_well_formed(&corpus_directory());
    assert!(
        cases.len() >= 500,
        "the corpus shrank to {} cases",
        cases.len()
    );
}

/// A case marker splits a file, text before the first marker belongs to no
/// case, and a case keeps its lines in order.
#[test]
fn a_corpus_file_splits_into_its_cases() {
    let cases = parse_cases(
        "f.sql",
        "-- a comment\n-- case: x/one\nSELECT 1;\nSELECT 2;\n-- case: x/two\nSELECT 3;\n",
    );
    assert_eq!(cases.len(), 2);
    assert_eq!(cases[0].name, "x/one");
    assert_eq!(cases[0].script, "SELECT 1;\nSELECT 2;\n");
    assert_eq!(cases[1].script, "SELECT 3;\n");
}

/// Both pointer styles are removed from under an error line, and nothing
/// else is: a row that follows an error with no pointer stays.
#[test]
fn only_the_pointer_lines_are_set_aside() {
    let text = "Parse error near line 3: x\n  SELECT 1;\n  ^--- error here\nError near line 4: y\n  long statement\n   error here ---^\nError near line 5: z\n'row'\n";
    assert_eq!(
        without_carets(text),
        "Parse error near line 3: x\nError near line 4: y\nError near line 5: z\n'row'\n"
    );
}

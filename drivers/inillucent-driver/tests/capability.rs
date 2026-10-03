//! The capability table, checked against the engine in both directions.
//!
//! This is the driver's most important test and the reason the table is worth
//! having at all. `java.sql.DatabaseMetaData` has had `supportsFullOuterJoins()`
//! since 1997 and its answers are famously unreliable, because every driver
//! hand-writes them and nothing runs them. A capability list nobody checks is a
//! list of claims that were true once.
//!
//! So every row is run:
//!
//! - declared **supported** and the probe fails → the driver was lying about
//!   something it offered.
//! - declared **unsupported** and the probe *succeeds* → the engine has grown
//!   it, and the table is stale. This half is what stops the list decaying while
//!   Phase 6 closes the gaps it names.
//!
//! A row with no probe is exempt and has to say why in its note, which
//! `capability.rs`'s own unit test enforces.
//!
//! Invariant: **every capability this driver claims is checked in both
//! directions.** A claimed capability that fails and a denied one that now
//! works each turn the build red, which is what makes the table worth trusting
//! in a way a hand-written feature list is not.

use std::path::PathBuf;

use inillucent_driver::capability::{Probe, Support, CAPABILITIES};
use inillucent_driver::{Database, Status, Value};

/// Returns a scratch database nothing else is using.
///
/// One file per probe, because a probe leaves a schema behind and the next
/// probe's `CREATE TABLE` would meet it. A shared file would also make the
/// order of the table significant, which is exactly the kind of coupling that
/// makes a failing row hard to read.
///
/// @param name - what to call the file
fn scratch(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "inillucent-driver-cap-{name}-{}-{:?}.rdb",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0)
    ));
    path
}

/// What running one probe produced.
enum Outcome {
    /// It ran.
    Ran,
    /// It was refused, with the driver's word for why.
    Refused(String),
    /// It answered, and this is its first cell rendered.
    Answered(String),
}

/// Runs one capability's setup and probe against a fresh database.
///
/// @param name - the capability's name, used for the file
/// @param setup - the fixture statements
/// @param sql - the statement under test
/// @param want_value - whether the probe reads a value rather than an outcome
fn probe(name: &str, setup: &[&str], sql: &str, want_value: bool) -> Outcome {
    probe_with(name, setup, sql, want_value, false)
}

/// Runs one capability's probe, optionally registering a function and a
/// collation on the connection first.
///
/// The registration is a separate flag rather than a separate function because
/// everything after it is identical, and two copies of "open a scratch
/// database, run the setup, run the statement, read the first cell" would be
/// two things that could disagree about what a probe is.
///
/// @param name - the capability's name, used for the file
/// @param setup - the fixture statements
/// @param sql - the statement under test
/// @param want_value - whether the probe reads a value rather than an outcome
/// @param register - whether to register the probe function and collation first
fn probe_with(name: &str, setup: &[&str], sql: &str, want_value: bool, register: bool) -> Outcome {
    let path = scratch(name);
    let database = Database::open(&path).expect("the scratch database opens");
    let connection = database.session();
    if register {
        // Doubling and a case-insensitive comparison: two things whose right
        // answer is obvious, so a probe that returned the wrong one could not
        // be mistaken for a probe that happened to pass.
        let registered = connection
            .create_scalar_function("driver_probe", 1, |arguments| match arguments.first() {
                Some(Value::Integer(number)) => Ok(Value::Integer(number.saturating_mul(2))),
                _ => Ok(Value::Null),
            })
            .and_then(|()| {
                connection.create_collation("driver_probe_ci", |left, right| {
                    left.to_ascii_lowercase().cmp(&right.to_ascii_lowercase())
                })
            });
        if let Err(why) = registered {
            let _ = std::fs::remove_file(&path);
            return Outcome::Refused(format!("{}: {}", why.status.name(), why.message));
        }
    }
    for statement in setup {
        // A setup statement that is refused is a fact about the engine too, and
        // silently carrying on would make the probe measure something else. The
        // one exception is `PRAGMA foreign_keys`, which the engine answers and
        // does not honour - that is the very thing the row is testing.
        if let Err(why) = connection.query(statement, &[], usize::MAX) {
            if !statement.starts_with("PRAGMA") {
                panic!("{name}: the fixture statement `{statement}` was refused: {why}");
            }
        }
    }
    let outcome = match connection.query(sql, &[], usize::MAX) {
        Err(why) => Outcome::Refused(format!("{}: {}", why.status.name(), why.message)),
        Ok(rows) if want_value => Outcome::Answered(match rows.value(0, 0) {
            Some(Value::Text(text)) => text.clone(),
            Some(Value::Integer(number)) => number.to_string(),
            Some(Value::Real(number)) => number.to_string(),
            Some(Value::Blob(_)) => "<bytes>".to_owned(),
            Some(Value::Null) | None => "<null>".to_owned(),
        }),
        Ok(_) => Outcome::Ran,
    };
    let _ = connection;
    drop(database);
    let _ = std::fs::remove_file(&path);
    outcome
}

/// Runs one query on a fresh database opened with a raw key, and reads its
/// first cell.
///
/// Opening is part of what is probed: an engine that cannot open a keyed
/// database answers `Refused`, which a `yes` row counts as wrong.
///
/// @param name - the capability's name, used for the file
/// @param sql - the query under test
fn probe_keyed(name: &str, sql: &str) -> Outcome {
    let path = scratch(name);
    let options = inillucent_driver::OpenOptions {
        key: Some(inillucent_driver::EncryptionKey::raw([0x33; 32])),
        ..inillucent_driver::OpenOptions::default()
    };
    let outcome = match Database::open_with(&path, options) {
        Err(why) => Outcome::Refused(format!("{}: {}", why.status.name(), why.message)),
        Ok(database) => {
            let answered = database.session().query(sql, &[], usize::MAX);
            let outcome = match answered {
                Err(why) => Outcome::Refused(format!("{}: {}", why.status.name(), why.message)),
                Ok(rows) => Outcome::Answered(match rows.value(0, 0) {
                    Some(Value::Text(text)) => text.clone(),
                    other => format!("{other:?}"),
                }),
            };
            drop(database);
            outcome
        }
    };
    let _ = std::fs::remove_file(&path);
    outcome
}

/// Every declared capability is what the engine actually does, in both
/// directions.
#[test]
fn the_capability_table_matches_the_engine() {
    let mut wrong: Vec<String> = Vec::new();
    for entry in CAPABILITIES {
        let supported = entry.support == Support::Yes;
        // `rerank` is a function of the `embed` feature. The runner builds this suite with the
        // feature, from the `features` row in `tests/selection.toml`. A plain `cargo test` does
        // not, and the row is then not evidence of anything, so it says so and moves on.
        if entry.name == "rerank" && !cfg!(feature = "embed") {
            inillucent_base::testing::skipping(
                "this build has no embedding support compiled in, so `rerank` cannot be probed",
            );
            continue;
        }
        match entry.probe {
            Probe::Nothing => continue,
            Probe::Runs { setup, sql } => match (supported, probe(entry.name, setup, sql, false)) {
                (true, Outcome::Ran) | (false, Outcome::Refused(_)) => {}
                (true, Outcome::Refused(why)) => wrong.push(format!(
                    "`{}` is declared supported and was refused - {why}",
                    entry.name
                )),
                (false, Outcome::Ran) => wrong.push(format!(
                    "`{}` is declared unsupported and it WORKED. The engine has grown it; \
                     update capability.rs rather than this test.",
                    entry.name
                )),
                (_, Outcome::Answered(_)) => unreachable!("Runs never reads a value"),
            },
            Probe::Refuses { setup, sql } => {
                match (supported, probe(entry.name, setup, sql, false)) {
                    (true, Outcome::Refused(_)) | (false, Outcome::Ran) => {}
                    (true, Outcome::Ran) => wrong.push(format!(
                        "`{}` is declared supported, and the statement that should have been \
                         refused succeeded",
                        entry.name
                    )),
                    (false, Outcome::Refused(why)) => wrong.push(format!(
                        "`{}` is declared unsupported and the engine REFUSED the violating \
                         statement - {why}. The engine has grown it; update capability.rs.",
                        entry.name
                    )),
                    (_, Outcome::Answered(_)) => unreachable!("Refuses never reads a value"),
                }
            }
            Probe::Registers { sql, expect } => {
                match (supported, probe_with(entry.name, &[], sql, true, true)) {
                    (true, Outcome::Answered(got)) if got == expect => {}
                    (false, Outcome::Answered(got)) if got != expect => {}
                    (false, Outcome::Refused(_)) => {}
                    (true, other) => wrong.push(format!(
                        "`{}` is declared supported and answered `{}` where `{expect}` was                          required - a registration nothing can reach is the failure this                          row exists to catch",
                        entry.name,
                        described(&other)
                    )),
                    (false, other) => wrong.push(format!(
                        "`{}` is declared unsupported and answered `{expect}` anyway ({}).                          The engine has grown it; update capability.rs.",
                        entry.name,
                        described(&other)
                    )),
                }
            }
            Probe::Keyed { sql, expect } => match (supported, probe_keyed(entry.name, sql)) {
                (true, Outcome::Answered(got)) if got == expect => {}
                (false, Outcome::Answered(got)) if got != expect => {}
                (false, Outcome::Refused(_)) => {}
                (true, other) => wrong.push(format!(
                    "`{}` is declared supported and a keyed database answered `{}` where                      `{expect}` was required",
                    entry.name,
                    described(&other)
                )),
                (false, other) => wrong.push(format!(
                    "`{}` is declared unsupported and a keyed database answered `{expect}`                      anyway ({}). The engine has grown it; update capability.rs.",
                    entry.name,
                    described(&other)
                )),
            },
            Probe::Answers { setup, sql, expect } => {
                match (supported, probe(entry.name, setup, sql, true)) {
                    (true, Outcome::Answered(got)) if got == expect => {}
                    (false, Outcome::Answered(got)) if got != expect => {}
                    (true, other) => wrong.push(format!(
                        "`{}` is declared supported and answered `{}` where `{expect}` was \
                         required",
                        entry.name,
                        described(&other)
                    )),
                    (false, other) => wrong.push(format!(
                        "`{}` is declared unsupported and answered `{expect}` anyway ({}). The \
                         engine has grown it; update capability.rs.",
                        entry.name,
                        described(&other)
                    )),
                }
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "the capability table does not describe this engine:\n  - {}",
        wrong.join("\n  - ")
    );
}

/// Renders an outcome for a failure message.
///
/// @param outcome - what the probe produced
fn described(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Ran => "it ran and returned no value".to_owned(),
        Outcome::Refused(why) => format!("refused: {why}"),
        Outcome::Answered(value) => value.clone(),
    }
}

/// A construct the engine has not implemented is refused as
/// [`Status::Unsupported`] and names itself, rather than arriving as the syntax
/// error a typo gets.
///
/// This is the distinction the whole driver is built around, and it is asserted
/// here against the real engine rather than against a constructed error.
#[test]
fn an_unimplemented_construct_refuses_by_name_and_a_typo_does_not() {
    let path = scratch("classify");
    let database = Database::open(&path).expect("opens");
    let connection = database.session();
    connection
        .query("CREATE TABLE people (a INTEGER PRIMARY KEY)", &[], 0)
        .expect("the fixture is made");
    connection
        .query("CREATE TABLE teams (a INTEGER PRIMARY KEY)", &[], 0)
        .expect("the fixture is made");

    // **The example moves as the engine grows, and that is the point.** It was
    // a `LEFT JOIN`, then `ATTACH`, then `VACUUM`, then a second `ON CONFLICT`
    // clause, then `ATTACH ... KEY`, each retired as the engine implemented it
    // in turn; the assertion is about the *classification*, so it is repointed
    // at a construct that is still unimplemented rather than weakened. A row
    // value on the left of `IN (SELECT ...)` was one, and is built now. An
    // `ATTACH` whose file name is a parameter is one: the capability table's
    // `attach_computed_path` row says so and checks it.
    let refused = connection
        .query("ATTACH ? AS other", &[], 10)
        .expect_err("a computed ATTACH file name is refused");
    assert_eq!(
        refused.status,
        Status::Unsupported,
        "a computed ATTACH file name is a capability gap, not a syntax error: {refused}"
    );
    let named = refused.feature.expect("the refusal names the construct");
    assert!(!named.is_empty(), "the refusal named nothing");

    let typo = connection
        .query("SELECT a FROM peple", &[], 10)
        .expect_err("a missing table is refused");
    assert_eq!(
        typo.status,
        Status::NotFound,
        "a mistyped table name is not a capability gap: {typo}"
    );
    assert_eq!(typo.feature, None, "and it names no construct");

    let malformed = connection
        .query("SELECT a FORM people", &[], 10)
        .expect_err("a malformed statement is refused");
    assert_eq!(malformed.status, Status::Syntax, "{malformed}");
    assert_eq!(malformed.feature, None);

    let _ = connection;
    drop(database);
    let _ = std::fs::remove_file(&path);
}

/// The number of rows the two-direction check skips is the number AGENTS.md
/// states.
///
/// **AGENTS.md said "every row" and two rows had no probe (task-1969, 5.9).**
/// `Probe::Nothing` rows are skipped by
/// [`the_capability_table_matches_the_engine`] in both directions, so they are
/// exactly the claims the page's own argument does not cover - and the page was
/// the thing telling a reader to trust the table over a hand-written list.
///
/// Either the number is zero or the page names it. It is two, the page names
/// them, and this is what stops a third being added quietly: a new unprobed row
/// fails here until somebody writes down what it is and why it cannot be run.
#[test]
fn the_unprobed_rows_are_the_ones_the_documents_name() {
    let unprobed: Vec<&str> = CAPABILITIES
        .iter()
        .filter(|entry| matches!(entry.probe, Probe::Nothing))
        .map(|entry| entry.name)
        .collect();
    assert_eq!(
        unprobed,
        vec!["cancel", "readonly_open"],
        "the rows with no probe have changed, and AGENTS.md names the old two"
    );

    let mut root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    root.pop();
    root.pop();
    let page = std::fs::read_to_string(root.join("AGENTS.md")).expect("AGENTS.md is readable");
    // Whitespace-flattened, because the sentence is wrapped in the page and a
    // reflow would otherwise fail this for a reason that is not about the
    // claim.
    let flattened: String = page.split_whitespace().collect::<Vec<&str>>().join(" ");
    assert!(
        flattened.contains("every row but two is checked against the running engine"),
        "AGENTS.md no longer states how many capability rows the two-direction check skips"
    );
    for name in &unprobed {
        assert!(
            flattened.contains(&format!("`{name}`")),
            "AGENTS.md does not name `{name}`, which is one of the rows the check skips"
        );
    }
}

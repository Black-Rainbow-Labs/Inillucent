//! The GitHub workflows have to be files GitHub will run, and have to answer
//! every prerequisite the suite can ask for.
//!
//! Invariant: **every workflow under `.github/workflows/` is free of the YAML
//! mistake that stopped CI for a week, and every name a `requires` row in
//! `tests/selection.toml` uses is either provided by the workflow or declared
//! absent in it.**
//!
//! ## Why each check exists
//!
//! - **A plain YAML value holding `: `.** From 2026-09-24 the contracts step in
//!   `tests.yml` read `run: cargo test ... -- policy:: selection::`. In a plain
//!   scalar `: ` starts a mapping, so the file was not YAML, GitHub refused it,
//!   and every push run on both repositories failed in under a second with no
//!   job started. Nothing graded a week of commits, and nothing local could see
//!   it, because nothing local reads the workflow. `no_plain_value_holds_a_colon`
//!   reads every line the way the YAML scanner does for this one rule.
//! - **A prerequisite CI was never told about.** The suites that drive the
//!   program built with `embed` gained `requires = ["embed"]`, `["embed",
//!   "reranker"]` and `["embed", "cuda"]`, and no workflow declared those names
//!   absent. Under `--strict` such a suite fails the run as one that evidenced
//!   nothing, on every runner, for a reason that has nothing to do with the
//!   change being tested. `every_prerequisite_has_an_answer_in_ci` makes the
//!   person who adds a name decide what CI does about it.
//!
//! The workspace has no YAML parser and may not take one for this
//! (`docs/dependency-policy.md`), so the checks read lines. That is enough for
//! both, and each was checked by putting the mistake back and watching it fail.

use std::collections::BTreeSet;

use inillucent_compat::selection::Map;
use inillucent_compat::workspace_root;

/// The prerequisites a hosted runner has, and what gives it each one.
///
/// A name here is one no workflow declares absent. The first fully green run
/// of `tests.yml` with these, 36094918694 on 2026-09-25, is the evidence that
/// each is really there.
const PROVIDED: [(&str, &str); 21] = [
    ("oracle", "the pinned SQLite step builds `.sqlite-ref/`"),
    ("shell", "the pinned SQLite step builds the reference shell"),
    (
        "fixtures",
        "the gate fixtures step builds `small.db`, `medium.db` and `large.db`",
    ),
    (
        "sqlite-bench",
        "the pinned SQLite step builds the benchmark driver",
    ),
    ("testrun", "the runner is built before the suite"),
    ("tracked-fixtures", "the fixtures are checked in"),
    ("btree-corpus", "`compat/corpus/btree/` is checked in"),
    ("baseline", "the performance baseline is checked in"),
    (
        "narrow-slots",
        "the narrow integer slots are on in the shipped build",
    ),
    (
        "network",
        "a hosted runner has a network and the workflow sets INILLUCENT_NETWORK_TESTS",
    ),
    (
        "local-timezone",
        "a hosted runner has a configured time zone",
    ),
    (
        "directory-link",
        "both runner images can make a directory link",
    ),
    ("cc", "both runner images have a C compiler"),
    ("asan", "both runner images can build with AddressSanitizer"),
    ("python", "both runner images have Python"),
    ("openssl", "both runner images have openssl"),
    ("node", "both runner images have Node"),
    ("go", "both runner images have Go"),
    ("php", "both runner images have PHP"),
    (
        "postgres",
        "the Linux job starts PostgreSQL 17 in a container",
    ),
    ("mysql", "the Linux job starts MySQL 8.4 in a container"),
];

/// Reads one workflow file.
///
/// @param name - the file name under `.github/workflows/`
fn workflow(name: &str) -> String {
    let path = workspace_root().join(".github/workflows").join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Every name a `requires` row in the selection map uses.
fn required_names() -> BTreeSet<String> {
    let map = Map::load(&workspace_root().join("tests/selection.toml")).expect("the map parses");
    map.rows
        .iter()
        .flat_map(|row| row.requires.iter().cloned())
        .collect()
}

/// The names one line passes with `--absent`.
///
/// @param line - a line of a workflow
fn absent_on(line: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut words = line.split_whitespace();
    while let Some(word) = words.next() {
        if word == "--absent" {
            if let Some(name) = words.next() {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// Returns the problem with a line of YAML outside a block scalar, if any.
///
/// Only a value written as a plain scalar is checked. A quoted value, a block
/// scalar, a flow collection, an anchor, an alias and a tag are all read by
/// other rules, and a comment ends a plain scalar at ` #`.
///
/// @param line - the line, without its line break
fn plain_value_problem(line: &str) -> Option<String> {
    let body = line.trim_start().trim_start_matches("- ");
    let (key, value) = body.split_once(": ")?;
    if key.is_empty() || key.contains(' ') || key.starts_with('#') {
        return None;
    }
    let value = value.trim();
    if value.is_empty() || value.starts_with(['\'', '"', '|', '>', '[', '{', '&', '*', '!', '#']) {
        return None;
    }
    let value = value.split(" #").next().unwrap_or(value).trim_end();
    if value.contains(": ") || value.ends_with(':') {
        return Some(format!(
            "`{key}` has a plain value holding `: `, which YAML reads as a mapping: quote it"
        ));
    }
    None
}

/// The lines of a workflow that are YAML structure, with their numbers.
///
/// A line inside a block scalar (`run: |` and the lines indented under it) is
/// the scalar's text, and `key: value` there is shell, not YAML.
///
/// @param text - the workflow
fn structural_lines(text: &str) -> Vec<(usize, &str)> {
    let mut kept = Vec::new();
    let mut block_indent: Option<usize> = None;
    for (index, line) in text.lines().enumerate() {
        let indent = line.len() - line.trim_start().len();
        if let Some(owner) = block_indent {
            if line.trim().is_empty() || indent > owner {
                continue;
            }
            block_indent = None;
        }
        if line.trim_start().starts_with('#') {
            continue;
        }
        let value = line.split_once(": ").map(|(_, value)| value.trim());
        let opens_block = value.is_some_and(|value| value.starts_with(['|', '>']))
            || line.trim_end().ends_with(": |")
            || line.trim_end().ends_with(": >");
        kept.push((index + 1, line));
        if opens_block {
            block_indent = Some(indent);
        }
    }
    kept
}

/// No workflow holds a plain YAML value with `: ` in it.
#[test]
fn no_plain_value_holds_a_colon() {
    let mut problems = Vec::new();
    let mut read = 0usize;
    for name in ["tests.yml", "nightly.yml"] {
        let text = workflow(name);
        for (number, line) in structural_lines(&text) {
            read += 1;
            if let Some(problem) = plain_value_problem(line) {
                problems.push(format!("{name}:{number}: {problem}\n    {}", line.trim()));
            }
        }
    }
    assert!(read > 50, "only {read} lines were read as YAML structure");
    assert!(
        problems.is_empty(),
        "GitHub refuses a workflow that is not YAML, and then runs no job at all:\n{}",
        problems.join("\n")
    );
}

/// The check above finds the line that stopped CI, and passes the forms that
/// are fine.
#[test]
fn the_colon_check_finds_the_line_that_stopped_ci() {
    let broken =
        "        run: cargo test -p inillucent-compat --test tooling -- policy:: selection::";
    assert!(
        plain_value_problem(broken).is_some(),
        "the line from 41359eb4 is refused"
    );
    for fine in [
        "        run: 'cargo test --test tooling -- policy:: selection::'",
        "        run: cargo build -p inillucent-compat --bin inillucent-testrun",
        "    name: ${{ matrix.os }}",
        "  group: tests-${{ github.ref }}",
        "    - cron: \"0 3 * * *\"",
        "        run: |",
        "      timeout-minutes: 150 # the suite: about 70 minutes",
    ] {
        assert_eq!(plain_value_problem(fine), None, "`{fine}` is valid YAML");
    }
    let text = "      - name: x\n        run: |\n          echo \"a: b\"\n      - name: y\n";
    let lines: Vec<&str> = structural_lines(text)
        .into_iter()
        .map(|(_, line)| line)
        .collect();
    assert_eq!(
        lines,
        vec!["      - name: x", "        run: |", "      - name: y"],
        "the text of a block scalar is not YAML structure"
    );
}

/// Every prerequisite a row asks for is provided in CI or declared absent
/// there, on every runner each workflow uses.
#[test]
fn every_prerequisite_has_an_answer_in_ci() {
    let required = required_names();
    assert!(
        required.len() > 10,
        "only {} names were read",
        required.len()
    );
    let provided: BTreeSet<String> = PROVIDED.iter().map(|(name, _)| name.to_string()).collect();

    // Every matrix entry of tests.yml is a job of its own with its own
    // `absent:` line, named by the `os:` line above it. Each one is checked,
    // because the shares of one system are separate jobs and a name left off
    // one of them fails that job alone.
    let mut runners: Vec<(String, BTreeSet<String>)> = Vec::new();
    let mut system = String::new();
    for (number, line) in workflow("tests.yml").lines().enumerate() {
        if let Some((_, os)) = line.split_once("- os: ") {
            system = os.trim().to_string();
        } else if line.trim_start().starts_with("absent:") {
            runners.push((
                format!("tests.yml:{} on {system}", number + 1),
                absent_on(line),
            ));
        }
    }
    for system in ["windows-latest", "ubuntu-latest"] {
        assert!(
            runners.iter().any(|(runner, _)| runner.ends_with(system)),
            "no `absent:` line was found for {system} in tests.yml"
        );
    }
    let nightly: BTreeSet<String> = workflow("nightly.yml")
        .lines()
        .flat_map(absent_on)
        .collect();
    assert!(!nightly.is_empty(), "nightly.yml declares nothing absent");
    runners.push(("nightly.yml on ubuntu-latest".to_string(), nightly));

    let mut unanswered = Vec::new();
    for (runner, absent) in &runners {
        for name in &required {
            if !provided.contains(name) && !absent.contains(name) {
                unanswered.push(format!(
                    "{runner}: `{name}` is neither provided nor declared absent"
                ));
            }
        }
    }
    assert!(
        unanswered.is_empty(),
        "a strict run fails on a suite whose prerequisite CI does not answer for. Add the \
         name to `--absent` in the workflow with the reason, or to PROVIDED here with what \
         provides it:\n{}",
        unanswered.join("\n")
    );

    let stale: Vec<&String> = provided.difference(&required).collect();
    assert!(
        stale.is_empty(),
        "no row requires these any more, so take them out of PROVIDED: {stale:?}"
    );
}

/// Both workflows run only on the public mirror, and never on a push.
///
/// The development repository is private, so its Actions minutes are billed:
/// each push there cost about seven runner hours, and on 2026-10-02 the
/// account's spending stopped every job. Both workflows are disabled there, and
/// each job's `if:` names the mirror so a workflow turned back on still runs
/// nothing. A `push:` or `pull_request:` trigger would start runs on every
/// push to the mirror's release branch, which `ship.ps1` already starts by hand.
#[test]
fn the_workflows_run_only_on_the_mirror_and_never_on_a_push() {
    for name in ["tests.yml", "nightly.yml"] {
        let text = workflow(name);
        let structure: Vec<&str> = structural_lines(&text)
            .into_iter()
            .map(|(_, line)| line.trim())
            .collect();
        for trigger in ["push:", "pull_request:"] {
            assert!(
                !structure.contains(&trigger),
                "{name} runs on `{trigger}`; CI runs only at night and by hand"
            );
        }
        assert!(
            structure.contains(&"schedule:") && structure.contains(&"workflow_dispatch:"),
            "{name} has lost its schedule or its manual start"
        );
        assert!(
            structure.contains(&"if: github.repository == 'Black-Rainbow-Labs/Inillucent'"),
            "{name}'s job does not name the public mirror in its `if:`"
        );
    }
}

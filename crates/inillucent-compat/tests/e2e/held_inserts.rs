//! Single row inserts the shell holds and runs as one statement print what
//! running each as it arrived prints.
//!
//! Invariant: **a script's output, its errors and their line numbers, its exit
//! code and the rows it leaves are the same whether the shell holds its
//! inserts or not.** Inside a transaction the shell holds a run of `INSERT`
//! statements of one shape and runs them as one statement before anything that
//! could see them (task-2191; `crates/inillucent-cli/src/shell/deferred.rs`).
//! When that statement fails, every held statement runs again as written.
//!
//! Each script runs twice through the built `inillucent-shell`: as it is, and
//! with `.progress 1000000000 --quiet` given on the command line. A progress
//! handler acts per statement, so the shell holds nothing while one is set,
//! and a quiet one at that interval prints nothing. The two runs must agree to
//! the byte. The scripts are the cases a held run can get wrong: a duplicate
//! key among the held rows, the same under `.bail`, several statements on one
//! line with an error in the middle, a syntax error, a dot command between two
//! runs, a rollback, two shapes taking turns, and the same rows outside a
//! transaction, where nothing is held. They were also run against the pinned
//! `sqlite3` shell when this was written, and printed what it prints.

use inillucent_compat::cliproc::{program, run_with_input};
use inillucent_compat::workspace_root;

/// One `INSERT` line for each id in a range.
///
/// @param from - the first id
/// @param to - the last id
fn rows(from: u32, to: u32) -> Vec<String> {
    (from..=to)
        .map(|id| format!("INSERT INTO t VALUES ({id}, 'v{id}');"))
        .collect()
}

/// Builds a script from fixed lines and ranges of insert lines.
///
/// @param parts - the lines, in order
fn script(parts: Vec<Vec<String>>) -> String {
    let mut text = parts.concat().join("\n");
    text.push('\n');
    text
}

/// Turns a line of text into a one line part of a script.
///
/// @param line - the line
fn line(line: &str) -> Vec<String> {
    vec![line.to_string()]
}

/// The scripts, by name.
fn scripts() -> Vec<(&'static str, String)> {
    let schema = || line("CREATE TABLE t (id INTEGER PRIMARY KEY, a TEXT NOT NULL);");
    let report = || {
        line("SELECT count(*), max(id), changes(), total_changes(), last_insert_rowid() FROM t;")
    };
    vec![
        (
            "plain",
            script(vec![schema(), line("BEGIN;"), rows(1, 3000), line("COMMIT;"), report()]),
        ),
        (
            "duplicate",
            script(vec![
                schema(),
                line("BEGIN;"),
                rows(1, 50),
                line("INSERT INTO t VALUES (7, 'dup');"),
                rows(51, 80),
                line("COMMIT;"),
                report(),
            ]),
        ),
        (
            "duplicate under bail",
            script(vec![
                line(".bail on"),
                schema(),
                line("BEGIN;"),
                rows(1, 50),
                line("INSERT INTO t VALUES (7, 'dup');"),
                rows(51, 80),
                line("COMMIT;"),
                report(),
            ]),
        ),
        (
            "several on one line",
            script(vec![
                schema(),
                line("BEGIN;"),
                rows(1, 20),
                line("INSERT INTO t VALUES (100, 'x'); INSERT INTO t VALUES (3, 'dup'); INSERT INTO t VALUES (101, 'y');"),
                line("INSERT INTO t VALUES (102, NULL); INSERT INTO t VALUES (103, 'z');"),
                line("COMMIT;"),
                report(),
            ]),
        ),
        (
            "syntax error",
            script(vec![
                schema(),
                line("BEGIN;"),
                rows(1, 20),
                line("INSERT INTO t VALUES (200 'bad');"),
                rows(21, 30),
                line("COMMIT;"),
                report(),
            ]),
        ),
        (
            "a query between",
            script(vec![
                schema(),
                line("BEGIN;"),
                rows(1, 10),
                line("INSERT INTO t VALUES (11, NULL); INSERT INTO t VALUES (12, 'a'); SELECT count(*) FROM t;"),
                rows(13, 20),
                line("COMMIT;"),
                report(),
            ]),
        ),
        (
            "a dot command between",
            script(vec![
                schema(),
                line("BEGIN;"),
                rows(1, 10),
                line(".tables"),
                rows(11, 20),
                line("INSERT INTO t VALUES (5, 'dup');"),
                line(".print after"),
                line("COMMIT;"),
                report(),
            ]),
        ),
        (
            "outside a transaction",
            script(vec![
                schema(),
                rows(1, 30),
                line("INSERT INTO t VALUES (4, 'dup');"),
                report(),
            ]),
        ),
        (
            "rolled back",
            script(vec![
                schema(),
                line("BEGIN;"),
                rows(1, 40),
                line("ROLLBACK;"),
                report(),
                line("BEGIN;"),
                rows(1, 5),
                line("COMMIT;"),
                report(),
            ]),
        ),
        (
            "two shapes",
            script(vec![
                schema(),
                line("CREATE TABLE u (x, y);"),
                line("BEGIN;"),
                rows(1, 5),
                line("INSERT INTO u VALUES (1, 2);"),
                line("INSERT INTO u VALUES (3, 4);"),
                rows(6, 9),
                line("COMMIT;"),
                report(),
                line("SELECT * FROM u;"),
            ]),
        ),
    ]
}

/// Every script prints the same, to the byte, with its inserts held and with
/// nothing held.
#[test]
fn held_inserts_print_what_inserts_run_one_at_a_time_print() {
    let shell = program("inillucent-shell");
    let directory = workspace_root().join("_agent_output/held-inserts");
    let _ = std::fs::create_dir_all(&directory);
    for (name, text) in scripts() {
        let mut answers = Vec::new();
        for (arm, arguments) in [
            ("held", Vec::new()),
            (
                "one at a time",
                vec!["-cmd", ".progress 1000000000 --quiet"],
            ),
        ] {
            let path = directory.join(format!(
                "{}-{}.rdb",
                name.replace(' ', "-"),
                arm.replace(' ', "-")
            ));
            inillucent_base::testing::remove_database(&path);
            let path_text = path.to_string_lossy().into_owned();
            let mut all = arguments.clone();
            all.push(&path_text);
            answers.push(run_with_input(&shell, &all, &text));
        }
        let (held, single) = (&answers[0], &answers[1]);
        assert_eq!(
            (held.code, &held.stdout, &held.stderr),
            (single.code, &single.stdout, &single.stderr),
            "{name}: held printed {:?}, one at a time printed {:?}",
            held.said(),
            single.said()
        );
    }
}

/// The duplicate case reports the duplicate's own line, the rows before and
/// after it are kept, and `changes()` is the last statement's.
///
/// What the first test compares, stated once so a change that broke both arms
/// the same way still fails here.
#[test]
fn a_duplicate_among_held_inserts_is_reported_on_its_own_line() {
    let shell = program("inillucent-shell");
    let directory = workspace_root().join("_agent_output/held-inserts");
    let _ = std::fs::create_dir_all(&directory);
    let path = directory.join("stated.rdb");
    inillucent_base::testing::remove_database(&path);
    let text = scripts()
        .into_iter()
        .find(|(name, _)| *name == "duplicate")
        .map(|(_, text)| text)
        .unwrap_or_default();
    let ran = run_with_input(&shell, &[&path.to_string_lossy()], &text);
    assert_eq!(ran.code, 1, "{}", ran.said());
    assert_eq!(ran.stdout.trim(), "80|80|1|80|80", "{}", ran.said());
    assert_eq!(
        ran.stderr.trim(),
        "Error near line 53: UNIQUE constraint failed: t.id",
        "{}",
        ran.said()
    );
}

/// `.import` writes a file's rows as one statement, and a file with a row
/// that will not go in reports that row's line and keeps none of them.
///
/// The import runs its rows at once when the engine allows it (task-2191) and
/// one at a time when that fails, which is how it finds the line to report.
/// A row shorter than the first is NULL in the rest, and a longer one is the
/// error it always was.
#[test]
fn an_import_reports_the_first_line_that_will_not_go_in() {
    let shell = program("inillucent-shell");
    let directory = workspace_root().join("_agent_output/held-inserts");
    let _ = std::fs::create_dir_all(&directory);
    let file = |name: &str, lines: Vec<String>| {
        let path = directory.join(name);
        std::fs::write(&path, lines.join("\n") + "\n")
            .unwrap_or_else(|error| panic!("{name} was not written: {error}"));
        path.to_string_lossy().replace('\\', "/")
    };
    let clean = file(
        "clean.csv",
        std::iter::once("id,name,score".to_string())
            .chain((1..=3000).map(|id| format!("{id},name {id},{}", id * 2)))
            .collect(),
    );
    let duplicate = file(
        "duplicate.csv",
        (1..=60)
            .map(|row| format!("{},n{row},{row}", if row == 30 { 5 } else { row }))
            .collect(),
    );
    let short = file(
        "short.csv",
        vec!["1,a,1".into(), "2,b".into(), "3".into(), "4,d,4".into()],
    );
    for (name, text, wanted_out, wanted_err) in [
        (
            "clean",
            format!(".import --csv {clean} t\nSELECT count(*), sum(score) FROM t;\n"),
            "3000|9003000",
            String::new(),
        ),
        (
            "duplicate",
            format!(
                "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, score);\n.import --csv {duplicate} t\nSELECT count(*) FROM t;\n"
            ),
            "0",
            format!("{duplicate}:30: UNIQUE constraint failed: t.id"),
        ),
        (
            "short",
            format!(
                "CREATE TABLE t (id, name, score);\n.import --csv {short} t\nSELECT id, name IS NULL, score IS NULL FROM t;\n"
            ),
            "1|0|0\n2|0|1\n3|1|1\n4|0|0",
            String::new(),
        ),
    ] {
        let path = directory.join(format!("import-{name}.rdb"));
        inillucent_base::testing::remove_database(&path);
        let ran = run_with_input(&shell, &[&path.to_string_lossy()], &text);
        assert_eq!(ran.stdout.trim().replace("\r\n", "\n"), wanted_out, "{name}: {}", ran.said());
        assert_eq!(ran.stderr.trim(), wanted_err, "{name}: {}", ran.said());
    }
}

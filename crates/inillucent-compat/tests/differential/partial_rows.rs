//! A runtime error in the middle of a result, graded against the pinned SQLite.
//!
//! Invariant: **a statement that fails on row N hands out rows 1 to N minus 1
//! first, and then reports the failure.** SQLite evaluates one row at a time, so
//! everything before the failing row is already out when the error is raised.
//! The engine evaluates a batch of up to 2048 rows at once, and an error in the
//! batch used to discard the whole batch: the rows before the failing one were
//! never returned. A program that reads rows until an error and reports how far
//! it got, and a script that compares transcripts, both saw a different answer.
//!
//! The first group runs whole scripts through `inillucent-shell` and the pinned
//! `sqlite3` and compares standard output and standard error separately, so the
//! rows and the error line must each match. The second group reads the same
//! failure through the `inillucent query` command, in text and in JSON, and
//! through the MCP server. The third group reads it through the Rust driver.
//!
//! **A run with no pinned shell records nothing rather than passing.** The suite
//! declares `requires = ["oracle"]` in `tests/selection.toml`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use inillucent_compat::cliproc::{self, program};
use inillucent_compat::workspace_root;

/// Returns the pinned SQLite shell, if it has been downloaded.
fn pinned_shell() -> Option<PathBuf> {
    let path = workspace_root()
        .join(".sqlite-ref/3.53.4/shell")
        .join(format!("sqlite3{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// A table `t(a)` holding 1 to 5000, which is more than two batches.
const FILL: &str =
    "CREATE TABLE t(a); WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x+1 FROM c \
                    WHERE x<5000) INSERT INTO t SELECT x FROM c;";

/// A column expression that fails with "malformed JSON" on row 3000 only.
const BAD: &str = "CASE WHEN a = 3000 THEN json('x') ELSE a END";

/// Runs a script on standard input of a shell over an empty in memory database.
///
/// @param shell - the shell to run
/// @param sql - the script
fn transcript(shell: &Path, sql: &str) -> (String, String) {
    let ran = cliproc::run_with_input(shell, &[":memory:"], sql);
    (
        ran.stdout.replace("\r\n", "\n"),
        ran.stderr.replace("\r\n", "\n"),
    )
}

/// Every script whose transcript is compared, with the number of rows SQLite
/// prints before the error, or `None` when SQLite prints no error.
fn scripts() -> Vec<(&'static str, String, Option<usize>)> {
    let mut all = scripts_over_one_select();
    all.extend(scripts_over_compounds_and_windows());
    all
}

/// The scripts whose failure is in the projection or the filter of one `SELECT`.
fn scripts_over_one_select() -> Vec<(&'static str, String, Option<usize>)> {
    let t = |text: &str| format!("{FILL} {text}");
    vec![
        (
            "a projected column fails on row 3000 of 5000",
            t("SELECT a, CASE WHEN a = 3000 THEN json('x') END FROM t;"),
            Some(2999),
        ),
        (
            "a projected column fails on the third row of three",
            "CREATE TABLE t(a); INSERT INTO t VALUES (1),(2),(NULL); \
             SELECT a, coalesce(a, abs(-9223372036854775808)) FROM t;"
                .to_string(),
            Some(2),
        ),
        (
            "a filter fails on row 3000",
            t("SELECT a FROM t WHERE CASE WHEN a = 3000 THEN json('x') ELSE 1 END;"),
            Some(2999),
        ),
        (
            "two columns fail on different rows and the earlier row wins",
            t(
                "SELECT a, abs(CASE WHEN a=100 THEN -9223372036854775808 ELSE a END), \
               json(CASE WHEN a=50 THEN 'x' ELSE '1' END) FROM t;",
            ),
            Some(49),
        ),
        (
            "a LIMIT that ends before the failing row raises nothing",
            t("SELECT a, CASE WHEN a = 3000 THEN json('x') END FROM t LIMIT 2999;"),
            None,
        ),
        (
            "a LIMIT that reaches the failing row raises it",
            t("SELECT a, CASE WHEN a = 3000 THEN json('x') END FROM t LIMIT 3001;"),
            Some(2999),
        ),
        (
            "a sort holds every row until the end, so none come out",
            t(&format!("SELECT a, {BAD} FROM t ORDER BY a DESC;")),
            Some(0),
        ),
        (
            "an ordered read of the rowid streams",
            t(&format!("SELECT {BAD} FROM t ORDER BY rowid;")),
            Some(2999),
        ),
        (
            "an aggregate returns no row of its own before it fails",
            t(&format!("SELECT count(*), sum({BAD}) FROM t;")),
            Some(0),
        ),
        (
            "a derived table streams into the outer query",
            t(&format!("SELECT x FROM (SELECT a, {BAD} AS x FROM t);")),
            Some(2999),
        ),
        (
            "a view streams",
            t(&format!(
                "CREATE VIEW v AS SELECT a, {BAD} AS b FROM t; SELECT * FROM v;"
            )),
            Some(2999),
        ),
        (
            "a constant right operand of AND in a result column is evaluated before the first row",
            "CREATE TABLE k(a); INSERT INTO k VALUES (0),(1),(NULL);              SELECT a, a AND abs(-9223372036854775808) FROM k;"
                .to_string(),
            Some(0),
        ),
        (
            "a constant right operand of AND that is a JSON function",
            "CREATE TABLE k(a); INSERT INTO k VALUES (0),(1),(NULL);              SELECT a, a AND json('{') FROM k;"
                .to_string(),
            Some(0),
        ),
        (
            "a constant right operand in a WHERE clause is evaluated when a row needs it",
            "CREATE TABLE k(a); INSERT INTO k VALUES (1),(0),(NULL);              SELECT a FROM k WHERE (a = 1) OR abs(-9223372036854775808);"
                .to_string(),
            Some(1),
        ),
    ]
}

/// The scripts whose failure is in a compound query, a `DISTINCT`, a window pass or a subquery.
fn scripts_over_compounds_and_windows() -> Vec<(&'static str, String, Option<usize>)> {
    let t = |text: &str| format!("{FILL} {text}");
    vec![
        (
            "a failure in the second arm of a UNION ALL follows every row of the first",
            t(&format!("SELECT a FROM t UNION ALL SELECT {BAD} FROM t;")),
            Some(7999),
        ),
        (
            "a failure in the first arm of a UNION ALL ends it",
            t(&format!("SELECT {BAD} FROM t UNION ALL SELECT 7;")),
            Some(2999),
        ),
        (
            "a UNION ALL arm after a one row arm",
            t(&format!("SELECT 7 UNION ALL SELECT {BAD} FROM t;")),
            Some(3000),
        ),
        (
            "a UNION builds its table first, so no row comes out",
            t(&format!("SELECT 7 UNION SELECT {BAD} FROM t;")),
            Some(0),
        ),
        (
            "a UNION ALL with a LIMIT met before the failure raises nothing",
            t(&format!("SELECT a, {BAD} FROM t UNION ALL SELECT 1, 2 LIMIT 2000;")),
            None,
        ),
        (
            "a UNION ALL with a LIMIT past the failure raises it",
            t(&format!("SELECT a, {BAD} FROM t UNION ALL SELECT 1, 2 LIMIT 4000;")),
            Some(2999),
        ),
        (
            "DISTINCT hands out each new row as it is found",
            t(&format!("SELECT DISTINCT {BAD} FROM t;")),
            Some(2999),
        ),
        (
            "DISTINCT with a LIMIT stops at the limit",
            t(&format!("SELECT DISTINCT a, {BAD} FROM t LIMIT 5;")),
            None,
        ),
        (
            "DISTINCT under an ORDER BY holds its rows",
            t("SELECT DISTINCT a % 10, CASE WHEN a=4000 THEN json('x') ELSE 1 END FROM t ORDER BY 1;"),
            Some(0),
        ),
        (
            "a window computed value fails on row 4000",
            t("SELECT a, sum(CASE WHEN a=4000 THEN 9223372036854775807 ELSE a END) OVER \
               (ORDER BY a ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t;"),
            Some(3999),
        ),
        (
            "a window with a LIMIT met before the failure raises nothing",
            t("SELECT a, sum(CASE WHEN a=4000 THEN 9223372036854775807 ELSE a END) OVER \
               (ORDER BY a ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM t LIMIT 3000;"),
            None,
        ),
        (
            "a window over four rows that overflows on the third",
            "CREATE TABLE m(id,v); INSERT INTO m VALUES (1,9223372036854775807),\
             (2,9223372036854775807),(3,1),(4,2); SELECT id, sum(v) OVER (ORDER BY id ROWS \
             BETWEEN 1 PRECEDING AND CURRENT ROW) FROM m;"
                .to_string(),
            Some(1),
        ),
        (
            "a projected column fails after a window pass computed fine",
            t(&format!("SELECT a, sum(a) OVER (ORDER BY a), {BAD} FROM t;")),
            Some(2999),
        ),
        (
            "a projected column fails after a window with no ordering",
            t(&format!("SELECT a, sum(a) OVER (), {BAD} FROM t;")),
            Some(2999),
        ),
        (
            "a projected column fails after row_number",
            t(&format!("SELECT a, row_number() OVER (ORDER BY a), {BAD} FROM t;")),
            Some(2999),
        ),
        (
            "a correlated subquery fails",
            t(&format!("SELECT a, (SELECT {BAD}) FROM t;")),
            Some(2999),
        ),
        (
            "the same statement twice in one script",
            t(&format!("SELECT a, {BAD} FROM t; SELECT a, {BAD} FROM t;")),
            Some(5998),
        ),
    ]
}

/// Counts the lines of a transcript's standard output.
///
/// @param text - what a shell printed
fn row_lines(text: &str) -> usize {
    text.lines().filter(|line| !line.trim().is_empty()).count()
}

/// Every script prints the same rows and the same error as SQLite.
#[test]
fn the_rows_before_a_runtime_error_match_sqlite() {
    let Some(oracle) = pinned_shell() else {
        inillucent_base::testing::skipping("the pinned SQLite 3.53.4 shell is not downloaded");
        return;
    };
    let ours = program("inillucent-shell");
    let mut differing: Vec<String> = Vec::new();
    for (name, sql, expected) in scripts() {
        let theirs_ran = transcript(&oracle, &sql);
        let ours_ran = transcript(&ours, &sql);
        // The oracle's own answer is checked first, so a script that stopped
        // failing in SQLite (and so proves nothing) is reported as that.
        match expected {
            Some(before) => assert_eq!(
                (row_lines(&theirs_ran.0), theirs_ran.1.contains("Error")),
                (before, true),
                "SQLite printed something other than {before} rows and an error for `{name}`: \
                 {} / {}",
                theirs_ran.0.lines().count(),
                theirs_ran.1
            ),
            None => assert!(
                !theirs_ran.1.contains("Error"),
                "SQLite raised an error for `{name}`: {}",
                theirs_ran.1
            ),
        }
        if theirs_ran != ours_ran {
            differing.push(format!(
                "{name}: SQLite printed {} lines then `{}`, this engine printed {} lines then `{}`",
                theirs_ran.0.lines().count(),
                theirs_ran.1.trim(),
                ours_ran.0.lines().count(),
                ours_ran.1.trim()
            ));
        }
    }
    assert!(
        differing.is_empty(),
        "these scripts do not print what SQLite prints:\n  {}",
        differing.join("\n  ")
    );
}

/// A database file with a table of four rows, the third of which fails the probe.
///
/// @param name - the file's name under the suite's scratch folder
fn small_database(name: &str) -> PathBuf {
    let folder = workspace_root().join("_agent_output/partial_rows");
    let _ = std::fs::create_dir_all(&folder);
    let path = folder.join(format!("{name}-{}.rdb", std::process::id()));
    let cli = program("inillucent");
    let named = path.to_string_lossy().into_owned();
    let made = cliproc::run(&cli, &["create", &named]);
    assert_eq!(made.code, 0, "{}", made.said());
    for sql in ["CREATE TABLE t(a)", "INSERT INTO t VALUES (1),(2),(3),(4)"] {
        let ran = cliproc::run(&cli, &["--db", &named, "exec", sql]);
        assert_eq!(ran.code, 0, "{}", ran.said());
    }
    path
}

/// The statement every command line and MCP case runs.
const PROBE: &str = "SELECT a, CASE WHEN a = 3 THEN json('x') END FROM t";

/// `inillucent query` prints the rows before the failure, then the failure.
#[test]
fn the_query_command_prints_the_rows_before_a_failure() {
    let database = small_database("text");
    let named = database.to_string_lossy().into_owned();
    let ran = cliproc::run(&program("inillucent"), &["--db", &named, "query", PROBE]);
    assert_eq!(ran.code, 1, "{}", ran.said());
    let printed: Vec<&str> = ran.stdout.lines().map(str::trim_end).collect();
    assert_eq!(
        printed.len(),
        4,
        "the header, its rule and the two rows before the failure: {printed:?}"
    );
    assert_eq!(printed.get(2).copied(), Some("1"));
    assert_eq!(printed.get(3).copied(), Some("2"));
    assert!(
        ran.stderr.contains("malformed JSON"),
        "the failure goes to standard error: {}",
        ran.stderr
    );
}

/// `inillucent query --output json` reports the rows beside the failure.
#[test]
fn the_json_failure_carries_the_rows_before_it() {
    let database = small_database("json");
    let named = database.to_string_lossy().into_owned();
    let ran = cliproc::run(
        &program("inillucent"),
        &["--db", &named, "query", PROBE, "--output", "json"],
    );
    assert_eq!(ran.code, 1, "{}", ran.said());
    let node = cliproc::document(&ran.stdout);
    let ok = cliproc::field(&node, "ok").map(inillucent_scalar::json::render::to_text);
    assert_eq!(ok.as_deref(), Some("false"), "{}", ran.stdout);
    assert_eq!(
        cliproc::text_field(&ran.stdout, "message"),
        "malformed JSON"
    );
    assert_eq!(
        cliproc::rows(&ran.stdout),
        vec![
            vec!["1".to_string(), "null".to_string()],
            vec!["2".to_string(), "null".to_string()]
        ],
        "{}",
        ran.stdout
    );
    assert_eq!(cliproc::number_field(&ran.stdout, "row_count"), 2.0);
}

/// A failure that came before any row has no rows member, as before.
#[test]
fn a_failure_before_the_first_row_has_no_rows() {
    let database = small_database("none");
    let named = database.to_string_lossy().into_owned();
    let ran = cliproc::run(
        &program("inillucent"),
        &[
            "--db",
            &named,
            "query",
            "SELECT json('x') FROM t",
            "--output",
            "json",
        ],
    );
    assert_eq!(ran.code, 1, "{}", ran.said());
    let node = cliproc::document(&ran.stdout);
    assert!(
        cliproc::field(&node, "rows").is_none(),
        "a failure on the first row reports no rows: {}",
        ran.stdout
    );
}

/// An MCP server on a database, and the two pipes to it.
struct Server {
    /// The process.
    child: Child,
    /// Its answers, one JSON object per line.
    answers: BufReader<std::process::ChildStdout>,
}

impl Server {
    /// Starts a server on a database and completes the handshake.
    ///
    /// @param database - the file to serve
    fn start(database: &Path) -> Server {
        let mut child = Command::new(program("inillucent-mcp"))
            .arg("--db")
            .arg(database)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap_or_else(|error| panic!("inillucent-mcp did not start: {error}"));
        let Some(stdout) = child.stdout.take() else {
            panic!("inillucent-mcp was started with a piped standard output and has none");
        };
        let mut server = Server {
            child,
            answers: BufReader::new(stdout),
        };
        let _ = server.ask(concat!(
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":"#,
            r#"{"protocolVersion":"2025-06-18","capabilities":{},"#,
            r#""clientInfo":{"name":"partial_rows"}}}"#
        ));
        if let Some(stdin) = server.child.stdin.as_mut() {
            let _ = writeln!(
                stdin,
                r#"{{"jsonrpc":"2.0","method":"notifications/initialized"}}"#
            );
            let _ = stdin.flush();
        }
        server
    }

    /// Sends one request and returns the line that came back.
    ///
    /// @param request - the JSON-RPC request, without its newline
    fn ask(&mut self, request: &str) -> String {
        if let Some(stdin) = self.child.stdin.as_mut() {
            let _ = writeln!(stdin, "{request}");
            let _ = stdin.flush();
        }
        let mut line = String::new();
        let _ = self.answers.read_line(&mut line);
        line
    }

    /// Stops the server.
    fn stop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The MCP `inillucent_query` tool reports the rows before a failure.
#[test]
fn the_mcp_query_tool_reports_the_rows_before_a_failure() {
    let database = small_database("mcp");
    let mut server = Server::start(&database);
    let request = |output: &str| {
        format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":\
             {{\"name\":\"inillucent_query\",\"arguments\":{{\"sql\":\"{PROBE}\"{output}}}}}}}"
        )
    };
    let plain = server.ask(&request(""));
    let json = server.ask(&request(",\"output\":\"json\""));
    server.stop();
    assert!(
        plain.contains("isError\":true") || plain.contains("isError\": true"),
        "a failure is a tool error: {plain}"
    );
    let rows_then_error = plain.find("\\n1\\n2\\n").zip(plain.find("malformed JSON"));
    assert!(
        rows_then_error.is_some_and(|(rows, error)| rows < error),
        "the text answer holds rows 1 and 2 and then the failure: {plain}"
    );
    assert!(
        json.contains("row_count") && json.contains("malformed JSON"),
        "the JSON answer holds the rows and the failure: {json}"
    );
}

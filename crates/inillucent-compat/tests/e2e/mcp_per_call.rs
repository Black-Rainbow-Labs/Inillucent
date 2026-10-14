//! What one MCP call may change about the session, and what it is refused.
//!
//! Invariant: **an argument names the call it is on and nothing after it, and a
//! tool does only what its description says.** A `db` argument runs that one
//! call on another file and the next call without `db` is back on the default
//! database, with any transaction it had still open. A read tool does not write.
//! A path the server may not reach is `invalid_state` whichever statement
//! named it. `ping` is answered while a tool call runs.
//!
//! ## Where these came from
//!
//! A bug hunt over `inillucent-mcp` found each by driving the shipped server over
//! its pipes: the `db` argument and `inillucent_create` re-pointed the session
//! for good (and rolled back an open transaction), a `db` naming a missing file
//! made it even under `--readonly`, `inillucent_query` ran `INSERT` and `DROP`,
//! `params-file: "-"` read the protocol's own standard input, and `ping` waited
//! for a running call. Each case below fails on the build before the fix.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use inillucent_compat::cliproc::{document, field, items, program, run, text_field, text_of};
use inillucent_compat::mcpclient::Session;
use inillucent_compat::workspace_root;

/// A scratch directory of one case's own, emptied first.
///
/// @param case - what to name it after
fn area(case: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output/mcp-per-call")
        .join(case);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// A path as JSON text accepts it: forward slashes, so no escape is needed.
///
/// @param path - the path
fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Starts the server on `app.rdb` in a case's directory.
///
/// @param directory - the case's directory
/// @param extra - the flags to start it with
fn serve(directory: &Path, extra: &[&str]) -> Session {
    let database = directory.join("app.rdb");
    let made = run(
        &program("inillucent"),
        &["create", &database.to_string_lossy()],
    );
    assert_eq!(made.code, 0, "`create` failed:\n{}", made.said());
    Session::start_with(&program("inillucent-mcp"), &database, extra)
}

/// Calls a tool and returns whether it refused, and the text it answered with.
///
/// @param session - the server
/// @param tool - the tool's name
/// @param arguments - its arguments as a JSON object
fn call(session: &mut Session, tool: &str, arguments: &str) -> (bool, String) {
    let reply = session.tool(tool, arguments);
    let node = document(&reply);
    let result = field(&node, "result");
    let Some(result) = result else {
        return (true, reply);
    };
    let refused = matches!(
        field(result, "isError"),
        Some(inillucent_scalar::json::node::Node::True)
    );
    let text = field(result, "content")
        .and_then(items)
        .and_then(|content| content.first())
        .and_then(|first| field(first, "text"))
        .and_then(text_of)
        .unwrap_or_default();
    (refused, text)
}

/// Calls a tool and returns the status it failed with, or `ok` when it did not.
///
/// The status is read from the text rendering, `Error [status]: message`, because
/// not every tool takes `output`.
///
/// @param session - the server
/// @param tool - the tool's name
/// @param arguments - its arguments as a JSON object
fn status_of(session: &mut Session, tool: &str, arguments: &str) -> String {
    let (refused, text) = call(session, tool, arguments);
    if !refused {
        return "ok".to_string();
    }
    text.strip_prefix("Error [")
        .and_then(|rest| rest.split(']').next())
        .map(str::to_string)
        .unwrap_or(text)
}

/// A `db` argument is for that call only, and an open transaction survives it.
///
/// The sequence is the one a client produces: begin on the default database,
/// insert, run one call on another file, and read the uncommitted row back.
/// Before the fix the call on the other file re-pointed the session, which
/// rolled the transaction back and left every later call on the other file.
#[test]
fn a_db_argument_is_for_one_call_and_keeps_an_open_transaction() {
    let directory = area("db_argument");
    let mut session = serve(&directory, &["--root", &directory.to_string_lossy()]);
    let other = slashed(&directory.join("other.rdb"));
    let (refused, said) = call(
        &mut session,
        "inillucent_create",
        &format!("{{\"path\":\"{other}\"}}"),
    );
    assert!(!refused, "create refused: {said}");

    for sql in ["CREATE TABLE t (x)", "BEGIN", "INSERT INTO t VALUES (1)"] {
        let (refused, said) = call(
            &mut session,
            "inillucent_exec",
            &format!("{{\"sql\":\"{sql}\"}}"),
        );
        assert!(!refused, "{sql} refused: {said}");
    }
    let (refused, said) = call(
        &mut session,
        "inillucent_query",
        &format!("{{\"sql\":\"SELECT 1 AS one\",\"db\":\"{other}\"}}"),
    );
    assert!(
        !refused && said.contains("one"),
        "the call on the other file failed: {said}"
    );

    let (refused, count) = call(
        &mut session,
        "inillucent_query",
        "{\"sql\":\"SELECT count(*) AS n FROM t\"}",
    );
    assert!(!refused, "the default database lost its table: {count}");
    assert!(
        count.contains('1'),
        "the open transaction was rolled back: {count}"
    );
    let (_, databases) = call(&mut session, "inillucent_databases", "{}");
    assert!(
        databases.contains("app.rdb") && !databases.contains("other.rdb"),
        "the session was re-pointed: {databases}"
    );
    let (refused, said) = call(&mut session, "inillucent_exec", "{\"sql\":\"COMMIT\"}");
    assert!(!refused, "the transaction could not be committed: {said}");
    let (_, after) = call(
        &mut session,
        "inillucent_query",
        "{\"sql\":\"SELECT count(*) AS n FROM t\"}",
    );
    assert!(after.contains('1'), "the committed row is gone: {after}");
}

/// `inillucent_create` makes the file and leaves the session where it was.
#[test]
fn create_does_not_move_the_session() {
    let directory = area("create_stays");
    let mut session = serve(&directory, &["--root", &directory.to_string_lossy()]);
    let other = slashed(&directory.join("made.rdb"));
    let (refused, said) = call(
        &mut session,
        "inillucent_create",
        &format!("{{\"path\":\"{other}\"}}"),
    );
    assert!(!refused, "create refused: {said}");
    assert!(directory.join("made.rdb").is_file(), "create made no file");
    let (_, databases) = call(&mut session, "inillucent_databases", "{}");
    assert!(
        databases.contains("app.rdb") && !databases.contains("made.rdb"),
        "{databases}"
    );
}

/// A `db` naming a file that is not there is `not_found` and makes nothing,
/// with or without `--readonly`.
#[test]
fn a_db_argument_naming_a_missing_file_makes_nothing() {
    for (case, extra) in [
        ("missing_rw", Vec::new()),
        ("missing_ro", vec!["--readonly"]),
    ] {
        let directory = area(case);
        let mut session = serve(&directory, &extra);
        let target = directory.join("typo.rdb");
        let status = status_of(
            &mut session,
            "inillucent_query",
            &format!("{{\"sql\":\"SELECT 1\",\"db\":\"{}\"}}", slashed(&target)),
        );
        assert_eq!(status, "not_found", "{case}");
        assert!(!target.exists(), "{case}: the file was made");
    }
}

/// `inillucent_query` refuses a statement that writes, `inillucent_exec` runs it.
#[test]
fn the_read_tool_refuses_a_write() {
    let directory = area("query_writes");
    let mut session = serve(&directory, &[]);
    let (refused, said) = call(
        &mut session,
        "inillucent_exec",
        "{\"sql\":\"CREATE TABLE u (x)\"}",
    );
    assert!(!refused, "{said}");
    for sql in [
        "INSERT INTO u VALUES (99)",
        "DELETE FROM u",
        "CREATE TABLE made (a)",
        "DROP TABLE u",
    ] {
        let status = status_of(
            &mut session,
            "inillucent_query",
            &format!("{{\"sql\":\"{sql}\"}}"),
        );
        assert_eq!(status, "readonly", "{sql}");
    }
    let (refused, said) = call(
        &mut session,
        "inillucent_query",
        "{\"sql\":\"SELECT count(*) AS n FROM u\"}",
    );
    assert!(
        !refused && said.contains('0'),
        "a refused write still ran: {said}"
    );
    let (_, tables) = call(&mut session, "inillucent_tables", "{}");
    assert!(!tables.contains("made"), "{tables}");
    let (refused, said) = call(
        &mut session,
        "inillucent_exec",
        "{\"sql\":\"INSERT INTO u VALUES (1)\"}",
    );
    assert!(!refused, "exec could not write: {said}");
}

/// The command line's `query` refuses a write the same way.
#[test]
fn the_command_line_query_refuses_a_write() {
    let directory = area("cli_query_writes");
    let database = directory.join("app.rdb").to_string_lossy().into_owned();
    let binary = program("inillucent");
    let made = run(&binary, &["--db", &database, "exec", "CREATE TABLE u (x)"]);
    assert_eq!(made.code, 0, "{}", made.said());
    let ran = run(
        &binary,
        &[
            "--db",
            &database,
            "--output",
            "json",
            "query",
            "INSERT INTO u VALUES (1)",
        ],
    );
    assert_eq!(ran.code, 1, "{}", ran.said());
    assert_eq!(text_field(&ran.stdout, "status"), "readonly");
    let counted = run(
        &binary,
        &["--db", &database, "query", "SELECT count(*) FROM u"],
    );
    assert!(counted.stdout.contains('0'), "{}", counted.said());
}

/// A statement that does not parse is `syntax` on `query`, on the command line
/// and over MCP, and not `readonly`.
///
/// Bug hunt 10: `SELEC 1`, `FOO BAR` and `INSERT INTO` have no statement class
/// that a read only filter can admit, so `query` reported `readonly` for a typo.
#[test]
fn a_statement_that_does_not_parse_is_a_syntax_error_on_query() {
    let directory = area("query_syntax");
    let database = directory.join("app.rdb").to_string_lossy().into_owned();
    let binary = program("inillucent");
    let made = run(&binary, &["--db", &database, "exec", "CREATE TABLE u (x)"]);
    assert_eq!(made.code, 0, "{}", made.said());
    let mut session = serve(&area("query_syntax_mcp"), &[]);
    for sql in ["SELEC 1", "FOO BAR", "INSERT INTO"] {
        let ran = run(
            &binary,
            &["--db", &database, "--output", "json", "query", sql],
        );
        assert_eq!(ran.code, 1, "{sql}: {}", ran.said());
        assert_eq!(text_field(&ran.stdout, "status"), "syntax", "{sql}");
        let status = status_of(
            &mut session,
            "inillucent_query",
            &format!("{{\"sql\":\"{sql}\"}}"),
        );
        assert_eq!(status, "syntax", "mcp {sql}");
    }
}

/// `params-file` of `-` is refused, and the next request is still answered.
#[test]
fn a_params_file_of_dash_does_not_read_the_protocol() {
    let directory = area("params_dash");
    let mut session = serve(&directory, &[]);
    let status = status_of(
        &mut session,
        "inillucent_query",
        "{\"sql\":\"SELECT ?1\",\"params-file\":\"-\"}",
    );
    assert_eq!(status, "invalid_state");
    let (refused, said) = call(
        &mut session,
        "inillucent_query",
        "{\"sql\":\"SELECT 51 AS n\"}",
    );
    assert!(
        !refused && said.contains("51"),
        "the next request was lost: {said}"
    );
}

/// A `SELECT` after a write reports no changes and no rowid of its own.
#[test]
fn a_select_reports_no_changes_after_a_write() {
    let directory = area("stale_changes");
    let mut session = serve(&directory, &[]);
    for sql in ["CREATE TABLE m (x)", "INSERT INTO m VALUES (1)"] {
        let (refused, said) = call(
            &mut session,
            "inillucent_exec",
            &format!("{{\"sql\":\"{sql}\"}}"),
        );
        assert!(!refused, "{said}");
    }
    let (_, text) = call(
        &mut session,
        "inillucent_query",
        "{\"sql\":\"SELECT 1\",\"output\":\"json\"}",
    );
    assert_eq!(
        inillucent_compat::cliproc::number_field(&text, "changes"),
        0.0,
        "{text}"
    );
    assert_eq!(
        inillucent_compat::cliproc::number_field(&text, "last_insert_rowid"),
        0.0,
        "{text}"
    );
    let (_, text) = call(
        &mut session,
        "inillucent_exec",
        "{\"sql\":\"INSERT INTO m VALUES (2)\",\"output\":\"json\"}",
    );
    assert_eq!(
        inillucent_compat::cliproc::number_field(&text, "changes"),
        1.0,
        "{text}"
    );
    assert_eq!(
        inillucent_compat::cliproc::number_field(&text, "last_insert_rowid"),
        2.0,
        "{text}"
    );
}

/// A path outside the root is `invalid_state` for `ATTACH` and `VACUUM INTO` as
/// it is for the file tools.
#[test]
fn a_path_outside_the_root_is_invalid_state_for_attach_and_vacuum() {
    let directory = area("outside_root");
    let mut session = serve(&directory, &["--root", &directory.to_string_lossy()]);
    let elsewhere = slashed(
        &directory
            .parent()
            .unwrap_or(&directory)
            .join("outside-root-target.rdb"),
    );
    for (tool, arguments) in [
        ("inillucent_backup", format!("{{\"file\":\"{elsewhere}\"}}")),
        (
            "inillucent_exec",
            format!("{{\"sql\":\"ATTACH '{elsewhere}' AS o\"}}"),
        ),
        (
            "inillucent_exec",
            format!("{{\"sql\":\"VACUUM INTO '{elsewhere}'\"}}"),
        ),
    ] {
        assert_eq!(
            status_of(&mut session, tool, &arguments),
            "invalid_state",
            "{arguments}"
        );
    }
}

/// A Windows device name or alternate data stream is refused as a file name
/// under `--root`, by every tool that takes a path.
#[cfg(windows)]
#[test]
fn windows_device_names_and_streams_are_refused_under_a_root() {
    let directory = area("devices");
    let mut session = serve(&directory, &["--root", &directory.to_string_lossy()]);
    let inside = slashed(&directory);
    for name in [
        "CON",
        "nul.csv",
        "aux.txt",
        "COM1.rdb",
        "LPT9",
        "CONIN$",
        "CONOUT$",
        "app.rdb:stream",
    ] {
        let path = format!("{inside}/{name}");
        for (tool, arguments) in [
            ("inillucent_create", format!("{{\"path\":\"{path}\"}}")),
            (
                "inillucent_export",
                format!("{{\"sql\":\"SELECT 1\",\"out\":\"{path}\"}}"),
            ),
            ("inillucent_backup", format!("{{\"file\":\"{path}\"}}")),
            (
                "inillucent_exec",
                format!("{{\"sql\":\"ATTACH '{path}' AS d\"}}"),
            ),
            (
                "inillucent_exec",
                format!("{{\"sql\":\"VACUUM INTO '{path}'\"}}"),
            ),
        ] {
            assert_eq!(
                status_of(&mut session, tool, &arguments),
                "invalid_state",
                "{tool} {name}"
            );
        }
    }
    let (refused, said) = call(
        &mut session,
        "inillucent_query",
        "{\"sql\":\"SELECT 1 AS n\"}",
    );
    assert!(!refused, "the server stopped answering: {said}");
}

/// `setup-embeddings` on a server with a root writes nothing outside it.
#[test]
fn setup_embeddings_without_a_dir_is_refused_under_a_root() {
    let directory = area("setup_root");
    let mut session = serve(&directory, &["--root", &directory.to_string_lossy()]);
    let status = status_of(
        &mut session,
        "inillucent_setup_embeddings",
        "{\"threads\":3}",
    );
    assert_eq!(status, "invalid_state");
}

/// A vector of the wrong width, or holding a value that is not finite, is
/// refused with a sentence that names the problem.
#[test]
fn a_bad_vector_is_named() {
    let directory = area("bad_vector");
    let mut session = serve(&directory, &[]);
    for sql in [
        "CREATE TABLE vec (id INTEGER PRIMARY KEY, e VECTOR(3))",
        "INSERT INTO vec VALUES (1, '[1,0,0]')",
    ] {
        let (refused, said) = call(
            &mut session,
            "inillucent_exec",
            &format!("{{\"sql\":\"{sql}\"}}"),
        );
        assert!(!refused, "{sql}: {said}");
    }
    let (refused, said) = call(
        &mut session,
        "inillucent_vector_search",
        "{\"table\":\"vec\",\"column\":\"e\",\"vector\":[1,0]}",
    );
    assert!(
        refused && said.contains("expected dimension 3, got 2"),
        "{said}"
    );
    let (refused, said) = call(
        &mut session,
        "inillucent_vector_search",
        "{\"table\":\"vec\",\"column\":\"e\",\"vector\":[1,0,1e999]}",
    );
    assert!(refused && said.contains("finite"), "{said}");
    let (refused, said) = call(
        &mut session,
        "inillucent_vector_search",
        "{\"table\":\"vec\",\"column\":\"e\",\"vector\":[1,0,0]}",
    );
    assert!(!refused, "{said}");
}

/// A `ping` before the handshake is answered.
#[test]
fn a_ping_before_initialize_is_answered() {
    let directory = area("ping_first");
    let database = directory.join("app.rdb");
    let made = run(
        &program("inillucent"),
        &["create", &database.to_string_lossy()],
    );
    assert_eq!(made.code, 0, "`create` failed:\n{}", made.said());
    let mut session = Session::start_silent(&program("inillucent-mcp"), &database, &[]);
    let reply = session.call("ping", "{}");
    assert!(reply.contains("\"result\":{}"), "{reply}");
}

/// A `ping` is answered while a tool call is still running.
#[test]
fn a_ping_is_answered_while_a_call_runs() {
    let directory = area("ping_busy");
    let mut session = serve(&directory, &[]);
    let (refused, said) = call(
        &mut session,
        "inillucent_exec",
        "{\"sql\":\"CREATE TABLE t (n INTEGER)\"}",
    );
    assert!(!refused, "{said}");
    let values: Vec<String> = (0..600).map(|nth| format!("({nth})")).collect();
    let (refused, said) = call(
        &mut session,
        "inillucent_exec",
        &format!(
            "{{\"sql\":\"INSERT INTO t (n) VALUES {}\"}}",
            values.join(",")
        ),
    );
    assert!(!refused, "{said}");
    let slow = session.send(
        "tools/call",
        "{\"name\":\"inillucent_query\",\"arguments\":{\"sql\":\"SELECT count(*) FROM t AS a, t AS b, t AS c WHERE a.n + b.n + c.n >= 0\"}}",
    );
    std::thread::sleep(Duration::from_millis(300));
    let asked = Instant::now();
    let ping = session.send("ping", "{}");
    let reply = session.reply_to(ping, "ping");
    let waited = asked.elapsed();
    assert!(reply.contains("\"result\":{}"), "{reply}");
    session.notify(&format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{{\"requestId\":{slow}}}}}"
    ));
    let answered = session.reply_to(slow, "the slow query");
    assert!(
        waited < Duration::from_secs(2),
        "the ping waited {waited:?} for the running call, which answered {answered}"
    );
}

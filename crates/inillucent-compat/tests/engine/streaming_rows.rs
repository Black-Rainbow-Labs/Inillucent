//! Statements that hand their rows over one at a time.
//!
//! Invariant: **`Connection::query_each` hands a sink exactly the rows, the column names and the
//! count that `Connection::query_all` returns, for every kind of statement, and holds none of
//! them.** A statement the engine cannot run one row at a time (a compound query, a window
//! function, a write with `RETURNING`, a pragma) runs as it always did and its rows are then handed
//! over, so the sink cannot tell which kind it was given. The memory the streaming saves is
//! measured by `streaming_memory`; this suite is about what is returned.

use std::cell::RefCell;
use std::ops::ControlFlow;
use std::path::PathBuf;
use std::rc::Rc;

use inillucent_driver::{Column, Database, Error, Result, RowSink, Status, Value};

/// How many rows the large table holds, which is several batches of the engine's pipeline.
const ROWS: i64 = 6_000;

/// Keeps what a statement hands over, and stops or fails when it is told to.
#[derive(Default)]
struct Keep {
    /// The result columns the sink was told.
    columns: Vec<String>,
    /// How many times the sink was told the columns.
    told: usize,
    /// The rows handed over.
    rows: Vec<Vec<Value>>,
    /// Stop the statement after this many rows.
    stop_after: Option<usize>,
    /// Fail after this many rows.
    fail_after: Option<usize>,
}

impl RowSink for Keep {
    /// Keeps the column names.
    ///
    /// @param columns - the result columns
    fn columns(&mut self, columns: &[Column]) {
        self.told += 1;
        self.columns = columns.iter().map(|column| column.name.clone()).collect();
    }

    /// Keeps a row, then stops or fails when the case says to.
    ///
    /// @param row - the row's values
    fn row(&mut self, row: &[Value]) -> Result<ControlFlow<()>> {
        self.rows.push(row.to_vec());
        if self.fail_after == Some(self.rows.len()) {
            return Err(Error::said(Status::InvalidState, "the sink refused a row"));
        }
        if self.stop_after == Some(self.rows.len()) {
            return Ok(ControlFlow::Break(()));
        }
        Ok(ControlFlow::Continue(()))
    }
}

/// Returns a directory for one case, made empty.
///
/// @param name - the case's name
fn directory(name: &str) -> PathBuf {
    let directory = inillucent_compat::workspace_root()
        .join("_agent_output")
        .join("streaming-rows")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    directory
}

/// Opens a database holding the tables the cases read.
///
/// `t` has `ROWS` rows of an integer and some text, `u` has a few rows that join to it, and `log`
/// takes the rows a `RETURNING` case writes.
///
/// @param name - the case's name
fn open(name: &str) -> Database {
    let database = Database::open(directory(name).join("app.rdb")).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE t (a INTEGER, b TEXT);\
             CREATE TABLE u (a INTEGER PRIMARY KEY, label TEXT);\
             CREATE TABLE log (id INTEGER PRIMARY KEY, v INTEGER);",
        )
        .expect("the tables are made");
    connection
        .execute_batch(&format!(
            "BEGIN;\
             WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < {ROWS}) \
             INSERT INTO t SELECT i, 'row-' || i FROM s;\
             INSERT INTO u SELECT a, 'label-' || a FROM t WHERE a % 500 = 0;\
             COMMIT;"
        ))
        .expect("the rows are written");
    drop(connection);
    database
}

/// Runs a statement both ways and returns what each gave: the rows and column names `query_all`
/// returned, and the sink and the summary `query_each` produced.
///
/// @param database - the database
/// @param sql - the statement
/// @param params - the bound values
fn both_ways(
    database: &Database,
    sql: &str,
    params: &[Value],
) -> (
    inillucent_driver::Rows,
    Rc<RefCell<Keep>>,
    inillucent_driver::Streamed,
) {
    let connection = database.session();
    let sink = Rc::new(RefCell::new(Keep::default()));
    let streamed = connection
        .query_each(sql, params, &sink)
        .unwrap_or_else(|error| panic!("{sql}: query_each failed: {}", error.message));
    let collected = connection
        .query_all(sql, params)
        .unwrap_or_else(|error| panic!("{sql}: query_all failed: {}", error.message));
    (collected, sink, streamed)
}

/// Every kind of read gives the sink what `query_all` returns.
///
/// The shapes are one pipeline (a scan, a filter, a sort, a group, a join, a limit) and the shapes
/// that are not (a compound, a window, a pragma, `EXPLAIN`), with bound values, with no rows, and
/// with a limit of zero.
#[test]
fn every_kind_of_read_gives_the_sink_what_query_all_returns() {
    let database = open("reads");
    let cases: Vec<(&str, Vec<Value>)> = vec![
        ("SELECT a, b FROM t", vec![]),
        ("SELECT a FROM t WHERE a % 7 = 0 ORDER BY b DESC", vec![]),
        ("SELECT a % 10, count(*), sum(a) FROM t GROUP BY 1 ORDER BY 1", vec![]),
        ("SELECT a FROM t UNION SELECT a + 1 FROM t", vec![]),
        ("SELECT a FROM t UNION ALL SELECT a FROM u", vec![]),
        ("SELECT a, row_number() OVER (ORDER BY a DESC) FROM t", vec![]),
        ("SELECT t.a, u.label FROM t JOIN u ON t.a = u.a", vec![]),
        ("SELECT a FROM t WHERE a < ?1", vec![Value::Integer(4_500)]),
        ("SELECT a, b FROM t WHERE b = ?1", vec![Value::Text("row-77".to_string())]),
        ("SELECT a FROM t LIMIT 0", vec![]),
        ("SELECT a FROM t WHERE a < 0", vec![]),
        ("SELECT a FROM t LIMIT 10 OFFSET 5", vec![]),
        ("SELECT count(*) FROM t", vec![]),
        ("SELECT 1, 'two', 3.5, NULL, x'ff'", vec![]),
        ("PRAGMA table_info(t)", vec![]),
        ("EXPLAIN SELECT a FROM t", vec![]),
        ("EXPLAIN QUERY PLAN SELECT a FROM t WHERE a = 3", vec![]),
        ("WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < 3000) SELECT i FROM s", vec![]),
        ("SELECT a FROM t WHERE a IN (SELECT a FROM u)", vec![]),
        ("SELECT a, (SELECT label FROM u WHERE u.a = t.a) FROM t WHERE a <= 1000", vec![]),
    ];
    for (sql, params) in cases {
        let (collected, sink, streamed) = both_ways(&database, sql, &params);
        let sink = sink.borrow();
        assert_eq!(sink.rows, collected.rows, "{sql}: the rows differ");
        let names: Vec<String> = collected
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect();
        assert_eq!(sink.columns, names, "{sql}: the column names differ");
        assert_eq!(sink.told, 1, "{sql}: the columns are told once");
        assert_eq!(streamed.total, collected.total, "{sql}: the counts differ");
        assert_eq!(
            streamed.total,
            sink.rows.len(),
            "{sql}: the count is not the rows"
        );
        assert_eq!(
            streamed.affected, collected.affected,
            "{sql}: affected differs"
        );
        assert_eq!(streamed.tag, collected.tag, "{sql}: the tag differs");
        assert!(!streamed.stopped, "{sql}: the sink did not stop it");
    }
}

/// A write with `RETURNING` and a statement with no result columns answer through the sink too.
#[test]
fn a_write_answers_through_the_sink_as_it_does_through_query() {
    let database = open("writes");
    let connection = database.session();
    let sink = Rc::new(RefCell::new(Keep::default()));
    let written = connection
        .query_each(
            "INSERT INTO log (v) SELECT a FROM t WHERE a <= 3 RETURNING id, v",
            &[],
            &sink,
        )
        .expect("the insert runs");
    assert_eq!(
        sink.borrow().rows,
        vec![
            vec![Value::Integer(1), Value::Integer(1)],
            vec![Value::Integer(2), Value::Integer(2)],
            vec![Value::Integer(3), Value::Integer(3)],
        ]
    );
    assert_eq!(written.total, 3);
    assert_eq!(written.affected, Some(3));
    let quiet = Rc::new(RefCell::new(Keep::default()));
    let updated = connection
        .query_each("UPDATE log SET v = v + 10", &[], &quiet)
        .expect("the update runs");
    assert_eq!(updated.total, 0);
    assert_eq!(updated.affected, Some(3));
    assert!(quiet.borrow().rows.is_empty());
    let after = connection
        .query_all("SELECT sum(v) FROM log", &[])
        .expect("the sum reads");
    assert_eq!(after.value(0, 0), Some(&Value::Integer(36)));
}

/// A statement that is run again, with other values, hands over the rows of those values: the
/// chain the engine keeps for it is not left holding the consumer or the rows of the run before.
#[test]
fn a_statement_run_again_hands_over_the_rows_of_the_new_values() {
    let database = open("again");
    let connection = database.session();
    for bound in [10_i64, 5_000, 3, 5_999, 10] {
        let sql = "SELECT a, b FROM t WHERE a > ?1";
        let sink = Rc::new(RefCell::new(Keep::default()));
        let streamed = connection
            .query_each(sql, &[Value::Integer(bound)], &sink)
            .expect("the statement streams");
        let collected = connection
            .query_all(sql, &[Value::Integer(bound)])
            .expect("the statement collects");
        assert_eq!(sink.borrow().rows, collected.rows, "above {bound}");
        assert_eq!(streamed.total, (ROWS - bound) as usize, "above {bound}");
    }
}

/// A sink that answers `Break` stops the statement, and the count is the rows it was given.
#[test]
fn a_sink_that_breaks_stops_the_statement() {
    let database = open("break");
    let connection = database.session();
    let sink = Rc::new(RefCell::new(Keep {
        stop_after: Some(10),
        ..Keep::default()
    }));
    let streamed = connection
        .query_each("SELECT a FROM t", &[], &sink)
        .expect("the statement streams");
    assert!(streamed.stopped);
    assert_eq!(streamed.total, 10);
    let kept: Vec<Value> = sink
        .borrow()
        .rows
        .iter()
        .map(|row| row[0].clone())
        .collect();
    assert_eq!(kept, (1..=10).map(Value::Integer).collect::<Vec<_>>());
}

/// A sink that fails stops the statement, and the failure is what `query_each` returns.
#[test]
fn a_sink_that_fails_stops_the_statement_and_its_failure_is_returned() {
    let database = open("sink-failure");
    let connection = database.session();
    let sink = Rc::new(RefCell::new(Keep {
        fail_after: Some(3),
        ..Keep::default()
    }));
    let failed = connection
        .query_each("SELECT a FROM t", &[], &sink)
        .expect_err("the sink's failure is returned");
    assert_eq!(failed.status, Status::InvalidState);
    assert_eq!(failed.message, "the sink refused a row");
    assert_eq!(sink.borrow().rows.len(), 3);
}

/// A statement that fails part way hands over the rows of the batches before the failing one, and
/// then reports the failure.
///
/// The failing value is in the last batch of a result larger than one batch. The rows of a batch
/// are produced together, so the batch with the failing row delivers none of its rows, which is
/// what the pipeline did before when it kept the rows for the error.
#[test]
fn a_failure_part_way_follows_the_rows_before_it() {
    let database = open("failure");
    let connection = database.session();
    let sink = Rc::new(RefCell::new(Keep::default()));
    let failed = connection
        .query_each(
            "SELECT abs(CASE WHEN a = 5900 THEN -9223372036854775808 ELSE a END) FROM t",
            &[],
            &sink,
        )
        .expect_err("the overflow is reported");
    assert!(
        failed.message.contains("overflow"),
        "the failure is the overflow: {}",
        failed.message
    );
    let kept = sink.borrow();
    assert!(
        !kept.rows.is_empty(),
        "the rows before the failing batch were handed over"
    );
    assert!(kept.rows.len() < 5_900);
    for (at, row) in kept.rows.iter().enumerate() {
        assert_eq!(
            row[0],
            Value::Integer(at as i64 + 1),
            "rows arrive in order"
        );
    }
    let again = connection
        .query_all("SELECT count(*) FROM t", &[])
        .expect("the connection is usable afterwards");
    assert_eq!(again.value(0, 0), Some(&Value::Integer(ROWS)));
}

/// A statement the connection refuses for being read only is refused by `query_each` too, and
/// writes nothing.
#[test]
fn a_read_only_connection_refuses_a_write_through_query_each() {
    let database = open("readonly");
    let path = database.path().to_path_buf();
    drop(database);
    let options = inillucent_driver::OpenOptions {
        read_only: true,
        ..inillucent_driver::OpenOptions::default()
    };
    let database = Database::open_with(&path, options).expect("the file opens read only");
    let connection = database.session();
    let sink = Rc::new(RefCell::new(Keep::default()));
    let refused = connection
        .query_each("INSERT INTO log (v) VALUES (1)", &[], &sink)
        .expect_err("a write is refused");
    assert_eq!(refused.status, Status::ReadOnly);
    connection
        .query_each("SELECT count(*) FROM t", &[], &sink)
        .expect("a read is allowed");
}

/// The shell prints a result of several chunks of rows, in every mode that streams, as it prints
/// the same result when it collects the rows first.
///
/// `.explain on` keeps a statement on the path that collects every row and renders them in one
/// call, which is how the shell printed every result before it streamed. The table holds a NULL, a
/// text with a quote, a comma and a line break, a real, a blob and non ASCII text, in more than
/// three chunks of the rows the shell renders at a time.
#[test]
fn the_shell_prints_a_streamed_result_as_it_prints_a_collected_one() {
    let directory = directory("shell-modes");
    let database = directory.join("modes.rdb");
    let shell = inillucent_compat::cliproc::program("inillucent-shell");
    let setup = "CREATE TABLE m (id INTEGER, note TEXT, ratio REAL, raw BLOB);\n\
         WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < 1700) \
         INSERT INTO m SELECT i, \
           CASE i % 5 WHEN 0 THEN NULL WHEN 1 THEN 'say \"hi\", ok' WHEN 2 THEN 'two' || char(10) || 'lines' \
                      WHEN 3 THEN 'caf\u{e9} \u{4e2d}' ELSE 'plain' END, \
           i / 4.0, CASE WHEN i % 7 = 0 THEN x'00ff10' ELSE NULL END FROM s;\n";
    let made =
        inillucent_compat::cliproc::run_with_input(&shell, &[&database.to_string_lossy()], setup);
    assert_eq!(made.code, 0, "{}", made.said());
    for mode in [
        "list", "csv", "tabs", "quote", "ascii", "tcl", "c", "line", "insert", "json", "jobject",
        "html", "count",
    ] {
        for headers in ["off", "on"] {
            let script = |explain: &str| {
                format!(".headers {headers}\n.mode {mode}\n.explain {explain}\nSELECT * FROM m;\nSELECT id FROM m WHERE id > 100000;\nSELECT 1, 'x';\n")
            };
            let streamed = inillucent_compat::cliproc::run_with_input(
                &shell,
                &[&database.to_string_lossy()],
                &script("off"),
            );
            let collected = inillucent_compat::cliproc::run_with_input(
                &shell,
                &[&database.to_string_lossy()],
                &script("on"),
            );
            assert_eq!(streamed.code, 0, "{mode}: {}", streamed.stderr);
            assert!(
                mode == "count" || streamed.stdout.len() > 20_000,
                "{mode} headers {headers}: the result is several chunks"
            );
            assert_eq!(
                streamed.stdout, collected.stdout,
                "{mode} with headers {headers}: streamed and collected output differ"
            );
        }
    }
}

/// A write with `RETURNING` whose deferred foreign key fails when the statement ends hands over
/// the rows it returned, and then reports the failure.
///
/// This is what `Statement::step` does for the same statement: the rows come out first, then the
/// error. A statement that is not streamed must not lose them.
#[test]
fn a_write_that_fails_after_returning_rows_hands_them_over_before_the_failure() {
    let database = open("returning-failure");
    let connection = database.session();
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;\
             CREATE TABLE p (id INTEGER PRIMARY KEY);\
             CREATE TABLE c (id INTEGER PRIMARY KEY, pid INTEGER REFERENCES p(id) \
               DEFERRABLE INITIALLY DEFERRED);",
        )
        .expect("the tables are made");
    let sink = Rc::new(RefCell::new(Keep::default()));
    let failed = connection
        .query_each(
            "INSERT INTO c VALUES (1, 98), (2, 99) RETURNING id",
            &[],
            &sink,
        )
        .expect_err("the deferred key fails when the statement ends");
    assert_eq!(failed.status, Status::Constraint, "{}", failed.message);
    assert_eq!(
        sink.borrow().rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    let left = connection
        .query_all("SELECT count(*) FROM c", &[])
        .expect("the table reads");
    assert_eq!(
        left.value(0, 0),
        Some(&Value::Integer(0)),
        "the insert was undone"
    );
}

/// A window query that fails after producing rows hands them over before the failure, as the
/// reference shell prints them. A window query is not one pipeline, so its rows are collected and
/// the rows kept by the failure are what the sink is given.
#[test]
fn a_window_query_that_fails_hands_over_the_rows_it_had_before_the_failure() {
    let database = open("window-failure");
    let connection = database.session();
    connection
        .execute_batch("CREATE TABLE z1 (a); INSERT INTO z1 VALUES (9223372036854775807), (1);")
        .expect("the table is made");
    let sink = Rc::new(RefCell::new(Keep::default()));
    let failed = connection
        .query_each(
            "SELECT sum(a) OVER (ORDER BY a ROWS BETWEEN 1 PRECEDING AND CURRENT ROW) FROM z1",
            &[],
            &sink,
        )
        .expect_err("the sum overflows");
    assert!(failed.message.contains("overflow"), "{}", failed.message);
    assert_eq!(sink.borrow().rows, vec![vec![Value::Integer(1)]]);
}

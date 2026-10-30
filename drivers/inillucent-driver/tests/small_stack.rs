//! Valid SQL of modest depth on a thread the caller owns, with a small stack.
//!
//! Invariant: **a statement that nests deeply either answers or fails with an
//! error, and never ends the process, on any thread the caller started.**
//!
//! A language binding runs on the caller's thread. Python's main thread on Windows has 2,000,000
//! bytes of stack and every thread Rust or C starts by default has 1 MiB. The engine used about
//! 6 KB of native stack for each level of nesting, so 350 `OR` terms, 300 nested scalar subqueries,
//! 600 nested views or 400 recursive trigger levels overflowed it and the operating system ended
//! the process with a stack overflow. Each test here runs the statement on a thread with a stack of
//! 1 MiB and asserts a value: the answer, or an error message. A crash fails the test binary, which
//! is the failure these tests exist to catch.

use inillucent_driver::{Database, Value};

/// The stack the thread under test gets: the smaller of the two defaults above.
const SMALL_STACK: usize = 1 << 20;

/// What running one script on the small thread produced.
#[derive(Debug)]
enum Outcome {
    /// The last statement answered; the first value of its first row.
    Answered(Value),
    /// A statement failed; the error's message.
    Failed(String),
}

/// Runs `setup` as a batch and then `sql` as one query on a thread with a stack of 1 MiB.
///
/// @param setup - statements to run first, or an empty string
/// @param sql - the statement under test
fn run_on_small_stack(setup: String, sql: String) -> Outcome {
    run_with_stack(SMALL_STACK, setup, sql)
}

/// Runs `setup` as a batch and then `sql` as one query on a thread with `stack` bytes of stack.
///
/// @param stack - the thread's stack size in bytes
/// @param setup - statements to run first, or an empty string
/// @param sql - the statement under test
fn run_with_stack(stack: usize, setup: String, sql: String) -> Outcome {
    let runner = std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            let database = Database::open(":memory:").expect("the database opens");
            let connection = database.session();
            if !setup.is_empty() {
                if let Err(why) = connection.execute_batch(&setup) {
                    return Outcome::Failed(why.message);
                }
            }
            match connection.query(&sql, &[], usize::MAX) {
                Ok(rows) => Outcome::Answered(rows.value(0, 0).cloned().unwrap_or(Value::Null)),
                Err(why) => Outcome::Failed(why.message),
            }
        })
        .expect("the thread starts");
    runner
        .join()
        .expect("the thread finished without panicking")
}

/// Asserts that `outcome` is the answer `expected` or an error that names a depth or stack limit.
///
/// @param outcome - what the statement produced
/// @param expected - the correct answer, which a limit error may replace
fn answers_or_refuses(outcome: Outcome, expected: Value) {
    match outcome {
        Outcome::Answered(value) => assert_eq!(value, expected),
        Outcome::Failed(message) => assert!(
            message.contains("too large")
                || message.contains("too deep")
                || message.contains("stack")
                || message.contains("nest")
                || message.contains("limit")
                || message.contains("depth"),
            "the refusal does not name a limit: {message}"
        ),
    }
}

/// Returns `count` copies of `term` joined by `joiner`.
///
/// @param count - how many terms
/// @param term - one term
/// @param joiner - the operator text between terms
fn joined(count: usize, term: &str, joiner: &str) -> String {
    vec![term; count].join(joiner)
}

/// 400 `OR` terms in one expression.
#[test]
fn four_hundred_or_terms_do_not_overflow_the_stack() {
    let sql = format!("SELECT {}", joined(400, "1=1", " OR "));
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(1));
}

/// 700 `AND` terms in one expression.
#[test]
fn seven_hundred_and_terms_do_not_overflow_the_stack() {
    let sql = format!("SELECT {}", joined(700, "1=1", " AND "));
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(1));
}

/// 900 additions in one expression.
#[test]
fn nine_hundred_additions_do_not_overflow_the_stack() {
    let sql = format!("SELECT {}", joined(900, "1", "+"));
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(900));
}

/// 900 concatenations in one expression.
#[test]
fn nine_hundred_concatenations_do_not_overflow_the_stack() {
    let sql = format!("SELECT length({})", joined(900, "'a'", "||"));
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(900));
}

/// 600 nested `coalesce` calls.
#[test]
fn six_hundred_nested_coalesce_calls_do_not_overflow_the_stack() {
    let sql = format!(
        "SELECT {}1{}",
        "coalesce(NULL, ".repeat(600),
        ")".repeat(600)
    );
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(1));
}

/// 600 nested `NOT` operators.
#[test]
fn six_hundred_nested_not_operators_do_not_overflow_the_stack() {
    let sql = format!("SELECT {}1", "NOT ".repeat(600));
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(1));
}

/// 400 nested calls of `abs`.
#[test]
fn four_hundred_nested_abs_calls_do_not_overflow_the_stack() {
    let sql = format!("SELECT {}1{}", "abs(".repeat(400), ")".repeat(400));
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(1));
}

/// 300 nested scalar subqueries.
#[test]
fn three_hundred_nested_scalar_subqueries_do_not_overflow_the_stack() {
    let sql = format!("SELECT {}1{}", "(SELECT ".repeat(300), ")".repeat(300));
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(1));
}

/// A chain of 600 views, each selecting from the one before.
#[test]
fn a_chain_of_six_hundred_views_does_not_overflow_the_stack() {
    let mut setup = String::from("CREATE TABLE base (a INTEGER); INSERT INTO base VALUES (7);");
    setup.push_str("CREATE VIEW v0 AS SELECT a FROM base;");
    for level in 1..600 {
        setup.push_str(&format!(
            "CREATE VIEW v{level} AS SELECT a FROM v{};",
            level - 1
        ));
    }
    let outcome = run_on_small_stack(setup, "SELECT a FROM v599".to_string());
    answers_or_refuses(outcome, Value::Integer(7));
}

/// 400 common table expressions, each reading the one before.
#[test]
fn four_hundred_common_table_expressions_do_not_overflow_the_stack() {
    let mut sql = String::from("WITH c0(a) AS (SELECT 5)");
    for level in 1..400 {
        sql.push_str(&format!(", c{level}(a) AS (SELECT a FROM c{})", level - 1));
    }
    sql.push_str(" SELECT a FROM c399");
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(5));
}

/// 400 derived tables, each inside the one before.
#[test]
fn four_hundred_nested_derived_tables_do_not_overflow_the_stack() {
    let sql = format!(
        "{}SELECT 3{}",
        "SELECT * FROM (".repeat(400),
        ")".repeat(400)
    );
    answers_or_refuses(run_on_small_stack(String::new(), sql), Value::Integer(3));
}

/// A trigger that fires itself 400 levels deep with `recursive_triggers` on.
#[test]
fn recursive_triggers_four_hundred_deep_do_not_overflow_the_stack() {
    let setup = "PRAGMA recursive_triggers = ON; \
        CREATE TABLE r (x INTEGER); \
        CREATE TRIGGER r_again AFTER INSERT ON r WHEN new.x < 400 \
        BEGIN INSERT INTO r VALUES (new.x + 1); END;";
    let outcome = run_on_small_stack(setup.to_string(), "INSERT INTO r VALUES (1)".to_string());
    match outcome {
        Outcome::Answered(_) => {}
        Outcome::Failed(message) => assert!(
            message.contains("stack") || message.contains("trigger") || message.contains("depth"),
            "the refusal does not name a limit: {message}"
        ),
    }
}

/// A chain of 700 `COLLATE` operators on one value.
#[test]
fn seven_hundred_collate_operators_do_not_overflow_the_stack() {
    let sql = format!("SELECT 'a'{}", " COLLATE NOCASE".repeat(700));
    answers_or_refuses(
        run_on_small_stack(String::new(), sql),
        Value::Text("a".to_string()),
    );
}

/// 400 `OR` terms answer on a thread with 8 MiB, so the refusal on a small thread is made for want of
/// stack and not because the statement is refused everywhere.
#[test]
fn four_hundred_or_terms_answer_on_a_larger_stack() {
    let sql = format!("SELECT {}", joined(400, "1=1", " OR "));
    match run_with_stack(8 << 20, String::new(), sql) {
        Outcome::Answered(value) => assert_eq!(value, Value::Integer(1)),
        Outcome::Failed(message) => panic!("refused on 8 MiB: {message}"),
    }
}

/// 600 nested scalar subqueries on a thread with 1 MiB are refused with the message that names the
/// stack, which is what a caller reads to learn the remedy.
#[test]
fn a_refusal_for_want_of_stack_names_the_stack() {
    let sql = format!("SELECT {}1{}", "(SELECT ".repeat(600), ")".repeat(600));
    match run_on_small_stack(String::new(), sql) {
        Outcome::Failed(message) => {
            assert!(message.contains("stack of the calling thread"), "{message}")
        }
        Outcome::Answered(value) => panic!("answered {value:?} on 1 MiB, which needs about 5 MB"),
    }
}

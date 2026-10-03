//! Times single statements through the shipped `Connection` API, one shape at a time.
//!
//! Invariant: **each case runs a prepared statement many times inside one
//! transaction and reports nanoseconds per execution, and the cases differ in
//! one thing each.** The hill climb (task-2175) found single row inserts into
//! an indexed table at 30 microseconds through the `Connection` against 6 for
//! a plain insert into the contract fixture, while 20,000 of the same rows as
//! one `INSERT ... SELECT` cost 8 a row. A gate workload cannot say which part
//! of a statement is the cost; a pair of cases that differ in one thing can.
//!
//! Usage:
//!   inillucent-hcprobe `<scratch dir>` [--iterations N] [--only <case>] [--autocommit] [--setup-only | --reuse]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use inillucent_compat::newengine::connect::{Connection, Database};

#[global_allocator]
static ALLOCATOR: inillucent_alloc::Pooled = inillucent_alloc::Pooled;

/// One measured shape.
struct Case {
    /// The case's name.
    name: &'static str,
    /// Statements run once, untimed, on a fresh database.
    setup: &'static str,
    /// The measured statement.
    sql: &'static str,
    /// How the parameters are made from the iteration number.
    binds: fn(i64) -> Vec<i64>,
    /// Whether the statement is prepared on every execution.
    prepare_each: bool,
}

/// The schema every insert case writes into, the hillclimb plan's `fresh`.
const FRESH: &str = "CREATE TABLE fresh(id INTEGER PRIMARY KEY, grp INTEGER NOT NULL, \
     name TEXT NOT NULL, amount REAL, created INTEGER NOT NULL); \
     CREATE INDEX fresh_grp ON fresh(grp, created); \
     CREATE UNIQUE INDEX fresh_name ON fresh(name);";

/// The table the side table cases write into, the contract fixture's `side_table`.
const SIDE: &str =
    "CREATE TABLE side_table(id INTEGER PRIMARY KEY, owner INTEGER NOT NULL, note TEXT); \
     CREATE INDEX side_owner ON side_table(owner);";

/// The contract fixture's `side_table` with its 25,000 rows, for the cases that insert into a table already holding rows.
const SIDE_FILLED: &str = "CREATE TABLE side_table(id INTEGER PRIMARY KEY, owner INTEGER NOT NULL, note TEXT);      CREATE INDEX side_owner ON side_table(owner);      WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 25000)      INSERT INTO side_table SELECT i, ((i * 7) % 100000) + 1, 'note ' || i FROM c;";

/// `SIDE_FILLED` after 2,000 more rows went in, the state the gate's `app.insert.returning` starts from.
const SIDE_AFTER: &str = "CREATE TABLE side_table(id INTEGER PRIMARY KEY, owner INTEGER NOT NULL, note TEXT);      CREATE INDEX side_owner ON side_table(owner);      WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 25000)      INSERT INTO side_table SELECT i, ((i * 7) % 100000) + 1, 'note ' || i FROM c;      WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 2000)      INSERT INTO side_table(owner, note) SELECT (i * 1103515245 + 12345) % 2147483648, 'row ' || i || ' lorem ipsum dolor sit amet consectetur' FROM c;";

/// A 100,000 row table built by `INSERT ... SELECT`, for the bulk cases.
const COPIED: &str = "CREATE TABLE copied(id INTEGER PRIMARY KEY, key INTEGER, label TEXT);      WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 100000)      INSERT INTO copied SELECT i, (i * 2654435761) % 100000, 'row ' || i || ' lorem ipsum dolor sit amet' FROM c;";

/// Returns the scattered integer the gate's `int` bind produces.
///
/// @param iteration - the iteration number
fn scattered(iteration: i64) -> i64 {
    (iteration.wrapping_mul(1_103_515_245).wrapping_add(12_345)) & 0x7fff_ffff
}

/// Returns every case, in the order they run.
fn cases() -> Vec<Case> {
    let mut all = insert_cases();
    all.extend(bulk_cases());
    all.extend(read_cases());
    all
}

/// Returns the single row `INSERT` cases: a fresh table and a side table.
fn insert_cases() -> Vec<Case> {
    vec![
        Case {
            name: "fresh.expressions",
            setup: FRESH,
            sql: "INSERT INTO fresh(grp, name, amount, created) \
                  VALUES (?1 % 97, 'user ' || ?2, (?1 % 10000) * 0.25, ?2)",
            binds: |i| vec![scattered(i), 100_001 + i],
            prepare_each: false,
        },
        Case {
            name: "fresh.plain",
            setup: FRESH,
            sql: "INSERT INTO fresh(grp, name, amount, created) VALUES (?1, ?2, ?3, ?4)",
            binds: |i| vec![scattered(i) % 97, 100_001 + i, scattered(i) % 10_000, 100_001 + i],
            prepare_each: false,
        },
        Case {
            name: "fresh.no_unique",
            setup: "CREATE TABLE fresh(id INTEGER PRIMARY KEY, grp INTEGER NOT NULL, \
                    name TEXT NOT NULL, amount REAL, created INTEGER NOT NULL); \
                    CREATE INDEX fresh_grp ON fresh(grp, created); \
                    CREATE INDEX fresh_name ON fresh(name);",
            sql: "INSERT INTO fresh(grp, name, amount, created) VALUES (?1, ?2, ?3, ?4)",
            binds: |i| vec![scattered(i) % 97, 100_001 + i, scattered(i) % 10_000, 100_001 + i],
            prepare_each: false,
        },
        Case {
            name: "fresh.no_index",
            setup: "CREATE TABLE fresh(id INTEGER PRIMARY KEY, grp INTEGER NOT NULL, \
                    name TEXT NOT NULL, amount REAL, created INTEGER NOT NULL);",
            sql: "INSERT INTO fresh(grp, name, amount, created) VALUES (?1, ?2, ?3, ?4)",
            binds: |i| vec![scattered(i) % 97, 100_001 + i, scattered(i) % 10_000, 100_001 + i],
            prepare_each: false,
        },
        Case {
            name: "side.plain",
            setup: SIDE,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, ?2)",
            binds: |i| vec![scattered(i), i],
            prepare_each: false,
        },
        Case {
            name: "side.returning",
            setup: SIDE,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, ?2) RETURNING id",
            binds: |i| vec![scattered(i), i],
            prepare_each: false,
        },
        Case {
            name: "side.prepare_each",
            setup: SIDE,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, ?2)",
            binds: |i| vec![scattered(i), i],
            prepare_each: true,
        },
        Case {
            name: "side.big.plain",
            setup: SIDE_FILLED,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, 'row ' || ?2 || ' lorem ipsum dolor sit amet consectetur')",
            binds: |i| vec![scattered(i) % 100_000, i],
            prepare_each: false,
        },
        Case {
            name: "side.big.returning",
            setup: SIDE_FILLED,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, 'row ' || ?2 || ' lorem ipsum dolor sit amet consectetur') RETURNING id",
            binds: |i| vec![scattered(i) % 100_000, i],
            prepare_each: false,
        },
        Case {
            name: "side.text.plain",
            setup: SIDE_FILLED,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, ?2)",
            binds: |i| vec![1 + (i * 2_654_435_761) % 100_000, -1],
            prepare_each: false,
        },
        Case {
            name: "side.text.returning",
            setup: SIDE_FILLED,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, ?2) RETURNING id",
            binds: |i| vec![1 + (i * 2_654_435_761) % 100_000, -1],
            prepare_each: false,
        },
        Case {
            name: "side.after.returning",
            setup: SIDE_AFTER,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, ?2) RETURNING id",
            binds: |i| vec![1 + (i * 2_654_435_761) % 100_000, -1],
            prepare_each: false,
        },
        Case {
            name: "side.after.plain",
            setup: SIDE_AFTER,
            sql: "INSERT INTO side_table(owner, note) VALUES (?1, ?2)",
            binds: |i| vec![1 + (i * 2_654_435_761) % 100_000, -1],
            prepare_each: false,
        },
        Case {
            name: "side.rowid_given",
            setup: SIDE,
            sql: "INSERT INTO side_table(id, owner, note) VALUES (?1, ?2, ?3)",
            binds: |i| vec![i + 1, scattered(i), i],
            prepare_each: false,
        },
    ]
}

/// Returns the cases that write many rows in one statement.
fn bulk_cases() -> Vec<Case> {
    vec![
        Case {
            name: "bulk.delete.range",
            setup: COPIED,
            sql: "DELETE FROM copied WHERE id BETWEEN ?1 * 1000 + 1 AND ?1 * 1000 + 1000",
            binds: |i| vec![i % 100],
            prepare_each: false,
        },
        Case {
            name: "bulk.delete.big",
            setup: COPIED,
            sql: "DELETE FROM copied WHERE id BETWEEN 20000 AND 59999",
            binds: |_| Vec::new(),
            prepare_each: false,
        },
        Case {
            name: "bulk.update.all",
            setup: COPIED,
            sql: "UPDATE copied SET label = label || '!'",
            binds: |_| Vec::new(),
            prepare_each: false,
        },
        Case {
            name: "bulk.update.grow",
            setup: COPIED,
            sql: "UPDATE copied SET label = label || '!' WHERE id BETWEEN ?1 * 1000 + 1 AND ?1 * 1000 + 1000",
            binds: |i| vec![i % 100],
            prepare_each: false,
        },
        Case {
            name: "bulk.insert.select",
            setup: "CREATE TABLE source(id INTEGER PRIMARY KEY, key INTEGER, label TEXT);                     WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 100000)                     INSERT INTO source SELECT i, (i * 2654435761) % 100000, 'row ' || i || ' lorem ipsum dolor sit amet' FROM c;                     CREATE TABLE copied(id INTEGER PRIMARY KEY, key INTEGER, label TEXT);",
            sql: "INSERT INTO copied SELECT id, key, label FROM source WHERE id BETWEEN ?1 * 1000 + 1 AND ?1 * 1000 + 1000",
            binds: |i| vec![i % 100],
            prepare_each: false,
        },
    ]
}

/// Returns the read cases.
fn read_cases() -> Vec<Case> {
    vec![
        Case {
            name: "read.group.high",
            setup: COPIED,
            sql: "SELECT key % 50000, count(*) FROM copied GROUP BY 1 ORDER BY 2 DESC, 1 LIMIT 3",
            binds: |_| Vec::new(),
            prepare_each: false,
        },
        Case {
            name: "read.like.contains",
            setup: COPIED,
            sql: "SELECT count(*) FROM copied WHERE label LIKE '%9 lorem%'",
            binds: |_| Vec::new(),
            prepare_each: false,
        },
        Case {
            name: "read.json.where",
            setup: "CREATE TABLE docs(id INTEGER PRIMARY KEY, body TEXT);                     WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 5000)                     INSERT INTO docs SELECT i, json_object('k', i % 100, 'name', 'row ' || i, 'tags', json_array(i % 64, i % 7)) FROM c;",
            sql: "SELECT count(*) FROM docs WHERE json_extract(body, '$.k') = ?1 % 100",
            binds: |i| vec![i],
            prepare_each: false,
        },
        Case {
            name: "read.count.fresh",
            setup: COPIED,
            sql: "SELECT count(*), sum(key) FROM copied",
            binds: |_| Vec::new(),
            prepare_each: false,
        },
        Case {
            name: "read.cte.recursive",
            setup: "CREATE TABLE unused(id INTEGER PRIMARY KEY);",
            sql: "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 10000) SELECT sum(x) FROM c",
            binds: |_| Vec::new(),
            prepare_each: false,
        },
        Case {
            name: "select.point",
            setup: "CREATE TABLE side_table(id INTEGER PRIMARY KEY, owner INTEGER NOT NULL, note TEXT); \
                    WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 20000) \
                    INSERT INTO side_table SELECT i, i * 7, 'note ' || i FROM c;",
            sql: "SELECT note FROM side_table WHERE id = ?1",
            binds: |i| vec![1 + (i * 7919) % 20_000],
            prepare_each: false,
        },
    ]
}

/// Runs one case on a fresh database and returns nanoseconds per execution.
///
/// @param directory - where the case's database goes
/// @param case - the case
/// @param iterations - how many executions are timed
/// @param mode - the transaction and setup choices
fn run_case(directory: &Path, case: &Case, iterations: i64, mode: Mode) -> Result<f64, String> {
    let path = directory.join(format!("{}.rdb", case.name));
    if mode.reuse {
        return run_on(&path, case, iterations, mode);
    }
    let _ = std::fs::remove_file(&path);
    {
        let database = Database::open(&path).map_err(|error| error.to_string())?;
        database
            .session()
            .execute_batch(case.setup)
            .map_err(|error| format!("setup: {error}"))?;
    }
    if mode.setup_only {
        return Ok(0.0);
    }
    run_on(&path, case, iterations, mode)
}

/// How a run treats its database and its transactions.
#[derive(Clone, Copy)]
struct Mode {
    /// Each execution commits on its own instead of one transaction holding them all.
    autocommit: bool,
    /// Builds the case's database and measures nothing, so a profile of the next run sees only the measured part.
    setup_only: bool,
    /// Opens the database a `setup_only` run left and runs no setup.
    reuse: bool,
}

/// Opens a case's database and times its statement.
///
/// @param path - the database file
/// @param case - the case
/// @param iterations - how many executions are timed
/// @param mode - the transaction and setup choices
fn run_on(path: &Path, case: &Case, iterations: i64, mode: Mode) -> Result<f64, String> {
    let autocommit = mode.autocommit;
    let database = Database::open(path).map_err(|error| error.to_string())?;
    let connection = database.session();
    if !autocommit {
        connection
            .execute("BEGIN")
            .map_err(|error| error.to_string())?;
    }
    let started = Instant::now();
    time_executions(&connection, case, iterations)?;
    let nanos = started.elapsed().as_secs_f64() * 1e9 / iterations as f64;
    if !autocommit {
        connection
            .execute("COMMIT")
            .map_err(|error| error.to_string())?;
    }
    Ok(nanos)
}

/// Runs the case's statement the given number of times.
///
/// @param connection - the session, inside a transaction
/// @param case - the case
/// @param iterations - how many executions
fn time_executions(
    connection: &Connection<'_>,
    case: &Case,
    iterations: i64,
) -> Result<(), String> {
    let mut held = if case.prepare_each {
        None
    } else {
        Some(
            connection
                .prepare(case.sql)
                .map_err(|error| error.to_string())?,
        )
    };
    for iteration in 0..iterations {
        let mut fresh;
        let statement = match held.as_mut() {
            Some(statement) => statement,
            None => {
                fresh = connection
                    .prepare(case.sql)
                    .map_err(|error| error.to_string())?;
                &mut fresh
            }
        };
        statement.reset();
        let values = (case.binds)(iteration);
        let count = values.len();
        for (index, value) in values.into_iter().enumerate() {
            // A negative last value binds the gate's `text` parameter in its
            // place, so a case can match a workload that binds text.
            if index + 1 == count && value < 0 {
                statement
                    .bind_text(
                        index as u32 + 1,
                        &format!("row {iteration} lorem ipsum dolor sit amet consectetur"),
                    )
                    .map_err(|error| error.to_string())?;
                continue;
            }
            statement
                .bind_integer(index as u32 + 1, value)
                .map_err(|error| error.to_string())?;
        }
        while statement.step().map_err(|error| error.to_string())? {}
    }
    Ok(())
}

/// Returns a flag's value, when it was given.
///
/// @param arguments - the command line
/// @param name - the flag, with its dashes
fn flag(arguments: &[String], name: &str) -> Option<String> {
    let at = arguments.iter().position(|value| value == name)?;
    arguments.get(at.saturating_add(1)).cloned()
}

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let Some(directory) = arguments.first().filter(|first| !first.starts_with("--")) else {
        eprintln!("usage: inillucent-hcprobe <scratch dir> [--iterations N] [--only <case>] [--autocommit] [--setup-only | --reuse]");
        return ExitCode::from(2);
    };
    let directory = PathBuf::from(directory);
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("hcprobe: {error}");
        return ExitCode::from(2);
    }
    let iterations: i64 = flag(&arguments, "--iterations")
        .and_then(|value| value.parse().ok())
        .unwrap_or(2_000);
    let only = flag(&arguments, "--only");
    let mode = Mode {
        autocommit: arguments.iter().any(|value| value == "--autocommit"),
        setup_only: arguments.iter().any(|value| value == "--setup-only"),
        reuse: arguments.iter().any(|value| value == "--reuse"),
    };
    for case in cases() {
        if only.as_deref().is_some_and(|name| name != case.name) {
            continue;
        }
        match run_case(&directory, &case, iterations, mode) {
            Ok(nanos) => println!("{:<20} {:>10.0} ns", case.name, nanos),
            Err(reason) => {
                eprintln!("{}: {reason}", case.name);
                return ExitCode::from(1);
            }
        }
    }
    ExitCode::SUCCESS
}

//! Work done again and again inside one connection leaves nothing behind that grows.
//!
//! Invariant: **after any number of `CREATE TEMP TABLE` and `DROP TABLE` pairs, the temporary
//! database holds the same number of tree handles and about the same amount of log as after a
//! few of them, and a transaction that changes the same rows again and again keeps one undo
//! record for each row and rolls back to the first image.**
//!
//! ## Why this exists
//!
//! A loop of `CREATE TEMP TABLE z(a); DROP TABLE z;` grew the connection by about 1.3 KB a cycle
//! and slowed as it ran: 200,000 cycles took 45 s and 266 MB where SQLite takes 4 s and 6 MB
//! (bug hunt 9). Two things grew. The temporary database's log was never folded, because the fold
//! skips a file that has no path and the temporary databases were not in the list it walks. And
//! the map from a file's tree identifiers to this connection's handles kept an entry for every
//! tree that was ever dropped, and is searched linearly. A transaction of a million updates of
//! 1,000 rows held a million undo records, each a copy of a whole row.

use inillucent_compat::newengine::connect::{Connection, Database};
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database file of this test's own.
///
/// @param name - the case's name, which is also its directory's
fn fresh(name: &str) -> Database {
    let directory = workspace_root()
        .join("_agent_output/scratch-memory-flat")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    Database::open(directory.join("case.rdb")).expect("the database opens")
}

/// Reads one integer from the first row of a query.
///
/// @param session - the connection, which owns any temporary table the query names
/// @param sql - a query whose first column is an integer
fn integer(session: &Connection<'_>, sql: &str) -> i64 {
    let rows = session.query(sql).expect("the query runs");
    match rows.first().and_then(|row| row.first()) {
        Some(OwnedDatum::Int(number)) => *number,
        other => panic!("expected an integer from {sql}, got {other:?}"),
    }
}

/// Creating and dropping a temporary table many times leaves the handle map and the log small.
///
/// Before the fix the map held 20,000 entries after 20,000 cycles and the log held every record
/// of every cycle (about 25 MB).
#[test]
fn a_temp_table_created_and_dropped_in_a_loop_leaves_nothing_behind() {
    let database = fresh("temp_loop");
    let session = database.session();
    for _ in 0..50 {
        session
            .execute_batch("CREATE TEMP TABLE z(a); DROP TABLE z;")
            .expect("the cycle runs");
    }
    let (handles_early, _) = database.temp_bookkeeping();
    for _ in 0..20_000 {
        session
            .execute_batch("CREATE TEMP TABLE z(a); DROP TABLE z;")
            .expect("the cycle runs");
    }
    let (handles_late, log_bytes) = database.temp_bookkeeping();
    assert!(
        handles_late <= handles_early + 4,
        "the handle map grew from {handles_early} to {handles_late} entries over 20,000 cycles"
    );
    assert!(
        log_bytes <= 2 << 20,
        "the temporary database's log holds {log_bytes} bytes after 20,000 cycles"
    );
}

/// A table created in a temporary database after the loop works, and its rows survive the pruning.
#[test]
fn a_temp_table_made_after_the_loop_keeps_its_rows() {
    let database = fresh("temp_after");
    let session = database.session();
    for _ in 0..300 {
        session
            .execute_batch("CREATE TEMP TABLE z(a); INSERT INTO z VALUES (1); DROP TABLE z;")
            .expect("the cycle runs");
    }
    session
        .execute_batch("CREATE TEMP TABLE keep(a); INSERT INTO keep VALUES (7), (8);")
        .expect("the table is made");
    session
        .execute_batch("BEGIN; CREATE TEMP TABLE gone(a); INSERT INTO gone VALUES (1); DROP TABLE gone; ROLLBACK;")
        .expect("the rolled back cycle runs");
    session
        .execute_batch("BEGIN; DROP TABLE keep; ROLLBACK;")
        .expect("the rolled back drop runs");
    assert_eq!(integer(&session, "SELECT sum(a) FROM keep"), 15);
    assert_eq!(
        integer(&session, "SELECT count(*) FROM temp.sqlite_schema"),
        1
    );
}

/// Attaching and detaching an in memory database many times costs the same at the end as at the
/// start.
#[test]
fn an_in_memory_attach_and_detach_loop_stays_flat() {
    let database = fresh("attach_loop");
    let session = database.session();
    let cycles = 3_000;
    let timed = |count: usize| {
        let started = std::time::Instant::now();
        for _ in 0..count {
            session
                .execute_batch("ATTACH ':memory:' AS m; DETACH m;")
                .expect("the cycle runs");
        }
        started.elapsed().as_secs_f64()
    };
    let first = timed(cycles);
    let second = timed(cycles);
    assert!(
        second <= first * 2.5 + 0.2,
        "the second {cycles} cycles took {second:.3} s against {first:.3} s for the first"
    );
}

/// A transaction that changes the same rows many times rolls back to the first image of each.
///
/// The statements between the savepoints are the cases the dropped records could break: a
/// rollback to a savepoint needs the image from after the first statement, and a rollback of
/// the whole transaction needs the image from before it.
#[test]
fn repeated_updates_roll_back_to_the_image_before_each_floor() {
    let database = fresh("undo_floors");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT); \
             INSERT INTO t SELECT value, 'start' FROM generate_series(1, 50);",
        )
        .expect("the table is built");
    session
        .execute_batch("BEGIN")
        .expect("the transaction begins");
    for round in 0..40 {
        session
            .execute_batch(&format!(
                "UPDATE t SET v = 'a{round}' WHERE id = {}",
                round % 5 + 1
            ))
            .expect("the update runs");
    }
    session
        .execute_batch("SAVEPOINT mid")
        .expect("the savepoint is made");
    for round in 0..40 {
        session
            .execute_batch(&format!(
                "UPDATE t SET v = 'b{round}' WHERE id = {}",
                round % 5 + 1
            ))
            .expect("the update runs");
    }
    session
        .execute_batch("DELETE FROM t WHERE id = 1")
        .expect("the delete runs");
    session
        .execute_batch("ROLLBACK TO mid")
        .expect("the rollback to the savepoint runs");
    assert_eq!(
        integer(&session, "SELECT count(*) FROM t WHERE v LIKE 'b%'"),
        0
    );
    assert_eq!(
        integer(&session, "SELECT count(*) FROM t WHERE v LIKE 'a%'"),
        5
    );
    assert_eq!(integer(&session, "SELECT count(*) FROM t"), 50);
    session
        .execute_batch("UPDATE t SET v = 'c' WHERE id <= 5; UPDATE t SET v = 'd' WHERE id <= 5")
        .expect("the updates run");
    session
        .execute_batch("ROLLBACK")
        .expect("the rollback runs");
    assert_eq!(
        integer(&session, "SELECT count(*) FROM t WHERE v = 'start'"),
        50
    );
}

/// A statement that fails halfway restores the rows it changed, though an older statement of the
/// same transaction changed them first.
#[test]
fn a_failed_statement_restores_rows_an_earlier_statement_changed() {
    let database = fresh("undo_statement");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, v INTEGER UNIQUE); \
             INSERT INTO t SELECT value, value FROM generate_series(1, 10);",
        )
        .expect("the table is built");
    session
        .execute_batch("BEGIN")
        .expect("the transaction begins");
    session
        .execute_batch("UPDATE t SET v = v + 100")
        .expect("the first update runs");
    let failed = session.execute_batch("UPDATE t SET v = 105 WHERE id >= 2");
    assert!(
        failed.is_err(),
        "the second update must break the unique index"
    );
    assert_eq!(integer(&session, "SELECT sum(v) FROM t"), 55 + 1000);
    session
        .execute_batch("ROLLBACK")
        .expect("the rollback runs");
    assert_eq!(integer(&session, "SELECT sum(v) FROM t"), 55);
}

/// An insert of many consecutive rowids is undone completely, and the undo takes a few records.
#[test]
fn a_rolled_back_insert_of_many_rows_removes_every_row() {
    let database = fresh("undo_insert_run");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT); INSERT INTO t VALUES (1, 'x');",
        )
        .expect("the table is built");
    session
        .execute_batch("BEGIN")
        .expect("the transaction begins");
    session
        .execute_batch("INSERT INTO t SELECT value + 1, 'y' FROM generate_series(1, 5000)")
        .expect("the insert runs");
    session
        .execute_batch("SAVEPOINT s")
        .expect("the savepoint is made");
    session
        .execute_batch("INSERT INTO t SELECT value + 6000, 'z' FROM generate_series(1, 3000)")
        .expect("the second insert runs");
    assert_eq!(integer(&session, "SELECT count(*) FROM t"), 8001);
    session
        .execute_batch("ROLLBACK TO s")
        .expect("the rollback to the savepoint runs");
    assert_eq!(integer(&session, "SELECT count(*) FROM t"), 5001);
    session
        .execute_batch("ROLLBACK")
        .expect("the rollback runs");
    assert_eq!(integer(&session, "SELECT count(*) FROM t"), 1);
    assert_eq!(integer(&session, "SELECT max(id) FROM t"), 1);
}

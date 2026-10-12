//! Write statements finish in time that grows with the work they do.
//!
//! Invariant: **a write's cost depends on the rows it touches, not on how many
//! other rows share its leaf, how many other leaves are part empty, or how many
//! other tables the schema holds.**
//!
//! ## Why this exists
//!
//! The bug hunt's speed hunters found five statements that were 10 to 100 times
//! slower than SQLite, each for a cost that grew with something the statement did
//! not touch:
//!
//! - Updating a row that was written since its leaf was last packed rebuilt the
//!   whole delta area of the leaf. 40,000 upserts onto 1,000 keys took 952 ms
//!   against SQLite's 23 ms, while 10 keys and 40,000 keys were both fast.
//! - A leaf below half full tried to merge into its right sibling on every delete,
//!   and each attempt packed both leaves. A cascading delete of 100,000 children
//!   scattered over the table made 8,569 attempts for 50,000 deletes.
//! - Every `CREATE TABLE` synced the data file, and parsed and copied every table
//!   already in the schema. 1,600 tables in one transaction took 6.8 s against 0.1 s.
//! - A `WHERE` term that reads one table was tested after the join, once for every
//!   pair of rows, instead of before it.
//! - A line of 20,000 statements gave each statement a copy of the rest of the line.
//! - An index nested loop join that probed more than 64 leaves of a written index in no particular
//!   order built a packed copy of a leaf for most probes and kept none: 540 ms against 75 ms.
//! - `INSERT OR REPLACE` onto existing keys ran six tree operations per row: 750 ms against 130 ms
//!   onto new keys.
//! - `count(DISTINCT x)` over integers allocated an encoded key for every new value.
//! - `VACUUM` ran one statement for every row it copied.
//!
//! The limits are wide on purpose: they are about ten times what the fixed code
//! takes on a loaded machine, and several times less than the old code took, so
//! they tell the two apart without depending on the machine. Where a count says
//! the same thing it is used instead of a clock.

use std::time::{Duration, Instant};

use inillucent_compat::cliproc;
use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Opens a fresh database in a directory named for the case.
///
/// @param name - the case's name, which is also its directory's
fn fresh(name: &str) -> (std::path::PathBuf, Database) {
    let directory = workspace_root()
        .join("_agent_output/write-speed")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    (directory, database)
}

/// How many times longer than the same statement onto 10 keys the upsert and replace cases onto
/// 1,000 keys may take.
///
/// The old code copied a leaf's delta area for every write, and that area holds every key the
/// statement has written since the leaf was packed: 10 rows for the 10 key run, up to 1,000 for the
/// 1,000 key run. So the old code took many times longer onto 1,000 keys. Measured on a desktop,
/// the fixed code takes 2.5 times as long onto 1,000 keys for replaces and about as long for
/// upserts, and the old upserts took about 12 times as long as the fixed ones (952 ms against
/// 75 ms). A bound in seconds failed on the Windows CI runner, which ran the fixed replaces in
/// 2.9 s against 2.5 s because it is about five times slower than a desktop; the 10 key run is
/// slower there by the same amount.
const TIMES_TEN_KEYS: u32 = 6;

/// Times one statement onto 10 keys of a table of its own, for the 1,000 key case to be compared
/// with on the same machine at the same moment.
///
/// @param database - the database the case runs in
/// @param statement - the statement, writing into the table `few`
fn onto_ten_keys(database: &Database, statement: &str) -> Duration {
    let session = database.session();
    session
        .execute_batch("CREATE TABLE few(id INTEGER PRIMARY KEY, n INT);")
        .expect("the 10 key table is made");
    let started = Instant::now();
    session
        .execute_batch(statement)
        .expect("the statement onto 10 keys runs");
    started.elapsed()
}

/// Returns the integers in the first row a query answers.
///
/// @param database - the database
/// @param sql - a query returning integers
fn integers(database: &Database, sql: &str) -> Vec<i64> {
    let rows = database.session().query(sql).expect("the query runs");
    let first = rows.first().expect("the query answers a row");
    first
        .iter()
        .map(|value| match value {
            OwnedDatum::Int(n) => *n,
            other => panic!("{sql} answered {other:?}"),
        })
        .collect()
}

/// 40,000 upserts onto 1,000 keys leave 1,000 rows each updated 40 times, and do not copy the
/// leaf's delta area once per upsert.
///
/// The 1,000 rows are written after the table is created, so they sit in the delta area of the
/// leaf and nothing runs out of room. Each update of an integer to an integer is the same length
/// as the row it replaces, so it is written over it.
#[test]
fn upserts_onto_a_thousand_keys_finish_in_time() {
    let (_, database) = fresh("upsert");
    let few = onto_ten_keys(
        &database,
        "INSERT INTO few SELECT value % 10, 1 FROM generate_series(1, 40000) WHERE true \
         ON CONFLICT(id) DO UPDATE SET n = n + 1;",
    );
    let session = database.session();
    session
        .execute_batch("CREATE TABLE k(id INTEGER PRIMARY KEY, n INT);")
        .expect("the table is made");
    let started = Instant::now();
    session
        .execute_batch(
            "INSERT INTO k SELECT value % 1000, 1 FROM generate_series(1, 40000) WHERE true \
             ON CONFLICT(id) DO UPDATE SET n = n + 1;",
        )
        .expect("the upserts run");
    let took = started.elapsed();
    assert_eq!(
        integers(&database, "SELECT count(*), sum(n), min(n), max(n) FROM k"),
        vec![1000, 40_000, 40, 40]
    );
    println!("40,000 upserts onto 1,000 keys: {took:?}, against {few:?} onto 10 keys");
    assert!(
        took < few * TIMES_TEN_KEYS,
        "40,000 upserts onto 1,000 keys took {took:?}, more than {TIMES_TEN_KEYS} times the {few:?} \
         onto 10 keys; each one rebuilt the delta area of its leaf"
    );
}

/// 40,000 `INSERT OR REPLACE` statements onto 1,000 keys leave 1,000 rows with the last value
/// written to each.
#[test]
fn replaces_onto_a_thousand_keys_finish_in_time() {
    let (_, database) = fresh("replace");
    let few = onto_ten_keys(
        &database,
        "INSERT OR REPLACE INTO few SELECT value % 10, value FROM generate_series(1, 40000);",
    );
    let session = database.session();
    session
        .execute_batch("CREATE TABLE k(id INTEGER PRIMARY KEY, n INT);")
        .expect("the table is made");
    let started = Instant::now();
    session
        .execute_batch(
            "INSERT OR REPLACE INTO k SELECT value % 1000, value FROM generate_series(1, 40000);",
        )
        .expect("the replaces run");
    let took = started.elapsed();
    assert_eq!(
        integers(&database, "SELECT count(*), sum(n), min(n), max(n) FROM k"),
        vec![1000, 39_500_500, 39_001, 40_000]
    );
    println!("40,000 replaces onto 1,000 keys: {took:?}, against {few:?} onto 10 keys");
    assert!(
        took < few * TIMES_TEN_KEYS,
        "40,000 replaces onto 1,000 keys took {took:?}, more than {TIMES_TEN_KEYS} times the {few:?} \
         onto 10 keys; each one rebuilt the delta area of its leaf"
    );
}

/// A cascading delete whose children are scattered over the child table does not pack two
/// leaves together on every delete.
///
/// The children of one parent are 1,000 rowids apart, so each parent's delete takes a few rows
/// out of every leaf of the child table, and every leaf spends most of the statement below half
/// full next to a sibling that is also part empty. A merge attempt packs both leaves, so one per
/// delete cost more than the delete.
#[test]
fn cascading_delete_of_scattered_children_does_not_retry_merges() {
    let (_, database) = fresh("cascade");
    let session = database.session();
    session
        .execute_batch(
            "PRAGMA foreign_keys = ON; CREATE TABLE par(id INTEGER PRIMARY KEY); \
             CREATE TABLE ch(id INTEGER PRIMARY KEY, pid INT REFERENCES par(id) ON DELETE CASCADE); \
             CREATE INDEX chp ON ch(pid); \
             INSERT INTO par SELECT value FROM generate_series(1, 1000); \
             INSERT INTO ch SELECT value, value % 1000 + 1 FROM generate_series(1, 100000);",
        )
        .expect("the tables are built");
    let before = database.write_stats();
    session
        .execute_batch("DELETE FROM par WHERE id <= 250;")
        .expect("the cascade runs");
    let after = database.write_stats();
    assert_eq!(
        integers(
            &database,
            "SELECT (SELECT count(*) FROM par), (SELECT count(*) FROM ch)"
        ),
        vec![750, 75_000]
    );
    assert_eq!(after.deleted - before.deleted, 250 + 25_000 + 25_000);
    let attempts = after.merge_attempts - before.merge_attempts;
    assert!(
        attempts < 500,
        "{attempts} merge attempts for 50,250 deletes; the old code made 8,569"
    );
}

/// 1,200 `CREATE TABLE` statements in one transaction cost the same each, and the tables are
/// all there after the commit and after a reopen.
///
/// Each statement used to sync the data file and to parse and copy every table already in the
/// schema.
#[test]
fn many_create_tables_in_one_transaction_finish_in_time() {
    let (directory, database) = fresh("tables");
    let session = database.session();
    session
        .execute_batch("BEGIN;")
        .expect("the transaction opens");
    let started = Instant::now();
    for table in 0..1_200 {
        session
            .execute_batch(&format!(
                "CREATE TABLE m{table}(id INTEGER PRIMARY KEY, v INT, s TEXT);"
            ))
            .expect("the table is made");
    }
    session
        .execute_batch("INSERT INTO m0 VALUES (1, 10, 'first'); INSERT INTO m1199 VALUES (2, 20, 'last'); COMMIT;")
        .expect("the transaction commits");
    let took = started.elapsed();
    assert_eq!(
        integers(
            &database,
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table'"
        ),
        vec![1_200]
    );
    drop(database);
    let reopened = Database::open(directory.join("case.rdb")).expect("the file reopens");
    assert_eq!(
        integers(
            &reopened,
            "SELECT (SELECT count(*) FROM sqlite_schema), (SELECT v FROM m0), (SELECT v FROM m1199)"
        ),
        vec![1_200, 10, 20]
    );
    assert!(
        took < Duration::from_millis(3_000),
        "1,200 CREATE TABLE statements took {took:?}; each one touched every table before it"
    );
}

/// A `WHERE` term that reads one table of a join is tested before the join, so a join of two
/// 20,000 row tables with a term on each pairs 20 rows with 20,000, not 20,000 with 20,000.
///
/// `a` is `value % 1000`, so 20 rows of each table have `a = 1` and 20 have `a = 0`: 400 pairs.
#[test]
fn a_term_on_one_table_of_a_join_filters_before_the_join() {
    let (_, database) = fresh("join");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE t(a, b, c); \
             INSERT INTO t SELECT value % 1000, value % 77, value FROM generate_series(1, 20000);",
        )
        .expect("the table is built");
    let started = Instant::now();
    let both = integers(
        &database,
        "SELECT count(*) FROM t x, t y WHERE x.a = 1 AND y.a = 0",
    );
    let joined = integers(
        &database,
        "SELECT count(*) FROM t x JOIN t y ON x.c = y.c + 1 WHERE x.a = 1 AND y.a = 0",
    );
    let took = started.elapsed();
    assert_eq!(both, vec![400]);
    assert_eq!(joined, vec![19]);
    assert!(
        took < Duration::from_millis(3_000),
        "the two joins took {took:?}; the term on x was tested for every pair"
    );
}

/// Times one query, returning its answer and how long it took.
///
/// @param database - the database
/// @param sql - a query returning integers
fn timed(database: &Database, sql: &str) -> (Vec<i64>, Duration) {
    let started = Instant::now();
    let answer = integers(database, sql);
    (answer, started.elapsed())
}

/// Probing a written index in no particular order costs about what probing it in key order does.
///
/// The index is created before the rows are inserted, and a row is already in the table when they are
/// (an insert into an empty table builds clean leaves), so the leaves hold their entries in the
/// delta area. The pool kept 64 packed copies of such leaves and built a copy for most probes once
/// the index had more leaves than that. For 60,000 probes into 300,000 entries the old code took
/// 3.7 s in key order and 185 ms shuffled, and the fixed code about 150 ms for both.
#[test]
fn probes_into_a_written_index_in_any_order_cost_the_same() {
    let (_, database) = fresh("shuffled_probes");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE posts(id INTEGER PRIMARY KEY, user_id INT, note TEXT); \
             CREATE INDEX posts_user ON posts(user_id, note); \
             INSERT INTO posts VALUES (0, -1, 'seed'); \
             INSERT INTO posts SELECT value, (value * 7919) % 60000, printf('%040d', value) \
             FROM generate_series(1, 300000); \
             CREATE TABLE in_order(id INT); \
             INSERT INTO in_order SELECT value FROM generate_series(0, 59999); \
             CREATE TABLE shuffled(id INT); \
             INSERT INTO shuffled SELECT (value * 7907) % 60000 FROM generate_series(0, 59999);",
        )
        .expect("the tables are built");
    let probe = |outer: &str| {
        timed(
            &database,
            &format!(
                "SELECT count(*) FROM {outer} o LEFT JOIN posts p ON p.user_id = o.id WHERE p.id IS NULL"
            ),
        )
    };
    let (_, warm) = probe("in_order");
    let (ordered_answer, ordered) = probe("in_order");
    let (shuffled_answer, shuffled) = probe("shuffled");
    assert_eq!(ordered_answer, vec![0]);
    assert_eq!(shuffled_answer, vec![0]);
    println!("probes in key order: {ordered:?} (first {warm:?}), shuffled: {shuffled:?}");
    assert!(
        shuffled < ordered * 3 + Duration::from_millis(50)
            && ordered < shuffled * 3 + Duration::from_millis(50),
        "60,000 probes took {shuffled:?} shuffled and {ordered:?} in key order; a packed copy of a \
         leaf was built for most of the probes of one of the two orders"
    );
}

/// `INSERT OR REPLACE` onto keys that exist costs about what it does onto keys that do not.
///
/// A replace of an existing row used to look for a clash, check the other constraints, read the old
/// row, delete it, look for a clash again and write the new row.
#[test]
fn replacing_existing_keys_costs_about_what_adding_new_ones_does() {
    let (_, database) = fresh("replace_existing");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE k(id INTEGER PRIMARY KEY, n INT); \
             CREATE TABLE fresh_0(id INTEGER PRIMARY KEY, n INT); \
             CREATE TABLE fresh_1(id INTEGER PRIMARY KEY, n INT); \
             CREATE TABLE fresh_2(id INTEGER PRIMARY KEY, n INT); \
             INSERT INTO k SELECT value, 0 FROM generate_series(1, 60000);",
        )
        .expect("the tables are built");
    // The best of three runs of each, so a busy machine does not decide the comparison.
    let mut existing = Duration::MAX;
    let mut new_keys = Duration::MAX;
    for round in 0..3 {
        let started = Instant::now();
        session
            .execute_batch(&format!(
                "INSERT OR REPLACE INTO k SELECT value, {} FROM generate_series(1, 60000);",
                5 + round
            ))
            .expect("the replaces run");
        existing = existing.min(started.elapsed());
        let started = Instant::now();
        session
            .execute_batch(&format!(
                "INSERT OR REPLACE INTO fresh_{round} SELECT value, 5 FROM generate_series(1, 60000);"
            ))
            .expect("the inserts run");
        new_keys = new_keys.min(started.elapsed());
    }
    assert_eq!(
        integers(&database, "SELECT count(*), sum(n), min(n) FROM k"),
        vec![60_000, 420_000, 7]
    );
    println!("60,000 replaces onto existing keys: {existing:?}, onto new keys: {new_keys:?}");
    assert!(
        existing < new_keys * 3 + Duration::from_millis(50),
        "60,000 replaces onto existing keys took {existing:?}, against {new_keys:?} onto new keys"
    );
}

/// `INSERT OR REPLACE` keeps every index in step with the row it replaces.
#[test]
fn replacing_a_row_moves_its_index_entries() {
    let (_, database) = fresh("replace_indexes");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE r(id INTEGER PRIMARY KEY, a INT, b TEXT); \
             CREATE INDEX r_a ON r(a); CREATE INDEX r_b ON r(b); \
             INSERT INTO r VALUES (1, 10, 'x'), (2, 20, 'y'); \
             INSERT OR REPLACE INTO r VALUES (1, 11, 'z'), (3, 30, 'w');",
        )
        .expect("the statements run");
    assert_eq!(
        integers(
            &database,
            "SELECT (SELECT count(*) FROM r WHERE a = 10), (SELECT count(*) FROM r WHERE a = 11), \
             (SELECT count(*) FROM r WHERE b = 'x'), (SELECT count(*) FROM r WHERE b = 'z'), \
             (SELECT count(*) FROM r)"
        ),
        vec![0, 1, 0, 1, 3]
    );
}

/// `count(DISTINCT x)` over integers is not slower than the same count over their text.
///
/// The integers go in a set of integers. The text of the same values goes through the encoded
/// key set the integers used to share: 15 ms against 100 ms for 500,000 rows.
#[test]
fn counting_distinct_integers_is_faster_than_counting_distinct_text() {
    let (_, database) = fresh("count_distinct");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE c(id INTEGER PRIMARY KEY, post_id INT); \
             INSERT INTO c SELECT value, (value * 7919) % 200000 FROM generate_series(1, 400000);",
        )
        .expect("the table is built");
    let (_, _) = timed(&database, "SELECT count(DISTINCT post_id) FROM c");
    let (integers_answer, integer_time) = timed(&database, "SELECT count(DISTINCT post_id) FROM c");
    let (text_answer, text_time) = timed(&database, "SELECT count(DISTINCT post_id || '') FROM c");
    assert_eq!(integers_answer, vec![200_000]);
    assert_eq!(text_answer, vec![200_000]);
    println!("count(DISTINCT) of integers: {integer_time:?}, of their text: {text_time:?}");
    assert!(
        integer_time < text_time,
        "count(DISTINCT) of integers took {integer_time:?}, no less than {text_time:?} for their text"
    );
}

/// `count(DISTINCT x)` treats an integer and a real with the same value as one value.
#[test]
fn count_distinct_merges_an_integer_with_the_real_of_the_same_value() {
    let (_, database) = fresh("count_distinct_mixed");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE m(v); \
             INSERT INTO m VALUES (1), (2), (2), (3.0), (1.0), (4.5), ('2'), (NULL);",
        )
        .expect("the table is built");
    assert_eq!(
        integers(&database, "SELECT count(DISTINCT v) FROM m"),
        vec![5]
    );
}

/// `VACUUM` keeps every row and index entry, and copies rows in chunks.
///
/// The copy ran one statement per row. Timing cannot tell the two apart by a wide margin, so this
/// checks the answers the rebuild must keep and bounds the time by the time a table copy takes.
#[test]
fn vacuum_keeps_every_row_and_finishes_in_time() {
    let (_, database) = fresh("vacuum_rows");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, a INT, b TEXT); \
             CREATE INDEX t_a ON t(a); \
             INSERT INTO t SELECT value, value * 7, 'row' || value FROM generate_series(1, 100000);",
        )
        .expect("the table is built");
    let started = Instant::now();
    session
        .execute_batch("CREATE TABLE copy_of_t AS SELECT * FROM t;")
        .expect("the copy runs");
    let copy = started.elapsed();
    let started = Instant::now();
    session.execute_batch("VACUUM;").expect("the vacuum runs");
    let vacuum = started.elapsed();
    assert_eq!(
        integers(
            &database,
            "SELECT (SELECT count(*) FROM t), (SELECT sum(a) FROM t), (SELECT count(*) FROM t WHERE a = 700), \
             (SELECT count(*) FROM copy_of_t)"
        ),
        vec![100_000, 35_000_350_000, 1, 100_000]
    );
    println!("copy of 100,000 rows: {copy:?}, VACUUM of two such tables: {vacuum:?}");
    assert!(
        vacuum < copy * 20 + Duration::from_millis(500),
        "VACUUM took {vacuum:?} against {copy:?} for a table copy"
    );
}

/// A line of 20,000 statements runs in time proportional to its length.
///
/// The shell cut the line statement by statement and copied everything after each statement to
/// show in an error message, so the copies added up to the square of the line's length: 3.2 s
/// against 0.05 s when the statements are on separate lines.
#[test]
fn twenty_thousand_statements_on_one_line_finish_in_time() {
    let shell = cliproc::program("inillucent-shell");
    let mut script = String::from("CREATE TABLE one(a INTEGER, b TEXT);\nBEGIN;");
    for row in 0..20_000 {
        script.push_str(&format!("INSERT INTO one VALUES ({row}, 'row {row}');"));
    }
    script.push_str("COMMIT;\nSELECT count(*), sum(a), max(b) FROM one;\n");
    let started = Instant::now();
    let ran = cliproc::run_with_input(&shell, &[":memory:"], &script);
    let took = started.elapsed();
    assert_eq!(
        ran.stdout.trim(),
        "20000|199990000|row 9999",
        "{}",
        ran.said()
    );
    assert!(
        took < Duration::from_millis(1_500),
        "20,000 statements on one line took {took:?}; each one copied the rest of the line"
    );
}

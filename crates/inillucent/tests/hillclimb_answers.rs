//! Wrong answers the performance hill climb's plan found, each with the shape that found it.
//!
//! Invariant: **every case compares the engine with itself through a second
//! way of asking that does not share the suspect path**, or with a value
//! computed in the test, and names that value. The hill climb's gate compares
//! a digest with SQLite's and stops timing a workload whose digest differs;
//! these tests keep each shape it found from coming back without a failure
//! that says which rows were wrong.

use std::path::PathBuf;

use inillucent_engine::connect::{Connection, Database};
use inillucent_tree::datum::OwnedDatum;

/// Returns a fresh, empty directory for one test's files.
///
/// @param tag - what to name the directory after
fn scratch(tag: &str) -> PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "inillucent-hillclimb-{}-{tag}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// Returns every integer the query answers, in order, from its first column.
///
/// @param connection - the session
/// @param sql - the query
fn ids(connection: &Connection<'_>, sql: &str) -> Vec<i64> {
    connection
        .query(sql)
        .expect("the query runs")
        .iter()
        .map(|row| match row.first() {
            Some(OwnedDatum::Int(value)) => *value,
            other => panic!("expected an integer, got {other:?}"),
        })
        .collect()
}

/// Builds `fresh` the way an application does: 20,000 rows, 40 transactions of 500.
///
/// The group and the name arrive in no order, so the two secondary indexes
/// split in the middle and a group's entries end up across several leaves.
///
/// @param connection - the session
fn build_fresh(connection: &Connection<'_>) {
    connection
        .execute_batch(
            "CREATE TABLE fresh(id INTEGER PRIMARY KEY, grp INTEGER NOT NULL, \
             name TEXT NOT NULL, amount REAL, created INTEGER NOT NULL); \
             CREATE INDEX fresh_grp ON fresh(grp, created); \
             CREATE UNIQUE INDEX fresh_name ON fresh(name);",
        )
        .expect("the schema is created");
    let mut insert = connection
        .prepare(
            "INSERT INTO fresh(grp, name, amount, created) \
             VALUES (?1 % 97, 'user ' || ?2, (?1 % 10000) * 0.25, ?2)",
        )
        .expect("the insert prepares");
    for batch in 0..40i64 {
        connection.execute("BEGIN").expect("a transaction starts");
        for row in 0..500i64 {
            let iteration = batch * 500 + row;
            let scattered = (iteration * 1_103_515_245 + 12_345) & 0x7fff_ffff;
            insert.reset();
            insert.bind_integer(1, scattered).expect("?1 binds");
            insert
                .bind_integer(2, 100_001 + iteration)
                .expect("?2 binds");
            while insert.step().expect("the insert runs") {}
        }
        connection
            .execute("COMMIT")
            .expect("the transaction commits");
    }
}

/// `WHERE grp = ? ORDER BY created DESC LIMIT 20` over the index answers the newest twenty.
///
/// It answered twenty rows from the middle of the group for every group whose
/// entries spanned a leaf boundary, because the backward walk started at the
/// leaf where the group begins. `+grp` keeps the index out of the plan, so it
/// is the answer the scan and the sort give.
#[test]
fn newest_rows_of_a_group_that_spans_leaves() {
    let directory = scratch("newest");
    let path = directory.join("fresh.rdb");
    {
        let database = Database::open(&path).expect("the database opens");
        let connection = database.session();
        build_fresh(&connection);
        check_every_group(&connection);
        connection
            .execute("DELETE FROM fresh WHERE id % 2 = 0")
            .expect("half the rows go");
        check_every_group(&connection);
    }
    let database = Database::open(&path).expect("the database reopens");
    check_every_group(&database.session());
}

/// Compares the index's newest twenty with the scan's for every group.
///
/// @param connection - the session
fn check_every_group(connection: &Connection<'_>) {
    for group in 0..97 {
        let by_index = ids(
            connection,
            &format!("SELECT id FROM fresh WHERE grp = {group} ORDER BY created DESC LIMIT 20"),
        );
        let by_scan = ids(
            connection,
            &format!("SELECT id FROM fresh WHERE +grp = {group} ORDER BY created DESC LIMIT 20"),
        );
        assert_eq!(by_index.len(), 20, "group {group}");
        assert_eq!(by_index, by_scan, "group {group}");
        let lowest = ids(
            connection,
            &format!("SELECT id FROM fresh WHERE grp = {group} AND created <= 110000 ORDER BY created DESC LIMIT 3"),
        );
        let lowest_by_scan = ids(
            connection,
            &format!("SELECT id FROM fresh WHERE +grp = {group} AND +created <= 110000 ORDER BY created DESC LIMIT 3"),
        );
        assert_eq!(lowest, lowest_by_scan, "group {group} below a bound");
    }
}

/// An `OFFSET` that is an expression over a parameter follows each new binding.
///
/// The gate prepares once and binds a new value per run, which is what a
/// paging endpoint does. Each page must start where its own offset says.
#[test]
fn an_offset_expression_follows_each_binding() {
    let directory = scratch("offset");
    let database = Database::open(directory.join("offset.rdb")).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE t(id INTEGER PRIMARY KEY, label TEXT); \
             WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 5000) \
             INSERT INTO t SELECT i, 'row ' || i FROM c;",
        )
        .expect("the table is filled");
    let mut page = connection
        .prepare("SELECT id, label FROM t ORDER BY id LIMIT 20 OFFSET (?1 % 250) * 20")
        .expect("the page prepares");
    for bound in [7i64, 3, 249, 0, 1001] {
        page.reset();
        page.bind_integer(1, bound).expect("?1 binds");
        let mut seen = Vec::new();
        while page.step().expect("the page runs") {
            match page.row().first() {
                Some(OwnedDatum::Int(id)) => seen.push(*id),
                other => panic!("expected an id, got {other:?}"),
            }
        }
        let first = (bound % 250) * 20 + 1;
        let wanted: Vec<i64> = (first..first + 20).collect();
        assert_eq!(seen, wanted, "offset for ?1 = {bound}");
    }
}

/// Returns the single integer, or NULL as `None`, a one cell query answers.
///
/// @param connection - the session
/// @param sql - the query
fn one(connection: &Connection<'_>, sql: &str) -> Option<i64> {
    match connection.query(sql).expect("the query runs").as_slice() {
        [row] => match row.first() {
            Some(OwnedDatum::Int(value)) => Some(*value),
            Some(OwnedDatum::Null) => None,
            other => panic!("expected an integer or NULL, got {other:?}"),
        },
        other => panic!("expected one row, got {}", other.len()),
    }
}

/// A long `IN` list answers what a chain of `=` comparisons answers, NULLs included.
///
/// Lists of eight entries or more are searched as a sorted set. Each case is
/// asked twice: through the long list, and through the same comparisons
/// written as `OR`, which never reaches the set. NULL in the list makes a
/// non-match NULL, an integer matches the real with its value, a big integer
/// does not match its rounded real, and the column's affinity and collation
/// apply.
#[test]
fn a_long_in_list_answers_what_the_comparisons_answer() {
    let directory = scratch("inset");
    let database = Database::open(directory.join("inset.rdb")).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE v(id INTEGER PRIMARY KEY, n INTEGER, r REAL, t TEXT COLLATE NOCASE, x); \
             INSERT INTO v VALUES (1, 1, 1.0, 'Apple', 1), (2, 9007199254740993, 2.5, 'pear', '7'), \
             (3, NULL, NULL, NULL, NULL), (4, 40, 40.0, 'FIG', 40.0), (5, -0, -0.0, 'kiwi', x'07');",
        )
        .expect("the table is filled");
    let lists = [
        "1, 2, 3, 4, 5, 6, 7, 9007199254740992.0, 41",
        "1.0, 2.5, 'APPLE', 'Fig', 7, 70, 700, 7000, 0",
        "NULL, 10, 20, 30, 40, 50, 60, 70, 80",
        "'7', 8, 9, 10, 11, 12, 13, 14, x'07'",
        "-0.0, 1e300, 'kiwi', 'b', 'c', 'd', 'e', 'f', 'g'",
    ];
    for list in lists {
        let entries: Vec<&str> = list.split(", ").collect();
        for column in ["n", "r", "t", "x", "id"] {
            for negated in ["", "NOT "] {
                let ors: Vec<String> = entries
                    .iter()
                    .map(|entry| format!("{column} = {entry}"))
                    .collect();
                for id in 1..=5 {
                    let by_set = one(
                        &connection,
                        &format!("SELECT {column} {negated}IN ({list}) FROM v WHERE id = {id}"),
                    );
                    let any = format!("({})", ors.join(" OR "));
                    let by_or = one(
                        &connection,
                        &format!(
                            "SELECT CASE WHEN {any} THEN {yes} WHEN {any} IS NULL THEN NULL ELSE {no} END \
                             FROM v WHERE id = {id}",
                            yes = if negated.is_empty() { 1 } else { 0 },
                            no = if negated.is_empty() { 0 } else { 1 },
                        ),
                    );
                    assert_eq!(by_set, by_or, "{column} {negated}IN ({list}) on row {id}");
                }
            }
        }
    }
    assert_eq!(
        one(
            &connection,
            "SELECT count(*) FROM v WHERE id NOT IN (SELECT n FROM v WHERE id < 3 UNION ALL SELECT 10 UNION ALL SELECT 11 UNION ALL SELECT 12 UNION ALL SELECT 13 UNION ALL SELECT 14 UNION ALL SELECT 15 UNION ALL SELECT 16)"
        ),
        Some(4),
        "a folded subquery with no NULL keeps every row it does not hold"
    );
    assert_eq!(
        one(
            &connection,
            "SELECT count(*) FROM v WHERE id NOT IN (SELECT n FROM v UNION ALL SELECT 10 UNION ALL SELECT 11 UNION ALL SELECT 12 UNION ALL SELECT 13 UNION ALL SELECT 14 UNION ALL SELECT 15 UNION ALL SELECT 16)"
        ),
        Some(0),
        "a folded subquery holding a NULL keeps no row"
    );
}

/// A rowid the engine chooses is one past the largest live rowid, after every kind of delete.
///
/// The largest rowid is read from two places in the rightmost leaf since the
/// hill climb: the last live row of the sorted region and the largest key in
/// the delta area. Each step below empties or tombstones one of them, and the
/// next insert must still take `max(id) + 1`, which is SQLite's rule for a
/// table that is not `AUTOINCREMENT`. A reopen in the middle puts rows that
/// were in the delta area into the sorted region.
#[test]
fn a_chosen_rowid_follows_the_largest_live_one() {
    let directory = scratch("rowid");
    let path = directory.join("rowid.rdb");
    let steps = [
        "DELETE FROM t WHERE id > (SELECT max(id) - 3 FROM t)",
        "DELETE FROM t WHERE id = (SELECT max(id) FROM t)",
        "UPDATE t SET note = note || '!' WHERE id = (SELECT max(id) FROM t)",
        "DELETE FROM t WHERE id > 2000",
        "DELETE FROM t WHERE id > 10",
    ];
    {
        let database = Database::open(&path).expect("the database opens");
        let connection = database.session();
        connection
            .execute_batch(
                "CREATE TABLE t(id INTEGER PRIMARY KEY, note TEXT); \
                 WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 3000) \
                 INSERT INTO t(note) SELECT 'row ' || i FROM c;",
            )
            .expect("the table is filled");
        for step in &steps[..2] {
            insert_after(&connection, step);
        }
    }
    let database = Database::open(&path).expect("the database reopens");
    let connection = database.session();
    for step in &steps[2..] {
        insert_after(&connection, step);
    }
}

/// Runs one change, then inserts a row without a rowid and checks the rowid it got.
///
/// @param connection - the session
/// @param step - the change to make first
fn insert_after(connection: &Connection<'_>, step: &str) {
    connection.execute(step).expect("the step runs");
    let largest = one(connection, "SELECT max(id) FROM t").unwrap_or(0);
    connection
        .execute("INSERT INTO t(note) VALUES ('next')")
        .expect("the insert runs");
    assert_eq!(
        one(connection, "SELECT last_insert_rowid()"),
        Some(largest + 1),
        "after `{step}`"
    );
}

/// An `OR` read as a union of seeks answers what a scan answers.
///
/// Each query is asked twice: as written, which the planner now answers with
/// one seek per arm when every arm can seek, and with `NOT INDEXED`, which
/// takes every index away, so it scans. The cases cover a rowid arm and
/// an index arm, two indexes, a row both arms reach, an arm with a second
/// conjunct the seek does not use, an `IN` arm, a NULL parameter, text against
/// an integer column, and a partial index.
#[test]
fn an_or_of_seeks_answers_what_a_scan_answers() {
    let directory = scratch("or-union");
    let database = Database::open(directory.join("or.rdb")).expect("the database opens");
    let connection = database.session();
    connection
        .execute_batch(
            "CREATE TABLE m(id INTEGER PRIMARY KEY, k INTEGER, c INTEGER, t TEXT); \
             CREATE INDEX m_k ON m(k); CREATE INDEX m_c ON m(c, k); \
             CREATE INDEX m_t ON m(t) WHERE t IS NOT NULL; \
             WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < 3000) \
             INSERT INTO m SELECT i, (i * 37) % 1000, i % 50, CASE WHEN i % 3 = 0 THEN NULL ELSE 'v' || (i % 200) END FROM s;",
        )
        .expect("the table is filled");
    let cases = [
        "k = 370 OR id = 10",
        "k = 370 OR c = 7",
        "k = 370 OR id = 10 OR c = 7",
        "(c = 7 AND k > 500) OR id = 3",
        "k IN (1, 2, 370) OR id BETWEEN 5 AND 9",
        "k = NULL OR id = 4",
        "k = '370' OR id = '11'",
        "t = 'v5' OR k = 5",
        "id = 1 OR id = 1",
    ];
    for clause in cases {
        let by_seek = ids(
            &connection,
            &format!("SELECT id FROM m WHERE {clause} ORDER BY id"),
        );
        let by_scan = ids(
            &connection,
            &format!("SELECT id FROM m NOT INDEXED WHERE {clause} ORDER BY id"),
        );
        assert_eq!(by_seek, by_scan, "WHERE {clause}");
        let counted = one(
            &connection,
            &format!("SELECT count(*) FROM m WHERE {clause}"),
        );
        assert_eq!(
            counted,
            Some(by_scan.len() as i64),
            "count(*) WHERE {clause}"
        );
    }
    let plan = connection
        .query("EXPLAIN QUERY PLAN SELECT id FROM m WHERE k = 370 OR id = 10")
        .expect("the plan is described");
    let text: Vec<String> = plan
        .iter()
        .flatten()
        .filter_map(|value| match value {
            OwnedDatum::Text(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
            _ => None,
        })
        .collect();
    assert!(
        text.iter().any(|line| line == "MULTI-INDEX OR"),
        "the plan is a union of seeks: {text:?}"
    );
}

/// Deleting many rows, which now reuses the leaf the last delete used, leaves every tree consistent.
///
/// A table with two indexes over many leaves loses every other row, then a
/// range, then a scattered set. After each step the count, a read through
/// each index, and `integrity_check` agree, and they still agree after a
/// reopen.
#[test]
fn many_deletes_leave_every_tree_consistent() {
    let directory = scratch("deletes");
    let path = directory.join("deletes.rdb");
    let steps = [
        "DELETE FROM d WHERE id % 2 = 0",
        "DELETE FROM d WHERE id BETWEEN 4000 AND 12000",
        "DELETE FROM d WHERE id % 7 = 3",
    ];
    {
        let database = Database::open(&path).expect("the database opens");
        let connection = database.session();
        connection
            .execute_batch(
                "CREATE TABLE d(id INTEGER PRIMARY KEY, k INTEGER, name TEXT); \
                 CREATE INDEX d_k ON d(k); CREATE UNIQUE INDEX d_name ON d(name); \
                 WITH RECURSIVE s(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM s WHERE i < 20000) \
                 INSERT INTO d SELECT i, (i * 7919) % 20011, 'name ' || ((i * 104729) % 1000003) FROM s;",
            )
            .expect("the table is filled");
        for step in steps {
            connection.execute(step).expect("the delete runs");
            check_deleted(&connection, step);
        }
    }
    let database = Database::open(&path).expect("the database reopens");
    check_deleted(&database.session(), "after the reopen");
}

/// Checks the count, both indexes and the integrity check agree on what is left.
///
/// @param connection - the session
/// @param step - what was just done, for the message
fn check_deleted(connection: &Connection<'_>, step: &str) {
    let count = one(connection, "SELECT count(*) FROM d");
    let by_k = one(connection, "SELECT count(*) FROM d WHERE k >= 0");
    let by_name = one(connection, "SELECT count(*) FROM d WHERE name >= ''");
    assert_eq!(count, by_k, "the k index after `{step}`");
    assert_eq!(count, by_name, "the name index after `{step}`");
    let checked = connection
        .query("PRAGMA integrity_check")
        .expect("the check runs");
    assert_eq!(
        checked,
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]],
        "integrity after `{step}`"
    );
}

/// Rows an `INSERT` added are taken out again by a rollback and by a statement that fails part way.
///
/// The undo record of an inserted row holds its rowid as an integer since the
/// hill climb, where it held a vector of one value. Rolling back reads that
/// record to delete the row, so both ways a statement's inserts are undone are
/// checked, and the table is read after a reopen as well.
#[test]
fn inserted_rows_are_undone() {
    let directory = scratch("undo-insert");
    let path = directory.join("undo.rdb");
    {
        let database = Database::open(&path).expect("the database opens");
        let connection = database.session();
        connection
            .execute_batch(
                "CREATE TABLE u(id INTEGER PRIMARY KEY, v INTEGER CHECK (v < 5000)); \
                 INSERT INTO u(v) VALUES (1), (2), (3);",
            )
            .expect("the table is filled");
        connection.execute("BEGIN").expect("a transaction starts");
        connection
            .execute(
                "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 3000) \
                 INSERT INTO u(v) SELECT i FROM c",
            )
            .expect("the insert runs");
        connection
            .execute("ROLLBACK")
            .expect("the transaction rolls back");
        assert_eq!(one(&connection, "SELECT count(*) FROM u"), Some(3));
        let failed = connection.execute(
            "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < 6000) \
             INSERT INTO u(v) SELECT i FROM c",
        );
        assert!(failed.is_err(), "the CHECK fails at the 5,000th row");
        assert_eq!(one(&connection, "SELECT count(*) FROM u"), Some(3));
        assert_eq!(one(&connection, "SELECT max(id) FROM u"), Some(3));
    }
    let database = Database::open(&path).expect("the database reopens");
    let connection = database.session();
    assert_eq!(one(&connection, "SELECT count(*) FROM u"), Some(3));
    connection
        .execute("INSERT INTO u(v) VALUES (4)")
        .expect("an insert after the rollbacks runs");
    assert_eq!(one(&connection, "SELECT last_insert_rowid()"), Some(4));
}

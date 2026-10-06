//! What the second round of the usage hill climb changed about statements and
//! opens, checked through the engine's public connection.
//!
//! Invariant: **each shortcut answers exactly what the long way answered.**
//! Task-2191 made four of them: an `INSERT ... VALUES` of plain literals is
//! compiled once as `?1`, `?2` and run with the literals bound; an open stops
//! after the file's own checks and finishes at the first statement; a tree
//! remembers its largest rowid; and recovery hands its log segment to the
//! writer. The first three are checked here against the answers the long way
//! gives. The fourth changes which handle a write goes through and nothing a
//! statement can see, and the durability tier covers what it writes.

use std::path::PathBuf;

use inillucent_compat::workspace_root;
use inillucent_engine::connect::{Connection, Database};
use inillucent_engine::OwnedDatum;

/// Returns a fresh directory for one case and the database path inside it.
///
/// @param name - the case's name, which is also its directory
fn fresh(name: &str) -> PathBuf {
    let directory = workspace_root()
        .join("_agent_output/usage-paths")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("the case's directory was not made: {error}"));
    directory.join("db.rdb")
}

/// Runs a query and returns its rows.
///
/// @param session - the connection
/// @param sql - the statement
fn rows(session: &Connection<'_>, sql: &str) -> Vec<Vec<OwnedDatum>> {
    session
        .query(sql)
        .unwrap_or_else(|error| panic!("{sql} failed: {error}"))
}

/// Statements that differ only in their literal values compile once, and the
/// values they store are the ones the literals denote.
#[test]
fn inserts_that_differ_only_in_their_values_share_one_plan() {
    let path = fresh("one-plan");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE t (a TEXT, b INTEGER, c REAL, d)")
        .expect("the table is made");
    let before = session.compiled_statement_count().expect("a count");
    for n in 0..50 {
        session
            .execute(&format!(
                "INSERT INTO t VALUES ('row {n}', {n}, {n}.5, -{n})"
            ))
            .expect("the insert runs");
    }
    let compiled = session.compiled_statement_count().expect("a count") - before;
    assert_eq!(
        compiled, 1,
        "fifty inserts of one shape compiled {compiled} times"
    );
    // The values the binder would have given each literal, sign folded in,
    // quotes undoubled, and the extremes converted as the binder converts them.
    session
        .execute("INSERT INTO t VALUES ('it''s', -9223372036854775808, 9223372036854775808, 0x10)")
        .expect("the extremes insert");
    let last = rows(
        &session,
        "SELECT a, typeof(b), b, typeof(c), c, typeof(d), d FROM t WHERE rowid = 51",
    );
    assert_eq!(
        last,
        vec![vec![
            OwnedDatum::Text(b"it's".to_vec()),
            OwnedDatum::Text(b"integer".to_vec()),
            OwnedDatum::Int(i64::MIN),
            OwnedDatum::Text(b"real".to_vec()),
            OwnedDatum::Real(9_223_372_036_854_775_808.0),
            OwnedDatum::Text(b"integer".to_vec()),
            OwnedDatum::Int(16),
        ]]
    );
    let seventh = rows(&session, "SELECT a, b, c, d FROM t WHERE rowid = 8");
    assert_eq!(
        seventh,
        vec![vec![
            OwnedDatum::Text(b"row 7".to_vec()),
            OwnedDatum::Int(7),
            OwnedDatum::Real(7.5),
            OwnedDatum::Int(-7),
        ]]
    );
}

/// A statement the lift rewrote and then could not compile reports the error,
/// and the position, of the text the caller wrote.
#[test]
fn a_lifted_statement_reports_the_error_of_the_text_written() {
    let path = fresh("errors");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE t (a, b, c)")
        .expect("the table is made");
    let missing = session
        .execute("INSERT INTO nope VALUES (1, 'x')")
        .expect_err("a missing table is refused");
    assert!(
        missing.to_string().contains("no such table: nope"),
        "the error was {missing}"
    );
    let short = session
        .execute("INSERT INTO t VALUES (1, 2)")
        .expect_err("too few values are refused");
    assert!(
        short
            .to_string()
            .contains("table t has 3 columns but 2 values were supplied"),
        "the error was {short}"
    );
}

/// An open that runs no statement changes nothing on the disk, and a file that
/// is not a database is still refused by the open itself.
#[test]
fn an_open_that_runs_nothing_writes_nothing() {
    let path = fresh("no-statement");
    {
        let database = Database::open(&path).expect("the database opens");
        database
            .session()
            .execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1), (2), (3);")
            .expect("the table is filled");
    }
    let directory = path.parent().expect("a directory").to_path_buf();
    let listing = |directory: &PathBuf| -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(directory)
            .expect("the directory lists")
            .map(|entry| {
                let entry = entry.expect("an entry");
                let bytes = std::fs::read(entry.path()).expect("the file reads");
                (entry.file_name().to_string_lossy().into_owned(), bytes)
            })
            .collect();
        files.sort();
        files
    };
    let before = listing(&directory);
    for _ in 0..3 {
        let database = Database::open(&path).expect("the database opens again");
        drop(database);
    }
    assert!(
        listing(&directory) == before,
        "an open and a close that ran nothing changed a file"
    );
    let database = Database::open(&path).expect("the database opens");
    assert_eq!(
        rows(&database.session(), "SELECT count(*) FROM t"),
        vec![vec![OwnedDatum::Int(3)]]
    );
    drop(database);

    let garbage = fresh("not-a-database");
    std::fs::write(&garbage, vec![0x5a; 65_536]).expect("the file is written");
    assert!(
        Database::open(&garbage).is_err(),
        "a file that is not a database opened"
    );
}

/// The remembered largest rowid gives the number a fresh look at the tree
/// would: after a delete of the newest row, after a rollback, and after an
/// update that moved a row past the end.
#[test]
fn the_largest_rowid_follows_deletes_rollbacks_and_moves() {
    let path = fresh("largest-rowid");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    let newest = |session: &Connection<'_>| -> i64 {
        match rows(session, "SELECT max(rowid) FROM t")
            .first()
            .and_then(|row| row.first())
        {
            Some(OwnedDatum::Int(value)) => *value,
            other => panic!("max(rowid) answered {other:?}"),
        }
    };
    session
        .execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES ('a'); INSERT INTO t VALUES ('b'); INSERT INTO t VALUES ('c');")
        .expect("three rows");
    assert_eq!(newest(&session), 3);
    session
        .execute_batch("DELETE FROM t WHERE rowid = 3; INSERT INTO t VALUES ('d');")
        .expect("a delete and an insert");
    assert_eq!(newest(&session), 3, "the deleted number was not reused");
    session
        .execute_batch(
            "BEGIN; INSERT INTO t VALUES ('e'); INSERT INTO t VALUES ('f'); ROLLBACK; \
             INSERT INTO t VALUES ('g');",
        )
        .expect("a rollback and an insert");
    assert_eq!(
        newest(&session),
        4,
        "a rolled back row's number was skipped"
    );
    session
        .execute_batch("UPDATE t SET rowid = 100 WHERE rowid = 1; INSERT INTO t VALUES ('h');")
        .expect("a move and an insert");
    assert_eq!(
        newest(&session),
        101,
        "an insert after a moved row reused a number"
    );
    assert_eq!(
        rows(&session, "SELECT count(*) FROM t"),
        vec![vec![OwnedDatum::Int(5)]]
    );
}

/// An `INSERT` of several rows of literals writes what the same rows written
/// one statement at a time write, as one statement: a failure in any row
/// undoes them all, and a foreign key is checked when the statement ends.
#[test]
fn several_rows_of_literals_are_one_statement() {
    let path = fresh("several-rows");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE many (id INTEGER PRIMARY KEY, a TEXT, b REAL);
             CREATE TABLE one (id INTEGER PRIMARY KEY, a TEXT, b REAL);",
        )
        .expect("the tables are made");
    let values: Vec<String> = (0..300)
        .map(|n| format!("({n}, 'row {n}', {n}.25)"))
        .collect();
    let before = session.compiled_statement_count().expect("a count");
    session
        .execute(&format!("INSERT INTO many VALUES {}", values.join(", ")))
        .expect("three hundred rows in one statement");
    session
        .execute("INSERT INTO many VALUES (1000, 'x', 1.0), (1001, 'y', 2)")
        .expect("two more rows");
    let compiled = session.compiled_statement_count().expect("a count") - before;
    assert_eq!(
        compiled, 1,
        "two inserts of one row shape compiled {compiled} times"
    );
    for value in &values {
        session
            .execute(&format!("INSERT INTO one VALUES {value}"))
            .expect("one row");
    }
    session
        .execute("INSERT INTO one VALUES (1000, 'x', 1.0)")
        .expect("one row");
    session
        .execute("INSERT INTO one VALUES (1001, 'y', 2)")
        .expect("one row");
    let read = |table: &str| {
        rows(
            &session,
            &format!("SELECT id, a, typeof(b), b FROM {table} ORDER BY id"),
        )
    };
    assert_eq!(
        read("many"),
        read("one"),
        "the two ways stored different rows"
    );

    // A duplicate in the third row undoes the first two.
    let refused = session
        .execute("INSERT INTO many VALUES (5000, 'a', 1), (5001, 'b', 2), (5000, 'c', 3)")
        .expect_err("a duplicate key is refused");
    assert!(
        refused.to_string().contains("UNIQUE"),
        "the error was {refused}"
    );
    assert_eq!(
        rows(&session, "SELECT count(*) FROM many WHERE id >= 5000"),
        vec![vec![OwnedDatum::Int(0)]],
        "a refused statement left rows behind"
    );

    // A row that names a later row of the same statement passes, because the
    // key is checked when the statement ends, as SQLite checks it.
    session
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE node (id INTEGER PRIMARY KEY, parent INTEGER REFERENCES node(id));",
        )
        .expect("the table is made");
    session
        .execute("INSERT INTO node VALUES (2, 1), (1, NULL)")
        .expect("a parent written after its child in one statement");
    assert_eq!(
        rows(&session, "SELECT count(*) FROM node"),
        vec![vec![OwnedDatum::Int(2)]]
    );
}

/// Counts one tag's rows through its index and through a scan of the table,
/// which have to agree.
///
/// @param session - the connection
/// @param tag - the tag to count
fn counted_both_ways(
    session: &Connection<'_>,
    tag: &str,
) -> (Vec<Vec<OwnedDatum>>, Vec<Vec<OwnedDatum>>) {
    (
        rows(
            session,
            &format!("SELECT id FROM note WHERE tag = '{tag}' ORDER BY id"),
        ),
        rows(
            session,
            &format!("SELECT id FROM note WHERE +tag = '{tag}' ORDER BY id"),
        ),
    )
}

/// An index entry written for a reused rowid lands once, whether the old
/// entry is still in the leaf's sorted region as a tombstone or was removed.
///
/// The entry a non unique index takes for a newly placed row is written with
/// one search of the leaf, the delta directory's, because its key cannot be
/// live anywhere (`PagedTree::put_new_key`). A rowid reused after a delete is
/// the case where the leaf still holds the old key, tombstoned, and the read
/// path has to keep the new copy and drop the old one. The rows are enough to
/// fill a leaf several times, so the old entries are in the sorted region.
#[test]
fn an_index_entry_for_a_reused_rowid_is_found_once() {
    let path = fresh("reused-rowid");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE note (id INTEGER PRIMARY KEY, tag TEXT, n INTEGER);
             CREATE INDEX note_tag ON note(tag);
             CREATE INDEX note_n ON note(n);",
        )
        .expect("the schema is made");
    session.execute_batch("BEGIN").expect("begin");
    for id in 1..=6000 {
        session
            .execute(&format!(
                "INSERT INTO note VALUES ({id}, 't{}', {})",
                id % 7,
                id % 101
            ))
            .expect("a row");
    }
    session.execute_batch("COMMIT").expect("commit");

    // The highest rowid, deleted and reused with the same values.
    session
        .execute("DELETE FROM note WHERE id = 6000")
        .expect("delete");
    session
        .execute("INSERT INTO note (tag, n) VALUES ('t1', 41)")
        .expect("the rowid is reused");
    // A middle rowid, deleted and written again by number, rolled back once
    // and then kept.
    for keep in [false, true] {
        session.execute_batch("BEGIN").expect("begin");
        session
            .execute("DELETE FROM note WHERE id = 3000")
            .expect("delete");
        session
            .execute("INSERT INTO note VALUES (3000, 't4', 71)")
            .expect("the same rowid again");
        session
            .execute_batch(if keep { "COMMIT" } else { "ROLLBACK" })
            .expect("end");
    }

    for tag in ["t1", "t4", "t6"] {
        let (indexed, scanned) = counted_both_ways(&session, tag);
        assert_eq!(
            indexed, scanned,
            "the index and the table disagree on {tag}"
        );
    }
    assert_eq!(
        rows(&session, "SELECT count(*) FROM note WHERE n = 41"),
        rows(&session, "SELECT count(*) FROM note WHERE +n = 41"),
    );
    assert_eq!(
        rows(&session, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]],
        "the index no longer matches its table"
    );
}

/// A prepared `INSERT` of several rows of literals writes them every time it
/// runs, not only the first.
///
/// The first execution takes the rows the statement was prepared with, so a
/// second one lifts them out of the caller's text again (`Lifted::Rows`).
#[test]
fn a_prepared_insert_of_several_rows_runs_twice() {
    let path = fresh("prepared-rows");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    session
        .execute_batch("CREATE TABLE r (a INTEGER, b TEXT)")
        .expect("the table is made");
    let mut statement = session
        .prepare("INSERT INTO r VALUES (1, 'one'), (2, 'two'), (3, 'three')")
        .expect("prepared");
    for _ in 0..2 {
        while statement.step().expect("the insert runs") {}
        statement.reset();
    }
    drop(statement);
    assert_eq!(
        rows(
            &session,
            "SELECT a, b, count(*) FROM r GROUP BY a, b ORDER BY a"
        ),
        vec![
            vec![
                OwnedDatum::Int(1),
                OwnedDatum::Text(b"one".to_vec()),
                OwnedDatum::Int(2)
            ],
            vec![
                OwnedDatum::Int(2),
                OwnedDatum::Text(b"two".to_vec()),
                OwnedDatum::Int(2)
            ],
            vec![
                OwnedDatum::Int(3),
                OwnedDatum::Text(b"three".to_vec()),
                OwnedDatum::Int(2)
            ],
        ],
        "a second execution did not write the rows again"
    );
}

/// The rows `rows_at_once` tests write: an id, a text the indexes key on and
/// a number.
///
/// @param from - the first id
/// @param count - how many rows
fn numbered_rows(from: i64, count: i64) -> Vec<Vec<OwnedDatum>> {
    (from..from + count)
        .map(|id| {
            vec![
                OwnedDatum::Int(id),
                OwnedDatum::Text(format!("t{}", id % 13).into_bytes()),
                OwnedDatum::Int(id % 97),
            ]
        })
        .collect()
}

/// Makes the two tables `rows_at_once` tests compare, each with two indexes.
///
/// @param session - the connection
fn two_tables(session: &Connection<'_>) {
    session
        .execute_batch(
            "CREATE TABLE one (id INTEGER PRIMARY KEY, tag TEXT, n INTEGER);
             CREATE INDEX one_tag ON one(tag); CREATE INDEX one_n ON one(n);
             CREATE TABLE many (id INTEGER PRIMARY KEY, tag TEXT, n INTEGER);
             CREATE INDEX many_tag ON many(tag); CREATE INDEX many_n ON many(n);",
        )
        .expect("the tables are made");
}

/// Many rows run as one statement leave what the same rows run one at a time
/// leave, in the table, its indexes and the connection's counters.
///
/// Both an empty table, which takes the bulk build, and a table that already
/// holds rows, which takes the ordinary insert.
#[test]
fn rows_at_once_write_what_rows_one_at_a_time_write() {
    let path = fresh("rows-at-once");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    two_tables(&session);
    for (from, count) in [(1, 3000), (3001, 500)] {
        session.execute_batch("BEGIN").expect("begin");
        let mut one = session
            .prepare("INSERT INTO one VALUES (?1, ?2, ?3)")
            .expect("prepared");
        for row in numbered_rows(from, count) {
            for (nth, value) in row.into_iter().enumerate() {
                one.bind(nth as u32 + 1, value).expect("bound");
            }
            while one.step().expect("a row") {}
            one.reset();
        }
        drop(one);
        let total_before = session.total_changes().expect("total");
        let mut many = session
            .prepare("INSERT INTO many VALUES (?1, ?2, ?3)")
            .expect("prepared");
        assert!(many.can_run_rows_at_once().expect("asked"));
        let changed = many
            .run_rows_at_once(numbered_rows(from, count))
            .expect("ran");
        drop(many);
        session.execute_batch("COMMIT").expect("commit");
        assert_eq!(changed, Some(count as usize));
        assert_eq!(
            session.changes().expect("changes"),
            1,
            "changes() is the last row's"
        );
        assert_eq!(
            session.total_changes().expect("total") - total_before,
            count,
            "total_changes() counts every row"
        );
        assert_eq!(
            session.last_insert_rowid().expect("rowid"),
            from + count - 1
        );
    }
    for query in [
        "SELECT id, tag, n FROM {} ORDER BY id",
        "SELECT tag, count(*) FROM {} WHERE tag >= 't5' GROUP BY tag",
        "SELECT id FROM {} WHERE n = 42 ORDER BY id",
    ] {
        assert_eq!(
            rows(&session, &query.replace("{}", "one")),
            rows(&session, &query.replace("{}", "many")),
            "{query}"
        );
    }
    assert_eq!(
        rows(&session, "PRAGMA integrity_check"),
        vec![vec![OwnedDatum::Text(b"ok".to_vec())]]
    );
}

/// Many rows that fail as one statement write nothing and leave the
/// transaction open, so the caller can run them one at a time.
#[test]
fn rows_at_once_that_fail_write_nothing() {
    let path = fresh("rows-at-once-fail");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    two_tables(&session);
    session
        .execute("INSERT INTO many VALUES (50, 'x', 1)")
        .expect("a row in the way");
    session.execute_batch("BEGIN").expect("begin");
    let mut many = session
        .prepare("INSERT INTO many VALUES (?1, ?2, ?3)")
        .expect("prepared");
    assert!(many.can_run_rows_at_once().expect("asked"));
    // Rows 1 to 100, and row 50 is taken.
    assert_eq!(
        many.run_rows_at_once(numbered_rows(1, 100)).expect("ran"),
        None
    );
    drop(many);
    assert!(
        !session.autocommit().expect("asked"),
        "the transaction was ended"
    );
    assert_eq!(
        rows(&session, "SELECT count(*) FROM many"),
        vec![vec![OwnedDatum::Int(1)]],
        "a failed statement left rows behind"
    );
    session.execute_batch("COMMIT").expect("commit");
}

/// Only a single row `INSERT` of its parameters, inside a transaction, with
/// nothing that acts per statement, runs its rows at once.
#[test]
fn rows_at_once_are_offered_only_where_they_are_the_same() {
    let path = fresh("rows-at-once-offered");
    let database = Database::open(&path).expect("the database opens");
    let session = database.session();
    two_tables(&session);
    session
        .execute_batch(
            "CREATE TABLE logged (id INTEGER PRIMARY KEY, tag TEXT, n INTEGER);
             CREATE TABLE seen (id INTEGER);
             CREATE TRIGGER logged_seen AFTER INSERT ON logged BEGIN INSERT INTO seen VALUES (new.id); END;",
        )
        .expect("the trigger is made");
    let offered = |sql: &str| {
        session
            .prepare(sql)
            .expect("prepared")
            .can_run_rows_at_once()
            .expect("asked")
    };
    assert!(
        !offered("INSERT INTO many VALUES (?1, ?2, ?3)"),
        "outside a transaction"
    );
    session.execute_batch("BEGIN").expect("begin");
    assert!(offered("INSERT INTO many VALUES (?1, ?2, ?3)"));
    assert!(offered("INSERT INTO many (id, tag, n) VALUES (?1, ?2, ?3)"));
    for refused in [
        "INSERT INTO many VALUES (?1, upper(?2), ?3)",
        "INSERT INTO many VALUES (?2, ?1, ?3)",
        "INSERT OR IGNORE INTO many VALUES (?1, ?2, ?3)",
        "INSERT OR REPLACE INTO many VALUES (?1, ?2, ?3)",
        "INSERT INTO many VALUES (?1, ?2, ?3) RETURNING id",
        "INSERT INTO many VALUES (?1, ?2, ?3), (?4, ?5, ?6)",
        "INSERT INTO logged VALUES (?1, ?2, ?3)",
        "INSERT INTO many SELECT ?1, ?2, ?3",
    ] {
        assert!(!offered(refused), "{refused} was offered");
    }
    session.execute_batch("COMMIT").expect("commit");
}

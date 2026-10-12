//! A table that is written and trimmed over and over stays the size of what it
//! holds.
//!
//! Invariant: **the pages a delete empties go back to the free map and are
//! used again by later inserts, so a table trimmed to the same row count after
//! every round of inserts does not grow round after round.**
//!
//! ## Why this exists
//!
//! A queue, a log kept to its newest rows and a session table all insert at the
//! high end of the key and delete from the low end. The delete empties the
//! leftmost leaf while its right sibling is full, and the merge that would free
//! the empty leaf refused every time: it asked whether the full sibling had room
//! to take more rows, and then packed the two at the fill a compaction leaves
//! room for, which a full sibling exceeds. So no page of such a table was ever
//! freed. With 100 live rows the file grew by every round of inserts it ever
//! took: after ten rounds of 2,000 rows SQLite's file was 1.09 MB and this
//! engine's 10.9 MB, and `PRAGMA freelist_count` stayed at 0. Found by the bug
//! hunt's SQLite fix mining.

use inillucent_compat::workspace_root;
use inillucent_driver::{Database, Value};

/// Returns one integer a query answers.
///
/// @param database - the database
/// @param sql - a query returning one integer
fn integer(database: &Database, sql: &str) -> i64 {
    let rows = database
        .session()
        .query_all(sql, &[])
        .expect("the query runs");
    match rows.rows.first().and_then(|row| row.first()) {
        Some(Value::Integer(n)) => *n,
        other => panic!("{sql} answered {other:?}"),
    }
}

/// Runs rounds of inserting 2,000 rows and deleting all but the newest 100,
/// and returns the page count after the second round and after the last.
///
/// **After the second round, because that is the first to insert on top of
/// rows a trim kept.** The first round starts from an empty table, so the
/// second one needs a few more pages for the hundred rows still there, and
/// every round after it is the same as the second.
///
/// @param name - the case's name, which is also its directory's
/// @param page_size - the page size the database is created with
/// @param rounds - how many rounds to run
fn queue_rounds(name: &str, page_size: usize, rounds: usize) -> (i64, i64) {
    let directory = workspace_root()
        .join("_agent_output/space-reuse")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let path = directory.join("queue.rdb");
    // The page size is fixed when the file is made, and `PRAGMA page_size`
    // does not change it, so the file is made at the size first.
    drop(
        inillucent_engine::connect::Database::open_at(&path, page_size, 256)
            .expect("the file is made at the page size"),
    );
    let database = Database::open(&path).expect("the database opens");
    database
        .session()
        .execute_batch(
            "CREATE TABLE q(id INTEGER PRIMARY KEY, queued INTEGER, payload TEXT); \
             CREATE INDEX q_queued ON q(queued);",
        )
        .expect("the table is made");
    assert_eq!(integer(&database, "PRAGMA page_size"), page_size as i64);
    let mut first = 0;
    for round in 0..rounds {
        database
            .session()
            .execute_batch(
                "INSERT INTO q(queued, payload) \
                 SELECT (SELECT coalesce(max(queued), 0) FROM q) + value, \
                 printf('%.*c', 400, 'x') FROM generate_series(1, 2000); \
                 DELETE FROM q WHERE id <= (SELECT max(id) FROM q) - 100;",
            )
            .expect("a round runs");
        if round == 1 {
            first = integer(&database, "PRAGMA page_count");
        }
    }
    assert_eq!(integer(&database, "SELECT count(*) FROM q"), 100);
    let last = integer(&database, "PRAGMA page_count");
    let check = database
        .session()
        .query_all("PRAGMA integrity_check", &[])
        .expect("the check runs");
    assert_eq!(
        check.rows,
        vec![vec![Value::Text("ok".to_string())]],
        "the trimmed table failed its integrity check"
    );
    drop(database);
    let reopened = Database::open(&path).expect("the database reopens");
    assert_eq!(integer(&reopened, "SELECT count(*) FROM q"), 100);
    assert!(
        integer(&reopened, "PRAGMA freelist_count") > 0,
        "the pages the deletes emptied are not on the free map"
    );
    (first, last)
}

/// At the default page size, ten rounds leave the file the size the second
/// round made it.
#[test]
fn a_table_trimmed_from_the_front_reuses_its_pages() {
    let (first, last) = queue_rounds("default-pages", 32_768, 10);
    assert!(
        last <= first + 4,
        "the file grew from {first} pages after the second round to {last} after the tenth"
    );
}

/// At 4,096 byte pages, where the tree has an interior level below the root,
/// the file stays flat over 40 rounds, as it does at the default page.
///
/// A merge happens under one parent, so when every leaf under a parent had
/// emptied, the last one stayed, with its parent: about 6 pages a round. An
/// interior page left with one child now hands that child to a neighbour and is
/// freed, so nothing is left behind.
#[test]
fn a_table_trimmed_from_the_front_reuses_its_pages_at_small_pages() {
    let (first, last) = queue_rounds("small-pages", 4_096, 40);
    assert!(
        last <= first + 4,
        "the file grew from {first} pages after the second round to {last} after the fortieth"
    );
}

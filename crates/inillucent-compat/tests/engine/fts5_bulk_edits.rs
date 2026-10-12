//! Statements that change many rows of an FTS5 table.
//!
//! Invariant: **the work a statement does grows with the rows it changes, and a
//! statement that fails leaves none of its rows.** Both were false. An `UPDATE`
//! or `DELETE` of every row took time that grew as the square of the row count,
//! because a word that is in every row has one doclist and each row decoded and
//! encoded all of it again. A multi row `INSERT` or `UPDATE` that failed on a
//! later row kept the earlier ones, in the table and in the index.
//!
//! The first is checked by counting the doclist entries decoded, which is a
//! number that does not depend on how fast the machine is. The second is
//! checked by what a query answers, and again after the file is closed and
//! opened, because the rows a failed statement left were in the log.

use inillucent_compat::facade::{Connection, Database};
use inillucent_value::Value;

/// Where this suite's scratch databases live.
const AREA: &str = "fts5-bulk-edits";

/// How many rows the work count cases change.
const ROWS: usize = 3_000;

/// Returns a database file of this test's own, under the gitignored root.
fn scratch() -> std::path::PathBuf {
    let root = inillucent_compat::workspace_root()
        .join("_agent_output")
        .join(AREA);
    let _ = std::fs::create_dir_all(&root);
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = root.join(format!("{}-{serial}.rdb", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

/// Returns the first column of every row a query answers, as text.
///
/// @param connection - the connection to ask
/// @param sql - the query
fn column(connection: &Connection, sql: &str) -> Vec<String> {
    connection
        .query(sql)
        .expect("the query runs")
        .iter()
        .map(|row| match row.first() {
            Some(Value::Integer(number)) => number.to_string(),
            Some(Value::Text(text)) => String::from_utf8_lossy(text.raw()).into_owned(),
            Some(other) => format!("{other:?}"),
            None => String::new(),
        })
        .collect()
}

/// Fills a table with `rows` rows that all contain the word `body`.
///
/// @param connection - a connection to an empty database
/// @param rows - how many rows
fn fill(connection: &Connection, rows: usize) {
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a, b)")
        .expect("creates the table");
    connection
        .execute(&format!(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<{rows}) \
             INSERT INTO ft(rowid, a, b) SELECT i, 'title'||i, 'body word'||i||' uniq'||i FROM n"
        ))
        .expect("fills the table");
}

/// Runs a statement and returns how many doclist entries it decoded.
///
/// @param connection - the connection to run it on
/// @param sql - the statement
fn decoded_by(connection: &Connection, sql: &str) -> u64 {
    inillucent_ext::vtab::fts5::reset_doclist_entries_decoded();
    connection.execute(sql).expect("the statement runs");
    inillucent_ext::vtab::fts5::doclist_entries_decoded()
}

/// An `UPDATE` of every row decodes each doclist a bounded number of times.
///
/// The word `body` is in all `ROWS` rows. Taking a row out of its doclist
/// decoded the whole list, so changing every row decoded about `ROWS * ROWS / 2`
/// entries: 4.5 million here, and 32 million at 8,000 rows, which took 7.7 s
/// where SQLite takes 0.04 s. The bound is twenty entries a row, against the
/// five or so a linear statement needs.
#[test]
fn updating_every_row_decodes_a_bounded_number_of_entries() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    fill(&connection, ROWS);

    let decoded = decoded_by(&connection, "UPDATE ft SET b = b||' edited'||rowid");

    let bound = 20 * ROWS as u64;
    assert!(
        decoded <= bound,
        "an UPDATE of {ROWS} rows decoded {decoded} doclist entries; a statement whose work grows \
         with the rows decodes about {bound} at most, and one that decodes a whole doclist per row \
         decodes about {}",
        ROWS * ROWS / 2
    );
    assert_eq!(
        column(
            &connection,
            "SELECT count(*) FROM ft WHERE ft MATCH 'edited1'"
        ),
        vec!["1"],
        "the row whose rowid is 1 was edited, and no other row has that word"
    );
    assert_eq!(
        column(&connection, "SELECT count(*) FROM ft WHERE ft MATCH 'body'"),
        vec![ROWS.to_string()],
        "every row still has the word the update left alone"
    );
}

/// A `DELETE` of every row decodes each doclist a bounded number of times.
#[test]
fn deleting_every_row_decodes_a_bounded_number_of_entries() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    fill(&connection, ROWS);

    let decoded = decoded_by(&connection, "DELETE FROM ft");

    let bound = 20 * ROWS as u64;
    assert!(
        decoded <= bound,
        "a DELETE of {ROWS} rows decoded {decoded} doclist entries, against a bound of {bound}"
    );
    assert_eq!(
        column(&connection, "SELECT count(*) FROM ft WHERE ft MATCH 'body'"),
        vec!["0"],
        "no row is left to match"
    );
    assert_eq!(
        column(&connection, "SELECT count(*) FROM ft_idx"),
        vec!["0"],
        "every term lost its last row, so the dictionary is empty"
    );
}

/// Updates through the triggers of an external content table are linear too.
///
/// This is the form an application uses: the index follows a table through
/// triggers that send the old row to the `delete` command and the new row to an
/// insert, one row at a time.
#[test]
fn external_content_triggers_decode_a_bounded_number_of_entries() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    for statement in [
        "CREATE TABLE t(id INTEGER PRIMARY KEY, a, b)",
        "CREATE VIRTUAL TABLE fx USING fts5(a, b, content='t', content_rowid='id')",
        "CREATE TRIGGER t_ai AFTER INSERT ON t BEGIN \
           INSERT INTO fx(rowid, a, b) VALUES (new.id, new.a, new.b); END",
        "CREATE TRIGGER t_au AFTER UPDATE ON t BEGIN \
           INSERT INTO fx(fx, rowid, a, b) VALUES ('delete', old.id, old.a, old.b); \
           INSERT INTO fx(rowid, a, b) VALUES (new.id, new.a, new.b); END",
    ] {
        connection.execute(statement).expect("sets up");
    }
    connection
        .execute(&format!(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<{ROWS}) \
             INSERT INTO t(id, a, b) SELECT i, 'title'||i, 'body word'||i FROM n"
        ))
        .expect("fills the table");

    let decoded = decoded_by(&connection, "UPDATE t SET b = b||' edited'||id");

    let bound = 20 * ROWS as u64;
    assert!(
        decoded <= bound,
        "an UPDATE of {ROWS} rows through triggers decoded {decoded} doclist entries, against a \
         bound of {bound}"
    );
    assert_eq!(
        column(&connection, "SELECT count(*) FROM fx WHERE fx MATCH 'body'"),
        vec![ROWS.to_string()],
        "the index follows every row through the update"
    );
}

/// A multi row insert that fails on a later row leaves nothing, outside a transaction.
#[test]
fn a_failed_multi_row_insert_leaves_no_rows() {
    let path = scratch();
    {
        let database = Database::open(&path).expect("opens");
        let connection = database.session().expect("connects");
        connection
            .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
            .expect("creates the table");
        connection
            .execute("INSERT INTO ft(rowid, a) VALUES (1, 'one')")
            .expect("the first row");

        let failed = connection
            .execute("INSERT INTO ft(rowid, a) VALUES (2, 'two'), (3, 'three'), (1, 'dup')");
        assert!(failed.is_err(), "the third row repeats rowid 1");

        assert_eq!(
            column(&connection, "SELECT rowid FROM ft ORDER BY rowid"),
            vec!["1"],
            "the rows before the failing one are not kept"
        );
        assert_eq!(
            column(
                &connection,
                "SELECT rowid FROM ft WHERE ft MATCH 'two OR three'"
            ),
            Vec::<String>::new(),
            "the index does not hold their words either"
        );
        connection
            .execute("INSERT INTO ft(ft) VALUES ('integrity-check')")
            .expect("the index and the table agree");
        connection
            .execute("INSERT INTO ft(rowid, a) VALUES (2, 'two again')")
            .expect("the rowid the failed statement used is free");
    }
    let database = Database::open(&path).expect("reopens");
    let connection = database.session().expect("connects");
    assert_eq!(
        column(&connection, "SELECT rowid FROM ft ORDER BY rowid"),
        vec!["1", "2"],
        "the file holds the rows that were kept, and not the ones that were undone"
    );
    assert_eq!(
        column(&connection, "SELECT rowid FROM ft WHERE ft MATCH 'three'"),
        Vec::<String>::new(),
        "and its index does not name a row that is not there"
    );
}

/// A multi row insert that fails inside a transaction leaves only the earlier statements.
#[test]
fn a_failed_multi_row_insert_inside_a_transaction_keeps_the_earlier_statements() {
    let path = scratch();
    {
        let database = Database::open(&path).expect("opens");
        let connection = database.session().expect("connects");
        connection
            .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
            .expect("creates the table");
        connection
            .execute("INSERT INTO ft(rowid, a) VALUES (1, 'one')")
            .expect("the first row");
        connection.execute("BEGIN").expect("begins");
        connection
            .execute("INSERT INTO ft(rowid, a) VALUES (2, 'two')")
            .expect("an earlier statement of the transaction");
        let failed = connection.execute("INSERT INTO ft(rowid, a) VALUES (3, 'three'), (1, 'dup')");
        assert!(failed.is_err(), "the second row repeats rowid 1");
        assert_eq!(
            column(&connection, "SELECT rowid FROM ft ORDER BY rowid"),
            vec!["1", "2"],
            "the transaction still holds its earlier statement and not the failed one"
        );
        assert_eq!(
            column(&connection, "SELECT rowid FROM ft WHERE ft MATCH 'two'"),
            vec!["2"],
            "the earlier statement's words are still indexed after the failure"
        );
        connection.execute("COMMIT").expect("commits");
    }
    let database = Database::open(&path).expect("reopens");
    let connection = database.session().expect("connects");
    assert_eq!(
        column(&connection, "SELECT rowid FROM ft ORDER BY rowid"),
        vec!["1", "2"],
        "the committed file holds the two statements that worked"
    );
    assert_eq!(
        column(&connection, "SELECT rowid FROM ft WHERE ft MATCH 'three'"),
        Vec::<String>::new(),
        "and nothing of the one that did not"
    );
}

/// A multi row `UPDATE` that moves rows onto a rowid that is taken changes none of them.
#[test]
fn a_failed_multi_row_update_changes_no_rows() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
        .expect("creates the table");
    connection
        .execute("INSERT INTO ft(rowid, a) VALUES (1, 'one'), (2, 'two'), (5, 'five')")
        .expect("fills the table");

    let failed = connection.execute("UPDATE ft SET rowid = rowid + 3 WHERE rowid < 3");

    assert!(failed.is_err(), "rowid 2 moves to 5, which is taken");
    assert_eq!(
        column(&connection, "SELECT rowid FROM ft ORDER BY rowid"),
        vec!["1", "2", "5"],
        "the row that could have moved was put back"
    );
    assert_eq!(
        column(&connection, "SELECT rowid FROM ft WHERE ft MATCH 'one'"),
        vec!["1"],
        "and the index still finds it under its own rowid"
    );
}

/// A token longer than a page is accepted, and the row is found by its start.
///
/// The insert used to report "a row's keys and fixed-width columns alone are
/// larger than a page" when the statement ended, after the row's text was
/// written, so the error left a row that no word found.
#[test]
fn a_row_with_a_very_long_token_is_indexed() {
    let path = scratch();
    {
        let database = Database::open(&path).expect("opens");
        let connection = database.session().expect("connects");
        connection
            .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
            .expect("creates the table");
        connection
            .execute("INSERT INTO ft(rowid, a) VALUES (1, 'fine row')")
            .expect("the first row");
        connection
            .execute("INSERT INTO ft(rowid, a) VALUES (2, 'abc'||hex(zeroblob(100000)))")
            .expect("a token of 200,000 characters is accepted");
        assert_eq!(
            column(&connection, "SELECT rowid FROM ft WHERE ft MATCH 'abc*'"),
            vec!["2"],
            "the long token is found by its first characters"
        );
        assert_eq!(
            column(&connection, "SELECT length(a) FROM ft WHERE rowid = 2"),
            vec!["200003"],
            "the text of the column is stored whole"
        );
    }
    let database = Database::open(&path).expect("reopens");
    let connection = database.session().expect("connects");
    assert_eq!(
        column(&connection, "SELECT rowid FROM ft WHERE ft MATCH 'fine'"),
        vec!["1"],
        "the other row is found after the file is opened again"
    );
    connection
        .execute("INSERT INTO ft(ft) VALUES ('integrity-check')")
        .expect("the index agrees with the content");
}

/// Two long tokens that share their first characters are one token to the index.
///
/// This is the difference from SQLite that the cap on a token's length makes,
/// stated as a test so a change to the cap shows up here.
#[test]
fn tokens_are_cut_at_the_longest_the_index_stores() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
        .expect("creates the table");
    let cap = inillucent_ext::vtab::fts5::tokenize::MAX_TOKEN_BYTES;
    let shared = "x".repeat(cap);
    connection
        .execute(&format!(
            "INSERT INTO ft(rowid, a) VALUES (1, '{shared}one'), (2, '{shared}two'), (3, 'short')"
        ))
        .expect("fills the table");
    assert_eq!(
        column(
            &connection,
            &format!("SELECT rowid FROM ft WHERE ft MATCH '{shared}one' ORDER BY rowid")
        ),
        vec!["1", "2"],
        "both tokens are the same {cap} bytes to the index"
    );
    assert_eq!(
        column(&connection, "SELECT rowid FROM ft WHERE ft MATCH 'short'"),
        vec!["3"],
        "a token under the cap is stored whole"
    );
}

/// A bulk load made of many multi row statements in one transaction writes the index about once.
///
/// The engine flushed a full text table before every multi row statement, and a flush writes the
/// whole doclist of every term the statement touched. A word that is in every row has a doclist as
/// long as the table, so statement number n wrote about n statements' worth of that word, and the
/// load took time that grew with the square of its rows: 40,000 rows took 3.9 s, 80,000 took
/// 24.8 s and 120,000 took 282 s, where SQLite takes 0.4, 0.7 and 1 s. A clock does not belong in a
/// test, so this counts the bytes of doclists the flushes wrote and compares them with the bytes
/// the finished index holds. Written once, the ratio is 1. Written at every statement it is about
/// half the number of statements.
#[test]
fn a_load_of_many_statements_writes_the_index_about_once() {
    const STATEMENTS: usize = 24;
    const PER_STATEMENT: usize = 500;

    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
        .expect("creates the table");
    inillucent_ext::vtab::fts5::reset_build_stages();
    connection.execute("BEGIN").expect("begins");
    for statement in 0..STATEMENTS {
        let rows: Vec<String> = (0..PER_STATEMENT)
            .map(|at| {
                let id = statement * PER_STATEMENT + at;
                format!(
                    "({},'the body of all rows has word{} and uniq{id}')",
                    id + 1,
                    id % 40
                )
            })
            .collect();
        connection
            .execute(&format!(
                "INSERT INTO ft(rowid, a) VALUES {}",
                rows.join(",")
            ))
            .expect("inserts");
    }
    connection.execute("COMMIT").expect("commits");

    let written = inillucent_ext::vtab::fts5::build_stages().flushed_bytes;
    let held = column(&connection, "SELECT sum(length(doclist)) FROM ft_idx");
    let held: u64 = held.first().and_then(|text| text.parse().ok()).unwrap_or(0);
    assert!(held > 0, "the index holds something");
    assert!(
        written <= 3 * held,
        "{STATEMENTS} statements wrote {written} bytes of doclists to build an index of {held}; a \
         load that writes it about once writes {held}, and one that writes it at every statement \
         writes about {}",
        held * (STATEMENTS as u64) / 2
    );
    assert_eq!(
        column(&connection, "SELECT count(*) FROM ft WHERE ft MATCH 'body'"),
        vec![(STATEMENTS * PER_STATEMENT).to_string()]
    );
    assert_eq!(
        column(
            &connection,
            "SELECT count(*) FROM ft WHERE ft MATCH 'word7'"
        ),
        vec![(STATEMENTS * PER_STATEMENT / 40).to_string()]
    );
}

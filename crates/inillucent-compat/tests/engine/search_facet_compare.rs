//! A facet column compared outside a search, and a write that changed nothing.
//!
//! Invariant: `WHERE email_id = 3` selects the same rows on an
//! `inillucent_search` facet column whether or not the query is a search, and a
//! connection that committed a statement leaves a file the next open does not
//! have to replay.
//!
//! Both were found by the same use case: a mailbox whose chunks carry their
//! email's integer id as a facet. The application deleted an email's chunks
//! with `DELETE FROM email_search WHERE email_id = 3`, which reported `0 rows
//! changed` and left the chunks searchable, because the facet stores `'3'` and
//! the engine's recheck of a constraint the module does not claim compared it
//! with the integer 3 under no affinity. And that zero row `DELETE` committed a
//! transaction the connection never folded, so every later open printed
//! `replayed the log: 1 committed transactions`.

use inillucent_compat::differential::{scratch, start_inillucent};
use inillucent_compat::rendering::datum_text as render;
use inillucent_engine::connect::{Connection, Database};

/// Where this suite's scratch databases live.
const AREA: &str = "search-facet-compare";

/// Runs a statement for its effect.
///
/// @param connection - the database
/// @param sql - one or more statements
fn exec(connection: &Connection<'_>, sql: &str) {
    connection
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
}

/// Returns the first column of every row, rendered as text.
///
/// @param connection - the database
/// @param sql - the query
fn column(connection: &Connection<'_>, sql: &str) -> Vec<String> {
    let mut statement = connection
        .prepare(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
    let mut out = Vec::new();
    while statement
        .step()
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()))
    {
        if let Some(first) = statement.row().first() {
            out.push(render(first));
        }
    }
    out
}

/// Creates an email table and a search table whose chunks name their email.
///
/// @param connection - the database
fn seed_mailbox(connection: &Connection<'_>) {
    exec(
        connection,
        "CREATE TABLE email (id INTEGER PRIMARY KEY, subject TEXT);
         INSERT INTO email VALUES (1, 'invoice'), (2, 'offer'), (3, 'dinner');
         CREATE VIRTUAL TABLE email_search USING inillucent_search(text, email_id FACET, dims = 3);
         INSERT INTO email_search (text, email_id, vector) VALUES
           ('invoice 4471 is attached', 1, '[1, 0, 0]'),
           ('we would like to extend a position', 2, '[0, 1, 0]'),
           ('dinner on sunday', 3, '[0, 0, 1]'),
           ('bring the kids to dinner', 3, '[0, 0.1, 0.9]');",
    );
}

/// An integer compared with a facet selects the rows a text value does.
#[test]
fn an_integer_selects_a_facet_outside_a_search() {
    let connection = start_inillucent(AREA, "integer");
    seed_mailbox(&connection);
    let by_text = column(
        &connection,
        "SELECT rowid FROM email_search WHERE email_id = '3' ORDER BY rowid",
    );
    assert_eq!(by_text, vec!["3".to_string(), "4".to_string()]);
    let by_integer = column(
        &connection,
        "SELECT rowid FROM email_search WHERE email_id = 3 ORDER BY rowid",
    );
    assert_eq!(
        by_integer, by_text,
        "email_id = 3 and email_id = '3' disagree"
    );
    let greater = column(
        &connection,
        "SELECT rowid FROM email_search WHERE email_id > 1 ORDER BY rowid",
    );
    assert_eq!(
        greater,
        vec!["2", "3", "4"],
        "the comparison is under text affinity"
    );
}

/// The same integer inside a search, and in a join, agree with the scan.
#[test]
fn a_search_and_a_join_agree_with_the_scan() {
    let connection = start_inillucent(AREA, "search-join");
    seed_mailbox(&connection);
    let searched = column(
        &connection,
        "SELECT rowid FROM email_search WHERE email_search MATCH 'dinner' AND email_id = 3 \
         AND k = 5 ORDER BY rowid",
    );
    assert_eq!(searched, vec!["3", "4"]);
    let joined = column(
        &connection,
        "SELECT e.subject FROM email e JOIN email_search s ON s.email_id = e.id \
         WHERE s.rowid = 2",
    );
    assert_eq!(joined, vec!["offer"]);
}

/// Deleting and updating by an integer facet value reach the rows.
#[test]
fn a_delete_by_an_integer_facet_removes_the_chunks() {
    let connection = start_inillucent(AREA, "delete");
    seed_mailbox(&connection);
    let deleted = connection
        .execute("DELETE FROM email_search WHERE email_id = 3")
        .unwrap_or_else(|error| panic!("the delete: {}", error.message()));
    assert_eq!(deleted, 2, "the delete reached both chunks of email 3");
    let left = column(
        &connection,
        "SELECT rowid FROM email_search WHERE email_search MATCH 'dinner' AND k = 5",
    );
    assert!(
        left.is_empty(),
        "a deleted chunk is still searchable: {left:?}"
    );
    exec(
        &connection,
        "UPDATE email_search SET text = 'corrected invoice 4471' WHERE email_id = 1",
    );
    let updated = column(
        &connection,
        "SELECT rowid FROM email_search WHERE email_search MATCH 'corrected' AND k = 5",
    );
    assert_eq!(updated, vec!["1"]);
}

/// A statement that changed no rows leaves a file the next open need not replay.
#[test]
fn a_write_that_changed_nothing_is_folded_at_close() {
    let path = scratch(AREA, "fold", "inillucent");
    {
        let database = Database::open(&path).expect("it opens");
        let connection = database.session();
        exec(&connection, "CREATE TABLE o (x); INSERT INTO o VALUES (1);");
    }
    for statement in ["DELETE FROM o WHERE x = 5", "UPDATE o SET x = 2 WHERE 0"] {
        {
            let database = Database::open(&path).expect("it reopens");
            let connection = database.session();
            exec(&connection, statement);
        }
        let database = Database::open(&path).expect("it reopens");
        let report = database.recovery_report();
        assert!(
            !report.recovered,
            "`{statement}` left a transaction in the log: {report:?}"
        );
        let connection = database.session();
        assert_eq!(column(&connection, "SELECT x FROM o"), vec!["1"]);
    }
}

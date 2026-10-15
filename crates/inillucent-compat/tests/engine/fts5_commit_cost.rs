//! What a commit writes into an FTS5 index whose common words already have long lists.
//!
//! Invariant: **a commit writes about as many bytes as the rows it commits, not as many as the
//! index holds.** Every commit used to read the whole list of each word its rows contained,
//! add the new entries and write the whole list back. A word that is in every row has a list as
//! long as the table, so a load that commits each statement wrote the list again for each row:
//! 40,000 rows took 16 s where SQLite takes 0.36 s, and the work grew as the square of the rows.
//!
//! The index now appends. A commit writes the entries it adds as a new segment, and small
//! segments are merged into larger ones as the load goes on, so each entry is written a number
//! of times that grows with the logarithm of the rows.
//!
//! The check is a count of the bytes of index rows the module wrote, which does not depend on
//! how fast or how loaded the machine is. `flushed_bytes` counts the postings written by a
//! commit and by every merge it starts.

use inillucent_compat::facade::{Connection, Database};

/// Where this suite's scratch databases live.
const AREA: &str = "fts5-commit-cost";

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

/// Runs a statement and returns how many bytes of index rows it wrote.
///
/// @param connection - the connection to run it on
/// @param sql - the statement
fn bytes_written_by(connection: &Connection, sql: &str) -> u64 {
    inillucent_ext::vtab::fts5::reset_build_stages();
    connection.execute(sql).expect("the statement runs");
    inillucent_ext::vtab::fts5::build_stages().flushed_bytes
}

/// Returns the text of row `n`: four words that are in every row and two that are in few.
///
/// @param n - the row number
fn text_of(n: usize) -> String {
    format!(
        "the quick body of w{} w{} doc{n}",
        n % 97,
        n.wrapping_mul(31) % 211
    )
}

/// Creates the table and loads `rows` rows in one statement.
///
/// @param connection - a connection to an empty database
/// @param rows - how many rows
fn load_in_one_statement(connection: &Connection, rows: usize) {
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
        .expect("creates the table");
    connection
        .execute(&format!(
            "WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<{rows}) \
             INSERT INTO ft(rowid, a) SELECT i, 'the quick body of w'||(i%97)||' w'||((i*31)%211)||' doc'||i FROM n"
        ))
        .expect("fills the table");
}

/// A commit of one small row writes a bounded number of bytes, however long the lists are.
///
/// The words `the`, `quick`, `body` and `of` are in all 4,000 rows, so each has a list of about
/// 12 KB. Rewriting the four lists for one more row wrote about 50 KB. The new row has eleven
/// postings of about five bytes each, so the commit writes far less than one list.
#[test]
fn a_commit_of_one_small_row_does_not_rewrite_the_common_words() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    load_in_one_statement(&connection, 4_000);

    let written = bytes_written_by(
        &connection,
        "INSERT INTO ft(rowid, a) VALUES (4001, 'the quick body of w1 w2 doc4001')",
    );

    assert!(
        written < 1_024,
        "committing one row of seven words wrote {written} bytes of index rows; a commit that \
         appends writes the row's own postings, about 100 bytes, and one that rewrites the lists \
         of the four common words writes about 50,000"
    );
}

/// A load that commits every row writes bytes in proportion to the rows, up to a logarithm.
///
/// 1,500 single row commits over four words that are in every row. Rewriting the lists wrote
/// about 1,500 * 1,500 / 2 entries of each of four words, which is over 13 MB. Appending and
/// merging writes each entry about once per level, and there are about six levels.
#[test]
fn a_load_that_commits_every_row_writes_bytes_in_proportion_to_the_rows() {
    let rows = 1_500usize;
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
        .expect("creates the table");

    inillucent_ext::vtab::fts5::reset_build_stages();
    for n in 1..=rows {
        connection
            .execute(&format!(
                "INSERT INTO ft(rowid, a) VALUES ({n}, '{}')",
                text_of(n)
            ))
            .expect("inserts");
    }
    let written = inillucent_ext::vtab::fts5::build_stages().flushed_bytes;

    // Each row holds eleven postings of about five bytes. Every entry is written once by its
    // own commit and again by each merge it goes through, and six levels is generous.
    let bound = (rows as u64) * 11 * 5 * 8;
    assert!(
        written <= bound,
        "{rows} commits of one row wrote {written} bytes of index rows, and a load whose cost is \
         in proportion to its rows writes at most {bound}; rewriting the lists writes millions"
    );
    let found = connection
        .query("SELECT count(*) FROM ft WHERE ft MATCH 'quick'")
        .expect("searches");
    assert_eq!(found.len(), 1);
}

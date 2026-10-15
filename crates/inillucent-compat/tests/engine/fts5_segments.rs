//! The segments of an FTS5 index answer the way a plain model of the table does.
//!
//! Invariant: **however the rows were written, committed, rolled back and merged, a search
//! finds the rows whose text holds the words.** A commit writes its postings as a segment of
//! its own and small segments are merged as the load goes on, so a row's entries can be in any
//! number of segments at once, a deleted row is hidden by a marker in a newer segment, and an
//! updated row is in an older segment with its old text and a newer one with its new text.
//!
//! The model is a map from rowid to the two columns' text. Each case runs a long random
//! sequence of statements against the table and the model, with a commit after each statement,
//! with several statements in one transaction, with transactions that roll back, and with
//! savepoints that roll back. After every few statements it asks the index about every word, a
//! prefix, a phrase and a column filter, and compares the rows with what the model says. The
//! merge chunk is set to a few bytes so a merge runs in many passes.

use std::collections::BTreeMap;

use inillucent_compat::facade::{Connection, Database};
use inillucent_value::Value;

/// Where this suite's scratch databases live.
const AREA: &str = "fts5-segments";

/// The words the documents are made of. A small set, so most words are in many rows.
const WORDS: [&str; 10] = [
    "alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa",
];

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

/// A small deterministic random number generator, so a failure repeats.
struct Random(u64);

impl Random {
    /// Returns the next number below a limit.
    ///
    /// @param limit - the number the result is below
    fn below(&mut self, limit: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % limit as u64) as usize
    }

    /// Returns a text of a few words, or none.
    fn text(&mut self) -> String {
        let count = self.below(5);
        (0..count)
            .map(|_| WORDS[self.below(WORDS.len())])
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// The rows the table should hold: rowid to the text of the two columns.
type Model = BTreeMap<i64, (String, String)>;

/// Returns the rowids a query answers, ascending.
///
/// @param connection - the connection to ask
/// @param sql - a query whose first column is a rowid
fn rowids(connection: &Connection, sql: &str) -> Vec<i64> {
    let mut found: Vec<i64> = connection
        .query(sql)
        .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
        .iter()
        .filter_map(|row| match row.first() {
            Some(Value::Integer(number)) => Some(*number),
            _ => None,
        })
        .collect();
    found.sort_unstable();
    found
}

/// Returns the tokens of a text.
///
/// @param text - the text
fn tokens(text: &str) -> Vec<&str> {
    text.split_whitespace().collect()
}

/// Returns the rowids of the model whose text matches a test on one column or on either.
///
/// @param model - the rows
/// @param column - which column, or nothing for both
/// @param test - a test of a column's tokens
fn model_rows(model: &Model, column: Option<usize>, test: &dyn Fn(&[&str]) -> bool) -> Vec<i64> {
    model
        .iter()
        .filter(|(_, (a, b))| {
            let in_a = test(&tokens(a));
            let in_b = test(&tokens(b));
            match column {
                Some(0) => in_a,
                Some(_) => in_b,
                None => in_a || in_b,
            }
        })
        .map(|(rowid, _)| *rowid)
        .collect()
}

/// Compares what the index answers with what the model says, for every kind of query.
///
/// @param connection - the connection to ask
/// @param model - the rows the table should hold
/// @param when - where in the run this is, for the failure message
fn check(connection: &Connection, model: &Model, when: &str) {
    for word in WORDS {
        let expected = model_rows(model, None, &|held| held.contains(&word));
        assert_eq!(
            rowids(
                connection,
                &format!("SELECT rowid FROM ft WHERE ft MATCH '{word}'")
            ),
            expected,
            "{when}: the word {word}"
        );
        let in_a = model_rows(model, Some(0), &|held| held.contains(&word));
        assert_eq!(
            rowids(
                connection,
                &format!("SELECT rowid FROM ft WHERE ft MATCH 'a:{word}'")
            ),
            in_a,
            "{when}: the word {word} in column a"
        );
    }
    for prefix in ["al", "e", "th", "k"] {
        let expected = model_rows(model, None, &|held| {
            held.iter().any(|token| token.starts_with(prefix))
        });
        assert_eq!(
            rowids(
                connection,
                &format!("SELECT rowid FROM ft WHERE ft MATCH '{prefix}*'")
            ),
            expected,
            "{when}: the prefix {prefix}"
        );
    }
    for (first, second) in [("alpha", "beta"), ("gamma", "gamma"), ("eta", "theta")] {
        let expected = model_rows(model, None, &|held| {
            held.windows(2)
                .any(|pair| pair[0] == first && pair[1] == second)
        });
        assert_eq!(
            rowids(
                connection,
                &format!("SELECT rowid FROM ft WHERE ft MATCH '\"{first} {second}\"'")
            ),
            expected,
            "{when}: the phrase {first} {second}"
        );
    }
    assert_eq!(
        rowids(connection, "SELECT rowid FROM ft ORDER BY rowid"),
        model.keys().copied().collect::<Vec<_>>(),
        "{when}: the rows of the table"
    );
}

/// One statement of the random run, applied to the table and the model together.
///
/// @param connection - the connection to the table
/// @param model - the rows the table should hold, changed to match
/// @param random - the random numbers
/// @param next - the next rowid an insert names
fn step(connection: &Connection, model: &mut Model, random: &mut Random, next: &mut i64) {
    let kind = random.below(10);
    if kind < 5 || model.is_empty() {
        let (a, b) = (random.text(), random.text());
        let rowid = *next;
        *next += 1;
        connection
            .execute(&format!(
                "INSERT INTO ft(rowid, a, b) VALUES ({rowid}, '{a}', '{b}')"
            ))
            .expect("inserts");
        model.insert(rowid, (a, b));
        return;
    }
    let keys: Vec<i64> = model.keys().copied().collect();
    let rowid = keys[random.below(keys.len())];
    if kind < 8 {
        let (a, b) = (random.text(), random.text());
        connection
            .execute(&format!(
                "UPDATE ft SET a = '{a}', b = '{b}' WHERE rowid = {rowid}"
            ))
            .expect("updates");
        model.insert(rowid, (a, b));
    } else {
        connection
            .execute(&format!("DELETE FROM ft WHERE rowid = {rowid}"))
            .expect("deletes");
        model.remove(&rowid);
    }
}

/// Runs a long random sequence of statements and compares the index with the model throughout.
///
/// @param seed - the random seed
/// @param statements - how many statements to run
/// @param chunk - the merge chunk size in bytes
fn run(seed: u64, statements: usize, chunk: usize) {
    inillucent_ext::vtab::fts5::set_merge_chunk_bytes(chunk);
    let path = scratch();
    let mut database = Database::open(&path).expect("opens");
    let mut connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a, b)")
        .expect("creates the table");
    let mut random = Random(seed);
    let mut model = Model::new();
    let mut next = 1i64;
    let mut done = 0usize;
    while done < statements {
        match random.below(10) {
            // Several statements in one transaction, which commits.
            0..=2 => {
                connection.execute("BEGIN").expect("begins");
                for _ in 0..random.below(8) + 1 {
                    step(&connection, &mut model, &mut random, &mut next);
                    done += 1;
                }
                connection.execute("COMMIT").expect("commits");
            }
            // A transaction that rolls back, leaving the model as it was.
            3 => {
                let (kept, kept_next) = (model.clone(), next);
                connection.execute("BEGIN").expect("begins");
                for _ in 0..random.below(8) + 1 {
                    step(&connection, &mut model, &mut random, &mut next);
                    done += 1;
                }
                connection.execute("ROLLBACK").expect("rolls back");
                model = kept;
                next = kept_next;
            }
            // A savepoint inside a transaction, which rolls back or is released.
            4 => {
                connection.execute("BEGIN").expect("begins");
                step(&connection, &mut model, &mut random, &mut next);
                let (kept, kept_next) = (model.clone(), next);
                connection.execute("SAVEPOINT inner").expect("saves");
                for _ in 0..random.below(6) + 1 {
                    step(&connection, &mut model, &mut random, &mut next);
                    done += 1;
                }
                if random.below(2) == 0 {
                    connection
                        .execute("ROLLBACK TO inner")
                        .expect("rolls back to the savepoint");
                    model = kept;
                    next = kept_next;
                }
                connection.execute("RELEASE inner").expect("releases");
                step(&connection, &mut model, &mut random, &mut next);
                connection.execute("COMMIT").expect("commits");
            }
            // A single statement that commits on its own.
            _ => {
                step(&connection, &mut model, &mut random, &mut next);
                done += 1;
            }
        }
        if done % 7 == 0 {
            check(&connection, &model, &format!("seed {seed}, after {done}"));
        }
        if random.below(60) == 0 {
            // The file is closed and opened, so what is checked is what was stored.
            drop(connection);
            drop(database);
            database = Database::open(&path).expect("opens again");
            connection = database.session().expect("connects again");
            check(&connection, &model, &format!("seed {seed}, reopened"));
        }
        if random.below(80) == 0 {
            connection
                .execute("INSERT INTO ft(ft) VALUES ('integrity-check')")
                .unwrap_or_else(|error| {
                    panic!("seed {seed}: the index is inconsistent: {error:?}")
                });
        }
    }
    check(&connection, &model, &format!("seed {seed}, at the end"));
    connection
        .execute("INSERT INTO ft(ft) VALUES ('integrity-check')")
        .expect("the index is consistent");
    connection
        .execute("INSERT INTO ft(ft) VALUES ('optimize')")
        .expect("merges every segment");
    check(&connection, &model, &format!("seed {seed}, after optimize"));
    let outside_the_base = connection
        .query("SELECT count(*) FROM ft_idx WHERE segid <> 0")
        .expect("counts");
    assert_eq!(
        outside_the_base
            .first()
            .and_then(|row| row.first())
            .and_then(Value::as_integer),
        Some(0),
        "seed {seed}: optimize leaves everything in the base segment"
    );
    connection
        .execute("INSERT INTO ft(ft) VALUES ('rebuild')")
        .expect("rebuilds");
    check(&connection, &model, &format!("seed {seed}, after rebuild"));
    inillucent_ext::vtab::fts5::set_merge_chunk_bytes(4 * 1024 * 1024);
}

/// Statements of every kind, with merges that run in many passes.
#[test]
fn a_random_run_with_tiny_merge_chunks_matches_the_model() {
    run(0x9e37_79b9_7f4a_7c15, 700, 40);
}

/// The same with the merge chunk the index uses.
#[test]
fn a_random_run_matches_the_model() {
    run(0x1234_5678_9abc_def1, 700, 4 * 1024 * 1024);
}

/// A different seed, since the sequence decides which merges happen.
#[test]
fn another_random_run_matches_the_model() {
    run(0xdead_beef_cafe_f00d, 700, 200);
}

/// Returns the segment numbers `%_idx` holds rows in.
///
/// @param connection - the connection to the table `ft`
fn segments(connection: &Connection) -> Vec<i64> {
    rowids(
        connection,
        "SELECT DISTINCT segid FROM ft_idx ORDER BY segid",
    )
}

/// A load that commits every row keeps a small number of segments.
///
/// 600 commits of one row each make 600 segments if nothing merges them. Merging four
/// neighbours of a level into one of the next leaves at most three at each level, and 600 rows
/// need about five levels.
#[test]
fn single_row_commits_leave_a_small_number_of_segments() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a, b)")
        .expect("creates the table");
    for rowid in 1..=600 {
        connection
            .execute(&format!(
                "INSERT INTO ft(rowid, a, b) VALUES ({rowid}, 'alpha beta', 'gamma {rowid}')"
            ))
            .expect("inserts");
    }
    let held = segments(&connection);
    assert!(
        held.len() <= 20,
        "600 single row commits left {} segments: {held:?}",
        held.len()
    );
    assert_eq!(
        rowids(&connection, "SELECT rowid FROM ft WHERE ft MATCH 'alpha'").len(),
        600
    );
}

/// Returns the bytes of the layout record, which `%_data` row 2 holds.
///
/// @param connection - the connection to the table `ft`
fn layout_record(connection: &Connection) -> Vec<u8> {
    let rows = connection
        .query("SELECT block FROM ft_data WHERE id = 2")
        .expect("reads the layout record");
    match rows.first().and_then(|row| row.first()) {
        Some(Value::Blob(blob)) => blob.raw().to_vec(),
        other => panic!("the layout record is {other:?}"),
    }
}

/// Returns the layout number a layout record names.
///
/// @param record - the record's bytes
fn layout_number(record: &[u8]) -> u32 {
    let mut number = [0u8; 4];
    number.copy_from_slice(&record[8..12]);
    u32::from_le_bytes(number)
}

/// An index an earlier build wrote is read, written to, and marked as one with segments.
///
/// **The older layout is manufactured from a file this build wrote**, as the legacy layout
/// test does: `optimize` leaves every term in segment 0, the segment list is removed, and the
/// layout record is rewritten to say layout 2. That is what the build before segments left. The
/// file must then answer, take inserts, updates and deletes, and say layout 3 afterwards, so a
/// build that does not read segments refuses it.
#[test]
fn an_index_from_before_segments_is_read_and_extended() {
    let path = scratch();
    let database = Database::open(&path).expect("opens");
    let connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a, b)")
        .expect("creates the table");
    let mut model = Model::new();
    for rowid in 1..=40i64 {
        let (a, b) = (
            format!(
                "{} {}",
                WORDS[(rowid % 10) as usize],
                WORDS[((rowid * 3) % 10) as usize]
            ),
            format!("{}", WORDS[((rowid * 7) % 10) as usize]),
        );
        connection
            .execute(&format!(
                "INSERT INTO ft(rowid, a, b) VALUES ({rowid}, '{a}', '{b}')"
            ))
            .expect("inserts");
        model.insert(rowid, (a, b));
    }
    connection
        .execute("INSERT INTO ft(ft) VALUES ('optimize')")
        .expect("merges");
    assert_eq!(segments(&connection), vec![0]);
    connection
        .execute("DELETE FROM ft_data WHERE id = 1099511627776")
        .expect("removes the segment list");
    let mut old = layout_record(&connection);
    old[8..12].copy_from_slice(&2u32.to_le_bytes());
    let mut statement = connection
        .prepare("UPDATE ft_data SET block = ?1 WHERE id = 2")
        .expect("prepares");
    statement.bind_blob(1, &old).expect("binds");
    while statement.step().expect("steps") {}
    drop(statement);
    assert_eq!(layout_number(&layout_record(&connection)), 2);
    // The connection remembers the layout it read, so the file is closed and opened as a build
    // that finds this file would open it.
    drop(connection);
    drop(database);
    let database = Database::open(&path).expect("opens again");
    let connection = database.session().expect("connects again");

    check(&connection, &model, "an index from before segments");
    assert_eq!(
        layout_number(&layout_record(&connection)),
        2,
        "reading does not change the file"
    );

    for rowid in 41..=60i64 {
        let (a, b) = (
            format!("{} iota", WORDS[(rowid % 10) as usize]),
            String::new(),
        );
        connection
            .execute(&format!(
                "INSERT INTO ft(rowid, a, b) VALUES ({rowid}, '{a}', '{b}')"
            ))
            .expect("inserts");
        model.insert(rowid, (a, b));
    }
    for rowid in [3i64, 17, 40] {
        connection
            .execute(&format!("DELETE FROM ft WHERE rowid = {rowid}"))
            .expect("deletes a row the base holds");
        model.remove(&rowid);
    }
    connection
        .execute("UPDATE ft SET a = 'kappa kappa' WHERE rowid = 5")
        .expect("updates a row the base holds");
    model.insert(5, ("kappa kappa".to_string(), model[&5].1.clone()));
    check(
        &connection,
        &model,
        "an index from before segments, written to",
    );
    assert_eq!(
        layout_number(&layout_record(&connection)),
        3,
        "the first segment written into the file marks it as one with segments"
    );
    assert!(segments(&connection).len() > 1);
    connection
        .execute("INSERT INTO ft(ft) VALUES ('integrity-check')")
        .expect("the index is consistent");
    connection
        .execute("INSERT INTO ft(ft) VALUES ('optimize')")
        .expect("merges");
    assert_eq!(segments(&connection), vec![0]);
    check(
        &connection,
        &model,
        "an index from before segments, optimized",
    );
}

/// An index whose segment list names a segment that is gone, or lacks one, is reported.
#[test]
fn a_row_in_a_segment_the_list_does_not_name_is_reported() {
    let database = Database::open(scratch()).expect("opens");
    let connection = database.session().expect("connects");
    connection
        .execute("CREATE VIRTUAL TABLE ft USING fts5(a)")
        .expect("creates the table");
    connection
        .execute("INSERT INTO ft(rowid, a) VALUES (1, 'alpha')")
        .expect("inserts");
    connection
        .execute(
            "INSERT INTO ft_idx(segid, term, doclist) VALUES (77, x'616c706861', x'0201000100')",
        )
        .expect("writes a stray row");
    let verdict = connection.execute("INSERT INTO ft(ft) VALUES ('integrity-check')");
    assert!(
        verdict.is_err(),
        "a row in segment 77, which the list does not name, passed the check"
    );
}

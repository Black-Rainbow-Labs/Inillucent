//! Keys and indexed values longer than a page, graded against pinned SQLite 3.53.4.
//!
//! Invariant: **an indexed or key value of any length up to the key limit is
//! stored, found and ordered exactly as SQLite does it.** SQLite accepts a
//! 50,000 character value in an indexed TEXT column. This engine refused it,
//! because a key was the one value never stored out of line, and two keys of
//! about 17 KB could not share a leaf.
//!
//! ## Why the page size is stated
//!
//! A value goes out of line when it is longer than `page_size / 8`, and the
//! leaf keeps the first 512 bytes of it. At the 4,096 byte page size these
//! tests use that is 512 bytes, so every key here that matters is out of line,
//! and a separator over 512 bytes is shortened at a split. Keys that share
//! their first thousand bytes force a separator that cannot be shortened below
//! the limit, which is stored out of line in the interior page.

use std::path::PathBuf;

use inillucent_base::testing::ScratchDir;
use inillucent_compat::newengine::ImportedDatabase;
use inillucent_compat::oracle::{Driver, Op, TaggedValue};
use inillucent_compat::workspace_root;
use inillucent_exec::physical::Params;
use inillucent_tree::datum::OwnedDatum;

/// The page size these tests build at, which fixes the spill threshold at 512.
const PAGE_SIZE: usize = 4_096;

/// Returns the pinned SQLite oracle binary, if it has been built.
fn oracle_path() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("INILLUCENT_SQLITE_ORACLE") {
        let path = PathBuf::from(explicit);
        return path.is_file().then_some(path);
    }
    let directory = workspace_root().join(".sqlite-ref/3.53.4");
    let path = directory.join(format!("sqlite-oracle{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Returns a fresh directory for one test's files, removed when the returned
/// guard drops.
///
/// @param tag - what to name it after
fn scratch(tag: &str) -> ScratchDir {
    ScratchDir::new(&format!(
        "inillucent-longkeys-{}-{tag}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ))
}

/// A fixture both engines have opened.
struct Pair {
    engine: ImportedDatabase,
    oracle: Driver,
    /// Kept so the directory outlives the test.
    _directory: ScratchDir,
}

/// Builds an empty fixture with the oracle from a schema, and imports it.
///
/// @param tag - what to name the scratch directory after
/// @param schema - the statements that build the tables and indexes
fn pair(tag: &str, schema: &[&str]) -> Option<Pair> {
    let program = oracle_path()?;
    let directory = scratch(tag);
    let path = directory.join("long.db");
    let mut oracle = Driver::start("sqlite", &program).expect("the oracle starts");
    oracle
        .send(&Op::Open(path.to_string_lossy().into_owned()))
        .expect("the oracle opens the fixture");
    for sql in schema {
        let observed = oracle
            .send(&Op::Exec(sql.to_string()))
            .expect("the exec runs");
        assert!(observed.ok, "the fixture did not build: {sql}");
    }
    let engine = ImportedDatabase::import_with(path, PAGE_SIZE, 4_096)
        .unwrap_or_else(|error| panic!("the fixture did not import: {:?}", error.detail()));
    Some(Pair {
        engine,
        oracle,
        _directory: directory,
    })
}

impl Pair {
    /// Runs one statement on both engines and asserts they agreed about it.
    ///
    /// @param sql - the statement
    fn both(&mut self, sql: &str) {
        let theirs = self
            .oracle
            .send(&Op::Exec(sql.to_string()))
            .expect("the oracle runs the statement");
        let ours = self.engine.execute_any(sql, &Params::new());
        assert_eq!(
            theirs.ok,
            ours.is_ok(),
            "{}: sqlite said {} ({}), inillucent said {} ({:?})",
            shorten(sql),
            theirs.ok,
            theirs.message,
            ours.is_ok(),
            ours.as_ref().err().and_then(|error| error.detail())
        );
    }

    /// Asserts both engines answer one query identically.
    ///
    /// @param sql - the query
    fn answers_agree(&mut self, sql: &str) {
        let theirs = self
            .oracle
            .send(&Op::Query(sql.to_string()))
            .expect("the oracle runs the query");
        assert!(
            theirs.ok,
            "{}: the oracle refused: {}",
            shorten(sql),
            theirs.message
        );
        let ours = self
            .engine
            .execute_any(sql, &Params::new())
            .unwrap_or_else(|error| {
                panic!(
                    "{}: the new engine refused: {:?}",
                    shorten(sql),
                    error.detail()
                )
            });
        let mine: Vec<Vec<TaggedValue>> = ours
            .rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|value| match value {
                        OwnedDatum::Null => TaggedValue::Null,
                        OwnedDatum::Int(number) => TaggedValue::Integer(*number),
                        OwnedDatum::Real(number) => TaggedValue::Real(*number),
                        OwnedDatum::Text(bytes) => TaggedValue::Text(bytes.clone()),
                        OwnedDatum::Blob(bytes) => TaggedValue::Blob(bytes.clone()),
                    })
                    .collect()
            })
            .collect();
        assert_eq!(mine, theirs.rows, "{} disagreed", shorten(sql));
    }

    /// Walks every tree and fails on the first broken invariant.
    ///
    /// @param after - what was just run, for the message
    fn is_intact(&self, after: &str) {
        self.engine.check_trees().unwrap_or_else(|error| {
            panic!(
                "the file is not intact after `{}`: {error:?}",
                shorten(after)
            )
        });
    }

    /// Asserts the file is intact, both engines agree it is, and says so again
    /// after the file is closed and opened.
    ///
    /// @param after - what was just run, for the message
    fn is_intact_and_reopens(&mut self, after: &str) {
        self.is_intact(after);
        self.answers_agree("PRAGMA integrity_check");
        self.engine.reopen().expect("the file reopens");
        self.is_intact(&format!("{after}, after a reopen"));
    }
}

/// Returns the start of a statement, so a failure message is readable.
///
/// @param sql - the statement
fn shorten(sql: &str) -> String {
    sql.chars().take(160).collect()
}

/// Says the suite could not run, rather than passing quietly.
fn no_oracle() {
    inillucent_compat::differential::skipping(
        "the pinned SQLite oracle is not built; run tools/sqlite-reference.{ps1,sh}",
    );
}

/// Returns an SQL expression for a text of `length` copies of `letter`.
///
/// @param length - how many characters, which must be even
/// @param letter - the character
fn run_of(length: usize, letter: char) -> String {
    format!(
        "replace(hex(zeroblob({})), '0', '{letter}')",
        length.div_ceil(2)
    )
}

/// The questions that grade a table `t` with an index on `v`.
///
/// Lengths and a hash-like projection rather than the 50,000 character values
/// themselves, so a failure message stays readable; the ordering questions
/// return the ids, which is what an index gets wrong.
const T_PROBES: &[&str] = &[
    "SELECT id, length(v) FROM t ORDER BY id",
    "SELECT id FROM t ORDER BY v, id",
    "SELECT id FROM t ORDER BY v DESC, id",
    "SELECT count(*), count(DISTINCT v) FROM t",
    "SELECT length(v), count(*) FROM t GROUP BY v ORDER BY v",
    "SELECT DISTINCT length(v) FROM t ORDER BY 1",
    "SELECT id FROM t WHERE v > 'qq' ORDER BY v, id",
    "SELECT id FROM t WHERE v BETWEEN 'a' AND 'qr' ORDER BY id",
    "SELECT count(*) FROM t WHERE v < 'qqqqqqqqqqqqqqqq'",
];

/// A 50,000 character value in an indexed column: the bug hunt's defect.
///
/// Insert, an equality lookup, ranges, `ORDER BY` through the index, `DISTINCT`
/// and `GROUP BY`, a delete, a reinsert and a reopen, each answer equal to
/// SQLite's.
#[test]
fn a_fifty_thousand_character_indexed_value_is_stored_and_found() {
    let Some(mut pair) = pair(
        "d1",
        &[
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "CREATE INDEX t_v ON t (v)",
        ],
    ) else {
        return no_oracle();
    };
    let long = run_of(50_000, 'q');
    for (id, value) in [
        (1, format!("{long} || 'a'")),
        (2, format!("{long} || 'b'")),
        (3, run_of(50_000, 'r')),
        (4, "'short'".to_string()),
    ] {
        let sql = format!("INSERT INTO t VALUES ({id}, {value})");
        pair.both(&sql);
        pair.is_intact(&sql);
    }
    for probe in T_PROBES {
        pair.answers_agree(probe);
    }
    // An equality lookup by the whole value, in both of the ways it is asked.
    pair.answers_agree(&format!(
        "SELECT id FROM t WHERE v = {long} || 'b' ORDER BY id"
    ));
    pair.answers_agree(&format!(
        "SELECT id FROM t WHERE v IN ({long} || 'a', {}) ORDER BY id",
        run_of(50_000, 'r')
    ));
    pair.answers_agree(&format!(
        "SELECT count(*) FROM t WHERE v >= {long} || 'a' AND v < {long} || 'c'"
    ));
    pair.is_intact_and_reopens("the inserts");
    for probe in T_PROBES {
        pair.answers_agree(probe);
    }
    pair.both("DELETE FROM t WHERE id = 2");
    pair.is_intact("the delete");
    pair.both(&format!("INSERT INTO t VALUES (2, {long} || 'b')"));
    pair.both(&format!("UPDATE t SET v = {long} || 'z' WHERE id = 1"));
    pair.is_intact_and_reopens("the delete, the reinsert and the update");
    for probe in T_PROBES {
        pair.answers_agree(probe);
    }
}

/// A `UNIQUE` index over a long column refuses a duplicate and replaces on request.
#[test]
fn a_unique_index_over_a_long_column_refuses_a_duplicate() {
    let Some(mut pair) = pair(
        "d2",
        &[
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "CREATE UNIQUE INDEX t_v ON t (v)",
        ],
    ) else {
        return no_oracle();
    };
    let long = run_of(50_000, 'u');
    pair.both(&format!("INSERT INTO t VALUES (1, {long})"));
    pair.both(&format!("INSERT INTO t VALUES (2, {long} || 'x')"));
    // The duplicate is refused by both, with the constraint status.
    pair.both(&format!("INSERT INTO t VALUES (3, {long})"));
    let refused = pair
        .engine
        .execute_any(&format!("INSERT INTO t VALUES (4, {long})"), &Params::new())
        .expect_err("a duplicate of a long unique value is refused");
    assert!(
        refused.message().contains("UNIQUE"),
        "the refusal names the constraint: {}",
        refused.message()
    );
    pair.answers_agree("SELECT id, length(v) FROM t ORDER BY id");
    pair.both(&format!("INSERT OR REPLACE INTO t VALUES (5, {long})"));
    pair.answers_agree("SELECT id, length(v) FROM t ORDER BY id");
    pair.is_intact_and_reopens("the replace over a long unique value");
}

/// A `WITHOUT ROWID` table keyed by a long TEXT primary key, updated in a non
/// key column.
#[test]
fn a_without_rowid_table_with_a_long_text_primary_key() {
    let Some(mut pair) = pair(
        "d3",
        &["CREATE TABLE w (k TEXT PRIMARY KEY, n INTEGER, note TEXT) WITHOUT ROWID"],
    ) else {
        return no_oracle();
    };
    for (n, letter) in ['a', 'b', 'c', 'd', 'e'].into_iter().enumerate() {
        pair.both(&format!(
            "INSERT INTO w VALUES ({} || '{n}', {n}, 'note {n}')",
            run_of(20_000, letter)
        ));
    }
    pair.both(&format!(
        "UPDATE w SET note = 'changed', n = n + 100 WHERE k = {} || '2'",
        run_of(20_000, 'c')
    ));
    for probe in [
        "SELECT length(k), n, note FROM w ORDER BY k",
        "SELECT n FROM w WHERE k >= 'c' ORDER BY k",
        "SELECT count(*) FROM w WHERE k < 'bb'",
    ] {
        pair.answers_agree(probe);
    }
    pair.both(&format!(
        "DELETE FROM w WHERE k = {} || '1'",
        run_of(20_000, 'b')
    ));
    pair.answers_agree("SELECT length(k), n, note FROM w ORDER BY k");
    pair.is_intact_and_reopens("the updates over a long primary key");
}

/// Many keys that share a long prefix and differ after it, inserted in random
/// order, under BINARY and under NOCASE.
///
/// The prefix is 6,000 bytes, so every separator between two neighbours is over
/// the 512 byte limit even after it is shortened, and goes out of line in the
/// interior page. Every lookup, a full index scan and the integrity check agree
/// with SQLite.
#[test]
fn keys_sharing_a_six_thousand_byte_prefix_are_ordered_like_sqlite() {
    for (collation, tag) in [("", "d4-binary"), (" COLLATE NOCASE", "d4-nocase")] {
        let Some(mut pair) = pair(
            tag,
            &[
                "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
                &format!("CREATE INDEX t_v ON t (v{collation})"),
            ],
        ) else {
            return no_oracle();
        };
        let rows = 600usize;
        // A fixed shuffle: a multiplier coprime to the count visits every id once.
        for step in 0..rows {
            let id = (step * 211) % rows;
            // Under NOCASE the prefix's case differs between rows that must still
            // sort together, which BINARY puts far apart.
            let prefix = match id % 2 {
                0 => run_of(6_000, 'q'),
                _ => format!("upper({})", run_of(6_000, 'q')),
            };
            pair.both(&format!(
                "INSERT INTO t VALUES ({id}, {prefix} || printf('%05d', {id}))"
            ));
        }
        pair.is_intact(tag);
        for probe in [
            "SELECT id FROM t ORDER BY v, id",
            "SELECT id FROM t ORDER BY v DESC, id",
            "SELECT count(*) FROM t WHERE v > substr(v, 1, 5999)",
            "SELECT id FROM t WHERE id % 37 = 0 ORDER BY v",
        ] {
            pair.answers_agree(probe);
        }
        for id in (0..rows).step_by(23) {
            pair.answers_agree(&format!(
                "SELECT id FROM t WHERE v = (SELECT v FROM t WHERE id = {id})"
            ));
            pair.answers_agree(&format!(
                "SELECT count(*) FROM t WHERE v >= (SELECT v FROM t WHERE id = {id})"
            ));
        }
        pair.answers_agree("PRAGMA integrity_check");
        pair.is_intact_and_reopens(tag);
    }
}

/// Five hundred rows with the same 40,000 character value in an indexed column.
///
/// Duplicates of a long value are the common form of a long key: the index key
/// is the value and then the rowid, so the boundary between two leaves of the
/// same value needs the whole value in its separator. Then every other row is
/// deleted, the file is vacuumed and compared, and checked again.
#[test]
fn five_hundred_duplicates_of_a_forty_thousand_character_value() {
    let Some(mut pair) = pair(
        "d5",
        &[
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "CREATE INDEX t_v ON t (v)",
        ],
    ) else {
        return no_oracle();
    };
    let long = run_of(40_000, 'd');
    for id in 1..=500 {
        pair.both(&format!("INSERT INTO t VALUES ({id}, {long})"));
    }
    pair.is_intact("the inserts");
    for probe in [
        "SELECT count(*), min(id), max(id) FROM t WHERE v >= 'd'",
        "SELECT id FROM t WHERE v = (SELECT v FROM t WHERE id = 250) ORDER BY id LIMIT 5",
        "SELECT id FROM t ORDER BY v, id DESC LIMIT 7",
        "SELECT count(DISTINCT v) FROM t",
    ] {
        pair.answers_agree(probe);
    }
    pair.both("DELETE FROM t WHERE id % 2 = 0");
    pair.is_intact("the deletes");
    pair.answers_agree("SELECT count(*), min(id), max(id) FROM t WHERE v >= 'd'");
    pair.both("VACUUM");
    // The connection reattaches to the rebuilt file before it is checked: the
    // trees it held name roots that VACUUM moved.
    pair.engine.reopen().expect("the file reopens");
    pair.is_intact("the vacuum");
    for probe in [
        "SELECT count(*), min(id), max(id) FROM t WHERE v >= 'd'",
        "SELECT id FROM t ORDER BY v, id LIMIT 9",
        "SELECT sum(length(v)) FROM t",
    ] {
        pair.answers_agree(probe);
    }
    pair.is_intact_and_reopens("the vacuum over duplicates of a long value");
}

/// A blob key of 16,000 zero bytes beside one of 15,999.
///
/// The encoded form escapes every zero byte, so the separator is twice the raw
/// key.
#[test]
fn blob_keys_of_zero_bytes_encode_to_twice_their_length() {
    let Some(mut pair) = pair(
        "d6",
        &[
            "CREATE TABLE b (id INTEGER PRIMARY KEY, k BLOB)",
            "CREATE INDEX b_k ON b (k)",
        ],
    ) else {
        return no_oracle();
    };
    for (id, length) in [
        (1, 16_000),
        (2, 15_999),
        (3, 16_001),
        (4, 15_998),
        (5, 16_000),
    ] {
        pair.both(&format!("INSERT INTO b VALUES ({id}, zeroblob({length}))"));
    }
    for probe in [
        "SELECT id, length(k) FROM b ORDER BY k, id",
        "SELECT id FROM b WHERE k = zeroblob(16000) ORDER BY id",
        "SELECT id FROM b WHERE k > zeroblob(15999) ORDER BY k, id",
        "SELECT id FROM b WHERE k < zeroblob(16000) ORDER BY k DESC, id",
    ] {
        pair.answers_agree(probe);
    }
    pair.is_intact_and_reopens("the zero blob keys");
}

/// Three keys of 17,000 bytes, inserted small, large, large.
///
/// The split priced the left half and never the right, so two keys of about
/// 17 KB on the right half overflowed the encoder instead of being refused or
/// stored.
#[test]
fn three_keys_of_seventeen_thousand_bytes_inserted_small_large_large() {
    let Some(mut pair) = pair(
        "d7",
        &[
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "CREATE INDEX t_v ON t (v)",
        ],
    ) else {
        return no_oracle();
    };
    pair.both("INSERT INTO t VALUES (1, 'small')");
    pair.both(&format!(
        "INSERT INTO t VALUES (2, {})",
        run_of(17_000, 'm')
    ));
    pair.both(&format!(
        "INSERT INTO t VALUES (3, {})",
        run_of(17_000, 'n')
    ));
    pair.both(&format!(
        "INSERT INTO t VALUES (4, {})",
        run_of(17_000, 'o')
    ));
    pair.is_intact("the inserts");
    pair.answers_agree("SELECT id, length(v) FROM t ORDER BY v");
    pair.is_intact_and_reopens("three keys of 17,000 bytes");
}

/// A multi column index whose first column is long and whose second decides the
/// order.
#[test]
fn a_multi_column_index_with_a_long_first_column() {
    let Some(mut pair) = pair(
        "d8",
        &[
            "CREATE TABLE m (id INTEGER PRIMARY KEY, a TEXT, b INTEGER)",
            "CREATE INDEX m_ab ON m (a, b)",
        ],
    ) else {
        return no_oracle();
    };
    let long = run_of(3_000, 'a');
    for id in 1..=120usize {
        pair.both(&format!(
            "INSERT INTO m VALUES ({id}, {long}, {})",
            (id * 37) % 101
        ));
    }
    for id in 121..=140usize {
        pair.both(&format!(
            "INSERT INTO m VALUES ({id}, {long} || '{}', {})",
            id % 3,
            id % 5
        ));
    }
    for probe in [
        "SELECT id, b FROM m ORDER BY a, b, id",
        "SELECT id FROM m WHERE a = (SELECT a FROM m WHERE id = 1) AND b = 5 ORDER BY id",
        "SELECT count(*) FROM m WHERE a = (SELECT a FROM m WHERE id = 1) AND b > 50",
        "SELECT id FROM m WHERE a > (SELECT a FROM m WHERE id = 1) ORDER BY a, b, id",
    ] {
        pair.answers_agree(probe);
    }
    pair.is_intact_and_reopens("the multi column index");
}

/// A key one byte over the limit is refused here and accepted by SQLite.
///
/// The limit is 1,048,576 bytes of encoded key. SQLite's is a gigabyte. This is
/// recorded as a test that asserts the difference, so it cannot grow or close
/// without a change to this file.
#[test]
fn a_key_over_the_key_limit_is_refused_where_sqlite_accepts_it() {
    let Some(mut pair) = pair(
        "d9",
        &[
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)",
            "CREATE INDEX t_v ON t (v)",
        ],
    ) else {
        return no_oracle();
    };
    let sql = format!("INSERT INTO t VALUES (1, {})", run_of(1_048_578, 'z'));
    let theirs = pair
        .oracle
        .send(&Op::Exec(sql.clone()))
        .expect("the oracle runs the statement");
    assert!(theirs.ok, "SQLite accepts the key: {}", theirs.message);
    let ours = pair
        .engine
        .execute_any(&sql, &Params::new())
        .expect_err("this engine refuses a key over the limit");
    assert!(
        ours.message().contains("longer than 1,048,576 bytes"),
        "the refusal says what the limit is: {}",
        ours.message()
    );
    // A key at the limit's edge is stored, so the refusal is the limit and not
    // a size this engine cannot hold.
    pair.both(&format!(
        "INSERT INTO t VALUES (2, {})",
        run_of(1_000_000, 'y')
    ));
    pair.is_intact_and_reopens("a key under the limit");
}

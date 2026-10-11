//! An index nested loop that joins the rest of a large outer side with one walk of the inner
//! index, graded against pinned SQLite 3.53.4.
//!
//! Invariant: **a join that walks the inner index emits the rows the probes would, in their
//! order.** Since task-2226 an inner join on one key probes its first outer rows and joins the
//! rest by walking the inner index once over the range their keys cover, then puts the pairs
//! back in the outer rows' order. The tables below are shaped like `app.dashboard`'s, many more
//! outer rows than a tenth of the inner index, so the walk runs. `owner` holds what an integer
//! join key can meet in an untyped column: NULLs, text that looks like a number and text that
//! does not, reals equal to an integer and reals that are not, negatives, keys past every outer
//! row, and many rows on one key.
//!
//! The statements whose answers depend on the order rows arrive in are here too: a `sum` of
//! reals and a list of joined rows, and a `LEFT JOIN`, which keeps the probes. A
//! `group_concat` is not, because the order it lists rows in follows each engine's own join
//! order, which SQLite does not specify and this engine's plans already differ from.

use inillucent_compat::differential::{self, Step};

/// The outer table, a key per row and a small category to group by.
const OUTER: &str = "CREATE TABLE m(id INTEGER PRIMARY KEY, category INTEGER, key INTEGER); \
     CREATE INDEX m_category ON m(category, key)";

/// Fills [`OUTER`] with 4,000 rows over 64 categories.
const FILL_OUTER: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 4000) \
     INSERT INTO m(id, category, key) SELECT x, x % 64, (x * 2654435761) % 4000 FROM c";

/// The inner table, with no declared type on `owner`, and the index the join probes.
const INNER: &str = "CREATE TABLE s(id INTEGER PRIMARY KEY, owner, note TEXT); \
     CREATE INDEX s_owner ON s(owner)";

/// Fills [`INNER`] with 3,000 rows whose owners are scattered over the outer ids.
const FILL_INNER: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 3000) \
     INSERT INTO s(owner, note) SELECT ((x * 7) % 4000) + 1, 'n' || x FROM c";

/// The owners an integer key can meet besides integers it matches.
const AWKWARD: &str = "INSERT INTO s(owner, note) VALUES \
     (NULL, 'a'), (NULL, 'b'), ('12', 'c'), ('abc', 'd'), (15.0, 'e'), (15.5, 'f'), \
     (-3, 'g'), (999999, 'h'), (64, 'i'), (64, 'j'), (64, 'k'), (64, 'l'), (1, 'm'), (4000, 'n')";

/// Runs the scenario against both engines and compares every reply.
///
/// @param name - the scenario's name, which names its database files
/// @param steps - the statements, after both tables are built
fn compare(name: &str, steps: &[Step]) {
    let mut all = vec![
        Step::Exec(OUTER),
        Step::Exec(FILL_OUTER),
        Step::Exec(INNER),
        Step::Exec(FILL_INNER),
        Step::Exec(AWKWARD),
    ];
    all.extend_from_slice(steps);
    let compared = differential::compare("merge_join", name, &all);
    assert!(
        compared == 0 || compared == all.len(),
        "compared {compared} of {} steps",
        all.len()
    );
}

/// The dashboard's shape: grouped counts and maxima over a range of categories.
#[test]
fn a_grouped_join_matches_sqlite() {
    compare(
        "grouped",
        &[
            Step::Query(
                "SELECT m.category, count(*), max(s.id), min(s.id) FROM s JOIN m ON m.id = s.owner \
                 WHERE m.category < 8 GROUP BY m.category ORDER BY 2 DESC, 1 LIMIT 5",
            ),
            Step::Query(
                "SELECT m.category, count(s.note), max(s.note) FROM s JOIN m ON m.id = s.owner \
                 WHERE m.category < 40 GROUP BY m.category ORDER BY 1",
            ),
            Step::Query(
                "SELECT count(*), max(s.id), min(m.key) FROM s JOIN m ON m.id = s.owner \
                 WHERE m.category < 60",
            ),
        ],
    );
}

/// An outer side with no rows, one with every row, and one whose keys meet no inner entry.
#[test]
fn edge_ranges_match_sqlite() {
    compare(
        "edges",
        &[
            Step::Query(
                "SELECT count(*), max(s.id) FROM s JOIN m ON m.id = s.owner WHERE m.category < 0",
            ),
            Step::Query(
                "SELECT count(*), max(s.id) FROM s JOIN m ON m.id = s.owner WHERE m.category < 64",
            ),
            Step::Exec("DELETE FROM s WHERE owner BETWEEN 1 AND 2000"),
            Step::Query(
                "SELECT m.category, count(*) FROM s JOIN m ON m.id = s.owner \
                 WHERE m.category < 30 GROUP BY m.category ORDER BY 1",
            ),
        ],
    );
}

/// Statements whose answers depend on the order the joined rows arrive in, and a `LEFT JOIN`.
#[test]
fn order_sensitive_joins_match_sqlite() {
    compare(
        "ordered",
        &[
            Step::Query(
                "SELECT m.category, sum(s.id * 0.1) FROM s JOIN m ON m.id = s.owner \
                 WHERE m.category < 8 GROUP BY m.category ORDER BY 1",
            ),
            Step::Query(
                "SELECT m.category, count(s.id) FROM m LEFT JOIN s ON m.id = s.owner \
                 WHERE m.category < 8 GROUP BY m.category ORDER BY 1",
            ),
            Step::Query(
                "SELECT m.id, s.id FROM s JOIN m ON m.id = s.owner WHERE m.category < 2 \
                 ORDER BY 1, 2",
            ),
        ],
    );
}

/// A `LEFT JOIN` whose `ON` holds more than the key, which joins each outer row by itself.
///
/// The probe for each outer row is an inner join inside `ProbedOuterJoin`, which reads its
/// matches before the next outer row and never finishes the inner join in between. A merge
/// there held the matches back, and every outer row came back null extended.
#[test]
fn a_left_join_with_more_than_the_key_matches_sqlite() {
    compare(
        "left_extra",
        &[
            Step::Query(
                "SELECT m.category, count(s.id), max(s.note) FROM m LEFT JOIN s \
                 ON s.owner = m.id AND s.id % 2 = 0 WHERE m.category < 8 \
                 GROUP BY m.category ORDER BY 1",
            ),
            Step::Query(
                "SELECT m.id, s.id FROM m LEFT JOIN s ON s.owner = m.id AND s.note > 'n2' \
                 WHERE m.category < 2 ORDER BY 1, 2",
            ),
        ],
    );
}

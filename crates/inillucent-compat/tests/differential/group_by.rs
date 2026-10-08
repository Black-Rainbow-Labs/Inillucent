//! A hash `GROUP BY` on one key, graded against pinned SQLite 3.53.4.
//!
//! Invariant: **finding a group by its integer value answers exactly what finding it by its
//! encoded key answers.** Since task-2209 a `GROUP BY` on one key sends a small non negative
//! integer straight to its group's position, and every other value through the encoded key. The
//! cases below put both kinds in one statement: integers inside and outside the direct range,
//! reals equal to integers, negatives, NULLs and text, so a group the two paths split in two, or
//! a row they send to the wrong group, shows as a different count or sum.

use inillucent_compat::differential::{self, Step};

/// A table with no declared type on `k`, so text stays text and reals stay reals.
const MIXED: &str = "CREATE TABLE g(id INTEGER PRIMARY KEY, k, v INTEGER)";

/// Fills [`MIXED`]: integers cycling through 0 to 299, then the awkward values.
const FILL_MIXED: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 3000) \
     INSERT INTO g(k, v) SELECT x % 300, x FROM c";

/// The values that are not small non negative integers, each beside an integer group it may
/// share with.
const AWKWARD: &str = "INSERT INTO g(k, v) VALUES \
     (5.0, 1), (17.0, 2), (17.5, 3), (-3, 4), (-1, 5), (65535, 6), (65536, 7), (70000, 8), \
     (NULL, 9), (NULL, 10), ('7', 11), ('abc', 12), (299, 13), (0, 14), (5, 15)";

/// A table whose key is only small integers, so the direct path stays on for the whole scan.
const INTEGERS: &str = "CREATE TABLE h(id INTEGER PRIMARY KEY, k INTEGER, v REAL)";

/// Fills [`INTEGERS`].
const FILL_INTEGERS: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 5000) \
     INSERT INTO h(k, v) SELECT (x * 7919) % 97, x * 0.25 FROM c";

/// Runs the scenario against both engines and compares every reply.
///
/// @param name - the scenario's name, which names its database files
/// @param steps - the statements, after both tables are built
fn compare(name: &str, steps: &[Step]) {
    let mut all = vec![
        Step::Exec(MIXED),
        Step::Exec(FILL_MIXED),
        Step::Exec(INTEGERS),
        Step::Exec(FILL_INTEGERS),
    ];
    all.extend_from_slice(steps);
    let compared = differential::compare("group_by", name, &all);
    assert!(
        compared == 0 || compared == all.len(),
        "compared {compared} of {} steps",
        all.len()
    );
}

/// Groups over small integers alone, with sums, counts, a `HAVING` and descending order.
#[test]
fn small_integer_groups_match_sqlite() {
    compare(
        "small-integers",
        &[
            Step::Query("SELECT k, count(*), sum(v), min(v), max(v) FROM h GROUP BY k ORDER BY k"),
            Step::Query("SELECT k, count(*) FROM h GROUP BY k ORDER BY k DESC"),
            Step::Query("SELECT k, avg(v) FROM h GROUP BY k HAVING count(*) > 51 ORDER BY k"),
            Step::Query("SELECT count(*) FROM (SELECT k FROM h GROUP BY k)"),
        ],
    );
}

/// Groups where integers in and out of the direct range, reals equal to integers, NULLs and
/// text share one statement, before and after the awkward rows arrive.
#[test]
fn mixed_key_groups_match_sqlite() {
    compare(
        "mixed-keys",
        &[
            Step::Query("SELECT k, count(*), sum(v) FROM g GROUP BY k ORDER BY k"),
            Step::Exec(AWKWARD),
            // `k + 0.0` names a group by its number, which is the same whichever row of a
            // group mixing 5 and 5.0 an engine takes the key from.
            Step::Query(
                "SELECT k + 0.0, typeof(k) = 'text', count(*), sum(v) FROM g GROUP BY k \
                 ORDER BY 1, 2, 3",
            ),
            Step::Query(
                "SELECT k + 0.0, typeof(k) = 'text', count(*) FROM g GROUP BY k ORDER BY 1, 2, 3",
            ),
            Step::Query("SELECT count(*), sum(c) FROM (SELECT count(*) AS c FROM g GROUP BY k)"),
        ],
    );
}

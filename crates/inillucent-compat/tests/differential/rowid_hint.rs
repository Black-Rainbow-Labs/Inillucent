//! Rowids allocated while the largest key is remembered, graded against pinned SQLite 3.53.4.
//!
//! Invariant: **a rowid the engine picks is the one SQLite picks**: one more than the largest
//! rowid in the table, after every kind of write in the same transaction.
//!
//! Since task-2209 an append through `put` keeps the remembered largest key, where it used to be
//! forgotten after every write, so the next allocation reads it instead of descending the tree.
//! The cases put a write of every kind between two allocations, inside a transaction and outside
//! one: an explicit rowid above the largest, one below it, a delete of the largest row, an update
//! of a rowid, a rollback to a savepoint, and enough rows to split the rightmost leaf.

use inillucent_compat::differential::{self, Step};

/// Runs a scenario against both engines and compares every reply.
///
/// @param name - the scenario's name, which names its database files
/// @param steps - the statements
fn compare(name: &str, steps: &[Step]) {
    let compared = differential::compare("rowid_hint", name, steps);
    assert!(
        compared == 0 || compared == steps.len(),
        "compared {compared} of {} steps",
        steps.len()
    );
}

/// Allocations between writes of every kind, in one transaction and then outside one.
#[test]
fn allocated_rowids_match_sqlite() {
    for wrapped in [true, false] {
        let mut steps = vec![
            Step::Exec("CREATE TABLE r(a TEXT, b INTEGER)"),
            Step::Exec("CREATE INDEX r_b ON r(b)"),
        ];
        if wrapped {
            steps.push(Step::Exec("BEGIN"));
        }
        steps.extend([
            Step::Exec("INSERT INTO r(a, b) VALUES ('one', 1), ('two', 2), ('three', 3)"),
            Step::Exec("INSERT INTO r(rowid, a, b) VALUES (100, 'explicit high', 4)"),
            Step::Exec("INSERT INTO r(a, b) VALUES ('after high', 5)"),
            Step::Exec("INSERT INTO r(rowid, a, b) VALUES (50, 'explicit low', 6)"),
            Step::Exec("INSERT INTO r(a, b) VALUES ('after low', 7)"),
            Step::Exec("DELETE FROM r WHERE rowid = (SELECT max(rowid) FROM r)"),
            Step::Exec("INSERT INTO r(a, b) VALUES ('after delete of the largest', 8)"),
            Step::Exec("UPDATE r SET rowid = 500 WHERE a = 'one'"),
            Step::Exec("INSERT INTO r(a, b) VALUES ('after an update of a rowid', 9)"),
            Step::Exec("SAVEPOINT s"),
            Step::Exec("INSERT INTO r(a, b) VALUES ('rolled back', 10)"),
            Step::Exec("ROLLBACK TO s"),
            Step::Exec("RELEASE s"),
            Step::Exec("INSERT INTO r(a, b) VALUES ('after a rollback to a savepoint', 11)"),
            Step::Exec(
                "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 3000) \
                 INSERT INTO r(a, b) SELECT 'filler ' || x, x FROM c",
            ),
            Step::Exec("INSERT INTO r(a, b) VALUES ('after a split', 12)"),
            Step::Query("SELECT rowid, a, b FROM r WHERE b < 20 OR b > 2990 ORDER BY rowid"),
            Step::Query("SELECT count(*), max(rowid), sum(rowid) FROM r"),
        ]);
        if wrapped {
            steps.push(Step::Exec("COMMIT"));
            steps.push(Step::Query(
                "SELECT count(*), max(rowid), sum(rowid) FROM r",
            ));
        }
        compare(
            if wrapped {
                "in-a-transaction"
            } else {
                "autocommit"
            },
            &steps,
        );
    }
}

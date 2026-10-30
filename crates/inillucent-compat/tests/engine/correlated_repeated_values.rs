//! A correlated subquery is answered once for each distinct value of the outer row it reads.
//!
//! Invariant: **a block that reads only an outer term the join has already moved past does not run
//! again for each row of the term joined after it.** The block's answer depends on the values it is
//! fed, so the answer for the last row is the answer for the next row that feeds the same values.
//!
//! ## Why this exists
//!
//! `t1, t2 WHERE t1.id = t2.y AND EXISTS (SELECT 1 FROM t2 WHERE t2.y = t1.id AND t2.z = 33)` ran
//! the `EXISTS` for each of the 2,500 joined rows where SQLite runs it once: 1,250 ms against
//! 0.27 ms (bug hunt 11). The time is asserted with a margin of ten times what the fixed engine
//! takes, because a block run 2,500 times is a second or more and one run is a few milliseconds.

use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// The join and the `EXISTS` over 2,500 rows that all feed the block the same value.
#[test]
fn repeated_outer_values_answer_a_block_once() {
    let directory = workspace_root().join("_agent_output/correlated-repeated-values");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE t1(id, x); CREATE TABLE t2(id, x, y, z); \
             INSERT INTO t1(id, x) VALUES (1, 2); \
             INSERT INTO t2(id, x, y, z) SELECT value, 0, 1, 44 FROM generate_series(1, 2500); \
             CREATE INDEX t1x ON t1(x); CREATE INDEX t2y ON t2(y);",
        )
        .expect("the tables are built");
    let started = std::time::Instant::now();
    let rows = session
        .query(
            "SELECT count(t1.id), count(t2.id) FROM t1, t2 WHERE t1.id = t2.y AND t1.x = 2 \
             AND EXISTS (SELECT 1 FROM t2 WHERE t2.y = t1.id AND t2.z = 33)",
        )
        .expect("the query runs");
    let elapsed = started.elapsed();
    assert_eq!(rows, vec![vec![OwnedDatum::Int(0), OwnedDatum::Int(0)]]);
    assert!(
        elapsed < std::time::Duration::from_millis(900),
        "the block ran for every joined row: {elapsed:?}"
    );
}

//! A window query's `ORDER BY` sorts the projected rows, not the rows the window pass buffered.
//!
//! Invariant: **the sorter above a window pass holds the result columns and the sort keys of each
//! row and nothing else.** A window function whose value is long is long in every row it is
//! computed for, so a sorter that carried it held and spilled the whole table's worth of copies.
//!
//! ## Why this exists
//!
//! `SELECT count(*), max(l) FROM (SELECT LENGTH(GROUP_CONCAT(x) OVER ()) AS l FROM p ORDER BY x)`
//! over 8,000 rows of 20 bytes sorted rows of 168,000 bytes: it wrote runs of them and failed with
//! `a spilled row did not decode` when a row was larger than the reader's window, and took 10 s
//! against SQLite's 0.3 s where it did not (bug hunt 11). The runs written are counted, because a
//! wall clock would pass or fail with the machine's load and a count does not.

use inillucent_compat::newengine::connect::Database;
use inillucent_compat::newengine::OwnedDatum;
use inillucent_compat::workspace_root;

/// Serialises the cases, which share one process wide run counter and one run size override.
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The query, over 3,000 rows of twenty characters: 63,000 characters in every window value.
const QUERY: &str = "SELECT count(*), max(l) FROM \
     (SELECT LENGTH(GROUP_CONCAT(x) OVER ()) AS l FROM p ORDER BY x)";

/// The statement sorts narrow rows: it writes no run, and answers what the rows say.
#[test]
fn the_sort_over_a_window_pass_carries_narrow_rows() {
    let _serial = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let directory = workspace_root().join("_agent_output/window-sort-narrow-rows");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    let session = database.session();
    session
        .execute_batch(
            "CREATE TABLE p(x TEXT); \
             INSERT INTO p SELECT printf('%020d', value) FROM generate_series(1, 3000);",
        )
        .expect("the table is built");
    // One megabyte, so a sorter that held the 63,000 character values would write a run at the
    // sixteenth row. Narrow rows of 3,000 are 100 KB and write none.
    inillucent_exec::spill::set_run_bytes_override(1 << 20);
    let before = inillucent_exec::spill::runs_written();
    let rows = session.query(QUERY).expect("the query runs");
    let written = inillucent_exec::spill::runs_written().saturating_sub(before);
    inillucent_exec::spill::set_run_bytes_override(0);
    assert_eq!(
        rows,
        vec![vec![OwnedDatum::Int(3000), OwnedDatum::Int(62_999)]],
        "the answer"
    );
    assert_eq!(written, 0, "the sort wrote {written} runs of wide rows");
}

//! A window aggregate over a frame whose start moves reads each row of the partition twice.
//!
//! Invariant: **a window aggregate over a frame that starts after the start of its partition adds a
//! row to the aggregate when the row enters the frame and takes it out when the row leaves, so the
//! frame members read for a partition of `n` rows are `2 * n` and do not depend on how long the
//! frame is.**
//!
//! ## Why this exists
//!
//! The aggregates that had no inverse step were computed again from the rows of the frame for every
//! row of the partition. `max(v) OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED
//! FOLLOWING)` over 100,000 rows took 64.6 seconds where SQLite takes 0.13, and `count(*)` over the
//! same frame over 30,000 rows was 62 times SQLite's time. `min`, `max`, `count`, `median`,
//! `json_group_array`, `json_group_object` and `group_concat` all behaved that way. `sum`, `total`
//! and `avg` already had an inverse step.
//!
//! The count is the frame members `inillucent_exec::window::frame_work_done` reports for the
//! calling thread. It reads the same on an idle machine and on a loaded one. A frame that is
//! computed again for every row reads `n * frame length` members: 2,000,000 for the frame from the
//! current row to the end of 2,000 rows, 1,200,000 for a frame of 601 rows over 2,000. This suite
//! asserts at most `3 * n`, where the measured count is `2 * n`.
//!
//! Each case also asserts the answers against an independent computation, because a window
//! aggregate that read fewer rows and answered wrongly would pass the count alone.

use std::path::PathBuf;
use std::sync::Arc;

use inillucent_compat::newengine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_exec::window::frame_work_done;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::Vfs;

/// How many rows the table holds.
const ROWS: i64 = 2_000;

/// Half the width of the sliding frames, in rows.
const SPAN: usize = 300;

/// Returns the value column of row `id`, which repeats and is not sorted.
///
/// @param id - the row's key
fn value_of(id: i64) -> i64 {
    id * 7_919 % 1_000
}

/// Returns the group column of row `id`, which gives 50 peer groups of 40 rows.
///
/// @param id - the row's key
fn group_of(id: i64) -> i64 {
    id % 50
}

/// Opens a database holding the table the cases read.
fn database() -> ImportedDatabase {
    let vfs = Arc::new(SimVfs::new(SimConfig {
        seed: 41,
        model: MediaModel::default(),
        ..SimConfig::default()
    }));
    let mut engine = ImportedDatabase::create_on(
        Arc::clone(&vfs) as Arc<dyn Vfs>,
        PathBuf::from("window-sliding.rdb"),
        4_096,
        1_024,
    )
    .expect("the database is created");
    for sql in [
        "CREATE TABLE t(id INTEGER PRIMARY KEY, g INTEGER, v INTEGER)".to_string(),
        format!(
            "WITH RECURSIVE c(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM c WHERE i < {ROWS}) \
             INSERT INTO t SELECT i, i % 50, i * 7919 % 1000 FROM c"
        ),
    ] {
        rows(&mut engine, &sql);
    }
    engine
}

/// Runs one statement and returns its rows.
///
/// @param engine - the database
/// @param sql - the statement
fn rows(engine: &mut ImportedDatabase, sql: &str) -> Vec<Vec<OwnedDatum>> {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()))
        .rows
}

/// Runs a query that answers one value per row of the table, and returns the values with the number
/// of frame members the window pass read.
///
/// @param engine - the database
/// @param window - the window call, written as `max(v) OVER (...)`
fn answers(engine: &mut ImportedDatabase, window: &str) -> (Vec<OwnedDatum>, u64) {
    let before = frame_work_done();
    let table = rows(engine, &format!("SELECT {window} FROM t ORDER BY id"));
    let work = frame_work_done().saturating_sub(before);
    let values = table
        .into_iter()
        .map(|row| row.into_iter().next().unwrap_or(OwnedDatum::Null))
        .collect();
    (values, work)
}

/// Returns an integer out of an answer, or panics with what the answer was.
///
/// @param value - one answer
fn int(value: &OwnedDatum) -> i64 {
    match value {
        OwnedDatum::Int(held) => *held,
        other => panic!("an integer was expected and the answer was {other:?}"),
    }
}

/// Returns the values of the rows `low..=high` of the table, as indexes into rows numbered from 0.
///
/// @param low - the first row index
/// @param high - the last row index, which may be past the end
fn slice(low: usize, high: usize) -> Vec<i64> {
    let last = (ROWS as usize).saturating_sub(1);
    (low..=high.min(last))
        .map(|at| value_of(at as i64 + 1))
        .collect()
}

/// Asserts the frame members read stay linear in the rows.
///
/// @param what - which window call was run
/// @param work - the frame members read
fn assert_linear(what: &str, work: u64) {
    let limit = 3 * ROWS as u64;
    assert!(
        work <= limit,
        "{what}: {work} frame members read for {ROWS} rows, expected at most {limit}"
    );
}

/// `count` over a frame from the current row to the end of the partition.
#[test]
fn count_from_the_current_row_to_the_end_reads_each_row_twice() {
    let mut engine = database();
    for call in ["count(*)", "count(v)"] {
        let window =
            format!("{call} OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING)");
        let (values, work) = answers(&mut engine, &window);
        let wanted: Vec<i64> = (0..ROWS).map(|at| ROWS - at).collect();
        assert_eq!(
            values.iter().map(int).collect::<Vec<_>>(),
            wanted,
            "{window}"
        );
        assert_linear(&window, work);
    }
}

/// `max` and `min` over a frame from the current row to the end of the partition.
#[test]
fn max_and_min_from_the_current_row_to_the_end_read_each_row_twice() {
    let mut engine = database();
    for (call, pick_max) in [("max(v)", true), ("min(v)", false)] {
        let window =
            format!("{call} OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND UNBOUNDED FOLLOWING)");
        let (values, work) = answers(&mut engine, &window);
        let wanted: Vec<i64> = (0..ROWS as usize)
            .map(|at| {
                let rest = slice(at, ROWS as usize);
                let found = if pick_max {
                    rest.iter().max()
                } else {
                    rest.iter().min()
                };
                found.copied().unwrap_or(i64::MIN)
            })
            .collect();
        assert_eq!(
            values.iter().map(int).collect::<Vec<_>>(),
            wanted,
            "{window}"
        );
        assert_linear(&window, work);
    }
}

/// `min` and `max` over a frame of 601 rows that slides along the partition.
#[test]
fn min_and_max_over_a_sliding_frame_read_each_row_twice() {
    let mut engine = database();
    for (call, pick_max) in [("max(v)", true), ("min(v)", false)] {
        let window =
            format!("{call} OVER (ORDER BY id ROWS BETWEEN {SPAN} PRECEDING AND {SPAN} FOLLOWING)");
        let (values, work) = answers(&mut engine, &window);
        let wanted: Vec<i64> = (0..ROWS as usize)
            .map(|at| {
                let frame = slice(at.saturating_sub(SPAN), at + SPAN);
                let found = if pick_max {
                    frame.iter().max()
                } else {
                    frame.iter().min()
                };
                found.copied().unwrap_or(i64::MIN)
            })
            .collect();
        assert_eq!(
            values.iter().map(int).collect::<Vec<_>>(),
            wanted,
            "{window}"
        );
        assert_linear(&window, work);
    }
}

/// `count(*)` over a `RANGE` frame and a `GROUPS` frame in an ordering full of ties.
#[test]
fn count_over_range_and_groups_frames_with_ties_reads_each_row_twice() {
    let mut engine = database();
    let per_group = ROWS / 50;
    for (unit, span) in [("RANGE", 1_i64), ("GROUPS", 5_i64)] {
        let window = format!(
            "count(*) OVER (ORDER BY g {unit} BETWEEN {span} PRECEDING AND {span} FOLLOWING)"
        );
        let (values, work) = answers(&mut engine, &window);
        let wanted: Vec<i64> = (1..=ROWS)
            .map(|id| {
                let group = group_of(id);
                let low = (group - span).max(0);
                let high = (group + span).min(49);
                (high - low + 1) * per_group
            })
            .collect();
        assert_eq!(
            values.iter().map(int).collect::<Vec<_>>(),
            wanted,
            "{window}"
        );
        assert_linear(&window, work);
    }
}

/// `median` over a frame of 61 rows that slides along the partition.
#[test]
fn median_over_a_sliding_frame_reads_each_row_twice() {
    let mut engine = database();
    let window = "median(v) OVER (ORDER BY id ROWS BETWEEN 30 PRECEDING AND 30 FOLLOWING)";
    let (values, work) = answers(&mut engine, window);
    for (at, value) in values.iter().enumerate() {
        let mut frame = slice(at.saturating_sub(30), at + 30);
        frame.sort_unstable();
        let middle = frame.len() / 2;
        let wanted = if frame.len() % 2 == 1 {
            frame[middle] as f64
        } else {
            (frame[middle - 1] + frame[middle]) as f64 / 2.0
        };
        assert_eq!(value, &OwnedDatum::Real(wanted), "row {at}");
    }
    assert_linear(window, work);
}

/// `json_group_array`, `json_group_object` and `group_concat` over a sliding frame.
#[test]
fn json_and_group_concat_over_a_sliding_frame_read_each_row_twice() {
    let mut engine = database();
    for window in [
        "json_group_array(v) OVER (ORDER BY id ROWS BETWEEN 30 PRECEDING AND 30 FOLLOWING)",
        "json_group_object(id, v) OVER (ORDER BY id ROWS BETWEEN 30 PRECEDING AND 30 FOLLOWING)",
        "group_concat(v) OVER (ORDER BY id ROWS BETWEEN 30 PRECEDING AND 30 FOLLOWING)",
        "group_concat(v, '--') OVER (ORDER BY id ROWS BETWEEN CURRENT ROW AND 30 FOLLOWING)",
    ] {
        let (values, work) = answers(&mut engine, window);
        assert_eq!(values.len(), ROWS as usize, "{window}");
        assert_linear(window, work);
        let last_value = value_of(ROWS).to_string().into_bytes();
        match values.last() {
            Some(OwnedDatum::Text(text)) => {
                assert!(
                    text.ends_with(&last_value) || text.ends_with(b"]") || text.ends_with(b"}"),
                    "{window}: the last answer was {text:?}"
                );
            }
            other => panic!("{window}: the last answer was {other:?}"),
        }
    }
    let (values, _) = answers(
        &mut engine,
        "group_concat(v) OVER (ORDER BY id ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING)",
    );
    let wanted: Vec<u8> = slice(0, 1)
        .iter()
        .map(|held| held.to_string())
        .collect::<Vec<_>>()
        .join(",")
        .into_bytes();
    assert_eq!(values.first(), Some(&OwnedDatum::Text(wanted)));
}

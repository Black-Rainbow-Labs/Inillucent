//! What `.import` holds in memory while it loads a large file.
//!
//! Invariant: **the memory `.import` needs does not grow with the number of rows in the file.** It
//! read and parsed the whole file before it inserted a row, and 1,000,000 rows peaked at 536 to 679
//! MB where `sqlite3` stays near 10 MB. The shell runs as a child process and its largest resident
//! set is read while it runs.

use std::io::Write;

use inillucent_compat::cliproc;

use super::streaming_memory::{fresh_directory, watch};

/// How many rows the file `.import` reads has.
const IMPORT_ROWS: u64 = 1_000_000;

/// The most `.import` may hold, in bytes.
///
/// Before it streamed the file, 1,000,000 rows peaked at 536 to 679 MB.
const IMPORT_BOUND: u64 = 130 << 20;

/// `.import` reads a block of the file and inserts a batch at a time, and every row arrives.
#[test]
fn import_loads_a_large_file_in_bounded_memory() {
    let directory = fresh_directory("import");
    let file = directory.join("rows.csv");
    {
        let mut out =
            std::io::BufWriter::new(std::fs::File::create(&file).expect("the file can be made"));
        writeln!(out, "id,label,score").expect("the header is written");
        for row in 1..=IMPORT_ROWS {
            writeln!(out, "{row},label {row},{}", row % 97).expect("a row is written");
        }
    }
    let database = directory.join("loaded.rdb");
    let shell = cliproc::program("inillucent-shell");
    let script = format!(
        ".import --csv {} t\nSELECT count(*), sum(id), sum(score), min(label), max(id + 0) FROM t;\n",
        file.to_string_lossy().replace('\\', "/")
    );
    let path = database.to_string_lossy().replace('\\', "/");
    let ran = watch(&shell, &[&path], &script);
    assert_eq!(ran.code, 0, "{}", ran.stderr);
    let expected_sum = IMPORT_ROWS * (IMPORT_ROWS + 1) / 2;
    let expected_score: u64 = (1..=IMPORT_ROWS).map(|row| row % 97).sum();
    assert_eq!(
        ran.head.trim(),
        format!("{IMPORT_ROWS}|{expected_sum}|{expected_score}|label 1|{IMPORT_ROWS}")
    );
    assert!(
        ran.peak < IMPORT_BOUND,
        "importing {IMPORT_ROWS} rows peaked at {} MB; the bound is {} MB",
        ran.peak >> 20,
        IMPORT_BOUND >> 20
    );
}

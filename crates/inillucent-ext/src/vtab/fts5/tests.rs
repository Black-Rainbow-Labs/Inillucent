//! Tests of the FTS5 module's argument parsing, totals and run appender.
//!
//! Invariant: the bytes a bulk load appends to a doclist are the bytes the entry appender writes,
//! and the arguments of `CREATE VIRTUAL TABLE` split into columns and options the way SQLite
//! splits them.

use super::doclist::append_doclist_entry;
use super::*;

/// The totals round-trip.
#[test]
fn the_totals_round_trip() {
    let totals = Totals {
        rows: 17,
        tokens: vec![100, 250],
    };
    assert_eq!(Totals::decode(&totals.encode()), totals);
}

/// A column list and the options are told apart by the equals sign.
#[test]
fn columns_and_options_are_told_apart() {
    let options = parse_options(&[
        b"title".to_vec(),
        b"body".to_vec(),
        b"meta UNINDEXED".to_vec(),
        b"tokenize = 'ascii'".to_vec(),
    ])
    .expect("parses");
    assert_eq!(options.columns.len(), 3);
    assert_eq!(options.columns[0].name, b"title");
    assert!(!options.columns[1].unindexed);
    assert!(options.columns[2].unindexed);
    assert_eq!(options.tokenizer, vec![b"ascii".to_vec()]);
}

/// A quoted column name keeps its spaces and loses its quotes.
#[test]
fn a_quoted_column_is_unquoted() {
    let options = parse_options(&[b"\"a b\"".to_vec()]).expect("parses");
    assert_eq!(options.columns[0].name, b"a b");
    let flagged = parse_options(&[b"\"a b\" UNINDEXED".to_vec()]).expect("parses");
    assert_eq!(flagged.columns[0].name, b"a b");
    assert!(flagged.columns[0].unindexed);
}

/// A table with no columns is refused.
#[test]
fn a_table_needs_a_column() {
    assert!(parse_options(&[b"tokenize = 'ascii'".to_vec()]).is_err());
}

/// The run appender writes the bytes the entry appender would.
///
/// **Two ways to write a doclist entry is a format with two definitions**,
/// and the bulk-load path takes the one that never builds an entry - so if
/// the two ever disagree, an index built by a bulk load and one built a row
/// at a time hold different bytes for the same documents, and only one of
/// them decodes.
///
/// The cases are the shapes that separate them: one column and one
/// position, one column and several, two columns, a gap between column
/// numbers, and a position of zero.
#[test]
fn a_run_appends_the_bytes_the_entry_would() {
    let runs: Vec<Vec<(Vec<u8>, usize, u32)>> = vec![
        vec![(b"a".to_vec(), 0, 0)],
        vec![(b"a".to_vec(), 0, 3)],
        vec![
            (b"a".to_vec(), 0, 0),
            (b"a".to_vec(), 0, 3),
            (b"a".to_vec(), 0, 9),
        ],
        vec![
            (b"a".to_vec(), 0, 2),
            (b"a".to_vec(), 1, 0),
            (b"a".to_vec(), 1, 1),
        ],
        vec![(b"a".to_vec(), 0, 1), (b"a".to_vec(), 3, 7)],
        vec![
            (b"a".to_vec(), 1, 0),
            (b"a".to_vec(), 1, 4),
            (b"a".to_vec(), 2, 2),
            (b"a".to_vec(), 2, 3),
        ],
    ];
    for run in &runs {
        for delta in [1i64, 7, 300] {
            let mut from_run = Vec::new();
            append_run(&mut from_run, delta, run);
            let mut from_entry = Vec::new();
            append_doclist_entry(
                &mut from_entry,
                delta,
                &DocEntry {
                    rowid: 0,
                    columns: columns_of(run),
                },
            );
            assert_eq!(
                from_run, from_entry,
                "the two appenders disagree for {run:?} at delta {delta}"
            );
        }
    }
}

/// A run groups into the per-column form an entry holds.
#[test]
fn a_run_groups_by_column() {
    let run = vec![
        (b"a".to_vec(), 0, 1),
        (b"a".to_vec(), 0, 4),
        (b"a".to_vec(), 2, 0),
    ];
    assert_eq!(columns_of(&run), vec![(0, vec![1, 4]), (2, vec![0])]);
    assert!(columns_of(&[]).is_empty());
}

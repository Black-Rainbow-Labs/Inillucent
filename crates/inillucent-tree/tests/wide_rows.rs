//! Rows of several hundred integer columns, appended one at a time, share leaves.
//!
//! Invariant: **a table of 450 to 790 integer columns loaded by appends takes a small multiple of
//! the leaves a bulk build of the same rows takes.** The leaves of such a table spend most of their
//! page on the column directory and the class arrays, and a row waiting in the delta area is several
//! kilobytes, so the rule that asks a packed leaf for a third of the page free on top of the arriving
//! row could never be met and every full leaf split.
//!
//! ## Why this exists
//!
//! 300 rows of 600 integer columns took 146 leaves appended against 13 built in bulk (bug hunt 11),
//! and a scan of the table read 11 times what SQLite's read. The leaf count is asserted, because it
//! is what decides how many pages a scan, an update and a `VACUUM` touch, and it does not depend on
//! how loaded the machine is.

use inillucent_pool::{Database, Options};
use inillucent_tree::datum::Datum;
use inillucent_tree::types::{ColumnSpec, PhysicalType};
use inillucent_tree::write::NoLog;
use inillucent_tree::PagedTree;
use inillucent_vfs::{DbPath, MemoryVfs};

/// Returns a key column and `width` integer columns.
///
/// @param width - how many integer columns follow the key
fn columns_of(width: usize) -> Vec<ColumnSpec> {
    let mut columns = vec![ColumnSpec::key(PhysicalType::Int64)];
    columns.extend((0..width).map(|_| ColumnSpec::new(PhysicalType::Int64)));
    columns
}

/// Returns the leaves of a bulk build and of 300 appended rows of `width` integer columns.
///
/// @param width - how many integer columns follow the key
fn leaves_for(width: usize) -> (u64, u64) {
    let rows: Vec<Vec<Datum<'static>>> = (0..300i64)
        .map(|row| {
            let mut values = vec![Datum::Int(row + 1)];
            values.extend((0..width).map(|column| Datum::Int((row * 7 + column as i64) % 100)));
            values
        })
        .collect();
    let vfs = MemoryVfs::new();
    let path = DbPath::new("wide-rows.rdb");
    let mut database = Database::create(
        &vfs,
        &path,
        Options::default().with_page_size(32_768).with_frames(512),
    )
    .expect("a database");
    let bulk = PagedTree::bulk_build(&mut database, 11, columns_of(width), 1, &rows)
        .expect("a bulk build");
    let mut appended = PagedTree::bulk_build(&mut database, 12, columns_of(width), 1, &rows[..0])
        .expect("an empty tree");
    let mut log = NoLog::default();
    for row in &rows {
        appended
            .insert(&mut database, &mut log, row)
            .expect("the insert");
    }
    (bulk.leaf_count(), appended.leaf_count())
}

/// Appended rows of 540 and 600 columns take no more than four times the leaves of a bulk build.
#[test]
fn appended_wide_rows_share_leaves() {
    for width in [540usize, 600] {
        let (bulk, appended) = leaves_for(width);
        assert!(
            appended <= bulk.saturating_mul(4),
            "{width} columns: {appended} leaves appended, {bulk} built in bulk"
        );
    }
}

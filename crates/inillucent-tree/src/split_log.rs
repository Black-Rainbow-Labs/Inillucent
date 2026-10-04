//! The logical split record: what a leaf split puts in the log, and how a replay
//! rebuilds each of the three pages from it.
//!
//! Invariant: **a replay produces the bytes the split produced, one page at a
//! time, from that page's own state and the record, and from nothing else.**
//! [`left_image`] and [`right_image`] are `LeafBuilder::encode_rows` over the
//! rows the write path packed, with a spiller given as the write path gives
//! one, and [`parent_image`] is the separator insertion
//! `PagedTree::insert_separator` makes through the same [`read_interior`].
//! `PagedTree::split_carrying` compares the three pages it installs with what
//! these functions produce in every debug build, which is the build every
//! test runs in, so a change to one side that the other does not follow fails
//! the first test that splits a leaf.
//!
//! ## Why each page stands alone
//!
//! A split rewrites three pages, and an eviction writes a dirty page to the
//! data file one page at a time. So after a crash the file can hold any one of
//! them at its state after the split while the other two are still at their
//! state before it, and the page LSN rule then replays the record for those
//! two only. A record that rebuilt the right page from the left page's rows
//! would have nothing to rebuild it from once the left page was in the file
//! without those rows. So:
//!
//! - the left page is rebuilt from its own live rows, which redo order says are
//!   the rows the split saw when the page's stamp is below the record's;
//! - the right page is rebuilt from the rows the record carries, and reads no
//!   page;
//! - the parent is rebuilt from its own separators and children, plus the
//!   separator and the child the record names.
//!
//! ## What the spiller is for at replay
//!
//! A split is only logged this way when it moved no value out of line. The
//! builder still decides how to lay out a column by whether a spiller was
//! given, because a value over the spill threshold is laid out differently
//! when it can go out of line, so a replay passes one that refuses: the layout
//! decisions are the write path's, and a replay that was asked to spill
//! anything is replaying a record the write path would not have written.

use inillucent_base::error::corrupt;
use inillucent_base::DbResult;
use inillucent_pool::extent::ExtentRef;
use inillucent_pool::interior::{InteriorBuilder, InteriorRef};
use inillucent_pool::page;
use inillucent_pool::{PageId, Pool, Swip};

use crate::datum::Datum;
use crate::leaf::{LeafBuilder, LeafRef, RowSlice, Spill};

/// A spiller that refuses, for a replay that must not move any value.
///
/// See the module comment: its presence keeps the layout identical to the
/// write path's, and being asked anything means the record does not describe
/// the rows it is being replayed over.
struct Refuse;

impl Spill for Refuse {
    fn spill(&mut self, row: usize, column: usize, _value: &[u8]) -> DbResult<ExtentRef> {
        Err(corrupt(format!(
            "replaying a logical split was asked to move row {row}, column {column} out of \
             line, and the write path only logs a split this way when it moved nothing"
        )))
    }
}

/// Encodes a split's right half for the record: a `u32` row count, then each
/// row as a `u32` length and its values in the tagged encoding.
///
/// @param rows - the right half's rows, in key order
pub fn encode_rows<'d, R: AsRef<[Datum<'d>]>>(rows: &[R]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
    let mut row_bytes = Vec::new();
    for row in rows {
        row_bytes.clear();
        for value in row.as_ref() {
            value.encode_tagged(&mut row_bytes);
        }
        out.extend_from_slice(&(row_bytes.len() as u32).to_le_bytes());
        out.extend_from_slice(&row_bytes);
    }
    out
}

/// Decodes what [`encode_rows`] wrote.
///
/// Every length is checked against the bytes that are there, because these
/// bytes come out of a log a crash may have damaged past its checksum's reach.
///
/// @param bytes - the record's rows
pub fn decode_rows(bytes: &[u8]) -> DbResult<Vec<Vec<Datum<'_>>>> {
    let (count, mut rest) = take_u32(bytes)?;
    let mut rows = Vec::with_capacity((count as usize).min(rest.len()));
    for _ in 0..count {
        let (length, after) = take_u32(rest)?;
        let row = after
            .get(..length as usize)
            .ok_or_else(|| corrupt("a logical split's row runs past the record"))?;
        rest = after.get(length as usize..).unwrap_or(&[]);
        let mut values = Vec::new();
        let mut at = 0usize;
        while at < row.len() {
            let (value, width) = Datum::decode_tagged(row.get(at..).unwrap_or(&[]))?;
            values.push(value);
            at = at.saturating_add(width);
        }
        rows.push(values);
    }
    if !rest.is_empty() {
        return Err(corrupt(format!(
            "a logical split's rows end {} bytes before the record does",
            rest.len()
        )));
    }
    Ok(rows)
}

/// Splits a little endian `u32` off the front of a buffer.
///
/// @param bytes - the buffer
fn take_u32(bytes: &[u8]) -> DbResult<(u32, &[u8])> {
    let head = bytes
        .get(..4)
        .ok_or_else(|| corrupt("a logical split's rows end inside a length"))?;
    let mut raw = [0u8; 4];
    raw.copy_from_slice(head);
    Ok((u32::from_le_bytes(raw), bytes.get(4..).unwrap_or(&[])))
}

/// Writes the two header fields a split's halves take from the leaf they came
/// from, which a fresh pack does not know.
///
/// @param image - the packed page
/// @param right - the page it points at
/// @param max_cts - the commit watermark the split leaf carried
fn finish_leaf(image: &mut [u8], right: PageId, max_cts: u64) -> DbResult<()> {
    page::set_right(image, right)?;
    crate::page::write_u64(image, crate::leaf::leaf_header::MAX_CTS, max_cts)
}

/// Rebuilds the left half of a split from the leaf it was split from.
///
/// `None` when the leaf holds no more than `kept` live rows, which means the
/// page is not the one the split read: a split leaves at least one row on each
/// side.
///
/// @param builder - the tree's leaf builder
/// @param leaf - the leaf as the split saw it, parsed with its tree's
///   collations and directions
/// @param kept - how many live rows the left half keeps
/// @param right - the new right half's page
/// @param max_cts - the commit watermark the leaf carried
pub fn left_image(
    builder: &LeafBuilder,
    leaf: &LeafRef<'_>,
    kept: usize,
    right: PageId,
    max_cts: u64,
) -> DbResult<Option<Vec<u8>>> {
    let source = leaf.live_order()?.materialise()?;
    if kept == 0 || kept >= source.len() {
        return Ok(None);
    }
    let mut image = builder.encode_rows(&source, 0, kept, Some(&mut Refuse))?;
    finish_leaf(&mut image, right, max_cts)?;
    Ok(Some(image))
}

/// Packs the right half of a split from its rows.
///
/// @param builder - the tree's leaf builder
/// @param rows - the right half's rows, in key order
/// @param right_sibling - the page the split leaf pointed at
/// @param max_cts - the commit watermark the split leaf carried
pub fn right_image<'d, R: AsRef<[Datum<'d>]>>(
    builder: &LeafBuilder,
    rows: &[R],
    right_sibling: PageId,
    max_cts: u64,
) -> DbResult<Vec<u8>> {
    let mut image = builder.encode_rows(&RowSlice(rows), 0, rows.len(), Some(&mut Refuse))?;
    finish_leaf(&mut image, right_sibling, max_cts)?;
    Ok(image)
}

/// Returns an interior page's separators, children and level.
///
/// Children come back as page ids whether the resident frame holds them
/// swizzled or not, so the write path and a replay read the same list.
///
/// @param pool - the buffer pool
/// @param page - the interior page
pub fn read_interior(pool: &Pool, page: PageId) -> DbResult<(Vec<Vec<u8>>, Vec<PageId>, u16)> {
    let guard = pool.fetch(page)?;
    let interior = InteriorRef::parse(&guard)?;
    let mut separators = Vec::with_capacity(interior.count());
    for slot in 0..interior.count() {
        separators.push(interior.key(slot)?.to_vec());
    }
    let mut children = Vec::with_capacity(interior.children());
    for child in 0..interior.children() {
        children.push(pool.page_of_swip(interior.swip(child)?)?);
    }
    Ok((separators, children, interior.level()))
}

/// Rebuilds a parent with a separator and a right child added after `left`.
///
/// `None` when the parent does not route to `left`, or when the separator
/// does not fit: the split only logs this record when it did, so either means
/// the page is not the one the split changed.
///
/// @param pool - the buffer pool holding the parent
/// @param page_size - the database's page size
/// @param tree - the tree the page belongs to
/// @param parent - the interior page
/// @param left - the child that was split
/// @param separator - the right half's first key, encoded
/// @param right - the new child
pub fn parent_image(
    pool: &Pool,
    page_size: usize,
    tree: u64,
    parent: PageId,
    left: PageId,
    separator: &[u8],
    right: PageId,
) -> DbResult<Option<Vec<u8>>> {
    let (mut separators, mut children, level) = read_interior(pool, parent)?;
    let Some(position) = children.iter().position(|page| *page == left) else {
        return Ok(None);
    };
    separators.insert(position, separator.to_vec());
    children.insert(position.saturating_add(1), right);
    let builder = InteriorBuilder::new(page_size, tree, level)?;
    let keys: Vec<&[u8]> = separators.iter().map(Vec::as_slice).collect();
    if !builder.fits(&keys) {
        return Ok(None);
    }
    let swips: Vec<Swip> = children
        .iter()
        .map(|page| Swip::unswizzled(*page))
        .collect();
    Ok(Some(builder.build(&keys, &swips)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows a record carries come back as the values that went in.
    #[test]
    fn rows_round_trip() {
        let rows = vec![
            vec![Datum::Int(7), Datum::Text(b"seven"), Datum::Null],
            vec![
                Datum::Real(-0.0),
                Datum::Blob(&[0, 1, 2]),
                Datum::Int(i64::MIN),
            ],
        ];
        let bytes = encode_rows(&rows);
        let back = decode_rows(&bytes).unwrap();
        assert_eq!(back.len(), 2);
        for (got, want) in back.iter().zip(rows.iter()) {
            assert_eq!(got.len(), want.len());
            for (a, b) in got.iter().zip(want.iter()) {
                let (mut x, mut y) = (Vec::new(), Vec::new());
                a.encode_tagged(&mut x);
                b.encode_tagged(&mut y);
                assert_eq!(x, y);
            }
        }
    }

    /// Every truncation of the rows, and trailing bytes after them, are refused.
    #[test]
    fn damaged_rows_are_refused() {
        let rows = vec![vec![Datum::Int(1), Datum::Text(b"one")]];
        let bytes = encode_rows(&rows);
        for cut in 0..bytes.len() {
            assert!(decode_rows(&bytes[..cut]).is_err(), "cut at {cut} decoded");
        }
        let mut longer = bytes.clone();
        longer.push(0);
        assert!(decode_rows(&longer).is_err());
    }

    /// The refusing spiller refuses.
    #[test]
    fn the_replay_spiller_refuses() {
        assert!(Refuse.spill(1, 2, b"x").is_err());
    }
}

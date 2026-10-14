//! The encoding of undo records that an open transaction has moved to a temporary file.
//!
//! Invariant: **a record that is written to the file and read back is equal to the record that
//! was written, down to which records share one copy of a leaf.** A rollback reads these bytes
//! once, newest record first, and writes the rows they hold back into the tree, so a record
//! that came back different would put a different row back.
//!
//! ## Why a copy of a leaf is written once for all the records that name it
//!
//! An update that changes one column of every row of a table records, for each leaf, one copy
//! of the leaf and a place in that copy for each changed row (`FromPage`). A hundred rows of
//! one leaf are a hundred records that share one `Rc<[u8]>` of 32 KiB. Writing the copy again
//! for each record would turn 32 KiB held once into 3 MiB written, so each chunk starts with
//! the distinct copies its records name and the records refer to them by position. Records of
//! different chunks that shared a copy get one each, which costs at most one extra copy of a
//! leaf at each chunk boundary.
//!
//! ## What is not in the file
//!
//! No header, no checksum and no version. The file is made by one transaction of one
//! connection, is read by the same transaction, and is deleted when the connection closes the
//! file or the process ends. A format that outlived the process would be a format somebody has
//! to keep compatible.

use std::collections::HashMap;
use std::rc::Rc;

use inillucent_base::{DbError, DbResult, PrimaryCode};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::Located;

use crate::engine::write::{Before, FromPage, KeyCopy};

/// The bytes a record occupies in memory, counted once for a copy of a leaf that the record
/// shares with the one before it.
///
/// An estimate: it counts the record's own size, the bytes of its values and 24 bytes for each
/// value's header, and it does not count the allocator's rounding. It decides when to spill, and
/// being 10 percent off moves the moment of the spill by 10 percent.
///
/// @param record - the record
/// @param last_page - the address of the copy of a leaf the previous record named, updated here
pub(crate) fn resident_bytes(record: &Before, last_page: &mut usize) -> usize {
    let mut bytes = std::mem::size_of::<Before>();
    if let Some(row) = &record.row {
        bytes = bytes.saturating_add(values_bytes(row));
    }
    match &record.key {
        KeyCopy::One(value) => {
            bytes = bytes.saturating_add(values_bytes(std::slice::from_ref(value)))
        }
        KeyCopy::Two(pair) => {
            bytes = bytes.saturating_add(values_bytes(std::slice::from_ref(&pair.0)));
            bytes = bytes.saturating_add(values_bytes(std::slice::from_ref(&pair.1)));
        }
        KeyCopy::Many(values) => bytes = bytes.saturating_add(values_bytes(values)),
        KeyCopy::Empty | KeyCopy::Rowids { .. } => {}
    }
    match record.page.as_deref() {
        Some(FromPage::One(page, _)) => bytes = bytes.saturating_add(page_bytes(page, last_page)),
        Some(FromPage::Run(page, located)) => {
            bytes = bytes.saturating_add(page_bytes(page, last_page));
            bytes = bytes.saturating_add(located.len().saturating_mul(16));
        }
        None => {}
    }
    bytes
}

/// Returns the bytes of a copy of a leaf, or none when the previous record named the same copy.
///
/// @param page - the copy
/// @param last_page - the address of the copy the previous record named, updated here
fn page_bytes(page: &Rc<[u8]>, last_page: &mut usize) -> usize {
    let address = page.as_ptr() as usize;
    if *last_page == address {
        return 0;
    }
    *last_page = address;
    page.len()
}

/// Returns the bytes a row's values occupy in memory.
///
/// @param values - the values
fn values_bytes(values: &[OwnedDatum]) -> usize {
    values.iter().fold(0usize, |sum, value| {
        let payload = match value {
            OwnedDatum::Text(bytes) | OwnedDatum::Blob(bytes) => bytes.len(),
            OwnedDatum::Null | OwnedDatum::Int(_) | OwnedDatum::Real(_) => 0,
        };
        sum.saturating_add(24).saturating_add(payload)
    })
}

/// Writes records as bytes.
///
/// @param records - the records, oldest first
pub(crate) fn encode_records(records: &[Before]) -> Vec<u8> {
    let mut pages: Vec<Rc<[u8]>> = Vec::new();
    let mut page_numbers: HashMap<usize, u32> = HashMap::new();
    let mut body: Vec<u8> = Vec::new();
    body.extend_from_slice(&to_u32(records.len()).to_le_bytes());
    for record in records {
        body.push(record.schema);
        body.push(u8::from(record.sealed));
        body.extend_from_slice(&record.tree.to_le_bytes());
        encode_key(&record.key, &mut body);
        match &record.row {
            Some(row) => {
                body.push(1);
                encode_values(row, &mut body);
            }
            None => body.push(0),
        }
        encode_page(
            record.page.as_deref(),
            &mut pages,
            &mut page_numbers,
            &mut body,
        );
    }
    let mut out: Vec<u8> = Vec::with_capacity(body.len().saturating_add(64));
    out.extend_from_slice(&to_u32(pages.len()).to_le_bytes());
    for page in &pages {
        out.extend_from_slice(&to_u32(page.len()).to_le_bytes());
        out.extend_from_slice(page);
    }
    out.extend_from_slice(&body);
    out
}

/// Narrows a count to the four bytes the file keeps it in.
///
/// @param count - the count
fn to_u32(count: usize) -> u32 {
    u32::try_from(count).unwrap_or(u32::MAX)
}

/// Writes a record's key.
///
/// @param key - the key
/// @param out - where the bytes go
fn encode_key(key: &KeyCopy, out: &mut Vec<u8>) {
    match key {
        KeyCopy::Empty => out.push(0),
        KeyCopy::One(value) => {
            out.push(1);
            value.borrow().encode_tagged(out);
        }
        KeyCopy::Two(pair) => {
            out.push(2);
            pair.0.borrow().encode_tagged(out);
            pair.1.borrow().encode_tagged(out);
        }
        KeyCopy::Rowids { first, last } => {
            out.push(3);
            out.extend_from_slice(&first.to_le_bytes());
            out.extend_from_slice(&last.to_le_bytes());
        }
        KeyCopy::Many(values) => {
            out.push(4);
            encode_values(values, out);
        }
    }
}

/// Writes a count of values and the values.
///
/// @param values - the values
/// @param out - where the bytes go
fn encode_values(values: &[OwnedDatum], out: &mut Vec<u8>) {
    out.extend_from_slice(&to_u32(values.len()).to_le_bytes());
    for value in values {
        value.borrow().encode_tagged(out);
    }
}

/// Writes the place a record names in a copy of a leaf, adding the copy to the chunk's table.
///
/// @param page - the record's place, or `None`
/// @param pages - the distinct copies named so far, in the order they were first named
/// @param numbers - each copy's position in `pages`, by address
/// @param out - where the bytes go
fn encode_page(
    page: Option<&FromPage>,
    pages: &mut Vec<Rc<[u8]>>,
    numbers: &mut HashMap<usize, u32>,
    out: &mut Vec<u8>,
) {
    let mut number_of = |copy: &Rc<[u8]>| -> u32 {
        let address = copy.as_ptr() as usize;
        *numbers.entry(address).or_insert_with(|| {
            pages.push(Rc::clone(copy));
            to_u32(pages.len().saturating_sub(1))
        })
    };
    match page {
        None => out.push(0),
        Some(FromPage::One(copy, located)) => {
            out.push(1);
            out.extend_from_slice(&number_of(copy).to_le_bytes());
            encode_located(*located, out);
        }
        Some(FromPage::Run(copy, located)) => {
            out.push(2);
            out.extend_from_slice(&number_of(copy).to_le_bytes());
            out.extend_from_slice(&to_u32(located.len()).to_le_bytes());
            for place in located.iter() {
                encode_located(*place, out);
            }
        }
    }
}

/// Writes where a row lies in a leaf.
///
/// @param located - the place
/// @param out - where the bytes go
fn encode_located(located: Located, out: &mut Vec<u8>) {
    let (kind, at) = match located {
        Located::Sorted(at) => (0u8, at as u64),
        Located::Delta(at) => (1u8, at as u64),
        Located::Absent => (2u8, 0u64),
    };
    out.push(kind);
    out.extend_from_slice(&at.to_le_bytes());
}

/// Reads bytes written by [`encode_records`] back into records.
///
/// @param bytes - the bytes of one chunk
pub(crate) fn decode_records(bytes: &[u8]) -> DbResult<Vec<Before>> {
    let mut reader = Reader { bytes, at: 0 };
    let page_count = reader.u32()? as usize;
    let mut pages: Vec<Rc<[u8]>> = Vec::with_capacity(page_count.min(4096));
    for _ in 0..page_count {
        let length = reader.u32()? as usize;
        pages.push(Rc::from(reader.take(length)?));
    }
    let count = reader.u32()? as usize;
    let mut records: Vec<Before> = Vec::with_capacity(count.min(1 << 16));
    for _ in 0..count {
        records.push(decode_record(&mut reader, &pages)?);
    }
    Ok(records)
}

/// Reads one record.
///
/// @param reader - the chunk's bytes, positioned at the record
/// @param pages - the chunk's copies of leaves
fn decode_record(reader: &mut Reader<'_>, pages: &[Rc<[u8]>]) -> DbResult<Before> {
    let schema = reader.u8()?;
    let sealed = reader.u8()? != 0;
    let tree = reader.u64()?;
    let key = decode_key(reader)?;
    let row = match reader.u8()? {
        0 => None,
        _ => Some(decode_values(reader)?),
    };
    let page = decode_page(reader, pages)?;
    Ok(Before {
        schema,
        sealed,
        tree,
        key,
        row,
        page,
    })
}

/// Reads a record's key.
///
/// @param reader - the chunk's bytes, positioned at the key
fn decode_key(reader: &mut Reader<'_>) -> DbResult<KeyCopy> {
    Ok(match reader.u8()? {
        0 => KeyCopy::Empty,
        1 => KeyCopy::One(reader.datum()?),
        2 => {
            let first = reader.datum()?;
            let second = reader.datum()?;
            KeyCopy::Two(Box::new((first, second)))
        }
        3 => KeyCopy::Rowids {
            first: reader.u64()? as i64,
            last: reader.u64()? as i64,
        },
        4 => KeyCopy::Many(decode_values(reader)?),
        other => return Err(unreadable(&format!("an unknown key kind {other}"))),
    })
}

/// Reads a count of values and the values.
///
/// @param reader - the chunk's bytes, positioned at the count
fn decode_values(reader: &mut Reader<'_>) -> DbResult<Vec<OwnedDatum>> {
    let width = reader.u32()? as usize;
    let mut values = Vec::with_capacity(width.min(4096));
    for _ in 0..width {
        values.push(reader.datum()?);
    }
    Ok(values)
}

/// Reads the place a record names in a copy of a leaf.
///
/// @param reader - the chunk's bytes, positioned at the place
/// @param pages - the chunk's copies of leaves
fn decode_page(reader: &mut Reader<'_>, pages: &[Rc<[u8]>]) -> DbResult<Option<Box<FromPage>>> {
    let kind = reader.u8()?;
    if kind == 0 {
        return Ok(None);
    }
    let number = reader.u32()? as usize;
    let copy = pages
        .get(number)
        .ok_or_else(|| unreadable("a record names a copy of a leaf the chunk does not hold"))?;
    if kind == 1 {
        let located = decode_located(reader)?;
        return Ok(Some(Box::new(FromPage::One(Rc::clone(copy), located))));
    }
    let count = reader.u32()? as usize;
    let mut places: Vec<Located> = Vec::with_capacity(count.min(1 << 16));
    for _ in 0..count {
        places.push(decode_located(reader)?);
    }
    Ok(Some(Box::new(FromPage::Run(
        Rc::clone(copy),
        places.into_boxed_slice(),
    ))))
}

/// Reads where a row lies in a leaf.
///
/// @param reader - the chunk's bytes, positioned at the place
fn decode_located(reader: &mut Reader<'_>) -> DbResult<Located> {
    let kind = reader.u8()?;
    let at = reader.u64()? as usize;
    Ok(match kind {
        0 => Located::Sorted(at),
        1 => Located::Delta(at),
        _ => Located::Absent,
    })
}

/// A cursor over the bytes of one chunk.
struct Reader<'a> {
    /// The chunk.
    bytes: &'a [u8],
    /// The next byte to read.
    at: usize,
}

impl<'a> Reader<'a> {
    /// Takes the next `count` bytes.
    ///
    /// @param count - how many bytes
    fn take(&mut self, count: usize) -> DbResult<&'a [u8]> {
        let end = self.at.saturating_add(count);
        let taken = self
            .bytes
            .get(self.at..end)
            .ok_or_else(|| unreadable("a chunk ends inside a record"))?;
        self.at = end;
        Ok(taken)
    }

    /// Reads one byte.
    fn u8(&mut self) -> DbResult<u8> {
        self.take(1)?
            .first()
            .copied()
            .ok_or_else(|| unreadable("a chunk ends inside a record"))
    }

    /// Reads four bytes as a number.
    fn u32(&mut self) -> DbResult<u32> {
        let mut raw = [0u8; 4];
        raw.copy_from_slice(self.take(4)?);
        Ok(u32::from_le_bytes(raw))
    }

    /// Reads eight bytes as a number.
    fn u64(&mut self) -> DbResult<u64> {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(raw))
    }

    /// Reads one tagged value.
    fn datum(&mut self) -> DbResult<OwnedDatum> {
        let rest = self
            .bytes
            .get(self.at..)
            .ok_or_else(|| unreadable("a chunk ends inside a value"))?;
        let (value, taken) = Datum::decode_tagged(rest)
            .map_err(|why| unreadable(&format!("a value did not decode: {}", why.message())))?;
        self.at = self.at.saturating_add(taken);
        Ok(OwnedDatum::from_datum(&value))
    }
}

/// The failure a chunk that does not read back reports.
///
/// `Internal` and not `Corrupt`: these bytes were written by this process during this
/// transaction and are deleted with it, so a caller told their database is corrupt would go and
/// look at a file that is fine.
///
/// @param said - what went wrong
fn unreadable(said: &str) -> DbError {
    DbError::primary(PrimaryCode::Internal).with_message(said)
}

#[cfg(test)]
mod tests {
    use super::*;
    use inillucent_tree::datum::OwnedDatum;

    /// Builds one record with the fields the caller names.
    fn record(
        tree: u64,
        key: KeyCopy,
        row: Option<Vec<OwnedDatum>>,
        page: Option<FromPage>,
    ) -> Before {
        Before {
            schema: 2,
            sealed: tree.is_multiple_of(2),
            tree,
            key,
            row,
            page: page.map(Box::new),
        }
    }

    /// Every kind of record reads back as it was written, and records that shared a copy of a
    /// leaf still share one.
    #[test]
    fn every_kind_of_record_reads_back_equal() {
        let leaf: Rc<[u8]> = Rc::from(vec![7u8; 100]);
        let other: Rc<[u8]> = Rc::from(vec![9u8; 50]);
        let records = vec![
            record(
                1,
                KeyCopy::Empty,
                Some(vec![OwnedDatum::Int(-4), OwnedDatum::Text(b"hi".to_vec())]),
                None,
            ),
            record(2, KeyCopy::One(OwnedDatum::Blob(vec![1, 2, 3])), None, None),
            record(
                4,
                KeyCopy::Two(Box::new((OwnedDatum::Null, OwnedDatum::Real(1.5)))),
                None,
                None,
            ),
            record(
                5,
                KeyCopy::Rowids {
                    first: -3,
                    last: 90,
                },
                None,
                None,
            ),
            record(
                6,
                KeyCopy::Many(vec![
                    OwnedDatum::Int(1),
                    OwnedDatum::Int(2),
                    OwnedDatum::Int(3),
                ]),
                None,
                None,
            ),
            record(
                7,
                KeyCopy::Empty,
                None,
                Some(FromPage::One(Rc::clone(&leaf), Located::Sorted(3))),
            ),
            record(
                8,
                KeyCopy::Empty,
                None,
                Some(FromPage::Run(
                    Rc::clone(&leaf),
                    vec![Located::Delta(1), Located::Absent].into_boxed_slice(),
                )),
            ),
            record(
                9,
                KeyCopy::Empty,
                None,
                Some(FromPage::One(Rc::clone(&other), Located::Delta(0))),
            ),
        ];
        let bytes = encode_records(&records);
        let back = decode_records(&bytes).expect("the chunk reads back");
        assert_eq!(back.len(), records.len());
        for (read, written) in back.iter().zip(&records) {
            assert_eq!(read.schema, written.schema);
            assert_eq!(read.sealed, written.sealed);
            assert_eq!(read.tree, written.tree);
            assert_eq!(format!("{:?}", read.key), format!("{:?}", written.key));
            assert_eq!(format!("{:?}", read.row), format!("{:?}", written.row));
            assert_eq!(format!("{:?}", read.page), format!("{:?}", written.page));
        }
        let first = match back.get(5).and_then(|held| held.page.as_deref()) {
            Some(FromPage::One(page, _)) => Rc::clone(page),
            _ => panic!("the sixth record names a copy of a leaf"),
        };
        let second = match back.get(6).and_then(|held| held.page.as_deref()) {
            Some(FromPage::Run(page, _)) => Rc::clone(page),
            _ => panic!("the seventh record names a copy of a leaf"),
        };
        assert!(
            Rc::ptr_eq(&first, &second),
            "two records that shared a copy of a leaf share one after the read"
        );
        assert!(
            bytes.len() < 100 * 2 + 50 + 400,
            "the shared copy was written once: {} bytes",
            bytes.len()
        );
    }

    /// A chunk cut short is an error and not a panic or a wrong record.
    #[test]
    fn a_short_chunk_is_refused() {
        let records = vec![record(
            1,
            KeyCopy::Empty,
            Some(vec![OwnedDatum::Int(1)]),
            None,
        )];
        let bytes = encode_records(&records);
        let cut = bytes.get(..bytes.len().saturating_sub(3)).unwrap_or(&[]);
        assert!(decode_records(cut).is_err());
    }
}

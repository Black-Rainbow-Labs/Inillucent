//! The doclist byte format: one row's or one term's postings, varint-encoded.
//!
//! Invariant: **a doclist is a run of entries, each the rowid as a delta from
//! the previous one, then per column that has a position, the column number,
//! how many positions, and the positions as deltas.** Everything is a varint,
//! which is what makes a doclist for a common word small enough to be worth
//! keeping in one row - `%_idx`'s own row, since task-1911 folded the
//! dictionary and the doclist together; see the module's own doc comment.
//!
//! This file is the codec alone: encode, decode, and the two reads that answer
//! a narrower question without decoding everything - `last_doclist_rowid` for
//! "where does this end", `doclist_rows` for "which rows have a position in
//! this column". [`mod.rs`](super) and [`expr.rs`](super::expr) hold what does
//! the reading and writing; nothing here touches a shadow table.

use inillucent_base::varint;

/// Reads one varint, advancing the offset.
///
/// `pub(super)` rather than private: `Totals` in the parent module encodes
/// the same way and is not a doclist, so it reads through here rather than
/// keeping a second copy of the same eight lines.
pub(super) fn read_varint(bytes: &[u8], at: &mut usize) -> u64 {
    let Some(rest) = bytes.get(*at..) else {
        return 0;
    };
    let Ok(decoded) = varint::decode(rest) else {
        *at = bytes.len();
        return 0;
    };
    *at = at.saturating_add(decoded.len);
    decoded.value
}

/// Appends one varint.
pub(super) fn write_varint(out: &mut Vec<u8>, value: u64) {
    let mut buffer = [0u8; 9];
    if let Ok(used) = varint::encode(&mut buffer, value) {
        out.extend_from_slice(buffer.get(..used).unwrap_or(&[]));
    }
}

/// One row's appearance in one term's doclist.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocEntry {
    /// Which row.
    pub rowid: i64,
    /// The positions the term appears at, by column.
    pub columns: Vec<(usize, Vec<u32>)>,
}

/// Encodes a doclist.
pub fn encode_doclist(entries: &[DocEntry]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut previous = 0i64;
    for entry in entries {
        write_varint(&mut out, entry.rowid.wrapping_sub(previous) as u64);
        previous = entry.rowid;
        write_varint(&mut out, entry.columns.len() as u64);
        for (column, positions) in &entry.columns {
            write_varint(&mut out, *column as u64);
            write_varint(&mut out, positions.len() as u64);
            let mut last = 0u32;
            for position in positions {
                write_varint(&mut out, u64::from(position.wrapping_sub(last)));
                last = *position;
            }
        }
    }
    out
}

/// Returns the last rowid in an encoded doclist, without decoding it.
///
/// Walks the same structure `decode_doclist` walks and allocates nothing: it
/// keeps the running rowid and steps over each entry's varints. It answers
/// `None` unless the walk consumes the blob exactly, which is what makes it
/// safe to act on - a doclist this cannot account for byte-for-byte is one the
/// caller falls back to decoding, rather than one it appends to on a guess.
pub fn last_doclist_rowid(bytes: &[u8]) -> Option<i64> {
    let mut at = 0usize;
    let mut rowid = 0i64;
    let mut seen = false;
    while at < bytes.len() {
        rowid = rowid.wrapping_add(read_varint(bytes, &mut at) as i64);
        let columns = read_varint(bytes, &mut at) as usize;
        if columns > 4096 || at > bytes.len() {
            return None;
        }
        for _ in 0..columns {
            let _column = read_varint(bytes, &mut at);
            let count = read_varint(bytes, &mut at) as usize;
            if count > 1 << 24 || at > bytes.len() {
                return None;
            }
            for _ in 0..count {
                let _delta = read_varint(bytes, &mut at);
            }
            if at > bytes.len() {
                return None;
            }
        }
        seen = true;
    }
    if at == bytes.len() && seen {
        Some(rowid)
    } else {
        None
    }
}

/// Appends one entry to an encoded doclist, given its rowid delta.
///
/// The bytes it writes are exactly the bytes `encode_doclist` would write for
/// the same entry in the same position, which is the property that lets the
/// fast path below produce a doclist indistinguishable from a re-encoded one.
pub fn append_doclist_entry(out: &mut Vec<u8>, delta: i64, entry: &DocEntry) {
    write_varint(out, delta as u64);
    write_varint(out, entry.columns.len() as u64);
    for (column, positions) in &entry.columns {
        write_varint(out, *column as u64);
        write_varint(out, positions.len() as u64);
        let mut last = 0u32;
        for position in positions {
            write_varint(out, u64::from(position.wrapping_sub(last)));
            last = *position;
        }
    }
}

thread_local! {
    /// How many doclist entries this thread has decoded.
    ///
    /// A count of work done, which a test can compare with a bound that does not
    /// depend on how fast the machine is. Decoding a whole doclist to change one
    /// entry in it was the cost that made an `UPDATE` of every row of a table
    /// take time that grew as the square of the row count.
    static DECODED_ENTRIES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Returns how many doclist entries this thread has decoded since the last reset.
pub fn doclist_entries_decoded() -> u64 {
    DECODED_ENTRIES.with(|held| held.get())
}

/// Sets the count of decoded doclist entries back to zero.
pub fn reset_doclist_entries_decoded() {
    DECODED_ENTRIES.with(|held| held.set(0));
}

/// Decodes a doclist.
pub fn decode_doclist(bytes: &[u8]) -> Vec<DocEntry> {
    let entries = decode_entries(bytes);
    DECODED_ENTRIES.with(|held| held.set(held.get().saturating_add(entries.len() as u64)));
    entries
}

/// Decodes a doclist without counting it.
fn decode_entries(bytes: &[u8]) -> Vec<DocEntry> {
    let mut entries = Vec::new();
    let mut at = 0usize;
    let mut rowid = 0i64;
    while at < bytes.len() {
        rowid = rowid.wrapping_add(read_varint(bytes, &mut at) as i64);
        let columns = read_varint(bytes, &mut at) as usize;
        if columns > 4096 {
            break;
        }
        let mut per_column = Vec::with_capacity(columns);
        for _ in 0..columns {
            let column = read_varint(bytes, &mut at) as usize;
            let count = read_varint(bytes, &mut at) as usize;
            if count > 1 << 24 {
                return entries;
            }
            let mut positions = Vec::with_capacity(count.min(4096));
            let mut last = 0u32;
            for _ in 0..count {
                last = last.wrapping_add(read_varint(bytes, &mut at) as u32);
                positions.push(last);
            }
            per_column.push((column, positions));
        }
        entries.push(DocEntry {
            rowid,
            columns: per_column,
        });
        if at >= bytes.len() {
            break;
        }
    }
    entries
}

/// Reads one varint, answering nothing when the bytes end inside it.
///
/// @param bytes - the encoded doclist
/// @param at - where the varint starts, advanced past it
fn strict_varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let decoded = varint::decode(bytes.get(*at..)?).ok()?;
    *at = at.saturating_add(decoded.len);
    Some(decoded.value)
}

/// Takes one row out of an encoded doclist without decoding the others.
///
/// Walks the varints, finds the row's entry, cuts it out, and writes the next
/// entry's rowid delta again, because that delta was counted from the row that
/// is gone. Nothing is allocated for the entries that are kept, which is what
/// makes a single `DELETE` on a long doclist cheaper than decoding the list.
///
/// Answers `None` and changes nothing when the bytes cannot be walked exactly,
/// so the caller decodes the list instead. Otherwise answers whether the row
/// was there. `last` is kept equal to the rowid of the last entry.
///
/// @param bytes - the encoded doclist, changed in place
/// @param last - the rowid the last entry names, kept up to date
/// @param target - the row to take out
pub fn remove_entry_in_place(
    bytes: &mut Vec<u8>,
    last: &mut Option<i64>,
    target: i64,
) -> Option<bool> {
    let mut at = 0usize;
    let mut previous = 0i64;
    while at < bytes.len() {
        let start = at;
        let delta = strict_varint(bytes, &mut at)? as i64;
        let rowid = previous.wrapping_add(delta);
        let columns = strict_varint(bytes, &mut at)? as usize;
        if columns > 4096 {
            return None;
        }
        for _ in 0..columns {
            let _column = strict_varint(bytes, &mut at)?;
            let count = strict_varint(bytes, &mut at)? as usize;
            if count > 1 << 24 {
                return None;
            }
            for _ in 0..count {
                strict_varint(bytes, &mut at)?;
            }
        }
        if rowid != target {
            previous = rowid;
            continue;
        }
        let end = at;
        if end < bytes.len() {
            let mut next_at = end;
            let next_delta = read_varint(bytes, &mut next_at) as i64;
            let mut replacement = Vec::with_capacity(9);
            write_varint(&mut replacement, next_delta.wrapping_add(delta) as u64);
            bytes.splice(end..next_at, replacement);
        } else {
            *last = (start > 0).then_some(previous);
        }
        bytes.drain(start..end);
        return Some(true);
    }
    Some(false)
}

/// Collects the rows a doclist names, without decoding their positions.
///
/// A row is collected when it has a position in a column the caller asked for
/// and that the table declares - the same test [`super::expr::phrase_hits`]
/// applies at offset zero, decided by walking the varints rather than by
/// building the vectors that would prove it.
///
/// @param bytes - the doclist as `%_idx` holds it
/// @param wanted - the columns a `column:term` filter named
/// @param columns - how many columns the table declares
/// @param out - where the rowids are appended, in doclist order
pub fn doclist_rows(
    bytes: &[u8],
    wanted: &super::expr::ColumnFilter,
    columns: usize,
    out: &mut Vec<i64>,
) {
    let mut at = 0usize;
    let mut rowid = 0i64;
    while at < bytes.len() {
        rowid = rowid.wrapping_add(read_varint(bytes, &mut at) as i64);
        let count = read_varint(bytes, &mut at) as usize;
        if count > 4096 {
            break;
        }
        let mut matched = false;
        for _ in 0..count {
            let column = read_varint(bytes, &mut at) as usize;
            let positions = read_varint(bytes, &mut at) as usize;
            if positions > 1 << 24 {
                return;
            }
            for _ in 0..positions {
                let _ = read_varint(bytes, &mut at);
            }
            if positions == 0 || column >= columns {
                continue;
            }
            if !wanted.admits(column) {
                continue;
            }
            matched = true;
        }
        if matched {
            out.push(rowid);
        }
        if at >= bytes.len() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A doclist round-trips, positions and all.
    #[test]
    fn a_doclist_round_trips() {
        let entries = vec![
            DocEntry {
                rowid: 1,
                columns: vec![(0, vec![0, 3, 9])],
            },
            DocEntry {
                rowid: 40,
                columns: vec![(0, vec![2]), (1, vec![0, 1])],
            },
        ];
        let encoded = encode_doclist(&entries);
        assert_eq!(decode_doclist(&encoded), entries);
    }

    /// Builds a doclist of four rows with a different shape each.
    fn four_rows() -> Vec<DocEntry> {
        vec![
            DocEntry {
                rowid: 3,
                columns: vec![(0, vec![0, 3, 9])],
            },
            DocEntry {
                rowid: 10,
                columns: vec![(0, vec![2]), (1, vec![0, 1])],
            },
            DocEntry {
                rowid: 11,
                columns: vec![(1, vec![5])],
            },
            DocEntry {
                rowid: 4_000_000,
                columns: vec![(0, vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10])],
            },
        ]
    }

    /// Cutting a row out leaves the bytes that encoding the other rows gives.
    #[test]
    fn cutting_a_row_out_matches_encoding_the_rest() {
        for target in [3i64, 10, 11, 4_000_000] {
            let entries = four_rows();
            let mut bytes = encode_doclist(&entries);
            let mut last = last_doclist_rowid(&bytes);
            assert_eq!(
                remove_entry_in_place(&mut bytes, &mut last, target),
                Some(true)
            );
            let rest: Vec<DocEntry> = entries
                .into_iter()
                .filter(|entry| entry.rowid != target)
                .collect();
            assert_eq!(bytes, encode_doclist(&rest), "after cutting row {target}");
            assert_eq!(
                last,
                rest.last().map(|entry| entry.rowid),
                "the last rowid after cutting row {target}"
            );
        }
    }

    /// A row that is not in the list changes nothing, and the last row is the same.
    #[test]
    fn cutting_a_row_that_is_not_there_changes_nothing() {
        let mut bytes = encode_doclist(&four_rows());
        let before = bytes.clone();
        let mut last = last_doclist_rowid(&bytes);
        assert_eq!(remove_entry_in_place(&mut bytes, &mut last, 7), Some(false));
        assert_eq!(bytes, before);
        assert_eq!(last, Some(4_000_000));
    }

    /// Cutting the only row empties the list.
    #[test]
    fn cutting_the_only_row_leaves_nothing() {
        let only = vec![DocEntry {
            rowid: 9,
            columns: vec![(0, vec![4])],
        }];
        let mut bytes = encode_doclist(&only);
        let mut last = Some(9);
        assert_eq!(remove_entry_in_place(&mut bytes, &mut last, 9), Some(true));
        assert!(bytes.is_empty());
        assert_eq!(last, None);
    }

    /// Bytes that cannot be walked are refused, so the caller decodes them.
    #[test]
    fn bytes_that_cannot_be_walked_are_refused() {
        let mut bytes = Vec::new();
        write_varint(&mut bytes, 0);
        write_varint(&mut bytes, 5000);
        let before = bytes.clone();
        let mut last = None;
        assert_eq!(remove_entry_in_place(&mut bytes, &mut last, 1), None);
        assert_eq!(bytes, before);
    }
}

//! The segments an FTS5 index keeps its postings in.
//!
//! Invariant: **a commit appends to the index and never rewrites what is already in it.** The
//! postings a transaction adds are written as one new segment, and small segments are merged
//! into larger ones as the load goes on, so the bytes written for a row grow with the logarithm
//! of the rows already there and not with the rows themselves.
//!
//! ## Why the index is segmented
//!
//! Every commit used to read the whole list of each word its rows contained, add the new
//! entries and write the whole list back. A word that is in every row has a list as long as the
//! table, so a load that commits every statement wrote that list again for each row. 40,000
//! rows took 16 s where SQLite takes 0.36 s, and the work grew as the square of the rows.
//! SQLite's FTS5 does what this module does: write the new postings as a small segment of their
//! own, and merge segments of about the same size.
//!
//! ## The layout
//!
//! `%_idx(segid, term, doclist)` is keyed by the segment first and the term second, so one
//! segment is one contiguous run of the tree.
//!
//! - **Segment 0 is the base.** It is where every earlier build kept its one doclist per term,
//!   and it is still read the same way, including a row whose third column is a page number in
//!   `%_data` that the oldest layout left. Nothing in it is ever rewritten by a commit. It is
//!   merged only by `optimize`, which also writes its output there.
//! - **Segments 1 and up are appended by commits and merged by this module.** A higher number
//!   is newer. A segment holds, for each term, the entries of the rows that segment changed.
//!   An entry with no columns says the row was deleted, and it hides the row's entries in every
//!   older segment. An entry with columns replaces the row's older entries.
//! - **`%_data` row [`STRUCTURE_ROW`] lists the live segments**, with each one's level and size.
//!   It is read when a query starts and when a commit writes, and never kept between them, so a
//!   second connection that wrote in the meantime is seen.
//!
//! A term's answer is its entries from the base and then each segment, oldest first, each
//! overriding the rows before it, and then the entries the running transaction has staged.
//!
//! ## Merging
//!
//! Segments have a level. A new segment is at the level its size gives, from level 0 for less
//! than 32 KB, and each level is four times the size of the one below. When four segments that
//! are next to each other in age have the same level, they are merged into one segment a level
//! higher. The merged segment takes the number of the newest of the four, so it stays in its
//! place in the order of age. Only neighbours are merged, because a segment between two of the
//! inputs would otherwise be overridden by entries that are older than it.
//!
//! A merge keeps the deletion markers unless it includes the oldest segment there is and the
//! base holds nothing, because an older segment may still hold the rows the markers hide.

use std::collections::BTreeMap;

use inillucent_base::DbResult;
use inillucent_value::Value;

use super::super::Context;
use super::doclist::{
    decode_doclist, doclist_has_empty_entry, encode_doclist, read_varint, write_varint, DocEntry,
};
use super::index::{record_stage, require_doclist, term_value, Buffer, TermValue};
use crate::shadow::ShadowTables;

/// The `%_data` row that holds the list of live segments.
///
/// Far above any page number the oldest layout allocated, which counted up from one, so the
/// row cannot be mistaken for a doclist that an older build left.
pub(crate) const STRUCTURE_ROW: i64 = 1 << 40;

/// The four bytes a structure record begins with.
const MAGIC: [u8; 4] = *b"SEG1";

/// How many neighbouring segments of one level are merged into one of the next.
const MERGE_FACTOR: usize = 4;

/// The size below which a segment is at level 0, and the size each level multiplies by four.
const LEVEL_UNIT: u64 = 32 * 1024;

/// How many bytes of one segment's rows a merge reads at a time.
///
/// A merge holds this much from each of its inputs, so its memory is bounded however large
/// the segments are.
const CHUNK_BYTES: usize = 4 * 1024 * 1024;

thread_local! {
    /// The chunk size this thread's merges use.
    ///
    /// A thread rather than a constant so a test can make the chunk a few bytes, which makes
    /// every merge run in many passes on a small index. The passes are the part of a merge that
    /// is easy to get wrong and that a small index would otherwise never reach.
    static MERGE_CHUNK_BYTES: std::cell::Cell<usize> = const { std::cell::Cell::new(CHUNK_BYTES) };
}

/// Sets how many bytes of one segment a merge on this thread reads at a time.
///
/// @param bytes - the chunk size, which a test makes small to force many passes
pub fn set_merge_chunk_bytes(bytes: usize) {
    MERGE_CHUNK_BYTES.with(|held| held.set(bytes.max(1)));
}

/// One live segment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Segment {
    /// The segment number in `%_idx`. A higher number is newer.
    pub(crate) id: i64,
    /// Which level the segment is at.
    pub(crate) level: u32,
    /// How many bytes of terms and doclists the segment holds, at the time it was written.
    pub(crate) bytes: u64,
}

/// The segments of an index, oldest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Structure {
    /// The number the next new segment gets.
    pub(crate) next_id: i64,
    /// Whether segment 0 holds no rows, so a reader need not look there.
    ///
    /// True for an index this build created and for one it rebuilt. False for an index an
    /// earlier build wrote, whose rows are all in segment 0.
    pub(crate) base_empty: bool,
    /// The segments after the base, oldest first.
    pub(crate) segments: Vec<Segment>,
}

impl Structure {
    /// Returns the structure of an index that has no record: the base holds everything.
    pub(crate) fn legacy() -> Structure {
        Structure {
            next_id: 1,
            base_empty: false,
            segments: Vec::new(),
        }
    }

    /// Returns the structure of an index with nothing in it.
    pub(crate) fn empty() -> Structure {
        Structure {
            next_id: 1,
            base_empty: true,
            segments: Vec::new(),
        }
    }

    /// Returns the segment numbers a reader looks in, oldest first.
    pub(crate) fn sources(&self) -> Vec<i64> {
        let mut found = Vec::with_capacity(self.segments.len().saturating_add(1));
        if !self.base_empty {
            found.push(0);
        }
        found.extend(self.segments.iter().map(|segment| segment.id));
        found
    }

    /// Returns the structure as the bytes `%_data` holds.
    fn encode(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.push(u8::from(self.base_empty));
        write_varint(&mut out, self.next_id as u64);
        write_varint(&mut out, self.segments.len() as u64);
        for segment in &self.segments {
            write_varint(&mut out, segment.id as u64);
            write_varint(&mut out, u64::from(segment.level));
            write_varint(&mut out, segment.bytes);
        }
        out
    }

    /// Reads a structure back, or nothing when the bytes are not one.
    ///
    /// @param bytes - the `%_data` row's block
    fn decode(bytes: &[u8]) -> Option<Structure> {
        if bytes.get(..MAGIC.len())? != MAGIC {
            return None;
        }
        let flags = *bytes.get(MAGIC.len())?;
        let mut at = MAGIC.len().checked_add(1)?;
        let next_id = read_varint(bytes, &mut at) as i64;
        let count = read_varint(bytes, &mut at) as usize;
        if count > 1 << 20 {
            return None;
        }
        let mut segments = Vec::with_capacity(count);
        for _ in 0..count {
            let id = read_varint(bytes, &mut at) as i64;
            let level = u32::try_from(read_varint(bytes, &mut at)).unwrap_or(u32::MAX);
            let held = read_varint(bytes, &mut at);
            segments.push(Segment {
                id,
                level,
                bytes: held,
            });
        }
        if at > bytes.len() {
            return None;
        }
        Some(Structure {
            next_id,
            base_empty: flags & 1 != 0,
            segments,
        })
    }
}

/// Returns whether a `%_data` rowid is the structure record's.
///
/// `rebuild` and `delete-all` clear every `%_data` row but the totals and the records that
/// describe the index itself.
///
/// @param rowid - the `%_data` row being considered
pub(crate) fn is_structure_row(rowid: i64) -> bool {
    rowid == STRUCTURE_ROW
}

/// Returns the level a segment of a given size starts at.
///
/// @param bytes - how many bytes of terms and doclists the segment holds
pub(crate) fn level_for(bytes: u64) -> u32 {
    let mut level = 0u32;
    let mut limit = LEVEL_UNIT;
    while bytes >= limit && level < 24 {
        level = level.saturating_add(1);
        limit = limit.saturating_mul(4);
    }
    level
}

/// Reads the list of live segments.
///
/// An index with no record is one an earlier build wrote, which kept everything in the base.
/// A record that cannot be read is corruption and is refused, because treating it as no record
/// would leave the segments out of every answer.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
pub(crate) fn read_structure(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
) -> DbResult<Structure> {
    let Some(row) = shadows.read_row(context, b"data", STRUCTURE_ROW)? else {
        return Ok(Structure::legacy());
    };
    let Some(block) = row.get(1).and_then(Value::as_blob) else {
        return Ok(Structure::legacy());
    };
    Structure::decode(block.raw()).ok_or_else(|| {
        inillucent_base::error::corrupt(
            "the full-text index has a segment list that cannot be read",
        )
    })
}

/// Writes the list of live segments.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param structure - the segments
pub(crate) fn write_structure(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    structure: &Structure,
) -> DbResult<()> {
    shadows.write_row(
        context,
        b"data",
        STRUCTURE_ROW,
        &[Value::Null, Value::owned_blob(&structure.encode())?],
    )
}

/// Returns a term's term bytes as a `%_idx` key.
///
/// @param segment - the segment number
/// @param term - the term
fn key_of(segment: i64, term: &[u8]) -> DbResult<[Value<'static>; 2]> {
    Ok([Value::Integer(segment), Value::owned_blob(term)?])
}

/// Returns the term a `%_idx` row is for, whichever type the column holds.
///
/// @param values - one `%_idx` row
fn term_of(values: &[Value<'static>]) -> Option<Vec<u8>> {
    match values.get(1) {
        Some(Value::Blob(blob)) => Some(blob.raw().to_vec()),
        Some(Value::Text(text)) => Some(text.utf8_bytes().into_owned()),
        _ => None,
    }
}

/// Returns the segment number a `%_idx` row is in.
///
/// @param values - one `%_idx` row
fn segment_of(values: &[Value<'static>]) -> Option<i64> {
    values.first().and_then(Value::as_integer)
}

/// Lays one newer list of entries over an older one.
///
/// Both lists are in rowid order. Where they name the same row the newer entry wins, and a
/// newer entry with no columns removes the row. With `keep_markers` such an entry is kept in
/// the result, so that a segment that is merged without the oldest segment still hides the
/// older rows.
///
/// @param older - the entries of the older sources, in rowid order
/// @param newer - the entries of the newer source, in rowid order
/// @param keep_markers - whether a deletion marker in the result stays in it
pub(crate) fn overlay(
    older: Vec<DocEntry>,
    newer: Vec<DocEntry>,
    keep_markers: bool,
) -> Vec<DocEntry> {
    let keeps = |entry: &DocEntry| keep_markers || !entry.columns.is_empty();
    if older.is_empty() {
        return newer.into_iter().filter(keeps).collect();
    }
    if newer.is_empty() {
        return older;
    }
    let mut out = Vec::with_capacity(older.len().saturating_add(newer.len()));
    let mut older = older.into_iter().peekable();
    let mut newer = newer.into_iter().peekable();
    loop {
        let take_older = match (older.peek(), newer.peek()) {
            (Some(left), Some(right)) => match left.rowid.cmp(&right.rowid) {
                std::cmp::Ordering::Less => Some(true),
                std::cmp::Ordering::Greater => Some(false),
                std::cmp::Ordering::Equal => {
                    older.next();
                    None
                }
            },
            (Some(_), None) => Some(true),
            (None, Some(_)) => Some(false),
            (None, None) => break,
        };
        let taken = match take_older {
            Some(true) => older.next(),
            Some(false) | None => newer.next(),
        };
        if let Some(entry) = taken.filter(|entry| take_older == Some(true) || keeps(entry)) {
            out.push(entry);
        }
    }
    out
}

/// Reads one segment's doclist for a term, decoded.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param segment - the segment number
/// @param term - the term
fn segment_entries(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    segment: i64,
    term: &[u8],
) -> DbResult<Option<Vec<DocEntry>>> {
    let Some(row) = shadows.read_keyed(context, b"idx", &key_of(segment, term)?, 3)? else {
        return Ok(None);
    };
    let bytes = require_doclist(context, shadows, term, &row)?;
    Ok(Some(decode_doclist(&bytes)))
}

/// Returns the staged postings for a term as encoded bytes, when the transaction has some.
///
/// @param buffer - the postings this transaction has staged
/// @param term - the term
fn staged_bytes(buffer: &Buffer, term: &[u8]) -> Option<Vec<u8>> {
    let held = buffer.lock().ok()?;
    held.doclists
        .get(term)
        .filter(|staged| !staged.is_empty())
        .map(|staged| staged.encoded().into_owned())
}

/// Returns every live entry of a term: the segments' and the running transaction's.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param buffer - the postings this transaction has staged
/// @param structure - the segments, as read when the statement started
/// @param term - the term
pub(crate) fn read_term_entries(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
    structure: &Structure,
    term: &[u8],
) -> DbResult<Vec<DocEntry>> {
    let mut entries: Vec<DocEntry> = Vec::new();
    for segment in structure.sources() {
        if let Some(list) = segment_entries(context, shadows, segment, term)? {
            entries = overlay(entries, list, false);
        }
    }
    if let Some(bytes) = staged_bytes(buffer, term) {
        entries = overlay(entries, decode_doclist(&bytes), false);
    }
    Ok(entries)
}

/// Returns a term's live entries as one encoded doclist, or nothing when it has none.
///
/// **A term that lives in one place, with nothing deleted from it, is handed on as it is
/// stored.** That is every term of an index an earlier build wrote and of one `optimize` has
/// merged, and it is what a query used to do for every term.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param buffer - the postings this transaction has staged
/// @param structure - the segments, as read when the statement started
/// @param term - the term
pub(crate) fn read_term_bytes(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
    structure: &Structure,
    term: &[u8],
) -> DbResult<Option<Vec<u8>>> {
    let mut lists: Vec<Vec<u8>> = Vec::new();
    for segment in structure.sources() {
        let key = key_of(segment, term)?;
        if let Some(row) = shadows.read_keyed(context, b"idx", &key, 3)? {
            lists.push(require_doclist(context, shadows, term, &row)?);
        }
    }
    lists.extend(staged_bytes(buffer, term));
    if lists.len() == 1 {
        let only = lists.swap_remove(0);
        return Ok(match (only.is_empty(), doclist_has_empty_entry(&only)) {
            (true, _) => None,
            (false, false) => Some(only),
            (false, true) => encoded_or_none(overlay(Vec::new(), decode_doclist(&only), false)),
        });
    }
    let mut entries: Vec<DocEntry> = Vec::new();
    for bytes in &lists {
        entries = overlay(entries, decode_doclist(bytes), false);
    }
    Ok(encoded_or_none(entries))
}

/// Encodes a list of entries, or answers nothing when it is empty.
///
/// @param entries - the entries
fn encoded_or_none(entries: Vec<DocEntry>) -> Option<Vec<u8>> {
    (!entries.is_empty()).then(|| encode_doclist(&entries))
}

/// Returns every term with at least one live entry, with its entries.
///
/// **One scan of each segment, then one merge per term.** With a prefix the scan starts at the
/// prefix and stops at the first term past it, so a prefix search reads only the terms it names.
/// With none it reads every term, which is what `integrity-check`, `rebuild` and `fts5vocab`
/// need. The postings the running transaction staged are laid over what is stored, so a query
/// inside a transaction sees what the transaction wrote.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param buffer - the postings the transaction has staged, for a reader that can see them
/// @param structure - the segments, as read when the statement started
/// @param prefix - the start of the terms wanted, or nothing for every term
pub(crate) fn scan_terms(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: Option<&Buffer>,
    structure: &Structure,
    prefix: Option<&[u8]>,
) -> DbResult<BTreeMap<Vec<u8>, Vec<DocEntry>>> {
    let start = prefix.unwrap_or_default();
    let mut rows: BTreeMap<Vec<u8>, Vec<Vec<Value<'static>>>> = BTreeMap::new();
    for segment in structure.sources() {
        let from = key_of(segment, start)?;
        shadows.scan_keyed_from(context, b"idx", 2, &from, |values| {
            if segment_of(values) != Some(segment) {
                return Ok(false);
            }
            let Some(term) = term_of(values) else {
                return Ok(true);
            };
            if !term.starts_with(start) {
                return Ok(false);
            }
            rows.entry(term).or_default().push(values.to_vec());
            Ok(true)
        })?;
    }
    let mut staged: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
    if let Some(held) = buffer.and_then(|buffer| buffer.lock().ok()) {
        for (term, list) in held.doclists.range(start.to_vec()..) {
            if !term.starts_with(start) {
                break;
            }
            if !list.is_empty() {
                staged.insert(term.clone(), list.encoded().into_owned());
            }
        }
    }
    for term in staged.keys() {
        rows.entry(term.clone()).or_default();
    }
    let mut found = BTreeMap::new();
    for (term, stored) in rows {
        let mut entries: Vec<DocEntry> = Vec::new();
        for row in &stored {
            let bytes = require_doclist(context, shadows, &term, row)?;
            entries = overlay(entries, decode_doclist(&bytes), false);
        }
        if let Some(bytes) = staged.get(&term) {
            entries = overlay(entries, decode_doclist(bytes), false);
        }
        if !entries.is_empty() {
            found.insert(term, entries);
        }
    }
    Ok(found)
}

/// Writes one segment from the postings a transaction staged, and returns how many bytes it holds.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param segment - the segment number
/// @param lists - each term and its encoded entries, in term order
pub(crate) fn write_segment(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    segment: i64,
    lists: &[(Vec<u8>, Vec<u8>)],
) -> DbResult<u64> {
    let mut written = 0u64;
    for (term, bytes) in lists {
        let row = [
            Value::Integer(segment),
            Value::owned_blob(term)?,
            Value::owned_blob(bytes)?,
        ];
        shadows.write_keyed(context, b"idx", 2, &row)?;
        written = written.saturating_add((term.len().saturating_add(bytes.len())) as u64);
    }
    record_stage(|stages| stages.flushed_bytes = stages.flushed_bytes.saturating_add(written));
    Ok(written)
}

/// Finds the neighbouring run of one level that is due to be merged.
///
/// The run with the lowest level is chosen, so small segments are merged before large ones.
///
/// @param structure - the segments
fn due_run(structure: &Structure) -> Option<(usize, usize)> {
    let mut best: Option<(u32, usize, usize)> = None;
    let mut start = 0usize;
    while start < structure.segments.len() {
        let level = structure.segments.get(start)?.level;
        let mut end = start.saturating_add(1);
        while structure
            .segments
            .get(end)
            .is_some_and(|segment| segment.level == level)
        {
            end = end.saturating_add(1);
        }
        let length = end.saturating_sub(start);
        if length >= MERGE_FACTOR && best.is_none_or(|(lowest, _, _)| level < lowest) {
            best = Some((level, start, length));
        }
        start = end;
    }
    best.map(|(_, start, length)| (start, length))
}

/// Merges segments until no level has a run of neighbours that is due.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param structure - the segments, changed to match what was merged
pub(crate) fn maintain(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    structure: &mut Structure,
) -> DbResult<()> {
    while let Some((start, length)) = due_run(structure) {
        let end = start.saturating_add(length);
        let run: Vec<Segment> = structure
            .segments
            .get(start..end)
            .map(<[Segment]>::to_vec)
            .unwrap_or_default();
        let (Some(first), Some(newest)) = (run.first(), run.last()) else {
            break;
        };
        let inputs: Vec<i64> = run.iter().map(|segment| segment.id).collect();
        let drops_markers = structure.base_empty && start == 0;
        let bytes = merge_segments(context, shadows, &inputs, newest.id, drops_markers)?;
        let merged = (bytes > 0).then(|| Segment {
            id: newest.id,
            level: first.level.saturating_add(1).max(level_for(bytes)),
            bytes,
        });
        structure.segments.splice(start..end, merged);
    }
    Ok(())
}

/// Merges every segment, the base included, into the base.
///
/// What `optimize` does. The result is one run of terms in segment 0 with no deletion markers,
/// which is the layout every earlier build wrote and every reader handles without a merge.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param structure - the segments, changed to hold none
pub(crate) fn merge_all(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    structure: &mut Structure,
) -> DbResult<()> {
    let inputs = structure.sources();
    let bytes = merge_segments(context, shadows, &inputs, 0, true)?;
    structure.segments.clear();
    structure.base_empty = bytes == 0;
    Ok(())
}

/// Collects up to a chunk of one segment's rows, starting after a term.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param segment - the segment
/// @param after - the last term already merged, which is skipped, or nothing for the start
fn read_chunk(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    segment: i64,
    after: Option<&[u8]>,
) -> DbResult<(Vec<(Vec<u8>, Vec<Value<'static>>)>, bool)> {
    let from = key_of(segment, after.unwrap_or_default())?;
    let mut rows: Vec<(Vec<u8>, Vec<Value<'static>>)> = Vec::new();
    let mut size = 0usize;
    let mut truncated = false;
    shadows.scan_keyed_from(context, b"idx", 2, &from, |values| {
        if segment_of(values) != Some(segment) {
            return Ok(false);
        }
        let Some(term) = term_of(values) else {
            return Ok(true);
        };
        if after.is_some_and(|last| term.as_slice() <= last) {
            return Ok(true);
        }
        if size >= MERGE_CHUNK_BYTES.with(|held| held.get()) {
            truncated = true;
            return Ok(false);
        }
        size = size.saturating_add(term.len());
        if let Some(Value::Blob(blob)) = values.get(2) {
            size = size.saturating_add(blob.raw().len());
        }
        rows.push((term, values.to_vec()));
        Ok(true)
    })?;
    Ok((rows, truncated))
}

/// Merges the rows of several segments into one segment, a chunk of terms at a time.
///
/// The inputs are oldest first. Each term's lists are laid over one another in that order, and
/// the result is written under `output`, which may be one of the inputs. The inputs' own rows
/// for a term are removed as the term is merged, so afterwards the only rows left are the
/// output's. Answers how many bytes the output holds.
///
/// **Memory is bounded by the chunk, not by the segments.** Each pass reads up to
/// [`CHUNK_BYTES`] from every input, merges the terms that every input has read in full, and
/// carries on from the last of them.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param inputs - the segment numbers, oldest first
/// @param output - the segment number the result is written under
/// @param drops_markers - whether deletion markers are left out of the result
fn merge_segments(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    inputs: &[i64],
    output: i64,
    drops_markers: bool,
) -> DbResult<u64> {
    let mut after: Option<Vec<u8>> = None;
    let mut out_bytes = 0u64;
    loop {
        let mut chunks: Vec<(i64, Vec<(Vec<u8>, Vec<Value<'static>>)>)> = Vec::new();
        let mut bound: Option<Vec<u8>> = None;
        for segment in inputs {
            let (rows, truncated) = read_chunk(context, shadows, *segment, after.as_deref())?;
            if truncated {
                let last = rows.last().map(|(term, _)| term.clone());
                bound = match (bound, last) {
                    (Some(held), Some(last)) => Some(held.min(last)),
                    (held, last) => held.or(last),
                };
            }
            chunks.push((*segment, rows));
        }
        let mut by_term: BTreeMap<Vec<u8>, Vec<(i64, Vec<Value<'static>>)>> = BTreeMap::new();
        for (segment, rows) in chunks {
            for (term, row) in rows {
                if bound.as_ref().is_some_and(|limit| term > *limit) {
                    continue;
                }
                by_term.entry(term).or_default().push((segment, row));
            }
        }
        let Some(last) = by_term.keys().next_back().cloned() else {
            break;
        };
        for (term, rows) in by_term {
            out_bytes = out_bytes.saturating_add(merge_term(
                context,
                shadows,
                &term,
                &rows,
                output,
                drops_markers,
            )?);
        }
        if bound.is_none() {
            break;
        }
        after = Some(last);
    }
    Ok(out_bytes)
}

/// Merges one term's rows from several segments, and writes the result under the output.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param term - the term
/// @param rows - the term's row in each input that has one, oldest first
/// @param output - the segment number the result is written under
/// @param drops_markers - whether deletion markers are left out of the result
fn merge_term(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    term: &[u8],
    rows: &[(i64, Vec<Value<'static>>)],
    output: i64,
    drops_markers: bool,
) -> DbResult<u64> {
    let mut entries: Vec<DocEntry> = Vec::new();
    for (segment, row) in rows {
        let bytes = require_doclist(context, shadows, term, row)?;
        entries = overlay(entries, decode_doclist(&bytes), !drops_markers);
        shadows.delete_keyed(context, b"idx", &key_of(*segment, term)?)?;
        // A row an earlier layout left names a page of `%_data`, which is no longer needed.
        if let Some(TermValue::Page(page)) = term_value(row) {
            shadows.delete_row(context, b"data", page)?;
        }
    }
    if entries.is_empty() {
        return Ok(0);
    }
    let bytes = encode_doclist(&entries);
    let written = (term.len().saturating_add(bytes.len())) as u64;
    let row = [
        Value::Integer(output),
        Value::owned_blob(term)?,
        Value::owned_blob(&bytes)?,
    ];
    shadows.write_keyed(context, b"idx", 2, &row)?;
    record_stage(|stages| stages.flushed_bytes = stages.flushed_bytes.saturating_add(written));
    Ok(written)
}

/// Removes every row of every segment, and the segment list.
///
/// What `rebuild` and `delete-all` do before they write the index again.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
pub(crate) fn clear(context: &mut Context<'_>, shadows: &ShadowTables) -> DbResult<()> {
    let mut keys: Vec<(i64, Vec<u8>)> = Vec::new();
    shadows.scan_keyed(context, b"idx", 2, |values| {
        if let (Some(segment), Some(term)) = (segment_of(values), term_of(values)) {
            keys.push((segment, term));
        }
        Ok(true)
    })?;
    for (segment, term) in keys {
        shadows.delete_keyed(context, b"idx", &key_of(segment, &term)?)?;
    }
    write_structure(context, shadows, &Structure::empty())
}

/// Checks the segment list against the rows, and returns what is wrong.
///
/// A row in a segment the list does not name is one no reader looks in, so the rows it holds
/// are missing from every answer. A segment the list names with no rows is only wasted work.
///
/// @param context - the running statement
/// @param shadows - the table's shadow tables
/// @param structure - the segments
pub(crate) fn check_structure(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    structure: &Structure,
) -> DbResult<Vec<String>> {
    let known = structure.sources();
    let mut orphans: BTreeMap<i64, u64> = BTreeMap::new();
    shadows.scan_keyed(context, b"idx", 2, |values| {
        if let Some(segment) = segment_of(values) {
            if !known.contains(&segment) {
                *orphans.entry(segment).or_default() += 1;
            }
        }
        Ok(true)
    })?;
    Ok(orphans
        .into_iter()
        .map(|(segment, rows)| {
            format!("segment {segment} holds {rows} rows and is not in the segment list")
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an entry for a row with one position.
    ///
    /// @param rowid - the row
    fn live(rowid: i64) -> DocEntry {
        DocEntry {
            rowid,
            columns: vec![(0, vec![1])],
        }
    }

    /// Builds the marker that says a row was deleted.
    ///
    /// @param rowid - the row
    fn gone(rowid: i64) -> DocEntry {
        DocEntry {
            rowid,
            columns: Vec::new(),
        }
    }

    /// A newer entry replaces an older one for the same row, and the rest are kept in order.
    #[test]
    fn a_newer_entry_replaces_an_older_one() {
        let older = vec![live(1), live(3), live(5)];
        let mut changed = live(3);
        changed.columns = vec![(1, vec![9])];
        let newer = vec![live(2), changed.clone(), live(8)];
        let merged = overlay(older, newer, false);
        let rowids: Vec<i64> = merged.iter().map(|entry| entry.rowid).collect();
        assert_eq!(rowids, vec![1, 2, 3, 5, 8]);
        assert_eq!(merged.get(2), Some(&changed));
    }

    /// A deletion marker removes the row, or stays as a marker when the merge keeps them.
    #[test]
    fn a_deletion_marker_removes_the_row_it_names() {
        let older = vec![live(1), live(3)];
        let newer = vec![gone(3), gone(7)];
        let dropped = overlay(older.clone(), newer.clone(), false);
        assert_eq!(dropped, vec![live(1)]);
        let kept = overlay(older, newer, true);
        assert_eq!(kept, vec![live(1), gone(3), gone(7)]);
    }

    /// A marker with nothing older to hide is dropped when markers are not kept.
    #[test]
    fn a_marker_over_nothing_is_dropped() {
        assert!(overlay(Vec::new(), vec![gone(4)], false).is_empty());
        assert_eq!(overlay(Vec::new(), vec![gone(4)], true), vec![gone(4)]);
    }

    /// The structure record reads back as it was written.
    #[test]
    fn the_structure_round_trips() {
        let structure = Structure {
            next_id: 41,
            base_empty: true,
            segments: vec![
                Segment {
                    id: 3,
                    level: 2,
                    bytes: 90_000,
                },
                Segment {
                    id: 40,
                    level: 0,
                    bytes: 120,
                },
            ],
        };
        assert_eq!(Structure::decode(&structure.encode()), Some(structure));
        assert_eq!(Structure::decode(b"not a record"), None);
    }

    /// A segment's level follows its size, four times the bytes for each level.
    #[test]
    fn a_level_is_four_times_the_size_of_the_one_below() {
        assert_eq!(level_for(0), 0);
        assert_eq!(level_for(LEVEL_UNIT - 1), 0);
        assert_eq!(level_for(LEVEL_UNIT), 1);
        assert_eq!(level_for(LEVEL_UNIT * 4 - 1), 1);
        assert_eq!(level_for(LEVEL_UNIT * 4), 2);
    }

    /// Four neighbours of one level are due, and three are not.
    #[test]
    fn four_neighbours_of_one_level_are_due_for_a_merge() {
        let segment = |id: i64, level: u32| Segment {
            id,
            level,
            bytes: 10,
        };
        let mut structure = Structure::empty();
        structure.segments = vec![segment(1, 0), segment(2, 0), segment(3, 0)];
        assert_eq!(due_run(&structure), None);
        structure.segments.push(segment(4, 0));
        assert_eq!(due_run(&structure), Some((0, 4)));
        // Four of one level that are not next to each other are not a run.
        structure.segments = vec![
            segment(1, 0),
            segment(2, 1),
            segment(3, 0),
            segment(4, 1),
            segment(5, 0),
            segment(6, 1),
            segment(7, 0),
        ];
        assert_eq!(due_run(&structure), None);
    }
}

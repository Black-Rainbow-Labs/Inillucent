//! The undo buffer of an open transaction.
//!
//! Invariant: **rolling back to any floor puts every row back the way it was when that
//! floor was taken, and a row changed again after a statement has finished costs no new
//! record unless a floor lies between the two changes.** A floor is the start of the
//! transaction, a savepoint, or the statement that is running.
//!
//! ## Why this exists
//!
//! A transaction that updated the same 1,000 rows a million times held a million records,
//! each a copy of a whole row, until it ended: 228 MB. Rolling back pops records newest
//! first, so only the oldest record of a row decides what the row is put back to, and every
//! later record of the same row is popped and then overwritten. [`UndoBuffer::finish_statement`]
//! drops those later records once the statement that wrote them has succeeded, which is the
//! first moment no floor of that statement can ask for them.
//!
//! ## Why it is not done as each record is pushed
//!
//! The statement that is running is a floor too. If it fails, rolling back to its floor must
//! restore each row to what it was when the statement began, and the older record of that
//! row, from an earlier statement, holds an earlier state.
//!
//! ## Why the oldest records move to a file (bug hunt 10)
//!
//! A transaction held one record for every row it changed until it ended, and a record holds the
//! row as it was, or a copy of the leaf the row was in. `BEGIN; UPDATE big SET v = v + 1; COMMIT`
//! over a 205 MB table peaked at 388 MB, 640 MB over 410 MB, and `cache_size` did not change the
//! slope, because the pool already spills its own pages and these records are not pool pages. A
//! database larger than the machine's memory could not finish such a transaction.
//!
//! Past [`RESIDENT_LIMIT`] bytes the buffer writes its oldest records to a temporary file of the
//! database's file system, in chunks (`undo_spill`), and keeps the newest record in memory. Only a
//! rollback reads them back, newest chunk first, which is the order [`UndoBuffer::pop`] asks for
//! them in. A commit throws the file away unread. The file is opened with `delete_on_close`, so a
//! killed process leaves nothing behind, and nothing in it has to survive a crash: rollback of a
//! transaction that was open when the process died is done from the write ahead log by recovery,
//! never from this buffer.
//!
//! The no steal rule is untouched. It is about pages of the database file, and this file holds
//! row images that only this connection's rollback can ask for.
//!
//! A position in the buffer is a count of records since the transaction began, whether a record
//! is in memory or in the file. A floor taken before a spill is still the right floor after it.
//! The only thing a spill takes away is the chance to drop a duplicate record of a statement that
//! has finished, because [`UndoBuffer::finish_statement`] edits the records in memory.

use std::collections::HashMap;
use std::sync::Arc;

use inillucent_base::DbResult;
use inillucent_exec::spill::{Spill, SpillFile};
use inillucent_tree::datum::Datum;
use inillucent_vfs::Vfs;

use crate::engine::undo_spill::{decode_records, encode_records, resident_bytes};
use crate::engine::write::{Before, FromPage};

/// How many bytes of records the buffer keeps in memory before it moves the older ones to a file.
///
/// Sixteen mebibytes: a transaction of a few thousand rows never opens a file, and a transaction
/// of a million rows holds as much as a few thousand do.
const RESIDENT_LIMIT: usize = 16 * 1024 * 1024;

/// About how many bytes of records are written as one chunk, and so read back as one.
const CHUNK_BYTES: usize = 1024 * 1024;

/// How many records a reservation for a statement's expected rows may ask for.
///
/// `expect_undo` reserved room for every row of the statement, 90 bytes a row, before the first
/// record was pushed. Room for more than this is never held at once, because records past the
/// limit above are moved out.
const RESERVE_LIMIT: usize = 16_384;

/// How many rows the running statement remembers the rows of, to drop duplicates afterwards.
///
/// The list held a copy of every key of the statement, one allocation each, though only
/// [`TRACKED_ROWS`] of them are remembered after the statement. A row past the limit keeps its
/// record, which is what it did before the list existed.
const PENDING_ROWS: usize = TRACKED_ROWS;

/// How many rows the buffer remembers the oldest record of. A transaction that changes more
/// distinct rows than this keeps a record for each change of a row it no longer remembers,
/// which is what it did before, so the limit bounds the memory this index costs.
const TRACKED_ROWS: usize = 65_536;

/// A row of a tree, as the buffer identifies it: the schema, the tree, and the tagged
/// encoding of the key the tree was written with.
type RowId = (u8, u64, Vec<u8>);

/// The records of the open transaction, newest last, and the index that lets a repeated change
/// to one row cost nothing once its statement has finished.
///
/// Dereferences to the vector of records, so every reader of the buffer reads it as before.
/// The methods that remove records are redefined here, because removing one makes the index
/// wrong.
#[derive(Debug, Default)]
pub(crate) struct UndoBuffer {
    /// The newest records, in the order the writes happened. The records before them, if any,
    /// are in the file described by `spilled`.
    records: Vec<Before>,
    /// For a row, the position of the oldest record since `boundary` that holds its image.
    tracked: HashMap<RowId, usize>,
    /// The rows with an image in the running statement's records, with each record's position.
    pending: Vec<(usize, RowId)>,
    /// The position of the newest savepoint's floor. A record before it is never dropped for
    /// one after it, because rolling back to the savepoint needs the later image.
    boundary: usize,
    /// The records that were moved to the file, which come before every record in `records`.
    spilled: SpillState,
    /// An estimate of the bytes `records` occupies.
    resident: usize,
}

/// The file the oldest records were moved to, and where each chunk of them lies in it.
#[derive(Default)]
struct SpillState {
    /// The file system to make the file on, or `None` for a buffer that cannot spill.
    vfs: Option<Arc<dyn Vfs>>,
    /// The open file, made when the first chunk is written.
    file: Option<Box<dyn SpillFile>>,
    /// The chunks that are still wanted, oldest first.
    chunks: Vec<Chunk>,
    /// How many records the chunks hold.
    records: usize,
    /// Whether writing a chunk failed, after which the buffer keeps its records in memory.
    failed: bool,
}

impl std::fmt::Debug for SpillState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SpillState")
            .field("chunks", &self.chunks.len())
            .field("records", &self.records)
            .field("failed", &self.failed)
            .finish()
    }
}

/// Where one chunk of records lies in the file.
#[derive(Clone, Copy, Debug)]
struct Chunk {
    /// The offset of the chunk's first byte.
    at: u64,
    /// The chunk's length in bytes.
    bytes: usize,
    /// How many records it holds.
    count: usize,
}

impl std::ops::Deref for UndoBuffer {
    type Target = Vec<Before>;

    fn deref(&self) -> &Vec<Before> {
        &self.records
    }
}

impl std::ops::DerefMut for UndoBuffer {
    fn deref_mut(&mut self) -> &mut Vec<Before> {
        &mut self.records
    }
}

impl UndoBuffer {
    /// Names the file system the buffer may move its oldest records onto.
    ///
    /// @param vfs - the file system the database was opened on
    pub(crate) fn spill_onto(&mut self, vfs: Arc<dyn Vfs>) {
        self.spilled.vfs = Some(vfs);
    }

    /// Returns how many records the buffer holds, in memory and in the file.
    pub(crate) fn len(&self) -> usize {
        self.spilled.records.saturating_add(self.records.len())
    }

    /// Reports whether the buffer holds no record.
    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Makes room for records a statement expects to write, as far as they will stay in memory.
    ///
    /// @param rows - how many records the statement expects
    pub(crate) fn reserve(&mut self, rows: usize) {
        self.records.reserve(rows.min(RESERVE_LIMIT));
    }

    /// Adds a record.
    ///
    /// @param record - the record
    pub(crate) fn push(&mut self, record: Before) {
        let mut last_page = self.records.last().map_or(0, page_address);
        self.resident = self
            .resident
            .saturating_add(resident_bytes(&record, &mut last_page));
        self.records.push(record);
        if self.resident >= RESIDENT_LIMIT {
            self.move_older_records_to_the_file();
        }
    }

    /// Adds a record of a row whose before image the record holds, and remembers which row.
    ///
    /// @param record - the record, with the row it came from
    /// @param key - the key the row was written with, one value per key column
    pub(crate) fn push_image(&mut self, record: Before, key: &[Datum<'_>]) {
        if self.pending.len() < PENDING_ROWS {
            let id = row_id(record.schema, record.tree, key);
            self.pending.push((self.len(), id));
        }
        self.push(record);
    }

    /// Starts a statement: forgets the rows the previous one wrote and closes its last record
    /// to merging. The floor of the statement is the length the caller reads next.
    pub(crate) fn begin_statement(&mut self) {
        self.pending.clear();
        if let Some(newest) = self.records.last_mut() {
            newest.sealed = true;
        }
    }

    /// Starts a savepoint, which is a floor that outlives the statement that opens it.
    pub(crate) fn begin_savepoint(&mut self) {
        self.begin_statement();
        self.boundary = self.len();
    }

    /// Removes the newest record, reading it back from the file when it was moved there.
    pub(crate) fn pop(&mut self) -> DbResult<Option<Before>> {
        self.forget_index();
        if self.records.is_empty() && !self.spilled.chunks.is_empty() {
            self.read_back_the_newest_chunk()?;
        }
        let popped = self.records.pop();
        if let Some(record) = &popped {
            // An estimate that may count a shared copy of a leaf more than once; it is set right
            // again whenever a chunk is read back or the buffer is cleared.
            self.resident = self
                .resident
                .saturating_sub(resident_bytes(record, &mut 0usize));
        }
        Ok(popped)
    }

    /// Removes every record, and the file they were moved to.
    pub(crate) fn clear(&mut self) {
        self.forget_index();
        self.boundary = 0;
        self.records.clear();
        self.resident = 0;
        self.spilled.chunks.clear();
        self.spilled.records = 0;
        self.spilled.file = None;
        self.spilled.failed = false;
    }

    /// Counts the estimated bytes of the records in memory, which reads each of them once.
    fn recount_resident(&self) -> usize {
        let mut last_page = 0usize;
        self.records.iter().fold(0usize, |sum, record| {
            sum.saturating_add(resident_bytes(record, &mut last_page))
        })
    }

    /// Writes every record but the newest to the file, in chunks, and drops them from memory.
    ///
    /// **The newest record stays**, because [`UndoBuffer::begin_statement`] seals it and the
    /// engine's `extend_rowid_run` folds the next insert into it. A failure to write leaves every
    /// record in memory and stops the buffer trying again, which is what it did before it could
    /// spill: a full disk must not turn into a failed statement.
    fn move_older_records_to_the_file(&mut self) {
        if self.spilled.failed || self.spilled.vfs.is_none() || self.records.len() < 2 {
            return;
        }
        let mut all = std::mem::take(&mut self.records);
        let newest = all.pop();
        match self.write_chunks(&all) {
            Ok(chunks) => {
                self.spilled.records = self.spilled.records.saturating_add(all.len());
                self.spilled.chunks.extend(chunks);
                self.records = newest.into_iter().collect();
                self.resident = self.recount_resident();
            }
            Err(_) => {
                self.spilled.failed = true;
                all.extend(newest);
                self.records = all;
            }
        }
    }

    /// Appends records to the file in chunks and returns where each lies.
    ///
    /// @param records - the records, oldest first
    fn write_chunks(&mut self, records: &[Before]) -> DbResult<Vec<Chunk>> {
        if self.spilled.file.is_none() {
            let vfs = self.spilled.vfs.as_ref().map(Arc::clone);
            let Some(vfs) = vfs else {
                return Ok(Vec::new());
            };
            self.spilled.file = Some(crate::spillfile::VfsSpill::new(vfs).open()?);
        }
        let Some(file) = self.spilled.file.as_mut() else {
            return Ok(Vec::new());
        };
        let mut chunks = Vec::new();
        let mut last_page = 0usize;
        let mut start = 0usize;
        let mut weight = 0usize;
        for (position, record) in records.iter().enumerate() {
            weight = weight.saturating_add(resident_bytes(record, &mut last_page));
            let end = position.saturating_add(1);
            if weight < CHUNK_BYTES && end < records.len() {
                continue;
            }
            let bytes = encode_records(records.get(start..end).unwrap_or(&[]));
            let at = file.append(&bytes)?;
            chunks.push(Chunk {
                at,
                bytes: bytes.len(),
                count: end - start,
            });
            start = end;
            weight = 0;
        }
        Ok(chunks)
    }

    /// Reads the newest chunk of the file back into memory, as the records the buffer ends with.
    fn read_back_the_newest_chunk(&mut self) -> DbResult<()> {
        let Some(chunk) = self.spilled.chunks.last().copied() else {
            return Ok(());
        };
        let Some(file) = self.spilled.file.as_ref() else {
            return Ok(());
        };
        let mut bytes = vec![0u8; chunk.bytes];
        file.read_at(chunk.at, &mut bytes)?;
        let mut older = decode_records(&bytes)?;
        self.spilled.chunks.pop();
        self.spilled.records = self.spilled.records.saturating_sub(chunk.count);
        older.append(&mut self.records);
        self.records = older;
        self.resident = self.recount_resident();
        Ok(())
    }

    /// Forgets the positions the index holds, which a removal makes wrong.
    fn forget_index(&mut self) {
        self.tracked.clear();
        self.pending.clear();
    }

    /// Drops the records of a statement that has succeeded when an older record already holds
    /// the image of the same row.
    ///
    /// **Only for a statement that is not inside another and has no savepoint opened since the
    /// older record.** The caller checks the first; `boundary` records the second.
    ///
    /// @param floor - the length of the buffer when the statement began
    pub(crate) fn finish_statement(&mut self, floor: usize) {
        let pending = std::mem::take(&mut self.pending);
        if pending.is_empty() || floor > self.len() {
            return;
        }
        let in_file = self.spilled.records;
        let mut dropped: Vec<usize> = Vec::new();
        let mut first_seen: HashMap<&RowId, usize> = HashMap::new();
        let mut remembered: Vec<(&RowId, usize)> = Vec::new();
        for (position, id) in &pending {
            if *position < floor {
                continue;
            }
            let older = self
                .tracked
                .get(id)
                .copied()
                .filter(|older| *older >= self.boundary);
            if older.is_some_and(|older| older < floor) || first_seen.contains_key(id) {
                // A duplicate that was moved to the file stays there: it is harmless, and
                // reading it back to drop it would cost what moving it saved.
                if *position >= in_file {
                    dropped.push(*position);
                }
                continue;
            }
            first_seen.insert(id, *position);
            remembered.push((id, *position));
        }
        let moved = self.remove_positions(floor, &dropped);
        for (id, position) in remembered {
            if self.tracked.len() >= TRACKED_ROWS && !self.tracked.contains_key(id) {
                continue;
            }
            let at = position.saturating_sub(moved(position));
            self.tracked.insert(id.clone(), at);
        }
    }

    /// Removes records at the given positions, all at or after `floor`, and returns a function
    /// giving how many removed records lay before a position.
    ///
    /// @param floor - the first position that may be removed
    /// @param dropped - the positions to remove, ascending
    fn remove_positions(&mut self, floor: usize, dropped: &[usize]) -> impl Fn(usize) -> usize {
        if !dropped.is_empty() {
            // Positions count the records in the file too; `records` starts after them.
            let in_file = self.spilled.records;
            let start = floor.max(in_file);
            let tail = self.records.split_off(start.saturating_sub(in_file));
            let mut next = dropped.iter().copied().peekable();
            for (offset, record) in tail.into_iter().enumerate() {
                if next.peek() == Some(&start.saturating_add(offset)) {
                    next.next();
                    continue;
                }
                self.records.push(record);
            }
            self.resident = self.recount_resident();
        }
        let removed: Vec<usize> = dropped.to_vec();
        move |position| removed.iter().filter(|gone| **gone < position).count()
    }
}

/// Returns the address of the copy of a leaf a record names, or zero when it names none.
///
/// @param record - the record
fn page_address(record: &Before) -> usize {
    match record.page.as_deref() {
        Some(FromPage::One(page, _)) | Some(FromPage::Run(page, _)) => page.as_ptr() as usize,
        None => 0,
    }
}

/// Builds the identity of a row.
///
/// @param schema - the schema of the tree
/// @param tree - the tree
/// @param key - the key values
fn row_id(schema: u8, tree: u64, key: &[Datum<'_>]) -> RowId {
    let mut bytes = Vec::new();
    for value in key {
        value.encode_tagged(&mut bytes);
    }
    (schema, tree, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::write::KeyCopy;
    use inillucent_tree::datum::OwnedDatum;

    /// Builds a record that holds a row of about a kilobyte, numbered by `number`.
    fn record(number: i64) -> Before {
        Before {
            schema: 0,
            sealed: false,
            tree: 1,
            key: KeyCopy::Empty,
            row: Some(vec![
                OwnedDatum::Int(number),
                OwnedDatum::Blob(vec![(number % 251) as u8; 1000]),
            ]),
            page: None,
        }
    }

    /// Returns the number a record's row was built with.
    fn number_of(record: &Before) -> i64 {
        match record.row.as_deref() {
            Some([OwnedDatum::Int(number), ..]) => *number,
            _ => -1,
        }
    }

    /// A buffer that holds more than the limit moves records to the file and gives every one back
    /// in the order a rollback needs, newest first.
    #[test]
    fn records_moved_to_the_file_come_back_newest_first() {
        let mut buffer = UndoBuffer::default();
        buffer.spill_onto(Arc::new(inillucent_vfs::memory::MemoryVfs::new()));
        let total = 40_000i64;
        for number in 0..total {
            buffer.push(record(number));
        }
        assert_eq!(buffer.len(), total as usize);
        assert!(
            buffer.records.len() < 20_000,
            "{} records stayed in memory, so none were moved to the file",
            buffer.records.len()
        );
        for expected in (0..total).rev() {
            let popped = buffer
                .pop()
                .expect("the file reads back")
                .expect("a record");
            assert_eq!(number_of(&popped), expected);
            assert_eq!(
                popped.row.as_deref().and_then(|row| row.get(1)),
                Some(&OwnedDatum::Blob(vec![(expected % 251) as u8; 1000]))
            );
        }
        assert!(buffer.is_empty());
        assert!(buffer.pop().expect("an empty buffer reads").is_none());
    }

    /// A floor taken before records were moved still counts the same records afterwards, and a
    /// statement that finishes after a spill does not lose or duplicate any.
    #[test]
    fn a_floor_taken_before_a_spill_still_cuts_at_the_same_record() {
        let mut buffer = UndoBuffer::default();
        buffer.spill_onto(Arc::new(inillucent_vfs::memory::MemoryVfs::new()));
        for number in 0..100 {
            buffer.push(record(number));
        }
        buffer.begin_statement();
        let floor = buffer.len();
        for number in 100..30_100 {
            buffer.push_image(record(number), &[Datum::Int(number)]);
        }
        buffer.finish_statement(floor);
        assert_eq!(
            buffer.len(),
            30_100,
            "a statement of distinct rows keeps every record"
        );
        let mut seen = 0usize;
        while buffer.len() > floor {
            let popped = buffer.pop().expect("reads").expect("a record");
            assert_eq!(number_of(&popped), (buffer.len()) as i64);
            seen += 1;
        }
        assert_eq!(seen, 30_000);
        assert_eq!(buffer.len(), 100);
    }

    /// `clear` forgets the records in the file, and the buffer is usable afterwards.
    #[test]
    fn clearing_forgets_the_records_in_the_file() {
        let mut buffer = UndoBuffer::default();
        buffer.spill_onto(Arc::new(inillucent_vfs::memory::MemoryVfs::new()));
        for number in 0..30_000 {
            buffer.push(record(number));
        }
        buffer.clear();
        assert!(buffer.is_empty());
        buffer.push(record(7));
        assert_eq!(
            number_of(&buffer.pop().expect("reads").expect("a record")),
            7
        );
    }
}

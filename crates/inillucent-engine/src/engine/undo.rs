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

use std::collections::HashMap;

use inillucent_tree::datum::Datum;

use crate::engine::write::Before;

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
    /// Every record, in the order the writes happened.
    records: Vec<Before>,
    /// For a row, the position of the oldest record since `boundary` that holds its image.
    tracked: HashMap<RowId, usize>,
    /// The rows with an image in the running statement's records, with each record's position.
    pending: Vec<(usize, RowId)>,
    /// The position of the newest savepoint's floor. A record before it is never dropped for
    /// one after it, because rolling back to the savepoint needs the later image.
    boundary: usize,
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
    /// Adds a record of a row whose before image the record holds, and remembers which row.
    ///
    /// @param record - the record, with the row it came from
    /// @param key - the key the row was written with, one value per key column
    pub(crate) fn push_image(&mut self, record: Before, key: &[Datum<'_>]) {
        let id = row_id(record.schema, record.tree, key);
        self.pending.push((self.records.len(), id));
        self.records.push(record);
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
        self.boundary = self.records.len();
    }

    /// Removes the newest record.
    pub(crate) fn pop(&mut self) -> Option<Before> {
        self.forget_index();
        self.records.pop()
    }

    /// Removes every record.
    pub(crate) fn clear(&mut self) {
        self.forget_index();
        self.boundary = 0;
        self.records.clear();
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
        if pending.is_empty() || floor > self.records.len() {
            return;
        }
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
                dropped.push(*position);
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
            let tail = self.records.split_off(floor);
            let mut next = dropped.iter().copied().peekable();
            for (offset, record) in tail.into_iter().enumerate() {
                if next.peek() == Some(&floor.saturating_add(offset)) {
                    next.next();
                    continue;
                }
                self.records.push(record);
            }
        }
        let removed: Vec<usize> = dropped.to_vec();
        move |position| removed.iter().filter(|gone| **gone < position).count()
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

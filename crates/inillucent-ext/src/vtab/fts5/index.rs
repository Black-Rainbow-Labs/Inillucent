//! The pending buffer, the doclists in it, and the index's own totals.
//!
//! Invariant: **one doclist per term, and it is rewritten rather than
//! segmented.** There are no segments to merge, which is why the merge
//! commands are accepted and do nothing: a term's postings are one blob, and
//! an append rewrites it under one lock.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use inillucent_base::DbResult;
use inillucent_value::Value;

use super::super::Context;
use crate::shadow::ShadowTables;

use super::doclist::{
    append_doclist_entry, last_doclist_rowid, read_varint, remove_entry_in_place, write_varint,
    DocEntry,
};
use super::*;

/// Where an FTS5 index build spends its time, in nanoseconds.
///
/// **Measured rather than reasoned about**, which is the rule Phase 3's Part E
/// states for the read families and which applies here: the build path could be
/// tokenisation, the per-row shadow write, the doclist merge or the same
/// allocation-per-row shape found earlier in the index build, and guessing
/// which has been wrong twice on this project.
///
/// It is kept the way `create_index`'s stage timing is kept - permanently, and
/// read by the gate beside the ratio - because a workload under a bar needs its
/// breakdown printed beside the number, not reconstructed by a later ticket
/// from a profile nobody kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuildStages {
    /// How many rows were indexed.
    pub rows: u64,
    /// Writing the row into `%_content`.
    pub content: u128,
    /// Turning the row's text into tokens.
    pub tokenize: u128,
    /// Encoding the per-column token counts and writing `%_docsize`.
    pub docsize: u128,
    /// Sorting the postings and grouping them by term, without the merge.
    pub group: u128,
    /// Adding this document to a term's doclist, for a term this transaction
    /// has already seen.
    pub terms: u128,
    /// The same, for a term this transaction has not seen before: finding or
    /// creating its `%_idx` row, and reading its doclist back.
    pub new_terms: u128,
    /// How many terms took that path.
    pub new_term_count: u64,
    /// Looking a term up in the `%_idx` dictionary.
    pub dictionary_read: u128,
    /// Writing a new term's `%_idx` row.
    pub dictionary_write: u128,
    /// How many bytes of doclists the flushes wrote.
    ///
    /// A count, so a test can check that a bulk load writes the index about once without a clock.
    /// A load that writes every term it touched at every statement writes the index again for each
    /// statement, and this grows with the square of the rows.
    pub flushed_bytes: u64,
    /// Reading, updating and staging the corpus totals `bm25` needs.
    pub totals: u128,
    /// Writing the staged doclists out to `%_data`.
    pub flush: u128,
    /// The whole of one `add`, from its first statement to its last.
    ///
    /// **Because the named stages do not add up, and guessing which one grew is how a
    /// ticket optimises the wrong thing** (task-2006). They summed to about 5.5 ms of
    /// `extension.fts.build`'s 8.1, leaving 2.6 ms that could have been inside `add`
    /// between two timers or outside it in the SQL and virtual table plumbing. This
    /// says which: `whole` minus the other stages is what `add` does and does not name,
    /// and the workload's own time minus `whole` is everything around it.
    pub whole: u128,
}
/// Adds one measurement to this thread's tally.
///
/// @param edit - what to add
pub(crate) fn record_stage(edit: impl FnOnce(&mut BuildStages)) {
    BUILD_STAGES.with(|held| {
        let mut stages = held.get();
        edit(&mut stages);
        held.set(stages);
    });
}
/// Returns where the FTS5 builds on this thread have spent their time.
pub fn build_stages() -> BuildStages {
    BUILD_STAGES.with(|held| held.get())
}
/// Forgets what this thread's FTS5 builds have spent, so the next is on its own.
pub fn reset_build_stages() {
    BUILD_STAGES.with(|held| held.set(BuildStages::default()));
}
/// The totals `bm25` needs: how many rows, and how many tokens per column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Totals {
    /// How many rows the index holds.
    pub rows: i64,
    /// How many tokens each column holds across every row.
    pub tokens: Vec<i64>,
}
impl Totals {
    /// Returns the totals of an empty index.
    pub(crate) fn empty(columns: usize) -> Totals {
        Totals {
            rows: 0,
            tokens: vec![0; columns],
        }
    }

    /// Returns the totals as the bytes `%_data` row one holds.
    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        write_varint(&mut out, self.rows as u64);
        write_varint(&mut out, self.tokens.len() as u64);
        for count in &self.tokens {
            write_varint(&mut out, *count as u64);
        }
        out
    }

    /// Reads the totals back.
    pub(crate) fn decode(bytes: &[u8]) -> Totals {
        let mut at = 0usize;
        let rows = read_varint(bytes, &mut at) as i64;
        let columns = read_varint(bytes, &mut at) as usize;
        let mut tokens = Vec::with_capacity(columns);
        for _ in 0..columns.min(1024) {
            tokens.push(read_varint(bytes, &mut at) as i64);
        }
        Totals { rows, tokens }
    }
}
/// Reads the totals row.
pub(crate) fn get_totals(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    columns: usize,
) -> Totals {
    let Ok(Some(row)) = shadows.read_row(context, b"data", TOTALS) else {
        return Totals::empty(columns);
    };
    let Some(blob) = row.get(1).and_then(Value::as_blob) else {
        return Totals::empty(columns);
    };
    Totals::decode(blob.raw())
}
/// Returns the totals, from the buffer when this transaction has them.
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the staged rows
/// @param columns - how many columns the table has
pub(crate) fn buffered_totals(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
    columns: usize,
) -> Totals {
    if let Ok(held) = buffer.lock() {
        if let Some(totals) = held.totals.as_ref() {
            return totals.clone();
        }
    }
    let totals = get_totals(context, shadows, columns);
    if let Ok(mut held) = buffer.lock() {
        held.totals = Some(totals.clone());
    }
    totals
}
/// Stages the totals, to be written when the transaction flushes.
///
/// @param buffer - the staged rows
/// @param totals - the totals as they now stand
pub(crate) fn stage_totals(buffer: &Buffer, totals: Totals) {
    if let Ok(mut held) = buffer.lock() {
        held.totals = Some(totals);
        held.totals_dirty = true;
    }
}
/// Writes the totals row.
pub(crate) fn put_totals(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    totals: &Totals,
) -> DbResult<()> {
    shadows.write_row(
        context,
        b"data",
        TOTALS,
        &[Value::Null, Value::owned_blob(&totals.encode())?],
    )
}
/// The doclists this transaction has changed but has not written yet.
///
/// **A doclist is rewritten once per transaction, not once per document.**
/// `merge_term` reads a term's whole doclist, appends one entry and writes the
/// whole thing back, so a term that appears in every document of a bulk load is
/// read and written once per document and the bytes moved grow with the load:
/// five hundred documents over a ten-word vocabulary moved about ten megabytes
/// to store thirty kilobytes. The entries are the same entries and the row is
/// the same row; only the number of times it is written changes.
///
/// It is shared with the cursors the table opens, so a query inside the same
/// transaction reads what the transaction has written - which is what makes
/// this a buffer rather than a delayed write.
#[derive(Default)]
pub struct Pending {
    /// A term's doclist, keyed by the term itself.
    ///
    /// **The dictionary row and the doclist are the same row now, so the
    /// cache of one is the cache of the other.** A term this transaction has
    /// looked up - to append to it or merely to answer a query - is staged
    /// here whether or not its bytes have changed, which is what lets a
    /// document that repeats a common word skip a second read of `%_idx` for
    /// it: the map holds the one row `%_idx` would otherwise be asked for
    /// again.
    ///
    /// **A `BTreeMap` because the order it iterates in *is* the key order the
    /// flush wants.** `%_idx` is an index tree keyed by the term, and the
    /// terms of a document arrive in whatever order the text put them in - so
    /// writing each as it appears is a descent and a possible split per term,
    /// into a tree that is growing under it. Held here and written in key
    /// order at the flush, the same run of terms is a walk to the right of the
    /// tree instead. It was measured before it was done: `extension.fts.build`
    /// spent 1.8 of its 9.4 ms writing 507 dictionary rows one at a time,
    /// against 0.5 ms reading them and 1.2 ms writing every doclist - and
    /// those two writes are now one.
    ///
    /// **Every reader that scans `%_idx` flushes first.** A staged row is
    /// invisible to a scan, and the scans are `expr.rs`'s prefix search,
    /// `integrity`, `rebuild` and `remove`.
    pub(crate) doclists: BTreeMap<Vec<u8>, Staged>,
    /// How many bytes the doclists have grown or been rewritten by since the last flush, so the
    /// buffer can be bounded.
    ///
    /// **Changed bytes, not held bytes.** A list that was only read counts nothing. Counting the
    /// lists a statement merely read made the buffer look full as soon as the hot terms of a large
    /// index were staged, and it was written out after every statement.
    pub(crate) bytes: usize,
    /// How many bytes every staged doclist holds, changed or not. Bounds memory.
    pub(crate) resident: usize,
    /// The savepoints that are open, innermost last, and how to put the buffer back to each.
    pub(crate) epochs: Vec<Epoch>,
    /// The totals row, read once and written once per transaction.
    ///
    /// **One `%_data` row, and it was being rewritten per document.** `add`
    /// finishes by reading row 1, adding this document's counts and writing it
    /// back, so a bulk load of five hundred documents read and wrote the same
    /// row five hundred times - and each of those is a descent, a log record
    /// and a page image on a row that nothing reads until a query asks for a
    /// score. `fts.build` measured 0.10x against SQLite with 22.6 of its 23.7
    /// ms inside the module, and this is the part of it that is pure repetition.
    ///
    /// `None` means "not read yet this transaction", which is different from
    /// "there are no totals": an empty index has a totals row of zeroes.
    totals: Option<Totals>,
    /// Whether the buffered totals differ from what `%_data` holds.
    totals_dirty: bool,
    /// Which layout the index on disk is in, once something has asked.
    ///
    /// `None` means "not read yet on this connection". It is read once and
    /// never cleared, unlike [`Pending::totals`], because it is a property of
    /// the file rather than of the transaction: the layout an index is in
    /// cannot change while this connection holds it open, and asking again per
    /// query would put a tree descent in front of every search. See
    /// `layout.rs` for what the number means and what a missing record
    /// reads as.
    pub(crate) layout: Option<u32>,
    /// The release the layout record names, empty when there is no record.
    pub(crate) layout_writer: String,
    /// The highest `%_content` rowid this transaction has handed out.
    ///
    /// **Two tree descents per document, for a number the module already
    /// knows.** An insert that names no rowid asked `max_rowid` for one - a
    /// descent to the rightmost leaf - and then asked `read_row` whether that
    /// rowid was taken, which is a second descent for a question whose answer
    /// is no by construction. Together they were half the descents a document
    /// costs, and `fts.build` is five hundred documents in one transaction.
    ///
    /// It is cleared at `sync`, which is the transaction boundary, so a rowid
    /// freed by a delete in a *later* transaction is reused exactly as SQLite
    /// reuses it. Within one transaction the number only rises, which is the
    /// same rule a rowid table follows for rows it has just written.
    pub(crate) content_highest: Option<i64>,
}
/// One staged doclist, and the rowid it currently ends at.
///
/// **The rowid is remembered because finding it costs a walk of the whole
/// list.** A doclist is a run of varints with no length prefix and no index, so
/// the only way to read the last entry is to decode every entry before it -
/// and an append has to know it, because entries are stored as deltas.
///
/// That made appending O(the list so far), and a bulk load appends to the same
/// term once per document: five hundred documents over a ten-word vocabulary
/// walked about a hundred and twenty thousand entries to add five thousand.
/// `merge_term`'s own doc comment records the identical shape being fixed for
/// the *unbuffered* path, by appending instead of decoding; the buffered path
/// it introduced brought the walk back one level down.
///
/// **Two forms, because the two jobs want different things.** A bulk load only
/// ever adds a row after every row the list holds, and the bytes form appends
/// to the encoded list without decoding anything. An `UPDATE` or a `DELETE`
/// takes a row out of the middle, and in the bytes form that means decoding the
/// whole list, dropping one entry and encoding it again, for every row touched.
/// A term that is in every row of a table made an `UPDATE` of all its rows take
/// time that grew as the square of the row count: 8,000 rows took 7.7 s where
/// SQLite takes 0.04 s. The first edit in the middle of a list therefore
/// decodes it once into a map keyed by rowid, every later edit of that list is
/// a map operation, and the list is encoded once when it is read or flushed.
#[derive(Clone, Default)]
pub(crate) struct Staged {
    /// Whether the list is exactly what `%_idx` holds, because it was only read.
    ///
    /// A clean list is not written at a flush. Reading a term to append to it and then not
    /// changing it, which a query inside the transaction does, used to rewrite the whole row.
    clean: bool,
    /// The doclist as `%_idx` will hold it, while it has no edits in the middle.
    bytes: Vec<u8>,
    /// The rowid the last entry names, when there is one.
    last: Option<i64>,
    /// The entries by rowid, once an edit in the middle has decoded the list.
    ///
    /// `None` means `bytes` is the list. `Some` means this map is the list and
    /// `bytes` and `last` are not used.
    edited: Option<BTreeMap<i64, Vec<(usize, Vec<u32>)>>>,
    /// The encoded length of the list in the edited form, kept up to date as
    /// entries are added and removed so the buffer can be bounded.
    edited_size: usize,
    /// How many times a row has been cut out of the bytes since the list was
    /// read, which decides when the list moves to the edited form.
    cuts: u8,
}

/// How many rows are cut out of a list's bytes before it moves to the map.
const MAX_CUTS_FROM_BYTES: u8 = 2;

/// Returns how many bytes one entry takes once encoded, as an estimate.
///
/// Exact enough to bound the buffer: each number is counted as two bytes.
///
/// @param columns - the entry's positions by column
fn entry_size(columns: &[(usize, Vec<u32>)]) -> usize {
    let mut size = 4usize;
    for (_, positions) in columns {
        size = size.saturating_add(4);
        size = size.saturating_add(positions.len().saturating_mul(2));
    }
    size
}

impl Staged {
    /// Wraps a doclist read from `%_idx` or built from scratch.
    ///
    /// @param bytes - the encoded doclist
    pub(crate) fn from_bytes(bytes: Vec<u8>) -> Staged {
        // The walk happens here, once per whole-list rewrite, rather than on
        // every append: this is the path a list reaches when it is read back or
        // built from scratch, and it is the only place the end is not already
        // known.
        let last = last_doclist_rowid(&bytes);
        Staged {
            clean: true,
            bytes,
            last,
            edited: None,
            edited_size: 0,
            cuts: 0,
        }
    }

    /// Reports whether the list is exactly what `%_idx` holds.
    pub(crate) fn is_clean(&self) -> bool {
        self.clean
    }

    /// Reports whether `put_row` would append this row to the end of the encoded bytes.
    ///
    /// That is the one change that can be undone by cutting the bytes back to where they were.
    ///
    /// @param rowid - the row about to be put
    pub(crate) fn appends(&self, rowid: i64) -> bool {
        self.edited.is_none() && self.last.is_some_and(|last| rowid > last)
    }

    /// Cuts a list that has only been appended to back to an earlier length.
    ///
    /// @param len - how many bytes the list had
    /// @param last - the rowid its last entry named
    fn truncate_appended(&mut self, len: usize, last: Option<i64>) {
        self.bytes.truncate(len);
        self.last = last;
    }

    /// Returns the encoded doclist.
    pub(crate) fn encoded(&self) -> std::borrow::Cow<'_, [u8]> {
        match &self.edited {
            None => std::borrow::Cow::Borrowed(self.bytes.as_slice()),
            Some(entries) => std::borrow::Cow::Owned(encode_entries(entries)),
        }
    }

    /// Returns the encoded doclist, consuming the staged one.
    pub(crate) fn into_bytes(self) -> Vec<u8> {
        match self.edited {
            None => self.bytes,
            Some(entries) => encode_entries(&entries),
        }
    }

    /// Returns how many bytes the list takes, or about that many once edited.
    pub(crate) fn size(&self) -> usize {
        match self.edited {
            None => self.bytes.len(),
            Some(_) => self.edited_size,
        }
    }

    /// Returns whether the list has no entries.
    pub(crate) fn is_empty(&self) -> bool {
        match &self.edited {
            None => self.bytes.is_empty(),
            Some(entries) => entries.is_empty(),
        }
    }

    /// Decodes the list into the edited form, if it is not in it already.
    fn edit(&mut self) -> &mut BTreeMap<i64, Vec<(usize, Vec<u32>)>> {
        if self.edited.is_none() {
            let decoded = decode_doclist(&self.bytes);
            self.edited_size = self.bytes.len();
            self.bytes = Vec::new();
            self.last = None;
            self.edited = Some(
                decoded
                    .into_iter()
                    .map(|entry| (entry.rowid, entry.columns))
                    .collect(),
            );
        }
        self.edited.get_or_insert_with(BTreeMap::new)
    }

    /// Takes one row out of the list, wherever it is.
    ///
    /// @param rowid - the row to remove
    pub(crate) fn remove_row(&mut self, rowid: i64) {
        self.clean = false;
        // The first removals cut the entry out of the bytes, which decodes
        // nothing. A list that loses more than that in one transaction is being
        // emptied row by row, and cutting from the bytes each time would walk
        // the whole list each time, so it goes to the map.
        if self.edited.is_none() && self.cuts < MAX_CUTS_FROM_BYTES {
            self.cuts = self.cuts.saturating_add(1);
            let cut = remove_entry_in_place(&mut self.bytes, &mut self.last, rowid);
            if cut.is_some() {
                return;
            }
        }
        let removed = self.edit().remove(&rowid);
        if let Some(columns) = removed {
            self.edited_size = self.edited_size.saturating_sub(entry_size(&columns));
        }
    }

    /// Puts one row into the list, replacing the row of the same rowid.
    ///
    /// Appends to the bytes when the row sorts after every row there, which is
    /// the bulk load. Answers false, and changes nothing, when the list is in
    /// the bytes form and its end is not known, so the caller takes the general
    /// path that decodes it.
    ///
    /// @param rowid - the row
    /// @param columns - the positions of the term in the row, by column
    /// @param run - the same postings as a sorted run, when the caller has one
    pub(crate) fn put_row(
        &mut self,
        rowid: i64,
        columns: impl FnOnce() -> Vec<(usize, Vec<u32>)>,
        run: Option<&[(Vec<u8>, usize, u32)]>,
    ) -> bool {
        self.clean = false;
        if self.edited.is_none() {
            let Some(last) = self.last else {
                return false;
            };
            if rowid > last {
                match run {
                    Some(run) => append_run(&mut self.bytes, rowid.wrapping_sub(last), run),
                    None => {
                        let entry = DocEntry {
                            rowid,
                            columns: columns(),
                        };
                        append_doclist_entry(&mut self.bytes, rowid.wrapping_sub(last), &entry);
                    }
                }
                self.last = Some(rowid);
                return true;
            }
        }
        let columns = columns();
        let added = entry_size(&columns);
        let replaced = self.edit().insert(rowid, columns);
        self.edited_size = self
            .edited_size
            .saturating_add(added)
            .saturating_sub(replaced.map(|held| entry_size(&held)).unwrap_or(0));
        true
    }
}

/// How to put one term's staged doclist back to what it was when a savepoint opened.
#[derive(Clone)]
pub(crate) enum Undo {
    /// The term was not staged, so the way back is to remove it.
    Absent,
    /// Rows were appended to the end of the encoded bytes, so the way back is to cut them off.
    ///
    /// **Recorded this way because an append is the whole of a bulk load.** Keeping a copy of the
    /// list would make every statement copy every term it touched, which is the cost of the index
    /// again. The length and the last rowid are two numbers.
    Appended {
        /// How long the encoded list was.
        len: usize,
        /// The rowid its last entry named.
        last: Option<i64>,
    },
    /// The list was changed in some other way, so the way back is the list as it was.
    Whole(Staged),
}

/// The buffer's counters, which a savepoint has to bring back along with the doclists.
#[derive(Clone, Default)]
pub(crate) struct Counters {
    /// `Pending::bytes`.
    bytes: usize,
    /// `Pending::resident`.
    resident: usize,
    /// `Pending::totals`.
    totals: Option<Totals>,
    /// `Pending::totals_dirty`.
    totals_dirty: bool,
    /// `Pending::content_highest`.
    content_highest: Option<i64>,
}

/// The whole buffer as it stood at some moment.
#[derive(Clone)]
pub(crate) struct Snapshot {
    /// The staged doclists.
    doclists: BTreeMap<Vec<u8>, Staged>,
    /// The counters.
    counters: Counters,
}

/// What a savepoint needs to put the buffer back to where it was when the savepoint opened.
///
/// **Until a flush happens inside it, an epoch is a record of what changed.** A term's first
/// change since the savepoint opened is recorded, and putting the buffer back replays the records.
/// A flush writes the buffer out, so after one the records say nothing about the buffer that now
/// exists. The flush first works out the whole buffer as it was when each open savepoint opened and
/// keeps that in `restore`, and that is what rolling back to the savepoint uses.
#[derive(Default)]
pub(crate) struct Epoch {
    /// Each term's way back, recorded the first time the term changes.
    undo: BTreeMap<Vec<u8>, Undo>,
    /// The counters when the savepoint opened.
    counters: Counters,
    /// The whole buffer when the savepoint opened, once a flush made `undo` useless.
    restore: Option<Box<Snapshot>>,
}

/// Puts the staged doclists back, term by term, from an epoch's records.
///
/// @param doclists - the doclists as they are now, changed in place
/// @param undo - the records of one epoch
fn replay_undo(doclists: &mut BTreeMap<Vec<u8>, Staged>, undo: &BTreeMap<Vec<u8>, Undo>) {
    for (term, way_back) in undo {
        match way_back {
            Undo::Absent => {
                doclists.remove(term);
            }
            Undo::Appended { len, last } => {
                if let Some(staged) = doclists.get_mut(term) {
                    staged.truncate_appended(*len, *last);
                }
            }
            Undo::Whole(staged) => {
                doclists.insert(term.clone(), staged.clone());
            }
        }
    }
}

impl Pending {
    /// Returns the counters as they are now.
    fn counters(&self) -> Counters {
        Counters {
            bytes: self.bytes,
            resident: self.resident,
            totals: self.totals.clone(),
            totals_dirty: self.totals_dirty,
            content_highest: self.content_highest,
        }
    }

    /// Puts the counters back.
    ///
    /// @param counters - what `counters` answered earlier
    fn set_counters(&mut self, counters: Counters) {
        self.bytes = counters.bytes;
        self.resident = counters.resident;
        self.totals = counters.totals;
        self.totals_dirty = counters.totals_dirty;
        self.content_highest = counters.content_highest;
    }

    /// Opens a savepoint at a level.
    ///
    /// A level the buffer already holds is stale: a statement that failed leaves its savepoint
    /// open, because nothing releases it, and the next one is numbered from the same level.
    ///
    /// @param level - how many savepoints were open before this one
    pub(crate) fn open_epoch(&mut self, level: usize) {
        self.epochs.truncate(level);
        while self.epochs.len() < level {
            let counters = self.counters();
            self.epochs.push(Epoch {
                counters,
                ..Epoch::default()
            });
        }
        let counters = self.counters();
        self.epochs.push(Epoch {
            counters,
            ..Epoch::default()
        });
    }

    /// Closes the savepoints from a level up, keeping what they changed.
    ///
    /// Their records pass to the savepoint below, which has none of its own for a term they
    /// changed first: the record is the term as it was when that lower savepoint opened too.
    ///
    /// @param level - the lowest level released
    pub(crate) fn close_epochs(&mut self, level: usize) {
        while self.epochs.len() > level {
            let Some(closed) = self.epochs.pop() else {
                break;
            };
            if let Some(below) = self.epochs.last_mut() {
                for (term, way_back) in closed.undo {
                    below.undo.entry(term).or_insert(way_back);
                }
            }
        }
    }

    /// Puts the buffer back to where it was when a savepoint opened.
    ///
    /// The savepoint stays open, empty, as a `ROLLBACK TO` leaves it. A buffer that holds no record
    /// of the savepoint is thrown away, which is what happened to every rollback before the buffer
    /// could be put back.
    ///
    /// @param level - the savepoint's level
    pub(crate) fn rollback_epochs(&mut self, level: usize) {
        if level >= self.epochs.len() {
            *self = Pending {
                layout: self.layout,
                layout_writer: std::mem::take(&mut self.layout_writer),
                ..Pending::default()
            };
            return;
        }
        let lowest_restore = self
            .epochs
            .get_mut(level)
            .and_then(|epoch| epoch.restore.take());
        let counters = self.epochs.get(level).map(|epoch| epoch.counters.clone());
        match lowest_restore {
            Some(snapshot) => self.doclists = snapshot.doclists,
            None => {
                for epoch in self.epochs.iter().skip(level).rev() {
                    replay_undo(&mut self.doclists, &epoch.undo);
                }
            }
        }
        self.epochs.truncate(level);
        if let Some(counters) = counters {
            self.set_counters(counters);
        }
        let counters = self.counters();
        self.epochs.push(Epoch {
            counters,
            ..Epoch::default()
        });
    }

    /// Works out the buffer as it was when each open savepoint opened, before a flush empties it.
    ///
    /// Done once per savepoint: a savepoint that already has its copy keeps it, and so does every
    /// savepoint below it. The records are dropped afterwards, because a flush leaves the buffer
    /// empty and they describe a buffer that no longer exists.
    pub(crate) fn keep_restores_before_a_flush(&mut self) {
        if self.epochs.is_empty() {
            return;
        }
        let mut state = Snapshot {
            doclists: self.doclists.clone(),
            counters: self.counters(),
        };
        for epoch in self.epochs.iter_mut().rev() {
            if epoch.restore.is_some() {
                break;
            }
            replay_undo(&mut state.doclists, &epoch.undo);
            state.counters = epoch.counters.clone();
            epoch.restore = Some(Box::new(state.clone()));
        }
        for epoch in self.epochs.iter_mut() {
            epoch.undo.clear();
        }
    }

    /// Records how to undo the next change to a term, the first time it changes in a savepoint.
    ///
    /// @param term - the term about to change
    /// @param appending - whether the change is an append to the end of the encoded list
    pub(crate) fn note_change(&mut self, term: &[u8], appending: bool) {
        let Some(epoch) = self.epochs.last_mut() else {
            return;
        };
        if epoch.restore.is_some() {
            return;
        }
        let current = self.doclists.get(term);
        match epoch.undo.get(term) {
            // An append after an append needs nothing more.
            Some(Undo::Appended { .. }) if appending => {}
            Some(Undo::Appended { len, last }) => {
                // Something other than an append is about to change a list that was appended to.
                // The list as it was is the list as it is, cut back.
                let before = current.map(|staged| {
                    let mut before = staged.clone();
                    before.truncate_appended(*len, *last);
                    before
                });
                if let Some(before) = before {
                    epoch.undo.insert(term.to_vec(), Undo::Whole(before));
                }
            }
            Some(_) => {}
            None => {
                let way_back = match current {
                    None => Undo::Absent,
                    Some(staged) if appending && staged.edited.is_none() => Undo::Appended {
                        len: staged.bytes.len(),
                        last: staged.last,
                    },
                    Some(staged) => Undo::Whole(staged.clone()),
                };
                epoch.undo.insert(term.to_vec(), way_back);
            }
        }
    }

    /// Empties the buffer, keeping what a savepoint needs to bring it back.
    pub(crate) fn discard_all(&mut self) {
        self.keep_restores_before_a_flush();
        self.doclists.clear();
        self.bytes = 0;
        self.resident = 0;
    }
}

/// Encodes entries held by rowid, which is the order a doclist keeps them in.
///
/// @param entries - the entries, by rowid
fn encode_entries(entries: &BTreeMap<i64, Vec<(usize, Vec<u32>)>>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut previous = 0i64;
    for (rowid, columns) in entries {
        write_varint(&mut out, rowid.wrapping_sub(previous) as u64);
        previous = *rowid;
        write_varint(&mut out, columns.len() as u64);
        for (column, positions) in columns {
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
/// A handle on the buffer, shared by a table and the cursors it opens.
pub type Buffer = Arc<Mutex<Pending>>;
/// How many bytes of doclists are held before the buffer is written out.
///
/// A bound rather than a tuning knob: without one, a bulk load of a large
/// corpus would hold the whole index in memory. Flushing early costs a rewrite
/// of what is held, which is what the unbuffered path paid per document.
pub(crate) const PENDING_BUDGET: usize = 8 * 1024 * 1024;
/// How many bytes of staged doclists, read or changed, are held before the buffer is written out.
pub(crate) const RESIDENT_BUDGET: usize = 512 * 1024 * 1024;
/// Returns the rowid an insert that named none is given.
///
/// The table is asked once per transaction and the mark rises from there.
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the staged rows
/// @param suffix - the shadow the rows are reached under
pub(crate) fn next_content_rowid(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
    suffix: &[u8],
) -> DbResult<i64> {
    if let Ok(mut held) = buffer.lock() {
        if let Some(highest) = held.content_highest {
            let next = highest.saturating_add(1);
            held.content_highest = Some(next);
            return Ok(next);
        }
    }
    let next = shadows.max_rowid(context, suffix)?.saturating_add(1);
    if let Ok(mut held) = buffer.lock() {
        held.content_highest = Some(next);
    }
    Ok(next)
}
/// Records that a rowid the caller named is now in the table.
///
/// @param buffer - the staged rows
/// @param rowid - the rowid that was written
pub(crate) fn note_content_rowid(buffer: &Buffer, rowid: i64) {
    if let Ok(mut held) = buffer.lock() {
        // **Only a mark that exists is raised.** With no mark, the next
        // allocation reads the highest rowid in the table, which includes this
        // one. Creating a mark from the named rowid alone made the next
        // allocated rowid one past it: after `INSERT ... (rowid, a) VALUES (0,
        // ...)` inside a transaction, the next insert that named no rowid was
        // given rowid 1 and replaced the row that was already there.
        if let Some(highest) = held.content_highest {
            held.content_highest = Some(highest.max(rowid));
        }
    }
}
/// Records that a row is gone, so the next rowid is worked out again if it was the highest.
///
/// SQLite gives a row inserted without a rowid one more than the highest rowid
/// the table holds when it is inserted. The mark above keeps the highest rowid
/// handed out this transaction, and when that row is deleted the highest rowid
/// is lower, so the mark is dropped and the next insert reads the table. Keeping
/// it gave the next row a rowid SQLite does not give it, and the two differ for
/// as long as the table is used.
///
/// @param buffer - the staged rows
/// @param rowid - the rowid that was deleted
pub(crate) fn forget_content_rowid(buffer: &Buffer, rowid: i64) {
    if let Ok(mut held) = buffer.lock() {
        if held.content_highest == Some(rowid) {
            held.content_highest = None;
        }
    }
}
/// What a `%_idx` row's third column holds.
///
/// Self-describing rather than versioned - see the module's own doc comment
/// for the argument. A row this build wrote, or has rewritten since a reopen,
/// carries its doclist inline; a row an older build wrote and this build has
/// not touched yet still names a `%_data` page.
pub(crate) enum TermValue {
    /// The doclist, exactly as `%_idx` holds it.
    Inline(Vec<u8>),
    /// The `%_data` row an older build left the doclist in.
    Page(i64),
}
/// Reads a `%_idx` row's third column, telling the two layouts apart by the
/// value's own type: an `Integer` is a page an older build wrote, a `Blob` or
/// `Text` is this build's doclist.
///
/// @param values - one `%_idx` row, as `scan_keyed` or `read_keyed` hands it
pub(crate) fn term_value(values: &[Value<'static>]) -> Option<TermValue> {
    match values.get(2) {
        Some(Value::Integer(page)) => Some(TermValue::Page(*page)),
        Some(Value::Blob(blob)) => Some(TermValue::Inline(blob.raw().to_vec())),
        Some(Value::Text(text)) => Some(TermValue::Inline(text.utf8_bytes().into_owned())),
        _ => None,
    }
}
/// Returns a `%_idx` row's doclist, refusing when the row does not resolve.
///
/// **A dictionary row whose doclist cannot be found is not a term with no
/// documents** (task-2053). [`resolve_doclist`] answers `None` for two states -
/// a third column that is neither a doclist nor a page number, and a page
/// number naming a `%_data` row that is not there - and both are damage rather
/// than an answer. Every caller but `integrity-check` was treating that `None`
/// as "this term matches nothing", which is the same silence the layout record
/// exists to end: a search over an index full of documents returned an empty
/// result set, and one path did worse and wrote the term's postings back as if
/// they had been empty.
///
/// `integrity-check` keeps the `Option`, because reporting the state is what it
/// is for.
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param term - the term, so the refusal can name it
/// @param values - the `%_idx` row
pub(crate) fn require_doclist(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    term: &[u8],
    values: &[Value<'static>],
) -> DbResult<Vec<u8>> {
    match resolve_doclist(context, shadows, values)? {
        Some(bytes) => Ok(bytes),
        None => Err(inillucent_base::error::corrupt(format!(
            "the full-text term {} has a dictionary row whose doclist cannot be read",
            String::from_utf8_lossy(term)
        ))
        .with_message(
            "the full-text index has a dictionary row whose doclist cannot be read".to_string(),
        )),
    }
}
/// Returns a `%_idx` row's doclist, wherever it actually lives.
///
/// One read for a row this build wrote; one read plus the `%_data` row an
/// older build's indirection still names, for a row it has not touched yet.
///
/// `None` is damage, not an answer; [`require_doclist`] is what every caller
/// but `integrity-check` uses, and its doc comment says why.
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param values - the `%_idx` row
pub(crate) fn resolve_doclist(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    values: &[Value<'static>],
) -> DbResult<Option<Vec<u8>>> {
    match term_value(values) {
        Some(TermValue::Inline(bytes)) => Ok(Some(bytes)),
        Some(TermValue::Page(page)) => {
            Ok(shadows.read_row(context, b"data", page)?.and_then(|row| {
                row.get(1)
                    .and_then(Value::as_blob)
                    .map(|blob| blob.raw().to_vec())
            }))
        }
        None => Ok(None),
    }
}
/// Returns a term's doclist, from the buffer when this transaction has
/// staged it, or read from `%_idx` otherwise.
///
/// **A read-only accessor.** Unlike [`term_row`], it never stages what it
/// reads - the query path this serves does not necessarily hold a write
/// transaction to stage into, and does not need to: it asks for a term once
/// per query rather than once per document.
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the staged doclists
/// @param term - the term
pub(crate) fn read_doclist(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
    term: &[u8],
) -> DbResult<Option<Vec<u8>>> {
    if let Ok(held) = buffer.lock() {
        if let Some(staged) = held.doclists.get(term) {
            return Ok(Some(staged.encoded().into_owned()));
        }
    }
    let key = [Value::Integer(SEGMENT), Value::owned_blob(term)?];
    match shadows.read_keyed(context, b"idx", &key, 3)? {
        Some(row) => require_doclist(context, shadows, term, &row).map(Some),
        // The one legitimate `None`: the dictionary holds no row for this term,
        // so the term really is in no documents.
        None => Ok(None),
    }
}
/// What [`append_in_one_lock`] managed.
pub(crate) enum Appended {
    /// The entry is in the buffer, and whether the buffer is now over budget.
    Done {
        /// Whether the caller should flush.
        full: bool,
    },
    /// Nothing was written; the caller takes the general path.
    No,
}
/// Appends one document's occurrences of one term, under a single lock.
///
/// **The whole of the bulk-load path, in one critical section.** It answers
/// [`Appended::No`] and writes nothing whenever any of its three conditions
/// does not hold - the term has not been looked up in this transaction, its
/// doclist is not staged, or the document does not sort after everything
/// already in it - and the caller then does the general thing, which is the
/// same code that ran before this existed.
///
/// The postings are written straight out of the sorted run, so nothing is
/// allocated to describe them: they arrive sorted by column and then by
/// position, which is exactly the order a doclist entry holds.
///
/// @param buffer - the staged doclists
/// @param term - the term
/// @param rowid - the document
/// @param run - this term's postings in this document
pub(crate) fn append_in_one_lock(
    buffer: &Buffer,
    term: &[u8],
    rowid: i64,
    run: &[(Vec<u8>, usize, u32)],
) -> Appended {
    let Ok(mut held) = buffer.lock() else {
        return Appended::No;
    };
    let appending = match held.doclists.get(term) {
        Some(staged) => staged.appends(rowid),
        None => return Appended::No,
    };
    held.note_change(term, appending);
    let Some(staged) = held.doclists.get_mut(term) else {
        return Appended::No;
    };
    let was = staged.size();
    if !staged.put_row(rowid, || columns_of(run), Some(run)) {
        return Appended::No;
    }
    let grew = staged.size().saturating_sub(was);
    held.bytes = held.bytes.saturating_add(grew);
    held.resident = held.resident.saturating_add(grew);
    Appended::Done {
        full: is_over_budget(&held),
    }
}

/// Reports whether the buffer holds more than it should.
///
/// Two bounds. The changed bytes since the last flush are the budget a flush is for. The bytes of
/// every staged list, changed or only read, bound memory, and are much larger because a hot term of
/// a big index is a list as long as the index has rows.
///
/// @param held - the buffer
fn is_over_budget(held: &Pending) -> bool {
    held.bytes > PENDING_BUDGET || held.resident > RESIDENT_BUDGET
}
/// Writes one doclist entry straight out of a sorted run of postings.
///
/// The bytes are the ones [`append_doclist_entry`] writes for the same
/// occurrences - the two are checked against each other by
/// `a_run_appends_the_bytes_the_entry_would`, because a doclist written two
/// ways is a format with two definitions.
///
/// @param out - the doclist being appended to
/// @param delta - this document's rowid minus the previous entry's
/// @param run - the postings, sorted by column then position
pub(crate) fn append_run(out: &mut Vec<u8>, delta: i64, run: &[(Vec<u8>, usize, u32)]) {
    write_varint(out, delta as u64);
    // How many distinct columns the run touches, counted without a collection:
    // it is sorted, so a column starts wherever it differs from the one before.
    let mut columns = 0u64;
    let mut previous: Option<usize> = None;
    for (_, column, _) in run {
        if previous != Some(*column) {
            columns = columns.saturating_add(1);
            previous = Some(*column);
        }
    }
    write_varint(out, columns);
    let mut at = 0usize;
    while at < run.len() {
        let Some((_, column, _)) = run.get(at) else {
            break;
        };
        let start = at;
        while matches!(run.get(at), Some((_, held, _)) if held == column) {
            at = at.saturating_add(1);
        }
        write_varint(out, *column as u64);
        write_varint(out, at.saturating_sub(start) as u64);
        let mut last = 0u32;
        for (_, _, position) in run.get(start..at).unwrap_or_default() {
            write_varint(out, u64::from(position.wrapping_sub(last)));
            last = *position;
        }
    }
}
/// Groups a sorted run of postings into the per-column form a `DocEntry` holds.
///
/// Only for the paths that need a whole entry - the first document to hold a
/// term, and one that does not sort last - so the allocation it costs is off
/// the bulk-load path.
///
/// @param run - the postings, sorted by column then position
pub(crate) fn columns_of(run: &[(Vec<u8>, usize, u32)]) -> Vec<(usize, Vec<u32>)> {
    let mut columns: Vec<(usize, Vec<u32>)> = Vec::new();
    for (_, column, position) in run {
        match columns.last_mut() {
            Some((held, positions)) if held == column => positions.push(*position),
            _ => columns.push((*column, vec![*position])),
        }
    }
    columns
}
/// Appends one entry to a staged doclist without copying it.
///
/// **The buffer is written into, not read out of and put back.** Reading a
/// staged doclist hands back a copy, and appending through that copy moves the
/// whole doclist per occurrence - which is the same quadratic the buffer was
/// added to remove, relocated from the file into memory. A term in five hundred
/// documents grew a two-kilobyte list, so the copies were half a megabyte per
/// term.
///
/// Answers false when the term is not staged, or when the entry does not sort
/// after everything already there; the caller then takes the general path.
///
/// @param buffer - the staged doclists
/// @param term - the term whose doclist this is
/// @param entry - the entry to append
pub(crate) fn append_staged(buffer: &Buffer, term: &[u8], entry: &DocEntry) -> bool {
    let Ok(mut held) = buffer.lock() else {
        return false;
    };
    let appending = match held.doclists.get(term) {
        Some(staged) => staged.appends(entry.rowid),
        None => return false,
    };
    held.note_change(term, appending);
    let Some(staged) = held.doclists.get_mut(term) else {
        return false;
    };
    // The remembered end, rather than a walk to find it. See `Staged`.
    let was = staged.size();
    if !staged.put_row(entry.rowid, || entry.columns.clone(), None) {
        return false;
    }
    let grew = staged.size().saturating_sub(was);
    held.bytes = held.bytes.saturating_add(grew);
    held.resident = held.resident.saturating_add(grew);
    true
}
/// Stages a doclist to be written when the buffer is next flushed.
///
/// @param buffer - the staged doclists
/// @param term - the term it belongs to
/// @param bytes - the whole doclist
/// @param changed - whether the list differs from `%_idx`, and so has to be written
pub(crate) fn stage_doclist(buffer: &Buffer, term: &[u8], bytes: Vec<u8>, changed: bool) {
    let mut staged = Staged::from_bytes(bytes);
    staged.clean = !changed;
    if let Ok(mut held) = buffer.lock() {
        held.note_change(term, false);
        let prior = held
            .doclists
            .get(term)
            .map(|held| (held.size(), held.is_clean()));
        let (was, was_clean) = prior.unwrap_or((0, true));
        held.resident = held
            .resident
            .saturating_sub(was)
            .saturating_add(staged.size());
        if !was_clean {
            held.bytes = held.bytes.saturating_sub(was);
        }
        if changed {
            held.bytes = held.bytes.saturating_add(staged.size());
        }
        held.doclists.insert(term.to_vec(), staged);
    }
}
/// Returns every staged doclist whose term starts with a prefix.
///
/// A prefix query reads the terms `%_idx` holds that start with the prefix, and
/// a term this transaction changed is only in the buffer or is newer there than
/// in `%_idx`. The doclists come back encoded, by term.
///
/// @param buffer - the staged doclists
/// @param prefix - the start of the terms wanted
pub(crate) fn staged_with_prefix(buffer: &Buffer, prefix: &[u8]) -> BTreeMap<Vec<u8>, Vec<u8>> {
    let mut found = BTreeMap::new();
    let Ok(held) = buffer.lock() else {
        return found;
    };
    for (term, staged) in held.doclists.range(prefix.to_vec()..) {
        if !term.starts_with(prefix) {
            break;
        }
        found.insert(term.clone(), staged.encoded().into_owned());
    }
    found
}
/// What taking a row out of one term's doclist left.
pub(crate) enum TermRemoval {
    /// The term has no doclist, staged or stored.
    Missing,
    /// The doclist still has other rows, and is staged.
    Kept,
    /// The row was the last one in the doclist, which is gone from the buffer.
    /// The `%_idx` row and any older page it names are for the caller to delete.
    Emptied {
        /// The `%_data` page an older build left the doclist in, when this
        /// call is the one that read it.
        legacy_page: Option<i64>,
    },
}
/// Takes one row out of one term's doclist, staging the result.
///
/// **The staged doclist is edited in place.** Reading the stored row, decoding
/// it, dropping an entry and encoding it again for every row of a multi row
/// `UPDATE` made the statement quadratic; see [`Staged`].
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the staged doclists
/// @param term - the term
/// @param rowid - the row to take out
pub(crate) fn remove_row_from_term(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
    term: &[u8],
    rowid: i64,
) -> DbResult<TermRemoval> {
    // The buffer first: a term this transaction has already staged has no
    // stored `%_idx` row to read, whether it is brand new or already rewritten
    // in the inline form.
    if let Ok(mut held) = buffer.lock() {
        if held.doclists.contains_key(term) {
            held.note_change(term, false);
        }
        let state = match held.doclists.get_mut(term) {
            Some(staged) => {
                let was = staged.size();
                staged.remove_row(rowid);
                Some((was, staged.size(), staged.is_empty()))
            }
            None => None,
        };
        if let Some((was, now, emptied)) = state {
            held.bytes = held.bytes.saturating_sub(was.saturating_sub(now));
            held.resident = held.resident.saturating_sub(was.saturating_sub(now));
            if emptied {
                held.doclists.remove(term);
                held.resident = held.resident.saturating_sub(now);
                return Ok(TermRemoval::Emptied { legacy_page: None });
            }
            return Ok(TermRemoval::Kept);
        }
    }
    let key = [Value::Integer(SEGMENT), Value::owned_blob(term)?];
    let Some(row) = shadows.read_keyed(context, b"idx", &key, 3)? else {
        return Ok(TermRemoval::Missing);
    };
    // The `%_data` page a legacy row still names, freed by the caller when
    // this pass is the one that reads it.
    let legacy_page = match term_value(&row) {
        Some(TermValue::Page(page)) => Some(page),
        _ => None,
    };
    let mut staged = Staged::from_bytes(require_doclist(context, shadows, term, &row)?);
    staged.remove_row(rowid);
    if staged.is_empty() {
        return Ok(TermRemoval::Emptied { legacy_page });
    }
    if let Ok(mut held) = buffer.lock() {
        held.note_change(term, false);
        held.bytes = held.bytes.saturating_add(staged.size());
        held.resident = held.resident.saturating_add(staged.size());
        held.doclists.insert(term.to_vec(), staged);
    }
    Ok(TermRemoval::Kept)
}
/// Reports whether the buffer is holding more than it should.
///
/// @param buffer - the staged doclists
pub(crate) fn buffer_is_full(buffer: &Buffer) -> bool {
    buffer
        .lock()
        .map(|held| is_over_budget(&held))
        .unwrap_or(false)
}
/// Writes every staged doclist and empties the buffer.
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the staged doclists
pub(crate) fn flush_doclists(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
) -> DbResult<()> {
    let started = std::time::Instant::now();
    let answer = flush_doclists_timed(context, shadows, buffer);
    let spent = started.elapsed().as_nanos();
    record_stage(|stages| stages.flush = stages.flush.saturating_add(spent));
    answer
}
/// Writes every staged doclist and empties the buffer, without the timing.
///
/// **One write per term, not two.** Every entry the buffer holds is a term's
/// whole `%_idx` row - the dictionary key and the doclist together - so
/// writing it out in key order is the same walk to the right of the tree this
/// used to spend on the dictionary alone, and there is no second pass over a
/// separate `%_data` table behind it.
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the staged doclists
pub(crate) fn flush_doclists_timed(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
) -> DbResult<()> {
    // **In term order.** `%_idx` is keyed by the term, so taking the
    // `BTreeMap` in its own order writes the run as a walk to the right of the
    // tree rather than a descent per term into a growing one.
    let staged: Vec<(Vec<u8>, Staged)> = match buffer.lock() {
        Ok(mut held) => {
            // A savepoint that is open when the buffer is written out can no longer be put back
            // by replaying records, so what it needs is kept first. See `Epoch`.
            held.keep_restores_before_a_flush();
            held.bytes = 0;
            held.resident = 0;
            core::mem::take(&mut held.doclists).into_iter().collect()
        }
        Err(_) => return Ok(()),
    };
    let started = std::time::Instant::now();
    for (term, doclist) in staged {
        // A list that was only read is what `%_idx` already holds.
        if doclist.is_clean() {
            continue;
        }
        let bytes = doclist.into_bytes();
        let written = bytes.len() as u64;
        shadows.write_keyed(
            context,
            b"idx",
            2,
            &[
                Value::Integer(SEGMENT),
                Value::owned_blob(&term)?,
                Value::owned_blob(&bytes)?,
            ],
        )?;
        record_stage(|stages| stages.flushed_bytes = stages.flushed_bytes.saturating_add(written));
    }
    let spent = started.elapsed().as_nanos();
    record_stage(|stages| stages.dictionary_write = stages.dictionary_write.saturating_add(spent));
    // The totals go out with them, once, rather than once per document. The
    // buffered copy is kept: a reader inside the same transaction has to see
    // what the transaction wrote, which is what makes this a buffer.
    let totals = match buffer.lock() {
        Ok(mut held) if held.totals_dirty => {
            held.totals_dirty = false;
            held.totals.clone()
        }
        _ => None,
    };
    if let Some(totals) = totals {
        put_totals(context, shadows, &totals)?;
    }
    Ok(())
}
/// Resolves a term's dictionary entry, staging its doclist for reuse.
///
/// **One table now, so finding a term and reading its doclist are the same
/// row.** They used to be two: `%_idx` named a `%_data` page and a second
/// descent read it, so every term this transaction had not already staged
/// cost two tree reads whether or not the caller was about to change it. The
/// row `%_idx` returns now carries the doclist already - inline if this build
/// wrote it, or through the `%_data` indirection an older build left, which
/// [`resolve_doclist`] follows - so it is staged here the moment it is found
/// and a caller never reads it twice.
///
/// @param context - the host
/// @param shadows - the table's shadow tables
/// @param buffer - the staged doclists
/// @param term - the term
/// @param create - whether a term with no row yet is reported as fresh rather
///   than absent
pub(crate) fn term_row(
    context: &mut Context<'_>,
    shadows: &ShadowTables,
    buffer: &Buffer,
    term: &[u8],
    create: bool,
) -> DbResult<Option<Term>> {
    if let Ok(held) = buffer.lock() {
        if held.doclists.contains_key(term) {
            return Ok(Some(Term { fresh: false }));
        }
    }
    let probed = std::time::Instant::now();
    let key = [Value::Integer(SEGMENT), Value::owned_blob(term)?];
    let found = shadows.read_keyed(context, b"idx", &key, 3)?;
    let probe_ns = probed.elapsed().as_nanos();
    record_stage(|stages| stages.dictionary_read = stages.dictionary_read.saturating_add(probe_ns));
    let Some(row) = found else {
        return Ok(create.then_some(Term { fresh: true }));
    };
    // **Not `unwrap_or_default()`** (task-2053). A row whose doclist does not
    // resolve was staged here as an empty one, and the flush then wrote that
    // empty doclist back over the term's row - so a term whose postings could
    // not be read lost the postings as well as the answer.
    let bytes = require_doclist(context, shadows, term, &row)?;
    stage_doclist(buffer, term, bytes, false);
    Ok(Some(Term { fresh: false }))
}
/// Whether a term's dictionary row was found, and staged, by [`term_row`].
///
/// **`fresh` is what lets the caller skip a decode it knows will miss.** A
/// term with no row yet has no doclist to merge into, so building its first
/// entry from scratch is the whole job; one `term_row` found has already been
/// staged, and the caller's own retry of the fast append path is what uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Term {
    /// Whether the row was created by the call that returned this, rather
    /// than found on disk or already staged.
    pub(crate) fresh: bool,
}
/// Returns the error a command FTS5 does not know reports.
///
/// SQLite says nothing more than `SQL logic error` here - there is no message
/// naming the command - so neither does this.
pub(crate) fn unrecognized() -> inillucent_base::DbError {
    inillucent_base::DbError::primary(inillucent_base::PrimaryCode::Error)
}
/// Reads a `%_docsize` blob back into one count per column.
pub fn decode_sizes(bytes: &[u8], columns: usize) -> Vec<i64> {
    let mut at = 0usize;
    (0..columns)
        .map(|_| read_varint(bytes, &mut at) as i64)
        .collect()
}
/// Returns a value as the text the tokenizer reads.
pub(crate) fn text_of(value: &Value<'static>) -> Option<Vec<u8>> {
    match value {
        Value::Text(text) => Some(text.utf8_bytes().into_owned()),
        Value::Blob(blob) => Some(blob.raw().to_vec()),
        Value::Integer(number) => Some(number.to_string().into_bytes()),
        Value::Real(number) => Some(inillucent_value::numeric::real_to_text(*number)),
        Value::Null => None,
    }
}

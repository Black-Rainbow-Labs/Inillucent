//! Collecting the rows a write reads before it writes any: in memory while they fit, in a transient
//! file when they do not.
//!
//! Invariant: **the rows come back in the order they were collected, value for value, and the
//! memory a collection holds does not grow with the number of rows once it passes
//! [`GATHER_BYTES`].** An `INSERT ... SELECT` reads its whole source before it writes a row, so
//! that no row of the source can see a row the statement wrote. That is correct and it is also
//! what held 190 bytes a row of a 5,000,000 row copy (988 MB) in memory. The read still finishes
//! before the first write. What changes is where the rows wait: past the byte limit they are
//! appended to a transient file as they arrive and read back in order, a few at a time, by the
//! insert.
//!
//! ## Why the statement does not change shape
//!
//! The statement is still one statement. It reads everything, then writes everything inside one
//! write view, and a failure on any row puts back everything the statement wrote, exactly as it did
//! when the rows were a vector. Only the container of the source rows differs, so which row fails,
//! with which message and which constraint, is decided by the same code on the same rows in the
//! same order.
//!
//! ## What stays in memory
//!
//! A collection that never passes the limit is the vector or the packed set it always was. A
//! statement of a few thousand rows pays nothing for this module except one comparison per batch.

use inillucent_base::DbResult;
use inillucent_tree::datum::{Datum, OwnedDatum};

use crate::batch::Batch;
use crate::ops::{resident_row_bytes, Flow};
use crate::packed::PackedRows;
use crate::spill::{Run, RunReader, Spill, SpillFile};

/// How many bytes of source rows a write holds before it moves them to a transient file.
///
/// 32 MiB, which is a few hundred thousand rows of the sizes an `INSERT ... SELECT` copies. The
/// bulk build, which needs every row at once to sort them and check each rowid, keeps its own
/// buffers beside this one, so a copy that passes the limit takes the ordinary insert, which needs
/// no more than a handful of rows at a time.
pub const GATHER_BYTES: u64 = 32 * 1024 * 1024;

/// How many bytes of encoded rows the spool buffers before it appends them to its file.
const SPOOL_BUFFER: usize = 1 << 20;

/// How many collections have moved their rows to a file, in this process.
///
/// **So a test can say that a statement's source reached the file.** A case that lowers the limit
/// and then copies fewer rows than the limit grades the in memory path and passes.
static SPOOLED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Returns how many collections have moved their rows to a file since the process started.
pub fn spooled_collections() -> u64 {
    SPOOLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// How many copies have been built by the streaming bulk build, in this process.
static STREAMED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Records that a copy was built by the streaming bulk build.
pub fn note_streamed_build() {
    STREAMED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Returns how many copies the streaming bulk build has built since the process started, so a test
/// can say that a copy took that path.
pub fn streamed_builds() -> u64 {
    STREAMED.load(std::sync::atomic::Ordering::Relaxed)
}

/// What a collection holds once it is complete.
pub enum Gathered {
    /// One vector a row.
    Rows(Vec<Vec<OwnedDatum>>),
    /// Every row in a few large buffers; see [`PackedRows`].
    Packed(PackedRows),
    /// Rows in a transient file, read back in order.
    Spooled(SpooledRows),
}

impl Gathered {
    /// Returns how many rows there are.
    pub fn len(&self) -> u64 {
        match self {
            Gathered::Rows(rows) => rows.len() as u64,
            Gathered::Packed(packed) => packed.len() as u64,
            Gathered::Spooled(spooled) => spooled.len(),
        }
    }

    /// Reports whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns every row as a vector each, reading a spooled set back in full.
    ///
    /// For a caller that cannot take rows a few at a time. It holds every row again, so it is the
    /// fallback and not the path a large statement takes.
    pub fn into_rows(self) -> DbResult<Vec<Vec<OwnedDatum>>> {
        match self {
            Gathered::Rows(rows) => Ok(rows),
            Gathered::Packed(packed) => Ok(packed.into_rows()),
            Gathered::Spooled(spooled) => spooled.collect(),
        }
    }
}

/// Where a collection holds its rows now.
enum Store {
    /// One vector a row, with the bytes they occupy as held.
    Rows {
        /// The rows.
        rows: Vec<Vec<OwnedDatum>>,
        /// What they occupy, counted as [`resident_row_bytes`] does.
        held: u64,
    },
    /// Packed.
    Packed(PackedRows),
    /// Appended to a file.
    Spool(RowSpool),
}

/// Rows being collected by a pipeline's last operator.
pub struct Gathering {
    /// Where the rows are.
    store: Store,
    /// Where a file comes from, when the connection can offer one.
    spill: Option<std::rc::Rc<dyn Spill>>,
    /// How many bytes the rows may occupy before they move to a file.
    limit: u64,
}

impl Gathering {
    /// Returns a collection that keeps one vector a row until it has to move to a file.
    ///
    /// @param spill - where a file comes from, or none to hold every row
    pub fn rows(spill: Option<std::rc::Rc<dyn Spill>>) -> Gathering {
        Gathering {
            store: Store::Rows {
                rows: Vec::new(),
                held: 0,
            },
            spill,
            limit: crate::spill::run_bytes(GATHER_BYTES),
        }
    }

    /// Returns a collection that packs its rows until it has to move to a file.
    ///
    /// @param spill - where a file comes from, or none to hold every row
    pub fn packed(spill: Option<std::rc::Rc<dyn Spill>>) -> Gathering {
        Gathering {
            store: Store::Packed(PackedRows::new()),
            spill,
            limit: crate::spill::run_bytes(GATHER_BYTES),
        }
    }

    /// Returns how many rows have been collected.
    pub fn len(&self) -> usize {
        match &self.store {
            Store::Rows { rows, .. } => rows.len(),
            Store::Packed(packed) => packed.len(),
            Store::Spool(spool) => usize::try_from(spool.rows).unwrap_or(usize::MAX),
        }
    }

    /// Reports whether no row has been collected.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Appends a batch's live rows, stopping at `limit` rows.
    ///
    /// @param batch - the rows
    /// @param limit - how many rows to keep in all, or `None` for every row
    pub fn push_batch(&mut self, batch: &Batch<'_>, limit: Option<usize>) -> DbResult<Flow> {
        let width = batch.columns.len();
        let mut row: Vec<Datum<'_>> = Vec::with_capacity(width);
        for nth in 0..batch.live() {
            if limit.is_some_and(|limit| self.len() >= limit) {
                return Ok(Flow::Stop);
            }
            row.clear();
            for column in 0..width {
                row.push(batch.value(nth, column)?);
            }
            match &mut self.store {
                Store::Rows { rows, held } => {
                    let owned: Vec<OwnedDatum> = row.iter().map(OwnedDatum::from_datum).collect();
                    *held = held.saturating_add(resident_row_bytes(&owned));
                    rows.push(owned);
                }
                Store::Packed(packed) => {
                    if !packed.push(row.iter().copied()) {
                        return Err(inillucent_base::error::misuse(
                            "a row collected for an insert is not as wide as the rows before it",
                        ));
                    }
                }
                Store::Spool(spool) => spool.push(&row)?,
            }
        }
        self.move_to_a_file_when_full()?;
        match limit {
            Some(limit) if self.len() >= limit => Ok(Flow::Stop),
            _ => Ok(Flow::Continue),
        }
    }

    /// Moves what is held to a file when it has passed the limit and a file is available.
    fn move_to_a_file_when_full(&mut self) -> DbResult<()> {
        let held = match &self.store {
            Store::Rows { held, .. } => *held,
            Store::Packed(packed) => packed.resident_bytes(),
            Store::Spool(_) => return Ok(()),
        };
        let Some(spill) = self.spill.clone() else {
            return Ok(());
        };
        if held < self.limit {
            return Ok(());
        }
        let mut spool = RowSpool::new(spill);
        match std::mem::replace(&mut self.store, Store::Packed(PackedRows::new())) {
            Store::Rows { rows, .. } => {
                for row in &rows {
                    let values: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
                    spool.push(&values)?;
                }
            }
            Store::Packed(packed) => {
                for row in 0..packed.len() {
                    let values: Vec<Datum<'_>> = (0..packed.width())
                        .map(|column| packed.value(row, column))
                        .collect();
                    spool.push(&values)?;
                }
            }
            Store::Spool(held) => spool = held,
        }
        self.store = Store::Spool(spool);
        SPOOLED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Ends the collection.
    pub fn finish(self) -> DbResult<Gathered> {
        Ok(match self.store {
            Store::Rows { rows, .. } => Gathered::Rows(rows),
            Store::Packed(packed) => Gathered::Packed(packed),
            Store::Spool(spool) => Gathered::Spooled(spool.finish()?),
        })
    }
}

/// Rows appended to a transient file, in the order they arrive.
struct RowSpool {
    /// Where the file comes from.
    spill: std::rc::Rc<dyn Spill>,
    /// The file, opened by the first append.
    file: Option<Box<dyn SpillFile>>,
    /// Rows encoded and not yet appended.
    buffer: Vec<u8>,
    /// How many bytes the file holds.
    written: u64,
    /// How many rows have been pushed.
    rows: u64,
}

impl RowSpool {
    /// Returns an empty spool.
    ///
    /// @param spill - where the file comes from
    fn new(spill: std::rc::Rc<dyn Spill>) -> RowSpool {
        RowSpool {
            spill,
            file: None,
            buffer: Vec::with_capacity(SPOOL_BUFFER.saturating_add(4096)),
            written: 0,
            rows: 0,
        }
    }

    /// Appends one row.
    ///
    /// The encoding is the one [`crate::spill::encode_run`] writes, so [`RunReader`] reads it back.
    ///
    /// @param values - the row's values
    fn push(&mut self, values: &[Datum<'_>]) -> DbResult<()> {
        let width = u32::try_from(values.len()).unwrap_or(u32::MAX);
        self.buffer.extend_from_slice(&width.to_le_bytes());
        for value in values {
            value.encode_tagged(&mut self.buffer);
        }
        self.rows = self.rows.saturating_add(1);
        if self.buffer.len() >= SPOOL_BUFFER {
            self.flush()?;
        }
        Ok(())
    }

    /// Appends the buffered rows to the file.
    fn flush(&mut self) -> DbResult<()> {
        if self.buffer.is_empty() {
            return Ok(());
        }
        if self.file.is_none() {
            self.file = Some(self.spill.open()?);
        }
        if let Some(file) = self.file.as_mut() {
            file.append(&self.buffer)?;
            self.written = self.written.saturating_add(self.buffer.len() as u64);
        }
        self.buffer.clear();
        Ok(())
    }

    /// Ends the spool and returns the rows to be read back.
    fn finish(mut self) -> DbResult<SpooledRows> {
        self.flush()?;
        let rows = self.rows;
        let run = Run {
            at: 0,
            bytes: self.written,
            rows: usize::try_from(rows).unwrap_or(usize::MAX),
        };
        Ok(SpooledRows {
            reader: RunReader::new(run),
            run,
            file: self.file,
            rows,
        })
    }
}

/// Rows read back, in order, from a transient file.
///
/// An iterator, because the insert takes one row at a time and never needs two at once.
pub struct SpooledRows {
    /// Reads the file a window at a time.
    reader: RunReader,
    /// Where the rows are in the file.
    run: Run,
    /// The file, or none when no row was written.
    file: Option<Box<dyn SpillFile>>,
    /// How many rows the file holds.
    rows: u64,
}

impl SpooledRows {
    /// Returns how many rows the file holds in all.
    pub fn len(&self) -> u64 {
        self.rows
    }

    /// Reports whether the file holds no row.
    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }

    /// Starts reading again from the first row.
    ///
    /// For a statement that reads its source twice: once to check every row, and once to write
    /// them.
    pub fn rewind(&mut self) {
        self.reader = RunReader::new(self.run);
    }
}

impl Iterator for SpooledRows {
    type Item = DbResult<Vec<OwnedDatum>>;

    fn next(&mut self) -> Option<Self::Item> {
        let file = self.file.as_deref()?;
        match self.reader.next(file) {
            Ok(Some(row)) => Some(Ok(row)),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch::Vector;

    /// A spill file held in memory, for the cases below.
    #[derive(Default)]
    struct InMemory {
        bytes: std::cell::RefCell<Vec<u8>>,
    }

    impl SpillFile for InMemory {
        fn append(&mut self, bytes: &[u8]) -> DbResult<u64> {
            let mut held = self.bytes.borrow_mut();
            let at = held.len() as u64;
            held.extend_from_slice(bytes);
            Ok(at)
        }

        fn read_at(&self, offset: u64, out: &mut [u8]) -> DbResult<()> {
            let held = self.bytes.borrow();
            let from = usize::try_from(offset).unwrap_or(usize::MAX);
            let slice = held
                .get(from..from.saturating_add(out.len()))
                .ok_or_else(|| inillucent_base::error::misuse("a spill read ran past the file"))?;
            out.copy_from_slice(slice);
            Ok(())
        }
    }

    /// Opens a fresh in memory file each time.
    struct InMemorySpill;

    impl Spill for InMemorySpill {
        fn open(&self) -> DbResult<Box<dyn SpillFile>> {
            Ok(Box::new(InMemory::default()))
        }
    }

    /// Pushes rows of an integer and a text through a collection, a batch at a time.
    ///
    /// @param gathering - the collection
    /// @param from - the first number
    /// @param count - how many rows
    fn push_rows(gathering: &mut Gathering, from: i64, count: i64) {
        let numbers: Vec<Datum<'_>> = (from..from + count).map(Datum::Int).collect();
        let texts: Vec<String> = (from..from + count).map(|n| format!("text {n}")).collect();
        let text_values: Vec<Datum<'_>> = texts.iter().map(|t| Datum::Text(t.as_bytes())).collect();
        let batch = Batch::new(
            count as usize,
            vec![Vector::Values(&numbers), Vector::Values(&text_values)],
        );
        gathering
            .push_batch(&batch, None)
            .expect("the batch is kept");
    }

    /// **Rows collected past the limit come back in order and equal to the rows that went in.**
    ///
    /// Both stores are checked, because each moves its rows to the file by its own code.
    #[test]
    fn rows_past_the_limit_are_read_back_in_order() {
        for packed in [false, true] {
            let spill: Option<std::rc::Rc<dyn Spill>> = Some(std::rc::Rc::new(InMemorySpill));
            let mut gathering = if packed {
                Gathering::packed(spill)
            } else {
                Gathering::rows(spill)
            };
            gathering.limit = 4096;
            for start in (0..3000i64).step_by(100) {
                push_rows(&mut gathering, start, 100);
            }
            assert_eq!(gathering.len(), 3000);
            let gathered = gathering.finish().expect("the collection ends");
            let Gathered::Spooled(spooled) = gathered else {
                panic!("the collection did not move to a file (packed: {packed})");
            };
            let rows: Vec<Vec<OwnedDatum>> = spooled.map(|row| row.expect("a row")).collect();
            assert_eq!(rows.len(), 3000);
            for (nth, row) in rows.iter().enumerate() {
                let n = nth as i64;
                assert_eq!(
                    row,
                    &vec![
                        OwnedDatum::Int(n),
                        OwnedDatum::Text(format!("text {n}").into_bytes())
                    ]
                );
            }
        }
    }

    /// A collection under its limit is the vector or the packed set it always was.
    #[test]
    fn rows_under_the_limit_stay_in_memory() {
        let spill: Option<std::rc::Rc<dyn Spill>> = Some(std::rc::Rc::new(InMemorySpill));
        let mut gathering = Gathering::rows(spill);
        push_rows(&mut gathering, 0, 50);
        assert!(matches!(gathering.finish(), Ok(Gathered::Rows(rows)) if rows.len() == 50));
    }

    /// With no file to move to, a collection holds every row whatever its size.
    #[test]
    fn a_collection_with_nowhere_to_spill_holds_every_row() {
        let mut gathering = Gathering::rows(None);
        gathering.limit = 1;
        push_rows(&mut gathering, 0, 500);
        assert!(matches!(gathering.finish(), Ok(Gathered::Rows(rows)) if rows.len() == 500));
    }
}

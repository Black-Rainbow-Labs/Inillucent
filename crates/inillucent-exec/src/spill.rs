//! Where a buffering operator puts rows it cannot hold.
//!
//! Invariant: **a spill changes when a row is compared, never how.** The rows
//! that come out of an external sort are the rows a sort in memory would have
//! emitted, in the same order, because both use `compare_by` on the same
//! `OwnedDatum`s. What the spill changes is that the whole set is never
//! resident at once.
//!
//! ## Why the trait is here and the implementation is not
//!
//! `Sort` lives in this crate, and `docs/invariants/layering.toml` does not let
//! this crate depend on `inillucent-vfs` - a query executor has no business
//! naming a file system. `inillucent-engine` may, and already depends on this
//! crate, so the dependency points the right way when the trait is declared
//! here and implemented there (task-2066 §4.3.6).
//!
//! `TreeCatalog::spill` answers `None` by default. An operator with no spill
//! file behaves exactly as it did before this existed: it buffers everything
//! and the byte budget refuses it if a budget was armed.
//!
//! ## What a run is
//!
//! A run is rows in sorted order, each one a `u32` count of values followed by
//! that many tagged values - `Datum::encode_tagged`, which is the same
//! self-describing form the delta area and the exception heap store. Reading a
//! run back is `Datum::decode_tagged` in a loop.
//!
//! There is no header and no checksum. A run is written and read by one
//! statement inside one process, it never outlives the operator, and the file
//! is opened with `delete_on_close`: a format that survived a crash would be a
//! format somebody has to keep compatible.

use inillucent_base::{DbResult, PrimaryCode};
use inillucent_tree::datum::{Datum, OwnedDatum};

use crate::ops::resident_row_bytes;

/// How many bytes of rows an operator holds before it writes a run, when a test has not changed it.
///
/// A test moves it through [`set_run_bytes_override`] so that a few thousand rows exercise the code
/// that writes and merges runs. The shipped figures are far above any workload a unit test builds,
/// and code that no test reaches is code whose first run is a customer's.
static RUN_BYTES_OVERRIDE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many rows a statement moves at a time through a bounded buffer, when a test has not changed
/// it.
static CHUNK_ROWS_OVERRIDE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many runs every [`RunWriter`] in the process has written.
///
/// **So a test can say that a statement spilled.** A case that lowers the run size and then sorts
/// fewer rows than the lowered size compares an in memory sort with an in memory sort and passes,
/// which is a test that cannot fail. The count is read before and after the statement.
static RUNS_WRITTEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Returns how many runs have been written in this process since it started.
pub fn runs_written() -> u64 {
    RUNS_WRITTEN.load(std::sync::atomic::Ordering::Relaxed)
}

/// Returns how many bytes an operator holds before it writes a run.
///
/// @param shipped - the figure the operator uses when nothing has changed it
pub fn run_bytes(shipped: u64) -> u64 {
    match RUN_BYTES_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        0 => shipped,
        changed => changed,
    }
}

/// Changes how many bytes every spilling operator holds before it writes a run, for every
/// connection in the process.
///
/// **Only a test calls this.** Lowering it changes when a run is written and never which rows come
/// out, so a test running beside the caller still gets right answers, only slower ones.
///
/// @param bytes - the new figure, or zero to go back to each operator's own
pub fn set_run_bytes_override(bytes: u64) {
    RUN_BYTES_OVERRIDE.store(bytes, std::sync::atomic::Ordering::Relaxed);
}

/// Returns how many rows a statement moves at a time through a bounded buffer.
///
/// @param shipped - the figure used when nothing has changed it
pub fn chunk_rows(shipped: usize) -> usize {
    match CHUNK_ROWS_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        0 => shipped,
        changed => usize::try_from(changed).unwrap_or(shipped),
    }
}

/// Changes how many rows a statement moves at a time through a bounded buffer, for every
/// connection in the process.
///
/// **Only a test calls this**, for the reason [`set_run_bytes_override`] gives.
///
/// @param rows - the new figure, or zero to go back to the shipped one
pub fn set_chunk_rows_override(rows: u64) {
    CHUNK_ROWS_OVERRIDE.store(rows, std::sync::atomic::Ordering::Relaxed);
}

/// A file a buffering operator writes runs to and reads them back from.
///
/// Deliberately narrower than the VFS's own file: an operator appends, then
/// reads from an offset, and never locks, syncs, truncates or renames. A
/// narrow trait is what lets the test implementation below be eleven lines.
pub trait SpillFile {
    /// Appends bytes and returns the offset they were written at.
    ///
    /// @param bytes - what to write
    fn append(&mut self, bytes: &[u8]) -> DbResult<u64>;

    /// Reads into `out`, filling it, starting at `offset`.
    ///
    /// @param offset - where to read from
    /// @param out - the buffer to fill
    fn read_at(&self, offset: u64, out: &mut [u8]) -> DbResult<()>;
}

/// Opens the files a statement's buffering operators spill to.
///
/// One per statement rather than one per operator, so a query with two sorts
/// in it does not open two files when one would do; the runs carry their own
/// offsets.
pub trait Spill {
    /// Returns a fresh file to write runs to.
    fn open(&self) -> DbResult<Box<dyn SpillFile>>;
}

/// Where one run lives in a spill file.
#[derive(Clone, Copy, Debug)]
pub struct Run {
    /// Where the run's first row starts.
    pub at: u64,
    /// How many bytes the run occupies.
    pub bytes: u64,
    /// How many rows it holds.
    pub rows: usize,
}

/// Encodes rows into a run's bytes.
///
/// The caller has already sorted them; this only writes.
///
/// @param rows - the rows, in the order they will be read back
pub fn encode_run(rows: &[Vec<OwnedDatum>]) -> Vec<u8> {
    let mut out = Vec::new();
    for row in rows {
        let width = u32::try_from(row.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&width.to_le_bytes());
        for value in row {
            value.borrow().encode_tagged(&mut out);
        }
    }
    out
}

/// Reads one row from the front of a run's bytes.
///
/// Returns the row and how many bytes it consumed, or `None` at the end.
///
/// **The values are copied into owned storage** rather than borrowed from the
/// buffer, because the merge holds one row per run while the buffer that row
/// came from is refilled underneath it.
///
/// @param bytes - the remaining bytes of the run
pub fn decode_row(bytes: &[u8]) -> DbResult<Option<(Vec<OwnedDatum>, usize)>> {
    let Some(header) = bytes.get(..4) else {
        return Ok(None);
    };
    let mut width = [0u8; 4];
    width.copy_from_slice(header);
    let width = u32::from_le_bytes(width) as usize;
    let mut row = Vec::with_capacity(width);
    let mut at = 4usize;
    for _ in 0..width {
        let rest = bytes
            .get(at..)
            .ok_or_else(|| short("a spilled row ends inside a value"))?;
        // **Re-classified on the way out.** `decode_tagged` reports `Corrupt`,
        // which is the right answer about a page and the wrong one about a
        // spill file: these bytes were written by this process minutes ago and
        // are deleted on close, so a caller told their data is corrupt would
        // go and look at a database that is fine.
        let (value, taken) = Datum::decode_tagged(rest)
            .map_err(|why| short(&format!("a spilled row did not decode: {}", why.message())))?;
        row.push(OwnedDatum::from_datum(&value));
        at = at.saturating_add(taken);
    }
    Ok(Some((row, at)))
}

/// How many bytes of a run are held at once while merging.
///
/// Sixty-four kilobytes a run, so a sixteen-way merge holds a megabyte. Large
/// enough that a row almost never spans two fills, small enough that the
/// merge's own footprint is not the thing that runs out of memory.
const WINDOW: usize = 64 * 1024;

/// One run, read back a window at a time.
///
/// **The whole run is never resident**, which is the half of an external sort
/// that makes it external: a merge that read every run into memory would have
/// the same high-water mark as the sort it replaced.
pub struct RunReader {
    /// Where the run lives and how long it is.
    run: Run,
    /// The run-relative offset of `held[0]`.
    start: u64,
    /// The window.
    held: Vec<u8>,
    /// Where in `held` the next row starts.
    at: usize,
    /// How large the next fill will be.
    ///
    /// **It grows, because a row can be larger than the window.** A blob is
    /// bounded by the page size and the page size can be larger than 64 KiB,
    /// so a fixed window would decode the front of such a row, find the value
    /// running past the buffer, and report a truncated run for a run that is
    /// perfectly good. Doubling until the row fits is bounded by the row.
    ///
    /// **It goes back to [`WINDOW`] once a row has been read.** It used to double after every
    /// fill, so a run read from its first row to its last read a window twice the size of the one
    /// before, and a single run of a million rows was read into memory whole by the fourteenth
    /// fill. A merge's runs are short enough for that not to show; a file of rows read back in
    /// one run is not.
    window: usize,
}

impl RunReader {
    /// Returns a reader positioned at the start of a run.
    ///
    /// @param run - where the run lives
    pub fn new(run: Run) -> RunReader {
        RunReader {
            run,
            start: 0,
            held: Vec::new(),
            at: 0,
            window: WINDOW,
        }
    }

    /// Returns the next row of the run, or `None` when it is exhausted.
    ///
    /// @param file - the spill file the run lives in
    pub fn next(&mut self, file: &dyn SpillFile) -> DbResult<Option<Vec<OwnedDatum>>> {
        let mut fills = 0usize;
        loop {
            if let Ok(Some((row, taken))) = decode_row(self.held.get(self.at..).unwrap_or(&[])) {
                self.at = self.at.saturating_add(taken);
                self.window = WINDOW;
                return Ok(Some(row));
            }
            // A fill that did not hold a whole row means the row is larger than the window, so
            // the next fill is twice as large. The first fill of a call is not that case: the
            // window was only empty.
            if fills > 0 {
                self.window = self.window.saturating_mul(2);
            }
            let consumed = self
                .start
                .saturating_add(u64::try_from(self.at).unwrap_or(0));
            if consumed >= self.run.bytes {
                return Ok(None);
            }
            let remaining = self.run.bytes.saturating_sub(consumed);
            let want = usize::try_from(remaining.min(self.window as u64)).unwrap_or(self.window);
            let already = self.held.len().saturating_sub(self.at);
            if want <= already {
                // Everything left is already in the window and it still does
                // not decode, so the run is short. `decode_row` says which.
                return decode_row(self.held.get(self.at..).unwrap_or(&[]))
                    .map(|held| held.map(|(row, _)| row));
            }
            let mut buffer = vec![0u8; want];
            file.read_at(self.run.at.saturating_add(consumed), &mut buffer)?;
            self.start = consumed;
            self.held = buffer;
            self.at = 0;
            fills = fills.saturating_add(1);
        }
    }
}

/// Compares two rows of a run, for the merge.
pub type RowOrder<'a> = Box<dyn Fn(&[OwnedDatum], &[OwnedDatum]) -> std::cmp::Ordering + 'a>;

/// The runs an operator has written, and the file they are in.
///
/// **One writer for every operator that spills.** `Sort` wrote its own copy of this and `Distinct`,
/// the set operations, the window pass and `CREATE INDEX` need the same thing: rows already in the
/// order the operator wants, appended to one transient file as a run, and read back by a merge. The
/// file is opened on the first run and not before, so an operator that never spills never opens one.
pub struct RunWriter {
    /// Where files come from.
    spill: std::rc::Rc<dyn Spill>,
    /// The file the runs are in, opened by the first run.
    file: Option<Box<dyn SpillFile>>,
    /// Every run written so far, in the order they were written.
    runs: Vec<Run>,
    /// How many rows the runs hold in all.
    rows: u64,
}

impl RunWriter {
    /// Returns a writer that has written nothing.
    ///
    /// @param spill - where files come from
    pub fn new(spill: std::rc::Rc<dyn Spill>) -> RunWriter {
        RunWriter {
            spill,
            file: None,
            runs: Vec::new(),
            rows: 0,
        }
    }

    /// Reports whether no run has been written.
    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }

    /// Returns how many runs have been written.
    pub fn run_count(&self) -> usize {
        self.runs.len()
    }

    /// Returns how many rows the runs hold.
    pub fn row_count(&self) -> u64 {
        self.rows
    }

    /// Writes rows, already in the order the merge will compare them in, as one run.
    ///
    /// @param rows - the rows
    pub fn write(&mut self, rows: &[Vec<OwnedDatum>]) -> DbResult<()> {
        if rows.is_empty() {
            return Ok(());
        }
        self.write_encoded(&encode_run(rows), rows.len())
    }

    /// Writes rows that the caller has already encoded as one run.
    ///
    /// The encoding is the one [`encode_run`] produces. A caller whose rows are not
    /// `Vec<OwnedDatum>` (the index builder keeps its entries in an arena) encodes them itself and
    /// avoids building the owned form of every row.
    ///
    /// @param bytes - the encoded rows
    /// @param rows - how many rows the bytes hold
    pub fn write_encoded(&mut self, bytes: &[u8], rows: usize) -> DbResult<()> {
        if rows == 0 {
            return Ok(());
        }
        if self.file.is_none() {
            self.file = Some(self.spill.open()?);
        }
        let Some(file) = self.file.as_mut() else {
            return Err(short("a spill file was not opened"));
        };
        let at = file.append(bytes)?;
        self.runs.push(Run {
            at,
            bytes: bytes.len() as u64,
            rows,
        });
        self.rows = self.rows.saturating_add(rows as u64);
        RUNS_WRITTEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    /// Returns a merge over every run written, in the order `compare` gives.
    ///
    /// @param compare - the order the rows of each run were written in
    pub fn merge<'a>(&'a self, compare: RowOrder<'a>) -> DbResult<RunMerge<'a>> {
        match self.file.as_deref() {
            Some(file) => RunMerge::new(file, &self.runs, compare),
            None => Ok(RunMerge::empty(compare)),
        }
    }
}

/// Reads the rows of many sorted runs back in one sorted order.
///
/// **The comparison is the caller's, and it is the same one that sorted each run.** A heap of the
/// runs' front rows keeps the cost of a row at the logarithm of the runs and not at their number,
/// which matters once a large input has written dozens of them. Of two rows that compare equal the
/// one from the earlier run comes first, and the runs were written in arrival order, so the merge
/// is stable whenever each run was sorted stably.
pub struct RunMerge<'a> {
    /// The file the runs are in.
    file: Option<&'a dyn SpillFile>,
    /// One reader per run.
    readers: Vec<RunReader>,
    /// The next row of each run, or none when it is exhausted.
    fronts: Vec<Option<Vec<OwnedDatum>>>,
    /// The runs that still have a front row, as a min heap ordered by those rows.
    heap: Vec<usize>,
    /// The order the rows of each run are in.
    compare: RowOrder<'a>,
}

impl<'a> RunMerge<'a> {
    /// Returns a merge of nothing.
    ///
    /// @param compare - the order, unused
    fn empty(compare: RowOrder<'a>) -> RunMerge<'a> {
        RunMerge {
            file: None,
            readers: Vec::new(),
            fronts: Vec::new(),
            heap: Vec::new(),
            compare,
        }
    }

    /// Returns a merge positioned before the first row.
    ///
    /// @param file - the file the runs are in
    /// @param runs - the runs, each already in the order `compare` gives
    /// @param compare - the order
    fn new(file: &'a dyn SpillFile, runs: &[Run], compare: RowOrder<'a>) -> DbResult<RunMerge<'a>> {
        let mut readers: Vec<RunReader> = runs.iter().map(|run| RunReader::new(*run)).collect();
        let mut fronts: Vec<Option<Vec<OwnedDatum>>> = Vec::with_capacity(readers.len());
        for reader in &mut readers {
            fronts.push(reader.next(file)?);
        }
        let mut merge = RunMerge {
            file: Some(file),
            readers,
            fronts,
            heap: Vec::new(),
            compare,
        };
        for run in 0..merge.fronts.len() {
            if merge.fronts.get(run).is_some_and(Option::is_some) {
                merge.heap.push(run);
            }
        }
        for at in (0..merge.heap.len() / 2).rev() {
            merge.sift_down(at);
        }
        Ok(merge)
    }

    /// Reports whether the run `a` sorts before the run `b`, by their front rows.
    ///
    /// @param a - one run
    /// @param b - the other
    fn precedes(&self, a: usize, b: usize) -> bool {
        let (Some(Some(left)), Some(Some(right))) = (self.fronts.get(a), self.fronts.get(b)) else {
            return a < b;
        };
        match (self.compare)(left, right) {
            std::cmp::Ordering::Less => true,
            std::cmp::Ordering::Greater => false,
            std::cmp::Ordering::Equal => a < b,
        }
    }

    /// Restores the heap below one position.
    ///
    /// @param start - the heap position whose children are heaps
    fn sift_down(&mut self, start: usize) {
        let mut at = start;
        loop {
            let first = at.saturating_mul(2).saturating_add(1);
            let mut best = at;
            for child in [first, first.saturating_add(1)] {
                let (Some(candidate), Some(held)) = (self.heap.get(child), self.heap.get(best))
                else {
                    continue;
                };
                if self.precedes(*candidate, *held) {
                    best = child;
                }
            }
            if best == at {
                return;
            }
            self.heap.swap(at, best);
            at = best;
        }
    }

    /// Returns the next row in order, or `None` when every run is exhausted.
    pub fn next(&mut self) -> DbResult<Option<Vec<OwnedDatum>>> {
        let Some(top) = self.heap.first().copied() else {
            return Ok(None);
        };
        let Some(file) = self.file else {
            return Ok(None);
        };
        let row = self.fronts.get_mut(top).and_then(Option::take);
        let refilled = match self.readers.get_mut(top) {
            Some(reader) => reader.next(file)?,
            None => None,
        };
        let more = refilled.is_some();
        if let Some(front) = self.fronts.get_mut(top) {
            *front = refilled;
        }
        if !more {
            self.heap.swap_remove(0);
        }
        self.sift_down(0);
        Ok(row)
    }
}

/// Compares two rows, owned by the sorter so it can outlive the call that built it.
pub type OwnedOrder = std::rc::Rc<dyn Fn(&[OwnedDatum], &[OwnedDatum]) -> std::cmp::Ordering>;

/// Says whether the later of two rows that sorted next to each other adds nothing.
///
/// Applied to the rows of a run as it is written, so a run of rows that repeat a key is shorter
/// than the input that made it. A later stage still sees repeats that sit in different runs, and
/// must drop them itself: this saves space and never decides an answer.
pub type Redundant = fn(&[OwnedDatum], &[OwnedDatum]) -> bool;

/// Sorts rows that may not fit in memory, by writing sorted runs and merging them.
///
/// **The one external sort the operators that are not `ORDER BY` share.** `DISTINCT`, the set
/// operations and the window pass each need their rows in an order of their own, sometimes twice,
/// and each used to hold every row in a vector. A sorter holds rows until they pass a byte limit,
/// sorts them with a stable sort, and writes them as a run. [\`ExternalSorter::finish\`] returns
/// the rows in order, merging the runs when there are any. Of two rows the order calls equal, the
/// one pushed first comes first, whether they sat in one run or two, because the runs are merged
/// in the order they were written.
pub struct ExternalSorter {
    /// The order of the output.
    order: OwnedOrder,
    /// Drops a row from a run when it adds nothing to the row before it.
    redundant: Option<Redundant>,
    /// Where files come from, or none to hold every row.
    spill: Option<std::rc::Rc<dyn Spill>>,
    /// The rows held since the last run.
    rows: Vec<Vec<OwnedDatum>>,
    /// Roughly how many bytes they occupy.
    held: u64,
    /// How many bytes may be held before a run is written.
    limit: u64,
    /// The runs, once there is one.
    writer: Option<RunWriter>,
}

impl ExternalSorter {
    /// Returns a sorter with nothing in it.
    ///
    /// @param order - the order of the output
    /// @param spill - where files come from, or none to hold every row
    /// @param shipped - how many bytes to hold before writing a run, unless a test changed it
    pub fn new(
        order: OwnedOrder,
        spill: Option<std::rc::Rc<dyn Spill>>,
        shipped: u64,
    ) -> ExternalSorter {
        ExternalSorter {
            order,
            redundant: None,
            spill,
            rows: Vec::new(),
            held: 0,
            limit: run_bytes(shipped),
            writer: None,
        }
    }

    /// Returns the sorter with a rule that drops repeats from the runs it writes.
    ///
    /// @param redundant - the rule
    pub fn with_redundant(mut self, redundant: Redundant) -> ExternalSorter {
        self.redundant = Some(redundant);
        self
    }

    /// Adds one row, writing a run first when the rows held have passed the limit.
    ///
    /// @param row - the row
    pub fn push(&mut self, row: Vec<OwnedDatum>) -> DbResult<()> {
        self.held = self.held.saturating_add(resident_row_bytes(&row));
        self.rows.push(row);
        if self.spill.is_some() && self.held >= self.limit {
            self.write_a_run()?;
        }
        Ok(())
    }

    /// Returns how many runs have been written.
    pub fn runs_written(&self) -> usize {
        self.writer.as_ref().map_or(0, RunWriter::run_count)
    }

    /// Sorts the rows held and writes them as one run.
    fn write_a_run(&mut self) -> DbResult<()> {
        let Some(spill) = self.spill.clone() else {
            return Ok(());
        };
        if self.rows.is_empty() {
            return Ok(());
        }
        let order = std::rc::Rc::clone(&self.order);
        self.rows.sort_by(|left, right| order(left, right));
        if let Some(redundant) = self.redundant {
            let mut kept: Vec<Vec<OwnedDatum>> = Vec::with_capacity(self.rows.len());
            for row in std::mem::take(&mut self.rows) {
                if kept.last().is_some_and(|last| redundant(last, &row)) {
                    continue;
                }
                kept.push(row);
            }
            self.rows = kept;
        }
        let writer = self.writer.get_or_insert_with(|| RunWriter::new(spill));
        writer.write(&self.rows)?;
        self.rows.clear();
        self.held = 0;
        Ok(())
    }

    /// Ends the sort and returns the rows in order.
    pub fn finish(mut self) -> DbResult<SortedRows> {
        if self.writer.is_some() {
            self.write_a_run()?;
        } else {
            let order = std::rc::Rc::clone(&self.order);
            self.rows.sort_by(|left, right| order(left, right));
        }
        Ok(SortedRows {
            rows: self.rows,
            writer: self.writer,
            order: self.order,
        })
    }
}

/// Rows in order, from memory or from a merge of runs.
pub struct SortedRows {
    /// The rows, when no run was written.
    rows: Vec<Vec<OwnedDatum>>,
    /// The runs, when any were.
    writer: Option<RunWriter>,
    /// The order the runs were written in.
    order: OwnedOrder,
}

impl SortedRows {
    /// Hands each row to \`each\` in order, until it answers \`Flow::Stop\`.
    ///
    /// @param each - what to do with a row
    pub fn for_each(
        self,
        mut each: impl FnMut(Vec<OwnedDatum>) -> DbResult<crate::ops::Flow>,
    ) -> DbResult<()> {
        let SortedRows {
            rows,
            writer,
            order,
        } = self;
        let Some(writer) = writer else {
            for row in rows {
                if each(row)? == crate::ops::Flow::Stop {
                    return Ok(());
                }
            }
            return Ok(());
        };
        let mut merge = writer.merge(Box::new(move |left, right| order(left, right)))?;
        while let Some(row) = merge.next()? {
            inillucent_base::budget::check()?;
            if each(row)? == crate::ops::Flow::Stop {
                break;
            }
        }
        Ok(())
    }
}

/// The failure a truncated run reports.
///
/// A spill file is written and read by one statement in one process, so a run
/// that does not decode is this engine's own defect rather than a damaged
/// file, and it says so rather than reporting corruption a user could act on.
///
/// @param said - what was wrong
fn short(said: &str) -> inillucent_base::DbError {
    inillucent_base::DbError::primary(PrimaryCode::Internal).with_message(said)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A run round-trips every value kind, in order.
    #[test]
    fn a_run_reads_back_the_rows_it_was_written_from() {
        let rows = vec![
            vec![OwnedDatum::Null, OwnedDatum::Int(-9)],
            vec![OwnedDatum::Real(1.5), OwnedDatum::Text(b"hello".to_vec())],
            vec![OwnedDatum::Blob(vec![0, 255, 7]), OwnedDatum::Int(0)],
        ];
        let bytes = encode_run(&rows);
        let mut read: Vec<Vec<OwnedDatum>> = Vec::new();
        let mut at = 0usize;
        while let Some((row, taken)) = decode_row(bytes.get(at..).unwrap_or(&[])).unwrap() {
            read.push(row);
            at = at.saturating_add(taken);
            if taken == 0 {
                break;
            }
        }
        assert_eq!(
            read, rows,
            "a run did not read back what it was written from"
        );
    }

    /// An empty run reads back as no rows rather than as one empty row.
    #[test]
    fn an_empty_run_holds_nothing() {
        let bytes = encode_run(&[]);
        assert!(bytes.is_empty());
        assert!(decode_row(&bytes).unwrap().is_none());
    }

    /// A run cut short says so rather than answering a row it did not read.
    ///
    /// The bytes come from this process, so the failure is `Internal`: a
    /// caller can do nothing about it and it is not damage to their file.
    #[test]
    fn a_truncated_run_is_refused() {
        let rows = vec![vec![OwnedDatum::Text(b"a long enough value".to_vec())]];
        let bytes = encode_run(&rows);
        let cut = bytes.get(..bytes.len() - 5).unwrap_or(&[]);
        let failure = decode_row(cut).expect_err("a truncated run must not decode");
        assert_eq!(failure.code(), PrimaryCode::Internal);
    }

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
                .ok_or_else(|| short("a spill read ran past the file"))?;
            out.copy_from_slice(slice);
            Ok(())
        }
    }

    /// Writes one run and returns where it landed.
    fn write(file: &mut InMemory, rows: &[Vec<OwnedDatum>]) -> Run {
        let bytes = encode_run(rows);
        let at = file.append(&bytes).expect("a run is written");
        Run {
            at,
            bytes: bytes.len() as u64,
            rows: rows.len(),
        }
    }

    /// A run reads back through the reader exactly as it was written.
    #[test]
    fn a_reader_walks_a_run_to_its_end() {
        let mut file = InMemory::default();
        let rows: Vec<Vec<OwnedDatum>> = (0..500)
            .map(|n| {
                vec![
                    OwnedDatum::Int(n),
                    OwnedDatum::Text(format!("row-{n:04}").into_bytes()),
                ]
            })
            .collect();
        let run = write(&mut file, &rows);
        let mut reader = RunReader::new(run);
        let mut read = Vec::new();
        while let Some(row) = reader.next(&file).expect("a row") {
            read.push(row);
        }
        assert_eq!(read, rows, "the reader did not walk the whole run");
    }

    /// **A row larger than the window is read, not reported as truncated.**
    ///
    /// The window is 64 KiB and a value is bounded by the page size, which can
    /// be larger. A fixed window would decode the front of such a row, find
    /// the value running past the buffer, and call a perfectly good run short.
    #[test]
    fn a_row_larger_than_the_window_is_read() {
        let mut file = InMemory::default();
        let rows = vec![
            vec![OwnedDatum::Blob(vec![7u8; 200 * 1024])],
            vec![OwnedDatum::Int(1)],
        ];
        let run = write(&mut file, &rows);
        let mut reader = RunReader::new(run);
        let mut read = Vec::new();
        while let Some(row) = reader.next(&file).expect("a row") {
            read.push(row);
        }
        assert_eq!(read, rows, "a row past the window was not read back");
    }

    /// Two runs in one file do not read each other's rows.
    ///
    /// The offsets are the whole of what keeps them apart, so a reader that
    /// ignored `Run::bytes` would walk off the end of its own run and into the
    /// next one - and the rows would still decode, which is what makes it
    /// worth a case.
    #[test]
    fn two_runs_in_one_file_stay_apart() {
        let mut file = InMemory::default();
        let first: Vec<Vec<OwnedDatum>> = (0..20).map(|n| vec![OwnedDatum::Int(n)]).collect();
        let second: Vec<Vec<OwnedDatum>> = (100..130).map(|n| vec![OwnedDatum::Int(n)]).collect();
        let one = write(&mut file, &first);
        let two = write(&mut file, &second);

        for (run, expected) in [(one, &first), (two, &second)] {
            let mut reader = RunReader::new(run);
            let mut read = Vec::new();
            while let Some(row) = reader.next(&file).expect("a row") {
                read.push(row);
            }
            assert_eq!(&read, expected, "a run read past its own bytes");
        }
    }
}

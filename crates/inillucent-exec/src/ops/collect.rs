//! The sinks that keep rows: the two collectors and the row emitter.
//!
//! Invariant: **a collector is where a borrowed batch stops being
//! borrowed.** Everything above it hands on a slice of the page it was
//! read from; these are the operators that copy, because their whole
//! job is to outlive the pin.

use inillucent_base::DbResult;
use inillucent_tree::datum::{Datum, OwnedDatum};

use crate::batch::{Batch, Vector};

/// Whether the pipeline should keep going.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Flow {
    /// Keep pushing.
    Continue,
    /// Enough rows have been produced; the source may stop.
    Stop,
}
/// An operator that consumes batches.
pub trait Sink {
    /// Consumes one batch.
    ///
    /// @param batch - the batch to consume
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow>;

    /// Signals end of input and lets a pipeline breaker emit.
    fn finish(&mut self) -> DbResult<()>;

    /// Returns the operator, and everything below it, to its pre-input state.
    ///
    /// This is what makes an operator chain a *prepared statement* rather than
    /// a single-use object, and it has to be implemented rather than defaulted:
    /// an operator that forgot to clear an accumulator would answer the second
    /// execution with the first one's rows folded in, which is a wrong answer
    /// that no test of a single execution can see. The compiler asking every
    /// implementor the question is the point.
    ///
    /// An operator with no state of its own still forwards the call, because
    /// the operator below it may have some.
    fn reset(&mut self) -> DbResult<()>;
}
/// The end of a pipeline: it keeps the rows.
///
/// A pull-style statement API steps over what this collected. Rows are owned
/// because they outlive the pages they came from - that is what "the result of
/// a query" means.
#[derive(Default)]
pub struct Collect {
    rows: Vec<Vec<OwnedDatum>>,
    limit: Option<usize>,
}
impl Collect {
    /// Returns a sink that keeps every row.
    pub fn new() -> Collect {
        Collect {
            rows: Vec::new(),
            limit: None,
        }
    }

    /// Returns a sink that stops the pipeline after `limit` rows.
    ///
    /// @param limit - how many rows to keep
    pub fn with_limit(limit: usize) -> Collect {
        Collect {
            rows: Vec::new(),
            limit: Some(limit),
        }
    }

    /// Returns the rows collected.
    pub fn rows(&self) -> &[Vec<OwnedDatum>] {
        &self.rows
    }

    /// Returns the rows collected, consuming the sink.
    pub fn into_rows(self) -> Vec<Vec<OwnedDatum>> {
        self.rows
    }
}
impl Sink for Collect {
    /// Collects one batch, counting it against the request's budget.
    ///
    /// **This is where a result's size is bounded**, and it is the right place
    /// because it is the one every row a caller receives passes through. The
    /// batch is counted before it is copied, so a request that is already past
    /// its budget does not pay for the copy that takes it further past.
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        inillucent_base::budget::check()?;
        inillucent_base::budget::spend(batch.live() as u64, batch_bytes(batch))?;
        for nth in 0..batch.live() {
            if let Some(limit) = self.limit {
                if self.rows.len() >= limit {
                    return Ok(Flow::Stop);
                }
            }
            let mut row = Vec::with_capacity(batch.columns.len());
            for column in 0..batch.columns.len() {
                row.push(OwnedDatum::from_datum(&batch.value(nth, column)?));
            }
            self.rows.push(row);
        }
        match self.limit {
            Some(limit) if self.rows.len() >= limit => Ok(Flow::Stop),
            _ => Ok(Flow::Continue),
        }
    }

    fn finish(&mut self) -> DbResult<()> {
        Ok(())
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.rows.clear();
        Ok(())
    }
}
/// Holds every row it is given and passes them on last first.
///
/// What `PRAGMA reverse_unordered_selects` does to a query with no `ORDER BY`.
/// SQLite runs every loop of such a query backwards: the outer scan, each inner
/// join term, and the values of an `IN` list. A nested loop whose every level
/// runs backwards produces exactly the reverse of what it produces forwards, so
/// reversing the joined rows gives SQLite's order however many terms there are.
/// The planner puts this in only when every term reads a table or an index,
/// because SQLite cannot run a virtual table or a subquery backwards.
///
/// It sits below the aggregate and the `LIMIT`, so `LIMIT 1` keeps the last row
/// and `group_concat` sees the rows last first, as they do in SQLite. Rows a
/// query reads no column of are counted rather than kept, because a batch with
/// no columns still carries how many rows it has.
pub struct Reversed {
    rows: Vec<Vec<OwnedDatum>>,
    /// Rows seen with no columns at all.
    bare: usize,
    downstream: Box<dyn Sink>,
}
impl Reversed {
    /// Returns a sink that reverses everything it is given.
    ///
    /// @param downstream - what the rows are passed on to
    pub fn new(downstream: Box<dyn Sink>) -> Reversed {
        Reversed {
            rows: Vec::new(),
            bare: 0,
            downstream,
        }
    }
}
impl Sink for Reversed {
    /// Keeps one batch, counting it against the request's budget.
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        inillucent_base::budget::check()?;
        inillucent_base::budget::spend(batch.live() as u64, batch_bytes(batch))?;
        if batch.columns.is_empty() {
            self.bare = self.bare.saturating_add(batch.live());
            return Ok(Flow::Continue);
        }
        for nth in 0..batch.live() {
            let mut row = Vec::with_capacity(batch.columns.len());
            for column in 0..batch.columns.len() {
                row.push(OwnedDatum::from_datum(&batch.value(nth, column)?));
            }
            self.rows.push(row);
        }
        Ok(Flow::Continue)
    }

    fn finish(&mut self) -> DbResult<()> {
        let mut rows = std::mem::take(&mut self.rows);
        rows.reverse();
        let mut flow = emit_rows(&rows, self.downstream.as_mut())?;
        let mut bare = std::mem::take(&mut self.bare);
        while bare > 0 && flow == Flow::Continue {
            let chunk = bare.min(crate::batch::BATCH_ROWS);
            flow = self.downstream.push(&Batch::new(chunk, Vec::new()))?;
            bare -= chunk;
        }
        self.downstream.finish()
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.rows.clear();
        self.bare = 0;
        self.downstream.reset()
    }
}
/// A sink that appends into a buffer the caller still holds.
///
/// The pipeline owns its sink, so a caller that wants the rows back cannot
/// simply unwrap the chain afterwards. Sharing the buffer is the small, honest
/// way out: the caller keeps one handle, the pipeline keeps the other, and the
/// rows are readable the moment the scan returns.
pub struct CollectInto {
    rows: std::rc::Rc<std::cell::RefCell<Vec<Vec<OwnedDatum>>>>,
    limit: Option<usize>,
    /// Where the rows go instead, while the caller has put a collection here (task-2226).
    gathering: Option<Gather>,
    /// Where the rows go instead, while the caller has put a consumer here.
    streaming: Option<Streaming>,
    /// How many rows have been handed to the consumer, which the limit counts.
    streamed: usize,
}

/// The collection a [`CollectInto`] fills while the caller has put one in the cell.
///
/// Packed or one vector a row, moving to a transient file past a byte limit; see
/// [`crate::gather::Gathering`].
pub type Gather = std::rc::Rc<std::cell::RefCell<Option<crate::gather::Gathering>>>;

/// What receives the rows of a statement that is streamed instead of collected.
///
/// **The consumer is handed each row while the page it was read from is still
/// pinned**, so the row borrows from the page and is gone when `accept` returns.
/// A consumer that keeps a row copies it. A statement that is collected holds
/// every row until it ends; a statement that is streamed holds one batch, which
/// is why printing the rows of a large table no longer costs the size of the
/// table.
pub trait RowConsumer: std::any::Any {
    /// Tells the consumer the names of the result columns, once, before its first row.
    ///
    /// Called for a statement that produces no rows too, so a consumer that draws a header can
    /// tell a statement with columns and no rows from one with no columns. Provided, because a
    /// consumer that only counts has no use for the names.
    ///
    /// @param names - the result columns, in order
    fn columns(&mut self, _names: &[String]) {}

    /// Takes one row of the answer.
    ///
    /// @param row - the row's values, borrowed from the page
    fn accept(&mut self, row: &[Datum<'_>]) -> DbResult<Flow>;
}

/// The consumer a [`CollectInto`] hands rows to while the caller has put one in the cell.
pub type Streaming = std::rc::Rc<std::cell::RefCell<Option<Box<dyn RowConsumer>>>>;

/// A consumer that takes no rows, handed back when a streamed run lost the caller's own.
pub struct NoConsumer;

impl RowConsumer for NoConsumer {
    /// Takes nothing and asks the pipeline to stop.
    fn accept(&mut self, _row: &[Datum<'_>]) -> DbResult<Flow> {
        Ok(Flow::Stop)
    }
}

/// Hands rows that were collected to a consumer, one at a time, until it asks to stop.
///
/// For a statement that cannot stream: the rows exist already, so this is the same call the
/// consumer would have had from the pipeline.
///
/// @param rows - the collected rows
/// @param consumer - what takes each row
pub fn hand_over(rows: &[Vec<OwnedDatum>], consumer: &mut dyn RowConsumer) -> DbResult<()> {
    for row in rows {
        let borrowed = inillucent_tree::datum::borrow_row(row);
        if consumer.accept(&borrowed)? == Flow::Stop {
            break;
        }
    }
    Ok(())
}
impl CollectInto {
    /// Returns a sink appending into a shared buffer.
    ///
    /// @param rows - the buffer the caller keeps a handle on
    pub fn new(rows: std::rc::Rc<std::cell::RefCell<Vec<Vec<OwnedDatum>>>>) -> CollectInto {
        CollectInto {
            rows,
            limit: None,
            gathering: None,
            streaming: None,
            streamed: 0,
        }
    }

    /// Returns the sink handing rows to a consumer instead whenever the cell holds one.
    ///
    /// **For a statement the caller prints or counts row by row.** Nothing is
    /// appended to the buffer while a consumer is in the cell, so the memory the
    /// statement needs for its answer is one batch and not the answer.
    ///
    /// @param streaming - the cell; empty, the sink collects as before
    pub fn with_streaming(mut self, streaming: Streaming) -> CollectInto {
        self.streaming = Some(streaming);
        self
    }

    /// Hands a batch's live rows to a consumer one at a time, stopping at the limit.
    ///
    /// @param consumer - what takes each row
    /// @param batch - the rows
    /// @param limit - how many rows to hand over in all
    /// @param streamed - how many rows were handed over before this batch
    fn push_streamed(
        consumer: &mut dyn RowConsumer,
        batch: &Batch<'_>,
        limit: Option<usize>,
        streamed: &mut usize,
    ) -> DbResult<Flow> {
        let width = batch.columns.len();
        let mut row: Vec<Datum<'_>> = Vec::with_capacity(width);
        for nth in 0..batch.live() {
            if limit.is_some_and(|limit| *streamed >= limit) {
                return Ok(Flow::Stop);
            }
            row.clear();
            for column in 0..width {
                row.push(batch.value(nth, column)?);
            }
            *streamed = streamed.saturating_add(1);
            if consumer.accept(&row)? == Flow::Stop {
                return Ok(Flow::Stop);
            }
        }
        match limit {
            Some(limit) if *streamed >= limit => Ok(Flow::Stop),
            _ => Ok(Flow::Continue),
        }
    }

    /// Returns the sink collecting into a [`crate::gather::Gathering`] instead whenever the cell
    /// holds one.
    ///
    /// **For an `INSERT ... SELECT`** (task-2226). Its rows are read once each and then copied
    /// into the images, so a vector a row and a copy of each text were 200,000 allocations for a
    /// 100,000 row copy. The collection packs them, and moves them to a transient file when they
    /// pass a byte limit, so a copy of millions of rows is not held in memory whole.
    ///
    /// @param gathering - the cell; empty, the sink collects into `rows` as before
    pub fn with_gathering(mut self, gathering: Gather) -> CollectInto {
        self.gathering = Some(gathering);
        self
    }

    /// Returns a sink that stops the pipeline after `limit` rows.
    ///
    /// @param rows - the buffer the caller keeps a handle on
    /// @param limit - how many rows to keep
    pub fn with_limit(
        rows: std::rc::Rc<std::cell::RefCell<Vec<Vec<OwnedDatum>>>>,
        limit: usize,
    ) -> CollectInto {
        CollectInto {
            rows,
            limit: Some(limit),
            gathering: None,
            streaming: None,
            streamed: 0,
        }
    }
}
impl Sink for CollectInto {
    fn push(&mut self, batch: &Batch<'_>) -> DbResult<Flow> {
        if let Some(streaming) = &self.streaming {
            if let Some(consumer) = streaming.borrow_mut().as_mut() {
                return CollectInto::push_streamed(
                    consumer.as_mut(),
                    batch,
                    self.limit,
                    &mut self.streamed,
                );
            }
        }
        if let Some(cell) = &self.gathering {
            if let Some(gathering) = cell.borrow_mut().as_mut() {
                return gathering.push_batch(batch, self.limit);
            }
        }
        let mut held = self.rows.borrow_mut();
        for nth in 0..batch.live() {
            if let Some(limit) = self.limit {
                if held.len() >= limit {
                    return Ok(Flow::Stop);
                }
            }
            let mut row = Vec::with_capacity(batch.columns.len());
            for column in 0..batch.columns.len() {
                row.push(OwnedDatum::from_datum(&batch.value(nth, column)?));
            }
            held.push(row);
        }
        match self.limit {
            Some(limit) if held.len() >= limit => Ok(Flow::Stop),
            _ => Ok(Flow::Continue),
        }
    }

    fn finish(&mut self) -> DbResult<()> {
        Ok(())
    }

    /// Returns this operator and everything below it to its pre-input state.
    fn reset(&mut self) -> DbResult<()> {
        self.rows.borrow_mut().clear();
        self.streamed = 0;
        Ok(())
    }
}
/// Pushes materialised rows downstream in batch-sized chunks.
///
/// Returns roughly how many bytes of row data a batch holds.
///
/// **Roughly, and that is the honest word for it.** The number bounds a
/// *result*, so what it has to track is how much a caller is about to be handed
/// rather than what the engine allocated on the way. A text value costs its
/// bytes, a number costs its width, and a NULL costs the slot it occupies in
/// the row. Counting the allocator's real footprint would mean asking every
/// operator, and a budget nobody can compute is a budget nobody enforces.
///
/// @param batch - the batch about to be handed on
pub(crate) fn batch_bytes(batch: &Batch<'_>) -> u64 {
    let mut total = 0u64;
    for nth in 0..batch.live() {
        for column in 0..batch.columns.len() {
            let cost = match batch.value(nth, column) {
                Ok(Datum::Text(bytes)) | Ok(Datum::Blob(bytes)) => bytes.len() as u64,
                Ok(_) => 8,
                // A value that cannot be read is counted as a whole one rather
                // than as nothing: the failure it is about to raise is a better
                // report than an undercount, and an undercount here is the one
                // way this budget could be walked past.
                Err(_) => 8,
            };
            total = total.saturating_add(cost);
        }
    }
    total
}
/// Returns roughly how many bytes one owned row holds.
///
/// The same accounting `batch_bytes` uses, so a row charged as a batch and the
/// same row charged after it was materialised cost the same.
///
/// @param row - the row to measure
pub(crate) fn owned_row_bytes(row: &[OwnedDatum]) -> u64 {
    row.iter()
        .map(|value| datum_bytes(&value.borrow()))
        .fold(0u64, u64::saturating_add)
}
/// Returns roughly how many bytes one owned row occupies in memory.
///
/// [`owned_row_bytes`] counts the values, which is what a budget is stated in. A buffer that
/// decides when to write to disk has to count the row as it is held: the vector, a fixed size slot
/// per value, and an allocation per text or blob, which the allocator rounds up to 16 bytes.
///
/// @param row - the row to measure
pub(crate) fn resident_row_bytes(row: &[OwnedDatum]) -> u64 {
    let slots = std::mem::size_of::<Vec<OwnedDatum>>()
        .saturating_add(row.len().saturating_mul(std::mem::size_of::<OwnedDatum>()));
    let heap: u64 = row
        .iter()
        .map(|value| match value {
            OwnedDatum::Text(bytes) | OwnedDatum::Blob(bytes) => {
                (bytes.capacity() as u64).div_ceil(16) * 16 + 16
            }
            _ => 0,
        })
        .fold(0u64, u64::saturating_add);
    (slots as u64).saturating_add(heap)
}
/// Returns roughly how many bytes one value holds.
///
/// @param value - the value to measure
pub(crate) fn datum_bytes(value: &Datum<'_>) -> u64 {
    match value {
        Datum::Text(bytes) | Datum::Blob(bytes) => bytes.len() as u64,
        _ => 8,
    }
}
/// The one place a pipeline breaker turns owned rows back into batches. It
/// transposes row-major storage into per-column vectors, which is why every
/// breaker calls it rather than writing the transpose out again.
///
/// @param rows - the rows to emit
/// @param downstream - what to push them into
pub fn emit_rows(rows: &[Vec<OwnedDatum>], downstream: &mut dyn Sink) -> DbResult<Flow> {
    let width = rows.first().map(|row| row.len()).unwrap_or(0);
    if width == 0 {
        return Ok(Flow::Continue);
    }
    let mut start = 0usize;
    while start < rows.len() {
        let end = start
            .saturating_add(crate::batch::BATCH_ROWS)
            .min(rows.len());
        let chunk = rows.get(start..end).unwrap_or(&[]);
        let mut columns_owned: Vec<Vec<Datum<'_>>> = Vec::with_capacity(width);
        for column in 0..width {
            columns_owned.push(
                chunk
                    .iter()
                    .map(|row| {
                        row.get(column)
                            .map(OwnedDatum::borrow)
                            .unwrap_or(Datum::Null)
                    })
                    .collect(),
            );
        }
        let columns: Vec<Vector<'_>> = columns_owned
            .iter()
            .map(|values| Vector::Values(values.as_slice()))
            .collect();
        let batch = Batch::new(chunk.len(), columns);
        if downstream.push(&batch)? == Flow::Stop {
            return Ok(Flow::Stop);
        }
        start = end;
    }
    Ok(Flow::Continue)
}
/// Emits rows that all begin with the same leading values, without copying those values into
/// every row.
///
/// A lateral join pairs one outer row with every row a module answered. Building each joined row
/// by cloning the outer row copied the outer row's text once per answer, so `FROM j,
/// json_each(j.doc)` over a 110 KB document copied 110 KB for each of its 20,000 elements. The
/// leading values are constant columns of the batch instead, which borrow the one copy.
///
/// @param prefix - the values every emitted row starts with
/// @param rows - the rest of each emitted row, all of the same width
/// @param downstream - what to push them into
pub fn emit_rows_after_prefix(
    prefix: &[OwnedDatum],
    rows: &[Vec<OwnedDatum>],
    downstream: &mut dyn Sink,
) -> DbResult<Flow> {
    let rest = rows.first().map(|row| row.len()).unwrap_or(0);
    if prefix.len().saturating_add(rest) == 0 {
        return Ok(Flow::Continue);
    }
    for chunk in rows.chunks(crate::batch::BATCH_ROWS) {
        let mut columns_owned: Vec<Vec<Datum<'_>>> = Vec::with_capacity(rest);
        for column in 0..rest {
            columns_owned.push(
                chunk
                    .iter()
                    .map(|row| {
                        row.get(column)
                            .map(OwnedDatum::borrow)
                            .unwrap_or(Datum::Null)
                    })
                    .collect(),
            );
        }
        let mut columns: Vec<Vector<'_>> = prefix
            .iter()
            .map(|value| Vector::Const(value.borrow()))
            .collect();
        columns.extend(
            columns_owned
                .iter()
                .map(|values| Vector::Values(values.as_slice())),
        );
        let batch = Batch::new(chunk.len(), columns);
        if downstream.push(&batch)? == Flow::Stop {
            return Ok(Flow::Stop);
        }
    }
    Ok(Flow::Continue)
}

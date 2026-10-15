//! Sorting the entries of a new index: in memory when they fit, in sorted runs when they do not.
//!
//! Invariant: **the entries come out in the order the tree compares them in, whichever way they
//! were sorted, and the peak memory of the sort does not grow with the number of entries.** The
//! in memory sort is `EntrySet::order`, a radix pass over a prefix of the tree's own key encoding
//! with the tree's comparison for what the prefix ties. When the arena passes [`RUN_BYTES`] the
//! entries held are sorted the same way and written to a transient file as one run, and a merge of
//! the runs under the same comparison produces the order. An index over five million rows used to
//! hold every entry in one arena (about 105 bytes each) and peaked at 543 to 616 MB; with runs of
//! 32 MiB the arena never grows past that, and the merge holds one row of each run.
//!
//! ## Why the merge compares values and not the prefix
//!
//! A run is read back as rows, and the prefix is a sort accelerator that is not stored. The merge
//! compares each column with `compare_under`, the comparison the tree's search and its integrity
//! check use, and reverses a descending column. That is the fallback `EntrySet::compare` applies to
//! what the prefix ties, so the two halves of the sort cannot disagree about an order.
//!
//! ## What the tree build gets
//!
//! A sort that stayed in memory is handed to the bulk builder as the `Ordered` view it always was,
//! so a table that fits builds the same tree through the same code as before. A sort that wrote
//! runs is handed over a chunk at a time (`RowChunks`), because the builder's other form needs
//! every row addressable at once.

use std::cmp::Ordering;
use std::rc::Rc;

use inillucent_base::DbResult;
use inillucent_exec::spill::{chunk_rows, run_bytes, RowOrder, RunMerge, RunWriter, Spill};
use inillucent_tree::datum::{Datum, OwnedDatum};
use inillucent_tree::leaf::Rows;
use inillucent_tree::paged::{KeyEncoding, RowChunks};
use inillucent_tree::types::{compare_under, ColumnSpec};
use inillucent_tree::PagedTree;
use inillucent_value::collation::Collation;

use crate::entries::{EntrySet, Ordered};

/// How many bytes of entries are held before they are sorted and written as a run.
///
/// 32 MiB, half of what `Sort` holds, because the radix sort needs the arena and, beside it, a
/// vector of `(prefix word, entry)` pairs twice over: sorting a full arena peaks at about 1.4 times
/// it. A run this size at the measured 105 bytes an entry is about 320,000 entries, so five million
/// rows write sixteen runs and the merge keeps a heap of sixteen.
pub(crate) const RUN_BYTES: u64 = 32 * 1024 * 1024;

/// How many entries are handed to the tree builder at a time when the sort spilled.
const CHUNK_ROWS: usize = 16 * 1024;

/// Collects index entries and sorts them, spilling sorted runs past a byte limit.
pub(crate) struct EntrySorter {
    /// The entries held now.
    set: EntrySet,
    /// How many columns an entry has.
    width: usize,
    /// The key columns' collations.
    collations: Vec<Collation>,
    /// The key columns' directions.
    directions: Vec<bool>,
    /// The runs written so far, when there is somewhere to write them.
    writer: Option<RunWriter>,
    /// How many bytes the arena may hold before a run is written.
    limit: u64,
}

impl EntrySorter {
    /// Returns an empty sorter.
    ///
    /// @param width - how many columns an entry has
    /// @param expected_rows - how many entries are expected, which only sizes the first arena
    /// @param encoding - the index tree's key encoding
    /// @param collations - the key columns' collations, in key order
    /// @param directions - the key columns' directions, in key order
    /// @param spill - where runs are written, or none to hold everything
    pub(crate) fn new(
        width: usize,
        expected_rows: usize,
        encoding: KeyEncoding,
        collations: &[Collation],
        directions: &[bool],
        spill: Option<Rc<dyn Spill>>,
    ) -> EntrySorter {
        let limit = run_bytes(RUN_BYTES);
        // An entry costs its cells, a prefix and some payload. The arena is sized for the rows
        // expected or for the rows a run holds, whichever is fewer, so a table of millions of
        // rows does not reserve a gigabyte for an arena that is written out at 32 MiB.
        let per_entry = width.saturating_mul(16).saturating_add(64).max(1) as u64;
        let capacity = match spill {
            Some(_) => expected_rows.min(usize::try_from(limit / per_entry).unwrap_or(usize::MAX)),
            None => expected_rows,
        };
        EntrySorter {
            set: EntrySet::with_capacity(width, capacity, encoding, collations, directions),
            width,
            collations: collations.to_vec(),
            directions: directions.to_vec(),
            writer: spill.map(RunWriter::new),
            limit,
        }
    }

    /// Adds one entry, writing a run first when the arena has reached its limit.
    ///
    /// @param values - the entry's columns, in tree order
    pub(crate) fn push(&mut self, values: &[Datum<'_>]) -> DbResult<()> {
        self.set.push(values);
        if self.writer.is_some() && self.set.resident_bytes() >= self.limit {
            self.write_a_run()?;
        }
        Ok(())
    }

    /// Sorts the entries held and writes them as one run.
    fn write_a_run(&mut self) -> DbResult<()> {
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };
        if self.set.is_empty() {
            return Ok(());
        }
        let order = self.set.order();
        let bytes = self.set.encode_in_order(&order);
        writer.write_encoded(&bytes, order.len())?;
        self.set.clear();
        Ok(())
    }

    /// How many runs have been written, so a test can say whether the sort spilled at all.
    #[cfg(test)]
    pub(crate) fn runs_written(&self) -> usize {
        self.writer.as_ref().map_or(0, RunWriter::run_count)
    }

    /// Finishes the sort.
    ///
    /// A sort that never wrote a run orders the arena in memory. One that did writes what is held
    /// as a last run, so there is one rule to merge and not two.
    pub(crate) fn finish(mut self) -> DbResult<SortedEntries> {
        let spilled = self
            .writer
            .as_ref()
            .is_some_and(|writer| !writer.is_empty());
        if !spilled {
            let order = self.set.order();
            return Ok(SortedEntries::Memory {
                set: self.set,
                order,
            });
        }
        self.write_a_run()?;
        let Some(writer) = self.writer else {
            return Ok(SortedEntries::Memory {
                order: self.set.order(),
                set: self.set,
            });
        };
        Ok(SortedEntries::External(ExternalEntries {
            writer,
            collations: self.collations,
            directions: self.directions,
            width: self.width,
        }))
    }
}

/// The entries of an index in key order, wherever they are.
pub(crate) enum SortedEntries {
    /// Held in one arena, with the order they are read in.
    Memory {
        /// The arena.
        set: EntrySet,
        /// The entries in key order.
        order: Vec<u32>,
    },
    /// Written to sorted runs and merged on the way out.
    External(ExternalEntries),
}

/// Sorted runs of index entries in a transient file.
pub(crate) struct ExternalEntries {
    /// The runs.
    writer: RunWriter,
    /// The key columns' collations.
    collations: Vec<Collation>,
    /// The key columns' directions.
    directions: Vec<bool>,
    /// How many columns an entry has.
    width: usize,
}

impl SortedEntries {
    /// Reports whether two entries in a row agree on their leading `compared` columns, with no
    /// NULL among them.
    ///
    /// @param compared - how many leading columns the index declares
    pub(crate) fn has_duplicate(&self, compared: usize) -> DbResult<bool> {
        match self {
            SortedEntries::Memory { set, order } => Ok((0..order.len().saturating_sub(1))
                .any(|at| set.shares_key_prefix(order, at, compared))),
            SortedEntries::External(external) => external.has_duplicate(compared),
        }
    }

    /// Reports whether a `UNIQUE` index has two entries with one key, after refusing keys over the
    /// limit.
    ///
    /// **Both checks run before the first page is written.** Entries sorted in memory are checked for
    /// a key over the limit by the builder, which looks at every row before it allocates. Entries
    /// sorted in runs reach the builder a chunk at a time, so they are read once here instead.
    ///
    /// @param unique - whether the index is `UNIQUE`
    /// @param columns - the index tree's column directory
    /// @param key_columns - how many leading columns form the key, the row identity included
    pub(crate) fn has_duplicate_or_long_key(
        &self,
        unique: bool,
        columns: &[ColumnSpec],
        key_columns: usize,
    ) -> DbResult<bool> {
        if unique && self.has_duplicate(key_columns.saturating_sub(1))? {
            return Ok(true);
        }
        self.refuse_long_keys(columns, key_columns)?;
        Ok(false)
    }

    /// Refuses entries whose keys are over the limit, before any page is written.
    ///
    /// The in memory form leaves this to the builder, which makes the same check over the whole
    /// set before it allocates a page. A build of chunks would find a long key in the last chunk
    /// after writing the first ones, so the chunks are read once first.
    ///
    /// @param columns - the index tree's column directory
    /// @param key_columns - how many leading columns form the key
    pub(crate) fn refuse_long_keys(
        &self,
        columns: &[ColumnSpec],
        key_columns: usize,
    ) -> DbResult<()> {
        match self {
            SortedEntries::Memory { .. } => Ok(()),
            SortedEntries::External(external) => {
                let mut chunks = external.chunks()?;
                chunks.for_each_chunk(&mut |rows| {
                    PagedTree::refuse_chunk_keys(columns, key_columns, rows)
                })
            }
        }
    }

    /// Drops what only the sort needed, before the tree is packed.
    pub(crate) fn release_sort_scratch(&mut self) {
        if let SortedEntries::Memory { set, .. } = self {
            set.release_sort_scratch();
        }
    }

    /// Returns the in memory view the leaf builder packs, when the sort stayed in memory.
    pub(crate) fn in_memory(&self) -> Option<Ordered<'_>> {
        match self {
            SortedEntries::Memory { set, order } => Some(set.in_order(order)),
            SortedEntries::External(_) => None,
        }
    }

    /// Returns the chunks the tree builder reads, when the sort wrote runs.
    pub(crate) fn chunks(&self) -> DbResult<Option<EntryChunks<'_>>> {
        match self {
            SortedEntries::Memory { .. } => Ok(None),
            SortedEntries::External(external) => external.chunks().map(Some),
        }
    }
}

impl ExternalEntries {
    /// Returns the order entries are merged in.
    fn order(&self) -> RowOrder<'_> {
        let collations = &self.collations;
        let directions = &self.directions;
        Box::new(move |left, right| entry_order(left, right, collations, directions))
    }

    /// Returns a merge over every run.
    fn merge(&self) -> DbResult<RunMerge<'_>> {
        self.writer.merge(self.order())
    }

    /// Returns the entries as chunks for the tree builder.
    fn chunks(&self) -> DbResult<EntryChunks<'_>> {
        Ok(EntryChunks {
            merge: self.merge()?,
            width: self.width,
        })
    }

    /// Reports whether two entries in a row share their leading `compared` columns.
    ///
    /// The entries come out in key order, so rows with the same indexed columns are adjacent. A
    /// NULL is distinct from every other NULL, so an entry with one in its compared columns is
    /// never a duplicate, which is what `EntrySet::shares_key_prefix` also says.
    ///
    /// @param compared - how many leading columns the index declares
    fn has_duplicate(&self, compared: usize) -> DbResult<bool> {
        let mut merge = self.merge()?;
        let mut previous: Option<Vec<OwnedDatum>> = None;
        while let Some(row) = merge.next_row()? {
            if let Some(held) = previous.as_deref() {
                if shares_key_prefix(held, &row, compared, &self.collations) {
                    return Ok(true);
                }
            }
            previous = Some(row);
        }
        Ok(false)
    }
}

/// Returns the failure a `CREATE UNIQUE INDEX` reports when two rows share a key.
///
/// @param owner - the table, for the message
/// @param index - the index, for the message
pub(crate) fn unique_violation(
    owner: &inillucent_sql::catalog_view::TableInfo,
    index: &inillucent_sql::catalog_view::IndexInfo,
) -> inillucent_base::DbError {
    let columns: Vec<String> = index
        .columns
        .iter()
        .map(|key| {
            let name = key
                .column
                .and_then(|at| owner.column(at))
                .map(|column| String::from_utf8_lossy(&column.name).into_owned())
                .unwrap_or_else(|| "?".to_string());
            format!("{}.{}", String::from_utf8_lossy(&owner.name), name)
        })
        .collect();
    // `SQLITE_CONSTRAINT_UNIQUE` (2067), which is what SQLite reports
    // when the rows already there break the new index; `refusal`
    // answered `SQLITE_MISUSE` (21).
    let said = format!("UNIQUE constraint failed: {}", columns.join(", "));
    inillucent_base::DbError::new(inillucent_base::ExtendedCode(
        inillucent_sql::dml::codes::UNIQUE,
    ))
    .with_message(said.clone())
    .with_detail(said)
}

/// Orders two entries the way the tree compares them: column by column under the key's collations,
/// reversing a descending column.
///
/// @param left - one entry
/// @param right - the other
/// @param collations - the key columns' collations
/// @param directions - the key columns' directions
fn entry_order(
    left: &[OwnedDatum],
    right: &[OwnedDatum],
    collations: &[Collation],
    directions: &[bool],
) -> Ordering {
    for column in 0..left.len().min(right.len()) {
        let (Some(a), Some(b)) = (left.get(column), right.get(column)) else {
            break;
        };
        let order = compare_under(
            &a.borrow(),
            &b.borrow(),
            collations.get(column).copied().unwrap_or(Collation::Binary),
        );
        let order = if directions.get(column).copied().unwrap_or(false) {
            order.reverse()
        } else {
            order
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    Ordering::Equal
}

/// Reports whether two entries agree on their leading columns, none of which is NULL.
///
/// @param left - the earlier entry
/// @param right - the later entry
/// @param compared - how many leading columns to compare
/// @param collations - the key columns' collations
fn shares_key_prefix(
    left: &[OwnedDatum],
    right: &[OwnedDatum],
    compared: usize,
    collations: &[Collation],
) -> bool {
    if (0..compared).any(|column| matches!(left.get(column), Some(OwnedDatum::Null))) {
        return false;
    }
    (0..compared).all(|column| match (left.get(column), right.get(column)) {
        (Some(a), Some(b)) => {
            compare_under(
                &a.borrow(),
                &b.borrow(),
                collations.get(column).copied().unwrap_or(Collation::Binary),
            ) == Ordering::Equal
        }
        _ => false,
    })
}

/// The merged entries, delivered to the tree builder a chunk at a time.
pub(crate) struct EntryChunks<'a> {
    /// The merge of every run.
    merge: RunMerge<'a>,
    /// How many columns an entry has.
    width: usize,
}

/// One chunk of entries, flat, for the tree builder to read.
struct FlatRows<'a> {
    /// `rows * width` values, an entry after another.
    data: &'a [OwnedDatum],
    /// How many columns an entry has.
    width: usize,
    /// How many entries there are.
    rows: usize,
}

impl<'a> Rows<'a> for FlatRows<'a> {
    fn len(&self) -> usize {
        self.rows
    }

    fn value(&self, row: usize, column: usize) -> Datum<'a> {
        self.data
            .get(row.saturating_mul(self.width).saturating_add(column))
            .map_or(Datum::Null, OwnedDatum::borrow)
    }
}

impl RowChunks for EntryChunks<'_> {
    fn for_each_chunk(
        &mut self,
        each: &mut dyn for<'d> FnMut(&dyn Rows<'d>) -> DbResult<()>,
    ) -> DbResult<()> {
        let per_chunk = chunk_rows(CHUNK_ROWS);
        loop {
            let mut flat: Vec<OwnedDatum> =
                Vec::with_capacity(per_chunk.saturating_mul(self.width));
            let mut rows = 0usize;
            while rows < per_chunk {
                let Some(mut entry) = self.merge.next_row()? else {
                    break;
                };
                // An entry is exactly `width` values; a run is written from entries of that
                // width, so this only guards the arithmetic the reader above relies on.
                entry.resize(self.width, OwnedDatum::Null);
                flat.append(&mut entry);
                rows = rows.saturating_add(1);
            }
            if rows == 0 {
                return Ok(());
            }
            let chunk = FlatRows {
                data: &flat,
                width: self.width,
                rows,
            };
            each(&chunk)?;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spillfile::VfsSpill;

    /// Returns a spill factory over a fresh memory file system.
    fn spill() -> Option<Rc<dyn Spill>> {
        Some(Rc::new(VfsSpill::new(std::sync::Arc::new(
            inillucent_vfs::memory::MemoryVfs::new(),
        ))))
    }

    /// Returns entries of a text, an integer and a row identity: the text shares a long prefix and
    /// differs in case, the integer repeats and is sometimes NULL, and the identity is unique.
    ///
    /// @param count - how many entries
    fn entries(count: i64) -> Vec<(Vec<u8>, Option<i64>, i64)> {
        (0..count)
            .map(|n| {
                let word = format!(
                    "{}shared-prefix-of-thirty-two-bytes-{}",
                    if n % 2 == 0 { "Key" } else { "key" },
                    n.wrapping_mul(7919) % 4001
                );
                let number = if n % 11 == 0 {
                    None
                } else {
                    Some(n.wrapping_mul(31) % 97)
                };
                (word.into_bytes(), number, n)
            })
            .collect()
    }

    /// Sorts the entries with a sorter that writes a run past `limit` bytes, and returns what came
    /// out together with how many runs were written.
    ///
    /// @param rows - the entries
    /// @param limit - how many bytes the sorter holds before it writes a run
    fn sorted(rows: &[(Vec<u8>, Option<i64>, i64)], limit: u64) -> (Vec<Vec<OwnedDatum>>, usize) {
        let collations = [Collation::NoCase, Collation::Binary, Collation::Binary];
        let directions = [false, true, false];
        let mut sorter = EntrySorter::new(
            3,
            rows.len(),
            KeyEncoding::General,
            &collations,
            &directions,
            spill(),
        );
        sorter.limit = limit;
        for (word, number, identity) in rows {
            let values = [
                Datum::Text(word),
                number.map_or(Datum::Null, Datum::Int),
                Datum::Int(*identity),
            ];
            sorter.push(&values).expect("an entry is pushed");
        }
        let runs = sorter.runs_written();
        let sorted = sorter.finish().expect("the sort finishes");
        let mut out: Vec<Vec<OwnedDatum>> = Vec::new();
        if let Some(view) = sorted.in_memory() {
            for row in 0..view.len() {
                out.push(
                    (0..3)
                        .map(|c| OwnedDatum::from_datum(&view.value(row, c)))
                        .collect(),
                );
            }
        } else if let Some(mut chunks) = sorted.chunks().expect("chunks open") {
            chunks
                .for_each_chunk(&mut |rows| {
                    for row in 0..rows.len() {
                        out.push(
                            (0..3)
                                .map(|c| OwnedDatum::from_datum(&rows.value(row, c)))
                                .collect(),
                        );
                    }
                    Ok(())
                })
                .expect("the chunks are read");
        }
        (out, runs)
    }

    /// **Entries sorted in many runs come out in the order a sort in memory gives.**
    ///
    /// The comparison covers a collation, a descending column, NULLs and a long shared prefix, which
    /// is every way the merge's value comparison can differ from the radix sort's.
    #[test]
    fn runs_merge_into_the_order_a_memory_sort_gives() {
        let rows = entries(6000);
        let (in_memory, none) = sorted(&rows, u64::MAX);
        let (merged, runs) = sorted(&rows, 24 * 1024);
        assert_eq!(none, 0, "the reference sort was meant to stay in memory");
        assert!(runs >= 5, "the sort wrote {runs} runs, so it did not merge");
        assert_eq!(in_memory.len(), rows.len());
        assert_eq!(
            merged, in_memory,
            "the merged order is not the memory order"
        );
    }

    /// The duplicate check over runs agrees with the one over the arena, including for NULLs.
    #[test]
    fn duplicates_are_found_across_runs_and_nulls_are_not_duplicates() {
        let mut rows = entries(3000);
        // Two entries that agree on the first two columns and differ in identity.
        rows.push((rows[10].0.clone(), rows[10].1, 99_999));
        let collations = [Collation::NoCase, Collation::Binary, Collation::Binary];
        let directions = [false, false, false];
        for (limit, expect_runs) in [(u64::MAX, false), (16 * 1024, true)] {
            let mut sorter = EntrySorter::new(
                3,
                rows.len(),
                KeyEncoding::General,
                &collations,
                &directions,
                spill(),
            );
            sorter.limit = limit;
            for (word, number, identity) in &rows {
                let values = [
                    Datum::Text(word),
                    number.map_or(Datum::Null, Datum::Int),
                    Datum::Int(*identity),
                ];
                sorter.push(&values).expect("an entry is pushed");
            }
            assert_eq!(sorter.runs_written() > 0, expect_runs);
            let sorted = sorter.finish().expect("the sort finishes");
            // The first row's number is NULL only when n % 11 == 0; row 10 is not one.
            assert!(
                sorted.has_duplicate(2).expect("the check runs"),
                "a duplicate was missed (limit {limit})"
            );
        }
        // Entries that repeat only in a NULL column are not duplicates.
        let nulls: Vec<(Vec<u8>, Option<i64>, i64)> =
            (0..2000).map(|n| (b"same".to_vec(), None, n)).collect();
        let mut sorter = EntrySorter::new(
            3,
            nulls.len(),
            KeyEncoding::General,
            &collations,
            &directions,
            spill(),
        );
        sorter.limit = 8 * 1024;
        for (word, number, identity) in &nulls {
            let values = [
                Datum::Text(word),
                number.map_or(Datum::Null, Datum::Int),
                Datum::Int(*identity),
            ];
            sorter.push(&values).expect("an entry is pushed");
        }
        assert!(sorter.runs_written() > 0);
        let sorted = sorter.finish().expect("the sort finishes");
        assert!(
            !sorted.has_duplicate(2).expect("the check runs"),
            "NULLs were counted as duplicates"
        );
    }
}

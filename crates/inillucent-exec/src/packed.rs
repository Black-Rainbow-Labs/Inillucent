//! Many rows held in a few large buffers, for a statement that reads every row before it writes
//! any.
//!
//! Invariant: **a row read back is the row that was put in, value for value.** A text or a blob
//! is kept as its bytes in a shared buffer and read back as a slice of it; every other value is
//! kept as it is.
//!
//! ## Why
//!
//! `INSERT INTO copied SELECT id, key, label FROM main_table` collected its 100,000 rows as
//! one vector per row and one more for each text value, built each row's image, and freed them
//! all at the end of the statement: 200,000 allocations and 200,000 frees in a statement
//! SQLite answers in 41 ms. A profile put the system heap, its page faults and the frees at
//! about two fifths of the statement (task-2226).
//!
//! ## Why in chunks
//!
//! The first version kept the cells in one vector and the bytes in another, and growing them
//! was a third of the same statement: each doubling copies everything held so far into new
//! memory, which the system then faults in page by page. A chunk is never moved once made, so
//! growth costs one allocation per chunk and no copy.

use inillucent_tree::datum::{Datum, OwnedDatum};

/// How many cells one chunk of cells holds: 256 KiB of them.
const CELLS_PER_CHUNK: usize = 16_384;

/// How many bytes one chunk of text and blob bytes holds. A longer value gets a chunk of its own.
const BYTES_PER_CHUNK: usize = 256 << 10;

/// One value of a packed row.
///
/// A text or a blob names the chunk of bytes it lies in, where in it, and how long it is.
#[derive(Clone, Copy, Debug)]
enum Cell {
    /// SQL NULL.
    Null,
    /// An integer.
    Int(i64),
    /// A real.
    Real(f64),
    /// Text: its chunk, where it starts in the chunk, and its length.
    Text(u32, u32, u32),
    /// A blob: its chunk, where it starts in the chunk, and its length.
    Blob(u32, u32, u32),
}

/// Rows of one width, each value a cell and every text and blob in shared chunks of bytes.
#[derive(Debug, Default)]
pub struct PackedRows {
    /// How many values a row has, set by the first row put in.
    width: usize,
    /// How many rows there are, which a width of zero cannot tell from the cells.
    rows: usize,
    /// Every row's values, one row after another, in chunks of [`CELLS_PER_CHUNK`].
    cells: Vec<Vec<Cell>>,
    /// The bytes of every text and blob.
    bytes: Vec<Vec<u8>>,
}

impl PackedRows {
    /// Returns an empty set of rows.
    pub fn new() -> PackedRows {
        PackedRows::default()
    }

    /// Returns how many rows there are.
    pub fn len(&self) -> usize {
        self.rows
    }

    /// Reports whether there are none.
    pub fn is_empty(&self) -> bool {
        self.rows == 0
    }

    /// Returns how many values a row has.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Returns roughly how many bytes the rows occupy in memory.
    ///
    /// The chunks' capacities, which is what the allocator has been asked for, so a limit that
    /// moves rows to a file counts what the process holds.
    pub fn resident_bytes(&self) -> u64 {
        let cells: usize = self
            .cells
            .iter()
            .map(|chunk| chunk.capacity().saturating_mul(std::mem::size_of::<Cell>()))
            .fold(0usize, usize::saturating_add);
        let bytes: usize = self
            .bytes
            .iter()
            .map(Vec::capacity)
            .fold(0usize, usize::saturating_add);
        cells.saturating_add(bytes) as u64
    }

    /// Appends one cell.
    ///
    /// @param cell - the cell
    fn push_cell(&mut self, cell: Cell) {
        match self.cells.last_mut() {
            Some(chunk) if chunk.len() < CELLS_PER_CHUNK => chunk.push(cell),
            _ => {
                let mut chunk = Vec::with_capacity(CELLS_PER_CHUNK);
                chunk.push(cell);
                self.cells.push(chunk);
            }
        }
    }

    /// Copies bytes into a chunk with room for them, and says where they went.
    ///
    /// `None` when there are more chunks or bytes than a cell can name, which no statement that
    /// fits in memory reaches.
    ///
    /// @param value - the bytes
    fn place(&mut self, value: &[u8]) -> Option<(u32, u32, u32)> {
        let fits = self
            .bytes
            .last()
            .is_some_and(|chunk| chunk.capacity().saturating_sub(chunk.len()) >= value.len());
        if !fits {
            self.bytes
                .push(Vec::with_capacity(BYTES_PER_CHUNK.max(value.len())));
        }
        let chunk = u32::try_from(self.bytes.len().checked_sub(1)?).ok()?;
        let held = self.bytes.last_mut()?;
        let at = u32::try_from(held.len()).ok()?;
        let length = u32::try_from(value.len()).ok()?;
        held.extend_from_slice(value);
        Some((chunk, at, length))
    }

    /// Makes a cell of one value, copying a text or a blob into the bytes.
    ///
    /// @param value - the value
    fn cell_of(&mut self, value: &Datum<'_>) -> Option<Cell> {
        Some(match value {
            Datum::Null => Cell::Null,
            Datum::Int(number) => Cell::Int(*number),
            Datum::Real(number) => Cell::Real(*number),
            Datum::Text(bytes) => {
                let (chunk, at, length) = self.place(bytes)?;
                Cell::Text(chunk, at, length)
            }
            Datum::Blob(bytes) => {
                let (chunk, at, length) = self.place(bytes)?;
                Cell::Blob(chunk, at, length)
            }
        })
    }

    /// Appends one row.
    ///
    /// The first row sets the width. A later row of another width is refused with `false` and
    /// nothing appended, which a caller holding rows of one statement never sees; so is a row
    /// holding more bytes than a cell can name.
    ///
    /// @param values - the row's values, in column order
    pub fn push<'v>(&mut self, values: impl ExactSizeIterator<Item = Datum<'v>>) -> bool {
        if self.rows == 0 {
            self.width = values.len();
        } else if values.len() != self.width {
            return false;
        }
        for value in values {
            let Some(cell) = self.cell_of(&value) else {
                return false;
            };
            self.push_cell(cell);
        }
        self.rows = self.rows.saturating_add(1);
        true
    }

    /// Appends one row of owned values.
    ///
    /// @param values - the row's values, in column order
    pub fn push_owned(&mut self, values: &[OwnedDatum]) -> bool {
        self.push(values.iter().map(OwnedDatum::borrow))
    }

    /// Returns one cell, by its place among all the cells.
    ///
    /// @param at - the place
    fn cell(&self, at: usize) -> Option<&Cell> {
        self.cells
            .get(at / CELLS_PER_CHUNK)
            .and_then(|chunk| chunk.get(at % CELLS_PER_CHUNK))
    }

    /// Returns one value, or NULL past the end.
    ///
    /// @param row - which row
    /// @param column - which value of it
    pub fn value(&self, row: usize, column: usize) -> Datum<'_> {
        if column >= self.width {
            return Datum::Null;
        }
        let at = row.saturating_mul(self.width).saturating_add(column);
        match self.cell(at) {
            None | Some(Cell::Null) => Datum::Null,
            Some(Cell::Int(number)) => Datum::Int(*number),
            Some(Cell::Real(number)) => Datum::Real(*number),
            Some(Cell::Text(chunk, start, length)) => {
                Datum::Text(self.slice(*chunk, *start, *length))
            }
            Some(Cell::Blob(chunk, start, length)) => {
                Datum::Blob(self.slice(*chunk, *start, *length))
            }
        }
    }

    /// Returns bytes of a chunk, or none when the cell names bytes it does not hold.
    ///
    /// @param chunk - which chunk
    /// @param start - where they start in it
    /// @param length - how many
    fn slice(&self, chunk: u32, start: u32, length: u32) -> &[u8] {
        let start = start as usize;
        self.bytes
            .get(chunk as usize)
            .and_then(|held| held.get(start..start.saturating_add(length as usize)))
            .unwrap_or(&[])
    }

    /// Returns one row as owned values.
    ///
    /// @param row - which row
    pub fn owned_row(&self, row: usize) -> Vec<OwnedDatum> {
        (0..self.width)
            .map(|column| OwnedDatum::from_datum(&self.value(row, column)))
            .collect()
    }

    /// Returns every row as owned values, for a caller that takes rows one vector each.
    pub fn into_rows(self) -> Vec<Vec<OwnedDatum>> {
        (0..self.rows).map(|row| self.owned_row(row)).collect()
    }

    /// Puts the rows in a new order.
    ///
    /// The bytes do not move: each cell still names its own.
    ///
    /// @param order - the rows' old positions, in the order wanted; a position past the end is
    ///   skipped
    pub fn reorder(&mut self, order: &[usize]) {
        let mut moved = PackedRows {
            width: self.width,
            rows: 0,
            cells: Vec::new(),
            bytes: Vec::new(),
        };
        for row in order {
            if *row >= self.rows {
                continue;
            }
            let at = row.saturating_mul(self.width);
            for column in 0..self.width {
                if let Some(cell) = self.cell(at.saturating_add(column)) {
                    moved.push_cell(*cell);
                }
            }
            moved.rows = moved.rows.saturating_add(1);
        }
        self.cells = moved.cells;
        self.rows = moved.rows;
    }
}

impl<'r> inillucent_tree::leaf::Rows<'r> for &'r PackedRows {
    fn len(&self) -> usize {
        self.rows
    }

    fn value(&self, row: usize, column: usize) -> Datum<'r> {
        PackedRows::value(self, row, column)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every kind of value comes back as it went in, in its row and column.
    #[test]
    fn a_row_comes_back_as_it_went_in() {
        let mut packed = PackedRows::new();
        let first = [
            OwnedDatum::Int(7),
            OwnedDatum::Text(b"seven".to_vec()),
            OwnedDatum::Null,
            OwnedDatum::Real(0.5),
            OwnedDatum::Blob(vec![0, 1, 2]),
        ];
        let second = [
            OwnedDatum::Int(-3),
            OwnedDatum::Text(Vec::new()),
            OwnedDatum::Blob(Vec::new()),
            OwnedDatum::Real(-2.25),
            OwnedDatum::Text(b"x".to_vec()),
        ];
        assert!(packed.push_owned(&first));
        assert!(packed.push_owned(&second));
        assert_eq!(packed.len(), 2);
        assert_eq!(packed.owned_row(0), first.to_vec());
        assert_eq!(packed.owned_row(1), second.to_vec());
        let owned = |row: usize, column: usize| OwnedDatum::from_datum(&packed.value(row, column));
        assert_eq!(owned(1, 1), OwnedDatum::Text(Vec::new()));
        assert_eq!(owned(2, 0), OwnedDatum::Null);
        assert_eq!(owned(0, 5), OwnedDatum::Null);
        // A row of another width is refused and nothing is appended.
        assert!(!packed.push_owned(&[OwnedDatum::Int(1)]));
        assert_eq!(packed.len(), 2);
        assert_eq!(packed.into_rows(), vec![first.to_vec(), second.to_vec()]);
    }

    /// Reordering moves whole rows, and each text stays with its row.
    #[test]
    fn reordering_moves_whole_rows() {
        let mut packed = PackedRows::new();
        for (key, label) in [(3, "c"), (1, "a"), (2, "bb")] {
            let row = [
                OwnedDatum::Int(key),
                OwnedDatum::Text(label.as_bytes().to_vec()),
            ];
            assert!(packed.push_owned(&row));
        }
        packed.reorder(&[1, 2, 0]);
        let rows: Vec<Vec<OwnedDatum>> =
            (0..packed.len()).map(|row| packed.owned_row(row)).collect();
        let wanted: Vec<Vec<OwnedDatum>> = [(1, "a"), (2, "bb"), (3, "c")]
            .iter()
            .map(|(key, label)| {
                vec![
                    OwnedDatum::Int(*key),
                    OwnedDatum::Text(label.as_bytes().to_vec()),
                ]
            })
            .collect();
        assert_eq!(rows, wanted);
    }

    /// Rows past the first chunk of cells, and texts past the first chunk of bytes, including
    /// one longer than a chunk, come back as they went in.
    #[test]
    fn rows_across_chunks_come_back_as_they_went_in() {
        let long = vec![b'z'; BYTES_PER_CHUNK + 7];
        let row_of = |key: usize| -> Vec<OwnedDatum> {
            let label = match key {
                5_000 => long.clone(),
                _ => format!("label {key}").into_bytes(),
            };
            vec![
                OwnedDatum::Int(key as i64),
                OwnedDatum::Text(label),
                OwnedDatum::Real(key as f64 / 2.0),
            ]
        };
        let count = CELLS_PER_CHUNK * 2 + 11;
        let mut packed = PackedRows::new();
        for key in 0..count {
            assert!(packed.push_owned(&row_of(key)));
        }
        assert!(
            packed.cells.len() >= 6,
            "{} chunks of cells",
            packed.cells.len()
        );
        assert!(
            packed.bytes.len() >= 3,
            "{} chunks of bytes",
            packed.bytes.len()
        );
        for key in [0, 1, 5_000, CELLS_PER_CHUNK / 3, CELLS_PER_CHUNK, count - 1] {
            assert_eq!(packed.owned_row(key), row_of(key), "row {key}");
        }
        let order: Vec<usize> = (0..count).rev().collect();
        packed.reorder(&order);
        assert_eq!(packed.len(), count);
        assert_eq!(packed.owned_row(0), row_of(count - 1));
        assert_eq!(packed.owned_row(count - 1 - 5_000), row_of(5_000));
    }
}

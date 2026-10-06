//! The pool's per-frame tables, built a chunk at a time as frames are used.
//!
//! Invariant: **an entry, once built, never moves.** A frame's latch, pin
//! counter and buffer are reached through `&self` while other frames are being
//! claimed, so the storage behind them cannot be a vector that reallocates.
//! Each chunk is a boxed slice set once, so growing the table adds chunks and
//! never touches one that exists.
//!
//! **Why the tables are not built in full at open** (task-2191). A pool of
//! 4,096 frames used to build every latch, pin counter, buffer slot, frame
//! record and free list entry when it was created: about 600 KB of fresh
//! memory, written once, at every open. That was 145 us of a 1.2 ms open on
//! Windows, most of it page faults, and more than SQLite's whole open of the
//! same file. A connection that reads a few pages now builds one chunk.

use std::cell::OnceCell;

/// How many entries a chunk holds.
const CHUNK: usize = 256;

/// A fixed number of entries, each built the first time its chunk is reached.
pub(crate) struct Chunked<T> {
    /// How many entries the table holds.
    len: usize,
    /// The chunks, each built on first use.
    chunks: Vec<OnceCell<Box<[T]>>>,
    /// Builds one entry.
    make: fn() -> T,
}

impl<T> Chunked<T> {
    /// Makes a table of `len` entries, none of them built yet.
    ///
    /// @param len - how many entries
    /// @param make - builds one entry
    pub(crate) fn new(len: usize, make: fn() -> T) -> Chunked<T> {
        Chunked {
            len,
            chunks: (0..len.div_ceil(CHUNK)).map(|_| OnceCell::new()).collect(),
            make,
        }
    }

    /// Returns how many entries the table holds, built or not.
    pub(crate) fn len(&self) -> usize {
        self.len
    }

    /// Returns one entry, building its chunk the first time it is reached.
    ///
    /// @param index - the entry
    pub(crate) fn get(&self, index: usize) -> Option<&T> {
        if index >= self.len {
            return None;
        }
        let chunk = self
            .chunks
            .get(index / CHUNK)?
            .get_or_init(|| (0..CHUNK).map(|_| (self.make)()).collect());
        chunk.get(index % CHUNK)
    }

    /// Grows the table to `len` entries, leaving every built entry where it is.
    ///
    /// @param len - how many entries the table should hold
    pub(crate) fn grow(&mut self, len: usize) -> Result<(), std::collections::TryReserveError> {
        if len <= self.len {
            return Ok(());
        }
        let wanted = len.div_ceil(CHUNK);
        self.chunks
            .try_reserve(wanted.saturating_sub(self.chunks.len()))?;
        while self.chunks.len() < wanted {
            self.chunks.push(OnceCell::new());
        }
        self.len = len;
        Ok(())
    }
}

/// The frames holding nothing: the ones given back, and the ones never used.
///
/// The second kind is a counter rather than a list, so a pool of 4,096 frames
/// does not write 4,096 indices at open. Frames are handed out lowest index
/// first, as the list this replaces did.
#[derive(Debug)]
pub(crate) struct FreeFrames {
    /// Frames that held a page and were given back, most recent last.
    returned: Vec<u32>,
    /// The lowest frame never handed out.
    fresh: u32,
    /// How many frames the pool holds.
    limit: u32,
}

impl FreeFrames {
    /// Makes the free set of a pool of `limit` frames, all of them unused.
    ///
    /// @param limit - how many frames
    pub(crate) fn new(limit: u32) -> FreeFrames {
        FreeFrames {
            returned: Vec::new(),
            fresh: 0,
            limit,
        }
    }

    /// Takes a free frame: one given back if there is one, the next unused
    /// frame otherwise.
    pub(crate) fn pop(&mut self) -> Option<u32> {
        if let Some(frame) = self.returned.pop() {
            return Some(frame);
        }
        if self.fresh < self.limit {
            let frame = self.fresh;
            self.fresh = self.fresh.saturating_add(1);
            return Some(frame);
        }
        None
    }

    /// Gives a frame back.
    ///
    /// @param frame - the frame, which holds nothing now
    pub(crate) fn push(&mut self, frame: u32) {
        self.returned.push(frame);
    }

    /// Returns how many frames hold nothing.
    pub(crate) fn len(&self) -> usize {
        self.returned
            .len()
            .saturating_add(self.limit.saturating_sub(self.fresh) as usize)
    }

    /// Adds frames to the pool, all of them unused.
    ///
    /// @param limit - how many frames the pool holds now
    pub(crate) fn grow(&mut self, limit: u32) {
        self.limit = self.limit.max(limit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_entry_is_built_once_and_stays_where_it_is() {
        let mut table: Chunked<std::cell::Cell<u32>> =
            Chunked::new(300, || std::cell::Cell::new(7));
        let first = table.get(299).unwrap() as *const _;
        table.get(299).unwrap().set(9);
        assert!(table.get(300).is_none());
        table.grow(1_000).unwrap();
        assert_eq!(table.get(299).unwrap() as *const _, first);
        assert_eq!(table.get(299).unwrap().get(), 9);
        assert_eq!(table.get(999).unwrap().get(), 7);
        assert_eq!(table.len(), 1_000);
    }

    #[test]
    fn free_frames_go_lowest_first_and_given_back_ones_first() {
        let mut free = FreeFrames::new(3);
        assert_eq!(free.len(), 3);
        assert_eq!(free.pop(), Some(0));
        assert_eq!(free.pop(), Some(1));
        free.push(0);
        assert_eq!(free.pop(), Some(0));
        assert_eq!(free.pop(), Some(2));
        assert_eq!(free.pop(), None);
        assert_eq!(free.len(), 0);
        free.grow(4);
        assert_eq!(free.pop(), Some(3));
    }
}

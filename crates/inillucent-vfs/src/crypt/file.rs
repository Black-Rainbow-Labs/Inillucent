//! An open encrypted file: the plaintext view the engine reads and writes.
//!
//! Invariant: the logical bytes this file reports are exactly the bytes
//! written to it, at the offsets they were written at, and a read past the
//! logical end zero fills and reports a short read, as the operating system's
//! file does. Nothing above this module can tell the file is encrypted except
//! by timing it.
//!
//! **A double slot unit is rewritten into the slot that does not hold the
//! newest copy**, unless this handle wrote that newest copy itself and has not
//! synced since. So a crash in the middle of a rewrite leaves the last synced
//! copy readable. That is the property a log needs: a later append that shares
//! a unit with an earlier synced record must not be able to destroy it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use inillucent_base::crypt::SecretKey;

use super::unit::{self, Found, Layout};
use super::Shared;
use crate::contract::{
    DeviceCharacteristics, FileIdentity, FileLock, SharedMemory, SyncMode, VfsFile,
};
use crate::error::{self, VfsError, VfsOperation, VfsResult};

/// What one unit holds now, after choosing between its copies.
#[derive(Clone, Debug)]
struct Current {
    /// The whole unit's plaintext. Zeros for a hole, random bytes for damage.
    plain: Vec<u8>,
    /// Plaintext bytes that belong to the file.
    length: u32,
    /// The generation of the copy that was chosen, zero when none was.
    generation: u64,
    /// Which copy was chosen, when one was whole.
    slot: Option<u64>,
}

/// An open file whose contents are encrypted in units.
pub struct CryptFile {
    /// The file underneath, holding ciphertext.
    inner: Box<dyn VfsFile>,
    /// The data key.
    key: SecretKey,
    /// The random id every unit's associated data names.
    file_id: [u8; 16],
    /// How units are placed.
    layout: Layout,
    /// Where the first unit starts: after the header, or zero.
    base: u64,
    /// Nonces, randomness and counters shared by every file of one `CryptVfs`.
    shared: Arc<Shared>,
    /// Whether every write is refused.
    read_only: bool,
    /// Units this handle rewrote since its last sync: the slot and generation.
    unsynced: Mutex<HashMap<u64, (u64, u64)>>,
    /// Held across a read, modify and write so two writers cannot interleave.
    writing: Mutex<()>,
}

impl std::fmt::Debug for CryptFile {
    /// Names the layout and nothing secret.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("CryptFile")
            .field("layout", &self.layout)
            .field("read_only", &self.read_only)
            .finish()
    }
}

/// The base a handle is given when it must read as an empty file whatever
/// the file underneath holds: a read only open of a file with no usable
/// header. Every unit starts past the end of any real file, so none is present.
pub(crate) const NOTHING: u64 = u64::MAX;

/// Converts a length that is known to fit into a `usize`.
///
/// @param value - the length
fn size(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

impl CryptFile {
    /// Wraps an opened file.
    ///
    /// @param inner - the file holding ciphertext
    /// @param key - the data key
    /// @param file_id - the id from the file's header
    /// @param layout - how units are placed
    /// @param base - where the first unit starts
    /// @param shared - the owning file system's shared state
    /// @param read_only - whether writes are refused
    pub(crate) fn new(
        inner: Box<dyn VfsFile>,
        key: SecretKey,
        file_id: [u8; 16],
        layout: Layout,
        base: u64,
        shared: Arc<Shared>,
        read_only: bool,
    ) -> CryptFile {
        CryptFile {
            inner,
            key,
            file_id,
            layout,
            base,
            shared,
            read_only,
            unsynced: Mutex::new(HashMap::new()),
            writing: Mutex::new(()),
        }
    }

    /// Plaintext bytes per unit.
    fn unit(&self) -> u64 {
        u64::from(self.layout.unit())
    }

    /// Where unit `index` starts on disk.
    ///
    /// @param index - the unit number
    fn physical(&self, index: u64) -> u64 {
        self.base
            .saturating_add(index.saturating_mul(self.layout.stride()))
    }

    /// How many units have any bytes on disk.
    fn units_present(&self) -> VfsResult<u64> {
        let length = self.inner.file_size()?;
        let body = length.saturating_sub(self.base);
        Ok(body.div_ceil(self.layout.stride()))
    }

    /// Reads the raw bytes of units `first..first + count`, zero filling past
    /// the end of the file.
    ///
    /// @param first - the first unit
    /// @param count - how many units
    fn raw(&self, first: u64, count: u64) -> VfsResult<Vec<u8>> {
        let mut bytes = vec![0u8; size(count.saturating_mul(self.layout.stride()))];
        match self.inner.read_exact_at(self.physical(first), &mut bytes) {
            Ok(()) => Ok(bytes),
            Err(failure) if failure.extended() == error::short_read("").extended() => Ok(bytes),
            Err(failure) => Err(failure),
        }
    }

    /// Chooses what a unit holds from its raw bytes.
    ///
    /// @param index - the unit number
    /// @param raw - the unit's `stride` bytes
    fn decide(&self, index: u64, raw: &[u8]) -> Current {
        let unit = size(self.unit());
        let slot_bytes = self.layout.slot_bytes();
        let mut best: Option<(Vec<u8>, unit::Trailer, u64)> = None;
        let mut damaged = false;
        for slot in 0..self.layout.slots() {
            let start = size(slot.saturating_mul(unit::SMALL_SLOT));
            let copy = raw
                .get(start..start.saturating_add(slot_bytes))
                .unwrap_or(&[]);
            match unit::open(self.key.bytes(), &self.file_id, index, unit, copy) {
                Found::Whole(plain, trailer) => {
                    if best
                        .as_ref()
                        .is_none_or(|(_, held, _)| trailer.generation > held.generation)
                    {
                        best = Some((plain, trailer, slot));
                    }
                }
                Found::Hole => {}
                Found::Damaged => damaged = true,
            }
        }
        match best {
            Some((plain, trailer, slot)) => Current {
                plain,
                length: trailer.length,
                generation: trailer.generation,
                slot: Some(slot),
            },
            None if damaged => {
                self.shared.count_failure();
                Current {
                    plain: self.shared.filler(unit),
                    length: self.layout.unit(),
                    generation: 0,
                    slot: None,
                }
            }
            None => Current {
                plain: vec![0u8; unit],
                length: 0,
                generation: 0,
                slot: None,
            },
        }
    }

    /// Returns what one unit holds now.
    ///
    /// @param index - the unit number
    fn load(&self, index: u64) -> VfsResult<Current> {
        let raw = self.raw(index, 1)?;
        Ok(self.decide(index, &raw))
    }

    /// Picks the slot a rewrite of a unit goes to.
    ///
    /// @param index - the unit number
    /// @param current - what the unit holds now
    fn target(&self, index: u64, current: &Current) -> u64 {
        if self.layout.slots() == 1 {
            return 0;
        }
        let mine = self
            .unsynced
            .lock()
            .ok()
            .and_then(|held| held.get(&index).copied());
        match (current.slot, mine) {
            (Some(newest), Some((slot, generation)))
                if newest == slot && generation == current.generation =>
            {
                slot
            }
            (Some(newest), _) => 1u64.saturating_sub(newest),
            (None, Some((slot, _))) => slot,
            (None, None) => 0,
        }
    }

    /// Encrypts one unit's new plaintext and returns the bytes and offset of
    /// the write that stores it.
    ///
    /// @param index - the unit number
    /// @param plain - the whole unit's plaintext
    /// @param length - plaintext bytes that belong to the file
    /// @param current - what the unit held, for the slot and generation
    /// @param fresh - whether the unit has no bytes on disk yet
    fn sealed(
        &self,
        index: u64,
        mut plain: Vec<u8>,
        length: u32,
        current: &Current,
        fresh: bool,
    ) -> (u64, Vec<u8>) {
        let slot = self.target(index, current);
        let generation = current.generation.saturating_add(1);
        let trailer = unit::seal(
            self.key.bytes(),
            &self.file_id,
            index,
            generation,
            length,
            self.shared.nonce(),
            &mut plain,
        );
        plain.extend_from_slice(&trailer);
        if self.layout.slots() == 2 {
            if let Ok(mut held) = self.unsynced.lock() {
                held.insert(index, (slot, generation));
            }
        }
        let offset = self.physical(index);
        match (fresh, self.layout) {
            // A new unit is written whole, both slots, so the write is one run
            // with its neighbours and the unused slot is known to be empty.
            (true, Layout::Double) => {
                plain.resize(size(self.layout.stride()), 0);
                (offset, plain)
            }
            (_, Layout::Double) => (
                offset.saturating_add(slot.saturating_mul(unit::SMALL_SLOT)),
                plain,
            ),
            (_, Layout::Single { footprint, .. }) => {
                if fresh {
                    plain.resize(size(footprint), 0);
                }
                (offset, plain)
            }
        }
    }

    /// Writes the queued runs, joining neighbours into one call each.
    ///
    /// @param runs - offsets and bytes, in ascending offset order
    fn flush(&self, runs: Vec<(u64, Vec<u8>)>) -> VfsResult<()> {
        let mut pending: Option<(u64, Vec<u8>)> = None;
        for (offset, bytes) in runs {
            match pending.as_mut() {
                Some((start, joined)) if start.saturating_add(joined.len() as u64) == offset => {
                    joined.extend_from_slice(&bytes);
                }
                _ => {
                    if let Some((start, joined)) = pending.take() {
                        self.inner.write_all_at(start, &joined)?;
                    }
                    pending = Some((offset, bytes));
                }
            }
        }
        if let Some((start, joined)) = pending {
            self.inner.write_all_at(start, &joined)?;
        }
        Ok(())
    }

    /// Writes `input` at logical `offset`, unit by unit.
    ///
    /// @param offset - where the bytes go
    /// @param input - the bytes
    fn write_units(&self, offset: u64, input: &[u8]) -> VfsResult<()> {
        let unit = self.unit();
        let end = offset.saturating_add(input.len() as u64);
        let present = self.units_present()?;
        let mut runs = Vec::new();
        let mut index = offset / unit;
        while index.saturating_mul(unit) < end {
            let start = index.saturating_mul(unit);
            let from = offset.max(start).saturating_sub(start);
            let to = end.min(start.saturating_add(unit)).saturating_sub(start);
            let fresh = index >= present;
            let whole = from == 0 && to == unit;
            let mut current = match (fresh, whole, self.layout) {
                (true, _, _) | (false, true, Layout::Single { .. }) => Current {
                    plain: vec![0u8; size(unit)],
                    length: 0,
                    generation: 0,
                    slot: None,
                },
                _ => self.load(index)?,
            };
            let mut plain = std::mem::take(&mut current.plain);
            let source_from = size(start.saturating_add(from).saturating_sub(offset));
            let source = input
                .get(source_from..source_from.saturating_add(size(to.saturating_sub(from))))
                .unwrap_or(&[]);
            if let Some(target) = plain.get_mut(size(from)..size(to)) {
                target.copy_from_slice(source);
            }
            let length = current.length.max(u32::try_from(to).unwrap_or(u32::MAX));
            runs.push(self.sealed(index, plain, length, &current, fresh));
            index = index.saturating_add(1);
        }
        self.flush(runs)
    }

    /// Returns the file's logical length.
    ///
    /// **A database file answers from its last trailer when it can.** The
    /// pool asks for the length once for every page a checkpoint journals, and
    /// decrypting a whole page to read one number made that a page decrypt per
    /// page written. The engine writes whole pages, so a database file's last
    /// unit is full, and a trailer that says so is the answer: a unit whose
    /// tag then fails still counts as a full unit, which is what the slow path
    /// below answers for a damaged unit too. Any other trailer takes the slow
    /// path, so the two can only agree.
    fn logical_size(&self) -> VfsResult<u64> {
        let present = self.units_present()?;
        let Some(last) = present.checked_sub(1) else {
            return Ok(0);
        };
        if let Layout::Single { unit, .. } = self.layout {
            let at = self.physical(last).saturating_add(u64::from(unit));
            let mut bytes = [0u8; unit::TRAILER];
            if self.inner.read_exact_at(at, &mut bytes).is_ok()
                && bytes.iter().any(|byte| *byte != 0)
                && unit::Trailer::decode(&bytes).length == unit
            {
                return Ok(present.saturating_mul(u64::from(unit)));
            }
        }
        let current = self.load(last)?;
        Ok(last
            .saturating_mul(self.unit())
            .saturating_add(u64::from(current.length)))
    }

    /// Refuses a write on a read only handle, as the file underneath would.
    ///
    /// @param operation - what was attempted
    fn require_writable(&self, operation: VfsOperation) -> VfsResult<()> {
        match self.read_only {
            true => Err(error::read_only(format!(
                "{operation:?} on a read only file"
            ))),
            false => Ok(()),
        }
    }
}

impl VfsFile for CryptFile {
    /// Reads plaintext, decrypting each unit the range touches.
    fn read_exact_at(&self, offset: u64, output: &mut [u8]) -> VfsResult<()> {
        output.fill(0);
        if output.is_empty() {
            return Ok(());
        }
        let unit = self.unit();
        let end = offset.saturating_add(output.len() as u64);
        let present = self.units_present()?;
        let first = offset / unit;
        let last = (end.saturating_sub(1) / unit).min(present.saturating_sub(1));
        let mut available = 0u64;
        if first < present {
            let count = last.saturating_sub(first).saturating_add(1);
            let raw = self.raw(first, count)?;
            let stride = size(self.layout.stride());
            for (step, bytes) in raw.chunks(stride).enumerate() {
                let index = first.saturating_add(step as u64);
                let current = self.decide(index, bytes);
                let start = index.saturating_mul(unit);
                let valid = match index.saturating_add(1) == present {
                    true => u64::from(current.length),
                    false => unit,
                };
                let from = offset.max(start);
                let to = end.min(start.saturating_add(valid));
                if to > from {
                    let source = current
                        .plain
                        .get(size(from.saturating_sub(start))..size(to.saturating_sub(start)))
                        .unwrap_or(&[]);
                    if let Some(target) = output
                        .get_mut(size(from.saturating_sub(offset))..size(to.saturating_sub(offset)))
                    {
                        target.copy_from_slice(source);
                    }
                    available = available.max(to.saturating_sub(offset));
                }
            }
        }
        if available < output.len() as u64 {
            if let Some(rest) = output.get_mut(size(available)..) {
                rest.fill(0);
            }
            return Err(error::short_read(format!(
                "read {available} of {} bytes at {offset}",
                output.len()
            )));
        }
        Ok(())
    }

    /// Writes plaintext, encrypting each unit the range touches.
    fn write_all_at(&self, offset: u64, input: &[u8]) -> VfsResult<()> {
        self.require_writable(VfsOperation::Write)?;
        if input.is_empty() {
            return Ok(());
        }
        let _held = self
            .writing
            .lock()
            .map_err(|_| error::misuse("an encrypted file's write lock was poisoned"))?;
        self.write_units(offset, input)
    }

    /// Returns the logical length: the plaintext bytes the file holds.
    fn file_size(&self) -> VfsResult<u64> {
        self.logical_size()
    }

    /// Sets the logical length, rewriting the last unit when it is cut part
    /// of the way through.
    fn truncate(&self, size_wanted: u64) -> VfsResult<()> {
        self.require_writable(VfsOperation::Truncate)?;
        let _held = self
            .writing
            .lock()
            .map_err(|_| error::misuse("an encrypted file's write lock was poisoned"))?;
        let now = self.logical_size()?;
        if size_wanted > now {
            return self.write_units(size_wanted.saturating_sub(1), &[0]);
        }
        let unit = self.unit();
        let keep = size_wanted.div_ceil(unit);
        let tail = size_wanted % unit;
        if tail != 0 {
            let index = keep.saturating_sub(1);
            let mut current = self.load(index)?;
            if let Some(cut) = current.plain.get_mut(size(tail)..) {
                cut.fill(0);
            }
            let plain = current.plain.clone();
            current.length = u32::try_from(tail).unwrap_or(u32::MAX);
            let run = self.sealed(index, plain, current.length, &current, false);
            self.flush(vec![run])?;
        }
        if let Ok(mut held) = self.unsynced.lock() {
            held.retain(|index, _| *index < keep);
        }
        self.inner.truncate(self.physical(keep))
    }

    /// Syncs the file underneath, after which every unit this handle wrote
    /// is the synced copy.
    fn sync(&self, mode: SyncMode) -> VfsResult<()> {
        self.inner.sync(mode)?;
        if let Ok(mut held) = self.unsynced.lock() {
            held.clear();
        }
        Ok(())
    }

    /// Passes the lock through; locks are on the file, not its contents.
    /// Answers for this file's own layout, where a page is a unit and its trailer.
    fn pages_under_the_lock_bytes(&self, page_size: u64) -> Vec<u64> {
        // A page here is a unit, and a unit sits at `base + index * stride` with
        // its trailer, so the lock bytes fall in whichever units span them. The
        // caller's page size is the unit size for a database file.
        let _ = page_size;
        crate::os::pages_under_the_lock_bytes(self.base, self.layout.stride())
    }

    fn lock(&self, level: FileLock) -> VfsResult<()> {
        self.inner.lock(level)
    }

    /// Passes the unlock through.
    fn unlock(&self, level: FileLock) -> VfsResult<()> {
        self.inner.unlock(level)
    }

    /// Passes the lock level through.
    fn lock_level(&self) -> FileLock {
        self.inner.lock_level()
    }

    /// Passes the reserved lock check through.
    fn check_reserved_lock(&self) -> VfsResult<bool> {
        self.inner.check_reserved_lock()
    }

    /// Reports the file underneath's guarantees, less memory mapping, which
    /// would expose ciphertext.
    fn device_characteristics(&self) -> DeviceCharacteristics {
        let mut device = self.inner.device_characteristics();
        device.supports_mmap = false;
        device
    }

    /// Passes the shared memory file through. It holds lock slots and an
    /// index of page numbers, and no row.
    fn shared_memory(&self) -> VfsResult<Option<Arc<dyn SharedMemory>>> {
        self.inner.shared_memory()
    }

    /// Passes the identity through.
    fn file_identity(&self) -> VfsResult<FileIdentity> {
        self.inner.file_identity()
    }
}

/// Builds the error an open reports for a file it cannot use as encrypted.
///
/// @param detail - what was wrong, for the internal detail
pub(crate) fn not_a_database(detail: impl Into<String>) -> VfsError {
    VfsError::new(
        inillucent_base::ExtendedCode::from_primary(inillucent_base::PrimaryCode::NotADb),
        detail,
    )
}

//! The file lock this pool holds, and who else holds it.
//!
//! Invariant: **the pool owns the file, so the pool owns the lock.** The
//! protocol itself is `inillucent-vfs`'s - SHARED, RESERVED, PENDING,
//! EXCLUSIVE, implemented and conformance tested since Phase 2 - and taking it
//! here is what makes `PRAGMA locking_mode = normal` a description of what
//! happens rather than a claim about it.
//!
//! Split out of `pool.rs` in task-1980, which added `file` below. Nothing moved
//! but the text: these are still `impl Pool`, reading the same private state,
//! because a child module can see its parent's private items - the same split
//! `eviction.rs` and `swizzle.rs` are.

use super::*;

/// How long [`Pool::hold_for_writing`] waits for RESERVED, in milliseconds.
///
/// Long enough to outlast a writer that took RESERVED to start a statement and
/// is about to let it go because a reader is in its way, which is a matter of a
/// millisecond or two. Short enough that a reader beside a transaction holding
/// RESERVED gives way before either of them has waited for anything that
/// matters - see that function.
const RESERVED_PATIENCE_MILLIS: u64 = 100;

impl Pool {
    /// Raises the lock on the database file.
    ///
    /// **The pool owns the file, so the pool owns the lock.** The protocol
    /// itself is `inillucent-vfs`'s - it has been implemented and conformance
    /// tested since Phase 2 and nothing used it, because the engine assumed it
    /// was the only process on the file. Using it is what makes
    /// `PRAGMA locking_mode = normal` a description rather than a claim.
    ///
    /// @param level - the level to raise to
    pub fn lock(&self, level: FileLock) -> DbResult<()> {
        self.lock_within(level, crate::file::default_busy_millis())
    }

    /// Raises the lock, waiting up to a budget for the holder to let go.
    ///
    /// **Waiting is the whole of what a busy timeout is.** A lock another
    /// process holds is not an error - it is a lock that will be released - and
    /// an engine that reported failure immediately would make every concurrent
    /// pair of writers fail rather than take turns. The sleep grows so that a
    /// long wait is not a spin, and the last attempt reports what it found.
    ///
    /// @param level - the level to raise to
    /// @param budget_millis - how long to keep trying
    pub fn lock_within(&self, level: FileLock, budget_millis: u64) -> DbResult<()> {
        lock_with_wait(self.file.as_ref(), level, budget_millis)
    }

    /// Lowers the lock on the database file.
    ///
    /// @param level - the level to drop to, `None` to release entirely
    pub fn unlock(&self, level: FileLock) -> DbResult<()> {
        if self.file.lock_level() <= level {
            return Ok(());
        }
        self.file
            .unlock(level)
            .map_err(|error| error.into_db_error())
    }

    /// Returns the level currently held.
    pub fn lock_level(&self) -> FileLock {
        self.file.lock_level()
    }

    /// Raises a shared lock to the exclusive one, before this pool writes the
    /// database file or its journal.
    ///
    /// **A connection that holds only SHARED writes pages too, and it used to
    /// do it under SHARED** (task-2166). A reader that finds another process
    /// has committed replays the log into its own pool, and a replay that
    /// dirties more pages than the pool holds evicts them - which writes each
    /// one to the file and its old image to `<database>-journal`. Every other
    /// reader holds SHARED at the same moment, so two of them could write the
    /// same journal, and one could read a page while the other was halfway
    /// through writing it. The journal was then left beside the file after the
    /// reader let go, where the next process to open the file put the old
    /// images back over pages the reader believed were current.
    ///
    /// Raising here rather than in the engine is what reaches every such write:
    /// an eviction inside a replay, inside an open, inside a read. A connection
    /// that raised keeps EXCLUSIVE until it lets the file go, and the engine
    /// folds before it does so the journal does not outlive the lock - see
    /// `ImportedDatabase::release_if_idle`.
    ///
    /// **RESERVED is waited for only briefly.** A connection that holds SHARED
    /// and is refused RESERVED is beside another one that has it. If that one
    /// is a transaction, it is waiting for every SHARED holder to leave before
    /// it can take EXCLUSIVE, and waiting here would be each waiting for the
    /// other until a budget ran out - so this one gives way and reports busy,
    /// which is what SQLite does in the same place. The brief wait is for the
    /// other case: a writer starting a statement takes RESERVED and lets it go
    /// again within a millisecond when a reader is in its way, and failing a
    /// read over that would be failing it for nothing. EXCLUSIVE is then
    /// waited for: what stands in the way of it is readers finishing a
    /// statement.
    ///
    /// A pool holding no lock at all is left alone. That is a file nobody else
    /// can reach - one being created, or one a test opened without the
    /// protocol - and a pool holding EXCLUSIVE has nothing to raise.
    pub fn hold_for_writing(&self) -> DbResult<()> {
        let level = self.file.lock_level();
        if level == FileLock::None || level == FileLock::Exclusive || !self.writable.get() {
            return Ok(());
        }
        if level < FileLock::Reserved {
            lock_with_wait(
                self.file.as_ref(),
                FileLock::Reserved,
                RESERVED_PATIENCE_MILLIS.min(self.write_lock_millis.get()),
            )
            .map_err(|error| {
                error.with_detail(
                    "this connection had to write pages it replayed from the log, and another \
                     connection holds the file for writing; run the statement again",
                )
            })?;
        }
        lock_with_wait(
            self.file.as_ref(),
            FileLock::Exclusive,
            self.write_lock_millis.get(),
        )
    }

    /// Reports whether `writeback` may write a dirty page, raising the lock
    /// first when it may.
    ///
    /// **A read only handle keeps the page rather than failing the write**
    /// (task-2166). Its dirty pages are ones a replay produced, and the file
    /// cannot take them. Held back, the frame stays resident and the pool
    /// uses a frame it has spare instead, which is what `take_frame` does
    /// past the budget - so a read only connection whose replay is larger
    /// than `cache_size` still reads, up to the frames the pool owns.
    ///
    /// Every other pool writes under EXCLUSIVE or not at all: a reader that
    /// replays past its pool reaches `writeback` holding SHARED - see
    /// [`Pool::hold_for_writing`].
    pub(super) fn may_write_back(&self) -> DbResult<bool> {
        if !self.writable.get() {
            Counters::add(&self.counters.held_back, 1);
            return Ok(false);
        }
        // **A reader beside a writer keeps the page too.** A writer that holds
        // only RESERVED (`Database::set_reserved_writes`) leaves readers
        // reading through its whole transaction, and a reader whose replay
        // outgrows its pool would otherwise ask for RESERVED, be refused and
        // fail its statement. Its dirty pages are ones the replay made and the
        // log holds, so keeping them resident loses nothing.
        let level = self.file.lock_level();
        if level == FileLock::Shared && self.file.check_reserved_lock().unwrap_or(false) {
            Counters::add(&self.counters.held_back, 1);
            return Ok(false);
        }
        self.hold_for_writing()?;
        Ok(true)
    }

    /// Sets how long [`Pool::hold_for_writing`] waits for readers to leave.
    ///
    /// The connection's `busy_timeout`, pushed down by `Database::set_busy_millis`
    /// so a raise made on the connection's behalf waits as long as the
    /// connection said it would.
    ///
    /// @param millis - the budget
    pub fn set_write_lock_millis(&self, millis: u64) {
        self.write_lock_millis.set(millis);
    }

    /// Records that this pool's file handle may not write, so a raise would
    /// only hold other processes out for a write that the handle refuses.
    pub fn forbid_writing(&self) {
        self.writable.set(false);
    }

    /// Returns the file itself, for a caller that has to ask it about its own
    /// locks.
    ///
    /// Narrow on purpose: it exists so a refusal can name who holds the file
    /// rather than which lock level was refused (task-1979, C6), and nothing
    /// else should read or write through it - every read and write in this
    /// crate goes through the pool so the cache and the write-ahead rule are
    /// not bypassed.
    pub fn file(&self) -> &dyn VfsFile {
        self.file.as_ref()
    }

    /// Returns an empty free map that never hands out the pages under this
    /// file's lock bytes.
    ///
    /// Every free map a database builds comes from here, so none of them can
    /// forget the reservation. See `VfsFile::pages_under_the_lock_bytes`.
    pub fn new_free_map(&self) -> crate::freemap::FreeMap {
        crate::freemap::FreeMap::new(self.page_size)
            .reserving(self.file.pages_under_the_lock_bytes(self.page_size as u64))
    }
}

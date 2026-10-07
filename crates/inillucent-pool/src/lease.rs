//! A shared lock kept for a moment after a statement that only read.
//!
//! **Why** (task-2197). Under `locking_mode = normal` every statement outside a
//! transaction took SHARED, read the shadow meta record to learn whether
//! another process had folded, asked the open log segment its length, and let
//! SHARED go again: four calls into the kernel around a point query whose own
//! work is about a microsecond. Measured from Python, those four calls were
//! 6.4 us of a 17 us `SELECT` by primary key, against 0.9 us for the lookup.
//!
//! A connection that keeps SHARED between two statements needs neither the
//! lock nor the meta record the second time: a fold needs EXCLUSIVE, which
//! nobody can take while this connection holds SHARED, so the file and its meta
//! record cannot have changed. It still asks the log its length, because a
//! writer appends to the log under RESERVED beside a reader.
//!
//! Invariant: **a lease never outlives [`IDLE`] without a statement, nor
//! [`MOST`] in all, and a connection in this process that needs the file takes
//! it away at once.** That bounds what a writer in another process waits for
//! to what one statement of a reader cost it before, plus at most [`MOST`].
//! The lease ends three ways:
//!
//! - **the next statement claims it** ([`Lease::settle`]), and the lock is the
//!   connection's again, held the ordinary way, for that statement;
//! - **a thread of this module lets it go** once it has been idle for
//!   [`IDLE`], so a connection that stops issuing statements does not hold a
//!   writer off;
//! - **a statement that finds [`MOST`] has passed since the lock was taken lets
//!   it go and takes it again the ordinary way**, which waits behind a writer
//!   holding PENDING. Without this a reader running statements back to back
//!   would keep SHARED for ever and a writer waiting for it would starve.
//!
//! The lease lives beside the pool's file. A statement claims it at its start
//! (`Database::begin_read` and `Database::begin_write_within`), and every lock
//! the pool takes or drops claims it first, so nothing can raise or drop the
//! lock while the thread here might be letting it go. **Asking which lock is
//! held does not claim it**: a caller that only looks, between statements,
//! would otherwise turn a lease into a lock that nothing then lets go of.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, Weak};
use std::time::{Duration, Instant};

use inillucent_vfs::{FileIdentity, FileLock, VfsFile};

/// How long a lease survives with no statement on its connection.
///
/// Long enough to bridge the gap between two statements of a loop in an
/// interpreter, which is a few microseconds, with room for the time the
/// caller spends on each row. Short enough that a writer in another process,
/// which retries after 1 ms and then 2 ms, finds the file free on its second
/// try.
pub const IDLE: Duration = Duration::from_millis(1);

/// How long one lock taken the ordinary way may be kept by leases in all.
///
/// The lock is then let go and taken again, which costs the four kernel calls
/// the lease saves once every hundred or so statements, and lets a writer that
/// is waiting with PENDING in. A writer retries after 1 ms, then at 3 ms and
/// 7 ms, so this is kept well under 1 ms.
///
/// **What a writer beside a busy reader still waits for is the reader's
/// replay, not the lease** (measured in task-2197). A Python reader running
/// `SELECT count(*)` back to back beside a process running one row `exec`s
/// made each `exec` wait 3 to 7 ms for EXCLUSIVE whether the lease was 50 us,
/// 250 us or off: the reader sees the commit in the log before the writer
/// folds, and replays it under SHARED. With the lease the reader is almost
/// always inside a statement or a lease when the commit lands, so the writer
/// meets that wait on most `exec`s rather than on some: its median went from
/// 12.8 ms to 19 ms. A writer beside a reader that is idle, or reads now and
/// then, does not meet it.
pub const MOST: Duration = Duration::from_micros(250);

/// Where a lease is.
#[derive(Clone, Copy, Debug)]
enum State {
    /// No lease: the lock, if any, is held the ordinary way.
    Off,
    /// SHARED is held between statements, until the first of the two times.
    Armed {
        /// When the lease ends if no statement claims it.
        idle_until: Instant,
        /// When the lock must be let go whatever happens, for a waiting writer.
        held_until: Instant,
    },
}

/// One pool's lease: its state, and the file whose lock it holds.
#[derive(Debug)]
pub struct LeaseCell {
    /// The state, behind a lock the releasing thread takes too.
    state: Mutex<State>,
    /// Set when the lease ended by letting the lock go, and cleared by
    /// [`Lease::take_released`]. What a connection derived under the lock is
    /// no longer to be trusted once this is set.
    released: AtomicBool,
    /// Whether this cell is in the releasing thread's list.
    queued: AtomicBool,
    /// The file, to let its lock go.
    file: Arc<dyn VfsFile>,
    /// The file's identity, read the first time another handle asks.
    identity: OnceLock<Option<FileIdentity>>,
}

impl LeaseCell {
    /// Locks the state, recovering it if a panic poisoned the lock.
    ///
    /// The state is one value written whole, so a poisoned one is still a
    /// state this module wrote.
    fn state(&self) -> MutexGuard<'_, State> {
        match self.state.lock() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Lets the lock go and records that the lease ended that way.
    ///
    /// A failure to unlock is ignored on purpose: the handle's own lock state
    /// is what the next statement reads, and a lock that could not be dropped
    /// is still held, which is what that state will say.
    ///
    /// @param state - the state, locked by the caller
    fn let_go(&self, state: &mut State) {
        *state = State::Off;
        if self.file.lock_level() == FileLock::Shared {
            let _ = self.file.unlock(FileLock::None);
        }
        self.released.store(true, Ordering::Release);
    }

    /// Lets an idle lease go, when it has been idle for [`IDLE`] or held for
    /// [`MOST`].
    ///
    /// @param now - the time to judge by
    fn expire(&self, now: Instant) {
        let mut state = self.state();
        if let State::Armed {
            idle_until,
            held_until,
        } = *state
        {
            if now >= idle_until.min(held_until) {
                self.let_go(&mut state);
            }
        }
    }

    /// When this lease ends on its own, or `None` when there is none.
    fn deadline(&self) -> Option<Instant> {
        match *self.state() {
            State::Armed {
                idle_until,
                held_until,
            } => Some(idle_until.min(held_until)),
            State::Off => None,
        }
    }

    /// The file's identity, read once.
    fn identity(&self) -> Option<&FileIdentity> {
        self.identity
            .get_or_init(|| self.file.file_identity().ok())
            .as_ref()
    }
}

impl Drop for LeaseCell {
    /// Lets the lock of a lease nobody claimed go, when the pool is dropped.
    fn drop(&mut self) {
        let mut state = self.state();
        if let State::Armed { .. } = *state {
            self.let_go(&mut state);
        }
    }
}

/// The pool's handle on its lease.
#[derive(Debug)]
pub struct Lease {
    /// The cell, shared with the releasing thread by a weak reference.
    cell: Arc<LeaseCell>,
}

impl Lease {
    /// Makes the lease for a pool's file and hands the file back shared with
    /// it, as the pool keeps it.
    ///
    /// @param file - the pool's file
    pub fn for_file(file: Box<dyn VfsFile>) -> (Arc<dyn VfsFile>, Lease) {
        let file: Arc<dyn VfsFile> = Arc::from(file);
        let lease = Lease::new(Arc::clone(&file));
        (file, lease)
    }

    /// Makes the lease for a pool's file. It starts with no lease.
    ///
    /// @param file - the pool's file
    pub fn new(file: Arc<dyn VfsFile>) -> Lease {
        Lease {
            cell: Arc::new(LeaseCell {
                state: Mutex::new(State::Off),
                released: AtomicBool::new(false),
                queued: AtomicBool::new(false),
                file,
                identity: OnceLock::new(),
            }),
        }
    }

    /// Keeps SHARED past the end of a statement, until a statement claims it
    /// or the lease ends.
    ///
    /// Answers false, and arms nothing, when `held_since` is already [`MOST`]
    /// ago, the handle does not hold exactly SHARED, or the file does not allow
    /// a lease (`VfsFile::may_lease`); the caller then lets the lock go the
    /// ordinary way.
    ///
    /// @param held_since - when the lock was taken the ordinary way
    pub fn arm(&self, held_since: Instant) -> bool {
        let now = Instant::now();
        let held_until = held_since + MOST;
        if now >= held_until
            || !self.cell.file.may_lease()
            || self.cell.file.lock_level() != FileLock::Shared
        {
            return false;
        }
        *self.cell.state() = State::Armed {
            idle_until: now + IDLE,
            held_until,
        };
        reaper().watch(&self.cell);
        true
    }

    /// Ends a lease before the pool touches its lock: the lock is held the
    /// ordinary way again, or it has been let go.
    ///
    /// A lease past [`MOST`] is let go here rather than handed back, so the
    /// statement takes SHARED again the ordinary way, behind any writer
    /// waiting with PENDING.
    pub fn settle(&self) {
        let mut state = self.cell.state();
        if let State::Armed { held_until, .. } = *state {
            match Instant::now() >= held_until {
                true => self.cell.let_go(&mut state),
                false => *state = State::Off,
            }
        }
    }

    /// Reports, once, that a lease ended by letting the lock go since the
    /// last time this was asked.
    pub fn take_released(&self) -> bool {
        self.cell.released.swap(false, Ordering::AcqRel)
    }
}

/// Retries a refused raise once after letting go of the leases other pools of
/// this process hold on the same file, and answers whether it then succeeded.
///
/// **Called by the two lock waits after their first refusal** (task-2197). A
/// lease would end by itself within a millisecond; letting it go now is what
/// keeps a writer beside a reader in the same thread from sleeping once per
/// write.
///
/// @param file - the file a raise was refused on
/// @param level - the level the raise asked for
pub fn retry_after_yield(file: &dyn VfsFile, level: FileLock) -> bool {
    yield_others(file) && file.lock(level).is_ok()
}

/// Lets go of every lease another pool in this process holds on the same
/// file, so a raise this handle is refused does not wait for them.
///
/// **Two connections of one process are two handles**, and the operating
/// system arbitrates their locks exactly as it does two processes'. A writer
/// beside a leasing reader in the same thread would otherwise sleep until the
/// lease ended, once per write.
///
/// Answers whether it let any go, which is when retrying the raise is worth
/// it.
///
/// @param file - the file a raise was refused on
pub fn yield_others(file: &dyn VfsFile) -> bool {
    let Some(reaper) = REAPER.get() else {
        return false;
    };
    let cells: Vec<Arc<LeaseCell>> = reaper.cells().iter().filter_map(Weak::upgrade).collect();
    if !cells.iter().any(|cell| cell.deadline().is_some()) {
        return false;
    }
    let Ok(wanted) = file.file_identity() else {
        return false;
    };
    let mut released = false;
    for cell in cells {
        let mut state = cell.state();
        if let State::Armed { .. } = *state {
            if cell.identity() == Some(&wanted) {
                cell.let_go(&mut state);
                released = true;
            }
        }
    }
    released
}

/// The thread that lets idle leases go, and the cells it watches.
struct Reaper {
    /// The cells with a lease, by weak reference so a dropped pool goes.
    cells: Mutex<Vec<Weak<LeaseCell>>>,
    /// Woken when a cell is added.
    wake: Condvar,
}

/// The one releasing thread of this process, started by the first lease.
static REAPER: OnceLock<Reaper> = OnceLock::new();

/// Returns the releasing thread's state, starting the thread the first time.
///
/// **The thread is started after `REAPER` holds its value.** It used to be
/// started inside `get_or_init`, and a thread that ran before `get_or_init`
/// stored the value found `REAPER.get()` empty and exited. That process then
/// had no releasing thread at all, so an idle lease was kept until the
/// connection's next statement and a writer in another process was refused
/// meanwhile. It happened on a loaded Linux runner in 2.3.2's public tests.
fn reaper() -> &'static Reaper {
    let mut first = false;
    let reaper = REAPER.get_or_init(|| {
        first = true;
        Reaper {
            cells: Mutex::new(Vec::new()),
            wake: Condvar::new(),
        }
    });
    if first {
        let spawned = std::thread::Builder::new()
            .name("inillucent-lease".into())
            .spawn(|| {
                if let Some(reaper) = REAPER.get() {
                    reaper.run();
                }
            });
        // A thread that could not start leaves leases to be claimed or to end
        // at `MOST` on the next statement; the only cost is a writer that waits
        // longer for an idle reader.
        drop(spawned);
    }
    reaper
}

impl Reaper {
    /// Locks the list, recovering it if a panic poisoned the lock.
    fn cells(&self) -> MutexGuard<'_, Vec<Weak<LeaseCell>>> {
        match self.cells.lock() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Adds a cell to the list, once, and wakes the thread.
    ///
    /// **The list's lock is taken even when the cell is already listed.** The
    /// thread looks for a lease and starts waiting under that lock, so taking
    /// it here, after the lease was armed, means the thread either sees the
    /// lease or is already waiting when the wake arrives. Without it a wake
    /// sent between the thread's look and its wait was lost, and an idle
    /// lease was kept until the next statement.
    ///
    /// @param cell - the cell that was just armed
    fn watch(&self, cell: &Arc<LeaseCell>) {
        let mut cells = self.cells();
        if !cell.queued.swap(true, Ordering::AcqRel) {
            cells.push(Arc::downgrade(cell));
        }
        drop(cells);
        self.wake.notify_one();
    }

    /// The thread's loop: sleep until the earliest lease ends, let the ended
    /// ones go, and wait for a lease when there is none.
    fn run(&self) {
        loop {
            let earliest = {
                let mut cells = self.cells();
                loop {
                    let earliest = cells
                        .iter()
                        .filter_map(Weak::upgrade)
                        .filter_map(|cell| cell.deadline())
                        .min();
                    if let Some(earliest) = earliest {
                        break earliest;
                    }
                    cells = match self.wake.wait(cells) {
                        Ok(held) => held,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                }
            };
            let now = Instant::now();
            if earliest > now {
                std::thread::sleep(earliest - now);
            }
            self.expire_due();
        }
    }

    /// Lets every lease that has ended go, and drops the cells of pools that
    /// are gone.
    ///
    /// A cell stays listed for as long as its pool lives, leased or not, so a
    /// cell armed again is never missing from the list.
    fn expire_due(&self) {
        let now = Instant::now();
        let cells: Vec<Arc<LeaseCell>> = {
            let mut list = self.cells();
            list.retain(|weak| weak.strong_count() > 0);
            list.iter().filter_map(Weak::upgrade).collect()
        };
        for cell in cells {
            cell.expire(now);
        }
    }
}

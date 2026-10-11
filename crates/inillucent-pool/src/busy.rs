//! How long a wait for a file lock sleeps between two tries.
//!
//! Invariant: **a wait that has run for 15 ms tries again every 2 or 3 ms for as
//! long as it lasts, so a process that has waited longer never tries less often
//! than one that has just begun.** Every loop in this crate that waits for a lock
//! takes its next sleep from [`next_busy_pause`].

/// The longest a wait for a lock sleeps between two tries, in milliseconds,
/// once its first sleeps have grown to it.
pub const LONGEST_BUSY_PAUSE: u64 = 3;

/// Returns how long a wait for a lock sleeps before its next try.
///
/// **A process that has waited longer tries at least as often as one that has
/// just begun.** The sleep doubles from 1 ms to 8 ms and then stays at 2 or 3
/// ms. It used to double up to 50 ms, so a process that had waited a while tried
/// a few times a second while one that had just committed and wanted the lock
/// again tried at 1, 2 and 4 ms, and the newcomer took nearly every moment the
/// lock was free. With 16 processes each running `BEGIN IMMEDIATE` for 15 s,
/// two transactions waited out the 5 s busy timeout that way while the lock
/// changed hands about a hundred times a second. A try is a few calls into the
/// kernel, so trying every few milliseconds costs a waiter nothing it would
/// notice.
///
/// @param pause - the sleep just taken, in milliseconds
/// @param waited - how long this wait has slept in all, in milliseconds
pub fn next_busy_pause(pause: u64, waited: u64) -> u64 {
    // The first sleeps are 1, 2, 4 and 8 ms, which add up to 15.
    match waited < 15 {
        true => pause.saturating_mul(2).min(8),
        // Two or three, so waiters that started together do not keep trying
        // at the same moments.
        false => (LONGEST_BUSY_PAUSE - 1).saturating_add(waited % 2),
    }
}

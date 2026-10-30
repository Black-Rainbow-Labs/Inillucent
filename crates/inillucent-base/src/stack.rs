//! How much stack the calling thread has left.
//!
//! Invariant: **[`room`] never reports more stack than the operating system gave the calling
//! thread, and reports no limit at all only where the operating system will not say.** A caller
//! that asks [`has_room`] before it recurses is told no while [`MARGIN`] bytes still remain, so the
//! frames that run between two checks always have somewhere to go.
//!
//! ## Why this exists
//!
//! The engine recurses over the shape of a statement: the parser over nested parentheses and
//! subqueries, the binder over an expression tree and the views and common table expressions a
//! query names, the planner and the executor over the same trees, and the trigger machinery over
//! the writes a trigger makes. Each level costs about 6 KB of native stack. The command line
//! programs run a statement on a thread with 64 MiB, which fits the depth limit of 1,000. A language
//! binding runs a statement on whatever thread its caller owns: Python's main thread on Windows has
//! 2,000,000 bytes, a thread Rust or C starts has 1 MiB, and 350 `OR` terms, 300 nested scalar
//! subqueries or a chain of 600 views ended the process with a stack overflow, which no caller can
//! catch.
//!
//! The limit is a property of the thread, so it is read from the operating system once per thread
//! and kept in a thread local. Asking costs one thread local read and one subtraction.
//!
//! ## Why the `unsafe` is here
//!
//! The bounds of the running thread's stack have no representation in the standard library.
//! `GetCurrentThreadStackLimits` on Windows, `pthread_getattr_np` on Linux and
//! `pthread_get_stackaddr_np` on macOS each write integers into memory this module owns and keep
//! no pointer to it. This file holds those calls and nothing else.

use std::cell::Cell;

/// How many bytes must remain before a recursive step may go one level deeper.
///
/// The frames between two checks are the ones a check does not cover: a function that calls a
/// builtin, formats an error or drops a partly built tree. Measured on the deepest of them, those
/// use well under 64 KB, so 128 KB leaves them twice the room they were seen to need. The
/// operating system's own guard pages at the low end of the stack are inside this margin.
pub const MARGIN: usize = 128 * 1024;

/// The message every refusal made for want of stack carries, so one phrase names the cause in the
/// parser, the binder and the executor.
pub const REFUSAL: &str = "statement nests too deeply for the stack of the calling thread; run it on a thread with a larger stack or flatten the statement";

thread_local! {
    /// The lowest address of this thread's stack. `0` before the first question, and `1` when the
    /// operating system would not say, which leaves every question answered with a very large room.
    static LOW: Cell<usize> = const { Cell::new(0) };
}

/// Returns an address close to the current top of the calling thread's stack.
///
/// The address of a local is within one frame of the stack pointer, which is the precision a
/// margin of 128 KB needs.
#[inline(always)]
fn stack_pointer() -> usize {
    let marker = 0u8;
    std::ptr::addr_of!(marker) as usize
}

/// Returns how many bytes of stack the calling thread has left.
///
/// Answers `usize::MAX` when the operating system does not report the thread's bounds.
pub fn room() -> usize {
    let low = LOW.with(|cell| {
        let known = cell.get();
        if known != 0 {
            return known;
        }
        let asked = low_limit().filter(|address| *address != 0).unwrap_or(1);
        cell.set(asked);
        asked
    });
    if low == 1 {
        return usize::MAX;
    }
    stack_pointer().saturating_sub(low)
}

/// Builds the error a recursive step returns when [`has_room`] answered false.
///
/// `SQLITE_TOOBIG`, which the driver reports as `too_big`: the statement is valid and its nesting
/// is past what this thread can hold, the same reading as a value past a hard limit.
pub fn refusal() -> crate::DbError {
    crate::DbError::primary(crate::PrimaryCode::TooBig)
        .with_message(REFUSAL)
        .with_detail(REFUSAL)
}

/// Whether the calling thread has more than [`MARGIN`] bytes of stack left.
///
/// A recursive function calls this where it descends and refuses the statement when it answers
/// false.
#[inline]
pub fn has_room() -> bool {
    room() > MARGIN
}

/// Asks the operating system for the lowest address of the calling thread's stack.
#[cfg(windows)]
#[allow(unsafe_code)]
fn low_limit() -> Option<usize> {
    let mut low: usize = 0;
    let mut high: usize = 0;
    // SAFETY: the call writes the two integers it is handed and keeps no pointer to them; both
    // live on this frame for the length of the call.
    unsafe {
        windows_sys::Win32::System::Threading::GetCurrentThreadStackLimits(&mut low, &mut high);
    }
    Some(low)
}

/// Asks the operating system for the lowest address of the calling thread's stack.
#[cfg(any(target_os = "linux", target_os = "android"))]
#[allow(unsafe_code)]
fn low_limit() -> Option<usize> {
    // SAFETY: an all zero `pthread_attr_t` is the state `pthread_getattr_np` expects to overwrite.
    let mut attributes: libc::pthread_attr_t = unsafe { std::mem::zeroed() };
    // SAFETY: `pthread_self` names the calling thread, which is alive, and `attributes` is a valid
    // place to write to for the length of the call.
    if unsafe { libc::pthread_getattr_np(libc::pthread_self(), &mut attributes) } != 0 {
        return None;
    }
    let mut address: *mut libc::c_void = std::ptr::null_mut();
    let mut size: libc::size_t = 0;
    // SAFETY: `attributes` was filled by the successful call above, and the two outputs are
    // locals on this frame.
    let answered =
        unsafe { libc::pthread_attr_getstack(&attributes, &mut address, &mut size) } == 0;
    // SAFETY: `attributes` was initialised by `pthread_getattr_np` and is destroyed once.
    unsafe {
        libc::pthread_attr_destroy(&mut attributes);
    }
    answered.then_some(address as usize)
}

/// Asks the operating system for the lowest address of the calling thread's stack.
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
fn low_limit() -> Option<usize> {
    // SAFETY: `pthread_self` names the calling thread, and both calls only read that thread's
    // recorded stack bounds.
    let (high, size) = unsafe {
        let thread = libc::pthread_self();
        (
            libc::pthread_get_stackaddr_np(thread) as usize,
            libc::pthread_get_stacksize_np(thread),
        )
    };
    high.checked_sub(size)
}

/// Asks the operating system for the lowest address of the calling thread's stack.
///
/// A system this module has no call for answers nothing, which [`room`] reads as no limit.
#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "android",
    target_os = "macos"
)))]
fn low_limit() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recurses until [`has_room`] says no and returns how deep it got.
    ///
    /// @param depth - the levels so far
    fn descend(depth: usize) -> usize {
        if !has_room() {
            return depth;
        }
        let padding = [depth as u8; 512];
        let reached = descend(depth + 1);
        std::hint::black_box(&padding);
        reached
    }

    /// A thread with a stack of 1 MiB reports room below that, and a recursion that asks
    /// [`has_room`] stops before it overflows.
    #[test]
    fn a_small_thread_runs_out_of_room_before_it_overflows() {
        let reached = std::thread::Builder::new()
            .stack_size(1 << 20)
            .spawn(|| {
                let at_start = room();
                (at_start, descend(0))
            })
            .expect("the thread starts")
            .join()
            .expect("the thread finished");
        let (at_start, depth) = reached;
        assert!(at_start <= (1 << 20), "room at the start was {at_start}");
        assert!(at_start > MARGIN, "room at the start was {at_start}");
        // 1 MiB minus the margin, in frames of at least 512 bytes.
        assert!(depth > 100 && depth < 2048, "descended {depth} levels");
    }

    /// A thread with a larger stack reports more room, so the answer follows the thread.
    #[test]
    fn a_larger_thread_reports_more_room() {
        let big = std::thread::Builder::new()
            .stack_size(16 << 20)
            .spawn(room)
            .expect("the thread starts")
            .join()
            .expect("the thread finished");
        assert!(big > (8 << 20), "room on a 16 MiB thread was {big}");
    }
}

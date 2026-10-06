//! Whether the thread a program starts on already has the stack its
//! statements need.
//!
//! Invariant: **the answer is read from the operating system, never assumed
//! from how the program was built.** A program linked with a 64 MiB main
//! thread reserve runs its body where it is; one linked without, as a debug
//! build is, still starts the sized thread it always did.
//!
//! ## Why this exists (task-2191)
//!
//! [`crate::on_a_sized_stack`] started a thread with [`crate::STATEMENT_STACK`]
//! bytes of stack and joined it, for every command line call. Starting and
//! joining a thread is tens of microseconds on Windows, paid by a one row query
//! whose whole run is a millisecond. The Windows release build now links its
//! programs with `/STACK` at the same size (`packaging/release-all.ps1`), so the
//! main thread already has the reserve and the extra thread buys nothing.
//!
//! ## Why the `unsafe` is here
//!
//! The reserve of the running thread has no representation in the standard
//! library. `GetCurrentThreadStackLimits` is one call that writes two integers
//! the caller owns, and this file holds it and nothing else.

/// Reports whether the calling thread is the program's main thread and has at
/// least `wanted` bytes of stack reserved.
///
/// @param wanted - how many bytes the caller needs
#[cfg(windows)]
pub(crate) fn main_thread_has(wanted: usize) -> bool {
    if std::thread::current().name() != Some("main") {
        return false;
    }
    let mut low: usize = 0;
    let mut high: usize = 0;
    // SAFETY: the call writes the two integers it is handed and keeps no
    // pointer to them; both live on this frame for the length of the call.
    unsafe {
        windows_sys::Win32::System::Threading::GetCurrentThreadStackLimits(&mut low, &mut high);
    }
    high.saturating_sub(low) >= wanted
}

/// Reports whether the calling thread is the program's main thread and has at
/// least `wanted` bytes of stack reserved.
///
/// Always `false` off Windows: a Unix main thread's stack is the `ulimit` the
/// shell set, which the program cannot rely on, so the sized thread is started
/// as before.
///
/// @param wanted - how many bytes the caller needs
#[cfg(not(windows))]
pub(crate) fn main_thread_has(wanted: usize) -> bool {
    let _ = wanted;
    false
}

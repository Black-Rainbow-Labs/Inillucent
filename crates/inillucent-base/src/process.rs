//! Facts about the running process that the standard library cannot answer everywhere.
//!
//! Invariant: every function here answers on every target the workspace builds,
//! and never panics.
//!
//! `std::process::id` panics with "unsupported" on `wasm32-wasip1`, because
//! WASI preview 1 has no process ids, and the engine reads the id to make a
//! scratch name unique and to seed `random()`. A WebAssembly instance is the
//! only process in its own memory, so 0 is as unique there as any number.

/// Returns this process's id, or 0 on a target that has none.
pub fn id() -> u32 {
    #[cfg(not(target_os = "wasi"))]
    {
        std::process::id()
    }
    #[cfg(target_os = "wasi")]
    {
        0
    }
}

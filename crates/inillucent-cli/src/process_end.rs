//! Ends the `inillucent` program with the exit code it chose, without letting Windows run library
//! detach code after ONNX Runtime was loaded.
//!
//! Invariant: **a process that loaded ONNX Runtime, or tried to and failed, exits with the exit
//! code it chose.** Before this, about 1 process in 300 that printed the refusal for an
//! `INILLUCENT_EMBED_DEVICE` of `cuda:99` ended with `STATUS_ACCESS_VIOLATION` (0xC0000005) when
//! many programs ran at once.
//!
//! ## What a debugger showed
//!
//! Two crash dumps, taken with `cdb` under the same load, had one thread left. Both stopped in
//! `ntdll!RtlFreeHeap` called from a library's `DLL_PROCESS_DETACH` code during `ExitProcess`: one
//! from `dbghelp`'s `atexit` destructors, one from `KERNELBASE!TipInitialize`. The freed block held
//! `0xbaadf00d`, the heap's marker for a corrupted free list, so the heap itself was already
//! damaged when detach started.
//!
//! `ExitProcess` ends every other thread before it calls the detach code of the loaded libraries.
//! The CUDA and ONNX Runtime threads that are running at that moment can be in the middle of a
//! heap operation. The detach code of an unrelated library then frees into a heap in that state.
//! The environment reference that `inillucent_core::exit_hook::keep_environment` holds stops
//! `ort`'s own release from being the last one, but it cannot stop those threads or the libraries
//! that ONNX Runtime pulled in from running their detach code.
//!
//! ## What this does
//!
//! On Windows, when ONNX Runtime was loaded, [`finish`] flushes standard output and standard
//! error and calls `TerminateProcess` on the current process with the exit code. That ends the
//! process without running any detach code, so nothing frees into the damaged heap. It is only
//! called from the end of `main` in the `inillucent` program, never from the library crates, so a
//! program that embeds the library keeps the normal exit.

use std::process::ExitCode;

/// Returns the number that `code` was built from, or `None` when it is none of 0 to 255.
///
/// `ExitCode` has no accessor, so this compares it with each value `ExitCode::from` can make.
/// @param code - the exit code the program chose
fn number_of(code: ExitCode) -> Option<u8> {
    (0..=u8::MAX).find(|candidate| ExitCode::from(*candidate) == code)
}

/// Whether this process loaded ONNX Runtime, or tried to and created its environment.
fn onnx_runtime_was_loaded() -> bool {
    #[cfg(feature = "embed")]
    {
        inillucent_core::exit_hook::environment_was_created()
    }
    #[cfg(not(feature = "embed"))]
    {
        false
    }
}

/// Ends the process now with `code`, skipping every library's detach code.
/// @param code - the exit status to end with
#[cfg(windows)]
fn terminate_now(code: u8) {
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    // SAFETY: `GetCurrentProcess` returns the pseudo handle for this process and cannot fail.
    // `TerminateProcess` on that handle ends the process with the given status. Nothing is
    // read or written through a pointer.
    unsafe {
        windows_sys::Win32::System::Threading::TerminateProcess(
            windows_sys::Win32::System::Threading::GetCurrentProcess(),
            u32::from(code),
        );
    }
}

/// Does nothing on other systems, where the normal exit path is not the one that crashed.
/// @param code - the exit status to end with
#[cfg(not(windows))]
fn terminate_now(_code: u8) {}

/// Returns `code`, after ending the process first when Windows would crash while exiting.
///
/// Call it with the value `main` is about to return.
/// @param code - the exit status the program chose
pub fn finish(code: ExitCode) -> ExitCode {
    if cfg!(windows) && onnx_runtime_was_loaded() {
        if let Some(number) = number_of(code) {
            terminate_now(number);
        }
    }
    code
}

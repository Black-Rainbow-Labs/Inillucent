//! Ends the `inillucent` program with the exit code it chose, without letting Windows run library
//! detach code after ONNX Runtime's CUDA execution provider failed to start.
//!
//! Invariant: **a process that loaded ONNX Runtime exits with the exit code it chose.** In 2.8.0
//! about 1 process in 300 that printed the refusal for an `INILLUCENT_EMBED_DEVICE` of `cuda:99`
//! ended with `STATUS_ACCESS_VIOLATION` (0xC0000005) when many programs ran at once, and 2.8.0
//! ended every process that had loaded ONNX Runtime with `TerminateProcess`.
//!
//! ## What a debugger showed
//!
//! Two crash dumps had stopped in `ntdll!RtlFreeHeap`, called from an unrelated library's
//! `DLL_PROCESS_DETACH` code, on a heap that was already damaged. Full page heap found what
//! damaged it. `cdb` stops the process at its creation, sets page heap in the process's own
//! `NtGlobalFlag` and full page heap in `verifier!AVrfDphGlobalFlags` before the first heap is
//! made, which needs no administrator rights. Every `cuda:99` process then stops at exit on
//! `lock xadd dword ptr [rbx+8]` in `onnxruntime_providers_cuda`, called from `onnxruntime.dll`'s
//! own static destructors: the release of a static `std::shared_ptr` whose control block the
//! provider had already freed. A run on the processor, a run on a card that exists, and a session
//! dropped before exit all end cleanly under full page heap.
//!
//! ## What this does
//!
//! `inillucent_core::cuda_probe` asks the NVIDIA driver how many cards there are and refuses a
//! card that is not there before ONNX Runtime's CUDA provider is loaded, which removes the crash
//! for every program that embeds the library. A card that is there and still fails to start goes
//! through the provider, and on Windows, when that happened, [`finish`] flushes standard output
//! and standard error and calls `TerminateProcess` with the exit code, so no detach code runs.
//! Every other run, including every run on the processor, exits normally again. It is only called
//! from the end of `main` in the `inillucent` program, never from the library crates.

use std::process::ExitCode;

/// Returns the number that `code` was built from, or `None` when it is none of 0 to 255.
///
/// `ExitCode` has no accessor, so this compares it with each value `ExitCode::from` can make.
/// @param code - the exit code the program chose
fn number_of(code: ExitCode) -> Option<u8> {
    (0..=u8::MAX).find(|candidate| ExitCode::from(*candidate) == code)
}

/// Whether ONNX Runtime's CUDA execution provider loaded in this process and failed to start.
fn cuda_provider_failed() -> bool {
    #[cfg(feature = "embed")]
    {
        inillucent_core::exit_hook::cuda_provider_failed()
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
    if cfg!(windows) && cuda_provider_failed() {
        if let Some(number) = number_of(code) {
            terminate_now(number);
        }
    }
    code
}

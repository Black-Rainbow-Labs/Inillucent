//! Drops every loaded model session before the process's C++ destructors run, and keeps ONNX
//! Runtime's environment alive until the process ends.
//!
//! Invariant: **a process that loaded a model, or tried to and failed, exits with the exit code it
//! chose.** On macOS, 2.0.3 aborted with exit code 134 and
//! `libc++abi: terminating due to uncaught exception of type std::__1::system_error: mutex lock failed`
//! after any `embed()` call under the `idle` or `resident` profile, from the C
//! library, from Node, and from the `inillucent` command line. Under
//! `on-demand`, which drops the session after every call, the same programs
//! exited with 0.
//!
//! ## Why it happened
//!
//! A loaded session lives in a process wide holder that is never dropped,
//! because Rust does not run the destructors of statics. When the program calls
//! `exit`, the C runtime runs the destructors ONNX Runtime registered for its own
//! C++ statics when it was loaded, including mutexes the session's thread pool
//! still uses. A pool thread then locks a destroyed mutex, and libc++ turns that
//! into `std::terminate`.
//!
//! ## What this does
//!
//! The C runtime runs `atexit` handlers and C++ static destructors in the
//! reverse of the order they were registered. ONNX Runtime registers its
//! destructors when the library is loaded, which is when the first session is
//! opened. [`register`] is called after a session has opened, so the handler it
//! installs is registered later and runs earlier. The handler drops every
//! session that is still loaded, which joins the session's threads while the
//! mutexes they use still exist.
//!
//! [`release_all`] runs the same unloaders on request, for a program that knows it is about to
//! exit and would rather release the sessions from its own code than from an exit handler. The C
//! library exports it as `inillucent_release_models`, and the Node and Python wrappers call it
//! before they exit. The PHP package starts the `inillucent` program for each call and loads no
//! library, so it has nothing to release.
//!
//! ## The environment reference
//!
//! The `ort` crate keeps ONNX Runtime's environment in a process wide `Arc` and releases it from
//! a function that runs while the process exits. That release is only safe when it is not the last
//! reference, and it runs at a different point on each operating system:
//!
//! - On Linux it runs from the `.fini_array` of the library that links `ort`. The loader runs it
//!   after ONNX Runtime, which was loaded later, has already run its own destructors, so the call
//!   goes into a library that has been torn down.
//! - On Windows it runs from a thread local storage callback during `ExitProcess`, after the CUDA
//!   execution provider's library and other libraries have started to unload. A debugger on
//!   2.7.0 showed `ReleaseEnv` reading memory the heap had already filled with `0xfeeefeee`,
//!   which ended the process with `STATUS_ACCESS_VIOLATION` or `STATUS_STACK_BUFFER_OVERRUN`.
//! - On macOS it runs from a `__cxa_atexit` handler registered when the environment is first
//!   created, which can run after ONNX Runtime's own C++ destructors.
//!
//! [`keep_environment`] holds one reference to the environment for the life of the process on
//! every operating system, so that release is never the last one. The environment is never
//! released, and the process is exiting anyway. It is called when a session is about to be built,
//! not only after one opened, because a CUDA execution provider that fails to start still creates
//! the environment, and the process then crashed at exit after printing the refusal.
//!
//! ## The exiting flag
//!
//! The idle reaper thread in `residency` drops a session when nothing has used it for the idle
//! period. A drop that starts while the exit handler is running would race the handler, so the
//! handler sets [`exiting`] first and the reaper leaves a session alone once it is set.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// Something that drops one holder's session when the process exits.
///
/// It holds a weak handle, so a holder that is dropped before the exit is not
/// kept alive by being registered here.
type Unloader = Box<dyn Fn() + Send>;

/// Every holder that has loaded a session in this process.
static UNLOADERS: Mutex<Vec<Unloader>> = Mutex::new(Vec::new());

/// Whether the process has started to exit, so that no other thread drops a session now.
static EXITING: AtomicBool = AtomicBool::new(false);

/// Whether the `atexit` handler has been installed.
#[cfg(unix)]
static INSTALLED: OnceLock<bool> = OnceLock::new();

/// One reference to ONNX Runtime's environment, held until the process ends.
///
/// The module comment says why the `ort` crate's own release at exit must never be the last
/// reference on any operating system.
static KEPT_ENVIRONMENT: OnceLock<Arc<ort::environment::Environment>> = OnceLock::new();

#[cfg(unix)]
extern "C" {
    /// The C runtime's exit handler registration, from the C standard library.
    fn atexit(callback: extern "C" fn()) -> std::os::raw::c_int;
}

/// Holds one reference to ONNX Runtime's environment for the life of the process.
///
/// Call it before the first session is built and again whenever one is, because it does nothing
/// once a reference is held. It loads the library when `ort` has not yet, so a caller that cannot
/// tolerate a panic from a library that will not load wraps it in `catch_unwind`, as
/// `build_session` does.
pub fn keep_environment() {
    if KEPT_ENVIRONMENT.get().is_some() {
        return;
    }
    if let Ok(environment) = ort::environment::Environment::current() {
        let _ = KEPT_ENVIRONMENT.set(environment);
    }
}

/// Whether ONNX Runtime's CUDA execution provider was asked to start in this process.
static CUDA_PROVIDER_ASKED: AtomicBool = AtomicBool::new(false);

/// Whether a session failed to open after the CUDA execution provider was asked to start.
static CUDA_PROVIDER_FAILED: AtomicBool = AtomicBool::new(false);

/// Records that ONNX Runtime's CUDA execution provider is about to be asked to start.
///
/// Called after `cuda_probe` found the card, so the provider's library is about to load.
pub fn note_cuda_provider_asked() {
    CUDA_PROVIDER_ASKED.store(true, Ordering::SeqCst);
}

/// Records that a session failed to open, which matters only when the CUDA provider was asked.
pub fn note_session_failed() {
    if CUDA_PROVIDER_ASKED.load(Ordering::SeqCst) {
        CUDA_PROVIDER_FAILED.store(true, Ordering::SeqCst);
    }
}

/// Whether ONNX Runtime's CUDA execution provider loaded in this process and then failed to start.
///
/// **The one state in which ONNX Runtime damages the heap at exit.** Its CUDA provider releases a
/// static `std::shared_ptr` whose control block it had already freed, from `onnxruntime.dll`'s own
/// static destructors (see `cuda_probe`). A program that must not crash at exit ends without
/// running library detach code when this is true; the `inillucent` command line does. A card that
/// is not there is refused before the provider loads, so this is the rarer failure of a card that
/// is there and still does not start.
pub fn cuda_provider_failed() -> bool {
    CUDA_PROVIDER_FAILED.load(Ordering::SeqCst)
}

/// Whether the process has started to exit.
///
/// The idle reaper checks it so it never drops a session while the exit handler is dropping them.
pub fn exiting() -> bool {
    EXITING.load(Ordering::SeqCst)
}

/// Asks for `unload` to be called when the process exits.
///
/// Call it after a session has opened, never before: the handler has to be
/// registered after ONNX Runtime was loaded for it to run before ONNX Runtime's
/// own destructors. A holder calls it once, the first time it loads. On Windows no exit handler is
/// installed, because the loader stops the program's threads before it unloads a library, so a
/// session still loaded at exit is left alone; the unloader is kept for [`release_all`].
/// @param unload - drops the holder's session, if it still has one
pub fn register(unload: Unloader) {
    if let Ok(mut unloaders) = UNLOADERS.lock() {
        unloaders.push(unload);
    }
    keep_environment();
    #[cfg(unix)]
    INSTALLED.get_or_init(|| {
        // SAFETY: `atexit` is the C standard library's function, declared
        // above with its real signature. `run_unloaders` is an `extern "C"`
        // function with no arguments that never unwinds, which is what `atexit`
        // requires of a handler. Registering it has no other effect.
        let status = unsafe { atexit(run_unloaders) };
        status == 0
    });
}

/// Drops every loaded session now and leaves the holders registered, so a later call can load
/// again.
///
/// This is for a program that is about to exit. It does not set [`exiting`], because the process
/// has not started to exit yet and a caller may still embed afterwards. A holder whose lock is
/// busy with a call in flight is left alone, as at exit.
pub fn release_all() {
    let Ok(unloaders) = UNLOADERS.lock() else {
        return;
    };
    for unload in unloaders.iter() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(unload));
    }
}

/// The `atexit` handler: drops every registered holder's session.
///
/// A panic here could not unwind into the C runtime, so it is caught and the
/// remaining holders are still unloaded. The list is taken with `try_lock`
/// because a thread stopped while holding it must not hang the exit.
#[cfg(unix)]
extern "C" fn run_unloaders() {
    EXITING.store(true, Ordering::SeqCst);
    let taken = match UNLOADERS.try_lock() {
        Ok(mut unloaders) => std::mem::take(&mut *unloaders),
        Err(_) => return,
    };
    for unload in taken {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(unload));
    }
}

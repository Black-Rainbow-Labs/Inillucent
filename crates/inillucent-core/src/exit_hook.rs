//! Drops every loaded model session before the process's C++ destructors run.
//!
//! Invariant: **a process that loaded a model exits with the exit code it
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
//! ## Linux crashed the other way round
//!
//! On Linux the same 2.0.3 programs exited with 0 under `idle` and `resident`
//! and crashed under `on-demand`, with `free(): invalid pointer`, a bus error or
//! a segmentation fault. The `ort` crate releases ONNX Runtime's environment at
//! exit from a function in the `.fini_array` of the library that links it. The
//! loader runs that after ONNX Runtime, which was loaded later, has already run
//! its own destructors. When a session is still loaded it holds a reference to
//! the environment and the release does nothing. Under `on-demand` the session
//! was already gone, so the release was the last reference and called into a
//! library that had been torn down. [`register`] keeps one reference to the
//! environment for the life of the process on Linux, so that late release is
//! never the last one. The environment is never released, and the process is
//! exiting anyway.
//!
//! ## Windows
//!
//! On Windows `ort` releases the environment from a thread local callback at
//! process exit, after the loader may already have detached ONNX Runtime. The
//! 2.0.3 command line, run 30 times with `INILLUCENT_EMBED_RESIDENCY=on-demand`,
//! crashed at exit in 10 of them. So Windows keeps the same one reference Linux
//! does. It needs no `atexit` handler: the loader stops the program's threads
//! before it unloads a DLL, so a session still loaded at exit is left alone.

#[cfg(unix)]
use std::sync::Mutex;
use std::sync::OnceLock;

/// Something that drops one holder's session when the process exits.
///
/// It holds a weak handle, so a holder that is dropped before the exit is not
/// kept alive by being registered here.
#[cfg(unix)]
type Unloader = Box<dyn Fn() + Send>;

/// Every holder that has loaded a session in this process.
#[cfg(unix)]
static UNLOADERS: Mutex<Vec<Unloader>> = Mutex::new(Vec::new());

/// Whether the `atexit` handler has been installed.
#[cfg(unix)]
static INSTALLED: OnceLock<bool> = OnceLock::new();

/// One reference to ONNX Runtime's environment, held until the process ends.
///
/// Linux only; the module comment says why the `ort` crate's own release at
/// exit must never be the last reference there. On macOS that release runs
/// before ONNX Runtime's destructors and is the right thing to let happen.
#[cfg(not(target_vendor = "apple"))]
static KEPT_ENVIRONMENT: OnceLock<Option<std::sync::Arc<ort::environment::Environment>>> =
    OnceLock::new();

#[cfg(unix)]
extern "C" {
    /// The C runtime's exit handler registration, from the C standard library.
    fn atexit(callback: extern "C" fn()) -> std::os::raw::c_int;
}

/// Asks for `unload` to be called when the process exits.
///
/// Call it after a session has opened, never before: the handler has to be
/// registered after ONNX Runtime was loaded for it to run before ONNX Runtime's
/// own destructors. A holder calls it once, the first time it loads.
/// @param unload - drops the holder's session, if it still has one
#[cfg(unix)]
pub fn register(unload: Unloader) {
    if let Ok(mut unloaders) = UNLOADERS.lock() {
        unloaders.push(unload);
    }
    #[cfg(not(target_vendor = "apple"))]
    KEPT_ENVIRONMENT.get_or_init(|| ort::environment::Environment::current().ok());
    INSTALLED.get_or_init(|| {
        // SAFETY: `atexit` is the C standard library's function, declared
        // above with its real signature. `run_unloaders` is an `extern "C"`
        // function with no arguments that never unwinds, which is what `atexit`
        // requires of a handler. Registering it has no other effect.
        let status = unsafe { atexit(run_unloaders) };
        status == 0
    });
}

/// Keeps one reference to the environment on Windows; see the module comment.
/// @param _unload - unused, because Windows needs no exit handler
#[cfg(not(unix))]
pub fn register(_unload: Box<dyn Fn() + Send>) {
    KEPT_ENVIRONMENT.get_or_init(|| ort::environment::Environment::current().ok());
}

/// The `atexit` handler: drops every registered holder's session.
///
/// A panic here could not unwind into the C runtime, so it is caught and the
/// remaining holders are still unloaded. The list is taken with `try_lock`
/// because a thread stopped while holding it must not hang the exit.
#[cfg(unix)]
extern "C" fn run_unloaders() {
    let taken = match UNLOADERS.try_lock() {
        Ok(mut unloaders) => std::mem::take(&mut *unloaders),
        Err(_) => return,
    };
    for unload in taken {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unload()));
    }
}

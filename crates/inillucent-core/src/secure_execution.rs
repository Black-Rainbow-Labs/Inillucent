//! Whether the operating system started this process with privileges its caller does not have.
//!
//! Invariant: **the answer is `true` whenever the system's own loader runs this process in its
//! secure mode**, the mode in which it ignores `LD_LIBRARY_PATH`, `LD_PRELOAD` and the `DYLD_`
//! variables. It is asked through the call the loader itself answers from: `getauxval(AT_SECURE)`
//! on Linux and Android, and `issetugid()` on macOS, iOS and the BSDs. Windows has no setuid or
//! setgid program, so the answer there is `false`.
//!
//! [`crate::embed_onnx`] loads ONNX Runtime from a path that the environment chooses:
//! `ORT_DYLIB_PATH`, or the install folder that `INILLUCENT_HOME` or `HOME` names. `ort` also reads
//! `ORT_DYLIB_PATH` by itself the first time it is used. In an ordinary process that is the
//! caller's own choice about the caller's own process. In a setuid or setgid program it would let
//! the caller run a library of their choosing with the program's privileges. The loader drops its
//! own variables in that case and keeps going, but it cannot know about these, so this module is
//! how the engine does the same. Ignoring only `ORT_DYLIB_PATH` would not be enough, because
//! `HOME` chooses the install folder and the loader keeps `HOME`.
//!
//! The two functions are declared here rather than taken from the `libc` crate, as `atexit` is in
//! `exit_hook`, because each is one declaration from the C library every Rust program on these
//! systems already links.

/// `AT_SECURE` from `<elf.h>`: the auxiliary vector entry the kernel sets to 1 when the loader
/// must run in secure mode.
#[cfg(any(target_os = "linux", target_os = "android"))]
const AT_SECURE: std::os::raw::c_ulong = 23;

#[cfg(any(target_os = "linux", target_os = "android"))]
extern "C" {
    /// Reads one entry of the auxiliary vector the kernel handed this process, or 0 when there is
    /// no such entry. glibc has it from 2.16, musl from its first release, Android from API 18.
    fn getauxval(kind: std::os::raw::c_ulong) -> std::os::raw::c_ulong;
}

#[cfg(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "freebsd",
    target_os = "openbsd",
    target_os = "netbsd",
    target_os = "dragonfly"
))]
extern "C" {
    /// Returns nonzero when the process was made setuid or setgid by the program it runs, or
    /// changed its user or group since.
    fn issetugid() -> std::os::raw::c_int;
}

/// Whether this process runs with privileges its caller does not have, so a library path the
/// environment names must not be loaded.
///
/// On Linux `AT_SECURE` is set for a setuid or setgid program, for a program with file
/// capabilities, and when a security module asks for it, so it is wider than comparing the real
/// and effective user and group, and it is the same answer the loader acts on.
pub(crate) fn active() -> bool {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        // SAFETY: `getauxval` is the C library's function, declared above with its real signature.
        // It takes an integer, reads the auxiliary vector the kernel placed in this process's
        // memory at start, and returns an integer. It touches no memory of the caller's.
        unsafe { getauxval(AT_SECURE) != 0 }
    }
    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    ))]
    {
        // SAFETY: `issetugid` is the C library's function, declared above with its real
        // signature. It takes nothing, reads a flag the kernel keeps for the process, and returns
        // an integer.
        unsafe { issetugid() != 0 }
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd",
        target_os = "dragonfly"
    )))]
    {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test process is started by the person or the CI job running it, with no setuid or setgid
    /// bit, so the loader is not in secure mode and neither is this. An `active` that answered
    /// `true` everywhere would refuse every embedding and fail here.
    #[test]
    fn an_ordinary_process_is_not_in_secure_mode() {
        assert!(!active());
    }
}

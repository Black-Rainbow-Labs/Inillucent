//! Asks the NVIDIA driver how many CUDA cards there are, before ONNX Runtime is asked for one.
//!
//! Invariant: **a card the driver does not have is refused here, and ONNX Runtime's CUDA execution
//! provider is never loaded for it.** That provider writes into memory it has already freed when
//! it is unloaded after it failed to start: full page heap, enabled from `cdb` at process creation,
//! stops every such process at exit on `lock xadd dword ptr [rbx+8]` in
//! `onnxruntime_providers_cuda`, the release of a static `std::shared_ptr` whose control block was
//! already freed, called from `onnxruntime.dll`'s own static destructors. Without page heap the
//! write lands in a free heap block, and about one process in a hundred that asked for
//! `INILLUCENT_EMBED_DEVICE=cuda:99` then ended with `STATUS_ACCESS_VIOLATION` somewhere else in
//! its exit, in whichever library freed into the damaged heap next. A run on the processor, a run
//! on a card that exists, and a session dropped before exit all end cleanly under full page heap.
//!
//! The driver library ships with the NVIDIA driver, not with the CUDA toolkit, so the probe needs
//! no toolkit version. `cuInit` and `cuDeviceGetCount` count the cards `CUDA_VISIBLE_DEVICES`
//! leaves visible, the same ordinals the CUDA runtime and ONNX Runtime number. When the library or
//! either function is missing the answer is `None`, and the caller asks ONNX Runtime as it always
//! did.

/// The driver library's name on this operating system.
#[cfg(windows)]
const DRIVER_LIBRARY: &str = "nvcuda.dll";
/// The driver library's name on this operating system.
#[cfg(target_os = "linux")]
const DRIVER_LIBRARY: &str = "libcuda.so.1";

/// `CUresult cuInit(unsigned int flags)`.
// SAFETY: a type and not a call. It is the declaration in `cuda.h`, and each call below says why
// it is sound.
#[cfg(any(windows, target_os = "linux"))]
type CuInit = unsafe extern "system" fn(u32) -> i32;
/// `CUresult cuDeviceGetCount(int *count)`.
// SAFETY: a type and not a call, as above.
#[cfg(any(windows, target_os = "linux"))]
type CuDeviceGetCount = unsafe extern "system" fn(*mut i32) -> i32;

/// `CUDA_ERROR_NO_DEVICE`: the driver is installed and sees no card.
#[cfg(any(windows, target_os = "linux"))]
const CUDA_ERROR_NO_DEVICE: i32 = 100;

/// Returns how many CUDA cards the driver can see, or `None` when the driver cannot be asked.
///
/// The library stays loaded for the life of the process. ONNX Runtime loads the same library to
/// run on a card, and unloading it between the two would only make it load twice.
#[cfg(any(windows, target_os = "linux"))]
pub(crate) fn device_count() -> Option<i32> {
    // SAFETY: loading the NVIDIA driver library runs its initialisation routine, which is what
    // every CUDA program does first; it takes no argument from this process.
    let library = unsafe { libloading::Library::new(DRIVER_LIBRARY) }.ok()?;
    // SAFETY: the two names are the driver API's own exported functions, and the types above are
    // their declarations in `cuda.h`: an unsigned int in and a `CUresult` out, and a pointer to an
    // int out and a `CUresult` out. `system` is the calling convention the driver exports them in.
    let init: CuInit = *unsafe { library.get::<CuInit>(b"cuInit\0") }.ok()?;
    // SAFETY: as above.
    let count_of: CuDeviceGetCount =
        *unsafe { library.get::<CuDeviceGetCount>(b"cuDeviceGetCount\0") }.ok()?;
    // SAFETY: `cuInit` takes the flags, which must be zero, and touches no memory of the caller's.
    let initialised = unsafe { init(0) };
    if initialised == CUDA_ERROR_NO_DEVICE {
        std::mem::forget(library);
        return Some(0);
    }
    if initialised != 0 {
        std::mem::forget(library);
        return None;
    }
    let mut count: i32 = 0;
    // SAFETY: the pointer is to a local `i32` that lives across the call, which is the one place
    // the function writes.
    let counted = unsafe { count_of(&mut count) };
    std::mem::forget(library);
    (counted == 0).then_some(count)
}

/// Returns `None` where there is no NVIDIA driver library to ask: ONNX Runtime is asked instead.
#[cfg(not(any(windows, target_os = "linux")))]
pub(crate) fn device_count() -> Option<i32> {
    None
}

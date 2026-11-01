//! Memory a caller borrows from the library to hand bytes in.
//!
//! Invariant: a buffer is allocated and freed by this library's own allocator,
//! with the length it was asked for, and with nothing else.
//!
//! A C program has `malloc` and can point at its own memory, so it never needs
//! these. A WebAssembly host cannot: JavaScript sees the module's linear memory
//! as one byte array and has no allocator inside it, so a SQL string or a
//! blob has nowhere to go until the module gives it room. The web package calls
//! `inillucent_buffer_alloc`, copies the bytes in, passes the pointer to
//! `inillucent_execute` or `inillucent_bind_blob`, and frees it straight after.
//! The functions are exported on every platform, so the header, `abi.toml` and
//! the library agree everywhere and a test on the development machine checks
//! them.

use std::alloc::Layout;

use crate::capi::error::guarded_value;

/// The alignment every buffer has: enough for any scalar a caller might write
/// into one, so a host can place an `int64_t` or a `double` there.
const ALIGN: usize = 8;

/// Returns the layout a buffer of `len` bytes was, or is to be, allocated with.
///
/// A zero length is allocated as one byte, because the allocator refuses a
/// zero sized request and a caller with an empty string still needs a pointer.
///
/// @param len - the length the caller asked for
fn layout(len: usize) -> Option<Layout> {
    Layout::from_size_align(len.max(1), ALIGN).ok()
}

/// Allocates `len` bytes the caller may write into, or returns null.
///
/// The bytes are not initialised. Free the buffer with `inillucent_buffer_free`
/// and the same `len`.
///
/// @param len - how many bytes
#[no_mangle]
pub extern "C" fn inillucent_buffer_alloc(len: usize) -> *mut u8 {
    guarded_value(
        || match layout(len) {
            // SAFETY: the layout has a non zero size, which is all `alloc` requires.
            Some(layout) => unsafe { std::alloc::alloc(layout) },
            None => std::ptr::null_mut(),
        },
        std::ptr::null_mut(),
    )
}

/// Frees a buffer `inillucent_buffer_alloc` returned. Null is ignored.
///
/// # Safety
///
/// `buffer` must be null or a pointer `inillucent_buffer_alloc(len)` returned,
/// not already freed, and `len` must be the length it was allocated with.
///
/// @param buffer - the buffer
/// @param len - the length it was allocated with
#[no_mangle]
pub unsafe extern "C" fn inillucent_buffer_free(buffer: *mut u8, len: usize) {
    guarded_value(
        || {
            if buffer.is_null() {
                return;
            }
            if let Some(layout) = layout(len) {
                // SAFETY: the caller promises the pointer came from `alloc` with
                // this same layout and has not been freed.
                unsafe { std::alloc::dealloc(buffer, layout) };
            }
        },
        (),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A buffer holds what was written into it until it is freed, and a zero
    /// length still answers a pointer.
    #[test]
    fn a_buffer_round_trips_bytes_and_an_empty_one_is_not_null() {
        let buffer = inillucent_buffer_alloc(5);
        assert!(!buffer.is_null());
        // SAFETY: five bytes were just allocated.
        let written = unsafe {
            std::ptr::copy_nonoverlapping(b"hello".as_ptr(), buffer, 5);
            std::slice::from_raw_parts(buffer, 5).to_vec()
        };
        assert_eq!(written, b"hello");
        // SAFETY: freed once, with the length it was allocated with.
        unsafe { inillucent_buffer_free(buffer, 5) };
        let empty = inillucent_buffer_alloc(0);
        assert!(!empty.is_null());
        // SAFETY: as above.
        unsafe { inillucent_buffer_free(empty, 0) };
        // SAFETY: null is documented as ignored.
        unsafe { inillucent_buffer_free(std::ptr::null_mut(), 3) };
    }
}

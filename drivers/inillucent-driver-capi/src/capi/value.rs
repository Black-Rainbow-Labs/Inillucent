//! The result set handle, and reading one value out of it.
//!
//! Invariant: **a pointer a reader hands back borrows the result set.** The
//! text and blob accessors return a pointer into the rows the handle owns,
//! which is live until that handle is freed and not one moment longer.

use std::ffi::{c_char, CString};

use inillucent_driver::{Rows, Value};

use crate::capi::error::*;
use crate::*;

/// A materialised result.
pub struct inillucent_rows {
    /// The word this handle carries while it is alive - see [`Live`].
    live: Live,
    /// The rows themselves.
    rows: Rows,
    /// Column names as C strings, built once so a pointer into one is stable.
    names: Vec<CString>,
    /// Declared types as C strings, likewise.
    types: Vec<CString>,
    /// The completion tag as a C string.
    tag: CString,
    /// The whole result as JSON, written the first time it is asked for.
    ///
    /// Kept here for the reason the names are: the header promises a pointer
    /// valid until the result is freed.
    json: std::cell::OnceCell<CString>,
}
/// Turns a driver result into the handle, with its strings in C form.
///
/// The C strings are built **here rather than on demand**, because the header
/// promises a pointer valid until the result is freed and one built inside an
/// accessor would be freed when that accessor returned.
///
/// @param rows - the driver's result
pub(crate) fn built(rows: Rows) -> inillucent_rows {
    let names = rows.columns.iter().map(|c| c_string(&c.name)).collect();
    let types = rows
        .columns
        .iter()
        .map(|c| c_string(&c.declared_type))
        .collect();
    let tag = c_string(&rows.tag);
    inillucent_rows {
        live: Live::new(inillucent_rows::MAGIC),
        rows,
        names,
        types,
        tag,
        json: std::cell::OnceCell::new(),
    }
}
/// Turns a C caller's `uint64_t` limit into a `usize`, saturating.
///
/// A 32-bit build cannot hold `UINT64_MAX`, and the shape a caller writes for
/// "no limit" is exactly that - so saturating is what makes `~0ull` mean what
/// it obviously means rather than wrapping to nothing.
///
/// @param limit - what the caller asked for
pub(crate) fn capped(limit: u64) -> usize {
    usize::try_from(limit).unwrap_or(usize::MAX)
}
/// Frees a result.
///
/// @param rows - the result
///
/// # Safety
///
/// `rows` must be null or a live handle, freed exactly once.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_free(rows: *mut inillucent_rows) {
    guarded_value(
        || {
            if !rows.is_null() {
                if held(rows as *const inillucent_rows).is_none() {
                    return;
                }
                drop(reclaim(rows));
            }
        },
        (),
    )
}
/// Returns how many columns a result has.
///
/// @param rows - the result
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_column_count(rows: *const inillucent_rows) -> usize {
    guarded_value(|| held(rows).map_or(0, |held| held.rows.columns.len()), 0)
}
/// Returns one column's name.
///
/// @param rows - the result
/// @param nth - the column, from zero
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_column_name(
    rows: *const inillucent_rows,
    nth: usize,
) -> *const c_char {
    guarded_value(
        || match held(rows).and_then(|held| held.names.get(nth)) {
            Some(name) => name.as_ptr(),
            None => std::ptr::null(),
        },
        std::ptr::null(),
    )
}
/// Returns one column's declared type, or the empty string for an expression.
///
/// @param rows - the result
/// @param nth - the column, from zero
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_column_type(
    rows: *const inillucent_rows,
    nth: usize,
) -> *const c_char {
    guarded_value(
        || match held(rows).and_then(|held| held.types.get(nth)) {
            Some(name) => name.as_ptr(),
            None => std::ptr::null(),
        },
        std::ptr::null(),
    )
}
/// Returns how many rows the caller was handed.
///
/// @param rows - the result
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_count(rows: *const inillucent_rows) -> usize {
    guarded_value(|| held(rows).map_or(0, |held| held.rows.rows.len()), 0)
}
/// Returns how many rows the statement produced, exactly.
///
/// @param rows - the result
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_total(rows: *const inillucent_rows) -> usize {
    guarded_value(|| held(rows).map_or(0, |held| held.rows.total), 0)
}
/// Reports whether the limit cut anything off.
///
/// @param rows - the result
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_more(rows: *const inillucent_rows) -> i32 {
    guarded_value(|| held(rows).map_or(0, |held| i32::from(held.rows.more)), 0)
}
/// Returns how many rows the statement changed, or -1 for a query.
///
/// @param rows - the result
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_affected(rows: *const inillucent_rows) -> i64 {
    guarded_value(
        || {
            held(rows).map_or(-1, |held| {
                held.rows
                    .affected
                    .map_or(-1, |count| i64::try_from(count).unwrap_or(i64::MAX))
            })
        },
        0,
    )
}
/// Returns how long the statement took, in microseconds.
///
/// @param rows - the result
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_elapsed_us(rows: *const inillucent_rows) -> u64 {
    guarded_value(
        || {
            held(rows).map_or(0, |held| {
                u64::try_from(held.rows.elapsed.as_micros()).unwrap_or(u64::MAX)
            })
        },
        0,
    )
}
/// Returns the completion tag.
///
/// @param rows - the result
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_tag(rows: *const inillucent_rows) -> *const c_char {
    guarded_value(
        || match held(rows) {
            Some(held) => held.tag.as_ptr(),
            None => std::ptr::null(),
        },
        std::ptr::null(),
    )
}
/// Returns what kind of value a cell holds.
///
/// @param rows - the result
/// @param row - the row, from zero
/// @param column - the column, from zero
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_value_type(
    rows: *const inillucent_rows,
    row: usize,
    column: usize,
) -> i32 {
    guarded_value(
        || match held(rows).and_then(|held| held.rows.value(row, column)) {
            Some(value) => value.kind() as i32,
            None => 0,
        },
        0,
    )
}
/// Returns a cell as an integer, or zero when it is not one.
///
/// @param rows - the result
/// @param row - the row, from zero
/// @param column - the column, from zero
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_value_int(
    rows: *const inillucent_rows,
    row: usize,
    column: usize,
) -> i64 {
    guarded_value(
        || match held(rows).and_then(|held| held.rows.value(row, column)) {
            Some(Value::Integer(number)) => *number,
            Some(Value::Real(number)) => *number as i64,
            _ => 0,
        },
        0,
    )
}
/// Returns a cell as a float, or zero when it is not one.
///
/// @param rows - the result
/// @param row - the row, from zero
/// @param column - the column, from zero
///
/// # Safety
///
/// `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_value_real(
    rows: *const inillucent_rows,
    row: usize,
    column: usize,
) -> f64 {
    guarded_value(
        || match held(rows).and_then(|held| held.rows.value(row, column)) {
            Some(Value::Real(number)) => *number,
            Some(Value::Integer(number)) => *number as f64,
            _ => 0.0,
        },
        0.0,
    )
}
/// Returns a cell's bytes and their length.
///
/// **Not NUL-terminated**, because a text value may contain a NUL byte and
/// truncating there would lose data with nothing to say it had.
///
/// @param rows - the result
/// @param row - the row, from zero
/// @param column - the column, from zero
/// @param len - where the length goes
///
/// # Safety
///
/// `rows` must be null or a live handle and `len` must be null or writable.
#[no_mangle]
pub unsafe extern "C" fn inillucent_value_bytes(
    rows: *const inillucent_rows,
    row: usize,
    column: usize,
    len: *mut usize,
) -> *const u8 {
    guarded_value(
        || {
            let bytes = held(rows)
                .and_then(|held| held.rows.value(row, column))
                .and_then(Value::bytes);
            match bytes {
                Some(bytes) => {
                    if !len.is_null() {
                        *len = bytes.len();
                    }
                    bytes.as_ptr()
                }
                None => {
                    if !len.is_null() {
                        *len = 0;
                    }
                    std::ptr::null()
                }
            }
        },
        std::ptr::null(),
    )
}

/// Returns the whole result as one JSON text, and its length in bytes.
///
/// **One call for the whole result, for a binding that pays for every call.**
/// Python's `ctypes` costs about half a microsecond a call and the cell
/// accessors take two calls a cell, so a result of 20,000 rows of five columns
/// took 170 ms to read through them while the engine produced it in 12 ms
/// (task-2191). The text is parsed by the host's own JSON reader instead.
///
/// The object holds `columns`, `types`, `rows`, `total`, `more`, `affected`,
/// `elapsed_us` and `tag`. A value is `null`, an integer, a real that always
/// has a fraction or an exponent, a string, `{"blob": "<hex>"}`, or for a real
/// JSON cannot spell, `{"real": "Infinity"}`, `{"real": "-Infinity"}` or
/// `{"real": "NaN"}`. The text is NUL terminated and holds no other NUL,
/// because JSON escapes one inside a string.
///
/// @param rows - the result
/// @param len - where the length in bytes goes, not counting the NUL, or null
///
/// # Safety
///
/// `rows` must be null or a live handle and `len` must be null or writable.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_json(
    rows: *const inillucent_rows,
    len: *mut usize,
) -> *const c_char {
    guarded_value(
        || {
            let Some(held) = held(rows) else {
                if !len.is_null() {
                    *len = 0;
                }
                return std::ptr::null();
            };
            // Taken by value, so the text is not copied a second time. JSON has
            // no raw NUL, so the refusal is unreachable; `null` keeps the
            // promise of a parseable text if it ever were reached.
            let text = held.json.get_or_init(|| {
                CString::new(inillucent_driver::wire::rows_to_json(&held.rows))
                    .unwrap_or_else(|_| c"null".to_owned())
            });
            if !len.is_null() {
                *len = text.as_bytes().len();
            }
            text.as_ptr()
        },
        std::ptr::null(),
    )
}

impl inillucent_rows {
    /// Returns the driver's result this handle holds.
    pub(crate) fn result(&self) -> &Rows {
        &self.rows
    }
}

impl Handle for inillucent_rows {
    const MAGIC: u32 = 0x5244_4235;
    fn live(&self) -> &Live {
        &self.live
    }
}

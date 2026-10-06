//! A result built as Python objects, for a binding running inside CPython.
//!
//! **Why the library builds Python objects at all** (task-2191). The Python
//! binding reaches this library through `ctypes`, which costs about half a
//! microsecond a call, so reading a result cell by cell was 170 ms for 20,000
//! rows. Reading it as one JSON text brought that to 13.5 ms, and Python's own
//! `sqlite3` module takes 7.8 ms, because it builds each value with CPython's
//! constructors as it steps. This module does the same: the binding hands over
//! the addresses of CPython's constructors once, and a result comes back as
//! Python lists and values with no text in between.
//!
//! **No CPython is linked.** The addresses come from `ctypes.pythonapi` in the
//! process that loaded the library, so the library works under whichever
//! Python loaded it and does not require one when nobody calls this. Every
//! function used is part of CPython's limited API, whose signatures have not
//! changed since Python 3.2.
//!
//! Invariant: **this module calls CPython only while the caller holds the
//! interpreter lock.** The binding calls `inillucent_rows_py` through
//! `ctypes.PyDLL`, which keeps the lock across the call, and nothing here
//! starts a thread.

use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

use inillucent_driver::Value;

use crate::capi::value::inillucent_rows;
use crate::*;

/// `PyObject *`, which this module never looks inside.
type PyObject = c_void;

/// The CPython functions this module calls, in the order
/// `inillucent_py_init` receives their addresses.
struct PyApi {
    /// `PyList_New(Py_ssize_t)`.
    list_new: unsafe extern "C" fn(isize) -> *mut PyObject,
    /// `PyList_SetItem(list, index, item)`, which takes the item's reference.
    list_set_item: unsafe extern "C" fn(*mut PyObject, isize, *mut PyObject) -> c_int,
    /// `PyLong_FromLongLong(long long)`.
    long_from_long_long: unsafe extern "C" fn(i64) -> *mut PyObject,
    /// `PyFloat_FromDouble(double)`.
    float_from_double: unsafe extern "C" fn(f64) -> *mut PyObject,
    /// `PyUnicode_FromStringAndSize(const char *, Py_ssize_t)`.
    unicode_from_string_and_size: unsafe extern "C" fn(*const c_char, isize) -> *mut PyObject,
    /// `PyBytes_FromStringAndSize(const char *, Py_ssize_t)`.
    bytes_from_string_and_size: unsafe extern "C" fn(*const c_char, isize) -> *mut PyObject,
    /// `Py_IncRef(PyObject *)`.
    incref: unsafe extern "C" fn(*mut PyObject),
    /// `Py_DecRef(PyObject *)`.
    decref: unsafe extern "C" fn(*mut PyObject),
    /// `PyErr_SetString(PyObject *type, const char *message)`.
    err_set_string: unsafe extern "C" fn(*mut PyObject, *const c_char),
    /// The address of `None`.
    none: usize,
    /// The address of the exception type a misuse raises.
    error_type: usize,
}

/// How many addresses `inillucent_py_init` takes.
const PY_API_LEN: usize = 11;

/// The table, set once by the first `inillucent_py_init`.
static PY_API: OnceLock<PyApi> = OnceLock::new();

/// Reads an address as the function pointer type it is the address of.
///
/// `transmute_copy` because the type is generic here; the size is checked,
/// and every caller names the type through the field it fills.
///
/// @param address - the address of a function of type `F`
///
/// # Safety
///
/// `F` must be a function pointer type and `address` a function of that type.
pub(crate) unsafe fn as_function<F: Copy>(address: *const c_void) -> F {
    debug_assert_eq!(
        std::mem::size_of::<F>(),
        std::mem::size_of::<*const c_void>()
    );
    // SAFETY: as the caller promises, `F` is a function pointer, which has the
    // size and representation of an address.
    unsafe { std::mem::transmute_copy::<*const c_void, F>(&address) }
}

/// Takes the addresses of the CPython functions `inillucent_rows_py` calls.
///
/// The order is `PyList_New`, `PyList_SetItem`, `PyLong_FromLongLong`,
/// `PyFloat_FromDouble`, `PyUnicode_FromStringAndSize`,
/// `PyBytes_FromStringAndSize`, `Py_IncRef`, `Py_DecRef`, `PyErr_SetString`,
/// then the addresses of the `None` object and of the exception type to raise
/// on a misuse. One process holds one CPython, so the first table is kept and
/// a later call with a different one is refused.
///
/// @param api - the addresses, in that order
/// @param count - how many there are, which must be 11
///
/// # Safety
///
/// `api` must point to `count` readable pointers, each the address of the
/// CPython function or object named above, in the CPython of this process.
#[no_mangle]
pub unsafe extern "C" fn inillucent_py_init(api: *const *const c_void, count: usize) -> i32 {
    guarded_value(
        || {
            if api.is_null() || count != PY_API_LEN {
                return INILLUCENT_MISUSE;
            }
            let at = std::slice::from_raw_parts(api, count);
            if at.iter().any(|address| address.is_null()) {
                return INILLUCENT_MISUSE;
            }
            let address = |nth: usize| at.get(nth).copied().unwrap_or(std::ptr::null());
            let table = PyApi {
                list_new: as_function(address(0)),
                list_set_item: as_function(address(1)),
                long_from_long_long: as_function(address(2)),
                float_from_double: as_function(address(3)),
                unicode_from_string_and_size: as_function(address(4)),
                bytes_from_string_and_size: as_function(address(5)),
                incref: as_function(address(6)),
                decref: as_function(address(7)),
                err_set_string: as_function(address(8)),
                none: address(9) as usize,
                error_type: address(10) as usize,
            };
            let first = address(0) as usize;
            let kept = PY_API.get_or_init(|| table);
            match kept.list_new as usize == first {
                true => INILLUCENT_OK,
                false => INILLUCENT_MISUSE,
            }
        },
        INILLUCENT_INTERNAL,
    )
}

/// Builds a whole result as Python objects and returns a new reference to it.
///
/// The object is a list: the column names, the declared types, the rows (a
/// list of lists of values), `total`, `more` as 0 or 1, `affected` or `None`
/// for a query, `elapsed_us`, and the tag. A value is `None`, an `int`, a
/// `float`, a `str` or `bytes`, which is what the cell accessors say it is.
/// On a failure it returns null with a Python exception set, which is what
/// `ctypes.PyDLL` turns into a raise.
///
/// @param rows - the result
///
/// # Safety
///
/// Call it only through `ctypes.PyDLL`, with the interpreter lock held, after
/// `inillucent_py_init` succeeded. `rows` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_rows_py(rows: *const inillucent_rows) -> *mut c_void {
    guarded_value(
        || {
            let Some(api) = PY_API.get() else {
                return std::ptr::null_mut();
            };
            let Some(held) = held(rows) else {
                (api.err_set_string)(
                    api.error_type as *mut PyObject,
                    c"inillucent_rows_py was given a result that is not live".as_ptr(),
                );
                return std::ptr::null_mut();
            };
            build_result(api, held.result()).unwrap_or(std::ptr::null_mut())
        },
        std::ptr::null_mut(),
    )
}

/// Owns one reference until it is handed on, and gives it back if it is not.
struct Owned<'a> {
    /// The table that knows how to give it back.
    api: &'a PyApi,
    /// The object, null once handed on.
    object: *mut PyObject,
}

impl<'a> Owned<'a> {
    /// Takes a new reference, or `None` for null, which means CPython has set
    /// an exception.
    ///
    /// @param api - the table
    /// @param object - the new reference
    fn new(api: &'a PyApi, object: *mut PyObject) -> Option<Owned<'a>> {
        (!object.is_null()).then_some(Owned { api, object })
    }

    /// Hands the reference on, so it is no longer given back here.
    fn release(mut self) -> *mut PyObject {
        std::mem::replace(&mut self.object, std::ptr::null_mut())
    }
}

impl Drop for Owned<'_> {
    /// Gives a reference back that was never handed on, on a failure path.
    fn drop(&mut self) {
        if !self.object.is_null() {
            // SAFETY: the reference is ours, and the caller holds the lock.
            unsafe { (self.api.decref)(self.object) };
        }
    }
}

/// Builds a list from items, giving every one back if any step fails.
///
/// @param api - the table
/// @param count - how many items
/// @param item - builds the item at a position, as a new reference
unsafe fn list_of<'a>(
    api: &'a PyApi,
    count: usize,
    mut item: impl FnMut(usize) -> Option<*mut PyObject>,
) -> Option<Owned<'a>> {
    let length = isize::try_from(count).ok()?;
    let list = Owned::new(api, (api.list_new)(length))?;
    for at in 0..count {
        let made = item(at)?;
        // `PyList_SetItem` takes the reference whether or not it succeeds.
        if (api.list_set_item)(list.object, at as isize, made) != 0 {
            return None;
        }
    }
    Some(list)
}

/// Builds one value as a new reference.
///
/// @param api - the table
/// @param value - the cell
unsafe fn value_of(api: &PyApi, value: &Value) -> Option<*mut PyObject> {
    let made = match value {
        Value::Null => {
            (api.incref)(api.none as *mut PyObject);
            api.none as *mut PyObject
        }
        Value::Integer(number) => (api.long_from_long_long)(*number),
        Value::Real(number) => (api.float_from_double)(*number),
        Value::Text(text) => text_of(api, text),
        Value::Blob(bytes) => {
            (api.bytes_from_string_and_size)(bytes.as_ptr().cast(), bytes.len() as isize)
        }
    };
    (!made.is_null()).then_some(made)
}

/// Builds a `str` from text the driver already holds as UTF-8.
///
/// @param api - the table
/// @param text - the text
unsafe fn text_of(api: &PyApi, text: &str) -> *mut PyObject {
    (api.unicode_from_string_and_size)(text.as_ptr().cast(), text.len() as isize)
}

/// Builds the result list `inillucent_rows_py` returns.
///
/// @param api - the table
/// @param rows - the driver's result
unsafe fn build_result(api: &PyApi, rows: &inillucent_driver::Rows) -> Option<*mut PyObject> {
    let columns = list_of(api, rows.columns.len(), |at| {
        let column = rows.columns.get(at)?;
        let made = text_of(api, &column.name);
        (!made.is_null()).then_some(made)
    })?;
    let types = list_of(api, rows.columns.len(), |at| {
        let column = rows.columns.get(at)?;
        let made = text_of(api, &column.declared_type);
        (!made.is_null()).then_some(made)
    })?;
    let body = list_of(api, rows.rows.len(), |at| {
        let row = rows.rows.get(at)?;
        let built = list_of(api, row.len(), |column| value_of(api, row.get(column)?))?;
        Some(built.release())
    })?;
    let affected = match rows.affected {
        Some(count) => (api.long_from_long_long)(i64::try_from(count).unwrap_or(i64::MAX)),
        None => {
            (api.incref)(api.none as *mut PyObject);
            api.none as *mut PyObject
        }
    };
    let affected = Owned::new(api, affected)?;
    let total = Owned::new(
        api,
        (api.long_from_long_long)(i64::try_from(rows.total).unwrap_or(i64::MAX)),
    )?;
    let more = Owned::new(api, (api.long_from_long_long)(i64::from(rows.more)))?;
    let elapsed = Owned::new(
        api,
        (api.long_from_long_long)(i64::try_from(rows.elapsed.as_micros()).unwrap_or(i64::MAX)),
    )?;
    let tag = Owned::new(api, text_of(api, &rows.tag))?;
    let mut parts = [columns, types, body, total, more, affected, elapsed, tag].map(Some);
    let whole = list_of(api, parts.len(), |at| {
        Some(parts.get_mut(at)?.take()?.release())
    })?;
    Some(whole.release())
}

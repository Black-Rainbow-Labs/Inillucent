//! Parameters for many executions, read out of Python objects, for a binding
//! running inside CPython.
//!
//! **Why the library reads Python objects at all** (task-2191). The Python
//! binding's `execute_many` wrote every row as JSON and the library parsed it
//! back: for 10,000 rows of a table with a long text column that was about
//! 3 ms of `json.dumps` and 2 ms of parsing, where Python's own `sqlite3`
//! module reads each value straight out of its object. This module does the
//! same with addresses of CPython's own functions, handed over once, the way
//! `python.rs` builds results.
//!
//! Invariant: **a value is read only when its type is exactly one this module
//! knows, and anything else answers "not here" rather than a guess.** The
//! binding then sends the rows as JSON, which is the path that reports a value
//! it cannot bind and refuses an integer wider than 64 bits. So every refusal
//! and every error message is the JSON path's, unchanged, and this module only
//! ever makes a batch it can read faster.
//!
//! The reading half runs with the interpreter lock held, through
//! `ctypes.PyDLL`. The running half takes no Python object and is called
//! through `ctypes.CDLL`, which lets other Python threads run while the
//! statement does.

use std::ffi::{c_char, c_int, c_void};
use std::sync::OnceLock;

use inillucent_driver::Value;

use crate::capi::error::*;
use crate::capi::python::as_function;
use crate::capi::stmt::{inillucent_stmt, run_rows};
use crate::*;

/// `PyObject *`, read here only for its type.
type PyObject = c_void;

/// The values of many executions, read by [`inillucent_py_params`] and run by
/// [`inillucent_stmt_execute_params`].
pub struct inillucent_params {
    /// The values of each execution.
    rows: Vec<Vec<Value>>,
    /// The list or tuple they were read from, a borrowed reference the
    /// binding keeps alive until `inillucent_stmt_execute_params` returns. It
    /// is read a second time only when the rows' one statement fails; see
    /// `read_again`.
    source: *mut PyObject,
}

/// The CPython functions and type objects this module reads with, in the
/// order `inillucent_py_init_params` receives their addresses.
struct PyRead {
    /// `PyList_Size(list)`.
    list_size: unsafe extern "C" fn(*mut PyObject) -> isize,
    /// `PyList_GetItem(list, index)`, a borrowed reference.
    list_get_item: unsafe extern "C" fn(*mut PyObject, isize) -> *mut PyObject,
    /// `PyTuple_Size(tuple)`.
    tuple_size: unsafe extern "C" fn(*mut PyObject) -> isize,
    /// `PyTuple_GetItem(tuple, index)`, a borrowed reference.
    tuple_get_item: unsafe extern "C" fn(*mut PyObject, isize) -> *mut PyObject,
    /// `PyLong_AsLongLong(int)`, which sets an error for one wider than 64 bits.
    long_as_long_long: unsafe extern "C" fn(*mut PyObject) -> i64,
    /// `PyFloat_AsDouble(float)`.
    float_as_double: unsafe extern "C" fn(*mut PyObject) -> f64,
    /// `PyUnicode_AsUTF8AndSize(str, size)`, the text the `str` keeps.
    unicode_as_utf8_and_size: unsafe extern "C" fn(*mut PyObject, *mut isize) -> *const c_char,
    /// `PyBytes_AsStringAndSize(bytes, buffer, size)`.
    bytes_as_string_and_size:
        unsafe extern "C" fn(*mut PyObject, *mut *mut c_char, *mut isize) -> c_int,
    /// `PyErr_Clear()`.
    err_clear: unsafe extern "C" fn(),
    /// `PyErr_Occurred()`, a borrowed reference or null.
    err_occurred: unsafe extern "C" fn() -> *mut PyObject,
    /// `PyGILState_Ensure()`, which takes the interpreter lock on a thread
    /// that let it go and answers the state to give back.
    gil_ensure: unsafe extern "C" fn() -> c_int,
    /// `PyGILState_Release(state)`.
    gil_release: unsafe extern "C" fn(c_int),
    /// `list`.
    list_type: usize,
    /// `tuple`.
    tuple_type: usize,
    /// `int`.
    int_type: usize,
    /// `bool`.
    bool_type: usize,
    /// `float`.
    float_type: usize,
    /// `str`.
    str_type: usize,
    /// `bytes`.
    bytes_type: usize,
    /// `type(None)`.
    none_type: usize,
}

/// Where the `None` object sits in the addresses `inillucent_py_init_params`
/// takes, which is also how many come before it.
const PY_READ_LEN: usize = 20;

/// The table, set once by the first `inillucent_py_init_params`.
static PY_READ: OnceLock<PyRead> = OnceLock::new();

/// Returns an object's type, from the object's header.
///
/// `ob_type` follows the reference count in every CPython build that keeps
/// the two in one word each, which is every build but the free threaded one.
/// `inillucent_py_init_params` checks it on `None` before keeping the table,
/// so a build that lays the header out otherwise never reaches this.
///
/// @param object - a live Python object
///
/// # Safety
///
/// `object` must point to a live Python object, with the interpreter lock held.
unsafe fn type_of(object: *mut PyObject) -> usize {
    // SAFETY: as the caller promises, the object is live, and its header is
    // at least two words long.
    unsafe { (object as *const usize).add(1).read() }
}

/// Takes the addresses of the CPython functions and types this module reads
/// with.
///
/// The order is `PyList_Size`, `PyList_GetItem`, `PyTuple_Size`,
/// `PyTuple_GetItem`, `PyLong_AsLongLong`, `PyFloat_AsDouble`,
/// `PyUnicode_AsUTF8AndSize`, `PyBytes_AsStringAndSize`, `PyErr_Clear`,
/// `PyErr_Occurred`, then
/// the type objects `list`, `tuple`, `int`, `bool`, `float`, `str`, `bytes`
/// and `type(None)`, then `PyGILState_Ensure` and `PyGILState_Release`, then
/// the `None` object. The last is used once, here, to
/// check that an object's type is where `type_of` reads it, and the call is
/// refused when it is not. One process holds one CPython, so the first table
/// is kept and a later call with a different one is refused.
///
/// @param api - the addresses, in that order
/// @param count - how many there are, which must be 21
///
/// # Safety
///
/// `api` must point to `count` readable pointers, each the address of the
/// CPython function or object named above, in the CPython of this process,
/// and the caller must hold the interpreter lock.
#[no_mangle]
pub unsafe extern "C" fn inillucent_py_init_params(api: *const *const c_void, count: usize) -> i32 {
    guarded_value(
        || {
            if api.is_null() || count != PY_READ_LEN.saturating_add(1) {
                return INILLUCENT_MISUSE;
            }
            let at = std::slice::from_raw_parts(api, count);
            if at.iter().any(|address| address.is_null()) {
                return INILLUCENT_MISUSE;
            }
            let address = |nth: usize| at.get(nth).copied().unwrap_or(std::ptr::null());
            let none = address(PY_READ_LEN) as *mut PyObject;
            if type_of(none) != address(17) as usize {
                return INILLUCENT_MISUSE;
            }
            let table = PyRead {
                list_size: as_function(address(0)),
                list_get_item: as_function(address(1)),
                tuple_size: as_function(address(2)),
                tuple_get_item: as_function(address(3)),
                long_as_long_long: as_function(address(4)),
                float_as_double: as_function(address(5)),
                unicode_as_utf8_and_size: as_function(address(6)),
                bytes_as_string_and_size: as_function(address(7)),
                err_clear: as_function(address(8)),
                err_occurred: as_function(address(9)),
                list_type: address(10) as usize,
                tuple_type: address(11) as usize,
                int_type: address(12) as usize,
                bool_type: address(13) as usize,
                float_type: address(14) as usize,
                str_type: address(15) as usize,
                bytes_type: address(16) as usize,
                none_type: address(17) as usize,
                gil_ensure: as_function(address(18)),
                gil_release: as_function(address(19)),
            };
            let first = address(0) as usize;
            let kept = PY_READ.get_or_init(|| table);
            match kept.list_size as usize == first {
                true => INILLUCENT_OK,
                false => INILLUCENT_MISUSE,
            }
        },
        INILLUCENT_INTERNAL,
    )
}

/// Reads a list or tuple of rows, each a list or tuple of values, into
/// parameters for [`inillucent_stmt_execute_params`].
///
/// Answers null, with no Python exception set, when anything in it is not a
/// shape this reads: a sequence of another type, a value of another type, an
/// integer wider than 64 bits, or a `str` that cannot be written as UTF-8.
/// The caller then sends the rows the other way. See the module's invariant.
///
/// @param rows - the rows, a borrowed reference
///
/// # Safety
///
/// Call it only through `ctypes.PyDLL`, with the interpreter lock held, after
/// `inillucent_py_init_params` succeeded. `rows` must be a live Python object.
#[no_mangle]
pub unsafe extern "C" fn inillucent_py_params(rows: *mut c_void) -> *mut inillucent_params {
    guarded_value(
        || {
            let Some(api) = PY_READ.get() else {
                return std::ptr::null_mut();
            };
            if rows.is_null() {
                return std::ptr::null_mut();
            }
            match read_sequence(api, rows, |item| read_row(api, item)) {
                Some(read) => Box::into_raw(Box::new(inillucent_params {
                    rows: read,
                    source: rows,
                })),
                None => {
                    (api.err_clear)();
                    std::ptr::null_mut()
                }
            }
        },
        std::ptr::null_mut(),
    )
}

/// Runs a statement once for each row of `params`, as
/// `inillucent_stmt_execute_many` runs it for each array of its JSON, and
/// frees `params`.
///
/// @param stmt - the statement
/// @param params - what `inillucent_py_params` read, taken whatever happens
/// @param changed - where the total of rows changed goes, or null
/// @param error - where a failure goes, or null
///
/// # Safety
///
/// `stmt` must be a live handle, `params` null or a pointer
/// `inillucent_py_params` returned and nothing has used since, and `changed`
/// null or writable. The rows `params` was read from must stay alive until
/// this returns: when their one statement fails they are read again, with
/// the interpreter lock taken for the read.
#[no_mangle]
pub unsafe extern "C" fn inillucent_stmt_execute_params(
    stmt: *mut inillucent_stmt,
    params: *mut inillucent_params,
    changed: *mut u64,
    error: *mut *mut inillucent_error,
) -> i32 {
    guarded("inillucent_stmt_execute_params", error, || {
        if params.is_null() {
            return misused("inillucent_stmt_execute_params", error);
        }
        let taken = Box::from_raw(params);
        let source = taken.source;
        run_rows(
            stmt,
            Ok(taken.rows),
            || read_again(source),
            changed,
            error,
            "inillucent_stmt_execute_params",
        )
    })
}

/// Reads the rows of an execution a second time, taking the interpreter lock
/// for the read.
///
/// **Why the rows are read again rather than kept** (task-2191). They move
/// into the engine, which is what saves copying every text and every row, and
/// the one statement they run as consumes them. It fails only on a row the
/// statement refuses, and it has then written nothing; the rows run one at a
/// time to report the failing row as running them one at a time always did.
/// This runs without the interpreter lock, as the binding calls it, so the
/// lock is taken here for the length of the read.
///
/// @param source - the list or tuple `inillucent_py_params` read
///
/// # Safety
///
/// `source` must be the object `inillucent_py_params` read, still alive, and
/// the calling thread must not hold the interpreter lock.
unsafe fn read_again(source: *mut PyObject) -> inillucent_driver::Result<Vec<Vec<Value>>> {
    let Some(api) = PY_READ.get() else {
        return Err(inillucent_driver::Error::said(
            inillucent_driver::Status::InvalidState,
            "the rows were read before the Python table was set",
        ));
    };
    let state = (api.gil_ensure)();
    let rows = read_sequence(api, source, |item| read_row(api, item));
    if rows.is_none() {
        (api.err_clear)();
    }
    (api.gil_release)(state);
    rows.ok_or_else(|| {
        inillucent_driver::Error::said(
            inillucent_driver::Status::InvalidState,
            "the rows changed while they ran, and could not be read again",
        )
    })
}

/// Frees parameters `inillucent_py_params` read and nothing ran.
///
/// @param params - what it returned, or null
///
/// # Safety
///
/// `params` must be null or a pointer `inillucent_py_params` returned and
/// nothing has used since.
#[no_mangle]
pub unsafe extern "C" fn inillucent_params_free(params: *mut inillucent_params) {
    guarded_value(
        || {
            if !params.is_null() {
                drop(Box::from_raw(params));
            }
        },
        (),
    )
}

/// Reads a list or a tuple item by item.
///
/// @param api - the table
/// @param sequence - the list or tuple
/// @param item - reads one item
///
/// # Safety
///
/// `sequence` must be a live Python object, with the interpreter lock held.
unsafe fn read_sequence<T>(
    api: &PyRead,
    sequence: *mut PyObject,
    mut item: impl FnMut(*mut PyObject) -> Option<T>,
) -> Option<Vec<T>> {
    let kind = type_of(sequence);
    let (size, get): (
        _,
        unsafe extern "C" fn(*mut PyObject, isize) -> *mut PyObject,
    ) = if kind == api.list_type {
        (api.list_size, api.list_get_item)
    } else if kind == api.tuple_type {
        (api.tuple_size, api.tuple_get_item)
    } else {
        return None;
    };
    let count = usize::try_from(size(sequence)).ok()?;
    let mut out = Vec::with_capacity(count);
    for at in 0..count {
        let held = get(sequence, at as isize);
        if held.is_null() {
            return None;
        }
        out.push(item(held)?);
    }
    Some(out)
}

/// Reads one row.
///
/// @param api - the table
/// @param row - the list or tuple of values
///
/// # Safety
///
/// `row` must be a live Python object, with the interpreter lock held.
unsafe fn read_row(api: &PyRead, row: *mut PyObject) -> Option<Vec<Value>> {
    read_sequence(api, row, |value| read_value(api, value))
}

/// Reads one value of a type this module knows.
///
/// A `bool` is read as the integer it is, 0 or 1, which is how the JSON path
/// binds `true` and `false`.
///
/// @param api - the table
/// @param value - the object
///
/// # Safety
///
/// `value` must be a live Python object, with the interpreter lock held.
unsafe fn read_value(api: &PyRead, value: *mut PyObject) -> Option<Value> {
    let kind = type_of(value);
    if kind == api.none_type {
        return Some(Value::Null);
    }
    if kind == api.int_type || kind == api.bool_type {
        let number = (api.long_as_long_long)(value);
        // -1 is also an answer, so the error is asked about only then.
        if number == -1 && !(api.err_occurred)().is_null() {
            return None;
        }
        return Some(Value::Integer(number));
    }
    if kind == api.float_type {
        return Some(Value::Real((api.float_as_double)(value)));
    }
    if kind == api.str_type {
        let mut size: isize = 0;
        let text = (api.unicode_as_utf8_and_size)(value, &mut size);
        if text.is_null() {
            return None;
        }
        let length = usize::try_from(size).ok()?;
        let bytes = std::slice::from_raw_parts(text as *const u8, length);
        return std::str::from_utf8(bytes)
            .ok()
            .map(|text| Value::Text(text.to_owned()));
    }
    if kind == api.bytes_type {
        let mut buffer: *mut c_char = std::ptr::null_mut();
        let mut size: isize = 0;
        if (api.bytes_as_string_and_size)(value, &mut buffer, &mut size) != 0 || buffer.is_null() {
            return None;
        }
        let length = usize::try_from(size).ok()?;
        let bytes = std::slice::from_raw_parts(buffer as *const u8, length);
        return Some(Value::Blob(bytes.to_vec()));
    }
    None
}

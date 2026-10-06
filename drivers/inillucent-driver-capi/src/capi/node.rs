//! The C library as a Node addon: open, query and close, in process.
//!
//! **Why the library is its own Node addon** (task-2191). The npm package ran
//! the command line once a call, about 18 ms each, and then a session over one
//! `inillucent-mcp` process, about 0.08 ms a call: a pipe to another process
//! and a JSON text each way. `node:sqlite` answers a lookup in 0.014 ms because
//! it runs in the process. Loaded with `process.dlopen`, this library does the
//! same, and builds every result as JavaScript values as it reads them.
//!
//! **No Node is linked.** Node exports its `napi_*` functions from its own
//! executable, so they are looked up in the host process when Node loads the
//! addon: `GetProcAddress` on Windows, `dlsym` elsewhere. A process that never
//! loads the library as an addon never asks for them.
//!
//! Invariant: **every entry here runs on the JavaScript thread that called
//! it**, which is the only thread N-API allows, and every handle it returns is
//! a handle of the C ABI, freed the way that ABI frees it.

use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::OnceLock;

use inillucent_driver::Value;

use crate::*;

/// `napi_env`, `napi_value` and `napi_callback_info`, which this module never
/// looks inside.
type Opaque = *mut c_void;

/// `napi_callback`.
type Callback = unsafe extern "C" fn(Opaque, Opaque) -> Opaque;

/// The N-API functions this module calls.
struct Napi {
    create_function: unsafe extern "C" fn(
        Opaque,
        *const c_char,
        usize,
        Callback,
        *mut c_void,
        *mut Opaque,
    ) -> i32,
    set_named_property: unsafe extern "C" fn(Opaque, Opaque, *const c_char, Opaque) -> i32,
    set_property: unsafe extern "C" fn(Opaque, Opaque, Opaque, Opaque) -> i32,
    get_cb_info: unsafe extern "C" fn(
        Opaque,
        Opaque,
        *mut usize,
        *mut Opaque,
        *mut Opaque,
        *mut *mut c_void,
    ) -> i32,
    create_object: unsafe extern "C" fn(Opaque, *mut Opaque) -> i32,
    create_array_with_length: unsafe extern "C" fn(Opaque, usize, *mut Opaque) -> i32,
    set_element: unsafe extern "C" fn(Opaque, Opaque, u32, Opaque) -> i32,
    get_element: unsafe extern "C" fn(Opaque, Opaque, u32, *mut Opaque) -> i32,
    get_array_length: unsafe extern "C" fn(Opaque, Opaque, *mut u32) -> i32,
    is_array: unsafe extern "C" fn(Opaque, Opaque, *mut bool) -> i32,
    create_string_utf8: unsafe extern "C" fn(Opaque, *const c_char, usize, *mut Opaque) -> i32,
    create_double: unsafe extern "C" fn(Opaque, f64, *mut Opaque) -> i32,
    create_bigint_int64: unsafe extern "C" fn(Opaque, i64, *mut Opaque) -> i32,
    get_null: unsafe extern "C" fn(Opaque, *mut Opaque) -> i32,
    get_undefined: unsafe extern "C" fn(Opaque, *mut Opaque) -> i32,
    create_buffer_copy:
        unsafe extern "C" fn(Opaque, usize, *const c_void, *mut *mut c_void, *mut Opaque) -> i32,
    type_of: unsafe extern "C" fn(Opaque, Opaque, *mut i32) -> i32,
    get_value_double: unsafe extern "C" fn(Opaque, Opaque, *mut f64) -> i32,
    get_value_bool: unsafe extern "C" fn(Opaque, Opaque, *mut bool) -> i32,
    get_value_string_utf8:
        unsafe extern "C" fn(Opaque, Opaque, *mut c_char, usize, *mut usize) -> i32,
    get_value_bigint_int64: unsafe extern "C" fn(Opaque, Opaque, *mut i64, *mut bool) -> i32,
    is_typedarray: unsafe extern "C" fn(Opaque, Opaque, *mut bool) -> i32,
    get_typedarray_info: unsafe extern "C" fn(
        Opaque,
        Opaque,
        *mut i32,
        *mut usize,
        *mut *mut c_void,
        *mut Opaque,
        *mut usize,
    ) -> i32,
    throw_error: unsafe extern "C" fn(Opaque, *const c_char, *const c_char) -> i32,
    add_env_cleanup_hook:
        unsafe extern "C" fn(Opaque, unsafe extern "C" fn(*mut c_void), *mut c_void) -> i32,
    open_handle_scope: unsafe extern "C" fn(Opaque, *mut Opaque) -> i32,
    close_handle_scope: unsafe extern "C" fn(Opaque, Opaque) -> i32,
}

/// The table, resolved once when Node first loads the addon.
static NAPI: OnceLock<Option<Napi>> = OnceLock::new();

/// `napi_valuetype`, the members this module reads.
const TYPE_UNDEFINED: i32 = 0;
const TYPE_NULL: i32 = 1;
const TYPE_BOOLEAN: i32 = 2;
const TYPE_NUMBER: i32 = 3;
const TYPE_STRING: i32 = 4;
const TYPE_OBJECT: i32 = 6;
const TYPE_BIGINT: i32 = 9;

/// `napi_typedarray_type`'s `napi_uint8_array`.
const UINT8_ARRAY: i32 = 1;

/// The largest integer a JavaScript number holds exactly.
const SAFE_INTEGER: i64 = (1 << 53) - 1;

#[cfg(windows)]
mod host {
    //! The host's exported functions, on Windows.
    use std::ffi::{c_char, c_void};

    #[link(name = "kernel32")]
    extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    /// Returns the address the host process exports under a name, or null.
    ///
    /// @param name - the symbol, NUL terminated
    pub(super) unsafe fn symbol(name: &std::ffi::CStr) -> *mut c_void {
        GetProcAddress(GetModuleHandleW(std::ptr::null()), name.as_ptr())
    }
}

#[cfg(unix)]
mod host {
    //! The host's exported functions, elsewhere.
    use std::ffi::{c_char, c_void};

    // `dlsym` is in libc on glibc 2.34 and later and on macOS, and in libdl on
    // the glibc 2.28 the Linux release is built against.
    #[cfg_attr(target_os = "linux", link(name = "dl"))]
    extern "C" {
        fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    }

    /// `RTLD_DEFAULT`: every image loaded in the process.
    #[cfg(target_os = "macos")]
    const DEFAULT: *mut c_void = -2isize as *mut c_void;
    #[cfg(not(target_os = "macos"))]
    const DEFAULT: *mut c_void = std::ptr::null_mut();

    /// Returns the address the host process exports under a name, or null.
    ///
    /// @param name - the symbol, NUL terminated
    pub(super) unsafe fn symbol(name: &std::ffi::CStr) -> *mut c_void {
        dlsym(DEFAULT, name.as_ptr())
    }
}

/// Looks every N-API function up in the host, or answers `None` when one is
/// missing, which is a host that is not Node.
unsafe fn resolve() -> Option<Napi> {
    macro_rules! find {
        ($name:literal) => {{
            let address = host::symbol($name);
            if address.is_null() {
                return None;
            }
            super::python::as_function(address.cast_const())
        }};
    }
    Some(Napi {
        create_function: find!(c"napi_create_function"),
        set_named_property: find!(c"napi_set_named_property"),
        set_property: find!(c"napi_set_property"),
        get_cb_info: find!(c"napi_get_cb_info"),
        create_object: find!(c"napi_create_object"),
        create_array_with_length: find!(c"napi_create_array_with_length"),
        set_element: find!(c"napi_set_element"),
        get_element: find!(c"napi_get_element"),
        get_array_length: find!(c"napi_get_array_length"),
        is_array: find!(c"napi_is_array"),
        create_string_utf8: find!(c"napi_create_string_utf8"),
        create_double: find!(c"napi_create_double"),
        create_bigint_int64: find!(c"napi_create_bigint_int64"),
        get_null: find!(c"napi_get_null"),
        get_undefined: find!(c"napi_get_undefined"),
        create_buffer_copy: find!(c"napi_create_buffer_copy"),
        type_of: find!(c"napi_typeof"),
        get_value_double: find!(c"napi_get_value_double"),
        get_value_bool: find!(c"napi_get_value_bool"),
        get_value_string_utf8: find!(c"napi_get_value_string_utf8"),
        get_value_bigint_int64: find!(c"napi_get_value_bigint_int64"),
        is_typedarray: find!(c"napi_is_typedarray"),
        get_typedarray_info: find!(c"napi_get_typedarray_info"),
        throw_error: find!(c"napi_throw_error"),
        add_env_cleanup_hook: find!(c"napi_add_env_cleanup_hook"),
        open_handle_scope: find!(c"napi_open_handle_scope"),
        close_handle_scope: find!(c"napi_close_handle_scope"),
    })
}

/// Registers the addon's functions on the object Node hands in.
///
/// Node calls this when `process.dlopen` loads the library. It answers the
/// object with `open`, `connect`, `query`, `disconnect` and `close` on it, or
/// the object unchanged when the host is not Node.
///
/// @param env - the environment
/// @param exports - the object to fill
///
/// # Safety
///
/// Only Node calls this, on its JavaScript thread.
#[no_mangle]
pub unsafe extern "C" fn napi_register_module_v1(env: Opaque, exports: Opaque) -> Opaque {
    // SAFETY: as for this function; the body only calls the host's own
    // functions with the environment and object Node passed in.
    guarded_value(|| unsafe { register(env, exports) }, exports)
}

/// The body of [`napi_register_module_v1`], inside its panic guard.
///
/// @param env - the environment
/// @param exports - the object to fill
unsafe fn register(env: Opaque, exports: Opaque) -> Opaque {
    let Some(api) = NAPI.get_or_init(|| resolve()).as_ref() else {
        return exports;
    };
    let entries: [(&CStr, Callback); 6] = [
        (c"open", js_open),
        (c"connect", js_connect),
        (c"query", js_query),
        (c"batch", js_batch),
        (c"disconnect", js_disconnect),
        (c"close", js_close),
    ];
    for (name, body) in entries {
        let mut function = std::ptr::null_mut();
        let made = (api.create_function)(
            env,
            name.as_ptr(),
            name.to_bytes().len(),
            body,
            std::ptr::null_mut(),
            &mut function,
        );
        if made == 0 {
            (api.set_named_property)(env, exports, name.as_ptr(), function);
        }
    }
    exports
}

/// Makes the function Node calls for one entry, so a panic never unwinds into
/// Node: a caught panic answers `undefined`, as `guarded_value` does for every
/// other entry point of this library.
macro_rules! guarded_entry {
    ($name:ident, $body:ident) => {
        /// The entry Node calls. See the body it guards.
        unsafe extern "C" fn $name(env: Opaque, info: Opaque) -> Opaque {
            // SAFETY: Node passes the environment and the call information it
            // owns for the length of this call, which is what the body needs.
            super::error::guarded_value(|| unsafe { $body(env, info) }, std::ptr::null_mut())
        }
    };
}

guarded_entry!(js_open, open_body);
guarded_entry!(js_connect, connect_body);
guarded_entry!(js_query, query_body);
guarded_entry!(js_batch, batch_body);
guarded_entry!(js_disconnect, disconnect_body);
guarded_entry!(js_close, close_body);

/// The table, which `napi_register_module_v1` has resolved before any of the
/// functions it registered can be called.
fn napi() -> Option<&'static Napi> {
    NAPI.get().and_then(Option::as_ref)
}

/// The handles this thread opened, so each is used only on its own thread and
/// closed when the thread's environment goes away.
///
/// **Per thread, because the engine is** (task-2191; the guidance in
/// `tasks/task-2191-fable-guidance.md` section 9.2). Node runs each
/// `worker_threads` worker on a thread of its own and loads the addon again
/// there, and a handle is only a number: a worker given one the main thread
/// opened would call into an engine built of `Rc` and `RefCell` from another
/// thread. And nothing closed a database a script left open when Node exited,
/// so the next open had a log to replay.
#[derive(Default)]
struct Opened {
    /// The databases this thread opened and has not closed.
    databases: Vec<usize>,
    /// The sessions this thread opened and has not freed.
    sessions: Vec<usize>,
    /// Whether this thread's environment has the cleanup hook.
    hooked: bool,
}

thread_local! {
    /// What this thread opened; see [`Opened`].
    static OPENED: std::cell::RefCell<Opened> = std::cell::RefCell::new(Opened::default());
}

/// Reports whether this thread opened a handle and still holds it.
///
/// @param address - the handle
/// @param database - whether it is a database rather than a session
fn owned(address: usize, database: bool) -> bool {
    OPENED
        .try_with(|opened| {
            let opened = opened.borrow();
            match database {
                true => opened.databases.contains(&address),
                false => opened.sessions.contains(&address),
            }
        })
        .unwrap_or(false)
}

/// Records or forgets a handle this thread holds.
///
/// @param address - the handle
/// @param database - whether it is a database rather than a session
/// @param held - whether it is now held
fn note(address: usize, database: bool, held: bool) {
    let _ = OPENED.try_with(|opened| {
        let mut opened = opened.borrow_mut();
        let list = match database {
            true => &mut opened.databases,
            false => &mut opened.sessions,
        };
        list.retain(|kept| *kept != address);
        if held {
            list.push(address);
        }
    });
}

/// Closes what this thread's environment left open, when Node tears it down.
///
/// Sessions first, because a database refuses to close while one is open.
///
/// @param _argument - unused
unsafe extern "C" fn close_what_was_left(_argument: *mut c_void) {
    guarded_value(
        || {
            let (sessions, databases) = OPENED
                .try_with(|opened| {
                    let mut opened = opened.borrow_mut();
                    (
                        std::mem::take(&mut opened.sessions),
                        std::mem::take(&mut opened.databases),
                    )
                })
                .unwrap_or_default();
            for session in sessions {
                // SAFETY: a session this thread opened and did not free.
                unsafe { inillucent_conn_free(session as *mut inillucent_conn) };
            }
            for database in databases {
                let mut error: *mut inillucent_error = std::ptr::null_mut();
                // SAFETY: a database this thread opened and did not close.
                unsafe {
                    if inillucent_close(database as *mut inillucent_db, &mut error) != INILLUCENT_OK
                        && !error.is_null()
                    {
                        inillucent_error_free(error);
                    }
                }
            }
        },
        (),
    );
}

/// Asks Node to call [`close_what_was_left`] when this thread's environment
/// ends, once per thread.
///
/// @param api - the table
/// @param env - the environment
unsafe fn hook_the_cleanup(api: &Napi, env: Opaque) {
    let first = OPENED
        .try_with(|opened| !std::mem::replace(&mut opened.borrow_mut().hooked, true))
        .unwrap_or(false);
    if first {
        (api.add_env_cleanup_hook)(env, close_what_was_left, std::ptr::null_mut());
    }
}

/// Reads a handle argument and answers it only when this thread holds it.
///
/// @param api - the table
/// @param env - the environment
/// @param value - the argument
/// @param database - whether it should be a database rather than a session
unsafe fn handle_of(api: &Napi, env: Opaque, value: Opaque, database: bool) -> Option<usize> {
    let address = number_of(api, env, value).unwrap_or(0.0) as usize;
    owned(address, database).then_some(address)
}

/// What a call gets for a handle this thread does not hold.
///
/// @param api - the table
/// @param env - the environment
unsafe fn not_this_threads(api: &Napi, env: Opaque) -> Opaque {
    throw(
        api,
        env,
        "invalid_state",
        "that handle is not open on this thread: a database and its sessions are used only on \
         the thread that opened them",
    )
}

/// Reads up to `N` arguments of a call.
///
/// @param api - the table
/// @param env - the environment
/// @param info - the call
unsafe fn arguments<const N: usize>(api: &Napi, env: Opaque, info: Opaque) -> [Opaque; N] {
    let mut count = N;
    let mut values = [std::ptr::null_mut(); N];
    (api.get_cb_info)(
        env,
        info,
        &mut count,
        values.as_mut_ptr(),
        std::ptr::null_mut(),
        std::ptr::null_mut(),
    );
    values
}

/// Throws a JavaScript `Error` with a code, and answers null for the callback
/// to return.
///
/// @param api - the table
/// @param env - the environment
/// @param code - the status name, which the error carries as `code`
/// @param message - what went wrong
unsafe fn throw(api: &Napi, env: Opaque, code: &str, message: &str) -> Opaque {
    let code = CString::new(code).unwrap_or_default();
    let message = CString::new(message.replace('\0', "?")).unwrap_or_default();
    (api.throw_error)(env, code.as_ptr(), message.as_ptr());
    std::ptr::null_mut()
}

/// Throws the failure a C ABI call reported, and frees it.
///
/// @param api - the table
/// @param env - the environment
/// @param status - the status the call returned
/// @param error - the error it produced, or null
unsafe fn throw_failure(
    api: &Napi,
    env: Opaque,
    status: i32,
    error: *mut inillucent_error,
) -> Opaque {
    let message = match error.is_null() {
        true => format!("the call failed with status {status}"),
        false => CStr::from_ptr(inillucent_error_message(error))
            .to_string_lossy()
            .into_owned(),
    };
    if !error.is_null() {
        inillucent_error_free(error);
    }
    let code = status_name(status);
    throw(api, env, code, &message)
}

/// Returns a status's name, as the command line's JSON spells it.
///
/// @param status - the C ABI status
fn status_name(status: i32) -> &'static str {
    match status {
        1 => "unsupported",
        2 => "syntax",
        3 => "not_found",
        4 => "constraint",
        5 => "readonly",
        6 => "busy",
        7 => "interrupted",
        8 => "corrupt",
        9 => "io",
        10 => "full",
        11 => "too_big",
        12 => "invalid_state",
        _ => "internal",
    }
}

/// Reads a JavaScript string argument.
///
/// @param api - the table
/// @param env - the environment
/// @param value - the argument
unsafe fn string_of(api: &Napi, env: Opaque, value: Opaque) -> Option<String> {
    let mut length = 0usize;
    if (api.get_value_string_utf8)(env, value, std::ptr::null_mut(), 0, &mut length) != 0 {
        return None;
    }
    let mut bytes = vec![0u8; length.saturating_add(1)];
    let mut written = 0usize;
    if (api.get_value_string_utf8)(
        env,
        value,
        bytes.as_mut_ptr().cast(),
        bytes.len(),
        &mut written,
    ) != 0
    {
        return None;
    }
    bytes.truncate(written);
    String::from_utf8(bytes).ok()
}

/// Reads a number argument.
///
/// @param api - the table
/// @param env - the environment
/// @param value - the argument
unsafe fn number_of(api: &Napi, env: Opaque, value: Opaque) -> Option<f64> {
    let mut number = 0f64;
    ((api.get_value_double)(env, value, &mut number) == 0).then_some(number)
}

/// Makes a JavaScript number from a handle's address.
///
/// An address in a 64-bit process fits in the 53 bits a number holds exactly.
///
/// @param api - the table
/// @param env - the environment
/// @param address - the handle
unsafe fn handle_value(api: &Napi, env: Opaque, address: usize) -> Opaque {
    let mut made = std::ptr::null_mut();
    (api.create_double)(env, address as f64, &mut made);
    made
}

/// `open(path, flags)`: opens a database and answers its handle.
unsafe fn open_body(env: Opaque, info: Opaque) -> Opaque {
    let Some(api) = napi() else {
        return std::ptr::null_mut();
    };
    let [path, flags] = arguments::<2>(api, env, info);
    let Some(path) = string_of(api, env, path) else {
        return throw(
            api,
            env,
            "invalid_state",
            "open takes the database's path as a string",
        );
    };
    let flags = number_of(api, env, flags).unwrap_or(f64::from(INILLUCENT_OPEN_CREATE)) as u32;
    let Ok(path) = CString::new(path) else {
        return throw(
            api,
            env,
            "invalid_state",
            "a database path cannot hold a NUL",
        );
    };
    let mut db: *mut inillucent_db = std::ptr::null_mut();
    let mut error: *mut inillucent_error = std::ptr::null_mut();
    let status = inillucent_open(path.as_ptr(), flags, &mut db, &mut error);
    if status != INILLUCENT_OK {
        return throw_failure(api, env, status, error);
    }
    hook_the_cleanup(api, env);
    note(db as usize, true, true);
    handle_value(api, env, db as usize)
}

/// `connect(db)`: opens a session on a database and answers its handle.
unsafe fn connect_body(env: Opaque, info: Opaque) -> Opaque {
    let Some(api) = napi() else {
        return std::ptr::null_mut();
    };
    let [db] = arguments::<1>(api, env, info);
    let Some(db) = handle_of(api, env, db, true) else {
        return not_this_threads(api, env);
    };
    let db = db as *mut inillucent_db;
    let mut conn: *mut inillucent_conn = std::ptr::null_mut();
    let mut error: *mut inillucent_error = std::ptr::null_mut();
    let status = inillucent_connect(db, &mut conn, &mut error);
    if status != INILLUCENT_OK {
        return throw_failure(api, env, status, error);
    }
    note(conn as usize, false, true);
    handle_value(api, env, conn as usize)
}

/// Runs a script on a session, the way `inillucent_execute_batch` does, and
/// answers the failure's status and message when it fails.
///
/// @param conn - the session
/// @param sql - the statements
unsafe fn run_script(
    conn: *mut inillucent_conn,
    sql: &CStr,
) -> Result<(), (i32, *mut inillucent_error)> {
    let mut error: *mut inillucent_error = std::ptr::null_mut();
    match inillucent_execute_batch(conn, sql.as_ptr(), &mut error) {
        INILLUCENT_OK => Ok(()),
        status => Err((status, error)),
    }
}

/// `batch(conn, sql)`: runs several statements as one transaction.
///
/// What the command line's `batch` does: a script outside a transaction runs
/// inside one it opens and commits, and is rolled back when a statement fails;
/// a script inside a transaction the caller opened joins it. Answers
/// `{ changes, transaction }`, where `transaction` is `committed` or `joined`.
unsafe fn batch_body(env: Opaque, info: Opaque) -> Opaque {
    let Some(api) = napi() else {
        return std::ptr::null_mut();
    };
    let [conn, sql] = arguments::<2>(api, env, info);
    let Some(conn) = handle_of(api, env, conn, false) else {
        return not_this_threads(api, env);
    };
    let conn = conn as *mut inillucent_conn;
    let Some(sql) = string_of(api, env, sql).and_then(|text| CString::new(text).ok()) else {
        return throw(
            api,
            env,
            "invalid_state",
            "batch takes the statements as a string",
        );
    };
    let joined = inillucent_in_transaction(conn) == 1;
    let before = inillucent_total_changes(conn);
    if !joined {
        if let Err((status, error)) = run_script(conn, c"BEGIN") {
            return throw_failure(api, env, status, error);
        }
    }
    if let Err((status, error)) = run_script(conn, &sql) {
        if !joined {
            if let Err((_, second)) = run_script(conn, c"ROLLBACK") {
                if !second.is_null() {
                    inillucent_error_free(second);
                }
            }
        }
        return throw_failure(api, env, status, error);
    }
    if !joined {
        if let Err((status, error)) = run_script(conn, c"COMMIT") {
            return throw_failure(api, env, status, error);
        }
    }
    let changes = inillucent_total_changes(conn).saturating_sub(before);
    let mut result = std::ptr::null_mut();
    (api.create_object)(env, &mut result);
    (api.set_named_property)(
        env,
        result,
        c"changes".as_ptr(),
        js_value(api, env, &Value::Integer(changes)),
    );
    let word = match joined {
        true => "joined",
        false => "committed",
    };
    let mut said = std::ptr::null_mut();
    (api.create_string_utf8)(env, word.as_ptr().cast(), word.len(), &mut said);
    (api.set_named_property)(env, result, c"transaction".as_ptr(), said);
    result
}

/// `disconnect(conn)`: frees a session.
unsafe fn disconnect_body(env: Opaque, info: Opaque) -> Opaque {
    let Some(api) = napi() else {
        return std::ptr::null_mut();
    };
    let [conn] = arguments::<1>(api, env, info);
    let Some(conn) = handle_of(api, env, conn, false) else {
        return not_this_threads(api, env);
    };
    note(conn, false, false);
    inillucent_conn_free(conn as *mut inillucent_conn);
    let mut nothing = std::ptr::null_mut();
    (api.get_undefined)(env, &mut nothing);
    nothing
}

/// `close(db)`: closes a database, folding what it wrote into the file.
unsafe fn close_body(env: Opaque, info: Opaque) -> Opaque {
    let Some(api) = napi() else {
        return std::ptr::null_mut();
    };
    let [db] = arguments::<1>(api, env, info);
    let Some(address) = handle_of(api, env, db, true) else {
        return not_this_threads(api, env);
    };
    let mut error: *mut inillucent_error = std::ptr::null_mut();
    let status = inillucent_close(address as *mut inillucent_db, &mut error);
    if status != INILLUCENT_OK {
        return throw_failure(api, env, status, error);
    }
    note(address, true, false);
    let mut nothing = std::ptr::null_mut();
    (api.get_undefined)(env, &mut nothing);
    nothing
}

/// Reads one JavaScript parameter as the value it binds.
///
/// `null` and `undefined` bind NULL, a boolean binds 1 or 0, a number that is
/// an integer binds an integer and any other number a real, a bigint binds an
/// integer when it fits in 64 bits, a string binds text, and a `Uint8Array`
/// (a `Buffer` is one) binds a blob. Anything else is refused.
///
/// @param api - the table
/// @param env - the environment
/// @param value - the parameter
unsafe fn parameter_of(api: &Napi, env: Opaque, value: Opaque) -> Result<Value, String> {
    let mut kind = TYPE_UNDEFINED;
    (api.type_of)(env, value, &mut kind);
    match kind {
        TYPE_UNDEFINED | TYPE_NULL => Ok(Value::Null),
        TYPE_BOOLEAN => {
            let mut flag = false;
            (api.get_value_bool)(env, value, &mut flag);
            Ok(Value::Integer(i64::from(flag)))
        }
        TYPE_NUMBER => {
            let number = number_of(api, env, value).unwrap_or(f64::NAN);
            let integral = number.fract() == 0.0 && number.abs() <= SAFE_INTEGER as f64;
            if integral && !(number == 0.0 && number.is_sign_negative()) {
                Ok(Value::Integer(number as i64))
            } else if number.is_finite() {
                Ok(Value::Real(number))
            } else {
                Err(format!(
                    "a parameter cannot be {number}: SQL has no spelling for it"
                ))
            }
        }
        TYPE_BIGINT => {
            let mut number = 0i64;
            let mut lossless = false;
            (api.get_value_bigint_int64)(env, value, &mut number, &mut lossless);
            match lossless {
                true => Ok(Value::Integer(number)),
                false => Err("a bigint parameter does not fit in 64 bits".to_string()),
            }
        }
        TYPE_STRING => string_of(api, env, value)
            .map(Value::Text)
            .ok_or_else(|| "a string parameter could not be read".to_string()),
        TYPE_OBJECT => {
            let mut typed = false;
            (api.is_typedarray)(env, value, &mut typed);
            if !typed {
                return Err("an object parameter must be a Uint8Array".to_string());
            }
            let mut array_type = 0i32;
            let mut length = 0usize;
            let mut data: *mut c_void = std::ptr::null_mut();
            let mut buffer = std::ptr::null_mut();
            let mut offset = 0usize;
            (api.get_typedarray_info)(
                env,
                value,
                &mut array_type,
                &mut length,
                &mut data,
                &mut buffer,
                &mut offset,
            );
            if array_type != UINT8_ARRAY {
                return Err("a typed array parameter must be a Uint8Array".to_string());
            }
            let bytes = match data.is_null() || length == 0 {
                true => Vec::new(),
                false => std::slice::from_raw_parts(data.cast::<u8>(), length).to_vec(),
            };
            Ok(Value::Blob(bytes))
        }
        _ => Err(
            "a parameter must be null, a boolean, a number, a bigint, a string or a Uint8Array"
                .to_string(),
        ),
    }
}

/// Makes one JavaScript value from a cell.
///
/// An integer is a number when a number holds it exactly and a bigint when it
/// does not, so no integer is rounded on the way out.
///
/// @param api - the table
/// @param env - the environment
/// @param value - the cell
unsafe fn js_value(api: &Napi, env: Opaque, value: &Value) -> Opaque {
    let mut made = std::ptr::null_mut();
    let status = match value {
        Value::Null => (api.get_null)(env, &mut made),
        Value::Integer(number) if number.abs() <= SAFE_INTEGER => {
            (api.create_double)(env, *number as f64, &mut made)
        }
        Value::Integer(number) => (api.create_bigint_int64)(env, *number, &mut made),
        Value::Real(number) => (api.create_double)(env, *number, &mut made),
        Value::Text(text) => {
            (api.create_string_utf8)(env, text.as_ptr().cast(), text.len(), &mut made)
        }
        Value::Blob(bytes) => {
            let mut copy = std::ptr::null_mut();
            (api.create_buffer_copy)(
                env,
                bytes.len(),
                bytes.as_ptr().cast(),
                &mut copy,
                &mut made,
            )
        }
    };
    // A failed call leaves a pending exception and a null handle; null is
    // passed up so the caller stops at the first one.
    match status {
        0 => made,
        _ => std::ptr::null_mut(),
    }
}

/// `query(conn, sql, params, arrays)`: runs one statement.
///
/// Answers `{ columns, rows, changes, lastInsertRowid, total }`. `rows` holds
/// one object a row keyed by column name, or one array a row when `arrays` is
/// true. `params` is an array of values, or `undefined` for none.
unsafe fn query_body(env: Opaque, info: Opaque) -> Opaque {
    let Some(api) = napi() else {
        return std::ptr::null_mut();
    };
    let [conn, sql, params, arrays] = arguments::<4>(api, env, info);
    let Some(conn) = handle_of(api, env, conn, false) else {
        return not_this_threads(api, env);
    };
    let conn = conn as *mut inillucent_conn;
    let Some(sql) = string_of(api, env, sql) else {
        return throw(
            api,
            env,
            "invalid_state",
            "query takes the statement as a string",
        );
    };
    let mut as_arrays = false;
    (api.get_value_bool)(env, arrays, &mut as_arrays);
    let values = match parameters_of(api, env, params) {
        Ok(values) => values,
        Err(message) => return throw(api, env, "invalid_state", &message),
    };
    let Some(database) = database_of(conn) else {
        return throw(
            api,
            env,
            "invalid_state",
            "query was given a session that is not open",
        );
    };
    // **The driver, not a statement handle.** A handle is an allocation, a
    // registration in the live handle table and a second lookup of the plan for
    // its parameter count, on every call; the engine already keeps the plan by
    // its text, so the driver's `query` is the same work without them.
    let connection = database.database.session_as(session_of(conn));
    match connection.query(&sql, &values, usize::MAX) {
        Ok(rows) => result_of(api, env, &rows, conn, as_arrays),
        Err(why) => throw(api, env, why.status.name(), &why.message),
    }
}

/// Reads the `params` argument: an array of values, or nothing.
///
/// @param api - the table
/// @param env - the environment
/// @param params - the argument
unsafe fn parameters_of(api: &Napi, env: Opaque, params: Opaque) -> Result<Vec<Value>, String> {
    let mut kind = TYPE_UNDEFINED;
    (api.type_of)(env, params, &mut kind);
    if kind == TYPE_UNDEFINED || kind == TYPE_NULL {
        return Ok(Vec::new());
    }
    let mut listed = false;
    (api.is_array)(env, params, &mut listed);
    if !listed {
        return Err("params must be an array".to_string());
    }
    let mut length = 0u32;
    (api.get_array_length)(env, params, &mut length);
    let mut values = Vec::with_capacity(length as usize);
    for at in 0..length {
        let mut item = std::ptr::null_mut();
        (api.get_element)(env, params, at, &mut item);
        values.push(parameter_of(api, env, item)?);
    }
    Ok(values)
}

/// How many rows one handle scope holds while a result is built.
const ROWS_PER_SCOPE: usize = 1_000;

/// An open handle scope, closed when it is dropped.
struct Scope<'a> {
    /// The table.
    api: &'a Napi,
    /// The environment.
    env: Opaque,
    /// The scope.
    scope: Opaque,
}

impl<'a> Scope<'a> {
    /// Opens a handle scope, or answers `None` when Node refuses one.
    ///
    /// @param api - the table
    /// @param env - the environment
    unsafe fn open(api: &'a Napi, env: Opaque) -> Option<Scope<'a>> {
        let mut scope = std::ptr::null_mut();
        match (api.open_handle_scope)(env, &mut scope) {
            0 => Some(Scope { api, env, scope }),
            _ => None,
        }
    }
}

impl Drop for Scope<'_> {
    /// Closes the scope.
    fn drop(&mut self) {
        // SAFETY: the scope was opened on this environment, on this thread,
        // and nothing opened inside it is still open.
        unsafe { (self.api.close_handle_scope)(self.env, self.scope) };
    }
}

/// Builds the object `query` answers.
///
/// @param api - the table
/// @param env - the environment
/// @param rows - the driver's result
/// @param conn - the session, for `lastInsertRowid`
/// @param as_arrays - whether a row is an array rather than an object
unsafe fn result_of(
    api: &Napi,
    env: Opaque,
    rows: &inillucent_driver::Rows,
    conn: *mut inillucent_conn,
    as_arrays: bool,
) -> Opaque {
    // **Every status is checked** (task-2191; the guidance's section 9.2).
    // After a failure every later call answers a pending exception and a null
    // handle, so the first failure ends the result and Node throws it.
    macro_rules! ok {
        ($call:expr) => {
            if $call != 0 {
                return std::ptr::null_mut();
            }
        };
    }
    let mut keys = Vec::with_capacity(rows.columns.len());
    let mut columns = std::ptr::null_mut();
    ok!((api.create_array_with_length)(
        env,
        rows.columns.len(),
        &mut columns
    ));
    for (at, column) in rows.columns.iter().enumerate() {
        let mut name = std::ptr::null_mut();
        ok!((api.create_string_utf8)(
            env,
            column.name.as_ptr().cast(),
            column.name.len(),
            &mut name,
        ));
        ok!((api.set_element)(env, columns, at as u32, name));
        keys.push(name);
    }
    let mut body = std::ptr::null_mut();
    ok!((api.create_array_with_length)(
        env,
        rows.rows.len(),
        &mut body
    ));
    for (block, chunk) in rows.rows.chunks(ROWS_PER_SCOPE).enumerate() {
        // **A handle scope per block of rows**, so a large result does not hold
        // a handle per cell until the call returns. The rows go into `body`,
        // which belongs to the scope outside this one, so nothing escapes.
        let Some(_scope) = Scope::open(api, env) else {
            return std::ptr::null_mut();
        };
        for (offset, row) in chunk.iter().enumerate() {
            let at = block.saturating_mul(ROWS_PER_SCOPE).saturating_add(offset);
            let mut item = std::ptr::null_mut();
            match as_arrays {
                true => {
                    ok!((api.create_array_with_length)(env, row.len(), &mut item));
                    for (column, value) in row.iter().enumerate() {
                        let made = js_value(api, env, value);
                        if made.is_null() {
                            return std::ptr::null_mut();
                        }
                        ok!((api.set_element)(env, item, column as u32, made));
                    }
                }
                false => {
                    ok!((api.create_object)(env, &mut item));
                    for (key, value) in keys.iter().zip(row) {
                        let made = js_value(api, env, value);
                        if made.is_null() {
                            return std::ptr::null_mut();
                        }
                        ok!((api.set_property)(env, item, *key, made));
                    }
                }
            }
            ok!((api.set_element)(env, body, at as u32, item));
        }
    }
    let mut result = std::ptr::null_mut();
    ok!((api.create_object)(env, &mut result));
    ok!((api.set_named_property)(
        env,
        result,
        c"columns".as_ptr(),
        columns
    ));
    ok!((api.set_named_property)(
        env,
        result,
        c"rows".as_ptr(),
        body
    ));
    let changes = rows
        .affected
        .map_or(0, |count| i64::try_from(count).unwrap_or(i64::MAX));
    for (name, number) in [
        (c"changes", changes),
        (c"lastInsertRowid", inillucent_last_insert_rowid(conn)),
        (c"total", i64::try_from(rows.total).unwrap_or(i64::MAX)),
    ] {
        (api.set_named_property)(
            env,
            result,
            name.as_ptr(),
            js_value(api, env, &Value::Integer(number)),
        );
    }
    result
}

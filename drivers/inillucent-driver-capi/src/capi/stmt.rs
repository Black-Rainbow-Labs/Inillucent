//! The prepared statement handle, and the bindings it carries.
//!
//! Invariant: **a binding index is one-based, as SQL writes it.** `?1` is
//! index 1, and index 0 is refused rather than silently treated as the
//! first parameter.

use std::ffi::c_char;
use std::rc::Rc;

use inillucent_driver::Value;

use crate::capi::db::*;
use crate::capi::error::*;
use crate::capi::value::*;
use crate::*;

/// A statement and the values bound to it.
pub struct inillucent_stmt {
    /// The word this handle carries while it is alive - see [`Live`].
    live: Live,
    /// The connection it was prepared on, shared rather than pointed at.
    connection: Rc<ConnState>,
    /// The statement text.
    sql: String,
    /// How many parameters the statement declares.
    ///
    /// The bound a bind index is checked against - see
    /// [`inillucent_driver::Connection::parameter_count`] for what an unbounded
    /// one cost.
    pub(crate) declared: u32,
    /// The values bound so far, by one-based index.
    pub(crate) params: Vec<Value>,
}
/// Prepares a statement, checking it compiles before the handle exists.
///
/// **It compiles now rather than at the first execute**, so that a bad
/// statement is a failure of `prepare` where a caller is looking for one. The
/// compiled form is the engine's, cached by the statement's text, so preparing
/// again inside execute is a hash lookup rather than a second compile.
///
/// @param conn - the connection
/// @param sql - the statement
/// @param out - where the handle goes
/// @param error - where a failure goes, or null
///
/// # Safety
///
/// `conn` must be a live handle, `sql` a NUL-terminated string, `out` writable.
#[no_mangle]
pub unsafe extern "C" fn inillucent_prepare(
    conn: *mut inillucent_conn,
    sql: *const c_char,
    out: *mut *mut inillucent_stmt,
    error: *mut *mut inillucent_error,
) -> i32 {
    guarded("inillucent_prepare", error, || {
        let (Some(database), Some(sql)) = (database_of(conn), borrowed(sql)) else {
            return misused("inillucent_prepare", error);
        };
        if out.is_null() {
            return misused("inillucent_prepare", error);
        }
        let connection = database.database.session_as(session_of(conn));
        if let Err(why) = connection.prepare(sql) {
            let status = why.status as i32;
            report(error, &why, false);
            return status;
        }
        // The count the bind index is checked against, read from the statement
        // that has just compiled - see `inillucent_stmt::declared`.
        let declared = match connection.parameter_count(sql) {
            Ok(declared) => declared,
            Err(why) => {
                let status = why.status as i32;
                report(error, &why, false);
                return status;
            }
        };
        let Some(handle) = held(conn as *const inillucent_conn) else {
            return misused("inillucent_prepare", error);
        };
        *out = publish(Box::new(inillucent_stmt {
            live: Live::new(inillucent_stmt::MAGIC),
            connection: Rc::clone(&handle.state),
            sql: sql.to_owned(),
            declared,
            params: Vec::new(),
        }));
        INILLUCENT_OK
    })
}
/// Frees a statement.
///
/// @param stmt - the statement
///
/// # Safety
///
/// `stmt` must be null or a live handle, freed exactly once.
#[no_mangle]
pub unsafe extern "C" fn inillucent_stmt_free(stmt: *mut inillucent_stmt) {
    guarded_value(
        || {
            if !stmt.is_null() {
                if held(stmt as *const inillucent_stmt).is_none() {
                    return;
                }
                drop(reclaim(stmt));
            }
        },
        (),
    )
}
/// Binds NULL.
///
/// @param stmt - the statement
/// @param index - the one-based parameter number
///
/// # Safety
///
/// `stmt` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_bind_null(stmt: *mut inillucent_stmt, index: u32) -> i32 {
    guarded_value(|| bind(stmt, index, Value::Null), 0)
}
/// Binds an integer.
///
/// @param stmt - the statement
/// @param index - the one-based parameter number
/// @param value - the value
///
/// # Safety
///
/// `stmt` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_bind_int(
    stmt: *mut inillucent_stmt,
    index: u32,
    value: i64,
) -> i32 {
    guarded_value(|| bind(stmt, index, Value::Integer(value)), 0)
}
/// Binds a float.
///
/// @param stmt - the statement
/// @param index - the one-based parameter number
/// @param value - the value
///
/// # Safety
///
/// `stmt` must be a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_bind_real(
    stmt: *mut inillucent_stmt,
    index: u32,
    value: f64,
) -> i32 {
    guarded_value(|| bind(stmt, index, Value::Real(value)), 0)
}
/// Binds text, copying it.
///
/// @param stmt - the statement
/// @param index - the one-based parameter number
/// @param value - the bytes
/// @param len - how many bytes
///
/// # Safety
///
/// `stmt` must be a live handle and `value` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn inillucent_bind_text(
    stmt: *mut inillucent_stmt,
    index: u32,
    value: *const c_char,
    len: usize,
) -> i32 {
    guarded_value(
        || {
            if value.is_null() {
                return bind(stmt, index, Value::Null);
            }
            let bytes = std::slice::from_raw_parts(value as *const u8, len);
            // Text that is not UTF-8 is bound as a blob rather than lossily converted,
            // for the reason `Value::from_engine` gives: a replacement character is a
            // value nobody passed.
            match std::str::from_utf8(bytes) {
                Ok(text) => bind(stmt, index, Value::Text(text.to_owned())),
                Err(_) => bind(stmt, index, Value::Blob(bytes.to_vec())),
            }
        },
        0,
    )
}
/// Binds bytes, copying them.
///
/// @param stmt - the statement
/// @param index - the one-based parameter number
/// @param value - the bytes
/// @param len - how many bytes
///
/// # Safety
///
/// `stmt` must be a live handle and `value` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn inillucent_bind_blob(
    stmt: *mut inillucent_stmt,
    index: u32,
    value: *const u8,
    len: usize,
) -> i32 {
    guarded_value(
        || {
            if value.is_null() {
                return bind(stmt, index, Value::Null);
            }
            let bytes = std::slice::from_raw_parts(value, len);
            bind(stmt, index, Value::Blob(bytes.to_vec()))
        },
        0,
    )
}
/// Unbinds every parameter.
///
/// @param stmt - the statement
///
/// # Safety
///
/// `stmt` must be null or a live handle.
#[no_mangle]
pub unsafe extern "C" fn inillucent_clear_bindings(stmt: *mut inillucent_stmt) {
    guarded_value(
        || {
            // **The handle is checked before it is dereferenced** (task-2066
            // §4.1.13). This took `as_mut()` on whatever the caller passed, so
            // clearing the bindings of a freed statement wrote into freed
            // memory - the same defect task-1979 fixed in `bind`, which is the
            // function immediately beside it and which has carried the guard
            // ever since.
            //
            // Nothing is reported, because this returns nothing: a caller that
            // passes a dead handle gets the same "no bindings were cleared"
            // it would get for a null one, which is what the signature allows
            // this to say.
            if held(stmt as *const inillucent_stmt).is_none() {
                return;
            }
            if let Some(statement) = stmt.as_mut() {
                statement.params.clear();
            }
        },
        (),
    )
}
/// Runs a statement with what is bound.
///
/// @param stmt - the statement
/// @param limit - how many rows to hand back
/// @param out - where the result goes
/// @param error - where a failure goes, or null
///
/// # Safety
///
/// `stmt` must be a live handle and `out` writable.
#[no_mangle]
pub unsafe extern "C" fn inillucent_stmt_execute(
    stmt: *mut inillucent_stmt,
    limit: u64,
    out: *mut *mut inillucent_rows,
    error: *mut *mut inillucent_error,
) -> i32 {
    guarded("inillucent_stmt_execute", error, || {
        let Some(statement) = held(stmt as *const inillucent_stmt) else {
            return misused("inillucent_stmt_execute", error);
        };
        let Some(database) = database_in(&statement.connection) else {
            return misused("inillucent_stmt_execute", error);
        };
        if out.is_null() {
            return misused("inillucent_stmt_execute", error);
        }
        let connection = database.database.session_as(statement.connection.session);
        match connection.query(&statement.sql, &statement.params, capped(limit)) {
            Ok(rows) => {
                *out = publish(Box::new(built(rows)));
                INILLUCENT_OK
            }
            Err(why) => {
                let status = why.status as i32;
                report(error, &why, why.detail.is_some());
                status
            }
        }
    })
}
/// Runs a statement with these values bound in place of what was bound, as
/// `inillucent_bind_json` followed by `inillucent_stmt_execute` would.
///
/// The body of the Python binding's one call per execution (task-2197). The
/// values replace the statement's bindings, as `inillucent_bind_json` replaces
/// them, so a later `inillucent_stmt_execute` runs with what this one ran with.
/// Answers `None` for a handle that is not live, which the caller reports as a
/// misuse.
///
/// @param stmt - the statement
/// @param values - the values of `?1`, `?2`, ...
/// @param limit - how many rows to hand back
///
/// # Safety
///
/// `stmt` must be null or a pointer this library returned.
pub(crate) unsafe fn run_with_values(
    stmt: *mut inillucent_stmt,
    values: Vec<Value>,
    limit: u64,
) -> Option<inillucent_driver::Result<inillucent_driver::Rows>> {
    held(stmt as *const inillucent_stmt)?;
    let statement = stmt.as_mut()?;
    let database = database_in(&statement.connection)?;
    if let Err(why) = within_declared(statement, values.len()) {
        return Some(Err(why));
    }
    statement.params = values;
    let connection = database.database.session_as(statement.connection.session);
    Some(connection.query(&statement.sql, &statement.params, capped(limit)))
}

/// Reads the JSON text a caller passed, refusing a null pointer or bytes that
/// are not UTF-8.
///
/// @param json - the text
/// @param len - its length in bytes
///
/// # Safety
///
/// `json` must be null or point to `len` readable bytes.
unsafe fn json_text<'a>(json: *const c_char, len: usize) -> Option<&'a str> {
    if json.is_null() {
        return None;
    }
    std::str::from_utf8(std::slice::from_raw_parts(json as *const u8, len)).ok()
}
/// Refuses a list of values longer than the statement declares parameters.
///
/// The same bound [`bind`] checks one index against, for the reason it gives.
///
/// @param statement - the statement
/// @param count - how many values arrived
fn within_declared(statement: &inillucent_stmt, count: usize) -> inillucent_driver::Result<()> {
    if count <= statement.declared as usize {
        return Ok(());
    }
    Err(inillucent_driver::Error::said(
        inillucent_driver::Status::InvalidState,
        format!(
            "{count} values were bound to a statement that declares {} parameters.",
            statement.declared
        ),
    ))
}
/// Binds every parameter of the next execution from one JSON array.
///
/// **One call where a binding made one call a value** (task-2191). It replaces
/// whatever was bound before, the way `inillucent_clear_bindings` followed by
/// a bind of each value would. The values are written the way
/// `inillucent_rows_json` writes them, so a value read out of one result binds
/// into the next statement unchanged. `true` and `false` bind as 1 and 0, and
/// the bare words `Infinity`, `-Infinity` and `NaN` are read as reals, because
/// Python's `json.dumps` writes them.
///
/// @param stmt - the statement
/// @param json - a JSON array of values
/// @param len - its length in bytes
/// @param error - where a failure goes, or null
///
/// # Safety
///
/// `stmt` must be a live handle and `json` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn inillucent_bind_json(
    stmt: *mut inillucent_stmt,
    json: *const c_char,
    len: usize,
    error: *mut *mut inillucent_error,
) -> i32 {
    guarded("inillucent_bind_json", error, || {
        if held(stmt as *const inillucent_stmt).is_none() {
            return misused("inillucent_bind_json", error);
        }
        let (Some(statement), Some(text)) = (stmt.as_mut(), json_text(json, len)) else {
            return misused("inillucent_bind_json", error);
        };
        let read = inillucent_driver::wire::params_from_json(text)
            .and_then(|values| within_declared(statement, values.len()).map(|()| values));
        match read {
            Ok(values) => {
                statement.params = values;
                INILLUCENT_OK
            }
            Err(why) => {
                let status = why.status as i32;
                report(error, &why, false);
                status
            }
        }
    })
}
/// Runs a statement once for each list of parameters, and reports how many rows
/// the executions changed in all.
///
/// **`executemany` in one call** (task-2191). A binding that ran an `INSERT`
/// ten thousand times made thirty thousand foreign calls to do it; this makes
/// one, and the statement keeps its compiled plan across the executions.
/// `json` is a JSON array of arrays, each read the way `inillucent_bind_json`
/// reads one. Nothing is wrapped in a transaction that the caller did not open,
/// so outside one each execution commits by itself, as it would have when run
/// one at a time. A row longer than the statement's parameters is refused
/// before any row runs. Otherwise the first failure stops the run and is
/// reported, and the executions before it stand, as they would have.
///
/// The parameters bound with `inillucent_bind_*` are left as they were.
///
/// @param stmt - the statement
/// @param json - a JSON array of arrays of values
/// @param len - its length in bytes
/// @param changed - where the total of rows changed goes, or null
/// @param error - where a failure goes, or null
///
/// # Safety
///
/// `stmt` must be a live handle, `json` must point to `len` readable bytes, and
/// `changed` must be null or writable.
#[no_mangle]
pub unsafe extern "C" fn inillucent_stmt_execute_many(
    stmt: *mut inillucent_stmt,
    json: *const c_char,
    len: usize,
    changed: *mut u64,
    error: *mut *mut inillucent_error,
) -> i32 {
    guarded("inillucent_stmt_execute_many", error, || {
        let Some(text) = json_text(json, len) else {
            return misused("inillucent_stmt_execute_many", error);
        };
        run_rows(
            stmt,
            inillucent_driver::wire::param_rows_from_json(text),
            || inillucent_driver::wire::param_rows_from_json(text),
            changed,
            error,
            "inillucent_stmt_execute_many",
        )
    })
}

/// Runs a statement once for each row, after refusing a row longer than its
/// parameters, and reports the rows changed in all.
///
/// The body both batch calls share: `inillucent_stmt_execute_many` with rows
/// read from JSON, and `inillucent_stmt_execute_params` with rows read from
/// Python objects. A failure to read them arrives as `rows` and is reported
/// the same way as a failure to run them.
///
/// The rows move into the engine, and `again` reads them a second time only
/// when the one statement they run as fails and they run one at a time to find
/// the failing row. See `inillucent_driver::Statement::execute_many_owned`.
///
/// @param stmt - the statement
/// @param rows - the values of each execution, or why they could not be read
/// @param again - reads the same rows again
/// @param changed - where the total of rows changed goes, or null
/// @param error - where a failure goes, or null
/// @param entry - the entry point's name, for a misuse report
///
/// # Safety
///
/// `stmt` must be a live handle and `changed` null or writable.
pub(crate) unsafe fn run_rows(
    stmt: *mut inillucent_stmt,
    rows: inillucent_driver::Result<Vec<Vec<Value>>>,
    again: impl FnOnce() -> inillucent_driver::Result<Vec<Vec<Value>>>,
    changed: *mut u64,
    error: *mut *mut inillucent_error,
    entry: &str,
) -> i32 {
    let Some(statement) = held(stmt as *const inillucent_stmt) else {
        return misused(entry, error);
    };
    let Some(database) = database_in(&statement.connection) else {
        return misused(entry, error);
    };
    let connection = database.database.session_as(statement.connection.session);
    let mut total = 0u64;
    let outcome = rows.and_then(|rows| {
        for row in &rows {
            within_declared(statement, row.len())?;
        }
        let mut prepared = connection.prepare(&statement.sql)?;
        prepared.execute_many_owned(rows, again, &mut total)
    });
    if !changed.is_null() {
        *changed = total;
    }
    match outcome {
        Ok(()) => INILLUCENT_OK,
        Err(why) => {
            let status = why.status as i32;
            report(error, &why, why.detail.is_some());
            status
        }
    }
}

impl Handle for inillucent_stmt {
    const MAGIC: u32 = 0x5244_4234;
    fn live(&self) -> &Live {
        &self.live
    }
}

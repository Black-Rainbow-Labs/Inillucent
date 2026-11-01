// Loads the engine's WebAssembly module and calls its C ABI from JavaScript.
//
// The module exports the same functions `drivers/inillucent-driver-capi`
// exports everywhere else, and every argument of every one of them is an
// integer, a float or a pointer into the module's memory. So this file is the
// whole binding: it copies strings and bytes in through
// `inillucent_buffer_alloc`, reads results back out of linear memory, and turns
// a returned error into an exception. Nothing here decides anything about SQL;
// it only moves values across.

import { MemoryFileSystem } from './memory-fs.mjs';
import { WasiExit, WasiHost, missingImports } from './wasi.mjs';

/** The ABI major this file was written against. Only the major has to match. */
export const ABI_MAJOR = 1;

/** The largest limit the C ABI takes, which means every row. */
const NO_LIMIT = 0xffffffffffffffffn;

/** Value kinds, as `inillucent_value_type` reports them. */
export const ValueKind = { Null: 0, Integer: 1, Real: 2, Text: 3, Blob: 4 };

/** Flags for `inillucent_open`. */
export const OpenFlags = { CREATE: 0x0001, READONLY: 0x0002, DIAGNOSTICS: 0x0004 };

/** Capability states, as `inillucent_capability` reports them. */
export const Support = { No: 0, Yes: 1, Partial: -1, Unknown: -2 };

/** Status names, by the C ABI's frozen status codes. */
const STATUS_NAMES = [
  'ok', 'unsupported', 'syntax', 'not_found', 'constraint', 'readonly', 'busy',
  'interrupted', 'corrupt', 'io', 'full', 'too_big', 'invalid_state', 'internal',
];

/**
 * Returns the name of a status code.
 *
 * @param status - a status code
 */
export function statusName(status) {
  return STATUS_NAMES[status] ?? `status ${status}`;
}

/**
 * Something the engine refused, with the driver's status.
 */
export class InillucentError extends Error {
  /**
   * @param status - the status code
   * @param message - the engine's own words
   * @param feature - the construct not built, for `unsupported`
   * @param detail - diagnostic text, when the database was opened with diagnostics
   * @param offset - the byte offset into the statement, when there is one
   */
  constructor(status, message, feature, detail, offset) {
    const where = offset === undefined ? '' : ` at byte ${offset}`;
    super(`${message} [${statusName(status)}]${where}`);
    this.name = 'InillucentError';
    /** The engine's own words, without the status and offset this message adds. */
    this.engineMessage = message;
    this.status = status;
    this.statusName = statusName(status);
    this.feature = feature;
    this.detail = detail;
    this.offset = offset;
  }
}

/** The engine has not built the construct. Rewording the SQL does not help. */
export class UnsupportedError extends InillucentError {
  constructor(...args) {
    super(...args);
    this.name = 'UnsupportedError';
  }
}

/** The module could not be loaded, or it stopped and cannot be used again. */
export class EngineLoadError extends Error {
  /** @param message - what went wrong */
  constructor(message) {
    super(message);
    this.name = 'EngineLoadError';
  }
}

/**
 * The instance stopped in the middle of a call and cannot be used again.
 *
 * `reason` is `stack` when the statement nested deeper than the JavaScript
 * engine's call stack allows, `trap` for any other WebAssembly trap (a Rust
 * panic aborts, which is a trap), and `exit` when the module called
 * `proc_exit`. The files are outside the instance, in its file system, so a
 * new engine on the same file system opens the databases again and recovers
 * them the way a process that was killed recovers.
 */
export class EngineStoppedError extends EngineLoadError {
  /**
   * @param reason - `stack`, `trap` or `exit`
   * @param message - what went wrong
   * @param cause - what the call threw
   */
  constructor(reason, message, cause) {
    super(message);
    this.name = 'EngineStoppedError';
    this.reason = reason;
    this.cause = cause;
  }
}

/**
 * Says why a call into the instance threw, or null when it was an ordinary JavaScript error.
 *
 * @param error - what the call threw
 */
function stopReason(error) {
  if (error instanceof WasiExit) return 'exit';
  if (error instanceof WebAssembly.RuntimeError) return 'trap';
  // V8 and JavaScriptCore say "Maximum call stack size exceeded";
  // SpiderMonkey says "too much recursion", as an InternalError.
  if (error instanceof RangeError && /call stack|recursion/i.test(error.message)) return 'stack';
  if (error?.name === 'InternalError' && /recursion/i.test(error.message)) return 'stack';
  return null;
}

/**
 * Compiles the module from whatever the caller gave, or from `dist/inillucent.wasm` beside this package.
 *
 * @param source - a `WebAssembly.Module`, bytes, a `Response`, a URL or a path; or undefined
 */
async function compile(source) {
  if (source instanceof WebAssembly.Module) return source;
  if (source instanceof ArrayBuffer || ArrayBuffer.isView(source)) return WebAssembly.compile(source);
  if (typeof Response !== 'undefined' && source instanceof Response) return compileResponse(source);
  const url = source === undefined ? new URL('../dist/inillucent.wasm', import.meta.url) : new URL(String(source), import.meta.url);
  if (url.protocol === 'file:') {
    // A specifier held in a variable, so a bundler building for a browser does
    // not try to resolve a Node module it cannot have; only Node reaches here.
    const nodeFiles = 'node:fs/promises';
    const { readFile } = await import(/* webpackIgnore: true */ /* @vite-ignore */ nodeFiles);
    return WebAssembly.compile(await readFile(url));
  }
  return compileResponse(await fetch(url));
}

/**
 * Compiles a fetched module, streaming when the server sent the right type.
 *
 * @param response - the fetch response
 */
async function compileResponse(response) {
  if (!response.ok) throw new EngineLoadError(`fetching ${response.url} answered HTTP ${response.status}`);
  if (WebAssembly.compileStreaming && response.headers.get('content-type')?.startsWith('application/wasm')) {
    return WebAssembly.compileStreaming(response);
  }
  return WebAssembly.compile(await response.arrayBuffer());
}

/**
 * One instance of the engine: its own memory, its own files, its own databases.
 */
export class Engine {
  /**
   * Compiles and instantiates the module, and checks its ABI.
   *
   * @param options - `wasm` (the module, see `compile`), `fileSystem` (default a new
   *   `MemoryFileSystem`), `stdout`, `stderr` and `env`
   */
  static async load(options = {}) {
    const module = await compile(options.wasm);
    const fileSystem = options.fileSystem ?? new MemoryFileSystem();
    const host = new WasiHost(fileSystem, options);
    const imports = host.imports();
    const missing = missingImports(module, imports);
    if (missing.length > 0) {
      throw new EngineLoadError(`the module imports WASI functions this host does not provide: ${missing.join(', ')}`);
    }
    const instance = await WebAssembly.instantiate(module, imports);
    host.attach(instance.exports.memory);
    return new Engine(instance, host, fileSystem, module);
  }

  /**
   * @param instance - the WebAssembly instance
   * @param host - its WASI host
   * @param fileSystem - its files
   * @param module - the compiled module, so a worker can be given the same one
   */
  constructor(instance, host, fileSystem, module) {
    this.exports = instance.exports;
    this.host = host;
    this.fs = fileSystem;
    this.module = module;
    this.trapped = null;
    this.scratch = this.raw('inillucent_buffer_alloc', 64);
    const reported = this.raw('inillucent_abi_version');
    const major = Math.floor(reported / 1_000_000);
    if (major !== ABI_MAJOR) {
      throw new EngineLoadError(`the module reports ABI ${major}.x and this package was written for ${ABI_MAJOR}.x`);
    }
    this.abiVersion = `${major}.${Math.floor(reported / 1000) % 1000}.${reported % 1000}`;
    this.version = this.cString(this.raw('inillucent_version'));
  }

  /**
   * Calls an export, and turns a trap into an error that says the instance is spent.
   *
   * A Rust panic in this build aborts, which WebAssembly reports as a trap. The
   * instance's memory is then in whatever state the panic left it, so every
   * later call is refused rather than run on it.
   *
   * @param name - the export
   * @param args - its arguments
   */
  raw(name, ...args) {
    if (this.trapped) throw new EngineStoppedError(this.trapped.reason, `the engine stopped earlier (${this.trapped.message}); load a new one`);
    const fn = this.exports[name];
    if (typeof fn !== 'function') throw new EngineLoadError(`the module does not export ${name}`);
    try {
      return fn(...args);
    } catch (error) {
      const reason = stopReason(error);
      if (reason === null) throw error;
      const message = reason === 'stack'
        ? "the statement nested deeper than this JavaScript engine's call stack allows"
        : `${error.message}. This is a defect; please report it with the statement that caused it`;
      this.trapped = { reason, message };
      throw new EngineStoppedError(reason, `the engine stopped: ${message}`, error);
    }
  }

  /** Returns the bytes of linear memory, fresh because growing it replaces the buffer. */
  bytes() {
    return new Uint8Array(this.exports.memory.buffer);
  }

  /** Returns a view over linear memory, fresh for the same reason. */
  view() {
    return new DataView(this.exports.memory.buffer);
  }

  /**
   * Copies bytes into a new buffer in the module's memory. Free it with `free`.
   *
   * @param bytes - what to copy
   * @param terminate - whether to add a NUL byte after them
   */
  copyIn(bytes, terminate = false) {
    const length = bytes.length + (terminate ? 1 : 0);
    const pointer = this.raw('inillucent_buffer_alloc', length) >>> 0;
    if (pointer === 0) throw new InillucentError(10, `the engine could not allocate ${length} bytes`);
    const memory = this.bytes();
    memory.set(bytes, pointer);
    if (terminate) memory[pointer + bytes.length] = 0;
    return { pointer, length };
  }

  /**
   * Frees a buffer `copyIn` made.
   *
   * @param buffer - what `copyIn` returned
   */
  free(buffer) {
    this.raw('inillucent_buffer_free', buffer.pointer, buffer.length);
  }

  /**
   * Runs a call with some strings copied in as C strings, freeing them afterwards.
   *
   * @param texts - the strings
   * @param body - called with their pointers
   */
  withStrings(texts, body) {
    const buffers = texts.map((text) => (text === null || text === undefined ? null : this.copyIn(new TextEncoder().encode(text), true)));
    try {
      return body(...buffers.map((buffer) => (buffer ? buffer.pointer : 0)));
    } finally {
      // After a trap the instance is spent and freeing would only hide the first error.
      if (!this.trapped) for (const buffer of buffers) if (buffer) this.free(buffer);
    }
  }

  /**
   * Reads a NUL terminated string out of the module's memory.
   *
   * @param pointer - where it starts; 0 reads as null
   */
  cString(pointer) {
    pointer >>>= 0;
    if (!pointer) return null;
    const memory = this.bytes();
    let end = pointer;
    while (memory[end] !== 0) end += 1;
    return new TextDecoder().decode(memory.subarray(pointer, end));
  }

  /**
   * Returns the address of one of the eight byte out parameter cells, zeroed.
   *
   * @param nth - which cell, 0 to 7
   */
  cell(nth) {
    const address = this.scratch + nth * 8;
    this.view().setBigUint64(address, 0n, true);
    return address;
  }

  /**
   * Reads a pointer an out parameter cell holds.
   *
   * @param address - the cell
   */
  pointerAt(address) {
    return this.view().getUint32(address, true);
  }

  /**
   * Throws the error a failed call left in an error cell, freeing it.
   *
   * @param status - what the call returned
   * @param errorCell - the cell the call wrote its error to
   */
  check(status, errorCell) {
    if (status === 0) return;
    const error = this.pointerAt(errorCell);
    if (!error) throw new InillucentError(status, 'the engine reported a failure and no error');
    const code = this.raw('inillucent_error_status', error);
    const message = this.cString(this.raw('inillucent_error_message', error)) ?? '';
    const feature = this.cString(this.raw('inillucent_error_feature', error)) ?? undefined;
    const detail = this.cString(this.raw('inillucent_error_detail', error)) ?? undefined;
    const at = this.raw('inillucent_error_offset', error);
    this.raw('inillucent_error_free', error);
    const Kind = code === 1 ? UnsupportedError : InillucentError;
    throw new Kind(code, message, feature, detail, at < 0 ? undefined : at);
  }

  /**
   * Runs a call that takes an out handle and an error, and returns the handle.
   *
   * @param call - called with the out cell and the error cell
   */
  handleOut(call) {
    const out = this.cell(0);
    const error = this.cell(1);
    this.check(call(out, error), error);
    return this.pointerAt(out);
  }

  /** Returns every capability this build declares: `{ name, state, note }`. */
  capabilities() {
    const list = [];
    const count = this.raw('inillucent_capability_count');
    for (let nth = 0; nth < count; nth += 1) {
      const name = this.cell(0);
      const state = this.cell(1);
      const note = this.cell(2);
      if (this.raw('inillucent_capability', nth, name, state, note) !== 0) continue;
      list.push({
        name: this.cString(this.pointerAt(name)),
        state: this.view().getInt32(state, true),
        note: this.cString(this.pointerAt(note)) ?? '',
      });
    }
    return list;
  }

  /**
   * Returns one capability's state, `Support.Unknown` for a name this build does not know.
   *
   * @param name - the capability
   */
  supports(name) {
    return this.withStrings([name], (pointer) => this.raw('inillucent_supports', pointer));
  }

  /**
   * Opens or creates a database and returns its handle.
   *
   * @param path - the file's path inside this engine's file system
   * @param options - `create` (default true), `readonly`, `diagnostics`, `key` (an encryption key) and `busyTimeoutMs`
   */
  open(path, options = {}) {
    let flags = options.create === false ? 0 : OpenFlags.CREATE;
    if (options.readonly) flags = (flags & ~OpenFlags.CREATE) | OpenFlags.READONLY;
    if (options.diagnostics) flags |= OpenFlags.DIAGNOSTICS;
    if (options.key !== undefined && options.key !== null) {
      return this.withStrings([path, options.key], (p, k) => this.handleOut((out, error) => this.raw('inillucent_open_with_key', p, flags, k, out, error)));
    }
    if (options.busyTimeoutMs !== undefined) {
      return this.withStrings([path], (p) => this.handleOut((out, error) => this.raw('inillucent_open_with_timeout', p, flags, options.busyTimeoutMs, out, error)));
    }
    return this.withStrings([path], (p) => this.handleOut((out, error) => this.raw('inillucent_open', p, flags, out, error)));
  }

  /**
   * Runs a call whose only outputs are a status and an error.
   *
   * @param call - called with the error cell
   */
  statusOnly(call) {
    const error = this.cell(1);
    this.check(call(error), error);
  }

  /** @param db - a database handle */
  close(db) {
    this.statusOnly((error) => this.raw('inillucent_close', db, error));
  }

  /** @param db - a database handle */
  checkpoint(db) {
    this.statusOnly((error) => this.raw('inillucent_checkpoint', db, error));
  }

  /** @param db - a database handle */
  integrityCheck(db) {
    this.statusOnly((error) => this.raw('inillucent_integrity_check', db, error));
  }

  /**
   * Writes a consistent copy of the database to another path in this engine's file system.
   *
   * @param db - a database handle
   * @param path - where the copy goes
   */
  backupTo(db, path) {
    this.withStrings([path], (p) => this.statusOnly((error) => this.raw('inillucent_backup_to', db, p, error)));
  }

  /** @param db - a database handle */
  databasePath(db) {
    return this.cString(this.raw('inillucent_path', db));
  }

  /**
   * Opens a connection to a database.
   *
   * @param db - a database handle
   */
  connect(db) {
    return this.handleOut((out, error) => this.raw('inillucent_connect', db, out, error));
  }

  /** @param conn - a connection handle */
  connectionFree(conn) {
    this.raw('inillucent_conn_free', conn);
  }

  /**
   * Runs one statement, binding parameters when there are any, and returns its result.
   *
   * @param conn - a connection handle
   * @param sql - the statement
   * @param params - values for `?1`, `?2` and so on
   * @param limit - the most rows to hand back; undefined for all
   */
  execute(conn, sql, params = [], limit = undefined) {
    if (params.length > 0) {
      const stmt = this.prepare(conn, sql);
      try {
        params.forEach((value, index) => this.bind(stmt, index + 1, value));
        return this.run(stmt, limit);
      } finally {
        this.statementFree(stmt);
      }
    }
    const rows = this.withStrings([sql], (s) => this.handleOut((out, error) => this.raw('inillucent_execute', conn, s, toLimit(limit), out, error)));
    return this.readRows(rows);
  }

  /**
   * Runs several statements separated by semicolons, with no parameters and no result.
   *
   * @param conn - a connection handle
   * @param sql - the statements
   */
  executeBatch(conn, sql) {
    this.withStrings([sql], (s) => this.statusOnly((error) => this.raw('inillucent_execute_batch', conn, s, error)));
  }

  /**
   * Compiles a statement.
   *
   * @param conn - a connection handle
   * @param sql - the statement
   */
  prepare(conn, sql) {
    return this.withStrings([sql], (s) => this.handleOut((out, error) => this.raw('inillucent_prepare', conn, s, out, error)));
  }

  /**
   * Binds one value to a parameter, choosing the call by the value's JavaScript type.
   *
   * @param stmt - a statement handle
   * @param index - the parameter, from 1
   * @param value - null, a boolean, a number, a bigint, a string or bytes
   */
  bind(stmt, index, value) {
    let status;
    if (value === null || value === undefined) status = this.raw('inillucent_bind_null', stmt, index);
    else if (typeof value === 'boolean') status = this.raw('inillucent_bind_int', stmt, index, value ? 1n : 0n);
    else if (typeof value === 'bigint') status = this.raw('inillucent_bind_int', stmt, index, BigInt.asIntN(64, value));
    else if (typeof value === 'number') {
      status = Number.isSafeInteger(value) && !Object.is(value, -0)
        ? this.raw('inillucent_bind_int', stmt, index, BigInt(value))
        : this.raw('inillucent_bind_real', stmt, index, value);
    } else if (typeof value === 'string' || value instanceof ArrayBuffer || ArrayBuffer.isView(value)) {
      const text = typeof value === 'string';
      const bytes = text ? new TextEncoder().encode(value) : toBytes(value);
      const buffer = this.copyIn(bytes);
      try {
        status = this.raw(text ? 'inillucent_bind_text' : 'inillucent_bind_blob', stmt, index, buffer.pointer, bytes.length);
      } finally {
        if (!this.trapped) this.free(buffer);
      }
    } else {
      throw new TypeError(`cannot bind a ${typeof value}: the engine stores NULL, integers, reals, text and bytes`);
    }
    if (status !== 0) throw new InillucentError(status, `could not bind parameter ${index}`);
  }

  /** @param stmt - a statement handle */
  clearBindings(stmt) {
    this.raw('inillucent_clear_bindings', stmt);
  }

  /**
   * Runs a prepared statement with what is bound.
   *
   * @param stmt - a statement handle
   * @param limit - the most rows to hand back; undefined for all
   */
  run(stmt, limit = undefined) {
    return this.readRows(this.handleOut((out, error) => this.raw('inillucent_stmt_execute', stmt, toLimit(limit), out, error)));
  }

  /** @param stmt - a statement handle */
  statementFree(stmt) {
    this.raw('inillucent_stmt_free', stmt);
  }

  /** @param conn - a connection handle */
  lastInsertRowid(conn) {
    return fromInteger(this.raw('inillucent_last_insert_rowid', conn));
  }

  /** @param conn - a connection handle */
  totalChanges(conn) {
    return fromInteger(this.raw('inillucent_total_changes', conn));
  }

  /** @param conn - a connection handle */
  inTransaction(conn) {
    return this.raw('inillucent_in_transaction', conn) !== 0;
  }

  /** @param conn - a connection handle */
  schemaCookie(conn) {
    return fromInteger(BigInt.asIntN(64, this.raw('inillucent_schema_cookie', conn)));
  }

  /** @param conn - a connection handle */
  cancel(conn) {
    this.statusOnly((error) => this.raw('inillucent_cancel', conn, error));
  }

  /**
   * Begins a transaction the caller holds open.
   *
   * @param conn - a connection handle
   */
  transactionBegin(conn) {
    return this.handleOut((out, error) => this.raw('inillucent_txn_begin', conn, out, error));
  }

  /**
   * Runs one statement inside a transaction and returns how many rows it changed.
   *
   * @param txn - a transaction handle
   * @param sql - the statement
   */
  transactionExecute(txn, sql) {
    return this.withStrings([sql], (s) => {
      const affected = this.cell(2);
      const error = this.cell(1);
      this.check(this.raw('inillucent_txn_execute', txn, s, affected, error), error);
      return Number(this.view().getBigUint64(affected, true));
    });
  }

  /** @param txn - a transaction handle; spent either way */
  transactionCommit(txn) {
    this.statusOnly((error) => this.raw('inillucent_txn_commit', txn, error));
  }

  /**
   * Frees a transaction, rolling it back when it was neither committed nor failed.
   *
   * @param txn - a transaction handle
   */
  transactionFree(txn) {
    this.raw('inillucent_txn_rollback', txn);
  }

  /**
   * Reads a whole result out of the module and frees it.
   *
   * The cell accessors are used rather than `inillucent_rows_json`: a call into
   * the instance costs nanoseconds here, and `JSON.parse` would lose every
   * integer past 2^53.
   *
   * @param rows - a rows handle
   */
  readRows(rows) {
    try {
      const count = this.raw('inillucent_rows_column_count', rows);
      const columns = [];
      const types = [];
      for (let nth = 0; nth < count; nth += 1) {
        columns.push(this.cString(this.raw('inillucent_rows_column_name', rows, nth)) ?? '');
        types.push(this.cString(this.raw('inillucent_rows_column_type', rows, nth)) ?? '');
      }
      const handed = this.raw('inillucent_rows_count', rows) >>> 0;
      const values = [];
      for (let row = 0; row < handed; row += 1) {
        const cells = [];
        for (let column = 0; column < count; column += 1) cells.push(this.readCell(rows, row, column));
        values.push(cells);
      }
      const affected = Number(this.raw('inillucent_rows_affected', rows));
      return {
        columns,
        types,
        rows: values,
        total: this.raw('inillucent_rows_total', rows) >>> 0,
        more: this.raw('inillucent_rows_more', rows) !== 0,
        affected: affected < 0 ? null : affected,
        elapsedMicros: Number(this.raw('inillucent_rows_elapsed_us', rows)),
        tag: this.cString(this.raw('inillucent_rows_tag', rows)) ?? '',
      };
    } finally {
      this.raw('inillucent_rows_free', rows);
    }
  }

  /**
   * Reads one cell.
   *
   * @param rows - a rows handle
   * @param row - the row
   * @param column - the column
   */
  readCell(rows, row, column) {
    const kind = this.raw('inillucent_value_type', rows, row, column);
    if (kind === ValueKind.Null) return null;
    if (kind === ValueKind.Integer) return fromInteger(this.raw('inillucent_value_int', rows, row, column));
    if (kind === ValueKind.Real) return this.raw('inillucent_value_real', rows, row, column);
    const length = this.cell(3);
    const pointer = this.raw('inillucent_value_bytes', rows, row, column, length) >>> 0;
    const size = this.view().getUint32(length, true);
    const bytes = pointer ? this.bytes().slice(pointer, pointer + size) : new Uint8Array(0);
    return kind === ValueKind.Text ? new TextDecoder().decode(bytes) : bytes;
  }
}

/**
 * Turns an `int64_t` the module returned into a number when it fits exactly, and a bigint when not.
 *
 * @param value - a bigint
 */
export function fromInteger(value) {
  const whole = BigInt(value);
  return whole >= BigInt(Number.MIN_SAFE_INTEGER) && whole <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(whole) : whole;
}

/**
 * Turns a row limit into the `uint64_t` the C ABI takes.
 *
 * @param limit - a count, or undefined for every row
 */
function toLimit(limit) {
  if (limit === undefined || limit === null) return NO_LIMIT;
  if (typeof limit === 'bigint') return BigInt.asUintN(64, limit);
  if (!Number.isInteger(limit) || limit < 0) throw new TypeError(`a limit is a whole number of rows, not ${limit}`);
  return BigInt(limit);
}

/**
 * Returns the bytes of an `ArrayBuffer` or a view as a `Uint8Array`.
 *
 * @param value - the buffer or view
 */
function toBytes(value) {
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
}

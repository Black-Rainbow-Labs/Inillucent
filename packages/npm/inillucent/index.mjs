// inillucent, from Node.
//
// **This is not a database binding.** It is the four programs, plus one
// convenience for calling the command line and getting a typed result back.
// A real binding would go through the C ABI in
// `drivers/inillucent-driver-capi/include/inillucent_driver.h` with koffi or an
// N-API addon, and `drivers/README.md` says exactly how to write one - it is a
// worthwhile thing to have and it is not this.
//
// What this is for: a script or an agent harness that wants to run a query
// without shelling out by hand and parsing a table. Each call is a process, so
// it is the wrong tool for a loop over a million rows and the right one for the
// dozen calls a build script or a tool wrapper makes.

import { execFile, spawn as spawnProcess } from 'node:child_process';
import { existsSync, statSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';
import { promisify } from 'node:util';

import { resolveBinary, resolveLibrary, PROGRAMS, platformPackage } from './resolve.mjs';

const run = promisify(execFile);

/**
 * Encodes one bound value as the JSON text the command line reads.
 *
 * **`JSON.stringify` loses three things a parameter can be (task-1979, D13 and
 * D16).** A NaN and an Infinity both become `null`, so a REAL column silently
 * stored NULL and nothing said so; `-0` becomes `0`, so the sign of negative
 * zero was gone before the value left Node; and a byte string has no JSON form
 * at all, so there was no way to bind a BLOB.
 *
 * What is written instead: a non-finite number throws here, where the caller
 * can see which value it was, rather than turning into a NULL nobody asked for.
 * Negative zero is written `-0.0`, which JSON's own grammar carries and the
 * command line's parser reads back as a negative zero. Bytes are written
 * `{"blob":"<hex>"}`.
 *
 * @param value - one element of `params`
 */
function encodeParam(value) {
  if (typeof value === 'number') {
    if (!Number.isFinite(value)) {
      throw new TypeError(
        `a parameter cannot be ${value}: SQL has no spelling for NaN or Infinity.`,
      );
    }
    return Object.is(value, -0) ? '-0.0' : JSON.stringify(value);
  }
  if (value instanceof Uint8Array) {
    const hex = Buffer.from(value).toString('hex');
    return JSON.stringify({ blob: hex });
  }
  if (Array.isArray(value)) {
    return `[${value.map(encodeParam).join(',')}]`;
  }
  return JSON.stringify(value === undefined ? null : value);
}

/**
 * Encodes the whole `params` array as JSON text.
 *
 * @param values - the bound values, in order
 */
function encodeParams(values) {
  return `[${values.map(encodeParam).join(',')}]`;
}

export { resolveBinary, resolveLibrary, PROGRAMS, platformPackage };

/**
 * Runs a program, writing `stdin` to it when there is any.
 *
 * `execFile` cannot write to standard input, so a call that has something to
 * write goes through `spawn` and the two are answered the same way: `{ stdout,
 * stderr }`, or a rejection carrying both plus the exit code, which is the
 * shape the caller below already handles.
 *
 * @param binary - the program to run
 * @param args - its command line
 * @param stdin - what to write to its standard input, or null
 * @param env - the environment the program runs with
 */
async function spawnWith(binary, args, stdin, env) {
  if (stdin === null) {
    return run(binary, args, { maxBuffer: 256 * 1024 * 1024, env });
  }
  const { spawn } = await import('node:child_process');
  return new Promise((resolve, reject) => {
    const child = spawn(binary, args, { stdio: ['pipe', 'pipe', 'pipe'], env });
    let stdout = '';
    let stderr = '';
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
    });
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
    });
    child.on('error', reject);
    child.on('close', (code) => {
      if (code === 0) {
        resolve({ stdout, stderr });
        return;
      }
      const why = new Error(`inillucent exited ${code}`);
      why.stdout = stdout;
      why.stderr = stderr;
      why.code = code;
      reject(why);
    });
    child.stdin.on('error', () => {});
    child.stdin.end(stdin);
  });
}

/**
 * Builds the environment a child program runs with.
 *
 * The key of an encrypted database travels in `INILLUCENT_KEY` and never on the
 * command line, because a command line is visible in the process list. With no
 * key the inherited environment is passed on untouched.
 *
 * @param key - the key text of an encrypted database, or undefined
 */
function childEnvironment(key) {
  if (key === undefined || key === null) {
    return process.env;
  }
  return { ...process.env, INILLUCENT_KEY: String(key) };
}

/**
 * Runs one inillucent command and returns its result object.
 *
 * The command line's `--output json` contract: `{ ok, command, columns, rows,
 * total, more, changes, last_insert_rowid, elapsed_ms, text }` on success, and
 * `{ ok: false, status, message, ... }` on failure. `status` is one of the
 * driver's thirteen names, and `unsupported` is its own - a construct the
 * engine has not built is not a syntax error and should not be handled as one.
 *
 * A failure is returned rather than thrown, because the interesting failures
 * here are answers: "no such table", "this construct is not built yet". Only a
 * broken invocation throws.
 *
 * @param command - the verb, such as "query" or "describe"
 * @param options - the named arguments the verb takes, plus `db` and `key`
 *   (the key of an encrypted database, sent in `INILLUCENT_KEY`)
 */
export async function inillucent(command, options = {}) {
  // `query`, `exec` and `batch` run in this process when the addon is there; see
  // `nativeAddon`. Everything else, and an encrypted database, starts a program.
  if (runsNatively(command, options)) {
    return nativeOnce(command, options);
  }
  const { db, key, ...rest } = options;
  const args = [command, '--output', 'json'];
  let stdin = null;
  if (db) {
    args.push('--db', db);
  }
  for (const [name, value] of Object.entries(rest)) {
    if (value === undefined || value === null) {
      continue;
    }
    if (value === true) {
      args.push(`--${name}`);
      continue;
    }
    if (value === false) {
      continue;
    }
    // **`params` travels on standard input (task-1979, D17).** A command line
    // has a length ceiling - about 32 KB on Windows - and a parameter past it
    // failed with an operating system error rather than with anything about
    // SQL. `--params-file -` has no such limit, and it is also what lets the
    // encoding above carry bytes and a negative zero.
    if (name === 'params' && Array.isArray(value)) {
      stdin = encodeParams(value);
      args.push('--params-file', '-');
      continue;
    }
    // Any other array is a JSON argument - `vector` is the one - and the
    // command line reads it as JSON, so it is serialised rather than joined.
    args.push(`--${name}`, Array.isArray(value) ? JSON.stringify(value) : String(value));
  }
  const binary = resolveBinary('inillucent');
  try {
    const { stdout } = await spawnWith(binary, args, stdin, childEnvironment(key));
    return JSON.parse(stdout);
  } catch (why) {
    // A non-zero exit still prints the result object on standard output when
    // `--output json` was asked for, so the failure the caller wants is in
    // there. Only a truly broken invocation has nothing to parse.
    if (typeof why.stdout === 'string' && why.stdout.trim().startsWith('{')) {
      return JSON.parse(why.stdout);
    }
    throw new Error(
      `inillucent ${command} could not be run: ${why.stderr || why.message}`,
    );
  }
}

/**
 * Runs a query and returns its rows as objects keyed by column name.
 *
 * The shape most callers actually want. `inillucent()` is there for the ones
 * that need the counts, the timing or the failure class.
 *
 * @param sql - the statement
 * @param options - `db`, `key`, `params`, `limit`
 */
export async function query(sql, options = {}) {
  return rowsOf(await inillucent('query', { sql, ...options }));
}

/**
 * Turns a query's result object into its rows, keyed by column name, or throws
 * the failure it carries.
 *
 * @param result - what `inillucent('query', ...)` returned
 */
function rowsOf(result) {
  if (!result.ok) {
    const error = new Error(result.message);
    error.status = result.status;
    error.feature = result.feature;
    throw error;
  }
  const names = result.columns.map((column) => column.name);
  return result.rows.map((row) =>
    Object.fromEntries(names.map((name, at) => [name, decodeValue(row[at])])),
  );
}

/**
 * Turns one cell of a result into the JavaScript value it stands for.
 *
 * **Bytes went in and could not come back** (task-2066 section 4.1.15). A blob
 * left as the string `"x'00ff'"`, typed `text` in the column list, so nothing
 * told it apart from a TEXT column holding that text - while `encodeParam`
 * above has always sent bytes as `{blob: "<hex>"}`. The two halves of the same
 * grammar now agree, and a `Uint8Array` read out of one query can be bound
 * straight into the next.
 *
 * Anything that is not an envelope is passed through, so a caller's own object
 * column - which arrives as text - is untouched.
 *
 * @param value - one cell, as `--output json` rendered it
 */
function decodeValue(value) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return value;
  }
  if (typeof value.blob === 'string') {
    return Uint8Array.from(Buffer.from(value.blob, 'hex'));
  }
  return value;
}

/**
 * Opens a session: one long lived `inillucent-mcp` process over the database,
 * which every call on the session reuses.
 *
 * **One process for many calls, where `inillucent()` and `query()` start one a
 * call** (task-2191). Starting a program costs about 17 ms on Windows before it
 * reads a byte of the database, so a script that ran a hundred queries through
 * `query()` spent nearly two seconds starting programs. A session sends each
 * call down the server's standard input and reads the answer back, which is
 * well under a millisecond for a small query. The answers are the same objects
 * `inillucent()` returns, because the server writes the command line's
 * `--output json` result for each call.
 *
 * Close the session when you are done. An idle session does not keep Node
 * running, and a call that is waiting for its answer does.
 *
 * The server opens only a database that is there, so a missing file is made
 * first with `inillucent create`, the way a first write through `inillucent()`
 * makes one. Pass `create: false` to be refused instead.
 *
 * @param db - the database file
 * @param options - `key` (the key of an encrypted database, sent in
 *   `INILLUCENT_KEY`), `readonly` (refuse every write), `root` (refuse every
 *   path outside this directory), `create` (make a missing file, default true)
 */
export async function open(db, options = {}) {
  // In this process when the addon is there and nothing asks for what only the
  // server does: a key, which the addon cannot open with, or a root, which the
  // server enforces on every path.
  const native = nativeAddon();
  if (native !== null && options.key === undefined && !options.root) {
    if (options.create === false && !existsSync(db)) {
      throw Object.assign(new Error(`there is no database at ${db}.`), { status: 'not_found' });
    }
    return new NativeSession(native, db, options);
  }
  return openServer(db, options);
}

/**
 * Starts the `inillucent-mcp` session `open()` answers when the addon cannot
 * serve it, and that a `NativeSession` starts for the commands it does not
 * run itself.
 *
 * @param db - the database file
 * @param options - what `open()` was given
 */
async function openServer(db, options) {
  const env = childEnvironment(options.key);
  if (options.create !== false && !options.readonly && !existsSync(db)) {
    await spawnWith(resolveBinary('inillucent'), ['create', db, '--output', 'json'], null, env);
  }
  const args = ['--db', db];
  if (options.readonly) {
    args.push('--readonly');
  }
  if (options.root) {
    args.push('--root', options.root);
  }
  const session = new Session(resolveBinary('inillucent-mcp'), args, env);
  await session.ready;
  return session;
}

/**
 * A database held open by one `inillucent-mcp` process. Made by `open()`.
 */
export class Session {
  /**
   * Starts the server and sends the MCP handshake.
   *
   * @param binary - the `inillucent-mcp` program
   * @param args - its command line
   * @param env - the environment it runs with
   */
  constructor(binary, args, env) {
    this.child = spawnProcess(binary, args, {
      stdio: ['pipe', 'pipe', 'inherit'],
      env,
      windowsHide: true,
    });
    this.next = 1;
    this.waiting = new Map();
    this.buffered = '';
    this.failure = null;
    this.child.stdout.setEncoding('utf8');
    this.child.stdout.on('data', (chunk) => this.receive(chunk));
    this.child.on('error', (why) => this.fail(why));
    this.child.on('close', (code) => this.fail(new Error(`inillucent-mcp exited ${code}`)));
    this.child.stdin.on('error', () => {});
    this.ready = this.request('initialize', {
      protocolVersion: '2025-06-18',
      capabilities: {},
      clientInfo: { name: 'inillucent-node', version: '1' },
    }).then(() => this.send({ jsonrpc: '2.0', method: 'notifications/initialized' }));
  }

  /**
   * Runs one inillucent command and returns its result object, as
   * `inillucent()` does.
   *
   * @param command - the verb, such as "query" or "exec"
   * @param options - the named arguments the verb takes
   */
  async inillucent(command, options = {}) {
    await this.ready;
    const { params, ...rest } = options;
    const named = {};
    for (const [name, value] of Object.entries(rest)) {
      if (value !== undefined && value !== null && value !== false) {
        named[name] = value;
      }
    }
    named.output = 'json';
    // The parameters are spliced in as the text `encodeParams` writes, because
    // JSON.stringify would lose negative zero and cannot spell a blob, which
    // are the reasons `encodeParams` exists.
    let argumentsText = JSON.stringify(named);
    if (Array.isArray(params)) {
      argumentsText = `${argumentsText.slice(0, -1)},"params":${encodeParams(params)}}`;
    }
    const answer = await this.request(
      'tools/call',
      null,
      `{"name":${JSON.stringify(`inillucent_${command}`)},"arguments":${argumentsText}}`,
    );
    const text = answer?.content?.[0]?.text;
    if (typeof text !== 'string' || !text.trim().startsWith('{')) {
      throw new Error(`inillucent ${command} did not answer with a result: ${text}`);
    }
    return JSON.parse(text);
  }

  /**
   * Runs a query and returns its rows as objects keyed by column name, as
   * `query()` does.
   *
   * @param sql - the statement
   * @param options - `params`, `limit`
   */
  async query(sql, options = {}) {
    return rowsOf(await this.inillucent('query', { sql, ...options }));
  }

  /**
   * Stops the server, and waits for it to exit. Calls still waiting are refused.
   */
  async close() {
    if (this.child.exitCode !== null) {
      return;
    }
    const exited = new Promise((resolve) => this.child.once('exit', resolve));
    // Held while it exits, or Node would leave before the server had finished.
    this.hold(true);
    this.child.stdin.end();
    await exited;
  }

  /**
   * Sends one JSON-RPC request and waits for its answer.
   *
   * @param method - the method
   * @param params - its parameters as a value, or null when `paramsText` is given
   * @param paramsText - its parameters as JSON text already written
   */
  request(method, params, paramsText) {
    if (this.failure) {
      return Promise.reject(this.failure);
    }
    const id = this.next++;
    const body = paramsText ?? JSON.stringify(params);
    return new Promise((resolve, reject) => {
      this.waiting.set(id, { resolve, reject });
      this.hold(true);
      this.child.stdin.write(`{"jsonrpc":"2.0","id":${id},"method":"${method}","params":${body}}\n`);
    });
  }

  /**
   * Sends a message that has no answer.
   *
   * @param message - the message
   */
  send(message) {
    this.child.stdin.write(`${JSON.stringify(message)}\n`);
  }

  /**
   * Reads answers out of what the server wrote, one per line.
   *
   * @param chunk - text from the server's standard output
   */
  receive(chunk) {
    this.buffered += chunk;
    let end;
    while ((end = this.buffered.indexOf('\n')) >= 0) {
      const line = this.buffered.slice(0, end);
      this.buffered = this.buffered.slice(end + 1);
      if (!line.trim()) {
        continue;
      }
      const message = JSON.parse(line);
      const waiter = this.waiting.get(message.id);
      if (!waiter) {
        continue;
      }
      this.waiting.delete(message.id);
      if (message.error) {
        waiter.reject(new Error(message.error.message));
      } else {
        waiter.resolve(message.result);
      }
    }
    if (this.waiting.size === 0) {
      this.hold(false);
    }
  }

  /**
   * Refuses every waiting call, because the server has gone.
   *
   * @param why - what happened
   */
  fail(why) {
    this.failure = this.failure ?? why;
    for (const waiter of this.waiting.values()) {
      waiter.reject(why);
    }
    this.waiting.clear();
  }

  /**
   * Keeps Node running while a call waits for its answer, and lets it exit
   * while nothing does.
   *
   * @param waiting - whether a call is waiting
   */
  hold(waiting) {
    for (const handle of [this.child, this.child.stdout, this.child.stdin]) {
      if (waiting) {
        handle.ref?.();
      } else {
        handle.unref?.();
      }
    }
  }
}

/** `INILLUCENT_OPEN_CREATE` and `INILLUCENT_OPEN_READONLY` from the C header. */
const OPEN_CREATE = 0x0001;
const OPEN_READONLY = 0x0002;

/** The addon, once loaded: `undefined` before the first look, `null` when there is none. */
let addon;

/**
 * Loads the C library as a Node addon, once, or answers null when it cannot.
 *
 * **The library is the addon** (task-2191). It exports Node's addon entry
 * point and finds Node's `napi_*` functions in this process, so the file the
 * platform package already ships under `lib/` loads with `process.dlopen` and
 * runs every statement in this process. A call through it costs what the
 * statement costs, where starting the `inillucent` program cost about 18 ms. A
 * library from before this, or none at all, answers null, and every call goes
 * through a program as it did.
 */
function nativeAddon() {
  if (addon !== undefined) {
    return addon;
  }
  addon = null;
  const path = resolveLibrary();
  if (path === null || process.env.INILLUCENT_NO_ADDON) {
    return addon;
  }
  try {
    const module = { exports: {} };
    process.dlopen(module, path);
    if (typeof module.exports.query === 'function') {
      addon = module.exports;
    }
  } catch {
    addon = null;
  }
  return addon;
}

/**
 * Runs one statement through the addon and answers the result object the
 * command line's `--output json` writes for `query` and `exec`.
 *
 * @param native - the addon
 * @param conn - the session's handle
 * @param command - `query` or `exec`
 * @param options - `sql`, `params`, `limit`
 */
function nativeCall(native, conn, command, options) {
  const started = performance.now();
  try {
    const result = native.query(conn, String(options.sql), options.params, true);
    const limit = options.limit === undefined || options.limit === null ? 200 : Number(options.limit);
    const kept = command === 'query' && limit > 0 ? result.rows.slice(0, limit) : result.rows;
    return {
      ok: true,
      command,
      columns: result.columns.map((name) => ({ name, type: '' })),
      rows: kept,
      row_count: kept.length,
      total: result.total,
      more: kept.length < result.rows.length,
      changes: result.changes,
      last_insert_rowid: result.lastInsertRowid,
      elapsed_ms: performance.now() - started,
    };
  } catch (why) {
    return { ok: false, command, status: why.code ?? 'internal', message: why.message };
  }
}

/**
 * Runs a `batch` through the addon and answers the result object the command
 * line writes for it.
 *
 * @param native - the addon
 * @param conn - the session's handle
 * @param options - `sql`
 */
function nativeBatch(native, conn, options) {
  const started = performance.now();
  try {
    const result = native.batch(conn, String(options.sql));
    return {
      ok: true,
      command: 'batch',
      columns: [],
      rows: [],
      row_count: 0,
      total: 0,
      more: false,
      changes: result.changes,
      last_insert_rowid: 0,
      elapsed_ms: performance.now() - started,
      transaction: result.transaction,
    };
  } catch (why) {
    return { ok: false, command: 'batch', status: why.code ?? 'internal', message: why.message };
  }
}

/**
 * Whether a call can run in this process: the addon is there, the command is
 * one statement, and nothing asks for what only the programs do.
 *
 * @param command - the verb
 * @param options - its options
 */
function runsNatively(command, options) {
  const statement = command === 'query' || command === 'exec' || command === 'batch';
  return statement && options.key === undefined && nativeAddon() !== null;
}

/**
 * Runs one `query`, `exec` or `batch` in this process: open, run, close.
 *
 * A statement that only reads runs on a database kept open from the last such
 * call on the same file, when there is one; see `keptDatabase`.
 *
 * @param command - `query`, `exec` or `batch`
 * @param options - `db`, `sql`, `params`, `limit`
 */
function nativeOnce(command, options) {
  const native = nativeAddon();
  const kept = command !== 'batch' && onlyReads(options.sql) ? keptDatabase(native, options.db) : null;
  if (kept !== null) {
    return nativeCall(native, kept.conn, command, options);
  }
  const flags = command === 'query' ? 0 : OPEN_CREATE;
  let db;
  try {
    db = native.open(String(options.db ?? ':memory:'), flags);
  } catch (why) {
    return { ok: false, command, status: why.code ?? 'internal', message: why.message };
  }
  const conn = native.connect(db);
  try {
    return command === 'batch' ? nativeBatch(native, conn, options) : nativeCall(native, conn, command, options);
  } finally {
    native.disconnect(conn);
    native.close(db);
  }
}

/** How long a kept database stays open after its last call, in milliseconds. */
const KEPT_IDLE_MS = 1000;

/** The database kept open between one shot reads: `{ path, ino, dev, db, conn, timer }`, or null. */
let keptOne = null;

/**
 * Whether a statement's text starts with `SELECT`, `WITH` or `VALUES`.
 *
 * Only such a statement runs on a kept database. **A kept connection must not
 * carry anything from one call into the next**, and a statement that reads
 * leaves nothing on it: no open transaction, no setting, no temporary table,
 * no attached file. A `BEGIN`, a `PRAGMA` or an `ATTACH` run through `exec()`
 * would, so those still open and close a database of their own.
 *
 * @param sql - the statement
 */
function onlyReads(sql) {
  const head = /^\s*([A-Za-z]+)/.exec(String(sql ?? ''));
  if (head === null) {
    return false;
  }
  const word = head[1].toUpperCase();
  return word === 'SELECT' || word === 'WITH' || word === 'VALUES';
}

/**
 * Answers a database and connection open on `db`, kept from an earlier call
 * when that call was on the same file, or null when nothing can be kept.
 *
 * **Opening is most of a one shot query's cost.** An open reads the file's
 * header and meta pages, checks for a journal and the log, reads the log and
 * loads the schema. On the bench's 10,000 row table that was 420 us of a 423 us
 * call, against 88 us for the whole of `node:sqlite`'s open, query and close.
 * A kept connection costs what the statement costs.
 *
 * Keeping it open changes nothing a caller can see. An idle connection holds
 * no file lock. Each statement takes the lock and reads the meta record, and
 * reloads when another connection or process has written since, which is how
 * a session from `open()` already sees other writers. The file is checked to
 * be the same file on every call: a database deleted and created again at the
 * same path has a new file id, and the kept one is closed. It is closed after
 * `KEPT_IDLE_MS` with no call, so the file is not held by a process that has
 * finished with it, and the addon closes it when Node exits.
 *
 * @param native - the addon
 * @param db - the database file
 */
function keptDatabase(native, db) {
  if (db === undefined || db === null || db === ':memory:' || String(db) === '') {
    return null;
  }
  const path = resolvePath(String(db));
  const stat = statSync(path, { bigint: true, throwIfNoEntry: false });
  if (stat === undefined) {
    closeKept();
    return null;
  }
  if (keptOne !== null && (keptOne.path !== path || keptOne.ino !== stat.ino || keptOne.dev !== stat.dev)) {
    closeKept();
  }
  if (keptOne === null) {
    let opened;
    try {
      opened = native.open(path, 0);
    } catch {
      return null;
    }
    keptOne = { path, ino: stat.ino, dev: stat.dev, db: opened, conn: native.connect(opened), timer: null };
  }
  clearTimeout(keptOne.timer);
  keptOne.timer = setTimeout(closeKept, KEPT_IDLE_MS);
  keptOne.timer.unref?.();
  return keptOne;
}

/** Closes the kept database, if there is one. */
function closeKept() {
  if (keptOne === null) {
    return;
  }
  const { db, conn, timer } = keptOne;
  keptOne = null;
  clearTimeout(timer);
  const native = nativeAddon();
  native.disconnect(conn);
  native.close(db);
}

/**
 * A database held open in this process through the addon. Made by `open()`.
 *
 * It answers `query` and `exec` itself. Any other command starts one
 * `inillucent-mcp` session beside it, the first time one is asked for.
 */
export class NativeSession {
  /**
   * Opens the database.
   *
   * @param native - the addon
   * @param db - the database file
   * @param options - what `open()` was given
   */
  constructor(native, db, options) {
    this.native = native;
    this.path = db;
    this.options = options;
    this.db = native.open(db, options.readonly ? OPEN_READONLY : OPEN_CREATE);
    this.conn = native.connect(this.db);
    this.server = null;
  }

  /**
   * Runs one inillucent command and returns its result object, as
   * `inillucent()` does.
   *
   * @param command - the verb, such as "query" or "exec"
   * @param options - the named arguments the verb takes
   */
  async inillucent(command, options = {}) {
    if (command === 'query' || command === 'exec') {
      return nativeCall(this.native, this.conn, command, options);
    }
    if (command === 'batch') {
      return nativeBatch(this.native, this.conn, options);
    }
    if (this.server === null) {
      this.server = await openServer(this.path, this.options);
    }
    return this.server.inillucent(command, options);
  }

  /**
   * Runs a query and returns its rows as objects keyed by column name, as
   * `query()` does.
   *
   * @param sql - the statement
   * @param options - `params`, `limit`
   */
  async query(sql, options = {}) {
    let result;
    try {
      result = this.native.query(this.conn, String(sql), options.params, false);
    } catch (why) {
      const error = new Error(why.message);
      error.status = why.code ?? 'internal';
      throw error;
    }
    const limit = options.limit === undefined || options.limit === null ? 200 : Number(options.limit);
    return limit > 0 ? result.rows.slice(0, limit) : result.rows;
  }

  /**
   * Closes the database, and the server when one was started.
   */
  async close() {
    if (this.server !== null) {
      await this.server.close();
      this.server = null;
    }
    if (this.db !== null) {
      this.native.disconnect(this.conn);
      this.native.close(this.db);
      this.db = null;
    }
  }
}

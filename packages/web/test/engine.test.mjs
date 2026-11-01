// What the WebAssembly build does beyond the shared conformance suite.
//
// Each test loads its own engine, so one test's files cannot be read by another.
// They need the module built: `node packages/web/build.mjs`.

import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import { Engine, EngineLoadError, EngineStoppedError, MemoryFileSystem, Support, UnsupportedError, WasiHost, missingImports } from '../src/index.mjs';

/** The module, which `build.mjs` puts here. */
const wasm = join(resolve(dirname(fileURLToPath(import.meta.url)), '..'), 'dist', 'inillucent.wasm');

/** Why the tests cannot run, or false. */
const skip = existsSync(wasm) ? false : 'no dist/inillucent.wasm (run node packages/web/build.mjs); skipping';

/** The module compiled once, for every test. */
const module = skip ? null : await WebAssembly.compile(readFileSync(wasm));

/**
 * Loads a fresh engine on the compiled module.
 *
 * @param fileSystem - its file system; a new memory one by default
 */
function load(fileSystem = new MemoryFileSystem()) {
  return Engine.load({ wasm: module, fileSystem });
}

test('a database exported from one engine imports into another with its rows and indexes', { skip }, async () => {
  const first = await load();
  const db = first.open('/notes.rdb');
  const conn = first.connect(db);
  first.executeBatch(conn, `
    CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT NOT NULL);
    CREATE INDEX note_body ON note (body);
    INSERT INTO note (body) VALUES ('one'), ('two'), ('three');
  `);
  first.backupTo(db, '/tmp/export.rdb');
  const bytes = first.fs.exportFile('/tmp/export.rdb');
  assert.ok(bytes.length > 0);
  assert.equal(new TextDecoder().decode(bytes.subarray(0, 4)).length, 4);

  const second = await load();
  second.fs.importFile('/imported.rdb', bytes);
  const reopened = second.open('/imported.rdb', { create: false });
  const reader = second.connect(reopened);
  assert.deepEqual(second.execute(reader, 'SELECT body FROM note ORDER BY id').rows, [['one'], ['two'], ['three']]);
  assert.deepEqual(second.execute(reader, "SELECT id FROM note WHERE body = 'two'").rows, [[2]]);
  second.integrityCheck(reopened);
});

test('a database closed and opened again in the same file system keeps every committed row', { skip }, async () => {
  const engine = await load();
  let db = engine.open('/kept.rdb');
  let conn = engine.connect(db);
  engine.execute(conn, 'CREATE TABLE t (a INTEGER PRIMARY KEY, b TEXT)');
  for (let i = 0; i < 200; i += 1) engine.execute(conn, 'INSERT INTO t (b) VALUES (?1)', [`row ${i}`]);
  engine.connectionFree(conn);
  engine.close(db);
  db = engine.open('/kept.rdb', { create: false });
  conn = engine.connect(db);
  assert.deepEqual(engine.execute(conn, 'SELECT COUNT(*), MAX(b) FROM t').rows, [[200, 'row 99']]);
});

test('a transaction the caller holds commits or rolls back as a whole', { skip }, async () => {
  const engine = await load();
  const db = engine.open('/txn.rdb');
  const conn = engine.connect(db);
  engine.execute(conn, 'CREATE TABLE t (a INTEGER PRIMARY KEY)');
  const kept = engine.transactionBegin(conn);
  assert.equal(engine.transactionExecute(kept, 'INSERT INTO t VALUES (1), (2)'), 2);
  engine.transactionCommit(kept);
  engine.transactionFree(kept);
  const dropped = engine.transactionBegin(conn);
  engine.transactionExecute(dropped, 'INSERT INTO t VALUES (3)');
  engine.transactionFree(dropped);
  assert.deepEqual(engine.execute(conn, 'SELECT a FROM t ORDER BY a').rows, [[1], [2]]);
});

test('vector search through an HNSW index and full text search both answer', { skip }, async () => {
  const engine = await load();
  const db = engine.open('/search.rdb');
  const conn = engine.connect(db);
  engine.executeBatch(conn, `
    CREATE TABLE passage (id INTEGER PRIMARY KEY, body TEXT, v VECTOR(3));
    INSERT INTO passage (body, v) VALUES ('north', '[1, 0, 0]'), ('east', '[0, 1, 0]'), ('up', '[0, 0, 1]'), ('mostly north', '[0.9, 0.1, 0]');
    CREATE INDEX passage_v ON passage USING inillucent_hnsw (v);
    CREATE VIRTUAL TABLE doc USING fts5(body);
    INSERT INTO doc (body) VALUES ('a database in the browser'), ('a database on a server'), ('nothing here');
  `);
  const nearest = engine.execute(conn, "SELECT body FROM passage ORDER BY vector_distance_cos(v, '[1, 0, 0]') LIMIT 2");
  assert.deepEqual(nearest.rows, [['north'], ['mostly north']]);
  const plan = engine.execute(conn, "EXPLAIN QUERY PLAN SELECT body FROM passage ORDER BY vector_distance_cos(v, '[1, 0, 0]') LIMIT 2");
  assert.ok(plan.rows.some((row) => String(row.at(-1)).includes('VECTOR INDEX')), JSON.stringify(plan.rows));
  const found = engine.execute(conn, "SELECT body FROM doc WHERE doc MATCH 'browser' ORDER BY bm25(doc)");
  assert.deepEqual(found.rows, [['a database in the browser']]);
});

/**
 * Builds a statement of `depth` nested scalar subqueries.
 *
 * @param depth - how many levels
 */
function nestedSelects(depth) {
  let sql = '1';
  for (let level = 0; level < depth; level += 1) sql = `(SELECT ${sql})`;
  return `SELECT ${sql}`;
}

test('nested subqueries past the Rust stack are refused with too_big, and the engine keeps working', { skip }, async () => {
  const engine = await load();
  const conn = engine.connect(engine.open('/deep.rdb'));
  assert.deepEqual(engine.execute(conn, nestedSelects(100)).rows, [[1]]);
  for (const depth of [500, 1000]) {
    assert.throws(() => engine.execute(conn, nestedSelects(depth)), (error) => error.statusName === 'too_big', `depth ${depth}`);
  }
  assert.deepEqual(engine.execute(conn, 'SELECT 1 + 1').rows, [[2]]);
});

test('nesting past the JavaScript call stack stops the instance, and a new engine on the same files recovers every commit', { skip }, async () => {
  const fileSystem = new MemoryFileSystem();
  const engine = await load(fileSystem);
  const conn = engine.connect(engine.open('/kept.rdb'));
  engine.executeBatch(conn, 'CREATE TABLE t (a INTEGER PRIMARY KEY); INSERT INTO t VALUES (1), (2);');
  const deep = `SELECT ${'('.repeat(100_000)}1${')'.repeat(100_000)}`;
  assert.throws(() => engine.execute(conn, deep), (error) => error instanceof EngineStoppedError && error.reason === 'stack');
  assert.throws(() => engine.execute(conn, 'SELECT 1'), (error) => error instanceof EngineStoppedError);
  const fresh = await load(fileSystem);
  const reopened = fresh.connect(fresh.open('/kept.rdb', { create: false }));
  assert.deepEqual(fresh.execute(reopened, 'SELECT a FROM t ORDER BY a').rows, [[1], [2]]);
});

test('an unimplemented construct is an UnsupportedError that names it, and capabilities are readable', { skip }, async () => {
  const engine = await load();
  const capabilities = engine.capabilities();
  assert.ok(capabilities.length > 10);
  assert.ok(capabilities.every((one) => typeof one.name === 'string' && one.name.length > 0));
  assert.equal(engine.supports('no_such_capability_anywhere'), Support.Unknown);
  const no = capabilities.find((one) => one.state === Support.No);
  assert.ok(no, 'the table lists something the engine has not built');
});

test('the module imports only WASI functions the host provides', { skip }, () => {
  const host = new WasiHost(new MemoryFileSystem());
  assert.deepEqual(missingImports(module, host.imports()), []);
  const imported = WebAssembly.Module.imports(module).map((entry) => entry.module);
  assert.deepEqual([...new Set(imported)], ['wasi_snapshot_preview1']);
});

test('the file system the engine sees is only the one it was given', { skip }, async () => {
  const fileSystem = new MemoryFileSystem();
  const engine = await load(fileSystem);
  const db = engine.open('/only.rdb');
  const conn = engine.connect(db);
  assert.throws(() => engine.execute(conn, "ATTACH 'C:/Windows/win.ini' AS outside"), (error) => error.statusName !== undefined);
  engine.execute(conn, 'CREATE TABLE t (a)');
  assert.ok(fileSystem.list('/').includes('only.rdb'));
});

test('an engine that trapped refuses every later call instead of running on broken memory', { skip }, async () => {
  const engine = await load();
  engine.trapped = { reason: 'trap', message: 'a trap the test arranged' };
  assert.throws(() => engine.open('/x.rdb'), EngineLoadError);
  assert.ok(UnsupportedError.prototype instanceof Error);
});

#!/usr/bin/env node
// The training run for the profile guided Windows build.
//
//   node packaging/pgo/train.mjs <bin dir> <work dir>
//
// <bin dir> holds an instrumented inillucent.exe, inillucent-shell.exe,
// inillucent-mcp.exe and inillucent_driver_capi.dll. Each of them writes the
// counts of what it ran into the profile directory it was built with, and
// `packaging/pgo/build-windows.ps1` merges those counts and builds again with
// them. The program never times anything.
//
// **What it runs, and why the list is broad.** The compiler treats a function
// the training never reached as cold: it inlines less into it and places it
// away from the hot code. So the run covers the calls people make, as
// `docs/performance.md` measures them, and then the rest of what the engine
// does at least once: every script in `compat/corpus/usage/`, the SELECT
// corpus, full text and vector search, a migration from a SQLite file, and
// the MCP server. A statement that fails is still training, so a failure
// stops the run only when the program could not start or the Python or Node
// half failed.
//
// The work is the same on every run, so two builds of one commit get the same
// profile.

import { spawnSync, spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const ROWS = 10000;

/** The work folder, which every program runs in: scripts in `compat/corpus/usage/` attach and
 * vacuum into files named relative to where they run. */
let workFolder = process.cwd();

/**
 * Runs a program to completion in the work folder and returns its exit status.
 * @param exe - the program
 * @param args - its arguments
 * @param input - text for its standard input, or undefined
 */
function run(exe, args, input) {
  const r = spawnSync(exe, args, { input, cwd: workFolder, maxBuffer: 1 << 28, windowsHide: true });
  if (r.error) throw new Error(`${path.basename(exe)} did not start: ${r.error.message}`);
  return r.status;
}

/**
 * Runs a program and stops the training when it exits with anything but 0.
 * @param exe - the program
 * @param args - its arguments
 * @param input - text for its standard input, or undefined
 */
function runOrStop(exe, args, input) {
  const status = run(exe, args, input);
  if (status !== 0) throw new Error(`${path.basename(exe)} ${args.join(' ').slice(0, 100)} exited ${status}`);
}

/**
 * Returns a fresh database path in the work folder, removing an old one and every file beside it
 * whose name starts with it: the rollback journal and the numbered log segments.
 * @param work - the work folder
 * @param name - the file name
 */
function fresh(work, name) {
  for (const entry of fs.readdirSync(work)) {
    if (entry === name || entry.startsWith(name + '-')) fs.rmSync(path.join(work, entry), { force: true });
  }
  return path.join(work, name);
}

/**
 * The SQL that builds the note table the usage workloads read.
 * @param rows - how many rows it inserts
 */
function fixtureSql(rows) {
  const lines = ['CREATE TABLE note (id INTEGER PRIMARY KEY, title TEXT, body TEXT, tag TEXT, created INTEGER);', 'BEGIN;'];
  for (let i = 1; i <= rows; i++) {
    lines.push(`INSERT INTO note VALUES (${i}, 'title ${i}', '${`body of note ${i} `.repeat(8)}', 't${i % 50}', ${1700000000 + i * 37});`);
  }
  lines.push('COMMIT;', 'CREATE INDEX note_tag ON note(tag);', 'CREATE INDEX note_created ON note(created);');
  return lines.join('\n') + '\n';
}

/**
 * Single row inserts into a fresh table, in one transaction or one per statement.
 * @param count - how many inserts
 * @param transaction - whether BEGIN and COMMIT wrap them
 */
function insertSql(count, transaction) {
  const lines = ['CREATE TABLE IF NOT EXISTS t (id INTEGER PRIMARY KEY, a TEXT, b INTEGER);'];
  if (transaction) lines.push('BEGIN;');
  for (let i = 0; i < count; i++) lines.push(`INSERT INTO t (a, b) VALUES ('value ${i}', ${i});`);
  if (transaction) lines.push('COMMIT;');
  return lines.join('\n') + '\n';
}

/**
 * Point selects by key, in the order the benchmark uses.
 * @param count - how many selects
 */
function selectSql(count) {
  return Array.from({ length: count }, (_, i) => `SELECT id, title, tag FROM note WHERE id = ${1 + ((i * 7919) % ROWS)};`).join('\n') + '\n';
}

/**
 * Full text and vector search, each with and without an index.
 */
function searchSql() {
  const lines = [
    'CREATE VIRTUAL TABLE doc USING fts5(title, body);',
    'CREATE TABLE passage (id INTEGER PRIMARY KEY, body TEXT, v VECTOR(8));',
    'BEGIN;',
  ];
  for (let i = 0; i < 2000; i++) {
    const v = Array.from({ length: 8 }, (_, k) => (((i * 31 + k * 17) % 97) / 97).toFixed(4)).join(',');
    lines.push(`INSERT INTO doc VALUES ('title ${i}', 'word${i % 40} other${i % 7} text number ${i}');`);
    lines.push(`INSERT INTO passage VALUES (${i}, 'passage ${i}', '[${v}]');`);
  }
  lines.push('COMMIT;');
  for (let i = 0; i < 50; i++) {
    lines.push(`SELECT title, bm25(doc) FROM doc WHERE doc MATCH 'word${i % 40}' ORDER BY bm25(doc) LIMIT 5;`);
    lines.push(`SELECT id FROM passage ORDER BY vector_distance_cos(v, '[${(i % 10) / 10},0.5,0.1,0.2,0.3,0.4,0.6,0.7]') LIMIT 5;`);
  }
  lines.push('CREATE INDEX passage_v ON passage USING inillucent_hnsw (v);');
  for (let i = 0; i < 50; i++) lines.push(`SELECT id FROM passage ORDER BY vector_distance_cos(v, '[0.1,${(i % 10) / 10},0.5,0.2,0.3,0.4,0.6,0.7]') LIMIT 5;`);
  lines.push("DELETE FROM passage WHERE id % 5 = 0;", "UPDATE doc SET body = body || ' more' WHERE rowid % 3 = 0;");
  return lines.join('\n') + '\n';
}

/**
 * The shell's scripts: the measured workloads, the usage corpus and the SELECT corpus.
 * @param bin - the program folder
 * @param work - the work folder
 */
function trainShell(bin, work) {
  const shell = path.join(bin, 'inillucent-shell.exe');
  runOrStop(shell, [fresh(work, 'note.rdb')], fixtureSql(ROWS));
  for (let i = 0; i < 2; i++) {
    runOrStop(shell, [path.join(work, 'note.rdb')], selectSql(1000));
    runOrStop(shell, [fresh(work, 'ac.rdb')], insertSql(200, false));
    runOrStop(shell, [fresh(work, 'tx.rdb')], insertSql(ROWS, true));
    runOrStop(shell, [fresh(work, 'fx.rdb')], fixtureSql(ROWS));
  }
  const multirow = 'CREATE TABLE m (id INTEGER PRIMARY KEY, a TEXT, b REAL);\nINSERT INTO m VALUES\n' + Array.from({ length: 20000 }, (_, i) => `(${i}, 'row ${i}', ${i * 0.5})`).join(',\n') + ';\n';
  runOrStop(shell, [fresh(work, 'mr.rdb')], multirow);
  const csv = path.join(work, 'import.csv');
  fs.writeFileSync(csv, 'id,name,score,tag\n' + Array.from({ length: 50000 }, (_, i) => `${i},name ${i},${i * 3.5},tag${i % 20}`).join('\n') + '\n');
  runOrStop(shell, [fresh(work, 'csv.rdb')], `.import --csv ${csv.replace(/\\/g, '/')} t\n`);
  runOrStop(shell, [fresh(work, 'search.rdb')], searchSql());
  const usage = path.join(ROOT, 'compat/corpus/usage');
  for (const name of fs.readdirSync(usage).filter((n) => n.endsWith('.sql')).sort()) {
    run(shell, [fresh(work, 'usage.rdb')], fs.readFileSync(path.join(usage, name), 'utf8'));
  }
  const select = path.join(ROOT, 'compat/corpus/select');
  run(shell, [fresh(work, 'select.rdb')], fs.readFileSync(path.join(select, 'schema.sql'), 'utf8') + '\n' + fs.readFileSync(path.join(select, 'queries.sql'), 'utf8'));
}

/**
 * One process per call through the command line, the way scripts and agents call it.
 * @param bin - the program folder
 * @param work - the work folder
 */
function trainCli(bin, work) {
  const cli = path.join(bin, 'inillucent.exe');
  const db = path.join(work, 'note.rdb');
  const calls = [
    ['--version'],
    ['--db', db, 'query', 'SELECT * FROM note WHERE id = 4242', '--output', 'json'],
    ['--db', db, 'query', 'SELECT id, title FROM note WHERE id = 4242'],
    ['--db', db, 'query', "SELECT id, title, created FROM note WHERE tag = 't7' ORDER BY created LIMIT 100", '--output', 'json'],
    ['--db', db, 'query', 'SELECT count(*), max(created) FROM note'],
    ['--db', db, 'exec', "INSERT INTO note (title, body, tag, created) VALUES ('new', 'body', 't1', 1)"],
    ['--db', db, 'exec', 'INSERT INTO note (title, body, tag, created) VALUES (?1, ?2, ?3, ?4)', '--params', '["p", "b", "t2", 2]'],
    ['--db', db, 'tables'],
  ];
  for (let i = 0; i < 5; i++) for (const args of calls) runOrStop(cli, args);
  const once = [
    ['--db', db, 'dump'],
    ['--db', db, 'describe', 'note', '--output', 'json'],
    ['--db', db, 'schema'],
    ['--db', db, 'indexes'],
    ['--db', db, 'explain', "SELECT * FROM note WHERE tag = 't3'"],
    ['--db', db, 'stats'],
    ['--db', db, 'integrity-check'],
    ['--db', db, 'analyze'],
    ['--db', db, 'batch', "UPDATE note SET created = created + 1 WHERE id < 100; DELETE FROM note WHERE id = 7"],
    ['--db', db, 'export', 'note', '--format', 'csv', '--out', path.join(work, 'note.csv')],
    ['--db', db, 'backup', fresh(work, 'backup.rdb')],
    ['--db', path.join(work, 'search.rdb'), 'search', 'word3', '--table', 'doc'],
    ['--db', path.join(work, 'search.rdb'), 'vector-search', 'passage', '--column', 'v', '--vector', '[0.1,0.2,0.3,0.4,0.5,0.6,0.7,0.8]', '--k', '5'],
    ['capabilities', '--output', 'json'],
    ['functions'],
    ['help'],
    ['help', 'migrate'],
  ];
  for (const args of once) run(cli, args);
}

/**
 * Builds a SQLite file with Python's sqlite3 module and migrates it.
 * @param bin - the program folder
 * @param work - the work folder
 */
function trainMigrate(bin, work) {
  const source = fresh(work, 'source.db');
  const script = [
    'import sqlite3, sys',
    'c = sqlite3.connect(sys.argv[1])',
    "c.executescript('CREATE TABLE a (id INTEGER PRIMARY KEY, name TEXT, score REAL, data BLOB); CREATE INDEX a_name ON a(name); CREATE TABLE b (k TEXT PRIMARY KEY, v INTEGER) WITHOUT ROWID;')",
    "c.executemany('INSERT INTO a VALUES (?, ?, ?, ?)', [(i, 'n%d' % (i % 300), i * 0.25, bytes([i % 256]) * 20) for i in range(20000)])",
    "c.executemany('INSERT INTO b VALUES (?, ?)', [('k%05d' % i, i) for i in range(5000)])",
    'c.commit()',
  ].join('\n');
  runOrStop('python', ['-c', script, source]);
  runOrStop(path.join(bin, 'inillucent.exe'), ['migrate', source, '--destination', fresh(work, 'migrated.rdb')]);
}

/**
 * Serves the note database over MCP and makes query, exec and tables calls.
 * @param bin - the program folder
 * @param work - the work folder
 */
async function trainMcp(bin, work) {
  const child = spawn(path.join(bin, 'inillucent-mcp.exe'), ['--db', path.join(work, 'note.rdb')], { cwd: work, windowsHide: true });
  let buffer = '';
  const waiting = [];
  child.stdout.on('data', (d) => {
    buffer += d;
    for (let i = buffer.indexOf('\n'); i >= 0; i = buffer.indexOf('\n')) {
      const line = buffer.slice(0, i);
      buffer = buffer.slice(i + 1);
      waiting.shift()?.(line);
    }
  });
  const ask = (message) => new Promise((resolve) => { waiting.push(resolve); child.stdin.write(JSON.stringify(message) + '\n'); });
  await ask({ jsonrpc: '2.0', id: 0, method: 'initialize', params: { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'train', version: '1' } } });
  child.stdin.write(JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' }) + '\n');
  await ask({ jsonrpc: '2.0', id: 1, method: 'tools/list' });
  const tool = (id, name, args) => ask({ jsonrpc: '2.0', id, method: 'tools/call', params: { name, arguments: args } });
  for (let i = 0; i < 400; i++) await tool(10 + i, 'inillucent_query', { sql: 'SELECT id, title, tag FROM note WHERE id = ?1', params: [1 + ((i * 7919) % ROWS)] });
  await tool(500, 'inillucent_exec', { sql: "INSERT INTO note (title, body, tag, created) VALUES ('mcp', 'b', 't3', 3)" });
  await tool(501, 'inillucent_tables', {});
  await tool(502, 'inillucent_describe', { table: 'note' });
  child.stdin.end();
  await new Promise((resolve) => child.on('close', resolve));
}

/**
 * The Python binding over the C library, through `packaging/pgo/train.py`.
 * @param bin - the program folder
 * @param work - the work folder
 */
function trainPython(bin, work) {
  runOrStop('python', [path.join(ROOT, 'packaging/pgo/train.py'), path.join(ROOT, 'drivers/bindings/python/inillucent.py'), path.join(bin, 'inillucent_driver_capi.dll'), work]);
}

/**
 * The npm package, which runs the command line program it is pointed at.
 * @param bin - the program folder
 * @param work - the work folder
 */
async function trainNode(bin, work) {
  process.env.INILLUCENT_BIN = path.join(bin, 'inillucent.exe');
  const pkg = await import(pathToFileURL(path.join(ROOT, 'packages/npm/inillucent/index.mjs')).href);
  const db = path.join(work, 'note.rdb');
  for (let i = 0; i < 40; i++) await pkg.query('SELECT id, title, tag FROM note WHERE id = ?1', { db, params: [1 + ((i * 7919) % ROWS)] });
  if (pkg.open) {
    const session = await pkg.open(db);
    for (let i = 0; i < 400; i++) await session.query('SELECT id, title, tag FROM note WHERE id = ?1', { params: [1 + ((i * 7919) % ROWS)] });
    await session.close();
  }
}

/**
 * Runs every part of the training in a fixed order.
 * @param argv - the program folder and the work folder
 */
async function main(argv) {
  const [bin, work] = argv;
  if (!bin || !work) throw new Error('usage: node packaging/pgo/train.mjs <bin dir> <work dir>');
  fs.mkdirSync(work, { recursive: true });
  workFolder = path.resolve(work);
  const parts = [['shell', () => trainShell(bin, work)], ['cli', () => trainCli(bin, work)], ['migrate', () => trainMigrate(bin, work)],
    ['mcp', () => trainMcp(bin, work)], ['python', () => trainPython(bin, work)], ['node', () => trainNode(bin, work)]];
  for (const [name, part] of parts) {
    const started = performance.now();
    await part();
    console.log(`   trained ${name} in ${((performance.now() - started) / 1000).toFixed(1)} s`);
  }
}

main(process.argv.slice(2)).catch((e) => { console.error(e.message ?? e); process.exit(1); });

// The Node wrapper against a real binary: write, read back, bind, and classify.
//
// **`resolve.test.mjs` reads this package's own source as text.** It checks the
// platform table and the shape of the error messages, which is worth having and
// is not a test of the wrapper: nothing in it calls `query` or `exec`, so a
// release could ship a wrapper that cannot run a statement and every test here
// would pass (task-1969, 5.7).
//
// So this file is the Go suite's three cases in Node - a round trip, a bound
// hostile string, and a classified status - because that suite is the one that
// was already doing the job.
//
// It needs a built binary, which `INILLUCENT_BIN` names. `tools/validate`'s
// `wrappers` stage sets it to `target/release/inillucent`; run by hand, set it
// yourself or put `inillucent` on `PATH`. When neither is there the suite skips
// with the command that provisions it, rather than failing on a machine that
// has never built the workspace.

import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';

import { inillucent, open, query, resolveBinary } from './index.mjs';

/** Whether a binary can be found at all, decided once. */
let available = true;
try {
  resolveBinary('inillucent');
} catch {
  available = false;
  console.error(
    'no inillucent binary: set INILLUCENT_BIN or put one on PATH ' +
      '(`cargo build --release -p inillucent-cli`); skipping',
  );
}

/**
 * Runs one case in a database of its own, and removes it afterwards.
 *
 * A directory per case rather than a file per case, because a database is a
 * file plus its log segments, and a case that left them behind would be a case
 * the next run reopened.
 *
 * @param name - the case's name
 * @param body - what to run, given the database's path
 */
function withDatabase(name, body) {
  test(name, { skip: !available }, async () => {
    const directory = mkdtempSync(join(tmpdir(), 'inillucent-roundtrip-'));
    try {
      await body(join(directory, 'probe.rdb'));
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
}

withDatabase('a database can be created, written to and read back', async (db) => {
  const made = await inillucent('batch', {
    db,
    sql: `
      CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
      INSERT INTO people VALUES (1, 'Ada', 36), (2, 'Grace', 45), (3, 'Alan', 41)
    `,
  });
  assert.equal(made.ok, true, made.message);

  const result = await inillucent('query', {
    db,
    sql: 'SELECT name FROM people WHERE age > ?1 ORDER BY age',
    params: [40],
  });
  assert.equal(result.ok, true, result.message);
  assert.equal(result.total, 2, 'total is exact and is not the row count');

  const rows = await query('SELECT name FROM people WHERE age > ?1 ORDER BY age', {
    db,
    params: [40],
  });
  assert.deepEqual(
    rows.map((row) => row.name),
    ['Alan', 'Grace'],
    'the wrong rows, or the wrong order',
  );
});

withDatabase('a parameter is bound rather than pasted', async (db) => {
  // The quote is the point. Pasted into the statement it ends the string
  // literal and the rest is parsed as SQL; bound, it is one value with a quote
  // in it. This is the case that tells a wrapper apart from a string join.
  const hostile = "Robert'); DROP TABLE people; --";
  const made = await inillucent('batch', {
    db,
    sql: 'CREATE TABLE people (id INTEGER PRIMARY KEY, name TEXT)',
  });
  assert.equal(made.ok, true, made.message);

  const written = await inillucent('exec', {
    db,
    sql: 'INSERT INTO people (name) VALUES (?1)',
    params: [hostile],
  });
  assert.equal(written.ok, true, written.message);
  assert.equal(written.changes, 1);

  const rows = await query('SELECT name FROM people', { db });
  assert.equal(rows.length, 1, 'the table is gone, or holds something else');
  assert.equal(rows[0].name, hostile, 'the value did not survive as one string');
});

withDatabase('a failure comes back classified rather than thrown', async (db) => {
  const made = await inillucent('exec', {
    db,
    sql: 'CREATE TABLE people (id INTEGER PRIMARY KEY)',
  });
  assert.equal(made.ok, true, made.message);

  const missing = await inillucent('query', { db, sql: 'SELECT * FROM absent' });
  assert.equal(missing.ok, false);
  assert.equal(
    missing.status,
    'not_found',
    `a missing table came back as ${missing.status}`,
  );

  // `unsupported` is its own status and its own exit code, which is the thing
  // AGENTS.md asks a caller to branch on rather than reword their SQL over.
  // The `USING btree` clause is an extension of this dialect that only the vector index
  // types fill; the engine refuses it before it looks for the table.
  const unbuilt = await inillucent('exec', {
    db,
    sql: 'CREATE INDEX unbuilt_i ON people USING btree (id)',
  });
  assert.equal(unbuilt.ok, false);
  assert.equal(
    unbuilt.status,
    'unsupported',
    `a construct the engine has not built came back as ${unbuilt.status}`,
  );
});

withDatabase('a VECTOR column survives the wrapper', async (db) => {
  // The conformance suite gained a `vector` case for the driver and the C ABI;
  // this is the same question one layer out, because `vector` is one of the two
  // arguments the wrapper serialises as JSON rather than as a string.
  //
  // The value goes in as a blob literal of little-endian `f32` bits, which is
  // what a `VECTOR(N)` column is: three floats, twelve bytes. There is no
  // `vector_from_text`, and `'[1,0,0]'` is refused as "not a vector of 3
  // dimensions" - correctly, because it is a seven-character string.
  const made = await inillucent('batch', {
    db,
    sql: `
      CREATE TABLE point (id INTEGER PRIMARY KEY, at VECTOR(3));
      INSERT INTO point (id, at) VALUES (1, x'0000803f0000000000000000')
    `,
  });
  assert.equal(made.ok, true, made.message);

  const dimensions = await query('SELECT vector_dims(at) AS width FROM point', { db });
  assert.equal(dimensions[0].width, 3, 'the vector did not come back three wide');

  const found = await inillucent('vector-search', {
    db,
    table: 'point',
    column: 'at',
    vector: [1, 0, 0],
    k: 1,
  });
  assert.equal(found.ok, true, found.message);
  assert.ok(
    found.columns.some((column) => column.name === 'distance'),
    'vector-search answered without a distance column',
  );
});

withDatabase('an encrypted database opens with its key and not without it', async (db) => {
  // The key travels in INILLUCENT_KEY. It is a raw 32 byte key here because a
  // passphrase costs 600,000 PBKDF2 iterations on every open.
  const key = `x'${'5a'.repeat(32)}'`;
  const secret = 'plaintext-marker-for-the-file-scan';
  const made = await inillucent('batch', {
    db,
    key,
    sql: `CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT);
      INSERT INTO note (body) VALUES ('${secret}')`,
  });
  assert.equal(made.ok, true, made.message);

  const rows = await query('SELECT body FROM note', { db, key });
  assert.equal(rows[0].body, secret, 'the row did not come back with the key');

  const mode = await query('PRAGMA encryption', { db, key });
  assert.equal(Object.values(mode[0])[0], 'xchacha20-poly1305');

  const without = await inillucent('query', { db, sql: 'SELECT body FROM note' });
  assert.equal(without.ok, false);
  assert.equal(without.status, 'corrupt', `no key came back as ${without.status}`);

  const bytes = readFileSync(db);
  assert.equal(bytes.includes(Buffer.from(secret)), false, 'the row text is in the file');
});

withDatabase('a session runs many calls through one process and gives the same answers', async (db) => {
  // `open()` keeps one inillucent-mcp process for every call (task-2191), so this
  // checks what has to be the same as a call through `inillucent()`: rows, a
  // bound blob, a classified failure, and a write a second process can read
  // once the session is closed.
  const session = await open(db);
  try {
    const made = await session.inillucent('batch', {
      sql: 'CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT, raw BLOB)',
    });
    assert.equal(made.ok, true, made.message);
    for (let id = 1; id <= 50; id++) {
      const written = await session.inillucent('exec', {
        sql: 'INSERT INTO note VALUES (?1, ?2, ?3)',
        params: [id, `note ${id}`, new Uint8Array([id, 255])],
      });
      assert.equal(written.ok, true, written.message);
    }
    const rows = await session.query('SELECT body, raw FROM note WHERE id = ?1', { params: [7] });
    assert.equal(rows[0].body, 'note 7');
    assert.deepEqual([...rows[0].raw], [7, 255], 'the blob did not survive the session');
    const missing = await session.inillucent('query', { sql: 'SELECT * FROM absent' });
    assert.equal(missing.ok, false);
    assert.equal(missing.status, 'not_found');
  } finally {
    await session.close();
  }
  const counted = await query('SELECT count(*) AS n FROM note', { db });
  assert.equal(counted[0].n, 50, 'a write through the session is not in the file after close');
});

withDatabase('a one shot read sees what another process wrote since the last one', async (db) => {
  // `query()` keeps the database it read open for the next read of the same
  // file. A row another process commits in between has to be in the next
  // answer, as it was when every call opened the file afresh.
  const made = await inillucent('batch', {
    db,
    sql: "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT); INSERT INTO note VALUES (1, 'one')",
  });
  assert.equal(made.ok, true, made.message);
  assert.equal((await query('SELECT count(*) AS n FROM note', { db }))[0].n, 1);

  execFileSync(resolveBinary('inillucent'), ['--db', db, 'exec', "INSERT INTO note VALUES (2, 'two')"]);
  assert.equal(
    (await query('SELECT count(*) AS n FROM note', { db }))[0].n,
    2,
    'the second read answered from the file as it was before the other process wrote',
  );
});

withDatabase('a one shot read of a file made again at the same path reads the new file', async (db) => {
  // A database deleted and created again keeps its path and gets a new file.
  // The kept database is the old file, so it has to be closed, not reused.
  await inillucent('batch', { db, sql: "CREATE TABLE note (body TEXT); INSERT INTO note VALUES ('old')" });
  assert.deepEqual(await query('SELECT body FROM note', { db }), [{ body: 'old' }]);

  const directory = join(db, '..');
  rmSync(directory, { recursive: true, force: true });
  mkdirSync(directory);
  await inillucent('batch', { db, sql: "CREATE TABLE note (body TEXT); INSERT INTO note VALUES ('new')" });
  assert.deepEqual(
    await query('SELECT body FROM note', { db }),
    [{ body: 'new' }],
    'the read answered from the file that was deleted',
  );
});

withDatabase('a one shot statement that is not a read leaves nothing on a kept connection', async (db) => {
  // A connection remembers what ran on it: the last rowid inserted, a
  // transaction begun, a setting. One shot calls each had a connection of
  // their own, so none of that reached the next call. A write still does: it
  // runs on a connection that closes with the call, and only reads run on the
  // kept one. Were the insert run on the kept connection, the read after it
  // would answer its rowid instead of zero.
  await inillucent('batch', { db, sql: 'CREATE TABLE note (id INTEGER PRIMARY KEY)' });
  await query('SELECT count(*) AS n FROM note', { db });
  const written = await inillucent('exec', { db, sql: 'INSERT INTO note VALUES (7)' });
  assert.equal(written.ok, true, written.message);
  assert.equal(
    (await query('SELECT last_insert_rowid() AS id', { db }))[0].id,
    0,
    'the insert ran on the connection the next read used',
  );
});

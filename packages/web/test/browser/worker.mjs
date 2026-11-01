// The worker the browser test drives: the engine on OPFS, one request at a time.
//
// A request is `{ name, statements, keepOpen }`. The worker opens the OPFS
// database `name`, runs each statement, and answers the results, or the error
// with its name. It closes the database and every slot handle afterwards unless
// `keepOpen` is set, which is how the test holds a database open while a second
// worker tries it.

import { Engine, OpfsFileSystem } from '../../src/index.mjs';

let held = null;

/**
 * Runs one request against an OPFS database.
 *
 * @param request - `{ name, statements, keepOpen }`
 */
async function handle(request) {
  const fileSystem = await OpfsFileSystem.open(request.name);
  const engine = await Engine.load({ fileSystem, wasm: new URL('../../dist/inillucent.wasm', import.meta.url) });
  const db = engine.open('/test.rdb');
  const conn = engine.connect(db);
  const results = [];
  let files = [];
  try {
    for (const sql of request.statements) {
      const result = engine.execute(conn, sql);
      results.push({ columns: result.columns, rows: result.rows.map((row) => row.map((cell) => (typeof cell === 'bigint' ? String(cell) : cell))), affected: result.affected });
      await fileSystem.refill();
    }
    files = [...fileSystem.byPath.keys()];
  } finally {
    engine.connectionFree(conn);
    if (request.keepOpen) held = { engine, db, fileSystem };
    else {
      engine.close(db);
      fileSystem.close();
    }
  }
  return { results, files };
}

self.onmessage = async (event) => {
  try {
    if (event.data.release && held) {
      held.engine.close(held.db);
      held.fileSystem.close();
      held = null;
      self.postMessage({ ok: true });
      return;
    }
    self.postMessage({ ok: true, ...(await handle(event.data)) });
  } catch (error) {
    self.postMessage({ ok: false, name: error?.name, message: String(error?.message ?? error) });
  }
};

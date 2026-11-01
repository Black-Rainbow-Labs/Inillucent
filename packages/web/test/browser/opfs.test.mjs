// The OPFS file system in real browsers, through Playwright.
//
// What only a browser has: `navigator.storage.getDirectory`, sync access
// handles in a dedicated worker, and their exclusivity. Each browser writes
// rows in one worker, closes it, and reads them back in a new worker after the
// page reloads, so the rows came from OPFS and not from memory. Then one worker
// holds the database open and a second is refused with `DatabaseBusyError`.
//
// It needs the module built (`node packages/web/build.mjs`), Playwright
// (`npm install` in packages/web) and its browsers
// (`npx playwright install chromium firefox webkit`). A browser Playwright
// cannot start is reported as a skip with the reason.

import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, before, test } from 'node:test';

/** This package's folder, which the server serves. */
const here = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');

/** Content types for what the test page loads. */
const TYPES = { '.html': 'text/html', '.mjs': 'text/javascript', '.js': 'text/javascript', '.wasm': 'application/wasm' };

let playwright = null;
try {
  playwright = await import('playwright');
} catch {
  playwright = null;
}

let server;
let origin;

before(async () => {
  server = createServer((request, response) => {
    const path = normalize(join(here, decodeURIComponent(new URL(request.url, 'http://x').pathname)));
    if (!path.startsWith(here) || !existsSync(path)) {
      response.writeHead(404).end();
      return;
    }
    response.writeHead(200, { 'content-type': TYPES[extname(path)] ?? 'application/octet-stream' });
    response.end(readFileSync(path));
  });
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  origin = `http://127.0.0.1:${server.address().port}`;
});

after(() => server?.close());

/** Why the tests cannot run here, or false when they can. */
const missing = !existsSync(join(here, 'dist', 'inillucent.wasm'))
  ? 'no dist/inillucent.wasm (run node packages/web/build.mjs); skipping'
  : playwright === null
    ? 'playwright is not installed (run npm install in packages/web); skipping'
    : false;

/** What the first worker writes. */
const WRITES = [
  'CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT)',
  "INSERT INTO note (body) VALUES ('kept'), ('in'), ('opfs')",
  'CREATE VIRTUAL TABLE doc USING fts5(body)',
  "INSERT INTO doc (body) VALUES ('the browser keeps this'), ('and this too')",
];

/** What the second worker reads, after the reload. */
const READS = ['SELECT body FROM note ORDER BY id', "SELECT body FROM doc WHERE doc MATCH 'browser'"];

for (const browserName of ['chromium', 'firefox', 'webkit']) {
  test(`rows written to OPFS in ${browserName} survive a reload, and a second opener is refused`, { skip: missing }, async (t) => {
    let browser;
    try {
      browser = await playwright[browserName].launch();
    } catch (error) {
      t.skip(`${browserName} would not start: ${String(error.message).split('\n')[0]}`);
      return;
    }
    try {
      const page = await browser.newPage();
      const errors = [];
      page.on('pageerror', (error) => errors.push(error.message));
      await page.goto(`${origin}/test/browser/index.html`);
      await page.waitForFunction(() => window.ready === true);
      const opfs = await page.evaluate(() => new Promise((resolve) => {
        const probe = new Worker(URL.createObjectURL(new Blob(['postMessage(Boolean(navigator.storage && navigator.storage.getDirectory && self.FileSystemFileHandle && FileSystemFileHandle.prototype.createSyncAccessHandle))'])));
        probe.onmessage = (event) => resolve(event.data);
      }));
      if (!opfs) {
        t.skip(`this ${browserName} build has no OPFS sync access handles in a worker`);
        return;
      }
      const name = `test-${Date.now()}`;

      const wrote = await page.evaluate(([n, statements]) => window.ask({ name: n, statements }), [name, WRITES]);
      assert.equal(wrote.ok, true, JSON.stringify(wrote));
      assert.ok(wrote.files.includes('/test.rdb'), `the database file is in a slot: ${JSON.stringify(wrote.files)}`);

      await page.reload();
      await page.waitForFunction(() => window.ready === true);
      const read = await page.evaluate(([n, statements]) => window.ask({ name: n, statements }), [name, READS]);
      assert.equal(read.ok, true, JSON.stringify(read));
      assert.deepEqual(read.results[0].rows, [['kept'], ['in'], ['opfs']]);
      assert.deepEqual(read.results[1].rows, [['the browser keeps this']]);

      const holding = await page.evaluate((n) => window.ask({ name: n, statements: ['SELECT COUNT(*) FROM note'], keepOpen: true }), name);
      assert.equal(holding.ok, true, JSON.stringify(holding));
      const refused = await page.evaluate((n) => window.ask({ name: n, statements: ['SELECT 1'] }), name);
      assert.equal(refused.ok, false, 'a second worker opened a database another worker holds');
      assert.equal(refused.name, 'DatabaseBusyError', JSON.stringify(refused));
      assert.equal((await page.evaluate(() => window.release())).ok, true);
      const again = await page.evaluate((n) => window.ask({ name: n, statements: ['SELECT COUNT(*) FROM note'] }), name);
      assert.equal(again.ok, true, `released, the database opens again: ${JSON.stringify(again)}`);
      assert.deepEqual(again.results[0].rows, [[3]]);
      assert.deepEqual(errors, []);
    } finally {
      await browser.close();
    }
  });
}

# inillucent in a web page

inillucent runs in a browser as WebAssembly. The engine, its SQL, full text search and vector search
are the same code the command line runs, compiled for the `wasm32-wasip1` target. A database written
in a page is an ordinary `.rdb` file, and the command line opens it.

Two npm packages carry it:

| Package | What it is | Where it lives |
|---|---|---|
| `inillucent-web` | the client most pages want: `connect`, `query`, `scalar`, transactions, and `openPersistent` for a database the browser keeps | `web/` in [inillucent-clients](https://github.com/Black-Rainbow-Labs/inillucent-clients) |
| `inillucent-wasm` | the layer under it: the module `inillucent.wasm`, the WASI host it runs on, the two file systems, and the C ABI's calls in JavaScript | `packages/web` in this repository |

Words used on this page:

| Term | Meaning |
|---|---|
| WebAssembly module | a `.wasm` file a browser compiles and runs: here, the engine |
| WASI | a fixed list of functions a module calls to reach files, clocks and randomness. Browsers do not provide them, so `inillucent-wasm` does |
| OPFS | the origin private file system: files a browser keeps for one site, between visits |
| worker | a script a page starts that runs on its own thread |

## A database in memory

```js
import { connect } from 'inillucent-web';

const db = await connect('notes.rdb');
db.execute('CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT)');
db.execute('INSERT INTO note (body) VALUES (?1)', ['hello']);
db.query('SELECT * FROM note');           // [{ id: 1, body: 'hello' }]
const bytes = db.exportDatabase();        // the database as one .rdb file
```

`connect` is awaited once, because the browser compiles the module asynchronously. Every call after
it is synchronous, as in the other clients. The database lives in the page's memory and is gone when
the page is. `exportDatabase()` returns it as the bytes of a `.rdb` file, which a page can offer as a
download, and `connect(name, { bytes })` opens a file the page was given.

This works on a page's main thread, in a worker and in Node 20 or later.

## A database the browser keeps

```js
import { openPersistent } from 'inillucent-web';

const db = await openPersistent('notes');
await db.execute('INSERT INTO note (body) VALUES (?1)', ['still here tomorrow']);
const rows = await db.query('SELECT body FROM note');
await db.transaction(async (tx) => {
  await tx.execute('UPDATE note SET body = upper(body)');
});
await db.close();
```

`openPersistent` starts a worker and runs the engine there, on OPFS. A browser gives synchronous
access to OPFS files only inside a worker, and the engine reads and writes synchronously, so this is
the only arrangement that works. Every call returns a promise. A database is one folder in OPFS,
named after the first argument, and `removePersistent(name)` deletes it.

The engine's log and recovery are the same as on a disk. A tab closed in the middle of a write is
recovered on the next open, the way a process that was killed is recovered.

**One tab at a time.** A browser lets one worker hold an OPFS file. A second `openPersistent` of the
same name, in the same tab or another, is refused with `DatabaseBusyError` until the first is closed.

OPFS with synchronous access exists in Chrome 108, Firefox 111 and Safari 16.4, and later versions.

## What is the same and what is different

| | Native library | WebAssembly |
|---|---|---|
| SQL, FTS5, vector search, encryption | yes | yes, the same code |
| The 35 cases of `drivers/conformance/suite.json` | pass | pass, under Node and in the clients repository's runner |
| Files | the disk | memory, or OPFS in a worker |
| Threads | a parallel HNSW build uses every core | one thread: the same work runs in order |
| `embed()` | in a build with the `embed` feature | not available: the ONNX runtime it needs does not build for WebAssembly |
| Two processes on one file | locked by the operating system | one worker holds a database; others are refused |
| Download size | not applicable | 6.6 MiB, 2.4 MiB compressed with gzip |

## Statements that nest very deeply

A WebAssembly module has two stacks. The Rust stack is in the module's own memory, and the engine
measures it: a statement that nests past it is refused with `too_big`, as on every other platform.
The JavaScript engine's call stack holds the WebAssembly frames, and the module cannot measure it.
A statement with thousands of nested parentheses can run out of that stack first, and the instance
stops in the middle of the call.

The files are outside the instance, so nothing committed is lost. `inillucent-web` keeps a second
instance ready, swaps it in, reopens the database, and reports the statement as `too_big`. A
transaction that was open on that connection is rolled back, as it would be after a crash.

## How it is built

```sh
rustup target add wasm32-wasip1
node packages/web/build.mjs
```

`build.mjs` builds `drivers/inillucent-driver-capi` for `wasm32-wasip1` with the release profile
and copies the module to `packages/web/dist/inillucent.wasm`. The module exports the same C ABI the
other bindings use. JavaScript calls those exports directly, so neither package has a dependency.

The engine needed four changes to build for WebAssembly, all below the SQL:

- `crates/inillucent-vfs/src/os/wasi.rs`, a third platform beside Windows and POSIX. Its locks are
  decided inside the instance, because an instance is one process with one thread, and its shared
  memory is on the heap.
- The stack guard reads the stack's ends from the linker on WebAssembly. Without that it limited
  nothing.
- `std::process::id` and `std::env::temp_dir` panic on WASI, so the engine asks
  `inillucent_base::process::id` and uses `/tmp`.
- The C ABI has `inillucent_buffer_alloc` and `inillucent_buffer_free`, ABI 1.6.0, because
  JavaScript cannot allocate memory inside the module to pass a string in.

## How it is tested

| Test | What it checks |
|---|---|
| `packages/web/test/conformance.test.mjs` | the 35 conformance cases through the WebAssembly build under Node |
| `packages/web/test/engine.test.mjs` | export and import, a reopened file, a held transaction, vector and full text search, the two stacks, and the module's imports |
| `packages/web/test/browser/opfs.test.mjs` | OPFS in Chromium and Firefox through Playwright: rows survive a reload, and a second opener is refused |
| `web/test/` in inillucent-clients | the client's conformance suite and API under Node, and `openPersistent` in Chromium and Firefox, bundled with esbuild |

Playwright's WebKit on Windows has no OPFS sync access handles, so the WebKit run checks the
database in memory and reports the OPFS half as skipped.

`.github/workflows/web.yml` builds the module and runs the three tests in this repository every
night on Linux, and `tests.yml` calls it, so the run checked after a release includes them.

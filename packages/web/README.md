# inillucent-wasm

The [inillucent](https://github.com/Black-Rainbow-Labs/Inillucent) embedded SQL database compiled to
WebAssembly, with the WASI host it needs to run in a browser or in Node. SQL, FTS5 full text search,
`VECTOR(N)` columns with HNSW indexes, and encryption, in one 6.6 MiB module (2.4 MiB compressed).

Most pages want [`inillucent-web`](https://www.npmjs.com/package/inillucent-web), the client built
on this package. It has `connect`, `query`, transactions, and `openPersistent` for a database the
browser keeps. This package is the layer under it, for a page that wants the engine's calls
directly.

```sh
npm install inillucent-wasm
```

## Use

```js
import { Engine } from 'inillucent-wasm';

const engine = await Engine.load();          // compiles dist/inillucent.wasm
const db = engine.open('/notes.rdb');
const conn = engine.connect(db);
engine.execute(conn, 'CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT)');
engine.execute(conn, 'INSERT INTO note (body) VALUES (?1)', ['hello']);
const result = engine.execute(conn, 'SELECT id, body FROM note');
// { columns: ['id', 'body'], rows: [[1, 'hello']], total: 1, more: false, affected: null, ... }
```

`Engine.load({ wasm })` takes the module as a URL, bytes, a fetch `Response` or a compiled
`WebAssembly.Module`. A bundler that moves `dist/inillucent.wasm` needs to be told where it went:
`import wasm from 'inillucent-wasm/inillucent.wasm?url'` in Vite, for example.

Values come back as `null`, a number, a `BigInt` for an integer past 2^53, a string, or a
`Uint8Array`. A failure is an `InillucentError` with the driver's `statusName`, and a construct the
engine has not built is an `UnsupportedError` naming it.

## Files

Every path the engine opens is resolved by the file system the engine was given:

| File system | Where the files are | Where it works |
|---|---|---|
| `MemoryFileSystem` (the default) | JavaScript memory; `exportFile(path)` and `importFile(path, bytes)` move a file in and out | anywhere |
| `OpfsFileSystem.open(name)` | the browser's origin private file system, one folder per name | a dedicated worker only |

`OpfsFileSystem` keeps a pool of OPFS files with their sync access handles open, because a handle
cannot be made while the engine is waiting for a file. Call `await fileSystem.refill()` between
requests to keep the pool full, and `fileSystem.close()` to let another worker open the database. A
second worker opening the same name while the first holds it gets a `DatabaseBusyError`.

## When the instance stops

A statement that nests deeper than the JavaScript engine's call stack allows stops the instance, and
every later call throws `EngineStoppedError` with `reason: 'stack'`. The files are outside the
instance, so `Engine.load({ wasm: engine.module, fileSystem: engine.fs })` makes a new engine that
opens the database and recovers it. `inillucent-web` does this itself.

## Build and test

From a checkout of the repository:

```sh
rustup target add wasm32-wasip1
node packages/web/build.mjs
node --test packages/web/test/conformance.test.mjs packages/web/test/engine.test.mjs
npm --prefix packages/web install
node --test packages/web/test/browser/opfs.test.mjs
```

[docs/web.md](../../docs/web.md) explains the design and what differs from the native library.

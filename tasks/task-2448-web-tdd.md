# inillucent in the browser: technical design

## 1. What this adds

inillucent runs as a program or a linked library on Windows, Linux, macOS, Android and iOS. A web
page cannot use it, because nothing builds the engine for a browser and nothing in JavaScript can
reach it there. This design adds:

| Piece | Where | What it is |
|---|---|---|
| The engine as WebAssembly | `drivers/inillucent-driver-capi`, built for `wasm32-wasip1` | the same C ABI every other binding uses, compiled to one `inillucent.wasm` |
| `inillucent-wasm` | `packages/web` in this repository, published to npm | the module, a WASI host written in JavaScript, and a small loader with the C ABI's calls |
| `inillucent-web` | `web/` in the inillucent-clients repository, published to npm | the client API the other eight clients have (`connect`, `query`, `scalar`, transactions), for a browser and for Node |

Both packages are graded by the driver's conformance suite, `drivers/conformance/suite.json`, the
35 cases that grade the Rust driver, the Python binding and the two phone bindings.

## 2. Terms

| Term | Meaning |
|---|---|
| WebAssembly module | a `.wasm` file: compiled code plus a list of the functions it imports from its host and exports to it |
| WASI preview 1 | a fixed list of about 45 functions (`fd_read`, `path_open`, `clock_time_get` and so on) a module imports to reach files, clocks and randomness. Rust's `wasm32-wasip1` target compiles the standard library against it |
| WASI host | the code that provides those functions. In Node it could be `node:wasi`; in a browser nothing provides them, so `packages/web` does |
| linear memory | the module's memory, which JavaScript sees as one `ArrayBuffer` |
| OPFS | the origin private file system: files a browser keeps per site, outside the page's reach to anyone else |
| sync access handle | `FileSystemSyncAccessHandle`, the OPFS handle with synchronous `read`, `write`, `truncate`, `getSize` and `flush`. Only a dedicated worker can make one |
| slot | one OPFS file in the pool `packages/web` keeps, which holds one of the engine's files (section 6.3) |

## 3. Research

1. **Which WebAssembly target.** Rust offers `wasm32-unknown-unknown`, with no system interface at
   all, and `wasm32-wasip1`, with WASI preview 1. On `wasm32-unknown-unknown`,
   `std::time::Instant::now`, `SystemTime::now` and the file system panic, and the `getrandom` crate
   the engine reaches through `rand` needs its `js` feature, which pulls in `wasm-bindgen`. The
   dependency policy allows neither the panics nor the new crates. On `wasm32-wasip1` every one of
   those works through a WASI import, the standard library's `std::fs` works unchanged, and the
   engine needs no new crate. The cost is that the host must implement the WASI imports the module
   uses, which for this module is 26 functions (section 6.1).
2. **Threads.** `wasm32-wasip1` has no threads: `std::thread::Builder::spawn` returns an error. The
   engine's production code spawns two threads, the lease reaper (`inillucent-pool/src/lease.rs`),
   which already treats a failed spawn as "no reaper", and the embedder reaper, which is only built
   with the `onnx` feature. Rayon's global pool falls back to running on the calling thread when
   the target cannot start threads, so `rayon::join` and the parallel HNSW build run serially. The
   threaded target, `wasm32-wasip1-threads`, needs `SharedArrayBuffer`, which needs the page served
   with cross origin isolation headers. That is a requirement most sites cannot meet, so it is not
   used.
3. **Persistence in a browser.** SQLite's own WebAssembly build has two OPFS file systems. `opfs`
   needs `SharedArrayBuffer` and cross origin isolation; `opfs-sahpool`, added in SQLite 3.43,
   keeps a pool of OPFS files, opens a sync access handle on every one of them when the database
   opens, and maps the database's files onto pool files. It needs no special headers. That is the
   design here, for the same reason: `createSyncAccessHandle` is asynchronous, so a file cannot be
   created while the engine is in the middle of a synchronous `path_open`, and a pool of files
   opened in advance is what lets it. Sync access handles exist from Chrome 108, Firefox 111 and
   Safari 16.4, and only in a dedicated worker.
4. **One tab at a time.** The default sync access handle mode allows one handle on a file in the
   whole browser. Chrome 121 added `readwrite-unsafe`, which allows several, and leaves the locking
   to the page. The engine's locks are decided inside one WebAssembly instance (section 5.2), and a
   second tab is a second instance that cannot see them, so the default mode is kept: a second tab
   that opens the same database is refused by the browser, and `packages/web` names the cause in its
   error instead of passing on `NoModificationAllowedError`: `DatabaseBusyError`, saying the
   database is open in another tab or worker.
5. **Size.** The release build of the module is 6.6 MiB, 2.4 MiB compressed with gzip, which is what
   a web server sends. That is the whole engine, the full text search and the vector index
   included. It is not split, because a database that refuses `MATCH` until a second download
   arrives is worse than a larger first load.
6. **The JavaScript tooling other projects use.** `wasm-bindgen` and `wasm-pack` generate a binding
   from Rust attributes. They are not needed here, because the C ABI is already the binding: every
   argument is an integer, a float or a pointer into linear memory, which is exactly what a
   WebAssembly export takes. The loader in `packages/web` calls the exports directly, and the
   package has no dependencies.

Sources: [SQLite persistence on the web](https://www.powersync.com/blog/sqlite-persistence-on-the-web),
[SQLite in the browser with OPFS](https://blog.openreplay.com/sqlite-browser-opfs/),
[MDN: createSyncAccessHandle](https://developer.mozilla.org/en-US/docs/Web/API/FileSystemFileHandle/createSyncAccessHandle),
[Chrome: multiple readers and writers](https://developer.chrome.com/blog/new-dev-trial-for-multiple-readers-and-writers),
[rayon issue 861](https://github.com/rayon-rs/rayon/issues/861),
[rayon-core fallback](https://docs.rs/rayon-core-wasm/latest/src/rayon_core_wasm/lib.rs.html).

## 4. What does not change

The engine's SQL, storage format and log are the same code on every platform. A database written in
a browser is a `.rdb` file the command line opens, and the reverse. Nothing in this design adds a
code path that only WebAssembly takes above the VFS, apart from the stack bound in 5.3.

## 5. The engine changes

### 5.1 A WASI platform for the operating system VFS

`crates/inillucent-vfs/src/os/mod.rs` picks a platform module with `#[cfg(unix)]` or
`#[cfg(windows)]`. `wasm32-wasip1` is neither, so the build failed with `unresolved import
crate::os::platform`. `os/wasi.rs` is the third platform module. It provides the same functions:

| Function | On WASI |
|---|---|
| `read_at`, `read_at_spare`, `write_at` | `fd_pread` and `fd_pwrite`, imported directly. The standard library has them only behind the unstable `wasi_ext` feature |
| `file_len`, `file_identity`, `file_number_by_name` | `fd_filestat_get`: the file's size, device and inode number |
| `path_state`, `names_starting_with` | `std::fs::metadata` and `std::fs::read_dir`, which work on WASI |
| `system_randomness` | `random_get` |
| `sync_directory` | nothing: preview 1 cannot open a directory to flush it |
| `lock_state`, `LockState` | the in-process `LockTable` alone, keyed by device and inode |
| `open_shm` | the in-memory VFS's `MemoryShm`, one per shared-memory path in a process wide map |

`os/filelock.rs`, the shared memory file built on kernel byte range locks, is not compiled for WASI.

### 5.2 Why in-process locks are correct

A WebAssembly instance is one process with one thread. Two connections in one instance are two
handles on one file, and the `LockTable` arbitrates them exactly as it does between two connections
in one process on Linux. There is no second process inside the instance to exclude. A second
instance (another tab, another worker) is excluded by the browser itself, because it cannot get a
sync access handle on a file the first instance holds (3.4). The memory file system has no second
instance at all: its files are inside one instance's JavaScript objects.

### 5.3 The stack

A WebAssembly module's Rust stack is in linear memory. `inillucent-base/src/stack.rs` refuses a
statement that nests past the stack with `too_big`, and on a platform it has no call for it limits
nothing. On WebAssembly that turned a deeply nested statement into an out of bounds trap that
stopped the instance. `wasm-ld` names the stack's ends `__stack_low` and `__stack_high`, so the WASI
build reads those. The stack is placed first in memory, so its low end is address 0, and `room()`
reads a low end of 1 as "not known"; the low end is raised to 1024. The first version raised it to
1, which switched the guard off, and 300 nested subqueries trapped with a 1 MiB stack.

There is a second stack the module cannot see. The WebAssembly frames themselves are on the
JavaScript engine's call stack, about 1 MB in V8. Measured under Node 24:

| Rust stack | 1,000 nested subqueries | 5,000 nested parentheses |
|---|---|---|
| 8 MiB | V8's stack ran out first: the instance stopped | V8's stack ran out first |
| 1 MiB | refused with `too_big`, instance unharmed | V8's stack ran out first |

So the module links with a 1 MiB Rust stack, which is also a Windows main thread's, and the guard
trips first for nested subqueries. For the cases where V8's stack still runs out first, the loader
reports `EngineStoppedError` with `reason: 'stack'` and refuses every later call on that instance,
because the WebAssembly call was abandoned partway and its memory cannot be trusted. The files are
outside the instance, so the client recovers (section 7).

### 5.4 Two smaller fixes

- `inillucent-core/src/vectors.rs` reads the vector file positionally with a `cfg(unix)` and a
  `cfg(windows)` branch; WASI gets a seek and a read, which is safe with one thread.
- `inillucent-driver-capi/src/capi/node.rs`, the Node add-on, finds N-API in the host process with
  `dlsym` or `GetProcAddress`. It is compiled only on `unix` and `windows`.

### 5.5 Two new C ABI functions, ABI 1.6.0

JavaScript cannot allocate inside the module's memory, so it has nowhere to put a SQL string. The
C ABI gains `inillucent_buffer_alloc(len)` and `inillucent_buffer_free(buffer, len)`, provisional,
exported on every platform so the header, `drivers/abi.toml` and the library agree and
`tests/abi.rs` checks them on the development machine. The ABI version becomes 1.6.0. Nothing else
in the ABI moves.

## 6. `packages/web`: the npm package `inillucent-wasm`

```
packages/web/
  package.json            name inillucent-wasm, no dependencies, type module
  build.mjs               cargo build for wasm32-wasip1, copy to dist/inillucent.wasm
  src/wasi.mjs            the WASI preview 1 host, over a FileSystem
  src/memory-fs.mjs       files in JavaScript memory
  src/opfs-fs.mjs         files in OPFS slots, for a dedicated worker
  src/engine.mjs          loads the module and wraps the C ABI's calls
  src/index.mjs           what the package exports
  test/conformance.test.mjs   the shared suite under Node
  test/engine.test.mjs        memory files, export and import, the file system
  test/browser/           the OPFS test page and its Playwright driver
```

### 6.1 The WASI host

The module imports 26 WASI functions. `wasi.mjs` implements all 26, and refuses any other import
when the module is instantiated, with a message naming it, so a future engine change that needs a
new import fails at load and not at the call. File descriptors 0, 1 and 2 are the console; 3 is the
one preopened directory, `/`. Every other descriptor is a file the engine opened. Paths are resolved
inside the file system object, which knows nothing about the host's disk: in Node too, the module
sees only the files the host gave it.

`clock_time_get` answers `Date.now()` for the real time clock and `performance.now()` for the
monotonic one. `poll_oneoff` with one clock subscription is a sleep; the engine sleeps while it
waits on a busy database, and since nothing else in the instance can release the lock, the wait is
answered at once and the busy timeout runs out on the next check.

### 6.2 The memory file system

A map from path to a growable `Uint8Array`, with an inode number per file. It is what `connect()`
uses by default, on the main thread, in a worker, or in Node. `exportDatabase()` asks the engine to
`inillucent_backup_to` a path in the same file system and hands back those bytes: one
self-contained `.rdb`. `importDatabase(bytes)` writes a file before the engine opens it.

### 6.3 The OPFS file system

For a dedicated worker. At open, `OpfsFileSystem.open(name)` finds or makes the directory
`inillucent/<name>/` in OPFS, makes sure it holds at least a minimum number of slot files, and opens
a sync access handle on every slot. Each slot file starts with a 4096 byte header: a magic number,
the logical path it holds (empty when free), and the file's length. The engine's bytes start at 4096.
`path_open` with create on a path no slot holds takes a free slot and writes its header. Unlinking
clears the header and truncates the slot. Renaming rewrites the header.

Because a new slot cannot be made synchronously, the worker refills the pool between requests: after
each message it handles it counts the free slots and, if there are fewer than the minimum, creates
more and opens their handles before it reads the next message. A single statement that creates more
files than the pool holds fails with a disk full error that names the pool. The minimum is eight,
and a statement creates at most a log segment and a temporary file.

`flush()` on the handle is `fd_sync` and `fd_datasync`. The engine's log and checkpoint code is the
same as on a disk, so a tab closed in the middle of a write recovers on the next open the way a
process killed in the middle of one does.

### 6.4 The loader

`Engine.load({ wasm, fileSystem })` takes the module as a URL, bytes, or a compiled
`WebAssembly.Module`, defaulting to `dist/inillucent.wasm` beside the package. It checks
`inillucent_abi_version()`'s major against 1 before anything else, like every other binding. It
exposes the C ABI calls with JavaScript arguments: strings are encoded with `TextEncoder` into a
buffer from `inillucent_buffer_alloc`, out parameters are 8 byte cells in a scratch buffer, and a
returned error is read into a plain object and freed.

Values cross with the cell accessors, which cost nanoseconds a call inside one instance. The JSON
form, `inillucent_rows_json`, was built for Python, where a foreign call costs half a microsecond;
here it would lose integers past 2^53 to `JSON.parse`. Integers come back as numbers when they fit
exactly and as `BigInt` past that, text as strings, blobs as `Uint8Array`, NULL as `null`.

## 7. The client: `inillucent-web` in inillucent-clients

`web/` beside `typescript/`. The same API as `inillucent-client`, so a page and a Node program read
alike:

```ts
import { connect } from 'inillucent-web';

const db = await connect();                     // a memory database
db.execute('CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT)');
db.execute('INSERT INTO note (body) VALUES (?1)', ['hello']);
db.query('SELECT * FROM note');                 // [{ id: 1, body: 'hello' }]
const bytes = db.exportDatabase();              // a .rdb file
```

`connect()` is asynchronous once, because compiling the module is; every call after it is
synchronous, like the other clients. For a database kept between visits:

```ts
import { openPersistent } from 'inillucent-web';

const db = await openPersistent('notes');       // OPFS, in a worker the package starts
await db.execute('INSERT INTO note (body) VALUES (?1)', ['kept']);
await db.query('SELECT * FROM note');
```

`openPersistent` runs the engine in a dedicated worker, because that is the only place OPFS has
synchronous handles, and every call returns a promise. The worker script ships in the package.

Errors are `InillucentError` with the driver's status name, and `UnsupportedError` for exit code 3's
`unsupported`, as in every other client. `capabilities()` and `supports(name)` read the table.

**A stopped instance is replaced.** `load()` makes two instances on one `MemoryFileSystem`: the one
in use and a spare, because making an instance is asynchronous and the API is not. When a call
stops the instance, the client swaps in the spare, moves a generation number on, and throws
`too_big` (or `internal` for any other stop). Each `Database` and `Connection` remembers the
generation it was opened in and reopens itself on its next call; a `Statement` or `Transaction`
from before cannot, and reports `invalid_state`. A new spare is made in the background. The worker
behind `openPersistent` does the same with its own instance and the OPFS files.

## 8. Tests

| Test | What it proves |
|---|---|
| `packages/web/test/conformance.test.mjs` | all 35 cases of the suite through the WASI host and the memory file system, under Node; writes `_agent_output/conformance/web.json` |
| `packages/web/test/engine.test.mjs` | export then import gives the same rows; a file reopened in the same file system keeps its rows; a deep statement is refused with `too_big`, not a trap; a vector index and full text search answer |
| `packages/web/test/browser/opfs.spec.mjs` | in Chromium through Playwright: a worker writes rows to OPFS, the page reloads, a new worker reads them back; a second worker on the same name is refused with the named error |
| `inillucent-clients/web/test/*.test.js` | the client's own API under Node, and the clients repository's shared conformance suite |

`drivers/conformance/suite.json` gains a `web` entry under `skipped_by` with nothing lacking.
`packages/package-tests.toml` names the package's test files, so `bindings::every_package_test_is_named_here`
keeps passing.

## 9. Release

`packaging/ship.ps1` gains a `web` route that builds the module, runs the package's Node tests, and
publishes `inillucent-wasm` with the release's version, and its version file joins
`Get-VersionCarriers`. The client in inillucent-clients is published from that repository with the
same npm credential.

## 10. Follow up

A ticket for inillucent.com: a section about the web package, and a page with a database explorer
running this package against a database already filled in, so a visitor can try the SQL, full text
and vector search without installing anything.

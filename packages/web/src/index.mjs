// inillucent-wasm: the inillucent engine as a WebAssembly module, with the WASI
// host and file systems it runs on in a browser or in Node.
//
// Most applications want `inillucent-web`, the client built on this package,
// which has `connect`, `query`, transactions and a worker that keeps the
// database in the browser's origin private file system. This package is the
// layer under it: `Engine` calls the C ABI, `MemoryFileSystem` and
// `OpfsFileSystem` hold the engine's files, and `WasiHost` is what the module
// imports.

export {
  ABI_MAJOR,
  Engine,
  EngineLoadError,
  EngineStoppedError,
  InillucentError,
  OpenFlags,
  Support,
  UnsupportedError,
  ValueKind,
  fromInteger,
  statusName,
} from './engine.mjs';
export { MemoryFileSystem } from './memory-fs.mjs';
export { OpfsFileSystem } from './opfs-fs.mjs';
export { Errno, WasiError, WasiExit, WasiHost, missingImports, normalisePath } from './wasi.mjs';

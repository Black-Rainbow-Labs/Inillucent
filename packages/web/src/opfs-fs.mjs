// Files kept in the browser's origin private file system (OPFS), for a dedicated worker.
//
// The engine opens, reads and writes files synchronously, from inside a WASI
// call. OPFS has synchronous reads and writes only through a sync access
// handle, only in a dedicated worker, and making one is asynchronous. So a file
// cannot be created at the moment the engine asks for it. This keeps a pool of
// slot files instead, the design SQLite's `opfs-sahpool` uses: every slot has
// its handle opened when the database is opened, each slot's first 4096 bytes
// say which of the engine's paths it holds, and creating a file takes a free
// slot. `refill()` makes new slots between requests, when waiting is allowed.
//
// Paths under `/tmp` stay in memory. They are the engine's temporary files,
// which nothing reads after the statement that made them, and they would
// otherwise take slots.
//
// A sync access handle in its default mode is exclusive across the whole
// browser, so a second tab or worker opening the same database is refused by
// the browser. `open` turns that refusal into an error that names the cause.

import { MemoryFileSystem, parentOf } from './memory-fs.mjs';
import { Errno, WasiError } from './wasi.mjs';

/** Bytes at the start of every slot that describe it. The engine's bytes follow. */
export const HEADER_BYTES = 4096;

/** What a slot's header starts with. */
const MAGIC = 'inillucent-slot1';

/** The fewest free slots kept between requests. A statement makes at most a log segment or two. */
const MINIMUM_FREE = 8;

/** The folder in OPFS every database's slots go under. */
const ROOT_FOLDER = 'inillucent';

/**
 * One slot: an OPFS file, its open handle, and the engine path it holds.
 */
class Slot {
  /**
   * @param name - the slot file's name
   * @param handle - its open sync access handle
   * @param ino - the file number the engine sees for it
   */
  constructor(name, handle, ino) {
    this.name = name;
    this.handle = handle;
    this.ino = ino;
    this.path = readHeader(handle);
  }

  /**
   * Records which path this slot holds, or that it is free.
   *
   * @param path - an absolute path, or '' for free
   */
  assign(path) {
    writeHeader(this.handle, path);
    this.path = path;
  }

  /**
   * Copies bytes out of the engine's part of the file.
   *
   * @param target - where they go
   * @param offset - where in the engine's file to start
   */
  read(target, offset) {
    return this.handle.read(target, { at: HEADER_BYTES + offset });
  }

  /**
   * Copies bytes into the engine's part of the file.
   *
   * @param source - the bytes
   * @param offset - where in the engine's file they go
   */
  write(source, offset) {
    return this.handle.write(source, { at: HEADER_BYTES + offset });
  }

  /** Returns the length of the engine's file. */
  size() {
    return Math.max(0, this.handle.getSize() - HEADER_BYTES);
  }

  /**
   * Sets the length of the engine's file.
   *
   * @param size - the new length
   */
  truncate(size) {
    this.handle.truncate(HEADER_BYTES + size);
  }

  /** Flushes the file to the browser's storage. */
  sync() {
    this.handle.flush();
  }
}

/**
 * Reads the engine path a slot's header names, or '' for a free slot or one never written.
 *
 * @param handle - the slot's sync access handle
 */
function readHeader(handle) {
  if (handle.getSize() < HEADER_BYTES) return '';
  const header = new Uint8Array(HEADER_BYTES);
  handle.read(header, { at: 0 });
  if (new TextDecoder().decode(header.subarray(0, MAGIC.length)) !== MAGIC) return '';
  const length = new DataView(header.buffer).getUint32(16, true);
  if (length === 0 || length > HEADER_BYTES - 24) return '';
  return new TextDecoder().decode(header.subarray(24, 24 + length));
}

/**
 * Writes a slot's header, naming the path it holds.
 *
 * @param handle - the slot's sync access handle
 * @param path - the path, or '' for free
 */
function writeHeader(handle, path) {
  const name = new TextEncoder().encode(path);
  if (name.length > HEADER_BYTES - 24) throw new WasiError(Errno.INVAL, `a path of ${name.length} bytes does not fit a slot header`);
  const header = new Uint8Array(HEADER_BYTES);
  header.set(new TextEncoder().encode(MAGIC), 0);
  new DataView(header.buffer).setUint32(16, name.length, true);
  header.set(name, 24);
  handle.write(header, { at: 0 });
  if (handle.getSize() < HEADER_BYTES) handle.truncate(HEADER_BYTES);
  handle.flush();
}

/**
 * Turns the browser's refusal of a second handle into an error that says what it means.
 *
 * @param name - the database's name
 * @param error - what `createSyncAccessHandle` threw
 */
function explain(name, error) {
  if (error?.name === 'NoModificationAllowedError' || error?.name === 'InvalidStateError') {
    const busy = new Error(`the database "${name}" is open in another tab or worker; close it there first`);
    busy.name = 'DatabaseBusyError';
    busy.cause = error;
    return busy;
  }
  return error;
}

/**
 * A file system whose files live in OPFS slots. Make one with `OpfsFileSystem.open`.
 */
export class OpfsFileSystem {
  /**
   * Opens, or creates, the slot folder for one database name and opens every slot's handle.
   *
   * @param name - the database's name: a folder under `inillucent/` in OPFS
   * @param options - `minimumFree`, the free slots to keep (default 8)
   */
  static async open(name, options = {}) {
    if (typeof navigator === 'undefined' || !navigator.storage?.getDirectory) {
      throw new Error('this environment has no origin private file system (navigator.storage.getDirectory)');
    }
    const top = await (await navigator.storage.getDirectory()).getDirectoryHandle(ROOT_FOLDER, { create: true });
    const folder = await top.getDirectoryHandle(name, { create: true });
    const system = new OpfsFileSystem(name, folder, options.minimumFree ?? MINIMUM_FREE);
    const names = [];
    for await (const [entry, handle] of folder.entries()) if (handle.kind === 'file' && entry.startsWith('slot-')) names.push(entry);
    names.sort();
    try {
      for (const entry of names) await system.addSlot(entry);
      await system.refill();
    } catch (error) {
      system.close();
      throw explain(name, error);
    }
    return system;
  }

  /**
   * Deletes every slot of a database name. The database must not be open.
   *
   * @param name - the database's name
   */
  static async remove(name) {
    const top = await (await navigator.storage.getDirectory()).getDirectoryHandle(ROOT_FOLDER, { create: true });
    try {
      await top.removeEntry(name, { recursive: true });
    } catch (error) {
      if (error?.name !== 'NotFoundError') throw explain(name, error);
    }
  }

  /**
   * @param name - the database's name
   * @param folder - its OPFS folder
   * @param minimumFree - the free slots `refill` keeps
   */
  constructor(name, folder, minimumFree) {
    this.name = name;
    this.folder = folder;
    this.minimumFree = minimumFree;
    this.slots = [];
    this.byPath = new Map();
    this.directories = new Set(['/']);
    this.scratch = new MemoryFileSystem();
    this.nextNumber = 0;
  }

  /**
   * Opens one slot file's handle and adds it to the pool.
   *
   * @param entry - the slot file's name
   */
  async addSlot(entry) {
    const file = await this.folder.getFileHandle(entry, { create: true });
    const handle = await file.createSyncAccessHandle();
    const number = Number(entry.slice('slot-'.length));
    this.nextNumber = Math.max(this.nextNumber, number + 1);
    const slot = new Slot(entry, handle, 1_000 + number);
    this.slots.push(slot);
    if (slot.path) this.byPath.set(slot.path, slot);
    for (let parent = parentOf(slot.path); slot.path && parent !== '/'; parent = parentOf(parent)) this.directories.add(parent);
  }

  /** Returns how many slots hold no file. */
  freeCount() {
    return this.slots.filter((slot) => !slot.path).length;
  }

  /** Makes new slots until at least `minimumFree` are free. Call it between requests. */
  async refill() {
    while (this.freeCount() < this.minimumFree) {
      await this.addSlot(`slot-${String(this.nextNumber).padStart(5, '0')}`);
    }
  }

  /** Closes every handle, so another worker may open this database. */
  close() {
    for (const slot of this.slots) {
      try {
        slot.handle.close();
      } catch {
        // A handle that is already closed has nothing left to release.
      }
    }
    this.slots = [];
    this.byPath.clear();
  }

  /**
   * Reports whether a path belongs to the in-memory scratch area.
   *
   * @param path - an absolute path
   */
  isScratch(path) {
    return path === '/tmp' || path.startsWith('/tmp/');
  }

  /**
   * Describes a path, or answers null when nothing is there.
   *
   * @param path - an absolute path
   */
  stat(path) {
    if (this.isScratch(path)) return this.scratch.stat(path);
    const slot = this.byPath.get(path);
    if (slot) return { kind: 'file', size: slot.size(), ino: slot.ino };
    if (this.directories.has(path)) return { kind: 'dir', size: 0, ino: 1 };
    return null;
  }

  /**
   * Opens a file, taking a free slot to create it.
   *
   * @param path - an absolute path
   * @param options - `create`
   */
  open(path, options = {}) {
    if (this.isScratch(path)) return this.scratch.open(path, options);
    const existing = this.byPath.get(path);
    if (existing) return existing;
    if (!options.create) throw new WasiError(Errno.NOENT, `no file ${path}`);
    if (!this.directories.has(parentOf(path))) throw new WasiError(Errno.NOENT, `no directory for ${path}`);
    const slot = this.slots.find((candidate) => !candidate.path);
    if (!slot) throw new WasiError(Errno.NOSPC, `every OPFS slot of "${this.name}" is in use; the pool is refilled between requests`);
    slot.truncate(0);
    slot.assign(path);
    this.byPath.set(path, slot);
    return slot;
  }

  /**
   * Removes a file and frees its slot.
   *
   * @param path - an absolute path
   */
  unlink(path) {
    if (this.isScratch(path)) return this.scratch.unlink(path);
    const slot = this.byPath.get(path);
    if (!slot) throw new WasiError(Errno.NOENT, `no file ${path}`);
    this.byPath.delete(path);
    slot.assign('');
    slot.truncate(0);
  }

  /**
   * Moves a file by rewriting its slot's header, replacing whatever was at the destination.
   *
   * @param from - the current path
   * @param to - the new path
   */
  rename(from, to) {
    if (this.isScratch(from) && this.isScratch(to)) return this.scratch.rename(from, to);
    if (this.isScratch(from) || this.isScratch(to)) throw new WasiError(Errno.NOTSUP, 'a file cannot move between /tmp and the database folder');
    const slot = this.byPath.get(from);
    if (!slot) throw new WasiError(Errno.NOENT, `no file ${from}`);
    if (this.byPath.has(to)) this.unlink(to);
    this.byPath.delete(from);
    slot.assign(to);
    this.byPath.set(to, slot);
  }

  /**
   * Lists the names directly inside a directory.
   *
   * @param path - an absolute directory path
   */
  list(path) {
    if (this.isScratch(path)) return this.scratch.list(path);
    const prefix = path === '/' ? '/' : `${path}/`;
    const names = new Set(path === '/' ? ['tmp'] : []);
    for (const name of [...this.byPath.keys(), ...this.directories]) {
      if (name !== path && name.startsWith(prefix)) names.add(name.slice(prefix.length).split('/')[0]);
    }
    return [...names];
  }

  /**
   * Makes a directory. Directories are not stored: one exists while a file is inside it.
   *
   * @param path - an absolute path
   */
  mkdir(path) {
    if (this.isScratch(path)) return this.scratch.mkdir(path);
    if (this.stat(path)) throw new WasiError(Errno.EXIST);
    this.directories.add(path);
  }

  /**
   * Removes an empty directory.
   *
   * @param path - an absolute path
   */
  rmdir(path) {
    if (this.isScratch(path)) return this.scratch.rmdir(path);
    if (!this.directories.has(path)) throw new WasiError(Errno.NOENT);
    if (this.list(path).length > 0) throw new WasiError(Errno.NOTEMPTY);
    this.directories.delete(path);
  }

  /**
   * Returns a copy of a file's bytes, or null when there is no such file.
   *
   * @param path - an absolute path
   */
  exportFile(path) {
    if (this.isScratch(path)) return this.scratch.exportFile(path);
    const slot = this.byPath.get(path);
    if (!slot) return null;
    const bytes = new Uint8Array(slot.size());
    slot.read(bytes, 0);
    return bytes;
  }

  /**
   * Writes a whole file, replacing any file already at the path.
   *
   * @param path - an absolute path
   * @param bytes - the contents
   */
  importFile(path, bytes) {
    if (this.isScratch(path)) return this.scratch.importFile(path, bytes);
    const slot = this.open(path, { create: true });
    slot.truncate(0);
    slot.write(new Uint8Array(bytes), 0);
    slot.sync();
  }

  /**
   * Removes a file and every file whose name starts with its name: the database, its log
   * segments and its shared memory file.
   *
   * @param path - the database's path
   */
  removeDatabase(path) {
    for (const name of [...this.byPath.keys()]) if (name.startsWith(path)) this.unlink(name);
  }
}

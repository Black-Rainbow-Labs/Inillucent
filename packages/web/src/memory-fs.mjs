// Files kept in JavaScript memory, for the WASI host.
//
// This is the file system `connect()` uses unless it is asked for another. It
// works on a page's main thread, in any worker and in Node, and everything in
// it is gone when the page is. `exportFile` and `importFile` are how a
// database leaves and enters it as one `.rdb` file.
//
// Every file system the host is given has the same six calls: `stat`, `open`,
// `unlink`, `rename`, `list`, `mkdir` and `rmdir`, and an open file has
// `read`, `write`, `size`, `truncate`, `sync` and `ino`. `opfs-fs.mjs` is the
// other one.

import { Errno, WasiError, normalisePath } from './wasi.mjs';

/**
 * One file's bytes, kept in a buffer that grows by doubling so a log that is
 * appended to a page at a time is not copied on every write.
 */
class MemoryFile {
  /** @param ino - the file's number */
  constructor(ino) {
    this.ino = ino;
    this.data = new Uint8Array(0);
    this.length = 0;
    this.mtime = Date.now();
  }

  /**
   * Makes room for at least `size` bytes.
   *
   * @param size - the length needed
   */
  reserve(size) {
    if (size <= this.data.length) return;
    const grown = new Uint8Array(Math.max(size, this.data.length * 2, 4096));
    grown.set(this.data.subarray(0, this.length));
    this.data = grown;
  }

  /**
   * Copies bytes out, and answers how many there were.
   *
   * @param target - where they go
   * @param offset - where in the file to start
   */
  read(target, offset) {
    if (offset >= this.length) return 0;
    const count = Math.min(target.length, this.length - offset);
    target.set(this.data.subarray(offset, offset + count));
    return count;
  }

  /**
   * Copies bytes in, growing the file when they reach past its end.
   *
   * @param source - the bytes
   * @param offset - where in the file they go
   */
  write(source, offset) {
    const end = offset + source.length;
    this.reserve(end);
    this.data.set(source, offset);
    if (end > this.length) this.length = end;
    this.mtime = Date.now();
    return source.length;
  }

  /** Returns the length in bytes. */
  size() {
    return this.length;
  }

  /**
   * Sets the length, filling with zeros when it grows.
   *
   * @param size - the new length
   */
  truncate(size) {
    if (size > this.length) {
      this.reserve(size);
      this.data.fill(0, this.length, size);
    }
    this.length = size;
    this.mtime = Date.now();
  }

  /** Does nothing: memory has nothing to flush to. */
  sync() {}

  /** Returns a copy of the file's bytes. */
  contents() {
    return this.data.slice(0, this.length);
  }
}

/**
 * A file system in memory: a map from absolute path to file, and a set of directories.
 */
export class MemoryFileSystem {
  constructor() {
    this.files = new Map();
    this.directories = new Set(['/', '/tmp']);
    this.nextIno = 10;
  }

  /**
   * Describes a path, or answers null when nothing is there.
   *
   * @param path - an absolute path
   */
  stat(path) {
    const file = this.files.get(path);
    if (file) return { kind: 'file', size: file.length, ino: file.ino, mtime: file.mtime };
    if (this.directories.has(path)) return { kind: 'dir', size: 0, ino: 1 };
    return null;
  }

  /**
   * Opens a file, creating it when asked.
   *
   * @param path - an absolute path
   * @param options - `create`
   */
  open(path, options = {}) {
    let file = this.files.get(path);
    if (!file) {
      if (!options.create) throw new WasiError(Errno.NOENT, `no file ${path}`);
      if (!this.directories.has(parentOf(path))) throw new WasiError(Errno.NOENT, `no directory for ${path}`);
      file = new MemoryFile(this.nextIno);
      this.nextIno += 1;
      this.files.set(path, file);
    }
    return file;
  }

  /**
   * Removes a file. An open handle keeps its bytes, as on a POSIX system.
   *
   * @param path - an absolute path
   */
  unlink(path) {
    if (!this.files.delete(path)) throw new WasiError(Errno.NOENT, `no file ${path}`);
  }

  /**
   * Moves a file, replacing whatever was at the destination.
   *
   * @param from - the current path
   * @param to - the new path
   */
  rename(from, to) {
    const file = this.files.get(from);
    if (!file) throw new WasiError(Errno.NOENT, `no file ${from}`);
    this.files.delete(from);
    this.files.set(to, file);
  }

  /**
   * Lists the names directly inside a directory.
   *
   * @param path - an absolute directory path
   */
  list(path) {
    const prefix = path === '/' ? '/' : `${path}/`;
    const names = new Set();
    for (const name of [...this.files.keys(), ...this.directories]) {
      if (name !== path && name.startsWith(prefix)) names.add(name.slice(prefix.length).split('/')[0]);
    }
    return [...names];
  }

  /**
   * Makes a directory.
   *
   * @param path - an absolute path
   */
  mkdir(path) {
    if (this.stat(path)) throw new WasiError(Errno.EXIST);
    this.directories.add(path);
  }

  /**
   * Removes an empty directory.
   *
   * @param path - an absolute path
   */
  rmdir(path) {
    if (!this.directories.has(path)) throw new WasiError(Errno.NOENT);
    if (this.list(path).length > 0) throw new WasiError(Errno.NOTEMPTY);
    this.directories.delete(path);
  }

  /**
   * Returns a copy of a file's bytes, or null when there is no such file.
   *
   * @param path - the path, absolute or relative to `/`
   */
  exportFile(path) {
    return this.files.get(normalisePath(path))?.contents() ?? null;
  }

  /**
   * Writes a whole file, replacing any file already at the path.
   *
   * @param path - the path, absolute or relative to `/`
   * @param bytes - the contents
   */
  importFile(path, bytes) {
    const resolved = normalisePath(path);
    this.files.delete(resolved);
    this.open(resolved, { create: true }).write(new Uint8Array(bytes), 0);
  }

  /**
   * Removes a file and every file whose name starts with its name, which is
   * how the engine names a database's log segments and shared memory file.
   *
   * @param path - the database's path
   */
  removeDatabase(path) {
    const resolved = normalisePath(path);
    for (const name of [...this.files.keys()]) if (name.startsWith(resolved)) this.files.delete(name);
  }
}

/**
 * Returns the directory part of an absolute path.
 *
 * @param path - an absolute path
 */
export function parentOf(path) {
  const cut = path.lastIndexOf('/');
  return cut <= 0 ? '/' : path.slice(0, cut);
}

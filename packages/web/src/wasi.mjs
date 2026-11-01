// The WASI preview 1 host the engine's WebAssembly module runs on.
//
// A browser provides no WASI functions, so this file does. It implements the
// 26 the module imports and nothing else: `missingImports` names any other one
// when the module is loaded, so an engine change that starts needing a new
// call fails at load with the call's name rather than at the moment it runs.
//
// The module sees a file system of one preopened directory, `/`, and every path
// it opens is resolved by the file system object this host was given
// (`memory-fs.mjs` or `opfs-fs.mjs`). It never sees the host's own disk, in
// Node either.

/** WASI error numbers this host answers. */
export const Errno = {
  SUCCESS: 0,
  ACCES: 2,
  BADF: 8,
  EXIST: 20,
  INVAL: 28,
  IO: 29,
  ISDIR: 31,
  NOENT: 44,
  NOSPC: 51,
  NOSYS: 52,
  NOTDIR: 54,
  NOTEMPTY: 55,
  NOTSUP: 58,
  PERM: 63,
  SPIPE: 70,
};

/** WASI file types. */
const FileType = { UNKNOWN: 0, CHARACTER_DEVICE: 2, DIRECTORY: 3, REGULAR_FILE: 4 };

/** `path_open`'s open flags. */
const OFlags = { CREAT: 1, DIRECTORY: 2, EXCL: 4, TRUNC: 8 };

/** Every right, granted on every descriptor: the file system object enforces what is allowed. */
const ALL_RIGHTS = 0x1fffffffn;

/** The descriptor of the one preopened directory. */
const ROOT_FD = 3;

/**
 * An error a file system throws to answer a WASI call with an error number.
 */
export class WasiError extends Error {
  /**
   * @param errno - the WASI error number
   * @param message - what went wrong, for a person
   */
  constructor(errno, message) {
    super(message ?? `WASI error ${errno}`);
    this.errno = errno;
  }
}

/**
 * Thrown by `proc_exit`, so the caller can tell a module that exited from one that trapped.
 */
export class WasiExit extends Error {
  /** @param code - the exit status */
  constructor(code) {
    super(`the WebAssembly module exited with status ${code}`);
    this.code = code;
  }
}

/**
 * Turns a path the module sent into the absolute, normalised path a file system keys by.
 *
 * @param path - the path, relative to `/` or absolute
 */
export function normalisePath(path) {
  const parts = [];
  for (const part of path.split('/')) {
    if (part === '' || part === '.') continue;
    if (part === '..') parts.pop();
    else parts.push(part);
  }
  return '/' + parts.join('/');
}

/**
 * Lists the WASI functions a module imports that this host does not provide.
 *
 * @param module - a compiled `WebAssembly.Module`
 * @param host - the import object `WasiHost.imports()` returned
 */
export function missingImports(module, host) {
  return WebAssembly.Module.imports(module)
    .filter((entry) => entry.module === 'wasi_snapshot_preview1')
    .map((entry) => entry.name)
    .filter((name) => typeof host.wasi_snapshot_preview1[name] !== 'function');
}

/**
 * The WASI host for one WebAssembly instance.
 */
export class WasiHost {
  /**
   * @param fileSystem - where the module's files live
   * @param options - `env` (an object of variables) and `stdout`/`stderr` (functions taking a line)
   */
  constructor(fileSystem, options = {}) {
    this.fs = fileSystem;
    this.env = Object.entries(options.env ?? {}).map(([key, value]) => `${key}=${value}`);
    this.out = { 1: options.stdout ?? ((line) => console.log(line)), 2: options.stderr ?? ((line) => console.error(line)) };
    this.pending = { 1: '', 2: '' };
    this.descriptors = new Map();
    this.nextFd = ROOT_FD + 1;
    this.memory = null;
  }

  /**
   * Gives the host the instance's memory, which it reads and writes for every call.
   *
   * @param memory - the instance's exported `WebAssembly.Memory`
   */
  attach(memory) {
    this.memory = memory;
  }

  /** Returns a view over the whole of linear memory, fresh because a growth replaces the buffer. */
  view() {
    return new DataView(this.memory.buffer);
  }

  /** Returns the bytes of linear memory, fresh for the same reason. */
  bytes() {
    return new Uint8Array(this.memory.buffer);
  }

  /**
   * Reads a UTF-8 string the module passed as a pointer and a length.
   *
   * @param pointer - where it starts
   * @param length - how many bytes
   */
  string(pointer, length) {
    return new TextDecoder().decode(this.bytes().subarray(pointer, pointer + length));
  }

  /**
   * Resolves a path the module sent against a directory descriptor.
   *
   * @param fd - the directory it is relative to
   * @param pointer - the path's bytes
   * @param length - their length
   */
  resolve(fd, pointer, length) {
    const base = fd === ROOT_FD ? '/' : this.descriptors.get(fd)?.path;
    if (base === undefined) throw new WasiError(Errno.BADF, `no directory descriptor ${fd}`);
    const relative = this.string(pointer, length);
    return normalisePath(relative.startsWith('/') ? relative : `${base}/${relative}`);
  }

  /**
   * Returns an open file's record, or throws `BADF`.
   *
   * @param fd - the descriptor
   */
  file(fd) {
    const entry = this.descriptors.get(fd);
    if (!entry) throw new WasiError(Errno.BADF, `no file descriptor ${fd}`);
    if (entry.kind !== 'file') throw new WasiError(Errno.ISDIR, `descriptor ${fd} is a directory`);
    return entry;
  }

  /**
   * Walks a scatter or gather list in linear memory.
   *
   * @param pointer - the first `iovec`
   * @param count - how many there are
   */
  vectors(pointer, count) {
    const view = this.view();
    const list = [];
    for (let i = 0; i < count; i += 1) {
      list.push({ buf: view.getUint32(pointer + i * 8, true), len: view.getUint32(pointer + i * 8 + 4, true) });
    }
    return list;
  }

  /**
   * Writes a WASI `filestat` into linear memory.
   *
   * @param pointer - where it goes
   * @param stat - `{ kind, size, ino }`
   */
  writeStat(pointer, stat) {
    const view = this.view();
    view.setBigUint64(pointer, 1n, true);
    view.setBigUint64(pointer + 8, BigInt(stat.ino ?? 0), true);
    view.setUint8(pointer + 16, stat.kind === 'dir' ? FileType.DIRECTORY : FileType.REGULAR_FILE);
    view.setBigUint64(pointer + 24, 1n, true);
    view.setBigUint64(pointer + 32, BigInt(stat.size ?? 0), true);
    const now = BigInt(Date.now()) * 1_000_000n;
    view.setBigUint64(pointer + 40, now, true);
    view.setBigUint64(pointer + 48, BigInt(stat.mtime ?? 0) * 1_000_000n || now, true);
    view.setBigUint64(pointer + 56, now, true);
  }

  /**
   * Sends console output on to the line callbacks, a line at a time.
   *
   * @param fd - 1 or 2
   * @param text - what the module wrote
   */
  console(fd, text) {
    const joined = this.pending[fd] + text;
    const lines = joined.split('\n');
    this.pending[fd] = lines.pop();
    for (const line of lines) this.out[fd](line);
  }

  /**
   * Runs one WASI call, turning a thrown `WasiError` into its error number.
   *
   * @param body - the call
   */
  guard(body) {
    try {
      return body() ?? Errno.SUCCESS;
    } catch (error) {
      if (error instanceof WasiError) return error.errno;
      if (error instanceof WasiExit) throw error;
      this.out[2](`inillucent-wasm: a WASI call failed: ${error?.stack ?? error}`);
      return Errno.IO;
    }
  }

  /** Returns the import object a module is instantiated with. */
  imports() {
    const host = this;
    const call = (body) => (...args) => host.guard(() => body(...args));
    return {
      wasi_snapshot_preview1: {
        args_get: () => Errno.SUCCESS,
        args_sizes_get: call((count, size) => {
          host.view().setUint32(count, 0, true);
          host.view().setUint32(size, 0, true);
        }),
        environ_sizes_get: call((count, size) => {
          const encoded = host.env.map((entry) => new TextEncoder().encode(entry));
          host.view().setUint32(count, encoded.length, true);
          host.view().setUint32(size, encoded.reduce((sum, bytes) => sum + bytes.length + 1, 0), true);
        }),
        environ_get: call((pointers, buffer) => {
          let at = buffer;
          host.env.forEach((entry, index) => {
            const bytes = new TextEncoder().encode(entry);
            host.view().setUint32(pointers + index * 4, at, true);
            host.bytes().set(bytes, at);
            host.bytes()[at + bytes.length] = 0;
            at += bytes.length + 1;
          });
        }),
        clock_time_get: call((id, _precision, out) => {
          const nanos = id === 0 ? BigInt(Date.now()) * 1_000_000n : BigInt(Math.round(performance.now() * 1_000_000));
          host.view().setBigUint64(out, nanos, true);
        }),
        clock_res_get: call((_id, out) => host.view().setBigUint64(out, 1000n, true)),
        random_get: call((pointer, length) => {
          // `getRandomValues` fills at most 65,536 bytes a call.
          for (let at = 0; at < length; at += 65536) {
            crypto.getRandomValues(host.bytes().subarray(pointer + at, pointer + Math.min(length, at + 65536)));
          }
        }),
        sched_yield: () => Errno.SUCCESS,
        proc_exit: (code) => {
          throw new WasiExit(code);
        },
        poll_oneoff: call((input, output, count, events) => host.pollOneOff(input, output, count, events)),
        fd_prestat_get: call((fd, out) => {
          if (fd !== ROOT_FD) throw new WasiError(Errno.BADF);
          host.view().setUint8(out, 0);
          host.view().setUint32(out + 4, 1, true);
        }),
        fd_prestat_dir_name: call((fd, pointer, length) => {
          if (fd !== ROOT_FD) throw new WasiError(Errno.BADF);
          if (length >= 1) host.bytes()[pointer] = 0x2f;
        }),
        fd_fdstat_get: call((fd, out) => host.fdstat(fd, out)),
        fd_fdstat_set_flags: call((fd) => {
          if (fd > 2 && !host.descriptors.has(fd) && fd !== ROOT_FD) throw new WasiError(Errno.BADF);
        }),
        fd_close: call((fd) => host.close(fd)),
        fd_read: call((fd, iovs, count, out) => host.readVectors(fd, iovs, count, null, out)),
        fd_pread: call((fd, iovs, count, offset, out) => host.readVectors(fd, iovs, count, offset, out)),
        fd_write: call((fd, iovs, count, out) => host.writeVectors(fd, iovs, count, null, out)),
        fd_pwrite: call((fd, iovs, count, offset, out) => host.writeVectors(fd, iovs, count, offset, out)),
        fd_seek: call((fd, offset, whence, out) => host.seek(fd, offset, whence, out)),
        fd_tell: call((fd, out) => host.view().setBigUint64(out, BigInt(host.file(fd).position), true)),
        fd_sync: call((fd) => host.file(fd).handle.sync()),
        fd_datasync: call((fd) => host.file(fd).handle.sync()),
        fd_filestat_get: call((fd, out) => host.fdFilestat(fd, out)),
        fd_filestat_set_size: call((fd, size) => host.file(fd).handle.truncate(Number(size))),
        fd_readdir: call((fd, buffer, length, cookie, out) => host.readdir(fd, buffer, length, cookie, out)),
        path_open: call((dirfd, _dirflags, path, pathLength, oflags, _base, _inheriting, fdflags, out) =>
          host.pathOpen(dirfd, path, pathLength, oflags, fdflags, out),
        ),
        path_filestat_get: call((dirfd, _flags, path, length, out) => {
          const stat = host.fs.stat(host.resolve(dirfd, path, length));
          if (!stat) throw new WasiError(Errno.NOENT);
          host.writeStat(out, stat);
        }),
        path_readlink: call((dirfd, path, length) => {
          if (!host.fs.stat(host.resolve(dirfd, path, length))) throw new WasiError(Errno.NOENT);
          throw new WasiError(Errno.INVAL);
        }),
        path_rename: call((fromFd, from, fromLength, toFd, to, toLength) =>
          host.fs.rename(host.resolve(fromFd, from, fromLength), host.resolve(toFd, to, toLength)),
        ),
        path_unlink_file: call((dirfd, path, length) => host.fs.unlink(host.resolve(dirfd, path, length))),
        path_create_directory: call((dirfd, path, length) => host.fs.mkdir(host.resolve(dirfd, path, length))),
        path_remove_directory: call((dirfd, path, length) => host.fs.rmdir(host.resolve(dirfd, path, length))),
      },
    };
  }

  /**
   * Answers `fd_fdstat_get`.
   *
   * @param fd - the descriptor
   * @param out - where the `fdstat` goes
   */
  fdstat(fd, out) {
    let type;
    if (fd <= 2) type = FileType.CHARACTER_DEVICE;
    else if (fd === ROOT_FD) type = FileType.DIRECTORY;
    else {
      const entry = this.descriptors.get(fd);
      if (!entry) throw new WasiError(Errno.BADF);
      type = entry.kind === 'dir' ? FileType.DIRECTORY : FileType.REGULAR_FILE;
    }
    const view = this.view();
    view.setUint8(out, type);
    view.setUint16(out + 2, 0, true);
    view.setBigUint64(out + 8, ALL_RIGHTS, true);
    view.setBigUint64(out + 16, ALL_RIGHTS, true);
  }

  /**
   * Answers `fd_filestat_get`.
   *
   * @param fd - the descriptor
   * @param out - where the `filestat` goes
   */
  fdFilestat(fd, out) {
    if (fd <= 2) return this.writeStat(out, { kind: 'file', size: 0, ino: fd });
    if (fd === ROOT_FD) return this.writeStat(out, { kind: 'dir', size: 0, ino: 1 });
    const entry = this.descriptors.get(fd);
    if (!entry) throw new WasiError(Errno.BADF);
    if (entry.kind === 'dir') return this.writeStat(out, this.fs.stat(entry.path) ?? { kind: 'dir', ino: 2 });
    return this.writeStat(out, { kind: 'file', size: entry.handle.size(), ino: entry.handle.ino });
  }

  /**
   * Answers `path_open`.
   *
   * @param dirfd - the directory the path is relative to
   * @param path - the path's bytes
   * @param length - their length
   * @param oflags - create, directory, exclusive, truncate
   * @param fdflags - append and the sync flags
   * @param out - where the new descriptor goes
   */
  pathOpen(dirfd, path, length, oflags, fdflags, out) {
    const resolved = this.resolve(dirfd, path, length);
    const existing = this.fs.stat(resolved);
    if (existing?.kind === 'dir' || (oflags & OFlags.DIRECTORY)) {
      if (!existing) throw new WasiError(Errno.NOENT, `no directory ${resolved}`);
      if (existing.kind !== 'dir') throw new WasiError(Errno.NOTDIR);
      return this.install({ kind: 'dir', path: resolved }, out);
    }
    if (existing && (oflags & OFlags.EXCL)) throw new WasiError(Errno.EXIST, `${resolved} exists`);
    if (!existing && !(oflags & OFlags.CREAT)) throw new WasiError(Errno.NOENT, `no file ${resolved}`);
    const handle = this.fs.open(resolved, { create: Boolean(oflags & OFlags.CREAT) });
    if (oflags & OFlags.TRUNC) handle.truncate(0);
    return this.install({ kind: 'file', path: resolved, handle, position: 0, append: Boolean(fdflags & 1) }, out);
  }

  /**
   * Gives an open file or directory the next descriptor number.
   *
   * @param entry - what the descriptor names
   * @param out - where the number goes
   */
  install(entry, out) {
    const fd = this.nextFd;
    this.nextFd += 1;
    this.descriptors.set(fd, entry);
    this.view().setUint32(out, fd, true);
  }

  /**
   * Answers `fd_close`.
   *
   * @param fd - the descriptor
   */
  close(fd) {
    const entry = this.descriptors.get(fd);
    if (!entry) throw new WasiError(Errno.BADF);
    this.descriptors.delete(fd);
    if (entry.kind === 'file') entry.handle.close?.();
  }

  /**
   * Answers `fd_read` and `fd_pread`.
   *
   * @param fd - the descriptor
   * @param iovs - the scatter list
   * @param count - its length
   * @param offset - a `BigInt` for `fd_pread`, null to read at the cursor
   * @param out - where the byte count goes
   */
  readVectors(fd, iovs, count, offset, out) {
    if (fd <= 2) {
      this.view().setUint32(out, 0, true);
      return;
    }
    const entry = this.file(fd);
    let at = offset === null ? entry.position : Number(offset);
    let total = 0;
    for (const { buf, len } of this.vectors(iovs, count)) {
      const read = entry.handle.read(this.bytes().subarray(buf, buf + len), at);
      at += read;
      total += read;
      if (read < len) break;
    }
    if (offset === null) entry.position = at;
    this.view().setUint32(out, total, true);
  }

  /**
   * Answers `fd_write` and `fd_pwrite`.
   *
   * @param fd - the descriptor
   * @param iovs - the gather list
   * @param count - its length
   * @param offset - a `BigInt` for `fd_pwrite`, null to write at the cursor
   * @param out - where the byte count goes
   */
  writeVectors(fd, iovs, count, offset, out) {
    const list = this.vectors(iovs, count);
    if (fd === 1 || fd === 2) {
      let total = 0;
      for (const { buf, len } of list) {
        this.console(fd, this.string(buf, len));
        total += len;
      }
      this.view().setUint32(out, total, true);
      return;
    }
    const entry = this.file(fd);
    let at = offset === null ? (entry.append ? entry.handle.size() : entry.position) : Number(offset);
    let total = 0;
    for (const { buf, len } of list) {
      // Copied out first: the file system may grow its own buffers, and a view
      // into linear memory must not be held across anything that could run the module.
      const written = entry.handle.write(this.bytes().slice(buf, buf + len), at);
      at += written;
      total += written;
    }
    if (offset === null) entry.position = at;
    this.view().setUint32(out, total, true);
  }

  /**
   * Answers `fd_seek`.
   *
   * @param fd - the descriptor
   * @param offset - a `BigInt`, signed
   * @param whence - 0 from the start, 1 from the cursor, 2 from the end
   * @param out - where the new position goes
   */
  seek(fd, offset, whence, out) {
    if (fd <= 2) throw new WasiError(Errno.SPIPE);
    const entry = this.file(fd);
    const base = whence === 0 ? 0 : whence === 1 ? entry.position : entry.handle.size();
    const next = base + Number(offset);
    if (next < 0) throw new WasiError(Errno.INVAL);
    entry.position = next;
    this.view().setBigUint64(out, BigInt(next), true);
  }

  /**
   * Answers `fd_readdir`: WASI `dirent` records, from the entry the cookie names.
   *
   * @param fd - a directory descriptor
   * @param buffer - where the records go
   * @param length - how much room there is
   * @param cookie - a `BigInt`: how many entries to skip
   * @param out - where the byte count goes
   */
  readdir(fd, buffer, length, cookie, out) {
    const entry = fd === ROOT_FD ? { kind: 'dir', path: '/' } : this.descriptors.get(fd);
    if (!entry) throw new WasiError(Errno.BADF);
    if (entry.kind !== 'dir') throw new WasiError(Errno.NOTDIR);
    const names = ['.', '..', ...this.fs.list(entry.path)];
    const encoder = new TextEncoder();
    let used = 0;
    for (let index = Number(cookie); index < names.length; index += 1) {
      const name = encoder.encode(names[index]);
      const child = this.fs.stat(normalisePath(`${entry.path}/${names[index]}`));
      const record = new Uint8Array(24 + name.length);
      const view = new DataView(record.buffer);
      view.setBigUint64(0, BigInt(index + 1), true);
      view.setBigUint64(8, BigInt(child?.ino ?? 0), true);
      view.setUint32(16, name.length, true);
      view.setUint8(20, child?.kind === 'file' ? FileType.REGULAR_FILE : FileType.DIRECTORY);
      record.set(name, 24);
      const room = length - used;
      // A record that does not fit is cut at the room left; the caller sees a
      // full buffer and asks again from the same cookie, which is the WASI rule.
      this.bytes().set(record.subarray(0, Math.min(room, record.length)), buffer + used);
      used += Math.min(room, record.length);
      if (used >= length) break;
    }
    this.view().setUint32(out, used, true);
  }

  /**
   * Answers `poll_oneoff`, which the engine uses only to sleep.
   *
   * Nothing else runs inside the instance while it sleeps, so nothing it is
   * waiting for can change: every clock subscription is answered at once, and a
   * busy wait ends on the engine's next check of its own deadline.
   *
   * @param input - the subscriptions
   * @param output - where the events go
   * @param count - how many subscriptions
   * @param events - where the event count goes
   */
  pollOneOff(input, output, count, events) {
    const view = this.view();
    for (let i = 0; i < count; i += 1) {
      const userdata = view.getBigUint64(input + i * 48, true);
      const tag = view.getUint8(input + i * 48 + 8);
      view.setBigUint64(output + i * 32, userdata, true);
      view.setUint16(output + i * 32 + 8, tag === 0 ? Errno.SUCCESS : Errno.NOTSUP, true);
      view.setUint8(output + i * 32 + 10, tag);
    }
    view.setUint32(events, count, true);
  }
}

//! The WASI half of the operating-system VFS, for `wasm32-wasip1`.
//!
//! Invariant: a WASI module is one process with one thread, so every lock is
//! decided by the in-process `LockTable` alone and the wal-index lives on the
//! heap. Nothing here asks the host to lock a byte range, because WASI preview 1
//! has no call for it and there is no second process to exclude.
//!
//! The host is a browser or Node, through the WASI shim in
//! `packages/web/src/wasi.js`. That shim keeps files in memory or in the
//! origin private file system. A second browser tab is a second WebAssembly
//! instance with its own heap, so two tabs on one persisted database would not
//! see each other's locks; the web package refuses a second opener of the same
//! database with the Web Locks API before the engine is asked, and says so.
//!
//! Positional reads and writes go straight to `fd_pread` and `fd_pwrite`. The
//! standard library has them only behind the unstable `wasi_ext` feature, and a
//! seek followed by a read would move a cursor that two handles on one file
//! share in some hosts.

use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use crate::contract::{DeviceCharacteristics, FileIdentity, FileLock, SharedMemory};
use crate::error::{self, VfsError, VfsOperation, VfsResult};
use crate::locks::{next_step, HandleId, LockConflict, LockTable};
use crate::path::DbPath;

/// The name this VFS registers under.
pub const VFS_NAME: &str = "wasi";

/// The calls this module makes to the WASI host directly.
///
/// The standard library imports the same functions from the same module for
/// its own use, so declaring them again adds no import the host must provide.
mod host {
    /// One buffer of a scatter or gather list, as WASI lays it out.
    #[repr(C)]
    pub struct IoVec {
        /// Where the buffer starts.
        pub buf: *mut u8,
        /// How long it is.
        pub len: usize,
    }

    /// What `fd_filestat_get` writes, in WASI's layout.
    #[repr(C)]
    #[derive(Default)]
    pub struct FileStat {
        /// The device the file is on.
        pub dev: u64,
        /// The file's number on that device.
        pub ino: u64,
        /// Regular file, directory and so on.
        pub filetype: u8,
        /// How many names the file has.
        pub nlink: u64,
        /// The length in bytes.
        pub size: u64,
        /// Last access, in nanoseconds.
        pub atim: u64,
        /// Last change to the contents, in nanoseconds.
        pub mtim: u64,
        /// Last change to the metadata, in nanoseconds.
        pub ctim: u64,
    }

    #[link(wasm_import_module = "wasi_snapshot_preview1")]
    extern "C" {
        /// Reads at an absolute offset without moving the file's cursor.
        pub fn fd_pread(
            fd: u32,
            iovs: *const IoVec,
            count: usize,
            offset: u64,
            read: *mut usize,
        ) -> u16;
        /// Writes at an absolute offset without moving the file's cursor.
        pub fn fd_pwrite(
            fd: u32,
            iovs: *const IoVec,
            count: usize,
            offset: u64,
            written: *mut usize,
        ) -> u16;
        /// Describes an open file.
        pub fn fd_filestat_get(fd: u32, stat: *mut FileStat) -> u16;
        /// Fills a buffer with randomness.
        pub fn random_get(buf: *mut u8, len: usize) -> u16;
    }
}

/// Turns a WASI error number into the standard library's error.
///
/// @param errno - what the host answered; 0 is success
fn check(errno: u16) -> io::Result<()> {
    if errno == 0 {
        return Ok(());
    }
    Err(io::Error::from_raw_os_error(i32::from(errno)))
}

/// Returns the WASI descriptor number of an open file.
///
/// @param file - the open file
fn descriptor(file: &File) -> io::Result<u32> {
    u32::try_from(file.as_raw_fd()).map_err(|_| io::Error::other("a negative file descriptor"))
}

/// Reads at an absolute offset.
///
/// @param file - the open file
/// @param offset - where to read from
/// @param output - where the bytes go
pub fn read_at(file: &File, offset: u64, output: &mut [u8]) -> io::Result<usize> {
    let fd = descriptor(file)?;
    let vector = host::IoVec {
        buf: output.as_mut_ptr(),
        len: output.len(),
    };
    let mut read = 0usize;
    // SAFETY: the vector names memory `output` owns for the whole call, the host
    // writes at most `len` bytes into it, and `read` is a live local.
    check(unsafe { host::fd_pread(fd, &vector, 1, offset, &mut read) })?;
    Ok(read)
}

/// Reads at an absolute offset into memory that has not been initialised.
///
/// The host only writes to the buffer, so it needs no initialised bytes to read
/// over; see `VfsFile::read_exact_into`.
///
/// @param file - the open file
/// @param offset - where to read from
/// @param output - where the bytes go
pub fn read_at_spare(
    file: &File,
    offset: u64,
    output: &mut [std::mem::MaybeUninit<u8>],
) -> io::Result<usize> {
    let fd = descriptor(file)?;
    let vector = host::IoVec {
        buf: output.as_mut_ptr().cast(),
        len: output.len(),
    };
    let mut read = 0usize;
    // SAFETY: as in `read_at`; the host writes and never reads the buffer, so
    // its being uninitialised is never observed.
    check(unsafe { host::fd_pread(fd, &vector, 1, offset, &mut read) })?;
    Ok(read)
}

/// Writes at an absolute offset.
///
/// @param file - the open file
/// @param offset - where to write
/// @param input - the bytes
pub fn write_at(file: &File, offset: u64, input: &[u8]) -> io::Result<usize> {
    let fd = descriptor(file)?;
    let vector = host::IoVec {
        buf: input.as_ptr().cast_mut(),
        len: input.len(),
    };
    let mut written = 0usize;
    // SAFETY: the vector names memory `input` owns for the whole call, and
    // `fd_pwrite` only reads from it, so the cast to a mutable pointer is never
    // used to write.
    check(unsafe { host::fd_pwrite(fd, &vector, 1, offset, &mut written) })?;
    Ok(written)
}

/// Asks the host to describe an open file.
///
/// @param file - the open file
fn stat(file: &File) -> io::Result<host::FileStat> {
    let fd = descriptor(file)?;
    let mut answer = host::FileStat::default();
    // SAFETY: `answer` is a live, correctly laid out `filestat` the host fills.
    check(unsafe { host::fd_filestat_get(fd, &mut answer) })?;
    Ok(answer)
}

/// Returns a file's length in bytes.
///
/// @param file - the open file
pub fn file_len(file: &File) -> io::Result<u64> {
    stat(file).map(|answer| answer.size)
}

/// Reports whether a path names something, and what.
///
/// @param path - the path to ask about
pub fn path_state(path: &std::path::Path) -> io::Result<Option<crate::os::PathState>> {
    match std::fs::metadata(path) {
        Ok(metadata) => Ok(Some(crate::os::PathState::of(&metadata))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Returns the file number of the file at a path, or `None` when nothing is there.
///
/// WASI preview 1 answers a file number only for an open descriptor, so the
/// file is opened for the question and closed again.
///
/// @param path - the path to ask about
pub fn file_number_by_name(path: &std::path::Path) -> io::Result<Option<u128>> {
    match File::open(path) {
        Ok(file) => Ok(Some(u128::from(stat(&file)?.ino))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Returns the names in a directory that start with a prefix.
///
/// @param directory - the directory to list
/// @param prefix - what every returned name starts with
pub fn names_starting_with(directory: &std::path::Path, prefix: &str) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name.starts_with(prefix) {
            names.push(name);
        }
    }
    Ok(names)
}

/// Builds a new handle's lock state, keyed by the file's identity.
///
/// @param file - the open file
/// @param handle - this handle's number in the process
pub fn lock_state(
    file: &Arc<File>,
    handle: HandleId,
) -> VfsResult<(LockState, Option<FileIdentity>)> {
    let identity = file_identity(file)?;
    Ok((LockState::new(&identity, handle), Some(identity)))
}

/// Returns the device and file numbers that identify a file.
///
/// @param file - the open file
pub fn file_identity(file: &File) -> VfsResult<FileIdentity> {
    let answer = stat(file).map_err(|error| VfsError::from_io(VfsOperation::FileSize, &error))?;
    Ok(FileIdentity {
        volume: answer.dev,
        file: u128::from(answer.ino),
    })
}

/// Returns what a WASI host guarantees, which is nothing beyond the defaults.
pub fn device_characteristics() -> DeviceCharacteristics {
    DeviceCharacteristics::conservative()
}

/// Does nothing: a WASI host keeps its directory entries itself, and preview 1
/// cannot open a directory as a file to flush it.
///
/// @param _path - the directory that would be flushed
pub fn sync_directory(_path: &DbPath) -> VfsResult<()> {
    Ok(())
}

/// Flushes a file the strongest way WASI can, which is `fd_sync`.
///
/// @param file - the file to flush
pub fn full_sync(file: &File) -> std::io::Result<()> {
    file.sync_all()
}

/// Fills `output` with randomness from the host.
///
/// @param output - the buffer to fill
pub fn system_randomness(output: &mut [u8]) -> VfsResult<()> {
    if output.is_empty() {
        return Ok(());
    }
    // SAFETY: the pointer and length describe memory `output` owns for the call.
    check(unsafe { host::random_get(output.as_mut_ptr(), output.len()) })
        .map_err(|error| VfsError::from_io(VfsOperation::Read, &error))
}

/// Every wal-index this process has open, by the path of its shared-memory file.
///
/// Two connections to one database must see one wal-index, and with no memory
/// mapping to share the bytes through, the process keeps one heap copy per path.
fn shm_registry() -> &'static Mutex<HashMap<PathBuf, Arc<crate::memory::MemoryShm>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, Arc<crate::memory::MemoryShm>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Opens the shared memory for a database: the heap copy every connection in
/// this process shares.
///
/// @param path - the database's shared-memory path
pub fn open_shm(path: &DbPath) -> VfsResult<Arc<dyn SharedMemory>> {
    let mut map = guard(shm_registry());
    let shm = map.entry(path.as_path().to_path_buf()).or_default();
    Ok(Arc::clone(shm) as Arc<dyn SharedMemory>)
}

/// Every file identity this process has handles on, with their lock table.
fn registry() -> &'static Mutex<HashMap<FileIdentity, Arc<Mutex<LockTable>>>> {
    static REGISTRY: OnceLock<Mutex<HashMap<FileIdentity, Arc<Mutex<LockTable>>>>> =
        OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The lock level one handle holds, arbitrated against its siblings.
#[derive(Debug)]
pub struct LockState {
    handle: HandleId,
    table: Arc<Mutex<LockTable>>,
    level: Mutex<FileLock>,
}

impl LockState {
    /// Creates the state for a freshly opened handle.
    ///
    /// @param identity - the file the handle is on
    /// @param handle - this handle's number in the process
    pub fn new(identity: &FileIdentity, handle: HandleId) -> LockState {
        let mut map = guard(registry());
        let table = Arc::clone(
            map.entry(identity.clone())
                .or_insert_with(|| Arc::new(Mutex::new(LockTable::new()))),
        );
        LockState {
            handle,
            table,
            level: Mutex::new(FileLock::None),
        }
    }

    /// Returns the level this handle holds.
    pub fn level(&self) -> FileLock {
        *guard(&self.level)
    }

    /// Raises the lock to `target`, one protocol step at a time, stopping at the
    /// first step another handle refuses.
    ///
    /// @param _file - unused: the table alone decides
    /// @param target - the level wanted
    pub fn acquire(&self, _file: &File, target: FileLock) -> VfsResult<()> {
        let mut level = guard(&self.level);
        let mut table = guard(&self.table);
        let mut step = *level;
        let mut refusal = None;
        while step < target {
            let next = next_step(step, target);
            match table.acquire(self.handle, step, next) {
                Ok(()) => step = next,
                Err(LockConflict::Busy) => {
                    refusal = Some(error::busy(format!("cannot raise to {next:?}")));
                    break;
                }
                Err(LockConflict::Protocol) => {
                    refusal = Some(error::misuse(format!(
                        "illegal transition {step:?} -> {next:?}"
                    )));
                    break;
                }
            }
        }
        *level = step;
        refusal.map_or(Ok(()), Err)
    }

    /// Lowers the lock to `target`.
    ///
    /// @param _file - unused: the table alone decides
    /// @param target - the level wanted
    pub fn release(&self, _file: &File, target: FileLock) -> VfsResult<()> {
        let mut level = guard(&self.level);
        if target >= *level {
            return Ok(());
        }
        guard(&self.table)
            .release(self.handle, *level, target)
            .map_err(|_| error::misuse(format!("illegal release {:?} -> {target:?}", *level)))?;
        *level = target;
        Ok(())
    }

    /// Reports whether another handle in this process holds RESERVED or stronger.
    ///
    /// @param _file - unused: the table alone decides
    pub fn check_reserved(&self, _file: &File) -> VfsResult<bool> {
        Ok(guard(&self.table).has_reserved_or_stronger(self.handle))
    }
}

impl Drop for LockState {
    /// Releases whatever the handle still held.
    fn drop(&mut self) {
        guard(&self.table).release_all(self.handle);
    }
}

/// Locks a mutex, recovering from poisoning rather than propagating a panic.
///
/// @param mutex - the mutex to lock
fn guard<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    }
}

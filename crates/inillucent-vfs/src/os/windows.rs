//! The Windows half of the operating-system VFS.
//!
//! Invariant: every `unsafe` block here has its safety argument written above
//! it, and none of them holds a raw pointer past the call it was made for.
//!
//! Windows byte-range locks are per *handle*, not per process, so two handles
//! in one process conflict exactly as two processes do. That is the semantics
//! the locking protocol wants, so the Windows implementation goes straight to
//! the kernel with no in-process bookkeeping; the POSIX implementation cannot,
//! and says why in its own file.

use std::fs::File;
use std::io;
use std::os::windows::fs::FileExt;
use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;
use std::sync::Mutex;

use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, GetFileSizeEx, LockFileEx, ReadFile, UnlockFileEx,
    BY_HANDLE_FILE_INFORMATION, LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY,
};
use windows_sys::Win32::System::Memory::{
    CreateFileMappingW, MapViewOfFile, UnmapViewOfFile, FILE_MAP_READ, FILE_MAP_WRITE,
    PAGE_READWRITE,
};
use windows_sys::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows_sys::Win32::System::IO::OVERLAPPED;

use crate::contract::{DeviceCharacteristics, FileIdentity, FileLock, SharedMemory};
use crate::error::{self, VfsError, VfsOperation, VfsResult};
use crate::locks::HandleId;
use crate::os::filelock::FileShm;
use crate::os::ranges::{PENDING_BYTE, RESERVED_BYTE, SHARED_FIRST, SHARED_SIZE};
use crate::path::DbPath;

/// The name this VFS registers under.
pub const VFS_NAME: &str = "win32";

/// Reads at an absolute offset without disturbing logical file position use.
pub fn read_at(file: &File, offset: u64, output: &mut [u8]) -> io::Result<usize> {
    file.seek_read(output, offset)
}

/// Reads at an absolute offset into memory that has not been initialised.
///
/// `ReadFile` only writes to the buffer, so it needs no initialised bytes to
/// read over; see `VfsFile::read_exact_into` for why that matters. The end of
/// the file is `Ok(0)`, as `read_at` reports it.
///
/// @param file - the open file
/// @param offset - where to read from
/// @param output - where the bytes go
pub fn read_at_spare(
    file: &File,
    offset: u64,
    output: &mut [std::mem::MaybeUninit<u8>],
) -> io::Result<usize> {
    let wanted = u32::try_from(output.len()).unwrap_or(u32::MAX);
    let mut overlapped = overlapped_at(offset);
    let mut read: u32 = 0;
    // SAFETY: the pointer and `wanted` describe memory `output` owns for the
    // whole call, `ReadFile` writes at most `wanted` bytes to it and reads none,
    // the handle is valid for the life of `file`, and `overlapped` and `read`
    // are owned locals the synchronous call finishes with before it returns.
    let ok = unsafe {
        ReadFile(
            file.as_raw_handle() as HANDLE,
            output.as_mut_ptr().cast(),
            wanted,
            &mut read,
            &mut overlapped,
        )
    };
    if ok == 0 {
        let failure = io::Error::last_os_error();
        // A read that starts at or past the end of the file fails with
        // ERROR_HANDLE_EOF (38) on a handle given an offset this way.
        if failure.raw_os_error() == Some(38) {
            return Ok(0);
        }
        return Err(failure);
    }
    Ok(read as usize)
}

/// Writes at an absolute offset.
pub fn write_at(file: &File, offset: u64, input: &[u8]) -> io::Result<usize> {
    file.seek_write(input, offset)
}

/// Returns a file's length in bytes.
///
/// **`GetFileSizeEx` rather than the standard library's `metadata`**
/// (task-2185). `metadata` asks for every attribute of the file, and a
/// connection under `locking_mode = normal` asks the log's length on every
/// statement, to see whether another process committed: on an autocommit
/// point read that one call was a third of the statement. The size alone is a
/// lighter query.
///
/// @param file - the open file
pub fn file_len(file: &File) -> io::Result<u64> {
    let mut size: i64 = 0;
    // SAFETY: `size` is an owned, aligned i64, which is the out parameter the
    // call writes, and the handle is valid for the life of `file`.
    let ok = unsafe { GetFileSizeEx(file.as_raw_handle() as HANDLE, &mut size) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    u64::try_from(size).map_err(|_| io::Error::other("a file reported a negative length"))
}

/// Reports whether a path names something, and what.
///
/// `None` when nothing is there.
///
/// **`GetFileAttributesW` rather than the standard library's `metadata`**
/// (task-2191). On Windows `metadata` opens a handle to the file, asks for its
/// information and closes it again, which is three system calls and the file
/// system filter's work for each. An open of a database asks this several
/// times, for the file, its journal and its log segments, and the calls were
/// a sixth of opening a small database from Python. The attributes answer both
/// questions asked here in one call that opens nothing.
///
/// A path too long for the call without the long path prefix is asked the
/// standard library's way, which adds the prefix itself.
///
/// @param path - the path to ask about
pub fn path_state(path: &std::path::Path) -> io::Result<Option<crate::os::PathState>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{
        GetLastError, ERROR_FILE_NOT_FOUND, ERROR_INVALID_NAME, ERROR_PATH_NOT_FOUND,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_READONLY,
        INVALID_FILE_ATTRIBUTES,
    };
    /// The longest path the call takes without the long path prefix.
    const SHORT_PATH: usize = 259;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.len() > SHORT_PATH || wide.contains(&0) {
        return match std::fs::metadata(path) {
            Ok(metadata) => Ok(Some(crate::os::PathState::of(&metadata))),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        };
    }
    wide.push(0);
    if let Some(by_name) = file_information_by_name() {
        let mut stat = FileStatInformation::default();
        // SAFETY: `wide` is a NUL terminated UTF-16 string and `stat` is a
        // writable buffer of the size passed, both alive across the call,
        // which keeps neither. Class 0 is `FileStatByNameInfo`, whose layout
        // `FileStatInformation` repeats.
        let ok = unsafe {
            by_name(
                wide.as_ptr(),
                0,
                (&mut stat as *mut FileStatInformation).cast(),
                std::mem::size_of::<FileStatInformation>() as u32,
            )
        };
        if ok != 0 {
            return Ok(Some(crate::os::PathState {
                read_only: stat.file_attributes & FILE_ATTRIBUTE_READONLY != 0,
                file: stat.file_attributes & FILE_ATTRIBUTE_DIRECTORY == 0,
                len: u64::try_from(stat.end_of_file).ok(),
            }));
        }
        // **"Not found" is an answer, and asking again is a second system call
        // for it** (task-2191). Every open asks whether a rollback journal is
        // beside the database, and there normally is none: the older call
        // after this one was 7% of a Python open and close. Any other failure
        // falls through to the older call, which answers every case this one
        // can refuse, such as a file system that does not support it.
        // SAFETY: reads the calling thread's last error, which the call above set.
        let code = unsafe { GetLastError() };
        if matches!(code, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
            return Ok(None);
        }
    }
    // SAFETY: `wide` is a NUL terminated UTF-16 string that lives across the
    // call, and the call reads it and keeps nothing.
    let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attributes != INVALID_FILE_ATTRIBUTES {
        return Ok(Some(crate::os::PathState {
            read_only: attributes & FILE_ATTRIBUTE_READONLY != 0,
            file: attributes & FILE_ATTRIBUTE_DIRECTORY == 0,
            len: None,
        }));
    }
    // SAFETY: reads the calling thread's last error, which the call above set.
    let code = unsafe { GetLastError() };
    match code {
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND | ERROR_INVALID_NAME => Ok(None),
        other => Err(io::Error::from_raw_os_error(other as i32)),
    }
}

/// Returns the names in a directory that start with a prefix.
///
/// **`FindFirstFileExW` with the prefix as its pattern** (task-2191). The
/// standard library's `read_dir` lists every name in the directory and the
/// caller then filters them, so a check for a database's few log segments in a
/// directory of a thousand files read a thousand names, at every command line
/// call. The pattern makes the file system do the filtering. `*` and `?` in
/// the prefix are refused, so it cannot widen what is matched.
///
/// @param directory - the directory to list
/// @param prefix - what every returned name starts with
pub fn names_starting_with(directory: &std::path::Path, prefix: &str) -> io::Result<Vec<String>> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Foundation::{
        GetLastError, ERROR_FILE_NOT_FOUND, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW, FindNextFileW,
        FIND_FIRST_EX_LARGE_FETCH, WIN32_FIND_DATAW,
    };
    if prefix.contains(['*', '?']) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "a prefix cannot hold a wildcard",
        ));
    }
    let mut pattern: Vec<u16> = directory
        .join(format!("{prefix}*"))
        .as_os_str()
        .encode_wide()
        .collect();
    pattern.push(0);
    // SAFETY: the structure is plain data the call fills, so zero is a valid start.
    let mut found: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    // SAFETY: `pattern` is NUL terminated and `found` is writable, both alive
    // across the call; the handle it answers is closed below.
    let handle = unsafe {
        FindFirstFileExW(
            pattern.as_ptr(),
            FindExInfoBasic,
            (&mut found as *mut WIN32_FIND_DATAW).cast(),
            FindExSearchNameMatch,
            std::ptr::null(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        // SAFETY: reads the calling thread's last error, which the call set.
        return match unsafe { GetLastError() } {
            ERROR_FILE_NOT_FOUND => Ok(Vec::new()),
            other => Err(io::Error::from_raw_os_error(other as i32)),
        };
    }
    let mut names = Vec::new();
    loop {
        let length = found
            .cFileName
            .iter()
            .position(|unit| *unit == 0)
            .unwrap_or(found.cFileName.len());
        let name = std::ffi::OsString::from_wide(found.cFileName.get(..length).unwrap_or(&[]));
        names.push(name.to_string_lossy().into_owned());
        // SAFETY: the handle is the open search above and `found` is writable.
        if unsafe { FindNextFileW(handle, &mut found) } == 0 {
            break;
        }
    }
    // SAFETY: the handle is the open search above, closed once.
    unsafe { FindClose(handle) };
    Ok(names)
}

/// `FILE_STAT_INFORMATION`, which `GetFileInformationByName` fills for
/// `FileStatByNameInfo`.
#[repr(C)]
#[derive(Default)]
struct FileStatInformation {
    file_id: i64,
    creation_time: i64,
    last_access_time: i64,
    last_write_time: i64,
    change_time: i64,
    allocation_size: i64,
    end_of_file: i64,
    file_attributes: u32,
    reparse_tag: u32,
    number_of_links: u32,
    effective_access: u32,
}

/// `GetFileInformationByName`'s signature.
// SAFETY: a function pointer type and not a call; the one call through it
// states its own argument where it is made.
type FileInformationByName =
    unsafe extern "system" fn(*const u16, i32, *mut core::ffi::c_void, u32) -> i32;

/// Returns `GetFileInformationByName` when this Windows has it.
///
/// **Looked up at run time, once** (task-2191). It asks a path's attributes
/// without opening the file, in about 3 us here against 7 to 11 for the
/// attribute calls. It arrived in Windows 11 24H2, so linking it would stop
/// the library from loading on anything older; an older Windows gets `None`
/// and the caller uses `GetFileAttributesW`.
fn file_information_by_name() -> Option<FileInformationByName> {
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut core::ffi::c_void;
        fn GetProcAddress(
            module: *mut core::ffi::c_void,
            name: *const core::ffi::c_char,
        ) -> *mut core::ffi::c_void;
    }
    static FOUND: std::sync::OnceLock<Option<FileInformationByName>> = std::sync::OnceLock::new();
    *FOUND.get_or_init(|| {
        let library: Vec<u16> = "api-ms-win-core-file-l2-1-4.dll\0".encode_utf16().collect();
        // SAFETY: the name is NUL terminated and lives across the call. A
        // library that is not there answers null, which is checked.
        let module = unsafe { LoadLibraryW(library.as_ptr()) };
        if module.is_null() {
            return None;
        }
        // SAFETY: the module is loaded and stays loaded, since nothing frees
        // it, and the symbol name is a NUL terminated constant.
        let address = unsafe { GetProcAddress(module, c"GetFileInformationByName".as_ptr()) };
        if address.is_null() {
            return None;
        }
        // SAFETY: the address is the export of that name, whose signature is
        // the one `FileInformationByName` states.
        Some(unsafe {
            std::mem::transmute::<*mut core::ffi::c_void, FileInformationByName>(address)
        })
    })
}

/// Builds a new handle's lock state, without the file's identity.
///
/// **The identity is not read at the open here** (task-2191). Windows keeps
/// byte range locks per handle, so [`LockState`] has no use for it, and
/// nothing on the engine's path asks for it. `GetFileInformationByHandle` was
/// 4.5% of a Python open and close. `OsFile::file_identity` reads it the first
/// time it is asked.
///
/// @param _file - the open file
/// @param _handle - this handle's number in the process
pub fn lock_state(
    _file: &Arc<File>,
    _handle: HandleId,
) -> VfsResult<(LockState, Option<FileIdentity>)> {
    Ok((
        LockState {
            level: Mutex::new(FileLock::None),
            seen: AtomicU8::new(0),
        },
        None,
    ))
}

/// Returns the volume serial number and file index that identify a file.
pub fn file_identity(file: &File) -> VfsResult<FileIdentity> {
    // SAFETY: BY_HANDLE_FILE_INFORMATION is plain data with no invalid bit
    // patterns, so an all-zero value is a valid one; every field the caller
    // reads below is written by the call that follows.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a properly aligned, fully owned structure of the exact
    // type the call expects, and the handle is valid for the life of `file`.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) };
    if ok == 0 {
        return Err(VfsError::from_io(
            VfsOperation::FileSize,
            &io::Error::last_os_error(),
        ));
    }
    let index = (u128::from(info.nFileIndexHigh) << 32) | u128::from(info.nFileIndexLow);
    Ok(FileIdentity {
        volume: u64::from(info.dwVolumeSerialNumber),
        file: index,
    })
}

/// Returns what a Windows file system guarantees.
///
/// Nothing is claimed, and in particular `undeletable_when_open` is *not*.
/// SQLite's own Windows VFS does declare it, because it opens files without
/// `FILE_SHARE_DELETE`; the Rust standard library opens them with it, so a file
/// here really can be unlinked while a handle is open. The conformance suite's
/// `vfs.device.claims-are-true` case caught the optimistic version of this
/// declaration, which is exactly what that case is for: a capability the
/// durability code takes a shortcut on has to be probed, not assumed.
pub fn device_characteristics() -> DeviceCharacteristics {
    DeviceCharacteristics::conservative()
}

/// Flushing a directory is not a Windows concept; NTFS journals the metadata
/// change that a create or delete makes, so there is nothing to force.
pub fn sync_directory(_path: &DbPath) -> VfsResult<()> {
    Ok(())
}

/// Flushes a file the strongest way this platform can.
///
/// `FlushFileBuffers`, which is what `sync_all` calls and which Windows
/// documents as reaching the disk. There is no second, stronger barrier the
/// way Darwin's `F_FULLFSYNC` is stronger than its `fsync` - see
/// `os::unix::full_sync`.
///
/// @param file - the file to flush
pub fn full_sync(file: &File) -> std::io::Result<()> {
    file.sync_all()
}

// `ProcessPrng` is the system generator's user mode entry point, the one the
// standard library itself calls for its hash seeds. It has no import library
// in the SDK, so it is linked the way the standard library links it.
#[link(name = "bcryptprimitives", kind = "raw-dylib")]
extern "system" {
    fn ProcessPrng(pbdata: *mut u8, cbdata: usize) -> i32;
}

/// Fills `output` with randomness from the system generator.
///
/// **`ProcessPrng`, not `BCryptGenRandom` (task-2191).** Both read the same
/// generator. `BCryptGenRandom` lives in `bcrypt.dll`, which a one row `exec`
/// loaded for nothing but the rollback journal's nonce: 23 more pages of a
/// library mapped into a process that runs one statement. `bcryptprimitives.dll`,
/// which holds `ProcessPrng`, is already loaded, because the standard library
/// seeds its hash maps from it. Windows documents that `ProcessPrng` always
/// returns TRUE, and the check below is kept for the day it does not.
///
/// @param output - where the bytes go
pub fn system_randomness(output: &mut [u8]) -> VfsResult<()> {
    if output.is_empty() {
        return Ok(());
    }
    // SAFETY: the buffer is valid for `output.len()` bytes for the duration of
    // the call, and `ProcessPrng` writes at most that many and keeps nothing.
    let ok = unsafe { ProcessPrng(output.as_mut_ptr(), output.len()) };
    if ok == 0 {
        return Err(error::misuse("ProcessPrng reported a failure"));
    }
    Ok(())
}

/// Builds the overlapped structure that carries a lock's start offset.
pub(crate) fn overlapped_at(offset: u64) -> OVERLAPPED {
    // SAFETY: OVERLAPPED is a plain data structure with no invalid bit patterns
    // for the fields the call reads; every field it reads is set here, and the
    // union is written rather than read, which is what makes zeroing sound.
    unsafe {
        let mut overlapped: OVERLAPPED = std::mem::zeroed();
        overlapped.Anonymous.Anonymous.Offset = (offset & 0xffff_ffff) as u32;
        overlapped.Anonymous.Anonymous.OffsetHigh = (offset >> 32) as u32;
        overlapped
    }
}

/// Tries to take a byte-range lock, returning false when another holder has it.
pub fn try_lock_bytes(
    file: &File,
    start: u64,
    len: u64,
    exclusive: bool,
    operation: VfsOperation,
) -> VfsResult<bool> {
    let mut overlapped = overlapped_at(start);
    let mut flags = LOCKFILE_FAIL_IMMEDIATELY;
    if exclusive {
        flags |= LOCKFILE_EXCLUSIVE_LOCK;
    }
    // SAFETY: the handle is valid for the life of `file`, and `overlapped`
    // lives across the call and is not aliased.
    let ok = unsafe {
        LockFileEx(
            file.as_raw_handle() as HANDLE,
            flags,
            0,
            (len & 0xffff_ffff) as u32,
            (len >> 32) as u32,
            &mut overlapped,
        )
    };
    if ok != 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        // ERROR_LOCK_VIOLATION and ERROR_IO_PENDING both mean "someone else has
        // it"; with FAIL_IMMEDIATELY the pending case is reported rather than
        // waited on.
        Some(33) | Some(997) => Ok(false),
        _ => Err(VfsError::from_io(operation, &error)),
    }
}

/// Releases a byte-range lock. Releasing a range that is not held is not an
/// error, because unlocking is used on paths that do not track every level.
pub fn unlock_bytes(file: &File, start: u64, len: u64, operation: VfsOperation) -> VfsResult<()> {
    let mut overlapped = overlapped_at(start);
    // SAFETY: as for `try_lock_bytes`.
    let ok = unsafe {
        UnlockFileEx(
            file.as_raw_handle() as HANDLE,
            0,
            (len & 0xffff_ffff) as u32,
            (len >> 32) as u32,
            &mut overlapped,
        )
    };
    if ok != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    match error.raw_os_error() {
        // ERROR_NOT_LOCKED
        Some(158) => Ok(()),
        _ => Err(VfsError::from_io(operation, &error)),
    }
}

/// Opens the shared-memory file for a database.
pub fn open_shm(path: &DbPath) -> VfsResult<Arc<dyn SharedMemory>> {
    FileShm::open(path)
}

/// A window onto a file that other processes see the same bytes of.
///
/// This is what makes the wal-index shared memory rather than a file two
/// processes happen to be reading: a store here is visible to every other
/// mapping of the same pages without a system call, which is what the WAL
/// protocol's barriers are ordering. It also sidesteps a Windows rule that
/// makes the file-I/O version impossible: a shared byte-range lock forbids
/// *writes* to the locked range even from the handle that took it, and the
/// wal-index deliberately stores a counter at the same byte the dead-man
/// switch is locked on. A mapped store is not a write in that sense, which is
/// exactly why SQLite maps this file too.
#[derive(Debug)]
pub struct SharedMapping {
    /// The address the view begins at, which may precede the caller's window
    /// because a view has to start on an allocation-granularity boundary.
    view: *mut u8,
    /// How many bytes the view covers, for unmapping and for bounds checks.
    view_len: usize,
    /// Where the caller's window starts within the view.
    offset: usize,
    /// How long the caller's window is.
    len: usize,
}

// SAFETY: the pointer is a mapping of a shared file view, which is valid for
// the life of this value on any thread; the type hands out no references to it
// and every access goes through the bounds-checked methods below.
unsafe impl Send for SharedMapping {}
// SAFETY: as above. Concurrent access is the point of shared memory, and the
// callers order their stores with the barrier the shared-memory contract
// provides rather than relying on Rust's aliasing rules, which do not describe
// memory another process is writing.
unsafe impl Sync for SharedMapping {}

impl SharedMapping {
    /// Copies bytes out of the window.
    pub fn read(&self, offset: usize, output: &mut [u8]) -> VfsResult<()> {
        let start = self.window(offset, output.len())?;
        // SAFETY: `window` has proved the range lies inside the mapped view,
        // the pointer is aligned for bytes, and `output` cannot overlap it
        // because it is a Rust-owned slice and this mapping hands out no
        // references into itself.
        unsafe {
            std::ptr::copy_nonoverlapping(self.view.add(start), output.as_mut_ptr(), output.len());
        }
        Ok(())
    }

    /// Copies bytes into the window.
    pub fn write(&self, offset: usize, input: &[u8]) -> VfsResult<()> {
        let start = self.window(offset, input.len())?;
        // SAFETY: as in `read`, with the direction reversed.
        unsafe {
            std::ptr::copy_nonoverlapping(input.as_ptr(), self.view.add(start), input.len());
        }
        Ok(())
    }

    /// Returns where a window of `len` bytes at `offset` starts in the view.
    fn window(&self, offset: usize, len: usize) -> VfsResult<usize> {
        let end = offset
            .checked_add(len)
            .ok_or_else(|| error::misuse("a shared-memory window overflowed"))?;
        if end > self.len {
            return Err(error::misuse(
                "a shared-memory access ran past the end of its region",
            ));
        }
        let start = self
            .offset
            .checked_add(offset)
            .filter(|start| start.saturating_add(len) <= self.view_len)
            .ok_or_else(|| error::misuse("a shared-memory window left its view"))?;
        Ok(start)
    }
}

impl Drop for SharedMapping {
    /// Releases the view.
    fn drop(&mut self) {
        // SAFETY: the pointer came from `MapViewOfFile` and has not been
        // unmapped before, because only this value owns it and it is dropped
        // once.
        unsafe {
            UnmapViewOfFile(
                windows_sys::Win32::System::Memory::MEMORY_MAPPED_VIEW_ADDRESS {
                    Value: self.view.cast(),
                },
            );
        }
    }
}

/// Maps `len` bytes of `file` starting at `offset` into this process.
///
/// The view is taken from the allocation-granularity boundary at or below
/// `offset`, because Windows refuses any other starting point, and the window
/// the caller asked for is recorded as an offset into it.
pub fn map_shared(file: &File, offset: u64, len: usize) -> VfsResult<SharedMapping> {
    let granularity = allocation_granularity();
    let aligned = offset - (offset % granularity);
    let delta = usize::try_from(offset - aligned)
        .map_err(|_| error::misuse("a shared-memory offset did not fit in memory"))?;
    let view_len = delta
        .checked_add(len)
        .ok_or_else(|| error::misuse("a shared-memory view overflowed"))?;
    // SAFETY: the handle is valid for the life of `file`, a null security
    // descriptor and a null name are documented as "default, unnamed", and a
    // zero size means "as large as the file", which the caller has already
    // grown to cover the region.
    let mapping = unsafe {
        CreateFileMappingW(
            file.as_raw_handle() as HANDLE,
            std::ptr::null(),
            PAGE_READWRITE,
            0,
            0,
            std::ptr::null(),
        )
    };
    if mapping.is_null() {
        return Err(VfsError::from_io(
            VfsOperation::ShmMap,
            &io::Error::last_os_error(),
        ));
    }
    // SAFETY: `mapping` is a valid mapping handle, the offset is aligned to the
    // allocation granularity as the call requires, and the length lies within
    // the file the mapping was made from.
    let view = unsafe {
        MapViewOfFile(
            mapping,
            FILE_MAP_READ | FILE_MAP_WRITE,
            (aligned >> 32) as u32,
            (aligned & 0xffff_ffff) as u32,
            view_len,
        )
    };
    // The mapping handle is not needed once a view exists: the view keeps the
    // mapping alive, and leaving the handle open would leak one per region.
    // SAFETY: `mapping` is a handle this function created and has not closed.
    unsafe {
        CloseHandle(mapping);
    }
    if view.Value.is_null() {
        return Err(VfsError::from_io(
            VfsOperation::ShmMap,
            &io::Error::last_os_error(),
        ));
    }
    Ok(SharedMapping {
        view: view.Value.cast(),
        view_len,
        offset: delta,
        len,
    })
}

/// Returns the boundary a mapped view has to start on.
fn allocation_granularity() -> u64 {
    // SAFETY: SYSTEM_INFO is plain data with no invalid bit patterns, and the
    // call fills every field this function reads.
    let mut info: SYSTEM_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is a properly aligned owned structure of the exact type
    // the call expects.
    unsafe { GetSystemInfo(&mut info) };
    u64::from(info.dwAllocationGranularity.max(1))
}

/// The lock level one handle holds, and the transitions between levels.
#[derive(Debug)]
pub struct LockState {
    level: Mutex<FileLock>,
    /// A copy of `level` for reading without the mutex, written whenever a transition ends.
    ///
    /// **Why** (task-2209). The engine asks a handle its level three or four times a statement
    /// (`set_reserved_writes`, `begin_read`, `wrote_anything`, `end_access_leased`), and each ask
    /// took the mutex: `lock_level` was 3.8% of an autocommit point read. A transition still runs
    /// under the mutex, so two transitions cannot interleave; a reader sees the level as it stood
    /// when the last one ended, which is all the mutex gave a reader too, since the level could
    /// change the moment the guard was dropped.
    seen: AtomicU8,
}

/// The mutex guard of a transition, which copies the level it leaves into `LockState::seen`
/// however the transition ends, an early error included.
struct Transition<'a> {
    level: std::sync::MutexGuard<'a, FileLock>,
    seen: &'a AtomicU8,
}

impl std::ops::Deref for Transition<'_> {
    type Target = FileLock;

    /// Reads the level under the mutex.
    fn deref(&self) -> &FileLock {
        &self.level
    }
}

impl std::ops::DerefMut for Transition<'_> {
    /// Writes the level under the mutex.
    fn deref_mut(&mut self) -> &mut FileLock {
        &mut self.level
    }
}

impl Drop for Transition<'_> {
    /// Publishes the level the transition left.
    fn drop(&mut self) {
        self.seen
            .store(level_number(*self.level), Ordering::Release);
    }
}

/// Returns a lock level as the number `LockState::seen` holds.
///
/// @param level - the level
fn level_number(level: FileLock) -> u8 {
    match level {
        FileLock::None => 0,
        FileLock::Shared => 1,
        FileLock::Reserved => 2,
        FileLock::Pending => 3,
        FileLock::Exclusive => 4,
    }
}

/// Returns the lock level `LockState::seen` holds as a number.
///
/// @param number - the number
fn level_of(number: u8) -> FileLock {
    match number {
        1 => FileLock::Shared,
        2 => FileLock::Reserved,
        3 => FileLock::Pending,
        4 => FileLock::Exclusive,
        _ => FileLock::None,
    }
}

impl LockState {
    /// Returns the level this handle holds.
    pub fn level(&self) -> FileLock {
        level_of(self.seen.load(Ordering::Acquire))
    }

    /// Takes the mutex for a transition whose end is copied into `seen`.
    fn transition(&self) -> Transition<'_> {
        Transition {
            level: guard(&self.level),
            seen: &self.seen,
        }
    }

    /// Raises the lock to `target`, one protocol step at a time.
    pub fn acquire(&self, file: &File, target: FileLock) -> VfsResult<()> {
        let mut level = self.transition();
        if target <= *level {
            return Ok(());
        }
        if *level == FileLock::None {
            take_shared(file)?;
            *level = FileLock::Shared;
        }
        if target == FileLock::Reserved && *level < FileLock::Reserved {
            if !try_lock_bytes(file, RESERVED_BYTE, 1, true, VfsOperation::Lock)? {
                return Err(error::busy("another connection holds RESERVED"));
            }
            *level = FileLock::Reserved;
        }
        if target >= FileLock::Pending && *level < FileLock::Pending {
            if !try_lock_bytes(file, PENDING_BYTE, 1, true, VfsOperation::Lock)? {
                return Err(error::busy("another connection holds PENDING"));
            }
            *level = FileLock::Pending;
        }
        if target == FileLock::Exclusive && *level < FileLock::Exclusive {
            promote_to_exclusive(file)?;
            *level = FileLock::Exclusive;
        }
        Ok(())
    }

    /// Lowers the lock to `target`, which must be `Shared` or `None`.
    ///
    /// **A handle at SHARED holds neither RESERVED nor PENDING, so releasing it
    /// unlocks one byte range and not three** (task-2046). RESERVED is taken
    /// only on the way to RESERVED, and `take_shared` gives PENDING back before
    /// it returns, so unlocking both on the way down from SHARED was two
    /// `UnlockFileEx` calls that answered `ERROR_NOT_LOCKED` and were swallowed.
    /// That is two of the six byte-range calls an ordinary statement made:
    /// under `locking_mode = normal` every statement outside a transaction
    /// takes the file and gives it back, and `leave` was 6.8 us of the 21.5 us
    /// such a statement cost once the meta record was no longer being reread.
    ///
    /// A handle above SHARED still unlocks both, whether or not it took them.
    /// `acquire` reaches EXCLUSIVE from SHARED without passing through
    /// RESERVED when a caller asks for it directly, so "the level says it is
    /// held" is not true there - and an unlock of a range nobody holds is the
    /// harmless call this is removing from the path where it is provably
    /// pointless, rather than a thing to reason about per level.
    pub fn release(&self, file: &File, target: FileLock) -> VfsResult<()> {
        let mut level = self.transition();
        if target >= *level {
            return Ok(());
        }
        // The line above has already returned for a target at or above the
        // level held, so a handle at SHARED that reaches here is going to NONE
        // and the read range is the one range it holds.
        if *level == FileLock::Shared {
            unlock_bytes(file, SHARED_FIRST, SHARED_SIZE, VfsOperation::Unlock)?;
            *level = target;
            return Ok(());
        }
        if *level == FileLock::Exclusive {
            unlock_bytes(file, SHARED_FIRST, SHARED_SIZE, VfsOperation::Unlock)?;
            if target == FileLock::Shared
                && !try_lock_bytes(file, SHARED_FIRST, SHARED_SIZE, false, VfsOperation::Unlock)?
            {
                return Err(error::busy(
                    "cannot retake the read lock while dropping down",
                ));
            }
        }
        unlock_bytes(file, RESERVED_BYTE, 1, VfsOperation::Unlock)?;
        unlock_bytes(file, PENDING_BYTE, 1, VfsOperation::Unlock)?;
        if target == FileLock::None && *level != FileLock::Exclusive {
            unlock_bytes(file, SHARED_FIRST, SHARED_SIZE, VfsOperation::Unlock)?;
        }
        *level = target;
        Ok(())
    }

    /// Reports whether another handle holds RESERVED or stronger.
    pub fn check_reserved(&self, file: &File) -> VfsResult<bool> {
        if *guard(&self.level) >= FileLock::Reserved {
            return Ok(false);
        }
        if try_lock_bytes(
            file,
            RESERVED_BYTE,
            1,
            true,
            VfsOperation::CheckReservedLock,
        )? {
            unlock_bytes(file, RESERVED_BYTE, 1, VfsOperation::CheckReservedLock)?;
            return Ok(false);
        }
        Ok(true)
    }
}

/// How many times a reader retries the PENDING byte before reporting BUSY.
///
/// Windows byte-range locks have no shared mode that two readers can take on
/// the same byte at once, so two connections acquiring SHARED at the same
/// moment collide on the serialising PENDING byte even though neither is a
/// writer. SQLite's own Windows VFS makes exactly three attempts with a
/// millisecond between them for this reason; reporting BUSY on the first
/// collision would turn ordinary read concurrency into a spurious failure.
const PENDING_ATTEMPTS: u32 = 3;

/// Takes the read lock, using the PENDING byte to serialise the two steps so
/// that a writer cannot slip in between them.
fn take_shared(file: &File) -> VfsResult<()> {
    let mut took_pending = false;
    for attempt in 0..PENDING_ATTEMPTS {
        if try_lock_bytes(file, PENDING_BYTE, 1, true, VfsOperation::Lock)? {
            took_pending = true;
            break;
        }
        if attempt + 1 < PENDING_ATTEMPTS {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    if !took_pending {
        return Err(error::busy("a writer holds PENDING"));
    }
    let took_read = try_lock_bytes(file, SHARED_FIRST, SHARED_SIZE, false, VfsOperation::Lock)?;
    unlock_bytes(file, PENDING_BYTE, 1, VfsOperation::Lock)?;
    if !took_read {
        return Err(error::busy("a writer holds the read range"));
    }
    Ok(())
}

/// Swaps the shared read lock for an exclusive one, putting the read lock back
/// if the swap fails so the caller keeps the level it had.
fn promote_to_exclusive(file: &File) -> VfsResult<()> {
    unlock_bytes(file, SHARED_FIRST, SHARED_SIZE, VfsOperation::Lock)?;
    if try_lock_bytes(file, SHARED_FIRST, SHARED_SIZE, true, VfsOperation::Lock)? {
        return Ok(());
    }
    let _ = try_lock_bytes(file, SHARED_FIRST, SHARED_SIZE, false, VfsOperation::Lock)?;
    Err(error::busy("readers are still present"))
}

/// Locks a mutex, recovering from poisoning rather than propagating a panic.
pub(crate) fn guard<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    match mutex.lock() {
        Ok(inner) => inner,
        Err(poisoned) => poisoned.into_inner(),
    }
}

#[cfg(test)]
mod randomness_tests {
    use super::system_randomness;

    /// The generator fills the whole buffer, and two calls do not agree.
    ///
    /// A binding that wrote nothing would leave zeros, and one that wrote the
    /// same bytes every time would give every journal the same nonce, which is
    /// the one thing the nonce exists to prevent.
    #[test]
    fn system_randomness_fills_the_buffer_and_differs_between_calls() {
        let mut first = [0u8; 64];
        let mut second = [0u8; 64];
        system_randomness(&mut first).unwrap();
        system_randomness(&mut second).unwrap();
        assert!(
            first.iter().any(|byte| *byte != 0),
            "the buffer is all zeros"
        );
        assert!(
            first[32..].iter().any(|byte| *byte != 0),
            "the second half was not written"
        );
        assert_ne!(first, second, "two calls returned the same bytes");
    }
}

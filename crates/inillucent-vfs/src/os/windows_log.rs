//! One log segment per process, and the oplock that proves nobody else has it open.
//!
//! Invariant: **while `LogShare::solo` is true, no handle to the segment exists outside this
//! process, and every write of this process went through the share**, so the length the share
//! keeps is the length on disk.
//!
//! **Why** (task-2209). A connection under `locking_mode = normal` asks the open log segment its
//! length before every statement, to learn whether another process appended a commit beside its
//! read. That was one `GetFileSizeEx` a statement, 53% of an autocommit point read once the read
//! lease had removed the lock calls (task-2197), and no cheaper kernel call answers the question:
//! a size query, a one byte read at the end, and a lock and unlock measured 0.8 to 1.6 us each on
//! the development machine.
//!
//! A batch oplock answers it without a call. While this process holds one on a segment, no other
//! handle to that segment exists anywhere on the machine, so the segment grows only through this
//! process's own writes. When another process opens the segment, the oplock breaks and the kernel
//! holds that process's `CreateFile` until this process acknowledges. The watcher clears `solo`
//! before it acknowledges, so a commit by the other process cannot reach the file before this
//! process has stopped trusting its own count. That needs nothing from the other process, so an
//! older build beside this one is covered too.
//!
//! Three things follow from the oplock's rules. Every open of a segment in this process shares one
//! file object, because a second `CreateFile` by this process breaks the oplock as surely as
//! another process's does; `open_log_share` hands out that object. The oplock needs the file
//! object opened for overlapped I/O, and the standard library's positional reads wait on the file
//! handle when a read is pending, which goes wrong when two threads use one object, so reads and
//! writes on a shared segment go through `overlapped_io` with an event per thread. And a process
//! stopped in a debugger while it holds the oplock holds up every other opener of that segment
//! until it is resumed or killed, so `INILLUCENT_LOG_OPLOCK=0` turns all of this off.

use std::fs::File;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Storage::FileSystem::{
    FileStandardInfo, GetFileInformationByHandleEx, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED,
    FILE_STANDARD_INFO,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject, INFINITE};
use windows_sys::Win32::System::IO::{DeviceIoControl, GetOverlappedResult, OVERLAPPED};

use super::platform::{file_len, guard, overlapped_at};

/// `FSCTL_REQUEST_BATCH_OPLOCK`, from `winioctl.h`; the `Win32_System_Ioctl` feature is not on.
const FSCTL_REQUEST_BATCH_OPLOCK: u32 = 0x0009_0008;
/// `FSCTL_OPLOCK_BREAK_ACK_NO_2`, from `winioctl.h`.
const FSCTL_OPLOCK_BREAK_ACK_NO_2: u32 = 0x0009_0050;
/// `ERROR_IO_PENDING`.
const ERROR_IO_PENDING: i32 = 997;
/// `ERROR_HANDLE_EOF`.
const ERROR_HANDLE_EOF: i32 = 38;
/// How long the watcher waits before asking for the oplock again after it was refused or broken.
const OPLOCK_RETRY: std::time::Duration = std::time::Duration::from_millis(100);

/// How many times a shared segment's length was asked of the kernel, for the tests that check
/// that the oplock removed the call.
static LOG_SIZE_QUERIES: AtomicU64 = AtomicU64::new(0);

/// Returns how many times a shared log segment's length was asked of the kernel in this process.
pub fn log_size_queries() -> u64 {
    LOG_SIZE_QUERIES.load(Ordering::Relaxed)
}

/// Reports whether log segments share one file object and an oplock, which
/// `INILLUCENT_LOG_OPLOCK=0` turns off.
pub fn log_sharing_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("INILLUCENT_LOG_OPLOCK").map_or(true, |value| value.trim() != "0")
    })
}

/// One log segment as every connection of this process holds it.
#[derive(Debug)]
pub struct LogShare {
    /// The one file object, opened for overlapped I/O.
    file: Arc<File>,
    /// Whether it was opened for writing; a writer cannot share a read only object.
    writable: bool,
    /// Whether this process holds the oplock, so no other handle to the segment exists.
    solo: AtomicBool,
    /// The segment's length as this process's own writes and truncations left it.
    len: AtomicU64,
}

impl LogShare {
    /// Returns the shared file.
    pub fn file(&self) -> &Arc<File> {
        &self.file
    }

    /// Returns the segment's length: the kept count while the oplock holds, the kernel's otherwise.
    pub fn len(&self) -> io::Result<u64> {
        if self.solo.load(Ordering::SeqCst) {
            return Ok(self.len.load(Ordering::SeqCst));
        }
        LOG_SIZE_QUERIES.fetch_add(1, Ordering::Relaxed);
        file_len(&self.file)
    }

    /// Records that the segment was cut or extended to `size`.
    ///
    /// Only a connection holding the database's write lock truncates a segment, so no write of
    /// this process runs beside it.
    ///
    /// @param size - the new length
    pub fn set_to(&self, size: u64) {
        self.len.store(size, Ordering::SeqCst);
    }

    /// Reads at an absolute offset into initialised memory; the end of the file is `Ok(0)`.
    ///
    /// @param offset - where to read from
    /// @param output - where the bytes go
    pub fn read_at(&self, offset: u64, output: &mut [u8]) -> io::Result<usize> {
        let wanted = u32::try_from(output.len()).unwrap_or(u32::MAX);
        let buffer = output.as_mut_ptr();
        overlapped_io(&self.file, offset, |handle, overlapped| {
            // SAFETY: `buffer` and `wanted` describe memory `output` owns until `overlapped_io`
            // has waited for the read to finish, and `overlapped` is that function's own.
            unsafe { ReadFile(handle, buffer, wanted, std::ptr::null_mut(), overlapped) }
        })
    }

    /// Reads at an absolute offset into memory that has not been initialised; `ReadFile` only
    /// writes to it.
    ///
    /// @param offset - where to read from
    /// @param output - where the bytes go
    pub fn read_at_spare(
        &self,
        offset: u64,
        output: &mut [std::mem::MaybeUninit<u8>],
    ) -> io::Result<usize> {
        let wanted = u32::try_from(output.len()).unwrap_or(u32::MAX);
        let buffer: *mut u8 = output.as_mut_ptr().cast();
        overlapped_io(&self.file, offset, |handle, overlapped| {
            // SAFETY: as for `read_at`.
            unsafe { ReadFile(handle, buffer, wanted, std::ptr::null_mut(), overlapped) }
        })
    }

    /// Writes at an absolute offset and records where the write ended.
    ///
    /// @param offset - where to write
    /// @param input - the bytes
    pub fn write_at(&self, offset: u64, input: &[u8]) -> io::Result<usize> {
        let wanted = u32::try_from(input.len()).unwrap_or(u32::MAX);
        let source = input.as_ptr();
        let written = overlapped_io(&self.file, offset, |handle, overlapped| {
            // SAFETY: `source` and `wanted` describe memory `input` owns until `overlapped_io` has
            // waited for the write to finish, and `WriteFile` only reads it.
            unsafe { WriteFile(handle, source, wanted, std::ptr::null_mut(), overlapped) }
        })?;
        self.len
            .fetch_max(offset.saturating_add(written as u64), Ordering::SeqCst);
        Ok(written)
    }
}

/// An event handle that is closed when dropped.
struct Event(HANDLE);

impl Event {
    /// Makes a manual reset event that starts unset, or an empty one when the system refuses.
    fn new() -> Event {
        // SAFETY: no security attributes and no name.
        Event(unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) })
    }
}

impl Drop for Event {
    /// Closes the event.
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the handle came from `CreateEventW` for this value and is closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

thread_local! {
    /// This thread's event for overlapped I/O, made on first use.
    static IO_EVENT: Event = Event::new();
}

/// Starts one read, write or control request on a file opened for overlapped I/O and waits for it.
///
/// The event is this thread's own, so a completion of another thread's I/O cannot end this wait,
/// which is what goes wrong when the standard library waits on the file handle instead. The end
/// of the file is `Ok(0)`.
///
/// @param file - the shared file
/// @param offset - where the operation starts
/// @param start - the call that starts it, given the handle and the `OVERLAPPED`
fn overlapped_io(
    file: &File,
    offset: u64,
    start: impl FnOnce(HANDLE, *mut OVERLAPPED) -> i32,
) -> io::Result<usize> {
    let event = IO_EVENT.with(|event| event.0);
    if event.is_null() {
        return Err(io::Error::other("no event for overlapped I/O"));
    }
    let handle = file.as_raw_handle() as HANDLE;
    let mut overlapped = overlapped_at(offset);
    overlapped.hEvent = event;
    if start(handle, &mut overlapped) == 0 {
        let failure = io::Error::last_os_error();
        match failure.raw_os_error() {
            Some(ERROR_IO_PENDING) => {}
            Some(ERROR_HANDLE_EOF) => return Ok(0),
            _ => return Err(failure),
        }
    }
    let mut transferred: u32 = 0;
    // SAFETY: `overlapped` is the structure the operation was started with and lives until this
    // returns, which is after the wait; the handle is valid for the life of `file`.
    let done = unsafe { GetOverlappedResult(handle, &overlapped, &mut transferred, 1) };
    if done == 0 {
        let failure = io::Error::last_os_error();
        if failure.raw_os_error() == Some(ERROR_HANDLE_EOF) {
            return Ok(0);
        }
        return Err(failure);
    }
    Ok(transferred as usize)
}

/// The shared segments of this process, by lowercased absolute path.
type Registry = Mutex<Vec<(String, Weak<LogShare>)>>;

/// Returns the registry of shared segments.
fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

/// Reports whether a shared segment's file has been deleted, so its name now belongs to another
/// file or to none.
///
/// Windows removes a deleted file's name at once while handles to it stay open, so a segment
/// deleted and created again under the same name is a different file. A share that outlived the
/// delete in a connection nobody closed must not be handed to an open of the new segment: the
/// opener would write its log into the deleted file, and its commits would be gone at the next
/// open. `walperf` found it: its recovery measurement leaks one database, deletes its files, opens
/// the same path again, and the reopen read `no such table: t`. An open of a log segment is rare,
/// so one query of the handle's standard information on each costs nothing measurable.
///
/// @param share - the segment found in the registry
fn deleted(share: &LogShare) -> bool {
    let mut information = FILE_STANDARD_INFO {
        AllocationSize: 0,
        EndOfFile: 0,
        NumberOfLinks: 0,
        DeletePending: 0,
        Directory: 0,
    };
    let handle = share.file.as_raw_handle() as HANDLE;
    // SAFETY: `information` is a `FILE_STANDARD_INFO` this frame owns, its size is passed with it,
    // and the handle is valid for the life of `share`.
    let answered = unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileStandardInfo,
            std::ptr::addr_of_mut!(information).cast(),
            std::mem::size_of::<FILE_STANDARD_INFO>() as u32,
        )
    };
    // A handle that cannot answer is not trusted either: the ordinary open is always correct.
    answered == 0 || information.DeletePending != 0 || information.NumberOfLinks == 0
}

/// Opens a log segment through this process's shared file object, making it on the first open,
/// or answers `None` when the segment is to be opened the ordinary way.
///
/// `None` when sharing is turned off, when the open must create the file new (the ordinary open
/// then reports a file that is already there), and when a writer finds only a read only object.
/// An ordinary open breaks the oplock, which is correct and only slower.
///
/// @param path - the segment
/// @param writable - whether the caller writes
/// @param create - whether to create the file when it is missing
/// @param exclusive - whether the file must not exist yet
pub fn open_log_share(
    path: &std::path::Path,
    writable: bool,
    create: bool,
    exclusive: bool,
) -> io::Result<Option<Arc<LogShare>>> {
    if !log_sharing_enabled() || exclusive {
        return Ok(None);
    }
    let key = std::path::absolute(path)?.to_string_lossy().to_lowercase();
    let mut shares = guard(registry());
    shares.retain(|(_, weak)| weak.strong_count() > 0);
    if let Some(share) = shares
        .iter()
        .find(|(held, _)| *held == key)
        .and_then(|(_, weak)| weak.upgrade())
    {
        if deleted(&share) {
            // The old share keeps serving the connections that hold it; this name gets a new one.
            shares.retain(|(held, _)| *held != key);
        } else if writable && !share.writable {
            return Ok(None);
        } else {
            return Ok(Some(share));
        }
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    if writable {
        options.write(true).create(create);
    }
    std::os::windows::fs::OpenOptionsExt::custom_flags(&mut options, FILE_FLAG_OVERLAPPED);
    let share = Arc::new(LogShare {
        file: Arc::new(options.open(path)?),
        writable,
        solo: AtomicBool::new(false),
        len: AtomicU64::new(0),
    });
    shares.push((key, Arc::downgrade(&share)));
    drop(shares);
    let watched = Arc::downgrade(&share);
    // A watcher that could not start leaves `solo` false, which is the behaviour before this.
    drop(
        std::thread::Builder::new()
            .name("inillucent-log-oplock".into())
            .spawn(move || watch_oplock(&watched)),
    );
    Ok(Some(share))
}

/// The watcher of one shared segment: asks for the oplock, keeps `solo` true while it holds, and
/// acknowledges a break only after clearing `solo`, until the segment's last handle closes.
///
/// It holds the segment by a weak reference while it waits, so a segment nobody uses is closed,
/// and the close ends the pending request, which ends this loop.
///
/// @param watched - the segment
fn watch_oplock(watched: &Weak<LogShare>) {
    let event = Event::new();
    if event.0.is_null() {
        return;
    }
    // Boxed so its address stays put while the kernel holds the request, and owned here rather
    // than by the share so it outlives the handle's close.
    let mut overlapped: Box<OVERLAPPED> = Box::new(overlapped_at(0));
    loop {
        let Some(share) = watched.upgrade() else {
            return;
        };
        *overlapped = overlapped_at(0);
        overlapped.hEvent = event.0;
        let handle = share.file.as_raw_handle() as HANDLE;
        // SAFETY: the request takes no buffers, and `overlapped` outlives it: this loop waits for
        // the event before it reuses or drops the structure.
        let requested = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_REQUEST_BATCH_OPLOCK,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut *overlapped,
            )
        };
        let granted =
            requested == 0 && io::Error::last_os_error().raw_os_error() == Some(ERROR_IO_PENDING);
        if !granted {
            drop(share);
            std::thread::sleep(OPLOCK_RETRY);
            continue;
        }
        hold(&share);
        drop(share);
        // SAFETY: the event is this thread's own and valid until `event` drops.
        unsafe { WaitForSingleObject(event.0, INFINITE) };
        // The request ended: another open broke the oplock, or the last handle closed.
        let Some(share) = watched.upgrade() else {
            return;
        };
        share.solo.store(false, Ordering::SeqCst);
        acknowledge_break(&share.file);
        drop(share);
        std::thread::sleep(OPLOCK_RETRY);
    }
}

/// Seeds the kept length from the kernel and sets `solo`, once the oplock is granted.
///
/// No handle outside this process exists from the grant on, so the kernel's length now is the
/// count to keep. A write of this process racing this seed records its own end with `fetch_max`,
/// so whichever lands last, the larger length stands.
///
/// @param share - the segment
fn hold(share: &LogShare) {
    LOG_SIZE_QUERIES.fetch_add(1, Ordering::Relaxed);
    if let Ok(size) = file_len(&share.file) {
        share.len.fetch_max(size, Ordering::SeqCst);
        share.solo.store(true, Ordering::SeqCst);
    }
}

/// Acknowledges a broken batch oplock, giving it up, so the open that broke it may proceed.
///
/// A failure is ignored: a break nobody acknowledges is acknowledged by the handle's close, and
/// the opener waits for one or the other.
///
/// @param file - the shared segment
fn acknowledge_break(file: &File) {
    let _ = overlapped_io(file, 0, |handle, overlapped| {
        // SAFETY: the request takes no buffers, and `overlapped_io` waits for it before its
        // `OVERLAPPED` goes out of scope.
        unsafe {
            DeviceIoControl(
                handle,
                FSCTL_OPLOCK_BREAK_ACK_NO_2,
                std::ptr::null(),
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                overlapped,
            )
        }
    });
}

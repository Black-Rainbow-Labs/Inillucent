//! The shared lock a statement that only read keeps for the next one.
//!
//! What this file checks (task-2197): a lease holds SHARED against another
//! handle, the releasing thread lets an idle lease go, a writer in the same
//! process takes the lease away without waiting, and a connection whose lease
//! was let go takes the lock again the ordinary way.
//!
//! Invariant: **every test opens its own file in its own directory**, so a
//! lease one test leaves armed can never stand in another test's way.

use std::sync::Arc;
use std::time::{Duration, Instant};

use inillucent_pool::{Database, Options};
use inillucent_vfs::OsVfs;
use inillucent_vfs::{DbPath, FileLock, OpenOptions, Vfs};

/// Makes a fresh database file in a directory of its own and returns its path.
///
/// @param name - the test's name, which names the directory
fn fresh_file(name: &str) -> DbPath {
    let directory =
        std::env::temp_dir().join(format!("inillucent-lease-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    let path = DbPath::new(directory.join("lease.rdb").to_string_lossy().as_ref());
    let vfs = OsVfs::new();
    let mut created = Database::create(&vfs, &path, Options::default()).expect("a new file");
    created.checkpoint().expect("a checkpoint");
    drop(created);
    path
}

/// Opens a second, raw handle on the file, the way another process would hold it.
///
/// @param vfs - the file system
/// @param path - the file
fn raw_handle(vfs: &OsVfs, path: &DbPath) -> Box<dyn inillucent_vfs::VfsFile> {
    vfs.open(path, OpenOptions::main_db())
        .expect("a second handle")
}

/// Waits until a raw handle can take EXCLUSIVE, up to a limit, and reports how
/// long that took.
///
/// @param raw - the handle
/// @param limit - how long to try
fn exclusive_within(raw: &dyn inillucent_vfs::VfsFile, limit: Duration) -> Option<Duration> {
    let started = Instant::now();
    while started.elapsed() < limit {
        if raw.lock(FileLock::Exclusive).is_ok() {
            return Some(started.elapsed());
        }
        std::thread::sleep(Duration::from_micros(200));
    }
    None
}

/// A lease holds SHARED against another handle, and the releasing thread lets
/// it go once nothing has used it for `IDLE`.
#[test]
fn a_lease_holds_the_file_until_it_is_idle() {
    let path = fresh_file("idle");
    let vfs = Arc::new(OsVfs::new());
    let mut reader = Database::open(vfs.as_ref(), &path, 64).expect("the reader");
    // Opened before the lease, so the check below comes within the lease's
    // millisecond on a loaded machine too.
    let raw = raw_handle(&vfs, &path);
    reader.begin_read().expect("a read lock");
    let armed = Instant::now();
    reader.end_access_leased().expect("a lease");
    let took = raw.lock(FileLock::Exclusive).is_ok();
    // **A lock taken after the lease's own time has passed is the lease
    // ending as it should**, so only a lock taken inside `IDLE` is a failure.
    assert!(
        !took || armed.elapsed() >= inillucent_pool::lease::IDLE,
        "another handle took EXCLUSIVE while the lease held SHARED"
    );
    if took {
        raw.unlock(FileLock::None).expect("an unlock");
    }
    let waited = exclusive_within(raw.as_ref(), Duration::from_secs(2));
    assert!(
        waited.is_some(),
        "the releasing thread never let an idle lease go"
    );
    raw.unlock(FileLock::None).expect("an unlock");
    // The lease ended by letting go, so the next statement takes the lock the
    // ordinary way and reads the meta record again.
    reader.begin_read().expect("the lock taken again");
    assert_eq!(reader.lock_level(), FileLock::Shared);
    reader.end_access().expect("a release");
}

/// A writer in the same process with no patience at all still gets the file,
/// because a refused raise lets the leases of this process go first.
#[test]
fn a_writer_in_the_same_process_takes_a_lease_away() {
    let path = fresh_file("yield");
    let vfs = Arc::new(OsVfs::new());
    let mut reader = Database::open(vfs.as_ref(), &path, 64).expect("the reader");
    let mut writer = Database::open(vfs.as_ref(), &path, 64).expect("the writer");
    writer.set_busy_millis(0);
    reader.begin_read().expect("a read lock");
    reader.end_access_leased().expect("a lease");
    writer
        .begin_write()
        .expect("the writer took the file without waiting for the lease");
    assert_eq!(writer.lock_level(), FileLock::Exclusive);
    writer.end_access().expect("the writer let go");
    reader
        .begin_read()
        .expect("the reader takes the lock again");
    reader.end_access().expect("the reader let go");
}

/// A statement that follows within the lease takes no lock: the lock it holds
/// is the one the lease kept, and no other handle got in between.
#[test]
fn the_next_statement_claims_the_lease() {
    let path = fresh_file("claim");
    let vfs = Arc::new(OsVfs::new());
    let mut reader = Database::open(vfs.as_ref(), &path, 64).expect("the reader");
    reader.begin_read().expect("a read lock");
    reader.end_access_leased().expect("a lease");
    assert!(
        !reader.begin_read().expect("a claim"),
        "a claimed lease threw the cache away"
    );
    let raw = raw_handle(&vfs, &path);
    assert!(
        raw.lock(FileLock::Exclusive).is_err(),
        "a claimed lease was let go behind the statement holding it"
    );
    reader.end_access().expect("a release");
    assert!(
        raw.lock(FileLock::Exclusive).is_ok(),
        "the release let nothing go"
    );
    raw.unlock(FileLock::None).expect("an unlock");
}

/// Asking which lock a connection holds, between statements, leaves the lease
/// to end by itself: only a statement claims it.
#[test]
fn asking_about_the_lock_does_not_keep_the_lease() {
    let path = fresh_file("ask");
    let vfs = Arc::new(OsVfs::new());
    let mut reader = Database::open(vfs.as_ref(), &path, 64).expect("the reader");
    reader.begin_read().expect("a read lock");
    reader.end_access_leased().expect("a lease");
    assert_eq!(
        reader.lock_level(),
        FileLock::Shared,
        "a lease holds SHARED"
    );
    let _ = reader.pool().file();
    let raw = raw_handle(&vfs, &path);
    assert!(
        exclusive_within(raw.as_ref(), Duration::from_secs(2)).is_some(),
        "asking about the lock kept the lease from ending"
    );
    raw.unlock(FileLock::None).expect("an unlock");
}

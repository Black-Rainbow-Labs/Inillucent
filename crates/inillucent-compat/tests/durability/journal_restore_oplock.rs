//! A rollback journal put back by another process is seen by a reader that holds the log
//! segment's oplock.
//!
//! Invariant: **a statement that starts after another process has put a hot journal back reads
//! the meta record, whether or not the reader held the oplock on its log segment.** On Windows a
//! reader holding the oplock skips that read, because no other process can have folded without
//! opening the segment first. A restore changes the record and writes no log, so the process
//! doing it has to open the segments itself (`replay_hot_journal_holding`). Before it did, a
//! reader that finished its open just before a restore read the pages from before it in its next
//! statement: 2 of 448 process storms with a 64 page pool.
//!
//! The restoring process here is a child of this test binary, so it is a separate process with
//! its own file objects, which is the only way to break the oplock the reader holds.

#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
use inillucent_compat::workspace_root;
#[cfg(windows)]
use inillucent_engine::ImportedDatabase;
#[cfg(windows)]
use inillucent_exec::physical::Params;
#[cfg(windows)]
use inillucent_pool::journal::{Journal, JournalMode};
#[cfg(windows)]
use inillucent_pool::page::PageId;
#[cfg(windows)]
use inillucent_vfs::{DbPath, OsVfs, Vfs};

/// The page size the reader's file is built at.
#[cfg(windows)]
const PAGE_SIZE: usize = 4_096;

/// The variable that makes [`the_restoring_process`] restore the journal beside this database.
#[cfg(windows)]
const CHILD_DATABASE: &str = "JOURNAL_RESTORE_CHILD_DATABASE";

/// A scratch directory of this case's own, emptied first.
#[cfg(windows)]
fn area() -> PathBuf {
    let path = workspace_root().join("_agent_output/journal-restore-oplock");
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs one statement that must succeed.
///
/// @param engine - the open connection
/// @param sql - the statement
#[cfg(windows)]
fn run(engine: &mut ImportedDatabase, sql: &str) {
    engine
        .execute_any(sql, &Params::new())
        .unwrap_or_else(|why| panic!("{sql}: {}", why.message()));
}

/// Waits until a hundred reads make no probe of the meta record, which is what holding the
/// oplock looks like from outside, and reports whether that happened within five seconds.
///
/// @param engine - the reader
#[cfg(windows)]
fn becomes_quiet(engine: &mut ImportedDatabase) -> bool {
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(5) {
        let before = engine.pool_stats().meta_probes;
        for _ in 0..100 {
            run(engine, "SELECT n FROM note WHERE n = 1");
        }
        if engine.pool_stats().meta_probes == before {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

/// Leaves a sealed journal beside the file that holds the pre-image of one data page, as a
/// writer killed in the middle of a fold leaves it.
///
/// The pre-image is the page as it is, so the restore changes no row; what it changes is the
/// meta record's generation, which is the part a reader has to notice.
///
/// @param database - the database file
#[cfg(windows)]
fn leave_a_hot_journal(database: &std::path::Path) {
    let bytes = std::fs::read(database).expect("the file reads");
    let page = bytes
        .get(2 * PAGE_SIZE..3 * PAGE_SIZE)
        .expect("the file has a page 2");
    let vfs: std::sync::Arc<dyn Vfs> = std::sync::Arc::new(OsVfs::new());
    let path = DbPath::new(database.to_string_lossy().as_ref());
    let mut journal = Journal::new(vfs, &path, JournalMode::Delete, PAGE_SIZE);
    journal
        .save(PageId(2), page)
        .expect("the pre-image is saved");
    journal.seal().expect("the journal is sealed");
}

/// The restoring process: puts back the journal beside the database named by
/// `JOURNAL_RESTORE_CHILD_DATABASE`, the way an engine does, and does nothing else.
///
/// It is a test only so the test binary can start itself. Run without the variable it passes
/// at once.
#[test]
fn the_restoring_process() {
    #[cfg(windows)]
    if let Some(database) = std::env::var_os(CHILD_DATABASE) {
        let path = DbPath::new(PathBuf::from(database).to_string_lossy().as_ref());
        let restored =
            inillucent_engine::recovery::replay_hot_journal_holding_the_log(&OsVfs::new(), &path)
                .expect("the journal is put back");
        assert!(restored, "the journal held a page to put back");
    }
}

/// A reader that holds the oplock reads the meta record on its next statement after another
/// process put a journal back.
#[cfg(windows)]
#[test]
fn a_restore_by_another_process_is_seen_by_a_reader_holding_the_oplock() {
    let directory = area();
    let database = directory.join("shared.rdb");
    let mut reader = ImportedDatabase::create(database.clone(), PAGE_SIZE, 256)
        .expect("the reader creates the file");
    run(&mut reader, "CREATE TABLE note (n INTEGER PRIMARY KEY)");
    run(
        &mut reader,
        "INSERT INTO note SELECT value FROM generate_series(1, 200)",
    );
    reader.checkpoint().expect("the file is folded");
    assert!(
        becomes_quiet(&mut reader),
        "the reader never stopped reading the meta record, so this test would not test the skip"
    );

    leave_a_hot_journal(&database);
    let ran = std::process::Command::new(std::env::current_exe().expect("this binary's path"))
        .args([
            "--exact",
            "journal_restore_oplock::the_restoring_process",
            "--nocapture",
        ])
        .env(CHILD_DATABASE, &database)
        .output()
        .expect("the restoring process starts");
    assert!(
        ran.status.success(),
        "the restoring process failed:\n{}\n{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );

    let before = reader.pool_stats();
    run(&mut reader, "SELECT n FROM note WHERE n = 1");
    let after = reader.pool_stats();
    assert!(
        after.meta_probes > before.meta_probes,
        "the first statement after another process put a journal back did not look at the meta \
         record, so it read through a cache the restore had made stale"
    );
    assert!(
        after.meta_reads > before.meta_reads,
        "the reader saw the record change and did not read it"
    );
}

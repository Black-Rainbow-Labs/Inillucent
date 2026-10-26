//! The one way a test says it did not run.
//!
//! Invariant: a suite that skips does it through [`skipping`], so exactly one
//! phrase reaches the classifier and exactly one lever turns a skip into a
//! failure. A suite that prints its own sentence is invisible to
//! `inillucent-testrun --strict`, and a suite that returns without printing is
//! invisible to everything.
//!
//! It also holds [`remove_database`], the one way a test clears a scratch
//! database before reusing its path, because a database here is a file plus
//! numbered log segments and every copy of that list elsewhere forgot the
//! segments.
//!
//! And it holds [`ScratchDir`], the one way a test makes a folder in the
//! system temporary directory, because a folder that nothing removes stays
//! there for good. On 2026-10-25 `%TEMP%` on the development machine held
//! 105,562 `inillucent-*` folders left by this workspace's tests, and every
//! program that lists that directory paid for each of them.
//!
//! **Why this is in the bottom crate rather than in the test harness
//! (task-1969, 4.6).** `inillucent_compat::differential::skipping` was the
//! original home, and three crates could not reach it: the layering contract
//! refuses a production crate a dependency on the harness, even as a
//! dev-dependency, because that puts harness code in the engine's test surface.
//! So `inillucent-driver`, `inillucent-cli` and `inillucent-tree` each printed
//! their own `eprintln!`, `inillucent-remote` kept a private copy of this
//! function, and all four were dropped by `testrun.rs`'s classifier because the
//! phrase alone is not the signal it reads. `inillucent-base` is below every one
//! of them, so the helper reaches them all without an upward edge, and the
//! compat helper now delegates here rather than owning a second copy.
//!
//! The module is behind `#[cfg(any(test, feature = "testing"))]` so a shipped
//! build of `inillucent-base` does not carry it. Each consumer turns the feature
//! on from its own `[dev-dependencies]`, which is on for that crate's test
//! targets and off everywhere else.

// **This module may panic, and the crate-level deny is why the allow is here.**
// Panicking is the function's purpose: under `INILLUCENT_STRICT` a skip has to
// fail the test that skipped, because that is what names the case rather than
// the binary. The bans at the crate root exist to keep panics out of paths that
// read persistent bytes, and this path reads an environment variable.
#![allow(clippy::panic)]

/// What a strict run's skip panic says after the marker.
///
/// **A skip and a failure are different things and the report has to keep them
/// apart (task-1932, H10).** `--strict` makes a skip fail the test, which is
/// what names the case rather than the binary - but a suite that skipped did
/// not evidence a problem, it evidenced nothing, and listing it under FAILED
/// would put a second wrong label on the same event. `inillucent-testrun`'s
/// `missing_prerequisites` reads this sentinel to tell one from the other: a
/// target whose every failure carries it is hollow, and a target with even one
/// failure that does not is a failure.
pub const STRICT_SKIP: &str = " - and this run is strict, so a skip is a failure";

/// The marker every skip message ends with.
///
/// `tests/inillucent-testing-tdd.md` §9 asks for one phrase and
/// `inillucent-testrun`'s classifier matches this one. It is a constant so that
/// a test can assert on it rather than on a literal typed twice.
pub const MARKER: &str = "; skipping";

/// Says why a case did not run, and fails the case when the run is strict.
///
/// **One marker and one decision, in one place (task-1932, H10; task-1969,
/// 4.6).** Every skip site in the workspace ends its message with `; skipping`,
/// which is what `testrun`'s classifier matches. Before task-1932 there were
/// three phrasings and a list of six substrings trying to catch them; before
/// task-1969 there were four crates that could not reach the helper at all and
/// printed the phrase without the panic, so `--strict` dropped them.
///
/// `INILLUCENT_STRICT` makes the skip a failure of the test rather than a
/// classification of the binary. `inillucent-testrun --strict` sets it, and the
/// panic then names the test and the thing that is missing. That matters most
/// in a binary that runs other tests, because a skip of one case there was
/// invisible to `--strict` by both routes.
///
/// @param reason - what is missing, without the marker
pub fn skipping(reason: &str) {
    if std::env::var("INILLUCENT_STRICT").is_ok_and(|value| !value.is_empty()) {
        panic!("{reason}{MARKER}{STRICT_SKIP}");
    }
    eprintln!("{reason}{MARKER}");
}

/// Removes a database file and every file the engine or SQLite keeps beside it.
///
/// **The log is not one file (task-2110, bugs 1 and 7).** The engine writes its
/// log as numbered segments, `<name>-wal.0000000001` and on, and the helpers
/// this replaces removed `<name>-wal` and nothing else. A test that reuses a
/// scratch path then opened a new database beside the last run's segments, and
/// the engine replayed them into it: `new_engine_surface` failed with "table t
/// already exists" after any run of it that had been stopped partway, which
/// read as a defect in whatever the branch had changed. The segments are found
/// by listing the directory because their numbers are not predictable - a chain
/// can start anywhere once a checkpoint has removed its early segments.
///
/// Every removal ignores a missing file, so this is also what a test calls
/// before it creates a database.
///
/// @param path - the database file
pub fn remove_database(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    for suffix in ["-wal", "-journal", "-shm"] {
        let _ = std::fs::remove_file(path.with_file_name(format!("{name}{suffix}")));
    }
    let directory = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => std::path::Path::new("."),
    };
    let Ok(listing) = std::fs::read_dir(directory) else {
        return;
    };
    let prefix = format!("{name}-wal.");
    for entry in listing.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_some_and(|file| file.starts_with(&prefix))
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// A fresh folder in the system temporary directory, removed when the guard is
/// dropped.
///
/// **Every test that makes a folder in `%TEMP%` makes it through this.** The
/// helpers it replaced each built a unique name from the process id, a tag and
/// the thread id, created the folder, and returned a bare `PathBuf`, so every
/// run of every suite added folders and nothing ever took one away.
/// `inillucent-testrun` now points `TMP`, `TEMP` and `TMPDIR` at a folder of
/// its own and removes it when the run ends, but a plain `cargo test` has no
/// runner around it, and this guard is what makes that leave nothing either.
///
/// **A failing test keeps its folder.** The drop does nothing while the
/// thread is panicking, so the files a failed assertion was about are still
/// there to read. Under the runner they go with the run's folder.
///
/// **Keep the guard alive for as long as anything uses a path inside it.** It
/// dereferences to the folder's [`Path`], so `scratch.join("a.rdb")` reads the
/// way the old `PathBuf` did. A helper that returns only a path inside the
/// folder drops the guard on return and removes the folder under the caller,
/// so such a helper returns the guard alongside the path. Locals drop in the
/// reverse of the order they were declared, so declaring the guard before the
/// database that lives in it closes the database before the folder goes.
#[must_use = "the folder is removed as soon as the guard is dropped"]
#[derive(Debug)]
pub struct ScratchDir {
    path: std::path::PathBuf,
}

impl ScratchDir {
    /// Makes an empty folder called `name` in the system temporary directory.
    ///
    /// A folder left at that name by an earlier run is removed first, so the
    /// test starts from nothing. `name` should already be unique to the test
    /// and the process: the callers put the process id and the thread id in it,
    /// because tests in one binary run in parallel.
    ///
    /// @param name - the folder's name, without a directory
    pub fn new(name: &str) -> ScratchDir {
        ScratchDir::at(std::env::temp_dir().join(name))
    }

    /// Makes an empty folder at `path` and removes it when the guard drops.
    ///
    /// For a test whose folder lives somewhere other than directly in the
    /// temporary directory. Anything already at `path` is removed first.
    ///
    /// @param path - where the folder goes
    pub fn at(path: std::path::PathBuf) -> ScratchDir {
        match ScratchDir::try_at(path.clone()) {
            Ok(scratch) => scratch,
            Err(error) => panic!(
                "the scratch folder {} was not made: {error}",
                path.display()
            ),
        }
    }

    /// Makes an empty folder at `path`, or says why it could not.
    ///
    /// For a measurement program that reports a folder it cannot make as an
    /// error of its own rather than a panic.
    ///
    /// @param path - where the folder goes
    pub fn try_at(path: std::path::PathBuf) -> std::io::Result<ScratchDir> {
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)?;
        Ok(ScratchDir { path })
    }

    /// Returns the folder.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Gives up the guard and keeps the folder, returning where it is.
    ///
    /// For a program that keeps its files only when they are evidence, such
    /// as a gate whose two databases disagreed.
    pub fn keep(self) -> std::path::PathBuf {
        let mut kept = std::mem::ManuallyDrop::new(self);
        std::mem::take(&mut kept.path)
    }
}

impl std::ops::Deref for ScratchDir {
    type Target = std::path::Path;

    /// Returns the folder, so the guard reads like the `PathBuf` it replaced.
    fn deref(&self) -> &std::path::Path {
        &self.path
    }
}

impl AsRef<std::path::Path> for ScratchDir {
    /// Returns the folder, for functions that take `impl AsRef<Path>`.
    fn as_ref(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for ScratchDir {
    /// Removes the folder, unless the thread is panicking.
    ///
    /// The removal ignores errors. On Windows a file that is still open cannot
    /// be removed, and a test that leaked a handle should not fail for it in
    /// its cleanup.
    fn drop(&mut self) {
        if !std::thread::panicking() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The folder exists while the guard is alive and is gone after it drops,
    /// files and all, and a second guard at the same name starts empty.
    #[test]
    fn a_scratch_folder_is_removed_when_its_guard_drops() {
        let name = format!("inillucent-scratch-guard-{}", std::process::id());
        let kept;
        {
            let scratch = ScratchDir::new(&name);
            kept = scratch.to_path_buf();
            std::fs::create_dir_all(scratch.join("inner")).expect("a nested folder");
            std::fs::write(scratch.join("inner/a.rdb"), b"x").expect("a file");
            assert!(kept.join("inner/a.rdb").is_file());
            let again = ScratchDir::new(&format!("{name}-second"));
            assert_eq!(
                std::fs::read_dir(again.path()).expect("it lists").count(),
                0
            );
        }
        assert!(!kept.exists(), "{} was left behind", kept.display());
        assert!(!std::env::temp_dir().join(format!("{name}-second")).exists());
    }

    /// A folder left by an earlier run at the same name is emptied, so a test
    /// never reads the last run's files.
    #[test]
    fn a_scratch_folder_starts_empty_over_a_leftover() {
        let path = std::env::temp_dir().join(format!(
            "inillucent-scratch-leftover-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).expect("the leftover folder");
        std::fs::write(path.join("stale.rdb"), b"x").expect("a stale file");
        let scratch = ScratchDir::at(path.clone());
        assert_eq!(
            std::fs::read_dir(scratch.path()).expect("it lists").count(),
            0
        );
        drop(scratch);
        assert!(!path.exists());
    }

    /// A test that fails keeps its folder, so the files it was about can be
    /// read afterwards.
    #[test]
    fn a_failing_test_keeps_its_scratch_folder() {
        let path =
            std::env::temp_dir().join(format!("inillucent-scratch-panics-{}", std::process::id()));
        let inside = path.clone();
        let outcome = std::thread::spawn(move || {
            let _scratch = ScratchDir::at(inside);
            panic!("the test failed");
        })
        .join();
        assert!(outcome.is_err());
        assert!(path.is_dir(), "a failed test's folder was removed");
        std::fs::remove_dir_all(&path).expect("cleanup");
    }

    /// Every numbered segment goes with the database, and a database whose
    /// name is a prefix of this one's is left alone.
    #[test]
    fn a_removed_database_takes_its_segments_with_it() {
        let directory =
            std::env::temp_dir().join(format!("inillucent-remove-database-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("the scratch directory is made");
        let database = directory.join("case.rdb");
        let names = [
            "case.rdb",
            "case.rdb-wal",
            "case.rdb-shm",
            "case.rdb-wal.0000000002",
            "case.rdb-wal.0000000007",
            "case.rdb2",
            "case.rdb2-wal.0000000001",
        ];
        for name in names {
            std::fs::write(directory.join(name), b"x").expect("a file is written");
        }
        remove_database(&database);
        let mut left: Vec<String> = std::fs::read_dir(&directory)
            .expect("the directory lists")
            .flatten()
            .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
            .collect();
        left.sort();
        assert_eq!(left, ["case.rdb2", "case.rdb2-wal.0000000001"]);
        std::fs::remove_file(directory.join("case.rdb2")).expect("cleanup");
        std::fs::remove_file(directory.join("case.rdb2-wal.0000000001")).expect("cleanup");
        std::fs::remove_dir(&directory).expect("cleanup");
    }

    /// The message a non-strict run prints ends with the one marker.
    ///
    /// The function writes to stderr rather than returning, so what is asserted
    /// here is the shape of the text the classifier reads, built the same way
    /// the function builds it.
    #[test]
    fn the_message_ends_with_the_marker() {
        let message = format!("the oracle is not built{MARKER}");
        assert!(message.ends_with(MARKER));
        assert!(!message.contains(STRICT_SKIP));
    }

    /// A strict run's panic carries the sentinel after the marker, in that
    /// order, so `missing_prerequisites` can tell a skip from a failure.
    #[test]
    fn a_strict_message_carries_the_sentinel_after_the_marker() {
        let message = format!("the oracle is not built{MARKER}{STRICT_SKIP}");
        let marker_at = message.find(MARKER).expect("the marker is present");
        let sentinel_at = message.find(STRICT_SKIP).expect("the sentinel is present");
        assert!(marker_at < sentinel_at);
        assert!(message.ends_with(STRICT_SKIP));
    }
}

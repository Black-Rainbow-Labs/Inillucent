//! A temporary folder for one test run, removed when the run ends.
//!
//! Invariant: **while a [`RunFolder`] is alive, `TMP`, `TEMP` and `TMPDIR` in
//! this process name a folder that belongs to this run alone, and every process
//! the run starts afterwards inherits them.** When the guard drops, the folder
//! and everything the run's builds and tests left in it are removed. A folder
//! left by a run that was killed is removed by the next run once nothing holds
//! it.
//!
//! ## Why this exists
//!
//! On 2026-10-25 the system temporary directory on the development machine
//! held 169,221 entries, and 105,562 of them were `inillucent-*` folders that
//! this workspace's tests had made and never removed. A program that lists
//! that directory pays for every entry: in Unluminous, a Jupyter kernel started
//! there gave no answer in 188 s where the same kernel in an empty folder
//! answered in 17 s. `inillucent_base::testing::ScratchDir` makes each test
//! remove its own folder; this module is the second half, which catches what a
//! test still leaves (a database a test leaked, a program's temporary file, a
//! test that failed and kept its folder) and removes it with the run.
//!
//! Unluminous's `tools/test-runs.mjs` does the same for its runners, and this
//! is the same layout: `<temp>/inillucent-tests/run-<pid>`.
//!
//! ## How a live run is told from a dead one
//!
//! Each run folder holds a file, [`ALIVE`], that the run keeps open for as
//! long as it lasts. On Windows the file is opened with no sharing, so another
//! process cannot open it again until the run's process has ended, which also
//! covers a run that was killed and a process id that was reused. On Unix
//! nothing stops a second open, so there the run is judged alive while a
//! process with the id in the folder's name exists. A reused id keeps a dead
//! run's folder until that process ends too, which leaves a folder behind and
//! never removes a live one.

use std::path::{Path, PathBuf};

/// The folder in the system temporary directory that holds every run folder.
pub const PARENT: &str = "inillucent-tests";

/// The file inside a run folder that the run holds open while it lasts.
pub const ALIVE: &str = ".alive";

/// The prefix of a run folder's name; the process id follows it.
const RUN_PREFIX: &str = "run-";

/// One run's temporary folder. Dropping it removes the folder.
#[derive(Debug)]
pub struct RunFolder {
    path: PathBuf,
    /// Held open for the run's lifetime; see the module comment.
    alive: Option<std::fs::File>,
}

impl RunFolder {
    /// Returns the folder this run's temporary files go in.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RunFolder {
    /// Closes the file that marks the run alive, then removes the folder.
    ///
    /// Errors are ignored. A file another process still holds open cannot be
    /// removed on Windows, and the next run removes what is left.
    fn drop(&mut self) {
        drop(self.alive.take());
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// Makes this run's folder, points the temporary directory variables at it,
/// and removes the folders of runs that have ended.
///
/// Returns `None` when the folder could not be made. The run then goes ahead
/// with the temporary directory it was given, because a test run that refused
/// to start over a cleanup would be worse than the folders it leaves.
///
/// @param parent - where run folders go, normally `<temp>/inillucent-tests`
pub fn enter(parent: &Path) -> Option<RunFolder> {
    let removed = sweep(parent);
    if removed > 0 {
        println!(
            "removed {removed} temporary folder(s) left by earlier runs in {}",
            parent.display()
        );
    }
    let folder = make(parent, std::process::id())?;
    for name in ["TMP", "TEMP", "TMPDIR"] {
        std::env::set_var(name, folder.path());
    }
    Some(folder)
}

/// Returns where run folders go: `inillucent-tests` in the temporary directory
/// this process was started with.
pub fn default_parent() -> PathBuf {
    std::env::temp_dir().join(PARENT)
}

/// Makes the run folder for process `pid` under `parent` and holds it alive.
///
/// @param parent - where run folders go
/// @param pid - the process id the folder is named after
pub fn make(parent: &Path, pid: u32) -> Option<RunFolder> {
    let path = parent.join(format!("{RUN_PREFIX}{pid}"));
    // A folder at this name is a dead run's whose process id was reused, since
    // this process has only just started.
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).ok()?;
    let alive = hold(&path.join(ALIVE))?;
    Some(RunFolder {
        path,
        alive: Some(alive),
    })
}

/// Removes every run folder under `parent` whose run has ended, and returns
/// how many were removed.
///
/// @param parent - where run folders go
pub fn sweep(parent: &Path) -> usize {
    let Ok(listing) = std::fs::read_dir(parent) else {
        return 0;
    };
    let mut removed = 0;
    for entry in listing.flatten() {
        let path = entry.path();
        let Some(pid) = run_pid(&path) else {
            continue;
        };
        if !run_is_alive(&path, pid) && std::fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Returns the process id a run folder is named after, or `None` when the path
/// is not a run folder.
///
/// @param path - an entry of the parent folder
fn run_pid(path: &Path) -> Option<u32> {
    if !path.is_dir() {
        return None;
    }
    path.file_name()?
        .to_str()?
        .strip_prefix(RUN_PREFIX)?
        .parse()
        .ok()
}

/// Opens the file that marks a run alive, so that no other process can open it
/// while this one lasts.
///
/// @param path - the file
#[cfg(windows)]
fn hold(path: &Path) -> Option<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .share_mode(0)
        .open(path)
        .ok()
}

/// Opens the file that marks a run alive.
///
/// Unix has no sharing modes, so the file only records that the folder is a
/// run folder; [`run_is_alive`] reads the process id instead.
///
/// @param path - the file
#[cfg(not(windows))]
fn hold(path: &Path) -> Option<std::fs::File> {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .ok()
}

/// Reports whether the run that made a folder is still going.
///
/// A folder with no [`ALIVE`] file is a run that ended before it made one or
/// after its drop removed it, so it is not alive.
///
/// @param folder - the run folder
/// @param pid - the process id in its name, unused on Windows
#[cfg(windows)]
fn run_is_alive(folder: &Path, _pid: u32) -> bool {
    let marker = folder.join(ALIVE);
    if !marker.exists() {
        return false;
    }
    // A sharing violation is the live run's no-sharing handle refusing us.
    // Any other failure means the file cannot be judged, and a folder that
    // cannot be judged is left alone.
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&marker)
        .is_err()
}

/// Reports whether the run that made a folder is still going.
///
/// @param folder - the run folder
/// @param pid - the process id in its name
#[cfg(not(windows))]
fn run_is_alive(folder: &Path, pid: u32) -> bool {
    if !folder.join(ALIVE).exists() {
        return false;
    }
    if Path::new("/proc").is_dir() {
        return Path::new("/proc").join(pid.to_string()).exists();
    }
    // macOS has no /proc. `kill -0` sends nothing and says whether the process
    // exists; a `kill` that cannot be run counts as alive, so the folder stays.
    std::process::Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_or(true, |status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Returns an empty parent folder for one test.
    ///
    /// @param tag - what to name it after
    fn parent(tag: &str) -> inillucent_base::testing::ScratchDir {
        inillucent_base::testing::ScratchDir::new(&format!(
            "inillucent-run-temp-{}-{tag}",
            std::process::id()
        ))
    }

    /// A live run's folder survives a sweep, and the same folder is removed by
    /// the sweep once the run has dropped its guard's hold, or by the drop.
    #[test]
    fn a_live_run_folder_survives_a_sweep_and_goes_when_it_drops() {
        let parent = parent("live");
        let folder = make(&parent, std::process::id()).expect("the run folder is made");
        std::fs::write(folder.path().join("left.rdb"), b"x").expect("a file");
        assert_eq!(sweep(&parent), 0);
        assert!(folder.path().join("left.rdb").is_file());
        let path = folder.path().to_path_buf();
        drop(folder);
        assert!(!path.exists(), "{} was left behind", path.display());
    }

    /// A folder whose run has ended is removed, and something in the parent
    /// that is not a run folder is left alone.
    #[test]
    fn a_dead_run_folder_is_removed_and_other_folders_are_not() {
        let parent = parent("dead");
        // A run folder with no marker: a run that ended before it made one.
        let dead = parent.join(format!("{RUN_PREFIX}{}", u32::MAX));
        std::fs::create_dir_all(dead.join("inner")).expect("a dead run folder");
        std::fs::write(dead.join("inner/stale.rdb"), b"x").expect("a stale file");
        let other = parent.join("not-a-run");
        std::fs::create_dir_all(&other).expect("another folder");
        assert_eq!(sweep(&parent), 1);
        assert!(!dead.exists());
        assert!(other.is_dir());
    }

    /// On Windows a run that was killed leaves its marker behind, closed, and
    /// that folder is removed.
    #[cfg(windows)]
    #[test]
    fn a_killed_runs_folder_with_its_marker_is_removed() {
        let parent = parent("killed");
        let dead = parent.join(format!("{RUN_PREFIX}{}", std::process::id()));
        std::fs::create_dir_all(&dead).expect("a killed run folder");
        std::fs::write(dead.join(ALIVE), b"").expect("a closed marker");
        assert_eq!(sweep(&parent), 1);
        assert!(!dead.exists());
    }
}

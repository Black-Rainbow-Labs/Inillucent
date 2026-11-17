//! The shell opened read only, on a database whose log holds a commit the file does not.
//!
//! Invariant: **`inillucent-shell -readonly`, a `file:...?mode=ro` name and `.open --readonly`
//! leave every file beside the database byte for byte as they were, and still answer from the
//! committed transaction that only the log holds.**
//!
//! ## Why this exists
//!
//! The shell opened the engine for writing when read only was asked, and refused writes itself.
//! A connection the engine opens for writing replays the committed transactions still in the log
//! and checkpoints them into the file. So a read only look at a release fixture rewrote about 200
//! bytes of it, both meta pages among them, and moved the format number from 1 to 2: the 0.1.3
//! binary then refused its own file as malformed. A file with an unreplayed log from any release
//! was changed the same way. `inillucent --readonly` opened the engine read only and changed
//! nothing, which is the behavior this holds the shell to.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use inillucent_base::testing::ScratchDir;
use inillucent_compat::cliproc::program;
use inillucent_compat::interop;

/// The questions each open asks: the row count, and the one row only the log holds.
const QUESTIONS: &str = "SELECT count(*) FROM note;\nSELECT title FROM note WHERE id = 9001;\n";

/// What the questions answer on every fixture: 120 rows in the file and one in the log.
const ANSWERS: &str = "121\nthe row that only the log holds\n";

/// What a run of the shell produced.
struct Ran {
    /// The exit status.
    code: Option<i32>,
    /// Standard output, with Windows line endings taken out.
    out: String,
    /// Standard error, with Windows line endings taken out.
    err: String,
}

/// Returns every entry in a folder with its bytes, so a change to any file, or a new entry, shows.
///
/// A folder is recorded by name with no bytes, so one an open leaves behind is named rather than
/// failing the read.
///
/// @param folder - the staged fixture's folder
fn snapshot(folder: &Path) -> BTreeMap<String, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    for entry in std::fs::read_dir(folder).expect("the staged folder is listed") {
        let entry = entry.expect("a folder entry");
        let bytes = match entry.path().is_dir() {
            true => None,
            false => Some(std::fs::read(entry.path()).expect("a staged file is read")),
        };
        entries.insert(entry.file_name().to_string_lossy().into_owned(), bytes);
    }
    entries
}

/// Returns the entries whose bytes differ between two snapshots, or that only one of them has.
///
/// @param before - the snapshot taken before the shell ran
/// @param after - the snapshot taken after it
fn changed<'a>(
    before: &'a BTreeMap<String, Option<Vec<u8>>>,
    after: &'a BTreeMap<String, Option<Vec<u8>>>,
) -> BTreeSet<&'a String> {
    before
        .keys()
        .chain(after.keys())
        .filter(|name| before.get(*name) != after.get(*name))
        .collect()
}

/// Runs the shell in a folder with a command line and a script.
///
/// @param folder - where it runs, which holds the staged fixture
/// @param arguments - the command line
/// @param script - what is written to standard input
fn shell(folder: &Path, arguments: &[&str], script: &str) -> Ran {
    let mut child = Command::new(program("inillucent-shell"))
        .args(arguments)
        .current_dir(folder)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the shell starts");
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(script.as_bytes())
            .expect("the script is written");
    }
    let output = child.wait_with_output().expect("the shell finishes");
    Ran {
        code: output.status.code(),
        out: String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n"),
        err: String::from_utf8_lossy(&output.stderr).replace("\r\n", "\n"),
    }
}

/// Stages a fixture in a folder of its own and checks that its log holds something to replay.
///
/// @param version - the release that wrote the fixture
/// @param case - what names the folder
fn staged(version: &str, case: &str) -> ScratchDir {
    let folder = ScratchDir::new(&format!(
        "shell-read-only-{}-{version}-{case}",
        std::process::id()
    ));
    interop::stage(version, &folder);
    assert!(
        snapshot(&folder)
            .keys()
            .any(|name| name.starts_with("app.rdb-wal.")),
        "{version}: the staged fixture has no log segment, so nothing here would replay"
    );
    folder
}

/// Each way of asking the shell for read only, on a fixture whose log holds a commit.
///
/// 0.1.3 is a format 1 fixture, the kind the replay used to move to format 2, and the newest
/// fixture is the format a current release writes. Every fixture was written to once
/// after its last checkpoint, so the log of each holds a committed row the file does not.
#[test]
fn a_read_only_shell_answers_from_the_log_and_writes_nothing() {
    let newest = interop::versions()
        .pop()
        .expect("tests/interop holds at least one release fixture");
    // The way, its command line, and whether the script opens the file itself.
    let ways: [(&str, &[&str], bool); 3] = [
        ("-readonly", &["-readonly", "app.rdb"], false),
        ("mode=ro", &["file:app.rdb?mode=ro"], false),
        (".open --readonly", &[], true),
    ];
    for version in ["0.1.3", newest.as_str()] {
        for (index, (way, arguments, opens)) in ways.iter().enumerate() {
            let folder = staged(version, &index.to_string());
            let before = snapshot(&folder);
            let script = match opens {
                true => format!(".open --readonly app.rdb\n{QUESTIONS}"),
                false => QUESTIONS.to_string(),
            };
            let ran = shell(&folder, arguments, &script);
            // Nothing on standard error either: a notice about the old format or the replay
            // would be new output on a read, and is worth a look before this is relaxed.
            assert!(
                ran.code == Some(0) && ran.err.is_empty(),
                "{version}, {way}: ended with {:?} and wrote to standard error:\n{}",
                ran.code,
                ran.err
            );
            assert_eq!(
                ran.out, ANSWERS,
                "{version}, {way}: the read only shell did not answer from the log's commit"
            );
            let after = snapshot(&folder);
            assert!(
                changed(&before, &after).is_empty(),
                "{version}, {way}: a read only open changed {:?}",
                changed(&before, &after)
            );
        }
    }
}

/// A rollback journal from an interrupted write is refused under read only, and nothing changes.
///
/// Putting a journal back is a write. The shell used to open for writing and put it back without
/// a word; a read only connection names what is wrong instead, as `inillucent --readonly` does.
#[test]
fn a_read_only_shell_refuses_a_journal_it_would_have_to_put_back() {
    let folder = staged("0.1.3", "journal");
    std::fs::write(folder.join("app.rdb-journal"), b"").expect("a journal is left");
    let before = snapshot(&folder);
    let ran = shell(&folder, &["-readonly", "app.rdb"], QUESTIONS);
    assert_eq!(ran.code, Some(1), "the open was not refused:\n{}", ran.out);
    assert!(
        ran.err.contains("a read only connection cannot replay it"),
        "the refusal does not say why:\n{}",
        ran.err
    );
    let after = snapshot(&folder);
    assert!(
        changed(&before, &after).is_empty(),
        "a refused read only open changed {:?}",
        changed(&before, &after)
    );
}

//! What people and other programs do to a database file from outside the
//! engine, and what the engine makes of the file afterwards.
//!
//! Invariant: **a file damaged, copied or separated from its log outside the
//! engine either opens into a state some committed prefix of its history had,
//! with every transaction whole, or is refused with an error. It is never
//! answered wrong without an error, and nothing panics.** SQLite's "How To
//! Corrupt An SQLite Database File" lists these as the usual causes of damage
//! in the field: a file copied while a program writes it, a log deleted or
//! swapped for another database's, and bytes changed on the disk.
//!
//! ## What each case can and cannot promise
//!
//! A log segment deleted after a kill takes the transactions only it held with
//! it, and no engine can bring those back; what this engine must do is open at
//! the last fold, consistent. Changed bytes on a page the database uses are
//! detected by the page checksum. Changed bytes on a free page change nothing
//! anybody reads, and the case counts those as harmless only when every query
//! still answers what the undamaged file answers.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use inillucent_compat::chaos::{check_snapshot, open, Check, Settings};
use inillucent_compat::cliproc::program;
use inillucent_compat::workspace_root;
use inillucent_driver::Value;

/// Returns `inillucent-chaos`, which cargo builds for this package's tests.
fn chaos() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_inillucent-chaos"))
}

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/file-damage")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs `inillucent-chaos` with arguments and fails the case if it fails.
///
/// @param arguments - the command line after the program name
fn chaos_ok(arguments: &[&str]) {
    let ran = Command::new(chaos())
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("inillucent-chaos did not start: {error}"));
    assert!(
        ran.status.success(),
        "inillucent-chaos {arguments:?} failed: {}",
        String::from_utf8_lossy(&ran.stdout)
    );
}

/// Builds a workload database with one worker's transactions, folded into the
/// file by the worker's clean exit.
///
/// @param path - the file to build
/// @param transactions - how many transactions the worker commits
fn built(path: &Path, transactions: u32) {
    let text = path.to_string_lossy().to_string();
    chaos_ok(&["setup", &text]);
    chaos_ok(&[
        "worker",
        &text,
        "--name",
        "w1",
        "--id",
        "1",
        "--seed",
        "9",
        "--txns",
        &transactions.to_string(),
        "--bulk-parts",
        "8",
    ]);
}

/// Copies every file whose name starts with the database's into another
/// directory, leaving out one name and optionally adding one file.
///
/// @param from - the directory the database lives in
/// @param to - the directory to copy into
/// @param leave_out - a file name not to copy
/// @param add - a file to add, as (source, name in `to`)
fn copy_files(from: &Path, to: &Path, leave_out: Option<&str>, add: Option<(&Path, &str)>) {
    let _ = std::fs::create_dir_all(to);
    if let Ok(entries) = std::fs::read_dir(from) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if Some(name.as_str()) == leave_out || !entry.path().is_file() {
                continue;
            }
            let _ = std::fs::copy(entry.path(), to.join(&name));
        }
    }
    if let Some((source, name)) = add {
        std::fs::copy(source, to.join(name))
            .unwrap_or_else(|error| panic!("could not add {name}: {error}"));
    }
}

/// What opening a file found.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// It opened, passed its integrity check and every workload invariant.
    Sound(i64),
    /// It was refused, at the open or by the integrity check, with this error.
    Refused(String),
    /// It opened and passed its integrity check, and a workload invariant failed.
    Wrong(String),
}

/// Opens a workload file and grades it.
///
/// @param path - the file
fn graded(path: &Path) -> Verdict {
    let settings = Settings {
        busy_ms: 5_000,
        ..Settings::default()
    };
    let database = match open(path, &settings) {
        Ok(database) => database,
        Err(why) => return Verdict::Refused(why),
    };
    if let Err(error) = database.integrity_check() {
        return Verdict::Refused(format!("{error:?}"));
    }
    let connection = database.session();
    match check_snapshot(&connection, &settings, std::time::Duration::ZERO, 0) {
        Check::Held(seen) => Verdict::Sound(seen),
        Check::Busy(why) => Verdict::Refused(why),
        Check::Broken(why) => Verdict::Wrong(why),
    }
}

/// Returns the last transaction the file records for the worker `w1`.
///
/// @param path - the file
fn recorded(path: &Path) -> i64 {
    let settings = Settings::default();
    let database = open(path, &settings).unwrap_or_else(|why| panic!("{why}"));
    let rows = database
        .session()
        .query_all("SELECT last_seq FROM progress WHERE worker = 'w1'", &[])
        .unwrap_or_else(|error| panic!("{error:?}"));
    match rows.rows.first().and_then(|row| row.first()) {
        Some(Value::Integer(seq)) => *seq,
        _ => 0,
    }
}

/// Copies of the files taken while a worker commits, the way a backup tool or
/// a person copying a folder takes them, each open into a state the history
/// had, or are refused.
#[test]
fn a_copy_taken_while_a_writer_runs_opens_whole_or_is_refused() {
    let directory = area("copy-while-writing");
    let live = directory.join("live");
    let _ = std::fs::create_dir_all(&live);
    let database = live.join("a.rdb");
    let text = database.to_string_lossy().to_string();
    chaos_ok(&["setup", &text]);
    let mut worker = Command::new(chaos())
        .args([
            "worker",
            &text,
            "--name",
            "w1",
            "--id",
            "1",
            "--seed",
            "3",
            "--txns",
            "100000",
            "--bulk-parts",
            "20",
        ])
        .stdout(Stdio::null())
        .spawn()
        .unwrap_or_else(|error| panic!("the worker did not start: {error}"));
    let mut verdicts = Vec::new();
    for copy in 0..12u64 {
        std::thread::sleep(std::time::Duration::from_millis(250 + copy * 37 % 200));
        let target = directory.join(format!("copy-{copy}"));
        copy_files(&live, &target, None, None);
        verdicts.push(graded(&target.join("a.rdb")));
    }
    let _ = worker.kill();
    let _ = worker.wait();
    let wrong: Vec<&Verdict> = verdicts
        .iter()
        .filter(|verdict| matches!(verdict, Verdict::Wrong(_)))
        .collect();
    assert!(
        wrong.is_empty(),
        "a copy opened, passed its integrity check, and held a state no committed prefix had: \
         {wrong:?}"
    );
    assert!(
        verdicts
            .iter()
            .any(|verdict| matches!(verdict, Verdict::Sound(seen) if *seen > 0)),
        "no copy held any committed transaction, so the case graded nothing: {verdicts:?}"
    );
}

/// A writer is killed with transactions in the log only. Each log segment is
/// then removed in turn, and another database's segment is put beside the
/// file: every copy opens consistent, the foreign segment is ignored, and a
/// removed segment loses only what it alone held.
#[test]
fn a_log_segment_removed_or_swapped_opens_consistent() {
    let directory = area("segments");
    let live = directory.join("live");
    let other = directory.join("other");
    let _ = std::fs::create_dir_all(&live);
    let _ = std::fs::create_dir_all(&other);
    let database = live.join("a.rdb");
    built(&other.join("a.rdb"), 30);
    let text = database.to_string_lossy().to_string();
    chaos_ok(&["setup", &text]);
    let acknowledged = killed_after(&text, 37);
    let segments = segment_names(&live);
    assert!(
        !segments.is_empty(),
        "the kill left no log, so nothing is tested"
    );

    let whole = directory.join("whole");
    copy_files(&live, &whole, None, None);
    let full = recorded(&whole.join("a.rdb"));
    assert!(
        full == acknowledged || full == acknowledged + 1,
        "with every file present the reopen records seq {full}, and the worker acknowledged \
         {acknowledged}"
    );
    assert_eq!(graded(&whole.join("a.rdb")), Verdict::Sound(full));

    for segment in &segments {
        let without = directory.join(format!("without-{segment}"));
        copy_files(&live, &without, Some(segment), None);
        let verdict = graded(&without.join("a.rdb"));
        assert!(
            matches!(verdict, Verdict::Sound(_) | Verdict::Refused(_)),
            "without {segment} the file opened into a state no committed prefix had: {verdict:?}"
        );
    }

    let foreign = segment_names(&other)
        .pop()
        .unwrap_or_else(|| panic!("the other database has no log segment"));
    let next = next_segment_name(segments.last().map(String::as_str).unwrap_or_default());
    let swapped = directory.join("foreign-next");
    copy_files(&live, &swapped, None, Some((&other.join(&foreign), &next)));
    assert_eq!(
        graded(&swapped.join("a.rdb")),
        Verdict::Sound(full),
        "another database's segment beside the file changed what it opens to"
    );
}

/// Starts a worker and kills it once it has acknowledged `count`
/// transactions, returning how many it acknowledged.
///
/// @param database - the file, as text
/// @param count - how many acknowledgements to wait for
fn killed_after(database: &str, count: usize) -> i64 {
    use std::io::{BufRead, BufReader};
    let mut worker = Command::new(chaos())
        .args([
            "worker", database, "--name", "w1", "--id", "1", "--seed", "3", "--txns", "100000",
        ])
        .stdout(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("the worker did not start: {error}"));
    let mut acknowledged = 0i64;
    if let Some(out) = worker.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            if let Some(seq) = line.strip_prefix("ACK ") {
                acknowledged = seq.trim().parse().unwrap_or(acknowledged);
                if acknowledged as usize >= count {
                    break;
                }
            }
        }
    }
    let _ = worker.kill();
    let _ = worker.wait();
    acknowledged
}

/// Returns the names of a directory's log segments, oldest first.
///
/// @param directory - the directory
fn segment_names(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.file_name().to_string_lossy().to_string())
                .filter(|name| name.contains("-wal."))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Returns the name the segment after `name` would have.
///
/// @param name - a segment name, `<db>-wal.<ten digits>`
fn next_segment_name(name: &str) -> String {
    let (base, sequence) = name.rsplit_once('.').unwrap_or((name, "0"));
    let next = sequence.parse::<u64>().unwrap_or(0) + 1;
    format!("{base}.{next:010}")
}

/// Every page of a small database, zeroed, with one bit flipped, and with the
/// file cut there: each copy is refused by the integrity check or answers
/// exactly what the undamaged file answers, and none panics.
#[test]
fn damage_at_every_page_is_found_or_changes_nothing() {
    let directory = area("every-page");
    let base = directory.join("base.rdb");
    built(&base, 60);
    let expected = answers(&base).unwrap_or_else(|why| panic!("the undamaged file: {why}"));
    let bytes = std::fs::read(&base).unwrap_or_else(|error| panic!("{error}"));
    let page = inillucent_driver::PAGE_SIZE;
    let pages = bytes.len() / page;
    let (mut found, mut harmless) = (0usize, 0usize);
    for number in 0..pages {
        for kind in ["zero", "flip", "cut"] {
            let damaged = directory.join(format!("p{number}-{kind}.rdb"));
            std::fs::write(&damaged, damage(&bytes, number, page, kind))
                .unwrap_or_else(|error| panic!("{error}"));
            match outcome(&damaged, &expected) {
                Outcome::Found => found += 1,
                Outcome::Harmless => harmless += 1,
                Outcome::Silent(why) => {
                    panic!("page {number} {kind}: the integrity check passed and {why}")
                }
            }
            let _ = std::fs::remove_file(&damaged);
        }
    }
    assert!(
        found > pages,
        "only {found} of {} damaged copies were found, so the checks may not be reading the \
         pages ({harmless} harmless)",
        pages * 3
    );
}

/// What one damaged copy did.
enum Outcome {
    /// The open or the integrity check refused it.
    Found,
    /// It passed, and every query answered what the undamaged file answers.
    Harmless,
    /// It passed, and a query answered something else.
    Silent(String),
}

/// Returns `bytes` with one page damaged.
///
/// @param bytes - the undamaged file
/// @param number - the page
/// @param page - the page size
/// @param kind - `zero`, `flip` or `cut`
fn damage(bytes: &[u8], number: usize, page: usize, kind: &str) -> Vec<u8> {
    let start = number * page;
    match kind {
        "zero" => {
            let mut copy = bytes.to_vec();
            copy.iter_mut()
                .skip(start)
                .take(page)
                .for_each(|byte| *byte = 0);
            copy
        }
        "flip" => {
            let mut copy = bytes.to_vec();
            if let Some(byte) = copy.get_mut(start + page / 2) {
                *byte ^= 0x10;
            }
            copy
        }
        _ => bytes.iter().take(start).copied().collect(),
    }
}

/// Grades one damaged copy against the undamaged file's answers.
///
/// @param path - the damaged copy
/// @param expected - what the undamaged file answers
fn outcome(path: &Path, expected: &[String]) -> Outcome {
    let checked = std::panic::catch_unwind(|| {
        let settings = Settings::default();
        let database = open(path, &settings).map_err(|_| ())?;
        database.integrity_check().map_err(|_| ())?;
        drop(database);
        answers(path).map_err(|_| ())
    });
    match checked {
        Err(_) => Outcome::Silent("the engine panicked".to_string()),
        Ok(Err(())) => Outcome::Found,
        Ok(Ok(got)) if got == expected => Outcome::Harmless,
        Ok(Ok(got)) => Outcome::Silent(format!(
            "answered {got:?} where the file answers {expected:?}"
        )),
    }
}

/// Runs the queries that read every table and index of the workload.
///
/// @param path - the file
fn answers(path: &Path) -> Result<Vec<String>, String> {
    let database = open(path, &Settings::default())?;
    let connection = database.session();
    let mut out = Vec::new();
    for sql in [
        "SELECT count(*), sum(length(body)), total(digest) FROM ledger",
        "SELECT sum(balance), count(*) FROM account",
        "SELECT count(*) FROM ledger INDEXED BY ledger_digest WHERE digest >= -9223372036854775808",
        "SELECT worker, last_seq FROM progress",
    ] {
        let rows = connection
            .query_all(sql, &[])
            .map_err(|error| format!("{sql}: {error:?}"))?;
        out.push(format!("{:?}", rows.rows));
    }
    Ok(out)
}

/// `inillucent dump` of a damaged file exits with a status a script can read,
/// zero or one, and never a crash.
#[test]
fn a_dump_of_a_damaged_file_ends_with_a_status() {
    let directory = area("dump");
    let base = directory.join("base.rdb");
    built(&base, 30);
    let bytes = std::fs::read(&base).unwrap_or_else(|error| panic!("{error}"));
    let page = inillucent_driver::PAGE_SIZE;
    for number in 1..(bytes.len() / page) {
        let damaged = directory.join(format!("p{number}.rdb"));
        std::fs::write(&damaged, damage(&bytes, number, page, "zero"))
            .unwrap_or_else(|error| panic!("{error}"));
        let ran = Command::new(program("inillucent"))
            .args(["--db", &damaged.to_string_lossy(), "dump"])
            .stdin(Stdio::null())
            .output()
            .unwrap_or_else(|error| panic!("{error}"));
        let code = ran.status.code();
        assert!(
            matches!(code, Some(0) | Some(1)),
            "dump of a file with page {number} zeroed ended with {code:?}: {}",
            String::from_utf8_lossy(&ran.stderr)
        );
        let _ = std::fs::remove_file(&damaged);
    }
}

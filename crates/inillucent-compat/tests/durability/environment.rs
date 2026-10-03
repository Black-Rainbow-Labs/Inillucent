//! The machine around a database: a disk that fills, a file the process may not
//! write, a file reached through more than one spelling of its path, and paths
//! that are not plain ASCII or are longer than Windows' old limit.
//!
//! Invariant: **each of these either works as it would on a plain path with
//! space to spare, or is refused with the status that names the cause, and in
//! both cases every committed row survives.** None of them is exotic. A full
//! disk is the most common way SQLite users first meet `SQLITE_FULL`, read only
//! files are what restores from backups and read only media produce, and paths
//! with accents or more than 260 characters are where Windows applications put
//! their data by default.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

use inillucent_compat::chaos::{check_snapshot, open, Check, Settings};
use inillucent_compat::cliproc::{program, rows, run};
use inillucent_compat::newengine::ImportedDatabase;
use inillucent_compat::workspace_root;
use inillucent_driver::{Database, OpenOptions, Status, Value};
use inillucent_exec::physical::Params;
use inillucent_sim::failpoint::{Failure, Policy, Site};
use inillucent_sim::sim_vfs::{SimConfig, SimVfs};
use inillucent_tree::datum::OwnedDatum;
use inillucent_vfs::Vfs;

/// Returns a directory of this case's own, emptied first.
///
/// @param name - the case's name, which is also the directory's
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/environment")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs a statement on an engine opened on a simulated file system.
///
/// @param engine - the database
/// @param sql - the statement
fn run_sql(
    engine: &mut ImportedDatabase,
    sql: &str,
) -> Result<Vec<Vec<OwnedDatum>>, inillucent_base::DbError> {
    engine
        .execute_any(sql, &Params::new())
        .map(|outcome| outcome.rows)
}

/// Returns the one integer a query answers.
///
/// @param engine - the database
/// @param sql - the query
fn number(engine: &mut ImportedDatabase, sql: &str) -> i64 {
    match run_sql(engine, sql) {
        Ok(found) => match found.first().and_then(|row| row.first()) {
            Some(OwnedDatum::Int(value)) => *value,
            other => panic!("{sql} answered {other:?}"),
        },
        Err(error) => panic!("{sql} failed: {error}"),
    }
}

/// A disk that fills while a connection is open refuses the write with
/// `SQLITE_FULL`, the connection keeps reading what was committed, and once
/// space returns the same connection writes, folds and reopens clean.
///
/// RocksDB issue 7784 and SQLite's own notes both describe the second half
/// going wrong: a connection that met a full disk once kept failing, or a fold
/// that failed half way left the file unreadable.
#[test]
fn a_full_disk_refuses_the_write_and_the_same_connection_writes_once_space_returns() {
    let config = SimConfig {
        seed: 0x2173,
        ..SimConfig::default()
    };
    let simulated = Arc::new(SimVfs::new(config.clone()));
    let vfs: Arc<dyn Vfs> = Arc::clone(&simulated) as Arc<dyn Vfs>;
    let path = PathBuf::from("full.rdb");
    let mut engine = ImportedDatabase::create_on(Arc::clone(&vfs), path.clone(), 4_096, 64)
        .unwrap_or_else(|error| panic!("the database was not created: {error}"));
    let fill = |count: u32| {
        format!(
            "WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < {count}) \
             INSERT INTO t (body) SELECT randomblob(3000) FROM k"
        )
    };
    run_sql(
        &mut engine,
        "CREATE TABLE t (id INTEGER PRIMARY KEY, body BLOB NOT NULL)",
    )
    .unwrap_or_else(|error| panic!("{error}"));
    run_sql(&mut engine, &fill(200)).unwrap_or_else(|error| panic!("{error}"));
    engine
        .checkpoint()
        .unwrap_or_else(|error| panic!("{error}"));

    for site in [Site::Write, Site::Allocate] {
        simulated
            .failpoints()
            .set(site, Policy::Always(Failure::DiskFull));
    }
    let refused = run_sql(&mut engine, &fill(400));
    let code = refused.as_ref().err().map(|error| error.code());
    assert_eq!(
        code,
        Some(inillucent_base::error::PrimaryCode::Full),
        "a write on a full disk answered {refused:?}"
    );
    let _ = engine.checkpoint();
    // **A read on a full disk may be refused too, with the same status.** The
    // refused statement's undo is logged, so the pages it touched stay dirty,
    // and a 64 page pool evicting one of them has to write it somewhere. What
    // it may not do is answer anything but the committed count, or fail with a
    // status other than `full`.
    match run_sql(&mut engine, "SELECT count(*) FROM t") {
        Ok(found) => assert_eq!(
            found.first().and_then(|row| row.first()),
            Some(&OwnedDatum::Int(200)),
            "the committed rows on a full disk"
        ),
        Err(error) => assert_eq!(
            error.code(),
            inillucent_base::error::PrimaryCode::Full,
            "a read on a full disk failed with {error}"
        ),
    }

    for site in [Site::Write, Site::Allocate] {
        simulated.failpoints().set(site, Policy::Off);
    }
    run_sql(&mut engine, &fill(10)).unwrap_or_else(|error| {
        panic!("the same connection could not write once space returned: {error}")
    });
    engine
        .checkpoint()
        .unwrap_or_else(|error| panic!("the fold after space returned failed: {error}"));
    assert_eq!(number(&mut engine, "SELECT count(*) FROM t"), 210);
    drop(engine);

    let mut reopened = ImportedDatabase::open_on(vfs, path, 4_096, 64)
        .unwrap_or_else(|error| panic!("the file did not reopen: {error}"));
    assert_eq!(number(&mut reopened, "SELECT count(*) FROM t"), 210);
    let checked =
        run_sql(&mut reopened, "PRAGMA integrity_check").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(
        checked.first().and_then(|row| row.first()),
        Some(&OwnedDatum::Text("ok".into())),
        "the reopened file failed its integrity check: {checked:?}"
    );
}

/// Marks a file read only, or writable again.
///
/// @param path - the file
/// @param read_only - which
fn set_read_only(path: &Path, read_only: bool) {
    let mut permissions = std::fs::metadata(path)
        .unwrap_or_else(|error| panic!("{error}"))
        .permissions();
    permissions.set_readonly(read_only);
    std::fs::set_permissions(path, permissions).unwrap_or_else(|error| panic!("{error}"));
}

/// A database file the process may not write opens read only: it answers
/// queries, refuses a write with status `readonly`, and its bytes do not move.
///
/// **This used to refuse the open itself**, with `could not open ...: access
/// permission denied` and status `invalid_state`, for a `SELECT`. SQLite opens
/// such a file read only, and a restore from a backup or a copy off read only
/// media is exactly such a file.
#[test]
fn a_read_only_file_is_read_and_refuses_writes_by_name() {
    let directory = area("read-only-file");
    let path = directory.join("ro.rdb");
    {
        let database = Database::open(&path).unwrap_or_else(|error| panic!("{error:?}"));
        database
            .session()
            .execute_batch(
                "CREATE TABLE t (x INTEGER);
                 WITH RECURSIVE k(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM k WHERE i < 50)
                 INSERT INTO t SELECT i FROM k;",
            )
            .unwrap_or_else(|error| panic!("{error:?}"));
        database
            .checkpoint()
            .unwrap_or_else(|error| panic!("{error:?}"));
    }
    let before = std::fs::read(&path).unwrap_or_else(|error| panic!("{error}"));
    set_read_only(&path, true);
    let outcome = std::panic::catch_unwind(|| {
        let database = Database::open_with(&path, OpenOptions::default()).unwrap_or_else(|error| {
            panic!("a read only file could not be opened to read: {error:?}")
        });
        let session = database.session();
        let counted = session
            .query("SELECT count(*), sum(x) FROM t", &[], 1)
            .unwrap_or_else(|error| panic!("{error:?}"));
        assert_eq!(
            counted.rows,
            vec![vec![Value::Integer(50), Value::Integer(1275)]]
        );
        let refused = session
            .execute("INSERT INTO t VALUES (51)", &[])
            .expect_err("a write to a read only file must be refused");
        assert_eq!(
            refused.status,
            Status::ReadOnly,
            "the refusal was {refused:?}"
        );
        let cli = run(
            &program("inillucent"),
            &[
                "--db",
                &path.to_string_lossy(),
                "query",
                "SELECT count(*) FROM t",
                "--output",
                "json",
            ],
        );
        assert_eq!(
            cli.code,
            0,
            "the command line could not read it: {}",
            cli.said()
        );
        assert_eq!(rows(&cli.stdout), vec![vec!["50".to_string()]]);
    });
    set_read_only(&path, false);
    if let Err(panicked) = outcome {
        std::panic::resume_unwind(panicked);
    }
    let after = std::fs::read(&path).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        before == after,
        "reading a read only file changed its bytes"
    );
}

/// Two writer processes reach one file through different spellings of its
/// path, and every acknowledged insert is there.
///
/// Locks are taken on the open file, and the log's segments are named from
/// the path. A spelling with `.` and `..` in it, and on Windows one in a
/// different case, must still name the same log, or the two processes would
/// each write a log the other never replays.
#[test]
fn two_writers_through_different_spellings_of_one_path_lose_nothing() {
    let directory = area("spellings");
    let _ = std::fs::create_dir_all(directory.join("sub"));
    let canonical = directory.join("Shared.rdb");
    let binary = program("inillucent");
    let text = canonical.to_string_lossy().to_string();
    for arguments in [
        vec!["create", text.as_str()],
        vec![
            "--db",
            text.as_str(),
            "exec",
            "CREATE TABLE note (who TEXT NOT NULL, n INTEGER NOT NULL, UNIQUE(who, n))",
        ],
    ] {
        let ran = run(&binary, &arguments);
        assert_eq!(ran.code, 0, "{arguments:?}: {}", ran.said());
    }
    let mut spellings = vec![
        directory.join(".").join("Shared.rdb"),
        directory.join("sub").join("..").join("Shared.rdb"),
    ];
    if cfg!(windows) {
        spellings.push(PathBuf::from(text.to_uppercase()));
    }
    let writers: Vec<std::thread::JoinHandle<usize>> = spellings
        .into_iter()
        .enumerate()
        .map(|(index, spelling)| {
            let binary = binary.clone();
            std::thread::spawn(move || one_writer(&binary, &spelling, &format!("w{index}"), 40))
        })
        .collect();
    let acknowledged: usize = writers
        .into_iter()
        .map(|writer| writer.join().unwrap_or(0))
        .sum();
    let counted = run(
        &binary,
        &[
            "--db",
            &text,
            "query",
            "SELECT count(*) FROM note",
            "--output",
            "json",
        ],
    );
    assert_eq!(rows(&counted.stdout), vec![vec![acknowledged.to_string()]]);
    assert!(
        acknowledged > 40,
        "too few inserts were acknowledged to show anything"
    );
}

/// Sends `count` inserts, one process each, and returns how many succeeded.
///
/// @param binary - the built `inillucent`
/// @param path - the spelling of the database path this writer uses
/// @param who - the writer's name
/// @param count - how many inserts
fn one_writer(binary: &Path, path: &Path, who: &str, count: usize) -> usize {
    (1..=count)
        .filter(|n| {
            let sql = format!("INSERT INTO note (who, n) VALUES ('{who}', {n})");
            Command::new(binary)
                .args(["--db", &path.to_string_lossy(), "exec", &sql])
                .env("INILLUCENT_BUSY_TIMEOUT", "30000")
                .stdin(Stdio::null())
                .output()
                .is_ok_and(|ran| ran.status.success())
        })
        .count()
}

/// A database in a folder whose name is not ASCII, and one whose full path is
/// longer than 260 characters, are created, written by a process that is then
/// killed, and reopened whole.
#[test]
fn unusual_paths_survive_a_killed_writer() {
    let directory = area("paths");
    let accented = directory.join("donn\u{e9}es-\u{65e5}\u{672c}-\u{f1}");
    let mut long = directory.join("long");
    while long.to_string_lossy().len() < 300 {
        long = long.join("a-rather-long-directory-name-0123456789");
    }
    for folder in [accented, long] {
        let _ = std::fs::create_dir_all(&folder);
        let database = folder.join("\u{e9}t\u{e9}.rdb");
        let text = database.to_string_lossy().to_string();
        let chaos = PathBuf::from(env!("CARGO_BIN_EXE_inillucent-chaos"));
        let built = Command::new(&chaos).args(["setup", &text]).output();
        assert!(
            built.as_ref().is_ok_and(|ran| ran.status.success()),
            "setup failed at {text}: {built:?}"
        );
        let mut worker = Command::new(&chaos)
            .args([
                "worker", &text, "--name", "w1", "--id", "1", "--seed", "5", "--txns", "100000",
            ])
            .stdout(Stdio::null())
            .spawn()
            .unwrap_or_else(|error| panic!("{error}"));
        std::thread::sleep(std::time::Duration::from_millis(1500));
        let _ = worker.kill();
        let _ = worker.wait();
        let settings = Settings::default();
        let reopened = open(&database, &settings).unwrap_or_else(|why| panic!("{text}: {why}"));
        reopened
            .integrity_check()
            .unwrap_or_else(|error| panic!("{text}: {error:?}"));
        let session = reopened.session();
        match check_snapshot(&session, &settings, std::time::Duration::ZERO, 0) {
            Check::Held(seen) => assert!(seen > 0, "{text}: the worker committed nothing"),
            other => panic!("{text}: {other:?}"),
        }
    }
    // **The long tree goes when the case passes.** `git worktree remove` cannot
    // delete a path over 260 characters on Windows and stops with `Filename too
    // long`, so a worktree that had run this case could not be retired.
    let _ = std::fs::remove_dir_all(&directory);
}

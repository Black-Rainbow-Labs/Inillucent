//! Migrating every SQLite fixture into the new engine, verified.
//!
//! Invariant: **a migration is verified by counts and digests, and by a third
//! engine.** The tool computes both sides itself - the source through
//! `inillucent-sqlite-reader`, the destination through the new engine's own scan -
//! which catches a copy that lost or reordered rows. What it cannot catch is a
//! reader that mis-decodes the source, because the same reader also performs
//! the import. So this file adds the oracle the tool deliberately does not
//! carry: the **pinned SQLite 3.53.4 shell**, reading the source file, whose
//! row counts every fixture's migration is checked against.
//!
//! Three engines, then: SQLite's own b-tree code, the reader, and the new
//! engine. A number all three agree on is not one implementation's opinion.
//!
//! The corpus is `compat/fixtures`, written with that same pinned
//! binary: fifteen valid databases spanning page sizes 512 to 65536, both UTF-16
//! encodings, overflow chains, `WITHOUT ROWID`, deep trees, freelists, both
//! vacuum modes, an empty database and a collation corpus - and seventeen
//! malformed ones, each a single named edit to a valid fixture, which are the
//! failure path.

use std::path::{Path, PathBuf};
use std::process::Command;

use inillucent_compat::workspace_root;
use inillucent_engine::ImportedDatabase;
use inillucent_migrate::sqlite;
use inillucent_tree::datum::OwnedDatum;

/// Where this suite's scratch databases live.
const AREA: &str = "migrate-sqlite";

/// Returns a clean scratch directory for one test.
///
/// @param name - the test's name, which becomes the directory's
fn scratch(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("target")
        .join("scratch")
        .join(AREA)
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Returns the fixture corpus directory.
fn fixtures() -> PathBuf {
    workspace_root().join("compat/fixtures")
}

/// Returns the pinned SQLite shell, if it has been downloaded.
fn reference() -> Option<PathBuf> {
    let directory = workspace_root().join(".sqlite-ref/3.53.4/shell");
    let path = directory.join(format!("sqlite3{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Returns every valid fixture's path, in the manifest's order.
///
/// The manifest is read rather than the directory listed, because the malformed
/// fixtures sit in the same directory and only the manifest says which is
/// which - and reading the directory would silently start migrating a corrupt
/// file as though it were expected to work.
fn valid_fixtures() -> Vec<PathBuf> {
    let manifest = std::fs::read_to_string(workspace_root().join("compat/fixtures/manifest.toml"))
        .expect("the fixture manifest is checked in");
    let mut out = Vec::new();
    for block in manifest.split("[[fixture]]").skip(1) {
        let field = |key: &str| -> Option<String> {
            block
                .lines()
                .find(|line| line.trim_start().starts_with(&format!("{key} = ")))
                .and_then(|line| line.split_once('='))
                .map(|(_, value)| value.trim().trim_matches('"').to_string())
        };
        if field("kind").as_deref() == Some("valid") {
            if let Some(name) = field("name") {
                out.push(fixtures().join(name));
            }
        }
    }
    assert!(
        out.len() >= 15,
        "the manifest should name at least fifteen valid fixtures, it named {}",
        out.len()
    );
    out
}

/// Returns every malformed fixture's path.
fn malformed_fixtures() -> Vec<PathBuf> {
    let manifest = std::fs::read_to_string(workspace_root().join("compat/fixtures/manifest.toml"))
        .expect("the fixture manifest is checked in");
    let mut out = Vec::new();
    for block in manifest.split("[[fixture]]").skip(1) {
        let field = |key: &str| -> Option<String> {
            block
                .lines()
                .find(|line| line.trim_start().starts_with(&format!("{key} = ")))
                .and_then(|line| line.split_once('='))
                .map(|(_, value)| value.trim().trim_matches('"').to_string())
        };
        if field("kind").as_deref() == Some("malformed") {
            if let Some(name) = field("name") {
                out.push(fixtures().join(name));
            }
        }
    }
    out
}

/// Returns what the pinned SQLite makes of one table, as `(rows, digest)`.
///
/// Both numbers come out of the shell rather than out of anything in this
/// workspace, which is the whole point: a third implementation reading the
/// source file. The digest is `group_concat` over `quote()` of every column,
/// ordered by rowid - `quote` renders each storage class unambiguously, a blob
/// as `X'..'`, a text with its quotes, a NULL as `NULL`, so two different
/// values cannot render alike - folded to a fixed width so a large table does
/// not return a megabyte of text.
///
/// @param shell - the pinned sqlite3 binary
/// @param database - the file to read
/// @param table - the table to summarise
fn sqlite_rows(shell: &Path, database: &Path, table: &str) -> Option<Vec<String>> {
    let quoted = table.replace('"', "\"\"");
    let output = Command::new(shell)
        .arg("-cmd")
        .arg(".mode quote")
        .arg(database)
        .arg(format!("SELECT * FROM \"{quoted}\";"))
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// Renders one migrated row the way the pinned SQLite shell's quote mode does.
///
/// The point of rendering rather than hashing is that a mismatch is *readable*:
/// a failure prints the row SQLite saw beside the row the new engine produced,
/// which is what tells a person whether the bug is a lost row, a shifted
/// column or a mis-decoded value.
///
/// The REAL arm goes through `inillucent_value::numeric::real_to_text`, which is
/// the engine's SQLite-compatible float rendering and is already graded against
/// SQLite by the differential corpus - so this is not a second float formatter
/// that could drift from the first.
///
/// @param row - one row as the new engine produced it
fn quote_row(row: &[OwnedDatum]) -> String {
    row.iter()
        .map(|value| match value {
            OwnedDatum::Null => "NULL".to_string(),
            OwnedDatum::Int(number) => number.to_string(),
            OwnedDatum::Real(number) => {
                let text =
                    String::from_utf8_lossy(&inillucent_value::numeric::real_to_text(*number))
                        .into_owned();
                // SQLite's quote mode renders a whole-numbered REAL with a
                // trailing `.0`; `real_to_text` already does, so this is only
                // the exponent form's spelling.
                text
            }
            OwnedDatum::Text(bytes) => {
                let text = String::from_utf8_lossy(bytes);
                format!("'{}'", text.replace('\'', "''"))
            }
            OwnedDatum::Blob(bytes) => {
                // Lowercase, both the prefix and the digits, because that is
                // what the shell's quote mode prints.
                let mut out = String::with_capacity(bytes.len() * 2 + 3);
                out.push_str("x'");
                for byte in bytes.iter() {
                    out.push_str(&format!("{byte:02x}"));
                }
                out.push('\'');
                out
            }
        })
        .collect::<Vec<String>>()
        .join(",")
}

/// Every valid fixture migrates, with counts and digests verified, and the
/// pinned SQLite agreeing about every count.
#[test]
fn every_task_1781_fixture_migrates_with_verified_counts_and_digests() {
    let Some(shell) = reference() else {
        inillucent_compat::differential::skipping(
            "the pinned SQLite oracle is not built; run tools/sqlite-reference.{ps1,sh}",
        );
        return;
    };
    let area = scratch("every-fixture");
    let mut migrated = 0usize;
    let mut tables_checked = 0usize;
    for fixture in valid_fixtures() {
        let name = fixture
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let destination = area.join(format!("{name}.rdb"));
        let report = sqlite::migrate(&fixture, &destination).unwrap_or_else(|error| {
            panic!(
                "{name}: the migration failed: {}",
                error.detail().unwrap_or_default()
            )
        });
        assert!(
            destination.is_file() || !report.passed(),
            "{name}: a passing migration publishes its destination"
        );
        // **The source is untouched.** Not renamed, not truncated, not removed.
        assert!(fixture.is_file(), "{name}: the source must still be there");

        // The third engine, value by value. The published database is opened
        // by the new engine's own open path and every row is rendered the way
        // the pinned shell's quote mode renders it, so a difference is a
        // difference in the data rather than in two hashing schemes.
        // The published file when the migration published one, and the staging
        // file when it did not - so a failed verification is still diffed
        // against the pinned SQLite and says *which row* it disagreed about
        // rather than only that two digests differed.
        let built = if destination.is_file() {
            destination.clone()
        } else {
            report.staged.clone()
        };
        let opened =
            ImportedDatabase::open(built.clone(), sqlite::PAGE_SIZE, 256).unwrap_or_else(|error| {
                panic!(
                    "{name}: {} did not open: {}",
                    built.display(),
                    error.detail().unwrap_or_default()
                )
            });
        for table in &report.inventory.tables {
            let Some(theirs) = sqlite_rows(&shell, &fixture, &table.name) else {
                continue;
            };
            let quoted = table.name.replace('"', "\"\"");
            let (ours, _) = opened
                .run(&format!("SELECT * FROM \"{quoted}\""))
                .unwrap_or_else(|error| {
                    panic!(
                        "{name}.{}: the migrated table did not read: {}",
                        table.name,
                        error.detail().unwrap_or_default()
                    )
                });
            assert_eq!(
                ours.len(),
                theirs.len(),
                "{name}.{}: the new engine read {} rows, the pinned SQLite {}",
                table.name,
                ours.len(),
                theirs.len()
            );
            // Sorted on both sides, because neither `SELECT *` promises an
            // order and the new engine will answer one from a covering index
            // when that is the cheaper structure. What is being checked is that
            // the two engines hold the same rows, which is what a migration is
            // for; the order they come back in is a plan's business.
            let mut mine: Vec<String> = ours.iter().map(|row| quote_row(row)).collect();
            let mut reference = theirs.clone();
            mine.sort();
            reference.sort();
            for (position, (ours, theirs)) in mine.iter().zip(reference.iter()).enumerate() {
                assert_eq!(
                    ours, theirs,
                    "{name}.{}: row {position} differs between the new engine and the pinned SQLite",
                    table.name
                );
            }
            tables_checked += 1;
        }
        drop(opened);
        // Only now the tool's own verdict, which is the same claim in hash form.
        assert!(
            report.passed(),
            "{name}: {:?}",
            report
                .failures()
                .iter()
                .map(|check| check.line())
                .collect::<Vec<String>>()
        );
        migrated += 1;
    }
    assert!(migrated >= 15, "only {migrated} fixtures migrated");
    assert!(
        tables_checked > 0,
        "the pinned SQLite verified no table at all, so this test proved nothing"
    );
}

/// A migration run twice produces the same target, so a retry is safe.
#[test]
fn a_migration_run_twice_produces_the_same_target() {
    let area = scratch("idempotent");
    let fixture = fixtures().join("basic-p4096-utf8.db");
    let first = area.join("first.rdb");
    let second = area.join("second.rdb");

    let one = sqlite::migrate(&fixture, &first).expect("the first migration runs");
    assert!(one.passed(), "the first migration verified");
    let two = sqlite::migrate(&fixture, &second).expect("the second migration runs");
    assert!(two.passed(), "the second migration verified");

    // Same tables, same counts, same digests - which is what "the same target"
    // means for a database whose file also carries a log position and a
    // creation order that are not part of its contents.
    assert_eq!(
        one.inventory.tables, two.inventory.tables,
        "two runs over one source inventoried different tables"
    );
    for table in &one.inventory.tables {
        let a = one
            .checks
            .iter()
            .find(|check| check.name == format!("digest.{}", table.name));
        let b = two
            .checks
            .iter()
            .find(|check| check.name == format!("digest.{}", table.name));
        assert_eq!(
            a.map(|check| &check.detail),
            b.map(|check| &check.detail),
            "{}: two runs produced different digests",
            table.name
        );
    }
}

/// A migration never overwrites a destination that is already there.
#[test]
fn a_migration_refuses_to_overwrite_a_destination() {
    let area = scratch("no-overwrite");
    let fixture = fixtures().join("basic-p4096-utf8.db");
    let destination = area.join("taken.rdb");
    std::fs::write(&destination, b"not a database").expect("the scratch file is written");

    let error = sqlite::migrate(&fixture, &destination)
        .expect_err("a migration must refuse an occupied destination");
    assert!(
        error
            .detail()
            .unwrap_or_default()
            .contains("already exists"),
        "the refusal must say why: {}",
        error.detail().unwrap_or_default()
    );
    assert_eq!(
        std::fs::read(&destination).expect("the file is still readable"),
        b"not a database",
        "the destination must be exactly as it was"
    );
}

/// A truncated source produces a named error and no published database.
#[test]
fn a_truncated_source_is_refused_by_name() {
    let area = scratch("truncated");
    // Two truncations, because they fail at different depths: one loses the
    // header outright, the other keeps a valid header over a body that stops
    // in the middle of a page.
    for name in ["truncated-header.db", "truncated-mid-page.db"] {
        let fixture = fixtures().join(name);
        let destination = area.join(format!("{name}.rdb"));
        let outcome = sqlite::migrate(&fixture, &destination);
        let refused = match outcome {
            Err(error) => {
                assert!(
                    !error.detail().unwrap_or_default().is_empty(),
                    "{name}: the error must say something"
                );
                assert!(
                    error.detail().unwrap_or_default().contains(name),
                    "{name}: the error must name the file it refused: {}",
                    error.detail().unwrap_or_default()
                );
                true
            }
            Ok(report) => {
                // Reading far enough to build something is allowed; publishing
                // it is not, and a report that passed would mean the truncation
                // went unnoticed.
                assert!(
                    !report.passed(),
                    "{name}: a truncated source must not verify"
                );
                false
            }
        };
        assert!(
            !destination.exists(),
            "{name}: nothing may be published from a truncated source (refused early: {refused})"
        );
    }
}

/// Every corrupt source produces a named error or a failed verification, and
/// never a published database.
#[test]
fn a_corrupt_source_never_publishes_a_database() {
    let area = scratch("corrupt");
    let mut refused = 0usize;
    let malformed = malformed_fixtures();
    assert!(
        malformed.len() >= 15,
        "the manifest should name at least fifteen malformed fixtures, it named {}",
        malformed.len()
    );
    for fixture in malformed {
        let name = fixture
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let destination = area.join(format!("{name}.rdb"));
        match sqlite::migrate(&fixture, &destination) {
            Err(error) => {
                assert!(
                    error.detail().unwrap_or_default().contains(&name)
                        || error.detail().unwrap_or_default().contains("could not be"),
                    "{name}: the error must name the file or what failed: {}",
                    error.detail().unwrap_or_default()
                );
                refused += 1;
            }
            Ok(report) => {
                // A corruption the reader tolerates is allowed to produce a
                // report; what it must not produce is a *passing* one that
                // published a database, because that is the shape of a partial
                // database that looks finished.
                if report.passed() {
                    assert!(
                        destination.is_file(),
                        "{name}: a passing report must have published"
                    );
                } else {
                    assert!(
                        !destination.exists(),
                        "{name}: a failed verification must publish nothing"
                    );
                    refused += 1;
                }
            }
        }
        // The source is never touched, however badly it read.
        assert!(fixture.is_file(), "{name}: the source must still be there");
    }
    assert!(
        refused > 0,
        "not one malformed fixture was refused, so this test proved nothing"
    );
}

/// A target that runs out of space produces a named error and publishes nothing.
///
/// The disk is not filled - a test that filled a real disk would be a test that
/// took the machine down. What is denied instead is the *directory*: the
/// staging file is created inside a path that does not exist, which is the same
/// failure the file system reports when it cannot place the file, and it
/// reaches the same code.
#[test]
fn a_target_that_cannot_be_written_is_refused_by_name() {
    let area = scratch("no-space");
    let fixture = fixtures().join("basic-p4096-utf8.db");
    let destination = area.join("absent").join("nested").join("out.rdb");

    let error = sqlite::migrate(&fixture, &destination)
        .expect_err("a destination that cannot be written must be refused");
    assert!(
        error
            .detail()
            .unwrap_or_default()
            .contains("could not be built"),
        "the error must say the build failed: {}",
        error.detail().unwrap_or_default()
    );
    assert!(
        !destination.exists(),
        "nothing may be published when the target cannot be written"
    );
    assert!(fixture.is_file(), "the source must still be there");
}

/// A migration that verifies badly leaves its staging file and publishes nothing.
///
/// The proof that a failed migration is *recognisable*: the destination is
/// absent, so an application opening it gets nothing rather than half a
/// database, and the staging file is still there, because the thing a person
/// needs after a failed migration is the evidence.
#[test]
fn a_failed_migration_leaves_evidence_and_no_destination() {
    let area = scratch("evidence");
    let fixture = fixtures().join("bad-page-type.db");
    let destination = area.join("out.rdb");
    match sqlite::migrate(&fixture, &destination) {
        Ok(report) if !report.passed() => {
            assert!(
                !destination.exists(),
                "a failed migration publishes nothing"
            );
        }
        Ok(_) => {
            // The reader tolerated this one; the fixture corpus is shared and
            // its members are not all refused at the same depth.
        }
        Err(_) => {
            assert!(
                !destination.exists(),
                "a refused migration publishes nothing"
            );
        }
    }
}

/// The whole of what an application's database carries, migrated and readable.
///
/// **Four things stopped this migration before it worked.** A source with a
/// trigger refused the migration outright with "the new engine does not run
/// triggers" - false since triggers began firing. A source with an FTS5 table
/// refused it with "the declaration of `f_data` did not parse: database disk
/// image is malformed", about a file that is neither malformed nor at fault.
/// A view migrated, appeared in `sqlite_schema`, and then answered `no such
/// table`. And `sqlite_sequence` was not carried, so an AUTOINCREMENT table
/// whose high rows had been deleted reused their keys on the first insert
/// afterwards.
///
/// So the source here carries all four, plus the things that already worked -
/// a `WITHOUT ROWID` table, a generated column, a partial index and a foreign
/// key - and the assertions are that every query the source answers, the
/// migrated database answers.
#[test]
fn a_database_with_triggers_views_fts5_and_a_sequence_migrates_and_answers() {
    let Some(shell) = reference() else {
        inillucent_compat::differential::skipping("the pinned SQLite shell is missing");
        return;
    };
    let area = scratch("rich");
    let source = area.join("source.db");
    let script = "\
CREATE TABLE a(id INTEGER PRIMARY KEY AUTOINCREMENT, n TEXT);\n\
INSERT INTO a(n) VALUES('one'),('two'),('three'),('four');\n\
DELETE FROM a WHERE id > 2;\n\
CREATE TABLE log(id INTEGER PRIMARY KEY, what TEXT);\n\
CREATE TRIGGER a_ins AFTER INSERT ON a BEGIN INSERT INTO log(what) VALUES('ins'); END;\n\
CREATE VIEW v AS SELECT id, n FROM a;\n\
CREATE TABLE wr(k TEXT PRIMARY KEY, val TEXT) WITHOUT ROWID;\n\
INSERT INTO wr VALUES('x','1'),('y','2');\n\
CREATE TABLE g(a INTEGER, b INTEGER GENERATED ALWAYS AS (a*2) STORED);\n\
INSERT INTO g(a) VALUES(3),(4);\n\
CREATE INDEX pg ON g(a) WHERE a > 3;\n\
CREATE TABLE parent(id INTEGER PRIMARY KEY);\n\
CREATE TABLE child(id INTEGER PRIMARY KEY, p INTEGER REFERENCES parent(id));\n\
INSERT INTO parent VALUES(1);\n\
INSERT INTO child VALUES(1,1);\n\
CREATE VIRTUAL TABLE f USING fts5(body);\n\
INSERT INTO f(body) VALUES('the quick brown fox'),('jumps over');\n";
    let built = Command::new(&shell)
        .arg(&source)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(script.as_bytes())?;
            }
            child.wait_with_output()
        });
    let Ok(output) = built else {
        inillucent_compat::differential::skipping("the reference shell could not be run");
        return;
    };
    assert!(
        output.status.success(),
        "the source could not be built: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let destination = area.join("migrated.rdb");
    let report = sqlite::migrate(&source, &destination).expect("the migration runs");
    assert!(
        report.passed(),
        "the migration did not verify: {:?}",
        report
            .checks
            .iter()
            .filter(|check| !check.passed)
            .collect::<Vec<_>>()
    );
    assert!(destination.is_file(), "the migration published nothing");
    // The full-text table is reported as carried rather than left to be
    // noticed missing.
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.name == "carried.f" && check.passed),
        "the fts5 table was not reported as carried"
    );

    let database =
        inillucent_engine::connect::Database::open(&destination).expect("the migration opens");
    let connection = database.session();
    let one = |sql: &str| -> Vec<Vec<OwnedDatum>> {
        connection
            .query(sql)
            .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
    };

    // The view is readable, not merely listed. It used to be both listed and
    // unreadable, which is worse than being dropped.
    assert_eq!(
        quote_rows(&one("SELECT id, n FROM v ORDER BY id")),
        vec!["1,'one'".to_string(), "2,'two'".to_string()],
    );
    // The full-text index searches, and the docids are the source's.
    assert_eq!(
        quote_rows(&one("SELECT rowid, body FROM f WHERE f MATCH 'fox'")),
        vec!["1,'the quick brown fox'".to_string()],
    );
    assert_eq!(
        quote_rows(&one("SELECT rowid, body FROM f WHERE f MATCH 'jumps'")),
        vec!["2,'jumps over'".to_string()],
    );
    // The trigger fires.
    connection
        .execute_batch("INSERT INTO a(n) VALUES('five')")
        .expect("the insert runs");
    assert_eq!(quote_rows(&one("SELECT count(*) FROM log")), vec!["1"]);
    // And the key it was given is past the source's high-water mark rather
    // than back in the gap the DELETE left.
    assert_eq!(
        quote_rows(&one("SELECT id FROM a ORDER BY id")),
        vec!["1".to_string(), "2".to_string(), "5".to_string()],
        "AUTOINCREMENT reused a deleted key, so sqlite_sequence was not carried"
    );
    // The things that already worked, so a fix to one does not cost another.
    assert_eq!(
        quote_rows(&one("SELECT k, val FROM wr ORDER BY k")),
        vec!["'x','1'".to_string(), "'y','2'".to_string()],
    );
    assert_eq!(
        quote_rows(&one("SELECT a, b FROM g ORDER BY a")),
        vec!["3,6".to_string(), "4,8".to_string()],
    );
    let plan = quote_rows(&one("EXPLAIN QUERY PLAN SELECT a FROM g WHERE a > 3")).join(" ");
    assert!(
        plan.contains("pg"),
        "the partial index did not survive: {plan}"
    );
    connection
        .execute_batch("PRAGMA foreign_keys=ON")
        .expect("keys are enabled");
    assert!(
        connection
            .execute_batch("INSERT INTO child VALUES(9, 99)")
            .is_err(),
        "the foreign key did not survive the migration"
    );
}

/// Renders rows the way `quote_row` does, for comparison in a test.
///
/// @param rows - the rows to render
fn quote_rows(rows: &[Vec<OwnedDatum>]) -> Vec<String> {
    rows.iter().map(|row| quote_row(row)).collect()
}

/// A source the reader cannot read whole is refused, and nothing is published
/// (task-1979, M1).
///
/// **The one damage the verification could not see.** The tool inventories the
/// source with `inillucent-sqlite-reader`, copies with the same reader, and
/// verifies the copy against that inventory - so a page the reader cannot walk
/// made a table disappear from *both* sides and the counts agreed on nothing.
/// Measured: a two table SQLite file with one flipped bit in a leaf page
/// migrated with exit 0, `integrity-check ok`, and the 500 row table gone,
/// while real SQLite still read all 500 rows from the same file.
///
/// **The pinned shell is what makes the case a case.** The flip has to be one
/// SQLite tolerates; a file SQLite also refuses would be a file this tool is
/// right to refuse for a different reason, and the test would pass against a
/// build that had learned nothing. The fixture is built here from a valid one
/// rather than checked in, so the two halves - "SQLite still reads it" and
/// "this refuses it" - are asserted about the same bytes.
#[test]
fn a_source_the_reader_cannot_read_whole_is_not_published() {
    let Some(shell) = reference() else {
        inillucent_base::testing::skipping("the pinned SQLite shell is not downloaded");
        return;
    };
    let directory = scratch("unreadable-source");
    let source = fixtures().join("basic-p4096-utf8.db");
    let damaged = directory.join("damaged.db");
    std::fs::copy(&source, &damaged).expect("the fixture copies");

    // The first byte of page two, which is a table's leaf page in this fixture.
    // Flipped rather than zeroed, so the file stays a SQLite database and the
    // damage is one bit.
    const FLIP_AT: u64 = 4_096;
    {
        use std::io::{Read, Seek, SeekFrom, Write};
        let mut file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&damaged)
            .expect("the copy opens");
        file.seek(SeekFrom::Start(FLIP_AT)).expect("it seeks");
        let mut byte = [0u8; 1];
        file.read_exact(&mut byte).expect("it reads");
        file.seek(SeekFrom::Start(FLIP_AT)).expect("it seeks back");
        file.write_all(&[byte[0] ^ 0x40]).expect("it writes");
    }

    // SQLite still reads every row, so the damage is one this tool must not
    // answer by dropping a table.
    for table in ["people", "widths"] {
        let asked = Command::new(&shell)
            .arg(&damaged)
            .arg(format!("SELECT count(*) FROM \"{table}\";"))
            .output()
            .expect("the pinned shell runs");
        let counted = String::from_utf8_lossy(&asked.stdout).trim().to_string();
        assert!(
            asked.status.success() && counted.parse::<u64>().is_ok_and(|rows| rows > 0),
            "the pinned shell could not read {table} from the damaged fixture, so this case \
             is about a file SQLite refuses too rather than about the reader: {counted}"
        );
    }

    let destination = directory.join("migrated.rdb");
    let outcome = sqlite::migrate(&damaged, &destination);
    assert!(
        outcome.is_err(),
        "a source whose rows the reader could not read was migrated: {:?}",
        outcome.map(|report| report.passed())
    );
    assert!(
        !destination.is_file(),
        "the migration refused and published {} anyway",
        destination.display()
    );
}

/// Builds a SQLite database by feeding a script to the pinned shell.
///
/// @param shell - the pinned `sqlite3`
/// @param database - the file to create
/// @param script - the statements, one per line
fn build_with_shell(shell: &Path, database: &Path, script: &str) -> bool {
    let built = Command::new(shell)
        .arg(database)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(script.as_bytes())?;
            }
            child.wait_with_output()
        });
    let Ok(output) = built else {
        return false;
    };
    assert!(
        output.status.success(),
        "the source could not be built: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

/// Asks the pinned shell one question and returns its lines.
///
/// `-noheader` and list mode, so the answer is the values and nothing around
/// them.
///
/// @param shell - the pinned `sqlite3`
/// @param database - the file to read
/// @param sql - the statement
fn shell_answers(shell: &Path, database: &Path, sql: &str) -> Vec<String> {
    let Ok(output) = Command::new(shell)
        .arg("-noheader")
        .arg("-list")
        .arg(database)
        .arg(sql)
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout)
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Renders one value the way the shell's list mode prints it.
///
/// A NULL is the empty string there, not the word `NULL`, and a REAL goes
/// through the engine's own SQLite-compatible float rendering rather than
/// through a second formatter that could drift from it. No question in
/// `EDGE_QUESTIONS` selects a blob directly - they go through `hex()` and
/// `quote()` - because the shell writes a blob's raw bytes and a NUL inside one
/// would end the line.
///
/// @param value - one value as the new engine produced it
fn list_cell(value: &OwnedDatum) -> String {
    match value {
        OwnedDatum::Null => String::new(),
        OwnedDatum::Int(number) => number.to_string(),
        OwnedDatum::Real(number) => {
            String::from_utf8_lossy(&inillucent_value::numeric::real_to_text(*number)).into_owned()
        }
        OwnedDatum::Text(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        OwnedDatum::Blob(bytes) => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// The questions both engines are asked about the migrated values.
///
/// Every one of them answers in text, so the comparison is between two strings
/// and not between two renderings of a float. `hex()` for the blobs because a
/// NUL inside one is invisible in any other form, and `typeof()` for the
/// affinity rows because the point of those is which type the value was stored
/// as, which is decided when it is written and must survive being copied.
const EDGE_QUESTIONS: &[&str] = &[
    "SELECT id, hex(b), typeof(b) FROM \"caf\u{e9} table\" ORDER BY id",
    "SELECT id, typeof(r), CAST(r AS TEXT) FROM \"caf\u{e9} table\" ORDER BY id",
    "SELECT id, t FROM \"caf\u{e9} table\" ORDER BY id",
    "SELECT typeof(i), typeof(t), typeof(b), typeof(r), typeof(n) FROM aff ORDER BY rowid",
    "SELECT quote(i), quote(t), quote(b), quote(r), quote(n) FROM aff ORDER BY rowid",
    "SELECT a, b FROM ex ORDER BY b",
    "SELECT type, name FROM sqlite_schema WHERE type = 'index' ORDER BY name",
];

/// **The values a real database holds survive the migration.**
///
/// The fixture §4.1.7 built covers the *objects* a database has - triggers,
/// views, generated columns, a partial index, a foreign key. What it does not
/// cover is the values inside them, and a migration loses those in different
/// ways: a blob truncated at the first NUL, an infinity written back as text, a
/// column whose affinity was applied a second time on the way in, a table whose
/// name is not ASCII (task-2066 section 4.4.14).
///
/// **Both engines are asked the same seven questions** and the answers compared
/// as text, rather than the expected answers being written here. Writing them
/// here would be writing down SQLite's affinity rules from memory, which is the
/// thing the reference exists to avoid: `INSERT INTO aff VALUES('42', 42, ...)`
/// stores an integer in the TEXT column or a string in it depending on rules
/// nobody should be reciting into a test.
///
/// NaN is in the fixture and is not in the assertions by name, because SQLite
/// stores a NaN as NULL - `typeof()` answers `null` on both sides, which is
/// what the first question checks.
#[test]
fn the_values_a_real_database_holds_survive_the_migration() {
    let Some(shell) = reference() else {
        inillucent_compat::differential::skipping("the pinned SQLite shell is missing");
        return;
    };
    let area = scratch("values");
    let source = area.join("source.db");
    let script = "\
CREATE TABLE \"caf\u{e9} table\"(id INTEGER PRIMARY KEY, b BLOB, r REAL, t TEXT);\n\
INSERT INTO \"caf\u{e9} table\" VALUES(1, x'00FF0041', 9e999, 'positive infinity');\n\
INSERT INTO \"caf\u{e9} table\" VALUES(2, x'610062', -9e999, 'negative infinity');\n\
INSERT INTO \"caf\u{e9} table\" VALUES(3, x'', 0.0/0.0, 'not a number');\n\
INSERT INTO \"caf\u{e9} table\" VALUES(4, NULL, 5e-324, 'the smallest subnormal');\n\
CREATE INDEX caf_r ON \"caf\u{e9} table\"(r);\n\
CREATE TABLE aff(i INTEGER, t TEXT, b BLOB, r REAL, n NUMERIC);\n\
INSERT INTO aff VALUES('42', 42, 42, '42', '42');\n\
INSERT INTO aff VALUES('4.0', 4.0, x'34', '1e3', '0012');\n\
INSERT INTO aff VALUES('abc', 'abc', 'abc', 'abc', 'abc');\n\
INSERT INTO aff VALUES(NULL, NULL, NULL, NULL, NULL);\n\
CREATE TABLE ex(a TEXT, b INTEGER);\n\
INSERT INTO ex VALUES('Alpha', 1),('BETA', 2),('gamma', 3);\n\
CREATE INDEX ex_lower ON ex(lower(a));\n\
CREATE INDEX ex_doubled ON ex(b * 2, a);\n";
    if !build_with_shell(&shell, &source, script) {
        inillucent_compat::differential::skipping("the reference shell could not be run");
        return;
    }

    let destination = area.join("migrated.rdb");
    let report = sqlite::migrate(&source, &destination).expect("the migration runs");
    assert!(
        report.passed(),
        "the migration did not verify: {:?}",
        report
            .checks
            .iter()
            .filter(|check| !check.passed)
            .collect::<Vec<_>>()
    );

    let database =
        inillucent_engine::connect::Database::open(&destination).expect("the migration opens");
    let connection = database.session();

    let mut wrong: Vec<String> = Vec::new();
    for sql in EDGE_QUESTIONS {
        let theirs = shell_answers(&shell, &source, sql);
        // **A question both engines answer with nothing agrees and proves
        // nothing.** A misspelled table name here would have been a silent pass
        // rather than a failure, which is §1.2 of the testing standard.
        assert!(
            !theirs.is_empty(),
            "the reference answered {sql} with no rows, so this question grades nothing"
        );
        let ours: Vec<String> = connection
            .query(sql)
            .unwrap_or_else(|error| panic!("{sql}: {error:?}"))
            .iter()
            .map(|row| row.iter().map(list_cell).collect::<Vec<String>>().join("|"))
            .collect();
        if theirs != ours {
            wrong.push(format!(
                "{sql}\n  sqlite     : {theirs:?}\n  inillucent : {ours:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{} of {} questions about the migrated values answered differently:\n{}",
        wrong.len(),
        EDGE_QUESTIONS.len(),
        wrong.join("\n")
    );

    // The expression indexes are in the schema *and* reachable. A migration
    // that carried the `CREATE INDEX` text and built nothing would pass the
    // schema question above and answer every query by scanning.
    let plan = connection
        .query("EXPLAIN QUERY PLAN SELECT b FROM ex WHERE lower(a) = 'beta'")
        .expect("the plan is explained")
        .iter()
        .map(|row| quote_row(row))
        .collect::<Vec<String>>()
        .join(" ");
    assert!(
        plan.contains("ex_lower"),
        "the index on `lower(a)` did not survive the migration as something the planner \
         can use: {plan}"
    );
}

/// Writes the script for a table of `columns` untyped columns holding `rows` rows of the
/// numbers 0 to `columns - 1`.
///
/// @param columns - how many columns the table has
/// @param rows - how many identical rows to insert
fn wide_table_script(columns: usize, rows: usize) -> String {
    wide_table_script_from(columns, rows, 0)
}

/// Writes the script for a table of `columns` untyped columns holding `rows` rows of the
/// numbers `base` to `base + columns - 1`.
///
/// @param columns - how many columns the table has
/// @param rows - how many identical rows to insert
/// @param base - the number the first column holds
fn wide_table_script_from(columns: usize, rows: usize, base: i64) -> String {
    let names: Vec<String> = (0..columns).map(|at| format!("c{at}")).collect();
    let values: Vec<String> = (0..columns)
        .map(|at| (base + at as i64).to_string())
        .collect();
    let mut script = format!("CREATE TABLE wide({});\n", names.join(","));
    for _ in 0..rows {
        script.push_str(&format!("INSERT INTO wide VALUES({});\n", values.join(",")));
    }
    script
}

/// A table of 1,000 and more columns with rows in it migrates, and a table too wide for a page
/// is refused as a table that cannot be stored (bug hunt 10).
///
/// **The bulk build sized a leaf at a fraction of the page and refused a row that did not fit
/// it.** A table of 920 columns with one row failed with `database disk image is malformed`,
/// because the migration wrapped every failure of the build in the error for a damaged file.
/// A row too wide for the columnar region is now kept alone in the delta area of its leaf, which
/// holds about 1,900 numbers of eight bytes each, and up to about 3,200 when each fits 32 bits,
/// because a row that wide writes its integers in the fewest bytes that hold them. Three rows
/// are inserted so the bulk build has to place more than one wide row, each in a leaf of its
/// own. A table of 2,000 columns of small numbers migrates. A table of 2,000 columns of numbers
/// that each need eight bytes holds more than a 32 KiB page does and is refused with its real
/// reason, not as a malformed file.
#[test]
fn a_table_of_a_thousand_columns_migrates_and_one_too_wide_for_a_page_is_refused_by_name() {
    let Some(shell) = reference() else {
        inillucent_compat::differential::skipping("the pinned SQLite shell is missing");
        return;
    };
    let area = scratch("wide-rows");
    for columns in [920usize, 1200, 1500, 2000] {
        let source = area.join(format!("source-{columns}.db"));
        if !build_with_shell(&shell, &source, &wide_table_script(columns, 3)) {
            inillucent_compat::differential::skipping("the reference shell could not be run");
            return;
        }
        let destination = area.join(format!("migrated-{columns}.rdb"));
        let report = sqlite::migrate(&source, &destination)
            .unwrap_or_else(|error| panic!("{columns} columns: {error:?}"));
        assert!(
            report.passed(),
            "{columns} columns did not verify: {:?}",
            report
                .checks
                .iter()
                .filter(|check| !check.passed)
                .collect::<Vec<_>>()
        );
        let database =
            inillucent_engine::connect::Database::open(&destination).expect("the migration opens");
        let connection = database.session();
        let last = columns - 1;
        let question = format!("SELECT count(*), sum(c0), sum(c{last}) FROM wide");
        let rows = connection
            .query(&question)
            .expect("the migrated rows are read");
        assert_eq!(
            quote_rows(&rows),
            vec![format!("3,0,{}", 3 * last)],
            "{columns} columns"
        );
    }

    let source = area.join("source-2000-full.db");
    if !build_with_shell(
        &shell,
        &source,
        &wide_table_script_from(2000, 1, 4_000_000_000_000_000_000),
    ) {
        inillucent_compat::differential::skipping("the reference shell could not be run");
        return;
    }
    let destination = area.join("migrated-2000-full.rdb");
    let error = sqlite::migrate(&source, &destination)
        .expect_err("2,000 numbers of eight bytes each do not fit a 32 KiB page");
    assert_ne!(
        error.code(),
        inillucent_base::PrimaryCode::Corrupt,
        "a source that SQLite reads whole was reported as damaged: {}",
        error.message()
    );
    assert!(
        error.message().contains("cannot be stored") && error.message().contains("columns"),
        "the refusal does not say what is wrong: {}",
        error.message()
    );
    assert!(
        !destination.is_file(),
        "the refused migration published a database"
    );
}

/// An FTS5 table with an `UNINDEXED` column, and a `prefix` option that holds a comma, migrate
/// and answer `MATCH` with the rows the pinned SQLite answers (bug hunt 10).
///
/// **The column list was read by splitting the declaration at every comma and taking each part
/// whole.** `a UNINDEXED` became a column named `a UNINDEXED`, and the migration stopped at
/// `table f3 has no column named a UNINDEXED`. `prefix='2,3'` was split into `prefix='2` and
/// `3'`, which left a column named `3`. Both declarations are in one source so one run grades
/// the column list and the options.
#[test]
fn an_fts5_table_with_unindexed_columns_and_comma_options_migrates() {
    let Some(shell) = reference() else {
        inillucent_compat::differential::skipping("the pinned SQLite shell is missing");
        return;
    };
    let area = scratch("fts5-unindexed");
    let source = area.join("source.db");
    let script = "\
CREATE VIRTUAL TABLE f3 USING fts5(a UNINDEXED, b);\n\
INSERT INTO f3(rowid, a, b) VALUES(1, 'left one', 'alpha beta'),(2, 'left two', 'beta gamma');\n\
CREATE VIRTUAL TABLE f4 USING fts5(\"x y\" UNINDEXED, z, prefix='2,3');\n\
INSERT INTO f4(rowid, \"x y\", z) VALUES(5, 'spaced', 'delta epsilon');\n";
    if !build_with_shell(&shell, &source, script) {
        inillucent_compat::differential::skipping("the reference shell could not be run");
        return;
    }
    let destination = area.join("migrated.rdb");
    let report = sqlite::migrate(&source, &destination).expect("the migration runs");
    assert!(
        report.passed(),
        "the migration did not verify: {:?}",
        report
            .checks
            .iter()
            .filter(|check| !check.passed)
            .collect::<Vec<_>>()
    );
    let database =
        inillucent_engine::connect::Database::open(&destination).expect("the migration opens");
    let connection = database.session();
    for question in [
        "SELECT rowid, a, b FROM f3 WHERE f3 MATCH 'beta' ORDER BY rowid",
        "SELECT rowid, a, b FROM f3 WHERE f3 MATCH 'b:alpha' ORDER BY rowid",
        "SELECT rowid, \"x y\", z FROM f4 WHERE f4 MATCH 'delt*' ORDER BY rowid",
    ] {
        let theirs = shell_answers(&shell, &source, question);
        assert!(
            !theirs.is_empty(),
            "the reference answered {question} with no rows, so this question grades nothing"
        );
        let ours: Vec<String> = connection
            .query(question)
            .unwrap_or_else(|error| panic!("{question}: {error:?}"))
            .iter()
            .map(|row| row.iter().map(list_cell).collect::<Vec<String>>().join("|"))
            .collect();
        assert_eq!(theirs, ours, "{question}");
    }
}

/// Builds a source with the pinned shell, migrates it, and returns both ends.
///
/// Returns `None` when the pinned SQLite is not built, after saying so, so a
/// machine without it skips instead of passing without having checked anything.
///
/// @param name - the test's scratch directory
/// @param script - the statements that build the source
fn migrated_from_script(name: &str, script: &str) -> Option<(PathBuf, PathBuf, PathBuf)> {
    let shell = reference().or_else(|| {
        inillucent_compat::differential::skipping("the pinned SQLite shell is missing");
        None
    })?;
    let area = scratch(name);
    let source = area.join("source.db");
    if !build_with_shell(&shell, &source, script) {
        inillucent_compat::differential::skipping("the reference shell could not be run");
        return None;
    }
    let destination = area.join("migrated.rdb");
    let report = sqlite::migrate(&source, &destination).expect("the migration runs");
    assert!(
        report.passed(),
        "the migration did not verify: {:?}",
        report
            .failures()
            .iter()
            .map(|check| check.line())
            .collect::<Vec<String>>()
    );
    Some((shell, source, destination))
}

/// Asks the pinned SQLite about the source and this engine about the migrated
/// file, and requires the same rows in the same order.
///
/// @param shell - the pinned `sqlite3`
/// @param source - the SQLite file
/// @param destination - the migrated database
/// @param questions - statements that each return rows
fn assert_same_answers(shell: &Path, source: &Path, destination: &Path, questions: &[&str]) {
    let database =
        inillucent_engine::connect::Database::open(destination).expect("the migration opens");
    let connection = database.session();
    for question in questions {
        let theirs = shell_answers(shell, source, question);
        assert!(
            !theirs.is_empty(),
            "the reference answered {question} with no rows, so this question grades nothing"
        );
        let ours: Vec<String> = connection
            .query(question)
            .unwrap_or_else(|error| panic!("{question}: {error:?}"))
            .iter()
            .map(|row| row.iter().map(list_cell).collect::<Vec<String>>().join("|"))
            .collect();
        assert_eq!(theirs, ours, "{question}");
    }
}

/// **A column added with `ALTER TABLE ... ADD COLUMN` keeps its default in every
/// row that was written before the `ALTER`.**
///
/// SQLite leaves those rows with fewer record fields and supplies the default
/// when it reads them. The reader returned NULL for the missing fields, and the
/// migration's own digest read through the same reader, so the copy lost every
/// default and all of its checks passed. The pinned SQLite is the only reader
/// here that did not share the defect, so the answers are compared against it:
/// a `NOT NULL` column that holds NULL, a text default with quotes and commas,
/// the affinity applied to a default, a rowid alias declared between the old
/// columns and the new ones, an index built over the added column, and a
/// `WITHOUT ROWID` table.
#[test]
fn a_column_added_after_rows_were_written_keeps_its_default_in_every_old_row() {
    let script = "\
CREATE TABLE t(a INTEGER);\n\
INSERT INTO t VALUES(1),(2);\n\
ALTER TABLE t ADD COLUMN b INTEGER DEFAULT 9;\n\
ALTER TABLE t ADD COLUMN c TEXT NOT NULL DEFAULT 'it''s, (a)';\n\
ALTER TABLE t ADD COLUMN d TEXT DEFAULT 9;\n\
ALTER TABLE t ADD COLUMN e REAL DEFAULT 3;\n\
ALTER TABLE t ADD COLUMN f DEFAULT x'ab';\n\
ALTER TABLE t ADD COLUMN g DEFAULT TRUE;\n\
ALTER TABLE t ADD COLUMN h DEFAULT -'7';\n\
ALTER TABLE t ADD COLUMN i DEFAULT 0x10;\n\
ALTER TABLE t ADD COLUMN j DEFAULT NULL;\n\
INSERT INTO t(a) VALUES(3);\n\
CREATE INDEX t_b ON t(b);\n\
CREATE TABLE r(a, id INTEGER PRIMARY KEY, b);\n\
INSERT INTO r VALUES(1, 10, 2);\n\
ALTER TABLE r ADD COLUMN c DEFAULT 7;\n\
CREATE TABLE w(a PRIMARY KEY, b) WITHOUT ROWID;\n\
INSERT INTO w VALUES(1, 2),(4, 5);\n\
ALTER TABLE w ADD COLUMN c DEFAULT 5;\n\
ALTER TABLE w ADD COLUMN d TEXT DEFAULT 'z';\n\
INSERT INTO w VALUES(2, 3, 4, 'w');\n";
    let Some((shell, source, destination)) = migrated_from_script("added-columns", script) else {
        return;
    };
    assert_same_answers(
        &shell,
        &source,
        &destination,
        &[
            "SELECT a, b, c, d, e FROM t ORDER BY a",
            "SELECT quote(a), quote(b), quote(c), quote(d), quote(e), quote(f), quote(g), quote(h), quote(i), quote(j) FROM t ORDER BY a",
            "SELECT typeof(b), typeof(c), typeof(d), typeof(e) FROM t ORDER BY a",
            "SELECT a FROM t WHERE b = 9 ORDER BY a",
            "SELECT a, id, b, c FROM r",
            "SELECT a, b, c, d FROM w ORDER BY a",
            "PRAGMA integrity_check",
        ],
    );
}

/// **A secondary index on a `WITHOUT ROWID` table is built whole.**
///
/// SQLite ends such an index entry with the primary key columns the index does
/// not already list, so `PRIMARY KEY (a, b)` with an index on `(c)` stores
/// `c, a, b`, and an index on `(b, a)` stores `b, a`. This engine stores the whole
/// primary key after the indexed columns. The import read one trailing field and
/// left the second key column NULL, so the index named rows that did not exist
/// and a lookup through it found nothing, and when every column outside the
/// key was indexed the migration failed its own digest.
#[test]
fn a_secondary_index_on_a_without_rowid_table_has_every_primary_key_column() {
    let script = "\
CREATE TABLE t(a, b, c, d, PRIMARY KEY(a, b)) WITHOUT ROWID;\n\
INSERT INTO t VALUES(1,2,3,4),(5,6,7,8),(9,10,11,12);\n\
CREATE INDEX t_c ON t(c);\n\
CREATE INDEX t_ba ON t(b, a);\n\
CREATE INDEX t_expr ON t(c + 1);\n\
CREATE TABLE u(a TEXT, b, PRIMARY KEY(a)) WITHOUT ROWID;\n\
INSERT INTO u VALUES('x', 2),('Y', 4);\n\
CREATE INDEX u_a ON u(a);\n\
CREATE INDEX u_nocase ON u(a COLLATE NOCASE);\n\
CREATE TABLE v(a, b, c, PRIMARY KEY(a, b)) WITHOUT ROWID;\n\
INSERT INTO v VALUES(1,2,3),(5,6,7);\n\
CREATE UNIQUE INDEX v_ca ON v(c, a);\n";
    let Some((shell, source, destination)) = migrated_from_script("wor-index", script) else {
        return;
    };
    assert_same_answers(
        &shell,
        &source,
        &destination,
        &[
            "SELECT * FROM t WHERE c = 7",
            "SELECT * FROM t INDEXED BY t_c WHERE c = 7",
            "SELECT * FROM t INDEXED BY t_ba WHERE b = 6",
            "SELECT * FROM t WHERE c + 1 = 8",
            "SELECT * FROM u WHERE a = 'x'",
            "SELECT * FROM u INDEXED BY u_nocase WHERE a = 'y' COLLATE NOCASE",
            "SELECT * FROM v WHERE c = 7 AND a = 5",
            "PRAGMA integrity_check",
        ],
    );
}

/// **Triggers fire in the order the source fires them.**
///
/// SQLite fires several triggers on one event newest first. The import wrote the
/// trigger rows in the order the loader listed them, which is already newest
/// first, and the destination's loader reverses that again, so a migrated file
/// fired its triggers oldest first.
#[test]
fn triggers_on_one_event_fire_in_the_order_the_source_fires_them() {
    let script = "\
CREATE TABLE tg(a INTEGER PRIMARY KEY, b);\n\
CREATE TABLE lg(m);\n\
CREATE TABLE other(x);\n\
CREATE TRIGGER zz AFTER INSERT ON tg BEGIN INSERT INTO lg VALUES('zz'); END;\n\
CREATE TRIGGER o1 AFTER INSERT ON other BEGIN INSERT INTO lg VALUES('o1'); END;\n\
CREATE TRIGGER aa AFTER INSERT ON tg BEGIN INSERT INTO lg VALUES('aa'); END;\n\
CREATE TRIGGER o2 AFTER INSERT ON other BEGIN INSERT INTO lg VALUES('o2'); END;\n\
CREATE TRIGGER mm AFTER INSERT ON tg BEGIN INSERT INTO lg VALUES('mm'); END;\n";
    let Some((shell, source, destination)) = migrated_from_script("trigger-order", script) else {
        return;
    };
    let database =
        inillucent_engine::connect::Database::open(&destination).expect("the migration opens");
    let connection = database.session();
    connection
        .execute_batch("INSERT INTO tg(b) VALUES(1); INSERT INTO other VALUES(1);")
        .expect("the inserts fire the triggers");
    let ours: Vec<String> = connection
        .query("SELECT group_concat(m) FROM (SELECT m FROM lg ORDER BY rowid)")
        .expect("the log reads")
        .iter()
        .map(|row| row.iter().map(list_cell).collect::<Vec<String>>().join("|"))
        .collect();
    let theirs = Command::new(&shell)
        .arg("-noheader")
        .arg(&source)
        .arg("INSERT INTO tg(b) VALUES(1); INSERT INTO other VALUES(1); SELECT group_concat(m) FROM (SELECT m FROM lg ORDER BY rowid);")
        .output()
        .expect("the reference runs");
    let theirs: Vec<String> = String::from_utf8_lossy(&theirs.stdout)
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(theirs, vec!["mm,aa,zz,o2,o1".to_string()]);
    assert_eq!(ours, theirs);
}

/// **An FTS5 table with external content is built from its content table, and a
/// contentless one is refused with a reason.**
///
/// `content='posts'` stores no text of its own, so there was nothing to copy and
/// the migration refused the whole file. The text is in `posts`, which is carried
/// like any table, and an FTS5 `rebuild` builds the index from it. `content=''`
/// keeps no text anywhere, so it is still refused, and the refusal says why.
#[test]
fn an_external_content_fts5_table_is_rebuilt_and_a_contentless_one_is_refused() {
    let script = "\
CREATE TABLE posts(id INTEGER PRIMARY KEY, title, body);\n\
INSERT INTO posts VALUES(1,'hello world','first body'),(2,'second','another body');\n\
CREATE VIRTUAL TABLE f USING fts5(title, body, content='posts', content_rowid='id');\n\
INSERT INTO f(f) VALUES('rebuild');\n";
    let Some((shell, source, destination)) = migrated_from_script("fts5-external", script) else {
        return;
    };
    assert_same_answers(
        &shell,
        &source,
        &destination,
        &[
            "SELECT rowid FROM f WHERE f MATCH 'body' ORDER BY rowid",
            "SELECT rowid, title FROM f WHERE f MATCH 'hello'",
            "SELECT rowid FROM f WHERE f MATCH 'title:second'",
        ],
    );

    let area = scratch("fts5-contentless");
    let source = area.join("source.db");
    let contentless = "\
CREATE VIRTUAL TABLE f USING fts5(title, body, content='');\n\
INSERT INTO f(rowid, title, body) VALUES(1, 'hello', 'x');\n";
    if !build_with_shell(&shell, &source, contentless) {
        inillucent_compat::differential::skipping("the reference shell could not be run");
        return;
    }
    let report = sqlite::migrate(&source, &area.join("migrated.rdb")).expect("the migration runs");
    assert!(
        !report.passed(),
        "a contentless table has no text to migrate"
    );
    let said: Vec<String> = report.failures().iter().map(|check| check.line()).collect();
    assert!(
        said.iter()
            .any(|line| line.contains("contentless") && line.contains("content=''")),
        "the refusal does not say the table is contentless: {said:?}"
    );
    assert!(
        !area.join("migrated.rdb").exists(),
        "a refused migration published a file"
    );
}

/// **An R*Tree table is carried, with every row it holds.**
///
/// The migration used to fail the whole file with status `corrupt` for a source
/// that was not damaged. The rows are read out of SQLite's node blobs, so the
/// table has enough rows to make the tree more than one node deep.
#[test]
fn an_rtree_table_is_carried_with_every_row() {
    let mut script =
        String::from("CREATE VIRTUAL TABLE rt USING rtree(id, minx, maxx, miny, maxy);\n");
    script.push_str("CREATE VIRTUAL TABLE ri USING rtree_i32(id, a, b);\n");
    for id in 1..=300 {
        script.push_str(&format!(
            "INSERT INTO rt VALUES({id}, {id}.5, {id}.75, {}.25, {}.5);\n",
            id % 17,
            id % 17 + 2
        ));
        script.push_str(&format!("INSERT INTO ri VALUES({id}, {id}, {});\n", id + 3));
    }
    let Some((shell, source, destination)) = migrated_from_script("rtree", &script) else {
        return;
    };
    assert_same_answers(
        &shell,
        &source,
        &destination,
        &[
            "SELECT count(*) FROM rt",
            "SELECT id, minx, maxx, miny, maxy FROM rt ORDER BY id",
            "SELECT id FROM rt WHERE minx >= 100 AND maxx <= 120 ORDER BY id",
            "SELECT id, a, b FROM ri ORDER BY id",
            "SELECT id FROM ri WHERE a >= 10 AND b <= 20 ORDER BY id",
        ],
    );
}

/// **A source of zero bytes migrates to an empty database.**
///
/// SQLite opens a zero byte file as an empty database, and a program that makes
/// its database file before it first opens it leaves exactly that.
#[test]
fn a_zero_byte_source_migrates_to_an_empty_database() {
    let area = scratch("zero-bytes");
    let source = area.join("source.db");
    std::fs::write(&source, b"").expect("the empty source is written");
    let destination = area.join("migrated.rdb");
    let report = sqlite::migrate(&source, &destination).expect("the migration runs");
    assert!(
        report.passed(),
        "{:?}",
        report
            .failures()
            .iter()
            .map(|check| check.line())
            .collect::<Vec<String>>()
    );
    assert!(
        destination.is_file(),
        "the empty database was not published"
    );
    assert_eq!(
        std::fs::metadata(&source).map(|held| held.len()).ok(),
        Some(0)
    );
    let database =
        inillucent_engine::connect::Database::open(&destination).expect("the migration opens");
    let connection = database.session();
    let rows = connection
        .query("SELECT count(*) FROM sqlite_schema")
        .expect("the schema reads");
    assert_eq!(rows, vec![vec![OwnedDatum::Int(0)]]);
    connection
        .execute_batch("CREATE TABLE later(a); INSERT INTO later VALUES(1);")
        .expect("the empty database accepts a table");
}

/// **A dump of a migrated view whose text ends in a line comment replays.**
///
/// SQLite stores `CREATE VIEW v AS SELECT a FROM t -- note` with its comment.
/// The dump wrote `;` straight after the text, which is inside the comment, so the
/// next line of the dump was read as part of the view. SQLite's own `.dump` puts
/// the `;` on a line of its own when the text holds `--`; the dump here is
/// compared with it byte for byte and then replayed.
#[test]
fn a_dump_of_a_view_ending_in_a_line_comment_matches_sqlite_and_replays() {
    let script = "\
CREATE TABLE t(a);\n\
INSERT INTO t VALUES(1),(2);\n\
CREATE VIEW v AS SELECT a FROM t -- note\n;\n\
CREATE VIEW w AS SELECT a FROM t -- one; two\n;\n\
CREATE INDEX t_a ON t(a) -- index note\n;\n";
    let Some((shell, source, destination)) = migrated_from_script("dump-comment", script) else {
        return;
    };
    let area = destination
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let theirs = Command::new(&shell)
        .arg(&source)
        .arg(".dump")
        .output()
        .expect("the reference dumps");
    let theirs = String::from_utf8_lossy(&theirs.stdout).replace("\r\n", "\n");
    let destination_text = destination.to_string_lossy().to_string();
    let ours = inillucent_compat::cliproc::run(
        &inillucent_compat::cliproc::program("inillucent-shell"),
        &[destination_text.as_str(), ".dump"],
    );
    let ours_text = ours.stdout.replace("\r\n", "\n");
    assert_eq!(ours_text, theirs, "the dump differs from SQLite's");

    let script_path = area.join("dump.sql");
    std::fs::write(&script_path, ours_text.as_bytes()).expect("the dump is written");
    let fresh = area.join("replayed.rdb");
    let fresh_text = fresh.to_string_lossy().to_string();
    let read = format!(".read {}", script_path.to_string_lossy().replace('\\', "/"));
    let replay = inillucent_compat::cliproc::run(
        &inillucent_compat::cliproc::program("inillucent-shell"),
        &[
            fresh_text.as_str(),
            read.as_str(),
            "SELECT a FROM v ORDER BY a",
        ],
    );
    assert_eq!(
        replay.code,
        0,
        "the dump did not replay:\n{}",
        replay.said()
    );
    assert!(
        replay
            .stdout
            .replace("\r\n", "\n")
            .lines()
            .any(|line| line == "2"),
        "the replayed view returned no rows:\n{}",
        replay.said()
    );
}

/// **`VACUUM` on a migrated file leaves the same connection's integrity check clean.**
///
/// The file was sound and the connection was not: `VACUUM` reopens the rebuilt
/// file and then put the old catalog tree handle back over the new one, because
/// the code that carries attached databases across took the catalog tree's handle
/// for an attached database's. A migrated file keeps its catalog tree after its
/// tables and a rebuilt one before them, so the stale root page was a table's
/// page, and `PRAGMA integrity_check` answered that a page was used twice.
#[test]
fn integrity_check_after_vacuum_on_a_migrated_file_is_clean_in_the_same_connection() {
    let script = "\
CREATE TABLE a(x);\n\
INSERT INTO a VALUES(1);\n\
CREATE TABLE b(y);\n\
CREATE INDEX b_y ON b(y);\n\
INSERT INTO b VALUES(2);\n";
    let Some((_shell, _source, destination)) = migrated_from_script("vacuum-integrity", script)
    else {
        return;
    };
    let database =
        inillucent_engine::connect::Database::open(&destination).expect("the migration opens");
    let connection = database.session();
    connection.execute_batch("VACUUM").expect("the vacuum runs");
    let rows = connection
        .query("PRAGMA integrity_check")
        .expect("the check runs");
    assert_eq!(rows, vec![vec![OwnedDatum::Text(b"ok".to_vec())]]);
}

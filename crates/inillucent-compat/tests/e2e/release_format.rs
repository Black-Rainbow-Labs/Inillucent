//! The current build reading databases that earlier releases wrote.
//!
//! Invariant: **a file written by any published release from 0.1.1 on still
//! opens, still verifies, still answers every question the same way, and still
//! takes a write.** A format change that breaks one of those is a change that
//! costs an application its database, and the only way to know is to have that
//! release write the file rather than to reason about the format.
//!
//! ## What the fixtures are, and what a release cannot choose
//!
//! `tools/build-interop-fixture.ps1 <version>` downloads that release, verifies
//! it against the published `SHA256SUMS` and its minisign signature, runs
//! `tests/interop/build.sql` with that release's own `inillucent.exe`, and
//! checks in what it produced. Six releases have a fixture: 0.1.1, 0.1.2,
//! 0.1.3, 0.1.5, 0.1.6 and 0.1.7. There is no 0.1.4 - it was never published.
//!
//! The design asked for a fixture at 4,096 bytes and one at 32,768. **A
//! released binary cannot choose a page size**: it is an argument to
//! `Database::open_at`, the command line has no flag for it, and
//! `PRAGMA page_size` reports the page size rather than setting one. So every
//! fixture is at the engine's own 32,768, and the smaller page is covered where
//! it can be - by `matrix.rs`'s `sqlite_page` arm, which drives the library
//! directly. `tests/interop/README.md` records this.
//!
//! ## The row that only the log holds
//!
//! Each fixture was written, checkpointed, and then written to once more. That
//! last row is in `app.rdb-wal.*` and nowhere else, so a build that opened the
//! database and ignored the log would answer 120 rows where the fixture says
//! 121. Replaying an old release's log is the part of the format most likely to
//! move and the part an ordinary read would never exercise.
//!
//! ## Quick and full
//!
//! The quick form reads the newest fixture, which is the case a change to the
//! storage layer breaks first, and runs in under a second.
//! `INILLUCENT_SCENARIO=full` reads every fixture. The backward direction -
//! an earlier release opening a file this build wrote - is
//! `release_format_history.rs`, in the `nightly` tier, because it needs a
//! downloaded release binary that a fresh clone does not have.

use std::path::{Path, PathBuf};

use inillucent_compat::interop;
use inillucent_compat::matrix::{default_arm, Scale};
use inillucent_compat::stories::{ask, open, reopen_and_check, run};
use inillucent_compat::workspace_root;

/// Returns an empty scratch directory to stage a fixture in.
///
/// @param name - what to call it
fn scratch(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("target")
        .join("scratch")
        .join("release-format")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Returns the format version a release's fixture carries, from byte 8 of its file.
///
/// @param version - the release that wrote the fixture
fn fixture_format(version: &str) -> u32 {
    let path = interop::directory().join(version).join("app.rdb");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|why| panic!("{} could not be read: {why}", path.display()));
    let mut four = [0u8; 4];
    four.copy_from_slice(bytes.get(8..12).expect("a fixture has a header"));
    u32::from_le_bytes(four)
}

/// Reads one staged fixture and compares every answer to what it recorded.
///
/// Rule 1.1: each question answers with a value, and the value is compared to
/// the one the release that wrote the file gave for the same question. Rule
/// 1.4: the file is opened, checked, and read by a handle that did not write
/// any of it - which for a fixture is every handle there is.
///
/// @param version - the release that wrote it
/// @param database - the staged copy to read
fn read_everything(version: &str, database: &Path) {
    let arm = default_arm();
    let database_handle = open(&arm, database);
    let connection = database_handle.session();

    assert_eq!(
        ask(&connection, "PRAGMA integrity_check"),
        "ok",
        "{version}: the file written by that release does not verify under this build"
    );

    let questions = interop::questions();
    let recorded = interop::expected(version);
    assert_eq!(
        questions.len(),
        recorded.len(),
        "{version}: verify.sql asks {} questions and expected.tsv records {}, so the fixture \
         was built from a different list - rebuild it with \
         `pwsh tools/build-interop-fixture.ps1 -Version {version}`",
        questions.len(),
        recorded.len()
    );

    for (question, (name, wanted)) in questions.iter().zip(recorded.iter()) {
        assert_eq!(
            &question.name, name,
            "{version}: expected.tsv is in a different order from verify.sql"
        );
        let got = ask(&connection, &question.sql);
        assert_eq!(
            &got, wanted,
            "{version}: `{}` answers differently under this build.\n  {version} said: {wanted}\n  \
             this build says: {got}",
            question.name
        );
    }
}

/// The current build reads what an earlier release wrote.
///
/// The quick form reads the newest fixture; `INILLUCENT_SCENARIO=full` reads
/// every one of them. A fixture that cannot be opened at all fails in `open`
/// with the file named, which is the failure a format change produces.
#[test]
fn the_current_build_reads_what_an_earlier_release_wrote() {
    let versions = interop::versions();
    assert!(
        versions.len() >= 6,
        "tests/interop holds {} fixtures and six releases have been published from 0.1.1 on; \
         build the missing ones with `pwsh tools/build-interop-fixture.ps1 -Version <version>`",
        versions.len()
    );

    let chosen: Vec<String> = match Scale::from_env() {
        Scale::Quick => versions.last().cloned().into_iter().collect(),
        Scale::Full => versions.clone(),
    };
    assert!(!chosen.is_empty(), "no fixture was chosen to read");

    for version in chosen {
        let area = scratch(&version);
        let database = interop::stage(&version, &area);
        assert!(
            database.is_file(),
            "{version}: the fixture did not stage into {}",
            area.display()
        );
        read_everything(&version, &database);
    }
}

/// `VACUUM` of a database an earlier release wrote leaves every recorded answer the same
/// (bug hunt 10).
///
/// **The shadow tables of an FTS5 table changed layout between releases.** Release 0.1.1 wrote
/// `note_fts_idx(segid, term, pgno)` where this build creates `(segid, term, doclist)`. The
/// module reads the third column by position, so every query worked, but `VACUUM` copied the
/// rows by the column names of the old table into a table that has the new ones and stopped
/// with `table note_fts_idx has no column named pgno`. The quick form vacuums the oldest and the
/// newest fixture, and `INILLUCENT_SCENARIO=full` vacuums every one. Each is then read with the
/// same questions `the_current_build_reads_what_an_earlier_release_wrote` asks, which include
/// an FTS5 `MATCH` over the table that was rebuilt.
#[test]
fn vacuum_keeps_every_answer_of_a_database_an_earlier_release_wrote() {
    let versions = interop::versions();
    let chosen: Vec<String> = match Scale::from_env() {
        Scale::Quick => versions
            .first()
            .into_iter()
            .chain(versions.last())
            .cloned()
            .collect(),
        Scale::Full => versions.clone(),
    };
    assert!(!chosen.is_empty(), "no fixture was chosen to vacuum");
    for version in chosen {
        let area = scratch(&format!("vacuum-{version}"));
        let database = interop::stage(&version, &area);
        {
            let arm = default_arm();
            let handle = open(&arm, &database);
            let connection = handle.session();
            run(&connection, "VACUUM");
        }
        read_everything(&version, &database);
    }
}

/// Opening a format 2 fixture, reading it and checkpointing it leaves it at format 2.
///
/// **The number moves only when a file first stores a key out of line.** A file
/// that does not is byte for byte a format 2 file, and a build that stamped 3 on
/// it at a checkpoint would make every release from 0.1.8 refuse a file it reads
/// correctly. The newest fixture is the release that wrote format 2, and both
/// copies of its meta record are read before and after. Releases from the one that
/// creates every new file at format 3 on have format 3 fixtures, so the newest
/// fixture at format 2 is the one read.
#[test]
fn opening_a_format_two_fixture_and_checkpointing_it_leaves_it_at_two() {
    let Some(newest) = interop::versions()
        .into_iter()
        .rev()
        .find(|version| fixture_format(version) == 2)
    else {
        panic!("tests/interop holds no format 2 fixture");
    };
    let area = scratch("stays-at-two");
    let database = interop::stage(&newest, &area);
    let formats = |path: &Path| -> (u32, u32) {
        let bytes = std::fs::read(path).expect("the staged fixture reads");
        let read = |at: usize| -> u32 {
            let mut four = [0u8; 4];
            four.copy_from_slice(bytes.get(at..at + 4).expect("a header"));
            u32::from_le_bytes(four)
        };
        let page_size = read(12) as usize;
        (read(8), read(page_size + 8))
    };
    assert_eq!(
        formats(&database).0,
        2,
        "{newest} writes format 2, so its fixture says 2 before this build opens it"
    );
    {
        let arm = default_arm();
        let handle = open(&arm, &database);
        let connection = handle.session();
        assert_eq!(
            ask(&connection, "SELECT count(*) FROM note"),
            "121",
            "{newest}: the fixture holds the 120 rows it was built with and the one the log holds"
        );
        run(
            &connection,
            "INSERT INTO note (id, title, body, weight, tag) VALUES (9200, 't', 'b', 1.0, 'x')",
        );
        run(&connection, "PRAGMA wal_checkpoint");
    }
    assert_eq!(
        formats(&database),
        (2, 2),
        "{newest}: a checkpoint of a file with no key stored out of line moved its format"
    );
}

/// Every fixture a format 3 release wrote reads its long keys back through their index.
///
/// `tools/build-interop-fixture.ps1` runs `tests/interop/build-long-keys.sql` for a
/// release whose new file says format 3. That puts 83 keys of about 5,000 bytes,
/// which are stored on pages of their own, and the separators between them,
/// which are stored out of line too, into the index `long_key_k`. Every later build
/// has to find each key through that index and read every byte of it back, so a
/// change to how a key extent or an out of line separator is read fails here. No
/// fixture before the first format 3 release has the table, so for those this
/// checks nothing.
#[test]
fn every_format_three_fixture_reads_its_long_keys() {
    let prefix = "replace(hex(zeroblob(2496)), '0', 'k')";
    for version in interop::versions() {
        if fixture_format(&version) < 3 {
            continue;
        }
        let area = scratch(&format!("long-keys-{version}"));
        let database = interop::stage(&version, &area);
        let arm = default_arm();
        let handle = open(&arm, &database);
        let connection = handle.session();
        assert_eq!(
            ask(&connection, "PRAGMA integrity_check"),
            "ok",
            "{version}: the fixture with long keys does not verify under this build"
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT count(*) || ' ' || sum(length(k)) FROM long_key"
            ),
            "84 414583",
            "{version}: the long keys did not read back at their full length"
        );
        assert_eq!(
            ask(
                &connection,
                &format!("SELECT id FROM long_key INDEXED BY long_key_k WHERE k = {prefix} || 'b'")
            ),
            "3",
            "{version}: the index did not find a long key that shares its first 4,992 bytes \
             with 82 others"
        );
        assert_eq!(
            ask(
                &connection,
                &format!(
                    "SELECT group_concat(id, ',') FROM (SELECT id FROM long_key INDEXED BY \
                     long_key_k WHERE k > {prefix} || '085' ORDER BY k)"
                )
            ),
            "86,87,88,89,2,3,4",
            "{version}: a range over the long keys returned them in the wrong order"
        );
    }
}

/// A database an earlier release wrote still takes a write today.
///
/// Reading an old file is half of interop and the easier half. This is the
/// other half: the current build appends to a tree an earlier release built,
/// closes, and a handle that wrote none of it reads both rows back - the old
/// one and the new one. Rule 1.4, and the reason the assertion names the old
/// row as well as the new one is that a write that rebuilt the table instead of
/// appending to it would satisfy an assertion about the new row alone.
#[test]
fn a_database_an_earlier_release_wrote_still_takes_a_write() {
    let Some(oldest) = interop::versions().first().cloned() else {
        panic!("tests/interop holds no fixtures");
    };
    let area = scratch("written-to");
    let database = interop::stage(&oldest, &area);
    let arm = default_arm();

    {
        let handle = open(&arm, &database);
        let connection = handle.session();
        run(
            &connection,
            "INSERT INTO note (id, title, body, weight, tag) VALUES (9100, 'written by this \
             build', 'appended to a tree that 0.1.1 built', 1.5, 'Ledger')",
        );
    }

    let handle = reopen_and_check(&arm, &database);
    let connection = handle.session();
    assert_eq!(
        ask(
            &connection,
            "SELECT title FROM note WHERE id IN (9001, 9100) ORDER BY id"
        ),
        "the row that only the log holds\nwritten by this build",
        "{oldest}: the row this build appended, or the row that release left in its log, is not \
         there after a reopen"
    );
}

/// A database an earlier release wrote recovers this build's writes after a
/// crash, from every release.
///
/// **Format 2 changed the leaf's delta area and the page checksum (task-2074),
/// and every release before it wrote format 1.** This build reads format 1, and
/// it keeps writing a format 1 leaf by format 1's rules until something repacks
/// it - a compaction, a split - and logs that repack with the page's image. The
/// reason is recovery: it replays the log onto the pages the file holds, which
/// for a file an earlier release wrote are format 1 pages, and a replay that
/// followed different rules from the write would land on different bytes.
///
/// So this writes enough rows into the fixture's table and its index to take a
/// format 1 delta area past its 32 rows, which makes the first compactions of
/// format 1 leaves happen inside the log; kills the process with all of it
/// unfolded; and reopens. The old rows, the new ones, the update and the
/// deletes all have to be there, and the file has to pass the integrity check.
#[test]
fn an_earlier_releases_file_recovers_this_builds_writes_after_a_crash() {
    let shell = inillucent_compat::cliproc::program("inillucent-shell");
    let mut sql = String::new();
    for id in 20_000..20_300 {
        sql.push_str(&format!(
            "INSERT INTO note (id, title, body, weight, tag) VALUES ({id}, 'crash {id}', \
             'written by this build into a file an earlier release wrote', {}, 'Tag{}');\n",
            id % 17,
            id % 13
        ));
    }
    sql.push_str("UPDATE note SET weight = weight + 100 WHERE id % 3 = 0 AND id >= 20000;\n");
    sql.push_str("DELETE FROM note WHERE id BETWEEN 20100 AND 20109;\n");
    let expected_weight: i64 = (20_000i64..20_300)
        .filter(|id| !(20_100..=20_109).contains(id))
        .map(|id| id % 17 + if id % 3 == 0 { 100 } else { 0 })
        .sum();
    let arm = default_arm();
    for version in interop::versions() {
        let area = scratch(&format!("crashed-{version}"));
        let database = interop::stage(&version, &area);
        let said = inillucent_compat::cliproc::write_and_crash(&shell, &database, &sql);
        assert!(
            said.contains("written"),
            "{version}: the statements did not run before the process was killed:\n{said}"
        );
        let handle = reopen_and_check(&arm, &database);
        let connection = handle.session();
        assert_eq!(
            ask(&connection, "SELECT count(*) FROM note WHERE id >= 20000"),
            "290",
            "{version}: the rows this build wrote before the crash did not all come back"
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT sum(weight) FROM note WHERE id >= 20000"
            ),
            expected_weight.to_string(),
            "{version}: the update this build made before the crash did not come back"
        );
        assert_eq!(
            ask(&connection, "SELECT title FROM note WHERE id = 9001"),
            "the row that only the log holds",
            "{version}: the row that release left in its own log is gone"
        );
        // The index over `title` holds exactly what the table does: a range the
        // index answers against the same range read off the table.
        assert_eq!(
            ask(
                &connection,
                "SELECT count(*) FROM note WHERE title >= 'crash' AND title < 'crasi'"
            ),
            "290",
            "{version}: the index over `title` disagrees with the table after recovery"
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT count(*) FROM note WHERE +title >= 'crash' AND +title < 'crasi'"
            ),
            "290",
            "{version}: the table's own rows disagree with the count after recovery"
        );
    }
}

/// Every release answered the questions identically.
///
/// `build.sql` is deterministic, so six releases writing it produced six files
/// that hold the same values - and the six `expected.tsv` files are therefore
/// byte for byte the same. The comparison is worth making because the files
/// themselves are **not** the same: the six `app.rdb` differ byte for byte,
/// which is what makes reading them a test rather than a copy.
#[test]
fn every_release_recorded_the_same_answers() {
    let versions = interop::versions();
    let Some(first) = versions.first() else {
        panic!("tests/interop holds no fixtures");
    };
    let reference = interop::expected(first);
    assert!(
        reference.len() >= 11,
        "{first}'s expected.tsv records {} answers and verify.sql asks 11",
        reference.len()
    );
    for version in versions.iter().skip(1) {
        assert_eq!(
            interop::expected(version),
            reference,
            "{version} answered verify.sql differently from {first}, which is a difference \
             between two released builds rather than a stale fixture"
        );
    }
}

/// A table that 2.0.2 altered after its table constraints reads, writes, alters
/// and dumps under this build.
///
/// **Releases before 2.1.0 stored `ADD COLUMN` after the table constraints**
/// (`CREATE TABLE chunk (..., UNIQUE (document_id, ordinal), CHECK (...),
/// embedded_at INTEGER, weight ...)`), which the grammar refuses. 2.8.1 skipped
/// the table when it loaded the schema, so the file opened, `sqlite_schema`
/// listed `chunk`, and every statement naming it answered `no such table`. A
/// real application's 606,649 row table looked dropped that way. The fixture in
/// `tests/interop/alter-after-constraint` was written by the 2.0.2 binary from
/// its `build.sql`, with one row left only in the log.
///
/// Every answer is compared to what 2.0.2 itself gave. After a checkpoint the
/// stored text is the reordered one, and the dump, which must load again, is
/// read back through a fresh database.
#[test]
fn a_table_that_2_0_2_altered_after_its_table_constraints_reads_and_writes() {
    let area = scratch("alter-after-constraint");
    let database = interop::stage("alter-after-constraint", &area);
    let arm = default_arm();
    {
        let handle = open(&arm, &database);
        let connection = handle.session();
        assert_eq!(ask(&connection, "PRAGMA integrity_check"), "ok");
        assert_eq!(
            ask(
                &connection,
                "SELECT id, document_id, ordinal, body, coalesce(embedded_at, 0), weight \
                 FROM chunk ORDER BY id"
            ),
            "c1,d1,0,alpha,0,1.5\nc2,d1,1,beta,100,1.5\nc3,d2,0,gamma,0,1.5\n\
             c4,d2,1,delta,200,2.5\nc5,d1,2,the row that only the log holds,0,3.5",
            "the rows 2.0.2 wrote, including the one only its log holds, do not come back"
        );
        assert_eq!(
            ask(&connection, "SELECT id FROM chunk WHERE embedded_at = 200"),
            "c4",
            "the index 2.0.2 built on the added column does not answer"
        );
        run(
            &connection,
            "INSERT INTO chunk VALUES ('c6', 'd2', 2, 'written by this build', 300, 4.5)",
        );
        let duplicate =
            connection.execute_batch("INSERT INTO chunk VALUES ('c7', 'd2', 2, 'dup', 0, 0)");
        assert!(
            duplicate.is_err(),
            "the UNIQUE (document_id, ordinal) declared before the added columns is not enforced"
        );
        let negative =
            connection.execute_batch("INSERT INTO chunk VALUES ('c8', 'd2', -1, 'neg', 0, 0)");
        assert!(negative.is_err(), "the table's CHECK is not enforced");
        run(&connection, "ALTER TABLE chunk RENAME COLUMN body TO text");
        run(&connection, "PRAGMA wal_checkpoint(TRUNCATE)");
        assert_eq!(
            ask(
                &connection,
                "SELECT sql FROM sqlite_schema WHERE name = 'chunk'"
            ),
            "CREATE TABLE chunk (id TEXT PRIMARY KEY, document_id TEXT NOT NULL REFERENCES \
             document(id) ON DELETE CASCADE, ordinal INTEGER NOT NULL, text TEXT, embedded_at \
             INTEGER, weight REAL NOT NULL DEFAULT 1.5, UNIQUE (document_id, ordinal), CHECK \
             (ordinal >= 0))",
            "the stored text is not in the order the grammar accepts after a checkpoint"
        );
        run(
            &connection,
            "PRAGMA foreign_keys = ON; DELETE FROM document WHERE id = 'd2'",
        );
        assert_eq!(
            ask(&connection, "SELECT group_concat(id, ' ') FROM chunk"),
            "c1 c2 c5",
            "the foreign key's cascade declared before the added columns did not run"
        );
    }
    let handle = reopen_and_check(&arm, &database);
    let connection = handle.session();
    assert_eq!(
        ask(&connection, "SELECT count(*), sum(weight) FROM chunk"),
        "3,6.5"
    );
    drop(connection);
    drop(handle);

    // The dump is of a copy nothing has written, so its stored text is still
    // the one 2.0.2 wrote and the dump has to reorder it itself.
    let untouched_area = scratch("alter-after-constraint-dump");
    let untouched = interop::stage("alter-after-constraint", &untouched_area);
    let dumped = std::process::Command::new(inillucent_compat::cliproc::program("inillucent"))
        .arg("--db")
        .arg(&untouched)
        .arg("dump")
        .output()
        .expect("inillucent dump starts");
    let script = String::from_utf8_lossy(&dumped.stdout).into_owned();
    assert!(dumped.status.success(), "inillucent dump failed:\n{script}");
    let copy = untouched_area.join("from-dump.rdb");
    let loaded = open(&arm, &copy);
    let loading = loaded.session();
    run(&loading, &script);
    assert_eq!(
        ask(&loading, "SELECT group_concat(id, ' ') FROM chunk"),
        "c1 c2 c3 c4 c5",
        "the dump of the altered table does not load into a fresh database"
    );
}

/// A table that 2.0.2 declared with a collation inside a table level constraint answers
/// correctly on the first read, enforces the constraint, and checks clean.
///
/// **Releases 0.1.1 to 2.0.9 wrote NULL where a `COLLATE` inside a table level `UNIQUE` or
/// `PRIMARY KEY` named a column.** `UNIQUE (first, last COLLATE NOCASE)` made an automatic
/// index whose every entry held NULL for `last`. 2.8.1 read the same declaration as a key on
/// `last`, so `WHERE first = 'Ann' AND last = 'Bravo'` found nothing for a row that was there,
/// `integrity_check` said `row 1 missing from index sqlite_autoindex_person_1`, and the
/// constraint was not enforced. The fixture in `tests/interop/collate-autoindex` was written by
/// the 2.0.2 binary from its `build.sql`, with two rows left only in the log.
///
/// The index is compared with its table when the file opens. This build withdraws it from the
/// plans, rebuilds it before the first statement of a connection that may write, and checks the
/// result with `integrity_check`.
#[test]
fn a_table_that_2_0_2_declared_a_collation_in_its_unique_constraint_answers_and_enforces_it() {
    let area = scratch("collate-autoindex");
    let database = interop::stage("collate-autoindex", &area);
    let arm = default_arm();
    {
        let handle = open(&arm, &database);
        let connection = handle.session();
        assert_eq!(
            ask(
                &connection,
                "SELECT count(*) FROM person WHERE first = 'Ann' AND last = 'Bravo'"
            ),
            "1",
            "a row 2.0.2 wrote is not found through the index on the first read"
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT name FROM tag WHERE name = 'alpha' COLLATE NOCASE"
            ),
            "alpha"
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT name FROM tag ORDER BY name COLLATE NOCASE"
            ),
            "alpha\nBravo\ncharlie\nDelta\necho\nZulu",
            "the rows, including the one only the log holds, do not come back in NOCASE order"
        );
        assert_eq!(ask(&connection, "PRAGMA integrity_check"), "ok");
        let duplicate = connection.execute_batch("INSERT INTO tag VALUES ('ALPHA', 'dup')");
        assert!(
            duplicate.is_err(),
            "PRIMARY KEY (name COLLATE NOCASE) does not refuse 'ALPHA' beside 'alpha'"
        );
        let second =
            connection.execute_batch("INSERT INTO person (first, last) VALUES ('Ann', 'BRAVO')");
        assert!(
            second.is_err(),
            "UNIQUE (first, last COLLATE NOCASE) does not refuse ('Ann', 'BRAVO') beside ('Ann', 'Bravo')"
        );
        run(
            &connection,
            "INSERT INTO person (first, last) VALUES ('Ann', 'Yankee')",
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT count(*) FROM person WHERE first = 'Ann' AND last = 'Yankee'"
            ),
            "1"
        );
    }
    let handle = reopen_and_check(&arm, &database);
    let connection = handle.session();
    assert_eq!(ask(&connection, "SELECT count(*) FROM person"), "7");
    assert_eq!(
        ask(
            &connection,
            "SELECT count(*) FROM person INDEXED BY sqlite_autoindex_person_1 \
             WHERE first = 'Bob' AND last = 'Delta'"
        ),
        "1",
        "the rebuilt index does not answer for a row written before the rebuild"
    );
}

/// The same file read through a connection that cannot write answers correctly.
///
/// A read only connection cannot rebuild the index, so it must not read it: the answers come from
/// the table.
#[test]
fn a_read_only_connection_answers_correctly_from_a_2_0_2_collated_constraint() {
    let area = scratch("collate-autoindex-read-only");
    let database = interop::stage("collate-autoindex", &area);
    let ask_read_only = |sql: &str| -> String {
        let output = std::process::Command::new(inillucent_compat::cliproc::program("inillucent"))
            .arg("--db")
            .arg(&database)
            .arg("--readonly")
            .arg("query")
            .arg(sql)
            .output()
            .expect("inillucent query starts");
        assert!(
            output.status.success(),
            "{sql} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .skip(2)
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(
        ask_read_only("SELECT count(*) FROM person WHERE first = 'Ann' AND last = 'Bravo'"),
        "1"
    );
    assert_eq!(
        ask_read_only("SELECT name FROM tag WHERE name = 'alpha' COLLATE NOCASE"),
        "alpha"
    );
}

/// The same file attached to another database answers correctly and enforces its constraints.
///
/// An application that attaches a 2.0.x file to its own database reads and writes it through the
/// attached schema. The index is withdrawn from that schema's plans when the file is attached, and
/// the first statement of a connection that may write rebuilds it with `REINDEX old.<index>`.
#[test]
fn an_attached_2_0_2_file_with_a_collated_constraint_answers_and_enforces_it() {
    let area = scratch("collate-autoindex-attached");
    let old = interop::stage("collate-autoindex", &area);
    let main = area.join("main.rdb");
    let arm = default_arm();
    {
        let handle = open(&arm, &main);
        let connection = handle.session();
        run(
            &connection,
            &format!(
                "ATTACH '{}' AS old",
                old.display().to_string().replace('\\', "/")
            ),
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT count(*) FROM old.person WHERE first = 'Ann' AND last = 'Bravo'"
            ),
            "1",
            "a row 2.0.2 wrote is not found in the attached file"
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT name FROM old.tag WHERE name = 'alpha' COLLATE NOCASE"
            ),
            "alpha"
        );
        assert!(
            connection
                .execute_batch("INSERT INTO old.tag VALUES ('ALPHA', 'dup')")
                .is_err(),
            "PRIMARY KEY (name COLLATE NOCASE) of the attached file does not refuse 'ALPHA'"
        );
        assert!(
            connection
                .execute_batch("INSERT INTO old.person (first, last) VALUES ('Ann', 'BRAVO')")
                .is_err(),
            "UNIQUE (first, last COLLATE NOCASE) of the attached file does not refuse 'BRAVO'"
        );
        assert_eq!(ask(&connection, "PRAGMA old.integrity_check"), "ok");
    }
}

/// A table that 2.0.2 declared `contentless_delete=1` reads, takes a `VACUUM` and is still
/// refused a new declaration of the option.
///
/// **Releases 0.1.1 to 2.3.3 accepted the option and stored a plain contentless table.** A
/// `DELETE` from it was refused, as it is from any contentless table. 2.3.4 to 2.8.1 answered
/// every query of such a table with `fts5 contentless_delete=1 is not built` and refused every
/// `VACUUM` of the file, so the rows 2.0.2 wrote could not be read at all. The fixture in
/// `tests/interop/fts5-contentless-delete` was written by the 2.0.2 binary from its `build.sql`.
#[test]
fn a_table_that_2_0_2_declared_contentless_delete_reads_and_vacuums() {
    let area = scratch("fts5-contentless-delete");
    let database = interop::stage("fts5-contentless-delete", &area);
    let arm = default_arm();
    {
        let handle = open(&arm, &database);
        let connection = handle.session();
        assert_eq!(
            ask(
                &connection,
                "SELECT rowid FROM note_search WHERE note_search MATCH 'hello' ORDER BY rowid"
            ),
            "1\n3"
        );
        assert_eq!(
            ask(
                &connection,
                "SELECT rowid FROM note_search WHERE note_search MATCH 'holds'"
            ),
            "4",
            "the row only the log holds is not found"
        );
        assert!(
            connection
                .execute_batch("DELETE FROM note_search WHERE rowid = 1")
                .is_err(),
            "a DELETE from the contentless table was accepted, which 2.0.2 refused"
        );
        let new_table = connection.execute_batch(
            "CREATE VIRTUAL TABLE fresh_search USING fts5(body, content='', contentless_delete=1)",
        );
        assert!(
            new_table.is_err(),
            "a new table that asks for contentless_delete=1 was accepted"
        );
        run(&connection, "VACUUM");
        assert_eq!(
            ask(
                &connection,
                "SELECT rowid FROM note_search WHERE note_search MATCH 'second'"
            ),
            "2",
            "the index does not answer after VACUUM"
        );
        run(
            &connection,
            "INSERT INTO note_search (rowid, body) VALUES (5, 'written after the vacuum')",
        );
    }
    let handle = reopen_and_check(&arm, &database);
    let connection = handle.session();
    assert_eq!(
        ask(
            &connection,
            "SELECT rowid FROM note_search WHERE note_search MATCH 'vacuum'"
        ),
        "5"
    );
    assert_eq!(
        ask(
            &connection,
            "SELECT sql FROM sqlite_schema WHERE name = 'note_search'"
        ),
        "CREATE VIRTUAL TABLE note_search USING fts5(body, content='', contentless_delete=1)",
        "VACUUM changed the stored declaration"
    );
}

/// The release script builds a fixture for the version it is shipping.
///
/// Rule 1.5, and the thing it holds is a claim this file's own documentation
/// makes: that `tests/interop/` never lags a release. Nothing in the test suite
/// can make that true - it is true because `packaging/ship.ps1` calls the
/// script in its publish phase - so what is checked here is that the call is
/// still there and still passes the version being shipped.
#[test]
fn the_release_script_builds_the_shipped_versions_fixture() {
    let ship = workspace_root().join("packaging/ship.ps1");
    let text = std::fs::read_to_string(&ship).unwrap_or_default();
    assert!(
        !text.is_empty(),
        "packaging/ship.ps1 could not be read at {}",
        ship.display()
    );
    assert!(
        text.contains("build-interop-fixture.ps1"),
        "packaging/ship.ps1 no longer calls tools/build-interop-fixture.ps1, so tests/interop \
         will lag the next release and this suite will grade a format nobody ships"
    );
    let calls = text
        .lines()
        .filter(|line| line.contains("build-interop-fixture.ps1"))
        .collect::<Vec<&str>>()
        .join("\n");
    assert!(
        calls.contains("$Version"),
        "packaging/ship.ps1 calls build-interop-fixture.ps1 without passing the version being \
         shipped:\n{calls}"
    );
}

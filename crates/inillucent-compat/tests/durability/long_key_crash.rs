//! Power loss while an index entry longer than a page is being written.
//!
//! Invariant: **a key stored out of line comes back whole or not at all, and
//! the index it is in is sound whichever record the cut falls after.** A key
//! over `page_size / 8` is written as a run of pages, a reference and a prefix
//! in the leaf, and, when two neighbours share a long prefix, a separator stored
//! out of line in the interior page above. A cut can fall after the run and
//! before the leaf names it, after the leaf and before the separator, or
//! between the pages of a split, so each of those has to leave either the whole
//! insert or none of it.
//!
//! The probes read lengths, a point lookup through the index and the integrity
//! check rather than the keys themselves, for the reason `overflow_crash.rs`
//! gives: a campaign that compared rendered keys would carry megabytes of
//! expected state through every cut.

use inillucent_compat::crashcampaign::{record, Campaign};
use inillucent_sim::failpoint::Failure;

/// The database every run starts from.
///
/// Fifty eight keys of 4,500 characters that share their first 4,496 and differ
/// in the last four, so the index leaf is nearly full at the 32,768 byte page
/// and every separator between two neighbours is over the 4,096 bytes an
/// interior page keeps whole.
const SCHEMA: &str = "CREATE TABLE t(a INTEGER PRIMARY KEY, k TEXT);
     CREATE INDEX t_k ON t(k);
     WITH RECURSIVE c(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM c WHERE n < 58)
       INSERT INTO t SELECT n, replace(hex(zeroblob(2248)), '0', 'q') || printf('%04d', n) FROM c;";

/// What each run tries to do, and is cut in the middle of.
///
/// One transaction that adds three keys of 40,000 characters between the ones
/// already there, so the index leaf splits and the new separator is stored out
/// of line.
const WORKLOAD: &str = "BEGIN;
     INSERT INTO t VALUES(100, replace(hex(zeroblob(2248)), '0', 'q') || printf('%04d', 20) || replace(hex(zeroblob(17750)), '0', 'x'));
     INSERT INTO t VALUES(101, replace(hex(zeroblob(2248)), '0', 'q') || printf('%04d', 30) || replace(hex(zeroblob(17750)), '0', 'y'));
     INSERT INTO t VALUES(102, replace(hex(zeroblob(2248)), '0', 'q') || printf('%04d', 40) || replace(hex(zeroblob(17750)), '0', 'z'));
     COMMIT;";

/// The statements after the workload, so a cut can land past its commit.
const TAIL: &str = "SELECT count(*) FROM t; PRAGMA wal_checkpoint;";

/// What a run is graded on.
const PROBES: &[&str] = &[
    "SELECT count(*) FROM t",
    "SELECT a, length(k) FROM t ORDER BY k, a",
    "SELECT a FROM t WHERE k = replace(hex(zeroblob(2248)), '0', 'q') || printf('%04d', 30) || replace(hex(zeroblob(17750)), '0', 'y')",
    "SELECT count(*) FROM t WHERE k > replace(hex(zeroblob(2248)), '0', 'q') || printf('%04d', 25)",
    "PRAGMA integrity_check",
];

/// Power loss anywhere in a transaction that splits an index of long keys.
#[test]
fn a_long_key_under_a_log_comes_back_whole_or_not_at_all() {
    let report = Campaign {
        name: "long-key-wal-crash",
        mode: "wal",
        schema: SCHEMA,
        workload: WORKLOAD,
        tail: TAIL,
        probes: PROBES,
        failure: Failure::Crash,
        cuts: 600,
    }
    .run();
    record("long-key-wal-crash", &report);
}

/// The same, through a rollback journal.
#[test]
fn a_long_key_under_a_journal_comes_back_whole_or_not_at_all() {
    let report = Campaign {
        name: "long-key-journal-crash",
        mode: "delete",
        schema: SCHEMA,
        workload: WORKLOAD,
        tail: TAIL,
        probes: PROBES,
        failure: Failure::Crash,
        cuts: 600,
    }
    .run();
    record("long-key-journal-crash", &report);
}

/// Returns the format version both copies of a file's meta record carry.
///
/// @param database - the database file
fn formats_of(database: &std::path::Path) -> (u32, u32) {
    let bytes = std::fs::read(database).expect("the database reads back");
    let read = |at: usize| -> u32 {
        let mut four = [0u8; 4];
        four.copy_from_slice(bytes.get(at..at + 4).expect("a header"));
        u32::from_le_bytes(four)
    };
    let page_size = read(12) as usize;
    (read(8), read(page_size + 8))
}

/// A crash after the format record and before any checkpoint recovers a file that
/// says format 3, whether or not an earlier transaction that raised it rolled back.
///
/// **The format record belongs to no transaction.** The first key stored out of
/// line raises the version the next meta record will say, and logs it. When that
/// transaction rolls back the version stays raised, so the next transaction that
/// stores a key does not log it again; if the record had belonged to the rolled
/// back transaction, recovery would drop it and the committed key would sit in a
/// file that says format 2. The statements are sent to a real shell that is killed
/// with nothing folded, so the data file still says 2 when the process dies, and
/// recovery is what has to bring the number to 3.
#[test]
fn a_crash_between_the_format_record_and_a_checkpoint_recovers_format_three() {
    use inillucent_compat::cliproc::{program, write_and_crash};
    use inillucent_compat::matrix::default_arm;
    use inillucent_compat::stories::{ask, open, reopen_and_check, run};

    let shell = program("inillucent-shell");
    let directory = inillucent_compat::workspace_root().join("_agent_output/long-key-crash/format");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a directory");
    let database = directory.join("app.rdb");
    let arm = default_arm();
    {
        let handle = open(&arm, &database);
        let connection = handle.session();
        run(
            &connection,
            "CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT); CREATE INDEX t_v ON t (v)",
        );
    }
    // A new file is created at format 3. This case is about the number moving from
    // 2 to 3, so the file is lowered to 2 the way a file made by an older build is.
    lower_to_format_two(&database);
    assert_eq!(
        formats_of(&database),
        (2, 2),
        "nothing stored out of line yet"
    );

    let long = "replace(hex(zeroblob(25000)), '0', 'q')";
    let said = write_and_crash(
        &shell,
        &database,
        &format!(
            "BEGIN; INSERT INTO t VALUES (5, {long} || 'a'); ROLLBACK; \
             INSERT INTO t VALUES (6, {long} || 'b'); SELECT 'written';"
        ),
    );
    assert!(
        said.contains("written"),
        "the statements did not run:\n{said}"
    );
    assert_eq!(
        formats_of(&database),
        (2, 2),
        "the crash left the data file as the last checkpoint wrote it"
    );

    {
        let handle = reopen_and_check(&arm, &database);
        let connection = handle.session();
        assert_eq!(ask(&connection, "SELECT id FROM t ORDER BY id"), "6");
        assert_eq!(
            ask(
                &connection,
                &format!("SELECT count(*) FROM t WHERE v = {long} || 'b'")
            ),
            "1"
        );
    }
    assert_eq!(
        formats_of(&database),
        (3, 3),
        "recovery replayed the format record and the next checkpoint wrote it"
    );
}

/// Rewrites both copies of a file's meta record to say format 2.
///
/// @param database - the database file
fn lower_to_format_two(database: &std::path::Path) {
    let mut bytes = std::fs::read(database).expect("the database reads back");
    let mut size = [0u8; 4];
    size.copy_from_slice(bytes.get(12..16).expect("a header"));
    let page_size = u32::from_le_bytes(size) as usize;
    for slot in 0..2 {
        let page = bytes
            .get_mut(slot * page_size..(slot + 1) * page_size)
            .expect("a meta page");
        let mut meta = inillucent_pool::meta::Meta::decode(page).expect("a meta record");
        meta.format = inillucent_pool::meta::FORMAT_TWO;
        meta.encode(page).expect("the record encodes");
    }
    std::fs::write(database, bytes).expect("the database is written back");
}

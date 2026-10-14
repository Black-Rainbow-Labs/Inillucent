//! `ALTER TABLE ... ADD COLUMN` on a table with many rows, against the pinned SQLite.
//!
//! Invariant: **every column added to a table that has rows answers, on every row that was
//! there before and on every row written after, what SQLite answers.** The engine gives the
//! rows already in the table the column's default without writing them, so the answer for an
//! old row is read from the declaration, and a row written into an old leaf goes through a
//! leaf that has to be widened first. Both are the places a wrong answer would hide, so each
//! script reads before the writes, makes them, and reads again.
//!
//! The tables hold 2,500 rows, enough to take several leaves at the default page size, and
//! the writes are spread through them so most leaves are written into and some are not.
//! Defaults, `NOT NULL`, `CHECK` and generated columns are each graded for what they accept,
//! what they refuse, and the message SQLite gives for a refusal.

use std::path::{Path, PathBuf};
use std::process::Command;

use inillucent_compat::facade::Database;
use inillucent_compat::interchange::reference_shell as pinned_shell;
use inillucent_compat::rendering::shell_text as render;
use inillucent_compat::workspace_root;

/// Returns a scratch directory for one scenario, emptied first.
///
/// @param name - the scenario
fn scratch(name: &str) -> PathBuf {
    let directory = workspace_root().join("_agent_output/add-column").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    let _ = std::fs::create_dir_all(&directory);
    directory
}

/// Runs a script in the pinned shell.
///
/// @param main - the database file
/// @param script - the statements
fn shell(main: &Path, script: &str) -> Option<String> {
    use std::io::Write;
    let program = pinned_shell()?;
    // **The script goes in on standard input.** Given as a command line argument the shell
    // stops at the first statement that fails, and these scripts refuse statements in the
    // middle and read the table after them.
    let mut child = Command::new(program)
        .arg(main)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(script.as_bytes()).ok()?;
    let output = child.wait_with_output().ok()?;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    Some(text)
}

/// Runs a script through inillucent, statement by statement.
///
/// @param main - the database file
/// @param script - the statements
fn run(main: &Path, script: &str) -> String {
    let database = Database::open(main).expect("the database opens");
    let connection = database.session().expect("the connection opens");
    let mut out = String::new();
    // One statement at a time, because a statement that does not prepare must cost only
    // itself. None of these scripts holds a semicolon inside a literal.
    for piece in script.split(';') {
        if piece.trim().is_empty() {
            continue;
        }
        let refused = |out: &mut String, message: &str| {
            out.push_str("Error: ");
            out.push_str(message);
            out.push('\n');
        };
        match connection.prepare_with_tail(piece) {
            Ok((mut statement, _)) => loop {
                match statement.step() {
                    Ok(true) => {
                        let cells: Vec<String> = statement.row().iter().map(render).collect();
                        out.push_str(&cells.join("|"));
                        out.push('\n');
                    }
                    Ok(false) => break,
                    Err(error) => {
                        refused(&mut out, error.message());
                        break;
                    }
                }
            },
            Err(error) => refused(&mut out, error.message()),
        }
    }
    out
}

/// Reduces a report to the rows in order, then the refusals with their messages.
///
/// Unlike the attach comparison this keeps the text of a refusal, because the message of a
/// refused `ADD COLUMN` is part of what SQLite answers and applications match on it.
///
/// @param report - what an engine printed
fn normalise(report: &str) -> Vec<String> {
    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for line in report.lines().map(|line| line.trim_end()) {
        if line.is_empty() {
            continue;
        }
        if line.to_ascii_lowercase().contains("error") {
            errors.push(format!("Error: {}", refusal_message(line)));
        } else {
            rows.push(line.to_string());
        }
    }
    rows.extend(errors);
    rows
}

/// Returns the message of a refusal as either engine prints it, without the code or the position.
///
/// The shell prints `Runtime error near line 7: Cannot add a column (19)` and this engine's
/// report says `Error: Cannot add a column`. Both reduce to the lower case message.
///
/// @param line - the line that reports the refusal
fn refusal_message(line: &str) -> String {
    let mut text = line;
    match text.find("near line ") {
        Some(at) => {
            if let Some(colon) = text.get(at..).and_then(|rest| rest.find(": ")) {
                text = text.get(at + colon + 2..).unwrap_or(text);
            }
        }
        None => text = text.strip_prefix("Error: ").unwrap_or(text),
    }
    let trimmed = text.trim();
    let without_code = match trimmed.rfind(" (") {
        Some(at) if trimmed.ends_with(')') => trimmed.get(..at).unwrap_or(trimmed),
        _ => trimmed,
    };
    without_code.trim().to_ascii_lowercase()
}

/// Runs one script against both engines and requires the same report.
///
/// @param name - the scenario
/// @param script - the statements
fn grade(name: &str, script: &str) {
    let reference_dir = scratch(&format!("{name}-ref"));
    let reference_main = reference_dir.join("main.db");
    let Some(reference) = shell(&reference_main, script) else {
        inillucent_compat::differential::skipping("the pinned shell is not present");
        return;
    };
    let candidate_dir = scratch(&format!("{name}-inillucent"));
    let candidate_main = candidate_dir.join("main.db");
    let candidate = run(&candidate_main, script);
    assert_eq!(
        normalise(&reference),
        normalise(&candidate),
        "\n--- script\n{script}\n--- sqlite\n{reference}\n--- inillucent\n{candidate}"
    );
}

/// The table every script starts from: 2,500 rows, a rowid key, a text and a small integer.
const TABLE: &str = "CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, n INTEGER);
    WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2500)
    INSERT INTO t SELECT x, 'row' || x, x % 7 FROM c;";

/// Returns a script that reads the new columns before and after writes into old leaves.
///
/// @param columns - the names of the added columns
/// @param writes - statements that write rows spread through the table
fn scenario(additions: &str, columns: &str, writes: &str) -> String {
    format!(
        "{TABLE}
         {additions}
         SELECT 'before';
         SELECT id, {columns} FROM t WHERE id IN (1, 700, 1250, 2500) ORDER BY id;
         SELECT count(*) FROM t;
         {writes}
         SELECT 'after';
         SELECT id, {columns} FROM t WHERE id IN (1, 7, 700, 1250, 2500, 9000) ORDER BY id;
         SELECT count(*) FROM t;
         PRAGMA integrity_check;"
    )
}

/// Each class of literal default, and the affinity of the column it is added to.
#[test]
fn a_literal_default_reads_as_sqlite_reads_it() {
    let additions = "
        ALTER TABLE t ADD COLUMN i INTEGER DEFAULT 5;
        ALTER TABLE t ADD COLUMN neg INTEGER DEFAULT -3;
        ALTER TABLE t ADD COLUMN plus INTEGER DEFAULT +4;
        ALTER TABLE t ADD COLUMN r REAL DEFAULT 2;
        ALTER TABLE t ADD COLUMN rs REAL DEFAULT '2.5';
        ALTER TABLE t ADD COLUMN s TEXT DEFAULT 'it''s';
        ALTER TABLE t ADD COLUMN b BLOB DEFAULT x'00ff10';
        ALTER TABLE t ADD COLUMN nul INTEGER DEFAULT NULL;
        ALTER TABLE t ADD COLUMN hx INTEGER DEFAULT 0x10;
        ALTER TABLE t ADD COLUMN mx INTEGER DEFAULT 9223372036854775807;
        ALTER TABLE t ADD COLUMN mn INTEGER DEFAULT -9223372036854775808;
        ALTER TABLE t ADD COLUMN ov INTEGER DEFAULT 9223372036854775808;
        ALTER TABLE t ADD COLUMN fl DEFAULT 1e3;
        ALTER TABLE t ADD COLUMN nm NUMERIC DEFAULT '12';
        ALTER TABLE t ADD COLUMN nt NUMERIC DEFAULT 'twelve';
        ALTER TABLE t ADD COLUMN it INTEGER DEFAULT '7';
        ALTER TABLE t ADD COLUMN ti TEXT DEFAULT 7;
        ALTER TABLE t ADD COLUMN ba DEFAULT 'q';
        ALTER TABLE t ADD COLUMN bw TEXT DEFAULT hello;
        ALTER TABLE t ADD COLUMN ci INTEGER DEFAULT 2.5;";
    // A real is shown as text, so the two engines' ways of printing one cannot differ.
    let columns = "typeof(i), i, neg, plus, typeof(r), CAST(r AS TEXT), typeof(rs), \
                   CAST(rs AS TEXT), s, hex(b), nul, hx, mx, mn, typeof(ov), CAST(ov AS TEXT), \
                   typeof(fl), CAST(fl AS TEXT), typeof(nm), nm, typeof(nt), nt, typeof(it), it, \
                   typeof(ti), ti, typeof(ba), ba, bw, typeof(ci), CAST(ci AS TEXT)";
    let writes = "
        INSERT INTO t(id, a) VALUES (9000, 'new');
        UPDATE t SET i = i + 1 WHERE id % 350 = 7;
        UPDATE t SET s = s || '!' WHERE id % 500 = 0;
        DELETE FROM t WHERE id % 11 = 0;";
    grade("literal-defaults", &scenario(additions, columns, writes));
}

/// Defaults on a column with no default at all, and a column added after a column.
#[test]
fn a_column_with_no_default_reads_null_and_a_later_column_does_not_disturb_an_earlier_one() {
    let additions = "
        ALTER TABLE t ADD COLUMN x;
        ALTER TABLE t ADD COLUMN y INTEGER DEFAULT 3;
        INSERT INTO t(id, a, x) VALUES (3000, 'between', 'kept');
        ALTER TABLE t ADD COLUMN z TEXT DEFAULT 'zz';";
    let columns = "x, y, z";
    let writes = "
        UPDATE t SET x = 'set' WHERE id % 250 = 0;
        INSERT INTO t(id, a, y) VALUES (9000, 'new', 8);
        DELETE FROM t WHERE id % 13 = 0;";
    grade("no-default", &scenario(additions, columns, writes));
}

/// `NOT NULL` with and without a default, and what the column refuses afterwards.
#[test]
fn not_null_is_accepted_with_a_default_and_refused_without_one() {
    let additions = "
        ALTER TABLE t ADD COLUMN k INTEGER NOT NULL DEFAULT 4;
        ALTER TABLE t ADD COLUMN k2 INTEGER NOT NULL;
        ALTER TABLE t ADD COLUMN k3 INTEGER NOT NULL DEFAULT NULL;
        ALTER TABLE t ADD COLUMN k4 TEXT NOT NULL DEFAULT '';
        INSERT INTO t(id, a, k) VALUES (9001, 'null here', NULL);
        UPDATE t SET k = NULL WHERE id = 5;";
    let columns = "k, k4";
    let writes = "
        UPDATE t SET k = k * 2 WHERE id % 100 = 0;
        INSERT INTO t(id, a) VALUES (9000, 'default');";
    grade("not-null", &scenario(additions, columns, writes));
}

/// `CHECK` on the new column is run against the rows already there, and holds afterwards.
#[test]
fn a_check_constraint_is_run_over_the_existing_rows() {
    let additions = "
        ALTER TABLE t ADD COLUMN g INT DEFAULT 5 CHECK (g > 0);
        ALTER TABLE t ADD COLUMN g2 INT DEFAULT 5 CHECK (g2 < 0);
        ALTER TABLE t ADD COLUMN g3 INT DEFAULT 5 CHECK (g3 >= n);
        ALTER TABLE t ADD COLUMN g4 INT DEFAULT 9 CHECK (g4 >= n);
        ALTER TABLE t ADD COLUMN g5 INT DEFAULT 1 CHECK (g5 IN (1, 2));
        INSERT INTO t(id, a, g) VALUES (9001, 'bad', -1);
        UPDATE t SET g4 = 0 WHERE id = 6;
        UPDATE t SET g5 = 3 WHERE id = 7;";
    let columns = "g, g4, g5";
    let writes = "
        UPDATE t SET g4 = 20 WHERE id % 111 = 0;
        INSERT INTO t(id, a, g4) VALUES (9000, 'ok', 12);";
    grade("check", &scenario(additions, columns, writes));
}

/// A table that already has a `CHECK` is rechecked when a column is added.
#[test]
fn a_check_the_table_already_had_is_still_graded() {
    let script = "CREATE TABLE t(id INTEGER PRIMARY KEY, a TEXT, n INTEGER CHECK (n < 7));
        WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2500)
        INSERT INTO t SELECT x, 'row' || x, x % 7 FROM c;
        ALTER TABLE t ADD COLUMN m INTEGER DEFAULT 1 CHECK (m + n < 8);
        ALTER TABLE t ADD COLUMN m2 INTEGER DEFAULT 2 CHECK (m2 + n < 8);
        SELECT id, m FROM t WHERE id IN (1, 2500) ORDER BY id;
        INSERT INTO t(id, a, n, m) VALUES (9000, 'x', 6, 1);
        INSERT INTO t(id, a, n, m) VALUES (9001, 'x', 6, 2);
        SELECT count(*) FROM t;";
    grade("check-existing", script);
}

/// Generated columns: virtual is accepted, stored is refused, and both read the same.
#[test]
fn a_generated_column_is_accepted_when_virtual_and_refused_when_stored() {
    let additions = "
        ALTER TABLE t ADD COLUMN v AS (n * 2);
        ALTER TABLE t ADD COLUMN v2 INTEGER AS (id + 1) VIRTUAL;
        ALTER TABLE t ADD COLUMN s AS (n * 3) STORED;
        ALTER TABLE t ADD COLUMN v3 TEXT AS (a || '?') VIRTUAL NOT NULL;
        ALTER TABLE t ADD COLUMN w INTEGER DEFAULT 6;";
    let columns = "v, v2, v3, w";
    let writes = "
        UPDATE t SET n = n + 1 WHERE id % 200 = 0;
        INSERT INTO t(id, a, n) VALUES (9000, 'new', 4);
        UPDATE t SET w = v + w WHERE id % 300 = 0;";
    grade("generated", &scenario(additions, columns, writes));
}

/// What an `ADD COLUMN` refuses by its definition, whatever the table holds.
#[test]
fn definitions_sqlite_refuses_are_refused_with_its_message() {
    let script = format!(
        "{TABLE}
         ALTER TABLE t ADD COLUMN p INTEGER PRIMARY KEY;
         ALTER TABLE t ADD COLUMN u INTEGER UNIQUE;
         ALTER TABLE t ADD COLUMN d INTEGER DEFAULT (abs(-1));
         ALTER TABLE t ADD COLUMN ts TEXT DEFAULT CURRENT_TIMESTAMP;
         ALTER TABLE t ADD COLUMN n INTEGER;
         PRAGMA foreign_keys = ON;
         ALTER TABLE t ADD COLUMN r INTEGER REFERENCES t(id) DEFAULT 1;
         ALTER TABLE t ADD COLUMN r2 INTEGER REFERENCES t(id);
         SELECT count(*) FROM pragma_table_info('t');
         SELECT id, r2 FROM t WHERE id = 2500;"
    );
    grade("refusals", &script);
}

/// Indexes, a unique index, a rename and a drop after the column was added.
#[test]
fn indexes_renames_and_drops_after_a_lazy_add_agree() {
    let script = format!(
        "{TABLE}
         ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 5;
         ALTER TABLE t ADD COLUMN f TEXT DEFAULT 'hi';
         CREATE INDEX t_e ON t(e, f);
         SELECT count(*) FROM t WHERE e = 5 AND f = 'hi';
         SELECT id FROM t WHERE e = 5 AND f = 'hi' AND id > 2497 ORDER BY id;
         UPDATE t SET e = id WHERE id % 500 = 0;
         SELECT id, e FROM t WHERE e > 5 ORDER BY id;
         CREATE UNIQUE INDEX t_u ON t(e);
         DELETE FROM t WHERE id % 500 <> 0;
         CREATE UNIQUE INDEX t_u2 ON t(e);
         SELECT id, e FROM t ORDER BY id;
         ALTER TABLE t RENAME COLUMN e TO ee;
         ALTER TABLE t DROP COLUMN a;
         SELECT id, ee, f, n FROM t ORDER BY id;
         INSERT INTO t DEFAULT VALUES;
         INSERT OR REPLACE INTO t(id, ee) VALUES (500, 77);
         SELECT id, ee, f, n FROM t ORDER BY id;
         VACUUM;
         SELECT count(*), sum(ee) FROM t;
         PRAGMA integrity_check;"
    );
    grade("indexes-renames-drops", &script);
}

/// A table without a rowid, and a table with a text key.
#[test]
fn a_table_without_a_rowid_takes_the_column_the_same_way() {
    let script = "CREATE TABLE k(a TEXT PRIMARY KEY, b INTEGER) WITHOUT ROWID;
        WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c WHERE x < 2500)
        INSERT INTO k SELECT printf('key-%06d', x), x FROM c;
        ALTER TABLE k ADD COLUMN c INTEGER DEFAULT 3;
        ALTER TABLE k ADD COLUMN d TEXT DEFAULT 'dd';
        SELECT a, b, c, d FROM k WHERE b IN (1, 1250, 2500) ORDER BY a;
        UPDATE k SET c = b WHERE b % 400 = 0;
        INSERT INTO k(a, b) VALUES ('zzz', 0);
        DELETE FROM k WHERE b % 17 = 0;
        SELECT count(*), sum(c), count(d) FROM k;
        SELECT a, c FROM k WHERE c <> 3 ORDER BY a;
        PRAGMA integrity_check;";
    grade("without-rowid", script);
}

/// Adding a column inside a transaction that is rolled back leaves the table as it was.
#[test]
fn a_rolled_back_add_column_leaves_no_trace() {
    let script = format!(
        "{TABLE}
         BEGIN;
         ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 5;
         UPDATE t SET e = 6 WHERE id % 3 = 0;
         SELECT count(*), sum(e) FROM t;
         ROLLBACK;
         SELECT count(*) FROM pragma_table_info('t');
         SELECT count(*) FROM t;
         ALTER TABLE t ADD COLUMN e INTEGER DEFAULT 8;
         SELECT count(*), sum(e) FROM t;
         BEGIN;
         UPDATE t SET e = 1 WHERE id % 2 = 0;
         DELETE FROM t WHERE id % 5 = 0;
         INSERT INTO t(id, a) VALUES (9500, 'gone');
         ROLLBACK;
         SELECT count(*), sum(e) FROM t;
         BEGIN;
         INSERT INTO t(id, a) VALUES (9000, 'x');
         ALTER TABLE t ADD COLUMN f INTEGER DEFAULT 1;
         UPDATE t SET f = 2 WHERE id < 100;
         COMMIT;
         SELECT count(*), sum(e), sum(f) FROM t;
         PRAGMA integrity_check;"
    );
    grade("rollback", &script);
}

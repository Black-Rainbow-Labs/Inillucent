//! Loading a dump into `inillucent-shell`: the dump SQLite writes for FTS5 tables, and the dump
//! this engine writes for tables with names that are not ASCII.
//!
//! Invariant: **a dump loads into a new database and the database then holds the same rows and
//! answers the same `MATCH` queries as the database that was dumped.** The FTS5 case is graded
//! against the pinned SQLite 3.53.4, which writes the dump and answers the same queries on the
//! source file. The other case dumps with this engine and loads with this engine, because that is
//! the round trip a name with a multibyte character at byte 7 or byte 11 used to panic in.

use std::path::{Path, PathBuf};

use inillucent_compat::cliproc::{program, run, run_with_input};
use inillucent_compat::workspace_root;

/// Returns an empty directory of this file's own for one case.
///
/// @param name - the case
fn area(name: &str) -> PathBuf {
    let path = workspace_root()
        .join("_agent_output/sqlite-dump-load")
        .join(name);
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a scratch directory");
    path
}

/// Returns the pinned SQLite shell, if it has been downloaded.
fn reference() -> Option<PathBuf> {
    let path = workspace_root()
        .join(".sqlite-ref/3.53.4/shell")
        .join(format!("sqlite3{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

/// Runs the pinned shell on a database with a script on standard input, and returns standard
/// output.
///
/// @param shell - the pinned `sqlite3`
/// @param database - the file
/// @param script - what to write to standard input
fn sqlite_runs(shell: &Path, database: &Path, script: &str) -> String {
    let ran = run_with_input(shell, &[&database.to_string_lossy()], script);
    assert_eq!(ran.code, 0, "the pinned shell failed: {}", ran.said());
    ran.stdout.replace("\r\n", "\n")
}

/// The questions both engines answer about the FTS5 tables, in list mode.
const QUESTIONS: &[&str] = &[
    "SELECT rowid, title, body FROM own WHERE own MATCH 'alpha' ORDER BY rowid;",
    "SELECT rowid, title FROM own WHERE own MATCH 'title:jumping' ORDER BY rowid;",
    "SELECT rowid, title, body FROM ext WHERE ext MATCH 'lazy' ORDER BY rowid;",
    "SELECT rowid FROM ext WHERE ext MATCH 'fox OR dog' ORDER BY rowid;",
    "SELECT rowid, title FROM own WHERE own MATCH 'gamma' ORDER BY rowid;",
    "SELECT rowid, title FROM own WHERE own MATCH '\u{65e5}\u{672c}' ORDER BY rowid;",
    "SELECT count(*) FROM own;",
    "SELECT count(*) FROM docs;",
];

/// A dump SQLite wrote for FTS5 tables loads, and the tables answer `MATCH` as they did.
///
/// **SQLite dumps an FTS5 table as an `INSERT INTO sqlite_schema` row and then the module's
/// private tables in its own format.** This engine refused the row with `table sqlite_master may
/// not be modified` in a shell that started with a fresh database, and the private tables loaded
/// as ordinary tables of blocks that nothing could search. The source holds a table that keeps
/// its text, one with an `UNINDEXED` column and the `porter` tokenizer, one that reads another
/// table (`content='docs'`), a ranking function stored in its settings, and a non ASCII word.
/// The same questions are asked of the SQLite source and of the database the dump was loaded
/// into.
#[test]
fn a_sqlite_dump_with_fts5_tables_loads_and_answers_match_queries() {
    let Some(sqlite) = reference() else {
        inillucent_compat::differential::skipping("the pinned SQLite shell is missing");
        return;
    };
    let directory = area("fts5");
    let source = directory.join("source.db");
    let script = "\
CREATE TABLE docs(id INTEGER PRIMARY KEY, title TEXT, body TEXT);
INSERT INTO docs VALUES(1,'Alpha','the quick brown fox'),(2,'Beta jumping','jumps over the lazy dog'),(3,'Gamma','\u{65e5}\u{672c}\u{8a9e} text here');
CREATE VIRTUAL TABLE own USING fts5(title, body UNINDEXED, tokenize='porter unicode61');
INSERT INTO own(rowid,title,body) SELECT id,title,body FROM docs;
CREATE VIRTUAL TABLE ext USING fts5(title, body, content='docs', content_rowid='id');
INSERT INTO ext(ext) VALUES('rebuild');
INSERT INTO own(own, rank) VALUES('rank','bm25(5.0)');
";
    sqlite_runs(&sqlite, &source, script);
    let dumped = run(&sqlite, &[&source.to_string_lossy(), ".dump"]);
    assert_eq!(dumped.code, 0, "the dump failed: {}", dumped.said());
    assert!(
        dumped.stdout.contains("INSERT INTO sqlite_schema"),
        "the dump holds no sqlite_schema row, so this case grades nothing: {}",
        dumped.stdout
    );

    let target = directory.join("loaded.rdb");
    let target_text = target.to_string_lossy().into_owned();
    let loaded = run_with_input(
        &program("inillucent-shell"),
        &[&target_text],
        &dumped.stdout,
    );
    assert_eq!(loaded.code, 0, "the dump did not load: {}", loaded.said());
    assert!(
        loaded.stderr.trim().is_empty(),
        "loading the dump reported errors: {}",
        loaded.stderr
    );

    let questions = format!(".mode list\n{}\n", QUESTIONS.join("\n"));
    let theirs = sqlite_runs(&sqlite, &source, &questions);
    assert!(
        theirs.lines().count() >= QUESTIONS.len(),
        "the reference answered {theirs:?}, so the questions grade little"
    );
    let ours = run_with_input(&program("inillucent-shell"), &[&target_text], &questions);
    assert_eq!(ours.code, 0, "the questions failed: {}", ours.said());
    assert_eq!(
        ours.stdout.replace("\r\n", "\n"),
        theirs,
        "the loaded database answers differently from the source"
    );

    let tables = run_with_input(
        &program("inillucent-shell"),
        &[&target_text],
        "SELECT name FROM sqlite_schema WHERE name IN ('own','ext') ORDER BY name;\n",
    );
    assert_eq!(tables.stdout.replace("\r\n", "\n"), "ext\nown\n");
}

/// A dump of a table whose name has a multibyte character at byte 7 or byte 11 is written by the
/// shell and by `inillucent dump`, and loads back.
///
/// **`.dump` sliced the table's name at byte 7 to see whether it starts with `sqlite_`, and at
/// byte 11 to see whether it is a statistics table.** A character that spans that byte is not a
/// place a string can be cut, and the shell panicked. The names are three and four characters of
/// three bytes each, so the first has a character across byte 7 and the second across byte 11.
#[test]
fn a_dump_of_a_table_named_in_japanese_is_written_by_both_programs_and_loads_back() {
    let directory = area("unicode-names");
    let source = directory.join("source.rdb");
    let source_text = source.to_string_lossy().into_owned();
    let shell = program("inillucent-shell");
    let setup = "\
CREATE TABLE \"\u{65e5}\u{672c}\u{8a9e}\"(x); INSERT INTO \"\u{65e5}\u{672c}\u{8a9e}\" VALUES(1),(2);
CREATE TABLE \"\u{65e5}\u{672c}\u{8a9e}\u{5b57}\"(y); INSERT INTO \"\u{65e5}\u{672c}\u{8a9e}\u{5b57}\" VALUES('a');
";
    let made = run_with_input(&shell, &[&source_text], setup);
    assert_eq!(made.code, 0, "the tables were not made: {}", made.said());

    let from_shell = run(&shell, &[&source_text, ".dump"]);
    assert_eq!(
        from_shell.code,
        0,
        "the shell's .dump failed: {}",
        from_shell.said()
    );
    let cli = program("inillucent");
    let from_cli = run(&cli, &["--db", &source_text, "dump"]);
    assert_eq!(
        from_cli.code,
        0,
        "inillucent dump failed: {}",
        from_cli.said()
    );
    for (who, dump) in [
        ("the shell", &from_shell.stdout),
        ("inillucent dump", &from_cli.stdout),
    ] {
        assert!(
            dump.contains("INSERT INTO \"\u{65e5}\u{672c}\u{8a9e}\" VALUES(2);")
                && dump.contains("INSERT INTO \"\u{65e5}\u{672c}\u{8a9e}\u{5b57}\" VALUES('a');"),
            "{who} did not write both tables: {dump}"
        );
        let name = if who == "the shell" {
            "from-shell"
        } else {
            "from-cli"
        };
        let target = directory.join(format!("{name}.rdb"));
        let target_text = target.to_string_lossy().into_owned();
        let loaded = run_with_input(&shell, &[&target_text], dump);
        assert_eq!(
            loaded.code,
            0,
            "{who}'s dump did not load: {}",
            loaded.said()
        );
        let rows = run_with_input(
            &shell,
            &[&target_text],
            "SELECT (SELECT group_concat(x) FROM \"\u{65e5}\u{672c}\u{8a9e}\"), \
             (SELECT y FROM \"\u{65e5}\u{672c}\u{8a9e}\u{5b57}\");\n",
        );
        assert_eq!(
            rows.stdout.trim(),
            "1,2|a",
            "{who}'s dump loaded the wrong rows"
        );
    }
}

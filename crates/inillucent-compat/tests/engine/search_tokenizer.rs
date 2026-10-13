//! `inillucent_search` with a tokenizer pack: `tokenize = '<name>'` through SQL.
//!
//! Invariant: **a table tokenizes with the tokenizer it was created with, on
//! every path and after every reopen.** A pack is read from the tokenizer folder
//! once, at `CREATE`, and copied into the table's `%_config`. A search, a write,
//! the BM25 score ceiling and an integrity check all read that copy, so removing
//! the installed pack changes nothing about a table that already exists.
//!
//! The suite points `INILLUCENT_TOKENIZER_DIR` at a folder of its own under
//! `_agent_output`, holding the Brazilian Portuguese example pack the
//! repository ships, so a pack installed on the machine cannot change an
//! answer. A test that removes a pack removes one with a name of its own.

use std::path::PathBuf;
use std::sync::Once;

use inillucent_base::DbError;
use inillucent_compat::differential::{scratch, start_inillucent};
use inillucent_compat::rendering::datum_text as render;
use inillucent_compat::workspace_root;
use inillucent_engine::connect::{Connection, Database};

/// Where this suite's scratch databases live.
const AREA: &str = "search-tokenizer";

/// The rows the Portuguese tests search.
const ROWS: &str = "INSERT INTO t(rowid, body) VALUES \
    (1, 'Política de reembolso'), \
    (2, 'Notificação enviada'), \
    (3, 'Horário de atendimento')";

/// Returns the folder this suite installs packs into, filling it the first
/// time.
fn packs() -> PathBuf {
    static FILLED: Once = Once::new();
    let folder = workspace_root().join("_agent_output/search-tokenizer/packs");
    FILLED.call_once(|| {
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("the pack folder is made");
        std::fs::copy(
            workspace_root().join("examples/tokenizers/pt-br.json"),
            folder.join("pt-br.json"),
        )
        .expect("the example pack is copied");
        std::env::set_var(inillucent_core::install::TOKENIZER_DIR_VAR, &folder);
    });
    folder
}

/// Installs the example pack under another name, for a test that removes it.
///
/// @param name - the name the pack is installed as
fn install_renamed(name: &str) -> PathBuf {
    let text = std::fs::read_to_string(workspace_root().join("examples/tokenizers/pt-br.json"))
        .expect("the example pack reads");
    let renamed = text.replacen("\"name\": \"pt-br\"", &format!("\"name\": \"{name}\""), 1);
    let path = packs().join(format!("{name}.json"));
    std::fs::write(&path, renamed).expect("the pack is written");
    path
}

/// Runs a statement for its effect.
fn exec(connection: &Connection<'_>, sql: &str) {
    connection
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
}

/// Returns every row of a query, each column rendered as text, or the error.
fn try_rows(connection: &Connection<'_>, sql: &str) -> Result<Vec<Vec<String>>, DbError> {
    let mut statement = connection.prepare(sql)?;
    let mut out = Vec::new();
    while statement.step()? {
        out.push(statement.row().iter().map(render).collect());
    }
    Ok(out)
}

/// Returns the first column of every row.
fn column(connection: &Connection<'_>, sql: &str) -> Vec<String> {
    try_rows(connection, sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()))
        .into_iter()
        .filter_map(|row| row.into_iter().next())
        .collect()
}

/// Returns the rowids a keyword search finds, best first.
///
/// @param query - the words to search for
fn matches(connection: &Connection<'_>, query: &str) -> Vec<String> {
    column(
        connection,
        &format!("SELECT rowid FROM t WHERE t MATCH '{query}' ORDER BY rank"),
    )
}

/// Returns a connection to a database that is left open for the test.
///
/// @param path - the database file
fn open_at(path: &std::path::Path) -> Connection<'static> {
    let database: &'static Database =
        Box::leak(Box::new(Database::open(path).expect("the database opens")));
    database.session()
}

/// A pack table finds a plural and a spelling without accents, which the
/// built in tokenizer does not.
#[test]
fn a_pack_table_finds_plurals_and_unaccented_spellings() {
    packs();
    let connection = start_inillucent(AREA, "pack");
    exec(
        &connection,
        "CREATE VIRTUAL TABLE t USING inillucent_search(body, tokenize = 'pt-br')",
    );
    exec(&connection, ROWS);
    assert_eq!(matches(&connection, "reembolsos"), vec!["1"]);
    assert_eq!(matches(&connection, "notificacao"), vec!["2"]);
    assert_eq!(matches(&connection, "notificação"), vec!["2"]);
    assert_eq!(matches(&connection, "notificações"), vec!["2"]);
    assert_eq!(matches(&connection, "horario"), vec!["3"]);
    // And the other way round: a row written without accents is found by a
    // query that has them.
    exec(
        &connection,
        "INSERT INTO t(rowid, body) VALUES (4, 'aviso de notificacao')",
    );
    let mut both = matches(&connection, "notificação");
    both.sort();
    assert_eq!(both, vec!["2", "4"]);
}

/// The same rows under the built in tokenizer: the accent and the `-ões`
/// plural do not meet their stored word.
///
/// English removes a final s from any word, so `reembolsos` does reach
/// `reembolso` here too. That is asserted rather than left out, because the
/// Portuguese pack is not the only way to get that one result.
#[test]
fn the_same_rows_under_porter_do_not_match() {
    packs();
    let connection = start_inillucent(AREA, "porter");
    exec(
        &connection,
        "CREATE VIRTUAL TABLE t USING inillucent_search(body)",
    );
    exec(&connection, ROWS);
    assert!(matches(&connection, "notificacao").is_empty());
    assert!(matches(&connection, "notificações").is_empty());
    assert!(matches(&connection, "horario").is_empty());
    assert_eq!(matches(&connection, "notificação"), vec!["2"]);
    assert_eq!(matches(&connection, "reembolsos"), vec!["1"]);
}

/// A pack table reopens, searches and takes writes after the pack it was made
/// with has been removed, because it keeps its own copy.
#[test]
fn a_pack_table_survives_reopening_after_the_pack_is_removed() {
    let installed = install_renamed("pt-br-moved");
    let path = scratch(AREA, "moved", "inillucent");
    {
        let database = Database::open(&path).expect("it opens");
        let connection = database.session();
        exec(
            &connection,
            "CREATE VIRTUAL TABLE t USING inillucent_search(body, tokenize = 'pt-br-moved', \
             compact = 1)",
        );
        exec(&connection, ROWS);
    }
    std::fs::remove_file(&installed).expect("the pack is removed");

    let reopened = open_at(&path);
    // The first statement is a read, which never reaches `begin`.
    assert_eq!(matches(&reopened, "notificacao"), vec!["2"]);
    exec(
        &reopened,
        "INSERT INTO t(rowid, body) VALUES (4, 'Reembolsos aprovados')",
    );
    assert_eq!(matches(&reopened, "aprovado"), vec!["4"]);
    assert_eq!(matches(&reopened, "reembolso"), vec!["1", "4"]);
    assert_eq!(
        column(&reopened, "SELECT v FROM t_config WHERE k = 'tokenize'"),
        vec!["pt-br-moved"]
    );
    let pack_rows = column(
        &reopened,
        "SELECT count(*) FROM t_config WHERE k = 'tokenizer_pack'",
    );
    assert_eq!(pack_rows, vec!["1"]);
}

/// A name nobody installed is refused as an unmet requirement, naming the
/// tokenizers there are, before the statement writes anything.
///
/// **Before anything, because a refusal that comes later leaves a trace.** The
/// engine writes a table's shadow tables before it connects the module, so a
/// refusal from `connect` rolled back a transaction that had already reached the
/// log, and every later open of the file reported a recovery that discarded
/// it. The reopen below asserts there is none.
#[test]
fn an_unknown_tokenizer_is_refused_naming_the_ones_there_are() {
    packs();
    let path = scratch(AREA, "unknown", "inillucent");
    {
        let database = Database::open(&path).expect("it opens");
        let connection = database.session();
        let refused = connection
            .execute_batch(
                "CREATE VIRTUAL TABLE t USING inillucent_search(body, tokenize = 'klingon')",
            )
            .expect_err("klingon is not installed");
        let message = refused.message();
        for named in ["klingon", "porter", "pt-br", "inillucent tokenizer install"] {
            assert!(message.contains(named), "{named} is missing from {message}");
        }
        assert_eq!(refused.requirement(), Some("a tokenizer pack"));
        let bad = connection
            .execute_batch(
                "CREATE VIRTUAL TABLE u USING inillucent_search(body, tokenize = '../x')",
            )
            .expect_err("a path is not a name");
        assert!(
            bad.message().contains("tokenizer name"),
            "{}",
            bad.message()
        );
        assert_eq!(bad.requirement(), None);
    }
    let database = Database::open(&path).expect("it reopens");
    assert_eq!(
        database.recovery_report().losers,
        0,
        "the refused statement left a transaction for recovery to discard: {:?}",
        database.recovery_report()
    );
    let connection = database.session();
    assert_eq!(
        column(&connection, "SELECT count(*) FROM sqlite_schema"),
        vec!["0"],
        "the refused tables were not created"
    );
}

/// A table with no `tokenize` option stores `porter` and no pack, which is
/// what every table stored before packs existed.
#[test]
fn a_table_without_tokenize_stores_what_it_stored_before() {
    packs();
    let connection = start_inillucent(AREA, "default");
    exec(
        &connection,
        "CREATE VIRTUAL TABLE t USING inillucent_search(body)",
    );
    assert_eq!(
        column(&connection, "SELECT v FROM t_config WHERE k = 'tokenize'"),
        vec!["porter"]
    );
    assert_eq!(
        column(
            &connection,
            "SELECT count(*) FROM t_config WHERE k = 'tokenizer_pack'"
        ),
        vec!["0"]
    );
    assert_eq!(
        column(&connection, "SELECT count(*) FROM t_config"),
        vec!["20"]
    );
}

/// `confidence()` stays between 0 and 1 on a pack table, and the row that
/// holds every query term ranks above the row that holds one.
///
/// The BM25 score ceiling is what scales a keyword score into a confidence,
/// and it reads the query with a tokenizer. Read with the built in one, the
/// query terms here are not in the dictionary, the ceiling is zero, and every
/// confidence is zero.
#[test]
fn confidence_stays_within_bounds_and_ranks_the_exact_match_first() {
    packs();
    let connection = start_inillucent(AREA, "confidence");
    exec(
        &connection,
        "CREATE VIRTUAL TABLE t USING inillucent_search(body, tokenize = 'pt-br')",
    );
    exec(
        &connection,
        "INSERT INTO t(rowid, body) VALUES (1, 'reembolso aprovado'), \
         (2, 'notificação de reembolso')",
    );
    let found = try_rows(
        &connection,
        "SELECT rowid, confidence(t) FROM t WHERE t MATCH 'notificações reembolsos' ORDER BY rank",
    )
    .unwrap_or_else(|error| panic!("{}", error.message()));
    let ids: Vec<&str> = found
        .iter()
        .filter_map(|row| row.first().map(String::as_str))
        .collect();
    assert_eq!(ids, vec!["2", "1"]);
    let confidence: Vec<f64> = found
        .iter()
        .map(|row| {
            row.get(1)
                .and_then(|value| value.parse::<f64>().ok())
                .expect("a confidence is a number")
        })
        .collect();
    for value in &confidence {
        assert!((0.0..=1.0).contains(value), "{confidence:?}");
    }
    assert!(confidence[0] > confidence[1], "{confidence:?}");
}

/// A pack table whose stored name was changed to `porter` is refused on its
/// first search after a reopen, naming both tokenizers.
#[test]
fn a_tampered_tokenizer_row_is_refused_on_read() {
    packs();
    let path = scratch(AREA, "tampered", "inillucent");
    {
        let database = Database::open(&path).expect("it opens");
        let connection = database.session();
        exec(
            &connection,
            "CREATE VIRTUAL TABLE t USING inillucent_search(body, tokenize = 'pt-br')",
        );
        exec(&connection, ROWS);
        exec(
            &connection,
            "UPDATE t_config SET v = 'porter' WHERE k = 'tokenize'",
        );
    }
    let reopened = open_at(&path);
    let refused = try_rows(&reopened, "SELECT rowid FROM t WHERE t MATCH 'notificacao'")
        .expect_err("the stored and declared tokenizers disagree");
    let message = refused.message();
    assert!(
        message.contains("porter") && message.contains("pt-br"),
        "{message}"
    );
    // A scan tokenizes nothing, so the rows can still be read and repaired.
    assert_eq!(column(&reopened, "SELECT count(*) FROM t"), vec!["3"]);
}

/// Both integrity checks pass on a reopened pack table whose pack is no longer
/// installed, because each reads the table's stored pack.
///
/// `PRAGMA integrity_check` checks pages and never asks the module, so it is not
/// what this tests. The module's own check is reached two ways. The
/// `integrity-check` command is a write, and `begin` reads the pack before it
/// runs. The engine's module check, which `rtreecheck` reaches, connects to the
/// table afresh and never calls `begin`, so the check has to read the pack
/// itself, or every segment it loads is reported unreadable.
#[test]
fn integrity_check_passes_on_a_reopened_pack_table() {
    let installed = install_renamed("pt-br-integrity");
    let path = scratch(AREA, "integrity", "inillucent");
    {
        let database = Database::open(&path).expect("it opens");
        let connection = database.session();
        exec(
            &connection,
            "CREATE VIRTUAL TABLE t USING inillucent_search(body, tokenize = 'pt-br-integrity', \
             compact = 1)",
        );
        exec(&connection, ROWS);
        exec(
            &connection,
            "INSERT INTO t(rowid, body) VALUES (4, 'Notificações de reembolso')",
        );
    }
    std::fs::remove_file(&installed).expect("the pack is removed");
    let reopened = open_at(&path);
    assert_eq!(
        column(&reopened, "SELECT rtreecheck('t')"),
        vec!["ok".to_string()]
    );
    exec(&reopened, "INSERT INTO t(t) VALUES ('integrity-check')");
    assert_eq!(matches(&reopened, "notificacao"), vec!["2", "4"]);
}

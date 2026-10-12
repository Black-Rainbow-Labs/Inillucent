//! What an open does with a virtual table whose module refuses to connect.
//!
//! Invariant: **a virtual table that cannot be connected is refused alone.**
//! The file opens, every other table reads and writes as before, and a
//! statement that uses the refused table fails with the module's own error.
//! That is what SQLite does with a table whose module is missing or refuses
//! the arguments it was created with.
//!
//! ## The defect
//!
//! `reconnect_modules` returned the first module error it met, and that error
//! stopped the whole open. A search table created with an option an older build
//! did not know therefore made the whole file unreadable to that build, plain
//! tables included, while the release notes said the older build would read
//! every relational table and refuse only the search table. A missing module
//! was already skipped; it now also answers "no such module" for its table, as
//! SQLite does, where it answered "no such table" before.
//!
//! ## A refused create
//!
//! A module may also refuse at `CREATE VIRTUAL TABLE`, after the engine has
//! written its shadow tables. The directive is undone, and its undo is now
//! committed as a failed DML statement's is, so the log holds nothing for the
//! next open to discard.
//!
//! ## How it is tested
//!
//! A module that connects or refuses depending on a switch the test holds. The
//! file is written with the switch off, reopened with it on, and reopened a
//! third time with the switch off again to show the refusal changed nothing in
//! the file.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use inillucent_compat::differential::scratch;
use inillucent_engine::connect::{Connection, Database};
use inillucent_engine::ext::vtab::FilterPlan;
use inillucent_engine::ext::vtab::{
    Context, Declaration, DeclaredColumn, IndexQuery, Module, ModuleArguments, ShadowTable,
    VirtualCursor, VirtualTable,
};
use inillucent_tree::datum::OwnedDatum;
use inillucent_value::value::Value;

/// Where this suite's scratch databases live.
const AREA: &str = "vtab-refused-connect";

/// The error the module gives when it refuses.
const REFUSAL: &str = "picky refuses this table: no such tokenizer: pt-br";

/// A module that refuses every reconnect while `refuse` is set.
///
/// The creating connect always succeeds, so the table can be made. Only a
/// later open is refused, which is the case an older build meets.
struct PickyModule {
    refuse: Arc<AtomicBool>,
}

impl Module for PickyModule {
    fn name(&self) -> &str {
        "picky"
    }

    fn connect(
        &self,
        _arguments: &ModuleArguments,
        creating: bool,
    ) -> inillucent_base::DbResult<Box<dyn VirtualTable>> {
        if !creating && self.refuse.load(Ordering::Relaxed) {
            return Err(inillucent_base::error::statement_refusal(REFUSAL));
        }
        Ok(Box::new(PickyTable {
            declaration: Declaration {
                columns: vec![DeclaredColumn::visible("body")],
                without_rowid: false,
            },
        }))
    }
}

/// One connected picky table, which holds no rows.
struct PickyTable {
    declaration: Declaration,
}

impl VirtualTable for PickyTable {
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    fn best_index(&self, query: &mut IndexQuery) -> inillucent_base::DbResult<()> {
        query.estimated_cost = 1.0;
        query.estimated_rows = 0;
        Ok(())
    }

    fn open(&self) -> inillucent_base::DbResult<Box<dyn VirtualCursor>> {
        Ok(Box::new(EmptyCursor))
    }
}

/// A cursor over no rows.
struct EmptyCursor;

impl VirtualCursor for EmptyCursor {
    fn filter(
        &mut self,
        _context: &mut Context<'_>,
        _plan: &FilterPlan,
    ) -> inillucent_base::DbResult<()> {
        Ok(())
    }

    fn next(&mut self, _context: &mut Context<'_>) -> inillucent_base::DbResult<()> {
        Ok(())
    }

    fn eof(&self) -> bool {
        true
    }

    fn column(
        &mut self,
        _context: &mut Context<'_>,
        _index: usize,
    ) -> inillucent_base::DbResult<Value<'static>> {
        Ok(Value::Null)
    }

    fn rowid(&self) -> inillucent_base::DbResult<i64> {
        Ok(0)
    }
}

/// Runs a statement for its effect.
///
/// @param connection - the connection to run it on
/// @param sql - the statement
fn exec(connection: &Connection<'_>, sql: &str) {
    connection
        .execute_batch(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()));
}

/// Runs a query and returns its rows.
///
/// @param connection - the connection to run it on
/// @param sql - the query
fn rows(connection: &Connection<'_>, sql: &str) -> Vec<Vec<OwnedDatum>> {
    connection
        .query(sql)
        .unwrap_or_else(|error| panic!("{sql}: {}", error.message()))
}

/// Returns the message a statement fails with.
///
/// @param connection - the connection to run it on
/// @param sql - a statement expected to fail
fn failure(connection: &Connection<'_>, sql: &str) -> String {
    match connection.query(sql) {
        Ok(rows) => panic!("{sql} returned {} rows instead of failing", rows.len()),
        Err(error) => error.message().to_string(),
    }
}

/// Opens a database, with the picky module registered when `refuse` is given.
///
/// @param path - the file
/// @param refuse - the module's switch, or `None` to register no module
fn open(path: &std::path::Path, refuse: Option<&Arc<AtomicBool>>) -> Database {
    let database = Database::open(path).expect("the database opens");
    if let Some(refuse) = refuse {
        database
            .register_module(Arc::new(PickyModule {
                refuse: Arc::clone(refuse),
            }))
            .expect("the module registers");
    }
    database
}

/// Writes a file holding one plain table with a row and one picky table.
///
/// @param name - the file's name
fn written(name: &str) -> (std::path::PathBuf, Arc<AtomicBool>) {
    let path = scratch(AREA, name, "inillucent");
    let refuse = Arc::new(AtomicBool::new(false));
    // Closed at the end of the block, so the reopens below start from the file.
    {
        let database = open(&path, Some(&refuse));
        let connection = database.session();
        exec(
            &connection,
            "CREATE TABLE note (id INTEGER PRIMARY KEY, body TEXT);
             INSERT INTO note (body) VALUES ('kept');
             CREATE VIRTUAL TABLE picky_t USING picky(tokenize='pt-br');",
        );
    }
    (path, refuse)
}

/// The file opens, the plain table reads, and the picky table gives the
/// module's error.
///
/// **The case an older build met with a tokenizer pack.** Before the fix a
/// built in module's refusal failed the open, so `SELECT body FROM note` never
/// ran, and a module the application registers was never connected after a
/// reopen at all, so its table answered "unsupported" whatever the module said.
#[test]
fn a_refused_table_fails_alone_with_the_module_error() {
    let (path, refuse) = written("refused");
    refuse.store(true, Ordering::Relaxed);
    let database = open(&path, Some(&refuse));
    let connection = database.session();
    assert_eq!(
        rows(&connection, "SELECT body FROM note"),
        vec![vec![OwnedDatum::Text(b"kept".to_vec())]]
    );
    assert_eq!(
        rows(
            &connection,
            "SELECT name FROM sqlite_schema WHERE name = 'picky_t'"
        ),
        vec![vec![OwnedDatum::Text(b"picky_t".to_vec())]],
        "the refused table is still in the catalog"
    );
    assert_eq!(failure(&connection, "SELECT * FROM picky_t"), REFUSAL);
    assert_eq!(failure(&connection, "SELECT * FROM main.PICKY_T"), REFUSAL);
    assert_eq!(
        failure(&connection, "SELECT * FROM nowhere"),
        "no such table: nowhere",
        "a table that is not in the file still says so"
    );
}

/// Writes to the other tables work while one table is refused, and the next
/// open that can connect the module reads everything.
#[test]
fn a_refusal_leaves_the_file_whole() {
    let (path, refuse) = written("whole");
    refuse.store(true, Ordering::Relaxed);
    {
        let database = open(&path, Some(&refuse));
        let connection = database.session();
        exec(&connection, "INSERT INTO note (body) VALUES ('added')");
    }
    refuse.store(false, Ordering::Relaxed);
    let database = open(&path, Some(&refuse));
    let connection = database.session();
    assert_eq!(
        rows(&connection, "SELECT body FROM note ORDER BY id"),
        vec![
            vec![OwnedDatum::Text(b"kept".to_vec())],
            vec![OwnedDatum::Text(b"added".to_vec())],
        ]
    );
    assert_eq!(
        rows(&connection, "SELECT * FROM picky_t"),
        Vec::<Vec<OwnedDatum>>::new()
    );
}

/// A build without the module opens the file and answers "no such module" for
/// the table, which is what SQLite answers.
#[test]
fn a_missing_module_answers_no_such_module() {
    let (path, _refuse) = written("missing");
    let database = open(&path, None);
    let connection = database.session();
    assert_eq!(
        rows(&connection, "SELECT body FROM note"),
        vec![vec![OwnedDatum::Text(b"kept".to_vec())]]
    );
    assert_eq!(
        failure(&connection, "SELECT * FROM picky_t"),
        "no such module: picky"
    );
}

/// A module that writes a shadow table and then refuses to connect at create.
struct LateRefusal;

impl Module for LateRefusal {
    fn name(&self) -> &str {
        "late"
    }

    fn shadow_tables(
        &self,
        _arguments: &ModuleArguments,
    ) -> inillucent_base::DbResult<Vec<ShadowTable>> {
        Ok(vec![ShadowTable {
            suffix: b"data".to_vec(),
            create_sql: "CREATE TABLE \"%_data\"(k INTEGER PRIMARY KEY, v)".to_string(),
            owner: None,
        }])
    }

    fn connect(
        &self,
        _arguments: &ModuleArguments,
        _creating: bool,
    ) -> inillucent_base::DbResult<Box<dyn VirtualTable>> {
        Err(inillucent_base::error::statement_refusal(
            "late refuses after its shadow table is written",
        ))
    }
}

/// A `CREATE VIRTUAL TABLE` refused after its shadow tables were written
/// leaves nothing in the log for the next open to discard.
///
/// **The engine writes a table's shadow tables before it connects the
/// module**, because a module that writes an initial row writes it into one
/// of them. A failed DML statement commits its undo with it; a failed
/// directive did not, so when the refused create was the only statement
/// before the close, the next open reported a loser. Measured before the fix:
/// `losers: 1`, `recovered: true`.
#[test]
fn a_create_refused_after_its_shadow_tables_leaves_no_loser() {
    let path = scratch(AREA, "late", "inillucent");
    {
        let database = Database::open(&path).expect("the database opens");
        database
            .register_module(Arc::new(LateRefusal))
            .expect("the module registers");
        let connection = database.session();
        assert_eq!(
            failure(&connection, "CREATE VIRTUAL TABLE s USING late"),
            "late refuses after its shadow table is written"
        );
        assert_eq!(
            rows(&connection, "SELECT name FROM sqlite_schema"),
            Vec::<Vec<OwnedDatum>>::new(),
            "the shadow table was undone"
        );
    }
    let database = Database::open(&path).expect("the database reopens");
    let connection = database.session();
    let report = database.recovery_report();
    assert_eq!(report.losers, 0, "{report:?}");
    assert_eq!(report.dropped, 0, "{report:?}");
    assert_eq!(
        rows(&connection, "SELECT name FROM sqlite_schema"),
        Vec::<Vec<OwnedDatum>>::new()
    );
    exec(&connection, "CREATE TABLE after (id INTEGER PRIMARY KEY)");
}

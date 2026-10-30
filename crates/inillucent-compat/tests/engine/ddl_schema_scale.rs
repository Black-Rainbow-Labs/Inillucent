//! A `CREATE TABLE`, `CREATE INDEX`, `CREATE VIEW` and `CREATE TRIGGER` cost the same however many
//! tables the schema holds.
//!
//! Invariant: **a schema statement derives the tables it changes and no others.** The number of
//! tables read from their `CREATE` text by one `CREATE INDEX` or `CREATE TRIGGER` does not depend
//! on how many tables already exist, and a `CREATE VIEW` derives none.
//!
//! ## Why this exists
//!
//! Every `CREATE INDEX`, `CREATE VIEW` and `CREATE TRIGGER` re-derived every table in the schema
//! from its stored text, so a migration that made 3,000 tables with an index each in one
//! transaction took 162 s against SQLite's 3.4 s, and the cost of each statement grew with the
//! schema (bug hunt 11). The count of tables derived is asserted, because a wall clock would pass
//! or fail with the machine's load and a count does not.

use inillucent_catalog::load::tables_derived;
use inillucent_compat::newengine::connect::Database;
use inillucent_compat::workspace_root;

/// Opens a fresh database holding `tables` tables with no index.
///
/// @param name - the case's name, which is also its directory's
/// @param tables - how many tables to create
fn schema_of(name: &str, tables: usize) -> Database {
    let directory = workspace_root()
        .join("_agent_output/ddl-schema-scale")
        .join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("the case directory can be made");
    let database = Database::open(directory.join("case.rdb")).expect("the database opens");
    let mut script = String::from("BEGIN;");
    for number in 0..tables {
        script.push_str(&format!(
            "CREATE TABLE tbl_{number} (id INTEGER PRIMARY KEY, a TEXT, b INT);"
        ));
    }
    script.push_str("COMMIT;");
    database
        .session()
        .execute_batch(&script)
        .expect("the tables are made");
    database
}

/// Runs `count` statements built from a template inside one transaction and returns how many
/// tables were derived while they ran.
///
/// @param database - the database
/// @param count - how many statements to run, each over a table of its own
/// @param template - the statement, with `{n}` where the table's number goes
fn tables_derived_by(database: &Database, count: usize, template: &str) -> u64 {
    let session = database.session();
    session
        .execute_batch("BEGIN")
        .expect("the transaction opens");
    let before = tables_derived();
    for number in 0..count {
        let statement = template.replace("{n}", &number.to_string());
        session
            .execute_batch(&statement)
            .expect("the statement runs");
    }
    let after = tables_derived();
    session
        .execute_batch("COMMIT")
        .expect("the transaction commits");
    after.saturating_sub(before)
}

/// Twenty statements derive a handful of tables in a schema of 60 tables and in one of 600.
#[test]
fn a_schema_statement_derives_only_the_table_it_changes() {
    let small = schema_of("small", 60);
    let large = schema_of("large", 600);
    for (template, allowed) in [
        ("CREATE INDEX idx_{n} ON tbl_{n}(a)", 60),
        (
            "CREATE TRIGGER trg_{n} AFTER INSERT ON tbl_{n} BEGIN SELECT 1; END",
            60,
        ),
        ("CREATE VIEW view_{n} AS SELECT id, a FROM tbl_{n}", 0),
    ] {
        let in_small = tables_derived_by(&small, 20, template);
        let in_large = tables_derived_by(&large, 20, template);
        assert!(
            in_large <= allowed,
            "20 of {template} in a schema of 600 tables derived {in_large} tables"
        );
        assert!(
            in_large <= in_small.saturating_add(20),
            "{template} derived {in_large} tables in a schema of 600 and {in_small} in one of 60"
        );
    }
}

/// An index, a trigger and a view made this way work, and the table keeps what it had.
#[test]
fn the_re_derived_table_keeps_what_it_had() {
    let database = schema_of("keeps", 5);
    let session = database.session();
    session
        .execute_batch(
            "CREATE INDEX idx_a ON tbl_2(a); CREATE UNIQUE INDEX idx_b ON tbl_2(b); \
             INSERT INTO tbl_2(a, b) VALUES ('x', 1), ('y', 2); \
             CREATE TRIGGER tr AFTER INSERT ON tbl_2 BEGIN UPDATE tbl_3 SET b = new.b; END; \
             CREATE INDEX idx_c ON tbl_2(a, b); \
             CREATE VIEW vw AS SELECT id, a FROM tbl_2; \
             CREATE TABLE after_view (x);",
        )
        .expect("the statements run");
    let rows = session
        .query("SELECT name FROM sqlite_schema WHERE tbl_name = 'tbl_2' ORDER BY name")
        .expect("the schema is read");
    assert_eq!(rows.len(), 5, "{rows:?}");
    let refused = session.execute_batch("INSERT INTO tbl_2(a, b) VALUES ('z', 1)");
    assert!(
        refused.is_err(),
        "the unique index idx_b still enforces its key"
    );
    session
        .execute_batch(
            "INSERT INTO tbl_3(a, b) VALUES ('t', 0); INSERT INTO tbl_2(a, b) VALUES ('w', 9)",
        )
        .expect("the trigger's table is written");
    let viewed = session
        .query("SELECT count(*) FROM vw")
        .expect("the view is read");
    assert!(format!("{viewed:?}").contains('3'), "{viewed:?}");
    let updated = session
        .query("SELECT b FROM tbl_3")
        .expect("the trigger's effect is read");
    assert!(format!("{updated:?}").contains('9'), "{updated:?}");
}

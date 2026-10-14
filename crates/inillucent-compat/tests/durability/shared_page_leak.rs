//! Shared value pages after a refused vector insert and a cascade delete.
//!
//! Invariant: **a statement that is refused leaves every page it allocated for a
//! value either reachable from a tree or on the free map, and a delete that
//! removes the last row on a shared page gives the page back.** `PRAGMA
//! integrity_check` reports a page that is neither as `Page N: never used`.
//!
//! The case this came from: a sync inserted `VECTOR(768)` values into a table in
//! a transaction, an insert was refused as a duplicate key after its vector had
//! been written to a shared value page, and cascade deletes later removed the
//! other rows on that page. Nothing named the refused row's slot, so the page
//! stayed allocated with no tree reaching it. The vectors are 3,072 bytes, so
//! the page sizes below are the ones where they are stored on shared pages.

use std::path::PathBuf;
use std::sync::Arc;

use inillucent_engine::ImportedDatabase;
use inillucent_exec::physical::Params;
use inillucent_sim::media::MediaModel;
use inillucent_sim::sim_vfs::{SimConfig, SimVfs};
use inillucent_vfs::Vfs;

/// How many frames the pool holds.
const FRAMES: usize = 4_096;

/// The database path inside the simulator.
const DATABASE: &str = "/sim/shared_leak.db";

/// The page sizes at which a 768 dimension vector is stored on a shared page.
const PAGE_SIZES: [usize; 3] = [4_096, 8_192, 16_384];

/// Returns a vector literal of 768 numbers that differs by `seed`.
///
/// @param seed - what the numbers are derived from
fn vector(seed: usize) -> String {
    let numbers: Vec<String> = (0..768)
        .map(|index| format!("{:.4}", ((index * 31 + seed * 17) % 1000) as f32 / 1000.0))
        .collect();
    format!("[{}]", numbers.join(","))
}

/// Runs one statement and reports its error text.
///
/// @param engine - the open connection
/// @param sql - the statement
fn attempt(engine: &mut ImportedDatabase, sql: &str) -> Result<(), String> {
    engine
        .execute_any(sql, &Params::new())
        .map(|_| ())
        .map_err(|why| format!("{} {:?}", why.message(), why.detail()))
}

/// Runs one statement that must succeed.
///
/// @param engine - the open connection
/// @param sql - the statement
fn run(engine: &mut ImportedDatabase, sql: &str) {
    if let Err(why) = attempt(engine, sql) {
        panic!("{}: {why}", sql.chars().take(80).collect::<String>());
    }
}

/// Returns every line `PRAGMA integrity_check` answers, one entry per row.
///
/// @param engine - the open connection
fn integrity(engine: &mut ImportedDatabase) -> Vec<String> {
    let outcome = engine
        .execute_any("PRAGMA integrity_check", &Params::new())
        .expect("the check runs");
    outcome
        .rows
        .iter()
        .map(|row| match row.first() {
            Some(inillucent_tree::datum::OwnedDatum::Text(text)) => {
                String::from_utf8_lossy(text).to_string()
            }
            other => format!("{other:?}"),
        })
        .collect()
}

/// Opens a new database with the Nikaya tables that matter: documents, chunks
/// that cascade from them, and a vector per chunk that cascades from the chunk.
///
/// @param page_size - the page size
fn new_database(page_size: usize) -> ImportedDatabase {
    let vfs = Arc::new(SimVfs::new(SimConfig {
        seed: 95_000 + page_size as u64,
        model: MediaModel::default(),
        ..SimConfig::default()
    }));
    let mut engine = ImportedDatabase::create_on(
        vfs as Arc<dyn Vfs>,
        PathBuf::from(DATABASE),
        page_size,
        FRAMES,
    )
    .expect("the database is created");
    run(&mut engine, "PRAGMA journal_mode=wal");
    run(&mut engine, "PRAGMA foreign_keys=ON");
    run(
        &mut engine,
        "CREATE TABLE document (id INTEGER PRIMARY KEY, name TEXT)",
    );
    run(
        &mut engine,
        "CREATE TABLE chunk (id INTEGER PRIMARY KEY, document_id INTEGER NOT NULL \
         REFERENCES document(id) ON DELETE CASCADE)",
    );
    run(
        &mut engine,
        "CREATE TABLE chunk_embedding (id INTEGER PRIMARY KEY, chunk_id INTEGER NOT NULL \
         REFERENCES chunk(id) ON DELETE CASCADE, embedding VECTOR(768) NOT NULL)",
    );
    engine
}

/// Inserts `documents` documents with `chunks` chunks each and a vector per chunk.
///
/// @param engine - the open connection
/// @param documents - how many documents
/// @param chunks - how many chunks per document
fn fill(engine: &mut ImportedDatabase, documents: usize, chunks: usize) {
    for document in 1..=documents {
        run(
            engine,
            &format!("INSERT INTO document VALUES ({document}, 'd')"),
        );
        for chunk in 1..=chunks {
            let id = document * 100 + chunk;
            run(
                engine,
                &format!("INSERT INTO chunk VALUES ({id}, {document})"),
            );
            run(
                engine,
                &format!(
                    "INSERT INTO chunk_embedding VALUES ({id}, {id}, '{}')",
                    vector(id)
                ),
            );
        }
    }
}

/// Refused inserts whose vector had been written, then every document deleted.
///
/// **Every shared page must be free at the end.** A refused row's vector landed
/// on a page it shared with committed rows, so while those rows lived the leak
/// was invisible: the page was reachable through them. Deleting them all is what
/// shows the page nothing reaches.
#[test]
fn refused_vector_inserts_leave_no_shared_page_behind() {
    for page_size in PAGE_SIZES {
        let mut engine = new_database(page_size);
        fill(&mut engine, 6, 5);
        for round in 0..6usize {
            let duplicate = 101 + round % 5;
            let refused = attempt(
                &mut engine,
                &format!(
                    "INSERT INTO chunk_embedding VALUES ({duplicate}, {duplicate}, '{}')",
                    vector(500 + round)
                ),
            );
            assert!(
                refused.is_err(),
                "page size {page_size}: the duplicate was accepted"
            );
        }
        assert_eq!(
            integrity(&mut engine),
            ["ok"],
            "page size {page_size}, after the refusals"
        );
        run(&mut engine, "DELETE FROM document");
        assert_eq!(
            integrity(&mut engine),
            ["ok"],
            "page size {page_size}: a refused insert left a shared page that nothing reaches"
        );
    }
}

/// A transaction that inserts vectors, has one insert refused, commits, and is
/// followed by cascade deletes of the documents the vectors belong to.
#[test]
fn a_refused_insert_inside_a_transaction_leaves_no_shared_page_behind() {
    for page_size in PAGE_SIZES {
        let mut engine = new_database(page_size);
        fill(&mut engine, 4, 5);
        run(&mut engine, "BEGIN");
        for n in 0..6usize {
            let id = 1_000 + n;
            run(&mut engine, &format!("INSERT INTO chunk VALUES ({id}, 1)"));
            run(
                &mut engine,
                &format!(
                    "INSERT INTO chunk_embedding VALUES ({id}, {id}, '{}')",
                    vector(id)
                ),
            );
        }
        let refused = attempt(
            &mut engine,
            &format!(
                "INSERT INTO chunk_embedding VALUES (101, 101, '{}')",
                vector(9)
            ),
        );
        assert!(
            refused.is_err(),
            "page size {page_size}: the duplicate was accepted"
        );
        run(&mut engine, "COMMIT");
        assert_eq!(
            integrity(&mut engine),
            ["ok"],
            "page size {page_size}, after the commit"
        );
        run(&mut engine, "DELETE FROM document WHERE id IN (1, 2)");
        assert_eq!(
            integrity(&mut engine),
            ["ok"],
            "page size {page_size}, after the cascade"
        );
        run(&mut engine, "DELETE FROM document");
        assert_eq!(
            integrity(&mut engine),
            ["ok"],
            "page size {page_size}: a shared page is left after every row was deleted"
        );
    }
}

/// A refused multi row insert, whose first rows were written before the
/// duplicate was reached, leaves nothing behind either.
#[test]
fn a_refused_multi_row_insert_leaves_no_shared_page_behind() {
    for page_size in PAGE_SIZES {
        let mut engine = new_database(page_size);
        fill(&mut engine, 3, 4);
        let refused = attempt(
            &mut engine,
            &format!(
                "INSERT INTO chunk_embedding VALUES (900, 101, '{}'), (901, 102, '{}'), \
                 (101, 103, '{}')",
                vector(1),
                vector(2),
                vector(3)
            ),
        );
        assert!(
            refused.is_err(),
            "page size {page_size}: the duplicate was accepted"
        );
        run(&mut engine, "DELETE FROM document");
        assert_eq!(integrity(&mut engine), ["ok"], "page size {page_size}");
    }
}

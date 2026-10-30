//! `VACUUM`: rebuilding a database into a file with nothing spare in it.
//!
//! Invariant: a rebuild is a **logical** copy, not a page copy. Every table is
//! read back as rows and written into a fresh database through the ordinary
//! write path, so the result is a database this engine would have produced had
//! the rows arrived in key order and nothing had ever been deleted. That is the
//! only version of `VACUUM` that can honestly claim the space back: a byte copy
//! of the file reproduces its free pages, its half-empty leaves and its
//! interleaved trees exactly, and the file it writes is the same size as the
//! one it read.
//!
//! It is also what SQLite does. `sqlite3RunVacuum` opens a second database,
//! copies the schema into it by *running the `CREATE` statements*, copies the
//! rows with `INSERT ... SELECT`, and then swaps the two files' contents. The
//! shape here is the same, and for the same reason: the rebuild then goes
//! through every check the ordinary write path makes, so a `VACUUM` cannot
//! produce a database a `CREATE TABLE` could not have produced.
//!
//! # Why the two forms share this
//!
//! `VACUUM` and `VACUUM INTO 'file'` differ in one line: where the rebuilt
//! database ends up. They used to share nothing - `VACUUM`
//! checkpointed and `VACUUM INTO` called `std::fs::copy` - and neither of them
//! reclaimed a byte. A reader who asked "is the file smaller afterwards?" got
//! "no" from both, which is not what either statement means.

use std::path::{Path, PathBuf};

use inillucent_base::{error::refusal, DbResult};
use inillucent_catalog::paged::ObjectKind;

use crate::ImportedDatabase;

/// One object to recreate, with the rows it holds.
///
/// The schema is captured before anything is written, because the rebuild reads
/// from a database it is about to replace and a half-read schema is worse than
/// no schema.
struct Captured {
    /// What kind of object it is.
    kind: ObjectKind,
    /// Its name, as written.
    name: Vec<u8>,
    /// The `CREATE` text, empty for an object that has none.
    sql: Vec<u8>,
    /// Whether a virtual table owns it as one of its shadow tables.
    shadow: bool,
    /// Whether it is a virtual table that owns shadow tables of its own.
    owns_shadows: bool,
}

/// Rebuilds a database into a fresh file, and reports what it wrote.
///
/// The order is the one a schema can be replayed in: tables first - a virtual
/// table is one of these, with `VIRTUAL` in its `CREATE` text - because
/// everything else names one; then the rows, while no secondary index exists to
/// maintain; then the indexes, which build over rows that are already there;
/// then views and triggers, which may name any of the above.
///
/// An automatic index - the one a `UNIQUE` or `PRIMARY KEY` declaration
/// implies - has no `CREATE` text and is skipped: recreating the table recreates
/// it, and replaying it would be a second index under the same name.
///
/// **The fresh file is created on the caller's own file system.** This used to
/// call `ImportedDatabase::create`, which constructs a fresh `OsVfs`, so a
/// connection on `MemoryVfs` or `SimVfs` wrote its rebuild onto the real disk
/// and then renamed a path that was not there (task-1946, H2).
///
/// @param vfs - the file system the database lives on
/// @param source - the database being rebuilt
/// @param destination - the file to write, which must not exist
/// @param page_size - the page size the new file is laid out with
/// @param frames - how many frames its pool is given
pub(crate) fn rebuild_into(
    vfs: std::sync::Arc<dyn inillucent_vfs::Vfs>,
    source: &ImportedDatabase,
    destination: &Path,
    page_size: usize,
    frames: usize,
) -> DbResult<()> {
    rebuild_schema_into(vfs, source, crate::MAIN, destination, page_size, frames)
}

/// Rebuilds one schema of a connection into a fresh file.
///
/// **`VACUUM <schema> INTO` rebuilt `main` whatever it was given.** The rebuild
/// read `main`'s catalog and queried unqualified table names, so `VACUUM m INTO
/// 'f'` wrote a copy of `main` and called it `m` (bug hunt 10). Everything the
/// rebuild reads now comes from `schema`: its catalog rows, its rows through
/// qualified names, its sequences and its header fields. The fresh file always
/// holds the copy as its own `main`, which is what an attached database is once
/// it is opened by itself.
///
/// @param vfs - the file system the fresh file is made on
/// @param source - the connection holding the schema
/// @param schema - the schema to copy, as the binder numbers them
/// @param destination - the file to write, which must not exist
/// @param page_size - the page size the new file is laid out with
/// @param frames - how many frames its pool is given
pub(crate) fn rebuild_schema_into(
    vfs: std::sync::Arc<dyn inillucent_vfs::Vfs>,
    source: &ImportedDatabase,
    schema: usize,
    destination: &Path,
    page_size: usize,
    frames: usize,
) -> DbResult<()> {
    let captured = capture_schema(source, schema);
    let mut fresh = ImportedDatabase::create_on(vfs, destination.to_path_buf(), page_size, frames)
        .map_err(|error| {
            inillucent_base::error::misuse(format!(
                "VACUUM could not create the file to rebuild into, {}: {}",
                destination.display(),
                error
            ))
        })?;
    // **One transaction for the whole copy.** Each row used to be its own
    // autocommit statement, so each was a commit with its log syncs: 20,000
    // rows took 72 seconds against SQLite's 48 milliseconds. Nothing reads the
    // new file until it is renamed into place, and a failure part way is
    // thrown away with the file, so the transaction gives nothing up.
    // **The mode goes in before the first table**, so every root the rebuild makes is
    // placed at the start of the file by the rule a file in that mode follows. The
    // mode is the one the connection will have after the rebuild: the file's own, or
    // the one a `PRAGMA auto_vacuum` asked for on a file that already held tables.
    let mode = match source.schema_file(schema) {
        Some(file) if schema != crate::MAIN => file.auto_vacuum(),
        _ => source.pragmas.auto_vacuum(),
    };
    if mode != 0 {
        fresh.apply_auto_vacuum_mode(crate::MAIN, mode, true)?;
    }
    fresh.begin_batch();
    replay_schema(&mut fresh, &captured, |entry| {
        entry.kind == ObjectKind::Table && !entry.shadow
    })?;
    for entry in &captured {
        if entry.kind != ObjectKind::Table || entry.owns_shadows {
            continue;
        }
        copy_rows(source, schema, &mut fresh, &entry.name, entry.shadow)?;
    }
    carry_sequences(source, schema, &mut fresh)?;
    replay_schema(&mut fresh, &captured, |entry| {
        entry.kind == ObjectKind::Index
    })?;
    replay_schema(&mut fresh, &captured, |entry| {
        matches!(entry.kind, ObjectKind::View | ObjectKind::Trigger)
    })?;
    fresh.commit_batch()?;
    carry_header(source, schema, &mut fresh).map_err(|error| {
        inillucent_base::error::misuse(format!("VACUUM could not carry the header: {error}"))
    })?;
    fresh.checkpoint().map_err(|error| {
        inillucent_base::error::misuse(format!(
            "VACUUM could not checkpoint the rebuilt file: {error}"
        ))
    })?;
    Ok(())
}

/// Reads the schema of one database into something that outlives the source.
///
/// @param source - the database being rebuilt
/// @param schema - the schema to read, as the binder numbers them
fn capture_schema(source: &ImportedDatabase, schema: usize) -> Vec<Captured> {
    source
        .entries_of(schema)
        .iter()
        .map(|held| &held.entry)
        .filter(|entry| !entry.sql.is_empty())
        .filter(|entry| !entry.name.starts_with(b"sqlite_"))
        .map(|entry| Captured {
            kind: entry.kind,
            name: entry.name.clone(),
            sql: entry.sql.clone(),
            shadow: source.is_shadow_table_in(&entry.name, schema),
            owns_shadows: source.owns_shadow_tables_in(&entry.name, schema),
        })
        .collect()
}

/// Runs the `CREATE` statements of every object a predicate selects.
///
/// @param fresh - the database being built
/// @param captured - the whole schema, in catalog order
/// @param wanted - which kinds to replay on this pass
fn replay_schema(
    fresh: &mut ImportedDatabase,
    captured: &[Captured],
    wanted: impl Fn(&Captured) -> bool,
) -> DbResult<()> {
    for entry in captured {
        if !wanted(entry) {
            continue;
        }
        let sql = String::from_utf8_lossy(&entry.sql).into_owned();
        let empty = inillucent_exec::physical::Params::new();
        fresh.execute_any(&sql, &empty).map_err(|error| {
            refusal(format!(
                "cannot rebuild {}: {}",
                String::from_utf8_lossy(&entry.name),
                error.detail().unwrap_or_else(|| error.message())
            ))
        })?;
    }
    Ok(())
}

/// Copies one table's rows, key order first, in batches.
///
/// **Read in `rowid` order and written in `rowid` order**, which is what makes
/// the rebuilt tree dense: a leaf filled by ascending appends is full, and one
/// filled by arriving keys is half full on average. It is also what makes the
/// copy reproducible - two `VACUUM`s of one database produce the same file.
///
/// A virtual table's rows are copied through its module the same way, because
/// `INSERT` into one is the module's business and a rebuild has no business
/// reaching around it.
///
/// @param source - the database being rebuilt
/// @param schema - the schema the table is read from, as the binder numbers them
/// @param fresh - the database being built
/// @param table - the table's name, as written
fn copy_rows(
    source: &ImportedDatabase,
    schema: usize,
    fresh: &mut ImportedDatabase,
    table: &[u8],
    shadow: bool,
) -> DbResult<()> {
    let name = quoted(table);
    // The read is qualified so a table of the same name in another schema is
    // never the one copied; the write is not, because the fresh file holds the
    // copy as its own `main`.
    let source_name = format!("{}.{name}", quoted(&source.schema_name_of(schema)));
    let paged = can_be_read_in_pages(source, schema, table);
    let mut after: Option<i64> = None;
    let mut limit = FIRST_PAGE_ROWS;
    let mut writer: Option<RowWriter> = None;
    loop {
        let (rows, columns, last) = match paged {
            true => read_page(source, &source_name, after, limit)?,
            false => {
                let (rows, columns) = source.run(&format!("SELECT * FROM {source_name}"))?;
                (rows, columns, None)
            }
        };
        if rows.is_empty() {
            return Ok(());
        }
        if writer.is_none() {
            writer = Some(RowWriter::prepare(
                source, schema, fresh, table, shadow, &columns,
            )?);
        }
        limit = rows_for_a_page(&rows);
        if let Some(prepared) = writer.as_ref() {
            prepared.write(fresh, rows)?;
        }
        match last {
            Some(rowid) => after = Some(rowid),
            None => return Ok(()),
        }
    }
}

/// The rows the first page of a table is asked for, before the size of a row is known.
const FIRST_PAGE_ROWS: usize = 256;

/// The bytes of rows one page of a table aims at, whatever the size of a row.
///
/// **The copy read every row of a table before it wrote the first** (`SELECT * FROM t`), so
/// `VACUUM` of a 194 MB table peaked at 494 MB and of a 1.1 GB file at 937 MB, where SQLite's
/// peaks at 11 MB. A page is read, written into the new file, and dropped before the next.
const PAGE_BYTES: usize = 8 * 1024 * 1024;

/// Reports whether a table can be read a page at a time in rowid order.
///
/// A rowid table that is not virtual and has no column a query would read as the rowid instead.
/// Any other table is read whole, as it always was.
///
/// @param source - the database being rebuilt
/// @param schema - the schema the table is read from
/// @param table - the table's name, as written
fn can_be_read_in_pages(source: &ImportedDatabase, schema: usize, table: &[u8]) -> bool {
    let folded = table.to_ascii_lowercase();
    source
        .schema
        .tables
        .iter()
        .find(|held| held.folded == folded && held.database == schema)
        .is_some_and(|held| {
            held.kind == inillucent_sql::catalog_view::TableKind::Table
                && held.has_rowid()
                && !held.columns.iter().any(|column| {
                    matches!(column.folded.as_slice(), b"rowid" | b"oid" | b"_rowid_")
                })
        })
}

/// Reads up to `limit` rows of a table that come after a rowid, in rowid order.
///
/// Returns the rows without the rowid column that was added to order them, the column names, and
/// the rowid of the last row when the page was full (so another page may follow).
///
/// @param source - the database being rebuilt
/// @param name - the table's quoted name
/// @param after - the rowid the previous page ended at, or `None` for the first page
/// @param limit - how many rows to ask for
fn read_page(
    source: &ImportedDatabase,
    name: &str,
    after: Option<i64>,
    limit: usize,
) -> DbResult<(
    Vec<Vec<inillucent_tree::datum::OwnedDatum>>,
    Vec<String>,
    Option<i64>,
)> {
    let sql = format!("SELECT *, rowid FROM {name} WHERE rowid > ?1 ORDER BY rowid LIMIT {limit}");
    let mut params = inillucent_exec::physical::Params::new();
    params.set(
        1,
        inillucent_tree::datum::OwnedDatum::Int(after.unwrap_or(i64::MIN)),
    );
    let (mut rows, mut columns) = source.run_with(&sql, &params)?;
    columns.pop();
    let full = rows.len() >= limit;
    let mut last = None;
    for row in rows.iter_mut() {
        if let Some(inillucent_tree::datum::OwnedDatum::Int(rowid)) = row.pop() {
            last = Some(rowid);
        }
    }
    Ok((rows, columns, last.filter(|_| full)))
}

/// Returns how many rows the next page should ask for, so that a page is about [`PAGE_BYTES`].
///
/// @param rows - the page that was just read
fn rows_for_a_page(rows: &[Vec<inillucent_tree::datum::OwnedDatum>]) -> usize {
    let bytes = rows.iter().fold(0usize, |sum, row| {
        row.iter()
            .fold(sum.saturating_add(32), |inner, value| match value {
                inillucent_tree::datum::OwnedDatum::Text(held)
                | inillucent_tree::datum::OwnedDatum::Blob(held) => {
                    inner.saturating_add(held.len()).saturating_add(24)
                }
                _ => inner.saturating_add(24),
            })
    });
    let each = (bytes / rows.len().max(1)).max(1);
    (PAGE_BYTES / each).clamp(16, ROWS_PER_STATEMENT)
}

/// The statement a table's rows are inserted with, and the columns it takes.
struct RowWriter {
    /// The prepared `INSERT`.
    statement: crate::Statement,
    /// Whether the engine runs a chunk of rows as one statement.
    at_once: bool,
    /// Which of the source's columns the `INSERT` takes (a generated column is not).
    kept: Vec<usize>,
    /// The text of the `INSERT`.
    insert: String,
}

impl RowWriter {
    /// Prepares the `INSERT` for a table.
    ///
    /// @param source - the database being rebuilt
    /// @param schema - the schema the table is read from
    /// @param fresh - the database being built
    /// @param table - the table's name, as written
    /// @param shadow - whether the table is a virtual table's shadow table
    /// @param columns - the column names the source returned
    fn prepare(
        source: &ImportedDatabase,
        schema: usize,
        fresh: &mut ImportedDatabase,
        table: &[u8],
        shadow: bool,
        columns: &[String],
    ) -> DbResult<RowWriter> {
        let name = quoted(table);
        // **A generated column is computed again, not copied.** `SELECT *` reads
        // a STORED generated column's value, and naming that column in the
        // `INSERT` is refused ("cannot INSERT into generated column"), which made
        // `VACUUM` fail on any table that had one. SQLite's own copy leaves them
        // out for the same reason, and the new table computes them from the
        // columns it is given.
        let folded = table.to_ascii_lowercase();
        let generated: Vec<Vec<u8>> = source
            .schema
            .tables
            .iter()
            .find(|held| held.folded == folded && held.database == schema)
            .map(|held| {
                held.columns
                    .iter()
                    .filter(|column| column.generated)
                    .map(|column| column.folded.clone())
                    .collect()
            })
            .unwrap_or_default();
        let kept: Vec<usize> = columns
            .iter()
            .enumerate()
            .filter(|(_, column)| !generated.contains(&column.to_ascii_lowercase().into_bytes()))
            .map(|(at, _)| at)
            .collect();
        let placeholders = (1..=kept.len())
            .map(|at| format!("?{at}"))
            .collect::<Vec<_>>()
            .join(", ");
        let names = kept
            .iter()
            .filter_map(|at| columns.get(*at))
            .map(|column| quoted(column.as_bytes()))
            .collect::<Vec<_>>()
            .join(", ");
        // **A shadow table is not empty when its rows arrive.** The module wrote
        // its own opening rows when `CREATE VIRTUAL TABLE` connected it - a version
        // row in `_config`, a structure row in `_data` - and those keys are in the
        // source too, so a plain `INSERT` reports a `UNIQUE` violation on the first
        // of them. The source's row is the one that is right.
        let verb = match shadow {
            true => "INSERT OR REPLACE INTO",
            false => "INSERT INTO",
        };
        // **A shadow table is filled by position, not by column name.** A module reads its own
        // shadow tables by position, and the layout can change between releases: an FTS5 table
        // written by release 0.1.1 has `note_fts_idx(segid, term, pgno)` where this build creates
        // `(segid, term, doclist)`, so `SELECT *` named `pgno` and the insert into the fresh table
        // failed with `table note_fts_idx has no column named pgno`. The third column holds the
        // value the module resolves either way (bug hunt 10).
        let insert = match shadow {
            true => format!("{verb} {name} VALUES ({placeholders})"),
            false => format!("{verb} {name} ({names}) VALUES ({placeholders})"),
        };
        let statement = fresh.prepare_statement(&insert)?;
        let at_once = fresh.runs_rows_at_once(&statement)?;
        Ok(RowWriter {
            statement,
            at_once,
            kept,
            insert,
        })
    }

    /// Inserts rows read from the source, in chunks of [`ROWS_PER_STATEMENT`].
    ///
    /// @param fresh - the database being built
    /// @param rows - the rows, with every column the source returned
    fn write(
        &self,
        fresh: &mut ImportedDatabase,
        rows: Vec<Vec<inillucent_tree::datum::OwnedDatum>>,
    ) -> DbResult<()> {
        let mut rows = rows.into_iter().peekable();
        while rows.peek().is_some() {
            let chunk: Vec<Vec<inillucent_tree::datum::OwnedDatum>> = rows
                .by_ref()
                .take(ROWS_PER_STATEMENT)
                .map(|row| {
                    self.kept
                        .iter()
                        .map(|at| {
                            row.get(*at)
                                .cloned()
                                .unwrap_or(inillucent_tree::datum::OwnedDatum::Null)
                        })
                        .collect()
                })
                .collect();
            if self.at_once
                && fresh
                    .execute_rows_at_once(&self.statement, chunk.clone())?
                    .is_some()
            {
                continue;
            }
            for row in chunk {
                let mut params = inillucent_exec::physical::Params::new();
                for (slot, value) in row.into_iter().enumerate() {
                    params.set(slot.saturating_add(1) as u32, value);
                }
                fresh.execute_any(&self.insert, &params)?;
            }
        }
        Ok(())
    }
}

/// How many rows one `INSERT` of a rebuild carries when the engine can run rows at once.
///
/// **One statement per chunk where the copy ran one per row.** A rebuild of two tables of
/// 500,000 rows made a million statements, each paying for the lock check, the counters and the
/// undo mark that `execute_rows_at_once` pays once. The chunk bounds the copy of the rows a
/// failed chunk would be run again from.
const ROWS_PER_STATEMENT: usize = 20_000;

/// Carries every `AUTOINCREMENT` table's high water mark across a rebuild.
///
/// **`sqlite_sequence` is skipped with the other `sqlite_` tables**, and the
/// copy inserts rows with their own ids, so the sequence the rebuilt file held
/// was the largest id that survived rather than the largest ever handed out:
/// after `DELETE` of the newest row, `VACUUM` gave its id to the next insert,
/// and a table emptied before `VACUUM` lost its row in `sqlite_sequence`. A
/// reused id points an old reference at a new row. SQLite copies
/// `sqlite_sequence` as it is. Found by the bug hunt's ORM schemas.
///
/// @param source - the database being rebuilt
/// @param schema - the schema whose sequences are carried, as the binder numbers them
/// @param fresh - the database being built
fn carry_sequences(
    source: &ImportedDatabase,
    schema: usize,
    fresh: &mut ImportedDatabase,
) -> DbResult<()> {
    let has_sequences = source
        .entries_of(schema)
        .iter()
        .any(|held| held.entry.name.eq_ignore_ascii_case(b"sqlite_sequence"));
    if !has_sequences {
        return Ok(());
    }
    let (rows, _) = source.run(&format!(
        "SELECT name, seq FROM {}.sqlite_sequence",
        quoted(&source.schema_name_of(schema))
    ))?;
    let empty = inillucent_exec::physical::Params::new();
    fresh.execute_any("DELETE FROM main.sqlite_sequence", &empty)?;
    for row in rows {
        let mut params = inillucent_exec::physical::Params::new();
        for (slot, value) in row.into_iter().enumerate() {
            params.set(slot.saturating_add(1) as u32, value);
        }
        fresh.execute_any(
            "INSERT INTO main.sqlite_sequence(name, seq) VALUES (?1, ?2)",
            &params,
        )?;
    }
    Ok(())
}

/// Carries the header fields a rebuild must not invent.
///
/// `user_version` and `application_id` belong to the application rather than to
/// the file's layout, so a rebuild that reset them would silently break the
/// migration scheme of every application that uses one. SQLite carries both
/// across a `VACUUM` for the same reason.
///
/// @param source - the database being rebuilt
/// @param schema - the schema whose header fields are carried, as the binder numbers them
/// @param fresh - the database being built
fn carry_header(
    source: &ImportedDatabase,
    schema: usize,
    fresh: &mut ImportedDatabase,
) -> DbResult<()> {
    // A `temp` the connection never made is an empty database with nothing to
    // carry.
    let Some(file) = source.schema_file(schema) else {
        return Ok(());
    };
    // The vacuum mode was applied before the first table, so the roots are where
    // the mode keeps them, and SQLite keeps the mode across a `VACUUM`.
    // **WAL mode is in the header as well, and it outlives the connection.**
    // It was not carried, so a database in WAL mode came out of `VACUUM` in
    // `delete` mode and stayed there on every later open; SQLite keeps it.
    fresh.storage.database.set_wal_mode(file.wal_mode());
    let empty = inillucent_exec::physical::Params::new();
    fresh.execute_any(
        &format!("PRAGMA user_version={}", file.user_version()),
        &empty,
    )?;
    fresh.execute_any(
        &format!("PRAGMA application_id={}", file.application_id()),
        &empty,
    )?;
    Ok(())
}

/// Returns an identifier quoted so that any name can be written into SQL.
///
/// Double quotes with the internal ones doubled, which is the form every
/// identifier takes regardless of what it holds - a rebuild reads names out of a
/// file it did not write, and a table called `select` or `a"b` is legal.
///
/// @param name - the identifier's bytes
fn quoted(name: &[u8]) -> String {
    let mut out = String::from("\"");
    for byte in name {
        if *byte == b'"' {
            out.push('"');
        }
        out.push(char::from(*byte));
    }
    out.push('"');
    out
}

/// Returns a path beside `target` that nothing else is using.
///
/// The rebuild is written next to the database rather than into a temporary
/// directory so that the swap at the end is a rename within one filesystem,
/// which is atomic; a rename across filesystems is a copy, and a copy is what
/// `VACUUM` exists to avoid doing twice.
///
/// @param target - the database being rebuilt
/// @param stamp - something that makes the name unique
pub(crate) fn scratch_beside(target: &Path, stamp: u64) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(format!(".vacuum-{stamp}"));
    PathBuf::from(name)
}

/// Swaps a rebuilt file into place over `path`, durably.
///
/// **A rename, not a copy.** `std::fs::copy` reads `scratch` and writes
/// `path` a buffer at a time; a crash midway through leaves `path` holding
/// some of the old file's bytes and some of the new one's, which is neither
/// database. `std::fs::rename` within one directory is a single update to the
/// directory entry - it names the old file, or it names the new one, and
/// there is no byte offset in between for a crash to land on. Both Windows
/// and Unix guarantee this when the rename replaces an existing destination
/// on the same volume, which is exactly what `scratch_beside` arranges by
/// naming the rebuilt file `path`'s own directory.
///
/// The rename itself is enough to survive a **crash of this process** -
/// `scratch`'s bytes are already durable, by the checkpoint `rebuild_into`
/// ran before this is ever called, and the directory update is one the file
/// system either applies whole or not at all. What it is not enough for is a
/// **power loss**: the directory entry's own new value is dirty page-cache
/// state until it is flushed, same as any other write. Unix needs an
/// explicit `fsync` of the containing directory to force that; Windows does
/// not, because NTFS journals a rename's metadata itself - the same split
/// `inillucent_vfs`'s own directory sync draws for a delete.
///
/// **Through the caller's `Vfs`, not `std::fs`.** This used to rename directly
/// on the operating system's file system whatever VFS the connection was opened
/// on, so an application on `MemoryVfs`, `SimVfs` or its own encrypting VFS
/// rebuilt into a file the rename could not find (task-1946, H2). The
/// platform-specific directory flush moved with it: `Vfs::rename` is documented
/// to make its own directory entry durable, and `OsVfs` does exactly what this
/// function used to do.
///
/// @param vfs - the file system the database lives on
/// @param scratch - the rebuilt file, already checkpointed and therefore durable
/// @param path - the database file it replaces
pub(crate) fn commit_rebuild(
    vfs: &std::sync::Arc<dyn inillucent_vfs::Vfs>,
    scratch: &Path,
    path: &Path,
) -> DbResult<()> {
    let from = inillucent_vfs::DbPath::new(scratch.to_string_lossy().as_ref());
    let to = inillucent_vfs::DbPath::new(path.to_string_lossy().as_ref());
    vfs.rename(&from, &to).map_err(|error| {
        inillucent_base::error::misuse(format!(
            "cannot write the rebuilt database over {}: {}",
            path.display(),
            error.detail()
        ))
    })
}

/// Removes every write-ahead log segment beside a database file.
///
/// A segment describes the *pages* of the database it was written for, so one
/// left beside a file whose bytes have been replaced is not stale bookkeeping -
/// it is a recipe for undoing the replacement, applied silently by recovery on
/// the next open. This is called at exactly the two moments a file's bytes are
/// about to change identity, and at no other time.
///
/// Failures are ignored: the segments are this engine's own scratch, and a
/// segment that cannot be removed is a warning rather than a reason to abandon
/// a rebuild that has already succeeded.
///
/// **Through the caller's `Vfs`, and by name rather than by listing.** This used
/// to `read_dir` the containing directory and `remove_file` whatever matched,
/// which is two more operations on the operating system's file system that a
/// connection on another VFS never asked for (task-1946, H2). A listing is not
/// needed: `inillucent_wal::segment::segment_name` makes a segment's name a
/// function of the database's name and a sequence number, so the names can be
/// generated. The walk stops at the first sequence number that is not there,
/// with a small run of misses tolerated, because the sequence is dense in
/// practice and an unbounded walk over a `u64` is not a walk.
///
/// @param vfs - the file system the database lives on
/// @param database - the database file the segments belong to
pub(crate) fn remove_log_segments(vfs: &std::sync::Arc<dyn inillucent_vfs::Vfs>, database: &Path) {
    let Some(base) = database.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let directory = database.parent();
    // How many consecutive absent sequence numbers end the walk. A log's
    // segments are numbered from one without gaps; the tolerance is for a
    // directory somebody has tidied by hand rather than for anything the engine
    // does.
    const TOLERATED_GAP: u64 = 16;
    let mut missed = 0u64;
    let mut sequence = 0u64;
    while missed < TOLERATED_GAP {
        let path = inillucent_wal::writer::segment_path(base, directory, sequence);
        sequence = sequence.saturating_add(1);
        match vfs.access(&path, inillucent_vfs::AccessMode::Exists) {
            Ok(true) => {
                missed = 0;
                let _ = vfs.delete(&path, true);
            }
            _ => missed = missed.saturating_add(1),
        }
    }
}

/// Returns what `last_insert_rowid()` should read once `VACUUM` has rebuilt
/// `database`'s schema, given what the counter held before the rebuild.
///
/// Measured against the pinned reference: a schema with no view leaves the
/// counter exactly where `preserved` already has it, but a view moves it.
/// SQLite's own rebuild copies every table, index and trigger into the new
/// schema with one bulk `INSERT ... SELECT`, which does not touch the
/// counter, then recreates each view by running its own `CREATE VIEW` as an
/// ordinary statement, which does. Every view therefore lands after
/// everything else, so the last one recreated sets the counter to the
/// schema's total row count - checked with the view at every position among
/// four fixtures, and it was always the total rather than the view's own.
///
/// @param database - the schema `VACUUM` just rebuilt
/// @param preserved - what the counter held before the rebuild
pub(crate) fn last_rowid_after_vacuum(database: &ImportedDatabase, preserved: i64) -> i64 {
    let entries = database.main_entries();
    if entries.iter().any(|entry| entry.kind == ObjectKind::View) {
        entries.len() as i64
    } else {
        preserved
    }
}

/// Every pragma-set connection setting `VACUUM`'s reopen would otherwise
/// default, captured so it can be put back.
///
/// **None of these are a fact about the file.** Two connections open on the
/// same file may disagree about every one of them, which is exactly why
/// `*self = ImportedDatabase::open(...)` - a fresh connection at every
/// default - is the wrong answer for a statement that is supposed to leave
/// the connection alone. SQLite's own `VACUUM` never faces this question: it
/// rewrites the file without ever closing the connection, so there is
/// nothing for it to put back.
///
/// `attached` and `temps` are carried separately, by [`AttachedSchemas`] -
/// they are schemas rather than settings. `session`, `next_session` and the
/// transaction-scoped fields (`batch`, `undo`, `marks`) are not carried at
/// all: `vacuum_in_place` is only reachable outside a transaction in the
/// first place (`Directive::Vacuum` refuses one before this ever runs), so
/// the transaction-scoped fields are already at their empty defaults on every
/// path that reaches this struct, and `session`/`next_session` restart at the
/// same values - zero and one - that `ImportedDatabase::open` always gives
/// them, which is what lets a restored `temp` schema's own recorded session
/// still match. The random built-ins' seed is left to reseed too - it is
/// per-connection state, not a setting a caller chose, and nothing asks a
/// `VACUUM`d connection's random stream to continue where the last one left
/// off.
pub(crate) struct ConnectionSettings {
    journal_mode: inillucent_pool::journal::JournalMode,
    foreign_keys: bool,
    defer_foreign_keys: bool,
    locking_exclusive: bool,
    defensive: bool,
    secure_delete: u8,
    auto_vacuum: u8,
    automatic_index: bool,
    ignore_check_constraints: bool,
    case_sensitive_like: bool,
    cache_size: Option<i64>,
    analysis_limit: i64,
    writable_schema: bool,
    query_only: bool,
    data_version: u64,
    recursive_triggers: bool,
    legacy_alter_table: bool,
    max_page_count: u64,
    temp_store: i64,
    /// The settings `pragma::remembered` stores, which a `VACUUM` must not
    /// reset: `PRAGMA mmap_size` would read its default again afterwards.
    remembered: std::collections::BTreeMap<&'static str, i64>,
    busy_timeout_ms: u64,
    collations: Vec<(String, inillucent_value::collation::Collation)>,
    authorizer: Option<std::rc::Rc<dyn inillucent_sql::bind::Authorizer>>,
    levers: inillucent_sql::plan::Levers,
    /// Application-registered modules and scalar/aggregate functions -
    /// `register_module`/`create_scalar_function`'s own state. Pure metadata,
    /// not a handle into the file about to be replaced, so cloning it here and
    /// putting it back is exactly as safe as `collations` above.
    registry: inillucent_ext::registry::Registry,
}

impl ConnectionSettings {
    /// Reads every setting off a connection, before its reopen defaults them.
    ///
    /// @param database - the connection `VACUUM` is about to reopen
    pub(crate) fn capture(database: &ImportedDatabase) -> ConnectionSettings {
        ConnectionSettings {
            journal_mode: database.pragmas.journal_mode(),
            foreign_keys: database.pragmas.foreign_keys(),
            defer_foreign_keys: database.pragmas.defer_foreign_keys(),
            locking_exclusive: database.pragmas.locking_exclusive(),
            defensive: database.pragmas.defensive(),
            secure_delete: database.pragmas.secure_delete(),
            auto_vacuum: database.pragmas.auto_vacuum(),
            automatic_index: database.pragmas.automatic_index(),
            ignore_check_constraints: database.pragmas.ignore_check_constraints(),
            case_sensitive_like: database.pragmas.case_sensitive_like(),
            cache_size: database.pragmas.cache_size(),
            analysis_limit: database.pragmas.analysis_limit(),
            writable_schema: database.pragmas.writable_schema(),
            query_only: database.pragmas.query_only(),
            data_version: database.pragmas.data_version(),
            recursive_triggers: database.pragmas.recursive_triggers(),
            legacy_alter_table: database.pragmas.legacy_alter_table(),
            max_page_count: database.storage.database.max_page_count(),
            temp_store: database.pragmas.temp_store(),
            remembered: database.pragmas.remembered_all(),
            busy_timeout_ms: database.pragmas.busy_timeout_ms(),
            collations: database.session_state.collations.clone(),
            authorizer: database.session_state.authorizer.clone(),
            levers: database.pragmas.levers(),
            registry: database.session_state.registry.clone(),
        }
    }

    /// Writes every setting back onto a freshly reopened connection.
    ///
    /// `journal_mode` goes through [`ImportedDatabase::set_journal_mode`]
    /// rather than a field write, deliberately: the WAL flag it carries is
    /// also written into the file's own header, and a plain assignment here
    /// would leave that header unset - a `VACUUM`d WAL database would then
    /// reopen as `delete` for every later opener, not just this connection.
    /// Every other field here is connection-local, so a direct write is
    /// exactly what restoring it means.
    ///
    /// The registry just written may name a module or function the fresh
    /// open's default registry did not have, so the eponymous list and the
    /// catalog it feeds are rebuilt against it afterwards - the same two
    /// calls `register_module` makes whenever it changes the registry.
    ///
    /// @param database - the freshly reopened connection
    pub(crate) fn restore(self, database: &mut ImportedDatabase) -> DbResult<()> {
        database.set_journal_mode(self.journal_mode)?;
        database.pragmas.set_foreign_keys(self.foreign_keys);
        database
            .pragmas
            .set_defer_foreign_keys(self.defer_foreign_keys);
        database
            .pragmas
            .set_locking_exclusive(self.locking_exclusive);
        database.pragmas.set_defensive(self.defensive);
        database.pragmas.set_secure_delete(self.secure_delete);
        database.pragmas.set_auto_vacuum(self.auto_vacuum);
        database.pragmas.set_automatic_index(self.automatic_index);
        database
            .pragmas
            .set_ignore_check_constraints(self.ignore_check_constraints);
        database
            .pragmas
            .set_case_sensitive_like(self.case_sensitive_like);
        database.pragmas.set_cache_size(self.cache_size);
        database.pragmas.set_analysis_limit(self.analysis_limit);
        database.pragmas.set_writable_schema(self.writable_schema);
        database.pragmas.set_query_only(self.query_only);
        database.pragmas.set_data_version(self.data_version);
        database
            .pragmas
            .set_recursive_triggers(self.recursive_triggers);
        database
            .pragmas
            .set_legacy_alter_table(self.legacy_alter_table);
        database
            .storage
            .database
            .set_max_page_count(self.max_page_count);
        database.pragmas.set_temp_store(self.temp_store);
        database.pragmas.restore_remembered(self.remembered);
        database.pragmas.set_busy_timeout_ms(self.busy_timeout_ms);
        database.session_state.collations = self.collations;
        database.session_state.authorizer = self.authorizer;
        database.pragmas.set_levers(self.levers);
        database.session_state.registry = self.registry;
        database.session_state.eponymous.clear();
        database.refresh_catalog();
        Ok(())
    }
}

/// Every temporary table and every attached database, taken off a connection
/// so `VACUUM`'s reopen cannot silently drop them.
///
/// **Carried across whole, not rebuilt.** `main` is the only file `VACUUM`
/// touches: a temporary table's tree lives in its own `MemoryVfs` and an
/// attachment's lives in its own file or its own `MemoryVfs`, and neither is
/// opened, read or written by anything else in this module. So the fix is not
/// to recreate them from a captured schema, the way `rebuild_into` does for
/// `main` - it is to stop discarding them: taken out of the connection before
/// `*self` is replaced and put back once the new one exists, they are
/// exactly the schemas they were, because nothing about them ever changed.
/// This is checked against the pinned SQLite shell, which keeps both a `TEMP`
/// table's rows and an attached database - a real file or `:memory:` - across
/// its own `VACUUM` of `main`.
///
/// `owner` and `next_handle` travel with them: they are what a tree root
/// number resolves to a schema through, keyed by the same root numbers
/// `attached`'s trees already carry, so restoring one without the other would
/// leave every attached tree unreachable by name.
pub(crate) struct AttachedSchemas {
    temps: Vec<crate::Attached>,
    attached: Vec<crate::Attached>,
    owner: std::collections::HashMap<u32, usize>,
    next_handle: u32,
    /// Every built tree an attached or temporary schema owns.
    ///
    /// **`self.schema.trees` is connection-wide, keyed by handle rather than by
    /// schema, and `main`'s own trees are not among these** - `Attached`
    /// itself holds no `PagedTree` at all, only the catalog row that names
    /// one, so a handle here is unreachable without its matching entry moving
    /// too. Filtered to handles at or past [`crate::FIRST_ATTACHED_HANDLE`],
    /// which `main`'s own roots - imported page numbers below it and
    /// DDL-created ones from `crate::FIRST_CREATED_ROOT` - never reach.
    trees: std::collections::HashMap<u32, inillucent_tree::PagedTree>,
    /// The matching `SourceLayout` for each tree above.
    layouts: std::collections::HashMap<u32, std::rc::Rc<inillucent_exec::physical::SourceLayout>>,
    /// The matching index roots for each table above that has one.
    covering: std::collections::HashMap<u32, Vec<u32>>,
}

impl AttachedSchemas {
    /// Takes every temporary table and every attached database off a
    /// connection, leaving it as though neither had ever been used - which is
    /// what the connection `*self = ImportedDatabase::open(...)` builds next
    /// actually is.
    ///
    /// @param database - the connection about to be reopened
    pub(crate) fn take(database: &mut ImportedDatabase) -> AttachedSchemas {
        let is_attached = |root: &u32| *root >= crate::FIRST_ATTACHED_HANDLE;
        let trees = std::mem::take(&mut database.schema.trees)
            .into_iter()
            .filter(|(root, _)| is_attached(root))
            .collect();
        let layouts = std::mem::take(&mut database.schema.layouts)
            .into_iter()
            .filter(|(root, _)| is_attached(root))
            .collect();
        let covering = std::mem::take(&mut database.schema.covering)
            .into_iter()
            .filter(|(root, _)| is_attached(root))
            .collect();
        AttachedSchemas {
            temps: std::mem::take(&mut database.session_state.temps),
            attached: std::mem::take(&mut database.session_state.attached),
            owner: std::mem::take(&mut database.session_state.owner),
            next_handle: database.schema.next_handle,
            trees,
            layouts,
            covering,
        }
    }

    /// Puts every temporary table and every attached database back onto a
    /// freshly reopened connection, and rebuilds the tables that describe
    /// them - the same step `ATTACH` itself takes after adding one, needed
    /// here because the fresh connection derived `self.schema.tables` from `main`
    /// alone, before any of this existed to derive it from.
    ///
    /// @param database - the freshly reopened connection
    pub(crate) fn restore(self, database: &mut ImportedDatabase) -> DbResult<()> {
        database.session_state.temps = self.temps;
        database.session_state.attached = self.attached;
        database.session_state.owner = self.owner;
        database.schema.next_handle = self.next_handle;
        database.schema.trees.extend(self.trees);
        database.schema.layouts.extend(self.layouts);
        database.schema.covering.extend(self.covering);
        database.rebuild_tables()
    }
}

/// Reopens one of the two files `VACUUM` swaps, saying which when it cannot.
///
/// **An error here used to say `Open: No such file or directory` and nothing
/// else**, which names neither the file nor the step - and `VACUUM` opens two
/// different files in sequence, so the message left a reader unable to tell a
/// rebuild that was never written from a rename that did not land.
///
/// @param vfs - the file system the database lives on
/// @param path - the file to open
/// @param page_size - the page size to open it with
/// @param frames - how many frames its pool is given
/// @param which - what this file is, for the message
fn reopened(
    vfs: &std::sync::Arc<dyn inillucent_vfs::Vfs>,
    path: &Path,
    page_size: usize,
    frames: usize,
    which: &str,
) -> DbResult<ImportedDatabase> {
    ImportedDatabase::open_on(
        std::sync::Arc::clone(vfs),
        path.to_path_buf(),
        page_size,
        frames,
    )
    .map_err(|error| {
        inillucent_base::error::misuse(format!(
            "VACUUM could not reopen {which}, {}: {}",
            path.display(),
            error
        ))
    })
}

/// Reports whether any write-ahead log segment sits beside a database file.
///
/// The non-destructive twin of [`remove_log_segments`], for a test that needs
/// to know the fixture it built actually has segments to be endangered by -
/// asserting on a crash scenario that never had anything to lose would not be
/// testing the thing it claims to.
///
/// @param database - the database file to check beside
/// Rebuilds a database over itself, reclaiming everything nothing uses.
///
/// @param connection - the connection to rebuild, replaced in place
///
/// **Written beside the database and renamed over it, not copied**: see
/// `commit_rebuild` for why a rename is what a crash
/// cannot catch halfway and a byte copy is. The connection reopens onto
/// the new file afterwards, because every tree handle it holds names a
/// root that has moved.
///
/// **Every step is on `connection.storage.vfs`** - the rebuild, the rename, the segment
/// removals and both reopens. It was not: the reopens constructed a fresh
/// `OsVfs` and the rebuild called `std::fs` directly, so a connection on
/// `MemoryVfs`, `SimVfs` or an application's own encrypting VFS either
/// failed to find its own database or silently finished the statement on
/// the operating system's file system and stayed there (task-1946, H2).
/// `docs/relational-architecture.md` section 7 describes the rename.
///
/// **Every temporary table and every attached database is carried across
/// too**, by `AttachedSchemas` (see its doc) - `main` is
/// the only file this rewrites, so both are simply moved onto the
/// reopened connection unchanged. Refused instead for a declared imposter
/// table, which is bound into `main`'s own trees by page - every index
/// below gets a fresh one, so there is nothing to move it onto.
///
/// **`changes()`, `total_changes()` and `last_insert_rowid()` are
/// preserved - except when the rebuilt schema holds a view**, which moves
/// the last one; see `last_rowid_after_vacuum`. Every
/// other connection setting is captured before the first reopen and put
/// back after the second, by `ConnectionSettings`.
pub(crate) fn vacuum_in_place(connection: &mut ImportedDatabase) -> DbResult<()> {
    if !connection.schema.imposters.is_empty() {
        return Err(refusal(
            "cannot VACUUM a connection with an imposter table declared - every index gets a fresh tree below",
        ));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos() as u64)
        .unwrap_or(0);
    let scratch = scratch_beside(&connection.storage.path, stamp);
    let _ = connection.storage.vfs.delete(
        &inillucent_vfs::DbPath::new(scratch.to_string_lossy().as_ref()),
        false,
    );
    // **The statement holds the write lock for all of this**, and keeps it
    // until the rename below has landed: `VACUUM` takes it on its way in, and
    // every statement the rebuild runs is nested inside it. No other process
    // can commit to the old file while it is held. See the old connection's
    // close below for the moment that used to let go of it too early.
    connection.rebuild_into(&scratch)?;
    let path = connection.storage.path.clone();
    let page_size = connection.storage.page_size;
    let frames = connection.storage.frames;
    // **Taken before the first `*connection`.** Every reopen and every file
    // operation below has to land on the file system this connection was
    // opened on, and `*connection` replaces the whole `ImportedDatabase` - so
    // the handle is taken here, while it is still the one the caller
    // gave us (task-1946, H2).
    let vfs = std::sync::Arc::clone(&connection.storage.vfs);
    // **And the writer is taken here for the same reason (task-1962, A1
    // step 3).** `crate::connect::Database` holds a second handle on it, so a
    // reopen that left a fresh group behind would leave that handle reading a
    // writer nothing writes to. Put back below, with the reopen's own values
    // copied into it.
    let writer = std::rc::Rc::clone(&connection.writing);
    // The settings group for the same reason, and it is put back before
    // `settings.restore` below so that the restore writes into the group both
    // handles hold rather than into one the reopen made.
    let pragmas = std::rc::Rc::clone(&connection.pragmas);
    // **`changes()`/`total_changes()`/`last_insert_rowid()` are the
    // connection's own history, not a fact about the file `VACUUM` is
    // rewriting, and SQLite's own `VACUUM` leaves them alone.** Assigning
    // through `connection` below replaces the whole `ImportedDatabase` with a
    // freshly opened one, whose `last_changes`/`changed_ever`/
    // `last_rowid`/`session_change_baseline` all start at zero - so the
    // differential suite's `vacuum_matches_sqlite` read `changes()` as 0
    // and `last_insert_rowid()` as 0 straight after a `VACUUM` that
    // changed nothing itself and inserted no row, where the reference
    // still answered whatever the last real write left there. Saved here
    // and restored once, after the second and final swap - nothing runs a
    // statement on it between the two, so there is nothing to restore
    // in between.
    // Carrying the group itself is what keeps them, and it keeps
    // `session_change_baseline` with them, which is the one a reopen could not
    // have reconstructed.
    let counters = std::rc::Rc::clone(&connection.counters);
    let last_rowid = connection.counters.last_rowid.get();
    // The plan cache is carried for the handle rather than for the plans:
    // `crate::connect::Database` holds a second handle on it, and every plan in
    // it names a tree this rebuild has moved, so it takes the reopen's own
    // empty cache through `Compiled::adopt` below.
    let compiled = std::rc::Rc::clone(&connection.compiled);
    let settings = ConnectionSettings::capture(connection);
    let schemas = AttachedSchemas::take(connection);
    // **The rename comes before the old file is closed, so the lock is held
    // across it.** The old connection used to be closed first, which let go of
    // the write lock while the rebuilt file was still only a scratch name.
    // Another process waiting to insert took the lock in that moment and
    // committed to the old file, and the rename then put its rows out of reach:
    // 265 of 502 acknowledged inserts went missing, with no error. A process
    // that takes the lock after the rename finds the file replaced and reopens;
    // see `ImportedDatabase::file_was_replaced`.
    //
    // **On the caller's own VFS.** `ImportedDatabase::open` constructs a
    // fresh `OsVfs`, so this used to move the whole connection onto the
    // operating system's file system for the rest of the session whatever
    // it was opened on (task-1946, H2).
    let rebuilt = reopened(&vfs, &scratch, page_size, frames, "the rebuilt file")?;
    // **The old file's log goes before the rename, while no process but this
    // one can reach the rebuilt file.** `rebuild_into` checkpointed the old
    // file under the write lock this statement still holds, so its log holds
    // nothing the file does not, and no other process can append to it. A
    // crash from here to the rename leaves the old file whole with no log
    // beside it, and a crash after the rename leaves the rebuilt file with no
    // stale log beside it.
    //
    // **After the rename was the wrong place.** The rebuilt file's log is
    // numbered from the start, as every new database's is, and the lock this
    // statement holds is on the old file. So a process that found its file
    // replaced could reopen onto the rebuilt one and write its commits into a
    // segment with the same name as one of the old file's, between this
    // function reading that segment's header and deleting it. The commits
    // were acknowledged and went with the deleted file: in the crash hunter's
    // `VACUUM` scenario, acknowledged deletes came back and inserts went
    // missing in 4 of 30 runs. A trace of one run showed the writer opening
    // the rebuilt file one millisecond after `VACUUM` read the header.
    //
    // The order this replaced, removal before a byte copy, lost the original
    // because the copy could be cut off halfway; see
    // `an_early_segment_removal_loses_the_original_to_an_interrupted_copy`. A
    // rename cannot be.
    // **EXCLUSIVE first, so no process holds the old file at all when it is
    // replaced.** RESERVED keeps writers out and lets readers read, and a
    // process that held SHARED on the old file while the rename landed went on
    // to recover the old file's log under the names the new file's log then
    // used, truncating commits other processes had made. With EXCLUSIVE, a
    // process that holds any lock on the file knows it has not been replaced
    // yet; see `ImportedDatabase::lock_the_file_it_opened`. SQLite's `VACUUM`
    // takes EXCLUSIVE to write its result too.
    connection.storage.database.pool().hold_for_writing()?;
    remove_log_segments(&vfs, &path);
    // **The one crash-sensitive moment.** `commit_rebuild` is a single rename,
    // and once it returns `path` holds the rebuilt bytes durably.
    commit_rebuild(&vfs, &scratch, &path)?;
    // The old connection closes without folding: its file is one no path
    // names now, and its log is gone. It holds nothing to fold, because
    // `rebuild_into` checkpointed it and the rebuild only read.
    connection.storage.read_only = true;
    *connection = rebuilt;
    *connection = reopened(&vfs, &path, page_size, frames, "the database it replaced")?;
    writer.adopt(&connection.writing);
    connection.writing = writer;
    connection.pragmas = pragmas;
    compiled.adopt(&connection.compiled);
    connection.compiled = compiled;
    connection.counters = counters;
    connection
        .counters
        .last_rowid
        .set(last_rowid_after_vacuum(connection, last_rowid));
    // Before the settings: `ConnectionSettings::restore`'s
    // `refresh_catalog` needs the tables `rebuild_tables` derives here
    // already in place to describe them.
    schemas.restore(connection)?;
    settings.restore(connection)?;
    // The scratch name is spent - `commit_rebuild` renamed the file away -
    // so this is only the log segments it picked up along the way.
    remove_log_segments(&vfs, &scratch);
    Ok(())
}

impl ImportedDatabase {
    /// Rebuilds this database into a file that holds nothing spare.
    ///
    /// **What `VACUUM` and `VACUUM INTO` both do**, differing only in where the
    /// result goes. See `crate::rebuild` for why it is a logical copy rather
    /// than a page one - in short, because a page copy reproduces the free
    /// space it was asked to remove.
    ///
    /// @param destination - the file to write, which must not already exist
    pub(crate) fn rebuild_into(&mut self, destination: &std::path::Path) -> DbResult<()> {
        self.checkpoint()?;
        self.rebuild_without_checkpoint(destination)
    }

    /// Writes a rebuilt copy of this database to another file, for `VACUUM INTO`.
    ///
    /// **The checkpoint is a courtesy here, not a need.** The rebuild reads the
    /// source through ordinary statements, which read the file and the log
    /// together, so another process's open transaction, which makes the
    /// checkpoint answer `busy`, does not stop the copy. It failed with
    /// "readers are still present" where SQLite writes the copy. `VACUUM` in
    /// place keeps the checkpoint, because it then renames the result over the
    /// file another process is reading.
    ///
    /// Takes the schema to copy, so `VACUUM temp INTO` and `VACUUM aux INTO` write
    /// that database and not `main` (bug hunt 10).
    ///
    /// @param schema - the schema to copy, as the binder numbers them
    /// @param destination - the file to write, which must not already exist
    pub(crate) fn rebuild_schema_copy_into(
        &mut self,
        schema: usize,
        destination: &std::path::Path,
    ) -> DbResult<()> {
        match self.checkpoint_outside_the_ddl_schema() {
            Err(error) if error.code() == inillucent_base::error::PrimaryCode::Busy => {}
            other => other?,
        }
        crate::rebuild::rebuild_schema_into(
            std::sync::Arc::clone(&self.storage.vfs),
            self,
            schema,
            destination,
            self.storage.page_size,
            self.storage.frames,
        )
    }

    /// The rebuild [`ImportedDatabase::rebuild_into`] runs once the checkpoint is done.
    ///
    /// @param destination - the file to write, which must not already exist
    fn rebuild_without_checkpoint(&mut self, destination: &std::path::Path) -> DbResult<()> {
        crate::rebuild::rebuild_into(
            std::sync::Arc::clone(&self.storage.vfs),
            self,
            destination,
            self.storage.page_size,
            self.storage.frames,
        )
    }

    /// Runs `VACUUM` on `temp` or an attached database.
    ///
    /// **Packs every table and index of the schema into new trees in the same
    /// file, then gives the freed pages back, so the file gets shorter.**
    /// `main` is rebuilt into a new file and renamed over, which is safe because
    /// every process that opens `main` checks for a replaced file
    /// (`file_was_replaced`). Nothing checks that on an attached file, so a
    /// rename would send another process that has the same file attached on
    /// writing to the old file, and its commits would be lost without an error.
    /// This keeps the file where it is. Each table is read out and packed again
    /// the way `ALTER TABLE ... ADD COLUMN` packs one, each index is rebuilt the
    /// way `REINDEX` rebuilds it, and the commit then moves the pages above the
    /// first free page down and cuts the file, with the same step `auto_vacuum`
    /// uses. A delete that left every leaf a tenth full freed one page of 147
    /// before this, and the file kept its size.
    ///
    /// @param schema - the schema to vacuum, as the binder numbers them
    pub(crate) fn vacuum_schema_in_place(&mut self, schema: usize) -> DbResult<()> {
        if self.schema_file(schema).is_none() {
            // A connection that never made a temporary table has no `temp`
            // yet, and SQLite vacuums the empty one without complaint.
            if schema == crate::TEMP {
                return Ok(());
            }
            return Err(refusal(format!(
                "unknown database {}",
                String::from_utf8_lossy(&self.schema_name_of(schema))
            )));
        }
        self.checkpoint_outside_the_ddl_schema()?;
        self.repack_schema(schema)?;
        self.checkpoint_outside_the_ddl_schema()
    }

    /// Packs every table and index of one schema again and queues the pages it
    /// freed to be given back at the commit.
    ///
    /// Virtual tables and views have no tree of their own to pack. An index a
    /// module owns, and the primary key of a `WITHOUT ROWID` table, which is the
    /// table's own tree, are packed with their table.
    ///
    /// @param schema - the schema to pack, as the binder numbers them
    fn repack_schema(&mut self, schema: usize) -> DbResult<()> {
        let tables: Vec<(Vec<u8>, Vec<u8>, Vec<Vec<u8>>)> = self
            .schema
            .tables
            .iter()
            .filter(|table| {
                table.database == schema
                    && table.module.is_none()
                    && table.kind == inillucent_sql::catalog_view::TableKind::Table
            })
            .map(|table| {
                let indexes = table
                    .indexes
                    .iter()
                    .filter(|index| {
                        index.origin != inillucent_sql::catalog_view::IndexOrigin::Module
                            && !(table.without_rowid
                                && index.origin
                                    == inillucent_sql::catalog_view::IndexOrigin::PrimaryKey
                                && index.root == table.root)
                    })
                    .map(|index| index.name.clone())
                    .collect();
                (table.name.clone(), table.folded.clone(), indexes)
            })
            .collect();
        for (name, folded, indexes) in tables {
            self.rebuild_table_tree(schema, &folded, None)?;
            self.rebuild_tables()?;
            self.refresh_catalog();
            for index in indexes {
                self.rebuild_index(schema, &name, &index)?;
                self.rebuild_tables()?;
                self.refresh_catalog();
            }
        }
        self.writing
            .incremental_steps()
            .borrow_mut()
            .insert(schema, u64::MAX);
        self.writing
            .set_touched(self.writing.touched() | crate::schema_bit(schema));
        self.seal()
    }

    /// Checkpoints with `main` as the schema the catalog primitives write to.
    ///
    /// **`execute_ddl` sets the statement's schema for the primitives that
    /// write catalog rows, and a checkpoint writes catalog rows of its own.**
    /// The checkpoint refreshes the statistics of `main`'s tables first, and
    /// with the schema of `VACUUM aux` still selected those rows went into
    /// `aux`'s catalog: `aux.sqlite_master` then listed `main`'s tables and
    /// `aux`'s own were gone (bug hunt 10). `VACUUM` creates and drops nothing,
    /// so it has no use for the selection.
    fn checkpoint_outside_the_ddl_schema(&mut self) -> DbResult<()> {
        let selected = std::mem::replace(&mut self.schema.ddl_schema, crate::MAIN);
        let checkpointed = self.checkpoint();
        self.schema.ddl_schema = selected;
        checkpointed
    }

    /// Rebuilds this database into a database held in memory, and discards it.
    ///
    /// **What `VACUUM INTO ':memory:'` and `VACUUM INTO ''` do.** SQLite opens
    /// the target as a private in memory or temporary database, copies into it,
    /// and closes it, so the statement succeeds and leaves nothing behind. It is
    /// still a full rebuild, which is what makes it a check that the schema and
    /// every row can be replayed.
    pub(crate) fn rebuild_into_memory(&mut self, schema: usize) -> DbResult<()> {
        self.checkpoint_outside_the_ddl_schema()?;
        let vfs: std::sync::Arc<dyn inillucent_vfs::Vfs> =
            std::sync::Arc::new(inillucent_vfs::memory::MemoryVfs::new());
        crate::rebuild::rebuild_schema_into(
            vfs,
            self,
            schema,
            std::path::Path::new("vacuum-into-memory.rdb"),
            self.storage.page_size,
            self.storage.frames,
        )
    }
}

/// Reopens a connection on the file now at its path, keeping everything that belongs to the
/// connection rather than to the file.
///
/// **For a file another process replaced.** `VACUUM` and a backup rename a new file over the
/// database, and every connection that had the old one open has to move to the new one before it
/// reads or writes again; see `ImportedDatabase::file_was_replaced`. What is carried is what
/// `vacuum_in_place` carries across its own reopen: the writer, the settings, the plan cache's
/// handle, the counters, the temporary and attached databases.
///
/// The old connection is marked read only before it is dropped, so its close folds nothing. Its
/// file is one no path names any more, and a fold would roll its log into segment names that now
/// belong to the new file's log.
///
/// @param connection - the connection to move
pub(crate) fn reopen_in_place(connection: &mut ImportedDatabase) -> DbResult<()> {
    let path = connection.storage.path.clone();
    let vfs = std::sync::Arc::clone(&connection.storage.vfs);
    let fresh = reopened(
        &vfs,
        &path,
        connection.storage.page_size,
        connection.storage.frames,
        "the file another process put in its place",
    )?;
    let writer = std::rc::Rc::clone(&connection.writing);
    let pragmas = std::rc::Rc::clone(&connection.pragmas);
    let counters = std::rc::Rc::clone(&connection.counters);
    let compiled = std::rc::Rc::clone(&connection.compiled);
    let settings = ConnectionSettings::capture(connection);
    let schemas = AttachedSchemas::take(connection);
    connection.storage.read_only = true;
    *connection = fresh;
    writer.adopt(&connection.writing);
    connection.writing = writer;
    connection.pragmas = pragmas;
    compiled.adopt(&connection.compiled);
    connection.compiled = compiled;
    connection.counters = counters;
    schemas.restore(connection)?;
    settings.restore(connection)
}

/// A second handle on a database, holding RESERVED so that no other process can write it.
///
/// **What makes a backup over a database another process has open lose nothing.** The backup
/// writes its copy under a scratch name and renames it over the destination, and a process that
/// had the destination open kept writing the old file after the rename, to a file no path names.
/// Held while the old log segments are removed and the rename lands, so no commit can fall between
/// them. RESERVED keeps every writer out and lets readers read. `VACUUM` does not need one: the
/// statement already holds the write lock on its own connection, and keeps it across the rename.
///
/// Dropping it lets the lock go. A writer that was waiting then holds the old file and finds, by
/// `ImportedDatabase::file_was_replaced`, that it has to reopen.
pub(crate) struct WritersHeldOut {
    /// The handle the lock is held on.
    file: Box<dyn inillucent_vfs::VfsFile>,
}

impl WritersHeldOut {
    /// Opens a second handle on a database and takes RESERVED on it, waiting as long as a write
    /// would.
    ///
    /// @param vfs - the file system the database is on
    /// @param path - the database file
    /// @param busy_millis - how long to wait for another writer to finish
    pub(crate) fn take(
        vfs: &std::sync::Arc<dyn inillucent_vfs::Vfs>,
        path: &Path,
        busy_millis: u64,
    ) -> DbResult<WritersHeldOut> {
        let options = inillucent_vfs::OpenOptions {
            create: false,
            ..inillucent_vfs::OpenOptions::of_kind(inillucent_vfs::FileKind::MainDb)
        };
        let file = vfs
            .open(
                &inillucent_vfs::DbPath::new(path.to_string_lossy().as_ref()),
                options,
            )
            .map_err(|error| error.into_db_error())?;
        for level in [
            inillucent_vfs::FileLock::Shared,
            inillucent_vfs::FileLock::Reserved,
        ] {
            inillucent_pool::pool::lock_with_wait(file.as_ref(), level, busy_millis)?;
        }
        Ok(WritersHeldOut { file })
    }
}

impl Drop for WritersHeldOut {
    /// Lets the lock go before the handle closes, so a waiting writer is not left to the
    /// operating system's own time for releasing a closed handle's locks.
    fn drop(&mut self) {
        let _ = self.file.unlock(inillucent_vfs::FileLock::None);
    }
}

#[cfg(test)]
fn has_log_segments(database: &Path) -> bool {
    let Some(directory) = database.parent() else {
        return false;
    };
    let Some(base) = database.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let prefix = format!("{base}-wal.");
    let Ok(listing) = std::fs::read_dir(directory) else {
        return false;
    };
    listing.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&prefix))
    })
}

/// Crash-recovery cases specific to `VACUUM`'s own swap, over real files.
///
/// **Not driven through `SimVfs`.** `ImportedDatabase::create`/`open` (and so
/// `rebuild_into`, which calls them) always build on `OsVfs`, regardless of
/// the vfs the connection they are rebuilding was itself opened on - a
/// pre-existing fact about this engine, not something this ticket changes -
/// and the swap `commit_rebuild` performs is a raw `std::fs::rename` that
/// bypasses the `Vfs` trait entirely, on the task's own instruction. Neither
/// is reachable by `durability.rs`'s call-counting fault injection, which
/// only ever sees calls made through a `Vfs`, so the cases below reproduce
/// the two states a crash can leave `vacuum_in_place` in directly: they run
/// its actual primitives (`rebuild_into`, `commit_rebuild`,
/// `remove_log_segments`) up to a chosen point and stop, exactly as a killed
/// process would, then reopen what is on disk the way a new process starting
/// up would.
#[cfg(test)]
mod vacuum_crash {
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use crate::ImportedDatabase;

    use super::{commit_rebuild, has_log_segments, remove_log_segments, scratch_beside};

    const PAGE_SIZE: usize = 4_096;
    const FRAMES: usize = 256;

    /// The operating system's file system, which is what these tests rebuild on.
    fn os_vfs() -> std::sync::Arc<dyn inillucent_vfs::Vfs> {
        std::sync::Arc::new(inillucent_vfs::OsVfs::new())
    }

    /// Returns a fresh folder under the system temp directory and the path of a
    /// database file inside it.
    ///
    /// **The folder holds the database, its log segments and the rebuild's
    /// scratch file with its own segments.** A case that renames the scratch
    /// file over the database leaves the scratch file's segments behind, so
    /// removing the folder is what leaves nothing in the temp directory. The
    /// caller binds the guard with a named binding and keeps it for the whole
    /// case, declared before the engine opened on the database.
    ///
    /// @param tag - which case this path belongs to, for a reader of the temp
    ///   directory
    fn temp_db_path(tag: &str) -> (inillucent_base::testing::ScratchDir, PathBuf) {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory = inillucent_base::testing::ScratchDir::new(&format!(
            "inillucent-vacuum-crash-{}-{tag}-{unique}",
            std::process::id()
        ));
        let path = directory.join("case.rdb");
        (directory, path)
    }

    /// Builds a real database with something for `VACUUM` to reclaim - the
    /// deleted row leaves a hole the rebuild has to compact away, which is
    /// what makes the rebuilt file's bytes different from the original's
    /// rather than a no-op copy of them.
    ///
    /// @param path - where to create it
    fn seeded(path: &Path) -> ImportedDatabase {
        let mut engine = ImportedDatabase::create(path.to_path_buf(), PAGE_SIZE, FRAMES)
            .expect("the fixture database is created");
        for sql in [
            "CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT)",
            "INSERT INTO t VALUES(1, 'one'), (2, 'two'), (3, 'three'), (4, 'four')",
            "DELETE FROM t WHERE a = 2",
        ] {
            engine
                .execute_any(sql, &inillucent_exec::physical::Params::new())
                .expect("the fixture script runs");
        }
        engine
    }

    /// Returns `t`'s rows, in a stable order.
    ///
    /// @param engine - the connection to read through
    fn rows(engine: &ImportedDatabase) -> Vec<Vec<inillucent_tree::datum::OwnedDatum>> {
        engine
            .run("SELECT a, b FROM t ORDER BY a")
            .expect("the fixture's own query runs")
            .0
    }

    /// A crash between `rebuild_into` returning and the rename that follows
    /// it leaves the original file exactly as it was.
    ///
    /// Nothing above the rename touches `path` or its log segments, so this
    /// is true by construction rather than by anything worth measuring - it
    /// is here as the other half of the pair with the case below, which is
    /// the one this ticket's fix is actually about.
    #[test]
    fn a_crash_before_the_rename_recovers_the_original() {
        let (_scratch_dir, path) = temp_db_path("before-rename");
        let mut engine = seeded(&path);
        let before = rows(&engine);

        let scratch = scratch_beside(&path, 1);
        let _ = std::fs::remove_file(&scratch);
        engine
            .rebuild_into(&scratch)
            .expect("the rebuild checkpoints the scratch file durably");
        // The crash: everything `vacuum_in_place` still had left to do -
        // closing this handle, renaming `scratch` over `path`, removing
        // `path`'s segments - never runs. `engine` is simply dropped, as a
        // killed process's connection would be.
        drop(engine);

        let reopened = ImportedDatabase::open(path.clone(), PAGE_SIZE, FRAMES)
            .expect("the untouched original still opens");
        assert_eq!(
            rows(&reopened),
            before,
            "a crash before the rename must leave the original database exactly as it was"
        );
        drop(reopened);
    }

    /// A crash between removing the original's log segments and the rename,
    /// which is the order `vacuum_in_place` runs in, leaves the original whole.
    ///
    /// `rebuild_into` checkpoints the original before it copies a row, so the
    /// file holds everything its log does and the log can go first. It goes
    /// first because a process that reopened onto the rebuilt file could write
    /// a segment with the name of one of the original's while it was being
    /// removed after the rename.
    #[test]
    fn a_crash_after_the_early_segment_removal_recovers_the_original() {
        let (_scratch_dir, path) = temp_db_path("segments-gone");
        let mut engine = seeded(&path);
        let before = rows(&engine);

        let scratch = scratch_beside(&path, 4);
        let _ = std::fs::remove_file(&scratch);
        engine
            .rebuild_into(&scratch)
            .expect("the rebuild checkpoints the original and writes the scratch file");
        assert!(
            has_log_segments(&path),
            "the original has log segments for this case to remove"
        );
        remove_log_segments(&os_vfs(), &path);
        // The crash: the rename never runs.
        drop(engine);

        let reopened = ImportedDatabase::open(path.clone(), PAGE_SIZE, FRAMES)
            .expect("the original opens without its log");
        assert_eq!(
            rows(&reopened),
            before,
            "the original holds every row its log did"
        );
        drop(reopened);
    }

    /// A crash after the rename lands, but before `path`'s pre-rebuild log
    /// segments are removed, must still recover the rebuilt file - not the
    /// pre-rebuild pages those stale segments describe, replayed back over
    /// it.
    ///
    /// This is the window the fix opens that the old code did not have in
    /// this shape: the old order removed the segments *before* the swap,
    /// which closed this window by opening the one
    /// `an_early_segment_removal_loses_the_original_to_an_interrupted_copy`
    /// demonstrates instead. Proving both stay safe under their own ordering
    /// is what justifies moving the removal rather than just picking a
    /// different unsafe order.
    #[test]
    fn a_crash_after_the_rename_recovers_the_rebuilt_file() {
        let (_scratch_dir, path) = temp_db_path("after-rename");
        let mut engine = seeded(&path);
        let before = rows(&engine);

        let scratch = scratch_beside(&path, 2);
        let _ = std::fs::remove_file(&scratch);
        engine
            .rebuild_into(&scratch)
            .expect("the rebuild checkpoints the scratch file durably");
        assert!(
            has_log_segments(&path),
            "the fixture must still carry its own log segments for this case to test anything"
        );
        // Closes the handle on `path`, as `vacuum_in_place` does before the
        // rename - a rename cannot replace a file this process still has open
        // for writing.
        engine = ImportedDatabase::open(scratch.clone(), PAGE_SIZE, FRAMES)
            .expect("the scratch reopens");
        commit_rebuild(&os_vfs(), &scratch, &path).expect("the rename lands");
        // The crash: `remove_log_segments(&path)`, the very next line in
        // `vacuum_in_place`, never runs - `path`'s pre-rebuild segments are
        // still sitting beside the rebuilt file.
        drop(engine);

        let reopened = ImportedDatabase::open(path.clone(), PAGE_SIZE, FRAMES)
            .expect("the rebuilt file must still open despite the stale segments left beside it");
        assert_eq!(
            rows(&reopened),
            before,
            "a crash after the rename must recover the rebuilt file, not a replay of the segments \
             that described the file it replaced"
        );
        drop(reopened);
    }

    /// The order `vacuum_in_place` used to run in - the original's log
    /// segments removed *before* anything overwrites `path`, and a byte copy
    /// rather than a rename - loses data when the copy is interrupted.
    ///
    /// Reproduced directly against this file's own primitives rather than by
    /// reverting `vacuum_in_place`, so this keeps demonstrating the old
    /// defect even after nothing in the source still runs in that order.
    #[test]
    fn an_early_segment_removal_loses_the_original_to_an_interrupted_copy() {
        let (_scratch_dir, path) = temp_db_path("old-order");
        let mut engine = seeded(&path);
        let before = rows(&engine);

        let scratch = scratch_beside(&path, 3);
        let _ = std::fs::remove_file(&scratch);
        engine
            .rebuild_into(&scratch)
            .expect("the rebuild checkpoints the scratch file durably");
        engine = ImportedDatabase::open(scratch.clone(), PAGE_SIZE, FRAMES)
            .expect("the scratch reopens");

        // The defect: the original's segments are gone *before* `path` has
        // been touched at all.
        remove_log_segments(&os_vfs(), &path);
        assert!(
            !has_log_segments(&path),
            "the segments this case removes early must actually have existed"
        );

        // The old mechanism: a byte copy, interrupted partway - exactly what
        // `std::fs::copy` gave a crash the room to do, and a rename never
        // does. Half of `scratch`'s bytes land in `path`; the write stops
        // there, as a killed process's would.
        let rebuilt_bytes = std::fs::read(&scratch).expect("the scratch file reads back");
        let torn = &rebuilt_bytes[..rebuilt_bytes.len() / 2];
        std::fs::write(&path, torn).expect("the torn write lands");
        drop(engine);

        // What the old order leaves behind: `path` holding half the rebuilt
        // file's bytes, with its own recovery log already deleted. There is
        // no longer a definition of "recovers correctly" available to it -
        // either it is detected as unreadable, or it opens and answers
        // something other than the rows it held a moment before. Both are
        // the loss this ticket's fix removes; neither is "either the original
        // or the rebuilt file, whole", which is what the comment above
        // `vacuum_in_place` used to claim.
        match ImportedDatabase::open(path.clone(), PAGE_SIZE, FRAMES) {
            Err(_) => {}
            Ok(reopened) => {
                assert_ne!(
                    rows(&reopened),
                    before,
                    "a torn write over a file whose own recovery log was already deleted read back \
                     correctly by coincidence on this run - the old order still cannot be trusted \
                     in general, but this run did not demonstrate why"
                );
            }
        }
    }

    /// `VACUUM` refuses a connection with an imposter table declared.
    ///
    /// Unlike a temporary table or an attachment, an imposter is bound into
    /// `main`'s own trees by page - every index gets a fresh one when the
    /// schema is replayed into the rebuilt file, so there is no tree left for
    /// the declaration to still name afterward.
    #[test]
    fn a_declared_imposter_refuses_vacuum() {
        let (_scratch_dir, path) = temp_db_path("imposter-refusal");
        let mut engine = seeded(&path);
        engine
            .execute_any(
                "CREATE INDEX t_b ON t(b)",
                &inillucent_exec::physical::Params::new(),
            )
            .expect("the index the imposter reads is created");
        engine
            .imposter(Some(b"t_b"), b"imposter_t_b")
            .expect("the imposter declares");

        let refused = engine.execute_any("VACUUM", &inillucent_exec::physical::Params::new());
        assert!(
            refused.is_err(),
            "VACUUM must refuse a connection with an imposter table declared"
        );
        drop(engine);
    }
}

//! A seeded workload that several real processes run against one file at once,
//! and the checks that grade what the file holds afterwards.
//!
//! Invariant: **every transaction a worker acknowledged is in the file whole,
//! no transaction is ever seen in part, and the money in `account` always adds
//! up to the amount it started with.** Each of those is checked by a process
//! that did not write the data: the readers while the workers run, and the
//! parent test once they have stopped.
//!
//! ## Why a seeded plan rather than a log of what was written
//!
//! A worker is killed at a moment nobody chose, so whatever it meant to write
//! next is lost with it. The plan for transaction `seq` of a worker is a pure
//! function of the worker's seed and `seq` ([`step`]), so the checker can
//! rebuild exactly what each acknowledged transaction wrote, byte for byte,
//! without the worker having told it anything but the number. A retry after a
//! busy refusal writes the same bytes again, because it is the same `seq`.
//!
//! ## What one transaction does
//!
//! - inserts between one and four `ledger` rows, numbered `part` from zero, each
//!   carrying the total `parts` so a reader can tell a whole transaction from
//!   part of one; one value in twenty is larger than a page, so overflow chains
//!   are written and freed all the time;
//! - now and then inserts [`Settings::bulk_parts`] rows of 24 KiB in one go,
//!   which with a small pool is a transaction larger than the cache: the path
//!   that spills pages before commit, where task-2166 and task-2169 found their
//!   damage;
//! - moves an amount between two accounts, so a reader that sees one half of a
//!   transfer sees the total change;
//! - deletes the rows of transaction `seq - WINDOW`, so pages are freed and
//!   reused while other processes read them;
//! - records `seq` in `progress`, so the checker knows whether the transaction
//!   after the last acknowledged one committed before the kill;
//! - optionally writes one row of an `inillucent_search` table and deletes the
//!   one from `WINDOW` transactions ago, so the search table's own storage is
//!   written by several processes too.
//!
//! Before some transactions it also opens one, writes to it and rolls it back,
//! because a rollback that left anything behind is the defect task-2169 fixed.

use inillucent_driver::{Connection, Database, EncryptionKey, OpenOptions, Status, Value};

/// How many accounts the transfers move money between.
pub const ACCOUNTS: i64 = 32;

/// What every account holds when the database is built.
pub const OPENING_BALANCE: i64 = 1_000;

/// How many transactions a worker's rows stay for before a later one deletes them.
///
/// Twelve, so a worker that has committed more than twelve transactions always
/// has exactly twelve of them in the file, and a missing or extra one is a count
/// a reader can see without knowing the seed.
pub const WINDOW: i64 = 12;

/// The size of one row of a bulk transaction.
const BULK_BODY: usize = 24 * 1024;

/// How a worker or reader opens the file and what the workload includes.
#[derive(Clone, Debug, Default)]
pub struct Settings {
    /// How many frames the buffer pool holds. Zero means the driver's default.
    pub frames: usize,
    /// The passphrase the database is encrypted with, when it is.
    pub key: Option<String>,
    /// Whether the schema has the `note` search table and the workload writes it.
    pub search: bool,
    /// How many 24 KiB rows a bulk transaction writes. Zero turns bulk
    /// transactions off.
    pub bulk_parts: usize,
    /// How long to wait for a lock another process holds, in milliseconds.
    pub busy_ms: u64,
}

/// What one transaction of one worker writes, rebuilt from the seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The transaction's number, from one.
    pub seq: i64,
    /// The `ledger` bodies, in `part` order.
    pub bodies: Vec<Vec<u8>>,
    /// The account money leaves.
    pub from: i64,
    /// The account money arrives in.
    pub to: i64,
    /// How much moves.
    pub amount: i64,
    /// Whether a transaction is opened and rolled back before this one.
    pub rollback_first: bool,
    /// Whether the worker asks for a checkpoint after this one commits.
    pub checkpoint_after: bool,
}

/// A small generator with a seed, splitmix64.
///
/// Here rather than a crate because the workspace may not take a dependency
/// for a few lines, and because the checker must produce the same numbers on
/// every platform for ever: a generator that a dependency upgrade could change
/// would turn every recorded seed into a different workload.
#[derive(Clone, Debug)]
pub struct Mix(u64);

impl Mix {
    /// Starts a generator.
    ///
    /// @param seed - the starting state
    pub fn new(seed: u64) -> Mix {
        Mix(seed)
    }

    /// Returns the next number.
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Returns a number in `low..high`, or `low` when the range is empty.
    ///
    /// @param low - the smallest value
    /// @param high - one past the largest value
    pub fn range(&mut self, low: u64, high: u64) -> u64 {
        if high <= low {
            return low;
        }
        low + self.next_u64() % (high - low)
    }

    /// Returns true about once in `n` calls.
    ///
    /// @param n - how rare the event is
    pub fn one_in(&mut self, n: u64) -> bool {
        n > 0 && self.next_u64().is_multiple_of(n)
    }
}

/// Returns the FNV-1a digest of some bytes, as the signed integer SQL stores.
///
/// @param bytes - what to digest
pub fn digest(bytes: &[u8]) -> i64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash as i64
}

/// Returns `length` bytes that only this worker, transaction and part produce.
///
/// @param mix - the generator for this part
/// @param length - how many bytes
fn body(mix: &mut Mix, length: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(length + 8);
    while out.len() < length {
        out.extend_from_slice(&mix.next_u64().to_le_bytes());
    }
    out.truncate(length);
    out
}

/// Rebuilds what transaction `seq` of the worker with this seed writes.
///
/// @param seed - the worker's seed
/// @param seq - the transaction's number, from one
/// @param settings - which parts of the workload are on
pub fn step(seed: u64, seq: i64, settings: &Settings) -> Step {
    let mut mix = Mix::new(seed ^ (seq as u64).wrapping_mul(0xA24B_AED4_963E_E407));
    let bulk = settings.bulk_parts > 0 && mix.one_in(29);
    let count = if bulk {
        settings.bulk_parts
    } else {
        mix.range(1, 5) as usize
    };
    let mut bodies = Vec::with_capacity(count);
    for _ in 0..count {
        let length = if bulk {
            BULK_BODY
        } else if mix.one_in(20) {
            mix.range(40_000, 140_000) as usize
        } else if mix.one_in(4) {
            mix.range(1_000, 8_000) as usize
        } else {
            mix.range(16, 400) as usize
        };
        bodies.push(body(&mut mix, length));
    }
    let from = mix.range(1, ACCOUNTS as u64 + 1) as i64;
    let to = mix.range(1, ACCOUNTS as u64 + 1) as i64;
    Step {
        seq,
        bodies,
        from,
        to,
        amount: mix.range(1, 50) as i64,
        rollback_first: mix.one_in(9),
        checkpoint_after: mix.one_in(15),
    }
}

/// Returns the statements that build an empty workload database.
///
/// @param settings - whether the search table is part of it
pub fn schema(settings: &Settings) -> String {
    let mut sql = String::from(
        "CREATE TABLE account (id INTEGER PRIMARY KEY, balance INTEGER NOT NULL);
         CREATE TABLE ledger (worker TEXT NOT NULL, seq INTEGER NOT NULL, part INTEGER NOT NULL,
                              parts INTEGER NOT NULL, body BLOB NOT NULL, digest INTEGER NOT NULL,
                              UNIQUE (worker, seq, part));
         CREATE INDEX ledger_digest ON ledger (digest);
         CREATE TABLE progress (worker TEXT PRIMARY KEY, last_seq INTEGER NOT NULL);",
    );
    if settings.search {
        sql.push_str(
            "CREATE VIRTUAL TABLE note USING inillucent_search(worker, body, dims = 4, compact = 0);",
        );
    }
    sql
}

/// Opens the database the way every process in a run opens it.
///
/// @param path - the file
/// @param settings - the pool size, key and lock wait
pub fn open(path: &std::path::Path, settings: &Settings) -> Result<Database, String> {
    let mut options = OpenOptions::default();
    if settings.frames > 0 {
        options.cache_frames = settings.frames;
    }
    options.key = settings.key.as_deref().map(EncryptionKey::parse);
    options.busy_timeout = Some(std::time::Duration::from_millis(settings.busy_ms.max(1)));
    options.diagnostics = true;
    Database::open_with(path, options).map_err(|error| describe(&error))
}

/// Returns an error as one line with its status, message and detail.
///
/// @param error - the driver's error
pub fn describe(error: &inillucent_driver::Error) -> String {
    format!(
        "{}: {}{}",
        error.status.name(),
        error.message,
        error
            .detail
            .as_deref()
            .map(|detail| format!(" ({detail})"))
            .unwrap_or_default()
    )
}

/// Builds the schema and the opening balances in a new file.
///
/// @param path - the file, which must not exist yet
/// @param settings - which parts of the workload are on
pub fn setup(path: &std::path::Path, settings: &Settings) -> Result<(), String> {
    let database = open(path, settings)?;
    let connection = database.session();
    connection
        .execute_batch(&schema(settings))
        .map_err(|error| describe(&error))?;
    let transaction = connection.begin().map_err(|error| describe(&error))?;
    for id in 1..=ACCOUNTS {
        transaction
            .execute(
                "INSERT INTO account (id, balance) VALUES (?1, ?2)",
                &[Value::Integer(id), Value::Integer(OPENING_BALANCE)],
            )
            .map_err(|error| describe(&error))?;
    }
    transaction.commit().map_err(|error| describe(&error))
}

/// The rowid a worker's search row for one transaction takes.
///
/// @param id - the worker's number, unique in the run
/// @param seq - the transaction's number
pub fn note_rowid(id: i64, seq: i64) -> i64 {
    id * 1_000_000 + seq
}

/// Writes one transaction, inside a transaction the caller opened.
///
/// @param transaction - the open transaction
/// @param name - the worker's name
/// @param id - the worker's number
/// @param step - what to write
/// @param settings - whether the search table is written
fn write_step(
    transaction: &inillucent_driver::Transaction<'_>,
    name: &str,
    id: i64,
    step: &Step,
    settings: &Settings,
) -> inillucent_driver::Result<()> {
    let parts = step.bodies.len() as i64;
    for (part, bytes) in step.bodies.iter().enumerate() {
        transaction.execute(
            "INSERT INTO ledger (worker, seq, part, parts, body, digest) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            &[
                Value::Text(name.to_string()),
                Value::Integer(step.seq),
                Value::Integer(part as i64),
                Value::Integer(parts),
                Value::Blob(bytes.clone()),
                Value::Integer(digest(bytes)),
            ],
        )?;
    }
    transaction.execute(
        "UPDATE account SET balance = balance - ?2 WHERE id = ?1",
        &[Value::Integer(step.from), Value::Integer(step.amount)],
    )?;
    transaction.execute(
        "UPDATE account SET balance = balance + ?2 WHERE id = ?1",
        &[Value::Integer(step.to), Value::Integer(step.amount)],
    )?;
    transaction.execute(
        "DELETE FROM ledger WHERE worker = ?1 AND seq = ?2",
        &[
            Value::Text(name.to_string()),
            Value::Integer(step.seq - WINDOW),
        ],
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO progress (worker, last_seq) VALUES (?1, ?2)",
        &[Value::Text(name.to_string()), Value::Integer(step.seq)],
    )?;
    if settings.search {
        let vector = format!(
            "[{}, {}, 1, 0.5]",
            (step.seq % 7) as f64 / 7.0,
            (id % 5) as f64 / 5.0
        );
        transaction.execute(
            "INSERT INTO note (rowid, worker, body, vector) VALUES (?1, ?2, ?3, ?4)",
            &[
                Value::Integer(note_rowid(id, step.seq)),
                Value::Text(name.to_string()),
                Value::Text(format!("{name} seq{} token{id}", step.seq)),
                Value::Text(vector),
            ],
        )?;
        if step.seq > WINDOW {
            transaction.execute(
                "DELETE FROM note WHERE rowid = ?1",
                &[Value::Integer(note_rowid(id, step.seq - WINDOW))],
            )?;
        }
    }
    Ok(())
}

/// Opens a transaction, writes rows that must never be seen, and rolls it back.
///
/// @param connection - the worker's connection
/// @param name - the worker's name, which the rows are written under with a suffix
/// @param step - the transaction this one goes before
fn doomed(connection: &Connection<'_>, name: &str, step: &Step) -> inillucent_driver::Result<()> {
    let transaction = connection.begin()?;
    for (part, bytes) in step.bodies.iter().enumerate() {
        transaction.execute(
            "INSERT INTO ledger (worker, seq, part, parts, body, digest) VALUES (?1, ?2, ?3, 0, ?4, 0)",
            &[
                Value::Text(format!("{name}-rolled-back")),
                Value::Integer(step.seq),
                Value::Integer(part as i64),
                Value::Blob(bytes.clone()),
            ],
        )?;
    }
    transaction.execute(
        "UPDATE account SET balance = balance + 1000000 WHERE id = ?1",
        &[Value::Integer(step.from)],
    )?;
    transaction.rollback()
}

/// What happened when a worker tried one transaction.
#[derive(Debug)]
pub enum Outcome {
    /// It committed.
    Committed,
    /// Another process held the file past the wait; the same `seq` is tried again.
    Busy,
    /// Anything else, which ends the worker.
    Failed(String),
}

/// Tries transaction `step` once: the doomed rollback first when the plan says
/// so, then the real one.
///
/// @param connection - the worker's connection
/// @param name - the worker's name
/// @param id - the worker's number
/// @param step - what to write
/// @param settings - which parts of the workload are on
pub fn attempt(
    connection: &Connection<'_>,
    name: &str,
    id: i64,
    step: &Step,
    settings: &Settings,
) -> Outcome {
    let classify = |error: inillucent_driver::Error| {
        if error.status == Status::Busy {
            Outcome::Busy
        } else {
            Outcome::Failed(describe(&error))
        }
    };
    if step.rollback_first {
        if let Err(error) = doomed(connection, name, step) {
            return classify(error);
        }
    }
    let transaction = match connection.begin() {
        Ok(transaction) => transaction,
        Err(error) => return classify(error),
    };
    if let Err(error) = write_step(&transaction, name, id, step, settings) {
        let _ = transaction.rollback();
        return classify(error);
    }
    match transaction.commit() {
        Ok(()) => Outcome::Committed,
        Err(error) => classify(error),
    }
}

/// Returns the first value of the first row of a query, as an integer.
///
/// @param rows - what the query returned
fn integer(rows: &inillucent_driver::Rows) -> Option<i64> {
    match rows.rows.first().and_then(|row| row.first()) {
        Some(Value::Integer(value)) => Some(*value),
        Some(Value::Null) => Some(0),
        _ => None,
    }
}

/// Returns a value as an integer, or a description of what it was instead.
///
/// @param value - the value
fn as_integer(value: Option<&Value>) -> Result<i64, String> {
    match value {
        Some(Value::Integer(number)) => Ok(*number),
        other => Err(format!("expected an integer, found {other:?}")),
    }
}

/// Runs a query inside the reader's transaction and returns every row.
///
/// @param connection - the connection holding the read transaction
/// @param sql - the statement
/// @param values - its bound values
fn read(
    connection: &Connection<'_>,
    sql: &str,
    values: &[Value],
) -> Result<inillucent_driver::Rows, inillucent_driver::Error> {
    connection.query(sql, values, usize::MAX)
}

/// A reader's verdict on one snapshot.
#[derive(Debug)]
pub enum Check {
    /// Every invariant held. The number is how many committed worker
    /// transactions the snapshot held, so a run can show readers saw work.
    Held(i64),
    /// The file was busy past the wait, with the engine's words for why.
    Busy(String),
    /// An invariant failed, or a statement failed for a reason other than busy.
    Broken(String),
}

/// Checks everything that must hold of any snapshot, without knowing any seed.
///
/// - the balances add up to the opening total;
/// - every `(worker, seq)` in `ledger` has all its parts and no others;
/// - every worker's rows are exactly its last `WINDOW` transactions, ending at
///   the `seq` `progress` records for it, in the same snapshot;
/// - no row written by a rolled back transaction is visible;
/// - the index on `digest` holds the same number of rows as the table;
/// - a sample of bodies still match their digests;
/// - with the search table, it holds one row per transaction in the window.
///
/// @param connection - a connection on the file
/// @param settings - whether the search table is part of the workload
/// @param hold - how long to keep the snapshot open after reading it
/// @param sample - which rows' bodies to digest: those whose rowid modulo 13 equals this
pub fn check_snapshot(
    connection: &Connection<'_>,
    settings: &Settings,
    hold: std::time::Duration,
    sample: i64,
) -> Check {
    // **A deferred `BEGIN`, not the driver's `begin`.** The driver's
    // transaction is for writing and asks for the write lock, so beside a
    // writer holding RESERVED it is refused for as long as that writer runs.
    // A reader's transaction takes SHARED at its first read, which is what an
    // application that reads in a transaction gets.
    match connection.execute_batch("BEGIN") {
        Ok(()) => {}
        Err(error) if error.status == Status::Busy => return Check::Busy(describe(&error)),
        Err(error) => return Check::Broken(format!("begin: {}", describe(&error))),
    }
    let verdict = snapshot_verdict(connection, settings, sample);
    if !hold.is_zero() {
        std::thread::sleep(hold);
    }
    let _ = connection.execute_batch("ROLLBACK");
    match verdict {
        Ok(seen) => Check::Held(seen),
        Err(Fault::Busy(why)) => Check::Busy(why),
        Err(Fault::Broken(why)) => Check::Broken(why),
    }
}

/// Why a snapshot check stopped.
enum Fault {
    /// A statement was refused as busy, with the engine's words for why.
    Busy(String),
    /// An invariant failed, or a statement failed otherwise.
    Broken(String),
}

impl From<inillucent_driver::Error> for Fault {
    /// Sorts a driver error into busy and everything else.
    ///
    /// @param error - the driver's error
    fn from(error: inillucent_driver::Error) -> Fault {
        if error.status == Status::Busy {
            Fault::Busy(describe(&error))
        } else {
            Fault::Broken(describe(&error))
        }
    }
}

/// The body of [`check_snapshot`], with `?` for the statements.
///
/// @param transaction - the connection holding the open read transaction
/// @param settings - whether the search table is part of the workload
/// @param sample - which rows' bodies to digest
fn snapshot_verdict(
    transaction: &Connection<'_>,
    settings: &Settings,
    sample: i64,
) -> Result<i64, Fault> {
    let balances = read(
        transaction,
        "SELECT sum(balance), count(*) FROM account",
        &[],
    )?;
    let total = integer(&balances);
    if total != Some(ACCOUNTS * OPENING_BALANCE) {
        return Err(Fault::Broken(format!(
            "the balances add up to {total:?}, not {}: a transfer was seen in part",
            ACCOUNTS * OPENING_BALANCE
        )));
    }
    let torn = read(
        transaction,
        "SELECT worker, seq, count(*), min(parts), max(parts), min(part), max(part) FROM ledger
         GROUP BY worker, seq
         HAVING count(*) != min(parts) OR min(parts) != max(parts) OR min(part) != 0 OR max(part) != min(parts) - 1
         LIMIT 5",
        &[],
    )?;
    if !torn.rows.is_empty() {
        return Err(Fault::Broken(format!(
            "a transaction is in the file in part: {:?}",
            torn.rows
        )));
    }
    let ghosts = read(
        transaction,
        "SELECT count(*) FROM ledger WHERE worker LIKE '%-rolled-back'",
        &[],
    )?;
    if integer(&ghosts) != Some(0) {
        return Err(Fault::Broken(format!(
            "{:?} rows written by a rolled back transaction are visible",
            integer(&ghosts)
        )));
    }
    let windows = read(
        transaction,
        "SELECT p.worker, p.last_seq, count(DISTINCT l.seq), min(l.seq), max(l.seq)
         FROM progress p LEFT JOIN ledger l ON l.worker = p.worker
         GROUP BY p.worker, p.last_seq",
        &[],
    )?;
    let mut seen = 0i64;
    for row in &windows.rows {
        let last = as_integer(row.get(1)).map_err(Fault::Broken)?;
        let distinct = as_integer(row.get(2)).map_err(Fault::Broken)?;
        let expected = last.min(WINDOW);
        seen += last;
        let low = row.get(3).cloned().unwrap_or(Value::Null);
        let high = row.get(4).cloned().unwrap_or(Value::Null);
        if distinct != expected
            || high != Value::Integer(last)
            || low != Value::Integer(last - expected + 1)
        {
            return Err(Fault::Broken(format!(
                "worker {:?} records seq {last} but its rows are {distinct} transactions from {low:?} to {high:?}",
                row.first()
            )));
        }
    }
    let orphans = read(
        transaction,
        "SELECT count(*) FROM ledger WHERE worker NOT IN (SELECT worker FROM progress)",
        &[],
    )?;
    if integer(&orphans) != Some(0) {
        return Err(Fault::Broken(format!(
            "{:?} ledger rows belong to no worker in progress",
            integer(&orphans)
        )));
    }
    let through_table = integer(&read(transaction, "SELECT count(*) FROM ledger", &[])?);
    let through_index = integer(&read(
        transaction,
        "SELECT count(*) FROM ledger INDEXED BY ledger_digest WHERE digest >= -9223372036854775808",
        &[],
    )?);
    if through_table != through_index {
        return Err(Fault::Broken(format!(
            "the table holds {through_table:?} rows and its digest index {through_index:?}"
        )));
    }
    let bodies = read(
        transaction,
        "SELECT worker, seq, part, body, digest FROM ledger WHERE rowid % 13 = ?1",
        &[Value::Integer(sample.rem_euclid(13))],
    )?;
    for row in &bodies.rows {
        let stored = as_integer(row.get(4)).map_err(Fault::Broken)?;
        let actual = match row.get(3) {
            Some(Value::Blob(bytes)) => digest(bytes),
            other => return Err(Fault::Broken(format!("a body is {other:?}, not a blob"))),
        };
        if stored != actual {
            return Err(Fault::Broken(format!(
                "the body of {:?} seq {:?} part {:?} no longer matches its digest",
                row.first(),
                row.get(1),
                row.get(2)
            )));
        }
    }
    if settings.search {
        check_search(transaction)?;
    }
    Ok(seen)
}

/// Checks that the search table holds one row per transaction in each window,
/// and that a keyword search finds them.
///
/// @param transaction - the connection holding the open read transaction
fn check_search(transaction: &Connection<'_>) -> Result<(), Fault> {
    let notes = integer(&read(transaction, "SELECT count(*) FROM note", &[])?);
    let expected = integer(&read(
        transaction,
        "SELECT count(*) FROM (SELECT DISTINCT worker, seq FROM ledger)",
        &[],
    )?);
    if notes != expected {
        return Err(Fault::Broken(format!(
            "the search table holds {notes:?} rows and the ledger {expected:?} transactions"
        )));
    }
    let workers = read(transaction, "SELECT worker FROM progress LIMIT 1", &[])?;
    if let Some(Value::Text(name)) = workers.rows.first().and_then(|row| row.first()) {
        let window = integer(&read(
            transaction,
            "SELECT count(DISTINCT seq) FROM ledger WHERE worker = ?1",
            &[Value::Text(name.clone())],
        )?);
        let token = format!("\"{name}\"");
        let found = integer(&read(
            transaction,
            "SELECT count(*) FROM (SELECT rowid FROM note WHERE note MATCH ?1 AND k = 1000)",
            &[Value::Text(token)],
        )?);
        if found != window {
            return Err(Fault::Broken(format!(
                "a keyword search for worker {name} finds {found:?} rows and its window holds {window:?}"
            )));
        }
    }
    Ok(())
}

/// One run of one worker, as the parent saw it.
#[derive(Clone, Debug)]
pub struct Incarnation {
    /// The worker's name, unique in the run.
    pub name: String,
    /// Its seed.
    pub seed: u64,
    /// The highest `seq` it acknowledged, zero for none.
    pub acked: i64,
    /// Whether it printed that it had finished, so nothing after `acked` can have committed.
    pub finished: bool,
}

/// Grades the file after every process has stopped.
///
/// On top of [`check_snapshot`]: for each worker, the `seq` recorded in
/// `progress` is the last one it acknowledged, or one more when it was killed
/// between a commit and the acknowledgement, and every row in its window holds
/// exactly the bytes the seed says it wrote. Then the file's own integrity check.
///
/// @param path - the file
/// @param settings - how to open it and what the workload includes
/// @param incarnations - every worker that ran
pub fn verify(
    path: &std::path::Path,
    settings: &Settings,
    incarnations: &[Incarnation],
) -> Result<String, String> {
    let database = open(path, settings)?;
    database
        .integrity_check()
        .map_err(|error| format!("integrity check: {}", describe(&error)))?;
    let connection = database.session();
    let snapshot = match check_snapshot(&connection, settings, std::time::Duration::ZERO, 0) {
        Check::Held(seen) => seen,
        Check::Busy(why) => return Err(format!("the final check was refused as busy: {why}")),
        Check::Broken(why) => return Err(why),
    };
    let mut rows = 0usize;
    for incarnation in incarnations {
        rows += verify_worker(&connection, settings, incarnation)?;
    }
    Ok(format!(
        "{} workers, {snapshot} transactions recorded, {rows} rows checked byte for byte",
        incarnations.len()
    ))
}

/// Grades one worker's window against its seed.
///
/// @param connection - a connection on the file
/// @param settings - which parts of the workload are on
/// @param incarnation - the worker
fn verify_worker(
    connection: &Connection<'_>,
    settings: &Settings,
    incarnation: &Incarnation,
) -> Result<usize, String> {
    let name = Value::Text(incarnation.name.clone());
    let recorded = connection
        .query_all(
            "SELECT last_seq FROM progress WHERE worker = ?1",
            std::slice::from_ref(&name),
        )
        .map_err(|error| describe(&error))?;
    let last = integer(&recorded).unwrap_or(0);
    let allowed_extra = if incarnation.finished { 0 } else { 1 };
    if last < incarnation.acked || last > incarnation.acked + allowed_extra {
        return Err(format!(
            "worker {} acknowledged seq {} (finished: {}) and the file records {last}: {}",
            incarnation.name,
            incarnation.acked,
            incarnation.finished,
            if last < incarnation.acked {
                "an acknowledged transaction was lost"
            } else {
                "a transaction committed that was never attempted"
            }
        ));
    }
    let mut checked = 0usize;
    for seq in (last - WINDOW + 1).max(1)..=last {
        let expected = step(incarnation.seed, seq, settings);
        let found = connection
            .query_all(
                "SELECT part, body FROM ledger WHERE worker = ?1 AND seq = ?2 ORDER BY part",
                &[name.clone(), Value::Integer(seq)],
            )
            .map_err(|error| describe(&error))?;
        if found.rows.len() != expected.bodies.len() {
            return Err(format!(
                "worker {} seq {seq} has {} rows, and its plan wrote {}",
                incarnation.name,
                found.rows.len(),
                expected.bodies.len()
            ));
        }
        for (row, bytes) in found.rows.iter().zip(&expected.bodies) {
            if row.get(1) != Some(&Value::Blob(bytes.clone())) {
                return Err(format!(
                    "worker {} seq {seq} part {:?} does not hold the bytes its plan wrote",
                    incarnation.name,
                    row.first()
                ));
            }
            checked += 1;
        }
    }
    Ok(checked)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plan is a function of the seed and the number, and nothing else.
    #[test]
    fn a_step_is_the_same_every_time_it_is_rebuilt() {
        let settings = Settings {
            bulk_parts: 8,
            ..Settings::default()
        };
        for seq in 1..200 {
            assert_eq!(step(7, seq, &settings), step(7, seq, &settings));
        }
        assert_ne!(step(7, 1, &settings), step(8, 1, &settings));
    }

    /// Bulk transactions, large values, rollbacks and checkpoints all occur in
    /// a few hundred steps, so a short run exercises every path the plan has.
    #[test]
    fn every_kind_of_step_occurs_in_a_short_plan() {
        let settings = Settings {
            bulk_parts: 8,
            ..Settings::default()
        };
        let steps: Vec<Step> = (1..300).map(|seq| step(11, seq, &settings)).collect();
        assert!(steps.iter().any(|step| step.bodies.len() == 8));
        assert!(steps
            .iter()
            .any(|step| step.bodies.iter().any(|body| body.len() > 32_768)));
        assert!(steps.iter().any(|step| step.rollback_first));
        assert!(steps.iter().any(|step| step.checkpoint_after));
    }
}

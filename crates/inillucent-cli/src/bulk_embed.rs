//! `inillucent embed`: fill a vector column for every row of a table, on the processor or a graphics card.
//!
//! Invariant: **a row is either embedded by the same model and the same text that
//! `embed(prefix || text)` would use, or it is reported.** The command reads the rows whose
//! vector column is `NULL`, embeds each text with the prefix put in front, and writes the vector
//! in the layout `embed()` returns. A row whose text is `NULL` or empty is skipped and counted. A
//! row cut at the model's token limit is embedded from the tokens that fit and is counted and
//! named, so nothing is silently shortened. The device is the one that was asked for: a card that
//! will not start is an error that names the installer command and never a run on the processor.
//!
//! ## Why it is a command and not `UPDATE t SET v = embed(text)`
//!
//! `embed()` embeds one row per call on the processor, which for the 558,429 chunks of the study
//! would take about 12 hours. This command sorts a slice of rows by length, groups them under a
//! memory ceiling, runs several sessions at once, and commits every `--commit-every` rows, so a
//! stopped run resumes at the first row still `NULL`.
//!
//! The model is opened here from `inillucent_core` because the SQL engine sits below the
//! retrieval engine and cannot reach it; this crate already links both.

use crate::command::{Arguments, Context, Failed, Outcome};

/// The rows written in one transaction when `--commit-every` is not given.
pub const DEFAULT_COMMIT_EVERY: i64 = 1024;

/// How many rowids of truncated rows the report names.
pub const MAX_NAMED_TRUNCATED: usize = 20;

/// `embed`: embeds a text column into a vector column.
///
/// @param context - the open database
/// @param arguments - what was asked for
#[cfg(not(feature = "embed"))]
pub fn embed_table(_context: &mut Context, _arguments: &Arguments) -> Result<Outcome, Failed> {
    Err(Failed::unsupported(
        "embed",
        "embed: this build has no embedding support compiled in",
    ))
}

/// `embed`: embeds a text column into a vector column.
///
/// @param context - the open database
/// @param arguments - what was asked for
#[cfg(feature = "embed")]
pub fn embed_table(context: &mut Context, arguments: &Arguments) -> Result<Outcome, Failed> {
    real::embed_table(context, arguments)
}

#[cfg(feature = "embed")]
mod real {
    use std::io::IsTerminal;
    use std::time::Instant;

    use inillucent_core::embed_onnx::{Device, Embedded, OnnxEmbedder, OnnxOptions};
    use inillucent_core::install;
    use inillucent_core::model::ModelManifest;
    use inillucent_driver::Status;
    use inillucent_engine::connect::Connection;
    use inillucent_engine::OwnedDatum;

    use super::{DEFAULT_COMMIT_EVERY, MAX_NAMED_TRUNCATED};
    use crate::command::{Arguments, Context, Failed, Outcome};
    use crate::json::{self, Json};

    /// What one run was asked to do.
    struct Plan {
        table: String,
        text_column: String,
        vector_column: String,
        prefix: String,
        device: Device,
        threads: Option<usize>,
        sessions: usize,
        commit_every: usize,
        batch_size: usize,
        all: bool,
    }

    /// What one run did.
    #[derive(Default)]
    struct Report {
        embedded: usize,
        skipped: usize,
        truncated: usize,
        truncated_rowids: Vec<i64>,
    }

    /// One row read from the table.
    struct Row {
        rowid: i64,
        text: Option<String>,
    }

    /// Runs the command.
    ///
    /// @param context - the open database
    /// @param arguments - what was asked for
    pub fn embed_table(context: &mut Context, arguments: &Arguments) -> Result<Outcome, Failed> {
        let plan = read_plan(arguments)?;
        let connection = context.shell().connection();
        // The table and the columns are checked before a model is opened, so a misspelled name
        // is refused at once and does not cost the load of the model.
        check_target(&connection, &plan)?;
        let total = count_rows(&connection, &plan)?;
        let started = Instant::now();
        let embedders = open_embedders(&plan)?;
        let loaded = started.elapsed();
        let max_tokens = embedders
            .first()
            .map(|embedder| embedder.options().max_tokens)
            .unwrap_or(0);
        let began = Instant::now();
        let report = embed_all(&connection, &plan, &embedders, max_tokens, total);
        park(&session_key(&plan), embedders);
        Ok(outcome(&plan, &report?, began.elapsed(), loaded))
    }

    /// Reads and checks the arguments, refusing a value that cannot be used before any model is opened.
    ///
    /// The device and the thread count default to the machine's settings: the environment
    /// variable, then what `setup-embeddings` recorded, then the processor.
    ///
    /// @param arguments - what was asked for
    fn read_plan(arguments: &Arguments) -> Result<Plan, Failed> {
        let device_text = match arguments.text("device") {
            Some(text) => install::parse_device(text).map_err(Failed::misuse)?,
            None => install::configured_device().value,
        };
        let device =
            Device::parse(&device_text).map_err(|reason| Failed::misuse(format!("{reason:#}")))?;
        let threads = match arguments.integer("threads") {
            Some(count) => {
                Some(install::parse_threads(&count.to_string()).map_err(Failed::misuse)?)
            }
            None => install::configured_threads().value,
        };
        Ok(Plan {
            table: arguments.required_text("table")?.to_string(),
            text_column: arguments.required_text("text")?.to_string(),
            vector_column: arguments.required_text("vector")?.to_string(),
            prefix: arguments.text("prefix").unwrap_or("").to_string(),
            device,
            threads,
            sessions: positive(arguments, "sessions", 1)?,
            commit_every: positive(arguments, "commit-every", DEFAULT_COMMIT_EVERY)?,
            batch_size: positive(arguments, "batch-size", 16)?,
            all: arguments.flag("all"),
        })
    }

    /// Reads a parameter that must be a whole number of at least 1.
    ///
    /// @param arguments - what was asked for
    /// @param name - the parameter
    /// @param default - the value when it is not given
    fn positive(arguments: &Arguments, name: &str, default: i64) -> Result<usize, Failed> {
        let value = arguments.integer(name).unwrap_or(default);
        match usize::try_from(value) {
            Ok(count) if count >= 1 => Ok(count),
            _ => Err(Failed::misuse(format!(
                "embed: {name} must be a whole number of at least 1, not {value}"
            ))),
        }
    }

    /// The sessions an earlier call left behind, so a long lived process does not open them again.
    ///
    /// **A finished run parks its sessions here and never drops them.** Dropping an ONNX Runtime
    /// session and then exiting the process ended it with `STATUS_STACK_BUFFER_OVERRUN` in about one
    /// run in six on this machine, after the command had printed its result and committed every row,
    /// so a script saw a failure for a run that had worked. A session that is never dropped does not
    /// do it: 0 failures in 30 runs, against 5 in 30. `embed()` has the same property, because its
    /// embedder is a static that is never dropped. Parked sessions are reused by the next call with
    /// the same settings, so the MCP server, which calls this command many times in one process,
    /// holds one set of sessions and not one per call.
    static PARKED: std::sync::Mutex<Vec<(String, OnnxEmbedder)>> =
        std::sync::Mutex::new(Vec::new());

    /// Returns the key two runs share sessions under: the device, the thread count and the batch size.
    ///
    /// @param plan - what was asked for
    fn session_key(plan: &Plan) -> String {
        format!(
            "{}|{:?}|{}",
            plan.device.label(),
            plan.threads,
            plan.batch_size
        )
    }

    /// Takes every parked session that was opened under a key.
    ///
    /// @param key - the settings the sessions were opened with
    fn take_parked(key: &str) -> Vec<OnnxEmbedder> {
        let Ok(mut parked) = PARKED.lock() else {
            return Vec::new();
        };
        let (mine, others): (Vec<_>, Vec<_>) = parked.drain(..).partition(|(held, _)| held == key);
        *parked = others;
        mine.into_iter().map(|(_, embedder)| embedder).collect()
    }

    /// Parks sessions for the next call, and forgets them when the lock is not available.
    ///
    /// @param key - the settings the sessions were opened with
    /// @param embedders - the sessions to keep
    fn park(key: &str, embedders: Vec<OnnxEmbedder>) {
        match PARKED.lock() {
            Ok(mut parked) => parked.extend(
                embedders
                    .into_iter()
                    .map(|embedder| (key.to_string(), embedder)),
            ),
            Err(_) => embedders.into_iter().for_each(std::mem::forget),
        }
    }

    /// Opens one session per `--sessions` on the device, from the installed model.
    ///
    /// A device that will not start fails here, with the message that names
    /// `inillucent setup-embeddings runtime --gpu`. There is no fallback to the processor.
    ///
    /// @param plan - what was asked for
    fn open_embedders(plan: &Plan) -> Result<Vec<OnnxEmbedder>, Failed> {
        let Some(dir) = install::model_dir(install::DEFAULT_MODEL) else {
            return Err(Failed::misuse(format!(
                "embed: no embedding model is installed. Run `inillucent setup-embeddings all` to \
                 download {} and the ONNX Runtime it needs",
                install::DEFAULT_MODEL
            )));
        };
        let manifest = ModelManifest::read(&dir).unwrap_or_else(|_| ModelManifest::nomic_v1_5());
        let mut options = OnnxOptions::for_model_on(&manifest, plan.batch_size, plan.device);
        options.intra_threads = plan.threads;
        let key = session_key(plan);
        let mut opened = take_parked(&key);
        if opened.len() > plan.sessions {
            park(&key, opened.split_off(plan.sessions));
        }
        while opened.len() < plan.sessions {
            let embedder = OnnxEmbedder::open_model(&dir, &manifest.model_file, options.clone())
                .map_err(|reason| Failed::misuse(format!("embed: {reason:#}")))?;
            opened.push(embedder);
        }
        Ok(opened)
    }

    /// Quotes a table or column name for SQL, doubling any quote inside it.
    ///
    /// @param name - the name as the caller typed it
    fn quoted(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }

    /// Checks that the table and both columns exist, before any row is read.
    ///
    /// @param connection - the open database
    /// @param plan - what was asked for
    fn check_target(connection: &Connection<'_>, plan: &Plan) -> Result<(), Failed> {
        let sql = format!(
            "SELECT rowid, {}, {} FROM {} LIMIT 0",
            quoted(&plan.text_column),
            quoted(&plan.vector_column),
            quoted(&plan.table)
        );
        connection
            .query(&sql)
            .map(|_| ())
            .map_err(|error| Failed::from_engine(&error))
    }

    /// The condition that picks the rows to embed.
    ///
    /// Every row with `--all`, and otherwise the rows whose vector column is `NULL`, which is
    /// what makes a stopped run resume where it ended.
    ///
    /// @param plan - what was asked for
    fn wanted(plan: &Plan) -> String {
        match plan.all {
            true => "1".to_string(),
            false => format!("{} IS NULL", quoted(&plan.vector_column)),
        }
    }

    /// Counts the rows the run will visit, for the progress line.
    ///
    /// @param connection - the open database
    /// @param plan - what was asked for
    fn count_rows(connection: &Connection<'_>, plan: &Plan) -> Result<usize, Failed> {
        let sql = format!(
            "SELECT count(*) FROM {} WHERE {}",
            quoted(&plan.table),
            wanted(plan)
        );
        let rows = connection
            .query(&sql)
            .map_err(|error| Failed::from_engine(&error))?;
        let counted = rows
            .first()
            .and_then(|row| row.first())
            .and_then(|value| match value {
                OwnedDatum::Int(count) => Some(*count),
                _ => None,
            });
        Ok(counted
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or(0))
    }

    /// Embeds every wanted row, one transaction at a time.
    ///
    /// @param connection - the open database
    /// @param plan - what was asked for
    /// @param embedders - the open sessions
    /// @param max_tokens - the model's token limit
    /// @param total - how many rows the run will visit
    fn embed_all(
        connection: &Connection<'_>,
        plan: &Plan,
        embedders: &[OnnxEmbedder],
        max_tokens: usize,
        total: usize,
    ) -> Result<Report, Failed> {
        let mut report = Report::default();
        let mut after = i64::MIN;
        let started = Instant::now();
        loop {
            let slice = read_slice(connection, plan, after)?;
            let Some(last) = slice.last() else {
                break;
            };
            after = last.rowid;
            let vectors = embed_slice(embedders, plan, &slice)?;
            write_slice(connection, plan, &slice, &vectors, max_tokens, &mut report)?;
            progress(&report, total, started.elapsed());
        }
        Ok(report)
    }

    /// Reads the next `--commit-every` wanted rows after a rowid, in rowid order.
    ///
    /// @param connection - the open database
    /// @param plan - what was asked for
    /// @param after - the last rowid already visited
    fn read_slice(
        connection: &Connection<'_>,
        plan: &Plan,
        after: i64,
    ) -> Result<Vec<Row>, Failed> {
        let sql = format!(
            "SELECT rowid, {} FROM {} WHERE {} AND rowid > ?1 ORDER BY rowid LIMIT {}",
            quoted(&plan.text_column),
            quoted(&plan.table),
            wanted(plan),
            plan.commit_every
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| Failed::from_engine(&error))?;
        statement
            .bind_integer(1, after)
            .map_err(|error| Failed::from_engine(&error))?;
        let mut rows = Vec::new();
        while statement
            .step()
            .map_err(|error| Failed::from_engine(&error))?
        {
            let row = statement.row();
            let Some(OwnedDatum::Int(rowid)) = row.first() else {
                continue;
            };
            rows.push(Row {
                rowid: *rowid,
                text: row.get(1).and_then(text_of),
            });
        }
        Ok(rows)
    }

    /// Returns a cell as the text to embed, or `None` for NULL and for an empty value.
    ///
    /// @param value - the text column's value
    fn text_of(value: &OwnedDatum) -> Option<String> {
        match value {
            OwnedDatum::Text(bytes) | OwnedDatum::Blob(bytes) if !bytes.is_empty() => {
                Some(String::from_utf8_lossy(bytes).into_owned())
            }
            OwnedDatum::Int(number) => Some(number.to_string()),
            OwnedDatum::Real(number) => Some(number.to_string()),
            _ => None,
        }
    }

    /// Embeds the rows of a slice that have text, spreading them over the sessions.
    ///
    /// The rows are sorted by length and dealt out one at a time, so every session gets a mix of
    /// short and long texts and none waits for another. Each session sorts and groups its own
    /// share by token count under the memory ceiling.
    ///
    /// @param embedders - the open sessions
    /// @param plan - what was asked for
    /// @param slice - the rows read
    /// @returns one entry per row of the slice: its vector and token count, or `None` for a row with no text
    fn embed_slice(
        embedders: &[OnnxEmbedder],
        plan: &Plan,
        slice: &[Row],
    ) -> Result<Vec<Option<(Vec<f32>, usize)>>, Failed> {
        let mut order: Vec<usize> = (0..slice.len())
            .filter(|at| slice.get(*at).is_some_and(|row| row.text.is_some()))
            .collect();
        order.sort_by_key(|at| {
            slice
                .get(*at)
                .and_then(|row| row.text.as_ref())
                .map_or(0, String::len)
        });
        let shares = deal(&order, embedders.len());
        let results = run_shares(embedders, plan, slice, &shares)?;
        let mut vectors: Vec<Option<(Vec<f32>, usize)>> = vec![None; slice.len()];
        for (share, embedded) in shares.iter().zip(results) {
            for ((at, vector), tokens) in share.iter().zip(embedded.vectors).zip(embedded.tokens) {
                if let Some(slot) = vectors.get_mut(*at) {
                    *slot = Some((vector, tokens));
                }
            }
        }
        Ok(vectors)
    }

    /// Deals a sorted list of row positions out to a number of sessions, one at a time.
    ///
    /// @param order - the positions, sorted by text length
    /// @param sessions - how many sessions share them
    fn deal(order: &[usize], sessions: usize) -> Vec<Vec<usize>> {
        let mut shares: Vec<Vec<usize>> = vec![Vec::new(); sessions.max(1)];
        for (turn, at) in order.iter().enumerate() {
            if let Some(share) = shares.get_mut(turn % sessions.max(1)) {
                share.push(*at);
            }
        }
        shares
    }

    /// Embeds one session's share of a slice.
    ///
    /// @param embedder - the session
    /// @param plan - what was asked for
    /// @param slice - the rows read
    /// @param share - the row positions this session embeds
    fn embed_share(
        embedder: &OnnxEmbedder,
        plan: &Plan,
        slice: &[Row],
        share: &[usize],
    ) -> Result<Embedded, Failed> {
        let texts: Vec<String> = share
            .iter()
            .filter_map(|at| slice.get(*at).and_then(|row| row.text.as_ref()))
            .map(|text| format!("{}{text}", plan.prefix))
            .collect();
        embedder
            .embed_prefixed_counted(&texts)
            .map_err(|reason| Failed::said(Status::InvalidState, format!("embed: {reason:#}")))
    }

    /// Runs each session's share on its own thread and returns the results in share order.
    ///
    /// @param embedders - the open sessions
    /// @param plan - what was asked for
    /// @param slice - the rows read
    /// @param shares - the row positions each session embeds
    fn run_shares(
        embedders: &[OnnxEmbedder],
        plan: &Plan,
        slice: &[Row],
        shares: &[Vec<usize>],
    ) -> Result<Vec<Embedded>, Failed> {
        if let ([embedder], [share]) = (embedders, shares) {
            return Ok(vec![embed_share(embedder, plan, slice, share)?]);
        }
        let outcomes: Vec<Result<Embedded, String>> = std::thread::scope(|scope| {
            let handles: Vec<_> = embedders
                .iter()
                .zip(shares)
                .map(|(embedder, share)| {
                    let texts: Vec<String> = share
                        .iter()
                        .filter_map(|at| slice.get(*at).and_then(|row| row.text.as_ref()))
                        .map(|text| format!("{}{text}", plan.prefix))
                        .collect();
                    scope.spawn(move || {
                        embedder
                            .embed_prefixed_counted(&texts)
                            .map_err(|reason| format!("{reason:#}"))
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err("an embedding thread stopped unexpectedly".to_string())
                    })
                })
                .collect()
        });
        outcomes
            .into_iter()
            .map(|outcome| {
                outcome.map_err(|reason| {
                    Failed::said(Status::InvalidState, format!("embed: {reason}"))
                })
            })
            .collect()
    }

    /// Writes one slice's vectors in one transaction, and adds what happened to the report.
    ///
    /// A crash loses at most this transaction, and the rows in it are still `NULL`, so a rerun
    /// embeds them again.
    ///
    /// @param connection - the open database
    /// @param plan - what was asked for
    /// @param slice - the rows read
    /// @param vectors - one entry per row of the slice
    /// @param max_tokens - the model's token limit
    /// @param report - the totals so far
    fn write_slice(
        connection: &Connection<'_>,
        plan: &Plan,
        slice: &[Row],
        vectors: &[Option<(Vec<f32>, usize)>],
        max_tokens: usize,
        report: &mut Report,
    ) -> Result<(), Failed> {
        let transaction = connection
            .begin()
            .map_err(|error| Failed::from_engine(&error))?;
        let update = format!(
            "UPDATE {} SET {} = ?1 WHERE rowid = ?2",
            quoted(&plan.table),
            quoted(&plan.vector_column)
        );
        let mut statement = transaction
            .prepare(&update)
            .map_err(|error| Failed::from_engine(&error))?;
        for (row, vector) in slice.iter().zip(vectors) {
            let Some((values, tokens)) = vector else {
                report.skipped = report.skipped.saturating_add(1);
                continue;
            };
            statement
                .bind_blob(1, &as_bytes(values))
                .and_then(|_| statement.bind_integer(2, row.rowid))
                .and_then(|_| statement.step())
                .map_err(|error| Failed::from_engine(&error))?;
            statement.reset();
            report.embedded = report.embedded.saturating_add(1);
            if *tokens > max_tokens {
                report.truncated = report.truncated.saturating_add(1);
                if report.truncated_rowids.len() < MAX_NAMED_TRUNCATED {
                    report.truncated_rowids.push(row.rowid);
                }
            }
        }
        drop(statement);
        transaction
            .commit()
            .map_err(|error| Failed::from_engine(&error))
    }

    /// Returns a vector as the bytes a `VECTOR(n)` column holds: little endian 32 bit floats.
    ///
    /// @param values - the vector
    fn as_bytes(values: &[f32]) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(values.len().saturating_mul(4));
        for value in values {
            bytes.extend_from_slice(&value.to_bits().to_le_bytes());
        }
        bytes
    }

    /// Prints one progress line to standard error when a person is watching.
    ///
    /// @param report - the totals so far
    /// @param total - how many rows the run will visit
    /// @param elapsed - the time since the first row
    fn progress(report: &Report, total: usize, elapsed: std::time::Duration) {
        if !std::io::stderr().is_terminal() {
            return;
        }
        let done = report.embedded.saturating_add(report.skipped);
        let rate = report.embedded as f64 / elapsed.as_secs_f64().max(0.001);
        eprintln!("embed: {done} of {total} rows, {rate:.0} rows a second");
    }

    /// Builds what the command prints and returns as JSON.
    ///
    /// @param plan - what was asked for
    /// @param report - what the run did
    /// @param elapsed - the time from the first row to the last
    /// @param loaded - the time it took to open the sessions
    fn outcome(
        plan: &Plan,
        report: &Report,
        elapsed: std::time::Duration,
        loaded: std::time::Duration,
    ) -> Outcome {
        let seconds = elapsed.as_secs_f64();
        let rate = report.embedded as f64 / seconds.max(0.001);
        let mut text = format!(
            "embedded {} rows in {seconds:.1} s, {rate:.0} rows a second, on {} with {} session{}. \
             {} skipped because the text was NULL or empty. {} cut at the model's token limit",
            report.embedded,
            plan.device.label(),
            plan.sessions,
            if plan.sessions == 1 { "" } else { "s" },
            report.skipped,
            report.truncated
        );
        if !report.truncated_rowids.is_empty() {
            let named: Vec<String> = report.truncated_rowids.iter().map(i64::to_string).collect();
            text.push_str(&format!(". First cut rowids: {}", named.join(", ")));
        }
        let mut said = Outcome::said("embed", text);
        said.changes = report.embedded as i64;
        said.with("embedded", Json::Int(report.embedded as i64))
            .with("skipped", Json::Int(report.skipped as i64))
            .with("truncated", Json::Int(report.truncated as i64))
            .with(
                "truncated_rowids",
                Json::Array(
                    report
                        .truncated_rowids
                        .iter()
                        .map(|id| Json::Int(*id))
                        .collect(),
                ),
            )
            .with("seconds", Json::Real(seconds))
            .with("rows_per_second", Json::Real(rate))
            .with("load_seconds", Json::Real(loaded.as_secs_f64()))
            .with("device", json::text(plan.device.label()))
            .with("sessions", Json::Int(plan.sessions as i64))
    }
}

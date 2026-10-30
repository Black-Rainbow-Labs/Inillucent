//! The pragmas that check something, and the checkpoint.
//!
//! Invariant: **a check reads and never repairs.** `integrity_check` is a
//! question about the file, and one that quietly fixed what it found would
//! answer `ok` about a file it had just changed.

use inillucent_base::{DbError, DbResult, PrimaryCode};
use inillucent_sql::declare::argument_text;
use inillucent_sql::directive::PragmaArgument;
use inillucent_tree::datum::OwnedDatum;

use crate::engine::integrity::CheckDepth;
use crate::Outcome;

/// The most lines `integrity_check` reports when it is not given a number.
const DEFAULT_ERROR_LIMIT: usize = 100;

/// Reads the argument of `integrity_check` or `quick_check`.
///
/// A number is how many problems to stop at, where zero or less means the default, and anything
/// else names the one table to check.
///
/// @param argument - the pragma's argument, when it has one
fn check_arguments(argument: Option<&PragmaArgument>) -> (usize, Option<Vec<u8>>) {
    let Some(argument) = argument else {
        return (DEFAULT_ERROR_LIMIT, None);
    };
    let text = argument_text(argument);
    match text.trim().parse::<i32>() {
        Ok(number) if number > 0 => (usize::try_from(number).unwrap_or(DEFAULT_ERROR_LIMIT), None),
        Ok(_) => (DEFAULT_ERROR_LIMIT, None),
        Err(_) => (
            DEFAULT_ERROR_LIMIT,
            Some(text.to_ascii_lowercase().into_bytes()),
        ),
    }
}

/// Builds the query that counts one for a table that has a row and zero for an empty one.
///
/// @param database - the connection, which names the schema
/// @param table - the table
/// @param at - the database the table is in
fn row_exists_query(
    database: &crate::ImportedDatabase,
    table: &inillucent_sql::catalog_view::TableInfo,
    at: usize,
) -> String {
    let quote = |text: &[u8]| String::from_utf8_lossy(text).replace('"', "\"\"");
    let schema =
        inillucent_sql::catalog_view::CatalogView::database_name(&database.schema.catalog, at);
    format!(
        "SELECT count(*) FROM (SELECT 1 FROM \"{}\".\"{}\" LIMIT 1)",
        quote(schema),
        quote(&table.name)
    )
}

/// Builds the query that counts the rows of a table that break one of its `CHECK` constraints.
///
/// @param database - the connection, which names the schema
/// @param table - the table
/// @param at - the database the table is in
fn check_violation_query(
    database: &crate::ImportedDatabase,
    table: &inillucent_sql::catalog_view::TableInfo,
    at: usize,
) -> String {
    let quote = |text: &[u8]| String::from_utf8_lossy(text).replace('"', "\"\"");
    let schema =
        inillucent_sql::catalog_view::CatalogView::database_name(&database.schema.catalog, at);
    let broken: Vec<String> = table
        .checks
        .iter()
        .map(|check| format!("NOT ({})", String::from_utf8_lossy(&check.expr_sql)))
        .collect();
    format!(
        "SELECT count(*) FROM \"{}\".\"{}\" WHERE {}",
        quote(schema),
        quote(&table.name),
        broken.join(" OR ")
    )
}

impl crate::ImportedDatabase {
    /// Reports every row whose foreign key has no parent.
    ///
    /// **It is a query, not a scan written by hand**, so it uses the planner
    /// and the indexes an ordinary query would: a check over a million-row
    /// child with an index on its key is a lookup per row rather than a second
    /// scan. `inillucent-sql`'s `violation_query` builds it, which is the same
    /// text a deferred constraint is tested with at commit - so the pragma and
    /// the commit cannot disagree about what a violation is.
    ///
    /// **It reads under `&self`, so `pragma_foreign_key_check` can call it.** The
    /// queries it runs only read, and the table valued form has no mutable
    /// connection to hand them.
    ///
    /// **A table that has no foreign keys, or no such table at all, differ.**
    /// SQLite answers no rows for the first and fails with `no such table` for
    /// the second, whether the pragma is written as a statement or as a
    /// function.
    ///
    /// **Rows come out in the order SQLite gives them**: the tables newest
    /// first, and within a table by rowid, with a row that breaks two keys
    /// reported once for each, the key SQLite numbers 0 first.
    ///
    /// @param argument - one table to check, or none for every table
    /// @param at - the attached database the pragma was qualified with
    pub(crate) fn pragma_foreign_key_check(
        &self,
        argument: Option<&PragmaArgument>,
        at: Option<usize>,
    ) -> DbResult<Outcome> {
        let only = argument.map(|argument| argument_text(argument).to_ascii_lowercase());
        let database = self.database_to_check(only.as_deref(), at)?;
        if let Some(message) = self.schema.foreign_key_mismatch(only.as_deref(), database) {
            return Err(inillucent_base::error::refusal(message));
        }
        let queries =
            self.schema
                .foreign_key_checks(only.as_deref(), database, &self.schema_label(database));
        let mut rows = Vec::new();
        let mut children: Vec<&[u8]> = Vec::new();
        for query in queries.iter().rev() {
            if !children.contains(&query.child.as_slice()) {
                children.push(query.child.as_slice());
            }
        }
        for child in children {
            let own: Vec<&crate::engine::keys::ViolationQuery> = queries
                .iter()
                .filter(|query| query.child.as_slice() == child)
                .collect();
            rows.extend(self.violations_of_table(&own)?);
        }
        Ok(Outcome {
            rows,
            names: std::rc::Rc::new(vec![
                "table".into(),
                "rowid".into(),
                "parent".into(),
                "fkid".into(),
            ]),
            changes: Default::default(),
        })
    }
    /// Refuses a check pragma whose table does not exist or whose key has no parent key, when the statement is prepared.
    ///
    /// SQLite looks the table up while compiling `PRAGMA foreign_key_check(missing)` and
    /// `PRAGMA integrity_check(missing)`, and finds each foreign key's parent index there too, so
    /// the shell reports `Parse error` and a prepared statement fails before any step. Checking
    /// again at run time is still done by [`Self::pragma_foreign_key_check`].
    ///
    /// @param directive - the bound statement; only the three check pragmas are looked at
    pub(crate) fn check_pragma_at_prepare(
        &self,
        directive: &inillucent_sql::directive::Directive,
    ) -> DbResult<()> {
        let inillucent_sql::directive::Directive::Pragma {
            database,
            name,
            argument,
        } = directive
        else {
            return Ok(());
        };
        match name.as_slice() {
            b"foreign_key_check" => {
                let only = argument
                    .as_ref()
                    .map(|argument| argument_text(argument).to_ascii_lowercase());
                let checked = self.database_to_check(only.as_deref(), *database)?;
                match self.schema.foreign_key_mismatch(only.as_deref(), checked) {
                    Some(message) => Err(inillucent_base::error::refusal(message)),
                    None => Ok(()),
                }
            }
            b"integrity_check" | b"quick_check" => {
                self.check_named_table(argument.as_ref(), *database)
            }
            _ => Ok(()),
        }
    }

    /// Returns the database a `foreign_key_check` looks in.
    ///
    /// **A name with no qualifier is looked for in every database, in the order
    /// SQLite resolves one: `main`, `temp`, then the attachments.** With no
    /// name and no qualifier the check is of `main`. A name that is not a table
    /// of the database it is looked for in is SQLite's `no such table`, whether
    /// the pragma is written as a statement or as a function.
    ///
    /// @param folded - the folded name of the table to check, when one was given
    /// @param at - the database the pragma was qualified with, when it was
    fn database_to_check(&self, folded: Option<&str>, at: Option<usize>) -> DbResult<usize> {
        let Some(name) = folded else {
            return Ok(at.unwrap_or(crate::MAIN));
        };
        let holds = |database: usize| {
            self.schema
                .tables
                .iter()
                .any(|table| table.folded == name.as_bytes() && table.database == database)
        };
        let found = match at {
            Some(database) => Some(database).filter(|held| holds(*held)),
            None => self.schema_numbers().into_iter().find(|held| holds(*held)),
        };
        // A qualified pragma names the database in the message, as `temp.t`.
        found.ok_or_else(|| {
            let schema = at.map(|database| self.schema_label(database));
            let shown = match schema {
                Some(label) => format!("{}.{name}", String::from_utf8_lossy(&label)),
                None => name.to_string(),
            };
            inillucent_base::error::refusal(format!("no such table: {shown}"))
        })
    }

    /// Returns the rows `PRAGMA foreign_key_check` reports for one table.
    ///
    /// SQLite scans the table once and tests each row against every key, so a
    /// row is reported in rowid order and, when it breaks two keys, once for each
    /// key from the one numbered 0. Running each key as its own query and sorting
    /// the answers by rowid and key reproduces that. A table without a rowid
    /// reports NULL for it, so its rows stay in the order the queries gave.
    ///
    /// @param own - the queries of the table's keys
    fn violations_of_table(
        &self,
        own: &[&crate::engine::keys::ViolationQuery],
    ) -> DbResult<Vec<Vec<OwnedDatum>>> {
        let mut found: Vec<(OwnedDatum, &crate::engine::keys::ViolationQuery)> = Vec::new();
        for query in own {
            let (answer, _) = self.run(&query.sql)?;
            for row in answer {
                found.push((row.first().cloned().unwrap_or(OwnedDatum::Null), query));
            }
        }
        found.sort_by(|(left, near), (right, far)| match (left, right) {
            (OwnedDatum::Int(left), OwnedDatum::Int(right)) => {
                left.cmp(right).then(near.key.cmp(&far.key))
            }
            _ => near.key.cmp(&far.key),
        });
        Ok(found
            .into_iter()
            .map(|(rowid, query)| {
                vec![
                    OwnedDatum::Text(query.child.clone()),
                    rowid,
                    OwnedDatum::Text(query.parent.clone()),
                    OwnedDatum::Int(i64::from(query.key)),
                ]
            })
            .collect())
    }
    /// Runs the integrity checker over every tree.
    ///
    /// `ok` when they all hold, and the first failure otherwise, which is the
    /// shape SQLite's answer has.
    ///
    /// **Two names, and now two amounts of reading.** They used to be the same
    /// pass, because this engine had no cheaper variant to offer. `quick_check`
    /// reads every tree's own shape and accounts for every page of the file;
    /// `integrity_check` does that and then reads each index against the table
    /// it is on.
    ///
    /// **The line is drawn at the index pass because that is the expensive
    /// one**, which was measured rather than assumed: the index pass walks each
    /// index and its table and merges them, where the page walk reads a tree's
    /// interior pages and its leaves and takes an out-of-line value's pages
    /// from the reference in the leaf it is already holding. Over a table of
    /// sixty out-of-line values the whole page walk cost 4 page fetches on top
    /// of 133.
    ///
    /// The pinned SQLite 3.53.4 draws it in the same place: its `quick_check`
    /// omits index content against table content, `UNIQUE`, `CHECK` and
    /// `NOT NULL`, and still accounts for every page of the file
    /// (`sqlite3BtreeIntegrityCheck`, which both pragmas reach).
    ///
    /// @param column - which of the two names asked for this, and so which one
    ///   the answer is reported under
    /// @param depth - how much of the file that name reads
    /// @param named - the pragma's argument and the database it was qualified with
    pub(crate) fn pragma_integrity_check(
        &self,
        column: &str,
        depth: CheckDepth,
        named: (Option<&PragmaArgument>, Option<usize>),
    ) -> DbResult<Outcome> {
        self.check_named_table(named.0, named.1)?;
        let (limit, table) = check_arguments(named.0);
        let mut found: Vec<String> = Vec::new();
        if let Err(error) = self.check_trees_to(depth) {
            found.push(error.detail().unwrap_or(error.message()).to_string());
        }
        found.extend(self.check_constraint_failures(table.as_deref(), named.1, limit)?);
        found.truncate(limit);
        if found.is_empty() {
            found.push("ok".to_string());
        }
        Ok(Outcome {
            rows: found
                .into_iter()
                .map(|line| vec![OwnedDatum::Text(line.into_bytes())])
                .collect(),
            names: std::rc::Rc::new(vec![column.into()]),
            changes: Default::default(),
        })
    }

    /// Returns one line for every row that breaks a `CHECK` constraint of its table.
    ///
    /// **Both `integrity_check` and `quick_check` read the rows against the constraints**, as
    /// SQLite does, and a row that fails more than one constraint is reported once. A `NULL`
    /// result passes, as it does on a write. Nothing is reported while
    /// `PRAGMA ignore_check_constraints` is on, which is when SQLite skips the check too.
    /// Tables are visited newest first within each database, the order SQLite walks its table
    /// hash in, and the databases in the order `main`, `temp`, then the attached ones.
    ///
    /// @param only - one table's folded name, when the pragma named a table
    /// @param at - the database the pragma was qualified with, when it was
    /// @param limit - how many lines to stop at
    fn check_constraint_failures(
        &self,
        only: Option<&[u8]>,
        at: Option<usize>,
        limit: usize,
    ) -> DbResult<Vec<String>> {
        let mut lines = Vec::new();
        if self.pragmas.ignore_check_constraints() {
            return Ok(lines);
        }
        let databases = match at {
            Some(database) => vec![database],
            None => self.schema_numbers(),
        };
        for database in databases {
            let tables = self
                .schema
                .tables
                .iter()
                .filter(|table| table.database == database && !table.checks.is_empty());
            let newest_first: Vec<_> = tables.collect::<Vec<_>>().into_iter().rev().collect();
            for table in newest_first {
                if only.is_some_and(|name| name != table.folded.as_slice()) {
                    continue;
                }
                // **A table with no rows evaluates no `CHECK`.** SQLite reads each row against the
                // constraints, so `CHECK(ABS(-9223372036854775808))` on an empty table reports `ok`.
                // The count query below folds that constant before it looks for a row, and the
                // overflow it raises would fail the pragma.
                if self.count_rows(&row_exists_query(self, table, database))? == 0 {
                    continue;
                }
                let broken = self.count_rows(&check_violation_query(self, table, database))?;
                for _ in 0..broken.min(i64::try_from(limit).unwrap_or(i64::MAX)) {
                    lines.push(format!(
                        "CHECK constraint failed in {}",
                        String::from_utf8_lossy(&table.name)
                    ));
                }
                if lines.len() >= limit {
                    return Ok(lines);
                }
            }
        }
        Ok(lines)
    }
    /// Refuses an `integrity_check` or `quick_check` argument that names a table that does not exist.
    ///
    /// **The argument is a table name unless it reads as an integer.** SQLite
    /// reads `integrity_check(10)` as the most errors to report and anything
    /// else as the one table to check, which it looks up while compiling, so
    /// `integrity_check(nosuch)` fails with `no such table: nosuch` and used to
    /// answer `ok` here.
    ///
    /// @param argument - the pragma's argument, when it has one
    /// @param at - the database the pragma was qualified with, when it was
    fn check_named_table(
        &self,
        argument: Option<&PragmaArgument>,
        at: Option<usize>,
    ) -> DbResult<()> {
        let Some(argument) = argument else {
            return Ok(());
        };
        let text = argument_text(argument);
        if text.trim().parse::<i32>().is_ok() {
            return Ok(());
        }
        self.database_to_check(Some(&text.to_ascii_lowercase()), at)
            .map(|_| ())
    }

    /// Checkpoints the log into the data file.
    ///
    /// SQLite answers three integers: whether it was blocked, how many frames
    /// the log held, and how many of them were moved. The engine's checkpoint is
    /// not blockable from here - there is one writer - so the first is always
    /// zero, and the second and third are always equal because a checkpoint here
    /// always moves everything.
    ///
    /// The number reported is **pages the checkpoint wrote**, counted off the
    /// pool rather than off the log. SQLite's log holds one frame per dirty page
    /// and this one holds a record per change, so a record count would be a
    /// bigger number meaning something else; the pages written is the same
    /// physical quantity SQLite's frame count is.
    ///
    /// **The mode decides what is reported afterwards, as in SQLite.** `PASSIVE`,
    /// `FULL` and `RESTART` report the pages the checkpoint moved. `TRUNCATE`
    /// empties the log, so it reports `0|0|0` whatever it moved: a log with no
    /// frames in it, and none left to move. A word that is not a mode is
    /// `PASSIVE`, as SQLite reads it.
    ///
    /// @param argument - the mode named in the pragma, when one was given
    pub(crate) fn pragma_wal_checkpoint(
        &mut self,
        argument: Option<&PragmaArgument>,
    ) -> DbResult<Outcome> {
        let mode = argument.map(argument_text).unwrap_or_default();
        let truncates = mode.eq_ignore_ascii_case("truncate");
        let passive = mode.is_empty() || mode.eq_ignore_ascii_case("passive");
        // **Refused, not answered `1 | -1 | -1`, once the open transaction has
        // written anything.** The pinned reference checkpoints fine after a
        // bare `BEGIN` - no write lock is held yet - and answers `database
        // table is locked` (`SQLITE_LOCKED`) the moment a statement has
        // written, because this connection is itself the lock a checkpoint
        // needs. A `busy` row here would let a script read it, believe
        // nothing happened, and `COMMIT` over a checkpoint that in fact never
        // ran; recording the checkpoint's start no earlier than the open
        // transaction's own first record - which is what letting it proceed
        // would require - is exactly the no-steal argument `holds_uncommitted`
        // makes, so this is refused rather than made honest.
        if self.writing.batch().is_some() && self.writing.touched() != 0 {
            return Err(
                DbError::primary(PrimaryCode::Locked).with_detail("database table is locked")
            );
        }
        // **Minus one twice when there is no log to check point.** SQLite
        // answers `0|-1|-1` under a rollback journal because the two counts are
        // "frames in the log" and "frames moved", and a database with no
        // write-ahead log has neither - which is a different statement from
        // "no frames moved". A caller polling the second column to decide
        // whether a checkpoint is due needs to be able to tell those apart.
        if self.pragmas.journal_mode() != inillucent_pool::journal::JournalMode::Wal {
            let busy = match self.checkpoint() {
                Ok(()) => 0,
                Err(error) if error.code() == PrimaryCode::Busy => i64::from(!passive),
                Err(error) => return Err(error),
            };
            return Ok(Outcome {
                rows: vec![vec![
                    OwnedDatum::Int(busy),
                    OwnedDatum::Int(-1),
                    OwnedDatum::Int(-1),
                ]],
                names: std::rc::Rc::new(vec!["busy".into(), "log".into(), "checkpointed".into()]),
                changes: Default::default(),
            });
        }
        // **The pages the checkpoint is about to write are the log's frames.**
        // Counting what the checkpoint wrote also counted the meta page it
        // always rewrites, so a checkpoint with nothing to do reported 3.
        let waiting = self.storage.database.pool().dirty_pages() as i64;
        // **Another process holding the file is an answer, not a failure.**
        // SQLite returns the row with nothing checkpointed: `busy` 0 for
        // PASSIVE, which never waits for readers, and 1 for FULL, RESTART and
        // TRUNCATE. This failed the statement with "readers are still present",
        // so a program that checkpoints on a timer stopped at the first moment
        // another process was reading.
        let (busy, log, moved) = match self.checkpoint() {
            Ok(()) => {
                let moved = if truncates { 0 } else { waiting };
                (0, moved, moved)
            }
            Err(error) if error.code() == PrimaryCode::Busy => (i64::from(!passive), waiting, 0),
            Err(error) => return Err(error),
        };
        Ok(Outcome {
            rows: vec![vec![
                OwnedDatum::Int(busy),
                OwnedDatum::Int(log),
                OwnedDatum::Int(moved),
            ]],
            names: std::rc::Rc::new(vec!["busy".into(), "log".into(), "checkpointed".into()]),
            changes: Default::default(),
        })
    }
}

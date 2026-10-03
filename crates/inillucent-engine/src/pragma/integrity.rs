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
    /// Refuses a pragma whose table argument does not exist, when the statement is prepared.
    ///
    /// SQLite looks the table up while compiling `PRAGMA foreign_key_check(missing)`, so the
    /// shell reports `Parse error` and a prepared statement fails before any step. Checking
    /// again at run time is still done by [`Self::pragma_foreign_key_check`].
    ///
    /// @param directive - the bound statement; only a `foreign_key_check` with a name is checked
    pub(crate) fn check_pragma_at_prepare(
        &self,
        directive: &inillucent_sql::directive::Directive,
    ) -> DbResult<()> {
        let inillucent_sql::directive::Directive::Pragma {
            database,
            name,
            argument: Some(argument),
        } = directive
        else {
            return Ok(());
        };
        if name.as_slice() != b"foreign_key_check" {
            return Ok(());
        }
        let only = argument_text(argument).to_ascii_lowercase();
        self.database_to_check(Some(&only), *database).map(|_| ())
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
        found.ok_or_else(|| inillucent_base::error::refusal(format!("no such table: {name}")))
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
    pub(crate) fn pragma_integrity_check(
        &self,
        column: &str,
        depth: CheckDepth,
    ) -> DbResult<Outcome> {
        let answer = match self.check_trees_to(depth) {
            Ok(()) => b"ok".to_vec(),
            Err(error) => error
                .detail()
                .unwrap_or(error.message())
                .as_bytes()
                .to_vec(),
        };
        Ok(Outcome {
            rows: vec![vec![OwnedDatum::Text(answer)]],
            names: std::rc::Rc::new(vec![column.into()]),
            changes: Default::default(),
        })
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
        let truncates = argument
            .map(|argument| argument_text(argument).eq_ignore_ascii_case("truncate"))
            .unwrap_or(false);
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
            self.checkpoint()?;
            return Ok(Outcome {
                rows: vec![vec![
                    OwnedDatum::Int(0),
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
        self.checkpoint()?;
        let moved = if truncates { 0 } else { waiting };
        Ok(Outcome {
            rows: vec![vec![
                OwnedDatum::Int(0),
                OwnedDatum::Int(moved),
                OwnedDatum::Int(moved),
            ]],
            names: std::rc::Rc::new(vec!["busy".into(), "log".into(), "checkpointed".into()]),
            changes: Default::default(),
        })
    }
}

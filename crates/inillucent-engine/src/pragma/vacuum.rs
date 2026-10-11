//! `PRAGMA auto_vacuum` and `PRAGMA incremental_vacuum`.
//!
//! Invariant: **the mode a read reports is the mode the file is in, and a mode
//! asked for on a file that already holds a table waits for `VACUUM`.** The mode
//! decides how a file lays out its roots, so it cannot change under tables that
//! were made without it. A set on such a file is remembered on the connection and
//! `VACUUM`, which rebuilds the file, applies it.
//!
//! The file's mode is in its meta record. Mode 1 (`FULL`) gives pages back at every
//! commit; mode 2 (`INCREMENTAL`) gives back as many as `incremental_vacuum` asks
//! for, at the commit of the transaction that asked. See `crate::shrink`.

use inillucent_base::DbResult;
use inillucent_sql::declare::{argument_integer, argument_text};
use inillucent_sql::directive::PragmaArgument;
use inillucent_tree::datum::OwnedDatum;

use inillucent_pool::meta::FORMAT_VERSION;

use crate::Outcome;

use super::*;

/// Returns the mode a word or a number names, or `None` for anything else.
///
/// Words and numbers, in any case, quoted or not, as SQLite reads them. A word
/// that is not a mode is not an error: the pragma ignores it.
///
/// @param text - what the pragma was given
fn mode_named(text: &str) -> Option<u8> {
    match text.trim().to_ascii_lowercase().as_str() {
        "0" | "none" => Some(0),
        "1" | "full" => Some(1),
        "2" | "incremental" => Some(2),
        _ => None,
    }
}

impl crate::ImportedDatabase {
    /// Reads or sets `auto_vacuum`.
    ///
    /// A read answers the file's mode. A set on a file with no table changes the
    /// file's mode at once. A set that moves a file already in mode 1 or 2 to the
    /// other of the two does the same, because both modes lay the file out alike. Any
    /// other set on a file that holds a table is remembered by the connection as
    /// the mode `VACUUM` will give the rebuilt file, and changes nothing now.
    ///
    /// @param argument - the mode, when one was given
    /// @param at - the attached database the pragma was qualified with
    pub(crate) fn pragma_auto_vacuum(
        &mut self,
        argument: Option<&PragmaArgument>,
        at: Option<usize>,
    ) -> DbResult<Outcome> {
        let Some(argument) = argument else {
            return Ok(named_integer(
                "auto_vacuum",
                i64::from(self.file_of(at)?.auto_vacuum()),
            ));
        };
        if let Some(mode) = mode_named(&argument_text(argument)) {
            self.set_auto_vacuum_mode(at.unwrap_or(MAIN), mode)?;
        }
        Ok(Outcome::empty())
    }

    /// Applies a mode to a file, or remembers it for `VACUUM`.
    ///
    /// @param number - the schema, as the binder numbers them
    /// @param mode - 0 none, 1 full, 2 incremental
    fn set_auto_vacuum_mode(&mut self, number: usize, mode: u8) -> DbResult<()> {
        let held = self.file_of(Some(number))?.auto_vacuum();
        let no_table = self
            .schema
            .tables
            .iter()
            .all(|table| table.database != number || table.folded.starts_with(b"sqlite_"));
        let between_full_and_incremental = matches!((held, mode), (1 | 2, 1 | 2));
        if no_table || between_full_and_incremental {
            return self.apply_auto_vacuum_mode(number, mode, no_table);
        }
        if number == MAIN {
            self.pragmas.set_auto_vacuum(mode);
        }
        Ok(())
    }

    /// Writes a mode into a file's meta record and checkpoints it.
    ///
    /// **A file with no table also gets its roots recorded**: in a mode that gives
    /// pages back, the highest root in the file, so the roots that exist stay at the
    /// start of it; in mode 0, zero.
    ///
    /// @param number - the schema
    /// @param mode - the mode
    /// @param record_roots - whether the file has no table, so its roots can be recorded
    pub(crate) fn apply_auto_vacuum_mode(
        &mut self,
        number: usize,
        mode: u8,
        record_roots: bool,
    ) -> DbResult<()> {
        let largest = match (mode, record_roots) {
            (0, _) => Some(0),
            (_, true) => Some(self.highest_root_of(number)?),
            (_, false) => None,
        };
        // A file in a mode that gives pages back writes extent pages the step can
        // move, so it is raised to format 3 first, and the move is logged for the
        // reason `inillucent_tree::format` gives: it belongs to no transaction.
        let raised = mode != 0 && self.file_of_mut(Some(number))?.raise_format(FORMAT_VERSION);
        if raised {
            let wal = self.log_of(number).ok_or_else(|| {
                inillucent_base::error::refusal("that pragma names a database that is not attached")
            })?;
            wal.append(
                0,
                inillucent_wal::record::Body::Format {
                    version: FORMAT_VERSION,
                },
            )?;
        }
        let file = self.file_of_mut(Some(number))?;
        file.set_auto_vacuum(mode);
        if let Some(largest) = largest {
            file.set_largest_root(largest);
        }
        if number == MAIN {
            self.pragmas.set_auto_vacuum(mode);
        }
        // Into the file as well, and checkpointed for the reason `user_version` is:
        // the meta page is not in the log, and SQLite reads the mode back on the
        // next open.
        self.checkpoint()
    }

    /// Returns the highest page that is the root of a tree in one file.
    ///
    /// @param number - the schema
    fn highest_root_of(&self, number: usize) -> DbResult<u64> {
        let catalog = self.file_of(Some(number))?.catalog_root().0;
        let entries = self
            .entries_of(number)
            .iter()
            .map(|held| held.entry.root.0)
            .max()
            .unwrap_or(0);
        Ok(catalog.max(entries))
    }

    /// Runs `incremental_vacuum`, which gives free pages back from the end of a
    /// file.
    ///
    /// It does nothing unless the file is in mode 2. It answers one row with no
    /// columns for each page it will give back, which is the smaller of the count
    /// asked for and the free pages, and the shell prints each as an empty line.
    /// The count is any integer above zero; no count, zero, a negative number or
    /// a word means every free page. The moves happen at the commit of the
    /// transaction, so a `ROLLBACK` drops the request and the page count is
    /// unchanged, as in SQLite.
    ///
    /// @param argument - how many pages to give back, or all of them
    /// @param at - the attached database the pragma was qualified with
    pub(crate) fn pragma_incremental_vacuum(
        &mut self,
        argument: Option<&PragmaArgument>,
        at: Option<usize>,
    ) -> DbResult<Outcome> {
        let number = at.unwrap_or(MAIN);
        let file = self.file_of(at)?;
        if file.auto_vacuum() != 2 {
            return Ok(Outcome::empty());
        }
        let asked = argument.map(argument_integer).filter(|asked| *asked > 0);
        let steps = asked.map_or(u64::MAX, |asked| u64::try_from(asked).unwrap_or(u64::MAX));
        let rows = steps.min(file.free_pages());
        self.queue_incremental_steps(number, steps)?;
        Ok(Outcome {
            rows: vec![Vec::<OwnedDatum>::new(); usize::try_from(rows).unwrap_or(usize::MAX)],
            names: std::rc::Rc::new(Vec::new()),
            changes: Default::default(),
        })
    }

    /// Queues pages to give back at the commit, and commits when no transaction is
    /// open.
    ///
    /// @param number - the schema
    /// @param steps - how many pages to give back
    fn queue_incremental_steps(&mut self, number: usize, steps: u64) -> DbResult<()> {
        {
            let mut queued = self.writing.incremental_steps().borrow_mut();
            let held = queued.entry(number).or_insert(0);
            *held = held.saturating_add(steps);
        }
        let bit = crate::schema_bit(number);
        if self.writing.batch().is_some() {
            self.writing.set_touched(self.writing.touched() | bit);
            return Ok(());
        }
        let txn = self.writing.next_txn();
        self.writing.set_next_txn(txn.saturating_add(1));
        let participants = self.writing.replace_touched(0) | bit;
        self.commit_across(txn, participants)
    }
}

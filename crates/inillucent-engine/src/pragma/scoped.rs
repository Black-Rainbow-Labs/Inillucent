//! The pragmas whose value belongs to one database.
//!
//! Invariant: **`PRAGMA aux.cache_size = 10` changes `aux` and leaves `main` as it was.** SQLite
//! keeps `cache_size`, `synchronous` and `journal_mode` for each database. This engine keeps one
//! value of each for `main`, which sizes the pool and sets the log's policy, and an attached
//! database or `temp` had no value of its own, so a qualified pragma changed `main`'s.
//!
//! A database other than `main` keeps its value here and it changes nothing about how the engine
//! runs: the pool and the log belong to the connection, as the pages of `main` do.

use inillucent_pool::journal::JournalMode;
use inillucent_sql::declare::{argument_integer, argument_text, sqlite_safety_level};
use inillucent_sql::directive::PragmaArgument;

use crate::Outcome;

use super::*;

/// The journal modes in the order they are stored in.
const MODES: [JournalMode; 6] = [
    JournalMode::Wal,
    JournalMode::Delete,
    JournalMode::Truncate,
    JournalMode::Persist,
    JournalMode::Memory,
    JournalMode::Off,
];

impl crate::ImportedDatabase {
    /// Answers `cache_size`, `synchronous` and `journal_mode` for a database other than `main`.
    ///
    /// `Ok(None)` means the pragma is not one of those or names `main`, and the ordinary
    /// handler answers it.
    ///
    /// @param name - the pragma's folded name
    /// @param argument - the value it was given, when it was given one
    /// @param at - the database it was qualified with, when it was
    pub(super) fn pragma_scoped(
        &mut self,
        name: &[u8],
        argument: Option<&PragmaArgument>,
        at: Option<usize>,
    ) -> DbResult<Option<Outcome>> {
        let Some(database) = at.filter(|held| *held != crate::MAIN) else {
            return Ok(None);
        };
        Ok(Some(match name {
            b"cache_size" => self.scoped_cache_size(database, argument),
            b"synchronous" => self.scoped_synchronous(database, argument)?,
            b"journal_mode" => self.scoped_journal_mode(database, argument),
            _ => return Ok(None),
        }))
    }

    /// Reads or sets `cache_size` for one database. `temp` starts at 0 and the rest at the
    /// default the pool reports.
    ///
    /// @param database - the database
    /// @param argument - the value it was given, when it was given one
    fn scoped_cache_size(&self, database: usize, argument: Option<&PragmaArgument>) -> Outcome {
        let Some(argument) = argument else {
            let bytes = self
                .storage
                .frames
                .saturating_mul(self.storage.page_size.max(1));
            let default = if database == crate::TEMP {
                0
            } else {
                -((bytes / 1024) as i64)
            };
            return named_integer(
                "cache_size",
                self.pragmas.scoped(database, "cache_size").unwrap_or(default),
            );
        };
        // Read as `sqlite3Atoi` reads it, so a number outside 32 bits is 0, as for `main`.
        let asked = i32::try_from(argument_integer(argument)).map_or(0, i64::from);
        self.pragmas.set_scoped(database, "cache_size", asked);
        Outcome::empty()
    }

    /// Reads or sets `synchronous` for one database. `temp` is always 0 and ignores a setting.
    ///
    /// @param database - the database
    /// @param argument - the value it was given, when it was given one
    fn scoped_synchronous(
        &self,
        database: usize,
        argument: Option<&PragmaArgument>,
    ) -> DbResult<Outcome> {
        let Some(argument) = argument else {
            let level = match database {
                crate::TEMP => 0,
                _ => self.pragmas.scoped(database, "synchronous").unwrap_or(2),
            };
            return Ok(named_integer("synchronous", level));
        };
        if !self.autocommit() {
            return Err(inillucent_base::error::refusal(
                "Safety level may not be changed inside a transaction",
            ));
        }
        if database != crate::TEMP {
            let asked = sqlite_safety_level(&argument_text(argument), false, 1);
            let level = i64::from((u16::from(asked) + 1) & 7);
            let held = if level == 0 { 0 } else { level - 1 };
            self.pragmas.set_scoped(database, "synchronous", held);
        }
        Ok(Outcome::empty())
    }

    /// Reads or sets `journal_mode` for one database.
    ///
    /// A database held in memory is always `memory`. `temp` is `delete` until set, and cannot be
    /// `wal`. A set answers with the mode now in force, as it does for `main`.
    ///
    /// @param database - the database
    /// @param argument - the value it was given, when it was given one
    fn scoped_journal_mode(&self, database: usize, argument: Option<&PragmaArgument>) -> Outcome {
        let in_memory = database != crate::TEMP
            && self
                .session_state
                .attached
                .get(database.saturating_sub(crate::FIRST_ATTACHED))
                .is_some_and(|held| held.path.is_none());
        if in_memory {
            return word_row("journal_mode", JournalMode::Memory.word());
        }
        if let Some(mode) = argument
            .map(argument_text)
            .and_then(|asked| JournalMode::named(asked.trim()))
        {
            let refused = database == crate::TEMP && mode == JournalMode::Wal
                || self.pragmas.defensive() && mode == JournalMode::Off;
            if let (false, Some(code)) = (refused, MODES.iter().position(|held| *held == mode)) {
                self.pragmas
                    .set_scoped(database, "journal_mode", i64::try_from(code).unwrap_or(1));
            }
        }
        let code = self.pragmas.scoped(database, "journal_mode").unwrap_or(1);
        let mode = usize::try_from(code)
            .ok()
            .and_then(|code| MODES.get(code).copied())
            .unwrap_or(JournalMode::Delete);
        word_row("journal_mode", mode.word())
    }
}

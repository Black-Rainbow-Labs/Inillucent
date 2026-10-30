//! Reading a script without running it.
//!
//! Invariant: **a script is cut into statements by the parser, before any of them runs, and the
//! words that begin each statement are all this module reads.**

use crate::{Connection, Result};

impl Connection<'_> {
    /// Reports whether a script begins, commits or rolls back a transaction of its own in any of
    /// its statements.
    ///
    /// For a caller that wraps a script in a transaction, as the command line's `batch` does: a
    /// script with its own `BEGIN` and `COMMIT`, such as the output of `.dump`, failed at its
    /// `BEGIN` with "cannot start a transaction within a transaction". The test is the first word
    /// of each statement, with the statements cut where the parser cuts them, so a word inside a
    /// string, a comment or a trigger body is not read as one. `command_line::batch` makes the same
    /// test with the shell's own cutter, and the two have to agree.
    ///
    /// @param sql - the script
    pub fn script_controls_its_own_transaction(&self, sql: &str) -> Result<bool> {
        let mut rest = sql;
        loop {
            let skipped = crate::leading_trivia(rest);
            rest = rest.get(skipped..).unwrap_or("");
            if rest.is_empty() {
                return Ok(false);
            }
            let word: String = rest
                .chars()
                .take_while(char::is_ascii_alphabetic)
                .collect::<String>()
                .to_ascii_uppercase();
            if matches!(word.as_str(), "BEGIN" | "COMMIT" | "END" | "ROLLBACK") {
                return Ok(true);
            }
            let consumed = self
                .engine
                .parsed_statement_length(rest)
                .map_err(|error| self.database.classify(&error))?;
            if consumed == 0 {
                return Ok(false);
            }
            rest = rest.get(consumed..).unwrap_or("");
        }
    }
}

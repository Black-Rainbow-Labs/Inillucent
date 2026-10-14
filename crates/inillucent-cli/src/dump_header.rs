//! How the shell recognises the first lines of a dump, as the reference's shell does.
//!
//! Invariant: **defensive mode is lifted only between a dump's two opening statements and the end
//! of the transaction they open, and only when those are the session's first two statements and
//! the database they load into is empty.**
//! The reference's shell opens every database with `SQLITE_DBCONFIG_DEFENSIVE` on, which makes
//! `PRAGMA writable_schema=ON` do nothing. A `.dump` is still loadable because the shell turns
//! defensive mode off when the first two statements it runs are exactly `PRAGMA foreign_keys=OFF;`
//! and `BEGIN TRANSACTION;`, which is how a dump starts, and turns it on again at the `COMMIT`.
//! Measured against SQLite 3.53.4: a `select 1;` ahead of the two lines, `foreign_keys=ON`, or
//! `BEGIN;` in place of `BEGIN TRANSACTION;` each leave defensive mode on, and a dump then fails on
//! `CREATE TABLE IF NOT EXISTS sqlite_sequence(name,seq);` with
//! `object name reserved for internal use`.

/// How far into a dump header the session has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Stage {
    /// No statement has run yet.
    Start,
    /// `PRAGMA foreign_keys=OFF` was the first statement.
    Pragma,
    /// The header was complete and defensive mode is off until the transaction ends.
    Open,
    /// Nothing more can happen: the input was not a dump, or the transaction has ended.
    Closed,
}

/// What the shell must do to the connection for a statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// Leave defensive mode as it is.
    Nothing,
    /// Turn defensive mode off, because a dump header just ended.
    Lift,
    /// Turn defensive mode back on, because the dump's transaction just ended.
    Restore,
}

/// Works out the next stage and the change a statement makes.
///
/// @param stage - where the session was before this statement
/// @param sql - the statement, with or without its semicolon
pub(crate) fn advance(stage: Stage, sql: &str) -> (Stage, Change) {
    let words = normalised(sql);
    match stage {
        Stage::Start if words == "PRAGMA FOREIGN_KEYS=OFF" => (Stage::Pragma, Change::Nothing),
        Stage::Pragma if words == "BEGIN TRANSACTION" => (Stage::Open, Change::Lift),
        Stage::Open if ends_transaction(&words) => (Stage::Closed, Change::Restore),
        Stage::Open => (Stage::Open, Change::Nothing),
        _ => (Stage::Closed, Change::Nothing),
    }
}

/// Reports whether a statement ends the transaction it is in.
///
/// @param words - the statement, upper cased with its spaces collapsed
fn ends_transaction(words: &str) -> bool {
    let first = words.split(' ').next().unwrap_or("");
    matches!(first, "COMMIT" | "END" | "ROLLBACK")
}

/// Upper cases a statement, drops its trailing semicolon and collapses runs of white space.
///
/// @param sql - the statement
fn normalised(sql: &str) -> String {
    let trimmed = sql.trim().trim_end_matches(';');
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    words.join(" ").to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two opening lines of a dump lift defensive mode and a `COMMIT` restores it.
    #[test]
    fn a_dump_header_lifts_defensive_mode_until_the_commit() {
        let (stage, change) = advance(Stage::Start, "PRAGMA foreign_keys=OFF;");
        assert_eq!((stage, change), (Stage::Pragma, Change::Nothing));
        let (stage, change) = advance(stage, "BEGIN TRANSACTION;");
        assert_eq!((stage, change), (Stage::Open, Change::Lift));
        let (stage, change) = advance(stage, "INSERT INTO t VALUES(1);");
        assert_eq!((stage, change), (Stage::Open, Change::Nothing));
        let (stage, change) = advance(stage, "COMMIT;");
        assert_eq!((stage, change), (Stage::Closed, Change::Restore));
    }

    /// Anything else in the first two places leaves defensive mode on for good.
    #[test]
    fn other_openings_leave_defensive_mode_on() {
        assert_eq!(advance(Stage::Start, "select 1;").0, Stage::Closed);
        assert_eq!(advance(Stage::Pragma, "BEGIN;").0, Stage::Closed);
        assert_eq!(
            advance(Stage::Start, "PRAGMA foreign_keys=ON;").0,
            Stage::Closed
        );
    }
}

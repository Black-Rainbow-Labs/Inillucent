//! `DELETE ... LIMIT` and `UPDATE ... LIMIT`, asked of an oracle built
//! without them.
//!
//! Invariant: **a limited `DELETE` or `UPDATE` is graded by what it did, not
//! refused as a build difference.** The pinned SQLite is compiled without
//! `SQLITE_ENABLE_UPDATE_DELETE_LIMIT`, so it refuses the statement this engine
//! runs, as builds with that option do. The oracle is sent the same statement
//! with its `ORDER BY` and `LIMIT` moved into a `rowid IN (...)` subquery,
//! which is what the option does, and the two answers, their counters and the
//! state after them are compared as for any other statement (section 6.2 of the
//! design).
//!
//! A `RETURNING` clause, written before the `ORDER BY` or after the `LIMIT`
//! (this engine reads both, `docs/sql.md` lists the second as an extension), is
//! moved to the end of the rewritten statement, the one place SQLite reads it.
//!
//! A statement this cannot rewrite, because it has an `UPDATE ... FROM` or a
//! `WITH` in front, is sent unchanged, and `deliberate.toml` names the
//! difference.

use crate::statement_matrix::case::top_level_spans;

/// The clauses that can follow the target of a limited statement, in the order
/// SQL allows them.
const TAIL: &[&str] = &["WHERE", "ORDER BY", "LIMIT"];

/// Returns the oracle's form of a limited `DELETE` or `UPDATE`, or `None` when
/// the statement is not one, or is one this cannot rewrite.
///
/// @param sql - one statement
pub fn rewrite(sql: &str) -> Option<String> {
    let trimmed = sql.trim().trim_end_matches(';').trim_end();
    let (without, returning) = split_returning(trimmed);
    let rewritten = rewrite_without_returning(&without)?;
    Some(format!("{rewritten}{returning}"))
}

/// Takes a top level `RETURNING` clause out of a statement.
///
/// SQLite reads `RETURNING` last in the `rowid IN` form the oracle is sent, and
/// this engine reads it before `ORDER BY` or after `LIMIT`, so the clause is cut
/// from where it was written, up to the next `ORDER BY` or `LIMIT` or the end,
/// and handed back to be appended.
///
/// @param sql - one statement without its final semicolon
fn split_returning(sql: &str) -> (String, String) {
    let Some((start, _)) = top_level_spans(sql, "RETURNING").first().copied() else {
        return (sql.to_string(), String::new());
    };
    let end = ["ORDER BY", "LIMIT"]
        .iter()
        .filter_map(|phrase| {
            top_level_spans(sql, phrase)
                .into_iter()
                .map(|(at, _)| at)
                .find(|at| *at > start)
        })
        .min()
        .unwrap_or(sql.len());
    let before = sql.get(..start).unwrap_or("").trim_end();
    let after = sql.get(end..).unwrap_or("").trim_start();
    let clause = sql.get(start..end).unwrap_or("").trim();
    (format!("{before} {after}"), format!(" {clause}"))
}

/// Rewrites a limited statement that has no `RETURNING` clause.
///
/// @param trimmed - one statement without its final semicolon
fn rewrite_without_returning(trimmed: &str) -> Option<String> {
    let trimmed = trimmed.trim();
    let upper = trimmed.to_ascii_uppercase();
    let limit = top_level_spans(trimmed, "LIMIT").last().copied()?;
    let tail_start = TAIL
        .iter()
        .filter_map(|phrase| {
            top_level_spans(trimmed, phrase)
                .first()
                .map(|(start, _)| *start)
        })
        .min()
        .unwrap_or(limit.0);
    let tail = trimmed.get(tail_start..)?.trim();
    if upper.starts_with("DELETE") {
        let from = top_level_spans(trimmed, "FROM").first().copied()?;
        let target = trimmed.get(from.1..tail_start)?.trim();
        return Some(format!(
            "DELETE FROM {target} WHERE rowid IN (SELECT rowid FROM {target} {tail})"
        ));
    }
    if upper.starts_with("UPDATE") {
        let set = top_level_spans(trimmed, "SET").first().copied()?;
        if top_level_spans(trimmed, "FROM")
            .iter()
            .any(|(start, _)| *start > set.1)
        {
            return None;
        }
        let head = trimmed.get(..set.0)?.trim();
        let target = head
            .split_whitespace()
            .skip_while(|word| {
                let word = word.to_ascii_uppercase();
                word == "UPDATE" || word == "OR"
            })
            .skip_while(|word| {
                ["ROLLBACK", "ABORT", "REPLACE", "FAIL", "IGNORE"]
                    .contains(&word.to_ascii_uppercase().as_str())
            })
            .collect::<Vec<&str>>()
            .join(" ");
        let assignments = trimmed.get(set.1..tail_start)?.trim();
        return Some(format!(
            "{head} SET {assignments} WHERE rowid IN (SELECT rowid FROM {target} {tail})"
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rewritten forms keep the target, the filter, the order and the
    /// limit.
    #[test]
    fn a_limited_statement_becomes_a_rowid_subquery() {
        assert_eq!(
            rewrite("DELETE FROM t WHERE a > 1 ORDER BY a LIMIT 5").as_deref(),
            Some(
                "DELETE FROM t WHERE rowid IN (SELECT rowid FROM t WHERE a > 1 ORDER BY a LIMIT 5)"
            )
        );
        assert_eq!(
            rewrite("DELETE FROM t LIMIT 5 OFFSET 2").as_deref(),
            Some("DELETE FROM t WHERE rowid IN (SELECT rowid FROM t LIMIT 5 OFFSET 2)")
        );
        assert_eq!(
            rewrite("UPDATE OR IGNORE t SET b = 'z' ORDER BY a DESC LIMIT 1").as_deref(),
            Some(
                "UPDATE OR IGNORE t SET b = 'z' WHERE rowid IN (SELECT rowid FROM t ORDER BY a \
                 DESC LIMIT 1)"
            )
        );
    }

    /// A statement with no top level `LIMIT`, or one this cannot rewrite, is
    /// left alone.
    #[test]
    fn other_statements_are_left_alone() {
        assert!(rewrite("DELETE FROM t WHERE a IN (SELECT a FROM u LIMIT 1)").is_none());
        assert!(rewrite("SELECT a FROM t LIMIT 1").is_none());
        assert!(rewrite("UPDATE t SET a = 1 FROM u WHERE t.a = u.a LIMIT 1").is_none());
    }

    /// A `RETURNING` clause moves to the end of the rewritten statement from
    /// either place this engine reads it.
    #[test]
    fn returning_follows_the_rewritten_statement() {
        let wanted =
            "DELETE FROM t WHERE rowid IN (SELECT rowid FROM t ORDER BY a LIMIT 1) RETURNING a, b";
        assert_eq!(
            rewrite("DELETE FROM t ORDER BY a LIMIT 1 RETURNING a, b").as_deref(),
            Some(wanted)
        );
        assert_eq!(
            rewrite("DELETE FROM t RETURNING a, b ORDER BY a LIMIT 1").as_deref(),
            Some(wanted)
        );
        assert!(rewrite("DELETE FROM t RETURNING a").is_none());
    }
}

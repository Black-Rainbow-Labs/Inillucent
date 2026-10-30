//! Reading what a plan's arguments ask for.
//!
//! Invariant: **each argument of a plan has a role, the plan string says which, and the roles are
//! read the way `fts5FilterMethod` reads its own.** `t` is a `MATCH` on the table's own column,
//! `c<n>` is a `MATCH` on column n, `r` is the rank function from `rank MATCH` or `rank =`, and
//! `=` is a rowid that the match is limited to. A text that is NULL is the empty string, which
//! the query parser then refuses, as it does in SQLite.

use inillucent_value::Value;

use super::super::FilterPlan;
use super::index::text_of;

/// What the arguments of one plan ask for.
pub(super) struct Constraints {
    /// Each `MATCH` text with the column it was written against, in the order they were given.
    pub(super) queries: Vec<(Vec<u8>, Option<usize>)>,
    /// The rank function the plan named: `None` when it named none, and `Some(None)` when it
    /// named NULL.
    pub(super) rank: Option<Option<Vec<u8>>>,
    /// The rowid a match is limited to, when the statement gave one.
    pub(super) rowid: Option<Value<'static>>,
    /// Whether a `LIKE` or `GLOB` was given whose pattern has no piece to narrow by, so that it
    /// leaves every row a candidate.
    pub(super) open_pattern: bool,
}

/// Reads the arguments of a plan by the roles its plan string gives them.
///
/// Only the first rank constraint is used, as SQLite's `fts5BestIndexMethod` does.
///
/// @param plan - the plan `best_index` chose and the values it was given
pub(super) fn read_constraints(plan: &FilterPlan) -> Constraints {
    let roles: Vec<&str> = plan.index_string.split(',').collect();
    let mut found = Constraints {
        queries: Vec::new(),
        rank: None,
        rowid: None,
        open_pattern: false,
    };
    for (position, argument) in plan.arguments.iter().enumerate() {
        let role = roles.get(position).copied().unwrap_or("t");
        match role {
            "r" if found.rank.is_none() => found.rank = Some(text_of(argument)),
            "r" => {}
            "=" => found.rowid = Some(argument.clone()),
            // A `LIKE` or `GLOB` on a trigram index adds the phrases of its pattern as a query on
            // the column, or nothing when the pattern has none.
            _ if role.starts_with(['L', 'G']) => match super::pattern::narrowed(role, argument) {
                Some(Some((column_role, query))) => {
                    let column = column_role
                        .strip_prefix('c')
                        .and_then(|digits| digits.parse::<usize>().ok());
                    found.queries.push((query, column));
                }
                _ => found.open_pattern = true,
            },
            _ => {
                let column = role
                    .strip_prefix('c')
                    .and_then(|digits| digits.parse::<usize>().ok());
                found
                    .queries
                    .push((text_of(argument).unwrap_or_default(), column));
            }
        }
    }
    found
}

/// Reads a value compared with a rowid the way an `INTEGER PRIMARY KEY` column reads it.
///
/// A real with no fraction is that integer, and text that is a whole number is too, so
/// `rowid = '5'` and `rowid = 5.0` find row 5. Anything else is no rowid.
///
/// @param value - the value the rowid is compared with
pub(super) fn rowid_of(value: &Value<'static>) -> Option<i64> {
    match value {
        Value::Integer(number) => Some(*number),
        Value::Real(number) => whole_number(*number),
        Value::Text(text) => {
            let bytes = text.utf8_bytes();
            let trimmed = String::from_utf8_lossy(&bytes).trim().to_string();
            if let Ok(number) = trimmed.parse::<i64>() {
                return Some(number);
            }
            let numeric = !trimmed.is_empty()
                && trimmed
                    .chars()
                    .all(|c| matches!(c, '0'..='9' | '.' | '+' | '-' | 'e' | 'E'));
            numeric
                .then(|| trimmed.parse::<f64>().ok())
                .flatten()
                .and_then(whole_number)
        }
        _ => None,
    }
}

/// Reads the rowid that limits a match, which SQLite takes only when the value is an integer.
///
/// `fts5GetRowidLimit` asks for the value's numeric type and uses the limit only when that is
/// INTEGER. An integer and text that is a whole number written without a fraction are, so
/// `rowid = '5'` limits a match to row 5. A real, `'5.0'`, NULL, a blob and text that is not a
/// number are not, and then the equality limits nothing: every row the match found is returned.
///
/// @param value - the value the rowid was compared with
pub(super) fn match_rowid_limit(value: &Value<'static>) -> Option<i64> {
    match value {
        Value::Integer(number) => Some(*number),
        Value::Text(text) => {
            let bytes = text.utf8_bytes();
            let trimmed = String::from_utf8_lossy(&bytes).trim().to_string();
            let digits = trimmed.strip_prefix(['+', '-']).unwrap_or(&trimmed);
            let plain = !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit());
            plain.then(|| trimmed.parse::<i64>().ok()).flatten()
        }
        _ => None,
    }
}

/// Returns the integer a real is, when it has no fraction and fits.
///
/// @param number - the real
fn whole_number(number: f64) -> Option<i64> {
    (number.fract() == 0.0 && number.abs() < 9.2e18).then_some(number as i64)
}

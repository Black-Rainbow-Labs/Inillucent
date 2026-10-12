//! When a `HAVING` with no `GROUP BY` is a group over the whole table, and when
//! it is a statement to refuse.
//!
//! Invariant: **the grammar accepts every `HAVING`, and this decides which ones
//! are legal, in SQLite's own words.** The two used to disagree: `HAVING` was
//! read only inside the `GROUP BY` arm of `parse_select_core`, so
//! `SELECT count(*) AS n FROM t HAVING n > 0` - which the reference answers
//! with the count - stopped at `near "HAVING": syntax error` (task-2040). That
//! is the worst refusal available for it. Exit code 3 and the `unsupported`
//! status exist so a caller can tell "this engine has not built that" from
//! "your SQL is wrong", and a syntax error said the second about a statement
//! that is correct, which sends the caller rewording SQL that needs no
//! rewording. Deciding it here keeps the parser's job to shape and this one to
//! meaning, and keeps the sentence in one place.

use super::BoundExpr;
use crate::diagnostic::ParseError;
use crate::lexer::Span;

/// Splits a `HAVING` into the terms that can filter rows before grouping and the rest.
///
/// **SQLite moves a `HAVING` term to the `WHERE` when it needs no aggregate and reads only
/// `GROUP BY` columns or constants** (`havingToWhere`). The rows of a group a term removes are
/// then never accumulated, so `SELECT sum(v) FROM t GROUP BY g HAVING g = 'a'` does not add up
/// the `integer overflow` in a group the `HAVING` discards. Here every group was finished before
/// the filter looked at it, and the overflow was raised for a row nobody asked for.
///
/// Only a plain list of node kinds that cannot differ between the rows of one group is moved. A
/// column counts only when the `GROUP BY` holds exactly that column, so a `COLLATE` on the group
/// term, a function call and a subquery all keep the term where it was.
///
/// @param having - the bound `HAVING`
/// @param group - the bound `GROUP BY` terms
pub(crate) fn split_for_where(
    having: BoundExpr,
    group: &[BoundExpr],
) -> (Vec<BoundExpr>, Option<BoundExpr>) {
    let mut terms = Vec::new();
    flatten_and(having, &mut terms);
    let (moved, kept): (Vec<_>, Vec<_>) = terms
        .into_iter()
        .partition(|term| is_group_constant(term, group));
    let rest = kept
        .into_iter()
        .reduce(|left, right| BoundExpr::And(Box::new(left), Box::new(right)));
    (moved, rest)
}

/// Lists the terms of an `AND` chain in the order they were written.
///
/// @param expr - the expression to split
/// @param into - where the terms go
fn flatten_and(expr: BoundExpr, into: &mut Vec<BoundExpr>) {
    match expr {
        BoundExpr::And(left, right) => {
            flatten_and(*left, into);
            flatten_and(*right, into);
        }
        other => into.push(other),
    }
}

/// Reports whether an expression has the same value for every row of one group.
///
/// @param expr - the expression
/// @param group - the bound `GROUP BY` terms
fn is_group_constant(expr: &BoundExpr, group: &[BoundExpr]) -> bool {
    match expr {
        BoundExpr::Null
        | BoundExpr::Integer(_)
        | BoundExpr::Real(_)
        | BoundExpr::Text(_)
        | BoundExpr::Blob(_)
        | BoundExpr::Parameter(_) => true,
        BoundExpr::Column { .. } => group.contains(expr),
        BoundExpr::Compare { .. }
        | BoundExpr::Is { .. }
        | BoundExpr::IsNull { .. }
        | BoundExpr::Not(_)
        | BoundExpr::And(..)
        | BoundExpr::Or(..)
        | BoundExpr::Arithmetic { .. }
        | BoundExpr::Between { .. }
        | BoundExpr::InList { .. }
        | BoundExpr::Unary { .. }
        | BoundExpr::Cast { .. }
        | BoundExpr::Pattern { .. }
        | BoundExpr::Case { .. } => expr
            .children()
            .into_iter()
            .all(|child| is_group_constant(child, group)),
        _ => false,
    }
}

/// Refuses a `HAVING` on a statement that does not aggregate.
///
/// **What makes a statement an aggregating one, for SQLite, is an aggregate
/// among the *result columns* and nothing else.** A `GROUP BY` makes one too.
/// An aggregate that appears only in the `HAVING`, or only in the `ORDER BY`,
/// does not, which is why the count is taken before the `HAVING` is bound
/// rather than read off the binder afterwards - by then `self.aggregates` holds
/// the ones the `HAVING` itself introduced. Measured against the pinned
/// `sqlite3` 3.53.4:
///
/// ```sql
/// SELECT count(*) AS n FROM t HAVING n > 0;    -- 3
/// SELECT count(*) FROM t HAVING b > 0;         -- 3
/// SELECT 1 FROM t HAVING count(*) > 0;         -- HAVING clause on a non-aggregate query
/// SELECT 1 FROM t HAVING 1 ORDER BY count(*);  -- HAVING clause on a non-aggregate query
/// ```
///
/// The failure is `ParseErrorKind::Refused`, not `Unsupported`: the reference
/// refuses these too, so no release of this engine will ever accept them, and
/// `unsupported` would tell a caller to wait for a feature that is not coming.
/// Its span is the default one, which `statements::refused` reads as
/// positionless - the reference reports this error with no offset, so its shell
/// prints the sentence and no caret art, and a span here would make two
/// transcripts differ over two lines of drawing rather than over an answer.
///
/// @param has_having - whether the statement carries a `HAVING` at all
/// @param group_terms - how many `GROUP BY` terms it has
/// @param aggregates_in_columns - how many aggregates its result columns asked for
pub(crate) fn refuse_when_nothing_aggregates(
    has_having: bool,
    group_terms: usize,
    aggregates_in_columns: usize,
) -> Result<(), ParseError> {
    if has_having && group_terms == 0 && aggregates_in_columns == 0 {
        return Err(crate::bind::refused(
            "HAVING clause on a non-aggregate query",
            Span::default(),
        ));
    }
    Ok(())
}

/// Moves the `HAVING` terms that read only group keys into `WHERE`, where they
/// filter rows before the grouping; see [`split_for_where`].
///
/// @param having - the bound `HAVING`, left holding the terms that stay
/// @param filter - the bound `WHERE`, which the moved terms are joined to
/// @param group - the bound `GROUP BY` keys
pub(crate) fn move_group_terms_to_where(
    having: &mut Option<BoundExpr>,
    filter: &mut Option<BoundExpr>,
    group: &[BoundExpr],
) {
    let Some(having_expr) = having.as_ref() else {
        return;
    };
    if group.is_empty() {
        return;
    }
    let (moved, rest) = split_for_where(having_expr.clone(), group);
    for term in moved {
        *filter = Some(match filter.take() {
            Some(existing) => BoundExpr::And(Box::new(existing), Box::new(term)),
            None => term,
        });
    }
    *having = rest;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three shapes that are legal, and the one that is not.
    ///
    /// The `GROUP BY` row is the one that matters most: a `HAVING` over a
    /// grouped statement with no aggregate anywhere is legal, and a rule
    /// written as "there must be an aggregate" would refuse
    /// `SELECT a FROM t GROUP BY a HAVING a > 15`, which the reference answers.
    #[test]
    fn only_a_having_over_a_statement_that_does_not_aggregate_is_refused() {
        assert!(refuse_when_nothing_aggregates(false, 0, 0).is_ok());
        assert!(refuse_when_nothing_aggregates(true, 1, 0).is_ok());
        assert!(refuse_when_nothing_aggregates(true, 0, 1).is_ok());
        let refused = refuse_when_nothing_aggregates(true, 0, 0)
            .expect_err("a HAVING over a statement that does not aggregate is refused");
        assert_eq!(refused.message(), "HAVING clause on a non-aggregate query");
    }

    /// The refusal points at nothing, so the shell draws no caret under it.
    ///
    /// `statements::refused` attaches an offset only when the span is not the
    /// default one, so this is what keeps the transcript equal to the
    /// reference's.
    #[test]
    fn the_refusal_carries_no_position() {
        let refused = refuse_when_nothing_aggregates(true, 0, 0)
            .expect_err("a HAVING over a statement that does not aggregate is refused");
        assert_eq!(refused.span, Span::default());
    }
}

//! The refusals the binder answers with, each in SQLite's own words.
//!
//! Invariant: **a refusal says what SQLite says.** Not approximately, and not in
//! this engine's own phrasing: an application that matches on a message, and a
//! shell that draws a caret under a span, are both reading a contract. The
//! comments here record where a wording was measured against the reference and
//! what the difference was, because that is the part a rewrite would lose.
//!
//! ## Why they are in a file of their own
//!
//! `bind.rs` reached the ceiling `crates/inillucent-compat/tests/tooling/policy.rs`
//! records for it, and that check's message says what to do about it: extract
//! something rather than raise the number, because the way a module reaches
//! eight thousand lines is that every individual addition to it was reasonable.
//!
//! These were the obvious thing to take. Every one is a pure function from a
//! name and a span to a `ParseError`, none of them reads the binder's state, and
//! they share one job. Nothing about the binder had to change to move them - the
//! call sites are the same names, re-exported.

use crate::diagnostic::{ParseError, ParseErrorKind};
use crate::lexer::{QuoteForm, Span};

/// Returns a refusal that carries its own wording.
pub(crate) fn schema_refused(message: impl Into<String>, span: Span) -> ParseError {
    ParseError::new(ParseErrorKind::Refused(message.into()), span)
}

/// Returns an "unsupported construct" failure.
pub(crate) fn unsupported(what: &'static str, span: Span) -> ParseError {
    ParseError::new(ParseErrorKind::Unsupported(what), span)
}

/// Returns a "no such table" failure in SQLite's wording.
///
/// It deliberately carries no position. SQLite reports one for `no such
/// column` and not for this, and a caller that draws a caret under the offset -
/// the shell does - would otherwise point at a table name where the reference
/// points at nothing.
pub(crate) fn no_such_table(name: &[u8], span: Span) -> ParseError {
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused(format!("no such table: {}", String::from_utf8_lossy(name))),
        Span::default(),
    )
}

/// Names the schema in a "no such table" failure that came out of a stored view
/// or trigger body.
///
/// SQLite fixes every table a stored body names to the schema the body was
/// created in, so a view over a table that was dropped answers `no such table:
/// main.book` where a plain statement answers `no such table: book`. A temporary
/// object is not fixed to a schema, and a table that is already qualified is left
/// as it is.
///
/// @param error - the failure the body's bind returned
/// @param database - the schema the view or trigger is stored in
pub(crate) fn qualify_missing_table(error: ParseError, database: &[u8]) -> ParseError {
    let ParseErrorKind::Refused(message) = &error.kind else {
        return error;
    };
    let Some(missing) = message.strip_prefix("no such table: ") else {
        return error;
    };
    if missing.contains('.') || database.eq_ignore_ascii_case(b"temp") {
        return error;
    }
    let qualified = format!(
        "no such table: {}.{missing}",
        String::from_utf8_lossy(database)
    );
    ParseError::new(ParseErrorKind::Refused(qualified), error.span)
}

/// Returns a "no such column" failure in SQLite's wording, with the hint a
/// double-quoted name earns.
///
/// A bare `"word"` is an identifier that *may* fall back to a string literal,
/// and this build refuses the fallback the way the reference's does. The
/// reference does not simply refuse it, though: it re-quotes the name and adds
/// the sentence that tells the author what they probably meant. The hint is
/// only for the unqualified form, because `t."b"` cannot be a string literal in
/// any dialect and the reference prints `no such column: t.b` for it.
///
/// @param name - the column name as written, with quoting already removed
/// @param quote - how it was quoted
/// @param span - where it was written
pub(crate) fn no_such_column_quoted(name: &[u8], quote: QuoteForm, span: Span) -> ParseError {
    if quote != QuoteForm::Double {
        return no_such_column(name, span);
    }
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "no such column: \"{}\" - should this be a string literal in single-quotes?",
            String::from_utf8_lossy(name)
        )),
        span,
    )
}

/// Returns a "no such index" failure, for an `INDEXED BY` that names none.
///
/// @param name - the index name as written
/// @param span - where to point the diagnostic
pub(crate) fn no_such_index(name: &[u8], span: Span) -> ParseError {
    // SQLite points at nothing for this failure, so the span is dropped; the callers
    // still pass it because they know where the text is.
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused(format!("no such index: {}", String::from_utf8_lossy(name))),
        Span::default(),
    )
}

/// Returns SQLite's "no query solution", for an `INDEXED BY` whose index
/// cannot answer the statement; `plan::unanswerable_index_hint` says which.
///
/// @param span - where to point the diagnostic, which is nowhere for both
///   callers because SQLite's own message has no position
pub(crate) fn no_query_solution(span: Span) -> ParseError {
    ParseError::new(
        ParseErrorKind::Refused("no query solution".to_string()),
        span,
    )
}

/// Returns a "no such column" failure in SQLite's wording.
pub(crate) fn no_such_column(name: &[u8], span: Span) -> ParseError {
    ParseError::new(
        ParseErrorKind::Refused(format!("no such column: {}", String::from_utf8_lossy(name))),
        span,
    )
}

/// Returns an "ambiguous column name" failure.
pub(crate) fn ambiguous_column(name: &[u8], span: Span) -> ParseError {
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "ambiguous column name: {}",
            String::from_utf8_lossy(name)
        )),
        span,
    )
}

/// The names that exist but only inside a window frame.
///
/// A call to one of these outside `OVER (...)` is a misuse, not an absence, and
/// SQLite says so. Reporting it as "no such function" made an audit that
/// enumerated by calling names count eleven functions as missing that are
/// present and byte-identical over a real frame.
const WINDOW_ONLY: &[&[u8]] = &[
    b"cume_dist",
    b"dense_rank",
    b"first_value",
    b"lag",
    b"last_value",
    b"lead",
    b"nth_value",
    b"ntile",
    b"percent_rank",
    b"rank",
    b"row_number",
];

/// The names that exist but only where a virtual table can answer them.
///
/// FTS5's and FTS3/4's auxiliary functions take the table as their first
/// argument and are answered by the module's cursor; called anywhere else there
/// is no cursor to ask. SQLite refuses those with "unable to use function X in
/// the requested context", and so does this - the twelve of them were the rest
/// of the twenty-three names the audit read as missing.
const CONTEXT_ONLY: &[&[u8]] = &[
    b"bm25",
    b"fts5",
    b"fts5_get_locale",
    b"fts5_insttoken",
    b"fts5_locale",
    b"highlight",
    b"match",
    b"matchinfo",
    b"offsets",
    b"optimize",
    b"snippet",
];

/// Returns the failure a name that did not resolve deserves.
///
/// **Three different facts, three different sentences.** A name nobody has is
/// "no such function". A window function outside a frame is a misuse. An
/// auxiliary function outside the virtual table that answers it is a context
/// error. The engine used to say the first about all three, which is the only
/// one of the three that is a claim about *existence* - so an audit that probed
/// by calling read twenty-three present functions as absent.
///
/// @param written - the name as it was written; SQLite repeats it in that case
/// @param span - where it was written
pub(crate) fn no_such_function(written: &[u8], span: Span) -> ParseError {
    let folded = written.to_ascii_lowercase();
    let name = folded.as_slice();
    if let Some(said) = crate::function::needs_a_component(name) {
        return ParseError::new(ParseErrorKind::Unsupported(said), span);
    }
    if WINDOW_ONLY.contains(&name) {
        return ParseError::new(
            ParseErrorKind::Refused(format!(
                "misuse of window function {}()",
                String::from_utf8_lossy(name)
            )),
            span,
        );
    }
    if CONTEXT_ONLY.contains(&name) {
        // SQLite spells `MATCH` in upper case here and the rest as written,
        // because `MATCH` reaches this path as the operator's keyword.
        let spelled = if name == b"match" {
            "MATCH".to_string()
        } else {
            String::from_utf8_lossy(written).into_owned()
        };
        return ParseError::new(
            ParseErrorKind::Refused(format!(
                "unable to use function {spelled} in the requested context"
            )),
            span,
        );
    }
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "no such function: {}",
            String::from_utf8_lossy(written)
        )),
        span,
    )
}

/// Returns a "wrong number of arguments" failure.
pub(crate) fn wrong_arguments(name: &[u8], span: Span) -> ParseError {
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "wrong number of arguments to function {}()",
            String::from_utf8_lossy(name)
        )),
        span,
    )
}

/// Returns a "no such collation" failure.
pub(crate) fn no_such_collation(name: &[u8], span: Span) -> ParseError {
    // SQLite points at nothing for this failure, so the span is dropped; the callers
    // still pass it because they know where the text is.
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "no such collation sequence: {}",
            String::from_utf8_lossy(name)
        )),
        Span::default(),
    )
}

/// Returns a number with SQLite's English ordinal suffix: `1st`, `2nd`, `3rd`, `4th`,
/// `11th`, `12th`, `13th`, `21st`.
///
/// SQLite's `%r` conversion is what words these messages, and the suffix rule treats
/// 11, 12 and 13 as `th` in every hundred.
///
/// @param number - the one based position being named
pub(crate) fn ordinal(number: usize) -> String {
    let suffix = match (number % 100, number % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{number}{suffix}")
}

/// Writes a position as SQLite's `%r` does: 1st, 2nd, 3rd, 4th, 11th, 21st.
///
/// @param position - the one-based position
pub(crate) fn ordinal_word(position: usize) -> String {
    let suffix = match (position % 100, position % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{position}{suffix}")
}

/// Returns an "ORDER BY term out of range" failure.
///
/// SQLite numbers the term by its place in the list, not by the integer written
/// in it, and says how many result columns there are.
///
/// @param position - the one-based place of the term in the `ORDER BY` list
/// @param columns - how many result columns the query has
/// @param span - where the term was written
pub(crate) fn order_out_of_range(position: usize, columns: usize, span: Span) -> ParseError {
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "{} ORDER BY term out of range - should be between 1 and {columns}",
            ordinal_word(position)
        )),
        span,
    )
}

/// Returns a "GROUP BY term out of range" failure.
///
/// @param position - the one-based place of the term in the `GROUP BY` list
/// @param columns - how many result columns the query has
/// @param span - where the term was written
pub(crate) fn group_out_of_range(position: usize, columns: usize, span: Span) -> ParseError {
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "{} GROUP BY term out of range - should be between 1 and {columns}",
            ordinal_word(position)
        )),
        span,
    )
}

/// Returns the failure a compound's `ORDER BY` gives when it names nothing.
///
/// @param position - the one based place of the term in the `ORDER BY` list
/// @param span - where the term was written
pub(crate) fn compound_order_unmatched(position: usize, span: Span) -> ParseError {
    // SQLite points at nothing for this failure, so the span is dropped; the callers
    // still pass it because they know where the text is.
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "{} ORDER BY term does not match any column in the result set",
            ordinal(position)
        )),
        Span::default(),
    )
}

/// Returns the failure for a compound whose arms have different numbers of columns.
///
/// SQLite names the operator, so `UNION ALL` and `UNION` read differently.
///
/// @param operator - the compound operator between the two arms
/// @param span - where to point the diagnostic
pub(crate) fn compound_width_mismatch(operator: crate::ast::CompoundOp, span: Span) -> ParseError {
    // SQLite points at nothing for this failure, so the span is dropped; the callers
    // still pass it because they know where the text is.
    let _ = span;
    let name = match operator {
        crate::ast::CompoundOp::Union => "UNION",
        crate::ast::CompoundOp::UnionAll => "UNION ALL",
        crate::ast::CompoundOp::Intersect => "INTERSECT",
        crate::ast::CompoundOp::Except => "EXCEPT",
    };
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "SELECTs to the left and right of {name} do not have the same number of result columns"
        )),
        Span::default(),
    )
}

/// Returns the failure for a `VALUES` list whose rows have different widths.
///
/// @param span - where to point the diagnostic
pub(crate) fn values_width_mismatch(span: Span) -> ParseError {
    // SQLite points at nothing for this failure, so the span is dropped; the callers
    // still pass it because they know where the text is.
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused("all VALUES must have the same number of terms".to_string()),
        Span::default(),
    )
}

/// Rewords an aggregate misuse found in the `WHERE` of a query that does aggregate.
///
/// SQLite finds the aggregate in such a query by a second pass over the statement,
/// after the first pass allowed it, and that pass words the failure
/// `misuse of aggregate: count()` with a colon where the first pass says `function`.
/// A query with no aggregate and no `GROUP BY` fails in the first pass.
///
/// @param error - the failure the `WHERE` produced
pub(crate) fn reword_for_aggregate_query(error: ParseError) -> ParseError {
    match &error.kind {
        ParseErrorKind::Refused(message)
            if message.starts_with("misuse of aggregate function ") =>
        {
            let reworded = message.replacen("aggregate function ", "aggregate: ", 1);
            ParseError::new(ParseErrorKind::Refused(reworded), error.span)
        }
        _ => error,
    }
}

/// Returns the failure for a named column list that does not match its query.
///
/// @param name - the table or common table expression the list belongs to
/// @param found - how many columns the query produces
/// @param named - how many names the list gives
/// @param span - where to point the diagnostic
pub(crate) fn named_column_count(
    name: &[u8],
    found: usize,
    named: usize,
    span: Span,
) -> ParseError {
    // SQLite points at nothing for this failure, so the span is dropped; the callers
    // still pass it because they know where the text is.
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "table {} has {found} values for {named} columns",
            String::from_utf8_lossy(name)
        )),
        Span::default(),
    )
}

/// Returns the failure for a subquery that returns the wrong number of columns.
///
/// @param found - how many columns the subquery returns
/// @param expected - how many the expression needs
/// @param span - where to point the diagnostic
pub(crate) fn subquery_width_mismatch(found: usize, expected: usize, span: Span) -> ParseError {
    // SQLite points at nothing for this failure, so the span is dropped; the callers
    // still pass it because they know where the text is.
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "sub-select returns {found} columns - expected {expected}"
        )),
        Span::default(),
    )
}

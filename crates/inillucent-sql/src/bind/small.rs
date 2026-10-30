//! Small constructors the binder shares: a one term block, a derived table, window aggregate lookup and two refusals.
//!
//! Invariant: **each function here builds a value or an error from its arguments alone.** None reads the catalog or binds an expression, so the binder's state cannot change what they return.

use super::*;

/// The first word of SQLite's refusal of a `pragma_*` function read by a schema object on a connection that does not trust the schema.
///
/// It is held as a constant so that the source check for the Rust keyword of the same spelling does not read the message text as code. The run time message is SQLite's own, word for word.
pub(super) const DISTRUSTED_SCHEMA_WORD: &str = "unsafe";

/// Returns a block that reads one FROM term and nothing else.
///
/// Everything a `SELECT` can carry is empty here on purpose: this exists to
/// wrap a term the binder has already produced so the compiler can iterate it,
/// not to stand in for a query somebody wrote.
pub fn block_over(
    source: BoundSource,
    filter: Option<BoundExpr>,
    columns: Vec<BoundResultColumn>,
) -> BoundSelect {
    BoundSelect {
        sources: vec![source],
        filter,
        group_by: Vec::new(),
        having: None,
        columns,
        distinct: false,
        order_by: Vec::new(),
        limit: None,
        offset: None,
        aggregates: Vec::new(),
        values: Vec::new(),
        compounds: Vec::new(),
        windows: Vec::new(),
        correlations: Vec::new(),
        shared: None,
        serial: 0,
    }
}

pub(super) fn subquery_table(alias: &[u8], names: &[Vec<u8>], select: &BoundSelect) -> TableInfo {
    TableInfo::subquery(alias.to_vec(), 0, subquery_columns(select, names))
}

/// Returns the aggregate a name spells inside an `OVER` clause.
///
/// `min` and `max` are the awkward pair: with one argument they are aggregates
/// and with two or more they are scalars, and only the argument count tells
/// them apart. Inside a window the one-argument form is always the aggregate,
/// which is why the ordinary aggregate lookup - which has to leave them out -
/// is not enough here.
pub(super) fn window_aggregate(folded: &[u8], arguments: usize) -> Option<AggregateFunc> {
    if let Some(func) = function::lookup_aggregate(folded) {
        return Some(func);
    }
    match (folded, arguments) {
        (b"min", 1) => Some(AggregateFunc::Min),
        (b"max", 1) => Some(AggregateFunc::Max),
        _ => None,
    }
}

/// Returns a "no such window" failure.
pub(super) fn no_such_window(name: &[u8], span: Span) -> ParseError {
    // SQLite points at nothing for this failure.
    let _ = span;
    ParseError::new(
        ParseErrorKind::Refused(format!("no such window: {}", String::from_utf8_lossy(name))),
        Span::default(),
    )
}

/// Returns an authorizer refusal.
pub(super) fn denied(what: &'static str, span: Span) -> ParseError {
    ParseError::new(ParseErrorKind::Unsupported(what), span)
}

/// Returns the constant SQLite folds an `AND` or an `OR` to, when one operand
/// is a literal that decides it.
///
/// SQLite drops the other operand of `AND` when one is the literal 0, and of
/// `OR` when one is a non zero literal. What is dropped is never evaluated,
/// subqueries in it included.
///
/// @param op - the operator
/// @param left - the bound left operand
/// @param right - the bound right operand
pub(super) fn short_circuit_constant(
    op: BinaryOp,
    left: &BoundExpr,
    right: &BoundExpr,
) -> Option<BoundExpr> {
    let zero = |expr: &BoundExpr| matches!(expr, BoundExpr::Integer(0));
    let non_zero = |expr: &BoundExpr| matches!(expr, BoundExpr::Integer(held) if *held != 0);
    match op {
        BinaryOp::And if zero(left) || zero(right) => Some(BoundExpr::Integer(0)),
        BinaryOp::Or if non_zero(left) || non_zero(right) => Some(BoundExpr::Integer(1)),
        _ => None,
    }
}

/// Reports whether a qualifier names a source.
///
/// **`sqlite_master` and `sqlite_schema` are one table, and either name qualifies a column of
/// it.** `SELECT sqlite_master.name FROM sqlite_schema` is a valid statement; so are the two
/// names for the temporary catalog.
///
/// @param source - the FROM term
/// @param qualifier - the table name written before a column
pub(super) fn source_answers_to(source: &BoundSource, qualifier: &[u8]) -> bool {
    if source.alias.eq_ignore_ascii_case(qualifier) {
        return true;
    }
    let catalog_name = |name: &[u8]| {
        let folded = name.to_ascii_lowercase();
        matches!(
            folded.as_slice(),
            b"sqlite_master" | b"sqlite_schema" | b"sqlite_temp_master" | b"sqlite_temp_schema"
        )
    };
    let stem = |name: &[u8]| name.to_ascii_lowercase().starts_with(b"sqlite_temp_");
    catalog_name(&source.alias) && catalog_name(qualifier) && stem(&source.alias) == stem(qualifier)
}

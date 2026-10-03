//! Which statements read nothing from the file, and so take no file lock.
//!
//! Invariant: **a statement this module calls lockless reads no page, no
//! catalog row and no module, in any arm, subquery or function it holds.**
//! SQLite emits no `OP_Transaction` for such a statement, so `SELECT 1` and
//! `SELECT json_extract(?1, '$.a')` take no lock there. Here every statement
//! took the shared lock under `locking_mode = normal` and released it, which
//! on Windows is seven system calls, about 13 microseconds against a
//! statement that costs well under one (task-2175). Skipping the lock is only
//! right while the statement cannot see the file, so the test below is an
//! allow list: a source, a subquery, a column, a module function, a
//! registered function or a window makes the answer `false`.

use inillucent_sql::bind::{BoundExpr, BoundSelect};
use inillucent_sql::plan::PhysicalPlan;

use crate::plans::Cached;

/// Reports whether a compiled statement reads nothing from the database file.
///
/// @param cached - the compiled statement
pub(crate) fn reads_no_file(cached: &Cached) -> bool {
    match cached {
        Cached::Select(plan, ..) => plan_reads_nothing(plan),
        _ => false,
    }
}

/// Reports whether a planned query, and every compound arm of it, reads nothing.
///
/// @param plan - the planned query
fn plan_reads_nothing(plan: &PhysicalPlan) -> bool {
    plan.select.sources.is_empty()
        && !plan.subqueries
        && select_reads_nothing(&plan.select)
        && plan
            .compounds
            .iter()
            .all(|(_, arm)| plan_reads_nothing(arm))
}

/// Reports whether every expression a bound query holds is built from the allow list.
///
/// @param select - the bound query
fn select_reads_nothing(select: &BoundSelect) -> bool {
    select.sources.is_empty()
        && select.windows.is_empty()
        && select.correlations.is_empty()
        && select.shared.is_none()
        && select.columns.iter().all(|column| pure(&column.expr))
        && select.filter.as_ref().is_none_or(pure)
        && select.having.as_ref().is_none_or(pure)
        && select.group_by.iter().all(pure)
        && select.order_by.iter().all(|term| pure(&term.expr))
        && select.limit.as_ref().is_none_or(pure)
        && select.offset.as_ref().is_none_or(pure)
        && select.values.iter().flatten().all(pure)
        && select
            .aggregates
            .iter()
            .all(|aggregate| aggregate.external.is_none() && aggregate.arguments.iter().all(pure))
        && select
            .compounds
            .iter()
            .all(|(_, arm)| select_reads_nothing(arm))
}

/// Reports whether an expression, and everything under it, is computed from its own values alone.
///
/// The built-in scalar, date, math and JSON functions qualify: none of them
/// reads the file. `changes()` and `last_insert_rowid()` read the connection's
/// counters, which are in memory.
///
/// @param expr - the expression
fn pure(expr: &BoundExpr) -> bool {
    let allowed = matches!(
        expr,
        BoundExpr::Null
            | BoundExpr::Integer(_)
            | BoundExpr::Real(_)
            | BoundExpr::Text(_)
            | BoundExpr::Blob(_)
            | BoundExpr::Parameter(_)
            | BoundExpr::Unary { .. }
            | BoundExpr::Arithmetic { .. }
            | BoundExpr::Compare { .. }
            | BoundExpr::And(..)
            | BoundExpr::Or(..)
            | BoundExpr::Not(_)
            | BoundExpr::IsNull { .. }
            | BoundExpr::Is { .. }
            | BoundExpr::Between { .. }
            | BoundExpr::InList { .. }
            | BoundExpr::Case { .. }
            | BoundExpr::Cast { .. }
            | BoundExpr::Collate { .. }
            | BoundExpr::Pattern { .. }
            | BoundExpr::Time { .. }
            | BoundExpr::Math { .. }
            | BoundExpr::Json { .. }
            | BoundExpr::Function { .. }
            | BoundExpr::Aggregate { .. }
    );
    allowed && expr.children().into_iter().all(pure)
}

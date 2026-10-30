//! Answering `min()` and `max()` from one entry of an index.
//!
//! Invariant: **a plan returned from here reads the entries of one walk in the order `ORDER BY
//! argument` would, passes the first row whose argument is not NULL to the aggregate and stops.**
//! The walk is chosen by planning the statement `SELECT argument ... ORDER BY argument LIMIT 1`
//! over the same term, so it is an index only where the planner already answers that order from an
//! index, in the direction, collation and NULL placement the aggregate needs. Where it would sort,
//! the statement keeps its ordinary plan.
//!
//! **Why.** `SELECT min(json_extract(p, '$.k')) FROM t` over an index on that expression scanned
//! every row and evaluated the function on each, 42 ms on 200,000 rows against SQLite's 0.06 ms
//! (bug hunt 11). SQLite reads one end of the index.
//!
//! **Ties.** A walk of an index visits equal values in rowid order, and a backward walk visits them
//! in reverse rowid order. SQLite's own seek to the end of an index has the same property, so
//! `max()` over values that compare equal but differ (`a` and `A` under NOCASE, `1` and `1.0`)
//! returns the entry SQLite returns when the index exists.
//!
//! Here rather than in [`super`] because `plan.rs` is at its recorded size.

use super::{
    covering, covering_slots, plan_block, AccessPath, AggregationMode, BoundExpr, BoundSelect,
    ColumnUse, Levers, PhysicalPlan, SourceRows,
};
use crate::ast::{NullOrder, SortOrder};
use crate::bind::BoundOrderTerm;
use inillucent_value::Collation;

/// What a statement that is one `min()` or one `max()` asks for.
struct Extreme {
    /// Whether it is `min()`, which walks the index forwards.
    ascending: bool,
    /// The aggregate's argument.
    argument: BoundExpr,
    /// The collation the aggregate compares with.
    collation: Collation,
}

/// Returns the plan that reads one entry for a `min()` or `max()` statement, or the plan it was
/// given when no walk answers the order.
///
/// @param plan - the statement's ordinary plan
/// @param levers - which optimizations are on
pub(super) fn answered_from_one_entry(plan: PhysicalPlan, levers: Levers) -> PhysicalPlan {
    let Some(extreme) = asks_for_one_extreme(&plan, levers) else {
        return plan;
    };
    let found = plan_block(probe_of(&plan.select, &extreme), levers, false);
    match adopt(&plan, found) {
        Some(answer) => answer,
        None => plan,
    }
}

/// Returns what the statement asks for when it is one `min()` or `max()` over one table and
/// nothing else of the table is read.
///
/// A `min()` beside a bare column answers that column from the row the extreme came from, so such
/// a statement is left alone, as is one with a `GROUP BY`, a `DISTINCT` aggregate, a `FILTER`, a
/// join, a window or an `INDEXED BY` or `NOT INDEXED` written on the table.
///
/// @param plan - the statement's ordinary plan
/// @param levers - which optimizations are on
fn asks_for_one_extreme(plan: &PhysicalPlan, levers: Levers) -> Option<Extreme> {
    let select = &plan.select;
    let [call] = select.aggregates.as_slice() else {
        return None;
    };
    let ascending = match call.func {
        crate::function::AggregateFunc::Min => true,
        crate::function::AggregateFunc::Max => false,
        _ => return None,
    };
    let [argument] = call.arguments.as_slice() else {
        return None;
    };
    let [bound] = select.sources.as_slice() else {
        return None;
    };
    if plan.aggregation != AggregationMode::Whole
        || !levers.has(Levers::ORDERED_WALK)
        || call.distinct
        || call.star
        || call.filter.is_some()
        || call.external.is_some()
        || !call.order_by.is_empty()
        || plan.sources.len() != 1
        || !plan.compounds.is_empty()
        || select.having.is_some()
        || select.distinct
        || !select.windows.is_empty()
        || !select.group_by.is_empty()
        || !select.order_by.is_empty()
        || !select.values.is_empty()
        || !matches!(bound.rows, SourceRows::Table)
        || bound.index_hint != crate::bind::IndexChoice::Any
        || bound.table.module.is_some()
    {
        return None;
    }
    let mut outside = ColumnUse::default();
    for column in &select.columns {
        column.expr.columns_read(bound.id, &mut outside);
    }
    if !outside.columns.is_empty()
        || outside.rowid
        || outside.opaque
        || !outside.functions.is_empty()
    {
        return None;
    }
    Some(Extreme {
        ascending,
        argument: argument.clone(),
        collation: call.collation,
    })
}

/// Returns the statement `SELECT argument ... ORDER BY argument LIMIT 1` over the same term, with
/// the same `WHERE`.
///
/// NULLs are not removed here: the executor drops them before the aggregate, because a `WHERE`
/// term over the argument would have to be answered from the entry too. The NULLs sit at the
/// start of an ascending walk and at the end of a descending one, which is where `min()` and
/// `max()` need them to be passed over or never reached.
///
/// @param select - the aggregate statement
/// @param extreme - what the statement asks for
fn probe_of(select: &BoundSelect, extreme: &Extreme) -> BoundSelect {
    let mut probe = select.clone();
    probe.aggregates.clear();
    probe.columns.truncate(1);
    if let Some(column) = probe.columns.first_mut() {
        column.expr = extreme.argument.clone();
    }
    probe.order_by = vec![BoundOrderTerm {
        expr: extreme.argument.clone(),
        order: if extreme.ascending {
            SortOrder::Ascending
        } else {
            SortOrder::Descending
        },
        nulls: if extreme.ascending {
            NullOrder::First
        } else {
            NullOrder::Last
        },
        collation: extreme.collation,
    }];
    probe.limit = Some(BoundExpr::Integer(1));
    probe.offset = None;
    probe
}

/// Turns the probe's plan into the aggregate's plan, or returns `None` when the probe sorts.
///
/// The walk, its bounds and its direction are the probe's. The statement, the aggregation and the
/// subquery flag are the aggregate's own.
///
/// @param plan - the statement's ordinary plan
/// @param found - the plan of the probe
fn adopt(plan: &PhysicalPlan, found: PhysicalPlan) -> Option<PhysicalPlan> {
    let ordered_path = matches!(
        found.sources.first().map(|source| &source.path),
        Some(
            AccessPath::IndexSeek { .. }
                | AccessPath::TableScan { .. }
                | AccessPath::RowidRange { .. }
                | AccessPath::RowidSeek { .. }
        )
    );
    if found.sources.len() != 1
        || !found.compounds.is_empty()
        || found.needs_sort
        || found.reverse_stream
        || !ordered_path
    {
        return None;
    }
    let mut answer = found;
    answer.select = plan.select.clone();
    answer.aggregation = AggregationMode::Whole;
    answer.needs_sort = false;
    answer.order_constant = false;
    answer.order_by_computed_key = false;
    answer.grouped_walk = false;
    answer.distinct_walk = false;
    answer.subqueries = plan.subqueries;
    answer.first_entry_answers = true;
    cover_through_computed_keys(&mut answer);
    Some(answer)
}

/// Gives the walk the covering entry it has when the statement's reads of a column are all inside
/// expressions the index computes.
///
/// The probe read `p` to evaluate the argument, so its walk was not covering. The executor
/// answers an expression the index computes from the entry, which makes a walk of
/// `json_extract(p, '$.k')` hold everything `min(json_extract(p, '$.k'))` reads.
///
/// @param plan - the aggregate's plan, whose first term is the walk
fn cover_through_computed_keys(plan: &mut PhysicalPlan) {
    if !plan.levers.has(Levers::COVERING_INDEX) {
        return;
    }
    let Some(source) = plan.sources.first_mut() else {
        return;
    };
    let AccessPath::IndexSeek {
        covering: held,
        index_name,
        ..
    } = &mut source.path
    else {
        return;
    };
    if held.is_some() {
        return;
    }
    let Some((at, index)) = source
        .table
        .indexes
        .iter()
        .enumerate()
        .find(|(_, candidate)| candidate.name == *index_name)
    else {
        return;
    };
    let keys: Vec<BoundExpr> = plan
        .select
        .sources
        .iter()
        .find(|term| term.id == source.id)
        .and_then(|term| {
            term.index_exprs
                .iter()
                .find(|computed| computed.position == at)
        })
        .map(|computed| computed.keys.iter().flatten().cloned().collect())
        .unwrap_or_default();
    if keys.is_empty() {
        return;
    }
    let reduced = covering::needed_beyond_computed(&plan.select, source.id, &keys);
    if let Some(slots) = covering_slots(&source.table, index, &reduced, true) {
        *held = Some(slots);
    }
}

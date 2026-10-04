//! The tree `EXPLAIN QUERY PLAN` prints.
//!
//! Invariant: **each line sits under the line SQLite 3.53.4 puts it under, and
//! the nodes are SQLite's.** A derived table SQLite runs as a co-routine is a
//! `CO-ROUTINE` node holding its own plan, one it fills once is a
//! `MATERIALIZE` node, a `UNION ALL` is a `COMPOUND QUERY` with one child per
//! arm, the other compound operators are a `MERGE` of a `LEFT` and a `RIGHT`
//! side each sorted on the result columns, and an expression subquery is a
//! `SCALAR SUBQUERY` or `LIST SUBQUERY` node numbered the way SQLite numbers
//! its `Select`s. The lines inside a node are the ones `describe` writes.
//!
//! A shell draws the tree from each line's parent. The flat list this replaced
//! drew every line at the top level, so the three corpus cases with a derived
//! table differed from SQLite in their plan text while their rows matched.

use super::describe;
use super::*;
use crate::bind::{BoundOrderTerm, SubqueryKind};

/// One line of `EXPLAIN QUERY PLAN`, and how deep in the tree it sits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanLine {
    /// How many nodes are above it; zero is a line at the top level.
    pub depth: u16,
    /// The text, such as `SCAN t` or `CO-ROUTINE x`.
    pub detail: String,
}

/// Appends one line.
///
/// @param out - the lines so far
/// @param depth - its depth
/// @param detail - its text
fn push(out: &mut Vec<PlanLine>, depth: u16, detail: impl Into<String>) {
    out.push(PlanLine {
        depth,
        detail: detail.into(),
    });
}

/// Appends the lines of one plan at a depth.
///
/// @param plan - the plan
/// @param depth - the depth of its top level lines
/// @param out - the lines so far
pub(super) fn tree_of(plan: &PhysicalPlan, depth: u16, out: &mut Vec<PlanLine>) {
    if !plan.compounds.is_empty() {
        compound_tree(plan, depth, out);
        return;
    }
    if !plan.select.windows.is_empty() {
        window_tree(plan, depth, out);
        return;
    }
    select_tree(plan, depth, out);
}

/// Appends the lines of one plan that is not a compound: its derived tables,
/// its loops, its expression subqueries and its sorters, in that order.
///
/// @param plan - the plan
/// @param depth - the depth of its top level lines
/// @param out - the lines so far
fn select_tree(plan: &PhysicalPlan, depth: u16, out: &mut Vec<PlanLine>) {
    if let Some(inner) = flattened_compound(plan) {
        tree_of(inner, depth, out);
        return;
    }
    derived_nodes(plan, depth, out);
    if plan.sources.is_empty() {
        let rows = plan.select.values.len();
        if rows > 1 {
            push(out, depth, format!("SCAN {rows}-ROW VALUES CLAUSE"));
        } else {
            push(out, depth, "SCAN CONSTANT ROW");
        }
    }
    let extreme = min_or_max_search(plan);
    for (position, line) in describe::loop_lines(plan).into_iter().enumerate() {
        match (&extreme, position) {
            (Some(search), 0) => push(out, depth, search.clone()),
            _ => push(out, depth, line),
        }
    }
    expression_subqueries(plan, depth, out);
    for line in describe::temp_lines(plan) {
        push(out, depth, line);
    }
}

/// Returns the plan of a `UNION ALL` derived table SQLite flattens into the
/// statement, which then has no node of its own.
///
/// SQLite's compound flattening (restriction 17) turns `SELECT * FROM (SELECT
/// .. UNION ALL SELECT ..) WHERE a = 7` into a compound of the arms, each with
/// the `WHERE` in it. This engine fills the derived table instead, with the
/// `WHERE` pushed into each arm, so the arms' plans are the ones SQLite prints.
/// It is done when the derived table is the only FROM term, every operator is
/// `UNION ALL`, no arm aggregates, is `DISTINCT` or has a window, the derived
/// table has no `ORDER BY` or `LIMIT`, and the statement neither aggregates,
/// sorts, limits, is `DISTINCT` nor has a window.
///
/// @param plan - the statement's plan
fn flattened_compound(plan: &PhysicalPlan) -> Option<&PhysicalPlan> {
    let select = &plan.select;
    let [only] = select.sources.as_slice() else {
        return None;
    };
    let SourceRows::Subquery(block) = &only.rows else {
        return None;
    };
    let simple = |arm: &BoundSelect| {
        arm.aggregates.is_empty()
            && arm.group_by.is_empty()
            && !arm.distinct
            && arm.windows.is_empty()
            && !arm.sources.is_empty()
    };
    if block.compounds.is_empty()
        || !block
            .compounds
            .iter()
            .all(|(op, arm)| *op == CompoundOp::UnionAll && simple(arm))
        || !simple(block)
        || !block.order_by.is_empty()
        || block.limit.is_some()
        || only.derived.materialized == Some(true)
        || !select.aggregates.is_empty()
        || !select.group_by.is_empty()
        || select.distinct
        || !select.windows.is_empty()
        || !select.order_by.is_empty()
        || select.limit.is_some()
    {
        return None;
    }
    match &plan.sources.first()?.path {
        AccessPath::Subquery { plan: inner, .. } => Some(inner),
        _ => None,
    }
}

/// Returns the loop line SQLite prints for a statement that is one `min()` or
/// one `max()` over one table, which it answers by seeking the first or last
/// entry rather than scanning.
///
/// The line is `SEARCH t USING COVERING INDEX i` when an index starts with the
/// argument column and holds every column the statement reads, `SEARCH t USING
/// INDEX i` when it starts with it and does not, and `SEARCH t` otherwise.
///
/// @param plan - the statement's plan
fn min_or_max_search(plan: &PhysicalPlan) -> Option<String> {
    let select = &plan.select;
    let [aggregate] = select.aggregates.as_slice() else {
        return None;
    };
    let extreme = matches!(
        aggregate.func,
        crate::function::AggregateFunc::Min | crate::function::AggregateFunc::Max
    );
    let [only] = plan.sources.as_slice() else {
        return None;
    };
    let unbounded = match &only.path {
        AccessPath::TableScan { .. } => true,
        AccessPath::IndexSeek {
            equalities,
            low,
            high,
            ..
        } => equalities.is_empty() && low.is_none() && high.is_none(),
        _ => false,
    };
    if !extreme || !unbounded || !select.group_by.is_empty() || aggregate.distinct {
        return None;
    }
    let [argument] = aggregate.arguments.as_slice() else {
        return None;
    };
    let bound = select.sources.first()?;
    let name = loop_name(bound);
    let column = match argument {
        BoundExpr::Column { source, column, .. } if *source == only.id => *column,
        _ => return Some(format!("SEARCH {name}")),
    };
    if only.table.rowid_alias == Some(column) {
        return Some(format!("SEARCH {name}"));
    }
    let Some(index) = only.table.indexes.iter().find(|index| {
        index
            .columns
            .first()
            .is_some_and(|key| key.column == Some(column))
            && index.partial_sql.is_none()
            && index.metric.is_none()
    }) else {
        return Some(format!("SEARCH {name}"));
    };
    let reads = select.columns_read(only.id);
    let covering = !reads.opaque
        && reads.columns.iter().all(|read| {
            Some(*read) == only.table.rowid_alias
                || index.columns.iter().any(|key| key.column == Some(*read))
        });
    let kind = if covering { "COVERING INDEX" } else { "INDEX" };
    Some(format!(
        "SEARCH {name} USING {kind} {}",
        String::from_utf8_lossy(&index.name)
    ))
}

/// Returns the name SQLite gives a FROM term in a node: the name of the CTE or
/// view it reads, else its alias, else `(subquery-N)`.
///
/// This is SQLite's `%!S`, which prefers the name to the alias. A loop line
/// uses `%S`, which prefers the alias; see [`loop_name`].
///
/// @param source - the bound FROM term
fn node_name(source: &BoundSource) -> String {
    if (source.derived.cte || source.derived.view) && !source.derived.name.is_empty() {
        return String::from_utf8_lossy(&source.derived.name).into_owned();
    }
    loop_name(source)
}

/// Returns the name SQLite gives a FROM term in a loop line: its alias, else
/// `(subquery-N)` for a derived table written with no alias.
///
/// @param source - the bound FROM term
pub(super) fn loop_name(source: &BoundSource) -> String {
    if let Some(rows) = values_rows(source) {
        return format!("{rows}-ROW VALUES CLAUSE");
    }
    if source.derived.anonymous {
        if let SourceRows::Subquery(block) = &source.rows {
            return format!("(subquery-{})", last_serial(block));
        }
    }
    String::from_utf8_lossy(&source.alias).into_owned()
}

/// Returns how many rows a derived table of several `VALUES` rows has.
///
/// SQLite reads such a table straight from its rows, with no node of its own,
/// and names the loop `SCAN 2-ROW VALUES CLAUSE`.
///
/// @param source - the bound FROM term
fn values_rows(source: &BoundSource) -> Option<usize> {
    match &source.rows {
        SourceRows::Subquery(block) if block.values.len() > 1 => Some(block.values.len()),
        _ => None,
    }
}

/// Returns SQLite's number for a block: its last arm's, because SQLite's
/// `Select` for a compound is the rightmost one.
///
/// @param block - the block
pub(super) fn last_serial(block: &BoundSelect) -> u32 {
    block
        .compounds
        .last()
        .map_or(block.serial, |(_, arm)| arm.serial)
}

/// Reports whether SQLite runs the derived table at one FROM position as a
/// co-routine rather than filling a table with it, as its
/// `fromClauseTermCanBeCoroutine` decides.
///
/// A `MATERIALIZED` CTE, and a CTE used more than once that is not `NOT
/// MATERIALIZED`, are filled. Otherwise the first term is a co-routine, and a
/// later one is when neither it nor any term before it is joined by `LEFT` or
/// `CROSS` and no term before it is a derived table.
///
/// @param select - the statement
/// @param position - the FROM position
pub(super) fn runs_as_coroutine(select: &BoundSelect, position: usize) -> bool {
    let Some(source) = select.sources.get(position) else {
        return false;
    };
    let note = &source.derived;
    if note.cte
        && (note.materialized == Some(true) || (note.uses >= 2 && note.materialized != Some(false)))
    {
        return false;
    }
    if select
        .sources
        .first()
        .is_some_and(|first| matches!(first.join, JoinKind::Right | JoinKind::Full))
    {
        return false;
    }
    if position == 0 {
        return true;
    }
    let mut at = position;
    loop {
        let Some(held) = select.sources.get(at) else {
            return false;
        };
        if matches!(
            held.join,
            JoinKind::Left | JoinKind::Full | JoinKind::Right | JoinKind::Cross
        ) {
            return false;
        }
        if at == 0 {
            return true;
        }
        at -= 1;
        if select
            .sources
            .get(at)
            .is_some_and(|before| !matches!(before.rows, SourceRows::Table))
        {
            return false;
        }
    }
}

/// Appends a `CO-ROUTINE` or `MATERIALIZE` node for each derived table, in
/// FROM order, each holding the derived table's own plan.
///
/// A CTE that is filled once for several references is named once, at its
/// first reference.
///
/// @param plan - the plan
/// @param depth - the depth of the nodes
/// @param out - the lines so far
fn derived_nodes(plan: &PhysicalPlan, depth: u16, out: &mut Vec<PlanLine>) {
    let mut filled: Vec<String> = Vec::new();
    for (position, bound) in plan.select.sources.iter().enumerate() {
        let Some(planned) = plan.sources.iter().find(|held| held.id == bound.id) else {
            continue;
        };
        if values_rows(bound).is_some() {
            continue;
        }
        let coroutine = runs_as_coroutine(&plan.select, position);
        let name = node_name(bound);
        if bound.derived.cte && !coroutine {
            if filled.contains(&name) {
                continue;
            }
            filled.push(name.clone());
        }
        let kind = if coroutine {
            "CO-ROUTINE"
        } else {
            "MATERIALIZE"
        };
        match &planned.path {
            AccessPath::Subquery { plan: inner, .. } => {
                push(out, depth, format!("{kind} {name}"));
                tree_of(inner, depth + 1, out);
            }
            AccessPath::Recursive { seeds, steps, .. } => {
                push(out, depth, format!("{kind} {name}"));
                push(out, depth + 1, "SETUP");
                for (_, seed) in seeds {
                    tree_of(seed, depth + 2, out);
                }
                push(out, depth + 1, "RECURSIVE STEP");
                for (_, step) in steps {
                    tree_of(step, depth + 2, out);
                }
            }
            _ => {}
        }
    }
}

/// Appends a node for each subquery the statement uses as a value, after its
/// loops: the `WHERE`'s first, then the result columns', then the rest.
///
/// `LIST SUBQUERY` for an `IN`, `SCALAR SUBQUERY` for the others, prefixed
/// `CORRELATED` when the subquery reads the statement. Each holds the
/// subquery's own plan.
///
/// @param plan - the plan
/// @param depth - the depth of the nodes
/// @param out - the lines so far
fn expression_subqueries(plan: &PhysicalPlan, depth: u16, out: &mut Vec<PlanLine>) {
    let select = &plan.select;
    let mut roots: Vec<&BoundExpr> = Vec::new();
    roots.extend(select.columns.iter().map(|column| &column.expr));
    roots.extend(select.group_by.iter());
    roots.extend(select.having.iter());
    roots.extend(select.order_by.iter().map(|term| &term.expr));
    subquery_nodes_into(select.filter.as_ref(), &roots, plan.levers, depth, out);
}

/// Returns the nodes for the subqueries a write uses as values: those in its
/// `WHERE`, then those in its other expressions, at the top level.
///
/// @param filter - the write's `WHERE`
/// @param others - its other expressions, such as an `UPDATE`'s new values
/// @param levers - which optimizations are on
pub fn subquery_nodes(
    filter: Option<&BoundExpr>,
    others: &[&BoundExpr],
    levers: Levers,
) -> Vec<PlanLine> {
    let mut out = Vec::new();
    subquery_nodes_into(filter, others, levers, 0, &mut out);
    out
}

/// Appends a node for each subquery in a `WHERE` and then in other expressions.
///
/// An `IN` subquery in the `WHERE` tests each row against the subquery's rows,
/// and SQLite builds a bloom filter over them for that, which its node shows
/// as a last child. SQLite seeks an index by an `IN` subquery where it can and
/// builds no filter then; this engine does not seek by one.
///
/// @param filter - the `WHERE`
/// @param others - the other expressions
/// @param levers - which optimizations are on
/// @param depth - the depth of the nodes
/// @param out - the lines so far
fn subquery_nodes_into(
    filter: Option<&BoundExpr>,
    others: &[&BoundExpr],
    levers: Levers,
    depth: u16,
    out: &mut Vec<PlanLine>,
) {
    let mut filtering: Vec<&BoundExpr> = Vec::new();
    if let Some(filter) = filter {
        subqueries_in(filter, &mut filtering);
    }
    let in_filter: Vec<usize> = filtering
        .iter()
        .filter_map(|expr| match expr {
            BoundExpr::Subquery { id, .. } => Some(*id),
            _ => None,
        })
        .collect();
    let mut found = filtering;
    for root in others {
        subqueries_in(root, &mut found);
    }
    let mut seen: Vec<usize> = Vec::new();
    for expr in found {
        let BoundExpr::Subquery {
            id, kind, block, ..
        } = expr
        else {
            continue;
        };
        if seen.contains(id) {
            continue;
        }
        seen.push(*id);
        let correlated = if block.correlations.is_empty() {
            ""
        } else {
            "CORRELATED "
        };
        let noun = match kind {
            SubqueryKind::In => "LIST",
            _ => "SCALAR",
        };
        push(
            out,
            depth,
            format!("{correlated}{noun} SUBQUERY {}", last_serial(block)),
        );
        let inner = plan_select_with((**block).clone(), levers);
        tree_of(&inner, depth + 1, out);
        if *kind == SubqueryKind::In && in_filter.contains(id) {
            push(out, depth + 1, "CREATE BLOOM FILTER");
        }
    }
}

/// Collects the subqueries an expression holds, without looking inside them.
///
/// @param expr - the expression
/// @param found - where they are collected
fn subqueries_in<'e>(expr: &'e BoundExpr, found: &mut Vec<&'e BoundExpr>) {
    if matches!(expr, BoundExpr::Subquery { .. }) {
        found.push(expr);
        if let BoundExpr::Subquery {
            operand: Some(operand),
            ..
        } = expr
        {
            subqueries_in(operand, found);
        }
        return;
    }
    for child in expr.children() {
        subqueries_in(child, found);
    }
}

/// Returns the word `EXPLAIN QUERY PLAN` names a compound operator by.
///
/// @param op - the operator
fn operator_name(op: CompoundOp) -> &'static str {
    match op {
        CompoundOp::Union => "UNION",
        CompoundOp::UnionAll => "UNION ALL",
        CompoundOp::Intersect => "INTERSECT",
        CompoundOp::Except => "EXCEPT",
    }
}

/// Returns the plan of a compound's first arm on its own, without the
/// compound's ordering, as `run_arm` runs it.
///
/// @param plan - the compound's plan
fn first_arm(plan: &PhysicalPlan) -> PhysicalPlan {
    let mut arm = plan.clone();
    arm.compounds.clear();
    arm.needs_sort = false;
    arm.reverse = false;
    arm.select.order_by.clear();
    arm.select.limit = None;
    arm.select.offset = None;
    arm
}

/// Appends a compound's lines.
///
/// A `UNION ALL` with no `ORDER BY` runs its arms one after another under a
/// `COMPOUND QUERY` node. Anything else is merged: SQLite sorts each side on
/// the result columns and walks the two sorted lists together, which
/// `merge_tree` describes.
///
/// @param plan - the compound's plan, whose first arm is the plan itself
/// @param depth - the depth of the top node
/// @param out - the lines so far
fn compound_tree(plan: &PhysicalPlan, depth: u16, out: &mut Vec<PlanLine>) {
    let ordered = !plan.select.order_by.is_empty();
    if !ordered
        && plan
            .compounds
            .iter()
            .all(|(op, _)| *op == CompoundOp::UnionAll)
    {
        push(out, depth, "COMPOUND QUERY");
        push(out, depth + 1, "LEFT-MOST SUBQUERY");
        tree_of(&first_arm(plan), depth + 2, out);
        for (op, arm) in &plan.compounds {
            push(out, depth + 1, operator_name(*op));
            tree_of(arm, depth + 2, out);
        }
        return;
    }
    let mut arms: Vec<(Option<CompoundOp>, BoundSelect)> = vec![(None, first_arm(plan).select)];
    for (op, arm) in &plan.compounds {
        arms.push((Some(*op), arm.select.clone()));
    }
    merge_tree(&arms, &plan.select.order_by, plan.levers, depth, out);
}

/// Appends the `MERGE` of a compound's arms.
///
/// The last arm is the right side, and everything before it is the left side,
/// itself a merge when it has more than one arm. Each side is sorted on the
/// compound's `ORDER BY` followed by the result columns it does not name,
/// which is the order SQLite's `multiSelectOrderBy` merges in.
///
/// @param arms - the arms, each with the operator joining it to the ones before
/// @param order - the compound's `ORDER BY`
/// @param levers - which optimizations are on
/// @param depth - the depth of the `MERGE` node
/// @param out - the lines so far
fn merge_tree(
    arms: &[(Option<CompoundOp>, BoundSelect)],
    order: &[BoundOrderTerm],
    levers: Levers,
    depth: u16,
    out: &mut Vec<PlanLine>,
) {
    let Some(((op, last), before)) = arms.split_last() else {
        return;
    };
    let op = op.unwrap_or(CompoundOp::UnionAll);
    push(out, depth, format!("MERGE ({})", operator_name(op)));
    push(out, depth + 1, "LEFT");
    match before {
        [(_, only)] => sorted_arm(only, order, levers, depth + 2, out),
        _ => merge_tree(before, order, levers, depth + 2, out),
    }
    push(out, depth + 1, "RIGHT");
    sorted_arm(last, order, levers, depth + 2, out);
}

/// Appends the lines of one arm of a merge, planned as if it had the merge's
/// ordering, so a sorter appears when its loops do not give that order.
///
/// @param arm - the arm
/// @param order - the compound's `ORDER BY`, by result column
/// @param levers - which optimizations are on
/// @param depth - the depth of its lines
/// @param out - the lines so far
fn sorted_arm(
    arm: &BoundSelect,
    order: &[BoundOrderTerm],
    levers: Levers,
    depth: u16,
    out: &mut Vec<PlanLine>,
) {
    let mut sorted = arm.clone();
    let mut terms: Vec<BoundOrderTerm> = Vec::new();
    let mut named: Vec<usize> = Vec::new();
    for term in order {
        let BoundExpr::SorterColumn { column } = term.expr else {
            continue;
        };
        let Some(result) = arm.columns.get(usize::from(column)) else {
            continue;
        };
        named.push(usize::from(column));
        terms.push(BoundOrderTerm {
            expr: result.expr.clone(),
            ..term.clone()
        });
    }
    for (position, result) in arm.columns.iter().enumerate() {
        if named.contains(&position) {
            continue;
        }
        terms.push(BoundOrderTerm {
            expr: result.expr.clone(),
            order: crate::ast::SortOrder::Ascending,
            nulls: crate::ast::NullOrder::First,
            collation: Collation::Binary,
        });
    }
    sorted.order_by = terms;
    let planned = plan_select_with(sorted, levers);
    select_tree(&planned, depth, out);
}

/// Appends the lines of a statement with a window function.
///
/// SQLite rewrites it into a co-routine over the statement's rows, sorted into
/// the window's order, and scans that. The co-routine is numbered after every
/// `Select` the parser wrote.
///
/// @param plan - the plan
/// @param depth - the depth of the top lines
/// @param out - the lines so far
fn window_tree(plan: &PhysicalPlan, depth: u16, out: &mut Vec<PlanLine>) {
    let number = highest_serial(&plan.select).saturating_add(1);
    let name = format!("(subquery-{number})");
    push(out, depth, format!("CO-ROUTINE {name}"));
    derived_nodes(plan, depth + 1, out);
    for line in describe::loop_lines(plan) {
        push(out, depth + 1, line);
    }
    let sorts = plan
        .select
        .windows
        .first()
        .is_some_and(|window| !window.partition_by.is_empty() || !window.order_by.is_empty());
    if sorts {
        push(out, depth + 1, "USE TEMP B-TREE FOR ORDER BY");
    }
    push(out, depth, format!("SCAN {name}"));
    let same_order = plan.select.windows.first().is_some_and(|window| {
        window.partition_by.is_empty()
            && window.order_by.len() == plan.select.order_by.len()
            && window
                .order_by
                .iter()
                .zip(&plan.select.order_by)
                .all(|(left, right)| left.expr == right.expr && left.order == right.order)
    });
    if !plan.select.order_by.is_empty() && !same_order {
        push(out, depth, "USE TEMP B-TREE FOR ORDER BY");
    }
}

/// Returns the largest SQLite number of any block in a statement.
///
/// @param select - the statement
fn highest_serial(select: &BoundSelect) -> u32 {
    let mut highest = select.serial;
    for source in &select.sources {
        if let SourceRows::Subquery(block) = &source.rows {
            highest = highest.max(highest_serial(block));
        }
    }
    for (_, arm) in &select.compounds {
        highest = highest.max(highest_serial(arm));
    }
    let mut probe = select.clone();
    crate::rewrite::rewrite_select(&mut probe, &mut |expr: &mut BoundExpr| {
        if let Some(block) = expr.block_mut() {
            highest = highest.max(block.serial);
        }
    });
    highest
}

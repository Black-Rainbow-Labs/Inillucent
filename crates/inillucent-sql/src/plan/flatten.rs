//! Flattening a derived table into the query that reads it, and simplifying a
//! `LEFT JOIN` the `WHERE` makes an inner join.
//!
//! Invariant: **a derived table is flattened only where SQLite 3.53.4's
//! `flattenSubquery` flattens it, and the flattened query returns the same rows
//! the derived table did.** Flattening is not only faster; it is visible. A
//! derived table SQLite flattens reads its tables again for every execution of
//! a correlated subquery that holds it, where one it materializes is read once,
//! and `EXPLAIN QUERY PLAN` names the base tables of a flattened one with the
//! index the query uses on them.
//!
//! The rules followed are SQLite's numbered restrictions, with three kept
//! narrower here than SQLite's because this engine has no `TK_IF_NULL_ROW`:
//!
//! - The right side of a `LEFT JOIN` is flattened only when every result
//!   column of the derived table is a column of its one table, which a null
//!   extended row reads as NULL. SQLite also flattens `SELECT 1 AS c FROM u`
//!   there, by wrapping `1` in an expression that is NULL on a null extended row.
//! - A derived table whose result columns hold a correlated subquery is not
//!   flattened, because flattening copies the subquery into every place the
//!   column is read.
//! - A compound derived table is not flattened. `EXPLAIN QUERY PLAN` still
//!   describes a `UNION ALL` one the way SQLite plans it; see `describe`.

use super::*;

/// Flattens every derived table SQLite would flatten, after turning each
/// `LEFT JOIN` the `WHERE` makes an inner join into one.
///
/// A flattened derived table's own derived tables become terms of this query
/// and are considered in turn, which is why the walk starts again after each.
///
/// @param select - the statement being planned
/// @param in_compound - whether the statement is an arm of a compound, whose
///   `ORDER BY` and `LIMIT` belong to the compound
pub(super) fn flatten_derived_tables(select: &mut BoundSelect, in_compound: bool) {
    if !select
        .sources
        .iter()
        .any(|source| matches!(source.rows, SourceRows::Subquery(_)) || is_outer(source.join))
    {
        return;
    }
    simplify_left_joins(select);
    let mut position = 0;
    let mut steps = 0usize;
    while position < select.sources.len() && steps < 64 {
        if can_flatten(select, position, in_compound) {
            inline(select, position, in_compound);
            position = 0;
            steps += 1;
        } else {
            position += 1;
        }
    }
    // A flattened derived table's joins are this query's now, and its `WHERE`
    // may hold the term that keeps one of them from null extending.
    if steps > 0 {
        simplify_left_joins(select);
    }
}

/// Turns each outer join whose null extended side the `WHERE` cannot leave
/// null into a join that does not null extend it, as SQLite's join
/// simplification does.
///
/// `t LEFT JOIN u ON ... WHERE u.a = 5` keeps no null extended row, because
/// `NULL = 5` is not true, so the join is an inner join and the planner may
/// start from `u`. A term before a `RIGHT` or `FULL` join is the side that join
/// null extends: `s RIGHT JOIN r ON ... WHERE s.k = 1` is an inner join, and a
/// `FULL` join after `s` becomes a `LEFT` one. A `FULL` join whose own term the
/// `WHERE` keeps becomes a `RIGHT` one. SQLite visits the terms in FROM order,
/// and so does this.
///
/// @param select - the statement being planned
fn simplify_left_joins(select: &mut BoundSelect) {
    let Some(filter) = filter_without_table_arguments(select) else {
        return;
    };
    for position in 0..select.sources.len() {
        let Some(source) = select.sources.get(position) else {
            continue;
        };
        let (id, join) = (source.id, source.join);
        let before_a_right_join = select
            .sources
            .iter()
            .skip(position.saturating_add(1))
            .any(|later| matches!(later.join, JoinKind::Right | JoinKind::Full));
        let extendable = matches!(join, JoinKind::Left | JoinKind::Full) || before_a_right_join;
        if !extendable || !implies_non_null_row(&filter, id) {
            continue;
        }
        if let Some(source) = select.sources.get_mut(position) {
            source.join = match source.join {
                JoinKind::Left => JoinKind::Inner,
                JoinKind::Full => JoinKind::Right,
                other => other,
            };
        }
        if before_a_right_join {
            for later in select.sources.iter_mut().skip(position.saturating_add(1)) {
                later.join = match later.join {
                    JoinKind::Right => JoinKind::Inner,
                    JoinKind::Full => JoinKind::Left,
                    other => other,
                };
            }
        }
    }
}

/// Returns the `WHERE` without the equalities a table valued function's
/// arguments were bound as, or nothing when no other conjunct is left.
///
/// SQLite keeps `json_each(t.j)`'s argument out of the `WHERE`, so it never
/// makes an outer join an inner one: `t LEFT JOIN json_each(t.j)` over an empty
/// array still null extends `t`, and `json_each('[1]') RIGHT JOIN r` still
/// returns every row of `r`. The binder ANDs the arguments into the `WHERE`.
///
/// @param select - the statement being planned
fn filter_without_table_arguments(select: &BoundSelect) -> Option<BoundExpr> {
    let filter = select.filter.as_ref()?;
    let mut conjuncts = Vec::new();
    split_conjunction(filter, &mut conjuncts);
    conjuncts
        .into_iter()
        .filter(|conjunct| !is_table_argument(select, conjunct))
        .reduce(|left, right| BoundExpr::And(Box::new(left), Box::new(right)))
}

/// Reports whether a conjunct is an equality on a hidden column of a virtual
/// table, which is how the binder writes a table valued function's argument.
///
/// @param select - the statement being planned
/// @param conjunct - one conjunct of the `WHERE`
pub(super) fn is_table_argument(select: &BoundSelect, conjunct: &BoundExpr) -> bool {
    let BoundExpr::Compare {
        op: BinaryOp::Equal,
        left,
        ..
    } = conjunct
    else {
        return false;
    };
    let BoundExpr::Column { source, column, .. } = left.as_ref() else {
        return false;
    };
    select.sources.iter().any(|term| {
        term.id == *source
            && term.table.kind == crate::catalog_view::TableKind::Virtual
            && term
                .table
                .columns
                .get(usize::from(*column))
                .is_some_and(|declared| declared.hidden)
    })
}

/// Reports whether a `WHERE` is false or NULL whenever every column of one
/// FROM term is NULL, as SQLite's `sqlite3ExprImpliesNonNullRow` decides it.
///
/// One conjunct is enough. Within a conjunct, a comparison, `BETWEEN` or `IN`
/// list that reads a column of the term implies it, both sides of an `AND` or
/// an `OR` must, and `IS`, `IS NULL`, `CASE`, a function and a subquery never
/// do, because each can be true of NULL.
///
/// @param filter - the `WHERE`
/// @param id - the FROM term
pub(super) fn implies_non_null_row(filter: &BoundExpr, id: usize) -> bool {
    let mut expr = filter;
    while let BoundExpr::Collate { operand, .. } = expr {
        expr = operand;
    }
    match expr {
        BoundExpr::IsNull {
            negated: true,
            operand,
        } => reads_the_term(operand, id),
        BoundExpr::And(left, right) => {
            implies_non_null_row(left, id) || implies_non_null_row(right, id)
        }
        other => reads_the_term(other, id),
    }
}

/// Reports whether an expression is NULL or false when the term's columns are
/// NULL, by the walk `impliesNotNullRow` makes.
///
/// @param expr - the expression
/// @param id - the FROM term
fn reads_the_term(expr: &BoundExpr, id: usize) -> bool {
    match expr {
        BoundExpr::Column { source, .. } | BoundExpr::Rowid { source } => *source == id,
        BoundExpr::IsNull { .. }
        | BoundExpr::Is { .. }
        | BoundExpr::Case { .. }
        | BoundExpr::Function { .. }
        | BoundExpr::Json { .. }
        | BoundExpr::Math { .. }
        | BoundExpr::Time { .. }
        | BoundExpr::Pattern { .. }
        | BoundExpr::External { .. }
        | BoundExpr::VirtualFunction { .. }
        | BoundExpr::Subquery { .. }
        | BoundExpr::Aggregate { .. }
        | BoundExpr::WindowRef { .. } => false,
        BoundExpr::And(left, right) | BoundExpr::Or(left, right) => {
            reads_the_term(left, id) && reads_the_term(right, id)
        }
        BoundExpr::InList { operand, list, .. } => !list.is_empty() && reads_the_term(operand, id),
        BoundExpr::Between { operand, .. } => reads_the_term(operand, id),
        other => other
            .children()
            .into_iter()
            .any(|child| reads_the_term(child, id)),
    }
}

/// Reports whether the FROM term at one position is a derived table SQLite
/// would flatten into this query.
///
/// @param select - the statement being planned
/// @param position - the term's position in the FROM clause
/// @param in_compound - whether the statement is an arm of a compound
fn can_flatten(select: &BoundSelect, position: usize, in_compound: bool) -> bool {
    let Some(source) = select.sources.get(position) else {
        return false;
    };
    let SourceRows::Subquery(block) = &source.rows else {
        return false;
    };
    if source.derived.materialized == Some(true) || source.derived.pinned {
        return false; // (28)
    }
    let aggregate = !select.aggregates.is_empty() || !select.group_by.is_empty();
    let block_aggregate =
        !block.aggregates.is_empty() || !block.group_by.is_empty() || block.having.is_some();
    if !select.windows.is_empty() || !block.windows.is_empty() // (25)
        || block_aggregate
        || block.distinct // (4)
        || block.sources.is_empty() // (7)
        || !block.values.is_empty()
        || !block.compounds.is_empty()
        || !block.correlations.is_empty()
    {
        return false;
    }
    if !limits_allow(select, block, position, in_compound, aggregate) {
        return false;
    }
    if select
        .sources
        .iter()
        .any(|held| matches!(held.join, JoinKind::Right | JoinKind::Full))
    {
        return false; // (26), (27)
    }
    if source.join == JoinKind::Left && !outer_join_allows(select, block) {
        return false;
    }
    // A derived table that joins with RIGHT or FULL fills its unmatched right
    // rows after the scan, so moving its terms into an enclosing join loses the
    // rows an enclosing equality would have kept.
    if block
        .sources
        .iter()
        .any(|inner| matches!(inner.join, JoinKind::Right | JoinKind::Full))
    {
        return false;
    }
    if block.sources.iter().any(|inner| {
        matches!(
            inner.rows,
            SourceRows::Recursive(_) | SourceRows::RecursiveSelf { .. }
        )
    }) {
        return false; // (22)
    }
    if reads_the_rowid(select, source.id) {
        return false;
    }
    !block
        .columns
        .iter()
        .any(|column| holds_a_correlated_subquery(&column.expr))
}

/// The restrictions on `LIMIT`, `OFFSET` and `ORDER BY`: 8, 9, 11, 13, 14, 15,
/// 16, 19 and 21, and the one in `sqlite3Select` that keeps a derived table
/// with an `ORDER BY` from a statement whose result columns compute something.
///
/// An `ORDER BY` SQLite would drop as superfluous counts as dropped here; see
/// [`drops_its_order_by`].
///
/// @param select - the statement being planned
/// @param block - the derived table's query
/// @param position - its position in the FROM clause
/// @param in_compound - whether the statement is an arm of a compound
/// @param aggregate - whether the statement aggregates
fn limits_allow(
    select: &BoundSelect,
    block: &BoundSelect,
    position: usize,
    in_compound: bool,
    aggregate: bool,
) -> bool {
    let sub_limit = block.limit.is_some();
    let outer_limit = !in_compound && select.limit.is_some();
    let outer_order = !in_compound && !select.order_by.is_empty();
    if block.offset.is_some()
        || (sub_limit && outer_limit)
        || (sub_limit && in_compound)
        || (sub_limit && (select.sources.len() > 1 || aggregate))
        || (sub_limit && select.filter.is_some())
        || (sub_limit && select.distinct)
    {
        return false;
    }
    let sub_order = !block.order_by.is_empty() && !drops_its_order_by(select, block, in_compound);
    // **An arm of a compound keeps a derived table's order.** SQLite moves the
    // derived table's `ORDER BY` onto the arm it flattens into, and runs each
    // arm of a `UNION ALL` in that order. This engine's arms carry no
    // `ORDER BY` of their own, so flattening one in dropped it:
    // `SELECT * FROM (SELECT a FROM t ORDER BY a DESC) UNION ALL SELECT 100`
    // came back in table order. Not flattening keeps the order.
    if sub_order && (outer_order || aggregate || in_compound) {
        return false;
    }
    let computes = select
        .columns
        .iter()
        .any(|column| !matches!(column.expr, BoundExpr::Column { .. }));
    let alone = select.sources.len() == 1
        || select
            .sources
            .get(1)
            .is_some_and(|next| matches!(next.join, JoinKind::Left | JoinKind::Cross));
    !(sub_order && position == 0 && computes && alone)
}

/// Reports whether SQLite drops a derived table's `ORDER BY` as superfluous.
///
/// It does when the statement has its own `ORDER BY` or more than one FROM
/// term, the derived table has no `LIMIT`, and the statement does not call an
/// aggregate whose answer depends on the order, which is every aggregate but
/// `count`, `min` and `max`.
///
/// @param select - the statement being planned
/// @param block - the derived table's query
/// @param in_compound - whether the statement is an arm of a compound
fn drops_its_order_by(select: &BoundSelect, block: &BoundSelect, in_compound: bool) -> bool {
    let outer_order = !in_compound && !select.order_by.is_empty();
    let order_matters = select.aggregates.iter().any(|aggregate| {
        !matches!(
            aggregate.func,
            crate::function::AggregateFunc::Count
                | crate::function::AggregateFunc::Min
                | crate::function::AggregateFunc::Max
        )
    });
    (outer_order || select.sources.len() > 1) && block.limit.is_none() && !order_matters
}

/// The restrictions on the right side of a `LEFT JOIN`: one table (3a), a
/// statement that is not `DISTINCT` (3d), and result columns that are columns
/// of that table, which is this engine's own restriction.
///
/// @param select - the statement being planned
/// @param block - the derived table's query
fn outer_join_allows(select: &BoundSelect, block: &BoundSelect) -> bool {
    let [only] = block.sources.as_slice() else {
        return false;
    };
    !select.distinct
        && block.columns.iter().all(
            |column| matches!(column.expr, BoundExpr::Column { source, .. } if source == only.id),
        )
}

/// Reports whether anything in the statement reads a FROM term's rowid, which
/// a derived table does not have.
///
/// @param select - the statement being planned
/// @param id - the FROM term
fn reads_the_rowid(select: &BoundSelect, id: usize) -> bool {
    let mut found = false;
    let mut probe = select.clone();
    crate::rewrite::rewrite_select(&mut probe, &mut |expr: &mut BoundExpr| {
        if matches!(expr, BoundExpr::Rowid { source } if *source == id) {
            found = true;
        }
    });
    found
}

/// Reports whether an expression holds a subquery that reads an enclosing query.
///
/// @param expr - a result column of the derived table
fn holds_a_correlated_subquery(expr: &BoundExpr) -> bool {
    if let BoundExpr::Subquery { block, .. } = expr {
        if !block.correlations.is_empty() {
            return true;
        }
    }
    expr.children().into_iter().any(holds_a_correlated_subquery)
}

/// Replaces the derived table at one position with its FROM terms.
///
/// Every reference to one of its columns becomes the expression that column
/// was. Its `WHERE` joins the statement's, or its `ON` when it was the right
/// side of a `LEFT JOIN`. Its `ORDER BY` and `LIMIT` move to the statement
/// when the statement has none, and are dropped when SQLite drops them.
///
/// @param select - the statement being planned
/// @param position - the derived table's position in the FROM clause
/// @param in_compound - whether the statement is an arm of a compound
fn inline(select: &mut BoundSelect, position: usize, in_compound: bool) {
    let drop_order = match select.sources.get(position).map(|source| &source.rows) {
        Some(SourceRows::Subquery(block)) => drops_its_order_by(select, block, in_compound),
        _ => return,
    };
    let source = select.sources.remove(position);
    let SourceRows::Subquery(block) = source.rows else {
        return;
    };
    let mut block = *block;
    let id = source.id;
    // **A JSON value read through a derived table is plain text in SQLite.**
    // The JSON mark travels here by a JSON call being the argument of another,
    // so putting `json_object(...)` in place of the column a query read from
    // `(SELECT json_object(...) AS obj FROM foo)` handed `json_group_array(obj)`
    // the mark, and the array held objects where SQLite holds quoted strings.
    // A unary plus is the identity on the value and is not a JSON call.
    let replacements: Vec<BoundExpr> = block
        .columns
        .iter()
        .map(|column| match &column.expr {
            BoundExpr::Json { .. } => BoundExpr::Unary {
                op: crate::ast::UnaryOp::Identity,
                operand: Box::new(column.expr.clone()),
            },
            other => other.clone(),
        })
        .collect();
    let mut substitute = |expr: &mut BoundExpr| {
        if let BoundExpr::Column { source, column, .. } = expr {
            if *source == id {
                if let Some(replacement) = replacements.get(usize::from(*column)) {
                    *expr = replacement.clone();
                }
            }
        }
    };
    crate::rewrite::rewrite_select(select, &mut substitute);
    let mut on = source.constraint;
    if let Some(on) = on.as_mut() {
        crate::rewrite::rewrite_expr(on, &mut substitute);
    }
    let inner_ids: Vec<usize> = block.sources.iter().map(|inner| inner.id).collect();
    replace_correlation(select, id, &inner_ids);
    let inner_filter = block.filter.take();
    let mut inner_sources = core::mem::take(&mut block.sources);
    if let Some(first) = inner_sources.first_mut() {
        first.join = source.join;
        if source.join == JoinKind::Left {
            first.constraint = conjoined([on, inner_filter, first.constraint.take()]);
        } else {
            select.filter = conjoined([select.filter.take(), on, inner_filter]);
        }
    }
    for (offset, inner) in inner_sources.into_iter().enumerate() {
        select.sources.insert(position + offset, inner);
    }
    if !in_compound {
        if !drop_order && select.order_by.is_empty() {
            select.order_by = block.order_by;
        }
        if block.limit.is_some() {
            select.limit = block.limit;
        }
    }
}

/// Returns the conjunction of the expressions that are present.
///
/// @param parts - the expressions, in the order they are joined
fn conjoined<const N: usize>(parts: [Option<BoundExpr>; N]) -> Option<BoundExpr> {
    parts
        .into_iter()
        .flatten()
        .reduce(|left, right| BoundExpr::And(Box::new(left), Box::new(right)))
}

/// Replaces one FROM term in the lists of enclosing terms every nested query
/// reads, after the term was flattened into the terms of its derived table.
///
/// A nested query that read a column of the derived table now reads the
/// expression the column was, which reads the derived table's own terms, so
/// it is correlated to those instead. All of them are listed, which can only
/// make a query re-run where it did not need to.
///
/// @param select - the statement, searched to any depth
/// @param old - the flattened FROM term
/// @param new - its derived table's FROM terms
fn replace_correlation(select: &mut BoundSelect, old: usize, new: &[usize]) {
    replace_in(&mut select.correlations, old, new);
    for source in &mut select.sources {
        match &mut source.rows {
            SourceRows::Subquery(block) => replace_correlation(block, old, new),
            SourceRows::Recursive(body) => {
                for (_, arm) in body.seeds.iter_mut().chain(body.steps.iter_mut()) {
                    replace_correlation(arm, old, new);
                }
            }
            _ => {}
        }
    }
    for (_, arm) in &mut select.compounds {
        replace_correlation(arm, old, new);
    }
    crate::rewrite::rewrite_select(select, &mut |expr: &mut BoundExpr| {
        if let Some(block) = expr.block_mut() {
            replace_in(&mut block.correlations, old, new);
        }
    });
}

/// Replaces one number in a list with several, keeping each once.
///
/// @param list - the list
/// @param old - the number to replace
/// @param new - what replaces it
fn replace_in(list: &mut Vec<usize>, old: usize, new: &[usize]) {
    if !list.contains(&old) {
        return;
    }
    list.retain(|held| *held != old);
    for id in new {
        if !list.contains(id) {
            list.push(*id);
        }
    }
}

/// Makes each derived table SQLite runs as a co-routine run again whenever the
/// query holding it runs, instead of once for the statement.
///
/// The binder marks every derived table inside a correlated subquery to be
/// filled once, because a table SQLite fills is filled once (`OP_Once`) however
/// many outer rows the subquery runs for. A co-routine is not filled: SQLite
/// runs it from the start each time, so it sees rows the statement has already
/// changed. `UPDATE t SET v = (SELECT s FROM (SELECT sum(v) s FROM t) WHERE
/// t.id > 0)` reads the sum again for each row in SQLite, giving 60, 110 and 200
/// over 10, 20 and 30, where a sum kept from the first row gave 60 three times.
///
/// A co-routine SQLite reads through an automatic index is the exception. The
/// index is built once, so its rows are the first run's: a derived table with
/// an equality between one of its columns and a column of another table keeps
/// its mark. A CTE's own mark, kept for a body that calls `random()`, is left
/// alone.
///
/// @param select - the statement being planned, after flattening
pub(super) fn unshare_coroutines(select: &mut BoundSelect) {
    let mut unshare = Vec::new();
    for (position, source) in select.sources.iter().enumerate() {
        let SourceRows::Subquery(block) = &source.rows else {
            continue;
        };
        if block
            .shared
            .is_none_or(|key| key < crate::bind::FIRST_ANONYMOUS_SHARED)
        {
            continue;
        }
        if super::tree::runs_as_coroutine(select, position)
            && !reads_through_an_automatic_index(select, source)
        {
            unshare.push(position);
        }
    }
    for position in unshare {
        if let Some(SourceRows::Subquery(block)) = select
            .sources
            .get_mut(position)
            .map(|source| &mut source.rows)
        {
            block.shared = None;
        }
    }
}

/// Reports whether a derived table has an equality between one of its columns
/// and a column of another table, in the `WHERE` or in its `ON`, which is what
/// SQLite reads a derived table through an automatic index for.
///
/// A comparison with a literal does not count: SQLite pushes that into the
/// derived table instead.
///
/// @param select - the statement
/// @param source - the derived table
fn reads_through_an_automatic_index(select: &BoundSelect, source: &BoundSource) -> bool {
    let mut terms: Vec<BoundExpr> = Vec::new();
    if let Some(filter) = &select.filter {
        terms.extend(conjunction(filter));
    }
    if let Some(on) = &source.constraint {
        terms.extend(conjunction(on));
    }
    terms.iter().any(|term| {
        let BoundExpr::Compare {
            op: BinaryOp::Equal,
            left,
            right,
            ..
        } = term
        else {
            return false;
        };
        // A correlated subquery is planned with the enclosing query's columns
        // already turned into parameters, so a parameter counts as such a
        // column here.
        let keyed = |side: &BoundExpr, other: &BoundExpr| {
            let mut used = Vec::new();
            other.sources_used(&mut used);
            let outside = matches!(other, BoundExpr::Parameter(_))
                || (!used.is_empty() && !used.contains(&source.id));
            matches!(side, BoundExpr::Column { source: id, .. } if *id == source.id) && outside
        };
        keyed(left, right) || keyed(right, left)
    })
}

//! Pushing a `WHERE` condition on a derived table's columns into the derived
//! table.
//!
//! Invariant: **a condition is copied into a derived table only when doing so
//! cannot change which rows the statement returns, and the original stays where
//! it was.** The copy is a filter the inner query applies before it builds its
//! rows, so the inner planner can seek by key instead of building every row of
//! a view. The outer condition is still tested on every row the derived table
//! produces, so if a rule here were too generous the cost would be a condition
//! tested twice, never a row that should have been filtered.
//!
//! This is SQLite's push-down optimisation, restricted to the cases where it is
//! plainly sound. Measured on the coffee shop example's database before it:
//! `SELECT * FROM order_summary WHERE id = 57` over a view of three joins and a
//! correlated subquery took 2.50 ms against 0.048 ms for the same query written
//! without the view, because every row of the view, correlated subquery
//! included, was built before the `WHERE` was applied. SQLite plans both as a
//! primary key search.
//!
//! Here rather than in [`super`] because `plan.rs` is at its recorded size.

use super::*;

mod unused;

/// Copies each `WHERE` conjunct that reads only one derived table's columns
/// into that derived table's own `WHERE`, and then replaces each derived
/// table's unread result columns with NULL.
///
/// The second step is in [`unused`]. It runs here because it is the other
/// half of treating a derived table the way SQLite's flattener does, and
/// `plan.rs` is at its recorded size.
///
/// @param select - the statement being planned, whose derived tables may gain
///   a filter and lose result columns
pub(super) fn push_into_derived_tables(select: &mut BoundSelect) {
    push_filters(select);
    unused::drop_unread_columns(select);
}

/// Copies each `WHERE` conjunct that reads only one derived table's columns
/// into that derived table's own `WHERE`.
///
/// @param select - the statement being planned, whose derived tables may gain
///   a filter
fn push_filters(select: &mut BoundSelect) {
    let Some(filter) = select.filter.as_ref() else {
        return;
    };
    // Nothing is split or copied for a statement with no derived table, which
    // is almost every statement: the compile of `SELECT id FROM t WHERE email
    // = ?1` has an allocation budget, and splitting its `WHERE` here took three
    // of them to find nothing to push.
    if !select
        .sources
        .iter()
        .any(|source| matches!(source.rows, SourceRows::Subquery(_)))
    {
        return;
    }
    // A `RIGHT` or `FULL` join can null extend any term before it, so a
    // statement with one pushes nothing.
    if select
        .sources
        .iter()
        .any(|source| matches!(source.join, JoinKind::Right | JoinKind::Full))
    {
        return;
    }
    let conjuncts = conjunction(filter);
    for source in &mut select.sources {
        // The right side of a `LEFT JOIN` is null extended when nothing in it
        // matches, and a condition such as `v.x IS NULL` is true of a null
        // extended row and false of the rows the push would have removed.
        if source.join == JoinKind::Left {
            continue;
        }
        let id = source.id;
        let SourceRows::Subquery(block) = &mut source.rows else {
            continue;
        };
        // The references of a shared common table expression read one set of
        // rows, so a filter pushed into one of them would change the others.
        if block.shared.is_some() {
            continue;
        }
        // SQLite pushes nothing into a `MATERIALIZED` CTE, nor into a CTE named
        // by more than one FROM term, which it fills once for all of them.
        if source.derived.cte
            && (source.derived.materialized == Some(true) || source.derived.uses >= 2)
        {
            continue;
        }
        if block.compounds.is_empty() {
            if accepts_a_pushed_filter(block) {
                push_into_arm(block, id, &conjuncts);
            } else if accepts_a_grouped_filter(block) {
                let on_groups: Vec<BoundExpr> = conjuncts
                    .iter()
                    .filter(|conjunct| reads_only_grouping_columns(conjunct, id, block))
                    .cloned()
                    .collect();
                push_into_arm(block, id, &on_groups);
            }
        } else if accepts_a_compound_filter(block) {
            push_into_compound(block, id, &conjuncts);
        }
    }
}

/// Copies each conjunct on the derived table into one `SELECT` that produces
/// the derived table's rows, with the conjunct's columns replaced by the
/// expressions that compute them in that `SELECT`.
///
/// @param arm - the `SELECT` (one arm of a compound, or the whole derived table)
/// @param id - the derived table's statement-wide number
/// @param conjuncts - the terms of the outer `WHERE`
fn push_into_arm(arm: &mut BoundSelect, id: usize, conjuncts: &[BoundExpr]) {
    for conjunct in conjuncts {
        let mut used = Vec::new();
        conjunct.sources_used(&mut used);
        if used.as_slice() != [id] || !pushable(conjunct, id) {
            continue;
        }
        let Some(inner) = substituted(conjunct, id, arm) else {
            continue;
        };
        arm.filter = Some(match arm.filter.take() {
            Some(existing) => BoundExpr::And(Box::new(existing), Box::new(inner)),
            None => inner,
        });
    }
}

/// Copies the conjuncts into every arm of a compound derived table.
///
/// **Each arm compares with its own columns' affinity and collation.** SQLite
/// pushes the term into every arm, and a term `x > 7` over `SELECT x FROM t
/// UNION ALL SELECT y FROM v` with a TEXT `x` and an INTEGER `y` compares text
/// with text in the first arm and integers in the second. Tested after the
/// compound, the column has no affinity and a text `'1'` is greater than the
/// integer 7 by storage class. An arm that cannot take a filter (a `VALUES`
/// list, an aggregate arm) is left as it is; the outer term stays in place.
///
/// @param block - the compound derived table; its first arm is the block itself
/// @param id - the derived table's statement-wide number
/// @param conjuncts - the terms of the outer `WHERE`
fn push_into_compound(block: &mut BoundSelect, id: usize, conjuncts: &[BoundExpr]) {
    if accepts_an_arm_filter(block) {
        push_into_arm(block, id, conjuncts);
    }
    for (_, arm) in &mut block.compounds {
        if accepts_an_arm_filter(arm) && arm.compounds.is_empty() {
            push_into_arm(arm, id, conjuncts);
        }
    }
}

/// Reports whether a filter on a compound's result may be copied into its arms.
///
/// SQLite refuses a compound with a `LIMIT` or `OFFSET`, which count rows
/// before the filter, and a compound that has a window function in any arm.
/// When an arm is joined by `UNION`, `INTERSECT` or `EXCEPT` it also refuses
/// when the compound's `ORDER BY` has a term that is not a result column.
///
/// @param block - the compound derived table
fn accepts_a_compound_filter(block: &BoundSelect) -> bool {
    let all_union_all = block
        .compounds
        .iter()
        .all(|(op, _)| *op == crate::ast::CompoundOp::UnionAll);
    // SQLite refuses to push into a compound joined by anything but UNION ALL
    // when any result column of any arm has a collation other than BINARY. The
    // compound removes duplicates under that collation and the pushed term
    // compares under its own, so the two can keep different rows:
    // `SELECT * FROM (SELECT a FROM t1 INTERSECT SELECT b FROM t2) WHERE a||''
    // = 'ABC'` with NOCASE columns keeps 'ABC' only when nothing is pushed.
    let binary_columns = |arm: &BoundSelect| {
        arm.columns
            .iter()
            .all(|column| crate::bind::result_collation(&column.expr) == Collation::Binary)
    };
    block.limit.is_none()
        && block.offset.is_none()
        && (all_union_all || block.order_by.is_empty())
        && (all_union_all
            || (binary_columns(block)
                && block.compounds.iter().all(|(_, arm)| binary_columns(arm))))
        && block.windows.is_empty()
        && block
            .compounds
            .iter()
            .all(|(_, arm)| arm.windows.is_empty())
}

/// Reports whether one arm of a compound can take a copied filter.
///
/// An arm with its own `DISTINCT`, grouping, aggregate or window computes its
/// result columns over rows a filter would remove, and a `VALUES` list has no
/// expressions to substitute.
///
/// @param arm - one arm of the compound
fn accepts_an_arm_filter(arm: &BoundSelect) -> bool {
    !arm.distinct
        && arm.group_by.is_empty()
        && arm.aggregates.is_empty()
        && arm.having.is_none()
        && arm.windows.is_empty()
        && arm.values.is_empty()
}

/// Reports whether a derived table's rows are the same whether a condition on
/// its result columns is applied before it builds them or after.
///
/// **Not for a `LIMIT` or an `OFFSET`**, which count rows before the condition;
/// **not for `DISTINCT`**, which keeps one row of several equal under its own
/// collation, so a condition under a different collation can keep a different
/// one; **not for grouping or a window**, whose result columns are computed
/// over rows the condition would remove; and **not for a compound**, which is
/// several blocks.
///
/// @param block - the derived table's query
fn accepts_a_pushed_filter(block: &BoundSelect) -> bool {
    block.compounds.is_empty()
        && block.limit.is_none()
        && block.offset.is_none()
        && !block.distinct
        && block.group_by.is_empty()
        && block.aggregates.is_empty()
        && block.having.is_none()
        && block.windows.is_empty()
        && block.values.is_empty()
}

/// Reports whether a grouped derived table can take a filter on its grouping
/// columns.
///
/// **SQLite's push down into an aggregate.** A condition that reads only the
/// columns a derived table groups by keeps or removes whole groups, so it can
/// be tested on the rows before they are grouped. That is what lets `SELECT s
/// FROM (SELECT g, sum(v) s FROM t GROUP BY g) WHERE g = 5` search an index on
/// `g` rather than group the whole table: a view that sums per customer,
/// read for one customer, cost the whole table per read, and a correlated
/// lookup into such a view 3,000 times took nine times SQLite's time. Not with
/// a window, a `DISTINCT` or a `LIMIT`, which work on the grouped rows.
///
/// @param block - the derived table's query
fn accepts_a_grouped_filter(block: &BoundSelect) -> bool {
    block.compounds.is_empty()
        && !block.group_by.is_empty()
        && block.limit.is_none()
        && block.offset.is_none()
        && !block.distinct
        && block.windows.is_empty()
        && block.values.is_empty()
}

/// Reports whether every derived table column a condition reads is computed
/// by an expression the derived table groups by.
///
/// @param conjunct - the condition, over the derived table's columns
/// @param id - the derived table's statement-wide number
/// @param block - the derived table's query
fn reads_only_grouping_columns(conjunct: &BoundExpr, id: usize, block: &BoundSelect) -> bool {
    let mut grouped = true;
    let mut probe = conjunct.clone();
    crate::rewrite::rewrite_expr(&mut probe, &mut |expr: &mut BoundExpr| {
        if let BoundExpr::Column { source, column, .. } = expr {
            if *source == id {
                let computed = block
                    .columns
                    .get(usize::from(*column))
                    .map(|held| &held.expr);
                grouped &=
                    computed.is_some_and(|inner| block.group_by.iter().any(|key| key == inner));
            }
        }
    });
    grouped
}

/// Reports whether a condition is one that may be evaluated anywhere, any
/// number of times, with the same answer.
///
/// A whitelist: a subquery, an aggregate, a window value, a registered function
/// whose determinism the planner cannot see, and the scalar functions whose
/// answer changes from call to call are all refused.
///
/// @param expr - the condition, or a part of it
/// @param id - the derived table's statement-wide number; its rowid has no
///   inner expression to stand for it
fn pushable(expr: &BoundExpr, id: usize) -> bool {
    let this = match expr {
        BoundExpr::Rowid { source } => *source != id,
        BoundExpr::Subquery { .. }
        | BoundExpr::Aggregate { .. }
        | BoundExpr::WindowRef { .. }
        | BoundExpr::SorterColumn { .. }
        | BoundExpr::External { .. }
        | BoundExpr::VirtualFunction { .. }
        | BoundExpr::Raise { .. } => false,
        BoundExpr::Function { func, .. } => !matches!(
            func,
            crate::function::ScalarFunc::Random
                | crate::function::ScalarFunc::RandomBlob
                | crate::function::ScalarFunc::Changes
                | crate::function::ScalarFunc::TotalChanges
                | crate::function::ScalarFunc::LastInsertRowid
        ),
        _ => true,
    };
    this && expr.children().iter().all(|child| pushable(child, id))
}

/// Reports whether a condition may be tested more than once with the same
/// answer each time.
///
/// A condition on an outer term can be tested before a lateral join runs its
/// function, so that the function is not called for rows the condition
/// removes. It is tested again with the rest of the `WHERE`, which is only
/// harmless for a condition with no subquery, no random function and no
/// registered function whose determinism the planner cannot see.
///
/// @param expr - the condition
pub fn is_repeatable_condition(expr: &BoundExpr) -> bool {
    pushable(expr, usize::MAX)
}

/// Reports whether an expression calls a function whose answer changes from one
/// call to the next, such as `random()`.
///
/// @param expr - the expression, or a part of it
pub fn calls_a_volatile_function(expr: &BoundExpr) -> bool {
    let this = matches!(
        expr,
        BoundExpr::Function {
            func: crate::function::ScalarFunc::Random
                | crate::function::ScalarFunc::RandomBlob
                | crate::function::ScalarFunc::Changes
                | crate::function::ScalarFunc::TotalChanges
                | crate::function::ScalarFunc::LastInsertRowid,
            ..
        }
    );
    this || expr
        .children()
        .iter()
        .any(|child| calls_a_volatile_function(child))
}

/// Returns a condition with each of the derived table's columns replaced by
/// the expression that computes it inside the derived table.
///
/// `None` when a column's expression is one that should not be evaluated in a
/// `WHERE`, such as a correlated subquery: the condition is then left outside,
/// where it was.
///
/// @param conjunct - the condition, over the derived table's columns
/// @param id - the derived table's statement-wide number
/// @param block - the derived table's query
fn substituted(conjunct: &BoundExpr, id: usize, block: &BoundSelect) -> Option<BoundExpr> {
    let mut copy = conjunct.clone();
    replace_columns(&mut copy, id, block).then_some(copy)
}

/// Replaces the derived table's columns in place, reporting whether every one
/// could be replaced.
///
/// @param expr - the expression being rewritten
/// @param id - the derived table's statement-wide number
/// @param block - the derived table's query
fn replace_columns(expr: &mut BoundExpr, id: usize, block: &BoundSelect) -> bool {
    if let BoundExpr::Column {
        source,
        column,
        collation: outer_collation,
        ..
    } = expr
    {
        if *source != id {
            return true;
        }
        let outer_collation = *outer_collation;
        let Some(inner) = block.columns.get(usize::from(*column)) else {
            return false;
        };
        if !pushable(&inner.expr, usize::MAX) {
            return false;
        }
        // A JSON call stands behind a unary plus, so the pushed copy reads the
        // column as the plain text the derived table hands out; see `inline`
        // in `flatten.rs`. Pushed bare, `WHERE NOT json_quote(c0)` over
        // `SELECT json(TRUE) AS c0` read the JSON mark and kept no row.
        *expr = match &inner.expr {
            BoundExpr::Json { .. } => BoundExpr::Unary {
                op: crate::ast::UnaryOp::Identity,
                operand: Box::new(inner.expr.clone()),
            },
            other => other.clone(),
        };
        *expr = with_derived_collation(std::mem::replace(expr, BoundExpr::Null), outer_collation);
        return true;
    }
    let replaced = expr
        .children_mut()
        .into_iter()
        .all(|child| replace_columns(child, id, block));
    if replaced {
        refresh_comparison_rules(expr);
    }
    replaced
}

/// Makes a substituted expression compare with the collation the derived
/// table's column had.
///
/// SQLite's `substExpr` wraps the replacement in a `COLLATE` when its own
/// collation differs from the one the replaced column had. For a compound
/// derived table the column has the leftmost arm's collation, so a filter
/// `b = 'BbB'` over `SELECT a, b FROM t1 UNION ALL SELECT c, d FROM t2` with a
/// NOCASE `t1.b` must compare `t2.d` with NOCASE as well. Without the wrapper
/// each arm compared with its own column's collation and the BINARY arm
/// missed the row the unpushed filter would have kept.
///
/// @param replacement - the expression that replaced the derived column
/// @param outer - the collation the derived table's column had
fn with_derived_collation(replacement: BoundExpr, outer: Collation) -> BoundExpr {
    if crate::bind::result_collation(&replacement) == outer {
        return replacement;
    }
    BoundExpr::Collate {
        operand: Box::new(replacement),
        collation: outer,
    }
}

/// Recomputes the affinity and collation a comparison applies, from its
/// operands as they are now.
///
/// A comparison fixes both when the statement is bound, from the operands it
/// was written with. After a derived table's column is replaced by the
/// expression of one arm, the operand may have a different affinity: the
/// derived table's column of a compound has none when the arms disagree, and
/// the arm's own column has its declared one. SQLite compares the substituted
/// expression, so the comparison is rebuilt from it.
///
/// @param expr - the expression whose children were just substituted
fn refresh_comparison_rules(expr: &mut BoundExpr) {
    use crate::bind::comparison_rules;
    match expr {
        BoundExpr::Compare {
            left,
            right,
            affinity,
            collation,
            ..
        }
        | BoundExpr::Is {
            left,
            right,
            affinity,
            collation,
            ..
        } => (*affinity, *collation) = comparison_rules(left, right),
        BoundExpr::Between {
            operand,
            low,
            high,
            low_affinity,
            low_collation,
            high_affinity,
            high_collation,
            ..
        } => {
            (*low_affinity, *low_collation) = comparison_rules(operand, low);
            (*high_affinity, *high_collation) = comparison_rules(operand, high);
        }
        BoundExpr::InList {
            operand,
            list,
            affinity,
            collation,
            ..
        } => {
            if let Some(first) = list.first() {
                (*affinity, *collation) = comparison_rules(operand, first);
            }
        }
        BoundExpr::Case {
            operand: Some(operand),
            branches,
            comparisons,
            ..
        } => {
            *comparisons = branches
                .iter()
                .map(|(when, _)| comparison_rules(operand, when))
                .collect();
        }
        _ => {}
    }
}

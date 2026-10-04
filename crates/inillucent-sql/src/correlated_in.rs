//! Lowering a correlated `IN` subquery to `EXISTS`.
//!
//! Invariant: **the lowering answers what SQLite answers, NULLs included.**
//! `x IN (SELECT y FROM s WHERE p)` is not "is there a row where `y = x`": an
//! empty list is false even when `x` is NULL, a NULL `x` over a list with rows
//! is NULL, and a list that holds a NULL turns a non-match into NULL. A
//! lowering that kept only the first of those would answer false where SQLite
//! answers NULL, and `WHERE` treats the two the same - so the difference would
//! never show in a row count and would show in an `IS NULL` or a `CASE`.
//!
//! **Why it is a rewrite at all.** The physical pass computes a correlated
//! block once per outer row and hands the operator one column; an `IN` needs
//! the whole list, so `crates/inillucent-exec/src/correlate.rs` refused one by
//! name rather than answering wrongly. `EXISTS` over the same block is what the
//! engine already runs per row, so writing the `IN` as three `EXISTS` tests
//! reaches an execution path that exists (task-1979, section 8.2, gap 1).
//!
//! The three blocks are: does the list have any row, does it hold a row equal
//! to the operand, and does it hold a NULL. The `CASE` below orders them so
//! that each one is only consulted where its answer decides the result.
//!
//! **What is not lowered.** A block with `GROUP BY`, `HAVING`, `DISTINCT`,
//! `LIMIT`, `OFFSET`, a compound arm, a window or a `VALUES` list keeps the
//! refusal: the equality is pushed into the block's `WHERE`, and a `WHERE` is
//! applied before grouping and before a limit, so pushing it past either would
//! ask a different question.

use std::rc::Rc;

use inillucent_value::{Affinity, Collation};

use crate::ast::{BinaryOp, JoinKind};
use crate::bind::{BoundExpr, BoundSelect, BoundStatement, SourceRows, SubqueryKind};
use crate::catalog_view::{TableInfo, TableKind};

/// Rewrites every correlated `IN` in a statement into `EXISTS` tests.
///
/// Called after binding and before planning. A block this cannot lower is left
/// exactly as it was, so the refusal that names it still fires.
///
/// @param statement - the bound statement, rewritten in place
///
/// **The statement's triggers are lowered too.** A trigger body is bound
/// with the statement that fires it, and a correlated `IN` in a body was
/// refused as not built when the body ran, while the same query typed as a
/// statement of its own was lowered and answered.
pub fn lower(statement: &mut BoundStatement) {
    let mut highest = 0usize;
    let mut correlated_in = false;
    walk_statement(statement, &mut |expr: &mut BoundExpr| {
        if let BoundExpr::Subquery {
            id, kind, block, ..
        } = expr
        {
            highest = highest.max(*id);
            correlated_in |= *kind == SubqueryKind::In && !block.correlations.is_empty();
        }
    });
    // Every statement is lowered, and nearly none holds a correlated `IN`;
    // the tables are gathered only for one that does.
    if !correlated_in {
        return;
    }
    let mut next = highest.saturating_add(1);
    let tables = never_null_tables(statement);
    walk_statement(statement, &mut |expr: &mut BoundExpr| {
        lower_one(expr, &mut next, &tables)
    });
}

/// Returns the FROM terms whose `NOT NULL` columns can never read as NULL.
///
/// An ordinary table read in a block with no outer join. A term on either side
/// of a `LEFT`, `RIGHT` or `FULL` join can be null extended, so every term of
/// such a block is left out, which is the cautious half of the rule. So are
/// the terms of an `INSERT`, `UPDATE` or `DELETE`, which are not walked.
///
/// @param statement - the bound statement
fn never_null_tables(statement: &mut BoundStatement) -> Vec<(usize, Rc<TableInfo>)> {
    let mut found = Vec::new();
    if let BoundStatement::Select(select) = statement {
        gather_tables(select, &mut found);
    }
    walk_statement(statement, &mut |expr: &mut BoundExpr| {
        if let Some(block) = expr.block_mut() {
            gather_tables(block, &mut found);
        }
    });
    found
}

/// Adds one block's ordinary tables, and its derived tables' and arms', to a list.
///
/// @param select - the block
/// @param into - the terms found so far
fn gather_tables(select: &BoundSelect, into: &mut Vec<(usize, Rc<TableInfo>)>) {
    let outer = select.sources.iter().any(|source| {
        matches!(
            source.join,
            JoinKind::Left | JoinKind::Right | JoinKind::Full
        )
    });
    for source in &select.sources {
        match &source.rows {
            SourceRows::Table if !outer && source.table.kind == TableKind::Table => {
                into.push((source.id, Rc::clone(&source.table)));
            }
            SourceRows::Subquery(block) => gather_tables(block, into),
            SourceRows::Recursive(body) => {
                for (_, arm) in body.seeds.iter().chain(body.steps.iter()) {
                    gather_tables(arm, into);
                }
            }
            _ => {}
        }
    }
    for (_, arm) in &select.compounds {
        gather_tables(arm, into);
    }
}

/// Reports whether an expression is a column that can never be NULL.
///
/// A `NOT NULL` column or the rowid of a term [`never_null_tables`] listed.
///
/// @param expr - the expression
/// @param tables - the terms whose declared constraints hold
fn never_null(expr: &BoundExpr, tables: &[(usize, Rc<TableInfo>)]) -> bool {
    let table_of = |wanted: usize| {
        tables
            .iter()
            .find(|(id, _)| *id == wanted)
            .map(|(_, table)| table)
    };
    match expr {
        BoundExpr::Column { source, column, .. } => table_of(*source).is_some_and(|table| {
            table.rowid_alias == Some(*column)
                || table
                    .columns
                    .get(usize::from(*column))
                    .is_some_and(|held| held.not_null)
        }),
        BoundExpr::Rowid { source } => table_of(*source).is_some_and(|table| !table.without_rowid),
        _ => false,
    }
}

/// Applies a rewrite to a statement and to every trigger body it fires.
///
/// @param statement - the bound statement
/// @param rewrite - what to do to each expression
fn walk_statement(statement: &mut BoundStatement, rewrite: crate::rewrite::Rewrite<'_>) {
    match statement {
        BoundStatement::Select(select) => crate::rewrite::rewrite_select(select, rewrite),
        BoundStatement::Insert(insert) => {
            crate::rewrite::rewrite_insert(insert, rewrite);
            walk_triggers(&mut insert.triggers, rewrite);
            for clause in &mut insert.upsert {
                walk_triggers(&mut clause.triggers, rewrite);
            }
        }
        BoundStatement::Update(update) => {
            crate::rewrite::rewrite_update(update, rewrite);
            walk_triggers(&mut update.triggers, rewrite);
        }
        BoundStatement::Delete(delete) => {
            crate::rewrite::rewrite_delete(delete, rewrite);
            walk_triggers(&mut delete.triggers, rewrite);
        }
        BoundStatement::Directive(_) | BoundStatement::Empty => {}
    }
}

/// Applies a rewrite to every statement in some triggers' bodies, and to the
/// triggers those statements fire in turn.
///
/// @param triggers - the triggers
/// @param rewrite - what to do to each expression
fn walk_triggers(triggers: &mut [crate::dml::BoundTrigger], rewrite: crate::rewrite::Rewrite<'_>) {
    use crate::dml::BoundTriggerStatement;
    for trigger in triggers {
        for statement in &mut trigger.body {
            match statement {
                BoundTriggerStatement::Select(select) => {
                    crate::rewrite::rewrite_select(select, rewrite)
                }
                BoundTriggerStatement::Insert(insert) => {
                    crate::rewrite::rewrite_insert(insert, rewrite);
                    walk_triggers(&mut insert.triggers, rewrite);
                    for clause in &mut insert.upsert {
                        walk_triggers(&mut clause.triggers, rewrite);
                    }
                }
                BoundTriggerStatement::Update(update) => {
                    crate::rewrite::rewrite_update(update, rewrite);
                    walk_triggers(&mut update.triggers, rewrite);
                }
                BoundTriggerStatement::Delete(delete) => {
                    crate::rewrite::rewrite_delete(delete, rewrite);
                    walk_triggers(&mut delete.triggers, rewrite);
                }
            }
        }
    }
}

/// Replaces one expression when it is a correlated `IN` this can lower.
///
/// @param expr - the expression, replaced in place
/// @param next - the next free subquery number, advanced by up to three on a rewrite
/// @param tables - the terms whose `NOT NULL` columns can never read as NULL
fn lower_one(expr: &mut BoundExpr, next: &mut usize, tables: &[(usize, Rc<TableInfo>)]) {
    let BoundExpr::Subquery {
        kind: SubqueryKind::In,
        negated,
        operand: Some(operand),
        block,
        affinity,
        collation,
        ..
    } = expr
    else {
        return;
    };
    if block.correlations.is_empty() || !liftable(block) {
        return;
    }
    let listed = match block.columns.first() {
        Some(column) => column.expr.clone(),
        None => return,
    };
    let replacement = lowered(
        &Lowering {
            operand_never_null: never_null(operand, tables),
            listed_never_null: never_null(&listed, tables),
            operand: (**operand).clone(),
            listed,
            negated: *negated,
            affinity: *affinity,
            collation: *collation,
        },
        block,
        next,
    );
    *expr = replacement;
}

/// Returns whether a block's `WHERE` decides the same rows the block reports.
///
/// The equality is pushed into the block's `WHERE`, so anything that reads the
/// rows *after* the `WHERE` - grouping, a limit, a compound arm - would be
/// asked a different question by the rewritten block.
///
/// @param block - the subquery's own select
fn liftable(block: &BoundSelect) -> bool {
    block.group_by.is_empty()
        && block.having.is_none()
        && !block.distinct
        && block.limit.is_none()
        && block.offset.is_none()
        && block.compounds.is_empty()
        && block.windows.is_empty()
        && block.aggregates.is_empty()
        && block.values.is_empty()
        && !block.sources.is_empty()
}

/// What one `IN` was written as, which is what the lowering needs.
///
/// A struct rather than six parameters, because
/// `crates/inillucent-compat/tests/tooling/policy.rs` refuses an
/// `#[allow(clippy::too_many_arguments)]`: the threshold is set once in
/// `clippy.toml` with the argument for where it is, and an attribute moves the
/// bar for one function and says nothing about why.
struct Lowering {
    /// The left side of the `IN`.
    operand: BoundExpr,
    /// The block's first result column, which is what `IN` compares against.
    listed: BoundExpr,
    /// Whether `NOT IN` was written.
    negated: bool,
    /// The affinity `IN` applies to both sides.
    affinity: Option<Affinity>,
    /// The collation `IN` compares with.
    collation: Collation,
    /// Whether the operand is a column that can never be NULL.
    operand_never_null: bool,
    /// Whether the listed column can never be NULL.
    listed_never_null: bool,
}

/// Builds the `CASE` that answers what `IN` answers.
///
/// ```text
/// CASE WHEN <a row equals the operand>   THEN 1
///      WHEN <the operand is NULL>        THEN CASE WHEN <the list has a row> THEN NULL ELSE 0 END
///      WHEN <the list holds a NULL>      THEN NULL
///      ELSE 0 END
/// ```
///
/// The first arm cannot fire when the operand is NULL, because `y = NULL` is
/// NULL rather than true, so the second arm is reached exactly when the operand
/// is NULL. `NOT IN` is the same shape with the 1 and the 0 exchanged; NULL
/// stays NULL, which is what makes `NOT IN` over a list holding a NULL answer
/// nothing.
///
/// **An arm that cannot fire is left out (task-2183).** The correlation
/// operator answers every block of a row before the `CASE` reads any of them,
/// so each arm was a whole `EXISTS` run per outer row whether it decided the
/// answer or not. The second arm needs a NULL operand and the third a NULL in
/// the list, and a column declared `NOT NULL` in a block with no outer join
/// can be neither. `wide.id IN (SELECT owner FROM side_table WHERE owner =
/// wide.id)` ran three blocks per row and runs one.
///
/// @param about - what the `IN` was written as
/// @param block - the subquery's own select
/// @param next - the next free subquery number, advanced by three
fn lowered(about: &Lowering, block: &BoundSelect, next: &mut usize) -> BoundExpr {
    let matched = exists(
        block,
        Some(BoundExpr::Compare {
            op: BinaryOp::Equal,
            left: Box::new(widened(about.listed.clone(), about.affinity)),
            right: Box::new(widened(about.operand.clone(), about.affinity)),
            affinity: about.affinity,
            collation: about.collation,
        }),
        next,
    );
    // With neither NULL arm left the `CASE` is the `EXISTS` itself, which is
    // 1 or 0 and never NULL, or its negation for `NOT IN`.
    if about.operand_never_null && about.listed_never_null {
        let mut matched = matched;
        if let BoundExpr::Subquery { negated, .. } = &mut matched {
            *negated = about.negated;
        }
        return matched;
    }
    let (found, missing) = match about.negated {
        true => (BoundExpr::Integer(0), BoundExpr::Integer(1)),
        false => (BoundExpr::Integer(1), BoundExpr::Integer(0)),
    };
    let mut branches = vec![(matched, found)];
    if !about.operand_never_null {
        let any_row = exists(block, None, next);
        branches.push((
            BoundExpr::IsNull {
                negated: false,
                operand: Box::new(about.operand.clone()),
            },
            BoundExpr::Case {
                operand: None,
                branches: vec![(any_row, BoundExpr::Null)],
                otherwise: Some(Box::new(missing.clone())),
                comparisons: Vec::new(),
            },
        ));
    }
    if !about.listed_never_null {
        let any_null = exists(
            block,
            Some(BoundExpr::IsNull {
                negated: false,
                operand: Box::new(about.listed.clone()),
            }),
            next,
        );
        branches.push((any_null, BoundExpr::Null));
    }
    BoundExpr::Case {
        operand: None,
        branches,
        otherwise: Some(Box::new(missing)),
        comparisons: Vec::new(),
    }
}

/// Widens an integer to a real when the `IN` compares under a REAL affinity.
///
/// SQLite tests `IN` membership through an ephemeral index that stores both
/// sides under the comparison's affinity, and a REAL affinity stores an integer
/// as a real. So `9223372036854775806 IN (SELECT a ...)` matches a REAL column
/// holding 9223372036854775807, while the `=` this lowering otherwise writes
/// keeps the integer exact and does not. The uncorrelated `IN` makes the same
/// conversion in `inillucent-exec`'s `membership_sides`; this is the same rule
/// for the lowered form, written as
/// `CASE WHEN typeof(x) = 'integer' THEN CAST(x AS REAL) ELSE x END`.
///
/// @param expr - one side of the equality
/// @param affinity - the affinity `IN` applies to both sides
fn widened(expr: BoundExpr, affinity: Option<Affinity>) -> BoundExpr {
    if affinity != Some(Affinity::Real) {
        return expr;
    }
    let is_integer = BoundExpr::Compare {
        op: BinaryOp::Equal,
        left: Box::new(BoundExpr::Function {
            func: crate::function::ScalarFunc::TypeOf,
            arguments: vec![expr.clone()],
            collation: Collation::Binary,
        }),
        right: Box::new(BoundExpr::Text(b"integer".to_vec())),
        affinity: None,
        collation: Collation::Binary,
    };
    BoundExpr::Case {
        operand: None,
        branches: vec![(
            is_integer,
            BoundExpr::Cast {
                operand: Box::new(expr.clone()),
                affinity: Affinity::Real,
            },
        )],
        otherwise: Some(Box::new(expr)),
        comparisons: Vec::new(),
    }
}

/// Returns the terms of a conjunction, left to right.
///
/// @param filter - a `WHERE` clause
fn conjuncts(filter: &BoundExpr) -> Box<dyn Iterator<Item = &BoundExpr> + '_> {
    match filter {
        BoundExpr::And(left, right) => Box::new(conjuncts(left).chain(conjuncts(right))),
        other => Box::new(std::iter::once(other)),
    }
}

/// Returns an `EXISTS` over a copy of the block, with one more `WHERE` term.
///
/// @param block - the subquery's own select
/// @param extra - the term to add to its `WHERE`, when there is one
/// @param next - the next free subquery number, advanced by one
fn exists(block: &BoundSelect, extra: Option<BoundExpr>, next: &mut usize) -> BoundExpr {
    let mut copy = block.clone();
    // The block's own result columns are not read by `EXISTS`, and one of them
    // may be the column the equality now tests - so they are replaced by a
    // constant rather than kept.
    copy.columns.truncate(1);
    if let Some(first) = copy.columns.first_mut() {
        first.expr = BoundExpr::Integer(1);
        first.origin = None;
    }
    copy.order_by.clear();
    // **An equality the `WHERE` already holds is not added twice (task-2183).**
    // `x IN (SELECT y FROM s WHERE y = x)` would test `y = x` twice, and the
    // second copy is a residual filter the access path does not consume.
    let held_already = |extra: &BoundExpr| {
        copy.filter
            .as_ref()
            .is_some_and(|filter| conjuncts(filter).any(|term| term == extra))
    };
    if let Some(extra) = extra.filter(|extra| !held_already(extra)) {
        copy.filter = Some(match copy.filter.take() {
            Some(held) => BoundExpr::And(Box::new(held), Box::new(extra)),
            None => extra,
        });
    }
    let id = *next;
    *next = next.saturating_add(1);
    BoundExpr::Subquery {
        id,
        kind: SubqueryKind::Exists,
        negated: false,
        operand: None,
        block: Box::new(copy),
        affinity: None,
        collation: Collation::Binary,
    }
}

//! Result columns of a derived table that nothing reads, and aggregates that
//! only such a column held.
//!
//! Invariant: **an aggregate of an enclosing query that was written in a result
//! column nothing reads is not computed and is not refused.** SQLite replaces
//! the unread result columns of a derived table made of `UNION ALL` arms by
//! `NULL` before it generates code, and it raises `misuse of aggregate` only
//! when the code for such a call is generated. So `SELECT count(*) FROM t0 WHERE
//! EXISTS (SELECT 1 FROM t1 GROUP BY c3 HAVING (SELECT count(*) FROM (SELECT 1
//! UNION ALL SELECT sum(DISTINCT c1))))` answers, because `count(*)` reads no
//! column of the derived table, while `SELECT max(x) FROM (SELECT 1 AS x UNION
//! ALL SELECT sum(c1))` is refused when `c1` belongs to a query that computes no
//! aggregate.
//!
//! This module holds the binder's two halves of that rule: the derived table's
//! unread columns are replaced by `NULL`, and a call that no expression reads
//! any more is told apart from one that is still read (see
//! [`super::outer_aggregate`]).

use super::outer_aggregate::terms_read;
use super::{Binder, BoundExpr, BoundSelect, SourceRows};
use crate::ast::CompoundOp;
use crate::rewrite::rewrite_select;

/// Returns which columns of the reserved FROM term a block reads, counting the
/// block's `EXISTS` result list or not.
///
/// @param block - the query that has just closed
/// @param source - the reserved FROM term number an aggregate was attached to
/// @param exists - whether the block is an `EXISTS` whose result list is never read
pub(super) fn stand_in_reads(block: &BoundSelect, source: usize, exists: bool) -> Vec<usize> {
    let mut copy = block.clone();
    if exists {
        copy = super::outer_aggregate::exists_block(copy);
    }
    let mut found = Vec::new();
    rewrite_select(&mut copy, &mut |expr: &mut BoundExpr| {
        if let BoundExpr::Column {
            source: held,
            column,
            ..
        } = expr
        {
            if *held == source && !found.contains(&usize::from(*column)) {
                found.push(usize::from(*column));
            }
        }
    });
    found
}

/// Removes a reserved FROM term from the correlation list of a block and of
/// every block it holds.
///
/// @param block - the block to correct
/// @param source - the FROM term number that nothing reads
fn drop_correlation(block: &mut BoundSelect, source: usize) {
    block.correlations.retain(|id| *id != source);
    for term in &mut block.sources {
        if let SourceRows::Subquery(inner) = &mut term.rows {
            drop_correlation(inner, source);
        }
    }
    for (_, arm) in &mut block.compounds {
        drop_correlation(arm, source);
    }
}

/// Removes a reserved FROM term from every correlation list in a query whose
/// subqueries attached aggregates that were all dropped.
///
/// @param block - the owning query
/// @param source - the reserved FROM term number
pub(super) fn forget_reserved_source(block: &mut BoundSelect, source: usize) {
    drop_correlation(block, source);
    rewrite_select(block, &mut |expr: &mut BoundExpr| {
        if let Some(inner) = expr.block_mut() {
            drop_correlation(inner, source);
        }
    });
}

/// Reports whether an expression reads one of the reserved FROM terms.
///
/// @param expr - a result column of a compound arm
/// @param reserved - the numbers reserved for aggregates of enclosing queries
fn reads_reserved(expr: &BoundExpr, reserved: &[usize]) -> bool {
    let mut used = Vec::new();
    terms_read(expr, &mut used);
    used.iter().any(|id| reserved.contains(id))
}

/// Reports whether a derived table is one SQLite replaces the unread columns of:
/// a compound of `UNION ALL` arms, none of which is `DISTINCT`, aggregates or
/// windows, and which has no `ORDER BY`, `LIMIT` or `OFFSET` of its own.
///
/// @param inner - the derived table's query
fn drops_unread_columns(inner: &BoundSelect) -> bool {
    let plain = |arm: &BoundSelect| {
        !arm.distinct
            && arm.aggregates.is_empty()
            && arm.windows.is_empty()
            && arm.group_by.is_empty()
    };
    !inner.compounds.is_empty()
        && inner.order_by.is_empty()
        && inner.limit.is_none()
        && inner.offset.is_none()
        && plain(inner)
        && inner
            .compounds
            .iter()
            .all(|(op, arm)| matches!(op, CompoundOp::UnionAll) && plain(arm))
}

/// Lists the result columns of a derived table that hold an aggregate of an
/// enclosing query in some arm, and that the enclosing block does not read.
///
/// @param block - the enclosing block, with its sources
/// @param at - the derived table's position among its sources
/// @param reserved - the numbers reserved for aggregates of enclosing queries
fn unread_columns_with_aggregates(
    block: &BoundSelect,
    at: usize,
    reserved: &[usize],
) -> Vec<usize> {
    let Some(term) = block.sources.get(at) else {
        return Vec::new();
    };
    let SourceRows::Subquery(inner) = &term.rows else {
        return Vec::new();
    };
    if !drops_unread_columns(inner) {
        return Vec::new();
    }
    let used = block.columns_read(term.id);
    if used.opaque || used.rowid {
        return Vec::new();
    }
    (0..inner.columns.len())
        .filter(|index| !used.columns.contains(&(*index as u16)))
        .filter(|index| {
            std::iter::once(&**inner)
                .chain(inner.compounds.iter().map(|(_, arm)| arm))
                .any(|arm| {
                    arm.columns
                        .get(*index)
                        .is_some_and(|column| reads_reserved(&column.expr, reserved))
                })
        })
        .collect()
}

impl Binder<'_> {
    /// Replaces, in each derived table of a closed block, the columns that
    /// nothing reads and that hold an aggregate of an enclosing query.
    ///
    /// **Done before the aggregates the block's subqueries attached are
    /// settled**, because the settling decides by what the tree still reads.
    ///
    /// @param block - the block that has just closed, with its sources set
    pub(super) fn drop_unread_derived_columns(&self, block: &mut BoundSelect) {
        if self.outer.reserved_sources().is_empty() {
            return;
        }
        let reserved = self.outer.reserved_sources();
        for at in 0..block.sources.len() {
            let unread = unread_columns_with_aggregates(block, at, &reserved);
            let Some(term) = block.sources.get_mut(at) else {
                continue;
            };
            let SourceRows::Subquery(inner) = &mut term.rows else {
                continue;
            };
            for index in unread {
                null_column(inner, index);
                for (_, arm) in &mut inner.compounds {
                    null_column(arm, index);
                }
            }
        }
    }
}

/// Sets one result column of an arm to `NULL`, keeping its name.
///
/// @param arm - one arm of the derived table
/// @param index - the result column
fn null_column(arm: &mut BoundSelect, index: usize) {
    if let Some(column) = arm.columns.get_mut(index) {
        column.expr = BoundExpr::Null;
    }
}

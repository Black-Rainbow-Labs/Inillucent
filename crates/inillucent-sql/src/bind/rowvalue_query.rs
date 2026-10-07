//! `(a, b) IN (SELECT x, y ...)`: a row value tested against the rows of a
//! query.
//!
//! Invariant: **the answer is the one an `OR` of row equalities over the query's
//! rows gives, NULLs included, and it is written as `EXISTS` tests so that
//! nothing below the binder has a row value in it.** The result is true when
//! some row equals the operand in every part, NULL when none does and some row
//! compares as NULL (a NULL in a position where no other position already
//! differed), and false otherwise. An empty query is false even when the operand
//! holds NULLs. `NOT IN` is the same with true and false exchanged.
//!
//! The comparison is made inside the query's `WHERE` when the query is a plain
//! select, which lets the planner use its indexes. A query with a limit, a
//! compound, grouping or a window is read through a derived table instead,
//! because a `WHERE` is applied before those and would ask another question.

use inillucent_value::Collation;

use super::rowvalue::{equality_chain_under, query_arity_refusal};
use super::{
    block_over, subquery_table, Binder, BoundExpr, BoundResultColumn, BoundSelect, BoundSource,
    IndexChoice, SourceRows, SubqueryKind,
};
use crate::ast::{self, JoinKind};
use crate::diagnostic::ParseError;
use crate::lexer::Span;

impl Binder<'_> {
    /// Binds `(a, b) [NOT] IN (SELECT ...)`.
    ///
    /// @param lefts - the operand row's parts, bound
    /// @param select - the query on the right
    /// @param negated - whether `NOT IN` was written
    /// @param span - where the test was written
    pub(super) fn bind_row_in_query(
        &mut self,
        lefts: Vec<BoundExpr>,
        select: ast::SelectId,
        negated: bool,
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let block = self.bind_value_subquery(select, span)?;
        if block.columns.len() != lefts.len() {
            return Err(query_arity_refusal(block.columns.len(), lefts.len(), span));
        }
        let rule_columns: Vec<BoundExpr> = block
            .compounds
            .last()
            .map_or(&block.columns, |(_, arm)| &arm.columns)
            .iter()
            .map(|column| column.expr.clone())
            .collect();
        let lifted = liftable(&block);
        let (rows, rights) = if lifted {
            let rights = block
                .columns
                .iter()
                .map(|column| column.expr.clone())
                .collect();
            (block, rights)
        } else {
            self.read_through_derived_table(block)
        };
        let mut correlations = rows.correlations.clone();
        for left in &lefts {
            let mut used = Vec::new();
            left.sources_used(&mut used);
            for id in used {
                if !correlations.contains(&id) {
                    correlations.push(id);
                }
            }
        }
        // **A compound takes its rules from its last arm**, as `x IN (SELECT
        // ...)` does: SQLite holds the rightmost `Select` and reads each
        // part's affinity and collation off its result column.
        let rules: Vec<_> = lefts
            .iter()
            .zip(rule_columns.iter())
            .map(|(left, right)| super::comparison_rules(left, right))
            .collect();
        let rights = match lifted {
            true => rights,
            false => name_the_rule_collations(rights, &rules),
        };
        let equal = equality_chain_under(&lefts, &rights, &rules);
        let unknown = BoundExpr::IsNull {
            negated: false,
            operand: Box::new(equal.clone()),
        };
        let matched = self.exists_where(&rows, &correlations, Some(equal));
        let uncertain = self.exists_where(&rows, &correlations, Some(unknown));
        let (found, missing) = match negated {
            true => (0, 1),
            false => (1, 0),
        };
        Ok(BoundExpr::Case {
            operand: None,
            branches: vec![
                (matched, BoundExpr::Integer(found)),
                (uncertain, BoundExpr::Null),
            ],
            otherwise: Some(Box::new(BoundExpr::Integer(missing))),
            comparisons: Vec::new(),
        })
    }

    /// Wraps a query as a derived table and returns the block that reads it
    /// together with one column reference per result column.
    ///
    /// @param block - the bound query
    pub(super) fn read_through_derived_table(
        &mut self,
        block: BoundSelect,
    ) -> (BoundSelect, Vec<BoundExpr>) {
        let id = self.sources.len();
        let table = subquery_table(b"subquery", &[], &block);
        let columns: Vec<BoundExpr> = block
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| BoundExpr::Column {
                source: id,
                column: index as u16,
                slot: index as u16,
                affinity: block.column_affinity(index),
                collation: column.expr.collation().unwrap_or(Collation::Binary),
            })
            .collect();
        let correlations = block.correlations.clone();
        let source = BoundSource {
            index_hint: IndexChoice::Any,
            id,
            rows: SourceRows::Subquery(Box::new(block)),
            table: std::rc::Rc::new(table),
            alias: b"subquery".to_vec(),
            join: JoinKind::Comma,
            constraint: None,
            suppressed: Vec::new(),
            index_exprs: Vec::new(),
            written_schema: None,
            derived: Default::default(),
        };
        self.sources.push(source.clone());
        let mut wrapper = block_over(source, None, Vec::new());
        wrapper.correlations = correlations;
        (wrapper, columns)
    }

    /// Returns `EXISTS (SELECT 1 FROM ... WHERE <extra>)` over a copy of a block.
    ///
    /// @param block - the block whose rows are tested
    /// @param correlations - the outer FROM terms the copy reads
    /// @param extra - the term added to the block's `WHERE`
    fn exists_where(
        &mut self,
        block: &BoundSelect,
        correlations: &[usize],
        extra: Option<BoundExpr>,
    ) -> BoundExpr {
        let mut copy = block.clone();
        copy.columns = vec![BoundResultColumn {
            expr: BoundExpr::Integer(1),
            name: b"1".to_vec(),
            origin: None,
            declared_type: Vec::new(),
            written: None,
        }];
        copy.order_by.clear();
        copy.distinct = false;
        copy.correlations = correlations.to_vec();
        if let Some(extra) = extra {
            copy.filter = Some(match copy.filter.take() {
                Some(held) => BoundExpr::And(Box::new(held), Box::new(extra)),
                None => extra,
            });
        }
        BoundExpr::Subquery {
            id: self.next_subquery_id(),
            kind: SubqueryKind::Exists,
            negated: false,
            operand: None,
            block: Box::new(copy),
            affinity: None,
            collation: Collation::Binary,
        }
    }
}

/// Writes the collation each part is compared with onto the derived table's
/// column it is compared against, as an explicit `COLLATE`.
///
/// **The planner copies a condition on a derived table into each arm of a
/// compound, and recomputes a comparison's collation from the operands it finds
/// there** (see `refresh_comparison_rules` in `plan/pushdown.rs`). The rule
/// chosen above comes from the last arm, so a first arm that has no collation
/// of its own made the copy in that arm compare with BINARY:
/// `('a',1) IN (SELECT 'A', 1 UNION SELECT 'x' COLLATE NOCASE, 2)` answered 0
/// where SQLite answers 1. An explicit collation on the operand survives the
/// recomputation, and gives the same answer where nothing is copied.
///
/// @param rights - the derived table's columns, one per part
/// @param rules - the affinity and collation each part is compared with
fn name_the_rule_collations(
    rights: Vec<BoundExpr>,
    rules: &[(Option<inillucent_value::Affinity>, Collation)],
) -> Vec<BoundExpr> {
    rights
        .into_iter()
        .zip(rules.iter())
        .map(|(right, (_, collation))| BoundExpr::Collate {
            operand: Box::new(right),
            collation: *collation,
        })
        .collect()
}

/// Returns whether a block's `WHERE` decides the same rows the block reports.
///
/// Anything that reads the rows after the `WHERE` (grouping, a limit, a compound
/// arm, a window, a `VALUES` list) is asked a different question by a block with
/// one more term in its `WHERE`.
///
/// @param block - the query on the right of the `IN`
fn liftable(block: &BoundSelect) -> bool {
    block.group_by.is_empty()
        && block.having.is_none()
        && block.limit.is_none()
        && block.offset.is_none()
        && block.compounds.is_empty()
        && block.windows.is_empty()
        && block.aggregates.is_empty()
        && block.values.is_empty()
        && !block.sources.is_empty()
}

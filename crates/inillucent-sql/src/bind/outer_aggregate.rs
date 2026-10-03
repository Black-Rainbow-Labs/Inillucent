//! An aggregate written inside a subquery whose arguments read only the FROM
//! terms of an enclosing query.
//!
//! Invariant: **an aggregate belongs to the innermost query whose FROM terms its
//! arguments read (or to the innermost query when they read none), and a query
//! that owns an aggregate written in a subquery computes it before the subquery
//! is evaluated.** `SELECT a, (SELECT count(*) FROM u WHERE u.x < sum(t.b))
//! FROM t GROUP BY a` computes `sum(t.b)` for each group of `t` and then runs the
//! subquery with that value, which is what SQLite does. The subquery used to
//! register `sum(t.b)` in its own accumulators, which gave a wrong answer when
//! the subquery was not itself refused.
//!
//! ## How the binder carries it
//!
//! The binder cannot evaluate a subquery after an aggregation, because the
//! planner evaluates a correlated subquery beside each row it reads, before the
//! aggregate sees the row. So the owning query is rewritten once it is bound into
//! two queries that the planner already runs:
//!
//! - an inner query, with the FROM terms, `WHERE`, `GROUP BY` and every
//!   aggregate of the original, that returns one row per group holding each
//!   value the outer expressions read (the aggregates, the `GROUP BY` terms and
//!   the bare columns);
//! - an outer query over that one as a derived table, holding the original
//!   result columns, `HAVING` as its `WHERE`, `ORDER BY`, `DISTINCT`, `LIMIT` and
//!   `OFFSET`, with the subqueries reading the derived table's columns.
//!
//! A reference to an aggregate of the owning query, made inside a subquery, is
//! bound as a column of a FROM term number the binder reserves for that derived
//! table, so every pass that follows treats it as the ordinary correlation it
//! becomes.
//!
//! ## What is refused, and why
//!
//! SQLite reports `misuse of aggregate: f()` when the owning query reads the
//! aggregate where it computes none (its `WHERE`, its `ORDER BY` when nothing
//! else aggregates), and `misuse of aggregate function f()` when the subquery
//! that wrote the call would not allow an aggregate there. Both are raised here
//! with the same words. A window function in the owning query is refused by
//! name. An owner that is not a `SELECT` (the target of an `UPDATE` or `DELETE`,
//! a trigger body) keeps the earlier behavior of registering the call in the
//! subquery. A compound is supported: the first arm is rewritten with the
//! compound's `ORDER BY`, `LIMIT` and later arms moved onto the outer query, and
//! any other arm is rewritten on its own.

use std::rc::Rc;

use inillucent_value::{Affinity, Collation};

use super::aggregate::explicit_argument_collation;
use super::{
    block_over, refused, subquery_table, unsupported, Binder, BoundAggregate, BoundExpr,
    BoundResultColumn, BoundSelect, BoundSource, IndexChoice, SourceRows,
};
use crate::ast::{JoinKind, UnaryOp};
use crate::catalog_view::TableInfo;
use crate::diagnostic::ParseError;
use crate::lexer::Span;
use crate::rewrite::rewrite_select;

/// One aggregate an inner query asked an enclosing query to compute.
struct Attached {
    /// The call, bound in the inner query's scope.
    aggregate: BoundAggregate,
    /// The name as written, for the refusals.
    written: Vec<u8>,
    /// Where the call was written.
    span: Span,
    /// The enclosing query's accumulator number, once it has one.
    slot: Option<usize>,
}

/// What the subqueries of one enclosing query asked it to compute.
pub(super) struct OuterUse {
    /// The enclosing query's depth: how many scopes were open while it was the
    /// innermost.
    depth: usize,
    /// The statement-wide FROM term number reserved for the derived table that
    /// will hold the values.
    pub(super) source: usize,
    /// The aggregates, in the order the derived table's first columns hold them.
    attached: Vec<Attached>,
}

/// The binder's state for aggregates that belong to an enclosing query.
#[derive(Default)]
pub(super) struct OuterAggregates {
    /// One entry for each open query that a subquery has attached to.
    uses: Vec<OuterUse>,
    /// Bit `n` is set while the query opened at depth `n` is a plain `SELECT`
    /// that can own such an aggregate.
    statements: u128,
    /// The first aggregate of the block being bound that was written where the
    /// block allows none, kept until its result columns show whether it
    /// aggregates.
    pub(super) deferred: Option<(Vec<u8>, Span)>,
    /// Whether the refusal being returned was worded for the block that wrote
    /// the call, which an enclosing `WHERE` must not reword.
    pub(super) reported: bool,
}

/// Replaces the references a rewritten query makes to its own terms with
/// columns of the derived table that now holds their values.
struct Lowering {
    /// The statement-wide numbers of the original query's FROM terms.
    ids: Vec<usize>,
    /// The original query's `GROUP BY` terms.
    group_by: Vec<BoundExpr>,
    /// The number of the derived table.
    target: usize,
    /// The values the derived table holds, one per column.
    leaves: Vec<BoundExpr>,
}

impl Lowering {
    /// Reports whether an expression is a value the derived table holds.
    ///
    /// @param expr - an expression of the original query
    fn is_leaf(&self, expr: &BoundExpr) -> bool {
        match expr {
            BoundExpr::Aggregate { .. } => true,
            BoundExpr::Column { source, .. } | BoundExpr::Rowid { source } => {
                self.ids.contains(source)
            }
            other => self.group_by.contains(other),
        }
    }

    /// Returns the expression that reads a value from the derived table,
    /// adding the value as a column when it is not there yet.
    ///
    /// **It keeps what a comparison reads off the original.** An aggregate has
    /// no affinity, and the derived table's column would otherwise report BLOB,
    /// which compares differently from none. An explicit `COLLATE` inside the
    /// aggregate's arguments is written outside the reference.
    ///
    /// @param leaf - the original expression
    fn reference(&mut self, leaf: &BoundExpr) -> BoundExpr {
        let index = match self.leaves.iter().position(|held| held == leaf) {
            Some(index) => index,
            None => {
                self.leaves.push(leaf.clone());
                self.leaves.len().saturating_sub(1)
            }
        };
        let affinity = leaf.affinity();
        let column = BoundExpr::Column {
            source: self.target,
            column: index as u16,
            slot: index as u16,
            affinity: affinity.unwrap_or(Affinity::Blob),
            collation: leaf.collation().unwrap_or(Collation::Binary),
        };
        let held = match affinity {
            Some(_) => column,
            None => BoundExpr::Unary {
                op: UnaryOp::Identity,
                operand: Box::new(column),
            },
        };
        match leaf.explicit_collation() {
            Some(collation) => BoundExpr::Collate {
                operand: Box::new(held),
                collation,
            },
            None => held,
        }
    }

    /// Rewrites one expression of the original query's output in place.
    ///
    /// @param expr - the expression, read after the aggregation
    fn lower(&mut self, expr: &mut BoundExpr) {
        if self.is_leaf(expr) {
            let leaf = expr.clone();
            *expr = self.reference(&leaf);
            return;
        }
        if let Some(block) = expr.block_mut() {
            self.lower_block(block);
        }
        for child in expr.children_mut() {
            self.lower(child);
        }
    }

    /// Rewrites a subquery's reads of the original query's FROM terms.
    ///
    /// @param block - a subquery of the original query's output
    fn lower_block(&mut self, block: &mut BoundSelect) {
        rewrite_select(block, &mut |expr: &mut BoundExpr| {
            let reads_a_term = match expr {
                BoundExpr::Column { source, .. } | BoundExpr::Rowid { source } => {
                    self.ids.contains(source)
                }
                _ => false,
            };
            if reads_a_term {
                let leaf = expr.clone();
                *expr = self.reference(&leaf);
            }
            if let Some(inner) = expr.block_mut() {
                self.retarget(inner);
            }
        });
        self.retarget(block);
    }

    /// Names the derived table, in place of the original FROM terms, in the
    /// correlation list of a block and of every block it holds.
    ///
    /// @param block - the block to correct
    fn retarget(&self, block: &mut BoundSelect) {
        let mut fixed: Vec<usize> = Vec::with_capacity(block.correlations.len());
        for id in block.correlations.drain(..) {
            let id = if self.ids.contains(&id) {
                self.target
            } else {
                id
            };
            if !fixed.contains(&id) {
                fixed.push(id);
            }
        }
        block.correlations = fixed;
        for source in &mut block.sources {
            if let SourceRows::Subquery(inner) = &mut source.rows {
                self.retarget(inner);
            }
        }
        for (_, arm) in &mut block.compounds {
            self.retarget(arm);
        }
    }
}

/// Returns the reference an inner query holds to an aggregate of an enclosing
/// query before the enclosing query is rewritten.
///
/// @param source - the number reserved for the enclosing query's derived table
/// @param index - which column of it holds the aggregate
/// @param aggregate - the call
fn stand_in(source: usize, index: usize, aggregate: &BoundAggregate) -> BoundExpr {
    let column = BoundExpr::Column {
        source,
        column: index as u16,
        slot: index as u16,
        affinity: Affinity::Blob,
        collation: Collation::Binary,
    };
    // `+` hides the column's affinity, as an aggregate's result has none.
    let held = BoundExpr::Unary {
        op: UnaryOp::Identity,
        operand: Box::new(column),
    };
    match explicit_argument_collation(&aggregate.arguments) {
        Some(collation) => BoundExpr::Collate {
            operand: Box::new(held),
            collation,
        },
        None => held,
    }
}

/// Adds to a list the FROM terms an expression reads, not looking into a
/// subquery's block.
///
/// @param expr - the expression
/// @param into - the numbers read, with repeats
fn terms_read(expr: &BoundExpr, into: &mut Vec<usize>) {
    if let BoundExpr::Column { source, .. } | BoundExpr::Rowid { source } = expr {
        into.push(*source);
    }
    for child in expr.children() {
        terms_read(child, into);
    }
}

impl Binder<'_> {
    /// Reports whether an aggregate is written where the block being bound
    /// allows none.
    pub(super) fn aggregate_is_misplaced(&self) -> bool {
        !self.allow_aggregates || self.inside_aggregate || self.in_plain_order_by
    }

    /// Reports whether an aggregate written here could belong to an enclosing
    /// query, so its arguments have to be bound before it is refused.
    pub(super) fn aggregate_may_belong_outside(&self) -> bool {
        self.scopes.len() >= 2 && !self.inside_aggregate && !self.in_group_by
    }

    /// Returns how far out the query is that owns an aggregate: the depth of
    /// the innermost scope whose FROM terms its arguments read.
    ///
    /// SQLite's `sqlite3FunctionUsesThisSrc`: a query owns the call when the
    /// arguments read one of its terms, or when they read no term at all. An
    /// aggregate that reads a term of this query and a term of an enclosing one
    /// belongs to this query, and the enclosing term is a constant for the group.
    ///
    /// @param arguments - the call's bound arguments
    pub(super) fn aggregate_owner(&self, arguments: &[BoundExpr]) -> usize {
        let innermost = self.scopes.len().saturating_sub(1);
        if innermost == 0 {
            return innermost;
        }
        let mut used: Vec<usize> = Vec::new();
        for argument in arguments {
            terms_read(argument, &mut used);
        }
        for level in (0..self.scopes.len()).rev() {
            let Some(scope) = self.scopes.get(level) else {
                continue;
            };
            let here = used.iter().filter(|id| scope.contains(id)).count();
            if here > 0 || used.len() == here {
                return level;
            }
        }
        innermost
    }

    /// Reports whether the query at a scope level can own an aggregate that a
    /// subquery wrote, and the call does not read the subquery's own terms
    /// outside its arguments.
    ///
    /// @param level - the owner's scope level
    /// @param aggregate - the call
    pub(super) fn can_host_outer_aggregate(
        &self,
        level: usize,
        aggregate: &BoundAggregate,
    ) -> bool {
        let depth = level.saturating_add(1);
        if depth >= 128 || ((self.outer.statements >> depth) & 1) == 0 {
            return false;
        }
        let mut used: Vec<usize> = Vec::new();
        for expr in aggregate
            .filter
            .iter()
            .chain(aggregate.order_by.iter().map(|term| &term.expr))
        {
            terms_read(expr, &mut used);
        }
        // A `FILTER` or `ORDER BY` that reads a term of a query nested deeper
        // than the owner could not be evaluated by the owner.
        !used.iter().any(|id| {
            self.scopes
                .iter()
                .skip(depth)
                .any(|scope| scope.contains(id))
        })
    }

    /// Marks the query just opened as one that can own such an aggregate.
    ///
    /// @param depth - how many scopes are open now
    pub(super) fn open_statement(&mut self, depth: usize) {
        if depth < 128 {
            self.outer.statements |= 1u128 << depth;
        }
    }

    /// Clears the mark [`Binder::open_statement`] set.
    ///
    /// @param depth - the depth it was set for
    pub(super) fn close_statement(&mut self, depth: usize) {
        if depth < 128 {
            self.outer.statements &= !(1u128 << depth);
        }
    }

    /// Attaches an aggregate to the enclosing query that owns it, returning
    /// the reference the subquery reads its value through.
    ///
    /// @param level - the owner's scope level
    /// @param candidate - the bound call
    /// @param written - the function name as written
    /// @param span - where the call was written
    /// @param deferred - whether this block allows no aggregate where it was written
    pub(super) fn attach_outer_aggregate(
        &mut self,
        level: usize,
        candidate: BoundAggregate,
        written: &[u8],
        span: Span,
        deferred: bool,
    ) -> BoundExpr {
        if deferred && self.outer.deferred.is_none() {
            self.outer.deferred = Some((written.to_vec(), span));
        }
        let at = self.outer_use_at(level.saturating_add(1));
        let source = self.outer.uses.get(at).map_or(0, |held| held.source);
        let index = self.outer.uses.get_mut(at).map_or(0, |held| {
            match held.attached.iter().position(|a| a.aggregate == candidate) {
                Some(index) => index,
                None => {
                    held.attached.push(Attached {
                        aggregate: candidate.clone(),
                        written: written.to_vec(),
                        span,
                        slot: None,
                    });
                    held.attached.len().saturating_sub(1)
                }
            }
        });
        self.note_correlation(source);
        stand_in(source, index, &candidate)
    }

    /// Returns the position of the record for the query at a depth, making it
    /// and reserving its FROM term number the first time.
    ///
    /// @param depth - the query's depth
    fn outer_use_at(&mut self, depth: usize) -> usize {
        if let Some(at) = self.outer.uses.iter().position(|held| held.depth == depth) {
            return at;
        }
        let source = self.sources.len();
        self.sources.push(BoundSource {
            index_hint: IndexChoice::Any,
            id: source,
            rows: SourceRows::Table,
            table: Rc::new(TableInfo::subquery(Vec::new(), 0, Vec::new())),
            alias: Vec::new(),
            join: JoinKind::Comma,
            constraint: None,
            suppressed: Vec::new(),
            index_exprs: Vec::new(),
            written_schema: None,
        });
        self.outer.uses.push(OuterUse {
            depth,
            source,
            attached: Vec::new(),
        });
        self.outer.uses.len().saturating_sub(1)
    }

    /// Raises the refusal for an aggregate written in a block that allows none,
    /// after the block's result columns show it does not aggregate.
    ///
    /// @param grouped - whether the block has a `GROUP BY`
    pub(super) fn check_deferred_aggregates(&mut self, grouped: bool) -> Result<(), ParseError> {
        let Some((name, span)) = self.outer.deferred.take() else {
            return Ok(());
        };
        if grouped || !self.aggregates.is_empty() {
            return Ok(());
        }
        self.outer.reported = true;
        Err(self.aggregate_misuse(&name, span))
    }

    /// Gives the query that has just become the innermost the aggregates its
    /// subqueries attached, as accumulators of its own.
    ///
    /// **Called after a nested query closes**, because only then is the owner's
    /// accumulator list back in the binder and its flags say where in the owner
    /// the subquery was written. An aggregate in the owner's `WHERE`, or in an
    /// `ORDER BY` of a query that aggregates nothing else, is refused as SQLite
    /// refuses it.
    pub(super) fn settle_outer_aggregates(&mut self) -> Result<(), ParseError> {
        let depth = self.scopes.len();
        let Some(at) = self.outer.uses.iter().position(|held| held.depth == depth) else {
            return Ok(());
        };
        let pending: Vec<(usize, BoundAggregate, Vec<u8>, Span)> = self
            .outer
            .uses
            .get(at)
            .map(|held| {
                held.attached
                    .iter()
                    .enumerate()
                    .filter(|(_, attached)| attached.slot.is_none())
                    .map(|(index, a)| (index, a.aggregate.clone(), a.written.clone(), a.span))
                    .collect()
            })
            .unwrap_or_default();
        for (index, aggregate, written, span) in pending {
            if self.aggregate_is_misplaced() {
                return Err(self.misplaced_outer_aggregate(&written, span));
            }
            let BoundExpr::Aggregate { slot, .. } = self.aggregate_slot(aggregate) else {
                continue;
            };
            if let Some(attached) = self
                .outer
                .uses
                .get_mut(at)
                .and_then(|held| held.attached.get_mut(index))
            {
                attached.slot = Some(slot);
            }
        }
        Ok(())
    }

    /// Returns the refusal for an aggregate of this query that a subquery wrote
    /// where this query computes none.
    ///
    /// @param written - the function name as written
    /// @param span - where the call was written
    fn misplaced_outer_aggregate(&self, written: &[u8], span: Span) -> ParseError {
        if self.in_group_by {
            return self.aggregate_misuse(written, span);
        }
        refused(
            format!(
                "misuse of aggregate: {}()",
                String::from_utf8_lossy(written)
            ),
            span,
        )
    }

    /// Removes and returns the record of what subqueries attached to the query
    /// at a depth, if any did.
    ///
    /// @param depth - the query's depth
    pub(super) fn take_outer_use(&mut self, depth: usize) -> Option<OuterUse> {
        let at = self
            .outer
            .uses
            .iter()
            .position(|held| held.depth == depth)?;
        Some(self.outer.uses.remove(at))
    }

    /// Rewrites a query whose subqueries read its aggregates into the inner and
    /// outer pair the module comment describes.
    ///
    /// @param bound - the query, with its sources, aggregates and tail clauses
    /// @param used - the aggregates its subqueries attached
    pub(super) fn lower_outer_aggregates(
        &mut self,
        bound: BoundSelect,
        used: OuterUse,
    ) -> Result<BoundSelect, ParseError> {
        self.correlations.retain(|id| *id != used.source);
        if !bound.windows.is_empty() || !bound.values.is_empty() {
            return Err(unsupported(
                "an aggregate of an enclosing query inside a subquery, with a window function",
                Span::default(),
            ));
        }
        let mut leaves = Vec::with_capacity(used.attached.len());
        for attached in &used.attached {
            let Some(slot) = attached.slot else {
                return Err(unsupported(
                    "an aggregate of an enclosing query",
                    attached.span,
                ));
            };
            leaves.push(BoundExpr::Aggregate {
                slot,
                collation: explicit_argument_collation(&attached.aggregate.arguments),
            });
        }
        let mut lowering = Lowering {
            ids: bound.sources.iter().map(|source| source.id).collect(),
            group_by: bound.group_by.clone(),
            target: used.source,
            leaves,
        };
        let BoundSelect {
            sources,
            filter,
            group_by,
            having,
            mut columns,
            distinct,
            mut order_by,
            limit,
            offset,
            aggregates,
            compounds,
            mut correlations,
            ..
        } = bound;
        correlations.retain(|id| *id != used.source);
        for column in &mut columns {
            lowering.lower(&mut column.expr);
        }
        let having = having.map(|mut held| {
            lowering.lower(&mut held);
            held
        });
        // A compound's `ORDER BY` names result columns by position and reads
        // none of the first arm's terms, so it is left as it is.
        if compounds.is_empty() {
            for term in &mut order_by {
                lowering.lower(&mut term.expr);
            }
        }
        let inner = inner_query(
            sources,
            filter,
            group_by,
            aggregates,
            &lowering,
            &correlations,
        );
        let source = self.derived_term(used.source, inner);
        let mut outer = block_over(source, having, columns);
        outer.distinct = distinct;
        outer.order_by = order_by;
        outer.limit = limit;
        outer.offset = offset;
        outer.compounds = compounds;
        outer.correlations = correlations;
        Ok(outer)
    }

    /// Builds the derived table over a query, and records it as the FROM term
    /// number the inner subqueries were bound against.
    ///
    /// @param id - the reserved FROM term number
    /// @param inner - the query the derived table reads
    fn derived_term(&mut self, id: usize, inner: BoundSelect) -> BoundSource {
        let table = subquery_table(b"subquery", &[], &inner);
        let source = BoundSource {
            index_hint: IndexChoice::Any,
            id,
            rows: SourceRows::Subquery(Box::new(inner)),
            table: Rc::new(table),
            alias: b"subquery".to_vec(),
            join: JoinKind::Comma,
            constraint: None,
            suppressed: Vec::new(),
            index_exprs: Vec::new(),
            written_schema: None,
        };
        if let Some(held) = self.sources.get_mut(id) {
            *held = source.clone();
        }
        source
    }
}

/// Returns the inner query: the original FROM terms, `WHERE`, `GROUP BY` and
/// aggregates, returning each value the outer expressions read.
///
/// @param sources - the original FROM terms
/// @param filter - the original `WHERE`
/// @param group_by - the original `GROUP BY` terms
/// @param aggregates - every aggregate the original query computed
/// @param lowering - the values the derived table has to hold
/// @param correlations - the enclosing terms the original query read
fn inner_query(
    sources: Vec<BoundSource>,
    filter: Option<BoundExpr>,
    group_by: Vec<BoundExpr>,
    aggregates: Vec<BoundAggregate>,
    lowering: &Lowering,
    correlations: &[usize],
) -> BoundSelect {
    let columns = lowering
        .leaves
        .iter()
        .enumerate()
        .map(|(index, leaf)| BoundResultColumn {
            expr: leaf.clone(),
            name: format!("c{index}").into_bytes(),
            origin: None,
            declared_type: Vec::new(),
            written: None,
        })
        .collect();
    BoundSelect {
        sources,
        filter,
        group_by,
        having: None,
        columns,
        distinct: false,
        order_by: Vec::new(),
        limit: None,
        offset: None,
        aggregates,
        values: Vec::new(),
        compounds: Vec::new(),
        windows: Vec::new(),
        correlations: correlations.to_vec(),
        shared: None,
    }
}

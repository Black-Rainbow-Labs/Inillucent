//! The binder's working vectors: the set it borrows from the connection, and
//! the subset it saves while a nested block is bound.
//!
//! Invariant: **a vector in here is scratch, never part of an answer.** What
//! [`Binder::bind_statement`] returns owns its own vectors and leaves with
//! them; everything this module moves about is state the binder keeps while it
//! works and has no use for afterwards. That is what makes it safe to hand back
//! to the connection and fill again.
//!
//! ## Why this is its own module
//!
//! The same reason [`super::cte`] is: `bind.rs` was at the size `policy.rs`
//! records for it and task-2026 added to it, and that check asks for an
//! extraction rather than a raised number. The two halves here are one
//! question - which of the binder's vectors are saved, and who they are given
//! back to - asked at two scales.
//!
//! [`BinderScratch`] is the outer one: the vectors a *connection* keeps
//! between statements, so a compile pushes into capacity the last compile
//! took. [`BlockFrame`] is the inner one: the vectors a *query block* borrows
//! from the block around it, so an aggregate written inside a subquery is
//! finalised at its own level rather than the enclosing one. Nothing moved
//! changed in the move.

use super::{
    Binder, BoundAggregate, BoundExpr, BoundSource, BoundWindow, CteBinding, RecursiveTarget,
};
use crate::ast;

/// The vectors a binder works in, kept by the connection rather than made for
/// every statement.
///
/// **The parse arena's counterpart, for the stage after the parse
/// (task-2026).** `Compiled::scratch_ast` exists because every vector in an
/// `Ast` is empty at construction and grows on its first push, so a parse that
/// is thrown away a microsecond later pays the allocator for capacity it
/// already had last time. The binder has exactly that shape and was not getting
/// that treatment: binding `SELECT 1` took the scope stack's buffer and the
/// result-alias buffer - 96 and 320 bytes - out of the allocator on every
/// compile, and `prepare.trivial` is a workload the gate compiles on every
/// iteration.
///
/// What is here is the binder's own working state. What the binder *returns* is
/// not: a `BoundStatement`'s vectors leave with it and belong to whoever asked
/// for the bind, so recycling them would mean handing back memory something
/// else is still reading.
///
/// Like the arena, it is cleared on the way in rather than on the way out, and
/// it keeps whatever capacity the largest statement so far needed - a
/// connection that once bound a statement with ten thousand FROM terms holds
/// that much `sources` until it closes. That is the trade [`crate::ast::Ast`]
/// already makes, made once more here rather than differently.
#[derive(Default)]
pub struct BinderScratch {
    sources: Vec<BoundSource>,
    scopes: Vec<Vec<usize>>,
    aggregates: Vec<BoundAggregate>,
    result_aliases: Vec<(Vec<u8>, BoundExpr)>,
    schemas: Vec<(usize, u32)>,
    ctes: Vec<Vec<CteBinding>>,
    recursing: Vec<RecursiveTarget>,
    binding_ctes: Vec<(usize, ast::SelectId)>,
    correlations: Vec<usize>,
    windows: Vec<BoundWindow>,
    named_windows: Vec<(Vec<u8>, ast::WindowId)>,
    firing: Vec<Vec<u8>>,
    firing_foreign_keys: Vec<Vec<u8>>,
    pending_constraints: Vec<BoundExpr>,
}

impl BinderScratch {
    /// Returns a scratch with nothing in it and nothing allocated.
    pub fn new() -> BinderScratch {
        BinderScratch::default()
    }

    /// Empties every vector, keeping the memory each has already taken.
    ///
    /// A `clear` rather than a `new` for the reason [`crate::ast::Ast::clear`]
    /// gives: the capacity is the point, and dropping it would leave a scratch
    /// that costs an allocation to refill.
    fn clear(&mut self) {
        self.sources.clear();
        self.scopes.clear();
        self.aggregates.clear();
        self.result_aliases.clear();
        self.schemas.clear();
        self.ctes.clear();
        self.recursing.clear();
        self.binding_ctes.clear();
        self.correlations.clear();
        self.windows.clear();
        self.named_windows.clear();
        self.firing.clear();
        self.firing_foreign_keys.clear();
        self.pending_constraints.clear();
    }
}

/// The per-block binder state saved while a nested block is bound.
///
/// Aggregates, result aliases and the correlation list all belong to one query
/// block. Without a frame, an aggregate written inside a subquery would be
/// added to the enclosing block's accumulator list and finalised at the wrong
/// level - which is a wrong answer rather than an error.
pub(super) struct BlockFrame {
    windows: Vec<BoundWindow>,
    named_windows: Vec<(Vec<u8>, ast::WindowId)>,
    aggregates: Vec<BoundAggregate>,
    result_aliases: Vec<(Vec<u8>, BoundExpr)>,
    allow_aggregates: bool,
    allow_windows: bool,
    in_group_by: bool,
    in_plain_order_by: bool,
    inside_aggregate: bool,
    correlations: Vec<usize>,
    tail_may_name_an_alias: bool,
    deferred: Option<(Vec<u8>, crate::lexer::Span)>,
    /// The table function argument constraints the enclosing block has bound
    /// and not yet put in its `WHERE`.
    ///
    /// **Per block, like the correlations.** They name the enclosing block's
    /// FROM terms, and a derived table bound after a table function took them
    /// into its own `WHERE`: `SELECT count(*) FROM json_each('[1,2]'),
    /// (SELECT 1 AS x)` failed with "the tree read for FROM term 0 does not
    /// carry column 8", because the subquery's pipeline has no term 0.
    pending_constraints: Vec<BoundExpr>,
}

impl<'a> Binder<'a> {
    /// Binds into vectors the caller keeps, rather than into fresh ones.
    ///
    /// **For a caller that binds one statement after another**, which is every
    /// connection: the scratch is cleared on the way in, so the second bind
    /// pushes into capacity the first one took. See [`BinderScratch`] for what
    /// is in it and what is deliberately not.
    ///
    /// @param scratch - the vectors to fill, cleared first
    pub fn with_scratch(mut self, mut scratch: BinderScratch) -> Binder<'a> {
        scratch.clear();
        self.sources = scratch.sources;
        self.scopes = scratch.scopes;
        self.aggregates = scratch.aggregates;
        self.result_aliases = scratch.result_aliases;
        self.dependencies.schemas = scratch.schemas;
        self.ctes = scratch.ctes;
        self.recursing = scratch.recursing;
        self.binding_ctes = scratch.binding_ctes;
        self.correlations = scratch.correlations;
        self.windows = scratch.windows;
        self.named_windows = scratch.named_windows;
        self.firing = scratch.firing;
        self.firing_foreign_keys = scratch.firing_foreign_keys;
        self.pending_constraints = scratch.pending_constraints;
        self
    }

    /// Returns the vectors this bind filled, for the next bind to reuse.
    ///
    /// The binder is consumed, so nothing can still be reading what is handed
    /// back. The bound statement is not in here - it was returned by
    /// [`Binder::bind_statement`] and owns its own vectors.
    pub fn into_scratch(self) -> BinderScratch {
        BinderScratch {
            sources: self.sources,
            scopes: self.scopes,
            aggregates: self.aggregates,
            result_aliases: self.result_aliases,
            schemas: self.dependencies.schemas,
            ctes: self.ctes,
            recursing: self.recursing,
            binding_ctes: self.binding_ctes,
            correlations: self.correlations,
            windows: self.windows,
            named_windows: self.named_windows,
            firing: self.firing,
            firing_foreign_keys: self.firing_foreign_keys,
            pending_constraints: self.pending_constraints,
        }
    }

    /// Binds the query of a derived table, which cannot name the terms beside it.
    ///
    /// **A derived table sees the queries around its block and not the other
    /// FROM terms of its own block (bug hunt 10).** `FROM b AS m, (SELECT c.z
    /// FROM c WHERE c.w <= m.y) AS d` is `no such column: m.y` in SQLite, and
    /// it was bound against `m` here and failed when it ran with "the tree
    /// read for FROM term 1 does not carry column 0". The block's scope stays
    /// on the stack, emptied while the query is bound, so a correlation to a
    /// block further out is still found and still passed outward. A
    /// parenthesised join is the same: `FROM a, (b JOIN c ON c.z = a.x)` is
    /// `no such column: a.x`.
    ///
    /// @param select - the derived table's query
    pub(super) fn bind_derived_select(
        &mut self,
        select: ast::SelectId,
    ) -> Result<super::BoundSelect, super::ParseError> {
        let siblings = self.scopes.last_mut().map(core::mem::take);
        let bound = self.bind_select(select);
        if let (Some(scope), Some(slot)) = (siblings, self.scopes.last_mut()) {
            *slot = scope;
        }
        bound
    }

    /// Binds every arm after the head, in a scope that does not hold the head's terms.
    ///
    /// **An arm cannot name the head arm's FROM terms (bug hunt 10).** The head
    /// arm's scope is still the innermost one while the other arms are bound,
    /// so `SELECT 1 FROM t1 UNION ALL SELECT b FROM (SELECT 1 AS x)` resolved
    /// `b` to `t1.b`, and failed at run time instead of at prepare time with
    /// "no such column: b". The scope stays on the stack, emptied while the
    /// arms are bound, so a correlation to an enclosing block is still passed
    /// outward exactly as before.
    ///
    /// @param select - the statement as written
    /// @param bound - the head arm, which the other arms are added to
    pub(super) fn bind_other_arms(
        &mut self,
        select: &ast::Select,
        bound: &mut super::BoundSelect,
    ) -> Result<(), super::ParseError> {
        let head_scope = self.scopes.last_mut().map(core::mem::take);
        let armed = self.bind_arms_in_order(select, bound.columns.len());
        if let (Some(scope), Some(slot)) = (head_scope, self.scopes.last_mut()) {
            *slot = scope;
        }
        bound.compounds.extend(armed?);
        Ok(())
    }

    /// Binds each compound arm of a statement and checks its width.
    ///
    /// @param select - the statement as written
    /// @param width - how many result columns the head arm has
    fn bind_arms_in_order(
        &mut self,
        select: &ast::Select,
        width: usize,
    ) -> Result<Vec<(ast::CompoundOp, super::BoundSelect)>, super::ParseError> {
        let mut arms = Vec::with_capacity(select.compounds.len());
        for (op, arm) in &select.compounds {
            let armed = self.bind_isolated_arm(*arm)?;
            if armed.columns.len() != width {
                return Err(super::refusal::compound_width_mismatch(*op, select.span));
            }
            arms.push((*op, armed));
        }
        Ok(arms)
    }

    /// Opens a query block: a fresh scope, and fresh per-block state.
    pub(super) fn enter_block(&mut self) -> BlockFrame {
        self.scopes.push(Vec::new());
        BlockFrame {
            windows: core::mem::take(&mut self.windows),
            named_windows: core::mem::take(&mut self.named_windows),
            aggregates: core::mem::take(&mut self.aggregates),
            result_aliases: core::mem::take(&mut self.result_aliases),
            allow_aggregates: core::mem::replace(&mut self.allow_aggregates, false),
            allow_windows: core::mem::replace(&mut self.allow_windows, false),
            in_group_by: core::mem::replace(&mut self.in_group_by, false),
            in_plain_order_by: core::mem::replace(&mut self.in_plain_order_by, false),
            inside_aggregate: core::mem::replace(&mut self.inside_aggregate, false),
            correlations: core::mem::take(&mut self.correlations),
            tail_may_name_an_alias: self.tail_may_name_an_alias,
            deferred: self.outer.deferred.take(),
            pending_constraints: core::mem::take(&mut self.pending_constraints),
        }
    }

    /// Closes a query block, returning the FROM terms it owned.
    ///
    /// A correlation the closing block recorded is passed outward when the
    /// block that is now innermost does not own the term either, which is what
    /// makes correlation transitive through two levels of nesting.
    pub(super) fn leave_block(&mut self, frame: BlockFrame) -> Vec<usize> {
        let ids = self.scopes.pop().unwrap_or_default();
        let inner = core::mem::replace(&mut self.correlations, frame.correlations);
        for id in inner {
            if ids.contains(&id) {
                continue;
            }
            let owned = self.scopes.last().is_some_and(|scope| scope.contains(&id));
            if !owned && !self.correlations.contains(&id) {
                self.correlations.push(id);
            }
        }
        self.windows = frame.windows;
        self.named_windows = frame.named_windows;
        self.aggregates = frame.aggregates;
        self.result_aliases = frame.result_aliases;
        self.allow_aggregates = frame.allow_aggregates;
        self.allow_windows = frame.allow_windows;
        self.pending_constraints = frame.pending_constraints;
        self.in_group_by = frame.in_group_by;
        self.in_plain_order_by = frame.in_plain_order_by;
        self.inside_aggregate = frame.inside_aggregate;
        self.tail_may_name_an_alias = frame.tail_may_name_an_alias;
        self.outer.deferred = frame.deferred;
        ids
    }
}

impl Binder<'_> {
    /// Takes the table function argument constraints that belong in the `WHERE`.
    ///
    /// An argument such as `json_each(t.tags)` constrains the function's own
    /// term, so on the right side of a `LEFT JOIN` it is part of that join's
    /// `ON`. In the `WHERE` it would reject the null extended row of an outer
    /// row the function returned nothing for. Those are added to the term's
    /// `ON`; the rest are returned.
    pub(super) fn pending_for_the_where(&mut self) -> Vec<BoundExpr> {
        let pending = core::mem::take(&mut self.pending_constraints);
        let mut for_where = Vec::with_capacity(pending.len());
        for constraint in pending {
            let owner = match &constraint {
                BoundExpr::Compare { left, .. } => match **left {
                    BoundExpr::Column { source, .. } => Some(source),
                    _ => None,
                },
                _ => None,
            };
            // **Every outer join, not only `LEFT`.** The planner seeks an outer
            // join's term on its own `ON` alone, so an argument left in the
            // `WHERE` never reached the function: `a RIGHT JOIN json_each('[..]')`
            // returned no rows and `RIGHT JOIN generate_series(1, 2)` failed
            // with "first argument missing". Every row a table function returns
            // satisfies its arguments, so in the `ON` they null extend nothing.
            let left_joined = owner
                .and_then(|id| self.sources.get_mut(id))
                .filter(|source| crate::plan::is_outer(source.join));
            match left_joined {
                Some(source) => {
                    source.constraint = Some(match source.constraint.take() {
                        Some(existing) => BoundExpr::And(Box::new(existing), Box::new(constraint)),
                        None => constraint,
                    });
                }
                None => for_where.push(constraint),
            }
        }
        for_where
    }
}

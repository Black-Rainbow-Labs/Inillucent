//! How a call to an aggregate becomes a reference to one accumulator, and how
//! a call to a function the application registered is bound.
//!
//! Invariant: **every `BoundExpr::Aggregate` the binder makes comes from
//! [`Binder::aggregate_slot`], so the same aggregate written twice names one
//! slot and every reference carries its arguments' explicit collation.** The
//! reference holds no arguments, because they live in the binder's aggregate
//! list, and `max(s COLLATE NOCASE) = 'C'` compared with BINARY until the
//! reference carried the collation (task-2094).
//!
//! ## Why this is its own module
//!
//! The same reason [`super::cte`] and [`super::scratch`] are: `bind.rs` was at
//! the size `policy.rs` records for it and task-2094 added to it, and that
//! check asks for an extraction rather than a raised number. A registered
//! function is the other half of this module because a registered aggregate is
//! the second place a call becomes an accumulator. Nothing changed in the move.

use inillucent_value::Collation;

use super::{no_such_function, refused, wrong_arguments, Binder, BoundAggregate, BoundExpr};
use crate::ast::{self, ExprId};
use crate::diagnostic::ParseError;
use crate::function::{self, AggregateFunc};
use crate::lexer::Span;

/// Returns the explicit collation a call's arguments carry, left first.
///
/// This is what an aggregate or a window reference answers for
/// [`BoundExpr::explicit_collation`]. SQLite reads an aggregate call's
/// collation from its first argument with `EP_Collate`, so
/// `max(s COLLATE NOCASE) = 'C'` and `group_concat(s, ',' COLLATE NOCASE) =
/// 'A,B,C,A,B,C'` both compare with NOCASE and 3.53.4 answers each with 1.
/// Before task-2094 the reference hid its arguments and both answered 0.
///
/// @param arguments - the call's bound arguments, in the order written
pub(super) fn explicit_argument_collation(arguments: &[BoundExpr]) -> Option<Collation> {
    arguments.iter().find_map(BoundExpr::explicit_collation)
}

impl Binder<'_> {
    /// Binds a call to a function an application registered, if there is one.
    ///
    /// Registered functions are consulted before the built-ins, which is what
    /// makes `sqlite3_create_function("upper", 1, ...)` replace `upper` rather
    /// than collide with it - the same order SQLite resolves in.
    pub(super) fn bind_external_call(
        &mut self,
        folded: &[u8],
        written: &[u8],
        arguments: &[ExprId],
        distinct: bool,
        span: Span,
    ) -> Result<Option<BoundExpr>, ParseError> {
        let Some(found) = function::lookup_external(self.externals, folded, arguments.len()) else {
            return Ok(None);
        };
        // **Where a schema is stopped from choosing what code runs
        // (task-1972).** The rule is `inillucent-sql`'s own, and
        // `Registry::authorize_function` reads the same one over the same
        // flags, so an application that asks the registry directly and a
        // statement the binder compiles get the same answer.
        if let Some(why) =
            function::schema_refusal(found.flags, self.call_site, self.trusted_schema)
        {
            return Err(refused(
                format!("{} {why}", String::from_utf8_lossy(folded)),
                span,
            ));
        }
        let aggregate = found.aggregate;
        if !aggregate {
            // `DISTINCT` means nothing to a function that sees one row, and
            // SQLite ignores it, so a registered scalar function ignores it too.
            let mut bound = Vec::with_capacity(arguments.len());
            for argument in arguments {
                bound.push(self.bind_expr(*argument)?);
            }
            return Ok(Some(BoundExpr::External {
                name: folded.to_vec(),
                arguments: bound,
            }));
        }
        let misplaced = self.aggregate_is_misplaced();
        if misplaced && !self.aggregate_may_belong_outside() {
            return Err(self.aggregate_misuse(written, span));
        }
        self.inside_aggregate = true;
        let saved_windows = core::mem::replace(&mut self.allow_windows, false);
        let bound = self.bind_aggregate_arguments(arguments);
        self.inside_aggregate = false;
        self.allow_windows = saved_windows;
        let bound = match (bound, misplaced) {
            (Ok(bound), _) => bound,
            (Err(_), true) => return Err(self.aggregate_misuse(written, span)),
            (Err(error), false) => return Err(error),
        };
        let collation = bound
            .first()
            .and_then(BoundExpr::collation)
            .unwrap_or(Collation::Binary);
        let candidate = BoundAggregate {
            func: AggregateFunc::External,
            external: Some(folded.to_vec()),
            distinct,
            arguments: bound,
            star: false,
            collation,
            // A registered aggregate reaches this path without a `FILTER` or an
            // `ORDER BY`; both are read where a built-in is bound.
            filter: None,
            order_by: Vec::new(),
        };
        self.place_aggregate(candidate, misplaced, written, span)
            .map(Some)
    }

    /// Binds a call to a built-in aggregate function.
    ///
    /// **Where the call is placed is decided after its arguments are bound.**
    /// The query that owns an aggregate is the innermost one whose FROM terms
    /// the arguments read, so `(SELECT max(t.a))` inside a query over `t`
    /// computes `max(t.a)` for `t` and not for the subquery. A call written
    /// where its own block allows none is refused unless it turns out to belong
    /// to an enclosing query; see [`super::outer_aggregate`].
    ///
    /// @param name - the function name
    /// @param distinct - whether `DISTINCT` was written
    /// @param star - whether the call was written `f(*)`
    /// @param list - the arguments as written
    /// @param filter - the `FILTER (WHERE ...)` clause, when one was written
    /// @param order_by - the `ORDER BY` inside the argument list
    /// @param span - where the call was written
    pub(super) fn bind_aggregate_call(
        &mut self,
        name: ast::NameId,
        distinct: bool,
        star: bool,
        list: &[ExprId],
        filter: Option<ExprId>,
        order_by: &[ast::OrderTerm],
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let folded = self.ast.folded(name).to_vec();
        let written = self.ast.text(name).to_vec();
        let Some(func) =
            function::lookup_aggregate(&folded).or_else(|| function::minmax_aggregate(&folded))
        else {
            return Err(no_such_function(&written, span));
        };
        let misplaced = self.aggregate_is_misplaced();
        if misplaced && !self.aggregate_may_belong_outside() {
            return Err(self.aggregate_misuse(&written, span));
        }
        if star && func != AggregateFunc::Count {
            return Err(wrong_arguments(&folded, span));
        }
        // **`count()` with no argument counts rows**, as `count(*)` does.
        // SQLite gives `count` a zero argument form and no other aggregate.
        let star = star || (func == AggregateFunc::Count && list.is_empty());
        if !function::aggregate_arity_ok(func, if star { 0 } else { list.len() }, star) {
            return Err(wrong_arguments(&folded, span));
        }
        self.inside_aggregate = true;
        let saved_windows = core::mem::replace(&mut self.allow_windows, false);
        let bound = self.bind_aggregate_arguments(list);
        self.inside_aggregate = false;
        let bound = match (bound, misplaced) {
            (Ok(bound), _) => bound,
            // The call is refused for where it was written, which is what the
            // failure to bind its arguments would have hidden.
            (Err(_), true) => {
                self.allow_windows = saved_windows;
                return Err(self.aggregate_misuse(&written, span));
            }
            (Err(error), false) => return Err(error),
        };
        // The `FILTER` and the `ORDER BY` read the row the aggregate is
        // folding, so they bind in the same scope the arguments did - and
        // outside `inside_aggregate`, because neither may itself contain
        // an aggregate.
        self.inside_aggregate = true;
        let bound_filter = match filter {
            Some(expr) => Some(self.bind_expr(expr)?),
            None => None,
        };
        let bound_order = self.bind_aggregate_order(order_by)?;
        self.inside_aggregate = false;
        self.allow_windows = saved_windows;
        let collation = bound
            .first()
            .and_then(BoundExpr::collation)
            .unwrap_or(Collation::Binary);
        let func = self.vector_aggregate(func, &bound);
        // **`DISTINCT` takes exactly one argument (task-1913).** SQLite
        // answers `DISTINCT aggregates must have exactly one argument`,
        // and this accepted `group_concat(DISTINCT s, ',')` and answered
        // it - a statement the reference cannot read, which is the same
        // class `refusals_match_the_oracle` exists to stop. There is
        // nothing for the second argument to be distinct *by*: the
        // de-duplication compares the first value alone, so the separator
        // of whichever duplicate arrived first is the one that survives.
        if distinct && bound.len() > 1 {
            return Err(refused(
                "DISTINCT aggregates must have exactly one argument",
                span,
            ));
        }
        let candidate = BoundAggregate {
            func,
            external: None,
            distinct,
            arguments: bound,
            star,
            collation,
            filter: bound_filter,
            order_by: bound_order,
        };
        self.place_aggregate(candidate, misplaced, &written, span)
    }

    /// Binds the arguments of an aggregate call, in the aggregate's own scope.
    ///
    /// @param arguments - the arguments as written
    fn bind_aggregate_arguments(
        &mut self,
        arguments: &[ExprId],
    ) -> Result<Vec<BoundExpr>, ParseError> {
        let mut bound = Vec::with_capacity(arguments.len());
        for argument in arguments {
            bound.push(self.bind_expr(*argument)?);
        }
        Ok(bound)
    }

    /// Chooses the vector form of `sum` and `avg` when an argument is a vector.
    ///
    /// The same reason `v + v` refuses: `sum(v)` and `avg(v)` coerced the blob
    /// through numeric affinity and answered `0.0` for a whole column of
    /// embeddings. pgvector defines them as element-wise, and this is that,
    /// chosen here where the argument's type is known rather than at run time
    /// where a blob is just a blob.
    ///
    /// @param func - the aggregate the name spells
    /// @param bound - the bound arguments
    fn vector_aggregate(&self, func: AggregateFunc, bound: &[BoundExpr]) -> AggregateFunc {
        let vector = bound.iter().any(|argument| self.reads_a_vector(argument));
        match func {
            AggregateFunc::Sum | AggregateFunc::Total if vector => AggregateFunc::VectorSum,
            AggregateFunc::Avg if vector => AggregateFunc::VectorAvg,
            other => other,
        }
    }

    /// Puts a bound aggregate where it belongs: in this block's accumulators,
    /// or in those of the enclosing query that owns it.
    ///
    /// @param candidate - the bound call
    /// @param misplaced - whether the call was written where this block allows none
    /// @param written - the function name as written
    /// @param span - where the call was written
    pub(super) fn place_aggregate(
        &mut self,
        candidate: BoundAggregate,
        misplaced: bool,
        written: &[u8],
        span: Span,
    ) -> Result<BoundExpr, ParseError> {
        let level = self.aggregate_owner(&candidate.arguments);
        let outside = level.saturating_add(1) < self.scopes.len()
            && self.can_host_outer_aggregate(level, &candidate);
        if outside {
            let deferred = !self.allow_aggregates;
            return Ok(self.attach_outer_aggregate(level, candidate, written, span, deferred));
        }
        if misplaced {
            return Err(self.aggregate_misuse(written, span));
        }
        Ok(self.aggregate_slot(candidate))
    }

    /// Builds the refusal for an aggregate written where one cannot be.
    ///
    /// SQLite words it differently inside a `GROUP BY`, and names the function
    /// as it was written everywhere else, so `Sum(a)` is reported as `Sum()`.
    ///
    /// @param written - the function name as written
    /// @param span - where the call was written
    pub(super) fn aggregate_misuse(&self, written: &[u8], span: Span) -> ParseError {
        if self.in_group_by {
            return refused(
                "aggregate functions are not allowed in the GROUP BY clause",
                Span::default(),
            );
        }
        if self.in_plain_order_by {
            return refused(
                format!(
                    "misuse of aggregate: {}()",
                    String::from_utf8_lossy(written)
                ),
                span,
            );
        }
        refused(
            format!(
                "misuse of aggregate function {}()",
                String::from_utf8_lossy(written)
            ),
            span,
        )
    }

    /// Returns the reference to an aggregate's accumulator, adding the
    /// accumulator when the block does not have it yet.
    ///
    /// The same aggregate written twice is one accumulator. It is not only
    /// cheaper: `... ORDER BY count(*)` has to name the *same* slot the result
    /// column named, or the two are different values that happen to be spelt
    /// alike. The reference also carries the explicit collation of the
    /// arguments, which is the only part of them a comparison around the call
    /// reads (task-2094).
    ///
    /// @param candidate - the bound aggregate call
    pub(super) fn aggregate_slot(&mut self, candidate: BoundAggregate) -> BoundExpr {
        let collation = explicit_argument_collation(&candidate.arguments);
        let slot = match self
            .aggregates
            .iter()
            .position(|existing| existing == &candidate)
        {
            Some(slot) => slot,
            None => {
                self.aggregates.push(candidate);
                self.aggregates.len().saturating_sub(1)
            }
        };
        BoundExpr::Aggregate { slot, collation }
    }
}

//! Methods of `BoundExpr` that walk, classify and describe a bound expression.
//!
//! Invariant: **every method here reads a `BoundExpr` and changes nothing but
//! the `children_mut` walk, and each lists operands in the order SQLite reads
//! them.** They live apart from the type so the binder's own file stays below
//! its recorded size.

use super::*;

impl BoundExpr {
    /// Returns the affinity this expression has as an operand.
    ///
    /// SQLite's rule: a column has its own affinity, a cast has the cast's, a
    /// parenthesised expression has its operand's, and everything else has
    /// none. "None" is a real answer here, not a missing one.
    pub fn affinity(&self) -> Option<Affinity> {
        match self {
            BoundExpr::Column { affinity, .. } => Some(*affinity),
            BoundExpr::Cast { affinity, .. } => Some(*affinity),
            BoundExpr::Generated { affinity, .. } => Some(*affinity),
            BoundExpr::Rowid { .. } => Some(Affinity::Integer),
            BoundExpr::Collate { operand, .. } => operand.affinity(),
            // SQLite gives a scalar subquery the affinity of its first result
            // column, so `(SELECT a FROM t) = 3` over a TEXT column `a` applies
            // TEXT affinity to the 3.
            BoundExpr::Subquery {
                kind: SubqueryKind::Scalar,
                block,
                ..
            } => block.scalar_affinity(),
            _ => None,
        }
    }

    /// Returns whether the expression reads any column or aggregate.
    pub fn is_constant(&self) -> bool {
        match self {
            BoundExpr::Null
            | BoundExpr::Integer(_)
            | BoundExpr::Real(_)
            | BoundExpr::Text(_)
            | BoundExpr::Blob(_)
            | BoundExpr::Parameter(_) => true,
            // RAISE never produces a value, so it is not constant: folding it
            // away would delete the abort it exists to perform.
            BoundExpr::Raise { .. }
            | BoundExpr::Column { .. }
            | BoundExpr::Generated { .. }
            | BoundExpr::Rowid { .. }
            | BoundExpr::External { .. }
            | BoundExpr::VirtualFunction { .. }
            | BoundExpr::Aggregate { .. }
            | BoundExpr::WindowRef { .. }
            | BoundExpr::SorterColumn { .. } => false,
            BoundExpr::Unary { operand, .. } => operand.is_constant(),
            BoundExpr::Collate { operand, .. } => operand.is_constant(),
            BoundExpr::Json { arguments, .. } => arguments.iter().all(BoundExpr::is_constant),
            BoundExpr::Not(operand) => operand.is_constant(),
            BoundExpr::IsNull { operand, .. } => operand.is_constant(),
            BoundExpr::Cast { operand, .. } => operand.is_constant(),
            BoundExpr::Arithmetic { left, right, .. }
            | BoundExpr::Compare { left, right, .. }
            | BoundExpr::Is { left, right, .. } => left.is_constant() && right.is_constant(),
            BoundExpr::And(left, right) | BoundExpr::Or(left, right) => {
                left.is_constant() && right.is_constant()
            }
            BoundExpr::Between {
                operand, low, high, ..
            } => operand.is_constant() && low.is_constant() && high.is_constant(),
            BoundExpr::InList { operand, list, .. } => {
                operand.is_constant() && list.iter().all(BoundExpr::is_constant)
            }
            BoundExpr::Case {
                operand,
                branches,
                otherwise,
                ..
            } => {
                operand.as_ref().is_none_or(|e| e.is_constant())
                    && branches
                        .iter()
                        .all(|(when, then)| when.is_constant() && then.is_constant())
                    && otherwise.as_ref().is_none_or(|e| e.is_constant())
            }
            BoundExpr::Pattern {
                operand,
                pattern,
                escape,
                ..
            } => {
                operand.is_constant()
                    && pattern.is_constant()
                    && escape.as_ref().is_none_or(|e| e.is_constant())
            }
            BoundExpr::Function { arguments, .. }
            | BoundExpr::Math { arguments, .. }
            | BoundExpr::Time { arguments, .. } => arguments.iter().all(BoundExpr::is_constant),
            // A subquery is never constant. It may read no column of the query
            // that encloses it, but it reads the database, and hoisting it out
            // of a loop is the compiler's decision to make from its correlation
            // list rather than one this predicate can make.
            BoundExpr::Subquery { .. } => false,
        }
    }

    /// Returns which declared column positions the expression reads.
    ///
    /// The declared position rather than the record slot, because the callers
    /// that ask - a generated column's dependency order, and the index-key
    /// matcher - both think in declared positions.
    pub fn columns_used(&self, into: &mut Vec<u16>) {
        if let BoundExpr::Column { column, .. } = self {
            if !into.contains(column) {
                into.push(*column);
            }
        }
        for child in self.children() {
            child.columns_used(into);
        }
    }

    /// Returns every sub-expression one expression holds, in no order.
    ///
    /// The match is exhaustive on purpose: there is no `_` arm, so a variant
    /// added later is a compilation error here rather than a silently unvisited
    /// subtree. That matters because the covering-index decision is built on
    /// this walk, and a missed subtree there would be a column read from an
    /// index that does not hold it.
    ///
    /// A subquery's *block* is deliberately not a child. It is a query of its
    /// own with its own FROM terms, and the only thing about it that concerns
    /// an enclosing term is which of that term's columns it correlates to -
    /// which the block records separately and which the caller reads.
    pub fn children(&self) -> Vec<&BoundExpr> {
        match self {
            BoundExpr::Null
            | BoundExpr::Integer(_)
            | BoundExpr::Real(_)
            | BoundExpr::Text(_)
            | BoundExpr::Blob(_)
            | BoundExpr::Parameter(_)
            | BoundExpr::Raise { computed: None, .. }
            | BoundExpr::Column { .. }
            | BoundExpr::Rowid { .. }
            | BoundExpr::WindowRef { .. }
            | BoundExpr::Aggregate { .. }
            | BoundExpr::SorterColumn { .. } => Vec::new(),
            BoundExpr::Unary { operand, .. }
            | BoundExpr::Not(operand)
            | BoundExpr::IsNull { operand, .. }
            | BoundExpr::Collate { operand, .. }
            | BoundExpr::Cast { operand, .. }
            | BoundExpr::Raise {
                computed: Some(operand),
                ..
            } => vec![operand],
            BoundExpr::Arithmetic { left, right, .. }
            | BoundExpr::Compare { left, right, .. }
            | BoundExpr::Is { left, right, .. }
            | BoundExpr::And(left, right)
            | BoundExpr::Or(left, right) => vec![left, right],
            BoundExpr::Generated {
                operand, present, ..
            } => vec![operand, present],
            BoundExpr::Between {
                operand, low, high, ..
            } => vec![operand, low, high],
            BoundExpr::InList { operand, list, .. } => {
                let mut found: Vec<&BoundExpr> = vec![operand];
                found.extend(list.iter());
                found
            }
            BoundExpr::Case {
                operand,
                branches,
                otherwise,
                ..
            } => {
                let mut found: Vec<&BoundExpr> = Vec::new();
                if let Some(operand) = operand {
                    found.push(operand);
                }
                for (when, then) in branches {
                    found.push(when);
                    found.push(then);
                }
                if let Some(otherwise) = otherwise {
                    found.push(otherwise);
                }
                found
            }
            BoundExpr::Pattern {
                operand,
                pattern,
                escape,
                ..
            } => {
                let mut found: Vec<&BoundExpr> = vec![operand, pattern];
                if let Some(escape) = escape {
                    found.push(escape);
                }
                found
            }
            BoundExpr::External { arguments, .. }
            | BoundExpr::VirtualFunction { arguments, .. }
            | BoundExpr::Function { arguments, .. }
            | BoundExpr::Math { arguments, .. }
            | BoundExpr::Json { arguments, .. }
            | BoundExpr::Time { arguments, .. } => arguments.iter().collect(),
            BoundExpr::Subquery { operand, .. } => operand.iter().map(|held| &**held).collect(),
        }
    }

    /// Returns every sub-expression one expression holds, mutably.
    ///
    /// The mirror of [`BoundExpr::children`], and exhaustive for the same
    /// reason: a variant added later is a compilation error here rather than a
    /// subtree some rewrite silently skips. `crate::rewrite` is the only caller
    /// and the trigger firing point is why it exists - a body's `OLD` and `NEW`
    /// reads are replaced by the values the row actually holds, and one missed
    /// subtree there is a trigger that reads a NULL where a value was.
    ///
    /// A subquery's *block* is not a child here either, for the reason it is
    /// not one there: it is a query of its own. `crate::rewrite` descends into
    /// it separately, because a correlated block is exactly where a foreign
    /// key's `NOT EXISTS (SELECT 1 FROM parent WHERE p.k = NEW.c)` keeps its
    /// `NEW`.
    pub fn children_mut(&mut self) -> Vec<&mut BoundExpr> {
        match self {
            BoundExpr::Null
            | BoundExpr::Integer(_)
            | BoundExpr::Real(_)
            | BoundExpr::Text(_)
            | BoundExpr::Blob(_)
            | BoundExpr::Parameter(_)
            | BoundExpr::Raise { computed: None, .. }
            | BoundExpr::Column { .. }
            | BoundExpr::Rowid { .. }
            | BoundExpr::WindowRef { .. }
            | BoundExpr::Aggregate { .. }
            | BoundExpr::SorterColumn { .. } => Vec::new(),
            BoundExpr::Unary { operand, .. }
            | BoundExpr::Not(operand)
            | BoundExpr::IsNull { operand, .. }
            | BoundExpr::Collate { operand, .. }
            | BoundExpr::Cast { operand, .. }
            | BoundExpr::Raise {
                computed: Some(operand),
                ..
            } => vec![operand],
            BoundExpr::Arithmetic { left, right, .. }
            | BoundExpr::Compare { left, right, .. }
            | BoundExpr::Is { left, right, .. }
            | BoundExpr::And(left, right)
            | BoundExpr::Or(left, right) => vec![left, right],
            BoundExpr::Generated {
                operand, present, ..
            } => vec![operand, present],
            BoundExpr::Between {
                operand, low, high, ..
            } => vec![operand, low, high],
            BoundExpr::InList { operand, list, .. } => {
                let mut found: Vec<&mut BoundExpr> = vec![operand];
                found.extend(list.iter_mut());
                found
            }
            BoundExpr::Case {
                operand,
                branches,
                otherwise,
                ..
            } => {
                let mut found: Vec<&mut BoundExpr> = Vec::new();
                if let Some(operand) = operand {
                    found.push(operand);
                }
                for (when, then) in branches {
                    found.push(when);
                    found.push(then);
                }
                if let Some(otherwise) = otherwise {
                    found.push(otherwise);
                }
                found
            }
            BoundExpr::Pattern {
                operand,
                pattern,
                escape,
                ..
            } => {
                let mut found: Vec<&mut BoundExpr> = vec![operand, pattern];
                if let Some(escape) = escape {
                    found.push(escape);
                }
                found
            }
            BoundExpr::External { arguments, .. }
            | BoundExpr::VirtualFunction { arguments, .. }
            | BoundExpr::Function { arguments, .. }
            | BoundExpr::Math { arguments, .. }
            | BoundExpr::Json { arguments, .. }
            | BoundExpr::Time { arguments, .. } => arguments.iter_mut().collect(),
            BoundExpr::Subquery { operand, .. } => {
                operand.iter_mut().map(|held| &mut **held).collect()
            }
        }
    }

    /// Returns the block a subquery expression holds, when it is one.
    ///
    /// Separate from [`BoundExpr::children_mut`] because a block is not a
    /// sub-expression: it is a query, with its own FROM terms and its own
    /// scope. A rewrite that treats it as one would run over the wrong tree.
    pub fn block_mut(&mut self) -> Option<&mut BoundSelect> {
        match self {
            BoundExpr::Subquery { block, .. } => Some(block),
            _ => None,
        }
    }

    /// Records which of one FROM term's columns this expression reads.
    ///
    /// A correlated subquery makes the answer unknowable from here - the block
    /// is a query of its own and could read any column of the term it
    /// correlates to - so it is recorded as opaque rather than guessed at.
    /// @param source - the FROM term to look for
    /// @param into - what has been found so far
    pub fn columns_read(&self, source: usize, into: &mut ColumnUse) {
        match self {
            BoundExpr::Column {
                source: held, slot, ..
            } if *held == source => into.add(*slot),
            BoundExpr::Rowid { source: held } if *held == source => into.rowid = true,
            BoundExpr::Subquery { block, .. } if block.correlations.contains(&source) => {
                into.opaque = true;
            }
            BoundExpr::VirtualFunction {
                source: held,
                name,
                arguments,
            } if *held == source => into.add_function(name, arguments),
            _ => {}
        }
        for child in self.children() {
            child.columns_read(source, into);
        }
    }
}

impl BoundExpr {
    /// Returns which FROM terms the expression reads.
    pub fn sources_used(&self, into: &mut Vec<usize>) {
        match self {
            BoundExpr::Column { source, .. } | BoundExpr::Rowid { source }
                if !into.contains(source) =>
            {
                into.push(*source);
            }
            BoundExpr::Unary { operand, .. }
            | BoundExpr::Not(operand)
            | BoundExpr::IsNull { operand, .. }
            | BoundExpr::Collate { operand, .. }
            | BoundExpr::Cast { operand, .. }
            | BoundExpr::Raise {
                computed: Some(operand),
                ..
            } => operand.sources_used(into),
            BoundExpr::Arithmetic { left, right, .. }
            | BoundExpr::Compare { left, right, .. }
            | BoundExpr::Is { left, right, .. }
            | BoundExpr::And(left, right)
            | BoundExpr::Or(left, right) => {
                left.sources_used(into);
                right.sources_used(into);
            }
            BoundExpr::Between {
                operand, low, high, ..
            } => {
                operand.sources_used(into);
                low.sources_used(into);
                high.sources_used(into);
            }
            BoundExpr::InList { operand, list, .. } => {
                operand.sources_used(into);
                for item in list {
                    item.sources_used(into);
                }
            }
            BoundExpr::Case {
                operand,
                branches,
                otherwise,
                ..
            } => {
                if let Some(operand) = operand {
                    operand.sources_used(into);
                }
                for (when, then) in branches {
                    when.sources_used(into);
                    then.sources_used(into);
                }
                if let Some(otherwise) = otherwise {
                    otherwise.sources_used(into);
                }
            }
            BoundExpr::Pattern {
                operand,
                pattern,
                escape,
                ..
            } => {
                operand.sources_used(into);
                pattern.sources_used(into);
                if let Some(escape) = escape {
                    escape.sources_used(into);
                }
            }
            // **A JSON call and a registered function's call read their
            // arguments' terms too.** Both were missing here, so `i.id =
            // c.value ->> '$.id'` looked like it read no term: the planner put
            // `i` first and sought it with a key that reads `c`, which had not
            // been read yet, and the statement failed with "a seek key or range
            // bound reads a column".
            BoundExpr::Function { arguments, .. }
            | BoundExpr::Math { arguments, .. }
            | BoundExpr::Time { arguments, .. }
            | BoundExpr::Json { arguments, .. }
            | BoundExpr::External { arguments, .. } => {
                for argument in arguments {
                    argument.sources_used(into);
                }
            }
            BoundExpr::Generated {
                operand, present, ..
            } => {
                operand.sources_used(into);
                present.sources_used(into);
            }
            BoundExpr::VirtualFunction {
                source, arguments, ..
            } => {
                if !into.contains(source) {
                    into.push(*source);
                }
                for argument in arguments {
                    argument.sources_used(into);
                }
            }
            BoundExpr::Subquery { operand, block, .. } => {
                if let Some(operand) = operand {
                    operand.sources_used(into);
                }
                // The block's correlations are terms of the *enclosing* query,
                // so they decide which loop level the subquery can first be
                // evaluated at. Leaving them out put a correlated `EXISTS`
                // before the loop whose row it reads.
                for source in &block.correlations {
                    if !into.contains(source) {
                        into.push(*source);
                    }
                }
            }
            _ => {}
        }
    }
}

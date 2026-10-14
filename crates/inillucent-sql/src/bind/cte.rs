//! Common table expressions: what a `WITH` binds, and how a recursive one is
//! filled.
//!
//! Invariant: **a CTE is bound once per reference and never bound inside
//! itself.** Two references to one CTE are two independent scans with their
//! own FROM-term numbers, which is why a binding holds an AST id rather than a
//! bound block; and a definition already being bound is a cycle, which is
//! answered rather than followed.
//!
//! ## Why this is its own module
//!
//! `bind.rs` was at its recorded ceiling and task-1913 added ninety-nine lines
//! to it, so the ratchet in `policy.rs` asked for an extraction rather than a
//! raised number. This is one question - what a name in a `WITH` stands for -
//! and the ten items here were the only ones asking it. Nothing moved changed
//! in the move.

use super::{
    subquery_table, unsupported, Binder, BoundExpr, BoundSource, RecursiveBody, SourceRows,
};
use crate::ast::{self, CompoundOp, JoinKind, SelectId};
use crate::catalog_view::TableInfo;
use crate::diagnostic::{ParseError, ParseErrorKind};
use crate::lexer::Span;
use inillucent_value::Affinity;

/// The first number a derived table inside a correlated subquery keeps its rows
/// under, which is past any number a common table expression can have.
pub const FIRST_ANONYMOUS_SHARED: usize = 1 << 20;

/// One common table expression visible to a block.
///
/// The definition is kept as an AST id rather than a bound block because two
/// references to the same CTE are two independent scans: each gets its own
/// FROM-term numbers and its own materialisation. Binding once and cloning
/// would give both references the same source ids, and the second scan would
/// then read the first one's cursors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CteBinding {
    /// The folded name a FROM term matches against.
    pub folded: Vec<u8>,
    /// The name as written, which the expansion is aliased to.
    pub name: Vec<u8>,
    /// The explicit column list, when the `WITH` wrote one.
    pub columns: Vec<Vec<u8>>,
    /// The query the name stands for.
    pub select: SelectId,
    /// Whether the `WITH` said `RECURSIVE`.
    pub recursive: bool,
    /// `Some(true)` for `MATERIALIZED`, `Some(false)` for `NOT MATERIALIZED`.
    pub materialized: Option<bool>,
    /// The position of the `WITH` that defined it in the stack of `WITH`
    /// levels, which is how far down the body of the definition may look.
    pub level: usize,
}

/// One recursive CTE whose definition is being bound.
#[derive(Clone, Debug)]
pub(super) struct RecursiveTarget {
    /// The CTE's folded name.
    pub(super) folded: Vec<u8>,
    /// The CTE's name as the `WITH` wrote it, for an error message.
    name: Vec<u8>,
    /// The statement-wide number of the FROM term that will hold its store.
    id: usize,
    /// The columns a reference to it exposes, taken from the seed arm.
    table: TableInfo,
    /// Whether any arm bound so far referred to it.
    referenced: bool,
    /// The affinity each column has when a step arm reads it, `None` when it has none.
    ///
    /// SQLite gives the recursive table the affinity a compound select gives its column, which
    /// reads every arm and not only the seed (see `Binder::step_column_affinity`).
    affinities: Vec<Option<Affinity>>,
}

/// A recursive CTE's compound arms, bound, each with the operator before it.
type BoundArms = Vec<(CompoundOp, crate::bind::BoundSelect)>;

/// Returns the affinity each column of a recursive CTE has once every arm is bound.
///
/// @param seed - the first arm
/// @param extra_seeds - the other arms that do not read the CTE
/// @param steps - the arms that do
fn settled_affinities(
    seed: &crate::bind::BoundSelect,
    extra_seeds: &BoundArms,
    steps: &BoundArms,
) -> Vec<Option<Affinity>> {
    let mut probe = seed.clone();
    probe.compounds = extra_seeds.iter().chain(steps.iter()).cloned().collect();
    (0..seed.columns.len())
        .map(|column| probe.column_affinity_if_any(column))
        .collect()
}

impl Binder<'_> {
    /// Pushes the CTEs of a `WITH` prefix, returning whether it pushed any.
    pub(crate) fn push_ctes(&mut self, with: &ast::With) -> Result<bool, ParseError> {
        if with.ctes.is_empty() {
            return Ok(false);
        }
        let mut bindings = Vec::with_capacity(with.ctes.len());
        for cte in &with.ctes {
            // **A name may be defined once in one `WITH`.** SQLite refuses the
            // second definition while it parses, in these words; an inner
            // `WITH` is a different clause and may reuse the name.
            let folded = self.ast.folded(cte.name);
            if bindings
                .iter()
                .any(|held: &CteBinding| held.folded.as_slice() == folded)
            {
                return Err(super::refused(
                    format!(
                        "duplicate WITH table name: {}",
                        String::from_utf8_lossy(self.ast.text(cte.name))
                    ),
                    crate::lexer::Span::default(),
                ));
            }
            bindings.push(CteBinding {
                folded: self.ast.folded(cte.name).to_vec(),
                name: self.ast.text(cte.name).to_vec(),
                columns: cte
                    .columns
                    .iter()
                    .map(|name| self.ast.text(*name).to_vec())
                    .collect(),
                select: cte.select,
                recursive: with.recursive,
                materialized: cte.materialized,
                level: self.ctes.len(),
            });
        }
        self.ctes.push(bindings);
        Ok(true)
    }

    /// Drops the innermost level of CTE bindings.
    pub(crate) fn pop_ctes(&mut self) {
        self.ctes.pop();
    }

    /// Returns the innermost CTE a folded name matches.
    pub(super) fn find_cte(&self, folded: &[u8]) -> Option<CteBinding> {
        for level in self.ctes.iter().rev() {
            if let Some(found) = level.iter().find(|cte| cte.folded == folded) {
                return Some(found.clone());
            }
        }
        None
    }

    /// Reports whether the statement refers to a name in more than one FROM term.
    ///
    /// Counted over every FROM term the statement was parsed into, so an inner
    /// `WITH` that reuses the name is counted too. That only ever shares a CTE
    /// that did not need to be shared.
    ///
    /// @param folded - the folded name
    pub(super) fn name_is_used_twice(&self, folded: &[u8]) -> bool {
        self.name_uses(folded) > 1
    }

    /// Counts the FROM terms of the statement that name a table.
    ///
    /// Counted over every FROM term the statement was parsed into, as
    /// [`Binder::name_is_used_twice`] counts them.
    ///
    /// @param folded - the folded name
    pub(super) fn name_uses(&self, folded: &[u8]) -> u32 {
        let mut uses = 0u32;
        for at in 0..self.ast.from_term_count() {
            let Some(term) = self.ast.from_term(ast::FromTermId(at as u32)) else {
                continue;
            };
            if let ast::FromSource::Table {
                database: None,
                name,
                ..
            } = &term.source
            {
                if self.ast.folded(*name) == folded {
                    uses = uses.saturating_add(1);
                }
            }
        }
        uses
    }

    /// Marks the block a CTE reference was just bound to as one that shares its
    /// evaluation with the other references, when that can be seen.
    ///
    /// SQLite evaluates a CTE used more than once a single time unless it is
    /// `NOT MATERIALIZED`. The difference can only be seen when the body is not
    /// a function of its tables, so only a body that calls `random()` or a
    /// function like it is marked, and one that reads a column of an enclosing
    /// query is left alone because it has a different answer for every row.
    ///
    /// @param cte - the definition
    pub(super) fn share_last_source(&mut self, cte: &CteBinding) {
        if cte.materialized == Some(false) || !self.name_is_used_twice(&cte.folded) {
            return;
        }
        let key = match self.shared_ctes.iter().position(|(arena, select)| {
            *arena == self.ast as *const _ as usize && *select == cte.select
        }) {
            Some(key) => key,
            None => {
                self.shared_ctes
                    .push((self.ast as *const _ as usize, cte.select));
                self.shared_ctes.len().saturating_sub(1)
            }
        };
        let Some(source) = self.sources.last_mut() else {
            return;
        };
        let SourceRows::Subquery(block) = &mut source.rows else {
            return;
        };
        if !block.correlations.is_empty() {
            return;
        }
        let mut volatile = false;
        let mut probe = (**block).clone();
        crate::rewrite::rewrite_select(&mut probe, &mut |expr: &mut super::BoundExpr| {
            if crate::plan::calls_a_volatile_function(expr) {
                volatile = true;
            }
        });
        if volatile {
            block.shared = Some(key);
        }
    }

    /// Makes the derived tables of a correlated subquery that read nothing of
    /// the enclosing query keep their rows for the whole statement.
    ///
    /// **SQLite materialises such a derived table once** (`OP_Once`), however
    /// many outer rows the subquery runs for. Read again for every row, a
    /// `SELECT ... FROM (SELECT sum(v) OVER () ...)` inside an `UPDATE`'s `SET`
    /// saw the rows the statement had already changed.
    ///
    /// @param block - a subquery that reads a column of an enclosing query
    pub(super) fn share_uncorrelated_sources(&mut self, block: &mut super::BoundSelect) {
        for source in &mut block.sources {
            let SourceRows::Subquery(inner) = &mut source.rows else {
                continue;
            };
            if inner.correlations.is_empty() {
                if inner.shared.is_none() {
                    inner.shared = Some(FIRST_ANONYMOUS_SHARED + self.shared_anonymous);
                    self.shared_anonymous = self.shared_anonymous.saturating_add(1);
                }
            } else {
                self.share_uncorrelated_sources(inner);
            }
        }
        for (_, arm) in &mut block.compounds {
            self.share_uncorrelated_sources(arm);
        }
    }

    /// Reports whether a CTE's own query names it in a FROM clause.
    ///
    /// **What makes a CTE recursive is the self-reference, not the keyword.**
    /// SQLite accepts `WITH c AS (SELECT 1 UNION ALL SELECT ... FROM c)` with
    /// no `RECURSIVE` written and answers it; this binder read only the
    /// keyword, so the same query bound `c`'s definition inside `c`'s
    /// definition until the process ran out of stack (task-1913).
    ///
    /// An inner `WITH` that binds the same name shadows the outer one, so
    /// nothing under it can be the recursion - which is why this stops there
    /// rather than reporting every mention of the name.
    ///
    /// @param select - the CTE's query
    /// @param folded - the CTE's folded name
    pub(super) fn select_names_itself(&self, select: ast::SelectId, folded: &[u8]) -> bool {
        let Some(query) = self.ast.select(select) else {
            return false;
        };
        if query
            .with
            .ctes
            .iter()
            .any(|inner| self.ast.folded(inner.name) == folded)
        {
            return false;
        }
        if self.core_names_cte(query.first, folded) {
            return true;
        }
        query
            .compounds
            .iter()
            .any(|(_, arm)| self.core_names_cte(*arm, folded))
    }

    /// Reports whether one arm of a compound names a CTE in its FROM clause.
    ///
    /// @param core - the arm
    /// @param folded - the CTE's folded name
    pub(super) fn core_names_cte(&self, core: ast::SelectCoreId, folded: &[u8]) -> bool {
        let Some(arm) = self.ast.core(core) else {
            return false;
        };
        let ast::SelectBody::Select { from, .. } = &arm.body else {
            return false;
        };
        self.terms_name_cte(from, folded)
    }

    /// Reports whether any FROM term names a CTE.
    ///
    /// @param terms - the FROM terms
    /// @param folded - the CTE's folded name
    pub(super) fn terms_name_cte(&self, terms: &[ast::FromTermId], folded: &[u8]) -> bool {
        terms.iter().any(|id| match self.ast.from_term(*id) {
            Some(term) => match &term.source {
                ast::FromSource::Table { database, name, .. } => {
                    database.is_none() && self.ast.folded(*name) == folded
                }
                ast::FromSource::Subquery(select) => self.select_names_itself(*select, folded),
                ast::FromSource::Join(inner) => self.terms_name_cte(inner, folded),
            },
            None => false,
        })
    }

    /// Counts the FROM terms of one compound arm that name a CTE directly, not through a subquery.
    ///
    /// @param core - the arm
    /// @param folded - the CTE's folded name
    fn core_direct_references(&self, core: ast::SelectCoreId, folded: &[u8]) -> usize {
        match self.ast.core(core).map(|arm| &arm.body) {
            Some(ast::SelectBody::Select { from, .. }) => {
                self.terms_direct_references(from, folded)
            }
            _ => 0,
        }
    }

    /// Counts the FROM terms that name a CTE, looking inside parenthesized joins but not inside subqueries.
    ///
    /// @param terms - the FROM terms
    /// @param folded - the CTE's folded name
    fn terms_direct_references(&self, terms: &[ast::FromTermId], folded: &[u8]) -> usize {
        terms
            .iter()
            .map(
                |id| match self.ast.from_term(*id).map(|term| &term.source) {
                    Some(ast::FromSource::Table { database, name, .. }) => {
                        usize::from(database.is_none() && self.ast.folded(*name) == folded)
                    }
                    Some(ast::FromSource::Join(inner)) => {
                        self.terms_direct_references(inner, folded)
                    }
                    _ => 0,
                },
            )
            .sum()
    }

    /// Refuses the shapes of recursive CTE that SQLite refuses before it binds a name.
    ///
    /// **SQLite takes the CTE as recursive only when its last arm reads the CTE in its own
    /// FROM clause and every operator from there back is `UNION` or `UNION ALL`.** A CTE that
    /// names itself in any other way is a `circular reference`: in the first arm, in an arm
    /// before a non recursive last arm, under another set operator, or only inside a derived
    /// table. An arm that lists the CTE twice in its FROM clause is `multiple references to
    /// recursive table`. This was accepted and run, which for the last arm of
    /// `SELECT 1 UNION ALL SELECT n+1 FROM r UNION ALL SELECT 100` meant the recursion fed
    /// the steps and the extra seed through the same queue.
    ///
    /// @param cte - the expression being bound
    /// @param arms - the compound arms after the first, with their operators
    /// @param direct - for each arm, how many FROM terms name the CTE directly
    fn refuse_invalid_recursion(
        &self,
        cte: &CteBinding,
        arms: &[(CompoundOp, ast::SelectCoreId)],
        direct: &[usize],
    ) -> Result<(), ParseError> {
        if !self.select_names_itself(cte.select, &cte.folded) {
            return Ok(());
        }
        let last_op = arms.last().map(|(op, _)| *op);
        let last_is_step = matches!(last_op, Some(CompoundOp::Union | CompoundOp::UnionAll))
            && direct.last().copied().unwrap_or(0) > 0;
        if !last_is_step {
            return Err(circular_reference(&cte.name));
        }
        for ((op, _), count) in arms.iter().zip(direct).rev() {
            if Some(*op) != last_op {
                break;
            }
            if *count > 1 {
                return Err(ParseError::new(
                    ParseErrorKind::Refused(format!(
                        "multiple references to recursive table: {}",
                        String::from_utf8_lossy(&cte.name)
                    )),
                    Span::default(),
                ));
            }
        }
        Ok(())
    }

    /// Returns the affinity a comparison applies for a column of a recursive CTE read from
    /// inside one of its own step arms, or `None` when the expression is not such a column.
    ///
    /// **The column has the affinity a compound select gives it, which reads every arm.**
    /// Measured against 3.53.4 with `b.v` an untyped column holding `1`, `'1'`, `'1.0'` and
    /// `1.0`: `s.x = b.v` in the step matches all four when the seed is `SELECT 1` and the step
    /// is `SELECT k` over an INTEGER key (both arms are numbers and one has INTEGER affinity),
    /// and matches only `1` and `1.0` when the step is `SELECT k+0` or the seed is `SELECT '1'`.
    /// The binder read the column as one with a declared type of nothing, which applies
    /// nothing to the other side, so `n.parent = s.x` with a TEXT `parent` compared the text
    /// `'1'` with the integer 1 and found them different (bug hunt 10).
    ///
    /// @param expr - the operand of a comparison
    pub(super) fn step_column_affinity(&self, expr: &BoundExpr) -> Option<Option<Affinity>> {
        let BoundExpr::Column { source, column, .. } = expr else {
            return None;
        };
        let held = self.sources.iter().find(|held| held.id == *source)?;
        let SourceRows::RecursiveSelf { cte } = held.rows else {
            return None;
        };
        let target = self.recursing.iter().find(|target| target.id == cte)?;
        target.affinities.get(usize::from(*column)).copied()
    }

    /// Registers a reference to the recursive CTE currently being bound.
    pub(super) fn push_recursive_self(
        &mut self,
        position: usize,
        alias: Option<ast::NameId>,
        join: JoinKind,
    ) -> Result<(), ParseError> {
        let Some(target) = self.recursing.get_mut(position) else {
            return Err(unsupported("unknown recursive reference", Span::default()));
        };
        // **One arm reads the recursive table once.** The direct references in a
        // FROM clause are counted before anything is bound, so a second one
        // that arrives here is inside a subquery of the arm, which SQLite
        // refuses as `multiple recursive references`.
        if target.referenced {
            return Err(ParseError::new(
                ParseErrorKind::Refused(format!(
                    "multiple recursive references: {}",
                    String::from_utf8_lossy(&target.name)
                )),
                Span::default(),
            ));
        }
        target.referenced = true;
        let cte = target.id;
        let table = target.table.clone();
        let alias = match alias {
            Some(alias) => self.ast.text(alias).to_vec(),
            None => table.name.clone(),
        };
        let id = self.sources.len();
        self.sources.push(BoundSource {
            index_hint: crate::bind::IndexChoice::Any,
            id,
            rows: SourceRows::RecursiveSelf { cte },
            table: std::rc::Rc::new(table),
            alias,
            join,
            constraint: None,
            suppressed: Vec::new(),
            index_exprs: Vec::new(),
            written_schema: None,
            derived: Default::default(),
        });
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(id);
        }
        Ok(())
    }

    /// Binds a `WITH RECURSIVE` CTE reference.
    ///
    /// The seed arm is bound first, alone, because until it is bound nothing
    /// knows what columns the CTE has - and the step arm cannot be bound until
    /// a reference to the CTE has columns to resolve against. A CTE declared
    /// `RECURSIVE` that turns out not to reference itself is an ordinary
    /// compound, and is rebuilt as one rather than run through a queue that
    /// would never be fed.
    pub(super) fn bind_recursive_cte(
        &mut self,
        cte: &CteBinding,
        alias: Vec<u8>,
        join: JoinKind,
        span: Span,
    ) -> Result<(), ParseError> {
        let Some(select) = self.ast.select(cte.select) else {
            return Err(unsupported("missing select", span));
        };
        if select.compounds.is_empty() {
            return self.bind_subquery_term(
                cte.select,
                Some(alias),
                cte.columns.clone(),
                join,
                span,
            );
        }
        let arms: Vec<(CompoundOp, ast::SelectCoreId)> = select.compounds.clone();
        let direct: Vec<usize> = arms
            .iter()
            .map(|(_, arm)| self.core_direct_references(*arm, &cte.folded))
            .collect();
        self.refuse_invalid_recursion(cte, &arms, &direct)?;
        let order_by = select.order_by.clone();
        let limit = select.limit;
        let offset = select.offset;
        let first = select.first;

        let id = self.sources.len();
        // The store's FROM-term number is reserved before anything is bound, so
        // that a self-reference inside the step arm can name the store it will
        // read without the two being bound in an impossible order.
        self.sources.push(BoundSource {
            index_hint: crate::bind::IndexChoice::Any,
            id,
            rows: SourceRows::Table,
            table: std::rc::Rc::new(TableInfo::subquery(alias.clone(), 0, Vec::new())),
            alias: alias.clone(),
            join,
            constraint: None,
            suppressed: Vec::new(),
            index_exprs: Vec::new(),
            written_schema: None,
            derived: Default::default(),
        });

        let seed = self.bind_isolated_arm(first)?;
        let table = subquery_table(&alias, &cte.columns, &seed);
        named_columns_fit(&alias, &cte.columns, seed.columns.len(), span)?;
        // **The step arms are bound against the seed's affinities first, and again when the
        // other arms change them.** A step arm compares the recursive table's column, and the
        // column's affinity is the one a compound select gives it, which depends on the step
        // arms that are not bound yet. The first pass finds them; a second pass binds the steps
        // with them when they differ from what the seed alone gave (bug hunt 10).
        let provisional: Vec<Option<Affinity>> = (0..seed.columns.len())
            .map(|column| seed.column_affinity_if_any(column))
            .collect();
        let mut target = RecursiveTarget {
            folded: cte.folded.clone(),
            name: cte.name.clone(),
            id,
            table: table.clone(),
            referenced: false,
            affinities: provisional.clone(),
        };
        let (mut extra_seeds, mut steps) =
            self.bind_recursive_arms(target.clone(), &arms, &direct, (&alias, span))?;
        let settled = settled_affinities(&seed, &extra_seeds, &steps);
        if settled != provisional && !steps.is_empty() {
            target.affinities = settled;
            (extra_seeds, steps) =
                self.bind_recursive_arms(target, &arms, &direct, (&alias, span))?;
        }
        let mut seeds = vec![(CompoundOp::UnionAll, seed)];
        seeds.extend(extra_seeds);
        // The ORDER BY, LIMIT and OFFSET belong to the whole recursive query:
        // SQLite orders its queue by them and stops the recursion at the limit,
        // so they are kept on the body rather than on the seed arm.
        let seed_columns = seeds
            .first()
            .map_or_else(Vec::new, |(_, seed)| seed.columns.clone());
        // An ORDER BY name may come from any arm, as in every other compound.
        let other_arms: Vec<(ast::CompoundOp, crate::bind::BoundSelect)> =
            seeds.iter().skip(1).chain(steps.iter()).cloned().collect();
        let order_by = self.bind_compound_order_by(&order_by, &seed_columns, &other_arms)?;
        let limit = limit.map(|expr| self.bind_expr(expr)).transpose()?;
        let offset = offset.map(|expr| self.bind_expr(expr)).transpose()?;
        let mut source = BoundSource {
            index_hint: crate::bind::IndexChoice::Any,
            id,
            rows: SourceRows::Recursive(Box::new(RecursiveBody {
                seeds,
                steps,
                order_by,
                limit,
                offset,
            })),
            table: std::rc::Rc::new(table),
            alias,
            join,
            constraint: None,
            suppressed: Vec::new(),
            index_exprs: Vec::new(),
            written_schema: None,
            derived: Default::default(),
        };
        if let SourceRows::Recursive(body) = &mut source.rows {
            if body.steps.is_empty() {
                // Declared recursive, never refers to itself: an ordinary
                // compound wearing the keyword.
                let mut arms = core::mem::take(&mut body.seeds);
                if arms.is_empty() {
                    return Err(unsupported("missing select core", span));
                }
                let mut head = arms.remove(0).1;
                head.compounds = arms;
                head.order_by = core::mem::take(&mut body.order_by);
                head.limit = body.limit.take();
                head.offset = body.offset.take();
                source.rows = SourceRows::Subquery(Box::new(head));
            }
        }
        if let Some(slot) = self.sources.get_mut(id) {
            *slot = source;
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(id);
        }
        Ok(())
    }

    /// Binds the arms of a recursive CTE after the first, sorting them into seeds and steps.
    ///
    /// An arm that reads the CTE is a step and one that does not is another seed. The CTE is
    /// registered as the recursion being bound for as long as the arms are, and removed again
    /// whether they bind or not.
    ///
    /// @param target - the CTE, as a reference to it inside its own arms sees it
    /// @param arms - the compound arms after the first, with their operators
    /// @param direct - for each arm, how many FROM terms name the CTE directly
    /// @param alias_and_span - the CTE's name at this use, and where the use is, for an error
    fn bind_recursive_arms(
        &mut self,
        target: RecursiveTarget,
        arms: &[(CompoundOp, ast::SelectCoreId)],
        direct: &[usize],
        alias_and_span: (&[u8], Span),
    ) -> Result<(BoundArms, BoundArms), ParseError> {
        let (alias, span) = alias_and_span;
        let (width, name) = (target.table.columns.len(), target.name.clone());
        self.recursing.push(target);
        let mut seeds = Vec::new();
        let mut steps = Vec::new();
        let mut outcome = Ok(());
        for (position, (op, arm)) in arms.iter().enumerate() {
            if !matches!(op, CompoundOp::Union | CompoundOp::UnionAll) {
                outcome = Err(ParseError::new(
                    ParseErrorKind::Unsupported("recursive query does not use UNION or UNION ALL"),
                    span,
                ));
                break;
            }
            if let Some(target) = self.recursing.last_mut() {
                target.referenced = false;
            }
            let bound = match self.bind_isolated_arm(*arm) {
                Ok(bound) => bound,
                Err(reason) => {
                    outcome = Err(reason);
                    break;
                }
            };
            let referenced = self
                .recursing
                .last()
                .is_some_and(|target| target.referenced);
            // An arm that reads the table only from inside a subquery is not a
            // recursive step in SQLite, so the name is a circular reference.
            if referenced && direct.get(position).copied().unwrap_or(0) == 0 {
                outcome = Err(circular_reference(&name));
                break;
            }
            if let Err(refusal) = arm_refusal(&bound, width, referenced, *op, alias, span) {
                outcome = Err(refusal);
                break;
            }
            if referenced {
                steps.push((*op, bound));
            } else {
                seeds.push((*op, bound));
            }
        }
        self.recursing.pop();
        outcome?;
        Ok((seeds, steps))
    }
}

impl<'a> Binder<'a> {
    /// Binds a FROM term that names a common table expression.
    ///
    /// @param cte - the expression the name stands for
    /// @param folded - the folded name, to tell a recursive reference from a plain one
    /// @param alias - the alias written on the term, if any
    /// @param join - how the term joins the ones before it
    /// @param span - where the term is, for an error
    pub(super) fn bind_cte_term(
        &mut self,
        cte: CteBinding,
        folded: &[u8],
        alias: Option<ast::NameId>,
        join: JoinKind,
        span: Span,
    ) -> Result<(), ParseError> {
        let alias = match alias {
            Some(alias) => self.ast.text(alias).to_vec(),
            None => cte.name.clone(),
        };
        // A definition already being bound cannot be bound again: that
        // is a cycle, and following it does not end.
        if self.binding_ctes.contains(&cte.select) {
            // SQLite names the expression and points at nothing.
            let _ = span;
            return Err(ParseError::new(
                ParseErrorKind::Refused(format!(
                    "circular reference: {}",
                    String::from_utf8_lossy(&cte.name)
                )),
                Span::default(),
            ));
        }
        self.binding_ctes.push(cte.select);
        // **A definition sees the names of its own `WITH` and the ones
        // outside it, not the ones of a `WITH` nested in the query that
        // happens to use it.** The levels above the definition's own are set
        // aside while its body is bound and put back afterwards.
        let hidden = self
            .ctes
            .split_off(cte.level.saturating_add(1).min(self.ctes.len()));
        // **`RECURSIVE` is a keyword SQLite does not require.** A CTE
        // whose FROM names itself *is* the recursion, written or not,
        // and reading the keyword as the only evidence sent this
        // binder round the same definition until the stack ran out.
        let outcome = if cte.recursive || self.select_names_itself(cte.select, folded) {
            self.bind_recursive_cte(&cte, alias, join, span)
        } else {
            let bound =
                self.bind_subquery_term(cte.select, Some(alias), cte.columns.clone(), join, span);
            if bound.is_ok() {
                self.share_last_source(&cte);
            }
            bound
        };
        self.ctes.extend(hidden);
        self.binding_ctes.pop();
        // A recursive CTE's references to itself, inside its own arms, are the
        // recursion and not uses of it.
        let own = match self.ast.select(cte.select) {
            Some(query) => core::iter::once(query.first)
                .chain(query.compounds.iter().map(|(_, arm)| *arm))
                .filter(|arm| self.core_names_cte(*arm, &cte.folded))
                .count() as u32,
            None => 0,
        };
        let uses = self.name_uses(&cte.folded).saturating_sub(own);
        // The reference is the term this binding just added to the block's
        // scope; the sources the body bound come before it.
        let id = self.scope().last().copied();
        if let Some(source) = id.and_then(|id| self.sources.get_mut(id)) {
            source.derived = super::DerivedNote {
                cte: true,
                materialized: cte.materialized,
                uses,
                name: cte.name.clone(),
                ..super::DerivedNote::default()
            };
        }
        outcome
    }

    /// Finds the table a FROM term names, falling back to a table valued
    /// function when the view's own database does not hold the name.
    ///
    /// A name that is not a table of the view's database may still be a table
    /// valued function such as `json_each`, which belongs to no schema.
    ///
    /// @param database - the schema written on the term, if any
    /// @param database_name - the schema to look in, folded
    /// @param folded - the table name, folded
    pub(super) fn find_term_table(
        &self,
        database: Option<ast::NameId>,
        database_name: Option<Vec<u8>>,
        folded: &[u8],
    ) -> (Option<&'a TableInfo>, Option<Vec<u8>>) {
        let found = self.catalog.find_table(database_name.as_deref(), folded);
        // **A schema written on the term does not stop the fallback.** SQLite
        // looks a name up in the schema first and tries the modules when that
        // finds nothing, so `main.generate_series(1, 3)` works as
        // `generate_series(1, 3)` does.
        let _ = database;
        if found.is_none() && database_name.is_some() {
            let eponymous = self
                .catalog
                .find_table(None, folded)
                .filter(|table| table.kind == crate::catalog_view::TableKind::Virtual);
            if eponymous.is_some() {
                return (eponymous, None);
            }
        }
        (found, database_name)
    }
}

/// Returns SQLite's refusal of a common table expression that reads itself where it may not.
///
/// @param name - the expression's name as the `WITH` wrote it
fn circular_reference(name: &[u8]) -> ParseError {
    ParseError::new(
        ParseErrorKind::Refused(format!(
            "circular reference: {}",
            String::from_utf8_lossy(name)
        )),
        Span::default(),
    )
}

/// Refuses a recursive common table expression that names a different number
/// of columns than its first arm makes.
///
/// @param alias - the expression's name, for the message
/// @param named - the column names written after the name, if any
/// @param width - the number of columns of the first arm
/// @param span - where the statement is, for the error
fn named_columns_fit<T>(
    alias: &[u8],
    named: &[T],
    width: usize,
    span: Span,
) -> Result<(), ParseError> {
    if named.is_empty() || named.len() == width {
        return Ok(());
    }
    Err(super::refusal::named_column_count(
        alias,
        width,
        named.len(),
        span,
    ))
}

/// Refuses an arm of a recursive common table expression that SQLite refuses.
///
/// An arm of another width than the first is refused, as in any compound. A
/// recursive step of the wrong width was run, and `WITH i(x) AS (SELECT 1
/// UNION ALL SELECT x+1, x*2 FROM i)` never finished. A recursive step is also
/// held to [`recursive_step_refusal`].
///
/// @param arm - the bound arm
/// @param width - the number of columns of the first arm
/// @param recursive - whether the arm reads the recursive table
/// @param op - the compound operator in front of the arm
/// @param alias - the recursive table's name, for the message
/// @param span - where the statement is, for the error
fn arm_refusal(
    arm: &crate::bind::BoundSelect,
    width: usize,
    recursive: bool,
    op: CompoundOp,
    alias: &[u8],
    span: Span,
) -> Result<(), ParseError> {
    if arm.columns.len() != width {
        return Err(super::refusal::compound_width_mismatch(op, span));
    }
    match recursive
        .then(|| recursive_step_refusal(arm, alias))
        .flatten()
    {
        Some(reason) => Err(super::refused(reason, span)),
        None => Ok(()),
    }
}

/// Returns why SQLite refuses a recursive step, or `None` when it does not.
///
/// **An aggregate in the step never finishes.** Each pass of the recursion
/// feeds the rows the last pass produced back in, and `SELECT count(*) FROM r`
/// produces a row from no rows, so the queue never empties: `WITH RECURSIVE
/// r(n) AS (SELECT 1 UNION ALL SELECT count(*) FROM r) SELECT * FROM r` ran
/// until it was stopped. SQLite refuses an aggregate, a `GROUP BY` and a
/// window function in a recursive step, and a step that names the recursive
/// table twice, before it runs anything.
///
/// @param step - the bound recursive arm
/// @param alias - the recursive table's name, for the message
fn recursive_step_refusal(step: &crate::bind::BoundSelect, alias: &[u8]) -> Option<String> {
    if !step.aggregates.is_empty() || !step.group_by.is_empty() {
        return Some("recursive aggregate queries not supported".to_string());
    }
    if !step.windows.is_empty() {
        return Some("cannot use window functions in recursive queries".to_string());
    }
    let references = step
        .sources
        .iter()
        .filter(|source| matches!(source.rows, SourceRows::RecursiveSelf { .. }))
        .count();
    (references > 1).then(|| {
        format!(
            "multiple references to recursive table: {}",
            String::from_utf8_lossy(alias)
        )
    })
}

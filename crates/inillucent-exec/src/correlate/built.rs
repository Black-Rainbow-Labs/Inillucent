//! The built table of a correlated block.
//!
//! Invariant: **a built table changes how fast a correlated block is answered
//! and never what it answers.** The whole residual of the block stays in the
//! block's chain, so the table only chooses which rows the chain is run over:
//! the rows whose key equals the outer row's value.
//!
//! When a block's only way to reach its rows is a scan of one table, with an
//! equality between a column of that table and a value the outer row supplies,
//! the first outer row reads the table once into a hash table keyed on those
//! columns. Every outer row after it looks its value up in the table instead of
//! scanning. SQLite builds a transient index for the same reason, and only for
//! an `EXISTS` it can turn into a loop of the enclosing query; this table
//! serves every block the engine answers per row, so `NOT EXISTS` and a scalar
//! `count(*)` in a result column get it too.
//!
//! The outer value can be an expression of the outer row, such as `o.a + 1` or
//! `lower(o.name)`. It is computed once per outer row and looked up the same way,
//! where the block used to scan the whole table for every outer row because only a
//! bare outer column was a key. An index on the column was already sought with the
//! expression's value, so this is the case with no index.
//!
//! The table belongs to one execution of the statement. A block over the table
//! that a statement changes is not given one, because the statements that
//! change data prepare their blocks with `Levers::BLOCK_TABLE` off. A request
//! budget that cannot hold the table makes the block scan as it did before.

use inillucent_base::DbResult;
use inillucent_sql::bind::{BoundExpr, SubqueryKind};
use inillucent_sql::catalog_view::TableKind;
use inillucent_sql::plan::{conjunction, AccessPath, Levers, PhysicalPlan};
use inillucent_tree::datum::{Datum, OwnedDatum};

use super::Correlation;
use crate::autoindex::{converted, owned_bytes, HashTable};
use crate::batch::{Batch, Vector};
use crate::expr::{compile, Eval, Expr, StaticType};
use crate::physical::{
    materialise_stage, space_of, translate_scan, AccessKind, Params, Prepared, TreeCatalog,
};

/// How a block reads its table through a built table.
pub(super) struct AutomaticKey {
    /// The key expressions over a row of the table.
    build_keys: Vec<Box<dyn Eval>>,
    /// The key expressions over the outer row's values, one per key.
    probe_keys: Vec<Box<dyn Eval>>,
    /// For each key, where the outer value comes from.
    probes: Vec<Probe>,
    /// The conjuncts that read only the table, applied once as it is read.
    filter: Option<BoundExpr>,
    /// Whether the answer is only whether the key's bucket has a row.
    existence_only: bool,
}

/// Where the outer value of one key comes from.
#[derive(Clone)]
enum Probe {
    /// A column of the joined outer row, read as it is.
    Feed(usize),
    /// An expression of the outer row's values, which the block holds as parameters.
    Expression(BoundExpr),
}

/// One key equality: the column's place in the table's record, where the outer value comes
/// from, and the affinity the comparison applies.
type Key = (usize, Probe, Option<inillucent_value::Affinity>);

/// The parts of a block's residual, sorted by what they do.
struct Sorted {
    /// Each key equality.
    keys: Vec<Key>,
    /// The conjuncts the table can be filtered by while it is read.
    filter: Vec<BoundExpr>,
    /// How many conjuncts were neither a key nor part of the filter.
    left_over: usize,
}

/// Decides whether a block reads its table through a built table, and how.
///
/// Every condition has to hold, and a block that fails one is prepared as it
/// always was: both levers on, one stage that scans a table in the forward
/// direction, and a residual with an equality between a column of that table
/// and a value the outer row feeds, compared as bytes.
///
/// @param plan - the block's plan
/// @param prepared - the block's structural choices
/// @param catalog - where the layouts come from
/// @param feeds - for each outer reference, the joined row column and the parameter it became
/// @param kind - how the block's rows are used
pub(super) fn decide(
    plan: &PhysicalPlan,
    prepared: &Prepared,
    catalog: &dyn TreeCatalog,
    feeds: &[(usize, u32)],
    kind: SubqueryKind,
) -> DbResult<Option<AutomaticKey>> {
    if !plan.levers.has(Levers::AUTOMATIC_INDEX) || !plan.levers.has(Levers::BLOCK_TABLE) {
        return Ok(None);
    }
    let [stage] = prepared.stages.as_slice() else {
        return Ok(None);
    };
    let Some(term) = plan.sources.get(stage.term) else {
        return Ok(None);
    };
    let scans_a_table = stage.kind == AccessKind::Full
        && stage.functions.is_empty()
        && !stage.is_lookup
        && reads_one_set_of_rows(term)
        && plan.sources.len() == 1
        && plan.compounds.is_empty();
    let Some(residual) = plan.residuals.first().and_then(Option::as_ref) else {
        return Ok(None);
    };
    if !scans_a_table {
        return Ok(None);
    }
    let held = space_of(catalog, prepared)?;
    let space = held.view(&prepared.stages);
    let sorted = sort_residual(residual, term.id, feeds, &space);
    if sorted.keys.is_empty() {
        return Ok(None);
    }
    let existence_only =
        kind == SubqueryKind::Exists && sorted.left_over == 0 && answers_existence_alone(plan);
    let types: Vec<StaticType> = space.types.to_vec();
    let (build_keys, probe_keys, probes) = compile_keys(&sorted.keys, &types)?;
    let filter = sorted
        .filter
        .into_iter()
        .reduce(|left, right| BoundExpr::And(Box::new(left), Box::new(right)));
    Ok(Some(AutomaticKey {
        build_keys,
        probe_keys,
        probes,
        filter,
        existence_only,
    }))
}

/// Reports whether a block's one FROM term gives the same rows for every outer row, so
/// they can be read once and keyed.
///
/// **A table scan, or a derived table or view that reads nothing from the outer row.**
/// A grouped view looked up once per outer row on a column it does not group by was
/// rescanned for every outer row, because only a table scan was keyed. `EXPLAIN` already
/// named an automatic index for it. A derived table whose body names an outer value as a
/// parameter changes with the outer row, so it is not keyed.
///
/// @param term - the block's one FROM term
fn reads_one_set_of_rows(term: &inillucent_sql::plan::PlannedSource) -> bool {
    match &term.path {
        AccessPath::TableScan { .. } => term.table.kind == TableKind::Table,
        AccessPath::Subquery {
            plan, correlated, ..
        } => !*correlated && !holds_outer_parameter(&plan.select),
        _ => false,
    }
}

/// Reports whether a query reads a parameter the engine made from an outer reference.
///
/// @param select - the query, searched through its derived tables, arms and subqueries
fn holds_outer_parameter(select: &inillucent_sql::bind::BoundSelect) -> bool {
    let mut found = false;
    let mut copy = select.clone();
    inillucent_sql::rewrite::rewrite_select(&mut copy, &mut |expr: &mut BoundExpr| {
        if let BoundExpr::Parameter(number) = expr {
            found |= *number >= crate::physical::ENGINE_PARAMETER_BASE;
        }
        if let Some(inner) = expr.block_mut() {
            found |= holds_outer_parameter(inner);
        }
    });
    found
}

/// Sorts the conjuncts of a residual into keys, filter and the rest.
///
/// @param residual - the block's residual
/// @param id - the statement wide number of the block's table
/// @param feeds - for each outer reference, the joined row column and the parameter it became
/// @param space - the column space of the block's one stage
fn sort_residual(
    residual: &BoundExpr,
    id: usize,
    feeds: &[(usize, u32)],
    space: &crate::physical::Space<'_>,
) -> Sorted {
    let mut sorted = Sorted {
        keys: Vec::new(),
        filter: Vec::new(),
        left_over: 0,
    };
    for term in conjunction(residual) {
        if let Some(key) = key_equality(&term, id, feeds, space) {
            sorted.keys.push(key);
        } else if buildable(&term, id, feeds) {
            sorted.filter.push(term);
        } else {
            sorted.left_over += 1;
        }
    }
    sorted
}

/// Returns the key an equality is, when it is a byte comparison of a column of
/// the table with a value the outer row feeds, or with an expression of such values.
///
/// The key is the column's place in the table's record, where the outer value
/// comes from, and the affinity the comparison applies.
///
/// @param term - one conjunct of the residual
/// @param id - the statement wide number of the block's table
/// @param feeds - for each outer reference, the joined row column and the parameter it became
/// @param space - the column space of the block's one stage
fn key_equality(
    term: &BoundExpr,
    id: usize,
    feeds: &[(usize, u32)],
    space: &crate::physical::Space<'_>,
) -> Option<Key> {
    let BoundExpr::Compare {
        op: inillucent_sql::ast::BinaryOp::Equal,
        left,
        right,
        affinity,
        collation: inillucent_value::Collation::Binary,
    } = term
    else {
        return None;
    };
    let (column, outer) = match (&**left, &**right) {
        (BoundExpr::Column { source, column, .. }, outer)
        | (outer, BoundExpr::Column { source, column, .. })
            if *source == id =>
        {
            (*column, outer)
        }
        _ => return None,
    };
    let probe = match outer {
        BoundExpr::Parameter(number) => {
            let (feed_column, _) = feeds.iter().find(|(_, fed)| fed == number)?;
            Probe::Feed(*feed_column)
        }
        expression if probe_expression(expression) && reads_a_feed(expression, feeds) => {
            Probe::Expression(expression.clone())
        }
        _ => return None,
    };
    let slot = space.column(id, usize::from(column))?;
    Some((slot, probe, *affinity))
}

/// Reports whether an expression gives one value for one outer row however often it is computed.
///
/// **Literals, parameters, operators and the scalar functions that read nothing but their
/// arguments.** The scan the built table replaces computes the expression once for every row of
/// the table, and the table computes it once for the outer row, so a function whose answer changes
/// between calls, such as `random()` or `changes()`, would answer differently. Anything not named
/// here keeps the block scanning.
///
/// @param expr - the outer side of an equality
fn probe_expression(expr: &BoundExpr) -> bool {
    use inillucent_sql::function::ScalarFunc as F;
    let own = match expr {
        BoundExpr::Null
        | BoundExpr::Integer(_)
        | BoundExpr::Real(_)
        | BoundExpr::Text(_)
        | BoundExpr::Blob(_)
        | BoundExpr::Parameter(_)
        | BoundExpr::Unary { .. }
        | BoundExpr::Arithmetic { .. }
        | BoundExpr::Cast { .. }
        | BoundExpr::Case { .. }
        | BoundExpr::Math { .. } => true,
        BoundExpr::Function { func, .. } => matches!(
            func,
            F::Abs
                | F::Char
                | F::Coalesce
                | F::Concat
                | F::ConcatWs
                | F::Hex
                | F::IfNull
                | F::Iif
                | F::Instr
                | F::Length
                | F::Lower
                | F::LTrim
                | F::Max
                | F::Min
                | F::NullIf
                | F::Quote
                | F::Replace
                | F::Round
                | F::RTrim
                | F::Sign
                | F::Substr
                | F::Trim
                | F::TypeOf
                | F::Unhex
                | F::Unicode
                | F::Upper
                | F::Printf
                | F::OctetLength
        ),
        _ => false,
    };
    own && expr.children().into_iter().all(probe_expression)
}

/// Reports whether an expression reads a value this block's outer row feeds.
///
/// One that reads none is the same for every outer row, and the planner already treats it as a
/// constant.
///
/// @param expr - the outer side of an equality
/// @param feeds - for each outer reference, the joined row column and the parameter it became
fn reads_a_feed(expr: &BoundExpr, feeds: &[(usize, u32)]) -> bool {
    match expr {
        BoundExpr::Parameter(number) => feeds.iter().any(|(_, fed)| fed == number),
        _ => expr
            .children()
            .into_iter()
            .any(|child| reads_a_feed(child, feeds)),
    }
}

/// Reports whether a conjunct can be applied once, while the table is read.
///
/// It must read only the table and the statement's own parameters, and be
/// made of nodes that give the same answer whenever they are evaluated.
///
/// @param expr - the conjunct
/// @param id - the statement wide number of the block's table
/// @param feeds - for each outer reference, the joined row column and the parameter it became
fn buildable(expr: &BoundExpr, id: usize, feeds: &[(usize, u32)]) -> bool {
    match expr {
        BoundExpr::Null
        | BoundExpr::Integer(_)
        | BoundExpr::Real(_)
        | BoundExpr::Text(_)
        | BoundExpr::Blob(_) => true,
        // A parameter the statement wrote is the same for every outer row. The
        // numbers from `ENGINE_PARAMETER_BASE` up are outer references, of this
        // block or of a block around it, and change with the outer row.
        BoundExpr::Parameter(number) => {
            *number < crate::physical::ENGINE_PARAMETER_BASE
                && !feeds.iter().any(|(_, fed)| fed == number)
        }
        BoundExpr::Column { source, .. } | BoundExpr::Rowid { source } => *source == id,
        BoundExpr::Compare { .. }
        | BoundExpr::And(..)
        | BoundExpr::Or(..)
        | BoundExpr::Not(..)
        | BoundExpr::IsNull { .. }
        | BoundExpr::Is { .. }
        | BoundExpr::Between { .. }
        | BoundExpr::Arithmetic { .. }
        | BoundExpr::Unary { .. }
        | BoundExpr::InList { .. } => expr
            .children()
            .into_iter()
            .all(|child| buildable(child, id, feeds)),
        _ => false,
    }
}

/// Reports whether the block's answer depends on nothing but whether a row
/// passes its residual: no grouping, aggregate, window, limit or compound.
///
/// @param plan - the block's plan
fn answers_existence_alone(plan: &PhysicalPlan) -> bool {
    let select = &plan.select;
    plan.constant_filter.is_none()
        && select.group_by.is_empty()
        && select.aggregates.is_empty()
        && select.having.is_none()
        && select.windows.is_empty()
        && select.limit.is_none()
        && select.offset.is_none()
}

/// Compiles the key expressions of both sides, and lists the feeding columns.
///
/// @param keys - each key: the record column, where the outer value comes from, the affinity
/// @param types - the static type of every column of the block's table
#[allow(clippy::type_complexity)]
fn compile_keys(
    keys: &[Key],
    types: &[StaticType],
) -> DbResult<(Vec<Box<dyn Eval>>, Vec<Box<dyn Eval>>, Vec<Probe>)> {
    let unknown = vec![StaticType::Unknown; keys.len()];
    let mut build = Vec::with_capacity(keys.len());
    let mut probe = Vec::with_capacity(keys.len());
    let mut feeds = Vec::with_capacity(keys.len());
    for (position, (slot, feed, affinity)) in keys.iter().enumerate() {
        build.push(compile(&converted(Expr::Column(*slot), *affinity), types)?);
        probe.push(compile(
            &converted(Expr::Column(position), *affinity),
            &unknown,
        )?);
        feeds.push(feed.clone());
    }
    Ok((build, probe, feeds))
}

/// Computes an outer expression key for the outer row whose values `bound` holds.
///
/// The expression reads no column, so it is computed over a space with none.
///
/// @param expression - the outer side of the equality
/// @param bound - the parameters, holding this outer row's values
fn outer_value(expression: &BoundExpr, bound: &Params) -> DbResult<OwnedDatum> {
    let held = crate::physical::HeldSpace {
        layouts: Vec::new(),
        types: Vec::new(),
        order: Vec::new(),
    };
    let space = held.view(&[]);
    crate::constant::constant_value(expression, &space, bound, None)
}

/// Reports whether a row makes a `WHERE` conjunct true.
///
/// The test a filter operator applies: a number other than zero is true, as is
/// a text value that starts with one, and NULL is not true.
///
/// @param test - the compiled conjunct, over the table's record
/// @param row - one row of the table
fn row_passes(test: &dyn Eval, row: &[OwnedDatum]) -> DbResult<bool> {
    let borrowed: Vec<Datum<'_>> = row.iter().map(OwnedDatum::borrow).collect();
    let columns: Vec<Vector<'_>> = borrowed.iter().map(|value| Vector::Const(*value)).collect();
    let batch = Batch::new(1, columns);
    let verdict = test.value(&batch, 0)?;
    Ok(crate::expr::truth(&verdict.get()) == Some(true))
}

impl Correlation {
    /// Answers the block for one outer row from the built table, when it has one.
    ///
    /// `None` when the block has no built table, or has given it up for this
    /// execution; the caller then answers it by scanning.
    ///
    /// @param catalog - where the trees and layouts come from
    /// @param bound - the statement's parameters, to write this row's feeds into
    /// @param feed - returns the joined row's value in one column
    pub(super) fn answer_by_table(
        &self,
        catalog: &dyn TreeCatalog,
        bound: &mut Params,
        feed: &dyn Fn(usize) -> DbResult<OwnedDatum>,
    ) -> DbResult<Option<OwnedDatum>> {
        let Some(key) = self.automatic.as_ref() else {
            return Ok(None);
        };
        if self.declined.get() {
            return Ok(None);
        }
        if self.built.borrow().is_none() && !self.build_table(key, catalog, bound)? {
            return Ok(None);
        }
        // **The outer values are bound only when something reads them**: an
        // expression key computes from them, and the block's chain reads them.
        // An `EXISTS` answered from the table alone reads neither.
        let expression_key = key
            .probes
            .iter()
            .any(|probe| matches!(probe, Probe::Expression(_)));
        if expression_key {
            self.bind_feeds(bound, feed)?;
        }
        // An outer expression that fails is answered by the scan, which raises
        // the error only if a row of the table makes it compute the expression.
        let Some(bucket) = self.bucket_for(key, feed, bound)? else {
            return Ok(None);
        };
        if key.existence_only {
            let found = !bucket.is_empty();
            return Ok(Some(OwnedDatum::Int(i64::from(found != self.negated))));
        }
        if !expression_key {
            self.bind_feeds(bound, feed)?;
        }
        let mut pending = Some(bucket);
        let rows = self.with_chain(catalog, bound, &mut |compiled| {
            let rows = pending.take().unwrap_or_default();
            compiled.run_over_rows(&self.plan, catalog, bound, rows)?;
            Ok(compiled.take_rows())
        })?;
        match rows {
            Some(rows) => Ok(Some(self.answer_from_rows(rows))),
            None => {
                self.give_up_table();
                Ok(None)
            }
        }
    }

    /// Writes the outer row's values into the parameters the block reads them through.
    ///
    /// @param bound - the statement's parameters
    /// @param feed - returns the joined row's value in one column
    fn bind_feeds(
        &self,
        bound: &mut Params,
        feed: &dyn Fn(usize) -> DbResult<OwnedDatum>,
    ) -> DbResult<()> {
        for (column, number) in &self.feeds {
            bound.set(*number, feed(*column)?);
        }
        Ok(())
    }

    /// Reads the block's table into the hash table, once for the execution.
    ///
    /// Returns `false` when the request budget cannot hold the table, in which
    /// case the block scans for the rest of the execution.
    ///
    /// @param key - how the block is keyed
    /// @param catalog - where the trees and layouts come from
    /// @param bound - the statement's parameters
    fn build_table(
        &self,
        key: &AutomaticKey,
        catalog: &dyn TreeCatalog,
        bound: &Params,
    ) -> DbResult<bool> {
        let Some(stage) = self.prepared.stages.first() else {
            return Ok(false);
        };
        let mut rows = materialise_stage(&self.plan, catalog, bound, stage, None)?;
        if let Some(filter) = &key.filter {
            rows = self.rows_passing(rows, filter, catalog, bound)?;
        }
        if !inillucent_base::budget::fits(owned_bytes(&rows)) {
            self.declined.set(true);
            return Ok(false);
        }
        let mut table = HashTable::new();
        table.absorb_owned(&key.build_keys, rows)?;
        *self.built.borrow_mut() = Some(table);
        Ok(true)
    }

    /// Keeps the rows of the table that pass the build filter.
    ///
    /// @param rows - the rows of the table
    /// @param filter - the conjuncts that read only the table
    /// @param catalog - where the layouts come from
    /// @param bound - the statement's parameters
    fn rows_passing(
        &self,
        rows: Vec<Vec<OwnedDatum>>,
        filter: &BoundExpr,
        catalog: &dyn TreeCatalog,
        bound: &Params,
    ) -> DbResult<Vec<Vec<OwnedDatum>>> {
        let held = space_of(catalog, &self.prepared)?;
        let mut space = held.view(&self.prepared.stages);
        space.catalog = Some(catalog);
        let expr = translate_scan(filter, &space, bound)?;
        let test = compile(&expr, space.types)?;
        let mut kept = Vec::with_capacity(rows.len());
        for row in rows {
            if row_passes(test.as_ref(), &row)? {
                kept.push(row);
            }
        }
        Ok(kept)
    }

    /// Returns the table's rows whose key equals the outer row's values.
    ///
    /// A NULL in any key value gives no rows, because the equality is never
    /// true for it. The block is still run over those no rows when it is not
    /// only an existence test: `count(*)` of nothing is 0 and `max(x)` of
    /// nothing is NULL, and only running the chain answers both.
    ///
    /// `None` when a key that is an expression fails to compute, so the caller
    /// scans for this outer row.
    ///
    /// @param key - how the block is keyed
    /// @param feed - returns the joined row's value in one column
    /// @param bound - the parameters, holding this outer row's values
    fn bucket_for(
        &self,
        key: &AutomaticKey,
        feed: &dyn Fn(usize) -> DbResult<OwnedDatum>,
        bound: &Params,
    ) -> DbResult<Option<Vec<Vec<OwnedDatum>>>> {
        let mut values: Vec<OwnedDatum> = Vec::with_capacity(key.probes.len());
        for probe in &key.probes {
            values.push(match probe {
                Probe::Feed(column) => feed(*column)?,
                Probe::Expression(expression) => match outer_value(expression, bound) {
                    Ok(value) => value,
                    Err(_) => return Ok(None),
                },
            });
        }
        let borrowed: Vec<Datum<'_>> = values.iter().map(OwnedDatum::borrow).collect();
        let columns: Vec<Vector<'_>> = borrowed.iter().map(|value| Vector::Const(*value)).collect();
        let batch = Batch::new(1, columns);
        let (encoded, has_null) = crate::join::encode_row_key(&key.probe_keys, &batch, 0)?;
        if has_null {
            return Ok(Some(Vec::new()));
        }
        let held = self.built.borrow();
        let Some(table) = held.as_ref() else {
            return Ok(Some(Vec::new()));
        };
        Ok(Some(
            table
                .probe(&encoded)
                .iter()
                .filter_map(|position| table.rows.get(*position as usize).cloned())
                .collect(),
        ))
    }

    /// Stops using the built table for the rest of the execution.
    fn give_up_table(&self) {
        self.declined.set(true);
        self.built.borrow_mut().take();
    }

    /// Drops the built table, and those of the blocks nested in this one.
    ///
    /// The table belongs to one execution of the statement, and the next
    /// execution tries to build it again even if this one gave it up.
    pub fn drop_table(&self) {
        self.built.borrow_mut().take();
        self.declined.set(false);
        if let Ok(chain) = self.chain.try_borrow() {
            if let Some(compiled) = chain.as_ref() {
                compiled.drop_block_tables();
            }
        }
    }
}

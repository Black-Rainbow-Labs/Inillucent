//! The wording of an index seek in `EXPLAIN QUERY PLAN`.
//!
//! Invariant: **the text is SQLite's, so a tool that reads a plan line reads
//! this engine's the same way.** A line ends with the columns the seek compares,
//! written `a=?`, `a>?`, `a<?`, with `<expr>` for a key the index computes.

use super::{AccessPath, AggregationMode, Levers, PhysicalPlan};
use crate::ast::{BinaryOp, JoinKind};
use crate::bind::BoundExpr;
use crate::catalog_view::{TableInfo, TableKind};

/// Returns the `(col=? AND col>? AND col<?)` detail an index seek's
/// description ends with, given how many leading columns of its equality prefix
/// it pins and which ends of a range on the column after it the seek carries.
///
/// Shared between [`super::AccessPath::IndexSeek`] and each branch of an
/// [`super::AccessPath::IndexSeekUnion`], which differ only in how many branches
/// there are - the naming of one branch's columns is exactly what a plain
/// seek already does.
///
/// **A key the index computes is `<expr>`**, which is what SQLite prints for an
/// index on `lower(email)`. A name that cannot be found, because the caller had
/// no declaration to look it up in, stays `?`.
///
/// @param index_name - the index's name
/// @param info - the table's declaration, when the caller has it
/// @param equalities - how many leading index columns are pinned
/// @param ends - whether the range has a lower end and an upper end
pub(super) fn index_seek_detail(
    index_name: &[u8],
    info: Option<&TableInfo>,
    equalities: usize,
    ends: (bool, bool),
) -> String {
    let keyed = info.and_then(|held| {
        held.indexes
            .iter()
            .find(|candidate| candidate.name == index_name)
    });
    let named = |position: usize| -> String {
        let Some(key) = keyed.and_then(|index| index.columns.get(position)) else {
            return "?".to_string();
        };
        match key.column {
            None => "<expr>".to_string(),
            Some(at) => info
                .and_then(|held| held.column(at))
                .map(|column| String::from_utf8_lossy(&column.name).into_owned())
                .unwrap_or_else(|| "?".to_string()),
        }
    };
    let mut detail = String::new();
    for index in 0..equalities {
        if index > 0 {
            detail.push_str(" AND ");
        }
        detail.push_str(&format!("{}=?", named(index)));
    }
    if ends.0 || ends.1 {
        if !detail.is_empty() {
            detail.push_str(" AND ");
        }
        detail.push_str(&range_detail(&named(equalities), ends.0, ends.1));
    }
    detail
}

/// Returns `col>?`, `col<?` or `col>? AND col<?` for the ends a range has.
///
/// SQLite writes `>` for both `>` and `>=` and `<` for both `<` and `<=`.
///
/// @param column - the column's name
/// @param low - whether the range has a lower end
/// @param high - whether the range has an upper end
pub(super) fn range_detail(column: &str, low: bool, high: bool) -> String {
    match (low, high) {
        (true, true) => format!("{column}>? AND {column}<?"),
        (false, true) => format!("{column}<?"),
        _ => format!("{column}>?"),
    }
}

/// Returns the lines SQLite prints for a joined term that it reads through an automatic index.
///
/// A table scanned in the inner loop of a join, with an equality between one of its columns and
/// the terms before it, is read through an index SQLite builds for the statement. The plan shows
/// a bloom filter over the key and then a search of that index, and a `LEFT JOIN` marks the
/// search. The key lists the columns in the order SQLite finds the equalities: the ones written
/// with this table's column on the left come in written order, and the ones written the other
/// way round come after them in reverse, because SQLite analyses the written terms from the
/// last to the first and adds the flipped copy of each.
///
/// An equality between a column of this term and a constant adds that column to the key, and any
/// other condition that reads only this term makes the index `PARTIAL`, as SQLite does. A key
/// side that is an expression over this term's columns prints as `<expr>`, which is what this
/// engine's hash join is keyed on.
///
/// A correlated subquery's only term is read through the same kind of index when its condition
/// has an equality against a column of an enclosing query. The pinned SQLite prints these two
/// lines under the subquery's node once `ANALYZE` has given it row counts, and scans before
/// that. This engine reads the table once into a hash table whether or not it has statistics.
///
/// `None` when the term is read some other way, which leaves the ordinary line.
///
/// @param plan - the plan being described
/// @param position - the term's place in the join order
/// @param shown - the name the line uses for the term
pub(super) fn automatic_index_lines(
    plan: &PhysicalPlan,
    position: usize,
    shown: &str,
) -> Option<Vec<String>> {
    if !plan.levers.has(Levers::AUTOMATIC_INDEX) {
        return None;
    }
    let used = index_use(plan, position)?;
    let partial = if used.partial { "PARTIAL " } else { "" };
    let source = plan.sources.get(position)?;
    let suffix = if source.join == JoinKind::Left {
        " LEFT-JOIN"
    } else {
        ""
    };
    let detail = used.detail;
    let search =
        format!("SEARCH {shown} USING AUTOMATIC {partial}COVERING INDEX ({detail}){suffix}");
    Some(vec![format!("BLOOM FILTER ON {shown} ({detail})"), search])
}

/// Returns the lines SQLite prints for an `EXISTS` it turns into a loop of the enclosing query.
///
/// SQLite rewrites a positive `EXISTS` over one table into a loop of the outer query, and writes
/// the word `EXISTS` after the table's name in that loop's line. With the automatic index on, the
/// loop is a search of the index built for it, and it is always `PARTIAL`. This engine does not
/// rewrite anything: the lines describe the hash table it reads the block's table into. `None`
/// when the block's table is not read that way.
///
/// @param block - the plan of the `EXISTS` block
/// @param shown - the name the line uses for the block's table
pub(super) fn exists_loop_lines(block: &PhysicalPlan, shown: &str) -> Option<Vec<String>> {
    let select = &block.select;
    if select.sources.len() != 1
        || !select.aggregates.is_empty()
        || select.limit.is_some()
        || !select.group_by.is_empty()
        || select.distinct
        || !block.compounds.is_empty()
        || !select.windows.is_empty()
    {
        return None;
    }
    let used = index_use(block, 0)?;
    if !used.block {
        return None;
    }
    if !block.levers.has(Levers::AUTOMATIC_INDEX) {
        return Some(vec![format!("SCAN {shown} EXISTS")]);
    }
    Some(vec![
        format!("BLOOM FILTER ON {shown} ({})", used.detail),
        format!(
            "SEARCH {shown} EXISTS USING AUTOMATIC PARTIAL COVERING INDEX ({})",
            used.detail
        ),
    ])
}

/// The key list of an automatic index on a term, and what kind of read it is.
struct IndexUse {
    /// The key columns written as `a=? AND b=?`.
    detail: String,
    /// Whether the index is `PARTIAL`.
    partial: bool,
    /// Whether the term is the only term of a correlated subquery.
    block: bool,
}

/// Returns the automatic index a term would be read through, ignoring whether the lever is on.
///
/// @param plan - the plan being described
/// @param position - the term's place in the join order
fn index_use(plan: &PhysicalPlan, position: usize) -> Option<IndexUse> {
    let source = plan.sources.get(position)?;
    let block = index_is_possible(plan, position)?;
    let condition = source
        .on
        .as_ref()
        .or_else(|| plan.residuals.get(position).and_then(Option::as_ref))?;
    // The values an index key may come from: the loops before this one, and
    // anything that is no loop of this plan at all, which is an enclosing query.
    let local: Vec<usize> = plan.sources.iter().map(|held| held.id).collect();
    let earlier: Vec<usize> = plan
        .sources
        .iter()
        .take(position)
        .map(|held| held.id)
        .collect();
    let found = index_keys(condition, source, &earlier, &local, block);
    if !found.joined {
        return None;
    }
    let detail = found
        .written
        .into_iter()
        .chain(found.flipped.into_iter().rev())
        .map(|name| format!("{name}=?"))
        .collect::<Vec<_>>()
        .join(" AND ");
    Some(IndexUse {
        detail,
        partial: found.partial,
        block,
    })
}

/// Reports whether a term could be read through an automatic index, and whether it is the only
/// term of a correlated subquery.
///
/// A derived table is read through one too, and SQLite favours one there: the index costs less
/// to build over rows it has just produced than over a table. It is used even as the first
/// loop, when the value it is sought by comes from a query enclosing this one. A table is
/// eligible after the first loop, and as the first and only loop of a plan: the plan of a
/// correlated subquery, which the key search then checks reads an enclosing query.
///
/// @param plan - the plan being described
/// @param position - the term's place in the join order
fn index_is_possible(plan: &PhysicalPlan, position: usize) -> Option<bool> {
    let source = plan.sources.get(position)?;
    let derived = matches!(
        source.path,
        AccessPath::Subquery {
            correlated: false,
            ..
        }
    );
    let table = matches!(source.path, AccessPath::TableScan { .. })
        && source.table.kind == TableKind::Table;
    let only_loop =
        position == 0 && plan.sources.len() == 1 && table && plan.levers.has(Levers::BLOCK_TABLE);
    let possible = (position > 0 || derived || only_loop)
        && (table || derived)
        && matches!(
            source.join,
            JoinKind::Inner | JoinKind::Cross | JoinKind::Comma | JoinKind::Left
        );
    possible.then_some(only_loop)
}

/// The key columns an automatic index would have, in the two orders SQLite lists them.
#[derive(Default)]
struct IndexKeys {
    /// Keys written with this term's column on the left, in written order.
    written: Vec<String>,
    /// Keys written with this term's column on the right, in written order.
    flipped: Vec<String>,
    /// Whether at least one key is an equality against an earlier loop or an enclosing query.
    joined: bool,
    /// Whether the term has a condition of its own, which SQLite shows as `PARTIAL`.
    partial: bool,
}

/// Sorts the conjuncts of a term's condition into the keys of its automatic index.
///
/// @param condition - the term's `ON`, or the residual that applies at its place
/// @param source - the term
/// @param earlier - the statement-wide numbers of the terms before this one
/// @param local - the statement-wide numbers of every term of this plan
/// @param block - whether the term is the only term of a correlated subquery
fn index_keys(
    condition: &BoundExpr,
    source: &super::PlannedSource,
    earlier: &[usize],
    local: &[usize],
    block: bool,
) -> IndexKeys {
    let mut found = IndexKeys::default();
    for term in super::conjunction(condition) {
        let BoundExpr::Compare {
            op: BinaryOp::Equal,
            left,
            right,
            ..
        } = &term
        else {
            found.partial |= reads_only_term(&term, source.id);
            continue;
        };
        // A correlated subquery's hash table is keyed on plain columns, compared
        // as bytes, against a plain column of the enclosing query.
        if block && !block_key_is_plain(&term) {
            found.partial |= reads_only_term(&term, source.id);
            continue;
        }
        match (
            key_name(left, source, !block),
            key_name(right, source, !block),
        ) {
            (Some(name), None) if reads_only(right, earlier, local) => {
                found.written.push(name);
                found.joined = true;
            }
            (None, Some(name)) if reads_only(left, earlier, local) => {
                found.flipped.push(name);
                found.joined = true;
            }
            _ => constant_key(&mut found, left, right, source),
        }
    }
    found
}

/// Reports whether an equality can be a key of a correlated subquery's hash table: bytes compare
/// equal, and neither side is more than a column.
///
/// @param term - the equality
fn block_key_is_plain(term: &BoundExpr) -> bool {
    let BoundExpr::Compare {
        left,
        right,
        collation,
        ..
    } = term
    else {
        return false;
    };
    let column = |side: &BoundExpr| matches!(side, BoundExpr::Column { .. });
    let constant = |side: &BoundExpr| reads_no_source(side);
    *collation == inillucent_value::Collation::Binary
        && ((column(left) && (column(right) || constant(right)))
            || (constant(left) && column(right)))
}

/// Adds an equality between a plain column of the term and a constant to the keys.
///
/// SQLite folds such an equality into the key list and marks the index `PARTIAL`. Any other
/// equality that reads only this term makes the index `PARTIAL` as well.
///
/// @param found - the keys so far
/// @param left - the left side of the equality
/// @param right - the right side of the equality
/// @param source - the term
fn constant_key(
    found: &mut IndexKeys,
    left: &BoundExpr,
    right: &BoundExpr,
    source: &super::PlannedSource,
) {
    match (
        key_name(left, source, false),
        key_name(right, source, false),
    ) {
        (Some(name), None) if reads_no_source(right) => {
            found.written.push(name);
            found.partial = true;
        }
        (None, Some(name)) if reads_no_source(left) => {
            found.flipped.push(name);
            found.partial = true;
        }
        _ => {
            let own = |side: &BoundExpr| reads_only_term(side, source.id);
            let constant = |side: &BoundExpr| reads_no_source(side);
            found.partial |=
                (own(left) && (own(right) || constant(right))) || (own(right) && constant(left));
        }
    }
}

/// Returns the name an expression has in an index key on a term, when it can be one.
///
/// A plain column of the term is its declared name. With `allow_expression`, an expression that
/// reads only this term's columns is `<expr>`, the way SQLite writes the key of an index on an
/// expression.
///
/// @param expr - one side of an equality
/// @param source - the term
/// @param allow_expression - whether a computed key side is accepted
fn key_name(
    expr: &BoundExpr,
    source: &super::PlannedSource,
    allow_expression: bool,
) -> Option<String> {
    if let Some(column) = key_column(expr, source.id) {
        return Some(
            source
                .table
                .column(column)
                .map(|held| String::from_utf8_lossy(&held.name).into_owned())
                .unwrap_or_else(|| "?".to_string()),
        );
    }
    let computed = !matches!(expr, BoundExpr::Rowid { .. });
    (allow_expression && computed && reads_only_term(expr, source.id)).then(|| "<expr>".to_string())
}

/// Reports whether an expression reads at least one column and only columns of one term.
///
/// @param expr - the expression
/// @param id - the term's statement-wide number
fn reads_only_term(expr: &BoundExpr, id: usize) -> bool {
    let mut used = Vec::new();
    expr.sources_used(&mut used);
    !used.is_empty() && used.iter().all(|source| *source == id)
}

/// Reports whether an expression reads no column of any term, so it is a constant.
///
/// @param expr - the expression
fn reads_no_source(expr: &BoundExpr) -> bool {
    let mut used = Vec::new();
    expr.sources_used(&mut used);
    used.is_empty()
}

/// Returns the declared column an expression is, when it is a column of the given term.
///
/// @param expr - one side of an equality
/// @param id - the term's statement-wide number
fn key_column(expr: &BoundExpr, id: usize) -> Option<u16> {
    match expr {
        BoundExpr::Column { source, column, .. } if *source == id => Some(*column),
        _ => None,
    }
}

/// Reports whether an expression reads columns of the earlier terms or of an
/// enclosing query alone, and at least one.
///
/// @param expr - the other side of an equality
/// @param earlier - the statement-wide numbers of the terms before this one
/// @param local - the statement-wide numbers of every term of this plan
fn reads_only(expr: &BoundExpr, earlier: &[usize], local: &[usize]) -> bool {
    let mut used = Vec::new();
    expr.sources_used(&mut used);
    !used.is_empty()
        && used
            .iter()
            .all(|source| earlier.contains(source) || !local.contains(source))
}

// Returns one line per loop of a plan, in loop order: `SCAN t`, `SEARCH t
/// USING INDEX ...`, and the two lines of an automatic index.
///
/// A derived table is named the way SQLite names it in a loop: its alias, or
/// `(subquery-N)` when it was written with none.
///
/// @param plan - the plan to describe
pub(super) fn loop_lines(plan: &PhysicalPlan) -> Vec<String> {
    let mut lines = Vec::new();
    for (position, source) in plan.sources.iter().enumerate() {
        let shown = shown_name(plan, position);
        if let Some(automatic) = automatic_index_lines(plan, position, &shown) {
            lines.extend(automatic);
            continue;
        }
        let mut line = source.path.describe_over(&shown, Some(&source.table));
        if source.join == JoinKind::Left && position > 0 {
            line.push_str(" LEFT-JOIN");
        }
        lines.push(line);
    }
    lines
}

/// Returns the name a loop line uses for the term at one position of a plan.
///
/// @param plan - the plan
/// @param position - the term's place in the join order
pub(super) fn shown_name(plan: &PhysicalPlan, position: usize) -> String {
    let Some(source) = plan.sources.get(position) else {
        return String::new();
    };
    let bound = plan.select.sources.iter().find(|held| held.id == source.id);
    match (&source.written_schema, bound) {
        (Some(schema), _) => format!(
            "{}.{}",
            String::from_utf8_lossy(schema),
            String::from_utf8_lossy(&source.alias)
        ),
        (None, Some(bound)) => super::tree::loop_name(bound),
        (None, None) => String::from_utf8_lossy(&source.alias).into_owned(),
    }
}

/// Returns the sorter lines of a plan: `USE TEMP B-TREE FOR GROUP BY`, `ORDER
/// BY` and `DISTINCT`, each only when the plan has that sorter.
///
/// @param plan - the plan to describe
pub(super) fn temp_lines(plan: &PhysicalPlan) -> Vec<String> {
    let mut lines = Vec::new();
    // A temp b-tree is only named when there is one. Grouping and
    // de-duplicating that the walk already delivers build nothing, and a
    // plan that said otherwise would be describing a different program.
    if plan.aggregation == AggregationMode::Grouped && !plan.grouped_walk {
        lines.push("USE TEMP B-TREE FOR GROUP BY".to_string());
    }
    if plan.needs_sort {
        lines.push("USE TEMP B-TREE FOR ORDER BY".to_string());
    }
    if plan.select.distinct && !plan.distinct_walk {
        lines.push("USE TEMP B-TREE FOR DISTINCT".to_string());
    }
    lines
}

/// Returns the lines for the search that finds the rows an `UPDATE` or `DELETE` changes.
///
/// SQLite reads the whole row to change it, so it never reports a covering index for a write
/// even when the index holds every column the statement names. The search here may be answered
/// from the index alone, and the line says `INDEX` all the same.
///
/// @param plan - the plan of the query that finds the rows
pub(super) fn write_lines(plan: &PhysicalPlan) -> Vec<String> {
    loop_lines(plan)
        .into_iter()
        .chain(temp_lines(plan))
        .map(|line| line.replace(" USING COVERING INDEX ", " USING INDEX "))
        .collect()
}

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
/// `None` when the term is read some other way, which leaves the ordinary line. A term that also
/// has an equality against a constant is not handled: SQLite folds the constant into the key and
/// may reorder the join, and this plan would not match it.
///
/// @param plan - the plan being described
/// @param position - the term's place in the join order
/// @param shown - the name the line uses for the term
pub(super) fn automatic_index_lines(
    plan: &PhysicalPlan,
    position: usize,
    shown: &str,
) -> Option<Vec<String>> {
    let source = plan.sources.get(position)?;
    // A derived table is read through an automatic index too, and SQLite
    // favours one there: the index costs less to build over rows it has just
    // produced than over a table. It is used even as the first loop, when the
    // value it is sought by comes from a query enclosing this one.
    let derived = matches!(
        source.path,
        AccessPath::Subquery {
            correlated: false,
            ..
        }
    );
    let table = matches!(source.path, AccessPath::TableScan { .. })
        && source.table.kind == TableKind::Table;
    let eligible = (position > 0 || derived)
        && plan.levers.has(Levers::AUTOMATIC_INDEX)
        && (table || derived)
        && matches!(
            source.join,
            JoinKind::Inner | JoinKind::Cross | JoinKind::Comma | JoinKind::Left
        );
    if !eligible {
        return None;
    }
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
    let mut written = Vec::new();
    let mut flipped = Vec::new();
    for term in super::conjunction(condition) {
        let BoundExpr::Compare {
            op: BinaryOp::Equal,
            left,
            right,
            ..
        } = &term
        else {
            continue;
        };
        match (key_column(left, source.id), key_column(right, source.id)) {
            (Some(column), None) if reads_only(right, &earlier, &local) => written.push(column),
            (None, Some(column)) if reads_only(left, &earlier, &local) => flipped.push(column),
            _ => {}
        }
    }
    if written.is_empty() && flipped.is_empty() {
        return None;
    }
    let keys: Vec<String> = written
        .into_iter()
        .chain(flipped.into_iter().rev())
        .map(|column| {
            let name = source
                .table
                .column(column)
                .map(|held| String::from_utf8_lossy(&held.name).into_owned())
                .unwrap_or_else(|| "?".to_string());
            format!("{name}=?")
        })
        .collect();
    let detail = keys.join(" AND ");
    let suffix = if source.join == JoinKind::Left {
        " LEFT-JOIN"
    } else {
        ""
    };
    Some(vec![
        format!("BLOOM FILTER ON {shown} ({detail})"),
        format!("SEARCH {shown} USING AUTOMATIC COVERING INDEX ({detail}){suffix}"),
    ])
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

/// Returns one line per loop of a plan, in loop order: `SCAN t`, `SEARCH t
/// USING INDEX ...`, and the two lines of an automatic index.
///
/// A derived table is named the way SQLite names it in a loop: its alias, or
/// `(subquery-N)` when it was written with none.
///
/// @param plan - the plan to describe
pub(super) fn loop_lines(plan: &PhysicalPlan) -> Vec<String> {
    let mut lines = Vec::new();
    for (position, source) in plan.sources.iter().enumerate() {
        let bound = plan.select.sources.iter().find(|held| held.id == source.id);
        let shown = match (&source.written_schema, bound) {
            (Some(schema), _) => format!(
                "{}.{}",
                String::from_utf8_lossy(schema),
                String::from_utf8_lossy(&source.alias)
            ),
            (None, Some(bound)) => super::tree::loop_name(bound),
            (None, None) => String::from_utf8_lossy(&source.alias).into_owned(),
        };
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

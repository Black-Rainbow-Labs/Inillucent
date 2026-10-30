//! Which order an access path's walk produces, and which `ORDER BY` and `GROUP BY` terms it
//! answers.
//!
//! Invariant: **a walk is claimed to answer an ordering only when every term names a key the walk
//! is ordered by, in the direction and collation the structure holds it in.** A wrong claim returns
//! rows in the wrong order and nothing about the result looks wrong. A key is a column of the table,
//! its rowid, or an expression an index computes and the binder bound for the term.
//!
//! Here rather than in [`super`] because `plan.rs` is at its recorded size and this is one idea
//! that `limit_walk` and `adjacent` share with it.

use super::*;

/// Returns which of the walk's keys an expression names, if it names one.
/// @param expr - the expression to resolve
/// @param id - the outer term's source id
/// @param table - the table being walked
pub(super) fn walk_key_of(
    expr: &BoundExpr,
    id: usize,
    table: &TableInfo,
    exprs: &[crate::dml::BoundIndexExprs],
) -> Option<OrderedBy> {
    let mut expr = expr;
    while let BoundExpr::Collate { operand, .. } = expr {
        expr = operand;
    }
    match expr {
        BoundExpr::Column { source, column, .. } if *source == id => {
            Some(named_key(table, OrderedBy::Column(*column)))
        }
        BoundExpr::Rowid { source } if *source == id => Some(OrderedBy::Rowid),
        other => computed_key_named(other, exprs),
    }
}

/// Returns the name an expression goes by when some index of the term computes it as a key.
///
/// An index on `json_extract(p, '$.k')` holds its entries in the order of that expression's value,
/// so an `ORDER BY` or `GROUP BY` term that is the same bound expression is a key the walk is
/// ordered by. An expression no index computes is not a key anything is ordered by, so it is not
/// named and the caller treats the statement as unordered by it.
///
/// @param expr - the `ORDER BY` or `GROUP BY` expression, with any COLLATE removed
/// @param exprs - the bound key expressions of the term's expression indexes
pub(super) fn computed_key_named(
    expr: &BoundExpr,
    exprs: &[crate::dml::BoundIndexExprs],
) -> Option<OrderedBy> {
    let computed = exprs
        .iter()
        .any(|held| held.keys.iter().flatten().any(|key| key == expr));
    computed.then(|| OrderedBy::Computed(expr.clone()))
}

/// Returns the bound index expressions of the FROM term with a statement-wide number.
///
/// @param select - the bound statement
/// @param id - the FROM term's statement-wide number
pub(super) fn exprs_of(select: &BoundSelect, id: usize) -> &[crate::dml::BoundIndexExprs] {
    select
        .sources
        .iter()
        .find(|source| source.id == id)
        .map_or(&[], |source| source.index_exprs.as_slice())
}

/// Returns whether the outer term's walk answers the `ORDER BY`, and whether it does so through a
/// key an index computes.
///
/// @param select - the bound statement
/// @param sources - the planned FROM terms, outer first
/// @param answerable - whether the statement's shape lets a walk answer an `ORDER BY` at all
pub(super) fn provided_by_outer(
    select: &BoundSelect,
    sources: &[PlannedSource],
    answerable: bool,
) -> (Option<bool>, bool) {
    let provided = sources.first().filter(|_| answerable).and_then(|outer| {
        let exprs = exprs_of(select, outer.id);
        ordering_provided(select, outer.id, &outer.table, exprs, &outer.path)
    });
    let computed = provided.is_some()
        && sources
            .first()
            .is_some_and(|outer| orders_by_computed_key(select, outer.id));
    (provided, computed)
}

/// Returns whether any `ORDER BY` term names a key an index of the term computes.
///
/// When the walk answers the order, such a term is never evaluated: the executor leaves it out, so
/// a covering walk of the index does not need the columns the expression reads. A statement that
/// groups, aggregates or removes duplicates evaluates its terms after that, so it is left alone.
///
/// @param select - the bound statement
/// @param id - the outermost FROM term's statement-wide number
pub(super) fn orders_by_computed_key(select: &BoundSelect, id: usize) -> bool {
    let exprs = exprs_of(select, id);
    select.group_by.is_empty()
        && select.aggregates.is_empty()
        && !select.distinct
        && !exprs.is_empty()
        && select.order_by.iter().any(|term| {
            let mut expr = &term.expr;
            while let BoundExpr::Collate { operand, .. } = expr {
                expr = operand;
            }
            matches!(
                computed_key_named(expr, exprs),
                Some(OrderedBy::Computed(_))
            )
        })
}

/// What a term of an `ORDER BY` names.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum OrderedBy {
    /// A column of the table, by its declared position.
    Column(u16),
    /// The row's key.
    Rowid,
    /// An expression an index computes as a key, as the binder bound it.
    Computed(BoundExpr),
}

/// The order one access path's own walk produces.
pub(super) struct PathOrdering {
    /// What the walk is ordered by, in order, with each key's direction and the
    /// collation the structure compares it with.
    pub(super) columns: Vec<(OrderedBy, bool, Collation)>,
    /// What an equality has pinned to a single value, which therefore holds
    /// still across the whole walk and cannot affect its order.
    pub(super) pinned: Vec<OrderedBy>,
}

/// Returns whether the outer term's path already produces the `ORDER BY`, and
/// whether it has to be walked backwards to do it.
///
/// `None` means it does not and the rows have to go through a sorter.
/// `Some(false)` means a forward walk answers it, `Some(true)` a backward one.
///
/// The rules are narrow on purpose, because getting this wrong returns rows in
/// the wrong order and nothing about the result looks wrong:
///
/// - every `ORDER BY` term is a plain column of the outer term, or its rowid;
/// - the path is one whose order is a key's - every rowid path, and an index
///   seek over whatever columns the equalities did not pin;
/// - the directions agree, all with the structure or all against it, because a
///   B-tree can be read either way but not both at once;
/// - the collation is the one the structure holds the column in;
/// - the NULLs land where the structure puts them, which for the SQL defaults
///   they already do: first ascending, last descending, exactly as an index
///   holds them.
///
/// A column an equality pinned is skipped rather than matched: it holds one
/// value for every row the path returns, so ordering by it changes nothing.
/// @param select - the bound statement, for its ORDER BY
/// @param outer - the planned outer term
pub(super) fn ordering_provided(
    select: &BoundSelect,
    id: usize,
    table: &TableInfo,
    exprs: &[crate::dml::BoundIndexExprs],
    path: &AccessPath,
) -> Option<bool> {
    if select.order_by.is_empty() {
        return Some(false);
    }
    let key = path_ordering(table, exprs, path)?;
    let mut reverse: Option<bool> = None;
    let mut at = 0usize;
    for term in &select.order_by {
        // `ORDER BY name COLLATE NOCASE` binds to a `Collate` around the
        // column, and the collation it names is already on the term - so the
        // wrapper is unwrapped rather than refused, or the one case an index
        // exists precisely to answer would be the one case that sorted.
        let mut expr = &term.expr;
        while let BoundExpr::Collate { operand, .. } = expr {
            expr = operand;
        }
        if order_constant::equated_to_outside_value(select, id, expr, term.collation) {
            continue;
        }
        let named = walk_key_of(expr, id, table, exprs)?;
        let descending = matches!(term.order, SortOrder::Descending);
        // The binder has already defaulted this, so what is left is a written
        // placement - and only the one the structure already produces can be
        // answered by a walk: an index holds NULLs first, so a forward walk is
        // NULLS FIRST and a backward one is NULLS LAST.
        let natural = match term.nulls {
            NullOrder::First => !descending,
            NullOrder::Last => descending,
        };
        if !natural {
            return None;
        }
        if key.pinned.contains(&named) {
            continue;
        }
        let (held, held_descending, held_collation) = key.columns.get(at).cloned()?;
        if held != named || held_collation != term.collation {
            return None;
        }
        let walk = descending != held_descending;
        match reverse {
            None => reverse = Some(walk),
            Some(existing) if existing == walk => {}
            Some(_) => return None,
        }
        at = at.saturating_add(1);
    }
    Some(reverse.unwrap_or(false))
}

/// Returns the order one access path's walk produces, if it produces one.
/// @param table - the table being read
/// @param path - the chosen path
pub(super) fn path_ordering(
    table: &TableInfo,
    exprs: &[crate::dml::BoundIndexExprs],
    path: &AccessPath,
) -> Option<PathOrdering> {
    match path {
        // A table B-tree is keyed by rowid, and a range over it is a slice of
        // that same walk.
        AccessPath::TableScan { .. } | AccessPath::RowidRange { .. } => Some(PathOrdering {
            columns: rowid_key(table),
            pinned: Vec::new(),
        }),
        // One row is in every order at once.
        AccessPath::RowidSeek { .. } => Some(PathOrdering {
            columns: Vec::new(),
            pinned: Vec::new(),
        }),
        AccessPath::IndexSeek {
            index_name,
            equalities,
            ..
        } => {
            let index = table
                .indexes
                .iter()
                .find(|candidate| candidate.name == *index_name)?;
            let mut columns: Vec<(OrderedBy, bool, Collation)> = Vec::new();
            let mut pinned: Vec<OrderedBy> = Vec::new();
            for (at, key_column) in index.columns.iter().enumerate() {
                let Some(named) = key_named(table, index, exprs, at) else {
                    break;
                };
                let collation = collation_of(&key_column.collation);
                if at < equalities.len() {
                    pinned.push(named);
                    continue;
                }
                columns.push((named, key_column.descending, collation));
            }
            // Every index entry ends with the row's key, so the walk is a total
            // order even where the indexed columns tie.
            columns.push((OrderedBy::Rowid, false, Collation::Binary));
            Some(PathOrdering { columns, pinned })
        }
        // A branch that needs de-duplicating (an `IN` list) has no order:
        // list values are probed in whatever order they were written, not
        // index order. A branch that does not - the keyset-range shape,
        // proven disjoint at plan time - runs each branch in the index's own
        // order and the branches themselves in that same order, so the whole
        // union reads exactly as an unconstrained walk of the index would: no
        // column is pinned, because no column has one value across every
        // branch.
        AccessPath::IndexSeekUnion {
            index_name,
            dedup: false,
            ..
        } => {
            let index = table
                .indexes
                .iter()
                .find(|candidate| candidate.name == *index_name)?;
            let mut columns: Vec<(OrderedBy, bool, Collation)> = Vec::new();
            for (at, key_column) in index.columns.iter().enumerate() {
                let Some(named) = key_named(table, index, exprs, at) else {
                    break;
                };
                let collation = collation_of(&key_column.collation);
                columns.push((named, key_column.descending, collation));
            }
            columns.push((OrderedBy::Rowid, false, Collation::Binary));
            Some(PathOrdering {
                columns,
                pinned: Vec::new(),
            })
        }
        _ => None,
    }
}

/// Returns what the index's key at a position is ordered by, when a statement could name it.
///
/// A bare column is named by its position and a key the index computes by the expression the
/// binder bound for it. A computed key whose schema text did not bind has no name, so the walk
/// stops describing itself there and an `ORDER BY` past it is left to the sorter.
///
/// @param table - the table being read
/// @param index - the index the path walks
/// @param exprs - the bound key expressions of the term's expression indexes
/// @param at - the position of the key within the index
pub(super) fn key_named(
    table: &TableInfo,
    index: &IndexInfo,
    exprs: &[crate::dml::BoundIndexExprs],
    at: usize,
) -> Option<OrderedBy> {
    let key_column = index.columns.get(at)?;
    if let Some(column) = key_column.plain_column() {
        return Some(named_key(table, OrderedBy::Column(column)));
    }
    let position = table
        .indexes
        .iter()
        .position(|candidate| candidate.name == index.name)?;
    let held = exprs.iter().find(|held| held.position == position)?;
    let bound = held.keys.get(at)?.as_ref()?;
    Some(OrderedBy::Computed(bound.clone()))
}

/// Returns the ordering a rowid walk produces.
/// @param table - the table being walked
pub(super) fn rowid_key(table: &TableInfo) -> Vec<(OrderedBy, bool, Collation)> {
    let _ = table;
    vec![(OrderedBy::Rowid, false, Collation::Binary)]
}

/// Returns the one name a key goes by.
///
/// `INTEGER PRIMARY KEY` is the rowid under another name, so a statement that
/// ordered by the declared column and one that ordered by `rowid` asked for the
/// same walk. Folding the two spellings into one here is what lets the rest of
/// the comparison be an equality.
/// @param table - the table the column belongs to
/// @param named - the key as the statement or the index spelled it
pub(super) fn named_key(table: &TableInfo, named: OrderedBy) -> OrderedBy {
    match named {
        OrderedBy::Column(column) if table.rowid_alias == Some(column) => OrderedBy::Rowid,
        other => other,
    }
}

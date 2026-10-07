//! The inner columns of a parenthesised join, and looking a name up among them.
//!
//! Invariant: **a name written against a parenthesised join resolves to the
//! derived column that carries the inner table's column of that name**, as it
//! does in SQLite, rather than failing because the derived table renamed it.

use super::*;

/// What a name matched among the inner columns of a parenthesised join.
pub(super) enum NestedHits {
    /// The derived columns, by position, that carry the name.
    Some(Vec<u16>),
    /// Nothing carries the name, and the lookup is settled for this term.
    NoneButNamed,
    /// The reference names something else, such as the derived table's alias.
    Unrelated,
}

/// Looks a column name up among the inner columns of a parenthesised join.
///
/// A bare name settles the lookup for the term whether or not it matched. A
/// qualified name does only when the qualifier is the name of an inner table.
///
/// @param names - where each derived column came from
/// @param column - the folded column name
/// @param qualifier - the folded qualifier, when one was written
pub(super) fn nested_hits(
    names: &[NestedName],
    column: &[u8],
    qualifier: Option<&[u8]>,
) -> NestedHits {
    let named_table = qualifier.is_none_or(|wanted| names.iter().any(|held| held.table == wanted));
    if !named_table {
        return NestedHits::Unrelated;
    }
    let hits: Vec<u16> = names
        .iter()
        .filter(|held| held.column == column && qualifier.is_none_or(|wanted| held.table == wanted))
        .map(|held| held.index)
        .collect();
    if hits.is_empty() {
        NestedHits::NoneButNamed
    } else {
        NestedHits::Some(hits)
    }
}

/// Where one column of a parenthesised join came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NestedName {
    /// The folded name of the inner table.
    pub table: Vec<u8>,
    /// The folded name of the inner column.
    pub column: Vec<u8>,
    /// The position of the derived table's column that carries it.
    pub index: u16,
}

/// Where one column of a parenthesised join's `SELECT *` came from.
#[derive(Clone, Copy)]
pub(super) struct Origin {
    /// The source that holds the column.
    pub source: usize,
    /// The column's position in that source.
    pub column: u16,
    /// Whether a `USING` hides the column from `*`.
    pub hidden: bool,
    /// Whether the result column holds the merged value of a `USING` column
    /// and not this source's own copy.
    pub merged: bool,
}

/// Returns the hits for a name a parenthesised join's visible columns carry.
///
/// A column that an inner `USING` hides still has the inner table's name, so
/// `t3.a` reaches it. A bare `a` means the merged column the join shows, so a
/// hidden copy is left out whenever a visible one carries the name.
///
/// @param source - the derived table the name was matched in
/// @param hits - the derived table's columns that carry the name
/// @param unqualified - whether the reference had no table qualifier
pub(super) fn prefer_shown(source: &BoundSource, hits: Vec<u16>, unqualified: bool) -> Vec<u16> {
    if !unqualified {
        return hits;
    }
    let shown: Vec<u16> = hits
        .iter()
        .copied()
        .filter(|index| source.table.column(*index).is_some_and(|held| !held.hidden))
        .collect();
    if shown.is_empty() {
        hits
    } else {
        shown
    }
}

impl Binder<'_> {
    /// Puts the columns a parenthesised join's `USING` terms hide back into its
    /// result, after the column each one follows in the join.
    ///
    /// SQLite keeps those columns in the derived table, marked as hidden from
    /// `*`, so that `t4.a` written outside the parentheses still reaches the
    /// inner `t4`. Under a `LEFT`, `RIGHT` or `FULL` join the hidden copy is
    /// NULL on a row the other side did not match, so it cannot be answered by
    /// the merged column. A derived table inside the join hands its own hidden
    /// columns up the same way.
    ///
    /// Returns where each result column came from, or nothing when the result
    /// is not the plain `SELECT *` this expects.
    ///
    /// @param bound - the block built for the parenthesised join
    pub(super) fn expose_using_columns(
        &mut self,
        bound: &mut BoundSelect,
    ) -> Result<Vec<Origin>, ParseError> {
        let mut plan: Vec<Origin> = Vec::new();
        for source in &bound.sources {
            let derived = source.table.kind == TableKind::Subquery;
            for (index, column) in source.table.columns.iter().enumerate() {
                let position = index as u16;
                let hides = source.suppressed.contains(&position) || (derived && column.hidden);
                if hides || !column.hidden {
                    plan.push(Origin {
                        source: source.id,
                        column: position,
                        hidden: hides,
                        merged: false,
                    });
                }
            }
        }
        let shown = plan.iter().filter(|origin| !origin.hidden).count();
        if shown != bound.columns.len() {
            return Ok(Vec::new());
        }
        let mut old = std::mem::take(&mut bound.columns).into_iter();
        let mut origins = Vec::with_capacity(plan.len());
        for origin in plan {
            if origin.hidden {
                let result = self.hidden_result_column(origin.source, origin.column)?;
                bound.columns.push(result);
                origins.push(origin);
                continue;
            }
            let Some(result) = old.next() else {
                continue;
            };
            // A `RIGHT` or `FULL` join makes the shown column the merged value,
            // and `t2.a` still has to read `t2`'s own copy, so that copy is
            // kept beside it as a hidden column.
            let own = self.hidden_result_column(origin.source, origin.column)?;
            let merged = result.expr != own.expr;
            bound.columns.push(result);
            origins.push(Origin { merged, ..origin });
            if merged {
                bound.columns.push(own);
                origins.push(Origin {
                    hidden: true,
                    ..origin
                });
            }
        }
        Ok(origins)
    }

    /// Builds the result column that reads one hidden `USING` column.
    ///
    /// @param source - the source that holds the column
    /// @param column - the column's position in that source
    fn hidden_result_column(
        &mut self,
        source: usize,
        column: u16,
    ) -> Result<BoundResultColumn, ParseError> {
        let expr = self.column_expr(source, column)?;
        let info = self
            .sources
            .get(source)
            .and_then(|held| held.table.column(column));
        Ok(BoundResultColumn {
            expr,
            name: info.map(|held| held.name.clone()).unwrap_or_default(),
            origin: None,
            declared_type: info
                .map(|held| held.declared_type.clone())
                .unwrap_or_default(),
            written: None,
        })
    }

    /// Marks columns of a derived table as hidden from `*`.
    ///
    /// @param id - the derived table's source id
    /// @param hidden - for each of its columns, whether to hide it
    pub(super) fn hide_derived_columns(&mut self, id: usize, hidden: &[bool]) {
        if !hidden.iter().any(|flag| *flag) {
            return;
        }
        let Some(source) = self.sources.get_mut(id) else {
            return;
        };
        let table = std::rc::Rc::make_mut(&mut source.table);
        for (column, flag) in table.columns.iter_mut().zip(hidden) {
            column.hidden = *flag;
        }
    }

    /// Returns which inner table and column each column of a parenthesised join
    /// came from.
    ///
    /// A column that comes through a parenthesised join nested inside this one
    /// keeps the inner table's name, so `t3.a` written outside several levels
    /// of parentheses still finds `t3`. The name follows the column `*` took it
    /// from, and under a `RIGHT` join that is not the column the result reads.
    ///
    /// @param bound - the block built for the parenthesised join
    /// @param origins - where each result column came from, when known
    pub(super) fn nested_names_of(
        &self,
        bound: &BoundSelect,
        origins: &[Origin],
    ) -> Vec<NestedName> {
        let mut names = Vec::new();
        for (index, column) in bound.columns.iter().enumerate() {
            let origin = match origins.get(index) {
                Some(known) if origins.len() == bound.columns.len() => {
                    Some((known.source, known.column))
                }
                _ => match &column.expr {
                    BoundExpr::Column { source, column, .. } => Some((*source, *column)),
                    BoundExpr::Function { arguments, .. } => match arguments.first() {
                        Some(BoundExpr::Column { source, column, .. }) => Some((*source, *column)),
                        _ => None,
                    },
                    _ => None,
                },
            };
            let Some((source, inner)) = origin else {
                continue;
            };
            let merged = origins.get(index).is_some_and(|known| known.merged);
            let through = self
                .nested_names_for(source)
                .and_then(|held| held.iter().find(|name| name.index == inner));
            if let Some(name) = through {
                names.push(NestedName {
                    table: if merged {
                        Vec::new()
                    } else {
                        name.table.clone()
                    },
                    column: name.column.clone(),
                    index: index as u16,
                });
                continue;
            }
            let Some(held) = self.sources.get(source) else {
                continue;
            };
            let Some(info) = held.table.column(inner) else {
                continue;
            };
            names.push(NestedName {
                table: if merged {
                    Vec::new()
                } else {
                    held.alias.to_ascii_lowercase()
                },
                column: info.folded.clone(),
                index: index as u16,
            });
        }
        names
    }
}

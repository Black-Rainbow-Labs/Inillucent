//! The names the columns of a derived table, a view and a `CREATE TABLE ... AS`
//! have.
//!
//! Invariant: **no two columns of one of them have the same name, ignoring
//! ASCII case, and the rule that makes them different is SQLite's.** The second
//! `a` of `SELECT a, a FROM t` becomes `a:1`, the third `a:2`. A name that
//! already ends in `:` and digits has them replaced rather than extended, so a
//! collision with `a:1` gives `a:2` and never `a:1:1`. A column with no name,
//! or whose name is `true` or `false`, is called `column` and its position,
//! counting from one. The top-level result of a `SELECT` is not renamed: its
//! columns keep their duplicates, which is what a caller reading the header of
//! `SELECT a, a FROM t` sees in SQLite too.

use inillucent_value::Collation;

use super::BoundSelect;
use crate::catalog_view::ColumnInfo;

/// Checks a view's declared column list against its body, and names the
/// view's columns by the declarations they resolve to.
///
/// **The width is checked when the view is read, not when it is created.**
/// SQLite does it in `sqlite3ViewGetColumnNames`, at the first use, so a view
/// declared with a column list of the wrong width is created and fails each
/// time it is read. Only a derived table keeps the spelling that was typed for
/// a bare column; a view uses the declaration's.
///
/// @param bound - the view's bound body
/// @param columns - the column list the view was declared with, if any
/// @param view_name - the view's name, for the message
/// @param span - where the view was named
pub(super) fn finish_view_columns(
    bound: &mut BoundSelect,
    columns: &[Vec<u8>],
    view_name: &[u8],
    span: crate::lexer::Span,
) -> Result<(), crate::diagnostic::ParseError> {
    if !columns.is_empty() && columns.len() != bound.columns.len() {
        return Err(super::refused(
            format!(
                "expected {} columns for '{}' but got {}",
                columns.len(),
                String::from_utf8_lossy(view_name),
                bound.columns.len()
            ),
            span,
        ));
    }
    for column in &mut bound.columns {
        column.written = None;
    }
    Ok(())
}

/// Returns the names with duplicates made unique.
///
/// @param names - the names in column order, as written or derived
pub fn unique_column_names(names: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut taken: Vec<Vec<u8>> = Vec::with_capacity(names.len());
    let mut out = Vec::with_capacity(names.len());
    for (position, name) in names.iter().enumerate() {
        let mut chosen = base_name(name, position);
        let mut count = 0u32;
        while taken.contains(&chosen.to_ascii_lowercase()) {
            count = count.saturating_add(1);
            chosen = with_suffix(&chosen, count);
        }
        taken.push(chosen.to_ascii_lowercase());
        out.push(chosen);
    }
    out
}

/// Returns the name a column starts from, before duplicates are looked for.
///
/// @param name - the name as written or derived
/// @param position - the column's position, counting from zero
fn base_name(name: &[u8], position: usize) -> Vec<u8> {
    let unnamed = name.is_empty()
        || name.eq_ignore_ascii_case(b"true")
        || name.eq_ignore_ascii_case(b"false");
    if unnamed {
        return format!("column{}", position.saturating_add(1)).into_bytes();
    }
    name.to_vec()
}

/// Returns a name with `:count` in place of any `:digits` it already ends in.
///
/// @param name - the name that collided
/// @param count - the number to put after the colon
fn with_suffix(name: &[u8], count: u32) -> Vec<u8> {
    let digits = name
        .iter()
        .rev()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let stem_end = name.len().saturating_sub(digits);
    let keeps_colon =
        digits > 0 && stem_end > 0 && name.get(stem_end.saturating_sub(1)) == Some(&b':');
    let stem = match keeps_colon {
        true => name.get(..stem_end.saturating_sub(1)).unwrap_or(name),
        false => name,
    };
    let mut out = stem.to_vec();
    out.extend_from_slice(format!(":{count}").as_bytes());
    out
}

/// Builds the table a nested query's rows are read through.
///
/// The columns are the block's result columns. Their affinity and collation
/// come from the expressions behind them, so a comparison against a subquery
/// column applies the rules it would have applied one level down; a column with
/// no affinity of its own gets none, which is what SQLite does for an
/// expression that is not a bare column or a cast.
/// Returns the columns a nested query's result presents to a reader.
///
/// Public because a write to a view needs them before there is a FROM term to
/// hang them on: the view's catalog entry carries no column list at all.
pub fn subquery_columns(select: &BoundSelect, names: &[Vec<u8>]) -> Vec<ColumnInfo> {
    let written: Vec<Vec<u8>> = select
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            names
                .get(index)
                .cloned()
                .or_else(|| column.written.clone())
                .unwrap_or_else(|| column.name.clone())
        })
        .collect();
    let unique = unique_column_names(&written);
    select
        .columns
        .iter()
        .enumerate()
        .zip(unique)
        .map(|((index, column), name)| {
            let folded = name.to_ascii_lowercase();
            let collation = column.expr.collation().unwrap_or(Collation::Binary);
            ColumnInfo {
                name,
                folded,
                declared_type: column.declared_type.clone(),
                affinity: select.column_affinity(index),
                collation: collation.name().as_bytes().to_ascii_lowercase(),
                not_null: false,
                not_null_conflict: None,
                primary_key_conflict: None,
                default_sql: None,
                primary_key_position: None,
                hidden: false,
                generated: false,
                stored: false,
                generated_sql: None,
            }
        })
        .collect()
}

impl super::Binder<'_> {
    /// Returns the name a bare column reference was written with, looking
    /// through `COLLATE`.
    ///
    /// A derived table names such a column after the identifier as it was
    /// typed, not after the declaration it resolved to: `SELECT * FROM (SELECT
    /// ABC FROM t)` over `t(Abc)` has a column called `ABC`. Parentheses leave
    /// no node in the tree, so `(abc)` is the same as `abc`.
    ///
    /// @param id - the result column's expression
    pub(super) fn written_column_name(&self, id: crate::ast::ExprId) -> Option<&[u8]> {
        let mut current = id;
        loop {
            match self.ast.expr(current)? {
                crate::ast::Expr::Collate { operand, .. } => current = *operand,
                crate::ast::Expr::Column { column, .. } => return Some(self.ast.text(*column)),
                _ => return None,
            }
        }
    }
}

impl super::Binder<'_> {
    /// Returns a column reference as it was written, with its qualifiers.
    ///
    /// SQLite prints the reference the way the statement spelled it, so
    /// `SELECT main.t.zz` fails with `no such column: main.t.zz`.
    ///
    /// @param database - the schema qualifier, when one was written
    /// @param table - the table qualifier, when one was written
    /// @param column - the column name
    pub(super) fn reference_label(
        &self,
        database: Option<crate::ast::NameId>,
        table: Option<crate::ast::NameId>,
        column: crate::ast::NameId,
    ) -> Vec<u8> {
        let mut label = Vec::new();
        for part in [database, table].into_iter().flatten() {
            label.extend_from_slice(self.ast.text(part));
            label.push(b'.');
        }
        label.extend_from_slice(self.ast.text(column));
        label
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> Vec<Vec<u8>> {
        list.iter().map(|name| name.as_bytes().to_vec()).collect()
    }

    /// Duplicates get `:1`, `:2`, and the comparison ignores case.
    #[test]
    fn duplicates_are_numbered_without_regard_to_case() {
        assert_eq!(
            unique_column_names(&names(&["a", "a", "A"])),
            names(&["a", "a:1", "A:2"])
        );
    }

    /// An existing `:digits` suffix is replaced, not extended.
    #[test]
    fn an_existing_suffix_is_replaced() {
        assert_eq!(
            unique_column_names(&names(&["x:1", "x", "x"])),
            names(&["x:1", "x", "x:2"])
        );
    }

    /// An empty name, `true` and `false` become `column` and a position.
    #[test]
    fn unnamed_columns_are_called_column_and_a_position() {
        assert_eq!(
            unique_column_names(&names(&["", "TRUE", "b"])),
            names(&["column1", "column2", "b"])
        );
    }
}

//! Two FROM terms that go by the same name.
//!
//! Invariant: **a name that two terms of one block share cannot be told apart
//! by a reference to it.** SQLite allows the terms to exist (`FROM t, t` is
//! legal when nothing reads a column of them) and refuses the first reference
//! that has to choose between them, including the references a `*` stands for.

use super::Binder;

impl Binder<'_> {
    /// Returns a column reference as it was written, qualifiers included.
    ///
    /// @param database - the schema qualifier, when one was written
    /// @param table - the table qualifier, when one was written
    /// @param column - the column name
    pub(super) fn written_reference(
        &self,
        database: Option<crate::ast::NameId>,
        table: Option<crate::ast::NameId>,
        column: crate::ast::NameId,
    ) -> Vec<u8> {
        [database, table]
            .iter()
            .flatten()
            .flat_map(|id| self.ast.text(*id).iter().copied().chain([b'.']))
            .chain(self.ast.text(column).iter().copied())
            .collect()
    }

    /// Reports whether a `USING` or `NATURAL` join merges a column of one term
    /// with another term's.
    ///
    /// Such a column is shown once, from the left term, so two terms of the
    /// same name do not make it ambiguous.
    ///
    /// @param scope - the ids of the block's FROM terms
    /// @param id - the term the column belongs to
    /// @param column - the column's name
    pub(super) fn is_joined_by_using(&self, scope: &[usize], id: usize, column: &[u8]) -> bool {
        scope.iter().any(|other| {
            *other != id
                && self.sources.get(*other).is_some_and(|held| {
                    held.suppressed.iter().any(|position| {
                        held.table
                            .column(*position)
                            .is_some_and(|merged| merged.name.eq_ignore_ascii_case(column))
                    })
                })
        })
    }

    /// Reports whether another term of the block has the same name and schema
    /// as one term.
    ///
    /// @param scope - the ids of the block's FROM terms
    /// @param id - the term being asked about
    pub(super) fn has_twin_term(&self, scope: &[usize], id: usize) -> bool {
        let Some(term) = self.sources.get(id) else {
            return false;
        };
        // A subquery written with no alias is given a name nothing else can
        // use, so two of them never share one. The binder's stand in name for
        // it is only for messages.
        if term.table.kind == crate::catalog_view::TableKind::Subquery && term.alias == b"subquery"
        {
            return false;
        }
        scope.iter().any(|other| {
            *other != id
                && self.sources.get(*other).is_some_and(|held| {
                    held.alias.eq_ignore_ascii_case(&term.alias)
                        && held.table.database == term.table.database
                })
        })
    }
}

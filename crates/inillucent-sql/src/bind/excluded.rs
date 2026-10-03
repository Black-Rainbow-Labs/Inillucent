//! Telling the upsert's `excluded` row from a table alias of the same name.
//!
//! Invariant: **a table alias in scope is looked up before the upsert's pseudo
//! table.** `INSERT INTO t AS excluded ... DO UPDATE SET n = excluded.n` names
//! the existing row, and `excluded` means the row that was to be inserted only
//! when no source in scope is called that.

use super::{Binder, BoundSource};

impl Binder<'_> {
    /// Reports whether a qualifier names the upsert's `excluded` row.
    ///
    /// @param qualifier - the folded table qualifier of the column, if any
    pub(super) fn names_excluded_row(&self, qualifier: Option<&[u8]>) -> bool {
        let source_of = |id: &usize| self.sources.get(*id);
        let named = |source: &BoundSource| source.alias.eq_ignore_ascii_case(b"excluded");
        let aliased = self
            .scopes
            .iter()
            .flatten()
            .filter_map(source_of)
            .any(named);
        qualifier == Some(b"excluded".as_slice()) && !aliased
    }
}

//! The inner columns of a parenthesised join, and looking a name up among them.
//!
//! Invariant: **a name written against a parenthesised join resolves to the
//! derived column that carries the inner table's column of that name**, as it
//! does in SQLite, rather than failing because the derived table renamed it.

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

//! Where a derived table came from.
//!
//! Invariant: **the note describes the FROM term as it was written, and the
//! binder fills it once, when the term is bound.** The planner reads it to
//! decide what SQLite would decide: whether to flatten the derived table, run
//! it as a co-routine or fill a table with it, and what to call it in
//! `EXPLAIN QUERY PLAN`.

/// Where a derived table came from.
///
/// SQLite plans a derived table by where it came from as much as by what it
/// holds. A CTE written `MATERIALIZED` is never flattened, and a CTE used more
/// than once is materialized once rather than run as a co-routine for each
/// use. A derived table that has no name is shown as `(subquery-N)`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DerivedNote {
    /// Whether the FROM term was a subquery written with no alias.
    pub anonymous: bool,
    /// Whether the FROM term named a view.
    pub view: bool,
    /// Whether the FROM term named a common table expression.
    pub cte: bool,
    /// `Some(true)` for `MATERIALIZED`, `Some(false)` for `NOT MATERIALIZED`.
    pub materialized: Option<bool>,
    /// How many FROM terms of the statement name the CTE.
    pub uses: u32,
    /// The CTE's or view's name as written, which a `CO-ROUTINE` or
    /// `MATERIALIZE` node shows in place of the alias.
    pub name: Vec<u8>,
    /// Whether something outside the query reads this term's columns by its
    /// number, so it has to stay a term rather than be flattened.
    ///
    /// The view an `INSTEAD OF UPDATE` or `DELETE` writes through is one: the
    /// trigger's `OLD` and the new values are read from its columns after the
    /// query has found the rows.
    pub pinned: bool,
}

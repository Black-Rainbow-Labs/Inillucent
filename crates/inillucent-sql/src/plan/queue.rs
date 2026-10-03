//! What a recursive CTE's own `ORDER BY`, `LIMIT` and `OFFSET` mean to the loop
//! that fills it.
//!
//! Invariant: **the `ORDER BY` decides which queued row is taken next and the
//! `LIMIT` stops the recursion.** A CTE with neither is first in, first out and
//! runs until a pass produces nothing. It is here rather than in [`super`]
//! because `plan.rs` is at its recorded size.

use crate::bind::{BoundExpr, BoundOrderTerm};

/// What a recursive CTE's own `ORDER BY`, `LIMIT` and `OFFSET` mean to the loop
/// that fills it.
///
/// The `ORDER BY` decides which queued row is taken next, so `ORDER BY depth
/// DESC` walks depth first, and the `LIMIT` stops the recursion once that many
/// rows have been produced after the `OFFSET`.
#[derive(Clone, Debug, PartialEq)]
pub struct RecursiveQueue {
    /// The terms that order the queue; empty for first in, first out.
    pub order_by: Vec<BoundOrderTerm>,
    /// The `LIMIT`.
    pub limit: Option<BoundExpr>,
    /// The `OFFSET`.
    pub offset: Option<BoundExpr>,
}

//! The JSON subtype: which values carry SQLite's `J` mark, and how the mark
//! reaches the JSON call that reads it.
//!
//! Invariant: **a subtype is a property of the call that produced a value, and
//! the binder decides it from the bound tree.** `Value` has no slot for one, so
//! `subtype()` is answered here from the producing call, and a JSON function's
//! argument that carries the mark in SQLite but is not itself a JSON call is
//! wrapped in one. Both rules were measured against the pinned 3.53.4 shell.

use inillucent_value::Collation;

use super::{Binder, BoundExpr, SubqueryKind};
use crate::ast::ExprId;
use crate::diagnostic::ParseError;
use crate::function;

impl Binder<'_> {
    /// Binds `subtype(X)`.
    ///
    /// **`subtype` is answered where the producing function is known.**
    /// A subtype is not a property of a value here - `Value` has no slot
    /// for one - it is a property of the *call* that made it, which is
    /// exactly what the reference records at run time and what the binder
    /// can see. The one call whose answer depends on the data is
    /// `json_extract`, which carries the JSON subtype only when what it
    /// extracted was itself an array or an object; that one is left to run.
    ///
    /// @param argument - the call's one argument
    pub(super) fn bind_subtype(&mut self, argument: ExprId) -> Result<BoundExpr, ParseError> {
        let bound = self.bind_expr(argument)?;
        let bound = self.walk_value(&bound).unwrap_or(bound);
        // A JSON group aggregate carries the subtype too, and its function
        // is in the binder's list rather than in the expression - so the
        // slot is resolved here, where the list is.
        if let BoundExpr::Aggregate { slot, .. } = &bound {
            let carries = matches!(
                self.aggregates.get(*slot).map(|held| held.func),
                Some(
                    function::AggregateFunc::JsonGroupArray
                        | function::AggregateFunc::JsonGroupObject
                )
            );
            return Ok(BoundExpr::Integer(if carries { 74 } else { 0 }));
        }
        Ok(match json_subtype(&bound) {
            Subtyped::Always => BoundExpr::Integer(74),
            Subtyped::Never => BoundExpr::Integer(0),
            Subtyped::WhenShaped => BoundExpr::Function {
                func: function::ScalarFunc::Subtype,
                arguments: vec![bound],
                collation: Collation::Binary,
            },
        })
    }

    /// Returns a JSON function's argument with its JSON subtype made visible.
    ///
    /// **A subtype reaches the call above only through a nested JSON call.**
    /// The executor carries the mark from one JSON call to the one that reads
    /// it, and an argument of any other shape arrives as a plain value. Two
    /// shapes carry the mark in SQLite and are not JSON calls here:
    ///
    /// - a `json_group_array` or `json_group_object` result. `json_object('items',
    ///   json_group_array(item))` answered `{"items":"[\"Latte\"]"}`, the array
    ///   quoted as a string, where SQLite answers `{"items":["Latte"]}`.
    /// - a scalar subquery whose one column is either of those or a JSON call.
    ///   SQLite keeps the subtype through a scalar subquery, and does not keep
    ///   it through a derived table or a CTE, which this leaves alone.
    ///
    /// Each is wrapped in `json()`, or `jsonb()` for the binary aggregates,
    /// which gives back the same document with the mark on it. Measured
    /// against 3.53.4, including the cases where SQLite quotes the value.
    ///
    /// @param argument - the argument, bound
    pub(super) fn marked_as_json(&self, argument: BoundExpr) -> BoundExpr {
        if let Some(walked) = self.walk_value(&argument) {
            return walked;
        }
        let wrapper = match &argument {
            BoundExpr::Aggregate { slot, .. } => {
                json_aggregate_wrapper(self.aggregates.get(*slot).map(|held| held.func))
            }
            BoundExpr::Subquery {
                kind: SubqueryKind::Scalar,
                block,
                ..
            } => match block.columns.as_slice() {
                [only] => match &only.expr {
                    BoundExpr::Aggregate { slot, .. } => {
                        json_aggregate_wrapper(block.aggregates.get(*slot).map(|held| held.func))
                    }
                    other if json_subtype(other) == Subtyped::Always => {
                        Some(function::JsonFunc::Json)
                    }
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        };
        match wrapper {
            Some(func) => BoundExpr::Json {
                func,
                arguments: vec![argument],
            },
            None => argument,
        }
    }
}

impl Binder<'_> {
    /// Returns a `json_each` or `json_tree` row's `value` wrapped so that it
    /// carries the JSON mark when the row is an array or an object, or `None`
    /// for any other expression.
    ///
    /// See `JsonFunc::WalkValue`. The row's `type` column is the second
    /// argument; it reads the same term, so it costs one more column of a row
    /// the scan already produced.
    ///
    /// @param argument - an argument of a JSON function, bound
    pub(super) fn walk_value(&self, argument: &BoundExpr) -> Option<BoundExpr> {
        const VALUE: u16 = 1;
        const TYPE: u16 = 2;
        let BoundExpr::Column {
            source,
            column: VALUE,
            ..
        } = argument
        else {
            return None;
        };
        let term = self.sources.iter().find(|term| term.id == *source)?;
        let walks = term.table.kind == crate::catalog_view::TableKind::Virtual
            && matches!(term.table.folded.as_slice(), b"json_each" | b"json_tree");
        if !walks {
            return None;
        }
        let kind = BoundExpr::Column {
            source: *source,
            column: TYPE,
            slot: TYPE,
            affinity: inillucent_value::affinity::Affinity::Blob,
            collation: Collation::Binary,
        };
        Some(BoundExpr::Json {
            func: function::JsonFunc::WalkValue,
            arguments: vec![argument.clone(), kind],
        })
    }
}

/// Returns the JSON call that marks a JSON group aggregate's result, if it is one.
///
/// @param func - the aggregate, when the slot named one
fn json_aggregate_wrapper(func: Option<function::AggregateFunc>) -> Option<function::JsonFunc> {
    match func? {
        function::AggregateFunc::JsonGroupArray | function::AggregateFunc::JsonGroupObject => {
            Some(function::JsonFunc::Json)
        }
        function::AggregateFunc::JsonbGroupArray | function::AggregateFunc::JsonbGroupObject => {
            Some(function::JsonFunc::Jsonb)
        }
        _ => None,
    }
}

/// Whether a bound expression carries the JSON subtype.
///
/// SQLite marks a value with the subtype `74` - the letter `J` - when it was
/// produced by a function that returns JSON *text*. The binary spellings do
/// not carry it (a `jsonb_` result is a blob, and a blob read back out of a
/// column has no subtype either), and the functions that answer a number or a
/// type name are not JSON at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Subtyped {
    /// The call always marks its answer.
    Always,
    /// The call never does.
    Never,
    /// It depends on what came out: `json_extract` marks an array or an
    /// object and does not mark the scalar it may equally have found.
    WhenShaped,
}

/// Returns whether an expression's value carries the JSON subtype.
///
/// @param bound - the argument to `subtype`
fn json_subtype(bound: &BoundExpr) -> Subtyped {
    let BoundExpr::Json { func, .. } = bound else {
        return Subtyped::Never;
    };
    use function::JsonFunc;
    match func {
        JsonFunc::Extract | JsonFunc::Arrow | JsonFunc::WalkValue => Subtyped::WhenShaped,
        JsonFunc::Jsonb
        | JsonFunc::ArrayB
        | JsonFunc::ExtractB
        | JsonFunc::InsertB
        | JsonFunc::ObjectB
        | JsonFunc::PatchB
        | JsonFunc::RemoveB
        | JsonFunc::ReplaceB
        | JsonFunc::SetB
        | JsonFunc::ArrayInsertB
        | JsonFunc::ArrowShift
        | JsonFunc::ArrayLength
        | JsonFunc::ErrorPosition
        | JsonFunc::Type
        | JsonFunc::Valid
        | JsonFunc::Pretty => Subtyped::Never,
        _ => Subtyped::Always,
    }
}

/// Reports whether an expression's value always carries the JSON subtype.
///
/// For a caller that reads the value after it has left the expression, such
/// as a window pass that computes each argument into a column first and so
/// cannot ask the producing call at run time.
///
/// @param bound - the expression
pub fn always_json(bound: &BoundExpr) -> bool {
    json_subtype(bound) == Subtyped::Always
}

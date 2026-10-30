//! What SQLite checks about a window before it is used.
//!
//! Invariant: **a window that extends another is checked against the fully
//! resolved base, with the rules SQLite applies when it chains them.** A window
//! may add an `ORDER BY` the base lacks. It may not add a `PARTITION BY`, a
//! second `ORDER BY`, or a frame when the base has one. The second argument of
//! `likelihood()` is checked here as well, because it is the other thing the
//! binder reads from a literal's spelling.

use super::{
    no_such_window, refused, unsupported, wrong_arguments, Binder, BoundExpr, WindowCall,
    MAX_COMPOUND_SELECT,
};
use crate::ast::{self, ExprId};
use crate::diagnostic::ParseError;
use crate::function::AggregateFunc;
use crate::lexer::Span;

impl Binder<'_> {
    /// Refuses a window call with the wrong number of arguments.
    ///
    /// **An aggregate keeps its arity when it has an `OVER` clause.** `sum() OVER
    /// ()` and `sum(*) OVER ()` ran, and the first answered the number of rows;
    /// SQLite refuses both with `wrong number of arguments to function sum()`.
    /// `count()` is `count(*)`, and a registered aggregate declared its arity
    /// when it was registered.
    ///
    /// @param call - the window function or aggregate being called
    /// @param count - how many arguments were written
    /// @param star - whether the call was written `f(*)`
    /// @param name - the function's name as written
    /// @param span - where the call was written
    pub(super) fn check_call_arity(
        &self,
        call: &WindowCall,
        count: usize,
        star: bool,
        name: ast::NameId,
        span: Span,
    ) -> Result<(), ParseError> {
        let fits = match call {
            WindowCall::Plain(func) => {
                let (least, most) = func.arity();
                count >= least && count <= most
            }
            WindowCall::Aggregate(AggregateFunc::External) => true,
            WindowCall::Aggregate(func) => {
                (*func == AggregateFunc::Count && count == 0)
                    || crate::function::aggregate_arity_ok(*func, count, star)
            }
        };
        if fits {
            return Ok(());
        }
        Err(wrong_arguments(self.ast.text(name), span))
    }

    /// Checks the arguments of `likelihood(x, y)`, `likely(x)` and `unlikely(x)`.
    ///
    /// The three share one function here, but SQLite gives each its own count:
    /// `likelihood` takes two and the others take one. SQLite accepts only a
    /// floating point literal from 0.0 to 1.0 as the second argument of
    /// `likelihood`, so an integer such as `1`, a string, a sign, an expression
    /// or a column is refused when the statement is compiled.
    ///
    /// @param name - the folded function name
    /// @param list - the call's arguments
    /// @param span - where the call was written
    pub(super) fn check_likelihood_call(
        &self,
        name: &[u8],
        list: &[ExprId],
        span: Span,
    ) -> Result<(), ParseError> {
        let wanted = if name == b"likelihood" { 2 } else { 1 };
        if list.len() != wanted {
            return Err(wrong_arguments(name, span));
        }
        let Some(probability) = list.get(1) else {
            return Ok(());
        };
        let value = match self.ast.expr(*probability) {
            Some(ast::Expr::Literal(ast::Literal::Float(text))) => std::str::from_utf8(text)
                .ok()
                .and_then(|written| written.parse::<f64>().ok()),
            _ => None,
        };
        if value.is_some_and(|number| (0.0..=1.0).contains(&number)) {
            return Ok(());
        }
        Err(refused(
            "second argument to likelihood() must be a constant between 0.0 and 1.0",
            span,
        ))
    }

    /// Refuses a window call that has an `ORDER BY` inside its parentheses.
    ///
    /// **A window call takes no `ORDER BY` inside its parentheses.** SQLite refuses
    /// `group_concat(a ORDER BY b) OVER ()` and the same for every other function, an aggregate
    /// included; this dropped the clause and answered in row order.
    ///
    /// @param name - the function's name as written
    /// @param has_order_by - whether the call has an `ORDER BY`
    /// @param span - where the call was written
    pub(super) fn refuse_order_by_in_window_call(
        &self,
        name: ast::NameId,
        has_order_by: bool,
        span: Span,
    ) -> Result<(), ParseError> {
        if !has_order_by {
            return Ok(());
        }
        Err(refused(
            format!(
                "ORDER BY may not be used with non-aggregate {}()",
                String::from_utf8_lossy(self.ast.text(name))
            ),
            span,
        ))
    }

    /// Refuses `FILTER` on a function that only exists in a window.
    ///
    /// **`FILTER` belongs to an aggregate.** `row_number() FILTER (WHERE a > 1) OVER ()` ran and
    /// ignored the filter; SQLite refuses it for every function that only exists in a window.
    ///
    /// @param call - the window function or aggregate being called
    /// @param has_filter - whether the call has a `FILTER` clause
    /// @param span - where the call was written
    pub(super) fn refuse_filter_on_plain_window(
        &self,
        call: &WindowCall,
        has_filter: bool,
        span: Span,
    ) -> Result<(), ParseError> {
        if has_filter && matches!(call, WindowCall::Plain(_)) {
            return Err(refused(
                "FILTER clause may only be used with aggregate window functions",
                span,
            ));
        }
        Ok(())
    }

    /// Binds a frame offset.
    ///
    /// **An offset SQLite cannot fold to a literal is NULL there.** SQLite reads `(SELECT 1)`,
    /// `abs(1)`, `coalesce(1, 2)`, `'a' LIKE 'a'` and a column as NULL in a frame offset, and
    /// answers `frame starting offset must be a non-negative integer` when a row reaches the frame.
    /// Literals and the operators between them (`1 + 1`, `CASE`, `CAST`, `IN`) evaluate. This
    /// refused the others when the statement was compiled, with a message SQLite never prints.
    /// Replacing the expression with NULL gives the same runtime error, and no error at all when
    /// the query has no rows, which is also what SQLite does.
    ///
    /// @param expr - the offset as written
    pub(super) fn bind_frame_offset(&mut self, expr: ExprId) -> Result<BoundExpr, ParseError> {
        let bound = self.bind_expr(expr)?;
        if !folds_to_a_literal(&bound) {
            return Ok(BoundExpr::Null);
        }
        Ok(bound)
    }

    /// Resolves an `OVER` clause into one fully-written window specification.
    ///
    /// @param id - the window
    /// @param span - where to point a diagnostic
    pub(super) fn resolve_window(
        &self,
        id: ast::WindowId,
        span: Span,
    ) -> Result<ast::Window, ParseError> {
        self.resolve_window_at(id, span, 0)
    }

    /// Resolves one window, and the windows it extends, to a written specification.
    ///
    /// The window being resolved is checked against its fully resolved base the
    /// way SQLite checks it: it may add an `ORDER BY` when the base has none,
    /// and it may not add a `PARTITION BY` or replace the base's frame.
    ///
    /// @param id - the window
    /// @param span - where to point a diagnostic
    /// @param depth - how many bases have been followed, to stop a window that extends itself
    fn resolve_window_at(
        &self,
        id: ast::WindowId,
        span: Span,
        depth: usize,
    ) -> Result<ast::Window, ParseError> {
        let Some(window) = self.ast.window(id) else {
            return Err(unsupported("missing window", span));
        };
        let mut spec = window.clone();
        // SQLite chains each definition of a `WINDOW` clause to the ones written
        // before it, so the first definition never has its base looked up: a
        // base it names is ignored, found or not.
        let first_defined = self
            .named_windows
            .first()
            .is_some_and(|(_, held)| *held == id);
        if first_defined {
            spec.base = None;
            return Ok(spec);
        }
        let Some(base) = spec.base else {
            return Ok(spec);
        };
        if depth > MAX_COMPOUND_SELECT {
            return Err(unsupported("a window that inherits from itself", span));
        }
        let folded = self.ast.folded(base).to_vec();
        // **The last definition of a name wins, as it does in SQLite.** `WINDOW w AS (ORDER BY a),
        // w AS (ORDER BY a DESC)` makes `OVER w` the descending one, and a window written between
        // the two sees only the definition before it. The first match was used for both.
        let defined_before = self
            .named_windows
            .iter()
            .position(|(_, held)| *held == id)
            .unwrap_or(self.named_windows.len());
        let found = self
            .named_windows
            .iter()
            .take(defined_before)
            .rev()
            .find(|(name, _)| *name == folded);
        let Some((_, base_id)) = found else {
            return Err(no_such_window(self.ast.text(base), span));
        };
        let parent = self.resolve_window_at(*base_id, span, depth.saturating_add(1))?;
        let overridden = if !spec.partition_by.is_empty() {
            Some("PARTITION clause")
        } else if !parent.order_by.is_empty() && !spec.order_by.is_empty() {
            Some("ORDER BY clause")
        } else if parent.unit.is_some() && !spec.bare_name {
            // SQLite accepts `OVER w` when `w` has a frame and refuses `OVER (w)`.
            Some("frame specification")
        } else {
            None
        };
        if let Some(what) = overridden {
            return Err(refused(
                format!(
                    "cannot override {what} of window: {}",
                    String::from_utf8_lossy(self.ast.text(base))
                ),
                span,
            ));
        }
        spec.base = parent.base;
        spec.partition_by = parent.partition_by.clone();
        if spec.order_by.is_empty() {
            spec.order_by = parent.order_by.clone();
        }
        if spec.unit.is_none() {
            spec.unit = parent.unit;
            spec.start = parent.start;
            spec.end = parent.end;
            spec.exclude = parent.exclude;
        }
        Ok(spec)
    }
}

/// Reports whether a frame starts after the point it ends, which is refused whatever its offsets
/// are, as SQLite's `sqlite3WindowAlloc` does: a start of `CURRENT ROW` cannot end `PRECEDING`,
/// and a start of `FOLLOWING` cannot end `PRECEDING` or at `CURRENT ROW`.
///
/// @param start - where the frame starts
/// @param end - where the frame ends
pub(super) fn starts_after_end(
    start: &super::BoundFrameBound,
    end: &super::BoundFrameBound,
) -> bool {
    use super::BoundFrameBound;
    matches!(
        (start, end),
        (BoundFrameBound::CurrentRow, BoundFrameBound::Preceding(_))
            | (
                BoundFrameBound::Following(_),
                BoundFrameBound::Preceding(_) | BoundFrameBound::CurrentRow
            )
    )
}

/// Reports whether SQLite evaluates a frame offset expression, which it does only for the
/// forms it folds to a constant: literals, parameters and the operators between them.
///
/// A function call, a pattern match, a subquery and a column are not in the list, and SQLite
/// reads each as NULL there. See [`Binder::bind_frame_offset`].
///
/// @param expr - the bound offset
fn folds_to_a_literal(expr: &BoundExpr) -> bool {
    let all = |list: &[&BoundExpr]| list.iter().all(|held| folds_to_a_literal(held));
    match expr {
        BoundExpr::Null
        | BoundExpr::Integer(_)
        | BoundExpr::Real(_)
        | BoundExpr::Text(_)
        | BoundExpr::Blob(_)
        | BoundExpr::Parameter(_) => true,
        BoundExpr::Unary { operand, .. }
        | BoundExpr::Collate { operand, .. }
        | BoundExpr::Not(operand)
        | BoundExpr::IsNull { operand, .. }
        | BoundExpr::Cast { operand, .. } => folds_to_a_literal(operand),
        BoundExpr::Arithmetic { left, right, .. }
        | BoundExpr::Compare { left, right, .. }
        | BoundExpr::Is { left, right, .. } => all(&[left, right]),
        BoundExpr::And(left, right) | BoundExpr::Or(left, right) => all(&[left, right]),
        BoundExpr::Between {
            operand, low, high, ..
        } => all(&[operand, low, high]),
        BoundExpr::InList { operand, list, .. } => {
            folds_to_a_literal(operand) && list.iter().all(folds_to_a_literal)
        }
        BoundExpr::Case {
            operand,
            branches,
            otherwise,
            ..
        } => {
            operand.as_ref().is_none_or(|held| folds_to_a_literal(held))
                && branches
                    .iter()
                    .all(|(when, then)| folds_to_a_literal(when) && folds_to_a_literal(then))
                && otherwise
                    .as_ref()
                    .is_none_or(|held| folds_to_a_literal(held))
        }
        _ => false,
    }
}

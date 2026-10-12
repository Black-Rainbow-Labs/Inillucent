//! What SQLite checks about a window before it is used.
//!
//! Invariant: **a window that extends another is checked against the fully
//! resolved base, with the rules SQLite applies when it chains them.** A window
//! may add an `ORDER BY` the base lacks. It may not add a `PARTITION BY`, a
//! second `ORDER BY`, or a frame when the base has one. The second argument of
//! `likelihood()` is checked here as well, because it is the other thing the
//! binder reads from a literal's spelling.

use super::{
    no_such_window, refused, unsupported, wrong_arguments, Binder, WindowCall, MAX_COMPOUND_SELECT,
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
        let Some((_, base_id)) = self.named_windows.iter().find(|(name, _)| *name == folded) else {
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

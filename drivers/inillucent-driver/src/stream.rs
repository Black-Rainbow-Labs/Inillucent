//! Running a statement and handing its rows over one at a time.
//!
//! Invariant: **a streamed statement holds a batch of rows, not its answer.**
//! [`Connection::query`] returns every row it keeps, so a caller that prints a
//! table of three million rows holds three million rows first. [`Connection::query_each`]
//! hands each row to a [`RowSink`] while the engine is still producing the
//! next one, and the sink decides what to keep. The count of rows is exact, as
//! [`Rows::total`] is, because it is counted while streaming.
//!
//! ## Why the sink is held in an `Rc<RefCell<..>>`
//!
//! The engine's pipeline pushes rows, so the statement runs inside the call and
//! the sink has to be reachable from inside it. A sink the engine owns cannot be
//! given back with the caller's own type in it when the statement fails, and a
//! sink the engine borrows would have to outlive the engine's cached plans, which
//! the borrow checker refuses. Holding it in a shared cell lets the caller keep
//! one handle, read what the sink collected after the call returns, and never
//! lose the sink to a failure.
//!
//! The sink must not use the connection the statement is running on.

use std::cell::RefCell;
use std::ops::ControlFlow;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use inillucent_engine::base::budget;
use inillucent_engine::connect::{Flow, RowConsumer};
use inillucent_engine::DbResult;

use crate::error::{Error, Result};
use crate::rows;
use crate::value::{Column, Value};
use crate::Connection;

/// What receives the rows of a statement run with [`Connection::query_each`].
///
/// ```
/// # use inillucent_driver::{Database, Result, RowSink, Value, Column};
/// # use std::{cell::RefCell, ops::ControlFlow, rc::Rc};
/// struct Sum(i64);
/// impl RowSink for Sum {
///     fn row(&mut self, row: &[Value]) -> Result<ControlFlow<()>> {
///         if let Some(Value::Integer(number)) = row.first() {
///             self.0 += number;
///         }
///         Ok(ControlFlow::Continue(()))
///     }
/// }
/// # fn main() -> Result<()> {
/// # let directory = std::env::temp_dir().join(format!("inillucent-doc-stream-{}", std::process::id()));
/// # std::fs::create_dir_all(&directory).ok();
/// let database = Database::open(directory.join("app.rdb"))?;
/// let connection = database.session();
/// connection.execute_batch("CREATE TABLE n (v INTEGER); INSERT INTO n VALUES (1), (2), (3)")?;
/// let sum = Rc::new(RefCell::new(Sum(0)));
/// let done = connection.query_each("SELECT v FROM n", &[], &sum)?;
/// assert_eq!(done.total, 3);
/// assert_eq!(sum.borrow().0, 6);
/// # drop(connection);
/// # drop(database);
/// # std::fs::remove_dir_all(&directory).ok();
/// # Ok(())
/// # }
/// ```
pub trait RowSink: 'static {
    /// Tells the sink the result columns, once, before its first row.
    ///
    /// Called for a statement that produces no rows too. The declared types are
    /// empty, because the engine learns the names of a statement's columns and
    /// not their types.
    ///
    /// @param columns - the result columns, in order
    fn columns(&mut self, _columns: &[Column]) {}

    /// Takes one row.
    ///
    /// Return [`ControlFlow::Break`] to stop the statement early, which is how a
    /// caller that has enough rows stops reading the rest. An error stops it too,
    /// and is returned by [`Connection::query_each`] after the rows that came
    /// before it.
    ///
    /// @param row - the row's values
    fn row(&mut self, row: &[Value]) -> Result<ControlFlow<()>>;
}

/// What a statement run with [`Connection::query_each`] reported when it ended.
///
/// The rows themselves went to the sink, so this has what [`Rows`] has besides
/// them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Streamed {
    /// The result columns, empty for a statement that returns none.
    pub columns: Vec<Column>,
    /// How many rows the statement handed to the sink, exactly.
    pub total: usize,
    /// How many rows the statement changed. `None` for a query.
    pub affected: Option<u64>,
    /// Whether the sink stopped the statement before it produced every row.
    pub stopped: bool,
    /// How long the statement took, measured around the call.
    pub elapsed: Duration,
    /// A one-line summary, composed as [`Rows::tag`] is.
    pub tag: String,
}

/// Adapts a [`RowSink`] to the engine's consumer, converting each row to driver values.
struct Adapter<S: RowSink> {
    /// The caller's sink, which the caller holds a handle on too.
    sink: Rc<RefCell<S>>,
    /// The values of the row being handed over, kept so a row costs no vector.
    scratch: Vec<Value>,
    /// The sink's failure, kept to be returned after the engine has stopped.
    failure: Rc<RefCell<Option<Error>>>,
    /// Whether the sink asked to stop.
    stopped: Rc<std::cell::Cell<bool>>,
}

impl<S: RowSink> RowConsumer for Adapter<S> {
    /// Passes the column names on, with no declared types.
    ///
    /// @param names - the result columns
    fn columns(&mut self, names: &[String]) {
        let columns: Vec<Column> = names
            .iter()
            .map(|name| Column::new(name.clone(), String::new()))
            .collect();
        if let Ok(mut sink) = self.sink.try_borrow_mut() {
            sink.columns(&columns);
        }
    }

    /// Converts a row and hands it to the sink, stopping the statement when the sink says to or
    /// fails.
    ///
    /// @param row - the row's values, borrowed from the page
    fn accept(&mut self, row: &[inillucent_engine::Datum<'_>]) -> DbResult<Flow> {
        self.scratch.clear();
        self.scratch.extend(row.iter().map(Value::from_datum));
        let Ok(mut sink) = self.sink.try_borrow_mut() else {
            return Ok(Flow::Stop);
        };
        match sink.row(&self.scratch) {
            Ok(ControlFlow::Continue(())) => Ok(Flow::Continue),
            Ok(ControlFlow::Break(())) => {
                self.stopped.set(true);
                Ok(Flow::Stop)
            }
            Err(error) => {
                *self.failure.borrow_mut() = Some(error);
                Ok(Flow::Stop)
            }
        }
    }
}

impl Connection<'_> {
    /// Runs one statement and hands each row to a sink as the engine produces it.
    ///
    /// **The memory the statement needs is a batch of rows, not the answer.**
    /// `query` returns the rows it keeps, and the engine behind it holds every row
    /// the statement produced until it ends. This holds none, so reading a table of
    /// three million rows costs what the sink keeps. The count in
    /// [`Streamed::total`] is exact, counted as the rows go by. A statement the
    /// engine cannot run one row at a time (a compound query, a window function, a
    /// write with `RETURNING`) runs as `query` does and its rows are then handed
    /// over, so the sink sees every statement the same way.
    ///
    /// A failure is returned after the rows that came before it, which the sink
    /// already has. In read only mode a statement that is not a query is refused,
    /// as `query` refuses it.
    ///
    /// @param sql - the statement
    /// @param params - the values bound to `?1`, `?2`, ...
    /// @param sink - what takes the columns and then each row; the caller keeps its handle
    pub fn query_each<S: RowSink>(
        &self,
        sql: &str,
        params: &[Value],
        sink: &Rc<RefCell<S>>,
    ) -> Result<Streamed> {
        if self.database.options.read_only {
            self.refuse_if_it_writes(sql)?;
        }
        let started = Instant::now();
        // Armed around the whole statement, as `run` arms it: a statement whose planning is
        // what runs long still has to stop.
        let _budget = budget::arm(
            self.database.options.limits.clone(),
            Arc::clone(&self.database.cancel),
        );
        let mut statement = self
            .engine
            .prepare(sql)
            .map_err(|error| self.database.classify(&error))?;
        crate::bind_all(&mut statement, params)?;
        let failure = Rc::new(RefCell::new(None));
        let stopped = Rc::new(std::cell::Cell::new(false));
        let adapter = Rc::new(RefCell::new(Adapter {
            sink: Rc::clone(sink),
            scratch: Vec::new(),
            failure: Rc::clone(&failure),
            stopped: Rc::clone(&stopped),
        }));
        let ended = statement.stream(&adapter);
        if let Some(error) = failure.borrow_mut().take() {
            return Err(error);
        }
        let ended = ended.map_err(|error| self.database.classify(&error))?;
        Ok(streamed(sql, &ended, stopped.get(), started))
    }
}

/// Builds the summary of a streamed statement the way `collect` builds a [`Rows`].
///
/// @param sql - the statement's text, for the tag
/// @param ended - how the engine reported the statement ended
/// @param stopped - whether the sink stopped it
/// @param started - when the call began
fn streamed(
    sql: &str,
    ended: &inillucent_engine::connect::StreamEnd,
    stopped: bool,
    started: Instant,
) -> Streamed {
    let columns: Vec<Column> = ended
        .names
        .iter()
        .map(|name| Column::new(name.clone(), String::new()))
        .collect();
    // A query is a statement that named columns and changed nothing. An `INSERT ... RETURNING`
    // does both, which is why the count decides rather than the shape.
    let affected = match columns.is_empty() || ended.changed > 0 {
        true => Some(ended.changed as u64),
        false => None,
    };
    let counted = match affected {
        Some(count) if columns.is_empty() => count as usize,
        _ => ended.rows,
    };
    Streamed {
        columns,
        total: ended.rows,
        affected,
        stopped,
        elapsed: started.elapsed(),
        tag: rows::tag_for(sql, counted),
    }
}

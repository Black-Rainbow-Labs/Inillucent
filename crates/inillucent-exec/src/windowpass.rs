//! The window pass: a whole execution strategy, reached from one line.
//!
//! Invariant: **a window reads its partition in both directions, so the rows
//! are buffered and not streamed.** `last_value` looks forward and `lag` looks
//! back, which no pipeline of sinks can serve: the input is collected, sorted
//! into the order each frame needs, and every window value is computed over the
//! buffer before a single row is handed on.
//!
//! This is here rather than in `physical.rs` because it is a strategy rather
//! than a stage. `run_any_prepared` chooses it and nothing else in that file
//! calls anything below; keeping it there made a 6,600 line module 7,200, and
//! the module size ratchet asks for an extraction rather than a larger number.

use inillucent_base::error::misuse;
use inillucent_base::DbResult;
use inillucent_sql::bind::{
    BoundExpr, BoundFrameBound, BoundOrderTerm, BoundResultColumn, BoundSelect, BoundWindow,
};
use inillucent_sql::plan::{plan_select_with, Levers, PhysicalPlan};
use inillucent_tree::datum::OwnedDatum;
use inillucent_value::collation::Collation;

use inillucent_sql::ast::{NullOrder, SortOrder};
use inillucent_sql::bind::WindowCall as BoundWindowCall;
use inillucent_sql::function::AggregateFunc;

use crate::aggregate::{AggregateKind, Percentile};
use crate::expr::{compile, Expr, StaticType};
use crate::ops::{CollectInto, Distinct, Limit, Project, Sink, Sort, SortKey};
use crate::physical::{
    build_prepared, constant_count, distinct_collations, expression_collation, prepare, translate,
    unsupported, ForcePlan, Frame, HeldSpace, Negative, Params, Shape, TreeCatalog,
};
use crate::window::{
    FrameEnd, OrderTerm as WindowOrderTerm, StreamShape, WindowCall, WindowFrame, WindowPlan,
    WindowSlot, WindowStream,
};

/// Runs a query with window functions.
///
/// **A window pass is a sort, a buffer and an append.** The rows arrive sorted
/// by the window's partition keys and its `ORDER BY`, [`crate::window::compute`]
/// appends one value per call to each row, and the statement's result columns
/// are then projected out of the widened row. That is what the operator is
/// written to expect - it addresses everything by buffered column number - so
/// what this function does is decide the column numbers.
///
/// ## Why the input rows come off an ordinary plan
///
/// The sort and the scan below a window pass are not special. Building an inner
/// `SELECT` whose result columns are exactly the values the pass needs, and
/// whose `ORDER BY` is the window's own, means the input comes off the *read
/// path* - index selection, an ordering the tree already provides, and the
/// merge over written-to leaves all included - rather than off a second scan
/// written here that would have to be kept in step with it.
///
/// ## What it refuses, and why those are the honest boundaries
///
/// Every call has to share one `PARTITION BY` **and** one `ORDER BY`, because
/// the operator computes each call's peer groups over a sequence it assumes is
/// sorted by that call's ordering, and one buffer can only be sorted one way.
/// Two different windows are two passes and two sorts; that is a real feature,
/// and it is refused by name rather than answered wrongly.
///
/// A window beside an aggregate is refused for a related reason: the pass would
/// have to run over the *grouped* rows, and the grouped rows are a different
/// space from the scan's.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param params - the values bound to `?1`, `?2`, ...
pub fn run_windowed(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    params: &Params,
) -> DbResult<(Vec<Vec<OwnedDatum>>, Shape)> {
    let rows = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let sink = Box::new(CollectInto::new(std::rc::Rc::clone(&rows)));
    match run_windowed_into(plan, catalog, params, sink) {
        Ok(shape) => {
            let collected = std::mem::take(&mut *rows.borrow_mut());
            Ok((collected, shape))
        }
        Err(error) => {
            // SQLite answers the rows before the one that raised the error, so the rows the
            // pipeline delivered before it failed are kept for whoever reports the failure.
            crate::physical::keep_partial_rows(std::mem::take(&mut *rows.borrow_mut()));
            Err(error)
        }
    }
}

/// Runs a query with window functions into a sink.
///
/// **The input is read once, in order, and each partition is computed and handed on before the
/// next is read.** The inner query that supplies the rows is pushed into one operator for each
/// distinct window frame ([`crate::window::WindowStream`]), and the rows come out of the last of
/// them into the projection, the `ORDER BY`, the `DISTINCT` and the `LIMIT` the statement has.
/// Nothing holds more than a partition (or, for the calls that only read what came before, a run
/// of peer groups) unless an operator above it does.
///
/// ## Several frames
///
/// Calls with different `PARTITION BY` or `ORDER BY` need the rows in different orders. Each
/// row is numbered as it arrives, the pass of the first frame fills its calls' slots, a sorter puts
/// the rows in the second frame's order (ties by the number, as the stable sort of the buffered
/// rows did), the second pass fills its slots, and so on. A last sort by the number puts the rows
/// back in the order the first frame read them in, which is the order the answer has when the
/// statement names none. The sorters write runs when they hold too much.
///
/// @param plan - the planner's output
/// @param catalog - where the trees and layouts come from
/// @param params - the values bound to `?1`, `?2`, ...
/// @param sink - where the answer goes; it is finished when the answer ends
pub fn run_windowed_into(
    plan: &PhysicalPlan,
    catalog: &dyn TreeCatalog,
    params: &Params,
    sink: Box<dyn Sink>,
) -> DbResult<Shape> {
    // Every uncorrelated subquery is answered once, here, before anything is
    // built over it. See `crate::subquery` for why it is per execution.
    let folded = crate::subquery::fold(plan, catalog, params)?;
    let params = folded.as_ref().unwrap_or(params);
    let select = &plan.select;
    if !select.compounds.is_empty() {
        return unsupported("a window function in a compound arm");
    }
    let Some(first) = select.windows.first() else {
        return unsupported("a window pass with no window in it");
    };
    // **One pass per distinct window frame, not one pass per statement.** Two
    // calls that share a `PARTITION BY` and an `ORDER BY` see the same
    // partitions and the same peer groups, so they are computed together over
    // one ordering; two that do not need the rows in two different orders and
    // there is no single sort that serves both.
    let groups = window_groups(select);
    let pre = window_inputs(select);
    // The output row is the buffered values followed by one slot per call, in
    // the order the binder numbered them - which is the space
    // `Frame::Window` addresses and the reason the slots are filled by
    // scattering rather than by appending.
    let width = pre.len();
    let slots = select.windows.len();
    let several = groups.len() > 1;
    let shape = Shape {
        names: select
            .columns
            .iter()
            .map(|column| column.name.clone())
            .collect(),
        operators: vec![
            "SCAN".to_string(),
            "SORT".to_string(),
            "WINDOW".to_string(),
            "PROJECT".to_string(),
        ],
    };
    let mut next = projection_chain(select, &pre, width, params, catalog, sink)?;
    if several {
        next = restoring_order(next, width, slots, catalog)?;
    }
    for (position, group) in groups.iter().enumerate().rev() {
        let pass = window_plan(select, &pre, group)?;
        let shape = StreamShape {
            width,
            total_slots: slots,
            first: position == 0,
            carries_seq: several,
            // Rows before a failing row are handed on only when this pass's rows go straight on
            // to the answer.
            partial_rows_on_failure: !several,
        };
        let stream = WindowStream::new(pass, group.slots.clone(), shape, next);
        next = if position == 0 {
            Box::new(stream)
        } else {
            let keys = group_sort_keys(&pre, group, width.saturating_add(slots))?;
            Box::new(Sort::new(keys, Box::new(stream)).spilling_to(catalog.spill()))
        };
    }
    run_inner_into(select, &pre, first, catalog, params, plan.levers, next)?;
    Ok(shape)
}

/// One set of window calls that share a frame.
struct WindowGroup {
    /// The `PARTITION BY` and `ORDER BY` every call in the group shares.
    window: BoundWindow,
    /// Which of `select.windows` the group holds, by the binder's slot.
    slots: Vec<usize>,
}

/// Groups a statement's window calls by the frame they share.
///
/// @param select - the bound statement
fn window_groups(select: &BoundSelect) -> Vec<WindowGroup> {
    let mut groups: Vec<WindowGroup> = Vec::new();
    for (slot, call) in select.windows.iter().enumerate() {
        match groups.iter_mut().find(|group| {
            group.window.partition_by == call.partition_by && group.window.order_by == call.order_by
        }) {
            Some(group) => group.slots.push(slot),
            None => groups.push(WindowGroup {
                window: call.clone(),
                slots: vec![slot],
            }),
        }
    }
    groups
}

/// Returns the terms a sorter orders rows by for one window group: the partition keys, the
/// window's own ordering, and then the number the row arrived with.
///
/// @param pre - the buffered row's expressions
/// @param group - the group whose frame decides the order
/// @param seq - the column holding the number the row arrived with
fn group_sort_keys(pre: &[BoundExpr], group: &WindowGroup, seq: usize) -> DbResult<Vec<SortKey>> {
    let mut keys: Vec<SortKey> = Vec::new();
    // A partition key only has to bring a partition's rows together, so its
    // direction is free; the ordering terms are the window's own and are not.
    for expr in &group.window.partition_by {
        keys.push(SortKey {
            column: column_of(pre, expr)?,
            descending: false,
            collation: expression_collation(expr),
            nulls_first: true,
        });
    }
    for term in &group.window.order_by {
        keys.push(SortKey {
            column: column_of(pre, &term.expr)?,
            descending: term.order == SortOrder::Descending,
            collation: term.collation,
            nulls_first: term.nulls == NullOrder::First,
        });
    }
    keys.push(SortKey {
        column: seq,
        descending: false,
        collation: Collation::Binary,
        nulls_first: true,
    });
    Ok(keys)
}

/// Puts a sorter by arrival number and a trim of that number in front of a sink.
///
/// @param next - what takes the rows in the order they arrived, without the number
/// @param width - how many values the passes read
/// @param slots - how many window calls the statement has
/// @param catalog - where the sorter writes its runs
fn restoring_order(
    next: Box<dyn Sink>,
    width: usize,
    slots: usize,
    catalog: &dyn TreeCatalog,
) -> DbResult<Box<dyn Sink>> {
    let whole = width.saturating_add(slots);
    let types = vec![StaticType::Unknown; whole.saturating_add(1)];
    let kept: Vec<Box<dyn crate::expr::Eval>> = (0..whole)
        .map(|column| compile(&Expr::Column(column), &types))
        .collect::<DbResult<Vec<_>>>()?;
    let trimmed: Box<dyn Sink> = Box::new(Project::new(kept, next));
    let key = SortKey {
        column: whole,
        descending: false,
        collation: Collation::Binary,
        nulls_first: true,
    };
    Ok(Box::new(
        Sort::new(vec![key], trimmed).spilling_to(catalog.spill()),
    ))
}

/// Returns the values a window pass reads, in buffered-row order.
///
/// The partition keys and the window's ordering come first, because the inner
/// query is ordered by them and reading them out of the same columns it sorted
/// by is one fewer thing to keep in step. Everything else is appended as it is
/// met, deduplicated, so a value used twice occupies one column.
///
/// @param select - the bound statement
fn window_inputs(select: &BoundSelect) -> Vec<BoundExpr> {
    let mut pre: Vec<BoundExpr> = Vec::new();
    // The *first* window's frame first, because the inner query is sorted by it
    // and reading it out of the same columns it sorted by is one fewer thing to
    // keep in step. Every other window's frame is gathered below with the rest.
    if let Some(first) = select.windows.first() {
        for expr in &first.partition_by {
            remember(&mut pre, expr);
        }
        for term in &first.order_by {
            remember(&mut pre, &term.expr);
        }
    }
    for call in &select.windows {
        for expr in &call.partition_by {
            remember(&mut pre, expr);
        }
        for term in &call.order_by {
            remember(&mut pre, &term.expr);
        }
        for expr in &call.arguments {
            remember(&mut pre, expr);
        }
        if let Some(filter) = &call.filter {
            remember(&mut pre, filter);
        }
        for bound in [&call.start, &call.end] {
            if let BoundFrameBound::Preceding(expr) | BoundFrameBound::Following(expr) = bound {
                remember(&mut pre, expr);
            }
        }
    }
    // Every leaf the statement's own expressions read, so the projection above
    // the pass has somewhere to read them from.
    for column in &select.columns {
        gather_leaves(&column.expr, &mut pre);
    }
    for term in &select.order_by {
        gather_leaves(&term.expr, &mut pre);
    }
    pre
}

/// Adds one expression to the buffered row if it is not already there.
///
/// @param pre - the buffered row's expressions
/// @param expr - the expression to carry
fn remember(pre: &mut Vec<BoundExpr>, expr: &BoundExpr) {
    if !pre.iter().any(|held| held == expr) {
        pre.push(expr.clone());
    }
}

/// Adds every column and rowid an expression reads to the buffered row.
///
/// A window reference is a leaf and is deliberately *not* gathered: it names a
/// value the pass is about to compute, which does not exist in its input.
///
/// @param expr - the expression to walk
/// @param pre - the buffered row's expressions
fn gather_leaves(expr: &BoundExpr, pre: &mut Vec<BoundExpr>) {
    match expr {
        BoundExpr::WindowRef { .. } => {}
        // An aggregate is a leaf here for the same reason a column is: the
        // inner query computes it, and what the projection above the pass reads
        // is the value rather than the call. Without this a statement that
        // aggregates *and* windows lost its `count(*)` between the two passes,
        // which is why the two used to be refused together.
        BoundExpr::Column { .. } | BoundExpr::Rowid { .. } | BoundExpr::Aggregate { .. } => {
            remember(pre, expr)
        }
        // **A correlated subquery is a leaf too.** It reads the row it sits
        // in, and the projection above the pass has no row of a table to give
        // it, so `SELECT o.id, (SELECT count(*) FROM order_line l WHERE
        // l.order_id = o.id), row_number() OVER (...) FROM orders o` was
        // refused as "a correlated subquery used as a value". The inner query
        // below the pass answers one like any other `SELECT` does, so it is
        // computed there and read back as a column.
        BoundExpr::Subquery { block, .. } if !block.correlations.is_empty() => remember(pre, expr),
        other => {
            for child in other.children() {
                gather_leaves(child, pre);
            }
        }
    }
}

/// Runs the query that supplies a window pass's rows into a sink.
///
/// @param select - the bound statement
/// @param pre - the buffered row's expressions
/// @param window - the window every call shares
/// @param catalog - where the trees and layouts come from
/// @param params - the bound parameters
/// @param levers - the planner levers the statement was planned with
/// @param sink - where the rows go, already sorted by the first window's frame
fn run_inner_into(
    select: &BoundSelect,
    pre: &[BoundExpr],
    window: &BoundWindow,
    catalog: &dyn TreeCatalog,
    params: &Params,
    levers: Levers,
    sink: Box<dyn Sink>,
) -> DbResult<()> {
    let mut inner = select.clone();
    inner.columns = pre
        .iter()
        .map(|expr| BoundResultColumn {
            expr: expr.clone(),
            name: b"w".to_vec(),
            origin: None,
            declared_type: Vec::new(),
            written: None,
        })
        .collect();
    inner.windows.clear();
    inner.distinct = false;
    inner.limit = None;
    inner.offset = None;
    // The pass's own ordering: the partition keys, then the window's `ORDER BY`.
    // A partition key only has to bring a partition's rows together, so its
    // direction is free; the ordering terms are the window's own and are not.
    inner.order_by = window
        .partition_by
        .iter()
        .map(|expr| BoundOrderTerm {
            expr: expr.clone(),
            order: SortOrder::Ascending,
            nulls: NullOrder::First,
            collation: expression_collation(expr),
        })
        .chain(window.order_by.iter().cloned())
        .collect();
    // The statement's own levers, so an optimization a pragma turned off for
    // the statement is off for the rows its window reads as well.
    let planned = plan_select_with(inner, levers);
    let prepared = prepare(&planned, catalog, ForcePlan::default())?;
    let (mut pipeline, _) = build_prepared(&planned, catalog, &prepared, params, sink)?;
    pipeline.run()
}

/// Builds one window pass, addressing everything by buffered column.
///
/// A pass covers exactly the calls that share a frame, which is what
/// `window_groups` decided: the partitions and the peer groups are properties
/// of the frame, so calls with different ones cannot be computed over one
/// ordering of the rows.
///
/// @param select - the bound statement
/// @param pre - the buffered row's expressions
/// @param group - the calls sharing one frame, and the frame
fn window_plan(
    select: &BoundSelect,
    pre: &[BoundExpr],
    group: &WindowGroup,
) -> DbResult<WindowPlan> {
    let window = &group.window;
    let partition = window
        .partition_by
        .iter()
        .map(|expr| Ok((column_of(pre, expr)?, expression_collation(expr))))
        .collect::<DbResult<Vec<(usize, Collation)>>>()?;
    let order = window
        .order_by
        .iter()
        .map(|term| {
            Ok(WindowOrderTerm {
                column: column_of(pre, &term.expr)?,
                descending: term.order == SortOrder::Descending,
                collation: term.collation,
            })
        })
        .collect::<DbResult<Vec<WindowOrderTerm>>>()?;
    let mut calls = Vec::with_capacity(group.slots.len());
    for call in group
        .slots
        .iter()
        .filter_map(|slot| select.windows.get(*slot))
    {
        let func = match call.call {
            BoundWindowCall::Plain(plain) => WindowSlot::Plain(plain),
            BoundWindowCall::Aggregate(aggregate) => WindowSlot::Aggregate(match aggregate {
                AggregateFunc::Count if call.star => AggregateKind::CountStar,
                AggregateFunc::Count => AggregateKind::Count,
                AggregateFunc::Sum => AggregateKind::Sum,
                AggregateFunc::Total => AggregateKind::Total,
                AggregateFunc::Avg => AggregateKind::Average,
                AggregateFunc::Min => AggregateKind::Minimum,
                AggregateFunc::Max => AggregateKind::Maximum,
                // **The same kinds a grouped call gets.** A window aggregate
                // folds each frame through the grouped accumulator, so a kind
                // that keeps whole rows works here once the whole row is handed
                // to it; see `window::aggregate`. These were refused with exit
                // code 3, and SQLite answers every one of them over a window.
                AggregateFunc::GroupConcat => match call.arguments.get(1) {
                    None => AggregateKind::GroupConcat(",".to_string()),
                    Some(BoundExpr::Text(bytes)) => {
                        AggregateKind::GroupConcat(String::from_utf8_lossy(bytes).into_owned())
                    }
                    Some(_) => AggregateKind::GroupConcatComputed,
                },
                AggregateFunc::JsonGroupArray => AggregateKind::JsonGroupArray(false),
                AggregateFunc::JsonbGroupArray => AggregateKind::JsonGroupArray(true),
                AggregateFunc::JsonGroupObject => AggregateKind::JsonGroupObject(false),
                AggregateFunc::JsonbGroupObject => AggregateKind::JsonGroupObject(true),
                AggregateFunc::Median => AggregateKind::Percentile(Percentile::Median),
                AggregateFunc::Percentile => AggregateKind::Percentile(Percentile::Hundredths),
                AggregateFunc::PercentileCont => AggregateKind::Percentile(Percentile::Continuous),
                AggregateFunc::PercentileDisc => AggregateKind::Percentile(Percentile::Discrete),
                other => return unsupported(&format!("the aggregate {other:?} over a window")),
            }),
        };
        let arguments = call
            .arguments
            .iter()
            .map(|expr| column_of(pre, expr))
            .collect::<DbResult<Vec<usize>>>()?;
        let filter = match &call.filter {
            Some(expr) => Some(column_of(pre, expr)?),
            None => None,
        };
        // Which arguments a JSON function produced, for the JSON group
        // aggregates: the value is read out of a buffered column, which cannot
        // say where it came from.
        let json_marks = call
            .arguments
            .iter()
            .enumerate()
            .filter(|(_, expr)| inillucent_sql::bind::always_json(expr))
            .fold(0u32, |marks, (at, _)| {
                marks | 1u32.checked_shl(at as u32).unwrap_or(0)
            });
        calls.push(WindowCall {
            func,
            json_marks,
            distinct: call.distinct,
            collation: call.collation,
            arguments,
            filter,
            order: order.clone(),
            frame: WindowFrame {
                unit: call.unit,
                start: frame_end(pre, &call.start)?,
                end: frame_end(pre, &call.end)?,
                exclude: call.exclude,
            },
        });
    }
    Ok(WindowPlan { partition, calls })
}

/// Returns one frame end, with its offset resolved to a buffered column.
///
/// @param pre - the buffered row's expressions
/// @param bound - the written bound
fn frame_end(pre: &[BoundExpr], bound: &BoundFrameBound) -> DbResult<FrameEnd> {
    Ok(match bound {
        BoundFrameBound::UnboundedPreceding => FrameEnd::UnboundedPreceding,
        BoundFrameBound::CurrentRow => FrameEnd::CurrentRow,
        BoundFrameBound::UnboundedFollowing => FrameEnd::UnboundedFollowing,
        BoundFrameBound::Preceding(expr) => FrameEnd::Offset {
            column: column_of(pre, expr)?,
            preceding: true,
        },
        BoundFrameBound::Following(expr) => FrameEnd::Offset {
            column: column_of(pre, expr)?,
            preceding: false,
        },
    })
}

/// Returns which buffered column holds one of the pass's inputs.
///
/// Every input was put there by [`window_inputs`], so a miss is a disagreement
/// between that function and this one rather than a query the engine cannot
/// answer - and it says so, because the two are a pair that has to stay in step.
///
/// @param pre - the buffered row's expressions
/// @param expr - the expression to find
fn column_of(pre: &[BoundExpr], expr: &BoundExpr) -> DbResult<usize> {
    pre.iter()
        .position(|held| held == expr)
        .ok_or_else(|| misuse("a window pass did not carry a value its own plan reads"))
}

/// Builds the operators that turn widened rows into the statement's answer.
///
/// The rows are pushed into the returned sink; it projects the result columns, and orders, de-duplicates
/// and limits them as the statement says.
///
/// **The ordering happens below the projection, not above it.** Under
/// [`Frame::Window`] a translated expression addresses the *widened* row - the
/// values the pass was given, then one per call - so a sort built from those
/// expressions has to run while the batch is still that row. Putting it above
/// the projection sorted by whichever output column happened to share the
/// index, which is a wrong answer rather than an error: `ORDER BY id` sorted by
/// the window value instead and the rows came back in the pass's input order.
///
/// An ordinal or an alias is the one term that genuinely names an *output*
/// column, and it is resolved by translating that column's own expression
/// rather than by moving the sort - so both kinds of term end up addressing the
/// same row.
///
/// @param select - the bound statement
/// @param pre - the buffered row's expressions
/// @param width - how many columns the buffered row had before the pass
/// @param catalog - where a sort that does not fit in memory writes its runs
/// @param sink - where the answer goes
/// @param params - the bound parameters
fn projection_chain(
    select: &BoundSelect,
    pre: &[BoundExpr],
    width: usize,
    params: &Params,
    catalog: &dyn TreeCatalog,
    sink: Box<dyn Sink>,
) -> DbResult<Box<dyn Sink>> {
    let held = HeldSpace {
        layouts: Vec::new(),
        types: Vec::new(),
        order: Vec::new(),
    };
    let space = held.view(&[]);
    let frame = Frame::Window { pre, width };
    let types = vec![StaticType::Unknown; width.saturating_add(select.windows.len())];
    let mut projected = Vec::with_capacity(select.columns.len());
    for column in &select.columns {
        let translated = translate(&column.expr, &space, params, frame)?;
        projected.push(compile(&translated, &types)?);
    }
    let mut keys = Vec::with_capacity(select.order_by.len());
    // **A computed sort key is appended to the row before the sort.** The
    // sorter reads a column, and `ORDER BY low DESC` over `on_hand <=
    // reorder_level AS low`, or `ORDER BY lower(name)`, is not one, so both
    // were refused as "a windowed query ordered by a computed expression".
    // The key is evaluated over the widened row, appended after it, and
    // sorted by; the projection reads only the columns before it.
    let whole = width.saturating_add(select.windows.len());
    let mut computed_keys: Vec<Box<dyn crate::expr::Eval>> = Vec::new();
    for term in &select.order_by {
        // An ordinal or an alias names an output column, so what it orders by
        // is that column's expression - which reads the widened row like every
        // other term here.
        let expr = match &term.expr {
            BoundExpr::SorterColumn { column } => select
                .columns
                .get(usize::from(*column))
                .map(|held| &held.expr)
                .ok_or_else(|| misuse("an ORDER BY ordinal outside the result list"))?,
            other => other,
        };
        let column = match translate(expr, &space, params, frame)? {
            Expr::Column(column) => column,
            translated => {
                computed_keys.push(compile(&translated, &types)?);
                whole.saturating_add(computed_keys.len()).saturating_sub(1)
            }
        };
        let descending = term.order == SortOrder::Descending;
        keys.push(SortKey {
            column,
            descending,
            collation: term.collation,
            nulls_first: match term.nulls {
                NullOrder::First => true,
                NullOrder::Last => false,
            },
        });
    }

    let limit = constant_count(select.limit.as_ref(), params, Negative::NoLimit)?;
    let offset = constant_count(select.offset.as_ref(), params, Negative::Zero)?;
    let mut head: Box<dyn Sink> = match (limit, offset) {
        (None, None) => sink,
        (limit, offset) => Box::new(Limit::new(
            limit.unwrap_or(usize::MAX),
            offset.unwrap_or(0),
            sink,
        )),
    };
    if select.distinct {
        head =
            Box::new(Distinct::new(distinct_collations(select), head).spilling_to(catalog.spill()));
    }
    head = Box::new(Project::new(projected, head));
    if !keys.is_empty() {
        head = Box::new(Sort::new(keys, head).spilling_to(catalog.spill()));
    }
    if !computed_keys.is_empty() {
        let mut extended: Vec<Box<dyn crate::expr::Eval>> = (0..whole)
            .map(|column| compile(&Expr::Column(column), &types))
            .collect::<DbResult<Vec<_>>>()?;
        extended.extend(computed_keys);
        head = Box::new(Project::new(extended, head));
    }
    Ok(head)
}

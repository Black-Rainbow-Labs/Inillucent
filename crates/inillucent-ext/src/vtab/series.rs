//! `generate_series`, the smallest complete virtual table.
//!
//! Invariant: it is here because it is the module the contract is *tested*
//! with. It has hidden columns that are really arguments, a `best_index` that
//! reports a real cost and a real ordering, an `ORDER BY` it can satisfy
//! without a sorter, and a plan that changes with the constraints - which is
//! every part of the interface a module can exercise without a single page of
//! storage. A contract that only ever ran FTS5 would be a contract nobody could
//! debug.

use inillucent_base::DbResult;
use inillucent_value::{cast, Value};

use super::{
    ConstraintOp, Context, Declaration, DeclaredColumn, FilterPlan, IndexQuery, Module,
    ModuleArguments, VirtualCursor, VirtualTable,
};

/// The visible column.
const VALUE: usize = 0;
/// The hidden column holding the first value.
const START: usize = 1;
/// The hidden column holding the last value.
const STOP: usize = 2;
/// The hidden column holding the step.
const STEP: usize = 3;

/// The `generate_series` module.
pub struct SeriesModule;

impl Module for SeriesModule {
    /// Returns the module's name.
    fn name(&self) -> &str {
        "generate_series"
    }

    /// It is a name rather than a table.
    fn eponymous(&self) -> bool {
        true
    }

    /// Connects, which is only declaring the four columns.
    fn connect(
        &self,
        _arguments: &ModuleArguments,
        _creating: bool,
    ) -> DbResult<Box<dyn VirtualTable>> {
        Ok(Box::new(SeriesTable {
            declaration: Declaration {
                columns: vec![
                    DeclaredColumn::visible("value"),
                    DeclaredColumn::hidden("start"),
                    DeclaredColumn::hidden("stop"),
                    DeclaredColumn::hidden("step"),
                ],
                without_rowid: false,
            },
        }))
    }
}

/// The plan bits, one per argument the scan was given.
const HAS_START: i32 = 1;
/// The plan bit for a stop value.
const HAS_STOP: i32 = 2;
/// The plan bit for a step value.
const HAS_STEP: i32 = 4;
/// The plan bit that says the scan runs backwards.
const DESCENDING: i32 = 8;
/// The plan bit that says the scan runs forwards whatever the step's sign.
const ASCENDING: i32 = 16;
/// The plan bit for `value = X`, or `rowid = X`.
const VALUE_EQ: i32 = 0x0080;
/// The plan bit for `value >= X`.
const VALUE_GE: i32 = 0x0100;
/// The plan bit for `value > X`.
const VALUE_GT: i32 = 0x0200;
/// The plan bit for `value <= X`.
const VALUE_LE: i32 = 0x1000;
/// The plan bit for `value < X`.
const VALUE_LT: i32 = 0x2000;

/// One connected `generate_series`.
struct SeriesTable {
    declaration: Declaration,
}

impl VirtualTable for SeriesTable {
    /// Returns the four columns.
    fn declaration(&self) -> &Declaration {
        &self.declaration
    }

    /// Takes the three hidden columns, the comparisons on `value`, and the
    /// ordering when both ends of the series are known. This is SQLite's
    /// `seriesBestIndex`, bit for bit.
    ///
    /// **Every constraint taken is promised, and so never tested again.** That
    /// is SQLite's build, and it is visible: the module reads each value with
    /// its own conversion, so `WHERE value = '3'` finds the row 3 and `stop IN
    /// ('3')` stops at 3, where a test of the row against the text `'3'` would
    /// find nothing. A comparison on `rowid` is a comparison on `value`, which
    /// the rowid equals.
    ///
    /// A series with no first value and no lower bound on `value` is refused
    /// with SQLite's message rather than walked from 0.
    fn best_index(&self, info: &mut IndexQuery) -> DbResult<()> {
        let mut plan = 0i32;
        let mut start_seen = false;
        // Start, stop, step, the lower or equal bound on value, the upper bound.
        let mut chosen: [Option<usize>; 5] = [None; 5];
        for (index, constraint) in info.constraints.iter().enumerate() {
            let column = constraint.column;
            if column < START as i32 {
                if column != VALUE as i32 && column != inillucent_sql::vtab::ROWID_COLUMN {
                    continue;
                }
                if !constraint.usable {
                    continue;
                }
                match constraint.op {
                    ConstraintOp::Eq | ConstraintOp::Is => {
                        plan |= VALUE_EQ;
                        plan &= !(VALUE_GE | VALUE_GT | VALUE_LE | VALUE_LT);
                        chosen[3] = Some(index);
                        chosen[4] = None;
                        start_seen = true;
                    }
                    ConstraintOp::Ge | ConstraintOp::Gt if plan & VALUE_EQ == 0 => {
                        let (set, clear) = if constraint.op == ConstraintOp::Ge {
                            (VALUE_GE, VALUE_GT)
                        } else {
                            (VALUE_GT, VALUE_GE)
                        };
                        plan |= set;
                        plan &= !clear;
                        chosen[3] = Some(index);
                        start_seen = true;
                    }
                    ConstraintOp::Le | ConstraintOp::Lt if plan & VALUE_EQ == 0 => {
                        let (set, clear) = if constraint.op == ConstraintOp::Le {
                            (VALUE_LE, VALUE_LT)
                        } else {
                            (VALUE_LT, VALUE_LE)
                        };
                        plan |= set;
                        plan &= !clear;
                        chosen[4] = Some(index);
                    }
                    _ => {}
                }
                continue;
            }
            let Some(slot) = usize::try_from(column - START as i32)
                .ok()
                .filter(|slot| *slot < 3)
            else {
                continue;
            };
            if slot == 0 && constraint.op == ConstraintOp::Eq {
                start_seen = true;
            }
            if constraint.usable && constraint.op == ConstraintOp::Eq {
                plan |= 1 << slot;
                if let Some(held) = chosen.get_mut(slot) {
                    *held = Some(index);
                }
            }
        }
        for index in chosen.into_iter().flatten() {
            info.use_constraint(index, true);
        }
        if !start_seen {
            return Err(inillucent_base::error::refusal(
                "first argument to \"generate_series()\" missing or unusable",
            ));
        }
        if plan & (HAS_START | HAS_STOP) == HAS_START | HAS_STOP {
            info.estimated_cost = if plan & HAS_STEP != 0 { 1.0 } else { 2.0 };
            info.estimated_rows = 1000;
            if let Some(order) = info.order_by.first() {
                if order.column as usize == VALUE {
                    info.ordered = true;
                    plan |= if order.descending {
                        DESCENDING
                    } else {
                        ASCENDING
                    };
                }
            }
        } else {
            info.estimated_rows = 2_147_483_647;
        }
        info.index_number = plan;
        Ok(())
    }

    /// Opens a cursor.
    fn open(&self) -> DbResult<Box<dyn VirtualCursor>> {
        Ok(Box::new(SeriesCursor {
            value: 0,
            term: 0,
            step: 1,
            descending: false,
            done: true,
            arguments: (0, 0xffff_ffff, 1),
        }))
    }
}

/// A cursor walking one series.
struct SeriesCursor {
    /// The current value.
    value: i64,
    /// The last value, which the walk stops on.
    term: i64,
    /// The distance between values, always positive. It is unsigned because
    /// the magnitude of a step of `i64::MIN` is 2^63.
    step: u64,
    /// Whether the walk runs downwards.
    descending: bool,
    /// Whether the walk is over.
    done: bool,
    /// The start, stop and step the hidden columns report.
    arguments: (i64, i64, i64),
}

/// Returns `a - b` as an unsigned distance, for `a >= b`.
///
/// @param a - the larger value
/// @param b - the smaller value
fn span(a: i64, b: i64) -> u64 {
    (a as u64).wrapping_sub(b as u64)
}

/// Returns `a + b`, wrapping as two's complement does.
///
/// @param a - the value
/// @param b - the distance
fn add(a: i64, b: u64) -> i64 {
    (a as u64).wrapping_add(b) as i64
}

/// Returns `a - b`, wrapping as two's complement does.
///
/// @param a - the value
/// @param b - the distance
fn sub(a: i64, b: u64) -> i64 {
    (a as u64).wrapping_sub(b) as i64
}

/// Returns the integer a double truncates to, clamped to the range of an integer.
///
/// @param r - the double
fn real_to_integer(r: f64) -> i64 {
    if r < -9_223_372_036_854_774_784.0 {
        i64::MIN
    } else if r > 9_223_372_036_854_774_784.0 {
        i64::MAX
    } else {
        r as i64
    }
}

/// Returns a constraint value as a double when SQLite reads it as one.
///
/// `sqlite3_value_numeric_type` answers `FLOAT` for a real and for text that
/// reads as a real, and the module then works with the double.
///
/// @param value - the value the constraint was given
fn as_real(value: &Value<'static>) -> Option<f64> {
    match inillucent_value::affinity::apply_numeric_affinity(value.clone(), false) {
        Value::Real(real) => Some(real),
        _ => None,
    }
}

/// Returns the lower bound one `value` constraint puts on the series.
///
/// `None` when no integer can satisfy it.
///
/// @param value - the constraint's value
/// @param strict - true for `>`, false for `>=`
fn lower_bound(value: &Value<'static>, strict: bool) -> Option<i64> {
    let Some(r) = as_real(value) else {
        let low = cast::integer_value(value);
        return if strict {
            low.checked_add(1)
        } else {
            Some(low)
        };
    };
    if r <= i64::MIN as f64 {
        return Some(i64::MIN);
    }
    if r > i64::MAX as f64 {
        return None;
    }
    let low = real_to_integer(r.ceil());
    if strict && r == r.ceil() {
        return low.checked_add(1);
    }
    Some(low)
}

/// Returns the upper bound one `value` constraint puts on the series.
///
/// `None` when no integer can satisfy it.
///
/// @param value - the constraint's value
/// @param strict - true for `<`, false for `<=`
fn upper_bound(value: &Value<'static>, strict: bool) -> Option<i64> {
    let Some(r) = as_real(value) else {
        let high = cast::integer_value(value);
        return if strict {
            high.checked_sub(1)
        } else {
            Some(high)
        };
    };
    if r >= i64::MAX as f64 {
        return Some(i64::MAX);
    }
    if r <= i64::MIN as f64 {
        return None;
    }
    let high = real_to_integer(r.floor());
    if strict && r == r.floor() {
        return high.checked_sub(1);
    }
    Some(high)
}

/// Returns the smallest and largest value the `value` constraints allow.
///
/// `None` when no value can satisfy them. `value = X` with a real `X` that is
/// not a whole number allows nothing.
///
/// @param plan - the plan bits
/// @param given - the constraint values for `value`, lower first
fn value_bounds(plan: i32, given: &mut std::slice::Iter<'_, Value<'static>>) -> Option<(i64, i64)> {
    if plan & VALUE_EQ != 0 {
        let value = given.next()?;
        let exact = match as_real(value) {
            Some(r) if r != r.ceil() || r < i64::MIN as f64 || r > i64::MAX as f64 => return None,
            Some(r) => real_to_integer(r),
            None => cast::integer_value(value),
        };
        return Some((exact, exact));
    }
    let low = if plan & (VALUE_GE | VALUE_GT) != 0 {
        lower_bound(given.next()?, plan & VALUE_GT != 0)?
    } else {
        i64::MIN
    };
    let high = if plan & (VALUE_LE | VALUE_LT) != 0 {
        upper_bound(given.next()?, plan & VALUE_LT != 0)?
    } else {
        i64::MAX
    };
    (low <= high).then_some((low, high))
}

impl SeriesCursor {
    /// Positions the cursor on the first value of the series a plan and its
    /// arguments describe. This is SQLite's `seriesFilter`, step for step.
    ///
    /// `None` when the series is empty.
    ///
    /// @param plan - the plan bits `best_index` chose
    /// @param arguments - the constraint values, in the order the bits list them
    fn position(&mut self, plan: i32, arguments: &[Value<'static>]) -> Option<()> {
        // SQLite's series is empty when any value it was given is NULL.
        if arguments.iter().any(|value| matches!(value, Value::Null)) {
            return None;
        }
        let mut given = arguments.iter();
        let mut take = |bit: i32, fallback: i64| -> i64 {
            if plan & bit == 0 {
                return fallback;
            }
            given.next().map(cast::integer_value).unwrap_or(fallback)
        };
        let mut start = take(HAS_START, 0);
        let mut stop = take(HAS_STOP, 0xffff_ffff);
        let mut step = take(HAS_STEP, 1);
        if step == 0 {
            step = 1;
        }
        // `value` constraints with no start, stop or step to anchor them bound
        // the walk by themselves, so it begins as the whole range of integers.
        if plan & (HAS_START | HAS_STEP) == 0 && plan & (VALUE_EQ | VALUE_GE | VALUE_GT) != 0 {
            start = i64::MIN;
        }
        if plan & (HAS_STOP | HAS_STEP) == 0 && plan & (VALUE_EQ | VALUE_LE | VALUE_LT) != 0 {
            stop = i64::MAX;
        }
        self.arguments = (start, stop, step);
        let (mut base, mut term) = (start, stop);
        self.step = step.unsigned_abs();
        self.descending = step < 0;
        if (!self.descending && base > term) || (self.descending && base < term) {
            return None;
        }
        if plan & (VALUE_EQ | VALUE_GE | VALUE_GT | VALUE_LE | VALUE_LT) != 0 {
            let (low, high) = value_bounds(plan, &mut given)?;
            if !self.descending {
                if base < low {
                    base = add(base, (span(low, base) / self.step) * self.step);
                    if base < low {
                        if base > sub(i64::MAX, self.step) {
                            return None;
                        }
                        base = add(base, self.step);
                    }
                }
                term = term.min(high);
            } else {
                if base > high {
                    base = sub(base, (span(base, high) / self.step) * self.step);
                    if base > high {
                        if base < add(i64::MIN, self.step) {
                            return None;
                        }
                        base = sub(base, self.step);
                    }
                }
                term = term.max(low);
            }
        }
        // The walk ends on the last value a whole number of steps reaches.
        if !self.descending {
            if base > term {
                return None;
            }
            term = sub(term, span(term, base) % self.step);
        } else {
            if base < term {
                return None;
            }
            term = add(term, span(base, term) % self.step);
        }
        let reverse = (plan & DESCENDING != 0 && !self.descending)
            || (plan & ASCENDING != 0 && self.descending);
        if reverse {
            std::mem::swap(&mut base, &mut term);
            self.descending = !self.descending;
        }
        self.value = base;
        self.term = term;
        Some(())
    }
}

impl VirtualCursor for SeriesCursor {
    /// Reads the arguments and positions on the first value.
    fn filter(&mut self, _context: &mut Context<'_>, plan: &FilterPlan) -> DbResult<()> {
        self.done = self.position(plan.index_number, &plan.arguments).is_none();
        Ok(())
    }

    /// Moves one step along the series, stopping on its last value.
    fn next(&mut self, _context: &mut Context<'_>) -> DbResult<()> {
        if self.value == self.term {
            self.done = true;
        } else if self.descending {
            self.value = sub(self.value, self.step);
        } else {
            self.value = add(self.value, self.step);
        }
        Ok(())
    }

    /// Returns whether the series is finished.
    fn eof(&self) -> bool {
        self.done
    }

    /// Returns one column: the value, or the argument it was given.
    fn column(&mut self, _context: &mut Context<'_>, index: usize) -> DbResult<Value<'static>> {
        Ok(match index {
            VALUE => Value::Integer(self.value),
            // The hidden columns echo the arguments as they were given, a
            // negative step included.
            START => Value::Integer(self.arguments.0),
            STOP => Value::Integer(self.arguments.1),
            STEP => Value::Integer(self.arguments.2),
            _ => Value::Null,
        })
    }

    /// Returns the row's rowid, which SQLite makes equal to its value.
    fn rowid(&self) -> DbResult<i64> {
        Ok(self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inillucent_base::limits::Limits;

    /// Runs a series and collects its values.
    fn run(plan: i32, arguments: &[i64]) -> Vec<i64> {
        let module = SeriesModule;
        let table = module
            .connect(
                &ModuleArguments {
                    database: 0,
                    schema: b"main".to_vec(),
                    table: b"generate_series".to_vec(),
                    module: b"generate_series".to_vec(),
                    arguments: Vec::new(),
                    shadows: Vec::new(),
                },
                false,
            )
            .expect("connects");
        let mut cursor = table.open().expect("opens");
        let mut pagers = NoPagers;
        let limits = Limits::default();
        let mut context = Context {
            host: &mut pagers,
            database: 0,
            limits: &limits,
            catalog: None,
        };
        cursor
            .filter(
                &mut context,
                &FilterPlan {
                    index_number: plan,
                    index_string: String::new(),
                    arguments: arguments
                        .iter()
                        .map(|value| Value::Integer(*value))
                        .collect(),
                },
            )
            .expect("filters");
        let mut out = Vec::new();
        while !cursor.eof() {
            let value = cursor.column(&mut context, VALUE).expect("reads");
            out.push(value.as_integer().unwrap_or(0));
            cursor.next(&mut context).expect("advances");
            if out.len() > 100 {
                break;
            }
        }
        out
    }

    /// A pager set with no databases, for a module that reads none.
    struct NoPagers;

    impl crate::vtab::Host for NoPagers {}

    /// The ordinary ascending series.
    #[test]
    fn an_ascending_series_counts_up() {
        assert_eq!(run(HAS_START | HAS_STOP, &[1, 5]), vec![1, 2, 3, 4, 5]);
        assert_eq!(
            run(HAS_START | HAS_STOP | HAS_STEP, &[1, 10, 3]),
            vec![1, 4, 7, 10]
        );
    }

    /// A descending scan walks the same values backwards, which is what lets
    /// `ORDER BY value DESC` skip the sorter.
    #[test]
    fn a_descending_series_counts_down() {
        assert_eq!(
            run(HAS_START | HAS_STOP | DESCENDING, &[1, 5]),
            vec![5, 4, 3, 2, 1]
        );
        assert_eq!(
            run(HAS_START | HAS_STOP | HAS_STEP | DESCENDING, &[1, 10, 3]),
            vec![10, 7, 4, 1]
        );
    }

    /// A stop below the start produces nothing rather than looping.
    #[test]
    fn an_empty_series_produces_nothing() {
        assert!(run(HAS_START | HAS_STOP, &[5, 1]).is_empty());
    }

    /// A zero step would never terminate, so it is read as one.
    #[test]
    fn a_zero_step_is_read_as_one() {
        assert_eq!(
            run(HAS_START | HAS_STOP | HAS_STEP, &[1, 3, 0]),
            vec![1, 2, 3]
        );
    }
}

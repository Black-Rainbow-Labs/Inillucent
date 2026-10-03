//! The date and time built-ins.
//!
//! Invariant: everything is computed on the *Julian day number*, as a double,
//! exactly as SQLite does. That is not an implementation detail that could be
//! swapped for a civil-calendar library: the value `julianday()` returns is
//! part of the observable behaviour, `unixepoch()` is derived from it, and the
//! `+N days` modifiers are additions to it. A civil-date implementation would
//! agree on most inputs and disagree on the ones that matter - the proleptic
//! Gregorian calendar before 1582, and the half-day offset that makes a Julian
//! day start at noon.
//!
//! `localtime` and `utc` ask the machine's zone, through
//! [`inillucent_vfs::zone`]. They used to answer NULL and nothing, on the
//! argument that an answer which depends on the machine's zone is one two
//! engines cannot be graded against. task-1979's differential corpus is that
//! grading, and it runs both engines on one machine, where the argument does
//! not hold: `datetime('2020-01-01 12:00:00','utc')` is a fixed conversion
//! both engines can be asked for and compared on. The same objection would
//! apply to `'now'`, which this module has always implemented.
//!
//! Three things here are kept the way SQLite keeps them rather than the way a
//! Julian day alone would give:
//!
//! - **The range.** A date outside Julian day 0 to 5373484.5 - roughly the
//!   year -4713 to the end of 9999 - is NULL rather than a date nobody can
//!   store. [`Moment::inside_the_range`].
//! - **The fields as they were spelled.** `'2020-01-01 24:00:00'` is an hour
//!   24 SQLite accepts and does not carry into the next day, so `date()` of it
//!   is `2020-01-01` and `strftime('%H', ...)` is `24`, while `julianday()` of
//!   it is the next day's midnight. [`Moment::spelled`] holds the fields until
//!   a modifier makes them stale.
//! - **What a `floor` modifier takes off.** Adding a month to 31 January lands
//!   on 31 February, which carries into March; `floor` asks for the last day
//!   of February instead. [`Moment::overflow`] is how many days the carry
//!   moved, recorded when it happens and subtracted when `floor` asks.

use inillucent_sql::function::TimeFunc;
use inillucent_value::{numeric, TextEncoding, Value};

/// The Julian day number of 1970-01-01T00:00:00Z.
const UNIX_EPOCH_JD: f64 = 2440587.5;

/// Seconds in a day, as the conversion between the two scales.
const SECONDS_PER_DAY: f64 = 86_400.0;

/// Milliseconds in a day, the unit SQLite keeps a Julian day in.
const MILLIS_PER_DAY: f64 = 86_400_000.0;

/// Returns a Julian day as the whole number of milliseconds SQLite would hold.
///
/// **SQLite's `DateTime` holds `iJD`, an integer count of milliseconds, and
/// `julianday()` answers `iJD / 86400000.0`.** A day built here by adding
/// fractions of a day as doubles lands on a neighbouring double:
/// `julianday('2026-09-25T17:30:00Z')` answered `2461309.229166667` where
/// SQLite answers `2461309.2291666665`, and a difference of two such days was
/// off by 5e-10, which is enough to change a rounded number of hours. Rounding
/// recovers the integer exactly for any day in the supported range, because a
/// double near 5.4 million days still resolves a hundredth of a millisecond.
///
/// @param day - the Julian day
fn millis_of(day: f64) -> i64 {
    (day * MILLIS_PER_DAY).round() as i64
}

/// Returns the Julian day a whole number of milliseconds names.
///
/// @param millis - milliseconds since Julian day 0
fn day_of_millis(millis: i64) -> f64 {
    millis as f64 / MILLIS_PER_DAY
}

/// Returns a Julian day the way SQLite reports one: on a whole millisecond.
///
/// A value outside the range a date function answers with is left alone, since
/// it is never reported as a Julian day.
///
/// @param day - the Julian day
fn on_the_millisecond(day: f64) -> f64 {
    if !(FIRST_JULIAN_DAY..PAST_LAST_JULIAN_DAY).contains(&day) {
        return day;
    }
    day_of_millis(millis_of(day))
}

/// Returns the Julian day of the wall clock, now.
///
/// It is read once per statement rather than per call, which is what makes two
/// mentions of `'now'` in one statement agree.
pub fn julian_now() -> f64 {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs_f64())
        .unwrap_or(0.0);
    UNIX_EPOCH_JD + since_epoch / SECONDS_PER_DAY
}

/// The first Julian day a date function will answer with.
const FIRST_JULIAN_DAY: f64 = 0.0;

/// The first Julian day past the last one a date function will answer with.
///
/// SQLite's bound, in its own units, is `iJD <= 464269060799999` milliseconds,
/// which is the last millisecond of 9999-12-31.
const PAST_LAST_JULIAN_DAY: f64 = 5_373_484.5;

/// How a year is written out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum YearStyle {
    /// `date()`, `time()` and `datetime()`: a minus sign if the year is
    /// negative, then four digits. SQLite writes the year of
    /// `date('0000-01-01','-1 day')` as `-0001`.
    Signed,
    /// `strftime()`: C's `%04d`, which counts the sign against the width, so
    /// the same year is `-001`. SQLite's two formatters really do differ here
    /// and both were checked against 3.53.4.
    Printf,
}

/// A broken-down date and time, as the formatter reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Civil {
    /// The proleptic Gregorian year.
    pub year: i64,
    /// The month, 1 to 12.
    pub month: i64,
    /// The day of the month, 1 to 31.
    pub day: i64,
    /// The hour, 0 to 23.
    pub hour: i64,
    /// The minute, 0 to 59.
    pub minute: i64,
    /// The second, with its fraction.
    pub second: f64,
}

/// Which zone conversion a modifier has already applied.
///
/// **Repeating the same one is a no-op, and the other one is not (task-1979,
/// F13's neighbour).** `datetime(x,'utc','utc')` is `datetime(x,'utc')` in
/// SQLite, and `datetime(x,'utc','+1 day','utc')` is that shifted by a day - an
/// ordinary modifier in between does not clear it. `datetime(x,'utc',
/// 'localtime')` does convert twice and lands back where it started. All eight
/// orderings were read off 3.53.4 rather than worked out from the rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Zone {
    /// Neither modifier has run.
    Unset,
    /// The last one was `utc`.
    Utc,
    /// The last one was `localtime`.
    Local,
}

/// A time value part-way through being resolved.
///
/// SQLite's `DateTime` keeps a Julian day and the broken-down fields side by
/// side and marks which of them is current; this is the same idea with the
/// three things that behaviour turns on.
#[derive(Clone, Copy, Debug)]
struct Moment {
    /// The Julian day, which is always current.
    day: f64,
    /// The fields exactly as the input spelled them, while no modifier has
    /// made them stale.
    ///
    /// **This is what keeps hour 24 (task-1979, F12).** SQLite accepts
    /// `'2020-01-01 24:00:00'` and leaves the fields alone, so `date()` of it
    /// answers `2020-01-01` and `strftime('%H', ...)` answers `24`, while
    /// `julianday()` of it answers the next day's midnight. Rendering from the
    /// Julian day instead answered `2020-01-02` and hour `00`. One modifier -
    /// any modifier - recomputes the fields and the carry happens, which is
    /// also SQLite's rule: `datetime('2020-01-01 24:00:00','+0 day')` is
    /// `2020-01-02 00:00:00` in both engines.
    spelled: Option<Civil>,
    /// How many days a `floor` modifier would take off.
    ///
    /// **Set by the month and year modifiers, read by `floor` (task-1979,
    /// F14).** `date('2020-01-31','+1 month')` lands on 31 February, which is
    /// two days past the end of that February, so the date carries to 2 March
    /// and this holds 2; `floor` subtracts it and the answer is 29 February.
    /// `ceiling` is the default and only clears it.
    overflow: i64,
    /// Which of the two zone conversions ran last - see [`Zone`].
    zone: Zone,
    /// The number the call started with, while it is still unexplained.
    ///
    /// **A bare number is a Julian day or unix seconds, and only a modifier says
    /// which.** SQLite keeps the number (`rawS`) until `julianday`, `unixepoch` or
    /// `auto` is applied, and those three are valid only while it is there. Without it
    /// `date('2024-05-15','unixepoch')` took the Julian day of a date as seconds and
    /// answered 1970; SQLite answers NULL.
    raw: Option<f64>,
}

impl Moment {
    /// Returns a moment with no spelled fields and no carry, which is what the clock
    /// gives.
    ///
    /// @param day - the Julian day
    fn at(day: f64) -> Moment {
        Moment {
            day,
            spelled: None,
            overflow: 0,
            zone: Zone::Unset,
            raw: None,
        }
    }

    /// Returns a moment for a bare number, which a modifier may still reinterpret.
    ///
    /// @param number - the number the call was given
    fn from_number(number: f64) -> Moment {
        Moment {
            raw: Some(number),
            ..Moment::at(number)
        }
    }

    /// Returns the same instant with the spelled fields discarded.
    ///
    /// @param day - the Julian day the modifier produced
    fn moved_to(self, day: f64) -> Moment {
        Moment {
            day,
            spelled: None,
            ..self
        }
    }

    /// Returns the broken-down fields a formatter should read.
    fn civil(self) -> Civil {
        self.spelled.unwrap_or_else(|| civil_of(self.day))
    }

    /// Returns the same instant with the calendar date recomputed from the
    /// Julian day and the clock left as it was spelled.
    ///
    /// **A day of the month the month does not have is normalised, and the hour
    /// is not.** `date('2020-02-30')` is `2020-03-01` in SQLite and
    /// `datetime('2020-02-30 24:00:00')` is `2020-03-02 24:00:00`: the date
    /// carried and the hour stayed at 24. Its `isDate` says so in one line -
    /// `if( argc==1 && p->validYMD && p->D>28 ) p->validYMD = 0;` - and the
    /// fields it clears are the calendar ones only. With a modifier written
    /// after it the rule does not apply at all, which is why
    /// `datetime('2020-02-30','subsec')` is `2020-02-30 00:00:00.000`.
    fn with_recomputed_date(self) -> Moment {
        let Some(spelled) = self.spelled else {
            return self;
        };
        if spelled.day <= 28 {
            return self;
        }
        let recomputed = civil_of(self.day);
        Moment {
            spelled: Some(Civil {
                year: recomputed.year,
                month: recomputed.month,
                day: recomputed.day,
                ..spelled
            }),
            ..self
        }
    }

    /// Reports whether the date is one SQLite will answer with.
    ///
    /// Outside it every date function answers NULL rather than a year nobody
    /// can store: `date('9999-12-31','+1 day')` is NULL, not `10000-01-01`
    /// (task-1979, F11).
    fn inside_the_range(self) -> bool {
        (FIRST_JULIAN_DAY..PAST_LAST_JULIAN_DAY).contains(&self.day)
    }
}

/// Calls a date or time function.
///
/// `now` is the wall clock the caller supplies rather than one this module
/// reads, so a statement that names `'now'` twice sees one time and a test can
/// pin it.
pub fn call(
    func: TimeFunc,
    arguments: &[Value<'static>],
    now: f64,
    encoding: TextEncoding,
) -> Value<'static> {
    if func == TimeFunc::TimeDiff {
        return timediff(arguments, now, encoding);
    }
    let (format, rest) = match func {
        TimeFunc::StrfTime => {
            let Some(first) = arguments.first() else {
                return Value::Null;
            };
            let Some(text) = modifier_bytes(first) else {
                return Value::Null;
            };
            (Some(text), arguments.get(1..))
        }
        _ => (None, arguments.get(..)),
    };
    let rest = rest.unwrap_or(&[]);
    let Some(moment) = resolve(rest, now, encoding) else {
        return Value::Null;
    };
    let day = moment.day;
    let civil = moment.civil();
    // **`subsec` is a modifier the *renderer* has to know about.** It asks for
    // the fractional second, so `datetime(x, 'subsec')` formats seconds as
    // `%f` rather than `%S`. It reached `unixepoch` and nothing else, so
    // `datetime('2024-03-01 12:00:00', 'subsec')` answered `12:00:00` where
    // SQLite answers `12:00:00.000` - a modifier accepted and dropped, which is
    // the one outcome a caller cannot detect.
    let subsec = asks_for_subsec(rest);
    match func {
        TimeFunc::JulianDay => Value::Real(on_the_millisecond(day)),
        TimeFunc::UnixEpoch => unix_epoch(rest, day, encoding),
        TimeFunc::Date => render(day, civil, b"%Y-%m-%d", YearStyle::Signed),
        TimeFunc::Time => render(
            day,
            civil,
            if subsec { b"%H:%M:%f" } else { b"%H:%M:%S" },
            YearStyle::Signed,
        ),
        TimeFunc::DateTime => render(
            day,
            civil,
            if subsec {
                b"%Y-%m-%d %H:%M:%f"
            } else {
                b"%Y-%m-%d %H:%M:%S"
            },
            YearStyle::Signed,
        ),
        TimeFunc::StrfTime => {
            let format = format.unwrap_or_default();
            let format = if subsec {
                subsec_epoch_conversion(&format, day)
            } else {
                format
            };
            render(day, civil, &format, YearStyle::Printf)
        }
        TimeFunc::TimeDiff => Value::Null,
    }
}

/// Replaces each `%s` in a strftime format with the epoch seconds written with three decimals.
///
/// SQLite prints `%s` as `86400.000` when the `subsec` modifier is given. A `%%` is
/// copied through so `%%s` stays a percent sign followed by an `s`.
///
/// @param format - the strftime format
/// @param day - the Julian day being formatted
fn subsec_epoch_conversion(format: &[u8], day: f64) -> Vec<u8> {
    let text = format!(
        "{:.3}",
        milliseconds_of((day - UNIX_EPOCH_JD) * SECONDS_PER_DAY) / 1000.0
    );
    let mut out = Vec::with_capacity(format.len());
    let mut index = 0;
    while let Some(&byte) = format.get(index) {
        let next = format.get(index + 1).copied();
        if byte == b'%' && next == Some(b's') {
            out.extend_from_slice(text.as_bytes());
            index += 2;
        } else if byte == b'%' && next == Some(b'%') {
            out.extend_from_slice(b"%%");
            index += 2;
        } else {
            out.push(byte);
            index += 1;
        }
    }
    out
}

/// Returns the bytes of a text or blob argument, which SQLite reads the same way.
///
/// @param value - a format, time value or modifier argument
fn modifier_bytes(value: &Value<'static>) -> Option<Vec<u8>> {
    match value {
        Value::Text(text) => Some(text.utf8_bytes().to_vec()),
        Value::Blob(blob) => Some(blob.raw().to_vec()),
        _ => None,
    }
}

/// Reports whether an argument is the `subsec` or `subsecond` keyword.
///
/// @param value - a time value or modifier argument
fn is_subsec_keyword(value: &Value<'static>) -> bool {
    modifier_bytes(value).is_some_and(|bytes| {
        let folded = bytes.to_ascii_lowercase();
        folded == b"subsec" || folded == b"subsecond"
    })
}

/// Reports whether an argument list carries the `subsec` modifier.
///
/// The pinned release also reads the keyword as the time value itself, so
/// `datetime('subsec')` is the current time with its fraction.
///
/// @param arguments - the time value and its modifiers
fn asks_for_subsec(arguments: &[Value<'static>]) -> bool {
    arguments.iter().any(is_subsec_keyword)
}

/// Returns the moment one argument list resolves to.
///
/// The first argument is the time value and every later one is a modifier,
/// applied in order. With no arguments at all the value is `'now'`, which is
/// why `date()` and `date('now')` are the same call.
///
/// `None` when any step has no answer, and also when the date the steps landed
/// on is outside the range SQLite answers with - see
/// [`Moment::inside_the_range`].
///
/// @param arguments - the time value and its modifiers
/// @param now - the wall clock the caller supplies
/// @param encoding - the text encoding a numeric string is read in
fn resolve(arguments: &[Value<'static>], now: f64, encoding: TextEncoding) -> Option<Moment> {
    let mut moment = match arguments.first() {
        Some(value) if is_subsec_keyword(value) => Moment::at(now),
        Some(value) => parse_time_value(value, now, encoding)?,
        None => Moment::at(now),
    };
    if arguments.len() <= 1 {
        moment = moment.with_recomputed_date();
    }
    for (index, modifier) in arguments.get(1..).unwrap_or(&[]).iter().enumerate() {
        let text = modifier_bytes(modifier)?;
        moment = apply_modifier(moment, &text, index.saturating_add(1))?;
    }
    moment.inside_the_range().then_some(moment)
}

/// Returns `unixepoch()`, honouring the `subsec` modifier.
fn unix_epoch(arguments: &[Value<'static>], day: f64, encoding: TextEncoding) -> Value<'static> {
    let _ = encoding;
    let seconds = (day - UNIX_EPOCH_JD) * SECONDS_PER_DAY;
    if asks_for_subsec(arguments) {
        // **Rounded to the millisecond first, for the same reason
        // `whole_seconds` rounds.** A Julian day is a binary fraction, so
        // `unixepoch('2024-03-01 09:05:07','subsec')` came out as
        // 1709283906.9999843 where SQLite answers 1709283907.0. SQLite holds
        // the instant in whole milliseconds and divides, and a millisecond is
        // as fine as `subsec` ever reports, so nothing a caller can ask for is
        // lost by matching it.
        return Value::Real(milliseconds_of(seconds) / 1000.0);
    }
    Value::Integer(whole_seconds(seconds))
}

/// Returns the whole seconds a Julian-day difference stands for.
///
/// **Flooring the double directly was one second low.** A Julian day is a
/// binary fraction, so `2024-03-01 09:05:07` comes back as
/// `1709283906.9999998` rather than as `1709283907`, and `floor` on that is
/// 1,709,283,906 - a wrong answer on `strftime('%s', ...)` and on
/// `unixepoch()` alike, for every timestamp whose representation happens to
/// fall short. SQLite works in whole milliseconds throughout, so the value is
/// rounded to a millisecond first and only then reduced to seconds; a
/// half-second is still floored, which is what makes the truncation SQLite's
/// rather than a rounding of its own.
///
/// @param seconds - the difference from the epoch, in seconds
fn whole_seconds(seconds: f64) -> i64 {
    (milliseconds_of(seconds) as i64).div_euclid(1000)
}

/// Returns the whole milliseconds a difference in seconds stands for.
///
/// The unit SQLite holds a time in, and the one both `unixepoch` answers are
/// derived from - see `whole_seconds` for what flooring the double instead
/// cost.
///
/// @param seconds - the difference from the epoch, in seconds
fn milliseconds_of(seconds: f64) -> f64 {
    (seconds * 1000.0).round()
}

/// Counts the whole years and months between two moments, and what is left over in days.
///
/// This follows SQLite's `timediff`, which is not symmetrical. When the first argument is the
/// later one, the earlier moment is moved forward by the years and months and the left over days
/// are counted from there. When the first argument is the earlier one, the later moment is moved
/// back by the years and months instead, so `timediff('2000-01-31', '2000-03-02')` is one month
/// and two days (March 2 back to February 2, then two days to January 31) where the reversed call
/// is one month and nought days (January 31 forward to the overflowed February 31, which is March 2).
///
/// @param low - the earlier Julian day
/// @param high - the later Julian day
/// @param forward - true when the first argument was the later moment
fn calendar_walk(low: f64, high: f64, forward: bool) -> (i64, i64, f64) {
    let (low_civil, high_civil) = (civil_of(low), civil_of(high));
    let mut years = high_civil.year - low_civil.year;
    let mut months = high_civil.month - low_civil.month;
    if months < 0 {
        years -= 1;
        months += 12;
    }
    // The moved moment keeps its own day and clock and takes the other's year and month.
    let mut moved = if forward { low_civil } else { high_civil };
    let target = if forward { high_civil } else { low_civil };
    if high_civil.year != low_civil.year {
        moved.year = target.year;
    }
    if months != 0 {
        moved.month = target.month;
    }
    let mut anchor = julian_of(moved);
    let mut guard = 0usize;
    // Adding a month to 31 January overshoots by carrying into March, so the walk steps back a
    // month at a time, and more than once when February is short.
    while guard < 24 && (if forward { anchor > high } else { low > anchor }) {
        guard = guard.saturating_add(1);
        months -= 1;
        if months < 0 {
            months = 11;
            years -= 1;
        }
        moved.month += if forward { -1 } else { 1 };
        if moved.month < 1 {
            moved.month = 12;
            moved.year -= 1;
        } else if moved.month > 12 {
            moved.month = 1;
            moved.year += 1;
        }
        anchor = julian_of(moved);
    }
    (
        years,
        months,
        if forward { high - anchor } else { anchor - low },
    )
}

/// Returns `timediff(a, b)` as SQLite's `+YYYY-MM-DD HH:MM:SS.SSS` string.
fn timediff(arguments: &[Value<'static>], now: f64, encoding: TextEncoding) -> Value<'static> {
    let (Some(left), Some(right)) = (arguments.first(), arguments.get(1)) else {
        return Value::Null;
    };
    let (Some(left), Some(right)) = (
        parse_time_value(left, now, encoding),
        parse_time_value(right, now, encoding),
    ) else {
        return Value::Null;
    };
    if !left.inside_the_range() || !right.inside_the_range() {
        return Value::Null;
    }
    let (left, right) = (left.day, right.day);
    let sign = if left >= right { '+' } else { '-' };
    let (years, months, remainder) = calendar_walk(left.min(right), left.max(right), sign == '+');
    // Whole milliseconds, which is what SQLite holds a time in. Splitting the fraction of a day as
    // a float turned 2.5 hours into 02:29:60.000.
    let millis = (remainder * MILLIS_PER_DAY).round() as i64;
    let days = millis.div_euclid(86_400_000);
    let of_day = millis.rem_euclid(86_400_000);
    let text = format!(
        "{sign}{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03}",
        years,
        months,
        days,
        of_day / 3_600_000,
        of_day / 60_000 % 60,
        of_day / 1000 % 60,
        of_day % 1000
    );
    // A fallible allocation is the only way this can fail, and a NULL is
    // the answer SQLite gives when it cannot build the string either.
    Value::owned_text(text.as_bytes()).unwrap_or(Value::Null)
}

/// Returns the moment a time value names.
///
/// The forms are SQLite's: a number is a Julian day unless a `unixepoch`
/// modifier says otherwise, and text is one of the ISO-8601 shapes or the
/// literal `now`.
///
/// Only the ISO-8601 shapes carry [`Moment::spelled`] fields, and only when the
/// text named no zone offset. A number has no fields to keep, and an offset
/// means the fields as written are not the instant that was stored.
///
/// @param value - the first argument of the call
/// @param now - the wall clock the caller supplies
/// @param encoding - the text encoding a numeric string is read in
fn parse_time_value(value: &Value<'static>, now: f64, encoding: TextEncoding) -> Option<Moment> {
    match value {
        Value::Null => None,
        Value::Integer(integer) => Some(Moment::from_number(*integer as f64)),
        Value::Real(real) => Some(Moment::from_number(*real)),
        // SQLite reads a blob as the text of its bytes, so a blob holding a date is one.
        Value::Blob(blob) => parse_time_text(blob.raw(), now, encoding),
        Value::Text(text) => parse_time_text(&text.utf8_bytes(), now, encoding),
    }
}

/// Returns the moment a piece of text names, which may be `now`, a date or a number.
///
/// @param raw - the text
/// @param now - the wall clock the caller supplies
/// @param encoding - the text encoding a numeric string is read in
fn parse_time_text(raw: &[u8], now: f64, encoding: TextEncoding) -> Option<Moment> {
    let trimmed = trim(raw);
    if trimmed.eq_ignore_ascii_case(b"now") {
        return Some(Moment::at(now));
    }
    // **Trailing whitespace only** (task-2066 section 4.2, item 27).
    // `date(' 2024-01-01')` answered `2024-01-01` here and is NULL in
    // SQLite, whose `parseYyyyMmDd` reads the string it was given and
    // skips nothing before the year. It *does* skip whitespace after
    // the date, which is why `date('2024-01-01 ')` is a date in both -
    // and trimming both ends made the first case wrong while a raw
    // parse would have made the second wrong. The reference's own
    // answers to the pair are `differential_part8`'s `t2066-003`.
    if let Some((day, spelled)) = parse_iso(trim_end(raw)) {
        return Some(Moment {
            day,
            spelled,
            overflow: 0,
            zone: if names_a_zone(trim_end(raw)) {
                Zone::Utc
            } else {
                Zone::Unset
            },
            raw: None,
        });
    }
    // A numeric string is a Julian day, which is what makes
    // `date('2451545.0')` work.
    if numeric::looks_numeric(trimmed, encoding) {
        let parsed = numeric::atof(trimmed, encoding);
        return Some(Moment::from_number(parsed.value));
    }
    None
}

/// Reports whether date text ends in a zone: `Z` or a signed `HH:MM`.
///
/// SQLite then treats the moment as already UTC, so a later `utc` modifier changes nothing.
///
/// @param bytes - the text, with trailing whitespace removed
fn names_a_zone(bytes: &[u8]) -> bool {
    if matches!(bytes.last(), Some(b'Z' | b'z')) {
        return true;
    }
    let Some(start) = bytes.len().checked_sub(6) else {
        return false;
    };
    let tail = bytes.get(start..).unwrap_or(&[]);
    matches!(tail.first(), Some(b'+' | b'-'))
        && tail.get(3) == Some(&b':')
        && tail.get(1).is_some_and(u8::is_ascii_digit)
        && start > 0
        && bytes
            .get(start.saturating_sub(1))
            .is_some_and(u8::is_ascii_digit)
        && bytes.iter().filter(|byte| **byte == b':').count() >= 2
}

/// Returns the bytes with any trailing whitespace removed.
///
/// @param bytes - the text as it was written
fn trim_end(bytes: &[u8]) -> &[u8] {
    let mut end = bytes.len();
    while end > 0
        && end
            .checked_sub(1)
            .and_then(|index| bytes.get(index))
            .is_some_and(|byte| numeric::is_space(*byte))
    {
        end = end.saturating_sub(1);
    }
    bytes.get(..end).unwrap_or(bytes)
}

/// Returns the bytes with leading and trailing spaces removed.
fn trim(bytes: &[u8]) -> &[u8] {
    let mut start = 0usize;
    let mut end = bytes.len();
    while start < end
        && bytes
            .get(start)
            .is_some_and(|byte| numeric::is_space(*byte))
    {
        start = start.saturating_add(1);
    }
    while end > start
        && end
            .checked_sub(1)
            .and_then(|index| bytes.get(index))
            .is_some_and(|byte| numeric::is_space(*byte))
    {
        end = end.saturating_sub(1);
    }
    bytes.get(start..end).unwrap_or(&[])
}

/// Parses one of the ISO-8601 shapes SQLite accepts.
///
/// This is `parseYyyyMmDd` and `parseHhMmSs` read the way SQLite reads them, field by
/// field: an optional `-` for a negative year, `YYYY-MM-DD`, any number of spaces or `T`
/// between the date and the time, then `HH:MM[:SS[.SSS]]` and an optional zone. A time
/// alone is accepted too, and takes the date 2000-01-01.
///
/// The fields come back beside the Julian day when the text named no offset,
/// so that an hour 24 survives to the formatter - see [`Moment::spelled`].
///
/// @param bytes - the text, with trailing whitespace already removed
fn parse_iso(bytes: &[u8]) -> Option<(f64, Option<Civil>)> {
    let (civil, offset_minutes) = match parse_date_and_time(bytes) {
        Some(parsed) => parsed,
        None => {
            let (hour, minute, second, offset) = parse_hms(bytes)?;
            let civil = Civil {
                year: 2000,
                month: 1,
                day: 1,
                hour,
                minute,
                second,
            };
            (civil, offset)
        }
    };
    let millis = julian_millis_of(civil);
    if offset_minutes != 0 {
        // Taken off in whole milliseconds, as SQLite's `computeJD` does.
        let shifted = millis.checked_sub(offset_minutes.checked_mul(60_000)?)?;
        return Some((day_of_millis(shifted), None));
    }
    Some((day_of_millis(millis), Some(civil)))
}

/// Reads a date, optionally followed by a time, returning the fields and the zone offset.
///
/// `None` unless the whole text is one. The year may be written `-YYYY`, which is how
/// a year before 0000 is spelled and is what makes `date('-0001-01-01')` a date.
///
/// @param bytes - the text
fn parse_date_and_time(bytes: &[u8]) -> Option<(Civil, i64)> {
    let (negative, body) = match bytes.first() {
        Some(b'-') => (true, bytes.get(1..)?),
        _ => (false, bytes),
    };
    // Four digits, two and two with their separators. SQLite's format is
    // `"40f-21a-21d"`: months run 1 to 12 and days 1 to 31, and the day is not checked
    // against the month because a day the month lacks carries into the next one.
    let year = digit_field(body, 0, 4, 0, 14_712)?;
    (body.get(4) == Some(&b'-')).then_some(())?;
    let month = digit_field(body, 5, 2, 1, 12)?;
    (body.get(7) == Some(&b'-')).then_some(())?;
    let day = digit_field(body, 8, 2, 1, 31)?;
    let mut rest = body.get(10..)?;
    while let Some((first, tail)) = rest.split_first() {
        if !(numeric::is_space(*first) || *first == b'T') {
            break;
        }
        rest = tail;
    }
    let (hour, minute, second, offset) = match rest.is_empty() {
        true => (0, 0, 0.0, 0),
        false => parse_hms(rest)?,
    };
    let civil = Civil {
        year: if negative { -year } else { year },
        month,
        day,
        hour,
        minute,
        second,
    };
    Some((civil, offset))
}

/// Parses `HH:MM[:SS[.SSS]]` and an optional zone, which must end the text.
///
/// Returns the hour, minute, second with its fraction, and the zone offset in minutes.
/// The fraction is cut to 0.999, because SQLite does, to keep a rounding to the
/// millisecond from carrying into the next second.
///
/// @param bytes - the text
fn parse_hms(bytes: &[u8]) -> Option<(i64, i64, f64, i64)> {
    let hour = digit_field(bytes, 0, 2, 0, 24)?;
    (bytes.get(2) == Some(&b':')).then_some(())?;
    let minute = digit_field(bytes, 3, 2, 0, 59)?;
    let mut at = 5usize;
    let mut second = 0.0;
    if bytes.get(at) == Some(&b':') {
        let whole = digit_field(bytes, at.saturating_add(1), 2, 0, 59)?;
        at = at.saturating_add(3);
        second = whole as f64;
        if bytes.get(at) == Some(&b'.')
            && bytes
                .get(at.saturating_add(1))
                .is_some_and(u8::is_ascii_digit)
        {
            at = at.saturating_add(1);
            let mut fraction = 0.0f64;
            let mut scale = 1.0f64;
            while let Some(digit) = bytes.get(at).filter(|byte| byte.is_ascii_digit()) {
                fraction = fraction * 10.0 + f64::from(digit.saturating_sub(b'0'));
                scale *= 10.0;
                at = at.saturating_add(1);
            }
            second += (fraction / scale).min(0.999);
        }
    }
    let offset = parse_zone(bytes.get(at..)?)?;
    Some((hour, minute, second, offset))
}

/// Parses what may follow a time: spaces, then nothing, `Z`, or `+HH:MM` / `-HH:MM`.
///
/// Returns the offset in minutes. A zone hour runs 0 to 14 and the minute 0 to 59, and
/// only spaces may follow it. Anything else makes the whole text not a date.
///
/// @param bytes - the text after the time
fn parse_zone(bytes: &[u8]) -> Option<i64> {
    let trimmed = skip_spaces(bytes);
    let Some((sign, rest)) = trimmed.split_first() else {
        return Some(0);
    };
    let sign = match sign {
        b'-' => -1,
        b'+' => 1,
        b'Z' | b'z' => return skip_spaces(rest).is_empty().then_some(0),
        _ => return None,
    };
    let hours = digit_field(rest, 0, 2, 0, 14)?;
    (rest.get(2) == Some(&b':')).then_some(())?;
    let minutes = digit_field(rest, 3, 2, 0, 59)?;
    skip_spaces(rest.get(5..)?)
        .is_empty()
        .then_some(sign * (hours * 60 + minutes))
}

/// Returns the bytes with any leading whitespace removed.
///
/// @param bytes - the text
fn skip_spaces(bytes: &[u8]) -> &[u8] {
    let mut rest = bytes;
    while let Some((first, tail)) = rest.split_first() {
        if !numeric::is_space(*first) {
            break;
        }
        rest = tail;
    }
    rest
}

/// Reads `width` ASCII digits at `at` and checks the number against `min..=max`.
///
/// SQLite's `getDigits` takes exactly the width it is told, so `2024-1-1` is not a
/// date and neither is `02024-01-01`.
///
/// @param bytes - the text
/// @param at - where the field starts
/// @param width - how many digits it has
/// @param min - the smallest value it may have
/// @param max - the largest value it may have
fn digit_field(bytes: &[u8], at: usize, width: usize, min: i64, max: i64) -> Option<i64> {
    let field = bytes.get(at..at.checked_add(width)?)?;
    let value = digits(field)?;
    (min..=max).contains(&value).then_some(value)
}

/// Parses a run of ASCII digits.
fn digits(bytes: &[u8]) -> Option<i64> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value = 0i64;
    for byte in bytes {
        value = value
            .checked_mul(10)?
            .checked_add(i64::from(byte.saturating_sub(b'0')))?;
    }
    Some(value)
}

/// The Julian day of 1970-01-01 00:00 UTC, which is where Unix time starts.
pub const UNIX_EPOCH_JULIAN_DAY: f64 = 2_440_587.5;

/// Returns the civil date a Unix day number names.
///
/// **The one implementation of this in the workspace (task-1961, A10).** There
/// were three more - `inillucent-cli`'s `archive.rs`, `inillucent-ext`'s
/// `zipfile.rs` and the `perfhistory` harness - each a private copy of Howard
/// Hinnant's `civil_from_days` answering a bare `(i64, i64, i64)` that a caller
/// had to read in the right order. The date functions in this module had
/// already been doing the same arithmetic, with a named result and a round trip
/// test against [`julian_of`], since before any of them was written.
///
/// @param days - days since 1970-01-01, which may be negative
pub fn civil_of_unix_day(days: i64) -> Civil {
    civil_of(days as f64 + UNIX_EPOCH_JULIAN_DAY)
}

/// Returns the Julian day of a civil date and time.
///
/// The formula is the standard one for the proleptic Gregorian calendar, and
/// the half-day is the reason a Julian day starts at noon. The value is on a
/// whole millisecond - see [`julian_millis_of`].
pub fn julian_of(civil: Civil) -> f64 {
    day_of_millis(julian_millis_of(civil))
}

/// Returns the Julian day of a civil date and time, in whole milliseconds.
///
/// **This is SQLite's `computeJD`, step for step.** The date part is a whole
/// day plus a half, which a double holds exactly; the clock is then added as
/// integers, and the seconds are rounded to the millisecond on their own. The
/// division into days happens once, when a caller asks for a Julian day.
///
/// @param civil - the date and time
fn julian_millis_of(civil: Civil) -> i64 {
    let (mut year, mut month) = (civil.year, civil.month);
    if month <= 2 {
        year -= 1;
        month += 12;
    }
    let a = year.div_euclid(100);
    let b = 2 - a + a.div_euclid(4);
    let days = (365.25 * ((year + 4716) as f64)).floor()
        + (30.6001 * ((month + 1) as f64)).floor()
        + civil.day as f64
        + b as f64
        - 1524.5;
    let clock = civil.hour * 3_600_000 + civil.minute * 60_000;
    // Saturating, because a date far outside the supported years (a Julian day read
    // from a number such as 2.5e11) converts to more milliseconds than an `i64`
    // holds, and the caller rejects it afterwards by its range.
    ((days * MILLIS_PER_DAY) as i64)
        .saturating_add(clock)
        .saturating_add((civil.second * 1000.0 + 0.5) as i64)
}

/// Returns the civil date and time of a Julian day.
///
/// @param day - the Julian day, where 2440587.5 is 1970-01-01 00:00 UTC
pub fn civil_of(day: f64) -> Civil {
    let shifted = day + 0.5;
    let z = shifted.floor();
    let fraction = shifted - z;
    let z = z as i64;
    // The correction is applied at every date, not only after 1582. SQLite's
    // calendar is the *proleptic* Gregorian one, so a date in 1200 is the
    // Gregorian date rather than the Julian one people used at the time - and
    // switching calendars here while `julian_of` did not made the two
    // directions disagree by the seven days between them.
    let alpha = ((z as f64 - 1867216.25) / 36524.25).floor() as i64;
    let a = z + 1 + alpha - alpha.div_euclid(4);
    let b = a + 1524;
    let c = ((b as f64 - 122.1) / 365.25).floor() as i64;
    let d = (365.25 * c as f64).floor() as i64;
    let e = ((b - d) as f64 / 30.6001).floor() as i64;
    let day_of_month = b - d - (30.6001 * e as f64).floor() as i64;
    let month = if e < 14 { e - 1 } else { e - 13 };
    let year = if month > 2 { c - 4716 } else { c - 4715 };
    // The seconds are rounded to the millisecond before they are broken up,
    // because a value that is 59.9999999 seconds past the minute renders as
    // ":60" otherwise - a time that does not exist.
    let mut seconds = (fraction * SECONDS_PER_DAY * 1000.0).round() / 1000.0;
    let mut hour = (seconds / 3600.0).floor() as i64;
    seconds -= (hour as f64) * 3600.0;
    let mut minute = (seconds / 60.0).floor() as i64;
    seconds -= (minute as f64) * 60.0;
    if minute >= 60 {
        minute -= 60;
        hour += 1;
    }
    Civil {
        year,
        month,
        day: day_of_month,
        hour,
        minute,
        second: seconds,
    }
}

/// Applies one modifier to a moment.
///
/// Every modifier but `subsec` makes the spelled fields stale, which is what
/// carries an hour 24 into the next day the moment anything else is asked for -
/// SQLite's rule as well as this one.
///
/// @param moment - the moment so far
/// @param modifier - the modifier, as written
/// @param position - which modifier this is, counting from 1; `julianday`,
///   `unixepoch` and `auto` are accepted only in the first place
fn apply_modifier(moment: Moment, modifier: &[u8], position: usize) -> Option<Moment> {
    // **Not trimmed.** SQLite compares the modifier exactly, ignoring case, so
    // `'+1 day '` and `' +1 day'` have no date.
    let folded = modifier.to_ascii_lowercase();
    match folded.as_slice() {
        // Read by `unixepoch` and by the renderer rather than applied, so the
        // fields it was given are still the fields it has.
        b"subsec" | b"subsecond" => Some(moment),
        b"julianday" => as_julian_day(moment, position),
        b"unixepoch" => as_unix_seconds(moment, position),
        b"auto" => guess_number_scale(moment, position),
        b"localtime" => to_local_time(moment),
        b"utc" => to_universal_time(moment),
        // The default, so it only forgets what a `floor` would have taken off.
        b"ceiling" => Some(Moment {
            spelled: None,
            overflow: 0,
            ..moment
        }),
        b"floor" => Some(Moment {
            day: moment.day - moment.overflow as f64,
            spelled: None,
            overflow: 0,
            ..moment
        }),
        _ => apply_calendar_modifier(moment, &folded),
    }
}

/// Reports whether a number is one SQLite reads as a Julian day without being told.
///
/// @param number - the number a date function was given
fn is_julian_day_number(number: f64) -> bool {
    (FIRST_JULIAN_DAY..PAST_LAST_JULIAN_DAY).contains(&number)
}

/// Applies `julianday`, which says the number the call started with is a Julian day.
///
/// **It is accepted only first, and only after a bare number that already was one.**
/// SQLite answers NULL for `date('2024-05-15','julianday')` and for
/// `date(-5,'julianday')`; the modifier exists to undo `unixepoch` and `auto`, and
/// elsewhere it is a mistake, not a no-op.
///
/// @param moment - the moment so far
/// @param position - which modifier this is, counting from 1
fn as_julian_day(moment: Moment, position: usize) -> Option<Moment> {
    let raw = moment.raw?;
    (position == 1 && is_julian_day_number(raw)).then_some(Moment {
        raw: None,
        ..moment
    })
}

/// Applies `unixepoch`, which says the number the call started with is seconds since 1970.
///
/// The conversion is done in whole milliseconds, as SQLite does it: the number of
/// seconds is scaled to milliseconds since Julian day 0 and rounded once. It is
/// accepted only first, only after a bare number, and only for the 464269060800000
/// milliseconds a date function can name.
///
/// @param moment - the moment so far
/// @param position - which modifier this is, counting from 1
fn as_unix_seconds(moment: Moment, position: usize) -> Option<Moment> {
    let raw = moment.raw?;
    if position != 1 {
        return None;
    }
    let scaled = raw * 1000.0 + 210_866_760_000_000.0;
    if !(0.0..464_269_060_800_000.0).contains(&scaled) {
        return None;
    }
    Some(Moment {
        day: day_of_millis((scaled + 0.5) as i64),
        spelled: None,
        raw: None,
        ..moment
    })
}

/// Applies `auto`, which reads a bare number as a Julian day or as unix seconds by size.
///
/// A number inside the Julian day range stays a Julian day, including every positive
/// number up to 5373484.5. Outside it a number between -210866760000 and
/// 253402300799 is unix seconds, which is how a negative number gets a date, and
/// anything further out has none. A text date has nothing to guess and passes.
///
/// @param moment - the moment so far
/// @param position - which modifier this is, counting from 1
fn guess_number_scale(moment: Moment, position: usize) -> Option<Moment> {
    if position != 1 {
        return None;
    }
    let Some(raw) = moment.raw else {
        return Some(moment);
    };
    if is_julian_day_number(raw) {
        return Some(Moment {
            raw: None,
            ..moment
        });
    }
    (-210_866_760_000.0..=253_402_300_799.0)
        .contains(&raw)
        .then(|| as_unix_seconds(moment, 1))?
}

/// Applies `localtime`, converting a UTC moment to the machine's zone.
///
/// @param moment - the moment so far
fn to_local_time(moment: Moment) -> Option<Moment> {
    if moment.zone == Zone::Local {
        return Some(moment);
    }
    // A bare number outside the Julian day range has no date, so SQLite converts
    // its default date, 2000-01-01 00:00:00.
    let day = if moment.raw.is_some_and(|raw| !is_julian_day_number(raw)) {
        2_451_544.5
    } else {
        moment.day
    };
    let mut moved = Moment {
        raw: None,
        ..moment.moved_to(day + zone_offset_days(day)?)
    };
    moved.zone = Zone::Local;
    Some(moved)
}

/// Applies `utc`, converting a moment in the machine's zone to UTC.
///
/// Two steps, as SQLite does it: the offset is asked for again at the instant the
/// first step landed on, so a conversion that crosses a daylight saving boundary
/// uses the offset in force on the far side.
///
/// @param moment - the moment so far
fn to_universal_time(moment: Moment) -> Option<Moment> {
    if moment.zone == Zone::Utc {
        return Some(moment);
    }
    // Without a valid Julian day SQLite converts Julian day 0.
    let day = if moment.raw.is_some_and(|raw| !is_julian_day_number(raw)) {
        0.0
    } else {
        moment.day
    };
    let first = zone_offset_days(day)?;
    let second = zone_offset_days(day - first)?;
    let mut moved = Moment {
        raw: None,
        ..moment.moved_to(day - second)
    };
    moved.zone = Zone::Utc;
    Some(moved)
}

/// Applies `start of day|month|year`, `weekday N`, and the numeric modifiers.
///
/// @param moment - the moment so far
/// @param folded - the modifier, trimmed and lowercased
fn apply_calendar_modifier(moment: Moment, folded: &[u8]) -> Option<Moment> {
    if let Some(rest) = folded.strip_prefix(b"start of ") {
        let mut civil = civil_of(moment.day);
        civil.hour = 0;
        civil.minute = 0;
        civil.second = 0.0;
        match rest {
            b"day" => {}
            b"month" => civil.day = 1,
            b"year" => {
                civil.month = 1;
                civil.day = 1;
            }
            _ => return None,
        }
        return Some(Moment {
            raw: None,
            ..moment.moved_to(julian_of(civil))
        });
    }
    if let Some(rest) = folded.strip_prefix(b"weekday ") {
        return advance_to_weekday(moment, rest);
    }
    // A bare number outside the Julian day range has no date until `unixepoch` or
    // `auto` reads it, so a numeric offset applied to it has no answer.
    if moment.raw.is_some_and(|raw| !is_julian_day_number(raw)) {
        return None;
    }
    apply_offset(moment, folded)
}

/// Applies `weekday N`, which moves forward to the next day that is weekday N.
///
/// **The time of day is kept.** Moving to a Wednesday from Monday 10:11:12 lands on
/// Wednesday 10:11:12; it used to land on midnight. A day that already is weekday N
/// does not move. N must be a whole number from 0 (Sunday) to 6, written as a number:
/// `weekday 3.0` is accepted and `weekday 1.5` is not.
///
/// @param moment - the moment so far
/// @param rest - what follows `weekday `
fn advance_to_weekday(moment: Moment, rest: &[u8]) -> Option<Moment> {
    if !numeric::looks_numeric(rest, TextEncoding::Utf8) {
        return None;
    }
    let wanted = numeric::atof(rest, TextEncoding::Utf8).value;
    if !(0.0..7.0).contains(&wanted) || wanted.fract() != 0.0 {
        return None;
    }
    if moment.raw.is_some_and(|raw| !is_julian_day_number(raw)) {
        return None;
    }
    let millis = millis_of(moment.day);
    // Julian day 0 started on a Monday at noon; 36 hours later it is a Sunday
    // midnight, which makes the integer day count mod 7 the weekday with Sunday as 0.
    let mut current = (millis.saturating_add(129_600_000) / 86_400_000) % 7;
    let wanted = wanted as i64;
    if current > wanted {
        current -= 7;
    }
    let moved = millis.saturating_add((wanted - current) * 86_400_000);
    Some(Moment {
        raw: None,
        ..moment.moved_to(day_of_millis(moved))
    })
}

/// Returns the local zone's offset from UTC at one Julian day, in days.
///
/// `None` when the operating system will not convert that instant, which is
/// what makes `localtime` and `utc` answer NULL for a date outside the range
/// the platform's own conversion accepts rather than guessing an offset.
///
/// @param day - the instant, as a Julian day
fn zone_offset_days(day: f64) -> Option<f64> {
    let mut seconds = whole_seconds((day - UNIX_EPOCH_JD) * SECONDS_PER_DAY);
    // SQLite does not ask the system about years it cannot convert on every
    // platform. Outside 1971 through 2037 it asks about 2000-01-01 00:00:00 and
    // uses that offset, so `datetime(0,'utc')` has an answer.
    let year = civil_of(day).year;
    if !(1971..2038).contains(&year) {
        seconds = 946_684_800;
    }
    let offset = inillucent_vfs::zone::local_offset_seconds(seconds)?;
    Some(offset as f64 / SECONDS_PER_DAY)
}

/// Returns how many days past the end of its month a date reaches.
///
/// **What a `floor` modifier takes off (task-1979, F14).** A month or year
/// modifier lands on a day of the month the new month may not have: one month
/// after 31 January is 31 February, which carries into March. `ceiling`, the
/// default, keeps the carry; `floor` asks for the last day of the month
/// instead, and this is the difference between the two. 31 February is two days
/// past a leap February and three past a short one, so
/// `date('2020-01-31','+1 month','floor')` is 29 February 2020 and
/// `date('2021-01-31','+1 month','floor')` is 28 February 2021.
///
/// @param civil - the date the modifier built, before the carry is applied
fn days_past_the_month(civil: Civil) -> i64 {
    if civil.day <= 28 {
        return 0;
    }
    // The months with 31 days. A day of the month never exceeds 31, so nothing
    // reaches past one of them.
    if matches!(civil.month, 1 | 3 | 5 | 7 | 8 | 10 | 12) {
        return 0;
    }
    if civil.month != 2 {
        return i64::from(civil.day == 31);
    }
    let leap = civil.year.rem_euclid(4) == 0
        && (civil.year.rem_euclid(100) != 0 || civil.year.rem_euclid(400) == 0);
    match leap {
        true => civil.day - 29,
        false => civil.day - 28,
    }
}

/// Applies a numeric modifier: `+NNN unit`, `+HH:MM[:SS[.SSS]]` or `+YYYY-MM-DD[ HH:MM:SS]`.
///
/// This follows `parseModifier`. The text up to the first colon, space, or the `-` of
/// a `YYYY-MM-DD` is the amount, and what follows it says which of the three forms
/// the modifier is.
///
/// @param moment - the moment so far
/// @param folded - the modifier, trimmed and lowercased
fn apply_offset(moment: Moment, folded: &[u8]) -> Option<Moment> {
    let first = *folded.first()?;
    if !(first == b'+' || first == b'-' || first.is_ascii_digit()) {
        return None;
    }
    let amount_length = amount_length(folded);
    let signed = parse_amount(folded.get(..amount_length)?)?;
    match folded.get(amount_length) {
        Some(b'-') => add_calendar_interval(moment, folded, amount_length),
        Some(b':') => add_clock_interval(moment, folded),
        _ => add_unit_interval(moment, signed, folded.get(amount_length..)?),
    }
}

/// Returns how long the leading number of a modifier is.
///
/// The number ends at a colon, at whitespace, or at the `-` that starts the month of a
/// `YYYY-MM-DD` interval after a four or five digit year.
///
/// @param folded - the modifier
fn amount_length(folded: &[u8]) -> usize {
    let mut length = 1usize;
    while let Some(byte) = folded.get(length) {
        if *byte == b':' || numeric::is_space(*byte) {
            break;
        }
        let year_width = length.saturating_sub(1);
        if *byte == b'-'
            && (length == 5 || length == 6)
            && digit_field(folded, 1, year_width, 0, 14_712).is_some()
        {
            break;
        }
        length = length.saturating_add(1);
    }
    length
}

/// Parses the amount of a modifier, which may carry a sign.
///
/// @param text - the amount as written
fn parse_amount(text: &[u8]) -> Option<f64> {
    let negative = text.first() == Some(&b'-');
    let magnitude = match text.first() {
        Some(b'+') | Some(b'-') => text.get(1..)?,
        _ => text,
    };
    if magnitude.is_empty() || !numeric::looks_numeric(magnitude, TextEncoding::Utf8) {
        return None;
    }
    let parsed = numeric::atof(magnitude, TextEncoding::Utf8).value;
    Some(if negative { -parsed } else { parsed })
}

/// Adds `+YYYY-MM-DD` or `+YYYY-MM-DD HH:MM:SS`, or subtracts it for a `-`.
///
/// Years and months move the calendar and the days move the clock, in that order,
/// and a time after the date is added or subtracted as a clock interval. The month
/// runs 0 to 11 and the day 0 to 30. It must begin with a sign.
///
/// @param moment - the moment so far
/// @param folded - the modifier
/// @param width - how long the year part is with its sign, 5 or 6
fn add_calendar_interval(moment: Moment, folded: &[u8], width: usize) -> Option<Moment> {
    let subtract = match folded.first() {
        Some(b'+') => false,
        Some(b'-') => true,
        _ => return None,
    };
    let year = digit_field(folded, 1, width.saturating_sub(1), 0, 14_712)?;
    let month = digit_field(folded, width.saturating_add(1), 2, 0, 12)?;
    (folded.get(width.saturating_add(3)) == Some(&b'-')).then_some(())?;
    let days = digit_field(folded, width.saturating_add(4), 2, 0, 31)?;
    if month >= 12 || days >= 31 {
        return None;
    }
    let sign = if subtract { -1 } else { 1 };
    // The months are carried into the year before the date is built, as SQLite does,
    // so eleven months past May is April of the next year and not a thirteenth month.
    let mut civil = civil_of(moment.day);
    let total = civil.year * 12 + (civil.month - 1) + sign * (year * 12 + month);
    civil.year = total.div_euclid(12);
    civil.month = total.rem_euclid(12) + 1;
    let calendar = moved_by_calendar(moment, civil);
    let moved = Moment {
        day: day_of_millis(millis_of(calendar.day).saturating_add(sign * days * 86_400_000)),
        ..calendar
    };
    let rest = folded.get(width.saturating_add(6)..)?;
    if rest.is_empty() {
        return Some(moved);
    }
    // After the date comes one space and a clock time; `+0001-01-01 ` with nothing
    // after the space has no date.
    let (separator, clock) = rest.split_first()?;
    numeric::is_space(*separator).then_some(())?;
    let millis = clock_interval_millis(clock)?;
    Some(Moment {
        day: day_of_millis(millis_of(moved.day).saturating_add(sign * millis)),
        ..moved
    })
}

/// Adds `+HH:MM[:SS[.SSS]]`, or subtracts it for a `-`.
///
/// @param moment - the moment so far
/// @param folded - the modifier
fn add_clock_interval(moment: Moment, folded: &[u8]) -> Option<Moment> {
    let sign = if folded.first() == Some(&b'-') { -1 } else { 1 };
    let clock = match folded.first() {
        Some(byte) if byte.is_ascii_digit() => folded,
        _ => folded.get(1..)?,
    };
    let millis = clock_interval_millis(clock)?;
    Some(Moment {
        raw: None,
        ..moment.moved_to(day_of_millis(
            millis_of(moment.day).saturating_add(sign * millis),
        ))
    })
}

/// Returns the length of a clock time in milliseconds, as a modifier counts it.
///
/// SQLite parses the time as a moment on a day, takes the day away, and keeps what is
/// left, so `24:00:00` is a whole day and a whole day is no interval at all.
///
/// @param clock - `HH:MM[:SS[.SSS]]`
fn clock_interval_millis(clock: &[u8]) -> Option<i64> {
    let (hour, minute, second, offset) = parse_hms(clock)?;
    // A zone on the time shifts it, as it does for a date, and the day is then taken
    // away: `00:00:00+01:00` is twenty three hours.
    let millis =
        hour * 3_600_000 + minute * 60_000 + (second * 1000.0 + 0.5) as i64 - offset * 60_000;
    Some(millis.rem_euclid(86_400_000))
}

/// Adds `NNN unit`, where the unit is seconds, minutes, hours, days, months or years.
///
/// The unit may be plural, and the amount may have a fraction: half a month is fifteen
/// days of thirty, and half a year is half of 365 days. An amount past the limit for
/// its unit has no date.
///
/// @param moment - the moment so far
/// @param amount - the amount, signed
/// @param unit - the text after the amount, with its leading whitespace
fn add_unit_interval(moment: Moment, amount: f64, unit: &[u8]) -> Option<Moment> {
    let unit = skip_spaces(unit);
    if unit.len() > 10 || unit.len() < 3 {
        return None;
    }
    let unit = unit.strip_suffix(b"s").unwrap_or(unit);
    // The name, the largest amount SQLite accepts, and the length of the unit in seconds.
    let (limit, seconds) = match unit {
        b"second" => (4.6427e14, 1.0),
        b"minute" => (7.7379e12, 60.0),
        b"hour" => (1.2897e11, 3600.0),
        b"day" => (5_373_485.0, 86_400.0),
        b"month" => (176_546.0, 2_592_000.0),
        b"year" => (14_713.0, 31_536_000.0),
        _ => return None,
    };
    if !(amount > -limit && amount < limit) {
        return None;
    }
    let (whole, fraction) = (amount.trunc(), amount - amount.trunc());
    let calendar = match unit {
        b"month" => Some(moved_by_months(moment, whole as i64)),
        b"year" => Some(moved_by_years(moment, whole as i64)),
        _ => None,
    };
    let (base, rest) = match calendar {
        Some(moved) => (moved, fraction),
        None => (
            Moment {
                overflow: 0,
                ..moment
            },
            amount,
        ),
    };
    let rounder = if rest < 0.0 { -0.5 } else { 0.5 };
    let step = (rest * 1000.0 * seconds + rounder) as i64;
    Some(Moment {
        raw: None,
        ..base.moved_to(day_of_millis(millis_of(base.day).saturating_add(step)))
    })
}

/// Moves a moment by whole months on the calendar.
///
/// @param moment - the moment so far
/// @param months - how many months, which may be negative
fn moved_by_months(moment: Moment, months: i64) -> Moment {
    let mut civil = civil_of(moment.day);
    let total = civil.year * 12 + (civil.month - 1) + months;
    civil.year = total.div_euclid(12);
    civil.month = total.rem_euclid(12) + 1;
    moved_by_calendar(moment, civil)
}

/// Moves a moment by whole years on the calendar.
///
/// @param moment - the moment so far
/// @param years - how many years, which may be negative
fn moved_by_years(moment: Moment, years: i64) -> Moment {
    let mut civil = civil_of(moment.day);
    civil.year += years;
    moved_by_calendar(moment, civil)
}

/// Returns the moment a calendar date names, with its carry recorded.
///
/// @param moment - the moment the modifier was applied to
/// @param civil - the date a month or year modifier built, which may name a day
///   the month does not have
fn moved_by_calendar(moment: Moment, civil: Civil) -> Moment {
    Moment {
        day: julian_of(civil),
        spelled: None,
        overflow: days_past_the_month(civil),
        zone: moment.zone,
        raw: None,
    }
}

/// Renders one moment through a `strftime` format.
///
/// The specifiers that name a field of the calendar read `civil`, which is what
/// the input spelled when it spelled one; the specifiers that name a position
/// in time - `%s`, `%J`, the week and weekday numbers - read `day`. That split
/// is SQLite's, and it is what makes `strftime('%H','2020-01-01 24:00:00')`
/// answer `24` while `strftime('%s', ...)` answers the next day's midnight.
///
/// @param day - the Julian day
/// @param civil - the broken-down fields to read
/// @param format - the format string
/// @param years - how to write a year out; the two SQLite formatters differ
fn render(day: f64, civil: Civil, format: &[u8], years: YearStyle) -> Value<'static> {
    let mut out: Vec<u8> = Vec::new();
    let mut index = 0usize;
    while index < format.len() {
        let byte = format.get(index).copied().unwrap_or(0);
        index = index.saturating_add(1);
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let Some(code) = format.get(index).copied() else {
            out.push(b'%');
            break;
        };
        index = index.saturating_add(1);
        match code {
            b'%' => out.push(b'%'),
            b'd' => push_padded(&mut out, civil.day, 2),
            b'e' => {
                let text = format!("{:2}", civil.day);
                out.extend_from_slice(text.as_bytes());
            }
            b'f' => {
                let text = format!("{:06.3}", civil.second);
                out.extend_from_slice(text.as_bytes());
            }
            b'F' => {
                push_year(&mut out, civil.year, years);
                let text = format!("-{:02}-{:02}", civil.month, civil.day);
                out.extend_from_slice(text.as_bytes());
            }
            b'H' => push_padded(&mut out, civil.hour, 2),
            b'I' => {
                let hour = match civil.hour % 12 {
                    0 => 12,
                    other => other,
                };
                push_padded(&mut out, hour, 2);
            }
            b'j' => push_padded(&mut out, day_of_year(civil), 3),
            b'J' => {
                // SQLite prints this one with sixteen significant digits, which
                // is not what the shortest round-trip rendering gives:
                // `2460370.878553241` against `2460370.8785532406`.
                out.extend_from_slice(sixteen_significant(on_the_millisecond(day)).as_bytes());
            }
            // The space-padded hours, which were being echoed back as `%k` and
            // `%l`. Fixing one member of a specifier family and assuming the
            // rest works is exactly how bugs in the others stay hidden, so the
            // whole table was walked against the reference rather than just
            // the ones that were reported.
            b'k' => {
                let text = format!("{:2}", civil.hour);
                out.extend_from_slice(text.as_bytes());
            }
            b'l' => {
                let hour = match civil.hour % 12 {
                    0 => 12,
                    other => other,
                };
                let text = format!("{hour:2}");
                out.extend_from_slice(text.as_bytes());
            }
            b'g' => push_padded(&mut out, iso_week(day).0.rem_euclid(100), 2),
            b'm' => push_padded(&mut out, civil.month, 2),
            b'M' => push_padded(&mut out, civil.minute, 2),
            b'p' => out.extend_from_slice(if civil.hour < 12 { b"AM" } else { b"PM" }),
            b'P' => out.extend_from_slice(if civil.hour < 12 { b"am" } else { b"pm" }),
            b'R' => {
                let text = format!("{:02}:{:02}", civil.hour, civil.minute);
                out.extend_from_slice(text.as_bytes());
            }
            b's' => {
                let seconds = whole_seconds((day - UNIX_EPOCH_JD) * SECONDS_PER_DAY);
                out.extend_from_slice(seconds.to_string().as_bytes());
            }
            b'S' => push_padded(&mut out, civil.second.floor() as i64, 2),
            b'T' => {
                let text = format!(
                    "{:02}:{:02}:{:02}",
                    civil.hour,
                    civil.minute,
                    civil.second.floor() as i64
                );
                out.extend_from_slice(text.as_bytes());
            }
            b'u' => {
                let weekday = weekday(day);
                push_padded(&mut out, if weekday == 0 { 7 } else { weekday }, 1);
            }
            b'w' => push_padded(&mut out, weekday(day), 1),
            b'U' => push_padded(&mut out, week_from(civil, days_after_sunday(day)), 2),
            b'V' => push_padded(&mut out, iso_week(day).1, 2),
            b'W' => push_padded(&mut out, week_from(civil, days_after_monday(day)), 2),
            b'G' => {
                let text = format!("{:04}", iso_week(day).0);
                out.extend_from_slice(text.as_bytes());
            }
            b'Y' => push_year(&mut out, civil.year, years),
            // **A specifier SQLite does not have makes the whole call NULL**,
            // rather than putting the two characters back. `strftime('%y', d)`
            // is NULL in SQLite and was the literal text `%y` here, which is a
            // format string silently half-applied.
            _ => return Value::Null,
        }
    }
    Value::owned_text(&out).unwrap_or(Value::Null)
}

/// Renders a double the way C's `%.16g` does.
///
/// Sixteen significant digits, trailing zeros removed, and the fixed form for
/// an exponent in the range a Julian day lives in. Only `%J` needs it, and it
/// needs it exactly: the shortest round-trip rendering Rust gives has
/// seventeen digits for the same value.
///
/// @param value - the number to render
fn sixteen_significant(value: f64) -> String {
    if !value.is_finite() || value == 0.0 {
        return format!("{value}");
    }
    let exponent = value.abs().log10().floor() as i32;
    if !(-5..16).contains(&exponent) {
        return format!("{value}");
    }
    let decimals = (15 - exponent).max(0) as usize;
    let text = format!("{value:.decimals$}");
    if !text.contains('.') {
        return text;
    }
    let trimmed = text.trim_end_matches('0');
    trimmed.trim_end_matches('.').to_string()
}

/// Appends a year.
///
/// **The two SQLite formatters differ on a negative year (task-1979, F23).**
/// `date('0000-01-01','-1 day')` is `-0001-12-31` because `dateFunc` writes the
/// sign and then four digits of its own; `strftime('%Y', ...)` on the same
/// value is `-001` because it hands the year to C's `%04d`, which counts the
/// sign against the width. Both were checked against 3.53.4. This module
/// answered `-001` for both, which was right for one of them.
///
/// @param out - the buffer being built
/// @param year - the year, which may be negative
/// @param style - which of the two formatters is asking
fn push_year(out: &mut Vec<u8>, year: i64, style: YearStyle) {
    let text = match style {
        YearStyle::Printf => format!("{year:04}"),
        YearStyle::Signed => match year < 0 {
            true => format!("-{:04}", year.saturating_abs()),
            false => format!("{year:04}"),
        },
    };
    out.extend_from_slice(text.as_bytes());
}

/// Appends a zero-padded number.
fn push_padded(out: &mut Vec<u8>, value: i64, width: usize) {
    let text = format!("{value:0width$}");
    out.extend_from_slice(text.as_bytes());
}

/// Returns the day of the week, Sunday as zero.
fn weekday(day: f64) -> i64 {
    ((day + 1.5).floor() as i64).rem_euclid(7)
}

/// Returns the day of the year, 1 January as one.
fn day_of_year(civil: Civil) -> i64 {
    let start = julian_of(Civil {
        year: civil.year,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0.0,
    });
    let here = julian_of(Civil {
        hour: 0,
        minute: 0,
        second: 0.0,
        ..civil
    });
    (here - start) as i64 + 1
}

/// Returns how many days have passed since the first of January.
///
/// Zero on the first, which is what SQLite's `daysAfterJan01` counts. Our
/// `day_of_year` is one-based, because `%j` is.
fn days_after_jan01(civil: Civil) -> i64 {
    day_of_year(civil).saturating_sub(1)
}

/// Returns how many days have passed since the week's Monday, Monday as zero.
fn days_after_monday(day: f64) -> i64 {
    (weekday(day) + 6).rem_euclid(7)
}

/// Returns how many days have passed since the week's Sunday, Sunday as zero.
fn days_after_sunday(day: f64) -> i64 {
    weekday(day)
}

/// Returns a week number counted from the year's first Monday or first Sunday.
///
/// **SQLite's own arithmetic, transcribed.** `%W` counts weeks whose first day
/// is Monday and `%U` weeks whose first day is Sunday; in both, the days before
/// the year's first such day are week 00 and the first is week 01. The previous
/// implementation counted from the first *Sunday* for `%W` and was off by one
/// besides, which put `strftime('%Y-%W','2024-03-01')` at `2024-08` against the
/// reference's `2024-09`.
///
/// @param civil - the instant, as a date
/// @param days_after_start - days since the week's first day
fn week_from(civil: Civil, days_after_start: i64) -> i64 {
    (days_after_jan01(civil) - days_after_start + 7) / 7
}

/// Returns the ISO-8601 week-numbering year and week of a date.
///
/// The week a date belongs to is the week holding its Thursday, and the year is
/// that Thursday's - which is why the two have to be computed together and why
/// `2023-01-01` is week 52 of 2022.
///
/// **A Julian day starts at noon, so `floor` is the wrong midnight.** For a
/// timestamp before noon it lands on the previous calendar day, and the week
/// computed from it was a day early: `strftime('%V %G','2026-01-04')` answered
/// week 53 of 2025 where SQLite answers week 01 of 2026, while the same date at
/// 13:00 answered correctly - which is why this survived every case that
/// carried a time. The calendar day's own midnight is `(day + 0.5).floor()
/// - 0.5`, the basis `weekday` already counts from.
///
/// @param day - the julian day
fn iso_week(day: f64) -> (i64, i64) {
    let midnight = (day + 0.5).floor() - 0.5;
    let thursday = midnight + (3 - days_after_monday(day)) as f64;
    let moved = civil_of(thursday);
    (moved.year, days_after_jan01(moved) / 7 + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Julian day of the unix epoch is the constant everything else is
    /// derived from, so it is worth pinning on its own.
    #[test]
    fn the_epoch_is_where_it_should_be() {
        let civil = Civil {
            year: 1970,
            month: 1,
            day: 1,
            hour: 0,
            minute: 0,
            second: 0.0,
        };
        assert!((julian_of(civil) - UNIX_EPOCH_JD).abs() < 1e-9);
    }

    /// Every civil date round-trips through the Julian day and back.
    #[test]
    fn civil_dates_round_trip() {
        for (year, month, day) in [
            (1970, 1, 1),
            (2000, 2, 29),
            (1999, 12, 31),
            (2026, 9, 3),
            (1582, 10, 15),
            (1200, 6, 6),
        ] {
            let civil = Civil {
                year,
                month,
                day,
                hour: 13,
                minute: 45,
                second: 30.0,
            };
            let back = civil_of(julian_of(civil));
            assert_eq!(
                (back.year, back.month, back.day, back.hour, back.minute),
                (year, month, day, 13, 45),
                "{year}-{month}-{day}"
            );
        }
    }
}

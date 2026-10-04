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

mod state;

use state::DateTime;

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
    let Some(mut value) = resolve(rest, now, encoding) else {
        return Value::Null;
    };
    let day = value.jd as f64 / MILLIS_PER_DAY;
    value.compute_ymd_hms();
    let civil = Civil {
        year: value.year,
        month: value.month,
        day: value.day,
        hour: value.hour,
        minute: value.minute,
        second: value.second,
    };
    // **`subsec` is a modifier the *renderer* has to know about.** It asks for
    // the fractional second, so `datetime(x, 'subsec')` formats seconds as
    // `%f` rather than `%S`. It is read off the value, because a later `utc`
    // rebuilds the value and forgets it, as SQLite's does.
    let subsec = value.use_subsec;
    match func {
        TimeFunc::JulianDay => Value::Real(day),
        TimeFunc::UnixEpoch => {
            if subsec {
                Value::Real((value.jd - 210_866_760_000_000) as f64 / 1000.0)
            } else {
                Value::Integer(value.jd / 1000 - 210_866_760_000)
            }
        }
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

/// Returns the value one argument list resolves to, as SQLite's `isDate` does.
///
/// The first argument is the time value and every later one is a modifier,
/// applied in order. With no arguments at all the value is `'now'`, which is
/// why `date()` and `date('now')` are the same call.
///
/// `None` when any step has no answer, when a step recorded an error that no
/// later `localtime` or `utc` cleared, and when the date is outside Julian day
/// 0 to 5373484.5, roughly the year -4713 to the end of 9999.
///
/// **A day of the month the month does not have is normalised only when there
/// is no modifier.** `date('2020-02-30')` is `2020-03-01`, but
/// `datetime('2020-02-30','subsec')` is `2020-02-30 00:00:00.000`, because
/// `isDate` marks the spelled fields stale only when `argc==1`. The hour is
/// never normalised, so `datetime('2020-02-30 24:00:00')` keeps hour 24.
///
/// @param arguments - the time value and its modifiers
/// @param now - the wall clock the caller supplies
/// @param encoding - the text encoding a numeric string is read in
fn resolve(arguments: &[Value<'static>], now: f64, encoding: TextEncoding) -> Option<DateTime> {
    let mut value = match arguments.first() {
        Some(first) => parse_time_value(first, now, encoding)?,
        None => DateTime::now(millis_of(now))?,
    };
    for (index, modifier) in arguments.get(1..).unwrap_or(&[]).iter().enumerate() {
        let text = modifier_bytes(modifier)?;
        if !value.apply(&text, index.saturating_add(1)) {
            return None;
        }
    }
    value.compute_jd();
    if value.is_error || !state::valid_julian_day(value.jd) {
        return None;
    }
    if arguments.len() == 1 && value.valid_ymd && value.day > 28 {
        value.valid_ymd = false;
    }
    Some(value)
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
    // Each side is resolved alone, as `isDate` with one argument resolves it.
    let (Some(left), Some(right)) = (
        resolve(std::slice::from_ref(left), now, encoding),
        resolve(std::slice::from_ref(right), now, encoding),
    ) else {
        return Value::Null;
    };
    let (left, right) = (
        left.jd as f64 / MILLIS_PER_DAY,
        right.jd as f64 / MILLIS_PER_DAY,
    );
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

/// Returns the value a time value names, as SQLite's `isDate` and
/// `parseDateOrTime` read it.
///
/// A number is kept raw: a Julian day unless a `unixepoch` or `auto` modifier
/// says otherwise. Text is tried as a date with an optional time, then as a
/// time alone, then as `now`, then as a number, then as `subsec`, which is the
/// current time with its fraction. A blob is read as the text of its bytes.
///
/// @param value - the first argument of the call
/// @param now - the wall clock the caller supplies
/// @param encoding - the text encoding a numeric string is read in
fn parse_time_value(value: &Value<'static>, now: f64, encoding: TextEncoding) -> Option<DateTime> {
    match value {
        Value::Null => None,
        Value::Integer(integer) => Some(DateTime::raw(*integer as f64)),
        Value::Real(real) => Some(DateTime::raw(*real)),
        Value::Blob(blob) => parse_time_text(blob.raw(), now, encoding),
        Value::Text(text) => parse_time_text(&text.utf8_bytes(), now, encoding),
    }
}

/// Returns the value a piece of text names.
///
/// **Only trailing whitespace is skipped** (task-2066 section 4.2, item 27).
/// SQLite's `parseYyyyMmDd` reads the year at the first byte and skips
/// whitespace after the date, so `date(' 2024-01-01')` is NULL and
/// `date('2024-01-01 ')` is a date.
///
/// @param raw - the text
/// @param now - the wall clock the caller supplies
/// @param encoding - the text encoding a numeric string is read in
fn parse_time_text(raw: &[u8], now: f64, encoding: TextEncoding) -> Option<DateTime> {
    let spelled = trim_end(raw);
    if let Some((civil, offset)) = parse_date_and_time(spelled) {
        let zulu = offset == 0 && names_a_zone(spelled);
        return Some(DateTime::spelled(civil, true, offset, zulu));
    }
    if let Some((hour, minute, second, offset)) = parse_hms(spelled) {
        let civil = Civil {
            year: 2000,
            month: 1,
            day: 1,
            hour,
            minute,
            second,
        };
        let zulu = offset == 0 && names_a_zone(spelled);
        return Some(DateTime::spelled(civil, false, offset, zulu));
    }
    if raw.eq_ignore_ascii_case(b"now") {
        return DateTime::now(millis_of(now));
    }
    let trimmed = trim(raw);
    if numeric::looks_numeric(trimmed, encoding) {
        return Some(DateTime::raw(numeric::atof(trimmed, encoding).value));
    }
    if raw.eq_ignore_ascii_case(b"subsec") || raw.eq_ignore_ascii_case(b"subsecond") {
        let mut current = DateTime::now(millis_of(now))?;
        current.use_subsec = true;
        return Some(current);
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

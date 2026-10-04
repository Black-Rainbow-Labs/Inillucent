//! SQLite's `DateTime` and its modifiers, step for step.
//!
//! Invariant: **every modifier changes the same fields, in the same order, as
//! `parseModifier` in the pinned 3.53.4.** The date functions' answers on
//! ordinary input follow from a Julian day alone, but their answers on unusual
//! input follow from which fields SQLite treats as current, and those rules are
//! only reproduced by keeping the same fields:
//!
//! - A bare number outside the Julian day range is kept raw (`raw_s`). A
//!   numeric modifier then finds no date, `compute_jd` records an error and
//!   resets every field, and the modifier is applied to the default date,
//!   2000-01-01. The error makes the answer NULL, unless `localtime` or `utc`
//!   runs afterwards: both rebuild the value and clear the error, so
//!   `datetime(1e12, '+1 hour', 'localtime')` is 2000-01-01 01:00 UTC in local
//!   time.
//! - A date spelled with a day its month lacks, such as `'2024-02-30'`, keeps
//!   those fields until a modifier recomputes them. `+1 year` moves the year of
//!   the spelled fields, so the answer is 2025-02-30, which is 2025-03-02.
//! - A modifier that has to break a Julian day into fields (`weekday`, `start
//!   of`, months, years) refuses one before -4713-11-24 12:00, and the whole
//!   call is then NULL even when a later step would bring the date back.
//! - `localtime` outside 1970 to 2038 asks the system about the same day in a
//!   year from 2000 to 2003 with the same leap position, keeping the month and
//!   day, so a summer date in 1960 gets the summer offset.
//! - `utc` finds the UTC instant by asking `localtime` about its guess and
//!   correcting it, up to four times, so a local time a daylight saving change
//!   skipped lands where SQLite's search lands.
//!
//! The parsing of the time value and the rendering of the answer stay in
//! `datetime`; this module is the state between them. Integer division here
//! truncates toward zero, as C's does, because SQLite's formulas are written
//! for it.

use super::{
    amount_length, civil_of_unix_day, digit_field, parse_amount, parse_hms, skip_spaces, Civil,
};
use inillucent_value::{numeric, TextEncoding};

/// Milliseconds in a day.
const DAY_MS: i64 = 86_400_000;

/// The largest Julian day in milliseconds SQLite accepts: 9999-12-31 23:59:59.999.
const LAST_JULIAN_MS: i64 = 464_269_060_799_999;

/// SQLite's `DateTime`: a Julian day, the broken-down fields, and which of the
/// two is current.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct DateTime {
    /// The Julian day in milliseconds, when `valid_jd`.
    pub(super) jd: i64,
    /// The year, when `valid_ymd`.
    pub(super) year: i64,
    /// The month, when `valid_ymd`.
    pub(super) month: i64,
    /// The day of the month, when `valid_ymd`. It may be past the month's end.
    pub(super) day: i64,
    /// The hour, when `valid_hms`. It may be 24.
    pub(super) hour: i64,
    /// The minute, when `valid_hms`.
    pub(super) minute: i64,
    /// The second with its fraction when `valid_hms`, or the raw number when `raw_s`.
    pub(super) second: f64,
    /// Whether `jd` is current.
    pub(super) valid_jd: bool,
    /// Whether `second` holds a bare number a modifier may still reinterpret.
    pub(super) raw_s: bool,
    /// Whether the year, month and day are current.
    pub(super) valid_ymd: bool,
    /// Whether the hour, minute and second are current.
    pub(super) valid_hms: bool,
    /// How many days `floor` takes off, after a month or year modifier.
    pub(super) n_floor: i64,
    /// The zone offset in minutes still to be applied to the fields.
    pub(super) tz: i64,
    /// Whether the value is known to be UTC, so `utc` does nothing.
    pub(super) is_utc: bool,
    /// Whether the value is known to be local time, so `localtime` does nothing.
    pub(super) is_local: bool,
    /// Whether a step found no date. The call answers NULL unless a later
    /// `localtime` or `utc` rebuilds the value.
    pub(super) is_error: bool,
    /// Whether `subsec` asked for the fractional second.
    pub(super) use_subsec: bool,
}

/// Reports whether a Julian day in milliseconds is one SQLite answers with.
///
/// @param jd - the Julian day in milliseconds
pub(super) fn valid_julian_day(jd: i64) -> bool {
    (0..=LAST_JULIAN_MS).contains(&jd)
}

impl DateTime {
    /// Returns the value of the clock, as `setDateTimeToCurrent` sets it.
    ///
    /// @param jd - the current time as a Julian day in milliseconds
    pub(super) fn now(jd: i64) -> Option<DateTime> {
        (jd > 0).then_some(DateTime {
            jd,
            valid_jd: true,
            is_utc: true,
            ..DateTime::default()
        })
    }

    /// Returns the value of a bare number, as `setRawDateNumber` sets it.
    ///
    /// @param number - the number the call was given
    pub(super) fn raw(number: f64) -> DateTime {
        let mut value = DateTime {
            second: number,
            raw_s: true,
            ..DateTime::default()
        };
        if (0.0..5_373_484.5).contains(&number) {
            value.jd = (number * 86_400_000.0 + 0.5) as i64;
            value.valid_jd = true;
        }
        value
    }

    /// Returns the value of parsed date text: the fields, and the zone it named.
    ///
    /// A time with no date keeps its zone until the Julian day is next
    /// computed, as `parseHhMmSs` leaves it; a date with a time applies it at
    /// once, as `parseYyyyMmDd` does.
    ///
    /// @param civil - the fields as written
    /// @param has_date - whether the text began with a date
    /// @param offset - the zone offset in minutes, or zero
    /// @param zulu - whether the text named a zone that is UTC: `Z` or `+00:00`
    pub(super) fn spelled(civil: Civil, has_date: bool, offset: i64, zulu: bool) -> DateTime {
        let mut value = DateTime {
            hour: civil.hour,
            minute: civil.minute,
            second: civil.second,
            valid_hms: true,
            tz: offset,
            is_utc: zulu,
            ..DateTime::default()
        };
        if has_date {
            value.year = civil.year;
            value.month = civil.month;
            value.day = civil.day;
            value.valid_ymd = true;
            value.compute_floor();
            if value.tz != 0 {
                value.compute_jd();
            }
        }
        value
    }

    /// Records that a step found no date, clearing every field, as `datetimeError` does.
    fn error(&mut self) {
        *self = DateTime {
            is_error: true,
            ..DateTime::default()
        };
    }

    /// Makes `jd` current from the fields, as `computeJD` does.
    ///
    /// With no date the date is 2000-01-01. A year outside -4713 to 9999, or a
    /// raw number nothing has explained, is an error.
    pub(super) fn compute_jd(&mut self) {
        if self.valid_jd {
            return;
        }
        let (mut year, mut month, day) = if self.valid_ymd {
            (self.year, self.month, self.day)
        } else {
            (2000, 1, 1)
        };
        if !(-4713..=9999).contains(&year) || self.raw_s {
            self.error();
            return;
        }
        if month <= 2 {
            year -= 1;
            month += 12;
        }
        let a = (year + 4800) / 100;
        let b = 38 - a + (a / 4);
        let x1 = 36525 * (year + 4716) / 100;
        let x2 = 306_001 * (month + 1) / 10_000;
        self.jd = (((x1 + x2 + day + b) as f64 - 1524.5) * 86_400_000.0) as i64;
        self.valid_jd = true;
        if self.valid_hms {
            self.jd = self
                .jd
                .wrapping_add(self.hour * 3_600_000 + self.minute * 60_000)
                .wrapping_add((self.second * 1000.0 + 0.5) as i64);
            if self.tz != 0 {
                self.jd = self.jd.wrapping_sub(self.tz * 60_000);
                self.valid_ymd = false;
                self.valid_hms = false;
                self.tz = 0;
                self.is_utc = true;
                self.is_local = false;
            }
        }
    }

    /// Makes the year, month and day current from `jd`, as `computeYMD` does.
    pub(super) fn compute_ymd(&mut self) {
        if self.valid_ymd {
            return;
        }
        if !self.valid_jd {
            self.year = 2000;
            self.month = 1;
            self.day = 1;
        } else if !valid_julian_day(self.jd) {
            self.error();
            return;
        } else {
            let z = (self.jd + 43_200_000) / DAY_MS;
            let alpha = ((z as f64 + 32_044.75) / 36_524.25) as i64 - 52;
            let a = z + 1 + alpha - ((alpha + 100) / 4) + 25;
            let b = a + 1524;
            let c = ((b as f64 - 122.1) / 365.25) as i64;
            let d = (36525 * (c & 32767)) / 100;
            let e = ((b - d) as f64 / 30.6001) as i64;
            let x1 = (30.6001 * e as f64) as i64;
            self.day = b - d - x1;
            self.month = if e < 14 { e - 1 } else { e - 13 };
            self.year = if self.month > 2 { c - 4716 } else { c - 4715 };
        }
        self.valid_ymd = true;
    }

    /// Makes the hour, minute and second current from `jd`, as `computeHMS` does.
    pub(super) fn compute_hms(&mut self) {
        if self.valid_hms {
            return;
        }
        self.compute_jd();
        let day_ms = (self.jd.wrapping_add(43_200_000)) % DAY_MS;
        self.second = (day_ms % 60_000) as f64 / 1000.0;
        let day_min = day_ms / 60_000;
        self.minute = day_min % 60;
        self.hour = day_min / 60;
        self.raw_s = false;
        self.valid_hms = true;
    }

    /// Makes every field current.
    pub(super) fn compute_ymd_hms(&mut self) {
        self.compute_ymd();
        self.compute_hms();
    }

    /// Marks the fields and the zone stale, as `clearYMD_HMS_TZ` does.
    fn clear_ymd_hms_tz(&mut self) {
        self.valid_ymd = false;
        self.valid_hms = false;
        self.tz = 0;
    }

    /// Records how far past its month's end the day is, as `computeFloor` does.
    fn compute_floor(&mut self) {
        let leap = !(self.year % 4 != 0 || (self.year % 100 == 0 && self.year % 400 != 0));
        self.n_floor = if self.day <= 28 || matches!(self.month, 1 | 3 | 5 | 7 | 8 | 10 | 12) {
            0
        } else if self.month != 2 {
            i64::from(self.day == 31)
        } else if leap {
            self.day - 29
        } else {
            self.day - 28
        };
    }

    /// Reads a raw number as a Julian day or as Unix seconds by its size, as `autoAdjustDate` does.
    fn auto_adjust(&mut self) {
        if !self.raw_s || self.valid_jd {
            self.raw_s = false;
        } else if self.second >= -210_866_760_000.0 && self.second <= 253_402_300_799.0 {
            let r = self.second * 1000.0 + 210_866_760_000_000.0;
            self.clear_ymd_hms_tz();
            self.jd = (r + 0.5) as i64;
            self.valid_jd = true;
            self.raw_s = false;
        }
    }

    /// Converts the value from UTC to local time, as `toLocaltime` does.
    ///
    /// `false` when the system will not convert the instant.
    fn shift_to_localtime(&mut self) -> bool {
        self.compute_jd();
        let (seconds, year_shift) =
            if self.jd < 210_866_760_000_000 || self.jd > 213_014_145_600_000 {
                let mut shifted = *self;
                shifted.compute_ymd_hms();
                let year_shift = (2000 + shifted.year % 4) - shifted.year;
                shifted.year += year_shift;
                shifted.valid_jd = false;
                shifted.compute_jd();
                (shifted.jd / 1000 - 210_866_760_000, year_shift)
            } else {
                (self.jd / 1000 - 210_866_760_000, 0)
            };
        let Some(local) = local_fields(seconds) else {
            return false;
        };
        self.year = local.year - year_shift;
        self.month = local.month;
        self.day = local.day;
        self.hour = local.hour;
        self.minute = local.minute;
        self.second = local.second + (self.jd % 1000) as f64 * 0.001;
        self.valid_ymd = true;
        self.valid_hms = true;
        self.valid_jd = false;
        self.raw_s = false;
        self.tz = 0;
        self.is_error = false;
        true
    }

    /// Converts the value from local time to UTC, as the `utc` modifier does.
    ///
    /// The UTC instant is the guess whose local time is the value. Each round
    /// asks `localtime` about the guess and moves the guess by how far off the
    /// answer was, at most four times. The value is then rebuilt from the
    /// guess alone, which also clears an earlier error.
    fn shift_to_utc(&mut self) -> bool {
        if self.is_utc {
            return true;
        }
        self.compute_jd();
        let original = self.jd;
        let mut guess = original;
        let mut off_by = 0i64;
        let mut rounds = 0;
        loop {
            guess = guess.wrapping_sub(off_by);
            let mut probe = DateTime {
                jd: guess,
                valid_jd: true,
                ..DateTime::default()
            };
            if !probe.shift_to_localtime() {
                return false;
            }
            probe.compute_jd();
            off_by = probe.jd.wrapping_sub(original);
            let again = off_by != 0 && rounds < 3;
            rounds += 1;
            if !again {
                break;
            }
        }
        *self = DateTime {
            jd: guess,
            valid_jd: true,
            is_utc: true,
            ..DateTime::default()
        };
        true
    }

    /// Applies one modifier, as `parseModifier` does. `false` means the call has no answer.
    ///
    /// @param text - the modifier as written
    /// @param position - which modifier this is, counting from 1
    pub(super) fn apply(&mut self, text: &[u8], position: usize) -> bool {
        let folded = text.to_ascii_lowercase();
        match folded.first() {
            Some(b'a') if folded == b"auto" => {
                if position > 1 {
                    return false;
                }
                self.auto_adjust();
                true
            }
            Some(b'c') if folded == b"ceiling" => {
                self.compute_jd();
                self.clear_ymd_hms_tz();
                self.n_floor = 0;
                true
            }
            Some(b'f') if folded == b"floor" => {
                self.compute_jd();
                self.jd = self.jd.wrapping_sub(self.n_floor * DAY_MS);
                self.clear_ymd_hms_tz();
                true
            }
            Some(b'j') if folded == b"julianday" => {
                position <= 1 && self.valid_jd && self.raw_s && {
                    self.raw_s = false;
                    true
                }
            }
            Some(b'l') if folded == b"localtime" => {
                let converted = self.is_local || self.shift_to_localtime();
                self.is_utc = false;
                self.is_local = true;
                converted
            }
            Some(b'u') if folded == b"unixepoch" && self.raw_s => self.unix_epoch(position),
            Some(b'u') if folded == b"utc" => self.shift_to_utc(),
            Some(b'w') => self.weekday(&folded),
            Some(b's') => self.start_of(&folded),
            Some(first) if *first == b'+' || *first == b'-' || first.is_ascii_digit() => {
                self.numeric(&folded)
            }
            _ => false,
        }
    }

    /// Applies `unixepoch` to a raw number.
    ///
    /// @param position - which modifier this is
    fn unix_epoch(&mut self, position: usize) -> bool {
        if position > 1 {
            return false;
        }
        let r = self.second * 1000.0 + 210_866_760_000_000.0;
        if !(0.0..464_269_060_800_000.0).contains(&r) {
            return false;
        }
        self.clear_ymd_hms_tz();
        self.jd = (r + 0.5) as i64;
        self.valid_jd = true;
        self.raw_s = false;
        true
    }

    /// Applies `weekday N`.
    ///
    /// @param folded - the modifier, lowercased
    fn weekday(&mut self, folded: &[u8]) -> bool {
        let Some(rest) = folded.strip_prefix(b"weekday ") else {
            return false;
        };
        if !numeric::looks_numeric(rest, TextEncoding::Utf8) {
            return false;
        }
        let wanted = numeric::atof(rest, TextEncoding::Utf8).value;
        if !(0.0..7.0).contains(&wanted) || wanted.fract() != 0.0 {
            return false;
        }
        self.compute_ymd_hms();
        self.tz = 0;
        self.valid_jd = false;
        self.compute_jd();
        let mut current = ((self.jd.wrapping_add(129_600_000)) / DAY_MS) % 7;
        let wanted = wanted as i64;
        if current > wanted {
            current -= 7;
        }
        self.jd = self.jd.wrapping_add((wanted - current) * DAY_MS);
        self.clear_ymd_hms_tz();
        true
    }

    /// Applies `start of day|month|year`, and `subsec`.
    ///
    /// @param folded - the modifier, lowercased
    fn start_of(&mut self, folded: &[u8]) -> bool {
        let Some(unit) = folded.strip_prefix(b"start of ") else {
            if folded == b"subsec" || folded == b"subsecond" {
                self.use_subsec = true;
                return true;
            }
            return false;
        };
        if !self.valid_jd && !self.valid_ymd && !self.valid_hms {
            return false;
        }
        self.compute_ymd();
        self.valid_hms = true;
        self.hour = 0;
        self.minute = 0;
        self.second = 0.0;
        self.raw_s = false;
        self.tz = 0;
        self.valid_jd = false;
        match unit {
            b"month" => {
                self.day = 1;
                true
            }
            b"year" => {
                self.month = 1;
                self.day = 1;
                true
            }
            b"day" => true,
            _ => false,
        }
    }

    /// Applies a numeric modifier: `NNN unit`, `HH:MM[:SS]` or `YYYY-MM-DD[ HH:MM]`, signed.
    ///
    /// @param folded - the modifier, lowercased
    fn numeric(&mut self, folded: &[u8]) -> bool {
        let length = amount_length(folded);
        let Some(amount) = folded.get(..length).and_then(parse_amount) else {
            return false;
        };
        let negative = folded.first() == Some(&b'-');
        match folded.get(length) {
            Some(b'-') => self.calendar_interval(folded, length, negative),
            Some(b':') => self.clock_interval(folded, negative),
            _ => self.unit_interval(amount, folded.get(length..).unwrap_or(&[])),
        }
    }

    /// Applies `+YYYY-MM-DD`, optionally followed by a space and `HH:MM[:SS]`.
    ///
    /// @param folded - the modifier
    /// @param width - the length of the signed year, 5 or 6
    /// @param negative - whether it subtracts
    fn calendar_interval(&mut self, folded: &[u8], width: usize, negative: bool) -> bool {
        if !matches!(folded.first(), Some(b'+' | b'-')) {
            return false;
        }
        let (Some(years), Some(months), Some(days)) = (
            digit_field(folded, 1, width.saturating_sub(1), 0, 14_712),
            digit_field(folded, width.saturating_add(1), 2, 0, 12)
                .filter(|_| folded.get(width.saturating_add(3)) == Some(&b'-')),
            digit_field(folded, width.saturating_add(4), 2, 0, 31),
        ) else {
            return false;
        };
        if months >= 12 || days >= 31 {
            return false;
        }
        self.compute_ymd_hms();
        self.valid_jd = false;
        let days = if negative {
            self.year -= years;
            self.month -= months;
            -days
        } else {
            self.year += years;
            self.month += months;
            days
        };
        self.carry_months();
        self.compute_floor();
        self.compute_jd();
        self.valid_hms = false;
        self.valid_ymd = false;
        self.jd = self.jd.wrapping_add(days * DAY_MS);
        let rest = folded.get(width.saturating_add(6)..).unwrap_or(&[]);
        let Some((separator, clock)) = rest.split_first() else {
            return true;
        };
        if !numeric::is_space(*separator)
            || digit_field(clock, 0, 2, 0, 24).is_none()
            || clock.get(2) != Some(&b':')
            || digit_field(clock, 3, 2, 0, 59).is_none()
        {
            return false;
        }
        self.add_clock(clock, negative)
    }

    /// Applies `+HH:MM[:SS[.SSS]]`.
    ///
    /// @param folded - the modifier
    /// @param negative - whether it subtracts
    fn clock_interval(&mut self, folded: &[u8], negative: bool) -> bool {
        let clock = match folded.first() {
            Some(byte) if byte.is_ascii_digit() => folded,
            _ => folded.get(1..).unwrap_or(&[]),
        };
        self.add_clock(clock, negative)
    }

    /// Adds or subtracts a clock time, taken as a time of day on 2000-01-01
    /// with the day then taken away.
    ///
    /// @param clock - `HH:MM[:SS[.SSS]]`, with an optional zone
    /// @param negative - whether it subtracts
    fn add_clock(&mut self, clock: &[u8], negative: bool) -> bool {
        let Some((hour, minute, second, offset)) = parse_hms(clock) else {
            return false;
        };
        let mut time = DateTime {
            hour,
            minute,
            second,
            valid_hms: true,
            tz: offset,
            ..DateTime::default()
        };
        time.compute_jd();
        time.jd -= 43_200_000;
        let whole_days = time.jd / DAY_MS;
        time.jd -= whole_days * DAY_MS;
        if negative {
            time.jd = -time.jd;
        }
        self.compute_jd();
        self.clear_ymd_hms_tz();
        self.jd = self.jd.wrapping_add(time.jd);
        true
    }

    /// Applies `NNN unit`, where the unit is seconds, minutes, hours, days,
    /// months or years, singular or plural.
    ///
    /// @param amount - the signed amount
    /// @param rest - what follows the amount
    fn unit_interval(&mut self, amount: f64, rest: &[u8]) -> bool {
        let unit = skip_spaces(rest);
        if unit.len() < 3 || unit.len() > 10 {
            return false;
        }
        let unit = unit.strip_suffix(b"s").unwrap_or(unit);
        self.compute_jd();
        let rounder = if amount < 0.0 { -0.5 } else { 0.5 };
        self.n_floor = 0;
        let (limit, seconds) = match unit {
            b"second" => (4.6427e14, 1.0),
            b"minute" => (7.7379e12, 60.0),
            b"hour" => (1.2897e11, 3600.0),
            b"day" => (5_373_485.0, 86_400.0),
            b"month" => (176_546.0, 2_592_000.0),
            b"year" => (14_713.0, 31_536_000.0),
            _ => {
                self.clear_ymd_hms_tz();
                return false;
            }
        };
        if !(amount > -limit && amount < limit) {
            self.clear_ymd_hms_tz();
            return false;
        }
        let mut amount = amount;
        if unit == b"month" || unit == b"year" {
            self.compute_ymd_hms();
            if unit == b"month" {
                self.month += amount as i64;
                self.carry_months();
            } else {
                self.year += amount as i64;
            }
            self.compute_floor();
            self.valid_jd = false;
            amount -= amount.trunc();
        }
        self.compute_jd();
        self.jd = self
            .jd
            .wrapping_add((amount * 1000.0 * seconds + rounder) as i64);
        self.clear_ymd_hms_tz();
        true
    }

    /// Carries a month outside 1 to 12 into the year.
    fn carry_months(&mut self) {
        let carry = if self.month > 0 {
            (self.month - 1) / 12
        } else {
            (self.month - 12) / 12
        };
        self.year += carry;
        self.month -= carry * 12;
    }
}

/// Returns the local time of a Unix second as the system gives it, which is
/// what `osLocaltime` returns.
///
/// @param seconds - seconds since 1970-01-01 00:00:00 UTC
fn local_fields(seconds: i64) -> Option<Civil> {
    let offset = inillucent_vfs::zone::local_offset_seconds(seconds)?;
    let local = seconds.checked_add(offset)?;
    let of_day = local.rem_euclid(86_400);
    let date = civil_of_unix_day(local.div_euclid(86_400));
    Some(Civil {
        hour: of_day / 3600,
        minute: of_day / 60 % 60,
        second: (of_day % 60) as f64,
        ..date
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Applies modifiers to a value and returns the Julian day, or `None`.
    ///
    /// @param value - the value before the modifiers
    /// @param modifiers - the modifiers, in order
    fn resolved(mut value: DateTime, modifiers: &[&str]) -> Option<i64> {
        let applied = modifiers
            .iter()
            .enumerate()
            .all(|(position, modifier)| value.apply(modifier.as_bytes(), position + 1));
        value.compute_jd();
        (applied && !value.is_error && valid_julian_day(value.jd)).then_some(value.jd)
    }

    /// Returns the value of a spelled date with no time.
    ///
    /// @param year - the year
    /// @param month - the month
    /// @param day - the day, which may be past the month's end
    fn date(year: i64, month: i64, day: i64) -> DateTime {
        DateTime::spelled(
            Civil {
                year,
                month,
                day,
                hour: 0,
                minute: 0,
                second: 0.0,
            },
            true,
            0,
            false,
        )
    }

    /// The Julian day of a date at midnight, through the same formula.
    ///
    /// @param year - the year
    /// @param month - the month
    /// @param day - the day
    fn midnight(year: i64, month: i64, day: i64) -> i64 {
        let mut value = date(year, month, day);
        value.compute_jd();
        value.jd
    }

    /// A year moved from a spelled 30 February keeps the 30th, which then carries.
    #[test]
    fn a_year_added_to_a_spelled_day_keeps_the_day() {
        assert_eq!(
            resolved(date(2024, 2, 30), &["+1 year"]),
            Some(midnight(2025, 3, 2))
        );
        assert_eq!(
            resolved(date(2024, 2, 30), &["+1 month"]),
            Some(midnight(2024, 3, 30))
        );
        assert_eq!(
            resolved(date(2024, 2, 30), &["start of month"]),
            Some(midnight(2024, 2, 1))
        );
        assert_eq!(
            resolved(date(2024, 2, 30), &["+1 year", "floor"]),
            Some(midnight(2025, 2, 28))
        );
    }

    /// A raw number outside the Julian day range has no date, and a numeric
    /// modifier applies to 2000-01-01 with the error kept.
    #[test]
    fn an_unexplained_number_is_an_error_a_shift_keeps() {
        assert_eq!(resolved(DateTime::raw(1e12), &["+1 hour"]), None);
        let mut value = DateTime::raw(1e12);
        assert!(value.apply(b"+1 hour", 1));
        assert!(value.is_error);
        value.compute_jd();
        assert_eq!(value.jd, midnight(2000, 1, 1) + 3_600_000);
        assert_eq!(resolved(DateTime::raw(1e12), &["start of day"]), None);
    }

    /// A Julian day before the first one is refused by a modifier that needs
    /// the calendar fields, and not by one that only adds to it.
    #[test]
    fn a_negative_day_has_no_weekday() {
        let noon = DateTime::raw(0.0);
        assert_eq!(resolved(noon, &["-90 minutes", "weekday 3"]), None);
        assert_eq!(
            resolved(noon, &["-90 minutes", "+1 day"]),
            Some(DAY_MS - 5_400_000)
        );
    }

    /// `julianday`, `unixepoch` and `auto` are accepted only first, and only
    /// on a raw number.
    #[test]
    fn the_number_modifiers_need_a_raw_number_in_first_place() {
        assert_eq!(
            resolved(DateTime::raw(0.0), &["unixepoch"]),
            Some(210_866_760_000_000)
        );
        assert_eq!(resolved(DateTime::raw(0.0), &["+1 day", "unixepoch"]), None);
        assert_eq!(resolved(date(2024, 1, 1), &["julianday"]), None);
        // Inside the Julian day range a number stays a Julian day; past it,
        // `auto` reads Unix seconds.
        assert_eq!(
            resolved(DateTime::raw(86_400.0), &["auto"]),
            Some(86_400 * DAY_MS)
        );
        assert_eq!(
            resolved(DateTime::raw(10_000_000.0), &["auto"]),
            Some(210_866_760_000_000 + 10_000_000_000)
        );
    }
}

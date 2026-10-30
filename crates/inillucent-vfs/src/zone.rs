//! The machine's local time zone, for the `utc` and `localtime` modifiers.
//!
//! Invariant: **this is a question only the operating system can answer, so it
//! is asked here.** The offset between local time and UTC is not a constant -
//! it changes at a daylight saving boundary and it has changed by legislation -
//! so it is asked for one instant at a time rather than read once and kept. The
//! date and time built-ins in `inillucent-scalar` call this; the module doc
//! there records why the two modifiers used to answer NULL instead.
//!
//! Neither implementation reads a time zone database of its own. On Unix it is
//! `localtime_r`, which is what SQLite's `date.c` calls; on Windows it is
//! `SystemTimeToTzSpecificLocalTime` against the zone the process is running
//! in, which is what the C runtime's `localtime` calls underneath.

/// Returns the local zone's offset from UTC at one instant, in seconds east.
///
/// A positive answer means local time is ahead of UTC. `None` means the
/// operating system would not answer - an instant outside the range its
/// conversion accepts, or a machine with no zone configured - and the caller
/// answers NULL rather than guessing an offset.
///
/// @param utc_seconds - the instant, in seconds since 1970-01-01 00:00 UTC
#[cfg(unix)]
pub fn local_offset_seconds(utc_seconds: i64) -> Option<i64> {
    let instant = utc_seconds as libc::time_t;
    // SAFETY: `tm` is plain data with no invalid bit patterns, so an all-zero
    // value is a valid one; every field read below is written by the call that
    // follows, and the return value is checked before any of them is read.
    let mut broken: libc::tm = unsafe { core::mem::zeroed() };
    // SAFETY: `localtime_r` writes into a `tm` this call owns and reads one
    // `time_t` by pointer. It is the reentrant form on purpose: the shared
    // buffer `localtime` returns is not safe to hand back from a connection
    // that may be one of several in a process.
    let answered = unsafe { libc::localtime_r(&instant, &mut broken) };
    if answered.is_null() {
        return None;
    }
    Some(broken.tm_gmtoff as i64)
}

/// One daylight saving transition as Windows describes it: a month, and a day within it.
///
/// `year` is zero for the usual form, where `day` counts weekdays in the month: 1 is the first
/// `weekday` of the month and 5 is the last. A nonzero `year` is an absolute date, and `day` is the
/// day of the month.
// Compiled on every platform so its tests run everywhere; only Windows builds one from the system.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
struct Transition {
    year: i64,
    month: i64,
    weekday: i64,
    day: i64,
    second_of_day: i64,
}

/// A zone's current rules, which are all the C runtime's `localtime_s` knows.
///
/// The biases are in minutes, with the sign Windows gives them: UTC is local time plus the bias.
/// `start` is when daylight time begins, written in standard time, and `end` is when it ends,
/// written in daylight time. A zone with no daylight saving has no `start`.
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
struct ZoneRules {
    bias: i64,
    standard_bias: i64,
    daylight_bias: i64,
    start: Option<Transition>,
    end: Transition,
}

/// Returns the number of days from 1970-01-01 to a civil date, for any year.
///
/// @param year - the year
/// @param month - 1 to 12
/// @param day - the day of the month
#[cfg_attr(not(windows), allow(dead_code))]
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year.rem_euclid(400);
    let shifted_month = (month + 9) % 12;
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// Returns the year a count of days since 1970-01-01 falls in.
///
/// @param days - days since 1970-01-01
#[cfg_attr(not(windows), allow(dead_code))]
fn year_of_days(days: i64) -> i64 {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    era * 400 + year_of_era + i64::from(month <= 2)
}

/// Returns how many days the month has.
///
/// @param year - the year
/// @param month - 1 to 12
#[cfg_attr(not(windows), allow(dead_code))]
fn days_in_month(year: i64, month: i64) -> i64 {
    days_from_civil(if month == 12 { year + 1 } else { year }, month % 12 + 1, 1)
        - days_from_civil(year, month, 1)
}

/// Returns a transition as seconds since 1970-01-01 on the clock it is written in, for one year.
///
/// @param transition - the rule
/// @param year - the year to apply it to
#[cfg_attr(not(windows), allow(dead_code))]
fn transition_seconds(transition: &Transition, year: i64) -> i64 {
    let first = days_from_civil(year, transition.month, 1);
    let day = if transition.year != 0 {
        transition.day
    } else {
        // 1970-01-01 was a Thursday, which is weekday 4 counting Sunday as 0.
        let first_weekday = (first + 4).rem_euclid(7);
        let mut day =
            1 + (transition.weekday - first_weekday).rem_euclid(7) + (transition.day - 1) * 7;
        while day > days_in_month(year, transition.month) {
            day -= 7;
        }
        day
    };
    (first + day - 1) * 86_400 + transition.second_of_day
}

/// Returns the offset a zone's current rules give one instant, in seconds east.
///
/// **One set of rules is applied to every year, which is what the C runtime on Windows does and
/// what the pinned SQLite for Windows therefore does.** `localtime_s` reads the zone once and
/// asks, for any year, whether the date falls between the start and the end the zone has now. It
/// does not know that the United States began daylight time on the first Sunday of April before
/// 2007, so `datetime('2000-03-15 12:00:00', 'localtime')` is an hour ahead of what the zone's
/// own history says. The comparison is made in standard time, as the C runtime makes it, so the
/// end, which is written in daylight time, is moved back by the daylight saving shift.
///
/// @param rules - the zone's current rules
/// @param utc_seconds - the instant, in seconds since 1970-01-01 00:00 UTC
#[cfg_attr(not(windows), allow(dead_code))]
fn offset_under_rules(rules: &ZoneRules, utc_seconds: i64) -> i64 {
    let standard_offset = -(rules.bias + rules.standard_bias) * 60;
    let daylight_offset = -(rules.bias + rules.daylight_bias) * 60;
    let Some(start) = rules.start else {
        return standard_offset;
    };
    let standard_clock = utc_seconds + standard_offset;
    let year = year_of_days(standard_clock.div_euclid(86_400));
    let begins = transition_seconds(&start, year);
    let ends = transition_seconds(&rules.end, year) - (daylight_offset - standard_offset);
    let in_daylight_time = if begins < ends {
        standard_clock >= begins && standard_clock < ends
    } else {
        standard_clock >= begins || standard_clock < ends
    };
    if in_daylight_time {
        daylight_offset
    } else {
        standard_offset
    }
}

/// Reads one `SYSTEMTIME` of a zone description as a transition, or `None` when it is unset.
///
/// @param time - the start or the end from `TIME_ZONE_INFORMATION`
#[cfg(windows)]
fn transition_of(time: &windows_sys::Win32::Foundation::SYSTEMTIME) -> Option<Transition> {
    if time.wMonth == 0 {
        return None;
    }
    Some(Transition {
        year: i64::from(time.wYear),
        month: i64::from(time.wMonth),
        weekday: i64::from(time.wDayOfWeek),
        day: i64::from(time.wDay),
        second_of_day: i64::from(time.wHour) * 3600
            + i64::from(time.wMinute) * 60
            + i64::from(time.wSecond),
    })
}

/// Reads the rules of the zone the process runs in.
///
/// `None` when Windows would not describe the zone.
#[cfg(windows)]
fn current_rules() -> Option<ZoneRules> {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    // SAFETY: `TIME_ZONE_INFORMATION` is plain data with no invalid bit patterns, so an all-zero
    // value is a valid one, and the call fills it before anything reads it.
    let mut zone: TIME_ZONE_INFORMATION = unsafe { core::mem::zeroed() };
    // SAFETY: the pointer is to a local of the right type, and the return value is checked.
    let answered = unsafe { GetTimeZoneInformation(&mut zone) };
    if answered == u32::MAX {
        return None;
    }
    let start = transition_of(&zone.DaylightDate);
    let end = transition_of(&zone.StandardDate);
    Some(ZoneRules {
        bias: i64::from(zone.Bias),
        standard_bias: i64::from(zone.StandardBias),
        daylight_bias: i64::from(zone.DaylightBias),
        start: start.filter(|_| end.is_some()),
        end: end.unwrap_or(Transition {
            year: 0,
            month: 1,
            weekday: 0,
            day: 1,
            second_of_day: 0,
        }),
    })
}

/// Returns the local zone's offset from UTC at one instant, in seconds east.
///
/// The offset comes from the zone's current rules applied to the instant's year, because that is
/// what the C runtime's `localtime_s` does and the pinned SQLite for Windows uses it. Asking
/// Windows for the zone's history (`SystemTimeToTzSpecificLocalTime`) is more accurate and is
/// not what SQLite prints: it gave 2000 the rule that year had, and any year past 2037 an
/// answer the C runtime does not give.
///
/// @param utc_seconds - the instant, in seconds since 1970-01-01 00:00 UTC
#[cfg(windows)]
pub fn local_offset_seconds(utc_seconds: i64) -> Option<i64> {
    // `localtime_s` refuses a time before 1970 and one past the year 3000.
    if !(0..=32_535_215_999).contains(&utc_seconds) {
        return None;
    }
    Some(offset_under_rules(&current_rules()?, utc_seconds))
}

/// Returns `None`, because this target has no zone lookup.
///
/// @param utc_seconds - the instant, in seconds since 1970-01-01 00:00 UTC
#[cfg(not(any(unix, windows)))]
pub fn local_offset_seconds(utc_seconds: i64) -> Option<i64> {
    let _ = utc_seconds;
    None
}

#[cfg(test)]
mod tests {
    use super::local_offset_seconds;

    /// The offset is a whole number of minutes, and one a zone can hold.
    ///
    /// The value itself is the machine's and cannot be asserted, so what is
    /// asserted is the shape every real zone has: between UTC-12 and UTC+14,
    /// and a whole minute. A stub that answered a constant would pass this and
    /// a stub that answered nonsense would not.
    #[test]
    fn the_offset_is_a_whole_minute_inside_the_range_zones_use() {
        let Some(offset) = local_offset_seconds(1_577_880_000) else {
            inillucent_base::testing::skipping(
                "this machine's operating system would not answer a local offset",
            );
            return;
        };
        assert_eq!(offset % 60, 0, "the offset is {offset} seconds");
        assert!(
            (-12 * 3600..=14 * 3600).contains(&offset),
            "the offset is {offset} seconds"
        );
    }

    /// Two instants six months apart differ by at most one hour.
    ///
    /// A zone that observes daylight saving answers two different offsets for
    /// January and July, and one that does not answers the same twice. Either
    /// is right; what would be wrong is a lookup that ignores its argument and
    /// a difference larger than any daylight saving rule uses.
    #[test]
    fn a_summer_instant_and_a_winter_one_differ_by_at_most_an_hour() {
        let (Some(winter), Some(summer)) = (
            local_offset_seconds(1_577_880_000),
            local_offset_seconds(1_593_604_800),
        ) else {
            inillucent_base::testing::skipping(
                "this machine's operating system would not answer a local offset",
            );
            return;
        };
        assert!(
            (winter - summer).abs() <= 3600,
            "winter {winter} and summer {summer} are further apart than a daylight saving rule"
        );
    }

    /// United States Pacific rules, as `GetTimeZoneInformation` reports them today.
    fn pacific() -> super::ZoneRules {
        let rule = |month, second_of_day| super::Transition {
            year: 0,
            month,
            weekday: 0,
            day: if month == 3 { 2 } else { 1 },
            second_of_day,
        };
        super::ZoneRules {
            bias: 480,
            standard_bias: 0,
            daylight_bias: -60,
            start: Some(rule(3, 7200)),
            end: rule(11, 7200),
        }
    }

    /// The current rules give the same dates to every year, and the boundaries are exact.
    ///
    /// 2000-03-15 is inside daylight time under the rule in force since 2007 although the rule of
    /// 2000 began it on the first Sunday of April. The second Sunday of March 2024 was the 10th,
    /// and the first Sunday of November was the 3rd.
    #[test]
    fn one_set_of_rules_applies_to_every_year_and_boundaries_are_exact() {
        let rules = pacific();
        let at =
            |year, month, day, seconds| super::days_from_civil(year, month, day) * 86_400 + seconds;
        let offset = |instant| super::offset_under_rules(&rules, instant);
        assert_eq!(offset(at(2000, 3, 15, 43_200)), -7 * 3600);
        assert_eq!(offset(at(2000, 1, 15, 43_200)), -8 * 3600);
        assert_eq!(offset(at(2038, 3, 15, 43_200)), -7 * 3600);
        // 02:00 standard time on 2024-03-10 is 10:00 UTC, and the second before it is standard.
        assert_eq!(offset(at(2024, 3, 10, 36_000 - 1)), -8 * 3600);
        assert_eq!(offset(at(2024, 3, 10, 36_000)), -7 * 3600);
        // 02:00 daylight time on 2024-11-03 is 09:00 UTC.
        assert_eq!(offset(at(2024, 11, 3, 32_400 - 1)), -7 * 3600);
        assert_eq!(offset(at(2024, 11, 3, 32_400)), -8 * 3600);
    }

    /// On Windows the zone's current rules apply to every year, as the C runtime applies them.
    ///
    /// The pinned SQLite for Windows asks the C runtime's `localtime_s`, which keeps one set of
    /// daylight saving rules and uses them for 1971 as for 2024. 2000 and 2028 start on the same
    /// weekday and are both leap years, so the same instant of the calendar year has the same
    /// offset in both under one set of rules. Asking the zone for its history instead gives
    /// 2000 the rule of that year, which changed in 2007 in the United States.
    #[cfg(windows)]
    #[test]
    fn windows_applies_the_current_rules_to_every_year() {
        let start_2000 = 946_684_800i64;
        let start_2028 = 1_830_297_600i64;
        for step in 0..(366 * 4) {
            let into_year = step * 6 * 3600;
            let (Some(then), Some(now)) = (
                local_offset_seconds(start_2000 + into_year),
                local_offset_seconds(start_2028 + into_year),
            ) else {
                inillucent_base::testing::skipping(
                    "this machine's operating system would not answer a local offset",
                );
                return;
            };
            assert_eq!(then, now, "{into_year} seconds into the year");
        }
    }
}

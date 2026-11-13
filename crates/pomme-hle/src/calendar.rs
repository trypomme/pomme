//! NSCalendar, NSDateComponents and NSTimeZone.
//!
//! Calendars are Gregorian (the only one pomme models; asked for another, you
//! get Gregorian arithmetic under that identifier), with the device's settings:
//! en_US (weeks start on Sunday, and the week holding 1 January is week 1) and
//! the device's time zone, GMT, as the C library's `localtime_r` also has it.
//!
//! Time zones are fixed offsets ("GMT+0530") or the IANA database's
//! ("Australia/Sydney", with its daylight saving), from chrono-tz.
//!
//! Arithmetic follows Foundation: dates split into components in the
//! calendar's time zone; components put together leniently (month 13 is next
//! January, day 0 the last of the month before); adding months keeps the day
//! where it can (31 January + 1 month is 28 or 29 February); days and weeks
//! keep the wall-clock time across daylight saving changes, and hours,
//! minutes and seconds are elapsed time.

use crate::foundation::{self, date_value, new_date, nsstring};
use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result};
use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Offset, TimeZone, Timelike};
use chrono_tz::{OffsetComponents, OffsetName, Tz};
use std::collections::HashMap;

/// NSDateComponentUndefined (NSIntegerMax).
pub const UNDEFINED: i64 = i64::MAX;

/// Seconds from 1970 to 2001 (NSDate's reference date).
const REFERENCE_1970: i64 = 978_307_200;

// NSCalendarUnit
const ERA: u64 = 1 << 1;
const YEAR: u64 = 1 << 2;
const MONTH: u64 = 1 << 3;
const DAY: u64 = 1 << 4;
const HOUR: u64 = 1 << 5;
const MINUTE: u64 = 1 << 6;
const SECOND: u64 = 1 << 7;
const WEEKDAY: u64 = 1 << 9;
const WEEKDAY_ORDINAL: u64 = 1 << 10;
const QUARTER: u64 = 1 << 11;
const WEEK_OF_MONTH: u64 = 1 << 12;
const WEEK_OF_YEAR: u64 = 1 << 13;
const YEAR_FOR_WEEK_OF_YEAR: u64 = 1 << 14;
const NANOSECOND: u64 = 1 << 15;
const CALENDAR: u64 = 1 << 20;
const TIME_ZONE: u64 = 1 << 21;

/// The device's time zone.
const DEVICE_ZONE: &str = "GMT";

#[derive(Clone, Debug, PartialEq)]
pub enum Zone {
    /// A fixed offset from GMT, in seconds.
    Fixed(i32),
    Named(Tz),
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimeZoneInfo {
    pub name: String,
    pub zone: Zone,
}

impl TimeZoneInfo {
    fn fixed(seconds: i32) -> Self {
        Self { name: fixed_name(seconds), zone: Zone::Fixed(seconds) }
    }

    /// The zone called `name`, as `+[NSTimeZone timeZoneWithName:]` finds it.
    pub fn named(name: &str) -> Option<Self> {
        if name == "GMT" || name == "UTC" || name == "Etc/GMT" || name == "Etc/UTC" {
            return Some(Self { name: name.into(), zone: Zone::Fixed(0) });
        }
        if let Some(s) = parse_fixed(name) {
            return Some(Self::fixed(s));
        }
        let tz: Tz = name.parse().ok()?;
        Some(Self { name: tz.name().into(), zone: Zone::Named(tz) })
    }

    /// Seconds from GMT at `t` (seconds since 1970).
    pub fn offset_at(&self, t: i64) -> i32 {
        match &self.zone {
            Zone::Fixed(s) => *s,
            Zone::Named(tz) => tz.offset_from_utc_datetime(&utc(t)).fix().local_minus_utc(),
        }
    }

    fn dst_at(&self, t: i64) -> i64 {
        match &self.zone {
            Zone::Fixed(_) => 0,
            Zone::Named(tz) => tz.offset_from_utc_datetime(&utc(t)).dst_offset().num_seconds(),
        }
    }

    fn abbreviation_at(&self, t: i64) -> String {
        match &self.zone {
            Zone::Fixed(s) => fixed_abbreviation(*s),
            Zone::Named(tz) => {
                let off = tz.offset_from_utc_datetime(&utc(t));
                match off.abbreviation() {
                    // Zones without letters ("+10") are written as GMT offsets, as Foundation does.
                    Some(a) if a.starts_with(|c: char| c.is_ascii_alphabetic()) => a.to_string(),
                    _ => fixed_abbreviation(off.fix().local_minus_utc()),
                }
            }
        }
    }

    /// The wall-clock time at `t` (seconds since 1970) and nanoseconds.
    fn local(&self, t: i64, nanos: u32) -> NaiveDateTime {
        let offset = self.offset_at(t) as i64;
        utc(t + offset).with_nanosecond(nanos).unwrap_or_else(|| utc(t + offset))
    }

    /// Seconds since 1970 for wall-clock time `local`: in a daylight saving gap,
    /// the time as if the clocks hadn't changed; when it happens twice, the first.
    fn instant(&self, local: NaiveDateTime) -> i64 {
        let naive = local.and_utc().timestamp();
        match &self.zone {
            Zone::Fixed(s) => naive - *s as i64,
            Zone::Named(tz) => match tz.from_local_datetime(&local) {
                chrono::LocalResult::Single(d) => d.timestamp(),
                chrono::LocalResult::Ambiguous(a, _) => a.timestamp(),
                chrono::LocalResult::None => {
                    let before = tz.offset_from_utc_datetime(&(local - Duration::hours(12))).fix().local_minus_utc();
                    naive - before as i64
                }
            },
        }
    }
}

/// "GMT", "GMT+0530", "GMT-0800": a fixed zone's name.
fn fixed_name(seconds: i32) -> String {
    if seconds == 0 {
        return "GMT".into();
    }
    let a = seconds.unsigned_abs();
    format!("GMT{}{:02}{:02}", if seconds < 0 { '-' } else { '+' }, a / 3600, a / 60 % 60)
}

/// "GMT", "GMT+5:30", "GMT-8": a fixed offset's abbreviation.
fn fixed_abbreviation(seconds: i32) -> String {
    if seconds == 0 {
        return "GMT".into();
    }
    let a = seconds.unsigned_abs();
    let sign = if seconds < 0 { '-' } else { '+' };
    if a.is_multiple_of(3600) { format!("GMT{sign}{}", a / 3600) } else { format!("GMT{sign}{}:{:02}", a / 3600, a / 60 % 60) }
}

/// "GMT+0530", "GMT-8", "UTC+10:00", "+0530": a fixed offset in seconds.
fn parse_fixed(name: &str) -> Option<i32> {
    let rest = name.strip_prefix("GMT").or_else(|| name.strip_prefix("UTC")).unwrap_or(name);
    let (sign, digits) = match rest.as_bytes().first()? {
        b'+' => (1, &rest[1..]),
        b'-' => (-1, &rest[1..]),
        _ => return None,
    };
    let (h, m) = match digits.split_once(':') {
        Some((h, m)) => (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?),
        None if digits.len() > 2 => (digits[..digits.len() - 2].parse().ok()?, digits[digits.len() - 2..].parse().ok()?),
        None => (digits.parse().ok()?, 0),
    };
    (h <= 18 && m < 60).then_some(sign * (h * 3600 + m * 60))
}

fn utc(t: i64) -> NaiveDateTime {
    chrono::DateTime::from_timestamp(t, 0).map_or(NaiveDateTime::MIN, |d| d.naive_utc())
}

/// NSDateComponents' fields, in NSCalendarUnit order.
#[derive(Clone, Debug, PartialEq)]
pub struct Components {
    pub era: i64,
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    pub nanosecond: i64,
    pub weekday: i64,
    pub weekday_ordinal: i64,
    pub quarter: i64,
    pub week_of_month: i64,
    pub week_of_year: i64,
    pub year_for_week_of_year: i64,
    pub leap_month: bool,
    pub calendar: u64,
    pub time_zone: u64,
}

impl Default for Components {
    fn default() -> Self {
        let u = UNDEFINED;
        Self {
            era: u,
            year: u,
            month: u,
            day: u,
            hour: u,
            minute: u,
            second: u,
            nanosecond: u,
            weekday: u,
            weekday_ordinal: u,
            quarter: u,
            week_of_month: u,
            week_of_year: u,
            year_for_week_of_year: u,
            leap_month: false,
            calendar: 0,
            time_zone: 0,
        }
    }
}

impl Components {
    fn field(&mut self, unit: u64) -> Option<&mut i64> {
        Some(match unit {
            ERA => &mut self.era,
            YEAR => &mut self.year,
            MONTH => &mut self.month,
            DAY => &mut self.day,
            HOUR => &mut self.hour,
            MINUTE => &mut self.minute,
            SECOND => &mut self.second,
            NANOSECOND => &mut self.nanosecond,
            WEEKDAY => &mut self.weekday,
            WEEKDAY_ORDINAL => &mut self.weekday_ordinal,
            QUARTER => &mut self.quarter,
            WEEK_OF_MONTH => &mut self.week_of_month,
            WEEK_OF_YEAR => &mut self.week_of_year,
            YEAR_FOR_WEEK_OF_YEAR => &mut self.year_for_week_of_year,
            _ => return None,
        })
    }
}

/// A Gregorian calendar's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Gregorian {
    pub identifier: String,
    pub zone: TimeZoneInfo,
    pub first_weekday: i64,
    pub minimum_days_in_first_week: i64,
}

impl Gregorian {
    pub fn device() -> Self {
        Self {
            identifier: "gregorian".into(),
            zone: TimeZoneInfo::named(DEVICE_ZONE).expect("GMT"),
            first_weekday: 1,
            minimum_days_in_first_week: 1,
        }
    }

    /// 1 (Sunday) … 7 (Saturday).
    fn weekday(d: NaiveDate) -> i64 {
        d.weekday().num_days_from_sunday() as i64 + 1
    }

    /// The first day of week 1 of the period starting on `start`.
    fn first_week_start(&self, start: NaiveDate) -> NaiveDate {
        let back = (Self::weekday(start) - self.first_weekday).rem_euclid(7);
        if 7 - back >= self.minimum_days_in_first_week { start - Duration::days(back) } else { start + Duration::days(7 - back) }
    }

    /// (week of year, the year it counts towards).
    fn week_of_year(&self, d: NaiveDate) -> (i64, i64) {
        let y = d.year();
        let start = |y: i32| self.first_week_start(NaiveDate::from_ymd_opt(y, 1, 1).unwrap_or(d));
        let year = if d < start(y) {
            y - 1
        } else if d >= start(y + 1) {
            y + 1
        } else {
            y
        };
        ((d - start(year)).num_days() / 7 + 1, year as i64)
    }

    fn week_of_month(&self, d: NaiveDate) -> i64 {
        let first = d.with_day(1).unwrap_or(d);
        (d - self.first_week_start(first)).num_days().div_euclid(7) + 1
    }

    /// `components:fromDate:`: `t` (seconds since 1970, with nanoseconds) split up.
    pub fn split(&self, units: u64, t: i64, nanos: u32) -> Components {
        let l = self.zone.local(t, nanos);
        let d = l.date();
        let mut c = Components::default();
        let year = d.year() as i64;
        let set = |unit: u64, v: i64, slot: &mut i64| {
            if units & unit != 0 {
                *slot = v;
            }
        };
        set(ERA, (year > 0) as i64, &mut c.era);
        set(YEAR, if year > 0 { year } else { 1 - year }, &mut c.year);
        set(MONTH, d.month() as i64, &mut c.month);
        set(DAY, d.day() as i64, &mut c.day);
        set(HOUR, l.hour() as i64, &mut c.hour);
        set(MINUTE, l.minute() as i64, &mut c.minute);
        set(SECOND, l.second() as i64, &mut c.second);
        set(NANOSECOND, l.nanosecond() as i64, &mut c.nanosecond);
        set(WEEKDAY, Self::weekday(d), &mut c.weekday);
        set(WEEKDAY_ORDINAL, (d.day() as i64 - 1) / 7 + 1, &mut c.weekday_ordinal);
        set(QUARTER, (d.month() as i64 - 1) / 3 + 1, &mut c.quarter);
        set(WEEK_OF_MONTH, self.week_of_month(d), &mut c.week_of_month);
        let (week, week_year) = self.week_of_year(d);
        set(WEEK_OF_YEAR, week, &mut c.week_of_year);
        set(YEAR_FOR_WEEK_OF_YEAR, week_year, &mut c.year_for_week_of_year);
        c
    }

    /// `dateFromComponents:`: seconds since 1970 and nanoseconds, or None
    /// when the date is out of range. Undefined fields count from 1 January
    /// of year 1 at midnight; fields past their range carry over.
    pub fn join(&self, c: &Components, zone: Option<&TimeZoneInfo>) -> Option<(i64, u32)> {
        let def = |v: i64, or: i64| if v == UNDEFINED { or } else { v };
        let date = if c.year == UNDEFINED && c.year_for_week_of_year != UNDEFINED && c.week_of_year != UNDEFINED {
            // A week of a week-numbering year, and a day in it.
            let jan1 = NaiveDate::from_ymd_opt(i32::try_from(c.year_for_week_of_year).ok()?, 1, 1)?;
            let start = self.first_week_start(jan1) + Duration::weeks(c.week_of_year - 1);
            start + Duration::days((def(c.weekday, self.first_weekday) - self.first_weekday).rem_euclid(7))
        } else {
            let mut year = def(c.year, 1);
            if c.era == 0 {
                year = 1 - year;
            }
            let month0 = def(c.month, 1) - 1;
            let (y, m) = (year.checked_add(month0.div_euclid(12))?, month0.rem_euclid(12) + 1);
            let first = NaiveDate::from_ymd_opt(i32::try_from(y).ok()?, m as u32, 1)?;
            if c.day == UNDEFINED && c.weekday != UNDEFINED && c.weekday_ordinal != UNDEFINED {
                // The nth such weekday of the month (negative counts from the end).
                if c.weekday_ordinal > 0 {
                    let to = (c.weekday - Self::weekday(first)).rem_euclid(7);
                    first + Duration::days(to + (c.weekday_ordinal - 1) * 7)
                } else {
                    let last = first.checked_add_months(chrono::Months::new(1))? - Duration::days(1);
                    let back = (Self::weekday(last) - c.weekday).rem_euclid(7);
                    last - Duration::days(back + (-c.weekday_ordinal - 1) * 7)
                }
            } else {
                first.checked_add_signed(Duration::try_days(def(c.day, 1) - 1)?)?
            }
        };
        let secs = def(c.hour, 0).checked_mul(3600)?.checked_add(def(c.minute, 0).checked_mul(60)?)?.checked_add(def(c.second, 0))?;
        let nanos = def(c.nanosecond, 0);
        let local = date.and_hms_opt(0, 0, 0)?;
        let zone = zone.unwrap_or(&self.zone);
        let t = zone.instant(local).checked_add(secs)?.checked_add(nanos.div_euclid(1_000_000_000))?;
        Some((t, nanos.rem_euclid(1_000_000_000) as u32))
    }

    /// `dateByAddingComponents:toDate:options:`.
    pub fn add(&self, c: &Components, t: i64, nanos: u32) -> Option<(i64, u32)> {
        let v = |x: i64| if x == UNDEFINED { 0 } else { x };
        let local = self.zone.local(t, 0);
        let months = v(c.year).checked_mul(12)?.checked_add(v(c.month))?.checked_add(v(c.quarter).checked_mul(3)?)?;
        let mut date = local.date();
        date = if months >= 0 {
            date.checked_add_months(chrono::Months::new(u32::try_from(months).ok()?))?
        } else {
            date.checked_sub_months(chrono::Months::new(u32::try_from(-months).ok()?))?
        };
        let weeks = v(c.week_of_year) + v(c.week_of_month);
        let days = v(c.day).checked_add(v(c.weekday))?.checked_add(weeks.checked_mul(7)?)?;
        date = date.checked_add_signed(Duration::try_days(days)?)?;
        let wall = date.and_time(local.time());
        let secs = v(c.hour).checked_mul(3600)?.checked_add(v(c.minute).checked_mul(60)?)?.checked_add(v(c.second))?;
        let total_nanos = nanos as i64 + v(c.nanosecond);
        let t = self.zone.instant(wall).checked_add(secs)?.checked_add(total_nanos.div_euclid(1_000_000_000))?;
        Some((t, total_nanos.rem_euclid(1_000_000_000) as u32))
    }

    /// `components:fromDate:toDate:options:`: the difference in `units`, the
    /// largest first, each as many whole units as fit.
    pub fn difference(&self, units: u64, from: (i64, u32), to: (i64, u32)) -> Components {
        let mut out = Components::default();
        let mut at = from;
        let key = |p: (i64, u32)| p.0 as i128 * 1_000_000_000 + p.1 as i128;
        let forward = key(to) >= key(from);
        for unit in [ERA, YEAR, QUARTER, MONTH, WEEK_OF_YEAR, WEEK_OF_MONTH, DAY, HOUR, MINUTE, SECOND, NANOSECOND] {
            if units & unit == 0 {
                continue;
            }
            let base = at;
            let step = |n: i64| {
                let mut c = Components::default();
                match unit {
                    ERA => {}
                    _ => *c.field(unit).expect("a unit") = n,
                }
                self.add(&c, base.0, base.1)
            };
            if unit == ERA {
                out.era = 0;
                continue;
            }
            // Estimate from the elapsed time, then settle on the most that doesn't pass `to`.
            let approx: f64 = match unit {
                YEAR => 365.2425 * 86400.0,
                QUARTER => 91.31 * 86400.0,
                MONTH => 30.44 * 86400.0,
                WEEK_OF_YEAR | WEEK_OF_MONTH => 7.0 * 86400.0,
                DAY => 86400.0,
                HOUR => 3600.0,
                MINUTE => 60.0,
                SECOND => 1.0,
                _ => 1e-9,
            };
            let gap = (key(to) - key(at)) as f64 / 1e9;
            let mut n = (gap / approx) as i64;
            let past = |p: Option<(i64, u32)>| p.is_none_or(|p| if forward { key(p) > key(to) } else { key(p) < key(to) });
            let dir = if forward { 1 } else { -1 };
            while n != 0 && past(step(n)) {
                n -= dir;
            }
            while !past(step(n + dir)) {
                n += dir;
            }
            if let Some(p) = step(n) {
                at = p;
            }
            if let Some(f) = out.field(unit) {
                *f = n;
            }
        }
        out
    }

    /// `rangeOfUnit:inUnit:forDate:`: (first, count).
    pub fn range(&self, smaller: u64, larger: u64, t: i64) -> Option<(i64, i64)> {
        let d = self.zone.local(t, 0).date();
        let days_in_month = |d: NaiveDate| {
            let first = d.with_day(1)?;
            Some((first.checked_add_months(chrono::Months::new(1))? - first).num_days())
        };
        let days_in_year = |d: NaiveDate| if d.leap_year() { 366 } else { 365 };
        Some(match (smaller, larger) {
            (DAY, MONTH) => (1, days_in_month(d)?),
            (DAY, YEAR) => (1, days_in_year(d)),
            (MONTH, YEAR) => (1, 12),
            (HOUR, DAY) => (0, 24),
            (MINUTE, HOUR) => (0, 60),
            (SECOND, MINUTE) => (0, 60),
            (WEEKDAY, WEEK_OF_YEAR | WEEK_OF_MONTH) => (1, 7),
            (WEEK_OF_MONTH, MONTH) => {
                let last = d.with_day(days_in_month(d)? as u32)?;
                let first = self.week_of_month(d.with_day(1)?);
                (first, self.week_of_month(last) - first + 1)
            }
            (WEEK_OF_YEAR, YEAR) => (1, self.week_of_year(NaiveDate::from_ymd_opt(d.year(), 12, 31)?).0.max(52)),
            (QUARTER, YEAR) => (1, 4),
            _ => return None,
        })
    }

    /// `ordinalityOfUnit:inUnit:forDate:`.
    pub fn ordinality(&self, smaller: u64, larger: u64, t: i64) -> Option<i64> {
        let l = self.zone.local(t, 0);
        let d = l.date();
        Some(match (smaller, larger) {
            (DAY, MONTH) => d.day() as i64,
            (DAY, YEAR) => d.ordinal() as i64,
            (MONTH, YEAR) => d.month() as i64,
            (HOUR, DAY) => l.hour() as i64 + 1,
            (MINUTE, HOUR) => l.minute() as i64 + 1,
            (SECOND, MINUTE) => l.second() as i64 + 1,
            (WEEKDAY, WEEK_OF_YEAR | WEEK_OF_MONTH) => (Self::weekday(d) - self.first_weekday).rem_euclid(7) + 1,
            (WEEK_OF_MONTH, MONTH) => self.week_of_month(d),
            (WEEK_OF_YEAR, YEAR) => self.week_of_year(d).0,
            (QUARTER, YEAR) => (d.month() as i64 - 1) / 3 + 1,
            (DAY, WEEK_OF_YEAR | WEEK_OF_MONTH) => (Self::weekday(d) - self.first_weekday).rem_euclid(7) + 1,
            _ => return None,
        })
    }

    /// Midnight (or the first moment) of `t`'s day.
    pub fn start_of_day(&self, t: i64) -> i64 {
        let d = self.zone.local(t, 0).date();
        self.zone.instant(d.and_hms_opt(0, 0, 0).expect("midnight"))
    }
}

// ---- the objects ----

#[derive(Default)]
pub struct State {
    calendars: HashMap<u64, Gregorian>,
    components: HashMap<u64, Components>,
    zones: HashMap<u64, TimeZoneInfo>,
    /// `+[NSTimeZone setDefaultTimeZone:]`'s zone.
    default_zone: Option<TimeZoneInfo>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_str(emu: &mut Emu, s: &str) -> Result<()> {
    let v = foundation::new_nsstring(emu, s)?;
    ret(emu, v)
}

fn ret_double(emu: &mut Emu, v: f64) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

/// An NSDate as (seconds since 1970, nanoseconds).
fn instant_of(emu: &mut Emu, date: u64) -> Result<(i64, u32)> {
    let abs = date_value(emu, date)?;
    let secs = abs.floor();
    let nanos = ((abs - secs) * 1e9).round().min(999_999_999.0) as u32;
    Ok((secs as i64 + REFERENCE_1970, nanos))
}

fn date_of(emu: &mut Emu, (t, nanos): (i64, u32)) -> Result<u64> {
    new_date(emu, (t - REFERENCE_1970) as f64 + nanos as f64 / 1e9)
}

fn ret_date(emu: &mut Emu, at: Option<(i64, u32)>) -> Result<()> {
    let d = match at {
        Some(at) => date_of(emu, at)?,
        None => 0,
    };
    ret(emu, d)
}

fn new_zone(emu: &mut Emu, info: TimeZoneInfo) -> Result<u64> {
    let cls = host_class(emu, "NSTimeZone")?;
    let z = objc::create_instance(emu, cls, 0)?;
    emu.calendars.zones.insert(z, info);
    Ok(z)
}

/// The zone an NSTimeZone object stands for; 0 is the default (device) zone.
pub(crate) fn zone_info(emu: &Emu, z: u64) -> TimeZoneInfo {
    if z == 0 {
        return emu.calendars.default_zone.clone().unwrap_or_else(|| TimeZoneInfo::named(DEVICE_ZONE).expect("GMT"));
    }
    zone_of(emu, z)
}

fn zone_of(emu: &Emu, z: u64) -> TimeZoneInfo {
    emu.calendars.zones.get(&z).cloned().unwrap_or_else(|| TimeZoneInfo::named(DEVICE_ZONE).expect("GMT"))
}

fn new_calendar(emu: &mut Emu, cal: Gregorian) -> Result<u64> {
    let cls = host_class(emu, "NSCalendar")?;
    let c = objc::create_instance(emu, cls, 0)?;
    emu.calendars.calendars.insert(c, cal);
    Ok(c)
}

fn device_calendar(emu: &Emu) -> Gregorian {
    let mut cal = Gregorian::device();
    if let Some(z) = &emu.calendars.default_zone {
        cal.zone = z.clone();
    }
    cal
}

fn cal_of(emu: &Emu, c: u64) -> Gregorian {
    emu.calendars.calendars.get(&c).cloned().unwrap_or_else(|| device_calendar(emu))
}

fn new_components(emu: &mut Emu, c: Components) -> Result<u64> {
    let cls = host_class(emu, "NSDateComponents")?;
    let o = objc::create_instance(emu, cls, 0)?;
    emu.calendars.components.insert(o, c);
    Ok(o)
}

fn comps_of(emu: &Emu, o: u64) -> Components {
    emu.calendars.components.get(&o).cloned().unwrap_or_default()
}

/// The calendar's `components:fromDate:` result as an NSDateComponents.
fn split_into(emu: &mut Emu, cal_obj: u64, units: u64, date: u64, zone: Option<TimeZoneInfo>) -> Result<u64> {
    let mut cal = cal_of(emu, cal_obj);
    if let Some(z) = zone {
        cal.zone = z;
    }
    let (t, n) = instant_of(emu, date)?;
    let mut c = cal.split(units, t, n);
    if units & CALENDAR != 0 {
        c.calendar = cal_obj;
    }
    if units & TIME_ZONE != 0 {
        c.time_zone = new_zone(emu, cal.zone.clone())?;
    }
    new_components(emu, c)
}

fn same_day(emu: &mut Emu, cal: u64, a: u64, b: (i64, u32)) -> Result<bool> {
    let cal = cal_of(emu, cal);
    let (ta, _) = instant_of(emu, a)?;
    Ok(cal.start_of_day(ta) == cal.start_of_day(b.0))
}

fn now(emu: &mut Emu) -> Result<(i64, u32)> {
    let cls = host_class(emu, "NSDate")?;
    let d = objc::send(emu, cls, "date", &[])?;
    instant_of(emu, d)
}

fn days_from_now(emu: &mut Emu, cal: u64, days: i64) -> Result<(i64, u32)> {
    let g = cal_of(emu, cal);
    let n = now(emu)?;
    let c = Components { day: days, ..Default::default() };
    Ok(g.add(&c, n.0, n.1).unwrap_or(n))
}

const C: &str = "NSCalendar";
const DC: &str = "NSDateComponents";
const TZ: &str = "NSTimeZone";

fn component_getter(unit: u64) -> HostFn {
    macro_rules! get {
        ($($u:ident),*) => {
            match unit {
                $($u => |e: &mut Emu| { let o = e.arg(0); let v = comps_of(e, o).field($u).map_or(UNDEFINED, |v| *v); ret(e, v as u64) },)*
                _ => unreachable!(),
            }
        };
    }
    get!(ERA, YEAR, MONTH, DAY, HOUR, MINUTE, SECOND, NANOSECOND, WEEKDAY, WEEKDAY_ORDINAL, QUARTER, WEEK_OF_MONTH, WEEK_OF_YEAR, YEAR_FOR_WEEK_OF_YEAR)
}

fn component_setter(unit: u64) -> HostFn {
    macro_rules! set {
        ($($u:ident),*) => {
            match unit {
                $($u => |e: &mut Emu| { let (o, v) = (e.arg(0), e.arg(2) as i64); let mut c = comps_of(e, o); if let Some(f) = c.field($u) { *f = v; } e.calendars.components.insert(o, c); Ok(()) },)*
                _ => unreachable!(),
            }
        };
    }
    set!(ERA, YEAR, MONTH, DAY, HOUR, MINUTE, SECOND, NANOSECOND, WEEKDAY, WEEKDAY_ORDINAL, QUARTER, WEEK_OF_MONTH, WEEK_OF_YEAR, YEAR_FOR_WEEK_OF_YEAR)
}

/// NSDateComponents' accessors: (getter, setter, unit).
const FIELDS: &[(&str, &str, u64)] = &[
    ("era", "setEra:", ERA),
    ("year", "setYear:", YEAR),
    ("month", "setMonth:", MONTH),
    ("day", "setDay:", DAY),
    ("hour", "setHour:", HOUR),
    ("minute", "setMinute:", MINUTE),
    ("second", "setSecond:", SECOND),
    ("nanosecond", "setNanosecond:", NANOSECOND),
    ("weekday", "setWeekday:", WEEKDAY),
    ("weekdayOrdinal", "setWeekdayOrdinal:", WEEKDAY_ORDINAL),
    ("quarter", "setQuarter:", QUARTER),
    ("weekOfMonth", "setWeekOfMonth:", WEEK_OF_MONTH),
    ("weekOfYear", "setWeekOfYear:", WEEK_OF_YEAR),
    ("yearForWeekOfYear", "setYearForWeekOfYear:", YEAR_FOR_WEEK_OF_YEAR),
];

/// NSDateComponents' field accessors, made from [`FIELDS`].
pub(crate) fn component_methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    let mut v = Vec::new();
    for &(get, set, unit) in FIELDS {
        v.push((DC, false, get, component_getter(unit)));
        v.push((DC, false, set, component_setter(unit)));
    }
    v
}

fn calendar_with_identifier(emu: &mut Emu) -> Result<()> {
    let id = emu.arg(2);
    if id == 0 {
        return ret(emu, 0);
    }
    let id = nsstring(emu, id)?;
    let mut cal = device_calendar(emu);
    cal.identifier = id;
    let c = new_calendar(emu, cal)?;
    ret(emu, c)
}

fn zone_with_name(emu: &mut Emu) -> Result<()> {
    let name = emu.arg(2);
    let info = if name == 0 { None } else { TimeZoneInfo::named(&nsstring(emu, name)?) };
    match info {
        Some(i) => {
            let z = new_zone(emu, i)?;
            ret(emu, z)
        }
        None => ret(emu, 0),
    }
}

/// US English's common abbreviations, as `timeZoneWithAbbreviation:` knows them.
const ABBREVIATIONS: &[(&str, &str)] = &[
    ("GMT", "GMT"),
    ("UTC", "UTC"),
    ("EST", "America/New_York"),
    ("EDT", "America/New_York"),
    ("CST", "America/Chicago"),
    ("CDT", "America/Chicago"),
    ("MST", "America/Denver"),
    ("MDT", "America/Denver"),
    ("PST", "America/Los_Angeles"),
    ("PDT", "America/Los_Angeles"),
    ("BST", "Europe/London"),
    ("CET", "Europe/Paris"),
    ("CEST", "Europe/Paris"),
    ("JST", "Asia/Tokyo"),
    ("IST", "Asia/Kolkata"),
    ("AEST", "Australia/Sydney"),
    ("AEDT", "Australia/Sydney"),
];

fn device_zone(emu: &mut Emu) -> Result<()> {
    let info = emu.calendars.default_zone.clone().unwrap_or_else(|| TimeZoneInfo::named(DEVICE_ZONE).expect("GMT"));
    let z = new_zone(emu, info)?;
    ret(emu, z)
}

fn t_now_or_arg(emu: &mut Emu, date_arg: Option<usize>) -> Result<i64> {
    match date_arg {
        Some(i) => {
            let d = emu.arg(i);
            Ok(instant_of(emu, d)?.0)
        }
        None => Ok(now(emu)?.0),
    }
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // NSCalendar
    (C, true, "currentCalendar", |e| {
        let cal = device_calendar(e);
        let c = new_calendar(e, cal)?;
        ret(e, c)
    }),
    (C, true, "autoupdatingCurrentCalendar", |e| {
        let cal = device_calendar(e);
        let c = new_calendar(e, cal)?;
        ret(e, c)
    }),
    (C, true, "calendarWithIdentifier:", calendar_with_identifier),
    (C, false, "initWithCalendarIdentifier:", |e| {
        let (c, id) = (e.arg(0), e.arg(2));
        if id == 0 {
            return ret(e, 0);
        }
        let mut cal = device_calendar(e);
        cal.identifier = nsstring(e, id)?;
        e.calendars.calendars.insert(c, cal);
        ret(e, c)
    }),
    (C, false, "calendarIdentifier", |e| {
        let id = cal_of(e, e.arg(0)).identifier;
        ret_str(e, &id)
    }),
    (C, false, "timeZone", |e| {
        let z = cal_of(e, e.arg(0)).zone;
        let z = new_zone(e, z)?;
        ret(e, z)
    }),
    (C, false, "setTimeZone:", |e| {
        let (c, z) = (e.arg(0), e.arg(2));
        let mut cal = cal_of(e, c);
        cal.zone = if z == 0 { device_calendar(e).zone } else { zone_of(e, z) };
        e.calendars.calendars.insert(c, cal);
        Ok(())
    }),
    (C, false, "locale", |e| {
        let cls = host_class(e, "NSLocale")?;
        let l = objc::send(e, cls, "currentLocale", &[])?;
        ret(e, l)
    }),
    (C, false, "setLocale:", |_| Ok(())),
    (C, false, "firstWeekday", |e| {
        let v = cal_of(e, e.arg(0)).first_weekday;
        ret(e, v as u64)
    }),
    (C, false, "setFirstWeekday:", |e| {
        let (c, v) = (e.arg(0), e.arg(2) as i64);
        let mut cal = cal_of(e, c);
        cal.first_weekday = v.clamp(1, 7);
        e.calendars.calendars.insert(c, cal);
        Ok(())
    }),
    (C, false, "minimumDaysInFirstWeek", |e| {
        let v = cal_of(e, e.arg(0)).minimum_days_in_first_week;
        ret(e, v as u64)
    }),
    (C, false, "setMinimumDaysInFirstWeek:", |e| {
        let (c, v) = (e.arg(0), e.arg(2) as i64);
        let mut cal = cal_of(e, c);
        cal.minimum_days_in_first_week = v.clamp(1, 7);
        e.calendars.calendars.insert(c, cal);
        Ok(())
    }),
    (C, false, "components:fromDate:", |e| {
        let (c, units, d) = (e.arg(0), e.arg(2), e.arg(3));
        let o = split_into(e, c, units, d, None)?;
        ret(e, o)
    }),
    (C, false, "componentsInTimeZone:fromDate:", |e| {
        let (c, z, d) = (e.arg(0), e.arg(2), e.arg(3));
        let zone = zone_of(e, z);
        let all = ERA | YEAR | MONTH | DAY | HOUR | MINUTE | SECOND | NANOSECOND | WEEKDAY | WEEKDAY_ORDINAL | QUARTER | WEEK_OF_MONTH | WEEK_OF_YEAR | YEAR_FOR_WEEK_OF_YEAR | CALENDAR | TIME_ZONE;
        let o = split_into(e, c, all, d, Some(zone))?;
        ret(e, o)
    }),
    (C, false, "component:fromDate:", |e| {
        let (c, unit, d) = (e.arg(0), e.arg(2), e.arg(3));
        let cal = cal_of(e, c);
        let (t, n) = instant_of(e, d)?;
        let v = cal.split(unit, t, n).field(unit).map_or(UNDEFINED, |v| *v);
        ret(e, v as u64)
    }),
    (C, false, "dateFromComponents:", |e| {
        let (c, o) = (e.arg(0), e.arg(2));
        let cal = cal_of(e, c);
        let comps = comps_of(e, o);
        let zone = (comps.time_zone != 0).then(|| zone_of(e, comps.time_zone));
        ret_date(e, cal.join(&comps, zone.as_ref()))
    }),
    (C, false, "dateByAddingComponents:toDate:options:", |e| {
        let (c, o, d) = (e.arg(0), e.arg(2), e.arg(3));
        let cal = cal_of(e, c);
        let comps = comps_of(e, o);
        let (t, n) = instant_of(e, d)?;
        ret_date(e, cal.add(&comps, t, n))
    }),
    (C, false, "dateByAddingUnit:value:toDate:options:", |e| {
        let (c, unit, v, d) = (e.arg(0), e.arg(2), e.arg(3) as i64, e.arg(4));
        let cal = cal_of(e, c);
        let mut comps = Components::default();
        let Some(f) = comps.field(unit) else { return ret(e, 0) };
        *f = v;
        let (t, n) = instant_of(e, d)?;
        ret_date(e, cal.add(&comps, t, n))
    }),
    (C, false, "components:fromDate:toDate:options:", |e| {
        let (c, units, a, b) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let cal = cal_of(e, c);
        let (from, to) = (instant_of(e, a)?, instant_of(e, b)?);
        let comps = cal.difference(units, from, to);
        let o = new_components(e, comps)?;
        ret(e, o)
    }),
    (C, false, "components:fromDateComponents:toDateComponents:options:", |e| {
        let (c, units, a, b) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let cal = cal_of(e, c);
        let (ca, cb) = (comps_of(e, a), comps_of(e, b));
        let comps = match (cal.join(&ca, None), cal.join(&cb, None)) {
            (Some(x), Some(y)) => cal.difference(units, x, y),
            _ => Components::default(),
        };
        let o = new_components(e, comps)?;
        ret(e, o)
    }),
    (C, false, "startOfDayForDate:", |e| {
        let (c, d) = (e.arg(0), e.arg(2));
        let cal = cal_of(e, c);
        let (t, _) = instant_of(e, d)?;
        ret_date(e, Some((cal.start_of_day(t), 0)))
    }),
    (C, false, "isDate:inSameDayAsDate:", |e| {
        let (c, a, b) = (e.arg(0), e.arg(2), e.arg(3));
        let b = instant_of(e, b)?;
        let same = same_day(e, c, a, b)?;
        ret(e, same as u64)
    }),
    (C, false, "isDateInToday:", |e| {
        let (c, d) = (e.arg(0), e.arg(2));
        let n = now(e)?;
        let same = same_day(e, c, d, n)?;
        ret(e, same as u64)
    }),
    (C, false, "isDateInYesterday:", |e| {
        let (c, d) = (e.arg(0), e.arg(2));
        let y = days_from_now(e, c, -1)?;
        let same = same_day(e, c, d, y)?;
        ret(e, same as u64)
    }),
    (C, false, "isDateInTomorrow:", |e| {
        let (c, d) = (e.arg(0), e.arg(2));
        let t = days_from_now(e, c, 1)?;
        let same = same_day(e, c, d, t)?;
        ret(e, same as u64)
    }),
    (C, false, "isDateInWeekend:", |e| {
        let (c, d) = (e.arg(0), e.arg(2));
        let cal = cal_of(e, c);
        let (t, n) = instant_of(e, d)?;
        let w = cal.split(WEEKDAY, t, n).weekday;
        ret(e, (w == 1 || w == 7) as u64)
    }),
    (C, false, "rangeOfUnit:inUnit:forDate:", |e| {
        let (c, small, large, d) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let cal = cal_of(e, c);
        let (t, _) = instant_of(e, d)?;
        let (loc, len) = cal.range(small, large, t).unwrap_or((i64::MAX, 0));
        e.ret(loc as u64);
        e.cpu.x[1] = len as u64;
        Ok(())
    }),
    (C, false, "ordinalityOfUnit:inUnit:forDate:", |e| {
        let (c, small, large, d) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let cal = cal_of(e, c);
        let (t, _) = instant_of(e, d)?;
        let v = cal.ordinality(small, large, t).unwrap_or(i64::MAX);
        ret(e, v as u64)
    }),
    (C, false, "dateBySettingHour:minute:second:ofDate:options:", |e| {
        let (c, h, m, s, d) = (e.arg(0), e.arg(2) as i64, e.arg(3) as i64, e.arg(4) as i64, e.arg(5));
        let cal = cal_of(e, c);
        let (t, n) = instant_of(e, d)?;
        let mut comps = cal.split(ERA | YEAR | MONTH | DAY, t, n);
        (comps.hour, comps.minute, comps.second) = (h, m, s);
        let valid = (0..24).contains(&h) && (0..60).contains(&m) && (0..60).contains(&s);
        ret_date(e, if valid { cal.join(&comps, None) } else { None })
    }),
    (C, false, "dateWithEra:year:month:day:hour:minute:second:nanosecond:", |e| {
        let c = e.arg(0);
        let cal = cal_of(e, c);
        // Eight NSInteger arguments: six in x2–x7, the last two on the stack.
        let mut a: Vec<i64> = (2..8).map(|i| e.arg(i) as i64).collect();
        for i in 0..2 {
            a.push(e.read_u64(e.cpu.sp + i * 8)? as i64);
        }
        let comps = Components { era: a[0], year: a[1], month: a[2], day: a[3], hour: a[4], minute: a[5], second: a[6], nanosecond: a[7], ..Default::default() };
        ret_date(e, cal.join(&comps, None))
    }),
    (C, false, "copyWithZone:", |e| {
        let cal = cal_of(e, e.arg(0));
        let c = new_calendar(e, cal)?;
        ret(e, c)
    }),
    (C, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = e.calendars.calendars.contains_key(&b) && cal_of(e, a) == cal_of(e, b);
        ret(e, same as u64)
    }),
    // NSDateComponents
    (DC, false, "calendar", |e| {
        let c = comps_of(e, e.arg(0)).calendar;
        ret(e, c)
    }),
    (DC, false, "setCalendar:", |e| {
        let (o, cal) = (e.arg(0), e.arg(2));
        let mut c = comps_of(e, o);
        c.calendar = cal;
        e.calendars.components.insert(o, c);
        Ok(())
    }),
    (DC, false, "timeZone", |e| {
        let z = comps_of(e, e.arg(0)).time_zone;
        ret(e, z)
    }),
    (DC, false, "setTimeZone:", |e| {
        let (o, z) = (e.arg(0), e.arg(2));
        let mut c = comps_of(e, o);
        c.time_zone = z;
        e.calendars.components.insert(o, c);
        Ok(())
    }),
    (DC, false, "isLeapMonth", |e| {
        let v = comps_of(e, e.arg(0)).leap_month;
        ret(e, v as u64)
    }),
    (DC, false, "setLeapMonth:", |e| {
        let (o, v) = (e.arg(0), e.arg(2) & 0xff != 0);
        let mut c = comps_of(e, o);
        c.leap_month = v;
        e.calendars.components.insert(o, c);
        Ok(())
    }),
    (DC, false, "valueForComponent:", |e| {
        let (o, unit) = (e.arg(0), e.arg(2));
        let v = comps_of(e, o).field(unit).map_or(UNDEFINED, |v| *v);
        ret(e, v as u64)
    }),
    (DC, false, "setValue:forComponent:", |e| {
        let (o, v, unit) = (e.arg(0), e.arg(2) as i64, e.arg(3));
        let mut c = comps_of(e, o);
        if let Some(f) = c.field(unit) {
            *f = v;
        }
        e.calendars.components.insert(o, c);
        Ok(())
    }),
    (DC, false, "date", |e| {
        let comps = comps_of(e, e.arg(0));
        if comps.calendar == 0 {
            return ret(e, 0);
        }
        let cal = cal_of(e, comps.calendar);
        let zone = (comps.time_zone != 0).then(|| zone_of(e, comps.time_zone));
        ret_date(e, cal.join(&comps, zone.as_ref()))
    }),
    (DC, false, "copyWithZone:", |e| {
        let c = comps_of(e, e.arg(0));
        let o = new_components(e, c)?;
        ret(e, o)
    }),
    (DC, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = e.calendars.components.contains_key(&b) && comps_of(e, a) == comps_of(e, b);
        ret(e, same as u64)
    }),
    // NSTimeZone
    (TZ, true, "systemTimeZone", device_zone),
    (TZ, true, "localTimeZone", device_zone),
    (TZ, true, "defaultTimeZone", device_zone),
    (TZ, true, "resetSystemTimeZone", |_| Ok(())),
    (TZ, true, "setDefaultTimeZone:", |e| {
        let z = e.arg(2);
        e.calendars.default_zone = (z != 0).then(|| zone_of(e, z));
        Ok(())
    }),
    (TZ, true, "timeZoneWithName:", zone_with_name),
    (TZ, false, "initWithName:", |e| {
        let (z, name) = (e.arg(0), e.arg(2));
        let info = if name == 0 { None } else { TimeZoneInfo::named(&nsstring(e, name)?) };
        match info {
            Some(i) => {
                e.calendars.zones.insert(z, i);
                ret(e, z)
            }
            None => ret(e, 0),
        }
    }),
    (TZ, true, "timeZoneForSecondsFromGMT:", |e| {
        let s = e.arg(2) as i64;
        if s.abs() > 18 * 3600 {
            return ret(e, 0);
        }
        let z = new_zone(e, TimeZoneInfo::fixed(s as i32))?;
        ret(e, z)
    }),
    (TZ, true, "timeZoneWithAbbreviation:", |e| {
        let a = e.arg(2);
        let a = if a == 0 { String::new() } else { nsstring(e, a)? };
        let info = ABBREVIATIONS.iter().find(|(k, _)| *k == a).and_then(|(_, n)| TimeZoneInfo::named(n)).or_else(|| parse_fixed(&a).map(TimeZoneInfo::fixed));
        match info {
            Some(i) => {
                let z = new_zone(e, i)?;
                ret(e, z)
            }
            None => ret(e, 0),
        }
    }),
    (TZ, true, "knownTimeZoneNames", |e| {
        let mut names = Vec::new();
        for tz in chrono_tz::TZ_VARIANTS.iter().filter(|t| t.name().contains('/') && !t.name().starts_with("Etc/")) {
            names.push(foundation::new_nsstring(e, tz.name())?);
        }
        let a = foundation::new_array_of(e, &names)?;
        ret(e, a)
    }),
    (TZ, false, "name", |e| {
        let n = zone_of(e, e.arg(0)).name;
        ret_str(e, &n)
    }),
    (TZ, false, "description", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, None)?;
        let s = format!("{} ({}) offset {}", z.name, z.abbreviation_at(t), z.offset_at(t));
        ret_str(e, &s)
    }),
    (TZ, false, "abbreviation", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, None)?;
        ret_str(e, &z.abbreviation_at(t))
    }),
    (TZ, false, "abbreviationForDate:", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, Some(2))?;
        ret_str(e, &z.abbreviation_at(t))
    }),
    (TZ, false, "secondsFromGMT", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, None)?;
        ret(e, z.offset_at(t) as i64 as u64)
    }),
    (TZ, false, "secondsFromGMTForDate:", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, Some(2))?;
        ret(e, z.offset_at(t) as i64 as u64)
    }),
    (TZ, false, "isDaylightSavingTime", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, None)?;
        ret(e, (z.dst_at(t) != 0) as u64)
    }),
    (TZ, false, "isDaylightSavingTimeForDate:", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, Some(2))?;
        ret(e, (z.dst_at(t) != 0) as u64)
    }),
    (TZ, false, "daylightSavingTimeOffset", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, None)?;
        ret_double(e, z.dst_at(t) as f64)
    }),
    (TZ, false, "daylightSavingTimeOffsetForDate:", |e| {
        let z = zone_of(e, e.arg(0));
        let t = t_now_or_arg(e, Some(2))?;
        ret_double(e, z.dst_at(t) as f64)
    }),
    (TZ, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = e.calendars.zones.contains_key(&b) && zone_of(e, a).name == zone_of(e, b).name;
        ret(e, same as u64)
    }),
    (TZ, false, "isEqualToTimeZone:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = e.calendars.zones.contains_key(&b) && zone_of(e, a).name == zone_of(e, b).name;
        ret(e, same as u64)
    }),
    (TZ, false, "hash", |e| {
        let n = zone_of(e, e.arg(0)).name;
        let h = n.bytes().fold(5381u64, |h, b| h.wrapping_mul(33) ^ b as u64);
        ret(e, h)
    }),
    (TZ, false, "copyWithZone:", |e| {
        let z = e.arg(0);
        ret(e, z)
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn ymd(y: i32, m: u32, d: u32) -> i64 {
        NaiveDate::from_ymd_opt(y, m, d).unwrap().and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp()
    }

    #[test]
    fn splits_and_joins_dates() {
        let cal = Gregorian::device();
        let t = ymd(2026, 11, 13) + 14 * 3600 + 3 * 60 + 22;
        let c = cal.split(!0, t, 5);
        assert_eq!((c.era, c.year, c.month, c.day, c.hour, c.minute, c.second, c.nanosecond), (1, 2026, 11, 13, 14, 3, 22, 5));
        assert_eq!(c.weekday, 6); // a Friday
        assert_eq!((c.quarter, c.weekday_ordinal), (4, 2));
        assert_eq!(cal.join(&c, None), Some((t, 5)));
    }

    #[test]
    fn components_carry_over() {
        let cal = Gregorian::device();
        let c = Components { year: 2026, month: 13, day: 0, ..Default::default() };
        // Month 13 is next January; day 0 the last day of the month before it.
        assert_eq!(cal.join(&c, None), Some((ymd(2026, 12, 31), 0)));
    }

    #[test]
    fn us_weeks() {
        let cal = Gregorian::device();
        // 1 January 2026 is a Thursday: its week is week 1, from Sunday 28 December 2025.
        let c = cal.split(WEEK_OF_YEAR | YEAR_FOR_WEEK_OF_YEAR, ymd(2025, 12, 28), 0);
        assert_eq!((c.week_of_year, c.year_for_week_of_year), (1, 2026));
        assert_eq!(cal.split(WEEK_OF_YEAR, ymd(2026, 1, 4), 0).week_of_year, 2);
        // 1 November 2026 is a Sunday: the 30th is in its fifth week.
        assert_eq!(cal.split(WEEK_OF_MONTH, ymd(2026, 11, 1), 0).week_of_month, 1);
        assert_eq!(cal.split(WEEK_OF_MONTH, ymd(2026, 11, 30), 0).week_of_month, 5);
    }

    #[test]
    fn adds_like_foundation() {
        let cal = Gregorian::device();
        let month = Components { month: 1, ..Default::default() };
        assert_eq!(cal.add(&month, ymd(2026, 1, 31), 0), Some((ymd(2026, 2, 28), 0)));
        assert_eq!(cal.add(&month, ymd(2028, 1, 31), 0), Some((ymd(2028, 2, 29), 0)));
        let back = Components { day: -1, ..Default::default() };
        assert_eq!(cal.add(&back, ymd(2026, 3, 1), 0), Some((ymd(2026, 2, 28), 0)));
    }

    #[test]
    fn days_keep_the_wall_clock_across_daylight_saving() {
        let mut cal = Gregorian::device();
        cal.zone = TimeZoneInfo::named("America/New_York").unwrap();
        // 9 am the day before the clocks go forward (8 March 2026), and a day later: still 9 am.
        let nine = cal.join(&Components { year: 2026, month: 3, day: 7, hour: 9, ..Default::default() }, None).unwrap();
        let next = cal.add(&Components { day: 1, ..Default::default() }, nine.0, 0).unwrap();
        assert_eq!(next.0 - nine.0, 23 * 3600);
        assert_eq!(cal.split(HOUR, next.0, 0).hour, 9);
        // 24 hours later is 10 am.
        let later = cal.add(&Components { hour: 24, ..Default::default() }, nine.0, 0).unwrap();
        assert_eq!(cal.split(HOUR, later.0, 0).hour, 10);
    }

    #[test]
    fn differences() {
        let cal = Gregorian::device();
        let d = cal.difference(YEAR | MONTH | DAY, (ymd(2024, 2, 29), 0), (ymd(2026, 3, 31), 0));
        assert_eq!((d.year, d.month, d.day), (2, 1, 3));
        let d = cal.difference(DAY, (ymd(2026, 3, 31), 0), (ymd(2026, 3, 1), 0));
        assert_eq!(d.day, -30);
        let d = cal.difference(HOUR | MINUTE, (0, 0), (5400, 0));
        assert_eq!((d.hour, d.minute), (1, 30));
    }

    #[test]
    fn ranges() {
        let cal = Gregorian::device();
        assert_eq!(cal.range(DAY, MONTH, ymd(2028, 2, 10)), Some((1, 29)));
        assert_eq!(cal.ordinality(DAY, YEAR, ymd(2026, 12, 31)), Some(365));
    }

    #[test]
    fn time_zones() {
        assert_eq!(TimeZoneInfo::fixed(19800).name, "GMT+0530");
        assert_eq!(fixed_abbreviation(-8 * 3600), "GMT-8");
        assert_eq!(TimeZoneInfo::named("GMT+0530").unwrap().zone, Zone::Fixed(19800));
        assert_eq!(TimeZoneInfo::named("UTC-03:30").unwrap().zone, Zone::Fixed(-12600));
        let sydney = TimeZoneInfo::named("Australia/Sydney").unwrap();
        assert_eq!(sydney.offset_at(ymd(2026, 1, 15)), 11 * 3600); // summer: AEDT
        assert_eq!(sydney.offset_at(ymd(2026, 7, 15)), 10 * 3600);
        assert_eq!(sydney.abbreviation_at(ymd(2026, 1, 15)), "AEDT");
        assert!(TimeZoneInfo::named("Nowhere/Special").is_none());
    }
}

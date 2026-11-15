//! Calendar time in the C library: `gmtime_r`/`localtime_r` (and the
//! non-reentrant forms), `timegm`/`mktime`, and `strftime`.
//!
//! Local time is in the device's time zone (`pomme run --time-zone`; GMT
//! unless set), as NSTimeZone.systemTimeZone has it (see calendar.rs), so
//! JavaScript's Date in Hermes, which asks `localtime_r`, agrees with
//! Foundation. `strftime` formats in the C locale, as an app's process starts in it.

use crate::{Emu, HostFn, Result};

/// Days since 1970-01-01 → (year, month 1-12, day 1-31). (Howard Hinnant's civil_from_days.)
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (year, month, day) → days since 1970-01-01. (Howard Hinnant's days_from_civil.)
pub(crate) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// `struct tm`'s fields, as C has them (months from 0, years from 1900).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Tm {
    pub sec: i32,
    pub min: i32,
    pub hour: i32,
    pub mday: i32,
    pub mon: i32,
    pub year: i32,
    pub wday: i32,
    pub yday: i32,
    pub isdst: i32,
    pub gmtoff: i64,
}

impl Tm {
    /// Broken-down UTC time for seconds since the epoch.
    pub(crate) fn utc(t: i64) -> Tm {
        let days = t.div_euclid(86_400);
        let secs = t.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        Tm {
            sec: (secs % 60) as i32,
            min: (secs / 60 % 60) as i32,
            hour: (secs / 3600) as i32,
            mday: d as i32,
            mon: m as i32 - 1,
            year: (y - 1900) as i32,
            wday: (days + 4).rem_euclid(7) as i32, // 1970-01-01 was a Thursday
            yday: (days - days_from_civil(y, 1, 1)) as i32,
            isdst: 0,
            gmtoff: 0,
        }
    }

    /// Seconds since the epoch, as `timegm` computes them: out-of-range fields
    /// carry over (month 12 is next January, day 0 the last of the month before).
    pub(crate) fn to_epoch(&self) -> i64 {
        let months = self.year as i64 * 12 + self.mon as i64;
        let (y, m) = (1900 + months.div_euclid(12), months.rem_euclid(12) as u32 + 1);
        let days = days_from_civil(y, m, 1) + self.mday as i64 - 1;
        days * 86_400 + self.hour as i64 * 3600 + self.min as i64 * 60 + self.sec as i64
    }
}

const TM_GMTOFF: u64 = 40;
const TM_ZONE: u64 = 48;

pub(crate) fn read_tm(emu: &Emu, p: u64) -> Result<Tm> {
    let f = |i: u64| -> Result<i32> { Ok(emu.p.memory.read_u32(p + i * 4)? as i32) };
    Ok(Tm {
        sec: f(0)?,
        min: f(1)?,
        hour: f(2)?,
        mday: f(3)?,
        mon: f(4)?,
        year: f(5)?,
        wday: f(6)?,
        yday: f(7)?,
        isdst: f(8)?,
        gmtoff: emu.read_u64(p + TM_GMTOFF)? as i64,
    })
}

/// Write `tm` and its zone's abbreviation (`tm_zone`).
fn write_tm(emu: &mut Emu, p: u64, tm: &Tm, zone: &str) -> Result<()> {
    let fields = [tm.sec, tm.min, tm.hour, tm.mday, tm.mon, tm.year, tm.wday, tm.yday, tm.isdst];
    for (i, f) in fields.iter().enumerate() {
        emu.p.memory.write_u32(p + i as u64 * 4, *f as u32)?;
    }
    emu.write_u64(p + TM_GMTOFF, tm.gmtoff as u64)?;
    let zone = zone_name(emu, zone)?;
    emu.write_u64(p + TM_ZONE, zone)
}

/// A zone abbreviation ("UTC", "AEDT"), one guest copy of each for every `struct tm`.
fn zone_name(emu: &mut Emu, name: &str) -> Result<u64> {
    if let Some(&p) = emu.time.zone_names.get(name) {
        return Ok(p);
    }
    let p = emu.alloc_cstr(name)?;
    emu.time.zone_names.insert(name.to_string(), p);
    Ok(p)
}

#[derive(Clone, Default)]
pub(crate) struct State {
    zone_names: std::collections::HashMap<String, u64>,
    /// The `struct tm` that `gmtime` and `localtime` return.
    static_tm: u64,
}

/// Broken-down time for `t` in the device's time zone, and the zone's abbreviation then.
fn local(emu: &Emu, t: i64) -> (Tm, String) {
    let zone = crate::calendar::system_zone(emu);
    let offset = zone.offset_at(t) as i64;
    let mut tm = Tm::utc(t + offset);
    tm.gmtoff = offset;
    tm.isdst = (zone.dst_at(t) != 0) as i32;
    (tm, zone.abbreviation_at(t))
}

/// Into `out`, or the one static `struct tm` when `out` is 0; returns where.
fn broken_down(emu: &mut Emu, tp: u64, out: u64, in_local: bool) -> Result<()> {
    let out = if out != 0 {
        out
    } else {
        if emu.time.static_tm == 0 {
            emu.time.static_tm = emu.alloc(56)?;
        }
        emu.time.static_tm
    };
    let t = emu.read_u64(tp)? as i64;
    let (tm, zone) = if in_local { local(emu, t) } else { (Tm::utc(t), "UTC".to_string()) };
    write_tm(emu, out, &tm, &zone)?;
    emu.ret(out);
    Ok(())
}

/// `gmtime_r(const time_t *, struct tm *)`
fn gmtime_r(emu: &mut Emu) -> Result<()> {
    broken_down(emu, emu.arg(0), emu.arg(1), false)
}

/// `localtime_r(const time_t *, struct tm *)`: in the device's time zone.
fn localtime_r(emu: &mut Emu) -> Result<()> {
    broken_down(emu, emu.arg(0), emu.arg(1), true)
}

/// `gmtime(const time_t *)`: into one static `struct tm`.
fn gmtime(emu: &mut Emu) -> Result<()> {
    broken_down(emu, emu.arg(0), 0, false)
}

/// `localtime(const time_t *)`
fn localtime(emu: &mut Emu) -> Result<()> {
    broken_down(emu, emu.arg(0), 0, true)
}

/// `timegm(struct tm *)`: seconds since the epoch, the fields normalized.
fn timegm(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    let t = read_tm(emu, p)?.to_epoch();
    write_tm(emu, p, &Tm::utc(t), "UTC")?;
    emu.ret(t as u64);
    Ok(())
}

/// `mktime(struct tm *)`: the fields as wall-clock time in the device's time
/// zone (in a daylight saving gap, as if the clocks hadn't changed; when the
/// time happens twice, the first), normalized.
fn mktime(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    let wall = read_tm(emu, p)?.to_epoch();
    let t = crate::calendar::system_zone(emu).instant_of_wall_clock(wall);
    let (tm, zone) = local(emu, t);
    write_tm(emu, p, &tm, &zone)?;
    emu.ret(t as u64);
    Ok(())
}

// ---- strftime ----

const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// ISO 8601 week-based year and week number (`%G`, `%V`).
fn iso_week(tm: &Tm) -> (i64, i32) {
    let year = tm.year as i64 + 1900;
    let wday = (tm.wday + 6) % 7; // Monday = 0
    // The Thursday of this week decides the year.
    let thursday = tm.yday - wday + 3;
    let days_in = |y: i64| if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 { 366 } else { 365 };
    if thursday < 0 {
        let prev = days_in(year - 1);
        return (year - 1, (thursday + prev) / 7 + 1);
    }
    if thursday >= days_in(year) {
        return (year + 1, 1);
    }
    (year, thursday / 7 + 1)
}

/// Format `tm` by `fmt`, as `strftime` does in the C locale.
pub(crate) fn strftime(tm: &Tm, fmt: &[u8], zone: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    let day = |n: i32| DAYS[n.rem_euclid(7) as usize];
    let month = |n: i32| MONTHS[n.rem_euclid(12) as usize];
    let year = tm.year as i64 + 1900;
    let hour12 = if tm.hour % 12 == 0 { 12 } else { tm.hour % 12 };
    while i < fmt.len() {
        let c = fmt[i];
        i += 1;
        if c != b'%' || i >= fmt.len() {
            out.push(c);
            continue;
        }
        // The E and O modifiers choose alternative forms; the C locale has none.
        while i < fmt.len() && matches!(fmt[i], b'E' | b'O') {
            i += 1;
        }
        let Some(&conv) = fmt.get(i) else { break };
        i += 1;
        let s = match conv {
            b'a' => day(tm.wday)[..3].to_string(),
            b'A' => day(tm.wday).to_string(),
            b'b' | b'h' => month(tm.mon)[..3].to_string(),
            b'B' => month(tm.mon).to_string(),
            b'c' => String::from_utf8_lossy(&strftime(tm, b"%a %b %e %H:%M:%S %Y", zone)).into_owned(),
            b'C' => format!("{:02}", year.div_euclid(100)),
            b'd' => format!("{:02}", tm.mday),
            b'D' | b'x' => String::from_utf8_lossy(&strftime(tm, b"%m/%d/%y", zone)).into_owned(),
            b'e' => format!("{:2}", tm.mday),
            b'F' => String::from_utf8_lossy(&strftime(tm, b"%Y-%m-%d", zone)).into_owned(),
            b'G' => iso_week(tm).0.to_string(),
            b'g' => format!("{:02}", iso_week(tm).0.rem_euclid(100)),
            b'H' => format!("{:02}", tm.hour),
            b'I' => format!("{hour12:02}"),
            b'j' => format!("{:03}", tm.yday + 1),
            b'k' => format!("{:2}", tm.hour),
            b'l' => format!("{hour12:2}"),
            b'm' => format!("{:02}", tm.mon + 1),
            b'M' => format!("{:02}", tm.min),
            b'n' => "\n".into(),
            b'p' => if tm.hour < 12 { "AM" } else { "PM" }.into(),
            b'r' => String::from_utf8_lossy(&strftime(tm, b"%I:%M:%S %p", zone)).into_owned(),
            b'R' => String::from_utf8_lossy(&strftime(tm, b"%H:%M", zone)).into_owned(),
            b's' => (tm.to_epoch() - tm.gmtoff).to_string(),
            b'S' => format!("{:02}", tm.sec),
            b't' => "\t".into(),
            b'T' | b'X' => String::from_utf8_lossy(&strftime(tm, b"%H:%M:%S", zone)).into_owned(),
            b'u' => (if tm.wday == 0 { 7 } else { tm.wday }).to_string(),
            b'U' => format!("{:02}", (tm.yday + 7 - tm.wday) / 7),
            b'V' => format!("{:02}", iso_week(tm).1),
            b'w' => tm.wday.to_string(),
            b'W' => format!("{:02}", (tm.yday + 7 - (tm.wday + 6) % 7) / 7),
            b'y' => format!("{:02}", year.rem_euclid(100)),
            b'Y' => year.to_string(),
            b'z' => {
                let off = tm.gmtoff / 60;
                format!("{}{:02}{:02}", if off < 0 { '-' } else { '+' }, off.abs() / 60, off.abs() % 60)
            }
            b'Z' => zone.to_string(),
            b'+' => String::from_utf8_lossy(&strftime(tm, b"%a %b %e %H:%M:%S %Z %Y", zone)).into_owned(),
            b'%' => "%".into(),
            // Unknown: as written.
            other => format!("%{}", other as char),
        };
        out.extend_from_slice(s.as_bytes());
    }
    out
}

/// `strftime(buf, maxsize, format, const struct tm *)`: the length, or 0 if it doesn't fit.
fn strftime_fn(emu: &mut Emu) -> Result<()> {
    let (buf, max, fmt, tp) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    format_into(emu, buf, max, fmt, tp)
}

/// `strftime_l(buf, maxsize, format, tm, locale)`
fn strftime_l(emu: &mut Emu) -> Result<()> {
    strftime_fn(emu)
}

fn format_into(emu: &mut Emu, buf: u64, max: u64, fmt: u64, tp: u64) -> Result<()> {
    let tm = read_tm(emu, tp)?;
    let zp = emu.read_u64(tp + TM_ZONE)?;
    let zone = if zp == 0 { String::new() } else { emu.cstr(zp)? };
    let f = emu.cstr_bytes(fmt)?;
    let mut s = strftime(&tm, &f, &zone);
    if s.len() as u64 >= max {
        emu.ret(0);
        return Ok(());
    }
    let n = s.len() as u64;
    s.push(0);
    emu.p.memory.write(buf, &s)?;
    emu.ret(n);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_gmtime_r", gmtime_r),
    ("_localtime_r", localtime_r),
    ("_gmtime", gmtime),
    ("_localtime", localtime),
    ("_timegm", timegm),
    ("_mktime", mktime),
    ("_strftime", strftime_fn),
    ("_strftime_l", strftime_l),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn fmt(t: i64, f: &str) -> String {
        String::from_utf8(strftime(&Tm::utc(t), f.as_bytes(), "UTC")).unwrap()
    }

    #[test]
    fn broken_down_time() {
        // 2026-11-13 09:05:07 UTC, a Friday.
        let t = 1_794_560_707;
        let tm = Tm::utc(t);
        assert_eq!((tm.year, tm.mon, tm.mday, tm.hour, tm.min, tm.sec, tm.wday, tm.yday), (126, 10, 13, 9, 5, 7, 5, 316));
        assert_eq!(tm.to_epoch(), t);
        // Fields out of range carry over.
        let mut odd = tm.clone();
        odd.mon = 12;
        odd.mday = 0;
        assert_eq!(Tm::utc(odd.to_epoch()).mday, 31); // 2026-12-31
    }

    #[test]
    fn formats() {
        let t = 1_794_560_707;
        assert_eq!(fmt(t, "%Y-%m-%d %H:%M:%S %z"), "2026-11-13 09:05:07 +0000");
        assert_eq!(fmt(t, "%a %A %b %B %e %j %p %I %y %C"), "Fri Friday Nov November 13 317 AM 09 26 20");
        assert_eq!(fmt(t, "%c"), "Fri Nov 13 09:05:07 2026");
        assert_eq!(fmt(t, "%D %T %R %F %u %w %%"), "11/13/26 09:05:07 09:05 2026-11-13 5 5 %");
        assert_eq!(fmt(t, "%s %Z %Ey"), "1794560707 UTC 26");
    }

    #[test]
    fn week_numbers() {
        // 2021-01-01 (a Friday) is in ISO week 53 of 2020; 2024-12-30 in week 1 of 2025.
        let jan1 = days_from_civil(2021, 1, 1) * 86_400;
        assert_eq!(fmt(jan1, "%G %V %U %W"), "2020 53 00 00");
        let dec30 = days_from_civil(2024, 12, 30) * 86_400;
        assert_eq!(fmt(dec30, "%G %V %g"), "2025 01 25");
        let jan7 = days_from_civil(2024, 1, 7) * 86_400; // a Sunday
        assert_eq!(fmt(jan7, "%U %W %V"), "01 01 01");
    }
}

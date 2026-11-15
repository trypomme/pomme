//! NSDateFormatter, on CoreFoundation's CFDateFormatter (ICU), as Foundation
//! builds it: a format string or date and time styles, a locale (the
//! device's unless set) and a time zone (the device's unless set). Each
//! conversion makes a CF formatter from those settings.
//!
//! Time zones: pomme's virtual file system has no tz database for CFTimeZone
//! to read, so a named zone CoreFoundation can't load is given to it under
//! its name with its offset at the date in question (from pomme's IANA data).
//! CFDateFormatter hands ICU only the zone's name, and ICU has its own zone
//! data, so names (`zzzz`: "Pacific Daylight Time") come out as on iOS.
//!
//! iOS's 24-Hour Time switch: Apple's ICU applies it to the styles; the
//! sysroot's open-source ICU can't, so pomme rebuilds the time pattern with a
//! forced hour field, as Apple's does.

use crate::calendar::{TimeZoneInfo, Zone};
use crate::foundation::{self, cf, nsstring};
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// Seconds from 1970 to 2001 (NSDate's reference date).
const REFERENCE_1970: f64 = 978_307_200.0;

#[derive(Clone, Default)]
struct Formatter {
    format: Option<String>,
    /// NSDateFormatterStyle for the date and the time (CFDateFormatterStyle has the same values).
    date_style: u64,
    time_style: u64,
    locale: Option<String>,
    /// An NSTimeZone, or 0 for the default zone.
    zone: u64,
    lenient: bool,
    /// `doesRelativeDateFormatting` ("Today", "Yesterday").
    relative: bool,
}

#[derive(Clone, Default)]
pub struct State {
    formatters: HashMap<u64, Formatter>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn get(emu: &Emu, f: u64) -> Formatter {
    emu.dateformatters.formatters.get(&f).cloned().unwrap_or_default()
}

fn edit(emu: &mut Emu, f: u64) -> &mut Formatter {
    emu.dateformatters.formatters.entry(f).or_default()
}

/// A time zone file (TZif) with one period: `offset` seconds from GMT, all the time.
fn fixed_tzif(offset: i32) -> Vec<u8> {
    let mut d = b"TZif".to_vec();
    d.extend([0u8; 16]); // version (1) and reserved
    // isutcnt, isstdcnt, leapcnt, timecnt, typecnt, charcnt
    for n in [0u32, 0, 0, 0, 1, 1] {
        d.extend(n.to_be_bytes());
    }
    d.extend(offset.to_be_bytes()); // utoff
    d.extend([0, 0]); // isdst, abbreviation index
    d.push(0); // the abbreviation ""
    d
}

/// A CFTimeZone for `info` around `t` (seconds since 1970).
fn cf_zone(emu: &mut Emu, info: &TimeZoneInfo, t: f64) -> Result<u64> {
    let offset = info.offset_at(t.floor() as i64);
    if let Zone::Named(_) = info.zone {
        let name = foundation::new_nsstring(emu, &info.name)?;
        let z = cf(emu, "_CFTimeZoneCreateWithName", &[0, name, 0])?;
        if z != 0 {
            return Ok(z);
        }
        // Its name for ICU, and its offset at `t` for CoreFoundation.
        let data = foundation::new_data(emu, &fixed_tzif(offset as i32))?;
        let z = cf(emu, "_CFTimeZoneCreate", &[0, name, data])?;
        if z != 0 {
            return Ok(z);
        }
    }
    emu.cpu.q[0] = (offset as f64).to_bits() as u128;
    cf(emu, "_CFTimeZoneCreateWithTimeIntervalFromGMT", &[0])
}

/// A date pattern's pieces: a field (`HH`, `zzzz`) or literal text (`:`, `'at'`).
#[derive(Debug, PartialEq)]
enum Piece {
    Field(char, String),
    Text(String),
}

fn pieces(pattern: &str) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut quoted = false;
    for c in pattern.chars() {
        if c == '\'' {
            quoted = !quoted;
        }
        if !quoted && c != '\'' && c.is_ascii_alphabetic() {
            match out.last_mut() {
                Some(Piece::Field(l, s)) if *l == c => s.push(c),
                _ => out.push(Piece::Field(c, c.to_string())),
            }
        } else {
            match out.last_mut() {
                Some(Piece::Text(s)) => s.push(c),
                _ => out.push(Piece::Text(c.to_string())),
            }
        }
    }
    out
}

/// The pattern with iOS's 24-Hour Time switch applied, as Apple's ICU does
/// it: the run of time fields around the hour (hour, minutes, seconds, day
/// period, and the plain separators between them) becomes the locale's best
/// pattern for the same fields with a 24-hour (`H`) or 12-hour (`h`) hour.
/// `skeleton_pattern` asks ICU for that best pattern.
fn force_pattern(pattern: &str, force_24: bool, skeleton_pattern: &mut dyn FnMut(&str) -> Result<String>) -> Result<String> {
    let ps = pieces(pattern);
    let is_time = |p: &Piece| matches!(p, Piece::Field(c, _) if "hHkKmsSabB".contains(*c));
    let is_sep = |p: &Piece| matches!(p, Piece::Text(t) if !t.contains('\'') && t.chars().all(|c| !c.is_alphanumeric()));
    let Some(hour) = ps.iter().position(|p| matches!(p, Piece::Field(c, _) if "hHkK".contains(*c))) else { return Ok(pattern.to_string()) };
    // Widen to the time fields (and the separators between them) on each side.
    let (mut start, mut end) = (hour, hour);
    while start >= 2 && is_sep(&ps[start - 1]) && is_time(&ps[start - 2]) {
        start -= 2;
    }
    while end + 2 < ps.len() && is_sep(&ps[end + 1]) && is_time(&ps[end + 2]) {
        end += 2;
    }
    if end + 1 < ps.len() && is_time(&ps[end + 1]) {
        end += 1;
    }
    let mut skeleton = String::new();
    for p in &ps[start..=end] {
        if let Piece::Field(c, s) = p {
            match c {
                'a' | 'b' | 'B' => {}
                'h' | 'H' | 'k' | 'K' => skeleton.push_str(&if force_24 { "HH" } else { "h" }.to_string()),
                _ => skeleton.push_str(s),
            }
        }
    }
    let time = skeleton_pattern(&skeleton)?;
    let text = |p: &Piece| match p {
        Piece::Field(_, s) | Piece::Text(s) => s.clone(),
    };
    let before: String = ps[..start].iter().map(text).collect();
    let after: String = ps[end + 1..].iter().map(text).collect();
    Ok(format!("{before}{time}{after}"))
}

/// Apply the 24-Hour Time switch to a formatter in the device locale.
fn force_hours(emu: &mut Emu, formatter: u64, fm: &Formatter) -> Result<()> {
    let Some(force_24) = crate::locale::forced_hours(emu)? else { return Ok(()) };
    let current = crate::locale::current_id(emu)?;
    if fm.locale.as_ref().is_some_and(|id| *id != current) {
        return Ok(());
    }
    let full = cf(emu, "_CFDateFormatterGetFormat", &[formatter])?;
    let full = nsstring(emu, full)?;
    // The locale without the device's preferences, so ICU gives its own patterns.
    let ids = foundation::new_nsstring(emu, &current)?;
    let plain = cf(emu, "_CFLocaleCreate", &[0, ids])?;
    let forced = force_pattern(&full, force_24, &mut |skeleton| {
        let s = foundation::new_nsstring(emu, skeleton)?;
        let f = cf(emu, "_CFDateFormatterCreateDateFormatFromTemplate", &[0, s, 0, plain])?;
        if f == 0 { Ok(skeleton.to_string()) } else { nsstring(emu, f) }
    })?;
    if forced != full {
        let s = foundation::new_nsstring(emu, &forced)?;
        cf(emu, "_CFDateFormatterSetFormat", &[formatter, s])?;
    }
    Ok(())
}

/// A CFDateFormatter for `f`'s settings, its zone taken at `t`.
fn cf_formatter(emu: &mut Emu, f: u64, t: f64) -> Result<u64> {
    let fm = get(emu, f);
    let locale = crate::locale::cf_locale_for(emu, fm.locale.as_deref())?;
    let formatter = cf(emu, "_CFDateFormatterCreate", &[0, locale, fm.date_style, fm.time_style])?;
    if let Some(format) = &fm.format {
        let s = foundation::new_nsstring(emu, format)?;
        cf(emu, "_CFDateFormatterSetFormat", &[formatter, s])?;
    }
    let info = crate::calendar::zone_info(emu, fm.zone);
    let zone = cf_zone(emu, &info, t)?;
    let key = foundation::cf_data_pub(emu, "_kCFDateFormatterTimeZone")?;
    cf(emu, "_CFDateFormatterSetProperty", &[formatter, key, zone])?;
    if fm.lenient {
        let key = foundation::cf_data_pub(emu, "_kCFDateFormatterIsLenient")?;
        let yes = foundation::boolean(emu, true);
        cf(emu, "_CFDateFormatterSetProperty", &[formatter, key, yes])?;
    }
    if fm.relative {
        let key = foundation::cf_data_pub(emu, "_kCFDateFormatterDoesRelativeDateFormattingKey")?;
        let yes = foundation::boolean(emu, true);
        cf(emu, "_CFDateFormatterSetProperty", &[formatter, key, yes])?;
    } else {
        force_hours(emu, formatter, &fm)?;
    }
    Ok(formatter)
}

/// `stringFromDate:`
fn string_from_date(emu: &mut Emu) -> Result<()> {
    let (f, d) = (emu.arg(0), emu.arg(2));
    if d == 0 {
        return ret(emu, 0);
    }
    let t = foundation::date_value(emu, d)? + REFERENCE_1970;
    let formatter = cf_formatter(emu, f, t)?;
    let s = cf(emu, "_CFDateFormatterCreateStringWithDate", &[0, formatter, d])?;
    ret(emu, s)
}

/// `dateFromString:`: nil unless the whole string matches (or the formatter is lenient).
fn date_from_string(emu: &mut Emu) -> Result<()> {
    let (f, s) = (emu.arg(0), emu.arg(2));
    if s == 0 {
        return ret(emu, 0);
    }
    let lenient = get(emu, f).lenient;
    let s = foundation::as_cf_string(emu, s)?;
    let len = cf(emu, "_CFStringGetLength", &[s])?;
    let range = emu.alloc(16)?;
    // A named zone's offset depends on the date: parse with today's, then again with the result's.
    let date_cls = host_class(emu, "NSDate")?;
    let today = send(emu, date_cls, "date", &[])?;
    let now = foundation::date_value(emu, today)? + REFERENCE_1970;
    let mut t = now;
    let mut date = 0;
    for _ in 0..2 {
        emu.write_u64(range, 0)?;
        emu.write_u64(range + 8, len)?;
        let formatter = cf_formatter(emu, f, t)?;
        date = cf(emu, "_CFDateFormatterCreateDateFromString", &[0, formatter, s, range])?;
        if date == 0 || (!lenient && emu.read_u64(range + 8)? != len) {
            return ret(emu, 0);
        }
        let parsed = foundation::date_value(emu, date)? + REFERENCE_1970;
        if (parsed - t).abs() < 1.0 {
            break;
        }
        t = parsed;
    }
    ret(emu, date)
}

const DF: &str = "NSDateFormatter";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (DF, false, "init", |e| {
        let f = e.arg(0);
        e.dateformatters.formatters.insert(f, Formatter::default());
        ret(e, f)
    }),
    (DF, false, "dateFormat", |e| {
        let f = get(e, e.arg(0)).format;
        let s = match f {
            Some(s) => foundation::new_nsstring(e, &s)?,
            None => 0,
        };
        ret(e, s)
    }),
    (DF, false, "setDateFormat:", |e| {
        let (f, s) = (e.arg(0), e.arg(2));
        let format = if s == 0 { None } else { Some(nsstring(e, s)?) };
        edit(e, f).format = format;
        Ok(())
    }),
    (DF, false, "setLocalizedDateFormatFromTemplate:", |e| {
        let (f, s) = (e.arg(0), e.arg(2));
        let t = nsstring(e, s)?;
        let locale = get(e, f).locale;
        let format = crate::locale::format_from_template(e, &t, locale.as_deref())?;
        edit(e, f).format = Some(format);
        Ok(())
    }),
    (DF, false, "dateStyle", |e| {
        let s = get(e, e.arg(0)).date_style;
        ret(e, s)
    }),
    (DF, false, "setDateStyle:", |e| {
        let (f, s) = (e.arg(0), e.arg(2));
        let fm = edit(e, f);
        fm.date_style = s;
        fm.format = None;
        Ok(())
    }),
    (DF, false, "timeStyle", |e| {
        let s = get(e, e.arg(0)).time_style;
        ret(e, s)
    }),
    (DF, false, "setTimeStyle:", |e| {
        let (f, s) = (e.arg(0), e.arg(2));
        let fm = edit(e, f);
        fm.time_style = s;
        fm.format = None;
        Ok(())
    }),
    (DF, false, "locale", |e| {
        let id = get(e, e.arg(0)).locale;
        let cls = host_class(e, "NSLocale")?;
        let l = match id {
            Some(id) => {
                let s = foundation::new_nsstring(e, &id)?;
                send(e, cls, "localeWithLocaleIdentifier:", &[s])?
            }
            None => send(e, cls, "currentLocale", &[])?,
        };
        ret(e, l)
    }),
    (DF, false, "setLocale:", |e| {
        let (f, l) = (e.arg(0), e.arg(2));
        let id = if l == 0 {
            None
        } else {
            let s = send(e, l, "localeIdentifier", &[])?;
            Some(nsstring(e, s)?)
        };
        edit(e, f).locale = id;
        Ok(())
    }),
    (DF, false, "timeZone", |e| {
        let z = get(e, e.arg(0)).zone;
        let z = if z == 0 {
            let cls = host_class(e, "NSTimeZone")?;
            send(e, cls, "defaultTimeZone", &[])?
        } else {
            z
        };
        ret(e, z)
    }),
    (DF, false, "setTimeZone:", |e| {
        let (f, z) = (e.arg(0), e.arg(2));
        edit(e, f).zone = z;
        Ok(())
    }),
    (DF, false, "isLenient", |e| {
        let l = get(e, e.arg(0)).lenient;
        ret(e, l as u64)
    }),
    (DF, false, "setLenient:", |e| {
        let (f, v) = (e.arg(0), e.arg(2) & 0xff != 0);
        edit(e, f).lenient = v;
        Ok(())
    }),
    // Gregorian is the only calendar pomme models; behaviour 10.4 is the only one.
    (DF, false, "setCalendar:", |_| Ok(())),
    (DF, false, "calendar", |e| {
        let cls = host_class(e, "NSCalendar")?;
        let c = send(e, cls, "currentCalendar", &[])?;
        ret(e, c)
    }),
    (DF, false, "setFormatterBehavior:", |_| Ok(())),
    (DF, false, "setDoesRelativeDateFormatting:", |e| {
        let (f, v) = (e.arg(0), e.arg(2) & 0xff != 0);
        edit(e, f).relative = v;
        Ok(())
    }),
    (DF, false, "doesRelativeDateFormatting", |e| {
        let v = get(e, e.arg(0)).relative;
        ret(e, v as u64)
    }),
    (DF, false, "stringFromDate:", string_from_date),
    (DF, false, "stringForObjectValue:", string_from_date),
    (DF, false, "dateFromString:", date_from_string),
    (DF, true, "localizedStringFromDate:dateStyle:timeStyle:", |e| {
        let (d, ds, ts) = (e.arg(2), e.arg(3), e.arg(4));
        let cls = host_class(e, DF)?;
        let f = objc::create_instance(e, cls, 0)?;
        e.dateformatters.formatters.insert(f, Formatter { date_style: ds, time_style: ts, ..Default::default() });
        e.cpu.x[0] = f;
        e.cpu.x[2] = d;
        string_from_date(e)
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// What ICU's en-GB patterns give for these skeletons.
    fn en_gb(skeleton: &str) -> Result<String> {
        Ok(match skeleton {
            "hmmss" => "h:mm:ss\u{202f}a",
            "hmm" => "h:mm\u{202f}a",
            "HHmmss" => "HH:mm:ss",
            other => other,
        }
        .to_string())
    }

    #[test]
    fn forces_the_time_fields_only() {
        let twelve = |p: &str| force_pattern(p, false, &mut en_gb).unwrap();
        assert_eq!(twelve("yyyy-MM-dd HH:mm:ss zzz"), "yyyy-MM-dd h:mm:ss\u{202f}a zzz");
        assert_eq!(twelve("dd/MM/y, HH:mm"), "dd/MM/y, h:mm\u{202f}a");
        assert_eq!(twelve("d MMM y 'at' HH:mm:ss"), "d MMM y 'at' h:mm:ss\u{202f}a");
        assert_eq!(twelve("dd/MM/y"), "dd/MM/y");
        let twenty_four = |p: &str| force_pattern(p, true, &mut en_gb).unwrap();
        assert_eq!(twenty_four("h:mm:ss\u{202f}a zzzz"), "HH:mm:ss zzzz");
    }
}

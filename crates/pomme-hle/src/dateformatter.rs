//! NSDateFormatter, on CoreFoundation's CFDateFormatter (ICU), as Foundation
//! builds it: a format string or date and time styles, a locale (the
//! device's, en_US, unless set) and a time zone (the device's, GMT, unless
//! set). Each conversion makes a CF formatter from those settings.
//!
//! Time zones: pomme's virtual file system has no tz database for CFTimeZone
//! to read, so a named zone CoreFoundation can't load is given to it as its
//! offset at the date in question (from pomme's IANA data): the digits are
//! right; a zone-name field (`zzzz`) shows the offset instead of the name.

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
}

#[derive(Default)]
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

/// A CFTimeZone for `info` around `t` (seconds since 1970).
fn cf_zone(emu: &mut Emu, info: &TimeZoneInfo, t: f64) -> Result<u64> {
    if let Zone::Named(_) = info.zone {
        let name = foundation::new_nsstring(emu, &info.name)?;
        let z = cf(emu, "_CFTimeZoneCreateWithName", &[0, name, 0])?;
        if z != 0 {
            return Ok(z);
        }
    }
    let offset = info.offset_at(t.floor() as i64) as f64;
    emu.cpu.q[0] = offset.to_bits() as u128;
    cf(emu, "_CFTimeZoneCreateWithTimeIntervalFromGMT", &[0])
}

/// A CFDateFormatter for `f`'s settings, its zone taken at `t`.
fn cf_formatter(emu: &mut Emu, f: u64, t: f64) -> Result<u64> {
    let fm = get(emu, f);
    let id = fm.locale.clone().unwrap_or_else(|| crate::locale::CURRENT.into());
    let id = foundation::new_nsstring(emu, &id)?;
    let locale = cf(emu, "_CFLocaleCreate", &[0, id])?;
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
        edit(e, f).format = Some(crate::locale::date_format_from_template(&t));
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
    (DF, false, "setDoesRelativeDateFormatting:", |_| Ok(())),
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

//! NSNumberFormatter, on CoreFoundation's CFNumberFormatter (ICU), as
//! Foundation builds it: a style (decimal, currency, percent, scientific,
//! spell-out, ordinal, ...), a locale (the device's, en_US, unless set), and
//! properties such as fraction digits and grouping. Each conversion makes a CF
//! formatter from those and asks it.

use crate::foundation::{self, cf, cf_data_pub, nsstring};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

#[derive(Clone, Default)]
struct Formatter {
    /// NSNumberFormatterStyle (CFNumberFormatterStyle has the same values).
    style: u64,
    /// The locale's identifier; None: the current locale.
    locale: Option<String>,
    /// CF property name (kCFNumberFormatter…) → value object.
    props: Vec<(&'static str, u64)>,
    lenient: bool,
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
    emu.numberformatters.formatters.get(&f).cloned().unwrap_or_default()
}

fn set_prop(emu: &mut Emu, f: u64, key: &'static str, value: u64) {
    let fm = emu.numberformatters.formatters.entry(f).or_default();
    fm.props.retain(|(k, _)| *k != key);
    if value != 0 {
        fm.props.push((key, value));
    }
}

/// A CFNumberFormatter for `f`'s settings.
fn cf_formatter(emu: &mut Emu, f: u64) -> Result<u64> {
    let fm = get(emu, f);
    let id = fm.locale.unwrap_or_else(|| crate::locale::CURRENT.into());
    let id = foundation::new_nsstring(emu, &id)?;
    let locale = cf(emu, "_CFLocaleCreate", &[0, id])?;
    let formatter = cf(emu, "_CFNumberFormatterCreate", &[0, locale, fm.style])?;
    for (key, value) in fm.props {
        let k = cf_data_pub(emu, key)?;
        cf(emu, "_CFNumberFormatterSetProperty", &[formatter, k, value])?;
    }
    Ok(formatter)
}

/// `stringFromNumber:`
fn string_from_number(emu: &mut Emu) -> Result<()> {
    let (f, n) = (emu.arg(0), emu.arg(2));
    if n == 0 {
        return ret(emu, 0);
    }
    // CoreFoundation reads the value itself: hand it a CFNumber (the
    // compiler's constant NSNumber literals aren't).
    let n = if foundation::number_is_float(emu, n)? {
        crate::objc::send(emu, n, "doubleValue", &[])?;
        let v = f64::from_bits(emu.last_q0 as u64);
        foundation::new_number_f64(emu, v)?
    } else {
        let v = crate::objc::send(emu, n, "longLongValue", &[])? as i64;
        foundation::new_number_i64(emu, v)?
    };
    let formatter = cf_formatter(emu, f)?;
    let s = cf(emu, "_CFNumberFormatterCreateStringWithNumber", &[0, formatter, n])?;
    ret(emu, s)
}

/// `numberFromString:`: nil unless the whole string is a number in the style
/// (a lenient formatter takes what it can parse).
fn number_from_string(emu: &mut Emu) -> Result<()> {
    let (f, s) = (emu.arg(0), emu.arg(2));
    if s == 0 {
        return ret(emu, 0);
    }
    let lenient = get(emu, f).lenient;
    let formatter = cf_formatter(emu, f)?;
    let s = foundation::as_cf_string(emu, s)?;
    let len = cf(emu, "_CFStringGetLength", &[s])?;
    let range = emu.alloc(16)?;
    emu.write_u64(range, 0)?;
    emu.write_u64(range + 8, len)?;
    let n = cf(emu, "_CFNumberFormatterCreateNumberFromString", &[0, formatter, s, range, 0])?;
    // The range comes back as what was parsed: all of it, or it isn't a number.
    let parsed = emu.read_u64(range + 8)?;
    if n == 0 || (!lenient && parsed != len) {
        return ret(emu, 0);
    }
    ret(emu, n)
}

/// A property's current value as the CF formatter reports it (defaults included).
fn property(emu: &mut Emu, key: &'static str) -> Result<()> {
    let f = emu.arg(0);
    let formatter = cf_formatter(emu, f)?;
    let k = cf_data_pub(emu, key)?;
    let v = cf(emu, "_CFNumberFormatterCopyProperty", &[formatter, k])?;
    ret(emu, v)
}

/// An integer property (fraction digits and the like), as an NSUInteger.
fn integer_property(emu: &mut Emu, key: &'static str) -> Result<()> {
    property(emu, key)?;
    let v = emu.cpu.x[0];
    let n = if v == 0 { 0 } else { foundation::number_value(emu, v)?.0 as u64 };
    ret(emu, n)
}

fn set_integer(emu: &mut Emu, key: &'static str) -> Result<()> {
    let (f, v) = (emu.arg(0), emu.arg(2) as i64);
    let n = foundation::new_number_i64(emu, v)?;
    set_prop(emu, f, key, n);
    Ok(())
}

fn set_object(emu: &mut Emu, key: &'static str) -> Result<()> {
    let (f, v) = (emu.arg(0), emu.arg(2));
    set_prop(emu, f, key, v);
    Ok(())
}

fn set_bool(emu: &mut Emu, key: &'static str) -> Result<()> {
    let (f, v) = (emu.arg(0), emu.arg(2) & 0xff != 0);
    let b = foundation::boolean(emu, v);
    set_prop(emu, f, key, b);
    Ok(())
}

fn bool_property(emu: &mut Emu, key: &'static str) -> Result<()> {
    property(emu, key)?;
    let v = emu.cpu.x[0];
    let b = v != 0 && crate::objc::send(emu, v, "boolValue", &[])? & 0xff != 0;
    ret(emu, b as u64)
}

const NF: &str = "NSNumberFormatter";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (NF, false, "init", |e| {
        let f = e.arg(0);
        e.numberformatters.formatters.insert(f, Formatter::default());
        ret(e, f)
    }),
    (NF, false, "numberStyle", |e| {
        let s = get(e, e.arg(0)).style;
        ret(e, s)
    }),
    (NF, false, "setNumberStyle:", |e| {
        let (f, s) = (e.arg(0), e.arg(2));
        e.numberformatters.formatters.entry(f).or_default().style = s;
        Ok(())
    }),
    (NF, false, "locale", |e| {
        let id = get(e, e.arg(0)).locale;
        let cls = crate::objc::host_class(e, "NSLocale")?;
        let l = match id {
            Some(id) => {
                let s = foundation::new_nsstring(e, &id)?;
                crate::objc::send(e, cls, "localeWithLocaleIdentifier:", &[s])?
            }
            None => crate::objc::send(e, cls, "currentLocale", &[])?,
        };
        ret(e, l)
    }),
    (NF, false, "setLocale:", |e| {
        let (f, l) = (e.arg(0), e.arg(2));
        let id = if l == 0 {
            None
        } else {
            let s = crate::objc::send(e, l, "localeIdentifier", &[])?;
            Some(nsstring(e, s)?)
        };
        e.numberformatters.formatters.entry(f).or_default().locale = id;
        Ok(())
    }),
    (NF, false, "isLenient", |e| {
        let l = get(e, e.arg(0)).lenient;
        ret(e, l as u64)
    }),
    (NF, false, "setLenient:", |e| {
        let (f, v) = (e.arg(0), e.arg(2) & 0xff != 0);
        e.numberformatters.formatters.entry(f).or_default().lenient = v;
        Ok(())
    }),
    (NF, false, "stringFromNumber:", string_from_number),
    (NF, false, "stringForObjectValue:", string_from_number),
    (NF, false, "numberFromString:", number_from_string),
    (NF, true, "localizedStringFromNumber:numberStyle:", |e| {
        let (n, style) = (e.arg(2), e.arg(3));
        let cls = crate::objc::host_class(e, NF)?;
        let f = crate::objc::send(e, cls, "new", &[])?;
        e.numberformatters.formatters.insert(f, Formatter { style, ..Default::default() });
        e.cpu.x[0] = f;
        e.cpu.x[2] = n;
        string_from_number(e)
    }),
    (NF, false, "minimumFractionDigits", |e| integer_property(e, "_kCFNumberFormatterMinFractionDigits")),
    (NF, false, "setMinimumFractionDigits:", |e| set_integer(e, "_kCFNumberFormatterMinFractionDigits")),
    (NF, false, "maximumFractionDigits", |e| integer_property(e, "_kCFNumberFormatterMaxFractionDigits")),
    (NF, false, "setMaximumFractionDigits:", |e| set_integer(e, "_kCFNumberFormatterMaxFractionDigits")),
    (NF, false, "minimumIntegerDigits", |e| integer_property(e, "_kCFNumberFormatterMinIntegerDigits")),
    (NF, false, "setMinimumIntegerDigits:", |e| set_integer(e, "_kCFNumberFormatterMinIntegerDigits")),
    (NF, false, "maximumIntegerDigits", |e| integer_property(e, "_kCFNumberFormatterMaxIntegerDigits")),
    (NF, false, "setMaximumIntegerDigits:", |e| set_integer(e, "_kCFNumberFormatterMaxIntegerDigits")),
    (NF, false, "maximumSignificantDigits", |e| integer_property(e, "_kCFNumberFormatterMaxSignificantDigits")),
    (NF, false, "setMaximumSignificantDigits:", |e| set_integer(e, "_kCFNumberFormatterMaxSignificantDigits")),
    (NF, false, "usesSignificantDigits", |e| bool_property(e, "_kCFNumberFormatterUseSignificantDigits")),
    (NF, false, "setUsesSignificantDigits:", |e| set_bool(e, "_kCFNumberFormatterUseSignificantDigits")),
    (NF, false, "usesGroupingSeparator", |e| bool_property(e, "_kCFNumberFormatterUseGroupingSeparator")),
    (NF, false, "setUsesGroupingSeparator:", |e| set_bool(e, "_kCFNumberFormatterUseGroupingSeparator")),
    (NF, false, "roundingMode", |e| integer_property(e, "_kCFNumberFormatterRoundingMode")),
    (NF, false, "setRoundingMode:", |e| set_integer(e, "_kCFNumberFormatterRoundingMode")),
    (NF, false, "decimalSeparator", |e| property(e, "_kCFNumberFormatterDecimalSeparator")),
    (NF, false, "setDecimalSeparator:", |e| set_object(e, "_kCFNumberFormatterDecimalSeparator")),
    (NF, false, "groupingSeparator", |e| property(e, "_kCFNumberFormatterGroupingSeparator")),
    (NF, false, "setGroupingSeparator:", |e| set_object(e, "_kCFNumberFormatterGroupingSeparator")),
    (NF, false, "currencyCode", |e| property(e, "_kCFNumberFormatterCurrencyCode")),
    (NF, false, "setCurrencyCode:", |e| set_object(e, "_kCFNumberFormatterCurrencyCode")),
    (NF, false, "currencySymbol", |e| property(e, "_kCFNumberFormatterCurrencySymbol")),
    (NF, false, "setCurrencySymbol:", |e| set_object(e, "_kCFNumberFormatterCurrencySymbol")),
    (NF, false, "positivePrefix", |e| property(e, "_kCFNumberFormatterPositivePrefix")),
    (NF, false, "setPositivePrefix:", |e| set_object(e, "_kCFNumberFormatterPositivePrefix")),
    (NF, false, "negativePrefix", |e| property(e, "_kCFNumberFormatterNegativePrefix")),
    (NF, false, "setNegativePrefix:", |e| set_object(e, "_kCFNumberFormatterNegativePrefix")),
    (NF, false, "positiveSuffix", |e| property(e, "_kCFNumberFormatterPositiveSuffix")),
    (NF, false, "setPositiveSuffix:", |e| set_object(e, "_kCFNumberFormatterPositiveSuffix")),
    (NF, false, "negativeSuffix", |e| property(e, "_kCFNumberFormatterNegativeSuffix")),
    (NF, false, "setNegativeSuffix:", |e| set_object(e, "_kCFNumberFormatterNegativeSuffix")),
];

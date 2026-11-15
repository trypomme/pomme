//! Measurements: NSUnit and its dimensions (temperature, length, mass),
//! NSMeasurement, and NSMeasurementFormatter, which writes a measurement as
//! the locale does, by ICU's measure formatter (Apple's ICU, inside the
//! sysroot's CoreFoundation). Unless told to keep the unit, it converts to the
//! region's own, as recorded in fixtures/locale: temperatures to the unit ICU
//! prefers ("68°F" for 20 °C in the US); lengths and masses to the same-scale
//! unit of the region's measurement system (5 km is "3.107 mi" in the US and
//! the UK; 2 kg is "4.409 lb" in the US but stays "2 kg" in the UK).

use crate::foundation::{self, cf, nsstring};
use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// A unit: ICU's code for it (UAMeasureUnit), its category, its symbol, and
/// how to reach the category's base unit (base = value × factor + offset).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Unit {
    code: u32,
    category: &'static str,
    symbol: &'static str,
    factor: f64,
    offset: f64,
}

const fn unit(code: u32, category: &'static str, symbol: &'static str, factor: f64, offset: f64) -> Unit {
    Unit { code, category, symbol, factor, offset }
}

const TEMPERATURE: u32 = 10 << 8;
const LENGTH: u32 = 5 << 8;
const MASS: u32 = 6 << 8;

/// The units pomme knows: class, class method, unit. (Base units: degrees
/// Celsius, metres, kilograms.)
const UNITS: &[(&str, &str, Unit)] = &[
    ("NSUnitTemperature", "celsius", unit(TEMPERATURE, "temperature", "°C", 1.0, 0.0)),
    ("NSUnitTemperature", "fahrenheit", unit(TEMPERATURE + 1, "temperature", "°F", 5.0 / 9.0, -32.0 * 5.0 / 9.0)),
    ("NSUnitTemperature", "kelvin", unit(TEMPERATURE + 2, "temperature", "K", 1.0, -273.15)),
    ("NSUnitLength", "meters", unit(LENGTH, "length", "m", 1.0, 0.0)),
    ("NSUnitLength", "centimeters", unit(LENGTH + 1, "length", "cm", 0.01, 0.0)),
    ("NSUnitLength", "kilometers", unit(LENGTH + 2, "length", "km", 1000.0, 0.0)),
    ("NSUnitLength", "millimeters", unit(LENGTH + 3, "length", "mm", 0.001, 0.0)),
    ("NSUnitLength", "feet", unit(LENGTH + 5, "length", "ft", 0.3048, 0.0)),
    ("NSUnitLength", "inches", unit(LENGTH + 6, "length", "in", 0.0254, 0.0)),
    ("NSUnitLength", "miles", unit(LENGTH + 7, "length", "mi", 1609.344, 0.0)),
    ("NSUnitLength", "yards", unit(LENGTH + 8, "length", "yd", 0.9144, 0.0)),
    ("NSUnitMass", "grams", unit(MASS, "mass", "g", 0.001, 0.0)),
    ("NSUnitMass", "kilograms", unit(MASS + 1, "mass", "kg", 1.0, 0.0)),
    ("NSUnitMass", "ounces", unit(MASS + 2, "mass", "oz", 0.028_349_523_125, 0.0)),
    ("NSUnitMass", "pounds", unit(MASS + 3, "mass", "lb", 0.453_592_37, 0.0)),
];

fn by_code(code: u32) -> Option<Unit> {
    UNITS.iter().map(|&(_, _, u)| u).find(|u| u.code == code)
}

impl Unit {
    fn convert(&self, value: f64, to: &Unit) -> f64 {
        (value * self.factor + self.offset - to.offset) / to.factor
    }
}

/// `unitOptions`: NSMeasurementFormatterUnitOptions.
const PROVIDED_UNIT: u64 = 1;
const TEMPERATURE_WITHOUT_UNIT: u64 = 4;

#[derive(Clone, Default)]
struct Formatter {
    /// The locale's identifier; None is the device's.
    locale: Option<String>,
    unit_options: u64,
    /// NSFormattingUnitStyle: 1 short, 2 medium (the default), 3 long.
    unit_style: u64,
}

#[derive(Clone, Default)]
pub struct State {
    /// Unit objects (one per kind, made on first use) and the unit each stands for.
    units: HashMap<u64, Unit>,
    unit_objects: HashMap<u32, u64>,
    /// NSMeasurement → (value, unit object).
    measurements: HashMap<u64, (f64, u64)>,
    formatters: HashMap<u64, Formatter>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn unit_object(emu: &mut Emu, class: &str, u: Unit) -> Result<u64> {
    if let Some(&o) = emu.measurements.unit_objects.get(&u.code) {
        return Ok(o);
    }
    let cls = host_class(emu, class)?;
    let o = objc::create_instance(emu, cls, 0)?;
    emu.measurements.units.insert(o, u);
    emu.measurements.unit_objects.insert(u.code, o);
    Ok(o)
}

fn new_measurement(emu: &mut Emu, value: f64, unit: u64) -> Result<u64> {
    let cls = host_class(emu, "NSMeasurement")?;
    let m = objc::create_instance(emu, cls, 0)?;
    emu.measurements.measurements.insert(m, (value, unit));
    Ok(m)
}

/// The same-scale unit in the other system: metric ↔ US customary.
const COUNTERPARTS: &[(u32, u32)] = &[
    (LENGTH + 2, LENGTH + 7), // kilometres ↔ miles
    (LENGTH, LENGTH + 5),     // metres ↔ feet
    (LENGTH + 1, LENGTH + 6), // centimetres ↔ inches
    (LENGTH + 3, LENGTH + 6), // millimetres → inches
    (MASS + 1, MASS + 3),     // kilograms ↔ pounds
    (MASS, MASS + 2),         // grams ↔ ounces
];

fn is_metric(u: &Unit) -> bool {
    !matches!(u.code, c if c == LENGTH + 5 || c == LENGTH + 6 || c == LENGTH + 7 || c == LENGTH + 8 || c == MASS + 2 || c == MASS + 3)
}

/// The unit a length or mass is shown in, for a measurement system ("U.S.",
/// "U.K." or "Metric"): the UK uses miles for distance and metric for the rest.
fn for_system(u: &Unit, system: &str) -> Unit {
    let want_metric = match system {
        "U.S." => false,
        "U.K." => !(u.category == "length" && matches!(u.code, c if c == LENGTH + 2 || c == LENGTH + 7)),
        _ => true,
    };
    if is_metric(u) == want_metric {
        return *u;
    }
    let other = COUNTERPARTS.iter().find_map(|&(m, i)| {
        if want_metric { (i == u.code).then_some(m) } else { (m == u.code).then_some(i) }
    });
    other.and_then(by_code).unwrap_or(*u)
}

/// ICU's preferred unit for a category in a locale (`uameasfmt_getUnitsForUsage`, usage "default").
fn preferred(emu: &mut Emu, locale: &str, category: &str) -> Result<Option<Unit>> {
    let (loc, cat, usage) = (emu.alloc_cstr(locale)?, emu.alloc_cstr(category)?, emu.alloc_cstr("default")?);
    let (units, status) = (emu.alloc(4 * 4)?, emu.alloc(4)?);
    emu.p.memory.write_u32(status, 0)?;
    let n = cf(emu, "_uameasfmt_getUnitsForUsage", &[loc, cat, usage, units, 4, status])? as i32;
    if n < 1 || emu.p.memory.read_u32(status)? as i32 > 0 {
        return Ok(None);
    }
    Ok(by_code(emu.p.memory.read_u32(units)?))
}

/// The locale's measurement system: "U.S.", "U.K." or "Metric".
fn measurement_system(emu: &mut Emu, locale: &str) -> Result<String> {
    let loc = crate::locale::cf_locale_for(emu, Some(locale))?;
    let key = foundation::cf_data_pub(emu, "_kCFLocaleMeasurementSystem")?;
    let v = cf(emu, "_CFLocaleGetValue", &[loc, key])?;
    if v == 0 { Ok("Metric".into()) } else { nsstring(emu, v) }
}

/// `value` in `unit`, formatted by ICU in `locale` at `width` (UAMeasureFormatWidth).
fn icu_format(emu: &mut Emu, locale: &str, width: u64, value: f64, unit: &Unit) -> Result<Option<String>> {
    let (loc, status) = (emu.alloc_cstr(locale)?, emu.alloc(4)?);
    emu.p.memory.write_u32(status, 0)?;
    let fmt = cf(emu, "_uameasfmt_open", &[loc, width, 0, status])?;
    if fmt == 0 || emu.p.memory.read_u32(status)? as i32 > 0 {
        return Ok(None);
    }
    const CAP: u64 = 256;
    let buf = emu.alloc(CAP * 2)?;
    emu.p.memory.write_u32(status, 0)?;
    emu.cpu.q[0] = value.to_bits() as u128;
    let len = cf(emu, "_uameasfmt_format", &[fmt, unit.code as u64, buf, CAP, status])? as i32;
    cf(emu, "_uameasfmt_close", &[fmt])?;
    if len < 0 || emu.p.memory.read_u32(status)? as i32 > 0 {
        return Ok(None);
    }
    let mut units = Vec::new();
    for i in 0..(len as u64).min(CAP) {
        units.push(emu.p.memory.read_u16(buf + 2 * i)?);
    }
    Ok(Some(String::from_utf16_lossy(&units)))
}

/// `-stringFromMeasurement:`
fn string_from_measurement(emu: &mut Emu) -> Result<()> {
    let (f, m) = (emu.arg(0), emu.arg(2));
    let fm = emu.measurements.formatters.get(&f).cloned().unwrap_or_default();
    let Some(&(value, unit)) = emu.measurements.measurements.get(&m) else { return ret(emu, 0) };
    let Some(&from) = emu.measurements.units.get(&unit) else { return ret(emu, 0) };
    let locale = match fm.locale {
        Some(id) => id,
        None => crate::locale::current_id(emu)?,
    };
    let to = if fm.unit_options & PROVIDED_UNIT != 0 {
        from
    } else if from.category == "temperature" {
        preferred(emu, &locale, from.category)?.filter(|t| t.category == from.category).unwrap_or(from)
    } else {
        let system = measurement_system(emu, &locale)?;
        for_system(&from, &system)
    };
    let value = from.convert(value, &to);
    // NSFormattingUnitStyle → UAMeasureFormatWidth: short → narrow, medium → short, long → wide.
    let width = match fm.unit_style {
        1 => 2,
        3 => 0,
        _ => 1,
    };
    let mut s = icu_format(emu, &locale, width, value, &to)?.unwrap_or_else(|| format!("{value} {}", to.symbol));
    if fm.unit_options & TEMPERATURE_WITHOUT_UNIT != 0 && to.category == "temperature" {
        // "20°C" → "20°": the number and the degree sign.
        s = s.trim_end_matches(|c: char| c.is_alphabetic() || c == '\u{200f}').trim_end().to_string();
    }
    let v = foundation::new_nsstring(emu, &s)?;
    ret(emu, v)
}

fn formatter(emu: &mut Emu) -> &mut Formatter {
    let f = emu.arg(0);
    emu.measurements.formatters.entry(f).or_insert_with(|| Formatter { unit_style: 2, ..Default::default() })
}

pub(crate) fn methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    let mut m: Vec<(&'static str, bool, &'static str, HostFn)> = vec![
        ("NSUnit", false, "symbol", |e| {
            let s = e.measurements.units.get(&e.arg(0)).map_or("", |u| u.symbol);
            let v = foundation::new_nsstring(e, s)?;
            ret(e, v)
        }),
        ("NSDimension", true, "baseUnit", |e| {
            let cls = e.arg(0);
            let name = e.objc.class_name(cls);
            let Some(&(c, _, u)) = UNITS.iter().find(|&&(c, _, u)| c == name && u.factor == 1.0 && u.offset == 0.0) else { return ret(e, 0) };
            let o = unit_object(e, c, u)?;
            ret(e, o)
        }),
        ("NSMeasurement", false, "initWithDoubleValue:unit:", |e| {
            let (m, unit) = (e.arg(0), e.arg(2));
            let value = f64::from_bits(e.cpu.q[0] as u64);
            e.measurements.measurements.insert(m, (value, unit));
            ret(e, m)
        }),
        ("NSMeasurement", false, "doubleValue", |e| {
            let v = e.measurements.measurements.get(&e.arg(0)).map_or(0.0, |m| m.0);
            e.cpu.q[0] = v.to_bits() as u128;
            Ok(())
        }),
        ("NSMeasurement", false, "unit", |e| {
            let u = e.measurements.measurements.get(&e.arg(0)).map_or(0, |m| m.1);
            ret(e, u)
        }),
        ("NSMeasurement", false, "measurementByConvertingToUnit:", |e| {
            let (m, to_obj) = (e.arg(0), e.arg(2));
            let Some(&(value, from_obj)) = e.measurements.measurements.get(&m) else { return ret(e, 0) };
            let (Some(&from), Some(&to)) = (e.measurements.units.get(&from_obj), e.measurements.units.get(&to_obj)) else { return ret(e, 0) };
            let n = new_measurement(e, from.convert(value, &to), to_obj)?;
            ret(e, n)
        }),
        ("NSMeasurementFormatter", false, "init", |e| {
            formatter(e);
            ret(e, e.arg(0))
        }),
        ("NSMeasurementFormatter", false, "unitOptions", |e| {
            let v = formatter(e).unit_options;
            ret(e, v)
        }),
        ("NSMeasurementFormatter", false, "setUnitOptions:", |e| {
            let v = e.arg(2);
            formatter(e).unit_options = v;
            Ok(())
        }),
        ("NSMeasurementFormatter", false, "unitStyle", |e| {
            let v = formatter(e).unit_style;
            ret(e, v)
        }),
        ("NSMeasurementFormatter", false, "setUnitStyle:", |e| {
            let v = e.arg(2);
            formatter(e).unit_style = v;
            Ok(())
        }),
        ("NSMeasurementFormatter", false, "locale", |e| {
            let id = match formatter(e).locale.clone() {
                Some(id) => id,
                None => crate::locale::current_id(e)?,
            };
            let cls = host_class(e, "NSLocale")?;
            let s = foundation::new_nsstring(e, &id)?;
            let l = objc::send(e, cls, "localeWithLocaleIdentifier:", &[s])?;
            ret(e, l)
        }),
        ("NSMeasurementFormatter", false, "setLocale:", |e| {
            let l = e.arg(2);
            let id = if l == 0 {
                None
            } else {
                let s = objc::send(e, l, "localeIdentifier", &[])?;
                Some(nsstring(e, s)?)
            };
            formatter(e).locale = id;
            Ok(())
        }),
        ("NSMeasurementFormatter", false, "stringFromMeasurement:", string_from_measurement),
    ];
    // The class methods for each unit (`+[NSUnitTemperature celsius]`).
    macro_rules! unit_methods {
        ($($i:literal),*) => {
            $(m.push((UNITS[$i].0, true, UNITS[$i].1, |e| {
                let (c, _, u) = UNITS[$i];
                let o = unit_object(e, c, u)?;
                ret(e, o)
            }));)*
        };
    }
    unit_methods!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14);
    debug_assert_eq!(UNITS.len(), 15);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_between_units() {
        let f = |code| by_code(code).unwrap();
        assert!((f(TEMPERATURE).convert(20.0, &f(TEMPERATURE + 1)) - 68.0).abs() < 1e-9);
        assert!((f(LENGTH + 2).convert(5.0, &f(LENGTH + 7)) - 3.106_855_96).abs() < 1e-6);
        assert!((f(MASS + 1).convert(2.0, &f(MASS + 3)) - 4.409_245_24).abs() < 1e-6);
        assert!((f(TEMPERATURE + 2).convert(273.15, &f(TEMPERATURE))).abs() < 1e-9);
    }

    #[test]
    fn picks_the_systems_unit() {
        let f = |code| by_code(code).unwrap();
        let (km, mi, kg, lb) = (f(LENGTH + 2), f(LENGTH + 7), f(MASS + 1), f(MASS + 3));
        assert_eq!(for_system(&km, "U.S."), mi);
        assert_eq!(for_system(&kg, "U.S."), lb);
        assert_eq!(for_system(&km, "U.K."), mi);
        assert_eq!(for_system(&kg, "U.K."), kg);
        assert_eq!(for_system(&km, "Metric"), km);
        assert_eq!(for_system(&mi, "Metric"), km);
        assert_eq!(for_system(&f(LENGTH + 1), "U.S."), f(LENGTH + 6));
    }
}

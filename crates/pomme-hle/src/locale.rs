//! NSLocale, and the device's language and region. The device is set to
//! English (United States), like a fresh Simulator, unless [`Settings`] say
//! otherwise (`pomme run --language ja-JP --region JP`). The settings are
//! written into the global user defaults domain as iOS keeps them
//! (`AppleLanguages`, `AppleLocale`, ...), and everything reads them from
//! there, so a launch argument such as `-AppleLanguages '(fr)'` wins, as on
//! iOS. Locales parse BCP 47-ish identifiers ("en-US", "zh-Hans-CN",
//! "en_US_POSIX") into language, script, region and variant.

use crate::foundation::{self, cf, nsstring};
use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// The locale of a device nobody has set up.
pub const CURRENT: &str = "en_US";

/// The device's language and region, as the Settings app sets them.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The preferred languages, in order (BCP 47: `ja-JP`, `en-AU`, `zh-Hans`).
    pub languages: Vec<String>,
    /// The region (`US`, `AU`, `JP`, ...).
    pub region: String,
    /// The device's time zone (an IANA name); None is GMT.
    pub time_zone: Option<String>,
    /// iOS's 24-Hour Time switch, when the user has moved it away from the
    /// region's choice: Some(true) forces 24 hours, Some(false) 12.
    pub force_24_hour: Option<bool>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { languages: vec!["en-US".into()], region: "US".into(), time_zone: None, force_24_hour: None }
    }
}

impl Settings {
    /// The locale iOS makes from them (`AppleLocale`): the first language,
    /// with its script but not its region, and the region (`ja-JP`, `JP` → `ja_JP`).
    pub fn locale_id(&self) -> String {
        let first = parse(self.languages.first().map_or("en", String::as_str));
        let mut id = first.language;
        if let Some(script) = first.script {
            id = format!("{id}-{script}");
        }
        format!("{id}_{}", self.region.to_uppercase())
    }
}

/// Whether pomme knows a time zone by this IANA name.
pub fn is_time_zone(name: &str) -> bool {
    crate::calendar::TimeZoneInfo::named(name).is_some()
}

#[derive(Clone, Default)]
pub struct State {
    /// The device's language and region.
    pub settings: Settings,
    /// NSLocale → its identifier.
    locales: HashMap<u64, String>,
    /// The device's CFLocale (with the global domain as its preferences), for its identifier.
    current_cf: Option<(String, u64)>,
    /// The device locale's week rules (first weekday, minimum days in the first week), for its identifier.
    week: Option<(String, (i64, i64))>,
}

#[derive(Debug, PartialEq, Default)]
pub struct Parts {
    pub language: String,
    pub script: Option<String>,
    pub region: Option<String>,
    pub variant: Option<String>,
}

/// Split an identifier: a 4-letter part is the script, 2 letters or 3 digits the region.
pub fn parse(id: &str) -> Parts {
    let mut it = id.split(['-', '_']).filter(|p| !p.is_empty());
    let mut p = Parts { language: it.next().unwrap_or("").to_lowercase(), ..Default::default() };
    for part in it {
        if part.len() == 4 && part.chars().all(|c| c.is_ascii_alphabetic()) && p.script.is_none() && p.region.is_none() {
            p.script = Some(format!("{}{}", part[..1].to_uppercase(), part[1..].to_lowercase()));
        } else if (part.len() == 2 && part.chars().all(|c| c.is_ascii_alphabetic())) || (part.len() == 3 && part.chars().all(|c| c.is_ascii_digit())) {
            if p.region.is_none() {
                p.region = Some(part.to_uppercase());
            }
        } else {
            p.variant = Some(part.to_uppercase());
        }
    }
    p
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_str(emu: &mut Emu, s: Option<&str>) -> Result<()> {
    let v = match s {
        Some(s) => foundation::new_nsstring(emu, s)?,
        None => 0,
    };
    ret(emu, v)
}

fn make(emu: &mut Emu, id: &str) -> Result<u64> {
    let cls = host_class(emu, "NSLocale")?;
    let l = objc::create_instance(emu, cls, 0)?;
    emu.locales.locales.insert(l, id.to_string());
    Ok(l)
}

fn id_of(emu: &Emu, l: u64) -> String {
    emu.locales.locales.get(&l).cloned().unwrap_or(CURRENT.into())
}

/// A string device setting (`AppleLocale`), from the launch arguments or the global domain.
fn device_string(emu: &mut Emu, key: &str) -> Result<Option<String>> {
    let v = crate::userdefaults::device_value(emu, key)?;
    let cls = host_class(emu, "NSString")?;
    if v == 0 || objc::send(emu, v, "isKindOfClass:", &[cls])? & 0xff == 0 {
        return Ok(None);
    }
    Ok(Some(nsstring(emu, v)?))
}

/// The device locale's identifier, as `currentLocale` reports it.
pub fn current_id(emu: &mut Emu) -> Result<String> {
    Ok(match device_string(emu, "AppleLocale")? {
        Some(id) => id,
        None => emu.locales.settings.locale_id(),
    })
}

/// iOS's 24-Hour Time switch: Some(true) forces 24 hours, Some(false) 12, None follows the region.
pub(crate) fn forced_hours(emu: &mut Emu) -> Result<Option<bool>> {
    for (key, force_24) in [("AppleICUForce24HourTime", true), ("AppleICUForce12HourTime", false)] {
        let v = crate::userdefaults::device_value(emu, key)?;
        if v != 0 && v == emu.fnd.true_obj {
            return Ok(Some(force_24));
        }
    }
    Ok(None)
}

/// The preferred languages (an NSArray of strings), as `preferredLanguages` reports them.
fn preferred_languages(emu: &mut Emu) -> Result<u64> {
    let v = crate::userdefaults::device_value(emu, "AppleLanguages")?;
    let cls = host_class(emu, "NSArray")?;
    if v != 0 && objc::send(emu, v, "isKindOfClass:", &[cls])? & 0xff != 0 {
        return Ok(v);
    }
    let mut langs = Vec::new();
    for l in emu.locales.settings.languages.clone() {
        langs.push(foundation::new_nsstring(emu, &l)?);
    }
    foundation::new_array_of(emu, &langs)
}

/// The device's CFLocale: the current identifier, with the global domain as
/// its preferences (so formatters see the 24-Hour Time switch, as on iOS).
/// The sysroot's CoreFoundation reads no preferences for it, so pomme makes it.
pub(crate) fn current_cf(emu: &mut Emu) -> Result<u64> {
    let id = current_id(emu)?;
    if let Some((cached, l)) = &emu.locales.current_cf {
        if *cached == id {
            return Ok(*l);
        }
    }
    let name = foundation::new_nsstring(emu, &id)?;
    let l = cf(emu, "__CFLocaleCopyAsIfCurrent", &[name])?;
    // struct __CFLocale { CFRuntimeBase (16 bytes); _identifier; _cache; _prefs; … }
    let prefs = crate::userdefaults::global_domain(emu)?;
    cf(emu, "_CFRetain", &[prefs])?;
    emu.write_u64(l + 32, prefs)?;
    emu.locales.current_cf = Some((id, l));
    Ok(l)
}

/// The device locale's week rules: (first weekday, 1 = Sunday; minimum days
/// in the first week), from ICU's Gregorian calendar for that locale.
pub(crate) fn week_rules(emu: &mut Emu) -> Result<(i64, i64)> {
    let id = current_id(emu)?;
    if let Some((cached, rules)) = &emu.locales.week {
        if *cached == id {
            return Ok(*rules);
        }
    }
    let gregorian = foundation::cf_data_pub(emu, "_kCFGregorianCalendar")?;
    let cal = cf(emu, "_CFCalendarCreateWithIdentifier", &[0, gregorian])?;
    let loc = current_cf(emu)?;
    cf(emu, "_CFCalendarSetLocale", &[cal, loc])?;
    let first = cf(emu, "_CFCalendarGetFirstWeekday", &[cal])? as i64;
    let min = cf(emu, "_CFCalendarGetMinimumDaysInFirstWeek", &[cal])? as i64;
    let rules = (first.clamp(1, 7), min.clamp(1, 7));
    emu.locales.week = Some((id, rules));
    Ok(rules)
}

/// The CFLocale for an NSLocale (the device's own when it's the current one).
pub(crate) fn cf_locale(emu: &mut Emu, l: u64) -> Result<u64> {
    let id = id_of(emu, l);
    cf_locale_for(emu, Some(&id))
}

/// The CFLocale for an identifier; None is the device's.
pub(crate) fn cf_locale_for(emu: &mut Emu, id: Option<&str>) -> Result<u64> {
    let current = current_id(emu)?;
    match id {
        None => current_cf(emu),
        Some(id) if id == current => current_cf(emu),
        Some(id) => {
            let id = foundation::new_nsstring(emu, id)?;
            cf(emu, "_CFLocaleCreate", &[0, id])
        }
    }
}

/// `CFLocaleCopyCurrent()` → the device's locale.
fn hook_copy_current(emu: &mut Emu) -> Result<()> {
    let l = current_cf(emu)?;
    cf(emu, "_CFRetain", &[l])?;
    ret(emu, l)
}

/// `CFLocaleCopyPreferredLanguages()` → the preferred languages.
fn hook_copy_preferred_languages(emu: &mut Emu) -> Result<()> {
    let a = preferred_languages(emu)?;
    let copy = cf(emu, "_CFArrayCreateCopy", &[0, a])?;
    ret(emu, copy)
}

/// Make CoreFoundation's current locale and language list the device's.
pub(crate) fn hook_cf(emu: &mut Emu) -> Result<()> {
    for (sym, f) in [("_CFLocaleCopyCurrent", hook_copy_current as HostFn), ("_CFLocaleCopyPreferredLanguages", hook_copy_preferred_languages)] {
        let a = foundation::cf_sym(emu, sym)?;
        emu.hook(a, f)?;
    }
    Ok(())
}

/// The NSLocale…Key constants (their own names in pomme) and CoreFoundation's keys for them.
const KEYS: &[(&str, &str)] = &[
    ("NSLocaleIdentifier", "_kCFLocaleIdentifier"),
    ("NSLocaleLanguageCode", "_kCFLocaleLanguageCode"),
    ("NSLocaleCountryCode", "_kCFLocaleCountryCode"),
    ("NSLocaleScriptCode", "_kCFLocaleScriptCode"),
    ("NSLocaleVariantCode", "_kCFLocaleVariantCode"),
    ("NSLocaleExemplarCharacterSet", "_kCFLocaleExemplarCharacterSet"),
    ("NSLocaleCalendar", "_kCFLocaleCalendar"),
    ("NSLocaleCollationIdentifier", "_kCFLocaleCollationIdentifier"),
    ("NSLocaleUsesMetricSystem", "_kCFLocaleUsesMetricSystem"),
    ("NSLocaleMeasurementSystem", "_kCFLocaleMeasurementSystem"),
    ("NSLocaleDecimalSeparator", "_kCFLocaleDecimalSeparator"),
    ("NSLocaleGroupingSeparator", "_kCFLocaleGroupingSeparator"),
    ("NSLocaleCurrencySymbol", "_kCFLocaleCurrencySymbol"),
    ("NSLocaleCurrencyCode", "_kCFLocaleCurrencyCode"),
    ("NSLocaleCollatorIdentifier", "_kCFLocaleCollatorIdentifier"),
    ("NSLocaleQuotationBeginDelimiterKey", "_kCFLocaleQuotationBeginDelimiterKey"),
    ("NSLocaleQuotationEndDelimiterKey", "_kCFLocaleQuotationEndDelimiterKey"),
    ("NSLocaleAlternateQuotationBeginDelimiterKey", "_kCFLocaleAlternateQuotationBeginDelimiterKey"),
    ("NSLocaleAlternateQuotationEndDelimiterKey", "_kCFLocaleAlternateQuotationEndDelimiterKey"),
];

/// CoreFoundation's key for an NSLocale key (a name in pomme, or CF's own key string).
fn cf_key(emu: &mut Emu, key: &str) -> Result<Option<u64>> {
    for &(ns, cf_name) in KEYS {
        if key == ns || key == format!("{}Key", &cf_name[1..]) || key == &cf_name[1..] {
            return Ok(Some(foundation::cf_data_pub(emu, cf_name)?));
        }
    }
    Ok(None)
}

/// The locale's value for a CF key symbol (`_kCFLocaleCurrencyCode`), or 0.
fn value(emu: &mut Emu, l: u64, cf_name: &'static str) -> Result<u64> {
    let loc = cf_locale(emu, l)?;
    let key = foundation::cf_data_pub(emu, cf_name)?;
    cf(emu, "_CFLocaleGetValue", &[loc, key])
}

fn ret_value(emu: &mut Emu, cf_name: &'static str) -> Result<()> {
    let v = value(emu, emu.arg(0), cf_name)?;
    ret(emu, v)
}

/// `objectForKey:`: CoreFoundation's value for the key.
fn object_for_key(emu: &mut Emu) -> Result<()> {
    let (l, key) = (emu.arg(0), emu.arg(2));
    let key = nsstring(emu, key)?;
    let v = match cf_key(emu, &key)? {
        Some(k) => {
            let loc = cf_locale(emu, l)?;
            cf(emu, "_CFLocaleGetValue", &[loc, k])?
        }
        None => 0,
    };
    ret(emu, v)
}

/// A display name for `value` of the CF key, in locale `l`'s language.
fn display_name(emu: &mut Emu, l: u64, cf_name: &'static str, value: u64) -> Result<u64> {
    if value == 0 {
        return Ok(0);
    }
    let loc = cf_locale(emu, l)?;
    let key = foundation::cf_data_pub(emu, cf_name)?;
    cf(emu, "_CFLocaleCopyDisplayNameForPropertyValue", &[loc, key, value])
}

const L: &str = "NSLocale";

/// The date format ICU picks for a template (`yMd`, `jmm`) in a locale (None: the device's).
pub(crate) fn format_from_template(emu: &mut Emu, template: &str, locale: Option<&str>) -> Result<String> {
    let t = foundation::new_nsstring(emu, template)?;
    let loc = cf_locale_for(emu, locale)?;
    let f = cf(emu, "_CFDateFormatterCreateDateFormatFromTemplate", &[0, t, 0, loc])?;
    if f == 0 {
        return Ok(template.to_string());
    }
    nsstring(emu, f)
}

/// `+[NSDateFormatter dateFormatFromTemplate:options:locale:]`, by CoreFoundation (ICU).
fn date_format_from_template(emu: &mut Emu) -> Result<()> {
    let (template, options, l) = (emu.arg(2), emu.arg(3), emu.arg(4));
    if template == 0 {
        return ret(emu, 0);
    }
    let loc = if l == 0 { current_cf(emu)? } else { cf_locale(emu, l)? };
    let f = cf(emu, "_CFDateFormatterCreateDateFormatFromTemplate", &[0, template, options, loc])?;
    ret(emu, f)
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (L, true, "preferredLanguages", |e| {
        let a = preferred_languages(e)?;
        ret(e, a)
    }),
    (L, true, "currentLocale", current),
    (L, true, "autoupdatingCurrentLocale", current),
    (L, true, "systemLocale", |e| {
        let l = make(e, "")?;
        ret(e, l)
    }),
    (L, true, "localeWithLocaleIdentifier:", |e| {
        let id = nsstring(e, e.arg(2))?;
        let l = make(e, &id)?;
        ret(e, l)
    }),
    (L, true, "availableLocaleIdentifiers", |e| {
        let a = cf(e, "_CFLocaleCopyAvailableLocaleIdentifiers", &[])?;
        ret(e, a)
    }),
    (L, true, "ISOLanguageCodes", |e| {
        let a = cf(e, "_CFLocaleCopyISOLanguageCodes", &[])?;
        ret(e, a)
    }),
    (L, true, "ISOCountryCodes", |e| {
        let a = cf(e, "_CFLocaleCopyISOCountryCodes", &[])?;
        ret(e, a)
    }),
    (L, true, "ISOCurrencyCodes", |e| {
        let a = cf(e, "_CFLocaleCopyISOCurrencyCodes", &[])?;
        ret(e, a)
    }),
    (L, true, "canonicalLocaleIdentifierFromString:", |e| {
        let s = cf(e, "_CFLocaleCreateCanonicalLocaleIdentifierFromString", &[0, e.arg(2)])?;
        ret(e, s)
    }),
    (L, true, "canonicalLanguageIdentifierFromString:", |e| {
        let s = cf(e, "_CFLocaleCreateCanonicalLanguageIdentifierFromString", &[0, e.arg(2)])?;
        ret(e, s)
    }),
    (L, true, "characterDirectionForLanguage:", |e| {
        let d = cf(e, "_CFLocaleGetLanguageCharacterDirection", &[e.arg(2)])?;
        ret(e, d)
    }),
    (L, true, "lineDirectionForLanguage:", |e| {
        let d = cf(e, "_CFLocaleGetLanguageLineDirection", &[e.arg(2)])?;
        ret(e, d)
    }),
    (L, true, "componentsFromLocaleIdentifier:", |e| {
        let id = nsstring(e, e.arg(2))?;
        let p = parse(&id);
        let mut pairs = Vec::new();
        for (k, v) in [("kCFLocaleLanguageCodeKey", Some(p.language)), ("kCFLocaleScriptCodeKey", p.script), ("kCFLocaleCountryCodeKey", p.region), ("kCFLocaleVariantCodeKey", p.variant)] {
            if let Some(v) = v {
                let k = foundation::new_nsstring(e, k)?;
                let v = foundation::new_nsstring(e, &v)?;
                pairs.push((k, v));
            }
        }
        let d = foundation::new_dict_of(e, &pairs)?;
        ret(e, d)
    }),
    (L, false, "initWithLocaleIdentifier:", |e| {
        let (l, id) = (e.arg(0), e.arg(2));
        let id = nsstring(e, id)?;
        e.locales.locales.insert(l, id);
        ret(e, l)
    }),
    (L, false, "localeIdentifier", |e| {
        let id = id_of(e, e.arg(0));
        ret_str(e, Some(&id))
    }),
    (L, false, "objectForKey:", object_for_key),
    (L, false, "displayNameForKey:value:", |e| {
        let (l, key, v) = (e.arg(0), e.arg(2), e.arg(3));
        let key = nsstring(e, key)?;
        let n = match cf_key(e, &key)? {
            Some(k) if v != 0 => {
                let loc = cf_locale(e, l)?;
                cf(e, "_CFLocaleCopyDisplayNameForPropertyValue", &[loc, k, v])?
            }
            _ => 0,
        };
        ret(e, n)
    }),
    (L, false, "localizedStringForLocaleIdentifier:", |e| {
        let n = display_name(e, e.arg(0), "_kCFLocaleIdentifier", e.arg(2))?;
        ret(e, n)
    }),
    (L, false, "localizedStringForLanguageCode:", |e| {
        let n = display_name(e, e.arg(0), "_kCFLocaleLanguageCode", e.arg(2))?;
        ret(e, n)
    }),
    (L, false, "localizedStringForCountryCode:", |e| {
        let n = display_name(e, e.arg(0), "_kCFLocaleCountryCode", e.arg(2))?;
        ret(e, n)
    }),
    (L, false, "localizedStringForScriptCode:", |e| {
        let n = display_name(e, e.arg(0), "_kCFLocaleScriptCode", e.arg(2))?;
        ret(e, n)
    }),
    (L, false, "localizedStringForCurrencyCode:", |e| {
        let n = display_name(e, e.arg(0), "_kCFLocaleCurrencyCode", e.arg(2))?;
        ret(e, n)
    }),
    (L, false, "languageCode", |e| {
        let p = parse(&id_of(e, e.arg(0)));
        ret_str(e, Some(&p.language))
    }),
    (L, false, "countryCode", |e| {
        let p = parse(&id_of(e, e.arg(0)));
        ret_str(e, p.region.as_deref())
    }),
    (L, false, "regionCode", |e| {
        let p = parse(&id_of(e, e.arg(0)));
        ret_str(e, p.region.as_deref())
    }),
    (L, false, "scriptCode", |e| {
        let p = parse(&id_of(e, e.arg(0)));
        ret_str(e, p.script.as_deref())
    }),
    (L, false, "variantCode", |e| {
        let p = parse(&id_of(e, e.arg(0)));
        ret_str(e, p.variant.as_deref())
    }),
    (L, false, "calendarIdentifier", |e| ret_value(e, "_kCFLocaleCalendarIdentifier")),
    (L, false, "decimalSeparator", |e| ret_value(e, "_kCFLocaleDecimalSeparator")),
    (L, false, "groupingSeparator", |e| ret_value(e, "_kCFLocaleGroupingSeparator")),
    (L, false, "currencyCode", |e| ret_value(e, "_kCFLocaleCurrencyCode")),
    (L, false, "currencySymbol", |e| ret_value(e, "_kCFLocaleCurrencySymbol")),
    (L, false, "collationIdentifier", |e| ret_value(e, "_kCFLocaleCollationIdentifier")),
    (L, false, "collatorIdentifier", |e| ret_value(e, "_kCFLocaleCollatorIdentifier")),
    (L, false, "exemplarCharacterSet", |e| ret_value(e, "_kCFLocaleExemplarCharacterSet")),
    (L, false, "quotationBeginDelimiter", |e| ret_value(e, "_kCFLocaleQuotationBeginDelimiterKey")),
    (L, false, "quotationEndDelimiter", |e| ret_value(e, "_kCFLocaleQuotationEndDelimiterKey")),
    (L, false, "alternateQuotationBeginDelimiter", |e| ret_value(e, "_kCFLocaleAlternateQuotationBeginDelimiterKey")),
    (L, false, "alternateQuotationEndDelimiter", |e| ret_value(e, "_kCFLocaleAlternateQuotationEndDelimiterKey")),
    (L, false, "usesMetricSystem", |e| {
        let v = value(e, e.arg(0), "_kCFLocaleUsesMetricSystem")?;
        let yes = v != 0 && v == e.fnd.true_obj;
        ret(e, yes as u64)
    }),
    ("NSDateFormatter", true, "dateFormatFromTemplate:options:locale:", date_format_from_template),
];

fn current(emu: &mut Emu) -> Result<()> {
    let id = current_id(emu)?;
    let l = make(emu, &id)?;
    ret(emu, l)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_identifiers() {
        assert_eq!(parse("en-US"), Parts { language: "en".into(), region: Some("US".into()), ..Default::default() });
        assert_eq!(parse("zh-Hans-CN"), Parts { language: "zh".into(), script: Some("Hans".into()), region: Some("CN".into()), variant: None });
        assert_eq!(parse("en_US_POSIX").variant.as_deref(), Some("POSIX"));
        assert_eq!(parse("es-419").region.as_deref(), Some("419"));
    }

}

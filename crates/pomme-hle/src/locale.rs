//! NSLocale: the device is set to English (United States), like a fresh
//! simulator. Locales parse BCP 47-ish identifiers ("en-US", "zh-Hans-CN",
//! "en_US_POSIX") into language, script, region and variant.

use crate::foundation::{self, nsstring};
use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// The device's language list and locale.
pub const PREFERRED_LANGUAGES: &[&str] = &["en-US"];
pub const CURRENT: &str = "en_US";

#[derive(Default)]
pub struct State {
    /// NSLocale → its identifier.
    locales: HashMap<u64, String>,
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

fn current(emu: &mut Emu) -> Result<()> {
    let l = make(emu, CURRENT)?;
    ret(emu, l)
}

/// `objectForKey:` with NSLocale…Code keys (whose values are their own names here).
fn object_for_key(emu: &mut Emu) -> Result<()> {
    let (l, key) = (emu.arg(0), emu.arg(2));
    let key = nsstring(emu, key)?;
    let id = id_of(emu, l);
    let p = parse(&id);
    let v = if key.ends_with("LanguageCode") {
        Some(p.language)
    } else if key.ends_with("CountryCode") {
        p.region
    } else if key.ends_with("ScriptCode") {
        p.script
    } else if key.ends_with("VariantCode") {
        p.variant
    } else if key.ends_with("Identifier") {
        Some(id)
    } else if key.ends_with("DecimalSeparator") {
        Some(".".into())
    } else if key.ends_with("GroupingSeparator") {
        Some(",".into())
    } else {
        None
    };
    ret_str(emu, v.as_deref())
}

const L: &str = "NSLocale";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (L, true, "preferredLanguages", |e| {
        let mut v = Vec::new();
        for l in PREFERRED_LANGUAGES {
            v.push(foundation::new_nsstring(e, l)?);
        }
        let a = foundation::new_array_of(e, &v)?;
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
        let s = foundation::new_nsstring(e, CURRENT)?;
        let a = foundation::new_array_of(e, &[s])?;
        ret(e, a)
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
    (L, false, "decimalSeparator", |e| ret_str(e, Some("."))),
    (L, false, "groupingSeparator", |e| ret_str(e, Some(","))),
    (L, false, "usesMetricSystem", |e| ret(e, 0)),
];

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

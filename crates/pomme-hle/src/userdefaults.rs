//! NSUserDefaults, as its domains work on iOS: each defaults object reads and
//! writes one persistent domain (the app's, named by its bundle ID, or a
//! suite's), shared by every object that names it; lookups fall back to the
//! global domain (the device's language and locale) and then the registration
//! domain, which `registerDefaults:` fills for the whole process. Values live
//! in memory for the run.

use crate::foundation::{self, cf};
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// The global domain's name.
const GLOBAL: &str = "NSGlobalDomain";

#[derive(Default)]
pub struct State {
    /// Persistent domains by name: mutable CFDictionaries.
    domains: HashMap<String, u64>,
    /// `registerDefaults:`'s values.
    registration: u64,
    standard: u64,
    /// Each defaults object's domain.
    objects: HashMap<u64, String>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// The app's own domain: its bundle identifier.
fn app_domain(emu: &Emu) -> String {
    emu.p.bundle_id.clone().unwrap_or_else(|| "pomme.app".into())
}

/// The domain called `name`, made (empty, or for the global domain, with a
/// fresh English-language device's settings) on first use.
fn domain(emu: &mut Emu, name: &str) -> Result<u64> {
    if let Some(&d) = emu.defaults.domains.get(name) {
        return Ok(d);
    }
    let d = foundation::new_dict(emu)?;
    if name == GLOBAL {
        let en = foundation::new_nsstring(emu, "en")?;
        let langs = foundation::array_from(emu, &[en])?;
        let k = foundation::new_nsstring(emu, "AppleLanguages")?;
        cf(emu, "_CFDictionarySetValue", &[d, k, langs])?;
        let k = foundation::new_nsstring(emu, "AppleLocale")?;
        let v = foundation::new_nsstring(emu, "en_US")?;
        cf(emu, "_CFDictionarySetValue", &[d, k, v])?;
    }
    emu.defaults.domains.insert(name.to_string(), d);
    Ok(d)
}

fn registration(emu: &mut Emu) -> Result<u64> {
    if emu.defaults.registration == 0 {
        emu.defaults.registration = foundation::new_dict(emu)?;
    }
    Ok(emu.defaults.registration)
}

/// The domain a defaults object writes to.
fn own(emu: &mut Emu, ud: u64) -> Result<u64> {
    let name = emu.defaults.objects.get(&ud).cloned().unwrap_or_else(|| app_domain(emu));
    domain(emu, &name)
}

/// The domains a lookup searches, in order.
fn search_list(emu: &mut Emu, ud: u64) -> Result<[u64; 3]> {
    Ok([own(emu, ud)?, domain(emu, GLOBAL)?, registration(emu)?])
}

fn lookup(emu: &mut Emu, ud: u64, key: u64) -> Result<u64> {
    if key == 0 {
        return Ok(0);
    }
    for d in search_list(emu, ud)? {
        let v = cf(emu, "_CFDictionaryGetValue", &[d, key])?;
        if v != 0 {
            return Ok(v);
        }
    }
    Ok(0)
}

fn is_kind(emu: &mut Emu, obj: u64, class: &str) -> Result<bool> {
    let cls = host_class(emu, class)?;
    Ok(obj != 0 && send(emu, obj, "isKindOfClass:", &[cls])? & 0xff != 0)
}

/// `…ForKey:` returning the value only when it's a `class`.
fn typed(emu: &mut Emu, class: &str) -> Result<()> {
    let (ud, key) = (emu.arg(0), emu.arg(2));
    let v = lookup(emu, ud, key)?;
    let v = if is_kind(emu, v, class)? { v } else { 0 };
    ret(emu, v)
}

fn set(emu: &mut Emu, ud: u64, key: u64, value: u64) -> Result<()> {
    if key == 0 {
        return Ok(());
    }
    let d = own(emu, ud)?;
    if value == 0 {
        cf(emu, "_CFDictionaryRemoveValue", &[d, key])?;
    } else {
        cf(emu, "_CFDictionarySetValue", &[d, key, value])?;
    }
    Ok(())
}

fn number(emu: &mut Emu, sel: &str) -> Result<u64> {
    let (ud, key) = (emu.arg(0), emu.arg(2));
    let v = lookup(emu, ud, key)?;
    if v == 0 || !(is_kind(emu, v, "NSNumber")? || is_kind(emu, v, "NSString")?) {
        return Ok(0);
    }
    send(emu, v, sel, &[])
}

fn double(emu: &mut Emu) -> Result<f64> {
    let (ud, key) = (emu.arg(0), emu.arg(2));
    let v = lookup(emu, ud, key)?;
    if v == 0 || !(is_kind(emu, v, "NSNumber")? || is_kind(emu, v, "NSString")?) {
        return Ok(0.0);
    }
    send(emu, v, "doubleValue", &[])?;
    Ok(f64::from_bits(emu.last_q0 as u64))
}

/// A copy of a domain, or nil when it doesn't exist or is empty.
fn domain_copy(emu: &mut Emu, name: &str) -> Result<u64> {
    let Some(&d) = emu.defaults.domains.get(name) else { return Ok(0) };
    if cf(emu, "_CFDictionaryGetCount", &[d])? == 0 {
        return Ok(0);
    }
    cf(emu, "_CFDictionaryCreateCopy", &[0, d])
}

const UD: &str = "NSUserDefaults";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (UD, true, "standardUserDefaults", |e| {
        if e.defaults.standard == 0 {
            let cls = host_class(e, UD)?;
            let ud = objc::create_instance(e, cls, 0)?;
            let name = app_domain(e);
            e.defaults.objects.insert(ud, name);
            e.defaults.standard = ud;
        }
        let ud = e.defaults.standard;
        ret(e, ud)
    }),
    (UD, true, "resetStandardUserDefaults", |_| Ok(())),
    (UD, false, "init", |e| {
        let ud = e.arg(0);
        let name = app_domain(e);
        e.defaults.objects.insert(ud, name);
        ret(e, ud)
    }),
    // nil is the app's domain; the app's own ID and the global domain can't be suites.
    (UD, false, "initWithSuiteName:", |e| {
        let (ud, s) = (e.arg(0), e.arg(2));
        let app = app_domain(e);
        let name = if s == 0 { app.clone() } else { foundation::nsstring(e, s)? };
        if s != 0 && (name == app || name == GLOBAL) {
            return ret(e, 0);
        }
        e.defaults.objects.insert(ud, name);
        ret(e, ud)
    }),
    (UD, false, "objectForKey:", |e| {
        let (ud, key) = (e.arg(0), e.arg(2));
        let v = lookup(e, ud, key)?;
        ret(e, v)
    }),
    (UD, false, "valueForKey:", |e| {
        let (ud, key) = (e.arg(0), e.arg(2));
        let v = lookup(e, ud, key)?;
        ret(e, v)
    }),
    // A number comes back as its string, as Foundation's does.
    (UD, false, "stringForKey:", |e| {
        let (ud, key) = (e.arg(0), e.arg(2));
        let v = lookup(e, ud, key)?;
        let v = if is_kind(e, v, "NSString")? {
            v
        } else if is_kind(e, v, "NSNumber")? {
            send(e, v, "stringValue", &[])?
        } else {
            0
        };
        ret(e, v)
    }),
    (UD, false, "arrayForKey:", |e| typed(e, "NSArray")),
    (UD, false, "stringArrayForKey:", |e| typed(e, "NSArray")),
    (UD, false, "dictionaryForKey:", |e| typed(e, "NSDictionary")),
    (UD, false, "dataForKey:", |e| typed(e, "NSData")),
    // A URL is kept as its string (setURL:forKey:); a plain path string counts too.
    (UD, false, "URLForKey:", |e| {
        let (ud, key) = (e.arg(0), e.arg(2));
        let v = lookup(e, ud, key)?;
        if !is_kind(e, v, "NSString")? {
            return ret(e, 0);
        }
        let s = foundation::nsstring(e, v)?;
        let cls = host_class(e, "NSURL")?;
        let u = if s.starts_with('/') || s.starts_with('~') { send(e, cls, "fileURLWithPath:", &[v])? } else { send(e, cls, "URLWithString:", &[v])? };
        ret(e, u)
    }),
    (UD, false, "setURL:forKey:", |e| {
        let (ud, url, key) = (e.arg(0), e.arg(2), e.arg(3));
        let s = if url == 0 { 0 } else { send(e, url, "absoluteString", &[])? };
        set(e, ud, key, s)
    }),
    (UD, false, "boolForKey:", |e| {
        let b = number(e, "boolValue")? & 0xff;
        ret(e, b)
    }),
    (UD, false, "integerForKey:", |e| {
        let i = number(e, "integerValue")?;
        ret(e, i)
    }),
    (UD, false, "doubleForKey:", |e| {
        let d = double(e)?;
        e.cpu.q[0] = d.to_bits() as u128;
        Ok(())
    }),
    (UD, false, "floatForKey:", |e| {
        let d = double(e)?;
        e.cpu.q[0] = (d as f32).to_bits() as u128;
        Ok(())
    }),
    (UD, false, "setObject:forKey:", |e| {
        let (ud, v, key) = (e.arg(0), e.arg(2), e.arg(3));
        set(e, ud, key, v)
    }),
    (UD, false, "setValue:forKey:", |e| {
        let (ud, v, key) = (e.arg(0), e.arg(2), e.arg(3));
        set(e, ud, key, v)
    }),
    (UD, false, "setBool:forKey:", |e| {
        let (ud, b, key) = (e.arg(0), e.arg(2) & 0xff != 0, e.arg(3));
        let v = foundation::boolean(e, b);
        set(e, ud, key, v)
    }),
    (UD, false, "setInteger:forKey:", |e| {
        let (ud, i, key) = (e.arg(0), e.arg(2) as i64, e.arg(3));
        let v = foundation::new_number_i64(e, i)?;
        set(e, ud, key, v)
    }),
    // The double is in d0, so the key is x2.
    (UD, false, "setDouble:forKey:", |e| {
        let (ud, d, key) = (e.arg(0), f64::from_bits(e.cpu.q[0] as u64), e.arg(2));
        let v = foundation::new_number_f64(e, d)?;
        set(e, ud, key, v)
    }),
    (UD, false, "setFloat:forKey:", |e| {
        let (ud, f, key) = (e.arg(0), f32::from_bits(e.cpu.q[0] as u32), e.arg(2));
        let v = foundation::new_number_f64(e, f as f64)?;
        set(e, ud, key, v)
    }),
    (UD, false, "removeObjectForKey:", |e| {
        let (ud, key) = (e.arg(0), e.arg(2));
        set(e, ud, key, 0)
    }),
    // Later registrations replace earlier ones.
    (UD, false, "registerDefaults:", |e| {
        let d = e.arg(2);
        if d == 0 {
            return Ok(());
        }
        let reg = registration(e)?;
        let src = foundation::as_cf_dict(e, d)?;
        for (k, v) in foundation::dict_pairs(e, src)? {
            cf(e, "_CFDictionarySetValue", &[reg, k, v])?;
        }
        Ok(())
    }),
    // Everything a lookup could find: registration, then global, then the object's own values on top.
    (UD, false, "dictionaryRepresentation", |e| {
        let ud = e.arg(0);
        let out = foundation::new_dict(e)?;
        let mut list = search_list(e, ud)?;
        list.reverse();
        for d in list {
            for (k, v) in foundation::dict_pairs(e, d)? {
                cf(e, "_CFDictionarySetValue", &[out, k, v])?;
            }
        }
        ret(e, out)
    }),
    (UD, false, "volatileDomainNames", |e| {
        let a = foundation::array_from(e, &[])?;
        ret(e, a)
    }),
    (UD, false, "persistentDomainForName:", |e| {
        let n = e.arg(2);
        if n == 0 {
            return ret(e, 0);
        }
        let name = foundation::nsstring(e, n)?;
        let d = domain_copy(e, &name)?;
        ret(e, d)
    }),
    (UD, false, "setPersistentDomain:forName:", |e| {
        let (d, n) = (e.arg(2), e.arg(3));
        if n == 0 {
            return Ok(());
        }
        let name = foundation::nsstring(e, n)?;
        let target = domain(e, &name)?;
        cf(e, "_CFDictionaryRemoveAllValues", &[target])?;
        if d != 0 {
            let src = foundation::as_cf_dict(e, d)?;
            for (k, v) in foundation::dict_pairs(e, src)? {
                cf(e, "_CFDictionarySetValue", &[target, k, v])?;
            }
        }
        Ok(())
    }),
    (UD, false, "removePersistentDomainForName:", |e| {
        let n = e.arg(2);
        if n == 0 {
            return Ok(());
        }
        let name = foundation::nsstring(e, n)?;
        if let Some(&d) = e.defaults.domains.get(&name) {
            cf(e, "_CFDictionaryRemoveAllValues", &[d])?;
        }
        Ok(())
    }),
    (UD, false, "synchronize", |e| ret(e, 1)),
];

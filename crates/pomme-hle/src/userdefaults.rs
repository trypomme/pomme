//! NSUserDefaults, as its domains work on iOS: each defaults object reads and
//! writes one persistent domain (the app's, named by its bundle ID, or a
//! suite's), shared by every object that names it; lookups fall back to the
//! global domain (the device's language and locale) and then the registration
//! domain, which `registerDefaults:` fills for the whole process. Values live
//! in memory for the run.
//!
//! CFPreferences is the same store, as on iOS (NSUserDefaults is built on it):
//! an application ID names a domain (`kCFPreferencesCurrentApplication` the
//! app's, `kCFPreferencesAnyApplication` the global one), so values written
//! either way read back either way.

use crate::foundation::{self, cf};
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// The global domain's name.
const GLOBAL: &str = "NSGlobalDomain";

#[derive(Clone, Default)]
pub struct State {
    /// Persistent domains by name: mutable CFDictionaries.
    domains: HashMap<String, u64>,
    /// `registerDefaults:`'s values.
    registration: u64,
    /// NSArgumentDomain: `-key value` pairs from the launch arguments (made on first use).
    arguments: u64,
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

/// NSArgumentDomain: each launch argument `-key` followed by a value (a
/// property list in text form, such as `YES`, `42` or `(fr, en)`, or else the
/// string itself), as an Xcode scheme's arguments set defaults.
fn arguments(emu: &mut Emu) -> Result<u64> {
    if emu.defaults.arguments != 0 {
        return Ok(emu.defaults.arguments);
    }
    let d = foundation::new_dict(emu)?;
    let args = emu.launch_args.clone();
    for pair in args.windows(2) {
        let (k, v) = (&pair[0], &pair[1]);
        let Some(key) = k.strip_prefix('-').filter(|k| !k.is_empty() && !v.starts_with('-')) else { continue };
        let key = foundation::new_nsstring(emu, key)?;
        let value = plist_value(emu, v)?;
        cf(emu, "_CFDictionarySetValue", &[d, key, value])?;
    }
    emu.defaults.arguments = d;
    Ok(d)
}

/// A text property list (old-style or XML) as its value, or the text as a string.
fn plist_value(emu: &mut Emu, text: &str) -> Result<u64> {
    let data = foundation::new_data(emu, text.as_bytes())?;
    let v = cf(emu, "_CFPropertyListCreateWithData", &[0, data, 0, 0, 0])?;
    if v != 0 {
        return Ok(v);
    }
    foundation::new_nsstring(emu, text)
}

/// The domains a lookup searches, in order.
fn search_list(emu: &mut Emu, ud: u64) -> Result<[u64; 4]> {
    Ok([arguments(emu)?, own(emu, ud)?, domain(emu, GLOBAL)?, registration(emu)?])
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

// ---- CFPreferences ----

/// The domain an application ID names.
fn app_id_domain(emu: &mut Emu, app: u64) -> Result<u64> {
    let name = if app == 0 { String::new() } else { foundation::nsstring(emu, app)? };
    let name = match name.as_str() {
        "" | "kCFPreferencesCurrentApplication" => app_domain(emu),
        "kCFPreferencesAnyApplication" => GLOBAL.into(),
        other => other.to_string(),
    };
    domain(emu, &name)
}

/// `CFPreferencesCopyAppValue(key, appID)`: through the app's search list
/// (arguments, the domain, global, registration), retained.
fn cf_copy_app_value(emu: &mut Emu) -> Result<()> {
    let (key, app) = (emu.arg(0), emu.arg(1));
    let own = app_id_domain(emu, app)?;
    for d in [arguments(emu)?, own, domain(emu, GLOBAL)?, registration(emu)?] {
        let v = cf(emu, "_CFDictionaryGetValue", &[d, key])?;
        if v != 0 {
            cf(emu, "_CFRetain", &[v])?;
            return ret(emu, v);
        }
    }
    ret(emu, 0)
}

/// `CFPreferencesCopyValue(key, appID, user, host)`: that domain only.
fn cf_copy_value(emu: &mut Emu) -> Result<()> {
    let (key, app) = (emu.arg(0), emu.arg(1));
    let d = app_id_domain(emu, app)?;
    let v = cf(emu, "_CFDictionaryGetValue", &[d, key])?;
    if v != 0 {
        cf(emu, "_CFRetain", &[v])?;
    }
    ret(emu, v)
}

fn set_in(emu: &mut Emu, d: u64, key: u64, value: u64) -> Result<()> {
    if key == 0 {
        return Ok(());
    }
    if value == 0 {
        cf(emu, "_CFDictionaryRemoveValue", &[d, key])?;
    } else {
        cf(emu, "_CFDictionarySetValue", &[d, key, value])?;
    }
    Ok(())
}

/// `CFPreferencesSetAppValue(key, value, appID)` and `CFPreferencesSetValue(…, user, host)`: NULL removes.
fn cf_set_value(emu: &mut Emu) -> Result<()> {
    let (key, value, app) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let d = app_id_domain(emu, app)?;
    set_in(emu, d, key, value)
}

/// `CFPreferencesSetMultiple(keysToSet, keysToRemove, appID, user, host)`
fn cf_set_multiple(emu: &mut Emu) -> Result<()> {
    let (set, remove, app) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let d = app_id_domain(emu, app)?;
    if set != 0 {
        for (k, v) in foundation::dict_pairs(emu, set)? {
            set_in(emu, d, k, v)?;
        }
    }
    if remove != 0 {
        for k in foundation::array_values(emu, remove)? {
            set_in(emu, d, k, 0)?;
        }
    }
    Ok(())
}

/// `CFPreferencesCopyKeyList(appID, user, host)`: the domain's keys, or NULL when it has none.
fn cf_copy_key_list(emu: &mut Emu) -> Result<()> {
    let d = app_id_domain(emu, emu.arg(0))?;
    let keys: Vec<u64> = foundation::dict_pairs(emu, d)?.into_iter().map(|(k, _)| k).collect();
    if keys.is_empty() {
        return ret(emu, 0);
    }
    let a = foundation::array_from(emu, &keys)?;
    ret(emu, a)
}

/// `CFPreferencesCopyMultiple(keys, appID, user, host)`: those keys' values (all, for NULL).
fn cf_copy_multiple(emu: &mut Emu) -> Result<()> {
    let (keys, app) = (emu.arg(0), emu.arg(1));
    let d = app_id_domain(emu, app)?;
    let out = foundation::new_dict(emu)?;
    let pairs = foundation::dict_pairs(emu, d)?;
    let wanted = if keys == 0 { None } else { Some(foundation::array_values(emu, keys)?) };
    for (k, v) in pairs {
        let keep = match &wanted {
            None => true,
            Some(list) => {
                let mut found = false;
                for &w in list {
                    if cf(emu, "_CFEqual", &[w, k])? & 0xff != 0 {
                        found = true;
                        break;
                    }
                }
                found
            }
        };
        if keep {
            cf(emu, "_CFDictionarySetValue", &[out, k, v])?;
        }
    }
    ret(emu, out)
}

/// A preference as an integer, as CFPreferencesGetApp…Value reads it: a
/// number, a boolean, or a string of digits or YES/true. None if it's none of those.
fn integer_of(emu: &mut Emu, v: u64) -> Result<Option<i64>> {
    if v == 0 {
        return Ok(None);
    }
    if is_kind(emu, v, "NSNumber")? {
        return Ok(Some(foundation::number_value(emu, v)?.0));
    }
    if is_kind(emu, v, "NSString")? {
        let s = foundation::nsstring(emu, v)?;
        let t = s.trim();
        return Ok(match t.to_ascii_lowercase().as_str() {
            "yes" | "true" => Some(1),
            "no" | "false" => Some(0),
            _ => t.parse().ok(),
        });
    }
    Ok(None)
}

/// `CFPreferencesGetAppBooleanValue(key, appID, Boolean *valid)` and `…IntegerValue`.
fn cf_get_app_number(emu: &mut Emu, boolean: bool) -> Result<()> {
    let (key, app, valid) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let own = app_id_domain(emu, app)?;
    let mut value = 0;
    for d in [arguments(emu)?, own, domain(emu, GLOBAL)?, registration(emu)?] {
        value = cf(emu, "_CFDictionaryGetValue", &[d, key])?;
        if value != 0 {
            break;
        }
    }
    let n = integer_of(emu, value)?;
    if valid != 0 {
        emu.p.memory.write_u8(valid, n.is_some() as u8)?;
    }
    let n = n.unwrap_or(0);
    ret(emu, if boolean { (n != 0) as u64 } else { n as u64 })
}

/// Make CoreFoundation's CFPreferences functions use these domains.
pub(crate) fn hook_cf_preferences(emu: &mut Emu) -> Result<()> {
    let hooks: [(&'static str, HostFn); 14] = [
        ("_CFPreferencesAppBooleanValue", |e| cf_get_app_number(e, true)),
        ("_CFPreferencesCopyAppValue", cf_copy_app_value),
        ("_CFPreferencesCopyValue", cf_copy_value),
        ("_CFPreferencesSetAppValue", cf_set_value),
        ("_CFPreferencesSetValue", cf_set_value),
        ("_CFPreferencesSetMultiple", cf_set_multiple),
        ("_CFPreferencesCopyKeyList", cf_copy_key_list),
        ("_CFPreferencesCopyMultiple", cf_copy_multiple),
        ("_CFPreferencesGetAppBooleanValue", |e| cf_get_app_number(e, true)),
        ("_CFPreferencesGetAppIntegerValue", |e| cf_get_app_number(e, false)),
        // Values are in memory: synchronizing always succeeds.
        ("_CFPreferencesAppSynchronize", |e| ret(e, 1)),
        ("_CFPreferencesSynchronize", |e| ret(e, 1)),
        ("_CFPreferencesAddSuitePreferencesToApp", |_| Ok(())),
        ("_CFPreferencesRemoveSuitePreferencesFromApp", |_| Ok(())),
    ];
    for (sym, f) in hooks {
        let a = foundation::cf_sym(emu, sym)?;
        emu.hook(a, f)?;
    }
    Ok(())
}

/// CFPreferences functions this CoreFoundation build doesn't have.
pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    // Nothing is managed by a profile here.
    ("_CFPreferencesAppValueIsForced", |e| ret(e, 0)),
];

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
        let names = [foundation::new_nsstring(e, "NSArgumentDomain")?, foundation::new_nsstring(e, "NSRegistrationDomain")?];
        let a = foundation::array_from(e, &names)?;
        ret(e, a)
    }),
    (UD, false, "volatileDomainForName:", |e| {
        let n = e.arg(2);
        let name = if n == 0 { String::new() } else { foundation::nsstring(e, n)? };
        let d = match name.as_str() {
            "NSArgumentDomain" => arguments(e)?,
            "NSRegistrationDomain" => registration(e)?,
            _ => 0,
        };
        ret(e, d)
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

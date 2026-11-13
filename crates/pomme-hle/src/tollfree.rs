//! Toll-free bridging for objects that aren't CoreFoundation's own: an
//! Objective-C or Swift subclass of NSArray, NSDictionary, NSString or NSData
//! (Swift's bridged arrays, dictionaries and strings, `Data`'s NSData) handed
//! to a CF function.
//!
//! Apple's CoreFoundation checks whether an object is its own and otherwise
//! sends it the matching Objective-C message. The sysroot's CoreFoundation
//! (swift-corelibs) has no such dispatch, so pomme adds it to the functions C
//! code uses on bridged objects:
//! - `CFGetTypeID` answers the bridged type (CFArray for any NSArray, …), as
//!   C code checks it before choosing which functions to call.
//! - Arrays and dictionaries: counts, lookups and apply-functions send
//!   `count`, `objectAtIndex:`, `objectForKey:`, `allKeys`.
//! - Strings and data: the reading functions run on a CFString or CFData
//!   with the object's contents.

use crate::foundation;
use crate::objc::{self, send};
use crate::{Emu, HostFn, Result};

/// The CF type an Objective-C object bridges to, by the Foundation class it's a kind of.
pub(crate) fn bridged_type_id(emu: &mut Emu, obj: u64) -> Result<Option<u64>> {
    const KINDS: [(&str, &str); 7] = [
        ("NSArray", "_CFArrayGetTypeID"),
        ("NSDictionary", "_CFDictionaryGetTypeID"),
        ("NSString", "_CFStringGetTypeID"),
        ("NSData", "_CFDataGetTypeID"),
        ("NSNumber", "_CFNumberGetTypeID"),
        ("NSSet", "_CFSetGetTypeID"),
        ("NSDate", "_CFDateGetTypeID"),
    ];
    for (class, type_fn) in KINDS {
        let cls = objc::host_class(emu, class)?;
        if objc::is_kind_of(emu, obj, cls)? {
            return Ok(Some(foundation::cf_call(emu, type_fn, &[])?));
        }
    }
    Ok(None)
}

/// A non-CF object in argument `i`: the hook handles it; a CF one runs CF's own code.
fn foreign(emu: &mut Emu, i: usize) -> Option<u64> {
    let o = emu.arg(i);
    if foundation::is_cf(emu, o) {
        emu.continue_original();
        None
    } else {
        Some(o)
    }
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

// ---- arrays ----

fn array_count(emu: &mut Emu) -> Result<()> {
    let Some(a) = foreign(emu, 0) else { return Ok(()) };
    let n = send(emu, a, "count", &[])?;
    ret(emu, n)
}

fn array_value(emu: &mut Emu) -> Result<()> {
    let Some(a) = foreign(emu, 0) else { return Ok(()) };
    let v = send(emu, a, "objectAtIndex:", &[emu.arg(1)])?;
    ret(emu, v)
}

/// `CFArrayGetValues(array, range, values)`
fn array_values(emu: &mut Emu) -> Result<()> {
    let Some(a) = foreign(emu, 0) else { return Ok(()) };
    let (loc, len, out) = (emu.arg(1), emu.arg(2), emu.arg(3));
    for i in 0..len {
        let v = send(emu, a, "objectAtIndex:", &[loc + i])?;
        emu.write_u64(out + i * 8, v)?;
    }
    Ok(())
}

/// `CFArrayApplyFunction(array, range, applier(value, context), context)`
fn array_apply(emu: &mut Emu) -> Result<()> {
    let Some(a) = foreign(emu, 0) else { return Ok(()) };
    let (loc, len, f, ctx) = (emu.arg(1), emu.arg(2), emu.arg(3), emu.arg(4));
    for i in 0..len {
        let v = send(emu, a, "objectAtIndex:", &[loc + i])?;
        emu.call(f, &[v, ctx])?;
    }
    Ok(())
}

// ---- dictionaries ----

fn dict_count(emu: &mut Emu) -> Result<()> {
    let Some(d) = foreign(emu, 0) else { return Ok(()) };
    let n = send(emu, d, "count", &[])?;
    ret(emu, n)
}

fn dict_value(emu: &mut Emu) -> Result<()> {
    let Some(d) = foreign(emu, 0) else { return Ok(()) };
    let v = send(emu, d, "objectForKey:", &[emu.arg(1)])?;
    ret(emu, v)
}

/// `CFDictionaryGetValueIfPresent(dict, key, &value)` → whether it's there.
fn dict_value_if_present(emu: &mut Emu) -> Result<()> {
    let Some(d) = foreign(emu, 0) else { return Ok(()) };
    let (key, out) = (emu.arg(1), emu.arg(2));
    let v = send(emu, d, "objectForKey:", &[key])?;
    if v != 0 && out != 0 {
        emu.write_u64(out, v)?;
    }
    ret(emu, (v != 0) as u64)
}

fn dict_contains_key(emu: &mut Emu) -> Result<()> {
    let Some(d) = foreign(emu, 0) else { return Ok(()) };
    let v = send(emu, d, "objectForKey:", &[emu.arg(1)])?;
    ret(emu, (v != 0) as u64)
}

/// The dictionary's (key, value) pairs.
fn dict_pairs(emu: &mut Emu, d: u64) -> Result<Vec<(u64, u64)>> {
    let keys = send(emu, d, "allKeys", &[])?;
    let n = send(emu, keys, "count", &[])?;
    let mut out = Vec::with_capacity(n as usize);
    for i in 0..n {
        let k = send(emu, keys, "objectAtIndex:", &[i])?;
        let v = send(emu, d, "objectForKey:", &[k])?;
        out.push((k, v));
    }
    Ok(out)
}

/// `CFDictionaryApplyFunction(dict, applier(key, value, context), context)`
fn dict_apply(emu: &mut Emu) -> Result<()> {
    let Some(d) = foreign(emu, 0) else { return Ok(()) };
    let (f, ctx) = (emu.arg(1), emu.arg(2));
    for (k, v) in dict_pairs(emu, d)? {
        emu.call(f, &[k, v, ctx])?;
    }
    Ok(())
}

/// `CFDictionaryGetKeysAndValues(dict, keys, values)` (either may be NULL).
fn dict_keys_and_values(emu: &mut Emu) -> Result<()> {
    let Some(d) = foreign(emu, 0) else { return Ok(()) };
    let (keys, values) = (emu.arg(1), emu.arg(2));
    for (i, (k, v)) in dict_pairs(emu, d)?.into_iter().enumerate() {
        if keys != 0 {
            emu.write_u64(keys + i as u64 * 8, k)?;
        }
        if values != 0 {
            emu.write_u64(values + i as u64 * 8, v)?;
        }
    }
    Ok(())
}

// ---- strings and data: CF's own code, on a copy of the contents ----

/// Swap argument `i` for a CFString with the same text, then run CF's function.
fn with_cf_string(emu: &mut Emu, i: usize) -> Result<()> {
    let s = emu.arg(i);
    let copy = foundation::as_cf_string(emu, s)?;
    emu.cpu.x[i] = copy;
    emu.continue_original();
    Ok(())
}

fn string_reader(emu: &mut Emu) -> Result<()> {
    with_cf_string(emu, 0)
}

/// `CFStringCompare(a, b, options)` and `CFEqual`-like two-string functions.
fn string_pair(emu: &mut Emu) -> Result<()> {
    for i in 0..2 {
        let s = emu.arg(i);
        let copy = foundation::as_cf_string(emu, s)?;
        emu.cpu.x[i] = copy;
    }
    emu.continue_original();
    Ok(())
}

fn data_reader(emu: &mut Emu) -> Result<()> {
    let d = emu.arg(0);
    if foundation::is_cf(emu, d) {
        emu.continue_original();
        return Ok(());
    }
    let bytes = foundation::data_bytes(emu, d)?;
    let copy = foundation::new_data(emu, &bytes)?;
    emu.cpu.x[0] = copy;
    emu.continue_original();
    Ok(())
}

/// Hook CoreFoundation's functions to bridge foreign objects.
pub(crate) fn install(emu: &mut Emu) -> Result<()> {
    let hooks: &[(&'static str, HostFn)] = &[
        ("_CFArrayGetCount", array_count),
        ("_CFArrayGetValueAtIndex", array_value),
        ("_CFArrayGetValues", array_values),
        ("_CFArrayApplyFunction", array_apply),
        ("_CFDictionaryGetCount", dict_count),
        ("_CFDictionaryGetValue", dict_value),
        ("_CFDictionaryGetValueIfPresent", dict_value_if_present),
        ("_CFDictionaryContainsKey", dict_contains_key),
        ("_CFDictionaryApplyFunction", dict_apply),
        ("_CFDictionaryGetKeysAndValues", dict_keys_and_values),
        ("_CFStringGetLength", string_reader),
        ("_CFStringGetCharacterAtIndex", string_reader),
        ("_CFStringGetCharacters", string_reader),
        ("_CFStringGetCString", string_reader),
        ("_CFStringGetCStringPtr", string_reader),
        ("_CFStringGetBytes", string_reader),
        ("_CFStringGetFastestEncoding", string_reader),
        ("_CFStringCompare", string_pair),
        ("_CFDataGetLength", data_reader),
        ("_CFDataGetBytePtr", data_reader),
        ("_CFDataGetBytes", data_reader),
    ];
    for &(sym, f) in hooks {
        let a = foundation::cf_sym(emu, sym)?;
        emu.hook(a, f)?;
    }
    Ok(())
}

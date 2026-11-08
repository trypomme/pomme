//! NSAttributedString and NSMutableAttributedString on CoreFoundation's
//! CFAttributedString (toll-free bridged as `__NSCFAttributedString`).
//!
//! `NSRange` arguments arrive in two registers (location, length), and
//! CFRange has the same layout, so ranges pass straight through. Strings and
//! dictionaries that aren't CF objects are converted first: CF's own
//! Objective-C dispatch is compiled out.

use crate::foundation::{as_cf_dict, as_cf_string, cf, cluster_alloc, wants_mutable};
use crate::libsystem::call_block;
use crate::objc::send;
use crate::textkit::cf_attributed;
use crate::{Emu, HostFn, Result};

const MUTABLE: &str = "NSMutableAttributedString";
const REVERSE: u64 = 1 << 1; // NSAttributedStringEnumerationReverse

fn len(emu: &mut Emu, s: u64) -> Result<u64> {
    cf(emu, "_CFAttributedStringGetLength", &[s])
}

/// Make a new attributed string with `string` and `attrs`, mutable if the receiver asked for it.
fn create(emu: &mut Emu, recv: u64, string: u64, attrs: u64) -> Result<u64> {
    let mutable = wants_mutable(emu, recv, MUTABLE)?;
    let string = if string == 0 { crate::foundation::new_nsstring(emu, "")? } else { as_cf_string(emu, string)? };
    let attrs = as_cf_dict(emu, attrs)?;
    let s = cf(emu, "_CFAttributedStringCreate", &[0, string, attrs])?;
    if !mutable {
        return Ok(s);
    }
    let m = cf(emu, "_CFAttributedStringCreateMutableCopy", &[0, 0, s])?;
    Ok(m)
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn init(emu: &mut Emu) -> Result<()> {
    let s = create(emu, emu.arg(0), 0, 0)?;
    ret(emu, s)
}

fn init_with_string(emu: &mut Emu) -> Result<()> {
    let s = create(emu, emu.arg(0), emu.arg(2), 0)?;
    ret(emu, s)
}

fn init_with_string_attributes(emu: &mut Emu) -> Result<()> {
    let s = create(emu, emu.arg(0), emu.arg(2), emu.arg(3))?;
    ret(emu, s)
}

fn init_with_attributed_string(emu: &mut Emu) -> Result<()> {
    let recv = emu.arg(0);
    let other = cf_attributed(emu, emu.arg(2))?;
    let s = if wants_mutable(emu, recv, MUTABLE)? {
        cf(emu, "_CFAttributedStringCreateMutableCopy", &[0, 0, other])?
    } else {
        cf(emu, "_CFAttributedStringCreateCopy", &[0, other])?
    };
    ret(emu, s)
}

fn string(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFAttributedStringGetString", &[emu.arg(0)])?;
    ret(emu, s)
}

fn length(emu: &mut Emu) -> Result<()> {
    let n = len(emu, emu.arg(0))?;
    ret(emu, n)
}

/// `-attributesAtIndex:effectiveRange:`
fn attributes_at(emu: &mut Emu) -> Result<()> {
    let d = cf(emu, "_CFAttributedStringGetAttributes", &[emu.arg(0), emu.arg(2), emu.arg(3)])?;
    ret(emu, d)
}

/// `-attribute:atIndex:effectiveRange:`
fn attribute_at(emu: &mut Emu) -> Result<()> {
    let name = as_cf_string(emu, emu.arg(2))?;
    let v = cf(emu, "_CFAttributedStringGetAttribute", &[emu.arg(0), emu.arg(3), name, emu.arg(4)])?;
    ret(emu, v)
}

/// `-attributesAtIndex:longestEffectiveRange:inRange:`
fn attributes_at_longest(emu: &mut Emu) -> Result<()> {
    let (s, i, out, loc, n) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    let d = cf(emu, "_CFAttributedStringGetAttributesAndLongestEffectiveRange", &[s, i, loc, n, out])?;
    ret(emu, d)
}

/// `-attribute:atIndex:longestEffectiveRange:inRange:`
fn attribute_at_longest(emu: &mut Emu) -> Result<()> {
    let (s, i, out, loc, n) = (emu.arg(0), emu.arg(3), emu.arg(4), emu.arg(5), emu.arg(6));
    let name = as_cf_string(emu, emu.arg(2))?;
    let v = cf(emu, "_CFAttributedStringGetAttributeAndLongestEffectiveRange", &[s, i, name, loc, n, out])?;
    ret(emu, v)
}

/// The runs of `s` inside `[loc, loc+n)`: (attributes or value, location, length).
fn runs(emu: &mut Emu, s: u64, name: Option<u64>, loc: u64, n: u64) -> Result<Vec<(u64, u64, u64)>> {
    let range = emu.alloc(16)?;
    let mut out = Vec::new();
    let mut i = loc;
    while i < loc + n {
        let v = match name {
            Some(k) => cf(emu, "_CFAttributedStringGetAttributeAndLongestEffectiveRange", &[s, i, k, loc, n, range])?,
            None => cf(emu, "_CFAttributedStringGetAttributesAndLongestEffectiveRange", &[s, i, loc, n, range])?,
        };
        let (rl, rn) = (emu.read_u64(range)?, emu.read_u64(range + 8)?);
        let end = (rl + rn).min(loc + n);
        let start = rl.max(i);
        out.push((v, start, end.saturating_sub(start)));
        if end <= i {
            break;
        }
        i = end;
    }
    emu.heap.free(&emu.p.memory, range)?;
    Ok(out)
}

fn enumerate(emu: &mut Emu, s: u64, name: Option<u64>, loc: u64, n: u64, opts: u64, block: u64) -> Result<()> {
    let mut rs = runs(emu, s, name, loc, n)?;
    if opts & REVERSE != 0 {
        rs.reverse();
    }
    let stop = emu.alloc(8)?;
    emu.write_u64(stop, 0)?;
    for (v, l, len) in rs {
        call_block(emu, block, &[v, l, len, stop])?;
        if emu.p.memory.read_u8(stop)? != 0 {
            break;
        }
    }
    Ok(())
}

/// `-enumerateAttributesInRange:options:usingBlock:`
fn enumerate_attributes(emu: &mut Emu) -> Result<()> {
    let (s, loc, n, opts, block) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    enumerate(emu, s, None, loc, n, opts, block)
}

/// `-enumerateAttribute:inRange:options:usingBlock:`
fn enumerate_attribute(emu: &mut Emu) -> Result<()> {
    let (s, loc, n, opts, block) = (emu.arg(0), emu.arg(3), emu.arg(4), emu.arg(5), emu.arg(6));
    let name = as_cf_string(emu, emu.arg(2))?;
    enumerate(emu, s, Some(name), loc, n, opts, block)
}

fn substring(emu: &mut Emu) -> Result<()> {
    let r = cf(emu, "_CFAttributedStringCreateWithSubstring", &[0, emu.arg(0), emu.arg(2), emu.arg(3)])?;
    ret(emu, r)
}

fn copy(emu: &mut Emu) -> Result<()> {
    let r = cf(emu, "_CFAttributedStringCreateCopy", &[0, emu.arg(0)])?;
    ret(emu, r)
}

fn mutable_copy(emu: &mut Emu) -> Result<()> {
    let r = cf(emu, "_CFAttributedStringCreateMutableCopy", &[0, 0, emu.arg(0)])?;
    ret(emu, r)
}

fn is_equal(emu: &mut Emu) -> Result<()> {
    let a = emu.arg(0);
    let b = cf_attributed(emu, emu.arg(2))?;
    let eq = a == b || (b != 0 && crate::foundation::is_cf(emu, b) && cf(emu, "_CFEqual", &[a, b])? & 0xff != 0);
    ret(emu, eq as u64)
}

fn hash(emu: &mut Emu) -> Result<()> {
    let h = cf(emu, "_CFHash", &[emu.arg(0)])?;
    ret(emu, h)
}

fn description(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFAttributedStringGetString", &[emu.arg(0)])?;
    ret(emu, s)
}

// ---- mutation ----

fn nothing(_: &mut Emu) -> Result<()> {
    Ok(())
}

/// `-replaceCharactersInRange:withString:`
fn replace_chars(emu: &mut Emu) -> Result<()> {
    let s = as_cf_string(emu, emu.arg(4))?;
    cf(emu, "_CFAttributedStringReplaceString", &[emu.arg(0), emu.arg(2), emu.arg(3), s])?;
    Ok(())
}

fn delete_chars(emu: &mut Emu) -> Result<()> {
    let empty = crate::foundation::new_nsstring(emu, "")?;
    cf(emu, "_CFAttributedStringReplaceString", &[emu.arg(0), emu.arg(2), emu.arg(3), empty])?;
    Ok(())
}

fn set_attributes(emu: &mut Emu, clear_others: bool) -> Result<()> {
    let d = as_cf_dict(emu, emu.arg(2))?;
    cf(emu, "_CFAttributedStringSetAttributes", &[emu.arg(0), emu.arg(3), emu.arg(4), d, clear_others as u64])?;
    Ok(())
}

/// `-addAttribute:value:range:`
fn add_attribute(emu: &mut Emu) -> Result<()> {
    let name = as_cf_string(emu, emu.arg(2))?;
    cf(emu, "_CFAttributedStringSetAttribute", &[emu.arg(0), emu.arg(4), emu.arg(5), name, emu.arg(3)])?;
    Ok(())
}

/// `-removeAttribute:range:`
fn remove_attribute(emu: &mut Emu) -> Result<()> {
    let name = as_cf_string(emu, emu.arg(2))?;
    cf(emu, "_CFAttributedStringRemoveAttribute", &[emu.arg(0), emu.arg(3), emu.arg(4), name])?;
    Ok(())
}

fn replace_attributed(emu: &mut Emu, s: u64, loc: u64, n: u64, other: u64) -> Result<()> {
    let other = cf_attributed(emu, other)?;
    cf(emu, "_CFAttributedStringReplaceAttributedString", &[s, loc, n, other])?;
    Ok(())
}

fn append(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let n = len(emu, s)?;
    replace_attributed(emu, s, n, 0, emu.arg(2))
}

/// `-insertAttributedString:atIndex:`
fn insert(emu: &mut Emu) -> Result<()> {
    replace_attributed(emu, emu.arg(0), emu.arg(3), 0, emu.arg(2))
}

/// `-replaceCharactersInRange:withAttributedString:`
fn replace_with_attributed(emu: &mut Emu) -> Result<()> {
    replace_attributed(emu, emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4))
}

fn set_attributed_string(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let n = len(emu, s)?;
    replace_attributed(emu, s, 0, n, emu.arg(2))
}

fn mutable_string(emu: &mut Emu) -> Result<()> {
    let m = cf(emu, "_CFAttributedStringGetMutableString", &[emu.arg(0)])?;
    if m != 0 {
        return ret(emu, m);
    }
    // corelibs CF doesn't provide one: hand back a copy (edits won't reflect).
    let s = cf(emu, "_CFAttributedStringGetString", &[emu.arg(0)])?;
    let m = send(emu, s, "mutableCopy", &[])?;
    ret(emu, m)
}

/// `+new`
fn class_new(emu: &mut Emu) -> Result<()> {
    let cls = emu.arg(0);
    let s = create(emu, cls, 0, 0)?;
    ret(emu, s)
}

const A: &str = "NSAttributedString";
const M: &str = MUTABLE;

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (A, true, "alloc", cluster_alloc),
    (A, true, "allocWithZone:", cluster_alloc),
    (A, true, "new", class_new),
    (A, false, "init", init),
    (A, false, "initWithString:", init_with_string),
    (A, false, "initWithString:attributes:", init_with_string_attributes),
    (A, false, "initWithAttributedString:", init_with_attributed_string),
    (A, false, "string", string),
    (A, false, "length", length),
    (A, false, "attributesAtIndex:effectiveRange:", attributes_at),
    (A, false, "attribute:atIndex:effectiveRange:", attribute_at),
    (A, false, "attributesAtIndex:longestEffectiveRange:inRange:", attributes_at_longest),
    (A, false, "attribute:atIndex:longestEffectiveRange:inRange:", attribute_at_longest),
    (A, false, "enumerateAttributesInRange:options:usingBlock:", enumerate_attributes),
    (A, false, "enumerateAttribute:inRange:options:usingBlock:", enumerate_attribute),
    (A, false, "attributedSubstringFromRange:", substring),
    (A, false, "copy", copy),
    (A, false, "copyWithZone:", copy),
    (A, false, "mutableCopy", mutable_copy),
    (A, false, "mutableCopyWithZone:", mutable_copy),
    (A, false, "isEqual:", is_equal),
    (A, false, "isEqualToAttributedString:", is_equal),
    (A, false, "hash", hash),
    (A, false, "description", description),
    (M, false, "beginEditing", nothing),
    (M, false, "endEditing", nothing),
    (M, false, "replaceCharactersInRange:withString:", replace_chars),
    (M, false, "deleteCharactersInRange:", delete_chars),
    (M, false, "setAttributes:range:", |e| set_attributes(e, true)),
    (M, false, "addAttributes:range:", |e| set_attributes(e, false)),
    (M, false, "addAttribute:value:range:", add_attribute),
    (M, false, "removeAttribute:range:", remove_attribute),
    (M, false, "appendAttributedString:", append),
    (M, false, "insertAttributedString:atIndex:", insert),
    (M, false, "replaceCharactersInRange:withAttributedString:", replace_with_attributed),
    (M, false, "setAttributedString:", set_attributed_string),
    (M, false, "mutableString", mutable_string),
];

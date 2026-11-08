//! Foundation: NSString and friends.
//!
//! Every NSString pomme creates uses the same layout as a compiler-emitted
//! constant CFString, so literals from the binary and host-made strings are
//! read the same way:
//!
//! ```text
//! +0  isa
//! +8  info   0x7c8 = 8-bit (we store UTF-8), 0x7d0 = UTF-16
//! +16 pointer to the characters
//! +24 length in code units of that encoding
//! ```

use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result, Stop};

const INFO_8BIT: u64 = 0x7c8;
const INFO_UTF16: u64 = 0x7d0;

pub(crate) fn init(_emu: &mut Emu) -> Result<()> {
    Ok(())
}

/// Create an NSString holding `s`.
pub fn new_nsstring(emu: &mut Emu, s: &str) -> Result<u64> {
    let cls = host_class(emu, "__NSCFString")?;
    let obj = emu.alloc(32)?;
    let chars = emu.alloc_cstr(s)?;
    emu.write_u64(obj, cls)?;
    emu.write_u64(obj + 8, INFO_8BIT)?;
    emu.write_u64(obj + 16, chars)?;
    emu.write_u64(obj + 24, s.len() as u64)?;
    Ok(obj)
}

/// Read an NSString's contents.
pub fn nsstring(emu: &mut Emu, obj: u64) -> Result<String> {
    if obj == 0 {
        return Ok(String::new());
    }
    let cls = objc::class_of(emu, obj)?;
    let name = emu.objc.class_name(cls);
    if !matches!(name.as_str(), "__NSCFString" | "__NSCFConstantString") {
        // A guest NSString subclass: ask it for UTF-8.
        let p = objc::send(emu, obj, "UTF8String", &[])?;
        return emu.cstr(p);
    }
    let info = emu.read_u64(obj + 8)?;
    let ptr = emu.read_u64(obj + 16)?;
    let len = emu.read_u64(obj + 24)? as usize;
    if info & 0x10 != 0 && info & 0x7f8 == INFO_UTF16 & 0x7f8 {
        let bytes = emu.p.memory.read(ptr, len * 2)?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Ok(String::from_utf16_lossy(&units))
    } else {
        Ok(String::from_utf8_lossy(emu.p.memory.read(ptr, len)?).into_owned())
    }
}

fn ret_string(emu: &mut Emu, s: &str) -> Result<()> {
    let r = new_nsstring(emu, s)?;
    emu.ret(r);
    Ok(())
}

// ---- C functions ----

fn ns_string_from_class(emu: &mut Emu) -> Result<()> {
    let cls = emu.arg(0);
    if cls == 0 {
        emu.ret(0);
        return Ok(());
    }
    let name = emu.objc.class_name(cls);
    ret_string(emu, &name)
}

fn ns_class_from_string(emu: &mut Emu) -> Result<()> {
    let name = nsstring(emu, emu.arg(0))?;
    let c = emu.objc.by_name.get(&name).copied().unwrap_or(0);
    emu.ret(c);
    Ok(())
}

fn ns_string_from_selector(emu: &mut Emu) -> Result<()> {
    let name = objc::sel_name(emu, emu.arg(0));
    ret_string(emu, &name)
}

fn ns_selector_from_string(emu: &mut Emu) -> Result<()> {
    let name = nsstring(emu, emu.arg(0))?;
    let s = objc::intern(emu, &name)?;
    emu.ret(s);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_NSStringFromClass", ns_string_from_class),
    ("_NSClassFromString", ns_class_from_string),
    ("_NSStringFromSelector", ns_string_from_selector),
    ("_NSSelectorFromString", ns_selector_from_string),
];

// ---- NSString methods ----

fn length(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    emu.ret(s.encode_utf16().count() as u64);
    Ok(())
}

fn utf8_string(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let p = emu.alloc_cstr(&s)?;
    emu.ret(p);
    Ok(())
}

fn character_at_index(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let i = emu.arg(2) as usize;
    let c = s.encode_utf16().nth(i).ok_or_else(|| Stop::Unsupported(format!("characterAtIndex: {i} out of range")))?;
    emu.ret(c as u64);
    Ok(())
}

fn is_equal_to_string(emu: &mut Emu) -> Result<()> {
    let other = emu.arg(2);
    let r = other != 0 && {
        let a = nsstring(emu, emu.arg(0))?;
        let b = nsstring(emu, other)?;
        a == b
    };
    emu.ret(r as u64);
    Ok(())
}

fn is_equal(emu: &mut Emu) -> Result<()> {
    let other = emu.arg(2);
    let nsstring_cls = host_class(emu, "NSString")?;
    if other == 0 || !objc::is_kind_of(emu, other, nsstring_cls)? {
        emu.ret(0);
        return Ok(());
    }
    is_equal_to_string(emu)
}

fn hash(emu: &mut Emu) -> Result<()> {
    // FNV-1a: stable across runs, unlike std's RandomState.
    let s = nsstring(emu, emu.arg(0))?;
    let h = s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    emu.ret(h);
    Ok(())
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn mutable_copy(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    ret_string(emu, &s)
}

fn string_with_utf8(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(2);
    if p == 0 {
        emu.ret(0);
        return Ok(());
    }
    let s = emu.cstr(p)?;
    ret_string(emu, &s)
}

fn string_by_appending(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    ret_string(emu, &(a + &b))
}

fn has_prefix(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    emu.ret((!b.is_empty() && a.starts_with(&b)) as u64);
    Ok(())
}

fn has_suffix(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    emu.ret((!b.is_empty() && a.ends_with(&b)) as u64);
    Ok(())
}

fn empty_string(emu: &mut Emu) -> Result<()> {
    ret_string(emu, "")
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSString", true, "string", empty_string),
    ("NSString", true, "stringWithUTF8String:", string_with_utf8),
    // Returns a fresh string in place of the allocated receiver.
    ("NSString", false, "initWithUTF8String:", string_with_utf8),
    ("NSString", false, "length", length),
    ("NSString", false, "UTF8String", utf8_string),
    ("NSString", false, "characterAtIndex:", character_at_index),
    ("NSString", false, "isEqualToString:", is_equal_to_string),
    ("NSString", false, "isEqual:", is_equal),
    ("NSString", false, "hash", hash),
    ("NSString", false, "copy", ret_self),
    ("NSString", false, "copyWithZone:", ret_self),
    ("NSString", false, "mutableCopy", mutable_copy),
    ("NSString", false, "description", ret_self),
    ("NSString", false, "stringByAppendingString:", string_by_appending),
    ("NSString", false, "hasPrefix:", has_prefix),
    ("NSString", false, "hasSuffix:", has_suffix),
];

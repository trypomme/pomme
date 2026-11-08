//! Foundation, bridged to the guest's CoreFoundation.
//!
//! CoreFoundation is open source, so pomme runs the real one as guest code
//! (built into the sysroot). Foundation's Objective-C classes are not, so they
//! are host classes whose methods call CF — the same toll-free bridging iOS
//! uses: an `NSArray` *is* a `CFArray` whose `isa` is `__NSCFArray`.
//!
//! - After CF initializes, [`bridge`] points each CF type at its class with
//!   `_CFRuntimeBridgeTypeToClass`, makes `__CFConstantStringClassReference` (the
//!   `isa` of every string literal in the app) a real class, and hooks CF's
//!   generic functions (`CFRetain`, `CFHash`, `CFEqual`, ...) so that elements
//!   which are plain Objective-C objects get Objective-C semantics.
//! - Memory: like the rest of pomme for now, nothing is freed. `CFRelease` is a
//!   no-op, which also keeps CF from freeing objects Objective-C code holds
//!   (pomme's `objc_retain` doesn't count).
//!
//! Without CoreFoundation in the sysroot, strings fall back to a minimal
//! host-only representation (the layout of a constant CFString).

use crate::libsystem::call_block;
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result, Stop};
use std::collections::{HashMap, HashSet};

const UTF8: u64 = 0x0800_0100; // kCFStringEncodingUTF8
const NUMBER_SINT32: u64 = 3;
const NUMBER_SINT64: u64 = 4;
const NUMBER_FLOAT32: u64 = 5;
const NUMBER_FLOAT64: u64 = 6;
/// Marks an object returned by `+alloc` on a class cluster (NSArray, NSString, ...).
/// The real object is made by its `-init...` method.
const PLACEHOLDER: u64 = 0x504c_4143_4548_4c44; // "PLACEHLD"

#[derive(Default)]
pub struct State {
    /// The CoreFoundation image, once loaded.
    cf: Option<usize>,
    syms: HashMap<&'static str, u64>,
    /// Classes CF objects can have as their isa.
    cf_classes: HashSet<u64>,
    const_string_class: u64,
    true_obj: u64,
    false_obj: u64,
    null_obj: u64,
}

pub(crate) fn init(_emu: &mut Emu) -> Result<()> {
    Ok(())
}

// ---- calling CoreFoundation ----

fn cf_sym(emu: &mut Emu, name: &'static str) -> Result<u64> {
    if let Some(&a) = emu.fnd.syms.get(name) {
        return Ok(a);
    }
    let img = emu.fnd.cf.ok_or_else(|| Stop::Unsupported(format!("{name}: CoreFoundation isn't loaded (run scripts/build-sysroot.sh)")))?;
    let a = emu.p.images[img].symbol(name).ok_or_else(|| Stop::Unsupported(format!("CoreFoundation has no {name}")))?;
    emu.fnd.syms.insert(name, a);
    Ok(a)
}

/// Call a CoreFoundation function with integer arguments.
fn cf(emu: &mut Emu, name: &'static str, args: &[u64]) -> Result<u64> {
    let f = cf_sym(emu, name)?;
    emu.call(f, args)
}

/// Read a CF data symbol (e.g. a `kCF...` constant pointer).
fn cf_data(emu: &mut Emu, name: &'static str) -> Result<u64> {
    let a = cf_sym(emu, name)?;
    emu.read_u64(a)
}

fn have_cf(emu: &Emu) -> bool {
    emu.fnd.cf.is_some()
}

/// Is `obj` a CoreFoundation object (as opposed to a plain Objective-C one)?
pub fn is_cf(emu: &Emu, obj: u64) -> bool {
    if obj == 0 || obj >> 63 != 0 {
        return obj == 0;
    }
    match emu.read_u64(obj) {
        Ok(isa) => isa == 0 || isa == emu.fnd.const_string_class || emu.fnd.cf_classes.contains(&isa),
        Err(_) => false,
    }
}

// ---- bridging ----

/// Called right after `__CFInitialize` (as libSystem would).
pub(crate) fn bridge(emu: &mut Emu, image: usize) -> Result<()> {
    emu.fnd.cf = Some(image);
    let bridge_fn = cf_sym(emu, "__CFRuntimeBridgeTypeToClass")?;

    // Everything defaults to __NSCFType; then the toll-free bridged types.
    let generic = host_class(emu, "__NSCFType")?;
    emu.fnd.cf_classes.insert(generic);
    for id in 1..96 {
        emu.call(bridge_fn, &[id, generic])?;
    }
    let types: &[(&'static str, &str)] = &[
        ("_CFStringGetTypeID", "__NSCFString"),
        ("_CFArrayGetTypeID", "__NSCFArray"),
        ("_CFDictionaryGetTypeID", "__NSCFDictionary"),
        ("_CFSetGetTypeID", "__NSCFSet"),
        ("_CFNumberGetTypeID", "__NSCFNumber"),
        ("_CFBooleanGetTypeID", "__NSCFBoolean"),
        ("_CFDataGetTypeID", "__NSCFData"),
        ("_CFNullGetTypeID", "NSNull"),
        ("_CFDateGetTypeID", "__NSDate"),
        ("_CFAttributedStringGetTypeID", "__NSCFAttributedString"),
        ("_CFURLGetTypeID", "NSURL"),
        ("_CFCharacterSetGetTypeID", "__NSCFCharacterSet"),
    ];
    for &(type_fn, class) in types {
        let id = cf(emu, type_fn, &[])?;
        let cls = host_class(emu, class)?;
        emu.call(bridge_fn, &[id, cls])?;
        emu.fnd.cf_classes.insert(cls);
    }

    // Objects CF made while initializing still have isa 0.
    for (sym, class) in [("_kCFBooleanTrue", "__NSCFBoolean"), ("_kCFBooleanFalse", "__NSCFBoolean"), ("_kCFNull", "NSNull")] {
        let obj = cf_data(emu, sym)?;
        let cls = host_class(emu, class)?;
        emu.write_u64(obj, cls)?;
    }
    emu.fnd.true_obj = cf_data(emu, "_kCFBooleanTrue")?;
    emu.fnd.false_obj = cf_data(emu, "_kCFBooleanFalse")?;
    emu.fnd.null_obj = cf_data(emu, "_kCFNull")?;

    // Every string literal's isa is &__CFConstantStringClassReference: make that a class.
    let at = cf_sym(emu, "___CFConstantStringClassReference")?;
    let base = host_class(emu, "__NSCFConstantString")?;
    objc::alias_class(emu, at, base)?;
    emu.fnd.const_string_class = at;

    // Generic functions: Objective-C semantics for non-CF objects; no freeing.
    for (sym, f) in [
        ("_CFRetain", hook_retain as HostFn),
        ("_CFRelease", hook_release),
        ("_CFAutorelease", hook_retain),
        ("_CFHash", hook_hash),
        ("_CFEqual", hook_equal),
        ("_CFCopyDescription", hook_copy_description),
        ("_CFGetTypeID", hook_get_type_id),
    ] {
        let a = cf_sym(emu, sym)?;
        emu.hook(a, f)?;
    }
    // The collection callbacks call CFRetain/CFRelease, possibly inlined: hook them too.
    let callbacks = cf_sym(emu, "_kCFTypeArrayCallBacks")?;
    let retain = emu.read_u64(callbacks + 8)?;
    let release = emu.read_u64(callbacks + 16)?;
    emu.hook(retain, hook_collection_retain)?;
    emu.hook(release, hook_release)?;
    Ok(())
}

fn hook_retain(_: &mut Emu) -> Result<()> {
    Ok(()) // returns its argument
}

fn hook_release(_: &mut Emu) -> Result<()> {
    Ok(())
}

/// `__CFTypeCollectionRetain(allocator, ptr)` → ptr
fn hook_collection_retain(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.arg(1));
    Ok(())
}

fn hook_hash(emu: &mut Emu) -> Result<()> {
    let obj = emu.arg(0);
    if is_cf(emu, obj) {
        emu.continue_original();
        return Ok(());
    }
    let h = send(emu, obj, "hash", &[])?;
    emu.ret(h);
    Ok(())
}

fn hook_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(1));
    if a == b {
        emu.ret(1);
        return Ok(());
    }
    if is_cf(emu, a) && is_cf(emu, b) {
        emu.continue_original();
        return Ok(());
    }
    let (recv, other) = if is_cf(emu, a) { (b, a) } else { (a, b) };
    let r = send(emu, recv, "isEqual:", &[other])?;
    emu.ret(r & 1);
    Ok(())
}

fn hook_copy_description(emu: &mut Emu) -> Result<()> {
    let obj = emu.arg(0);
    if is_cf(emu, obj) {
        emu.continue_original();
        return Ok(());
    }
    let d = send(emu, obj, "description", &[])?;
    emu.ret(d);
    Ok(())
}

fn hook_get_type_id(emu: &mut Emu) -> Result<()> {
    let obj = emu.arg(0);
    if is_cf(emu, obj) {
        emu.continue_original();
        return Ok(());
    }
    emu.ret(1); // _kCFRuntimeIDCFType
    Ok(())
}

// ---- strings ----

/// Create an NSString holding `s`.
pub fn new_nsstring(emu: &mut Emu, s: &str) -> Result<u64> {
    if !have_cf(emu) {
        return host_string(emu, s);
    }
    let bytes = emu.alloc_cstr(s)?;
    let r = cf(emu, "_CFStringCreateWithBytes", &[0, bytes, s.len() as u64, UTF8, 0])?;
    emu.heap.free(&emu.p.memory, bytes)?;
    Ok(r)
}

fn new_mutable_string(emu: &mut Emu, s: &str) -> Result<u64> {
    let m = cf(emu, "_CFStringCreateMutable", &[0, 0])?;
    let tmp = new_nsstring(emu, s)?;
    cf(emu, "_CFStringAppend", &[m, tmp])?;
    Ok(m)
}

/// Read an NSString's contents.
pub fn nsstring(emu: &mut Emu, obj: u64) -> Result<String> {
    if obj == 0 {
        return Ok(String::new());
    }
    if !have_cf(emu) {
        return read_host_string(emu, obj);
    }
    if !is_cf(emu, obj) {
        // A guest NSString subclass, e.g. a Swift string bridged as an object.
        // Read it through the NSString primitives it must implement.
        let len = send(emu, obj, "length", &[])?;
        let buf = emu.alloc(len * 2 + 2)?;
        let sel = objc::intern(emu, "getCharacters:range:")?;
        if objc::responds(emu, obj, sel)? {
            send(emu, obj, "getCharacters:range:", &[buf, 0, len])?;
        } else {
            for i in 0..len {
                let c = send(emu, obj, "characterAtIndex:", &[i])?;
                emu.p.memory.write_u16(buf + i * 2, c as u16)?;
            }
        }
        let bytes = emu.p.memory.read(buf, (len * 2) as usize)?;
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        emu.heap.free(&emu.p.memory, buf)?;
        return Ok(String::from_utf16_lossy(&units));
    }
    let len = cf(emu, "_CFStringGetLength", &[obj])?;
    let size = cf(emu, "_CFStringGetMaximumSizeForEncoding", &[len, UTF8])? + 1;
    let buf = emu.alloc(size)?;
    let ok = cf(emu, "_CFStringGetCString", &[obj, buf, size, UTF8])? & 0xff;
    let s = if ok != 0 { emu.cstr(buf)? } else { String::new() };
    emu.heap.free(&emu.p.memory, buf)?;
    Ok(s)
}

/// Host-only string, used before/without CoreFoundation: the constant-CFString layout.
fn host_string(emu: &mut Emu, s: &str) -> Result<u64> {
    let cls = host_class(emu, "__NSCFString")?;
    let obj = emu.alloc(32)?;
    let chars = emu.alloc_cstr(s)?;
    emu.write_u64(obj, cls)?;
    emu.write_u64(obj + 8, 0x7c8)?;
    emu.write_u64(obj + 16, chars)?;
    emu.write_u64(obj + 24, s.len() as u64)?;
    Ok(obj)
}

fn read_host_string(emu: &mut Emu, obj: u64) -> Result<String> {
    let info = emu.read_u64(obj + 8)?;
    let ptr = emu.read_u64(obj + 16)?;
    let len = emu.read_u64(obj + 24)? as usize;
    if info & 0x10 != 0 {
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

fn ret_bool(emu: &mut Emu, b: bool) -> Result<()> {
    emu.ret(b as u64);
    Ok(())
}

// ---- class clusters: +alloc returns a placeholder, -init... makes the real object ----

fn cluster_alloc(emu: &mut Emu) -> Result<()> {
    let cls = emu.arg(0);
    if !emu.objc.class(cls).is_some_and(|c| c.host) {
        // A guest subclass of a cluster class: a normal instance.
        let o = objc::create_instance(emu, cls, 0)?;
        emu.ret(o);
        return Ok(());
    }
    let p = emu.alloc(16)?;
    emu.write_u64(p, cls)?;
    emu.write_u64(p + 8, PLACEHOLDER)?;
    emu.ret(p);
    Ok(())
}

/// Whether the receiver (placeholder or class) is the mutable variant.
fn wants_mutable(emu: &mut Emu, obj: u64, mutable_class: &str) -> Result<bool> {
    let cls = if emu.objc.class(obj).is_some() { obj } else { emu.read_u64(obj)? };
    let m = host_class(emu, mutable_class)?;
    let mut c = cls;
    while c != 0 {
        if c == m {
            return Ok(true);
        }
        c = emu.objc.class(c).map_or(0, |k| k.superclass);
    }
    Ok(false)
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
    let c = objc::class_by_name(emu, &name)?;
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

fn ns_log(emu: &mut Emu) -> Result<()> {
    let fmt = nsstring(emu, emu.arg(0))?;
    let s = crate::printf::format(emu, fmt.as_bytes(), &mut crate::printf::Args::new(emu.cpu.sp))?;
    println!("[NSLog] {}", String::from_utf8_lossy(&s));
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_NSStringFromClass", ns_string_from_class),
    ("_NSClassFromString", ns_class_from_string),
    ("_NSStringFromSelector", ns_string_from_selector),
    ("_NSSelectorFromString", ns_selector_from_string),
    ("_NSLog", ns_log),
];

// ---- NSString ----

fn string_length(emu: &mut Emu) -> Result<()> {
    if !have_cf(emu) {
        let s = nsstring(emu, emu.arg(0))?;
        emu.ret(s.encode_utf16().count() as u64);
        return Ok(());
    }
    let n = cf(emu, "_CFStringGetLength", &[emu.arg(0)])?;
    emu.ret(n);
    Ok(())
}

fn utf8_string(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let p = emu.alloc_cstr(&s)?;
    emu.ret(p);
    Ok(())
}

fn character_at_index(emu: &mut Emu) -> Result<()> {
    let (s, i) = (emu.arg(0), emu.arg(2));
    if have_cf(emu) {
        let c = cf(emu, "_CFStringGetCharacterAtIndex", &[s, i])?;
        emu.ret(c & 0xffff);
        return Ok(());
    }
    let text = nsstring(emu, s)?;
    let c = text.encode_utf16().nth(i as usize).ok_or_else(|| Stop::Unsupported(format!("characterAtIndex: {i} out of range")))?;
    emu.ret(c as u64);
    Ok(())
}

fn is_equal_to_string(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    if b == 0 {
        return ret_bool(emu, false);
    }
    let x = nsstring(emu, a)?;
    let y = nsstring(emu, b)?;
    ret_bool(emu, x == y)
}

fn string_is_equal(emu: &mut Emu) -> Result<()> {
    let other = emu.arg(2);
    let nsstring_cls = host_class(emu, "NSString")?;
    if other == 0 || !objc::is_kind_of(emu, other, nsstring_cls)? {
        return ret_bool(emu, false);
    }
    is_equal_to_string(emu)
}

fn string_hash(emu: &mut Emu) -> Result<()> {
    if have_cf(emu) && is_cf(emu, emu.arg(0)) {
        // CF's hash, so Objective-C and CF collections agree.
        let h = cf(emu, "_CFHash", &[emu.arg(0)])?;
        emu.ret(h);
        return Ok(());
    }
    // FNV-1a over UTF-8: stable across runs.
    let s = nsstring(emu, emu.arg(0))?;
    let h = s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    emu.ret(h);
    Ok(())
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn string_copy(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    ret_string(emu, &s)
}

fn string_mutable_copy(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let m = new_mutable_string(emu, &s)?;
    emu.ret(m);
    Ok(())
}

fn string_with_utf8(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(2);
    if p == 0 {
        emu.ret(0);
        return Ok(());
    }
    let s = emu.cstr(p)?;
    if wants_mutable(emu, emu.arg(0), "NSMutableString")? {
        let m = new_mutable_string(emu, &s)?;
        emu.ret(m);
        return Ok(());
    }
    ret_string(emu, &s)
}

fn string_with_string(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(2))?;
    if wants_mutable(emu, emu.arg(0), "NSMutableString")? {
        let m = new_mutable_string(emu, &s)?;
        emu.ret(m);
        return Ok(());
    }
    ret_string(emu, &s)
}

fn empty_string(emu: &mut Emu) -> Result<()> {
    if have_cf(emu) && wants_mutable(emu, emu.arg(0), "NSMutableString")? {
        let m = new_mutable_string(emu, "")?;
        emu.ret(m);
        return Ok(());
    }
    ret_string(emu, "")
}

/// `+stringWithFormat:` / `-initWithFormat:`: format is x2, varargs on the stack.
fn string_with_format(emu: &mut Emu) -> Result<()> {
    let fmt = nsstring(emu, emu.arg(2))?;
    let s = crate::printf::format(emu, fmt.as_bytes(), &mut crate::printf::Args::new(emu.cpu.sp))?;
    ret_string(emu, &String::from_utf8_lossy(&s))
}

fn string_by_appending(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    ret_string(emu, &(a + &b))
}

fn append_string(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFStringAppend", &[emu.arg(0), emu.arg(2)])?;
    Ok(())
}

fn has_prefix(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    ret_bool(emu, !b.is_empty() && a.starts_with(&b))
}

fn has_suffix(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    ret_bool(emu, !b.is_empty() && a.ends_with(&b))
}

fn lowercase(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    ret_string(emu, &s.to_lowercase())
}

fn uppercase(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    ret_string(emu, &s.to_uppercase())
}

fn string_compare(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    emu.ret(a.cmp(&b) as i64 as u64);
    Ok(())
}

fn integer_value(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let t = s.trim_start();
    let end = t.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')))).map_or(t.len(), |(i, _)| i);
    emu.ret(t[..end].parse::<i64>().unwrap_or(0) as u64);
    Ok(())
}

fn double_value(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let v: f64 = s.trim().parse().unwrap_or(0.0);
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

fn bool_value_string(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let t = s.trim_start();
    let b = matches!(t.chars().next(), Some('Y' | 'y' | 'T' | 't' | '1'..='9'));
    ret_bool(emu, b)
}

/// `-componentsSeparatedByString:` → NSArray
fn components_separated(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let sep = nsstring(emu, emu.arg(2))?;
    let parts: Vec<String> = if sep.is_empty() { vec![s] } else { s.split(sep.as_str()).map(str::to_string).collect() };
    let arr = new_array(emu, true)?;
    for p in parts {
        let o = new_nsstring(emu, &p)?;
        cf(emu, "_CFArrayAppendValue", &[arr, o])?;
    }
    emu.ret(arr);
    Ok(())
}

/// UTF-16 offsets → a substring.
fn utf16_slice(s: &str, from: usize, to: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().collect();
    String::from_utf16_lossy(&units[from.min(units.len())..to.min(units.len())])
}

fn substring_from(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let r = utf16_slice(&s, emu.arg(2) as usize, usize::MAX);
    ret_string(emu, &r)
}

fn substring_to(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let r = utf16_slice(&s, 0, emu.arg(2) as usize);
    ret_string(emu, &r)
}

/// `-rangeOfString:` → NSRange {location, length} in x0/x1 (NSNotFound = i64::MAX).
fn range_of_string(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let needle = nsstring(emu, emu.arg(2))?;
    match s.find(&needle).filter(|_| !needle.is_empty()) {
        Some(byte) => {
            emu.cpu.x[0] = s[..byte].encode_utf16().count() as u64;
            emu.cpu.x[1] = needle.encode_utf16().count() as u64;
        }
        None => {
            emu.cpu.x[0] = i64::MAX as u64;
            emu.cpu.x[1] = 0;
        }
    }
    Ok(())
}

fn contains_string(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let needle = nsstring(emu, emu.arg(2))?;
    ret_bool(emu, s.contains(&needle))
}

/// `-getCString:maxLength:encoding:`
fn get_cstring(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let (buf, max) = (emu.arg(2), emu.arg(3));
    if (s.len() as u64) + 1 > max {
        return ret_bool(emu, false);
    }
    emu.p.memory.write(buf, s.as_bytes())?;
    emu.p.memory.write_u8(buf + s.len() as u64, 0)?;
    ret_bool(emu, true)
}

fn length_of_bytes(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    emu.ret(s.len() as u64);
    Ok(())
}

// ---- the selectors the Swift standard library uses to read foreign strings ----

const CF_ASCII: u64 = 0x0600;

/// NSStringEncoding → CFStringEncoding for the encodings the stdlib asks for.
fn cf_encoding(ns: u64) -> u64 {
    match ns {
        1 => CF_ASCII,     // NSASCIIStringEncoding
        4 => UTF8,         // NSUTF8StringEncoding
        5 => 0x0201,       // NSISOLatin1StringEncoding → kCFStringEncodingISOLatin1
        10 => 0x0100,      // NSUnicodeStringEncoding → kCFStringEncodingUnicode
        other => other,
    }
}

/// `-_fastCStringContents:(BOOL)nulTerminated` → ASCII bytes, or NULL.
fn fast_cstring_contents(emu: &mut Emu) -> Result<()> {
    let p = cf(emu, "_CFStringGetCStringPtr", &[emu.arg(0), CF_ASCII])?;
    emu.ret(p);
    Ok(())
}

/// `-_fastCharacterContents` → UTF-16 buffer, or NULL.
fn fast_character_contents(emu: &mut Emu) -> Result<()> {
    let p = cf(emu, "_CFStringGetCharactersPtr", &[emu.arg(0)])?;
    emu.ret(p);
    Ok(())
}

/// `-getCharacters:(unichar *)buffer range:(NSRange)range`
fn get_characters_range(emu: &mut Emu) -> Result<()> {
    let (s, buf, loc, len) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4));
    cf(emu, "_CFStringGetCharacters", &[s, loc, len, buf])?;
    Ok(())
}

/// `-getBytes:maxLength:usedLength:encoding:options:range:remainingRange:`.
/// x2..x6 hold the first five; the 16-byte NSRange doesn't fit in x7, so it
/// and `remainingRange` are on the stack.
fn get_bytes(emu: &mut Emu) -> Result<()> {
    let (s, buf, max, used, enc) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    let sp = emu.cpu.sp;
    let loc = emu.read_u64(sp)?;
    let len = emu.read_u64(sp + 8)?;
    let remaining = emu.read_u64(sp + 16)?;
    // CFStringGetBytes(str, range, encoding, lossByte, isExternalRepresentation, buffer, maxBufLen,
    // usedBufLen): usedBufLen would be a 9th (stack) argument, which emu.call doesn't pass, so it
    // gets NULL and the byte count is measured separately.
    let converted = cf(emu, "_CFStringGetBytes", &[s, loc, len, cf_encoding(enc), 0, 0, buf, max])?;
    if used != 0 {
        let n = cf_get_bytes_len(emu, s, loc, converted, enc)?;
        emu.write_u64(used, n)?;
    }
    if remaining != 0 {
        emu.write_u64(remaining, loc + converted)?;
        emu.write_u64(remaining + 8, len - converted.min(len))?;
    }
    emu.ret((converted > 0 || len == 0) as u64);
    Ok(())
}

/// Bytes needed to encode `count` UTF-16 units starting at `loc`.
fn cf_get_bytes_len(emu: &mut Emu, s: u64, loc: u64, count: u64, enc: u64) -> Result<u64> {
    let text = nsstring(emu, s)?;
    let units: Vec<u16> = text.encode_utf16().collect();
    let end = (loc + count).min(units.len() as u64) as usize;
    let part = String::from_utf16_lossy(&units[loc.min(end as u64) as usize..end]);
    Ok(match enc {
        10 => (part.encode_utf16().count() * 2) as u64,
        _ => part.len() as u64,
    })
}

/// `-compare:options:range:locale:` → NSComparisonResult
fn compare_options_range(emu: &mut Emu) -> Result<()> {
    let (s, other, options, loc, len, locale) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5), emu.arg(6));
    let r = cf(emu, "_CFStringCompareWithOptionsAndLocale", &[s, other, loc, len, options, locale])?;
    emu.ret(r);
    Ok(())
}

fn ret_nil(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

fn ret_yes(emu: &mut Emu) -> Result<()> {
    emu.ret(1);
    Ok(())
}

// ---- arrays ----

fn new_array(emu: &mut Emu, _mutable: bool) -> Result<u64> {
    let cb = cf_sym(emu, "_kCFTypeArrayCallBacks")?;
    cf(emu, "_CFArrayCreateMutable", &[0, 0, cb])
}

fn array_from(emu: &mut Emu, objs: &[u64]) -> Result<u64> {
    let a = new_array(emu, true)?;
    for &o in objs {
        cf(emu, "_CFArrayAppendValue", &[a, o])?;
    }
    Ok(a)
}

fn read_ptrs(emu: &Emu, at: u64, n: u64) -> Result<Vec<u64>> {
    (0..n).map(|i| emu.read_u64(at + i * 8)).collect()
}

fn array_values(emu: &mut Emu, arr: u64) -> Result<Vec<u64>> {
    if !is_cf(emu, arr) {
        let n = send(emu, arr, "count", &[])?;
        return (0..n).map(|i| send(emu, arr, "objectAtIndex:", &[i])).collect();
    }
    let n = cf(emu, "_CFArrayGetCount", &[arr])?;
    let buf = emu.alloc(n * 8 + 8)?;
    cf(emu, "_CFArrayGetValues", &[arr, 0, n, buf])?;
    let v = read_ptrs(emu, buf, n)?;
    emu.heap.free(&emu.p.memory, buf)?;
    Ok(v)
}

fn array_empty(emu: &mut Emu) -> Result<()> {
    let a = new_array(emu, true)?;
    emu.ret(a);
    Ok(())
}

/// `+arrayWithObjects:count:` / `-initWithObjects:count:` (what `@[...]` compiles to).
fn array_with_objects_count(emu: &mut Emu) -> Result<()> {
    let objs = read_ptrs(emu, emu.arg(2), emu.arg(3))?;
    let a = array_from(emu, &objs)?;
    emu.ret(a);
    Ok(())
}

fn array_with_object(emu: &mut Emu) -> Result<()> {
    let a = array_from(emu, &[emu.arg(2)])?;
    emu.ret(a);
    Ok(())
}

fn array_with_array(emu: &mut Emu) -> Result<()> {
    let src = emu.arg(2);
    let v = if src == 0 { Vec::new() } else { array_values(emu, src)? };
    let a = array_from(emu, &v)?;
    emu.ret(a);
    Ok(())
}

/// `+arrayWithObjects:(id)first, ...` — nil-terminated varargs on the stack.
fn array_with_objects_varargs(emu: &mut Emu) -> Result<()> {
    let mut objs = Vec::new();
    let mut o = emu.arg(2);
    let mut at = emu.cpu.sp;
    while o != 0 {
        objs.push(o);
        o = emu.read_u64(at)?;
        at += 8;
    }
    let a = array_from(emu, &objs)?;
    emu.ret(a);
    Ok(())
}

fn array_init(emu: &mut Emu) -> Result<()> {
    array_empty(emu)
}

fn array_count(emu: &mut Emu) -> Result<()> {
    let n = cf(emu, "_CFArrayGetCount", &[emu.arg(0)])?;
    emu.ret(n);
    Ok(())
}

fn array_object_at_index(emu: &mut Emu) -> Result<()> {
    let (a, i) = (emu.arg(0), emu.arg(2));
    let n = cf(emu, "_CFArrayGetCount", &[a])?;
    if i >= n {
        return Err(Stop::Unsupported(format!("NSRangeException: index {i} beyond bounds [0 .. {}]", n as i64 - 1)));
    }
    let v = cf(emu, "_CFArrayGetValueAtIndex", &[a, i])?;
    emu.ret(v);
    Ok(())
}

fn array_first(emu: &mut Emu) -> Result<()> {
    let a = emu.arg(0);
    let n = cf(emu, "_CFArrayGetCount", &[a])?;
    let v = if n == 0 { 0 } else { cf(emu, "_CFArrayGetValueAtIndex", &[a, 0])? };
    emu.ret(v);
    Ok(())
}

fn array_last(emu: &mut Emu) -> Result<()> {
    let a = emu.arg(0);
    let n = cf(emu, "_CFArrayGetCount", &[a])?;
    let v = if n == 0 { 0 } else { cf(emu, "_CFArrayGetValueAtIndex", &[a, n - 1])? };
    emu.ret(v);
    Ok(())
}

fn array_add_object(emu: &mut Emu) -> Result<()> {
    if emu.arg(2) == 0 {
        return Err(Stop::Unsupported("NSInvalidArgumentException: -addObject: nil".into()));
    }
    cf(emu, "_CFArrayAppendValue", &[emu.arg(0), emu.arg(2)])?;
    Ok(())
}

fn array_add_objects_from_array(emu: &mut Emu) -> Result<()> {
    let (a, src) = (emu.arg(0), emu.arg(2));
    for o in array_values(emu, src)? {
        cf(emu, "_CFArrayAppendValue", &[a, o])?;
    }
    Ok(())
}

fn array_insert(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFArrayInsertValueAtIndex", &[emu.arg(0), emu.arg(3), emu.arg(2)])?;
    Ok(())
}

fn array_remove_at(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFArrayRemoveValueAtIndex", &[emu.arg(0), emu.arg(2)])?;
    Ok(())
}

fn array_remove_last(emu: &mut Emu) -> Result<()> {
    let a = emu.arg(0);
    let n = cf(emu, "_CFArrayGetCount", &[a])?;
    if n > 0 {
        cf(emu, "_CFArrayRemoveValueAtIndex", &[a, n - 1])?;
    }
    Ok(())
}

fn array_remove_all(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFArrayRemoveAllValues", &[emu.arg(0)])?;
    Ok(())
}

fn array_replace(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFArraySetValueAtIndex", &[emu.arg(0), emu.arg(2), emu.arg(3)])?;
    Ok(())
}

fn index_of(emu: &mut Emu, a: u64, obj: u64) -> Result<Option<u64>> {
    for (i, v) in array_values(emu, a)?.into_iter().enumerate() {
        if v == obj || send(emu, v, "isEqual:", &[obj])? & 1 != 0 {
            return Ok(Some(i as u64));
        }
    }
    Ok(None)
}

fn array_index_of_object(emu: &mut Emu) -> Result<()> {
    let r = index_of(emu, emu.arg(0), emu.arg(2))?;
    emu.ret(r.unwrap_or(i64::MAX as u64));
    Ok(())
}

fn array_contains(emu: &mut Emu) -> Result<()> {
    let r = index_of(emu, emu.arg(0), emu.arg(2))?;
    ret_bool(emu, r.is_some())
}

fn array_remove_object(emu: &mut Emu) -> Result<()> {
    let a = emu.arg(0);
    while let Some(i) = index_of(emu, a, emu.arg(2))? {
        cf(emu, "_CFArrayRemoveValueAtIndex", &[a, i])?;
    }
    Ok(())
}

fn array_copy(emu: &mut Emu) -> Result<()> {
    let v = array_values(emu, emu.arg(0))?;
    let a = array_from(emu, &v)?;
    emu.ret(a);
    Ok(())
}

/// `-enumerateObjectsUsingBlock:` — block(obj, idx, BOOL *stop)
fn array_enumerate(emu: &mut Emu) -> Result<()> {
    let (a, block) = (emu.arg(0), emu.arg(2));
    let stop = emu.alloc(8)?;
    for (i, o) in array_values(emu, a)?.into_iter().enumerate() {
        call_block(emu, block, &[o, i as u64, stop])?;
        if emu.p.memory.read_u8(stop)? != 0 {
            break;
        }
    }
    Ok(())
}

fn array_components_joined(emu: &mut Emu) -> Result<()> {
    let (a, sep) = (emu.arg(0), emu.arg(2));
    let sep = nsstring(emu, sep)?;
    let mut parts = Vec::new();
    for o in array_values(emu, a)? {
        let d = send(emu, o, "description", &[])?;
        parts.push(nsstring(emu, d)?);
    }
    ret_string(emu, &parts.join(&sep))
}

/// `-countByEnumeratingWithState:objects:count:` (for-in loops), over a snapshot.
/// state: {unsigned long state; id *itemsPtr; unsigned long *mutationsPtr; unsigned long extra[5]}
fn fast_enumerate(emu: &mut Emu, values: impl FnOnce(&mut Emu) -> Result<Vec<u64>>) -> Result<()> {
    let state = emu.arg(2);
    if emu.read_u64(state)? == 0 {
        let v = values(emu)?;
        let buf = emu.alloc(v.len() as u64 * 8 + 8)?;
        for (i, o) in v.iter().enumerate() {
            emu.write_u64(buf + i as u64 * 8, *o)?;
        }
        emu.write_u64(state + 8, buf)?; // itemsPtr: the whole snapshot
        emu.write_u64(state + 16, state + 24)?; // mutationsPtr → extra[0] (never changes)
        emu.write_u64(state + 32, v.len() as u64)?; // extra[1]: count
        emu.write_u64(state, 1)?;
        emu.ret(v.len() as u64);
    } else {
        emu.ret(0);
    }
    Ok(())
}

fn array_fast_enumerate(emu: &mut Emu) -> Result<()> {
    let a = emu.arg(0);
    fast_enumerate(emu, |emu| array_values(emu, a))
}

fn array_description(emu: &mut Emu) -> Result<()> {
    let mut parts = Vec::new();
    for o in array_values(emu, emu.arg(0))? {
        let d = send(emu, o, "description", &[])?;
        parts.push(format!("    {}", nsstring(emu, d)?));
    }
    ret_string(emu, &format!("(\n{}\n)", parts.join(",\n")))
}

// ---- dictionaries ----

fn new_dict(emu: &mut Emu) -> Result<u64> {
    let k = cf_sym(emu, "_kCFTypeDictionaryKeyCallBacks")?;
    let v = cf_sym(emu, "_kCFTypeDictionaryValueCallBacks")?;
    cf(emu, "_CFDictionaryCreateMutable", &[0, 0, k, v])
}

fn dict_pairs(emu: &mut Emu, d: u64) -> Result<Vec<(u64, u64)>> {
    let n = cf(emu, "_CFDictionaryGetCount", &[d])?;
    let keys = emu.alloc(n * 8 + 8)?;
    let vals = emu.alloc(n * 8 + 8)?;
    cf(emu, "_CFDictionaryGetKeysAndValues", &[d, keys, vals])?;
    let k = read_ptrs(emu, keys, n)?;
    let v = read_ptrs(emu, vals, n)?;
    emu.heap.free(&emu.p.memory, keys)?;
    emu.heap.free(&emu.p.memory, vals)?;
    Ok(k.into_iter().zip(v).collect())
}

fn dict_empty(emu: &mut Emu) -> Result<()> {
    let d = new_dict(emu)?;
    emu.ret(d);
    Ok(())
}

/// `+dictionaryWithObjects:forKeys:count:` (what `@{...}` compiles to).
fn dict_with_objects_keys_count(emu: &mut Emu) -> Result<()> {
    let n = emu.arg(4);
    let vals = read_ptrs(emu, emu.arg(2), n)?;
    let keys = read_ptrs(emu, emu.arg(3), n)?;
    let d = new_dict(emu)?;
    for (k, v) in keys.into_iter().zip(vals) {
        cf(emu, "_CFDictionarySetValue", &[d, k, v])?;
    }
    emu.ret(d);
    Ok(())
}

fn dict_with_object_for_key(emu: &mut Emu) -> Result<()> {
    let d = new_dict(emu)?;
    cf(emu, "_CFDictionarySetValue", &[d, emu.arg(3), emu.arg(2)])?;
    emu.ret(d);
    Ok(())
}

fn dict_with_dict(emu: &mut Emu) -> Result<()> {
    let src = emu.arg(2);
    let d = new_dict(emu)?;
    if src != 0 {
        for (k, v) in dict_pairs(emu, src)? {
            cf(emu, "_CFDictionarySetValue", &[d, k, v])?;
        }
    }
    emu.ret(d);
    Ok(())
}

fn dict_count(emu: &mut Emu) -> Result<()> {
    let n = cf(emu, "_CFDictionaryGetCount", &[emu.arg(0)])?;
    emu.ret(n);
    Ok(())
}

fn dict_object_for_key(emu: &mut Emu) -> Result<()> {
    let (d, k) = (emu.arg(0), emu.arg(2));
    let v = if k == 0 { 0 } else { cf(emu, "_CFDictionaryGetValue", &[d, k])? };
    emu.ret(v);
    Ok(())
}

fn dict_set_object(emu: &mut Emu) -> Result<()> {
    let (d, v, k) = (emu.arg(0), emu.arg(2), emu.arg(3));
    if k == 0 {
        return Err(Stop::Unsupported("NSInvalidArgumentException: dictionary key is nil".into()));
    }
    if v == 0 {
        cf(emu, "_CFDictionaryRemoveValue", &[d, k])?;
    } else {
        cf(emu, "_CFDictionarySetValue", &[d, k, v])?;
    }
    Ok(())
}

fn dict_remove(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFDictionaryRemoveValue", &[emu.arg(0), emu.arg(2)])?;
    Ok(())
}

fn dict_remove_all(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFDictionaryRemoveAllValues", &[emu.arg(0)])?;
    Ok(())
}

fn dict_add_entries(emu: &mut Emu) -> Result<()> {
    let (d, src) = (emu.arg(0), emu.arg(2));
    for (k, v) in dict_pairs(emu, src)? {
        cf(emu, "_CFDictionarySetValue", &[d, k, v])?;
    }
    Ok(())
}

fn dict_all_keys(emu: &mut Emu) -> Result<()> {
    let keys: Vec<u64> = dict_pairs(emu, emu.arg(0))?.into_iter().map(|p| p.0).collect();
    let a = array_from(emu, &keys)?;
    emu.ret(a);
    Ok(())
}

fn dict_all_values(emu: &mut Emu) -> Result<()> {
    let vals: Vec<u64> = dict_pairs(emu, emu.arg(0))?.into_iter().map(|p| p.1).collect();
    let a = array_from(emu, &vals)?;
    emu.ret(a);
    Ok(())
}

fn dict_copy(emu: &mut Emu) -> Result<()> {
    dict_with_dict_of(emu, emu.arg(0))
}

fn dict_with_dict_of(emu: &mut Emu, src: u64) -> Result<()> {
    let d = new_dict(emu)?;
    for (k, v) in dict_pairs(emu, src)? {
        cf(emu, "_CFDictionarySetValue", &[d, k, v])?;
    }
    emu.ret(d);
    Ok(())
}

/// `-enumerateKeysAndObjectsUsingBlock:` — block(key, obj, BOOL *stop)
fn dict_enumerate(emu: &mut Emu) -> Result<()> {
    let (d, block) = (emu.arg(0), emu.arg(2));
    let stop = emu.alloc(8)?;
    for (k, v) in dict_pairs(emu, d)? {
        call_block(emu, block, &[k, v, stop])?;
        if emu.p.memory.read_u8(stop)? != 0 {
            break;
        }
    }
    Ok(())
}

fn dict_fast_enumerate(emu: &mut Emu) -> Result<()> {
    let d = emu.arg(0);
    fast_enumerate(emu, |emu| Ok(dict_pairs(emu, d)?.into_iter().map(|p| p.0).collect()))
}

fn dict_description(emu: &mut Emu) -> Result<()> {
    let mut parts = Vec::new();
    for (k, v) in dict_pairs(emu, emu.arg(0))? {
        let kd = send(emu, k, "description", &[])?;
        let vd = send(emu, v, "description", &[])?;
        parts.push(format!("    {} = {};", nsstring(emu, kd)?, nsstring(emu, vd)?));
    }
    ret_string(emu, &format!("{{\n{}\n}}", parts.join("\n")))
}

// ---- sets ----

fn new_set(emu: &mut Emu) -> Result<u64> {
    let cb = cf_sym(emu, "_kCFTypeSetCallBacks")?;
    cf(emu, "_CFSetCreateMutable", &[0, 0, cb])
}

fn set_values(emu: &mut Emu, s: u64) -> Result<Vec<u64>> {
    let n = cf(emu, "_CFSetGetCount", &[s])?;
    let buf = emu.alloc(n * 8 + 8)?;
    cf(emu, "_CFSetGetValues", &[s, buf])?;
    let v = read_ptrs(emu, buf, n)?;
    emu.heap.free(&emu.p.memory, buf)?;
    Ok(v)
}

fn set_empty(emu: &mut Emu) -> Result<()> {
    let s = new_set(emu)?;
    emu.ret(s);
    Ok(())
}

fn set_with_array(emu: &mut Emu) -> Result<()> {
    let src = emu.arg(2);
    let s = new_set(emu)?;
    if src != 0 {
        for o in array_values(emu, src)? {
            cf(emu, "_CFSetAddValue", &[s, o])?;
        }
    }
    emu.ret(s);
    Ok(())
}

fn set_with_objects_count(emu: &mut Emu) -> Result<()> {
    let objs = read_ptrs(emu, emu.arg(2), emu.arg(3))?;
    let s = new_set(emu)?;
    for o in objs {
        cf(emu, "_CFSetAddValue", &[s, o])?;
    }
    emu.ret(s);
    Ok(())
}

fn set_with_object(emu: &mut Emu) -> Result<()> {
    let s = new_set(emu)?;
    cf(emu, "_CFSetAddValue", &[s, emu.arg(2)])?;
    emu.ret(s);
    Ok(())
}

fn set_count(emu: &mut Emu) -> Result<()> {
    let n = cf(emu, "_CFSetGetCount", &[emu.arg(0)])?;
    emu.ret(n);
    Ok(())
}

fn set_add(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFSetAddValue", &[emu.arg(0), emu.arg(2)])?;
    Ok(())
}

fn set_remove(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFSetRemoveValue", &[emu.arg(0), emu.arg(2)])?;
    Ok(())
}

fn set_remove_all(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFSetRemoveAllValues", &[emu.arg(0)])?;
    Ok(())
}

fn set_contains(emu: &mut Emu) -> Result<()> {
    let r = cf(emu, "_CFSetContainsValue", &[emu.arg(0), emu.arg(2)])? & 0xff;
    ret_bool(emu, r != 0)
}

fn set_member(emu: &mut Emu) -> Result<()> {
    let v = cf(emu, "_CFSetGetValue", &[emu.arg(0), emu.arg(2)])?;
    emu.ret(v);
    Ok(())
}

fn set_all_objects(emu: &mut Emu) -> Result<()> {
    let v = set_values(emu, emu.arg(0))?;
    let a = array_from(emu, &v)?;
    emu.ret(a);
    Ok(())
}

fn set_any_object(emu: &mut Emu) -> Result<()> {
    let v = set_values(emu, emu.arg(0))?;
    emu.ret(v.first().copied().unwrap_or(0));
    Ok(())
}

fn set_copy(emu: &mut Emu) -> Result<()> {
    let v = set_values(emu, emu.arg(0))?;
    let s = new_set(emu)?;
    for o in v {
        cf(emu, "_CFSetAddValue", &[s, o])?;
    }
    emu.ret(s);
    Ok(())
}

fn set_fast_enumerate(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    fast_enumerate(emu, |emu| set_values(emu, s))
}

/// `-enumerateObjectsUsingBlock:` on a set — block(obj, BOOL *stop)
fn set_enumerate(emu: &mut Emu) -> Result<()> {
    let (s, block) = (emu.arg(0), emu.arg(2));
    let stop = emu.alloc(8)?;
    for o in set_values(emu, s)? {
        call_block(emu, block, &[o, stop])?;
        if emu.p.memory.read_u8(stop)? != 0 {
            break;
        }
    }
    Ok(())
}

// ---- NSNumber ----

fn number_create(emu: &mut Emu, kind: u64, bits: u64) -> Result<u64> {
    let tmp = emu.alloc(8)?;
    emu.write_u64(tmp, bits)?;
    let n = cf(emu, "_CFNumberCreate", &[0, kind, tmp])?;
    emu.heap.free(&emu.p.memory, tmp)?;
    Ok(n)
}

fn ret_number(emu: &mut Emu, kind: u64, bits: u64) -> Result<()> {
    let n = number_create(emu, kind, bits)?;
    emu.ret(n);
    Ok(())
}

fn number_int(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_SINT32, emu.arg(2) as u32 as u64)
}

fn number_long(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_SINT64, emu.arg(2))
}

fn number_unsigned_int(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_SINT64, emu.arg(2) as u32 as u64)
}

fn number_short(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_SINT32, emu.arg(2) as i16 as i32 as u32 as u64)
}

fn number_char(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_SINT32, emu.arg(2) as i8 as i32 as u32 as u64)
}

fn number_double(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_FLOAT64, emu.cpu.q[0] as u64)
}

fn number_float(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_FLOAT32, emu.cpu.q[0] as u32 as u64)
}

fn number_bool(emu: &mut Emu) -> Result<()> {
    let v = if emu.arg(2) & 0xff != 0 { emu.fnd.true_obj } else { emu.fnd.false_obj };
    emu.ret(v);
    Ok(())
}

fn is_boolean(emu: &Emu, n: u64) -> bool {
    n == emu.fnd.true_obj || n == emu.fnd.false_obj
}

/// Read a number as (i64, f64).
fn number_value(emu: &mut Emu, n: u64) -> Result<(i64, f64)> {
    if n == 0 {
        return Ok((0, 0.0));
    }
    if is_boolean(emu, n) {
        let b = (n == emu.fnd.true_obj) as i64;
        return Ok((b, b as f64));
    }
    let tmp = emu.alloc(8)?;
    let float = cf(emu, "_CFNumberIsFloatType", &[n])? & 0xff != 0;
    let r = if float {
        cf(emu, "_CFNumberGetValue", &[n, NUMBER_FLOAT64, tmp])?;
        let d = f64::from_bits(emu.read_u64(tmp)?);
        (d as i64, d)
    } else {
        cf(emu, "_CFNumberGetValue", &[n, NUMBER_SINT64, tmp])?;
        let i = emu.read_u64(tmp)? as i64;
        (i, i as f64)
    };
    emu.heap.free(&emu.p.memory, tmp)?;
    Ok(r)
}

fn number_int_value(emu: &mut Emu) -> Result<()> {
    let (i, _) = number_value(emu, emu.arg(0))?;
    emu.ret(i as i32 as u32 as u64);
    Ok(())
}

fn number_integer_value(emu: &mut Emu) -> Result<()> {
    let (i, _) = number_value(emu, emu.arg(0))?;
    emu.ret(i as u64);
    Ok(())
}

fn number_bool_value(emu: &mut Emu) -> Result<()> {
    let (i, d) = number_value(emu, emu.arg(0))?;
    ret_bool(emu, i != 0 || d != 0.0)
}

fn number_double_value(emu: &mut Emu) -> Result<()> {
    let (_, d) = number_value(emu, emu.arg(0))?;
    emu.cpu.q[0] = d.to_bits() as u128;
    Ok(())
}

fn number_float_value(emu: &mut Emu) -> Result<()> {
    let (_, d) = number_value(emu, emu.arg(0))?;
    emu.cpu.q[0] = (d as f32).to_bits() as u128;
    Ok(())
}

fn number_string_value(emu: &mut Emu) -> Result<()> {
    let n = emu.arg(0);
    let s = if is_boolean(emu, n) {
        if n == emu.fnd.true_obj { "1".to_string() } else { "0".to_string() }
    } else {
        let float = cf(emu, "_CFNumberIsFloatType", &[n])? & 0xff != 0;
        let (i, d) = number_value(emu, n)?;
        if float { format!("{d}") } else { i.to_string() }
    };
    ret_string(emu, &s)
}

fn number_compare(emu: &mut Emu) -> Result<()> {
    let (_, a) = number_value(emu, emu.arg(0))?;
    let (_, b) = number_value(emu, emu.arg(2))?;
    let r = a.partial_cmp(&b).map_or(0, |o| o as i64);
    emu.ret(r as u64);
    Ok(())
}

fn number_is_equal(emu: &mut Emu) -> Result<()> {
    let other = emu.arg(2);
    let num = host_class(emu, "NSNumber")?;
    if other == 0 || !objc::is_kind_of(emu, other, num)? {
        return ret_bool(emu, false);
    }
    let (ia, a) = number_value(emu, emu.arg(0))?;
    let (ib, b) = number_value(emu, other)?;
    ret_bool(emu, ia == ib && a == b)
}

fn number_hash(emu: &mut Emu) -> Result<()> {
    let (i, _) = number_value(emu, emu.arg(0))?;
    emu.ret(i as u64);
    Ok(())
}

fn null_null(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.fnd.null_obj);
    Ok(())
}

fn cf_description(emu: &mut Emu) -> Result<()> {
    // CF's own description for CF objects (strings return themselves).
    let d = cf(emu, "_CFCopyDescription", &[emu.arg(0)])?;
    emu.ret(d);
    Ok(())
}

// ---- NSSortDescriptor: {isa, key, ascending, selector} ----

fn sort_descriptor_init(emu: &mut Emu) -> Result<()> {
    let (obj, key, asc) = (emu.arg(0), emu.arg(2), emu.arg(3) & 0xff);
    let sel = objc::intern(emu, "compare:")?;
    emu.write_u64(obj + 8, key)?;
    emu.write_u64(obj + 16, asc)?;
    emu.write_u64(obj + 24, sel)?;
    emu.ret(obj);
    Ok(())
}

fn sort_descriptor_init_selector(emu: &mut Emu) -> Result<()> {
    let sel = emu.arg(4);
    sort_descriptor_init(emu)?;
    emu.write_u64(emu.cpu.x[0] + 24, sel)
}

fn sort_descriptor_new(emu: &mut Emu) -> Result<()> {
    let obj = objc::create_instance(emu, emu.arg(0), 0)?;
    emu.cpu.x[0] = obj;
    sort_descriptor_init(emu)
}

fn sort_descriptor_key(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.arg(0) + 8)?;
    emu.ret(v);
    Ok(())
}

fn sort_descriptor_ascending(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.arg(0) + 16)?;
    emu.ret(v);
    Ok(())
}

/// Order `objs` by `descriptors` (an NSArray of NSSortDescriptor). A stable
/// merge sort; keys come from `valueForKey:`, comparison from the
/// descriptor's selector (`compare:`).
fn sort_by_descriptors(emu: &mut Emu, objs: Vec<u64>, descriptors: u64) -> Result<Vec<u64>> {
    let mut descs = Vec::new();
    for d in array_values(emu, descriptors)? {
        let key = emu.read_u64(d + 8)?;
        let asc = emu.read_u64(d + 16)? != 0;
        let sel = emu.read_u64(d + 24)?;
        descs.push((nsstring(emu, key)?, asc, objc::sel_name(emu, sel)));
    }
    // Precompute each object's sort keys.
    let mut keyed = Vec::new();
    for o in objs {
        let mut keys = Vec::new();
        for (key, _, _) in &descs {
            let k = new_nsstring(emu, key)?;
            keys.push(send(emu, o, "valueForKey:", &[k])?);
        }
        keyed.push((o, keys));
    }
    // Merge sort with a fallible comparator.
    fn cmp(emu: &mut Emu, a: &[u64], b: &[u64], descs: &[(String, bool, String)]) -> Result<i64> {
        for (i, (_, asc, sel)) in descs.iter().enumerate() {
            let r = if a[i] == 0 || b[i] == 0 { (a[i] != 0) as i64 - (b[i] != 0) as i64 } else { send(emu, a[i], sel, &[b[i]])? as i64 };
            if r != 0 {
                return Ok(if *asc { r } else { -r });
            }
        }
        Ok(0)
    }
    let mut v = keyed;
    let mut width = 1;
    while width < v.len() {
        let mut out = Vec::with_capacity(v.len());
        for chunk in v.chunks(width * 2) {
            let (l, r) = chunk.split_at(width.min(chunk.len()));
            let (mut i, mut j) = (0, 0);
            while i < l.len() && j < r.len() {
                if cmp(emu, &r[j].1, &l[i].1, &descs)? < 0 {
                    out.push(r[j].clone());
                    j += 1;
                } else {
                    out.push(l[i].clone());
                    i += 1;
                }
            }
            out.extend_from_slice(&l[i..]);
            out.extend_from_slice(&r[j..]);
        }
        v = out;
        width *= 2;
    }
    Ok(v.into_iter().map(|(o, _)| o).collect())
}

fn sort_using_descriptors(emu: &mut Emu) -> Result<()> {
    let (a, descs) = (emu.arg(0), emu.arg(2));
    let objs = array_values(emu, a)?;
    let sorted = sort_by_descriptors(emu, objs, descs)?;
    for (i, o) in sorted.into_iter().enumerate() {
        cf(emu, "_CFArraySetValueAtIndex", &[a, i as u64, o])?;
    }
    Ok(())
}

fn sorted_using_descriptors(emu: &mut Emu) -> Result<()> {
    let (a, descs) = (emu.arg(0), emu.arg(2));
    let objs = array_values(emu, a)?;
    let sorted = sort_by_descriptors(emu, objs, descs)?;
    let r = array_from(emu, &sorted)?;
    emu.ret(r);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSMutableArray", false, "sortUsingDescriptors:", sort_using_descriptors),
    ("NSArray", false, "sortedArrayUsingDescriptors:", sorted_using_descriptors),
    ("NSSortDescriptor", false, "initWithKey:ascending:", sort_descriptor_init),
    ("NSSortDescriptor", false, "initWithKey:ascending:selector:", sort_descriptor_init_selector),
    ("NSSortDescriptor", true, "sortDescriptorWithKey:ascending:", sort_descriptor_new),
    ("NSSortDescriptor", false, "key", sort_descriptor_key),
    ("NSSortDescriptor", false, "ascending", sort_descriptor_ascending),
    // class clusters
    ("NSString", true, "alloc", cluster_alloc),
    ("NSString", true, "allocWithZone:", cluster_alloc),
    ("NSArray", true, "alloc", cluster_alloc),
    ("NSArray", true, "allocWithZone:", cluster_alloc),
    ("NSDictionary", true, "alloc", cluster_alloc),
    ("NSDictionary", true, "allocWithZone:", cluster_alloc),
    ("NSSet", true, "alloc", cluster_alloc),
    ("NSSet", true, "allocWithZone:", cluster_alloc),
    ("NSNumber", true, "alloc", cluster_alloc),
    ("NSNumber", true, "allocWithZone:", cluster_alloc),
    // NSString
    ("NSString", true, "string", empty_string),
    ("NSString", true, "new", empty_string),
    ("NSString", false, "init", empty_string),
    ("NSString", true, "stringWithUTF8String:", string_with_utf8),
    ("NSString", false, "initWithUTF8String:", string_with_utf8),
    ("NSString", true, "stringWithString:", string_with_string),
    ("NSString", false, "initWithString:", string_with_string),
    ("NSString", true, "stringWithFormat:", string_with_format),
    ("NSString", false, "initWithFormat:", string_with_format),
    ("NSString", false, "length", string_length),
    ("NSString", false, "UTF8String", utf8_string),
    ("NSString", false, "characterAtIndex:", character_at_index),
    ("NSString", false, "isEqualToString:", is_equal_to_string),
    ("NSString", false, "isEqual:", string_is_equal),
    ("NSString", false, "hash", string_hash),
    ("NSString", false, "copy", string_copy),
    ("NSString", false, "copyWithZone:", string_copy),
    ("NSString", false, "mutableCopy", string_mutable_copy),
    ("NSString", false, "description", ret_self),
    ("NSString", false, "stringByAppendingString:", string_by_appending),
    ("NSString", false, "hasPrefix:", has_prefix),
    ("NSString", false, "hasSuffix:", has_suffix),
    ("NSString", false, "lowercaseString", lowercase),
    ("NSString", false, "uppercaseString", uppercase),
    ("NSString", false, "compare:", string_compare),
    ("NSString", false, "intValue", integer_value),
    ("NSString", false, "integerValue", integer_value),
    ("NSString", false, "longLongValue", integer_value),
    ("NSString", false, "doubleValue", double_value),
    ("NSString", false, "boolValue", bool_value_string),
    ("NSString", false, "componentsSeparatedByString:", components_separated),
    ("NSString", false, "substringFromIndex:", substring_from),
    ("NSString", false, "substringToIndex:", substring_to),
    ("NSString", false, "rangeOfString:", range_of_string),
    ("NSString", false, "containsString:", contains_string),
    ("NSString", false, "getCString:maxLength:encoding:", get_cstring),
    ("NSString", false, "lengthOfBytesUsingEncoding:", length_of_bytes),
    ("NSString", false, "cStringUsingEncoding:", utf8_string),
    ("NSMutableString", false, "appendString:", append_string),
    ("NSString", false, "_fastCStringContents:", fast_cstring_contents),
    ("NSString", false, "_fastCharacterContents", fast_character_contents),
    ("NSString", false, "getCharacters:range:", get_characters_range),
    ("NSString", false, "getBytes:maxLength:usedLength:encoding:options:range:remainingRange:", get_bytes),
    ("NSString", false, "compare:options:range:locale:", compare_options_range),
    ("NSString", true, "newTaggedNSStringWithASCIIBytes_:length_:", ret_nil),
    ("NSString", false, "isNSString__", ret_yes),
    // NSArray
    ("NSArray", true, "array", array_empty),
    ("NSArray", true, "new", array_empty),
    ("NSArray", false, "init", array_init),
    ("NSArray", true, "arrayWithObjects:count:", array_with_objects_count),
    ("NSArray", false, "initWithObjects:count:", array_with_objects_count),
    ("NSArray", true, "arrayWithObject:", array_with_object),
    ("NSArray", true, "arrayWithObjects:", array_with_objects_varargs),
    ("NSArray", true, "arrayWithArray:", array_with_array),
    ("NSArray", false, "initWithArray:", array_with_array),
    ("NSMutableArray", true, "arrayWithCapacity:", array_empty),
    ("NSMutableArray", false, "initWithCapacity:", array_init),
    ("NSArray", false, "count", array_count),
    ("NSArray", false, "objectAtIndex:", array_object_at_index),
    ("NSArray", false, "objectAtIndexedSubscript:", array_object_at_index),
    ("NSArray", false, "firstObject", array_first),
    ("NSArray", false, "lastObject", array_last),
    ("NSArray", false, "indexOfObject:", array_index_of_object),
    ("NSArray", false, "containsObject:", array_contains),
    ("NSArray", false, "copy", array_copy),
    ("NSArray", false, "copyWithZone:", array_copy),
    ("NSArray", false, "mutableCopy", array_copy),
    ("NSArray", false, "enumerateObjectsUsingBlock:", array_enumerate),
    ("NSArray", false, "componentsJoinedByString:", array_components_joined),
    ("NSArray", false, "countByEnumeratingWithState:objects:count:", array_fast_enumerate),
    ("NSArray", false, "description", array_description),
    ("NSMutableArray", false, "addObject:", array_add_object),
    ("NSMutableArray", false, "addObjectsFromArray:", array_add_objects_from_array),
    ("NSMutableArray", false, "insertObject:atIndex:", array_insert),
    ("NSMutableArray", false, "removeObjectAtIndex:", array_remove_at),
    ("NSMutableArray", false, "removeLastObject", array_remove_last),
    ("NSMutableArray", false, "removeAllObjects", array_remove_all),
    ("NSMutableArray", false, "removeObject:", array_remove_object),
    ("NSMutableArray", false, "replaceObjectAtIndex:withObject:", array_replace),
    ("NSMutableArray", false, "setObject:atIndexedSubscript:", array_replace),
    // NSDictionary
    ("NSDictionary", true, "dictionary", dict_empty),
    ("NSDictionary", true, "new", dict_empty),
    ("NSDictionary", false, "init", dict_empty),
    ("NSDictionary", true, "dictionaryWithObjects:forKeys:count:", dict_with_objects_keys_count),
    ("NSDictionary", false, "initWithObjects:forKeys:count:", dict_with_objects_keys_count),
    ("NSDictionary", true, "dictionaryWithObject:forKey:", dict_with_object_for_key),
    ("NSDictionary", true, "dictionaryWithDictionary:", dict_with_dict),
    ("NSDictionary", false, "initWithDictionary:", dict_with_dict),
    ("NSMutableDictionary", true, "dictionaryWithCapacity:", dict_empty),
    ("NSMutableDictionary", false, "initWithCapacity:", dict_empty),
    ("NSDictionary", false, "count", dict_count),
    ("NSDictionary", false, "objectForKey:", dict_object_for_key),
    ("NSDictionary", false, "objectForKeyedSubscript:", dict_object_for_key),
    ("NSDictionary", false, "valueForKey:", dict_object_for_key),
    ("NSDictionary", false, "allKeys", dict_all_keys),
    ("NSDictionary", false, "allValues", dict_all_values),
    ("NSDictionary", false, "copy", dict_copy),
    ("NSDictionary", false, "copyWithZone:", dict_copy),
    ("NSDictionary", false, "mutableCopy", dict_copy),
    ("NSDictionary", false, "enumerateKeysAndObjectsUsingBlock:", dict_enumerate),
    ("NSDictionary", false, "countByEnumeratingWithState:objects:count:", dict_fast_enumerate),
    ("NSDictionary", false, "description", dict_description),
    ("NSMutableDictionary", false, "setObject:forKey:", dict_set_object),
    ("NSMutableDictionary", false, "setObject:forKeyedSubscript:", dict_set_object),
    ("NSMutableDictionary", false, "setValue:forKey:", dict_set_object),
    ("NSMutableDictionary", false, "removeObjectForKey:", dict_remove),
    ("NSMutableDictionary", false, "removeAllObjects", dict_remove_all),
    ("NSMutableDictionary", false, "addEntriesFromDictionary:", dict_add_entries),
    // NSSet
    ("NSSet", true, "set", set_empty),
    ("NSSet", true, "new", set_empty),
    ("NSSet", false, "init", set_empty),
    ("NSSet", true, "setWithArray:", set_with_array),
    ("NSSet", false, "initWithArray:", set_with_array),
    ("NSSet", true, "setWithObjects:count:", set_with_objects_count),
    ("NSSet", false, "initWithObjects:count:", set_with_objects_count),
    ("NSSet", true, "setWithObject:", set_with_object),
    ("NSMutableSet", true, "setWithCapacity:", set_empty),
    ("NSMutableSet", false, "initWithCapacity:", set_empty),
    ("NSSet", false, "count", set_count),
    ("NSSet", false, "containsObject:", set_contains),
    ("NSSet", false, "member:", set_member),
    ("NSSet", false, "allObjects", set_all_objects),
    ("NSSet", false, "anyObject", set_any_object),
    ("NSSet", false, "copy", set_copy),
    ("NSSet", false, "copyWithZone:", set_copy),
    ("NSSet", false, "mutableCopy", set_copy),
    ("NSSet", false, "countByEnumeratingWithState:objects:count:", set_fast_enumerate),
    ("NSSet", false, "enumerateObjectsUsingBlock:", set_enumerate),
    ("NSMutableSet", false, "addObject:", set_add),
    ("NSMutableSet", false, "removeObject:", set_remove),
    ("NSMutableSet", false, "removeAllObjects", set_remove_all),
    // NSNumber
    ("NSNumber", true, "numberWithInt:", number_int),
    ("NSNumber", false, "initWithInt:", number_int),
    ("NSNumber", true, "numberWithInteger:", number_long),
    ("NSNumber", false, "initWithInteger:", number_long),
    ("NSNumber", true, "numberWithLong:", number_long),
    ("NSNumber", true, "numberWithLongLong:", number_long),
    ("NSNumber", false, "initWithLongLong:", number_long),
    ("NSNumber", true, "numberWithUnsignedInt:", number_unsigned_int),
    ("NSNumber", true, "numberWithUnsignedInteger:", number_long),
    ("NSNumber", true, "numberWithUnsignedLong:", number_long),
    ("NSNumber", true, "numberWithUnsignedLongLong:", number_long),
    ("NSNumber", true, "numberWithShort:", number_short),
    ("NSNumber", true, "numberWithChar:", number_char),
    ("NSNumber", true, "numberWithDouble:", number_double),
    ("NSNumber", false, "initWithDouble:", number_double),
    ("NSNumber", true, "numberWithFloat:", number_float),
    ("NSNumber", false, "initWithFloat:", number_float),
    ("NSNumber", true, "numberWithBool:", number_bool),
    ("NSNumber", false, "initWithBool:", number_bool),
    ("NSNumber", false, "intValue", number_int_value),
    ("NSNumber", false, "integerValue", number_integer_value),
    ("NSNumber", false, "longValue", number_integer_value),
    ("NSNumber", false, "longLongValue", number_integer_value),
    ("NSNumber", false, "unsignedIntValue", number_int_value),
    ("NSNumber", false, "unsignedIntegerValue", number_integer_value),
    ("NSNumber", false, "unsignedLongLongValue", number_integer_value),
    ("NSNumber", false, "boolValue", number_bool_value),
    ("NSNumber", false, "doubleValue", number_double_value),
    ("NSNumber", false, "floatValue", number_float_value),
    ("NSNumber", false, "stringValue", number_string_value),
    ("NSNumber", false, "description", number_string_value),
    ("NSNumber", false, "compare:", number_compare),
    ("NSNumber", false, "isEqualToNumber:", number_is_equal),
    ("NSNumber", false, "isEqual:", number_is_equal),
    ("NSNumber", false, "hash", number_hash),
    ("NSNumber", false, "copy", ret_self),
    ("NSNumber", false, "copyWithZone:", ret_self),
    // NSNull
    ("NSNull", true, "null", null_null),
    // Every other CF type
    ("__NSCFType", false, "description", cf_description),
];

// ---- helpers for other modules ----

pub fn boolean(emu: &Emu, b: bool) -> u64 {
    if b { emu.fnd.true_obj } else { emu.fnd.false_obj }
}

pub fn null(emu: &Emu) -> u64 {
    emu.fnd.null_obj
}

pub fn new_number_i64(emu: &mut Emu, v: i64) -> Result<u64> {
    number_create(emu, NUMBER_SINT64, v as u64)
}

pub fn new_number_f64(emu: &mut Emu, v: f64) -> Result<u64> {
    number_create(emu, NUMBER_FLOAT64, v.to_bits())
}

pub fn new_array_of(emu: &mut Emu, objs: &[u64]) -> Result<u64> {
    array_from(emu, objs)
}

pub fn new_dict_of(emu: &mut Emu, pairs: &[(u64, u64)]) -> Result<u64> {
    let d = new_dict(emu)?;
    for &(k, v) in pairs {
        cf(emu, "_CFDictionarySetValue", &[d, k, v])?;
    }
    Ok(d)
}

pub fn dict_get(emu: &mut Emu, d: u64, k: u64) -> Result<u64> {
    cf(emu, "_CFDictionaryGetValue", &[d, k])
}

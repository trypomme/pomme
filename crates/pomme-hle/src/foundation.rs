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
    pub(crate) null_obj: u64,
    /// NSRunLoop object for each CFRunLoop.
    run_loops: HashMap<u64, u64>,
    center: u64,
    observers: Vec<Observer>,
    file_manager: u64,
    defaults: u64,
    process_info: u64,
    /// CF type ID → bridged class.
    type_classes: HashMap<u64, u64>,
}

pub(crate) fn init(_emu: &mut Emu) -> Result<()> {
    Ok(())
}

// ---- calling CoreFoundation ----

pub(crate) fn cf_sym(emu: &mut Emu, name: &'static str) -> Result<u64> {
    if let Some(&a) = emu.fnd.syms.get(name) {
        return Ok(a);
    }
    let img = emu.fnd.cf.ok_or_else(|| Stop::Unsupported(format!("{name}: CoreFoundation isn't loaded (run scripts/build-sysroot.sh)")))?;
    let a = emu.p.images[img].symbol(name).ok_or_else(|| Stop::Unsupported(format!("CoreFoundation has no {name}")))?;
    emu.fnd.syms.insert(name, a);
    Ok(a)
}

/// Call a CoreFoundation function with integer arguments.
pub(crate) fn cf(emu: &mut Emu, name: &'static str, args: &[u64]) -> Result<u64> {
    let f = cf_sym(emu, name)?;
    emu.call(f, args)
}

/// Call a CoreFoundation function (for other modules).
pub fn cf_call(emu: &mut Emu, name: &'static str, args: &[u64]) -> Result<u64> {
    cf(emu, name, args)
}

pub fn cf_data_pub(emu: &mut Emu, name: &'static str) -> Result<u64> {
    cf_data(emu, name)
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

/// The bridged class for a CF object whose isa is still 0 (created during CF's own initialization).
pub fn class_for_unbridged(emu: &mut Emu, obj: u64) -> Result<u64> {
    if emu.fnd.cf.is_none() {
        return Err(Stop::Unsupported(format!("object {obj:#x} has no class (isa 0)")));
    }
    let id = cf(emu, "_CFGetTypeID", &[obj])?;
    match emu.fnd.type_classes.get(&id) {
        Some(&c) => Ok(c),
        None => host_class(emu, "__NSCFType"),
    }
}

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
        ("_CFRunLoopTimerGetTypeID", "__NSCFTimer"),
        ("_CFURLGetTypeID", "NSURL"),
        ("_CFCharacterSetGetTypeID", "__NSCFCharacterSet"),
    ];
    for &(type_fn, class) in types {
        let id = cf(emu, type_fn, &[])?;
        let cls = host_class(emu, class)?;
        emu.call(bridge_fn, &[id, cls])?;
        emu.fnd.cf_classes.insert(cls);
        emu.fnd.type_classes.insert(id, cls);
    }

    // Objects CF made while initializing still have isa 0.
    for (sym, class) in [("_kCFBooleanTrue", "__NSCFBoolean"), ("_kCFBooleanFalse", "__NSCFBoolean"), ("_kCFNull", "NSNull")] {
        let obj = cf_data(emu, sym)?;
        let cls = host_class(emu, class)?;
        emu.write_u64(obj, cls)?;
    }
    // Foundation's NSString constants that are CF's under another name.
    for (ns, cfname) in [("_NSDefaultRunLoopMode", "_kCFRunLoopDefaultMode"), ("_NSRunLoopCommonModes", "_kCFRunLoopCommonModes")] {
        let stubs = emu.p.stubs.find(ns);
        if stubs.is_empty() {
            continue;
        }
        let value = cf_data(emu, cfname)?;
        let cell = emu.alloc(8)?;
        emu.write_u64(cell, value)?;
        for s in stubs {
            emu.p.rebind(s, cell).map_err(|e| Stop::Unsupported(e.to_string()))?;
        }
    }
    crate::constants::bind(emu)?;
    // `@[]` and friends compile to references to Apple's empty-collection singletons.
    for (sym, make) in [
        ("___NSArray0__struct", new_array as fn(&mut Emu, bool) -> Result<u64>),
        ("___NSDictionary0__struct", |e: &mut Emu, _| new_dict(e)),
        ("___NSSet0__struct", |e: &mut Emu, _| new_set(e)),
    ] {
        let stubs = emu.p.stubs.find(sym);
        if !stubs.is_empty() {
            let obj = make(emu, false)?;
            for s in stubs {
                emu.p.rebind(s, obj).map_err(|e| Stop::Unsupported(e.to_string()))?;
            }
        }
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
        ("_CFAutorelease", hook_autorelease),
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
    emu.hook(release, hook_collection_release)?;
    Ok(())
}


/// `CFRetain(obj)` → obj. CF objects are never freed, so only Swift objects count.
fn hook_retain(emu: &mut Emu) -> Result<()> {
    objc::objc_retain_obj(emu, emu.arg(0)) // returns its argument
}

fn hook_release(emu: &mut Emu) -> Result<()> {
    objc::objc_release_obj(emu, emu.arg(0))
}

fn hook_autorelease(_: &mut Emu) -> Result<()> {
    Ok(()) // pools are never drained: autorelease keeps the reference
}

/// `__CFTypeCollectionRetain(allocator, ptr)` → ptr
fn hook_collection_retain(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(1);
    objc::objc_retain_obj(emu, p)?;
    emu.ret(p);
    Ok(())
}

/// `__CFTypeCollectionRelease(allocator, ptr)`
fn hook_collection_release(emu: &mut Emu) -> Result<()> {
    objc::objc_release_obj(emu, emu.arg(1))
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

pub(crate) fn cluster_alloc(emu: &mut Emu) -> Result<()> {
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
pub(crate) fn wants_mutable(emu: &mut Emu, obj: u64, mutable_class: &str) -> Result<bool> {
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

/// `-initWithFormat:arguments:` — format x2, va_list x3.
fn string_with_format_va(emu: &mut Emu) -> Result<()> {
    let fmt = nsstring(emu, emu.arg(2))?;
    let s = crate::printf::format(emu, fmt.as_bytes(), &mut crate::printf::Args::new(emu.arg(3)))?;
    ret_string(emu, &String::from_utf8_lossy(&s))
}

fn string_by_appending(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    ret_string(emu, &(a + &b))
}

fn append_string(emu: &mut Emu) -> Result<()> {
    let s = as_cf_string(emu, emu.arg(2))?;
    cf(emu, "_CFStringAppend", &[emu.arg(0), s])?;
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

pub(crate) fn array_from(emu: &mut Emu, objs: &[u64]) -> Result<u64> {
    let a = new_array(emu, true)?;
    for &o in objs {
        cf(emu, "_CFArrayAppendValue", &[a, o])?;
    }
    Ok(a)
}

fn read_ptrs(emu: &Emu, at: u64, n: u64) -> Result<Vec<u64>> {
    (0..n).map(|i| emu.read_u64(at + i * 8)).collect()
}

/// Compile-time literals (`@1.5`, `@[...]`, `@{...}`) emitted by clang as
/// ready-made objects in the binary. Not CF objects: read their layout.
#[derive(Clone, Copy, PartialEq)]
enum Constant {
    /// {isa, const char *encoding, long long value}
    Integer,
    /// {isa, double value}
    Double,
    /// {isa, float value}
    Float,
    /// {isa, NSUInteger count, id const *objects}
    Array,
    /// {isa, NSUInteger options, NSUInteger count, id const *keys, id const *objects}
    Dictionary,
}

fn constant_kind(emu: &Emu, obj: u64) -> Option<Constant> {
    if obj == 0 || obj >> 63 != 0 {
        return None;
    }
    let isa = emu.read_u64(obj).ok()?;
    let name = emu.objc.class(isa)?.name.as_str();
    Some(match name {
        "NSConstantIntegerNumber" => Constant::Integer,
        "NSConstantDoubleNumber" => Constant::Double,
        "NSConstantFloatNumber" => Constant::Float,
        "NSConstantArray" => Constant::Array,
        "NSConstantDictionary" => Constant::Dictionary,
        _ => return None,
    })
}

fn constant_array_values(emu: &Emu, a: u64) -> Result<Vec<u64>> {
    let n = emu.read_u64(a + 8)?;
    let objs = emu.read_u64(a + 16)?;
    read_ptrs(emu, objs, n)
}

fn constant_dict_pairs(emu: &Emu, d: u64) -> Result<Vec<(u64, u64)>> {
    let n = emu.read_u64(d + 16)?;
    let keys = read_ptrs(emu, emu.read_u64(d + 24)?, n)?;
    let vals = read_ptrs(emu, emu.read_u64(d + 32)?, n)?;
    Ok(keys.into_iter().zip(vals).collect())
}

pub(crate) fn array_values(emu: &mut Emu, arr: u64) -> Result<Vec<u64>> {
    if constant_kind(emu, arr) == Some(Constant::Array) {
        return constant_array_values(emu, arr);
    }
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
/// A nil-terminated `id` vararg list (arm64 Darwin: the variadic part is on the stack).
fn varargs_objects(emu: &mut Emu) -> Result<Vec<u64>> {
    let mut objs = Vec::new();
    let mut o = emu.arg(2);
    let mut at = emu.cpu.sp;
    while o != 0 {
        objs.push(o);
        o = emu.read_u64(at)?;
        at += 8;
    }
    Ok(objs)
}

fn set_with_objects_varargs(emu: &mut Emu) -> Result<()> {
    let objs = varargs_objects(emu)?;
    let s = new_set(emu)?;
    for o in objs {
        cf(emu, "_CFSetAddValue", &[s, o])?;
    }
    emu.ret(s);
    Ok(())
}

/// `+dictionaryWithObjectsAndKeys:` (object, key, object, key, ..., nil).
fn dict_with_objects_and_keys(emu: &mut Emu) -> Result<()> {
    let items = varargs_objects(emu)?;
    let pairs: Vec<(u64, u64)> = items.chunks_exact(2).map(|p| (p[1], p[0])).collect();
    let d = new_dict_of(emu, &pairs)?;
    emu.ret(d);
    Ok(())
}

fn array_with_objects_varargs(emu: &mut Emu) -> Result<()> {
    let objs = varargs_objects(emu)?;
    let a = array_from(emu, &objs)?;
    emu.ret(a);
    Ok(())
}

/// `-isEqualToArray:` / `-isEqual:`: same length, elements equal pairwise.
fn array_is_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    let eq = a == b || (b != 0 && {
        let cls = crate::objc::host_class(emu, "NSArray")?;
        send(emu, b, "isKindOfClass:", &[cls])? & 0xff != 0 && {
            let (x, y) = (array_values(emu, a)?, array_values(emu, b)?);
            let mut same = x.len() == y.len();
            for (p, q) in x.into_iter().zip(y) {
                if !same {
                    break;
                }
                same = p == q || send(emu, p, "isEqual:", &[q])? & 0xff != 0;
            }
            same
        }
    });
    emu.ret(eq as u64);
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

/// Put `obj` at `index` (appending when `index` is the count, as subscripting allows).
fn array_set_at(emu: &mut Emu, a: u64, index: u64, obj: u64) -> Result<()> {
    let n = cf(emu, "_CFArrayGetCount", &[a])?;
    if index > n {
        return Err(Stop::Unsupported(format!("NSRangeException: index {index} beyond bounds [0 .. {n}]")));
    }
    if index == n {
        cf(emu, "_CFArrayAppendValue", &[a, obj])?;
    } else {
        cf(emu, "_CFArraySetValueAtIndex", &[a, index, obj])?;
    }
    Ok(())
}

/// `-replaceObjectAtIndex:withObject:`
fn array_replace(emu: &mut Emu) -> Result<()> {
    let (a, index, obj) = (emu.arg(0), emu.arg(2), emu.arg(3));
    array_set_at(emu, a, index, obj)
}

/// `-setObject:atIndexedSubscript:` (object first).
fn array_set_subscript(emu: &mut Emu) -> Result<()> {
    let (a, obj, index) = (emu.arg(0), emu.arg(2), emu.arg(3));
    array_set_at(emu, a, index, obj)
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

pub(crate) fn new_dict(emu: &mut Emu) -> Result<u64> {
    let k = cf_sym(emu, "_kCFTypeDictionaryKeyCallBacks")?;
    let v = cf_sym(emu, "_kCFTypeDictionaryValueCallBacks")?;
    cf(emu, "_CFDictionaryCreateMutable", &[0, 0, k, v])
}

pub(crate) fn dict_pairs(emu: &mut Emu, d: u64) -> Result<Vec<(u64, u64)>> {
    if constant_kind(emu, d) == Some(Constant::Dictionary) {
        return constant_dict_pairs(emu, d);
    }
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

pub(crate) fn is_boolean(emu: &Emu, n: u64) -> bool {
    n == emu.fnd.true_obj || n == emu.fnd.false_obj
}

/// Read a number as (i64, f64).
fn number_value(emu: &mut Emu, n: u64) -> Result<(i64, f64)> {
    if n == 0 {
        return Ok((0, 0.0));
    }
    match constant_kind(emu, n) {
        Some(Constant::Integer) => {
            let v = emu.read_u64(n + 16)? as i64;
            return Ok((v, v as f64));
        }
        Some(Constant::Double) => {
            let d = f64::from_bits(emu.read_u64(n + 8)?);
            return Ok((d as i64, d));
        }
        Some(Constant::Float) => {
            let d = f32::from_bits(emu.p.memory.read_u32(n + 8)?) as f64;
            return Ok((d as i64, d));
        }
        _ => {}
    }
    if is_boolean(emu, n) {
        let b = (n == emu.fnd.true_obj) as i64;
        return Ok((b, b as f64));
    }
    let tmp = emu.alloc(8)?;
    let float = number_is_float(emu, n)?;
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

pub(crate) fn number_is_float(emu: &mut Emu, n: u64) -> Result<bool> {
    match constant_kind(emu, n) {
        Some(Constant::Double | Constant::Float) => Ok(true),
        Some(_) => Ok(false),
        None => Ok(cf(emu, "_CFNumberIsFloatType", &[n])? & 0xff != 0),
    }
}

/// `-count` / `-objectAtIndex:` on a constant array, and the dictionary equivalents.
fn constant_array_count(emu: &mut Emu) -> Result<()> {
    let n = emu.read_u64(emu.arg(0) + 8)?;
    emu.ret(n);
    Ok(())
}

fn constant_array_object_at(emu: &mut Emu) -> Result<()> {
    let (a, i) = (emu.arg(0), emu.arg(2));
    let v = constant_array_values(emu, a)?;
    let o = *v.get(i as usize).ok_or_else(|| Stop::Unsupported(format!("NSRangeException: index {i} beyond bounds of {}", v.len())))?;
    emu.ret(o);
    Ok(())
}

fn constant_array_first(emu: &mut Emu) -> Result<()> {
    let v = constant_array_values(emu, emu.arg(0))?;
    emu.ret(v.first().copied().unwrap_or(0));
    Ok(())
}

fn constant_array_last(emu: &mut Emu) -> Result<()> {
    let v = constant_array_values(emu, emu.arg(0))?;
    emu.ret(v.last().copied().unwrap_or(0));
    Ok(())
}

fn constant_dict_count(emu: &mut Emu) -> Result<()> {
    let n = emu.read_u64(emu.arg(0) + 16)?;
    emu.ret(n);
    Ok(())
}

fn constant_dict_object_for_key(emu: &mut Emu) -> Result<()> {
    let (d, key) = (emu.arg(0), emu.arg(2));
    for (k, v) in constant_dict_pairs(emu, d)? {
        if k == key || send(emu, k, "isEqual:", &[key])? & 1 != 0 {
            emu.ret(v);
            return Ok(());
        }
    }
    emu.ret(0);
    Ok(())
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
        let float = number_is_float(emu, n)?;
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

/// Stable bottom-up merge sort with a guest comparator (NSComparisonResult: −1, 0, 1).
fn merge_sort(emu: &mut Emu, mut v: Vec<u64>, cmp: &mut dyn FnMut(&mut Emu, u64, u64) -> Result<i64>) -> Result<Vec<u64>> {
    let mut width = 1;
    while width < v.len() {
        let mut out = Vec::with_capacity(v.len());
        for chunk in (0..v.len()).step_by(2 * width) {
            let mid = (chunk + width).min(v.len());
            let end = (chunk + 2 * width).min(v.len());
            let (mut i, mut j) = (chunk, mid);
            while i < mid && j < end {
                if cmp(emu, v[j], v[i])? < 0 {
                    out.push(v[j]);
                    j += 1;
                } else {
                    out.push(v[i]);
                    i += 1;
                }
            }
            out.extend_from_slice(&v[i..mid]);
            out.extend_from_slice(&v[j..end]);
        }
        v = out;
        width *= 2;
    }
    Ok(v)
}

/// Sort the receiver's elements with a comparator block (`block_reg`) or a selector sent to each element.
fn sort_with(emu: &mut Emu, block_reg: Option<usize>, sel_reg: Option<usize>, in_place: bool) -> Result<()> {
    let a = emu.arg(0);
    let objs = array_values(emu, a)?;
    let sorted = match (block_reg, sel_reg) {
        (Some(r), _) => {
            let block = emu.arg(r);
            merge_sort(emu, objs, &mut |e, x, y| Ok(call_block(e, block, &[x, y])? as i64))?
        }
        (_, Some(r)) => {
            let sel = objc::sel_name(emu, emu.arg(r));
            merge_sort(emu, objs, &mut |e, x, y| Ok(send(e, x, &sel, &[y])? as i64))?
        }
        _ => objs,
    };
    if in_place {
        for (i, o) in sorted.into_iter().enumerate() {
            cf(emu, "_CFArraySetValueAtIndex", &[a, i as u64, o])?;
        }
    } else {
        let r = array_from(emu, &sorted)?;
        emu.ret(r);
    }
    Ok(())
}

/// `-[NSArray valueForKey:]`: each element's value (nil → NSNull); `@count` is the count.
fn array_value_for_key(emu: &mut Emu) -> Result<()> {
    let (a, key) = (emu.arg(0), emu.arg(2));
    let objs = array_values(emu, a)?;
    if nsstring(emu, key)? == "@count" {
        let cls = host_class(emu, "NSNumber")?;
        let r = send(emu, cls, "numberWithUnsignedInteger:", &[objs.len() as u64])?;
        emu.ret(r);
        return Ok(());
    }
    let mut out = Vec::with_capacity(objs.len());
    for o in objs {
        let v = send(emu, o, "valueForKey:", &[key])?;
        out.push(if v == 0 { emu.fnd.null_obj } else { v });
    }
    let r = array_from(emu, &out)?;
    emu.ret(r);
    Ok(())
}

// ---- the primitives the Swift standard library uses on Cocoa collections ----

fn write_ptrs(emu: &mut Emu, at: u64, v: &[u64]) -> Result<()> {
    for (i, o) in v.iter().enumerate() {
        emu.write_u64(at + i as u64 * 8, *o)?;
    }
    Ok(())
}

/// `-[NSArray getObjects:range:]` (buffer, NSRange in x3/x4)
fn array_get_objects_range(emu: &mut Emu) -> Result<()> {
    let (a, buf, loc, len) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4));
    let v = array_values(emu, a)?;
    let end = (loc + len).min(v.len() as u64) as usize;
    write_ptrs(emu, buf, &v[loc.min(end as u64) as usize..end])
}

/// `-[NSDictionary getObjects:andKeys:count:]` / `getObjects:andKeys:`
fn dict_get_objects_and_keys(emu: &mut Emu) -> Result<()> {
    let (d, objs, keys) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let pairs = dict_pairs(emu, d)?;
    if keys != 0 {
        write_ptrs(emu, keys, &pairs.iter().map(|p| p.0).collect::<Vec<_>>())?;
    }
    if objs != 0 {
        write_ptrs(emu, objs, &pairs.iter().map(|p| p.1).collect::<Vec<_>>())?;
    }
    Ok(())
}

/// `-[NSSet getObjects:]` / `getObjects:count:`
fn set_get_objects(emu: &mut Emu) -> Result<()> {
    let (s, buf) = (emu.arg(0), emu.arg(2));
    let v = set_values(emu, s)?;
    write_ptrs(emu, buf, &v)
}

/// An NSEnumerator over a snapshot: {isa, NSArray, next index}.
fn new_enumerator(emu: &mut Emu, objs: &[u64]) -> Result<u64> {
    let cls = host_class(emu, "__NSFastEnumerationEnumerator")?;
    let e = objc::create_instance(emu, cls, 0)?;
    let a = array_from(emu, objs)?;
    emu.write_u64(e + 8, a)?;
    emu.write_u64(e + 16, 0)?;
    Ok(e)
}

fn dict_key_enumerator(emu: &mut Emu) -> Result<()> {
    let keys: Vec<u64> = dict_pairs(emu, emu.arg(0))?.into_iter().map(|p| p.0).collect();
    let e = new_enumerator(emu, &keys)?;
    emu.ret(e);
    Ok(())
}

fn dict_object_enumerator(emu: &mut Emu) -> Result<()> {
    let vals: Vec<u64> = dict_pairs(emu, emu.arg(0))?.into_iter().map(|p| p.1).collect();
    let e = new_enumerator(emu, &vals)?;
    emu.ret(e);
    Ok(())
}

fn set_object_enumerator(emu: &mut Emu) -> Result<()> {
    let v = set_values(emu, emu.arg(0))?;
    let e = new_enumerator(emu, &v)?;
    emu.ret(e);
    Ok(())
}

fn array_object_enumerator(emu: &mut Emu) -> Result<()> {
    let v = array_values(emu, emu.arg(0))?;
    let e = new_enumerator(emu, &v)?;
    emu.ret(e);
    Ok(())
}

fn array_reverse_enumerator(emu: &mut Emu) -> Result<()> {
    let mut v = array_values(emu, emu.arg(0))?;
    v.reverse();
    let e = new_enumerator(emu, &v)?;
    emu.ret(e);
    Ok(())
}

fn enumerator_remaining(emu: &mut Emu, e: u64) -> Result<Vec<u64>> {
    let a = emu.read_u64(e + 8)?;
    let i = emu.read_u64(e + 16)? as usize;
    let v = array_values(emu, a)?;
    Ok(v[i.min(v.len())..].to_vec())
}

fn enumerator_next(emu: &mut Emu) -> Result<()> {
    let e = emu.arg(0);
    let a = emu.read_u64(e + 8)?;
    let i = emu.read_u64(e + 16)?;
    let n = cf(emu, "_CFArrayGetCount", &[a])?;
    let v = if i < n {
        emu.write_u64(e + 16, i + 1)?;
        cf(emu, "_CFArrayGetValueAtIndex", &[a, i])?
    } else {
        0
    };
    emu.ret(v);
    Ok(())
}

fn enumerator_all_objects(emu: &mut Emu) -> Result<()> {
    let e = emu.arg(0);
    let rest = enumerator_remaining(emu, e)?;
    let n = cf(emu, "_CFArrayGetCount", &[emu.read_u64(e + 8)?])?;
    emu.write_u64(e + 16, n)?;
    let a = array_from(emu, &rest)?;
    emu.ret(a);
    Ok(())
}

fn enumerator_fast_enumerate(emu: &mut Emu) -> Result<()> {
    let e = emu.arg(0);
    fast_enumerate(emu, |emu| enumerator_remaining(emu, e))
}

/// `-[NSNumber objCType]`: "c" for booleans, "d" for floats, "q" otherwise.
fn number_objc_type(emu: &mut Emu) -> Result<()> {
    let n = emu.arg(0);
    let t = if is_boolean(emu, n) {
        "c"
    } else if number_is_float(emu, n)? {
        "d"
    } else {
        "q"
    };
    let p = emu.alloc_cstr(t)?;
    emu.ret(p);
    Ok(())
}

// ---- NSURL (CFURL) ----

const POSIX_PATH_STYLE: u64 = 0; // kCFURLPOSIXPathStyle

/// A file URL for a guest path.
pub fn new_file_url(emu: &mut Emu, path: &str, is_dir: bool) -> Result<u64> {
    let p = new_nsstring(emu, path)?;
    cf(emu, "_CFURLCreateWithFileSystemPath", &[0, p, POSIX_PATH_STYLE, is_dir as u64])
}

/// A CF string with the same contents: CF can't read Objective-C strings
/// (e.g. Swift-native ones) because its Objective-C dispatch is compiled out.
pub fn as_cf_string(emu: &mut Emu, s: u64) -> Result<u64> {
    if s == 0 || is_cf(emu, s) {
        return Ok(s);
    }
    let text = nsstring(emu, s)?;
    new_nsstring(emu, &text)
}

fn url_with_string(emu: &mut Emu) -> Result<()> {
    let s = as_cf_string(emu, emu.arg(2))?;
    let u = if s == 0 { 0 } else { cf(emu, "_CFURLCreateWithString", &[0, s, 0])? };
    emu.ret(u);
    Ok(())
}

fn url_with_string_relative(emu: &mut Emu) -> Result<()> {
    let base = emu.arg(3);
    let s = as_cf_string(emu, emu.arg(2))?;
    let u = if s == 0 { 0 } else { cf(emu, "_CFURLCreateWithString", &[0, s, base])? };
    emu.ret(u);
    Ok(())
}

fn file_url_with_path(emu: &mut Emu) -> Result<()> {
    let path = nsstring(emu, emu.arg(2))?;
    let dir = path.ends_with('/');
    let u = new_file_url(emu, &path, dir)?;
    emu.ret(u);
    Ok(())
}

fn file_url_with_path_is_dir(emu: &mut Emu) -> Result<()> {
    let path = nsstring(emu, emu.arg(2))?;
    let u = new_file_url(emu, &path, emu.arg(3) & 0xff != 0)?;
    emu.ret(u);
    Ok(())
}

fn url_absolute_string(emu: &mut Emu) -> Result<()> {
    let u = emu.arg(0);
    let abs = cf(emu, "_CFURLCopyAbsoluteURL", &[u])?;
    let s = cf(emu, "_CFURLGetString", &[abs])?;
    emu.ret(s);
    Ok(())
}

fn url_path(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFURLCopyFileSystemPath", &[emu.arg(0), POSIX_PATH_STYLE])?;
    emu.ret(s);
    Ok(())
}

fn url_scheme(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFURLCopyScheme", &[emu.arg(0)])?;
    emu.ret(s);
    Ok(())
}

fn url_host(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFURLCopyHostName", &[emu.arg(0)])?;
    emu.ret(s);
    Ok(())
}

fn url_last_path_component(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFURLCopyLastPathComponent", &[emu.arg(0)])?;
    emu.ret(s);
    Ok(())
}

fn url_path_extension(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFURLCopyPathExtension", &[emu.arg(0)])?;
    let s = if s == 0 { new_nsstring(emu, "")? } else { s };
    emu.ret(s);
    Ok(())
}

fn url_is_file(emu: &mut Emu) -> Result<()> {
    let scheme = cf(emu, "_CFURLCopyScheme", &[emu.arg(0)])?;
    let is_file = scheme != 0 && nsstring(emu, scheme)? == "file";
    ret_bool(emu, is_file)
}

fn url_appending_component(emu: &mut Emu) -> Result<()> {
    let c = as_cf_string(emu, emu.arg(2))?;
    let u = cf(emu, "_CFURLCreateCopyAppendingPathComponent", &[0, emu.arg(0), c, 0])?;
    emu.ret(u);
    Ok(())
}

fn url_deleting_last_component(emu: &mut Emu) -> Result<()> {
    let u = cf(emu, "_CFURLCreateCopyDeletingLastPathComponent", &[0, emu.arg(0)])?;
    emu.ret(u);
    Ok(())
}

fn url_is_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    let r = b != 0 && is_cf(emu, b) && cf(emu, "_CFEqual", &[a, b])? & 0xff != 0;
    ret_bool(emu, r)
}

fn url_hash(emu: &mut Emu) -> Result<()> {
    let h = cf(emu, "_CFHash", &[emu.arg(0)])?;
    emu.ret(h);
    Ok(())
}

fn url_description(emu: &mut Emu) -> Result<()> {
    url_absolute_string(emu)
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

/// NSHashTable / NSMapTable: since pomme never frees objects, weak and strong
/// references behave the same, so these are plain CF sets and dictionaries
/// (which already answer every NSSet / NSDictionary-style method they need).
fn hash_table_new(emu: &mut Emu) -> Result<()> {
    set_empty(emu)
}

fn map_table_new(emu: &mut Emu) -> Result<()> {
    dict_empty(emu)
}

/// NSPointerFunctionsOptions personality (bits 8–11): anything but the object
/// personality (0) means the entries aren't objects: compare and hash them as
/// raw pointers, and don't retain them.
fn objects_personality(options: u64) -> bool {
    (options >> 8) & 0xf == 0
}

/// `+mapTableWithKeyOptions:valueOptions:` / `-initWithKeyOptions:valueOptions:capacity:`
fn map_table_with_options(emu: &mut Emu) -> Result<()> {
    let (keys, values) = (emu.arg(2), emu.arg(3));
    let kcb = if objects_personality(keys) { cf_sym(emu, "_kCFTypeDictionaryKeyCallBacks")? } else { 0 };
    let vcb = if objects_personality(values) { cf_sym(emu, "_kCFTypeDictionaryValueCallBacks")? } else { 0 };
    let d = cf(emu, "_CFDictionaryCreateMutable", &[0, 0, kcb, vcb])?;
    emu.ret(d);
    Ok(())
}

/// `+hashTableWithOptions:` / `-initWithOptions:capacity:`
fn hash_table_with_options(emu: &mut Emu) -> Result<()> {
    let options = emu.arg(2);
    let cb = if objects_personality(options) { cf_sym(emu, "_kCFTypeSetCallBacks")? } else { 0 };
    let s = cf(emu, "_CFSetCreateMutable", &[0, 0, cb])?;
    emu.ret(s);
    Ok(())
}

fn map_table_dictionary(emu: &mut Emu) -> Result<()> {
    dict_copy(emu)
}

// ---- NSDate (CFDate: seconds since 2001-01-01) ----

const SECONDS_1970_TO_2001: f64 = 978_307_200.0;

fn f64_arg(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn ret_double(emu: &mut Emu, v: f64) {
    emu.cpu.q[0] = v.to_bits() as u128;
}

/// Now, as CFAbsoluteTime.
fn absolute_now(emu: &Emu) -> f64 {
    crate::threads::now_ns(emu) as f64 / 1e9 + crate::libsystem::EPOCH_BASE_SECS as f64 - SECONDS_1970_TO_2001
}

pub fn new_date(emu: &mut Emu, abs: f64) -> Result<u64> {
    emu.cpu.q[0] = abs.to_bits() as u128;
    cf(emu, "_CFDateCreate", &[0])
}

pub(crate) fn date_value(emu: &mut Emu, d: u64) -> Result<f64> {
    cf(emu, "_CFDateGetAbsoluteTime", &[d])?;
    Ok(f64::from_bits(emu.last_q0 as u64))
}

fn ret_date(emu: &mut Emu, abs: f64) -> Result<()> {
    let d = new_date(emu, abs)?;
    emu.ret(d);
    Ok(())
}

fn date_now(emu: &mut Emu) -> Result<()> {
    let t = absolute_now(emu);
    ret_date(emu, t)
}

fn date_since_now(emu: &mut Emu) -> Result<()> {
    let t = absolute_now(emu) + f64_arg(emu, 0);
    ret_date(emu, t)
}

fn date_since_1970(emu: &mut Emu) -> Result<()> {
    let t = f64_arg(emu, 0) - SECONDS_1970_TO_2001;
    ret_date(emu, t)
}

fn date_since_reference(emu: &mut Emu) -> Result<()> {
    let t = f64_arg(emu, 0);
    ret_date(emu, t)
}

fn date_interval_since_now(emu: &mut Emu) -> Result<()> {
    let v = date_value(emu, emu.arg(0))? - absolute_now(emu);
    ret_double(emu, v);
    Ok(())
}

fn date_interval_since_1970(emu: &mut Emu) -> Result<()> {
    let v = date_value(emu, emu.arg(0))? + SECONDS_1970_TO_2001;
    ret_double(emu, v);
    Ok(())
}

fn date_interval_since_reference(emu: &mut Emu) -> Result<()> {
    let v = date_value(emu, emu.arg(0))?;
    ret_double(emu, v);
    Ok(())
}

fn date_interval_since_date(emu: &mut Emu) -> Result<()> {
    let a = date_value(emu, emu.arg(0))?;
    let b = date_value(emu, emu.arg(2))?;
    ret_double(emu, a - b);
    Ok(())
}

/// `-earlierDate:` / `-laterDate:` (nil counts as the other date losing).
fn date_pick(emu: &mut Emu, earlier: bool) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    if b == 0 {
        emu.ret(a);
        return Ok(());
    }
    let (va, vb) = (date_value(emu, a)?, date_value(emu, b)?);
    let pick_a = if earlier { va <= vb } else { va >= vb };
    emu.ret(if pick_a { a } else { b });
    Ok(())
}

fn date_compare(emu: &mut Emu) -> Result<()> {
    let a = date_value(emu, emu.arg(0))?;
    let b = date_value(emu, emu.arg(2))?;
    emu.ret(a.partial_cmp(&b).map_or(0, |o| o as i64) as u64);
    Ok(())
}

fn date_adding(emu: &mut Emu) -> Result<()> {
    let delta = f64_arg(emu, 0);
    let t = date_value(emu, emu.arg(0))? + delta;
    ret_date(emu, t)
}

// ---- NSRunLoop: {isa, CFRunLoopRef} ----

fn run_loop_for(emu: &mut Emu, cf_rl: u64) -> Result<u64> {
    if let Some(&o) = emu.fnd.run_loops.get(&cf_rl) {
        return Ok(o);
    }
    let cls = host_class(emu, "NSRunLoop")?;
    let o = objc::create_instance(emu, cls, 0)?;
    emu.write_u64(o + 8, cf_rl)?;
    emu.fnd.run_loops.insert(cf_rl, o);
    Ok(o)
}

fn run_loop_current(emu: &mut Emu) -> Result<()> {
    let rl = cf(emu, "_CFRunLoopGetCurrent", &[])?;
    let o = run_loop_for(emu, rl)?;
    emu.ret(o);
    Ok(())
}

fn run_loop_main(emu: &mut Emu) -> Result<()> {
    let rl = cf(emu, "_CFRunLoopGetMain", &[])?;
    let o = run_loop_for(emu, rl)?;
    emu.ret(o);
    Ok(())
}

fn run_loop_get_cf(emu: &mut Emu) -> Result<()> {
    let rl = emu.read_u64(emu.arg(0) + 8)?;
    emu.ret(rl);
    Ok(())
}

/// `-runMode:beforeDate:` → YES if the run loop ran (it had sources).
fn run_loop_run_mode(emu: &mut Emu) -> Result<()> {
    let (mode, date) = (emu.arg(2), emu.arg(3));
    let secs = if date == 0 { 0.0 } else { (date_value(emu, date)? - absolute_now(emu)).max(0.0) };
    emu.cpu.q[0] = secs.to_bits() as u128;
    let r = cf(emu, "_CFRunLoopRunInMode", &[mode, 1])?;
    ret_bool(emu, r != 1) // kCFRunLoopRunFinished → NO
}

fn run_loop_run(emu: &mut Emu) -> Result<()> {
    let mode = cf_data(emu, "_kCFRunLoopDefaultMode")?;
    loop {
        emu.cpu.q[0] = 1.0e10f64.to_bits() as u128;
        if cf(emu, "_CFRunLoopRunInMode", &[mode, 0])? == 1 {
            return Ok(());
        }
    }
}

fn run_loop_run_until(emu: &mut Emu) -> Result<()> {
    let date = emu.arg(2);
    let mode = cf_data(emu, "_kCFRunLoopDefaultMode")?;
    let secs = (date_value(emu, date)? - absolute_now(emu)).max(0.0);
    emu.cpu.q[0] = secs.to_bits() as u128;
    cf(emu, "_CFRunLoopRunInMode", &[mode, 0])?;
    Ok(())
}

/// `-addTimer:forMode:` — an NSTimer is a CFRunLoopTimer.
fn run_loop_add_timer(emu: &mut Emu) -> Result<()> {
    let (o, timer, mode) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let rl = emu.read_u64(o + 8)?;
    cf(emu, "_CFRunLoopAddTimer", &[rl, timer, mode])?;
    Ok(())
}

/// `-addPort:forMode:` — keeps a run loop alive; a port with a source (NSMachPort) isn't modeled
/// yet, so add an empty, never-signaled source instead.
fn run_loop_add_port(emu: &mut Emu) -> Result<()> {
    let (o, mode) = (emu.arg(0), emu.arg(3));
    let rl = emu.read_u64(o + 8)?;
    // CFRunLoopSourceContext (version 0): all callbacks NULL except perform, which must exist.
    let ctx = emu.alloc(80)?;
    let perform = emu.host_thunk(noop)?;
    emu.write_u64(ctx + 72, perform)?;
    let src = cf(emu, "_CFRunLoopSourceCreate", &[0, 0, ctx])?;
    cf(emu, "_CFRunLoopAddSource", &[rl, src, mode])?;
    Ok(())
}

fn run_loop_perform_block(emu: &mut Emu) -> Result<()> {
    let (o, block) = (emu.arg(0), emu.arg(2));
    let rl = emu.read_u64(o + 8)?;
    let modes = cf_data(emu, "_kCFRunLoopCommonModes")?;
    cf(emu, "_CFRunLoopPerformBlock", &[rl, modes, block])?;
    cf(emu, "_CFRunLoopWakeUp", &[rl])?;
    Ok(())
}

fn run_loop_current_mode(emu: &mut Emu) -> Result<()> {
    let rl = emu.read_u64(emu.arg(0) + 8)?;
    let m = cf(emu, "_CFRunLoopCopyCurrentMode", &[rl])?;
    emu.ret(m);
    Ok(())
}

// ---- NSNotificationCenter / NSNotification {isa, name, object, userInfo} ----

#[derive(Clone)]
struct Observer {
    /// Target and selector, or 0 and a block.
    target: u64,
    sel: u64,
    block: u64,
    name: Option<String>,
    object: u64,
}

fn default_center(emu: &mut Emu) -> Result<()> {
    if emu.fnd.center == 0 {
        let cls = host_class(emu, "NSNotificationCenter")?;
        emu.fnd.center = objc::create_instance(emu, cls, 0)?;
    }
    emu.ret(emu.fnd.center);
    Ok(())
}

fn opt_name(emu: &mut Emu, n: u64) -> Result<Option<String>> {
    if n == 0 { Ok(None) } else { nsstring(emu, n).map(Some) }
}

/// `-addObserver:selector:name:object:`
fn add_observer(emu: &mut Emu) -> Result<()> {
    let (target, sel, name, object) = (emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    let name = opt_name(emu, name)?;
    emu.fnd.observers.push(Observer { target, sel, block: 0, name, object });
    Ok(())
}

/// `-addObserverForName:object:queue:usingBlock:` → an opaque token
fn add_observer_block(emu: &mut Emu) -> Result<()> {
    let (name, object, block) = (emu.arg(2), emu.arg(3), emu.arg(5));
    let name = opt_name(emu, name)?;
    emu.cpu.x[0] = block;
    crate::libc::block_copy(emu)?;
    let block = emu.cpu.x[0];
    let cls = host_class(emu, "NSObject")?;
    let token = objc::create_instance(emu, cls, 0)?;
    emu.fnd.observers.push(Observer { target: token, sel: 0, block, name, object });
    emu.ret(token);
    Ok(())
}

fn remove_observer(emu: &mut Emu) -> Result<()> {
    let target = emu.arg(2);
    emu.fnd.observers.retain(|o| o.target != target);
    Ok(())
}

/// `-removeObserver:name:object:`
fn remove_observer_name(emu: &mut Emu) -> Result<()> {
    let (target, name, object) = (emu.arg(2), emu.arg(3), emu.arg(4));
    let name = opt_name(emu, name)?;
    emu.fnd.observers.retain(|o| !(o.target == target && (name.is_none() || o.name == name) && (object == 0 || o.object == object)));
    Ok(())
}

fn new_notification(emu: &mut Emu, name: u64, object: u64, info: u64) -> Result<u64> {
    let cls = host_class(emu, "NSNotification")?;
    let n = objc::create_instance(emu, cls, 0)?;
    emu.write_u64(n + 8, name)?;
    emu.write_u64(n + 16, object)?;
    emu.write_u64(n + 24, info)?;
    Ok(n)
}

/// Deliver synchronously to every matching observer, in registration order.
fn deliver(emu: &mut Emu, note: u64) -> Result<()> {
    let name = emu.read_u64(note + 8)?;
    let object = emu.read_u64(note + 16)?;
    let name = opt_name(emu, name)?;
    let matching: Vec<Observer> =
        emu.fnd.observers.iter().filter(|o| (o.name.is_none() || o.name == name) && (o.object == 0 || o.object == object)).cloned().collect();
    if emu.trace {
        println!("{:>10}   notification {} → {} observer(s)", emu.cpu.steps, name.as_deref().unwrap_or("<nil>"), matching.len());
    }
    for o in matching {
        if o.block != 0 {
            call_block(emu, o.block, &[note])?;
        } else {
            let sel = objc::sel_name(emu, o.sel);
            send(emu, o.target, &sel, &[note])?;
        }
    }
    Ok(())
}

fn post_notification(emu: &mut Emu) -> Result<()> {
    deliver(emu, emu.arg(2))
}

/// `-postNotificationName:object:` / `-postNotificationName:object:userInfo:`
fn post_notification_name(emu: &mut Emu) -> Result<()> {
    let (name, object) = (emu.arg(2), emu.arg(3));
    let info = if objc::sel_name(emu, emu.arg(1)).ends_with("userInfo:") { emu.arg(4) } else { 0 };
    let n = new_notification(emu, name, object, info)?;
    deliver(emu, n)
}

fn notification_with_name(emu: &mut Emu) -> Result<()> {
    let info = if objc::sel_name(emu, emu.arg(1)).ends_with("userInfo:") { emu.arg(4) } else { 0 };
    let n = new_notification(emu, emu.arg(2), emu.arg(3), info)?;
    emu.ret(n);
    Ok(())
}

fn notification_field(emu: &mut Emu, offset: u64) -> Result<()> {
    let v = emu.read_u64(emu.arg(0) + offset)?;
    emu.ret(v);
    Ok(())
}

// ---- NSString path helpers (POSIX paths) ----

fn string_map(emu: &mut Emu, f: impl Fn(&str) -> String) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    ret_string(emu, &f(&s))
}

fn last_component(p: &str) -> &str {
    let t = p.trim_end_matches('/');
    if t.is_empty() && p.starts_with('/') { "/" } else { t.rsplit('/').next().unwrap_or("") }
}

fn path_extension(p: &str) -> String {
    let last = last_component(p);
    match last.rfind('.') {
        Some(i) if i > 0 => last[i + 1..].to_string(),
        _ => String::new(),
    }
}

fn deleting_last_component(p: &str) -> String {
    let t = p.trim_end_matches('/');
    match t.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => t[..i].to_string(),
        None => String::new(),
    }
}

fn deleting_extension(p: &str) -> String {
    let ext = path_extension(p);
    let t = p.trim_end_matches('/');
    if ext.is_empty() { t.to_string() } else { t[..t.len() - ext.len() - 1].to_string() }
}

fn appending_component(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    let joined = if a.is_empty() {
        b
    } else if b.is_empty() {
        a
    } else {
        format!("{}/{}", a.trim_end_matches('/'), b.trim_start_matches('/'))
    };
    ret_string(emu, &joined)
}

fn appending_extension(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let e = nsstring(emu, emu.arg(2))?;
    ret_string(emu, &format!("{}.{e}", a.trim_end_matches('/')))
}

fn path_components(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let mut parts: Vec<String> = Vec::new();
    if s.starts_with('/') {
        parts.push("/".into());
    }
    parts.extend(s.split('/').filter(|c| !c.is_empty()).map(str::to_string));
    let mut objs = Vec::new();
    for p in parts {
        objs.push(new_nsstring(emu, &p)?);
    }
    let a = array_from(emu, &objs)?;
    emu.ret(a);
    Ok(())
}

/// `-stringByReplacingCharactersInRange:(NSRange)range withString:` — range in x2/x3 (UTF-16), string x4.
fn replacing_range(emu: &mut Emu) -> Result<()> {
    let (s, loc, len, with) = (emu.arg(0), emu.arg(2) as usize, emu.arg(3) as usize, emu.arg(4));
    let text: Vec<u16> = nsstring(emu, s)?.encode_utf16().collect();
    let with: Vec<u16> = nsstring(emu, with)?.encode_utf16().collect();
    let (a, b) = (loc.min(text.len()), (loc + len).min(text.len()));
    let mut out = text[..a].to_vec();
    out.extend_from_slice(&with);
    out.extend_from_slice(&text[b..]);
    ret_string(emu, &String::from_utf16_lossy(&out))
}

fn replacing_occurrences(emu: &mut Emu) -> Result<()> {
    let s = nsstring(emu, emu.arg(0))?;
    let from = nsstring(emu, emu.arg(2))?;
    let to = nsstring(emu, emu.arg(3))?;
    let r = if from.is_empty() { s } else { s.replace(&from, &to) };
    ret_string(emu, &r)
}

// ---- NSData (CFData) ----

pub fn new_data(emu: &mut Emu, bytes: &[u8]) -> Result<u64> {
    let buf = emu.alloc(bytes.len() as u64 + 1)?;
    emu.p.memory.write(buf, bytes)?;
    let d = cf(emu, "_CFDataCreate", &[0, buf, bytes.len() as u64])?;
    emu.heap.free(&emu.p.memory, buf)?;
    Ok(d)
}

fn new_mutable_data(emu: &mut Emu, bytes: &[u8]) -> Result<u64> {
    let d = cf(emu, "_CFDataCreateMutable", &[0, 0])?;
    if !bytes.is_empty() {
        let buf = emu.alloc(bytes.len() as u64)?;
        emu.p.memory.write(buf, bytes)?;
        cf(emu, "_CFDataAppendBytes", &[d, buf, bytes.len() as u64])?;
    }
    Ok(d)
}

pub fn data_bytes(emu: &mut Emu, d: u64) -> Result<Vec<u8>> {
    if d == 0 {
        return Ok(Vec::new());
    }
    if !is_cf(emu, d) {
        let p = send(emu, d, "bytes", &[])?;
        let n = send(emu, d, "length", &[])?;
        return Ok(emu.p.memory.read(p, n as usize)?.to_vec());
    }
    let p = cf(emu, "_CFDataGetBytePtr", &[d])?;
    let n = cf(emu, "_CFDataGetLength", &[d])?;
    Ok(if n == 0 { Vec::new() } else { emu.p.memory.read(p, n as usize)?.to_vec() })
}

fn ret_data(emu: &mut Emu, bytes: &[u8]) -> Result<()> {
    let mutable = wants_mutable(emu, emu.arg(0), "NSMutableData")?;
    let d = if mutable { new_mutable_data(emu, bytes)? } else { new_data(emu, bytes)? };
    emu.ret(d);
    Ok(())
}

fn data_with_bytes(emu: &mut Emu) -> Result<()> {
    let (p, n) = (emu.arg(2), emu.arg(3));
    let bytes = if n == 0 { Vec::new() } else { emu.p.memory.read(p, n as usize)?.to_vec() };
    ret_data(emu, &bytes)
}

fn data_empty(emu: &mut Emu) -> Result<()> {
    ret_data(emu, &[])
}

fn data_with_length(emu: &mut Emu) -> Result<()> {
    let n = emu.arg(2) as usize;
    ret_data(emu, &vec![0; n])
}

/// `+dataWithContentsOfFile:` (and `options:error:`) — nil if missing.
fn data_with_contents_of_file(emu: &mut Emu) -> Result<()> {
    let path = nsstring(emu, emu.arg(2))?;
    match crate::fs::read_file(emu, &path) {
        Some(bytes) => ret_data(emu, &bytes),
        None => {
            emu.ret(0);
            Ok(())
        }
    }
}

fn data_with_contents_of_url(emu: &mut Emu) -> Result<()> {
    let url = emu.arg(2);
    let path = cf(emu, "_CFURLCopyFileSystemPath", &[url, POSIX_PATH_STYLE])?;
    let path = nsstring(emu, path)?;
    match crate::fs::read_file(emu, &path) {
        Some(bytes) => ret_data(emu, &bytes),
        None => {
            emu.ret(0);
            Ok(())
        }
    }
}

fn data_length(emu: &mut Emu) -> Result<()> {
    let n = cf(emu, "_CFDataGetLength", &[emu.arg(0)])?;
    emu.ret(n);
    Ok(())
}

fn data_bytes_ptr(emu: &mut Emu) -> Result<()> {
    let p = cf(emu, "_CFDataGetBytePtr", &[emu.arg(0)])?;
    emu.ret(p);
    Ok(())
}

fn data_mutable_bytes(emu: &mut Emu) -> Result<()> {
    let p = cf(emu, "_CFDataGetMutableBytePtr", &[emu.arg(0)])?;
    emu.ret(p);
    Ok(())
}

/// `-getBytes:length:`
fn data_get_bytes(emu: &mut Emu) -> Result<()> {
    let (d, buf, n) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let bytes = data_bytes(emu, d)?;
    let n = (n as usize).min(bytes.len());
    emu.p.memory.write(buf, &bytes[..n])?;
    Ok(())
}

/// `-subdataWithRange:` (NSRange in x2, x3)
fn data_subdata(emu: &mut Emu) -> Result<()> {
    let (d, loc, len) = (emu.arg(0), emu.arg(2) as usize, emu.arg(3) as usize);
    let bytes = data_bytes(emu, d)?;
    let end = (loc + len).min(bytes.len());
    let r = new_data(emu, &bytes[loc.min(end)..end])?;
    emu.ret(r);
    Ok(())
}

fn data_is_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    if b == 0 {
        return ret_bool(emu, false);
    }
    let x = data_bytes(emu, a)?;
    let y = data_bytes(emu, b)?;
    ret_bool(emu, x == y)
}

fn data_copy(emu: &mut Emu) -> Result<()> {
    let bytes = data_bytes(emu, emu.arg(0))?;
    let d = new_data(emu, &bytes)?;
    emu.ret(d);
    Ok(())
}

fn data_mutable_copy(emu: &mut Emu) -> Result<()> {
    let bytes = data_bytes(emu, emu.arg(0))?;
    let d = new_mutable_data(emu, &bytes)?;
    emu.ret(d);
    Ok(())
}

fn data_append_bytes(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFDataAppendBytes", &[emu.arg(0), emu.arg(2), emu.arg(3)])?;
    Ok(())
}

fn data_append_data(emu: &mut Emu) -> Result<()> {
    let (d, other) = (emu.arg(0), emu.arg(2));
    let bytes = data_bytes(emu, other)?;
    if !bytes.is_empty() {
        let buf = emu.alloc(bytes.len() as u64)?;
        emu.p.memory.write(buf, &bytes)?;
        cf(emu, "_CFDataAppendBytes", &[d, buf, bytes.len() as u64])?;
    }
    Ok(())
}

fn data_set_length(emu: &mut Emu) -> Result<()> {
    cf(emu, "_CFDataSetLength", &[emu.arg(0), emu.arg(2)])?;
    Ok(())
}

// ---- strings ↔ data ----

/// `-dataUsingEncoding:` (UTF-8 for every 8-bit encoding; UTF-16LE for NSUnicodeStringEncoding)
fn string_data_using_encoding(emu: &mut Emu) -> Result<()> {
    let (s, enc) = (emu.arg(0), emu.arg(2));
    let text = nsstring(emu, s)?;
    let bytes: Vec<u8> = if enc == 10 || enc == 0x9400_0100 { text.encode_utf16().flat_map(|u| u.to_le_bytes()).collect() } else { text.into_bytes() };
    let d = new_data(emu, &bytes)?;
    emu.ret(d);
    Ok(())
}

/// `-initWithData:encoding:`
fn string_with_data(emu: &mut Emu) -> Result<()> {
    let (d, enc) = (emu.arg(2), emu.arg(3));
    let bytes = data_bytes(emu, d)?;
    string_from_bytes(emu, &bytes, enc)
}

/// `-initWithBytes:length:encoding:`
fn string_with_bytes(emu: &mut Emu) -> Result<()> {
    let (p, n, enc) = (emu.arg(2), emu.arg(3), emu.arg(4));
    let bytes = if n == 0 { Vec::new() } else { emu.p.memory.read(p, n as usize)?.to_vec() };
    string_from_bytes(emu, &bytes, enc)
}

fn string_from_bytes(emu: &mut Emu, bytes: &[u8], enc: u64) -> Result<()> {
    let s = if enc == 10 {
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) if enc == 4 => {
                emu.ret(0); // invalid UTF-8 → nil
                return Ok(());
            }
            Err(_) => bytes.iter().map(|&b| b as char).collect(), // Latin-1
        }
    };
    ret_string(emu, &s)
}

/// `+stringWithContentsOfFile:encoding:error:`
fn string_with_contents_of_file(emu: &mut Emu) -> Result<()> {
    let path = nsstring(emu, emu.arg(2))?;
    match crate::fs::read_file(emu, &path) {
        Some(bytes) => string_from_bytes(emu, &bytes, 4),
        None => {
            emu.ret(0);
            Ok(())
        }
    }
}

// ---- NSFileManager and standard directories ----

const DATA_CONTAINER: &str = "/private/var/mobile/Containers/Data/Application/pomme";

fn search_dir(kind: u64) -> String {
    match kind {
        9 => format!("{DATA_CONTAINER}/Documents"),
        13 => format!("{DATA_CONTAINER}/Library/Caches"),
        14 => format!("{DATA_CONTAINER}/Library/Application Support"),
        _ => format!("{DATA_CONTAINER}/Library"),
    }
}

fn file_manager(emu: &mut Emu) -> Result<()> {
    if emu.fnd.file_manager == 0 {
        let cls = host_class(emu, "NSFileManager")?;
        emu.fnd.file_manager = objc::create_instance(emu, cls, 0)?;
    }
    emu.ret(emu.fnd.file_manager);
    Ok(())
}

fn file_exists(emu: &mut Emu) -> Result<()> {
    let (path, is_dir_out) = (emu.arg(2), emu.arg(3));
    let path = nsstring(emu, path)?;
    let host = crate::fs::host_path(emu, &path);
    let exists = host.as_ref().is_some_and(|p| pomme_loader::vfs::exists(p));
    if exists && is_dir_out != 0 && objc::sel_name(emu, emu.arg(1)).contains("isDirectory") {
        let dir = host.as_ref().is_some_and(|p| pomme_loader::vfs::is_dir(p));
        emu.p.memory.write_u8(is_dir_out, dir as u8)?;
    }
    ret_bool(emu, exists)
}

fn file_contents(emu: &mut Emu) -> Result<()> {
    let path = nsstring(emu, emu.arg(2))?;
    match crate::fs::read_file(emu, &path) {
        Some(bytes) => {
            let d = new_data(emu, &bytes)?;
            emu.ret(d);
        }
        None => emu.ret(0),
    }
    Ok(())
}

/// `-URLsForDirectory:inDomains:` → [file URL]
fn urls_for_directory(emu: &mut Emu) -> Result<()> {
    let dir = search_dir(emu.arg(2));
    let u = new_file_url(emu, &dir, true)?;
    let a = array_from(emu, &[u])?;
    emu.ret(a);
    Ok(())
}

/// `-URLForDirectory:inDomain:appropriateForURL:create:error:`
fn url_for_directory(emu: &mut Emu) -> Result<()> {
    let dir = search_dir(emu.arg(2));
    let u = new_file_url(emu, &dir, true)?;
    emu.ret(u);
    Ok(())
}

/// `NSSearchPathForDirectoriesInDomains(directory, domainMask, expandTilde)` → [path]
fn search_path_for_directories(emu: &mut Emu) -> Result<()> {
    let dir = search_dir(emu.arg(0));
    let s = new_nsstring(emu, &dir)?;
    let a = array_from(emu, &[s])?;
    emu.ret(a);
    Ok(())
}

fn ns_home_directory(emu: &mut Emu) -> Result<()> {
    ret_string(emu, DATA_CONTAINER)
}

fn ns_temporary_directory(emu: &mut Emu) -> Result<()> {
    ret_string(emu, &format!("{DATA_CONTAINER}/tmp/"))
}

// ---- NSCache: a dictionary that never evicts (deterministic) ----

fn cache_get(emu: &mut Emu) -> Result<()> {
    let (c, key) = (emu.arg(0), emu.arg(2));
    let store = defaults_store(emu, c)?;
    let v = if key == 0 { 0 } else { cf(emu, "_CFDictionaryGetValue", &[store, key])? };
    emu.ret(v);
    Ok(())
}

fn cache_set(emu: &mut Emu) -> Result<()> {
    let (c, v, key) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let store = defaults_store(emu, c)?;
    if key != 0 {
        if v == 0 {
            cf(emu, "_CFDictionaryRemoveValue", &[store, key])?;
        } else {
            cf(emu, "_CFDictionarySetValue", &[store, key, v])?;
        }
    }
    Ok(())
}

fn cache_remove(emu: &mut Emu) -> Result<()> {
    let (c, key) = (emu.arg(0), emu.arg(2));
    let store = defaults_store(emu, c)?;
    cf(emu, "_CFDictionaryRemoveValue", &[store, key])?;
    Ok(())
}

fn cache_remove_all(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0);
    let store = defaults_store(emu, c)?;
    cf(emu, "_CFDictionaryRemoveAllValues", &[store])?;
    Ok(())
}

// ---- NSUserDefaults: in memory, per run (nothing persists yet) ----

fn user_defaults(emu: &mut Emu) -> Result<()> {
    if emu.fnd.defaults == 0 {
        let cls = host_class(emu, "NSUserDefaults")?;
        let ud = objc::create_instance(emu, cls, 0)?;
        let store = new_dict(emu)?;
        emu.write_u64(ud + 8, store)?;
        // What a fresh English-language device reports.
        let en = new_nsstring(emu, "en")?;
        let langs = array_from(emu, &[en])?;
        let k = new_nsstring(emu, "AppleLanguages")?;
        cf(emu, "_CFDictionarySetValue", &[store, k, langs])?;
        let k = new_nsstring(emu, "AppleLocale")?;
        let v = new_nsstring(emu, "en_US")?;
        cf(emu, "_CFDictionarySetValue", &[store, k, v])?;
        emu.fnd.defaults = ud;
    }
    emu.ret(emu.fnd.defaults);
    Ok(())
}

fn defaults_store(emu: &mut Emu, ud: u64) -> Result<u64> {
    let s = emu.read_u64(ud + 8)?;
    if s != 0 {
        return Ok(s);
    }
    let s = new_dict(emu)?;
    emu.write_u64(ud + 8, s)?;
    Ok(s)
}

fn defaults_get(emu: &mut Emu) -> Result<u64> {
    let (ud, key) = (emu.arg(0), emu.arg(2));
    let store = defaults_store(emu, ud)?;
    if key == 0 { Ok(0) } else { cf(emu, "_CFDictionaryGetValue", &[store, key]) }
}

fn defaults_set(emu: &mut Emu, value: u64) -> Result<()> {
    let (ud, key) = (emu.arg(0), emu.arg(3));
    let store = defaults_store(emu, ud)?;
    if value == 0 {
        cf(emu, "_CFDictionaryRemoveValue", &[store, key])?;
    } else {
        cf(emu, "_CFDictionarySetValue", &[store, key, value])?;
    }
    Ok(())
}

fn defaults_object(emu: &mut Emu) -> Result<()> {
    let v = defaults_get(emu)?;
    emu.ret(v);
    Ok(())
}

fn defaults_set_object(emu: &mut Emu) -> Result<()> {
    defaults_set(emu, emu.arg(2))
}

fn defaults_remove(emu: &mut Emu) -> Result<()> {
    let (ud, key) = (emu.arg(0), emu.arg(2));
    let store = defaults_store(emu, ud)?;
    cf(emu, "_CFDictionaryRemoveValue", &[store, key])?;
    Ok(())
}

fn defaults_bool(emu: &mut Emu) -> Result<()> {
    let v = defaults_get(emu)?;
    let b = v != 0 && send(emu, v, "boolValue", &[])? & 0xff != 0;
    ret_bool(emu, b)
}

fn defaults_integer(emu: &mut Emu) -> Result<()> {
    let v = defaults_get(emu)?;
    let i = if v == 0 { 0 } else { send(emu, v, "integerValue", &[])? };
    emu.ret(i);
    Ok(())
}

fn defaults_double(emu: &mut Emu) -> Result<()> {
    let v = defaults_get(emu)?;
    let d = if v == 0 {
        0.0
    } else {
        send(emu, v, "doubleValue", &[])?;
        f64::from_bits(emu.last_q0 as u64)
    };
    emu.cpu.q[0] = d.to_bits() as u128;
    Ok(())
}

fn defaults_set_bool(emu: &mut Emu) -> Result<()> {
    let v = boolean(emu, emu.arg(2) & 0xff != 0);
    defaults_set(emu, v)
}

fn defaults_set_integer(emu: &mut Emu) -> Result<()> {
    let v = new_number_i64(emu, emu.arg(2) as i64)?;
    defaults_set(emu, v)
}

fn defaults_set_double(emu: &mut Emu) -> Result<()> {
    let d = f64::from_bits(emu.cpu.q[0] as u64);
    // setDouble:forKey: has the key in x2 (the double is in d0).
    let (ud, key) = (emu.arg(0), emu.arg(2));
    let v = new_number_f64(emu, d)?;
    let store = defaults_store(emu, ud)?;
    cf(emu, "_CFDictionarySetValue", &[store, key, v])?;
    Ok(())
}

/// `-registerDefaults:` — only fills keys that aren't set.
fn defaults_register(emu: &mut Emu) -> Result<()> {
    let (ud, d) = (emu.arg(0), emu.arg(2));
    let store = defaults_store(emu, ud)?;
    for (k, v) in dict_pairs(emu, d)? {
        cf(emu, "_CFDictionaryAddValue", &[store, k, v])?;
    }
    Ok(())
}

fn defaults_dictionary(emu: &mut Emu) -> Result<()> {
    let store = defaults_store(emu, emu.arg(0))?;
    dict_with_dict_of(emu, store)
}

fn defaults_init_suite(emu: &mut Emu) -> Result<()> {
    let ud = emu.arg(0);
    defaults_store(emu, ud)?;
    emu.ret(ud);
    Ok(())
}

pub(crate) const PATH_FUNCS: &[(&str, HostFn)] = &[
    ("_NSSearchPathForDirectoriesInDomains", search_path_for_directories),
    ("_NSHomeDirectory", ns_home_directory),
    ("_NSTemporaryDirectory", ns_temporary_directory),
];

// ---- NSError {isa, domain, code, userInfo} ----

fn error_new(emu: &mut Emu) -> Result<()> {
    let (domain, code, info) = (emu.arg(2), emu.arg(3), emu.arg(4));
    let cls = if emu.objc.class(emu.arg(0)).is_some() { emu.arg(0) } else { host_class(emu, "NSError")? };
    let e = objc::create_instance(emu, cls, 0)?;
    emu.write_u64(e + 8, domain)?;
    emu.write_u64(e + 16, code)?;
    emu.write_u64(e + 24, info)?;
    // Errors made at startup usually explain what's about to go wrong: show them.
    let d = nsstring(emu, domain)?;
    let desc = if info == 0 {
        String::new()
    } else {
        let s = send(emu, info, "description", &[])?;
        nsstring(emu, s)?.replace('\n', " ")
    };
    println!("[NSError] {d} {} {desc}", code as i64);
    emu.ret(e);
    Ok(())
}

fn error_field(emu: &mut Emu, off: u64) -> Result<()> {
    let v = emu.read_u64(emu.arg(0) + off)?;
    emu.ret(v);
    Ok(())
}

fn error_localized_description(emu: &mut Emu) -> Result<()> {
    let e = emu.arg(0);
    let info = emu.read_u64(e + 24)?;
    if info != 0 {
        let key = new_nsstring(emu, "NSLocalizedDescription")?;
        let v = dict_get_any(emu, info, key)?;
        if v != 0 {
            emu.ret(v);
            return Ok(());
        }
    }
    let domain = nsstring(emu, emu.read_u64(e + 8)?)?;
    let code = emu.read_u64(e + 16)? as i64;
    ret_string(emu, &format!("The operation couldn’t be completed. ({domain} error {code}.)"))
}

/// objectForKey: on any NSDictionary (CF, constant or guest).
fn dict_get_any(emu: &mut Emu, d: u64, key: u64) -> Result<u64> {
    if is_cf(emu, d) {
        cf(emu, "_CFDictionaryGetValue", &[d, key])
    } else {
        send(emu, d, "objectForKey:", &[key])
    }
}

// ---- NSProcessInfo: an iPhone on iOS 26.0 ----

/// The presented iOS version as NSOperatingSystemVersion's fields.
fn os_version_triple(emu: &Emu) -> (u64, u64, u64) {
    let v = emu.system.os;
    (v.major() as u64, v.minor() as u64, v.patch() as u64)
}

fn process_info(emu: &mut Emu) -> Result<()> {
    if emu.fnd.process_info == 0 {
        let cls = host_class(emu, "NSProcessInfo")?;
        emu.fnd.process_info = objc::create_instance(emu, cls, 0)?;
    }
    emu.ret(emu.fnd.process_info);
    Ok(())
}

/// `-operatingSystemVersion` → NSOperatingSystemVersion {major, minor, patch}: 24 bytes, returned through x8.
fn os_version(emu: &mut Emu) -> Result<()> {
    let out = emu.cpu.x[8];
    let v = os_version_triple(emu);
    emu.write_u64(out, v.0)?;
    emu.write_u64(out + 8, v.1)?;
    emu.write_u64(out + 16, v.2)?;
    Ok(())
}

/// `-isOperatingSystemAtLeastVersion:` — the 24-byte struct argument arrives by reference (x2).
fn os_at_least(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(2);
    let want = (emu.read_u64(p)?, emu.read_u64(p + 8)?, emu.read_u64(p + 16)?);
    ret_bool(emu, os_version_triple(emu) >= want)
}

fn process_arguments(emu: &mut Emu) -> Result<()> {
    let exe = format!("{}/{}", crate::fs::guest_bundle_path(emu), emu.p.main().name);
    let s = new_nsstring(emu, &exe)?;
    let a = array_from(emu, &[s])?;
    emu.ret(a);
    Ok(())
}

fn process_environment(emu: &mut Emu) -> Result<()> {
    let d = new_dict(emu)?;
    emu.ret(d);
    Ok(())
}

fn system_uptime(emu: &mut Emu) -> Result<()> {
    let t = crate::threads::now_ns(emu) as f64 / 1e9;
    ret_double(emu, t);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSProcessInfo", true, "processInfo", process_info),
    ("NSProcessInfo", false, "operatingSystemVersion", os_version),
    ("NSProcessInfo", false, "isOperatingSystemAtLeastVersion:", os_at_least),
    ("NSProcessInfo", false, "operatingSystemVersionString", |e| {
        let v = format!("Version {} (Build pomme)", e.system.os.short());
        ret_string(e, &v)
    }),
    ("NSProcessInfo", false, "arguments", process_arguments),
    ("NSProcessInfo", false, "environment", process_environment),
    ("NSProcessInfo", false, "processName", |e| {
        let n = e.p.main().name.clone();
        ret_string(e, &n)
    }),
    ("NSProcessInfo", false, "processIdentifier", |e| {
        e.ret(1000);
        Ok(())
    }),
    ("NSProcessInfo", false, "globallyUniqueString", |e| ret_string(e, "00000000-0000-0000-0000-000000000000-1000-0000000000000000")),
    ("NSProcessInfo", false, "hostName", |e| ret_string(e, "pomme")),
    ("NSProcessInfo", false, "systemUptime", system_uptime),
    ("NSProcessInfo", false, "physicalMemory", |e| {
        e.ret(6 << 30);
        Ok(())
    }),
    ("NSProcessInfo", false, "processorCount", |e| {
        e.ret(1);
        Ok(())
    }),
    ("NSProcessInfo", false, "activeProcessorCount", |e| {
        e.ret(1);
        Ok(())
    }),
    ("NSProcessInfo", false, "thermalState", |e| {
        e.ret(0);
        Ok(())
    }),
    ("NSProcessInfo", false, "isLowPowerModeEnabled", |e| ret_bool(e, false)),
    ("NSProcessInfo", false, "isiOSAppOnMac", |e| ret_bool(e, false)),
    ("NSProcessInfo", false, "isMacCatalystApp", |e| ret_bool(e, false)),
    ("NSProcessInfo", false, "beginActivityWithOptions:reason:", |e| {
        let cls = host_class(e, "NSObject")?;
        let token = objc::create_instance(e, cls, 0)?;
        e.ret(token);
        Ok(())
    }),
    ("NSProcessInfo", false, "endActivity:", |_| Ok(())),
    ("NSError", true, "errorWithDomain:code:userInfo:", error_new),
    ("NSError", false, "initWithDomain:code:userInfo:", error_new),
    ("NSError", false, "domain", |e| error_field(e, 8)),
    ("NSError", false, "code", |e| error_field(e, 16)),
    ("NSError", false, "userInfo", |e| error_field(e, 24)),
    ("NSError", false, "localizedDescription", error_localized_description),
    ("NSError", false, "description", error_localized_description),
    ("NSError", false, "localizedFailureReason", |e| {
        e.ret(0);
        Ok(())
    }),
    ("NSConstantArray", false, "count", constant_array_count),
    ("NSConstantArray", false, "objectAtIndex:", constant_array_object_at),
    ("NSConstantArray", false, "objectAtIndexedSubscript:", constant_array_object_at),
    ("NSConstantArray", false, "firstObject", constant_array_first),
    ("NSConstantArray", false, "lastObject", constant_array_last),
    ("NSConstantDictionary", false, "count", constant_dict_count),
    ("NSConstantDictionary", false, "objectForKey:", constant_dict_object_for_key),
    ("NSConstantDictionary", false, "objectForKeyedSubscript:", constant_dict_object_for_key),
    ("NSCache", false, "objectForKey:", cache_get),
    ("NSCache", false, "setObject:forKey:", cache_set),
    ("NSCache", false, "setObject:forKey:cost:", cache_set),
    ("NSCache", false, "removeObjectForKey:", cache_remove),
    ("NSCache", false, "removeAllObjects", cache_remove_all),
    ("NSUserDefaults", true, "standardUserDefaults", user_defaults),
    ("NSUserDefaults", false, "initWithSuiteName:", defaults_init_suite),
    ("NSUserDefaults", false, "objectForKey:", defaults_object),
    ("NSUserDefaults", false, "stringForKey:", defaults_object),
    ("NSUserDefaults", false, "arrayForKey:", defaults_object),
    ("NSUserDefaults", false, "dictionaryForKey:", defaults_object),
    ("NSUserDefaults", false, "dataForKey:", defaults_object),
    ("NSUserDefaults", false, "stringArrayForKey:", defaults_object),
    ("NSUserDefaults", false, "URLForKey:", defaults_object),
    ("NSUserDefaults", false, "boolForKey:", defaults_bool),
    ("NSUserDefaults", false, "integerForKey:", defaults_integer),
    ("NSUserDefaults", false, "doubleForKey:", defaults_double),
    ("NSUserDefaults", false, "floatForKey:", |e| {
        defaults_double(e)?;
        let d = f64::from_bits(e.cpu.q[0] as u64);
        e.cpu.q[0] = (d as f32).to_bits() as u128;
        Ok(())
    }),
    ("NSUserDefaults", false, "setObject:forKey:", defaults_set_object),
    ("NSUserDefaults", false, "setValue:forKey:", defaults_set_object),
    ("NSUserDefaults", false, "setBool:forKey:", defaults_set_bool),
    ("NSUserDefaults", false, "setInteger:forKey:", defaults_set_integer),
    ("NSUserDefaults", false, "setDouble:forKey:", defaults_set_double),
    ("NSUserDefaults", false, "removeObjectForKey:", defaults_remove),
    ("NSUserDefaults", false, "registerDefaults:", defaults_register),
    ("NSUserDefaults", false, "dictionaryRepresentation", defaults_dictionary),
    ("NSUserDefaults", false, "synchronize", ret_yes),
    ("NSData", true, "alloc", cluster_alloc),
    ("NSData", true, "data", data_empty),
    ("NSData", true, "new", data_empty),
    ("NSData", false, "init", data_empty),
    ("NSData", true, "dataWithBytes:length:", data_with_bytes),
    ("NSData", false, "initWithBytes:length:", data_with_bytes),
    ("NSData", true, "dataWithBytesNoCopy:length:", data_with_bytes),
    ("NSData", true, "dataWithBytesNoCopy:length:freeWhenDone:", data_with_bytes),
    ("NSData", false, "initWithBytesNoCopy:length:", data_with_bytes),
    ("NSData", false, "initWithBytesNoCopy:length:freeWhenDone:", data_with_bytes),
    // We copy the bytes and never free, so the deallocator never runs (the bytes may be an owning pointer).
    ("NSData", false, "initWithBytesNoCopy:length:deallocator:", data_with_bytes),
    ("NSData", true, "dataWithBytesNoCopy:length:deallocator:", data_with_bytes),
    ("NSData", true, "dataWithContentsOfFile:", data_with_contents_of_file),
    ("NSData", true, "dataWithContentsOfFile:options:error:", data_with_contents_of_file),
    ("NSData", false, "initWithContentsOfFile:", data_with_contents_of_file),
    ("NSData", false, "initWithContentsOfFile:options:error:", data_with_contents_of_file),
    ("NSData", true, "dataWithContentsOfURL:", data_with_contents_of_url),
    ("NSData", true, "dataWithContentsOfURL:options:error:", data_with_contents_of_url),
    ("NSData", false, "initWithContentsOfURL:", data_with_contents_of_url),
    ("NSData", false, "initWithContentsOfURL:options:error:", data_with_contents_of_url),
    ("NSData", false, "length", data_length),
    ("NSData", false, "bytes", data_bytes_ptr),
    ("NSData", false, "getBytes:length:", data_get_bytes),
    ("NSData", false, "subdataWithRange:", data_subdata),
    ("NSData", false, "isEqualToData:", data_is_equal),
    ("NSData", false, "isEqual:", data_is_equal),
    ("NSData", false, "copy", data_copy),
    ("NSData", false, "copyWithZone:", data_copy),
    ("NSData", false, "mutableCopy", data_mutable_copy),
    ("NSMutableData", true, "dataWithCapacity:", data_empty),
    ("NSMutableData", false, "initWithCapacity:", data_empty),
    ("NSMutableData", true, "dataWithLength:", data_with_length),
    ("NSMutableData", false, "initWithLength:", data_with_length),
    ("NSMutableData", false, "mutableBytes", data_mutable_bytes),
    ("NSMutableData", false, "appendBytes:length:", data_append_bytes),
    ("NSMutableData", false, "appendData:", data_append_data),
    ("NSMutableData", false, "setLength:", data_set_length),
    ("NSString", false, "dataUsingEncoding:", string_data_using_encoding),
    ("NSString", false, "dataUsingEncoding:allowLossyConversion:", string_data_using_encoding),
    ("NSString", false, "initWithData:encoding:", string_with_data),
    ("NSString", false, "initWithBytes:length:encoding:", string_with_bytes),
    ("NSString", true, "stringWithContentsOfFile:encoding:error:", string_with_contents_of_file),
    ("NSString", false, "initWithContentsOfFile:encoding:error:", string_with_contents_of_file),
    ("NSFileManager", true, "defaultManager", file_manager),
    ("NSFileManager", false, "fileExistsAtPath:", file_exists),
    ("NSFileManager", false, "fileExistsAtPath:isDirectory:", file_exists),
    ("NSFileManager", false, "contentsAtPath:", file_contents),
    ("NSFileManager", false, "URLsForDirectory:inDomains:", urls_for_directory),
    ("NSFileManager", false, "URLForDirectory:inDomain:appropriateForURL:create:error:", url_for_directory),
    ("NSFileManager", false, "temporaryDirectory", |e| {
        let u = new_file_url(e, &format!("{DATA_CONTAINER}/tmp/"), true)?;
        e.ret(u);
        Ok(())
    }),
    // No writable storage yet: pretend directory creation works, refuse other writes.
    ("NSFileManager", false, "createDirectoryAtPath:withIntermediateDirectories:attributes:error:", ret_yes),
    ("NSFileManager", false, "createDirectoryAtURL:withIntermediateDirectories:attributes:error:", ret_yes),
    ("NSString", false, "pathExtension", |e| string_map(e, path_extension)),
    ("NSString", false, "lastPathComponent", |e| string_map(e, |p| last_component(p).to_string())),
    ("NSString", false, "stringByDeletingLastPathComponent", |e| string_map(e, deleting_last_component)),
    ("NSString", false, "stringByDeletingPathExtension", |e| string_map(e, deleting_extension)),
    ("NSString", false, "stringByStandardizingPath", |e| string_map(e, |p| p.to_string())),
    ("NSString", false, "stringByResolvingSymlinksInPath", |e| string_map(e, |p| p.to_string())),
    ("NSString", false, "stringByExpandingTildeInPath", |e| string_map(e, |p| p.replacen('~', "/var/mobile", 1))),
    ("NSString", false, "stringByAppendingPathComponent:", appending_component),
    ("NSString", false, "stringByAppendingPathExtension:", appending_extension),
    ("NSString", false, "pathComponents", path_components),
    ("NSString", false, "isAbsolutePath", |e| {
        let s = nsstring(e, e.arg(0))?;
        ret_bool(e, s.starts_with('/'))
    }),
    ("NSString", false, "stringByReplacingOccurrencesOfString:withString:", replacing_occurrences),
    ("NSString", false, "stringByReplacingCharactersInRange:withString:", replacing_range),
    ("NSString", false, "capitalizedString", |e| {
        let s = nsstring(e, e.arg(0))?;
        let cap: String = s
            .split(' ')
            .map(|w| {
                let mut c = w.chars();
                c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + &c.as_str().to_lowercase())
            })
            .collect::<Vec<_>>()
            .join(" ");
        ret_string(e, &cap)
    }),
    ("NSString", false, "substringWithRange:", |e| {
        let (s, loc, len) = (e.arg(0), e.arg(2) as usize, e.arg(3) as usize);
        let text = nsstring(e, s)?;
        let r = utf16_slice(&text, loc, loc + len);
        ret_string(e, &r)
    }),
    ("NSNotificationCenter", true, "defaultCenter", default_center),
    ("NSNotificationCenter", false, "addObserver:selector:name:object:", add_observer),
    ("NSNotificationCenter", false, "addObserverForName:object:queue:usingBlock:", add_observer_block),
    ("NSNotificationCenter", false, "removeObserver:", remove_observer),
    ("NSNotificationCenter", false, "removeObserver:name:object:", remove_observer_name),
    ("NSNotificationCenter", false, "postNotification:", post_notification),
    ("NSNotificationCenter", false, "postNotificationName:object:", post_notification_name),
    ("NSNotificationCenter", false, "postNotificationName:object:userInfo:", post_notification_name),
    ("NSNotification", true, "notificationWithName:object:", notification_with_name),
    ("NSNotification", true, "notificationWithName:object:userInfo:", notification_with_name),
    ("NSNotification", false, "name", |e| notification_field(e, 8)),
    ("NSNotification", false, "object", |e| notification_field(e, 16)),
    ("NSNotification", false, "userInfo", |e| notification_field(e, 24)),
    ("NSDate", true, "alloc", cluster_alloc),
    ("NSDate", true, "date", date_now),
    ("NSDate", true, "now", date_now),
    ("NSDate", false, "init", date_now),
    ("NSDate", true, "distantFuture", |e| ret_date(e, 63_113_904_000.0)),
    ("NSDate", true, "distantPast", |e| ret_date(e, -63_114_076_800.0)),
    ("NSDate", true, "dateWithTimeIntervalSinceNow:", date_since_now),
    ("NSDate", false, "initWithTimeIntervalSinceNow:", date_since_now),
    ("NSDate", true, "dateWithTimeIntervalSince1970:", date_since_1970),
    ("NSDate", false, "initWithTimeIntervalSince1970:", date_since_1970),
    ("NSDate", true, "dateWithTimeIntervalSinceReferenceDate:", date_since_reference),
    ("NSDate", false, "initWithTimeIntervalSinceReferenceDate:", date_since_reference),
    ("NSDate", true, "timeIntervalSinceReferenceDate", |e| {
        let t = absolute_now(e);
        ret_double(e, t);
        Ok(())
    }),
    ("NSDate", false, "timeIntervalSinceNow", date_interval_since_now),
    ("NSDate", false, "timeIntervalSince1970", date_interval_since_1970),
    ("NSDate", false, "timeIntervalSinceReferenceDate", date_interval_since_reference),
    ("NSDate", false, "timeIntervalSinceDate:", date_interval_since_date),
    ("NSDate", false, "compare:", date_compare),
    ("NSDate", false, "earlierDate:", |e| date_pick(e, true)),
    ("NSDate", false, "laterDate:", |e| date_pick(e, false)),
    ("NSDate", false, "isEqualToDate:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let eq = b != 0 && date_value(e, a)? == date_value(e, b)?;
        e.ret(eq as u64);
        Ok(())
    }),
    ("NSDate", false, "dateByAddingTimeInterval:", date_adding),
    ("NSDate", false, "copy", ret_self),
    ("NSDate", false, "copyWithZone:", ret_self),
    ("NSRunLoop", true, "currentRunLoop", run_loop_current),
    ("NSRunLoop", true, "mainRunLoop", run_loop_main),
    ("NSRunLoop", false, "getCFRunLoop", run_loop_get_cf),
    ("NSRunLoop", false, "runMode:beforeDate:", run_loop_run_mode),
    ("NSRunLoop", false, "run", run_loop_run),
    ("NSRunLoop", false, "runUntilDate:", run_loop_run_until),
    ("NSRunLoop", false, "addTimer:forMode:", run_loop_add_timer),
    ("NSRunLoop", false, "addPort:forMode:", run_loop_add_port),
    ("NSRunLoop", false, "performBlock:", run_loop_perform_block),
    ("NSRunLoop", false, "currentMode", run_loop_current_mode),
    ("NSHashTable", true, "alloc", cluster_alloc),
    ("NSHashTable", true, "weakObjectsHashTable", hash_table_new),
    ("NSHashTable", true, "hashTableWithOptions:", hash_table_with_options),
    ("NSHashTable", true, "hashTableWithWeakObjects", hash_table_new),
    ("NSHashTable", false, "initWithOptions:capacity:", hash_table_with_options),
    ("NSHashTable", false, "init", hash_table_new),
    ("NSMapTable", true, "alloc", cluster_alloc),
    ("NSMapTable", true, "strongToStrongObjectsMapTable", map_table_new),
    ("NSMapTable", true, "strongToWeakObjectsMapTable", map_table_new),
    ("NSMapTable", true, "weakToStrongObjectsMapTable", map_table_new),
    ("NSMapTable", true, "weakToWeakObjectsMapTable", map_table_new),
    ("NSMapTable", true, "mapTableWithKeyOptions:valueOptions:", map_table_with_options),
    ("NSMapTable", false, "initWithKeyOptions:valueOptions:capacity:", map_table_with_options),
    ("NSMapTable", false, "init", map_table_new),
    ("NSDictionary", false, "dictionaryRepresentation", map_table_dictionary),
    // Locks: one thread, so they always succeed immediately.
    ("NSLock", false, "lock", noop),
    ("NSLock", false, "unlock", noop),
    ("NSLock", false, "tryLock", ret_yes),
    ("NSLock", false, "lockBeforeDate:", ret_yes),
    ("NSLock", false, "setName:", noop),
    ("NSRecursiveLock", false, "lock", noop),
    ("NSRecursiveLock", false, "unlock", noop),
    ("NSRecursiveLock", false, "tryLock", ret_yes),
    ("NSRecursiveLock", false, "setName:", noop),
    ("NSCondition", false, "lock", noop),
    ("NSCondition", false, "unlock", noop),
    ("NSCondition", false, "signal", noop),
    ("NSCondition", false, "broadcast", noop),
    ("NSCondition", false, "setName:", noop),
    ("NSURL", true, "URLWithString:", url_with_string),
    ("NSURL", false, "initWithString:", url_with_string),
    ("NSURL", true, "URLWithString:relativeToURL:", url_with_string_relative),
    ("NSURL", false, "initWithString:relativeToURL:", url_with_string_relative),
    ("NSURL", true, "fileURLWithPath:", file_url_with_path),
    ("NSURL", false, "initFileURLWithPath:", file_url_with_path),
    ("NSURL", true, "fileURLWithPath:isDirectory:", file_url_with_path_is_dir),
    ("NSURL", false, "initFileURLWithPath:isDirectory:", file_url_with_path_is_dir),
    ("NSURL", true, "alloc", cluster_alloc),
    ("NSURL", true, "allocWithZone:", cluster_alloc),
    ("NSDate", true, "allocWithZone:", cluster_alloc),
    ("NSData", true, "allocWithZone:", cluster_alloc),
    ("NSURL", false, "absoluteString", url_absolute_string),
    ("NSURL", false, "absoluteURL", |e| {
        let u = cf(e, "_CFURLCopyAbsoluteURL", &[e.arg(0)])?;
        e.ret(u);
        Ok(())
    }),
    ("NSURL", false, "path", url_path),
    ("NSURL", false, "scheme", url_scheme),
    ("NSURL", false, "host", url_host),
    ("NSURL", false, "lastPathComponent", url_last_path_component),
    ("NSURL", false, "pathExtension", url_path_extension),
    ("NSURL", false, "isFileURL", url_is_file),
    ("NSURL", false, "URLByAppendingPathComponent:", url_appending_component),
    ("NSURL", false, "URLByDeletingLastPathComponent", url_deleting_last_component),
    ("NSURL", false, "isEqual:", url_is_equal),
    ("NSURL", false, "hash", url_hash),
    ("NSURL", false, "description", url_description),
    ("NSURL", false, "copy", ret_self),
    ("NSURL", false, "copyWithZone:", ret_self),
    ("NSArray", false, "getObjects:range:", array_get_objects_range),
    ("NSArray", false, "objectEnumerator", array_object_enumerator),
    ("NSArray", false, "reverseObjectEnumerator", array_reverse_enumerator),
    ("NSDictionary", false, "getObjects:andKeys:count:", dict_get_objects_and_keys),
    ("NSDictionary", false, "getObjects:andKeys:", dict_get_objects_and_keys),
    ("NSDictionary", false, "keyEnumerator", dict_key_enumerator),
    ("NSDictionary", false, "objectEnumerator", dict_object_enumerator),
    ("NSSet", false, "getObjects:", set_get_objects),
    ("NSSet", false, "getObjects:count:", set_get_objects),
    ("NSSet", false, "objectEnumerator", set_object_enumerator),
    ("__NSFastEnumerationEnumerator", false, "nextObject", enumerator_next),
    ("__NSFastEnumerationEnumerator", false, "allObjects", enumerator_all_objects),
    ("__NSFastEnumerationEnumerator", false, "countByEnumeratingWithState:objects:count:", enumerator_fast_enumerate),
    ("NSNumber", false, "objCType", number_objc_type),
    ("NSMutableArray", false, "sortUsingDescriptors:", sort_using_descriptors),
    ("NSArray", false, "sortedArrayUsingDescriptors:", sorted_using_descriptors),
    ("NSArray", false, "valueForKey:", array_value_for_key),
    ("NSArray", false, "sortedArrayUsingComparator:", |e| sort_with(e, Some(2), None, false)),
    ("NSArray", false, "sortedArrayWithOptions:usingComparator:", |e| sort_with(e, Some(3), None, false)),
    ("NSArray", false, "sortedArrayUsingSelector:", |e| sort_with(e, None, Some(2), false)),
    ("NSMutableArray", false, "sortUsingComparator:", |e| sort_with(e, Some(2), None, true)),
    ("NSMutableArray", false, "sortWithOptions:usingComparator:", |e| sort_with(e, Some(3), None, true)),
    ("NSMutableArray", false, "sortUsingSelector:", |e| sort_with(e, None, Some(2), true)),
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
    ("NSString", true, "stringWithCString:encoding:", string_with_utf8),
    ("NSString", false, "initWithCString:encoding:", string_with_utf8),
    ("NSString", true, "stringWithString:", string_with_string),
    ("NSString", false, "initWithString:", string_with_string),
    ("NSString", true, "stringWithFormat:", string_with_format),
    ("NSString", false, "initWithFormat:", string_with_format),
    ("NSString", false, "initWithFormat:arguments:", string_with_format_va),
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
    ("NSArray", false, "isEqualToArray:", array_is_equal),
    ("NSDictionary", true, "dictionaryWithObjectsAndKeys:", dict_with_objects_and_keys),
    ("NSArray", false, "isEqual:", array_is_equal),
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
    ("NSMutableArray", false, "setObject:atIndexedSubscript:", array_set_subscript),
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
    ("NSSet", true, "setWithObjects:", set_with_objects_varargs),
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

/// A CF dictionary with the same entries, for handing to CoreFoundation
/// (constant literals and Swift-native dictionaries aren't CF objects).
pub fn as_cf_dict(emu: &mut Emu, d: u64) -> Result<u64> {
    if d == 0 {
        return new_dict(emu);
    }
    if is_cf(emu, d) && constant_kind(emu, d).is_none() {
        return Ok(d);
    }
    let pairs = if constant_kind(emu, d).is_some() {
        dict_pairs(emu, d)?
    } else {
        let e = send(emu, d, "keyEnumerator", &[])?;
        let mut v = Vec::new();
        loop {
            let k = send(emu, e, "nextObject", &[])?;
            if k == 0 {
                break;
            }
            let val = send(emu, d, "objectForKey:", &[k])?;
            v.push((k, val));
        }
        v
    };
    let out = new_dict(emu)?;
    for (k, v) in pairs {
        let k = as_cf_string(emu, k)?;
        cf(emu, "_CFDictionarySetValue", &[out, k, v])?;
    }
    Ok(out)
}

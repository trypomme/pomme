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

#[derive(Clone, Default)]
pub struct State {
    /// Key-value observing (kvo.rs): registrations, changes begun
    /// (`willChangeValueForKey:`), and views pomme changed to tell of.
    pub(crate) kvo: Vec<crate::kvo::Observation>,
    pub(crate) kvo_pending: Vec<(u64, String, Vec<(crate::kvo::Observation, Option<u64>)>)>,
    pub(crate) kvo_later: Vec<(u64, &'static str)>,
    /// The CoreFoundation image, once loaded.
    cf: Option<usize>,
    syms: HashMap<&'static str, u64>,
    /// Per thread, where each `runMode:beforeDate:` in progress returns to
    /// (it tail-calls CFRunLoopRunInMode; see run_loop_run_mode).
    run_mode_returns: HashMap<usize, Vec<u64>>,
    /// Classes CF objects can have as their isa.
    cf_classes: HashSet<u64>,
    const_string_class: u64,
    pub(crate) true_obj: u64,
    false_obj: u64,
    pub(crate) null_obj: u64,
    /// NSRunLoop object for each CFRunLoop.
    run_loops: HashMap<u64, u64>,
    center: u64,
    observers: Vec<Observer>,
    file_manager: u64,
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
    for &(type_fn, class) in types.iter().chain(crate::streams::BRIDGED) {
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
    crate::constants::bind(emu)?;
    // Foundation's NSString constants that are CF's under another name
    // (after constants::bind, whose stand-ins by name would replace them:
    // NSDefaultRunLoopMode is kCFRunLoopDefaultMode, so CF's run loop finds
    // what was added in either's name).
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
    // `@[]` and friends compile to references to Apple's empty-collection
    // singletons: the object itself (`__NSArray0__struct`) or a pointer to it
    // (`__NSArray0__`), depending on the compiler.
    for (name, make) in [
        ("___NSArray0__", new_array as fn(&mut Emu, bool) -> Result<u64>),
        ("___NSDictionary0__", |e: &mut Emu, _| new_dict(e)),
        ("___NSSet0__", |e: &mut Emu, _| new_set(e)),
    ] {
        let objects = emu.p.stubs.find(&format!("{name}struct"));
        let pointers = emu.p.stubs.find(name);
        if objects.is_empty() && pointers.is_empty() {
            continue;
        }
        let obj = make(emu, false)?;
        for s in objects {
            emu.p.rebind(s, obj).map_err(|e| Stop::Unsupported(e.to_string()))?;
        }
        if !pointers.is_empty() {
            let cell = emu.alloc(8)?;
            emu.write_u64(cell, obj)?;
            for s in pointers {
                emu.p.rebind(s, cell).map_err(|e| Stop::Unsupported(e.to_string()))?;
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
    crate::userdefaults::hook_cf_preferences(emu)?;
    crate::locale::hook_cf(emu)?;
    crate::tollfree::install(emu)?;
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
    let isa = emu.read_u64(obj).unwrap_or(0);
    let name = if isa != 0 { emu.objc.class_name(isa) } else { String::new() };
    let id = match name.as_str() {
        "CTFont" => crate::coretext::CTFONT_TYPE_ID,
        "CTFontDescriptor" => crate::coretext::CTFONT_DESCRIPTOR_TYPE_ID,
        "CGPath" => crate::coretext::CGPATH_TYPE_ID,
        "CGContext" => crate::coretext::CGCONTEXT_TYPE_ID,
        // An Objective-C or Swift NSArray, NSString, …: the type it bridges to.
        _ => crate::tollfree::bridged_type_id(emu, obj)?.unwrap_or(1), // _kCFRuntimeIDCFType
    };
    emu.ret(id);
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

/// `new` and `init` of a class cluster (NSArray, NSDictionary, NSSet, NSString,
/// NSData) sent to a subclass the app defined: such a class keeps its own
/// storage and answers the primitives itself, so `new` is `alloc` + `init`
/// and `init` returns the instance, as Foundation's abstract classes do.
/// None when the receiver is one of the cluster's own classes.
fn subclass_new_or_init(emu: &mut Emu) -> Result<Option<()>> {
    let recv = emu.arg(0);
    if recv == 0 {
        return Ok(None);
    }
    let is_class = emu.objc.class(recv).is_some_and(|c| !c.meta) && emu.objc.classes.get(&emu.read_u64(recv)?).is_some_and(|m| m.meta);
    let cls = if is_class { recv } else { emu.read_u64(recv)? };
    if emu.objc.class(cls).is_none_or(|c| c.host) || emu.read_u64(recv + 8).ok() == Some(PLACEHOLDER) {
        return Ok(None);
    }
    let obj = if is_class {
        let o = send(emu, recv, "alloc", &[])?;
        send(emu, o, "init", &[])?
    } else {
        recv
    };
    emu.ret(obj);
    Ok(Some(()))
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

/// `NSStringFromProtocol(proto)`: its name.
fn ns_string_from_protocol(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    if p == 0 {
        emu.ret(0);
        return Ok(());
    }
    let name = objc::protocol_name(emu, p)?;
    ret_string(emu, &name)
}

/// `NSProtocolFromString(name)`: the protocol, or nil.
fn ns_protocol_from_string(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let name = if s == 0 { String::new() } else { nsstring(emu, s)? };
    emu.ret(emu.objc.protocols.get(&name).copied().unwrap_or(0));
    Ok(())
}

fn ns_log(emu: &mut Emu) -> Result<()> {
    let fmt = nsstring(emu, emu.arg(0))?;
    let s = crate::printf::format(emu, fmt.as_bytes(), &mut crate::printf::Args::new(emu.cpu.sp))?;
    println!("[NSLog] {}", String::from_utf8_lossy(&s));
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    // NSRange (location, length) arithmetic: ranges in x0/x1 and x2/x3, the result in x0/x1.
    ("_NSIntersectionRange", |e| {
        let (a0, a1, b0, b1) = (e.cpu.x[0], e.cpu.x[0].saturating_add(e.cpu.x[1]), e.cpu.x[2], e.cpu.x[2].saturating_add(e.cpu.x[3]));
        let (lo, hi) = (a0.max(b0), a1.min(b1));
        (e.cpu.x[0], e.cpu.x[1]) = if lo < hi { (lo, hi - lo) } else { (0, 0) };
        Ok(())
    }),
    ("_NSUnionRange", |e| {
        let (a0, a1, b0, b1) = (e.cpu.x[0], e.cpu.x[0].saturating_add(e.cpu.x[1]), e.cpu.x[2], e.cpu.x[2].saturating_add(e.cpu.x[3]));
        let (lo, hi) = (a0.min(b0), a1.max(b1));
        (e.cpu.x[0], e.cpu.x[1]) = (lo, hi - lo);
        Ok(())
    }),
    ("_NSStringFromClass", ns_string_from_class),
    ("_NSClassFromString", ns_class_from_string),
    ("_NSStringFromSelector", ns_string_from_selector),
    ("_NSSelectorFromString", ns_selector_from_string),
    ("_NSStringFromProtocol", ns_string_from_protocol),
    ("_NSProtocolFromString", ns_protocol_from_string),
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

/// A string a factory or initializer made: mutable when it was sent to
/// NSMutableString (`[[NSMutableString alloc] initWithFormat:...]`), as in Foundation.
fn ret_made_string(emu: &mut Emu, s: &str) -> Result<()> {
    if have_cf(emu) && wants_mutable(emu, emu.arg(0), "NSMutableString")? {
        let m = new_mutable_string(emu, s)?;
        emu.ret(m);
        return Ok(());
    }
    ret_string(emu, s)
}

fn string_with_utf8(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(2);
    if p == 0 {
        emu.ret(0);
        return Ok(());
    }
    let s = emu.cstr(p)?;
    ret_made_string(emu, &s)
}

/// `stringWithCharacters:length:` / `initWithCharacters:length:`: UTF-16 units.
fn string_with_characters(emu: &mut Emu) -> Result<()> {
    let (p, len) = (emu.arg(2), emu.arg(3));
    let mut units = Vec::with_capacity(len as usize);
    for i in 0..len {
        units.push(emu.p.memory.read_u16(p + 2 * i)?);
    }
    ret_made_string(emu, &String::from_utf16_lossy(&units))
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
    if subclass_new_or_init(emu)?.is_some() {
        return Ok(());
    }
    if have_cf(emu) && wants_mutable(emu, emu.arg(0), "NSMutableString")? {
        let m = new_mutable_string(emu, "")?;
        emu.ret(m);
        return Ok(());
    }
    ret_string(emu, "")
}

/// `+stringWithFormat:` / `-initWithFormat:`: format is x2, varargs on the stack.
fn string_with_format(emu: &mut Emu) -> Result<()> {
    let s = format_with(emu, emu.arg(2), emu.cpu.sp)?;
    ret_made_string(emu, &s)
}

/// `-initWithFormat:arguments:` — format x2, va_list x3.
fn string_with_format_va(emu: &mut Emu) -> Result<()> {
    let s = format_with(emu, emu.arg(2), emu.arg(3))?;
    ret_made_string(emu, &s)
}

/// `-initWithFormat:locale:` / `+localizedStringWithFormat:` — format x2,
/// varargs on the stack (the locale, x3, isn't applied yet).
fn string_with_format_locale(emu: &mut Emu) -> Result<()> {
    let s = format_with(emu, emu.arg(2), emu.cpu.sp)?;
    ret_made_string(emu, &s)
}

/// `-initWithFormat:locale:arguments:` — format x2, locale x3, va_list x4.
fn string_with_format_locale_va(emu: &mut Emu) -> Result<()> {
    let s = format_with(emu, emu.arg(2), emu.arg(4))?;
    ret_made_string(emu, &s)
}

/// Format guest string `fmt` (a plain or a plural format, localizable.rs)
/// with the arguments from `args`.
fn format_with(emu: &mut Emu, fmt: u64, args: u64) -> Result<String> {
    let mut args = crate::printf::Args::new(args);
    let f = crate::localizable::format_of(emu, fmt, &args)?;
    let s = crate::printf::format(emu, f.as_bytes(), &mut args)?;
    Ok(String::from_utf8_lossy(&s).into_owned())
}

fn string_by_appending(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = nsstring(emu, emu.arg(2))?;
    ret_string(emu, &(a + &b))
}

/// `-stringByAppendingFormat:`: format x2, varargs on the stack.
fn string_by_appending_format(emu: &mut Emu) -> Result<()> {
    let a = nsstring(emu, emu.arg(0))?;
    let b = format_with(emu, emu.arg(2), emu.cpu.sp)?;
    ret_string(emu, &(a + &b))
}

/// `-[NSMutableString appendFormat:]`.
fn append_format(emu: &mut Emu) -> Result<()> {
    let b = format_with(emu, emu.arg(2), emu.cpu.sp)?;
    let b = new_nsstring(emu, &b)?;
    let s = as_cf_string(emu, b)?;
    cf(emu, "_CFStringAppend", &[emu.arg(0), s])?;
    Ok(())
}

fn append_string(emu: &mut Emu) -> Result<()> {
    let s = as_cf_string(emu, emu.arg(2))?;
    cf(emu, "_CFStringAppend", &[emu.arg(0), s])?;
    Ok(())
}

/// The composed character sequence (a user-perceived character: a surrogate
/// pair, a base and its combining marks, an emoji sequence) at `i`, from
/// CoreFoundation: (location, length).
fn composed_at(emu: &mut Emu, s: u64, i: u64) -> Result<(u64, u64)> {
    let f = cf_sym(emu, "_CFStringGetRangeOfComposedCharactersAtIndex")?;
    emu.call_abi(f, &[s, i], &[], &[], None)?;
    Ok((emu.last_x01[0], emu.last_x01[1]))
}

/// `-rangeOfComposedCharacterSequencesForRange:` (NSRange in and out, in x0/x1):
/// widened so it doesn't split a composed character at either end.
fn range_of_composed(emu: &mut Emu) -> Result<()> {
    let (s, loc, len) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let s = as_cf_string(emu, s)?;
    let n = cf(emu, "_CFStringGetLength", &[s])?;
    let start = if loc < n { composed_at(emu, s, loc)?.0 } else { loc };
    let end = if len == 0 {
        start.max(loc)
    } else if loc + len <= n && loc + len > 0 {
        let (l, k) = composed_at(emu, s, loc + len - 1)?;
        l + k
    } else {
        loc + len
    };
    emu.cpu.x[0] = start;
    emu.cpu.x[1] = end.max(start) - start;
    Ok(())
}

/// `-rangeOfComposedCharacterSequenceAtIndex:`.
fn range_of_composed_at(emu: &mut Emu) -> Result<()> {
    let (s, i) = (emu.arg(0), emu.arg(2));
    let s = as_cf_string(emu, s)?;
    let (l, k) = composed_at(emu, s, i)?;
    emu.cpu.x[0] = l;
    emu.cpu.x[1] = k;
    Ok(())
}

/// `-replaceCharactersInRange:withString:` (and the edits built on it).
fn replace_characters(emu: &mut Emu) -> Result<()> {
    let (m, loc, len, with) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4));
    let with = as_cf_string(emu, with)?;
    cf(emu, "_CFStringReplace", &[m, loc, len, with])?;
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

// NSStringCompareOptions (CFStringCompareFlags has the same values).
const COMPARE_CASE_INSENSITIVE: u64 = 1;
const COMPARE_LOCALIZED: u64 = 32;
const COMPARE_NUMERIC: u64 = 64;
const COMPARE_WIDTH_INSENSITIVE: u64 = 256;

/// Compare the receiver (in `range`, or all of it) with x2 the way
/// `compare:options:range:locale:` does: CoreFoundation's comparison, with a
/// locale when it's localized (the current one, en_US, unless given).
fn compare_with(emu: &mut Emu, options: u64, range: Option<(u64, u64)>, locale: u64) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    if b == 0 {
        emu.ret(1); // NSOrderedDescending: nil sorts first
        return Ok(());
    }
    let (a, b) = (as_cf_string(emu, a)?, as_cf_string(emu, b)?);
    let (loc, len) = match range {
        Some(r) => r,
        None => (0, cf(emu, "_CFStringGetLength", &[a])?),
    };
    let locale = if locale != 0 {
        locale
    } else if options & COMPARE_LOCALIZED != 0 {
        cf(emu, "_CFLocaleCopyCurrent", &[])?
    } else {
        0
    };
    let r = cf(emu, "_CFStringCompareWithOptionsAndLocale", &[a, b, loc, len, options & !COMPARE_LOCALIZED, locale])?;
    emu.ret(r as i64 as u64);
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

/// `-stringByPaddingToLength:withString:startingAtIndex:` in UTF-16 units: cut
/// to the length, or extended with the pad repeated from `start` on.
fn pad_to_length(text: &[u16], len: usize, pad: &[u16], start: usize) -> Vec<u16> {
    if len <= text.len() {
        return text[..len].to_vec();
    }
    let mut out = text.to_vec();
    out.extend(pad.iter().cycle().skip(start).take(len - text.len()));
    out
}

fn string_padding(emu: &mut Emu) -> Result<()> {
    let (s, len, pad, start) = (emu.arg(0), emu.arg(2) as usize, emu.arg(3), emu.arg(4) as usize);
    let text: Vec<u16> = nsstring(emu, s)?.encode_utf16().collect();
    let pad: Vec<u16> = nsstring(emu, pad)?.encode_utf16().collect();
    if len > text.len() && (pad.is_empty() || start >= pad.len()) {
        return Err(Stop::Unsupported(format!("uncaught Objective-C exception NSRangeException: padding index {start} beyond the pad string's length {}", pad.len())));
    }
    let out = pad_to_length(&text, len, &pad, start);
    ret_string(emu, &String::from_utf16_lossy(&out))
}

/// `+[NSString pathWithComponents:]`: the components joined with "/", a
/// leading "/" component making the path absolute; empty ones are skipped.
fn join_path_components(parts: &[String]) -> String {
    let absolute = parts.first().is_some_and(|p| p.starts_with('/'));
    let names: Vec<&str> = parts.iter().map(|p| p.trim_matches('/')).filter(|p| !p.is_empty()).collect();
    let joined = names.join("/");
    if absolute { format!("/{joined}") } else { joined }
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

// NSStringCompareOptions for searching.
const SEARCH_REGULAR_EXPRESSION: u64 = 1024;

/// `rangeOfString:options:range:locale:` (and its shorter forms): the first
/// (or, backwards, last) match of x2 in the receiver's `range`, as
/// CoreFoundation's search finds it; regular-expression searches use the
/// regex engine. NSRange {location, length} in x0/x1, NSNotFound when none.
fn range_of_string_with(emu: &mut Emu, options: u64, range: Option<(u64, u64)>, locale: u64) -> Result<()> {
    let (s, needle) = (emu.arg(0), emu.arg(2));
    let not_found = |emu: &mut Emu| {
        emu.cpu.x[0] = i64::MAX as u64;
        emu.cpu.x[1] = 0;
        Ok(())
    };
    if needle == 0 {
        return not_found(emu);
    }
    if options & SEARCH_REGULAR_EXPRESSION != 0 {
        let units: Vec<u16> = nsstring(emu, s)?.encode_utf16().collect();
        let (loc, len) = range.map_or((0, units.len()), |(l, n)| (l as usize, n as usize));
        let Some(slice) = units.get(loc..loc + len) else { return not_found(emu) };
        let hay = String::from_utf16_lossy(slice);
        let pattern = nsstring(emu, needle)?;
        let pattern = if options & COMPARE_CASE_INSENSITIVE != 0 { format!("(?i){pattern}") } else { pattern };
        let pattern = if options & 8 != 0 { format!("^(?:{pattern})") } else { pattern }; // NSAnchoredSearch
        let Ok(re) = regex::Regex::new(&pattern) else { return not_found(emu) };
        return match re.find(&hay) {
            Some(m) => {
                emu.cpu.x[0] = (loc + hay[..m.start()].encode_utf16().count()) as u64;
                emu.cpu.x[1] = m.as_str().encode_utf16().count() as u64;
                Ok(())
            }
            None => not_found(emu),
        };
    }
    let (s, needle) = (as_cf_string(emu, s)?, as_cf_string(emu, needle)?);
    let (loc, len) = match range {
        Some(r) => r,
        None => (0, cf(emu, "_CFStringGetLength", &[s])?),
    };
    let out = emu.alloc(16)?;
    let found = cf(emu, "_CFStringFindWithOptionsAndLocale", &[s, needle, loc, len, options, locale, out])? & 0xff != 0;
    if !found {
        return not_found(emu);
    }
    emu.cpu.x[0] = emu.read_u64(out)?;
    emu.cpu.x[1] = emu.read_u64(out + 8)?;
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

/// `-lengthOfBytesUsingEncoding:`: the bytes the string takes in that encoding
/// (no byte-order mark), or 0 if it can't be converted.
fn length_of_bytes(emu: &mut Emu) -> Result<()> {
    let (s, enc) = (emu.arg(0), emu.arg(2));
    let s = as_cf_string(emu, s)?;
    let len = cf(emu, "_CFStringGetLength", &[s])?;
    let used = emu.alloc(8)?;
    emu.write_u64(used, 0)?;
    let f = cf_sym(emu, "_CFStringGetBytes")?;
    let cf_enc = cf_encoding(emu, enc)?;
    emu.call_abi(f, &[s, 0, len, cf_enc, 0, 0, 0, 0], &[], &[used], None)?;
    let converted = emu.last_x01[0];
    let n = emu.read_u64(used)?;
    emu.heap.free(&emu.p.memory, used)?;
    emu.ret(if converted == len { n } else { 0 });
    Ok(())
}

// ---- the selectors the Swift standard library uses to read foreign strings ----

const CF_ASCII: u64 = 0x0600;

/// NSStringEncoding → CFStringEncoding, as CoreFoundation maps them (UTF-16
/// and UTF-32 in either byte order, the 8-bit sets, the CJK encodings…).
fn cf_encoding(emu: &mut Emu, ns: u64) -> Result<u64> {
    Ok(cf(emu, "_CFStringConvertNSStringEncodingToEncoding", &[ns])? & 0xffff_ffff)
}

/// Whether Foundation writes a byte-order mark for this encoding: the
/// native-order Unicode forms (NSUnicodeStringEncoding, NSUTF32StringEncoding).
fn writes_bom(ns: u64) -> bool {
    ns == 10 || ns == 0x8c00_0100
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
    if !is_cf(emu, s) {
        // A subclass's own string: built on its characterAtIndex: primitive,
        // as NSString's default implementation is.
        for i in 0..len {
            let c = send(emu, s, "characterAtIndex:", &[loc + i])?;
            emu.p.memory.write_u16(buf + i * 2, c as u16)?;
        }
        return Ok(());
    }
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
    // usedBufLen): usedBufLen is the 9th argument, on the stack.
    let used_tmp = emu.alloc(8)?;
    emu.write_u64(used_tmp, 0)?;
    let f = cf_sym(emu, "_CFStringGetBytes")?;
    let cf_enc = cf_encoding(emu, enc)?;
    emu.call_abi(f, &[s, loc, len, cf_enc, 0, 0, buf, max], &[], &[used_tmp], None)?;
    let converted = emu.last_x01[0];
    if used != 0 {
        let n = emu.read_u64(used_tmp)?;
        emu.write_u64(used, n)?;
    }
    emu.heap.free(&emu.p.memory, used_tmp)?;
    if remaining != 0 {
        emu.write_u64(remaining, loc + converted)?;
        emu.write_u64(remaining + 8, len - converted.min(len))?;
    }
    emu.ret((converted > 0 || len == 0) as u64);
    Ok(())
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

fn ret_no(emu: &mut Emu) -> Result<()> {
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
    if subclass_new_or_init(emu)?.is_some() {
        return Ok(());
    }
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

/// `-isEqualToDictionary:` / `-isEqual:`: a dictionary with the same keys,
/// each with an equal value.
fn dict_is_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    let dict = crate::objc::host_class(emu, "NSDictionary")?;
    let eq = a == b || (b != 0 && send(emu, b, "isKindOfClass:", &[dict])? & 0xff != 0 && {
        let (x, y) = (as_cf_dict(emu, a)?, as_cf_dict(emu, b)?);
        let (px, py) = (dict_pairs(emu, x)?, dict_pairs(emu, y)?);
        let mut same = px.len() == py.len();
        for (k, v) in px {
            if !same {
                break;
            }
            let other = send(emu, b, "objectForKey:", &[k])?;
            same = other != 0 && (other == v || send(emu, v, "isEqual:", &[other])? & 0xff != 0);
        }
        same
    });
    emu.ret(eq as u64);
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
    if subclass_new_or_init(emu)?.is_some() {
        return Ok(());
    }
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

/// `-removeObjectIdenticalTo:`: every occurrence of this very object (by address, not isEqual:).
fn array_remove_identical(emu: &mut Emu) -> Result<()> {
    let (a, obj) = (emu.arg(0), emu.arg(2));
    let values = array_values(emu, a)?;
    for (i, &v) in values.iter().enumerate().rev() {
        if v == obj {
            cf(emu, "_CFArrayRemoveValueAtIndex", &[a, i as u64])?;
        }
    }
    Ok(())
}

/// `-indexOfObjectIdenticalTo:`: the first index holding this very object, or NSNotFound.
fn array_index_identical(emu: &mut Emu) -> Result<()> {
    let (a, obj) = (emu.arg(0), emu.arg(2));
    let i = array_values(emu, a)?.iter().position(|&v| v == obj).map_or(i64::MAX as u64, |i| i as u64);
    emu.ret(i);
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

/// Call `block(obj, idx, &stop)` for each element in `order` until it returns
/// true or stops: the indexes it returned true for.
fn array_passing(emu: &mut Emu, a: u64, block: u64, reverse: bool, first_only: bool) -> Result<Vec<u64>> {
    let stop = emu.alloc(8)?;
    let items = array_values(emu, a)?;
    let mut order: Vec<usize> = (0..items.len()).collect();
    if reverse {
        order.reverse();
    }
    let mut out = Vec::new();
    for i in order {
        if crate::libsystem::call_block(emu, block, &[items[i], i as u64, stop])? & 0xff != 0 {
            out.push(i as u64);
            if first_only {
                break;
            }
        }
        if emu.p.memory.read_u8(stop)? != 0 {
            break;
        }
    }
    Ok(out)
}

/// An NSIndexSet of `indexes`.
fn new_index_set(emu: &mut Emu, indexes: &[u64]) -> Result<u64> {
    let cls = host_class(emu, "NSMutableIndexSet")?;
    let set = send(emu, cls, "indexSet", &[])?;
    for &i in indexes {
        send(emu, set, "addIndex:", &[i])?;
    }
    Ok(set)
}

// NSEnumerationOptions
const ENUMERATION_REVERSE: u64 = 2;

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
pub(crate) fn fast_enumerate(emu: &mut Emu, values: impl FnOnce(&mut Emu) -> Result<Vec<u64>>) -> Result<()> {
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
    if !is_cf(emu, d) {
        return subclass_dict_pairs(emu, d);
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

/// An NSDictionary subclass of the app's (Swift's _SwiftDeferredNSDictionary,
/// a bridged `[K: V]`): its entries through the primitives it implements,
/// `keyEnumerator` and `objectForKey:` (NSDictionary's contract), as
/// NSDictionary's other methods use them.
fn subclass_dict_pairs(emu: &mut Emu, d: u64) -> Result<Vec<(u64, u64)>> {
    let cls = objc::class_of(emu, d)?;
    let sel = objc::intern(emu, "keyEnumerator")?;
    if !matches!(objc::lookup(emu, cls, sel)?, Some(objc::Imp::Guest(_))) {
        return Ok(Vec::new());
    }
    let e = send(emu, d, "keyEnumerator", &[])?;
    let mut out = Vec::new();
    loop {
        let k = send(emu, e, "nextObject", &[])?;
        if k == 0 {
            break;
        }
        let v = send(emu, d, "objectForKey:", &[k])?;
        out.push((k, v));
    }
    Ok(out)
}

fn dict_empty(emu: &mut Emu) -> Result<()> {
    if subclass_new_or_init(emu)?.is_some() {
        return Ok(());
    }
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

/// `dictionaryWithObjects:forKeys:` (and `initWithObjects:forKeys:`): two arrays, paired.
fn dict_with_object_and_key_arrays(emu: &mut Emu) -> Result<()> {
    let (vals, keys) = (emu.arg(2), emu.arg(3));
    let (vals, keys) = (array_values(emu, vals)?, array_values(emu, keys)?);
    if vals.len() != keys.len() {
        return Err(Stop::Unsupported(format!("NSInvalidArgumentException: {} objects for {} keys", vals.len(), keys.len())));
    }
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

/// `objectForKeyedSubscript:` and `valueForKey:`: `objectForKey:`, which an
/// app's NSDictionary subclass answers itself.
fn dict_lookup(emu: &mut Emu) -> Result<()> {
    let (d, k) = (emu.arg(0), emu.arg(2));
    if !is_cf(emu, d) && constant_kind(emu, d).is_none() {
        let v = if k == 0 { 0 } else { send(emu, d, "objectForKey:", &[k])? };
        emu.ret(v);
        return Ok(());
    }
    dict_object_for_key(emu)
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

pub(crate) fn set_values(emu: &mut Emu, s: u64) -> Result<Vec<u64>> {
    let s = crate::objc::bag_get(emu, s, COUNTED_STORE).unwrap_or(s);
    // A Swift Set handed to Objective-C (`_SetStorage`) isn't a CFSet: ask it
    // for its objects as Objective-C code would (its objectEnumerator).
    if crate::objc::is_native_swift(emu, s) {
        let e = send(emu, s, "objectEnumerator", &[])?;
        let mut v = Vec::new();
        if e != 0 {
            loop {
                let o = send(emu, e, "nextObject", &[])?;
                if o == 0 {
                    break;
                }
                v.push(o);
            }
        }
        return Ok(v);
    }
    let n = cf(emu, "_CFSetGetCount", &[s])?;
    let buf = emu.alloc(n * 8 + 8)?;
    cf(emu, "_CFSetGetValues", &[s, buf])?;
    let v = read_ptrs(emu, buf, n)?;
    emu.heap.free(&emu.p.memory, buf)?;
    Ok(v)
}

fn set_empty(emu: &mut Emu) -> Result<()> {
    if subclass_new_or_init(emu)?.is_some() {
        return Ok(());
    }
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

/// `-isEqualToSet:` / `-isEqual:`: a set of the same size, each member in it.
fn set_is_equal(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(2));
    let set = crate::objc::host_class(emu, "NSSet")?;
    let eq = a == b || (b != 0 && send(emu, b, "isKindOfClass:", &[set])? & 0xff != 0 && {
        let (x, y) = (set_values(emu, a)?, set_values(emu, b)?.len());
        let mut same = x.len() == y;
        for v in x {
            if !same {
                break;
            }
            same = send(emu, b, "containsObject:", &[v])? & 0xff != 0;
        }
        same
    });
    emu.ret(eq as u64);
    Ok(())
}

/// `-hash` of a collection: its count, as Foundation's is, so equal collections hash alike.
fn hash_is_count(emu: &mut Emu) -> Result<()> {
    let n = send(emu, emu.arg(0), "count", &[])?;
    emu.ret(n);
    Ok(())
}

fn set_count(emu: &mut Emu) -> Result<()> {
    let n = cf(emu, "_CFSetGetCount", &[emu.arg(0)])?;
    emu.ret(n);
    Ok(())
}

// ---- NSCountedSet ----
//
// pomme's sets are CFSets, which can't count. A counted set is an object of
// its own class (so it answers countForObject:), holding its members in a
// CFSet ("countedStore") and how many times each was added in a CFDictionary
// ("countedCounts", member -> NSNumber), both in guest memory. NSSet's
// methods it doesn't override are passed to the members' set.

const COUNTED_STORE: &str = "countedStore";
const COUNTED_COUNTS: &str = "countedCounts";

/// Give a counted set its storage (once), returning the members' CFSet.
fn counted_storage(emu: &mut Emu, s: u64) -> Result<u64> {
    if let Some(store) = crate::objc::bag_get(emu, s, COUNTED_STORE) {
        return Ok(store);
    }
    let store = new_set(emu)?;
    let kcb = cf_sym(emu, "_kCFTypeDictionaryKeyCallBacks")?;
    let vcb = cf_sym(emu, "_kCFTypeDictionaryValueCallBacks")?;
    let counts = cf(emu, "_CFDictionaryCreateMutable", &[0, 0, kcb, vcb])?;
    crate::objc::bag_set(emu, s, COUNTED_STORE, store);
    crate::objc::bag_set(emu, s, COUNTED_COUNTS, counts);
    Ok(store)
}

fn counted_count_of(emu: &mut Emu, s: u64, o: u64) -> Result<u64> {
    counted_storage(emu, s)?;
    let counts = crate::objc::bag_get(emu, s, COUNTED_COUNTS).unwrap_or(0);
    let n = cf(emu, "_CFDictionaryGetValue", &[counts, o])?;
    Ok(if n == 0 { 0 } else { send(emu, n, "unsignedLongLongValue", &[])? })
}

fn counted_set_count(emu: &mut Emu, s: u64, o: u64, n: u64) -> Result<()> {
    let store = counted_storage(emu, s)?;
    let counts = crate::objc::bag_get(emu, s, COUNTED_COUNTS).unwrap_or(0);
    if n == 0 {
        cf(emu, "_CFDictionaryRemoveValue", &[counts, o])?;
        cf(emu, "_CFSetRemoveValue", &[store, o])?;
    } else {
        let number = host_class(emu, "NSNumber")?;
        let v = send(emu, number, "numberWithUnsignedLongLong:", &[n])?;
        cf(emu, "_CFDictionarySetValue", &[counts, o, v])?;
        cf(emu, "_CFSetAddValue", &[store, o])?;
    }
    Ok(())
}

fn counted_add(emu: &mut Emu, s: u64, o: u64) -> Result<()> {
    if o != 0 {
        let n = counted_count_of(emu, s, o)?;
        counted_set_count(emu, s, o, n + 1)?;
    }
    Ok(())
}

/// +set, +setWithCapacity:, +new: a fresh counted set; +setWithArray:, +setWithSet:, +setWithObject: filled.
fn counted_make(emu: &mut Emu, fill: u8) -> Result<()> {
    let (cls, arg) = (emu.arg(0), emu.arg(2));
    let s = crate::objc::create_instance(emu, cls, 0)?;
    counted_fill(emu, s, arg, fill)?;
    emu.ret(s);
    Ok(())
}

/// -init…: storage, and the given objects (fill: 0 none, 1 an array or set, 2 one object).
fn counted_init(emu: &mut Emu, fill: u8) -> Result<()> {
    let (s, arg) = (emu.arg(0), emu.arg(2));
    counted_fill(emu, s, arg, fill)?;
    emu.ret(s);
    Ok(())
}

fn counted_fill(emu: &mut Emu, s: u64, arg: u64, fill: u8) -> Result<()> {
    counted_storage(emu, s)?;
    match fill {
        1 => {
            for o in elements_of(emu, arg)? {
                counted_add(emu, s, o)?;
            }
        }
        2 => counted_add(emu, s, arg)?,
        _ => {}
    }
    Ok(())
}

/// An NSSet method a counted set leaves to its members' set: the same message, sent there.
fn counted_forward(emu: &mut Emu, sel: &str) -> Result<()> {
    let s = emu.arg(0);
    let store = counted_storage(emu, s)?;
    let args = [emu.cpu.x[2], emu.cpu.x[3], emu.cpu.x[4], emu.cpu.x[5]];
    let r = send(emu, store, sel, &args)?;
    emu.ret(r);
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

/// Any NSSet's elements (the argument of a set operation may not be a CF set).
fn elements_of(emu: &mut Emu, set: u64) -> Result<Vec<u64>> {
    if set == 0 {
        return Ok(Vec::new());
    }
    let all = send(emu, set, "allObjects", &[])?;
    array_values(emu, all)
}

fn set_has(emu: &mut Emu, set: u64, o: u64) -> Result<bool> {
    Ok(send(emu, set, "containsObject:", &[o])? & 0xff != 0)
}

/// `isSubsetOfSet:`: every element is in the other set.
fn set_is_subset(emu: &mut Emu) -> Result<()> {
    let (s, other) = (emu.arg(0), emu.arg(2));
    let mut all = true;
    for o in set_values(emu, s)? {
        if other == 0 || !set_has(emu, other, o)? {
            all = false;
            break;
        }
    }
    ret_bool(emu, all)
}

/// `intersectsSet:`: some element is in both.
fn set_intersects(emu: &mut Emu) -> Result<()> {
    let (s, other) = (emu.arg(0), emu.arg(2));
    let mut any = false;
    for o in elements_of(emu, other)? {
        if cf(emu, "_CFSetContainsValue", &[s, o])? & 0xff != 0 {
            any = true;
            break;
        }
    }
    ret_bool(emu, any)
}

/// A new set: the receiver's elements and `more`.
fn set_adding(emu: &mut Emu, more: Vec<u64>) -> Result<()> {
    let src = emu.arg(0);
    let out = new_set(emu)?;
    for o in set_values(emu, src)?.into_iter().chain(more) {
        cf(emu, "_CFSetAddValue", &[out, o])?;
    }
    emu.ret(out);
    Ok(())
}

/// NSMutableSet's in-place operations: keep elements by `keep(in_other)`, or add the other's.
fn set_combine(emu: &mut Emu, op: &str) -> Result<()> {
    let (s, other) = (emu.arg(0), emu.arg(2));
    match op {
        "union" => {
            for o in elements_of(emu, other)? {
                cf(emu, "_CFSetAddValue", &[s, o])?;
            }
        }
        "minus" => {
            for o in elements_of(emu, other)? {
                cf(emu, "_CFSetRemoveValue", &[s, o])?;
            }
        }
        "intersect" => {
            for o in set_values(emu, s)? {
                if other == 0 || !set_has(emu, other, o)? {
                    cf(emu, "_CFSetRemoveValue", &[s, o])?;
                }
            }
        }
        _ => {
            // setSet:
            cf(emu, "_CFSetRemoveAllValues", &[s])?;
            for o in elements_of(emu, other)? {
                cf(emu, "_CFSetAddValue", &[s, o])?;
            }
        }
    }
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

fn number_unsigned_char(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_SINT32, emu.arg(2) as u8 as u64)
}

fn number_unsigned_short(emu: &mut Emu) -> Result<()> {
    ret_number(emu, NUMBER_SINT32, emu.arg(2) as u16 as u64)
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
pub(crate) fn number_value(emu: &mut Emu, n: u64) -> Result<(i64, f64)> {
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

/// `sortUsingFunction:context:` / `sortedArrayUsingFunction:context:`: a
/// stable sort by function(a, b, context) → NSComparisonResult.
fn sort_with_function(emu: &mut Emu, in_place: bool) -> Result<()> {
    let (a, f, ctx) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let objs = array_values(emu, a)?;
    let sorted = merge_sort(emu, objs, &mut |e, x, y| Ok(e.call(f, &[x, y, ctx])? as i64))?;
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
pub(crate) fn new_enumerator(emu: &mut Emu, objs: &[u64]) -> Result<u64> {
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

/// `+[NSURL fileURLWithPathComponents:]`: the components joined as a path (nil
/// for none); a last component ending in "/" makes a directory URL.
fn file_url_with_path_components(emu: &mut Emu) -> Result<()> {
    let a = emu.arg(2);
    let mut parts = Vec::new();
    if a != 0 {
        for c in array_values(emu, a)? {
            parts.push(nsstring(emu, c)?);
        }
    }
    if parts.is_empty() {
        emu.ret(0);
        return Ok(());
    }
    let dir = parts.last().is_some_and(|p| p.ends_with('/'));
    let u = new_file_url(emu, &join_path_components(&parts), dir)?;
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

/// An NSURL accessor that's a CFURL function of the URL alone: `f(url)`.
fn url_cf_copy(emu: &mut Emu, f: &'static str) -> Result<()> {
    let s = cf(emu, f, &[emu.arg(0)])?;
    emu.ret(s);
    Ok(())
}

/// `query`: percent-encoded, as Foundation gives it (nil without one).
fn url_query(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFURLCopyQueryString", &[emu.arg(0), 0])?;
    emu.ret(s);
    Ok(())
}

/// `fragment`: percent-encoded (nil without one).
fn url_fragment(emu: &mut Emu) -> Result<()> {
    let s = cf(emu, "_CFURLCopyFragment", &[emu.arg(0), 0])?;
    emu.ret(s);
    Ok(())
}

/// `port`: an NSNumber, or nil when the URL names none.
fn url_port(emu: &mut Emu) -> Result<()> {
    let p = cf(emu, "_CFURLGetPortNumber", &[emu.arg(0)])? as u32 as i32;
    let n = if p < 0 { 0 } else { new_number_i64(emu, p as i64)? };
    emu.ret(n);
    Ok(())
}

/// `pathComponents`: "/" first for an absolute path, then each name.
fn url_path_components(emu: &mut Emu) -> Result<()> {
    let p = cf(emu, "_CFURLCopyFileSystemPath", &[emu.arg(0), POSIX_PATH_STYLE])?;
    if p == 0 {
        emu.ret(0);
        return Ok(());
    }
    let path = nsstring(emu, p)?;
    let mut parts: Vec<&str> = Vec::new();
    if path.starts_with('/') {
        parts.push("/");
    }
    parts.extend(path.split('/').filter(|s| !s.is_empty()));
    let mut objs = Vec::new();
    for part in parts {
        objs.push(new_nsstring(emu, part)?);
    }
    let a = new_array_of(emu, &objs)?;
    emu.ret(a);
    Ok(())
}

fn url_appending_extension(emu: &mut Emu) -> Result<()> {
    let ext = as_cf_string(emu, emu.arg(2))?;
    let u = cf(emu, "_CFURLCreateCopyAppendingPathExtension", &[0, emu.arg(0), ext])?;
    emu.ret(u);
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
    url_appending(emu, 0)
}

/// `URLByAppendingPathComponent:isDirectory:`: a directory's URL ends in a slash.
fn url_appending_component_directory(emu: &mut Emu) -> Result<()> {
    let dir = emu.arg(3) & 0xff;
    url_appending(emu, dir)
}

fn url_appending(emu: &mut Emu, is_directory: u64) -> Result<()> {
    let c = as_cf_string(emu, emu.arg(2))?;
    let u = cf(emu, "_CFURLCreateCopyAppendingPathComponent", &[0, emu.arg(0), c, is_directory])?;
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
    crate::threads::now_ns(emu) as f64 / 1e9 + crate::libsystem::boot_time(emu) as f64 - SECONDS_1970_TO_2001
}

pub fn new_date(emu: &mut Emu, abs: f64) -> Result<u64> {
    emu.cpu.q[0] = abs.to_bits() as u128;
    cf(emu, "_CFDateCreate", &[0])
}

/// A date's seconds since 2001; nil's aren't a number
/// (`timeIntervalSinceDate:nil` is NaN: tests/fixtures/devmisc).
pub(crate) fn date_value(emu: &mut Emu, d: u64) -> Result<f64> {
    if d == 0 {
        return Ok(f64::NAN);
    }
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
/// `-[NSRunLoop runMode:beforeDate:]`: CFRunLoopRunInMode(mode, until the
/// date, return after a source), YES unless the mode had nothing to run
/// (kCFRunLoopRunFinished). It's a tail call into CF, not a call: a thread
/// that then waits in its run loop (SocketRocket's network thread waits
/// forever) waits in guest code, where pomme can switch threads, rather than
/// under this host frame, which would hold every other thread's turn,
/// the main thread's included. CF returns to [`run_mode_result`].
fn run_loop_run_mode(emu: &mut Emu) -> Result<()> {
    let (mode, date) = (emu.arg(2), emu.arg(3));
    let secs = if date == 0 { 0.0 } else { (date_value(emu, date)? - absolute_now(emu)).max(0.0) };
    let f = cf_sym(emu, "_CFRunLoopRunInMode")?;
    let back = emu.host_thunk(run_mode_result)?;
    let caller = emu.cpu.x[30];
    emu.fnd.run_mode_returns.entry(emu.th.current).or_default().push(caller);
    emu.cpu.x[0] = mode;
    emu.cpu.x[1] = 1;
    emu.cpu.q[0] = secs.to_bits() as u128;
    emu.cpu.x[30] = back;
    emu.cpu.pc = f;
    Ok(())
}

/// Where CFRunLoopRunInMode returns for `runMode:beforeDate:`: its result as
/// the method's (NO for kCFRunLoopRunFinished), back to the method's caller.
fn run_mode_result(emu: &mut Emu) -> Result<()> {
    let r = emu.cpu.x[0] as u32;
    let caller = emu.fnd.run_mode_returns.get_mut(&emu.th.current).and_then(Vec::pop).ok_or_else(|| Stop::Unsupported("runMode:beforeDate: returned with no caller".into()))?;
    emu.cpu.x[0] = (r != 1) as u64;
    emu.cpu.pc = caller;
    Ok(())
}

/// `-[NSRunLoop run]`: CFRunLoopRun(), tail-called (see run_loop_run_mode).
fn run_loop_run(emu: &mut Emu) -> Result<()> {
    emu.cpu.pc = cf_sym(emu, "_CFRunLoopRun")?;
    Ok(())
}

/// `-[NSRunLoop runUntilDate:]`: CFRunLoopRunInMode(default, until the date), tail-called.
fn run_loop_run_until(emu: &mut Emu) -> Result<()> {
    let date = emu.arg(2);
    let mode = cf_data(emu, "_kCFRunLoopDefaultMode")?;
    let secs = (date_value(emu, date)? - absolute_now(emu)).max(0.0);
    emu.cpu.x[0] = mode;
    emu.cpu.x[1] = 0;
    emu.cpu.q[0] = secs.to_bits() as u128;
    emu.cpu.pc = cf_sym(emu, "_CFRunLoopRunInMode")?;
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

/// Post the notification named `name` (constants are their own names) from `object`.
pub fn post_named(emu: &mut Emu, name: &str, object: u64) -> Result<()> {
    post_named_info(emu, name, object, 0)
}

/// Post the notification named `name` with a userInfo dictionary.
pub fn post_named_info(emu: &mut Emu, name: &str, object: u64, info: u64) -> Result<()> {
    let name = new_nsstring(emu, name)?;
    let n = new_notification(emu, name, object, info)?;
    deliver(emu, n)
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

// ---- base64 (RFC 4648) ----

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// NSDataBase64DecodingIgnoreUnknownCharacters.
const IGNORE_UNKNOWN: u64 = 1;

/// Decode base64 as NSData does: padding optional, whitespace and other
/// characters refused unless `options` says to ignore them.
pub(crate) fn base64_decode(text: &[u8], options: u64) -> Option<Vec<u8>> {
    let mut bits = 0u32;
    let mut n = 0;
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut padding = 0;
    for &c in text {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                padding += 1;
                continue;
            }
            _ if options & IGNORE_UNKNOWN != 0 => continue,
            _ => return None,
        };
        if padding > 0 {
            return None; // data after the padding
        }
        bits = bits << 6 | v as u32;
        n += 6;
        if n >= 8 {
            n -= 8;
            out.push((bits >> n) as u8);
        }
    }
    // Leftover bits must be zero padding, and a lone sextet can't be a byte.
    if n >= 6 || padding > 2 {
        return None;
    }
    Some(out)
}

/// Encode as NSData does: no line breaks unless `options` asks for 64- or
/// 76-character lines (ended CRLF unless just CR or LF is asked for).
pub(crate) fn base64_encode(bytes: &[u8], options: u64) -> String {
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let v = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        for i in 0..4 {
            if i <= chunk.len() {
                s.push(BASE64[(v >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    let line = if options & 1 != 0 { 64 } else if options & 2 != 0 { 76 } else { 0 };
    if line == 0 {
        return s;
    }
    let end = match (options & 16 != 0, options & 32 != 0) {
        (true, false) => "\r",
        (false, true) => "\n",
        _ => "\r\n",
    };
    let lines: Vec<&str> = s.as_bytes().chunks(line).map(|c| std::str::from_utf8(c).unwrap()).collect();
    lines.join(end)
}

/// `-rangeOfData:options:range:`: where `needle` occurs in the range
/// (NSDataSearchBackwards: the last place; NSDataSearchAnchored: only at the
/// range's start, or its end when backwards). {NSNotFound, 0} if nowhere.
fn data_range_of(emu: &mut Emu) -> Result<()> {
    const BACKWARDS: u64 = 1;
    const ANCHORED: u64 = 2;
    let (d, needle, options, loc, len) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    let hay = data_bytes(emu, d)?;
    let pat = data_bytes(emu, needle)?;
    let end = (loc + len).min(hay.len() as u64) as usize;
    let start = (loc as usize).min(end);
    let window = &hay[start..end];
    let found = if pat.is_empty() || pat.len() > window.len() {
        None
    } else {
        let last = window.len() - pat.len();
        let at = |i: usize| window[i..i + pat.len()] == pat[..];
        match (options & BACKWARDS != 0, options & ANCHORED != 0) {
            (false, false) => (0..=last).find(|&i| at(i)),
            (true, false) => (0..=last).rev().find(|&i| at(i)),
            (false, true) => at(0).then_some(0),
            (true, true) => at(last).then_some(last),
        }
    };
    let (l, n) = match found {
        Some(i) => ((start + i) as u64, pat.len() as u64),
        None => (i64::MAX as u64, 0),
    };
    emu.cpu.x[0] = l;
    emu.cpu.x[1] = n;
    Ok(())
}

/// Write the receiver's bytes to `path` (a file in the data container).
/// NSDataWritingWithoutOverwriting (2) refuses an existing file. On failure,
/// `*error` (if given) gets NSCocoaErrorDomain's NSFileWriteUnknownError (512).
fn data_write(emu: &mut Emu, path: &str, options: u64, error: u64) -> Result<()> {
    let bytes = data_bytes(emu, emu.arg(0))?;
    write_file(emu, path, &bytes, options, error)
}

/// Write `bytes` to `path` for NSData and NSString's writeTo… methods.
fn write_file(emu: &mut Emu, path: &str, bytes: &[u8], options: u64, error: u64) -> Result<()> {
    const WITHOUT_OVERWRITING: u64 = 2;
    let exists = crate::fs::exists(emu, path).is_some();
    let ok = !(options & WITHOUT_OVERWRITING != 0 && exists) && crate::fs::write_data_file(emu, path, bytes);
    if !ok && error != 0 {
        let domain = new_nsstring(emu, "NSCocoaErrorDomain")?;
        let cls = host_class(emu, "NSError")?;
        let code = if exists && options & WITHOUT_OVERWRITING != 0 { 516 } else { 512 }; // NSFileWriteFileExistsError
        let err = send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, code, 0])?;
        emu.write_u64(error, err)?;
    }
    ret_bool(emu, ok)
}

/// `-writeToFile:atomically:encoding:error:` (and the URL form): the string
/// in that encoding. One that can't hold every character fails
/// (NSFileWriteInapplicableStringEncodingError, 517).
fn string_write(emu: &mut Emu, path: &str, enc: u64, error: u64) -> Result<()> {
    let s = emu.arg(0);
    match encode(emu, s, enc, 0)? {
        Some(bytes) => write_file(emu, path, &bytes, 0, error),
        None => {
            if error != 0 {
                let domain = new_nsstring(emu, "NSCocoaErrorDomain")?;
                let cls = host_class(emu, "NSError")?;
                let err = send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, 517, 0])?;
                emu.write_u64(error, err)?;
            }
            ret_bool(emu, false)
        }
    }
}

pub(crate) fn url_file_path(emu: &mut Emu, url: u64) -> Result<String> {
    let p = send(emu, url, "path", &[])?;
    nsstring(emu, p)
}

fn data_from_base64(emu: &mut Emu, text: Vec<u8>) -> Result<()> {
    let options = emu.arg(3);
    match base64_decode(&text, options) {
        Some(bytes) => ret_data(emu, &bytes),
        None => {
            emu.ret(0);
            Ok(())
        }
    }
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
    if subclass_new_or_init(emu)?.is_some() {
        return Ok(());
    }
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

/// `-getBytes:range:` (NSRange in x3, x4): out of the data's bounds, NSRangeException.
fn data_get_bytes_range(emu: &mut Emu) -> Result<()> {
    let (d, buf, loc, len) = (emu.arg(0), emu.arg(2), emu.arg(3) as usize, emu.arg(4) as usize);
    let bytes = data_bytes(emu, d)?;
    if loc.checked_add(len).is_none_or(|end| end > bytes.len()) {
        return Err(Stop::Unsupported(format!("NSRangeException: -[NSData getBytes:range:]: range {{{loc}, {len}}} exceeds data length {}", bytes.len())));
    }
    emu.p.memory.write(buf, &bytes[loc..loc + len])?;
    Ok(())
}

/// `-getBytes:` (all of them).
fn data_get_all_bytes(emu: &mut Emu) -> Result<()> {
    let (d, buf) = (emu.arg(0), emu.arg(2));
    let bytes = data_bytes(emu, d)?;
    emu.p.memory.write(buf, &bytes)?;
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

/// The string's bytes in an NSStringEncoding, through CoreFoundation's
/// converters: None if a character has no equivalent and `loss` is 0
/// (otherwise it stands in for them). NSUnicodeStringEncoding and
/// NSUTF32StringEncoding get a byte-order mark, as Foundation writes them.
fn encode(emu: &mut Emu, s: u64, enc: u64, loss: u8) -> Result<Option<Vec<u8>>> {
    // A CFString for CF to read directly (other NSStrings would come back to pomme's getBytes:).
    let s = as_cf_string(emu, s)?;
    let len = cf(emu, "_CFStringGetLength", &[s])?;
    let f = cf_sym(emu, "_CFStringGetBytes")?;
    let used = emu.alloc(8)?;
    emu.write_u64(used, 0)?;
    // CFStringGetBytes(str, range, encoding, lossByte, isExternalRepresentation, buffer, maxBufLen, usedBufLen)
    let external = writes_bom(enc) as u64;
    let cf_enc = cf_encoding(emu, enc)?;
    emu.call_abi(f, &[s, 0, len, cf_enc, loss as u64, external, 0, 0], &[], &[used], None)?;
    let converted = emu.last_x01[0];
    let n = emu.read_u64(used)?;
    if converted < len {
        emu.heap.free(&emu.p.memory, used)?;
        return Ok(None);
    }
    let buf = emu.alloc(n.max(1))?;
    emu.call_abi(f, &[s, 0, len, cf_enc, loss as u64, external, buf, n], &[], &[used], None)?;
    let bytes = emu.p.memory.read(buf, n as usize)?.to_vec();
    emu.heap.free(&emu.p.memory, buf)?;
    emu.heap.free(&emu.p.memory, used)?;
    Ok(Some(bytes))
}

/// `-dataUsingEncoding:` / `-dataUsingEncoding:allowLossyConversion:`.
fn string_data_using_encoding(emu: &mut Emu) -> Result<()> {
    let (s, enc) = (emu.arg(0), emu.arg(2));
    let lossy = emu.arg(3) & 0xff != 0 && objc::sel_name(emu, emu.arg(1)).ends_with("allowLossyConversion:");
    let d = match encode(emu, s, enc, if lossy { b'?' } else { 0 })? {
        Some(bytes) => new_data(emu, &bytes)?,
        None => 0,
    };
    emu.ret(d);
    Ok(())
}

/// `-canBeConvertedToEncoding:`.
fn can_be_converted(emu: &mut Emu) -> Result<()> {
    let (s, enc) = (emu.arg(0), emu.arg(2));
    let ok = encode(emu, s, enc, 0)?.is_some();
    emu.ret(ok as u64);
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
    ret_made_string(emu, &s)
}

/// `+stringWithContentsOfFile:encoding:error:`
fn string_with_contents_of_file(emu: &mut Emu) -> Result<()> {
    let path = nsstring(emu, emu.arg(2))?;
    string_from_file(emu, &path)
}

/// `+stringWithContentsOfURL:encoding:error:` (file URLs).
fn string_with_contents_of_url(emu: &mut Emu) -> Result<()> {
    let p = cf(emu, "_CFURLCopyFileSystemPath", &[emu.arg(2), POSIX_PATH_STYLE])?;
    let path = if p == 0 { String::new() } else { nsstring(emu, p)? };
    string_from_file(emu, &path)
}

/// The file's text in the encoding (x3); a missing file is nil with
/// NSFileReadNoSuchFileError (260) in `*error` (x4).
fn string_from_file(emu: &mut Emu, path: &str) -> Result<()> {
    let (enc, error) = (emu.arg(3), emu.arg(4));
    match crate::fs::read_file(emu, path) {
        Some(bytes) => string_from_bytes(emu, &bytes, enc),
        None => {
            if error != 0 {
                let domain = new_nsstring(emu, "NSCocoaErrorDomain")?;
                let (k, v) = (new_nsstring(emu, "NSFilePath")?, new_nsstring(emu, path)?);
                let info = new_dict_of(emu, &[(k, v)])?;
                let cls = host_class(emu, "NSError")?;
                let err = send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, 260, info])?;
                emu.write_u64(error, err)?;
            }
            emu.ret(0);
            Ok(())
        }
    }
}

// ---- NSFileManager and standard directories ----

pub(crate) const DATA_CONTAINER: &str = "/private/var/mobile/Containers/Data/Application/pomme";

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
    let found = crate::fs::exists(emu, &path);
    if let Some(dir) = found
        && is_dir_out != 0
        && objc::sel_name(emu, emu.arg(1)).contains("isDirectory")
    {
        emu.p.memory.write_u8(is_dir_out, dir as u8)?;
    }
    ret_bool(emu, found.is_some())
}

/// File URLs for a directory's entries, depth first when `recursive`; nil if it isn't one.
fn dir_urls(emu: &mut Emu, path: &str, recursive: bool) -> Result<u64> {
    fn walk(emu: &mut Emu, path: &str, recursive: bool, out: &mut Vec<u64>) -> Result<bool> {
        let Some(entries) = crate::fs::list_dir(emu, path) else { return Ok(false) };
        for (name, dir) in entries {
            let child = format!("{}/{name}", path.trim_end_matches('/'));
            out.push(new_file_url(emu, &child, dir)?);
            if dir && recursive {
                walk(emu, &child, true, out)?;
            }
        }
        Ok(true)
    }
    let mut urls = Vec::new();
    if !walk(emu, path, recursive, &mut urls)? {
        return Ok(0);
    }
    new_array_of(emu, &urls)
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

/// The cache's dictionary, in its second word (made on first use).
fn cache_store(emu: &mut Emu, c: u64) -> Result<u64> {
    let s = emu.read_u64(c + 8)?;
    if s != 0 {
        return Ok(s);
    }
    let s = new_dict(emu)?;
    emu.write_u64(c + 8, s)?;
    Ok(s)
}

fn cache_get(emu: &mut Emu) -> Result<()> {
    let (c, key) = (emu.arg(0), emu.arg(2));
    let store = cache_store(emu, c)?;
    let v = if key == 0 { 0 } else { cf(emu, "_CFDictionaryGetValue", &[store, key])? };
    emu.ret(v);
    Ok(())
}

fn cache_set(emu: &mut Emu) -> Result<()> {
    let (c, v, key) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let store = cache_store(emu, c)?;
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
    let store = cache_store(emu, c)?;
    cf(emu, "_CFDictionaryRemoveValue", &[store, key])?;
    Ok(())
}

fn cache_remove_all(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0);
    let store = cache_store(emu, c)?;
    cf(emu, "_CFDictionaryRemoveAllValues", &[store])?;
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

/// The description from the user info, else one made from the domain and code.
/// Read through `userInfo`, `domain` and `code`, which subclasses override
/// (the Swift runtime's `__SwiftNativeNSError` keeps a Swift error instead of these fields).
fn error_localized_description(emu: &mut Emu) -> Result<()> {
    let e = emu.arg(0);
    let info = send(emu, e, "userInfo", &[])?;
    if info != 0 {
        let key = new_nsstring(emu, "NSLocalizedDescription")?;
        let v = dict_get_any(emu, info, key)?;
        if v != 0 {
            emu.ret(v);
            return Ok(());
        }
    }
    let d = send(emu, e, "domain", &[])?;
    let domain = if d == 0 { String::new() } else { nsstring(emu, d)? };
    let code = send(emu, e, "code", &[])? as i64;
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

/// `NSProcessInfo.arguments`: the executable's path, then the launch arguments.
fn process_arguments(emu: &mut Emu) -> Result<()> {
    let exe = format!("{}/{}", crate::fs::guest_bundle_path(emu), emu.p.main().name);
    let mut items = vec![new_nsstring(emu, &exe)?];
    for a in emu.launch_args.clone() {
        items.push(new_nsstring(emu, &a)?);
    }
    let a = array_from(emu, &items)?;
    emu.ret(a);
    Ok(())
}

/// `NSProcessInfo.environment`: the process's environment variables.
fn process_environment(emu: &mut Emu) -> Result<()> {
    let d = new_dict(emu)?;
    for (k, v) in crate::libsystem::environment(emu) {
        let (k, v) = (new_nsstring(emu, &k)?, new_nsstring(emu, &v)?);
        cf(emu, "_CFDictionarySetValue", &[d, k, v])?;
    }
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
    // The limits are kept and read back; pomme's cache never evicts (on iOS
    // eviction is the system's choice, at no fixed time).
    ("NSCache", false, "countLimit", |e| {
        let v = crate::objc::bag_get(e, e.arg(0), "countLimit").unwrap_or(0);
        e.ret(v);
        Ok(())
    }),
    ("NSCache", false, "setCountLimit:", |e| {
        let (c, v) = (e.arg(0), e.arg(2));
        crate::objc::bag_set(e, c, "countLimit", v);
        Ok(())
    }),
    ("NSCache", false, "totalCostLimit", |e| {
        let v = crate::objc::bag_get(e, e.arg(0), "totalCostLimit").unwrap_or(0);
        e.ret(v);
        Ok(())
    }),
    ("NSCache", false, "setTotalCostLimit:", |e| {
        let (c, v) = (e.arg(0), e.arg(2));
        crate::objc::bag_set(e, c, "totalCostLimit", v);
        Ok(())
    }),
    ("NSCache", false, "setObject:forKey:", cache_set),
    ("NSCache", false, "setObject:forKey:cost:", cache_set),
    ("NSCache", false, "removeObjectForKey:", cache_remove),
    ("NSCache", false, "removeAllObjects", cache_remove_all),
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
    ("NSData", false, "rangeOfData:options:range:", data_range_of),
    ("NSString", false, "writeToFile:atomically:encoding:error:", |e| {
        let (path, enc, error) = (nsstring(e, e.arg(2))?, e.arg(4), e.arg(5));
        string_write(e, &path, enc, error)
    }),
    ("NSString", false, "writeToURL:atomically:encoding:error:", |e| {
        let (enc, error) = (e.arg(4), e.arg(5));
        let path = url_file_path(e, e.arg(2))?;
        string_write(e, &path, enc, error)
    }),
    ("NSData", false, "writeToFile:atomically:", |e| {
        let path = nsstring(e, e.arg(2))?;
        data_write(e, &path, 0, 0)
    }),
    ("NSData", false, "writeToURL:atomically:", |e| {
        let path = url_file_path(e, e.arg(2))?;
        data_write(e, &path, 0, 0)
    }),
    ("NSData", false, "writeToFile:options:error:", |e| {
        let (path, options, error) = (nsstring(e, e.arg(2))?, e.arg(3), e.arg(4));
        data_write(e, &path, options, error)
    }),
    ("NSData", false, "writeToURL:options:error:", |e| {
        let (options, error) = (e.arg(3), e.arg(4));
        let path = url_file_path(e, e.arg(2))?;
        data_write(e, &path, options, error)
    }),
    ("NSData", false, "initWithBase64EncodedString:options:", |e| {
        let text = if e.arg(2) == 0 { return ret_nil(e) } else { nsstring(e, e.arg(2))?.into_bytes() };
        data_from_base64(e, text)
    }),
    ("NSData", false, "initWithBase64EncodedData:options:", |e| {
        let text = data_bytes(e, e.arg(2))?;
        data_from_base64(e, text)
    }),
    ("NSData", false, "base64EncodedStringWithOptions:", |e| {
        let (d, options) = (e.arg(0), e.arg(2));
        let bytes = data_bytes(e, d)?;
        ret_string(e, &base64_encode(&bytes, options))
    }),
    ("NSData", false, "base64EncodedDataWithOptions:", |e| {
        let (d, options) = (e.arg(0), e.arg(2));
        let bytes = data_bytes(e, d)?;
        let out = new_data(e, base64_encode(&bytes, options).as_bytes())?;
        e.ret(out);
        Ok(())
    }),
    ("NSData", false, "bytes", data_bytes_ptr),
    ("NSData", false, "getBytes:length:", data_get_bytes),
    ("NSData", false, "getBytes:range:", data_get_bytes_range),
    ("NSData", false, "getBytes:", data_get_all_bytes),
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
    ("NSString", false, "canBeConvertedToEncoding:", can_be_converted),
    ("NSString", false, "dataUsingEncoding:allowLossyConversion:", string_data_using_encoding),
    ("NSString", false, "initWithData:encoding:", string_with_data),
    ("NSString", false, "initWithBytes:length:encoding:", string_with_bytes),
    // The bytes are copied, so a buffer handed over (freeWhenDone) is freed now.
    ("NSString", false, "initWithBytesNoCopy:length:encoding:freeWhenDone:", |e| {
        let (bytes, free) = (e.arg(2), e.arg(5) & 0xff != 0);
        string_with_bytes(e)?;
        let s = e.cpu.x[0];
        if free && bytes != 0 {
            e.heap.free(&e.p.memory, bytes)?;
        }
        e.ret(s);
        Ok(())
    }),
    ("NSString", true, "stringWithContentsOfFile:encoding:error:", string_with_contents_of_file),
    ("NSString", false, "initWithContentsOfFile:encoding:error:", string_with_contents_of_file),
    ("NSString", true, "stringWithContentsOfURL:encoding:error:", string_with_contents_of_url),
    ("NSString", false, "initWithContentsOfURL:encoding:error:", string_with_contents_of_url),
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
    ("NSFileManager", false, "createDirectoryAtPath:withIntermediateDirectories:attributes:error:", |e| {
        let (path, inter) = (nsstring(e, e.arg(2))?, e.arg(3) & 0xff != 0);
        let ok = crate::fs::make_dirs(e, &path, inter) || crate::fs::exists(e, &path) == Some(true);
        ret_bool(e, ok)
    }),
    ("NSFileManager", false, "createDirectoryAtURL:withIntermediateDirectories:attributes:error:", |e| {
        let (url, inter) = (e.arg(2), e.arg(3) & 0xff != 0);
        let p = send(e, url, "path", &[])?;
        let path = nsstring(e, p)?;
        let ok = crate::fs::make_dirs(e, &path, inter) || crate::fs::exists(e, &path) == Some(true);
        ret_bool(e, ok)
    }),
    ("NSFileManager", false, "contentsOfDirectoryAtPath:error:", |e| {
        let path = nsstring(e, e.arg(2))?;
        let Some(entries) = crate::fs::list_dir(e, &path) else {
            e.ret(0);
            return Ok(());
        };
        let mut names = Vec::new();
        for (n, _) in entries {
            names.push(new_nsstring(e, &n)?);
        }
        let a = new_array_of(e, &names)?;
        e.ret(a);
        Ok(())
    }),
    ("NSFileManager", false, "contentsOfDirectoryAtURL:includingPropertiesForKeys:options:error:", |e| {
        let p = send(e, e.arg(2), "path", &[])?;
        let path = nsstring(e, p)?;
        let urls = dir_urls(e, &path, false)?;
        e.ret(urls);
        Ok(())
    }),
    ("NSFileManager", false, "enumeratorAtURL:includingPropertiesForKeys:options:errorHandler:", |e| {
        // NSDirectoryEnumerationSkipsSubdirectoryDescendants = 1.
        let (url, options) = (e.arg(2), e.arg(4));
        let p = send(e, url, "path", &[])?;
        let path = nsstring(e, p)?;
        let urls = dir_urls(e, &path, options & 1 == 0)?;
        let en = send(e, urls, "objectEnumerator", &[])?;
        e.ret(en);
        Ok(())
    }),
    ("NSFileManager", false, "createFileAtPath:contents:attributes:", |e| {
        let (path, data) = (nsstring(e, e.arg(2))?, e.arg(3));
        let bytes = if data == 0 { Vec::new() } else { data_bytes(e, data)? };
        let ok = crate::fs::write_data_file(e, &path, &bytes);
        ret_bool(e, ok)
    }),
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
    ("NSString", false, "stringByPaddingToLength:withString:startingAtIndex:", string_padding),
    ("NSString", true, "pathWithComponents:", |e| {
        let a = e.arg(2);
        let mut parts = Vec::new();
        for c in array_values(e, a)? {
            parts.push(nsstring(e, c)?);
        }
        ret_string(e, &join_path_components(&parts))
    }),
    ("NSString", false, "stringByAbbreviatingWithTildeInPath", |e| {
        let s = nsstring(e, e.arg(0))?;
        let home = DATA_CONTAINER;
        let r = if s == home { "~".to_string() } else { s.strip_prefix(&format!("{home}/")).map_or(s.clone(), |rest| format!("~/{rest}")) };
        ret_string(e, &r)
    }),
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
    // Equal dates are the same instant; the hash is the whole second (as Foundation's).
    ("NSDate", false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let date_cls = host_class(e, "NSDate")?;
        let same = b != 0 && objc::is_kind_of(e, b, date_cls)? && date_value(e, a)? == date_value(e, b)?;
        ret_bool(e, same)
    }),
    ("NSDate", false, "hash", |e| {
        let t = date_value(e, e.arg(0))?;
        e.ret(t.floor() as i64 as u64);
        Ok(())
    }),
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
    ("NSURL", true, "fileURLWithPathComponents:", file_url_with_path_components),
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
    // The file path as a C string (lives as long as the URL would autorelease it: pomme keeps it).
    ("NSURL", false, "fileSystemRepresentation", |e| {
        let path = url_file_path(e, e.arg(0))?;
        let p = e.alloc_cstr(&path)?;
        e.ret(p);
        Ok(())
    }),
    ("NSURL", false, "scheme", url_scheme),
    ("NSURL", false, "host", url_host),
    ("NSURL", false, "lastPathComponent", url_last_path_component),
    ("NSURL", false, "pathExtension", url_path_extension),
    ("NSURL", false, "isFileURL", url_is_file),
    ("NSURL", false, "URLByAppendingPathComponent:", url_appending_component),
    ("NSURL", false, "URLByAppendingPathComponent:isDirectory:", url_appending_component_directory),
    ("NSURL", false, "URLByDeletingLastPathComponent", url_deleting_last_component),
    ("NSURL", false, "URLByAppendingPathExtension:", url_appending_extension),
    ("NSURL", false, "URLByDeletingPathExtension", |e| {
        let u = cf(e, "_CFURLCreateCopyDeletingPathExtension", &[0, e.arg(0)])?;
        e.ret(u);
        Ok(())
    }),
    ("NSURL", false, "query", url_query),
    ("NSURL", false, "fragment", url_fragment),
    ("NSURL", false, "port", url_port),
    ("NSURL", false, "user", |e| url_cf_copy(e, "_CFURLCopyUserName")),
    ("NSURL", false, "password", |e| url_cf_copy(e, "_CFURLCopyPassword")),
    ("NSURL", false, "relativeString", |e| {
        let s = cf(e, "_CFURLGetString", &[e.arg(0)])?;
        e.ret(s);
        Ok(())
    }),
    ("NSURL", false, "baseURL", |e| {
        let s = cf(e, "_CFURLGetBaseURL", &[e.arg(0)])?;
        e.ret(s);
        Ok(())
    }),
    ("NSURL", false, "pathComponents", url_path_components),
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
    ("NSDictionary", false, "isEqualToDictionary:", dict_is_equal),
    ("NSDictionary", false, "isEqual:", dict_is_equal),
    ("NSDictionary", false, "hash", hash_is_count),
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
    ("NSMutableArray", false, "sortUsingFunction:context:", |e| sort_with_function(e, true)),
    ("NSArray", false, "sortedArrayUsingFunction:context:", |e| sort_with_function(e, false)),
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
    ("NSString", true, "stringWithCharacters:length:", string_with_characters),
    ("NSString", false, "initWithCharacters:length:", string_with_characters),
    ("NSString", true, "stringWithString:", string_with_string),
    ("NSString", false, "initWithString:", string_with_string),
    ("NSString", true, "stringWithFormat:", string_with_format),
    ("NSString", false, "initWithFormat:", string_with_format),
    ("NSString", false, "initWithFormat:arguments:", string_with_format_va),
    ("NSString", true, "localizedStringWithFormat:", string_with_format_locale),
    ("NSString", false, "initWithFormat:locale:", string_with_format_locale),
    ("NSString", false, "initWithFormat:locale:arguments:", string_with_format_locale_va),
    ("NSString", false, "length", string_length),
    ("NSString", false, "UTF8String", utf8_string),
    // A path's bytes for the file system: UTF-8 (pomme's file system keeps names as given).
    ("NSString", false, "fileSystemRepresentation", utf8_string),
    ("NSString", false, "getFileSystemRepresentation:maxLength:", |e| {
        let (s, buf, max) = (e.arg(0), e.arg(2), e.arg(3));
        let text = nsstring(e, s)?;
        if text.len() as u64 + 1 > max {
            return ret_bool(e, false);
        }
        e.p.memory.write(buf, text.as_bytes())?;
        e.p.memory.write_u8(buf + text.len() as u64, 0)?;
        ret_bool(e, true)
    }),
    ("NSString", false, "characterAtIndex:", character_at_index),
    ("NSString", false, "isEqualToString:", is_equal_to_string),
    ("NSString", false, "isEqual:", string_is_equal),
    ("NSString", false, "hash", string_hash),
    ("NSString", false, "copy", string_copy),
    ("NSString", false, "copyWithZone:", string_copy),
    ("NSString", false, "mutableCopy", string_mutable_copy),
    ("NSString", false, "description", ret_self),
    ("NSString", false, "stringByAppendingString:", string_by_appending),
    ("NSString", false, "stringByAppendingFormat:", string_by_appending_format),
    ("NSString", false, "hasPrefix:", has_prefix),
    ("NSString", false, "hasSuffix:", has_suffix),
    ("NSString", false, "lowercaseString", lowercase),
    ("NSString", false, "uppercaseString", uppercase),
    ("NSString", false, "compare:", string_compare),
    ("NSString", false, "compare:options:", |e| {
        let o = e.arg(3);
        compare_with(e, o, None, 0)
    }),
    ("NSString", false, "compare:options:range:", |e| {
        let (o, loc, len) = (e.arg(3), e.arg(4), e.arg(5));
        compare_with(e, o, Some((loc, len)), 0)
    }),
    ("NSString", false, "compare:options:range:locale:", |e| {
        let (o, loc, len, l) = (e.arg(3), e.arg(4), e.arg(5), e.arg(6));
        compare_with(e, o | if l != 0 { COMPARE_LOCALIZED } else { 0 }, Some((loc, len)), l)
    }),
    ("NSString", false, "caseInsensitiveCompare:", |e| compare_with(e, COMPARE_CASE_INSENSITIVE, None, 0)),
    ("NSString", false, "localizedCompare:", |e| compare_with(e, COMPARE_LOCALIZED, None, 0)),
    ("NSString", false, "localizedCaseInsensitiveCompare:", |e| compare_with(e, COMPARE_LOCALIZED | COMPARE_CASE_INSENSITIVE, None, 0)),
    // Finder's order: case- and width-insensitive, numbers by value.
    ("NSString", false, "localizedStandardCompare:", |e| {
        compare_with(e, COMPARE_LOCALIZED | COMPARE_CASE_INSENSITIVE | COMPARE_NUMERIC | COMPARE_WIDTH_INSENSITIVE, None, 0)
    }),
    ("NSString", false, "intValue", integer_value),
    ("NSString", false, "integerValue", integer_value),
    ("NSString", false, "longLongValue", integer_value),
    ("NSString", false, "doubleValue", double_value),
    ("NSString", false, "boolValue", bool_value_string),
    ("NSString", false, "componentsSeparatedByString:", components_separated),
    ("NSString", false, "substringFromIndex:", substring_from),
    ("NSString", false, "substringToIndex:", substring_to),
    ("NSString", false, "rangeOfString:", range_of_string),
    ("NSString", false, "rangeOfString:options:", |e| {
        let o = e.arg(3);
        range_of_string_with(e, o, None, 0)
    }),
    ("NSString", false, "rangeOfString:options:range:", |e| {
        let (o, loc, len) = (e.arg(3), e.arg(4), e.arg(5));
        range_of_string_with(e, o, Some((loc, len)), 0)
    }),
    ("NSString", false, "rangeOfString:options:range:locale:", |e| {
        let (o, loc, len, l) = (e.arg(3), e.arg(4), e.arg(5), e.arg(6));
        range_of_string_with(e, o, Some((loc, len)), l)
    }),
    ("NSString", false, "containsString:", contains_string),
    ("NSString", false, "getCString:maxLength:encoding:", get_cstring),
    ("NSString", false, "lengthOfBytesUsingEncoding:", length_of_bytes),
    ("NSString", false, "cStringUsingEncoding:", utf8_string),
    ("NSMutableString", false, "appendString:", append_string),
    ("NSMutableString", false, "appendFormat:", append_format),
    ("NSMutableString", false, "replaceCharactersInRange:withString:", replace_characters),
    ("NSString", false, "rangeOfComposedCharacterSequencesForRange:", range_of_composed),
    ("NSString", false, "rangeOfComposedCharacterSequenceAtIndex:", range_of_composed_at),
    ("NSMutableString", false, "deleteCharactersInRange:", |e| {
        let (m, loc, len) = (e.arg(0), e.arg(2), e.arg(3));
        cf(e, "_CFStringDelete", &[m, loc, len])?;
        Ok(())
    }),
    ("NSMutableString", false, "insertString:atIndex:", |e| {
        let (m, with, at) = (e.arg(0), e.arg(2), e.arg(3));
        let with = as_cf_string(e, with)?;
        cf(e, "_CFStringInsert", &[m, at, with])?;
        Ok(())
    }),
    ("NSMutableString", false, "setString:", |e| {
        let (m, with) = (e.arg(0), e.arg(2));
        let with = as_cf_string(e, with)?;
        cf(e, "_CFStringReplaceAll", &[m, with])?;
        Ok(())
    }),
    // CFStringFindAndReplace takes NSStringCompareOptions' values (case-insensitive, literal, backwards, anchored).
    ("NSMutableString", false, "replaceOccurrencesOfString:withString:options:range:", |e| {
        let (m, find, with, options, loc, len) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4), e.arg(5), e.arg(6));
        let (find, with) = (as_cf_string(e, find)?, as_cf_string(e, with)?);
        let n = cf(e, "_CFStringFindAndReplace", &[m, find, with, loc, len, options])?;
        e.ret(n);
        Ok(())
    }),
    ("NSString", false, "_fastCStringContents:", fast_cstring_contents),
    ("NSString", false, "_fastCharacterContents", fast_character_contents),
    ("NSString", false, "getCharacters:range:", get_characters_range),
    ("NSString", false, "getBytes:maxLength:usedLength:encoding:options:range:remainingRange:", get_bytes),
    ("NSString", false, "compare:options:range:locale:", compare_options_range),
    ("NSString", true, "newTaggedNSStringWithASCIIBytes_:length_:", ret_nil),
    // Foundation's private type checks (CoreFoundation and the Swift runtime
    // ask them): NSObject says NO to each, and each class YES to its own.
    ("NSObject", false, "isNSObject__", ret_yes),
    ("NSObject", false, "isNSString__", ret_no),
    ("NSObject", false, "isNSCFConstantString__", ret_no),
    ("NSObject", false, "isNSArray__", ret_no),
    ("NSObject", false, "isNSDictionary__", ret_no),
    ("NSObject", false, "isNSSet__", ret_no),
    ("NSObject", false, "isNSOrderedSet__", ret_no),
    ("NSObject", false, "isNSNumber__", ret_no),
    ("NSObject", false, "isNSValue__", ret_no),
    ("NSObject", false, "isNSData__", ret_no),
    ("NSObject", false, "isNSDate__", ret_no),
    ("NSObject", false, "isNSTimeZone__", ret_no),
    ("NSObject", false, "isNSAttributedString__", ret_no),
    ("NSString", false, "isNSString__", ret_yes),
    ("__NSCFConstantString", false, "isNSCFConstantString__", ret_yes),
    ("NSArray", false, "isNSArray__", ret_yes),
    ("NSDictionary", false, "isNSDictionary__", ret_yes),
    ("NSSet", false, "isNSSet__", ret_yes),
    ("NSOrderedSet", false, "isNSOrderedSet__", ret_yes),
    ("NSNumber", false, "isNSNumber__", ret_yes),
    ("NSValue", false, "isNSValue__", ret_yes),
    ("NSData", false, "isNSData__", ret_yes),
    ("NSDate", false, "isNSDate__", ret_yes),
    ("NSTimeZone", false, "isNSTimeZone__", ret_yes),
    ("NSAttributedString", false, "isNSAttributedString__", ret_yes),
    // NSArray
    ("NSArray", true, "array", array_empty),
    ("NSArray", true, "new", array_empty),
    ("NSArray", false, "init", array_init),
    ("NSArray", true, "arrayWithObjects:count:", array_with_objects_count),
    ("NSArray", false, "initWithObjects:count:", array_with_objects_count),
    ("NSArray", false, "initWithObjects:", array_with_objects_varargs),
    ("NSArray", true, "arrayWithObject:", array_with_object),
    ("NSArray", true, "arrayWithObjects:", array_with_objects_varargs),
    ("NSArray", false, "isEqualToArray:", array_is_equal),
    ("NSArray", false, "arrayByAddingObject:", |e| {
        let (a, o) = (e.arg(0), e.arg(2));
        let mut v = array_values(e, a)?;
        v.push(o);
        let r = array_from(e, &v)?;
        e.ret(r);
        Ok(())
    }),
    ("NSArray", false, "arrayByAddingObjectsFromArray:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let mut v = array_values(e, a)?;
        if b != 0 {
            v.extend(array_values(e, b)?);
        }
        let r = array_from(e, &v)?;
        e.ret(r);
        Ok(())
    }),
    ("NSDictionary", true, "dictionaryWithObjectsAndKeys:", dict_with_objects_and_keys),
    ("NSArray", false, "isEqual:", array_is_equal),
    ("NSArray", false, "hash", hash_is_count),
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
    // NSRange {location, length} in x2/x3.
    ("NSMutableArray", false, "removeObjectsInRange:", |e| {
        let (a, loc, len) = (e.arg(0), e.arg(2), e.arg(3));
        cf(e, "_CFArrayReplaceValues", &[a, loc, len, 0, 0])?;
        Ok(())
    }),
    ("NSMutableArray", false, "removeObjectsInArray:", |e| {
        let (a, other) = (e.arg(0), e.arg(2));
        for o in array_values(e, other)? {
            send(e, a, "removeObject:", &[o])?;
        }
        Ok(())
    }),
    ("NSMutableArray", false, "removeObjectsAtIndexes:", |e| {
        let (a, ix) = (e.arg(0), e.arg(2));
        for i in crate::indexset::index_list(e, ix).into_iter().rev() {
            cf(e, "_CFArrayRemoveValueAtIndex", &[a, i])?;
        }
        Ok(())
    }),
    ("NSMutableArray", false, "exchangeObjectAtIndex:withObjectAtIndex:", |e| {
        let (a, i, j) = (e.arg(0), e.arg(2), e.arg(3));
        cf(e, "_CFArrayExchangeValuesAtIndices", &[a, i, j])?;
        Ok(())
    }),
    ("NSMutableArray", false, "setArray:", |e| {
        let (a, other) = (e.arg(0), e.arg(2));
        let items = if other == 0 { vec![] } else { array_values(e, other)? };
        cf(e, "_CFArrayRemoveAllValues", &[a])?;
        for o in items {
            cf(e, "_CFArrayAppendValue", &[a, o])?;
        }
        Ok(())
    }),
    ("NSMutableArray", false, "replaceObjectsInRange:withObjectsFromArray:", |e| {
        let (a, loc, len, other) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let items = if other == 0 { vec![] } else { array_values(e, other)? };
        let buf = e.alloc(items.len() as u64 * 8 + 8)?;
        for (i, o) in items.iter().enumerate() {
            e.write_u64(buf + i as u64 * 8, *o)?;
        }
        cf(e, "_CFArrayReplaceValues", &[a, loc, len, buf, items.len() as u64])?;
        Ok(())
    }),
    ("NSMutableArray", false, "removeObject:", array_remove_object),
    ("NSMutableArray", false, "removeObjectIdenticalTo:", array_remove_identical),
    // NSRange {location, length} in x2/x3.
    ("NSArray", false, "subarrayWithRange:", |e| {
        let (a, loc, len) = (e.arg(0), e.arg(2) as usize, e.arg(3) as usize);
        let items = array_values(e, a)?;
        let Some(part) = items.get(loc..loc + len) else {
            return Err(Stop::Unsupported(format!("NSRangeException: range {loc}+{len} beyond bounds of {}", items.len())));
        };
        let r = array_from(e, part)?;
        e.ret(r);
        Ok(())
    }),
    ("NSArray", false, "objectsAtIndexes:", |e| {
        let (a, ix) = (e.arg(0), e.arg(2));
        let items = array_values(e, a)?;
        let picked: Vec<u64> = crate::indexset::index_list(e, ix).into_iter().filter_map(|i| items.get(i as usize).copied()).collect();
        let r = array_from(e, &picked)?;
        e.ret(r);
        Ok(())
    }),
    ("NSArray", false, "indexOfObjectPassingTest:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let found = array_passing(e, a, b, false, true)?;
        e.ret(found.first().copied().unwrap_or(i64::MAX as u64)); // NSNotFound
        Ok(())
    }),
    ("NSArray", false, "indexesOfObjectsPassingTest:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let found = array_passing(e, a, b, false, false)?;
        let set = new_index_set(e, &found)?;
        e.ret(set);
        Ok(())
    }),
    ("NSArray", false, "enumerateObjectsWithOptions:usingBlock:", |e| {
        let (a, options, b) = (e.arg(0), e.arg(2), e.arg(3));
        array_passing(e, a, b, options & ENUMERATION_REVERSE != 0, false)?;
        Ok(())
    }),
    ("NSArray", false, "firstObjectCommonWithArray:", |e| {
        let (a, other) = (e.arg(0), e.arg(2));
        let mut found = 0;
        for o in array_values(e, a)? {
            if send(e, other, "containsObject:", &[o])? & 0xff != 0 {
                found = o;
                break;
            }
        }
        e.ret(found);
        Ok(())
    }),
    ("NSArray", false, "indexOfObjectIdenticalTo:", array_index_identical),
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
    ("NSDictionary", false, "objectForKeyedSubscript:", dict_lookup),
    // Swift's NSDictionary subscript getter.
    ("NSDictionary", false, "__swift_objectForKeyedSubscript:", dict_lookup),
    ("NSDictionary", false, "valueForKey:", dict_lookup),
    ("NSDictionary", false, "allKeys", dict_all_keys),
    // The values for `keys`, `marker` where one is missing.
    ("NSDictionary", false, "objectsForKeys:notFoundMarker:", |e| {
        let (d, keys, marker) = (e.arg(0), e.arg(2), e.arg(3));
        let mut out = Vec::new();
        for k in array_values(e, keys)? {
            let v = send(e, d, "objectForKey:", &[k])?;
            out.push(if v == 0 { marker } else { v });
        }
        let r = array_from(e, &out)?;
        e.ret(r);
        Ok(())
    }),
    ("NSDictionary", false, "allKeysForObject:", |e| {
        let (d, o) = (e.arg(0), e.arg(2));
        let mut out = Vec::new();
        for (k, v) in dict_pairs(e, d)? {
            if send(e, v, "isEqual:", &[o])? & 0xff != 0 {
                out.push(k);
            }
        }
        let r = array_from(e, &out)?;
        e.ret(r);
        Ok(())
    }),
    // block(key, obj, &stop) → BOOL: the keys it accepts, as an NSSet.
    ("NSDictionary", false, "keysOfEntriesPassingTest:", |e| {
        let (d, b) = (e.arg(0), e.arg(2));
        let stop = e.alloc(8)?;
        let mut out = Vec::new();
        for (k, v) in dict_pairs(e, d)? {
            if crate::libsystem::call_block(e, b, &[k, v, stop])? & 0xff != 0 {
                out.push(k);
            }
            if e.p.memory.read_u8(stop)? != 0 {
                break;
            }
        }
        let a = array_from(e, &out)?;
        let cls = host_class(e, "NSSet")?;
        let set = send(e, cls, "setWithArray:", &[a])?;
        e.ret(set);
        Ok(())
    }),
    ("NSDictionary", false, "enumerateKeysAndObjectsWithOptions:usingBlock:", |e| {
        e.cpu.x[2] = e.cpu.x[3];
        dict_enumerate(e)
    }),
    ("NSDictionary", false, "allValues", dict_all_values),
    ("NSDictionary", false, "copy", dict_copy),
    ("NSDictionary", false, "copyWithZone:", dict_copy),
    ("NSDictionary", false, "mutableCopy", dict_copy),
    ("NSDictionary", false, "enumerateKeysAndObjectsUsingBlock:", dict_enumerate),
    ("NSDictionary", false, "countByEnumeratingWithState:objects:count:", dict_fast_enumerate),
    ("NSDictionary", false, "description", dict_description),
    ("NSMutableDictionary", false, "setObject:forKey:", dict_set_object),
    ("NSMutableDictionary", false, "setObject:forKeyedSubscript:", dict_set_object),
    // Swift's NSMutableDictionary subscript setter (nil removes the key).
    ("NSMutableDictionary", false, "__swift_setObject:forKeyedSubscript:", dict_set_object),
    ("NSMutableDictionary", false, "setValue:forKey:", dict_set_object),
    ("NSMutableDictionary", false, "removeObjectForKey:", dict_remove),
    ("NSMutableDictionary", false, "removeObjectsForKeys:", |e| {
        let (d, keys) = (e.arg(0), e.arg(2));
        for k in if keys == 0 { vec![] } else { array_values(e, keys)? } {
            cf(e, "_CFDictionaryRemoveValue", &[d, k])?;
        }
        Ok(())
    }),
    ("NSMutableDictionary", false, "setDictionary:", |e| {
        let (d, other) = (e.arg(0), e.arg(2));
        let pairs = if other == 0 { vec![] } else { dict_pairs(e, other)? };
        cf(e, "_CFDictionaryRemoveAllValues", &[d])?;
        for (k, v) in pairs {
            cf(e, "_CFDictionarySetValue", &[d, k, v])?;
        }
        Ok(())
    }),
    ("NSMutableDictionary", false, "removeAllObjects", dict_remove_all),
    ("NSMutableDictionary", false, "addEntriesFromDictionary:", dict_add_entries),
    // NSSet
    ("NSSet", true, "set", set_empty),
    ("NSSet", true, "new", set_empty),
    ("NSSet", false, "init", set_empty),
    ("NSSet", true, "setWithArray:", set_with_array),
    ("NSSet", true, "setWithSet:", |e| {
        let other = e.arg(2);
        let all = if other == 0 { 0 } else { send(e, other, "allObjects", &[])? };
        e.cpu.x[2] = all;
        set_with_array(e)
    }),
    ("NSSet", false, "initWithSet:", |e| {
        let other = e.arg(2);
        let all = if other == 0 { 0 } else { send(e, other, "allObjects", &[])? };
        e.cpu.x[2] = all;
        set_with_array(e)
    }),
    ("NSDictionary", true, "dictionaryWithObjects:forKeys:", dict_with_object_and_key_arrays),
    ("NSDictionary", false, "initWithObjects:forKeys:", dict_with_object_and_key_arrays),
    ("NSSet", false, "isSubsetOfSet:", set_is_subset),
    ("NSSet", false, "intersectsSet:", set_intersects),
    ("NSSet", false, "setByAddingObject:", |e| {
        let o = e.arg(2);
        set_adding(e, if o == 0 { vec![] } else { vec![o] })
    }),
    ("NSSet", false, "setByAddingObjectsFromSet:", |e| {
        let other = e.arg(2);
        let more = elements_of(e, other)?;
        set_adding(e, more)
    }),
    ("NSSet", false, "setByAddingObjectsFromArray:", |e| {
        let a = e.arg(2);
        let more = if a == 0 { vec![] } else { array_values(e, a)? };
        set_adding(e, more)
    }),
    ("NSMutableSet", false, "addObjectsFromArray:", |e| {
        let (s, a) = (e.arg(0), e.arg(2));
        for o in if a == 0 { vec![] } else { array_values(e, a)? } {
            cf(e, "_CFSetAddValue", &[s, o])?;
        }
        Ok(())
    }),
    ("NSMutableSet", false, "unionSet:", |e| set_combine(e, "union")),
    ("NSMutableSet", false, "minusSet:", |e| set_combine(e, "minus")),
    ("NSMutableSet", false, "intersectSet:", |e| set_combine(e, "intersect")),
    ("NSMutableSet", false, "setSet:", |e| set_combine(e, "set")),
    ("NSSet", true, "setWithObjects:", set_with_objects_varargs),
    ("NSSet", false, "initWithArray:", set_with_array),
    ("NSSet", true, "setWithObjects:count:", set_with_objects_count),
    ("NSSet", false, "initWithObjects:count:", set_with_objects_count),
    ("NSSet", false, "initWithObjects:", set_with_objects_varargs),
    ("NSSet", true, "setWithObject:", set_with_object),
    ("NSMutableSet", true, "setWithCapacity:", set_empty),
    ("NSMutableSet", false, "initWithCapacity:", set_empty),
    ("NSSet", false, "count", set_count),
    ("NSSet", false, "containsObject:", set_contains),
    ("NSSet", false, "isEqualToSet:", set_is_equal),
    ("NSSet", false, "isEqual:", set_is_equal),
    ("NSSet", false, "hash", hash_is_count),
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
    // NSCountedSet (above): its own class, counting; the rest goes to its members' set.
    ("NSCountedSet", true, "set", |e| counted_make(e, 0)),
    ("NSCountedSet", true, "new", |e| counted_make(e, 0)),
    ("NSCountedSet", true, "setWithCapacity:", |e| counted_make(e, 0)),
    ("NSCountedSet", true, "setWithArray:", |e| counted_make(e, 1)),
    ("NSCountedSet", true, "setWithSet:", |e| counted_make(e, 1)),
    ("NSCountedSet", true, "setWithObject:", |e| counted_make(e, 2)),
    ("NSCountedSet", false, "init", |e| counted_init(e, 0)),
    ("NSCountedSet", false, "initWithCapacity:", |e| counted_init(e, 0)),
    ("NSCountedSet", false, "initWithArray:", |e| counted_init(e, 1)),
    ("NSCountedSet", false, "initWithSet:", |e| counted_init(e, 1)),
    ("NSCountedSet", false, "addObject:", |e| {
        let (s, o) = (e.arg(0), e.arg(2));
        counted_add(e, s, o)
    }),
    ("NSCountedSet", false, "removeObject:", |e| {
        let (s, o) = (e.arg(0), e.arg(2));
        let n = counted_count_of(e, s, o)?;
        if n > 0 {
            counted_set_count(e, s, o, n - 1)?;
        }
        Ok(())
    }),
    ("NSCountedSet", false, "countForObject:", |e| {
        let (s, o) = (e.arg(0), e.arg(2));
        let n = counted_count_of(e, s, o)?;
        e.ret(n);
        Ok(())
    }),
    ("NSCountedSet", false, "removeAllObjects", |e| {
        let s = e.arg(0);
        let store = counted_storage(e, s)?;
        let counts = crate::objc::bag_get(e, s, COUNTED_COUNTS).unwrap_or(0);
        cf(e, "_CFSetRemoveAllValues", &[store])?;
        cf(e, "_CFDictionaryRemoveAllValues", &[counts])?;
        Ok(())
    }),
    ("NSCountedSet", false, "count", |e| counted_forward(e, "count")),
    ("NSCountedSet", false, "containsObject:", |e| counted_forward(e, "containsObject:")),
    ("NSCountedSet", false, "member:", |e| counted_forward(e, "member:")),
    ("NSCountedSet", false, "allObjects", |e| counted_forward(e, "allObjects")),
    ("NSCountedSet", false, "anyObject", |e| counted_forward(e, "anyObject")),
    ("NSCountedSet", false, "objectEnumerator", |e| counted_forward(e, "objectEnumerator")),
    ("NSCountedSet", false, "countByEnumeratingWithState:objects:count:", |e| counted_forward(e, "countByEnumeratingWithState:objects:count:")),
    ("NSCountedSet", false, "enumerateObjectsUsingBlock:", |e| counted_forward(e, "enumerateObjectsUsingBlock:")),
    ("NSCountedSet", false, "description", |e| counted_forward(e, "description")),
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
    ("NSNumber", true, "numberWithUnsignedChar:", number_unsigned_char),
    ("NSNumber", true, "numberWithUnsignedShort:", number_unsigned_short),
    ("NSNumber", false, "initWithChar:", number_char),
    ("NSNumber", false, "initWithUnsignedChar:", number_unsigned_char),
    ("NSNumber", false, "initWithShort:", number_short),
    ("NSNumber", false, "initWithUnsignedShort:", number_unsigned_short),
    ("NSNumber", false, "initWithUnsignedInt:", number_unsigned_int),
    ("NSNumber", false, "initWithLong:", number_long),
    ("NSNumber", false, "initWithUnsignedLong:", number_long),
    ("NSNumber", false, "initWithUnsignedInteger:", number_long),
    ("NSNumber", false, "initWithUnsignedLongLong:", number_long),
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
    // NSNull has one instance, kCFNull: allocating one gives it (Swift's
    // NSNull() is alloc/init), and C code compares against it.
    ("NSNull", true, "alloc", null_null),
    ("NSNull", true, "allocWithZone:", null_null),
    ("NSNull", true, "new", null_null),
    ("NSNull", false, "init", |e| {
        let n = e.fnd.null_obj;
        e.ret(n);
        Ok(())
    }),
    ("NSNull", false, "copyWithZone:", null_null),
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

#[cfg(test)]
mod base64_tests {
    use super::{base64_decode, base64_encode};

    #[test]
    fn round_trips_like_nsdata() {
        assert_eq!(base64_encode(b"pomme", 0), "cG9tbWU=");
        assert_eq!(base64_decode(b"cG9tbWU=", 0).unwrap(), b"pomme");
        assert_eq!(base64_decode(b"cG9tbWU", 0).unwrap(), b"pomme"); // padding optional
        assert_eq!(base64_decode(b"", 0).unwrap(), b"");
        // Whitespace is refused unless unknown characters are ignored.
        assert!(base64_decode(b"cG9t bWU=", 0).is_none());
        assert_eq!(base64_decode(b"cG9t\r\nbWU=", 1).unwrap(), b"pomme");
        assert!(base64_decode(b"c", 0).is_none());
        // 64-character lines, CRLF.
        let long = base64_encode(&[0u8; 60], 1);
        assert_eq!(long.split("\r\n").map(str::len).collect::<Vec<_>>(), vec![64, 16]);
    }
}

#[cfg(test)]
mod padding_tests {
    use super::pad_to_length;

    fn pad(s: &str, len: usize, with: &str, start: usize) -> String {
        let (t, p): (Vec<u16>, Vec<u16>) = (s.encode_utf16().collect(), with.encode_utf16().collect());
        String::from_utf16_lossy(&pad_to_length(&t, len, &p, start))
    }

    #[test]
    fn pads_and_truncates_like_foundation() {
        // Apple's examples: "abc" padded to 9 with ". " from 1, and cut to 2.
        assert_eq!(pad("abc", 9, ". ", 1), "abc . . .");
        assert_eq!(pad("abc", 2, ". ", 0), "ab");
        assert_eq!(pad("7", 4, "0", 0), "7000");
    }
}

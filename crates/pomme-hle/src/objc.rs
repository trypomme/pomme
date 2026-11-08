//! pomme's Objective-C runtime.
//!
//! Classes compiled into the app (and its bundled frameworks) are read from
//! their `__objc_*` metadata in guest memory. System classes — `NSObject`,
//! `UIView`, ... — don't exist in any loaded image: for each one the guest
//! references we synthesize a `class_t` in guest memory and point the guest's
//! `_OBJC_CLASS_$_X` binds at it. Their methods are host functions.
//!
//! A selector (`SEL`) is the guest address of a canonical C string for its
//! name, so pointer comparison works as on iOS. Message sends look the
//! method up on the host side and either tail-call the guest IMP or run the
//! host implementation.
//!
//! Memory management is a no-op for now: retain/release do nothing and
//! objects are never freed. Deterministic, and fine for short sessions.

use crate::{Emu, HostFn, Result, Stop};
use std::collections::HashMap;

pub type Sel = u64;

#[derive(Clone, Copy)]
pub enum Imp {
    Guest(u64),
    Host(HostFn),
}

pub struct Class {
    pub name: String,
    pub meta: bool,
    pub isa: u64,
    pub superclass: u64,
    pub methods: HashMap<Sel, Imp>,
    pub instance_size: u64,
    /// Synthesized by pomme (a system class).
    pub host: bool,
    pub initialized: bool,
    /// Names of protocols this class adopts directly (including inherited protocols).
    pub protocols: Vec<String>,
}

#[derive(Default)]
pub struct Objc {
    pub classes: HashMap<u64, Class>,
    pub by_name: HashMap<String, u64>,
    sels: HashMap<String, Sel>,
    sel_names: HashMap<Sel, String>,
    cache: HashMap<(u64, Sel), Imp>,
    pub associated: HashMap<(u64, u64), u64>,
}

// Guest structure layouts (arm64, non-pointer isa not used).
const CLASS_SIZE: u64 = 40;
const RO_SIZE: u64 = 72;
const RO_META: u32 = 1;
const FAST_DATA_MASK: u64 = 0x0000_7fff_ffff_fff8;
const SMALL_METHOD_LIST: u32 = 0x8000_0000;
const DIRECT_SELECTORS: u32 = 0x4000_0000;

/// Superclasses of system classes. Anything not listed inherits from NSObject.
const HIERARCHY: &[(&str, &str)] = &[
    ("NSString", "NSObject"),
    ("NSMutableString", "NSString"),
    ("__NSCFString", "NSMutableString"),
    ("__NSCFConstantString", "__NSCFString"),
    ("NSArray", "NSObject"),
    ("NSMutableArray", "NSArray"),
    ("NSDictionary", "NSObject"),
    ("NSMutableDictionary", "NSDictionary"),
    ("NSSet", "NSObject"),
    ("NSMutableSet", "NSSet"),
    ("NSValue", "NSObject"),
    ("NSNumber", "NSValue"),
    ("NSData", "NSObject"),
    ("NSMutableData", "NSData"),
    ("NSAttributedString", "NSObject"),
    ("NSMutableAttributedString", "NSAttributedString"),
    ("NSBlock", "NSObject"),
    ("__NSGlobalBlock__", "NSBlock"),
    ("__NSStackBlock__", "NSBlock"),
    ("__NSMallocBlock__", "NSBlock"),
    ("NSOperation", "NSObject"),
    ("NSBlockOperation", "NSOperation"),
    ("NSNotification", "NSObject"),
    ("NSURLSessionTask", "NSObject"),
    ("NSURLSessionDataTask", "NSURLSessionTask"),
    ("NSURLSessionDownloadTask", "NSURLSessionTask"),
    ("NSURLSessionUploadTask", "NSURLSessionDataTask"),
    ("NSInputStream", "NSStream"),
    ("NSOutputStream", "NSStream"),
    ("NSException", "NSObject"),
    ("NSError", "NSObject"),
    ("NSMutableURLRequest", "NSURLRequest"),
    ("NSMutableCharacterSet", "NSCharacterSet"),
    ("NSMutableIndexSet", "NSIndexSet"),
    ("NSMutableOrderedSet", "NSOrderedSet"),
    ("NSMutableParagraphStyle", "NSParagraphStyle"),
    ("NSDateFormatter", "NSFormatter"),
    ("NSNumberFormatter", "NSFormatter"),
    ("NSISO8601DateFormatter", "NSFormatter"),
    ("NSByteCountFormatter", "NSFormatter"),
    ("NSDateComponentsFormatter", "NSFormatter"),
    ("NSPersonNameComponentsFormatter", "NSFormatter"),
    ("NSJSONSerialization", "NSObject"),
    ("NSKeyedArchiver", "NSCoder"),
    ("NSKeyedUnarchiver", "NSCoder"),
    ("UIResponder", "NSObject"),
    ("UIApplication", "UIResponder"),
    ("UIView", "UIResponder"),
    ("UIViewController", "UIResponder"),
    ("UINavigationController", "UIViewController"),
    ("UITabBarController", "UIViewController"),
    ("UIAlertController", "UIViewController"),
    ("UIActivityViewController", "UIViewController"),
    ("UIDocumentPickerViewController", "UIViewController"),
    ("UIImagePickerController", "UINavigationController"),
    ("UIHostingController", "UIViewController"),
    ("UIWindow", "UIView"),
    ("UIScrollView", "UIView"),
    ("UITableView", "UIScrollView"),
    ("UICollectionView", "UIScrollView"),
    ("UITextView", "UIScrollView"),
    ("UIControl", "UIView"),
    ("UIButton", "UIControl"),
    ("UITextField", "UIControl"),
    ("UISwitch", "UIControl"),
    ("UISlider", "UIControl"),
    ("UISegmentedControl", "UIControl"),
    ("UIDatePicker", "UIControl"),
    ("UIRefreshControl", "UIControl"),
    ("UILabel", "UIView"),
    ("UIImageView", "UIView"),
    ("UIActivityIndicatorView", "UIView"),
    ("UIVisualEffectView", "UIView"),
    ("UIStackView", "UIView"),
    ("UINavigationBar", "UIView"),
    ("UIToolbar", "UIView"),
    ("UIInputView", "UIView"),
    ("UIGestureRecognizer", "NSObject"),
    ("UITapGestureRecognizer", "UIGestureRecognizer"),
    ("UIPanGestureRecognizer", "UIGestureRecognizer"),
    ("UILongPressGestureRecognizer", "UIGestureRecognizer"),
    ("UIPinchGestureRecognizer", "UIGestureRecognizer"),
    ("UIRotationGestureRecognizer", "UIGestureRecognizer"),
    ("UISwipeGestureRecognizer", "UIGestureRecognizer"),
    ("UIScreenEdgePanGestureRecognizer", "UIPanGestureRecognizer"),
    ("UIHoverGestureRecognizer", "UIGestureRecognizer"),
    ("UIScene", "UIResponder"),
    ("UIWindowScene", "UIScene"),
    ("UIPresentationController", "NSObject"),
    ("UIMutableApplicationShortcutItem", "UIApplicationShortcutItem"),
    ("UIGraphicsImageRenderer", "UIGraphicsRenderer"),
    ("CALayer", "NSObject"),
    ("CAShapeLayer", "CALayer"),
    ("CAGradientLayer", "CALayer"),
    ("CATextLayer", "CALayer"),
    ("CAAnimation", "NSObject"),
    ("CAPropertyAnimation", "CAAnimation"),
    ("CABasicAnimation", "CAPropertyAnimation"),
    ("CASpringAnimation", "CABasicAnimation"),
    ("CAKeyframeAnimation", "CAPropertyAnimation"),
    ("CAAnimationGroup", "CAAnimation"),
    ("CATransition", "CAAnimation"),
    ("WKWebView", "UIView"),
];

impl Objc {
    pub fn sel_name(&self, sel: Sel) -> Option<&str> {
        self.sel_names.get(&sel).map(String::as_str)
    }

    pub fn class(&self, addr: u64) -> Option<&Class> {
        self.classes.get(&addr)
    }

    pub fn class_name(&self, addr: u64) -> String {
        self.classes.get(&addr).map_or_else(|| format!("<class {addr:#x}>"), |c| c.name.clone())
    }
}

// ---- setup ----

/// Build the runtime from every loaded image's metadata.
pub(crate) fn init(emu: &mut Emu) -> Result<()> {
    // 1. System classes the guest links against become synthesized host classes.
    let mut names: Vec<String> = emu
        .p
        .stubs
        .iter()
        .filter_map(|(_, s)| {
            s.symbol
                .strip_prefix("_OBJC_CLASS_$_")
                .or_else(|| s.symbol.strip_prefix("_OBJC_METACLASS_$_"))
                .map(str::to_string)
        })
        .collect();
    names.extend(HOST_METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::foundation::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::uikit::METHODS.iter().map(|m| m.0.to_string()));
    names.sort();
    names.dedup();
    for name in &names {
        host_class(emu, name)?;
    }
    for name in &names {
        let cls = emu.objc.by_name[name];
        let meta = emu.objc.classes[&cls].isa;
        for stub in emu.p.stubs.find(&format!("_OBJC_CLASS_$_{name}")) {
            emu.p.rebind(stub, cls).map_err(|e| Stop::Unsupported(e.to_string()))?;
        }
        for stub in emu.p.stubs.find(&format!("_OBJC_METACLASS_$_{name}")) {
            emu.p.rebind(stub, meta).map_err(|e| Stop::Unsupported(e.to_string()))?;
        }
    }
    for (symbol, class) in [
        ("___CFConstantStringClassReference", "__NSCFConstantString"),
        ("__NSConcreteGlobalBlock", "__NSGlobalBlock__"),
        ("__NSConcreteStackBlock", "__NSStackBlock__"),
        ("__NSConcreteMallocBlock", "__NSMallocBlock__"),
    ] {
        let cls = host_class(emu, class)?;
        for stub in emu.p.stubs.find(symbol) {
            emu.p.rebind(stub, cls).map_err(|e| Stop::Unsupported(e.to_string()))?;
        }
    }

    // 2. Host method implementations.
    for tables in [HOST_METHODS, crate::foundation::METHODS, crate::uikit::METHODS] {
        for &(class, class_method, sel, f) in tables {
            let mut cls = emu.objc.by_name[class];
            if class_method {
                cls = emu.objc.classes[&cls].isa;
            }
            let sel = intern(emu, sel)?;
            emu.objc.classes.get_mut(&cls).unwrap().methods.insert(sel, Imp::Host(f));
        }
    }

    // 3. Unique selector references.
    let images: Vec<_> = emu.p.images.iter().map(|i| (i.section("__DATA", "__objc_selrefs"), i.section("__DATA", "__objc_classlist"), i.section("__DATA", "__objc_catlist"))).collect();
    for &(selrefs, _, _) in &images {
        let Some((addr, size)) = selrefs else { continue };
        for at in (addr..addr + size).step_by(8) {
            let ptr = emu.read_u64(at)?;
            let name = emu.cstr(ptr)?;
            let sel = *emu.objc.sels.entry(name.clone()).or_insert(ptr);
            emu.objc.sel_names.entry(sel).or_insert(name);
            emu.write_u64(at, sel)?;
        }
    }

    // 4. Guest classes, then categories (which override class methods).
    for &(_, classlist, _) in &images {
        let Some((addr, size)) = classlist else { continue };
        for at in (addr..addr + size).step_by(8) {
            let cls = emu.read_u64(at)?;
            guest_class(emu, cls)?;
        }
    }
    for &(_, _, catlist) in &images {
        let Some((addr, size)) = catlist else { continue };
        for at in (addr..addr + size).step_by(8) {
            let cat = emu.read_u64(at)?;
            attach_category(emu, cat)?;
        }
    }
    Ok(())
}

/// Find or synthesize the system class `name` (and its superclasses).
pub fn host_class(emu: &mut Emu, name: &str) -> Result<u64> {
    if let Some(&c) = emu.objc.by_name.get(name) {
        return Ok(c);
    }
    let superclass = match HIERARCHY.iter().find(|(c, _)| *c == name) {
        Some((_, s)) => host_class(emu, s)?,
        None if name == "NSObject" || name == "NSProxy" => 0,
        None => host_class(emu, "NSObject")?,
    };
    let (super_meta, root_meta) = if superclass == 0 {
        (0, 0) // fixed up below: root meta's isa is itself, its superclass the root class
    } else {
        let sm = emu.objc.classes[&superclass].isa;
        (sm, emu.objc.classes[&sm].isa)
    };

    let cls = emu.alloc(CLASS_SIZE + RO_SIZE)?;
    let meta = emu.alloc(CLASS_SIZE + RO_SIZE)?;
    let name_ptr = emu.alloc_cstr(name)?;
    let root_meta = if superclass == 0 { meta } else { root_meta };
    let super_meta = if superclass == 0 { cls } else { super_meta };
    for (c, isa, sup, meta_flag) in [(cls, meta, superclass, 0), (meta, root_meta, super_meta, RO_META)] {
        let ro = c + CLASS_SIZE;
        emu.write_u64(c, isa)?;
        emu.write_u64(c + 8, sup)?;
        emu.write_u64(c + 32, ro)?;
        emu.p.memory.write_u32(ro, meta_flag)?;
        emu.p.memory.write_u32(ro + 4, 8)?;
        emu.p.memory.write_u32(ro + 8, 8)?;
        emu.write_u64(ro + 24, name_ptr)?;
    }
    for (c, isa, sup, is_meta) in [(cls, meta, superclass, false), (meta, root_meta, super_meta, true)] {
        emu.objc.classes.insert(
            c,
            Class {
                name: name.to_string(),
                meta: is_meta,
                isa,
                superclass: sup,
                methods: HashMap::new(),
                instance_size: 8,
                host: true,
                initialized: false,
                protocols: Vec::new(),
            },
        );
    }
    emu.objc.by_name.insert(name.to_string(), cls);
    Ok(cls)
}

/// Register a class compiled into a guest image, and its metaclass.
fn guest_class(emu: &mut Emu, cls: u64) -> Result<()> {
    if cls == 0 || emu.objc.classes.contains_key(&cls) {
        return Ok(());
    }
    let meta = emu.read_u64(cls)?;
    for c in [cls, meta] {
        if emu.objc.classes.contains_key(&c) {
            continue;
        }
        let isa = emu.read_u64(c)?;
        let superclass = emu.read_u64(c + 8)?;
        let ro = emu.read_u64(c + 32)? & FAST_DATA_MASK;
        let flags = emu.p.memory.read_u32(ro)?;
        let instance_size = emu.p.memory.read_u32(ro + 8)? as u64;
        let name = emu.cstr(emu.read_u64(ro + 24)?)?;
        let mut methods = HashMap::new();
        read_methods(emu, emu.read_u64(ro + 32)?, &mut methods)?;
        let mut protocols = Vec::new();
        read_protocols(emu, emu.read_u64(ro + 40)?, &mut protocols, 0)?;
        let meta_flag = flags & RO_META != 0;
        if !meta_flag {
            emu.objc.by_name.insert(name.clone(), c);
        }
        emu.objc.classes.insert(
            c,
            Class { name, meta: meta_flag, isa, superclass, methods, instance_size, host: false, initialized: false, protocols },
        );
    }
    Ok(())
}

fn read_methods(emu: &mut Emu, list: u64, out: &mut HashMap<Sel, Imp>) -> Result<()> {
    if list == 0 {
        return Ok(());
    }
    let flags = emu.p.memory.read_u32(list)?;
    let count = emu.p.memory.read_u32(list + 4)? as u64;
    let entsize = (flags & 0xfffc) as u64;
    for i in 0..count {
        let e = list + 8 + i * entsize;
        let (name, imp) = if flags & SMALL_METHOD_LIST != 0 {
            let rel = |emu: &Emu, at: u64| -> Result<u64> { Ok(at.wrapping_add(emu.p.memory.read_u32(at)? as i32 as i64 as u64)) };
            let name_ref = rel(emu, e)?;
            let name_ptr = if flags & DIRECT_SELECTORS != 0 { name_ref } else { emu.read_u64(name_ref)? };
            (emu.cstr(name_ptr)?, rel(emu, e + 8)?)
        } else {
            (emu.cstr(emu.read_u64(e)?)?, emu.read_u64(e + 16)?)
        };
        if imp != 0 {
            let sel = intern(emu, &name)?;
            out.insert(sel, Imp::Guest(imp));
        }
    }
    Ok(())
}

fn read_protocols(emu: &Emu, list: u64, out: &mut Vec<String>, depth: u32) -> Result<()> {
    if list == 0 || depth > 16 {
        return Ok(());
    }
    let count = emu.read_u64(list)?;
    for i in 0..count {
        let proto = emu.read_u64(list + 8 + i * 8)?;
        let name = emu.cstr(emu.read_u64(proto + 8)?)?;
        if !out.contains(&name) {
            out.push(name);
        }
        read_protocols(emu, emu.read_u64(proto + 16)?, out, depth + 1)?;
    }
    Ok(())
}

fn attach_category(emu: &mut Emu, cat: u64) -> Result<()> {
    let cls = emu.read_u64(cat + 8)?;
    if cls == 0 {
        return Ok(()); // category on a weak-missing class
    }
    guest_class(emu, cls)?;
    let Some(meta) = emu.objc.classes.get(&cls).map(|c| c.isa) else {
        return Err(Stop::Unsupported(format!("category on unknown class {cls:#x}")));
    };
    for (c, list) in [(cls, emu.read_u64(cat + 16)?), (meta, emu.read_u64(cat + 24)?)] {
        let mut methods = HashMap::new();
        read_methods(emu, list, &mut methods)?;
        emu.objc.classes.get_mut(&c).unwrap().methods.extend(methods);
    }
    let mut protocols = Vec::new();
    read_protocols(emu, emu.read_u64(cat + 32)?, &mut protocols, 0)?;
    emu.objc.classes.get_mut(&cls).unwrap().protocols.extend(protocols);
    Ok(())
}

/// The canonical SEL for a name, allocating its string if no image has one.
pub fn intern(emu: &mut Emu, name: &str) -> Result<Sel> {
    if let Some(&s) = emu.objc.sels.get(name) {
        return Ok(s);
    }
    let s = emu.alloc_cstr(name)?;
    emu.objc.sels.insert(name.to_string(), s);
    emu.objc.sel_names.insert(s, name.to_string());
    Ok(s)
}

pub fn sel_name(emu: &Emu, sel: Sel) -> String {
    match emu.objc.sel_name(sel) {
        Some(n) => n.to_string(),
        None => emu.cstr(sel).unwrap_or_else(|_| format!("<sel {sel:#x}>")),
    }
}

// ---- lookup and dispatch ----

/// The class of an object (for a class object, its metaclass).
pub fn class_of(emu: &mut Emu, obj: u64) -> Result<u64> {
    if obj >> 63 != 0 {
        return Err(Stop::Unsupported(format!("tagged pointer {obj:#x}")));
    }
    let isa = emu.read_u64(obj)?;
    if !emu.objc.classes.contains_key(&isa) {
        // A class we haven't seen (e.g. a Swift class realized late): read it now.
        let is_meta_of_known = emu.objc.classes.contains_key(&obj);
        if is_meta_of_known {
            return Err(Stop::Unsupported(format!("unknown metaclass {isa:#x}")));
        }
        guest_class(emu, isa).map_err(|_| Stop::Unsupported(format!("object {obj:#x} has bad isa {isa:#x}")))?;
    }
    Ok(isa)
}

pub fn lookup(emu: &mut Emu, cls: u64, sel: Sel) -> Result<Option<Imp>> {
    if let Some(&imp) = emu.objc.cache.get(&(cls, sel)) {
        return Ok(Some(imp));
    }
    let mut c = cls;
    while c != 0 {
        if !emu.objc.classes.contains_key(&c) {
            guest_class(emu, c)?;
        }
        let class = &emu.objc.classes[&c];
        if let Some(&imp) = class.methods.get(&sel) {
            emu.objc.cache.insert((cls, sel), imp);
            return Ok(Some(imp));
        }
        c = class.superclass;
    }
    Ok(None)
}

pub fn responds(emu: &mut Emu, obj: u64, sel: Sel) -> Result<bool> {
    if obj == 0 {
        return Ok(false);
    }
    let cls = class_of(emu, obj)?;
    Ok(lookup(emu, cls, sel)?.is_some())
}

/// The class itself for `obj` when it's a class, else its class.
fn instance_class(emu: &mut Emu, obj: u64) -> Result<u64> {
    let c = class_of(emu, obj)?;
    Ok(if emu.objc.classes[&c].meta { obj } else { c })
}

/// Run `+initialize` for a class (superclasses first) the first time it's messaged.
fn ensure_initialized(emu: &mut Emu, cls: u64) -> Result<()> {
    match emu.objc.classes.get(&cls) {
        Some(c) if !c.initialized && !c.meta => {}
        _ => return Ok(()),
    }
    emu.objc.classes.get_mut(&cls).unwrap().initialized = true;
    let sup = emu.objc.classes[&cls].superclass;
    if sup != 0 {
        ensure_initialized(emu, sup)?;
    }
    let saved = save_args(emu);
    send(emu, cls, "initialize", &[])?;
    restore_args(emu, saved);
    Ok(())
}

type Saved = ([u64; 9], [u128; 8]);

fn save_args(emu: &Emu) -> Saved {
    let mut x = [0; 9];
    x.copy_from_slice(&emu.cpu.x[..9]);
    let mut q = [0; 8];
    q.copy_from_slice(&emu.cpu.q[..8]);
    (x, q)
}

fn restore_args(emu: &mut Emu, (x, q): Saved) {
    emu.cpu.x[..9].copy_from_slice(&x);
    emu.cpu.q[..8].copy_from_slice(&q);
}

/// Dispatch a message whose receiver and selector are in x0/x1 and whose
/// method lookup starts at `cls`. Guest IMPs are tail-called.
fn dispatch(emu: &mut Emu, cls: u64) -> Result<()> {
    let (recv, sel) = (emu.cpu.x[0], emu.cpu.x[1]);
    let k = instance_class(emu, recv)?;
    ensure_initialized(emu, k)?;
    if emu.trace {
        let rc = class_of(emu, recv)?;
        let meta = emu.objc.classes[&rc].meta;
        println!(
            "{:>10} {:indent$}{}[{} {}]",
            emu.cpu.steps,
            "",
            if meta { '+' } else { '-' },
            emu.objc.class_name(k),
            sel_name(emu, sel),
            indent = emu.depth * 2
        );
    }
    match lookup(emu, cls, sel)? {
        Some(Imp::Guest(addr)) => {
            emu.cpu.pc = addr;
            Ok(())
        }
        Some(Imp::Host(f)) => f(emu),
        None => Err(Stop::MissingMethod {
            class: emu.objc.class_name(k),
            selector: sel_name(emu, sel),
            class_method: emu.objc.classes[&cls].meta,
            caller: emu.cpu.x[30],
        }),
    }
}

/// Send a message from host code and return x0.
pub fn send(emu: &mut Emu, recv: u64, sel: &str, args: &[u64]) -> Result<u64> {
    if recv == 0 {
        return Ok(0);
    }
    let sel = intern(emu, sel)?;
    let cls = class_of(emu, recv)?;
    let k = instance_class(emu, recv)?;
    ensure_initialized(emu, k)?;
    let mut all = vec![recv, sel];
    all.extend_from_slice(args);
    match lookup(emu, cls, sel)? {
        Some(Imp::Guest(addr)) => emu.call(addr, &all),
        Some(Imp::Host(f)) => {
            let (pc, lr) = (emu.cpu.pc, emu.cpu.x[30]);
            for (i, a) in all.iter().enumerate() {
                emu.cpu.x[i] = *a;
            }
            f(emu)?;
            emu.cpu.pc = pc;
            emu.cpu.x[30] = lr;
            Ok(emu.cpu.x[0])
        }
        None => Err(Stop::MissingMethod {
            class: emu.objc.class_name(k),
            selector: sel_name(emu, sel),
            class_method: emu.objc.classes[&cls].meta,
            caller: emu.cpu.pc,
        }),
    }
}

/// Allocate a zeroed instance of `cls`.
pub fn create_instance(emu: &mut Emu, cls: u64, extra: u64) -> Result<u64> {
    let size = emu.objc.classes.get(&cls).map_or(16, |c| c.instance_size).max(8) + extra;
    let obj = emu.alloc(size)?;
    emu.write_u64(obj, cls)?;
    Ok(obj)
}

pub fn is_kind_of(emu: &mut Emu, obj: u64, cls: u64) -> Result<bool> {
    if obj == 0 {
        return Ok(false);
    }
    let mut c = class_of(emu, obj)?;
    while c != 0 {
        if c == cls {
            return Ok(true);
        }
        c = emu.objc.classes.get(&c).map_or(0, |k| k.superclass);
    }
    Ok(false)
}

fn conforms(emu: &Emu, mut cls: u64, protocol: &str) -> bool {
    while let Some(c) = emu.objc.classes.get(&cls) {
        if c.protocols.iter().any(|p| p == protocol) {
            return true;
        }
        cls = c.superclass;
    }
    false
}

// ---- C entry points ----

fn msg_send(emu: &mut Emu) -> Result<()> {
    let recv = emu.cpu.x[0];
    if recv == 0 {
        emu.cpu.x[0] = 0;
        emu.cpu.x[1] = 0;
        emu.cpu.q[..4].fill(0);
        return Ok(());
    }
    let cls = class_of(emu, recv)?;
    dispatch(emu, cls)
}

/// `objc_msgSendSuper2(struct objc_super *, SEL, ...)`: super = {receiver, current class}.
fn msg_send_super2(emu: &mut Emu) -> Result<()> {
    let sup = emu.cpu.x[0];
    let recv = emu.read_u64(sup)?;
    let current = emu.read_u64(sup + 8)?;
    if !emu.objc.classes.contains_key(&current) {
        guest_class(emu, current)?;
    }
    let start = emu.objc.classes[&current].superclass;
    emu.cpu.x[0] = recv;
    dispatch(emu, start)
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(()) // x0 already holds the object
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn ret_zero(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

/// `objc_retain_xN`: retain xN and return it in x0.
macro_rules! from_reg {
    ($($n:literal),*) => {
        [$(|emu: &mut Emu| -> Result<()> { emu.cpu.x[0] = emu.cpu.x[$n]; Ok(()) }),*]
    };
}
const RETAIN_X: [HostFn; 29] = from_reg!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28);

fn send_named(emu: &mut Emu, sel: &str) -> Result<()> {
    let r = send(emu, emu.cpu.x[0], sel, &[])?;
    emu.ret(r);
    Ok(())
}

fn objc_alloc(emu: &mut Emu) -> Result<()> {
    send_named(emu, "alloc")
}

fn objc_alloc_with_zone(emu: &mut Emu) -> Result<()> {
    send_named(emu, "allocWithZone:")
}

fn objc_alloc_init(emu: &mut Emu) -> Result<()> {
    let obj = send(emu, emu.cpu.x[0], "alloc", &[])?;
    let r = send(emu, obj, "init", &[])?;
    emu.ret(r);
    Ok(())
}

fn objc_opt_new(emu: &mut Emu) -> Result<()> {
    send_named(emu, "new")
}

fn objc_opt_class(emu: &mut Emu) -> Result<()> {
    send_named(emu, "class")
}

fn objc_opt_is_kind_of_class(emu: &mut Emu) -> Result<()> {
    let r = send(emu, emu.cpu.x[0], "isKindOfClass:", &[emu.cpu.x[1]])?;
    emu.ret(r & 1);
    Ok(())
}

fn objc_opt_responds_to_selector(emu: &mut Emu) -> Result<()> {
    let r = send(emu, emu.cpu.x[0], "respondsToSelector:", &[emu.cpu.x[1]])?;
    emu.ret(r & 1);
    Ok(())
}

/// `objc_storeStrong(id *location, id obj)`
fn objc_store_strong(emu: &mut Emu) -> Result<()> {
    emu.write_u64(emu.cpu.x[0], emu.cpu.x[1])
}

/// `objc_storeWeak` / `objc_initWeak(id *location, id obj)` → obj
fn objc_store_weak(emu: &mut Emu) -> Result<()> {
    emu.write_u64(emu.cpu.x[0], emu.cpu.x[1])?;
    emu.ret(emu.cpu.x[1]);
    Ok(())
}

fn objc_load_weak(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.cpu.x[0])?;
    emu.ret(v);
    Ok(())
}

fn objc_destroy_weak(emu: &mut Emu) -> Result<()> {
    emu.write_u64(emu.cpu.x[0], 0)
}

fn objc_copy_weak(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.cpu.x[1])?;
    emu.write_u64(emu.cpu.x[0], v)
}

fn objc_move_weak(emu: &mut Emu) -> Result<()> {
    objc_copy_weak(emu)?;
    emu.write_u64(emu.cpu.x[1], 0)
}

fn objc_get_class(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.cpu.x[0])?;
    let c = emu.objc.by_name.get(&name).copied().unwrap_or(0);
    emu.ret(c);
    Ok(())
}

fn objc_get_meta_class(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.cpu.x[0])?;
    let c = emu.objc.by_name.get(&name).map_or(0, |c| emu.objc.classes[c].isa);
    emu.ret(c);
    Ok(())
}

fn class_get_name(emu: &mut Emu) -> Result<()> {
    // The name string lives in the class's ro data.
    let cls = emu.cpu.x[0];
    if cls == 0 {
        let s = emu.alloc_cstr("nil")?;
        emu.ret(s);
        return Ok(());
    }
    let ro = emu.read_u64(cls + 32)? & FAST_DATA_MASK;
    let name = emu.read_u64(ro + 24)?;
    emu.ret(name);
    Ok(())
}

pub(crate) fn object_get_class(emu: &mut Emu) -> Result<()> {
    let obj = emu.cpu.x[0];
    let c = if obj == 0 { 0 } else { class_of(emu, obj)? };
    emu.ret(c);
    Ok(())
}

fn class_get_superclass(emu: &mut Emu) -> Result<()> {
    let cls = emu.cpu.x[0];
    let s = emu.objc.classes.get(&cls).map_or(0, |c| c.superclass);
    emu.ret(s);
    Ok(())
}

fn sel_register_name(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.cpu.x[0])?;
    let s = intern(emu, &name)?;
    emu.ret(s);
    Ok(())
}

fn sel_get_name(_: &mut Emu) -> Result<()> {
    Ok(()) // a SEL is its own name string
}

fn class_responds_to_selector(emu: &mut Emu) -> Result<()> {
    let (cls, sel) = (emu.cpu.x[0], emu.cpu.x[1]);
    let r = cls != 0 && lookup(emu, cls, sel)?.is_some();
    emu.ret(r as u64);
    Ok(())
}

fn objc_get_associated(emu: &mut Emu) -> Result<()> {
    let v = emu.objc.associated.get(&(emu.cpu.x[0], emu.cpu.x[1])).copied().unwrap_or(0);
    emu.ret(v);
    Ok(())
}

fn objc_set_associated(emu: &mut Emu) -> Result<()> {
    let (obj, key, value) = (emu.cpu.x[0], emu.cpu.x[1], emu.cpu.x[2]);
    if value == 0 {
        emu.objc.associated.remove(&(obj, key));
    } else {
        emu.objc.associated.insert((obj, key), value);
    }
    Ok(())
}

/// `objc_getProperty(self, _cmd, offset, atomic)`
fn objc_get_property(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.cpu.x[0].wrapping_add(emu.cpu.x[2]))?;
    emu.ret(v);
    Ok(())
}

/// `objc_setProperty_*(self, _cmd, newValue, offset)`
fn objc_set_property(emu: &mut Emu) -> Result<()> {
    emu.write_u64(emu.cpu.x[0].wrapping_add(emu.cpu.x[3]), emu.cpu.x[2])
}

fn objc_set_property_copy(emu: &mut Emu) -> Result<()> {
    let (obj, value, offset) = (emu.cpu.x[0], emu.cpu.x[2], emu.cpu.x[3]);
    let copy = send(emu, value, "copy", &[])?;
    emu.write_u64(obj.wrapping_add(offset), copy)
}

/// `objc_copyStruct(dest, src, size, atomic, hasStrong)`
fn objc_copy_struct(emu: &mut Emu) -> Result<()> {
    let bytes = emu.p.memory.read(emu.cpu.x[1], emu.cpu.x[2] as usize)?.to_vec();
    emu.p.memory.write(emu.cpu.x[0], &bytes)?;
    Ok(())
}

fn objc_exception_throw(emu: &mut Emu) -> Result<()> {
    let exc = emu.cpu.x[0];
    let reason = send(emu, exc, "reason", &[]).ok().and_then(|s| crate::foundation::nsstring(emu, s).ok());
    Err(Stop::Unsupported(format!("Objective-C exception thrown: {}", reason.unwrap_or_default())))
}

fn objc_enumeration_mutation(_: &mut Emu) -> Result<()> {
    Err(Stop::Unsupported("collection mutated while being enumerated".into()))
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_objc_msgSend", msg_send),
    ("_objc_msgSendSuper2", msg_send_super2),
    ("_objc_opt_self", ret_self),
    ("_objc_opt_class", objc_opt_class),
    ("_objc_opt_new", objc_opt_new),
    ("_objc_opt_isKindOfClass", objc_opt_is_kind_of_class),
    ("_objc_opt_respondsToSelector", objc_opt_responds_to_selector),
    ("_objc_alloc", objc_alloc),
    ("_objc_allocWithZone", objc_alloc_with_zone),
    ("_objc_alloc_init", objc_alloc_init),
    ("_objc_retain", ret_self),
    ("_objc_release", noop),
    ("_objc_autorelease", ret_self),
    ("_objc_autoreleaseReturnValue", ret_self),
    ("_objc_retainAutorelease", ret_self),
    ("_objc_retainAutoreleaseReturnValue", ret_self),
    ("_objc_retainAutoreleasedReturnValue", ret_self),
    ("_objc_unsafeClaimAutoreleasedReturnValue", ret_self),
    ("_objc_retainBlock", crate::libc::block_copy),
    ("_objc_autoreleasePoolPush", ret_zero),
    ("_objc_autoreleasePoolPop", noop),
    ("__objc_autoreleasePoolPush", ret_zero),
    ("__objc_autoreleasePoolPop", noop),
    ("_objc_storeStrong", objc_store_strong),
    ("_objc_storeWeak", objc_store_weak),
    ("_objc_initWeak", objc_store_weak),
    ("_objc_loadWeakRetained", objc_load_weak),
    ("_objc_loadWeak", objc_load_weak),
    ("_objc_destroyWeak", objc_destroy_weak),
    ("_objc_copyWeak", objc_copy_weak),
    ("_objc_moveWeak", objc_move_weak),
    ("_objc_getClass", objc_get_class),
    ("_objc_lookUpClass", objc_get_class),
    ("_objc_getRequiredClass", objc_get_class),
    ("_objc_getMetaClass", objc_get_meta_class),
    ("_class_getName", class_get_name),
    ("_object_getClass", object_get_class),
    ("_class_getSuperclass", class_get_superclass),
    ("_sel_registerName", sel_register_name),
    ("_sel_getUid", sel_register_name),
    ("_sel_getName", sel_get_name),
    ("_class_respondsToSelector", class_responds_to_selector),
    ("_objc_sync_enter", ret_zero),
    ("_objc_sync_exit", ret_zero),
    ("_objc_getAssociatedObject", objc_get_associated),
    ("_objc_setAssociatedObject", objc_set_associated),
    ("_objc_getProperty", objc_get_property),
    ("_objc_setProperty_atomic", objc_set_property),
    ("_objc_setProperty_nonatomic", objc_set_property),
    ("_objc_setProperty_atomic_copy", objc_set_property_copy),
    ("_objc_setProperty_nonatomic_copy", objc_set_property_copy),
    ("_objc_copyStruct", objc_copy_struct),
    ("_objc_exception_throw", objc_exception_throw),
    ("_objc_enumerationMutation", objc_enumeration_mutation),
];

macro_rules! reg_names {
    ($prefix:literal; $($n:literal),*) => { [$(concat!($prefix, $n)),*] };
}
const RETAIN_NAMES: [&str; 29] = reg_names!("_objc_retain_x"; 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28);
const RELEASE_NAMES: [&str; 29] = reg_names!("_objc_release_x"; 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28);

/// `objc_retain_xN` / `objc_release_xN` helpers, whose names are generated.
pub(crate) fn register_reg_variants(registry: &mut HashMap<&'static str, HostFn>) {
    for i in 0..29 {
        registry.insert(RETAIN_NAMES[i], RETAIN_X[i]);
        registry.insert(RELEASE_NAMES[i], noop);
    }
}

// ---- NSObject ----

fn nsobject_alloc(emu: &mut Emu) -> Result<()> {
    let obj = create_instance(emu, emu.cpu.x[0], 0)?;
    emu.ret(obj);
    Ok(())
}

fn nsobject_new(emu: &mut Emu) -> Result<()> {
    let obj = create_instance(emu, emu.cpu.x[0], 0)?;
    let r = send(emu, obj, "init", &[])?;
    emu.ret(r);
    Ok(())
}

fn nsobject_class(emu: &mut Emu) -> Result<()> {
    let c = instance_class(emu, emu.cpu.x[0])?;
    emu.ret(c);
    Ok(())
}

fn nsobject_superclass(emu: &mut Emu) -> Result<()> {
    let c = instance_class(emu, emu.cpu.x[0])?;
    let s = emu.objc.classes.get(&c).map_or(0, |c| c.superclass);
    emu.ret(s);
    Ok(())
}

fn nsobject_responds_to_selector(emu: &mut Emu) -> Result<()> {
    let r = responds(emu, emu.cpu.x[0], emu.cpu.x[2])?;
    emu.ret(r as u64);
    Ok(())
}

fn nsobject_instances_respond(emu: &mut Emu) -> Result<()> {
    let r = lookup(emu, emu.cpu.x[0], emu.cpu.x[2])?.is_some();
    emu.ret(r as u64);
    Ok(())
}

fn nsobject_is_kind_of_class(emu: &mut Emu) -> Result<()> {
    let r = is_kind_of(emu, emu.cpu.x[0], emu.cpu.x[2])?;
    emu.ret(r as u64);
    Ok(())
}

fn nsobject_is_member_of_class(emu: &mut Emu) -> Result<()> {
    let r = class_of(emu, emu.cpu.x[0])? == emu.cpu.x[2];
    emu.ret(r as u64);
    Ok(())
}

fn nsobject_is_subclass_of_class(emu: &mut Emu) -> Result<()> {
    let (mut c, target) = (emu.cpu.x[0], emu.cpu.x[2]);
    let mut r = false;
    while c != 0 {
        if c == target {
            r = true;
            break;
        }
        c = emu.objc.classes.get(&c).map_or(0, |k| k.superclass);
    }
    emu.ret(r as u64);
    Ok(())
}

fn nsobject_conforms_to_protocol(emu: &mut Emu) -> Result<()> {
    let proto = emu.cpu.x[2];
    let name = if proto == 0 { String::new() } else { emu.cstr(emu.read_u64(proto + 8)?)? };
    let c = instance_class(emu, emu.cpu.x[0])?;
    emu.ret(conforms(emu, c, &name) as u64);
    Ok(())
}

fn nsobject_is_equal(emu: &mut Emu) -> Result<()> {
    emu.ret((emu.cpu.x[0] == emu.cpu.x[2]) as u64);
    Ok(())
}

fn nsobject_retain_count(emu: &mut Emu) -> Result<()> {
    emu.ret(1);
    Ok(())
}

fn nsobject_description(emu: &mut Emu) -> Result<()> {
    let obj = emu.cpu.x[0];
    let c = instance_class(emu, obj)?;
    let s = format!("<{}: {obj:#x}>", emu.objc.class_name(c));
    let r = crate::foundation::new_nsstring(emu, &s)?;
    emu.ret(r);
    Ok(())
}

fn nsobject_perform_selector(emu: &mut Emu) -> Result<()> {
    // performSelector:(SEL) [withObject:(id) [withObject:(id)]]: shift the args down.
    let (obj, sel) = (emu.cpu.x[0], emu.cpu.x[2]);
    let name = sel_name(emu, sel);
    let r = send(emu, obj, &name, &[emu.cpu.x[3], emu.cpu.x[4]])?;
    emu.ret(r);
    Ok(())
}

pub(crate) const HOST_METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSObject", true, "alloc", nsobject_alloc),
    ("NSObject", true, "allocWithZone:", nsobject_alloc),
    ("NSObject", true, "new", nsobject_new),
    ("NSObject", true, "initialize", noop),
    ("NSObject", true, "load", noop),
    ("NSObject", true, "class", ret_self),
    ("NSObject", true, "instancesRespondToSelector:", nsobject_instances_respond),
    ("NSObject", true, "isSubclassOfClass:", nsobject_is_subclass_of_class),
    ("NSObject", false, "init", ret_self),
    ("NSObject", false, "self", ret_self),
    ("NSObject", false, "class", nsobject_class),
    ("NSObject", false, "superclass", nsobject_superclass),
    ("NSObject", false, "retain", ret_self),
    ("NSObject", false, "release", noop),
    ("NSObject", false, "autorelease", ret_self),
    ("NSObject", false, "retainCount", nsobject_retain_count),
    ("NSObject", false, "dealloc", noop),
    ("NSObject", false, "copy", ret_self),
    ("NSObject", false, "respondsToSelector:", nsobject_responds_to_selector),
    ("NSObject", false, "isKindOfClass:", nsobject_is_kind_of_class),
    ("NSObject", false, "isMemberOfClass:", nsobject_is_member_of_class),
    ("NSObject", false, "conformsToProtocol:", nsobject_conforms_to_protocol),
    ("NSObject", false, "isEqual:", nsobject_is_equal),
    ("NSObject", false, "hash", ret_self),
    ("NSObject", false, "description", nsobject_description),
    ("NSObject", false, "debugDescription", nsobject_description),
    ("NSObject", false, "performSelector:", nsobject_perform_selector),
    ("NSObject", false, "performSelector:withObject:", nsobject_perform_selector),
    ("NSObject", false, "performSelector:withObject:withObject:", nsobject_perform_selector),
];

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
    /// Instance variables: (name, address of its offset variable, type encoding).
    pub ivars: Vec<(String, u64, String)>,
}

#[derive(Default)]
pub struct Objc {
    pub classes: HashMap<u64, Class>,
    pub by_name: HashMap<String, u64>,
    sels: HashMap<String, Sel>,
    sel_names: HashMap<Sel, String>,
    cache: HashMap<(u64, Sel), Imp>,
    pub associated: HashMap<(u64, u64), u64>,
    /// Properties set on permissive objects: (object, property) → (x2, d0).
    bag: HashMap<(u64, String), (u64, u128)>,
    /// Class → is it a native Swift class (cache for retains).
    native_swift: HashMap<u64, bool>,
    swift_retain: u64,
    swift_release: u64,
    /// Type encoding (C string address) by selector, as seen in method lists.
    types: HashMap<Sel, u64>,
    /// `Method` structs handed to the guest: address → (class, selector).
    method_objs: HashMap<u64, (u64, Sel)>,
    method_by_key: HashMap<(u64, Sel), u64>,
    /// Protocol objects by name, from every image's `__objc_protolist`.
    pub protocols: HashMap<String, u64>,
    hooks: HashMap<&'static str, u64>,
}

// Guest structure layouts (arm64, non-pointer isa not used).
const CLASS_SIZE: u64 = 40;
const RO_SIZE: u64 = 72;
const RO_META: u32 = 1;
const FAST_DATA_MASK: u64 = 0x0000_7fff_ffff_fff8;
const SMALL_METHOD_LIST: u32 = 0x8000_0000;
const DIRECT_SELECTORS: u32 = 0x4000_0000;

/// System classes pomme doesn't implement yet whose missing methods may be
/// answered generically: setters are accepted and ignored, everything else
/// returns nil/0/NO, and class methods returning an instance get a fresh one.
/// These are configuration-style objects that don't affect what's on screen.
/// `--trace` shows every message answered this way.
const PERMISSIVE: &[&str] = &[
    "NSCache",
    "CALayer",
    "CATransaction",
    "UIGraphicsImageRendererFormat",
    "UIGraphicsImageRenderer",
    "UIGraphicsImageRendererContext",
    "UIImage",
    "UIView",
    "NSLayoutManager",
    "NSTextContainer",
    "NSURLSessionConfiguration",
    "NSURLSession",
    "NSURLSessionTask",
    "NSURLRequest",
    "NSURLResponse",
    "NSURLCache",
    "NSHTTPCookieStorage",
    "NSOperationQueue",
    // Device properties: 0 is a fine answer (no proximity sensor, battery unknown, ...).
    "UIDevice",
    // Traits pomme doesn't model are "unspecified" (0), which is what UIKit reports too.
    "UITraitCollection",
    // Until there's text layout: writing direction 0 is left-to-right.
    "NSParagraphStyle",
    // Screen properties beyond bounds/scale (brightness, mirroring, ...).
    "UIScreen",
    // No touch input yet: gesture recognizers are configured but never fire.
    "UIGestureRecognizer",
    // Navigation chrome configuration (react-native-screens' header config):
    // stored and read back, mostly not drawn yet.
    "UINavigationItem",
    "UINavigationBar",
    "UIBarAppearance",
    "UIBarButtonItem",
    "UIBarButtonItemAppearance",
    "UIBarButtonItemStateAppearance",
    "UIBlurEffect",
];

fn permissive_class(emu: &Emu, mut cls: u64) -> bool {
    while let Some(c) = emu.objc.classes.get(&cls) {
        if c.host && PERMISSIVE.contains(&c.name.as_str()) {
            return true;
        }
        cls = c.superclass;
    }
    false
}

/// A property stored on a permissive object (`setFoo:`), if it was set.
pub(crate) fn bag_get(emu: &Emu, obj: u64, key: &str) -> Option<u64> {
    emu.objc.bag.get(&(obj, key.to_string())).map(|&(x, _)| x)
}

/// Copy every stored property of `from` onto `to` (copy initializers).
pub(crate) fn bag_copy(emu: &mut Emu, from: u64, to: u64) {
    let entries: Vec<_> = emu.objc.bag.iter().filter(|((o, _), _)| *o == from).map(|((_, k), v)| (k.clone(), *v)).collect();
    for (k, v) in entries {
        emu.objc.bag.insert((to, k), v);
    }
}

/// Store a property on a permissive object, as `setFoo:` would.
pub(crate) fn bag_set(emu: &mut Emu, obj: u64, key: &str, value: u64) {
    emu.objc.bag.insert((obj, key.to_string()), (value, 0));
}

/// Answer a missing message on a permissive class.
fn permissive_answer(emu: &mut Emu, cls: u64, sel: &str) -> Result<()> {
    let meta = emu.objc.classes.get(&cls).is_some_and(|c| c.meta);
    if emu.trace {
        println!("{:>10}   (permissive: {}[{} {sel}] answered generically)", emu.cpu.steps, if meta { '+' } else { '-' }, emu.objc.class_name(cls));
    }
    // Instances are property bags: `setFoo:` stores, `foo` / `isFoo` reads back.
    if !meta {
        let obj = emu.cpu.x[0];
        if let Some(name) = sel.strip_prefix("set").and_then(|r| r.strip_suffix(':')).filter(|n| !n.contains(':') && !n.is_empty()) {
            let key = name[..1].to_ascii_lowercase() + &name[1..];
            emu.objc.bag.insert((obj, key), (emu.cpu.x[2], emu.cpu.q[0]));
            return Ok(());
        }
        if !sel.contains(':') {
            let is = sel.strip_prefix("is").filter(|r| r.starts_with(|c: char| c.is_ascii_uppercase()));
            let key = is.map_or_else(|| sel.to_string(), |r| r[..1].to_ascii_lowercase() + &r[1..]);
            if let Some(&(x, q)) = emu.objc.bag.get(&(obj, key)) {
                emu.ret(x);
                emu.cpu.q[0] = q;
                return Ok(());
            }
        }
    }
    let r = if !meta && sel.starts_with("init") {
        emu.cpu.x[0] // initializers return self
    } else if meta && !sel.starts_with("set") {
        // Factory methods (+defaultSessionConfiguration, +sharedSession, ...): a fresh instance.
        let k = emu.cpu.x[0];
        create_instance(emu, k, 0)?
    } else {
        0
    };
    emu.ret(r);
    emu.cpu.q[0] = 0;
    Ok(())
}

/// Instance sizes of host classes that store fields in the object (isa + fields).
const HOST_SIZES: &[(&str, u64)] = &[("NSSortDescriptor", 32), ("__NSFastEnumerationEnumerator", 24), ("NSThread", 64), ("NSRunLoop", 16), ("NSNotification", 32), ("CADisplayLink", 48), ("NSUserDefaults", 16), ("NSError", 32), ("NSMethodSignature", 16), ("NSInvocation", 32), ("UIFont", 32), ("NSCache", 16), ("NSException", 32), ("NSValue", 48), ("NSTextStorage", 24), ("NSLayoutManager", 24), ("NSTextContainer", 56)];

/// Superclasses of system classes. Anything not listed inherits from NSObject.
const HIERARCHY: &[(&str, &str)] = &[
    ("NSString", "NSObject"),
    ("NSMutableString", "NSString"),
    ("__NSCFString", "NSMutableString"),
    ("__NSCFConstantString", "__NSCFString"),
    // Compile-time literals.
    ("NSConstantIntegerNumber", "NSNumber"),
    ("NSConstantDoubleNumber", "NSNumber"),
    ("NSConstantFloatNumber", "NSNumber"),
    ("NSConstantArray", "NSArray"),
    ("NSConstantDictionary", "NSDictionary"),
    // Toll-free bridged CoreFoundation types.
    ("__NSCFArray", "NSMutableArray"),
    ("__NSCFDictionary", "NSMutableDictionary"),
    ("__NSCFSet", "NSMutableSet"),
    ("__NSCFNumber", "NSNumber"),
    ("__NSCFBoolean", "NSNumber"),
    ("__NSCFData", "NSMutableData"),
    ("__NSDate", "NSDate"),
    ("__NSCFAttributedString", "NSMutableAttributedString"),
    ("__NSCFCharacterSet", "NSMutableCharacterSet"),
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
    ("__NSFastEnumerationEnumerator", "NSEnumerator"),
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
    ("NSHTTPURLResponse", "NSURLResponse"),
    ("__NSCFTimer", "NSTimer"),
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
    ("UINavigationItem", "NSObject"),
    ("UIViewLayoutRegion", "NSObject"),
    ("UIScrollEdgeEffectStyle", "NSObject"),
    ("UIBarAppearance", "NSObject"),
    ("UINavigationBarAppearance", "UIBarAppearance"),
    ("UIToolbarAppearance", "UIBarAppearance"),
    ("UITabBarAppearance", "UIBarAppearance"),
    ("UIBarItem", "NSObject"),
    ("UIBarButtonItem", "UIBarItem"),
    ("UIBarButtonItemAppearance", "NSObject"),
    ("UIVisualEffect", "NSObject"),
    ("UIBlurEffect", "UIVisualEffect"),
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
    // Classes that must exist even if the app never names them (looked up by
    // name, e.g. by the Swift runtime when it reparents its native NS* classes).
    names.extend(
        ["NSObject", "NSString", "NSMutableString", "NSArray", "NSMutableArray", "NSDictionary", "NSMutableDictionary", "NSSet", "NSMutableSet", "NSEnumerator", "NSNumber", "NSNull"]
            .map(str::to_string),
    );
    names.extend(HOST_METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::foundation::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::uikit::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::bundle::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::uiview::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::threads::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::invocation::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::attributed::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::textkit::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::calayer::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::touch::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::net::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::nstimer::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::image::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::regex::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::json::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(EXCEPTION_METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::nsvalue::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::navigation::METHODS.iter().map(|m| m.0.to_string()));
    names.extend(crate::navigation::PROPS.iter().flatten().map(|m| m.0.to_string()));
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

    // 2. Host method implementations. NSProxy is a second root class with the same basics.
    let proxy = host_class(emu, "NSProxy")?;
    let proxy_meta = emu.objc.classes[&proxy].isa;
    for &(class, class_method, sel, f) in HOST_METHODS {
        if class == "NSObject" && !sel.contains("ForKey:") {
            let sel = intern(emu, sel)?;
            let c = if class_method { proxy_meta } else { proxy };
            emu.objc.classes.get_mut(&c).unwrap().methods.insert(sel, Imp::Host(f));
        }
    }
    let props: Vec<_> = crate::navigation::PROPS.iter().flatten().copied().collect();
    for tables in [&props[..], HOST_METHODS, crate::foundation::METHODS, crate::uikit::METHODS, crate::bundle::METHODS, crate::uiview::METHODS, crate::threads::METHODS, crate::invocation::METHODS, crate::attributed::METHODS, crate::textkit::METHODS, crate::calayer::METHODS, crate::touch::METHODS, crate::net::METHODS, crate::nstimer::METHODS, crate::image::METHODS, crate::regex::METHODS, crate::json::METHODS, crate::nsvalue::METHODS, crate::navigation::METHODS, EXCEPTION_METHODS] {
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

    // Protocols by name.
    let protolists: Vec<_> = emu.p.images.iter().filter_map(|i| i.section("__DATA", "__objc_protolist")).collect();
    for (addr, size) in protolists {
        for at in (addr..addr + size).step_by(8) {
            let proto = emu.read_u64(at)?;
            let name = emu.cstr(emu.read_u64(proto + 8)?)?;
            emu.objc.protocols.entry(name).or_insert(proto);
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
    // Host classes that keep state in the object itself declare their size.
    let own_size = HOST_SIZES.iter().find(|(n, _)| *n == name).map(|(_, s)| *s);
    let size = own_size.unwrap_or_else(|| emu.objc.classes.get(&superclass).map_or(8, |k| k.instance_size));
    for (c, isa, sup, is_meta) in [(cls, meta, superclass, false), (meta, root_meta, super_meta, true)] {
        emu.objc.classes.insert(
            c,
            Class {
                name: name.to_string(),
                meta: is_meta,
                isa,
                superclass: sup,
                methods: HashMap::new(),
                instance_size: if is_meta { 8 } else { size },
                host: true,
                initialized: false,
                protocols: Vec::new(),
                ivars: Vec::new(),
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
        let mut instance_size = emu.p.memory.read_u32(ro + 8)? as u64;
        if flags & RO_META == 0 && superclass != 0 {
            instance_size = slide_ivars(emu, ro, superclass, instance_size)?;
        }
        let name_ptr = emu.read_u64(ro + 24)?;
        let name = if name_ptr != 0 {
            emu.cstr(name_ptr)?
        } else {
            // Classes the Swift runtime builds at run time are named lazily
            // through the hook it registered (objc_setHook_lazyClassNamer).
            match emu.objc.hooks.get("lazyClassNamer").copied() {
                Some(hook) => {
                    let p = emu.call(hook, &[cls])?;
                    if p == 0 { format!("<class {cls:#x}>") } else { emu.cstr(p)? }
                }
                None => format!("<class {cls:#x}>"),
            }
        };
        let mut methods = HashMap::new();
        read_methods(emu, emu.read_u64(ro + 32)?, &mut methods)?;
        let mut protocols = Vec::new();
        read_protocols(emu, emu.read_u64(ro + 40)?, &mut protocols, 0)?;
        let meta_flag = flags & RO_META != 0;
        if !meta_flag {
            emu.objc.by_name.insert(name.clone(), c);
        }
        let ivars = read_ivars(emu, emu.read_u64(ro + 48)?)?;
        emu.objc.classes.insert(
            c,
            Class { name, meta: meta_flag, isa, superclass, methods, instance_size, host: false, initialized: false, protocols, ivars },
        );
    }
    Ok(())
}

/// Non-fragile ivars: a class is compiled assuming its superclass's size; if
/// the superclass is bigger at run time, move this class's ivars after it (as
/// the Objective-C runtime does when it realizes the class). Returns the
/// class's real instance size.
fn slide_ivars(emu: &mut Emu, ro: u64, superclass: u64, own_size: u64) -> Result<u64> {
    if !emu.objc.classes.contains_key(&superclass) {
        guest_class(emu, superclass)?;
    }
    let super_size = instance_size(emu, superclass);
    let start = emu.p.memory.read_u32(ro + 4)? as u64;
    let ivars = emu.read_u64(ro + 48)?;
    // Keep the ivars' alignment.
    let mut align = 8u64;
    let (entsize, count) = if ivars == 0 {
        (0, 0)
    } else {
        (emu.p.memory.read_u32(ivars)? as u64, emu.p.memory.read_u32(ivars + 4)? as u64)
    };
    for i in 0..count {
        let raw = emu.p.memory.read_u32(ivars + 8 + i * entsize + 24)?;
        if raw != u32::MAX && raw < 16 {
            align = align.max(1 << raw);
        }
    }
    let super_end = (super_size + align - 1) & !(align - 1);
    if super_end <= start {
        return Ok(own_size);
    }
    let diff = super_end - start;
    for i in 0..count {
        let offset_var = emu.read_u64(ivars + 8 + i * entsize)?;
        if offset_var != 0 {
            let v = emu.p.memory.read_u32(offset_var)? as u64;
            emu.p.memory.write_u32(offset_var, (v + diff) as u32)?;
        }
    }
    // Record the new layout in the class data itself (the single source of truth).
    emu.p.memory.write_u32(ro + 4, (start + diff) as u32)?;
    emu.p.memory.write_u32(ro + 8, (own_size + diff) as u32)?;
    Ok(own_size + diff)
}

/// `ivar_list_t`: entries are {int32_t *offset; const char *name; const char *type; uint32 alignment; uint32 size}.
fn read_ivars(emu: &Emu, list: u64) -> Result<Vec<(String, u64, String)>> {
    if list == 0 {
        return Ok(Vec::new());
    }
    let entsize = emu.p.memory.read_u32(list)? as u64;
    let count = emu.p.memory.read_u32(list + 4)? as u64;
    let mut out = Vec::new();
    for i in 0..count {
        let e = list + 8 + i * entsize;
        let offset_var = emu.read_u64(e)?;
        let name = emu.read_u64(e + 8)?;
        let ty = emu.read_u64(e + 16)?;
        if offset_var == 0 || name == 0 {
            continue;
        }
        out.push((emu.cstr(name)?, offset_var, if ty == 0 { String::new() } else { emu.cstr(ty)? }));
    }
    Ok(out)
}

/// Find an ivar by name on a class or its superclasses: (offset, type encoding).
pub fn find_ivar(emu: &Emu, mut cls: u64, name: &str) -> Result<Option<(u64, String)>> {
    while let Some(c) = emu.objc.classes.get(&cls) {
        if let Some((_, var, ty)) = c.ivars.iter().find(|(n, _, _)| n == name) {
            return Ok(Some((emu.p.memory.read_u32(*var)? as u64, ty.clone())));
        }
        cls = c.superclass;
    }
    Ok(None)
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
        let (name, types, imp) = if flags & SMALL_METHOD_LIST != 0 {
            let rel = |emu: &Emu, at: u64| -> Result<u64> { Ok(at.wrapping_add(emu.p.memory.read_u32(at)? as i32 as i64 as u64)) };
            let name_ref = rel(emu, e)?;
            let name_ptr = if flags & DIRECT_SELECTORS != 0 { name_ref } else { emu.read_u64(name_ref)? };
            (emu.cstr(name_ptr)?, rel(emu, e + 4)?, rel(emu, e + 8)?)
        } else {
            (emu.cstr(emu.read_u64(e)?)?, emu.read_u64(e + 8)?, emu.read_u64(e + 16)?)
        };
        if imp != 0 {
            let sel = intern(emu, &name)?;
            out.insert(sel, Imp::Guest(imp));
            emu.objc.types.entry(sel).or_insert(types);
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

/// The type encoding recorded for a selector (from any method list that has it).
pub fn method_type_encoding(emu: &Emu, sel: Sel) -> Option<String> {
    let p = *emu.objc.types.get(&sel)?;
    if p == 0 { None } else { emu.cstr(p).ok() }
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
    let mut isa = emu.read_u64(obj)?;
    if isa == 0 {
        // A CF object made before toll-free bridging was set up: give it its class now.
        isa = crate::foundation::class_for_unbridged(emu, obj)?;
        emu.write_u64(obj, isa)?;
    }
    if !emu.objc.classes.contains_key(&isa) {
        // A class we haven't seen (e.g. a Swift class realized late): read it now.
        let is_meta_of_known = emu.objc.classes.contains_key(&obj);
        if is_meta_of_known {
            return Err(Stop::Unsupported(format!("unknown metaclass {isa:#x}")));
        }
        if let Err(e) = guest_class(emu, isa) {
            let words: Vec<String> = (0..6).map(|i| format!("{:#x}", emu.read_u64(isa + i * 8).unwrap_or(0))).collect();
            return Err(Stop::Unsupported(format!(
                "object {obj:#x} has an isa {isa:#x} pomme can't read as a class ({e}); isa memory: [{}]",
                words.join(", ")
            )));
        }
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
    match emu.objc.classes.get(&c) {
        Some(k) => Ok(if k.meta { obj } else { c }),
        None => Err(Stop::Unsupported(format!("object {obj:#x}: class {c:#x} isn't registered"))),
    }
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
        None if permissive_class(emu, cls) => {
            let name = sel_name(emu, sel);
            permissive_answer(emu, cls, &name)
        }
        None if forwards(emu, cls)? => {
            // Fast forwarding: -forwardingTargetForSelector: names another receiver.
            let target = send(emu, recv, "forwardingTargetForSelector:", &[sel])?;
            if target != 0 && target != recv {
                emu.cpu.x[0] = target;
                let tcls = class_of(emu, target)?;
                return dispatch(emu, tcls);
            }
            Err(Stop::MissingMethod {
                class: emu.objc.class_name(k),
                selector: sel_name(emu, sel),
                class_method: emu.objc.classes[&cls].meta,
                caller: emu.cpu.x[30],
            })
        }
        None => Err(Stop::MissingMethod {
            class: emu.objc.class_name(k),
            selector: sel_name(emu, sel),
            class_method: emu.objc.classes[&cls].meta,
            caller: emu.cpu.x[30],
        }),
    }
}

/// Does the class implement `-forwardingTargetForSelector:` (in guest code, or a host wrapper like NSTextStorage)?
fn forwards(emu: &mut Emu, cls: u64) -> Result<bool> {
    let sel = intern(emu, "forwardingTargetForSelector:")?;
    Ok(lookup(emu, cls, sel)?.is_some())
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
            // Like Emu::call: the sender's registers survive.
            let (pc, lr) = (emu.cpu.pc, emu.cpu.x[30]);
            let saved = save_args(emu);
            let mut temps = [0u64; 9];
            temps.copy_from_slice(&emu.cpu.x[9..18]);
            for (i, a) in all.iter().enumerate() {
                emu.cpu.x[i] = *a;
            }
            f(emu)?;
            let result = emu.cpu.x[0];
            emu.last_q0 = emu.cpu.q[0];
            restore_args(emu, saved);
            emu.cpu.x[9..18].copy_from_slice(&temps);
            emu.cpu.pc = pc;
            emu.cpu.x[30] = lr;
            Ok(result)
        }
        None => Err(Stop::MissingMethod {
            class: emu.objc.class_name(k),
            selector: sel_name(emu, sel),
            class_method: emu.objc.classes[&cls].meta,
            caller: emu.cpu.pc,
        }),
    }
}

/// Call `sel` as defined directly by a class (or by one category on it), not
/// inherited: how dyld runs `+load`. `meta` holds the class methods.
pub(crate) fn call_own_method(emu: &mut Emu, cls: u64, meta: u64, entry: u64, is_category: bool, sel: &str) -> Result<()> {
    let sel_addr = intern(emu, sel)?;
    let imp = if is_category {
        let mut methods = HashMap::new();
        read_methods(emu, emu.read_u64(entry + 24)?, &mut methods)?;
        methods.get(&sel_addr).copied()
    } else {
        guest_class(emu, cls)?;
        emu.objc.classes.get(&meta).and_then(|c| c.methods.get(&sel_addr).copied())
    };
    match imp {
        Some(Imp::Guest(addr)) => {
            if emu.trace {
                println!("{:>10} +[{} {sel}]", emu.cpu.steps, emu.objc.class_name(cls));
            }
            emu.call(addr, &[cls, sel_addr])?;
        }
        Some(Imp::Host(f)) => {
            emu.cpu.x[0] = cls;
            emu.cpu.x[1] = sel_addr;
            f(emu)?;
        }
        None => {}
    }
    Ok(())
}

/// A class's instance size. For guest classes it's read live from the class
/// data: the Swift runtime lays out classes with resilient fields at run time
/// and rewrites their size after pomme first saw them.
pub fn instance_size(emu: &Emu, cls: u64) -> u64 {
    match emu.objc.classes.get(&cls) {
        Some(c) if c.host => c.instance_size,
        Some(c) => emu
            .read_u64(cls + 32)
            .ok()
            .map(|data| data & FAST_DATA_MASK)
            .and_then(|ro| emu.p.memory.read_u32(ro + 8).ok())
            .map_or(c.instance_size, |s| s as u64),
        None => 16,
    }
}

/// Allocate a zeroed instance of `cls`, running C++ ivar constructors.
pub fn create_instance(emu: &mut Emu, cls: u64, extra: u64) -> Result<u64> {
    let size = instance_size(emu, cls).max(8) + extra;
    let obj = emu.alloc(size)?;
    if emu.trace {
        println!("{:>10}   alloc {} → {obj:#x} ({size} bytes)", emu.cpu.steps, emu.objc.class_name(cls));
    }
    emu.write_u64(obj, cls)?;
    cxx_construct(emu, obj, cls)?;
    Ok(obj)
}

/// Ivars of C++ type are built by a compiler-generated `.cxx_construct` method
/// on each class that has them: call each class's own one, base class first.
fn cxx_construct(emu: &mut Emu, obj: u64, cls: u64) -> Result<()> {
    let sel = intern(emu, ".cxx_construct")?;
    let mut chain = Vec::new();
    let mut c = cls;
    while let Some(k) = emu.objc.classes.get(&c) {
        if let Some(Imp::Guest(imp)) = k.methods.get(&sel) {
            chain.push(*imp);
        }
        c = k.superclass;
    }
    for imp in chain.into_iter().rev() {
        let saved = save_args(emu);
        emu.call(imp, &[obj, sel])?;
        restore_args(emu, saved);
    }
    Ok(())
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
    let cls = match class_of(emu, recv) {
        Ok(c) => c,
        Err(Stop::Unsupported(why)) => {
            // Say who sent what, and what the "isa" memory holds.
            let sel = sel_name(emu, emu.cpu.x[1]);
            let from = emu.symbolize(emu.cpu.x[30].wrapping_sub(4));
            let isa = emu.read_u64(recv).unwrap_or(0);
            let words: Vec<String> = (0..5).map(|i| format!("{:#x}", emu.read_u64(isa + i * 8).unwrap_or(0))).collect();
            return Err(Stop::Unsupported(format!("{why}; message -{sel} from {from}; isa memory: [{}]", words.join(", "))));
        }
        Err(e) => return Err(e),
    };
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

/// Is `obj` an instance of a native Swift class (root `Swift._SwiftObject`)?
/// Those are reference-counted by the Swift runtime, which frees them.
fn is_native_swift(emu: &mut Emu, obj: u64) -> bool {
    if obj == 0 || obj >> 63 != 0 {
        return false;
    }
    let Ok(cls) = emu.read_u64(obj) else { return false };
    if let Some(&b) = emu.objc.native_swift.get(&cls) {
        return b;
    }
    let mut c = cls;
    let mut root = String::new();
    while let Some(k) = emu.objc.classes.get(&c) {
        if k.meta {
            break;
        }
        root = k.name.clone();
        c = k.superclass;
    }
    let native = root == "_TtCs12_SwiftObject" || root == "Swift._SwiftObject";
    emu.objc.native_swift.insert(cls, native);
    native
}

/// Retain/release from Objective-C (and CF collections). pomme never frees
/// Objective-C or CF objects, so for those these do nothing. Native Swift
/// objects are reference-counted by the guest Swift runtime, and Swift relies on
/// the counts being exact (uniqueness checks, copy-on-write), so for them these
/// forward to `swift_retain` / `swift_release`, as on iOS.
pub(crate) fn objc_retain_obj(emu: &mut Emu, obj: u64) -> Result<()> {
    swift_refcount(emu, obj, true)
}

pub(crate) fn objc_release_obj(emu: &mut Emu, obj: u64) -> Result<()> {
    swift_refcount(emu, obj, false)
}

fn swift_refcount(emu: &mut Emu, obj: u64, retain: bool) -> Result<()> {
    if obj != 0 && obj >> 63 == 0 && crate::libc::is_heap_block(emu, obj) {
        // Heap blocks are reference counted too (ARC retains/releases them as objects).
        if retain {
            let f = emu.p.memory.read_u32(obj + 8)?;
            emu.p.memory.write_u32(obj + 8, f + 2)?;
        } else {
            crate::libc::block_release_obj(emu, obj)?;
        }
        return Ok(());
    }
    if !is_native_swift(emu, obj) {
        return Ok(());
    }
    let (slot, name) = if retain { (&mut emu.objc.swift_retain, "_swift_retain") } else { (&mut emu.objc.swift_release, "_swift_release") };
    if *slot == 0 {
        let found = emu.p.images.iter().find_map(|i| i.symbol(name)).unwrap_or(u64::MAX);
        let slot = if retain { &mut emu.objc.swift_retain } else { &mut emu.objc.swift_release };
        *slot = found;
    }
    let f = if retain { emu.objc.swift_retain } else { emu.objc.swift_release };
    if f != u64::MAX {
        emu.call(f, &[obj])?;
    }
    Ok(())
}

fn objc_release(emu: &mut Emu) -> Result<()> {
    objc_release_obj(emu, emu.cpu.x[0])
}

/// `objc_release_xN`: release xN.
macro_rules! release_reg {
    ($($n:literal),*) => {
        [$(|emu: &mut Emu| -> Result<()> { objc_release_obj(emu, emu.cpu.x[$n]) }),*]
    };
}
const RELEASE_X: [HostFn; 29] = release_reg!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28);

fn objc_retain(emu: &mut Emu) -> Result<()> {
    objc_retain_obj(emu, emu.cpu.x[0]) // returns its argument (x0 is preserved)
}

/// `objc_retain_xN`: retain xN and return it in x0.
macro_rules! from_reg {
    ($($n:literal),*) => {
        [$(|emu: &mut Emu| -> Result<()> { emu.cpu.x[0] = emu.cpu.x[$n]; objc_retain_obj(emu, emu.cpu.x[0]) }),*]
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
    let (loc, obj) = (emu.cpu.x[0], emu.cpu.x[1]);
    let old = emu.read_u64(loc)?;
    objc_retain_obj(emu, obj)?;
    emu.write_u64(loc, obj)?;
    objc_release_obj(emu, old)
}

/// `objc_storeWeak` / `objc_initWeak(id *location, id obj)` → obj
fn objc_store_weak(emu: &mut Emu) -> Result<()> {
    emu.write_u64(emu.cpu.x[0], emu.cpu.x[1])?;
    emu.ret(emu.cpu.x[1]);
    Ok(())
}

/// `objc_loadWeak`: the object (+0).
fn objc_load_weak(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.cpu.x[0])?;
    emu.ret(v);
    Ok(())
}

/// `objc_loadWeakRetained`: the object, +1 (the caller releases it).
fn objc_load_weak_retained(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.cpu.x[0])?;
    objc_retain_obj(emu, v)?;
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
    let c = class_by_name(emu, &name)?;
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
    // Just the isa, like the real one: the Swift runtime calls this on classes
    // whose metaclass it is still filling in.
    let obj = emu.cpu.x[0];
    let c = if obj == 0 { 0 } else { emu.read_u64(obj)? };
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
/// `objc_setProperty_*(self, _cmd, newValue, offset)` for strong properties: retain new, release old.
fn objc_set_property(emu: &mut Emu) -> Result<()> {
    let (obj, value, offset) = (emu.cpu.x[0], emu.cpu.x[2], emu.cpu.x[3]);
    let at = obj.wrapping_add(offset);
    let old = emu.read_u64(at)?;
    objc_retain_obj(emu, value)?;
    emu.write_u64(at, value)?;
    objc_release_obj(emu, old)
}

/// …and for copy properties: store `[newValue copy]` (+1), release old.
fn objc_set_property_copy(emu: &mut Emu) -> Result<()> {
    let (obj, value, offset) = (emu.cpu.x[0], emu.cpu.x[2], emu.cpu.x[3]);
    let at = obj.wrapping_add(offset);
    let old = emu.read_u64(at)?;
    let copy = send(emu, value, "copy", &[])?;
    emu.write_u64(at, copy)?;
    objc_release_obj(emu, old)
}

/// `-copy` of an immutable object: itself, retained (copy returns +1).
fn nsobject_copy(emu: &mut Emu) -> Result<()> {
    objc_retain_obj(emu, emu.cpu.x[0]) // returns self
}

fn nsobject_retain(emu: &mut Emu) -> Result<()> {
    objc_retain_obj(emu, emu.cpu.x[0])
}

fn nsobject_release(emu: &mut Emu) -> Result<()> {
    objc_release_obj(emu, emu.cpu.x[0])
}

/// `-[NSBlock copy]`: `_Block_copy` (moves a stack block to the heap, or retains a heap one).
fn block_copy_method(emu: &mut Emu) -> Result<()> {
    crate::libc::block_copy(emu)
}

/// `objc_copyStruct(dest, src, size, atomic, hasStrong)`
fn objc_copy_struct(emu: &mut Emu) -> Result<()> {
    let bytes = emu.p.memory.read(emu.cpu.x[1], emu.cpu.x[2] as usize)?.to_vec();
    emu.p.memory.write(emu.cpu.x[0], &bytes)?;
    Ok(())
}

fn objc_exception_throw(emu: &mut Emu) -> Result<()> {
    let exc = emu.cpu.x[0];
    let mut text = |sel: &str| send(emu, exc, sel, &[]).ok().filter(|&s| s != 0).and_then(|s| crate::foundation::nsstring(emu, s).ok());
    let (name, reason) = (text("name"), text("reason"));
    Err(Stop::Unsupported(format!("uncaught Objective-C exception {}: {}", name.unwrap_or_default(), reason.unwrap_or_default())))
}

/// NSException {isa, name, reason, userInfo}: enough to describe what was thrown.
fn exception_init(emu: &mut Emu, class_method: bool) -> Result<()> {
    let e = if class_method {
        let cls = emu.arg(0);
        create_instance(emu, cls, 0)?
    } else {
        emu.arg(0)
    };
    for (i, reg) in [2, 3, 4].into_iter().enumerate() {
        let v = emu.arg(reg);
        if v != 0 {
            objc_retain_obj(emu, v)?;
        }
        emu.write_u64(e + 8 + i as u64 * 8, v)?;
    }
    emu.ret(e);
    Ok(())
}

fn exception_field(emu: &mut Emu, off: u64) -> Result<()> {
    let v = emu.read_u64(emu.arg(0) + off)?;
    emu.ret(v);
    Ok(())
}

fn objc_enumeration_mutation(_: &mut Emu) -> Result<()> {
    Err(Stop::Unsupported("collection mutated while being enumerated".into()))
}

pub(crate) const EXCEPTION_METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSException", true, "exceptionWithName:reason:userInfo:", |e| exception_init(e, true)),
    ("NSException", false, "initWithName:reason:userInfo:", |e| exception_init(e, false)),
    ("NSException", false, "name", |e| exception_field(e, 8)),
    ("NSException", false, "reason", |e| exception_field(e, 16)),
    ("NSException", false, "userInfo", |e| exception_field(e, 24)),
    ("NSException", false, "raise", objc_exception_throw),
    ("NSException", false, "callStackSymbols", |e| {
        let a = crate::foundation::new_array_of(e, &[])?;
        e.ret(a);
        Ok(())
    }),
];

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
    ("_objc_retain", objc_retain),
    ("_objc_release", objc_release),
    ("_objc_autorelease", ret_self),
    ("_objc_autoreleaseReturnValue", ret_self),
    ("_objc_retainAutorelease", objc_retain),
    ("_objc_retainAutoreleaseReturnValue", objc_retain),
    ("_objc_retainAutoreleasedReturnValue", objc_retain),
    ("_objc_unsafeClaimAutoreleasedReturnValue", ret_self),
    ("_objc_retainBlock", crate::libc::block_copy),
    ("_objc_autoreleasePoolPush", ret_zero),
    ("_objc_autoreleasePoolPop", noop),
    ("__objc_autoreleasePoolPush", ret_zero),
    ("__objc_autoreleasePoolPop", noop),
    ("_objc_storeStrong", objc_store_strong),
    ("_objc_storeWeak", objc_store_weak),
    ("_objc_initWeak", objc_store_weak),
    ("_objc_loadWeakRetained", objc_load_weak_retained),
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
        registry.insert(RELEASE_NAMES[i], RELEASE_X[i]);
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

/// Key-value coding, the basic case: `valueForKey:` calls the getter named `key`.
fn nsobject_value_for_key(emu: &mut Emu) -> Result<()> {
    let (obj, key) = (emu.cpu.x[0], emu.cpu.x[2]);
    let name = crate::foundation::nsstring(emu, key)?;
    let sel = intern(emu, &name)?;
    if responds(emu, obj, sel)? {
        let r = send(emu, obj, &name, &[])?;
        emu.ret(r);
        return Ok(());
    }
    // No getter: an instance variable named _key, _isKey, key or isKey.
    let k = instance_class(emu, obj)?;
    let cap = name[..1].to_uppercase() + &name[1..];
    for candidate in [format!("_{name}"), format!("_is{cap}"), name.clone(), format!("is{cap}")] {
        if let Some((offset, ty)) = find_ivar(emu, k, &candidate)? {
            let v = emu.read_u64(obj + offset)?;
            let r = match ty.chars().next() {
                Some('@' | '#') | None => v,
                Some('c' | 'B') => crate::foundation::boolean(emu, v & 0xff != 0),
                Some('d') => crate::foundation::new_number_f64(emu, f64::from_bits(v))?,
                Some('f') => crate::foundation::new_number_f64(emu, f32::from_bits(v as u32) as f64)?,
                Some('i' | 'I') => crate::foundation::new_number_i64(emu, v as u32 as i32 as i64)?,
                Some('s' | 'S') => crate::foundation::new_number_i64(emu, v as u16 as i16 as i64)?,
                _ => crate::foundation::new_number_i64(emu, v as i64)?,
            };
            emu.ret(r);
            return Ok(());
        }
    }
    Err(Stop::Unsupported(format!("valueForKey: {} has no getter or ivar for `{name}`", emu.objc.class_name(k))))
}

/// `setValue:forKey:`: the `setKey:` setter, else an ivar `_key`/`_isKey`/`key`/`isKey`.
fn nsobject_set_value_for_key(emu: &mut Emu) -> Result<()> {
    let (obj, value, key) = (emu.cpu.x[0], emu.cpu.x[2], emu.cpu.x[3]);
    let name = crate::foundation::nsstring(emu, key)?;
    let cap = name[..1].to_uppercase() + &name[1..];
    let setter = format!("set{cap}:");
    let sel = intern(emu, &setter)?;
    if responds(emu, obj, sel)? {
        send(emu, obj, &setter, &[value])?;
        return Ok(());
    }
    let k = instance_class(emu, obj)?;
    for candidate in [format!("_{name}"), format!("_is{cap}"), name.clone(), format!("is{cap}")] {
        if let Some((offset, ty)) = find_ivar(emu, k, &candidate)? {
            let at = obj + offset;
            match ty.chars().next() {
                Some('@' | '#') | None => emu.write_u64(at, value)?,
                Some('c' | 'B') => {
                    let b = send(emu, value, "boolValue", &[])?;
                    emu.p.memory.write_u8(at, b as u8)?;
                }
                Some('d') => {
                    send(emu, value, "doubleValue", &[])?;
                    emu.write_u64(at, emu.last_q0 as u64)?;
                }
                Some('i' | 'I') => {
                    let v = send(emu, value, "longLongValue", &[])?;
                    emu.p.memory.write_u32(at, v as u32)?;
                }
                _ => {
                    let v = send(emu, value, "longLongValue", &[])?;
                    emu.write_u64(at, v)?;
                }
            }
            return Ok(());
        }
    }
    Err(Stop::Unsupported(format!("setValue:forKey: {} has no setter or ivar for `{name}`", emu.objc.class_name(k))))
}

/// `+instanceMethodForSelector:` (receiver is a class) / `-methodForSelector:`.
fn method_for_selector(emu: &mut Emu, cls: u64) -> Result<()> {
    let sel = emu.cpu.x[2];
    let a = match lookup(emu, cls, sel)? {
        Some(imp) => imp_addr(emu, imp)?,
        None => emu.host_thunk(forward_missing)?,
    };
    emu.ret(a);
    Ok(())
}

fn instance_method_for_selector(emu: &mut Emu) -> Result<()> {
    method_for_selector(emu, emu.cpu.x[0])
}

fn nsobject_method_for_selector(emu: &mut Emu) -> Result<()> {
    let cls = class_of(emu, emu.cpu.x[0])?;
    method_for_selector(emu, cls)
}

pub(crate) const HOST_METHODS: &[(&str, bool, &str, HostFn)] = &[
    // Key-value observing: registrations are accepted, but no change notifications are sent yet.
    ("NSObject", false, "addObserver:forKeyPath:options:context:", noop),
    ("NSObject", false, "removeObserver:forKeyPath:", noop),
    ("NSObject", false, "removeObserver:forKeyPath:context:", noop),
    ("NSObject", false, "willChangeValueForKey:", noop),
    ("NSObject", false, "didChangeValueForKey:", noop),
    ("NSObject", true, "instanceMethodForSelector:", instance_method_for_selector),
    ("NSObject", false, "methodForSelector:", nsobject_method_for_selector),
    ("NSObject", false, "valueForKey:", nsobject_value_for_key),
    ("NSObject", false, "setValue:forKey:", nsobject_set_value_for_key),
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
    ("NSObject", false, "retain", nsobject_retain),
    ("NSObject", false, "release", nsobject_release),
    ("NSObject", false, "autorelease", ret_self),
    ("NSObject", false, "retainCount", nsobject_retain_count),
    ("NSObject", false, "dealloc", noop),
    ("NSObject", false, "copy", nsobject_copy),
    ("NSObject", false, "copyWithZone:", nsobject_copy),
    ("NSBlock", false, "copy", block_copy_method),
    ("NSBlock", false, "copyWithZone:", block_copy_method),
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

// ---- reflection and runtime hooks (used by the Swift runtime and by swizzling code) ----

fn imp_addr(emu: &mut Emu, imp: Imp) -> Result<u64> {
    match imp {
        Imp::Guest(a) => Ok(a),
        Imp::Host(f) => emu.host_thunk(f),
    }
}

fn imp_from_addr(emu: &Emu, addr: u64) -> Imp {
    match emu.thunk_at(addr) {
        Some(f) => Imp::Host(f),
        None => Imp::Guest(addr),
    }
}

/// The `Method` struct {SEL name; const char *types; IMP imp} for (cls, sel).
fn method_obj(emu: &mut Emu, cls: u64, sel: Sel, imp: Imp) -> Result<u64> {
    if let Some(&m) = emu.objc.method_by_key.get(&(cls, sel)) {
        return Ok(m);
    }
    let m = emu.alloc(24)?;
    let types = emu.objc.types.get(&sel).copied().unwrap_or(0);
    let addr = imp_addr(emu, imp)?;
    emu.write_u64(m, sel)?;
    emu.write_u64(m + 8, types)?;
    emu.write_u64(m + 16, addr)?;
    emu.objc.method_by_key.insert((cls, sel), m);
    emu.objc.method_objs.insert(m, (cls, sel));
    Ok(m)
}

/// Find the class that actually defines `sel` for `cls` (walking superclasses).
fn defining_class(emu: &mut Emu, cls: u64, sel: Sel) -> Result<Option<(u64, Imp)>> {
    let mut c = cls;
    while c != 0 {
        if !emu.objc.classes.contains_key(&c) {
            guest_class(emu, c)?;
        }
        let class = &emu.objc.classes[&c];
        if let Some(&imp) = class.methods.get(&sel) {
            return Ok(Some((c, imp)));
        }
        c = class.superclass;
    }
    Ok(None)
}

fn get_method(emu: &mut Emu, cls: u64) -> Result<()> {
    let sel = emu.arg(1);
    let m = match (cls != 0).then_some(()).map(|_| defining_class(emu, cls, sel)) {
        Some(r) => match r? {
            Some((c, imp)) => method_obj(emu, c, sel, imp)?,
            None => 0,
        },
        None => 0,
    };
    emu.ret(m);
    Ok(())
}

fn class_get_instance_method(emu: &mut Emu) -> Result<()> {
    get_method(emu, emu.arg(0))
}

fn class_get_class_method(emu: &mut Emu) -> Result<()> {
    let cls = emu.arg(0);
    let meta = if cls == 0 { 0 } else { class_of(emu, cls)? };
    get_method(emu, meta)
}

fn method_get_name(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.arg(0))?;
    emu.ret(v);
    Ok(())
}

fn method_get_type_encoding(emu: &mut Emu) -> Result<()> {
    let v = emu.read_u64(emu.arg(0) + 8)?;
    emu.ret(v);
    Ok(())
}

fn method_get_implementation(emu: &mut Emu) -> Result<()> {
    let m = emu.arg(0);
    let v = if m == 0 { 0 } else { emu.read_u64(m + 16)? };
    emu.ret(v);
    Ok(())
}

fn set_imp(emu: &mut Emu, cls: u64, sel: Sel, addr: u64) {
    let imp = imp_from_addr(emu, addr);
    if let Some(c) = emu.objc.classes.get_mut(&cls) {
        c.methods.insert(sel, imp);
    }
    emu.objc.cache.clear();
}

fn method_set_implementation(emu: &mut Emu) -> Result<()> {
    let (m, imp) = (emu.arg(0), emu.arg(1));
    let old = emu.read_u64(m + 16)?;
    emu.write_u64(m + 16, imp)?;
    if let Some(&(cls, sel)) = emu.objc.method_objs.get(&m) {
        set_imp(emu, cls, sel, imp);
    }
    emu.ret(old);
    Ok(())
}

fn method_exchange_implementations(emu: &mut Emu) -> Result<()> {
    let (m1, m2) = (emu.arg(0), emu.arg(1));
    let i1 = emu.read_u64(m1 + 16)?;
    let i2 = emu.read_u64(m2 + 16)?;
    emu.write_u64(m1 + 16, i2)?;
    emu.write_u64(m2 + 16, i1)?;
    for (m, imp) in [(m1, i2), (m2, i1)] {
        if let Some(&(cls, sel)) = emu.objc.method_objs.get(&m) {
            set_imp(emu, cls, sel, imp);
        }
    }
    Ok(())
}

fn forward_missing(emu: &mut Emu) -> Result<()> {
    let (recv, sel) = (emu.arg(0), emu.arg(1));
    let k = instance_class(emu, recv)?;
    Err(Stop::MissingMethod { class: emu.objc.class_name(k), selector: sel_name(emu, sel), class_method: false, caller: emu.cpu.x[30] })
}

fn class_get_method_implementation(emu: &mut Emu) -> Result<()> {
    let (cls, sel) = (emu.arg(0), emu.arg(1));
    let imp = if cls == 0 { None } else { lookup(emu, cls, sel)? };
    let a = match imp {
        Some(imp) => imp_addr(emu, imp)?,
        None => emu.host_thunk(forward_missing)?,
    };
    emu.ret(a);
    Ok(())
}

fn own_imp(emu: &Emu, cls: u64, sel: Sel) -> Option<Imp> {
    emu.objc.classes.get(&cls).and_then(|c| c.methods.get(&sel).copied())
}

/// `class_addMethod(cls, sel, imp, types)`: fails if the class itself already has it.
fn class_add_method(emu: &mut Emu) -> Result<()> {
    let (cls, sel, imp, types) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    if own_imp(emu, cls, sel).is_some() {
        emu.ret(0);
        return Ok(());
    }
    emu.objc.types.entry(sel).or_insert(types);
    set_imp(emu, cls, sel, imp);
    emu.ret(1);
    Ok(())
}

fn class_replace_method(emu: &mut Emu) -> Result<()> {
    let (cls, sel, imp, types) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    let old = match own_imp(emu, cls, sel) {
        Some(i) => imp_addr(emu, i)?,
        None => 0,
    };
    emu.objc.types.entry(sel).or_insert(types);
    set_imp(emu, cls, sel, imp);
    if let Some(&m) = emu.objc.method_by_key.get(&(cls, sel)) {
        emu.write_u64(m + 16, imp)?;
    }
    emu.ret(old);
    Ok(())
}

fn class_copy_method_list(emu: &mut Emu) -> Result<()> {
    let (cls, out_count) = (emu.arg(0), emu.arg(1));
    let mut own: Vec<(String, Sel, Imp)> = match emu.objc.classes.get(&cls) {
        Some(c) => c.methods.iter().map(|(&s, &i)| (String::new(), s, i)).collect(),
        None => Vec::new(),
    };
    for m in &mut own {
        m.0 = sel_name(emu, m.1);
    }
    own.sort_by(|a, b| a.0.cmp(&b.0)); // deterministic order
    let list = emu.alloc((own.len() as u64 + 1) * 8)?;
    for (i, (_, sel, imp)) in own.iter().enumerate() {
        let m = method_obj(emu, cls, *sel, *imp)?;
        emu.write_u64(list + i as u64 * 8, m)?;
    }
    if out_count != 0 {
        emu.p.memory.write_u32(out_count, own.len() as u32)?;
    }
    emu.ret(list);
    Ok(())
}

fn class_copy_ivar_list(emu: &mut Emu) -> Result<()> {
    if emu.arg(1) != 0 {
        emu.p.memory.write_u32(emu.arg(1), 0)?;
    }
    emu.ret(0);
    Ok(())
}

/// `imp_implementationWithBlock(block)`: an IMP that calls `block(self, args...)`.
fn imp_implementation_with_block(emu: &mut Emu) -> Result<()> {
    crate::libc::block_copy(emu)?;
    let block = emu.arg(0);
    let code = emu.emit_code(&[
        0xaa00_03e1, // mov x1, x0          (self becomes the block's first argument)
        0x5800_0060, // ldr x0, #12         (the block)
        0xf940_0810, // ldr x16, [x0, #16]  (its invoke function)
        0xd61f_0200, // br x16
        block as u32,
        (block >> 32) as u32,
    ])?;
    emu.ret(code);
    Ok(())
}

fn realize_class(emu: &mut Emu) -> Result<()> {
    guest_class(emu, emu.arg(0))?;
    Ok(()) // returns the class, already in x0
}

fn class_get_instance_size(emu: &mut Emu) -> Result<()> {
    let s = instance_size(emu, emu.arg(0));
    emu.ret(s);
    Ok(())
}

fn class_is_meta_class(emu: &mut Emu) -> Result<()> {
    let m = emu.objc.classes.get(&emu.arg(0)).is_some_and(|c| c.meta);
    emu.ret(m as u64);
    Ok(())
}

fn class_create_instance(emu: &mut Emu) -> Result<()> {
    let (cls, extra) = (emu.arg(0), emu.arg(1));
    let o = if cls == 0 { 0 } else { create_instance(emu, cls, extra)? };
    emu.ret(o);
    Ok(())
}

fn objc_construct_instance(emu: &mut Emu) -> Result<()> {
    let (cls, bytes) = (emu.arg(0), emu.arg(1));
    if bytes != 0 {
        emu.write_u64(bytes, cls)?;
        cxx_construct(emu, bytes, cls)?;
    }
    emu.ret(bytes);
    Ok(())
}

fn object_set_class(emu: &mut Emu) -> Result<()> {
    let (obj, cls) = (emu.arg(0), emu.arg(1));
    if obj == 0 {
        emu.ret(0);
        return Ok(());
    }
    let old = emu.read_u64(obj)?;
    emu.write_u64(obj, cls)?;
    emu.ret(old);
    Ok(())
}

fn class_set_superclass(emu: &mut Emu) -> Result<()> {
    let (cls, sup) = (emu.arg(0), emu.arg(1));
    let old = emu.read_u64(cls + 8)?;
    emu.write_u64(cls + 8, sup)?;
    if let Some(c) = emu.objc.classes.get_mut(&cls) {
        c.superclass = sup;
    }
    // The metaclass follows: class methods inherit from the new superclass too.
    let meta = emu.read_u64(cls)?;
    let sup_meta = if sup == 0 { cls } else { emu.read_u64(sup)? };
    if meta != 0 && meta != cls {
        if !emu.objc.classes.contains_key(&meta) {
            guest_class(emu, cls)?;
        }
        emu.write_u64(meta + 8, sup_meta)?;
        if let Some(m) = emu.objc.classes.get_mut(&meta) {
            m.superclass = sup_meta;
        }
    }
    emu.objc.cache.clear();
    emu.ret(old);
    Ok(())
}

fn protocol_name(emu: &Emu, proto: u64) -> Result<String> {
    if proto == 0 { Ok(String::new()) } else { emu.cstr(emu.read_u64(proto + 8)?) }
}

fn class_conforms_to_protocol(emu: &mut Emu) -> Result<()> {
    let name = protocol_name(emu, emu.arg(1))?;
    emu.ret(conforms(emu, emu.arg(0), &name) as u64);
    Ok(())
}

fn protocol_get_name(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    let n = if p == 0 { 0 } else { emu.read_u64(p + 8)? };
    emu.ret(n);
    Ok(())
}

fn protocol_conforms_to_protocol(emu: &mut Emu) -> Result<()> {
    let (p, other) = (emu.arg(0), emu.arg(1));
    let target = protocol_name(emu, other)?;
    let mut names = vec![protocol_name(emu, p)?];
    if p != 0 {
        read_protocols(emu, emu.read_u64(p + 16)?, &mut names, 0)?;
    }
    emu.ret((p != 0 && other != 0 && names.contains(&target)) as u64);
    Ok(())
}

fn objc_get_protocol(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.arg(0))?;
    emu.ret(emu.objc.protocols.get(&name).copied().unwrap_or(0));
    Ok(())
}

/// `objc_allocateClassPair(superclass, name, extraBytes)`
fn objc_allocate_class_pair(emu: &mut Emu) -> Result<()> {
    let (sup, name_ptr) = (emu.arg(0), emu.arg(1));
    let name = emu.cstr(name_ptr)?;
    if emu.objc.by_name.contains_key(&name) {
        emu.ret(0);
        return Ok(());
    }
    // Build it like a host class, then point it at the requested superclass.
    let cls = host_class(emu, &name)?;
    let meta = emu.objc.classes[&cls].isa;
    let sup_meta = if sup == 0 { cls } else { emu.objc.classes.get(&sup).map_or(0, |c| c.isa) };
    let root_meta = if sup == 0 { meta } else { emu.objc.classes.get(&sup_meta).map_or(meta, |c| c.isa) };
    let size = emu.objc.classes.get(&sup).map_or(8, |c| c.instance_size);
    emu.write_u64(cls + 8, sup)?;
    emu.write_u64(meta, root_meta)?;
    emu.write_u64(meta + 8, sup_meta)?;
    for (c, s) in [(cls, sup), (meta, sup_meta)] {
        let k = emu.objc.classes.get_mut(&c).unwrap();
        k.superclass = s;
        k.host = false;
        k.instance_size = size;
    }
    emu.objc.classes.get_mut(&meta).unwrap().isa = root_meta;
    emu.ret(cls);
    Ok(())
}

/// The default `getClass` hook: plain lookup by name. `BOOL (const char *name, Class *out)`
fn default_get_class_hook(emu: &mut Emu) -> Result<()> {
    let (name, out) = (emu.cstr(emu.arg(0))?, emu.arg(1));
    let c = emu.objc.by_name.get(&name).copied().unwrap_or(0);
    emu.write_u64(out, c)?;
    emu.ret((c != 0) as u64);
    Ok(())
}

/// The default `getImageName` hook: `BOOL (Class, const char **out)` → NO.
fn default_get_image_name_hook(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

fn set_hook(emu: &mut Emu, name: &'static str, default: HostFn) -> Result<()> {
    let (new, old_out) = (emu.arg(0), emu.arg(1));
    let old = match emu.objc.hooks.get(name) {
        Some(&h) => h,
        None => emu.host_thunk(default)?,
    };
    if old_out != 0 {
        emu.write_u64(old_out, old)?;
    }
    emu.objc.hooks.insert(name, new);
    Ok(())
}

fn objc_set_hook_get_class(emu: &mut Emu) -> Result<()> {
    set_hook(emu, "getClass", default_get_class_hook)
}

fn objc_set_hook_get_image_name(emu: &mut Emu) -> Result<()> {
    set_hook(emu, "getImageName", default_get_image_name_hook)
}

fn objc_set_hook_lazy_class_namer(emu: &mut Emu) -> Result<()> {
    emu.objc.hooks.insert("lazyClassNamer", emu.arg(0));
    Ok(())
}

/// Look a class up by name, asking the guest's `getClass` hook (Swift) on a miss.
pub fn class_by_name(emu: &mut Emu, name: &str) -> Result<u64> {
    if let Some(&c) = emu.objc.by_name.get(name) {
        // System classes newer than the OS pomme presents don't exist yet.
        let host = emu.objc.classes.get(&c).is_some_and(|k| k.host);
        if host && !crate::system::class_available(&emu.system, name) {
            return Ok(0);
        }
        return Ok(c);
    }
    let Some(&hook) = emu.objc.hooks.get("getClass") else { return Ok(0) };
    if emu.thunk_at(hook).is_some() {
        return Ok(0);
    }
    let name_ptr = emu.alloc_cstr(name)?;
    let out = emu.alloc(8)?;
    emu.call(hook, &[name_ptr, out])?;
    let c = emu.read_u64(out)?;
    if c != 0 {
        guest_class(emu, c)?;
    }
    Ok(c)
}

/// `objc_addLoadImageFunc(fn)`: call `fn(mach_header)` for every loaded image now.
fn objc_add_load_image_func(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    let headers: Vec<u64> = emu.p.images.iter().map(|i| i.header).collect();
    for h in headers {
        emu.call(f, &[h])?;
    }
    Ok(())
}

fn exceptions_unsupported(_: &mut Emu) -> Result<()> {
    Err(Stop::Unsupported("Objective-C/C++ exception handling isn't implemented yet".into()))
}

pub(crate) const REFLECT_FUNCS: &[(&str, HostFn)] = &[
    ("_class_getInstanceMethod", class_get_instance_method),
    ("_class_getClassMethod", class_get_class_method),
    ("_method_getName", method_get_name),
    ("_method_getTypeEncoding", method_get_type_encoding),
    ("_method_getImplementation", method_get_implementation),
    ("_method_setImplementation", method_set_implementation),
    ("_method_exchangeImplementations", method_exchange_implementations),
    ("_class_getMethodImplementation", class_get_method_implementation),
    ("_class_addMethod", class_add_method),
    ("_class_replaceMethod", class_replace_method),
    ("_class_copyMethodList", class_copy_method_list),
    ("_class_copyIvarList", class_copy_ivar_list),
    ("_imp_implementationWithBlock", imp_implementation_with_block),
    ("__objc_realizeClassFromSwift", realize_class),
    ("_objc_readClassPair", realize_class),
    ("_class_getInstanceSize", class_get_instance_size),
    ("_class_isMetaClass", class_is_meta_class),
    ("_class_createInstance", class_create_instance),
    ("_objc_constructInstance", objc_construct_instance),
    ("_objc_destructInstance", ret_self),
    ("_object_dispose", ret_zero),
    ("_object_setClass", object_set_class),
    ("_class_setSuperclass", class_set_superclass),
    ("_class_conformsToProtocol", class_conforms_to_protocol),
    ("_protocol_getName", protocol_get_name),
    ("_protocol_conformsToProtocol", protocol_conforms_to_protocol),
    ("_objc_getProtocol", objc_get_protocol),
    ("_objc_allocateClassPair", objc_allocate_class_pair),
    ("_objc_registerClassPair", noop),
    ("__objc_rootAutorelease", ret_self),
    ("_objc_setHook_getClass", objc_set_hook_get_class),
    ("_objc_setHook_getImageName", objc_set_hook_get_image_name),
    ("_objc_setHook_lazyClassNamer", objc_set_hook_lazy_class_namer),
    ("_objc_addLoadImageFunc", objc_add_load_image_func),
    ("_objc_begin_catch", exceptions_unsupported),
    ("_objc_end_catch", exceptions_unsupported),
    ("_objc_exception_rethrow", exceptions_unsupported),
    ("_objc_terminate", exceptions_unsupported),
];

/// Make the guest memory at `at` a class: a method-less subclass of `base`
/// with the same name. Used for `__CFConstantStringClassReference`, whose
/// address is the `isa` of every string literal compiled into the app.
pub fn alias_class(emu: &mut Emu, at: u64, base: u64) -> Result<()> {
    let (name, meta, size) = {
        let b = &emu.objc.classes[&base];
        (b.name.clone(), b.isa, b.instance_size)
    };
    let ro = emu.read_u64(base + 32)?;
    emu.write_u64(at, meta)?;
    emu.write_u64(at + 8, base)?;
    emu.write_u64(at + 16, 0)?;
    emu.write_u64(at + 24, 0)?;
    emu.write_u64(at + 32, ro)?;
    emu.objc.classes.insert(
        at,
        Class {
            name,
            meta: false,
            isa: meta,
            superclass: base,
            methods: HashMap::new(),
            instance_size: size,
            host: true,
            initialized: false,
            protocols: Vec::new(),
            ivars: Vec::new(),
        },
    );
    Ok(())
}

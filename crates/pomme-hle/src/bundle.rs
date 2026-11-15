//! NSBundle: the app's main bundle and the bundles inside it (its frameworks,
//! resource bundles), each with its Info.plist as real Foundation objects.
//!
//! Inside the guest the app lives where iOS would put it
//! (`/private/var/containers/Bundle/Application/pomme/<name>.app`); resource
//! lookups check the bundle's directory on the host. `bundleForClass:` is the
//! bundle of the image the class is in: Flutter finds its ICU data in
//! Flutter.framework that way.

use crate::foundation::{self, nsstring};
use crate::objc::{host_class, send};
use crate::{Emu, HostFn, Result};
use std::path::PathBuf;

const CONTAINER: &str = "/private/var/containers/Bundle/Application/pomme";

#[derive(Clone)]
struct Bundle {
    obj: u64,
    /// The bundle's directory on the host.
    host: PathBuf,
    /// Its path in the guest.
    guest: String,
    info: u64,
}

#[derive(Clone, Default)]
pub struct State {
    /// The main bundle first, then others as they're asked for.
    bundles: Vec<Bundle>,
}

/// The `.app` directory on the host.
fn host_app_dir(emu: &Emu) -> PathBuf {
    let dir = emu.p.main().path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    // Absolute, like the paths of the app's other images.
    pomme_loader::vfs::canonicalize(&dir)
}

fn guest_bundle_path(emu: &Emu) -> String {
    let dir = host_app_dir(emu);
    let name = dir.file_name().map_or_else(|| format!("{}.app", emu.p.main().name), |n| n.to_string_lossy().into_owned());
    format!("{CONTAINER}/{name}")
}

/// The app's Info.plist, parsed on the host.
pub(crate) fn host_info_plist(emu: &Emu) -> Option<plist::Value> {
    let bytes = pomme_loader::vfs::read(&host_app_dir(emu).join("Info.plist")).ok()?;
    plist::Value::from_reader(std::io::Cursor::new(bytes)).ok()
}

/// The app's localizations, as iOS finds them: its `.lproj` folders (not
/// Base) and Info.plist's `CFBundleLocalizations`; with neither, its
/// development region (`CFBundleDevelopmentRegion`) alone; with none of
/// these, none.
pub(crate) fn main_localizations(emu: &Emu) -> Vec<String> {
    let mut found: Vec<String> = pomme_loader::vfs::read_dir(&host_app_dir(emu))
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(name, is_dir)| if is_dir { name.strip_suffix(".lproj").map(str::to_string) } else { None })
        .filter(|n| n != "Base")
        .collect();
    let info = host_info_plist(emu);
    let info = info.as_ref().and_then(|v| v.as_dictionary());
    if let Some(list) = info.and_then(|d| d.get("CFBundleLocalizations")).and_then(|v| v.as_array()) {
        found.extend(list.iter().filter_map(|v| v.as_string()).map(str::to_string));
    }
    if found.is_empty() {
        if let Some(dev) = info.and_then(|d| d.get("CFBundleDevelopmentRegion")).and_then(|v| v.as_string()) {
            found.push(dev.to_string());
        }
    }
    found.sort();
    found.dedup();
    found
}

/// The app's development region (`CFBundleDevelopmentRegion`), if it names one.
pub(crate) fn main_development_region(emu: &Emu) -> Option<String> {
    let info = host_info_plist(emu)?;
    info.as_dictionary()?.get("CFBundleDevelopmentRegion")?.as_string().map(str::to_string)
}

/// Convert a parsed plist value into Foundation objects in guest memory.
fn plist_to_guest(emu: &mut Emu, v: &plist::Value) -> Result<u64> {
    Ok(match v {
        plist::Value::String(s) => foundation::new_nsstring(emu, s)?,
        plist::Value::Boolean(b) => foundation::boolean(emu, *b),
        plist::Value::Integer(i) => foundation::new_number_i64(emu, i.as_signed().unwrap_or(i.as_unsigned().unwrap_or(0) as i64))?,
        plist::Value::Real(r) => foundation::new_number_f64(emu, *r)?,
        plist::Value::Array(items) => {
            let mut objs = Vec::new();
            for item in items {
                objs.push(plist_to_guest(emu, item)?);
            }
            foundation::new_array_of(emu, &objs)?
        }
        plist::Value::Dictionary(d) => {
            let mut pairs = Vec::new();
            for (k, item) in d {
                let key = foundation::new_nsstring(emu, k)?;
                pairs.push((key, plist_to_guest(emu, item)?));
            }
            foundation::new_dict_of(emu, &pairs)?
        }
        // Data and dates aren't bridged yet.
        _ => foundation::null(emu),
    })
}

/// The Info.plist in a bundle directory on the host.
fn read_info_plist(dir: &std::path::Path) -> Option<plist::Value> {
    let bytes = pomme_loader::vfs::read(&dir.join("Info.plist")).ok()?;
    plist::Value::from_reader(std::io::Cursor::new(bytes)).ok()
}

/// The bundle for a host directory (the .app or one inside it), made on first use.
fn bundle_for_dir(emu: &mut Emu, host: PathBuf) -> Result<u64> {
    if emu.bundle.bundles.is_empty() && host != host_app_dir(emu) {
        main_bundle(emu)?;
    }
    if let Some(b) = emu.bundle.bundles.iter().find(|b| b.host == host) {
        return Ok(b.obj);
    }
    let app = host_app_dir(emu);
    let guest = match host.strip_prefix(&app) {
        Ok(rel) if rel.as_os_str().is_empty() => guest_bundle_path(emu),
        Ok(rel) => format!("{}/{}", guest_bundle_path(emu), rel.to_string_lossy()),
        Err(_) => return Ok(0),
    };
    let cls = host_class(emu, "NSBundle")?;
    let obj = emu.alloc(16)?;
    emu.write_u64(obj, cls)?;
    let info = match read_info_plist(&host) {
        Some(v) => plist_to_guest(emu, &v)?,
        None => foundation::new_dict_of(emu, &[])?,
    };
    emu.bundle.bundles.push(Bundle { obj, host, guest, info });
    Ok(obj)
}

fn main_bundle(emu: &mut Emu) -> Result<u64> {
    if let Some(b) = emu.bundle.bundles.first() {
        return Ok(b.obj);
    }
    let app = host_app_dir(emu);
    bundle_for_dir(emu, app)
}

fn ret_main(emu: &mut Emu) -> Result<()> {
    let b = main_bundle(emu)?;
    emu.ret(b);
    Ok(())
}

/// The receiver (x0) among the bundles: its index (the main bundle's, 0, if unknown).
fn this(emu: &mut Emu) -> Result<usize> {
    main_bundle(emu)?;
    let obj = emu.arg(0);
    Ok(emu.bundle.bundles.iter().position(|b| b.obj == obj).unwrap_or(0))
}

fn info_dictionary(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    emu.ret(emu.bundle.bundles[i].info);
    Ok(())
}

fn info_value_of(emu: &mut Emu, i: usize, key: &str) -> Result<u64> {
    let k = foundation::new_nsstring(emu, key)?;
    let info = emu.bundle.bundles[i].info;
    foundation::dict_get(emu, info, k)
}

/// A value from the main bundle's Info.plist (0 if it hasn't one).
pub(crate) fn main_info_value(emu: &mut Emu, key: &str) -> Result<u64> {
    info_value(emu, key)
}

fn info_value(emu: &mut Emu, key: &str) -> Result<u64> {
    main_bundle(emu)?;
    info_value_of(emu, 0, key)
}

fn object_for_info_key(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let key = nsstring(emu, emu.arg(2))?;
    let v = info_value_of(emu, i, &key)?;
    emu.ret(v);
    Ok(())
}

fn bundle_identifier(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let v = info_value_of(emu, i, "CFBundleIdentifier")?;
    emu.ret(v);
    Ok(())
}

fn ret_string(emu: &mut Emu, s: &str) -> Result<()> {
    let r = foundation::new_nsstring(emu, s)?;
    emu.ret(r);
    Ok(())
}

fn bundle_path(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let p = emu.bundle.bundles[i].guest.clone();
    ret_string(emu, &p)
}

fn executable_path(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let name = if i == 0 {
        emu.p.main().name.clone()
    } else {
        let host = &emu.bundle.bundles[i].host;
        let exe = read_info_plist(host).and_then(|v| v.as_dictionary()?.get("CFBundleExecutable")?.as_string().map(str::to_string));
        exe.unwrap_or_else(|| host.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default())
    };
    let p = format!("{}/{name}", emu.bundle.bundles[i].guest);
    ret_string(emu, &p)
}

/// The resource's file name from `name` + `ext` (either may carry the extension).
fn resource_name(emu: &mut Emu, name: u64, ext: u64) -> Result<String> {
    let mut n = nsstring(emu, name)?;
    let e = nsstring(emu, ext)?;
    if !e.is_empty() {
        n = format!("{n}.{}", e.trim_start_matches('.'));
    }
    Ok(n)
}

/// A resource's guest path if the file is in the receiver's bundle.
fn resource_path(emu: &mut Emu) -> Result<Option<String>> {
    let i = this(emu)?;
    let file = resource_name(emu, emu.arg(2), emu.arg(3))?;
    let b = &emu.bundle.bundles[i];
    if file.is_empty() || !pomme_loader::vfs::exists(&b.host.join(&file)) {
        return Ok(None);
    }
    Ok(Some(format!("{}/{file}", b.guest)))
}

/// `-pathForResource:ofType:` → guest path if the file is in the bundle, else nil.
fn path_for_resource(emu: &mut Emu) -> Result<()> {
    match resource_path(emu)? {
        Some(p) => ret_string(emu, &p),
        None => nil(emu),
    }
}

/// `-URLForResource:withExtension:` → file URL if the file is in the bundle, else nil.
fn url_for_resource(emu: &mut Emu) -> Result<()> {
    let Some(p) = resource_path(emu)? else { return nil(emu) };
    let u = foundation::new_file_url(emu, &p, false)?;
    emu.ret(u);
    Ok(())
}

fn bundle_url(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let p = emu.bundle.bundles[i].guest.clone();
    let u = foundation::new_file_url(emu, &p, true)?;
    emu.ret(u);
    Ok(())
}

/// `appStoreReceiptURL`: where StoreKit keeps the receipt, beside the app in
/// its bundle container. pomme installs apps as development builds, whose
/// receipt is the sandbox one (and isn't there until StoreKit fetches it).
fn app_store_receipt_url(emu: &mut Emu) -> Result<()> {
    let p = format!("{}/StoreKit/sandboxReceipt", crate::fs::CONTAINER);
    let u = foundation::new_file_url(emu, &p, false)?;
    emu.ret(u);
    Ok(())
}

/// `+bundleForClass:`: the bundle of the framework the class is in (the
/// main bundle for the executable's classes, and any it can't place).
fn bundle_for_class(emu: &mut Emu) -> Result<()> {
    let cls = emu.arg(2);
    let dir = emu.p.image_at(cls).and_then(|img| img.path.parent()).filter(|d| d.extension().is_some_and(|e| e == "framework")).map(pomme_loader::vfs::canonicalize);
    let b = match dir {
        Some(d) if d.starts_with(host_app_dir(emu)) => bundle_for_dir(emu, d)?,
        _ => main_bundle(emu)?,
    };
    emu.ret(b);
    Ok(())
}

/// The app's frameworks (Frameworks/*.framework) on the host.
fn framework_dirs(emu: &Emu) -> Vec<PathBuf> {
    let dir = host_app_dir(emu).join("Frameworks");
    let entries = pomme_loader::vfs::read_dir(&dir).unwrap_or_default();
    let mut out: Vec<PathBuf> = entries.into_iter().filter(|(name, is_dir)| *is_dir && name.ends_with(".framework")).map(|(name, _)| dir.join(name)).collect();
    out.sort();
    out
}

fn localized_string(emu: &mut Emu) -> Result<()> {
    // No .strings tables yet: the value if given, else the key.
    let (key, value) = (emu.arg(2), emu.arg(3));
    let v = if value != 0 && !nsstring(emu, value)?.is_empty() { value } else { key };
    emu.ret(v);
    Ok(())
}

fn localizations(emu: &mut Emu) -> Result<()> {
    let en = foundation::new_nsstring(emu, "en")?;
    let a = foundation::new_array_of(emu, &[en])?;
    emu.ret(a);
    Ok(())
}

fn development_localization(emu: &mut Emu) -> Result<()> {
    ret_string(emu, "en")
}

/// `+bundleWithIdentifier:`: the main bundle or one of the app's frameworks.
fn bundle_for_identifier(emu: &mut Emu) -> Result<()> {
    let want = nsstring(emu, emu.arg(2))?;
    let id = info_value(emu, "CFBundleIdentifier")?;
    if nsstring(emu, id)? == want {
        return ret_main(emu);
    }
    for dir in framework_dirs(emu) {
        let id = read_info_plist(&dir).and_then(|v| v.as_dictionary()?.get("CFBundleIdentifier")?.as_string().map(str::to_string));
        if id.as_deref() == Some(want.as_str()) {
            let b = bundle_for_dir(emu, dir)?;
            emu.ret(b);
            return Ok(());
        }
    }
    nil(emu)
}

/// `-initWithPath:` / `+bundleWithPath:` — a directory in the app (the app
/// itself, a framework, a resource bundle), else nil.
fn bundle_with_path(emu: &mut Emu) -> Result<()> {
    let p = nsstring(emu, emu.arg(2))?;
    let p = p.trim_end_matches('/');
    let root = guest_bundle_path(emu);
    let Some(rel) = p.strip_prefix(&root) else { return nil(emu) };
    let host = host_app_dir(emu).join(rel.trim_start_matches('/'));
    if !pomme_loader::vfs::is_dir(&host) {
        return nil(emu);
    }
    let b = bundle_for_dir(emu, host)?;
    emu.ret(b);
    Ok(())
}

fn bundle_with_url(emu: &mut Emu) -> Result<()> {
    let u = emu.arg(2);
    let path = send(emu, u, "path", &[])?;
    emu.cpu.x[2] = path;
    bundle_with_path(emu)
}

fn main_only(emu: &mut Emu) -> Result<()> {
    let b = main_bundle(emu)?;
    let a = foundation::new_array_of(emu, &[b])?;
    emu.ret(a);
    Ok(())
}

fn nil(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSBundle", true, "mainBundle", ret_main),
    ("NSBundle", true, "allBundles", main_only),
    ("NSBundle", true, "allFrameworks", main_only),
    ("NSBundle", true, "bundleForClass:", bundle_for_class),
    ("NSBundle", true, "bundleWithIdentifier:", bundle_for_identifier),
    ("NSBundle", true, "bundleWithPath:", bundle_with_path),
    ("NSBundle", false, "initWithPath:", bundle_with_path),
    ("NSBundle", true, "bundleWithURL:", bundle_with_url),
    ("NSBundle", false, "initWithURL:", bundle_with_url),
    ("NSBundle", false, "infoDictionary", info_dictionary),
    ("NSBundle", false, "localizedInfoDictionary", info_dictionary),
    ("NSBundle", false, "objectForInfoDictionaryKey:", object_for_info_key),
    ("NSBundle", false, "bundleIdentifier", bundle_identifier),
    ("NSBundle", false, "bundlePath", bundle_path),
    ("NSBundle", false, "resourcePath", bundle_path),
    ("NSBundle", false, "executablePath", executable_path),
    ("NSBundle", false, "pathForResource:ofType:", path_for_resource),
    ("NSBundle", false, "pathForResource:ofType:inDirectory:", path_for_resource),
    ("NSBundle", false, "URLForResource:withExtension:", url_for_resource),
    ("NSBundle", false, "URLForResource:withExtension:subdirectory:", url_for_resource),
    ("NSBundle", false, "bundleURL", bundle_url),
    ("NSBundle", false, "appStoreReceiptURL", app_store_receipt_url),
    ("NSBundle", false, "privateFrameworksPath", |e| {
        let p = format!("{}/Frameworks", guest_bundle_path(e));
        ret_string(e, &p)
    }),
    ("NSBundle", false, "privateFrameworksURL", |e| {
        let p = format!("{}/Frameworks", guest_bundle_path(e));
        let u = foundation::new_file_url(e, &p, true)?;
        e.ret(u);
        Ok(())
    }),
    ("NSBundle", false, "resourceURL", bundle_url),
    ("NSBundle", false, "localizedStringForKey:value:table:", localized_string),
    ("NSBundle", false, "preferredLocalizations", localizations),
    ("NSBundle", false, "localizations", localizations),
    ("NSBundle", false, "developmentLocalization", development_localization),
];

/// `[[NSBundle bundleForClass:class] pathForResource:file ofType:nil]`, for
/// tests and tools: where the app finds `file` next to `class`'s code.
pub fn bundle_resource(emu: &mut Emu, class: &str, file: &str) -> Result<Option<String>> {
    let Some(&cls) = emu.objc.by_name.get(class) else { return Ok(None) };
    let ns_bundle = host_class(emu, "NSBundle")?;
    let b = send(emu, ns_bundle, "bundleForClass:", &[cls])?;
    let name = foundation::new_nsstring(emu, file)?;
    let p = send(emu, b, "pathForResource:ofType:", &[name, 0])?;
    if p == 0 { Ok(None) } else { nsstring(emu, p).map(Some) }
}

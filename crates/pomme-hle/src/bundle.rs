//! NSBundle: the app's main bundle and the bundles inside it (its frameworks,
//! resource bundles), each with its Info.plist as real Foundation objects.
//!
//! Inside the guest the app lives where iOS would put it
//! (`/private/var/containers/Bundle/Application/pomme/<name>.app`); resource
//! lookups check the bundle's directory on the host. `bundleForClass:` is the
//! bundle of the image the class is in: Flutter finds its ICU data in
//! Flutter.framework that way.
//!
//! Localizations are iOS's (fixtures/localized): a bundle picks its
//! localizations for the preferred languages with CoreFoundation's own
//! matching (`en-AU` → `en`, `zh-HK` → `zh-Hant`), else its development
//! language. A resource is found at the bundle's top level first, then in the
//! chosen localizations' folders, then in `Base.lproj`. A string comes from
//! the first table file found that way (`.stringsdict` before `.strings`),
//! with no fallback to another language; `InfoPlist.strings` translates
//! Info.plist values.

use crate::foundation::{self, nsstring};
use crate::objc::{host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;

const CONTAINER: &str = "/private/var/containers/Bundle/Application/pomme";

#[derive(Clone)]
struct Bundle {
    obj: u64,
    /// The bundle's directory on the host.
    host: PathBuf,
    /// Its path in the guest.
    guest: String,
    info: u64,
    /// Its preferred localizations, once chosen.
    preferred: Option<Vec<String>>,
    /// Its string tables by name, once read.
    tables: HashMap<String, Rc<Table>>,
    /// Plural formats handed out, by (table, key).
    plurals: HashMap<(String, String), u64>,
    /// `localizedInfoDictionary`, once made.
    localized_info: Option<u64>,
}

/// A string table: its `.strings` and `.stringsdict` entries.
#[derive(Default)]
struct Table {
    strings: HashMap<String, String>,
    plurals: HashMap<String, crate::localizable::PluralFormat>,
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
    emu.bundle.bundles.push(Bundle { obj, host, guest, info, preferred: None, tables: HashMap::new(), plurals: HashMap::new(), localized_info: None });
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

/// `localizedInfoDictionary`: the values `InfoPlist.strings` translates (only those).
fn localized_info_dictionary(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    if let Some(d) = emu.bundle.bundles[i].localized_info {
        emu.ret(d);
        return Ok(());
    }
    let t = table(emu, i, "InfoPlist")?;
    let mut entries: Vec<_> = t.strings.iter().collect();
    entries.sort();
    let mut pairs = Vec::new();
    for (k, v) in entries {
        pairs.push((foundation::new_nsstring(emu, k)?, foundation::new_nsstring(emu, v)?));
    }
    let d = foundation::new_dict_of(emu, &pairs)?;
    emu.bundle.bundles[i].localized_info = Some(d);
    emu.ret(d);
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
    if let Some(v) = table(emu, i, "InfoPlist")?.strings.get(&key).cloned() {
        return ret_string(emu, &v);
    }
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

/// Where a resource is in bundle `i`, from the bundle's directory: at the top
/// level (of `subdir`), else in the folder of each of `localizations` (the
/// bundle's preferred ones if None), else in `Base.lproj`.
fn find_resource(emu: &mut Emu, i: usize, file: &str, subdir: &str, localizations: Option<Vec<String>>) -> Result<Option<String>> {
    if file.is_empty() {
        return Ok(None);
    }
    let dir = if subdir.is_empty() { String::new() } else { format!("{}/", subdir.trim_matches('/')) };
    let mut candidates = vec![format!("{dir}{file}")];
    let locs = match localizations {
        Some(l) => l,
        None => preferred(emu, i)?,
    };
    for l in locs.iter().map(String::as_str).chain(["Base"]) {
        candidates.push(format!("{dir}{l}.lproj/{file}"));
    }
    let host = emu.bundle.bundles[i].host.clone();
    Ok(candidates.into_iter().find(|c| pomme_loader::vfs::exists(&host.join(c))))
}

/// A resource named by x2 (and its extension, x3), in subdirectory `subdir`
/// (an argument index, or none) and localization `loc` (an argument index, or none): its guest path.
fn resource_at(emu: &mut Emu, subdir: Option<usize>, loc: Option<usize>) -> Result<Option<String>> {
    let i = this(emu)?;
    let file = resource_name(emu, emu.arg(2), emu.arg(3))?;
    let subdir = match subdir {
        Some(k) => nsstring(emu, emu.arg(k))?,
        None => String::new(),
    };
    let locs = match loc {
        Some(k) if emu.arg(k) != 0 => Some(vec![nsstring(emu, emu.arg(k))?]),
        _ => None,
    };
    let found = find_resource(emu, i, &file, &subdir, locs)?;
    Ok(found.map(|rel| format!("{}/{rel}", emu.bundle.bundles[i].guest)))
}

fn ret_path(emu: &mut Emu, p: Option<String>) -> Result<()> {
    match p {
        Some(p) => ret_string(emu, &p),
        None => nil(emu),
    }
}

fn ret_url(emu: &mut Emu, p: Option<String>) -> Result<()> {
    let Some(p) = p else { return nil(emu) };
    let u = foundation::new_file_url(emu, &p, false)?;
    emu.ret(u);
    Ok(())
}

/// `-pathsForResourcesOfType:inDirectory:` (as paths) and
/// `-URLsForResourcesWithExtension:subdirectory:` (as URLs): the files with
/// the extension (x2) at the top level of the subdirectory (x3), in the
/// preferred localizations' folders and in `Base.lproj`.
fn resources_of_type(emu: &mut Emu, urls: bool) -> Result<()> {
    let i = this(emu)?;
    let ext = nsstring(emu, emu.arg(2))?;
    let ext = ext.trim_start_matches('.');
    let subdir = nsstring(emu, emu.arg(3))?;
    let dir = if subdir.is_empty() { String::new() } else { format!("{}/", subdir.trim_matches('/')) };
    let mut dirs = vec![dir.clone()];
    for l in preferred(emu, i)?.iter().map(String::as_str).chain(["Base"]) {
        dirs.push(format!("{dir}{l}.lproj/"));
    }
    dirs.dedup();
    let (host, guest) = (emu.bundle.bundles[i].host.clone(), emu.bundle.bundles[i].guest.clone());
    let mut objs = Vec::new();
    for d in dirs {
        let mut names: Vec<String> = pomme_loader::vfs::read_dir(&host.join(&d))
            .unwrap_or_default()
            .into_iter()
            .filter(|(n, is_dir)| !is_dir && (ext.is_empty() || n.rsplit_once('.').is_some_and(|(_, e)| e == ext)))
            .map(|(n, _)| n)
            .collect();
        names.sort();
        for n in names {
            let p = format!("{guest}/{d}{n}");
            objs.push(if urls { foundation::new_file_url(emu, &p, false)? } else { foundation::new_nsstring(emu, &p)? });
        }
    }
    let a = foundation::new_array_of(emu, &objs)?;
    emu.ret(a);
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

/// Bundle `i`'s table `name`: its `.strings` and `.stringsdict`, each from
/// the first file found as a resource (read once).
fn table(emu: &mut Emu, i: usize, name: &str) -> Result<Rc<Table>> {
    if let Some(t) = emu.bundle.bundles[i].tables.get(name) {
        return Ok(t.clone());
    }
    let host = emu.bundle.bundles[i].host.clone();
    let mut t = Table::default();
    if let Some(rel) = find_resource(emu, i, &format!("{name}.strings"), "", None)? {
        let bytes = pomme_loader::vfs::read(&host.join(rel)).unwrap_or_default();
        t.strings = crate::localizable::parse_strings(&bytes).unwrap_or_default();
    }
    if let Some(rel) = find_resource(emu, i, &format!("{name}.stringsdict"), "", None)? {
        let bytes = pomme_loader::vfs::read(&host.join(&rel)).unwrap_or_default();
        // (Plural rules are the language of the folder it's in.)
        let language = rel.split('/').find_map(|p| p.strip_suffix(".lproj")).map(str::to_string).or_else(|| development_region(emu, i)).unwrap_or_else(|| "en".into());
        t.plurals = crate::localizable::parse_stringsdict(&bytes, &language);
    }
    let t = Rc::new(t);
    emu.bundle.bundles[i].tables.insert(name.to_string(), t.clone());
    Ok(t)
}

/// `-localizedStringForKey:value:table:` (key x2, value x3, table x4; table
/// nil or empty: Localizable): the translation; else the value, unless it is
/// nil or empty; else the key.
fn localized_string(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let (key_obj, value) = (emu.arg(2), emu.arg(3));
    let key = nsstring(emu, key_obj)?;
    let name = if emu.arg(4) == 0 { String::new() } else { nsstring(emu, emu.arg(4))? };
    let name = if name.is_empty() { "Localizable".to_string() } else { name };
    let t = table(emu, i, &name)?;
    if let Some(pf) = t.plurals.get(&key) {
        if let Some(&o) = emu.bundle.bundles[i].plurals.get(&(name.clone(), key.clone())) {
            emu.ret(o);
            return Ok(());
        }
        // A plural format: it reads as its format key (`%#@n@`).
        let o = foundation::new_nsstring(emu, &pf.format)?;
        crate::localizable::register(emu, o, pf.clone())?;
        emu.bundle.bundles[i].plurals.insert((name, key), o);
        emu.ret(o);
        return Ok(());
    }
    if let Some(v) = t.strings.get(&key) {
        return ret_string(emu, v);
    }
    let v = if value != 0 && !nsstring(emu, value)?.is_empty() { value } else { key_obj };
    emu.ret(v);
    Ok(())
}

/// Bundle `i`'s localizations: its `.lproj` folders (Base too) and
/// Info.plist's `CFBundleLocalizations`; with neither, its development region.
fn localizations_of(emu: &mut Emu, i: usize) -> Vec<String> {
    let host = emu.bundle.bundles[i].host.clone();
    let mut found: Vec<String> = pomme_loader::vfs::read_dir(&host).unwrap_or_default().into_iter().filter_map(|(name, is_dir)| if is_dir { name.strip_suffix(".lproj").map(str::to_string) } else { None }).collect();
    let info = read_info_plist(&host);
    let info = info.as_ref().and_then(|v| v.as_dictionary());
    if let Some(list) = info.and_then(|d| d.get("CFBundleLocalizations")).and_then(|v| v.as_array()) {
        found.extend(list.iter().filter_map(|v| v.as_string()).map(str::to_string));
    }
    if found.is_empty() {
        found.extend(development_region(emu, i));
    }
    found.sort();
    found.dedup();
    found
}

/// Bundle `i`'s `CFBundleDevelopmentRegion`.
fn development_region(emu: &Emu, i: usize) -> Option<String> {
    let info = read_info_plist(&emu.bundle.bundles[i].host)?;
    info.as_dictionary()?.get("CFBundleDevelopmentRegion")?.as_string().map(str::to_string)
}

/// The localizations of `localizations` for the preferred languages `prefs`,
/// best first, by CoreFoundation's matching (`CFBundleCopyLocalizationsForPreferences`).
pub(crate) fn choose(emu: &mut Emu, localizations: &[String], prefs: &[String]) -> Result<Vec<String>> {
    let mut locs = Vec::new();
    for l in localizations {
        locs.push(foundation::new_nsstring(emu, l)?);
    }
    let mut ps = Vec::new();
    for p in prefs {
        ps.push(foundation::new_nsstring(emu, p)?);
    }
    let (locs, ps) = (foundation::new_array_of(emu, &locs)?, foundation::new_array_of(emu, &ps)?);
    let r = match foundation::cf_call(emu, "_CFBundleCopyLocalizationsForPreferences", &[locs, ps]) {
        Ok(r) => r,
        // (Without CoreFoundation: the same tag, else the same language.)
        Err(_) => return Ok(crate::locale::best_localization(prefs, localizations).into_iter().collect()),
    };
    let mut out = Vec::new();
    if r != 0 {
        let n = foundation::cf_call(emu, "_CFArrayGetCount", &[r])?;
        for k in 0..n {
            let s = foundation::cf_call(emu, "_CFArrayGetValueAtIndex", &[r, k])?;
            out.push(nsstring(emu, s)?);
        }
    }
    Ok(out)
}

/// Bundle `i`'s preferred localizations: its localizations (not Base) for
/// the preferred languages, then its development region.
fn preferred(emu: &mut Emu, i: usize) -> Result<Vec<String>> {
    if let Some(p) = &emu.bundle.bundles[i].preferred {
        return Ok(p.clone());
    }
    let locs: Vec<String> = localizations_of(emu, i).into_iter().filter(|l| l != "Base").collect();
    let dev = development_region(emu, i);
    let p = if locs.is_empty() {
        dev.into_iter().collect()
    } else {
        let mut prefs = crate::locale::preferred_language_list(emu)?;
        prefs.extend(dev);
        choose(emu, &locs, &prefs)?
    };
    emu.bundle.bundles[i].preferred = Some(p.clone());
    Ok(p)
}

/// A file or directory in the main bundle, found as resources are (top
/// level, the preferred localizations' folders, `Base.lproj`): its guest path.
pub(crate) fn main_resource(emu: &mut Emu, file: &str) -> Result<Option<String>> {
    main_bundle(emu)?;
    let found = find_resource(emu, 0, file, "", None)?;
    Ok(found.map(|rel| format!("{}/{rel}", emu.bundle.bundles[0].guest)))
}

/// The main bundle's first preferred localization (None without localizations).
pub(crate) fn main_preferred_localization(emu: &mut Emu) -> Result<Option<String>> {
    main_bundle(emu)?;
    Ok(preferred(emu, 0)?.into_iter().next())
}

fn ret_strings(emu: &mut Emu, items: &[String]) -> Result<()> {
    let mut objs = Vec::new();
    for s in items {
        objs.push(foundation::new_nsstring(emu, s)?);
    }
    let a = foundation::new_array_of(emu, &objs)?;
    emu.ret(a);
    Ok(())
}

fn localizations(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let l = localizations_of(emu, i);
    ret_strings(emu, &l)
}

fn preferred_localizations(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let l = preferred(emu, i)?;
    ret_strings(emu, &l)
}

fn development_localization(emu: &mut Emu) -> Result<()> {
    let i = this(emu)?;
    let dev = development_region(emu, i).unwrap_or_else(|| "en".into());
    ret_string(emu, &dev)
}

/// `+preferredLocalizationsFromArray:forPreferences:` (x2, x3), by
/// CoreFoundation's matching; with no preferences (and
/// `+preferredLocalizationsFromArray:`), for the main bundle's preferred
/// localizations, as CoreFoundation does (fixtures/localized: on an
/// Australian English phone, an app in `en` gets `en`, not `en-AU`).
fn preferred_from_array(emu: &mut Emu, with_prefs: bool) -> Result<()> {
    let (locs, prefs) = (emu.arg(2), if with_prefs { emu.arg(3) } else { 0 });
    let prefs = if prefs != 0 {
        prefs
    } else {
        main_bundle(emu)?;
        let main = preferred(emu, 0)?;
        if main.is_empty() {
            crate::locale::preferred_languages(emu)?
        } else {
            let mut objs = Vec::new();
            for l in &main {
                objs.push(foundation::new_nsstring(emu, l)?);
            }
            foundation::new_array_of(emu, &objs)?
        }
    };
    let r = foundation::cf_call(emu, "_CFBundleCopyLocalizationsForPreferences", &[locs, prefs])?;
    emu.ret(r);
    Ok(())
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
    ("NSBundle", false, "localizedInfoDictionary", localized_info_dictionary),
    ("NSBundle", false, "objectForInfoDictionaryKey:", object_for_info_key),
    ("NSBundle", false, "bundleIdentifier", bundle_identifier),
    ("NSBundle", false, "bundlePath", bundle_path),
    ("NSBundle", false, "resourcePath", bundle_path),
    ("NSBundle", false, "executablePath", executable_path),
    ("NSBundle", false, "pathForResource:ofType:", |e| {
        let p = resource_at(e, None, None)?;
        ret_path(e, p)
    }),
    ("NSBundle", false, "pathForResource:ofType:inDirectory:", |e| {
        let p = resource_at(e, Some(4), None)?;
        ret_path(e, p)
    }),
    ("NSBundle", false, "pathForResource:ofType:inDirectory:forLocalization:", |e| {
        let p = resource_at(e, Some(4), Some(5))?;
        ret_path(e, p)
    }),
    ("NSBundle", false, "URLForResource:withExtension:", |e| {
        let p = resource_at(e, None, None)?;
        ret_url(e, p)
    }),
    ("NSBundle", false, "URLForResource:withExtension:subdirectory:", |e| {
        let p = resource_at(e, Some(4), None)?;
        ret_url(e, p)
    }),
    ("NSBundle", false, "URLForResource:withExtension:subdirectory:localization:", |e| {
        let p = resource_at(e, Some(4), Some(5))?;
        ret_url(e, p)
    }),
    ("NSBundle", false, "pathsForResourcesOfType:inDirectory:", |e| resources_of_type(e, false)),
    ("NSBundle", false, "URLsForResourcesWithExtension:subdirectory:", |e| resources_of_type(e, true)),
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
    ("NSBundle", false, "preferredLocalizations", preferred_localizations),
    ("NSBundle", true, "preferredLocalizationsFromArray:", |e| preferred_from_array(e, false)),
    ("NSBundle", true, "preferredLocalizationsFromArray:forPreferences:", |e| preferred_from_array(e, true)),
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

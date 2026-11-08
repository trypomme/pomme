//! NSBundle: the app's main bundle, with its Info.plist as real Foundation objects.
//!
//! Inside the guest the app lives where iOS would put it
//! (`/private/var/containers/Bundle/Application/pomme/<name>.app`); resource
//! lookups check the `.app` directory on the host.

use crate::foundation::{self, nsstring};
use crate::objc::{host_class, send};
use crate::{Emu, HostFn, Result};
use std::path::PathBuf;

const CONTAINER: &str = "/private/var/containers/Bundle/Application/pomme";

#[derive(Default)]
pub struct State {
    main: u64,
    info: u64,
}

/// The `.app` directory on the host.
fn host_app_dir(emu: &Emu) -> PathBuf {
    emu.p.main().path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

fn guest_bundle_path(emu: &Emu) -> String {
    let dir = host_app_dir(emu);
    let name = dir.file_name().map_or_else(|| format!("{}.app", emu.p.main().name), |n| n.to_string_lossy().into_owned());
    format!("{CONTAINER}/{name}")
}

/// The app's Info.plist, parsed on the host.
pub(crate) fn host_info_plist(emu: &Emu) -> Option<plist::Value> {
    plist::Value::from_file(host_app_dir(emu).join("Info.plist")).ok()
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

fn main_bundle(emu: &mut Emu) -> Result<u64> {
    if emu.bundle.main == 0 {
        let cls = host_class(emu, "NSBundle")?;
        let b = emu.alloc(16)?;
        emu.write_u64(b, cls)?;
        emu.bundle.main = b;
        let info = match plist::Value::from_file(host_app_dir(emu).join("Info.plist")) {
            Ok(v) => plist_to_guest(emu, &v)?,
            Err(_) => foundation::new_dict_of(emu, &[])?,
        };
        emu.bundle.info = info;
    }
    Ok(emu.bundle.main)
}

fn ret_main(emu: &mut Emu) -> Result<()> {
    let b = main_bundle(emu)?;
    emu.ret(b);
    Ok(())
}

fn info_dictionary(emu: &mut Emu) -> Result<()> {
    main_bundle(emu)?;
    emu.ret(emu.bundle.info);
    Ok(())
}

fn info_value(emu: &mut Emu, key: &str) -> Result<u64> {
    main_bundle(emu)?;
    let k = foundation::new_nsstring(emu, key)?;
    foundation::dict_get(emu, emu.bundle.info, k)
}

fn object_for_info_key(emu: &mut Emu) -> Result<()> {
    let key = nsstring(emu, emu.arg(2))?;
    let v = info_value(emu, &key)?;
    emu.ret(v);
    Ok(())
}

fn bundle_identifier(emu: &mut Emu) -> Result<()> {
    let v = info_value(emu, "CFBundleIdentifier")?;
    emu.ret(v);
    Ok(())
}

fn ret_string(emu: &mut Emu, s: &str) -> Result<()> {
    let r = foundation::new_nsstring(emu, s)?;
    emu.ret(r);
    Ok(())
}

fn bundle_path(emu: &mut Emu) -> Result<()> {
    let p = guest_bundle_path(emu);
    ret_string(emu, &p)
}

fn executable_path(emu: &mut Emu) -> Result<()> {
    let p = format!("{}/{}", guest_bundle_path(emu), emu.p.main().name);
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

/// `-pathForResource:ofType:` → guest path if the file is in the .app, else nil.
fn path_for_resource(emu: &mut Emu) -> Result<()> {
    let file = resource_name(emu, emu.arg(2), emu.arg(3))?;
    if file.is_empty() || !host_app_dir(emu).join(&file).exists() {
        emu.ret(0);
        return Ok(());
    }
    let p = format!("{}/{file}", guest_bundle_path(emu));
    ret_string(emu, &p)
}

/// `-URLForResource:withExtension:` → file URL if the file is in the .app, else nil.
fn url_for_resource(emu: &mut Emu) -> Result<()> {
    let file = resource_name(emu, emu.arg(2), emu.arg(3))?;
    if file.is_empty() || !host_app_dir(emu).join(&file).exists() {
        emu.ret(0);
        return Ok(());
    }
    let p = format!("{}/{file}", guest_bundle_path(emu));
    let u = foundation::new_file_url(emu, &p, false)?;
    emu.ret(u);
    Ok(())
}

fn bundle_url(emu: &mut Emu) -> Result<()> {
    let p = guest_bundle_path(emu);
    let u = foundation::new_file_url(emu, &p, true)?;
    emu.ret(u);
    Ok(())
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

fn bundle_for_identifier(emu: &mut Emu) -> Result<()> {
    let want = nsstring(emu, emu.arg(2))?;
    let id = info_value(emu, "CFBundleIdentifier")?;
    let have = nsstring(emu, id)?;
    if want == have { ret_main(emu) } else { emu.ret(0); Ok(()) }
}

/// `-initWithPath:` / `+bundleWithPath:` — the main bundle if it's the app's path, else nil.
fn bundle_with_path(emu: &mut Emu) -> Result<()> {
    let p = nsstring(emu, emu.arg(2))?;
    if p.trim_end_matches('/') == guest_bundle_path(emu) { ret_main(emu) } else { nil(emu) }
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
    // Frameworks don't get bundles of their own yet: everything is the main bundle.
    ("NSBundle", true, "allBundles", main_only),
    ("NSBundle", true, "allFrameworks", main_only),
    // Framework bundles share the main bundle for now.
    ("NSBundle", true, "bundleForClass:", ret_main),
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
    ("NSBundle", false, "resourceURL", bundle_url),
    ("NSBundle", false, "localizedStringForKey:value:table:", localized_string),
    ("NSBundle", false, "preferredLocalizations", localizations),
    ("NSBundle", false, "localizations", localizations),
    ("NSBundle", false, "developmentLocalization", development_localization),
];

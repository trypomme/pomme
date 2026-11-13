//! NSFileManager's attributes, moving, copying and permission checks, over
//! pomme's filesystem (the read-only app bundle and the app's writable data
//! container, see [`crate::fs`]).
//!
//! Attributes are Foundation's dictionary (NSFileType, NSFileSize, the
//! creation and modification dates, permissions, owner, and the data
//! protection class, iOS's default for app files unless one was set).

use crate::foundation::{self, nsstring};
use crate::fs::{self, Attributes};
use crate::objc::{host_class, send};
use crate::{Emu, HostFn, Result};

/// Seconds from 1970 to 2001 (NSDate's reference date).
const REFERENCE_1970: f64 = 978_307_200.0;

/// The phone's storage, and what's free (as statfs reports it).
const DISK: u64 = fs::DISK_BYTES;
const FREE: u64 = fs::FREE_BYTES;

/// iOS's protection class for app files that don't ask for another.
const DEFAULT_PROTECTION: &str = "NSFileProtectionCompleteUntilFirstUserAuthentication";

// NSCocoaErrorDomain codes
const NO_SUCH_FILE: u64 = 4;
const WRITE_NO_PERMISSION: u64 = 513;
const WRITE_FILE_EXISTS: u64 = 516;

// errno
const ENOENT: u64 = 2;
const EEXIST: u64 = 17;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn set_error(emu: &mut Emu, out: u64, code: u64, path: &str) -> Result<()> {
    if out == 0 {
        return Ok(());
    }
    let domain = foundation::new_nsstring(emu, "NSCocoaErrorDomain")?;
    let (k, v) = (foundation::new_nsstring(emu, "NSFilePath")?, foundation::new_nsstring(emu, path)?);
    let info = foundation::new_dict_of(emu, &[(k, v)])?;
    let cls = host_class(emu, "NSError")?;
    let err = send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, code, info])?;
    emu.write_u64(out, err)
}

fn path_of(emu: &mut Emu, s: u64) -> Result<String> {
    if s == 0 { Ok(String::new()) } else { nsstring(emu, s) }
}

fn url_path(emu: &mut Emu, url: u64) -> Result<String> {
    if url == 0 {
        return Ok(String::new());
    }
    let p = send(emu, url, "path", &[])?;
    path_of(emu, p)
}

fn date(emu: &mut Emu, t_1970: f64) -> Result<u64> {
    foundation::new_date(emu, t_1970 - REFERENCE_1970)
}

/// The attributes dictionary for `a`.
fn attributes_dict(emu: &mut Emu, a: &Attributes) -> Result<u64> {
    let kind = if a.is_dir { "NSFileTypeDirectory" } else { "NSFileTypeRegular" };
    let protection = a.meta.protection.clone().unwrap_or_else(|| DEFAULT_PROTECTION.into());
    let mut pairs = Vec::new();
    let strings = [("NSFileType", kind.to_string()), ("NSFileProtectionKey", protection), ("NSFileOwnerAccountName", "mobile".into()), ("NSFileGroupOwnerAccountName", "mobile".into())];
    for (k, v) in strings {
        pairs.push((foundation::new_nsstring(emu, k)?, foundation::new_nsstring(emu, &v)?));
    }
    let numbers = [("NSFileSize", a.size as i64), ("NSFilePosixPermissions", a.meta.permissions as i64), ("NSFileReferenceCount", 1), ("NSFileOwnerAccountID", 501), ("NSFileGroupOwnerAccountID", 501)];
    for (k, v) in numbers {
        pairs.push((foundation::new_nsstring(emu, k)?, foundation::new_number_i64(emu, v)?));
    }
    for (k, t) in [("NSFileCreationDate", a.meta.created), ("NSFileModificationDate", a.meta.modified)] {
        pairs.push((foundation::new_nsstring(emu, k)?, date(emu, t)?));
    }
    let no = foundation::new_number_i64(emu, 0)?;
    pairs.push((foundation::new_nsstring(emu, "NSFileExtensionHidden")?, no));
    foundation::new_dict_of(emu, &pairs)
}

/// `attributesOfItemAtPath:error:`
fn attributes_of(emu: &mut Emu) -> Result<()> {
    let (p, error) = (emu.arg(2), emu.arg(3));
    let path = path_of(emu, p)?;
    match fs::attributes(emu, &path) {
        Some(a) => {
            let d = attributes_dict(emu, &a)?;
            ret(emu, d)
        }
        None => {
            set_error(emu, error, 260, &path)?; // NSFileReadNoSuchFileError
            ret(emu, 0)
        }
    }
}

/// A value in an attributes dictionary given to `setAttributes:`.
fn attribute(emu: &mut Emu, dict: u64, key: &str) -> Result<u64> {
    let k = foundation::new_nsstring(emu, key)?;
    send(emu, dict, "objectForKey:", &[k])
}

/// `setAttributes:ofItemAtPath:error:`: dates, permissions and the protection
/// class (what an app may change on its own files).
fn set_attributes(emu: &mut Emu) -> Result<()> {
    let (dict, p, error) = (emu.arg(2), emu.arg(3), emu.arg(4));
    let path = path_of(emu, p)?;
    let modified = attribute(emu, dict, "NSFileModificationDate")?;
    let created = attribute(emu, dict, "NSFileCreationDate")?;
    let permissions = attribute(emu, dict, "NSFilePosixPermissions")?;
    let protection = attribute(emu, dict, "NSFileProtectionKey")?;
    let modified = if modified == 0 { None } else { Some(foundation::date_value(emu, modified)? + REFERENCE_1970) };
    let created = if created == 0 { None } else { Some(foundation::date_value(emu, created)? + REFERENCE_1970) };
    let permissions = if permissions == 0 { None } else { Some(foundation::number_value(emu, permissions)?.0 as u16) };
    let protection = if protection == 0 { None } else { Some(nsstring(emu, protection)?) };
    let ok = fs::set_attributes(emu, &path, |m| {
        if let Some(t) = modified {
            m.modified = t;
        }
        if let Some(t) = created {
            m.created = t;
        }
        if let Some(p) = permissions {
            m.permissions = p & 0o7777;
        }
        if let Some(p) = protection {
            m.protection = Some(p);
        }
    });
    if !ok {
        let code = if fs::exists(emu, &path).is_some() { WRITE_NO_PERMISSION } else { NO_SUCH_FILE };
        set_error(emu, error, code, &path)?;
    }
    ret(emu, ok as u64)
}

/// Move or copy: (from, to, error out).
fn transfer(emu: &mut Emu, from: String, to: String, error: u64, copy: bool) -> Result<()> {
    let r = if copy { fs::copy_item(emu, &from, &to) } else { fs::move_item(emu, &from, &to) };
    match r {
        Ok(()) => ret(emu, 1),
        Err(errno) => {
            let code = match errno {
                ENOENT => NO_SUCH_FILE,
                EEXIST => WRITE_FILE_EXISTS,
                _ => WRITE_NO_PERMISSION,
            };
            set_error(emu, error, code, &from)?;
            ret(emu, 0)
        }
    }
}

/// Every path under `dir`, relative to it, depth first.
fn subpaths(emu: &mut Emu, dir: &str, prefix: &str, out: &mut Vec<String>) {
    for (name, is_dir) in fs::list_dir(emu, dir).unwrap_or_default() {
        let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
        out.push(rel.clone());
        if is_dir {
            subpaths(emu, &format!("{}/{name}", dir.trim_end_matches('/')), &rel, out);
        }
    }
}

fn subpaths_of(emu: &mut Emu, error: u64) -> Result<()> {
    let p = emu.arg(2);
    let path = path_of(emu, p)?;
    if fs::exists(emu, &path) != Some(true) {
        set_error(emu, error, 260, &path)?;
        return ret(emu, 0);
    }
    let mut all = Vec::new();
    subpaths(emu, &path, "", &mut all);
    let mut objs = Vec::new();
    for s in all {
        objs.push(foundation::new_nsstring(emu, &s)?);
    }
    let a = foundation::new_array_of(emu, &objs)?;
    ret(emu, a)
}

/// `is…FileAtPath:`: whether it exists, and is writable/deletable (in the data container).
fn can(emu: &mut Emu, write: bool) -> Result<()> {
    let p = emu.arg(2);
    let path = path_of(emu, p)?;
    let ok = match fs::attributes(emu, &path) {
        Some(a) => !write || !a.read_only,
        None => false,
    };
    ret(emu, ok as u64)
}

/// `attributesOfFileSystemForPath:error:`: the phone's storage.
fn file_system(emu: &mut Emu) -> Result<()> {
    let mut pairs = Vec::new();
    for (k, v) in [("NSFileSystemSize", DISK), ("NSFileSystemFreeSize", FREE), ("NSFileSystemNodes", DISK / 4096), ("NSFileSystemFreeNodes", FREE / 4096), ("NSFileSystemNumber", 1)] {
        pairs.push((foundation::new_nsstring(emu, k)?, foundation::new_number_i64(emu, v as i64)?));
    }
    let d = foundation::new_dict_of(emu, &pairs)?;
    ret(emu, d)
}

/// One of a file URL's resource values (NSURL…Key), from the file's metadata;
/// 0 (nil) when the item doesn't exist or the key isn't one pomme knows.
fn resource_value(emu: &mut Emu, path: &str, key: &str) -> Result<u64> {
    if key == "NSURLIsExcludedFromBackupKey" {
        let excluded = emu.fs_flags.excluded_from_backup.contains(path);
        return Ok(foundation::boolean(emu, excluded));
    }
    let Some(a) = fs::attributes(emu, path) else { return Ok(0) };
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    let blocks = a.size.div_ceil(4096) * 4096;
    Ok(match key {
        "NSURLIsDirectoryKey" => foundation::boolean(emu, a.is_dir),
        "NSURLIsRegularFileKey" => foundation::boolean(emu, !a.is_dir),
        "NSURLIsSymbolicLinkKey" | "NSURLIsPackageKey" | "NSURLIsAliasFileKey" => foundation::boolean(emu, false),
        "NSURLIsHiddenKey" => foundation::boolean(emu, name.starts_with('.')),
        "NSURLIsReadableKey" => foundation::boolean(emu, true),
        "NSURLIsWritableKey" => foundation::boolean(emu, !a.read_only),
        "NSURLFileSizeKey" if !a.is_dir => foundation::new_number_i64(emu, a.size as i64)?,
        "NSURLFileAllocatedSizeKey" | "NSURLTotalFileAllocatedSizeKey" | "NSURLTotalFileSizeKey" if !a.is_dir => foundation::new_number_i64(emu, blocks as i64)?,
        "NSURLCreationDateKey" => date(emu, a.meta.created)?,
        "NSURLContentModificationDateKey" | "NSURLAttributeModificationDateKey" | "NSURLContentAccessDateKey" => date(emu, a.meta.modified)?,
        "NSURLNameKey" | "NSURLLocalizedNameKey" => foundation::new_nsstring(emu, &name)?,
        "NSURLPathKey" => foundation::new_nsstring(emu, path)?,
        "NSURLFileResourceTypeKey" => foundation::new_nsstring(emu, if a.is_dir { "NSFileTypeDirectory" } else { "NSFileTypeRegular" })?,
        "NSURLFileProtectionKey" => {
            let p = a.meta.protection.clone().unwrap_or_else(|| DEFAULT_PROTECTION.into());
            foundation::new_nsstring(emu, &p)?
        }
        _ => 0,
    })
}

/// `getResourceValue:forKey:error:`: YES, with the value (nil for unknown keys); NO if the file isn't there.
fn get_resource_value(emu: &mut Emu) -> Result<()> {
    let (url, out, key, error) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4));
    let path = url_path(emu, url)?;
    if fs::exists(emu, &path).is_none() {
        set_error(emu, error, 260, &path)?;
        if out != 0 {
            emu.write_u64(out, 0)?;
        }
        return ret(emu, 0);
    }
    let key = nsstring(emu, key)?;
    let v = resource_value(emu, &path, &key)?;
    if out != 0 {
        emu.write_u64(out, v)?;
    }
    ret(emu, 1)
}

/// `resourceValuesForKeys:error:`: the values pomme knows, by key.
fn resource_values(emu: &mut Emu) -> Result<()> {
    let (url, keys, error) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let path = url_path(emu, url)?;
    if fs::exists(emu, &path).is_none() {
        set_error(emu, error, 260, &path)?;
        return ret(emu, 0);
    }
    let mut pairs = Vec::new();
    for k in foundation::array_values(emu, keys)? {
        let name = nsstring(emu, k)?;
        let v = resource_value(emu, &path, &name)?;
        if v != 0 {
            pairs.push((k, v));
        }
    }
    let d = foundation::new_dict_of(emu, &pairs)?;
    ret(emu, d)
}

/// `setResourceValue:forKey:error:`: excluded-from-backup is kept (read back
/// as set); the modification date and protection change the file's; other
/// keys change nothing pomme models.
fn set_resource(emu: &mut Emu, url: u64, key: u64, value: u64) -> Result<()> {
    let path = url_path(emu, url)?;
    let key = nsstring(emu, key)?;
    match key.as_str() {
        "NSURLIsExcludedFromBackupKey" => {
            let on = value != 0 && send(emu, value, "boolValue", &[])? & 0xff != 0;
            if on {
                emu.fs_flags.excluded_from_backup.insert(path);
            } else {
                emu.fs_flags.excluded_from_backup.remove(&path);
            }
        }
        "NSURLContentModificationDateKey" if value != 0 => {
            let t = foundation::date_value(emu, value)? + REFERENCE_1970;
            fs::set_attributes(emu, &path, |m| m.modified = t);
        }
        "NSURLFileProtectionKey" if value != 0 => {
            let p = nsstring(emu, value)?;
            fs::set_attributes(emu, &path, |m| m.protection = Some(p));
        }
        _ => {}
    }
    Ok(())
}

/// Flags pomme keeps about files that aren't in their metadata.
#[derive(Default)]
pub struct Flags {
    excluded_from_backup: std::collections::HashSet<String>,
}

const FM: &str = "NSFileManager";

/// NSDictionary's file-attribute accessors (NSFileAttributes): an object.
fn attribute_object(emu: &mut Emu, key: &str) -> Result<()> {
    let v = attribute(emu, emu.arg(0), key)?;
    ret(emu, v)
}

/// … or a number's integer value (0 when absent).
fn attribute_integer(emu: &mut Emu, key: &str) -> Result<()> {
    let v = attribute(emu, emu.arg(0), key)?;
    let n = if v == 0 { 0 } else { foundation::number_value(emu, v)?.0 };
    ret(emu, n as u64)
}

/// Remove an item (and what's in it): NSFileNoSuchFileError when there's
/// none, NSFileWriteNoPermissionError for the read-only app bundle.
fn remove_item(emu: &mut Emu, path: &str, error: u64) -> Result<()> {
    if crate::fs::remove(emu, path) {
        return ret(emu, 1);
    }
    let code = if crate::fs::data_path(path).is_none() && crate::fs::exists(emu, path).is_some() { WRITE_NO_PERMISSION } else { NO_SUCH_FILE };
    set_error(emu, error, code, path)?;
    ret(emu, 0)
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("NSFileManager", false, "removeItemAtPath:error:", |e| {
        let (path, error) = (path_of(e, e.arg(2))?, e.arg(3));
        remove_item(e, &path, error)
    }),
    ("NSFileManager", false, "removeItemAtURL:error:", |e| {
        let (path, error) = (url_path(e, e.arg(2))?, e.arg(3));
        remove_item(e, &path, error)
    }),
    ("NSDictionary", false, "fileSize", |e| attribute_integer(e, "NSFileSize")),
    ("NSDictionary", false, "fileType", |e| attribute_object(e, "NSFileType")),
    ("NSDictionary", false, "fileModificationDate", |e| attribute_object(e, "NSFileModificationDate")),
    ("NSDictionary", false, "fileCreationDate", |e| attribute_object(e, "NSFileCreationDate")),
    ("NSDictionary", false, "filePosixPermissions", |e| attribute_integer(e, "NSFilePosixPermissions")),
    ("NSDictionary", false, "fileOwnerAccountName", |e| attribute_object(e, "NSFileOwnerAccountName")),
    ("NSDictionary", false, "fileGroupOwnerAccountName", |e| attribute_object(e, "NSFileGroupOwnerAccountName")),
    ("NSDictionary", false, "fileOwnerAccountID", |e| attribute_object(e, "NSFileOwnerAccountID")),
    ("NSDictionary", false, "fileGroupOwnerAccountID", |e| attribute_object(e, "NSFileGroupOwnerAccountID")),
    ("NSDictionary", false, "fileSystemNumber", |e| attribute_integer(e, "NSFileSystemNumber")),
    ("NSDictionary", false, "fileSystemFileNumber", |e| attribute_integer(e, "NSFileSystemFileNumber")),
    ("NSDictionary", false, "fileExtensionHidden", |e| attribute_integer(e, "NSFileExtensionHidden")),
    ("NSDictionary", false, "fileIsImmutable", |e| attribute_integer(e, "NSFileImmutable")),
    ("NSDictionary", false, "fileIsAppendOnly", |e| attribute_integer(e, "NSFileAppendOnly")),
    ("NSURL", false, "getResourceValue:forKey:error:", get_resource_value),
    ("NSURL", false, "resourceValuesForKeys:error:", resource_values),
    ("NSURL", false, "setResourceValue:forKey:error:", |e| {
        let (url, value, key) = (e.arg(0), e.arg(2), e.arg(3));
        set_resource(e, url, key, value)?;
        ret(e, 1)
    }),
    ("NSURL", false, "setResourceValues:error:", |e| {
        let (url, d) = (e.arg(0), e.arg(2));
        let cf_dict = foundation::as_cf_dict(e, d)?;
        for (k, v) in foundation::dict_pairs(e, cf_dict)? {
            set_resource(e, url, k, v)?;
        }
        ret(e, 1)
    }),
    (FM, false, "attributesOfItemAtPath:error:", attributes_of),
    (FM, false, "setAttributes:ofItemAtPath:error:", set_attributes),
    (FM, false, "attributesOfFileSystemForPath:error:", file_system),
    (FM, false, "moveItemAtPath:toPath:error:", |e| {
        let (a, b, err) = (e.arg(2), e.arg(3), e.arg(4));
        let (from, to) = (path_of(e, a)?, path_of(e, b)?);
        transfer(e, from, to, err, false)
    }),
    (FM, false, "moveItemAtURL:toURL:error:", |e| {
        let (a, b, err) = (e.arg(2), e.arg(3), e.arg(4));
        let (from, to) = (url_path(e, a)?, url_path(e, b)?);
        transfer(e, from, to, err, false)
    }),
    (FM, false, "copyItemAtPath:toPath:error:", |e| {
        let (a, b, err) = (e.arg(2), e.arg(3), e.arg(4));
        let (from, to) = (path_of(e, a)?, path_of(e, b)?);
        transfer(e, from, to, err, true)
    }),
    (FM, false, "copyItemAtURL:toURL:error:", |e| {
        let (a, b, err) = (e.arg(2), e.arg(3), e.arg(4));
        let (from, to) = (url_path(e, a)?, url_path(e, b)?);
        transfer(e, from, to, err, true)
    }),
    (FM, false, "subpathsOfDirectoryAtPath:error:", |e| {
        let err = e.arg(3);
        subpaths_of(e, err)
    }),
    (FM, false, "subpathsAtPath:", |e| subpaths_of(e, 0)),
    // A deep enumerator over the paths under a directory, relative to it (nil if it isn't one).
    (FM, false, "enumeratorAtPath:", |e| {
        subpaths_of(e, 0)?;
        let all = e.cpu.x[0];
        let en = if all == 0 { 0 } else { send(e, all, "objectEnumerator", &[])? };
        ret(e, en)
    }),
    (FM, false, "isReadableFileAtPath:", |e| can(e, false)),
    (FM, false, "isWritableFileAtPath:", |e| can(e, true)),
    (FM, false, "isDeletableFileAtPath:", |e| can(e, true)),
    (FM, false, "isExecutableFileAtPath:", |e| ret(e, 0)),
];

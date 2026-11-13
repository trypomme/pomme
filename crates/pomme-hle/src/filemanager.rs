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

const FM: &str = "NSFileManager";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
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
    (FM, false, "isReadableFileAtPath:", |e| can(e, false)),
    (FM, false, "isWritableFileAtPath:", |e| can(e, true)),
    (FM, false, "isDeletableFileAtPath:", |e| can(e, true)),
    (FM, false, "isExecutableFileAtPath:", |e| ret(e, 0)),
];

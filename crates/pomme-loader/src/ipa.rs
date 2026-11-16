//! Opening an app from an `.ipa` file.
//!
//! An `.ipa` is a zip with the app at `Payload/<Name>.app`. pomme unpacks it
//! into a cache folder named by a hash of the file's contents and runs the
//! `.app` from there, so everything after this step sees an ordinary `.app`
//! on disk. A second open of the same file reuses the folder.
//!
//! Each entry's path is checked before anything is written: an absolute path,
//! a `..`, or a symbolic link pointing outside the app refuses the whole file.

use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Seek};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum IpaError {
    #[error("reading {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{0} isn't a valid .ipa: it isn't a zip file ({1})")]
    NotZip(PathBuf, String),
    #[error("{0} isn't a valid .ipa: it has no Payload/<Name>.app")]
    NoApp(PathBuf),
    #[error("{0} isn't a valid .ipa: it has more than one app in Payload ({1})")]
    ManyApps(PathBuf, String),
    #[error("{0} isn't a valid .ipa: the entry {1:?} would land outside its folder")]
    UnsafePath(PathBuf, String),
    #[error("{0} isn't a valid .ipa: the link {1:?} points outside the app")]
    UnsafeLink(PathBuf, String),
}

const ZIP_MAGIC: &[u8; 4] = b"PK\x03\x04";
const S_IFMT: u32 = 0o170000;
const S_IFLNK: u32 = 0o120000;

/// The `.app` to run for `path`: `path` itself unless it's an `.ipa` (by its
/// extension or a zip's signature), which is unpacked into the cache.
pub fn open_app(path: &Path) -> Result<PathBuf, IpaError> {
    if path.is_dir() || !path.is_file() {
        return Ok(path.to_path_buf());
    }
    let is_ipa = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("ipa"));
    let mut head = [0u8; 4];
    let looks_zip = fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == ZIP_MAGIC;
    if !is_ipa && !looks_zip {
        return Ok(path.to_path_buf());
    }
    let bytes = fs::read(path).map_err(|source| IpaError::Io { path: path.into(), source })?;
    unpack(&bytes, &cache_root(), path)
}

/// Where unpacked `.ipa`s go: `$POMME_CACHE`, else the platform's cache folder, then `pomme/ipa`.
pub fn cache_root() -> PathBuf {
    if let Some(p) = std::env::var_os("POMME_CACHE") {
        return PathBuf::from(p).join("ipa");
    }
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Caches"))
    } else if cfg!(windows) {
        env("LOCALAPPDATA")
    } else {
        env("XDG_CACHE_HOME").or_else(|| env("HOME").map(|h| h.join(".cache")))
    };
    base.unwrap_or_else(std::env::temp_dir).join("pomme").join("ipa")
}

/// One entry to write, after checking.
enum Item {
    Dir(PathBuf),
    File { index: usize, rel: PathBuf, mode: Option<u32> },
    Link { rel: PathBuf, target: String },
}

/// Unpack `bytes` (an `.ipa` read from `source`, named in errors) under `root`,
/// reusing an earlier unpack of the same contents. Returns the `.app` path.
pub fn unpack(bytes: &[u8], root: &Path, source: &Path) -> Result<PathBuf, IpaError> {
    let hash: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    let dest = root.join(&hash);
    if let Some(app) = find_app(&dest) {
        return Ok(app);
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| IpaError::NotZip(source.into(), e.to_string()))?;
    let (app_name, items) = plan(&mut zip, source)?;

    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| IpaError::Io { path, source }
    };
    fs::create_dir_all(root).map_err(io(root))?;
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let tmp = root.join(format!("{hash}.tmp-{}-{nanos}", std::process::id()));
    let app = tmp.join(&app_name);
    let result = write(&mut zip, &app, &items, source);
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&tmp);
        return Err(e);
    }
    if fs::rename(&tmp, &dest).is_err() {
        // Another run unpacked the same file first: use its folder.
        let _ = fs::remove_dir_all(&tmp);
        if find_app(&dest).is_none() {
            return Err(IpaError::Io { path: dest, source: std::io::Error::other("couldn't move the unpacked app into place") });
        }
    }
    Ok(dest.join(app_name))
}

/// The one `.app` in an unpacked folder, if it's there.
fn find_app(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir).ok()?.flatten().map(|e| e.path()).find(|p| p.extension().is_some_and(|e| e == "app") && p.is_dir())
}

/// A zip entry's path as normal components, or None if it's absolute or climbs out.
fn safe_rel(name: &str) -> Option<PathBuf> {
    let name = name.replace('\\', "/");
    if name.starts_with('/') || name.as_bytes().get(1) == Some(&b':') {
        return None;
    }
    let mut out = PathBuf::new();
    for c in Path::new(&name).components() {
        match c {
            Component::Normal(s) => out.push(s),
            Component::CurDir => {}
            _ => return None,
        }
    }
    Some(out)
}

/// Check every entry and decide what to write, relative to the `.app`.
fn plan<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, source: &Path) -> Result<(String, Vec<Item>), IpaError> {
    let mut apps: Vec<String> = Vec::new();
    let mut items = Vec::new();
    for index in 0..zip.len() {
        let mut f = zip.by_index(index).map_err(|e| IpaError::NotZip(source.into(), e.to_string()))?;
        let name = f.name().to_string();
        let rel = safe_rel(&name).ok_or_else(|| IpaError::UnsafePath(source.into(), name.clone()))?;
        let mut parts = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
        if parts.next().as_deref() != Some("Payload") {
            continue; // Symbols, SwiftSupport, META-INF, iTunesMetadata.plist, __MACOSX
        }
        let Some(app) = parts.next().filter(|a| a.ends_with(".app")) else { continue };
        if !apps.contains(&app) {
            apps.push(app.clone());
        }
        let inner: PathBuf = parts.collect();
        let mode = f.unix_mode();
        if mode.is_some_and(|m| m & S_IFMT == S_IFLNK) {
            let mut target = String::new();
            f.read_to_string(&mut target).map_err(|e| IpaError::NotZip(source.into(), e.to_string()))?;
            // The link must resolve inside the app.
            let resolved = inner.parent().unwrap_or(Path::new("")).join(&target);
            let mut depth: i32 = 0;
            let inside = !target.starts_with('/')
                && resolved.components().all(|c| {
                    match c {
                        Component::ParentDir => depth -= 1,
                        Component::Normal(_) => depth += 1,
                        _ => {}
                    }
                    depth >= 0
                });
            if !inside {
                return Err(IpaError::UnsafeLink(source.into(), name));
            }
            items.push(Item::Link { rel: inner, target });
        } else if f.is_dir() {
            items.push(Item::Dir(inner));
        } else {
            items.push(Item::File { index, rel: inner, mode });
        }
    }
    match apps.len() {
        0 => Err(IpaError::NoApp(source.into())),
        1 => Ok((apps.remove(0), items)),
        _ => Err(IpaError::ManyApps(source.into(), apps.join(", "))),
    }
}

fn write<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, app: &Path, items: &[Item], source: &Path) -> Result<(), IpaError> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| IpaError::Io { path, source }
    };
    fs::create_dir_all(app).map_err(io(app))?;
    let mut links = Vec::new();
    for item in items {
        match item {
            Item::Dir(rel) => {
                let p = app.join(rel);
                fs::create_dir_all(&p).map_err(io(&p))?;
            }
            Item::File { index, rel, mode } => {
                let p = app.join(rel);
                if let Some(parent) = p.parent() {
                    fs::create_dir_all(parent).map_err(io(parent))?;
                }
                let mut f = zip.by_index(*index).map_err(|e| IpaError::NotZip(source.into(), e.to_string()))?;
                let mut out = fs::File::create(&p).map_err(io(&p))?;
                std::io::copy(&mut f, &mut out).map_err(io(&p))?;
                #[cfg(unix)]
                if let Some(m) = mode {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&p, fs::Permissions::from_mode(m & 0o777)).map_err(io(&p))?;
                }
                #[cfg(not(unix))]
                let _ = mode;
            }
            Item::Link { rel, target } => links.push((rel, target)),
        }
    }
    // Links last, so a copy (where links can't be made) finds its target.
    for (rel, target) in links {
        let p = app.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(io(parent))?;
        }
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, &p).map_err(io(&p))?;
        #[cfg(not(unix))]
        {
            let from = p.parent().unwrap_or(app).join(target);
            copy_tree(&from, &p).map_err(io(&p))?;
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn copy_tree(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        fs::create_dir_all(to)?;
        for e in fs::read_dir(from)? {
            let e = e?;
            copy_tree(&e.path(), &to.join(e.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(from, to).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A zip of (name, contents, unix mode); a mode with S_IFLNK makes a link to `contents`.
    fn zip_of(entries: &[(&str, &str, u32)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, body, mode) in entries {
            let opts = zip::write::SimpleFileOptions::default().unix_permissions(*mode);
            if mode & S_IFMT == S_IFLNK {
                w.add_symlink(*name, *body, opts).unwrap();
            } else if name.ends_with('/') {
                w.add_directory(*name, opts).unwrap();
            } else {
                w.start_file(*name, opts).unwrap();
                w.write_all(body.as_bytes()).unwrap();
            }
        }
        w.finish().unwrap().into_inner()
    }

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("pomme-ipa-test-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn unpacks_the_app_and_ignores_the_rest() {
        let root = scratch("good");
        let bytes = zip_of(&[
            ("Payload/Hello.app/Info.plist", "plist", 0o644),
            ("Payload/Hello.app/Hello", "binary", 0o755),
            ("SwiftSupport/iphoneos/libswiftCore.dylib", "x", 0o644),
            ("Symbols/abc.symbols", "x", 0o644),
        ]);
        let app = unpack(&bytes, &root, Path::new("Hello.ipa")).unwrap();
        assert!(app.ends_with("Hello.app"));
        assert_eq!(fs::read_to_string(app.join("Info.plist")).unwrap(), "plist");
        assert!(!app.parent().unwrap().join("SwiftSupport").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(app.join("Hello")).unwrap().permissions().mode() & 0o777, 0o755);
        }
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_same_bytes_unpack_once() {
        let root = scratch("reuse");
        let bytes = zip_of(&[("Payload/Hello.app/Hello", "binary", 0o755)]);
        let a = unpack(&bytes, &root, Path::new("a.ipa")).unwrap();
        fs::write(a.join("marker"), "kept").unwrap();
        let b = unpack(&bytes, &root, Path::new("b.ipa")).unwrap();
        assert_eq!(a, b);
        assert!(b.join("marker").exists(), "the second open reused the folder");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let other = zip_of(&[("Payload/Hello.app/Hello", "changed", 0o755)]);
        let c = unpack(&other, &root, Path::new("a.ipa")).unwrap();
        assert_ne!(a, c);
        assert_eq!(fs::read_to_string(c.join("Hello")).unwrap(), "changed");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn refuses_bad_packages() {
        let root = scratch("bad");
        let p = Path::new("bad.ipa");
        assert!(matches!(unpack(b"not a zip", &root, p), Err(IpaError::NotZip(..))));
        assert!(matches!(unpack(&zip_of(&[("Other/x", "x", 0o644)]), &root, p), Err(IpaError::NoApp(_))));
        let two = zip_of(&[("Payload/A.app/A", "a", 0o755), ("Payload/B.app/B", "b", 0o755)]);
        assert!(matches!(unpack(&two, &root, p), Err(IpaError::ManyApps(..))));
        let climb = zip_of(&[("Payload/A.app/A", "a", 0o755), ("Payload/../../evil", "x", 0o644)]);
        assert!(matches!(unpack(&climb, &root, p), Err(IpaError::UnsafePath(..))));
        let abs = zip_of(&[("/etc/evil", "x", 0o644)]);
        assert!(matches!(unpack(&abs, &root, p), Err(IpaError::UnsafePath(..))));
        let link = zip_of(&[("Payload/A.app/A", "a", 0o755), ("Payload/A.app/out", "../../../etc", S_IFLNK | 0o777)]);
        assert!(matches!(unpack(&link, &root, p), Err(IpaError::UnsafeLink(..))));
        // Nothing was written for any of them.
        assert!(fs::read_dir(&root).map(|d| d.count()).unwrap_or(0) == 0);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn keeps_framework_links() {
        let root = scratch("links");
        let bytes = zip_of(&[
            ("Payload/A.app/A", "a", 0o755),
            ("Payload/A.app/Frameworks/F.framework/Versions/A/F", "framework", 0o755),
            ("Payload/A.app/Frameworks/F.framework/Versions/Current", "A", S_IFLNK | 0o777),
            ("Payload/A.app/Frameworks/F.framework/F", "Versions/Current/F", S_IFLNK | 0o777),
        ]);
        let app = unpack(&bytes, &root, Path::new("A.ipa")).unwrap();
        assert_eq!(fs::read_to_string(app.join("Frameworks/F.framework/F")).unwrap(), "framework");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_app_folder_is_returned_as_is() {
        let dir = scratch("dir");
        fs::create_dir_all(dir.join("X.app")).unwrap();
        assert_eq!(open_app(&dir.join("X.app")).unwrap(), dir.join("X.app"));
        let _ = fs::remove_dir_all(&dir);
    }
}

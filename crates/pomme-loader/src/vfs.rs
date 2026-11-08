//! Where pomme reads files from: the host disk, or a bundle mounted in memory.
//!
//! Native runs read the `.app` and the sysroot from disk. In the browser
//! there is no disk: the host fetches one bundle (the app, the sysroot
//! libraries it loads, the font) and [`mount`]s it, after which every read
//! (loader, virtual filesystem, fonts) is served from memory.
//!
//! Bundle format, little-endian: `POMMEFS1`, then entries until the end:
//! `u8 kind` (0 file, 1 symlink), `u32` path length, path (absolute, UTF-8),
//! then for a file `u64` length and the bytes, for a symlink `u32` length and
//! the absolute target path.

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;

const MAGIC: &[u8; 8] = b"POMMEFS1";

#[derive(Default)]
struct Memory {
    files: HashMap<String, Vec<u8>>,
    links: HashMap<String, String>,
    dirs: HashSet<String>,
}

static MOUNTED: OnceLock<Memory> = OnceLock::new();

fn memory() -> Option<&'static Memory> {
    MOUNTED.get()
}

/// One bundle entry, for [`write_bundle`].
pub enum Entry {
    File(Vec<u8>),
    Symlink(String),
}

/// Serialize entries (absolute path → file or symlink) into a bundle.
pub fn write_bundle(entries: &[(String, Entry)]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    for (path, e) in entries {
        let kind: u8 = matches!(e, Entry::Symlink(_)) as u8;
        out.push(kind);
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        match e {
            Entry::File(data) => {
                out.extend_from_slice(&(data.len() as u64).to_le_bytes());
                out.extend_from_slice(data);
            }
            Entry::Symlink(target) => {
                out.extend_from_slice(&(target.len() as u32).to_le_bytes());
                out.extend_from_slice(target.as_bytes());
            }
        }
    }
    out
}

fn parse(bytes: &[u8]) -> Result<Memory, String> {
    let rest = bytes.strip_prefix(MAGIC).ok_or("not a pomme bundle")?;
    let mut m = Memory::default();
    let mut i = 0;
    let take = |i: &mut usize, n: usize| -> Result<&[u8], String> {
        let s = rest.get(*i..*i + n).ok_or("truncated bundle")?;
        *i += n;
        Ok(s)
    };
    while i < rest.len() {
        let kind = take(&mut i, 1)?[0];
        let n = u32::from_le_bytes(take(&mut i, 4)?.try_into().unwrap()) as usize;
        let path = String::from_utf8(take(&mut i, n)?.to_vec()).map_err(|e| e.to_string())?;
        if kind == 0 {
            let n = u64::from_le_bytes(take(&mut i, 8)?.try_into().unwrap()) as usize;
            m.files.insert(path.clone(), take(&mut i, n)?.to_vec());
        } else {
            let n = u32::from_le_bytes(take(&mut i, 4)?.try_into().unwrap()) as usize;
            let target = String::from_utf8(take(&mut i, n)?.to_vec()).map_err(|e| e.to_string())?;
            m.links.insert(path.clone(), target);
        }
        // Every ancestor is a directory.
        let mut p = Path::new(&path).parent();
        while let Some(d) = p {
            m.dirs.insert(d.to_string_lossy().into_owned());
            p = d.parent();
        }
    }
    Ok(m)
}

/// Serve every read from this bundle from now on. Only the first mount counts.
pub fn mount(bytes: &[u8]) -> Result<(), String> {
    let m = parse(bytes)?;
    MOUNTED.set(m).map_err(|_| "a bundle is already mounted".to_string())
}

/// Lexically normalize, then follow the bundle's symlinks.
fn resolve(m: &Memory, path: &Path) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut pending: Vec<String> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            Component::ParentDir => Some("..".into()),
            _ => None,
        })
        .rev()
        .collect();
    let mut hops = 0;
    while let Some(c) = pending.pop() {
        if c == ".." {
            parts.pop();
            continue;
        }
        parts.push(c);
        let so_far = format!("/{}", parts.join("/"));
        if let Some(target) = m.links.get(&so_far)
            && hops < 32
        {
            hops += 1;
            parts.clear();
            let t: Vec<String> = Path::new(target)
                .components()
                .filter_map(|c| match c {
                    Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    Component::ParentDir => Some("..".into()),
                    _ => None,
                })
                .collect();
            pending.extend(t.into_iter().rev());
        }
    }
    format!("/{}", parts.join("/"))
}

pub fn read(path: &Path) -> io::Result<Vec<u8>> {
    match memory() {
        Some(m) => m.files.get(&resolve(m, path)).cloned().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{} isn't in the bundle", path.display()))),
        None => std::fs::read(path),
    }
}

pub fn is_file(path: &Path) -> bool {
    match memory() {
        Some(m) => m.files.contains_key(&resolve(m, path)),
        None => path.is_file(),
    }
}

pub fn is_dir(path: &Path) -> bool {
    match memory() {
        Some(m) => m.dirs.contains(&resolve(m, path)),
        None => path.is_dir(),
    }
}

pub fn exists(path: &Path) -> bool {
    is_file(path) || is_dir(path)
}

/// File size (directories: 0).
pub fn len(path: &Path) -> Option<u64> {
    match memory() {
        Some(m) => {
            let p = resolve(m, path);
            m.files.get(&p).map(|f| f.len() as u64).or_else(|| m.dirs.contains(&p).then_some(0))
        }
        None => std::fs::metadata(path).ok().map(|md| if md.is_dir() { 0 } else { md.len() }),
    }
}

/// The path with symlinks resolved (the path itself if that fails).
pub fn canonicalize(path: &Path) -> PathBuf {
    match memory() {
        Some(m) => PathBuf::from(resolve(m, path)),
        None => path.canonicalize().unwrap_or_else(|_| path.to_path_buf()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_links() {
        let b = write_bundle(&[
            ("/sys/usr/lib/libc++.1.0.dylib".into(), Entry::File(vec![1, 2, 3])),
            ("/sys/usr/lib/libc++.1.dylib".into(), Entry::Symlink("/sys/usr/lib/libc++.1.0.dylib".into())),
        ]);
        let m = parse(&b).unwrap();
        assert_eq!(resolve(&m, Path::new("/sys/usr/lib/libc++.1.dylib")), "/sys/usr/lib/libc++.1.0.dylib");
        assert_eq!(resolve(&m, Path::new("/sys/usr/./lib/../lib/libc++.1.dylib")), "/sys/usr/lib/libc++.1.0.dylib");
        assert!(m.dirs.contains("/sys/usr"));
        assert_eq!(m.files["/sys/usr/lib/libc++.1.0.dylib"], vec![1, 2, 3]);
    }
}

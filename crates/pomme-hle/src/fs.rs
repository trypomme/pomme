//! A virtual filesystem.
//!
//! Inside the guest the app lives where iOS puts it
//! (`/private/var/containers/Bundle/Application/pomme/<name>.app`). That
//! directory maps, read-only, onto the `.app` on the host. The app's data
//! container (`/private/var/mobile/Containers/Data/Application/pomme`, its
//! HOME) is writable and lives in memory: each run starts from a fresh
//! install. Files are read whole into host memory when opened, so reads are
//! deterministic even if the host file changes later.

use crate::{Emu, HostFn, Result};
use pomme_loader::vfs;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

pub const CONTAINER: &str = "/private/var/containers/Bundle/Application/pomme";
const ENOENT: u64 = 2;
const EBADF: u64 = 9;
const EACCES: u64 = 13;
const EEXIST: u64 = 17;
const ENOTDIR: u64 = 20;

pub struct OpenFile {
    data: Vec<u8>,
    pos: usize,
    /// A file in the data container, written back on every write.
    path: Option<String>,
    append: bool,
}

#[derive(Default)]
pub struct State {
    files: HashMap<u64, OpenFile>,
    next_fd: u64,
    /// FILE* → fd
    streams: HashMap<u64, u64>,
    /// The data container's files and directories (made on first use).
    data: Option<Container>,
    /// Pipe ends: fd → (pipe, is the read end).
    pipe_ends: HashMap<u64, (u64, bool)>,
    pipes: HashMap<u64, Pipe>,
    next_pipe: u64,
    /// File status flags set with fcntl (O_NONBLOCK).
    status_flags: HashMap<u64, u64>,
    /// Directory fds → their guest path (for openat).
    dir_fds: HashMap<u64, String>,
    /// Descriptors open on character devices.
    devices: HashMap<u64, Device>,
    /// Directory streams (DIR *) → their entries and position.
    dirs: HashMap<u64, DirStream>,
    /// kqueue fd → its registered events.
    kqueues: HashMap<u64, Vec<KEvent>>,
}

#[derive(Default)]
pub struct Pipe {
    data: std::collections::VecDeque<u8>,
    readers: usize,
    writers: usize,
}

/// A kqueue registration (struct kevent, 32 bytes on arm64).
#[derive(Clone, Debug)]
pub struct KEvent {
    ident: u64,
    filter: i16,
    flags: u16,
    fflags: u32,
    data: i64,
    udata: u64,
    enabled: bool,
    /// EVFILT_USER: triggered and not yet reported.
    triggered: bool,
}

const EAGAIN: u64 = 35;
const EINVAL: u64 = 22;
const O_NONBLOCK: u64 = 4;
const EVFILT_READ: i16 = -1;
const EVFILT_WRITE: i16 = -2;
const EVFILT_USER: i16 = -10;
const EV_ADD: u16 = 0x1;
const EV_DELETE: u16 = 0x2;
const EV_ENABLE: u16 = 0x4;
const EV_DISABLE: u16 = 0x8;
const EV_ONESHOT: u16 = 0x10;
const EV_CLEAR: u16 = 0x20;
const EV_EOF: u16 = 0x8000;
const NOTE_TRIGGER: u32 = 0x0100_0000;

/// The character devices apps open.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Device {
    /// /dev/random and /dev/urandom: pomme's seeded generator.
    Random,
    /// /dev/null: reads end at once; writes vanish.
    Null,
    /// /dev/zero: zeros.
    Zero,
}

fn device(path: &str) -> Option<Device> {
    Some(match path {
        "/dev/random" | "/dev/urandom" => Device::Random,
        "/dev/null" => Device::Null,
        "/dev/zero" => Device::Zero,
        _ => return None,
    })
}

#[derive(Default)]
pub struct Container {
    files: HashMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
    /// Dates, permissions and protection of what's been made or changed;
    /// anything else dates from the install (boot).
    meta: HashMap<String, Meta>,
}

/// A file's or directory's metadata. Times are seconds since 1970.
#[derive(Clone, Debug, PartialEq)]
pub struct Meta {
    pub created: f64,
    pub modified: f64,
    /// The permission bits (0o644 for files, 0o755 for directories).
    pub permissions: u16,
    /// The data protection class (NSFileProtection…), if one was set.
    pub protection: Option<String>,
}

impl Meta {
    fn new(at: f64, dir: bool) -> Self {
        Meta { created: at, modified: at, permissions: if dir { 0o755 } else { 0o644 }, protection: None }
    }
}

/// What `stat` and NSFileManager report about a file or directory.
#[derive(Clone, Debug, PartialEq)]
pub struct Attributes {
    pub size: u64,
    pub is_dir: bool,
    pub meta: Meta,
    /// In the read-only app bundle.
    pub read_only: bool,
}

/// Now, in seconds since 1970 (the guest's clock).
fn now_1970(emu: &Emu) -> f64 {
    crate::libsystem::boot_time(emu) as f64 + crate::threads::now_ns(emu) as f64 / 1e9
}

/// When the app was installed: its boot.
fn install_time(emu: &Emu) -> f64 {
    crate::libsystem::boot_time(emu) as f64
}

impl Container {
    /// `p` was written (or made) at `at`.
    fn touch(&mut self, p: &str, at: f64) {
        let dir = self.dirs.contains(p);
        self.meta.entry(p.to_string()).or_insert_with(|| Meta::new(at, dir)).modified = at;
    }

    /// `p`'s metadata; `installed` dates what the app shipped with.
    fn meta_of(&self, p: &str, installed: f64) -> Meta {
        self.meta.get(p).cloned().unwrap_or_else(|| Meta::new(installed, self.dirs.contains(p)))
    }

    /// Forget `p` and everything under it.
    fn forget(&mut self, p: &str) {
        let prefix = format!("{p}/");
        self.meta.retain(|k, _| k != p && !k.starts_with(&prefix));
    }
}

/// A fresh install's data container.
fn container(emu: &mut Emu) -> &mut Container {
    emu.fs.data.get_or_insert_with(|| {
        let home = crate::foundation::DATA_CONTAINER;
        let mut dirs = BTreeSet::new();
        for d in ["", "/Documents", "/Library", "/Library/Caches", "/Library/Application Support", "/Library/Preferences", "/SystemData", "/tmp"] {
            dirs.insert(format!("{home}{d}"));
        }
        Container { files: HashMap::new(), dirs, meta: HashMap::new() }
    })
}

/// A normalized guest path in the data container (or /tmp), if it is in one.
pub fn data_path(path: &str) -> Option<String> {
    // Repeated slashes are one ("tmp//a" is "tmp/a"), as in any POSIX path.
    let mut path = path.to_string();
    while path.contains("//") {
        path = path.replace("//", "/");
    }
    let mut p = if path.starts_with("/var/") || path.starts_with("/tmp") { format!("/private{path}") } else { path.clone() };
    while p.len() > 1 && p.ends_with('/') {
        p.pop();
    }
    if p.split('/').any(|c| c == "..") {
        return None;
    }
    let home = crate::foundation::DATA_CONTAINER;
    (p == home || p.starts_with(&format!("{home}/")) || p.starts_with("/private/tmp")).then_some(p)
}

fn parent(p: &str) -> &str {
    p.rsplit_once('/').map_or("/", |(a, _)| if a.is_empty() { "/" } else { a })
}

/// A data-container file's contents.
pub fn data_file(emu: &mut Emu, path: &str) -> Option<Vec<u8>> {
    let p = data_path(path)?;
    container(emu).files.get(&p).cloned()
}

/// Create or replace a data-container file. False when its directory is missing.
pub fn write_data_file(emu: &mut Emu, path: &str, bytes: &[u8]) -> bool {
    let Some(p) = data_path(path) else { return false };
    let now = now_1970(emu);
    let c = container(emu);
    if !c.dirs.contains(parent(&p)) && !p.starts_with("/private/tmp") {
        return false;
    }
    c.files.insert(p.clone(), bytes.to_vec());
    c.touch(&p, now);
    true
}

/// A file's or directory's size, kind and metadata.
pub fn attributes(emu: &mut Emu, path: &str) -> Option<Attributes> {
    if let Some(p) = data_path(path) {
        let installed = install_time(emu);
        let c = container(emu);
        let (size, is_dir) = match (c.dirs.contains(&p), c.files.get(&p)) {
            (true, _) => (0, true),
            (_, Some(d)) => (d.len() as u64, false),
            _ => return None,
        };
        return Some(Attributes { size, is_dir, meta: c.meta_of(&p, installed), read_only: false });
    }
    let h = host_path(emu, path)?;
    let size = vfs::len(&h)?;
    let is_dir = vfs::is_dir(&h);
    Some(Attributes { size, is_dir, meta: Meta::new(install_time(emu), is_dir), read_only: true })
}

/// Change a data-container item's dates, permissions or protection class.
/// False when it doesn't exist or is in the read-only bundle.
pub fn set_attributes(emu: &mut Emu, path: &str, change: impl FnOnce(&mut Meta)) -> bool {
    let Some(p) = data_path(path) else { return false };
    let installed = install_time(emu);
    let c = container(emu);
    if !c.dirs.contains(&p) && !c.files.contains_key(&p) {
        return false;
    }
    let mut m = c.meta_of(&p, installed);
    change(&mut m);
    c.meta.insert(p, m);
    true
}

/// Move a data-container file or directory (and what's in it) to `to`,
/// whose directory must exist and which mustn't. Err(errno).
pub fn move_item(emu: &mut Emu, from: &str, to: &str) -> std::result::Result<(), u64> {
    let (Some(f), Some(t)) = (data_path(from), data_path(to)) else { return Err(EACCES) };
    let c = container(emu);
    if !c.dirs.contains(&f) && !c.files.contains_key(&f) {
        return Err(ENOENT);
    }
    if c.dirs.contains(&t) || c.files.contains_key(&t) {
        return Err(EEXIST);
    }
    if !c.dirs.contains(parent(&t)) || t.starts_with(&format!("{f}/")) {
        return Err(ENOENT);
    }
    let moved = |k: &str| -> Option<String> { if k == f { Some(t.clone()) } else { k.strip_prefix(&format!("{f}/")).map(|r| format!("{t}/{r}")) } };
    let files: Vec<String> = c.files.keys().filter(|k| moved(k).is_some()).cloned().collect();
    for k in files {
        let v = c.files.remove(&k).expect("listed");
        c.files.insert(moved(&k).expect("moves"), v);
    }
    let dirs: Vec<String> = c.dirs.iter().filter(|k| moved(k).is_some()).cloned().collect();
    for k in dirs {
        c.dirs.remove(&k);
        c.dirs.insert(moved(&k).expect("moves"));
    }
    let metas: Vec<String> = c.meta.keys().filter(|k| moved(k).is_some()).cloned().collect();
    for k in metas {
        let m = c.meta.remove(&k).expect("listed");
        c.meta.insert(moved(&k).expect("moves"), m);
    }
    Ok(())
}

/// Copy a file or directory (from the bundle or the data container) to `to`
/// in the data container, which mustn't exist. Err(errno).
pub fn copy_item(emu: &mut Emu, from: &str, to: &str) -> std::result::Result<(), u64> {
    let t = data_path(to).ok_or(EACCES)?;
    if exists(emu, &t).is_some() {
        return Err(EEXIST);
    }
    match exists(emu, from) {
        None => Err(ENOENT),
        Some(false) => {
            let bytes = read_file(emu, from).ok_or(ENOENT)?;
            if write_data_file(emu, &t, &bytes) { Ok(()) } else { Err(ENOENT) }
        }
        Some(true) => {
            if exists(emu, parent(&t)) != Some(true) || !make_dirs(emu, &t, false) {
                return Err(ENOENT);
            }
            let from = from.trim_end_matches('/').to_string();
            for (name, _) in list_dir(emu, &from).unwrap_or_default() {
                copy_item(emu, &format!("{from}/{name}"), &format!("{t}/{name}"))?;
            }
            Ok(())
        }
    }
}

/// Whether a guest path exists, and is a directory (bundle or data container).
pub fn exists(emu: &mut Emu, path: &str) -> Option<bool> {
    if let Some(p) = data_path(path) {
        let c = container(emu);
        if c.dirs.contains(&p) {
            return Some(true);
        }
        return c.files.contains_key(&p).then_some(false);
    }
    let h = host_path(emu, path)?;
    vfs::exists(&h).then(|| vfs::is_dir(&h))
}

/// A guest directory's entries (name, is a directory), sorted.
pub fn list_dir(emu: &mut Emu, path: &str) -> Option<Vec<(String, bool)>> {
    if let Some(p) = data_path(path) {
        let c = container(emu);
        if !c.dirs.contains(&p) {
            return None;
        }
        let prefix = format!("{p}/");
        let direct = |k: &String| k.strip_prefix(&prefix).filter(|r| !r.is_empty() && !r.contains('/')).map(str::to_string);
        let mut v: Vec<(String, bool)> = c.files.keys().filter_map(direct).map(|n| (n, false)).collect();
        v.extend(c.dirs.iter().filter_map(direct).map(|n| (n, true)));
        v.sort();
        return Some(v);
    }
    vfs::read_dir(&host_path(emu, path)?)
}

/// `mkdir -p` in the data container.
pub fn make_dirs(emu: &mut Emu, path: &str, intermediate: bool) -> bool {
    let Some(p) = data_path(path) else { return false };
    let now = now_1970(emu);
    let c = container(emu);
    if c.files.contains_key(&p) {
        return false;
    }
    if !intermediate && !c.dirs.contains(parent(&p)) {
        return false;
    }
    let mut cur = p.as_str();
    while data_path(cur).is_some() && !c.dirs.contains(cur) {
        c.dirs.insert(cur.to_string());
        c.touch(cur, now);
        cur = parent(cur);
    }
    true
}

/// Remove a data-container file or directory (and what's in it).
pub fn remove(emu: &mut Emu, path: &str) -> bool {
    let Some(p) = data_path(path) else { return false };
    let c = container(emu);
    let prefix = format!("{p}/");
    let had = c.files.remove(&p).is_some() | c.dirs.remove(&p);
    c.files.retain(|k, _| !k.starts_with(&prefix));
    c.dirs.retain(|k| !k.starts_with(&prefix));
    c.forget(&p);
    had
}

fn host_app_dir(emu: &Emu) -> PathBuf {
    emu.p.main().path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

/// The guest path of the app bundle.
pub fn guest_bundle_path(emu: &Emu) -> String {
    let dir = host_app_dir(emu);
    let name = dir.file_name().map_or_else(|| format!("{}.app", emu.p.main().name), |n| n.to_string_lossy().into_owned());
    format!("{CONTAINER}/{name}")
}

/// Where a guest path lives on the host, if anywhere. Refuses `..` escapes.
pub fn host_path(emu: &Emu, guest: &str) -> Option<PathBuf> {
    let bundle = guest_bundle_path(emu);
    // `/var/...` and `/private/var/...` are the same place on iOS.
    let guest = if guest.starts_with("/var/") { format!("/private{guest}") } else { guest.to_string() };
    let rest = guest.strip_prefix(&bundle)?;
    let rest = rest.trim_start_matches('/');
    if rest.split('/').any(|c| c == "..") {
        return None;
    }
    Some(host_app_dir(emu).join(rest))
}

/// Read a whole guest file (bundle or data container).
pub fn read_file(emu: &mut Emu, guest: &str) -> Option<Vec<u8>> {
    if data_path(guest).is_some() {
        return data_file(emu, guest);
    }
    let p = host_path(emu, guest)?;
    if !vfs::is_file(&p) {
        return None;
    }
    vfs::read(&p).ok()
}

fn fail(emu: &mut Emu, errno: u64) -> Result<()> {
    crate::libsystem::set_errno(emu, errno)?;
    emu.ret(u64::MAX);
    Ok(())
}

/// A descriptor's status flags (O_NONBLOCK and the like, as fcntl set them).
pub(crate) fn status_flags(emu: &Emu, fd: u64) -> u64 {
    emu.fs.status_flags.get(&fd).copied().unwrap_or(0)
}

/// A new descriptor number for something that isn't a file here (a socket).
/// A descriptor another module closed: forget its flags.
pub(crate) fn release_fd(emu: &mut Emu, fd: u64) {
    emu.fs.status_flags.remove(&fd);
}

pub(crate) fn reserve_fd(emu: &mut Emu) -> u64 {
    if emu.fs.next_fd < 3 {
        emu.fs.next_fd = 3;
    }
    let fd = emu.fs.next_fd;
    emu.fs.next_fd += 1;
    fd
}

/// What poll would say of a file, directory, pipe or kqueue descriptor:
/// (readable, writable, hung up); None when it isn't one.
pub(crate) fn poll_state(emu: &mut Emu, fd: u64) -> Option<(bool, bool, bool)> {
    if let Some(&(pipe, read_end)) = emu.fs.pipe_ends.get(&fd) {
        let p = emu.fs.pipes.get(&pipe)?;
        return Some(if read_end {
            (!p.data.is_empty() || p.writers == 0, false, p.writers == 0)
        } else {
            (false, p.readers > 0, p.readers == 0)
        });
    }
    if let Some(saved) = emu.fs.kqueues.get(&fd).cloned() {
        // Look without taking: kqueue_ready consumes one-shot and cleared events.
        let ready = !kqueue_ready(emu, fd, 1).is_empty();
        emu.fs.kqueues.insert(fd, saved);
        return Some((ready, false, false));
    }
    if emu.fs.dir_fds.contains_key(&fd) {
        return Some((true, false, false));
    }
    if fd <= 2 || emu.fs.files.contains_key(&fd) {
        return Some((true, true, false));
    }
    None
}

fn open_fd(emu: &mut Emu, data: Vec<u8>) -> u64 {
    open_file(emu, OpenFile { data, pos: 0, path: None, append: false })
}

fn open_file(emu: &mut Emu, f: OpenFile) -> u64 {
    if emu.fs.next_fd < 3 {
        emu.fs.next_fd = 3;
    }
    let fd = emu.fs.next_fd;
    emu.fs.next_fd += 1;
    emu.fs.files.insert(fd, f);
    fd
}

const O_ACCMODE: u64 = 3;
const O_APPEND: u64 = 0x8;
const O_CREAT: u64 = 0x200;
const O_TRUNC: u64 = 0x400;
const O_EXCL: u64 = 0x800;

/// Open a data-container file with open(2) flags; Err(errno).
fn open_data(emu: &mut Emu, path: &str, flags: u64) -> std::result::Result<u64, u64> {
    let p = data_path(path).ok_or(EACCES)?;
    let now = now_1970(emu);
    let c = container(emu);
    if c.dirs.contains(&p) {
        // Directories open read-only (for fsync and friends).
        return if flags & O_ACCMODE == 0 { Ok(open_fd(emu, Vec::new())) } else { Err(EACCES) };
    }
    let existing = c.files.get(&p).cloned();
    let data = match existing {
        Some(_) if flags & O_CREAT != 0 && flags & O_EXCL != 0 => return Err(EEXIST),
        Some(_) if flags & O_TRUNC != 0 => Vec::new(),
        Some(d) => d,
        None if flags & O_CREAT != 0 => {
            if !c.dirs.contains(parent(&p)) && !p.starts_with("/private/tmp") {
                return Err(ENOENT);
            }
            Vec::new()
        }
        None => return Err(ENOENT),
    };
    let writable = flags & O_ACCMODE != 0;
    if writable {
        let changed = !c.files.contains_key(&p) || flags & O_TRUNC != 0;
        c.files.insert(p.clone(), data.clone());
        if changed {
            c.touch(&p, now);
        }
    }
    Ok(open_file(emu, OpenFile { data, pos: 0, path: writable.then_some(p), append: flags & O_APPEND != 0 }))
}

/// `open(path, flags, mode)`: the bundle read-only, the data container read-write.
fn open(emu: &mut Emu) -> Result<()> {
    let (path, flags) = (emu.cstr(emu.arg(0))?, emu.arg(1));
    open_path(emu, path, flags)
}

/// `openat(dirfd, path, flags, mode)`: relative to a directory fd (AT_FDCWD = -2: "/").
fn openat(emu: &mut Emu) -> Result<()> {
    let (dirfd, rel, flags) = (emu.arg(0) as i32, emu.cstr(emu.arg(1))?, emu.arg(2));
    let path = if rel.starts_with('/') {
        rel
    } else {
        let base = if dirfd == -2 { "/".to_string() } else {
            match emu.fs.dir_fds.get(&(dirfd as u64)) {
                Some(b) => b.clone(),
                None => return fail(emu, EBADF),
            }
        };
        format!("{}/{rel}", base.trim_end_matches('/'))
    };
    open_path(emu, path, flags)
}

fn open_path(emu: &mut Emu, path: String, flags: u64) -> Result<()> {
    if emu.trace {
        println!("[fs] open {path} flags {flags:#x}");
    }
    if let Some(d) = device(&path) {
        let fd = open_fd(emu, Vec::new());
        emu.fs.devices.insert(fd, d);
        emu.ret(fd);
        return Ok(());
    }
    // A directory opens read-only, remembering where it is.
    if exists(emu, &path) == Some(true) {
        if flags & O_ACCMODE != 0 {
            return fail(emu, EACCES);
        }
        let fd = open_fd(emu, Vec::new());
        emu.fs.dir_fds.insert(fd, path);
        emu.ret(fd);
        return Ok(());
    }
    if data_path(&path).is_some() {
        return match open_data(emu, &path, flags) {
            Ok(fd) => {
                emu.ret(fd);
                Ok(())
            }
            Err(e) => fail(emu, e),
        };
    }
    if flags & O_ACCMODE != 0 {
        return fail(emu, EACCES);
    }
    match read_file(emu, &path) {
        Some(data) => {
            let fd = open_fd(emu, data);
            emu.ret(fd);
            Ok(())
        }
        None => fail(emu, ENOENT),
    }
}

fn close(emu: &mut Emu) -> Result<()> {
    let fd = emu.arg(0) & 0xffff_ffff;
    if crate::sockets::owns(emu, fd) {
        crate::sockets::close(emu, fd);
        emu.fs.status_flags.remove(&fd);
        emu.ret(0);
        return Ok(());
    }
    emu.fs.devices.remove(&fd);
    if let Some((id, read_end)) = emu.fs.pipe_ends.remove(&fd)
        && let Some(p) = emu.fs.pipes.get_mut(&id)
    {
        if read_end {
            p.readers = p.readers.saturating_sub(1);
        } else {
            p.writers = p.writers.saturating_sub(1);
        }
    }
    emu.fs.kqueues.remove(&fd);
    emu.fs.status_flags.remove(&fd);
    emu.fs.dir_fds.remove(&fd);
    emu.fs.files.remove(&fd);
    emu.ret(0);
    Ok(())
}

fn read_at(emu: &mut Emu, fd: u64, buf: u64, n: u64, at: Option<u64>) -> Result<Option<u64>> {
    let Some(f) = emu.fs.files.get_mut(&fd) else { return Ok(None) };
    let start = at.map_or(f.pos, |a| a as usize).min(f.data.len());
    let end = (start + n as usize).min(f.data.len());
    let chunk = f.data[start..end].to_vec();
    if at.is_none() {
        f.pos = end;
    }
    emu.p.memory.write(buf, &chunk)?;
    Ok(Some(chunk.len() as u64))
}

/// `readv(fd, iov, n)`: a socket's or file's bytes into each buffer in turn
/// (a pipe's into the first, as one read).
fn readv(emu: &mut Emu) -> Result<()> {
    let (fd, iov, n) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2));
    if crate::sockets::owns(emu, fd) {
        return crate::sockets::readv(emu, fd, iov, n);
    }
    if emu.fs.pipe_ends.contains_key(&fd) {
        let (base, len) = (emu.read_u64(iov)?, emu.read_u64(iov + 8)?);
        return pipe_read(emu, fd, base, len);
    }
    let mut total = 0;
    for i in 0..n {
        let (base, len) = (emu.read_u64(iov + i * 16)?, emu.read_u64(iov + i * 16 + 8)?);
        match read_at(emu, fd, base, len, None)? {
            Some(got) => {
                total += got;
                if got < len {
                    break;
                }
            }
            None => return fail(emu, EBADF),
        }
    }
    emu.ret(total);
    Ok(())
}

fn read(emu: &mut Emu) -> Result<()> {
    let (fd, buf, n) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2));
    if crate::sockets::owns(emu, fd) {
        return crate::sockets::read(emu, fd, buf, n);
    }
    if let Some(d) = emu.fs.devices.get(&fd).copied() {
        let bytes = match d {
            Device::Random => crate::libsystem::random_bytes(emu, n as usize),
            Device::Zero => vec![0u8; n as usize],
            Device::Null => Vec::new(),
        };
        emu.p.memory.write(buf, &bytes)?;
        emu.ret(bytes.len() as u64);
        return Ok(());
    }
    if emu.fs.pipe_ends.contains_key(&fd) {
        return pipe_read(emu, fd, buf, n);
    }
    match read_at(emu, fd, buf, n, None)? {
        Some(r) => {
            emu.ret(r);
            Ok(())
        }
        None => fail(emu, EBADF),
    }
}

/// `pread(fd, buf, n, offset)`
fn pread(emu: &mut Emu) -> Result<()> {
    let (fd, buf, n, off) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2), emu.arg(3));
    match read_at(emu, fd, buf, n, Some(off))? {
        Some(r) => {
            emu.ret(r);
            Ok(())
        }
        None => fail(emu, EBADF),
    }
}

/// Write into an open file at `at` (or its position), keeping the container in sync.
fn write_at(emu: &mut Emu, fd: u64, bytes: &[u8], at: Option<u64>) -> Option<u64> {
    let f = emu.fs.files.get_mut(&fd)?;
    let path = f.path.clone()?;
    let start = match at {
        Some(a) => a as usize,
        None if f.append => f.data.len(),
        None => f.pos,
    };
    if f.data.len() < start + bytes.len() {
        f.data.resize(start + bytes.len(), 0);
    }
    f.data[start..start + bytes.len()].copy_from_slice(bytes);
    if at.is_none() {
        f.pos = start + bytes.len();
    }
    let data = f.data.clone();
    let now = now_1970(emu);
    let c = container(emu);
    c.files.insert(path.clone(), data);
    c.touch(&path, now);
    Some(bytes.len() as u64)
}

/// `write(fd, buf, n)`: stdout/stderr print; data-container files are written.
fn write(emu: &mut Emu) -> Result<()> {
    let (fd, buf, n) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2));
    if crate::sockets::owns(emu, fd) {
        return crate::sockets::write(emu, fd, buf, n);
    }
    if emu.fs.devices.contains_key(&fd) {
        emu.ret(n); // devices take what's written
        return Ok(());
    }
    let bytes = emu.p.memory.read(buf, n as usize)?.to_vec();
    if fd == 1 || fd == 2 {
        print!("{}", String::from_utf8_lossy(&bytes));
        emu.ret(n);
        return Ok(());
    }
    if let Some(&(id, false)) = emu.fs.pipe_ends.get(&fd) {
        let p = emu.fs.pipes.entry(id).or_default();
        p.data.extend(&bytes);
        emu.ret(n);
        return Ok(());
    }
    match write_at(emu, fd, &bytes, None) {
        Some(w) => {
            emu.ret(w);
            Ok(())
        }
        None => fail(emu, EBADF),
    }
}

// ---- pipes ----

/// `pipe(int fds[2])`: [read end, write end].
fn pipe(emu: &mut Emu) -> Result<()> {
    let out = emu.arg(0);
    let id = emu.fs.next_pipe;
    emu.fs.next_pipe += 1;
    emu.fs.pipes.insert(id, Pipe { data: Default::default(), readers: 1, writers: 1 });
    let r = open_fd(emu, Vec::new());
    let w = open_fd(emu, Vec::new());
    emu.fs.pipe_ends.insert(r, (id, true));
    emu.fs.pipe_ends.insert(w, (id, false));
    emu.p.memory.write_u32(out, r as u32)?;
    emu.p.memory.write_u32(out + 4, w as u32)?;
    emu.ret(0);
    Ok(())
}

/// Read from a pipe now: Some(bytes read, 0 at end of file), or None if it would block.
pub(crate) fn pipe_try_read(emu: &mut Emu, fd: u64, buf: u64, n: u64) -> Result<Option<u64>> {
    let Some(&(id, _)) = emu.fs.pipe_ends.get(&fd) else { return Ok(Some(0)) };
    let p = emu.fs.pipes.entry(id).or_default();
    if p.data.is_empty() {
        return Ok(if p.writers == 0 { Some(0) } else { None });
    }
    let k = (n as usize).min(p.data.len());
    let bytes: Vec<u8> = p.data.drain(..k).collect();
    emu.p.memory.write(buf, &bytes)?;
    Ok(Some(k as u64))
}

fn pipe_read(emu: &mut Emu, fd: u64, buf: u64, n: u64) -> Result<()> {
    if let Some(k) = pipe_try_read(emu, fd, buf, n)? {
        emu.ret(k);
        return Ok(());
    }
    if emu.fs.status_flags.get(&fd).is_some_and(|f| f & O_NONBLOCK != 0) {
        return fail(emu, EAGAIN);
    }
    let r = crate::threads::block(emu, crate::threads::Wait::PipeRead { fd, buf, n })?;
    emu.ret(r);
    Ok(())
}

/// `fcntl(fd, cmd, ...)`: status flags (O_NONBLOCK) and descriptor flags (FD_CLOEXEC).
fn fcntl(emu: &mut Emu) -> Result<()> {
    const F_GETFD: u64 = 1;
    const F_SETFD: u64 = 2;
    const F_GETFL: u64 = 3;
    const F_SETFL: u64 = 4;
    let (fd, cmd) = (emu.arg(0) & 0xffff_ffff, emu.arg(1) & 0xffff_ffff);
    // The variadic argument is on the stack.
    let arg = emu.read_u64(emu.cpu.sp)?;
    let r = match cmd {
        F_GETFL => emu.fs.status_flags.get(&fd).copied().unwrap_or(0) | if emu.fs.pipe_ends.get(&fd).is_some_and(|e| !e.1) { 1 } else { 0 },
        F_SETFL => {
            emu.fs.status_flags.insert(fd, arg & O_NONBLOCK);
            0
        }
        F_GETFD | F_SETFD => 0,
        // Record locks: the app is the only process, so no lock can conflict.
        // Setting one always succeeds; asking finds the range unlocked.
        F_GETLK => {
            if !open_or_dir(emu, fd) {
                return fail(emu, EBADF);
            }
            emu.p.memory.write_u16(arg + 20, F_UNLCK)?; // struct flock's l_type
            0
        }
        F_SETLK | F_SETLKW | F_OFD_SETLK | F_OFD_SETLKW => {
            if !open_or_dir(emu, fd) {
                return fail(emu, EBADF);
            }
            0
        }
        // Writes are already where they're going (memory); caching hints change nothing.
        F_FULLFSYNC | F_BARRIERFSYNC | F_NOCACHE | F_RDAHEAD | F_RDADVISE | F_PREALLOCATE | F_SETNOSIGPIPE => {
            if !open_or_dir(emu, fd) {
                return fail(emu, EBADF);
            }
            0
        }
        _ => return fail(emu, EINVAL),
    };
    emu.ret(r);
    Ok(())
}

const F_GETLK: u64 = 7;
const F_SETLK: u64 = 8;
const F_SETLKW: u64 = 9;
const F_PREALLOCATE: u64 = 42;
const F_RDADVISE: u64 = 44;
const F_RDAHEAD: u64 = 45;
const F_NOCACHE: u64 = 48;
const F_FULLFSYNC: u64 = 51;
const F_SETNOSIGPIPE: u64 = 73;
const F_BARRIERFSYNC: u64 = 85;
const F_OFD_SETLK: u64 = 90;
const F_OFD_SETLKW: u64 = 91;
const F_UNLCK: u16 = 2;

fn open_or_dir(emu: &Emu, fd: u64) -> bool {
    emu.fs.files.contains_key(&fd) || emu.fs.dir_fds.contains_key(&fd) || emu.fs.pipe_ends.contains_key(&fd)
}

/// Set a data-container item's modification time to `t` (seconds since
/// 1970), or now. ENOENT if it's missing, EACCES in the read-only bundle.
fn touch_path(emu: &mut Emu, path: &str, t: Option<f64>) -> Result<()> {
    let now = now_1970(emu);
    if set_attributes(emu, path, |m| m.modified = t.unwrap_or(now)) {
        emu.ret(0);
        Ok(())
    } else {
        let errno = if exists(emu, path).is_some() { EACCES } else { ENOENT };
        fail(emu, errno)
    }
}

/// The modification time from `struct timeval times[2]` (NULL: now).
fn timeval_mtime(emu: &Emu, times: u64) -> Result<Option<f64>> {
    if times == 0 {
        return Ok(None);
    }
    let (s, us) = (emu.read_u64(times + 16)? as i64, emu.p.memory.read_u32(times + 24)? as i32);
    Ok(Some(s as f64 + us as f64 / 1e6))
}

/// `utimes(path, times)`
fn utimes(emu: &mut Emu) -> Result<()> {
    let (path, times) = (emu.cstr(emu.arg(0))?, emu.arg(1));
    let t = timeval_mtime(emu, times)?;
    touch_path(emu, &path, t)
}

/// `futimes(fd, times)`
fn futimes(emu: &mut Emu) -> Result<()> {
    let (fd, times) = (emu.arg(0) & 0xffff_ffff, emu.arg(1));
    let Some(path) = emu.fs.files.get(&fd).and_then(|f| f.path.clone()) else { return fail(emu, EBADF) };
    let t = timeval_mtime(emu, times)?;
    touch_path(emu, &path, t)
}

/// `utimensat(dirfd, path, times, flags)`: `struct timespec times[2]`
/// (NULL: now; UTIME_NOW / UTIME_OMIT in tv_nsec).
fn utimensat(emu: &mut Emu) -> Result<()> {
    const UTIME_NOW: i64 = -1;
    const UTIME_OMIT: i64 = -2;
    let (path, times) = (emu.cstr(emu.arg(1))?, emu.arg(2));
    if times == 0 {
        return touch_path(emu, &path, None);
    }
    let (s, ns) = (emu.read_u64(times + 16)? as i64, emu.read_u64(times + 24)? as i64);
    match ns {
        UTIME_OMIT => {
            if exists(emu, &path).is_none() {
                return fail(emu, ENOENT);
            }
            emu.ret(0);
            Ok(())
        }
        UTIME_NOW => touch_path(emu, &path, None),
        _ => touch_path(emu, &path, Some(s as f64 + ns as f64 / 1e9)),
    }
}

// ---- kqueue ----

fn kqueue(emu: &mut Emu) -> Result<()> {
    let fd = open_fd(emu, Vec::new());
    emu.fs.kqueues.insert(fd, Vec::new());
    emu.ret(fd);
    Ok(())
}

/// Events ready on a kqueue now (EV_ONESHOT ones removed, user triggers reset).
fn kqueue_ready(emu: &mut Emu, kq: u64, max: usize) -> Vec<KEvent> {
    let Some(regs) = emu.fs.kqueues.get(&kq).cloned() else { return Vec::new() };
    let mut out = Vec::new();
    let mut keep = Vec::new();
    for mut k in regs {
        if out.len() >= max || !k.enabled {
            keep.push(k);
            continue;
        }
        let ready = match k.filter {
            EVFILT_READ => match emu.fs.pipe_ends.get(&k.ident).and_then(|(id, _)| emu.fs.pipes.get(id)) {
                Some(p) if !p.data.is_empty() => Some((p.data.len() as i64, 0)),
                Some(p) if p.writers == 0 => Some((0, EV_EOF)),
                _ => None,
            },
            EVFILT_WRITE => match emu.fs.pipe_ends.get(&k.ident).and_then(|(id, _)| emu.fs.pipes.get(id)) {
                Some(p) if p.readers == 0 => Some((0, EV_EOF)),
                _ => Some((65536, 0)),
            },
            EVFILT_USER if k.triggered => Some((0, 0)),
            _ => None,
        };
        if let Some((data, extra)) = ready {
            let mut ev = k.clone();
            ev.data = data;
            ev.flags = k.flags | extra;
            out.push(ev);
            if k.filter == EVFILT_USER || k.flags & EV_CLEAR != 0 {
                k.triggered = false;
            }
            if k.flags & EV_ONESHOT != 0 {
                continue;
            }
        }
        keep.push(k);
    }
    emu.fs.kqueues.insert(kq, keep);
    out
}

/// Write ready events to the guest's event list; the count.
pub(crate) fn kevent_try(emu: &mut Emu, kq: u64, list: u64, n: u64) -> Result<Option<u64>> {
    let ready = kqueue_ready(emu, kq, n as usize);
    if ready.is_empty() {
        return Ok(None);
    }
    for (i, k) in ready.iter().enumerate() {
        let at = list + i as u64 * 32;
        emu.write_u64(at, k.ident)?;
        emu.p.memory.write_u16(at + 8, k.filter as u16)?;
        emu.p.memory.write_u16(at + 10, k.flags)?;
        emu.p.memory.write_u32(at + 12, k.fflags)?;
        emu.write_u64(at + 16, k.data as u64)?;
        emu.write_u64(at + 24, k.udata)?;
    }
    Ok(Some(ready.len() as u64))
}

/// `kevent(kq, changes, nchanges, events, nevents, const struct timespec *timeout)`.
fn kevent(emu: &mut Emu) -> Result<()> {
    let (kq, changes, nchanges, events, nevents, timeout) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    if !emu.fs.kqueues.contains_key(&kq) {
        return fail(emu, EBADF);
    }
    for i in 0..nchanges {
        let at = changes + i * 32;
        let ident = emu.read_u64(at)?;
        let filter = emu.p.memory.read_u16(at + 8)? as i16;
        let flags = emu.p.memory.read_u16(at + 10)?;
        let fflags = emu.p.memory.read_u32(at + 12)?;
        let data = emu.read_u64(at + 16)? as i64;
        let udata = emu.read_u64(at + 24)?;
        let regs = emu.fs.kqueues.get_mut(&kq).unwrap();
        let pos = regs.iter().position(|k| k.ident == ident && k.filter == filter);
        if flags & EV_DELETE != 0 {
            if let Some(p) = pos {
                regs.remove(p);
            }
            continue;
        }
        let k = match pos {
            Some(p) => &mut regs[p],
            None if flags & EV_ADD != 0 => {
                regs.push(KEvent { ident, filter, flags, fflags: 0, data, udata, enabled: true, triggered: false });
                regs.last_mut().unwrap()
            }
            None => continue,
        };
        k.flags = flags & !(EV_ADD | EV_ENABLE | EV_DISABLE);
        k.udata = udata;
        if flags & EV_ENABLE != 0 || flags & EV_ADD != 0 {
            k.enabled = true;
        }
        if flags & EV_DISABLE != 0 {
            k.enabled = false;
        }
        if filter == EVFILT_USER {
            k.fflags = fflags & 0x00ff_ffff;
            if fflags & NOTE_TRIGGER != 0 {
                k.triggered = true;
            }
        }
    }
    if nevents == 0 {
        emu.ret(0);
        return Ok(());
    }
    if let Some(k) = kevent_try(emu, kq, events, nevents)? {
        emu.ret(k);
        return Ok(());
    }
    let deadline = if timeout == 0 {
        None
    } else {
        let rel = emu.read_u64(timeout)? * 1_000_000_000 + emu.read_u64(timeout + 8)?;
        if rel == 0 {
            emu.ret(0);
            return Ok(());
        }
        Some(crate::threads::now_ns(emu) + rel)
    };
    let r = crate::threads::block(emu, crate::threads::Wait::Kevent { kq, list: events, n: nevents, deadline })?;
    emu.ret(r);
    Ok(())
}

fn pwrite(emu: &mut Emu) -> Result<()> {
    let (fd, buf, n, off) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2), emu.arg(3));
    let bytes = emu.p.memory.read(buf, n as usize)?.to_vec();
    match write_at(emu, fd, &bytes, Some(off)) {
        Some(w) => {
            emu.ret(w);
            Ok(())
        }
        None => fail(emu, EBADF),
    }
}

fn ftruncate(emu: &mut Emu) -> Result<()> {
    let (fd, len) = (emu.arg(0) & 0xffff_ffff, emu.arg(1) as usize);
    let Some(f) = emu.fs.files.get_mut(&fd) else { return fail(emu, EBADF) };
    let Some(path) = f.path.clone() else { return fail(emu, EACCES) };
    f.data.resize(len, 0);
    let data = f.data.clone();
    let now = now_1970(emu);
    let c = container(emu);
    c.files.insert(path.clone(), data);
    c.touch(&path, now);
    emu.ret(0);
    Ok(())
}

fn mkdir(emu: &mut Emu) -> Result<()> {
    let path = emu.cstr(emu.arg(0))?;
    if exists(emu, &path).is_some() {
        return fail(emu, EEXIST);
    }
    if make_dirs(emu, &path, false) {
        emu.ret(0);
        Ok(())
    } else {
        fail(emu, if data_path(&path).is_some() { ENOENT } else { EACCES })
    }
}

fn unlink(emu: &mut Emu) -> Result<()> {
    let path = emu.cstr(emu.arg(0))?;
    match exists(emu, &path) {
        Some(false) if remove(emu, &path) => {
            emu.ret(0);
            Ok(())
        }
        Some(_) => fail(emu, EACCES),
        None => fail(emu, ENOENT),
    }
}

fn rmdir(emu: &mut Emu) -> Result<()> {
    let path = emu.cstr(emu.arg(0))?;
    match exists(emu, &path) {
        Some(true) if remove(emu, &path) => {
            emu.ret(0);
            Ok(())
        }
        Some(false) => fail(emu, ENOTDIR),
        _ => fail(emu, ENOENT),
    }
}

/// `rename(from, to)`: a file replaces a file at `to`; a directory moves with what's in it.
fn rename(emu: &mut Emu) -> Result<()> {
    let (from, to) = (emu.cstr(emu.arg(0))?, emu.cstr(emu.arg(1))?);
    if exists(emu, &from) == Some(false) && exists(emu, &to) == Some(false) {
        remove(emu, &to);
    }
    match move_item(emu, &from, &to) {
        Ok(()) => {
            emu.ret(0);
            Ok(())
        }
        Err(e) => fail(emu, e),
    }
}

/// `lseek(fd, offset, whence)`
fn lseek(emu: &mut Emu) -> Result<()> {
    let (fd, off, whence) = (emu.arg(0) & 0xffff_ffff, emu.arg(1) as i64, emu.arg(2) & 0xffff_ffff);
    let Some(f) = emu.fs.files.get_mut(&fd) else { return fail(emu, EBADF) };
    let base = match whence {
        0 => 0,
        1 => f.pos as i64,
        _ => f.data.len() as i64,
    };
    f.pos = (base + off).max(0) as usize;
    let p = f.pos as u64;
    emu.ret(p);
    Ok(())
}

/// Fill a Darwin arm64 `struct stat` (144 bytes).
fn write_stat(emu: &mut Emu, buf: u64, size: u64, is_dir: bool) -> Result<()> {
    let meta = Meta::new(install_time(emu), is_dir);
    write_stat_meta(emu, buf, size, is_dir, &meta)
}

/// `struct stat` with the item's dates and permissions.
fn write_stat_meta(emu: &mut Emu, buf: u64, size: u64, is_dir: bool, meta: &Meta) -> Result<()> {
    emu.p.memory.write(buf, &[0u8; 144])?;
    let mode: u16 = if is_dir { 0o040000 } else { 0o100000 } | (meta.permissions & 0o7777);
    emu.p.memory.write_u16(buf + 4, mode)?;
    // st_atimespec, st_mtimespec, st_ctimespec, st_birthtimespec.
    for (at, t) in [(32, meta.modified), (48, meta.modified), (64, meta.modified), (80, meta.created)] {
        let secs = t.floor();
        emu.write_u64(buf + at, secs as i64 as u64)?;
        emu.write_u64(buf + at + 8, ((t - secs) * 1e9) as u64)?;
    }
    emu.p.memory.write_u16(buf + 6, 1)?; // nlink
    emu.p.memory.write_u32(buf + 16, 501)?; // uid
    emu.p.memory.write_u32(buf + 20, 501)?; // gid
    emu.write_u64(buf + 96, size)?;
    emu.write_u64(buf + 104, size.div_ceil(512))?;
    emu.p.memory.write_u32(buf + 112, 4096)?; // blksize
    Ok(())
}

fn fstat(emu: &mut Emu) -> Result<()> {
    let (fd, buf) = (emu.arg(0) & 0xffff_ffff, emu.arg(1));
    if emu.fs.devices.contains_key(&fd) {
        // A character device (S_IFCHR, rw for everyone).
        emu.p.memory.write(buf, &[0u8; 144])?;
        emu.p.memory.write_u16(buf + 4, 0o020666)?;
        emu.ret(0);
        return Ok(());
    }
    if crate::sockets::owns(emu, fd) {
        // A socket: S_IFSOCK, nothing else.
        emu.p.memory.write(buf, &[0u8; 144])?;
        emu.p.memory.write_u16(buf + 4, 0o140777)?;
        emu.ret(0);
        return Ok(());
    }
    if emu.fs.dir_fds.contains_key(&fd) {
        write_stat(emu, buf, 0, true)?;
        emu.ret(0);
        return Ok(());
    }
    let Some(len) = emu.fs.files.get(&fd).map(|f| f.data.len() as u64) else { return fail(emu, EBADF) };
    write_stat(emu, buf, len, false)?;
    emu.ret(0);
    Ok(())
}

/// `stat(path, buf)` / `lstat`
fn stat(emu: &mut Emu) -> Result<()> {
    let (path, buf) = (emu.cstr(emu.arg(0))?, emu.arg(1));
    if device(&path).is_some() {
        emu.p.memory.write(buf, &[0u8; 144])?;
        emu.p.memory.write_u16(buf + 4, 0o020666)?;
        emu.ret(0);
        return Ok(());
    }
    if data_path(&path).is_some() {
        let Some(a) = attributes(emu, &path) else { return fail(emu, ENOENT) };
        write_stat_meta(emu, buf, a.size, a.is_dir, &a.meta)?;
        emu.ret(0);
        return Ok(());
    }
    let Some(p) = host_path(emu, &path) else { return fail(emu, ENOENT) };
    let Some(len) = vfs::len(&p) else { return fail(emu, ENOENT) };
    write_stat(emu, buf, len, vfs::is_dir(&p))?;
    emu.ret(0);
    Ok(())
}

/// `access(path, mode)`: the bundle is read-only; the data container isn't.
fn access(emu: &mut Emu) -> Result<()> {
    let (path, mode) = (emu.cstr(emu.arg(0))?, emu.arg(1));
    if device(&path).is_some() {
        emu.ret(0);
        return Ok(());
    }
    if data_path(&path).is_some() {
        return if exists(emu, &path).is_some() {
            emu.ret(0);
            Ok(())
        } else {
            fail(emu, ENOENT)
        };
    }
    const W_OK: u64 = 2;
    match host_path(emu, &path) {
        Some(p) if vfs::exists(&p) && mode & W_OK == 0 => {
            emu.ret(0);
            Ok(())
        }
        Some(p) if vfs::exists(&p) => fail(emu, EACCES),
        _ => fail(emu, ENOENT),
    }
}

/// `mmap(addr, len, prot, flags, fd, offset)`: anonymous memory, or a private copy of a file.
fn mmap(emu: &mut Emu) -> Result<()> {
    let (len, flags, fd, off) = (emu.arg(1), emu.arg(3), emu.arg(4) as i32, emu.arg(5));
    const MAP_ANON: u64 = 0x1000;
    let page = pomme_mem::PAGE_SIZE;
    let raw = emu.alloc(len + page)?;
    let p = (raw + page - 1) & !(page - 1);
    if flags & MAP_ANON == 0 {
        let Some(f) = emu.fs.files.get(&(fd as u64)) else { return fail(emu, EBADF) };
        let start = (off as usize).min(f.data.len());
        let end = (start + len as usize).min(f.data.len());
        let chunk = f.data[start..end].to_vec();
        emu.p.memory.write(p, &chunk)?;
    }
    emu.ret(p);
    Ok(())
}

// ---- stdio streams (FILE* is an opaque handle mapped to an fd) ----

fn fopen(emu: &mut Emu) -> Result<()> {
    let (path, mode) = (emu.cstr(emu.arg(0))?, emu.cstr(emu.arg(1))?);
    if data_path(&path).is_some() {
        let flags = match mode.trim_end_matches(['b', 'x', 'e']) {
            "r" => 0,
            "r+" | "rb+" => 2,
            "w" => 1 | O_CREAT | O_TRUNC,
            "w+" | "wb+" => 2 | O_CREAT | O_TRUNC,
            "a" => 1 | O_CREAT | O_APPEND,
            _ => 2 | O_CREAT | O_APPEND,
        };
        return match open_data(emu, &path, flags) {
            Ok(fd) => {
                let file = emu.alloc(256)?;
                emu.fs.streams.insert(file, fd);
                emu.ret(file);
                Ok(())
            }
            Err(e) => {
                crate::libsystem::set_errno(emu, e)?;
                emu.ret(0);
                Ok(())
            }
        };
    }
    if mode.contains('w') || mode.contains('a') || mode.contains('+') {
        crate::libsystem::set_errno(emu, EACCES)?;
        emu.ret(0);
        return Ok(());
    }
    let Some(data) = read_file(emu, &path) else {
        crate::libsystem::set_errno(emu, ENOENT)?;
        emu.ret(0);
        return Ok(());
    };
    let fd = open_fd(emu, data);
    let file = emu.alloc(256)?;
    emu.fs.streams.insert(file, fd);
    emu.ret(file);
    Ok(())
}

/// `fdopen(fd, mode)`: a stdio stream over an open file descriptor (it
/// keeps the descriptor's own access; fclose closes it).
fn fdopen(emu: &mut Emu) -> Result<()> {
    let fd = emu.arg(0) & 0xffff_ffff;
    if !emu.fs.files.contains_key(&fd) && !emu.fs.pipe_ends.contains_key(&fd) {
        crate::libsystem::set_errno(emu, EBADF)?;
        emu.ret(0);
        return Ok(());
    }
    let file = emu.alloc(256)?;
    emu.fs.streams.insert(file, fd);
    emu.ret(file);
    Ok(())
}

fn stream_fd(emu: &Emu, file: u64) -> Option<u64> {
    emu.fs.streams.get(&file).copied()
}

/// `fwrite` to a stream opened here (printf.rs handles stdout/stderr): items written.
pub(crate) fn fwrite_stream(emu: &mut Emu) -> Option<Result<()>> {
    let (p, size, n, file) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    let fd = stream_fd(emu, file)?;
    let bytes = match emu.p.memory.read(p, (size * n) as usize) {
        Ok(b) => b.to_vec(),
        Err(e) => return Some(Err(e.into())),
    };
    let w = write_at(emu, fd, &bytes, None).unwrap_or(0);
    emu.ret(w.checked_div(size).unwrap_or(0));
    Some(Ok(()))
}

/// `fread(ptr, size, n, FILE*)` → items read
fn fread(emu: &mut Emu) -> Result<()> {
    let (buf, size, n, file) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    let Some(fd) = stream_fd(emu, file) else {
        emu.ret(0);
        return Ok(());
    };
    let bytes = read_at(emu, fd, buf, size * n, None)?.unwrap_or(0);
    emu.ret(bytes.checked_div(size).unwrap_or(0));
    Ok(())
}

fn fclose(emu: &mut Emu) -> Result<()> {
    let file = emu.arg(0);
    if let Some(fd) = emu.fs.streams.remove(&file) {
        emu.fs.files.remove(&fd);
    }
    emu.ret(0);
    Ok(())
}

/// `fseek(FILE*, offset, whence)` → 0
fn fseek(emu: &mut Emu) -> Result<()> {
    let file = emu.arg(0);
    let Some(fd) = stream_fd(emu, file) else { return fail(emu, EBADF) };
    emu.cpu.x[0] = fd;
    lseek(emu)?;
    emu.ret(0);
    Ok(())
}

fn ftell(emu: &mut Emu) -> Result<()> {
    let file = emu.arg(0);
    let pos = stream_fd(emu, file).and_then(|fd| emu.fs.files.get(&fd)).map_or(u64::MAX, |f| f.pos as u64);
    emu.ret(pos);
    Ok(())
}

fn rewind(emu: &mut Emu) -> Result<()> {
    let file = emu.arg(0);
    if let Some(f) = stream_fd(emu, file).and_then(|fd| emu.fs.files.get_mut(&fd)) {
        f.pos = 0;
    }
    Ok(())
}

fn feof(emu: &mut Emu) -> Result<()> {
    let file = emu.arg(0);
    let eof = stream_fd(emu, file).and_then(|fd| emu.fs.files.get(&fd)).is_none_or(|f| f.pos >= f.data.len());
    emu.ret(eof as u64);
    Ok(())
}

fn fileno(emu: &mut Emu) -> Result<()> {
    let file = emu.arg(0);
    let fd = stream_fd(emu, file).unwrap_or(u64::MAX);
    emu.ret(fd);
    Ok(())
}

/// An open directory stream: its entries (`.` and `..` first), where
/// readdir is, its descriptor, and the `struct dirent` readdir fills.
pub struct DirStream {
    entries: Vec<(String, bool)>,
    pos: usize,
    fd: u64,
    dirent: u64,
}

/// Darwin's `struct dirent` (64-bit inodes): d_ino, d_seekoff, d_reclen,
/// d_namlen, d_type, then the name (1024 bytes).
const DIRENT_SIZE: u64 = 1048;
const DT_DIR: u8 = 4;
const DT_REG: u8 = 8;

fn write_dirent(emu: &mut Emu, at: u64, index: usize, name: &str, is_dir: bool) -> Result<()> {
    emu.p.memory.write(at, &[0u8; DIRENT_SIZE as usize])?;
    let ino = name.bytes().fold(1469598103934665603u64, |h, b| (h ^ b as u64).wrapping_mul(1099511628211)) | 1;
    emu.write_u64(at, ino)?;
    emu.write_u64(at + 8, index as u64 + 1)?;
    emu.p.memory.write_u16(at + 16, DIRENT_SIZE as u16)?;
    emu.p.memory.write_u16(at + 18, name.len().min(1023) as u16)?;
    emu.p.memory.write_u8(at + 20, if is_dir { DT_DIR } else { DT_REG })?;
    emu.p.memory.write(at + 21, &name.as_bytes()[..name.len().min(1023)])?;
    Ok(())
}

/// A DIR * for the directory at `path` (with `fd` as its descriptor), or ENOENT/ENOTDIR.
fn open_dir_stream(emu: &mut Emu, path: &str, fd: Option<u64>) -> std::result::Result<u64, u64> {
    match exists(emu, path) {
        None => return Err(ENOENT),
        Some(false) => return Err(ENOTDIR),
        Some(true) => {}
    }
    let mut entries = vec![(".".to_string(), true), ("..".to_string(), true)];
    entries.extend(list_dir(emu, path).unwrap_or_default());
    let fd = match fd {
        Some(fd) => fd,
        None => {
            let fd = open_fd(emu, Vec::new());
            emu.fs.dir_fds.insert(fd, path.to_string());
            fd
        }
    };
    let handle = emu.alloc(64).map_err(|_| ENOENT)?;
    let dirent = emu.alloc(DIRENT_SIZE).map_err(|_| ENOENT)?;
    emu.fs.dirs.insert(handle, DirStream { entries, pos: 0, fd, dirent });
    Ok(handle)
}

fn ret_dir(emu: &mut Emu, r: std::result::Result<u64, u64>) -> Result<()> {
    match r {
        Ok(d) => {
            emu.ret(d);
            Ok(())
        }
        Err(e) => {
            crate::libsystem::set_errno(emu, e)?;
            emu.ret(0);
            Ok(())
        }
    }
}

/// `opendir(path)`
fn opendir(emu: &mut Emu) -> Result<()> {
    let path = emu.cstr(emu.arg(0))?;
    let r = open_dir_stream(emu, &path, None);
    ret_dir(emu, r)
}

/// `fdopendir(fd)`: a stream over a directory descriptor (from open or openat).
fn fdopendir(emu: &mut Emu) -> Result<()> {
    let fd = emu.arg(0) & 0xffff_ffff;
    let Some(path) = emu.fs.dir_fds.get(&fd).cloned() else {
        crate::libsystem::set_errno(emu, ENOTDIR)?;
        emu.ret(0);
        return Ok(());
    };
    let r = open_dir_stream(emu, &path, Some(fd));
    ret_dir(emu, r)
}

/// The next entry, written into the stream's dirent: its address, or None at the end.
fn next_entry(emu: &mut Emu, dir: u64, into: Option<u64>) -> Result<Option<u64>> {
    let Some(d) = emu.fs.dirs.get_mut(&dir) else { return Ok(None) };
    let Some((name, is_dir)) = d.entries.get(d.pos).cloned() else { return Ok(None) };
    let (index, at) = (d.pos, into.unwrap_or(d.dirent));
    d.pos += 1;
    write_dirent(emu, at, index, &name, is_dir)?;
    Ok(Some(at))
}

/// `readdir(dir)`: the next entry, or NULL at the end (errno untouched).
fn readdir(emu: &mut Emu) -> Result<()> {
    let dir = emu.arg(0);
    let e = next_entry(emu, dir, None)?.unwrap_or(0);
    emu.ret(e);
    Ok(())
}

/// `readdir_r(dir, entry, &result)`: 0, with *result the entry or NULL at the end.
fn readdir_r(emu: &mut Emu) -> Result<()> {
    let (dir, entry, result) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let e = next_entry(emu, dir, Some(entry))?.unwrap_or(0);
    emu.write_u64(result, e)?;
    emu.ret(0);
    Ok(())
}

/// `closedir(dir)`: closes its descriptor too.
fn closedir(emu: &mut Emu) -> Result<()> {
    let dir = emu.arg(0);
    match emu.fs.dirs.remove(&dir) {
        Some(d) => {
            emu.fs.dir_fds.remove(&d.fd);
            emu.fs.files.remove(&d.fd);
            emu.ret(0);
            Ok(())
        }
        None => fail(emu, EBADF),
    }
}

/// `chmod(path, mode)` / `fchmod(fd, mode)`: a data-container item's permissions.
fn chmod_path(emu: &mut Emu, path: &str, mode: u64) -> Result<()> {
    if set_attributes(emu, path, |m| m.permissions = (mode & 0o7777) as u16) {
        emu.ret(0);
        Ok(())
    } else {
        let errno = if exists(emu, path).is_some() { EACCES } else { ENOENT };
        fail(emu, errno)
    }
}

/// A random letter or digit for a temporary name, from pomme's seeded generator.
fn temp_char(emu: &mut Emu) -> u8 {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut x = emu.sys.rng_state();
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    emu.sys.set_rng_state(x);
    CHARS[(x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 32) as usize % CHARS.len()]
}

/// Fill the template's trailing X's (before `suffix` bytes) in place with a
/// name nothing has yet. Err(errno): EINVAL without X's, EEXIST if every try is taken.
fn fill_template(emu: &mut Emu, template: u64, suffix: usize) -> std::result::Result<String, u64> {
    let t = emu.cstr(template).map_err(|_| EINVAL)?;
    let end = t.len().checked_sub(suffix).ok_or(EINVAL)?;
    let xs = t[..end].bytes().rev().take_while(|&b| b == b'X').count();
    if xs == 0 {
        return Err(EINVAL);
    }
    for _ in 0..100 {
        let mut name = t.clone().into_bytes();
        for b in &mut name[end - xs..end] {
            *b = temp_char(emu);
        }
        let name = String::from_utf8(name).map_err(|_| EINVAL)?;
        if exists(emu, &name).is_none() {
            emu.p.memory.write(template, name.as_bytes()).map_err(|_| EINVAL)?;
            return Ok(name);
        }
    }
    Err(EEXIST)
}

/// `mktemp(template)`: the template, its X's made into a free name.
fn mktemp(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    match fill_template(emu, t, 0) {
        Ok(_) => {
            emu.ret(t);
            Ok(())
        }
        Err(e) => {
            crate::libsystem::set_errno(emu, e)?;
            emu.ret(0);
            Ok(())
        }
    }
}

/// `mkstemp`/`mkostemp`/`mkstemps`: make and open (read-write) a new file.
fn mkstemp_with(emu: &mut Emu, suffix: usize, extra_flags: u64) -> Result<()> {
    let t = emu.arg(0);
    let opened = fill_template(emu, t, suffix).and_then(|name| open_data(emu, &name, 2 | O_CREAT | O_EXCL | (extra_flags & O_APPEND)));
    match opened {
        Ok(fd) => {
            emu.ret(fd);
            Ok(())
        }
        Err(e) => fail(emu, e),
    }
}

/// `mkdtemp(template)`: make a new directory; the template, or NULL.
fn mkdtemp(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    match fill_template(emu, t, 0) {
        Ok(name) if make_dirs(emu, &name, false) => {
            emu.ret(t);
            Ok(())
        }
        Ok(_) => {
            crate::libsystem::set_errno(emu, ENOENT)?;
            emu.ret(0);
            Ok(())
        }
        Err(e) => {
            crate::libsystem::set_errno(emu, e)?;
            emu.ret(0);
            Ok(())
        }
    }
}

/// The phone's storage: 128 GB, half of it free, in 4 KB blocks.
pub const DISK_BYTES: u64 = 128 << 30;
pub const FREE_BYTES: u64 = 64 << 30;
const BLOCK: u64 = 4096;

/// Fill a Darwin `struct statfs` (64-bit inodes, 2168 bytes): the app's
/// files are on APFS, mounted at /private/var (the bundle, read-only, at /).
fn write_statfs(emu: &mut Emu, buf: u64, data: bool) -> Result<()> {
    emu.p.memory.write(buf, &[0u8; 2168])?;
    emu.p.memory.write_u32(buf, BLOCK as u32)?; // f_bsize
    emu.p.memory.write_u32(buf + 4, 1 << 20)?; // f_iosize
    emu.write_u64(buf + 8, DISK_BYTES / BLOCK)?; // f_blocks
    emu.write_u64(buf + 16, FREE_BYTES / BLOCK)?; // f_bfree
    emu.write_u64(buf + 24, FREE_BYTES / BLOCK)?; // f_bavail
    emu.write_u64(buf + 32, DISK_BYTES / BLOCK)?; // f_files
    emu.write_u64(buf + 40, FREE_BYTES / BLOCK)?; // f_ffree
    emu.p.memory.write_u32(buf + 48, if data { 0x0100_0004 } else { 0x0100_0001 })?; // f_fsid
    emu.p.memory.write_u32(buf + 56, 0)?; // f_owner (root)
    emu.p.memory.write_u32(buf + 60, 0x1a)?; // f_type: APFS
    const MNT_LOCAL: u32 = 0x1000;
    const MNT_RDONLY: u32 = 0x1;
    emu.p.memory.write_u32(buf + 64, MNT_LOCAL | if data { 0 } else { MNT_RDONLY })?; // f_flags
    emu.p.memory.write(buf + 72, b"apfs")?; // f_fstypename
    let (on, from) = if data { ("/private/var", "/dev/disk1s2") } else { ("/", "/dev/disk1s1") };
    emu.p.memory.write(buf + 88, on.as_bytes())?; // f_mntonname
    emu.p.memory.write(buf + 1112, from.as_bytes())?; // f_mntfromname
    Ok(())
}

/// `statfs(path, buf)`
fn statfs(emu: &mut Emu) -> Result<()> {
    let (path, buf) = (emu.cstr(emu.arg(0))?, emu.arg(1));
    if exists(emu, &path).is_none() {
        return fail(emu, ENOENT);
    }
    let data = data_path(&path).is_some();
    write_statfs(emu, buf, data)?;
    emu.ret(0);
    Ok(())
}

/// `fstatfs(fd, buf)`
fn fstatfs(emu: &mut Emu) -> Result<()> {
    let (fd, buf) = (emu.arg(0) & 0xffff_ffff, emu.arg(1));
    let data = match emu.fs.files.get(&fd) {
        Some(f) => f.path.is_some(),
        None if emu.fs.dir_fds.contains_key(&fd) => emu.fs.dir_fds.get(&fd).is_some_and(|p| data_path(p).is_some()),
        None => return fail(emu, EBADF),
    };
    write_statfs(emu, buf, data)?;
    emu.ret(0);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_utimes", utimes),
    ("_opendir", opendir),
    ("_fdopendir", fdopendir),
    ("_readdir", readdir),
    ("_readdir_r", readdir_r),
    ("_closedir", closedir),
    ("_rewinddir", |e| {
        let dir = e.arg(0);
        if let Some(d) = e.fs.dirs.get_mut(&dir) {
            d.pos = 0;
        }
        Ok(())
    }),
    ("_telldir", |e| {
        let dir = e.arg(0);
        let p = e.fs.dirs.get(&dir).map_or(0, |d| d.pos as u64);
        e.ret(p);
        Ok(())
    }),
    ("_seekdir", |e| {
        let (dir, at) = (e.arg(0), e.arg(1) as usize);
        if let Some(d) = e.fs.dirs.get_mut(&dir) {
            d.pos = at.min(d.entries.len());
        }
        Ok(())
    }),
    ("_dirfd", |e| {
        let dir = e.arg(0);
        let fd = e.fs.dirs.get(&dir).map_or(u64::MAX, |d| d.fd);
        e.ret(fd);
        Ok(())
    }),
    ("_chmod", |e| {
        let (path, mode) = (e.cstr(e.arg(0))?, e.arg(1));
        chmod_path(e, &path, mode)
    }),
    ("_fchmod", |e| {
        let (fd, mode) = (e.arg(0) & 0xffff_ffff, e.arg(1));
        let path = e.fs.files.get(&fd).and_then(|f| f.path.clone()).or_else(|| e.fs.dir_fds.get(&fd).cloned());
        match path {
            Some(p) => chmod_path(e, &p, mode),
            None => fail(e, EBADF),
        }
    }),
    ("_fdopen", fdopen),
    ("_readv", readv),
    ("_futimes", futimes),
    ("_utimensat", utimensat),
    ("_statfs", statfs),
    ("_fstatfs", fstatfs),
    ("_mktemp", mktemp),
    ("_mkstemp", |e| mkstemp_with(e, 0, 0)),
    ("_mkostemp", |e| {
        let flags = e.arg(1);
        mkstemp_with(e, 0, flags)
    }),
    ("_mkstemps", |e| {
        let suffix = e.arg(1) as usize;
        mkstemp_with(e, suffix, 0)
    }),
    ("_mkdtemp", mkdtemp),
    ("_open", open),
    ("_close", close),
    ("_read", read),
    ("_pread", pread),
    ("_lseek", lseek),
    ("_fstat", fstat),
    ("_stat", stat),
    ("_lstat", stat),
    ("_access", access),
    ("_mmap", mmap),
    ("_fopen", fopen),
    ("_fread", fread),
    ("_fclose", fclose),
    ("_fseek", fseek),
    ("_fseeko", fseek),
    ("_ftell", ftell),
    ("_ftello", ftell),
    ("_rewind", rewind),
    ("_feof", feof),
    ("_fileno", fileno),
    ("_write", write),
    ("_openat", openat),
    ("_pipe", pipe),
    ("_fcntl", fcntl),
    ("_kqueue", kqueue),
    ("_kevent", kevent),
    ("_pwrite", pwrite),
    ("_ftruncate", ftruncate),
    ("_mkdir", mkdir),
    ("_unlink", unlink),
    ("_rmdir", rmdir),
    ("_rename", rename),
];

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

#[derive(Default)]
pub struct Container {
    files: HashMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
}

/// A fresh install's data container.
fn container(emu: &mut Emu) -> &mut Container {
    emu.fs.data.get_or_insert_with(|| {
        let home = crate::foundation::DATA_CONTAINER;
        let mut dirs = BTreeSet::new();
        for d in ["", "/Documents", "/Library", "/Library/Caches", "/Library/Application Support", "/Library/Preferences", "/SystemData", "/tmp"] {
            dirs.insert(format!("{home}{d}"));
        }
        Container { files: HashMap::new(), dirs }
    })
}

/// A normalized guest path in the data container (or /tmp), if it is in one.
pub fn data_path(path: &str) -> Option<String> {
    let mut p = if path.starts_with("/var/") || path.starts_with("/tmp") { format!("/private{path}") } else { path.to_string() };
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
    let c = container(emu);
    if !c.dirs.contains(parent(&p)) && !p.starts_with("/private/tmp") {
        return false;
    }
    c.files.insert(p, bytes.to_vec());
    true
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
        c.files.insert(p.clone(), data.clone());
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
    let fd = emu.arg(0);
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

fn read(emu: &mut Emu) -> Result<()> {
    let (fd, buf, n) = (emu.arg(0), emu.arg(1), emu.arg(2));
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
    let (fd, buf, n, off) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
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
    container(emu).files.insert(path, data);
    Some(bytes.len() as u64)
}

/// `write(fd, buf, n)`: stdout/stderr print; data-container files are written.
fn write(emu: &mut Emu) -> Result<()> {
    let (fd, buf, n) = (emu.arg(0), emu.arg(1), emu.arg(2));
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
    let (fd, cmd) = (emu.arg(0), emu.arg(1));
    // The variadic argument is on the stack.
    let arg = emu.read_u64(emu.cpu.sp)?;
    let r = match cmd {
        F_GETFL => emu.fs.status_flags.get(&fd).copied().unwrap_or(0) | if emu.fs.pipe_ends.get(&fd).is_some_and(|e| !e.1) { 1 } else { 0 },
        F_SETFL => {
            emu.fs.status_flags.insert(fd, arg & O_NONBLOCK);
            0
        }
        F_GETFD | F_SETFD => 0,
        _ => return fail(emu, EINVAL),
    };
    emu.ret(r);
    Ok(())
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
    let (fd, buf, n, off) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
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
    let (fd, len) = (emu.arg(0), emu.arg(1) as usize);
    let Some(f) = emu.fs.files.get_mut(&fd) else { return fail(emu, EBADF) };
    let Some(path) = f.path.clone() else { return fail(emu, EACCES) };
    f.data.resize(len, 0);
    let data = f.data.clone();
    container(emu).files.insert(path, data);
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

fn rename(emu: &mut Emu) -> Result<()> {
    let (from, to) = (emu.cstr(emu.arg(0))?, emu.cstr(emu.arg(1))?);
    let (Some(f), Some(t)) = (data_path(&from), data_path(&to)) else { return fail(emu, EACCES) };
    let c = container(emu);
    let Some(bytes) = c.files.remove(&f) else { return fail(emu, ENOENT) };
    c.files.insert(t, bytes);
    emu.ret(0);
    Ok(())
}

/// `lseek(fd, offset, whence)`
fn lseek(emu: &mut Emu) -> Result<()> {
    let (fd, off, whence) = (emu.arg(0), emu.arg(1) as i64, emu.arg(2));
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
    emu.p.memory.write(buf, &[0u8; 144])?;
    let mode: u16 = if is_dir { 0o040755 } else { 0o100644 };
    emu.p.memory.write_u16(buf + 4, mode)?;
    emu.p.memory.write_u16(buf + 6, 1)?; // nlink
    emu.p.memory.write_u32(buf + 16, 501)?; // uid
    emu.p.memory.write_u32(buf + 20, 501)?; // gid
    emu.write_u64(buf + 96, size)?;
    emu.write_u64(buf + 104, size.div_ceil(512))?;
    emu.p.memory.write_u32(buf + 112, 4096)?; // blksize
    Ok(())
}

fn fstat(emu: &mut Emu) -> Result<()> {
    let (fd, buf) = (emu.arg(0), emu.arg(1));
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
    if let Some(p) = data_path(&path) {
        let c = container(emu);
        let (len, dir) = match (c.dirs.contains(&p), c.files.get(&p)) {
            (true, _) => (0, true),
            (_, Some(d)) => (d.len() as u64, false),
            _ => return fail(emu, ENOENT),
        };
        write_stat(emu, buf, len, dir)?;
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

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
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

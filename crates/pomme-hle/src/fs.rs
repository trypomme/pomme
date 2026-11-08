//! A virtual filesystem.
//!
//! Inside the guest the app lives where iOS puts it
//! (`/private/var/containers/Bundle/Application/pomme/<name>.app`). That
//! directory maps, read-only, onto the `.app` on the host. Everything else
//! doesn't exist (yet). Files are read whole into host memory when opened, so
//! reads are deterministic even if the host file changes later.

use crate::{Emu, HostFn, Result};
use pomme_loader::vfs;
use std::collections::HashMap;
use std::path::PathBuf;

pub const CONTAINER: &str = "/private/var/containers/Bundle/Application/pomme";
const ENOENT: u64 = 2;
const EBADF: u64 = 9;
const EACCES: u64 = 13;

pub struct OpenFile {
    data: Vec<u8>,
    pos: usize,
}

#[derive(Default)]
pub struct State {
    files: HashMap<u64, OpenFile>,
    next_fd: u64,
    /// FILE* → fd
    streams: HashMap<u64, u64>,
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

/// Read a whole guest file.
pub fn read_file(emu: &Emu, guest: &str) -> Option<Vec<u8>> {
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
    if emu.fs.next_fd < 3 {
        emu.fs.next_fd = 3;
    }
    let fd = emu.fs.next_fd;
    emu.fs.next_fd += 1;
    emu.fs.files.insert(fd, OpenFile { data, pos: 0 });
    fd
}

/// `open(path, flags, mode)` — read-only.
fn open(emu: &mut Emu) -> Result<()> {
    let (path, flags) = (emu.cstr(emu.arg(0))?, emu.arg(1));
    const O_ACCMODE: u64 = 3;
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
    let Some(len) = emu.fs.files.get(&fd).map(|f| f.data.len() as u64) else { return fail(emu, EBADF) };
    write_stat(emu, buf, len, false)?;
    emu.ret(0);
    Ok(())
}

/// `stat(path, buf)` / `lstat`
fn stat(emu: &mut Emu) -> Result<()> {
    let (path, buf) = (emu.cstr(emu.arg(0))?, emu.arg(1));
    let Some(p) = host_path(emu, &path) else { return fail(emu, ENOENT) };
    let Some(len) = vfs::len(&p) else { return fail(emu, ENOENT) };
    write_stat(emu, buf, len, vfs::is_dir(&p))?;
    emu.ret(0);
    Ok(())
}

/// `access(path, mode)`: read-only bundle.
fn access(emu: &mut Emu) -> Result<()> {
    let (path, mode) = (emu.cstr(emu.arg(0))?, emu.arg(1));
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
];

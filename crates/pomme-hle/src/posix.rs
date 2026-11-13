//! More of libSystem: process identity, OSAtomic, malloc zones, time
//! conversion, users, Mach ports and loader queries, plus files and sockets.
//!
//! There is no filesystem or network yet: file and socket calls fail cleanly
//! (-1 / NULL), which apps and CoreFoundation handle as "not there".

use crate::libsystem::call_block;
use crate::{Emu, HostFn, Result};

const UID: u64 = 501; // "mobile"
const GID: u64 = 501;
const HOME: &str = "/var/mobile";

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_zero(emu: &mut Emu) -> Result<()> {
    ret(emu, 0)
}

fn ret_one(emu: &mut Emu) -> Result<()> {
    ret(emu, 1)
}

fn fail(emu: &mut Emu) -> Result<()> {
    ret(emu, u64::MAX) // -1
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn write_cstr_buf(emu: &mut Emu, buf: u64, size: u64, s: &str) -> Result<bool> {
    if (s.len() as u64) + 1 > size {
        return Ok(false);
    }
    emu.p.memory.write(buf, s.as_bytes())?;
    emu.p.memory.write_u8(buf + s.len() as u64, 0)?;
    Ok(true)
}

// ---- identity ----

fn uid(emu: &mut Emu) -> Result<()> {
    ret(emu, UID)
}

fn gid(emu: &mut Emu) -> Result<()> {
    ret(emu, GID)
}

fn pthread_getugid_np(emu: &mut Emu) -> Result<()> {
    emu.p.memory.write_u32(emu.arg(0), UID as u32)?;
    emu.p.memory.write_u32(emu.arg(1), GID as u32)?;
    ret_zero(emu)
}

fn executable_path(emu: &Emu) -> String {
    format!("/private/var/containers/Bundle/Application/pomme/{0}.app/{0}", emu.p.main().name)
}

/// `_NSGetExecutablePath(char *buf, uint32_t *size)`
fn ns_get_executable_path(emu: &mut Emu) -> Result<()> {
    let (buf, size_p) = (emu.arg(0), emu.arg(1));
    let path = executable_path(emu);
    let size = emu.p.memory.read_u32(size_p)? as u64;
    if write_cstr_buf(emu, buf, size, &path)? {
        ret_zero(emu)
    } else {
        emu.p.memory.write_u32(size_p, path.len() as u32 + 1)?;
        fail(emu)
    }
}

fn getcwd(emu: &mut Emu) -> Result<()> {
    let (buf, size) = (emu.arg(0), emu.arg(1));
    if buf != 0 && write_cstr_buf(emu, buf, size, "/")? {
        return ret(emu, buf);
    }
    ret_zero(emu)
}

fn gethostname(emu: &mut Emu) -> Result<()> {
    let ok = write_cstr_buf(emu, emu.arg(0), emu.arg(1), "pomme")?;
    ret(emu, if ok { 0 } else { u64::MAX })
}

/// A static `struct passwd` for the one user.
fn passwd(emu: &mut Emu, out: u64) -> Result<()> {
    let name = emu.alloc_cstr("mobile")?;
    let empty = emu.alloc_cstr("")?;
    let dir = emu.alloc_cstr(HOME)?;
    let shell = emu.alloc_cstr("/bin/sh")?;
    emu.write_u64(out, name)?;
    emu.write_u64(out + 8, empty)?;
    emu.p.memory.write_u32(out + 16, UID as u32)?;
    emu.p.memory.write_u32(out + 20, GID as u32)?;
    emu.write_u64(out + 24, 0)?;
    emu.write_u64(out + 32, empty)?;
    emu.write_u64(out + 40, name)?;
    emu.write_u64(out + 48, dir)?;
    emu.write_u64(out + 56, shell)?;
    emu.write_u64(out + 64, 0)?;
    Ok(())
}

fn getpwuid(emu: &mut Emu) -> Result<()> {
    let p = emu.alloc(80)?;
    passwd(emu, p)?;
    ret(emu, p)
}

/// `getpwuid_r(uid, struct passwd *pwd, char *buf, size_t buflen, struct passwd **result)`
fn getpwuid_r(emu: &mut Emu) -> Result<()> {
    let (pwd, result) = (emu.arg(1), emu.arg(4));
    passwd(emu, pwd)?;
    emu.write_u64(result, pwd)?;
    ret_zero(emu)
}

fn getrlimit(emu: &mut Emu) -> Result<()> {
    let r = emu.arg(1);
    emu.write_u64(r, 1 << 20)?;
    emu.write_u64(r + 8, u64::MAX)?;
    ret_zero(emu)
}

fn nl_langinfo(emu: &mut Emu) -> Result<()> {
    let s = if emu.arg(0) == 0 { "UTF-8" } else { "" }; // CODESET
    let p = emu.alloc_cstr(s)?;
    ret(emu, p)
}

// ---- OSAtomic (one thread: plain memory operations) ----

fn os_atomic_add64(emu: &mut Emu) -> Result<()> {
    let (amount, p) = (emu.arg(0), emu.arg(1));
    let v = emu.read_u64(p)?.wrapping_add(amount);
    emu.write_u64(p, v)?;
    ret(emu, v)
}

fn os_atomic_inc32(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    let v = emu.p.memory.read_u32(p)?.wrapping_add(1);
    emu.p.memory.write_u32(p, v)?;
    ret(emu, v as i32 as i64 as u64)
}

fn os_atomic_dec32(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    let v = emu.p.memory.read_u32(p)?.wrapping_sub(1);
    emu.p.memory.write_u32(p, v)?;
    ret(emu, v as i32 as i64 as u64)
}

fn os_atomic_cas32(emu: &mut Emu) -> Result<()> {
    let (old, new, p) = (emu.arg(0) as u32, emu.arg(1) as u32, emu.arg(2));
    let ok = emu.p.memory.read_u32(p)? == old;
    if ok {
        emu.p.memory.write_u32(p, new)?;
    }
    ret(emu, ok as u64)
}

fn os_atomic_cas_ptr(emu: &mut Emu) -> Result<()> {
    let (old, new, p) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let ok = emu.read_u64(p)? == old;
    if ok {
        emu.write_u64(p, new)?;
    }
    ret(emu, ok as u64)
}

// ---- malloc zones (one zone: the heap) ----

fn shift_args(emu: &mut Emu, by: usize) {
    for i in 0..8 - by {
        emu.cpu.x[i] = emu.cpu.x[i + by];
    }
}

/// The default malloc zone: a real `malloc_zone_t` (version 13, as
/// <malloc/malloc.h> lays it out) whose functions are pomme's heap, so code
/// that calls through the zone (`zone->size(zone, p)`, as SQLite does) works.
/// Every zone is this one: pomme has one heap.
fn default_zone(emu: &mut Emu) -> Result<u64> {
    if emu.sys.malloc_zone != 0 {
        return Ok(emu.sys.malloc_zone);
    }
    let z = emu.alloc(152)?;
    let fns: [(u64, HostFn); 12] = [
        (16, zone_size),
        (24, zone_malloc),
        (32, zone_calloc),
        (40, zone_valloc),
        (48, zone_free),
        (56, zone_realloc),
        (64, noop),
        (112, zone_memalign),
        (120, zone_free), // free_definite_size(zone, p, size)
        (128, ret_zero),  // pressure_relief: nothing to give back
        (136, ret_one),   // claimed_address: everything's from the one heap
        (144, zone_free), // try_free_default
    ];
    for (off, f) in fns {
        let t = emu.host_thunk(f)?;
        emu.write_u64(z + off, t)?;
    }
    let name = emu.alloc_cstr("DefaultMallocZone")?;
    emu.write_u64(z + 72, name)?;
    emu.p.memory.write_u32(z + 104, 13)?; // version
    emu.sys.malloc_zone = z;
    Ok(z)
}

fn ret_default_zone(emu: &mut Emu) -> Result<()> {
    let z = default_zone(emu)?;
    ret(emu, z)
}

/// `zone->size(zone, p)`: the block's usable size (0 if it isn't one).
fn zone_size(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(1);
    let s = if p == 0 { 0 } else { emu.heap.size(&emu.p.memory, p)? };
    ret(emu, s)
}

/// `zone->valloc(zone, size)`: page-aligned.
fn zone_valloc(emu: &mut Emu) -> Result<()> {
    let size = emu.arg(1);
    alloc_aligned(emu, 16384, size)
}

fn zone_malloc(emu: &mut Emu) -> Result<()> {
    let p = crate::libc::guest_alloc(emu, emu.arg(1))?;
    ret(emu, p)
}

fn zone_calloc(emu: &mut Emu) -> Result<()> {
    let p = crate::libc::guest_alloc(emu, emu.arg(1).saturating_mul(emu.arg(2)))?;
    ret(emu, p)
}

fn zone_free(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(1);
    Ok(emu.heap.free(&emu.p.memory, p)?)
}

fn zone_realloc(emu: &mut Emu) -> Result<()> {
    shift_args(emu, 1);
    if emu.cpu.x[1] > crate::heap::MAX_ALLOC {
        return ret(emu, 0);
    }
    let p = emu.heap.realloc(&mut emu.p.memory, emu.cpu.x[0], emu.cpu.x[1])?;
    ret(emu, p)
}

fn zone_memalign(emu: &mut Emu) -> Result<()> {
    let (align, size) = (emu.arg(1), emu.arg(2));
    alloc_aligned(emu, align, size)
}

fn alloc_aligned(emu: &mut Emu, align: u64, size: u64) -> Result<()> {
    if size > crate::heap::MAX_ALLOC {
        return ret(emu, 0);
    }
    let p = emu.heap.alloc_aligned(&mut emu.p.memory, size, align.max(16).next_power_of_two())?;
    ret(emu, p)
}


/// `mach_vm_allocate(task, mach_vm_address_t *addr, size, flags)`
fn mach_vm_allocate(emu: &mut Emu) -> Result<()> {
    let (out, size) = (emu.arg(1), emu.arg(2));
    alloc_aligned(emu, pomme_mem::PAGE_SIZE, size)?;
    let p = emu.cpu.x[0];
    emu.write_u64(out, p)?;
    ret_zero(emu)
}

/// `vm_map(task, vm_address_t *addr, size, mask, flags, object, offset, copy, cur, max, inherit)`:
/// fresh zeroed memory (anywhere), aligned to `mask + 1`.
fn vm_map(emu: &mut Emu) -> Result<()> {
    let (out, size, mask) = (emu.arg(1), emu.arg(2), emu.arg(3));
    let align = (mask + 1).max(pomme_mem::PAGE_SIZE).next_power_of_two();
    alloc_aligned(emu, align, size)?;
    let p = emu.cpu.x[0];
    emu.p.memory.write(p, &vec![0; size as usize])?;
    emu.write_u64(out, p)?;
    ret_zero(emu)
}

// ---- setjmp / longjmp ----
// jmp_buf: x19…x28, fp, lr, sp, then d8…d15. The host stub returns through x30,
// so longjmp lands at setjmp's return address with the saved registers.

fn setjmp(emu: &mut Emu) -> Result<()> {
    let buf = emu.arg(0);
    for i in 0..12 {
        let v = emu.cpu.x[19 + i];
        emu.write_u64(buf + i as u64 * 8, v)?;
    }
    emu.write_u64(buf + 96, emu.cpu.sp)?;
    for i in 0..8 {
        let v = emu.cpu.q[8 + i] as u64;
        emu.write_u64(buf + 104 + i as u64 * 8, v)?;
    }
    ret_zero(emu)
}

fn longjmp(emu: &mut Emu) -> Result<()> {
    let (buf, val) = (emu.arg(0), emu.arg(1) as u32);
    for i in 0..12 {
        emu.cpu.x[19 + i] = emu.read_u64(buf + i as u64 * 8)?;
    }
    emu.cpu.sp = emu.read_u64(buf + 96)?;
    for i in 0..8 {
        emu.cpu.q[8 + i] = emu.read_u64(buf + 104 + i as u64 * 8)? as u128;
    }
    ret(emu, if val == 0 { 1 } else { val as u64 })
}

// ---- strings ----

fn cbytes(emu: &Emu, addr: u64) -> Result<Vec<u8>> {
    emu.cstr_bytes(addr)
}

/// `strlcpy(dst, src, size)` → strlen(src)
fn strlcpy(emu: &mut Emu) -> Result<()> {
    let (dst, src, size) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let s = cbytes(emu, src)?;
    if size > 0 {
        let n = s.len().min(size as usize - 1);
        emu.p.memory.write(dst, &s[..n])?;
        emu.p.memory.write_u8(dst + n as u64, 0)?;
    }
    ret(emu, s.len() as u64)
}

/// `strlcat(dst, src, size)` → initial strlen(dst) + strlen(src)
fn strlcat(emu: &mut Emu) -> Result<()> {
    let (dst, src, size) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let d = cbytes(emu, dst)?;
    let s = cbytes(emu, src)?;
    if (d.len() as u64) < size {
        let room = size as usize - d.len() - 1;
        let n = s.len().min(room);
        emu.p.memory.write(dst + d.len() as u64, &s[..n])?;
        emu.p.memory.write_u8(dst + (d.len() + n) as u64, 0)?;
    }
    ret(emu, (d.len() + s.len()) as u64)
}

fn strcat(emu: &mut Emu) -> Result<()> {
    let (dst, src) = (emu.arg(0), emu.arg(1));
    let d = cbytes(emu, dst)?;
    let mut s = cbytes(emu, src)?;
    s.push(0);
    emu.p.memory.write(dst + d.len() as u64, &s)?;
    ret(emu, dst)
}

fn strnlen(emu: &mut Emu) -> Result<()> {
    let (s, max) = (emu.arg(0), emu.arg(1));
    let mut n = 0;
    while n < max && emu.p.memory.read_u8(s + n)? != 0 {
        n += 1;
    }
    ret(emu, n)
}

fn strncasecmp(emu: &mut Emu) -> Result<()> {
    let n = emu.arg(2) as usize;
    let mut a = cbytes(emu, emu.arg(0))?.to_ascii_lowercase();
    let mut b = cbytes(emu, emu.arg(1))?.to_ascii_lowercase();
    a.truncate(n);
    b.truncate(n);
    ret(emu, a.cmp(&b) as i8 as i64 as u64)
}

/// `strnstr(haystack, needle, len)`
fn strnstr(emu: &mut Emu) -> Result<()> {
    let (h, n, len) = (emu.arg(0), emu.arg(1), emu.arg(2) as usize);
    let mut hay = cbytes(emu, h)?;
    hay.truncate(len);
    let needle = cbytes(emu, n)?;
    let r = if needle.is_empty() { Some(0) } else { hay.windows(needle.len()).position(|w| w == needle) };
    ret(emu, r.map_or(0, |i| h + i as u64))
}

fn atoi(emu: &mut Emu) -> Result<()> {
    let s = emu.cstr(emu.arg(0))?;
    let t = s.trim_start();
    let end = t.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')))).map_or(t.len(), |(i, _)| i);
    ret(emu, t[..end].parse::<i64>().unwrap_or(0) as u64)
}

/// `__strlcpy_chk(dst, src, size, dstlen)` and friends: same as the unchecked call.
fn strlcpy_chk(emu: &mut Emu) -> Result<()> {
    strlcpy(emu)
}

fn strlcat_chk(emu: &mut Emu) -> Result<()> {
    strlcat(emu)
}

/// `__sprintf_chk(buf, flags, len, fmt, ...)`
fn sprintf_chk(emu: &mut Emu) -> Result<()> {
    let fmt = emu.cstr(emu.arg(3))?.into_bytes();
    let s = crate::printf::format(emu, &fmt, &mut crate::printf::Args::new(emu.cpu.sp))?;
    let buf = emu.arg(0);
    emu.p.memory.write(buf, &s)?;
    emu.p.memory.write_u8(buf + s.len() as u64, 0)?;
    ret(emu, s.len() as u64)
}

fn flsl(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    ret(emu, if v == 0 { 0 } else { 64 - v.leading_zeros() as u64 })
}

/// `div(int, int)` → `div_t {int quot; int rem;}` packed in x0.
fn div(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0) as i32, emu.arg(1) as i32);
    let (q, r) = if b == 0 { (0, 0) } else { (a.wrapping_div(b), a.wrapping_rem(b)) };
    ret(emu, (q as u32 as u64) | ((r as u32 as u64) << 32))
}

fn scalbn(emu: &mut Emu) -> Result<()> {
    let x = f64::from_bits(emu.cpu.q[0] as u64);
    let n = emu.arg(0) as i32;
    emu.cpu.q[0] = (x * 2f64.powi(n)).to_bits() as u128;
    Ok(())
}

fn tanhf(emu: &mut Emu) -> Result<()> {
    let x = f32::from_bits(emu.cpu.q[0] as u32);
    emu.cpu.q[0] = x.tanh().to_bits() as u128;
    Ok(())
}

/// `bsearch(key, base, n, size, compar)`
fn bsearch(emu: &mut Emu) -> Result<()> {
    let (key, base, n, size, cmp) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3), emu.arg(4));
    let (mut lo, mut hi) = (0u64, n);
    while lo < hi {
        let mid = (lo + hi) / 2;
        let elem = base + mid * size;
        let c = emu.call(cmp, &[key, elem])? as i32;
        match c {
            0 => return ret(emu, elem),
            c if c < 0 => hi = mid,
            _ => lo = mid + 1,
        }
    }
    ret_zero(emu)
}

// ---- Mach ports and timers: names only; nothing is ever received ----




fn mach_port_type(emu: &mut Emu) -> Result<()> {
    emu.p.memory.write_u32(emu.arg(2), 0)?;
    ret_zero(emu)
}



fn mach_error_string(emu: &mut Emu) -> Result<()> {
    let p = emu.alloc_cstr("(pomme) mach error")?;
    ret(emu, p)
}

// ---- dispatch extras ----

/// `dispatch_apply(n, queue, block)` — block(i), in order.
fn dispatch_apply(emu: &mut Emu) -> Result<()> {
    let (n, block) = (emu.arg(0), emu.arg(2));
    for i in 0..n {
        call_block(emu, block, &[i])?;
    }
    Ok(())
}

// ---- loader queries that read real load commands from guest memory ----

const LC_SEGMENT_64: u32 = 0x19;

/// Find `segname` (and optionally `sectname`) in the load commands of the
/// Mach-O header at `mh`. Returns the address of the segment_command_64 or section_64.
fn find_load_command(emu: &Emu, mh: u64, segname: &str, sectname: Option<&str>) -> Result<u64> {
    let ncmds = emu.p.memory.read_u32(mh + 16)?;
    let mut lc = mh + 32;
    for _ in 0..ncmds {
        let cmd = emu.p.memory.read_u32(lc)?;
        let size = emu.p.memory.read_u32(lc + 4)? as u64;
        if cmd == LC_SEGMENT_64 && emu.p.memory.read_cstr(lc + 8, 16)? == segname {
            let Some(sect) = sectname else { return Ok(lc) };
            let nsects = emu.p.memory.read_u32(lc + 64)? as u64;
            for i in 0..nsects {
                let s = lc + 72 + i * 80;
                if emu.p.memory.read_cstr(s, 16)? == sect {
                    return Ok(s);
                }
            }
        }
        lc += size;
    }
    Ok(0)
}

/// `getsectbynamefromheader_64(mh, segname, sectname)` → const struct section_64 *
fn getsectbynamefromheader(emu: &mut Emu) -> Result<()> {
    let (mh, seg, sect) = (emu.arg(0), emu.cstr(emu.arg(1))?, emu.cstr(emu.arg(2))?);
    let r = find_load_command(emu, mh, &seg, Some(&sect))?;
    ret(emu, r)
}

/// `getsegbyname(segname)` in the main executable.
fn getsegbyname(emu: &mut Emu) -> Result<()> {
    let seg = emu.cstr(emu.arg(0))?;
    let mh = emu.p.main().header;
    let r = find_load_command(emu, mh, &seg, None)?;
    ret(emu, r)
}

fn dyld_image_path_containing_address(emu: &mut Emu) -> Result<()> {
    let addr = emu.arg(0);
    let Some(img) = emu.p.images.iter().find(|i| addr >= i.start && addr < i.end) else { return ret_zero(emu) };
    let path = img.install_name.clone().unwrap_or_else(|| executable_path(emu));
    let p = emu.alloc_cstr(&path)?;
    ret(emu, p)
}

fn writev(emu: &mut Emu) -> Result<()> {
    let (fd, iov, n) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2));
    if crate::sockets::owns(emu, fd) {
        return crate::sockets::writev(emu, fd, iov, n);
    }
    let mut total = 0;
    for i in 0..n {
        let base = emu.read_u64(iov + i * 16)?;
        let len = emu.read_u64(iov + i * 16 + 8)?;
        if fd == 1 || fd == 2 {
            let b = emu.p.memory.read(base, len as usize)?.to_vec();
            crate::printf::guest_print(emu, &b);
        }
        total += len;
    }
    ret(emu, total)
}


/// `uname(struct utsname *)`: five 256-byte fields, as an iPhone on the
/// presented iOS (iOS 26 runs Darwin 25).
fn uname(emu: &mut Emu) -> Result<()> {
    let buf = emu.arg(0);
    let release = format!("{}.0.0", emu.system.os.darwin_major());
    let version = format!("Darwin Kernel Version {release}: pomme");
    let model = emu.device.model();
    for (i, v) in ["Darwin", "pomme", release.as_str(), version.as_str(), model].iter().enumerate() {
        let at = buf + i as u64 * 256;
        emu.p.memory.write(at, &[0u8; 256])?;
        emu.p.memory.write(at, v.as_bytes())?;
    }
    ret_zero(emu)
}

/// `syscall(number, ...)`: the few apps use directly.
fn syscall(emu: &mut Emu) -> Result<()> {
    const SYS_KDEBUG_TRACE: u64 = 180;
    const SYS_KDEBUG_TRACE64: u64 = 179;
    const SYS_THREAD_SELFID: u64 = 372;
    // gettid(uid_t *, gid_t *): the thread's own credentials (pthread_setugid_np).
    // None is set, so ESRCH, as on a phone (it isn't Linux's gettid).
    const SYS_GETTID: u64 = 286;
    match emu.arg(0) {
        SYS_THREAD_SELFID => ret(emu, emu.th.current as u64 + 1),
        SYS_GETTID => {
            crate::libsystem::set_errno(emu, 3)?; // ESRCH
            fail(emu)
        }
        SYS_KDEBUG_TRACE | SYS_KDEBUG_TRACE64 => ret_zero(emu),
        n => {
            println!("[syscall] {n} isn't implemented (returning ENOSYS)");
            crate::libsystem::set_errno(emu, 78)?; // ENOSYS
            fail(emu)
        }
    }
}

/// `asl_log(client, msg, level, format, ...)`: print it (the Swift runtime reports fatal errors here).
fn asl_log(emu: &mut Emu) -> Result<()> {
    let fmt = emu.cstr(emu.arg(3))?.into_bytes();
    let s = crate::printf::format(emu, &fmt, &mut crate::printf::Args::new(emu.cpu.sp))?;
    println!("[asl] {}", String::from_utf8_lossy(&s).trim_end());
    ret_zero(emu)
}

/// `uuid_generate_random(uuid_t out)`: a version-4 UUID from a seeded generator,
/// so runs stay reproducible.
fn uuid_generate_random(emu: &mut Emu) -> Result<()> {
    let out = emu.arg(0);
    let bytes = random_uuid(emu);
    emu.p.memory.write(out, &bytes)?;
    Ok(())
}

/// A random (version 4) UUID's bytes, from pomme's seeded generator.
pub(crate) fn random_uuid(emu: &mut Emu) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    for b in bytes.iter_mut() {
        // xorshift64*
        let mut x = emu.sys.rng_state();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        emu.sys.set_rng_state(x);
        *b = (x.wrapping_mul(0x2545_f491_4f6c_dd1d) >> 56) as u8;
    }
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // RFC 4122 variant
    bytes
}

/// `uuid_unparse_upper(const uuid_t uu, char *out)` (and lower): 36 chars + NUL.
fn uuid_unparse(emu: &mut Emu, upper: bool) -> Result<()> {
    let (uu, out) = (emu.arg(0), emu.arg(1));
    let b = emu.p.memory.read(uu, 16)?.to_vec();
    let hex: Vec<String> = b.iter().map(|x| if upper { format!("{x:02X}") } else { format!("{x:02x}") }).collect();
    let s = format!("{}-{}-{}-{}-{}", hex[0..4].concat(), hex[4..6].concat(), hex[6..8].concat(), hex[8..10].concat(), hex[10..16].concat());
    emu.p.memory.write(out, s.as_bytes())?;
    emu.p.memory.write_u8(out + 36, 0)?;
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_uuid_unparse_upper", |e| uuid_unparse(e, true)),
    ("_uuid_unparse_lower", |e| uuid_unparse(e, false)),
    ("_uuid_unparse", |e| uuid_unparse(e, false)),
    ("_uuid_generate_random", uuid_generate_random),
    ("_uuid_generate", uuid_generate_random),
    ("_asl_log", asl_log),
    // No native backtraces for the guest (pomme prints its own on a stop).
    ("_backtrace", ret_zero),
    ("_backtrace_symbols", ret_zero),
    ("_uname", uname),
    ("_syscall", syscall),
    ("_getuid", uid),
    ("_geteuid", uid),
    ("_getgid", gid),
    ("_getegid", gid),
    ("_issetugid", ret_zero),
    ("_getppid", ret_one),
    ("_pthread_getugid_np", pthread_getugid_np),
    ("__NSGetExecutablePath", ns_get_executable_path),
    ("_getcwd", getcwd),
    ("_gethostname", gethostname),
    ("_gethostuuid", fail),
    ("_getpwuid", getpwuid),
    ("_getpwnam", getpwuid),
    ("_getpwuid_r", getpwuid_r),
    ("_getrlimit", getrlimit),
    ("_setrlimit", ret_zero),
    ("_nl_langinfo", nl_langinfo),
    ("_OSAtomicAdd64", os_atomic_add64),
    ("_OSAtomicAdd64Barrier", os_atomic_add64),
    ("_OSAtomicIncrement32", os_atomic_inc32),
    ("_OSAtomicIncrement32Barrier", os_atomic_inc32),
    ("_OSAtomicDecrement32", os_atomic_dec32),
    ("_OSAtomicDecrement32Barrier", os_atomic_dec32),
    ("_OSAtomicCompareAndSwap32Barrier", os_atomic_cas32),
    ("_OSAtomicCompareAndSwapPtrBarrier", os_atomic_cas_ptr),
    ("_OSAtomicCompareAndSwapLongBarrier", os_atomic_cas_ptr),
    ("_OSMemoryBarrier", noop),
    ("_malloc_default_zone", ret_default_zone),
    ("_malloc_create_zone", ret_default_zone),
    ("_malloc_default_purgeable_zone", ret_default_zone),
    ("_malloc_set_zone_name", noop),
    ("_malloc_destroy_zone", noop),
    ("_malloc_zone_malloc", zone_malloc),
    ("_malloc_zone_calloc", zone_calloc),
    ("_malloc_zone_free", zone_free),
    ("_malloc_zone_realloc", zone_realloc),
    ("_malloc_zone_memalign", zone_memalign),
    ("_malloc_zone_from_ptr", |e| {
        let p = e.arg(0);
        let z = if p == 0 { 0 } else { default_zone(e)? };
        ret(e, z)
    }),
    ("_munmap", ret_zero),
    ("_madvise", ret_zero),
    ("_mprotect", ret_zero),
    ("_mach_vm_allocate", mach_vm_allocate),
    ("_mach_vm_deallocate", ret_zero),
    ("_vm_map", vm_map),
    ("_setjmp", setjmp),
    ("__setjmp", setjmp),
    ("_sigsetjmp", setjmp),
    ("_longjmp", longjmp),
    ("__longjmp", longjmp),
    ("_siglongjmp", longjmp),
    ("_mach_vm_map", vm_map),
    ("_vm_allocate", mach_vm_allocate),
    ("_vm_deallocate", ret_zero),
    ("_mach_vm_region", ret_one), // KERN_INVALID_ADDRESS
    ("_vm_purgable_control", ret_zero),
    ("_strlcpy", strlcpy),
    ("_strlcat", strlcat),
    ("___strlcpy_chk", strlcpy_chk),
    ("___strlcat_chk", strlcat_chk),
    ("___sprintf_chk", sprintf_chk),
    ("_strcat", strcat),
    ("_strnlen", strnlen),
    ("_strncasecmp", strncasecmp),
    ("_strncasecmp_l", strncasecmp),
    ("_strnstr", strnstr),
    ("_atoi", atoi),
    ("_atol", atoi),
    ("_flsl", flsl),
    ("_div", div),
    ("_scalbn", scalbn),
    ("_tanhf", tanhf),
    ("_bsearch", bsearch),
    ("_mach_port_destruct", ret_zero),
    ("_mach_port_deallocate", ret_zero),
    ("_mach_port_insert_right", ret_zero),
    ("_mach_port_mod_refs", ret_zero),
    ("_mach_port_type", mach_port_type),
    ("_mach_port_set_attributes", ret_zero),
    ("_mk_timer_destroy", ret_zero),
    ("_mach_error_string", mach_error_string),
    ("_voucher_mach_msg_adopt", ret_zero),
    ("_voucher_mach_msg_revert", noop),
    ("_dispatch_apply", dispatch_apply),
    ("_dispatch_retain", noop),
    ("_pthread_atfork", ret_zero),
    ("_pthread_is_threaded_np", ret_zero),
    ("_pthread_main_thread_np", |e| ret(e, 0x3_2000_0000)),
    ("_pthread_attr_setdetachstate", ret_zero),
    ("_pthread_attr_setscope", ret_zero),
    ("_pthread_attr_set_qos_class_np", ret_zero),
    ("_qos_class_main", |e| ret(e, 0x21)),
    ("_getsectbynamefromheader_64", getsectbynamefromheader),
    ("_getsegbyname", getsegbyname),
    ("_dyld_image_path_containing_address", dyld_image_path_containing_address),
    ("_dlopen_preflight", ret_zero),
    ("_NXFindBestFatArch", ret_zero),
    ("_sysdir_start_search_path_enumeration", ret_zero),
    ("_sysdir_get_next_search_path_enumeration", ret_zero),
    ("___darwin_check_fd_set_overflow", ret_one),
    ("_asl_new", ret_zero),
    ("_asl_open", ret_zero),
    ("_asl_set", ret_zero),
    ("_asl_send", ret_zero),
    ("_asl_free", noop),
    ("_asl_close", noop),
    ("__os_log_debug_impl", os_log),
    ("__os_log_error_impl", os_log),
    ("__os_log_fault_impl", os_log),
    ("__os_log_impl", os_log),
    ("_os_log_type_enabled", |e| {
        // Debug messages (type 2) print only when asked for (`--debug-logs`);
        // info, default, error and fault always do.
        e.ret((e.arg(1) != 2 || e.os_log_debug) as u64);
        Ok(())
    }),
    ("_writev", writev),
    ("_fsync", ret_zero),
    // Not modelled: fail cleanly.
    ("_ioctl", fail),
    ("_readlink", fail),
    ("_chown", fail),
];

/// `_os_log_impl(dso, log, type, format, buf, size)`: the arguments come packed
/// in os_log's buffer ({summary, count} then {descriptor, size, bytes} per
/// item). Unpack them into 8-byte slots and format like printf, minus the
/// `{public}`/`{private}` annotations.
fn os_log(emu: &mut Emu) -> Result<()> {
    let (kind, fmt, buf, size) = (emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    let fmt = emu.cstr(fmt)?;
    let clean = strip_privacy(&fmt);
    let bytes = emu.p.memory.read(buf, size.min(4096) as usize)?.to_vec();
    let args = os_log_args(&bytes);
    let slots = emu.alloc(args.len() as u64 * 8 + 8)?;
    for (k, v) in args.iter().enumerate() {
        emu.write_u64(slots + k as u64 * 8, *v)?;
    }
    let text = crate::printf::format(emu, clean.as_bytes(), &mut crate::printf::Args::new(slots));
    emu.heap.free(&emu.p.memory, slots)?;
    let text = text?;
    if kind == 0x02 && !emu.os_log_debug {
        return Ok(());
    }
    let level = match kind {
        0x01 => "info",
        0x02 => "debug",
        0x10 => "error",
        0x11 => "fault",
        _ => "log",
    };
    println!("[os_log {level}] {}", String::from_utf8_lossy(&text));
    Ok(())
}

/// `%{public}s` → `%s`: os_log's privacy annotations, which printf doesn't know.
fn strip_privacy(fmt: &str) -> String {
    let mut clean = String::with_capacity(fmt.len());
    let mut rest = fmt;
    while let Some(i) = rest.find("%{") {
        clean.push_str(&rest[..i + 1]);
        rest = rest[i + 2..].split_once('}').map_or("", |(_, r)| r);
    }
    clean.push_str(rest);
    clean
}

/// The arguments packed in an os_log buffer, each widened to an 8-byte slot
/// (signed, as printf reads them).
fn os_log_args(bytes: &[u8]) -> Vec<u64> {
    let count = bytes.get(1).copied().unwrap_or(0) as usize;
    let mut out = Vec::with_capacity(count);
    let mut at = 2;
    for _ in 0..count {
        let Some(&n) = bytes.get(at + 1) else { break };
        let data = bytes.get(at + 2..at + 2 + n as usize).unwrap_or(&[]);
        out.push(match data.len() {
            1 => data[0] as i8 as i64 as u64,
            2 => i16::from_le_bytes([data[0], data[1]]) as i64 as u64,
            4 => i32::from_le_bytes(data.try_into().unwrap()) as i64 as u64,
            8 => u64::from_le_bytes(data.try_into().unwrap()),
            _ => 0,
        });
        at += 2 + n as usize;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_log_buffers() {
        assert_eq!(strip_privacy("%{public}s took %{private}d ms (%d%%)"), "%s took %d ms (%d%%)");
        // {summary, count}, then {descriptor, size, bytes}: an int (-2) and a pointer.
        let mut buf = vec![0x02, 2, 0x00, 4];
        buf.extend_from_slice(&(-2i32).to_le_bytes());
        buf.extend_from_slice(&[0x22, 8]);
        buf.extend_from_slice(&0x1234_5678u64.to_le_bytes());
        assert_eq!(os_log_args(&buf), vec![(-2i64) as u64, 0x1234_5678]);
        assert!(os_log_args(&[0, 3, 0]).is_empty());
    }
}

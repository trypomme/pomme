//! The rest of libSystem: string and memory helpers, libm, time, pthread,
//! thread-local variables, dispatch, logging, and libSystem's data symbols.
//!
//! Everything is single-threaded and deterministic:
//! - time comes from the instruction counter (24 MHz ticks, like CNTVCT);
//! - locks always succeed; there is one thread;
//! - `dispatch_async` work is queued and drained by the run loop, in order.

use crate::{Emu, HostFn, Result, Stop};
use std::collections::{HashMap, VecDeque};

/// The default wall-clock time at boot: 2026-01-01T00:00:00Z. Guest time = boot + steps / 24 MHz.
pub const EPOCH_BASE_SECS: u64 = 1_767_225_600;

/// The wall-clock time the phone booted at, in seconds since 1970.
pub fn boot_time(emu: &Emu) -> u64 {
    emu.sys.boot_time.unwrap_or(EPOCH_BASE_SECS)
}

/// Set the wall-clock time at boot (before the app runs): `--clock`.
pub fn set_boot_time(emu: &mut Emu, secs: u64) {
    emu.sys.boot_time = Some(secs);
}
pub const TICKS_PER_SEC: u64 = pomme_cpu::TICKS_PER_SEC;

#[derive(Default)]
pub struct State {
    errno: u64,
    /// dispatch_data objects → (bytes in guest memory, length).
    dispatch_data: HashMap<u64, (u64, u64)>,
    /// strtok's saved position (a guest pointer to a char *).
    strtok_save: u64,
    /// sigaction() records: signal → the 16-byte struct sigaction.
    signal_actions: HashMap<i32, Vec<u8>>,
    /// dlerror's pending message (a guest C string), or 0.
    dlerror: u64,
    /// Images' paths as C strings, for dladdr and _dyld_get_image_name.
    image_paths: HashMap<usize, u64>,
    /// The wall-clock time at boot, if not the default (see [`boot_time`]).
    boot_time: Option<u64>,
    /// `dlopen` handles for system libraries that aren't guest images (pomme
    /// implements their functions; on a phone they're in the shared cache): path → handle.
    system_libraries: HashMap<String, u64>,
    /// The environment (made on first use: see `default_env`).
    env: Option<Vec<(String, String)>>,
    /// Guest copies of values handed out by getenv.
    env_strings: HashMap<(String, String), u64>,
    /// `_NSGetEnviron()`'s cell (a `char **` to the environ array), or 0.
    environ_cell: u64,
    /// The default malloc zone (a `malloc_zone_t`), once made.
    pub(crate) malloc_zone: u64,
    /// `uselocale`'s current locale handle (0: the global one).
    thread_locale: u64,
    /// Per-image thread-local storage blocks.
    tlv_blocks: HashMap<(usize, usize), u64>,
    /// Queued async work: (function, context). Blocks are queued as (invoke, block).
    /// Work for serial queues (the main one included), run in order by the
    /// main thread: (function, context, the queue it was submitted to).
    pub queue: VecDeque<(u64, u64, u64)>,
    /// The queues each thread is running work for, innermost last
    /// (dispatch_sync nests): what "the current queue" means to
    /// dispatch_get_specific and DISPATCH_CURRENT_QUEUE_LABEL.
    current_queues: HashMap<usize, Vec<u64>>,
    queue_specific: HashMap<(u64, u64), u64>,
    /// The global (concurrent) queue: its work runs on threads of its own.
    global_queue: u64,
    /// Dispatch groups: outstanding work and the notify blocks waiting on it.
    groups: HashMap<u64, Group>,
    /// Queues the app made (serial or concurrent), by address.
    dqueues: HashMap<u64, DQueue>,
    /// Threads running a queue's work → (the queue, whether it's a barrier).
    queue_threads: HashMap<usize, (u64, bool)>,
    /// Threads running a group's work → the group.
    group_threads: HashMap<usize, u64>,
    /// Work waiting for its time (`dispatch_after`): (queue, function, context).
    after: Vec<Option<(u64, u64, u64)>>,
    /// libunwind asked about a pomme frame (an exception tried to unwind
    /// through host code that called back into the app).
    pub unwound_into_host: bool,
    /// Blocks cancelled with `dispatch_block_cancel`: they don't run.
    cancelled: std::collections::HashSet<u64>,
    /// `dispatch_block_notify` work waiting for a block to finish: block → (queue, invoke, notification block).
    block_notify: HashMap<u64, Vec<(u64, u64, u64)>>,
    data: HashMap<&'static str, u64>,
    pub next_port: u64,
    /// Seeded random state (arc4random, UUIDs): the same every run.
    rng: u64,
}

/// A dispatch queue the app made: work waits in order and runs on threads,
/// `width` at a time (1: serial); a barrier runs alone.
#[derive(Default)]
struct DQueue {
    width: usize,
    /// Its work runs on the main queue (its target is the main queue).
    on_main: bool,
    pending: VecDeque<Work>,
    running: usize,
    barrier_running: bool,
}

/// One piece of a queue's work.
#[derive(Clone, Copy)]
struct Work {
    f: u64,
    ctx: u64,
    group: Option<u64>,
    barrier: bool,
    /// A dispatch_sync caller waiting its turn: this flag becomes 1 when it may run (0: async work).
    turn: u64,
}

#[derive(Default)]
struct Group {
    count: u64,
    /// (queue, function, context) to submit when the count reaches zero.
    notify: Vec<(u64, u64, u64)>,
}

impl State {
    pub fn rng_state(&self) -> u64 {
        if self.rng == 0 { 0x9e37_79b9_7f4a_7c15 } else { self.rng }
    }

    pub fn set_rng_state(&mut self, v: u64) {
        self.rng = v;
    }
}


// ---- data symbols ----

const DATA: &[(&str, u64)] = &[
    ("___stack_chk_guard", 8),
    ("___stderrp", 8),
    ("___stdoutp", 8),
    ("___stdinp", 8),
    ("_mach_task_self_", 8),
    ("___progname", 8),
    ("_sys_nerr", 8),
    ("__DefaultRuneLocale", 0x1000),
    ("__dispatch_main_q", 128),
    ("__dispatch_data_empty", 64),
    ("__dispatch_data_destructor_free", 8),
    ("__dispatch_data_destructor_munmap", 8),
    ("__dispatch_queue_attr_concurrent", 64),
    ("__dispatch_source_type_timer", 64),
    ("__dispatch_source_type_data_add", 64),
    ("__dispatch_source_type_data_or", 64),
    ("__dispatch_source_type_data_replace", 64),
    ("__dispatch_source_type_read", 64),
    ("__dispatch_source_type_write", 64),
    ("__dispatch_source_type_signal", 64),
    ("__dispatch_source_type_proc", 64),
    ("__dispatch_source_type_vnode", 64),
    ("__dispatch_source_type_mach_send", 64),
    ("__dispatch_source_type_mach_recv", 64),
    ("__dispatch_source_type_memorypressure", 64),
    ("_vm_page_size", 8),
    ("_vm_kernel_page_size", 8),
    ("__os_log_default", 64),
    ("_timezone", 8),
    ("_tzname", 16),
    ("_daylight", 8),
];

pub(crate) fn init(emu: &mut Emu) -> Result<()> {
    let queue_cls = crate::objc::host_class(emu, "OS_dispatch_queue")?;
    for &(name, size) in DATA {
        let stubs = emu.p.stubs.find(name);
        if stubs.is_empty() {
            continue;
        }
        let addr = emu.alloc(size)?;
        match name {
            "___stack_chk_guard" => emu.write_u64(addr, 0x5ca1_ab1e_0dd5_eed5)?,
            "_mach_task_self_" => emu.p.memory.write_u32(addr, 0x103)?,
            "___progname" => {
                let s = emu.alloc_cstr(&emu.p.main().name.clone())?;
                emu.write_u64(addr, s)?;
            }
            "__dispatch_main_q" => emu.write_u64(addr, queue_cls)?,
            "__DefaultRuneLocale" => fill_rune_locale(emu, addr)?,
            "_vm_page_size" | "_vm_kernel_page_size" => emu.write_u64(addr, pomme_mem::PAGE_SIZE)?,
            "_tzname" => {
                let utc = emu.alloc_cstr("UTC")?;
                emu.write_u64(addr, utc)?;
                emu.write_u64(addr + 8, utc)?;
            }
            "___stderrp" | "___stdoutp" | "___stdinp" => {
                // A FILE: we only need a distinct non-null pointer; store the fd.
                let file = emu.alloc(256)?;
                let fd = match name {
                    "___stdinp" => 0,
                    "___stdoutp" => 1,
                    _ => 2,
                };
                emu.p.memory.write_u32(file + 18, fd)?;
                emu.write_u64(addr, file)?;
            }
            _ => {}
        }
        for stub in stubs {
            emu.p.rebind(stub, addr).map_err(|e| Stop::Unsupported(e.to_string()))?;
        }
        emu.sys.data.insert(name, addr);
    }
    Ok(())
}

/// The address of a libSystem data symbol pomme made (if the app imports it).
pub(crate) fn data_addr(emu: &Emu, name: &str) -> Option<u64> {
    emu.sys.data.get(name).copied()
}

/// The name of the libSystem data symbol at `addr`, if it's one.
pub(crate) fn data_name(emu: &Emu, addr: u64) -> Option<&'static str> {
    emu.sys.data.iter().find(|&(_, &a)| a == addr).map(|(&n, _)| n)
}

/// `dispatch_resume`, `dispatch_suspend`, `dispatch_activate` (sources; queues
/// aren't suspended here) and `dispatch_set_context`/`dispatch_get_context`.
fn dispatch_resume(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    if crate::dsource::is_source(emu, o) { crate::dsource::resume(emu, o) } else { Ok(()) }
}

fn dispatch_suspend(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    if crate::dsource::is_source(emu, o) { crate::dsource::suspend(emu, o) } else { Ok(()) }
}

fn dispatch_activate(emu: &mut Emu) -> Result<()> {
    let o = emu.arg(0);
    if crate::dsource::is_source(emu, o) { crate::dsource::activate(emu, o) } else { Ok(()) }
}

fn dispatch_set_context(emu: &mut Emu) -> Result<()> {
    crate::dsource::set_context(emu, emu.arg(0), emu.arg(1));
    Ok(())
}

fn dispatch_get_context(emu: &mut Emu) -> Result<()> {
    let c = crate::dsource::context(emu, emu.arg(0));
    emu.ret(c);
    Ok(())
}

// ---- helpers ----

fn f64_arg(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn f32_arg(emu: &Emu, i: usize) -> f32 {
    f32::from_bits(emu.cpu.q[i] as u32)
}

fn ret_f64(emu: &mut Emu, v: f64) {
    emu.cpu.q[0] = v.to_bits() as u128;
}

fn ret_f32(emu: &mut Emu, v: f32) {
    emu.cpu.q[0] = v.to_bits() as u128;
}

fn bytes(emu: &Emu, addr: u64, n: u64) -> Result<Vec<u8>> {
    Ok(emu.p.memory.read(addr, n as usize)?.to_vec())
}

/// Read bytes up to (not including) the NUL.
fn cbytes(emu: &Emu, addr: u64) -> Result<Vec<u8>> {
    emu.cstr_bytes(addr)
}

/// Call a block with arguments after the block pointer itself.
/// A global block whose invoke is the host function `f` (it gets the block in x0, then the arguments).
pub fn host_block(emu: &mut Emu, f: HostFn) -> Result<u64> {
    let invoke = emu.host_thunk(f)?;
    // Block_layout {isa, flags, reserved, invoke, descriptor}; descriptor {reserved, size}.
    let b = emu.alloc(48)?;
    let isa = crate::objc::host_class(emu, "__NSGlobalBlock__")?;
    emu.write_u64(b, isa)?;
    emu.p.memory.write_u32(b + 8, 1 << 28)?; // BLOCK_IS_GLOBAL
    emu.write_u64(b + 16, invoke)?;
    emu.write_u64(b + 24, b + 32)?;
    emu.write_u64(b + 40, 32)?;
    Ok(b)
}

// ---- dispatch_data: immutable byte buffers, held contiguously in guest memory ----

fn data_bytes_of(emu: &Emu, d: u64) -> (u64, u64) {
    if d == emu.sys.data.get("__dispatch_data_empty").copied().unwrap_or(0) {
        return (0, 0);
    }
    emu.sys.dispatch_data.get(&d).copied().unwrap_or((0, 0))
}

/// A new data object holding a copy of `bytes`.
fn new_dispatch_data(emu: &mut Emu, bytes: &[u8]) -> Result<u64> {
    let cls = crate::objc::host_class(emu, "OS_dispatch_data")?;
    let d = crate::objc::create_instance(emu, cls, 0)?;
    let p = emu.alloc(bytes.len().max(1) as u64)?;
    emu.p.memory.write(p, bytes)?;
    emu.sys.dispatch_data.insert(d, (p, bytes.len() as u64));
    Ok(d)
}

/// A dispatch_data object's bytes.
pub(crate) fn dispatch_data_bytes(emu: &Emu, d: u64) -> Result<Vec<u8>> {
    read_data(emu, d)
}

fn read_data(emu: &Emu, d: u64) -> Result<Vec<u8>> {
    let (p, n) = data_bytes_of(emu, d);
    Ok(if n == 0 { Vec::new() } else { emu.p.memory.read(p, n as usize)?.to_vec() })
}

/// `dispatch_data_create(buffer, size, queue, destructor)`: copies the bytes; a
/// free/munmap destructor or a block releases the caller's buffer right away.
fn dispatch_data_create(emu: &mut Emu) -> Result<()> {
    let (buf, n, destructor) = (emu.arg(0), emu.arg(1), emu.arg(3));
    let bytes = if n == 0 { Vec::new() } else { emu.p.memory.read(buf, n as usize)?.to_vec() };
    let d = new_dispatch_data(emu, &bytes)?;
    let free_sym = emu.sys.data.get("__dispatch_data_destructor_free").copied().unwrap_or(0);
    let munmap_sym = emu.sys.data.get("__dispatch_data_destructor_munmap").copied().unwrap_or(0);
    if destructor == free_sym && destructor != 0 {
        emu.heap.free(&emu.p.memory, buf)?;
    } else if destructor != 0 && destructor != munmap_sym {
        call_block(emu, destructor, &[])?;
    }
    emu.ret(d);
    Ok(())
}

/// `dispatch_data_create_map(data, const void **buffer, size_t *size)`.
fn dispatch_data_create_map(emu: &mut Emu) -> Result<()> {
    let (d, bp, sp) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let (p, n) = data_bytes_of(emu, d);
    if bp != 0 {
        emu.write_u64(bp, p)?;
    }
    if sp != 0 {
        emu.write_u64(sp, n)?;
    }
    crate::objc::send(emu, d, "retain", &[])?;
    emu.ret(d);
    Ok(())
}

fn dispatch_data_create_subrange(emu: &mut Emu) -> Result<()> {
    let (d, off, len) = (emu.arg(0), emu.arg(1) as usize, emu.arg(2) as usize);
    let all = read_data(emu, d)?;
    let start = off.min(all.len());
    let end = (start + len).min(all.len());
    let s = new_dispatch_data(emu, &all[start..end])?;
    emu.ret(s);
    Ok(())
}

fn dispatch_data_create_concat(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0), emu.arg(1));
    let mut v = read_data(emu, a)?;
    v.extend(read_data(emu, b)?);
    let d = new_dispatch_data(emu, &v)?;
    emu.ret(d);
    Ok(())
}

/// `dispatch_data_apply(data, applier(region, offset, buffer, size) -> bool)`: one region.
fn dispatch_data_apply(emu: &mut Emu) -> Result<()> {
    let (d, block) = (emu.arg(0), emu.arg(1));
    let (p, n) = data_bytes_of(emu, d);
    let r = if n == 0 { 1 } else { call_block(emu, block, &[d, 0, p, n])? & 0xff };
    emu.ret((r != 0) as u64);
    Ok(())
}

/// `dispatch_data_copy_region(data, location, size_t *offset)`: the whole buffer is one region.
fn dispatch_data_copy_region(emu: &mut Emu) -> Result<()> {
    let (d, off) = (emu.arg(0), emu.arg(2));
    if off != 0 {
        emu.write_u64(off, 0)?;
    }
    crate::objc::send(emu, d, "retain", &[])?;
    emu.ret(d);
    Ok(())
}

pub fn call_block(emu: &mut Emu, block: u64, args: &[u64]) -> Result<u64> {
    let invoke = emu.read_u64(block + 16)?;
    let mut all = vec![block];
    all.extend_from_slice(args);
    emu.call(invoke, &all)
}

fn ret_zero(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

fn ret_self(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn noop(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn ret_one(emu: &mut Emu) -> Result<()> {
    emu.ret(1);
    Ok(())
}

// ---- memory and strings ----

fn memcmp(emu: &mut Emu) -> Result<()> {
    if emu.arg(2) == 0 {
        emu.ret(0); // zero-length compares may pass NULL
        return Ok(());
    }
    let a = bytes(emu, emu.arg(0), emu.arg(2))?;
    let b = bytes(emu, emu.arg(1), emu.arg(2))?;
    let r = a.iter().zip(&b).find(|(x, y)| x != y).map_or(0, |(x, y)| *x as i64 - *y as i64);
    emu.ret(r as u64);
    Ok(())
}

fn memchr(emu: &mut Emu) -> Result<()> {
    let (s, c, n) = (emu.arg(0), emu.arg(1) as u8, emu.arg(2));
    if n == 0 {
        emu.ret(0);
        return Ok(());
    }
    let b = bytes(emu, s, n)?;
    emu.ret(b.iter().position(|&x| x == c).map_or(0, |i| s + i as u64));
    Ok(())
}

fn memset_pattern(emu: &mut Emu, width: u64) -> Result<()> {
    let (dst, pat, n) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let p = bytes(emu, pat, width)?;
    let fill: Vec<u8> = (0..n).map(|i| p[(i % width) as usize]).collect();
    emu.p.memory.write(dst, &fill)?;
    Ok(())
}

fn strchr(emu: &mut Emu) -> Result<()> {
    let (s, c) = (emu.arg(0), emu.arg(1) as u8);
    let b = cbytes(emu, s)?;
    let r = if c == 0 { Some(b.len()) } else { b.iter().position(|&x| x == c) };
    emu.ret(r.map_or(0, |i| s + i as u64));
    Ok(())
}

fn strrchr(emu: &mut Emu) -> Result<()> {
    let (s, c) = (emu.arg(0), emu.arg(1) as u8);
    let b = cbytes(emu, s)?;
    let r = if c == 0 { Some(b.len()) } else { b.iter().rposition(|&x| x == c) };
    emu.ret(r.map_or(0, |i| s + i as u64));
    Ok(())
}

fn strncmp(emu: &mut Emu) -> Result<()> {
    let n = emu.arg(2) as usize;
    let mut a = cbytes(emu, emu.arg(0))?;
    let mut b = cbytes(emu, emu.arg(1))?;
    a.truncate(n);
    b.truncate(n);
    emu.ret(a.cmp(&b) as i8 as i64 as u64);
    Ok(())
}

fn strcasecmp(emu: &mut Emu) -> Result<()> {
    let a = cbytes(emu, emu.arg(0))?.to_ascii_lowercase();
    let b = cbytes(emu, emu.arg(1))?.to_ascii_lowercase();
    emu.ret(a.cmp(&b) as i8 as i64 as u64);
    Ok(())
}

fn strstr(emu: &mut Emu) -> Result<()> {
    let (h, n) = (emu.arg(0), emu.arg(1));
    let hay = cbytes(emu, h)?;
    let needle = cbytes(emu, n)?;
    let r = if needle.is_empty() { Some(0) } else { hay.windows(needle.len()).position(|w| w == needle) };
    emu.ret(r.map_or(0, |i| h + i as u64));
    Ok(())
}

fn strcpy(emu: &mut Emu) -> Result<()> {
    let mut s = cbytes(emu, emu.arg(1))?;
    s.push(0);
    emu.p.memory.write(emu.arg(0), &s)?;
    Ok(())
}

fn strncpy(emu: &mut Emu) -> Result<()> {
    let n = emu.arg(2) as usize;
    let mut s = cbytes(emu, emu.arg(1))?;
    s.resize(n, 0);
    emu.p.memory.write(emu.arg(0), &s[..n])?;
    Ok(())
}

fn strncat(emu: &mut Emu) -> Result<()> {
    let dst = emu.arg(0);
    let len = cbytes(emu, dst)?.len() as u64;
    let mut s = cbytes(emu, emu.arg(1))?;
    s.truncate(emu.arg(2) as usize);
    s.push(0);
    emu.p.memory.write(dst + len, &s)?;
    Ok(())
}

fn strdup(emu: &mut Emu) -> Result<()> {
    let s = cbytes(emu, emu.arg(0))?;
    let p = emu.alloc(s.len() as u64 + 1)?;
    emu.p.memory.write(p, &s)?;
    emu.ret(p);
    Ok(())
}

/// strtol family: parse from a C string, set *endptr.
fn strto_int(emu: &mut Emu) -> Result<()> {
    let (s, end, base) = (emu.arg(0), emu.arg(1), emu.arg(2) as u32);
    let b = cbytes(emu, s)?;
    let mut i = 0;
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    let mut neg = false;
    if i < b.len() && (b[i] == b'-' || b[i] == b'+') {
        neg = b[i] == b'-';
        i += 1;
    }
    let mut base = base;
    if (base == 0 || base == 16) && b.get(i) == Some(&b'0') && matches!(b.get(i + 1), Some(b'x' | b'X')) {
        i += 2;
        base = 16;
    } else if base == 0 && b.get(i) == Some(&b'0') {
        base = 8;
    } else if base == 0 {
        base = 10;
    }
    let start = i;
    let mut v: u64 = 0;
    while i < b.len() {
        let Some(d) = (b[i] as char).to_digit(base) else { break };
        v = v.wrapping_mul(base as u64).wrapping_add(d as u64);
        i += 1;
    }
    if i == start {
        i = 0;
    }
    if end != 0 {
        emu.write_u64(end, s + i as u64)?;
    }
    let r = if neg { (v as i64).wrapping_neg() as u64 } else { v };
    emu.ret(r);
    Ok(())
}

fn strtod(emu: &mut Emu) -> Result<()> {
    let (s, end) = (emu.arg(0), emu.arg(1));
    let b = cbytes(emu, s)?;
    let text = String::from_utf8_lossy(&b);
    let trimmed = text.trim_start();
    let skip = text.len() - trimmed.len();
    // Longest prefix that parses as a float.
    let mut best = (0, 0.0);
    for n in 1..=trimmed.len() {
        if let Ok(v) = trimmed[..n].parse::<f64>() {
            best = (n, v);
        }
    }
    if end != 0 {
        emu.write_u64(end, s + if best.0 == 0 { 0 } else { (skip + best.0) as u64 })?;
    }
    ret_f64(emu, best.1);
    Ok(())
}

fn strerror(emu: &mut Emu) -> Result<()> {
    let s = emu.alloc_cstr(&format!("error {}", emu.arg(0) as i32))?;
    emu.ret(s);
    Ok(())
}

fn tolower(emu: &mut Emu) -> Result<()> {
    emu.ret((emu.arg(0) as u8).to_ascii_lowercase() as u64);
    Ok(())
}

fn toupper(emu: &mut Emu) -> Result<()> {
    emu.ret((emu.arg(0) as u8).to_ascii_uppercase() as u64);
    Ok(())
}

/// `__maskrune(c, mask)`: ctype classification against the C locale.
fn maskrune(emu: &mut Emu) -> Result<()> {
    let (c, mask) = (emu.arg(0) as u32, emu.arg(1));
    emu.ret(ctype_bits(c) & mask);
    Ok(())
}

/// The C locale's `_CTYPE_*` bits for a character.
pub(crate) fn ctype_bits(c: u32) -> u64 {
    let ch = char::from_u32(c).unwrap_or('\0');
    let mut bits = 0u64;
    let set = |cond: bool, bit: u64, bits: &mut u64| {
        if cond {
            *bits |= bit
        }
    };
    set(ch.is_ascii_alphabetic(), 0x100, &mut bits); // _CTYPE_A
    set(ch.is_ascii_control(), 0x200, &mut bits); // _CTYPE_C
    set(ch.is_ascii_digit(), 0x400, &mut bits); // _CTYPE_D
    set(ch.is_ascii_graphic(), 0x800, &mut bits); // _CTYPE_G
    set(ch.is_ascii_lowercase(), 0x1000, &mut bits); // _CTYPE_L
    set(ch.is_ascii_punctuation(), 0x2000, &mut bits); // _CTYPE_P
    set(ch.is_ascii_whitespace(), 0x4000, &mut bits); // _CTYPE_S
    set(ch.is_ascii_uppercase(), 0x8000, &mut bits); // _CTYPE_U
    set(ch.is_ascii_hexdigit(), 0x10000, &mut bits); // _CTYPE_X
    set(ch == ' ' || ch == '\t', 0x20000, &mut bits); // _CTYPE_B
    set(ch.is_ascii_graphic() || ch == ' ', 0x40000, &mut bits); // _CTYPE_R
    bits
}

/// `_DefaultRuneLocale` (the C locale), whose tables libc++'s `ctype<char>`
/// reads directly: `__runetype` at 60, `__maplower` at 1084, `__mapupper` at 2108.
fn fill_rune_locale(emu: &mut Emu, addr: u64) -> Result<()> {
    for (i, b) in b"RuneMagA".iter().enumerate() {
        emu.p.memory.write_u8(addr + i as u64, *b)?;
    }
    for (i, b) in b"NONE".iter().enumerate() {
        emu.p.memory.write_u8(addr + 8 + i as u64, *b)?;
    }
    emu.p.memory.write_u32(addr + 56, 0xfffd)?; // __invalid_rune
    for c in 0..256u32 {
        let o = c as u64 * 4;
        let bits = if c < 128 { ctype_bits(c) } else { 0 };
        emu.p.memory.write_u32(addr + 60 + o, bits as u32)?;
        let b = c as u8;
        let lower = if c < 128 { b.to_ascii_lowercase() as u32 } else { c };
        let upper = if c < 128 { b.to_ascii_uppercase() as u32 } else { c };
        emu.p.memory.write_u32(addr + 1084 + o, lower)?;
        emu.p.memory.write_u32(addr + 2108 + o, upper)?;
    }
    Ok(())
}

// ---- math ----

macro_rules! libm {
    ($($name:ident: $f:expr;)*) => {
        $(fn $name(emu: &mut Emu) -> Result<()> {
            let f: fn(f64) -> f64 = $f;
            let v = f(f64_arg(emu, 0));
            ret_f64(emu, v);
            Ok(())
        })*
    };
}
macro_rules! libm2 {
    ($($name:ident: $f:expr;)*) => {
        $(fn $name(emu: &mut Emu) -> Result<()> {
            let f: fn(f64, f64) -> f64 = $f;
            let v = f(f64_arg(emu, 0), f64_arg(emu, 1));
            ret_f64(emu, v);
            Ok(())
        })*
    };
}
macro_rules! libmf {
    ($($name:ident: $f:expr;)*) => {
        $(fn $name(emu: &mut Emu) -> Result<()> {
            let f: fn(f32) -> f32 = $f;
            let v = f(f32_arg(emu, 0));
            ret_f32(emu, v);
            Ok(())
        })*
    };
}

libm! {
    m_sin: f64::sin; m_cos: f64::cos; m_tan: f64::tan; m_asin: f64::asin; m_acos: f64::acos; m_atan: f64::atan;
    m_sinh: f64::sinh; m_cosh: f64::cosh; m_tanh: f64::tanh; m_asinh: f64::asinh; m_acosh: f64::acosh; m_atanh: f64::atanh;
    m_exp: f64::exp; m_exp2: f64::exp2; m_expm1: f64::exp_m1; m_exp10: |x| 10f64.powf(x);
    m_log: f64::ln; m_log2: f64::log2; m_log10: f64::log10; m_log1p: f64::ln_1p;
    m_sqrt: f64::sqrt; m_cbrt: f64::cbrt; m_floor: f64::floor; m_ceil: f64::ceil; m_trunc: f64::trunc;
}
libm2! {
    m_pow: f64::powf; m_atan2: f64::atan2; m_hypot: f64::hypot; m_fmod: |a, b| a % b;
    m_remainder: |a, b| a - (a / b).round_ties_even() * b;
    m_nextafter: |a, b| if a == b { b } else if a.is_nan() || b.is_nan() { f64::NAN } else if b > a { a.next_up() } else { a.next_down() };
}
libmf! {
    m_sinf: f32::sin; m_cosf: f32::cos; m_tanf: f32::tan; m_expf: f32::exp; m_exp2f: f32::exp2;
    m_logf: f32::ln; m_log2f: f32::log2; m_log10f: f32::log10;
}

libm! {
    m_round: f64::round; m_logb: |x| if x == 0.0 { f64::NEG_INFINITY } else { x.abs().log2().floor() };
}

/// Two-argument float functions.
fn libmf2(emu: &mut Emu, f: fn(f32, f32) -> f32) -> Result<()> {
    let v = f(f32_arg(emu, 0), f32_arg(emu, 1));
    ret_f32(emu, v);
    Ok(())
}

/// `modff(x, float *ip)` / `modf(x, double *ip)`: the fraction, the integer part through ip.
fn m_modff(emu: &mut Emu) -> Result<()> {
    let x = f32_arg(emu, 0);
    let ip = emu.arg(0);
    if ip != 0 {
        emu.p.memory.write_u32(ip, x.trunc().to_bits())?;
    }
    ret_f32(emu, x - x.trunc());
    Ok(())
}

// ---- strings: tokens and spans ----

/// `strspn(s, accept)` / `strcspn(s, reject)`.
fn span(emu: &mut Emu, accept: bool) -> Result<()> {
    let (s, set) = (cbytes(emu, emu.arg(0))?, cbytes(emu, emu.arg(1))?);
    let n = s.iter().take_while(|c| set.contains(c) == accept).count();
    emu.ret(n as u64);
    Ok(())
}

/// `strtok_r(str, sep, char **save)`: cut the next token in place.
fn strtok_at(emu: &mut Emu, s: u64, sep: u64, save: u64) -> Result<u64> {
    let mut p = if s != 0 { s } else { emu.read_u64(save)? };
    if p == 0 {
        return Ok(0);
    }
    let seps = cbytes(emu, sep)?;
    // Skip leading separators.
    loop {
        let c = emu.p.memory.read_u8(p)?;
        if c == 0 {
            emu.write_u64(save, 0)?;
            return Ok(0);
        }
        if !seps.contains(&c) {
            break;
        }
        p += 1;
    }
    let start = p;
    loop {
        let c = emu.p.memory.read_u8(p)?;
        if c == 0 {
            emu.write_u64(save, 0)?;
            break;
        }
        if seps.contains(&c) {
            emu.p.memory.write_u8(p, 0)?;
            emu.write_u64(save, p + 1)?;
            break;
        }
        p += 1;
    }
    Ok(start)
}

fn strtok_r(emu: &mut Emu) -> Result<()> {
    let (s, sep, save) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let t = strtok_at(emu, s, sep, save)?;
    emu.ret(t);
    Ok(())
}

/// `strtok(str, sep)`: strtok_r with a hidden save pointer.
fn strtok(emu: &mut Emu) -> Result<()> {
    if emu.sys.strtok_save == 0 {
        emu.sys.strtok_save = emu.alloc(8)?;
    }
    let (s, sep, save) = (emu.arg(0), emu.arg(1), emu.sys.strtok_save);
    let t = strtok_at(emu, s, sep, save)?;
    emu.ret(t);
    Ok(())
}

/// `strerror_r(errnum, buf, len)`.
fn strerror_r(emu: &mut Emu) -> Result<()> {
    let (n, buf, len) = (emu.arg(0) as i32, emu.arg(1), emu.arg(2) as usize);
    let msg = format!("{}", std::io::Error::from_raw_os_error(n));
    let msg = msg.split(" (os error").next().unwrap_or("Unknown error").to_string();
    let mut bytes = msg.into_bytes();
    bytes.truncate(len.saturating_sub(1));
    bytes.push(0);
    emu.p.memory.write(buf, &bytes)?;
    emu.ret(0);
    Ok(())
}

/// `getrusage(who, struct rusage *)`: the user time the guest has run (virtual), nothing else.
fn getrusage(emu: &mut Emu) -> Result<()> {
    let out = emu.arg(1);
    emu.p.memory.write(out, &[0u8; 144])?;
    let ns = crate::threads::now_ns(emu);
    emu.write_u64(out, ns / 1_000_000_000)?;
    emu.p.memory.write_u32(out + 8, ((ns % 1_000_000_000) / 1000) as u32)?;
    emu.ret(0);
    Ok(())
}

fn m_fmodf(emu: &mut Emu) -> Result<()> {
    let v = f32_arg(emu, 0) % f32_arg(emu, 1);
    ret_f32(emu, v);
    Ok(())
}

fn m_remainderf(emu: &mut Emu) -> Result<()> {
    let (a, b) = (f32_arg(emu, 0), f32_arg(emu, 1));
    ret_f32(emu, a - (a / b).round_ties_even() * b);
    Ok(())
}

fn m_sincos(emu: &mut Emu) -> Result<()> {
    let x = f64_arg(emu, 0);
    emu.cpu.q[0] = x.sin().to_bits() as u128;
    emu.cpu.q[1] = x.cos().to_bits() as u128;
    Ok(())
}

fn m_sincosf(emu: &mut Emu) -> Result<()> {
    let x = f32_arg(emu, 0);
    emu.cpu.q[0] = x.sin().to_bits() as u128;
    emu.cpu.q[1] = x.cos().to_bits() as u128;
    Ok(())
}

fn m_nan(emu: &mut Emu) -> Result<()> {
    ret_f64(emu, f64::NAN);
    Ok(())
}

fn m_modf(emu: &mut Emu) -> Result<()> {
    let x = f64_arg(emu, 0);
    emu.write_u64(emu.arg(0), x.trunc().to_bits())?;
    ret_f64(emu, x.fract());
    Ok(())
}

fn m_frexp(emu: &mut Emu) -> Result<()> {
    let x = f64_arg(emu, 0);
    let (m, e) = if x == 0.0 || !x.is_finite() {
        (x, 0)
    } else {
        let e = x.abs().log2().floor() as i32 + 1;
        let m = x / 2f64.powi(e);
        // Correct rounding at the boundary.
        if m.abs() >= 1.0 { (m / 2.0, e + 1) } else if m.abs() < 0.5 { (m * 2.0, e - 1) } else { (m, e) }
    };
    emu.p.memory.write_u32(emu.arg(0), e as u32)?;
    ret_f64(emu, m);
    Ok(())
}

/// 128-bit integer division helpers (x0:x1 = a, x2:x3 = b).
fn i128_args(emu: &Emu) -> (u128, u128) {
    let a = (emu.arg(1) as u128) << 64 | emu.arg(0) as u128;
    let b = (emu.arg(3) as u128) << 64 | emu.arg(2) as u128;
    (a, b)
}

fn ret_u128(emu: &mut Emu, v: u128) {
    emu.cpu.x[0] = v as u64;
    emu.cpu.x[1] = (v >> 64) as u64;
}

fn udivti3(emu: &mut Emu) -> Result<()> {
    let (a, b) = i128_args(emu);
    ret_u128(emu, a.checked_div(b).unwrap_or(0));
    Ok(())
}

fn umodti3(emu: &mut Emu) -> Result<()> {
    let (a, b) = i128_args(emu);
    ret_u128(emu, a.checked_rem(b).unwrap_or(0));
    Ok(())
}

fn divti3(emu: &mut Emu) -> Result<()> {
    let (a, b) = i128_args(emu);
    ret_u128(emu, (a as i128).checked_div(b as i128).unwrap_or(0) as u128);
    Ok(())
}

fn modti3(emu: &mut Emu) -> Result<()> {
    let (a, b) = i128_args(emu);
    ret_u128(emu, (a as i128).checked_rem(b as i128).unwrap_or(0) as u128);
    Ok(())
}

fn lldiv(emu: &mut Emu) -> Result<()> {
    let (a, b) = (emu.arg(0) as i64, emu.arg(1) as i64);
    emu.cpu.x[0] = a.checked_div(b).unwrap_or(0) as u64;
    emu.cpu.x[1] = a.checked_rem(b).unwrap_or(0) as u64;
    Ok(())
}

// ---- time ----

fn now_ns(emu: &Emu) -> u64 {
    crate::threads::now_ns(emu)
}

fn mach_absolute_time(emu: &mut Emu) -> Result<()> {
    emu.ret(crate::threads::now_ticks(emu));
    Ok(())
}

/// `mach_timebase_info(info*)`: ticks are 1/24 µs, so numer/denom = 125/3.
fn mach_timebase_info(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    emu.p.memory.write_u32(p, 125)?;
    emu.p.memory.write_u32(p + 4, 3)?;
    emu.ret(0);
    Ok(())
}

fn realtime_ns(emu: &Emu) -> u64 {
    boot_time(emu) * 1_000_000_000 + now_ns(emu)
}

/// CLOCK_REALTIME = 0, everything else counts from boot.
fn clock_ns(emu: &Emu, clock: u64) -> u64 {
    if clock == 0 { realtime_ns(emu) } else { now_ns(emu) }
}

fn clock_gettime(emu: &mut Emu) -> Result<()> {
    let (clock, ts) = (emu.arg(0), emu.arg(1));
    let ns = clock_ns(emu, clock);
    emu.write_u64(ts, ns / 1_000_000_000)?;
    emu.write_u64(ts + 8, ns % 1_000_000_000)?;
    emu.ret(0);
    Ok(())
}

fn clock_gettime_nsec_np(emu: &mut Emu) -> Result<()> {
    let ns = clock_ns(emu, emu.arg(0));
    emu.ret(ns);
    Ok(())
}

fn clock_getres(emu: &mut Emu) -> Result<()> {
    let ts = emu.arg(1);
    if ts != 0 {
        emu.write_u64(ts, 0)?;
        emu.write_u64(ts + 8, 42)?;
    }
    emu.ret(0);
    Ok(())
}

fn gettimeofday(emu: &mut Emu) -> Result<()> {
    let tv = emu.arg(0);
    let ns = realtime_ns(emu);
    if tv != 0 {
        emu.write_u64(tv, ns / 1_000_000_000)?;
        emu.p.memory.write_u32(tv + 8, ((ns % 1_000_000_000) / 1000) as u32)?;
    }
    emu.ret(0);
    Ok(())
}

fn time(emu: &mut Emu) -> Result<()> {
    let t = realtime_ns(emu) / 1_000_000_000;
    if emu.arg(0) != 0 {
        emu.write_u64(emu.arg(0), t)?;
    }
    emu.ret(t);
    Ok(())
}

// ---- process, environment, pthread ----

/// The process environment iOS gives an app: HOME is its data container.
fn default_env() -> Vec<(String, String)> {
    let home = crate::foundation::DATA_CONTAINER;
    vec![
        ("HOME".into(), home.into()),
        ("TMPDIR".into(), format!("{home}/tmp/")),
        ("CFFIXED_USER_HOME".into(), home.into()),
        ("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into()),
        ("USER".into(), "mobile".into()),
        ("LOGNAME".into(), "mobile".into()),
        ("SHELL".into(), "/bin/sh".into()),
    ]
}

/// The environment variables, in order.
pub fn environment(emu: &mut Emu) -> Vec<(String, String)> {
    env(emu).clone()
}

/// Set an environment variable for the app (as an Xcode scheme's do), before it runs.
pub fn set_env(emu: &mut Emu, name: &str, value: &str) {
    let e = env(emu);
    e.retain(|(k, _)| k != name);
    e.push((name.into(), value.into()));
}

fn env(emu: &mut Emu) -> &mut Vec<(String, String)> {
    emu.sys.env.get_or_insert_with(default_env)
}

/// `getenv(name)`: a pointer that stays valid until the variable changes.
fn getenv(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.arg(0))?;
    let Some(value) = env(emu).iter().find(|(k, _)| *k == name).map(|(_, v)| v.clone()) else {
        emu.ret(0);
        return Ok(());
    };
    let p = match emu.sys.env_strings.get(&(name.clone(), value.clone())) {
        Some(&p) => p,
        None => {
            let p = emu.alloc_cstr(&value)?;
            emu.sys.env_strings.insert((name, value), p);
            p
        }
    };
    emu.ret(p);
    Ok(())
}

/// `setenv(name, value, overwrite)`.
fn setenv(emu: &mut Emu) -> Result<()> {
    let (name, value, overwrite) = (emu.cstr(emu.arg(0))?, emu.cstr(emu.arg(1))?, emu.arg(2) as u32 != 0);
    let e = env(emu);
    match e.iter_mut().find(|(k, _)| *k == name) {
        Some(kv) if overwrite => kv.1 = value,
        Some(_) => {}
        None => e.push((name, value)),
    }
    emu.ret(0);
    Ok(())
}

/// `_NSGetEnviron()`: where `environ` lives, a `char ***`. The array it
/// points to is made afresh from the environment on each call, so it shows
/// what setenv and unsetenv did.
fn ns_get_environ(emu: &mut Emu) -> Result<()> {
    let vars: Vec<String> = env(emu).iter().map(|(k, v)| format!("{k}={v}")).collect();
    let array = emu.alloc((vars.len() as u64 + 1) * 8)?;
    for (i, kv) in vars.iter().enumerate() {
        let s = emu.alloc_cstr(kv)?;
        emu.write_u64(array + i as u64 * 8, s)?;
    }
    emu.write_u64(array + vars.len() as u64 * 8, 0)?;
    if emu.sys.environ_cell == 0 {
        emu.sys.environ_cell = emu.alloc(8)?;
    }
    let cell = emu.sys.environ_cell;
    emu.write_u64(cell, array)?;
    emu.ret(cell);
    Ok(())
}

fn unsetenv(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.arg(0))?;
    env(emu).retain(|(k, _)| *k != name);
    emu.ret(0);
    Ok(())
}

/// Set the guest's errno.
pub fn set_errno(emu: &mut Emu, v: u64) -> Result<()> {
    if emu.sys.errno == 0 {
        emu.sys.errno = emu.alloc(8)?;
    }
    emu.p.memory.write_u32(emu.sys.errno, v as u32)?;
    Ok(())
}

/// The guest's errno.
pub fn errno(emu: &mut Emu) -> Result<u32> {
    if emu.sys.errno == 0 {
        return Ok(0);
    }
    Ok(emu.p.memory.read_u32(emu.sys.errno)?)
}

fn errno_location(emu: &mut Emu) -> Result<()> {
    if emu.sys.errno == 0 {
        emu.sys.errno = emu.alloc(8)?;
    }
    emu.ret(emu.sys.errno);
    Ok(())
}

/// The app's process ID.
pub const PID: u64 = 1000;

fn getpid(emu: &mut Emu) -> Result<()> {
    emu.ret(PID);
    Ok(())
}

fn pagesize(emu: &mut Emu) -> Result<()> {
    emu.ret(pomme_mem::PAGE_SIZE);
    Ok(())
}

fn host_page_size(emu: &mut Emu) -> Result<()> {
    emu.write_u64(emu.arg(1), pomme_mem::PAGE_SIZE)?;
    emu.ret(0);
    Ok(())
}

/// sysconf: the few names apps ask about.
fn sysconf(emu: &mut Emu) -> Result<()> {
    // The same phone sysctl describes (CPUs, memory), and Darwin's limits.
    let v = match emu.arg(0) {
        1 => 1 << 20,                                                        // _SC_ARG_MAX
        2 => 266,                                                            // _SC_CHILD_MAX
        3 => 100,                                                            // _SC_CLK_TCK
        5 => 256,                                                            // _SC_OPEN_MAX (the default soft limit)
        29 => pomme_mem::PAGE_SIZE,                                          // _SC_PAGESIZE
        57 | 58 => crate::sysctl::CPUS as u64,                               // _SC_NPROCESSORS_CONF / _ONLN
        200 => crate::sysctl::MEMORY / pomme_mem::PAGE_SIZE,                 // _SC_PHYS_PAGES
        _ => u64::MAX,                                                       // -1: unknown
    };
    emu.ret(v);
    Ok(())
}




fn pthread_equal(emu: &mut Emu) -> Result<()> {
    emu.ret((emu.arg(0) == emu.arg(1)) as u64);
    Ok(())
}






/// `_tlv_bootstrap`: the thunk for every `__thread` variable. x0 points at a
/// descriptor {thunk, key, offset}; return the variable's address. Must only
/// touch x0, like the real one.
fn tlv_get_addr(emu: &mut Emu) -> Result<()> {
    let desc = emu.arg(0);
    let offset = emu.read_u64(desc + 16)?;
    let img = emu.p.images.iter().position(|i| desc >= i.start && desc < i.end).ok_or(Stop::Unsupported("TLV descriptor outside any image".into()))?;
    let key = (emu.th.current, img);
    let block = match emu.sys.tlv_blocks.get(&key) {
        Some(&b) => b,
        None => {
            let image = &emu.p.images[img];
            let data = image.section("__DATA", "__thread_data");
            let bss = image.section("__DATA", "__thread_bss");
            let start = data.or(bss).map_or(0, |d| d.0);
            let end = [data, bss].iter().flatten().map(|(a, s)| a + s).max().unwrap_or(0);
            let b = emu.alloc((end - start).max(16))?;
            if let Some((a, s)) = data {
                let init = bytes(emu, a, s)?;
                emu.p.memory.write(b + (a - start), &init)?;
            }
            emu.sys.tlv_blocks.insert(key, b);
            b
        }
    };
    emu.ret(block + offset);
    Ok(())
}

// ---- dispatch ----

/// `dispatch_sync(queue, block)` (and the barrier and `_f` forms): the block
/// runs as work for `queue`, in its turn. On the main queue, a concurrent
/// queue (not as a barrier) or an idle queue, it runs here and now. On a busy
/// queue it waits its turn: in place, then here, when this thread can wait in
/// place (the main thread, or inside a call); otherwise the queue runs it on
/// one of its own threads while this one waits for it to finish.
fn sync_on(emu: &mut Emu, queue: u64, f: u64, ctx: u64, barrier: bool) -> Result<()> {
    let run = |emu: &mut Emu| emu.call(f, &[ctx]).map(|_| ());
    let Some(q) = emu.sys.dqueues.get_mut(&queue).filter(|q| !q.on_main) else {
        return on_queue(emu, queue, run);
    };
    if q.width > 1 && !barrier && !q.barrier_running && !q.pending.iter().any(|w| w.barrier) {
        return on_queue(emu, queue, run);
    }
    let idle = q.running == 0 && q.pending.is_empty();
    if idle {
        q.running += 1;
        q.barrier_running |= barrier;
        let r = on_queue(emu, queue, run);
        finished(emu, queue, barrier)?;
        return r;
    }
    let flag = emu.alloc(8)?;
    emu.write_u64(flag, 0)?;
    if crate::threads::can_wait_in_place(emu) {
        submit_work(emu, queue, Work { f, ctx, group: None, barrier, turn: flag })?;
        crate::threads::block(emu, crate::threads::Wait::Flag { addr: flag, deadline: None })?;
        let r = on_queue(emu, queue, run);
        finished(emu, queue, barrier)?;
        return r;
    }
    // The queue runs it; this thread waits for the done flag (a block that sets it).
    let done = done_flag_block(emu, f, ctx, flag)?;
    let invoke = emu.read_u64(done + 16)?;
    submit_work(emu, queue, Work { f: invoke, ctx: done, group: None, barrier, turn: 0 })?;
    let r = crate::threads::block(emu, crate::threads::Wait::Flag { addr: flag, deadline: None })?;
    let _ = r;
    Ok(())
}

/// A block that runs `f(ctx)` and then sets `flag`.
fn done_flag_block(emu: &mut Emu, f: u64, ctx: u64, flag: u64) -> Result<u64> {
    let b = host_block(emu, |e| {
        let b = e.arg(0);
        let (f, ctx, flag) = (e.read_u64(b + 48)?, e.read_u64(b + 56)?, e.read_u64(b + 64)?);
        e.call(f, &[ctx])?;
        e.write_u64(flag, 1)?;
        Ok(())
    })?;
    // host_block's layout is 48 bytes: put the arguments after it.
    let big = emu.alloc(72)?;
    let bytes = emu.p.memory.read(b, 48)?.to_vec();
    emu.p.memory.write(big, &bytes)?;
    emu.write_u64(big + 24, big + 32)?; // its descriptor, inside it
    emu.write_u64(big + 40, 72)?;
    emu.write_u64(big + 48, f)?;
    emu.write_u64(big + 56, ctx)?;
    emu.write_u64(big + 64, flag)?;
    Ok(big)
}

fn dispatch_sync(emu: &mut Emu) -> Result<()> {
    let (queue, block) = (emu.arg(0), emu.arg(1));
    let invoke = emu.read_u64(block + 16)?;
    sync_on(emu, queue, invoke, block, false)
}

fn dispatch_barrier_sync(emu: &mut Emu) -> Result<()> {
    let (queue, block) = (emu.arg(0), emu.arg(1));
    let invoke = emu.read_u64(block + 16)?;
    sync_on(emu, queue, invoke, block, true)
}

fn dispatch_barrier_async(emu: &mut Emu) -> Result<()> {
    let queue = emu.arg(0);
    let (invoke, block) = copied_block(emu, emu.arg(1))?;
    submit_work(emu, queue, Work { f: invoke, ctx: block, group: None, barrier: true, turn: 0 })
}

/// Copy a block to the heap: (invoke, block).
fn copied_block(emu: &mut Emu, block: u64) -> Result<(u64, u64)> {
    emu.cpu.x[0] = block;
    crate::libc::block_copy(emu)?;
    let block = emu.cpu.x[0];
    Ok((emu.read_u64(block + 16)?, block))
}

/// Run `f(ctx)` on `queue`, as libdispatch does: the global queue starts a
/// thread for each piece of work; a queue the app made runs its work on
/// threads in order (one at a time when serial; barriers alone); the main
/// queue (and queues targeting it) is serviced by the main thread. Work for
/// `group` counts toward it.
fn submit(emu: &mut Emu, queue: u64, f: u64, ctx: u64, group: Option<u64>) -> Result<()> {
    submit_work(emu, queue, Work { f, ctx, group, barrier: false, turn: 0 })
}

fn submit_work(emu: &mut Emu, queue: u64, work: Work) -> Result<()> {
    let Work { f, ctx, group, .. } = work;
    if emu.sys.cancelled.contains(&ctx) {
        return Ok(());
    }
    if emu.sys.dqueues.get(&queue).is_some_and(|q| !q.on_main) {
        if let Some(g) = group {
            emu.sys.groups.entry(g).or_default().count += 1;
        }
        emu.sys.dqueues.get_mut(&queue).unwrap().pending.push_back(work);
        return pump(emu, queue);
    }
    if queue != 0 && queue == emu.sys.global_queue {
        let tid = crate::threads::spawn(emu, f, ctx, "dispatch")?;
        emu.sys.current_queues.insert(tid, vec![queue]);
        if let Some(g) = group {
            emu.sys.groups.entry(g).or_default().count += 1;
            emu.sys.group_threads.insert(tid, g);
        }
    } else {
        let queue = if queue == 0 { main_queue(emu) } else { queue };
        emu.sys.queue.push_back((f, ctx, queue));
        crate::threads::wake_main_queue(emu);
    }
    Ok(())
}

/// Start what `queue` can run now: its next work, in order, while it has room.
fn pump(emu: &mut Emu, queue: u64) -> Result<()> {
    loop {
        let Some(q) = emu.sys.dqueues.get_mut(&queue) else { return Ok(()) };
        let Some(next) = q.pending.front().copied() else { return Ok(()) };
        let room = if next.barrier { q.running == 0 } else { !q.barrier_running && q.running < q.width };
        if !room {
            return Ok(());
        }
        q.pending.pop_front();
        q.running += 1;
        q.barrier_running |= next.barrier;
        if next.turn != 0 {
            // A dispatch_sync caller's turn: it runs the block itself, then calls `finished`.
            emu.p.memory.write_u32(next.turn, 1)?;
            continue;
        }
        let tid = crate::threads::spawn(emu, next.f, next.ctx, "dispatch")?;
        emu.sys.current_queues.insert(tid, vec![queue]);
        emu.sys.queue_threads.insert(tid, (queue, next.barrier));
        if let Some(g) = next.group {
            emu.sys.group_threads.insert(tid, g);
        }
    }
}

/// A piece of `queue`'s work is done: let the next run.
fn finished(emu: &mut Emu, queue: u64, barrier: bool) -> Result<()> {
    if let Some(q) = emu.sys.dqueues.get_mut(&queue) {
        q.running = q.running.saturating_sub(1);
        if barrier {
            q.barrier_running = false;
        }
    }
    pump(emu, queue)
}

/// Run `f(ctx)` on dispatch queue `queue`, as `dispatch_async_f` does (no queue: the main one).
pub(crate) fn dispatch_on(emu: &mut Emu, queue: u64, f: u64, ctx: u64) -> Result<()> {
    submit(emu, queue, f, ctx, None)
}

/// Run `f(ctx)` on the main thread, in turn with the main queue's other work.
pub(crate) fn run_on_main(emu: &mut Emu, f: u64, ctx: u64) {
    let main = emu.sys.data.get("__dispatch_main_q").copied().unwrap_or(0);
    emu.sys.queue.push_back((f, ctx, main));
    crate::threads::wake_main_queue(emu);
}

fn dispatch_async(emu: &mut Emu) -> Result<()> {
    let queue = emu.arg(0);
    let (invoke, block) = copied_block(emu, emu.arg(1))?;
    submit(emu, queue, invoke, block, None)
}

fn dispatch_async_f(emu: &mut Emu) -> Result<()> {
    let (queue, ctx, f) = (emu.arg(0), emu.arg(1), emu.arg(2));
    submit(emu, queue, f, ctx, None)
}

/// A thread finished: if it was a group's work, the group has one less.
pub(crate) fn thread_finished(emu: &mut Emu, tid: usize) -> Result<()> {
    emu.sys.current_queues.remove(&tid);
    if let Some(g) = emu.sys.group_threads.remove(&tid) {
        group_leave(emu, g)?;
    }
    if let Some((queue, barrier)) = emu.sys.queue_threads.remove(&tid) {
        finished(emu, queue, barrier)?;
    }
    Ok(())
}

fn group_leave(emu: &mut Emu, g: u64) -> Result<()> {
    let grp = emu.sys.groups.entry(g).or_default();
    grp.count = grp.count.saturating_sub(1);
    if grp.count == 0 {
        for (q, f, ctx) in std::mem::take(&mut grp.notify) {
            submit(emu, q, f, ctx, None)?;
        }
    }
    Ok(())
}

/// Run queued async work until the queue is empty.
pub fn drain_queue(emu: &mut Emu) -> Result<usize> {
    let mut n = 0;
    while let Some((f, arg, queue)) = emu.sys.queue.pop_front() {
        if emu.sys.cancelled.contains(&arg) {
            continue;
        }
        on_queue(emu, queue, |emu| emu.call(f, &[arg]).map(|_| ()))?;
        n += 1;
    }
    Ok(n)
}

/// The main queue (`_dispatch_main_q`).
fn main_queue(emu: &Emu) -> u64 {
    emu.sys.data.get("__dispatch_main_q").copied().unwrap_or(0)
}

/// Run `work` as work for `queue` on this thread.
fn on_queue(emu: &mut Emu, queue: u64, work: impl FnOnce(&mut Emu) -> Result<()>) -> Result<()> {
    let tid = emu.th.current;
    emu.sys.current_queues.entry(tid).or_default().push(queue);
    let r = work(emu);
    if let Some(stack) = emu.sys.current_queues.get_mut(&tid) {
        stack.pop();
    }
    r
}

/// The queue this thread is running work for: the innermost one, the main
/// queue on the main thread outside any, or none (a thread of the app's own).
fn current_queue(emu: &Emu) -> u64 {
    let tid = emu.th.current;
    match emu.sys.current_queues.get(&tid).and_then(|s| s.last()) {
        Some(&q) => q,
        None if tid == 0 => main_queue(emu),
        None => 0,
    }
}

/// `dispatch_queue_create(label, attr)` (and `…_with_target`, whose target
/// may be the main queue): serial, or concurrent with DISPATCH_QUEUE_CONCURRENT.
fn dispatch_queue_create(emu: &mut Emu) -> Result<()> {
    let (label, attr) = (emu.arg(0), emu.arg(1));
    make_queue(emu, label, attr, 0)
}

fn dispatch_queue_create_with_target(emu: &mut Emu) -> Result<()> {
    let (label, attr, target) = (emu.arg(0), emu.arg(1), emu.arg(2));
    make_queue(emu, label, attr, target)
}

fn make_queue(emu: &mut Emu, label: u64, attr: u64, target: u64) -> Result<()> {
    let cls = crate::objc::host_class(emu, "OS_dispatch_queue")?;
    let q = emu.alloc(64)?;
    emu.write_u64(q, cls)?;
    let label = if label == 0 { 0 } else { emu.alloc_cstr(&emu.cstr(label)?)? };
    emu.write_u64(q + 8, label)?;
    let concurrent = attr != 0 && emu.sys.data.get("__dispatch_queue_attr_concurrent").is_some_and(|&c| c == attr);
    let on_main = target != 0 && target == main_queue(emu);
    emu.sys.dqueues.insert(q, DQueue { width: if concurrent { usize::MAX } else { 1 }, on_main, ..Default::default() });
    emu.ret(q);
    Ok(())
}

/// The dispatch queues with work waiting or running, for a stop's report:
/// one line each with the label, what's waiting and what's running.
pub fn queue_report(emu: &Emu) -> String {
    let label = |q: u64| -> String {
        let l = emu.read_u64(q + 8).unwrap_or(0);
        if l == 0 { format!("{q:#x}") } else { emu.cstr(l).unwrap_or_default() }
    };
    let mut s = String::new();
    if !emu.sys.queue.is_empty() {
        s += &format!("  main queue: {} waiting\n", emu.sys.queue.len());
    }
    let mut queues: Vec<_> = emu.sys.dqueues.iter().filter(|(_, d)| !d.pending.is_empty() || d.running > 0).collect();
    queues.sort_by_key(|(q, _)| **q);
    for (&q, d) in queues {
        let kind = if d.width == 1 { "serial" } else { "concurrent" };
        let barrier = if d.barrier_running { ", a barrier" } else { "" };
        s += &format!("  {} ({kind}): {} waiting, {} running{barrier}\n", label(q), d.pending.len(), d.running);
    }
    let after = emu.sys.after.iter().flatten().count();
    if after > 0 {
        s += &format!("  {after} dispatch_after items waiting for their time\n");
    }
    s
}

/// The one global queue (every quality of service is the same here); its work runs on threads.
pub(crate) fn global_queue(emu: &mut Emu) -> Result<u64> {
    if emu.sys.global_queue == 0 {
        let cls = crate::objc::host_class(emu, "OS_dispatch_queue")?;
        let q = emu.alloc(64)?;
        emu.write_u64(q, cls)?;
        let label = emu.alloc_cstr("com.apple.root.default-qos")?;
        emu.write_u64(q + 8, label)?;
        emu.sys.global_queue = q;
    }
    Ok(emu.sys.global_queue)
}

fn dispatch_get_global_queue(emu: &mut Emu) -> Result<()> {
    let q = global_queue(emu)?;
    emu.ret(q);
    Ok(())
}

/// `dispatch_queue_get_label(queue)`; DISPATCH_CURRENT_QUEUE_LABEL (NULL) is the current queue.
fn dispatch_queue_get_label(emu: &mut Emu) -> Result<()> {
    let q = emu.arg(0);
    let q = if q == 0 { current_queue(emu) } else { q };
    let l = if q == 0 { 0 } else { emu.read_u64(q + 8)? };
    let l = if l == 0 { emu.alloc_cstr("com.apple.main-thread")? } else { l };
    emu.ret(l);
    Ok(())
}

fn dispatch_queue_set_specific(emu: &mut Emu) -> Result<()> {
    emu.sys.queue_specific.insert((emu.arg(0), emu.arg(1)), emu.arg(2));
    Ok(())
}

/// `dispatch_get_specific(key)`: the value the current queue has for it.
fn dispatch_get_specific(emu: &mut Emu) -> Result<()> {
    let key = emu.arg(0);
    let q = current_queue(emu);
    let v = emu.sys.queue_specific.get(&(q, key)).copied().unwrap_or(0);
    emu.ret(v);
    Ok(())
}

/// `dispatch_queue_get_specific(queue, key)`
fn dispatch_queue_get_specific(emu: &mut Emu) -> Result<()> {
    let (q, key) = (emu.arg(0), emu.arg(1));
    let v = emu.sys.queue_specific.get(&(q, key)).copied().unwrap_or(0);
    emu.ret(v);
    Ok(())
}

fn dispatch_semaphore_create(emu: &mut Emu) -> Result<()> {
    let s = emu.alloc(16)?;
    emu.write_u64(s + 8, emu.arg(0))?;
    emu.ret(s);
    Ok(())
}


fn dispatch_semaphore_signal(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let v = emu.read_u64(s + 8)?;
    emu.write_u64(s + 8, v + 1)?;
    emu.ret(0);
    Ok(())
}

fn dispatch_group_create(emu: &mut Emu) -> Result<()> {
    let g = emu.alloc(16)?;
    emu.ret(g);
    Ok(())
}

/// `dispatch_group_async(group, queue, block)`
fn dispatch_group_async(emu: &mut Emu) -> Result<()> {
    let (g, queue) = (emu.arg(0), emu.arg(1));
    let (invoke, block) = copied_block(emu, emu.arg(2))?;
    submit(emu, queue, invoke, block, Some(g))
}

/// `dispatch_group_notify(group, queue, block)`: once the group's work is done.
/// (Work on the main queue isn't counted: it runs in order, before this.)
fn dispatch_group_notify(emu: &mut Emu) -> Result<()> {
    let (g, queue) = (emu.arg(0), emu.arg(1));
    let (invoke, block) = copied_block(emu, emu.arg(2))?;
    if emu.sys.groups.get(&g).is_some_and(|x| x.count > 0) {
        emu.sys.groups.get_mut(&g).unwrap().notify.push((queue, invoke, block));
        Ok(())
    } else {
        submit(emu, queue, invoke, block, None)
    }
}

/// `dispatch_group_wait(group, timeout)`: until the group's work is done (0) or the timeout (non-zero).
fn dispatch_group_wait(emu: &mut Emu) -> Result<()> {
    let (g, timeout) = (emu.arg(0), emu.arg(1));
    let deadline = (timeout != u64::MAX).then_some(timeout);
    let r = crate::threads::block(emu, crate::threads::Wait::Group { group: g, deadline })?;
    emu.ret(r);
    Ok(())
}

/// Whether `group` has no work outstanding.
pub(crate) fn group_done(emu: &Emu, group: u64) -> bool {
    emu.sys.groups.get(&group).is_none_or(|g| g.count == 0)
}


/// Run `f(ctx)` on `queue` at `when` (a `dispatch_time`: nanoseconds of
/// virtual time), from a main-run-loop timer.
fn submit_after(emu: &mut Emu, when: u64, queue: u64, f: u64, ctx: u64) -> Result<()> {
    if when == u64::MAX {
        return Ok(()); // DISPATCH_TIME_FOREVER
    }
    let now = crate::threads::now_ns(emu);
    let delay = when.saturating_sub(now) as f64 / 1e9;
    let i = emu.sys.after.len();
    emu.sys.after.push(Some((queue, f, ctx)));
    crate::uikit::after_on_main(emu, delay, fire_after, i as u64)
}

fn fire_after(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(1) as usize;
    let Some((queue, f, ctx)) = emu.sys.after.get_mut(i).and_then(Option::take) else { return Ok(()) };
    submit(emu, queue, f, ctx, None)
}

/// `dispatch_after(when, queue, block)`
fn dispatch_after(emu: &mut Emu) -> Result<()> {
    let (when, queue) = (emu.arg(0), emu.arg(1));
    let (invoke, block) = copied_block(emu, emu.arg(2))?;
    submit_after(emu, when, queue, invoke, block)
}

/// `dispatch_after_f(when, queue, context, function)`
fn dispatch_after_f(emu: &mut Emu) -> Result<()> {
    let (when, queue, ctx, f) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    submit_after(emu, when, queue, f, ctx)
}

// A dispatch block (dispatch_block_create) is a wrapper, as in libdispatch:
// a block whose invoke runs the original (unless cancelled), then marks it
// done, waking `dispatch_block_wait` and submitting `dispatch_block_notify`
// work. It's a global block, so copies are the same block (cancel, wait and
// notify find it). After the Block_layout: the original at +48, done at +56.
const WRAPPED: u64 = 48;
const DONE: u64 = 56;

/// `dispatch_block_create(flags, block)`
fn dispatch_block_create(emu: &mut Emu) -> Result<()> {
    let (_, original) = copied_block(emu, emu.arg(1))?;
    let invoke = emu.host_thunk(run_dispatch_block)?;
    let b = emu.alloc(64)?;
    let isa = crate::objc::host_class(emu, "__NSGlobalBlock__")?;
    emu.write_u64(b, isa)?;
    emu.p.memory.write_u32(b + 8, 1 << 28)?; // BLOCK_IS_GLOBAL
    emu.write_u64(b + 16, invoke)?;
    emu.write_u64(b + 24, b + 32)?; // descriptor {reserved, size}
    emu.write_u64(b + 40, 64)?;
    emu.write_u64(b + WRAPPED, original)?;
    emu.write_u64(b + DONE, 0)?;
    emu.ret(b);
    Ok(())
}

fn is_dispatch_block(emu: &mut Emu, b: u64) -> Result<bool> {
    let invoke = emu.host_thunk(run_dispatch_block)?;
    Ok(b != 0 && emu.read_u64(b + 16)? == invoke)
}

/// A dispatch block runs: the original, then it's done.
fn run_dispatch_block(emu: &mut Emu) -> Result<()> {
    let b = emu.arg(0);
    if emu.sys.cancelled.contains(&b) {
        return Ok(());
    }
    let original = emu.read_u64(b + WRAPPED)?;
    call_block(emu, original, &[])?;
    emu.write_u64(b + DONE, 1)?;
    for (queue, invoke, block) in emu.sys.block_notify.remove(&b).unwrap_or_default() {
        submit(emu, queue, invoke, block, None)?;
    }
    Ok(())
}

/// `dispatch_block_wait(block, timeout)`: 0 once it has run, non-zero on timeout.
fn dispatch_block_wait(emu: &mut Emu) -> Result<()> {
    let (b, timeout) = (emu.arg(0), emu.arg(1));
    if !is_dispatch_block(emu, b)? {
        emu.ret(0);
        return Ok(());
    }
    let deadline = (timeout != u64::MAX).then_some(timeout);
    let r = crate::threads::block(emu, crate::threads::Wait::Flag { addr: b + DONE, deadline })?;
    emu.ret(r);
    Ok(())
}

/// `dispatch_block_notify(block, queue, notification)`: run it on `queue` once `block` has.
fn dispatch_block_notify(emu: &mut Emu) -> Result<()> {
    let (b, queue) = (emu.arg(0), emu.arg(1));
    let (invoke, note) = copied_block(emu, emu.arg(2))?;
    if !is_dispatch_block(emu, b)? || emu.read_u64(b + DONE)? != 0 {
        return submit(emu, queue, invoke, note, None);
    }
    emu.sys.block_notify.entry(b).or_default().push((queue, invoke, note));
    Ok(())
}

// ---- C++ ABI bits that live in libSystem ----

/// `_Unwind_RaiseException`: pomme can't unwind C++ exceptions, so a throw
/// stops the run, reporting the exception's `what()` (a std::exception
/// subclass: jsi::JSError carries the JavaScript error's message and stack).
fn unwind_raise_exception(emu: &mut Emu) -> Result<()> {
    // libc++abi puts the thrown object right after the 32-byte _Unwind_Exception.
    let obj = emu.arg(0) + 32;
    let what = (|| -> Result<String> {
        let vtable = emu.read_u64(obj)?;
        let what_fn = emu.read_u64(vtable + 16)?;
        let p = emu.call(what_fn, &[obj])?;
        emu.cstr(p)
    })()
    .unwrap_or_else(|_| "(no message)".into());
    Err(crate::Stop::Unsupported(format!("C++ exception thrown (pomme can't unwind it): {what}")))
}

/// `_dyld_find_unwind_sections(addr, info*)`: for libunwind, the image that
/// holds `addr` and its unwind tables. dyld_unwind_sections is {mach_header,
/// dwarf_section (__eh_frame), its length, compact_unwind_section
/// (__unwind_info), its length}.
fn dyld_find_unwind_sections(emu: &mut Emu) -> Result<()> {
    let (addr, info) = (emu.arg(0), emu.arg(1));
    let Some(img) = emu.p.image_at(addr) else {
        // pomme's own frame (a host function that called back into the app):
        // libunwind can't see through it, so the exception stops here.
        if addr >= pomme_cpu::HOST_RETURN - 16 {
            emu.sys.unwound_into_host = true;
        }
        emu.ret(0);
        return Ok(());
    };
    let header = img.header;
    let (eh, eh_len) = img.section("__TEXT", "__eh_frame").unwrap_or((0, 0));
    let (cu, cu_len) = img.section("__TEXT", "__unwind_info").unwrap_or((0, 0));
    for (i, v) in [header, eh, eh_len, cu, cu_len].into_iter().enumerate() {
        emu.write_u64(info + i as u64 * 8, v)?;
    }
    emu.ret(1);
    Ok(())
}

fn cxa_atexit(emu: &mut Emu) -> Result<()> {
    emu.ret(0); // destructors at exit: never run
    Ok(())
}

/// `_availability_version_check(count, {platform, version}[])`: is the OS at
/// least the version given for iOS? (What `@available` and `#available` call.)
/// The answer depends on who asks: see `system::os_for_caller`.
fn availability_version_check(emu: &mut Emu) -> Result<()> {
    const IOS: u32 = 2;
    const IOS_SIMULATOR: u32 = 7;
    let (count, list) = (emu.arg(0), emu.arg(1));
    let mut ok = true;
    for i in 0..count.min(16) {
        let platform = emu.p.memory.read_u32(list + i * 8)?;
        let version = emu.p.memory.read_u32(list + i * 8 + 4)?;
        if platform == IOS || platform == IOS_SIMULATOR {
            ok = crate::system::available_for_caller(emu, emu.cpu.x[30], crate::system::Version(version));
        }
    }
    emu.ret(ok as u64);
    Ok(())
}

fn os_system_version(emu: &mut Emu) -> Result<()> {
    // Returns a struct { major, minor, patch } of i64 in x0..x2.
    let v = emu.system.os;
    emu.cpu.x[0] = v.major() as u64;
    emu.cpu.x[1] = v.minor() as u64;
    emu.cpu.x[2] = v.patch() as u64;
    Ok(())
}

fn stack_chk_fail(_: &mut Emu) -> Result<()> {
    Err(Stop::Unsupported("stack protector tripped (__stack_chk_fail)".into()))
}

fn assert_rtn(emu: &mut Emu) -> Result<()> {
    let func = emu.cstr(emu.arg(0)).unwrap_or_default();
    let file = emu.cstr(emu.arg(1)).unwrap_or_default();
    let expr = emu.cstr(emu.arg(3)).unwrap_or_default();
    Err(Stop::Unsupported(format!("assertion failed: {expr} in {func} ({file}:{})", emu.arg(2))))
}

fn nsget_argc(emu: &mut Emu) -> Result<()> {
    // Return a pointer to an int holding argc.
    let p = emu.alloc(8)?;
    emu.write_u64(p, emu.argc)?;
    emu.ret(p);
    Ok(())
}

fn nsget_argv(emu: &mut Emu) -> Result<()> {
    let p = emu.alloc(8)?;
    emu.write_u64(p, emu.argv)?;
    emu.ret(p);
    Ok(())
}

fn nsget_mach_header(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.p.main().header);
    Ok(())
}

fn arc4random_uniform(emu: &mut Emu) -> Result<()> {
    // Deterministic: a fixed-seed xorshift over the step counter.
    let bound = emu.arg(0).max(1);
    let mut x = emu.cpu.steps ^ 0x9e37_79b9_7f4a_7c15;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    emu.ret(x % bound);
    Ok(())
}

fn arc4random_buf(emu: &mut Emu) -> Result<()> {
    let (p, n) = (emu.arg(0), emu.arg(1));
    let mut x = emu.cpu.steps ^ 0x9e37_79b9_7f4a_7c15;
    let buf: Vec<u8> = (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect();
    emu.p.memory.write(p, &buf)?;
    Ok(())
}

/// `posix_memalign(&p, alignment, size)`: EINVAL unless the alignment is a power of two (a pointer's size or more).
fn posix_memalign(emu: &mut Emu) -> Result<()> {
    let (out, align, size) = (emu.arg(0), emu.arg(1), emu.arg(2));
    if !align.is_power_of_two() || align < 8 {
        emu.ret(22);
        return Ok(());
    }
    let p = emu.heap.alloc_aligned(&mut emu.p.memory, size, align)?;
    emu.write_u64(out, p)?;
    emu.ret(0);
    Ok(())
}

/// Bytes from pomme's seeded generator (so a run repeats): /dev/random, getentropy.
pub(crate) fn random_bytes(emu: &mut Emu, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n);
    let mut x = emu.sys.rng_state();
    while out.len() < n {
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        out.extend_from_slice(&x.wrapping_mul(0x2545_f491_4f6c_dd1d).to_le_bytes());
    }
    emu.sys.set_rng_state(x);
    out.truncate(n);
    out
}

/// `getentropy(buf, len)`: up to 256 bytes (EIO for more, as Darwin's).
fn getentropy(emu: &mut Emu) -> Result<()> {
    let (p, n) = (emu.arg(0), emu.arg(1));
    if n > 256 {
        set_errno(emu, 5)?;
        emu.ret(u64::MAX);
        return Ok(());
    }
    let bytes = random_bytes(emu, n as usize);
    emu.p.memory.write(p, &bytes)?;
    emu.ret(0);
    Ok(())
}

fn malloc_good_size(_: &mut Emu) -> Result<()> {
    Ok(())
}

// ---- dlopen ----
//
// Every image an app can dlopen (its own frameworks) was loaded at launch
// (see pomme-loader's bundled_frameworks), so dlopen finds it; a handle is
// the image's header address.

const RTLD_NEXT: u64 = -1i64 as u64;
const RTLD_DEFAULT: u64 = -2i64 as u64;
const RTLD_SELF: u64 = -3i64 as u64;
const RTLD_MAIN_ONLY: u64 = -5i64 as u64;

fn set_dlerror(emu: &mut Emu, msg: Option<String>) -> Result<()> {
    emu.sys.dlerror = match msg {
        Some(m) => emu.alloc_cstr(&m)?,
        None => 0,
    };
    Ok(())
}

/// `dlopen(path, mode)`: NULL is the main executable; otherwise a loaded image by path.
fn dlopen(emu: &mut Emu) -> Result<()> {
    let p = emu.arg(0);
    if p == 0 {
        let h = emu.p.main().header;
        emu.ret(h);
        return Ok(());
    }
    let path = emu.cstr(p)?;
    let host = crate::fs::host_path(emu, &path).map(|h| pomme_loader::vfs::canonicalize(&h));
    let file = std::path::Path::new(&path).file_name().map(|f| f.to_string_lossy().into_owned());
    let found = emu.p.images.iter().find(|i| {
        host.as_ref().is_some_and(|h| *h == i.path)
            || i.install_name.as_deref() == Some(path.as_str())
            || (path.contains(".framework/") && file.as_deref() == i.path.file_name().and_then(|f| f.to_str()) && i.path.to_string_lossy().ends_with(path.trim_start_matches("@rpath/").trim_start_matches("@executable_path/")))
    });
    let found = match found.map(|i| i.header) {
        Some(h) => Some(h),
        None if is_system_library(&path) => Some(match emu.sys.system_libraries.get(&path) {
            Some(&h) => h,
            None => {
                let h = emu.alloc(16)?;
                emu.sys.system_libraries.insert(path.clone(), h);
                h
            }
        }),
        None => None,
    };
    match found {
        Some(h) => {
            set_dlerror(emu, None)?;
            emu.ret(h);
        }
        None => {
            set_dlerror(emu, Some(format!("dlopen({path}): image not found")))?;
            emu.ret(0);
        }
    }
    Ok(())
}

/// A path in the system's libraries (the shared cache, on a phone): a
/// framework's binary or a dylib under /usr/lib.
fn is_system_library(path: &str) -> bool {
    let framework = |root: &str| {
        path.strip_prefix(root).and_then(|rest| rest.split_once(".framework/")).is_some_and(|(name, bin)| !name.contains('/') && bin == name)
    };
    framework("/System/Library/Frameworks/") || framework("/System/Library/PrivateFrameworks/") || (path.starts_with("/usr/lib/") && path.ends_with(".dylib"))
}

/// `dlsym(handle, name)`: in that image, or (RTLD_DEFAULT and friends) in every image.
fn dlsym(emu: &mut Emu) -> Result<()> {
    let (handle, raw) = (emu.arg(0), emu.cstr(emu.arg(1))?);
    let name = format!("_{raw}");
    let addr = match handle {
        RTLD_DEFAULT | RTLD_NEXT | RTLD_SELF => emu.p.images.iter().find_map(|i| i.symbol(&name)),
        RTLD_MAIN_ONLY => emu.p.main().symbol(&name),
        h => emu.p.images.iter().find(|i| i.header == h).and_then(|i| i.symbol(&name)),
    };
    // Not in a guest image: a system function pomme implements (libSystem and friends).
    let addr = match addr {
        Some(a) => Some(a),
        None if matches!(handle, RTLD_DEFAULT | RTLD_NEXT | RTLD_SELF) || !emu.p.images.iter().any(|i| i.header == handle) => match emu.functions.get(name.as_str()).copied() {
            Some(f) => Some(emu.host_thunk(f)?),
            None => None,
        },
        None => None,
    };
    if emu.trace {
        println!("[dlsym] {handle:#x} {raw} → {addr:#x?}");
    }
    match addr {
        Some(a) => {
            set_dlerror(emu, None)?;
            emu.ret(a);
        }
        None => {
            set_dlerror(emu, Some(format!("dlsym({handle:#x}, {raw}): symbol not found")))?;
            emu.ret(0);
        }
    }
    Ok(())
}

/// `dlerror()`: the last failure, once.
fn dlerror(emu: &mut Emu) -> Result<()> {
    let e = std::mem::take(&mut emu.sys.dlerror);
    emu.ret(e);
    Ok(())
}

fn write_fd(emu: &mut Emu) -> Result<()> {
    let (fd, buf, n) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2));
    if fd == 1 || fd == 2 {
        let b = bytes(emu, buf, n)?;
        print!("{}", String::from_utf8_lossy(&b));
    }
    emu.ret(n);
    Ok(())
}


pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_memcmp", memcmp),
    ("_memchr", memchr),
    ("_memset_pattern4", |e| memset_pattern(e, 4)),
    ("_memset_pattern16", |e| memset_pattern(e, 16)),
    ("_strchr", strchr),
    ("_strrchr", strrchr),
    ("_strncmp", strncmp),
    ("_strcasecmp", strcasecmp),
    ("_strstr", strstr),
    ("_strcpy", strcpy),
    ("_strncpy", strncpy),
    ("_strncat", strncat),
    ("_strdup", strdup),
    ("_strtol", strto_int),
    ("_strtoul", strto_int),
    ("_strtoull", strto_int),
    ("_strtod", strtod),
    ("_strerror", strerror),
    ("___tolower", tolower),
    ("___toupper", toupper),
    ("___maskrune", maskrune),
    ("_sin", m_sin),
    ("_cos", m_cos),
    ("_tan", m_tan),
    ("_asin", m_asin),
    ("_acos", m_acos),
    ("_atan", m_atan),
    ("_sinh", m_sinh),
    ("_cosh", m_cosh),
    ("_tanh", m_tanh),
    ("_asinh", m_asinh),
    ("_acosh", m_acosh),
    ("_atanh", m_atanh),
    ("_exp", m_exp),
    ("_exp2", m_exp2),
    ("_expm1", m_expm1),
    ("___exp10", m_exp10),
    ("_log", m_log),
    ("_log2", m_log2),
    ("_log10", m_log10),
    ("_log1p", m_log1p),
    ("_sqrt", m_sqrt),
    ("_cbrt", m_cbrt),
    ("_floor", m_floor),
    ("_ceil", m_ceil),
    ("_trunc", m_trunc),
    ("_pow", m_pow),
    ("_atan2", m_atan2),
    ("_hypot", m_hypot),
    ("_fmod", m_fmod),
    ("_remainder", m_remainder),
    ("_nextafter", m_nextafter),
    ("_sinf", m_sinf),
    ("_round", m_round),
    ("_logb", m_logb),
    ("_powf", |e| libmf2(e, f32::powf)),
    ("_atan2f", |e| libmf2(e, f32::atan2)),
    ("_hypotf", |e| libmf2(e, f32::hypot)),
    ("_fdim", |e| {
        let (a, b) = (f64_arg(e, 0), f64_arg(e, 1));
        ret_f64(e, if a > b { a - b } else { 0.0 });
        Ok(())
    }),
    ("_acosf", |e| {
        let v = f32_arg(e, 0).acos();
        ret_f32(e, v);
        Ok(())
    }),
    ("_asinf", |e| {
        let v = f32_arg(e, 0).asin();
        ret_f32(e, v);
        Ok(())
    }),
    ("_modff", m_modff),
    ("_strspn", |e| span(e, true)),
    ("_strcspn", |e| span(e, false)),
    ("_strtok", strtok),
    ("_strtok_r", strtok_r),
    ("_strerror_r", strerror_r),
    ("_getrusage", getrusage),
    ("_cosf", m_cosf),
    ("_tanf", m_tanf),
    ("_expf", m_expf),
    ("_exp2f", m_exp2f),
    ("_logf", m_logf),
    ("_log2f", m_log2f),
    ("_log10f", m_log10f),
    ("_fmodf", m_fmodf),
    ("_remainderf", m_remainderf),
    ("___sincos_stret", m_sincos),
    ("___sincosf_stret", m_sincosf),
    ("_nan", m_nan),
    ("_modf", m_modf),
    ("_frexp", m_frexp),
    ("___udivti3", udivti3),
    ("___umodti3", umodti3),
    ("___divti3", divti3),
    ("___modti3", modti3),
    ("_lldiv", lldiv),
    ("_mach_absolute_time", mach_absolute_time),
    ("_mach_timebase_info", mach_timebase_info),
    ("_clock_gettime", clock_gettime),
    ("_clock_gettime_nsec_np", clock_gettime_nsec_np),
    ("_clock_getres", clock_getres),
    ("_gettimeofday", gettimeofday),
    ("_time", time),
    ("_getenv", getenv),
    ("_setenv", setenv),
    ("_unsetenv", unsetenv),
    ("___error", errno_location),
    ("_getpid", getpid),
    ("__NSGetEnviron", ns_get_environ),
    ("_getpagesize", pagesize),
    ("_host_page_size", host_page_size),
    ("_sysconf", sysconf),
    ("_pthread_equal", pthread_equal),
    ("_pthread_sigmask", ret_zero),
    // Signals: handlers are recorded (and reported back as the previous action)
    // but pomme never delivers signals.
    ("_sigaction", |e| {
        // struct sigaction (Darwin): handler (8), mask (4), flags (4) = 16 bytes.
        let (sig, act, old) = (e.arg(0) as i32, e.arg(1), e.arg(2));
        let prev = e.sys.signal_actions.get(&sig).cloned().unwrap_or([0u8; 16].to_vec());
        if old != 0 {
            e.p.memory.write(old, &prev)?;
        }
        if act != 0 {
            let new = e.p.memory.read(act, 16)?.to_vec();
            e.sys.signal_actions.insert(sig, new);
        }
        e.ret(0);
        Ok(())
    }),
    ("_signal", |e| {
        let (sig, handler) = (e.arg(0) as i32, e.arg(1));
        let prev = e.sys.signal_actions.get(&sig).map_or(0, |a| u64::from_le_bytes(a[..8].try_into().unwrap()));
        let mut act = vec![0u8; 16];
        act[..8].copy_from_slice(&handler.to_le_bytes());
        e.sys.signal_actions.insert(sig, act);
        e.ret(prev);
        Ok(())
    }),
    ("_sigemptyset", |e| {
        let set = e.arg(0);
        e.p.memory.write_u32(set, 0)?;
        e.ret(0);
        Ok(())
    }),
    ("_sigfillset", |e| {
        let set = e.arg(0);
        e.p.memory.write_u32(set, u32::MAX)?;
        e.ret(0);
        Ok(())
    }),
    ("_sigaddset", |e| {
        let (set, sig) = (e.arg(0), e.arg(1) as u32);
        let v = e.p.memory.read_u32(set)? | 1u32.checked_shl(sig.wrapping_sub(1)).unwrap_or(0);
        e.p.memory.write_u32(set, v)?;
        e.ret(0);
        Ok(())
    }),
    ("_sigdelset", |e| {
        let (set, sig) = (e.arg(0), e.arg(1) as u32);
        let v = e.p.memory.read_u32(set)? & !1u32.checked_shl(sig.wrapping_sub(1)).unwrap_or(0);
        e.p.memory.write_u32(set, v)?;
        e.ret(0);
        Ok(())
    }),
    ("_sigprocmask", ret_zero),
    ("_sigaltstack", ret_zero),
    ("__tlv_bootstrap", tlv_get_addr),
    ("__tlv_atexit", noop),
    ("_dispatch_sync", dispatch_sync),
    ("_dispatch_barrier_sync", dispatch_barrier_sync),
    ("_dispatch_sync_f", |e| {
        let (queue, ctx, f) = (e.arg(0), e.arg(1), e.arg(2));
        sync_on(e, queue, f, ctx, false)
    }),
    ("_dispatch_barrier_sync_f", |e| {
        let (queue, ctx, f) = (e.arg(0), e.arg(1), e.arg(2));
        sync_on(e, queue, f, ctx, true)
    }),
    ("_dispatch_barrier_async_f", |e| {
        let (queue, ctx, f) = (e.arg(0), e.arg(1), e.arg(2));
        submit_work(e, queue, Work { f, ctx, group: None, barrier: true, turn: 0 })
    }),
    ("_dispatch_async", dispatch_async),
    ("_dispatch_barrier_async", dispatch_barrier_async),
    ("_dispatch_async_f", dispatch_async_f),
    ("_dispatch_after", dispatch_after),
    ("_dispatch_after_f", dispatch_after_f),
    ("__Unwind_RaiseException", unwind_raise_exception),
    // stdio locking (Swift's print): pomme's stdio needs none.
    ("_flockfile", noop),
    ("_funlockfile", noop),
    ("_dispatch_block_create", dispatch_block_create),
    ("_dispatch_block_wait", dispatch_block_wait),
    ("_dispatch_block_notify", dispatch_block_notify),
    // (flags, block): run it here and now.
    ("_dispatch_block_perform", |e| {
        let b = e.arg(1);
        call_block(e, b, &[])?;
        Ok(())
    }),
    // (flags, qos, relative priority, block): QoS doesn't matter here.
    ("_dispatch_block_create_with_qos_class", |e| {
        e.cpu.x[1] = e.cpu.x[3];
        dispatch_block_create(e)
    }),
    ("_dispatch_block_cancel", |e| {
        let b = e.arg(0);
        e.sys.cancelled.insert(b);
        Ok(())
    }),
    ("_dispatch_block_testcancel", |e| {
        let c = e.sys.cancelled.contains(&e.arg(0)) as u64;
        e.ret(c);
        Ok(())
    }),
    ("_dispatch_queue_create", dispatch_queue_create),
    // The target queue doesn't matter: all queues are serviced, in order, by the main loop.
    ("_dispatch_queue_create_with_target$V2", dispatch_queue_create_with_target),
    ("_dispatch_get_global_queue", dispatch_get_global_queue),
    ("_dispatch_queue_get_label", dispatch_queue_get_label),
    ("_dispatch_queue_set_specific", dispatch_queue_set_specific),
    ("_dispatch_get_specific", dispatch_get_specific),
    ("_dispatch_queue_get_specific", dispatch_queue_get_specific),
    ("_dispatch_queue_attr_make_with_qos_class", ret_self),
    // Autorelease pools around work items: pomme's queues keep the attribute's concurrency only.
    ("_dispatch_queue_attr_make_with_autorelease_frequency", ret_self),
    // Retargeting at the main queue moves the queue's work there; other targets don't change it here.
    ("_dispatch_set_target_queue", |e| {
        let (q, target) = (e.arg(0), e.arg(1));
        let main = main_queue(e);
        if let Some(dq) = e.sys.dqueues.get_mut(&q) {
            dq.on_main = target != 0 && target == main;
        }
        Ok(())
    }),
    ("_dispatch_semaphore_create", dispatch_semaphore_create),
    ("_dispatch_semaphore_signal", dispatch_semaphore_signal),
    ("_dispatch_group_create", dispatch_group_create),
    ("_dispatch_group_enter", |e| {
        let g = e.arg(0);
        e.sys.groups.entry(g).or_default().count += 1;
        Ok(())
    }),
    ("_dispatch_group_leave", |e| {
        let g = e.arg(0);
        group_leave(e, g)
    }),
    ("_dispatch_group_wait", dispatch_group_wait),
    ("_dispatch_group_async", dispatch_group_async),
    ("_dispatch_group_notify", dispatch_group_notify),
    ("_dispatch_release", noop),
    ("_dispatch_resume", dispatch_resume),
    ("_dispatch_suspend", dispatch_suspend),
    ("_dispatch_activate", dispatch_activate),
    ("_dispatch_set_context", dispatch_set_context),
    ("_dispatch_get_context", dispatch_get_context),
    ("_dispatch_data_create", dispatch_data_create),
    ("_dispatch_data_create_map", dispatch_data_create_map),
    ("_dispatch_data_create_subrange", dispatch_data_create_subrange),
    ("_dispatch_data_create_concat", dispatch_data_create_concat),
    ("_dispatch_data_apply", dispatch_data_apply),
    ("_dispatch_data_copy_region", dispatch_data_copy_region),
    ("_dispatch_data_get_size", |e| {
        let n = data_bytes_of(e, e.arg(0)).1;
        e.ret(n);
        Ok(())
    }),
    ("_dispatch_assert_queue$V2", noop),
    // Thread QoS and priorities: pomme's scheduler doesn't weigh them, so setting succeeds.
    ("_pthread_set_qos_class_self_np", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_pthread_attr_set_qos_class_np", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_pthread_attr_get_qos_class_np", |e| {
        let (cls, rel) = (e.arg(1), e.arg(2));
        if cls != 0 {
            e.p.memory.write_u32(cls, 0x21)?; // QOS_CLASS_USER_INTERACTIVE
        }
        if rel != 0 {
            e.p.memory.write_u32(rel, 0)?;
        }
        e.ret(0);
        Ok(())
    }),
    // Scheduling parameters: SCHED_OTHER (1) at Darwin's default priority 31.
    ("_pthread_getschedparam", |e| {
        let (policy, param) = (e.arg(1), e.arg(2));
        if policy != 0 {
            e.p.memory.write_u32(policy, 1)?;
        }
        if param != 0 {
            e.p.memory.write_u32(param, 31)?;
        }
        e.ret(0);
        Ok(())
    }),
    ("_pthread_setschedparam", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_pthread_attr_setschedparam", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_pthread_attr_setschedpolicy", |e| {
        e.ret(0);
        Ok(())
    }),
    ("_sched_get_priority_min", |e| {
        e.ret(15);
        Ok(())
    }),
    ("_sched_get_priority_max", |e| {
        e.ret(47);
        Ok(())
    }),
    ("_pthread_get_qos_class_np", |e| {
        let (cls, rel) = (e.arg(1), e.arg(2));
        if cls != 0 {
            e.p.memory.write_u32(cls, 0x21)?;
        }
        if rel != 0 {
            e.p.memory.write_u32(rel, 0)?;
        }
        e.ret(0);
        Ok(())
    }),
    ("_qos_class_self", |e| {
        e.ret(0x21); // QOS_CLASS_USER_INTERACTIVE
        Ok(())
    }),
    ("_voucher_adopt", ret_zero),
    ("_voucher_copy", ret_zero),
    ("___cxa_atexit", cxa_atexit),
    ("___chkstk_darwin", noop),
    ("___stack_chk_fail", stack_chk_fail),
    ("___assert_rtn", assert_rtn),
    ("__availability_version_check", availability_version_check),
    ("_os_system_version_get_current_version", os_system_version),
    ("__NSGetArgc", nsget_argc),
    ("__NSGetArgv", nsget_argv),
    ("__NSGetMachExecuteHeader", nsget_mach_header),
    ("_arc4random_uniform", arc4random_uniform),
    ("_arc4random_buf", arc4random_buf),
    ("_getentropy", getentropy),
    // SecRandomCopyBytes(rnd, count, bytes) / CCRandomGenerateBytes(bytes, count): the same
    // deterministic stream as arc4random_buf; both report success (0).
    ("_SecRandomCopyBytes", |e| {
        let (n, p) = (e.arg(1), e.arg(2));
        e.cpu.x[0] = p;
        e.cpu.x[1] = n;
        arc4random_buf(e)?;
        e.ret(0);
        Ok(())
    }),
    ("_CCRandomGenerateBytes", |e| {
        arc4random_buf(e)?;
        e.ret(0);
        Ok(())
    }),
    ("_posix_memalign", posix_memalign),
    ("_malloc_good_size", malloc_good_size),
    ("_dlsym", dlsym),
    ("_dlopen", dlopen),
    ("_dlerror", dlerror),
    ("_dlclose", ret_zero),
    ("_write", write_fd),
    ("_isatty", ret_zero),
    ("_os_log_create", ret_one),
    ("_os_signpost_enabled", ret_zero),
    ("_os_signpost_id_make_with_pointer", ret_one),
    ("__os_signpost_emit_with_name_impl", noop),
    ("_os_release", noop),
    ("_openlog", noop),
    // syslog(priority, format, ...): printed, like asl_log (Flutter's engine logs here).
    ("_syslog", |e| {
        let fmt = e.cstr(e.arg(1))?.into_bytes();
        let msg = crate::printf::format(e, &fmt, &mut crate::printf::Args::new(e.cpu.sp))?;
        println!("[syslog] {}", String::from_utf8_lossy(&msg).trim_end());
        Ok(())
    }),
    ("_vsyslog", |e| {
        let fmt = e.cstr(e.arg(1))?.into_bytes();
        let msg = crate::printf::format(e, &fmt, &mut crate::printf::Args::new(e.arg(2)))?;
        println!("[syslog] {}", String::from_utf8_lossy(&msg).trim_end());
        Ok(())
    }),
    ("_closelog", noop),
    ("__dyld_is_objc_constant", ret_zero),
];

// ---- locale (always the C locale) ----

fn newlocale(emu: &mut Emu) -> Result<()> {
    emu.ret(1); // any non-null locale_t; we only have "C"
    Ok(())
}

fn setlocale(emu: &mut Emu) -> Result<()> {
    let s = emu.alloc_cstr("C")?;
    emu.ret(s);
    Ok(())
}

/// `localeconv_l`: a static C-locale `struct lconv` ("." decimal point, empty grouping).
fn localeconv(emu: &mut Emu) -> Result<()> {
    if let Some(&p) = emu.sys.data.get("lconv") {
        emu.ret(p);
        return Ok(());
    }
    let p = emu.alloc(160)?;
    let dot = emu.alloc_cstr(".")?;
    let empty = emu.alloc_cstr("")?;
    for i in 0..10 {
        emu.write_u64(p + i * 8, if i == 0 { dot } else { empty })?;
    }
    // The char fields after the 10 pointers are CHAR_MAX ("not available").
    emu.p.memory.write(p + 80, &[0x7f; 14])?;
    emu.sys.data.insert("lconv", p);
    emu.ret(p);
    Ok(())
}

fn wcslen(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let mut n = 0;
    while emu.p.memory.read_u32(s + n * 4)? != 0 {
        n += 1;
    }
    emu.ret(n);
    Ok(())
}

fn arc4random(emu: &mut Emu) -> Result<()> {
    emu.cpu.x[0] = 1 << 32;
    arc4random_uniform(emu)
}

// ---- threads, locks, sleeping ----

fn ulock_wait(_: &mut Emu) -> Result<()> {
    Err(Stop::Unsupported("__ulock_wait: would block (lock held with no other thread to release it)".into()))
}


fn mach_port(emu: &mut Emu) -> Result<()> {
    emu.ret(0x103);
    Ok(())
}

/// `qsort(base, n, size, compar)`: insertion sort, calling the guest comparator.
/// Stable and deterministic; fine for the small arrays apps sort at startup.
fn qsort(emu: &mut Emu) -> Result<()> {
    let (base, n, size, cmp) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    let tmp = emu.alloc(size)?;
    for i in 1..n {
        let mut j = i;
        while j > 0 {
            let (a, b) = (base + (j - 1) * size, base + j * size);
            if emu.call(cmp, &[a, b])? as i32 <= 0 {
                break;
            }
            let x = bytes(emu, a, size)?;
            let y = bytes(emu, b, size)?;
            emu.p.memory.write(a, &y)?;
            emu.p.memory.write(b, &x)?;
            j -= 1;
        }
    }
    emu.heap.free(&emu.p.memory, tmp)?;
    Ok(())
}

/// `aligned_alloc(alignment, size)`: NULL (EINVAL) unless the alignment is a power of two.
fn aligned_alloc(emu: &mut Emu) -> Result<()> {
    let (align, size) = (emu.arg(0), emu.arg(1));
    if !align.is_power_of_two() {
        set_errno(emu, 22)?;
        emu.ret(0);
        return Ok(());
    }
    let p = emu.heap.alloc_aligned(&mut emu.p.memory, size, align)?;
    emu.ret(p);
    Ok(())
}

pub(crate) const MORE_FUNCS: &[(&str, HostFn)] = &[
    ("_newlocale", newlocale),
    ("_freelocale", ret_zero),
    // pomme formats numbers in the C locale regardless, so a thread's locale is
    // only a handle: uselocale returns the previous one (LC_GLOBAL_LOCALE = -1).
    ("_uselocale", |e| {
        let (new, old) = (e.arg(0), e.sys.thread_locale);
        if new != 0 {
            e.sys.thread_locale = new;
        }
        e.ret(if old == 0 { u64::MAX } else { old });
        Ok(())
    }),
    ("_duplocale", |e| {
        let l = e.arg(0);
        e.ret(l);
        Ok(())
    }),
    ("_setlocale", setlocale),
    ("_localeconv_l", localeconv),
    ("____mb_cur_max_l", ret_one),
    ("____tolower_l", tolower),
    ("____toupper_l", toupper),
    ("___maskrune_l", maskrune),
    ("_strtoll", strto_int),
    ("_strtoll_l", strto_int),
    ("_strtoull_l", strto_int),
    ("_strtod_l", strtod),
    ("_strtold", strtod),
    ("_strtold_l", strtod),
    ("_strtof", |e| {
        strtod(e)?;
        let v = f64::from_bits(e.cpu.q[0] as u64) as f32;
        e.cpu.q[0] = v.to_bits() as u128;
        Ok(())
    }),
    ("_strtof_l", |e| {
        strtod(e)?;
        let v = f64::from_bits(e.cpu.q[0] as u64) as f32;
        e.cpu.q[0] = v.to_bits() as u128;
        Ok(())
    }),
    ("_wcslen", wcslen),
    ("_arc4random", arc4random),
    ("___ulock_wait", ulock_wait),
    ("___ulock_wake", ret_zero),
    ("_mach_host_self", mach_port),
    ("_pthread_cond_init", ret_zero),
    ("_pthread_cond_destroy", ret_zero),
    ("_pthread_detach", ret_zero),
    ("_pthread_key_init_np", ret_zero),
    ("_close", ret_zero),
    ("_tzset", noop),
    ("_qsort", qsort),
    ("_aligned_alloc", aligned_alloc),
    ("_memset_s", |e| {
        // memset_s(dst, dstsize, c, n)
        let (dst, c, n) = (e.arg(0), e.arg(2) as u8, e.arg(3));
        e.p.memory.write(dst, &vec![c; n as usize])?;
        e.ret(0);
        Ok(())
    }),
];

// ---- multibyte/wide conversions for the C locale: one byte is one character ----

fn neg1(emu: &mut Emu) {
    emu.ret(u64::MAX);
}

/// `mbtowc_l(wchar_t *pwc, const char *s, size_t n, locale_t)`
fn mbtowc(emu: &mut Emu) -> Result<()> {
    let (pwc, s, n) = (emu.arg(0), emu.arg(1), emu.arg(2));
    if s == 0 {
        emu.ret(0);
        return Ok(());
    }
    if n == 0 {
        neg1(emu);
        return Ok(());
    }
    let c = emu.p.memory.read_u8(s)?;
    if pwc != 0 {
        emu.p.memory.write_u32(pwc, c as u32)?;
    }
    emu.ret((c != 0) as u64);
    Ok(())
}

/// `mbrtowc_l(pwc, s, n, mbstate_t *ps, locale_t)`
fn mbrtowc(emu: &mut Emu) -> Result<()> {
    mbtowc(emu)
}

/// `mbrlen_l(s, n, ps, locale_t)`
fn mbrlen(emu: &mut Emu) -> Result<()> {
    let (s, n) = (emu.arg(0), emu.arg(1));
    if s == 0 {
        emu.ret(0);
    } else if n == 0 {
        emu.ret(u64::MAX - 1); // (size_t)-2: incomplete
    } else {
        let c = emu.p.memory.read_u8(s)?;
        emu.ret((c != 0) as u64);
    }
    Ok(())
}

/// `wcrtomb_l(char *s, wchar_t wc, ps, locale_t)`
fn wcrtomb(emu: &mut Emu) -> Result<()> {
    let (s, wc) = (emu.arg(0), emu.arg(1) as u32);
    if s == 0 {
        emu.ret(1);
        return Ok(());
    }
    if wc > 0xff {
        neg1(emu);
        return Ok(());
    }
    emu.p.memory.write_u8(s, wc as u8)?;
    emu.ret(1);
    Ok(())
}

fn btowc(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0) as i32;
    emu.ret(if c < 0 { u32::MAX as u64 } else { (c & 0xff) as u64 });
    Ok(())
}

fn wctob(emu: &mut Emu) -> Result<()> {
    let wc = emu.arg(0) as u32;
    emu.ret(if wc <= 0xff { wc as u64 } else { u64::MAX });
    Ok(())
}

/// Shared body of `mbsrtowcs_l` / `mbsnrtowcs_l`: bytes → wchar_t, stop at NUL.
fn mbs_to_wcs(emu: &mut Emu, dst: u64, src_pp: u64, max_in: u64, len: u64) -> Result<()> {
    let mut src = emu.read_u64(src_pp)?;
    let mut n = 0;
    while n < len || dst == 0 {
        if n >= max_in {
            break;
        }
        let c = emu.p.memory.read_u8(src)?;
        if c == 0 {
            if dst != 0 {
                emu.p.memory.write_u32(dst + n * 4, 0)?;
                emu.write_u64(src_pp, 0)?;
            }
            emu.ret(n);
            return Ok(());
        }
        if dst != 0 {
            emu.p.memory.write_u32(dst + n * 4, c as u32)?;
        }
        src += 1;
        n += 1;
    }
    if dst != 0 {
        emu.write_u64(src_pp, src)?;
    }
    emu.ret(n);
    Ok(())
}

fn mbsrtowcs(emu: &mut Emu) -> Result<()> {
    mbs_to_wcs(emu, emu.arg(0), emu.arg(1), u64::MAX, emu.arg(2))
}

fn mbsnrtowcs(emu: &mut Emu) -> Result<()> {
    mbs_to_wcs(emu, emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3))
}

/// `wcsnrtombs_l(dst, const wchar_t **src, nwc, len, ps, locale_t)`
fn wcsnrtombs(emu: &mut Emu) -> Result<()> {
    let (dst, src_pp, nwc, len) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    let mut src = emu.read_u64(src_pp)?;
    let mut n = 0;
    while (n < len || dst == 0) && n < nwc {
        let wc = emu.p.memory.read_u32(src)?;
        if wc > 0xff {
            neg1(emu);
            return Ok(());
        }
        if wc == 0 {
            if dst != 0 {
                emu.p.memory.write_u8(dst + n, 0)?;
                emu.write_u64(src_pp, 0)?;
            }
            emu.ret(n);
            return Ok(());
        }
        if dst != 0 {
            emu.p.memory.write_u8(dst + n, wc as u8)?;
        }
        src += 4;
        n += 1;
    }
    if dst != 0 {
        emu.write_u64(src_pp, src)?;
    }
    emu.ret(n);
    Ok(())
}

pub(crate) const WIDE_FUNCS: &[(&str, HostFn)] = &[
    ("_mbtowc_l", mbtowc),
    ("_mbrtowc_l", mbrtowc),
    ("_mbrlen_l", mbrlen),
    ("_wcrtomb_l", wcrtomb),
    ("_btowc_l", btowc),
    ("_wctob_l", wctob),
    ("_mbsrtowcs_l", mbsrtowcs),
    ("_mbsnrtowcs_l", mbsnrtowcs),
    ("_wcsnrtombs_l", wcsnrtombs),
];

// ---- dyld queries ----

fn image_by_header(emu: &Emu, mh: u64) -> Option<usize> {
    emu.p.images.iter().position(|i| i.header == mh)
}

/// `getsectiondata(mh, segname, sectname, unsigned long *size)`
fn getsectiondata(emu: &mut Emu) -> Result<()> {
    let (mh, seg, sect, size_out) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    let seg = emu.cstr(seg)?;
    let sect = emu.cstr(sect)?;
    let found = image_by_header(emu, mh).and_then(|i| {
        let img = &emu.p.images[i];
        img.sections.iter().find(|(s, sec)| *s == seg && sec.name == sect).map(|(_, sec)| (sec.addr, sec.size))
    });
    let (addr, size) = found.unwrap_or((0, 0));
    if size_out != 0 {
        emu.write_u64(size_out, size)?;
    }
    emu.ret(addr);
    Ok(())
}

fn dyld_image_count(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.p.images.len() as u64);
    Ok(())
}

fn dyld_get_image_header(emu: &mut Emu) -> Result<()> {
    let h = emu.p.images.get(emu.arg(0) as usize).map_or(0, |i| i.header);
    emu.ret(h);
    Ok(())
}

fn dyld_get_image_slide(emu: &mut Emu) -> Result<()> {
    let s = emu.p.images.get(emu.arg(0) as usize).map_or(0, |i| i.slide);
    emu.ret(s);
    Ok(())
}

/// Image `i`'s path as a C string (made once per image).
fn image_path(emu: &mut Emu, i: usize) -> Result<u64> {
    if let Some(&p) = emu.sys.image_paths.get(&i) {
        return Ok(p);
    }
    let path = crate::fs::guest_image_path(emu, i);
    let p = emu.alloc_cstr(&path)?;
    emu.sys.image_paths.insert(i, p);
    Ok(p)
}

fn dyld_get_image_name(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(0) as usize;
    let p = if i < emu.p.images.len() { image_path(emu, i)? } else { 0 };
    emu.ret(p);
    Ok(())
}

/// `_dyld_register_func_for_add_image(void (*)(const mach_header *, intptr_t slide))`
fn dyld_register_add_image(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    let images: Vec<(u64, u64)> = emu.p.images.iter().map(|i| (i.header, i.slide)).collect();
    for (h, slide) in images {
        emu.call(f, &[h, slide])?;
    }
    Ok(())
}

/// `dladdr(addr, Dl_info *)`: {dli_fname, dli_fbase, dli_sname, dli_saddr}
fn dladdr(emu: &mut Emu) -> Result<()> {
    let (addr, info) = (emu.arg(0), emu.arg(1));
    let Some(i) = emu.p.images.iter().position(|img| addr >= img.start && addr < img.end) else {
        emu.ret(0);
        return Ok(());
    };
    let name = image_path(emu, i)?;
    let header = emu.p.images[i].header;
    emu.write_u64(info, name)?;
    emu.write_u64(info + 8, header)?;
    emu.write_u64(info + 16, 0)?;
    emu.write_u64(info + 24, 0)?;
    emu.ret(1);
    Ok(())
}

pub(crate) const DYLD_FUNCS: &[(&str, HostFn)] = &[
    ("_getsectiondata", getsectiondata),
    ("__dyld_image_count", dyld_image_count),
    ("__dyld_find_unwind_sections", dyld_find_unwind_sections),
    // Images are never unloaded.
    ("__dyld_register_func_for_remove_image", noop),
    ("__dyld_get_image_header", dyld_get_image_header),
    ("__dyld_get_image_vmaddr_slide", dyld_get_image_slide),
    ("__dyld_get_image_name", dyld_get_image_name),
    ("__dyld_register_func_for_add_image", dyld_register_add_image),
    ("_dladdr", dladdr),
];

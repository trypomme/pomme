//! The rest of libSystem: string and memory helpers, libm, time, pthread,
//! thread-local variables, dispatch, logging, and libSystem's data symbols.
//!
//! Everything is single-threaded and deterministic:
//! - time comes from the instruction counter (24 MHz ticks, like CNTVCT);
//! - locks always succeed; there is one thread;
//! - `dispatch_async` work is queued and drained by the run loop, in order.

use crate::{Emu, HostFn, Result, Stop};
use std::collections::{HashMap, VecDeque};

/// Wall-clock time at boot: 2026-01-01T00:00:00Z. Guest time = this + steps / 24 MHz.
pub const EPOCH_BASE_SECS: u64 = 1_767_225_600;
pub const TICKS_PER_SEC: u64 = 24_000_000;

#[derive(Default)]
pub struct State {
    errno: u64,
    /// Per-image thread-local storage blocks.
    tlv_blocks: HashMap<(usize, usize), u64>,
    /// Queued async work: (function, context). Blocks are queued as (invoke, block).
    pub queue: VecDeque<(u64, u64)>,
    queue_specific: HashMap<(u64, u64), u64>,
    data: HashMap<&'static str, u64>,
    pub next_port: u64,
    /// Seeded random state (arc4random, UUIDs): the same every run.
    rng: u64,
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
    ("__dispatch_queue_attr_concurrent", 64),
    ("__dispatch_source_type_timer", 64),
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
    let mut out = Vec::new();
    let mut a = addr;
    loop {
        let b = emu.p.memory.read_u8(a)?;
        if b == 0 {
            return Ok(out);
        }
        out.push(b);
        a += 1;
    }
}

/// Call a block with arguments after the block pointer itself.
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
    emu.ret(bits & mask);
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
    EPOCH_BASE_SECS * 1_000_000_000 + now_ns(emu)
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

fn getenv(emu: &mut Emu) -> Result<()> {
    emu.ret(0); // empty environment
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

fn errno_location(emu: &mut Emu) -> Result<()> {
    if emu.sys.errno == 0 {
        emu.sys.errno = emu.alloc(8)?;
    }
    emu.ret(emu.sys.errno);
    Ok(())
}

fn getpid(emu: &mut Emu) -> Result<()> {
    emu.ret(1000);
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
    let v = match emu.arg(0) {
        29 => pomme_mem::PAGE_SIZE, // _SC_PAGESIZE
        57 | 58 => 1,               // _SC_NPROCESSORS_CONF / _ONLN
        _ => u64::MAX,              // -1: unknown
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

/// `dispatch_once(predicate*, block)`
fn dispatch_once(emu: &mut Emu) -> Result<()> {
    let (pred, block) = (emu.arg(0), emu.arg(1));
    if emu.read_u64(pred)? != u64::MAX {
        emu.write_u64(pred, u64::MAX)?;
        call_block(emu, block, &[])?;
    }
    Ok(())
}

/// `dispatch_once_f(predicate*, context, function)`
fn dispatch_once_f(emu: &mut Emu) -> Result<()> {
    let (pred, ctx, f) = (emu.arg(0), emu.arg(1), emu.arg(2));
    if emu.read_u64(pred)? != u64::MAX {
        emu.write_u64(pred, u64::MAX)?;
        emu.call(f, &[ctx])?;
    }
    Ok(())
}

/// `dispatch_sync(queue, block)`: run now, on the one thread.
fn dispatch_sync(emu: &mut Emu) -> Result<()> {
    call_block(emu, emu.arg(1), &[])?;
    Ok(())
}

fn dispatch_async(emu: &mut Emu) -> Result<()> {
    let block = emu.arg(1);
    emu.cpu.x[0] = block;
    crate::libc::block_copy(emu)?;
    let block = emu.cpu.x[0];
    let invoke = emu.read_u64(block + 16)?;
    emu.sys.queue.push_back((invoke, block));
    crate::threads::wake_main_queue(emu);
    Ok(())
}

fn dispatch_async_f(emu: &mut Emu) -> Result<()> {
    emu.sys.queue.push_back((emu.arg(2), emu.arg(1)));
    crate::threads::wake_main_queue(emu);
    Ok(())
}

/// Run queued async work until the queue is empty.
pub fn drain_queue(emu: &mut Emu) -> Result<usize> {
    let mut n = 0;
    while let Some((f, arg)) = emu.sys.queue.pop_front() {
        emu.call(f, &[arg])?;
        n += 1;
    }
    Ok(n)
}

fn dispatch_queue_create(emu: &mut Emu) -> Result<()> {
    let cls = crate::objc::host_class(emu, "OS_dispatch_queue")?;
    let q = emu.alloc(64)?;
    emu.write_u64(q, cls)?;
    let label = emu.arg(0);
    let label = if label == 0 { 0 } else { emu.alloc_cstr(&emu.cstr(label)?)? };
    emu.write_u64(q + 8, label)?;
    emu.ret(q);
    Ok(())
}

fn dispatch_get_global_queue(emu: &mut Emu) -> Result<()> {
    emu.ret(emu.sys.data.get("__dispatch_main_q").copied().unwrap_or(0));
    Ok(())
}

fn dispatch_queue_get_label(emu: &mut Emu) -> Result<()> {
    let q = emu.arg(0);
    let l = if q == 0 { 0 } else { emu.read_u64(q + 8)? };
    let l = if l == 0 { emu.alloc_cstr("com.apple.main-thread")? } else { l };
    emu.ret(l);
    Ok(())
}

fn dispatch_queue_set_specific(emu: &mut Emu) -> Result<()> {
    emu.sys.queue_specific.insert((emu.arg(0), emu.arg(1)), emu.arg(2));
    Ok(())
}

fn dispatch_get_specific(emu: &mut Emu) -> Result<()> {
    // We're always "on" every queue's single thread; look the key up anywhere.
    let key = emu.arg(0);
    let v = emu.sys.queue_specific.iter().find(|((_, k), _)| *k == key).map_or(0, |(_, v)| *v);
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

fn dispatch_group_wait(emu: &mut Emu) -> Result<()> {
    drain_queue(emu)?;
    emu.ret(0);
    Ok(())
}


/// `dispatch_after(when, queue, block)`: no timers yet; queue it.
fn dispatch_after(emu: &mut Emu) -> Result<()> {
    emu.cpu.x[1] = emu.cpu.x[2];
    dispatch_async(emu)
}

fn dispatch_after_f(emu: &mut Emu) -> Result<()> {
    emu.sys.queue.push_back((emu.arg(3), emu.arg(2)));
    Ok(())
}

// ---- C++ ABI bits that live in libSystem ----

fn cxa_atexit(emu: &mut Emu) -> Result<()> {
    emu.ret(0); // destructors at exit: never run
    Ok(())
}

fn availability_version_check(emu: &mut Emu) -> Result<()> {
    emu.ret(1); // we claim to be the newest iOS
    Ok(())
}

fn os_system_version(emu: &mut Emu) -> Result<()> {
    // Returns a struct { major, minor, patch } of i64 in x0..x2.
    emu.cpu.x[0] = 26;
    emu.cpu.x[1] = 0;
    emu.cpu.x[2] = 0;
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

fn posix_memalign(emu: &mut Emu) -> Result<()> {
    let (out, align, size) = (emu.arg(0), emu.arg(1), emu.arg(2));
    if align > 16 {
        return Err(Stop::Unsupported(format!("posix_memalign alignment {align}")));
    }
    let p = emu.alloc(size)?;
    emu.write_u64(out, p)?;
    emu.ret(0);
    Ok(())
}

fn malloc_good_size(_: &mut Emu) -> Result<()> {
    Ok(())
}

fn dlsym(emu: &mut Emu) -> Result<()> {
    // Search loaded images' exports.
    let name = format!("_{}", emu.cstr(emu.arg(1))?);
    let addr = emu.p.images.iter().find_map(|i| i.symbol(&name)).unwrap_or(0);
    emu.ret(addr);
    Ok(())
}

fn write_fd(emu: &mut Emu) -> Result<()> {
    let (fd, buf, n) = (emu.arg(0), emu.arg(1), emu.arg(2));
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
    ("___memcpy_chk", crate::libc::memmove),
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
    ("___error", errno_location),
    ("_getpid", getpid),
    ("_getpagesize", pagesize),
    ("_host_page_size", host_page_size),
    ("_sysconf", sysconf),
    ("_pthread_equal", pthread_equal),
    ("_pthread_mutex_init", ret_zero),
    ("_pthread_mutex_destroy", ret_zero),
    ("_pthread_mutex_lock", ret_zero),
    ("_pthread_mutex_unlock", ret_zero),
    ("_pthread_mutexattr_init", ret_zero),
    ("_pthread_mutexattr_destroy", ret_zero),
    ("_pthread_mutexattr_settype", ret_zero),
    ("_pthread_rwlock_init", ret_zero),
    ("_pthread_rwlock_destroy", ret_zero),
    ("_pthread_rwlock_rdlock", ret_zero),
    ("_pthread_rwlock_wrlock", ret_zero),
    ("_pthread_rwlock_unlock", ret_zero),
    ("_pthread_attr_init", ret_zero),
    ("_pthread_attr_setstacksize", ret_zero),
    ("_pthread_sigmask", ret_zero),
    ("_os_unfair_lock_lock", noop),
    ("_os_unfair_lock_unlock", noop),
    ("_os_unfair_recursive_lock_lock_with_options", noop),
    ("_os_unfair_recursive_lock_unlock", noop),
    ("__tlv_bootstrap", tlv_get_addr),
    ("__tlv_atexit", noop),
    ("_dispatch_once", dispatch_once),
    ("_dispatch_once_f", dispatch_once_f),
    ("_dispatch_sync", dispatch_sync),
    ("_dispatch_barrier_sync", dispatch_sync),
    ("_dispatch_async", dispatch_async),
    ("_dispatch_barrier_async", dispatch_async),
    ("_dispatch_async_f", dispatch_async_f),
    ("_dispatch_after", dispatch_after),
    ("_dispatch_after_f", dispatch_after_f),
    ("_dispatch_queue_create", dispatch_queue_create),
    // The target queue doesn't matter: all queues are serviced, in order, by the main loop.
    ("_dispatch_queue_create_with_target$V2", dispatch_queue_create),
    ("_dispatch_get_global_queue", dispatch_get_global_queue),
    ("_dispatch_queue_get_label", dispatch_queue_get_label),
    ("_dispatch_queue_set_specific", dispatch_queue_set_specific),
    ("_dispatch_get_specific", dispatch_get_specific),
    ("_dispatch_queue_attr_make_with_qos_class", ret_self),
    ("_dispatch_set_target_queue", noop),
    ("_dispatch_semaphore_create", dispatch_semaphore_create),
    ("_dispatch_semaphore_signal", dispatch_semaphore_signal),
    ("_dispatch_group_create", dispatch_group_create),
    ("_dispatch_group_enter", noop),
    ("_dispatch_group_leave", noop),
    ("_dispatch_group_wait", dispatch_group_wait),
    ("_dispatch_release", noop),
    ("_dispatch_activate", noop),
    ("_dispatch_set_context", noop),
    ("_dispatch_assert_queue$V2", noop),
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
    ("_posix_memalign", posix_memalign),
    ("_malloc_good_size", malloc_good_size),
    ("_dlsym", dlsym),
    ("_dlopen", ret_zero),
    ("_dlclose", ret_zero),
    ("_write", write_fd),
    ("_isatty", ret_zero),
    ("_os_log_create", ret_one),
    ("_os_log_type_enabled", ret_zero),
    ("__os_log_impl", noop),
    ("_os_signpost_enabled", ret_zero),
    ("_os_signpost_id_make_with_pointer", ret_one),
    ("__os_signpost_emit_with_name_impl", noop),
    ("_os_release", noop),
    ("_openlog", noop),
    ("_syslog", noop),
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

fn aligned_alloc(emu: &mut Emu) -> Result<()> {
    let (align, size) = (emu.arg(0), emu.arg(1));
    if align > 16 {
        return Err(Stop::Unsupported(format!("aligned_alloc alignment {align}")));
    }
    let p = emu.alloc(size)?;
    emu.ret(p);
    Ok(())
}

pub(crate) const MORE_FUNCS: &[(&str, HostFn)] = &[
    ("_newlocale", newlocale),
    ("_freelocale", ret_zero),
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
    ("_pthread_mutex_trylock", ret_zero),
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

fn image_path(emu: &mut Emu, i: usize) -> Result<u64> {
    let img = &emu.p.images[i];
    let path = img.install_name.clone().unwrap_or_else(|| format!("/private/var/containers/Bundle/Application/pomme/{}", img.name));
    emu.alloc_cstr(&path)
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
    ("__dyld_get_image_header", dyld_get_image_header),
    ("__dyld_get_image_vmaddr_slide", dyld_get_image_slide),
    ("__dyld_get_image_name", dyld_get_image_name),
    ("__dyld_register_func_for_add_image", dyld_register_add_image),
    ("_dladdr", dladdr),
];

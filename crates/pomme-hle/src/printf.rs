//! printf-family formatting.
//!
//! On Apple arm64, every variadic argument is passed on the stack in its own
//! 8-byte slot (floats promoted to double), and a `va_list` is just a pointer
//! to the next slot. So both `snprintf(…, ...)` and `vsnprintf(…, va)` read
//! arguments the same way: from a pointer that advances by 8.

use crate::{Emu, HostFn, Result};

pub struct Args {
    next: u64,
}

impl Args {
    pub fn new(at: u64) -> Self {
        Args { next: at }
    }

    /// The next argument as a pointer (scanf's outputs).
    pub(crate) fn ptr(&mut self, emu: &Emu) -> Result<u64> {
        self.u64(emu)
    }

    fn u64(&mut self, emu: &Emu) -> Result<u64> {
        let v = emu.read_u64(self.next)?;
        self.next += 8;
        Ok(v)
    }
}

/// Format a C format string. `%@` formats an Objective-C object's description.
pub fn format(emu: &mut Emu, fmt: &[u8], args: &mut Args) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < fmt.len() {
        let c = fmt[i];
        i += 1;
        if c != b'%' {
            out.push(c);
            continue;
        }
        // %[flags][width][.precision][length]conv
        let mut left = false;
        let mut zero = false;
        let mut plus = false;
        let mut space = false;
        let mut alt = false;
        while i < fmt.len() {
            match fmt[i] {
                b'-' => left = true,
                b'0' => zero = true,
                b'+' => plus = true,
                b' ' => space = true,
                b'#' => alt = true,
                b'\'' => {}
                _ => break,
            }
            i += 1;
        }
        let mut width = 0usize;
        if fmt.get(i) == Some(&b'*') {
            let w = args.u64(emu)? as i32;
            if w < 0 {
                left = true;
            }
            width = w.unsigned_abs() as usize;
            i += 1;
        }
        while i < fmt.len() && fmt[i].is_ascii_digit() {
            width = width * 10 + (fmt[i] - b'0') as usize;
            i += 1;
        }
        let mut precision: Option<usize> = None;
        if fmt.get(i) == Some(&b'.') {
            i += 1;
            let mut p = 0usize;
            if fmt.get(i) == Some(&b'*') {
                let v = args.u64(emu)? as i32;
                p = v.max(0) as usize;
                i += 1;
            }
            while i < fmt.len() && fmt[i].is_ascii_digit() {
                p = p * 10 + (fmt[i] - b'0') as usize;
                i += 1;
            }
            precision = Some(p);
        }
        // Length: track only the operand width that matters for truncation.
        let mut bits = 32;
        while i < fmt.len() {
            match fmt[i] {
                b'l' | b'q' | b'j' | b'z' | b't' | b'L' => bits = 64,
                b'h' => bits = if bits == 16 { 8 } else { 16 },
                _ => break,
            }
            i += 1;
        }
        let Some(&conv) = fmt.get(i) else { break };
        i += 1;

        let body: Vec<u8> = match conv {
            b'%' => b"%".to_vec(),
            b'd' | b'i' => {
                let raw = args.u64(emu)?;
                let v = match bits {
                    8 => raw as i8 as i64,
                    16 => raw as i16 as i64,
                    32 => raw as i32 as i64,
                    _ => raw as i64,
                };
                let mut digits = v.unsigned_abs().to_string();
                if let Some(p) = precision {
                    while digits.len() < p {
                        digits.insert(0, '0');
                    }
                }
                let sign = if v < 0 {
                    "-"
                } else if plus {
                    "+"
                } else if space {
                    " "
                } else {
                    ""
                };
                pad_number(sign, &digits, width, left, zero && precision.is_none())
            }
            b'u' | b'x' | b'X' | b'o' => {
                let raw = args.u64(emu)?;
                let v = match bits {
                    8 => raw as u8 as u64,
                    16 => raw as u16 as u64,
                    32 => raw as u32 as u64,
                    _ => raw,
                };
                let mut digits = match conv {
                    b'u' => v.to_string(),
                    b'x' => format!("{v:x}"),
                    b'X' => format!("{v:X}"),
                    _ => format!("{v:o}"),
                };
                if let Some(p) = precision {
                    while digits.len() < p {
                        digits.insert(0, '0');
                    }
                }
                let prefix = match (alt && v != 0, conv) {
                    (true, b'x') => "0x",
                    (true, b'X') => "0X",
                    (true, b'o') => "0",
                    _ => "",
                };
                pad_number(prefix, &digits, width, left, zero && precision.is_none())
            }
            b'p' => {
                let v = args.u64(emu)?;
                pad_number("0x", &format!("{v:x}"), width, left, false)
            }
            b'c' => {
                let v = args.u64(emu)? as u8;
                pad(vec![v], width, left)
            }
            b's' => {
                let p = args.u64(emu)?;
                let mut s = if p == 0 { b"(null)".to_vec() } else { emu.cstr(p)?.into_bytes() };
                if let Some(n) = precision {
                    s.truncate(n);
                }
                pad(s, width, left)
            }
            b'@' => {
                let obj = args.u64(emu)?;
                let s = if obj == 0 {
                    "(null)".to_string()
                } else {
                    let d = crate::objc::send(emu, obj, "description", &[])?;
                    crate::foundation::nsstring(emu, d)?
                };
                pad(s.into_bytes(), width, left)
            }
            b'f' | b'F' | b'e' | b'E' | b'g' | b'G' | b'a' | b'A' => {
                let v = f64::from_bits(args.u64(emu)?);
                let p = precision.unwrap_or(6);
                let mut s = match conv {
                    b'f' | b'F' => format!("{v:.p$}"),
                    b'e' | b'E' => c_exp(v, p),
                    b'g' | b'G' => c_general(v, p, alt),
                    _ => format!("{v:e}"),
                };
                if conv.is_ascii_uppercase() {
                    s = s.to_uppercase();
                }
                if v.is_infinite() {
                    s = if v > 0.0 { "inf".into() } else { "-inf".into() };
                } else if v.is_nan() {
                    s = "nan".into();
                }
                let (sign, digits) = match s.strip_prefix('-') {
                    Some(d) => ("-", d.to_string()),
                    None if plus => ("+", s),
                    None if space => (" ", s),
                    None => ("", s),
                };
                pad_number(sign, &digits, width, left, zero && v.is_finite())
            }
            b'n' => {
                let p = args.u64(emu)?;
                emu.p.memory.write_u32(p, out.len() as u32)?;
                Vec::new()
            }
            other => vec![b'%', other],
        };
        out.extend(body);
    }
    Ok(out)
}

fn pad(s: Vec<u8>, width: usize, left: bool) -> Vec<u8> {
    if s.len() >= width {
        return s;
    }
    let fill = vec![b' '; width - s.len()];
    if left { [s, fill].concat() } else { [fill, s].concat() }
}

fn pad_number(prefix: &str, digits: &str, width: usize, left: bool, zero: bool) -> Vec<u8> {
    let len = prefix.len() + digits.len();
    if zero && !left && len < width {
        format!("{prefix}{}{digits}", "0".repeat(width - len)).into_bytes()
    } else {
        pad(format!("{prefix}{digits}").into_bytes(), width, left)
    }
}

/// `%e`: Rust prints `1.5e3`; C wants `1.500000e+03`.
fn c_exp(v: f64, p: usize) -> String {
    let s = format!("{v:.p$e}");
    let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
    let e: i32 = e.parse().unwrap_or(0);
    format!("{m}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}

/// `%g`: shortest of %e / %f with `p` significant digits, trailing zeros removed.
fn c_general(v: f64, p: usize, alt: bool) -> String {
    let p = p.max(1);
    if v == 0.0 {
        return "0".into();
    }
    let exp = v.abs().log10().floor() as i32;
    let s = if exp < -4 || exp >= p as i32 {
        c_exp(v, p - 1)
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        format!("{v:.decimals$}")
    };
    if alt {
        return s;
    }
    // Strip trailing zeros in the mantissa.
    let (m, e) = match s.find('e') {
        Some(i) => (&s[..i], &s[i..]),
        None => (&s[..], ""),
    };
    let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
    format!("{m}{e}")
}

/// Write `s` into a `size`-byte C buffer (truncating, NUL-terminating); return the full length.
fn store(emu: &mut Emu, buf: u64, size: u64, s: &[u8]) -> Result<()> {
    if size > 0 && buf != 0 {
        let n = s.len().min(size as usize - 1);
        emu.p.memory.write(buf, &s[..n])?;
        emu.p.memory.write_u8(buf + n as u64, 0)?;
    }
    emu.ret(s.len() as u64);
    Ok(())
}

fn fmt_at(emu: &mut Emu, fmt_reg: usize, va: u64) -> Result<Vec<u8>> {
    let fmt = emu.cstr(emu.arg(fmt_reg))?.into_bytes();
    format(emu, &fmt, &mut Args::new(va))
}

fn snprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 2, emu.cpu.sp)?;
    store(emu, emu.arg(0), emu.arg(1), &s)
}

fn snprintf_l(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 3, emu.cpu.sp)?;
    store(emu, emu.arg(0), emu.arg(1), &s)
}

fn vsnprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 2, emu.arg(3))?;
    store(emu, emu.arg(0), emu.arg(1), &s)
}

fn sprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 1, emu.cpu.sp)?;
    store(emu, emu.arg(0), u64::MAX, &s)
}

fn vsprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 1, emu.arg(2))?;
    store(emu, emu.arg(0), u64::MAX, &s)
}

/// The fortified (`_FORTIFY_SOURCE`) forms the compiler emits for buffers of
/// known size `slen`: as their plain versions, but a write that could pass
/// the buffer's end aborts, as libc's `__chk_fail` does.
fn chk_fail() -> Result<()> {
    Err(crate::Stop::Unsupported("buffer overflow detected (__chk_fail): a fortified printf was given more room than its buffer has".into()))
}

/// `__snprintf_chk(s, maxlen, flag, slen, fmt, ...)`
fn snprintf_chk(emu: &mut Emu) -> Result<()> {
    if emu.arg(1) > emu.arg(3) {
        return chk_fail();
    }
    let s = fmt_at(emu, 4, emu.cpu.sp)?;
    store(emu, emu.arg(0), emu.arg(1), &s)
}

/// `__vsnprintf_chk(s, maxlen, flag, slen, fmt, va_list)`
fn vsnprintf_chk(emu: &mut Emu) -> Result<()> {
    if emu.arg(1) > emu.arg(3) {
        return chk_fail();
    }
    let s = fmt_at(emu, 4, emu.arg(5))?;
    store(emu, emu.arg(0), emu.arg(1), &s)
}

/// `__sprintf_chk(s, flag, slen, fmt, ...)`
fn sprintf_chk(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 3, emu.cpu.sp)?;
    if s.len() as u64 >= emu.arg(2) {
        return chk_fail();
    }
    store(emu, emu.arg(0), u64::MAX, &s)
}

/// `__vsprintf_chk(s, flag, slen, fmt, va_list)`
fn vsprintf_chk(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 3, emu.arg(4))?;
    if s.len() as u64 >= emu.arg(2) {
        return chk_fail();
    }
    store(emu, emu.arg(0), u64::MAX, &s)
}

fn alloc_result(emu: &mut Emu, out: u64, s: &[u8]) -> Result<()> {
    let p = emu.alloc(s.len() as u64 + 1)?;
    emu.p.memory.write(p, s)?;
    emu.write_u64(out, p)?;
    emu.ret(s.len() as u64);
    Ok(())
}

fn asprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 1, emu.cpu.sp)?;
    alloc_result(emu, emu.arg(0), &s)
}

fn asprintf_l(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 2, emu.cpu.sp)?;
    alloc_result(emu, emu.arg(0), &s)
}

fn vasprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 1, emu.arg(2))?;
    alloc_result(emu, emu.arg(0), &s)
}

/// Guest stdout/stderr go to the host's, prefixed so they stand out.
/// The app's console output (stdout and stderr): printed, and kept in `emu.console`.
pub fn guest_print(emu: &mut Emu, s: &[u8]) {
    let text = String::from_utf8_lossy(s);
    print!("{text}");
    emu.console.push_str(&text);
}

fn printf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 0, emu.cpu.sp)?;
    guest_print(emu, &s);
    emu.ret(s.len() as u64);
    Ok(())
}

fn vprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 0, emu.arg(1))?;
    guest_print(emu, &s);
    emu.ret(s.len() as u64);
    Ok(())
}

fn fprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 1, emu.cpu.sp)?;
    guest_print(emu, &s);
    emu.ret(s.len() as u64);
    Ok(())
}

fn vfprintf(emu: &mut Emu) -> Result<()> {
    let s = fmt_at(emu, 1, emu.arg(2))?;
    guest_print(emu, &s);
    emu.ret(s.len() as u64);
    Ok(())
}

fn puts(emu: &mut Emu) -> Result<()> {
    let s = emu.cstr(emu.arg(0))?;
    guest_print(emu, format!("{s}\n").as_bytes());
    emu.ret(1);
    Ok(())
}

fn fputs(emu: &mut Emu) -> Result<()> {
    let s = emu.cstr(emu.arg(0))?;
    guest_print(emu, s.as_bytes());
    emu.ret(1);
    Ok(())
}

fn putchar(emu: &mut Emu) -> Result<()> {
    guest_print(emu, &[emu.arg(0) as u8]);
    Ok(())
}

fn fputc(emu: &mut Emu) -> Result<()> {
    guest_print(emu, &[emu.arg(0) as u8]);
    Ok(())
}

fn fwrite(emu: &mut Emu) -> Result<()> {
    if let Some(r) = crate::fs::fwrite_stream(emu) {
        return r;
    }
    let (p, size, n) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let b = emu.p.memory.read(p, (size * n) as usize)?.to_vec();
    guest_print(emu, &b);
    emu.ret(n);
    Ok(())
}

fn fflush(emu: &mut Emu) -> Result<()> {
    emu.ret(0);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_snprintf", snprintf),
    ("___snprintf_chk", snprintf_chk),
    ("___vsnprintf_chk", vsnprintf_chk),
    ("___sprintf_chk", sprintf_chk),
    ("___vsprintf_chk", vsprintf_chk),
    ("_snprintf_l", snprintf_l),
    ("_vsnprintf", vsnprintf),
    ("_sprintf", sprintf),
    ("_vsprintf", vsprintf),
    ("_asprintf", asprintf),
    ("_asprintf_l", asprintf_l),
    ("_vasprintf", vasprintf),
    ("_printf", printf),
    ("_vprintf", vprintf),
    ("_fprintf", fprintf),
    ("_vfprintf", vfprintf),
    ("_puts", puts),
    ("_fputs", fputs),
    ("_putchar", putchar),
    ("_putc", fputc),
    ("_fputc", fputc),
    ("_fwrite", fwrite),
    ("_fflush", fflush),
];

//! The scanf family over strings: `sscanf`, `vsscanf` and `sscanf_l`.
//!
//! [`scan`] follows C's rules on bytes and returns what each conversion
//! assigns; the host functions then store those through the pointer
//! arguments (each in its own 8-byte slot on Apple arm64, as for printf).
//!
//! Supported: `%d %i %u %o %x %X %p %n %s %c %[...] %f %e %g %a` (and their
//! capitals), `%%`, assignment suppression (`*`), field widths, and the
//! length modifiers `hh h l ll q j z t L`.

use crate::printf::Args;
use crate::{Emu, HostFn, Result};

/// A value a conversion assigns, with the size of the object it goes into.
#[derive(Debug, PartialEq)]
pub(crate) enum Value {
    Int(u64, usize),
    Float(f64, usize),
    /// `%s` and `%[` (NUL-terminated) or `%c` (not).
    Bytes(Vec<u8>),
}

/// The result of scanning: the count `sscanf` returns (-1 for EOF) and the
/// assignments in order.
#[derive(Debug, PartialEq)]
pub(crate) struct Scanned {
    pub ret: i64,
    pub values: Vec<Value>,
}

fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// Parse an integer in `base` (0: by prefix) from at most `max` bytes of `s`.
/// Returns (value, bytes used), or None when there are no digits.
fn integer(s: &[u8], max: usize, mut base: u32) -> Option<(u64, usize)> {
    let s = &s[..s.len().min(max)];
    let mut i = 0;
    let neg = match s.first() {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let hex_prefix = s.get(i) == Some(&b'0') && matches!(s.get(i + 1), Some(b'x' | b'X'));
    if (base == 0 || base == 16) && hex_prefix && s.get(i + 2).is_some_and(|c| c.is_ascii_hexdigit()) {
        base = 16;
        i += 2;
    } else if base == 0 {
        base = if s.get(i) == Some(&b'0') { 8 } else { 10 };
    }
    let start = i;
    let mut v: u64 = 0;
    while let Some(d) = s.get(i).and_then(|c| (*c as char).to_digit(base)) {
        v = v.wrapping_mul(base as u64).wrapping_add(d as u64);
        i += 1;
    }
    if i == start {
        // "0x" with no hex digits after it still reads the 0.
        return if hex_prefix { Some((0, start - 1)) } else { None };
    }
    Some((if neg { v.wrapping_neg() } else { v }, i))
}

/// Parse a floating-point number (decimal, hex, inf or nan) from at most
/// `max` bytes of `s`: (value, bytes used).
fn float(s: &[u8], max: usize) -> Option<(f64, usize)> {
    let s = &s[..s.len().min(max)];
    let mut i = 0;
    let neg = match s.first() {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let sign = |v: f64| if neg { -v } else { v };
    let rest = &s[i..];
    let starts = |w: &[u8]| rest.len() >= w.len() && rest[..w.len()].eq_ignore_ascii_case(w);
    if starts(b"infinity") {
        return Some((sign(f64::INFINITY), i + 8));
    }
    if starts(b"inf") {
        return Some((sign(f64::INFINITY), i + 3));
    }
    if starts(b"nan") {
        return Some((sign(f64::NAN), i + 3));
    }
    if starts(b"0x") && rest.get(2).is_some_and(|c| c.is_ascii_hexdigit() || *c == b'.') {
        // Hex: mantissa digits, an optional binary exponent.
        let mut j = i + 2;
        let (mut m, mut exp, mut digits, mut dot) = (0f64, 0i32, 0, false);
        while let Some(&c) = s.get(j) {
            if c == b'.' && !dot {
                dot = true;
            } else if let Some(d) = (c as char).to_digit(16) {
                m = m * 16.0 + d as f64;
                digits += 1;
                if dot {
                    exp -= 4;
                }
            } else {
                break;
            }
            j += 1;
        }
        if digits == 0 {
            return Some((sign(0.0), i + 1));
        }
        if matches!(s.get(j), Some(b'p' | b'P'))
            && let Some((e, n)) = integer(&s[j + 1..], usize::MAX, 10)
        {
            exp += e as i64 as i32;
            j += 1 + n;
        }
        return Some((sign(m * 2f64.powi(exp)), j));
    }
    // Decimal: digits, a point, digits, then an exponent if one follows.
    let mut j = i;
    let mut digits = 0;
    while s.get(j).is_some_and(u8::is_ascii_digit) {
        j += 1;
        digits += 1;
    }
    if s.get(j) == Some(&b'.') {
        j += 1;
        while s.get(j).is_some_and(u8::is_ascii_digit) {
            j += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if matches!(s.get(j), Some(b'e' | b'E')) {
        let mut k = j + 1;
        if matches!(s.get(k), Some(b'+' | b'-')) {
            k += 1;
        }
        if s.get(k).is_some_and(u8::is_ascii_digit) {
            while s.get(k).is_some_and(u8::is_ascii_digit) {
                k += 1;
            }
            j = k;
        }
    }
    let text = std::str::from_utf8(&s[i..j]).ok()?;
    let v: f64 = text.trim_end_matches('.').parse().or_else(|_| text.parse()).ok()?;
    Some((sign(v), j))
}

/// A `%[...]` set: the bytes it matches, and where the format continues.
fn scanset(fmt: &[u8], mut i: usize) -> ([bool; 256], usize) {
    let mut set = [false; 256];
    let invert = fmt.get(i) == Some(&b'^');
    if invert {
        i += 1;
    }
    let first = i;
    while let Some(&c) = fmt.get(i) {
        if c == b']' && i != first {
            i += 1;
            break;
        }
        if fmt.get(i + 1) == Some(&b'-') && fmt.get(i + 2).is_some_and(|&e| e != b']') {
            let e = fmt[i + 2];
            for b in c.min(e)..=c.max(e) {
                set[b as usize] = true;
            }
            i += 3;
        } else {
            set[c as usize] = true;
            i += 1;
        }
    }
    if invert {
        for b in &mut set {
            *b = !*b;
        }
    }
    (set, i)
}

/// Scan `input` by `fmt`, as `sscanf` does.
pub(crate) fn scan(input: &[u8], fmt: &[u8]) -> Scanned {
    let mut values = vec![];
    let (mut i, mut f) = (0, 0);
    let mut assigned = 0i64;
    let mut converted = false;
    let eof = |values, assigned, converted: bool| Scanned { ret: if converted { assigned } else { -1 }, values };
    while f < fmt.len() {
        let c = fmt[f];
        if is_space(c) {
            while fmt.get(f).is_some_and(|&c| is_space(c)) {
                f += 1;
            }
            while input.get(i).is_some_and(|&c| is_space(c)) {
                i += 1;
            }
            continue;
        }
        if c != b'%' || fmt.get(f + 1) == Some(&b'%') {
            let lit = if c == b'%' {
                f += 1;
                while input.get(i).is_some_and(|&c| is_space(c)) {
                    i += 1;
                }
                b'%'
            } else {
                c
            };
            f += 1;
            match input.get(i) {
                Some(&x) if x == lit => i += 1,
                Some(_) => break,
                None => return eof(values, assigned, converted),
            }
            continue;
        }
        // %[*][width][length]conv
        f += 1;
        let suppress = fmt.get(f) == Some(&b'*');
        if suppress {
            f += 1;
        }
        let mut width = 0usize;
        while let Some(d) = fmt.get(f).filter(|c| c.is_ascii_digit()) {
            width = width * 10 + (d - b'0') as usize;
            f += 1;
        }
        let mut size = 4usize;
        let mut fsize = 4usize;
        loop {
            match fmt.get(f) {
                Some(b'h') if fmt.get(f + 1) == Some(&b'h') => {
                    size = 1;
                    f += 2;
                }
                Some(b'h') => {
                    size = 2;
                    f += 1;
                }
                Some(b'l') if fmt.get(f + 1) == Some(&b'l') => {
                    size = 8;
                    fsize = 8;
                    f += 2;
                }
                Some(b'l' | b'q' | b'j' | b'z' | b't' | b'L') => {
                    size = 8;
                    fsize = 8;
                    f += 1;
                }
                _ => break,
            }
        }
        let Some(&conv) = fmt.get(f) else { break };
        f += 1;
        if !matches!(conv, b'[' | b'c' | b'n') {
            while input.get(i).is_some_and(|&c| is_space(c)) {
                i += 1;
            }
        }
        let max = if width == 0 { usize::MAX } else { width };
        let rest = &input[i..];
        if rest.is_empty() && conv != b'n' {
            return eof(values, assigned, converted);
        }
        let value = match conv {
            b'd' | b'i' | b'u' | b'o' | b'x' | b'X' | b'p' => {
                let base = match conv {
                    b'd' | b'u' => 10,
                    b'i' => 0,
                    b'o' => 8,
                    _ => 16,
                };
                let Some((v, n)) = integer(rest, max, base) else { break };
                i += n;
                Value::Int(v, if conv == b'p' { 8 } else { size })
            }
            b'a' | b'A' | b'e' | b'E' | b'f' | b'F' | b'g' | b'G' => {
                let Some((v, n)) = float(rest, max) else { break };
                i += n;
                Value::Float(v, fsize)
            }
            b's' => {
                let n = rest.iter().take(max).take_while(|&&c| !is_space(c)).count();
                i += n;
                let mut v = rest[..n].to_vec();
                v.push(0);
                Value::Bytes(v)
            }
            b'c' => {
                let n = if width == 0 { 1 } else { width };
                if rest.len() < n {
                    return eof(values, assigned, converted);
                }
                i += n;
                Value::Bytes(rest[..n].to_vec())
            }
            b'[' => {
                let (set, next) = scanset(fmt, f);
                f = next;
                let n = rest.iter().take(max).take_while(|&&c| set[c as usize]).count();
                if n == 0 {
                    break;
                }
                i += n;
                let mut v = rest[..n].to_vec();
                v.push(0);
                Value::Bytes(v)
            }
            b'n' => {
                if !suppress {
                    values.push(Value::Int(i as u64, size));
                }
                continue;
            }
            _ => break,
        };
        converted = true;
        if !suppress {
            values.push(value);
            assigned += 1;
        }
    }
    Scanned { ret: assigned, values }
}

/// Run `scan` and store the values through the pointers in `args`.
fn scan_into(emu: &mut Emu, input: u64, fmt: u64, mut args: Args) -> Result<()> {
    let s = emu.cstr_bytes(input)?;
    let f = emu.cstr_bytes(fmt)?;
    let scanned = scan(&s, &f);
    for v in scanned.values {
        let p = args.ptr(emu)?;
        match v {
            Value::Int(x, n) => emu.p.memory.write(p, &x.to_le_bytes()[..n])?,
            Value::Float(x, 4) => emu.p.memory.write(p, &(x as f32).to_le_bytes())?,
            Value::Float(x, _) => emu.p.memory.write(p, &x.to_le_bytes())?,
            Value::Bytes(b) => emu.p.memory.write(p, &b)?,
        }
    }
    emu.ret(scanned.ret as u64);
    Ok(())
}

/// `sscanf(str, format, ...)`
fn sscanf(emu: &mut Emu) -> Result<()> {
    let (s, f, sp) = (emu.arg(0), emu.arg(1), emu.cpu.sp);
    scan_into(emu, s, f, Args::new(sp))
}

/// `sscanf_l(str, locale, format, ...)`
fn sscanf_l(emu: &mut Emu) -> Result<()> {
    let (s, f, sp) = (emu.arg(0), emu.arg(2), emu.cpu.sp);
    scan_into(emu, s, f, Args::new(sp))
}

/// `vsscanf(str, format, va)`
fn vsscanf(emu: &mut Emu) -> Result<()> {
    let (s, f, va) = (emu.arg(0), emu.arg(1), emu.arg(2));
    scan_into(emu, s, f, Args::new(va))
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[("_sscanf", sscanf), ("_sscanf_l", sscanf_l), ("_vsscanf", vsscanf)];

#[cfg(test)]
mod tests {
    use super::Value::*;
    use super::*;

    fn ints(input: &str, fmt: &str) -> (i64, Vec<i64>) {
        let s = scan(input.as_bytes(), fmt.as_bytes());
        let v = s
            .values
            .iter()
            .map(|v| match v {
                Int(x, 4) => *x as u32 as i32 as i64,
                Int(x, _) => *x as i64,
                _ => panic!("not an int"),
            })
            .collect();
        (s.ret, v)
    }

    #[test]
    fn integers() {
        assert_eq!(ints("  42 -7 0x1F 017", "%d %d %x %i"), (4, vec![42, -7, 31, 15]));
        assert_eq!(ints("0x10 010 10", "%i %i %i"), (3, vec![16, 8, 10]));
        assert_eq!(ints("12345", "%3d%d"), (2, vec![123, 45]));
        assert_eq!(ints("1.2.3.4", "%d.%d.%d.%d"), (4, vec![1, 2, 3, 4]));
        assert_eq!(ints("HTTP/1.1 200", "HTTP/%d.%d %d"), (3, vec![1, 1, 200]));
    }

    #[test]
    fn failures_and_eof() {
        assert_eq!(ints("", "%d"), (-1, vec![]));
        assert_eq!(ints("   ", "%d"), (-1, vec![]));
        assert_eq!(ints("abc", "%d"), (0, vec![]));
        assert_eq!(ints("5 x", "%d %d"), (1, vec![5]));
        assert_eq!(ints("5", "%d %d"), (1, vec![5]));
        assert_eq!(ints("a5", "b%d"), (0, vec![]));
    }

    #[test]
    fn sizes_suppression_and_count() {
        let s = scan(b"300 7 99 abc", b"%hhu %*d %lld %n");
        assert_eq!(s.ret, 2);
        assert_eq!(s.values, vec![Int(300, 1), Int(99, 8), Int(9, 4)]);
    }

    #[test]
    fn strings_chars_and_sets() {
        let s = scan(b"key=value; rest", b"%[^=]=%[^;]; %3c");
        assert_eq!(s.ret, 3);
        assert_eq!(s.values, vec![Bytes(b"key\0".to_vec()), Bytes(b"value\0".to_vec()), Bytes(b"res".to_vec())]);
        let s = scan(b"  hello world", b"%4s%s");
        assert_eq!(s.values, vec![Bytes(b"hell\0".to_vec()), Bytes(b"o\0".to_vec())]);
        let s = scan(b"a]b-c", b"%[]a]%[-b]");
        assert_eq!(s.values, vec![Bytes(b"a]\0".to_vec()), Bytes(b"b-\0".to_vec())]);
        assert_eq!(scan(b"100%", b"%d%%").ret, 1);
    }

    #[test]
    fn floats() {
        let s = scan(b"3.5 -2e3 0x1.8p1 inf .25", b"%f %lf %a %g %f");
        assert_eq!(s.values, vec![Float(3.5, 4), Float(-2000.0, 8), Float(3.0, 4), Float(f64::INFINITY, 4), Float(0.25, 4)]);
        assert_eq!(scan(b"1e", b"%f%s").values, vec![Float(1.0, 4), Bytes(b"e\0".to_vec())]);
    }
}

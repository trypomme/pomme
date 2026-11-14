//! NSScanner: reading a string piece by piece (literal strings, runs of a
//! character set, numbers), as Foundation's does. Positions are in UTF-16
//! units. Before each scan the scanner skips `charactersToBeSkipped`
//! (whitespace and newlines unless set otherwise); a scan that matches nothing
//! leaves the position where it was and returns NO.

use crate::foundation::{self, cf_call, nsstring};
use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// CFCharacterSetPredefinedSet's whitespace and newlines.
const WHITESPACE_AND_NEWLINE: u64 = 3;

#[derive(Clone)]
pub struct Scanner {
    text: Vec<u16>,
    string: u64,
    loc: usize,
    /// The NSCharacterSet to skip, or 0 for none.
    skip: u64,
    case_sensitive: bool,
}

#[derive(Clone, Default)]
pub struct State {
    scanners: HashMap<u64, Scanner>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn make(emu: &mut Emu, obj: u64, string: u64) -> Result<u64> {
    let text: Vec<u16> = if string == 0 { Vec::new() } else { nsstring(emu, string)?.encode_utf16().collect() };
    let skip = cf_call(emu, "_CFCharacterSetGetPredefined", &[WHITESPACE_AND_NEWLINE])?;
    emu.scanners.scanners.insert(obj, Scanner { text, string, loc: 0, skip, case_sensitive: false });
    Ok(obj)
}

fn member(emu: &mut Emu, set: u64, unit: u16) -> Result<bool> {
    Ok(set != 0 && cf_call(emu, "_CFCharacterSetIsCharacterMember", &[set, unit as u64])? & 0xff != 0)
}

/// The scanner's text and the position after skipping: (text, start).
fn skipped(emu: &mut Emu, obj: u64) -> Result<Option<(Vec<u16>, usize)>> {
    let Some(s) = emu.scanners.scanners.get(&obj) else { return Ok(None) };
    let (text, mut at, skip) = (s.text.clone(), s.loc, s.skip);
    while at < text.len() && member(emu, skip, text[at])? {
        at += 1;
    }
    Ok(Some((text, at)))
}

fn set_loc(emu: &mut Emu, obj: u64, loc: usize) {
    if let Some(s) = emu.scanners.scanners.get_mut(&obj) {
        s.loc = loc;
    }
}

/// `*out` (if given) = a new NSString of `units`.
fn give_string(emu: &mut Emu, out: u64, units: &[u16]) -> Result<u64> {
    let s = foundation::new_nsstring(emu, &String::from_utf16_lossy(units))?;
    if out != 0 {
        emu.write_u64(out, s)?;
    }
    Ok(s)
}

/// `scanString:` at the position: the length matched.
fn match_literal(emu: &Emu, obj: u64, text: &[u16], at: usize, lit: &[u16]) -> Option<usize> {
    let cs = emu.scanners.scanners.get(&obj).is_some_and(|s| s.case_sensitive);
    let end = at + lit.len();
    if end > text.len() {
        return None;
    }
    let same = if cs { &text[at..end] == lit } else { String::from_utf16_lossy(&text[at..end]).to_lowercase() == String::from_utf16_lossy(lit).to_lowercase() };
    same.then_some(lit.len())
}

/// `scanString:intoString:` (or the newer `scanString:`, `returns_string`).
fn scan_string(emu: &mut Emu, out: u64, returns_string: bool) -> Result<()> {
    let (obj, lit) = (emu.arg(0), emu.arg(2));
    let lit: Vec<u16> = nsstring(emu, lit)?.encode_utf16().collect();
    let Some((text, at)) = skipped(emu, obj)? else { return ret(emu, 0) };
    match match_literal(emu, obj, &text, at, &lit) {
        Some(n) if n > 0 => {
            set_loc(emu, obj, at + n);
            let s = if returns_string || out != 0 { give_string(emu, out, &text[at..at + n])? } else { 0 };
            ret(emu, if returns_string { s } else { 1 })
        }
        _ => ret(emu, 0),
    }
}

/// `scanCharactersFromSet:` (`inside`) and `scanUpToCharactersFromSet:`.
fn scan_set(emu: &mut Emu, inside: bool, out: u64, returns_string: bool) -> Result<()> {
    let (obj, set) = (emu.arg(0), emu.arg(2));
    let Some((text, at)) = skipped(emu, obj)? else { return ret(emu, 0) };
    let mut end = at;
    while end < text.len() && member(emu, set, text[end])? == inside {
        end += 1;
    }
    if end == at {
        return ret(emu, 0);
    }
    set_loc(emu, obj, end);
    let s = if returns_string || out != 0 { give_string(emu, out, &text[at..end])? } else { 0 };
    ret(emu, if returns_string { s } else { 1 })
}

/// `scanUpToString:intoString:` (or `scanUpToString:`).
fn scan_up_to_string(emu: &mut Emu, out: u64, returns_string: bool) -> Result<()> {
    let (obj, lit) = (emu.arg(0), emu.arg(2));
    let lit: Vec<u16> = nsstring(emu, lit)?.encode_utf16().collect();
    let Some((text, at)) = skipped(emu, obj)? else { return ret(emu, 0) };
    let mut end = at;
    while end < text.len() && (lit.is_empty() || match_literal(emu, obj, &text, end, &lit).is_none()) {
        end += 1;
    }
    if end == at {
        return ret(emu, 0);
    }
    set_loc(emu, obj, end);
    let s = if returns_string || out != 0 { give_string(emu, out, &text[at..end])? } else { 0 };
    ret(emu, if returns_string { s } else { 1 })
}

/// A decimal integer at `at`: (value, end), saturating like Foundation.
fn parse_int(text: &[u16], at: usize) -> Option<(i128, usize)> {
    let mut i = at;
    let neg = match text.get(i) {
        Some(&c) if c == b'-' as u16 => {
            i += 1;
            true
        }
        Some(&c) if c == b'+' as u16 => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut v: i128 = 0;
    while let Some(&c) = text.get(i) {
        if !(b'0' as u16..=b'9' as u16).contains(&c) {
            break;
        }
        v = (v * 10 + (c - b'0' as u16) as i128).min(i128::from(u64::MAX) + 1);
        i += 1;
    }
    (i > start).then_some((if neg { -v } else { v }, i))
}

/// A hexadecimal integer at `at`, with an optional 0x: (value, end).
fn parse_hex(text: &[u16], at: usize) -> Option<(u128, usize)> {
    let mut i = at;
    if text.get(i) == Some(&(b'0' as u16)) && matches!(text.get(i + 1), Some(&c) if c == b'x' as u16 || c == b'X' as u16) && text.get(i + 2).and_then(|&c| char::from_u32(c as u32)).is_some_and(|c| c.is_ascii_hexdigit()) {
        i += 2;
    }
    let start = i;
    let mut v: u128 = 0;
    while let Some(d) = text.get(i).and_then(|&c| char::from_u32(c as u32)).and_then(|c| c.to_digit(16)) {
        v = (v * 16 + d as u128).min(u64::MAX as u128 + 1);
        i += 1;
    }
    (i > start).then_some((v, i))
}

/// A decimal floating-point number at `at`: (value, end).
fn parse_double(text: &[u16], at: usize) -> Option<(f64, usize)> {
    let c = |i: usize| text.get(i).and_then(|&u| char::from_u32(u as u32)).unwrap_or('\0');
    let mut i = at;
    if c(i) == '+' || c(i) == '-' {
        i += 1;
    }
    let digits_start = i;
    while c(i).is_ascii_digit() {
        i += 1;
    }
    if c(i) == '.' {
        i += 1;
        while c(i).is_ascii_digit() {
            i += 1;
        }
    }
    if i == digits_start || (i == digits_start + 1 && c(digits_start) == '.') {
        return None;
    }
    if c(i) == 'e' || c(i) == 'E' {
        let mut j = i + 1;
        if c(j) == '+' || c(j) == '-' {
            j += 1;
        }
        if c(j).is_ascii_digit() {
            while c(j).is_ascii_digit() {
                j += 1;
            }
            i = j;
        }
    }
    let s = String::from_utf16_lossy(&text[at..i]);
    s.parse().ok().map(|v| (v, i))
}

/// Scan an integer of `bits` (signed or not) into `*out`.
fn scan_integer(emu: &mut Emu, bits: u32, signed: bool) -> Result<()> {
    let (obj, out) = (emu.arg(0), emu.arg(2));
    let Some((text, at)) = skipped(emu, obj)? else { return ret(emu, 0) };
    let Some((v, end)) = parse_int(&text, at) else { return ret(emu, 0) };
    let (min, max) = if signed { (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1) } else { (0, (1i128 << bits) - 1) };
    // Unsigned scans of negative numbers give 0 in Foundation, larger ones the maximum.
    let v = v.clamp(min, max);
    set_loc(emu, obj, end);
    if out != 0 {
        if bits == 32 {
            emu.p.memory.write(out, &(v as u32).to_le_bytes())?;
        } else {
            emu.write_u64(out, v as u64)?;
        }
    }
    ret(emu, 1)
}

fn scan_hex(emu: &mut Emu, bits: u32) -> Result<()> {
    let (obj, out) = (emu.arg(0), emu.arg(2));
    let Some((text, at)) = skipped(emu, obj)? else { return ret(emu, 0) };
    let Some((v, end)) = parse_hex(&text, at) else { return ret(emu, 0) };
    let v = v.min((1u128 << bits) - 1);
    set_loc(emu, obj, end);
    if out != 0 {
        if bits == 32 {
            emu.p.memory.write(out, &(v as u32).to_le_bytes())?;
        } else {
            emu.write_u64(out, v as u64)?;
        }
    }
    ret(emu, 1)
}

fn scan_float(emu: &mut Emu, double: bool) -> Result<()> {
    let (obj, out) = (emu.arg(0), emu.arg(2));
    let Some((text, at)) = skipped(emu, obj)? else { return ret(emu, 0) };
    let Some((v, end)) = parse_double(&text, at) else { return ret(emu, 0) };
    set_loc(emu, obj, end);
    if out != 0 {
        if double {
            emu.write_u64(out, v.to_bits())?;
        } else {
            emu.p.memory.write(out, &(v as f32).to_bits().to_le_bytes())?;
        }
    }
    ret(emu, 1)
}

const S: &str = "NSScanner";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (S, true, "scannerWithString:", |e| {
        let string = e.arg(2);
        let cls = host_class(e, S)?;
        let obj = objc::create_instance(e, cls, 0)?;
        let o = make(e, obj, string)?;
        ret(e, o)
    }),
    (S, true, "localizedScannerWithString:", |e| {
        let string = e.arg(2);
        let cls = host_class(e, S)?;
        let obj = objc::create_instance(e, cls, 0)?;
        let o = make(e, obj, string)?;
        ret(e, o)
    }),
    (S, false, "initWithString:", |e| {
        let (obj, string) = (e.arg(0), e.arg(2));
        let o = make(e, obj, string)?;
        ret(e, o)
    }),
    (S, false, "string", |e| {
        let s = e.scanners.scanners.get(&e.arg(0)).map_or(0, |s| s.string);
        ret(e, s)
    }),
    (S, false, "scanLocation", |e| {
        let l = e.scanners.scanners.get(&e.arg(0)).map_or(0, |s| s.loc);
        ret(e, l as u64)
    }),
    (S, false, "setScanLocation:", |e| {
        let (obj, l) = (e.arg(0), e.arg(2) as usize);
        let len = e.scanners.scanners.get(&obj).map_or(0, |s| s.text.len());
        set_loc(e, obj, l.min(len));
        Ok(())
    }),
    (S, false, "currentIndex", |e| {
        let l = e.scanners.scanners.get(&e.arg(0)).map_or(0, |s| s.loc);
        ret(e, l as u64)
    }),
    (S, false, "charactersToBeSkipped", |e| {
        let s = e.scanners.scanners.get(&e.arg(0)).map_or(0, |s| s.skip);
        ret(e, s)
    }),
    (S, false, "setCharactersToBeSkipped:", |e| {
        let (obj, set) = (e.arg(0), e.arg(2));
        if let Some(s) = e.scanners.scanners.get_mut(&obj) {
            s.skip = set;
        }
        Ok(())
    }),
    (S, false, "caseSensitive", |e| {
        let c = e.scanners.scanners.get(&e.arg(0)).is_some_and(|s| s.case_sensitive);
        ret(e, c as u64)
    }),
    (S, false, "setCaseSensitive:", |e| {
        let (obj, v) = (e.arg(0), e.arg(2) & 0xff != 0);
        if let Some(s) = e.scanners.scanners.get_mut(&obj) {
            s.case_sensitive = v;
        }
        Ok(())
    }),
    (S, false, "isAtEnd", |e| {
        let obj = e.arg(0);
        let end = match skipped(e, obj)? {
            Some((text, at)) => at >= text.len(),
            None => true,
        };
        ret(e, end as u64)
    }),
    (S, false, "scanString:intoString:", |e| {
        let out = e.arg(3);
        scan_string(e, out, false)
    }),
    (S, false, "scanString:", |e| scan_string(e, 0, true)),
    (S, false, "scanCharactersFromSet:intoString:", |e| {
        let out = e.arg(3);
        scan_set(e, true, out, false)
    }),
    (S, false, "scanCharactersFromSet:", |e| scan_set(e, true, 0, true)),
    (S, false, "scanUpToCharactersFromSet:intoString:", |e| {
        let out = e.arg(3);
        scan_set(e, false, out, false)
    }),
    (S, false, "scanUpToCharactersFromSet:", |e| scan_set(e, false, 0, true)),
    (S, false, "scanUpToString:intoString:", |e| {
        let out = e.arg(3);
        scan_up_to_string(e, out, false)
    }),
    (S, false, "scanUpToString:", |e| scan_up_to_string(e, 0, true)),
    (S, false, "scanInt:", |e| scan_integer(e, 32, true)),
    (S, false, "scanInteger:", |e| scan_integer(e, 64, true)),
    (S, false, "scanLongLong:", |e| scan_integer(e, 64, true)),
    (S, false, "scanUnsignedLongLong:", |e| scan_integer(e, 64, false)),
    (S, false, "scanHexInt:", |e| scan_hex(e, 32)),
    (S, false, "scanHexLongLong:", |e| scan_hex(e, 64)),
    (S, false, "scanDouble:", |e| scan_float(e, true)),
    (S, false, "scanFloat:", |e| scan_float(e, false)),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_int(&u("-42abc"), 0), Some((-42, 3)));
        assert_eq!(parse_int(&u("+7"), 0), Some((7, 2)));
        assert_eq!(parse_int(&u("-x"), 0), None);
        assert_eq!(parse_hex(&u("0x1fZ"), 0), Some((0x1f, 4)));
        assert_eq!(parse_hex(&u("ff"), 0), Some((0xff, 2)));
        assert_eq!(parse_hex(&u("0xg"), 0), Some((0, 1))); // just the 0
        assert_eq!(parse_double(&u("3.5e2x"), 0), Some((350.0, 5)));
        assert_eq!(parse_double(&u("-.5"), 0), Some((-0.5, 3)));
        assert_eq!(parse_double(&u("1e"), 0), Some((1.0, 1)));
        assert_eq!(parse_double(&u("."), 0), None);
    }
}

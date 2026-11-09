//! NSCharacterSet: the standard sets are CoreFoundation's predefined ones
//! (toll-free bridged, so they're `__NSCFCharacterSet`s), and the NSString
//! methods that take a set: trimming, searching and splitting.

use crate::foundation::{self, cf_call, nsstring};
use crate::{Emu, HostFn, Result};

const NOT_FOUND: u64 = i64::MAX as u64;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// `CFCharacterSetGetPredefined(kind)`.
fn predefined(emu: &mut Emu, kind: u64) -> Result<()> {
    let s = cf_call(emu, "_CFCharacterSetGetPredefined", &[kind])?;
    ret(emu, s)
}

fn is_member(emu: &mut Emu, set: u64, ch: char) -> Result<bool> {
    Ok(cf_call(emu, "_CFCharacterSetIsLongCharacterMember", &[set, ch as u64])? & 0xff != 0)
}

/// The string's characters with their UTF-16 offsets.
fn chars_of(emu: &mut Emu, s: u64) -> Result<Vec<(usize, char)>> {
    let text = nsstring(emu, s)?;
    let mut out = Vec::new();
    let mut unit = 0;
    for c in text.chars() {
        out.push((unit, c));
        unit += c.len_utf16();
    }
    Ok(out)
}

fn trimming(emu: &mut Emu) -> Result<()> {
    let (s, set) = (emu.arg(0), emu.arg(2));
    let text = nsstring(emu, s)?;
    let chars: Vec<char> = text.chars().collect();
    let mut a = 0;
    while a < chars.len() && is_member(emu, set, chars[a])? {
        a += 1;
    }
    let mut b = chars.len();
    while b > a && is_member(emu, set, chars[b - 1])? {
        b -= 1;
    }
    let out: String = chars[a..b].iter().collect();
    let r = foundation::new_nsstring(emu, &out)?;
    ret(emu, r)
}

/// `rangeOfCharacterFromSet:` (and `…options:`, backwards with NSBackwardsSearch = 4).
fn range_of_character(emu: &mut Emu, options: u64) -> Result<()> {
    let (s, set) = (emu.arg(0), emu.arg(2));
    let chars = chars_of(emu, s)?;
    let mut found = None;
    let order: Vec<&(usize, char)> = if options & 4 != 0 { chars.iter().rev().collect() } else { chars.iter().collect() };
    for &&(at, c) in &order {
        if is_member(emu, set, c)? {
            found = Some((at, c.len_utf16()));
            break;
        }
    }
    let (loc, len) = found.map_or((NOT_FOUND, 0), |(a, l)| (a as u64, l as u64));
    emu.ret(loc);
    emu.cpu.x[1] = len;
    Ok(())
}

fn components(emu: &mut Emu) -> Result<()> {
    let (s, set) = (emu.arg(0), emu.arg(2));
    let text = nsstring(emu, s)?;
    let mut parts = vec![String::new()];
    for c in text.chars() {
        if is_member(emu, set, c)? {
            parts.push(String::new());
        } else {
            parts.last_mut().unwrap().push(c);
        }
    }
    let mut objs = Vec::new();
    for p in parts {
        objs.push(foundation::new_nsstring(emu, &p)?);
    }
    let a = foundation::new_array_of(emu, &objs)?;
    ret(emu, a)
}

const CS: &str = "NSCharacterSet";

// CFCharacterSetPredefinedSet
const CONTROL: u64 = 1;
const WHITESPACE: u64 = 2;
const WHITESPACE_AND_NEWLINE: u64 = 3;
const DECIMAL_DIGIT: u64 = 4;
const LETTER: u64 = 5;
const LOWERCASE: u64 = 6;
const UPPERCASE: u64 = 7;
const ALPHANUMERIC: u64 = 10;
const PUNCTUATION: u64 = 11;
const SYMBOL: u64 = 14;
const NEWLINE: u64 = 15;

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (CS, true, "controlCharacterSet", |e| predefined(e, CONTROL)),
    (CS, true, "whitespaceCharacterSet", |e| predefined(e, WHITESPACE)),
    (CS, true, "whitespaceAndNewlineCharacterSet", |e| predefined(e, WHITESPACE_AND_NEWLINE)),
    (CS, true, "decimalDigitCharacterSet", |e| predefined(e, DECIMAL_DIGIT)),
    (CS, true, "letterCharacterSet", |e| predefined(e, LETTER)),
    (CS, true, "lowercaseLetterCharacterSet", |e| predefined(e, LOWERCASE)),
    (CS, true, "uppercaseLetterCharacterSet", |e| predefined(e, UPPERCASE)),
    (CS, true, "alphanumericCharacterSet", |e| predefined(e, ALPHANUMERIC)),
    (CS, true, "punctuationCharacterSet", |e| predefined(e, PUNCTUATION)),
    (CS, true, "symbolCharacterSet", |e| predefined(e, SYMBOL)),
    (CS, true, "newlineCharacterSet", |e| predefined(e, NEWLINE)),
    (CS, true, "characterSetWithCharactersInString:", |e| {
        let s = e.arg(2);
        let set = cf_call(e, "_CFCharacterSetCreateWithCharactersInString", &[0, s])?;
        ret(e, set)
    }),
    (CS, false, "characterIsMember:", |e| {
        let (set, c) = (e.arg(0), e.arg(2) & 0xffff);
        let m = cf_call(e, "_CFCharacterSetIsCharacterMember", &[set, c])? & 0xff;
        ret(e, m)
    }),
    (CS, false, "longCharacterIsMember:", |e| {
        let (set, c) = (e.arg(0), e.arg(2) & 0xffff_ffff);
        let m = cf_call(e, "_CFCharacterSetIsLongCharacterMember", &[set, c])? & 0xff;
        ret(e, m)
    }),
    (CS, false, "invertedSet", |e| {
        let set = e.arg(0);
        let inv = cf_call(e, "_CFCharacterSetCreateInvertedSet", &[0, set])?;
        ret(e, inv)
    }),
    ("NSString", false, "stringByTrimmingCharactersInSet:", trimming),
    ("NSString", false, "rangeOfCharacterFromSet:", |e| range_of_character(e, 0)),
    ("NSString", false, "rangeOfCharacterFromSet:options:", |e| {
        let o = e.arg(3);
        range_of_character(e, o)
    }),
    ("NSString", false, "componentsSeparatedByCharactersInSet:", components),
];

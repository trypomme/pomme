//! NSCharacterSet: the standard sets are CoreFoundation's predefined ones
//! (toll-free bridged, so they're `__NSCFCharacterSet`s), and the NSString
//! methods that take a set: trimming, searching and splitting.
//!
//! NSMutableCharacterSet is CFMutableCharacterSet. As in Foundation, the
//! factory methods called on NSMutableCharacterSet (`[NSMutableCharacterSet
//! alphanumericCharacterSet]`) give a new mutable set, not the shared one.

use crate::foundation::{self, cf_call, nsstring};
use crate::{Emu, HostFn, Result};

const NOT_FOUND: u64 = i64::MAX as u64;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// A factory method's set: as made when sent to NSCharacterSet, a mutable copy
/// when sent to NSMutableCharacterSet (or a subclass).
fn give(emu: &mut Emu, set: u64) -> Result<()> {
    let receiver = emu.arg(0);
    let mutable = foundation::wants_mutable(emu, receiver, "NSMutableCharacterSet")?;
    let set = if mutable { cf_call(emu, "_CFCharacterSetCreateMutableCopy", &[0, set])? } else { set };
    ret(emu, set)
}

/// `CFCharacterSetGetPredefined(kind)`.
fn predefined(emu: &mut Emu, kind: u64) -> Result<()> {
    let s = cf_call(emu, "_CFCharacterSetGetPredefined", &[kind])?;
    give(emu, s)
}

/// A mutator: `CF…(self, argument)`, returning nothing.
fn mutate(emu: &mut Emu, f: &'static str) -> Result<()> {
    let (set, arg) = (emu.arg(0), emu.arg(2));
    cf_call(emu, f, &[set, arg])?;
    Ok(())
}

/// `add`/`removeCharactersInRange:` (an NSRange: location, length).
fn mutate_range(emu: &mut Emu, f: &'static str) -> Result<()> {
    let (set, loc, len) = (emu.arg(0), emu.arg(2), emu.arg(3));
    cf_call(emu, f, &[set, loc, len])?;
    Ok(())
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

/// `stringByAddingPercentEncodingWithAllowedCharacters:`: characters outside
/// the set become %XX escapes of their UTF-8 bytes (uppercase hex).
fn adding_percent_encoding(emu: &mut Emu) -> Result<()> {
    let (s, set) = (emu.arg(0), emu.arg(2));
    let text = nsstring(emu, s)?;
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if is_member(emu, set, c)? {
            out.push(c);
        } else {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    let r = foundation::new_nsstring(emu, &out)?;
    ret(emu, r)
}

/// `stringByRemovingPercentEncoding`: %XX escapes decoded as UTF-8; nil when
/// an escape is malformed or the bytes aren't UTF-8.
pub fn remove_percent_encoding(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
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
const MCS: &str = "NSMutableCharacterSet";

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
const NON_BASE: u64 = 8;
const DECOMPOSABLE: u64 = 9;
const ILLEGAL: u64 = 12;
const CAPITALIZED_LETTER: u64 = 13;
const SYMBOL: u64 = 14;
const NEWLINE: u64 = 15;

/// The ASCII letters and digits, which every URL component allows.
const URL_ALNUM: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Foundation's `URL…AllowedCharacterSet`s: the characters RFC 3986 lets that
/// part of a URL hold unescaped (all ASCII).
fn url_allowed(emu: &mut Emu, extra: &str) -> Result<()> {
    let s = foundation::new_nsstring(emu, &format!("{URL_ALNUM}{extra}"))?;
    let set = cf_call(emu, "_CFCharacterSetCreateWithCharactersInString", &[0, s])?;
    give(emu, set)
}

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
    (CS, true, "nonBaseCharacterSet", |e| predefined(e, NON_BASE)),
    (CS, true, "decomposableCharacterSet", |e| predefined(e, DECOMPOSABLE)),
    (CS, true, "illegalCharacterSet", |e| predefined(e, ILLEGAL)),
    (CS, true, "capitalizedLetterCharacterSet", |e| predefined(e, CAPITALIZED_LETTER)),
    (CS, true, "URLQueryAllowedCharacterSet", |e| url_allowed(e, "!$&'()*+,-./:;=?@_~")),
    (CS, true, "URLFragmentAllowedCharacterSet", |e| url_allowed(e, "!$&'()*+,-./:;=?@_~")),
    (CS, true, "URLPathAllowedCharacterSet", |e| url_allowed(e, "!$&'()*+,-./:=@_~")),
    (CS, true, "URLHostAllowedCharacterSet", |e| url_allowed(e, "!$&'()*+,-.:;=[]_~")),
    (CS, true, "URLUserAllowedCharacterSet", |e| url_allowed(e, "!$&'()*+,-.;=_~")),
    (CS, true, "URLPasswordAllowedCharacterSet", |e| url_allowed(e, "!$&'()*+,-.;=_~")),
    (CS, true, "characterSetWithCharactersInString:", |e| {
        let s = e.arg(2);
        let set = cf_call(e, "_CFCharacterSetCreateWithCharactersInString", &[0, s])?;
        give(e, set)
    }),
    (CS, true, "characterSetWithRange:", |e| {
        let (loc, len) = (e.arg(2), e.arg(3));
        let set = cf_call(e, "_CFCharacterSetCreateWithCharactersInRange", &[0, loc, len])?;
        give(e, set)
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
    (CS, false, "isSupersetOfSet:", |e| {
        let (set, other) = (e.arg(0), e.arg(2));
        let m = cf_call(e, "_CFCharacterSetIsSupersetOfSet", &[set, other])? & 0xff;
        ret(e, m)
    }),
    (CS, false, "hasMemberInPlane:", |e| {
        let (set, plane) = (e.arg(0), e.arg(2) & 0xff);
        let m = cf_call(e, "_CFCharacterSetHasMemberInPlane", &[set, plane])? & 0xff;
        ret(e, m)
    }),
    (CS, false, "invertedSet", |e| {
        let set = e.arg(0);
        let inv = cf_call(e, "_CFCharacterSetCreateInvertedSet", &[0, set])?;
        ret(e, inv)
    }),
    (CS, false, "copy", |e| {
        let set = e.arg(0);
        let c = cf_call(e, "_CFCharacterSetCreateCopy", &[0, set])?;
        ret(e, c)
    }),
    (CS, false, "mutableCopy", |e| {
        let set = e.arg(0);
        let c = cf_call(e, "_CFCharacterSetCreateMutableCopy", &[0, set])?;
        ret(e, c)
    }),
    (MCS, false, "addCharactersInString:", |e| mutate(e, "_CFCharacterSetAddCharactersInString")),
    (MCS, false, "removeCharactersInString:", |e| mutate(e, "_CFCharacterSetRemoveCharactersInString")),
    (MCS, false, "addCharactersInRange:", |e| mutate_range(e, "_CFCharacterSetAddCharactersInRange")),
    (MCS, false, "removeCharactersInRange:", |e| mutate_range(e, "_CFCharacterSetRemoveCharactersInRange")),
    (MCS, false, "formUnionWithCharacterSet:", |e| mutate(e, "_CFCharacterSetUnion")),
    (MCS, false, "formIntersectionWithCharacterSet:", |e| mutate(e, "_CFCharacterSetIntersect")),
    (MCS, false, "invert", |e| {
        let set = e.arg(0);
        cf_call(e, "_CFCharacterSetInvert", &[set])?;
        Ok(())
    }),
    ("NSString", false, "stringByTrimmingCharactersInSet:", trimming),
    ("NSString", false, "rangeOfCharacterFromSet:", |e| range_of_character(e, 0)),
    ("NSString", false, "rangeOfCharacterFromSet:options:", |e| {
        let o = e.arg(3);
        range_of_character(e, o)
    }),
    ("NSString", false, "componentsSeparatedByCharactersInSet:", components),
    ("NSString", false, "stringByAddingPercentEncodingWithAllowedCharacters:", adding_percent_encoding),
    ("NSString", false, "stringByRemovingPercentEncoding", |e| {
        let text = nsstring(e, e.arg(0))?;
        let r = match remove_percent_encoding(&text) {
            Some(s) => foundation::new_nsstring(e, &s)?,
            None => 0,
        };
        ret(e, r)
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_percent_encoding() {
        assert_eq!(remove_percent_encoding("a%20b%E2%9C%93").as_deref(), Some("a b✓"));
        assert_eq!(remove_percent_encoding("100%"), None);
        assert_eq!(remove_percent_encoding("%zz"), None);
        assert_eq!(remove_percent_encoding("%C3"), None); // not UTF-8 on its own
    }
}

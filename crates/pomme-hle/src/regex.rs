//! NSRegularExpression and NSTextCheckingResult, on Rust's `regex` crate.
//!
//! Apple's engine is ICU's. The common subset (classes, groups, quantifiers,
//! anchors, \b \d \s \w, case-insensitive) behaves the same; lookaround and
//! backreferences aren't supported and make the pattern fail to compile, as
//! an invalid pattern would. Ranges are UTF-16, like NSString's.

use crate::foundation;
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

#[derive(Default)]
pub struct State {
    regexes: HashMap<u64, (regex::Regex, u64, u64)>, // compiled, pattern NSString, options
    /// NSTextCheckingResult → UTF-16 ranges of each group (None: didn't participate).
    results: HashMap<u64, Vec<Option<(u64, u64)>>>,
}

const NOT_FOUND: u64 = i64::MAX as u64;

/// A Rust string with a map from UTF-16 offsets to byte offsets (and back).
struct Text {
    s: String,
    /// Byte offset of each UTF-16 unit (len + 1 entries).
    bytes: Vec<usize>,
}

impl Text {
    fn new(s: String) -> Self {
        let mut bytes = Vec::with_capacity(s.len() + 1);
        for (i, c) in s.char_indices() {
            for _ in 0..c.len_utf16() {
                bytes.push(i);
            }
        }
        bytes.push(s.len());
        Text { s, bytes }
    }
    fn byte(&self, unit: u64) -> usize {
        self.bytes[(unit as usize).min(self.bytes.len() - 1)]
    }
    fn unit(&self, byte: usize) -> u64 {
        self.bytes.partition_point(|&b| b < byte) as u64
    }
}

/// Compile with NSRegularExpressionOptions (case-insensitive 1, comments 2,
/// literal 4, dot-all 8, multi-line anchors 16).
pub fn compile(pattern: &str, options: u64) -> std::result::Result<regex::Regex, String> {
    let pattern = if options & 4 != 0 { regex::escape(pattern) } else { pattern.to_string() };
    regex::RegexBuilder::new(&pattern)
        .case_insensitive(options & 1 != 0)
        .ignore_whitespace(options & 2 != 0)
        .dot_matches_new_line(options & 8 != 0)
        .multi_line(options & 16 != 0)
        .build()
        .map_err(|e| e.to_string())
}

/// Expand an NSRegularExpression template: `$n` is group n, `\x` is x.
pub fn expand(template: &str, caps: &regex::Captures) -> String {
    let mut out = String::new();
    let mut it = template.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\\' => {
                if let Some(n) = it.next() {
                    out.push(n);
                }
            }
            '$' if it.peek().is_some_and(|d| d.is_ascii_digit()) => {
                let mut n = 0usize;
                while let Some(d) = it.peek().and_then(|d| d.to_digit(10)) {
                    // Like ICU: take more digits only while that's still a valid group.
                    let next = n * 10 + d as usize;
                    if n != 0 && next >= caps.len() {
                        break;
                    }
                    n = next;
                    it.next();
                }
                if let Some(m) = caps.get(n) {
                    out.push_str(m.as_str());
                }
            }
            c => out.push(c),
        }
    }
    out
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// `-initWithPattern:options:error:` / `+regularExpressionWithPattern:options:error:`.
fn init(emu: &mut Emu, class_method: bool) -> Result<()> {
    let (recv, pattern, options, error_out) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4));
    let text = foundation::nsstring(emu, pattern)?;
    match compile(&text, options) {
        Ok(re) => {
            let o = if class_method {
                let cls = host_class(emu, "NSRegularExpression")?;
                objc::create_instance(emu, cls, 0)?
            } else {
                recv
            };
            let p = send(emu, pattern, "copy", &[])?;
            emu.regex.regexes.insert(o, (re, p, options));
            ret(emu, o)
        }
        Err(e) => {
            if error_out != 0 {
                let domain = foundation::new_nsstring(emu, "NSCocoaErrorDomain")?;
                let (k, v) = (foundation::new_nsstring(emu, "NSLocalizedDescription")?, foundation::new_nsstring(emu, &e)?);
                let info = foundation::new_dict_of(emu, &[(k, v)])?;
                let cls = host_class(emu, "NSError")?;
                let err = send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, 2048, info])?;
                emu.write_u64(error_out, err)?;
            }
            ret(emu, 0)
        }
    }
}

/// The receiver's regex, the string and the UTF-16 search range from x2, x4, x5.
fn subject(emu: &mut Emu, string_reg: usize, range_reg: usize) -> Result<Option<(regex::Regex, Text, usize, usize)>> {
    let re = match emu.regex.regexes.get(&emu.arg(0)) {
        Some((re, _, _)) => re.clone(),
        None => return Ok(None),
    };
    let s = emu.arg(string_reg);
    if s == 0 {
        return Ok(None);
    }
    let text = Text::new(foundation::nsstring(emu, s)?);
    let (loc, len) = (emu.arg(range_reg), emu.arg(range_reg + 1));
    let (start, end) = (text.byte(loc), text.byte(loc.saturating_add(len)));
    Ok(Some((re, text, start, end)))
}

fn matches(re: &regex::Regex, text: &Text, start: usize, end: usize) -> Vec<Vec<Option<(u64, u64)>>> {
    let hay = &text.s[..end];
    let mut out = Vec::new();
    let mut at = start;
    while at <= end {
        let Some(caps) = re.captures_at(hay, at) else { break };
        let whole = caps.get(0).unwrap();
        out.push(
            (0..caps.len())
                .map(|i| caps.get(i).map(|m| (text.unit(m.start()), text.unit(m.end()) - text.unit(m.start()))))
                .collect(),
        );
        at = if whole.end() > whole.start() { whole.end() } else { hay[whole.end()..].chars().next().map_or(end + 1, |c| whole.end() + c.len_utf8()) };
    }
    out
}

fn new_result(emu: &mut Emu, ranges: Vec<Option<(u64, u64)>>) -> Result<u64> {
    let cls = host_class(emu, "NSTextCheckingResult")?;
    let r = objc::create_instance(emu, cls, 0)?;
    emu.regex.results.insert(r, ranges);
    Ok(r)
}

fn first_match(emu: &mut Emu) -> Result<()> {
    let Some((re, text, s, e)) = subject(emu, 2, 4)? else { return ret(emu, 0) };
    let r = match matches(&re, &text, s, e).into_iter().next() {
        Some(m) => new_result(emu, m)?,
        None => 0,
    };
    ret(emu, r)
}

fn all_matches(emu: &mut Emu) -> Result<()> {
    let Some((re, text, s, e)) = subject(emu, 2, 4)? else {
        let a = foundation::new_array_of(emu, &[])?;
        return ret(emu, a);
    };
    let mut objs = Vec::new();
    for m in matches(&re, &text, s, e) {
        objs.push(new_result(emu, m)?);
    }
    let a = foundation::new_array_of(emu, &objs)?;
    ret(emu, a)
}

fn number_of_matches(emu: &mut Emu) -> Result<()> {
    let n = match subject(emu, 2, 4)? {
        Some((re, text, s, e)) => matches(&re, &text, s, e).len() as u64,
        None => 0,
    };
    ret(emu, n)
}

fn range_of_first(emu: &mut Emu) -> Result<()> {
    let r = match subject(emu, 2, 4)? {
        Some((re, text, s, e)) => matches(&re, &text, s, e).into_iter().next().and_then(|m| m[0]),
        None => None,
    };
    let (loc, len) = r.unwrap_or((NOT_FOUND, 0));
    emu.cpu.x[0] = loc;
    emu.cpu.x[1] = len;
    Ok(())
}

/// `-stringByReplacingMatchesInString:options:range:withTemplate:`
fn replace(emu: &mut Emu) -> Result<()> {
    let template = foundation::nsstring(emu, emu.arg(6))?;
    let Some((re, text, s, e)) = subject(emu, 2, 4)? else { return ret(emu, emu.arg(2)) };
    let hay = &text.s[..e];
    let mut out = String::from(&text.s[..s]);
    let mut last = s;
    let mut at = s;
    while at <= e {
        let Some(caps) = re.captures_at(hay, at) else { break };
        let m = caps.get(0).unwrap();
        out.push_str(&text.s[last..m.start()]);
        out.push_str(&expand(&template, &caps));
        last = m.end();
        at = if m.end() > m.start() { m.end() } else { hay[m.end()..].chars().next().map_or(e + 1, |c| m.end() + c.len_utf8()) };
        if m.end() == m.start() && at <= e {
            out.push_str(&text.s[m.end()..at.min(e)]);
            last = at.min(e);
        }
    }
    out.push_str(&text.s[last..]);
    let r = foundation::new_nsstring(emu, &out)?;
    ret(emu, r)
}

/// `-enumerateMatchesInString:options:range:usingBlock:` — block(result, flags, BOOL *stop).
fn enumerate(emu: &mut Emu) -> Result<()> {
    let block = emu.arg(6);
    let Some((re, text, s, e)) = subject(emu, 2, 4)? else { return Ok(()) };
    let stop = emu.alloc(8)?;
    emu.write_u64(stop, 0)?;
    for m in matches(&re, &text, s, e) {
        let r = new_result(emu, m)?;
        crate::libsystem::call_block(emu, block, &[r, 0, stop])?;
        if emu.p.memory.read_u8(stop)? != 0 {
            break;
        }
    }
    Ok(())
}

fn result_range(emu: &mut Emu, index: Option<u64>) -> Result<()> {
    let (r, i) = (emu.arg(0), index.unwrap_or(0));
    let range = emu.regex.results.get(&r).and_then(|v| v.get(i as usize).copied().flatten());
    let (loc, len) = range.unwrap_or((NOT_FOUND, 0));
    emu.cpu.x[0] = loc;
    emu.cpu.x[1] = len;
    Ok(())
}

const RE: &str = "NSRegularExpression";
const TR: &str = "NSTextCheckingResult";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (RE, true, "regularExpressionWithPattern:options:error:", |e| init(e, true)),
    (RE, false, "initWithPattern:options:error:", |e| init(e, false)),
    (RE, false, "pattern", |e| {
        let p = e.regex.regexes.get(&e.arg(0)).map_or(0, |r| r.1);
        ret(e, p)
    }),
    (RE, false, "options", |e| {
        let o = e.regex.regexes.get(&e.arg(0)).map_or(0, |r| r.2);
        ret(e, o)
    }),
    (RE, false, "numberOfCaptureGroups", |e| {
        let n = e.regex.regexes.get(&e.arg(0)).map_or(0, |r| r.0.captures_len() - 1);
        ret(e, n as u64)
    }),
    (RE, false, "firstMatchInString:options:range:", first_match),
    (RE, false, "matchesInString:options:range:", all_matches),
    (RE, false, "numberOfMatchesInString:options:range:", number_of_matches),
    (RE, false, "rangeOfFirstMatchInString:options:range:", range_of_first),
    (RE, false, "stringByReplacingMatchesInString:options:range:withTemplate:", replace),
    (RE, false, "enumerateMatchesInString:options:range:usingBlock:", enumerate),
    (TR, false, "range", |e| result_range(e, None)),
    (TR, false, "rangeAtIndex:", |e| {
        let i = e.arg(2);
        result_range(e, Some(i))
    }),
    (TR, false, "numberOfRanges", |e| {
        let n = e.regex.results.get(&e.arg(0)).map_or(0, |v| v.len());
        ret(e, n as u64)
    }),
    (TR, false, "resultType", |e| ret(e, 1 << 10)),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn replace_all(pattern: &str, options: u64, s: &str, template: &str) -> String {
        let re = compile(pattern, options).unwrap();
        re.replace_all(s, |c: &regex::Captures| expand(template, c)).into_owned()
    }

    #[test]
    fn expo_error_codes() {
        // What Expo does to turn class names into error codes.
        assert_eq!(replace_all("(.)([A-Z])", 0, "FetchUnknownException", "$1_$2"), "Fetch_Unknown_Exception");
        assert_eq!(replace_all(r"\(.+?\).+?\b\s*", 1, "(void)doThing:(id)x", ""), ":");
    }

    #[test]
    fn utf16_ranges() {
        let t = Text::new("a🍎b".into());
        assert_eq!(t.byte(1), 1);
        assert_eq!(t.byte(3), 5); // the apple is two UTF-16 units, four bytes
        assert_eq!(t.unit(5), 3);
        let re = compile("b", 0).unwrap();
        assert_eq!(matches(&re, &t, 0, t.s.len())[0][0], Some((3, 1)));
    }
}

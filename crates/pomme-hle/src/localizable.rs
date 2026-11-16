//! An app's translations: `.strings` tables (text, in UTF-8 or UTF-16, or a
//! property list, binary or XML) and `.stringsdict` plurals, as NSBundle
//! reads them (bundle.rs finds the files).
//!
//! A `.stringsdict` entry is a format with variables (`%#@n@ in %@`), each a
//! set of plural forms. NSBundle hands the format out as an NSString that
//! reads as its format key; formatting with it (`stringWithFormat:`,
//! `localizedStringWithFormat:`) picks each variable's form for its argument
//! with ICU's plural rules for the table's language (the guest's ICU, in
//! CoreFoundation). As on iOS, a `zero` form is used for 0 in any language
//! (fixtures/localized: Japanese has no zero category). A format that comes
//! back as a copy (from Swift, which bridges short strings by value) is known
//! by its format key, as the last plural format handed out with that key.

use crate::printf::Args;
use crate::{Emu, Result};
use std::collections::HashMap;

/// One `.stringsdict` variable: its argument's type (`d`, `ld`, `f`, …) and
/// its forms by plural category.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Variable {
    value_type: String,
    forms: HashMap<String, String>,
}

/// One `.stringsdict` entry, with the language whose plural rules apply.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PluralFormat {
    pub language: String,
    pub format: String,
    vars: HashMap<String, Variable>,
}

#[derive(Clone, Default)]
pub struct State {
    /// Guest strings that are plural formats (kept alive, so their addresses aren't reused).
    formats: HashMap<u64, PluralFormat>,
    /// The plural format last handed out with each format key (`%#@n@`), for
    /// a format that reaches the formatter as a copy (Swift bridges a short
    /// string to a String of its own and back to a new NSString).
    by_text: HashMap<String, PluralFormat>,
    /// ICU plural rules by language (UPluralRules*; 0: none).
    rules: HashMap<String, u64>,
}

// ---- .strings ----

/// A `.strings` table's keys and values; None if it can't be read.
pub(crate) fn parse_strings(bytes: &[u8]) -> Option<HashMap<String, String>> {
    if bytes.starts_with(b"bplist") || bytes.starts_with(b"<?xml") || bytes.starts_with(b"<plist") {
        let v = plist::Value::from_reader(std::io::Cursor::new(bytes)).ok()?;
        return Some(v.as_dictionary()?.iter().filter_map(|(k, v)| Some((k.clone(), v.as_string()?.to_string()))).collect());
    }
    parse_text(&decode(bytes)?)
}

/// Text in UTF-16 (with a byte order mark) or UTF-8.
fn decode(b: &[u8]) -> Option<String> {
    let utf16 = |be: bool| -> Option<String> {
        let units: Vec<u16> = b[2..].chunks_exact(2).map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) }).collect();
        String::from_utf16(&units).ok()
    };
    match b {
        [0xfe, 0xff, ..] => utf16(true),
        [0xff, 0xfe, ..] => utf16(false),
        [0xef, 0xbb, 0xbf, rest @ ..] => String::from_utf8(rest.to_vec()).ok(),
        _ => String::from_utf8(b.to_vec()).ok(),
    }
}

/// Old-style property list text: `"key" = "value";` pairs (or `key;`, whose
/// value is the key), with `/* */` and `//` comments, optionally in `{ }`.
fn parse_text(s: &str) -> Option<HashMap<String, String>> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = HashMap::new();
    let skip = |i: &mut usize| loop {
        while *i < c.len() && c[*i].is_whitespace() {
            *i += 1;
        }
        if c.get(*i) == Some(&'/') && c.get(*i + 1) == Some(&'*') {
            *i += 2;
            while *i + 1 < c.len() && !(c[*i] == '*' && c[*i + 1] == '/') {
                *i += 1;
            }
            *i = (*i + 2).min(c.len());
        } else if c.get(*i) == Some(&'/') && c.get(*i + 1) == Some(&'/') {
            while *i < c.len() && c[*i] != '\n' {
                *i += 1;
            }
        } else {
            break;
        }
    };
    let token = |i: &mut usize| -> Option<String> {
        if c.get(*i) == Some(&'"') {
            *i += 1;
            let mut t = String::new();
            while *i < c.len() && c[*i] != '"' {
                if c[*i] == '\\' {
                    *i += 1;
                    match c.get(*i)? {
                        'n' => t.push('\n'),
                        't' => t.push('\t'),
                        'r' => t.push('\r'),
                        'U' | 'u' => {
                            let hex: String = c[*i + 1..].iter().take(4).collect();
                            t.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?)?);
                            *i += 4;
                        }
                        d @ '0'..='7' => {
                            let oct: String = std::iter::once(*d).chain(c[*i + 1..].iter().take(2).copied().take_while(|x| ('0'..='7').contains(x))).collect();
                            *i += oct.len() - 1;
                            t.push(char::from_u32(u32::from_str_radix(&oct, 8).ok()?)?);
                        }
                        other => t.push(*other),
                    }
                } else {
                    t.push(c[*i]);
                }
                *i += 1;
            }
            *i += 1;
            Some(t)
        } else {
            let start = *i;
            while *i < c.len() && (c[*i].is_alphanumeric() || "_$+/:.-".contains(c[*i])) {
                *i += 1;
            }
            (*i > start).then(|| c[start..*i].iter().collect())
        }
    };
    skip(&mut i);
    let braced = c.get(i) == Some(&'{');
    if braced {
        i += 1;
    }
    loop {
        skip(&mut i);
        if i >= c.len() || (braced && c[i] == '}') {
            break;
        }
        let key = token(&mut i)?;
        skip(&mut i);
        let value = if c.get(i) == Some(&'=') {
            i += 1;
            skip(&mut i);
            token(&mut i)?
        } else {
            key.clone()
        };
        skip(&mut i);
        if c.get(i) == Some(&';') {
            i += 1;
        }
        out.insert(key, value);
    }
    Some(out)
}

// ---- .stringsdict ----

/// A `.stringsdict` file's entries.
pub(crate) fn parse_stringsdict(bytes: &[u8], language: &str) -> HashMap<String, PluralFormat> {
    let Ok(v) = plist::Value::from_reader(std::io::Cursor::new(bytes)) else { return HashMap::new() };
    let Some(d) = v.as_dictionary() else { return HashMap::new() };
    let mut out = HashMap::new();
    for (key, entry) in d {
        let Some(entry) = entry.as_dictionary() else { continue };
        let Some(format) = entry.get("NSStringLocalizedFormatKey").and_then(|v| v.as_string()) else { continue };
        let mut vars = HashMap::new();
        for (name, var) in entry {
            let Some(var) = var.as_dictionary() else { continue };
            if var.get("NSStringFormatSpecTypeKey").and_then(|v| v.as_string()) != Some("NSStringPluralRuleType") {
                continue;
            }
            let value_type = var.get("NSStringFormatValueTypeKey").and_then(|v| v.as_string()).unwrap_or("d").to_string();
            let forms = ["zero", "one", "two", "few", "many", "other"].iter().filter_map(|k| Some((k.to_string(), var.get(*k)?.as_string()?.to_string()))).collect();
            vars.insert(name.clone(), Variable { value_type, forms });
        }
        out.insert(key.clone(), PluralFormat { language: language.to_string(), format: format.to_string(), vars });
    }
    out
}

/// Remember that guest string `obj` is a plural format (and keep it alive).
pub(crate) fn register(emu: &mut Emu, obj: u64, format: PluralFormat) -> Result<()> {
    if !emu.localizable.formats.contains_key(&obj) {
        crate::objc::send(emu, obj, "retain", &[])?;
    }
    emu.localizable.by_text.insert(format.format.clone(), format.clone());
    emu.localizable.formats.insert(obj, format);
    Ok(())
}

/// One conversion in a format, from its `%`: its end, and its explicit position.
fn conversion(f: &[char], at: usize) -> (usize, Option<usize>) {
    let mut i = at + 1;
    let digits = f[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    let mut position = None;
    if digits > 0 && f.get(i + digits) == Some(&'$') {
        position = f[i..i + digits].iter().collect::<String>().parse().ok();
        i += digits + 1;
    }
    while i < f.len() && !"diouxXeEfFgGaAcCsSp@%".contains(f[i]) {
        // A variable: `%#@name@`.
        if f[i] == '@' && f[i - 1] == '#' {
            break;
        }
        i += 1;
    }
    if f.get(i) == Some(&'@') && f[i - 1] == '#' {
        i += 1;
        while i < f.len() && f[i] != '@' {
            i += 1;
        }
    }
    ((i + 1).min(f.len()), position)
}

/// Every conversion in `f` given its argument's position (`%d` → `%3$d`).
fn positioned(f: &str, position: usize) -> String {
    let c: Vec<char> = f.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        if c[i] != '%' {
            out.push(c[i]);
            i += 1;
            continue;
        }
        let (end, explicit) = conversion(&c, i);
        let spec: String = c[i + 1..end].iter().collect();
        if spec == "%" || explicit.is_some() {
            out.push('%');
            out.push_str(&spec);
        } else {
            out.push_str(&format!("%{position}${spec}"));
        }
        i = end;
    }
    out
}

/// The plain format for a plural format and these arguments: each variable
/// replaced by its form for its argument, every conversion with its position.
pub(crate) fn expand(emu: &mut Emu, pf: &PluralFormat, args: &Args) -> Result<String> {
    let c: Vec<char> = pf.format.chars().collect();
    let mut out = String::new();
    let mut next = 1;
    let mut i = 0;
    while i < c.len() {
        if c[i] != '%' {
            out.push(c[i]);
            i += 1;
            continue;
        }
        let (end, explicit) = conversion(&c, i);
        let spec: String = c[i + 1..end].iter().collect();
        if spec == "%" {
            out.push_str("%%");
            i = end;
            continue;
        }
        let position = explicit.unwrap_or(next);
        next = position + 1;
        if let Some(name) = spec.split_once("#@").and_then(|(_, n)| n.strip_suffix('@')) {
            let Some(var) = pf.vars.get(name) else {
                i = end;
                continue;
            };
            let raw = args.nth(emu, position)?;
            let t = &var.value_type;
            let n = if t.contains(['f', 'e', 'g', 'F', 'E', 'G']) {
                f64::from_bits(raw)
            } else if t.contains(['u', 'x', 'X', 'o']) {
                if t.contains(['l', 'q', 'z']) { raw as f64 } else { raw as u32 as f64 }
            } else if t.contains(['l', 'q', 'z']) {
                raw as i64 as f64
            } else {
                raw as i32 as f64
            };
            let category = plural_category(emu, &pf.language, n)?;
            let form = if n == 0.0 && var.forms.contains_key("zero") { "zero" } else if var.forms.contains_key(&category) { category.as_str() } else { "other" };
            out.push_str(&positioned(var.forms.get(form).map_or("", String::as_str), position));
        } else {
            out.push_str(&format!("%{position}${}", spec.trim_start_matches(|ch: char| ch.is_ascii_digit()).trim_start_matches('$')));
        }
        i = end;
    }
    Ok(out)
}

/// The format to use for guest string `fmt` with these arguments: a plural
/// format expanded, else the string itself.
pub(crate) fn format_of(emu: &mut Emu, fmt: u64, args: &Args) -> Result<String> {
    if let Some(pf) = emu.localizable.formats.get(&fmt).cloned() {
        return expand(emu, &pf, args);
    }
    let text = crate::foundation::nsstring(emu, fmt)?;
    // (`%#@` is no printf conversion: only a .stringsdict format has it.)
    if text.contains("%#@") {
        if let Some(pf) = emu.localizable.by_text.get(&text).cloned() {
            return expand(emu, &pf, args);
        }
    }
    Ok(text)
}

/// ICU's plural category (`one`, `other`, …) for `n` in `language`; `other` without ICU.
fn plural_category(emu: &mut Emu, language: &str, n: f64) -> Result<String> {
    let rules = match emu.localizable.rules.get(language) {
        Some(&r) => r,
        None => {
            let r = open_rules(emu, language).unwrap_or(0);
            emu.localizable.rules.insert(language.to_string(), r);
            r
        }
    };
    if rules == 0 {
        return Ok("other".into());
    }
    let buf = emu.alloc(64 + 8)?;
    let status = buf + 64;
    emu.write_u64(status, 0)?;
    let f = crate::foundation::cf_sym(emu, "_uplrules_select")?;
    emu.call_abi(f, &[rules, buf, 32, status], &[n.to_bits() as u128], &[], None)?;
    let len = (emu.last_x01[0] as i32).clamp(0, 32) as u64;
    let mut units = Vec::new();
    for k in 0..len {
        units.push(emu.p.memory.read_u16(buf + 2 * k)?);
    }
    Ok(String::from_utf16_lossy(&units))
}

fn open_rules(emu: &mut Emu, language: &str) -> Result<u64> {
    let id = language.replace('-', "_");
    // The status (an 8-byte slot), then the locale's C string.
    let status = emu.alloc(8 + id.len() as u64 + 1)?;
    let s = status + 8;
    emu.p.memory.write(s, id.as_bytes())?;
    emu.p.memory.write(s + id.len() as u64, &[0])?;
    emu.write_u64(status, 0)?;
    let f = crate::foundation::cf_sym(emu, "_uplrules_open")?;
    let r = emu.call(f, &[s, status])?;
    let err = emu.read_u64(status)? as i32;
    Ok(if err > 0 { 0 } else { r })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_tables() {
        let t = parse_text("/* c */\n\"greeting\" = \"Hello\";\n// line\n\"esc\" = \"a\\nb \\\"q\\\" \\U00e9\";\nbare = word;\n\"same\";").unwrap();
        assert_eq!(t["greeting"], "Hello");
        assert_eq!(t["esc"], "a\nb \"q\" é");
        assert_eq!(t["bare"], "word");
        assert_eq!(t["same"], "same");
    }

    #[test]
    fn utf16_tables() {
        let text = "\"k\" = \"写真\";";
        let mut b = vec![0xfe, 0xff];
        b.extend(text.encode_utf16().flat_map(|u| u.to_be_bytes()));
        assert_eq!(parse_strings(&b).unwrap()["k"], "写真");
        let mut b = vec![0xff, 0xfe];
        b.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
        assert_eq!(parse_strings(&b).unwrap()["k"], "写真");
    }

    #[test]
    fn property_list_tables() {
        let xml = br#"<?xml version="1.0"?><plist version="1.0"><dict><key>farewell</key><string>Goodbye</string></dict></plist>"#;
        assert_eq!(parse_strings(xml).unwrap()["farewell"], "Goodbye");
    }

    #[test]
    fn conversions_get_positions() {
        assert_eq!(positioned("%d item", 1), "%1$d item");
        assert_eq!(positioned("%ld files, 100%%", 3), "%3$ld files, 100%%");
        assert_eq!(positioned("%2$@ %d", 1), "%2$@ %1$d");
    }
}

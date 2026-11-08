//! Tokens of the Metal Shading Language subset that shader generators emit.
//! Preprocessor lines (`#include`, `#pragma`, `#ifdef` …) are skipped: the
//! generated sources only use them for headers and warnings.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Ident(String),
    /// An integer literal and whether it was unsigned (`3u`).
    Int(u64, bool),
    /// A floating-point literal (any of `1.0`, `1e-05`, `0.5h`, `2.f`).
    Float(f64),
    /// `[[`
    AttrOpen,
    /// `]]`
    AttrClose,
    Punct(&'static str),
    Eof,
}

const PUNCTS: &[&str] = &[
    "<<=", ">>=", "...", "->", "++", "--", "<<", ">>", "<=", ">=", "==", "!=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "::", "{", "}", "(", ")",
    "[", "]", ";", ",", ".", "+", "-", "*", "/", "%", "<", ">", "=", "!", "~", "&", "|", "^", "?", ":",
];

pub fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    let mut line_start = true;
    while i < b.len() {
        let c = b[i];
        if c == b'\n' {
            line_start = true;
            i += 1;
            continue;
        }
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if line_start && c == b'#' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        line_start = false;
        if b[i..].starts_with(b"//") {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i..].starts_with(b"/*") {
            let end = src[i + 2..].find("*/").ok_or("unterminated comment")?;
            i += end + 4;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let s = i;
            while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                i += 1;
            }
            out.push(Tok::Ident(src[s..i].to_string()));
            continue;
        }
        if c.is_ascii_digit() || (c == b'.' && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            let (tok, len) = number(&src[i..])?;
            out.push(tok);
            i += len;
            continue;
        }
        if b[i..].starts_with(b"[[") {
            out.push(Tok::AttrOpen);
            i += 2;
            continue;
        }
        if b[i..].starts_with(b"]]") {
            out.push(Tok::AttrClose);
            i += 2;
            continue;
        }
        let p = PUNCTS.iter().find(|p| b[i..].starts_with(p.as_bytes())).ok_or_else(|| format!("unexpected character {:?}", c as char))?;
        out.push(Tok::Punct(p));
        i += p.len();
    }
    out.push(Tok::Eof);
    Ok(out)
}

/// A numeric literal at the start of `s`, and its length.
fn number(s: &str) -> Result<(Tok, usize), String> {
    let b = s.as_bytes();
    if b.starts_with(b"0x") || b.starts_with(b"0X") {
        let mut i = 2;
        while i < b.len() && b[i].is_ascii_hexdigit() {
            i += 1;
        }
        let v = u64::from_str_radix(&s[2..i], 16).map_err(|e| e.to_string())?;
        let unsigned = b.get(i).is_some_and(|&c| c == b'u' || c == b'U');
        return Ok((Tok::Int(v, unsigned), i + unsigned as usize));
    }
    let mut i = 0;
    let mut float = false;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i < b.len() && b[i] == b'.' {
        float = true;
        i += 1;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            float = true;
            i = j;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    let text = &s[..i];
    match b.get(i) {
        Some(b'h' | b'f' | b'H' | b'F') if float || text.chars().all(|c| c.is_ascii_digit()) => {
            let v: f64 = text.parse().map_err(|e| format!("{text}: {e}"))?;
            Ok((Tok::Float(v), i + 1))
        }
        Some(b'u' | b'U') if !float => Ok((Tok::Int(text.parse().map_err(|e| format!("{text}: {e}"))?, true), i + 1)),
        _ if float => Ok((Tok::Float(text.parse().map_err(|e| format!("{text}: {e}"))?), i)),
        _ => Ok((Tok::Int(text.parse().map_err(|e| format!("{text}: {e}"))?, false), i)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let t = lex("#include <metal_stdlib>\nfloat4 p [[position]];\nx = 1e-05 + 0.5h + 3u + 2 + .5; // c\n a->b").unwrap();
        assert_eq!(t[0], Tok::Ident("float4".into()));
        assert_eq!(t[2], Tok::AttrOpen);
        assert_eq!(t[4], Tok::AttrClose);
        assert!(t.contains(&Tok::Float(1e-05)));
        assert!(t.contains(&Tok::Float(0.5)));
        assert!(t.contains(&Tok::Int(3, true)));
        assert!(t.contains(&Tok::Int(2, false)));
        assert!(t.contains(&Tok::Punct("->")));
        assert_eq!(*t.last().unwrap(), Tok::Eof);
    }
}

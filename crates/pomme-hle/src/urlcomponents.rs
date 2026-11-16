//! NSURLComponents and NSURLQueryItem: a URL taken apart and put together,
//! with each part percent-encoded as Foundation does (tests/fixtures/
//! urlcomponents, measured on the iOS 26 Simulator). React Native builds its
//! bundle URL this way.
//!
//! - A path keeps `/ : @` and the sub-delimiters; a space is `%20`.
//! - A query set whole (`query`) keeps `& = + / ? : @`; a space is `%20`.
//! - A query item's name and value also escape `&` and `=` (`c&d=e` →
//!   `c%26d%3De`), keeping `+ / ? : @`.
//! - Anything else outside the URL characters is escaped as UTF-8 (`é` →
//!   `%C3%A9`, `#` → `%23`, `%` → `%25`).

use crate::foundation;
use crate::net::percent_decode;
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// A URL's parts, the encodable ones kept percent-encoded.
#[derive(Clone, Default, Debug, PartialEq)]
pub(crate) struct Parts {
    scheme: Option<String>,
    user: Option<String>,
    password: Option<String>,
    host: Option<String>,
    port: Option<i64>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

#[derive(Clone, Default)]
pub(crate) struct State {
    components: HashMap<u64, Parts>,
    /// NSURLQueryItem → (name, value).
    items: HashMap<u64, (String, Option<String>)>,
}

fn unreserved(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.' | b'_' | b'~')
}

fn sub_delim(c: u8) -> bool {
    matches!(c, b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'=')
}

fn path_char(c: u8) -> bool {
    unreserved(c) || sub_delim(c) || matches!(c, b':' | b'@' | b'/')
}

fn query_char(c: u8) -> bool {
    path_char(c) || c == b'?'
}

fn item_char(c: u8) -> bool {
    query_char(c) && c != b'&' && c != b'='
}

fn user_char(c: u8) -> bool {
    unreserved(c) || sub_delim(c)
}

fn host_char(c: u8) -> bool {
    unreserved(c) || sub_delim(c) || matches!(c, b'[' | b']' | b':')
}

/// Escape every byte `keep` doesn't allow.
fn encode(s: &str, keep: fn(u8) -> bool) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        if keep(b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Whether text is already a valid (percent-encoded) URL: the URL characters and well-formed escapes.
fn valid_url_text(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'%' {
            if i + 2 >= b.len() || !b[i + 1].is_ascii_hexdigit() || !b[i + 2].is_ascii_hexdigit() {
                return false;
            }
            i += 3;
            continue;
        }
        if !(query_char(c) || matches!(c, b'#' | b'[' | b']')) {
            return false;
        }
        i += 1;
    }
    true
}

/// Take a URL string apart (RFC 3986); None if it isn't a valid URL string.
pub(crate) fn parse(s: &str) -> Option<Parts> {
    if !valid_url_text(s) {
        return None;
    }
    let mut p = Parts::default();
    let mut rest = s;
    if let Some((f, frag)) = rest.split_once('#') {
        p.fragment = Some(frag.to_string());
        rest = f;
    }
    if let Some((r, q)) = rest.split_once('?') {
        p.query = Some(q.to_string());
        rest = r;
    }
    if let Some(i) = rest.find(':')
        && rest[..i].bytes().enumerate().all(|(k, c)| if k == 0 { c.is_ascii_alphabetic() } else { c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.') })
        && i > 0
    {
        p.scheme = Some(rest[..i].to_string());
        rest = &rest[i + 1..];
    }
    if let Some(after) = rest.strip_prefix("//") {
        let end = after.find('/').unwrap_or(after.len());
        let (authority, path) = after.split_at(end);
        let mut hostport = authority;
        if let Some((userinfo, hp)) = authority.rsplit_once('@') {
            hostport = hp;
            match userinfo.split_once(':') {
                Some((u, pw)) => {
                    p.user = Some(u.to_string());
                    p.password = Some(pw.to_string());
                }
                None => p.user = Some(userinfo.to_string()),
            }
        }
        // (An IPv6 host is in brackets; its colons aren't the port's.)
        let port_at = if hostport.starts_with('[') { hostport.find("]:").map(|i| i + 1) } else { hostport.rfind(':') };
        match port_at {
            Some(i) if !hostport[i + 1..].is_empty() => {
                p.port = Some(hostport[i + 1..].parse().ok()?);
                p.host = Some(hostport[..i].to_string());
            }
            Some(i) => p.host = Some(hostport[..i].to_string()),
            None => p.host = Some(hostport.to_string()),
        }
        p.path = path.to_string();
    } else {
        p.path = rest.to_string();
    }
    Some(p)
}

/// Put the parts together as a URL string.
pub(crate) fn string(p: &Parts) -> String {
    let mut s = String::new();
    if let Some(scheme) = &p.scheme {
        s.push_str(scheme);
        s.push(':');
    }
    if p.host.is_some() || p.user.is_some() || p.port.is_some() {
        s.push_str("//");
        if let Some(u) = &p.user {
            s.push_str(u);
            if let Some(pw) = &p.password {
                s.push(':');
                s.push_str(pw);
            }
            s.push('@');
        }
        s.push_str(p.host.as_deref().unwrap_or(""));
        if let Some(port) = p.port {
            s.push_str(&format!(":{port}"));
        }
    }
    s.push_str(&p.path);
    if let Some(q) = &p.query {
        s.push('?');
        s.push_str(q);
    }
    if let Some(f) = &p.fragment {
        s.push('#');
        s.push_str(f);
    }
    s
}

/// A percent-encoded query, from query items (name, value).
pub(crate) fn query_from_items(items: &[(String, Option<String>)]) -> String {
    items
        .iter()
        .map(|(n, v)| match v {
            Some(v) => format!("{}={}", encode(n, item_char), encode(v, item_char)),
            None => encode(n, item_char),
        })
        .collect::<Vec<_>>()
        .join("&")
}

/// Query items (decoded) from a percent-encoded query: `z` alone has no value, `y=` an empty one.
pub(crate) fn items_from_query(q: &str) -> Vec<(String, Option<String>)> {
    q.split('&')
        .map(|part| match part.split_once('=') {
            Some((n, v)) => (percent_decode(n), Some(percent_decode(v))),
            None => (percent_decode(part), None),
        })
        .collect()
}

// ---- the Objective-C side ----

fn parts(emu: &Emu, c: u64) -> Parts {
    emu.url_components.components.get(&c).cloned().unwrap_or_default()
}

fn update(emu: &mut Emu, c: u64, f: impl FnOnce(&mut Parts)) {
    f(emu.url_components.components.entry(c).or_default());
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_str(emu: &mut Emu, s: Option<String>) -> Result<()> {
    let v = match s {
        Some(s) => foundation::new_nsstring(emu, &s)?,
        None => 0,
    };
    ret(emu, v)
}

fn arg_str(emu: &mut Emu, i: usize) -> Result<Option<String>> {
    let v = emu.arg(i);
    if v == 0 { Ok(None) } else { Ok(Some(foundation::nsstring(emu, v)?)) }
}

/// A new NSURLComponents (of `cls`) with these parts.
fn make(emu: &mut Emu, cls: u64, p: Parts) -> Result<u64> {
    let c = objc::create_instance(emu, cls, 0)?;
    emu.url_components.components.insert(c, p);
    Ok(c)
}

/// `componentsWithURL:` / `initWithURL:`: the URL's absolute string, parsed.
fn from_url(emu: &mut Emu, url: u64) -> Result<Option<Parts>> {
    if url == 0 {
        return Ok(None);
    }
    let s = send(emu, url, "absoluteString", &[])?;
    let s = foundation::nsstring(emu, s)?;
    Ok(parse(&s))
}

fn new_item(emu: &mut Emu, cls: u64, name: String, value: Option<String>) -> Result<u64> {
    let i = objc::create_instance(emu, cls, 0)?;
    emu.url_components.items.insert(i, (name, value));
    Ok(i)
}

fn item(emu: &Emu, i: u64) -> (String, Option<String>) {
    emu.url_components.items.get(&i).cloned().unwrap_or_default()
}

const C: &str = "NSURLComponents";
const Q: &str = "NSURLQueryItem";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (C, true, "componentsWithURL:resolvingAgainstBaseURL:", |e| {
        let (cls, url) = (e.arg(0), e.arg(2));
        let r = match from_url(e, url)? {
            Some(p) => make(e, cls, p)?,
            None => 0,
        };
        ret(e, r)
    }),
    (C, false, "initWithURL:resolvingAgainstBaseURL:", |e| {
        let (c, url) = (e.arg(0), e.arg(2));
        match from_url(e, url)? {
            Some(p) => {
                e.url_components.components.insert(c, p);
                ret(e, c)
            }
            None => ret(e, 0),
        }
    }),
    (C, true, "componentsWithString:", |e| {
        let cls = e.arg(0);
        let s = arg_str(e, 2)?.unwrap_or_default();
        let r = match parse(&s) {
            Some(p) => make(e, cls, p)?,
            None => 0,
        };
        ret(e, r)
    }),
    (C, false, "initWithString:", |e| {
        let c = e.arg(0);
        let s = arg_str(e, 2)?.unwrap_or_default();
        match parse(&s) {
            Some(p) => {
                e.url_components.components.insert(c, p);
                ret(e, c)
            }
            None => ret(e, 0),
        }
    }),
    (C, false, "init", |e| {
        let c = e.arg(0);
        e.url_components.components.insert(c, Parts::default());
        ret(e, c)
    }),
    (C, false, "copyWithZone:", |e| {
        let c = e.arg(0);
        let p = parts(e, c);
        let cls = host_class(e, C)?;
        let r = make(e, cls, p)?;
        ret(e, r)
    }),
    (C, false, "copy", |e| {
        let c = e.arg(0);
        let p = parts(e, c);
        let cls = host_class(e, C)?;
        let r = make(e, cls, p)?;
        ret(e, r)
    }),
    (C, false, "scheme", |e| {
        let v = parts(e, e.arg(0)).scheme;
        ret_str(e, v)
    }),
    (C, false, "setScheme:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.scheme = v);
        Ok(())
    }),
    (C, false, "user", |e| {
        let v = parts(e, e.arg(0)).user.map(|s| percent_decode(&s));
        ret_str(e, v)
    }),
    (C, false, "setUser:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.user = v.map(|s| encode(&s, user_char)));
        Ok(())
    }),
    (C, false, "password", |e| {
        let v = parts(e, e.arg(0)).password.map(|s| percent_decode(&s));
        ret_str(e, v)
    }),
    (C, false, "setPassword:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.password = v.map(|s| encode(&s, user_char)));
        Ok(())
    }),
    (C, false, "host", |e| {
        let v = parts(e, e.arg(0)).host.map(|s| percent_decode(&s));
        ret_str(e, v)
    }),
    (C, false, "setHost:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.host = v.map(|s| encode(&s, host_char)));
        Ok(())
    }),
    (C, false, "port", |e| {
        let v = match parts(e, e.arg(0)).port {
            Some(n) => {
                let number = host_class(e, "NSNumber")?;
                send(e, number, "numberWithLongLong:", &[n as u64])?
            }
            None => 0,
        };
        ret(e, v)
    }),
    (C, false, "setPort:", |e| {
        let (c, n) = (e.arg(0), e.arg(2));
        let v = if n == 0 { None } else { Some(send(e, n, "longLongValue", &[])? as i64) };
        update(e, c, |p| p.port = v);
        Ok(())
    }),
    (C, false, "path", |e| {
        let v = percent_decode(&parts(e, e.arg(0)).path);
        ret_str(e, Some(v))
    }),
    (C, false, "setPath:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.path = encode(&v.unwrap_or_default(), path_char));
        Ok(())
    }),
    (C, false, "percentEncodedPath", |e| {
        let v = parts(e, e.arg(0)).path;
        ret_str(e, Some(v))
    }),
    (C, false, "setPercentEncodedPath:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.path = v.unwrap_or_default());
        Ok(())
    }),
    (C, false, "query", |e| {
        let v = parts(e, e.arg(0)).query.map(|s| percent_decode(&s));
        ret_str(e, v)
    }),
    (C, false, "setQuery:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.query = v.map(|s| encode(&s, query_char)));
        Ok(())
    }),
    (C, false, "percentEncodedQuery", |e| {
        let v = parts(e, e.arg(0)).query;
        ret_str(e, v)
    }),
    (C, false, "setPercentEncodedQuery:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.query = v);
        Ok(())
    }),
    (C, false, "fragment", |e| {
        let v = parts(e, e.arg(0)).fragment.map(|s| percent_decode(&s));
        ret_str(e, v)
    }),
    (C, false, "setFragment:", |e| {
        let (c, v) = (e.arg(0), arg_str(e, 2)?);
        update(e, c, |p| p.fragment = v.map(|s| encode(&s, query_char)));
        Ok(())
    }),
    (C, false, "percentEncodedFragment", |e| {
        let v = parts(e, e.arg(0)).fragment;
        ret_str(e, v)
    }),
    (C, false, "queryItems", |e| {
        let Some(q) = parts(e, e.arg(0)).query else { return ret(e, 0) };
        let cls = host_class(e, Q)?;
        let mut objs = Vec::new();
        for (n, v) in items_from_query(&q) {
            objs.push(new_item(e, cls, n, v)?);
        }
        let a = foundation::array_from(e, &objs)?;
        ret(e, a)
    }),
    (C, false, "setQueryItems:", |e| {
        let (c, list) = (e.arg(0), e.arg(2));
        let query = if list == 0 {
            None
        } else {
            let mut items = Vec::new();
            for o in foundation::array_values(e, list)? {
                let name = send(e, o, "name", &[])?;
                let value = send(e, o, "value", &[])?;
                let name = if name == 0 { String::new() } else { foundation::nsstring(e, name)? };
                let value = if value == 0 { None } else { Some(foundation::nsstring(e, value)?) };
                items.push((name, value));
            }
            Some(query_from_items(&items))
        };
        update(e, c, |p| p.query = query);
        Ok(())
    }),
    (C, false, "string", |e| {
        let v = string(&parts(e, e.arg(0)));
        ret_str(e, Some(v))
    }),
    (C, false, "URL", |e| {
        let s = string(&parts(e, e.arg(0)));
        let ns = foundation::new_nsstring(e, &s)?;
        let cls = host_class(e, "NSURL")?;
        let u = send(e, cls, "URLWithString:", &[ns])?;
        ret(e, u)
    }),
    (C, false, "description", |e| {
        let s = format!("<NSURLComponents {:#x}> {}", e.arg(0), string(&parts(e, e.arg(0))));
        ret_str(e, Some(s))
    }),
    // NSURLQueryItem
    (Q, true, "queryItemWithName:value:", |e| {
        let cls = e.arg(0);
        let (n, v) = (arg_str(e, 2)?.unwrap_or_default(), arg_str(e, 3)?);
        let i = new_item(e, cls, n, v)?;
        ret(e, i)
    }),
    (Q, false, "initWithName:value:", |e| {
        let i = e.arg(0);
        let (n, v) = (arg_str(e, 2)?.unwrap_or_default(), arg_str(e, 3)?);
        e.url_components.items.insert(i, (n, v));
        ret(e, i)
    }),
    (Q, false, "name", |e| {
        let (n, _) = item(e, e.arg(0));
        ret_str(e, Some(n))
    }),
    (Q, false, "value", |e| {
        let (_, v) = item(e, e.arg(0));
        ret_str(e, v)
    }),
    (Q, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = a == b || (e.url_components.items.contains_key(&b) && item(e, a) == item(e, b));
        ret(e, same as u64)
    }),
    (Q, false, "hash", |e| {
        let (n, v) = item(e, e.arg(0));
        let h = n.bytes().chain(v.unwrap_or_default().bytes()).fold(5381u64, |h, b| h.wrapping_mul(33) ^ b as u64);
        ret(e, h)
    }),
    (Q, false, "copyWithZone:", |e| {
        let i = e.arg(0);
        send(e, i, "retain", &[])?;
        ret(e, i)
    }),
    (Q, false, "description", |e| {
        let (n, v) = item(e, e.arg(0));
        let s = format!("<NSURLQueryItem {:#x}> {{name = {n}, value = {}}}", e.arg(0), v.as_deref().unwrap_or("(null)"));
        ret_str(e, Some(s))
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_items_encode_as_measured() {
        let items = vec![
            ("platform".into(), Some("ios".into())),
            ("a b".into(), Some("c&d=e".into())),
            ("plus".into(), Some("1+2".into())),
            ("slash".into(), Some("x/y?z:w@v".into())),
            ("novalue".into(), None),
            ("empty".into(), Some("".into())),
            ("accent".into(), Some("é#%".into())),
        ];
        assert_eq!(query_from_items(&items), "platform=ios&a%20b=c%26d%3De&plus=1+2&slash=x/y?z:w@v&novalue&empty=&accent=%C3%A9%23%25");
    }

    #[test]
    fn parses_as_measured() {
        let p = parse("http://user:pw@example.com:8081/a/b%20c?x=1&y=two%20words#frag").unwrap();
        assert_eq!(p.scheme.as_deref(), Some("http"));
        assert_eq!((p.user.as_deref(), p.password.as_deref()), (Some("user"), Some("pw")));
        assert_eq!((p.host.as_deref(), p.port), (Some("example.com"), Some(8081)));
        assert_eq!(p.path, "/a/b%20c");
        assert_eq!(percent_decode(&p.path), "/a/b c");
        assert_eq!(items_from_query("x=1&y=&z"), vec![("x".into(), Some("1".into())), ("y".into(), Some("".into())), ("z".into(), None)]);
        assert_eq!(string(&p), "http://user:pw@example.com:8081/a/b%20c?x=1&y=two%20words#frag");
        assert!(parse("http://exa mple.com").is_none());
    }

    #[test]
    fn a_whole_query_keeps_its_separators() {
        assert_eq!(encode("k=v w&x=y+z", query_char), "k=v%20w&x=y+z");
        assert_eq!(encode("/p q", path_char), "/p%20q");
    }
}

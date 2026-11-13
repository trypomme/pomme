//! NSJSONSerialization: JSON to and from Foundation objects.
//!
//! Writing walks the object graph (strings, numbers, booleans, NSNull,
//! arrays, dictionaries) the way Apple's does, escaping `/` unless asked not
//! to. Reading parses with serde_json and builds CoreFoundation objects.

use crate::foundation::{self, nsstring};
use crate::objc::{host_class, send};
use crate::{Emu, HostFn, Result};

// NSJSONWritingOptions
const PRETTY: u64 = 1;
const SORTED_KEYS: u64 = 2;
const WITHOUT_ESCAPING_SLASHES: u64 = 8;

/// A JSON value read out of guest objects.
#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

fn is_kind(emu: &mut Emu, obj: u64, class: &str) -> Result<bool> {
    let cls = host_class(emu, class)?;
    Ok(send(emu, obj, "isKindOfClass:", &[cls])? & 0xff != 0)
}

/// Read a guest object graph as JSON (None: something isn't serializable).
fn read(emu: &mut Emu, obj: u64, depth: usize) -> Result<Option<Json>> {
    if obj == 0 || depth > 512 {
        return Ok(None);
    }
    if obj == emu.fnd.null_obj || is_kind(emu, obj, "NSNull")? {
        return Ok(Some(Json::Null));
    }
    if is_kind(emu, obj, "NSString")? {
        return Ok(Some(Json::Str(nsstring(emu, obj)?)));
    }
    if is_kind(emu, obj, "NSNumber")? {
        if foundation::is_boolean(emu, obj) {
            return Ok(Some(Json::Bool(send(emu, obj, "boolValue", &[])? & 0xff != 0)));
        }
        if foundation::number_is_float(emu, obj)? {
            send(emu, obj, "doubleValue", &[])?;
            let v = f64::from_bits(emu.last_q0 as u64);
            return Ok(v.is_finite().then_some(Json::Float(v)));
        }
        return Ok(Some(Json::Int(send(emu, obj, "longLongValue", &[])? as i64)));
    }
    if is_kind(emu, obj, "NSArray")? {
        let mut out = Vec::new();
        for o in foundation::array_values(emu, obj)? {
            let Some(v) = read(emu, o, depth + 1)? else { return Ok(None) };
            out.push(v);
        }
        return Ok(Some(Json::Array(out)));
    }
    if is_kind(emu, obj, "NSDictionary")? {
        let d = foundation::as_cf_dict(emu, obj)?;
        let mut out = Vec::new();
        for (k, v) in foundation::dict_pairs(emu, d)? {
            if !is_kind(emu, k, "NSString")? {
                return Ok(None); // keys must be strings
            }
            let key = nsstring(emu, k)?;
            let Some(v) = read(emu, v, depth + 1)? else { return Ok(None) };
            out.push((key, v));
        }
        return Ok(Some(Json::Object(out)));
    }
    Ok(None)
}

fn escape(s: &str, slashes: bool, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '/' if slashes => out.push_str("\\/"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Serialize like NSJSONSerialization (compact, or pretty with two-space indents).
pub fn write(v: &Json, options: u64) -> String {
    fn go(v: &Json, options: u64, indent: usize, out: &mut String) {
        let pretty = options & PRETTY != 0;
        let slashes = options & WITHOUT_ESCAPING_SLASHES == 0;
        let nl = |out: &mut String, n: usize| {
            if pretty {
                out.push('\n');
                out.push_str(&"  ".repeat(n));
            }
        };
        match v {
            Json::Null => out.push_str("null"),
            Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Json::Int(i) => out.push_str(&i.to_string()),
            Json::Float(f) => {
                if f.fract() == 0.0 && f.abs() < 1e15 {
                    out.push_str(&(*f as i64).to_string());
                } else {
                    out.push_str(&f.to_string());
                }
            }
            Json::Str(s) => escape(s, slashes, out),
            Json::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    nl(out, indent + 1);
                    go(item, options, indent + 1, out);
                }
                if !items.is_empty() {
                    nl(out, indent);
                }
                out.push(']');
            }
            Json::Object(pairs) => {
                let mut pairs: Vec<&(String, Json)> = pairs.iter().collect();
                if options & SORTED_KEYS != 0 {
                    pairs.sort_by(|a, b| a.0.cmp(&b.0));
                }
                out.push('{');
                for (i, (k, val)) in pairs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    nl(out, indent + 1);
                    escape(k, slashes, out);
                    out.push_str(if pretty { " : " } else { ":" });
                    go(val, options, indent + 1, out);
                }
                if !pairs.is_empty() {
                    nl(out, indent);
                }
                out.push('}');
            }
        }
    }
    let mut out = String::new();
    go(v, options, 0, &mut out);
    out
}

/// Build guest objects from parsed JSON.
pub(crate) fn build(emu: &mut Emu, v: &serde_json::Value) -> Result<u64> {
    use serde_json::Value;
    Ok(match v {
        Value::Null => emu.fnd.null_obj,
        Value::Bool(b) => foundation::boolean(emu, *b),
        Value::Number(n) => match n.as_i64() {
            Some(i) => foundation::new_number_i64(emu, i)?,
            None => foundation::new_number_f64(emu, n.as_f64().unwrap_or(0.0))?,
        },
        Value::String(s) => foundation::new_nsstring(emu, s)?,
        Value::Array(items) => {
            let mut objs = Vec::with_capacity(items.len());
            for i in items {
                objs.push(build(emu, i)?);
            }
            foundation::new_array_of(emu, &objs)?
        }
        Value::Object(map) => {
            let mut pairs = Vec::with_capacity(map.len());
            for (k, val) in map {
                let k = foundation::new_nsstring(emu, k)?;
                pairs.push((k, build(emu, val)?));
            }
            foundation::new_dict_of(emu, &pairs)?
        }
    })
}

fn set_error(emu: &mut Emu, out: u64, code: u64, message: &str) -> Result<()> {
    if out == 0 {
        return Ok(());
    }
    let domain = foundation::new_nsstring(emu, "NSCocoaErrorDomain")?;
    let (k, v) = (foundation::new_nsstring(emu, "NSDebugDescription")?, foundation::new_nsstring(emu, message)?);
    let info = foundation::new_dict_of(emu, &[(k, v)])?;
    let cls = host_class(emu, "NSError")?;
    let err = send(emu, cls, "errorWithDomain:code:userInfo:", &[domain, code, info])?;
    emu.write_u64(out, err)
}

/// `+dataWithJSONObject:options:error:`
fn data_with_object(emu: &mut Emu) -> Result<()> {
    let (obj, options, error_out) = (emu.arg(2), emu.arg(3), emu.arg(4));
    match read(emu, obj, 0)? {
        Some(v) => {
            let text = write(&v, options);
            let d = foundation::new_data(emu, text.as_bytes())?;
            emu.ret(d);
        }
        None => {
            set_error(emu, error_out, 3851, "Invalid type in JSON write")?;
            emu.ret(0);
        }
    }
    Ok(())
}

/// `+JSONObjectWithData:options:error:`
fn object_with_data(emu: &mut Emu) -> Result<()> {
    let (data, error_out) = (emu.arg(2), emu.arg(4));
    let bytes = if data == 0 { Vec::new() } else { foundation::data_bytes(emu, data)? };
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(v) => {
            let o = build(emu, &v)?;
            emu.ret(o);
        }
        Err(e) => {
            set_error(emu, error_out, 3840, &e.to_string())?;
            emu.ret(0);
        }
    }
    Ok(())
}

fn is_valid(emu: &mut Emu) -> Result<()> {
    let ok = read(emu, emu.arg(2), 0)?.is_some();
    emu.ret(ok as u64);
    Ok(())
}

const J: &str = "NSJSONSerialization";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (J, true, "dataWithJSONObject:options:error:", data_with_object),
    (J, true, "JSONObjectWithData:options:error:", object_with_data),
    (J, true, "isValidJSONObject:", is_valid),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_like_foundation() {
        let v = Json::Object(vec![
            ("url".into(), Json::Str("https://a/b".into())),
            ("n".into(), Json::Array(vec![Json::Int(1), Json::Float(2.5), Json::Float(3.0), Json::Bool(true), Json::Null])),
        ]);
        assert_eq!(write(&v, 0), r#"{"url":"https:\/\/a\/b","n":[1,2.5,3,true,null]}"#);
        assert_eq!(write(&v, WITHOUT_ESCAPING_SLASHES | SORTED_KEYS), r#"{"n":[1,2.5,3,true,null],"url":"https://a/b"}"#);
        assert_eq!(write(&Json::Array(vec![Json::Int(1)]), PRETTY), "[\n  1\n]");
        assert_eq!(write(&Json::Str("a\"\n\u{1}".into()), 0), r#""a\"\n\u0001""#);
    }
}

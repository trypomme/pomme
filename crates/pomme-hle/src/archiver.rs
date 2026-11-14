//! NSKeyedArchiver and NSKeyedUnarchiver, writing and reading Apple's keyed
//! archive format: a binary property list with `$archiver`, `$version`,
//! `$top` (the root object's UID) and `$objects` (every object once, `$null`
//! first). Strings, numbers and data sit in `$objects` as themselves; other
//! objects are dictionaries of their keyed values with a `$class` UID naming
//! a `{$classname, $classes}` record.
//!
//! Foundation's own classes are stored as Foundation stores them (`NS.objects`
//! for arrays and sets, `NS.keys`/`NS.objects` for dictionaries, `NS.time` for
//! dates, `NS.base`/`NS.relative` for URLs, `NS.uuidbytes` for UUIDs, NSNull
//! as just its class); anything else goes through its own
//! `encodeWithCoder:` / `initWithCoder:` with the coder's keyed methods.

use crate::foundation::{self, nsstring};
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use plist::{Dictionary, Uid, Value};
use std::collections::HashMap;

/// An archive being written.
#[derive(Clone, Default)]
struct Writing {
    objects: Vec<Value>,
    /// Objects already archived → their UID.
    ids: HashMap<u64, u64>,
    /// Class records by class name → their UID.
    classes: HashMap<String, u64>,
    /// The dictionaries being filled by encodeWithCoder: (the top-level one first).
    stack: Vec<Dictionary>,
    data: u64,
}

/// An archive being read.
#[derive(Clone)]
struct Reading {
    objects: Vec<Value>,
    /// Objects already made → the object.
    made: HashMap<u64, u64>,
    /// The dictionaries initWithCoder: reads from (`$top` first).
    stack: Vec<Dictionary>,
}

#[derive(Clone, Default)]
pub struct State {
    writers: HashMap<u64, Writing>,
    readers: HashMap<u64, Reading>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn is_kind(emu: &mut Emu, obj: u64, class: &str) -> Result<bool> {
    let cls = host_class(emu, class)?;
    Ok(obj != 0 && objc::is_kind_of(emu, obj, cls)?)
}

/// `*out` (if given) = an NSCocoaErrorDomain error.
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

// ---- writing ----

fn writer(emu: &mut Emu, a: u64) -> &mut Writing {
    emu.archiver.writers.entry(a).or_insert_with(|| Writing { objects: vec![Value::String("$null".into())], stack: vec![Dictionary::new()], ..Default::default() })
}

/// The UID of the class record for `name` (and its superclasses).
fn class_uid(emu: &mut Emu, a: u64, name: &str, chain: Vec<String>) -> u64 {
    let w = writer(emu, a);
    if let Some(&u) = w.classes.get(name) {
        return u;
    }
    let mut d = Dictionary::new();
    d.insert("$classname".into(), Value::String(name.into()));
    d.insert("$classes".into(), Value::Array(chain.into_iter().map(Value::String).collect()));
    let u = w.objects.len() as u64;
    w.objects.push(Value::Dictionary(d));
    w.classes.insert(name.into(), u);
    u
}

/// A Foundation class's record: the class and its superclasses up to NSObject.
fn foundation_class(emu: &mut Emu, a: u64, chain: &[&str]) -> u64 {
    class_uid(emu, a, chain[0], chain.iter().map(|s| s.to_string()).collect())
}

/// An object's own class record, by walking its superclasses.
fn object_class(emu: &mut Emu, a: u64, obj: u64) -> Result<u64> {
    let mut c = objc::class_of(emu, obj)?;
    let mut chain = Vec::new();
    while c != 0 {
        chain.push(emu.objc.class_name(c));
        c = emu.objc.classes.get(&c).map_or(0, |k| k.superclass);
    }
    let name = chain.first().cloned().unwrap_or_else(|| "NSObject".into());
    Ok(class_uid(emu, a, &name, chain))
}

/// Archive `obj` (and what it holds): its UID.
fn encode(emu: &mut Emu, a: u64, obj: u64) -> Result<u64> {
    if obj == 0 {
        return Ok(0);
    }
    if let Some(&u) = writer(emu, a).ids.get(&obj) {
        return Ok(u);
    }
    // Reserve the object's place first, so cycles find it.
    let u = {
        let w = writer(emu, a);
        let u = w.objects.len() as u64;
        w.objects.push(Value::String("$null".into()));
        w.ids.insert(obj, u);
        u
    };
    let value = if is_kind(emu, obj, "NSString")? {
        Value::String(nsstring(emu, obj)?)
    } else if is_kind(emu, obj, "NSNumber")? {
        if foundation::is_boolean(emu, obj) {
            Value::Boolean(send(emu, obj, "boolValue", &[])? & 0xff != 0)
        } else if foundation::number_is_float(emu, obj)? {
            send(emu, obj, "doubleValue", &[])?;
            Value::Real(f64::from_bits(emu.last_q0 as u64))
        } else {
            Value::Integer((send(emu, obj, "longLongValue", &[])? as i64).into())
        }
    } else if is_kind(emu, obj, "NSData")? {
        Value::Data(foundation::data_bytes(emu, obj)?)
    } else {
        Value::Dictionary(encode_object(emu, a, obj)?)
    };
    writer(emu, a).objects[u as usize] = value;
    Ok(u)
}

fn uids(emu: &mut Emu, a: u64, objs: Vec<u64>) -> Result<Value> {
    let mut v = Vec::new();
    for o in objs {
        v.push(Value::Uid(Uid::new(encode(emu, a, o)?)));
    }
    Ok(Value::Array(v))
}

/// The dictionary for an object that isn't a plist scalar.
fn encode_object(emu: &mut Emu, a: u64, obj: u64) -> Result<Dictionary> {
    let mut d = Dictionary::new();
    let class = if is_kind(emu, obj, "NSArray")? {
        let items = foundation::array_values(emu, obj)?;
        d.insert("NS.objects".into(), uids(emu, a, items)?);
        foundation_class(emu, a, &["NSArray", "NSObject"])
    } else if is_kind(emu, obj, "NSDictionary")? {
        let cf_dict = foundation::as_cf_dict(emu, obj)?;
        let pairs = foundation::dict_pairs(emu, cf_dict)?;
        let (keys, values): (Vec<u64>, Vec<u64>) = pairs.into_iter().unzip();
        d.insert("NS.keys".into(), uids(emu, a, keys)?);
        d.insert("NS.objects".into(), uids(emu, a, values)?);
        foundation_class(emu, a, &["NSDictionary", "NSObject"])
    } else if is_kind(emu, obj, "NSSet")? {
        let all = send(emu, obj, "allObjects", &[])?;
        let items = foundation::array_values(emu, all)?;
        d.insert("NS.objects".into(), uids(emu, a, items)?);
        foundation_class(emu, a, &["NSSet", "NSObject"])
    } else if is_kind(emu, obj, "NSDate")? {
        d.insert("NS.time".into(), Value::Real(foundation::date_value(emu, obj)?));
        foundation_class(emu, a, &["NSDate", "NSObject"])
    } else if obj == emu.fnd.null_obj || is_kind(emu, obj, "NSNull")? {
        foundation_class(emu, a, &["NSNull", "NSObject"])
    } else if is_kind(emu, obj, "NSURL")? {
        let s = send(emu, obj, "absoluteString", &[])?;
        d.insert("NS.base".into(), Value::Uid(Uid::new(0)));
        d.insert("NS.relative".into(), Value::Uid(Uid::new(encode(emu, a, s)?)));
        foundation_class(emu, a, &["NSURL", "NSObject"])
    } else if is_kind(emu, obj, "NSUUID")? {
        let buf = emu.alloc(16)?;
        send(emu, obj, "getUUIDBytes:", &[buf])?;
        d.insert("NS.uuidbytes".into(), Value::Data(emu.p.memory.read(buf, 16)?.to_vec()));
        foundation_class(emu, a, &["NSUUID", "NSObject"])
    } else {
        // The object archives itself into a dictionary of its own.
        writer(emu, a).stack.push(Dictionary::new());
        send(emu, obj, "encodeWithCoder:", &[a])?;
        d = writer(emu, a).stack.pop().unwrap_or_default();
        object_class(emu, a, obj)?
    };
    d.insert("$class".into(), Value::Uid(Uid::new(class)));
    Ok(d)
}

/// Put `value` under `key` in the dictionary being filled.
fn put(emu: &mut Emu, a: u64, key: u64, value: Value) -> Result<()> {
    let key = nsstring(emu, key)?;
    if let Some(d) = writer(emu, a).stack.last_mut() {
        d.insert(key, value);
    }
    Ok(())
}

/// The archive's bytes.
fn finish(emu: &mut Emu, a: u64) -> Result<Vec<u8>> {
    let w = writer(emu, a);
    let top = w.stack.first().cloned().unwrap_or_default();
    let mut root = Dictionary::new();
    root.insert("$archiver".into(), Value::String("NSKeyedArchiver".into()));
    root.insert("$version".into(), Value::Integer(100000.into()));
    root.insert("$top".into(), Value::Dictionary(top));
    root.insert("$objects".into(), Value::Array(w.objects.clone()));
    let mut out = Vec::new();
    Value::Dictionary(root).to_writer_binary(&mut out).map_err(|e| crate::Stop::Unsupported(format!("writing a keyed archive: {e}")))?;
    Ok(out)
}

fn new_archiver(emu: &mut Emu) -> Result<u64> {
    let cls = host_class(emu, "NSKeyedArchiver")?;
    let a = objc::create_instance(emu, cls, 0)?;
    writer(emu, a);
    Ok(a)
}

/// `+archivedDataWithRootObject:requiringSecureCoding:error:` (and the old form).
/// (An object that can't be archived stops the run where Foundation would raise.)
fn archive_root(emu: &mut Emu) -> Result<()> {
    let root = emu.arg(2);
    let a = new_archiver(emu)?;
    let u = encode(emu, a, root)?;
    writer(emu, a).stack[0].insert("root".into(), Value::Uid(Uid::new(u)));
    let bytes = finish(emu, a)?;
    emu.archiver.writers.remove(&a);
    let d = foundation::new_data(emu, &bytes)?;
    ret(emu, d)
}

// ---- reading ----

fn uid_of(v: &Value) -> Option<u64> {
    match v {
        Value::Uid(u) => Some(u.get()),
        _ => None,
    }
}

/// The class names of an object's `$class` record, most specific first.
fn class_names(r: &Reading, d: &Dictionary) -> Vec<String> {
    let rec = d.get("$class").and_then(uid_of).and_then(|u| r.objects.get(u as usize)).and_then(Value::as_dictionary);
    let Some(rec) = rec else { return Vec::new() };
    match rec.get("$classes").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(|v| v.as_string().map(str::to_string)).collect(),
        None => rec.get("$classname").and_then(Value::as_string).map(|s| vec![s.to_string()]).unwrap_or_default(),
    }
}

/// The object with UID `u`, made on first use.
fn decode(emu: &mut Emu, rd: u64, u: u64) -> Result<u64> {
    if u == 0 {
        return Ok(0);
    }
    if let Some(&o) = emu.archiver.readers.get(&rd).and_then(|r| r.made.get(&u)) {
        return Ok(o);
    }
    let Some(v) = emu.archiver.readers.get(&rd).and_then(|r| r.objects.get(u as usize)).cloned() else { return Ok(0) };
    let o = match v {
        Value::String(s) if s == "$null" => 0,
        Value::String(s) => foundation::new_nsstring(emu, &s)?,
        Value::Boolean(b) => foundation::boolean(emu, b),
        Value::Integer(i) => foundation::new_number_i64(emu, i.as_signed().unwrap_or(i.as_unsigned().unwrap_or(0) as i64))?,
        Value::Real(f) => foundation::new_number_f64(emu, f)?,
        Value::Data(d) => foundation::new_data(emu, &d)?,
        Value::Dictionary(d) => decode_object(emu, rd, u, &d)?,
        _ => 0,
    };
    if let Some(r) = emu.archiver.readers.get_mut(&rd) {
        r.made.insert(u, o);
    }
    Ok(o)
}

fn decode_all(emu: &mut Emu, rd: u64, list: Option<&Value>) -> Result<Vec<u64>> {
    let mut out = Vec::new();
    for v in list.and_then(Value::as_array).cloned().unwrap_or_default() {
        if let Some(u) = uid_of(&v) {
            out.push(decode(emu, rd, u)?);
        }
    }
    Ok(out)
}

fn decode_object(emu: &mut Emu, rd: u64, u: u64, d: &Dictionary) -> Result<u64> {
    let names = emu.archiver.readers.get(&rd).map(|r| class_names(r, d)).unwrap_or_default();
    let kind = |n: &str| names.iter().any(|x| x == n);
    if kind("NSArray") {
        let items = decode_all(emu, rd, d.get("NS.objects"))?;
        return foundation::new_array_of(emu, &items);
    }
    if kind("NSDictionary") {
        let keys = decode_all(emu, rd, d.get("NS.keys"))?;
        let values = decode_all(emu, rd, d.get("NS.objects"))?;
        let pairs: Vec<(u64, u64)> = keys.into_iter().zip(values).collect();
        return foundation::new_dict_of(emu, &pairs);
    }
    if kind("NSSet") {
        let items = decode_all(emu, rd, d.get("NS.objects"))?;
        let a = foundation::new_array_of(emu, &items)?;
        let cls = host_class(emu, "NSMutableSet")?;
        return send(emu, cls, "setWithArray:", &[a]);
    }
    if kind("NSString") {
        let s = d.get("NS.string").and_then(Value::as_string).unwrap_or("").to_string();
        return if kind("NSMutableString") {
            let cls = host_class(emu, "NSMutableString")?;
            let s = foundation::new_nsstring(emu, &s)?;
            send(emu, cls, "stringWithString:", &[s])
        } else {
            foundation::new_nsstring(emu, &s)
        };
    }
    if kind("NSData") {
        let bytes = d.get("NS.data").and_then(Value::as_data).unwrap_or(&[]).to_vec();
        return foundation::new_data(emu, &bytes);
    }
    if kind("NSDate") {
        let t = d.get("NS.time").and_then(Value::as_real).unwrap_or(0.0);
        return foundation::new_date(emu, t);
    }
    if kind("NSNull") {
        return Ok(emu.fnd.null_obj);
    }
    if kind("NSURL") {
        let rel = d.get("NS.relative").and_then(uid_of).unwrap_or(0);
        let s = decode(emu, rd, rel)?;
        let cls = host_class(emu, "NSURL")?;
        return if s == 0 { Ok(0) } else { send(emu, cls, "URLWithString:", &[s]) };
    }
    if kind("NSUUID") {
        let bytes = d.get("NS.uuidbytes").and_then(Value::as_data).unwrap_or(&[0; 16]).to_vec();
        let buf = emu.alloc(16)?;
        emu.p.memory.write(buf, &bytes[..16.min(bytes.len())])?;
        let cls = host_class(emu, "NSUUID")?;
        let o = send(emu, cls, "alloc", &[])?;
        return send(emu, o, "initWithUUIDBytes:", &[buf]);
    }
    // Another class: the first of its names that exists here makes itself.
    let mut cls = 0;
    for n in &names {
        cls = objc::class_by_name(emu, n)?;
        if cls != 0 {
            break;
        }
    }
    if cls == 0 {
        return Ok(0);
    }
    let o = send(emu, cls, "alloc", &[])?;
    if let Some(r) = emu.archiver.readers.get_mut(&rd) {
        r.made.insert(u, o);
        r.stack.push(d.clone());
    }
    let made = send(emu, o, "initWithCoder:", &[rd])?;
    if let Some(r) = emu.archiver.readers.get_mut(&rd) {
        r.stack.pop();
    }
    Ok(made)
}

/// An unarchiver for `data` (an NSData), or None if it isn't a keyed archive.
fn new_unarchiver(emu: &mut Emu, data: u64) -> Result<Option<u64>> {
    if data == 0 {
        return Ok(None);
    }
    let bytes = foundation::data_bytes(emu, data)?;
    let Ok(Value::Dictionary(root)) = plist::from_bytes::<Value>(&bytes) else { return Ok(None) };
    if root.get("$archiver").and_then(Value::as_string) != Some("NSKeyedArchiver") {
        return Ok(None);
    }
    let objects = root.get("$objects").and_then(Value::as_array).cloned().unwrap_or_default();
    let top = root.get("$top").and_then(Value::as_dictionary).cloned().unwrap_or_default();
    let cls = host_class(emu, "NSKeyedUnarchiver")?;
    let rd = objc::create_instance(emu, cls, 0)?;
    emu.archiver.readers.insert(rd, Reading { objects, made: HashMap::new(), stack: vec![top] });
    Ok(Some(rd))
}

/// The root object of `data`, or nil with `*error` set.
fn unarchive_root(emu: &mut Emu, data: u64, error: u64) -> Result<()> {
    let Some(rd) = new_unarchiver(emu, data)? else {
        set_error(emu, error, 4864, "The data couldn’t be read because it isn’t in the correct format.")?;
        return ret(emu, 0);
    };
    let root = current_value(emu, rd, "root").and_then(|v| uid_of(&v)).unwrap_or(0);
    let o = decode(emu, rd, root)?;
    emu.archiver.readers.remove(&rd);
    if o == 0 {
        set_error(emu, error, 4864, "The archive has no root object.")?;
    }
    ret(emu, o)
}

/// A value of the dictionary being read.
fn current_value(emu: &Emu, rd: u64, key: &str) -> Option<Value> {
    emu.archiver.readers.get(&rd)?.stack.last()?.get(key).cloned()
}

fn key_value(emu: &mut Emu) -> Result<Option<Value>> {
    let (rd, key) = (emu.arg(0), emu.arg(2));
    let key = nsstring(emu, key)?;
    Ok(current_value(emu, rd, &key))
}

fn int_value(v: Option<Value>) -> i64 {
    match v {
        Some(Value::Integer(i)) => i.as_signed().unwrap_or(0),
        Some(Value::Real(f)) => f as i64,
        Some(Value::Boolean(b)) => b as i64,
        _ => 0,
    }
}

fn real_value(v: Option<Value>) -> f64 {
    match v {
        Some(Value::Real(f)) => f,
        Some(Value::Integer(i)) => i.as_signed().unwrap_or(0) as f64,
        _ => 0.0,
    }
}

fn decode_for_key(emu: &mut Emu) -> Result<()> {
    let rd = emu.arg(0);
    let u = key_value(emu)?.and_then(|v| uid_of(&v)).unwrap_or(0);
    let o = decode(emu, rd, u)?;
    ret(emu, o)
}

const KA: &str = "NSKeyedArchiver";
const KU: &str = "NSKeyedUnarchiver";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // Writing.
    (KA, true, "archivedDataWithRootObject:requiringSecureCoding:error:", archive_root),
    (KA, true, "archivedDataWithRootObject:", archive_root),
    (KA, false, "initRequiringSecureCoding:", |e| {
        let a = e.arg(0);
        writer(e, a);
        ret(e, a)
    }),
    (KA, false, "init", |e| {
        let a = e.arg(0);
        writer(e, a);
        ret(e, a)
    }),
    (KA, false, "encodeObject:forKey:", |e| {
        let (a, o, k) = (e.arg(0), e.arg(2), e.arg(3));
        let u = encode(e, a, o)?;
        put(e, a, k, Value::Uid(Uid::new(u)))
    }),
    (KA, false, "encodeConditionalObject:forKey:", |e| {
        let (a, o, k) = (e.arg(0), e.arg(2), e.arg(3));
        let u = encode(e, a, o)?;
        put(e, a, k, Value::Uid(Uid::new(u)))
    }),
    (KA, false, "encodeBool:forKey:", |e| {
        let (a, b, k) = (e.arg(0), e.arg(2) & 0xff != 0, e.arg(3));
        put(e, a, k, Value::Boolean(b))
    }),
    (KA, false, "encodeInt64:forKey:", |e| {
        let (a, v, k) = (e.arg(0), e.arg(2) as i64, e.arg(3));
        put(e, a, k, Value::Integer(v.into()))
    }),
    (KA, false, "encodeInteger:forKey:", |e| {
        let (a, v, k) = (e.arg(0), e.arg(2) as i64, e.arg(3));
        put(e, a, k, Value::Integer(v.into()))
    }),
    (KA, false, "encodeInt32:forKey:", |e| {
        let (a, v, k) = (e.arg(0), e.arg(2) as i32 as i64, e.arg(3));
        put(e, a, k, Value::Integer(v.into()))
    }),
    (KA, false, "encodeInt:forKey:", |e| {
        let (a, v, k) = (e.arg(0), e.arg(2) as i32 as i64, e.arg(3));
        put(e, a, k, Value::Integer(v.into()))
    }),
    // The double/float is in d0/s0, so the key is x2.
    (KA, false, "encodeDouble:forKey:", |e| {
        let (a, v, k) = (e.arg(0), f64::from_bits(e.cpu.q[0] as u64), e.arg(2));
        put(e, a, k, Value::Real(v))
    }),
    (KA, false, "encodeFloat:forKey:", |e| {
        let (a, v, k) = (e.arg(0), f32::from_bits(e.cpu.q[0] as u32), e.arg(2));
        put(e, a, k, Value::Real(v as f64))
    }),
    (KA, false, "encodeBytes:length:forKey:", |e| {
        let (a, p, n, k) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let bytes = if n == 0 { Vec::new() } else { e.p.memory.read(p, n as usize)?.to_vec() };
        put(e, a, k, Value::Data(bytes))
    }),
    (KA, false, "allowsKeyedCoding", |e| ret(e, 1)),
    (KA, false, "requiresSecureCoding", |e| ret(e, 1)),
    (KA, false, "setRequiresSecureCoding:", |_| Ok(())),
    (KA, false, "setOutputFormat:", |_| Ok(())),
    (KA, false, "finishEncoding", |e| {
        let a = e.arg(0);
        let bytes = finish(e, a)?;
        let d = foundation::new_data(e, &bytes)?;
        writer(e, a).data = d;
        Ok(())
    }),
    (KA, false, "encodedData", |e| {
        let a = e.arg(0);
        let mut d = writer(e, a).data;
        if d == 0 {
            let bytes = finish(e, a)?;
            d = foundation::new_data(e, &bytes)?;
            writer(e, a).data = d;
        }
        ret(e, d)
    }),
    // Reading.
    (KU, true, "unarchivedObjectOfClass:fromData:error:", |e| {
        let (d, err) = (e.arg(3), e.arg(4));
        unarchive_root(e, d, err)
    }),
    (KU, true, "unarchivedObjectOfClasses:fromData:error:", |e| {
        let (d, err) = (e.arg(3), e.arg(4));
        unarchive_root(e, d, err)
    }),
    (KU, true, "unarchivedArrayOfObjectsOfClass:fromData:error:", |e| {
        let (d, err) = (e.arg(3), e.arg(4));
        unarchive_root(e, d, err)
    }),
    (KU, true, "unarchivedDictionaryWithKeysOfClass:objectsOfClass:fromData:error:", |e| {
        let (d, err) = (e.arg(4), e.arg(5));
        unarchive_root(e, d, err)
    }),
    (KU, true, "unarchiveObjectWithData:", |e| {
        let d = e.arg(2);
        unarchive_root(e, d, 0)
    }),
    (KU, true, "unarchiveTopLevelObjectWithData:error:", |e| {
        let (d, err) = (e.arg(2), e.arg(3));
        unarchive_root(e, d, err)
    }),
    (KU, false, "initForReadingFromData:error:", |e| {
        let (d, err) = (e.arg(2), e.arg(3));
        match new_unarchiver(e, d)? {
            Some(rd) => ret(e, rd),
            None => {
                set_error(e, err, 4864, "The data couldn’t be read because it isn’t in the correct format.")?;
                ret(e, 0)
            }
        }
    }),
    (KU, false, "initForReadingWithData:", |e| {
        let d = e.arg(2);
        let rd = new_unarchiver(e, d)?.unwrap_or(0);
        ret(e, rd)
    }),
    (KU, false, "decodeObjectForKey:", decode_for_key),
    (KU, false, "decodeObjectOfClass:forKey:", |e| {
        e.cpu.x[2] = e.cpu.x[3];
        decode_for_key(e)
    }),
    (KU, false, "decodeObjectOfClasses:forKey:", |e| {
        e.cpu.x[2] = e.cpu.x[3];
        decode_for_key(e)
    }),
    (KU, false, "decodeArrayOfObjectsOfClass:forKey:", |e| {
        e.cpu.x[2] = e.cpu.x[3];
        decode_for_key(e)
    }),
    (KU, false, "decodeTopLevelObjectForKey:error:", decode_for_key),
    (KU, false, "containsValueForKey:", |e| {
        let has = key_value(e)?.is_some();
        ret(e, has as u64)
    }),
    (KU, false, "decodeBoolForKey:", |e| {
        let v = key_value(e)?;
        ret(e, (int_value(v) != 0) as u64)
    }),
    (KU, false, "decodeInt64ForKey:", |e| {
        let v = key_value(e)?;
        ret(e, int_value(v) as u64)
    }),
    (KU, false, "decodeIntegerForKey:", |e| {
        let v = key_value(e)?;
        ret(e, int_value(v) as u64)
    }),
    (KU, false, "decodeInt32ForKey:", |e| {
        let v = key_value(e)?;
        ret(e, int_value(v) as i32 as u32 as u64)
    }),
    (KU, false, "decodeIntForKey:", |e| {
        let v = key_value(e)?;
        ret(e, int_value(v) as i32 as u32 as u64)
    }),
    (KU, false, "decodeDoubleForKey:", |e| {
        let v = key_value(e)?;
        e.cpu.q[0] = real_value(v).to_bits() as u128;
        Ok(())
    }),
    (KU, false, "decodeFloatForKey:", |e| {
        let v = key_value(e)?;
        e.cpu.q[0] = (real_value(v) as f32).to_bits() as u128;
        Ok(())
    }),
    // (key, NSUInteger *length): the bytes live as long as the unarchiver.
    (KU, false, "decodeBytesForKey:returnedLength:", |e| {
        let out = e.arg(3);
        let bytes = match key_value(e)? {
            Some(Value::Data(d)) => d,
            _ => Vec::new(),
        };
        if out != 0 {
            e.write_u64(out, bytes.len() as u64)?;
        }
        if bytes.is_empty() {
            return ret(e, 0);
        }
        let p = e.alloc(bytes.len() as u64)?;
        e.p.memory.write(p, &bytes)?;
        ret(e, p)
    }),
    (KU, false, "allowsKeyedCoding", |e| ret(e, 1)),
    (KU, false, "requiresSecureCoding", |e| ret(e, 1)),
    (KU, false, "setRequiresSecureCoding:", |_| Ok(())),
    (KU, false, "decodingFailurePolicy", |e| ret(e, 1)),
    (KU, false, "setDecodingFailurePolicy:", |_| Ok(())),
    (KU, false, "error", |e| ret(e, 0)),
    (KU, false, "finishDecoding", |e| {
        let rd = e.arg(0);
        e.archiver.readers.remove(&rd);
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uids_and_class_names() {
        let mut objects = vec![Value::String("$null".into())];
        let mut rec = Dictionary::new();
        rec.insert("$classname".into(), Value::String("NSMutableArray".into()));
        rec.insert("$classes".into(), Value::Array(vec![Value::String("NSMutableArray".into()), Value::String("NSArray".into())]));
        objects.push(Value::Dictionary(rec));
        let r = Reading { objects, made: HashMap::new(), stack: vec![] };
        let mut d = Dictionary::new();
        d.insert("$class".into(), Value::Uid(Uid::new(1)));
        assert_eq!(class_names(&r, &d), ["NSMutableArray", "NSArray"]);
        assert_eq!(uid_of(&Value::Uid(Uid::new(7))), Some(7));
    }
}

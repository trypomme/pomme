//! Property lists: NSPropertyListSerialization, and the NSDictionary and
//! NSArray methods that read and write plist files
//! (`dictionaryWithContentsOfFile:`, `writeToFile:atomically:`, ...).
//!
//! Reading is CoreFoundation's CFPropertyList, as under Foundation: XML,
//! binary and OpenStep plists, with Foundation's mutability options
//! (immutable, mutable containers, mutable containers and leaves). Writing
//! walks the objects with messages (so the compiler's constant literals count
//! as the dictionaries, arrays and numbers they are) and writes XML or binary;
//! OpenStep can't be written, as in Foundation. Like Foundation's, the `…WithContentsOf…` methods give nil when the
//! file is missing, isn't a plist, or holds something other than the class
//! asked for, and sent to NSMutableDictionary or NSMutableArray they give
//! mutable containers.

use crate::foundation::{self, cf_call, nsstring};
use crate::objc::{host_class, send};
use plist::Value;
use crate::{Emu, HostFn, Result};

// NSPropertyListMutabilityOptions (CFPropertyListMutabilityOptions)
const MUTABLE_CONTAINERS: u64 = 1;

// NSPropertyListFormat
const XML: u64 = 100;
const BINARY: u64 = 200;

/// Seconds from 1970 to 2001 (NSDate's reference date).
const REFERENCE_1970: f64 = 978_307_200.0;

// NSCocoaErrorDomain codes
const READ_CORRUPT: u64 = 3840;
const WRITE_INVALID: u64 = 3851;
const FILE_READ_NO_SUCH_FILE: u64 = 260;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// `*out` (if given) = an NSCocoaErrorDomain error with `code` and a description.
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

/// Parse `data` (an NSData): the property list and its format, or None.
fn parse(emu: &mut Emu, data: u64, options: u64) -> Result<Option<(u64, u64)>> {
    if data == 0 {
        return Ok(None);
    }
    let format = emu.alloc(8)?;
    emu.write_u64(format, 0)?;
    let plist = cf_call(emu, "_CFPropertyListCreateWithData", &[0, data, options, format, 0])?;
    if plist == 0 {
        return Ok(None);
    }
    Ok(Some((plist, emu.read_u64(format)?)))
}

fn is_kind(emu: &mut Emu, obj: u64, class: &str) -> Result<bool> {
    let cls = host_class(emu, class)?;
    Ok(send(emu, obj, "isKindOfClass:", &[cls])? & 0xff != 0)
}

/// A guest object graph as a property list value (None: something in it isn't
/// a plist type, or a dictionary key isn't a string).
fn value(emu: &mut Emu, obj: u64, depth: usize) -> Result<Option<Value>> {
    if obj == 0 || depth > 512 {
        return Ok(None);
    }
    if is_kind(emu, obj, "NSString")? {
        return Ok(Some(Value::String(nsstring(emu, obj)?)));
    }
    if is_kind(emu, obj, "NSNumber")? {
        if foundation::is_boolean(emu, obj) {
            return Ok(Some(Value::Boolean(send(emu, obj, "boolValue", &[])? & 0xff != 0)));
        }
        if foundation::number_is_float(emu, obj)? {
            send(emu, obj, "doubleValue", &[])?;
            return Ok(Some(Value::Real(f64::from_bits(emu.last_q0 as u64))));
        }
        return Ok(Some(Value::Integer((send(emu, obj, "longLongValue", &[])? as i64).into())));
    }
    if is_kind(emu, obj, "NSData")? {
        return Ok(Some(Value::Data(foundation::data_bytes(emu, obj)?)));
    }
    if is_kind(emu, obj, "NSDate")? {
        let since_1970 = foundation::date_value(emu, obj)? + REFERENCE_1970;
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(since_1970.max(0.0));
        return Ok(Some(Value::Date(t.into())));
    }
    if is_kind(emu, obj, "NSArray")? {
        let mut out = Vec::new();
        for o in foundation::array_values(emu, obj)? {
            let Some(v) = value(emu, o, depth + 1)? else { return Ok(None) };
            out.push(v);
        }
        return Ok(Some(Value::Array(out)));
    }
    if is_kind(emu, obj, "NSDictionary")? {
        let d = foundation::as_cf_dict(emu, obj)?;
        let mut out = plist::Dictionary::new();
        for (k, v) in foundation::dict_pairs(emu, d)? {
            if !is_kind(emu, k, "NSString")? {
                return Ok(None);
            }
            let key = nsstring(emu, k)?;
            let Some(v) = value(emu, v, depth + 1)? else { return Ok(None) };
            out.insert(key, v);
        }
        out.sort_keys();
        return Ok(Some(Value::Dictionary(out)));
    }
    Ok(None)
}

/// `plist` written in `format` (XML or binary): its bytes, or None.
fn serialize_bytes(emu: &mut Emu, plist: u64, format: u64) -> Result<Option<Vec<u8>>> {
    if format != XML && format != BINARY {
        return Ok(None);
    }
    let Some(v) = value(emu, plist, 0)? else { return Ok(None) };
    let mut out = Vec::new();
    let ok = if format == XML { v.to_writer_xml(&mut out) } else { v.to_writer_binary(&mut out) };
    Ok(ok.ok().map(|_| out))
}

/// `plist` written in `format`: an NSData, or None.
fn serialize(emu: &mut Emu, plist: u64, format: u64) -> Result<Option<u64>> {
    match serialize_bytes(emu, plist, format)? {
        Some(b) => Ok(Some(foundation::new_data(emu, &b)?)),
        None => Ok(None),
    }
}

/// `+propertyListWithData:options:format:error:`
fn with_data(emu: &mut Emu) -> Result<()> {
    let (data, options, format_out, error) = (emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    match parse(emu, data, options)? {
        Some((plist, format)) => {
            if format_out != 0 {
                emu.write_u64(format_out, format)?;
            }
            ret(emu, plist)
        }
        None => {
            set_error(emu, error, READ_CORRUPT, "The data couldn’t be read because it isn’t in the correct format.")?;
            ret(emu, 0)
        }
    }
}

/// `+dataWithPropertyList:format:options:error:`
fn data_with(emu: &mut Emu) -> Result<()> {
    let (plist, format, error) = (emu.arg(2), emu.arg(3), emu.arg(5));
    match serialize(emu, plist, format)? {
        Some(d) => ret(emu, d),
        None => {
            set_error(emu, error, WRITE_INVALID, "Property list invalid for format")?;
            ret(emu, 0)
        }
    }
}

/// `+propertyList:isValidForFormat:`
fn is_valid(emu: &mut Emu) -> Result<()> {
    let (plist, format) = (emu.arg(2), emu.arg(3));
    let ok = (format == XML || format == BINARY) && value(emu, plist, 0)?.is_some();
    ret(emu, ok as u64)
}

/// The path of `url` (a file URL).
fn url_path(emu: &mut Emu, url: u64) -> Result<Option<String>> {
    if url == 0 {
        return Ok(None);
    }
    let p = send(emu, url, "path", &[])?;
    Ok(if p == 0 { None } else { Some(nsstring(emu, p)?) })
}

/// The plist in the file at `path` when it's a `type_fn` (CFDictionary or
/// CFArray), mutable when the receiver is `mutable`'s class; else nil, with
/// `*error` (if given) saying why.
fn read_file(emu: &mut Emu, path: Option<String>, type_fn: &'static str, mutable: &str, error: u64) -> Result<()> {
    let receiver = emu.arg(0);
    let options = if foundation::wants_mutable(emu, receiver, mutable)? { MUTABLE_CONTAINERS } else { 0 };
    let Some(bytes) = path.and_then(|p| crate::fs::read_file(emu, &p)) else {
        set_error(emu, error, FILE_READ_NO_SUCH_FILE, "The file doesn’t exist.")?;
        return ret(emu, 0);
    };
    let data = foundation::new_data(emu, &bytes)?;
    let plist = match parse(emu, data, options)? {
        Some((p, _)) => p,
        None => 0,
    };
    let want = cf_call(emu, type_fn, &[])?;
    if plist == 0 || cf_call(emu, "_CFGetTypeID", &[plist])? != want {
        set_error(emu, error, READ_CORRUPT, "The data couldn’t be read because it isn’t in the correct format.")?;
        return ret(emu, 0);
    }
    ret(emu, plist)
}

/// `-writeToFile:atomically:` (and the URL forms): the receiver as an XML plist.
fn write_file(emu: &mut Emu, path: Option<String>, error: u64) -> Result<()> {
    let plist = emu.arg(0);
    let ok = match (path, serialize_bytes(emu, plist, XML)?) {
        (Some(p), Some(bytes)) => crate::fs::write_data_file(emu, &p, &bytes),
        _ => false,
    };
    if !ok {
        set_error(emu, error, WRITE_INVALID, "The file couldn’t be saved.")?;
    }
    ret(emu, ok as u64)
}

fn path_arg(emu: &mut Emu) -> Result<Option<String>> {
    let p = emu.arg(2);
    Ok(if p == 0 { None } else { Some(nsstring(emu, p)?) })
}

fn url_arg(emu: &mut Emu) -> Result<Option<String>> {
    let u = emu.arg(2);
    url_path(emu, u)
}

const PS: &str = "NSPropertyListSerialization";
const D: &str = "NSDictionary";
const A: &str = "NSArray";
const DICT: &str = "_CFDictionaryGetTypeID";
const ARRAY: &str = "_CFArrayGetTypeID";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (PS, true, "propertyListWithData:options:format:error:", with_data),
    (PS, true, "propertyListFromData:mutabilityOption:format:errorDescription:", |e| {
        // The old form: *errorDescription would be an NSString; leave it unset.
        e.cpu.x[5] = 0;
        with_data(e)
    }),
    (PS, true, "dataWithPropertyList:format:options:error:", data_with),
    (PS, true, "dataFromPropertyList:format:errorDescription:", |e| {
        e.cpu.x[5] = 0;
        data_with(e)
    }),
    (PS, true, "propertyList:isValidForFormat:", is_valid),
    (D, true, "dictionaryWithContentsOfFile:", |e| {
        let p = path_arg(e)?;
        read_file(e, p, DICT, "NSMutableDictionary", 0)
    }),
    (D, false, "initWithContentsOfFile:", |e| {
        let p = path_arg(e)?;
        read_file(e, p, DICT, "NSMutableDictionary", 0)
    }),
    (D, true, "dictionaryWithContentsOfURL:", |e| {
        let p = url_arg(e)?;
        read_file(e, p, DICT, "NSMutableDictionary", 0)
    }),
    (D, false, "initWithContentsOfURL:", |e| {
        let p = url_arg(e)?;
        read_file(e, p, DICT, "NSMutableDictionary", 0)
    }),
    (D, true, "dictionaryWithContentsOfURL:error:", |e| {
        let (p, err) = (url_arg(e)?, e.arg(3));
        read_file(e, p, DICT, "NSMutableDictionary", err)
    }),
    (D, false, "initWithContentsOfURL:error:", |e| {
        let (p, err) = (url_arg(e)?, e.arg(3));
        read_file(e, p, DICT, "NSMutableDictionary", err)
    }),
    (A, true, "arrayWithContentsOfFile:", |e| {
        let p = path_arg(e)?;
        read_file(e, p, ARRAY, "NSMutableArray", 0)
    }),
    (A, false, "initWithContentsOfFile:", |e| {
        let p = path_arg(e)?;
        read_file(e, p, ARRAY, "NSMutableArray", 0)
    }),
    (A, true, "arrayWithContentsOfURL:", |e| {
        let p = url_arg(e)?;
        read_file(e, p, ARRAY, "NSMutableArray", 0)
    }),
    (A, false, "initWithContentsOfURL:", |e| {
        let p = url_arg(e)?;
        read_file(e, p, ARRAY, "NSMutableArray", 0)
    }),
    (A, true, "arrayWithContentsOfURL:error:", |e| {
        let (p, err) = (url_arg(e)?, e.arg(3));
        read_file(e, p, ARRAY, "NSMutableArray", err)
    }),
    (A, false, "initWithContentsOfURL:error:", |e| {
        let (p, err) = (url_arg(e)?, e.arg(3));
        read_file(e, p, ARRAY, "NSMutableArray", err)
    }),
    (D, false, "writeToFile:atomically:", |e| {
        let p = path_arg(e)?;
        write_file(e, p, 0)
    }),
    (D, false, "writeToURL:atomically:", |e| {
        let p = url_arg(e)?;
        write_file(e, p, 0)
    }),
    (D, false, "writeToURL:error:", |e| {
        let (p, err) = (url_arg(e)?, e.arg(3));
        write_file(e, p, err)
    }),
    (A, false, "writeToFile:atomically:", |e| {
        let p = path_arg(e)?;
        write_file(e, p, 0)
    }),
    (A, false, "writeToURL:atomically:", |e| {
        let p = url_arg(e)?;
        write_file(e, p, 0)
    }),
    (A, false, "writeToURL:error:", |e| {
        let (p, err) = (url_arg(e)?, e.arg(3));
        write_file(e, p, err)
    }),
];

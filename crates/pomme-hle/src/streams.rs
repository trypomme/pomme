//! NSInputStream and NSOutputStream: CoreFoundation's CFReadStream and
//! CFWriteStream (real code in the sysroot), toll-free bridged as on iOS to
//! `__NSCFInputStream` and `__NSCFOutputStream`, whose methods call the CF
//! functions on themselves. A stream's delegate hears CF's client events as
//! `stream:handleEvent:` (the NSStreamEvent values are CF's event flags).
//!
//! React Native reads a development bundle from Metro through one (a
//! multipart response turned into a stream), and its WebSocket library
//! (SocketRocket) talks to Metro through a pair.

use crate::foundation::{cf, cf_sym};
use crate::objc::{self, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

#[derive(Clone, Default)]
pub(crate) struct State {
    /// stream → its delegate (absent: the stream is its own, which ignores events).
    delegates: HashMap<u64, u64>,
}

/// Every CF stream event: open completed, has bytes, can accept bytes, error, end.
const ALL_EVENTS: u64 = 1 | 2 | 4 | 8 | 16;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// Whether `s` is a read stream (else a write stream).
fn is_read(emu: &mut Emu, s: u64) -> Result<bool> {
    let id = cf(emu, "_CFGetTypeID", &[s])?;
    Ok(id == cf(emu, "_CFReadStreamGetTypeID", &[])?)
}

/// `name` for the stream's kind: `_CFReadStream…` or `_CFWriteStream…`.
fn call(emu: &mut Emu, s: u64, read: &'static str, write: &'static str, args: &[u64]) -> Result<u64> {
    let name = if is_read(emu, s)? { read } else { write };
    let mut all = vec![s];
    all.extend_from_slice(args);
    cf(emu, name, &all)
}

/// CF's client callback: (stream, event, info) → the delegate's `stream:handleEvent:`.
fn client_callback(emu: &mut Emu) -> Result<()> {
    let (s, event) = (emu.arg(0), emu.arg(1));
    if let Some(&d) = emu.streams.delegates.get(&s)
        && d != 0
        && d != s
    {
        let sel = objc::intern(emu, "stream:handleEvent:")?;
        if objc::responds(emu, d, sel)? {
            send(emu, d, "stream:handleEvent:", &[s, event])?;
        }
    }
    Ok(())
}

/// Have CF tell us about the stream's events (once a delegate is set).
fn set_client(emu: &mut Emu, s: u64) -> Result<()> {
    let callback = emu.host_thunk(client_callback)?;
    // CFStreamClientContext {version, info, retain, release, copyDescription}
    let ctx = emu.alloc(40)?;
    emu.write_u64(ctx + 8, s)?;
    call(emu, s, "_CFReadStreamSetClient", "_CFWriteStreamSetClient", &[ALL_EVENTS, callback, ctx])?;
    Ok(())
}

/// A read stream over a copy of these bytes (CF reads them in place, so they
/// stay for the stream's life: pomme's heap keeps them).
pub(crate) fn input_stream_with_bytes(emu: &mut Emu, bytes: &[u8]) -> Result<u64> {
    let buf = emu.alloc(bytes.len().max(1) as u64)?;
    emu.p.memory.write(buf, bytes)?;
    let null = cf_sym(emu, "_kCFAllocatorNull")?;
    let null = emu.read_u64(null)?;
    cf(emu, "_CFReadStreamCreateWithBytesNoCopy", &[0, buf, bytes.len() as u64, null])
}

const I: &str = "__NSCFInputStream";
const O: &str = "__NSCFOutputStream";

/// Methods both kinds have, on a class.
macro_rules! common {
    ($c:expr) => {
        [
            ($c, false, "open", |e| {
                let s = e.arg(0);
                call(e, s, "_CFReadStreamOpen", "_CFWriteStreamOpen", &[])?;
                Ok(())
            }),
            ($c, false, "close", |e| {
                let s = e.arg(0);
                call(e, s, "_CFReadStreamClose", "_CFWriteStreamClose", &[])?;
                Ok(())
            }),
            ($c, false, "streamStatus", |e| {
                let s = e.arg(0);
                let v = call(e, s, "_CFReadStreamGetStatus", "_CFWriteStreamGetStatus", &[])?;
                ret(e, v)
            }),
            ($c, false, "streamError", |e| {
                let s = e.arg(0);
                let v = call(e, s, "_CFReadStreamCopyError", "_CFWriteStreamCopyError", &[])?;
                let v = if v != 0 { send(e, v, "autorelease", &[])? } else { 0 };
                ret(e, v)
            }),
            ($c, false, "propertyForKey:", |e| {
                let (s, k) = (e.arg(0), e.arg(2));
                let v = call(e, s, "_CFReadStreamCopyProperty", "_CFWriteStreamCopyProperty", &[k])?;
                let v = if v != 0 { send(e, v, "autorelease", &[])? } else { 0 };
                ret(e, v)
            }),
            ($c, false, "setProperty:forKey:", |e| {
                let (s, v, k) = (e.arg(0), e.arg(2), e.arg(3));
                let ok = call(e, s, "_CFReadStreamSetProperty", "_CFWriteStreamSetProperty", &[k, v])?;
                ret(e, ok & 0xff)
            }),
            ($c, false, "scheduleInRunLoop:forMode:", |e| {
                let (s, rl, mode) = (e.arg(0), e.arg(2), e.arg(3));
                let cfrl = send(e, rl, "getCFRunLoop", &[])?;
                call(e, s, "_CFReadStreamScheduleWithRunLoop", "_CFWriteStreamScheduleWithRunLoop", &[cfrl, mode])?;
                Ok(())
            }),
            ($c, false, "removeFromRunLoop:forMode:", |e| {
                let (s, rl, mode) = (e.arg(0), e.arg(2), e.arg(3));
                let cfrl = send(e, rl, "getCFRunLoop", &[])?;
                call(e, s, "_CFReadStreamUnscheduleFromRunLoop", "_CFWriteStreamUnscheduleFromRunLoop", &[cfrl, mode])?;
                Ok(())
            }),
            ($c, false, "delegate", |e| {
                let s = e.arg(0);
                let d = e.streams.delegates.get(&s).copied().unwrap_or(s);
                ret(e, d)
            }),
            // (The delegate isn't retained, as NSStream's isn't.)
            ($c, false, "setDelegate:", |e| {
                let (s, d) = (e.arg(0), e.arg(2));
                let first = !e.streams.delegates.contains_key(&s);
                e.streams.delegates.insert(s, if d == 0 { s } else { d });
                if first {
                    set_client(e, s)?;
                }
                Ok(())
            }),
        ]
    };
}

const INPUT_COMMON: [(&str, bool, &str, HostFn); 10] = common!(I);
const OUTPUT_COMMON: [(&str, bool, &str, HostFn); 10] = common!(O);

const OWN: &[(&str, bool, &str, HostFn)] = &[
    (I, false, "read:maxLength:", |e| {
        let (s, buf, len) = (e.arg(0), e.arg(2), e.arg(3));
        let n = cf(e, "_CFReadStreamRead", &[s, buf, len])?;
        ret(e, n)
    }),
    (I, false, "hasBytesAvailable", |e| {
        let s = e.arg(0);
        let v = cf(e, "_CFReadStreamHasBytesAvailable", &[s])?;
        ret(e, v & 0xff)
    }),
    (I, false, "getBuffer:length:", |e| ret(e, 0)),
    (O, false, "write:maxLength:", |e| {
        let (s, buf, len) = (e.arg(0), e.arg(2), e.arg(3));
        let n = cf(e, "_CFWriteStreamWrite", &[s, buf, len])?;
        ret(e, n)
    }),
    (O, false, "hasSpaceAvailable", |e| {
        let s = e.arg(0);
        let v = cf(e, "_CFWriteStreamCanAcceptBytes", &[s])?;
        ret(e, v & 0xff)
    }),
    // Making one: over data (CF's bytes stream).
    ("NSInputStream", true, "inputStreamWithData:", |e| {
        let d = e.arg(2);
        let bytes = crate::foundation::data_bytes(e, d)?;
        let s = input_stream_with_bytes(e, &bytes)?;
        let s = send(e, s, "autorelease", &[])?;
        ret(e, s)
    }),
    ("NSInputStream", false, "initWithData:", |e| {
        let d = e.arg(2);
        let bytes = crate::foundation::data_bytes(e, d)?;
        let s = input_stream_with_bytes(e, &bytes)?;
        ret(e, s)
    }),
];

pub(crate) fn methods() -> Vec<(&'static str, bool, &'static str, HostFn)> {
    INPUT_COMMON.iter().chain(OUTPUT_COMMON.iter()).chain(OWN.iter()).copied().collect()
}

/// The bridged classes, for foundation's toll-free table.
pub(crate) const BRIDGED: &[(&str, &str)] = &[("_CFReadStreamGetTypeID", I), ("_CFWriteStreamGetTypeID", O)];

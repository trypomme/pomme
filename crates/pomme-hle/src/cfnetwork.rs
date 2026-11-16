//! CFNetwork's socket streams: what `CFStreamCreatePairWithSocketToHost` and
//! friends give, over pomme's sockets.
//!
//! CoreFoundation (the sysroot's, from swift-corelibs-foundation) implements
//! CFStream itself (clients, run loops, dispatch queues, events) and asks
//! CFNetwork for socket streams: it `dlopen`s CFNetwork and looks up
//! `_CFSocketStreamCreatePair`, `_CFErrorCreateWithStreamError` and
//! `_CFStreamErrorFromCFError`. pomme provides those. A pair is a read and a
//! write stream (`CFReadStreamCreate`/`CFWriteStreamCreate` with callbacks
//! here) over one TCP connection.
//!
//! As in CFNetwork:
//! - Opening either stream connects (to the host's addresses in turn, or the
//!   signature's address).
//! - A network thread tells the streams when bytes arrive
//!   (`kCFStreamEventHasBytesAvailable`) and when they can take more
//!   (`kCFStreamEventCanAcceptBytes`). Each event comes once until the stream is
//!   read or written again.
//! - Reads and writes block until they can proceed.
//! - The connection closes when both streams have.

use crate::sockets::Pending;
use crate::threads::Wait;
use crate::{Emu, HostFn, Result, Stop};
use std::collections::HashMap;
use std::net::SocketAddr;

// CFStreamEventType
const HAS_BYTES_AVAILABLE: u64 = 2;
const CAN_ACCEPT_BYTES: u64 = 4;

const POLLIN: u16 = 0x1;
const POLLOUT: u16 = 0x4;

/// What a read or write callback's first step returns when it has to wait.
const RETRY: u64 = -2i64 as u64;

/// What the network thread's step returns: stop, or call the signal in the frame.
const PUMP_STOP: u64 = 0;
const PUMP_SIGNAL: u64 = 3;

/// `kCFHostErrorHostNotFound`, in `kCFErrorDomainCFNetwork`.
const HOST_NOT_FOUND: u64 = 1;

// A pair's frame in guest memory (also the streams' info pointer):
//   0  the network thread's call: function, then three arguments
//  32  its pollfds: the connection, then its wake-up descriptor
//  48  the pollfd a waiting read polls
//  56  the pollfd a waiting write polls
const FRAME: u64 = 64;
const PUMP_FDS: u64 = 32;
const READ_FD: u64 = 48;
const WRITE_FD: u64 = 56;

#[derive(Clone)]
struct Pair {
    host: Option<String>,
    port: u16,
    /// A signature's address, for `CFStreamCreatePairWithPeerSocketSignature`.
    target: Option<SocketAddr>,
    fd: Option<u64>,
    /// Whether closing the streams closes the socket (`kCFStreamPropertyShouldCloseNativeSocket`).
    owns_fd: bool,
    /// Connecting failed: the error, as (CFNetwork domain?, code).
    failed: Option<(bool, u64)>,
    /// The streams (0 once finalized), whether each is open, and whether each
    /// is to hear its next event.
    read: u64,
    write: u64,
    read_open: bool,
    write_open: bool,
    read_armed: bool,
    write_armed: bool,
    /// The network thread's wake-up descriptors (it polls the first).
    wake: Option<(u64, u64)>,
    pump_running: bool,
}

#[derive(Clone, Default)]
pub(crate) struct State {
    pairs: HashMap<u64, Pair>,
    /// CFHTTPMessages, by the CF object that stands for each.
    messages: HashMap<u64, HttpMessage>,
    read_callbacks: u64,
    write_callbacks: u64,
    pump_code: u64,
}

fn pair(emu: &mut Emu, frame: u64) -> Result<&mut Pair> {
    emu.cfnetwork.pairs.get_mut(&frame).ok_or_else(|| Stop::Unsupported(format!("CFNetwork: no socket stream pair at {frame:#x}")))
}

// ---- errors ----

/// A CFError for errno `e`, or a CFNetwork error code.
fn new_error(emu: &mut Emu, cfnetwork: bool, code: u64) -> Result<u64> {
    let domain = if cfnetwork { crate::foundation::new_nsstring(emu, "kCFErrorDomainCFNetwork")? } else { crate::foundation::cf_data_pub(emu, "_kCFErrorDomainPOSIX")? };
    crate::foundation::cf_call(emu, "_CFErrorCreate", &[0, domain, code, 0])
}

/// `_CFErrorCreateWithStreamError(alloc, const CFStreamError *)`
fn error_from_stream_error(emu: &mut Emu) -> Result<()> {
    let (alloc, p) = (emu.arg(0), emu.arg(1));
    let domain = emu.read_u64(p)? as i64;
    let code = emu.p.memory.read_u32(p + 8)? as i32 as i64 as u64;
    let d = match domain {
        1 => crate::foundation::cf_data_pub(emu, "_kCFErrorDomainPOSIX")?,
        2 => crate::foundation::cf_data_pub(emu, "_kCFErrorDomainOSStatus")?,
        _ => crate::foundation::new_nsstring(emu, "kCFErrorDomainCFNetwork")?,
    };
    let e = crate::foundation::cf_call(emu, "_CFErrorCreate", &[alloc, d, code, 0])?;
    emu.ret(e);
    Ok(())
}

/// `_CFStreamErrorFromCFError(error)`: a `CFStreamError` {domain, error}, in x0 and x1.
fn stream_error_from_error(emu: &mut Emu) -> Result<()> {
    let e = emu.arg(0);
    let domain = crate::foundation::cf_call(emu, "_CFErrorGetDomain", &[e])?;
    let code = crate::foundation::cf_call(emu, "_CFErrorGetCode", &[e])?;
    let posix = crate::foundation::cf_data_pub(emu, "_kCFErrorDomainPOSIX")?;
    let status = crate::foundation::cf_data_pub(emu, "_kCFErrorDomainOSStatus")?;
    let d = if crate::foundation::cf_call(emu, "_CFEqual", &[domain, posix])? & 0xff != 0 {
        1
    } else if crate::foundation::cf_call(emu, "_CFEqual", &[domain, status])? & 0xff != 0 {
        2
    } else {
        // kCFStreamErrorDomainNetDB, as CFNetwork's host errors map.
        12
    };
    emu.ret(d);
    emu.cpu.x[1] = code & 0xffff_ffff;
    Ok(())
}

// ---- creating a pair ----

/// Guest code that retries `attempt(args)` after `wait(args)` while it returns RETRY.
fn retry_trampoline(emu: &mut Emu, attempt: HostFn, wait: HostFn) -> Result<u64> {
    let (a, w) = (emu.host_thunk(attempt)?, emu.host_thunk(wait)?);
    let mut words = vec![
        0xa9bb_7bfd, // stp x29, x30, [sp, #-80]!
        0x9100_03fd, // mov x29, sp
        0xa901_07e0, // stp x0, x1, [sp, #16]
        0xa902_0fe2, // stp x2, x3, [sp, #32]
        0xa903_17e4, // stp x4, x5, [sp, #48]
        0xa904_1fe6, // stp x6, x7, [sp, #64]
        0xa941_07e0, // 1: ldp x0, x1, [sp, #16]
        0xa942_0fe2, // ldp x2, x3, [sp, #32]
        0xa943_17e4, // ldp x4, x5, [sp, #48]
        0xa944_1fe6, // ldp x6, x7, [sp, #64]
        0x5800_01d0, // ldr x16, attempt
        0xd63f_0200, // blr x16
        0xb100_081f, // cmn x0, #2
        0x5400_0101, // b.ne 2f
        0xa941_07e0, // ldp x0, x1, [sp, #16]
        0xa942_0fe2, // ldp x2, x3, [sp, #32]
        0xa943_17e4, // ldp x4, x5, [sp, #48]
        0xa944_1fe6, // ldp x6, x7, [sp, #64]
        0x5800_0110, // ldr x16, wait
        0xd63f_0200, // blr x16
        0x17ff_fff2, // b 1b
        0xa8c5_7bfd, // 2: ldp x29, x30, [sp], #80
        0xd65f_03c0, // ret
        0xd503_201f, // nop
    ];
    for v in [a, w] {
        words.extend([v as u32, (v >> 32) as u32]);
    }
    emu.emit_code(&words)
}

/// The callback tables, made once: CFReadStreamCallBacks and CFWriteStreamCallBacks (version 2).
fn callbacks(emu: &mut Emu) -> Result<(u64, u64)> {
    if emu.cfnetwork.read_callbacks != 0 {
        return Ok((emu.cfnetwork.read_callbacks, emu.cfnetwork.write_callbacks));
    }
    let read = retry_trampoline(emu, read_attempt, read_wait)?;
    let write = retry_trampoline(emu, write_attempt, write_wait)?;
    let t = |emu: &mut Emu, f: HostFn| emu.host_thunk(f);
    // {version, create, finalize, copyDescription, open, openCompleted, read,
    //  getBuffer, canRead, close, copyProperty, setProperty, requestEvents, schedule, unschedule}
    let r = [2, 0, t(emu, finalize)?, 0, t(emu, open)?, 0, read, 0, t(emu, can_read)?, t(emu, close)?, t(emu, copy_property)?, t(emu, set_property)?, 0, 0, 0];
    // {version, create, finalize, copyDescription, open, openCompleted, write,
    //  canWrite, close, copyProperty, setProperty, requestEvents, schedule, unschedule}
    let w = [2, 0, t(emu, finalize)?, 0, t(emu, open)?, 0, write, t(emu, can_write)?, t(emu, close)?, t(emu, copy_property)?, t(emu, set_property)?, 0, 0, 0];
    let mut table = |words: &[u64]| -> Result<u64> {
        let p = emu.alloc(words.len() as u64 * 8)?;
        for (i, &v) in words.iter().enumerate() {
            emu.write_u64(p + i as u64 * 8, v)?;
        }
        Ok(p)
    };
    let (rp, wp) = (table(&r)?, table(&w)?);
    emu.cfnetwork.read_callbacks = rp;
    emu.cfnetwork.write_callbacks = wp;
    Ok((rp, wp))
}

/// `_CFSocketStreamCreatePair(alloc, host, port, sock, signature, readStream*, writeStream*)`
fn create_pair(emu: &mut Emu) -> Result<()> {
    let (alloc, host, port, sock, sig, rs, ws) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5), emu.arg(6));
    let host_name = if host == 0 { None } else { Some(crate::foundation::nsstring(emu, host)?) };
    // CFSocketSignature: {protocolFamily, socketType, protocol, CFDataRef address}.
    let target = if sig == 0 {
        None
    } else {
        let data = emu.read_u64(sig + 16)?;
        let bytes = crate::foundation::cf_call(emu, "_CFDataGetBytePtr", &[data])?;
        crate::sockets::read_sockaddr(emu, bytes)
    };
    let fd = (host == 0 && sig == 0).then_some(sock & 0xffff_ffff);
    let frame = emu.alloc(FRAME)?;
    emu.p.memory.write(frame, &[0u8; FRAME as usize])?;
    emu.cfnetwork.pairs.insert(
        frame,
        Pair {
            host: host_name,
            port: port as u16,
            target,
            fd,
            // A socket the caller made stays theirs unless they say otherwise.
            owns_fd: fd.is_none(),
            failed: None,
            read: 0,
            write: 0,
            read_open: false,
            write_open: false,
            read_armed: false,
            write_armed: false,
            wake: None,
            pump_running: false,
        },
    );
    let (rcb, wcb) = callbacks(emu)?;
    if rs != 0 {
        let r = crate::foundation::cf_call(emu, "_CFReadStreamCreate", &[alloc, rcb, frame])?;
        pair(emu, frame)?.read = r;
        emu.write_u64(rs, r)?;
    }
    if ws != 0 {
        let w = crate::foundation::cf_call(emu, "_CFWriteStreamCreate", &[alloc, wcb, frame])?;
        pair(emu, frame)?.write = w;
        emu.write_u64(ws, w)?;
    }
    Ok(())
}

// ---- opening and closing ----

/// Connect the pair if it isn't yet: Err((CFNetwork domain?, code)) if that fails.
fn connect(emu: &mut Emu, frame: u64) -> Result<std::result::Result<(), (bool, u64)>> {
    let p = pair(emu, frame)?;
    if let Some(e) = p.failed {
        return Ok(Err(e));
    }
    if p.fd.is_some() {
        return Ok(Ok(()));
    }
    let (host, port, target) = (p.host.clone(), p.port, p.target);
    let addrs: Vec<SocketAddr> = match (target, host) {
        (Some(t), _) => vec![t],
        (None, Some(h)) => crate::sockets::lookup(emu, &h).into_iter().map(|ip| SocketAddr::new(ip, port)).collect(),
        (None, None) => vec![],
    };
    let mut result = Err((true, HOST_NOT_FOUND));
    for a in addrs {
        match crate::sockets::connect_new(emu, a) {
            Ok(fd) => {
                result = Ok(fd);
                break;
            }
            Err(errno) => result = Err((false, errno)),
        }
    }
    let p = pair(emu, frame)?;
    match result {
        Ok(fd) => {
            p.fd = Some(fd);
            Ok(Ok(()))
        }
        Err(e) => {
            p.failed = Some(e);
            Ok(Err(e))
        }
    }
}

/// `open(stream, CFErrorRef *error, Boolean *openComplete, info)`
fn open(emu: &mut Emu) -> Result<()> {
    let (stream, err, complete, frame) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    emu.p.memory.write(complete, &[1])?;
    if let Err((cfnetwork, code)) = connect(emu, frame)? {
        let e = new_error(emu, cfnetwork, code)?;
        emu.write_u64(err, e)?;
        emu.ret(0);
        return Ok(());
    }
    let p = pair(emu, frame)?;
    if stream == p.read {
        p.read_open = true;
        p.read_armed = true;
    } else {
        p.write_open = true;
        p.write_armed = true;
    }
    start_pump(emu, frame)?;
    rearm(emu, frame)?;
    emu.ret(1);
    Ok(())
}

/// One stream is closed (or gone): when neither is open, the connection closes.
fn close_side(emu: &mut Emu, stream: u64, frame: u64) -> Result<()> {
    let p = pair(emu, frame)?;
    if stream == p.read {
        p.read_open = false;
        p.read_armed = false;
    } else if stream == p.write {
        p.write_open = false;
        p.write_armed = false;
    }
    if !p.read_open && !p.write_open && let Some(fd) = p.fd {
        let owns = p.owns_fd;
        // The stream no longer uses it; the network thread sees that and stops.
        p.fd = None;
        p.failed = Some((false, 9)); // EBADF: a closed pair doesn't reopen.
        if owns {
            crate::sockets::close_fd(emu, fd);
        }
    }
    rearm(emu, frame)
}

/// `close(stream, info)`
fn close(emu: &mut Emu) -> Result<()> {
    let (stream, frame) = (emu.arg(0), emu.arg(1));
    close_side(emu, stream, frame)
}

/// `finalize(stream, info)`: the stream is being freed.
fn finalize(emu: &mut Emu) -> Result<()> {
    let (stream, frame) = (emu.arg(0), emu.arg(1));
    close_side(emu, stream, frame)?;
    let p = pair(emu, frame)?;
    if stream == p.read {
        p.read = 0;
    } else if stream == p.write {
        p.write = 0;
    }
    if p.read == 0 && p.write == 0 && !p.pump_running {
        free_pair(emu, frame)?;
    }
    Ok(())
}

fn free_pair(emu: &mut Emu, frame: u64) -> Result<()> {
    if let Some(p) = emu.cfnetwork.pairs.remove(&frame) {
        if let Some((a, b)) = p.wake {
            crate::sockets::close_fd(emu, a);
            crate::sockets::close_fd(emu, b);
        }
        emu.heap.free(&emu.p.memory, frame)?;
    }
    Ok(())
}

// ---- the network thread ----

fn start_pump(emu: &mut Emu, frame: u64) -> Result<()> {
    if pair(emu, frame)?.pump_running {
        return Ok(());
    }
    if emu.cfnetwork.pump_code == 0 {
        let step = emu.host_thunk(pump_step)?;
        emu.cfnetwork.pump_code = emu.emit_code(&[
            0xa9bf_7bf3, // stp x19, x30, [sp, #-16]!
            0xaa00_03f3, // mov x19, x0             (the frame)
            0xaa13_03e0, // 1: mov x0, x19
            0x5800_01b0, // ldr x16, step
            0xd63f_0200, // blr x16
            0xb400_00e0, // cbz x0, 2f              (stop)
            0xf100_0c1f, // cmp x0, #3
            0x54ff_ff61, // b.ne 1b                 (waited: step again)
            0xa940_0270, // ldp x16, x0, [x19]      (signal: the call in the frame)
            0xa941_0a61, // ldp x1, x2, [x19, #16]
            0xd63f_0200, // blr x16
            0x17ff_fff7, // b 1b
            0xa8c1_7bf3, // 2: ldp x19, x30, [sp], #16
            0xd280_0000, // mov x0, #0
            0xd65f_03c0, // ret
            0xd503_201f, // nop
            step as u32,
            (step >> 32) as u32,
        ])?;
    }
    let wake = crate::sockets::local_pair(emu);
    let p = pair(emu, frame)?;
    p.wake = Some(wake);
    p.pump_running = true;
    emu.p.memory.write_u32(frame + PUMP_FDS + 8, wake.0 as u32)?;
    emu.p.memory.write_u16(frame + PUMP_FDS + 12, POLLIN)?;
    let code = emu.cfnetwork.pump_code;
    crate::threads::spawn(emu, code, frame, "com.apple.CFSocketStream")?;
    Ok(())
}

/// What the network thread polls the connection for: the events a stream is to hear.
fn poll_for(emu: &mut Emu, frame: u64) -> Result<()> {
    let p = pair(emu, frame)?;
    let events = if p.read_open && p.read_armed { POLLIN } else { 0 } | if p.write_open && p.write_armed { POLLOUT } else { 0 };
    let fd = p.fd.filter(|_| events != 0).map_or(-1, |fd| fd as i32);
    emu.p.memory.write_u32(frame + PUMP_FDS, fd as u32)?;
    emu.p.memory.write_u16(frame + PUMP_FDS + 4, events)?;
    Ok(())
}

/// The streams' wants changed: update the poll and nudge the network thread to look again.
fn rearm(emu: &mut Emu, frame: u64) -> Result<()> {
    poll_for(emu, frame)?;
    if let Some((_, poke)) = pair(emu, frame)?.wake {
        crate::sockets::send_now(emu, poke, &[1])?;
    }
    Ok(())
}

/// The network thread's step: signal an event (PUMP_SIGNAL, the call in the
/// frame), stop (PUMP_STOP), or wait for the connection or a nudge.
fn pump_step(emu: &mut Emu) -> Result<()> {
    let frame = emu.arg(0);
    let Some(p) = emu.cfnetwork.pairs.get(&frame) else {
        emu.ret(PUMP_STOP);
        return Ok(());
    };
    let (wake, fd) = (p.wake, p.fd);
    if let Some((w, _)) = wake {
        crate::sockets::drain_local(emu, w);
    }
    let Some(fd) = fd else {
        let p = pair(emu, frame)?;
        p.pump_running = false;
        if p.read == 0 && p.write == 0 {
            free_pair(emu, frame)?;
        }
        emu.ret(PUMP_STOP);
        return Ok(());
    };
    let r = crate::sockets::ready(emu, fd);
    let p = pair(emu, frame)?;
    let signal = if p.read_open && p.read_armed && (r.readable || r.closed) {
        p.read_armed = false;
        Some(("_CFReadStreamSignalEvent", p.read, HAS_BYTES_AVAILABLE))
    } else if p.write_open && p.write_armed && (r.writable || r.closed) {
        p.write_armed = false;
        Some(("_CFWriteStreamSignalEvent", p.write, CAN_ACCEPT_BYTES))
    } else {
        None
    };
    if let Some((f, stream, event)) = signal {
        let f = crate::foundation::cf_sym(emu, f)?;
        for (i, v) in [f, stream, event, 0].into_iter().enumerate() {
            emu.write_u64(frame + i as u64 * 8, v)?;
        }
        poll_for(emu, frame)?;
        emu.ret(PUMP_SIGNAL);
        return Ok(());
    }
    poll_for(emu, frame)?;
    crate::threads::block(emu, Wait::Socket(Pending::Poll { fds: frame + PUMP_FDS, n: 2, deadline: None }))?;
    emu.ret(1);
    Ok(())
}

// ---- reading and writing ----

/// Report errno `e` through a callback's `CFErrorRef *`.
fn fail(emu: &mut Emu, err: u64, e: u64) -> Result<()> {
    if err != 0 {
        let error = new_error(emu, false, e)?;
        emu.write_u64(err, error)?;
    }
    emu.ret(u64::MAX);
    Ok(())
}

/// `read(stream, buffer, length, CFErrorRef *error, Boolean *atEOF, info)`, without waiting.
fn read_attempt(emu: &mut Emu) -> Result<()> {
    let (buf, len, err, eof, frame) = (emu.arg(1), emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    emu.p.memory.write(eof, &[0])?;
    let Some(fd) = pair(emu, frame)?.fd else { return fail(emu, err, 9) };
    match crate::sockets::recv_now(emu, fd, buf, len)? {
        Some(Ok(n)) => {
            if n == 0 && len > 0 {
                emu.p.memory.write(eof, &[1])?;
            } else {
                pair(emu, frame)?.read_armed = true;
                rearm(emu, frame)?;
            }
            emu.ret(n);
            Ok(())
        }
        Some(Err(e)) => fail(emu, err, e),
        None => {
            emu.p.memory.write_u32(frame + READ_FD, fd as u32)?;
            emu.p.memory.write_u16(frame + READ_FD + 4, POLLIN)?;
            emu.ret(RETRY);
            Ok(())
        }
    }
}

/// A read waits for the connection to have bytes (or close).
fn read_wait(emu: &mut Emu) -> Result<()> {
    let frame = emu.arg(5);
    crate::threads::block(emu, Wait::Socket(Pending::Poll { fds: frame + READ_FD, n: 1, deadline: None }))?;
    Ok(())
}

/// `write(stream, buffer, length, CFErrorRef *error, info)`, without waiting.
fn write_attempt(emu: &mut Emu) -> Result<()> {
    let (buf, len, err, frame) = (emu.arg(1), emu.arg(2), emu.arg(3), emu.arg(4));
    let Some(fd) = pair(emu, frame)?.fd else { return fail(emu, err, 9) };
    let data = emu.p.memory.read(buf, len as usize)?.to_vec();
    match crate::sockets::send_now(emu, fd, &data)? {
        Some(Ok(n)) => {
            pair(emu, frame)?.write_armed = true;
            rearm(emu, frame)?;
            emu.ret(n);
            Ok(())
        }
        Some(Err(e)) => fail(emu, err, e),
        None => {
            emu.p.memory.write_u32(frame + WRITE_FD, fd as u32)?;
            emu.p.memory.write_u16(frame + WRITE_FD + 4, POLLOUT)?;
            emu.ret(RETRY);
            Ok(())
        }
    }
}

/// A write waits for room.
fn write_wait(emu: &mut Emu) -> Result<()> {
    let frame = emu.arg(4);
    crate::threads::block(emu, Wait::Socket(Pending::Poll { fds: frame + WRITE_FD, n: 1, deadline: None }))?;
    Ok(())
}

/// `canRead(stream, CFErrorRef *error, info)`
fn can_read(emu: &mut Emu) -> Result<()> {
    let frame = emu.arg(2);
    let r = pair(emu, frame)?.fd.map(|fd| crate::sockets::ready(emu, fd));
    emu.ret(r.is_some_and(|r| r.readable || r.closed) as u64);
    Ok(())
}

/// `canWrite(stream, CFErrorRef *error, info)`
fn can_write(emu: &mut Emu) -> Result<()> {
    let frame = emu.arg(2);
    let r = pair(emu, frame)?.fd.map(|fd| crate::sockets::ready(emu, fd));
    emu.ret(r.is_some_and(|r| r.writable) as u64);
    Ok(())
}

// ---- properties ----

fn is_property(emu: &mut Emu, name: u64, constant: &'static str) -> Result<bool> {
    let k = crate::foundation::cf_data_pub(emu, constant)?;
    Ok(name == k || crate::foundation::cf_call(emu, "_CFEqual", &[name, k])? & 0xff != 0)
}

/// `copyProperty(stream, name, info)`: the socket, and where it goes.
fn copy_property(emu: &mut Emu) -> Result<()> {
    let (name, frame) = (emu.arg(1), emu.arg(2));
    let (fd, host, port) = {
        let p = pair(emu, frame)?;
        (p.fd, p.host.clone(), p.port)
    };
    let v = if is_property(emu, name, "_kCFStreamPropertySocketNativeHandle")? {
        match fd {
            Some(fd) => crate::foundation::new_data(emu, &(fd as i32).to_le_bytes())?,
            None => 0,
        }
    } else if is_property(emu, name, "_kCFStreamPropertySocketRemoteHostName")? {
        match host {
            Some(h) => crate::foundation::new_nsstring(emu, &h)?,
            None => 0,
        }
    } else if is_property(emu, name, "_kCFStreamPropertySocketRemotePortNumber")? {
        crate::foundation::new_number_i64(emu, port as i64)?
    } else {
        0
    };
    emu.ret(v);
    Ok(())
}

/// `setProperty(stream, name, value, info)`: whether closing closes the socket.
fn set_property(emu: &mut Emu) -> Result<()> {
    let (name, value, frame) = (emu.arg(1), emu.arg(2), emu.arg(3));
    if is_property(emu, name, "_kCFStreamPropertyShouldCloseNativeSocket")? {
        let yes = crate::foundation::cf_data_pub(emu, "_kCFBooleanTrue")?;
        pair(emu, frame)?.owns_fd = value == yes;
        emu.ret(1);
    } else {
        emu.ret(0);
    }
    Ok(())
}

/// `CFNetworkCopySystemProxySettings()`: the phone has no proxy. As measured
/// (tests/fixtures/proxies), the dictionary is there but says nothing (no
/// `HTTPEnable`).
fn copy_system_proxy_settings(emu: &mut Emu) -> Result<()> {
    let d = crate::foundation::new_dict_of(emu, &[])?;
    emu.ret(d);
    Ok(())
}

/// `CFNetworkCopyProxiesForURL(url, settings)`: one entry, no proxy
/// (`kCFProxyTypeKey` = `kCFProxyTypeNone`), as measured.
fn copy_proxies_for_url(emu: &mut Emu) -> Result<()> {
    let k = crate::foundation::new_nsstring(emu, "kCFProxyTypeKey")?;
    let v = crate::foundation::new_nsstring(emu, "kCFProxyTypeNone")?;
    let entry = crate::foundation::new_dict_of(emu, &[(k, v)])?;
    let list = crate::foundation::array_from(emu, &[entry])?;
    let list = crate::objc::send(emu, list, "retain", &[])?;
    emu.ret(list);
    Ok(())
}

// ---- CFHTTPMessage ----
//
// An HTTP message, built (a request: method, URL, headers, body) or parsed
// from bytes as they arrive (a response). As measured on the iOS 26
// Simulator (tests/fixtures/httpmessage): header names compare without
// regard to case and keep their first spelling; a request's line is the
// URL's path (`/` if none) and query; a response's header is complete at the
// blank line, and the bytes after it are the body. (iOS writes headers in a
// hashed order; pomme writes them in the order they were set.) The CF object
// standing for a message is a CFMutableDictionary, so CFRetain and CFRelease
// work on it.

#[derive(Clone, Default, Debug)]
pub(crate) struct HttpMessage {
    request: bool,
    method: String,
    url: String,
    version: String,
    status: u64,
    status_line: String,
    headers: Vec<(String, String)>,
    /// Bytes appended and not yet parsed (until the header is complete).
    raw: Vec<u8>,
    header_complete: bool,
    body: Vec<u8>,
}

impl HttpMessage {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn set_header(&mut self, name: &str, value: Option<String>) {
        let at = self.headers.iter().position(|(k, _)| k.eq_ignore_ascii_case(name));
        match (at, value) {
            (Some(i), Some(v)) => self.headers[i].1 = v,
            (Some(i), None) => {
                self.headers.remove(i);
            }
            (None, Some(v)) => self.headers.push((name.to_string(), v)),
            (None, None) => {}
        }
    }

    /// A request's target: the URL's path (`/` if none) and query.
    fn target(&self) -> String {
        let after_scheme = self.url.split_once("://").map_or(self.url.as_str(), |(_, r)| r);
        let path_and_query = after_scheme.find('/').map_or("", |i| &after_scheme[i..]);
        let path_and_query = path_and_query.split('#').next().unwrap_or("");
        if path_and_query.is_empty() || path_and_query.starts_with('?') { format!("/{path_and_query}") } else { path_and_query.to_string() }
    }

    fn serialized(&self) -> Vec<u8> {
        let mut s = if self.request { format!("{} {} {}\r\n", self.method, self.target(), self.version) } else { format!("{}\r\n", self.status_line) };
        for (k, v) in &self.headers {
            s.push_str(&format!("{k}: {v}\r\n"));
        }
        s.push_str("\r\n");
        let mut out = s.into_bytes();
        out.extend_from_slice(&self.body);
        out
    }

    /// Take in bytes; false if the header doesn't parse.
    fn append(&mut self, bytes: &[u8]) -> bool {
        if self.header_complete {
            self.body.extend_from_slice(bytes);
            return true;
        }
        self.raw.extend_from_slice(bytes);
        let Some(end) = self.raw.windows(4).position(|w| w == b"\r\n\r\n") else { return true };
        let head = String::from_utf8_lossy(&self.raw[..end]).into_owned();
        self.body = self.raw[end + 4..].to_vec();
        self.raw.clear();
        self.header_complete = true;
        let mut lines = head.split("\r\n");
        let first = lines.next().unwrap_or("");
        if self.request {
            let mut parts = first.splitn(3, ' ');
            self.method = parts.next().unwrap_or("").to_string();
            self.url = parts.next().unwrap_or("").to_string();
            self.version = parts.next().unwrap_or("").to_string();
        } else {
            self.status_line = first.to_string();
            let mut parts = first.splitn(3, ' ');
            self.version = parts.next().unwrap_or("").to_string();
            match parts.next().and_then(|c| c.parse().ok()) {
                Some(code) => self.status = code,
                None => return false,
            }
        }
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                self.set_header(k.trim(), Some(v.trim().to_string()));
            }
        }
        true
    }
}

fn http_message(emu: &Emu, m: u64) -> HttpMessage {
    emu.cfnetwork.messages.get(&m).cloned().unwrap_or_default()
}

/// A new message: a CF object to stand for it.
fn new_http_message(emu: &mut Emu, msg: HttpMessage) -> Result<u64> {
    let kcb = crate::foundation::cf_sym(emu, "_kCFTypeDictionaryKeyCallBacks")?;
    let vcb = crate::foundation::cf_sym(emu, "_kCFTypeDictionaryValueCallBacks")?;
    let m = crate::foundation::cf_call(emu, "_CFDictionaryCreateMutable", &[0, 0, kcb, vcb])?;
    emu.cfnetwork.messages.insert(m, msg);
    Ok(m)
}

fn cf_string_arg(emu: &mut Emu, v: u64) -> Result<String> {
    if v == 0 { Ok(String::new()) } else { crate::foundation::nsstring(emu, v) }
}

fn url_string(emu: &mut Emu, url: u64) -> Result<String> {
    if url == 0 {
        return Ok(String::new());
    }
    let s = crate::objc::send(emu, url, "absoluteString", &[])?;
    cf_string_arg(emu, s)
}

/// `CFHTTPMessageCreateRequest(alloc, method, url, version)`
fn http_create_request(emu: &mut Emu) -> Result<()> {
    let (method, url, version) = (emu.arg(1), emu.arg(2), emu.arg(3));
    let msg = HttpMessage { request: true, method: cf_string_arg(emu, method)?, url: url_string(emu, url)?, version: cf_string_arg(emu, version)?, header_complete: true, ..Default::default() };
    let m = new_http_message(emu, msg)?;
    emu.ret(m);
    Ok(())
}

/// `CFHTTPMessageCreateResponse(alloc, status, description, version)`
fn http_create_response(emu: &mut Emu) -> Result<()> {
    let (status, desc, version) = (emu.arg(1), emu.arg(2), emu.arg(3));
    let version = cf_string_arg(emu, version)?;
    let desc = cf_string_arg(emu, desc)?;
    let msg = HttpMessage { request: false, status, status_line: format!("{version} {status} {desc}"), version, header_complete: true, ..Default::default() };
    let m = new_http_message(emu, msg)?;
    emu.ret(m);
    Ok(())
}

/// `CFHTTPMessageCreateEmpty(alloc, isRequest)`: to be filled by AppendBytes.
fn http_create_empty(emu: &mut Emu) -> Result<()> {
    let request = emu.arg(1) & 0xff != 0;
    let m = new_http_message(emu, HttpMessage { request, ..Default::default() })?;
    emu.ret(m);
    Ok(())
}

fn http_set_header(emu: &mut Emu) -> Result<()> {
    let (m, k, v) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let k = cf_string_arg(emu, k)?;
    let v = if v == 0 { None } else { Some(cf_string_arg(emu, v)?) };
    if let Some(msg) = emu.cfnetwork.messages.get_mut(&m) {
        msg.set_header(&k, v);
    }
    Ok(())
}

fn http_copy_header(emu: &mut Emu) -> Result<()> {
    let (m, k) = (emu.arg(0), emu.arg(1));
    let k = cf_string_arg(emu, k)?;
    let v = http_message(emu, m).header(&k).map(str::to_string);
    let r = match v {
        Some(v) => crate::foundation::new_nsstring(emu, &v)?,
        None => 0,
    };
    emu.ret(r);
    Ok(())
}

fn http_copy_all_headers(emu: &mut Emu) -> Result<()> {
    let m = emu.arg(0);
    let mut pairs = Vec::new();
    for (k, v) in http_message(emu, m).headers {
        pairs.push((crate::foundation::new_nsstring(emu, &k)?, crate::foundation::new_nsstring(emu, &v)?));
    }
    let d = crate::foundation::new_dict_of(emu, &pairs)?;
    emu.ret(d);
    Ok(())
}

fn http_copy_serialized(emu: &mut Emu) -> Result<()> {
    let m = emu.arg(0);
    let bytes = http_message(emu, m).serialized();
    let d = crate::foundation::new_data(emu, &bytes)?;
    emu.ret(d);
    Ok(())
}

fn http_append_bytes(emu: &mut Emu) -> Result<()> {
    let (m, p, n) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let bytes = emu.p.memory.read(p, n as usize)?.to_vec();
    let ok = emu.cfnetwork.messages.get_mut(&m).is_some_and(|msg| msg.append(&bytes));
    emu.ret(ok as u64);
    Ok(())
}

fn http_copy_body(emu: &mut Emu) -> Result<()> {
    let m = emu.arg(0);
    let body = http_message(emu, m).body;
    let d = crate::foundation::new_data(emu, &body)?;
    emu.ret(d);
    Ok(())
}

fn http_set_body(emu: &mut Emu) -> Result<()> {
    let (m, d) = (emu.arg(0), emu.arg(1));
    let body = if d == 0 { Vec::new() } else { crate::foundation::data_bytes(emu, d)? };
    if let Some(msg) = emu.cfnetwork.messages.get_mut(&m) {
        msg.body = body;
    }
    Ok(())
}

fn http_string_field(emu: &mut Emu, f: fn(&HttpMessage) -> String) -> Result<()> {
    let m = emu.arg(0);
    let v = f(&http_message(emu, m));
    let r = crate::foundation::new_nsstring(emu, &v)?;
    emu.ret(r);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_CFHTTPMessageCreateRequest", http_create_request),
    ("_CFHTTPMessageCreateResponse", http_create_response),
    ("_CFHTTPMessageCreateEmpty", http_create_empty),
    ("_CFHTTPMessageSetHeaderFieldValue", http_set_header),
    ("_CFHTTPMessageCopyHeaderFieldValue", http_copy_header),
    ("_CFHTTPMessageCopyAllHeaderFields", http_copy_all_headers),
    ("_CFHTTPMessageCopySerializedMessage", http_copy_serialized),
    ("_CFHTTPMessageAppendBytes", http_append_bytes),
    ("_CFHTTPMessageCopyBody", http_copy_body),
    ("_CFHTTPMessageSetBody", http_set_body),
    ("_CFHTTPMessageIsHeaderComplete", |e| {
        let v = http_message(e, e.arg(0)).header_complete as u64;
        e.ret(v);
        Ok(())
    }),
    ("_CFHTTPMessageIsRequest", |e| {
        let v = http_message(e, e.arg(0)).request as u64;
        e.ret(v);
        Ok(())
    }),
    ("_CFHTTPMessageGetResponseStatusCode", |e| {
        let v = http_message(e, e.arg(0)).status;
        e.ret(v);
        Ok(())
    }),
    ("_CFHTTPMessageCopyResponseStatusLine", |e| http_string_field(e, |m| m.status_line.clone())),
    ("_CFHTTPMessageCopyRequestMethod", |e| http_string_field(e, |m| m.method.clone())),
    ("_CFHTTPMessageCopyVersion", |e| http_string_field(e, |m| m.version.clone())),
    ("_CFHTTPMessageCopyRequestURL", |e| {
        let m = e.arg(0);
        let url = http_message(e, m).url;
        let s = crate::foundation::new_nsstring(e, &url)?;
        let cls = crate::objc::host_class(e, "NSURL")?;
        let u = crate::objc::send(e, cls, "URLWithString:", &[s])?;
        let u = crate::objc::send(e, u, "retain", &[])?;
        e.ret(u);
        Ok(())
    }),
    ("_CFNetworkCopySystemProxySettings", copy_system_proxy_settings),
    ("_CFNetworkCopyProxiesForURL", copy_proxies_for_url),
    ("__CFSocketStreamCreatePair", create_pair),
    ("__CFErrorCreateWithStreamError", error_from_stream_error),
    ("__CFStreamErrorFromCFError", stream_error_from_error),
];

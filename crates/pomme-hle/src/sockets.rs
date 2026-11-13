//! BSD sockets: TCP over IPv4 and IPv6 for apps that speak to servers
//! themselves (gRPC, WebSockets, database clients), and local socket pairs.
//!
//! A TCP socket's connection is the host's, through [`crate::net::Net`] (the
//! same host interface as NSURLSession's requests): with no network there,
//! names don't resolve and connections don't open, as on a phone offline;
//! the modelled network (`--network offline`) does the same. Connecting
//! completes at once (as POSIX allows even for a non-blocking socket); reads
//! and writes then follow the socket's O_NONBLOCK: EAGAIN, or the thread
//! waits. `poll` and `select` cover sockets, pipes and files together.
//!
//! When every thread is waiting on the network, pomme waits for it in real
//! time and moves virtual time on by as much (see [`network_idle`]).

use crate::libsystem::set_errno;
use crate::net::{Readiness, SocketId};
use crate::{Emu, HostFn, Result};
use std::collections::{HashMap, VecDeque};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

// Address families and socket types (Darwin's values).
const AF_UNIX: u64 = 1;
const AF_INET: u64 = 2;
const AF_INET6: u64 = 30;
const SOCK_STREAM: u64 = 1;

// errno
const EPIPE: u64 = 32;
const EAGAIN: u64 = 35;
const ENOTSOCK: u64 = 38;
const EPROTONOSUPPORT: u64 = 43;
const EOPNOTSUPP: u64 = 45;
const EAFNOSUPPORT: u64 = 47;
const ENETUNREACH: u64 = 51;
const ECONNRESET: u64 = 54;
const EISCONN: u64 = 56;
const ENOTCONN: u64 = 57;
const ETIMEDOUT: u64 = 60;
const ECONNREFUSED: u64 = 61;
const EHOSTUNREACH: u64 = 65;
const EINVAL: u64 = 22;

// poll(2) events
const POLLIN: u16 = 0x1;
const POLLOUT: u16 = 0x4;
const POLLERR: u16 = 0x8;
const POLLHUP: u16 = 0x10;
const POLLNVAL: u16 = 0x20;

// getaddrinfo errors and flags
const EAI_NONAME: u64 = 8;
const EAI_SERVICE: u64 = 9;
const EAI_FAMILY: u64 = 5;
const AI_PASSIVE: u64 = 0x1;
const AI_CANONNAME: u64 = 0x2;
const AI_NUMERICHOST: u64 = 0x4;

// Socket options (SOL_SOCKET)
const SOL_SOCKET: u64 = 0xffff;
const SO_ERROR: u64 = 0x1007;
const SO_TYPE: u64 = 0x1008;

/// O_NONBLOCK, as fcntl keeps it.
const O_NONBLOCK: u64 = 4;

#[derive(Clone, Debug)]
enum Kind {
    /// Not connected yet.
    Fresh,
    /// A TCP connection the host holds.
    Tcp(SocketId),
    /// One end of a socketpair: what's been sent to it, and its peer's descriptor.
    Local { inbox: VecDeque<u8>, peer: Option<u64>, shut_write: bool },
}

#[derive(Clone, Debug)]
struct Sock {
    domain: u64,
    ty: u64,
    kind: Kind,
    options: HashMap<(u64, u64), Vec<u8>>,
}

#[derive(Default)]
pub struct State {
    socks: HashMap<u64, Sock>,
}

/// A socket call waiting for the network (see [`crate::threads::Wait::Socket`]).
#[derive(Clone, Debug)]
pub enum Pending {
    /// recv into these buffers (base, length).
    Recv { fd: u64, bufs: Vec<(u64, u64)> },
    /// send these bytes.
    Send { fd: u64, data: Vec<u8> },
    /// poll(fds, n) until something's ready or the deadline.
    Poll { fds: u64, n: u64, deadline: Option<u64> },
    /// select over these descriptor sets until something's ready or the deadline.
    Select { nfds: u64, sets: [u64; 3], wanted: [Vec<u64>; 3], deadline: Option<u64> },
}

impl Pending {
    pub fn deadline(&self) -> Option<u64> {
        match self {
            Pending::Poll { deadline, .. } | Pending::Select { deadline, .. } => *deadline,
            _ => None,
        }
    }
}

/// Whether `fd` is a socket.
pub(crate) fn owns(emu: &Emu, fd: u64) -> bool {
    emu.sockets.socks.contains_key(&fd)
}

fn nonblocking(emu: &Emu, fd: u64) -> bool {
    crate::fs::status_flags(emu, fd) & O_NONBLOCK != 0
}

/// Return -1 with errno.
fn fail(emu: &mut Emu, errno: u64) -> Result<()> {
    set_errno(emu, errno)?;
    emu.ret(u64::MAX);
    Ok(())
}

fn errno_of(e: &std::io::Error) -> u64 {
    use std::io::ErrorKind::*;
    match e.kind() {
        WouldBlock => EAGAIN,
        ConnectionRefused => ECONNREFUSED,
        ConnectionReset | ConnectionAborted => ECONNRESET,
        BrokenPipe => EPIPE,
        NotConnected => ENOTCONN,
        TimedOut => ETIMEDOUT,
        HostUnreachable => EHOSTUNREACH,
        NetworkUnreachable | NetworkDown | AddrNotAvailable => ENETUNREACH,
        _ => ECONNRESET,
    }
}

/// Whether the phone has a network for sockets: the modelled path, and a host network.
fn online(emu: &Emu) -> bool {
    emu.network.path != crate::network::Path::Offline && emu.net.client.is_some()
}

// ---- addresses ----

/// A `struct sockaddr_in` / `sockaddr_in6` at `p`.
pub(crate) fn read_sockaddr(emu: &Emu, p: u64) -> Option<SocketAddr> {
    let family = emu.p.memory.read_u8(p + 1).ok()? as u64;
    let port = u16::from_be_bytes([emu.p.memory.read_u8(p + 2).ok()?, emu.p.memory.read_u8(p + 3).ok()?]);
    match family {
        AF_INET => {
            let b = emu.p.memory.read(p + 4, 4).ok()?;
            Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(b[0], b[1], b[2], b[3])), port))
        }
        AF_INET6 => {
            let mut a = [0u8; 16];
            a.copy_from_slice(emu.p.memory.read(p + 8, 16).ok()?);
            Some(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(a)), port))
        }
        _ => None,
    }
}

/// The bytes of `addr` as a `struct sockaddr_in` (16) or `sockaddr_in6` (28).
pub(crate) fn sockaddr_bytes(addr: SocketAddr) -> Vec<u8> {
    match addr {
        SocketAddr::V4(a) => {
            let mut b = vec![16u8, AF_INET as u8];
            b.extend_from_slice(&a.port().to_be_bytes());
            b.extend_from_slice(&a.ip().octets());
            b.extend_from_slice(&[0; 8]);
            b
        }
        SocketAddr::V6(a) => {
            let mut b = vec![28u8, AF_INET6 as u8];
            b.extend_from_slice(&a.port().to_be_bytes());
            b.extend_from_slice(&a.flowinfo().to_be_bytes());
            b.extend_from_slice(&a.ip().octets());
            b.extend_from_slice(&a.scope_id().to_le_bytes());
            b
        }
    }
}

/// Write `addr` to `out` (at most `*len` bytes) and its size to `*len`.
fn write_sockaddr(emu: &mut Emu, addr: SocketAddr, out: u64, len: u64) -> Result<()> {
    let bytes = sockaddr_bytes(addr);
    let room = if len == 0 { bytes.len() } else { emu.p.memory.read_u32(len)? as usize };
    if out != 0 {
        emu.p.memory.write(out, &bytes[..bytes.len().min(room)])?;
    }
    if len != 0 {
        emu.p.memory.write_u32(len, bytes.len() as u32)?;
    }
    Ok(())
}

// ---- socket, connect, close ----

/// `socket(domain, type, protocol)`: TCP over IPv4/IPv6 (UDP and raw sockets aren't modelled yet).
fn socket(emu: &mut Emu) -> Result<()> {
    let (domain, ty) = (emu.arg(0) & 0xffff_ffff, emu.arg(1) & 0xffff_ffff);
    if !matches!(domain, AF_INET | AF_INET6) {
        return fail(emu, EAFNOSUPPORT);
    }
    if ty != SOCK_STREAM {
        return fail(emu, EPROTONOSUPPORT);
    }
    let fd = crate::fs::reserve_fd(emu);
    emu.sockets.socks.insert(fd, Sock { domain, ty, kind: Kind::Fresh, options: HashMap::new() });
    emu.ret(fd);
    Ok(())
}

/// `socketpair(AF_UNIX, SOCK_STREAM, 0, int sv[2])`: two connected local ends.
fn socketpair(emu: &mut Emu) -> Result<()> {
    let (domain, ty, sv) = (emu.arg(0) & 0xffff_ffff, emu.arg(1) & 0xffff_ffff, emu.arg(3));
    if domain != AF_UNIX {
        return fail(emu, EAFNOSUPPORT);
    }
    if ty != SOCK_STREAM {
        return fail(emu, EOPNOTSUPP);
    }
    let (a, b) = local_pair(emu);
    emu.p.memory.write_u32(sv, a as u32)?;
    emu.p.memory.write_u32(sv + 4, b as u32)?;
    emu.ret(0);
    Ok(())
}

/// `connect(fd, addr, len)`: the host opens the connection (it completes at once).
fn connect(emu: &mut Emu) -> Result<()> {
    let (fd, addr) = (emu.arg(0) & 0xffff_ffff, emu.arg(1));
    let Some(sock) = emu.sockets.socks.get(&fd) else { return fail(emu, ENOTSOCK) };
    if !matches!(sock.kind, Kind::Fresh) {
        return fail(emu, EISCONN);
    }
    let Some(target) = read_sockaddr(emu, addr) else { return fail(emu, EAFNOSUPPORT) };
    if !online(emu) {
        return fail(emu, ENETUNREACH);
    }
    let r = emu.net.client.as_mut().map(|n| n.connect(target));
    match r {
        Some(Ok(id)) => {
            if let Some(s) = emu.sockets.socks.get_mut(&fd) {
                s.kind = Kind::Tcp(id);
            }
            emu.ret(0);
            Ok(())
        }
        Some(Err(e)) => fail(emu, errno_of(&e)),
        None => fail(emu, ENETUNREACH),
    }
}

/// Close a socket descriptor (from `close`).
pub(crate) fn close(emu: &mut Emu, fd: u64) {
    let Some(sock) = emu.sockets.socks.remove(&fd) else { return };
    match sock.kind {
        Kind::Tcp(id) => {
            if let Some(n) = emu.net.client.as_mut() {
                n.close(id);
            }
        }
        Kind::Local { peer: Some(p), .. } => {
            if let Some(Sock { kind: Kind::Local { peer, .. }, .. }) = emu.sockets.socks.get_mut(&p) {
                *peer = None;
            }
        }
        _ => {}
    }
}

/// `shutdown(fd, how)`: 0 reading, 1 writing, 2 both.
fn shutdown(emu: &mut Emu) -> Result<()> {
    let (fd, how) = (emu.arg(0) & 0xffff_ffff, emu.arg(1) & 0xffff_ffff);
    let Some(sock) = emu.sockets.socks.get(&fd).cloned() else { return fail(emu, ENOTSOCK) };
    match sock.kind {
        Kind::Tcp(id) => {
            let how = match how {
                0 => std::net::Shutdown::Read,
                1 => std::net::Shutdown::Write,
                _ => std::net::Shutdown::Both,
            };
            if let Some(n) = emu.net.client.as_mut() {
                n.shutdown(id, how);
            }
        }
        Kind::Local { .. } if how != 0 => {
            if let Some(Sock { kind: Kind::Local { shut_write, .. }, .. }) = emu.sockets.socks.get_mut(&fd) {
                *shut_write = true;
            }
        }
        Kind::Fresh => return fail(emu, ENOTCONN),
        _ => {}
    }
    emu.ret(0);
    Ok(())
}

// ---- for CFNetwork's socket streams (cfnetwork.rs) ----

/// A new descriptor with a TCP connection to `addr`, or the errno connecting failed with.
pub(crate) fn connect_new(emu: &mut Emu, addr: SocketAddr) -> std::result::Result<u64, u64> {
    if !online(emu) {
        return Err(ENETUNREACH);
    }
    let id = match emu.net.client.as_mut().map(|n| n.connect(addr)) {
        Some(Ok(id)) => id,
        Some(Err(e)) => return Err(errno_of(&e)),
        None => return Err(ENETUNREACH),
    };
    let fd = crate::fs::reserve_fd(emu);
    let domain = if addr.is_ipv4() { AF_INET } else { AF_INET6 };
    emu.sockets.socks.insert(fd, Sock { domain, ty: SOCK_STREAM, kind: Kind::Tcp(id), options: HashMap::new() });
    Ok(fd)
}

/// Two connected local descriptors, as `socketpair` makes.
pub(crate) fn local_pair(emu: &mut Emu) -> (u64, u64) {
    let (a, b) = (crate::fs::reserve_fd(emu), crate::fs::reserve_fd(emu));
    for (me, other) in [(a, b), (b, a)] {
        let kind = Kind::Local { inbox: VecDeque::new(), peer: Some(other), shut_write: false };
        emu.sockets.socks.insert(me, Sock { domain: AF_UNIX, ty: SOCK_STREAM, kind, options: HashMap::new() });
    }
    (a, b)
}

/// Throw away what's waiting on local socket `fd`.
pub(crate) fn drain_local(emu: &mut Emu, fd: u64) {
    if let Some(Sock { kind: Kind::Local { inbox, .. }, .. }) = emu.sockets.socks.get_mut(&fd) {
        inbox.clear();
    }
}

/// Close socket descriptor `fd`, as `close` does.
pub(crate) fn close_fd(emu: &mut Emu, fd: u64) {
    close(emu, fd);
    crate::fs::release_fd(emu, fd);
}

/// Receive without waiting: Some(Ok(bytes), 0 at the end), Some(Err(errno)), or None if nothing has come.
pub(crate) fn recv_now(emu: &mut Emu, fd: u64, buf: u64, len: u64) -> Result<Option<std::result::Result<u64, u64>>> {
    Ok(match try_recv(emu, fd, &[(buf, len)])? {
        Some(u64::MAX) => Some(Err(crate::libsystem::errno(emu)? as u64)),
        Some(n) => Some(Ok(n)),
        None => None,
    })
}

/// Send without waiting: Some(Ok(bytes sent)), Some(Err(errno)), or None if it would block.
pub(crate) fn send_now(emu: &mut Emu, fd: u64, data: &[u8]) -> Result<Option<std::result::Result<u64, u64>>> {
    Ok(match try_send(emu, fd, data)? {
        Some(u64::MAX) => Some(Err(crate::libsystem::errno(emu)? as u64)),
        Some(n) => Some(Ok(n)),
        None => None,
    })
}

/// What socket `fd` can do now.
pub(crate) fn ready(emu: &mut Emu, fd: u64) -> Readiness {
    readiness(emu, fd)
}

// ---- reading and writing ----

/// What the socket can do now.
fn readiness(emu: &mut Emu, fd: u64) -> Readiness {
    let Some(sock) = emu.sockets.socks.get(&fd).cloned() else { return Readiness::default() };
    match sock.kind {
        Kind::Fresh => Readiness { writable: false, readable: false, closed: false },
        Kind::Tcp(id) => emu.net.client.as_mut().map_or(Readiness { closed: true, ..Default::default() }, |n| n.readiness(id)),
        Kind::Local { inbox, peer, shut_write } => {
            let peer_open = peer.is_some_and(|p| matches!(emu.sockets.socks.get(&p), Some(Sock { kind: Kind::Local { shut_write: false, .. }, .. })));
            Readiness { readable: !inbox.is_empty() || !peer_open, writable: peer.is_some() && !shut_write, closed: !peer_open && inbox.is_empty() }
        }
    }
}

/// Try to receive into `bufs`: Some(bytes read, 0 at end of stream) or
/// Some(-1) with errno; None if nothing has come yet.
fn try_recv(emu: &mut Emu, fd: u64, bufs: &[(u64, u64)]) -> Result<Option<u64>> {
    let max: u64 = bufs.iter().map(|b| b.1).sum();
    let Some(sock) = emu.sockets.socks.get(&fd).cloned() else {
        set_errno(emu, ENOTSOCK)?;
        return Ok(Some(u64::MAX));
    };
    let data = match sock.kind {
        Kind::Fresh => {
            set_errno(emu, ENOTCONN)?;
            return Ok(Some(u64::MAX));
        }
        Kind::Tcp(id) => {
            let r = emu.net.client.as_mut().map(|n| n.recv(id, max as usize));
            match r {
                Some(Ok(d)) => d,
                Some(Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(None),
                Some(Err(e)) => {
                    set_errno(emu, errno_of(&e))?;
                    return Ok(Some(u64::MAX));
                }
                None => Vec::new(),
            }
        }
        Kind::Local { .. } => {
            let ready = readiness(emu, fd);
            let Some(Sock { kind: Kind::Local { inbox, .. }, .. }) = emu.sockets.socks.get_mut(&fd) else { unreachable!() };
            if inbox.is_empty() {
                if ready.closed {
                    Vec::new()
                } else {
                    return Ok(None);
                }
            } else {
                let n = inbox.len().min(max as usize);
                inbox.drain(..n).collect()
            }
        }
    };
    let mut at = 0;
    for &(base, len) in bufs {
        if at >= data.len() {
            break;
        }
        let n = (len as usize).min(data.len() - at);
        emu.p.memory.write(base, &data[at..at + n])?;
        at += n;
    }
    Ok(Some(data.len() as u64))
}

/// Try to send `data`: Some(bytes sent) or Some(-1) with errno; None if it would block.
fn try_send(emu: &mut Emu, fd: u64, data: &[u8]) -> Result<Option<u64>> {
    let Some(sock) = emu.sockets.socks.get(&fd).cloned() else {
        set_errno(emu, ENOTSOCK)?;
        return Ok(Some(u64::MAX));
    };
    match sock.kind {
        Kind::Fresh => {
            set_errno(emu, ENOTCONN)?;
            Ok(Some(u64::MAX))
        }
        Kind::Tcp(id) => {
            let r = emu.net.client.as_mut().map(|n| n.send(id, data));
            match r {
                Some(Ok(n)) => Ok(Some(n as u64)),
                Some(Err(e)) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(None),
                Some(Err(e)) => {
                    set_errno(emu, errno_of(&e))?;
                    Ok(Some(u64::MAX))
                }
                None => {
                    set_errno(emu, ENETUNREACH)?;
                    Ok(Some(u64::MAX))
                }
            }
        }
        Kind::Local { peer, shut_write, .. } => {
            let Some(p) = peer.filter(|_| !shut_write) else {
                set_errno(emu, EPIPE)?;
                return Ok(Some(u64::MAX));
            };
            if let Some(Sock { kind: Kind::Local { inbox, .. }, .. }) = emu.sockets.socks.get_mut(&p) {
                inbox.extend(data);
            }
            Ok(Some(data.len() as u64))
        }
    }
}

/// recv with the socket's blocking mode: EAGAIN, or wait for data.
fn recv_into(emu: &mut Emu, fd: u64, bufs: Vec<(u64, u64)>) -> Result<()> {
    match try_recv(emu, fd, &bufs)? {
        Some(r) => {
            emu.ret(r);
            Ok(())
        }
        None if nonblocking(emu, fd) => fail(emu, EAGAIN),
        None => {
            let r = crate::threads::block(emu, crate::threads::Wait::Socket(Pending::Recv { fd, bufs }))?;
            emu.ret(r);
            Ok(())
        }
    }
}

/// send with the socket's blocking mode.
fn send_from(emu: &mut Emu, fd: u64, data: Vec<u8>) -> Result<()> {
    match try_send(emu, fd, &data)? {
        Some(r) => {
            emu.ret(r);
            Ok(())
        }
        None if nonblocking(emu, fd) => fail(emu, EAGAIN),
        None => {
            let r = crate::threads::block(emu, crate::threads::Wait::Socket(Pending::Send { fd, data }))?;
            emu.ret(r);
            Ok(())
        }
    }
}

/// `read`/`write` on a socket (from fs).
pub(crate) fn read(emu: &mut Emu, fd: u64, buf: u64, n: u64) -> Result<()> {
    recv_into(emu, fd, vec![(buf, n)])
}

pub(crate) fn write(emu: &mut Emu, fd: u64, buf: u64, n: u64) -> Result<()> {
    let data = emu.p.memory.read(buf, n as usize)?.to_vec();
    send_from(emu, fd, data)
}

/// An iovec array's (base, length) pairs.
fn iovecs(emu: &Emu, iov: u64, n: u64) -> Result<Vec<(u64, u64)>> {
    (0..n).map(|i| Ok((emu.read_u64(iov + i * 16)?, emu.read_u64(iov + i * 16 + 8)?))).collect()
}

/// `writev`/`readv` on a socket (from posix).
pub(crate) fn writev(emu: &mut Emu, fd: u64, iov: u64, n: u64) -> Result<()> {
    let mut data = Vec::new();
    for (base, len) in iovecs(emu, iov, n)? {
        data.extend_from_slice(emu.p.memory.read(base, len as usize)?);
    }
    send_from(emu, fd, data)
}

pub(crate) fn readv(emu: &mut Emu, fd: u64, iov: u64, n: u64) -> Result<()> {
    let bufs = iovecs(emu, iov, n)?;
    recv_into(emu, fd, bufs)
}

// struct msghdr: name, namelen, iov, iovlen, control, controllen, flags.
fn msghdr_iov(emu: &Emu, msg: u64) -> Result<Vec<(u64, u64)>> {
    let (iov, n) = (emu.read_u64(msg + 16)?, emu.p.memory.read_u32(msg + 24)? as u64);
    iovecs(emu, iov, n)
}

/// After recvmsg: no source address, no control data, no flags.
fn clear_msghdr_out(emu: &mut Emu, msg: u64) -> Result<()> {
    emu.p.memory.write_u32(msg + 8, 0)?;
    emu.p.memory.write_u32(msg + 40, 0)?;
    emu.p.memory.write_u32(msg + 44, 0)?;
    Ok(())
}

// ---- poll and select ----

/// One descriptor's poll events.
fn revents(emu: &mut Emu, fd: i32, events: u16) -> u16 {
    if fd < 0 {
        return 0;
    }
    let fd = fd as u64;
    let (readable, writable, hup) = if owns(emu, fd) {
        let r = readiness(emu, fd);
        (r.readable, r.writable, r.closed)
    } else if let Some(state) = crate::fs::poll_state(emu, fd) {
        state
    } else {
        return POLLNVAL;
    };
    let mut r = 0;
    if readable {
        r |= POLLIN;
    }
    if writable {
        r |= POLLOUT;
    }
    if hup {
        r |= POLLHUP;
    }
    (r & (events | POLLHUP | POLLERR)) | (r & POLLNVAL)
}

/// Fill in each pollfd's revents: how many are ready.
fn poll_check(emu: &mut Emu, fds: u64, n: u64) -> Result<u64> {
    let mut ready = 0;
    for i in 0..n {
        let at = fds + i * 8;
        let fd = emu.p.memory.read_u32(at)? as i32;
        let events = emu.p.memory.read_u16(at + 4)?;
        let r = revents(emu, fd, events);
        emu.p.memory.write_u16(at + 6, r)?;
        if r != 0 {
            ready += 1;
        }
    }
    Ok(ready)
}

/// `poll(fds, nfds, timeout_ms)`
fn poll(emu: &mut Emu) -> Result<()> {
    let (fds, n, timeout) = (emu.arg(0), emu.arg(1) & 0xffff_ffff, emu.arg(2) as u32 as i32);
    let ready = poll_check(emu, fds, n)?;
    if ready > 0 || timeout == 0 {
        emu.ret(ready);
        return Ok(());
    }
    let deadline = (timeout > 0).then(|| crate::threads::now_ns(emu) + timeout as u64 * 1_000_000);
    let r = crate::threads::block(emu, crate::threads::Wait::Socket(Pending::Poll { fds, n, deadline }))?;
    emu.ret(r);
    Ok(())
}

/// The descriptors set in an fd_set below `nfds` (32-bit words, as Darwin's).
fn fd_set_members(emu: &Emu, set: u64, nfds: u64) -> Result<Vec<u64>> {
    if set == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for fd in 0..nfds.min(1024) {
        if emu.p.memory.read_u32(set + (fd / 32) * 4)? & (1 << (fd % 32)) != 0 {
            out.push(fd);
        }
    }
    Ok(out)
}

/// Rewrite the sets to the ready descriptors: how many there are.
fn select_check(emu: &mut Emu, nfds: u64, sets: [u64; 3], wanted: &[Vec<u64>; 3]) -> Result<u64> {
    let mut ready: [Vec<u64>; 3] = Default::default();
    for (k, fds) in wanted.iter().enumerate() {
        for &fd in fds {
            let events = if k == 1 { POLLOUT } else { POLLIN };
            let r = revents(emu, fd as i32, events);
            let hit = match k {
                0 => r & (POLLIN | POLLHUP) != 0,
                1 => r & POLLOUT != 0,
                _ => r & POLLERR != 0,
            };
            if hit {
                ready[k].push(fd);
            }
        }
    }
    let count: usize = ready.iter().map(Vec::len).sum();
    if count > 0 {
        for k in 0..3 {
            if sets[k] == 0 {
                continue;
            }
            let words = nfds.min(1024).div_ceil(32);
            for w in 0..words {
                emu.p.memory.write_u32(sets[k] + w * 4, 0)?;
            }
            for &fd in &ready[k] {
                let at = sets[k] + (fd / 32) * 4;
                let word = emu.p.memory.read_u32(at)?;
                emu.p.memory.write_u32(at, word | 1 << (fd % 32))?;
            }
        }
    }
    Ok(count as u64)
}

/// `select(nfds, readfds, writefds, exceptfds, timeout)`
fn select(emu: &mut Emu) -> Result<()> {
    let nfds = emu.arg(0) & 0xffff_ffff;
    let sets = [emu.arg(1), emu.arg(2), emu.arg(3)];
    let timeout = emu.arg(4);
    let wanted = [fd_set_members(emu, sets[0], nfds)?, fd_set_members(emu, sets[1], nfds)?, fd_set_members(emu, sets[2], nfds)?];
    let ready = select_check(emu, nfds, sets, &wanted)?;
    // struct timeval {tv_sec, tv_usec}; NULL waits forever.
    let wait_ns = if timeout == 0 { None } else { Some(emu.read_u64(timeout)? * 1_000_000_000 + emu.p.memory.read_u32(timeout + 8)? as u64 * 1000) };
    if ready > 0 || wait_ns == Some(0) {
        if ready == 0 {
            for set in sets.into_iter().filter(|&s| s != 0) {
                for w in 0..nfds.min(1024).div_ceil(32) {
                    emu.p.memory.write_u32(set + w * 4, 0)?;
                }
            }
        }
        emu.ret(ready);
        return Ok(());
    }
    let deadline = wait_ns.map(|ns| crate::threads::now_ns(emu) + ns);
    let r = crate::threads::block(emu, crate::threads::Wait::Socket(Pending::Select { nfds, sets, wanted, deadline }))?;
    emu.ret(r);
    Ok(())
}

/// Whether a waiting socket call can finish: its result (see [`Pending`]).
pub(crate) fn try_complete(emu: &mut Emu, p: &Pending) -> Result<Option<u64>> {
    let now = crate::threads::now_ns(emu);
    let timed_out = |d: Option<u64>| d.is_some_and(|d| now >= d);
    Ok(match p {
        Pending::Recv { fd, bufs } => try_recv(emu, *fd, bufs)?,
        Pending::Send { fd, data } => try_send(emu, *fd, data)?,
        Pending::Poll { fds, n, deadline } => {
            let ready = poll_check(emu, *fds, *n)?;
            if ready > 0 || timed_out(*deadline) { Some(ready) } else { None }
        }
        Pending::Select { nfds, sets, wanted, deadline } => {
            let ready = select_check(emu, *nfds, *sets, wanted)?;
            if ready > 0 {
                Some(ready)
            } else if timed_out(*deadline) {
                for &set in sets.iter().filter(|&&s| s != 0) {
                    for w in 0..(*nfds).min(1024).div_ceil(32) {
                        emu.p.memory.write_u32(set + w * 4, 0)?;
                    }
                }
                Some(0)
            } else {
                None
            }
        }
    })
}

/// The host connections a waiting call is waiting on.
fn connections(emu: &Emu, p: &Pending) -> Vec<SocketId> {
    let id = |fd: u64| match emu.sockets.socks.get(&fd).map(|s| &s.kind) {
        Some(Kind::Tcp(id)) => Some(*id),
        _ => None,
    };
    match p {
        Pending::Recv { fd, .. } | Pending::Send { fd, .. } => id(*fd).into_iter().collect(),
        Pending::Poll { fds, n, .. } => (0..*n).filter_map(|i| emu.p.memory.read_u32(fds + i * 8).ok()).filter_map(|fd| id(fd as u64)).collect(),
        Pending::Select { wanted, .. } => wanted.iter().flatten().filter_map(|&fd| id(fd)).collect(),
    }
}

/// Every thread is waiting and some wait on the network: wait for it in real
/// time (no longer than until the next deadline, at most 50 ms at a time),
/// moving virtual time on by as long. False when nothing waits on the network.
pub(crate) fn network_idle(emu: &mut Emu, own: Option<&Pending>, next_deadline: Option<u64>) -> Result<bool> {
    let mut ids: Vec<SocketId> = own.map(|p| connections(emu, p)).unwrap_or_default();
    for p in crate::threads::socket_waits(emu) {
        ids.extend(connections(emu, &p));
    }
    if ids.is_empty() || emu.net.client.is_none() {
        return Ok(false);
    }
    let now = crate::threads::now_ns(emu);
    let cap = std::time::Duration::from_millis(50);
    let timeout = next_deadline.map_or(cap, |d| std::time::Duration::from_nanos(d.saturating_sub(now)).min(cap));
    let start = std::time::Instant::now();
    if let Some(n) = emu.net.client.as_mut() {
        n.wait(&ids, timeout);
    }
    let elapsed = (start.elapsed().as_nanos() as u64).max(1_000_000);
    crate::threads::skip_to(emu, now + elapsed)?;
    Ok(true)
}

// ---- names ----

/// A service's port: a number, or a well-known name.
fn service_port(s: &str) -> Option<u16> {
    s.parse().ok().or(match s {
        "http" => Some(80),
        "https" => Some(443),
        "ftp" => Some(21),
        "ssh" => Some(22),
        "smtp" => Some(25),
        "domain" => Some(53),
        _ => None,
    })
}

/// `getaddrinfo(node, service, hints, &res)`: numeric addresses as they are,
/// names through the host's resolver; a list of stream addresses.
fn getaddrinfo(emu: &mut Emu) -> Result<()> {
    let (node, service, hints, res) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    let (flags, family, socktype) = if hints == 0 {
        (0, 0, 0)
    } else {
        (emu.p.memory.read_u32(hints)? as u64, emu.p.memory.read_u32(hints + 4)? as u64, emu.p.memory.read_u32(hints + 8)? as u64)
    };
    if !matches!(family, 0 | AF_INET | AF_INET6) {
        emu.ret(EAI_FAMILY);
        return Ok(());
    }
    let port = if service == 0 {
        0
    } else {
        match service_port(&emu.cstr(service)?) {
            Some(p) => p,
            None => {
                emu.ret(EAI_SERVICE);
                return Ok(());
            }
        }
    };
    let host = if node == 0 { None } else { Some(emu.cstr(node)?) };
    let ips: Vec<IpAddr> = match &host {
        None if flags & AI_PASSIVE != 0 => vec![IpAddr::V6(Ipv6Addr::UNSPECIFIED), IpAddr::V4(Ipv4Addr::UNSPECIFIED)],
        None => vec![IpAddr::V6(Ipv6Addr::LOCALHOST), IpAddr::V4(Ipv4Addr::LOCALHOST)],
        Some(h) if flags & AI_NUMERICHOST != 0 => h.trim_start_matches('[').trim_end_matches(']').parse().into_iter().collect(),
        Some(h) => lookup(emu, h),
    };
    let ips: Vec<IpAddr> = ips.into_iter().filter(|ip| family == 0 || (family == AF_INET) == ip.is_ipv4()).collect();
    if ips.is_empty() {
        emu.ret(EAI_NONAME);
        return Ok(());
    }
    // struct addrinfo: flags, family, socktype, protocol, addrlen, canonname, addr, next (48 bytes).
    let socktype = if socktype == 0 { SOCK_STREAM } else { socktype };
    let mut head = 0;
    for (i, ip) in ips.iter().enumerate().rev() {
        let bytes = sockaddr_bytes(SocketAddr::new(*ip, port));
        let ai = emu.alloc(48)?;
        let addr = emu.alloc(bytes.len() as u64)?;
        emu.p.memory.write(addr, &bytes)?;
        emu.p.memory.write(ai, &[0u8; 48])?;
        emu.p.memory.write_u32(ai + 4, if ip.is_ipv4() { AF_INET } else { AF_INET6 } as u32)?;
        emu.p.memory.write_u32(ai + 8, socktype as u32)?;
        emu.p.memory.write_u32(ai + 12, if socktype == SOCK_STREAM { 6 } else { 17 })?;
        emu.p.memory.write_u32(ai + 16, bytes.len() as u32)?;
        if i == 0
            && flags & AI_CANONNAME != 0
            && let Some(h) = &host
        {
            let c = emu.alloc_cstr(h)?;
            emu.write_u64(ai + 24, c)?;
        }
        emu.write_u64(ai + 32, addr)?;
        emu.write_u64(ai + 40, head)?;
        head = ai;
    }
    emu.write_u64(res, head)?;
    emu.ret(0);
    Ok(())
}

/// A host's addresses: a literal address, localhost, or the resolver's answer
/// while the network is up (none otherwise, or when the name isn't known).
pub(crate) fn lookup(emu: &mut Emu, host: &str) -> Vec<IpAddr> {
    match host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>() {
        Ok(ip) => vec![ip],
        Err(_) if host == "localhost" => vec![IpAddr::V6(Ipv6Addr::LOCALHOST), IpAddr::V4(Ipv4Addr::LOCALHOST)],
        Err(_) if online(emu) => emu.net.client.as_mut().and_then(|n| n.resolve(host).ok()).unwrap_or_default(),
        Err(_) => vec![],
    }
}

/// `gai_strerror(code)`
fn gai_strerror(emu: &mut Emu) -> Result<()> {
    let msg = match emu.arg(0) as u32 as i32 {
        2 => "Temporary failure in name resolution",
        4 => "Non-recoverable failure in name resolution",
        5 => "ai_family not supported",
        8 => "nodename nor servname provided, or not known",
        9 => "servname not supported for ai_socktype",
        _ => "Unknown error",
    };
    let s = emu.alloc_cstr(msg)?;
    emu.ret(s);
    Ok(())
}

/// `inet_pton(af, src, dst)`: 1, 0 if `src` isn't an address of that family, -1 for an unknown family.
fn inet_pton(emu: &mut Emu) -> Result<()> {
    let (af, src, dst) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2));
    let text = emu.cstr(src)?;
    let r = match af {
        AF_INET => text.parse::<Ipv4Addr>().ok().map(|a| a.octets().to_vec()),
        AF_INET6 => text.parse::<Ipv6Addr>().ok().map(|a| a.octets().to_vec()),
        _ => return fail(emu, EAFNOSUPPORT),
    };
    match r {
        Some(bytes) => {
            emu.p.memory.write(dst, &bytes)?;
            emu.ret(1);
        }
        None => emu.ret(0),
    }
    Ok(())
}

/// `inet_ntop(af, src, dst, size)`: `dst`, or NULL if it's too small.
fn inet_ntop(emu: &mut Emu) -> Result<()> {
    let (af, src, dst, size) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2), emu.arg(3) & 0xffff_ffff);
    let text = match af {
        AF_INET => {
            let b = emu.p.memory.read(src, 4)?;
            Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string()
        }
        AF_INET6 => {
            let mut a = [0u8; 16];
            a.copy_from_slice(emu.p.memory.read(src, 16)?);
            Ipv6Addr::from(a).to_string()
        }
        _ => {
            set_errno(emu, EAFNOSUPPORT)?;
            emu.ret(0);
            return Ok(());
        }
    };
    if text.len() as u64 + 1 > size {
        set_errno(emu, 28)?; // ENOSPC
        emu.ret(0);
        return Ok(());
    }
    emu.p.memory.write(dst, text.as_bytes())?;
    emu.p.memory.write_u8(dst + text.len() as u64, 0)?;
    emu.ret(dst);
    Ok(())
}

// ---- options and addresses ----

/// `getsockopt(fd, level, name, value, &len)`: SO_ERROR (none pending: the
/// connection opened or failed at connect), SO_TYPE, and what was set.
fn getsockopt(emu: &mut Emu) -> Result<()> {
    let (fd, level, name, value, len) = (emu.arg(0) & 0xffff_ffff, emu.arg(1) & 0xffff_ffff, emu.arg(2) & 0xffff_ffff, emu.arg(3), emu.arg(4));
    let Some(sock) = emu.sockets.socks.get(&fd).cloned() else { return fail(emu, ENOTSOCK) };
    let bytes = match (level, name) {
        (SOL_SOCKET, SO_ERROR) => 0u32.to_le_bytes().to_vec(),
        (SOL_SOCKET, SO_TYPE) => (sock.ty as u32).to_le_bytes().to_vec(),
        (SOL_SOCKET, 0x1001 | 0x1002) => sock.options.get(&(level, name)).cloned().unwrap_or_else(|| 131_072u32.to_le_bytes().to_vec()),
        _ => sock.options.get(&(level, name)).cloned().unwrap_or_else(|| 0u32.to_le_bytes().to_vec()),
    };
    let room = emu.p.memory.read_u32(len)? as usize;
    emu.p.memory.write(value, &bytes[..bytes.len().min(room)])?;
    emu.p.memory.write_u32(len, bytes.len().min(room) as u32)?;
    emu.ret(0);
    Ok(())
}

/// `setsockopt(fd, level, name, value, len)`: kept (TCP_NODELAY, keep-alive,
/// buffer sizes and the like change nothing pomme models).
fn setsockopt(emu: &mut Emu) -> Result<()> {
    let (fd, level, name, value, len) = (emu.arg(0) & 0xffff_ffff, emu.arg(1) & 0xffff_ffff, emu.arg(2) & 0xffff_ffff, emu.arg(3), emu.arg(4) & 0xffff_ffff);
    let bytes = if value == 0 { Vec::new() } else { emu.p.memory.read(value, len as usize)?.to_vec() };
    let Some(sock) = emu.sockets.socks.get_mut(&fd) else { return fail(emu, ENOTSOCK) };
    sock.options.insert((level, name), bytes);
    emu.ret(0);
    Ok(())
}

fn name_of(emu: &mut Emu, peer: bool) -> Result<()> {
    let (fd, out, len) = (emu.arg(0) & 0xffff_ffff, emu.arg(1), emu.arg(2));
    let Some(sock) = emu.sockets.socks.get(&fd).cloned() else { return fail(emu, ENOTSOCK) };
    let addr = match sock.kind {
        Kind::Tcp(id) => emu.net.client.as_mut().and_then(|n| if peer { n.peer_addr(id) } else { n.local_addr(id) }),
        Kind::Fresh if !peer => Some(if sock.domain == AF_INET6 { SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0) } else { SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0) }),
        Kind::Local { .. } => {
            // An unnamed AF_UNIX socket: just its family.
            let room = emu.p.memory.read_u32(len)?;
            if out != 0 && room >= 2 {
                emu.p.memory.write(out, &[2, AF_UNIX as u8])?;
            }
            emu.p.memory.write_u32(len, 2)?;
            emu.ret(0);
            return Ok(());
        }
        _ => None,
    };
    match addr {
        Some(a) => {
            write_sockaddr(emu, a, out, len)?;
            emu.ret(0);
            Ok(())
        }
        None => fail(emu, ENOTCONN),
    }
}

/// `if_nametoindex(name)`: the loopback, and the interfaces up on the modelled network.
fn if_nametoindex(emu: &mut Emu) -> Result<()> {
    let name = emu.cstr(emu.arg(0))?;
    let index = if name == "lo0" { 1 } else { crate::network::interface_index(emu, &name).unwrap_or(0) };
    emu.ret(index);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_socket", socket),
    ("_socketpair", socketpair),
    ("_connect", connect),
    ("_shutdown", shutdown),
    ("_send", |e| {
        let (fd, buf, n) = (e.arg(0) & 0xffff_ffff, e.arg(1), e.arg(2));
        write(e, fd, buf, n)
    }),
    ("_sendto", |e| {
        let (fd, buf, n) = (e.arg(0) & 0xffff_ffff, e.arg(1), e.arg(2));
        write(e, fd, buf, n)
    }),
    ("_sendmsg", |e| {
        let (fd, msg) = (e.arg(0) & 0xffff_ffff, e.arg(1));
        let mut data = Vec::new();
        for (base, len) in msghdr_iov(e, msg)? {
            data.extend_from_slice(e.p.memory.read(base, len as usize)?);
        }
        send_from(e, fd, data)
    }),
    ("_recv", |e| {
        let (fd, buf, n) = (e.arg(0) & 0xffff_ffff, e.arg(1), e.arg(2));
        read(e, fd, buf, n)
    }),
    // (fd, buf, len, flags, from, fromlen): a connected stream's peer.
    ("_recvfrom", |e| {
        let (fd, buf, n, from, fromlen) = (e.arg(0) & 0xffff_ffff, e.arg(1), e.arg(2), e.arg(4), e.arg(5));
        if from != 0 && fromlen != 0 {
            e.p.memory.write_u32(fromlen, 0)?;
        }
        read(e, fd, buf, n)
    }),
    ("_recvmsg", |e| {
        let (fd, msg) = (e.arg(0) & 0xffff_ffff, e.arg(1));
        let bufs = msghdr_iov(e, msg)?;
        clear_msghdr_out(e, msg)?;
        recv_into(e, fd, bufs)
    }),
    ("_poll", poll),
    ("_select", select),
    ("_getaddrinfo", getaddrinfo),
    ("_freeaddrinfo", |_| Ok(())),
    ("_gai_strerror", gai_strerror),
    ("_inet_pton", inet_pton),
    ("_inet_ntop", inet_ntop),
    ("_getsockopt", getsockopt),
    ("_setsockopt", setsockopt),
    ("_getsockname", |e| name_of(e, false)),
    ("_getpeername", |e| name_of(e, true)),
    ("_if_nametoindex", if_nametoindex),
    // Listening sockets (servers) aren't modelled yet.
    ("_bind", |e| {
        let fd = e.arg(0) & 0xffff_ffff;
        if owns(e, fd) {
            e.ret(0);
            Ok(())
        } else {
            fail(e, ENOTSOCK)
        }
    }),
    ("_listen", |e| fail(e, EOPNOTSUPP)),
    ("_accept", |e| fail(e, EINVAL)),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sockaddrs() {
        let v4: SocketAddr = "142.250.70.10:443".parse().unwrap();
        let b = sockaddr_bytes(v4);
        assert_eq!(b.len(), 16);
        assert_eq!(&b[..8], &[16, 2, 1, 187, 142, 250, 70, 10]);
        let v6: SocketAddr = "[2404:6800:4006:80f::200a]:443".parse().unwrap();
        let b = sockaddr_bytes(v6);
        assert_eq!((b.len(), b[1]), (28, 30));
        assert_eq!(service_port("https"), Some(443));
        assert_eq!(service_port("8080"), Some(8080));
        assert_eq!(service_port("gopher"), None);
    }
}

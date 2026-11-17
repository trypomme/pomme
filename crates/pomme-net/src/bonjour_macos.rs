//! Bonjour browsing on macOS through the system's DNS Service Discovery API
//! (`dns_sd.h`, mDNSResponder), as `dns-sd -B` does: a command-line process's
//! own multicast is subject to macOS's local network privacy; asking the
//! system's responder isn't.
//!
//! One thread per kind: it browses, and resolves each service it finds (its
//! host, port and TXT record, then the host's addresses) into the shared list.

use pomme_hle::net::Service;
use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_void};
use std::sync::{Arc, Mutex};

type Ref = *mut c_void;

const FLAGS_MORE_COMING: u32 = 0x1;
const FLAGS_ADD: u32 = 0x2;
const PROTOCOL_IPV4: u32 = 0x1;
const PROTOCOL_IPV6: u32 = 0x2;

type BrowseReply = extern "C" fn(Ref, u32, u32, i32, *const c_char, *const c_char, *const c_char, *mut c_void);
type ResolveReply = extern "C" fn(Ref, u32, u32, i32, *const c_char, *const c_char, u16, u16, *const u8, *mut c_void);
type AddrReply = extern "C" fn(Ref, u32, u32, i32, *const c_char, *const libc_sockaddr, u32, *mut c_void);

#[repr(C)]
struct libc_sockaddr {
    len: u8,
    family: u8,
    data: [u8; 14],
}

unsafe extern "C" {
    fn DNSServiceBrowse(sd: *mut Ref, flags: u32, interface: u32, regtype: *const c_char, domain: *const c_char, reply: BrowseReply, context: *mut c_void) -> i32;
    fn DNSServiceResolve(sd: *mut Ref, flags: u32, interface: u32, name: *const c_char, regtype: *const c_char, domain: *const c_char, reply: ResolveReply, context: *mut c_void) -> i32;
    fn DNSServiceGetAddrInfo(sd: *mut Ref, flags: u32, interface: u32, protocol: u32, hostname: *const c_char, reply: AddrReply, context: *mut c_void) -> i32;
    fn DNSServiceProcessResult(sd: Ref) -> i32;
    fn DNSServiceRefSockFD(sd: Ref) -> i32;
    fn DNSServiceRefDeallocate(sd: Ref);
    fn poll(fds: *mut PollFd, n: u32, timeout_ms: i32) -> i32;
}

#[repr(C)]
struct PollFd {
    fd: i32,
    events: i16,
    revents: i16,
}

/// Wait up to `ms` for `sd`'s answer and handle it; whether one came.
fn answer(sd: Ref, ms: i32) -> bool {
    let mut p = PollFd { fd: unsafe { DNSServiceRefSockFD(sd) }, events: 1, revents: 0 };
    if unsafe { poll(&mut p, 1, ms) } <= 0 {
        return false;
    }
    unsafe { DNSServiceProcessResult(sd) == 0 }
}

fn text(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

/// A TXT record's `key=value` strings.
fn txt(bytes: &[u8]) -> Vec<(String, Option<Vec<u8>>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let n = bytes[i] as usize;
        let item = &bytes[(i + 1).min(bytes.len())..(i + 1 + n).min(bytes.len())];
        i += 1 + n;
        if item.is_empty() {
            continue;
        }
        match item.iter().position(|&b| b == b'=') {
            Some(eq) => out.push((String::from_utf8_lossy(&item[..eq]).into_owned(), Some(item[eq + 1..].to_vec()))),
            None => out.push((String::from_utf8_lossy(item).into_owned(), None)),
        }
    }
    out
}

/// What a resolve answers.
#[derive(Default)]
struct Resolved {
    host: String,
    port: u16,
    txt: Vec<(String, Option<Vec<u8>>)>,
    done: bool,
}

extern "C" fn on_resolve(_: Ref, _: u32, _: u32, err: i32, _: *const c_char, host: *const c_char, port: u16, len: u16, record: *const u8, ctx: *mut c_void) {
    let r = unsafe { &mut *(ctx as *mut Resolved) };
    if err == 0 {
        r.host = text(host);
        r.port = u16::from_be(port);
        let bytes = if record.is_null() { &[][..] } else { unsafe { std::slice::from_raw_parts(record, len as usize) } };
        r.txt = txt(bytes);
    }
    r.done = true;
}

#[derive(Default)]
struct Addresses {
    list: Vec<std::net::IpAddr>,
    more: bool,
}

extern "C" fn on_address(_: Ref, flags: u32, _: u32, err: i32, _: *const c_char, sa: *const libc_sockaddr, _: u32, ctx: *mut c_void) {
    let a = unsafe { &mut *(ctx as *mut Addresses) };
    a.more = flags & FLAGS_MORE_COMING != 0;
    if err != 0 || sa.is_null() || flags & FLAGS_ADD == 0 {
        return;
    }
    let s = unsafe { &*sa };
    // sockaddr_in: port (2), then the address (4); sockaddr_in6: port, flow (4), then 16.
    match s.family {
        2 => a.list.push(std::net::IpAddr::from([s.data[2], s.data[3], s.data[4], s.data[5]])),
        30 => {
            let six = unsafe { std::slice::from_raw_parts((sa as *const u8).add(8), 16) };
            let mut b = [0u8; 16];
            b.copy_from_slice(six);
            a.list.push(std::net::IpAddr::from(b));
        }
        _ => {}
    }
}

/// A found service, resolved: None if it doesn't answer in time.
fn resolve(name: &str, kind: &str, domain: &str, interface: u32) -> Option<Service> {
    let (n, k, d) = (CString::new(name).ok()?, CString::new(kind).ok()?, CString::new(domain).ok()?);
    let mut r = Resolved::default();
    let mut sd: Ref = std::ptr::null_mut();
    if unsafe { DNSServiceResolve(&mut sd, 0, interface, n.as_ptr(), k.as_ptr(), d.as_ptr(), on_resolve, &mut r as *mut _ as *mut c_void) } != 0 {
        return None;
    }
    while !r.done && answer(sd, 3000) {}
    unsafe { DNSServiceRefDeallocate(sd) };
    if !r.done || r.host.is_empty() {
        return None;
    }
    let mut a = Addresses::default();
    let h = CString::new(r.host.clone()).ok()?;
    let mut sd: Ref = std::ptr::null_mut();
    if unsafe { DNSServiceGetAddrInfo(&mut sd, 0, interface, PROTOCOL_IPV4 | PROTOCOL_IPV6, h.as_ptr(), on_address, &mut a as *mut _ as *mut c_void) } == 0 {
        // Every address that comes at once (the last without "more coming").
        while answer(sd, if a.list.is_empty() { 3000 } else { 300 }) && a.more {}
        unsafe { DNSServiceRefDeallocate(sd) };
    }
    Some(Service { name: name.to_string(), kind: kind.trim_end_matches('.').to_string(), domain: domain.to_string(), host: r.host, port: r.port, addresses: a.list, txt: r.txt })
}

/// What the browse thread keeps: the found list, and the services to resolve.
struct Browse {
    found: Arc<Mutex<HashMap<String, Service>>>,
    pending: Vec<(String, String, String, u32)>,
    gone: Vec<String>,
}

extern "C" fn on_browse(_: Ref, flags: u32, interface: u32, err: i32, name: *const c_char, kind: *const c_char, domain: *const c_char, ctx: *mut c_void) {
    if err != 0 {
        return;
    }
    let b = unsafe { &mut *(ctx as *mut Browse) };
    let name = text(name);
    if flags & FLAGS_ADD != 0 {
        b.pending.push((name, text(kind), text(domain), interface));
    } else {
        b.gone.push(name);
    }
}

/// Start looking for `kind` (`_expo._tcp`), keeping `found` up to date.
pub fn start(kind: &str, found: Arc<Mutex<HashMap<String, Service>>>) {
    let kind = kind.to_string();
    std::thread::spawn(move || {
        let Ok(k) = CString::new(kind) else { return };
        let mut b = Browse { found, pending: Vec::new(), gone: Vec::new() };
        let mut sd: Ref = std::ptr::null_mut();
        if unsafe { DNSServiceBrowse(&mut sd, 0, 0, k.as_ptr(), std::ptr::null(), on_browse, &mut b as *mut _ as *mut c_void) } != 0 {
            return;
        }
        loop {
            if !answer(sd, 1000) {
                continue;
            }
            for name in std::mem::take(&mut b.gone) {
                b.found.lock().unwrap().remove(&name);
            }
            for (name, kind, domain, interface) in std::mem::take(&mut b.pending) {
                if let Some(s) = resolve(&name, &kind, &domain, interface) {
                    b.found.lock().unwrap().insert(name, s);
                }
            }
        }
    });
}

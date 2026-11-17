//! DNS Service Discovery (`<dns_sd.h>`): `DNSServiceGetAddrInfo`, which
//! gRPC (Firestore's transport) and other networking code resolve hostnames
//! with, its results delivered on a dispatch queue.
//!
//! As mDNSResponder does: once a request has a queue, each address arrives as
//! a callback with `kDNSServiceFlagsAdd`, every one but the last also with
//! `kDNSServiceFlagsMoreComing`. With `kDNSServiceFlagsReturnIntermediates`, a
//! family with no addresses is reported as `kDNSServiceErr_NoSuchRecord` with
//! an empty address of that family (so the caller knows the lookup is done);
//! without it, `kDNSServiceFlagsTimeout` turns a failed lookup into
//! `kDNSServiceErr_Timeout`, and otherwise the request waits, as one does on a
//! phone with no answer. Addresses come from the host resolver (see `Net`).
//!
//! Bonjour browsing, for pomme's Network overlay (NWBrowser, NWConnection to
//! a service): `pomme_bonjour_browse`, the services of a kind the computer's
//! network has (`Net::browse`), as text.

use crate::{Emu, HostFn, Result};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};

// DNSServiceFlags
const MORE_COMING: u64 = 0x1;
const ADD: u64 = 0x2;
const RETURN_INTERMEDIATES: u64 = 0x1000;
const TIMEOUT: u64 = 0x10000;

// DNSServiceProtocol
const IPV4: u64 = 0x1;
const IPV6: u64 = 0x2;

// DNSServiceErrorType
const NO_ERROR: i32 = 0;
const BAD_PARAM: i32 = -65540;
const NO_SUCH_RECORD: i32 = -65554;
const TIMED_OUT: i32 = -65568;

/// How long answers may be cached, as records report it.
const TTL: u64 = 60;

/// A request, from `DNSServiceGetAddrInfo` to `DNSServiceRefDeallocate`.
#[derive(Clone)]
struct Request {
    /// Unique across requests (a freed DNSServiceRef's address may be reused).
    id: u64,
    flags: u64,
    protocol: u64,
    callback: u64,
    context: u64,
    hostname: String,
    started: bool,
    /// Guest memory its callbacks point into, freed with the request.
    owned: Vec<u64>,
}

/// One callback waiting on a request's queue.
#[derive(Clone, Copy)]
struct Reply {
    sd_ref: u64,
    id: u64,
    flags: u64,
    error: i32,
    hostname: u64,
    address: u64,
}

#[derive(Clone, Default)]
pub(crate) struct State {
    requests: HashMap<u64, Request>,
    replies: Vec<Option<Reply>>,
    next_id: u64,
}

fn ret_error(emu: &mut Emu, e: i32) {
    emu.ret(e as u32 as u64);
}

/// `DNSServiceGetAddrInfo(sdRef, flags, interfaceIndex, protocol, hostname, callBack, context)`
fn get_addr_info(emu: &mut Emu) -> Result<()> {
    let (out, flags, protocol, name, callback, context) = (emu.arg(0), emu.arg(1), emu.arg(3), emu.arg(4), emu.arg(5), emu.arg(6));
    if out == 0 || name == 0 || callback == 0 || protocol & !(IPV4 | IPV6) != 0 {
        ret_error(emu, BAD_PARAM);
        return Ok(());
    }
    let hostname = emu.cstr(name)?;
    // The opaque DNSServiceRef: a block of its own, so it has a unique address.
    let sd_ref = emu.alloc(16)?;
    let protocol = if protocol == 0 { IPV4 | IPV6 } else { protocol };
    emu.dnssd.next_id += 1;
    let id = emu.dnssd.next_id;
    emu.dnssd.requests.insert(sd_ref, Request { id, flags, protocol, callback, context, hostname, started: false, owned: vec![] });
    emu.write_u64(out, sd_ref)?;
    ret_error(emu, NO_ERROR);
    Ok(())
}

/// `DNSServiceSetDispatchQueue(sdRef, queue)`: look the name up, and deliver
/// the answers on `queue`.
fn set_dispatch_queue(emu: &mut Emu) -> Result<()> {
    let (sd_ref, queue) = (emu.arg(0), emu.arg(1));
    let started = emu.dnssd.requests.get(&sd_ref).map(|r| r.started);
    if queue == 0 || started != Some(false) {
        ret_error(emu, BAD_PARAM);
        return Ok(());
    }
    let req = emu.dnssd.requests.get_mut(&sd_ref).expect("request");
    req.started = true;
    let (id, flags, protocol, host) = (req.id, req.flags, req.protocol, req.hostname.clone());
    let ips = crate::sockets::lookup(emu, &host);

    // (error, address) per callback.
    let mut answers: Vec<(i32, Option<SocketAddr>)> = vec![];
    for (family, v4) in [(IPV4, true), (IPV6, false)] {
        if protocol & family == 0 {
            continue;
        }
        let found: Vec<IpAddr> = ips.iter().copied().filter(|ip| ip.is_ipv4() == v4).collect();
        if !found.is_empty() {
            answers.extend(found.into_iter().map(|ip| (NO_ERROR, Some(SocketAddr::new(ip, 0)))));
        } else if flags & RETURN_INTERMEDIATES != 0 {
            let none: IpAddr = if v4 { std::net::Ipv4Addr::UNSPECIFIED.into() } else { std::net::Ipv6Addr::UNSPECIFIED.into() };
            answers.push((NO_SUCH_RECORD, Some(SocketAddr::new(none, 0))));
        }
    }
    if ips.is_empty() && flags & RETURN_INTERMEDIATES == 0 {
        if flags & TIMEOUT == 0 {
            // No answer: the request waits until it's deallocated.
            ret_error(emu, NO_ERROR);
            return Ok(());
        }
        answers = vec![(TIMED_OUT, None)];
    }

    let hostname = emu.alloc_cstr(&host)?;
    let mut owned = vec![hostname];
    let last = answers.len().saturating_sub(1);
    let deliver = emu.host_thunk(run_reply)?;
    for (i, (error, addr)) in answers.into_iter().enumerate() {
        let address = match addr {
            Some(a) => {
                let bytes = crate::sockets::sockaddr_bytes(a);
                let p = emu.alloc(bytes.len() as u64)?;
                emu.p.memory.write(p, &bytes)?;
                owned.push(p);
                p
            }
            None => 0,
        };
        let mut f = if error == NO_ERROR { ADD } else { 0 };
        if i != last {
            f |= MORE_COMING;
        }
        let idx = emu.dnssd.replies.len() as u64;
        emu.dnssd.replies.push(Some(Reply { sd_ref, id, flags: f, error, hostname, address }));
        crate::libsystem::dispatch_on(emu, queue, deliver, idx)?;
    }
    if let Some(req) = emu.dnssd.requests.get_mut(&sd_ref) {
        req.owned = owned;
    }
    ret_error(emu, NO_ERROR);
    Ok(())
}

/// On the request's queue: `callBack(sdRef, flags, interfaceIndex, errorCode,
/// hostname, address, ttl, context)`, as a tail call so the callback runs as
/// ordinary guest code (it may block on locks, or deallocate the request).
fn run_reply(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(0) as usize;
    let Some(r) = emu.dnssd.replies.get_mut(i).and_then(Option::take) else { return Ok(()) };
    let Some(req) = emu.dnssd.requests.get(&r.sd_ref).filter(|q| q.id == r.id) else { return Ok(()) };
    let (callback, context) = (req.callback, req.context);
    let index = crate::network::primary_index(emu);
    let args = [r.sd_ref, r.flags, index, r.error as u32 as u64, r.hostname, r.address, TTL, context];
    emu.cpu.x[..8].copy_from_slice(&args);
    emu.cpu.pc = callback;
    Ok(())
}

/// `DNSServiceRefDeallocate(sdRef)`: cancel; no more callbacks.
fn ref_deallocate(emu: &mut Emu) -> Result<()> {
    let sd_ref = emu.arg(0);
    if let Some(req) = emu.dnssd.requests.remove(&sd_ref) {
        for p in req.owned.into_iter().chain([sd_ref]) {
            emu.heap.free(&emu.p.memory, p)?;
        }
    }
    Ok(())
}

/// `size_t pomme_bonjour_browse(const char *kind, char *out, size_t cap)`: the
/// services of `kind` found so far, one a line: name, kind, domain, host,
/// port, addresses (comma separated) and the TXT record (`key=hex` or `key`,
/// semicolon separated), tab separated. Writes it (with a NUL) when it fits in
/// `cap`; returns its length either way. None while the network is offline.
fn bonjour_browse(emu: &mut Emu) -> Result<()> {
    let (kind, out, cap) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let kind = emu.cstr(kind)?;
    let online = emu.network.path != crate::network::Path::Offline;
    let services = match emu.net.client.as_mut() {
        Some(n) if online => n.browse(&kind),
        _ => Vec::new(),
    };
    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    let mut text = String::new();
    for s in services {
        let addrs: Vec<String> = s.addresses.iter().map(|a| a.to_string()).collect();
        let txt: Vec<String> = s.txt.iter().map(|(k, v)| v.as_ref().map_or_else(|| k.clone(), |v| format!("{k}={}", hex(v)))).collect();
        text += &format!("{}\t{}\t{}\t{}\t{}\t{}\t{}\n", s.name, s.kind, s.domain, s.host, s.port, addrs.join(","), txt.join(";"));
    }
    if out != 0 && (text.len() as u64) < cap {
        emu.p.memory.write(out, text.as_bytes())?;
        emu.p.memory.write_u8(out + text.len() as u64, 0)?;
    }
    emu.ret(text.len() as u64);
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_pomme_bonjour_browse", bonjour_browse),
    ("_DNSServiceGetAddrInfo", get_addr_info),
    ("_DNSServiceSetDispatchQueue", set_dispatch_queue),
    ("_DNSServiceRefDeallocate", ref_deallocate),
];

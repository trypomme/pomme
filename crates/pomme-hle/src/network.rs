//! The device's network as the Network framework (`nw_path_*`) and libc
//! (`getifaddrs`) report it: a modeled path, Wi-Fi by default, or cellular,
//! wired or offline (`--network`), which can change while the app runs
//! (`--network offline@3`, or the window's controls). Path monitors hear
//! the current path when they start and every change after, on their
//! queue, as NWPathMonitor does.
//!
//! This is what the system says about connectivity; whether requests go
//! out is net.rs's business (`--offline` makes both agree).

use crate::objc::{self, host_class};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

/// How the phone is connected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Path {
    #[default]
    Wifi,
    Cellular,
    Wired,
    Offline,
}

impl std::str::FromStr for Path {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, String> {
        Ok(match s {
            "wifi" | "wi-fi" => Path::Wifi,
            "cellular" => Path::Cellular,
            "wired" | "ethernet" => Path::Wired,
            "offline" | "none" => Path::Offline,
            o => return Err(format!("{o}: expected wifi, cellular, wired or offline")),
        })
    }
}

/// nw_interface_type_t
const OTHER: u64 = 0;
const WIFI: u64 = 1;
const CELLULAR: u64 = 2;
const WIRED: u64 = 3;

/// nw_path_status_t
const SATISFIED: u64 = 1;
const UNSATISFIED: u64 = 2;

/// nw_path_unsatisfied_reason_t
const REASON_NONE: u64 = 0;
const REASON_NOT_AVAILABLE: u64 = 1;

/// An interface: its nw_interface_type_t, BSD name and index.
#[derive(Clone, Debug)]
struct Interface {
    kind: u64,
    name: &'static str,
    index: u64,
}

/// The interfaces up for a path (the primary one first), and its IPv4 address.
fn interfaces(p: Path) -> (Vec<Interface>, Option<[u8; 4]>) {
    match p {
        Path::Wifi => (vec![Interface { kind: WIFI, name: "en0", index: 11 }], Some([192, 168, 1, 23])),
        Path::Cellular => (vec![Interface { kind: CELLULAR, name: "pdp_ip0", index: 3 }], Some([10, 64, 12, 7])),
        Path::Wired => (vec![Interface { kind: WIRED, name: "en2", index: 13 }], Some([192, 168, 1, 41])),
        Path::Offline => (vec![], None),
    }
}

/// A path as a monitor saw it.
#[derive(Clone, Debug)]
struct PathInfo {
    satisfied: bool,
    interfaces: Vec<Interface>,
    expensive: bool,
}

#[derive(Default, Clone)]
struct Monitor {
    queue: u64,
    handler: u64,
    cancel_handler: u64,
    started: bool,
    cancelled: bool,
    /// nw_path_monitor_create_with_type: only this interface type counts.
    required: Option<u64>,
    prohibited: Vec<u64>,
}

#[derive(Default)]
pub struct State {
    pub path: Path,
    /// Scripted changes: (virtual ns, the path then).
    pub changes: Vec<(u64, Path)>,
    monitors: HashMap<u64, Monitor>,
    paths: HashMap<u64, PathInfo>,
    interfaces: HashMap<u64, Interface>,
    /// Updates waiting to run on their queues: (handler, path object).
    pending: Vec<Option<(u64, u64)>>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

/// The path as `m` sees it: its required and prohibited interface types apply.
fn path_for(emu: &Emu, m: &Monitor) -> PathInfo {
    let (all, _) = interfaces(emu.network.path);
    let usable: Vec<Interface> = all.into_iter().filter(|i| m.required.is_none_or(|t| t == i.kind) && !m.prohibited.contains(&i.kind)).collect();
    PathInfo { satisfied: !usable.is_empty(), expensive: usable.iter().any(|i| i.kind == CELLULAR), interfaces: usable }
}

fn new_path(emu: &mut Emu, info: PathInfo) -> Result<u64> {
    let p = new_object(emu, "OS_nw_path")?;
    emu.network.paths.insert(p, info);
    Ok(p)
}

fn new_interface(emu: &mut Emu, i: Interface) -> Result<u64> {
    let o = new_object(emu, "OS_nw_interface")?;
    emu.network.interfaces.insert(o, i);
    Ok(o)
}

/// Give monitor `m` the current path, on its queue.
fn update(emu: &mut Emu, m: u64) -> Result<()> {
    let Some(mon) = emu.network.monitors.get(&m).cloned() else { return Ok(()) };
    if !mon.started || mon.cancelled || mon.handler == 0 {
        return Ok(());
    }
    let info = path_for(emu, &mon);
    let path = new_path(emu, info)?;
    let i = emu.network.pending.len();
    emu.network.pending.push(Some((mon.handler, path)));
    let f = emu.host_thunk(run_update)?;
    crate::libsystem::dispatch_on(emu, mon.queue, f, i as u64)
}

/// On the monitor's queue: `handler(path)`.
fn run_update(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(0) as usize;
    let Some((handler, path)) = emu.network.pending.get_mut(i).and_then(Option::take) else { return Ok(()) };
    crate::libsystem::call_block(emu, handler, &[path])?;
    Ok(())
}

/// The network changes: every running monitor hears the new path.
pub fn set_path(emu: &mut Emu, p: Path) -> Result<()> {
    if emu.network.path == p {
        return Ok(());
    }
    emu.network.path = p;
    let monitors: Vec<u64> = emu.network.monitors.keys().copied().collect();
    for m in monitors {
        update(emu, m)?;
    }
    crate::reachability::path_changed(emu)
}

/// Put the scripted changes (`--network offline@3`) on the main run loop.
pub fn schedule_script(emu: &mut Emu) -> Result<()> {
    let now = crate::threads::now_ns(emu);
    for i in 0..emu.network.changes.len() {
        let delay = emu.network.changes[i].0.saturating_sub(now) as f64 / 1e9;
        crate::uikit::after_on_main(emu, delay, |e| {
            let p = e.network.changes[e.arg(1) as usize].1;
            set_path(e, p)
        }, i as u64)?;
    }
    Ok(())
}

fn monitor(emu: &mut Emu, m: u64) -> &mut Monitor {
    emu.network.monitors.entry(m).or_default()
}

fn path(emu: &Emu, p: u64) -> Option<&PathInfo> {
    emu.network.paths.get(&p)
}

// ---- getifaddrs ----

/// `getifaddrs(&list)`: loopback, and the path's interface with its IPv4
/// address. struct ifaddrs {next, name, flags (u32), addr, netmask,
/// dstaddr, data}; sockaddr_in {len 16, family AF_INET 2, port, addr}.
fn getifaddrs(emu: &mut Emu) -> Result<()> {
    const IFF_UP: u32 = 0x1;
    const IFF_LOOPBACK: u32 = 0x8;
    const IFF_RUNNING: u32 = 0x40;
    let out = emu.arg(0);
    let (ifs, addr) = interfaces(emu.network.path);
    let mut list: Vec<(&str, u32, [u8; 4], [u8; 4])> = vec![("lo0", IFF_UP | IFF_LOOPBACK | IFF_RUNNING, [127, 0, 0, 1], [255, 0, 0, 0])];
    if let (Some(i), Some(a)) = (ifs.first(), addr) {
        list.push((i.name, IFF_UP | IFF_RUNNING, a, [255, 255, 255, 0]));
    }
    let mut next = 0u64;
    for (name, flags, a, mask) in list.into_iter().rev() {
        let node = emu.alloc(56)?;
        let n = emu.alloc_cstr(name)?;
        let sockaddr = |emu: &mut Emu, ip: [u8; 4]| -> Result<u64> {
            let s = emu.alloc(16)?;
            emu.p.memory.write(s, &[16, 2, 0, 0, ip[0], ip[1], ip[2], ip[3], 0, 0, 0, 0, 0, 0, 0, 0])?;
            Ok(s)
        };
        let (sa, sm) = (sockaddr(emu, a)?, sockaddr(emu, mask)?);
        emu.write_u64(node, next)?;
        emu.write_u64(node + 8, n)?;
        emu.p.memory.write_u32(node + 16, flags)?;
        emu.write_u64(node + 24, sa)?;
        emu.write_u64(node + 32, sm)?;
        emu.write_u64(node + 40, 0)?;
        emu.write_u64(node + 48, 0)?;
        next = node;
    }
    emu.write_u64(out, next)?;
    ret(emu, 0)
}

/// `getnameinfo(sa, salen, host, hostlen, serv, servlen, flags)`: the
/// address as numbers (pomme resolves no names: NI_NUMERICHOST's answer either way).
fn getnameinfo(emu: &mut Emu) -> Result<()> {
    const EAI_FAMILY: u64 = 5;
    let (sa, host, hostlen) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let b = emu.p.memory.read(sa, 28)?.to_vec();
    let text = match b[1] {
        2 => format!("{}.{}.{}.{}", b[4], b[5], b[6], b[7]),
        30 => {
            let a: [u8; 16] = b[8..24].try_into().unwrap();
            std::net::Ipv6Addr::from(a).to_string()
        }
        _ => return ret(emu, EAI_FAMILY),
    };
    if host != 0 && hostlen > 0 {
        let mut bytes = text.into_bytes();
        bytes.truncate(hostlen as usize - 1);
        bytes.push(0);
        emu.p.memory.write(host, &bytes)?;
    }
    ret(emu, 0)
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_getifaddrs", getifaddrs),
    // The list stays: it's small and made by the app's request.
    ("_freeifaddrs", |_| Ok(())),
    ("_getnameinfo", getnameinfo),
    ("_nw_path_monitor_create", |e| {
        let m = new_object(e, "OS_nw_path_monitor")?;
        monitor(e, m);
        ret(e, m)
    }),
    ("_nw_path_monitor_create_with_type", |e| {
        let t = e.arg(0);
        let m = new_object(e, "OS_nw_path_monitor")?;
        monitor(e, m).required = Some(t);
        ret(e, m)
    }),
    ("_nw_path_monitor_prohibit_interface_type", |e| {
        let (m, t) = (e.arg(0), e.arg(1));
        monitor(e, m).prohibited.push(t);
        Ok(())
    }),
    ("_nw_path_monitor_set_queue", |e| {
        let (m, q) = (e.arg(0), e.arg(1));
        monitor(e, m).queue = q;
        Ok(())
    }),
    ("_nw_path_monitor_set_update_handler", |e| {
        let (m, b) = (e.arg(0), e.arg(1));
        let b = if b == 0 { 0 } else { crate::libc::copy_block(e, b)? };
        monitor(e, m).handler = b;
        Ok(())
    }),
    ("_nw_path_monitor_set_cancel_handler", |e| {
        let (m, b) = (e.arg(0), e.arg(1));
        let b = if b == 0 { 0 } else { crate::libc::copy_block(e, b)? };
        monitor(e, m).cancel_handler = b;
        Ok(())
    }),
    // Starting: the current path arrives on the queue.
    ("_nw_path_monitor_start", |e| {
        let m = e.arg(0);
        monitor(e, m).started = true;
        update(e, m)
    }),
    ("_nw_path_monitor_cancel", |e| {
        let m = e.arg(0);
        let mon = monitor(e, m);
        if mon.cancelled {
            return Ok(());
        }
        mon.cancelled = true;
        let (q, h) = (mon.queue, mon.cancel_handler);
        if h != 0 {
            let invoke = e.read_u64(h + 16)?;
            crate::libsystem::dispatch_on(e, q, invoke, h)?;
        }
        Ok(())
    }),
    ("_nw_path_get_status", |e| {
        let s = path(e, e.arg(0)).map_or(UNSATISFIED, |p| if p.satisfied { SATISFIED } else { UNSATISFIED });
        ret(e, s)
    }),
    ("_nw_path_get_unsatisfied_reason", |e| {
        let r = path(e, e.arg(0)).map_or(REASON_NOT_AVAILABLE, |p| if p.satisfied { REASON_NONE } else { REASON_NOT_AVAILABLE });
        ret(e, r)
    }),
    ("_nw_path_uses_interface_type", |e| {
        let t = e.arg(1);
        let u = path(e, e.arg(0)).is_some_and(|p| p.interfaces.iter().any(|i| i.kind == t));
        ret(e, u as u64)
    }),
    ("_nw_path_is_expensive", |e| {
        let x = path(e, e.arg(0)).is_some_and(|p| p.expensive);
        ret(e, x as u64)
    }),
    // Low Data Mode is off.
    ("_nw_path_is_constrained", |e| ret(e, 0)),
    ("_nw_path_has_ipv4", |e| {
        let x = path(e, e.arg(0)).is_some_and(|p| p.satisfied);
        ret(e, x as u64)
    }),
    ("_nw_path_has_ipv6", |e| {
        let x = path(e, e.arg(0)).is_some_and(|p| p.satisfied);
        ret(e, x as u64)
    }),
    ("_nw_path_has_dns", |e| {
        let x = path(e, e.arg(0)).is_some_and(|p| p.satisfied);
        ret(e, x as u64)
    }),
    ("_nw_path_is_equal", |e| {
        let (a, b) = (path(e, e.arg(0)).cloned(), path(e, e.arg(1)).cloned());
        let same = match (a, b) {
            (Some(a), Some(b)) => a.satisfied == b.satisfied && a.expensive == b.expensive && a.interfaces.iter().map(|i| i.index).eq(b.interfaces.iter().map(|i| i.index)),
            _ => false,
        };
        ret(e, same as u64)
    }),
    // (path, block(interface) → bool to go on)
    ("_nw_path_enumerate_interfaces", |e| {
        let (p, block) = (e.arg(0), e.arg(1));
        let list = path(e, p).map(|p| p.interfaces.clone()).unwrap_or_default();
        for i in list {
            let o = new_interface(e, i)?;
            if crate::libsystem::call_block(e, block, &[o])? & 0xff == 0 {
                break;
            }
        }
        Ok(())
    }),
    ("_nw_interface_get_type", |e| {
        let t = e.network.interfaces.get(&e.arg(0)).map_or(OTHER, |i| i.kind);
        ret(e, t)
    }),
    ("_nw_interface_get_name", |e| {
        let name = e.network.interfaces.get(&e.arg(0)).map(|i| i.name);
        let p = match name {
            Some(n) => e.alloc_cstr(n)?,
            None => 0,
        };
        ret(e, p)
    }),
    ("_nw_interface_get_index", |e| {
        let i = e.network.interfaces.get(&e.arg(0)).map_or(0, |i| i.index);
        ret(e, i)
    }),
    ("_nw_retain", |e| {
        let o = e.arg(0);
        if o != 0 {
            objc::send(e, o, "retain", &[])?;
        }
        ret(e, o)
    }),
    ("_nw_release", |e| {
        let o = e.arg(0);
        if o != 0 {
            objc::send(e, o, "release", &[])?;
        }
        Ok(())
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_their_interfaces() {
        assert_eq!("cellular".parse::<Path>(), Ok(Path::Cellular));
        assert!("bluetooth".parse::<Path>().is_err());
        assert_eq!(interfaces(Path::Wifi).0[0].name, "en0");
        assert!(interfaces(Path::Offline).0.is_empty() && interfaces(Path::Offline).1.is_none());
    }
}

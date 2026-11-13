//! SystemConfiguration's SCNetworkReachability, over the network pomme models
//! ([`crate::network`]): reachable over Wi-Fi or wire, reachable and WWAN on
//! cellular, unreachable offline. Every target answers for the phone's one
//! path (a host name or an address is reachable when the phone is online).
//!
//! As in SystemConfiguration, a target is a CoreFoundation object of its own
//! type (so CFRetain, CFRelease and CFGetTypeID work on it). Scheduled on a
//! run loop or a dispatch queue, its callback hears every change of flags;
//! a host-name target also hears the first answer once its name "resolves".

use crate::foundation::cf_call;
use crate::network::Path;
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

// SCNetworkReachabilityFlags
const REACHABLE: u32 = 1 << 1;
const IS_WWAN: u32 = 1 << 18;

#[derive(Clone, Default)]
struct Target {
    /// Made from a host name (its first answer comes when scheduled).
    by_name: bool,
    callout: u64,
    /// The context's `info`, handed back to the callout.
    info: u64,
    /// Where callbacks run: the main run loop, or a dispatch queue.
    on_run_loop: bool,
    queue: u64,
    /// The flags the callout last heard.
    last: Option<u32>,
}

#[derive(Default)]
pub struct State {
    /// The CF type ID of SCNetworkReachability, once registered.
    type_id: u64,
    targets: HashMap<u64, Target>,
    /// Callbacks waiting to run: (target, flags).
    pending: Vec<Option<(u64, u32)>>,
}

/// The flags for the phone's current path.
fn flags(emu: &Emu) -> u32 {
    match emu.network.path {
        Path::Wifi | Path::Wired => REACHABLE,
        Path::Cellular => REACHABLE | IS_WWAN,
        Path::Offline => 0,
    }
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// A new target: an instance of the SCNetworkReachability CF type.
fn create(emu: &mut Emu, by_name: bool) -> Result<u64> {
    if emu.reachability.type_id == 0 {
        // CFRuntimeClass, version 0: version, className, then seven callbacks (none).
        let class = emu.alloc(9 * 8)?;
        let name = emu.alloc_cstr("SCNetworkReachability")?;
        emu.write_u64(class + 8, name)?;
        emu.reachability.type_id = cf_call(emu, "__CFRuntimeRegisterClass", &[class])?;
    }
    let id = emu.reachability.type_id;
    let t = cf_call(emu, "__CFRuntimeCreateInstance", &[0, id, 0, 0])?;
    emu.reachability.targets.insert(t, Target { by_name, ..Default::default() });
    Ok(t)
}

/// Queue the callout for `t` with `flags`, where it's scheduled.
fn notify(emu: &mut Emu, t: u64, flags: u32) -> Result<()> {
    let Some(target) = emu.reachability.targets.get_mut(&t) else { return Ok(()) };
    if target.callout == 0 || (!target.on_run_loop && target.queue == 0) || target.last == Some(flags) {
        return Ok(());
    }
    target.last = Some(flags);
    let (on_run_loop, queue) = (target.on_run_loop, target.queue);
    let i = emu.reachability.pending.len();
    emu.reachability.pending.push(Some((t, flags)));
    let f = emu.host_thunk(run_callout)?;
    if on_run_loop {
        crate::libsystem::run_on_main(emu, f, i as u64);
        Ok(())
    } else {
        crate::libsystem::dispatch_on(emu, queue, f, i as u64)
    }
}

/// On the target's run loop or queue: `callout(target, flags, info)`.
fn run_callout(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(0) as usize;
    let Some((t, flags)) = emu.reachability.pending.get_mut(i).and_then(Option::take) else { return Ok(()) };
    let Some(target) = emu.reachability.targets.get(&t).cloned() else { return Ok(()) };
    if target.callout != 0 && (target.on_run_loop || target.queue != 0) {
        emu.call(target.callout, &[t, flags as u64, target.info])?;
    }
    Ok(())
}

/// The network changed: every scheduled target hears its new flags.
pub fn path_changed(emu: &mut Emu) -> Result<()> {
    let f = flags(emu);
    let targets: Vec<u64> = emu.reachability.targets.keys().copied().collect();
    for t in targets {
        notify(emu, t, f)?;
    }
    Ok(())
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_SCNetworkReachabilityCreateWithName", |e| {
        let t = create(e, true)?;
        ret(e, t)
    }),
    ("_SCNetworkReachabilityCreateWithAddress", |e| {
        let t = create(e, false)?;
        ret(e, t)
    }),
    ("_SCNetworkReachabilityCreateWithAddressPair", |e| {
        let t = create(e, false)?;
        ret(e, t)
    }),
    ("_SCNetworkReachabilityGetTypeID", |e| {
        if e.reachability.type_id == 0 {
            let t = create(e, false)?;
            e.reachability.targets.remove(&t);
        }
        let id = e.reachability.type_id;
        ret(e, id)
    }),
    ("_SCNetworkReachabilityGetFlags", |e| {
        let (t, out) = (e.arg(0), e.arg(1));
        if !e.reachability.targets.contains_key(&t) || out == 0 {
            return ret(e, 0);
        }
        let f = flags(e);
        e.p.memory.write_u32(out, f)?;
        ret(e, 1)
    }),
    // (target, callout, SCNetworkReachabilityContext *{version, info, retain, release, copyDescription})
    ("_SCNetworkReachabilitySetCallback", |e| {
        let (t, callout, context) = (e.arg(0), e.arg(1), e.arg(2));
        let info = if context == 0 { 0 } else { e.read_u64(context + 8)? };
        let Some(target) = e.reachability.targets.get_mut(&t) else { return ret(e, 0) };
        target.callout = callout;
        target.info = info;
        ret(e, 1)
    }),
    ("_SCNetworkReachabilityScheduleWithRunLoop", |e| {
        let t = e.arg(0);
        let f = flags(e);
        let Some(target) = e.reachability.targets.get_mut(&t) else { return ret(e, 0) };
        target.on_run_loop = true;
        if !target.by_name {
            target.last = Some(f);
        }
        scheduled_name(e, t)?;
        ret(e, 1)
    }),
    ("_SCNetworkReachabilityUnscheduleFromRunLoop", |e| {
        let t = e.arg(0);
        if let Some(target) = e.reachability.targets.get_mut(&t) {
            target.on_run_loop = false;
        }
        ret(e, 1)
    }),
    // A queue schedules it there; NULL unschedules it.
    ("_SCNetworkReachabilitySetDispatchQueue", |e| {
        let (t, q) = (e.arg(0), e.arg(1));
        let f = flags(e);
        let Some(target) = e.reachability.targets.get_mut(&t) else { return ret(e, 0) };
        target.queue = q;
        if q != 0 {
            if !target.by_name {
                target.last = Some(f);
            }
            scheduled_name(e, t)?;
        }
        ret(e, 1)
    }),
];

/// A host-name target hears its first answer once it's scheduled.
fn scheduled_name(emu: &mut Emu, t: u64) -> Result<()> {
    if emu.reachability.targets.get(&t).is_some_and(|x| x.by_name) {
        let f = flags(emu);
        notify(emu, t, f)?;
    }
    Ok(())
}

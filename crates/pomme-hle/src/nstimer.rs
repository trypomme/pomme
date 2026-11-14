//! NSTimer, toll-free bridged to CoreFoundation's CFRunLoopTimer (as
//! `__NSCFTimer`): the run loop does the scheduling, pomme keeps the
//! target/selector or block and calls it from the timer's callout.

use crate::foundation::{self, cluster_alloc};
use crate::objc::{self, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

#[derive(Clone, Copy, Default)]
struct Fire {
    target: u64,
    selector: u64,
    block: u64,
    user_info: u64,
}

#[derive(Clone, Default)]
pub struct State {
    timers: HashMap<u64, Fire>,
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// The CFRunLoopTimer callout `(timer, info)`.
fn callout(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    let Some(f) = emu.timers.timers.get(&t).copied() else { return Ok(()) };
    if f.block != 0 {
        crate::libsystem::call_block(emu, f.block, &[t])?;
    } else if f.target != 0 {
        let name = objc::sel_name(emu, f.selector);
        send(emu, f.target, &name, &[t])?;
    }
    Ok(())
}

fn copy_block(emu: &mut Emu, b: u64) -> Result<u64> {
    let saved = emu.cpu.x[0];
    emu.cpu.x[0] = b;
    crate::libc::block_copy(emu)?;
    let c = emu.cpu.x[0];
    emu.cpu.x[0] = saved;
    Ok(c)
}

/// A CFRunLoopTimer that fires at `fire_abs` (CFAbsoluteTime), every `interval` if repeating.
fn create(emu: &mut Emu, fire_abs: f64, interval: f64, repeats: bool, mut f: Fire) -> Result<u64> {
    let interval = if repeats { interval.max(0.0001) } else { 0.0 };
    let co = emu.host_thunk(callout)?;
    emu.cpu.q[0] = fire_abs.to_bits() as u128;
    emu.cpu.q[1] = interval.to_bits() as u128;
    // CFRunLoopTimerCreate(allocator, fireDate (d0), interval (d1), flags, order, callout, context)
    let t = foundation::cf_call(emu, "_CFRunLoopTimerCreate", &[0, 0, 0, co, 0])?;
    if f.target != 0 {
        send(emu, f.target, "retain", &[])?;
    }
    if f.user_info != 0 {
        send(emu, f.user_info, "retain", &[])?;
    }
    if f.block != 0 {
        f.block = copy_block(emu, f.block)?;
    }
    emu.timers.timers.insert(t, f);
    Ok(t)
}

fn now_abs(emu: &mut Emu) -> Result<f64> {
    foundation::cf_call(emu, "_CFAbsoluteTimeGetCurrent", &[])?;
    Ok(f64::from_bits(emu.last_q0 as u64))
}

fn schedule(emu: &mut Emu, t: u64) -> Result<()> {
    let rl = foundation::cf_call(emu, "_CFRunLoopGetCurrent", &[])?;
    let mode = foundation::cf_data_pub(emu, "_kCFRunLoopDefaultMode")?;
    foundation::cf_call(emu, "_CFRunLoopAddTimer", &[rl, t, mode])?;
    Ok(())
}

/// `+timerWithTimeInterval:target:selector:userInfo:repeats:` (and the scheduled variant).
fn with_target(emu: &mut Emu, scheduled: bool) -> Result<()> {
    let interval = d(emu, 0);
    let f = Fire { target: emu.arg(2), selector: emu.arg(3), user_info: emu.arg(4), block: 0 };
    let repeats = emu.arg(5) & 0xff != 0;
    let fire = now_abs(emu)? + interval;
    let t = create(emu, fire, interval, repeats, f)?;
    if scheduled {
        schedule(emu, t)?;
    }
    ret(emu, t)
}

/// `+timerWithTimeInterval:repeats:block:` (and the scheduled variant).
fn with_block(emu: &mut Emu, scheduled: bool) -> Result<()> {
    let interval = d(emu, 0);
    let (repeats, block) = (emu.arg(2) & 0xff != 0, emu.arg(3));
    let fire = now_abs(emu)? + interval;
    let t = create(emu, fire, interval, repeats, Fire { block, ..Default::default() })?;
    if scheduled {
        schedule(emu, t)?;
    }
    ret(emu, t)
}

/// `-initWithFireDate:interval:target:selector:userInfo:repeats:`
fn init_with_target(emu: &mut Emu) -> Result<()> {
    let (date, interval) = (emu.arg(2), d(emu, 0));
    let f = Fire { target: emu.arg(3), selector: emu.arg(4), user_info: emu.arg(5), block: 0 };
    let repeats = emu.arg(6) & 0xff != 0;
    let fire = foundation::date_value(emu, date)?;
    let t = create(emu, fire, interval, repeats, f)?;
    ret(emu, t)
}

/// `-initWithFireDate:interval:repeats:block:`
fn init_with_block(emu: &mut Emu) -> Result<()> {
    let (date, interval) = (emu.arg(2), d(emu, 0));
    let (repeats, block) = (emu.arg(3) & 0xff != 0, emu.arg(4));
    let fire = foundation::date_value(emu, date)?;
    let t = create(emu, fire, interval, repeats, Fire { block, ..Default::default() })?;
    ret(emu, t)
}

fn invalidate(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    foundation::cf_call(emu, "_CFRunLoopTimerInvalidate", &[t])?;
    Ok(())
}

fn fire_now(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    let repeating = foundation::cf_call(emu, "_CFRunLoopTimerDoesRepeat", &[t])? & 0xff != 0;
    emu.cpu.x[0] = t;
    callout(emu)?;
    if !repeating {
        foundation::cf_call(emu, "_CFRunLoopTimerInvalidate", &[t])?;
    }
    Ok(())
}

fn fire_date(emu: &mut Emu) -> Result<()> {
    let t = emu.arg(0);
    foundation::cf_call(emu, "_CFRunLoopTimerGetNextFireDate", &[t])?;
    let abs = f64::from_bits(emu.last_q0 as u64);
    let date = foundation::new_date(emu, abs)?;
    ret(emu, date)
}

fn set_fire_date(emu: &mut Emu) -> Result<()> {
    let (t, date) = (emu.arg(0), emu.arg(2));
    let abs = foundation::date_value(emu, date)?;
    emu.cpu.q[0] = abs.to_bits() as u128;
    foundation::cf_call(emu, "_CFRunLoopTimerSetNextFireDate", &[t])?;
    Ok(())
}

const T: &str = "NSTimer";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (T, true, "alloc", cluster_alloc),
    (T, true, "allocWithZone:", cluster_alloc),
    (T, true, "timerWithTimeInterval:target:selector:userInfo:repeats:", |e| with_target(e, false)),
    (T, true, "scheduledTimerWithTimeInterval:target:selector:userInfo:repeats:", |e| with_target(e, true)),
    (T, true, "timerWithTimeInterval:repeats:block:", |e| with_block(e, false)),
    (T, true, "scheduledTimerWithTimeInterval:repeats:block:", |e| with_block(e, true)),
    (T, false, "initWithFireDate:interval:target:selector:userInfo:repeats:", init_with_target),
    (T, false, "initWithFireDate:interval:repeats:block:", init_with_block),
    (T, false, "invalidate", invalidate),
    (T, false, "isValid", |e| {
        let v = foundation::cf_call(e, "_CFRunLoopTimerIsValid", &[e.arg(0)])? & 0xff;
        ret(e, v)
    }),
    (T, false, "fire", fire_now),
    (T, false, "fireDate", fire_date),
    (T, false, "setFireDate:", set_fire_date),
    (T, false, "timeInterval", |e| {
        foundation::cf_call(e, "_CFRunLoopTimerGetInterval", &[e.arg(0)])?;
        e.cpu.q[0] = e.last_q0;
        Ok(())
    }),
    (T, false, "userInfo", |e| {
        let u = e.timers.timers.get(&e.arg(0)).map_or(0, |f| f.user_info);
        ret(e, u)
    }),
    (T, false, "tolerance", |e| {
        crate::uikit::ret_f64(e, 0.0);
        Ok(())
    }),
    (T, false, "setTolerance:", |_| Ok(())),
];

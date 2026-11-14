//! Dispatch sources: timers and custom data sources, as libdispatch runs them.
//!
//! - A source starts inactive: nothing fires until `dispatch_resume` or
//!   `dispatch_activate`. `dispatch_suspend` pauses it again; events that
//!   arrive meanwhile are kept.
//! - Its event handler runs on the source's queue, never more than one at a
//!   time. Events that come while one is waiting or running are coalesced:
//!   `dispatch_source_get_data` in the handler says how many timer fires, or
//!   the merged `dispatch_source_merge_data` values (added, or'ed, or the last).
//! - A timer fires at its start time, then at each interval on the virtual
//!   clock (the main run loop's timers carry it).
//! - `dispatch_source_cancel` stops the events; the cancel handler runs on the
//!   queue once no event handler is waiting or running.
//!
//! Sources on descriptors, signals, processes and Mach ports aren't modelled:
//! creating one stops with a message rather than never firing.

use crate::{Emu, HostFn, Result, Stop};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Timer,
    DataAdd,
    DataOr,
    DataReplace,
}

/// A handler: a function and its argument (a block's invoke and the block, or
/// a function and the source's context).
#[derive(Clone, Copy, Default)]
struct Handler {
    f: u64,
    ctx: u64,
    /// A `_f` handler, called with the context current when it runs.
    uses_context: bool,
}

#[derive(Clone)]
struct Source {
    kind: Kind,
    handle: u64,
    mask: u64,
    queue: u64,
    event: Option<Handler>,
    cancel: Option<Handler>,
    registration: Option<Handler>,
    /// Suspension count: sources start at 1 (inactive).
    suspended: u32,
    activated: bool,
    cancelled: bool,
    cancel_submitted: bool,
    /// Events not yet handed to the handler, and what the running handler got.
    pending: u64,
    current: u64,
    /// An event handler is waiting on the queue or running.
    handler_queued: bool,
    /// Timer: next fire (ns), interval (ns, u64::MAX for once), and a generation
    /// so a re-set timer's earlier run-loop timer does nothing.
    next: Option<u64>,
    interval: u64,
    generation: u64,
}

#[derive(Clone, Default)]
pub(crate) struct State {
    sources: HashMap<u64, Source>,
    /// dispatch_set_context values, by object.
    contexts: HashMap<u64, u64>,
}

/// Whether `obj` is a dispatch source.
pub(crate) fn is_source(emu: &Emu, obj: u64) -> bool {
    emu.dsource.sources.contains_key(&obj)
}

fn source(emu: &mut Emu, s: u64) -> Option<&mut Source> {
    emu.dsource.sources.get_mut(&s)
}

/// `dispatch_source_create(type, handle, mask, queue)`
fn create(emu: &mut Emu) -> Result<()> {
    let (ty, handle, mask, queue) = (emu.arg(0), emu.arg(1), emu.arg(2), emu.arg(3));
    let named = |emu: &Emu, n: &str| crate::libsystem::data_addr(emu, n) == Some(ty);
    let kind = if named(emu, "__dispatch_source_type_timer") {
        Kind::Timer
    } else if named(emu, "__dispatch_source_type_data_add") {
        Kind::DataAdd
    } else if named(emu, "__dispatch_source_type_data_or") {
        Kind::DataOr
    } else if named(emu, "__dispatch_source_type_data_replace") {
        Kind::DataReplace
    } else {
        let name = crate::libsystem::data_name(emu, ty).unwrap_or("an unknown type");
        return Err(Stop::Unsupported(format!("dispatch sources of {name} aren't implemented yet (only timers and data sources)")));
    };
    let queue = if queue == 0 { crate::libsystem::global_queue(emu)? } else { queue };
    let cls = crate::objc::host_class(emu, "OS_dispatch_source")?;
    let s = emu.alloc(64)?;
    emu.write_u64(s, cls)?;
    emu.dsource.sources.insert(
        s,
        Source {
            kind,
            handle,
            mask,
            queue,
            event: None,
            cancel: None,
            registration: None,
            suspended: 1,
            activated: false,
            cancelled: false,
            cancel_submitted: false,
            pending: 0,
            current: 0,
            handler_queued: false,
            next: None,
            interval: u64::MAX,
            generation: 0,
        },
    );
    emu.ret(s);
    Ok(())
}

fn block_handler(emu: &mut Emu, block: u64) -> Result<Option<Handler>> {
    if block == 0 {
        return Ok(None);
    }
    emu.cpu.x[0] = block;
    crate::libc::block_copy(emu)?;
    let b = emu.cpu.x[0];
    Ok(Some(Handler { f: emu.read_u64(b + 16)?, ctx: b, uses_context: false }))
}

fn fn_handler(f: u64) -> Option<Handler> {
    (f != 0).then_some(Handler { f, ctx: 0, uses_context: true })
}

enum Which {
    Event,
    Cancel,
    Registration,
}

fn set_handler(emu: &mut Emu, which: Which, function: bool) -> Result<()> {
    let (s, h) = (emu.arg(0), emu.arg(1));
    let h = if function { fn_handler(h) } else { block_handler(emu, h)? };
    if let Some(src) = source(emu, s) {
        match which {
            Which::Event => src.event = h,
            Which::Cancel => src.cancel = h,
            Which::Registration => src.registration = h,
        }
    }
    Ok(())
}

/// Hand `h` to the source's queue: the thunk runs it with the right argument.
fn submit_handler(emu: &mut Emu, s: u64, f: HostFn) -> Result<()> {
    let Some(queue) = source(emu, s).map(|x| x.queue) else { return Ok(()) };
    let t = emu.host_thunk(f)?;
    crate::libsystem::dispatch_on(emu, queue, t, s)
}

fn call(emu: &mut Emu, s: u64, h: Handler) -> Result<()> {
    let ctx = if h.uses_context { emu.dsource.contexts.get(&s).copied().unwrap_or(0) } else { h.ctx };
    emu.call(h.f, &[ctx])?;
    Ok(())
}

/// Events arrived: queue the handler unless one is already waiting or running.
fn signal(emu: &mut Emu, s: u64) -> Result<()> {
    let Some(src) = source(emu, s) else { return Ok(()) };
    if src.cancelled || src.suspended > 0 || src.handler_queued || src.pending == 0 || src.event.is_none() {
        return Ok(());
    }
    src.handler_queued = true;
    submit_handler(emu, s, run_event)
}

/// On the queue: the event handler, with the coalesced events as its data.
fn run_event(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let Some(src) = source(emu, s) else { return Ok(()) };
    let h = src.event.filter(|_| !src.cancelled);
    src.current = std::mem::take(&mut src.pending);
    if let Some(h) = h {
        call(emu, s, h)?;
    }
    let Some(src) = source(emu, s) else { return Ok(()) };
    src.handler_queued = false;
    if src.cancelled {
        return cancel_if_idle(emu, s);
    }
    // More came while it ran.
    signal(emu, s)
}

fn run_cancel(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    if let Some(h) = source(emu, s).and_then(|x| x.cancel) {
        call(emu, s, h)?;
    }
    Ok(())
}

fn run_registration(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    if let Some(h) = source(emu, s).and_then(|x| x.registration.take()) {
        call(emu, s, h)?;
    }
    Ok(())
}

/// Cancelled and no handler waiting or running: the cancel handler's turn.
fn cancel_if_idle(emu: &mut Emu, s: u64) -> Result<()> {
    let Some(src) = source(emu, s) else { return Ok(()) };
    if !src.cancelled || src.handler_queued || src.cancel_submitted || src.suspended > 0 {
        return Ok(());
    }
    src.cancel_submitted = true;
    if src.cancel.is_some() {
        submit_handler(emu, s, run_cancel)?;
    }
    Ok(())
}

// ---- timers ----

/// `dispatch_source_set_timer(source, start, interval, leeway)`
fn set_timer(emu: &mut Emu) -> Result<()> {
    let (s, start, interval) = (emu.arg(0), emu.arg(1), emu.arg(2));
    let now = crate::threads::now_ns(emu);
    let Some(src) = source(emu, s) else { return Ok(()) };
    src.generation += 1;
    // DISPATCH_TIME_NOW is 0; DISPATCH_TIME_FOREVER never fires.
    src.next = match start {
        u64::MAX => None,
        0 => Some(now),
        t => Some(t),
    };
    src.interval = if interval == 0 { 1 } else { interval };
    src.pending = 0;
    arm(emu, s)
}

/// Put the timer's next fire on the clock, when it's running.
fn arm(emu: &mut Emu, s: u64) -> Result<()> {
    let now = crate::threads::now_ns(emu);
    let Some(src) = source(emu, s) else { return Ok(()) };
    if src.kind != Kind::Timer || src.cancelled || src.suspended > 0 {
        return Ok(());
    }
    let Some(next) = src.next else { return Ok(()) };
    let key = emu.dsource.timer_key(s);
    let delay = next.saturating_sub(now) as f64 / 1e9;
    crate::uikit::after_on_main(emu, delay, fire, key)
}

impl State {
    /// A run-loop timer's argument: the source, and its generation in the top bits.
    fn timer_key(&self, s: u64) -> u64 {
        let g = self.sources.get(&s).map_or(0, |x| x.generation);
        (g << 48) | (s & 0xffff_ffff_ffff)
    }
}

/// The run-loop timer for a source's fire: count the fires due, wake the handler, re-arm.
fn fire(emu: &mut Emu) -> Result<()> {
    let key = emu.arg(1);
    let (s, generation) = (key & 0xffff_ffff_ffff, key >> 48);
    let now = crate::threads::now_ns(emu);
    let Some(src) = source(emu, s) else { return Ok(()) };
    if src.generation != generation || src.cancelled || src.suspended > 0 {
        return Ok(());
    }
    let Some(next) = src.next else { return Ok(()) };
    if now < next {
        return arm(emu, s);
    }
    let fires = if src.interval == u64::MAX { 1 } else { 1 + (now - next) / src.interval };
    src.pending += fires;
    src.next = if src.interval == u64::MAX { None } else { Some(next + fires * src.interval) };
    signal(emu, s)?;
    arm(emu, s)
}

// ---- data, state, lifecycle ----

/// `dispatch_source_merge_data(source, value)`
fn merge_data(emu: &mut Emu) -> Result<()> {
    let (s, v) = (emu.arg(0), emu.arg(1));
    let Some(src) = source(emu, s) else { return Ok(()) };
    src.pending = match src.kind {
        Kind::DataAdd => src.pending.wrapping_add(v),
        Kind::DataOr => src.pending | v,
        Kind::DataReplace => v,
        Kind::Timer => return Ok(()),
    };
    signal(emu, s)
}

fn get_data(emu: &mut Emu) -> Result<()> {
    let v = source(emu, emu.arg(0)).map_or(0, |x| x.current);
    emu.ret(v);
    Ok(())
}

/// `dispatch_source_cancel(source)`
fn cancel(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let Some(src) = source(emu, s) else { return Ok(()) };
    if src.cancelled {
        return Ok(());
    }
    src.cancelled = true;
    src.next = None;
    cancel_if_idle(emu, s)
}

/// `dispatch_resume(object)` for a source: one suspension fewer; at zero it runs.
pub(crate) fn resume(emu: &mut Emu, s: u64) -> Result<()> {
    let Some(src) = source(emu, s) else { return Ok(()) };
    if src.suspended == 0 {
        return Err(Stop::Unsupported("BUG IN CLIENT OF LIBDISPATCH: Over-resume of an object".into()));
    }
    src.suspended -= 1;
    if src.suspended > 0 {
        return Ok(());
    }
    let first = !src.activated;
    src.activated = true;
    if first && src.registration.is_some() {
        submit_handler(emu, s, run_registration)?;
    }
    arm(emu, s)?;
    signal(emu, s)?;
    cancel_if_idle(emu, s)
}

/// `dispatch_activate(object)` for a source: its first resume (later calls do nothing).
pub(crate) fn activate(emu: &mut Emu, s: u64) -> Result<()> {
    if source(emu, s).is_some_and(|x| !x.activated) {
        return resume(emu, s);
    }
    Ok(())
}

/// `dispatch_suspend(object)` for a source.
pub(crate) fn suspend(emu: &mut Emu, s: u64) -> Result<()> {
    if let Some(src) = source(emu, s) {
        src.suspended += 1;
        // A timer's pending run-loop timer will find it suspended; resuming re-arms it.
        src.generation += 1;
    }
    Ok(())
}

/// `dispatch_set_context(object, context)` / `dispatch_get_context(object)`.
pub(crate) fn set_context(emu: &mut Emu, obj: u64, ctx: u64) {
    emu.dsource.contexts.insert(obj, ctx);
}

pub(crate) fn context(emu: &Emu, obj: u64) -> u64 {
    emu.dsource.contexts.get(&obj).copied().unwrap_or(0)
}

pub(crate) const FUNCS: &[(&str, HostFn)] = &[
    ("_dispatch_source_create", create),
    ("_dispatch_source_set_event_handler", |e| set_handler(e, Which::Event, false)),
    ("_dispatch_source_set_event_handler_f", |e| set_handler(e, Which::Event, true)),
    ("_dispatch_source_set_cancel_handler", |e| set_handler(e, Which::Cancel, false)),
    ("_dispatch_source_set_cancel_handler_f", |e| set_handler(e, Which::Cancel, true)),
    ("_dispatch_source_set_registration_handler", |e| set_handler(e, Which::Registration, false)),
    ("_dispatch_source_set_registration_handler_f", |e| set_handler(e, Which::Registration, true)),
    ("_dispatch_source_set_timer", set_timer),
    ("_dispatch_source_merge_data", merge_data),
    ("_dispatch_source_get_data", get_data),
    ("_dispatch_source_get_handle", |e| {
        let v = source(e, e.arg(0)).map_or(0, |x| x.handle);
        e.ret(v);
        Ok(())
    }),
    ("_dispatch_source_get_mask", |e| {
        let v = source(e, e.arg(0)).map_or(0, |x| x.mask);
        e.ret(v);
        Ok(())
    }),
    ("_dispatch_source_cancel", cancel),
    ("_dispatch_source_testcancel", |e| {
        let v = source(e, e.arg(0)).is_some_and(|x| x.cancelled);
        e.ret(v as u64);
        Ok(())
    }),
];

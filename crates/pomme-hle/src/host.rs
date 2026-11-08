//! Live hosts: something outside the emulator (a browser worker, a window)
//! that shows frames and feeds input while the app runs.
//!
//! The emulator calls [`Host::idle`] whenever every guest thread is waiting:
//! that's when the host paces virtual time against the wall clock and hands
//! over any input that arrived. Input goes in through a CFRunLoop source on
//! the main run loop, as UIKit's event source does, so touches are delivered
//! on the main thread between run-loop iterations. Frames are rendered at the
//! layout pass before the run loop sleeps (CoreAnimation's commit point).
//!
//! A run without a host (tests, `pomme run`) stays fully deterministic; a
//! live run is deterministic given the input and the times it arrived.

use crate::touch::Input;
use crate::{Emu, Result, Stop, foundation};

/// What the host wants after an idle call.
pub enum Idle {
    /// Nothing arrived: let virtual time jump to the next deadline.
    Advance,
    /// Input arrived at this virtual time (ns): deliver it now.
    Input { at_ns: u64, events: Vec<Input> },
    /// Stop the run.
    Quit,
}

pub trait Host {
    /// Every guest thread waits. `now_ns` is the virtual time, `next_ns` the
    /// earliest deadline (none: wait for input). Block until it's due on the
    /// wall clock or input arrives.
    fn idle(&mut self, now_ns: u64, next_ns: Option<u64>) -> Idle;
    /// A new frame: RGBA8, `width` × `height`, rows top to bottom.
    fn frame(&mut self, rgba: &[u8], width: u32, height: u32);
}

#[derive(Default)]
pub struct State {
    pub host: Option<Box<dyn Host>>,
    /// Pixels per point for frames.
    pub scale: f32,
    /// Virtual time of the last frame (ns).
    last_frame: Option<u64>,
    /// The input run-loop source and the main run loop.
    source: u64,
    run_loop: u64,
    pending: Vec<Input>,
}

const FRAME_NS: u64 = 1_000_000_000 / 30;

/// Called from the scheduler when nothing can run. Ok(true): input was delivered, look again.
pub(crate) fn idle(emu: &mut Emu, next_ns: Option<u64>) -> Result<bool> {
    let Some(mut host) = emu.live.host.take() else { return Ok(false) };
    let now = crate::threads::now_ns(emu);
    let r = host.idle(now, next_ns);
    emu.live.host = Some(host);
    match r {
        Idle::Advance => Ok(false),
        Idle::Quit => Err(Stop::Exit(0)),
        Idle::Input { at_ns, events } => {
            crate::threads::advance_to(emu, at_ns);
            emu.live.pending.extend(events);
            if emu.live.source != 0 {
                foundation::cf_call(emu, "_CFRunLoopSourceSignal", &[emu.live.source])?;
                foundation::cf_call(emu, "_CFRunLoopWakeUp", &[emu.live.run_loop])?;
            }
            Ok(true)
        }
    }
}

/// The source's perform callout: deliver queued touches.
fn perform(emu: &mut Emu) -> Result<()> {
    for inp in std::mem::take(&mut emu.live.pending) {
        crate::touch::send_touch(emu, inp.phase, inp.x, inp.y)?;
    }
    Ok(())
}

/// Set up the input source on the main run loop (when there's a host).
pub(crate) fn install(emu: &mut Emu) -> Result<()> {
    if emu.live.host.is_none() {
        return Ok(());
    }
    if emu.live.scale == 0.0 {
        emu.live.scale = 2.0;
    }
    let callout = emu.host_thunk(perform)?;
    // CFRunLoopSourceContext (version 0): {version, info, retain, release,
    // copyDescription, equal, hash, schedule, cancel, perform}
    let ctx = emu.alloc(80)?;
    emu.write_u64(ctx + 72, callout)?;
    let src = foundation::cf_call(emu, "_CFRunLoopSourceCreate", &[0, 0, ctx])?;
    let rl = foundation::cf_call(emu, "_CFRunLoopGetMain", &[])?;
    let common = foundation::cf_data_pub(emu, "_kCFRunLoopCommonModes")?;
    foundation::cf_call(emu, "_CFRunLoopAddSource", &[rl, src, common])?;
    emu.live.source = src;
    emu.live.run_loop = rl;
    Ok(())
}

/// After the layout pass: render a frame if one is due.
pub(crate) fn maybe_frame(emu: &mut Emu) -> Result<()> {
    if emu.live.host.is_none() {
        return Ok(());
    }
    let now = crate::threads::now_ns(emu);
    if emu.live.last_frame.is_some_and(|t| now < t + FRAME_NS) {
        return Ok(());
    }
    emu.live.last_frame = Some(now);
    let pix = crate::render::render(emu, emu.live.scale)?;
    if let Some(mut host) = emu.live.host.take() {
        host.frame(pix.data(), pix.width(), pix.height());
        emu.live.host = Some(host);
    }
    Ok(())
}

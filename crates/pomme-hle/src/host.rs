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

/// Something the person did.
#[derive(Clone, Debug)]
pub enum Event {
    /// A finger down, moving or up (`Input.at_ns` is ignored: the event's time is the batch's).
    Touch(Input),
    /// A key on a hardware keyboard, typed into the first responder as the
    /// software keyboard would (`'\n'` return, `'\u{8}'` delete).
    Key(char),
    /// The phone turned to be held this way (the interface follows if the app allows).
    Rotate(crate::device::Orientation),
    /// Something from the Simulator's Face ID menu: enrolment, or a face for a waiting prompt.
    Face(crate::keychain::Face),
}

/// A rendered frame: RGBA8, `width` × `height`, rows top to bottom, upright
/// for the interface `orientation` (the window decides how to show the phone around it).
pub struct Frame<'a> {
    pub rgba: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub orientation: crate::device::Orientation,
}

/// What the host wants after an idle call.
pub enum Idle {
    /// Nothing arrived: let virtual time jump to the next deadline.
    Advance,
    /// Input arrived at this virtual time (ns): deliver it now.
    Input { at_ns: u64, events: Vec<Event> },
    /// Stop the run.
    Quit,
}

pub trait Host {
    /// Every guest thread waits. `now_ns` is the virtual time, `next_ns` the
    /// earliest deadline (none: wait for input). Block until it's due on the
    /// wall clock or input arrives.
    fn idle(&mut self, now_ns: u64, next_ns: Option<u64>) -> Idle;
    /// A new frame.
    fn frame(&mut self, frame: Frame<'_>);
    /// A Face ID prompt is up, waiting for a face, with the app's reason.
    fn biometry_prompt(&mut self, _reason: &str) {}
}

#[derive(Default)]
pub struct State {
    pub host: Option<Box<dyn Host>>,
    /// Pixels per point for frames.
    pub scale: f32,
    /// Virtual time of the last frame (ns).
    last_frame: Option<u64>,
    /// A layout pass happened since the last frame (it was too soon to draw).
    frame_pending: bool,
    /// The input run-loop source and the main run loop.
    source: u64,
    run_loop: u64,
    pending: Vec<Event>,
}

const FRAME_NS: u64 = 1_000_000_000 / 30;

/// Called from the scheduler when nothing can run. Ok(true): input was delivered, look again.
pub(crate) fn idle(emu: &mut Emu, next_ns: Option<u64>) -> Result<bool> {
    if emu.live.host.is_none() {
        return Ok(false);
    }
    // Nothing is running: draw whatever the last layout pass left undrawn.
    if emu.live.frame_pending {
        draw(emu)?;
    }
    let Some(mut host) = emu.live.host.take() else { return Ok(false) };
    let now = crate::threads::now_ns(emu);
    let r = host.idle(now, next_ns);
    emu.live.host = Some(host);
    match r {
        Idle::Advance => Ok(false),
        Idle::Quit => Err(Stop::Exit(0)),
        Idle::Input { at_ns, events } => {
            crate::threads::advance_to(emu, at_ns);
            // Face ID is the system's (its own process on iOS), not the app's
            // run loop: a face answers a waiting prompt at once, even one
            // that blocks the main thread.
            let (system, app): (Vec<Event>, Vec<Event>) = events.into_iter().partition(|e| matches!(e, Event::Face(_)));
            for ev in system {
                if let Event::Face(f) = ev {
                    crate::keychain::present(emu, f)?;
                }
            }
            if app.is_empty() {
                return Ok(true);
            }
            emu.live.pending.extend(app);
            if emu.live.source != 0 {
                foundation::cf_call(emu, "_CFRunLoopSourceSignal", &[emu.live.source])?;
                foundation::cf_call(emu, "_CFRunLoopWakeUp", &[emu.live.run_loop])?;
            }
            Ok(true)
        }
    }
}

/// The source's perform callout: deliver what arrived, in order.
fn perform(emu: &mut Emu) -> Result<()> {
    for ev in std::mem::take(&mut emu.live.pending) {
        match ev {
            Event::Touch(inp) => crate::touch::send_touch(emu, inp.phase, inp.x, inp.y)?,
            Event::Key(ch) => crate::textinput::key(emu, ch)?,
            Event::Rotate(o) => crate::orientation::turn_device(emu, o)?,
            Event::Face(f) => crate::keychain::present(emu, f)?,
        }
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

/// Tell the host a biometry prompt is waiting.
pub(crate) fn biometry_prompt(emu: &mut Emu, reason: &str) {
    if let Some(host) = emu.live.host.as_mut() {
        host.biometry_prompt(reason);
    }
}

/// Something drew outside layout (a Metal frame): the next idle moment shows it.
pub(crate) fn frame_changed(emu: &mut Emu) {
    if emu.live.host.is_some() {
        emu.live.frame_pending = true;
    }
}

/// After the layout pass: render a frame if one is due.
pub(crate) fn maybe_frame(emu: &mut Emu) -> Result<()> {
    if emu.live.host.is_none() {
        return Ok(());
    }
    let now = crate::threads::now_ns(emu);
    if emu.live.last_frame.is_some_and(|t| now < t + FRAME_NS) {
        emu.live.frame_pending = true;
        return Ok(());
    }
    draw(emu)
}

fn draw(emu: &mut Emu) -> Result<()> {
    emu.live.last_frame = Some(crate::threads::now_ns(emu));
    emu.live.frame_pending = false;
    let pix = crate::render::render(emu, emu.live.scale)?;
    let orientation = crate::orientation::current(emu);
    if let Some(mut host) = emu.live.host.take() {
        host.frame(Frame { rgba: pix.data(), width: pix.width(), height: pix.height(), orientation });
        emu.live.host = Some(host);
    }
    Ok(())
}

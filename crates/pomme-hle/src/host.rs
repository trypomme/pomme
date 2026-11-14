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
    /// File ▸ Save Screen: the screen at the phone's own resolution comes back in [`Host::screenshot`].
    SaveScreen,
    /// File ▸ Record Screen: start (true) or stop recording. While it records,
    /// every frame also comes at the phone's own resolution in [`Host::recorded`],
    /// and [`Host::recording_stopped`] says when it ends.
    Record(bool),
}

/// A rendered frame: RGBA8, `width` × `height`, rows top to bottom, upright
/// for the interface `orientation` (the window decides how to show the phone around it).
pub struct Frame<'a> {
    pub rgba: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub orientation: crate::device::Orientation,
    /// The virtual time it shows (ns).
    pub at_ns: u64,
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
    /// The waiting Face ID prompts were answered (or failed): the sheet goes.
    fn biometry_answered(&mut self) {}
    /// The wall clock as the app's time (ns), once the host has started
    /// pacing: while the app is busy, its clock keeps up with this, as an
    /// iPhone's animations keep real time however busy the app is.
    fn wall_ns(&mut self) -> Option<u64> {
        None
    }
    /// The screen as it is now, at the phone's own resolution (File ▸ Save Screen).
    fn screenshot(&mut self, _shot: Frame<'_>) {}
    /// A frame of the recording, at the phone's own resolution: the screen as
    /// recording starts, then each frame drawn until it stops.
    fn recorded(&mut self, _frame: Frame<'_>) {}
    /// The recording ended at this (virtual) time: the last frame lasts until then.
    fn recording_stopped(&mut self, _at_ns: u64) {}
}

#[derive(Default)]
pub struct State {
    pub host: Option<Box<dyn Host>>,
    /// Pixels per point for frames.
    pub scale: f32,
    /// Virtual time of the last frame (ns).
    last_frame: Option<u64>,
    /// Something on screen changed since the last frame (CoreAnimation's
    /// uncommitted transaction): the next frame is due.
    dirty: bool,
    /// The input run-loop source and the main run loop.
    source: u64,
    run_loop: u64,
    pending: Vec<Event>,
    /// Recording the screen (File ▸ Record Screen).
    recording: bool,
    /// The minute the status bar's clock showed in the last frame.
    clock_minute: Option<i64>,
}

const FRAME_NS: u64 = 1_000_000_000 / 30;

/// Something moves by itself: an animation, or the status bar searching.
fn moving(emu: &Emu) -> bool {
    crate::animation::animating(emu) || crate::statusbar::animating(emu)
}

/// Called from the scheduler when nothing can run. Ok(true): input was delivered, look again.
pub(crate) fn idle(emu: &mut Emu, next_ns: Option<u64>) -> Result<bool> {
    if emu.live.host.is_none() {
        return Ok(false);
    }
    // Nothing is running: draw what changed since the last frame once the
    // frame interval has passed (and wake for it then).
    let now = crate::threads::now_ns(emu);
    let mut next_ns = next_ns;
    // The status bar's clock: draw it again when its minute turns over.
    let (minute, next_minute) = crate::statusbar::minute(emu);
    if emu.live.clock_minute.is_some_and(|m| m != minute) {
        emu.live.dirty = true;
    }
    next_ns = Some(next_ns.map_or(next_minute, |n| n.min(next_minute)));
    if emu.live.dirty || moving(emu) {
        let due = emu.live.last_frame.map_or(now, |t| t + FRAME_NS);
        if now >= due {
            draw(emu)?;
        } else {
            next_ns = Some(next_ns.map_or(due, |n| n.min(due)));
        }
    }
    let Some(mut host) = emu.live.host.take() else { return Ok(false) };
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
            // Saving and recording the screen are the Simulator's, not the app's, too.
            let (system, app): (Vec<Event>, Vec<Event>) = events.into_iter().partition(|e| matches!(e, Event::Face(_) | Event::SaveScreen | Event::Record(_)));
            for ev in system {
                match ev {
                    Event::Face(f) => crate::keychain::present(emu, f)?,
                    Event::SaveScreen => capture(emu, |h, f| h.screenshot(f))?,
                    Event::Record(true) if !emu.live.recording => {
                        emu.live.recording = true;
                        capture(emu, |h, f| h.recorded(f))?;
                    }
                    Event::Record(false) if emu.live.recording => {
                        emu.live.recording = false;
                        let now = crate::threads::now_ns(emu);
                        if let Some(h) = emu.live.host.as_mut() {
                            h.recording_stopped(now);
                        }
                    }
                    _ => {}
                }
            }
            if app.is_empty() {
                return Ok(true);
            }
            emu.live.pending.extend(app);
            signal_input(emu)?;
            Ok(true)
        }
    }
}

/// Signal the input source and wake the main run loop: its perform callout
/// runs on the main thread at the run loop's next turn. False before the
/// source is installed.
pub(crate) fn signal_input(emu: &mut Emu) -> Result<bool> {
    if emu.live.source == 0 {
        return Ok(false);
    }
    foundation::cf_call(emu, "_CFRunLoopSourceSignal", &[emu.live.source])?;
    foundation::cf_call(emu, "_CFRunLoopWakeUp", &[emu.live.run_loop])?;
    Ok(true)
}

/// The source's perform callout: deliver what arrived, in order, then give
/// a driving test its turn.
fn perform(emu: &mut Emu) -> Result<()> {
    deliver(emu)?;
    crate::driver::turn(emu)
}

fn deliver(emu: &mut Emu) -> Result<()> {
    for ev in std::mem::take(&mut emu.live.pending) {
        match ev {
            Event::Touch(inp) => crate::touch::send_touch(emu, inp.phase, inp.x, inp.y)?,
            Event::Key(ch) => crate::textinput::key(emu, ch)?,
            Event::Rotate(o) => crate::orientation::turn_device(emu, o)?,
            Event::Face(f) => crate::keychain::present(emu, f)?,
            Event::SaveScreen | Event::Record(_) => {}
        }
    }
    Ok(())
}

/// Render the screen at the phone's own resolution and hand it to the host.
fn capture(emu: &mut Emu, give: impl FnOnce(&mut dyn Host, Frame<'_>)) -> Result<()> {
    let pix = crate::render::render(emu, emu.device.scale as f32)?;
    give_frame(emu, &pix, give);
    Ok(())
}

fn give_frame(emu: &mut Emu, pix: &tiny_skia::Pixmap, give: impl FnOnce(&mut dyn Host, Frame<'_>)) {
    let orientation = crate::orientation::current(emu);
    let at_ns = crate::threads::now_ns(emu);
    if let Some(mut host) = emu.live.host.take() {
        give(host.as_mut(), Frame { rgba: pix.data(), width: pix.width(), height: pix.height(), orientation, at_ns });
        emu.live.host = Some(host);
    }
}

/// Set up the input source on the main run loop (when there's a host).
pub(crate) fn install(emu: &mut Emu) -> Result<()> {
    if emu.live.host.is_none() && !crate::driver::active(emu) {
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

/// When the next frame is due, if there's a live host and something to draw
/// (a change, or an animation running): a deadline for the scheduler, so idle
/// time stops there for the frame instead of jumping past it.
pub(crate) fn frame_due(emu: &Emu) -> Option<u64> {
    if emu.live.host.is_none() || !(emu.live.dirty || moving(emu)) {
        return None;
    }
    let now = crate::threads::now_ns(emu);
    Some(emu.live.last_frame.map_or(now, |t| (t + FRAME_NS).max(now)))
}

/// Between thread slices while the main thread waits: keep the app's clock up
/// with the wall clock, and, when its run loop is asleep (`committed`: every
/// change it made is done, as iOS's render server only shows committed
/// layers), draw a frame if one is due, as animations play on iOS whatever
/// the app's threads are doing. Only with a live host: headless runs keep
/// time by instructions alone, deterministically.
pub(crate) fn keep_pace(emu: &mut Emu, committed: bool) -> Result<()> {
    let Some(host) = emu.live.host.as_mut() else { return Ok(()) };
    let wall = host.wall_ns();
    let now = crate::threads::now_ns(emu);
    if let Some(w) = wall
        && w > now + 1_000_000
    {
        crate::threads::advance_to(emu, w);
    }
    if committed && frame_due(emu).is_some_and(|due| due <= crate::threads::now_ns(emu)) {
        draw(emu)?;
    }
    Ok(())
}

/// Tell the host a biometry prompt is waiting.
pub(crate) fn biometry_prompt(emu: &mut Emu, reason: &str) {
    if let Some(host) = emu.live.host.as_mut() {
        host.biometry_prompt(reason);
    }
}

/// Tell the host the waiting biometry prompts were answered.
pub(crate) fn biometry_answered(emu: &mut Emu) {
    if let Some(host) = emu.live.host.as_mut() {
        host.biometry_answered();
    }
}

/// Something on screen changed (a view or layer, text, a Metal frame): the next frame shows it.
pub(crate) fn changed(emu: &mut Emu) {
    emu.live.dirty = true;
}

/// Something drew outside the view and layer state (a Metal frame, text): the next frame shows it.
pub(crate) fn frame_changed(emu: &mut Emu) {
    changed(emu);
}

/// After the layout pass (CoreAnimation's commit): render a frame if
/// something changed and the frame interval has passed; otherwise the next
/// idle moment draws it.
pub(crate) fn maybe_frame(emu: &mut Emu) -> Result<()> {
    if emu.live.host.is_none() || !(emu.live.dirty || moving(emu)) {
        return Ok(());
    }
    let now = crate::threads::now_ns(emu);
    if emu.live.last_frame.is_some_and(|t| now < t + FRAME_NS) {
        return Ok(()); // too soon: the next idle moment draws it
    }
    draw(emu)
}

fn draw(emu: &mut Emu) -> Result<()> {
    emu.live.last_frame = Some(crate::threads::now_ns(emu));
    emu.live.dirty = false;
    emu.live.clock_minute = Some(crate::statusbar::minute(emu).0);
    let pix = crate::render::render(emu, emu.live.scale)?;
    give_frame(emu, &pix, |h, f| h.frame(f));
    if emu.live.recording {
        // The recording is at the phone's resolution, whatever the window's.
        if emu.device.scale as f32 == emu.live.scale {
            give_frame(emu, &pix, |h, f| h.recorded(f));
        } else {
            capture(emu, |h, f| h.recorded(f))?;
        }
    }
    Ok(())
}

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
//! Hosts can also show the app to the person's screen reader: one that
//! [`Host::wants_accessibility`] gets the screen's accessibility tree
//! ([`crate::accessibility::tree`]) through [`Host::accessibility`] whenever
//! it changes, and sends back what the screen reader did to an element as
//! [`Event::Accessibility`]. The tree is read on the main thread at a run-loop
//! turn (it asks the app's objects, as VoiceOver does), at most every
//! [`AX_INTERVAL_NS`] while the screen keeps changing.
//!
//! A run without a host (tests, `pomme run`) stays fully deterministic; a
//! live run is deterministic given the input and the times it arrived.

use crate::accessibility::{Action, Node};
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
    /// The screen reader did something to an element of the last tree the host got (by its id).
    Accessibility { id: u64, action: Action },
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
    /// Where the wall-clock time since the last frame went, when the host
    /// keeps a clock ([`Host::clock_ms`]); none for captures.
    pub timing: Option<FrameTiming>,
}

/// Where the wall-clock time between two frames went (ms, the host's clock).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameTiming {
    /// When the time since the last frame started (the last frame was handed over).
    pub since_ms: f64,
    /// When rendering this frame started.
    pub render_start_ms: f64,
    /// Rendering this frame.
    pub render_ms: f64,
    /// The app running (its code and pomme's system libraries), since the last frame.
    pub app_ms: f64,
    /// Reading the accessibility tree for the host's screen reader.
    pub accessibility_ms: f64,
    /// Waiting: every thread idle, the host pacing virtual time or waiting for input.
    pub idle_ms: f64,
    /// Handing the last frame to the host (copying it out).
    pub present_ms: f64,
    /// The app's instructions since the last frame.
    pub instructions: u64,
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
    /// A monotonic wall clock (ms), if the host keeps one: then each frame
    /// says where the time since the last one went ([`Frame::timing`]).
    fn clock_ms(&mut self) -> Option<f64> {
        None
    }
    /// Whether the host shows the app to assistive technology now (a screen
    /// reader is listening): then [`Host::accessibility`] hears of the screen.
    fn wants_accessibility(&mut self) -> bool {
        false
    }
    /// The screen as a screen reader sees it, when it's first wanted and
    /// whenever it changes: windows, groups and elements in reading order.
    fn accessibility(&mut self, _tree: &[Node]) {}
}

/// How smoothly frames arrive, for a host to report: per second of wall
/// time, how many, how much of the app's (virtual) time they covered, the
/// longest wall-clock gap between consecutive frames of motion, and, when
/// frames carry their [`FrameTiming`], where the time per frame went. The
/// host gives each frame and its own wall clock (ms).
#[derive(Default)]
pub struct FrameStats {
    since: Option<f64>,
    count: u32,
    first_virtual: u64,
    last_virtual: u64,
    last: Option<f64>,
    longest_ms: f64,
    timed: u32,
    sum: FrameTiming,
    max_render_ms: f64,
    max_app_ms: f64,
}

impl FrameStats {
    /// A frame arrived; once a second's worth has, the line to report.
    pub fn frame(&mut self, f: &Frame<'_>, wall_ms: f64) -> Option<String> {
        let virtual_ns = f.at_ns;
        let since = *self.since.get_or_insert(wall_ms);
        if self.count == 0 {
            self.first_virtual = virtual_ns;
        }
        // A gap counts only between consecutive frames of motion (no more
        // than ~1.5 frames apart in the app's time): idle stretches with
        // nothing to draw aren't stutters.
        let moving = self.last.is_some() && virtual_ns.saturating_sub(self.last_virtual) <= 50_000_000;
        if let Some(last) = self.last
            && moving
        {
            self.longest_ms = self.longest_ms.max(wall_ms - last);
        }
        // Timings of frames of motion (an idle stretch's wait isn't a frame's cost).
        if let Some(t) = f.timing
            && moving
        {
            self.timed += 1;
            self.sum.render_ms += t.render_ms;
            self.sum.app_ms += t.app_ms;
            self.sum.accessibility_ms += t.accessibility_ms;
            self.sum.idle_ms += t.idle_ms;
            self.sum.present_ms += t.present_ms;
            self.sum.instructions += t.instructions;
            self.max_render_ms = self.max_render_ms.max(t.render_ms);
            self.max_app_ms = self.max_app_ms.max(t.app_ms);
        }
        self.last = Some(wall_ms);
        self.count += 1;
        self.last_virtual = virtual_ns;
        if wall_ms - since < 1000.0 {
            return None;
        }
        let wall = (wall_ms - since) / 1000.0;
        let app = (self.last_virtual - self.first_virtual) as f64 / 1e9;
        let mut line = format!("frames: {:.0}/s, {app:.2} s of the app's time in {wall:.2} s, longest gap {:.0} ms", self.count as f64 / wall, self.longest_ms);
        if self.timed > 0 {
            let n = self.timed as f64;
            let s = &self.sum;
            line += &format!(
                "; per moving frame: app {:.1} ms (max {:.1}, {:.1}M instructions), render {:.1} ms (max {:.1}), present {:.1} ms, accessibility {:.1} ms, waiting {:.1} ms",
                s.app_ms / n,
                self.max_app_ms,
                s.instructions as f64 / n / 1e6,
                s.render_ms / n,
                self.max_render_ms,
                s.present_ms / n,
                s.accessibility_ms / n,
                s.idle_ms / n
            );
        }
        *self = FrameStats::default();
        Some(line)
    }
}

/// The least time between two readings of the accessibility tree while the screen changes.
pub const AX_INTERVAL_NS: u64 = 200_000_000;

/// The accessibility tree, as the host has it.
#[derive(Clone, Default)]
struct Ax {
    /// The host wants it.
    on: bool,
    /// The screen changed since it was last read.
    stale: bool,
    /// The input source is signalled to read it.
    signalled: bool,
    /// When it was last read (virtual ns).
    read_at: Option<u64>,
    /// What the host was last given (actions name its nodes).
    shown: Vec<Node>,
}

#[derive(Clone, Default)]
pub struct State {
    pub host: crate::External<Box<dyn Host>>,
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
    ax: Ax,
    timing: Timing,
}

/// The time since the last frame so far, by where it went.
#[derive(Clone, Default)]
struct Timing {
    /// When the last frame was handed over (none: no clock, or no frame yet).
    since: Option<f64>,
    idle_ms: f64,
    accessibility_ms: f64,
    present_ms: f64,
    instructions_at: u64,
}

fn clock(emu: &mut Emu) -> Option<f64> {
    emu.live.host.as_mut().and_then(|h| h.clock_ms())
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
    // The accessibility tree: read it on the main thread when it may have changed.
    let wants = emu.live.host.as_mut().is_some_and(|h| h.wants_accessibility());
    if wants != emu.live.ax.on {
        emu.live.ax = Ax { on: wants, stale: wants, ..Ax::default() };
    }
    if let Some(due) = accessibility_due(emu) {
        if now < due {
            next_ns = Some(next_ns.map_or(due, |n| n.min(due)));
        } else if signal_input(emu)? {
            emu.live.ax.signalled = true;
            return Ok(true);
        }
    }
    let Some(mut host) = emu.live.host.take() else { return Ok(false) };
    let waiting = host.clock_ms();
    let r = host.idle(now, next_ns);
    if let (Some(a), Some(b)) = (waiting, host.clock_ms()) {
        emu.live.timing.idle_ms += b - a;
    }
    *emu.live.host = Some(host);
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

/// The source's perform callout: read the accessibility tree if that's due,
/// deliver what arrived, in order, then give a driving test its turn.
fn perform(emu: &mut Emu) -> Result<()> {
    if std::mem::take(&mut emu.live.ax.signalled) {
        read_accessibility(emu)?;
    }
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
            Event::Accessibility { id, action } => {
                let shown = std::mem::take(&mut emu.live.ax.shown);
                let r = crate::accessibility::perform(emu, &shown, id, action);
                emu.live.ax.shown = shown;
                // As VoiceOver, look again after acting: what changed may not have drawn anything.
                accessibility_changed(emu);
                r?
            }
            Event::SaveScreen | Event::Record(_) => {}
        }
    }
    Ok(())
}

/// Where the time since the last frame went, now that this one is rendered
/// (rendering started at `render_start`); the tally starts again.
fn frame_timing(emu: &mut Emu, render_start: Option<f64>) -> Option<FrameTiming> {
    let (start, end) = (render_start?, clock(emu)?);
    let steps = emu.cpu.steps;
    let t = std::mem::take(&mut emu.live.timing);
    emu.live.timing.instructions_at = steps;
    // (The first frame has nothing before it to account for.)
    let since = t.since?;
    let between = start - since;
    Some(FrameTiming {
        since_ms: since,
        render_start_ms: start,
        render_ms: end - start,
        app_ms: (between - t.idle_ms - t.accessibility_ms).max(0.0),
        accessibility_ms: t.accessibility_ms,
        idle_ms: t.idle_ms,
        present_ms: t.present_ms,
        instructions: steps - t.instructions_at,
    })
}

/// Read the accessibility tree and give it to the host if it changed.
fn read_accessibility(emu: &mut Emu) -> Result<()> {
    let started = clock(emu);
    let tree = crate::accessibility::tree(emu)?;
    if let (Some(a), Some(b)) = (started, clock(emu)) {
        emu.live.timing.accessibility_ms += b - a;
    }
    // Changes made while reading (an app building its elements as it's asked) don't count.
    emu.live.ax.stale = false;
    emu.live.ax.read_at = Some(crate::threads::now_ns(emu));
    if tree != emu.live.ax.shown
        && let Some(host) = emu.live.host.as_mut()
    {
        host.accessibility(&tree);
        emu.live.ax.shown = tree;
    }
    Ok(())
}

/// What the host side knows, which isn't the phone's: the accessibility tree
/// the host was given and the frame timing so far. Restoring a snapshot keeps
/// it, as it keeps the host ([`take_host_side`], [`put_host_side`]).
pub(crate) struct HostSide {
    ax: Ax,
    timing: Timing,
}

pub(crate) fn take_host_side(emu: &mut Emu) -> HostSide {
    HostSide { ax: std::mem::take(&mut emu.live.ax), timing: std::mem::take(&mut emu.live.timing) }
}

/// Put back what the host side knows; the screen it shows may have changed, so the tree is read again.
pub(crate) fn put_host_side(emu: &mut Emu, side: HostSide) {
    emu.live.ax = side.ax;
    emu.live.ax.signalled = false;
    emu.live.timing = side.timing;
    accessibility_changed(emu);
}

/// What assistive technology sees may have changed (an accessibility property was set).
pub(crate) fn accessibility_changed(emu: &mut Emu) {
    if emu.live.ax.on {
        emu.live.ax.stale = true;
    }
}

/// Render the screen at the phone's own resolution and hand it to the host.
fn capture(emu: &mut Emu, give: impl FnOnce(&mut dyn Host, Frame<'_>)) -> Result<()> {
    let pix = crate::render::render(emu, emu.device.scale as f32)?;
    give_frame(emu, &pix, give);
    Ok(())
}

fn give_frame(emu: &mut Emu, pix: &tiny_skia::Pixmap, give: impl FnOnce(&mut dyn Host, Frame<'_>)) {
    give_timed_frame(emu, pix, None, give)
}

fn give_timed_frame(emu: &mut Emu, pix: &tiny_skia::Pixmap, timing: Option<FrameTiming>, give: impl FnOnce(&mut dyn Host, Frame<'_>)) {
    let orientation = crate::orientation::current(emu);
    let at_ns = crate::threads::now_ns(emu);
    if let Some(mut host) = emu.live.host.take() {
        give(host.as_mut(), Frame { rgba: pix.data(), width: pix.width(), height: pix.height(), orientation, at_ns, timing });
        *emu.live.host = Some(host);
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

/// When the accessibility tree is next to be read, if the host wants it and
/// the screen changed since: a deadline for the scheduler, like a frame's.
pub(crate) fn accessibility_due(emu: &Emu) -> Option<u64> {
    let ax = &emu.live.ax;
    if !(ax.on && ax.stale) || ax.signalled || emu.live.host.is_none() {
        return None;
    }
    let now = crate::threads::now_ns(emu);
    Some(ax.read_at.map_or(now, |t| (t + AX_INTERVAL_NS).max(now)))
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
    accessibility_changed(emu);
    let render_start = clock(emu);
    let pix = crate::render::render(emu, emu.live.scale)?;
    let timing = frame_timing(emu, render_start);
    give_timed_frame(emu, &pix, timing, |h, f| h.frame(f));
    // The next frame's time starts once this one is handed over.
    if let Some(handed) = clock(emu) {
        if let Some(t) = timing {
            emu.live.timing.present_ms = handed - (t.render_start_ms + t.render_ms);
        }
        emu.live.timing.since = Some(handed);
    }
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

// ---- Waiting for the host's asynchronous work (the GPU) ----

thread_local! {
    /// What [`block_on`] does while a future isn't ready: set by a host that
    /// can pause (the browser, with JSPI), so the work can finish meanwhile.
    static PAUSE: std::cell::Cell<Option<fn()>> = const { std::cell::Cell::new(None) };
}

/// Set (or clear) the pause hook. The browser host sets one that pauses the
/// emulator for one browser task.
pub fn set_pause_hook(hook: Option<fn()>) {
    PAUSE.with(|p| p.set(hook));
}

/// Whether [`block_on`] can wait: always on native hosts; in the browser,
/// only with a pause hook.
pub fn can_wait() -> bool {
    !cfg!(target_arch = "wasm32") || PAUSE.with(|p| p.get()).is_some()
}

/// Wait for `fut`. With a pause hook, poll it and pause between polls; with
/// none, block the thread (native hosts: what they always did). In the
/// browser, blocking would also block the browser that must do the work, so
/// with no hook it's an error rather than a hang.
pub fn block_on<F: std::future::Future>(fut: F) -> std::result::Result<F::Output, String> {
    let Some(pause) = PAUSE.with(|p| p.get()) else {
        #[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
        return Ok(pollster::block_on(fut));
        #[cfg(not(all(feature = "gpu", not(target_arch = "wasm32"))))]
        return Err("can't wait here: the host can't pause (in a browser, it needs JSPI)".into());
    };
    let mut fut = std::pin::pin!(fut);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        if let std::task::Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return Ok(v);
        }
        pause();
    }
}

#[cfg(test)]
mod wait_tests {
    use super::*;
    use std::cell::Cell;
    use std::task::Poll;

    thread_local! { static PAUSES: Cell<u32> = const { Cell::new(0) }; }
    fn count() {
        PAUSES.with(|p| p.set(p.get() + 1));
    }

    #[test]
    fn block_on_pauses_until_ready() {
        set_pause_hook(Some(count));
        let mut polls = 0;
        let r = block_on(std::future::poll_fn(|_| {
            polls += 1;
            if polls < 3 { Poll::Pending } else { Poll::Ready(42) }
        }));
        set_pause_hook(None);
        assert_eq!(r, Ok(42));
        assert_eq!(PAUSES.with(|p| p.get()), 2, "one pause between each poll that wasn't ready");
        assert!(can_wait());
    }
}

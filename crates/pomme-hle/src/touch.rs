//! Touch input: UITouch, UIEvent, hit testing, gesture recognizers, and
//! delivery the way UIKit does it.
//!
//! A touch hit-tests the key window (`-hitTest:withEvent:`, which apps
//! override), then goes to the gesture recognizers of the hit view and its
//! ancestors, then to the view's responder chain. React Native listens with a
//! recognizer on its root view (RCTSurfaceTouchHandler), so recognizers are
//! the path that matters most.
//!
//! Input is scripted: events are queued with a virtual time and fire from
//! main-run-loop timers, so a run with the same script is the same run.

use crate::foundation;
use crate::objc::{self, host_class, send};
use crate::uiview::{frame_of, view};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

pub const BEGAN: u64 = 0;
pub const MOVED: u64 = 1;
pub const ENDED: u64 = 3;
pub const CANCELLED: u64 = 4;

// UIGestureRecognizerState
const POSSIBLE: u64 = 0;
const STATE_BEGAN: u64 = 1;
const CHANGED: u64 = 2;
const RECOGNIZED: u64 = 3;
const STATE_CANCELLED: u64 = 4;
const FAILED: u64 = 5;

#[derive(Clone, Debug, Default)]
struct Touch {
    view: u64,
    window: u64,
    phase: u64,
    /// In window coordinates.
    location: [f64; 2],
    previous: [f64; 2],
    timestamp: f64,
    /// Recognizers this touch is delivered to (fixed when it begins).
    recognizers: Vec<u64>,
}

#[derive(Clone, Debug)]
struct Recognizer {
    view: u64,
    state: u64,
    targets: Vec<(u64, u64)>,
    delegate: u64,
    enabled: bool,
}

impl Default for Recognizer {
    fn default() -> Self {
        Recognizer { view: 0, state: POSSIBLE, targets: Vec::new(), delegate: 0, enabled: true }
    }
}

/// A scripted input event: at `at_ns` of virtual time, a touch phase at a window point.
#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub at_ns: u64,
    pub phase: u64,
    pub x: f64,
    pub y: f64,
}

#[derive(Default)]
pub struct State {
    touches: HashMap<u64, Touch>,
    recognizers: HashMap<u64, Recognizer>,
    view_recognizers: HashMap<u64, Vec<u64>>,
    /// UIEvent → its touches.
    events: HashMap<u64, Vec<u64>>,
    /// The finger currently down.
    active: u64,
    pub script: Vec<Input>,
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn ret_point(emu: &mut Emu, p: [f64; 2]) -> Result<()> {
    emu.cpu.q[0] = p[0].to_bits() as u128;
    emu.cpu.q[1] = p[1].to_bits() as u128;
    Ok(())
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn rec_mut(emu: &mut Emu, r: u64) -> &mut Recognizer {
    emu.touch.recognizers.entry(r).or_default()
}

fn rec(emu: &mut Emu, r: u64) -> Recognizer {
    rec_mut(emu, r).clone()
}

/// Where a view's bounds origin sits in window coordinates (no transforms).
fn origin_in_window(emu: &mut Emu, mut v: u64) -> [f64; 2] {
    let mut p = [0.0, 0.0];
    // Window coordinates of the view's (0,0) bounds point = Σ (frame origin − superview bounds origin) − own bounds origin.
    let own = view(emu, v).bounds;
    p[0] -= own[0];
    p[1] -= own[1];
    loop {
        let vw = view(emu, v);
        if vw.superview == 0 {
            break;
        }
        let f = frame_of(emu, v);
        let sb = view(emu, vw.superview).bounds;
        p[0] += f[0] - sb[0];
        p[1] += f[1] - sb[1];
        v = vw.superview;
    }
    p
}

/// A window point in `v`'s coordinates (nil: window coordinates).
fn to_view(emu: &mut Emu, p: [f64; 2], v: u64) -> [f64; 2] {
    if v == 0 {
        return p;
    }
    let o = origin_in_window(emu, v);
    [p[0] - o[0], p[1] - o[1]]
}

// ---- hit testing ----

/// `-[UIView hitTest:withEvent:]` (point in d0/d1, in the receiver's coordinates).
fn hit_test(emu: &mut Emu) -> Result<()> {
    let (v, event, x, y) = (emu.arg(0), emu.arg(2), d(emu, 0), d(emu, 1));
    let vw = view(emu, v);
    if vw.hidden || !vw.user_interaction || vw.alpha < 0.01 {
        return ret(emu, 0);
    }
    emu.cpu.q[0] = x.to_bits() as u128;
    emu.cpu.q[1] = y.to_bits() as u128;
    if send(emu, v, "pointInside:withEvent:", &[event])? & 0xff == 0 {
        return ret(emu, 0);
    }
    for s in vw.subviews.iter().rev() {
        let f = frame_of(emu, *s);
        let sb = view(emu, *s).bounds;
        emu.cpu.q[0] = (x - f[0] + sb[0]).to_bits() as u128;
        emu.cpu.q[1] = (y - f[1] + sb[1]).to_bits() as u128;
        let hit = send(emu, *s, "hitTest:withEvent:", &[event])?;
        if hit != 0 {
            return ret(emu, hit);
        }
    }
    ret(emu, v)
}

// ---- delivery ----

fn new_object(emu: &mut Emu, class: &str) -> Result<u64> {
    let cls = host_class(emu, class)?;
    objc::create_instance(emu, cls, 0)
}

fn set_of(emu: &mut Emu, objs: &[u64]) -> Result<u64> {
    let cls = host_class(emu, "NSSet")?;
    let a = foundation::new_array_of(emu, objs)?;
    send(emu, cls, "setWithArray:", &[a])
}

fn phase_selector(phase: u64) -> &'static str {
    match phase {
        BEGAN => "touchesBegan:withEvent:",
        MOVED => "touchesMoved:withEvent:",
        ENDED => "touchesEnded:withEvent:",
        _ => "touchesCancelled:withEvent:",
    }
}

/// Recognizers on `v` and its ancestors, innermost first.
fn recognizers_for(emu: &mut Emu, mut v: u64) -> Vec<u64> {
    let mut out = Vec::new();
    while v != 0 {
        if let Some(rs) = emu.touch.view_recognizers.get(&v) {
            out.extend(rs.iter().copied().filter(|r| emu.touch.recognizers.get(r).is_none_or(|x| x.enabled)));
        }
        v = view(emu, v).superview;
    }
    out
}

/// Deliver one touch phase at a window point.
/// Where the finger being delivered is, in window coordinates.
pub(crate) fn active_location(emu: &Emu) -> Option<[f64; 2]> {
    emu.touch.touches.get(&emu.touch.active).map(|t| t.location)
}

pub fn send_touch(emu: &mut Emu, phase: u64, x: f64, y: f64) -> Result<()> {
    let window = if emu.ui.key_window != 0 { emu.ui.key_window } else { emu.ui.windows.first().copied().unwrap_or(0) };
    if window == 0 {
        return Ok(());
    }
    let now = crate::threads::now_ns(emu) as f64 / 1e9;
    let touch = if phase == BEGAN {
        emu.cpu.q[0] = x.to_bits() as u128;
        emu.cpu.q[1] = y.to_bits() as u128;
        let hit = send(emu, window, "hitTest:withEvent:", &[0])?;
        let t = new_object(emu, "UITouch")?;
        let recognizers = recognizers_for(emu, hit);
        emu.touch.touches.insert(t, Touch { view: hit, window, phase, location: [x, y], previous: [x, y], timestamp: now, recognizers });
        emu.touch.active = t;
        t
    } else {
        let t = emu.touch.active;
        let Some(tc) = emu.touch.touches.get_mut(&t) else { return Ok(()) };
        tc.previous = tc.location;
        tc.location = [x, y];
        tc.phase = phase;
        tc.timestamp = now;
        t
    };
    let tc = emu.touch.touches[&touch].clone();
    let event = new_object(emu, "UIEvent")?;
    emu.touch.events.insert(event, vec![touch]);
    let touches = set_of(emu, &[touch])?;
    let sel = phase_selector(phase);
    for &r in &tc.recognizers {
        let st = rec(emu, r).state;
        if matches!(st, FAILED | STATE_CANCELLED) {
            continue;
        }
        send(emu, r, sel, &[touches, event])?;
    }
    // The view's responder chain (UIView passes touches up by default).
    if tc.view != 0 {
        send(emu, tc.view, sel, &[touches, event])?;
    }
    if phase == ENDED || phase == CANCELLED {
        // Every finger is up: recognizers that finished reset to Possible.
        for &r in &tc.recognizers {
            let st = rec(emu, r).state;
            if st != POSSIBLE {
                send(emu, r, "reset", &[])?;
                rec_mut(emu, r).state = POSSIBLE;
            }
        }
        emu.touch.active = 0;
    }
    Ok(())
}

/// Timer callout `(CFRunLoopTimerRef, info)`: info is the script index.
fn fire_input(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(1) as usize;
    let Some(inp) = emu.touch.script.get(i).copied() else { return Ok(()) };
    send_touch(emu, inp.phase, inp.x, inp.y)
}

/// Put the scripted input on the main run loop as one-shot timers.
pub fn schedule_script(emu: &mut Emu) -> Result<()> {
    if emu.touch.script.is_empty() {
        return Ok(());
    }
    let callout = emu.host_thunk(fire_input)?;
    let rl = foundation::cf_call(emu, "_CFRunLoopGetMain", &[])?;
    let common = foundation::cf_data_pub(emu, "_kCFRunLoopCommonModes")?;
    foundation::cf_call(emu, "_CFAbsoluteTimeGetCurrent", &[])?;
    let abs_now = f64::from_bits(emu.last_q0 as u64);
    let now = crate::threads::now_ns(emu);
    for i in 0..emu.touch.script.len() {
        let at = emu.touch.script[i].at_ns;
        let fire = abs_now + (at.saturating_sub(now)) as f64 / 1e9;
        // CFRunLoopTimerContext {version, info, retain, release, copyDescription}
        let ctx = emu.alloc(40)?;
        emu.write_u64(ctx + 8, i as u64)?;
        emu.cpu.q[0] = fire.to_bits() as u128;
        emu.cpu.q[1] = 0f64.to_bits() as u128;
        let timer = foundation::cf_call(emu, "_CFRunLoopTimerCreate", &[0, 0, 0, callout, ctx])?;
        foundation::cf_call(emu, "_CFRunLoopAddTimer", &[rl, timer, common])?;
    }
    Ok(())
}

// ---- UIView responder defaults ----

/// UIResponder's default: pass the touches to the next responder (the superview).
fn view_forward(emu: &mut Emu, sel: &str) -> Result<()> {
    let (v, touches, event) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let sup = view(emu, v).superview;
    if sup != 0 {
        send(emu, sup, sel, &[touches, event])?;
    }
    Ok(())
}

fn add_recognizer(emu: &mut Emu) -> Result<()> {
    let (v, r) = (emu.arg(0), emu.arg(2));
    if r == 0 {
        return Ok(());
    }
    let list = emu.touch.view_recognizers.entry(v).or_default();
    if !list.contains(&r) {
        list.push(r);
    }
    rec_mut(emu, r).view = v;
    Ok(())
}

fn remove_recognizer(emu: &mut Emu) -> Result<()> {
    let (v, r) = (emu.arg(0), emu.arg(2));
    if let Some(list) = emu.touch.view_recognizers.get_mut(&v) {
        list.retain(|&x| x != r);
    }
    rec_mut(emu, r).view = 0;
    Ok(())
}

fn view_recognizers(emu: &mut Emu) -> Result<()> {
    let list = emu.touch.view_recognizers.get(&emu.arg(0)).cloned().unwrap_or_default();
    let a = foundation::new_array_of(emu, &list)?;
    ret(emu, a)
}

// ---- UIGestureRecognizer ----

fn gr_init_with_target(emu: &mut Emu) -> Result<()> {
    let (r, target, action) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let g = rec_mut(emu, r);
    *g = Recognizer::default();
    if target != 0 && action != 0 {
        g.targets.push((target, action));
    }
    ret(emu, r)
}

fn gr_set_state(emu: &mut Emu) -> Result<()> {
    let (r, state) = (emu.arg(0), emu.arg(2));
    rec_mut(emu, r).state = state;
    if matches!(state, STATE_BEGAN | CHANGED | RECOGNIZED | STATE_CANCELLED) {
        for (target, action) in rec(emu, r).targets {
            let name = objc::sel_name(emu, action);
            send(emu, target, &name, &[r])?;
        }
    }
    Ok(())
}

fn gr_location(emu: &mut Emu) -> Result<()> {
    let (r, v) = (emu.arg(0), emu.arg(2));
    let t = emu.touch.touches.iter().find(|(_, t)| t.recognizers.contains(&r)).map(|(_, t)| t.location);
    let p = match t {
        Some(p) => to_view(emu, p, v),
        None => [0.0, 0.0],
    };
    ret_point(emu, p)
}

// ---- UITouch / UIEvent ----

fn touch(emu: &Emu, t: u64) -> Touch {
    emu.touch.touches.get(&t).cloned().unwrap_or_default()
}

fn touch_location(emu: &mut Emu, previous: bool) -> Result<()> {
    let (t, v) = (emu.arg(0), emu.arg(2));
    let tc = touch(emu, t);
    let p = to_view(emu, if previous { tc.previous } else { tc.location }, v);
    ret_point(emu, p)
}

fn event_all_touches(emu: &mut Emu) -> Result<()> {
    let ts = emu.touch.events.get(&emu.arg(0)).cloned().unwrap_or_default();
    let s = set_of(emu, &ts)?;
    ret(emu, s)
}

/// `-touchesForView:` / `-touchesForGestureRecognizer:` / `-touchesForWindow:`
fn event_touches_for(emu: &mut Emu, kind: u8) -> Result<()> {
    let (e, x) = (emu.arg(0), emu.arg(2));
    let ts = emu.touch.events.get(&e).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for t in ts {
        let tc = touch(emu, t);
        let ok = match kind {
            0 => {
                // the touch's view or a descendant of x
                let mut v = tc.view;
                while v != 0 && v != x {
                    v = view(emu, v).superview;
                }
                v != 0
            }
            1 => tc.recognizers.contains(&x),
            _ => tc.window == x,
        };
        if ok {
            out.push(t);
        }
    }
    let s = set_of(emu, &out)?;
    ret(emu, s)
}

fn nothing(_: &mut Emu) -> Result<()> {
    Ok(())
}

const GR: &str = "UIGestureRecognizer";

/// `-convertPoint:toCoordinateSpace:` — the screen's space is the window's (one full-screen window).
fn convert_to_space(emu: &mut Emu) -> Result<()> {
    let (v, space, x, y) = (emu.arg(0), emu.arg(2), d(emu, 0), d(emu, 1));
    let o = origin_in_window(emu, v);
    let mut p = [x + o[0], y + o[1]];
    if space != 0 && emu.ui.views.views.contains_key(&space) {
        p = to_view(emu, p, space);
    }
    ret_point(emu, p)
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    ("UIView", false, "convertPoint:toCoordinateSpace:", convert_to_space),
    ("UIScreen", false, "coordinateSpace", |e| {
        let s = e.arg(0);
        ret(e, s)
    }),
    ("UIScreen", false, "fixedCoordinateSpace", |e| {
        let s = e.arg(0);
        ret(e, s)
    }),
    ("UIView", false, "hitTest:withEvent:", hit_test),
    ("UIView", false, "touchesBegan:withEvent:", |e| view_forward(e, "touchesBegan:withEvent:")),
    ("UIView", false, "touchesMoved:withEvent:", |e| view_forward(e, "touchesMoved:withEvent:")),
    ("UIView", false, "touchesEnded:withEvent:", |e| view_forward(e, "touchesEnded:withEvent:")),
    ("UIView", false, "touchesCancelled:withEvent:", |e| view_forward(e, "touchesCancelled:withEvent:")),
    ("UIView", false, "addGestureRecognizer:", add_recognizer),
    ("UIView", false, "removeGestureRecognizer:", remove_recognizer),
    ("UIView", false, "gestureRecognizers", view_recognizers),
    (GR, false, "init", |e| {
        e.cpu.x[2] = 0;
        e.cpu.x[3] = 0;
        gr_init_with_target(e)
    }),
    (GR, false, "initWithTarget:action:", gr_init_with_target),
    (GR, false, "addTarget:action:", |e| {
        let (r, t, a) = (e.arg(0), e.arg(2), e.arg(3));
        rec_mut(e, r).targets.push((t, a));
        Ok(())
    }),
    (GR, false, "removeTarget:action:", |e| {
        let (r, t) = (e.arg(0), e.arg(2));
        rec_mut(e, r).targets.retain(|&(x, _)| t != 0 && x != t);
        Ok(())
    }),
    (GR, false, "state", |e| {
        let s = rec(e, e.arg(0)).state;
        ret(e, s)
    }),
    (GR, false, "setState:", gr_set_state),
    (GR, false, "view", |e| {
        let v = rec(e, e.arg(0)).view;
        ret(e, v)
    }),
    (GR, false, "delegate", |e| {
        let v = rec(e, e.arg(0)).delegate;
        ret(e, v)
    }),
    (GR, false, "setDelegate:", |e| {
        let (r, d) = (e.arg(0), e.arg(2));
        rec_mut(e, r).delegate = d;
        Ok(())
    }),
    (GR, false, "isEnabled", |e| {
        let v = rec(e, e.arg(0)).enabled;
        ret(e, v as u64)
    }),
    (GR, false, "setEnabled:", |e| {
        let (r, v) = (e.arg(0), e.arg(2) & 0xff != 0);
        rec_mut(e, r).enabled = v;
        Ok(())
    }),
    (GR, false, "locationInView:", gr_location),
    (GR, false, "numberOfTouches", |e| {
        let r = e.arg(0);
        let n = e.touch.touches.values().filter(|t| t.recognizers.contains(&r) && t.phase < ENDED).count();
        ret(e, n as u64)
    }),
    (GR, false, "reset", nothing),
    (GR, false, "touchesBegan:withEvent:", nothing),
    (GR, false, "touchesMoved:withEvent:", nothing),
    (GR, false, "touchesEnded:withEvent:", nothing),
    (GR, false, "touchesCancelled:withEvent:", nothing),
    (GR, false, "requireGestureRecognizerToFail:", nothing),
    (GR, false, "cancelsTouchesInView", |e| ret(e, 0)),
    ("UITouch", false, "locationInView:", |e| touch_location(e, false)),
    ("UITouch", false, "previousLocationInView:", |e| touch_location(e, true)),
    ("UITouch", false, "view", |e| {
        let v = touch(e, e.arg(0)).view;
        ret(e, v)
    }),
    ("UITouch", false, "window", |e| {
        let v = touch(e, e.arg(0)).window;
        ret(e, v)
    }),
    ("UITouch", false, "phase", |e| {
        let v = touch(e, e.arg(0)).phase;
        ret(e, v)
    }),
    ("UITouch", false, "timestamp", |e| {
        let v = touch(e, e.arg(0)).timestamp;
        crate::uikit::ret_f64(e, v);
        Ok(())
    }),
    ("UITouch", false, "tapCount", |e| ret(e, 1)),
    ("UITouch", false, "type", |e| ret(e, 0)),
    ("UITouch", false, "force", |e| {
        crate::uikit::ret_f64(e, 0.0);
        Ok(())
    }),
    ("UITouch", false, "maximumPossibleForce", |e| {
        crate::uikit::ret_f64(e, 0.0);
        Ok(())
    }),
    ("UITouch", false, "majorRadius", |e| {
        crate::uikit::ret_f64(e, 20.0);
        Ok(())
    }),
    ("UITouch", false, "gestureRecognizers", |e| {
        let rs = touch(e, e.arg(0)).recognizers;
        let a = foundation::new_array_of(e, &rs)?;
        ret(e, a)
    }),
    ("UIEvent", false, "allTouches", event_all_touches),
    ("UIEvent", false, "touchesForView:", |e| event_touches_for(e, 0)),
    ("UIEvent", false, "touchesForGestureRecognizer:", |e| event_touches_for(e, 1)),
    ("UIEvent", false, "touchesForWindow:", |e| event_touches_for(e, 2)),
    ("UIEvent", false, "type", |e| ret(e, 0)),
    ("UIEvent", false, "subtype", |e| ret(e, 0)),
    ("UIEvent", false, "timestamp", |e| {
        let now = crate::threads::now_ns(e) as f64 / 1e9;
        crate::uikit::ret_f64(e, now);
        Ok(())
    }),
];

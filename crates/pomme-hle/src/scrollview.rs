//! UIScrollView: content size and insets, the content offset (the bounds'
//! origin), and scrolling by touch: dragging, rubber-banding past the edges,
//! and decelerating after the finger lifts.
//!
//! UIKit drives this with the scroll view's pan gesture recognizer; pomme
//! builds it into touch delivery (touch.rs calls `touch_began` / `_moved` /
//! `_ended`). A touch that starts in a scroll view pans it once it has moved
//! 10 pt.
//!
//! The view under the finger hears of the touch as UIKit arranges it: with
//! `delaysContentTouches` (the default) it gets `touchesBegan` only once the
//! finger has stayed down 150 ms (or lifts sooner: then it hears the whole
//! tap at once), and only if `touchesShouldBegin:withEvent:inContentView:`
//! agrees. A drag that starts before then takes the touch silently. Once the
//! view has the touch, the scroll view takes it only if `canCancelContentTouches`
//! and `touchesShouldCancelInContentView:` say so (the latter is NO for
//! controls, except in table and collection views); the view then gets
//! `touchesCancelled`. Otherwise this touch doesn't scroll. The delegate hears what UIKit tells it:
//! `scrollViewWillBeginDragging:`, `scrollViewDidScroll:` on every change,
//! `scrollViewWillEndDragging:withVelocity:targetContentOffset:` (which may move
//! the target: React Native snaps that way), `scrollViewDidEndDragging:willDecelerate:`,
//! `scrollViewWillBeginDecelerating:` and `scrollViewDidEndDecelerating:`.
//!
//! Deceleration is UIKit's normal rate (0.998 per ms): released at v pt/ms the
//! content travels v · 0.998 / 0.002 more, easing out exponentially; past an
//! edge it eases back. A touch on decelerating content stops it.

use crate::objc::{self, host_class, send};
use crate::uikit::ret_rect;
use crate::uiview::{view, view_mut};
use crate::{Emu, HostFn, Result};

/// UIScrollViewDecelerationRateNormal, per millisecond.
const RATE: f64 = 0.998;
/// How far a finger moves before the scroll view starts dragging.
const SLOP: f64 = 10.0;
/// Past an edge, the content follows the finger at this fraction.
const RUBBER_BAND: f64 = 0.5;
/// Animation frames (60 Hz).
const FRAME_S: f64 = 1.0 / 60.0;

#[derive(Clone, Debug, Default)]
struct Pan {
    /// Scroll views under the finger, innermost first.
    candidates: Vec<u64>,
    /// The one being dragged (0 until the finger moves past the slop).
    view: u64,
    start_touch: [f64; 2],
    start_offset: [f64; 2],
    /// (time ns, finger) for the release velocity.
    samples: Vec<(u64, [f64; 2])>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Glide {
    view: u64,
    from: [f64; 2],
    to: [f64; 2],
    start_ns: u64,
    /// Bumped to stop the glide's frames.
    generation: u64,
}

#[derive(Default)]
pub struct State {
    pan: Option<Pan>,
    glide: Option<Glide>,
    generation: u64,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn d(emu: &Emu, i: usize) -> f64 {
    f64::from_bits(emu.cpu.q[i] as u64)
}

fn bag_f64(emu: &Emu, v: u64, key: &str) -> Option<f64> {
    objc::bag_get(emu, v, key).map(f64::from_bits)
}

fn set_bag_f64(emu: &mut Emu, v: u64, key: &str, x: f64) {
    objc::bag_set(emu, v, key, x.to_bits());
}

/// Whether a sheet may take a vertical drag that began on `hit` (going down
/// when `down`): no scroll view between it and `stop` scrolls that way first —
/// a scroll view at its top lets a downward drag move the sheet.
pub(crate) fn sheet_may_drag(emu: &mut Emu, hit: u64, stop: u64, down: bool) -> Result<bool> {
    let mut v = hit;
    while v != 0 && v != stop {
        if is_scroll_view(emu, v) && scroll_enabled(emu, v) && axes(emu, v)?[1] {
            let (min, _) = range(emu, v)?;
            let at_top = view(emu, v).bounds[1] <= min[1] + 0.5;
            if !(at_top && down) {
                return Ok(false);
            }
        }
        v = view(emu, v).superview;
    }
    Ok(true)
}

/// Something else (a sheet's drag) took the touch: no scroll view pans it.
pub(crate) fn abandon(emu: &mut Emu) {
    if let Some(p) = emu.scroll.pan.take() {
        for v in p.candidates {
            set_flag(emu, v, "tracking", false);
        }
    }
}

pub fn is_scroll_view(emu: &mut Emu, v: u64) -> bool {
    let Ok(cls) = host_class(emu, "UIScrollView") else { return false };
    objc::is_kind_of(emu, v, cls).unwrap_or(false)
}

const INSET_KEYS: [&str; 4] = ["contentInsetTop", "contentInsetLeft", "contentInsetBottom", "contentInsetRight"];

fn content_inset(emu: &Emu, v: u64) -> [f64; 4] {
    INSET_KEYS.map(|k| bag_f64(emu, v, k).unwrap_or(0.0))
}

/// The content's size: what the app set (a text view's comes from its text).
fn content_size(emu: &mut Emu, v: u64) -> Result<[f64; 2]> {
    if crate::textinput::is_text_view(emu, v) {
        let r = objc::send_fp(emu, v, "contentSize", &[], &[])?;
        return Ok([r[0], r[1]]);
    }
    Ok([bag_f64(emu, v, "contentWidth").unwrap_or(0.0), bag_f64(emu, v, "contentHeight").unwrap_or(0.0)])
}

/// The offsets the content rests within: (min, max) per axis.
fn range(emu: &mut Emu, v: u64) -> Result<([f64; 2], [f64; 2])> {
    let size = content_size(emu, v)?;
    let [top, left, bottom, right] = content_inset(emu, v);
    let b = view(emu, v).bounds;
    let min = [0.0 - left, 0.0 - top]; // not -0.0
    let max = [(size[0] + right - b[2]).max(min[0]), (size[1] + bottom - b[3]).max(min[1])];
    Ok((min, max))
}

fn scroll_enabled(emu: &Emu, v: u64) -> bool {
    objc::bag_get(emu, v, "scrollEnabled").is_none_or(|x| x & 0xff != 0)
}

/// Whether `v` scrolls along each axis (its content is larger, or it bounces anyway).
fn axes(emu: &mut Emu, v: u64) -> Result<[bool; 2]> {
    let (min, max) = range(emu, v)?;
    let always = [objc::bag_get(emu, v, "alwaysBounceHorizontal").is_some_and(|x| x & 0xff != 0), objc::bag_get(emu, v, "alwaysBounceVertical").is_some_and(|x| x & 0xff != 0)];
    Ok([max[0] > min[0] || always[0], max[1] > min[1] || always[1]])
}

/// Whether the delegate handles `sel`, asked as UIKit asks: React Native's
/// delegate is a splitter that forwards, answering `respondsToSelector:` itself.
fn delegate_responds(emu: &mut Emu, delegate: u64, sel: &str) -> Result<bool> {
    if delegate == 0 {
        return Ok(false);
    }
    let s = objc::intern(emu, sel)?;
    Ok(send(emu, delegate, "respondsToSelector:", &[s])? & 0xff != 0)
}

fn tell(emu: &mut Emu, v: u64, sel: &str, args: &[u64]) -> Result<()> {
    let delegate = objc::bag_get(emu, v, "delegate").unwrap_or(0);
    if delegate_responds(emu, delegate, sel)? {
        send(emu, delegate, sel, args)?;
    }
    Ok(())
}

/// Move the content to `offset`, telling the delegate.
pub(crate) fn set_offset(emu: &mut Emu, v: u64, offset: [f64; 2]) -> Result<()> {
    let vw = view_mut(emu, v);
    if (vw.bounds[0] - offset[0]).abs() < 1e-9 && (vw.bounds[1] - offset[1]).abs() < 1e-9 {
        return Ok(());
    }
    vw.bounds[0] = offset[0];
    vw.bounds[1] = offset[1];
    tell(emu, v, "scrollViewDidScroll:", &[v])?;
    crate::host::frame_changed(emu);
    Ok(())
}

fn offset(emu: &mut Emu, v: u64) -> [f64; 2] {
    let b = view(emu, v).bounds;
    [b[0], b[1]]
}

// ---- dragging (from touch.rs) ----

/// A finger went down on `hit`: the scroll views around it may pan. A
/// decelerating one stops.
pub(crate) fn touch_began(emu: &mut Emu, hit: u64, at: [f64; 2], now: u64) -> Result<()> {
    if let Some(g) = emu.scroll.glide.take() {
        emu.scroll.generation += 1;
        let _ = g;
        set_flag(emu, g.view, "decelerating", false);
        tell(emu, g.view, "scrollViewDidEndDecelerating:", &[g.view])?;
    }
    let mut candidates = Vec::new();
    let mut v = hit;
    while v != 0 {
        if is_scroll_view(emu, v) && scroll_enabled(emu, v) {
            candidates.push(v);
        }
        v = view(emu, v).superview;
    }
    emu.scroll.pan = (!candidates.is_empty()).then(|| Pan { candidates, view: 0, start_touch: at, start_offset: [0.0; 2], samples: vec![(now, at)] });
    if let Some(sv) = emu.scroll.pan.as_ref().and_then(|p| p.candidates.first().copied()) {
        set_flag(emu, sv, "tracking", true);
    }
    Ok(())
}

fn set_flag(emu: &mut Emu, v: u64, key: &str, on: bool) {
    objc::bag_set(emu, v, &format!("pomme.{key}"), on as u64);
}

fn flag(emu: &Emu, v: u64, key: &str) -> bool {
    objc::bag_get(emu, v, &format!("pomme.{key}")).is_some_and(|x| x != 0)
}

/// Rubber-band an offset past the range.
fn banded(x: f64, min: f64, max: f64) -> f64 {
    if x < min {
        min - (min - x) * RUBBER_BAND
    } else if x > max {
        max + (x - max) * RUBBER_BAND
    } else {
        x
    }
}

/// Whether a touch on `v` reaches it only after the delay: a scroll view it's
/// in (or is) delays content touches.
pub(crate) fn delays_content_touches(emu: &mut Emu, mut v: u64) -> Result<bool> {
    while v != 0 {
        if is_scroll_view(emu, v) && send(emu, v, "delaysContentTouches", &[])? & 0xff != 0 {
            return Ok(true);
        }
        v = view(emu, v).superview;
    }
    Ok(false)
}

/// The innermost scroll view `v` is content of (not `v` itself).
fn enclosing_scroll_view(emu: &mut Emu, v: u64) -> u64 {
    let mut s = view(emu, v).superview;
    while s != 0 && !is_scroll_view(emu, s) {
        s = view(emu, s).superview;
    }
    s
}

/// `touchesShouldBegin:withEvent:inContentView:` of the scroll view around `v` (YES without one).
pub(crate) fn should_begin(emu: &mut Emu, v: u64, touches: u64, event: u64) -> Result<bool> {
    let sv = enclosing_scroll_view(emu, v);
    if sv == 0 {
        return Ok(true);
    }
    Ok(send(emu, sv, "touchesShouldBegin:withEvent:inContentView:", &[touches, event, v])? & 0xff != 0)
}

/// Whether scroll view `sv` may take the touch from `content`, which has it.
fn may_cancel(emu: &mut Emu, sv: u64, content: u64) -> Result<bool> {
    if content == 0 || content == sv {
        return Ok(true);
    }
    Ok(send(emu, sv, "canCancelContentTouches", &[])? & 0xff != 0 && send(emu, sv, "touchesShouldCancelInContentView:", &[content])? & 0xff != 0)
}

/// The finger moved. Returns true when a scroll view started dragging (the
/// view under the finger loses the touch). `content` is the view under the
/// finger if it has the touch (0 if it hasn't heard of it).
pub(crate) fn touch_moved(emu: &mut Emu, at: [f64; 2], now: u64, content: u64) -> Result<bool> {
    let Some(mut pan) = emu.scroll.pan.clone() else { return Ok(false) };
    pan.samples.push((now, at));
    let mut started = false;
    if pan.view == 0 {
        let (dx, dy) = (at[0] - pan.start_touch[0], at[1] - pan.start_touch[1]);
        if dx.hypot(dy) < SLOP {
            emu.scroll.pan = Some(pan);
            return Ok(false);
        }
        // The innermost scroll view that scrolls the way the finger went.
        let vertical = dy.abs() >= dx.abs();
        let mut chosen = 0;
        for &c in &pan.candidates {
            let ax = axes(emu, c)?;
            if (vertical && ax[1]) || (!vertical && ax[0]) {
                chosen = c;
                break;
            }
        }
        if chosen == 0 {
            emu.scroll.pan = Some(pan);
            return Ok(false);
        }
        // The view under the finger keeps its touch: this touch doesn't scroll.
        if !may_cancel(emu, chosen, content)? {
            for &c in &pan.candidates {
                set_flag(emu, c, "tracking", false);
            }
            emu.scroll.pan = None;
            return Ok(false);
        }
        pan.view = chosen;
        // The first 10 pt only start the drag; the content follows from there.
        let len = dx.hypot(dy);
        pan.start_touch = [pan.start_touch[0] + dx / len * SLOP, pan.start_touch[1] + dy / len * SLOP];
        pan.start_offset = offset(emu, chosen);
        emu.scroll.pan = Some(pan.clone());
        set_flag(emu, chosen, "dragging", true);
        tell(emu, chosen, "scrollViewWillBeginDragging:", &[chosen])?;
        started = true;
    }
    let v = pan.view;
    emu.scroll.pan = Some(pan.clone());
    let ax = axes(emu, v)?;
    let (min, max) = range(emu, v)?;
    let mut o = offset(emu, v);
    for i in 0..2 {
        if ax[i] {
            let raw = pan.start_offset[i] - (at[i] - pan.start_touch[i]);
            o[i] = banded(raw, min[i], max[i]);
        }
    }
    set_offset(emu, v, o)?;
    Ok(started)
}

/// The finger's velocity over the last 100 ms, in pt/ms.
fn velocity(samples: &[(u64, [f64; 2])]) -> [f64; 2] {
    let Some(&(t1, p1)) = samples.last() else { return [0.0; 2] };
    let (t0, p0) = samples.iter().rev().find(|(t, _)| t1 - t >= 100_000_000).or(samples.first()).copied().unwrap_or((t1, p1));
    let dt = (t1 - t0) as f64 / 1e6;
    if dt <= 0.0 {
        return [0.0; 2];
    }
    [(p1[0] - p0[0]) / dt, (p1[1] - p0[1]) / dt]
}

/// Where content released at `offset` with velocity `v` (pt/ms) comes to rest.
pub(crate) fn projected(offset: f64, v: f64) -> f64 {
    offset + v * RATE / (1.0 - RATE)
}

/// The finger lifted (or the touch was cancelled).
pub(crate) fn touch_ended(emu: &mut Emu, at: [f64; 2], now: u64) -> Result<()> {
    let Some(mut pan) = emu.scroll.pan.take() else { return Ok(()) };
    for &c in &pan.candidates {
        set_flag(emu, c, "tracking", false);
    }
    let v = pan.view;
    if v == 0 {
        return Ok(());
    }
    pan.samples.push((now, at));
    let finger = velocity(&pan.samples);
    let ax = axes(emu, v)?;
    // The content moves against the finger.
    let vel = [if ax[0] { -finger[0] } else { 0.0 }, if ax[1] { -finger[1] } else { 0.0 }];
    let (min, max) = range(emu, v)?;
    let o = offset(emu, v);
    let mut target = [0.0; 2];
    for i in 0..2 {
        let outside = o[i] < min[i] || o[i] > max[i];
        target[i] = if outside { o[i].clamp(min[i], max[i]) } else { projected(o[i], vel[i]).clamp(min[i], max[i]) };
    }
    // The delegate may move the target (snapping): CGPoint *targetContentOffset.
    let p = emu.alloc(16)?;
    emu.write_u64(p, target[0].to_bits())?;
    emu.write_u64(p + 8, target[1].to_bits())?;
    let delegate = objc::bag_get(emu, v, "delegate").unwrap_or(0);
    if delegate_responds(emu, delegate, "scrollViewWillEndDragging:withVelocity:targetContentOffset:")? {
        objc::send_fp(emu, delegate, "scrollViewWillEndDragging:withVelocity:targetContentOffset:", &[v, p], &vel)?;
        target = [f64::from_bits(emu.read_u64(p)?), f64::from_bits(emu.read_u64(p + 8)?)];
    }
    let decelerate = (target[0] - o[0]).abs() > 0.5 || (target[1] - o[1]).abs() > 0.5;
    set_flag(emu, v, "dragging", false);
    tell(emu, v, "scrollViewDidEndDragging:willDecelerate:", &[v, decelerate as u64])?;
    if decelerate {
        set_flag(emu, v, "decelerating", true);
        tell(emu, v, "scrollViewWillBeginDecelerating:", &[v])?;
        glide(emu, v, o, target)?;
    }
    Ok(())
}

// ---- deceleration ----

fn glide(emu: &mut Emu, v: u64, from: [f64; 2], to: [f64; 2]) -> Result<()> {
    emu.scroll.generation += 1;
    let g = Glide { view: v, from, to, start_ns: crate::threads::now_ns(emu), generation: emu.scroll.generation };
    emu.scroll.glide = Some(g);
    crate::uikit::after_on_main(emu, FRAME_S, glide_frame, g.generation)
}

/// Where a glide is after `ms`: exponential ease-out at the deceleration rate.
pub(crate) fn glide_at(from: f64, to: f64, ms: f64) -> f64 {
    to - (to - from) * RATE.powf(ms)
}

fn glide_frame(emu: &mut Emu) -> Result<()> {
    let generation = emu.arg(1);
    let Some(g) = emu.scroll.glide.filter(|g| g.generation == generation) else { return Ok(()) };
    let ms = (crate::threads::now_ns(emu) - g.start_ns) as f64 / 1e6;
    let mut o = [0.0; 2];
    let mut done = true;
    for i in 0..2 {
        o[i] = glide_at(g.from[i], g.to[i], ms);
        if (g.to[i] - o[i]).abs() > 0.5 {
            done = false;
        } else {
            o[i] = g.to[i];
        }
    }
    set_offset(emu, g.view, o)?;
    if done {
        emu.scroll.glide = None;
        set_flag(emu, g.view, "decelerating", false);
        tell(emu, g.view, "scrollViewDidEndDecelerating:", &[g.view])?;
    } else {
        crate::uikit::after_on_main(emu, FRAME_S, glide_frame, generation)?;
    }
    Ok(())
}

// ---- methods ----

fn set_offset_msg(emu: &mut Emu) -> Result<()> {
    let (v, x, y) = (emu.arg(0), d(emu, 0), d(emu, 1));
    if emu.scroll.glide.is_some_and(|g| g.view == v) {
        emu.scroll.glide = None;
        emu.scroll.generation += 1;
        set_flag(emu, v, "decelerating", false);
    }
    set_offset(emu, v, [x, y])
}

const SV: &str = "UIScrollView";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (SV, false, "delegate", |e| {
        let d = objc::bag_get(e, e.arg(0), "delegate").unwrap_or(0);
        ret(e, d)
    }),
    (SV, false, "setDelegate:", |e| {
        let (v, d) = (e.arg(0), e.arg(2));
        objc::bag_set(e, v, "delegate", d);
        Ok(())
    }),
    (SV, false, "contentOffset", |e| {
        let o = offset(e, e.arg(0));
        ret_rect(e, [o[0], o[1], 0.0, 0.0]);
        Ok(())
    }),
    (SV, false, "setContentOffset:", set_offset_msg),
    // Animated or not, the content moves at once.
    (SV, false, "setContentOffset:animated:", set_offset_msg),
    (SV, false, "contentSize", |e| {
        let v = e.arg(0);
        let s = content_size(e, v)?;
        ret_rect(e, [s[0], s[1], 0.0, 0.0]);
        Ok(())
    }),
    (SV, false, "setContentSize:", |e| {
        let (v, w, h) = (e.arg(0), d(e, 0), d(e, 1));
        set_bag_f64(e, v, "contentWidth", w);
        set_bag_f64(e, v, "contentHeight", h);
        Ok(())
    }),
    (SV, false, "contentInset", |e| {
        let r = content_inset(e, e.arg(0));
        ret_rect(e, r);
        Ok(())
    }),
    (SV, false, "setContentInset:", |e| {
        let v = e.arg(0);
        let r = [0, 1, 2, 3].map(|i| d(e, i));
        for (k, x) in INSET_KEYS.iter().zip(r) {
            set_bag_f64(e, v, k, x);
        }
        Ok(())
    }),
    // No automatic adjustment for the safe area: the insets the app set.
    (SV, false, "adjustedContentInset", |e| {
        let r = content_inset(e, e.arg(0));
        ret_rect(e, r);
        Ok(())
    }),
    (SV, false, "isScrollEnabled", |e| {
        let v = scroll_enabled(e, e.arg(0)) as u64;
        ret(e, v)
    }),
    (SV, false, "delaysContentTouches", |e| {
        let v = objc::bag_get(e, e.arg(0), "delaysContentTouches").is_none_or(|x| x & 0xff != 0) as u64;
        ret(e, v)
    }),
    (SV, false, "setDelaysContentTouches:", |e| {
        let (v, x) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, v, "delaysContentTouches", x);
        Ok(())
    }),
    (SV, false, "canCancelContentTouches", |e| {
        let v = objc::bag_get(e, e.arg(0), "canCancelContentTouches").is_none_or(|x| x & 0xff != 0) as u64;
        ret(e, v)
    }),
    (SV, false, "setCanCancelContentTouches:", |e| {
        let (v, x) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, v, "canCancelContentTouches", x);
        Ok(())
    }),
    (SV, false, "touchesShouldBegin:withEvent:inContentView:", |e| ret(e, 1)),
    // A control keeps its touch (dragging off a button doesn't scroll)…
    (SV, false, "touchesShouldCancelInContentView:", |e| {
        let cls = host_class(e, "UIControl")?;
        let control = objc::is_kind_of(e, e.arg(2), cls)?;
        ret(e, !control as u64)
    }),
    // …except in lists, which scroll from anywhere.
    ("UITableView", false, "touchesShouldCancelInContentView:", |e| ret(e, 1)),
    ("UICollectionView", false, "touchesShouldCancelInContentView:", |e| ret(e, 1)),
    (SV, false, "isTracking", |e| {
        let v = flag(e, e.arg(0), "tracking") as u64;
        ret(e, v)
    }),
    (SV, false, "isDragging", |e| {
        let v = flag(e, e.arg(0), "dragging") as u64;
        ret(e, v)
    }),
    (SV, false, "isDecelerating", |e| {
        let v = flag(e, e.arg(0), "decelerating") as u64;
        ret(e, v)
    }),
    (SV, false, "zoomScale", |e| {
        crate::uikit::ret_f64(e, 1.0);
        Ok(())
    }),
    (SV, false, "minimumZoomScale", |e| {
        crate::uikit::ret_f64(e, 1.0);
        Ok(())
    }),
    (SV, false, "maximumZoomScale", |e| {
        crate::uikit::ret_f64(e, 1.0);
        Ok(())
    }),
    (SV, false, "decelerationRate", |e| {
        crate::uikit::ret_f64(e, RATE);
        Ok(())
    }),
    // The pan recognizer apps configure (it isn't what moves the content here).
    (SV, false, "panGestureRecognizer", |e| {
        let v = e.arg(0);
        if let Some(r) = objc::bag_get(e, v, "pomme.pan").filter(|&r| r != 0) {
            return ret(e, r);
        }
        let cls = host_class(e, "UIPanGestureRecognizer")?;
        let r = send(e, cls, "alloc", &[])?;
        let r = send(e, r, "init", &[])?;
        objc::bag_set(e, v, "pomme.pan", r);
        ret(e, r)
    }),
    // Scroll the least so the rect (in content coordinates) is in sight.
    (SV, false, "scrollRectToVisible:animated:", |e| {
        let v = e.arg(0);
        let r = [0, 1, 2, 3].map(|i| d(e, i));
        let b = view(e, v).bounds;
        let (min, max) = range(e, v)?;
        let mut o = [b[0], b[1]];
        for i in 0..2 {
            let (lo, len, size) = (r[i], r[i + 2], b[i + 2]);
            if lo + len > o[i] + size {
                o[i] = lo + len - size;
            }
            if lo < o[i] {
                o[i] = lo;
            }
            o[i] = o[i].clamp(min[i], max[i]);
        }
        set_offset(e, v, o)
    }),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_projects_like_uikit() {
        // 1 pt/ms (1000 pt/s) carries the content 499 pt further.
        assert!((projected(0.0, 1.0) - 499.0).abs() < 1e-9);
        assert_eq!(projected(100.0, 0.0), 100.0);
    }

    #[test]
    fn glides_ease_out_to_the_target() {
        assert_eq!(glide_at(0.0, 500.0, 0.0), 0.0);
        // Released at the projection's speed: 1 pt/ms at first.
        assert!((glide_at(0.0, 500.0, 1.0) - 1.0).abs() < 0.01);
        assert!(glide_at(0.0, 500.0, 3000.0) > 498.0);
    }

    #[test]
    fn rubber_band() {
        assert_eq!(banded(-40.0, 0.0, 100.0), -20.0);
        assert_eq!(banded(50.0, 0.0, 100.0), 50.0);
        assert_eq!(banded(140.0, 0.0, 100.0), 120.0);
    }

    #[test]
    fn velocity_over_the_last_100ms() {
        let s = [(0, [0.0, 0.0]), (100_000_000, [0.0, -50.0]), (200_000_000, [0.0, -150.0])];
        assert_eq!(velocity(&s), [0.0, -1.0]);
    }
}

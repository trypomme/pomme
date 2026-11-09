//! The controls that hold a value: UISwitch, UISlider and UISegmentedControl.
//!
//! Each takes part in UIControl's touch tracking (control.rs) the way UIKit's
//! do, by overriding `beginTrackingWithTouch:withEvent:` and friends, and
//! sends ValueChanged when the person changes it (never when the app sets it):
//! - a switch flips when a touch ends on it, or, dragged, goes to the side the
//!   finger ended on; it is always 51×31;
//! - a slider moves only when the touch starts on its thumb, then follows the
//!   finger (keeping where on the thumb it was held), sending ValueChanged as
//!   it goes (or once at the end if not `continuous`);
//! - a segmented control selects the segment a touch ends on (a momentary one
//!   reports it and lets go).
//!
//! They draw with subviews, as UIKit's are built: a switch's track and thumb,
//! a slider's two track halves and thumb, a segmented control's selection
//! indicator and title labels. The parts ignore touches, so the control is hit.
//! Sizes and colors follow iOS 13+ (where UIKit's exact numbers aren't
//! public, they're close: noted where so).

use crate::control::{self, PRIMARY_ACTION_TRIGGERED};
use crate::foundation;
use crate::objc::{self, host_class, send};
use crate::uiview::{set_frame_of, view, view_mut};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

pub const VALUE_CHANGED: u64 = 1 << 12;

const SYSTEM_BLUE: [f64; 4] = [0.0, 0.478, 1.0, 1.0];
const SYSTEM_GREEN: [f64; 4] = [0.204, 0.780, 0.349, 1.0];
const WHITE: [f64; 4] = [1.0, 1.0, 1.0, 1.0];
const LABEL: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
/// systemFill-like grays (#787880 / #767680 at low alpha).
const FILL: [f64; 4] = [0.471, 0.471, 0.502, 0.16];
const TRACK_FILL: [f64; 4] = [0.471, 0.471, 0.502, 0.2];
const SEGMENT_FILL: [f64; 4] = [0.463, 0.463, 0.502, 0.12];
/// The ring that stands in for a thumb's shadow.
const RING: [f64; 4] = [0.0, 0.0, 0.0, 0.08];

#[derive(Clone, Debug, Default)]
struct Segment {
    title: u64,
    enabled: bool,
    /// 0: sized automatically.
    width: f64,
    label: u64,
}

#[derive(Default)]
pub struct State {
    segments: HashMap<u64, Vec<Segment>>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_size(emu: &mut Emu, w: f64, h: f64) -> Result<()> {
    crate::uikit::ret_rect(emu, [w, h, 0.0, 0.0]);
    Ok(())
}

fn rect_arg(emu: &Emu) -> [f64; 4] {
    [0, 1, 2, 3].map(|i| f64::from_bits(emu.cpu.q[i] as u64))
}

fn f32_arg(emu: &Emu, i: usize) -> f32 {
    f32::from_bits(emu.cpu.q[i] as u32)
}

fn ret_f32(emu: &mut Emu, v: f32) -> Result<()> {
    emu.cpu.q[0] = v.to_bits() as u128;
    Ok(())
}

fn flag(emu: &Emu, v: u64, key: &str, default: bool) -> bool {
    objc::bag_get(emu, v, key).map_or(default, |x| x & 0xff != 0)
}

fn color(emu: &mut Emu, v: u64, key: &str, default: [f64; 4]) -> Result<u64> {
    match objc::bag_get(emu, v, key).filter(|&c| c != 0) {
        Some(c) => Ok(c),
        None => crate::uiview::new_color(emu, default),
    }
}

/// Store a retained object property and redraw.
fn set_object(emu: &mut Emu, key: &str, redraw: fn(&mut Emu, u64) -> Result<()>) -> Result<()> {
    let (v, x) = (emu.arg(0), emu.arg(2));
    if x != 0 {
        send(emu, x, "retain", &[])?;
    }
    objc::bag_set(emu, v, key, x);
    redraw(emu, v)
}

/// One of a control's parts: a plain view (made on first use) that ignores touches.
fn part(emu: &mut Emu, parent: u64, key: &str) -> Result<u64> {
    let key = format!("pomme.part.{key}");
    if let Some(p) = objc::bag_get(emu, parent, &key).filter(|&p| p != 0) {
        return Ok(p);
    }
    let cls = host_class(emu, "UIView")?;
    let p = send(emu, cls, "alloc", &[])?;
    emu.cpu.q[..4].fill(0);
    let p = send(emu, p, "initWithFrame:", &[])?;
    view_mut(emu, p).user_interaction = false;
    objc::bag_set(emu, parent, &key, p);
    send(emu, parent, "addSubview:", &[p])?;
    Ok(p)
}

/// Place and paint a part: its frame, fill, and corner radius.
fn shape(emu: &mut Emu, p: u64, frame: [f64; 4], fill: u64, radius: f64) -> Result<()> {
    objc::send_fp(emu, p, "setFrame:", &[], &frame)?;
    send(emu, p, "setBackgroundColor:", &[fill])?;
    let layer = send(emu, p, "layer", &[])?;
    objc::send_fp(emu, layer, "setCornerRadius:", &[], &[radius])?;
    Ok(())
}

/// A round thumb: a faint ring with a white disc inside, `d` across.
fn thumb(emu: &mut Emu, parent: u64, key: &str, x: f64, y: f64, d: f64, fill: u64) -> Result<()> {
    let ring = part(emu, parent, key)?;
    let ring_color = crate::uiview::new_color(emu, RING)?;
    shape(emu, ring, [x, y, d, d], ring_color, d / 2.0)?;
    let disc = part(emu, ring, "disc")?;
    shape(emu, disc, [0.5, 0.5, d - 1.0, d - 1.0], fill, (d - 1.0) / 2.0)
}

fn touch_point(emu: &mut Emu, c: u64) -> Option<[f64; 2]> {
    let touch = emu.arg(2);
    crate::touch::location_in(emu, touch, c)
}

fn touch_inside(emu: &mut Emu, c: u64) -> Result<bool> {
    Ok(send(emu, c, "isTouchInside", &[])? & 0xff != 0)
}

fn changed(emu: &mut Emu, c: u64, extra: u64) -> Result<()> {
    control::send_actions(emu, c, VALUE_CHANGED | extra, 0)
}

// ---- UISwitch ----

const SWITCH_SIZE: [f64; 2] = [51.0, 31.0];
const SWITCH_THUMB: f64 = 27.0;
/// How far a finger moves on a switch before it's a drag rather than a tap.
const SWITCH_DRAG: f64 = 10.0;

fn is_on(emu: &Emu, s: u64) -> bool {
    flag(emu, s, "on", false)
}

fn draw_switch(emu: &mut Emu, s: u64) -> Result<()> {
    let on = is_on(emu, s);
    let track = part(emu, s, "track")?;
    let fill = if on { color(emu, s, "onTintColor", SYSTEM_GREEN)? } else { crate::uiview::new_color(emu, FILL)? };
    shape(emu, track, [0.0, 0.0, SWITCH_SIZE[0], SWITCH_SIZE[1]], fill, SWITCH_SIZE[1] / 2.0)?;
    let x = if on { SWITCH_SIZE[0] - 2.0 - SWITCH_THUMB } else { 2.0 };
    let thumb_fill = color(emu, s, "thumbTintColor", WHITE)?;
    thumb(emu, s, "thumb", x, 2.0, SWITCH_THUMB, thumb_fill)?;
    crate::host::frame_changed(emu);
    Ok(())
}

/// A switch is always its own size, wherever it's put.
fn switch_set_frame(emu: &mut Emu) -> Result<()> {
    let (s, r) = (emu.arg(0), rect_arg(emu));
    set_frame_of(emu, s, [r[0], r[1], SWITCH_SIZE[0], SWITCH_SIZE[1]]);
    Ok(())
}

fn switch_init(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    switch_set_frame(emu)?;
    draw_switch(emu, s)?;
    ret(emu, s)
}

/// How long a switch takes to flip when animated.
const SWITCH_FLIP: f64 = 0.25;

/// `setOn:` (at once) and `setOn:animated:`: animated, the thumb slides and
/// the track's colour fades, as UIKit's switch does.
fn switch_set_on(emu: &mut Emu, animated: bool) -> Result<()> {
    let (s, on) = (emu.arg(0), emu.arg(2) & 0xff);
    let changed = (on != 0) != is_on(emu, s);
    objc::bag_set(emu, s, "on", on);
    if animated && changed {
        let timing = crate::animation::named_timing("easeInEaseOut");
        crate::animation::in_block(emu, 0.0, SWITCH_FLIP, timing, 0, |e| draw_switch(e, s))
    } else {
        draw_switch(emu, s)
    }
}

fn switch_begin(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let x = touch_point(emu, s).map_or(0.0, |p| p[0]);
    objc::bag_set(emu, s, "pomme.trackStart", x.to_bits());
    ret(emu, 1)
}

/// The touch ended: a tap flips the switch, a drag leaves it on the side the finger is.
fn switch_end(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    if !touch_inside(emu, s)? {
        return Ok(());
    }
    let Some(p) = touch_point(emu, s) else { return Ok(()) };
    let start = objc::bag_get(emu, s, "pomme.trackStart").map_or(p[0], f64::from_bits);
    let was = is_on(emu, s);
    let on = if (p[0] - start).abs() < SWITCH_DRAG { !was } else { p[0] > SWITCH_SIZE[0] / 2.0 };
    if on != was {
        send(emu, s, "setOn:animated:", &[on as u64, 1])?;
        changed(emu, s, PRIMARY_ACTION_TRIGGERED)?;
    }
    Ok(())
}

// ---- UISlider ----

const SLIDER_HEIGHT: f64 = 31.0;
const SLIDER_THUMB: f64 = 28.0;
const SLIDER_TRACK: f64 = 4.0;
/// How far around the thumb a touch still picks it up.
const THUMB_SLOP: f64 = 10.0;

fn slider_f32(emu: &Emu, s: u64, key: &str, default: f32) -> f32 {
    objc::bag_get(emu, s, key).map_or(default, |x| f32::from_bits(x as u32))
}

fn set_slider_f32(emu: &mut Emu, s: u64, key: &str, v: f32) {
    objc::bag_set(emu, s, key, v.to_bits() as u64);
}

/// (value, minimum, maximum)
fn slider_values(emu: &Emu, s: u64) -> (f32, f32, f32) {
    (slider_f32(emu, s, "value", 0.0), slider_f32(emu, s, "minimumValue", 0.0), slider_f32(emu, s, "maximumValue", 1.0))
}

/// The value within the range (a degenerate range pins it to the minimum).
fn clamp(v: f32, min: f32, max: f32) -> f32 {
    if max <= min { min } else { v.clamp(min, max) }
}

fn fraction(emu: &Emu, s: u64) -> f64 {
    let (v, min, max) = slider_values(emu, s);
    if max <= min { 0.0 } else { ((v - min) / (max - min)) as f64 }
}

/// `trackRectForBounds:`: a 4-pt bar along the middle.
fn track_rect(b: [f64; 4]) -> [f64; 4] {
    [b[0], b[1] + (b[3] - SLIDER_TRACK) / 2.0, b[2], SLIDER_TRACK]
}

/// `thumbRectForBounds:trackRect:value:`: the thumb travels the track, flush with its ends.
fn thumb_rect(b: [f64; 4], track: [f64; 4], fraction: f64) -> [f64; 4] {
    let x = track[0] + (track[2] - SLIDER_THUMB).max(0.0) * fraction.clamp(0.0, 1.0);
    [x, b[1] + (b[3] - SLIDER_THUMB) / 2.0, SLIDER_THUMB, SLIDER_THUMB]
}

/// The track as the slider (or a subclass) says, and the thumb on it.
fn slider_geometry(emu: &mut Emu, s: u64) -> Result<([f64; 4], [f64; 4])> {
    let b = view(emu, s).bounds;
    let b = [0.0, 0.0, b[2], b[3]];
    let t = objc::send_fp(emu, s, "trackRectForBounds:", &[], &b)?;
    let f = fraction(emu, s);
    Ok((t, thumb_rect(b, t, f)))
}

fn draw_slider(emu: &mut Emu, s: u64) -> Result<()> {
    let (t, th) = slider_geometry(emu, s)?;
    let mid = th[0] + th[2] / 2.0;
    let min_fill = color(emu, s, "minimumTrackTintColor", SYSTEM_BLUE)?;
    let max_fill = color(emu, s, "maximumTrackTintColor", TRACK_FILL)?;
    let lo = part(emu, s, "minimumTrack")?;
    shape(emu, lo, [t[0], t[1], (mid - t[0]).max(0.0), t[3]], min_fill, t[3] / 2.0)?;
    let hi = part(emu, s, "maximumTrack")?;
    shape(emu, hi, [mid, t[1], (t[0] + t[2] - mid).max(0.0), t[3]], max_fill, t[3] / 2.0)?;
    let thumb_fill = color(emu, s, "thumbTintColor", WHITE)?;
    thumb(emu, s, "thumb", th[0], th[1], th[2], thumb_fill)?;
    crate::host::frame_changed(emu);
    Ok(())
}

fn redraw_slider(emu: &mut Emu, s: u64) -> Result<()> {
    if objc::bag_get(emu, s, "pomme.part.thumb").is_some() { draw_slider(emu, s) } else { Ok(()) }
}

fn slider_set_value(emu: &mut Emu, s: u64, v: f32) -> Result<()> {
    let (_, min, max) = slider_values(emu, s);
    set_slider_f32(emu, s, "value", clamp(v, min, max));
    redraw_slider(emu, s)
}

/// A new end of the range: the value stays inside it.
fn slider_set_bound(emu: &mut Emu, key: &str) -> Result<()> {
    let (s, x) = (emu.arg(0), f32_arg(emu, 0));
    set_slider_f32(emu, s, key, x);
    let (v, _, _) = slider_values(emu, s);
    slider_set_value(emu, s, v)
}

fn slider_begin(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let Some(p) = touch_point(emu, s) else { return ret(emu, 0) };
    let (_, th) = slider_geometry(emu, s)?;
    let on_thumb = p[0] >= th[0] - THUMB_SLOP && p[0] <= th[0] + th[2] + THUMB_SLOP && p[1] >= th[1] - THUMB_SLOP && p[1] <= th[1] + th[3] + THUMB_SLOP;
    if !on_thumb {
        return ret(emu, 0);
    }
    // Where on the thumb the finger is, and the value when it started.
    objc::bag_set(emu, s, "pomme.grab", (p[0] - (th[0] + th[2] / 2.0)).to_bits());
    let (v, _, _) = slider_values(emu, s);
    set_slider_f32(emu, s, "pomme.startValue", v);
    ret(emu, 1)
}

fn slider_continue(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let Some(p) = touch_point(emu, s) else { return ret(emu, 1) };
    let (t, _) = slider_geometry(emu, s)?;
    let grab = objc::bag_get(emu, s, "pomme.grab").map_or(0.0, f64::from_bits);
    let travel = (t[2] - SLIDER_THUMB).max(0.0);
    let f = if travel > 0.0 { ((p[0] - grab - SLIDER_THUMB / 2.0 - t[0]) / travel).clamp(0.0, 1.0) } else { 0.0 };
    let (old, min, max) = slider_values(emu, s);
    let v = clamp(min + (max - min) * f as f32, min, max);
    if v != old {
        // Through the setter, so subclasses see it.
        emu.cpu.q[0] = v.to_bits() as u128;
        send(emu, s, "setValue:animated:", &[0])?;
        if flag(emu, s, "continuous", true) {
            changed(emu, s, 0)?;
        }
    }
    ret(emu, 1)
}

fn slider_end(emu: &mut Emu) -> Result<()> {
    let s = emu.arg(0);
    let (v, _, _) = slider_values(emu, s);
    if !flag(emu, s, "continuous", true) && v != slider_f32(emu, s, "pomme.startValue", v) {
        changed(emu, s, 0)?;
    }
    Ok(())
}

// ---- UISegmentedControl ----

/// UISegmentedControlNoSegment
const NO_SEGMENT: i64 = -1;
const SEGMENTED_HEIGHT: f64 = 32.0;
const SEGMENT_FONT: f64 = 13.0;
/// Room either side of the widest title in each segment (close to UIKit's).
const SEGMENT_PAD: f64 = 12.0;
const SEGMENTED_RADIUS: f64 = 9.0;
const INDICATOR_INSET: f64 = 2.0;
const SEMIBOLD: f64 = 0.3;

fn segments(emu: &Emu, c: u64) -> Vec<Segment> {
    emu.values.segments.get(&c).cloned().unwrap_or_default()
}

fn selected(emu: &Emu, c: u64) -> i64 {
    objc::bag_get(emu, c, "selectedSegmentIndex").map_or(NO_SEGMENT, |x| x as i64)
}

fn title_width(emu: &mut Emu, title: u64, weight: f64) -> Result<f64> {
    if title == 0 {
        return Ok(0.0);
    }
    let text = foundation::nsstring(emu, title)?;
    let (_, w) = crate::textkit::line_glyphs(emu, &text, SEGMENT_FONT, weight, LABEL, 0.0, 0.0);
    Ok(w.ceil())
}

/// The width of a segment that sizes itself: the widest title (as it looks
/// selected, so selecting doesn't change sizes) and padding.
fn auto_width(emu: &mut Emu, segs: &[Segment]) -> Result<f64> {
    let mut w: f64 = 0.0;
    for s in segs {
        w = w.max(title_width(emu, s.title, SEMIBOLD)?);
    }
    Ok(w + 2.0 * SEGMENT_PAD)
}

fn segmented_size(emu: &mut Emu, c: u64) -> Result<[f64; 2]> {
    let segs = segments(emu, c);
    let auto = auto_width(emu, &segs)?;
    let w = segs.iter().map(|s| if s.width > 0.0 { s.width } else { auto }).sum();
    Ok([w, SEGMENTED_HEIGHT])
}

/// Segments' x ranges across `width`: fixed widths as set, the rest share what's left.
fn segment_spans(segs: &[Segment], width: f64) -> Vec<(f64, f64)> {
    let fixed: f64 = segs.iter().map(|s| s.width).sum();
    let autos = segs.iter().filter(|s| s.width <= 0.0).count();
    let share = if autos > 0 { ((width - fixed) / autos as f64).max(0.0) } else { 0.0 };
    let mut x = 0.0;
    segs.iter()
        .map(|s| {
            let w = if s.width > 0.0 { s.width } else { share };
            let span = (x, w);
            x += w;
            span
        })
        .collect()
}

fn draw_segmented(emu: &mut Emu, c: u64) -> Result<()> {
    let b = view(emu, c).bounds;
    let segs = segments(emu, c);
    let spans = segment_spans(&segs, b[2]);
    let bg = part(emu, c, "background")?;
    let fill = crate::uiview::new_color(emu, SEGMENT_FILL)?;
    shape(emu, bg, [0.0, 0.0, b[2], b[3]], fill, SEGMENTED_RADIUS)?;
    let sel = selected(emu, c);
    let indicator = part(emu, c, "indicator")?;
    match spans.get(sel.max(0) as usize).filter(|_| sel >= 0) {
        Some(&(x, w)) => {
            let fill = color(emu, c, "selectedSegmentTintColor", WHITE)?;
            let i = INDICATOR_INSET;
            shape(emu, indicator, [x + i, i, (w - 2.0 * i).max(0.0), (b[3] - 2.0 * i).max(0.0)], fill, SEGMENTED_RADIUS - i)?;
            view_mut(emu, indicator).hidden = false;
        }
        None => view_mut(emu, indicator).hidden = true,
    }
    let enabled = send(emu, c, "isEnabled", &[])? & 0xff != 0;
    for (i, (seg, &(x, w))) in segs.iter().zip(&spans).enumerate() {
        let l = seg.label;
        if l == 0 {
            continue;
        }
        let weight = if i as i64 == sel { SEMIBOLD } else { 0.0 };
        let font = crate::uiview::new_font(emu, if weight > 0.0 { ".SFUI-Semibold" } else { ".SFUI-Regular" }, SEGMENT_FONT, weight)?;
        objc::bag_set(emu, l, "font", font);
        let dim = if enabled && seg.enabled { 1.0 } else { 0.3 };
        let text_color = crate::uiview::new_color(emu, [LABEL[0], LABEL[1], LABEL[2], dim])?;
        objc::bag_set(emu, l, "textColor", text_color);
        send(emu, l, "setText:", &[seg.title])?;
        objc::send_fp(emu, l, "setFrame:", &[], &[x, 0.0, w, b[3]])?;
    }
    crate::host::frame_changed(emu);
    Ok(())
}

fn new_segment(emu: &mut Emu, c: u64, title: u64) -> Result<Segment> {
    let cls = host_class(emu, "UILabel")?;
    let l = send(emu, cls, "alloc", &[])?;
    emu.cpu.q[..4].fill(0);
    let l = send(emu, l, "initWithFrame:", &[])?;
    objc::bag_set(emu, l, "textAlignment", 1);
    send(emu, c, "addSubview:", &[l])?;
    let title = if title != 0 && is_string(emu, title)? { send(emu, title, "copy", &[])? } else { 0 };
    Ok(Segment { title, enabled: true, width: 0.0, label: l })
}

/// Items are titles (NSString) or images (drawn as nothing here).
fn is_string(emu: &mut Emu, x: u64) -> Result<bool> {
    let cls = host_class(emu, "NSString")?;
    objc::is_kind_of(emu, x, cls)
}

fn insert_segment(emu: &mut Emu, c: u64, title: u64, at: usize) -> Result<()> {
    let seg = new_segment(emu, c, title)?;
    let list = emu.values.segments.entry(c).or_default();
    let at = at.min(list.len());
    list.insert(at, seg);
    // The selection stays on its segment.
    let sel = selected(emu, c);
    if sel >= at as i64 {
        objc::bag_set(emu, c, "selectedSegmentIndex", (sel + 1) as u64);
    }
    relayout(emu, c)
}

fn relayout(emu: &mut Emu, c: u64) -> Result<()> {
    send(emu, c, "invalidateIntrinsicContentSize", &[])?;
    send(emu, c, "setNeedsLayout", &[])?;
    draw_segmented(emu, c)
}

fn segmented_init_items(emu: &mut Emu) -> Result<()> {
    let (c, items) = (emu.arg(0), emu.arg(2));
    emu.cpu.q[..4].fill(0);
    let c = send(emu, c, "initWithFrame:", &[])?;
    let n = if items != 0 { send(emu, items, "count", &[])? } else { 0 };
    for i in 0..n {
        let item = send(emu, items, "objectAtIndex:", &[i])?;
        insert_segment(emu, c, item, i as usize)?;
    }
    // Its frame is its size.
    let size = segmented_size(emu, c)?;
    set_frame_of(emu, c, [0.0, 0.0, size[0], size[1]]);
    draw_segmented(emu, c)?;
    ret(emu, c)
}

fn segment_at(emu: &Emu, c: u64, i: u64) -> Option<Segment> {
    emu.values.segments.get(&c).and_then(|l| l.get(i as usize)).cloned()
}

fn with_segment(emu: &mut Emu, c: u64, i: u64, f: impl FnOnce(&mut Segment)) -> Result<()> {
    if let Some(s) = emu.values.segments.get_mut(&c).and_then(|l| l.get_mut(i as usize)) {
        f(s);
    }
    relayout(emu, c)
}

fn remove_segment(emu: &mut Emu, c: u64, i: usize) -> Result<()> {
    let Some(list) = emu.values.segments.get_mut(&c) else { return Ok(()) };
    if i >= list.len() {
        return Ok(());
    }
    let seg = list.remove(i);
    send(emu, seg.label, "removeFromSuperview", &[])?;
    let sel = selected(emu, c);
    if sel == i as i64 {
        objc::bag_set(emu, c, "selectedSegmentIndex", NO_SEGMENT as u64);
    } else if sel > i as i64 {
        objc::bag_set(emu, c, "selectedSegmentIndex", (sel - 1) as u64);
    }
    relayout(emu, c)
}

/// The touch ended: if on an enabled segment, select it (a momentary control
/// reports the segment and doesn't keep it).
fn segmented_end(emu: &mut Emu) -> Result<()> {
    let c = emu.arg(0);
    if !touch_inside(emu, c)? {
        return Ok(());
    }
    let Some(p) = touch_point(emu, c) else { return Ok(()) };
    let segs = segments(emu, c);
    let b = view(emu, c).bounds;
    let spans = segment_spans(&segs, b[2]);
    let Some(i) = spans.iter().position(|&(x, w)| p[0] >= x && p[0] < x + w).or_else(|| (!spans.is_empty()).then(|| if p[0] < 0.0 { 0 } else { spans.len() - 1 })) else {
        return Ok(());
    };
    if !segs[i].enabled {
        return Ok(());
    }
    let momentary = flag(emu, c, "momentary", false);
    if momentary || selected(emu, c) != i as i64 {
        send(emu, c, "setSelectedSegmentIndex:", &[i as u64])?;
        changed(emu, c, 0)?;
        if momentary {
            send(emu, c, "setSelectedSegmentIndex:", &[NO_SEGMENT as u64])?;
        }
    }
    Ok(())
}

const SW: &str = "UISwitch";
const SL: &str = "UISlider";
const SG: &str = "UISegmentedControl";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // UISwitch
    (SW, false, "initWithFrame:", switch_init),
    (SW, false, "setFrame:", switch_set_frame),
    (SW, false, "sizeThatFits:", |e| ret_size(e, SWITCH_SIZE[0], SWITCH_SIZE[1])),
    (SW, false, "intrinsicContentSize", |e| ret_size(e, SWITCH_SIZE[0], SWITCH_SIZE[1])),
    (SW, false, "isOn", |e| {
        let v = is_on(e, e.arg(0)) as u64;
        ret(e, v)
    }),
    (SW, false, "setOn:", |e| switch_set_on(e, false)),
    (SW, false, "setOn:animated:", |e| {
        let animated = e.arg(3) & 0xff != 0;
        switch_set_on(e, animated)
    }),
    (SW, false, "onTintColor", |e| {
        let v = objc::bag_get(e, e.arg(0), "onTintColor").unwrap_or(0);
        ret(e, v)
    }),
    (SW, false, "setOnTintColor:", |e| set_object(e, "onTintColor", draw_switch)),
    (SW, false, "thumbTintColor", |e| {
        let v = objc::bag_get(e, e.arg(0), "thumbTintColor").unwrap_or(0);
        ret(e, v)
    }),
    (SW, false, "setThumbTintColor:", |e| set_object(e, "thumbTintColor", draw_switch)),
    (SW, false, "beginTrackingWithTouch:withEvent:", switch_begin),
    (SW, false, "endTrackingWithTouch:withEvent:", switch_end),
    // UISlider
    (SL, false, "value", |e| {
        let v = slider_values(e, e.arg(0)).0;
        ret_f32(e, v)
    }),
    (SL, false, "setValue:", |e| {
        let (s, v) = (e.arg(0), f32_arg(e, 0));
        slider_set_value(e, s, v)
    }),
    (SL, false, "setValue:animated:", |e| {
        let (s, v) = (e.arg(0), f32_arg(e, 0));
        slider_set_value(e, s, v)
    }),
    (SL, false, "minimumValue", |e| {
        let v = slider_values(e, e.arg(0)).1;
        ret_f32(e, v)
    }),
    (SL, false, "setMinimumValue:", |e| slider_set_bound(e, "minimumValue")),
    (SL, false, "maximumValue", |e| {
        let v = slider_values(e, e.arg(0)).2;
        ret_f32(e, v)
    }),
    (SL, false, "setMaximumValue:", |e| slider_set_bound(e, "maximumValue")),
    (SL, false, "isContinuous", |e| {
        let v = flag(e, e.arg(0), "continuous", true) as u64;
        ret(e, v)
    }),
    (SL, false, "setContinuous:", |e| {
        let (s, x) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, s, "continuous", x);
        Ok(())
    }),
    (SL, false, "trackRectForBounds:", |e| {
        let r = track_rect(rect_arg(e));
        crate::uikit::ret_rect(e, r);
        Ok(())
    }),
    // bounds d0-d3, track d4-d7, the value (a float) on the stack.
    (SL, false, "thumbRectForBounds:trackRect:value:", |e| {
        let b = rect_arg(e);
        let t = [4, 5, 6, 7].map(|i| f64::from_bits(e.cpu.q[i] as u64));
        let v = f32::from_bits(e.read_u64(e.cpu.sp)? as u32);
        let (_, min, max) = slider_values(e, e.arg(0));
        let f = if max <= min { 0.0 } else { ((clamp(v, min, max) - min) / (max - min)) as f64 };
        crate::uikit::ret_rect(e, thumb_rect(b, t, f));
        Ok(())
    }),
    (SL, false, "sizeThatFits:", |e| {
        let w = view(e, e.arg(0)).bounds[2];
        ret_size(e, w, SLIDER_HEIGHT)
    }),
    (SL, false, "intrinsicContentSize", |e| ret_size(e, -1.0, SLIDER_HEIGHT)),
    (SL, false, "layoutSubviews", |e| {
        let s = e.arg(0);
        draw_slider(e, s)
    }),
    (SL, false, "minimumTrackTintColor", |e| {
        let v = objc::bag_get(e, e.arg(0), "minimumTrackTintColor").unwrap_or(0);
        ret(e, v)
    }),
    (SL, false, "setMinimumTrackTintColor:", |e| set_object(e, "minimumTrackTintColor", redraw_slider)),
    (SL, false, "maximumTrackTintColor", |e| {
        let v = objc::bag_get(e, e.arg(0), "maximumTrackTintColor").unwrap_or(0);
        ret(e, v)
    }),
    (SL, false, "setMaximumTrackTintColor:", |e| set_object(e, "maximumTrackTintColor", redraw_slider)),
    (SL, false, "thumbTintColor", |e| {
        let v = objc::bag_get(e, e.arg(0), "thumbTintColor").unwrap_or(0);
        ret(e, v)
    }),
    (SL, false, "setThumbTintColor:", |e| set_object(e, "thumbTintColor", redraw_slider)),
    (SL, false, "beginTrackingWithTouch:withEvent:", slider_begin),
    (SL, false, "continueTrackingWithTouch:withEvent:", slider_continue),
    (SL, false, "endTrackingWithTouch:withEvent:", slider_end),
    // UISegmentedControl
    (SG, false, "initWithItems:", segmented_init_items),
    (SG, false, "numberOfSegments", |e| {
        let n = segments(e, e.arg(0)).len() as u64;
        ret(e, n)
    }),
    (SG, false, "titleForSegmentAtIndex:", |e| {
        let t = segment_at(e, e.arg(0), e.arg(2)).map_or(0, |s| s.title);
        ret(e, t)
    }),
    (SG, false, "setTitle:forSegmentAtIndex:", |e| {
        let (c, t, i) = (e.arg(0), e.arg(2), e.arg(3));
        let t = if t != 0 { send(e, t, "copy", &[])? } else { 0 };
        with_segment(e, c, i, |s| s.title = t)
    }),
    (SG, false, "insertSegmentWithTitle:atIndex:animated:", |e| {
        let (c, t, i) = (e.arg(0), e.arg(2), e.arg(3));
        insert_segment(e, c, t, i as usize)
    }),
    (SG, false, "removeSegmentAtIndex:animated:", |e| {
        let (c, i) = (e.arg(0), e.arg(2));
        remove_segment(e, c, i as usize)
    }),
    (SG, false, "removeAllSegments", |e| {
        let c = e.arg(0);
        while !segments(e, c).is_empty() {
            remove_segment(e, c, 0)?;
        }
        Ok(())
    }),
    (SG, false, "selectedSegmentIndex", |e| {
        let v = selected(e, e.arg(0));
        ret(e, v as u64)
    }),
    (SG, false, "setSelectedSegmentIndex:", |e| {
        let (c, i) = (e.arg(0), e.arg(2) as i64);
        let n = segments(e, c).len() as i64;
        let i = if (0..n).contains(&i) { i } else { NO_SEGMENT };
        objc::bag_set(e, c, "selectedSegmentIndex", i as u64);
        draw_segmented(e, c)
    }),
    (SG, false, "isMomentary", |e| {
        let v = flag(e, e.arg(0), "momentary", false) as u64;
        ret(e, v)
    }),
    (SG, false, "setMomentary:", |e| {
        let (c, x) = (e.arg(0), e.arg(2) & 0xff);
        objc::bag_set(e, c, "momentary", x);
        Ok(())
    }),
    (SG, false, "setEnabled:forSegmentAtIndex:", |e| {
        let (c, on, i) = (e.arg(0), e.arg(2) & 0xff != 0, e.arg(3));
        with_segment(e, c, i, |s| s.enabled = on)
    }),
    (SG, false, "isEnabledForSegmentAtIndex:", |e| {
        let v = segment_at(e, e.arg(0), e.arg(2)).is_some_and(|s| s.enabled) as u64;
        ret(e, v)
    }),
    (SG, false, "setWidth:forSegmentAtIndex:", |e| {
        let (c, w, i) = (e.arg(0), f64::from_bits(e.cpu.q[0] as u64), e.arg(2));
        with_segment(e, c, i, |s| s.width = w.max(0.0))
    }),
    (SG, false, "widthForSegmentAtIndex:", |e| {
        let w = segment_at(e, e.arg(0), e.arg(2)).map_or(0.0, |s| s.width);
        crate::uikit::ret_f64(e, w);
        Ok(())
    }),
    (SG, false, "selectedSegmentTintColor", |e| {
        let v = objc::bag_get(e, e.arg(0), "selectedSegmentTintColor").unwrap_or(0);
        ret(e, v)
    }),
    (SG, false, "setSelectedSegmentTintColor:", |e| set_object(e, "selectedSegmentTintColor", draw_segmented)),
    (SG, false, "sizeThatFits:", |e| {
        let s = segmented_size(e, e.arg(0))?;
        ret_size(e, s[0], s[1])
    }),
    (SG, false, "intrinsicContentSize", |e| {
        let s = segmented_size(e, e.arg(0))?;
        ret_size(e, s[0], s[1])
    }),
    (SG, false, "layoutSubviews", |e| {
        let c = e.arg(0);
        draw_segmented(e, c)
    }),
    (SG, false, "endTrackingWithTouch:withEvent:", segmented_end),
];

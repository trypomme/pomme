//! UIControl: target-action and touch tracking, and UIButton.
//!
//! Targets are registered per control event (`addTarget:action:forControlEvents:`),
//! as are UIActions (`addAction:forControlEvents:`, a button's `primaryAction`).
//! A control fires a target through `-sendAction:to:forEvent:`, which hands
//! it to `-[UIApplication sendAction:to:from:forEvent:]`; a nil target there
//! goes up the responder chain (from the first responder, else from the
//! control) to the first responder that can perform the action. A UIAction
//! goes through `-sendAction:`, which calls its handler with `sender` set.
//!
//! Touches follow UIKit's UIControl: `touchesBegan` marks the touch inside,
//! asks `beginTrackingWithTouch:withEvent:` whether to track, highlights and
//! sends TouchDown; `touchesMoved` re-measures `touchInside` (within 70 pt of
//! the bounds, UIKit's slop), highlights accordingly, asks
//! `continueTrackingWithTouch:withEvent:` and sends TouchDragInside/Outside
//! (with DragEnter/Exit on crossing); `touchesEnded` unhighlights, calls
//! `endTrackingWithTouch:withEvent:` and sends TouchUpInside or
//! TouchUpOutside; `touchesCancelled` (a gesture recognizer or scroll view
//! took the touch) calls `cancelTrackingWithEvent:` and sends TouchCancel.
//! The tracking methods and `setHighlighted:` are sent as messages, so
//! subclasses' overrides take part. A disabled control doesn't track: its
//! touches go on to the next responder, as UIResponder's do.

use crate::objc::{self, host_class, send};
use crate::uiview::view;
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

// UIControlEvents
pub const TOUCH_DOWN: u64 = 1 << 0;
pub const TOUCH_DOWN_REPEAT: u64 = 1 << 1;
pub const TOUCH_DRAG_INSIDE: u64 = 1 << 2;
pub const TOUCH_DRAG_OUTSIDE: u64 = 1 << 3;
pub const TOUCH_DRAG_ENTER: u64 = 1 << 4;
pub const TOUCH_DRAG_EXIT: u64 = 1 << 5;
pub const TOUCH_UP_INSIDE: u64 = 1 << 6;
pub const TOUCH_UP_OUTSIDE: u64 = 1 << 7;
pub const TOUCH_CANCEL: u64 = 1 << 8;
pub const PRIMARY_ACTION_TRIGGERED: u64 = 1 << 13;

// UIControlState
const STATE_HIGHLIGHTED: u64 = 1 << 0;
const STATE_DISABLED: u64 = 1 << 1;
const STATE_SELECTED: u64 = 1 << 2;

/// How far outside its bounds a finger still counts as inside a control.
pub const TOUCH_SLOP: f64 = 70.0;

#[derive(Clone, Copy, Debug, Default)]
struct Tracking {
    /// The control tracks this touch (`beginTracking…` said yes, `continueTracking…` hasn't said no).
    tracking: bool,
    touch_inside: bool,
    /// Disabled when the touch began: its phases go to the next responder.
    passing: bool,
}

/// What a control does for some of its events: a target-action pair, or a UIAction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handler {
    Target(u64, u64),
    Action(u64),
}

#[derive(Default)]
pub struct State {
    /// Control → what it does and for which events, in the order added.
    targets: HashMap<u64, Vec<(Handler, u64)>>,
    tracking: HashMap<u64, Tracking>,
    next_action_id: u64,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn flag(emu: &Emu, c: u64, key: &str, default: bool) -> bool {
    objc::bag_get(emu, c, key).map_or(default, |x| x & 0xff != 0)
}

fn responds(emu: &mut Emu, obj: u64, sel: &str) -> Result<bool> {
    if obj == 0 {
        return Ok(false);
    }
    let s = objc::intern(emu, sel)?;
    objc::responds(emu, obj, s)
}

// ---- target-action ----

fn add_target(emu: &mut Emu) -> Result<()> {
    let (c, target, action, events) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4));
    add_handler(emu, c, Handler::Target(target, action), events);
    Ok(())
}

/// Register a handler for `events` (more events for one already there).
fn add_handler(emu: &mut Emu, c: u64, h: Handler, events: u64) {
    let list = emu.control.targets.entry(c).or_default();
    match list.iter_mut().find(|(x, _)| *x == h) {
        Some((_, e)) => *e |= events,
        None => list.push((h, events)),
    }
}

/// Stop `h` handling `events` (it's gone once it handles none).
fn remove_handlers(emu: &mut Emu, c: u64, events: u64, matches: impl Fn(Handler) -> bool) {
    if let Some(list) = emu.control.targets.get_mut(&c) {
        for (h, e) in list.iter_mut() {
            if matches(*h) {
                *e &= !events;
            }
        }
        list.retain(|&(_, e)| e != 0);
    }
}

fn remove_target(emu: &mut Emu) -> Result<()> {
    let (c, target, action, events) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4));
    remove_handlers(emu, c, events, |h| matches!(h, Handler::Target(t, a) if (target == 0 || t == target) && (action == 0 || a == action)));
    Ok(())
}

/// Fire what's registered for any of `events`: targets through the control's
/// `-sendAction:to:forEvent:`, UIActions through `-sendAction:` (both of
/// which subclasses may override).
pub fn send_actions(emu: &mut Emu, control: u64, events: u64, event: u64) -> Result<()> {
    let list = emu.control.targets.get(&control).cloned().unwrap_or_default();
    for (h, e) in list {
        if e & events == 0 {
            continue;
        }
        match h {
            Handler::Target(target, action) => send(emu, control, "sendAction:to:forEvent:", &[action, target, event])?,
            Handler::Action(a) => send(emu, control, "sendAction:", &[a])?,
        };
    }
    Ok(())
}

pub fn has_targets(emu: &Emu, control: u64, events: u64) -> bool {
    emu.control.targets.get(&control).is_some_and(|l| l.iter().any(|&(_, e)| e & events != 0))
}

// ---- UIAction ----

/// `+[UIAction actionWithTitle:image:identifier:handler:]` and friends.
fn new_action(emu: &mut Emu, cls: u64, title: u64, image: u64, identifier: u64, handler: u64) -> Result<u64> {
    let a = objc::create_instance(emu, cls, 0)?;
    let copy = |emu: &mut Emu, x: u64, sel: &str| if x != 0 { send(emu, x, sel, &[]) } else { Ok(0) };
    let title = copy(emu, title, "copy")?;
    let image = copy(emu, image, "retain")?;
    let handler = copy(emu, handler, "copy")?;
    // Without an identifier, a unique one (UIKit makes a UUID string).
    let identifier = if identifier != 0 {
        send(emu, identifier, "copy", &[])?
    } else {
        let n = emu.control.next_action_id;
        emu.control.next_action_id += 1;
        crate::foundation::new_nsstring(emu, &format!("dev.pomme.action.{n}"))?
    };
    objc::bag_set(emu, a, "title", title);
    objc::bag_set(emu, a, "image", image);
    objc::bag_set(emu, a, "identifier", identifier);
    objc::bag_set(emu, a, "pomme.handler", handler);
    Ok(a)
}

/// `-[UIControl sendAction:]`: run the action's handler, with `sender` the control.
fn send_ui_action(emu: &mut Emu) -> Result<()> {
    let (c, a) = (emu.arg(0), emu.arg(2));
    let handler = objc::bag_get(emu, a, "pomme.handler").unwrap_or(0);
    if handler == 0 {
        return Ok(());
    }
    let was = objc::bag_get(emu, a, "sender").unwrap_or(0);
    objc::bag_set(emu, a, "sender", c);
    crate::libsystem::call_block(emu, handler, &[a])?;
    objc::bag_set(emu, a, "sender", was);
    Ok(())
}

fn identifier_of(emu: &mut Emu, a: u64) -> Result<String> {
    let id = objc::bag_get(emu, a, "identifier").unwrap_or(0);
    if id == 0 { Ok(String::new()) } else { crate::foundation::nsstring(emu, id) }
}

/// Remove the actions with this identifier (an action and its replacement share one).
fn remove_action_named(emu: &mut Emu, c: u64, id: &str, events: u64) -> Result<()> {
    let actions: Vec<u64> = emu.control.targets.get(&c).map(|l| l.iter().filter_map(|&(h, _)| if let Handler::Action(a) = h { Some(a) } else { None }).collect()).unwrap_or_default();
    for a in actions {
        if identifier_of(emu, a)? == id {
            remove_handlers(emu, c, events, |h| h == Handler::Action(a));
        }
    }
    Ok(())
}

/// `-addAction:forControlEvents:`: an action with the identifier of one
/// already there replaces it.
fn add_ui_action(emu: &mut Emu, c: u64, a: u64, events: u64) -> Result<()> {
    if a == 0 {
        return Ok(());
    }
    let id = identifier_of(emu, a)?;
    remove_action_named(emu, c, &id, events)?;
    send(emu, a, "retain", &[])?;
    add_handler(emu, c, Handler::Action(a), events);
    Ok(())
}

/// `-[UIResponder targetForAction:withSender:]`: the first responder up the
/// chain from `r` that can perform the action.
fn target_for_action(emu: &mut Emu, mut r: u64, action: u64, sender: u64) -> Result<u64> {
    let mut hops = 0;
    while r != 0 && hops < 1000 {
        if send(emu, r, "canPerformAction:withSender:", &[action, sender])? & 0xff != 0 {
            return Ok(r);
        }
        r = send(emu, r, "nextResponder", &[])?;
        hops += 1;
    }
    Ok(0)
}

fn is_responder(emu: &mut Emu, obj: u64) -> Result<bool> {
    let cls = host_class(emu, "UIResponder")?;
    Ok(obj != 0 && objc::is_kind_of(emu, obj, cls)?)
}

/// `-[UIApplication sendAction:to:from:forEvent:]`: to the target, or with
/// none, to whoever up the responder chain can perform it (starting at the
/// first responder, else at the sender). YES if someone got it.
fn app_send_action(emu: &mut Emu) -> Result<()> {
    let (action, to, from, event) = (emu.arg(2), emu.arg(3), emu.arg(4), emu.arg(5));
    let target = if to != 0 {
        to
    } else {
        let fr = emu.input.first_responder;
        let start = if fr != 0 { fr } else if is_responder(emu, from)? { from } else { 0 };
        target_for_action(emu, start, action, from)?
    };
    if target == 0 {
        return ret(emu, 0);
    }
    let name = objc::sel_name(emu, action);
    // One-argument and no-argument actions ignore the extra registers.
    send(emu, target, &name, &[from, event])?;
    ret(emu, 1)
}

// ---- touch tracking ----

fn tracking(emu: &Emu, c: u64) -> Tracking {
    emu.control.tracking.get(&c).copied().unwrap_or_default()
}

fn set_tracking(emu: &mut Emu, c: u64, t: Tracking) {
    emu.control.tracking.insert(c, t);
}

fn any_touch(emu: &mut Emu, touches: u64) -> Result<u64> {
    send(emu, touches, "anyObject", &[])
}

/// Whether the touch is within the control's bounds grown by UIKit's slop.
fn touch_inside(emu: &mut Emu, c: u64, touch: u64) -> bool {
    let Some(p) = crate::touch::location_in(emu, touch, c) else { return false };
    let b = view(emu, c).bounds;
    p[0] >= b[0] - TOUCH_SLOP && p[0] <= b[0] + b[2] + TOUCH_SLOP && p[1] >= b[1] - TOUCH_SLOP && p[1] <= b[1] + b[3] + TOUCH_SLOP
}

fn is_enabled(emu: &mut Emu, c: u64) -> Result<bool> {
    Ok(send(emu, c, "isEnabled", &[])? & 0xff != 0)
}

/// UIResponder's default for a touch phase: on to the next responder.
fn pass_on(emu: &mut Emu, c: u64, sel: &str, touches: u64, event: u64) -> Result<()> {
    let next = send(emu, c, "nextResponder", &[])?;
    if responds(emu, next, sel)? {
        send(emu, next, sel, &[touches, event])?;
    }
    Ok(())
}

pub(crate) fn touches_began(emu: &mut Emu, c: u64, touches: u64, event: u64) -> Result<()> {
    if !is_enabled(emu, c)? {
        set_tracking(emu, c, Tracking { passing: true, ..Default::default() });
        return pass_on(emu, c, "touchesBegan:withEvent:", touches, event);
    }
    let touch = any_touch(emu, touches)?;
    set_tracking(emu, c, Tracking { touch_inside: true, ..Default::default() });
    let tracks = send(emu, c, "beginTrackingWithTouch:withEvent:", &[touch, event])? & 0xff != 0;
    set_tracking(emu, c, Tracking { tracking: tracks, touch_inside: true, passing: false });
    send(emu, c, "setHighlighted:", &[1])?;
    if tracks {
        let mut events = TOUCH_DOWN;
        if send(emu, touch, "tapCount", &[])? > 1 {
            events |= TOUCH_DOWN_REPEAT;
        }
        send_actions(emu, c, events, event)?;
    }
    Ok(())
}

pub(crate) fn touches_moved(emu: &mut Emu, c: u64, touches: u64, event: u64) -> Result<()> {
    let t = tracking(emu, c);
    if t.passing {
        return pass_on(emu, c, "touchesMoved:withEvent:", touches, event);
    }
    let touch = any_touch(emu, touches)?;
    let inside = touch_inside(emu, c, touch);
    set_tracking(emu, c, Tracking { touch_inside: inside, ..t });
    send(emu, c, "setHighlighted:", &[inside as u64])?;
    if t.tracking {
        let go_on = send(emu, c, "continueTrackingWithTouch:withEvent:", &[touch, event])? & 0xff != 0;
        set_tracking(emu, c, Tracking { tracking: go_on, ..tracking(emu, c) });
        if go_on {
            let mut events = if inside { TOUCH_DRAG_INSIDE } else { TOUCH_DRAG_OUTSIDE };
            if inside && !t.touch_inside {
                events |= TOUCH_DRAG_ENTER;
            } else if !inside && t.touch_inside {
                events |= TOUCH_DRAG_EXIT;
            }
            send_actions(emu, c, events, event)?;
        }
    }
    Ok(())
}

pub(crate) fn touches_ended(emu: &mut Emu, c: u64, touches: u64, event: u64) -> Result<()> {
    let t = tracking(emu, c);
    if t.passing {
        emu.control.tracking.remove(&c);
        return pass_on(emu, c, "touchesEnded:withEvent:", touches, event);
    }
    let touch = any_touch(emu, touches)?;
    let inside = touch_inside(emu, c, touch);
    // `touchInside` reads true while the actions go out, then the touch is over.
    set_tracking(emu, c, Tracking { touch_inside: inside, ..t });
    send(emu, c, "setHighlighted:", &[0])?;
    if t.tracking {
        send(emu, c, "endTrackingWithTouch:withEvent:", &[touch, event])?;
        let mut events = if inside { TOUCH_UP_INSIDE } else { TOUCH_UP_OUTSIDE };
        // A button's primary action is a tap.
        if inside && is_button(emu, c)? {
            events |= PRIMARY_ACTION_TRIGGERED;
        }
        send_actions(emu, c, events, event)?;
    }
    emu.control.tracking.remove(&c);
    Ok(())
}

pub(crate) fn touches_cancelled(emu: &mut Emu, c: u64, touches: u64, event: u64) -> Result<()> {
    let t = tracking(emu, c);
    if t.passing {
        emu.control.tracking.remove(&c);
        return pass_on(emu, c, "touchesCancelled:withEvent:", touches, event);
    }
    send(emu, c, "setHighlighted:", &[0])?;
    if t.tracking {
        send(emu, c, "cancelTrackingWithEvent:", &[event])?;
        send_actions(emu, c, TOUCH_CANCEL, event)?;
    }
    emu.control.tracking.remove(&c);
    Ok(())
}

fn control_state(emu: &mut Emu, c: u64) -> Result<u64> {
    let mut s = 0;
    if send(emu, c, "isHighlighted", &[])? & 0xff != 0 {
        s |= STATE_HIGHLIGHTED;
    }
    if !is_enabled(emu, c)? {
        s |= STATE_DISABLED;
    }
    if send(emu, c, "isSelected", &[])? & 0xff != 0 {
        s |= STATE_SELECTED;
    }
    Ok(s)
}

// ---- UIButton ----

fn is_button(emu: &mut Emu, v: u64) -> Result<bool> {
    let cls = host_class(emu, "UIButton")?;
    objc::is_kind_of(emu, v, cls)
}

/// UIButtonType
const BUTTON_SYSTEM: u64 = 1;
/// The system button's tint.
const SYSTEM_BLUE: [f64; 4] = [0.0, 0.478, 1.0, 1.0];

fn state_key(what: &str, state: u64) -> String {
    format!("pomme.{what}.{state}")
}

/// A per-state value: the state's own, else the normal state's (UIKit's fallback).
fn for_state(emu: &Emu, b: u64, what: &str, state: u64) -> u64 {
    objc::bag_get(emu, b, &state_key(what, state)).filter(|&x| x != 0).or_else(|| objc::bag_get(emu, b, &state_key(what, 0))).unwrap_or(0)
}

/// A button's title font: 15-pt system.
const TITLE_SIZE: f64 = 15.0;
/// Room above and below the title line: a 15-pt title makes a 30-pt button
/// (UIKit's 46×30 "Button").
const TITLE_PAD: f64 = 6.0;

/// The title label, a UILabel subview (made on first use) that layoutSubviews sizes and places.
fn title_label(emu: &mut Emu, b: u64) -> Result<u64> {
    if let Some(l) = objc::bag_get(emu, b, "pomme.titleLabel").filter(|&l| l != 0) {
        return Ok(l);
    }
    let cls = host_class(emu, "UILabel")?;
    let l = send(emu, cls, "alloc", &[])?;
    emu.cpu.q[..4].fill(0);
    let l = send(emu, l, "initWithFrame:", &[])?;
    let font = crate::uiview::new_font(emu, ".SFUI-Regular", TITLE_SIZE, 0.0)?;
    objc::bag_set(emu, l, "font", font);
    objc::bag_set(emu, l, "textAlignment", 1); // centered
    objc::bag_set(emu, b, "pomme.titleLabel", l);
    send(emu, b, "addSubview:", &[l])?;
    Ok(l)
}

/// Show the current state's title and color.
fn update_title(emu: &mut Emu, b: u64) -> Result<()> {
    let state = control_state(emu, b)?;
    let title = for_state(emu, b, "title", state);
    if title == 0 && objc::bag_get(emu, b, "pomme.titleLabel").is_none() {
        return Ok(());
    }
    let l = title_label(emu, b)?;
    if objc::bag_get(emu, b, "pomme.shownTitle") != Some(title) {
        objc::bag_set(emu, b, "pomme.shownTitle", title);
        send(emu, l, "setText:", &[title])?;
        // A new title is a new size.
        send(emu, b, "invalidateIntrinsicContentSize", &[])?;
        send(emu, b, "setNeedsLayout", &[])?;
    }
    let mut color = for_state(emu, b, "titleColor", state);
    if color == 0 && objc::bag_get(emu, b, "buttonType").unwrap_or(0) == BUTTON_SYSTEM {
        color = crate::uiview::new_color(emu, SYSTEM_BLUE)?;
    }
    objc::bag_set(emu, l, "textColor", color);
    crate::host::frame_changed(emu);
    Ok(())
}

const INSET_KEYS: [&str; 4] = ["pomme.contentInset.top", "pomme.contentInset.left", "pomme.contentInset.bottom", "pomme.contentInset.right"];

/// `contentEdgeInsets`: top, left, bottom, right.
fn content_insets(emu: &Emu, b: u64) -> [f64; 4] {
    INSET_KEYS.map(|k| objc::bag_get(emu, b, k).map_or(0.0, f64::from_bits))
}

/// The title's size, padded as UIKit pads it (nothing without a title).
fn title_size(emu: &mut Emu, b: u64) -> Result<[f64; 2]> {
    let Some(l) = objc::bag_get(emu, b, "pomme.titleLabel").filter(|&l| l != 0) else { return Ok([0.0; 2]) };
    if objc::bag_get(emu, b, "pomme.shownTitle").unwrap_or(0) == 0 {
        return Ok([0.0; 2]);
    }
    let r = objc::send_fp(emu, l, "intrinsicContentSize", &[], &[])?;
    Ok([r[0], r[1]])
}

/// What the button needs: its title (and the room around it) inside the content insets.
fn button_size(emu: &mut Emu, b: u64) -> Result<[f64; 2]> {
    let t = title_size(emu, b)?;
    let [top, left, bottom, right] = content_insets(emu, b);
    let pad = if t[1] > 0.0 { 2.0 * TITLE_PAD } else { 0.0 };
    Ok([t[0] + left + right, t[1] + pad + top + bottom])
}

/// Where the title goes: its size in the content rect (the bounds inside the
/// insets), placed by the content alignments (centered unless set).
fn lay_out_title(emu: &mut Emu, b: u64) -> Result<()> {
    let Some(l) = objc::bag_get(emu, b, "pomme.titleLabel").filter(|&l| l != 0) else { return Ok(()) };
    let bounds = view(emu, b).bounds;
    let [top, left, bottom, right] = content_insets(emu, b);
    let content = [left, top, (bounds[2] - left - right).max(0.0), (bounds[3] - top - bottom).max(0.0)];
    let t = title_size(emu, b)?;
    let (w, h) = (t[0].min(content[2]), t[1].min(content[3]));
    // UIControlContentHorizontalAlignment: center, left, right, fill, leading, trailing.
    let (x, w) = match objc::bag_get(emu, b, "contentHorizontalAlignment").unwrap_or(0) {
        1 | 4 => (content[0], w),
        2 | 5 => (content[0] + content[2] - w, w),
        3 => (content[0], content[2]),
        _ => (content[0] + (content[2] - w) / 2.0, w),
    };
    // UIControlContentVerticalAlignment: center, top, bottom, fill.
    let (y, h) = match objc::bag_get(emu, b, "contentVerticalAlignment").unwrap_or(0) {
        1 => (content[1], h),
        2 => (content[1] + content[3] - h, h),
        3 => (content[1], content[3]),
        _ => (content[1] + (content[3] - h) / 2.0, h),
    };
    objc::send_fp(emu, l, "setFrame:", &[], &[x, y, w, h])?;
    Ok(())
}

fn button_with_type(emu: &mut Emu) -> Result<()> {
    let (cls, ty) = (emu.arg(0), emu.arg(2));
    let b = make_button(emu, cls, ty)?;
    ret(emu, b)
}

fn make_button(emu: &mut Emu, cls: u64, ty: u64) -> Result<u64> {
    let b = send(emu, cls, "alloc", &[])?;
    emu.cpu.q[..4].fill(0);
    let b = send(emu, b, "initWithFrame:", &[])?;
    objc::bag_set(emu, b, "buttonType", ty);
    Ok(b)
}

/// `primaryAction`: fires on the button's primary action, and its title is the button's.
fn set_primary_action(emu: &mut Emu, b: u64, a: u64) -> Result<()> {
    if let Some(old) = objc::bag_get(emu, b, "primaryAction").filter(|&x| x != 0) {
        remove_handlers(emu, b, u64::MAX, |h| h == Handler::Action(old));
    }
    objc::bag_set(emu, b, "primaryAction", a);
    if a != 0 {
        add_ui_action(emu, b, a, PRIMARY_ACTION_TRIGGERED)?;
        let title = objc::bag_get(emu, a, "title").unwrap_or(0);
        if title != 0 {
            objc::bag_set(emu, b, &state_key("title", 0), title);
            update_title(emu, b)?;
        }
    }
    Ok(())
}

/// `+buttonWithType:primaryAction:` / `+systemButtonWithPrimaryAction:`.
fn button_with_primary_action(emu: &mut Emu, ty: u64, a: u64) -> Result<()> {
    let cls = emu.arg(0);
    let b = make_button(emu, cls, ty)?;
    set_primary_action(emu, b, a)?;
    ret(emu, b)
}

fn set_edge_insets(emu: &mut Emu) -> Result<()> {
    let b = emu.arg(0);
    for (i, k) in INSET_KEYS.iter().enumerate() {
        objc::bag_set(emu, b, k, emu.cpu.q[i] as u64);
    }
    send(emu, b, "invalidateIntrinsicContentSize", &[])?;
    send(emu, b, "setNeedsLayout", &[])?;
    Ok(())
}

/// A content alignment's setter: store it and lay out again.
fn set_alignment(emu: &mut Emu, key: &str) -> Result<()> {
    let (c, x) = (emu.arg(0), emu.arg(2));
    objc::bag_set(emu, c, key, x);
    send(emu, c, "setNeedsLayout", &[])?;
    Ok(())
}

fn set_for_state(emu: &mut Emu, what: &str) -> Result<()> {
    let (b, x, state) = (emu.arg(0), emu.arg(2), emu.arg(3));
    let x = if x != 0 { send(emu, x, if what == "title" { "copy" } else { "retain" }, &[])? } else { 0 };
    objc::bag_set(emu, b, &state_key(what, state), x);
    update_title(emu, b)
}

fn get_for_state(emu: &mut Emu, what: &str) -> Result<()> {
    let (b, state) = (emu.arg(0), emu.arg(2));
    let x = for_state(emu, b, what, state);
    ret(emu, x)
}

fn get_current(emu: &mut Emu, what: &str) -> Result<()> {
    let b = emu.arg(0);
    let state = control_state(emu, b)?;
    let x = for_state(emu, b, what, state);
    ret(emu, x)
}

/// A state flag's setter: store it; a button shows the new state's title.
fn set_state_flag(emu: &mut Emu, key: &str) -> Result<()> {
    let (c, x) = (emu.arg(0), emu.arg(2) & 0xff);
    objc::bag_set(emu, c, key, x);
    if is_button(emu, c)? {
        update_title(emu, c)?;
    }
    Ok(())
}

const C: &str = "UIControl";
const B: &str = "UIButton";

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (C, false, "addTarget:action:forControlEvents:", add_target),
    (C, false, "removeTarget:action:forControlEvents:", remove_target),
    (C, false, "sendActionsForControlEvents:", |e| {
        let (c, ev) = (e.arg(0), e.arg(2));
        send_actions(e, c, ev, 0)
    }),
    (C, false, "sendAction:to:forEvent:", |e| {
        let (c, action, target, event) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let app = e.ui.app;
        if app != 0 {
            send(e, app, "sendAction:to:from:forEvent:", &[action, target, c, event])?;
        }
        Ok(())
    }),
    (C, false, "allTargets", |e| {
        let mut t: Vec<u64> = e.control.targets.get(&e.arg(0)).map(|l| l.iter().filter_map(|&(h, _)| if let Handler::Target(t, _) = h { Some(t) } else { None }).filter(|&x| x != 0).collect()).unwrap_or_default();
        t.sort_unstable();
        t.dedup();
        let a = crate::foundation::new_array_of(e, &t)?;
        let cls = host_class(e, "NSSet")?;
        let s = send(e, cls, "setWithArray:", &[a])?;
        ret(e, s)
    }),
    (C, false, "allControlEvents", |e| {
        let ev = e.control.targets.get(&e.arg(0)).map_or(0, |l| l.iter().fold(0, |a, x| a | x.1));
        ret(e, ev)
    }),
    (C, false, "actionsForTarget:forControlEvent:", |e| {
        let (c, target, events) = (e.arg(0), e.arg(2), e.arg(3));
        let list = e.control.targets.get(&c).cloned().unwrap_or_default();
        let mut names = Vec::new();
        for (h, ev) in list {
            if let Handler::Target(t, a) = h
                && t == target
                && ev & events != 0
            {
                let name = objc::sel_name(e, a);
                names.push(crate::foundation::new_nsstring(e, &name)?);
            }
        }
        if names.is_empty() {
            return ret(e, 0);
        }
        let a = crate::foundation::new_array_of(e, &names)?;
        ret(e, a)
    }),
    // Enabled unless turned off (React Native's text field reads `editable` from it).
    (C, false, "isEnabled", |e| {
        let v = flag(e, e.arg(0), "enabled", true) as u64;
        ret(e, v)
    }),
    (C, false, "setEnabled:", |e| set_state_flag(e, "enabled")),
    (C, false, "isHighlighted", |e| {
        let v = flag(e, e.arg(0), "highlighted", false) as u64;
        ret(e, v)
    }),
    (C, false, "setHighlighted:", |e| set_state_flag(e, "highlighted")),
    (C, false, "isSelected", |e| {
        let v = flag(e, e.arg(0), "selected", false) as u64;
        ret(e, v)
    }),
    (C, false, "setSelected:", |e| set_state_flag(e, "selected")),
    (C, false, "state", |e| {
        let s = control_state(e, e.arg(0))?;
        ret(e, s)
    }),
    (C, false, "isTracking", |e| {
        let v = tracking(e, e.arg(0)).tracking as u64;
        ret(e, v)
    }),
    (C, false, "isTouchInside", |e| {
        let v = tracking(e, e.arg(0)).touch_inside as u64;
        ret(e, v)
    }),
    (C, false, "contentHorizontalAlignment", |e| {
        let v = objc::bag_get(e, e.arg(0), "contentHorizontalAlignment").unwrap_or(0);
        ret(e, v)
    }),
    (C, false, "setContentHorizontalAlignment:", |e| set_alignment(e, "contentHorizontalAlignment")),
    (C, false, "contentVerticalAlignment", |e| {
        let v = objc::bag_get(e, e.arg(0), "contentVerticalAlignment").unwrap_or(0);
        ret(e, v)
    }),
    (C, false, "setContentVerticalAlignment:", |e| set_alignment(e, "contentVerticalAlignment")),
    (C, false, "addAction:forControlEvents:", |e| {
        let (c, a, ev) = (e.arg(0), e.arg(2), e.arg(3));
        add_ui_action(e, c, a, ev)
    }),
    (C, false, "removeAction:forControlEvents:", |e| {
        let (c, a, ev) = (e.arg(0), e.arg(2), e.arg(3));
        let id = identifier_of(e, a)?;
        remove_action_named(e, c, &id, ev)
    }),
    (C, false, "removeActionForIdentifier:forControlEvents:", |e| {
        let (c, id, ev) = (e.arg(0), e.arg(2), e.arg(3));
        let id = if id == 0 { String::new() } else { crate::foundation::nsstring(e, id)? };
        remove_action_named(e, c, &id, ev)
    }),
    (C, false, "sendAction:", send_ui_action),
    // `-initWithFrame:primaryAction:` (iOS 14): a control that runs the action as its primary one.
    (C, false, "initWithFrame:primaryAction:", |e| {
        let (c, a) = (e.arg(0), e.arg(2));
        let c = send(e, c, "initWithFrame:", &[])?;
        if is_button(e, c)? {
            set_primary_action(e, c, a)?;
        } else {
            add_ui_action(e, c, a, PRIMARY_ACTION_TRIGGERED)?;
        }
        ret(e, c)
    }),
    (C, false, "beginTrackingWithTouch:withEvent:", |e| ret(e, 1)),
    (C, false, "continueTrackingWithTouch:withEvent:", |e| ret(e, 1)),
    (C, false, "endTrackingWithTouch:withEvent:", |_| Ok(())),
    (C, false, "cancelTrackingWithEvent:", |_| Ok(())),
    (C, false, "touchesBegan:withEvent:", |e| {
        let (c, t, ev) = (e.arg(0), e.arg(2), e.arg(3));
        touches_began(e, c, t, ev)
    }),
    (C, false, "touchesMoved:withEvent:", |e| {
        let (c, t, ev) = (e.arg(0), e.arg(2), e.arg(3));
        touches_moved(e, c, t, ev)
    }),
    (C, false, "touchesEnded:withEvent:", |e| {
        let (c, t, ev) = (e.arg(0), e.arg(2), e.arg(3));
        touches_ended(e, c, t, ev)
    }),
    (C, false, "touchesCancelled:withEvent:", |e| {
        let (c, t, ev) = (e.arg(0), e.arg(2), e.arg(3));
        touches_cancelled(e, c, t, ev)
    }),
    // UIResponder: the action-sending half of the responder chain.
    ("UIResponder", false, "canPerformAction:withSender:", |e| {
        let (r, action) = (e.arg(0), e.arg(2));
        let v = objc::responds(e, r, action)? as u64;
        ret(e, v)
    }),
    ("UIResponder", false, "targetForAction:withSender:", |e| {
        let (r, action, sender) = (e.arg(0), e.arg(2), e.arg(3));
        let t = target_for_action(e, r, action, sender)?;
        ret(e, t)
    }),
    // The chain's top: window → application → its delegate (when that's a responder).
    ("UIWindow", false, "nextResponder", |e| {
        let app = e.ui.app;
        ret(e, app)
    }),
    ("UIApplication", false, "nextResponder", |e| {
        let d = e.ui.delegate;
        let next = if d != 0 && d != e.arg(0) && is_responder(e, d)? { d } else { 0 };
        ret(e, next)
    }),
    ("UIApplication", false, "sendAction:to:from:forEvent:", app_send_action),
    // UIAction
    ("UIAction", true, "actionWithHandler:", |e| {
        let (cls, h) = (e.arg(0), e.arg(2));
        let a = new_action(e, cls, 0, 0, 0, h)?;
        ret(e, a)
    }),
    ("UIAction", true, "actionWithTitle:image:identifier:handler:", |e| {
        let (cls, t, i, id, h) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4), e.arg(5));
        let a = new_action(e, cls, t, i, id, h)?;
        ret(e, a)
    }),
    ("UIAction", false, "title", |e| {
        let v = objc::bag_get(e, e.arg(0), "title").unwrap_or(0);
        ret(e, v)
    }),
    ("UIAction", false, "image", |e| {
        let v = objc::bag_get(e, e.arg(0), "image").unwrap_or(0);
        ret(e, v)
    }),
    ("UIAction", false, "identifier", |e| {
        let v = objc::bag_get(e, e.arg(0), "identifier").unwrap_or(0);
        ret(e, v)
    }),
    ("UIAction", false, "sender", |e| {
        let v = objc::bag_get(e, e.arg(0), "sender").unwrap_or(0);
        ret(e, v)
    }),
    // UIButton
    (B, true, "buttonWithType:", button_with_type),
    (B, false, "buttonType", |e| {
        let t = objc::bag_get(e, e.arg(0), "buttonType").unwrap_or(0);
        ret(e, t)
    }),
    (B, false, "setTitle:forState:", |e| set_for_state(e, "title")),
    (B, false, "titleForState:", |e| get_for_state(e, "title")),
    (B, false, "currentTitle", |e| get_current(e, "title")),
    (B, false, "setTitleColor:forState:", |e| set_for_state(e, "titleColor")),
    (B, false, "titleColorForState:", |e| get_for_state(e, "titleColor")),
    (B, false, "currentTitleColor", |e| get_current(e, "titleColor")),
    (B, false, "titleLabel", |e| {
        let b = e.arg(0);
        let l = title_label(e, b)?;
        ret(e, l)
    }),
    (B, false, "layoutSubviews", |e| {
        let b = e.arg(0);
        lay_out_title(e, b)
    }),
    (B, false, "intrinsicContentSize", |e| {
        let s = button_size(e, e.arg(0))?;
        crate::uikit::ret_rect(e, [s[0], s[1], 0.0, 0.0]);
        Ok(())
    }),
    // Its own size, whatever it's offered (as UIKit's does for a title).
    (B, false, "sizeThatFits:", |e| {
        let s = button_size(e, e.arg(0))?;
        crate::uikit::ret_rect(e, [s[0], s[1], 0.0, 0.0]);
        Ok(())
    }),
    (B, false, "contentEdgeInsets", |e| {
        let i = content_insets(e, e.arg(0));
        crate::uikit::ret_rect(e, i);
        Ok(())
    }),
    (B, false, "setContentEdgeInsets:", set_edge_insets),
    (B, true, "buttonWithType:primaryAction:", |e| {
        let (ty, a) = (e.arg(2), e.arg(3));
        button_with_primary_action(e, ty, a)
    }),
    (B, true, "systemButtonWithPrimaryAction:", |e| {
        let a = e.arg(2);
        button_with_primary_action(e, BUTTON_SYSTEM, a)
    }),
    (B, false, "primaryAction", |e| {
        let a = objc::bag_get(e, e.arg(0), "primaryAction").unwrap_or(0);
        ret(e, a)
    }),
    (B, false, "setPrimaryAction:", |e| {
        let (b, a) = (e.arg(0), e.arg(2));
        set_primary_action(e, b, a)
    }),
];

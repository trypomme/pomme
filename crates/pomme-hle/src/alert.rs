//! UIAlertController and UIAlertAction, as iOS 26 shows them on iPhone
//! (tests/fixtures/alerts, notes.md there): a glass box 320 pt wide in the
//! middle of the safe area, with the title and message, and each action as a
//! capsule button. Alerts and action sheets look the same; an action sheet's
//! actions are always stacked.
//!
//! The box is the controller's view, made of pomme's own views (a glass
//! effect view, labels, buttons), so drawing, taps and accessibility need
//! nothing of their own. navigation.rs presents it (its own kind of
//! presentation: the box grows into place, the screen behind dims); a tap
//! on an action dismisses it, and the action's handler runs once it's gone.

use crate::objc::{self, bag_get, bag_set, send};
use crate::{Emu, HostFn, Result, foundation};

const AC: &str = "UIAlertController";
const AA: &str = "UIAlertAction";

/// UIAlertControllerStyle.
const ACTION_SHEET: u64 = 0;
/// UIAlertActionStyle.
const CANCEL: u64 = 1;
const DESTRUCTIVE: u64 = 2;

// Layout, in points (notes.md).
pub(crate) const WIDTH: f64 = 320.0;
const RADIUS: f64 = 34.0;
const TEXT_X: f64 = 30.0;
const TEXT_TOP: f64 = 22.0;
/// Between the title's bottom and the message's top.
const TEXT_GAP: f64 = 7.67;
/// Below the header's last line: a secondary message, or a lone 17-pt text.
const BELOW_MESSAGE: f64 = 4.33;
const BELOW_LONE: f64 = 4.0;
/// A wrapping message's lines are 2 pt further apart than the font's (the
/// `long` case: 18 + 20 + 20).
const MESSAGE_LINE_GAP: f64 = 2.0;
const BUTTON_INSET: f64 = 16.0;
const BUTTON_HEIGHT: f64 = 48.0;
const BUTTON_GAP: f64 = 8.0;

// Colours (notes.md; light appearance).
const LABEL: [f64; 4] = [0.0, 0.0, 0.0, 1.0];
/// The message: what's under it × 0.45 over every backdrop measured, as black at 55%.
const SECONDARY: [f64; 4] = [0.0, 0.0, 0.0, 0.55];
/// A disabled action's title, as drawn on its fill.
const DISABLED: [f64; 4] = [0.235, 0.235, 0.263, 0.24];
const RED: [f64; 4] = [1.0, 56.0 / 255.0, 60.0 / 255.0, 1.0];
const WHITE: [f64; 4] = [1.0, 1.0, 1.0, 1.0];
/// The preferred action's tint (its fill: render.rs, ALERT_TINTED_FILL).
const PREFERRED_FILL: [f64; 4] = [0.0, 0.533, 1.0, 1.0];

// UIFontWeight.
const REGULAR: f64 = 0.0;
const MEDIUM: f64 = 0.23;
const SEMIBOLD: f64 = 0.3;

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

/// Whether `vc` is an alert controller.
pub(crate) fn is_alert(emu: &mut Emu, vc: u64) -> Result<bool> {
    let cls = objc::host_class(emu, AC)?;
    objc::is_kind_of(emu, vc, cls)
}

/// Whether it's an action sheet (a tap outside it cancels).
pub(crate) fn is_action_sheet(emu: &Emu, vc: u64) -> bool {
    bag_get(emu, vc, "pomme.alertStyle") == Some(ACTION_SHEET)
}

/// The actions, in the order the app added them.
fn actions(emu: &mut Emu, vc: u64) -> Result<Vec<u64>> {
    let list = bag_get(emu, vc, "pomme.actions").unwrap_or(0);
    if list == 0 {
        return Ok(Vec::new());
    }
    foundation::array_values(emu, list)
}

fn string(emu: &mut Emu, s: u64) -> Result<Option<String>> {
    if s == 0 {
        return Ok(None);
    }
    let s = foundation::nsstring(emu, s)?;
    Ok((!s.is_empty()).then_some(s))
}

fn new_of(emu: &mut Emu, class: &str, frame: [f64; 4]) -> Result<u64> {
    let cls = objc::host_class(emu, class)?;
    let v = send(emu, cls, "alloc", &[])?;
    for (i, f) in frame.iter().enumerate() {
        emu.cpu.q[i] = f.to_bits() as u128;
    }
    send(emu, v, "initWithFrame:", &[])
}

fn set_frame(emu: &mut Emu, v: u64, frame: [f64; 4]) {
    crate::uiview::set_frame_of(emu, v, frame);
}

fn round(emu: &mut Emu, v: u64, radius: f64) -> Result<()> {
    let l = send(emu, v, "layer", &[])?;
    let ly = crate::calayer::layer_mut(emu, l);
    ly.corner_radius = radius;
    ly.continuous_corners = true;
    Ok(())
}

/// A label of `text` at y, the text's width, wrapping with `line_gap` more
/// between lines; returns its height.
fn label(emu: &mut Emu, parent: u64, text: &str, font: (&str, f64, f64), color: [f64; 4], y: f64, line_gap: f64) -> Result<f64> {
    let l = new_of(emu, "UILabel", [TEXT_X, y, WIDTH - 2.0 * TEXT_X, 0.0])?;
    // (pomme's UILabel keeps these as stored properties.)
    let s = foundation::new_nsstring(emu, text)?;
    bag_set(emu, l, "text", s);
    let f = crate::uiview::new_font(emu, font.0, font.1, font.2)?;
    bag_set(emu, l, "font", f);
    let c = crate::uiview::new_color(emu, color)?;
    bag_set(emu, l, "textColor", c);
    bag_set(emu, l, "numberOfLines", 0);
    if line_gap > 0.0 {
        objc::bag_set_f64(emu, l, "pomme.lineSpacing", line_gap);
    }
    // Its height, up to a whole pixel (as UIKit sizes it).
    let scale = emu.device.scale;
    let h = (crate::label::lay_out(emu, l, WIDTH - 2.0 * TEXT_X)?.height() * scale - 1e-6).ceil() / scale;
    set_frame(emu, l, [TEXT_X, y, WIDTH - 2.0 * TEXT_X, h]);
    send(emu, parent, "addSubview:", &[l])?;
    Ok(h)
}

/// The actions in the order they're shown: the cancel action on the left of
/// two side by side, last in a stack (wherever the app added it).
fn shown_order(emu: &mut Emu, vc: u64, side_by_side: bool) -> Result<Vec<u64>> {
    let all = actions(emu, vc)?;
    let mut cancel = Vec::new();
    let mut rest = Vec::new();
    for a in all {
        if bag_get(emu, a, "pomme.style") == Some(CANCEL) {
            cancel.push(a);
        } else {
            rest.push(a);
        }
    }
    Ok(if side_by_side { cancel.into_iter().chain(rest).collect() } else { rest.into_iter().chain(cancel).collect() })
}

/// One action's capsule button, at `frame` in the box.
fn button(emu: &mut Emu, vc: u64, parent: u64, action: u64, frame: [f64; 4]) -> Result<()> {
    let cls = objc::host_class(emu, "UIButton")?;
    let b = send(emu, cls, "buttonWithType:", &[0])?;
    set_frame(emu, b, frame);
    let preferred = bag_get(emu, vc, "pomme.preferredAction") == Some(action);
    let style = bag_get(emu, action, "pomme.style").unwrap_or(0);
    round(emu, b, BUTTON_HEIGHT / 2.0)?;
    let title = bag_get(emu, action, "pomme.title").unwrap_or(0);
    send(emu, b, "setTitle:forState:", &[title, 0])?;
    let color = if preferred {
        WHITE
    } else if style == DESTRUCTIVE {
        RED
    } else {
        LABEL
    };
    let c = crate::uiview::new_color(emu, color)?;
    send(emu, b, "setTitleColor:forState:", &[c, 0])?;
    let d = crate::uiview::new_color(emu, DISABLED)?;
    send(emu, b, "setTitleColor:forState:", &[d, 2])?;
    let tl = send(emu, b, "titleLabel", &[])?;
    let f = crate::uiview::new_font(emu, if preferred { ".SFUI-Semibold" } else { ".SFUI-Medium" }, 17.0, if preferred { SEMIBOLD } else { MEDIUM })?;
    bag_set(emu, tl, "font", f);
    let enabled = bag_get(emu, action, "pomme.disabled").unwrap_or(0) == 0;
    send(emu, b, "setEnabled:", &[enabled as u64])?;
    // The capsule's fill, behind the title: it colours the glass under it (render.rs).
    let cls = objc::host_class(emu, "UIGlassEffect")?;
    let effect = send(emu, cls, "effectWithStyle:", &[if preferred { crate::glass::ALERT_TINTED_FILL } else { crate::glass::ALERT_FILL }])?;
    if preferred {
        let tint = crate::uiview::new_color(emu, PREFERRED_FILL)?;
        bag_set(emu, effect, "pommeTint", tint);
    }
    let cls = objc::host_class(emu, "UIVisualEffectView")?;
    let fill = send(emu, cls, "alloc", &[])?;
    let fill = send(emu, fill, "initWithEffect:", &[effect])?;
    set_frame(emu, fill, [0.0, 0.0, frame[2], frame[3]]);
    round(emu, fill, BUTTON_HEIGHT / 2.0)?;
    crate::uiview::view_mut(emu, fill).user_interaction = false;
    send(emu, b, "insertSubview:atIndex:", &[fill, 0])?;
    bag_set(emu, b, "pomme.alertAction", action);
    let sel = objc::intern(emu, "_pommeAlertActionTapped:")?;
    // UIControlEventTouchUpInside
    send(emu, b, "addTarget:action:forControlEvents:", &[vc, sel, 1 << 6])?;
    send(emu, parent, "addSubview:", &[b])?;
    Ok(())
}

/// Build the box (the controller's view) for what it holds now; its size.
pub(crate) fn build(emu: &mut Emu, vc: u64) -> Result<[f64; 2]> {
    let bx = send(emu, vc, "view", &[])?;
    let content = send(emu, bx, "contentView", &[])?;
    for s in crate::uiview::view(emu, content).subviews {
        send(emu, s, "removeFromSuperview", &[])?;
    }
    let title = bag_get(emu, vc, "pomme.alertTitle").unwrap_or(0);
    let message = bag_get(emu, vc, "pomme.message").unwrap_or(0);
    let (title, message) = (string(emu, title)?, string(emu, message)?);
    // The header: both texts (a semibold title, a secondary message), or the one alone (regular 17).
    let mut y = TEXT_TOP;
    match (&title, &message) {
        (Some(t), Some(m)) => {
            y += label(emu, content, t, (".SFUI-Semibold", 17.0, SEMIBOLD), LABEL, y, 0.0)?;
            y += TEXT_GAP;
            y += label(emu, content, m, (".SFUI-Regular", 15.0, REGULAR), SECONDARY, y, MESSAGE_LINE_GAP)?;
            y += BELOW_MESSAGE;
        }
        (Some(t), None) | (None, Some(t)) => {
            y += label(emu, content, t, (".SFUI-Regular", 17.0, REGULAR), LABEL, y, 0.0)?;
            y += BELOW_LONE;
        }
        (None, None) => y = 0.0,
    }
    // The actions: two side by side in an alert, else stacked.
    let n = actions(emu, vc)?.len();
    let side_by_side = n == 2 && !is_action_sheet(emu, vc);
    let order = shown_order(emu, vc, side_by_side)?;
    let inner = WIDTH - 2.0 * BUTTON_INSET;
    let mut top = y + BUTTON_INSET;
    if side_by_side {
        let w = (inner - BUTTON_GAP) / 2.0;
        for (i, &a) in order.iter().enumerate() {
            button(emu, vc, content, a, [BUTTON_INSET + i as f64 * (w + BUTTON_GAP), top, w, BUTTON_HEIGHT])?;
        }
        top += BUTTON_HEIGHT;
    } else {
        for (i, &a) in order.iter().enumerate() {
            if i > 0 {
                top += BUTTON_GAP;
            }
            button(emu, vc, content, a, [BUTTON_INSET, top, inner, BUTTON_HEIGHT])?;
            top += BUTTON_HEIGHT;
        }
    }
    let height = if n > 0 { top + BUTTON_INSET } else { y };
    set_frame(emu, bx, [0.0, 0.0, WIDTH, height]);
    set_frame(emu, content, [0.0, 0.0, WIDTH, height]);
    Ok([WIDTH, height])
}

/// After the alert controller has gone: the tapped action's handler.
pub(crate) fn dismissed(emu: &mut Emu, vc: u64) -> Result<()> {
    let Some(a) = bag_get(emu, vc, "pomme.tappedAction").filter(|&a| a != 0) else { return Ok(()) };
    bag_set(emu, vc, "pomme.tappedAction", 0);
    let h = bag_get(emu, a, "pomme.handler").unwrap_or(0);
    if h != 0 {
        crate::libsystem::call_block(emu, h, &[a])?;
    }
    Ok(())
}

/// `action` was chosen: close the alert controller, once; the handler runs when it has gone.
fn choose(emu: &mut Emu, vc: u64, action: u64) -> Result<()> {
    if bag_get(emu, vc, "pomme.closing").unwrap_or(0) != 0 {
        return Ok(());
    }
    bag_set(emu, vc, "pomme.closing", 1);
    bag_set(emu, vc, "pomme.tappedAction", action);
    send(emu, vc, "dismissViewControllerAnimated:completion:", &[1, 0])?;
    Ok(())
}

/// A tap on the dimming around an action sheet: its cancel action, if it has one.
pub(crate) fn tapped_outside(emu: &mut Emu, vc: u64) -> Result<()> {
    for a in actions(emu, vc)? {
        if bag_get(emu, a, "pomme.style") == Some(CANCEL) {
            return choose(emu, vc, a);
        }
    }
    Ok(())
}

fn set_string(emu: &mut Emu, o: u64, key: &'static str, s: u64) -> Result<()> {
    let s = if s != 0 { send(emu, s, "copy", &[])? } else { 0 };
    bag_set(emu, o, key, s);
    Ok(())
}

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    (AC, true, "alertControllerWithTitle:message:preferredStyle:", |e| {
        let (cls, title, message, style) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let vc = send(e, cls, "alloc", &[])?;
        let vc = send(e, vc, "initWithNibName:bundle:", &[0, 0])?;
        set_string(e, vc, "pomme.alertTitle", title)?;
        set_string(e, vc, "pomme.message", message)?;
        bag_set(e, vc, "pomme.alertStyle", style);
        let cls = objc::host_class(e, "NSMutableArray")?;
        let list = send(e, cls, "new", &[])?;
        bag_set(e, vc, "pomme.actions", list);
        let vc = send(e, vc, "autorelease", &[])?;
        ret(e, vc)
    }),
    // The box: a glass view with continuous corners (its content is built when it's presented).
    (AC, false, "loadView", |e| {
        let vc = e.arg(0);
        let cls = objc::host_class(e, "UIGlassEffect")?;
        let glass = send(e, cls, "effectWithStyle:", &[crate::glass::ALERT])?;
        let cls = objc::host_class(e, "UIVisualEffectView")?;
        let v = send(e, cls, "alloc", &[])?;
        let v = send(e, v, "initWithEffect:", &[glass])?;
        set_frame(e, v, [0.0, 0.0, WIDTH, 0.0]);
        round(e, v, RADIUS)?;
        send(e, vc, "setView:", &[v])?;
        Ok(())
    }),
    (AC, false, "title", |e| {
        let v = bag_get(e, e.arg(0), "pomme.alertTitle").unwrap_or(0);
        ret(e, v)
    }),
    (AC, false, "setTitle:", |e| {
        let (vc, s) = (e.arg(0), e.arg(2));
        set_string(e, vc, "pomme.alertTitle", s)
    }),
    (AC, false, "message", |e| {
        let v = bag_get(e, e.arg(0), "pomme.message").unwrap_or(0);
        ret(e, v)
    }),
    (AC, false, "setMessage:", |e| {
        let (vc, s) = (e.arg(0), e.arg(2));
        set_string(e, vc, "pomme.message", s)
    }),
    (AC, false, "preferredStyle", |e| {
        let v = bag_get(e, e.arg(0), "pomme.alertStyle").unwrap_or(1);
        ret(e, v)
    }),
    (AC, false, "addAction:", |e| {
        let (vc, a) = (e.arg(0), e.arg(2));
        let list = bag_get(e, vc, "pomme.actions").unwrap_or(0);
        if list != 0 && a != 0 {
            send(e, list, "addObject:", &[a])?;
        }
        Ok(())
    }),
    (AC, false, "actions", |e| {
        let vc = e.arg(0);
        let items = actions(e, vc)?;
        let a = foundation::new_array_of(e, &items)?;
        ret(e, a)
    }),
    (AC, false, "preferredAction", |e| {
        let v = bag_get(e, e.arg(0), "pomme.preferredAction").unwrap_or(0);
        ret(e, v)
    }),
    (AC, false, "setPreferredAction:", |e| {
        let (vc, a) = (e.arg(0), e.arg(2));
        bag_set(e, vc, "pomme.preferredAction", a);
        Ok(())
    }),
    (AC, false, "textFields", |e| ret(e, 0)),
    // iPad only: there's no popover on an iPhone.
    (AC, false, "popoverPresentationController", |e| ret(e, 0)),
    (AC, false, "_pommeAlertActionTapped:", |e| {
        let (vc, b) = (e.arg(0), e.arg(2));
        let a = bag_get(e, b, "pomme.alertAction").unwrap_or(0);
        choose(e, vc, a)
    }),
    (AC, false, "_pommeAlertOutsideTapped:", |e| {
        let vc = e.arg(0);
        tapped_outside(e, vc)
    }),
    (AA, true, "actionWithTitle:style:handler:", |e| {
        let (cls, title, style, handler) = (e.arg(0), e.arg(2), e.arg(3), e.arg(4));
        let a = send(e, cls, "new", &[])?;
        set_string(e, a, "pomme.title", title)?;
        bag_set(e, a, "pomme.style", style);
        let h = if handler != 0 { crate::libc::copy_block(e, handler)? } else { 0 };
        bag_set(e, a, "pomme.handler", h);
        let a = send(e, a, "autorelease", &[])?;
        ret(e, a)
    }),
    (AA, false, "title", |e| {
        let v = bag_get(e, e.arg(0), "pomme.title").unwrap_or(0);
        ret(e, v)
    }),
    (AA, false, "style", |e| {
        let v = bag_get(e, e.arg(0), "pomme.style").unwrap_or(0);
        ret(e, v)
    }),
    (AA, false, "isEnabled", |e| {
        let v = bag_get(e, e.arg(0), "pomme.disabled").unwrap_or(0) == 0;
        ret(e, v as u64)
    }),
    (AA, false, "setEnabled:", |e| {
        let (a, on) = (e.arg(0), e.arg(2) & 0xff != 0);
        bag_set(e, a, "pomme.disabled", (!on) as u64);
        Ok(())
    }),
];

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
// Text fields (the `field` and `fields` cases): a capsule 290 × 48, 15 pt in
// from the box, 12 pt below the header; joined one under another, with a
// separator; the buttons 20 pt below, and the box's bottom 15.67 below them.
const FIELD_INSET: f64 = 15.0;
const FIELD_TOP: f64 = 12.0;
const FIELD_HEIGHT: f64 = 48.0;
const FIELD_TEXT: [f64; 4] = [15.0, 7.0, 260.0, 35.0];
const BELOW_FIELDS: f64 = 20.0;
const BOTTOM_WITH_FIELDS: f64 = 15.67;
const SEPARATOR: [f64; 4] = [0.235, 0.235, 0.263, 0.176];
const BUTTON_HEIGHT: f64 = 48.0;
const BUTTON_GAP: f64 = 8.0;

// Colours (notes.md), light and dark.
struct Colors {
    label: [f64; 4],
    /// The message: in light, what's under it × 0.45 (black at 55%); in dark,
    /// what's under it + 153 (an additive blend: `MESSAGE_BLEND`).
    message: [f64; 4],
    /// A disabled action's title, as drawn on its fill.
    disabled: [f64; 4],
    red: [f64; 4],
    /// The preferred action's tint (its fill: render.rs, ALERT_TINTED_FILL).
    preferred: [f64; 4],
}

const LIGHT: Colors = Colors {
    label: [0.0, 0.0, 0.0, 1.0],
    message: [0.0, 0.0, 0.0, 0.55],
    disabled: [0.235, 0.235, 0.263, 0.24],
    red: [1.0, 56.0 / 255.0, 60.0 / 255.0, 1.0],
    preferred: [0.0, 0.533, 1.0, 1.0],
};

const DARK: Colors = Colors {
    label: [1.0, 1.0, 1.0, 1.0],
    message: [0.6, 0.6, 0.6, 1.0],
    disabled: [235.0 / 255.0, 235.0 / 255.0, 245.0 / 255.0, 0.25],
    red: [1.0, 66.0 / 255.0, 69.0 / 255.0, 1.0],
    preferred: [0.0, 0.569, 1.0, 1.0],
};

const WHITE: [f64; 4] = [1.0, 1.0, 1.0, 1.0];

/// The dimming behind it: black at 20% (dark, 48%).
pub(crate) fn dimming(emu: &Emu) -> [f64; 4] {
    [0.0, 0.0, 0.0, if emu.appearance.dark { 0.48 } else { 0.2 }]
}

fn colors(emu: &Emu) -> &'static Colors {
    if emu.appearance.dark { &DARK } else { &LIGHT }
}

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

/// How a header text is drawn (notes.md).
#[derive(Clone, Copy)]
struct TextStyle {
    font: (&'static str, f64, f64),
    /// The message's colours (else the label's).
    message: bool,
    /// Extra room between lines.
    line_gap: f64,
    centred: bool,
}

/// The title or message with both: semibold 17, and secondary 15 with 2 pt more between lines.
const TITLE: TextStyle = TextStyle { font: (".SFUI-Semibold", 17.0, SEMIBOLD), message: false, line_gap: 0.0, centred: false };
const MESSAGE: TextStyle = TextStyle { font: (".SFUI-Regular", 15.0, REGULAR), message: true, line_gap: MESSAGE_LINE_GAP, centred: false };
/// Either one alone: regular 17, centred.
const LONE: TextStyle = TextStyle { font: (".SFUI-Regular", 17.0, REGULAR), message: false, line_gap: 0.0, centred: true };

/// A label of `text` at y, the text's width, wrapping; returns its height.
fn label(emu: &mut Emu, parent: u64, text: &str, style: &TextStyle, y: f64, attributed: u64) -> Result<f64> {
    let TextStyle { font, message, line_gap, centred } = *style;
    // (In dark, a header text is added to what's under it: the title + 242,
    // the message + 153.)
    let color = match (message, emu.appearance.dark) {
        (true, _) => colors(emu).message,
        (false, true) => [242.0 / 255.0, 242.0 / 255.0, 242.0 / 255.0, 1.0],
        (false, false) => colors(emu).label,
    };
    let l = new_of(emu, "UILabel", [TEXT_X, y, WIDTH - 2.0 * TEXT_X, 0.0])?;
    if centred {
        bag_set(emu, l, "textAlignment", 1);
    }
    if attributed != 0 {
        // (The app's own fonts and colours, where its string has them.)
        bag_set(emu, l, "attributedText", attributed);
    }
    // (pomme's UILabel keeps these as stored properties.)
    let s = foundation::new_nsstring(emu, text)?;
    bag_set(emu, l, "text", s);
    let f = crate::uiview::new_font(emu, font.0, font.1, font.2)?;
    bag_set(emu, l, "font", f);
    let c = crate::uiview::new_color(emu, color)?;
    bag_set(emu, l, "textColor", c);
    bag_set(emu, l, "numberOfLines", 0);
    if emu.appearance.dark {
        // (Added to what's under it: Core Animation's plus-lighter filter.)
        let layer = send(emu, l, "layer", &[])?;
        let f = foundation::new_nsstring(emu, "plusL")?;
        bag_set(emu, layer, "compositingFilter", f);
    }
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
    let palette = colors(emu);
    let color = if preferred {
        WHITE
    } else if style == DESTRUCTIVE {
        palette.red
    } else {
        palette.label
    };
    // (UIKit's private `titleTextColor`, set by key, wins: React Native's dev menu.)
    let c = match bag_get(emu, action, "pomme.titleTextColor").filter(|&c| c != 0) {
        Some(c) => c,
        None => crate::uiview::new_color(emu, color)?,
    };
    send(emu, b, "setTitleColor:forState:", &[c, 0])?;
    let d = crate::uiview::new_color(emu, colors(emu).disabled)?;
    send(emu, b, "setTitleColor:forState:", &[d, 2])?;
    let tl = send(emu, b, "titleLabel", &[])?;
    let f = crate::uiview::new_font(emu, if preferred { ".SFUI-Semibold" } else { ".SFUI-Medium" }, 17.0, if preferred { SEMIBOLD } else { MEDIUM })?;
    bag_set(emu, tl, "font", f);
    let enabled = bag_get(emu, action, "pomme.disabled").unwrap_or(0) == 0;
    send(emu, b, "setEnabled:", &[enabled as u64])?;
    // The capsule's fill, behind the title: it colours the glass under it (render.rs).
    let tint = if preferred { crate::uiview::new_color(emu, colors(emu).preferred)? } else { 0 };
    let fill = fill_view(emu, if preferred { crate::glass::ALERT_TINTED_FILL } else { crate::glass::ALERT_FILL }, tint, [0.0, 0.0, frame[2], frame[3]])?;
    send(emu, b, "insertSubview:atIndex:", &[fill, 0])?;
    bag_set(emu, b, "pomme.alertAction", action);
    let sel = objc::intern(emu, "_pommeAlertActionTapped:")?;
    // UIControlEventTouchUpInside
    send(emu, b, "addTarget:action:forControlEvents:", &[vc, sel, 1 << 6])?;
    send(emu, parent, "addSubview:", &[b])?;
    Ok(())
}

/// The text fields, in the order they were added.
fn fields(emu: &mut Emu, vc: u64) -> Result<Vec<u64>> {
    let list = bag_get(emu, vc, "pomme.textFields").unwrap_or(0);
    if list == 0 {
        return Ok(Vec::new());
    }
    foundation::array_values(emu, list)
}

/// Whether it has text fields (then the keyboard comes up with it).
pub(crate) fn has_fields(emu: &mut Emu, vc: u64) -> Result<bool> {
    Ok(!fields(emu, vc)?.is_empty())
}

/// The first text field, which has the keyboard when the alert is up.
pub(crate) fn first_field(emu: &mut Emu, vc: u64) -> Result<u64> {
    Ok(fields(emu, vc)?.first().copied().unwrap_or(0))
}

/// The fields' capsules from `y` in the box: one group, its outer corners
/// round; returns where they end.
fn lay_out_fields(emu: &mut Emu, parent: u64, list: &[u64], mut y: f64) -> Result<f64> {
    let n = list.len();
    for (i, &f) in list.iter().enumerate() {
        let capsule = fill_view(emu, crate::glass::ALERT_FILL, 0, [FIELD_INSET, y, WIDTH - 2.0 * FIELD_INSET, FIELD_HEIGHT])?;
        // CACornerMask: top left 1, top right 2, bottom left 4, bottom right 8.
        let mut mask = 0;
        if i == 0 {
            mask |= 1 | 2;
        }
        if i + 1 == n {
            mask |= 4 | 8;
        }
        let l = send(emu, capsule, "layer", &[])?;
        crate::calayer::layer_mut(emu, l).masked_corners = mask;
        crate::uiview::view_mut(emu, capsule).user_interaction = true;
        send(emu, parent, "addSubview:", &[capsule])?;
        set_frame(emu, f, FIELD_TEXT);
        let font = crate::uiview::new_font(emu, ".SFUI-Regular", 17.0, REGULAR)?;
        bag_set(emu, f, "font", font);
        let content = send(emu, capsule, "contentView", &[])?;
        send(emu, content, "addSubview:", &[f])?;
        if i + 1 < n {
            let line = new_of(emu, "UIView", [FIELD_TEXT[0], FIELD_HEIGHT - 1.0, FIELD_TEXT[2], 1.0])?;
            let c = crate::uiview::new_color(emu, SEPARATOR)?;
            send(emu, line, "setBackgroundColor:", &[c])?;
            send(emu, content, "addSubview:", &[line])?;
        }
        y += FIELD_HEIGHT;
    }
    Ok(y)
}

/// A rounded view of one of the alert's fills (render.rs), at `frame`.
fn fill_view(emu: &mut Emu, style: u64, tint: u64, frame: [f64; 4]) -> Result<u64> {
    let cls = objc::host_class(emu, "UIGlassEffect")?;
    let effect = send(emu, cls, "effectWithStyle:", &[style])?;
    if tint != 0 {
        bag_set(emu, effect, "pommeTint", tint);
    }
    let cls = objc::host_class(emu, "UIVisualEffectView")?;
    let v = send(emu, cls, "alloc", &[])?;
    let v = send(emu, v, "initWithEffect:", &[effect])?;
    set_frame(emu, v, frame);
    round(emu, v, frame[3] / 2.0)?;
    crate::uiview::view_mut(emu, v).user_interaction = false;
    Ok(v)
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
    let at = bag_get(emu, vc, "pomme.attributedTitle").unwrap_or(0);
    let am = bag_get(emu, vc, "pomme.attributedMessage").unwrap_or(0);
    // The header: both texts (a semibold title, a secondary message, left-aligned),
    // or the one alone (regular 17, centred).
    let mut y = TEXT_TOP;
    match (&title, &message) {
        (Some(t), Some(m)) => {
            y += label(emu, content, t, &TITLE, y, at)?;
            y += TEXT_GAP;
            y += label(emu, content, m, &MESSAGE, y, am)?;
            y += BELOW_MESSAGE;
        }
        (Some(t), None) | (None, Some(t)) => {
            let a = if title.is_some() { at } else { am };
            y += label(emu, content, t, &LONE, y, a)?;
            y += BELOW_LONE;
        }
        (None, None) => y = 0.0,
    }
    // The actions: two side by side in an alert, else stacked.
    let n = actions(emu, vc)?.len();
    let side_by_side = n == 2 && !is_action_sheet(emu, vc);
    let order = shown_order(emu, vc, side_by_side)?;
    let inner = WIDTH - 2.0 * BUTTON_INSET;
    let list = fields(emu, vc)?;
    let mut top = if list.is_empty() {
        y + BUTTON_INSET
    } else {
        lay_out_fields(emu, content, &list, y + FIELD_TOP)? + BELOW_FIELDS
    };
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
    // For accessibility the box is one container named by its title (the
    // Simulator's "Sheet", tests/fixtures/alerts/ios26-ax.txt); the screen
    // behind stays.
    let name = bag_get(emu, vc, "pomme.alertTitle").or_else(|| bag_get(emu, vc, "pomme.message")).unwrap_or(0);
    send(emu, bx, "setAccessibilityLabel:", &[name])?;
    let bottom = if list.is_empty() { BUTTON_INSET } else { BOTTOM_WITH_FIELDS };
    let height = if n > 0 { top + bottom } else { y };
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

/// An attributed title or message: its text, and the string itself (drawn as given).
fn set_attributed(emu: &mut Emu, vc: u64, a: u64, text_key: &'static str, key: &'static str) -> Result<()> {
    let s = if a != 0 { send(emu, a, "string", &[])? } else { 0 };
    set_string(emu, vc, text_key, s)?;
    let a = if a != 0 { send(emu, a, "copy", &[])? } else { 0 };
    bag_set(emu, vc, key, a);
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
    // UIKit's private attributed title and message, set by key (React Native's
    // dev menu: `setValue:forKey:@"_attributedTitle"`): their text.
    (AC, false, "set_attributedTitle:", |e| {
        let (vc, a) = (e.arg(0), e.arg(2));
        set_attributed(e, vc, a, "pomme.alertTitle", "pomme.attributedTitle")
    }),
    (AC, false, "set_attributedMessage:", |e| {
        let (vc, a) = (e.arg(0), e.arg(2));
        set_attributed(e, vc, a, "pomme.message", "pomme.attributedMessage")
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
    (AC, false, "textFields", |e| {
        let vc = e.arg(0);
        let list = fields(e, vc)?;
        let a = foundation::new_array_of(e, &list)?;
        ret(e, a)
    }),
    // A text field (the app configures it in the block), shown below the message.
    (AC, false, "addTextFieldWithConfigurationHandler:", |e| {
        let (vc, handler) = (e.arg(0), e.arg(2));
        let f = new_of(e, "UITextField", FIELD_TEXT)?;
        let mut list = bag_get(e, vc, "pomme.textFields").unwrap_or(0);
        if list == 0 {
            let cls = objc::host_class(e, "NSMutableArray")?;
            list = send(e, cls, "new", &[])?;
            bag_set(e, vc, "pomme.textFields", list);
        }
        send(e, list, "addObject:", &[f])?;
        send(e, f, "release", &[])?;
        if handler != 0 {
            crate::libsystem::call_block(e, handler, &[f])?;
        }
        Ok(())
    }),
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
    (AA, false, "setTitleTextColor:", |e| {
        let (a, c) = (e.arg(0), e.arg(2));
        if c != 0 {
            send(e, c, "retain", &[])?;
        }
        bag_set(e, a, "pomme.titleTextColor", c);
        Ok(())
    }),
    (AA, false, "titleTextColor", |e| {
        let v = bag_get(e, e.arg(0), "pomme.titleTextColor").unwrap_or(0);
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

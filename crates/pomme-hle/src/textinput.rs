//! Text input: the first responder, UITextField editing, UIKit's text input
//! protocols (UIKeyInput, the UITextPosition / UITextRange part of UITextInput),
//! and the software keyboard (its frame and the
//! show / hide notifications apps lay out around).
//!
//! UITextField follows UIKit's contract, which React Native's RCTUITextField
//! (and its delegate adapter) is written against:
//! - a tap or `becomeFirstResponder` asks the delegate
//!   (`textFieldShouldBeginEditing:`), becomes first responder, tells it
//!   (`textFieldDidBeginEditing:`), sends `EditingDidBegin`, posts
//!   `UITextFieldTextDidBeginEditingNotification` and shows the keyboard;
//! - each keystroke asks `textField:shouldChangeCharactersInRange:replacementString:`;
//!   if allowed the text changes, `EditingChanged` is sent and
//!   `UITextFieldTextDidChangeNotification` posted (the delegate may also apply
//!   the change itself and answer NO, as React Native does);
//! - return asks `textFieldShouldReturn:`, then sends `EditingDidEndOnExit`
//!   (which also ends editing when someone listens for it);
//! - `resignFirstResponder` mirrors the beginning and hides the keyboard.
//!
//! Text is UTF-16 internally, as NSRange counts. Input is scripted like touches:
//! `--type 'text@secs'` types into the first responder at a virtual time.

use crate::control::send_actions;
use crate::foundation::{self, nsstring};
use crate::objc::{self, host_class, send};
use crate::{Emu, HostFn, Result};
use std::collections::HashMap;

// UIControlEvents
pub const EDITING_DID_BEGIN: u64 = 1 << 16;
pub const EDITING_CHANGED: u64 = 1 << 17;
pub const EDITING_DID_END: u64 = 1 << 18;
pub const EDITING_DID_END_ON_EXIT: u64 = 1 << 19;

/// How long the keyboard takes to slide in or out (its height is the
/// device's: see device.rs).
pub const KEYBOARD_DURATION: f64 = 0.25;
/// UIViewAnimationCurve the keyboard animates with (UIKit's private "keyboard" curve).
const KEYBOARD_CURVE: i64 = 7;

#[derive(Clone, Debug, Default)]
struct Field {
    text: Vec<u16>,
    /// Selection [start, end) in UTF-16 units.
    sel: (usize, usize),
    editing: bool,
}

/// Typing scripted at a virtual time: characters, `\n` for return, `\u{8}` for delete.
#[derive(Clone, Debug)]
pub struct KeyInput {
    pub at_ns: u64,
    pub text: String,
}

#[derive(Default)]
pub struct State {
    pub first_responder: u64,
    fields: HashMap<u64, Field>,
    pub keyboard_shown: bool,
    /// The keyboard's height while it's up (it depends on the input's traits).
    pub keyboard_height: f64,
    /// The keyboard sliding in or out: (start, visible height from, to).
    slide: Option<(u64, f64, f64)>,
    pub script: Vec<KeyInput>,
}

fn ret(emu: &mut Emu, v: u64) -> Result<()> {
    emu.ret(v);
    Ok(())
}

fn ret_rect(emu: &mut Emu, r: [f64; 4]) -> Result<()> {
    for (i, v) in r.iter().enumerate() {
        emu.cpu.q[i] = v.to_bits() as u128;
    }
    Ok(())
}

fn rect_arg(emu: &Emu) -> [f64; 4] {
    [0, 1, 2, 3].map(|i| f64::from_bits(emu.cpu.q[i] as u64))
}

fn field(emu: &mut Emu, f: u64) -> &mut Field {
    emu.input.fields.entry(f).or_default()
}

fn responds(emu: &mut Emu, obj: u64, sel: &str) -> Result<bool> {
    if obj == 0 {
        return Ok(false);
    }
    let s = objc::intern(emu, sel)?;
    objc::responds(emu, obj, s)
}

/// Ask an optional delegate method returning BOOL; `default` when not implemented.
fn ask(emu: &mut Emu, delegate: u64, sel: &str, args: &[u64], default: bool) -> Result<bool> {
    if !responds(emu, delegate, sel)? {
        return Ok(default);
    }
    Ok(send(emu, delegate, sel, args)? & 0xff != 0)
}

fn tell(emu: &mut Emu, delegate: u64, sel: &str, args: &[u64]) -> Result<()> {
    if responds(emu, delegate, sel)? {
        send(emu, delegate, sel, args)?;
    }
    Ok(())
}

pub fn is_editing(emu: &Emu, f: u64) -> bool {
    emu.input.fields.get(&f).is_some_and(|x| x.editing)
}

pub fn is_text_field(emu: &mut Emu, v: u64) -> bool {
    let Ok(cls) = host_class(emu, "UITextField") else { return false };
    objc::is_kind_of(emu, v, cls).unwrap_or(false)
}

pub fn is_text_view(emu: &mut Emu, v: u64) -> bool {
    let Ok(cls) = host_class(emu, "UITextView") else { return false };
    objc::is_kind_of(emu, v, cls).unwrap_or(false)
}

/// The two editable text controls: UITextField (one line) and UITextView
/// (multiline; see textview.rs). They edit the same way and differ in their
/// delegate protocol, notifications and what return does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Field,
    View,
}

pub(crate) fn kind(emu: &mut Emu, v: u64) -> Option<Kind> {
    if v == 0 {
        None
    } else if is_text_field(emu, v) {
        Some(Kind::Field)
    } else if is_text_view(emu, v) {
        Some(Kind::View)
    } else {
        None
    }
}

/// A kind's delegate methods and notifications.
struct Names {
    should_begin: &'static str,
    did_begin: &'static str,
    should_end: &'static str,
    did_end: &'static str,
    should_change: &'static str,
    began: &'static str,
    changed: &'static str,
    ended: &'static str,
}

const FIELD_NAMES: Names = Names {
    should_begin: "textFieldShouldBeginEditing:",
    did_begin: "textFieldDidBeginEditing:",
    should_end: "textFieldShouldEndEditing:",
    did_end: "textFieldDidEndEditing:",
    should_change: "textField:shouldChangeCharactersInRange:replacementString:",
    began: "UITextFieldTextDidBeginEditingNotification",
    changed: "UITextFieldTextDidChangeNotification",
    ended: "UITextFieldTextDidEndEditingNotification",
};

const VIEW_NAMES: Names = Names {
    should_begin: "textViewShouldBeginEditing:",
    did_begin: "textViewDidBeginEditing:",
    should_end: "textViewShouldEndEditing:",
    did_end: "textViewDidEndEditing:",
    should_change: "textView:shouldChangeTextInRange:replacementText:",
    began: "UITextViewTextDidBeginEditingNotification",
    changed: "UITextViewTextDidChangeNotification",
    ended: "UITextViewTextDidEndEditingNotification",
};

fn names(k: Kind) -> &'static Names {
    match k {
        Kind::Field => &FIELD_NAMES,
        Kind::View => &VIEW_NAMES,
    }
}

// ---- first responder ----

/// `-[UIResponder becomeFirstResponder]` for responders that can.
fn become_generic(emu: &mut Emu) -> Result<()> {
    let r = emu.arg(0);
    if emu.input.first_responder == r {
        return ret(emu, 1);
    }
    if send(emu, r, "canBecomeFirstResponder", &[])? & 0xff == 0 {
        return ret(emu, 0);
    }
    let old = emu.input.first_responder;
    let keys = takes_keys(emu, r)?;
    if old != 0 {
        // Handing focus between text inputs keeps the keyboard up.
        let shown = emu.input.keyboard_shown;
        if keys {
            emu.input.keyboard_shown = false;
        }
        let resigned = send(emu, old, "resignFirstResponder", &[])? & 0xff != 0;
        emu.input.keyboard_shown = shown;
        if !resigned {
            return ret(emu, 0);
        }
    }
    emu.input.first_responder = r;
    if keys {
        show_keyboard(emu)?;
    }
    ret(emu, 1)
}

/// A responder that adopts UIKeyInput (Flutter's text input view, …): the
/// keyboard comes up while it's first responder.
fn takes_keys(emu: &mut Emu, r: u64) -> Result<bool> {
    responds(emu, r, "insertText:")
}

fn resign_generic(emu: &mut Emu) -> Result<()> {
    let r = emu.arg(0);
    if emu.input.first_responder == r {
        emu.input.first_responder = 0;
        if takes_keys(emu, r)? {
            hide_keyboard(emu)?;
        }
    }
    ret(emu, 1)
}

/// `-[UIView endEditing:]`: the first responder resigns if it's this view or inside it.
fn end_editing(emu: &mut Emu) -> Result<()> {
    let v = emu.arg(0);
    let fr = emu.input.first_responder;
    let mut x = fr;
    while x != 0 && x != v {
        x = crate::uiview::view(emu, x).superview;
    }
    if fr != 0 && x == v {
        let r = send(emu, fr, "resignFirstResponder", &[])?;
        return ret(emu, r & 1);
    }
    ret(emu, 1)
}

// ---- UITextField ----

fn tf_become(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    if emu.input.first_responder == f {
        return ret(emu, 1);
    }
    let k = kind(emu, f).unwrap_or(Kind::Field);
    // A text view that isn't editable can't be typed into (selecting text in it isn't modelled).
    if k == Kind::View && !crate::textview::is_editable(emu, f) {
        return ret(emu, 0);
    }
    let n = names(k);
    let delegate = send(emu, f, "delegate", &[])?;
    if !ask(emu, delegate, n.should_begin, &[f], true)? {
        return ret(emu, 0);
    }
    // Another field ending its editing hands the keyboard over (it stays up).
    let old = emu.input.first_responder;
    if old != 0 {
        let switching = kind(emu, old).is_some();
        let shown = emu.input.keyboard_shown;
        if switching {
            emu.input.keyboard_shown = false; // so the old field doesn't hide it
        }
        let resigned = send(emu, old, "resignFirstResponder", &[])? & 0xff != 0;
        emu.input.keyboard_shown = shown;
        if !resigned {
            return ret(emu, 0);
        }
    }
    emu.input.first_responder = f;
    let fd = field(emu, f);
    fd.editing = true;
    // A text field puts the caret at the end; a text view keeps its selection.
    if k == Kind::Field {
        let end = fd.text.len();
        fd.sel = (end, end);
    }
    tell(emu, delegate, n.did_begin, &[f])?;
    if k == Kind::Field {
        send_actions(emu, f, EDITING_DID_BEGIN, 0)?;
    }
    foundation::post_named(emu, n.began, f)?;
    show_keyboard(emu)?;
    ret(emu, 1)
}

fn tf_resign(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    if emu.input.first_responder != f {
        return ret(emu, 1);
    }
    let k = kind(emu, f).unwrap_or(Kind::Field);
    let n = names(k);
    let delegate = send(emu, f, "delegate", &[])?;
    if !ask(emu, delegate, n.should_end, &[f], true)? {
        return ret(emu, 0);
    }
    emu.input.first_responder = 0;
    field(emu, f).editing = false;
    if k == Kind::Field && responds(emu, delegate, "textFieldDidEndEditing:reason:")? {
        send(emu, delegate, "textFieldDidEndEditing:reason:", &[f, 0])?;
    } else {
        tell(emu, delegate, n.did_end, &[f])?;
    }
    if k == Kind::Field {
        send_actions(emu, f, EDITING_DID_END, 0)?;
    }
    foundation::post_named(emu, n.ended, f)?;
    hide_keyboard(emu)?;
    ret(emu, 1)
}

fn tf_text(emu: &mut Emu, f: u64) -> String {
    String::from_utf16_lossy(&field(emu, f).text)
}

/// A text control's text in UTF-16 units.
pub(crate) fn text_units(emu: &mut Emu, f: u64) -> Vec<u16> {
    field(emu, f).text.clone()
}

/// A text control's selection [start, end) in UTF-16 units.
pub(crate) fn selection(emu: &mut Emu, f: u64) -> (usize, usize) {
    field(emu, f).sel
}

pub(crate) fn set_selection(emu: &mut Emu, f: u64, a: usize, b: usize) {
    let fd = field(emu, f);
    let n = fd.text.len();
    fd.sel = (a.min(n), b.min(n));
}

fn set_text(emu: &mut Emu, f: u64, s: &str) {
    let fd = field(emu, f);
    fd.text = s.encode_utf16().collect();
    let end = fd.text.len();
    fd.sel = (end, end);
}

/// `-attributedText`: the text in the default attributes.
fn tf_attributed_text(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    let text = tf_text(emu, f);
    let s = foundation::new_nsstring(emu, &text)?;
    let attrs = match kind(emu, f) {
        Some(Kind::View) => send(emu, f, "typingAttributes", &[])?,
        _ => send(emu, f, "defaultTextAttributes", &[])?,
    };
    let cls = host_class(emu, "NSAttributedString")?;
    let a = send(emu, cls, "alloc", &[])?;
    let a = send(emu, a, "initWithString:attributes:", &[s, attrs])?;
    ret(emu, a)
}

fn tf_set_attributed_text(emu: &mut Emu) -> Result<()> {
    let (f, a) = (emu.arg(0), emu.arg(2));
    let text = if a == 0 {
        String::new()
    } else {
        let s = send(emu, a, "string", &[])?;
        nsstring(emu, s)?
    };
    set_text(emu, f, &text);
    Ok(())
}

fn tf_get_text(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    let text = tf_text(emu, f);
    let s = foundation::new_nsstring(emu, &text)?;
    ret(emu, s)
}

fn tf_set_text(emu: &mut Emu) -> Result<()> {
    let (f, s) = (emu.arg(0), emu.arg(2));
    let text = if s == 0 { String::new() } else { nsstring(emu, s)? };
    set_text(emu, f, &text);
    Ok(())
}

/// Replace `range` with `s` the way a keystroke does: ask the delegate, edit, notify.
fn edit(emu: &mut Emu, f: u64, range: (usize, usize), s: &str) -> Result<()> {
    let k = kind(emu, f).unwrap_or(Kind::Field);
    let delegate = send(emu, f, "delegate", &[])?;
    let ns = foundation::new_nsstring(emu, s)?;
    let (loc, len) = (range.0 as u64, (range.1 - range.0) as u64);
    if !ask(emu, delegate, names(k).should_change, &[f, loc, len, ns], true)? {
        return Ok(());
    }
    let fd = field(emu, f);
    let (a, b) = (range.0.min(fd.text.len()), range.1.min(fd.text.len()));
    let units: Vec<u16> = s.encode_utf16().collect();
    let n = units.len();
    fd.text.splice(a..b, units);
    fd.sel = (a + n, a + n);
    match k {
        Kind::Field => send_actions(emu, f, EDITING_CHANGED, 0)?,
        // UITextView: the selection moves first, then the text has changed
        // (React Native depends on this order).
        Kind::View => {
            tell(emu, delegate, "textViewDidChangeSelection:", &[f])?;
            tell(emu, delegate, "textViewDidChange:", &[f])?;
        }
    }
    foundation::post_named(emu, names(k).changed, f)?;
    Ok(())
}

/// Replace a text field's selection with `s`, as typing or pasting does.
pub(crate) fn replace_selection(emu: &mut Emu, f: u64, s: &str) -> Result<()> {
    let sel = field(emu, f).sel;
    edit(emu, f, sel, s)
}

/// A text field's selected text, if anything is selected.
pub(crate) fn selected_text(emu: &mut Emu, f: u64) -> Option<String> {
    let fd = field(emu, f);
    let (a, b) = (fd.sel.0.min(fd.text.len()), fd.sel.1.min(fd.text.len()));
    (a < b).then(|| String::from_utf16_lossy(&fd.text[a..b]))
}

pub(crate) fn select_all(emu: &mut Emu, f: u64) {
    let fd = field(emu, f);
    fd.sel = (0, fd.text.len());
}

/// One key on the keyboard, sent to the first responder.
pub fn key(emu: &mut Emu, ch: char) -> Result<()> {
    let f = emu.input.first_responder;
    if f == 0 {
        return Ok(());
    }
    let Some(k) = kind(emu, f) else {
        // Another UIKeyInput responder (Flutter's text input view, …). Like
        // the iOS keyboard, ask a UITextInput first whether the range may
        // change (Flutter performs return's action there and answers NO).
        let text = if ch == '\u{8}' { String::new() } else { ch.to_string() };
        let s = foundation::new_nsstring(emu, &text)?;
        if responds(emu, f, "shouldChangeTextInRange:replacementText:")? {
            let range = change_range(emu, f, ch == '\u{8}')?;
            if range != 0 && send(emu, f, "shouldChangeTextInRange:replacementText:", &[range, s])? & 0xff == 0 {
                return Ok(());
            }
        }
        if ch == '\u{8}' {
            send(emu, f, "deleteBackward", &[])?;
        } else {
            send(emu, f, "insertText:", &[s])?;
        }
        return Ok(());
    };
    if k == Kind::View && !crate::textview::is_editable(emu, f) {
        return Ok(());
    }
    match ch {
        // Return starts a new line in a text view.
        '\n' if k == Kind::View => {
            let sel = field(emu, f).sel;
            edit(emu, f, sel, "\n")?;
        }
        '\n' => {
            let delegate = send(emu, f, "delegate", &[])?;
            if ask(emu, delegate, "textFieldShouldReturn:", &[f], true)? {
                let ends = crate::control::has_targets(emu, f, EDITING_DID_END_ON_EXIT);
                send_actions(emu, f, EDITING_DID_END_ON_EXIT, 0)?;
                if ends && emu.input.first_responder == f {
                    send(emu, f, "resignFirstResponder", &[])?;
                }
            }
        }
        '\u{8}' => {
            let fd = field(emu, f).clone();
            let range = if fd.sel.0 != fd.sel.1 {
                fd.sel
            } else if fd.sel.0 == 0 {
                return Ok(());
            } else {
                // One character back (a surrogate pair is one character).
                let i = fd.sel.0;
                let back = if i >= 2 && (0xDC00..0xE000).contains(&fd.text[i - 1]) && (0xD800..0xDC00).contains(&fd.text[i - 2]) { 2 } else { 1 };
                (i - back, i)
            };
            edit(emu, f, range, "")?;
        }
        c => {
            let sel = field(emu, f).sel;
            edit(emu, f, sel, &c.to_string())?;
        }
    }
    // As the user types, a text view scrolls to keep the caret in sight.
    if k == Kind::View && emu.input.first_responder == f {
        crate::textview::scroll_to_caret(emu, f)?;
    }
    Ok(())
}

/// The range a keystroke replaces in a UITextInput: the selection, or for
/// delete with nothing selected, the character before the caret.
fn change_range(emu: &mut Emu, input: u64, delete: bool) -> Result<u64> {
    let sel = send(emu, input, "selectedTextRange", &[])?;
    if sel == 0 || !delete || send(emu, sel, "isEmpty", &[])? & 0xff == 0 {
        return Ok(sel);
    }
    let end = send(emu, sel, "start", &[])?;
    let start = send(emu, input, "positionFromPosition:offset:", &[end, (-1i64) as u64])?;
    if start == 0 {
        return Ok(sel);
    }
    send(emu, input, "textRangeFromPosition:toPosition:", &[start, end])
}

fn tf_insert_text(emu: &mut Emu) -> Result<()> {
    let (f, s) = (emu.arg(0), emu.arg(2));
    let text = nsstring(emu, s)?;
    let sel = field(emu, f).sel;
    edit(emu, f, sel, &text)
}

fn tf_delete_backward(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    let saved = emu.input.first_responder;
    emu.input.first_responder = f;
    let r = key(emu, '\u{8}');
    emu.input.first_responder = saved;
    r
}

/// A tap ends on the field: it starts editing (unless it's disabled).
fn tf_touches_ended(emu: &mut Emu) -> Result<()> {
    let (f, touches, event) = (emu.arg(0), emu.arg(2), emu.arg(3));
    crate::control::touches_ended(emu, f, touches, event)?;
    let enabled = send(emu, f, "isEnabled", &[])? & 0xff != 0;
    if enabled && emu.input.first_responder != f {
        send(emu, f, "becomeFirstResponder", &[])?;
    }
    Ok(())
}

// ---- UITextPosition / UITextRange ----

fn position(emu: &mut Emu, offset: usize) -> Result<u64> {
    let cls = host_class(emu, "UITextPosition")?;
    let p = objc::create_instance(emu, cls, 0)?;
    objc::bag_set(emu, p, "offset", offset as u64);
    Ok(p)
}

fn offset_of(emu: &Emu, p: u64) -> usize {
    objc::bag_get(emu, p, "offset").unwrap_or(0) as usize
}

fn text_range(emu: &mut Emu, a: usize, b: usize) -> Result<u64> {
    let (a, b) = (a.min(b), a.max(b));
    let (pa, pb) = (position(emu, a)?, position(emu, b)?);
    let cls = host_class(emu, "UITextRange")?;
    let r = objc::create_instance(emu, cls, 0)?;
    objc::bag_set(emu, r, "start", pa);
    objc::bag_set(emu, r, "end", pb);
    Ok(r)
}

fn range_offsets(emu: &Emu, r: u64) -> (usize, usize) {
    let (s, e) = (objc::bag_get(emu, r, "start").unwrap_or(0), objc::bag_get(emu, r, "end").unwrap_or(0));
    (offset_of(emu, s), offset_of(emu, e))
}

fn tf_selected_range(emu: &mut Emu) -> Result<()> {
    let f = emu.arg(0);
    let (a, b) = field(emu, f).sel;
    let r = text_range(emu, a, b)?;
    ret(emu, r)
}

fn tf_set_selected_range(emu: &mut Emu) -> Result<()> {
    let (f, r) = (emu.arg(0), emu.arg(2));
    if r != 0 {
        let (a, b) = range_offsets(emu, r);
        let fd = field(emu, f);
        let n = fd.text.len();
        fd.sel = (a.min(n), b.min(n));
    }
    Ok(())
}

fn tf_position_offset(emu: &mut Emu) -> Result<()> {
    let (f, p, off) = (emu.arg(0), emu.arg(2), emu.arg(3) as i64);
    let n = field(emu, f).text.len() as i64;
    let o = offset_of(emu, p) as i64 + off;
    if p == 0 || o < 0 || o > n {
        return ret(emu, 0);
    }
    let r = position(emu, o as usize)?;
    ret(emu, r)
}

/// `positionFromPosition:inDirection:offset:`: right/down (0, 3) forward, left/up back.
fn tf_position_direction(emu: &mut Emu) -> Result<()> {
    let (f, p, dir, off) = (emu.arg(0), emu.arg(2), emu.arg(3), emu.arg(4) as i64);
    let n = field(emu, f).text.len() as i64;
    let step = if matches!(dir, 0 | 3) { off } else { -off };
    let o = (offset_of(emu, p) as i64 + step).clamp(0, n);
    let r = position(emu, o as usize)?;
    ret(emu, r)
}

const TF: &str = "UITextField";
const TV: &str = "UITextView";
const TP: &str = "UITextPosition";
const TR: &str = "UITextRange";

/// What UITextField and UITextView share: editing, the first responder and
/// UITextInput's positions and ranges.
macro_rules! text_methods {
    ($c:expr) => {
        [
        ($c, false, "canBecomeFirstResponder", |e| ret(e, 1)),
        ($c, false, "becomeFirstResponder", tf_become),
        ($c, false, "resignFirstResponder", tf_resign),
        ($c, false, "isEditing", |e| {
            let f = e.arg(0);
            let v = field(e, f).editing as u64;
            ret(e, v)
        }),
        ($c, false, "delegate", |e| {
            let d = objc::bag_get(e, e.arg(0), "delegate").unwrap_or(0);
            ret(e, d)
        }),
        ($c, false, "setDelegate:", |e| {
            let (f, d) = (e.arg(0), e.arg(2));
            objc::bag_set(e, f, "delegate", d);
            Ok(())
        }),
        ($c, false, "text", tf_get_text),
        ($c, false, "setText:", tf_set_text),
        ($c, false, "attributedText", tf_attributed_text),
        ($c, false, "setAttributedText:", tf_set_attributed_text),
        ($c, false, "hasText", |e| {
            let f = e.arg(0);
            let v = !field(e, f).text.is_empty() as u64;
            ret(e, v)
        }),
        ($c, false, "insertText:", tf_insert_text),
        ($c, false, "deleteBackward", tf_delete_backward),
        ($c, false, "caretRectForPosition:", |e| {
            let b = crate::uiview::view(e, e.arg(0)).bounds;
            ret_rect(e, [0.0, 0.0, 2.0, b[3]])
        }),
        ($c, false, "firstRectForRange:", |e| {
            let b = crate::uiview::view(e, e.arg(0)).bounds;
            ret_rect(e, [0.0, 0.0, b[2], b[3]])
        }),
        ($c, false, "selectionRectsForRange:", |e| {
            let a = foundation::new_array_of(e, &[])?;
            ret(e, a)
        }),
        // UITextInput positions and ranges
        ($c, false, "beginningOfDocument", |e| {
            let p = position(e, 0)?;
            ret(e, p)
        }),
        ($c, false, "endOfDocument", |e| {
            let f = e.arg(0);
            let n = field(e, f).text.len();
            let p = position(e, n)?;
            ret(e, p)
        }),
        ($c, false, "selectedTextRange", tf_selected_range),
        ($c, false, "setSelectedTextRange:", tf_set_selected_range),
        ($c, false, "markedTextRange", |e| ret(e, 0)),
        ($c, false, "positionFromPosition:offset:", tf_position_offset),
        ($c, false, "positionFromPosition:inDirection:offset:", tf_position_direction),
        ($c, false, "textRangeFromPosition:toPosition:", |e| {
            let (a, b) = (e.arg(2), e.arg(3));
            if a == 0 || b == 0 {
                return ret(e, 0);
            }
            let (oa, ob) = (offset_of(e, a), offset_of(e, b));
            let r = text_range(e, oa, ob)?;
            ret(e, r)
        }),
        ($c, false, "offsetFromPosition:toPosition:", |e| {
            let (a, b) = (e.arg(2), e.arg(3));
            let d = offset_of(e, b) as i64 - offset_of(e, a) as i64;
            ret(e, d as u64)
        }),
        ($c, false, "comparePosition:toPosition:", |e| {
            let (a, b) = (offset_of(e, e.arg(2)), offset_of(e, e.arg(3)));
            // NSComparisonResult: ascending -1, same 0, descending 1.
            ret(e, a.cmp(&b) as i64 as u64)
        }),
        ($c, false, "textInRange:", |e| {
            let (f, r) = (e.arg(0), e.arg(2));
            let (a, b) = range_offsets(e, r);
            let fd = field(e, f);
            let (a, b) = (a.min(fd.text.len()), b.min(fd.text.len()));
            let s = String::from_utf16_lossy(&fd.text[a..b]);
            let ns = foundation::new_nsstring(e, &s)?;
            ret(e, ns)
        }),
        ($c, false, "replaceRange:withText:", |e| {
            let (f, r, s) = (e.arg(0), e.arg(2), e.arg(3));
            let range = range_offsets(e, r);
            let text = nsstring(e, s)?;
            edit(e, f, range, &text)
        }),
        ]
    };
}

pub(crate) const TEXT_METHODS: [&[(&str, bool, &str, HostFn)]; 2] = [&text_methods!(TF), &text_methods!(TV)];

pub(crate) const METHODS: &[(&str, bool, &str, HostFn)] = &[
    // UIResponder
    ("UIResponder", false, "becomeFirstResponder", become_generic),
    ("UIResponder", false, "resignFirstResponder", resign_generic),
    ("UIResponder", false, "canBecomeFirstResponder", |e| ret(e, 0)),
    ("UIResponder", false, "canResignFirstResponder", |e| ret(e, 1)),
    ("UIResponder", false, "isFirstResponder", |e| {
        let r = (e.input.first_responder == e.arg(0)) as u64;
        ret(e, r)
    }),
    ("UIResponder", false, "reloadInputViews", |_| Ok(())),
    // No undo support: nil, as for a responder chain without a window's undo manager.
    ("UIResponder", false, "undoManager", |e| ret(e, 0)),
    ("UIResponder", false, "inputView", |e| ret(e, 0)),
    ("UIResponder", false, "inputAccessoryView", |e| ret(e, 0)),
    ("UIResponder", false, "textInputMode", |e| ret(e, 0)),
    ("UIView", false, "becomeFirstResponder", become_generic),
    ("UIView", false, "resignFirstResponder", resign_generic),
    ("UIView", false, "canBecomeFirstResponder", |e| ret(e, 0)),
    ("UIView", false, "isFirstResponder", |e| {
        let r = (e.input.first_responder == e.arg(0)) as u64;
        ret(e, r)
    }),
    ("UIView", false, "endEditing:", end_editing),
    // UITextField only (the rest is shared with UITextView: TEXT_METHODS)
    // A text field is a control: it tracks touches like one (see control.rs).
    (TF, false, "touchesBegan:withEvent:", |e| {
        let (c, t, ev) = (e.arg(0), e.arg(2), e.arg(3));
        crate::control::touches_began(e, c, t, ev)
    }),
    (TF, false, "touchesMoved:withEvent:", |e| {
        let (c, t, ev) = (e.arg(0), e.arg(2), e.arg(3));
        crate::control::touches_moved(e, c, t, ev)
    }),
    (TF, false, "touchesEnded:withEvent:", tf_touches_ended),
    (TF, false, "touchesCancelled:withEvent:", |e| {
        let (c, t, ev) = (e.arg(0), e.arg(2), e.arg(3));
        crate::control::touches_cancelled(e, c, t, ev)
    }),
    // A text view isn't a control (it's a scroll view): a tap on an editable one starts editing.
    (TV, false, "touchesEnded:withEvent:", |e| {
        let v = e.arg(0);
        let editable = send(e, v, "isEditable", &[])? & 0xff != 0;
        if editable && e.input.first_responder != v {
            send(e, v, "becomeFirstResponder", &[])?;
        }
        Ok(())
    }),
    // UIKit's: the field's font (17 pt system unless set) and text color; or what the app set.
    (TF, false, "defaultTextAttributes", |e| {
        let f = e.arg(0);
        if let Some(a) = objc::bag_get(e, f, "defaultTextAttributes").filter(|&a| a != 0) {
            return ret(e, a);
        }
        let font = match objc::bag_get(e, f, "font").filter(|&x| x != 0) {
            Some(x) => x,
            None => crate::uiview::new_font(e, ".SFUI-Regular", 17.0, 0.0)?,
        };
        let mut pairs = vec![(foundation::new_nsstring(e, "NSFontAttributeName")?, font)];
        if let Some(c) = objc::bag_get(e, f, "textColor").filter(|&x| x != 0) {
            pairs.push((foundation::new_nsstring(e, "NSForegroundColorAttributeName")?, c));
        }
        let d = foundation::new_dict_of(e, &pairs)?;
        ret(e, d)
    }),
    // UITextField: geometry (no border, no side views: the bounds)
    (TF, false, "textRectForBounds:", |e| {
        let r = rect_arg(e);
        ret_rect(e, r)
    }),
    (TF, false, "editingRectForBounds:", |e| {
        let r = rect_arg(e);
        ret_rect(e, r)
    }),
    (TF, false, "placeholderRectForBounds:", |e| {
        let r = rect_arg(e);
        ret_rect(e, r)
    }),
    (TP, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = b != 0 && offset_of(e, a) == offset_of(e, b);
        ret(e, same as u64)
    }),
    (TR, false, "start", |e| {
        let v = objc::bag_get(e, e.arg(0), "start").unwrap_or(0);
        ret(e, v)
    }),
    (TR, false, "end", |e| {
        let v = objc::bag_get(e, e.arg(0), "end").unwrap_or(0);
        ret(e, v)
    }),
    (TR, false, "isEmpty", |e| {
        let (a, b) = range_offsets(e, e.arg(0));
        ret(e, (a == b) as u64)
    }),
    (TR, false, "isEqual:", |e| {
        let (a, b) = (e.arg(0), e.arg(2));
        let same = b != 0 && range_offsets(e, a) == range_offsets(e, b);
        ret(e, same as u64)
    }),
];

// ---- the keyboard ----

/// The keyboard's frame in screen coordinates while shown (and below the screen while hidden).
pub fn keyboard_frame(screen: (f64, f64), height: f64, shown: bool) -> [f64; 4] {
    let (w, h) = screen;
    let y = if shown { h - height } else { h };
    [0.0, y, w, height]
}

/// A UITextInputTraits value of the responder (0, "default", if it has none).
fn trait_value(emu: &mut Emu, r: u64, name: &str) -> Result<u64> {
    if let Some(v) = objc::bag_get(emu, r, name) {
        return Ok(v);
    }
    if responds(emu, r, name)? { send(emu, r, name, &[]).map(|v| v & 0xffff_ffff) } else { Ok(0) }
}

/// The keyboard height for `r`: without the predictive bar when autocorrection
/// and spell checking are both off (UITextAutocorrectionTypeNo, UITextSpellCheckingTypeNo).
fn keyboard_height_for(emu: &mut Emu, r: u64) -> Result<f64> {
    const NO: u64 = 1;
    let off = trait_value(emu, r, "autocorrectionType")? == NO && trait_value(emu, r, "spellCheckingType")? == NO;
    Ok(crate::orientation::keyboard_height(emu, !off))
}

/// The keyboard notifications' userInfo, with the keys iOS 26 sends (measured).
fn keyboard_info(emu: &mut Emu, from: [f64; 4], to: [f64; 4]) -> Result<u64> {
    let begin = crate::nsvalue::new_rect_value(emu, from)?;
    let end = crate::nsvalue::new_rect_value(emu, to)?;
    let center = |r: [f64; 4]| [r[0] + r[2] / 2.0, r[1] + r[3] / 2.0];
    let center_begin = crate::nsvalue::new_point_value(emu, center(from))?;
    let center_end = crate::nsvalue::new_point_value(emu, center(to))?;
    let bounds = crate::nsvalue::new_rect_value(emu, [0.0, 0.0, to[2], to[3]])?;
    let duration = foundation::new_number_f64(emu, KEYBOARD_DURATION)?;
    let curve = foundation::new_number_i64(emu, KEYBOARD_CURVE)?;
    let local = foundation::new_number_i64(emu, 1)?;
    let mut pairs = Vec::new();
    for (k, v) in [
        ("UIKeyboardFrameBeginUserInfoKey", begin),
        ("UIKeyboardFrameEndUserInfoKey", end),
        ("UIKeyboardCenterBeginUserInfoKey", center_begin),
        ("UIKeyboardCenterEndUserInfoKey", center_end),
        ("UIKeyboardBoundsUserInfoKey", bounds),
        ("UIKeyboardAnimationDurationUserInfoKey", duration),
        ("UIKeyboardAnimationCurveUserInfoKey", curve),
        ("UIKeyboardIsLocalUserInfoKey", local),
    ] {
        let k = foundation::new_nsstring(emu, k)?;
        pairs.push((k, v));
    }
    foundation::new_dict_of(emu, &pairs)
}

fn post_keyboard(emu: &mut Emu, names: &[&str], shown: bool) -> Result<()> {
    let h = emu.input.keyboard_height;
    let screen = crate::orientation::screen(emu);
    let info = keyboard_info(emu, keyboard_frame(screen, h, !shown), keyboard_frame(screen, h, shown))?;
    for n in names {
        foundation::post_named_info(emu, n, 0, info)?;
    }
    Ok(())
}

fn did_show(emu: &mut Emu) -> Result<()> {
    if emu.input.keyboard_shown {
        post_keyboard(emu, &["UIKeyboardDidShowNotification", "UIKeyboardDidChangeFrameNotification"], true)?;
    }
    Ok(())
}

fn did_hide(emu: &mut Emu) -> Result<()> {
    if !emu.input.keyboard_shown {
        post_keyboard(emu, &["UIKeyboardDidHideNotification", "UIKeyboardDidChangeFrameNotification"], false)?;
    }
    Ok(())
}

/// How much of the keyboard is on screen now (points from the bottom edge):
/// it slides over KEYBOARD_DURATION, easing out, as the iOS keyboard does.
pub fn keyboard_visible(emu: &Emu) -> f64 {
    let rest = if emu.input.keyboard_shown { emu.input.keyboard_height } else { 0.0 };
    let Some((start, from, to)) = emu.input.slide else { return rest };
    let t = (crate::threads::now_ns(emu).saturating_sub(start)) as f64 / 1e9 / KEYBOARD_DURATION;
    if t >= 1.0 {
        return to;
    }
    let eased = 1.0 - (1.0 - t).powi(3);
    from + (to - from) * eased
}

/// Start the keyboard sliding to `to` points visible, laying out each frame.
fn slide_keyboard(emu: &mut Emu, to: f64) -> Result<()> {
    let from = keyboard_visible(emu);
    let start = crate::threads::now_ns(emu);
    emu.input.slide = Some((start, from, to));
    crate::uikit::after_on_main(emu, 1.0 / 60.0, slide_frame, start)
}

fn slide_frame(emu: &mut Emu) -> Result<()> {
    let start = emu.arg(1);
    let Some((s, _, _)) = emu.input.slide.filter(|x| x.0 == start) else { return Ok(()) };
    crate::autolayout::mark_dirty(emu);
    crate::host::frame_changed(emu);
    if crate::threads::now_ns(emu).saturating_sub(s) as f64 / 1e9 < KEYBOARD_DURATION {
        crate::uikit::after_on_main(emu, 1.0 / 60.0, slide_frame, start)?;
    } else {
        emu.input.slide = None;
    }
    Ok(())
}

/// The keyboard slides up: will-notifications now, did-notifications when it's there.
pub fn show_keyboard(emu: &mut Emu) -> Result<()> {
    if emu.input.keyboard_shown {
        return Ok(());
    }
    let fr = emu.input.first_responder;
    emu.input.keyboard_height = keyboard_height_for(emu, fr)?;
    let h = emu.input.keyboard_height;
    slide_keyboard(emu, h)?;
    emu.input.keyboard_shown = true;
    post_keyboard(emu, &["UIKeyboardWillChangeFrameNotification", "UIKeyboardWillShowNotification"], true)?;
    crate::uikit::after_on_main(emu, KEYBOARD_DURATION, did_show, 0)?;
    crate::host::frame_changed(emu);
    Ok(())
}

/// The interface turned with the keyboard up: it comes back at the new
/// orientation's height, across the new screen, without sliding.
pub fn keyboard_reoriented(emu: &mut Emu, old_screen: (f64, f64)) -> Result<()> {
    if !emu.input.keyboard_shown {
        return Ok(());
    }
    let old = keyboard_frame(old_screen, emu.input.keyboard_height, true);
    let fr = emu.input.first_responder;
    emu.input.keyboard_height = keyboard_height_for(emu, fr)?;
    emu.input.slide = None;
    let new = keyboard_frame(crate::orientation::screen(emu), emu.input.keyboard_height, true);
    let info = keyboard_info(emu, old, new)?;
    for n in ["UIKeyboardWillChangeFrameNotification", "UIKeyboardWillShowNotification", "UIKeyboardDidChangeFrameNotification", "UIKeyboardDidShowNotification"] {
        foundation::post_named_info(emu, n, 0, info)?;
    }
    Ok(())
}

pub fn hide_keyboard(emu: &mut Emu) -> Result<()> {
    if !emu.input.keyboard_shown {
        return Ok(());
    }
    slide_keyboard(emu, 0.0)?;
    emu.input.keyboard_shown = false;
    post_keyboard(emu, &["UIKeyboardWillChangeFrameNotification", "UIKeyboardWillHideNotification"], false)?;
    crate::uikit::after_on_main(emu, KEYBOARD_DURATION, did_hide, 0)?;
    crate::host::frame_changed(emu);
    Ok(())
}

// ---- scripted typing ----

fn fire_typing(emu: &mut Emu) -> Result<()> {
    let i = emu.arg(1) as usize;
    let Some(inp) = emu.input.script.get(i).cloned() else { return Ok(()) };
    for ch in inp.text.chars() {
        key(emu, ch)?;
    }
    Ok(())
}

/// Put the scripted typing on the main run loop.
pub fn schedule_script(emu: &mut Emu) -> Result<()> {
    let now = crate::threads::now_ns(emu);
    for i in 0..emu.input.script.len() {
        let at = emu.input.script[i].at_ns;
        let delay = at.saturating_sub(now) as f64 / 1e9;
        crate::uikit::after_on_main(emu, delay, fire_typing, i as u64)?;
    }
    Ok(())
}

/// Text as typed on the command line: `\n` is return, `\b` delete, `\\` a backslash.
pub fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('b') => out.push('\u{8}'),
            Some('t') => out.push('\t'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

// ---- drawing ----

/// What a text field shows: its text (or placeholder), font size, weight and color.
pub fn appearance(emu: &mut Emu, f: u64) -> Result<Option<(String, f64, f64, [f64; 4])>> {
    let mut text = tf_text(emu, f);
    let attrs;
    let mut color_default = [0.0, 0.0, 0.0, 1.0];
    if text.is_empty() {
        let ph = send(emu, f, "attributedPlaceholder", &[])?;
        if ph == 0 {
            return Ok(None);
        }
        let s = send(emu, ph, "string", &[])?;
        text = nsstring(emu, s)?;
        if text.is_empty() {
            return Ok(None);
        }
        attrs = send(emu, ph, "attributesAtIndex:effectiveRange:", &[0, 0])?;
        color_default = [0.235, 0.235, 0.263, 0.3]; // placeholderTextColor
    } else {
        attrs = send(emu, f, "defaultTextAttributes", &[])?;
        if objc::bag_get(emu, f, "secureTextEntry").unwrap_or(0) & 0xff != 0 {
            text = "•".repeat(text.chars().count());
        }
    }
    let font = crate::textkit::font_in_attributes(emu, attrs)?;
    let (size, weight) = crate::textkit::font_info(emu, font)?;
    let mut color = color_default;
    if attrs != 0 {
        let key = foundation::new_nsstring(emu, "NSColor")?;
        let mut c = send(emu, attrs, "objectForKey:", &[key])?;
        if c == 0 {
            let key = foundation::new_nsstring(emu, "NSForegroundColorAttributeName")?;
            c = send(emu, attrs, "objectForKey:", &[key])?;
        }
        if let Some(rgba) = emu.ui.views.colors.get(&c) {
            color = *rgba;
        }
    }
    Ok(Some((text, size, weight, color)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_text_escapes() {
        assert_eq!(unescape(r"Ada\n"), "Ada\n");
        assert_eq!(unescape(r"ab\bc"), "ab\u{8}c");
        assert_eq!(unescape(r"a\\b"), "a\\b");
    }

    #[test]
    fn keyboard_sits_on_the_bottom_edge() {
        use crate::device::Orientation::*;
        let d = crate::device::default();
        for o in [Portrait, LandscapeLeft] {
            let (w, h) = d.screen(o);
            let shown = keyboard_frame((w, h), d.keyboard_height(o, true), true);
            assert_eq!((shown[1] + shown[3], shown[2]), (h, w));
            assert_eq!(keyboard_frame((w, h), d.keyboard_height(o, true), false)[1], h);
        }
    }
}

//! End to end: a native UIKit app with buttons (tests/fixtures/controls/main.m):
//! UIControl's touch tracking and target-action. Taps, drags out of and back
//! into a button, a disabled button, a nil target found up the responder
//! chain, a subclass that declines to track, and a pan that cancels the
//! button's touch, and buttons in scroll views (which hold touches back and
//! may or may not take them from a control). Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::touch::{self, Input, BEGAN, ENDED, MOVED};

/// The text of the text field tagged `tag`.
fn field(views: &str, tag: u64) -> String {
    let line = views.lines().find(|l| l.trim_start().starts_with("UITextField (") && l.contains(&format!(" tag {tag} "))).unwrap_or_else(|| panic!("no field {tag} in\n{views}"));
    line.split_once(" \"").map(|(_, t)| t.trim_end_matches('"').to_string()).unwrap_or_default()
}

fn run(script: impl FnOnce(&mut Vec<Input>)) -> Option<String> {
    let mut emu = common::boot("controls")?;
    script(&mut emu.touch.script);
    Some(common::views(&mut emu, 2.0))
}

const S: u64 = 1_000_000_000;

/// A tap: TouchDown (highlighted, tracking), then TouchUpInside (no longer
/// highlighted, still inside, with the touch's event) and the button's
/// primary action.
#[test]
fn tap_fires_down_and_up_inside() {
    let Some(v) = run(|s| touch::tap(s, S, 115.0, 125.0)) else { return };
    assert_eq!(field(&v, 100), "down(h1 t1) up(h0 in1 touches1) primary", "{v}");
    // The title is a label filling the button.
    let at = v.find("UIButton (40.0, 100.0, 150.0×50.0)").unwrap_or_else(|| panic!("no button in\n{v}"));
    assert!(v[at..].lines().nth(1).unwrap().trim_start().starts_with("UILabel (0.0, 0.0, 150.0×50.0) \"Tap\""), "{v}");
}

/// Dragging well away (past UIKit's 70 pt slop) exits and lifts outside: no primary action.
#[test]
fn drag_out_fires_up_outside() {
    let Some(v) = run(|s| touch::swipe(s, S, (115.0, 125.0), (115.0, 400.0), 300_000_000)) else { return };
    assert_eq!(field(&v, 100), "down(h1 t1) exit(h0) upOutside", "{v}");
}

/// Within the slop still counts as inside.
#[test]
fn slop_counts_as_inside() {
    let Some(v) = run(|s| {
        s.push(Input { at_ns: S, phase: BEGAN, x: 115.0, y: 125.0 });
        s.push(Input { at_ns: S + 50_000_000, phase: MOVED, x: 115.0, y: 200.0 });
        s.push(Input { at_ns: S + 100_000_000, phase: ENDED, x: 115.0, y: 200.0 });
    }) else { return };
    assert_eq!(field(&v, 100), "down(h1 t1) up(h0 in1 touches1) primary", "{v}");
}

/// Out and back in: DragExit, DragEnter, then up inside.
#[test]
fn drag_out_and_back() {
    let Some(v) = run(|s| {
        s.push(Input { at_ns: S, phase: BEGAN, x: 115.0, y: 125.0 });
        s.push(Input { at_ns: S + 50_000_000, phase: MOVED, x: 115.0, y: 400.0 });
        s.push(Input { at_ns: S + 100_000_000, phase: MOVED, x: 115.0, y: 130.0 });
        s.push(Input { at_ns: S + 150_000_000, phase: ENDED, x: 115.0, y: 130.0 });
    }) else { return };
    assert_eq!(field(&v, 100), "down(h1 t1) exit(h0) enter up(h0 in1 touches1) primary", "{v}");
}

/// A disabled button sends nothing; its touches go on to the view behind it.
#[test]
fn disabled_button_passes_touches_on() {
    let Some(v) = run(|s| touch::tap(s, S, 115.0, 215.0)) else { return };
    assert_eq!(field(&v, 101), "container", "{v}");
}

/// A nil target: the action travels the responder chain (button → views →
/// the view controller, which implements it).
#[test]
fn nil_target_goes_up_the_responder_chain() {
    let Some(v) = run(|s| touch::tap(s, S, 115.0, 305.0)) else { return };
    assert_eq!(field(&v, 102), "chain Chain", "{v}");
}

/// A subclass's beginTrackingWithTouch: is asked (with the touch, in the
/// button's coordinates); answering NO means no actions and no endTracking.
#[test]
fn subclass_declines_tracking() {
    let Some(v) = run(|s| touch::tap(s, S, 115.0, 385.0)) else { return };
    assert_eq!(field(&v, 103), "begin(75)", "{v}");
}

/// A pan recognizer on the button's superview takes the dragging touch: the
/// button gets touchesCancelled, unhighlights and sends TouchCancel.
#[test]
fn gesture_recognizer_cancels_tracking() {
    let Some(v) = run(|s| touch::swipe(s, S, (115.0, 465.0), (300.0, 465.0), 200_000_000)) else { return };
    assert_eq!(field(&v, 104), "down cancel(h0) pan", "{v}");
}

/// A press, resting past the scroll view's delay, then a drag.
fn hold_then_drag(s: &mut Vec<Input>, x: f64, y: f64) {
    s.push(Input { at_ns: S, phase: BEGAN, x, y });
    s.push(Input { at_ns: S + 300_000_000, phase: MOVED, x, y: y - 45.0 });
    s.push(Input { at_ns: S + 400_000_000, phase: ENDED, x, y: y - 45.0 });
}

/// A quick tap on a button in a scroll view: held back, then all at once on lift.
#[test]
fn tap_in_scroll_view() {
    let Some(v) = run(|s| touch::tap(s, S, 295.0, 145.0)) else { return };
    assert_eq!(field(&v, 105), "down up", "{v}");
}

/// A quick drag from a button scrolls: the button never heard of the touch.
#[test]
fn quick_drag_from_button_scrolls() {
    let Some(v) = run(|s| touch::swipe(s, S, (295.0, 145.0), (295.0, 45.0), 100_000_000)) else { return };
    assert_eq!(field(&v, 105), "drag", "{v}");
}

/// Once the button has the touch, a plain scroll view leaves it there: no
/// scrolling, and the finger (still within the slop) lifts inside.
#[test]
fn plain_scroll_view_leaves_a_control_its_touch() {
    let Some(v) = run(|s| hold_then_drag(s, 295.0, 145.0)) else { return };
    assert_eq!(field(&v, 105), "down up", "{v}");
}

/// A list takes it: the drag scrolls and the button's tracking is cancelled.
#[test]
fn list_takes_the_touch_from_a_control() {
    let Some(v) = run(|s| hold_then_drag(s, 295.0, 365.0)) else { return };
    assert_eq!(field(&v, 106), "down drag cancel", "{v}");
}

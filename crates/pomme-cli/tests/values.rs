//! End to end: a native UIKit app with switches, sliders and segmented
//! controls (tests/fixtures/values/main.m): taps and drags change their
//! values and send ValueChanged; setting them in code doesn't. Built with the
//! iOS SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::touch::{self, Input, BEGAN, ENDED, MOVED};

/// The text of the text field tagged `tag`.
fn field(views: &str, tag: u64) -> String {
    let line = views.lines().find(|l| l.trim_start().starts_with("UITextField (") && l.contains(&format!(" tag {tag} "))).unwrap_or_else(|| panic!("no field {tag} in\n{views}"));
    line.split_once(" \"").map(|(_, t)| t.trim_end_matches('"').to_string()).unwrap_or_default()
}

fn run(script: impl FnOnce(&mut Vec<Input>)) -> Option<String> {
    let mut emu = common::boot("values")?;
    script(&mut emu.touch.script);
    Some(common::views(&mut emu, 4.0))
}

const S: u64 = 1_000_000_000;

/// A press at `from`, a move every 1/60 s to `to`, and a lift there.
fn drag(s: &mut Vec<Input>, from: (f64, f64), to: (f64, f64)) {
    touch::swipe(s, S, from, to, 300_000_000);
    // The swipe lifts slightly past `to` (a flick); lift exactly there instead.
    let last = s.last_mut().unwrap();
    (last.x, last.y) = to;
}

/// Natural sizes: a switch is 51×31 whatever frame it's given; a slider is
/// 31 tall; a segmented control made from items is its own size (each
/// segment the widest 13-pt title, as pomme measures it, plus 12 pt a side).
/// Inserting and removing segments keeps titles in order.
#[test]
fn sizes_and_segments() {
    let Some(v) = run(|_| {}) else { return };
    assert_eq!(field(&v, 205), "switch {51, 31} on 1 slider {300, 31} fresh {104, 32} 2 Zero,One sel -1", "{v}");
    // The asked-for 100×100 became 51×31.
    assert!(v.contains("UISwitch (40.0, 100.0, 51.0×31.0)"), "{v}");
}

/// Taps flip switches, each sending ValueChanged.
#[test]
fn taps_flip_switches() {
    let Some(v) = run(|s| {
        touch::tap(s, S, 65.0, 115.0);
        touch::tap(s, 2 * S, 65.0, 175.0);
    }) else { return };
    assert_eq!(field(&v, 200), "off-switch 1 on-switch 0", "{v}");
}

/// Dragging a switch leaves it on the side the finger ends on: off → on;
/// an on switch dragged further on doesn't change (no event).
#[test]
fn drags_set_switches() {
    let Some(v) = run(|s| {
        drag(s, (45.0, 115.0), (85.0, 115.0));
        s.extend([Input { at_ns: 2 * S, phase: BEGAN, x: 50.0, y: 175.0 }, Input { at_ns: 2 * S + 100_000_000, phase: MOVED, x: 88.0, y: 175.0 }, Input { at_ns: 2 * S + 200_000_000, phase: ENDED, x: 88.0, y: 175.0 }]);
    }) else { return };
    assert_eq!(field(&v, 200), "off-switch 1", "{v}");
}

/// The thumb (at 25 of 0…100: x 68…96 on a 300-pt slider) follows the
/// finger: 136 pt right is half the 272-pt travel, so 75. ValueChanged comes
/// as it moves, while tracking.
#[test]
fn dragging_the_thumb() {
    let Some(v) = run(|s| drag(s, (122.0, 255.0), (258.0, 255.0))) else { return };
    let log = field(&v, 201);
    assert!(log.ends_with("value 75 tracking 1"), "{log}");
    let n: u32 = log.split_whitespace().nth(1).unwrap().parse().unwrap();
    assert!(n > 5, "one change per move: {log}");
}

/// Past the end it stops at the maximum.
#[test]
fn thumb_stops_at_the_end() {
    let Some(v) = run(|s| drag(s, (122.0, 255.0), (390.0, 255.0))) else { return };
    assert!(field(&v, 201).ends_with("value 100 tracking 1"), "{v}");
}

/// A touch on the track away from the thumb doesn't move it.
#[test]
fn track_touches_dont_move_the_thumb() {
    let Some(v) = run(|s| drag(s, (300.0, 255.0), (200.0, 255.0))) else { return };
    assert_eq!(field(&v, 201), "", "{v}");
}

/// A slider that isn't continuous reports once, when let go.
#[test]
fn settled_slider_reports_once() {
    let Some(v) = run(|s| drag(s, (122.0, 315.0), (258.0, 315.0))) else { return };
    assert_eq!(field(&v, 202), "changes 1 value 75", "{v}");
}

/// Tapping a segment selects it; tapping the selected one again or a
/// disabled one does nothing.
#[test]
fn segments_select() {
    let Some(v) = run(|s| {
        touch::tap(s, S, 190.0, 396.0);
        touch::tap(s, 2 * S, 190.0, 396.0);
        touch::tap(s, 3 * S, 290.0, 396.0);
    }) else { return };
    assert_eq!(field(&v, 203), "1", "{v}");
}

/// A momentary control reports the segment, then lets go of it.
#[test]
fn momentary_segments_let_go() {
    let Some(v) = run(|s| touch::tap(s, S, 240.0, 456.0)) else { return };
    assert_eq!(field(&v, 204), "1 then -1", "{v}");
}

//! End to end: a native UIKit app with a scroll view (tests/fixtures/scroll/main.m):
//! dragging, flinging, snapping through the delegate, rubber-banding at the
//! edge, and taps on the content. Built with the iOS SDK; skipped without
//! Xcode or the sysroot.

mod common;

use pomme_hle::Emu;
use pomme_hle::touch;

/// The text of the text field tagged `tag`.
fn field(views: &str, tag: u64) -> String {
    let line = views.lines().find(|l| l.trim_start().starts_with("UITextField (") && l.contains(&format!(" tag {tag} "))).unwrap_or_else(|| panic!("no field {tag} in\n{views}"));
    line.split_once(" \"").map(|(_, t)| t.trim_end_matches('"').to_string()).unwrap_or_default()
}

fn run(script: impl FnOnce(&mut Vec<touch::Input>), secs: f64) -> Option<(Emu, String)> {
    let mut emu = common::boot("scroll")?;
    script(&mut emu.touch.script);
    let v = common::views(&mut emu, secs);
    Some((emu, v))
}

/// A quick upward swipe (200 pt in 0.2 s: 1 pt/ms) drags the content 190 pt
/// (the first 10 pt only start the drag), then it decelerates: UIKit's rate
/// carries it 499 pt further, to 689, and the delegate snaps that to 700.
/// The row under the finger never hears of the touch: the drag began within
/// the scroll view's 150 ms delay.
#[test]
fn fling_decelerates_and_snaps() {
    let Some((_, v)) = run(|s| touch::swipe(s, 1_000_000_000, (195.0, 450.0), (195.0, 250.0), 200_000_000), 5.0) else { return };
    assert_eq!(field(&v, 100), "begin 1 end 1 decelerate 1 stop 1 tap 0 cancel 0", "{v}");
    assert_eq!(field(&v, 101), "offset 700 dragging 0 decelerating 0 scrolled 1", "{v}");
}

/// Mid-glide, the content is still moving.
#[test]
fn still_decelerating() {
    let Some((_, v)) = run(|s| touch::swipe(s, 1_000_000_000, (195.0, 450.0), (195.0, 250.0), 200_000_000), 1.5) else { return };
    let pos = field(&v, 101);
    let offset: f64 = pos.split_whitespace().nth(1).unwrap().parse().unwrap();
    assert!(offset > 190.0 && offset < 700.0, "{pos}");
    assert!(pos.ends_with("dragging 0 decelerating 1 scrolled 1"), "{pos}");
}

/// Pulling down at the top rubber-bands (the content follows at half speed)
/// and springs back to 0 when let go.
#[test]
fn rubber_band_at_the_top() {
    let Some((_, mid)) = run(|s| {
        s.push(touch::Input { at_ns: 1_000_000_000, phase: touch::BEGAN, x: 195.0, y: 250.0 });
        s.push(touch::Input { at_ns: 1_100_000_000, phase: touch::MOVED, x: 195.0, y: 450.0 });
    }, 2.0) else { return };
    // 200 pt down, less the 10 pt that started the drag, at half speed.
    assert_eq!(field(&mid, 101), "offset -95 dragging 1 decelerating 0 scrolled 1", "{mid}");
    let (_, after) = run(|s| touch::swipe(s, 1_000_000_000, (195.0, 250.0), (195.0, 450.0), 500_000_000), 5.0).unwrap();
    assert_eq!(field(&after, 101), "offset 0 dragging 0 decelerating 0 scrolled 1", "{after}");
}

/// A tap (no movement) still reaches the row; nothing scrolls.
#[test]
fn taps_reach_the_content() {
    let Some((_, v)) = run(|s| touch::tap(s, 1_000_000_000, 195.0, 300.0), 2.0) else { return };
    assert_eq!(field(&v, 100), "begin 0 end 0 decelerate 0 stop 0 tap 1 cancel 0", "{v}");
    assert_eq!(field(&v, 101), "offset 0 dragging 0 decelerating 0 scrolled 0", "{v}");
}

/// A finger that rests before dragging: the row gets the touch after the
/// delay, then loses it (touchesCancelled) when the drag starts. The finger
/// stops at 90 before lifting; the delegate's snapping glides it to 100.
#[test]
fn hold_then_drag_cancels_the_row() {
    let Some((_, v)) = run(|s| {
        s.push(touch::Input { at_ns: 1_000_000_000, phase: touch::BEGAN, x: 195.0, y: 450.0 });
        s.push(touch::Input { at_ns: 1_300_000_000, phase: touch::MOVED, x: 195.0, y: 350.0 });
        s.push(touch::Input { at_ns: 1_400_000_000, phase: touch::ENDED, x: 195.0, y: 350.0 });
    }, 3.0) else { return };
    assert_eq!(field(&v, 100), "begin 1 end 1 decelerate 1 stop 1 tap 0 cancel 1", "{v}");
    assert_eq!(field(&v, 101), "offset 100 dragging 0 decelerating 0 scrolled 1", "{v}");
}

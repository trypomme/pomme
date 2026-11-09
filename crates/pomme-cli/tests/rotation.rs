//! End to end: a native UIKit app turning with the phone
//! (tests/fixtures/rotation/main.m). What it hears, and in what order, is
//! what the iOS 26 Simulator showed for the same app. Built with the iOS
//! SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::device::Orientation::{self, *};

/// The text of field `tag` in the view dump.
fn field(views: &str, tag: u64) -> String {
    let line = views.lines().find(|l| l.trim_start().starts_with("UITextField (") && l.contains(&format!(" tag {tag} "))).unwrap_or_else(|| panic!("no field {tag} in\n{views}"));
    line.split_once(" \"").map(|(_, t)| t.trim_end_matches('"').to_string()).unwrap_or_default()
}

/// Launch held `held`, turn the phone at the given times, run for `secs`:
/// (the log of the last turn, the geometry after it).
fn run(held: Orientation, turns: &[(f64, Orientation)], taps: &[(f64, f64, f64)], secs: f64) -> Option<(String, String)> {
    let mut emu = common::boot("rotation")?;
    emu.ui.device_orientation = held;
    emu.ui.rotations = turns.iter().map(|&(t, o)| ((t * 1e9) as u64, o)).collect();
    for &(x, y, t) in taps {
        pomme_hle::touch::tap(&mut emu.touch.script, (t * 1e9) as u64, x, y);
    }
    let v = common::views(&mut emu, secs);
    Some((field(&v, 1), field(&v, 2)))
}

/// Turning to landscape: the measured sequence. The root view controller
/// hears it first, at its old size, and passes it to its child; the status
/// bar's notifications bracket the new geometry (margins first, then safe
/// areas, subviews before the controller); traits change, layout runs at the
/// new size, the alongside animation and the scene follow, and the
/// coordinator completes 0.3 s later. On the iPhone 16e (the default).
#[test]
fn turning_to_landscape() {
    let Some((log, state)) = run(Portrait, &[(1.0, LandscapeRight)], &[], 2.0) else { return };
    let expected = [
        "device 3 animated 1",
        "willTraits v1",
        "willSize 844x390 from 390x844 in 0.3",
        "childSize 844x390",
        "willStatusBar 3",
        "margins 0,63,20,63",
        "childSafe 0,47,20,47",
        "safe 0,47,20,47",
        "didStatusBar 1",
        "statusBarFrame 390x47",
        "traits v2 -> v1",
        "layout 844x390",
        "alongside 844x390",
        "scene from 390x844 1 now 3",
        "done",
    ];
    assert_eq!(log, expected.join("; "));
    assert_eq!(state, "screen 844x390 window 844x390 scene 3 device 3 statusBar 0x0 hidden 1 classes h1 v1");
}

/// And back: portrait's geometry returns, and the status bar.
#[test]
fn turning_back_to_portrait() {
    let Some((log, state)) = run(Portrait, &[(1.0, LandscapeLeft), (2.0, Portrait)], &[], 3.0) else { return };
    assert!(log.starts_with("device 1 animated 1; willTraits v2; willSize 390x844 from 844x390"), "{log}");
    assert!(log.contains("safe 47,0,34,0") && log.contains("didStatusBar 4") && log.contains("statusBarFrame 0x0"), "{log}");
    assert_eq!(state, "screen 390x844 window 390x844 scene 1 device 1 statusBar 390x47 hidden 0 classes h1 v2");
}

/// Launched held sideways: the app starts in landscape, with no turn to hear.
#[test]
fn launching_in_landscape() {
    let Some((log, state)) = run(LandscapeLeft, &[], &[], 1.0) else { return };
    assert_eq!(log, "");
    assert_eq!(state, "screen 844x390 window 844x390 scene 4 device 4 statusBar 0x0 hidden 1 classes h1 v1");
}

/// Upside down isn't for phones with a Face ID notch: the device turns, the
/// interface stays.
#[test]
fn upside_down_stays_put() {
    let Some((log, state)) = run(Portrait, &[(1.0, PortraitUpsideDown)], &[], 2.0) else { return };
    assert_eq!(log, "", "the turn wasn't reported (the log is shown after a turn completes)");
    assert_eq!(state, "screen 390x844 window 390x844 scene 1 device 1 statusBar 390x47 hidden 0 classes h1 v2");
}

/// A view controller that stops supporting landscape
/// (setNeedsUpdateOfSupportedInterfaceOrientations) turns the interface back
/// to portrait while the phone is still held sideways.
#[test]
fn locking_to_portrait() {
    let Some((log, state)) = run(Portrait, &[(1.0, LandscapeRight)], &[(400.0, 300.0, 2.0)], 3.0) else { return };
    assert!(log.starts_with("willTraits v2; willSize 390x844 from 844x390"), "no device notification: the phone didn't move\n{log}");
    assert_eq!(state, "screen 390x844 window 390x844 scene 1 device 3 statusBar 390x47 hidden 0 classes h1 v2");
}

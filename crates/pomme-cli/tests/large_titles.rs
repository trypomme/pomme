//! Large navigation titles, held to iOS 26's: the largetitles fixture (a
//! navigation controller preferring large titles, over a scroll view) moves
//! its content step by step and reports the bar, the safe area and the
//! scroll view's adjusted inset after each; fixtures/largetitles/ios26.txt
//! has what it reports on the iOS 26 simulator, iPhone 16e
//! (scripts/swiftui/metrics-simulator.sh largetitles). Then dragging, as
//! measured there: the title collapses with the content and settles whole or
//! gone. Skipped without Xcode or the sysroot.

mod common;

use pomme_hle::touch::{BEGAN, ENDED, Input, MOVED};

/// Moved by the app: the title collapses once the content passes the usual
/// bar, and stays collapsed back at the top; the bar is 54 pt under Liquid Glass.
#[test]
fn moved_content_matches_ios() {
    let Some(mut emu) = common::boot("largetitles") else { return };
    common::views(&mut emu, 6.0);
    let wrong = common::metric_differences("largetitles", &emu.console, |_, _| false);
    assert!(wrong.is_empty(), "differences from iOS 26:\n  {}\n\npomme reported:\n{}", wrong.join("\n  "), emu.console);
}

/// A finger at (200, `from`) moving to `to` over `ms`, held there 200 ms (so
/// it lets go without speed), then lifted.
fn drag(emu: &mut pomme_hle::Emu, at_s: f64, from: f64, to: f64, ms: u64) {
    let at = (at_s * 1e9) as u64;
    let script = &mut emu.touch.script;
    script.push(Input { at_ns: at, phase: BEGAN, x: 200.0, y: from });
    let steps = ms / 16;
    for i in 1..=steps {
        let y = from + (to - from) * i as f64 / steps as f64;
        script.push(Input { at_ns: at + i * 16_000_000, phase: MOVED, x: 200.0, y });
    }
    let end = at + ms * 1_000_000;
    for i in 1..=12 {
        script.push(Input { at_ns: end + i * 16_000_000, phase: MOVED, x: 200.0, y: to });
    }
    script.push(Input { at_ns: end + 210_000_000, phase: ENDED, x: 200.0, y: to });
}

/// Where things stand, from the view tree: the bar's height and the
/// content's offset.
fn state(emu: &mut pomme_hle::Emu, secs: f64) -> (f64, f64) {
    let views = common::views(emu, secs);
    let line = |name: &str| views.lines().find(|l| l.trim_start().starts_with(name)).unwrap_or_else(|| panic!("no {name}\n{views}")).to_string();
    let bar: f64 = line("UINavigationBar ").split('×').nth(1).unwrap().trim_end_matches(')').parse().unwrap();
    let scroll = line("UIScrollView ");
    let offset = scroll.split("offset (").nth(1).map_or(0.0, |o| o.split(", ").nth(1).unwrap().trim_end_matches(')').parse().unwrap());
    (bar, offset)
}

/// The probe's last reports of the scroll.
fn recent(console: &str) -> String {
    let lines: Vec<&str> = console.lines().filter(|l| l.starts_with("SCROLL ")).collect();
    lines[lines.len().saturating_sub(12)..].join("\n")
}

/// Dragged (as on the simulator): pulled down from collapsed, the title
/// opens with the content and the bar comes to rest whole; pushed up 35 pt
/// (a 45-pt move: the first 10 start the drag) and let go, it finishes
/// collapsing; pulled 35 pt back (more than half the title) and let go, it opens.
#[test]
fn dragging_collapses_and_settles_like_ios() {
    // After the probe's steps the title is collapsed, the content at its top.
    let drags: [(f64, f64, f64, u64); 3] = [(6.0, 300.0, 380.0, 400), (9.0, 600.0, 555.0, 400), (12.6, 300.0, 345.0, 300)];
    let checks = [(8.9, 106.0, "pulled open"), (12.5, 54.0, "let go 35 pt up"), (15.5, 106.0, "let go 35 pt down")];
    for (n, (secs, want_bar, what)) in checks.into_iter().enumerate() {
        // (Each from launch: a run starts the app.)
        let Some(mut emu) = common::boot("largetitles") else { return };
        for &(at, from, to, ms) in &drags[..=n] {
            drag(&mut emu, at, from, to, ms);
        }
        let (bar, offset) = state(&mut emu, secs);
        // At rest: just below the bar (the status bar's 47 pt above it).
        let rest = -(47.0 + want_bar);
        assert!((bar - want_bar).abs() < 0.5 && (offset - rest).abs() < 0.5, "{what}: bar {bar}, offset {offset} (rest {rest})\n{}", recent(&emu.console));
    }
}

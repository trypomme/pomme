//! Development builds: what Debug builds' extra code needs from Foundation
//! and UIKit (tests/fixtures/devmisc, recorded on the iOS 26 Simulator:
//! NSCountedSet, +[NSThread callStackSymbols], NSCache's limits, window
//! levels). Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use std::collections::BTreeMap;

fn values(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("VALUE "))
        .map(|l| l.split_once(' ').map_or((l.to_string(), String::new()), |(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

#[test]
fn what_debug_builds_ask_of_foundation_and_uikit() {
    let Some(mut emu) = common::boot("devmisc") else { return };
    common::views(&mut emu, 1.0);
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/devmisc/ios26.txt")).unwrap();
    let (mut want, got) = (values(&recording), values(&emu.console));
    // On iOS the stack includes UIKit's own frames (dozens). pomme's UIKit is
    // pomme's code, not the app's, so only the app's frames are there: the
    // first one (the probe's method) is checked above, the count isn't.
    // (docs/differences-from-ios.md, "Call stacks".)
    want.remove("stackHasFrames");
    let differences: Vec<String> = want.iter().filter(|(k, v)| got.get(*k) != Some(*v)).map(|(k, v)| format!("{k}: iOS {v:?}, pomme {:?}", got.get(k))).collect();
    assert!(differences.is_empty(), "{}\n\nconsole:\n{}", differences.join("\n"), emu.console);
    // The alert-level window, created first, is drawn above the normal one (blue, not red).
    let pix = pomme_hle::render::render(&mut emu, 1.0).unwrap();
    let c = pix.pixel(pix.width() / 2, pix.height() / 2).unwrap();
    assert!(c.blue() > 200 && c.red() < 50, "the middle of the screen is {c:?}, not the alert window's blue");
}

//! SwiftUI's layout, held to iOS's: the swiftuimetrics fixture reports each
//! view's frame (`METRIC name x y width height`); fixtures/swiftuimetrics/
//! ios26.txt has what it reports on the iOS 26 simulator, iPhone 16e
//! (scripts/swiftui/metrics-simulator.sh). pomme's must match within half a
//! point. A text's width isn't compared: pomme draws Inter in place of SF Pro,
//! whose glyphs are wider. Skipped without Xcode, the sysroot or pomme's SwiftUI.

mod common;

use std::path::Path;

#[test]
fn layout_matches_ios() {
    compare("swiftuimetrics");
}

/// Under a navigation bar with a large title: where the list starts.
#[test]
fn layout_under_a_large_title_matches_ios() {
    compare("swiftuinavmetrics");
}

fn compare(probe: &str) {
    let sysroot = Path::new("../../vendor/sysroot");
    if !sysroot.join("System/Library/Frameworks/SwiftUI.framework/SwiftUI").exists() {
        eprintln!("skipping: build pomme's SwiftUI first (scripts/swiftui/build.sh)");
        return;
    }
    let Some(mut emu) = common::boot(probe) else { return };
    common::views(&mut emu, 3.0);
    let wrong = common::metric_differences(probe, &emu.console, |name, i| {
        // (The texts: the button's label is one too.)
        let text = name.starts_with("text") || name.ends_with("text") || name.contains("header") || name.starts_with("hstack.") || name == "button";
        // (A text's width depends on the font; a trailing text's x does too.)
        (i == 2 && text) || (i == 0 && name.ends_with(".trailing"))
    });
    assert!(wrong.is_empty(), "differences from iOS 26:\n  {}\n\npomme reported:\n{}", wrong.join("\n  "), emu.console);
}

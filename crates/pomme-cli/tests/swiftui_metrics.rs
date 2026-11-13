//! SwiftUI's layout, held to iOS's: the swiftuimetrics fixture reports each
//! view's frame (`METRIC name x y width height`); fixtures/swiftuimetrics/
//! ios26.txt has what it reports on the iOS 26 simulator, iPhone 16e
//! (scripts/swiftui/metrics-simulator.sh). pomme's must match within half a
//! point. A text's width isn't compared: pomme draws Inter in place of SF Pro,
//! whose glyphs are wider. Skipped without Xcode, the sysroot or pomme's SwiftUI.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

type Frames = BTreeMap<String, [f64; 4]>;

/// Each view's last reported frame.
fn parse(text: &str) -> Frames {
    let mut frames = Frames::new();
    for line in text.lines().filter(|l| l.starts_with("METRIC ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let v: Vec<f64> = f[2..6].iter().map(|x| x.parse().unwrap()).collect();
        frames.insert(f[1].to_string(), [v[0], v[1], v[2], v[3]]);
    }
    frames
}

#[test]
fn layout_matches_ios() {
    let sysroot = Path::new("../../vendor/sysroot");
    if !sysroot.join("System/Library/Frameworks/SwiftUI.framework/SwiftUI").exists() {
        eprintln!("skipping: build pomme's SwiftUI first (scripts/swiftui/build.sh)");
        return;
    }
    let Some(mut emu) = common::boot("swiftuimetrics") else { return };
    common::views(&mut emu, 3.0);
    let ours = parse(&emu.console);
    let ios = parse(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/swiftuimetrics/ios26.txt")).unwrap());
    let mut wrong = Vec::new();
    for (name, want) in &ios {
        let Some(got) = ours.get(name) else {
            wrong.push(format!("{name}: not reported"));
            continue;
        };
        // (The texts: the button's label is one too.)
        let text = name.starts_with("text") || name.ends_with("text") || name.contains("header") || name.starts_with("hstack.") || name == "button";
        for (i, what) in ["x", "y", "width", "height"].iter().enumerate() {
            // (A text's width depends on the font; a trailing text's x does too.)
            if (i == 2 && text) || (i == 0 && name.ends_with(".trailing")) {
                continue;
            }
            if (got[i] - want[i]).abs() > 0.5 {
                wrong.push(format!("{name}: {what} {:.2}, iOS {:.2}", got[i], want[i]));
            }
        }
    }
    assert!(wrong.is_empty(), "differences from iOS 26:\n  {}\n\npomme reported:\n{}", wrong.join("\n  "), emu.console);
}

//! End to end: Expo UI (`@expo/ui/swift-ui`), SwiftUI views from React, on
//! pomme's SwiftUI and Combine. Skipped unless the app
//! (scripts/build-app.sh hello-expo-ui) and the sysroot (with
//! scripts/combine/build.sh and scripts/swiftui/build.sh) have been built.

use pomme_hle::touch::{self, BEGAN, ENDED, Input};
use pomme_hle::{Emu, Stop};
use std::path::Path;

const APP: &str = "../../apps/hello-expo-ui/ios/build/Build/Products/Release-iphoneos/helloexpoui.app";
const SYSROOT: &str = "../../vendor/sysroot";

// Where the controls are (points; from `pomme run --views`).
const BUTTON: (f64, f64) = (150.0, 378.0);
const SWITCH: (f64, f64) = (328.0, 173.0);
const SLIDER_THUMB: (f64, f64) = (195.0, 278.0);

fn boot() -> Option<Emu> {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    let frameworks = sysroot.join("System/Library/Frameworks");
    if !app.exists() || !frameworks.join("SwiftUI.framework/SwiftUI").exists() || !frameworks.join("Combine.framework/Combine").exists() {
        eprintln!("skipping: build hello-expo-ui, Combine and SwiftUI first");
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    Some(Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap())
}

fn tap(emu: &mut Emu, (x, y): (f64, f64), secs: f64) {
    let at = (secs * 1e9) as u64;
    emu.touch.script.push(Input { at_ns: at, phase: BEGAN, x, y });
    emu.touch.script.push(Input { at_ns: at + 80_000_000, phase: ENDED, x, y });
}

/// Run until the app idles past `secs` of virtual time; then the view tree.
fn run_for(emu: &mut Emu, secs: f64) -> String {
    emu.time_limit_ns = Some((secs * 1e9) as u64);
    match emu.run_main() {
        Err(Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}\n{}", emu.describe_views()),
    }
    emu.describe_views()
}

/// The form: Expo UI's Host (a SwiftUI hosting controller in React Native's
/// tree) shows a Form, its sections' headers, a text, a toggle (on), a row
/// with a Spacer between two texts, a slider at its value, and a button.
/// Laid out as on the iOS 26 simulator (iPhone 16e): content 16 pt in from
/// the groups' edges, headers as written, the iOS 26 switch (63 × 28) and
/// slider (34 tall).
#[test]
fn shows_the_form() {
    let Some(mut emu) = boot() else { return };
    let views = run_for(&mut emu, 6.0);
    for text in ["\"Expo UI\"", "\"Hello from SwiftUI\"", "\"Wi-Fi\"", "\"Brightness\"", "\"50%\"", "\"Taps\"", "\"Tapped 0 times\""] {
        assert!(views.contains(text), "{text} missing\n{views}");
    }
    let switch = views.lines().find(|l| l.contains("PommeSwitch")).expect("no switch");
    assert!(switch.contains("63.0×28.0") && switch.ends_with(" on"), "{switch}");
    let slider = views.lines().find(|l| l.contains("PommeSlider")).expect("no slider");
    assert!(slider.contains(", 326.0×34.0)") && slider.ends_with(" value 0.50"), "{slider}");
    let header = views.lines().find(|l| l.contains("\"Expo UI\"")).unwrap();
    assert!(header.contains("(32.0, "), "{header}");
    // The Spacer pushes the value to the row's trailing edge.
    let value = views.lines().find(|l| l.contains("\"50%\"")).unwrap();
    assert!(value.contains("(288.0, 0.0"), "{value}");
}

/// React state through SwiftUI and back: the button's taps count, the switch
/// turns Wi-Fi off, and dragging the slider changes the percentage.
#[test]
fn controls_update_react_state() {
    let Some(mut emu) = boot() else { return };
    tap(&mut emu, BUTTON, 3.0);
    tap(&mut emu, BUTTON, 3.5);
    tap(&mut emu, SWITCH, 4.0);
    touch::swipe(&mut emu.touch.script, (4.5 * 1e9) as u64, SLIDER_THUMB, (300.0, SLIDER_THUMB.1), 300_000_000);
    let views = run_for(&mut emu, 7.0);
    assert!(views.contains("\"Tapped 2 times\""), "{views}");
    assert!(!views.contains("\"50%\""), "the slider didn't move\n{views}");
    let switch = views.lines().find(|l| l.contains("PommeSwitch")).unwrap();
    assert!(switch.ends_with(" off"), "{switch}");
}

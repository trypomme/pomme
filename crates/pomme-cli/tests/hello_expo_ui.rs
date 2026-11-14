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
/// In the button's row, past its label: on iOS the whole row is the button.
const BUTTON: (f64, f64) = (300.0, 378.0);
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

/// The everyday set, laid out as on the iOS 26 simulator: text and secure
/// fields (body height), a picker row (label, value, chevrons), a progress
/// bar at its value (4 pt, 0.4 of the row filled in the tint), a spinner
/// (20 pt), a Group's views as rows, and a Divider in a row (a vertical
/// hairline, 22 pt). Then the modifiers: a 22 pt bold font (its line 26.3
/// tall), and a padded badge clipped to its rounded rectangle, the separator
/// below it starting at its text, as a row's separator does.
#[test]
fn shows_the_everyday_set() {
    let Some(mut emu) = boot() else { return };
    let views = run_for(&mut emu, 6.0);
    let fields: Vec<&str> = views.lines().filter(|l| l.contains("PommeTextField")).collect();
    assert_eq!(fields.len(), 2, "{views}");
    assert!(fields.iter().all(|f| f.contains(", 326.0×20.3) \"\"")), "{fields:?}");
    for text in ["\"Typed: \"", "\"Flavor\"", "\"Vanilla\"", "\"Grouped one\"", "\"Grouped two\"", "\"Modifiers\"", "\"Secondary\"", "\"Plain button\""] {
        assert!(views.contains(text), "{text} missing\n{views}");
    }
    assert!(views.contains("UIImageView (331.7, 175.0, 9.0×14.0)"), "the picker's chevrons\n{views}");
    assert!(views.contains("(16.0, 232.0, 326.0×4.0) bg #78788033 radius 2"), "the progress track\n{views}");
    assert!(views.contains("(0.0, 0.0, 130.4×4.0) bg #0088ffff radius 2"), "the progress at 0.4\n{views}");
    assert!(views.contains("UIActivityIndicatorView (16.0, 276.0, 20.0×20.0)"), "the spinner\n{views}");
    assert!(views.contains("(16.0, 379.0, 0.3×22.0) bg #c6c6c8ff"), "the divider row\n{views}");
    let bold = views.lines().find(|l| l.contains("\"Bold 22\"")).unwrap();
    assert!(bold.contains("×26.3)"), "{bold}");
    // The badge: clipped (radius 8); in it, the orange behind the label padded by 8.
    let badge = views.lines().position(|l| l.contains("\"Badge\"")).unwrap();
    let lines: Vec<&str> = views.lines().collect();
    assert!(lines[badge - 4].contains("67.0×36.3) radius 8"), "{}", lines[badge - 4]);
    assert!(lines[badge - 2].contains("(0.0, 0.0, 67.0×36.3) bg #ffa500ff"), "{}", lines[badge - 2]);
    assert!(lines[badge].contains("(8.0, 8.0, 51.0×20.3)"), "{}", lines[badge]);
    assert!(lines[badge + 1].contains("UIView (24.0, ") && lines[badge + 1].contains("318.0×1.0)"), "the separator below the badge\n{}", lines[badge + 1]);
}

/// The modifiers as drawn, scrolled to the end: the badge's corners rounded,
/// the secondary text gray, and the plain button's label in the primary
/// color rather than the tint.
#[test]
fn modifiers_draw_as_on_ios() {
    let Some(mut emu) = boot() else { return };
    // (Flung to the end, the list rests against its bottom.)
    touch::swipe(&mut emu.touch.script, 3_000_000_000, (195.0, 700.0), (195.0, 100.0), 300_000_000);
    touch::swipe(&mut emu.touch.script, 4_000_000_000, (195.0, 700.0), (195.0, 100.0), 300_000_000);
    run_for(&mut emu, 7.0);
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let at = |x: f64, y: f64| pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap();
    let orange = |x: f64, y: f64| {
        let c = at(x, y);
        c.red() > 240 && (150..180).contains(&c.green()) && c.blue() < 30
    };
    // The badge's box: its orange, found column by column at mid-height.
    let ys: Vec<f64> = (0..844 * 3).map(|y| y as f64 / 3.0).filter(|&y| orange(60.0, y)).collect();
    assert!(!ys.is_empty(), "no orange badge");
    let (top, bottom) = (ys[0], *ys.last().unwrap());
    assert!((bottom - top - 36.3).abs() < 1.0, "the badge is {} tall", bottom - top);
    let mid = (top + bottom) / 2.0;
    let xs: Vec<f64> = (0..390 * 3).map(|x| x as f64 / 3.0).filter(|&x| orange(x, mid)).collect();
    let (left, right) = (xs[0], *xs.last().unwrap());
    assert!((left - 32.0).abs() < 0.7 && (right - left - 67.0).abs() < 1.0, "the badge spans {left}–{right}");
    assert!(!orange(left + 0.5, top + 0.5), "the badge's corner isn't rounded");
    // Texts by their darkest pixel: the rows above and below the badge's.
    let darkest = |y0: f64, y1: f64| {
        let mut d = (255u8, 255u8, 255u8);
        let mut y = y0;
        while y < y1 {
            for x in (32 * 3)..(150 * 3) {
                let c = pix.pixel(x, (y * 3.0) as u32).unwrap();
                if (c.red() as u32 + c.green() as u32 + c.blue() as u32) < (d.0 as u32 + d.1 as u32 + d.2 as u32) {
                    d = (c.red(), c.green(), c.blue());
                }
            }
            y += 1.0 / 3.0;
        }
        d
    };
    // (Secondary: its top 46.9 pt above the badge's; the plain button 67.2 below.)
    let secondary = darkest(top - 46.9, top - 46.9 + 20.0);
    assert!((110..170).contains(&secondary.0) && secondary.2.abs_diff(secondary.0) < 20, "the secondary text is {secondary:?}");
    let plain = darkest(top + 67.2, top + 67.2 + 20.0);
    assert!(plain.0 < 60 && plain.1 < 60 && plain.2 < 60, "the plain button's label is {plain:?}");
}

/// Typing into the TextField: the field's binding carries it to React, which
/// echoes it.
#[test]
fn typing_reaches_react() {
    let Some(mut emu) = boot() else { return };
    tap(&mut emu, (100.0, 488.0), 3.0);
    emu.input.script.push(pomme_hle::textinput::KeyInput { at_ns: 4_000_000_000, text: "Ada".into() });
    let views = run_for(&mut emu, 6.0);
    let field = views.lines().find(|l| l.contains("PommeTextField")).unwrap();
    assert!(field.ends_with("\"Ada\""), "{field}");
    assert!(views.contains("\"Typed: Ada\""), "{views}");
}

/// Expo UI's Label and Image with SF Symbols (drawn with their stand-ins): a
/// list's labels with their icons in the tint, before their titles; images
/// in the primary color, a sized one larger, and a colored one in its color.
#[test]
fn symbols_draw_as_on_ios() {
    let Some(mut emu) = boot() else { return };
    // (Flung to the end, the list rests against its bottom.)
    touch::swipe(&mut emu.touch.script, 3_000_000_000, (195.0, 700.0), (195.0, 100.0), 300_000_000);
    touch::swipe(&mut emu.touch.script, 4_000_000_000, (195.0, 700.0), (195.0, 100.0), 300_000_000);
    let views = run_for(&mut emu, 7.0);
    // The labels: the icon (large heart, 26.7 × 23.7) centered 12 pt in, the title 40 pt in.
    let lines: Vec<&str> = views.lines().collect();
    let fav = lines.iter().position(|l| l.contains("\"Favorites\"")).expect("no Favorites label");
    assert!(lines[fav - 1].contains("UIImageView (-1.3, -1.7, 26.7×23.7)"), "{}", lines[fav - 1]);
    assert!(lines[fav].contains("UILabel (40.0, 0.0, "), "{}", lines[fav]);
    assert!(lines[fav + 1].contains("UIView (56.0, "), "the separator starts at the title\n{}", lines[fav + 1]);
    // The images: star.fill and checkmark.circle.fill at Expo's default 24 pt, the bell at 28.
    assert!(views.contains("UIImageView (0.0, 1.8, 31.0×28.3)"), "{views}");
    assert!(views.contains("UIImageView (43.0, 0.0, 31.3×32.0)"), "{views}");

    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    // Pixels of each kind in a region (points).
    let count = |x0: f64, y0: f64, x1: f64, y1: f64, kind: &dyn Fn(u8, u8, u8) -> bool| {
        let mut n = 0;
        for y in (y0 * 3.0) as u32..(y1 * 3.0) as u32 {
            for x in (x0 * 3.0) as u32..(x1 * 3.0) as u32 {
                let c = pix.pixel(x, y).unwrap();
                if kind(c.red(), c.green(), c.blue()) {
                    n += 1;
                }
            }
        }
        n
    };
    let blue = |r: u8, g: u8, b: u8| r < 60 && (100..170).contains(&g) && b > 220;
    let black = |r: u8, g: u8, b: u8| r < 60 && g < 60 && b < 60;
    let green = |r: u8, g: u8, b: u8| r < 80 && g > 100 && b < 80;
    // (Where they rest: the heart's row, then the images' row.)
    assert!(count(28.0, 618.0, 60.0, 650.0, &blue) > 200, "the label's heart isn't in the tint");
    assert!(count(30.0, 725.0, 66.0, 760.0, &black) > 300, "the star isn't in the primary color");
    assert!(count(115.0, 725.0, 150.0, 760.0, &green) > 300, "the checkmark isn't green");
}

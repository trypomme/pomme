//! Bar button items in the navigation bar (tests/fixtures/baritems/main.m),
//! held to the iOS 26 Simulator (fixtures/baritems/ios26-<case>.txt; the
//! design before iOS 26 in ios26-compat-<case>.txt; Arabic in
//! ios26-ar-<case>.txt): each item's glass capsule and its area, where its
//! content sits; taps send the item's action. Built with the iOS SDK;
//! skipped without Xcode or the sysroot.

mod common;

use pomme_hle::baritems::BarItem;

const BAR_TOP: f64 = 47.0;

/// The `VIEW class x y w h …` lines of a recording: (class, frame in the bar, rest).
fn views(file: &str) -> Vec<(String, [f64; 4], String)> {
    let text = std::fs::read_to_string(format!("{}/tests/fixtures/baritems/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            (f.len() >= 6 && f[0] == "VIEW").then(|| {
                let r = [f[2], f[3], f[4], f[5]].map(|v| v.parse::<f64>().unwrap());
                (f[1].to_string(), [r[0], r[1] - BAR_TOP, r[2], r[3]], f[6..].join(" "))
            })
        })
        .collect()
}

fn close(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= 0.5)
}

/// pomme's items for a case (on a phone in `language`).
fn items(case: &str, compat: bool, language: &str) -> Option<Vec<BarItem>> {
    let mut emu = common::boot("baritems")?;
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", case);
    emu.system.requires_compatibility = compat;
    emu.locales.settings.languages = vec![language.into()];
    common::views(&mut emu, 1.5);
    let bar = pomme_hle::navigation::bars(&emu)[0];
    Some(pomme_hle::navigation::bar_look(&mut emu, bar).unwrap().unwrap().items)
}

/// Liquid Glass: capsules (glass views with a radius, the back button's
/// aside) and item areas as recorded.
fn check_glass(case: &str, file: &str, language: &str) {
    let Some(ours) = items(case, false, language) else { return };
    let rec = views(file);
    let back = |r: &[f64; 4]| case == "pushed" && (r[2] - 44.0).abs() < 0.1 && (r[1]).abs() < 0.1 && (r[3] - 44.0).abs() < 0.1 && rec.iter().filter(|v| v.0 == "_UIButtonBarButton" && close(v.1, *r)).count() > 0;
    let mut want_caps: Vec<[f64; 4]> = rec.iter().filter(|v| v.0 == "_UINavigationBarPlatterGlassView" && v.2.contains("radius") && !back(&v.1)).map(|v| v.1).collect();
    let mut want_areas: Vec<[f64; 4]> = rec.iter().filter(|v| v.0 == "_UINavigationBarPlatterItemView" && !back(&v.1)).map(|v| v.1).collect();
    let mut got_caps: Vec<[f64; 4]> = ours.iter().filter_map(|i| i.capsule).collect();
    let mut got_areas: Vec<[f64; 4]> = ours.iter().map(|i| i.area).collect();
    for v in [&mut want_caps, &mut want_areas, &mut got_caps, &mut got_areas] {
        v.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
    }
    // (Each held by its outer edge, the one nearer the bar's side; a title's
    // width loosely: Inter isn't SF Pro.)
    let same = |a: &[[f64; 4]], b: &[[f64; 4]]| {
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| {
                let outer = if y[0] + y[2] / 2.0 > 195.0 { (x[0] + x[2] - y[0] - y[2]).abs() } else { (x[0] - y[0]).abs() };
                outer <= 0.5 && (x[1] - y[1]).abs() <= 0.5 && (x[3] - y[3]).abs() <= 0.5 && (x[2] - y[2]).abs() <= 3.0
            })
    };
    assert!(same(&got_caps, &want_caps), "{case}: capsules {got_caps:?}, iOS {want_caps:?}");
    assert!(same(&got_areas, &want_areas), "{case}: areas {got_areas:?}, iOS {want_areas:?}");
    // Content: centred where iOS centres it (titles' text widths aside).
    for i in &ours {
        let c = i.content_frame;
        let mid = c[0] + c[2] / 2.0;
        let inside = rec.iter().filter(|v| (v.0 == "UIImageView" || v.0 == "_UIModernBarButton" || v.0 == "UIView") && v.1[0] >= i.area[0] - 0.5 && v.1[0] + v.1[2] <= i.area[0] + i.area[2] + 0.5);
        assert!(inside.clone().any(|v| (v.1[0] + v.1[2] / 2.0 - mid).abs() <= 2.0), "{case}: content at {c:?}, iOS's {:?}", inside.map(|v| v.1).collect::<Vec<_>>());
    }
}

#[test]
fn one_system_item() {
    check_glass("add", "ios26-add.txt", "en-US");
}

#[test]
fn two_items() {
    check_glass("two", "ios26-two.txt", "en-US");
}

#[test]
fn leading_and_trailing() {
    check_glass("both", "ios26-both.txt", "en-US");
}

#[test]
fn title_item() {
    check_glass("title", "ios26-title.txt", "en-US");
}

#[test]
fn image_item() {
    check_glass("image", "ios26-image.txt", "en-US");
}

#[test]
fn custom_view_item() {
    check_glass("custom", "ios26-custom.txt", "en-US");
}

#[test]
fn prominent_item() {
    check_glass("prominent", "ios26-prominent.txt", "en-US");
    let Some(ours) = items("prominent", false, "en-US") else { return };
    assert!(ours[0].prominent);
    // The system Done item is prominent of itself (ios26-both.png: blue).
    let Some(both) = items("both", false, "en-US") else { return };
    assert!(both.iter().any(|i| i.prominent) && both.iter().any(|i| !i.prominent));
}

#[test]
fn without_a_shared_background() {
    check_glass("separate", "ios26-separate.txt", "en-US");
}

#[test]
fn beside_the_back_button() {
    check_glass("pushed", "ios26-pushed.txt", "en-US");
}

#[test]
fn right_to_left() {
    check_glass("both", "ios26-ar-both.txt", "ar");
    check_glass("pushed", "ios26-ar-pushed.txt", "ar");
}

/// Before iOS 26: tinted content, its tap area as the bar button's.
#[test]
fn before_ios_26() {
    for case in ["add", "both", "title", "pushed"] {
        let Some(ours) = items(case, true, "en-US") else { return };
        let rec = views(&format!("ios26-compat-{case}.txt"));
        // (The back button's own button aside.)
        let mut want: Vec<[f64; 4]> = rec.iter().filter(|v| v.0 == "_UIButtonBarButton" && v.1[0] > 1.0).map(|v| v.1).collect();
        let mut got: Vec<[f64; 4]> = ours.iter().map(|i| i.area).collect();
        want.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        got.sort_by(|a, b| a[0].partial_cmp(&b[0]).unwrap());
        assert!(got.len() == want.len() && got.iter().zip(&want).all(|(a, b)| (a[0] - b[0]).abs() <= 3.0 && (a[2] - b[2]).abs() <= 3.0), "{case}: areas {got:?}, iOS {want:?}");
    }
}

/// A tap on the add item sends its action.
#[test]
fn tapping_an_item_sends_its_action() {
    let Some(mut emu) = common::boot("baritems") else { return };
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", "add");
    pomme_hle::touch::swipe(&mut emu.touch.script, 1_500_000_000, (352.0, 69.0), (352.0, 69.0), 50_000_000);
    common::views(&mut emu, 2.0);
    assert!(emu.console.contains("VALUE tapped (no title)"), "{}", emu.console);
}

/// pomme's toolbar items for the `toolbar` case, in window coordinates, and
/// what the probe printed.
fn toolbar(compat: bool) -> Option<(Vec<BarItem>, String)> {
    let mut emu = common::boot("baritems")?;
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", "toolbar");
    emu.system.requires_compatibility = compat;
    common::views(&mut emu, 1.5);
    let t = pomme_hle::baritems::toolbars(&emu)[0];
    let o = pomme_hle::baritems::toolbar_origin(&mut emu, t);
    let items = pomme_hle::baritems::toolbar_look(&mut emu, t).unwrap().unwrap();
    let shift = |r: [f64; 4]| [r[0] + o[0], r[1] + o[1], r[2], r[3]];
    let items = items.into_iter().map(|i| BarItem { capsule: i.capsule.map(shift), area: shift(i.area), content_frame: shift(i.content_frame), ..i }).collect();
    Some((items, emu.console.clone()))
}

/// The navigation controller's toolbar under Liquid Glass (measured in
/// ios26-toolbar.png: the capsules at the sides and centred between the
/// flexible spaces, 48 pt tall, 28 pt above the screen's bottom); the
/// screen's safe area ends above them.
#[test]
fn toolbar_items() {
    let Some((ours, console)) = toolbar(false) else { return };
    // (add 28–76, Edit 167.33–223.33, Share 290.67–362, all 768–816; titles' widths loosely.)
    let want = [[28.0, 768.0, 48.0, 48.0], [167.33, 768.0, 56.0, 48.0], [290.67, 768.0, 71.33, 48.0]];
    let got: Vec<[f64; 4]> = ours.iter().filter_map(|i| i.capsule).collect();
    assert_eq!(got.len(), 3, "{got:?}");
    assert!((got[0][0] - 28.0).abs() <= 0.5 && (got[0][2] - 48.0).abs() <= 0.5, "add {:?}", got[0]);
    assert!(((got[1][0] + got[1][2] / 2.0) - 195.33).abs() <= 0.5 && (got[1][2] - 56.0).abs() <= 3.0, "Edit {:?}", got[1]);
    assert!(((got[2][0] + got[2][2]) - 362.0).abs() <= 0.5 && (got[2][2] - 71.33).abs() <= 3.0, "Share {:?}", got[2]);
    assert!(got.iter().zip(want).all(|(g, w)| (g[1] - w[1]).abs() <= 0.5 && (g[3] - w[3]).abs() <= 0.5), "{got:?}");
    assert!(console.contains("VALUE safeArea 101.00 0.00 76.00 0.00"), "{console}");
}

/// Before iOS 26: a 49-pt bar above the home indicator, the items' content
/// as recorded (ios26-compat-toolbar.txt), the safe area ending at the bar.
#[test]
fn toolbar_items_before_ios_26() {
    let Some((ours, console)) = toolbar(true) else { return };
    let rec = views("ios26-compat-toolbar.txt");
    let want: Vec<[f64; 4]> = rec.iter().filter(|v| v.0 == "_UIButtonBarButton").map(|v| [v.1[0], v.1[1] + BAR_TOP, v.1[2], v.1[3]]).collect();
    assert_eq!(ours.len(), want.len(), "{ours:?}");
    for (i, w) in ours.iter().zip(&want) {
        let a = i.area;
        // (Held by the outer edge; titles loosely.)
        let edge = if w[0] > 195.0 { (a[0] + a[2] - w[0] - w[2]).abs() } else { (a[0] - w[0]).abs() };
        assert!(edge <= 3.0 && (a[2] - w[2]).abs() <= 3.0, "area {a:?}, iOS {w:?}");
        let mid = i.content_frame[1] + i.content_frame[3] / 2.0;
        assert!((mid - 782.5).abs() <= 0.5, "content {:?}", i.content_frame);
    }
    assert!(console.contains("VALUE safeArea 91.00 0.00 83.00 0.00"), "{console}");
}

/// A tap on the toolbar's add item sends its action.
#[test]
fn tapping_a_toolbar_item() {
    let Some(mut emu) = common::boot("baritems") else { return };
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", "toolbar");
    pomme_hle::touch::swipe(&mut emu.touch.script, 1_500_000_000, (52.0, 792.0), (52.0, 792.0), 50_000_000);
    common::views(&mut emu, 2.0);
    assert!(emu.console.contains("VALUE tapped"), "{}", emu.console);
}

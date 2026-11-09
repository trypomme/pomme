//! End to end: a native UIKit app (tests/fixtures/autolayout/main.m) laid out
//! with Auto Layout against the safe area, the layout margins, a custom guide
//! and the keyboard. Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use common::views;
use pomme_hle::Emu;

fn boot(device: &str) -> Option<Emu> {
    let mut emu = common::boot("autolayout")?;
    emu.device = pomme_hle::device::find(device).unwrap();
    Some(emu)
}

/// The frame of the view of this class, from the dump.
fn frame(views: &str, class: &str) -> String {
    let line = views.lines().find(|l| l.trim_start().starts_with(&format!("{class} ("))).unwrap_or_else(|| panic!("no {class} in\n{views}"));
    line.trim_start()[class.len() + 1..].split(')').next().unwrap().to_string() + ")"
}

fn tag(views: &str, class: &str) -> u64 {
    let line = views.lines().find(|l| l.trim_start().starts_with(&format!("{class} ("))).unwrap();
    line.split(" tag ").nth(1).map(|t| t.split_whitespace().next().unwrap().parse().unwrap()).unwrap_or(0)
}

/// On the iPhone 16e (390×844, safe area 47 pt top, 34 pt bottom; root view
/// margins 16 pt at the sides).
#[test]
fn constraints_against_the_safe_area_and_margins() {
    let Some(mut emu) = boot("iphone-16e") else { return };
    let v = views(&mut emu, 1.0);
    assert_eq!(frame(&v, "Header"), "(0.0, 47.0, 390.0×50.0)", "{v}");
    assert_eq!(frame(&v, "Footer"), "(16.0, 766.0, 358.0×44.0)", "{v}");
    assert_eq!(frame(&v, "Box"), "(145.0, 397.0, 100.0×50.0)", "{v}");
    // Its leading margin (8 pt in) on the root's (16 pt in); its trailing edge on the root's margin.
    assert_eq!(frame(&v, "Margined"), "(8.0, 105.0, 366.0×30.0)", "{v}");
    // The guide spans 97–766: centered at 431.5, 20 tall; to its right.
    assert_eq!(frame(&v, "Spaced"), "(10.0, 421.5, 20.0×20.0)", "{v}");
    // Intrinsic size 60×24, 8 pt below the box.
    assert_eq!(frame(&v, "Badge"), "(165.0, 455.0, 60.0×24.0)", "{v}");
    // Keyboard down: the keyboard guide starts at the safe area's bottom.
    assert_eq!(frame(&v, "Above"), "(350.0, 800.0, 40.0×10.0)", "{v}");
    // Each callback once: the insets changed from none to the device's when the view reached the window.
    assert_eq!(tag(&v, "Listener"), 1, "safeAreaInsetsDidChange\n{v}");
    assert_eq!(v.matches(" tag 1").count(), 2, "viewSafeAreaInsetsDidChange\n{v}");
}

/// The same app on two other phones: everything follows the device.
#[test]
fn constraints_follow_the_device() {
    let Some(mut emu) = boot("iphone-17-pro-max") else { return };
    // 440×956, insets 62 / 34, margins 20.
    let v = views(&mut emu, 1.0);
    assert_eq!(frame(&v, "Header"), "(0.0, 62.0, 440.0×50.0)", "{v}");
    assert_eq!(frame(&v, "Footer"), "(16.0, 878.0, 408.0×44.0)", "{v}");
    assert_eq!(frame(&v, "Box"), "(170.0, 453.0, 100.0×50.0)", "{v}");
    assert_eq!(frame(&v, "Margined"), "(12.0, 120.0, 408.0×30.0)", "{v}");

    let mut emu = boot("iphone-se").unwrap();
    // 375×667, 20 pt status bar, no home indicator.
    let v = views(&mut emu, 1.0);
    assert_eq!(frame(&v, "Header"), "(0.0, 20.0, 375.0×50.0)", "{v}");
    assert_eq!(frame(&v, "Footer"), "(16.0, 623.0, 343.0×44.0)", "{v}");
    assert_eq!(frame(&v, "Above"), "(335.0, 657.0, 40.0×10.0)", "{v}");
}

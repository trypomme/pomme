//! End to end: a native UIKit app (tests/fixtures/autolayout/main.m) laid out
//! with Auto Layout against the safe area, the layout margins, a custom guide
//! and the keyboard. Built with the iOS SDK; skipped without Xcode or the sysroot.

use pomme_hle::Emu;
use std::path::{Path, PathBuf};
use std::process::Command;

const SYSROOT: &str = "../../vendor/sysroot";

fn build() -> Option<PathBuf> {
    let sdk = Command::new("xcrun").args(["--sdk", "iphoneos", "--show-sdk-path"]).output().ok().filter(|o| o.status.success())?;
    let _ = sdk;
    let app = Path::new(env!("CARGO_TARGET_TMPDIR")).join("Autolayout.app");
    std::fs::create_dir_all(&app).unwrap();
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/autolayout/main.m");
    let status = Command::new("xcrun")
        .args(["--sdk", "iphoneos", "clang", "-target", "arm64-apple-ios17.0", "-fobjc-arc", "-framework", "UIKit", "-framework", "Foundation"])
        .arg(&src)
        .arg("-o")
        .arg(app.join("Autolayout"))
        .status()
        .expect("run clang");
    assert!(status.success(), "building the fixture app failed");
    std::fs::write(
        app.join("Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>Autolayout</string>
<key>CFBundleIdentifier</key><string>dev.pomme.autolayout</string>
<key>CFBundleName</key><string>Autolayout</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>MinimumOSVersion</key><string>17.0</string>
</dict></plist>"#,
    )
    .unwrap();
    Some(app)
}

fn boot(device: &str) -> Option<Emu> {
    let sysroot = Path::new(SYSROOT);
    if !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() {
        eprintln!("skipping: build the sysroot first");
        return None;
    }
    let app = build()?;
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    let mut emu = Emu::new(pomme_loader::load_with(&app, &opts).unwrap()).unwrap();
    emu.device = pomme_hle::device::find(device).unwrap();
    Some(emu)
}

/// The views after running for `secs`, as `pomme run --views` prints them.
fn views(emu: &mut Emu, secs: f64) -> String {
    emu.time_limit_ns = Some((secs * 1e9) as u64);
    match emu.run_main() {
        // A static app has nothing scheduled once it's laid out.
        Err(pomme_hle::Stop::TimeLimit(_) | pomme_hle::Stop::Idle(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}"),
    }
    emu.describe_views()
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

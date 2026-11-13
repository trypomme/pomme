//! Native fixture apps: one Objective-C file each (tests/fixtures/NAME/main.m),
//! built with the iOS SDK into a minimal .app.

#![allow(dead_code)]

use pomme_hle::Emu;
use std::path::{Path, PathBuf};
use std::process::Command;

const SYSROOT: &str = "../../vendor/sysroot";

/// Build `tests/fixtures/{name}/main.m` into `{Name}.app` (once per test
/// process: tests run in parallel and would otherwise race on the binary);
/// None without Xcode.
pub fn build(name: &str) -> Option<PathBuf> {
    static BUILT: std::sync::Mutex<Vec<(String, Option<PathBuf>)>> = std::sync::Mutex::new(Vec::new());
    let mut built = BUILT.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, app)) = built.iter().find(|(n, _)| n == name) {
        return app.clone();
    }
    let app = build_now(name);
    built.push((name.to_string(), app.clone()));
    app
}

fn build_now(name: &str) -> Option<PathBuf> {
    Command::new("xcrun").args(["--sdk", "iphoneos", "--show-sdk-path"]).output().ok().filter(|o| o.status.success())?;
    let exe = name[..1].to_uppercase() + &name[1..];
    let app = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{exe}.app"));
    std::fs::create_dir_all(&app).unwrap();
    // main.m, or main.mm for Objective-C++ (linked with libc++).
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}"));
    let cxx = dir.join("main.mm").exists();
    let src = dir.join(if cxx { "main.mm" } else { "main.m" });
    let status = Command::new("xcrun")
        .args(["--sdk", "iphoneos", "clang", "-target", "arm64-apple-ios17.0", "-fobjc-arc", "-framework", "UIKit", "-framework", "Foundation", "-framework", "QuartzCore", "-framework", "CoreGraphics", "-framework", "SystemConfiguration", "-lsqlite3"])
        .args(if cxx { &["-lc++", "-std=c++17"][..] } else { &[][..] })
        .arg(&src)
        .arg("-o")
        .arg(app.join(&exe))
        .status()
        .expect("run clang");
    assert!(status.success(), "building the {name} fixture app failed");
    // More Info.plist keys (scenes, orientations, …), if the fixture has them.
    let extra = std::fs::read_to_string(dir.join("Info.extra.plist")).unwrap_or_default();
    std::fs::write(
        app.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>{exe}</string>
<key>CFBundleIdentifier</key><string>dev.pomme.{name}</string>
<key>CFBundleName</key><string>{exe}</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>MinimumOSVersion</key><string>17.0</string>
{extra}
</dict></plist>"#
        ),
    )
    .unwrap();
    Some(app)
}

/// The fixture app, loaded and ready to run; None without Xcode or the sysroot.
pub fn boot(name: &str) -> Option<Emu> {
    let sysroot = Path::new(SYSROOT);
    if !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() {
        eprintln!("skipping: build the sysroot first");
        return None;
    }
    let app = build(name)?;
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    Some(Emu::new(pomme_loader::load_with(&app, &opts).unwrap()).unwrap())
}

/// The views after running for `secs`, as `pomme run --views` prints them.
pub fn views(emu: &mut Emu, secs: f64) -> String {
    emu.time_limit_ns = Some((secs * 1e9) as u64);
    match emu.run_main() {
        // A static app has nothing scheduled once it's laid out.
        Err(pomme_hle::Stop::TimeLimit(_) | pomme_hle::Stop::Idle(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}"),
    }
    emu.describe_views()
}

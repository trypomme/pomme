//! Native fixture apps: one Objective-C file each (tests/fixtures/NAME/main.m),
//! or Swift (main.swift), built with the iOS SDK into a minimal .app.

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
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}"));
    if dir.join("main.swift").exists() {
        // A SwiftUI (or other Swift) app: `@main`, so parsed as a library.
        let status = Command::new("xcrun")
            .args(["--sdk", "iphoneos", "swiftc", "-target", "arm64-apple-ios17.0", "-parse-as-library", "-O"])
            .arg(dir.join("main.swift"))
            .arg("-o")
            .arg(app.join(&exe))
            .status()
            .expect("run swiftc");
        assert!(status.success(), "building the {name} fixture app failed");
        write_info_plist(&dir, &app, &exe, name);
        return Some(app);
    }
    // main.m, or main.mm for Objective-C++ (linked with libc++).
    let cxx = dir.join("main.mm").exists();
    let src = dir.join(if cxx { "main.mm" } else { "main.m" });
    // Other Objective-C files beside main (a library the fixture uses), built in.
    let mut others: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "m") && *p != src).collect();
    others.sort();
    // WebKit only for fixtures whose sources (headers too) import it.
    let webkit = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "h" || x == "m" || x == "mm")).any(|p| std::fs::read_to_string(p).unwrap_or_default().contains("<WebKit/"));
    let status = Command::new("xcrun")
        .args(["--sdk", "iphoneos", "clang", "-target", "arm64-apple-ios17.0", "-fobjc-arc", "-framework", "UIKit", "-framework", "Foundation", "-framework", "QuartzCore", "-framework", "CoreGraphics", "-framework", "SystemConfiguration", "-lsqlite3"])
        .args(if cxx { &["-lc++", "-std=c++17"][..] } else { &[][..] })
        .args(if webkit { &["-framework", "WebKit"][..] } else { &[][..] })
        .arg(&src)
        .args(&others)
        .arg("-o")
        .arg(app.join(&exe))
        .status()
        .expect("run clang");
    assert!(status.success(), "building the {name} fixture app failed");
    // Resources the fixture ships with go in the bundle's top level.
    if let Ok(entries) = std::fs::read_dir(dir.join("resources")) {
        for e in entries {
            let p = e.unwrap().path();
            std::fs::copy(&p, app.join(p.file_name().unwrap())).unwrap();
        }
    }
    write_info_plist(&dir, &app, &exe, name);
    Some(app)
}

/// The app's Info.plist, with the fixture's extra keys (scenes, orientations, …).
fn write_info_plist(dir: &Path, app: &Path, exe: &str, name: &str) {
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

/// A probe's reported frames (`METRIC name x y width height`), each view's last.
pub type Frames = std::collections::BTreeMap<String, [f64; 4]>;

pub fn parse_metrics(text: &str) -> Frames {
    let mut frames = Frames::new();
    for line in text.lines().filter(|l| l.starts_with("METRIC ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let v: Vec<f64> = f[2..6].iter().map(|x| x.parse().unwrap()).collect();
        frames.insert(f[1].to_string(), [v[0], v[1], v[2], v[3]]);
    }
    frames
}

/// What `probe` reported in pomme against what it reported on the iOS 26
/// simulator (fixtures/<probe>/ios26.txt): each value more than half a point
/// off, except those `skip(name, index)` passes over.
pub fn metric_differences(probe: &str, console: &str, skip: impl Fn(&str, usize) -> bool) -> Vec<String> {
    let ours = parse_metrics(console);
    let ios = parse_metrics(&std::fs::read_to_string(format!("{}/tests/fixtures/{probe}/ios26.txt", env!("CARGO_MANIFEST_DIR"))).unwrap());
    let mut wrong = Vec::new();
    for (name, want) in &ios {
        let Some(got) = ours.get(name) else {
            wrong.push(format!("{name}: not reported"));
            continue;
        };
        for (i, what) in ["x", "y", "width", "height"].iter().enumerate() {
            if !skip(name, i) && (got[i] - want[i]).abs() > 0.5 {
                wrong.push(format!("{name}: {what} {:.2}, iOS {:.2}", got[i], want[i]));
            }
        }
    }
    wrong
}

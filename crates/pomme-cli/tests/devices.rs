//! End to end: the devices probe (tests/fixtures/devices/main.m) in pomme
//! reports what it reported on the iOS 26.5 Simulator
//! (tests/fixtures/devices/ios26.txt, scripts/measure/devices/run.sh): the
//! screen, safe area, status bar, size classes, idiom, `UIDevice` and the
//! keyboards, portrait then landscape. On the iPhone 16e and the iPads, as an
//! iPad app (windowed and full screen) and as an iPhone-only app (iPadOS's
//! compatibility mode). Built with the iOS SDK; skipped without Xcode or the
//! sysroot.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The recording's lines for `id` in `mode`: "<phase>.<name>" → value.
fn recorded(id: &str, mode: &str) -> BTreeMap<String, String> {
    let rec = std::fs::read_to_string(format!("{}/tests/fixtures/devices/ios26.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let prefix = format!("{id}.{mode}.");
    rec.lines()
        .filter_map(|l| l.strip_prefix("METRIC "))
        .filter_map(|l| l.strip_prefix(&prefix))
        .map(|l| {
            let (k, v) = l.split_once(' ').unwrap();
            (k.to_string(), v.to_string())
        })
        .collect()
}

/// The probe built as the mode's app: "native" lists the iPad, "fullscreen"
/// also requires the full screen, "compat" is iPhone-only.
fn app(id: &str, mode: &str) -> Option<PathBuf> {
    let built = common::build("devices")?;
    if mode == "native" {
        return Some(built);
    }
    // One copy per test: the tests run at the same time.
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("devices-{id}-{mode}/Devices.app"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(built.join("Devices"), dir.join("Devices")).unwrap();
    let plist = std::fs::read_to_string(built.join("Info.plist")).unwrap();
    let plist = match mode {
        "compat" => plist.replace("<integer>1</integer><integer>2</integer>", "<integer>1</integer>"),
        "fullscreen" => plist.replace("<key>UILaunchScreen</key>", "<key>UIRequiresFullScreen</key><true/>\n<key>UILaunchScreen</key>"),
        _ => unreachable!(),
    };
    std::fs::write(dir.join("Info.plist"), plist).unwrap();
    Some(dir)
}

/// Run the probe on `id` as `mode`'s app and compare every line it prints
/// with the recording; the differences.
fn differences(id: &str, mode: &str) -> Option<Vec<String>> {
    let sysroot = Path::new("../../vendor/sysroot");
    if !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() {
        eprintln!("skipping: build the sysroot first");
        return None;
    }
    let app = app(id, mode)?;
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    let mut emu = pomme_hle::Emu::new(pomme_loader::load_with(&app, &opts).unwrap()).unwrap();
    emu.present(pomme_hle::device::find(id).unwrap());
    emu.time_limit_ns = Some(20_000_000_000);
    let stop = emu.run_main();
    eprintln!("{id} {mode}: {stop:?}");
    let ours: BTreeMap<String, String> = emu.console.lines().filter_map(|l| l.strip_prefix("METRIC ")).filter_map(|l| l.split_once(' ')).map(|(k, v)| (k.to_string(), v.to_string())).collect();
    assert!(emu.console.contains("DONE"), "the probe didn't finish on {id} {mode}:\n{}", emu.console);
    let mut wrong = Vec::new();
    for (k, want) in recorded(id, mode) {
        // A windowed iPad app can't turn itself on iPadOS (the recording's
        // "landscape" is portrait, with an error); pomme turns it.
        if mode == "native" && id.starts_with("ipad") && k.starts_with("landscape.") {
            continue;
        }
        let want = match k.rsplit('.').next() {
            // The Simulator's model variable is "?" in pomme; the rest must match.
            Some("device") => want.rsplit_once('|').unwrap().0.to_string(),
            _ => want,
        };
        let got = ours.get(&k).map(|g| if k.ends_with(".device") { g.rsplit_once('|').unwrap().0.to_string() } else { g.clone() });
        if got.as_deref() != Some(want.as_str()) {
            wrong.push(format!("{id} {mode} {k}: pomme {got:?}, iOS {want:?}"));
        }
    }
    Some(wrong)
}

/// Known differences: the iPhone 16e's keyboards (pomme: 304 pt for a plain
/// field, as Flutter's measured; 209 in landscape, measured on 26.2).
fn known(line: &str) -> bool {
    line.starts_with("iphone-16e") && line.contains(".kb.")
}

fn check(id: &str, modes: &[&str]) {
    let mut wrong = Vec::new();
    for mode in modes {
        let Some(w) = differences(id, mode) else { return };
        wrong.extend(w.into_iter().filter(|l| !known(l)));
    }
    assert!(wrong.is_empty(), "{} differences:\n{}", wrong.len(), wrong.join("\n"));
}

#[test]
fn iphone_16e() {
    check("iphone-16e", &["native"]);
}

#[test]
fn ipad_pro_11() {
    check("ipad-pro-11-m5", &["native", "fullscreen", "compat"]);
}

#[test]
fn ipad_pro_13() {
    check("ipad-pro-13-m5", &["native", "fullscreen", "compat"]);
}

#[test]
fn ipad_air_11() {
    check("ipad-air-11-m4", &["native", "fullscreen", "compat"]);
}

#[test]
fn ipad_air_13() {
    check("ipad-air-13-m4", &["native", "fullscreen", "compat"]);
}

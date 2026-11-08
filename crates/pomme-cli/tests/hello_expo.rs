//! End to end: boot hello-expo, tap its button twice, check what's on screen.
//! Skipped unless the app (scripts/build-hello-expo.sh) and the sysroot
//! (scripts/build-sysroot.sh) have been built.

use pomme_hle::touch::{BEGAN, ENDED, Input};
use pomme_hle::{Emu, Stop};
use std::path::Path;

const APP: &str = "../../apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app";
const SYSROOT: &str = "../../vendor/sysroot";

fn tap(emu: &mut Emu, x: f64, y: f64, secs: f64) {
    let at = (secs * 1e9) as u64;
    emu.touch.script.push(Input { at_ns: at, phase: BEGAN, x, y });
    emu.touch.script.push(Input { at_ns: at + 80_000_000, phase: ENDED, x, y });
}

#[test]
fn renders_and_counts_taps() {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() {
        eprintln!("skipping: build hello-expo and the sysroot first");
        return;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    let p = pomme_loader::load_with(app, &opts).unwrap();
    let mut emu = Emu::new(p).unwrap();
    emu.time_limit_ns = Some(6_000_000_000);
    tap(&mut emu, 195.0, 438.0, 3.0);
    tap(&mut emu, 195.0, 438.0, 4.0);
    match emu.run_main() {
        Err(Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}"),
    }
    emu.time_limit_ns = None;
    let views = emu.describe_views();
    assert!(views.contains("\"Hello from pomme 🍎\""), "{views}");
    assert!(views.contains("\"Tapped 2 times\""), "{views}");
    let png = emu.screenshot_png(1.0).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
}

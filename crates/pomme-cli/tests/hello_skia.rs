//! End to end: React Native Skia (Ganesh on Metal) in hello-skia.
//! Skipped unless the app (scripts/build-app.sh hello-skia) and the sysroot
//! have been built.

use pomme_hle::{Emu, Stop};
use std::path::Path;

const APP: &str = "../../apps/hello-skia/ios/build/Build/Products/Release-iphoneos/helloskia.app";
const SYSROOT: &str = "../../vendor/sysroot";

fn boot() -> Option<Emu> {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() {
        eprintln!("skipping: build hello-skia and the sysroot first");
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    Some(Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap())
}

/// Skia starts on Metal, compiles its shaders and presents a frame of the
/// canvas (300 pt at 3×) without stopping.
#[test]
fn skia_draws_through_metal() {
    let Some(mut emu) = boot() else { return };
    emu.time_limit_ns = Some(6_000_000_000);
    match emu.run_main() {
        Err(Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}\n{}", pomme_hle::metal::report(&emu)),
    }
    let views = emu.describe_views();
    assert!(views.contains("SkiaPictureView (45.0, 290.3, 300.0×300.0)"), "{views}");
    let m = &emu.metal;
    assert!(m.sources.len() >= 10, "{}", pomme_hle::metal::report(&emu));
    assert!(m.sources.iter().any(|s| s.contains("fragment Outputs fragmentMain")));
    assert!(m.presented >= 1, "no frame presented\n{}", pomme_hle::metal::report(&emu));
}

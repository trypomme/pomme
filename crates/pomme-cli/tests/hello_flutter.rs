//! End to end: a Flutter app (Impeller on Metal). Skipped unless the app
//! (scripts/build-flutter.sh hello-flutter), the sysroot and the Impeller MSL
//! (scripts/flutter/impeller-msl.sh) have been built.

use pomme_hle::Emu;
use std::path::Path;

const APP: &str = "../../apps/hello-flutter/build/ios/iphoneos/Runner.app";
const SYSROOT: &str = "../../vendor/sysroot";

fn boot() -> Option<Emu> {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() || !sysroot.join("pomme/impeller").exists() {
        eprintln!("skipping: build hello-flutter, the sysroot and the Impeller MSL first");
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    Some(Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap())
}

/// The engine starts: App.framework (the Dart AOT snapshot, dlopened by the
/// engine) is loaded, the Dart VM boots, and Impeller builds its pipelines from
/// its precompiled shaders, each substituted with MSL and specialized.
#[test]
fn engine_boots_and_builds_impeller_pipelines() {
    let Some(mut emu) = boot() else { return };
    assert!(emu.p.images.iter().any(|i| i.name == "App"), "App.framework isn't loaded");
    emu.time_limit_ns = Some(8_000_000_000);
    let stop = emu.run_main();
    let m = &emu.metal;
    // Impeller's entity shaders: one MSL source per function it asked for.
    assert!(m.sources.len() >= 40, "only {} shader functions made ({stop:?})\n{}", m.sources.len(), pomme_hle::metal::report(&emu));
    assert!(m.sources.iter().any(|s| s.contains("imp_solid_fill_vertex_main")));
}

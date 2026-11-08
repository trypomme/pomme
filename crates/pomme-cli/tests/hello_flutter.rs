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

/// The first frame reaches the screen: the root view controller appears (so
/// the framework enables frames), the display link drives a frame, Impeller
/// renders it on the host GPU into FlutterMetalLayer's IOSurface, and the
/// layer shows it: the white page and the slate rounded square (300 pt,
/// centered at 3×).
#[test]
fn first_frame_renders() {
    let Some(mut emu) = boot() else { return };
    emu.time_limit_ns = Some(3_000_000_000);
    match emu.run_main() {
        Err(pomme_hle::Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}\n{}", pomme_hle::metal::report(&emu)),
    }
    if let Some(e) = &emu.metal.gpu_error {
        eprintln!("skipping: no GPU here ({e})");
        return;
    }
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let at = |x: f64, y: f64| {
        let c = pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap();
        (c.red(), c.green(), c.blue())
    };
    let near = |(r, g, b): (u8, u8, u8), (er, eg, eb): (u8, u8, u8)| r.abs_diff(er) < 12 && g.abs_diff(eg) < 12 && b.abs_diff(eb) < 12;
    // The square spans (45, 272)–(345, 572) pt.
    assert!(near(at(60.0, 300.0), (0x1e, 0x29, 0x3b)), "square: {:?}", at(60.0, 300.0));
    assert!(near(at(330.0, 550.0), (0x1e, 0x29, 0x3b)), "square: {:?}", at(330.0, 550.0));
    // Outside it: the page; and just outside its rounded corner.
    assert!(near(at(195.0, 100.0), (255, 255, 255)), "page: {:?}", at(195.0, 100.0));
    assert!(near(at(47.0, 274.0), (255, 255, 255)), "corner: {:?}", at(47.0, 274.0));
}

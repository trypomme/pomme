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

/// Skia's frame runs on the host GPU (Metal → wgpu, shaders MSL → WGSL) and
/// shows in the canvas: the dark rounded rectangle, the circle's gradient
/// from rose (top left) to amber (bottom right), and the stroked chevron.
#[test]
fn skia_frame_renders_on_the_gpu() {
    let Some(mut emu) = boot() else { return };
    emu.time_limit_ns = Some(6_000_000_000);
    let _ = emu.run_main();
    if let Some(e) = &emu.metal.gpu_error {
        eprintln!("skipping: no GPU here ({e})");
        return;
    }
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    // The canvas is at (45, 290.3) pt; read pixels at 3×.
    let at = |x: f64, y: f64| {
        let c = pix.pixel(((45.0 + x) * 3.0) as u32, ((290.3 + y) * 3.0) as u32).unwrap();
        (c.red(), c.green(), c.blue())
    };
    let near = |(r, g, b): (u8, u8, u8), (er, eg, eb): (u8, u8, u8)| r.abs_diff(er) < 12 && g.abs_diff(eg) < 12 && b.abs_diff(eb) < 12;
    // The rounded rect (#1e293b), inside its corner radius.
    assert!(near(at(40.0, 150.0), (0x1e, 0x29, 0x3b)), "rect: {:?}", at(40.0, 150.0));
    // Outside the rect's rounded corner: the white page.
    assert!(near(at(21.0, 21.0), (255, 255, 255)), "corner: {:?}", at(21.0, 21.0));
    // The circle's gradient: rose near the start, amber near the end.
    let (start, end) = (at(110.0, 85.0), at(190.0, 165.0));
    assert!(start.0 > 220 && start.1 < 110 && start.2 > 60, "gradient start: {start:?}");
    assert!(end.0 > 220 && end.1 > 120 && end.2 < 60, "gradient end: {end:?}");
    // The chevron (M 90 245 L 150 205 L 210 245, 8 pt stroke), drawn through
    // Skia's MSAA atlas with stencil-then-cover: white on the stroke…
    assert!(near(at(120.0, 225.0), (255, 255, 255)), "stroke: {:?}", at(120.0, 225.0));
    assert!(near(at(180.0, 225.0), (255, 255, 255)), "stroke: {:?}", at(180.0, 225.0));
    // …and only there: inside its bounds but off the stroke is still the rect.
    assert!(near(at(150.0, 240.0), (0x1e, 0x29, 0x3b)), "under the chevron: {:?}", at(150.0, 240.0));
}

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
/// layer shows it: the white page, the slate rounded square (300 pt, centered
/// at 3×), the gradient circle, the stroked chevron and the text. Pixels
/// checked here match the iOS Simulator's.
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
    // The circle's gradient (stops from a storage buffer): rose near its
    // start (top left), amber near its end (bottom right).
    let (start, end) = (at(150.0, 377.0), at(240.0, 467.0));
    assert!(start.0 > 220 && start.1 < 110 && start.2 > 60, "gradient start: {start:?}");
    assert!(end.0 > 220 && end.1 > 120 && end.2 < 60, "gradient end: {end:?}");
    // The chevron (an 8 pt round-capped stroke, (135, 547) → (195, 507) →
    // (255, 547) pt, which Impeller tessellates into triangles): white on the
    // stroke, and only there: under its apex is still the square.
    assert!(near(at(165.0, 527.0), (255, 255, 255)), "stroke: {:?}", at(165.0, 527.0));
    assert!(near(at(225.0, 527.0), (255, 255, 255)), "stroke: {:?}", at(225.0, 527.0));
    assert!(near(at(195.0, 528.0), (0x1e, 0x29, 0x3b)), "under the chevron: {:?}", at(195.0, 528.0));
    // The round caps reach past the ends: half the stroke width.
    assert!(near(at(132.5, 547.0), (255, 255, 255)), "cap: {:?}", at(132.5, 547.0));
    assert!(near(at(129.0, 547.0), (0x1e, 0x29, 0x3b)), "past the cap: {:?}", at(129.0, 547.0));
    // The text above and below the square (CoreText glyphs in Impeller's atlas):
    // dark ink in each line's band, none in the empty band between them.
    let ink = |y0: f64, y1: f64| {
        let mut n = 0;
        for y in (y0 * 3.0) as u32..(y1 * 3.0) as u32 {
            for x in (100 * 3)..(290 * 3) {
                if pix.pixel(x, y).unwrap().red() < 100 {
                    n += 1;
                }
            }
        }
        n
    };
    assert!(ink(232.0, 256.0) > 500, "\"Hello from Flutter\": {} dark pixels", ink(232.0, 256.0));
    assert!(ink(588.0, 612.0) > 500, "\"Tapped 0 times\": {} dark pixels", ink(588.0, 612.0));
    assert_eq!(ink(150.0, 200.0), 0);
}

fn tap(emu: &mut Emu, (x, y): (f64, f64), secs: f64) {
    use pomme_hle::touch::{BEGAN, ENDED, Input};
    let at = (secs * 1e9) as u64;
    emu.touch.script.push(Input { at_ns: at, phase: BEGAN, x, y });
    emu.touch.script.push(Input { at_ns: at + 80_000_000, phase: ENDED, x, y });
}

/// The "Tapped N times" line after running with these taps, as ink/no-ink
/// pixels, or None without a GPU.
fn counter_line(taps: &[(f64, f64)]) -> Option<Vec<bool>> {
    let mut emu = boot()?;
    for (i, &p) in taps.iter().enumerate() {
        tap(&mut emu, p, 2.0 + 0.5 * i as f64);
    }
    emu.time_limit_ns = Some(2_000_000_000 + 500_000_000 * taps.len() as u64 + 500_000_000);
    match emu.run_main() {
        Err(pomme_hle::Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}"),
    }
    if emu.metal.gpu_error.is_some() {
        return None;
    }
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let mut line = Vec::new();
    for y in (588 * 3)..(612 * 3) {
        for x in (100 * 3)..(290 * 3) {
            line.push(pix.pixel(x, y).unwrap().red() < 100);
        }
    }
    Some(line)
}

/// Taps reach Dart: UIKit hit-tests FlutterView, the touches go up the
/// responder chain to FlutterViewController, Flutter's GestureDetector counts
/// them and the frame re-renders. A tap on the page outside the square doesn't count.
#[test]
fn taps_are_counted() {
    const CIRCLE: (f64, f64) = (195.0, 422.0);
    const SQUARE_CORNER: (f64, f64) = (60.0, 300.0);
    const PAGE: (f64, f64) = (195.0, 100.0);
    let Some(none) = counter_line(&[]) else { return };
    let one = counter_line(&[CIRCLE]).unwrap();
    let two = counter_line(&[CIRCLE, SQUARE_CORNER]).unwrap();
    let two_and_page = counter_line(&[CIRCLE, SQUARE_CORNER, PAGE]).unwrap();
    assert_ne!(none, one, "a tap didn't change the counter");
    assert_ne!(one, two, "a second tap didn't change the counter");
    assert_ne!(none, two);
    assert_eq!(two, two_and_page, "a tap outside the square was counted");
}

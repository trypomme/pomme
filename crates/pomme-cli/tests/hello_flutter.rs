//! End to end: a Flutter app (Impeller on Metal). Skipped unless the app
//! (scripts/build-flutter.sh hello-flutter), the sysroot and the Impeller MSL
//! (scripts/flutter/impeller-msl.sh) have been built.

mod cjk_lines;

use pomme_hle::Emu;
use std::path::Path;

const APP: &str = "../../apps/hello-flutter/build/ios/iphoneos/Runner.app";
const SYSROOT: &str = "../../vendor/sysroot";

fn boot() -> Option<Emu> {
    boot_on(pomme_hle::device::default())
}

fn boot_on(device: &'static pomme_hle::device::Device) -> Option<Emu> {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() || !sysroot.join("pomme/impeller").exists() {
        eprintln!("skipping: build hello-flutter, the sysroot and the Impeller MSL first");
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    let mut emu = Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap();
    emu.device = device;
    Some(emu)
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

/// Flutter's TextField: a tap focuses it (Flutter's text input view becomes
/// first responder and the keyboard comes up: 304 pt, since `autocorrect:
/// false` turns spell checking off too and iOS drops the predictive bar); the
/// Scaffold shrinks by the keyboard's height, lifting the field above it; keys
/// arrive through UIKeyInput; return (asked about through
/// shouldChangeTextInRange:replacementText:, as the iOS keyboard does) submits,
/// unfocuses and lowers the keyboard.
#[test]
fn text_field_and_keyboard_inset() {
    // Run with the tap and typing (and return, if `submit`) until `secs`; then
    // whether the keyboard is up, its height and the colors at `points` (pt).
    let run = |secs: f64, submit: bool, points: &[(f64, f64)]| -> Option<(bool, f64, Vec<(u8, u8, u8)>)> {
        let mut emu = boot()?;
        tap(&mut emu, (195.0, 800.0), 2.0);
        emu.input.script.push(pomme_hle::textinput::KeyInput { at_ns: 3_500_000_000, text: "Adx\u{8}\u{8}da".into() });
        if submit {
            emu.input.script.push(pomme_hle::textinput::KeyInput { at_ns: 5_000_000_000, text: "\n".into() });
        }
        emu.time_limit_ns = Some((secs * 1e9) as u64);
        match emu.run_main() {
            Err(pomme_hle::Stop::TimeLimit(_)) => {}
            other => panic!("expected the app to idle until the time limit, got {other:?}"),
        }
        if emu.metal.gpu_error.is_some() {
            return None;
        }
        let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
        let colors = points
            .iter()
            .map(|&(x, y)| {
                let c = pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap();
                (c.red(), c.green(), c.blue())
            })
            .collect();
        Some((emu.input.keyboard_shown, emu.input.keyboard_height, colors))
    };
    let near = |(r, g, b): (u8, u8, u8), (er, eg, eb): (u8, u8, u8)| r.abs_diff(er) < 12 && g.abs_diff(eg) < 12 && b.abs_diff(eb) < 12;
    const FOCUSED: (u8, u8, u8) = (103, 80, 164);
    const BORDER: (u8, u8, u8) = (121, 116, 126);
    const KEYBOARD: (u8, u8, u8) = (226, 228, 232);

    // The field's focused bottom border sits 16 pt above the keyboard (844 − 304 = 540).
    let Some((shown, height, c)) = run(4.5, false, &[(200.0, 523.0), (195.0, 700.0)]) else { return };
    assert!(shown, "the keyboard should be up");
    assert_eq!(height, 304.0);
    assert!(near(c[0], FOCUSED), "focused border: {:?}", c[0]);
    assert!(near(c[1], KEYBOARD), "keyboard: {:?}", c[1]);

    // After return: unfocused, back at the bottom, no keyboard.
    let (shown, _, c) = run(7.0, true, &[(200.0, 827.5), (195.0, 700.0)]).unwrap();
    assert!(!shown, "return should unfocus and lower the keyboard");
    assert!(near(c[0], BORDER), "resting border: {:?}", c[0]);
    assert!(near(c[1], (255, 255, 255)), "page: {:?}", c[1]);
}

/// Other phones: the app lays out for the device's screen and safe area, and
/// screenshots carry its sensor housing. On an iPhone 17 (402×874 pt, 62 pt
/// top inset) the frame matches the iOS 26 Simulator's, Dynamic Island
/// included (126×37.33 pt, 14 pt from the top); the keyboard lifts the field
/// as on the 16e. On an iPhone SE (375×667 @2x, home button) there's no
/// cutout, and the page fills the top of the screen.
#[test]
fn other_devices() {
    let find = |id| pomme_hle::device::find(id).unwrap();
    let run = |device, focus: bool, scale: f32, points: &[(f64, f64)]| -> Option<(f64, Vec<(u8, u8, u8)>)> {
        let mut emu = boot_on(device)?;
        if focus {
            tap(&mut emu, (201.0, 830.0), 2.0);
        }
        emu.time_limit_ns = Some(4_000_000_000);
        match emu.run_main() {
            Err(pomme_hle::Stop::TimeLimit(_)) => {}
            other => panic!("expected the app to idle until the time limit, got {other:?}"),
        }
        if emu.metal.gpu_error.is_some() {
            return None;
        }
        let pix = pomme_hle::render::render(&mut emu, scale).unwrap();
        assert_eq!((pix.width(), pix.height()), ((device.width * scale as f64) as u32, (device.height * scale as f64) as u32));
        let colors = points
            .iter()
            .map(|&(x, y)| {
                let c = pix.pixel((x * scale as f64) as u32, (y * scale as f64) as u32).unwrap();
                (c.red(), c.green(), c.blue())
            })
            .collect();
        Some((if emu.input.keyboard_shown { emu.input.keyboard_height } else { 0.0 }, colors))
    };
    let near = |(r, g, b): (u8, u8, u8), (er, eg, eb): (u8, u8, u8)| r.abs_diff(er) < 12 && g.abs_diff(eg) < 12 && b.abs_diff(eb) < 12;
    const WHITE: (u8, u8, u8) = (255, 255, 255);
    const BLACK: (u8, u8, u8) = (0, 0, 0);
    const SLATE: (u8, u8, u8) = (0x1e, 0x29, 0x3b);

    // iPhone 17: the island, the page around it, and the square at (51, 287)–(351, 587) pt as in the Simulator.
    let points = [(201.0, 32.0), (201.0, 8.0), (130.0, 32.0), (275.0, 32.0), (80.0, 320.0), (322.0, 555.0), (201.0, 270.0), (201.0, 600.0)];
    let Some((kb, c)) = run(find("iphone-17"), false, 3.0, &points) else { return };
    assert_eq!(kb, 0.0);
    assert!(near(c[0], BLACK) && near(c[1], WHITE) && near(c[2], WHITE) && near(c[3], WHITE), "island and page: {:?}", &c[..4]);
    assert!(near(c[4], SLATE) && near(c[5], SLATE), "square: {:?}", &c[4..6]);
    assert!(near(c[6], WHITE) && near(c[7], WHITE), "above and below the square: {:?}", &c[6..]);

    // Focused: the 304 pt keyboard (the iPhone 17's, measured) with the field's border 17 pt above it (874 − 304 − 17).
    let (kb, c) = run(find("iphone-17"), true, 3.0, &[(201.0, 553.0), (201.0, 750.0)]).unwrap();
    assert_eq!(kb, 304.0);
    assert!(near(c[0], (103, 80, 164)), "focused border: {:?}", c[0]);
    assert!(near(c[1], (226, 228, 232)), "keyboard: {:?}", c[1]);

    // iPhone SE: no cutout; the page reaches the top edge; the square is still centered.
    let (_, c) = run(find("iphone-se"), false, 2.0, &[(187.0, 2.0), (187.0, 20.0), (40.0, 250.0)]).unwrap();
    assert!(near(c[0], WHITE) && near(c[1], WHITE), "top of an SE: {:?}", &c[..2]);
    assert_eq!(c.len(), 3);
}

/// A paste into the TextField: Flutter's text input view takes `paste:`,
/// the framework reads the clipboard over its platform channel
/// (UIPasteboard's string, without a prompt: the user chose to paste) and
/// inserts it. Compared by ink in the field: "Hi " alone, then with "Ada Lovelace" pasted after it.
#[test]
fn paste_into_text_field() {
    let ink = |paste: bool| -> Option<(usize, u32)> {
        let mut emu = boot()?;
        tap(&mut emu, (195.0, 800.0), 2.0);
        emu.input.script.push(pomme_hle::textinput::KeyInput { at_ns: 3_500_000_000, text: "Hi ".into() });
        if paste {
            emu.clipboard.script.push(pomme_hle::clipboard::PasteInput { at_ns: 4_000_000_000, text: Some("Ada Lovelace".into()) });
        }
        emu.time_limit_ns = Some(5_000_000_000);
        match emu.run_main() {
            Err(pomme_hle::Stop::TimeLimit(_)) => {}
            other => panic!("expected the app to idle until the time limit, got {other:?}"),
        }
        if emu.metal.gpu_error.is_some() {
            return None;
        }
        let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
        // The focused field's text line (the field sits on the 304-pt keyboard).
        let mut n = 0;
        for y in (485 * 3)..(510 * 3) {
            for x in (25 * 3)..(370 * 3) {
                if pix.pixel(x, y).unwrap().red() < 100 {
                    n += 1;
                }
            }
        }
        Some((n, emu.clipboard.prompts))
    };
    let Some((typed, _)) = ink(false) else { return };
    let (pasted, prompts) = ink(true).unwrap();
    assert!(typed > 300, "\"Hi \": {typed} dark pixels");
    assert!(pasted > typed * 3, "\"Hi Ada Lovelace\": {pasted} dark pixels against {typed}");
    assert_eq!(prompts, 0);
}

/// Flutter finds its resources in its own framework's bundle:
/// `[NSBundle bundleForClass:]` of an engine class is Flutter.framework, where
/// its ICU data (icudtl.dat, for line breaks) is. And a long greeting wraps at
/// a word: "Lovelace!" starts the second line, at the left edge, which a short
/// greeting leaves empty.
#[test]
fn framework_bundle_and_text_wrapping() {
    let run = |name: &str| -> Option<(Emu, usize)> {
        let mut emu = boot()?;
        tap(&mut emu, (195.0, 800.0), 2.0);
        emu.input.script.push(pomme_hle::textinput::KeyInput { at_ns: 3_500_000_000, text: name.into() });
        emu.time_limit_ns = Some(5_000_000_000);
        match emu.run_main() {
            Err(pomme_hle::Stop::TimeLimit(_)) => {}
            other => panic!("expected the app to idle until the time limit, got {other:?}"),
        }
        if emu.metal.gpu_error.is_some() {
            return None;
        }
        let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
        // The greeting's second line, left of "Tapped 0 times".
        let mut n = 0;
        for y in (438 * 3)..(457 * 3) {
            for x in (17 * 3)..(92 * 3) {
                if pix.pixel(x, y).unwrap().red() < 100 {
                    n += 1;
                }
            }
        }
        Some((emu, n))
    };
    let Some((_, short)) = run("Ada") else { return };
    let (mut emu, long) = run("Augusta Ada King, Countess of Lovelace").unwrap();
    assert!(short < 20, "\"Hello, Ada!\" is one centered line: {short} dark pixels at the left");
    assert!(long > 300, "\"Lovelace!\" should wrap onto the second line: {long} dark pixels");

    let icu = pomme_hle::bundle_resource(&mut emu, "FlutterViewController", "icudtl.dat").unwrap();
    assert!(icu.as_deref().is_some_and(|p| p.ends_with("/Runner.app/Frameworks/Flutter.framework/icudtl.dat")), "{icu:?}");
}

/// Scrolling in Flutter (the pommeflutter:///list deep link): Flutter drives
/// its ListView from the raw touches, with its own iOS physics. Row i is
/// painted with red = i, so the red at a point says which row is there. A
/// fling carries the list well past where the finger left it; once it stops,
/// a tap lands on the row under the finger (it turns black).
#[test]
fn list_flings_and_taps() {
    // Red at (195, 500) after running `secs` with `script`; None if skipped.
    let run = |secs: f64, script: &dyn Fn(&mut Vec<pomme_hle::touch::Input>)| -> Option<(u8, u8, u8)> {
        let mut emu = boot()?;
        emu.ui.launch_url = Some("pommeflutter:///list".into());
        script(&mut emu.touch.script);
        emu.time_limit_ns = Some((secs * 1e9) as u64);
        match emu.run_main() {
            Err(pomme_hle::Stop::TimeLimit(_)) => {}
            other => panic!("expected the app to idle until the time limit, got {other:?}"),
        }
        if emu.metal.gpu_error.is_some() {
            return None;
        }
        let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
        let c = pix.pixel(195 * 3, 500 * 3).unwrap();
        Some((c.red(), c.green(), c.blue()))
    };
    let Some(still) = run(3.0, &|_| {}) else { return };
    // Rows start below the 47-pt safe area: (500 − 47) / 50 → row 9.
    assert_eq!(still, (9, 246, 128));
    let fling = |s: &mut Vec<pomme_hle::touch::Input>| pomme_hle::touch::swipe(s, 3_000_000_000, (195.0, 700.0), (195.0, 300.0), 200_000_000);
    let flung = run(7.0, &fling).unwrap();
    // Dragged 400 pt (8 rows) and then some.
    assert!(flung.0 > 9 + 8 + 5 && flung.1 == 255 - flung.0 && flung.2 == 128, "{flung:?}");
    let tapped = run(8.0, &|s| {
        fling(s);
        pomme_hle::touch::tap(s, 7_000_000_000, 195.0, 500.0);
    })
    .unwrap();
    assert_eq!(tapped, (0, 0, 0));
}

/// The frame after running held `held`, turning at the given times, as a
/// point → (r, g, b) lookup at 3×; None without a GPU.
fn landscape_frame(held: pomme_hle::device::Orientation, turns: &[(f64, pomme_hle::device::Orientation)], secs: f64) -> Option<(u32, u32, impl Fn(f64, f64) -> (u8, u8, u8))> {
    let mut emu = boot()?;
    emu.ui.device_orientation = held;
    emu.ui.rotations = turns.iter().map(|&(t, o)| ((t * 1e9) as u64, o)).collect();
    emu.time_limit_ns = Some((secs * 1e9) as u64);
    match emu.run_main() {
        Err(pomme_hle::Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}"),
    }
    if emu.metal.gpu_error.is_some() {
        return None;
    }
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let (w, h) = (pix.width(), pix.height());
    Some((w, h, move |x: f64, y: f64| {
        let c = pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap();
        (c.red(), c.green(), c.blue())
    }))
}

fn near((r, g, b): (u8, u8, u8), (er, eg, eb): (u8, u8, u8)) -> bool {
    r.abs_diff(er) < 12 && g.abs_diff(eg) < 12 && b.abs_diff(eb) < 12
}

/// Launched held sideways (Runner's Info.plist allows landscape): Flutter
/// lays the page out at 844 × 390, the square centered across the width,
/// and the notch is on the left (the phone was turned to the left).
#[test]
fn launches_in_landscape() {
    use pomme_hle::device::Orientation::LandscapeRight;
    let Some((w, h, at)) = landscape_frame(LandscapeRight, &[], 3.0) else { return };
    assert_eq!((w, h), (844 * 3, 390 * 3));
    // The square spans (272, 45)–(572, 345) pt.
    assert!(near(at(290.0, 60.0), (0x1e, 0x29, 0x3b)), "square: {:?}", at(290.0, 60.0));
    assert!(near(at(555.0, 330.0), (0x1e, 0x29, 0x3b)), "square: {:?}", at(555.0, 330.0));
    assert!(near(at(150.0, 200.0), (255, 255, 255)), "page: {:?}", at(150.0, 200.0));
    assert_eq!(at(5.0, 195.0), (0, 0, 0), "the notch, on the left");
    assert!(near(at(839.0, 195.0), (255, 255, 255)), "nothing on the right: {:?}", at(839.0, 195.0));
}

/// Turned while running: FlutterViewController hears the new size and the
/// engine re-lays the page out for it; the notch is now on the right.
#[test]
fn turns_while_running() {
    use pomme_hle::device::Orientation::{LandscapeLeft, Portrait};
    let Some((w, h, at)) = landscape_frame(Portrait, &[(2.0, LandscapeLeft)], 4.0) else { return };
    assert_eq!((w, h), (844 * 3, 390 * 3));
    assert!(near(at(290.0, 60.0), (0x1e, 0x29, 0x3b)), "square: {:?}", at(290.0, 60.0));
    assert!(near(at(150.0, 200.0), (255, 255, 255)), "page: {:?}", at(150.0, 200.0));
    assert_eq!(at(839.0, 195.0), (0, 0, 0), "the notch, on the right");
}

/// The browser's bundle (`pomme bundle`) carries the engine's Impeller MSL,
/// so the app runs from the bundle alone: Flutter's render passes run and no
/// precompiled library goes without its substitute MSL.
#[test]
fn runs_from_a_bundle_with_its_impeller_msl() {
    let Some(_) = boot() else { return };
    let pomme = env!("CARGO_BIN_EXE_pomme");
    let out = std::env::temp_dir().join(format!("hello-flutter-{}.pfs", std::process::id()));
    let made = std::process::Command::new(pomme).args(["bundle", APP, "--sysroot", SYSROOT, "-o"]).arg(&out).output().unwrap();
    assert!(made.status.success(), "pomme bundle: {}", String::from_utf8_lossy(&made.stderr));
    let run = std::process::Command::new(pomme)
        .args(["run", APP, "--bundle"]) // (the path is required, but the bundle's app runs)
        .arg(&out)
        .args(["--steps", "4000000000", "--seconds", "3"])
        .env("POMME_TRACE_METAL", "1")
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&out);
    let text = String::from_utf8_lossy(&run.stdout);
    assert!(!text.contains("has no substitute MSL"), "the bundle lacks Impeller's MSL:\n{}", &text[..text.len().min(4000)]);
    assert!(text.contains("cmd: pass"), "no render pass ran:\n{}", &text[text.len().saturating_sub(4000)..]);
}

/// Japanese in Flutter: Skia finds the fallback fonts through CoreText and
/// shapes with them, so its widths are Noto's own (a full em, with Noto Sans
/// JP's kerning). What hello-flutter prints at launch (each character's width,
/// and where the paragraph breaks at five widths) keeps the line rules
/// (cjk_lines), on a Japanese and on an English phone.
#[test]
fn japanese_text() {
    for language in ["ja-JP", "en-US"] {
        let Some(mut emu) = boot() else { return };
        emu.locales.settings.languages = vec![language.into()];
        emu.locales.settings.region = language[3..].into();
        emu.time_limit_ns = Some(3_000_000_000);
        let _ = emu.run_main();
        let ours: Vec<&str> = emu.console.lines().filter_map(|l| l.find("FLUTTER").map(|i| &l[i..])).collect();
        let mut wrong = Vec::new();
        // `FLUTTERW U+XXXX <width>`: a full em.
        let widths: Vec<&str> = ours.iter().copied().filter(|l| l.starts_with("FLUTTERW ")).collect();
        if widths.is_empty() {
            wrong.push("no FLUTTERW lines".to_string());
        }
        for l in &widths {
            let w: f64 = l.rsplit_once(' ').unwrap().1.parse().unwrap();
            if (w - cjk_lines::SIZE).abs() > 0.01 {
                wrong.push(format!("{l}: not a full em"));
            }
        }
        // `FLUTTER <width>.<line> <text> <line width>`.
        for width in cjk_lines::WIDTHS {
            let prefix = format!("FLUTTER {width}.");
            let lines: Vec<(String, f64)> = ours
                .iter()
                .filter_map(|l| l.strip_prefix(&prefix))
                .filter_map(|l| {
                    let (_, rest) = l.split_once(' ')?;
                    let (text, w) = rest.rsplit_once(' ')?;
                    Some((text.to_string(), w.parse().ok()?))
                })
                .collect();
            // (Kerning may take a few points from a line.)
            wrong.extend(cjk_lines::faults(&format!("FLUTTER {width}"), &lines, width as f64, 3.0));
        }
        assert!(wrong.is_empty(), "{language}:\n  {}", wrong.join("\n  "));
    }
}

/// The Japanese page (pommeflutter:///japanese) is drawn: the paragraph's
/// box (200 pt from x 20, from y 120) has ink on each of its first lines, not
/// empty boxes. (POMME_SAVE_JAPANESE=path saves the frame.)
#[test]
fn japanese_page_draws() {
    let Some(mut emu) = boot() else { return };
    emu.locales.settings.languages = vec!["ja-JP".into()];
    emu.ui.launch_url = Some("pommeflutter:///japanese".into());
    emu.time_limit_ns = Some(3_000_000_000);
    let _ = emu.run_main();
    if emu.metal.gpu_error.is_some() {
        return;
    }
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    if let Some(path) = std::env::var_os("POMME_SAVE_JAPANESE") {
        pix.save_png(path).unwrap();
    }
    // Each of the first five lines (about 20 pt apart from y 120) has dark pixels across it.
    for line in 0..5 {
        let y0 = 120 + line * 20;
        let dark = (y0 * 3..(y0 + 18) * 3).flat_map(|y| (20 * 3..220 * 3).map(move |x| (x, y))).filter(|&(x, y)| pix.pixel(x, y).unwrap().red() < 100).count();
        assert!(dark > 400, "line {line}: only {dark} dark pixels");
    }
}

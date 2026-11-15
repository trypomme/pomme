//! End to end: Uniwind (Tailwind CSS for React Native, apps/hello-uniwind) on
//! pomme, held to the iOS 26 Simulator. The app is a catalog of the
//! Tailwind classes Uniwind supports, a page per group (colors, borders,
//! effects, transforms, layout; text pages are checked by eye, as their
//! typeface differs), each class on a 64-pt tile. fixtures/uniwind has
//! the Simulator's screen sampled on and around every tile, in light and dark
//! (scripts/uniwind/record.sh). What Uniwind's classes turn into is plain
//! React Native style, so this holds pomme's drawing of React Native's
//! styles to iOS: backgrounds and gradients, borders (per side, dashed,
//! dotted), corners, outlines and rings, box shadows (outset, inset, spread),
//! opacity, blend modes, brightness, transforms, and flexbox layout.
//! Skipped unless the app (scripts/build-app.sh hello-uniwind) and the
//! sysroot have been built.

use pomme_hle::{Emu, Stop};
use std::collections::BTreeMap;
use std::path::Path;

const APP: &str = "../../apps/hello-uniwind/ios/build/Build/Products/Release-iphoneos/hellouniwind.app";
const SYSROOT: &str = "../../vendor/sysroot";

fn fixture(file: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/uniwind/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The app, launched at a page of its catalog (`pommeuniwind:///<page>`), idle.
fn page(name: &str, dark: bool) -> Option<Emu> {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/Foundation.framework").exists() {
        eprintln!("skipping: build hello-uniwind (scripts/build-app.sh hello-uniwind) and the sysroot first");
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    let mut emu = Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap();
    emu.ui.launch_url = Some(format!("pommeuniwind:///{name}"));
    emu.appearance.dark = dark;
    emu.time_limit_ns = Some(6_000_000_000);
    match emu.run_main() {
        Err(Stop::TimeLimit(_)) => Some(emu),
        other => panic!("expected {name} to idle until the time limit, got {other:?}\n{}", emu.describe_views()),
    }
}

/// Every sample of `page` within `tolerance` (per channel) of the Simulator's.
fn matches_simulator(name: &str, dark: bool) {
    let Some(mut emu) = page(name, dark) else { return };
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let file = if dark { "ios26-dark-pixels.txt" } else { "ios26-pixels.txt" };
    let mut wrong = BTreeMap::new();
    let mut checked = 0;
    for line in fixture(file).lines().filter(|l| l.starts_with(&format!("PIXEL {name}."))) {
        let f: Vec<&str> = line.split(' ').collect();
        let (sample, x, y): (&str, f64, f64) = (f[1], f[2].parse().unwrap(), f[3].parse().unwrap());
        let want: Vec<i32> = f[4..7].iter().map(|v| v.parse().unwrap()).collect();
        let c = pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap().demultiply();
        let got = [c.red() as i32, c.green() as i32, c.blue() as i32];
        checked += 1;
        if got.iter().zip(&want).any(|(g, w)| (g - w).abs() > TOLERANCE) {
            wrong.insert(sample.to_string(), format!("{got:?}, iOS {want:?}"));
        }
    }
    assert!(checked > 100, "{name}: {checked} samples");
    assert!(wrong.is_empty(), "{name} ({}): {} of {checked} samples off: {wrong:#?}", if dark { "dark" } else { "light" }, wrong.len());
}

/// Per channel, out of 255: blur, gradients and antialiasing differ a little from Core Animation's.
const TOLERANCE: i32 = 16;

#[test]
fn colors_and_gradients() {
    matches_simulator("colors", false);
}

#[test]
fn borders_corners_outlines_rings() {
    matches_simulator("borders", false);
}

#[test]
fn shadows_opacity_blend_modes_filters() {
    matches_simulator("effects", false);
}

#[test]
fn transforms() {
    matches_simulator("transforms", false);
}

#[test]
fn layout() {
    matches_simulator("layout", false);
}

#[test]
fn colors_dark() {
    matches_simulator("colors", true);
}

#[test]
fn effects_dark() {
    matches_simulator("effects", true);
}

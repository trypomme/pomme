//! Liquid Glass views, held to iOS 26's. The glass fixture puts regular,
//! clear and tinted glass (UIGlassEffect in a UIVisualEffectView), rounded
//! three ways (none, the layer's radius, iOS 26's corner configuration), a
//! container of two, and glass with a label, over white and over black and
//! orange stripes. fixtures/glass/ios26.txt has the views' frames as the
//! Simulator reported them, measured/glass.log what it said of their
//! effects, and ios26-pixels.txt its screen sampled at points.txt.
//! Skipped without Xcode, the sysroot, or the sysroot's UIKit overlay
//! (scripts/sysroot/build-overlays.sh), which apps reach corners through.

mod common;

use std::collections::BTreeMap;

const SYSROOT: &str = "../../vendor/sysroot";

fn fixture(file: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/glass/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

fn boot() -> Option<pomme_hle::Emu> {
    if !std::path::Path::new(SYSROOT).join("System/Library/Frameworks/UIKit.framework/UIKit").exists() {
        eprintln!("skipping: build the sysroot's UIKit overlay first (scripts/sysroot/build-overlays.sh)");
        return None;
    }
    common::boot("glass")
}

/// The glass views where iOS puts them, their content views filling them,
/// and their effects (a container's children glass of their own).
#[test]
fn glass_views_are_as_ios_makes_them() {
    let Some(mut emu) = boot() else { return };
    common::views(&mut emu, 3.0);
    let console = emu.console.clone();
    let ours = common::parse_metrics(&console);
    let mut wrong = Vec::new();
    for (name, want) in common::parse_metrics(&fixture("ios26.txt")) {
        match ours.get(&name) {
            Some(got) if got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 0.5) => {}
            got => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
        }
    }
    assert!(wrong.is_empty(), "{wrong:#?}");
    let effects = |text: &str| -> Vec<String> { text.lines().filter(|l| l.starts_with("EFFECT ")).map(|l| l.replace('\r', "")).collect() };
    assert_eq!(effects(&console), effects(&fixture("measured/glass.log")));
    // Each effect view holds its content view, and what's added to it.
    let structure = |text: &str| -> Vec<String> {
        text.lines().filter(|l| l.starts_with("VIEW label") || l.starts_with("VIEW regular")).map(|l| l.split(" bg ").next().unwrap().to_string()).collect()
    };
    assert_eq!(structure(&console), structure(&fixture("measured/glass.log")));
}

/// What the glass looks like, sampled where the Simulator was: away from
/// the glass's rims (iOS bends the backdrop there; pomme doesn't), over each
/// stripe and over white, regular, clear and tinted; and the shadow below
/// regular and tinted glass.
#[test]
fn glass_looks_as_on_ios() {
    glass_looks_as_recorded("", "ios26-pixels.txt");
}

/// And in the dark appearance (the window's, as the probe asks).
#[test]
fn dark_glass_looks_as_on_ios() {
    glass_looks_as_recorded("dark", "ios26-dark-pixels.txt");
}

fn glass_looks_as_recorded(variant: &str, file: &str) {
    let Some(mut emu) = boot() else { return };
    // (The probe turns its window dark; pomme's system colours, the tint's, follow the system's.)
    emu.appearance.dark = variant == "dark";
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", variant);
    common::views(&mut emu, 3.0);
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let mut wrong = BTreeMap::new();
    let mut checked = 0;
    for line in fixture(file).lines().filter(|l| l.starts_with("PIXEL ")) {
        let f: Vec<&str> = line.split(' ').collect();
        let (name, x, y): (&str, f64, f64) = (f[1], f[2].parse().unwrap(), f[3].parse().unwrap());
        let want: Vec<i32> = f[4..7].iter().map(|v| v.parse().unwrap()).collect();
        // (Not a container's: iOS joins glass that close into one shape,
        // bridged and bent, which pomme doesn't yet; nor the label's text.)
        if name.starts_with("container") || (name == "label.mid" && x < 160.0) {
            continue;
        }
        let tolerance = match name.split('.').nth(1) {
            // Inside the glass, clear of its rims.
            Some("mid") if (30.0..=360.0).contains(&x) => 10,
            Some("shadow") if !name.starts_with("clear") => 6,
            _ => continue,
        };
        let c = pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap().demultiply();
        let got = [c.red() as i32, c.green() as i32, c.blue() as i32];
        checked += 1;
        if got.iter().zip(&want).any(|(g, w)| (g - w).abs() > tolerance) {
            wrong.insert(format!("{name} {x} {y}"), format!("{got:?}, iOS {want:?}"));
        }
    }
    assert!(checked > 60, "{checked} samples");
    assert!(wrong.is_empty(), "{variant}: {} of {checked} off: {wrong:#?}", wrong.len());
}

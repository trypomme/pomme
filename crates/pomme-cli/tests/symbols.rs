//! SF Symbols and SwiftUI's Image and Label, held to iOS 26: the
//! symbolmetrics probe's report in pomme against its recording on the
//! simulator (fixtures/symbolmetrics/ios26.txt). Each symbol's size and
//! baseline at each scale, sizes across point sizes, weights and text styles,
//! and images and labels laid out in a list and outside one. Skipped without
//! Xcode or the sysroot.

mod common;

use std::collections::BTreeMap;

/// Each report's last values, by kind and name (`SYMBOL star 22 20 3.33`).
fn reports(text: &str) -> BTreeMap<(String, String), Vec<String>> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 3 && ["SYMBOL", "SMALL", "LARGE", "CONFIG", "METRIC"].contains(&f[0]) {
            out.insert((f[0].to_string(), f[1].to_string()), f[2..].iter().map(|s| s.to_string()).collect());
        }
    }
    out
}

fn numbers(v: &[String]) -> Vec<f64> {
    v.iter().map(|s| s.parse().unwrap_or(f64::NAN)).collect()
}

fn probe() -> Option<(BTreeMap<(String, String), Vec<String>>, BTreeMap<(String, String), Vec<String>>)> {
    let ios = reports(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/symbolmetrics/ios26.txt")).unwrap());
    let mut emu = common::boot("symbolmetrics")?;
    common::views(&mut emu, 4.0);
    Some((ios, reports(&emu.console)))
}

/// Every symbol pomme has a stand-in for is the size iOS makes it, with its
/// baseline, at each scale; and sizes follow the configuration. (A name iOS
/// doesn't know gives nil there; pomme can't tell, and draws a placeholder.)
#[test]
fn symbol_sizes_match_ios() {
    let Some((ios, ours)) = probe() else { return };
    let mut wrong = Vec::new();
    for ((kind, name), want) in ios.iter().filter(|((k, _), _)| k != "METRIC") {
        if name == "not.a.symbol" {
            continue;
        }
        let Some(got) = ours.get(&(kind.clone(), name.clone())) else {
            wrong.push(format!("{kind} {name}: missing"));
            continue;
        };
        // (Scaled sizes land within a pixel or two of iOS's rounding.)
        let tolerance = if kind == "CONFIG" { 0.7 } else { 0.01 };
        if numbers(want).iter().zip(numbers(got)).any(|(a, b)| (a - b).abs() > tolerance) {
            wrong.push(format!("{kind} {name}: {got:?}, iOS {want:?}"));
        }
    }
    assert!(wrong.is_empty(), "{} differences from iOS:\n  {}", wrong.len(), wrong.join("\n  "));
}

/// Images and labels laid out as on iOS: images at their symbol's size (the
/// font's, the image scale's); a list's label with its icon (large) centered
/// 12 pt in and its title 40 pt in; elsewhere the title 8 pt after the icon,
/// on its baseline. (Positions across the screen, and text widths, depend on
/// rows and fonts compared elsewhere: these are the images' sizes and each
/// label's icon and title relative to each other.)
#[test]
fn images_and_labels_lay_out_as_on_ios() {
    let Some((ios, ours)) = probe() else { return };
    let frame = |m: &BTreeMap<(String, String), Vec<String>>, name: &str| numbers(m.get(&("METRIC".into(), name.into())).unwrap_or_else(|| panic!("no {name}")));
    let mut wrong = Vec::new();
    let mut check = |what: String, got: f64, want: f64, tolerance: f64| {
        if (got - want).abs() > tolerance {
            wrong.push(format!("{what}: {got:.2}, iOS {want:.2}"));
        }
    };
    for (name, tolerance) in [("image", 0.01), ("image.large", 0.01), ("image.largeTitle", 0.7), ("hstack.icon", 0.01)] {
        let (o, i) = (frame(&ours, name), frame(&ios, name));
        check(format!("{name} width"), o[2], i[2], tolerance);
        check(format!("{name} height"), o[3], i[3], tolerance);
        check(format!("{name} x"), o[0], i[0], 0.01);
    }
    for label in ["label", "label2", "vlabel", "loose", "loose2", "loose3"] {
        let (oi, ot) = (frame(&ours, &format!("{label}.icon")), frame(&ours, &format!("{label}.title")));
        let (ii, it) = (frame(&ios, &format!("{label}.icon")), frame(&ios, &format!("{label}.title")));
        // (loose3's icon is at the title style's size: scaled.)
        check(format!("{label} icon size"), oi[2] + oi[3], ii[2] + ii[3], if label == "loose3" { 0.7 } else { 0.01 });
        check(format!("{label} icon x"), oi[0], ii[0], 0.01);
        check(format!("{label} title x"), ot[0], it[0], 0.01);
        check(format!("{label} icon above title"), ot[1] - oi[1], it[1] - ii[1], 0.5);
    }
    let hs = (frame(&ours, "hstack.text"), frame(&ios, "hstack.text"));
    check("hstack text x".into(), hs.0[0], hs.1[0], 0.01);
    assert!(wrong.is_empty(), "differences from iOS:\n  {}", wrong.join("\n  "));
}

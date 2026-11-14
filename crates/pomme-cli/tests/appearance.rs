//! Light and dark appearance: UIKit's system colours as iOS 26 resolves them
//! (fixtures/systemcolors/ios26.txt, recorded on its simulator), and the
//! appearance changing while an app runs. Skipped without Xcode or the sysroot.

mod common;

/// Every system colour, light and dark, within one step of 255 of iOS's.
#[test]
fn system_colors_match_ios() {
    let Some(mut emu) = common::boot("systemcolors") else { return };
    common::views(&mut emu, 2.0);
    let ours = common::parse_metrics(&emu.console);
    let ios = common::parse_metrics(&std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/systemcolors/ios26.txt")).unwrap());
    assert_eq!(ios.len(), 72);
    let mut wrong = Vec::new();
    for (name, want) in &ios {
        match ours.get(name) {
            Some(got) if got.iter().zip(want).all(|(a, b)| (a - b).abs() <= 1.0 / 255.0) => {}
            got => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
        }
    }
    assert!(wrong.is_empty(), "differences from iOS 26:\n  {}", wrong.join("\n  "));
}

/// Turned dark at 1 s: the app hears it (its traits' style goes from light
/// to dark), its system-background view turns black, and the status bar's
/// default style turns white.
#[test]
fn switching_to_dark() {
    let Some(mut emu) = common::boot("systemcolors") else { return };
    emu.appearance.changes.push((1_000_000_000, true));
    common::views(&mut emu, 2.0);
    let heard: Vec<&str> = emu.console.lines().filter(|l| l.starts_with("APPEARANCE ")).collect();
    assert_eq!(heard, ["APPEARANCE 1", "APPEARANCE 2"], "{}", emu.console);
    let pix = pomme_hle::render::render(&mut emu, 1.0).unwrap();
    let c = pix.pixel(195, 400).unwrap();
    assert_eq!((c.red(), c.green(), c.blue()), (0, 0, 0), "the background isn't black");
    let white = (0..47).flat_map(|y| (0..pix.width()).map(move |x| (x, y))).filter(|&(x, y)| pix.pixel(x, y).unwrap().red() > 200).count();
    assert!(white > 50, "the status bar isn't white ({white} white pixels)");
}

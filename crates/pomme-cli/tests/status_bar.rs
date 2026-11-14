//! The status bar, held to iOS 26's: on each phone recorded on the simulator
//! (fixtures/statusbar/ios26.txt, scripts/statusbar/simulator.sh), pomme's
//! status bar, pinned to the same time, signal and battery, puts its ink in
//! the same places to within half a point. And as apps shape it and read it:
//! light content, hiding it, and the battery through UIDevice. Skipped
//! without Xcode or the sysroot.

mod common;

use pomme_hle::Emu;
use pomme_hle::statusbar::{Battery, BatteryChange, Charge};

type Box4 = [f64; 4];

/// The status bar's ink in a frame rendered at `scale`, as
/// scripts/statusbar/inkboxes.py finds it: runs of columns with ink within
/// the top `band` points, with the rows each spans; the sensor housing
/// (wider than 100 pt) left out, as the notch isn't in the simulator's
/// screenshots.
fn ink_boxes(pix: &tiny_skia::Pixmap, scale: f64, band: f64) -> Vec<Box4> {
    let lum = |x: u32, y: u32| {
        let c = pix.pixel(x, y).unwrap();
        (c.red() as f64 + c.green() as f64 + c.blue() as f64) / 3.0
    };
    let bg = lum(2, 2);
    let rows = (band * scale) as u32;
    let ink = |x: u32, y: u32| (lum(x, y) - bg).abs() > 60.0;
    let mut runs: Vec<[u32; 2]> = Vec::new();
    for x in 0..pix.width() {
        if (0..rows).any(|y| ink(x, y)) {
            match runs.last_mut() {
                Some(r) if x - r[1] <= 2 => r[1] = x,
                _ => runs.push([x, x]),
            }
        }
    }
    runs.into_iter()
        .map(|[a, b]| {
            let ys: Vec<u32> = (0..rows).filter(|&y| (a..=b).any(|x| ink(x, y))).collect();
            [a as f64 / scale, (b + 1) as f64 / scale, ys[0] as f64 / scale, (*ys.last().unwrap() + 1) as f64 / scale]
        })
        .filter(|b| b[1] - b[0] <= 100.0)
        .collect()
}

#[test]
fn layout_matches_ios() {
    let recorded = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/statusbar/ios26.txt")).unwrap();
    let mut phones: Vec<(String, Vec<Box4>)> = Vec::new();
    for line in recorded.lines().filter(|l| l.starts_with("BOX ")) {
        let f: Vec<&str> = line.split_whitespace().collect();
        let b = [2, 3, 4, 5].map(|i| f[i].parse::<f64>().unwrap());
        if b[1] - b[0] > 100.0 {
            continue;
        }
        match phones.last_mut() {
            Some((id, boxes)) if id == f[1] => boxes.push(b),
            _ => phones.push((f[1].to_string(), vec![b])),
        }
    }
    assert_eq!(phones.len(), 6, "the recording's phones");
    let mut wrong = Vec::new();
    for (id, ios) in &phones {
        let Some(mut emu) = common::boot("largetitles") else { return };
        emu.device = pomme_hle::device::find(id).unwrap();
        emu.status_bar.overrides = "time=9:41,battery=100".parse().unwrap();
        common::views(&mut emu, 2.0);
        let scale = emu.device.scale;
        let band = ios.iter().map(|b| b[3]).fold(0.0, f64::max) + 2.0;
        let ours = ink_boxes(&pomme_hle::render::render(&mut emu, scale as f32).unwrap(), scale, band);
        // (The SE's "AM" is narrower in pomme's font than in SF: its time isn't compared.)
        let compared = |b: &Box4| !(id == "iphone-se" && b[0] > 150.0 && b[1] < 230.0);
        let (ours, ios): (Vec<&Box4>, Vec<&Box4>) = (ours.iter().filter(|b| compared(b)).collect(), ios.iter().filter(|b| compared(b)).collect());
        if ours.len() != ios.len() {
            wrong.push(format!("{id}: {} pieces, iOS {}:\n    {ours:?}\n    {ios:?}", ours.len(), ios.len()));
            continue;
        }
        for (o, i) in ours.iter().zip(&ios) {
            if o.iter().zip(i.iter()).any(|(a, b)| (a - b).abs() > 0.5) {
                wrong.push(format!("{id}: {o:?}, iOS {i:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "differences from iOS 26:\n  {}", wrong.join("\n  "));
}

/// Boot the statusbar fixture with the battery at 80%, dropping to 15% and
/// plugged in at 1 s; run to `secs`.
fn statusbar_app(secs: f64) -> Option<Emu> {
    let mut emu = common::boot("statusbar")?;
    emu.status_bar.battery = Battery { level: 0.8, charge: Charge::Unplugged };
    emu.status_bar.battery_changes.push((1_000_000_000, "15%,charging".parse::<BatteryChange>().unwrap()));
    common::views(&mut emu, secs);
    Some(emu)
}

/// Pixels in the status bar's band (the top 47 pt) that are white-ish, and red-ish.
fn status_ink(emu: &mut Emu) -> (usize, usize) {
    let pix = pomme_hle::render::render(emu, 1.0).unwrap();
    let (mut white, mut red) = (0, 0);
    for y in 0..47 {
        for x in 0..pix.width() {
            let c = pix.pixel(x, y).unwrap();
            if c.red() > 200 && c.green() > 200 && c.blue() > 200 {
                white += 1;
            }
            if c.red() > 200 && c.green() < 120 {
                red += 1;
            }
        }
    }
    (white, red)
}

/// The battery as UIDevice reports it: unknown until monitored, then the
/// phone's, and the changes with their notifications. The status bar shows
/// it, white on the app's black (light content), the low level red; hidden
/// once the app asks.
#[test]
fn apps_shape_it_and_read_the_battery() {
    let Some(mut emu) = statusbar_app(1.5) else { return };
    let lines: Vec<&str> = emu.console.lines().filter(|l| l.starts_with("BATTERY ")).collect();
    assert_eq!(lines, ["BATTERY unmonitored -1.0 0", "BATTERY monitored 0.8 1", "BATTERY state 0.15 2", "BATTERY level 0.15 2"], "{}", emu.console);
    let (white, red) = status_ink(&mut emu);
    assert!(white > 50, "no light content in the status bar ({white} white pixels)");
    assert!(red > 5, "the low battery isn't red ({red} red pixels)");

    let Some(mut emu) = statusbar_app(2.5) else { return };
    assert_eq!(status_ink(&mut emu), (0, 0), "the status bar shows after the app hid it");
}

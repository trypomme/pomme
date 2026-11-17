//! UIAlertController (tests/fixtures/alerts, recorded on the iOS 26
//! Simulator; notes.md there): the box, its texts and buttons where iOS puts
//! them, and the dimming, for each case the probe shows.
//! Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use std::collections::BTreeMap;

fn values(text: &str) -> BTreeMap<String, Vec<String>> {
    text.lines()
        .filter_map(|l| l.strip_prefix("VALUE "))
        .map(|l| {
            let mut f = l.split_whitespace().map(str::to_string);
            (f.next().unwrap(), f.collect())
        })
        .collect()
}

fn recording(case: &str) -> String {
    std::fs::read_to_string(format!("{}/tests/fixtures/alerts/ios26-{case}.txt", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// The probe's case in pomme: what it printed.
fn run(case: &str, secs: f64) -> Option<pomme_hle::Emu> {
    let mut emu = common::boot("alerts")?;
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", case);
    common::views(&mut emu, secs);
    Some(emu)
}

/// Each reported value against the recording: frames within half a point,
/// everything else (fonts, colours, radii) the same.
fn differences(case: &str, emu: &mut pomme_hle::Emu) -> Vec<String> {
    let (want, got) = (values(&recording(case)), values(&emu.console));
    let mut wrong = Vec::new();
    // iOS's buttons and box are its private classes (the probe finds them by
    // name); pomme's are a button for each action, and the controller's view.
    let buttons: Vec<[f64; 4]> = pomme_hle::accessibility::snapshot(emu).unwrap().iter().filter(|e| e.traits & 1 != 0).map(|e| e.frame).collect();
    for (k, w) in want.iter().filter(|(k, _)| k.starts_with("button")) {
        let f: Vec<f64> = w[..4].iter().map(|x| x.parse().unwrap()).collect();
        if !buttons.iter().any(|b| b.iter().zip(&f).all(|(a, b)| (a - b).abs() <= 0.5)) {
            wrong.push(format!("{k}: no button at {:?} (pomme's: {buttons:?})", f));
        }
    }
    for (k, w) in want.iter().filter(|(k, _)| !k.starts_with("button") && *k != "box") {
        let Some(g) = got.get(k) else {
            wrong.push(format!("{k}: not reported (iOS {})", w.join(" ")));
            continue;
        };
        let is_label = k.starts_with("label");
        // iOS reports the message's colour as the secondary label colour but
        // draws it vibrant (what's under it × 0.45): pomme sets the drawn one.
        // The pixels are compared instead (alerts_are_drawn_as_on_ios).
        let secondary = w.get(6..10).is_some_and(|c| c.join(" ") == "0.235 0.235 0.263 0.600");
        // A button's title: centred, so its x and width follow the font's
        // widths (pomme draws with Inter: docs/differences-from-ios.md), and its
        // reported colour isn't the drawn one on iOS (the pixels are checked
        // instead). Its vertical centre is compared.
        let button_title = is_label && w[0].parse::<f64>().is_ok_and(|x| (x - 65.0).abs() > 0.01);
        if button_title {
            let mid = |v: &[String]| v[1].parse::<f64>().unwrap() + v[3].parse::<f64>().unwrap() / 2.0;
            if (mid(w) - mid(g)).abs() > 0.5 || w[4..7] != g[4..7] {
                wrong.push(format!("{k}: iOS {}, pomme {}", w.join(" "), g.join(" ")));
            }
            continue;
        }
        for (i, (a, b)) in w.iter().zip(g).enumerate() {
            if secondary && (6..10).contains(&i) {
                continue;
            }
            match (a.parse::<f64>(), b.parse::<f64>()) {
                (Ok(x), Ok(y)) if (x - y).abs() <= 0.5 => {}
                _ if a == b => {}
                _ => {
                    wrong.push(format!("{k}: iOS {}, pomme {}", w.join(" "), g.join(" ")));
                    break;
                }
            }
        }
    }
    wrong
}

#[test]
fn alerts_and_action_sheets_are_laid_out_as_on_ios() {
    let mut all = Vec::new();
    for case in ["alert2", "alert3", "styles", "disabled", "sheet", "titleonly", "messageonly", "long"] {
        let Some(mut emu) = run(case, 4.0) else { return };
        for d in differences(case, &mut emu) {
            all.push(format!("{case}: {d}"));
        }
    }
    assert!(all.is_empty(), "{}", all.join("\n"));
}

/// The drawn colours at the sampled points (`points.txt`; `ios26-pixels-*.txt`),
/// over teal, white, black and orange: the dimming, the glass, the buttons'
/// fills and the texts' ink.
#[test]
fn alerts_are_drawn_as_on_ios() {
    let mut wrong = Vec::new();
    for case in ["styles", "disabled", "styles-white", "styles-black", "styles-orange"] {
        let Some(mut emu) = run(case, 4.0) else { return };
        let png = emu.screenshot_png(3.0).unwrap();
        if let Some(dir) = std::env::var_os("POMME_ALERT_SHOTS") {
            std::fs::write(std::path::Path::new(&dir).join(format!("pomme-{case}.png")), &png).unwrap();
        }
        let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
        let at = |x: f64, y: f64| {
            let c = pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap();
            [c.red() as i32, c.green() as i32, c.blue() as i32]
        };
        let text = std::fs::read_to_string(format!("{}/tests/fixtures/alerts/ios26-pixels-{case}.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
        // name → [(x, y, iOS colour)]
        let mut by_name: BTreeMap<String, Vec<(f64, f64, [i32; 3])>> = BTreeMap::new();
        for l in text.lines().filter(|l| l.starts_with("PIXEL ")) {
            let f: Vec<&str> = l.split(' ').collect();
            let c = [f[4].parse().unwrap(), f[5].parse().unwrap(), f[6].parse().unwrap()];
            by_name.entry(f[1].to_string()).or_default().push((f[2].parse().unwrap(), f[3].parse().unwrap(), c));
        }
        // The ink along a title: the pixel farthest from the line's commonest colour (its background).
        let far = |cs: &[[i32; 3]]| {
            let mut sorted = cs.to_vec();
            let bg: [i32; 3] = std::array::from_fn(|i| {
                sorted.sort_by_key(|c| c[i]);
                sorted[sorted.len() / 2][i]
            });
            *cs.iter().max_by_key(|c| (0..3).map(|i| (c[i] - bg[i]).pow(2)).sum::<i32>()).unwrap()
        };
        for (name, pts) in &by_name {
            let ios: Vec<[i32; 3]> = pts.iter().map(|p| p.2).collect();
            let ours: Vec<[i32; 3]> = pts.iter().map(|p| at(p.0, p.1)).collect();
            // One point: its colour. A line along a title: the ink, the pixel farthest from where the line starts.
            let (want, got) = if pts.len() == 1 { (ios[0], ours[0]) } else { (far(&ios), far(&ours)) };
            if (0..3).any(|i| (want[i] - got[i]).abs() > 12) {
                wrong.push(format!("{case} {name}: iOS {want:?}, pomme {got:?}"));
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// The probe's `EVENT` lines: (seconds, what).
fn events(console: &str) -> Vec<(f64, String)> {
    console
        .lines()
        .filter_map(|l| l.strip_prefix("EVENT "))
        .map(|l| {
            let (t, what) = l.split_once(' ').unwrap();
            (t.parse().unwrap(), what.to_string())
        })
        .collect()
}

/// The recorded events of a case (`ios26-events.txt`): (seconds, what).
fn ios_events(case: &str) -> Vec<(f64, String)> {
    let text = std::fs::read_to_string(format!("{}/tests/fixtures/alerts/ios26-events.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    text.lines()
        .filter_map(|l| l.strip_prefix(&format!("EVENT {case} ")))
        .map(|l| {
            let (t, what) = l.split_once(' ').unwrap();
            (t.parse().unwrap(), what.to_string())
        })
        .collect()
}

fn time_of(events: &[(f64, String)], what: &str) -> f64 {
    events.iter().find(|e| e.1 == what).unwrap_or_else(|| panic!("no {what:?} in {events:?}")).0
}

/// Run a case with the test driver: once the alert is up, `then` (a tap);
/// returns the events and when `then` ended.
fn drive_case(case: &str, then: impl FnOnce(&mut pomme_hle::driver::App) -> Result<(), pomme_hle::driver::Error> + Send) -> Option<(Vec<(f64, String)>, f64)> {
    use pomme_hle::driver::{drive, text};
    let mut emu = common::boot("alerts")?;
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", case);
    let at = drive(&mut emu, |app| {
        app.wait_for(&text("Delete the note?"))?;
        app.run_for(1.5)?;
        then(app)?;
        // (When the finger lifted: a tap ends there.)
        let at = app.now();
        app.run_for(1.5)?;
        Ok(at)
    })
    .unwrap();
    Some((events(&emu.console), at))
}

/// Tapping an action: it closes, then the handler runs, with nothing presented
/// any more (ios26-events.txt); appearing and closing take iOS's time.
#[test]
fn a_tap_closes_the_alert_then_runs_the_handler() {
    use pomme_hle::driver::text;
    let Some((ev, tapped)) = drive_case("alert2-tap", |app| app.tap(&text("OK")).map(|_| ())) else { return };
    let ios = ios_events("alert2-tap");
    let names: Vec<&str> = ev.iter().map(|e| e.1.as_str()).collect();
    assert_eq!(names, ios.iter().map(|e| e.1.as_str()).collect::<Vec<_>>(), "{ev:?}");
    let appear = time_of(&ev, "presented") - time_of(&ev, "present");
    let ios_appear = time_of(&ios, "presented") - time_of(&ios, "present");
    assert!((appear - ios_appear).abs() < 0.05, "appearing took {appear:.3} s, iOS {ios_appear:.3}");
    // From the tap to the handler: the close, as when it's closed in code.
    let ios_close = time_of(&ios_events("alert2-code"), "dismissed presented no") - time_of(&ios_events("alert2-code"), "dismiss in code");
    let close = time_of(&ev, "handler OK presented no") - tapped;
    assert!((close - ios_close).abs() < 0.05, "the handler ran {close:.3} s after the tap, iOS's close takes {ios_close:.3}");
}

/// A tap outside an action sheet is its cancel; outside an alert, nothing.
#[test]
fn a_tap_outside() {
    let Some((sheet, _)) = drive_case("sheet-outside", |app| app.tap_at(195.0, 100.0)) else { return };
    assert_eq!(sheet.last().map(|e| e.1.as_str()), Some("handler Cancel presented no"), "{sheet:?}");
    assert_eq!(ios_events("sheet-outside").last().map(|e| e.1.as_str()), Some("handler Cancel presented no"));
    let Some((alert, _)) = drive_case("alert2-outside", |app| app.tap_at(195.0, 100.0)) else { return };
    assert_eq!(alert.last().map(|e| e.1.as_str()), Some("presented"), "{alert:?}");
    assert_eq!(ios_events("alert2-outside").last().map(|e| e.1.as_str()), Some("presented"));
}

/// A disabled action ignores taps.
#[test]
fn a_disabled_action_does_nothing() {
    use pomme_hle::driver::text;
    let Some((ev, _)) = drive_case("disabled-tap", |app| app.tap(&text("Send")).map(|_| ())) else { return };
    assert_eq!(ev.last().map(|e| e.1.as_str()), Some("presented"), "{ev:?}");
}

/// Closed in code: no handler; the completion when it has gone.
#[test]
fn closing_in_code() {
    let Some((ev, _)) = drive_case("alert2-code", |_| Ok(())) else { return };
    let ios = ios_events("alert2-code");
    let names: Vec<&str> = ev.iter().map(|e| e.1.as_str()).collect();
    assert_eq!(names, ios.iter().map(|e| e.1.as_str()).collect::<Vec<_>>(), "{ev:?}");
    let took = time_of(&ev, "dismissed presented no") - time_of(&ev, "dismiss in code");
    let ios_took = time_of(&ios, "dismissed presented no") - time_of(&ios, "dismiss in code");
    assert!((took - ios_took).abs() < 0.05, "closing took {took:.3} s, iOS {ios_took:.3}");
}

type Frames = std::sync::Arc<std::sync::Mutex<Vec<(f64, u32, Vec<u8>)>>>;

struct Recording(Frames);

impl pomme_hle::host::Host for Recording {
    fn idle(&mut self, _: u64, _: Option<u64>) -> pomme_hle::host::Idle {
        pomme_hle::host::Idle::Advance
    }
    fn frame(&mut self, f: pomme_hle::host::Frame<'_>) {
        self.0.lock().unwrap().push((f.at_ns as f64 / 1e9, f.width, f.rgba.to_vec()));
    }
}

/// A recorded curve (`ios26-motion.txt`), from `presented` at time 0: linear between frames.
fn ios_curve(kind: &str, case: &str, col: usize) -> Vec<(f64, f64)> {
    let start = time_of(&ios_events(case), "present");
    let text = std::fs::read_to_string(format!("{}/tests/fixtures/alerts/ios26-motion.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    text.lines()
        .filter(|l| l.starts_with(&format!("{kind} {case} ")))
        .map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            (f[2].parse::<f64>().unwrap() - start, f[col].parse().unwrap())
        })
        .filter(|&(t, _)| t < 1.0)
        .collect()
}

fn at(curve: &[(f64, f64)], t: f64, before: f64) -> f64 {
    if curve.is_empty() || t < curve[0].0 {
        return before;
    }
    for w in curve.windows(2) {
        if t <= w[1].0 {
            let k = (t - w[0].0) / (w[1].0 - w[0].0);
            return w[0].1 + k * (w[1].1 - w[0].1);
        }
    }
    curve.last().unwrap().1
}

/// Appearing, frame by frame (ios26-motion.txt): the box's scale (its drawn
/// width) and the dimming (how dark the teal behind gets).
#[test]
fn the_alert_grows_into_place_as_on_ios() {
    let Some(mut emu) = common::boot("alerts") else { return };
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", "alert2");
    let frames: Frames = Default::default();
    *emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    common::views(&mut emu, 3.0);
    let start = time_of(&events(&emu.console), "present");
    let scale = ios_curve("BOX", "alert2-tap", 4);
    let dim = ios_curve("DIM", "alert2-tap", 3);
    let frames = frames.lock().unwrap();
    let mut checked = 0;
    for (t, w, px) in frames.iter() {
        let t = t - start;
        if !(0.0..0.6).contains(&t) {
            continue;
        }
        let pixel = |x: u32, y: u32| {
            let i = ((y * w + x) * 4) as usize;
            [px[i] as f64, px[i + 1] as f64, px[i + 2] as f64]
        };
        // The dimming: teal's green (195) darkened by 20% at full.
        let d = (1.0 - pixel(195, 100)[1] / 195.0) / 0.2;
        let want_d = at(&dim, t, 0.0);
        assert!((d - want_d).abs() < 0.08, "at {t:.3} s the dimming is {d:.2}, iOS {want_d:.2}");
        // The box: glass lifts the teal's red; where it starts in the middle row.
        let want_s = at(&scale, t, 1.2);
        if want_d > 0.3 {
            let left = (0..195).find(|&x| pixel(x, 428)[0] > 40.0).expect("the box in the middle row");
            let s = (390.0 - 2.0 * left as f64) / 320.0;
            assert!((s - want_s).abs() < 0.02, "at {t:.3} s the box is {s:.3}×, iOS {want_s:.3}×");
        }
        checked += 1;
    }
    assert!(checked > 12, "only {checked} frames while it appeared");
}

/// What accessibility sees (ios26-ax.txt): the box named by its title, its
/// texts, and each action a button at its capsule; the screen behind stays.
#[test]
fn alerts_for_accessibility() {
    use pomme_hle::driver::{drive, text};
    let ax = std::fs::read_to_string(format!("{}/tests/fixtures/alerts/ios26-ax.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for case in ["alert2", "sheet"] {
        let Some(mut emu) = common::boot("alerts") else { return };
        pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", case);
        let shown = drive(&mut emu, |app| {
            app.wait_for(&text("Delete the note?"))?;
            app.run_for(1.0)?;
            app.elements()
        })
        .unwrap();
        for line in ax.lines().filter_map(|l| l.strip_prefix(&format!("AX {case} "))) {
            let (kind, rest) = line.split_once(' ').unwrap();
            let (label, frame) = rest[1..].split_once("\" ").unwrap();
            let f: Vec<f64> = frame.split(' ').map(|v| v.parse().unwrap()).collect();
            let at = |e: &&pomme_hle::accessibility::Element| e.frame.iter().zip(&f).all(|(a, b)| (a - b).abs() <= 0.5);
            let found = match kind {
                "Application" => continue,
                // (A text's frame is the label's; its width follows the font only when centred.)
                "StaticText" => shown.iter().any(|e| e.label.as_deref() == Some(label) && (e.frame[1] - f[1]).abs() <= 0.5),
                "Button" => shown.iter().filter(at).any(|e| e.traits & 1 != 0 && e.label.as_deref() == Some(label)),
                _ => shown.iter().filter(at).any(|e| e.label.as_deref() == Some(label)),
            };
            assert!(found, "{case}: no {kind} {label:?} at {f:?}\n{}", pomme_hle::accessibility::describe(&shown));
            // A button is one element: its title isn't text of its own, as on iOS.
            if kind == "Button" {
                let texts = shown.iter().filter(|e| e.label.as_deref() == Some(label) && e.traits & 1 == 0).count();
                assert_eq!(texts, 0, "{case}: {label:?} is also text inside its button\n{}", pomme_hle::accessibility::describe(&shown));
            }
        }
    }
}

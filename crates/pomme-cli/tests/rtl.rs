//! Right-to-left layout (tests/fixtures/rtl/main.m; fixtures/rtlplain is the
//! same app without an Arabic localization): the layout direction, and the
//! frames Auto Layout and stack views give, held to what the iOS 26
//! Simulator reported (fixtures/<probe>/ios26-<language>.txt, recorded with
//! scripts/swiftui/metrics-simulator.sh). Built with the iOS SDK; skipped
//! without Xcode or the sysroot.

mod common;

use std::collections::BTreeMap;

/// `VALUE name text` lines, each name's last.
fn values(text: &str) -> BTreeMap<String, String> {
    let mut v = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("VALUE ") else { continue };
        let (name, value) = rest.split_once(' ').unwrap_or((rest, ""));
        v.insert(name.to_string(), value.to_string());
    }
    v
}

/// What pomme reports for `probe` on a phone in `language`, against the recording.
fn differences(probe: &str, variant: &str, language: &str, region: &str) -> Option<Vec<String>> {
    let mut emu = common::boot(probe)?;
    emu.locales.settings.languages = vec![language.into()];
    emu.locales.settings.region = region.into();
    common::views(&mut emu, 2.0);
    if std::env::var_os("RTL_DEBUG").is_some() {
        eprintln!("{probe} {variant}:\n{}", emu.console);
    }
    let recording = std::fs::read_to_string(format!("{}/tests/fixtures/{probe}/ios26-{variant}.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let (ours, ios) = (values(&emu.console), values(&recording));
    let (our_frames, ios_frames) = (common::parse_metrics(&emu.console), common::parse_metrics(&recording));
    let mut wrong = Vec::new();
    for (name, want) in &ios {
        if ours.get(name) != Some(want) {
            wrong.push(format!("{name}: {:?}, iOS {want:?}", ours.get(name)));
        }
    }
    for (name, want) in &ios_frames {
        match our_frames.get(name) {
            None => wrong.push(format!("{name}: no frame (iOS {want:?})")),
            Some(got) if got.iter().zip(want).any(|(g, w)| (g - w).abs() > 0.5) => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
            _ => {}
        }
    }
    Some(wrong)
}

fn check(probe: &str, variant: &str, language: &str, region: &str) {
    let Some(wrong) = differences(probe, variant, language, region) else { return };
    assert!(wrong.is_empty(), "{probe} {variant}: differences from iOS 26:\n  {}", wrong.join("\n  "));
}

#[test]
fn arabic_app_on_an_arabic_phone() {
    check("rtl", "ar", "ar", "SA");
}

#[test]
fn arabic_app_on_an_english_phone() {
    check("rtl", "en", "en-US", "US");
}

/// An app without an Arabic localization stays left to right.
#[test]
fn english_only_app_on_an_arabic_phone() {
    check("rtlplain", "ar", "ar", "SA");
}

// ---- Navigation (tests/fixtures/rtlnav: push at 0.5 s, pop at 2 s) ----

use pomme_hle::host::{Frame, Host, Idle};
use std::sync::{Arc, Mutex};

type Frames = Arc<Mutex<Vec<(u64, u32, Vec<u8>)>>>;

struct Recording(Frames);

impl Host for Recording {
    fn idle(&mut self, _: u64, _: Option<u64>) -> Idle {
        Idle::Advance
    }
    fn frame(&mut self, f: Frame<'_>) {
        self.0.lock().unwrap().push((f.at_ns, f.width, f.rgba.to_vec()));
    }
}

const S: u64 = 1_000_000_000;
const W: f64 = 390.0;

/// The rtlnav probe on an Arabic phone, with these swipes ((from, to, at ns, for ns)), for `secs`.
fn run_nav(swipes: &[((f64, f64), (f64, f64), u64, u64)], secs: f64) -> Option<Vec<(u64, u32, Vec<u8>)>> {
    let mut emu = common::boot("rtlnav")?;
    emu.locales.settings.languages = vec!["ar".into()];
    emu.locales.settings.region = "SA".into();
    let frames: Frames = Default::default();
    *emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    for &(from, to, at, duration) in swipes {
        pomme_hle::touch::swipe(&mut emu.touch.script, at, from, to, duration);
    }
    common::views(&mut emu, secs);
    let f = frames.lock().unwrap().clone();
    Some(f)
}

fn px(f: &(u64, u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
    let i = ((y * f.1 + x) * 4) as usize;
    [f.2[i], f.2[i + 1], f.2[i + 2]]
}

fn blue(c: [u8; 3]) -> bool {
    c[2] > 200 && c[0] < 60
}

/// The blue screen's right edge in row `y` (0: none).
fn right_edge(f: &(u64, u32, Vec<u8>), y: u32) -> f64 {
    (0..f.1).rev().find(|&x| blue(px(f, x, y))).map_or(0.0, |x| x as f64 + 1.0)
}

/// iOS 26's push and pop on the iPhone 16e (tests/transitions.rs, measured
/// from video): the moving screen's progress across the screen.
const PUSH: &[(f64, f64)] = &[(0.0, 0.0), (0.017, 0.066), (0.034, 0.219), (0.067, 0.489), (0.083, 0.597), (0.1, 0.685), (0.117, 0.757), (0.133, 0.813), (0.15, 0.858), (0.167, 0.892), (0.2, 0.939), (0.25, 0.975), (0.3, 0.99), (0.4, 0.998), (0.5, 1.0)];
const POP: &[(f64, f64)] = &[(0.0, 0.0), (0.017, 0.152), (0.033, 0.301), (0.05, 0.436), (0.067, 0.552), (0.083, 0.648), (0.1, 0.727), (0.117, 0.79), (0.133, 0.839), (0.15, 0.878), (0.167, 0.908), (0.2, 0.948), (0.25, 0.978), (0.3, 0.991), (0.4, 0.999), (0.5, 1.0)];

fn progress(curve: &[(f64, f64)], t: f64) -> f64 {
    if t <= 0.0 {
        return 0.0;
    }
    for w in curve.windows(2) {
        let ((t0, p0), (t1, p1)) = (w[0], w[1]);
        if t <= t1 {
            return p0 + (p1 - p0) * (t - t0) / (t1 - t0);
        }
    }
    1.0
}

/// In an Arabic app the pushed screen comes in from the left and a pop
/// takes it back out to the left. fixtures/rtlnav/ios26-ar.txt (sampled each
/// frame) shows iOS moves it as the mirror of the English push and pop
/// (from −390 to 0, settled in half a second; ios26-en.txt from 390), so
/// it's held to the measured English curves, mirrored.
#[test]
fn push_and_pop_from_the_left() {
    let Some(frames) = run_nav(&[], 2.8) else { return };
    let mut checked = 0;
    for (start, curve, push) in [(0.5, PUSH, true), (2.0, POP, false)] {
        for f in frames.iter().filter(|f| (0.0..0.35).contains(&(f.0 as f64 / 1e9 - start))) {
            let t = f.0 as f64 / 1e9 - start;
            let p = progress(curve, t);
            let want = if push { W * p } else { W * (1.0 - p) };
            let got = right_edge(f, 300);
            assert!((got - want).abs() <= 0.03 * W, "{} at {t:.3}: blue's right edge {got}, iOS {want:.0}", if push { "push" } else { "pop" });
            checked += 1;
        }
    }
    assert!(checked > 10, "only {checked} frames");
}

/// The glass back button is on the right (fixtures/rtlnav: a 44-pt circle at x 330).
#[test]
fn back_button_on_the_right() {
    let Some(frames) = run_nav(&[], 1.8) else { return };
    let f = frames.last().unwrap();
    let light = |c: [u8; 3]| c.iter().all(|&v| v > 200);
    assert!(light(px(f, 352, 69)), "no back button at the right: {:?}", px(f, 352, 69));
    assert!(blue(px(f, 38, 69)), "something at the left: {:?}", px(f, 38, 69));
}

/// The back swipe starts at the right edge (iOS's gesture: `edges` 8), not the left.
#[test]
fn back_swipe_from_the_right_edge() {
    let Some(frames) = run_nav(&[((386.0, 400.0), (90.0, 400.0), 11 * S / 10, S / 4)], 1.95) else { return };
    assert!(!blue(px(frames.last().unwrap(), 195, 400)), "the swipe from the right didn't pop");
    let Some(frames) = run_nav(&[((4.0, 400.0), (300.0, 400.0), 11 * S / 10, S / 4)], 1.95) else { return };
    assert!(blue(px(frames.last().unwrap(), 195, 400)), "a swipe from the left popped");
}

// ---- Controls (tests/fixtures/rtlcontrols) ----

/// The frames of the views under the first line of `views` containing
/// `header` (more indented, up to the next line that isn't), with their text.
fn children(views: &str, header: &str) -> Vec<([f64; 4], String)> {
    let mut lines = views.lines().skip_while(|l| !l.contains(header));
    let Some(head) = lines.next() else { return Vec::new() };
    let indent = |l: &str| l.len() - l.trim_start().len();
    let base = indent(head);
    lines
        .take_while(|l| indent(l) > base)
        .filter_map(|l| {
            let (_, rest) = l.split_once(" (")?;
            let (frame, tail) = rest.split_once(')')?;
            let nums: Vec<f64> = frame.split([',', '×']).filter_map(|v| v.trim().parse().ok()).collect();
            (nums.len() == 4).then(|| ([nums[0], nums[1], nums[2], nums[3]], tail.to_string()))
        })
        .collect()
}

/// iOS's subview frames for a control (`VIEW control class x y w h text`).
fn ios_views(control: &str) -> Vec<(String, [f64; 4], String)> {
    let text = std::fs::read_to_string(format!("{}/tests/fixtures/rtlcontrols/ios26-ar.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    text.lines()
        .filter_map(|l| {
            let mut f = l.split(' ');
            (f.next()? == "VIEW" && f.next()? == control).then_some(())?;
            let class = f.next()?.to_string();
            let v: Vec<f64> = f.by_ref().take(4).filter_map(|x| x.parse().ok()).collect();
            Some((class, [v[0], v[1], v[2], v[3]], f.collect::<Vec<_>>().join(" ")))
        })
        .collect()
}

/// A switch's thumb, a slider's fill and thumb and a segmented control's
/// segments are mirrored in their control, as on iOS (the recording's frames).
#[test]
fn controls_mirror() {
    let Some(mut emu) = common::boot("rtlcontrols") else { return };
    emu.locales.settings.languages = vec!["ar".into()];
    emu.locales.settings.region = "SA".into();
    let views = common::views(&mut emu, 2.5);
    let close = |a: f64, b: f64| (a - b).abs() <= 1.0;
    let lens_x = |c: &str| ios_views(c).iter().find(|v| v.0 == "_UILiquidLensView").map(|v| v.1[0]).unwrap();
    let mut wrong = Vec::new();
    // Thumbs: the 37 × 24 capsule.
    for (header, control) in [("UISwitch (40.0, 100.0", "switchOff"), ("UISwitch (40.0, 150.0", "switchOn"), ("UISlider (", "slider")] {
        let thumb = children(&views, header).into_iter().find(|c| close(c.0[2], 37.0) && close(c.0[3], 24.0)).map(|c| c.0[0]);
        if !thumb.is_some_and(|x| close(x, lens_x(control))) {
            wrong.push(format!("{control} thumb at {thumb:?}, iOS {}", lens_x(control)));
        }
    }
    // The slider's fill: from its right end.
    let fill = ios_views("slider").into_iter().find(|v| v.0 == "UIView" && close(v.1[3], 6.0) && v.1[0] > 0.0).unwrap().1;
    if !children(&views, "UISlider (").iter().any(|c| c.1.contains("#0088ff") && close(c.0[0], fill[0]) && close(c.0[2], fill[2])) {
        wrong.push(format!("slider fill not at {fill:?}"));
    }
    // The selection and the first segment's title on the right.
    let seg = children(&views, "UISegmentedControl (");
    let sel = lens_x("segmented");
    if !seg.iter().any(|c| close(c.0[0], sel) && close(c.0[2], 96.0)) {
        wrong.push(format!("segment selection not at {sel}"));
    }
    let one = ios_views("segmented").into_iter().find(|v| v.2 == "One").unwrap().1;
    let ours = seg.iter().find(|c| c.1.contains("\"One\"")).map(|c| c.0);
    if !ours.is_some_and(|r| r[0] <= one[0] && r[0] + r[2] >= one[0] + one[2]) {
        wrong.push(format!("\"One\" at {ours:?}, iOS's label at {one:?}"));
    }
    assert!(wrong.is_empty(), "differences from iOS 26:\n  {}\n{views}", wrong.join("\n  "));
}

/// A tap on a segment selects the segment shown there: the leftmost is the
/// last one in right to left.
#[test]
fn tapping_the_leftmost_segment_selects_the_last() {
    let Some(mut emu) = common::boot("rtlcontrols") else { return };
    emu.locales.settings.languages = vec!["ar".into()];
    emu.locales.settings.region = "SA".into();
    pomme_hle::touch::swipe(&mut emu.touch.script, 2 * S, (90.0, 266.0), (90.0, 266.0), S / 20);
    let views = common::views(&mut emu, 2.8);
    // The selection moved to the leftmost segment ("Three").
    assert!(children(&views, "UISegmentedControl (").iter().any(|c| (c.0[0] - 2.0).abs() < 1.0 && (c.0[2] - 96.0).abs() < 1.0), "{views}");
}

/// Images: one flipped for right to left is drawn mirrored, a plain one
/// isn't; the symbol `chevron.forward` points left, `chevron.right` still
/// right; `flipsForRightToLeftLayoutDirection` as iOS answers it.
#[test]
fn images_that_flip() {
    let Some(mut emu) = common::boot("rtlcontrols") else { return };
    emu.locales.settings.languages = vec!["ar".into()];
    emu.locales.settings.region = "SA".into();
    let frames: Frames = Default::default();
    *emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    common::views(&mut emu, 2.5);
    let recording = std::fs::read_to_string(format!("{}/tests/fixtures/rtlcontrols/ios26-ar.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    for (name, want) in values(&recording).into_iter().filter(|(n, _)| n.ends_with(".flips")) {
        assert_eq!(values(&emu.console).get(&name), Some(&want), "{name}");
    }
    let frames = frames.lock().unwrap();
    let f = frames.last().unwrap();
    let red = |c: [u8; 3]| c[0] > 200 && c[2] < 60;
    // Red on the left as drawn; flipped, blue on the left.
    assert!(red(px(f, 50, 580)), "plain: {:?}", px(f, 50, 580));
    assert!(blue(px(f, 130, 580)), "flipped: {:?}", px(f, 130, 580));
    // A chevron's ink across its middle row: left of its centre when it points left.
    let ink = |x0: u32| {
        let xs: Vec<f64> = (x0..x0 + 40).filter(|&x| px(f, x, 580).iter().all(|&v| v < 100)).map(|x| x as f64).collect();
        xs.iter().sum::<f64>() / xs.len().max(1) as f64 - (x0 + 20) as f64
    };
    assert!(ink(200) < -2.0, "chevron.forward should point left: ink {:.1} from its centre", ink(200));
    assert!(ink(280) > 2.0, "chevron.right should point right: ink {:.1} from its centre", ink(280));
}

/// A table cell in right to left: the image on the right, the text placed
/// as iOS places it, the disclosure chevron on the left and the separator
/// in from the right (fixtures/rtlcontrols: x and widths; the Arabic row is
/// 2 pt taller on iOS, from the Arabic font's metrics).
#[test]
fn table_cell_mirrors() {
    let Some(mut emu) = common::boot("rtlcontrols") else { return };
    emu.locales.settings.languages = vec!["ar".into()];
    emu.locales.settings.region = "SA".into();
    common::views(&mut emu, 2.5);
    let ours: Vec<(String, [f64; 4])> = emu
        .console
        .lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split(' ').collect();
            (f.len() >= 7 && f[0] == "VIEW" && f[1] == "table").then(|| (f[2].to_string(), [f[3], f[4], f[5], f[6]].map(|v| v.parse::<f64>().unwrap())))
        })
        .collect();
    let has = |x: f64, w: f64, h: Option<f64>| ours.iter().any(|(_, r)| (r[0] - x).abs() <= 0.5 && (r[2] - w).abs() <= 3.0 && h.is_none_or(|h| (r[3] - h).abs() <= 0.5));
    let mut wrong = Vec::new();
    for (class, r, _) in ios_views("table") {
        let what = match class.as_str() {
            "UIImageView" => Some(r[3]),
            "UITableViewLabel" => None,
            "_UITableViewCellSeparatorView" if r[2] < 390.0 => Some(1.0),
            _ => continue,
        };
        if !has(r[0], r[2], what) {
            wrong.push(format!("{class} at x {} width {}", r[0], r[2]));
        }
    }
    assert!(wrong.is_empty(), "not as iOS has them:\n  {}\npomme's: {ours:?}", wrong.join("\n  "));
}

// ---- SwiftUI (tests/fixtures/swiftuirtl) ----

/// SwiftUI mirrors in an Arabic app as iOS does (fixtures/swiftuirtl/ios26-ar.png):
/// the HStack runs from the right, padding from the leading edge is on the
/// right, and the environment says right to left. Each coloured shape's
/// first and last columns match the Simulator's within 3 pt.
#[test]
fn swiftui_mirrors() {
    let Some(mut emu) = common::boot("swiftuirtl") else { return };
    emu.locales.settings.languages = vec!["ar".into()];
    emu.locales.settings.region = "SA".into();
    let frames: Frames = Default::default();
    *emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    common::views(&mut emu, 2.0);
    assert!(emu.console.contains("VALUE layoutDirection rightToLeft"), "{}", emu.console);
    let frames = frames.lock().unwrap();
    let f = frames.last().unwrap();
    let ios = tiny_skia::Pixmap::decode_png(&std::fs::read(format!("{}/tests/fixtures/swiftuirtl/ios26-ar.png", env!("CARGO_MANIFEST_DIR"))).unwrap()).unwrap();
    let ios_px = |x: u32, y: u32| {
        let c = ios.pixel(x * 3 + 1, y * 3 + 1).unwrap().demultiply();
        [c.red(), c.green(), c.blue()]
    };
    let red = |c: [u8; 3]| c[0] > 200 && c[1] < 90 && c[2] < 90;
    let purple = |c: [u8; 3]| c[0] > 150 && c[1] < 100 && c[2] > 180;
    let first_span = |at: &dyn Fn(u32, u32) -> [u8; 3], is: &dyn Fn([u8; 3]) -> bool| {
        (60..400).find_map(|y| {
            let xs: Vec<u32> = (0..390).filter(|&x| is(at(x, y))).collect();
            Some((*xs.first()?, *xs.last()?))
        })
    };
    for (name, is) in [("red", &red as &dyn Fn([u8; 3]) -> bool), ("purple", &purple)] {
        let ours = first_span(&|x, y| px(f, x, y), is);
        let theirs = first_span(&ios_px, is);
        let (Some(a), Some(b)) = (ours, theirs) else { panic!("{name}: pomme {ours:?}, iOS {theirs:?}") };
        assert!((a.0 as i32 - b.0 as i32).abs() <= 3 && (a.1 as i32 - b.1 as i32).abs() <= 3, "{name}: pomme x {a:?}, iOS {b:?}");
    }
}

// ---- Text fields (tests/fixtures/rtlfield) ----

/// Text fields with mixed Arabic and English, against the Simulator: the
/// text rect, where the line starts (natural alignment by the text's own
/// direction, even in an English app), and the carets' order across the line
/// at every position, with the positions that share a place sharing it
/// (widths aside: Noto isn't SF Arabic).
fn fields_match(variant: &str, language: &str) {
    let Some(mut emu) = common::boot("rtlfield") else { return };
    emu.locales.settings.languages = vec![language.into()];
    common::views(&mut emu, 2.0);
    let recording = std::fs::read_to_string(format!("{}/tests/fixtures/rtlfield/ios26-{variant}.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let (ours, ios) = (common::parse_metrics(&emu.console), common::parse_metrics(&recording));
    let mut wrong = Vec::new();
    for field in 1..=3 {
        let rect = format!("field{field}.textRect");
        if ours.get(&rect).zip(ios.get(&rect)).is_none_or(|(a, b)| a.iter().zip(b).any(|(x, y)| (x - y).abs() > 0.5)) {
            wrong.push(format!("{rect}: {:?}, iOS {:?}", ours.get(&rect), ios.get(&rect)));
        }
        let xs = |m: &std::collections::BTreeMap<String, [f64; 4]>| -> Vec<f64> { (0..).map_while(|k| m.get(&format!("field{field}.caret{k}")).map(|r| r[0])).collect() };
        let (a, b) = (xs(&ours), xs(&ios));
        if a.len() != b.len() || a.is_empty() {
            wrong.push(format!("field{field}: {} carets, iOS {}", a.len(), b.len()));
            continue;
        }
        if (a[0] - b[0]).abs() > 1.5 {
            wrong.push(format!("field{field}: the line starts at {:.2}, iOS {:.2}", a[0], b[0]));
        }
        // Order and ties: each pair the same way round (or level) as iOS has it.
        for i in 0..a.len() {
            for j in 0..a.len() {
                let (da, db) = (a[i] - a[j], b[i] - b[j]);
                let side = |d: f64| if d.abs() < 0.5 { 0 } else { d.signum() as i32 };
                if side(da) != side(db) {
                    wrong.push(format!("field{field}: carets {i} and {j}: {:.2} vs {:.2}, iOS {:.2} vs {:.2}", a[i], a[j], b[i], b[j]));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{variant}:\n  {}", wrong.join("\n  "));
}

#[test]
fn text_fields_in_an_arabic_app() {
    fields_match("ar", "ar");
}

#[test]
fn text_fields_in_an_english_app() {
    fields_match("en", "en-US");
}

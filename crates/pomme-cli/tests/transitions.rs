//! UIKit's transitions match iOS's (tests/fixtures/transitions/main.m): frames
//! from a live host are compared with what the iOS 26 Simulator showed for
//! the same transitions (scripts/measure/transitions), on the iPhone 16e
//! (390 × 844 pt). Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

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

/// Progress of a transition at `t` s after it starts, as measured in the
/// Simulator (linear between the measured frames).
fn ios(curve: &[(f64, f64)], t: f64) -> f64 {
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

/// iOS 26 push, iPhone 16e: the incoming view's progress across the screen.
const PUSH: &[(f64, f64)] = &[(0.0, 0.0), (0.017, 0.066), (0.034, 0.219), (0.067, 0.489), (0.083, 0.597), (0.1, 0.685), (0.117, 0.757), (0.133, 0.813), (0.15, 0.858), (0.167, 0.892), (0.2, 0.939), (0.25, 0.975), (0.3, 0.99), (0.4, 0.998), (0.5, 1.0)];
/// iOS 26 pop.
const POP: &[(f64, f64)] = &[(0.0, 0.0), (0.017, 0.152), (0.033, 0.301), (0.05, 0.436), (0.067, 0.552), (0.083, 0.648), (0.1, 0.727), (0.117, 0.79), (0.133, 0.839), (0.15, 0.878), (0.167, 0.908), (0.2, 0.948), (0.25, 0.978), (0.3, 0.991), (0.4, 0.999), (0.5, 1.0)];
/// iOS 26 sheet and full-screen presentation: how far up it has come.
const PRESENT: &[(f64, f64)] = &[(0.0, 0.0), (0.017, 0.073), (0.034, 0.172), (0.051, 0.283), (0.067, 0.393), (0.084, 0.495), (0.101, 0.585), (0.117, 0.663), (0.134, 0.728), (0.151, 0.783), (0.167, 0.827), (0.184, 0.864), (0.201, 0.892), (0.217, 0.915), (0.3, 0.97), (0.4, 0.993), (0.5, 1.0)];
/// iOS 26 dismissal.
const DISMISS: &[(f64, f64)] = &[(0.0, 0.0), (0.017, 0.023), (0.033, 0.101), (0.05, 0.205), (0.067, 0.316), (0.083, 0.424), (0.1, 0.522), (0.117, 0.609), (0.133, 0.683), (0.15, 0.744), (0.167, 0.796), (0.183, 0.838), (0.2, 0.872), (0.217, 0.899), (0.3, 0.965), (0.4, 0.992), (0.5, 1.0)];

const W: f64 = 390.0;
const H: f64 = 844.0;
const SAFE_TOP: f64 = 47.0;

fn pixel(f: &(u64, u32, Vec<u8>), x: u32, y: u32) -> [u8; 4] {
    let i = ((y * f.1 + x) * 4) as usize;
    [f.2[i], f.2[i + 1], f.2[i + 2], f.2[i + 3]]
}

fn is(c: [u8; 4], rgb: [u8; 3]) -> bool {
    (0..3).all(|i| (c[i] as i32 - rgb[i] as i32).abs() < 40)
}

/// The first x in row `y` where colour `rgb` shows.
fn left_edge(f: &(u64, u32, Vec<u8>), y: u32, rgb: [u8; 3]) -> Option<u32> {
    (0..f.1).find(|&x| is(pixel(f, x, y), rgb))
}

/// The first x in row `y` at least half covered by green (as the Simulator's corner was measured).
fn half_green(f: &(u64, u32, Vec<u8>), y: u32) -> Option<u32> {
    (0..f.1).find(|&x| {
        let c = pixel(f, x, y);
        c[1] > 128 && c[0] < 128
    })
}

/// The first y in column `x` where colour `rgb` shows.
fn top_edge(f: &(u64, u32, Vec<u8>), x: u32, rgb: [u8; 3]) -> Option<u32> {
    (0..(f.2.len() as u32 / 4 / f.1)).find(|&y| is(pixel(f, x, y), rgb))
}

#[test]
fn transitions_match_ios() {
    let Some(mut emu) = common::boot("transitions") else { return };
    let frames: Frames = Default::default();
    emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    let v = common::views(&mut emu, 9.0);
    let log = v.lines().find(|l| l.contains(" tag 1 ")).map(|l| l.trim().to_string()).unwrap_or_default();
    assert!(log.ends_with("\"presented dismissed full back \""), "{log}");

    let frames = frames.lock().unwrap();
    let during = |start: f64| frames.iter().filter(move |f| {
        let t = f.0 as f64 / 1e9 - start;
        t > 0.0 && t < 0.35
    });
    let (red, blue, green, yellow) = ([255, 0, 0], [0, 0, 255], [0, 255, 0], [255, 255, 0]);
    let mut checked = 0;
    // Push: blue comes in from the right, red moves 30% left, dimmed.
    for f in during(0.5) {
        let p = ios(PUSH, f.0 as f64 / 1e9 - 0.5);
        let x = left_edge(f, 300, blue).unwrap_or(f.1) as f64;
        assert!((x - W * (1.0 - p)).abs() <= 0.03 * W, "push at {:.3}: blue at {x}, iOS {:.0}", f.0 as f64 / 1e9, W * (1.0 - p));
        // Red, now under 10% black dimming as it goes.
        let r = pixel(f, 2, 300);
        if x > 10.0 && p > 0.5 {
            assert!(r[0] < 250 && r[0] > 210, "push dims red to {r:?}");
        }
        checked += 1;
    }
    // Pop: blue leaves to the right, red comes back from 30% left.
    for f in during(2.0) {
        let p = ios(POP, f.0 as f64 / 1e9 - 2.0);
        let x = left_edge(f, 300, blue).unwrap_or(f.1) as f64;
        assert!((x - W * p).abs() <= 0.03 * W, "pop at {:.3}: blue at {x}, iOS {:.0}", f.0 as f64 / 1e9, W * p);
        checked += 1;
    }
    // A page sheet comes up to the safe area's top, over red dimmed 20%.
    for f in during(3.5) {
        let p = ios(PRESENT, f.0 as f64 / 1e9 - 3.5);
        let y = top_edge(f, 195, green).unwrap_or(H as u32) as f64;
        let expect = H - (H - SAFE_TOP) * p;
        assert!((y - expect).abs() <= 0.03 * H, "present at {:.3}: sheet top {y}, iOS {expect:.0}", f.0 as f64 / 1e9);
        checked += 1;
    }
    // Settled, nothing more is drawn until the dismissal: the last frame before it.
    let last_before = |t: f64| frames.iter().filter(|f| (f.0 as f64 / 1e9) < t).last().expect("a frame");
    let up = last_before(5.0);
    assert_eq!(top_edge(up, 195, green), Some(SAFE_TOP as u32));
    assert!(is(pixel(up, 20, 30), [204, 0, 0]), "red behind dimmed 20%: {:?}", pixel(up, 20, 30));
    // Its top corners: continuous, radius 37.6. Where the sheet starts in each
    // row, against the iOS 26 Simulator's (depth below the top, inset), pt.
    const CORNER: &[(f64, f64)] = &[(0.33, 38.0), (0.67, 34.0), (1.0, 31.33), (1.67, 28.0), (2.67, 24.33), (4.0, 21.0), (6.67, 16.33), (10.0, 12.0), (15.0, 7.67), (20.0, 4.67), (26.67, 2.0), (33.33, 1.0), (40.0, 0.33)];
    for row in 1..36u32 {
        // The row's centre, below the sheet's top.
        let depth = row as f64 + 0.5;
        let ios_inset = ios(&CORNER.iter().map(|&(d, x)| (d, 1.0 - x / 38.0)).collect::<Vec<_>>(), depth);
        let ios_inset = 38.0 * (1.0 - ios_inset);
        let ours = half_green(up, SAFE_TOP as u32 + row).unwrap() as f64;
        assert!((ours - ios_inset).abs() <= 1.5, "corner at {depth} pt down: {ours}, iOS {ios_inset:.1}");
    }
    for f in during(5.0) {
        let p = ios(DISMISS, f.0 as f64 / 1e9 - 5.0);
        let y = top_edge(f, 195, green).unwrap_or(H as u32) as f64;
        let expect = SAFE_TOP + (H - SAFE_TOP) * p;
        assert!((y - expect).abs() <= 0.03 * H, "dismiss at {:.3}: sheet top {y}, iOS {expect:.0}", f.0 as f64 / 1e9);
        checked += 1;
    }
    // Full screen: all the way up, no dimming, no corners.
    for f in during(6.5) {
        let p = ios(PRESENT, f.0 as f64 / 1e9 - 6.5);
        let y = top_edge(f, 195, yellow).unwrap_or(H as u32) as f64;
        assert!((y - H * (1.0 - p)).abs() <= 0.03 * H, "full screen at {:.3}: top {y}, iOS {:.0}", f.0 as f64 / 1e9, H * (1.0 - p));
        checked += 1;
    }
    let full = last_before(8.0);
    assert!(is(pixel(full, 1, 1), yellow));
    let back = frames.last().unwrap();
    assert!(is(pixel(back, 195, 300), red));
    assert!(checked > 40, "{checked} frames checked");
}

//! Core Animation (tests/fixtures/animation/main.m), seen through a live
//! host as the desktop window sees it: frames keep coming while things
//! animate, each showing the animation at its moment; UIView animations move
//! from where they were to the new value; a CABasicAnimation fades a layer and
//! leaves the model alone; completions and delegates run when they end.
//! Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::host::{Frame, Host, Idle};
use std::sync::{Arc, Mutex};

type Frames = Arc<Mutex<Vec<(u64, u32, Vec<u8>)>>>;

/// A host that never has input and keeps every frame with its time.
struct Recording(Frames);

impl Host for Recording {
    fn idle(&mut self, _: u64, _: Option<u64>) -> Idle {
        Idle::Advance
    }
    fn frame(&mut self, f: Frame<'_>) {
        self.0.lock().unwrap().push((f.at_ns, f.width, f.rgba.to_vec()));
    }
}

/// The first and last x (pt) of pixels in row `y` matching `is`, at scale 1.
fn span(width: u32, rgba: &[u8], y: u32, is: impl Fn(&[u8]) -> bool) -> Option<(u32, u32)> {
    let row = &rgba[(y * width * 4) as usize..((y + 1) * width * 4) as usize];
    let xs: Vec<u32> = row.chunks(4).enumerate().filter(|(_, p)| is(p)).map(|(x, _)| x as u32).collect();
    Some((*xs.first()?, *xs.last()?))
}

#[test]
fn views_and_layers_animate() {
    let Some(mut emu) = common::boot("animation") else { return };
    let frames: Frames = Default::default();
    *emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    let v = common::views(&mut emu, 2.5);
    let label = |tag: &str| v.lines().find(|l| l.contains(&format!(" tag {tag} "))).map(|l| l.trim().to_string()).unwrap_or_default();
    assert!(label("1").ends_with("\"done\""), "{v}");
    assert!(label("3").ends_with("\"stopped 1\""), "{v}");

    let frames = frames.lock().unwrap();
    let blue = |p: &[u8]| p[2] > 200 && p[0] < 60 && p[1] < 60;
    let at = |t: f64| frames.iter().min_by_key(|f| (f.0 as i64 - (t * 1e9) as i64).abs()).unwrap();
    // Frames while it moves: at 30 a second over the 1 s move, about 30.
    let moving = frames.iter().filter(|f| f.0 > 500_000_000 && f.0 < 1_500_000_000).count();
    assert!(moving >= 20, "{moving} frames while animating");
    // Linear: at 1.0 s (halfway) the square's left edge is 100 pt along; after 1.5 s, 200.
    for (t, x) in [(0.4, 0.0), (1.0, 100.0), (2.0, 200.0)] {
        let (ns, width, rgba) = at(t);
        let (left, _) = span(*width, rgba, 225, blue).unwrap_or_else(|| panic!("no square at {ns}"));
        assert!((left as f64 - x).abs() <= 8.0, "at {:.3} s the square is at {left}, expected about {x}", *ns as f64 / 1e9);
    }
    // The red layer fades: about half way through at 1.0 s, gone from the screen after.
    let red_alpha = |rgba: &[u8], width: u32| {
        let i = ((325 * width + 25) * 4) as usize;
        255 - rgba[i + 1] // on white: green falls as red shows
    };
    let (_, w, mid) = at(1.0);
    let half = red_alpha(mid, *w);
    assert!((90..=170).contains(&half), "red at 1.0 s: {half}");
    // Removed at the end: back to the model (opacity 1), fully red.
    let (_, w, end) = at(2.2);
    assert!(red_alpha(end, *w) > 240);

    // The spinner (37 pt, spokes centred 17.5 pt from its top-left, at
    // (181.5, 581.5) + 17.5): its darkest spoke goes round clockwise, one
    // spoke every 0.1 s, a turn every 0.8 s, as iOS's does.
    let darkest = |rgba: &[u8], width: u32| {
        (0..8)
            .min_by_key(|&k| {
                let a = (k as f64) * std::f64::consts::FRAC_PI_4;
                let (x, y) = (181.5 + 17.5 + 12.0 * a.sin(), 581.5 + 17.5 - 12.0 * a.cos());
                let i = ((y as u32 * width + x as u32) * 4) as usize;
                rgba[i] as u32 + rgba[i + 1] as u32 + rgba[i + 2] as u32
            })
            .unwrap()
    };
    let (t0, w0, f0) = at(1.0);
    let k0 = darkest(f0, *w0);
    for step in 1..=4 {
        let (t, w, f) = at(1.0 + 0.1 * step as f64);
        // Frames fall anywhere within a spoke's 0.1 s, so dt seconds apart the
        // head has moved ⌊dt / 0.1⌋ or ⌈dt / 0.1⌉ spokes, clockwise; a frame
        // landing exactly on a spoke change may show either side of it.
        let spokes = (*t as f64 - *t0 as f64) / 1e9 / 0.1;
        let allowed: Vec<usize> = ((spokes - 1.0 - 1e-3).ceil() as usize..=(spokes + 1.0 + 1e-3).floor() as usize)
            .filter(|&n| (n as f64 - spokes).abs() < 1.0 + 1e-3)
            .map(|n| (k0 + n) % 8)
            .collect();
        let k = darkest(f, *w);
        assert!(allowed.contains(&k), "{spokes:.2} spokes on, the head is at {k}, expected one of {allowed:?}");
    }
}


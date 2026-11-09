//! The back swipe (tests/fixtures/swipeback/main.m): from the left edge the
//! pushed screen follows the finger; let go past halfway (or flicked) it
//! pops, short and slow it springs back and stays; a swipe that doesn't start
//! at the edge leaves the screen alone. Built with the iOS SDK; skipped
//! without Xcode or the sysroot.

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

const S: u64 = 1_000_000_000;

fn run(swipes: &[((f64, f64), (f64, f64), u64, u64)], secs: f64) -> Option<(String, Vec<(u64, u32, Vec<u8>)>)> {
    let mut emu = common::boot("swipeback")?;
    let frames: Frames = Default::default();
    emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    for &(from, to, at, duration) in swipes {
        pomme_hle::touch::swipe(&mut emu.touch.script, at, from, to, duration);
    }
    let v = common::views(&mut emu, secs);
    let label = v.lines().find(|l| l.contains(" tag 1 ")).map(|l| l.trim().to_string()).unwrap_or_default();
    let f = frames.lock().unwrap().clone();
    Some((label, f))
}

fn px(f: &(u64, u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
    let i = ((y * f.1 + x) * 4) as usize;
    [f.2[i], f.2[i + 1], f.2[i + 2]]
}

fn blue(c: [u8; 3]) -> bool {
    c[2] > 200 && c[0] < 60
}

#[test]
fn a_full_swipe_pops() {
    // From the edge across to 300 pt in 0.25 s.
    let Some((label, frames)) = run(&[((4.0, 400.0), (300.0, 400.0), 2 * S, S / 4)], 3.5) else { return };
    // Mid-swipe the blue screen's left edge is under the finger.
    let mid = frames.iter().min_by_key(|f| (f.0 as i64 - (2 * S + S / 8) as i64).abs()).unwrap();
    let edge = (0..390).find(|&x| blue(px(mid, x, 400))).unwrap_or(390);
    let t = (mid.0 - 2 * S) as f64 / (S / 4) as f64;
    let finger = 4.0 + 296.0 * t.min(1.0);
    // (Frames and the scripted moves land at different moments: within one move, ~19 pt here.)
    assert!((edge as f64 - (finger - 4.0)).abs() <= 20.0, "edge {edge}, finger {finger:.0}");
    // Popped: red, and the root appeared again.
    let end = frames.last().unwrap();
    assert!(!blue(px(end, 195, 400)), "popped back to red: {:?}", px(end, 195, 400));
    assert!(label.ends_with("\"appeared 2\""), "{label}");
}

#[test]
fn a_short_slow_swipe_springs_back() {
    // 80 pt over half a second, then let go.
    let Some((label, frames)) = run(&[((4.0, 400.0), (84.0, 400.0), 2 * S, S / 2)], 3.5) else { return };
    let end = frames.last().unwrap();
    assert!(blue(px(end, 195, 400)), "still on the pushed screen: {:?}", px(end, 195, 400));
    assert!(blue(px(end, 2, 400)), "back in place");
    // The root screen went back off screen (its label isn't in the window).
    assert!(label.is_empty(), "{label}");
}

#[test]
fn a_swipe_away_from_the_edge_does_nothing() {
    let Some((label, frames)) = run(&[((100.0, 400.0), (380.0, 400.0), 2 * S, S / 4)], 3.5) else { return };
    let end = frames.last().unwrap();
    assert!(blue(px(end, 195, 400)), "{:?}", px(end, 195, 400));
    assert!(label.is_empty(), "{label}");
}

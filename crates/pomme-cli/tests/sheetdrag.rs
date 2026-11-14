//! Dragging sheets (tests/fixtures/sheetdrag/main.m), on the iPhone 16e:
//! dragged down past halfway (or flicked) a sheet is dismissed and its
//! delegate hears will/did; dragged up it moves to the next detent; a short
//! slow drag springs back; a sheet that's modal in presentation refuses and
//! its delegate hears the attempt. Built with the iOS SDK; skipped without
//! Xcode or the sysroot.

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

fn px(f: &(u64, u32, Vec<u8>), x: u32, y: u32) -> [u8; 3] {
    let i = ((y * f.1 + x) * 4) as usize;
    [f.2[i], f.2[i + 1], f.2[i + 2]]
}

/// The first y in column 195 where `is` holds.
fn top(f: &(u64, u32, Vec<u8>), is: impl Fn([u8; 3]) -> bool) -> Option<u32> {
    (0..844).find(|&y| is(px(f, 195, y)))
}

#[test]
fn sheets_follow_drags() {
    let Some(mut emu) = common::boot("sheetdrag") else { return };
    let frames: Frames = Default::default();
    *emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    let script = &mut emu.touch.script;
    // The green (large) sheet: flicked down from 300 to 700 in 0.15 s.
    pomme_hle::touch::swipe(script, 2 * S, (195.0, 300.0), (195.0, 700.0), S * 15 / 100);
    // The blue (medium) sheet: dragged up from 600 to 200 in 0.3 s; then 40 pt down, slowly.
    pomme_hle::touch::swipe(script, 4 * S, (195.0, 600.0), (195.0, 200.0), S * 3 / 10);
    pomme_hle::touch::swipe(script, 4 * S + S * 9 / 10, (195.0, 300.0), (195.0, 340.0), S / 2);
    // The yellow one (modal in presentation, up at 6.5 s): flicked down, it stays.
    pomme_hle::touch::swipe(script, 7 * S + S / 2, (195.0, 300.0), (195.0, 700.0), S * 15 / 100);
    let v = common::views(&mut emu, 9.0);
    let log = v.lines().find(|l| l.contains(" tag 1 ")).map(|l| l.trim().to_string()).unwrap_or_default();
    assert!(log.ends_with("\"will did large attempt \""), "{log}");

    let frames = frames.lock().unwrap();
    let at = |t: u64| frames.iter().filter(|f| f.0 < t).last().unwrap();
    let green = |c: [u8; 3]| c[1] > 200 && c[0] < 60 && c[2] < 60;
    let blue = |c: [u8; 3]| c[2] > 200 && c[0] < 60;
    let yellow = |c: [u8; 3]| c[0] > 200 && c[1] > 200 && c[2] < 60;
    // Mid-drag the green sheet follows the finger (1:1 down).
    let mid = frames.iter().min_by_key(|f| (f.0 as i64 - (2 * S + S * 7 / 100) as i64).abs()).unwrap();
    let t = (mid.0 as f64 - 2e9) / 0.15e9;
    let finger_dy = 400.0 * t.clamp(0.0, 1.0);
    let y = top(mid, green).unwrap_or(844) as f64;
    assert!((y - (47.0 + finger_dy)).abs() <= 30.0, "green sheet top {y}, finger moved {finger_dy:.0}");
    // Dismissed: red again.
    assert!(top(at(3 * S), green).is_none());
    // The blue sheet went from medium (top 393.6) to large (top 47).
    assert!((top(at(4 * S), blue).unwrap() as f64 - 393.6).abs() <= 2.0);
    assert_eq!(top(at(4 * S + S * 9 / 10), blue), Some(47));
    // The short slow drag sprang back (the fixture dismisses it at 6 s).
    assert_eq!(top(at(6 * S), blue), Some(47));
    // The modal one stayed.
    assert_eq!(top(frames.last().unwrap(), yellow), Some(47));
}

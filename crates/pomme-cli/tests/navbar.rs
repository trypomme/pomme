//! The navigation bar through a push and pop, as iOS 26 shows it
//! (tests/fixtures/navbar/main.m; scripts/measure/transitions snapshotted the
//! same scene in the Simulator): each screen's title rides with its screen,
//! the lower one's under the upper card, the screens are cards with the
//! display's corners while they move, and the glass back button fades in.
//! Built with the iOS SDK; skipped without Xcode or the sysroot.

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

#[test]
fn bar_follows_the_screens() {
    let Some(mut emu) = common::boot("navbar") else { return };
    emu.device = pomme_hle::device::find("iphone-17-pro").unwrap();
    let frames: Frames = Default::default();
    *emu.live.host = Some(Box::new(Recording(frames.clone())));
    emu.live.scale = 1.0;
    common::views(&mut emu, 3.0);
    let frames = frames.lock().unwrap();
    let px = |f: &(u64, u32, Vec<u8>), x: u32, y: u32| {
        let i = ((y * f.1 + x) * 4) as usize;
        [f.2[i], f.2[i + 1], f.2[i + 2]]
    };
    let dark = |c: [u8; 3]| c.iter().all(|&v| v < 80);
    let blue = |c: [u8; 3]| c[2] > 200 && c[0] < 60;
    let red = |c: [u8; 3]| c[0] > 180 && c[2] < 60;
    // Mid-push: the frame nearest 0.1 s in.
    let f = frames.iter().min_by_key(|f| (f.0 as i64 - 600_000_000).abs()).unwrap();
    // The incoming card's left edge, below the bar.
    let edge = (0..402).find(|&x| blue(px(f, x, 400))).expect("the incoming card");
    assert!((100..200).contains(&edge), "card edge at {edge}");
    // The bar's titles sit 84 pt down. "Pushed" rides at its card's centre…
    let centre = edge + 201;
    assert!((centre.saturating_sub(30)..(centre + 30).min(402)).any(|x| dark(px(f, x, 84))), "Pushed rides with its card");
    // …and "Root" (the lower screen's) is hidden under the card: no text between the edge and "Pushed".
    assert!(!(edge + 4..centre - 40).any(|x| dark(px(f, x, 84))), "Root is under the card");
    assert!((0..edge).any(|x| dark(px(f, x, 84))), "Root shows left of the card");
    // The card's top-left corner is round: just inside its box, the red screen shows.
    assert!(red(px(f, edge + 2, 2)) || !blue(px(f, edge + 2, 2)), "card corner {:?}", px(f, edge + 2, 2));
    // The glass back button (a light circle at x 16–60) is fading in: lighter than red, not yet its full 0.96 white.
    let b = px(f, 30, 84);
    assert!(b[1] > 40 && b[1] < 220, "back button part way: {b:?}");
    // Settled: the button is there in full, and the title centred.
    let done = frames.iter().filter(|f| f.0 < 2_000_000_000).last().unwrap();
    assert!(px(done, 30, 84)[1] > 220, "back button: {:?}", px(done, 30, 84));
    assert!(dark(px(done, 201, 84)) || (180..222).any(|x| dark(px(done, x, 84))), "Pushed centred");
    // After the pop, back at the root: no back button, "Root" centred.
    let end = frames.last().unwrap();
    assert!(red(px(end, 30, 84)), "no back button at the root: {:?}", px(end, 30, 84));
}

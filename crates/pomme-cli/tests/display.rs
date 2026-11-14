//! Drawing only what changed (tests/fixtures/display/main.m), with a host
//! attached as the desktop window has: a view's `drawRect:` runs when it first
//! shows, after `setNeedsDisplay` and after a size change, not for every frame;
//! and frames are drawn only when something on screen changed, however often
//! the app's run loop turns. Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::host::{Frame, Host, Idle};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A host that never has input and counts the frames it's shown.
struct Counting(Arc<AtomicU64>);

impl Host for Counting {
    fn idle(&mut self, _: u64, _: Option<u64>) -> Idle {
        Idle::Advance
    }
    fn frame(&mut self, _: Frame<'_>) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn draws_only_what_changed() {
    let Some(mut emu) = common::boot("display") else { return };
    let frames = Arc::new(AtomicU64::new(0));
    *emu.live.host = Some(Box::new(Counting(frames.clone())));
    emu.live.scale = 1.0;
    let v = common::views(&mut emu, 5.0);
    let line = v.lines().find(|l| l.contains(" tag 1 ")).unwrap_or_else(|| panic!("{v}"));
    assert!(line.ends_with("\"draws=3\""), "{line}");
    // A 60 Hz timer ran for 5 s: ~300 run-loop turns, but only a handful of
    // changes (launch, the label, the redraw, the resize).
    let n = frames.load(Ordering::Relaxed);
    assert!((1..=12).contains(&n), "{n} frames");
}

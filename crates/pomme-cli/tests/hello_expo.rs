//! End to end: boot hello-expo, tap its button twice, check what's on screen.
//! Skipped unless the app (scripts/build-hello-expo.sh) and the sysroot
//! (scripts/build-sysroot.sh) have been built.

use pomme_hle::touch::{BEGAN, ENDED, Input};
use pomme_hle::{Emu, Stop};
use std::path::Path;

const APP: &str = "../../apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app";
const SYSROOT: &str = "../../vendor/sysroot";

fn tap(emu: &mut Emu, x: f64, y: f64, secs: f64) {
    let at = (secs * 1e9) as u64;
    emu.touch.script.push(Input { at_ns: at, phase: BEGAN, x, y });
    emu.touch.script.push(Input { at_ns: at + 80_000_000, phase: ENDED, x, y });
}

#[test]
fn renders_and_counts_taps() {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() {
        eprintln!("skipping: build hello-expo and the sysroot first");
        return;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    let p = pomme_loader::load_with(app, &opts).unwrap();
    let mut emu = Emu::new(p).unwrap();
    emu.time_limit_ns = Some(6_000_000_000);
    tap(&mut emu, 195.0, 438.0, 3.0);
    tap(&mut emu, 195.0, 438.0, 4.0);
    match emu.run_main() {
        Err(Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}"),
    }
    emu.time_limit_ns = None;
    let views = emu.describe_views();
    assert!(views.contains("\"Hello from pomme 🍎\""), "{views}");
    assert!(views.contains("\"Tapped 2 times\""), "{views}");
    let png = emu.screenshot_png(1.0).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
}

/// A host that delivers input at fixed virtual times (no wall clock) and keeps the last frame.
struct ScriptedHost {
    taps: Vec<(u64, f64, f64)>,
    frames: std::rc::Rc<std::cell::RefCell<(usize, Vec<u8>)>>,
    end_ns: u64,
}

impl pomme_hle::host::Host for ScriptedHost {
    fn idle(&mut self, now_ns: u64, next_ns: Option<u64>) -> pomme_hle::host::Idle {
        use pomme_hle::host::Idle;
        if let Some(&(at, x, y)) = self.taps.first()
            && next_ns.is_none_or(|n| at <= n)
        {
            self.taps.remove(0);
            let at = at.max(now_ns);
            return Idle::Input {
                at_ns: at,
                events: vec![Input { at_ns: at, phase: BEGAN, x, y }, Input { at_ns: at, phase: ENDED, x, y }],
            };
        }
        if next_ns.is_none_or(|n| n > self.end_ns) { Idle::Quit } else { Idle::Advance }
    }

    fn frame(&mut self, rgba: &[u8], _w: u32, _h: u32) {
        let mut f = self.frames.borrow_mut();
        f.0 += 1;
        f.1 = rgba.to_vec();
    }
}

#[test]
fn live_host_gets_frames_and_delivers_input() {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() {
        eprintln!("skipping: build hello-expo and the sysroot first");
        return;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    let mut emu = Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap();
    let frames = std::rc::Rc::new(std::cell::RefCell::new((0, Vec::new())));
    emu.live.scale = 1.0;
    emu.live.host = Some(Box::new(ScriptedHost { taps: vec![(3_000_000_000, 195.0, 438.0)], frames: frames.clone(), end_ns: 5_000_000_000 }));
    match emu.run_main() {
        Ok(0) => {} // the host quit
        other => panic!("expected the host to quit, got {other:?}"),
    }
    emu.live.host = None;
    let views = emu.describe_views();
    assert!(views.contains("\"Tapped 1 time\""), "{views}");
    let (n, last) = &*frames.borrow();
    assert!(*n >= 2, "only {n} frames"); // frames come when the main run loop wakes
    assert_eq!(last.len(), 390 * 844 * 4);
}

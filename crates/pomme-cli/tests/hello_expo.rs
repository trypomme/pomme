//! End to end: boot hello-expo, tap it, fetch through it, check the screen.
//! Skipped unless the app (scripts/build-hello-expo.sh) and the sysroot
//! (scripts/build-sysroot.sh) have been built.

use pomme_hle::net::{Fixtures, Net, Request, Response};
use pomme_hle::touch::{BEGAN, ENDED, Input};
use pomme_hle::{Emu, Stop};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};

const APP: &str = "../../apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app";
const SYSROOT: &str = "../../vendor/sysroot";
const API: &str = "https://api.github.com/repos/";

// Where the app's buttons are (points).
const TAP_BUTTON: (f64, f64) = (195.0, 296.0);
const SWR_REFRESH: (f64, f64) = (195.0, 414.0);
const QUERY_REFRESH: (f64, f64) = (195.0, 540.0);

fn boot() -> Option<Emu> {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() {
        eprintln!("skipping: build hello-expo and the sysroot first");
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    Some(Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap())
}

fn tap(emu: &mut Emu, (x, y): (f64, f64), secs: f64) {
    let at = (secs * 1e9) as u64;
    emu.touch.script.push(Input { at_ns: at, phase: BEGAN, x, y });
    emu.touch.script.push(Input { at_ns: at + 80_000_000, phase: ENDED, x, y });
}

/// Run until the app idles past `secs` of virtual time; then the view tree.
fn run_for(emu: &mut Emu, secs: f64) -> String {
    emu.time_limit_ns = Some((secs * 1e9) as u64);
    match emu.run_main() {
        Err(Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}"),
    }
    emu.time_limit_ns = None;
    emu.describe_views()
}

fn repos() -> Fixtures {
    Fixtures::default()
        .json(&format!("{API}vercel/swr"), r#"{"full_name":"vercel/swr","stargazers_count":31415}"#)
        .json(&format!("{API}TanStack/query"), r#"{"full_name":"TanStack/query","stargazers_count":48000}"#)
        .json(&format!("{API}facebook/react"), r#"{"full_name":"facebook/react","stargazers_count":240000}"#)
}

/// Fixtures that also record which URLs were requested.
struct Recording(Fixtures, Rc<RefCell<Vec<String>>>);

impl Net for Recording {
    fn fetch(&mut self, req: &Request) -> Result<Response, String> {
        self.1.borrow_mut().push(req.url.clone());
        self.0.fetch(req)
    }
}

#[test]
fn renders_and_counts_taps() {
    let Some(mut emu) = boot() else { return };
    emu.net.client = Some(Box::new(repos()));
    tap(&mut emu, TAP_BUTTON, 3.0);
    tap(&mut emu, TAP_BUTTON, 4.0);
    let views = run_for(&mut emu, 6.0);
    assert!(views.contains("\"Hello from pomme 🍎\""), "{views}");
    assert!(views.contains("\"Tapped 2 times\""), "{views}");
    let png = emu.screenshot_png(1.0).unwrap();
    assert!(png.starts_with(b"\x89PNG"));
}

#[test]
fn fetches_with_swr_query_and_promise_all() {
    let Some(mut emu) = boot() else { return };
    let log = Rc::new(RefCell::new(Vec::new()));
    emu.net.client = Some(Box::new(Recording(repos(), log.clone())));
    // Refresh both cards once their first fetch has landed.
    tap(&mut emu, SWR_REFRESH, 5.0);
    tap(&mut emu, QUERY_REFRESH, 5.5);
    let views = run_for(&mut emu, 7.0);
    assert!(views.contains("\"vercel/swr ★ 31,415\""), "{views}");
    assert!(views.contains("\"TanStack/query ★ 48,000\""), "{views}");
    assert!(views.contains("\"Promise.all: 3 repos, ★ 319,415\""), "{views}");
    let log = log.borrow();
    let count = |repo: &str| log.iter().filter(|u| u.ends_with(repo)).count();
    // SWR: first load plus the refresh (Promise.all asks for it once more).
    assert!(count("vercel/swr") >= 3, "{log:?}");
    assert!(count("TanStack/query") >= 3, "{log:?}");
    assert_eq!(count("facebook/react"), 1, "{log:?}");
}

#[test]
fn offline_fetch_shows_the_error() {
    let Some(mut emu) = boot() else { return };
    // No network at all: every request fails as if the device were offline.
    let views = run_for(&mut emu, 6.0);
    assert!(views.contains("Couldn't load"), "{views}");
    assert!(views.contains("Promise.all failed"), "{views}");
}

static HITS: AtomicUsize = AtomicUsize::new(0);

fn count_hit(emu: &mut Emu) -> pomme_hle::Result<()> {
    HITS.fetch_add(1, Ordering::Relaxed);
    emu.continue_original();
    Ok(())
}

/// Breakpoints on local (non-exported) Swift functions, whatever their first
/// instruction, and the app still runs correctly through them.
#[test]
fn breakpoints_on_local_swift_functions() {
    let Some(mut emu) = boot() else { return };
    emu.net.client = Some(Box::new(repos()));
    let main = &emu.p.images[0];
    let targets: Vec<u64> = main
        .symbols
        .iter()
        .filter(|(_, n)| n.contains("14NativeResponseC") || n.contains("18ExpoURLSessionTaskC"))
        .map(|(a, _)| *a)
        .collect();
    assert!(targets.len() > 20, "found only {} local symbols", targets.len());
    let mut placed = 0;
    for a in targets {
        // PC-relative loads as first instructions are the one thing hooks still refuse.
        if emu.hook(a, count_hit).is_ok() {
            placed += 1;
        }
    }
    assert!(placed > 20);
    let views = run_for(&mut emu, 6.0);
    assert!(HITS.load(Ordering::Relaxed) > 10);
    assert!(views.contains("\"vercel/swr ★ 31,415\""), "{views}");
    let name = emu.symbolize(emu.p.images[0].local_symbol("_$s4Expo14NativeResponseCMa").unwrap() + 4);
    assert!(name.contains("NativeResponseCMa+0x4"), "{name}");
}

/// A host that delivers input at fixed virtual times (no wall clock) and keeps the last frame.
struct ScriptedHost {
    taps: Vec<(u64, f64, f64)>,
    frames: Rc<RefCell<(usize, Vec<u8>)>>,
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
    let Some(mut emu) = boot() else { return };
    emu.net.client = Some(Box::new(repos()));
    let frames = Rc::new(RefCell::new((0, Vec::new())));
    emu.live.scale = 1.0;
    emu.live.host = Some(Box::new(ScriptedHost { taps: vec![(3_000_000_000, TAP_BUTTON.0, TAP_BUTTON.1)], frames: frames.clone(), end_ns: 5_000_000_000 }));
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

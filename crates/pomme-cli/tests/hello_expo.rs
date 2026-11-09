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

// Where the app's buttons are (points; from `pomme run --views`).
const TAP_BUTTON: (f64, f64) = (195.0, 265.0);
const SWR_REFRESH: (f64, f64) = (195.0, 383.0);
const QUERY_REFRESH: (f64, f64) = (153.0, 508.0);
// Expo Router: the query card's link to /repo/TanStack/query, and the header's back button.
const DETAILS: (f64, f64) = (240.0, 508.0);
const BACK: (f64, f64) = (38.0, 69.0);

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

    // The 🍎 is drawn (from the emoji font): red pixels on the greeting's line.
    let line = views.lines().find(|l| l.contains("Hello from pomme")).unwrap();
    let frame: Vec<f64> = line.split(['(', ',', '×', ')']).filter_map(|t| t.trim().parse().ok()).take(4).collect();
    let (x, y, w, h) = (frame[0], frame[1], frame[2], frame[3]);
    let pix = pomme_hle::render::render(&mut emu, 1.0).unwrap();
    let red = (y as u32..(y + h) as u32)
        .flat_map(|py| (x as u32..(x + w) as u32).map(move |px| (px, py)))
        .filter_map(|(px, py)| pix.pixel(px, py))
        .filter(|c| c.red() > 180 && c.green() < 110 && c.blue() < 110)
        .count();
    assert!(red > 20, "no emoji on the greeting line ({red} red pixels)");
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

/// MSW, and the `expo` package's APIs (text encoding, URL, structuredClone, streams,
/// EventEmitter, CodedError, Platform, uuid, native modules, expo/fetch streaming).
#[test]
fn expo_sdk_and_msw() {
    let Some(mut emu) = boot() else { return };
    emu.net.client = Some(Box::new(repos()));
    let views = run_for(&mut emu, 8.0);
    assert!(views.contains("\"Expo SDK: 11/11 passed\""), "{views}");
    // MSW mocks https://pomme.test in JS: no request reaches the network for it.
    assert!(views.contains("\"MSW: hello from MSW\""), "{views}");
}

#[test]
fn offline_fetch_shows_the_error() {
    let Some(mut emu) = boot() else { return };
    // No network at all: every request fails as if the device were offline.
    let views = run_for(&mut emu, 6.0);
    assert!(views.contains("Couldn't load"), "{views}");
    assert!(views.contains("Promise.all failed"), "{views}");
}

/// Expo Router's native stack (react-native-screens on UINavigationController):
/// push a route, go back by the header button, push again; the home screen
/// keeps its state across the round trip.
#[test]
fn expo_router_pushes_and_goes_back() {
    let Some(mut emu) = boot() else { return };
    emu.net.client = Some(Box::new(repos()));
    tap(&mut emu, TAP_BUTTON, 3.0);
    tap(&mut emu, DETAILS, 5.0);
    tap(&mut emu, BACK, 6.5);
    let views = run_for(&mut emu, 7.5);
    assert!(views.contains("\"Tapped 1 time\""), "{views}");
    assert!(!views.contains("stars"), "still on the details screen: {views}");
    // React Navigation saw the native pop, so the link works again.
    let mut emu2 = boot().unwrap();
    emu2.net.client = Some(Box::new(repos()));
    tap(&mut emu2, DETAILS, 5.0);
    tap(&mut emu2, BACK, 6.5);
    tap(&mut emu2, DETAILS, 8.0);
    let views = run_for(&mut emu2, 9.0);
    assert!(views.contains("\"TanStack/query\""), "{views}");
    assert!(views.contains("\"★ 48,000 stars\""), "{views}");
    assert!(!views.contains("Hello from pomme"), "{views}");

    // The header draws its title: dark pixels centered in the bar (y 47–91).
    let pix = pomme_hle::render::render(&mut emu2, 1.0).unwrap();
    let dark = (55..85)
        .flat_map(|py| (150..240).map(move |px| (px, py)))
        .filter_map(|(px, py)| pix.pixel(px, py))
        .filter(|c| c.red() < 80 && c.green() < 80 && c.blue() < 80)
        .count();
    assert!(dark > 30, "no title in the navigation bar ({dark} dark pixels)");
}

/// The same app presented as iOS 18.6: pre-iOS 26 behaviour throughout. The
/// screen is laid out below the opaque bar, the bar is white with a hairline,
/// and the back button is a blue chevron with the previous screen's title.
#[test]
fn ios_18_has_the_pre_26_navigation_bar() {
    let Some(mut emu) = boot() else { return };
    emu.system.os = "18.6".parse().unwrap();
    assert!(!emu.system.liquid_glass());
    emu.net.client = Some(Box::new(repos()));
    // Below the 91-pt bar, the content is re-centered in 753 pt: Details sits lower.
    tap(&mut emu, (240.0, 554.0), 5.0);
    let views = run_for(&mut emu, 6.5);
    assert!(views.contains("RNSScreenView (0.0, 91.0, 390.0×753.0)"), "{views}");
    assert!(views.contains("\"★ 48,000 stars\""), "{views}");

    let pix = pomme_hle::render::render(&mut emu, 1.0).unwrap();
    let px = |x: u32, y: u32| pix.pixel(x, y).unwrap();
    // Opaque white bar up under the status bar (beside the notch), hairline at its bottom edge.
    assert_eq!((px(60, 20).red(), px(60, 20).green(), px(60, 20).blue()), (255, 255, 255));
    // The hairline is one device pixel: row 273 at 3×.
    let hi = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let line = hi.pixel(600, 273).unwrap();
    assert!(line.red() < 200 && hi.pixel(600, 276).unwrap().red() == 255, "no hairline under the bar ({line:?})");
    // "‹ pomme" in system blue on the left of the bar.
    let blue = (55..85)
        .flat_map(|y| (5..90).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let c = px(x, y);
            c.blue() > 200 && c.red() < 80
        })
        .count();
    assert!(blue > 40, "no blue back button ({blue} pixels)");

    // Back, by the label.
    let mut emu = boot().unwrap();
    emu.system.os = "18.6".parse().unwrap();
    emu.net.client = Some(Box::new(repos()));
    tap(&mut emu, (240.0, 554.0), 5.0);
    tap(&mut emu, (40.0, 69.0), 6.5);
    let views = run_for(&mut emu, 7.5);
    assert!(!views.contains("stars"), "{views}");
    assert!(views.contains("\"Hello from pomme 🍎\""), "{views}");
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

// ---- text input and the keyboard (the /keyboard screen) ----

const NAME_FIELD: (f64, f64) = (195.0, 806.0);

fn type_text(emu: &mut Emu, text: &str, secs: f64) {
    emu.input.script.push(pomme_hle::textinput::KeyInput { at_ns: (secs * 1e9) as u64, text: text.into() });
}

/// The y of a view line like `ClassName (x, y, w×h) "text"` holding `needle`.
fn y_of(views: &str, needle: &str) -> f64 {
    let line = views.lines().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("no {needle:?} in\n{views}"));
    let inner = &line[line.find('(').unwrap() + 1..];
    inner.split(',').nth(1).unwrap().trim().parse().unwrap()
}

/// The keyboard screen, opened by deep link, after `script` and `secs` of virtual time.
fn keyboard_screen(script: impl FnOnce(&mut Emu), secs: f64) -> Option<(Emu, String)> {
    let mut emu = boot()?;
    emu.ui.launch_url = Some("pommehello:///keyboard".into());
    script(&mut emu);
    let views = run_for(&mut emu, secs);
    Some((emu, views))
}

/// A deep link opens the screen; tapping the TextInput focuses it (the keyboard
/// comes up and the KeyboardAvoidingView lifts the content above it); typed
/// keys reach React through UITextField's delegate; delete edits; return
/// submits and ends editing, and the keyboard goes back down.
#[test]
fn text_input_and_keyboard_avoiding_view() {
    let Some((emu, before)) = keyboard_screen(|_| {}, 2.5) else { return };
    assert!(before.contains("\"Type your name below\""), "{before}");
    assert!(before.contains("\"Not editing · 0 characters\""), "{before}");
    assert!(!emu.input.keyboard_shown);
    let resting = y_of(&before, "RCTTextInputComponentView");

    let typing_script = |emu: &mut Emu| {
        tap(emu, NAME_FIELD, 3.0);
        type_text(emu, "Adx\u{8}", 4.0);
        type_text(emu, "a", 4.5);
    };
    let (emu, typing) = keyboard_screen(typing_script, 5.5).unwrap();
    assert!(emu.input.keyboard_shown, "the keyboard should be up");
    assert!(typing.contains("\"Hello, Ada!\""), "{typing}");
    assert!(typing.contains("\"Editing · 3 characters\""), "{typing}");
    // React Native's keyboardDidShow: iOS 26's 335-pt keyboard on the 844-pt screen.
    assert!(typing.contains("\"keyboard 509+335\""), "{typing}");
    // Lifted by the keyboard's height (less the screen's offset).
    let lifted = y_of(&typing, "RCTTextInputComponentView");
    assert!(resting - lifted > 200.0, "the input moved from {resting} to {lifted}");

    let (emu, done) = keyboard_screen(
        |emu| {
            typing_script(emu);
            type_text(emu, "\n", 6.0);
        },
        7.0,
    )
    .unwrap();
    assert!(!emu.input.keyboard_shown, "return should end editing");
    assert!(done.contains("submitted \\\"Ada\\\""), "{done}");
    assert!(done.contains("\"Not editing · 3 characters"), "{done}");
    assert!(done.contains("\"keyboard hidden\""), "{done}");
    assert_eq!(y_of(&done, "RCTTextInputComponentView"), resting);
}

/// A paste into the TextInput: another app's text arrives through
/// UITextField's `paste:` and its delegate, like typing; React sees it, and
/// the user chose to paste, so iOS doesn't ask.
#[test]
fn paste_into_text_input() {
    let script = |emu: &mut Emu| {
        tap(emu, NAME_FIELD, 3.0);
        type_text(emu, "Hi ", 4.0);
        emu.clipboard.script.push(pomme_hle::clipboard::PasteInput { at_ns: 4_500_000_000, text: Some("Ada Lovelace".into()) });
    };
    let Some((mut emu, v)) = keyboard_screen(script, 5.5) else { return };
    assert!(v.contains("\"Hello, Hi Ada Lovelace!\""), "{v}");
    assert!(v.contains("\"Editing · 15 characters\""), "{v}");
    assert_eq!(emu.clipboard.prompts, 0);
    assert_eq!(emu.clipboard.text().as_deref(), Some("Ada Lovelace"));
}

// ---- multiline text input (the /notes screen) ----

const NOTES: (f64, f64) = (195.0, 129.0);

/// The notes screen, opened by deep link, after `script` and `secs` of virtual time.
fn notes_screen(script: impl FnOnce(&mut Emu), secs: f64) -> Option<(Emu, String)> {
    let mut emu = boot()?;
    emu.ui.launch_url = Some("pommehello:///notes".into());
    script(&mut emu);
    let views = run_for(&mut emu, secs);
    Some((emu, views))
}

/// The height in a view line like `ClassName (x, y, w×h) …` holding `needle`.
fn height_of(views: &str, needle: &str) -> f64 {
    let line = views.lines().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("no {needle:?} in\n{views}"));
    let size = line.split(", ").nth(2).unwrap();
    size.split('×').nth(1).unwrap().split(')').next().unwrap().parse().unwrap()
}

/// A multiline TextInput is a UITextView (RCTUITextView): its placeholder is
/// a UILabel; return types a newline; React sees each change, and the input
/// grows with its content (10 pt padding + 20.3 pt lines) up to its maxHeight.
#[test]
fn multiline_text_input_grows() {
    let Some((_, before)) = notes_screen(|_| {}, 3.0) else { return };
    assert!(before.contains("UILabel (12.0, 10.0, 332.0×21.0) \"Write a note\""), "{before}");
    assert_eq!(height_of(&before, "RCTTextInputComponentView"), 44.0, "{before}");
    assert!(before.contains("\"content 40\""), "{before}");

    let (emu, three) = notes_screen(
        |emu| {
            tap(emu, NOTES, 3.0);
            type_text(emu, "Shopping\nmilk\neggs", 4.0);
        },
        5.0,
    )
    .unwrap();
    assert!(emu.input.keyboard_shown);
    assert!(three.contains("\"Shopping\\nmilk\\neggs\""), "{three}");
    assert!(three.contains("\"Editing · 3 lines · 18 characters\""), "{three}");
    assert!(three.contains("hidden \"Write a note\""), "the placeholder hides\n{three}");
    assert!(three.contains("\"content 81\""), "{three}");
    assert_eq!(height_of(&three, "RCTTextInputComponentView"), 83.0, "{three}");

    let (_, seven) = notes_screen(
        |emu| {
            tap(emu, NOTES, 3.0);
            type_text(emu, "Shopping\nmilk\neggs", 4.0);
            type_text(emu, "\nbread\ncoffee\njam\ntea", 5.0);
        },
        6.0,
    )
    .unwrap();
    assert!(seven.contains("\"Editing · 7 lines · 39 characters\""), "{seven}");
    // Capped at maxHeight: the rest scrolls.
    assert_eq!(height_of(&seven, "RCTTextInputComponentView"), 140.0, "{seven}");
}

/// A paste into the multiline input goes through UITextView's `paste:`,
/// newlines and all.
#[test]
fn paste_into_multiline_text_input() {
    let script = |emu: &mut Emu| {
        tap(emu, NOTES, 3.0);
        type_text(emu, "Shopping", 4.0);
        emu.clipboard.script.push(pomme_hle::clipboard::PasteInput { at_ns: 4_500_000_000, text: Some("\nmilk\neggs".into()) });
    };
    let Some((mut emu, v)) = notes_screen(script, 5.5) else { return };
    assert!(v.contains("\"Shopping\\nmilk\\neggs\""), "{v}");
    assert!(v.contains("\"Editing · 3 lines · 18 characters\""), "{v}");
    assert_eq!(emu.clipboard.prompts, 0);
}

// ---- scrolling (the /list screen) ----

/// A fling scrolls a FlatList: React Native's scroll view (RCTEnhancedScrollView,
/// sized by its autoresizing mask) drags 390 pt under a 400-pt, 0.2-s swipe
/// and decelerates at UIKit's rate (2 pt/ms carries it 998 pt further). Its
/// delegate is a splitter that forwards with NSInvocation, and React hears
/// the drag and the glide end. The rows that scrolled in are tappable where
/// they now are; the swipe itself taps nothing.
#[test]
fn flatlist_flings_and_taps() {
    let mut emu = match boot() {
        Some(e) => e,
        None => return,
    };
    emu.ui.launch_url = Some("pommehello:///list".into());
    pomme_hle::touch::swipe(&mut emu.touch.script, 3_000_000_000, (195.0, 700.0), (195.0, 300.0), 200_000_000);
    // Row 30 sits at 50 × 30 − 1388 below the list's top (133): 245…295.
    pomme_hle::touch::tap(&mut emu.touch.script, 7_500_000_000, 195.0, 269.0);
    let v = run_for(&mut emu, 8.5);
    assert!(v.contains("\"offset 1388 · still · tapped Row 30\""), "{v}");
}

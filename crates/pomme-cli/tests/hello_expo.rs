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
        other => panic!("expected the app to idle until the time limit, got {other:?} (last exception: {:?}; console: {})", emu.last_exception, emu.console.lines().rev().take(15).collect::<Vec<_>>().join("\n")),
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
    *emu.net.client = Some(Box::new(repos()));
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
    *emu.net.client = Some(Box::new(Recording(repos(), log.clone())));
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
    *emu.net.client = Some(Box::new(repos()));
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
    *emu.net.client = Some(Box::new(repos()));
    tap(&mut emu, TAP_BUTTON, 3.0);
    tap(&mut emu, DETAILS, 5.0);
    tap(&mut emu, BACK, 6.5);
    let views = run_for(&mut emu, 7.5);
    assert!(views.contains("\"Tapped 1 time\""), "{views}");
    assert!(!views.contains("stars"), "still on the details screen: {views}");
    // React Navigation saw the native pop, so the link works again.
    let mut emu2 = boot().unwrap();
    *emu2.net.client = Some(Box::new(repos()));
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
    *emu.net.client = Some(Box::new(repos()));
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
    *emu.net.client = Some(Box::new(repos()));
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
    *emu.net.client = Some(Box::new(repos()));
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
        use pomme_hle::host::{Event, Idle};
        if let Some(&(at, x, y)) = self.taps.first()
            && next_ns.is_none_or(|n| at <= n)
        {
            self.taps.remove(0);
            let at = at.max(now_ns);
            return Idle::Input {
                at_ns: at,
                events: vec![Event::Touch(Input { at_ns: at, phase: BEGAN, x, y }), Event::Touch(Input { at_ns: at, phase: ENDED, x, y })],
            };
        }
        if next_ns.is_none_or(|n| n > self.end_ns) { Idle::Quit } else { Idle::Advance }
    }

    fn frame(&mut self, frame: pomme_hle::host::Frame<'_>) {
        let mut f = self.frames.borrow_mut();
        f.0 += 1;
        f.1 = frame.rgba.to_vec();
    }
}

#[test]
fn live_host_gets_frames_and_delivers_input() {
    let Some(mut emu) = boot() else { return };
    *emu.net.client = Some(Box::new(repos()));
    let frames = Rc::new(RefCell::new((0, Vec::new())));
    emu.live.scale = 1.0;
    *emu.live.host = Some(Box::new(ScriptedHost { taps: vec![(3_000_000_000, TAP_BUTTON.0, TAP_BUTTON.1)], frames: frames.clone(), end_ns: 5_000_000_000 }));
    match emu.run_main() {
        Ok(0) => {} // the host quit
        other => panic!("expected the host to quit, got {other:?}"),
    }
    *emu.live.host = None;
    let views = emu.describe_views();
    assert!(views.contains("\"Tapped 1 time\""), "{views}");
    let (n, last) = &*frames.borrow();
    assert!(*n >= 2, "only {n} frames"); // frames come when the main run loop wakes
    assert_eq!(last.len(), 390 * 844 * 4);
}

/// A live frame shows what a still render shows: the frame lays out before
/// drawRect: runs, so React Native's paragraphs aren't cached drawn into the
/// drawing frame they had before layout (which cut "Tapped 0 times" to "T").
#[test]
fn live_frames_lay_out_before_they_draw() {
    let Some(mut emu) = boot() else { return };
    *emu.net.client = Some(Box::new(repos()));
    let frames = Rc::new(RefCell::new((0, Vec::new())));
    emu.live.scale = 2.0;
    *emu.live.host = Some(Box::new(ScriptedHost { taps: vec![], frames: frames.clone(), end_ns: 4_000_000_000 }));
    let _ = emu.run_main();
    // The same screen, drawn once at the end by an app that ran without frames.
    let Some(mut still) = boot() else { return };
    *still.net.client = Some(Box::new(repos()));
    run_for(&mut still, 4.0);
    let still = pomme_hle::render::render(&mut still, 2.0).unwrap();
    let (_, live) = &*frames.borrow();
    assert_eq!(live.len(), still.data().len());
    let differ = live.chunks(4).zip(still.data().chunks(4)).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 8)).count();
    assert!(differ < 200, "{differ} pixels differ from a still render");
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
/// sized by its autoresizing mask) drags 392 pt under a 400-pt, 0.2-s swipe
/// (less the 10 that start the drag, plus 2 as the finger lifts mid-motion)
/// and decelerates at UIKit's rate: 2 pt/ms carries it 996 pt further. Its
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
    // The glide takes about 3.8 s (to under half a point from rest).
    pomme_hle::touch::tap(&mut emu.touch.script, 8_500_000_000, 195.0, 269.0);
    let v = run_for(&mut emu, 9.5);
    assert!(v.contains("\"offset 1388 · still · tapped Row 30\""), "{v}");
}

// ---- react-native-gesture-handler (the /gestures screen) ----

/// Gesture handler's gestures run on UIKit recognizers: its tap is its own
/// recognizer (with a dispatch_after timeout it can cancel), and its pan
/// subclasses UIPanGestureRecognizer, which begins after 10 pt and reports
/// its translation (from where the finger went down) and velocity. Dragging
/// the box 150,200 moves it by that. (The iOS Simulator's synthesized drags
/// came out 10–20 pt short of their end point, fast or slow: its finger
/// samples, not the recognizer.)
#[test]
fn gesture_handler_tap_and_pan() {
    let mut emu = match boot() {
        Some(e) => e,
        None => return,
    };
    emu.ui.launch_url = Some("pommehello:///gestures".into());
    pomme_hle::touch::tap(&mut emu.touch.script, 3_000_000_000, 195.0, 171.0);
    pomme_hle::touch::swipe(&mut emu.touch.script, 4_000_000_000, (76.0, 277.0), (226.0, 477.0), 500_000_000);
    let v = run_for(&mut emu, 5.5);
    assert!(v.contains("\"dropped at 150,200 · box 150,200 · 1 taps\""), "{v}");
}

// ---- react-native-keyboard-controller (the /sticky screen) ----

/// react-native-keyboard-controller (with Reanimated and worklets): a
/// KeyboardStickyView rides on top of the keyboard and comes back down when
/// it's dismissed. Its keyboard handler's worklets run on Reanimated's UI
/// runtime and report back to React ("keyboard 335 at 100%").
#[test]
fn keyboard_controller_sticky_view() {
    // The colour at a point (pt) after `secs`, and the status line.
    let run = |secs: f64, dismiss: bool| -> Option<((u8, u8, u8), String)> {
        let mut emu = boot()?;
        emu.ui.launch_url = Some("pommehello:///sticky".into());
        tap(&mut emu, (195.0, 806.0), 3.0);
        if dismiss {
            type_text(&mut emu, "hi\n", 4.0);
        }
        let v = run_for(&mut emu, secs);
        let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
        // Left of the field, inside the sticky view's padding when it's up.
        let c = pix.pixel(8 * 3, 440 * 3).unwrap();
        let status = v.lines().find(|l| l.contains(" tag 14 ")).unwrap_or_default().to_string();
        Some(((c.red(), c.green(), c.blue()), status))
    };
    let Some((up, status)) = run(4.0, false) else { return };
    assert!(status.contains("\"keyboard 335 at 100%"), "{status}");
    // The sticky view's background (#f8fafc) sits just above the 335-pt keyboard (509).
    assert_eq!(up, (0xf8, 0xfa, 0xfc), "sticky view up: {up:?}");
    let (down, status) = run(6.0, true).unwrap();
    assert!(status.contains("\"keyboard 0 at 0%"), "{status}");
    assert_eq!(down, (0xff, 0xff, 0xff), "page, the sticky view back at the bottom: {down:?}");
}

/// The rotation screen's lines (size and change count, insets) after
/// turning the phone at the given times.
fn rotation_screen(turns: &[(f64, pomme_hle::device::Orientation)], secs: f64) -> Option<(Emu, String, String)> {
    let mut emu = boot()?;
    emu.ui.launch_url = Some("pommehello:///rotation".into());
    emu.ui.rotations = turns.iter().map(|&(t, o)| ((t * 1e9) as u64, o)).collect();
    let v = run_for(&mut emu, secs);
    let line = |needle: &str| v.lines().find(|l| l.contains(needle)).map(|l| l.trim().to_string()).unwrap_or_else(|| panic!("no {needle:?} in\n{v}"));
    let (size, insets) = (line("changes\""), line("\"insets "));
    Some((emu, size, insets))
}

/// Turned to landscape: React Native hears the window's new size once
/// (useWindowDimensions, Dimensions 'change'), react-native-safe-area-context
/// the new insets (the notch's side and the home indicator), and the
/// screen's content is laid out inside them; the bar spans the safe width.
#[test]
fn rotation_to_landscape() {
    use pomme_hle::device::Orientation::LandscapeRight;
    let Some((mut emu, size, insets)) = rotation_screen(&[(3.0, LandscapeRight)], 5.0) else { return };
    // Inset by the safe area's sides: 47 pt in, 750 pt wide.
    assert!(size.contains("(47.0, 44.0, 750.0×44.3)") && size.contains("\"landscape 844x390 · 1 changes\""), "{size}");
    assert!(insets.contains("\"insets 0,47,20,47\""), "{insets}");
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    assert_eq!((pix.width(), pix.height()), (844 * 3, 390 * 3));
    let at = |x: u32, y: u32| {
        let c = pix.pixel(x * 3, y * 3).unwrap();
        (c.red(), c.green(), c.blue())
    };
    // The bar (#0f766e) is below the two lines, from x 47 to 797.
    let y = 44 + 89 + 20;
    assert_eq!(at(60, y), (0x0f, 0x76, 0x6e));
    assert_eq!(at(790, y), (0x0f, 0x76, 0x6e));
    assert_eq!(at(805, y), (255, 255, 255));
}

/// And back to portrait: a second change, and portrait's insets again.
#[test]
fn rotation_and_back() {
    use pomme_hle::device::Orientation::{LandscapeLeft, Portrait};
    let Some((_, size, insets)) = rotation_screen(&[(3.0, LandscapeLeft), (5.0, Portrait)], 7.0) else { return };
    assert!(size.contains("\"portrait 390x844 · 2 changes\""), "{size}");
    assert!(insets.contains("\"insets 47,0,34,0\""), "{insets}");
}

/// expo-image (SDWebImage underneath): a bundled PNG (a file URL), a remote
/// JPEG through ImageIO, shown with contentFit cover (its middle third fills
/// the square) and contain (letterboxed), and a blurhash placeholder for an
/// image that's never found. onLoad reports each image's pixel size.
#[test]
fn expo_image() {
    let Some(mut emu) = boot() else { return };
    let jpeg = std::fs::read("tests/fixtures/wide.jpg").unwrap();
    *emu.net.client = Some(Box::new(Fixtures::default().file("https://images.pomme.test/wide.jpg", "image/jpeg", jpeg)));
    emu.ui.launch_url = Some("pommehello:///image".into());
    let v = run_for(&mut emu, 6.0);
    assert!(v.contains("\"contain 300x100 · cover 300x100 · local 200x200\""), "{v}");
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let at = |x: f64, y: f64| {
        let c = pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap();
        (c.red(), c.green(), c.blue())
    };
    let near = |(r, g, b): (u8, u8, u8), (er, eg, eb): (u8, u8, u8)| r.abs_diff(er) < 12 && g.abs_diff(eg) < 12 && b.abs_diff(eb) < 12;
    // The squares are 120 pt at x 20; their tops at y 133, 265, 397, 529.
    // Local: red, green / blue, amber quadrants.
    assert!(near(at(50.0, 163.0), (0xdc, 0x26, 0x26)), "local top left: {:?}", at(50.0, 163.0));
    assert!(near(at(110.0, 223.0), (0xf5, 0x9e, 0x0b)), "local bottom right: {:?}", at(110.0, 223.0));
    // Cover: the 3:1 picture filling the square shows its middle (purple) third, edge to edge.
    for x in [25.0, 80.0, 135.0] {
        assert!(near(at(x, 325.0), (0x7c, 0x3a, 0xed)), "cover at {x}: {:?}", at(x, 325.0));
    }
    // Contain: all three thirds in a 40 pt band across the middle; the view's grey above it.
    assert!(near(at(30.0, 457.0), (0x0f, 0x76, 0x6e)), "contain left: {:?}", at(30.0, 457.0));
    assert!(near(at(130.0, 457.0), (0xea, 0x58, 0x0c)), "contain right: {:?}", at(130.0, 457.0));
    assert!(near(at(80.0, 410.0), (0xe5, 0xe7, 0xeb)), "contain letterbox: {:?}", at(80.0, 410.0));
    // The blurhash: soft colour, not the view's grey.
    assert!(!near(at(80.0, 589.0), (0xe5, 0xe7, 0xeb)), "blurhash: {:?}", at(80.0, 589.0));
}

/// The network screen's lines: the state (type, connected, reachable,
/// listener updates) and the IP address.
fn network_screen(setup: impl FnOnce(&mut Emu), secs: f64) -> Option<(String, String)> {
    let mut emu = boot()?;
    emu.ui.launch_url = Some("pommehello:///network".into());
    setup(&mut emu);
    let v = run_for(&mut emu, secs);
    let line = |needle: &str| v.lines().find(|l| l.contains(needle)).map(|l| l.trim().to_string()).unwrap_or_else(|| panic!("no {needle:?} in\n{v}"));
    Some((line(" updates\""), line("\"ip ")))
}

/// expo-network over the Network framework: on Wi-Fi by default, with the
/// Wi-Fi interface's address; its listener hears the monitor's first path.
#[test]
fn expo_network_on_wifi() {
    let Some((state, ip)) = network_screen(|_| {}, 5.0) else { return };
    assert!(state.contains("\"WIFI · connected true · reachable true · 1 updates\""), "{state}");
    assert!(ip.contains("\"ip 192.168.1.23\""), "{ip}");
}

/// The network changes while the app runs: cellular, then offline; each change reaches the listener.
#[test]
fn expo_network_follows_changes() {
    use pomme_hle::network::Path;
    let Some((state, _)) = network_screen(|e| {
        e.network.changes = vec![(3_000_000_000, Path::Cellular)];
    }, 4.0) else { return };
    assert!(state.contains("\"CELLULAR · connected true · reachable true · 2 updates\""), "{state}");
    let Some((state, _)) = network_screen(|e| {
        e.network.changes = vec![(3_000_000_000, Path::Cellular), (4_000_000_000, Path::Offline)];
    }, 5.0) else { return };
    assert!(state.contains("\"NONE · connected false · reachable false · 3 updates\""), "{state}");
}

/// With the phone offline, requests fail even though a server (the fixtures) would answer.
#[test]
fn offline_network_fails_requests() {
    let Some(mut emu) = boot() else { return };
    *emu.net.client = Some(Box::new(repos()));
    emu.network.path = pomme_hle::network::Path::Offline;
    let views = run_for(&mut emu, 6.0);
    assert!(views.contains("Couldn't load"), "{views}");
}

/// The storage screen's lines (AsyncStorage, SecureStore, the Face ID-protected value).
fn storage_screen(biometry: pomme_hle::keychain::Biometry) -> Option<(String, String, String)> {
    storage_screen_with_faces(biometry, &[])
}

fn storage_screen_with_faces(biometry: pomme_hle::keychain::Biometry, faces: &[(f64, pomme_hle::keychain::Face)]) -> Option<(String, String, String)> {
    let mut emu = boot()?;
    emu.ui.launch_url = Some("pommehello:///storage".into());
    emu.keychain.biometry = biometry;
    emu.keychain.faces = faces.iter().map(|&(t, f)| ((t * 1e9) as u64, f)).collect();
    let v = run_for(&mut emu, 5.0);
    let line = |needle: &str| v.lines().find(|l| l.contains(needle)).map(|l| l.trim().to_string()).unwrap_or_else(|| panic!("no {needle:?} in\n{v}"));
    Some((line("\"async "), line("\"secure "), line("\"protected ")))
}

/// AsyncStorage (files in the data container) and expo-secure-store (the
/// Keychain) round trips: replacing, merging, removing, keychain services
/// kept apart, deleting. Face ID isn't enrolled, so the protected value can't be read.
#[test]
fn async_storage_and_secure_store() {
    use pomme_hle::keychain::Biometry;
    let Some((async_line, secure, protected)) = storage_screen(Biometry::NotEnrolled) else { return };
    assert!(async_line.contains(r#""async hello · b,greeting,profile · a=null b=2 · {\"name\":\"Ada\",\"city\":\"London\"}""#), "{async_line}");
    assert!(secure.contains("\"secure def456 · null · x · null · null · available true · biometrics false\""), "{secure}");
    assert!(protected.contains("\"protected error"), "{protected}");
}

/// With Face ID enrolled and a matching face, the protected value reads back; with a face that doesn't match, it doesn't.
#[test]
fn secure_store_with_face_id() {
    use pomme_hle::keychain::Biometry;
    let Some((_, secure, protected)) = storage_screen(Biometry::Matching) else { return };
    assert!(secure.contains("biometrics true\""), "{secure}");
    assert!(protected.contains("\"protected opensesame\""), "{protected}");
    let (_, _, protected) = storage_screen(Biometry::NonMatching).unwrap();
    assert!(protected.contains("\"protected error"), "{protected}");
}

/// On an enrolled phone, reading the protected value blocks expo-secure-store
/// while the prompt waits; the face presented decides what it reads.
#[test]
fn secure_store_waits_for_a_face() {
    use pomme_hle::keychain::{Biometry, Face};
    let Some((_, _, protected)) = storage_screen_with_faces(Biometry::Enrolled, &[]) else { return };
    assert!(protected.contains("\"protected …\""), "{protected}");
    let (_, _, protected) = storage_screen_with_faces(Biometry::Enrolled, &[(3.0, Face::Matching)]).unwrap();
    assert!(protected.contains("\"protected opensesame\""), "{protected}");
    let (_, _, protected) = storage_screen_with_faces(Biometry::Enrolled, &[(3.0, Face::NonMatching)]).unwrap();
    assert!(protected.contains("\"protected error"), "{protected}");
}

// ---- the lock screen: a PIN pad with Face ID ----

/// The keypad's keys (pt).
fn key(k: char) -> (f64, f64) {
    let i = "123456789 0⌫".chars().position(|c| c == k).unwrap();
    (103.0 + 92.0 * (i % 3) as f64, 225.0 + 92.0 * (i / 3) as f64)
}
const FACE_ID_SWITCH: (f64, f64) = (343.0, 166.0);
const LOCK_BUTTON: (f64, f64) = (195.0, 240.0);
const FACE_ID_BUTTON: (f64, f64) = (195.0, 587.0);

/// Run the lock screen with this biometry and these taps (from 2 s, 0.3 s apart): its status line.
fn lock_screen(biometry: pomme_hle::keychain::Biometry, taps: &[(f64, f64)]) -> Option<String> {
    lock_screen_with_faces(biometry, taps, &[])
}

/// As [`lock_screen`], presenting these faces at these times (s).
fn lock_screen_with_faces(biometry: pomme_hle::keychain::Biometry, taps: &[(f64, f64)], faces: &[(f64, pomme_hle::keychain::Face)]) -> Option<String> {
    let mut emu = boot()?;
    emu.ui.launch_url = Some("pommehello:///lock".into());
    emu.keychain.biometry = biometry;
    emu.keychain.faces = faces.iter().map(|&(t, f)| ((t * 1e9) as u64, f)).collect();
    for (i, &p) in taps.iter().enumerate() {
        tap(&mut emu, p, 2.0 + 0.3 * i as f64);
    }
    let v = run_for(&mut emu, 2.0 + 0.3 * taps.len() as f64 + 1.0);
    Some(v.lines().find(|l| l.contains(" tag 6 ")).map(|l| l.trim().to_string()).unwrap_or_else(|| panic!("no status in\n{v}")))
}

fn pin(digits: &str) -> Vec<(f64, f64)> {
    digits.chars().map(key).collect()
}

/// Choose a PIN on the pad (kept in the Keychain), then unlock with it; a wrong one doesn't.
#[test]
fn lock_screen_pin() {
    use pomme_hle::keychain::Biometry;
    let mut taps = pin("1234");
    taps.extend(pin("1234"));
    let Some(status) = lock_screen(Biometry::NotEnrolled, &taps) else { return };
    assert!(status.ends_with("\"Unlocked with PIN\""), "{status}");
    // ⌫ takes back a digit: 1 2 ⌫ 3 4 5 is 1345, not the PIN.
    let mut taps = pin("1234");
    taps.extend(pin("12⌫345"));
    let status = lock_screen(Biometry::NotEnrolled, &taps).unwrap();
    assert!(status.ends_with("\"Wrong PIN\""), "{status}");
}

/// With a matching face: turning "Unlock with Face ID" on asks for it, and once locked the Face ID button unlocks.
#[test]
fn lock_screen_face_id() {
    use pomme_hle::keychain::Biometry;
    let mut taps = pin("1234");
    taps.extend(pin("1234"));
    taps.extend([FACE_ID_SWITCH, LOCK_BUTTON, FACE_ID_BUTTON]);
    let Some(status) = lock_screen(Biometry::Matching, &taps) else { return };
    assert!(status.ends_with("\"Unlocked with Face ID\""), "{status}");
}

/// On an enrolled phone each prompt waits for a face, as in the Simulator:
/// nothing happens until one is presented, a face that doesn't match fails
/// that prompt, and a matching one passes the next.
#[test]
fn lock_screen_face_id_waits_for_a_face() {
    use pomme_hle::keychain::{Biometry, Face};
    let mut taps = pin("1234");
    taps.extend(pin("1234"));
    taps.push(FACE_ID_SWITCH); // 4.4 s: the prompt waits
    let switch_at = 2.0 + 0.3 * 8.0;
    // No face yet: still unlocked, Face ID not turned on.
    let Some(status) = lock_screen_with_faces(Biometry::Enrolled, &taps, &[]) else { return };
    assert!(status.ends_with("\"Unlocked with PIN\""), "{status}");
    // A matching face turns it on; locked, a face that doesn't match fails the
    // button's prompt; tapped again, a matching face unlocks.
    taps.extend([LOCK_BUTTON, FACE_ID_BUTTON]); // 4.7, 5.0
    let faces = [(switch_at + 0.2, Face::Matching), (5.3, Face::NonMatching)];
    let status = lock_screen_with_faces(Biometry::Enrolled, &taps, &faces).unwrap();
    assert!(status.ends_with("\"Face ID failed: authentication_failed\""), "{status}");
    taps.push(FACE_ID_BUTTON); // 5.3 (after the face that didn't match, also at 5.3)
    let faces = [(switch_at + 0.2, Face::Matching), (5.25, Face::NonMatching), (5.6, Face::Matching)];
    let status = lock_screen_with_faces(Biometry::Enrolled, &taps, &faces).unwrap();
    assert!(status.ends_with("\"Unlocked with Face ID\""), "{status}");
}

/// Turning enrolment off while a prompt waits fails it: biometry isn't enrolled any more.
#[test]
fn lock_screen_unenrolled_while_asking() {
    use pomme_hle::keychain::{Biometry, Face};
    let mut taps = pin("1234");
    taps.extend(pin("1234"));
    taps.push(FACE_ID_SWITCH);
    let Some(status) = lock_screen_with_faces(Biometry::Enrolled, &taps, &[(4.8, Face::ToggleEnrolled)]) else { return };
    assert!(status.ends_with("\"Face ID failed: not_enrolled\""), "{status}");
}

/// A face that doesn't match can't turn Face ID on; without Face ID set up, the setting says so.
#[test]
fn lock_screen_face_id_refused() {
    use pomme_hle::keychain::Biometry;
    let mut taps = pin("1234");
    taps.extend(pin("1234"));
    taps.push(FACE_ID_SWITCH);
    let Some(status) = lock_screen(Biometry::NonMatching, &taps) else { return };
    assert!(status.ends_with("\"Face ID failed: authentication_failed\""), "{status}");
    let status = lock_screen(Biometry::NotEnrolled, &taps).unwrap();
    assert!(status.ends_with("\"Face ID isn't set up\""), "{status}");
}

/// Expo Router's native tabs (react-native-screens' Tabs, pommehello:///tabs):
/// the system's tab bar with the tabs' titles, symbols and badge, the first
/// tab's screen, its bottom accessory above the bar, and tapping a tab (as
/// VoiceOver would, by its name) shows its screen. On iOS 26.0 they're given as view controllers; from 26.1 as
/// `UITab`s, the search tab on a circle of its own (Liquid Glass).
#[test]
fn native_tabs() {
    use pomme_hle::driver::{self, text};
    for (minor, detached) in [(0, false), (2, true)] {
        let Some(mut emu) = boot() else { return };
        emu.system.os = pomme_hle::system::Version::new(26, minor, 0);
        emu.ui.launch_url = Some("pommehello:///tabs".into());
        let run = driver::drive(&mut emu, move |app| {
            app.wait_for(&text("Home tab"))?;
            // The bottom accessory, laid out on its glass above the bar (Liquid Glass: regular).
            let accessory = app.wait_for(&text("Now playing · regular"))?;
            assert!((accessory.frame[1] - 705.0).abs() < 1.0 && (accessory.frame[3] - 48.0).abs() < 1.0, "26.{minor}: {accessory}");
            let look = app.with(|emu| {
                let bar = pomme_hle::tabbar::bars(emu)[0];
                Ok(pomme_hle::tabbar::look(emu, bar)?.unwrap())
            })?;
            let titles: Vec<_> = look.buttons.iter().map(|b| b.title.as_str()).collect();
            assert_eq!(titles, ["Home", "Messages", "Settings", "Search"], "26.{minor}");
            assert!(look.buttons[0].selected);
            assert_eq!(look.buttons[1].badge.as_ref().map(|b| b.text.as_str()), Some("3"));
            assert_eq!(look.buttons[3].detached, detached, "26.{minor}: the search tab");
            assert_eq!(look.search_platter.is_some(), detached);
            app.tap(&text("Messages"))?;
            app.wait_for(&text("Messages tab"))?;
            Ok(())
        });
        if let Err(e) = run {
            panic!("26.{minor}: {e}");
        }
    }
}

/// Liquid Glass through Expo's expo-glass-effect (pommehello:///glass): it
/// finds the glass API (isLiquidGlassAvailable), and its GlassViews are
/// glass as iOS draws it (tests/glass.rs): over a black stripe regular glass
/// reads grey, over white near white; tinted glass is the tint.
#[test]
fn liquid_glass() {
    use pomme_hle::driver::{self, text};
    let Some(mut emu) = boot() else { return };
    if !Path::new(SYSROOT).join("System/Library/Frameworks/UIKit.framework/UIKit").exists() {
        eprintln!("skipping: build the sysroot's UIKit overlay first");
        return;
    }
    emu.ui.launch_url = Some("pommehello:///glass".into());
    let run = driver::drive(&mut emu, |app| {
        app.wait_for(&text("Liquid Glass available"))?;
        app.wait_for(&text("Tinted"))?;
        Ok(())
    });
    if let Err(e) = run {
        panic!("{e}");
    }
    let pix = pomme_hle::render::render(&mut emu, 3.0).unwrap();
    let at = |x: f64, y: f64| {
        let c = pix.pixel((x * 3.0) as u32, (y * 3.0) as u32).unwrap().demultiply();
        [c.red() as i32, c.green() as i32, c.blue() as i32]
    };
    let near = |got: [i32; 3], want: [i32; 3]| got.iter().zip(want).all(|(g, w)| (g - w).abs() <= 10);
    // Regular glass (from y 160.3), over the first black stripe and over white.
    assert!(near(at(32.0, 225.0), [175, 175, 174]), "over black: {:?}", at(32.0, 225.0));
    assert!(near(at(280.0, 225.0), [251, 251, 251]), "over white: {:?}", at(280.0, 225.0));
    // Tinted (#007aff, from y 360.3), over white.
    assert!(near(at(280.0, 425.0), [0, 121, 252]), "tinted: {:?}", at(280.0, 425.0));
}

/// Seven native tabs (pommehello:///moretabs): four on the bar and More,
/// whose list has the rest; choosing one there pushes its screen as UIKit
/// does (react-native-screens hooks the More navigation controller's
/// pushes, calling the original with objc_msgSendSuper). Both paths: view
/// controllers (26.0) and `UITab`s (26.2).
#[test]
fn native_tabs_more() {
    use pomme_hle::driver::{self, text};
    for minor in [0, 2] {
        let Some(mut emu) = boot() else { return };
        emu.system.os = pomme_hle::system::Version::new(26, minor, 0);
        emu.ui.launch_url = Some("pommehello:///moretabs".into());
        let run = driver::drive(&mut emu, |app| {
            app.wait_for(&text("One tab"))?;
            app.tap(&text("More"))?;
            app.wait_for(&text("Seven"))?;
            app.tap(&text("Five"))?;
            app.wait_for(&text("Five tab"))?;
            Ok(())
        });
        if let Err(e) = run {
            panic!("26.{minor}: {e}");
        }
    }
}

/// The localization screen's facts (`name value`, one per text), with the
/// phone set to `languages`, `region`, `zone` and the 24-hour switch.
fn localization_screen(languages: &[&str], region: &str, zone: &str, hours: Option<bool>) -> Option<std::collections::BTreeMap<String, String>> {
    localization_screen_with(languages, region, zone, hours, &[])
}

/// The same, launched with these arguments (`-key value` set defaults).
fn localization_screen_with(languages: &[&str], region: &str, zone: &str, hours: Option<bool>, args: &[&str]) -> Option<std::collections::BTreeMap<String, String>> {
    let mut emu = boot()?;
    emu.launch_args.extend(args.iter().map(|s| s.to_string()));
    emu.locales.settings = pomme_hle::locale::Settings {
        languages: languages.iter().map(|s| s.to_string()).collect(),
        region: region.into(),
        time_zone: Some(zone.into()),
        force_24_hour: hours,
    };
    emu.ui.launch_url = Some("pommehello:///localization".into());
    let v = run_for(&mut emu, 5.0);
    let mut facts = std::collections::BTreeMap::new();
    for text in v.split('"').skip(1).step_by(2) {
        if let Some((name, value)) = text.split_once(' ') {
            facts.insert(name.to_string(), unescape(value));
        }
    }
    Some(facts)
}

/// The view dump's `\u{202f}` escapes, as the characters.
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("\\u{") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 3..];
        match after.find('}').and_then(|end| u32::from_str_radix(&after[..end], 16).ok().and_then(char::from_u32).map(|c| (c, end))) {
            Some((c, end)) => {
                out.push(c);
                rest = &after[end + 1..];
            }
            None => {
                out.push_str("\\u{");
                rest = after;
            }
        }
    }
    out + rest
}

/// expo-localization, Hermes's Intl and Date, held to what the screen shows
/// on the iOS 26 Simulator (fixtures/expo-localization, recorded by
/// scripts/measure/expo-localization/run.sh).
#[test]
fn expo_localization_and_intl_follow_the_device() {
    let cases: [(&str, &[&str], &str, &str, Option<bool>); 4] = [
        ("en_US", &["en-US"], "US", "America/Los_Angeles", None),
        ("en_AU", &["en-AU"], "AU", "Australia/Sydney", None),
        ("ja_JP", &["ja-JP"], "JP", "Asia/Tokyo", None),
        ("en_US-24h", &["en-US"], "US", "America/Los_Angeles", Some(true)),
    ];
    let mut report = String::new();
    for (name, languages, region, zone, hours) in cases {
        let Some(ours) = localization_screen(languages, region, zone, hours) else { return };
        let path = format!("tests/fixtures/expo-localization/ios26-{name}.txt");
        let ios = std::fs::read_to_string(path).unwrap();
        for line in ios.lines().filter(|l| !l.starts_with('#')) {
            let (fact, want) = line.split_once(' ').unwrap_or((line, ""));
            match ours.get(fact) {
                Some(got) if got == want => {}
                got => report += &format!("{name}: {fact}: {got:?}, iOS {want:?}\n"),
            }
        }
    }
    assert!(report.is_empty(), "differences from iOS 26:\n{report}");
}

/// React Native's layout direction: hello-expo has an Arabic localization
/// (expo-localization's `supportedLocales`), so on an Arabic phone
/// `I18nManager.isRTL` is true, as React Native's own RCTI18nUtil decides
/// from the app's chosen localization (right to left allowed, as
/// expo-localization allows it by default); on an English phone, false.
#[test]
fn i18n_manager_follows_the_apps_localization() {
    let Some(ar) = localization_screen(&["ar"], "SA", "Asia/Riyadh", None) else { return };
    assert_eq!(ar.get("I18nManager.isRTL").map(String::as_str), Some("true"), "{ar:?}");
    assert_eq!(ar.get("textDirection").map(String::as_str), Some("rtl"), "{ar:?}");
    let Some(en) = localization_screen(&["en-US"], "US", "America/Los_Angeles", None) else { return };
    assert_eq!(en.get("I18nManager.isRTL").map(String::as_str), Some("false"), "{en:?}");
}

/// `I18nManager.forceRTL(true)` saves `RCTI18nUtil_forceRTL` in the app's
/// defaults, for the next launch: launched with it set (as a launch
/// argument stands in for what the last run saved; each pomme run starts
/// from a fresh install), an English phone lays the app out right to left;
/// `allowRTL(false)` keeps an Arabic one left to right.
#[test]
fn force_rtl_takes_effect_at_the_next_launch() {
    let Some(forced) = localization_screen_with(&["en-US"], "US", "America/Los_Angeles", None, &["-RCTI18nUtil_forceRTL", "YES"]) else { return };
    assert_eq!(forced.get("I18nManager.isRTL").map(String::as_str), Some("true"), "{forced:?}");
    let Some(denied) = localization_screen_with(&["ar"], "SA", "Asia/Riyadh", None, &["-RCTI18nUtil_allowRTL", "NO"]) else { return };
    assert_eq!(denied.get("I18nManager.isRTL").map(String::as_str), Some("false"), "{denied:?}");
}

/// Expo Router's `headerRight` (the notes screen's Clear): React Native's
/// view, held by a bar button item, in the navigation bar at its trailing
/// side; a tap on it reaches React (the notes empty).
#[test]
fn header_right_button() {
    let Some((_, views)) = notes_screen(|_| {}, 3.0) else { return };
    let lines: Vec<&str> = views.lines().collect();
    let bar = lines.iter().position(|l| l.trim_start().starts_with("RNSNavigationBar")).unwrap_or_else(|| panic!("no bar\n{views}"));
    let clear = lines.iter().position(|l| l.contains("\"Clear\"")).unwrap_or_else(|| panic!("no Clear\n{views}"));
    assert!(clear > bar, "Clear isn't in the bar\n{views}");
    // Its item view: in the bar's trailing half, on its row.
    let item = lines[bar + 1].trim_start();
    let nums: Vec<f64> = item.split(['(', ',', '×', ')']).filter_map(|t| t.trim().parse().ok()).collect();
    assert!(nums[0] > 195.0 && nums[0] + nums[2] <= 390.0 && nums[1] >= 0.0 && nums[1] + nums[3] <= 54.0, "{item}");
    let (x, y) = (nums[0] + nums[2] / 2.0, 47.0 + nums[1] + nums[3] / 2.0);

    let (_, after) = notes_screen(
        |emu| {
            tap(emu, NOTES, 3.0);
            type_text(emu, "Shopping", 4.0);
            tap(emu, (x, y), 5.0);
        },
        6.0,
    )
    .unwrap();
    assert!(after.contains("· 0 lines · 0 characters\""), "{after}");
    // (Without the tap, the note stays.)
    let (_, kept) = notes_screen(
        |emu| {
            tap(emu, NOTES, 3.0);
            type_text(emu, "Shopping", 4.0);
        },
        6.0,
    )
    .unwrap();
    assert!(kept.contains("· 1 lines · 8 characters\""), "{kept}");
}

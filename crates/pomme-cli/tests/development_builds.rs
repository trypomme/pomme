//! Development builds: what Debug builds' extra code needs from Foundation
//! and UIKit (tests/fixtures/devmisc, recorded on the iOS 26 Simulator:
//! NSCountedSet, +[NSThread callStackSymbols], NSCache's limits, window
//! levels). Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use std::collections::BTreeMap;

fn values(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("VALUE "))
        .map(|l| l.split_once(' ').map_or((l.to_string(), String::new()), |(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

#[test]
fn what_debug_builds_ask_of_foundation_and_uikit() {
    let Some(mut emu) = common::boot("devmisc") else { return };
    common::views(&mut emu, 1.0);
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/devmisc/ios26.txt")).unwrap();
    let (mut want, got) = (values(&recording), values(&emu.console));
    // On iOS the stack includes UIKit's own frames (dozens). pomme's UIKit is
    // pomme's code, not the app's, so only the app's frames are there: the
    // first one (the probe's method) is checked above, the count isn't.
    // (docs/differences-from-ios.md, "Call stacks".)
    want.remove("stackHasFrames");
    let differences: Vec<String> = want.iter().filter(|(k, v)| got.get(*k) != Some(*v)).map(|(k, v)| format!("{k}: iOS {v:?}, pomme {:?}", got.get(k))).collect();
    assert!(differences.is_empty(), "{}\n\nconsole:\n{}", differences.join("\n"), emu.console);
    // The alert-level window, created first, is drawn above the normal one (blue, not red).
    let pix = pomme_hle::render::render(&mut emu, 1.0).unwrap();
    let c = pix.pixel(pix.width() / 2, pix.height() / 2).unwrap();
    assert!(c.blue() > 200 && c.red() < 50, "the middle of the screen is {c:?}, not the alert window's blue");
}

/// NSURLComponents and NSURLQueryItem (tests/fixtures/urlcomponents): parsing,
/// building, and each part's percent-encoding, as on the iOS 26 Simulator.
#[test]
fn url_components_as_on_ios() {
    let Some(mut emu) = common::boot("urlcomponents") else { return };
    common::views(&mut emu, 1.0);
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/urlcomponents/ios26.txt")).unwrap();
    let (want, got) = (values(&recording), values(&emu.console));
    let differences: Vec<String> = want.iter().filter(|(k, v)| got.get(*k) != Some(*v)).map(|(k, v)| format!("{k}: iOS {v:?}, pomme {:?}", got.get(k))).collect();
    assert!(differences.is_empty(), "{}\n\nconsole:\n{}", differences.join("\n"), emu.console);
}

/// The system's proxy settings (tests/fixtures/proxies): none, as on the
/// iOS 26 Simulator with no proxy set. SocketRocket asks before a WebSocket.
#[test]
fn no_proxy_as_on_ios() {
    let Some(mut emu) = common::boot("proxies") else { return };
    common::views(&mut emu, 1.0);
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/proxies/ios26.txt")).unwrap();
    let (want, got) = (values(&recording), values(&emu.console));
    let differences: Vec<String> = want.iter().filter(|(k, v)| got.get(*k) != Some(*v)).map(|(k, v)| format!("{k}: iOS {v:?}, pomme {:?}", got.get(k))).collect();
    assert!(differences.is_empty(), "{}\n\nconsole:\n{}", differences.join("\n"), emu.console);
}

/// CFHTTPMessage (tests/fixtures/httpmessage): a WebSocket handshake's
/// request, serialized, and its response parsed as it arrives in pieces, as
/// on the iOS 26 Simulator. iOS writes the headers in a hashed order and
/// pomme in the order they were set, so the request is compared line by line,
/// in any order.
#[test]
fn http_messages_as_on_ios() {
    let Some(mut emu) = common::boot("httpmessage") else { return };
    common::views(&mut emu, 1.0);
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/httpmessage/ios26.txt")).unwrap();
    let (mut want, mut got) = (values(&recording), values(&emu.console));
    let lines = |s: Option<String>| {
        let mut v: Vec<String> = s.unwrap_or_default().split("\\r\\n").map(str::to_string).collect();
        v.sort();
        v
    };
    assert_eq!(lines(got.remove("request")), lines(want.remove("request")), "the request's lines");
    let differences: Vec<String> = want.iter().filter(|(k, v)| got.get(*k) != Some(*v)).map(|(k, v)| format!("{k}: iOS {v:?}, pomme {:?}", got.get(k))).collect();
    assert!(differences.is_empty(), "{}\n\nconsole:\n{}", differences.join("\n"), emu.console);
}

/// A Debug build of hello-expo (no bundle inside) loads its JavaScript from a
/// running Metro and shows its first screen, as on the iOS 26 Simulator
/// (tests/fixtures/devbuild). Needs the build and Metro, so it runs only when
/// `POMME_DEV_APP` names the Debug `.app` (built with
/// `scripts/build-app.sh hello-expo device debug`) and Metro is running
/// (`npx expo start` in that app's folder); otherwise it's skipped.
#[test]
fn a_debug_build_loads_from_metro() {
    let Some(app) = std::env::var_os("POMME_DEV_APP") else {
        eprintln!("skipped: set POMME_DEV_APP to a Debug build of hello-expo, with Metro running");
        return;
    };
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_pomme"))
        .arg("run")
        .arg(&app)
        .args(["--sysroot", "../../vendor/sysroot", "--steps", "40000000000", "--seconds", "120", "--views"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    for want in ["\"Hello from pomme 🍎\"", "\"Tapped 0 times\"", "\"Expo SDK: 11/11 passed\""] {
        assert!(text.contains(want), "no {want} on screen:\n{}", &text[text.len().saturating_sub(4000)..]);
    }
}

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

/// Tests that use Metro take turns: a reload (or a Fast Refresh) reaches
/// every app connected to it.
static METRO: std::sync::Mutex<()> = std::sync::Mutex::new(());

type Turn = Option<std::sync::MutexGuard<'static, ()>>;

/// The Debug build of hello-expo that `POMME_DEV_APP` names, loaded; with
/// `live`, on the real network with the phone's clock at now (TLS
/// certificates are checked against it), as `pomme run` does online, and
/// Metro to itself while the turn is held. None (the test skipped) without it.
fn debug_build(live: bool) -> Option<(std::path::PathBuf, pomme_hle::Emu, Turn)> {
    let Some(app) = std::env::var_os("POMME_DEV_APP").map(std::path::PathBuf::from) else {
        eprintln!("skipped: set POMME_DEV_APP to a Debug build of hello-expo{}", if live { ", with Metro running" } else { "" });
        return None;
    };
    let sysroot = std::path::Path::new("../../vendor/sysroot").canonicalize().unwrap();
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot) };
    let mut emu = pomme_hle::Emu::new(pomme_loader::load_with(&app, &opts).unwrap()).unwrap();
    let mut turn = None;
    if live {
        turn = Some(METRO.lock().unwrap_or_else(|e| e.into_inner()));
        *emu.net.client = Some(Box::new(pomme_net::LiveNet::new(Default::default())));
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        pomme_hle::libsystem::set_boot_time(&mut emu, now);
    }
    Some((app, emu, turn))
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

/// hello-expo as a Debug build against a running Metro, as on the iOS 26
/// Simulator (tests/fixtures/devbuild/notes.md): it loads its JavaScript from
/// Metro and shows its first screen; Fast Refresh changes the greeting and
/// keeps the tap count; Metro's reload starts the JavaScript over. Needs the
/// build and Metro, so it runs only when `POMME_DEV_APP` names the Debug
/// `.app` (`scripts/build-app.sh hello-expo device debug`) with Metro running
/// in its folder (`scripts/dev-build-test.sh` does both); else it's skipped.
#[test]
fn a_debug_build_with_metro() {
    use pomme_hle::driver::{drive, id, text};
    let Some((app, mut emu, _turn)) = debug_build(true) else { return };
    // The app's source folder: …/hello-expo/ios/build/Build/Products/Debug-iphoneos/helloexpo.app.
    let src = app.ancestors().nth(6).unwrap().to_path_buf();
    let home = src.join("components/Home.tsx");
    let original = std::fs::read_to_string(&home).unwrap();
    /// Puts the source back however the test ends.
    struct Restore(std::path::PathBuf, String);
    impl Drop for Restore {
        fn drop(&mut self) {
            let _ = std::fs::write(&self.0, &self.1);
        }
    }
    let _restore = Restore(home.clone(), original.clone());

    let started = std::time::Instant::now();
    drive(&mut emu, |app| {
        // (Hermes compiles Metro's unminified bundle: a long first wait.)
        app.timeout = 300.0;
        app.wait_for(&text("Hello from pomme 🍎"))?;
        eprintln!("loaded from Metro after {:?}", started.elapsed());
        app.tap(&id("tap-button"))?;
        app.tap(&id("tap-button"))?;
        app.wait_for(&text("Tapped 2 times"))?;

        // Fast Refresh: the new text, the same state.
        std::fs::write(&home, original.replace("Hello from pomme 🍎", "Hello again from pomme 🍏")).unwrap();
        app.wait_for(&text("Hello again from pomme 🍏"))?;
        assert!(app.find(&text("Tapped 2 times"))?.is_some(), "Fast Refresh lost the tap count:\n{}", app.describe()?);
        eprintln!("refreshed after {:?}", started.elapsed());

        // Reload (Metro's `r`): the JavaScript starts over, from the file as it is now.
        std::fs::write(&home, &original).unwrap();
        let reload = std::process::Command::new("node").arg("../../scripts/devbuild/reload.js").arg(&src).status().unwrap();
        assert!(reload.success(), "couldn't reach Metro to reload");
        app.wait_for(&text("Tapped 0 times"))?;
        app.wait_for(&text("Hello from pomme 🍎"))?;
        eprintln!("reloaded after {:?}", started.elapsed());
        Ok(())
    })
    .unwrap();
}

/// `-[UIView recursiveDescription]` (tests/fixtures/recursivedescription), as
/// on the iOS 26 Simulator: React Native's developer menu searches it for
/// "RCTView" on a shake.
#[test]
fn recursive_description_as_on_ios() {
    let Some(mut emu) = common::boot("recursivedescription") else { return };
    common::views(&mut emu, 1.0);
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/recursivedescription/ios26.txt")).unwrap();
    let (want, got) = (values(&recording), values(&emu.console));
    let differences: Vec<String> = want.iter().filter(|(k, v)| got.get(*k) != Some(*v)).map(|(k, v)| format!("{k}: iOS {v:?}, pomme {:?}", got.get(k))).collect();
    assert!(differences.is_empty(), "{}\n\nconsole:\n{}", differences.join("\n"), emu.console);
}

/// With no Metro, React Native's error screen (RedBox), as on the iOS 26
/// Simulator (tests/fixtures/devbuild/notes.md, `ios26-no-metro.png`): its
/// message, and the device build's four buttons. React Native catches its
/// own fatal error (`RCTFatal`'s @try/@catch) to get there. Needs the Debug
/// build (`POMME_DEV_APP`) but no Metro: pomme's network here is offline.
#[test]
fn a_debug_build_without_metro() {
    use pomme_hle::driver::{drive, id, text_containing};
    let Some((_, mut emu, _)) = debug_build(false) else { return };
    drive(&mut emu, |app| {
        let message = app.wait_for(&id("redbox-error"))?.label.unwrap_or_default();
        assert!(
            message.starts_with("No script URL provided. Make sure the packager is running or you have embedded a JS bundle in your application bundle."),
            "the error screen says {message:?}"
        );
        assert!(message.ends_with("unsanitizedScriptURLString = (null)"), "the error screen says {message:?}");
        for (button, title) in [("redbox-dismiss", "Dismiss"), ("redbox-reload", "Reload JS"), ("redbox-copy", "Copy"), ("redbox-extra", "Extra Info")] {
            let b = app.find(&id(button))?.unwrap_or_else(|| panic!("no {button}"));
            assert_eq!(b.label.as_deref(), Some(title));
        }
        assert!(app.find(&text_containing("Hello from pomme"))?.is_none());
        Ok(())
    })
    .unwrap();
}

/// React Native's dev menu, on Shake (as Device > Shake on the iOS 26
/// Simulator: tests/fixtures/devbuild/ios26-dev-menu.txt): an action sheet of
/// its choices, where the Simulator shows them; its Reload starts the
/// JavaScript over. Needs the Debug build and Metro (as
/// `a_debug_build_with_metro`).
#[test]
fn the_dev_menu_on_shake() {
    use pomme_hle::driver::{drive, id, text};
    let Some((_, mut emu, _turn)) = debug_build(true) else { return };
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/devbuild/ios26-dev-menu.txt")).unwrap();
    // The Simulator's buttons: (label, frame).
    let want: Vec<(String, Vec<f64>)> = recording
        .lines()
        .filter(|l| l.starts_with("Button "))
        .map(|l| {
            let (label, rest) = l["Button \"".len()..].split_once('"').unwrap();
            (label.to_string(), rest.split_whitespace().map(|v| v.parse().unwrap()).collect())
        })
        .collect();
    drive(&mut emu, |app| {
        app.timeout = 300.0;
        app.wait_for(&text("Hello from pomme 🍎"))?;
        app.tap(&id("tap-button"))?;
        app.wait_for(&text("Tapped 1 time"))?;
        app.shake()?;
        app.wait_for(&text("React Native Dev Menu"))?;
        app.run_for(1.0)?;
        let shown = app.elements()?;
        for (label, f) in &want {
            // (The button, not the title inside it.)
            let b = shown.iter().find(|e| e.traits & 1 != 0 && e.label.as_deref() == Some(label)).unwrap_or_else(|| panic!("no {label:?} button in the dev menu:\n{}", app.describe().unwrap()));
            let got = b.frame;
            assert!(got.iter().zip(f).all(|(a, b)| (a - b).abs() <= 0.5), "{label}: at {got:?}, iOS {f:?}");
        }
        if let Some(path) = std::env::var_os("POMME_DEV_MENU_SHOT") {
            std::fs::write(path, app.screenshot_png(3.0)?).unwrap();
        }
        app.tap(&text("Reload"))?;
        app.wait_for(&text("Tapped 0 times"))?;
        Ok(())
    })
    .unwrap();
}

/// React Native's own alerts (hello-expo's pommehello:///alerts screen,
/// recorded in tests/fixtures/hello-expo-alerts): Alert.alert and
/// ActionSheetIOS show their buttons where the Simulator does, and a choice
/// reaches the JavaScript. Needs the Debug build and Metro (the screen is
/// JavaScript only, served by Metro).
#[test]
fn react_natives_alerts() {
    use pomme_hle::driver::{drive, id, text};
    let Some((_, mut emu, _turn)) = debug_build(true) else { return };
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/hello-expo-alerts/ios26.txt")).unwrap();
    // A dialog's buttons as the Simulator showed them: (label, frame).
    let buttons = |dialog: &str| -> Vec<(String, Vec<f64>)> {
        recording
            .lines()
            .filter_map(|l| l.strip_prefix(&format!("{dialog} Button \"")))
            .map(|l| {
                let (label, rest) = l.split_once('"').unwrap();
                (label.to_string(), rest.split_whitespace().map(|v| v.parse().unwrap()).collect())
            })
            .collect()
    };
    let (alert, sheet) = (buttons("alert"), buttons("sheet"));
    drive(&mut emu, |app| {
        app.timeout = 300.0;
        app.wait_for(&text("Hello from pomme 🍎"))?;
        app.open_url("pommehello:///alerts")?;
        app.wait_for(&id("show-alert"))?;
        let check = |app: &mut pomme_hle::driver::App, want: &[(String, Vec<f64>)]| -> Result<(), pomme_hle::driver::Error> {
            let shown = app.elements()?;
            for (label, f) in want {
                let b = shown.iter().find(|e| e.traits & 1 != 0 && e.label.as_deref() == Some(label)).unwrap_or_else(|| panic!("no {label:?} button:\n{}", app.describe().unwrap()));
                assert!(b.frame.iter().zip(f).all(|(a, b)| (a - b).abs() <= 0.5), "{label}: at {:?}, iOS {f:?}", b.frame);
            }
            Ok(())
        };
        app.tap(&id("show-alert"))?;
        app.wait_for(&text("Delete the note?"))?;
        app.run_for(1.0)?;
        check(app, &alert)?;
        app.tap(&text("Delete"))?;
        app.wait_for(&text("Alert: Delete"))?;
        app.tap(&id("show-sheet"))?;
        app.wait_for(&text("Share the note"))?;
        app.run_for(1.0)?;
        check(app, &sheet)?;
        app.tap_at(195.0, 120.0)?;
        app.wait_for(&text("Sheet: Cancel"))?;
        Ok(())
    })
    .unwrap_or_else(|e| {
        let bt: Vec<String> = emu.backtrace(20).iter().map(|&a| emu.p.symbolize(a)).collect();
        panic!("{e:?}\n  at {}\n  {}\nconsole:\n{}", emu.p.symbolize(emu.cpu.pc), bt.join("\n  "), emu.console.lines().rev().take(20).collect::<Vec<_>>().join("\n"))
    });
}

/// Firebase in the Debug build (hello-expo's pommehello:///firebase screen,
/// against the real project): every line finishes as in the release build:
/// the app, analytics, crashlytics, an anonymous sign-in, and Firestore's
/// write read back. (A Debug build's rpaths also name Xcode's build folder;
/// a second copy of FirebaseCore from there had no app configured.) Needs the
/// Debug build, Metro and the network.
#[test]
fn firebase_in_a_debug_build() {
    use pomme_hle::driver::{drive, id, text, text_containing};
    let Some((_, mut emu, _turn)) = debug_build(true) else { return };
    drive(&mut emu, |app| {
        app.timeout = 300.0;
        app.wait_for(&text("Hello from pomme 🍎"))?;
        app.open_url("pommehello:///firebase")?;
        app.wait_for(&id("firestore"))?;
        app.wait_for(&text_containing("auth anonymous true"))?;
        app.wait_for(&text_containing("firestore cache hello · server same"))?;
        for line in ["app", "analytics", "crashlytics", "auth", "firestore"] {
            let e = app.find(&id(line))?.unwrap();
            let label = e.label.unwrap_or_default();
            assert!(!label.contains("error") && !label.ends_with('…'), "{line}: {label:?}\n{}", app.describe()?);
        }
        Ok(())
    })
    .unwrap();
}


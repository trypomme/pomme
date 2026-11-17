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
    let Some(app) = std::env::var_os("POMME_DEV_APP").map(std::path::PathBuf::from) else {
        eprintln!("skipped: set POMME_DEV_APP to a Debug build of hello-expo, with Metro running");
        return;
    };
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

    let sysroot = std::path::Path::new("../../vendor/sysroot").canonicalize().unwrap();
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot) };
    let mut emu = pomme_hle::Emu::new(pomme_loader::load_with(&app, &opts).unwrap()).unwrap();
    *emu.net.client = Some(Box::new(pomme_net::LiveNet::new(Default::default())));
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
    let Some(app) = std::env::var_os("POMME_DEV_APP").map(std::path::PathBuf::from) else {
        eprintln!("skipped: set POMME_DEV_APP to a Debug build of hello-expo");
        return;
    };
    let sysroot = std::path::Path::new("../../vendor/sysroot").canonicalize().unwrap();
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot) };
    let mut emu = pomme_hle::Emu::new(pomme_loader::load_with(&app, &opts).unwrap()).unwrap();
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

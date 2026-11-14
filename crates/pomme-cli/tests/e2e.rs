//! End-to-end tests the way XCUITest or Maestro write them: find elements
//! by accessibility identifier or text, tap and type, wait for the screen
//! to show something (pomme_hle::driver). One app per framework: React
//! Native (hello-expo), Flutter (hello-flutter) and SwiftUI (hello-swiftui).
//! Each is skipped unless that app and the sysroot have been built.

use pomme_hle::Emu;
use pomme_hle::driver::{self, App, Error, id, text, text_containing};
use std::path::Path;

const SYSROOT: &str = "../../vendor/sysroot";
const EXPO: &str = "../../apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app";

fn boot(app: &str, needs: &[&str]) -> Option<Emu> {
    let (app, sysroot) = (Path::new(app), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/CoreFoundation.framework").exists() || needs.iter().any(|n| !sysroot.join(n).exists()) {
        eprintln!("skipping: build {} and the sysroot first", app.display());
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    Some(Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap())
}

/// Drive the app; a failed step fails the test with what the screen showed.
fn drive(emu: &mut Emu, test: impl FnOnce(&mut App) -> Result<(), Error> + Send) {
    if let Err(e) = driver::drive(emu, test) {
        panic!("{e}");
    }
}

// ---- React Native ----

/// hello-expo's keyboard screen by `testID`: tap the TextInput, type a name,
/// and the greeting answers; return submits it.
#[test]
fn react_native_find_by_test_id_tap_and_type() {
    let Some(mut emu) = boot(EXPO, &[]) else { return };
    emu.ui.launch_url = Some("pommehello:///keyboard".into());
    drive(&mut emu, |app| {
        let greeting = app.wait_for(&id("greeting"))?;
        assert_eq!(greeting.label.as_deref(), Some("Type your name below"));
        let field = app.tap(&id("name"))?;
        assert_eq!(field.label.as_deref(), Some("Your name"), "{field}");
        app.wait_for(&text_containing("Editing ·"))?;
        app.type_text("Ada")?;
        app.wait_for(&text("Hello, Ada!"))?;
        let name = app.find(&id("name"))?.expect("the field");
        assert_eq!(name.value.as_deref(), Some("Ada"));
        app.type_text("\n")?;
        app.wait_for(&text_containing("submitted \"Ada\""))?;
        Ok(())
    });
}

// ---- Flutter ----

const FLUTTER: &str = "../../apps/hello-flutter/build/ios/iphoneos/Runner.app";

fn boot_flutter() -> Option<Emu> {
    boot(FLUTTER, &["pomme/impeller"])
}

/// hello-flutter through its semantics tree, which Flutter builds once
/// something asks its view for accessibility elements (as XCUITest does):
/// `Semantics(identifier:)` is the identifier, a Text's text the label.
#[test]
fn flutter_semantics_identifiers_tap_and_type() {
    let Some(mut emu) = boot_flutter() else { return };
    drive(&mut emu, |app| {
        app.wait_for(&text("Hello from Flutter"))?;
        let taps = app.wait_for(&id("taps"))?;
        assert_eq!(taps.label.as_deref(), Some("Tapped 0 times"));
        app.tap(&id("square"))?;
        app.tap(&id("square"))?;
        app.wait_for(&text("Tapped 2 times"))?;
        app.tap(&id("name"))?;
        app.type_text("Ada")?;
        let greeting = app.wait_for(&text("Hello, Ada!"))?;
        assert_eq!(greeting.identifier.as_deref(), Some("greeting"));
        let field = app.find(&id("name"))?.expect("the field");
        assert_eq!(field.value.as_deref(), Some("Ada"), "{field}");
        Ok(())
    });
}

// ---- SwiftUI ----

mod common;

/// A SwiftUI form on pomme's SwiftUI, by `.accessibilityIdentifier`: a
/// button counts, a toggle switches, a text field greets.
#[test]
fn swiftui_identifiers_button_toggle_field() {
    let Some(mut emu) = common::boot("swiftuie2e") else { return };
    drive(&mut emu, |app| {
        app.wait_for(&text("Count: 0"))?;
        app.tap(&id("add"))?;
        app.tap(&id("add"))?;
        app.wait_for(&text("Count: 2"))?;
        app.tap(&id("notify"))?;
        app.wait_for(&text("Notifications on"))?;
        app.tap(&id("name"))?;
        app.type_text("Grace")?;
        app.wait_for(&text("Hello, Grace!"))?;
        Ok(())
    });
}

// ---- Snapshots: boot once, start each case from the same point ----

/// Type a name into hello-expo's keyboard screen; the screen and when.
fn greet(app: &mut App, name: &str) -> Result<(Vec<u8>, f64), Error> {
    app.tap(&id("name"))?;
    app.type_text(name)?;
    app.wait_for(&text(&format!("Hello, {name}!")))?;
    Ok((app.screenshot_png(1.0)?, app.now()))
}

/// React Native: a checkpoint once the screen is up; each case starts from
/// it, and the same steps from it give the same screen at the same time.
#[test]
fn react_native_checkpoint_and_restore() {
    let Some(mut emu) = boot(EXPO, &[]) else { return };
    emu.ui.launch_url = Some("pommehello:///keyboard".into());
    drive(&mut emu, |app| {
        app.wait_for(&text("Type your name below"))?;
        let start = app.checkpoint()?;
        let first = greet(app, "Ada")?;

        // The same steps again from the checkpoint: the same screen, at the same
        // virtual time. (Looking at the screen runs app code too, which takes
        // virtual time, so nothing else happens in between.)
        let wall = std::time::Instant::now();
        app.restore(&start)?;
        eprintln!("restored in {:?}", wall.elapsed());
        assert_eq!(app.now(), start.at());
        let again = greet(app, "Ada")?;
        assert_eq!(again.1, first.1, "the same steps should take the same virtual time");
        assert!(again.0 == first.0, "the same steps should draw the same screen");

        // Back as it was: no name, no keyboard; then another case.
        app.restore(&start)?;
        app.wait_for(&text("Type your name below"))?;
        assert_eq!(app.find(&id("name"))?.expect("the field").value, None);
        assert!(app.find(&text("Hello, Ada!"))?.is_none());
        app.restore(&start)?;
        greet(app, "Grace")?;
        Ok(())
    });
}

/// Flutter, whose screen is drawn with Metal (textures on the GPU are
/// copied into the snapshot): the counter goes back, and so do the pixels.
#[test]
fn flutter_checkpoint_and_restore() {
    let Some(mut emu) = boot_flutter() else { return };
    drive(&mut emu, |app| {
        app.wait_for(&text("Tapped 0 times"))?;
        app.settle()?;
        let start = app.checkpoint()?;
        let before = app.screenshot_png(1.0)?;
        app.tap(&id("square"))?;
        app.tap(&id("square"))?;
        app.wait_for(&text("Tapped 2 times"))?;
        app.restore(&start)?;
        assert!(app.screenshot_png(1.0)? == before, "the restored screen should be the checkpoint's");
        assert_eq!(app.find(&id("taps"))?.expect("the counter").label.as_deref(), Some("Tapped 0 times"));
        app.tap(&id("square"))?;
        app.wait_for(&text("Tapped 1 time"))?;
        Ok(())
    });
}

/// SwiftUI: state (@State) goes back with the rest of memory.
#[test]
fn swiftui_checkpoint_and_restore() {
    let Some(mut emu) = common::boot("swiftuie2e") else { return };
    drive(&mut emu, |app| {
        app.wait_for(&text("Count: 0"))?;
        let start = app.checkpoint()?;
        app.tap(&id("add"))?;
        app.tap(&id("add"))?;
        app.wait_for(&text("Count: 2"))?;
        app.restore(&start)?;
        app.wait_for(&text("Count: 0"))?;
        app.tap(&id("add"))?;
        app.wait_for(&text("Count: 1"))?;
        Ok(())
    });
}

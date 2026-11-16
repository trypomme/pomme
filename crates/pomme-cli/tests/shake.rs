//! A shake (tests/fixtures/shake, recorded on the iOS 26 Simulator with
//! Device ▸ Shake): motionBegan then motionEnded, subtype shake, from the
//! first responder (the view controller) up to the window and the
//! application, and no further. Built with the iOS SDK; skipped without
//! Xcode or the sysroot.

mod common;

use pomme_hle::driver::drive;

fn last(text: &str, name: &str) -> Option<String> {
    text.lines().filter_map(|l| l.strip_prefix(&format!("VALUE {name} "))).last().map(str::to_string)
}

#[test]
fn a_shake_goes_up_the_chain_as_on_ios() {
    let Some(mut emu) = common::boot("shake") else { return };
    drive(&mut emu, |app| {
        app.run_for(1.0)?;
        app.shake()
    })
    .unwrap();
    let recording = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/shake/ios26.txt")).unwrap();
    for name in ["becameFirstResponder", "events"] {
        assert_eq!(last(&emu.console, name), last(&recording, name), "{name}:\n{}", emu.console);
    }
}

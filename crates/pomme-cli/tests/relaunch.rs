//! Relaunching the app within a run (tests/fixtures/relaunch/main.m, run
//! three times on the Simulator: launch counts 1, 2, 3, the file each last
//! launch wrote, `inMemory` 1 each time): the new launch finds what the last one saved (a default, a file in
//! Documents, a keychain item, the defaults file in Library/Preferences)
//! but none of its memory (a global variable starts over). Built with the
//! iOS SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::driver::drive;

fn values(console: &str, name: &str) -> Vec<String> {
    console.lines().filter_map(|l| l.strip_prefix(&format!("VALUE {name} ")).map(str::to_string)).collect()
}

#[test]
fn a_relaunch_keeps_what_the_app_saved() {
    let Some(mut emu) = common::boot("relaunch") else { return };
    let started = std::time::Instant::now();
    drive(&mut emu, |app| {
        app.run_for(0.5)?;
        let first = app.now();
        app.relaunch()?;
        // (Virtual time starts again.)
        assert!(app.now() <= first, "{} after {first}", app.now());
        app.run_for(0.5)?;
        app.relaunch()?;
        app.run_for(0.5)
    })
    .unwrap();
    eprintln!("three launches in {:?}", started.elapsed());
    let c = &emu.console;
    assert_eq!(values(c, "launch"), ["1", "2", "3"], "{c}");
    assert_eq!(values(c, "inMemory"), ["1", "1", "1"], "{c}");
    assert_eq!(values(c, "file"), ["(none)", "written at launch 1", "written at launch 2"], "{c}");
    // (The app synchronizes, so its file holds the launch it's in.)
    assert_eq!(values(c, "prefsFile"), ["1", "2", "3"], "{c}");
    // (On the Simulator the probe can't use the keychain, unsigned: -34018.
    // A phone keeps keychain items across launches, and reinstalls too.)
    assert_eq!(values(c, "keychain"), ["(none)", "secret", "secret"], "{c}");
    assert_eq!(values(c, "keychainAdd"), ["0"], "{c}");
}

/// The window's way (Device ▸ Relaunch App): the Relaunch event from the
/// host, under `run_main_relaunching`, as `pomme run --window` runs.
#[test]
fn the_windows_relaunch_event() {
    use pomme_hle::host::{Event, Frame, Host, Idle};
    /// Waits half a second (as a window waits on the wall clock), relaunches,
    /// waits half a second into the new launch and closes.
    struct Window {
        step: usize,
    }
    impl Host for Window {
        fn idle(&mut self, now_ns: u64, _: Option<u64>) -> Idle {
            self.step += 1;
            match self.step {
                1 | 3 => Idle::Input { at_ns: now_ns + 500_000_000, events: vec![] },
                2 => Idle::Input { at_ns: now_ns, events: vec![Event::Relaunch] },
                _ => Idle::Quit,
            }
        }
        fn frame(&mut self, _: Frame<'_>) {}
    }
    let Some(mut emu) = common::boot("relaunch") else { return };
    *emu.live.host = Some(Box::new(Window { step: 0 }));
    let _ = emu.run_main_relaunching();
    let c = &emu.console;
    assert_eq!(values(c, "launch"), ["1", "2"], "{c}");
    assert_eq!(values(c, "file"), ["(none)", "written at launch 1"], "{c}");
}

//! Opening a link in a running app (tests/fixtures/openurl and
//! openurlnoscene, recorded on the iOS 26 Simulator as ios26-self.txt: the
//! app opening its own link, which iOS delivers with no prompt). The test
//! opens the same link through the driver, as the window's File ▸ Open URL
//! does, and pomme must report what iOS did: the same callback, URL, options,
//! app state and lifecycle notifications (none). Links the app doesn't take
//! are refused. Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::driver::{Error, drive};
use std::collections::BTreeMap;

/// The probe's VALUE lines (the last of each name), as the Simulator script records them.
fn values(text: &str) -> BTreeMap<String, String> {
    text.lines()
        .filter_map(|l| l.strip_prefix("VALUE "))
        .map(|l| l.split_once(' ').map_or((l.to_string(), String::new()), |(k, v)| (k.to_string(), v.to_string())))
        .collect()
}

fn check(probe: &str, url: &str) {
    let Some(mut emu) = common::boot(probe) else { return };
    drive(&mut emu, |app| {
        app.run_for(1.0)?;
        app.open_url(url)?;
        // (The probe reports the notifications in the 2 s after the link.)
        app.run_for(2.5)
    })
    .unwrap();
    let recording = std::fs::read_to_string(format!("{}/tests/fixtures/{probe}/ios26-self.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let mut want = values(&recording);
    // (On the Simulator the app opened the link itself, and said so.)
    want.remove("selfOpenSucceeded");
    let got = values(&emu.console);
    let differences: Vec<String> = want.iter().filter(|(k, v)| got.get(*k) != Some(*v)).map(|(k, v)| format!("{k}: iOS {v:?}, pomme {:?}", got.get(k))).collect();
    assert!(differences.is_empty(), "{probe}:\n{}\n\nconsole:\n{}", differences.join("\n"), emu.console);
}

#[test]
fn a_scene_delegate_gets_the_link() {
    check("openurl", "pommeprobe://open/path?x=1");
}

#[test]
fn an_app_without_scenes_gets_the_link() {
    check("openurlnoscene", "pommeprobenoscene://open/path?x=1");
}

#[test]
fn links_the_app_does_not_take_are_refused() {
    let Some(mut emu) = common::boot("openurl") else { return };
    drive(&mut emu, |app| {
        app.run_for(1.0)?;
        for (link, why) in [
            ("https://example.com", "the app doesn't handle https: links"),
            ("pommeprobenoscene://x", "the app doesn't handle pommeprobenoscene: links"),
            ("not a link", "“not a link” isn't a URL"),
        ] {
            match app.open_url(link) {
                Err(Error::Failed(got)) => assert_eq!(got, why),
                other => panic!("{link}: {other:?}"),
            }
        }
        // The scheme matches whatever its case.
        app.open_url("PommeProbe://open")?;
        app.run_for(0.5)
    })
    .unwrap();
    assert_eq!(emu.console.matches("VALUE callback").count(), 1, "{}", emu.console);
}

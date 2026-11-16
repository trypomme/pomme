//! Right-to-left layout (tests/fixtures/rtl/main.m; fixtures/rtlplain is the
//! same app without an Arabic localization): the layout direction, and the
//! frames Auto Layout and stack views give, held to what the iOS 26
//! Simulator reported (fixtures/<probe>/ios26-<language>.txt, recorded with
//! scripts/swiftui/metrics-simulator.sh). Built with the iOS SDK; skipped
//! without Xcode or the sysroot.

mod common;

use std::collections::BTreeMap;

/// `VALUE name text` lines, each name's last.
fn values(text: &str) -> BTreeMap<String, String> {
    let mut v = BTreeMap::new();
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("VALUE ") else { continue };
        let (name, value) = rest.split_once(' ').unwrap_or((rest, ""));
        v.insert(name.to_string(), value.to_string());
    }
    v
}

/// What pomme reports for `probe` on a phone in `language`, against the recording.
fn differences(probe: &str, variant: &str, language: &str, region: &str) -> Option<Vec<String>> {
    let mut emu = common::boot(probe)?;
    emu.locales.settings.languages = vec![language.into()];
    emu.locales.settings.region = region.into();
    common::views(&mut emu, 2.0);
    if std::env::var_os("RTL_DEBUG").is_some() {
        eprintln!("{probe} {variant}:\n{}", emu.console);
    }
    let recording = std::fs::read_to_string(format!("{}/tests/fixtures/{probe}/ios26-{variant}.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let (ours, ios) = (values(&emu.console), values(&recording));
    let (our_frames, ios_frames) = (common::parse_metrics(&emu.console), common::parse_metrics(&recording));
    let mut wrong = Vec::new();
    for (name, want) in &ios {
        if ours.get(name) != Some(want) {
            wrong.push(format!("{name}: {:?}, iOS {want:?}", ours.get(name)));
        }
    }
    for (name, want) in &ios_frames {
        match our_frames.get(name) {
            None => wrong.push(format!("{name}: no frame (iOS {want:?})")),
            Some(got) if got.iter().zip(want).any(|(g, w)| (g - w).abs() > 0.5) => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
            _ => {}
        }
    }
    Some(wrong)
}

fn check(probe: &str, variant: &str, language: &str, region: &str) {
    let Some(wrong) = differences(probe, variant, language, region) else { return };
    assert!(wrong.is_empty(), "{probe} {variant}: differences from iOS 26:\n  {}", wrong.join("\n  "));
}

#[test]
fn arabic_app_on_an_arabic_phone() {
    check("rtl", "ar", "ar", "SA");
}

#[test]
fn arabic_app_on_an_english_phone() {
    check("rtl", "en", "en-US", "US");
}

/// An app without an Arabic localization stays left to right.
#[test]
fn english_only_app_on_an_arabic_phone() {
    check("rtlplain", "ar", "ar", "SA");
}

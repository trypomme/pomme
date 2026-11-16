//! The app's own translations (tests/fixtures/localized/main.m, with its
//! .lproj folders in resources/): which localization the bundle picks, how
//! CoreFoundation matches preferred languages with a set of localizations,
//! the strings, plurals and Info.plist values it reads, and where it finds a
//! resource. Held to what the iOS 26 Simulator reported
//! (fixtures/localized/ios26-<variant>.txt, recorded with
//! scripts/swiftui/metrics-simulator.sh). Built with the iOS SDK; skipped
//! without Xcode or the sysroot.

mod common;

use std::collections::BTreeMap;

/// Each recorded run: its variant, the preferred languages and the region.
const CASES: &[(&str, &[&str], &str)] = &[
    ("en_US", &["en-US"], "US"),
    ("ja_JP", &["ja-JP"], "JP"),
    ("en_AU", &["en-AU"], "AU"),
    ("de_DE", &["de-DE"], "DE"),
    ("fr_ja", &["fr-FR", "ja-JP"], "FR"),
];

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

#[test]
fn every_language_matches_ios() {
    let mut report = String::new();
    for &(variant, languages, region) in CASES {
        let Some(mut emu) = common::boot("localized") else { return };
        emu.locales.settings.languages = languages.iter().map(|s| s.to_string()).collect();
        emu.locales.settings.region = region.into();
        emu.time_limit_ns = Some(1_000_000_000);
        let stop = emu.run_main();
        if std::env::var_os("LOCALIZED_DEBUG").is_some() {
            eprintln!("{variant}: {stop:?}\n{}", emu.console);
        }
        let ours = values(&emu.console);
        let path = format!("{}/tests/fixtures/localized/ios26-{variant}.txt", env!("CARGO_MANIFEST_DIR"));
        let ios = values(&std::fs::read_to_string(path).unwrap());
        let mut wrong = Vec::new();
        for (name, want) in &ios {
            match ours.get(name) {
                None => wrong.push(format!("{name}: not reported (iOS: {want:?})")),
                Some(got) if got != want => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
                _ => {}
            }
        }
        if !wrong.is_empty() {
            report += &format!("{variant}:\n  {}\n", wrong.join("\n  "));
        }
    }
    assert!(report.is_empty(), "differences from iOS 26:\n{report}");
}

/// SwiftUI (tests/fixtures/swiftuilocalized): `String(localized:)` and
/// `NSLocalizedString` as recorded, and the Text views show the same
/// translations (found as XCUITest finds text), on an English and a Japanese
/// phone.
#[test]
fn swiftui_shows_the_translations() {
    use pomme_hle::driver::{self, text};
    for (variant, language, region) in [("en_US", "en-US", "US"), ("ja_JP", "ja-JP", "JP")] {
        let Some(mut emu) = common::boot("swiftuilocalized") else { return };
        emu.locales.settings.languages = vec![language.into()];
        emu.locales.settings.region = region.into();
        let path = format!("{}/tests/fixtures/swiftuilocalized/ios26-{variant}.txt", env!("CARGO_MANIFEST_DIR"));
        let ios = values(&std::fs::read_to_string(path).unwrap());
        let shown: Vec<String> = ["greeting", "items.3", "farewell"].iter().map(|n| ios[*n].clone()).collect();
        let mut wrong = Vec::new();
        if let Err(e) = driver::drive(&mut emu, move |app| {
            for t in &shown {
                app.wait_for(&text(t))?;
            }
            Ok(())
        }) {
            wrong.push(format!("the views: {e}"));
        }
        let ours = values(&emu.console);
        for (name, want) in &ios {
            if ours.get(name) != Some(want) {
                wrong.push(format!("{name}: {:?}, iOS {want:?}", ours.get(name)));
            }
        }
        assert!(wrong.is_empty(), "{variant}:\n  {}", wrong.join("\n  "));
    }
}

//! The device's language and region (tests/fixtures/locale/main.m): what
//! NSLocale, CFLocale, the calendar, the time zone and the formatters report
//! for a locale, held to what the iOS 26 Simulator reported for it
//! (fixtures/locale/ios26-<locale>.txt, recorded with
//! scripts/swiftui/metrics-simulator.sh). Built with the iOS SDK; skipped
//! without Xcode or the sysroot.

mod common;

use std::collections::BTreeMap;

/// One recorded run: the variant's file name, the languages, the region, the
/// time zone, and the 12/24-hour override.
struct Case {
    variant: &'static str,
    languages: &'static [&'static str],
    region: &'static str,
    zone: &'static str,
    hours: Option<bool>,
}

const CASES: &[Case] = &[
    Case { variant: "en_US", languages: &["en-US"], region: "US", zone: "America/Los_Angeles", hours: None },
    Case { variant: "en_AU", languages: &["en-AU"], region: "AU", zone: "Australia/Sydney", hours: None },
    Case { variant: "en_GB", languages: &["en-GB"], region: "GB", zone: "Europe/London", hours: None },
    Case { variant: "ja_JP", languages: &["ja-JP"], region: "JP", zone: "Asia/Tokyo", hours: None },
    Case { variant: "ar_SA", languages: &["ar-SA"], region: "SA", zone: "Asia/Riyadh", hours: None },
    Case { variant: "he_IL", languages: &["he-IL"], region: "IL", zone: "Asia/Jerusalem", hours: None },
    Case { variant: "fr_FR", languages: &["fr-FR"], region: "FR", zone: "Europe/Paris", hours: None },
    Case { variant: "de_DE", languages: &["de-DE"], region: "DE", zone: "Europe/Berlin", hours: None },
    Case { variant: "en_US-24h", languages: &["en-US"], region: "US", zone: "America/Los_Angeles", hours: Some(true) },
    Case { variant: "en_GB-12h", languages: &["en-GB"], region: "GB", zone: "Europe/London", hours: Some(false) },
];

/// Facts the probe records that pomme doesn't provide yet: NSMeasurementFormatter,
/// NSListFormatter, NSDateComponentsFormatter, NSByteCountFormatter and
/// locale-aware case mapping. The probe reports them last, so the rest is
/// still compared.
const NOT_YET: &[&str] = &["measure.", "list.", "duration.", "bytes.", "string."];

/// Known differences, by variant and name, with the reason.
const KNOWN: &[(&str, &str, &str)] = &[
    // iOS makes Saudi Arabia's device calendar the Islamic (Umm al-Qura) one;
    // pomme's NSCalendar is Gregorian (out of scope: openspec add-localization).
    // Formatters still use the Islamic calendar, through ICU.
    ("ar_SA", "calendar.identifier", "a non-Gregorian device calendar"),
    ("ar_SA", "calendar.fixed.weekOfYear", "a non-Gregorian device calendar"),
    // Apple's ICU data calls London's winter zone "GMT" in British English;
    // the open-source ICU, "Greenwich Mean Time".
    ("en_GB", "datetime.full", "Apple's ICU data"),
    ("en_GB", "time.full", "Apple's ICU data"),
    ("en_GB-12h", "datetime.full", "Apple's ICU data"),
    ("en_GB-12h", "time.full", "Apple's ICU data"),
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

/// What pomme reports for `case`, against the Simulator's recording.
fn differences(case: &Case) -> Option<Vec<String>> {
    let mut emu = common::boot("locale")?;
    emu.locales.settings = pomme_hle::locale::Settings {
        languages: case.languages.iter().map(|s| s.to_string()).collect(),
        region: case.region.into(),
        time_zone: Some(case.zone.into()),
        force_24_hour: case.hours,
    };
    emu.time_limit_ns = Some(1_000_000_000);
    let stop = emu.run_main();
    if std::env::var_os("LOCALE_DEBUG").is_some() {
        eprintln!("{}: {stop:?}\n{}", case.variant, emu.console);
    }
    let ours = values(&emu.console);
    let path = format!("{}/tests/fixtures/locale/ios26-{}.txt", env!("CARGO_MANIFEST_DIR"), case.variant);
    let ios = values(&std::fs::read_to_string(path).unwrap());
    let mut wrong = Vec::new();
    let known = |n: &str| KNOWN.iter().any(|&(v, k, _)| v == case.variant && k == n);
    for (name, want) in ios.iter().filter(|(n, _)| !NOT_YET.iter().any(|p| n.starts_with(p)) && !known(n)) {
        match ours.get(name) {
            None => wrong.push(format!("{name}: not reported (iOS: {want:?})")),
            Some(got) if got != want => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
            _ => {}
        }
    }
    Some(wrong)
}

#[test]
fn every_locale_matches_ios() {
    let mut report = String::new();
    for case in CASES {
        let Some(wrong) = differences(case) else { return };
        if !wrong.is_empty() {
            report += &format!("{}:\n  {}\n", case.variant, wrong.join("\n  "));
        }
    }
    assert!(report.is_empty(), "differences from iOS 26:\n{report}");
}

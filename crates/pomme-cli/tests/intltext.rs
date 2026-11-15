//! Text in other scripts (tests/fixtures/intltext/main.m): the fonts iOS
//! falls back to for Chinese, Japanese and Korean, held to what the iOS 26
//! Simulator reports (fixtures/intltext/ios26-<language>.txt, recorded with
//! scripts/swiftui/metrics-simulator.sh, VARIANT=en and VARIANT=ja): each
//! CJK character's advance, and where TextKit breaks a Japanese paragraph
//! at five widths. Latin letters are left out of the widths: pomme draws
//! Inter in place of SF Pro. Built with the iOS SDK; skipped without Xcode or
//! the sysroot.

mod common;

use std::collections::BTreeMap;

/// Characters iOS draws from a font other than its CJK fonts (Helvetica
/// Neue's ditto mark), which pomme draws from its CJK fonts.
const OTHER_FONTS: &[char] = &['\u{3003}'];

/// Lines that break one character earlier in pomme: they hold the Latin word
/// "Pomme", which is wider in Inter than in SF Pro, and iOS's line has less
/// than a point to spare (219.43 of 220, 298.95 of 300).
const LATIN_WIDTH: &[&str] = &["ja.220.line4", "ja.220.line5", "ja.300.line3", "ja.300.line4"];

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

fn is_cjk(ch: char) -> bool {
    matches!(ch, '\u{3000}'..='\u{9fff}' | '\u{ff00}'..='\u{ffef}' | '\u{ac00}'..='\u{d7a3}')
}

/// What the probe reports in pomme with `languages`, against the recording.
fn differences(variant: &str, languages: &[&str], region: &str) -> Option<Vec<String>> {
    let mut emu = common::boot("intltext")?;
    emu.locales.settings.languages = languages.iter().map(|s| s.to_string()).collect();
    emu.locales.settings.region = region.into();
    emu.time_limit_ns = Some(1_000_000_000);
    let stop = emu.run_main();
    if std::env::var_os("INTLTEXT_DEBUG").is_some() {
        eprintln!("{variant}: {stop:?}\n{}", emu.console);
    }
    let (ours, ours_metrics) = (values(&emu.console), common::parse_metrics(&emu.console));
    let recording = std::fs::read_to_string(format!("{}/tests/fixtures/intltext/ios26-{variant}.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let (ios, ios_metrics) = (values(&recording), common::parse_metrics(&recording));
    let mut wrong = Vec::new();
    for (name, want) in &ios {
        // Widths: CJK characters only.
        if let Some(code) = name.strip_prefix("width.U+") {
            let ch = char::from_u32(u32::from_str_radix(code, 16).unwrap()).unwrap();
            if !is_cjk(ch) || OTHER_FONTS.contains(&ch) {
                continue;
            }
        } else if variant == "ja" && LATIN_WIDTH.iter().any(|l| name.starts_with(l)) {
            continue;
        } else if !name.starts_with("ja.") {
            // (The cascade lists and font names are Apple's own.)
            continue;
        }
        match ours.get(name) {
            None => wrong.push(format!("{name}: not reported (iOS: {want:?})")),
            Some(got) if name.starts_with("width.") => {
                let (g, w): (f64, f64) = (got.parse().unwrap(), want.parse().unwrap());
                if (g - w).abs() > 0.01 {
                    wrong.push(format!("{name}: {got}, iOS {want}"));
                }
            }
            Some(got) if got != want => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
            _ => {}
        }
    }
    // Each line's place and height (not its width: Latin letters differ).
    for (name, want) in &ios_metrics {
        if variant == "ja" && LATIN_WIDTH.contains(&name.as_str()) {
            continue;
        }
        let Some(got) = ours_metrics.get(name) else {
            wrong.push(format!("{name}: no frame (iOS {want:?})"));
            continue;
        };
        for (i, what) in ["x", "y", "width", "height"].iter().enumerate() {
            let text = ios.get(&format!("{name}.text")).map_or("", String::as_str);
            if i == 2 && (name == "label" || text.chars().any(|c| c.is_ascii())) {
                continue;
            }
            if (got[i] - want[i]).abs() > 0.5 {
                wrong.push(format!("{name}: {what} {:.2}, iOS {:.2}", got[i], want[i]));
            }
        }
    }
    Some(wrong)
}

#[test]
fn japanese_on_a_japanese_phone() {
    let Some(wrong) = differences("ja", &["ja-JP"], "JP") else { return };
    assert!(wrong.is_empty(), "differences from iOS 26:\n  {}", wrong.join("\n  "));
}

#[test]
fn japanese_on_an_english_phone() {
    let Some(wrong) = differences("en", &["en-US"], "US") else { return };
    assert!(wrong.is_empty(), "differences from iOS 26:\n  {}", wrong.join("\n  "));
}

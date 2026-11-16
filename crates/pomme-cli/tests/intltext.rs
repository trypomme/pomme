//! Text in other scripts (tests/fixtures/intltext/main.m), on phones set to
//! five languages. pomme draws Chinese, Japanese and Korean with the Noto
//! Sans fonts at their own widths, as it draws Latin with Inter for SF Pro,
//! so it doesn't match iOS's widths or break lines at the same characters.
//! It does keep iOS's rules, which this checks:
//!
//! - the phone's language picks the font, as on iOS: the Noto font for the
//!   Apple font iOS gives each sample (fixtures/intltext/ios26-<language>.txt,
//!   recorded with scripts/swiftui/metrics-simulator.sh);
//! - CJK characters are set solid, a full em each;
//! - lines break by Unicode's rules (no `、。」` or small kana at a line's
//!   start, no `「` at its end), fill the width, and lose no text;
//! - TextKit and CoreText (UILabel) agree, and lines are as tall as on iOS.
//!
//! Built with the iOS SDK; skipped without Xcode or the sysroot.

mod cjk_lines;
mod common;

use cjk_lines::{is_cjk, SIZE, WIDTHS};

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

/// The Noto font pomme uses for an Apple CJK font.
fn noto_for(apple: &str) -> Option<&'static str> {
    if apple.contains("Hiragino") {
        Some("Noto Sans JP")
    } else if apple.contains("Gothic Neo") {
        Some("Noto Sans KR")
    } else if apple.ends_with(" SC") {
        Some("Noto Sans SC")
    } else if apple.ends_with(" TC") {
        Some("Noto Sans TC")
    } else if apple.contains("SF Arabic") {
        Some("Noto Sans Arabic")
    } else if apple.contains("SF Hebrew") {
        Some("Noto Sans Hebrew")
    } else if apple.contains("Thonburi") {
        Some("Noto Sans Thai")
    } else {
        None
    }
}

fn num(v: &BTreeMap<String, String>, name: &str) -> f64 {
    v.get(name).and_then(|s| s.parse().ok()).unwrap_or(f64::NAN)
}

/// A layout's lines (`ja` for TextKit's, `ct` for CoreText's) at `width`.
fn lines(v: &BTreeMap<String, String>, kind: &str, width: u32) -> Vec<(String, f64)> {
    let n = num(v, &format!("{kind}.{width}.lines")) as usize;
    (0..n)
        .map(|i| {
            let text = v.get(&format!("{kind}.{width}.line{i}.text")).cloned().unwrap_or_default();
            (text, num(v, &format!("{kind}.{width}.line{i}.width")))
        })
        .collect()
}

/// What breaks a rule in pomme on a phone set to `languages`.
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

    // The font for each sample: the Noto font for iOS's.
    for sample in ["hiragana", "katakana", "kanji", "hangul", "fullwidthComma", "ideographicStop", "arabic", "hebrew", "thai"] {
        let name = format!("font.{sample}");
        let Some(want) = ios.get(&name).and_then(|f| noto_for(f)) else { continue };
        let got = ours.get(&name).map_or("", String::as_str);
        if got != want {
            wrong.push(format!("{name}: {got:?}, want {want:?} (iOS {:?})", ios[&name]));
        }
    }

    // CJK characters: a full em each.
    for (name, got) in &ours {
        let Some(code) = name.strip_prefix("width.U+") else { continue };
        let ch = char::from_u32(u32::from_str_radix(code, 16).unwrap()).unwrap();
        let got: f64 = got.parse().unwrap();
        if is_cjk(ch) && (got - SIZE).abs() > 0.01 {
            wrong.push(format!("{name} ({ch}): {got:.3}, want {SIZE}"));
        }
    }

    for width in WIDTHS {
        let w = width as f64;
        let textkit = lines(&ours, "ja", width);
        let coretext = lines(&ours, "ct", width);
        if textkit.iter().map(|l| &l.0).ne(coretext.iter().map(|l| &l.0)) {
            wrong.push(format!("{width}: TextKit's lines {:?}, CoreText's {:?}", textkit.iter().map(|l| &l.0).collect::<Vec<_>>(), coretext.iter().map(|l| &l.0).collect::<Vec<_>>()));
        }
        wrong.extend(cjk_lines::faults(&format!("ct.{width}"), &coretext, w, 0.0));
        for (i, (text, line_width)) in coretext.iter().enumerate() {
            let name = format!("ct.{width}.line{i} {text:?}");
            // Set solid: an all-CJK line is a full em per character.
            if text.chars().all(is_cjk) && (line_width - SIZE * text.chars().count() as f64).abs() > 0.01 {
                wrong.push(format!("{name}: {line_width:.2} wide, not {} ems", text.chars().count()));
            }
        }
        // Each line's place and height, as on iOS (but on a Korean phone,
        // whose lines in Apple SD Gothic Neo are shorter: task 2.9).
        if variant != "ko" {
            for (i, _) in textkit.iter().enumerate() {
                let name = format!("ja.{width}.line{i}");
                let Some(got) = ours_metrics.get(&name) else {
                    wrong.push(format!("{name}: no frame"));
                    continue;
                };
                let height = ios_metrics.get(&format!("ja.{width}.line0")).map_or(f64::NAN, |m| m[3]);
                if (got[1] - i as f64 * height).abs() > 0.5 || (got[3] - height).abs() > 0.5 {
                    wrong.push(format!("{name}: y {:.2} height {:.2}, iOS's lines {height:.2} tall", got[1], got[3]));
                }
            }
        }
    }
    Some(wrong)
}

fn check(variant: &str, languages: &[&str], region: &str) {
    let Some(wrong) = differences(variant, languages, region) else { return };
    assert!(wrong.is_empty(), "{variant}:\n  {}", wrong.join("\n  "));
}

#[test]
fn japanese_on_a_japanese_phone() {
    check("ja", &["ja-JP"], "JP");
}

#[test]
fn japanese_on_a_korean_phone() {
    check("ko", &["ko-KR"], "KR");
}

#[test]
fn japanese_on_a_simplified_chinese_phone() {
    check("zh-Hans", &["zh-Hans-CN"], "CN");
}

#[test]
fn japanese_on_a_traditional_chinese_phone() {
    check("zh-Hant", &["zh-Hant-TW"], "TW");
}

#[test]
fn japanese_on_an_english_phone() {
    check("en", &["en-US"], "US");
}

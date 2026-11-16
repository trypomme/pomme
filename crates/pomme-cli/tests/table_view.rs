//! Table views (tests/fixtures/tableview/main.m), held to the iOS 26
//! Simulator (fixtures/tableview/ios26-<case>.txt, light): the rects the table
//! reports for rows, headers and footers; each visible cell's parts (labels
//! by their text, images, separators) and the header and footer labels; the
//! rows asked for, the content size and the colours. Text widths are held
//! loosely (Inter isn't SF Pro). Built with the iOS SDK; skipped without
//! Xcode or the sysroot.

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

/// A listed view: its frame and the rest of its line (text, colours).
type Part = ([f64; 4], String);

/// The `VIEW owner class x y w h rest` lines: owner → (labels by text, one-point lines, images).
fn parts(text: &str) -> BTreeMap<String, (BTreeMap<String, Part>, Vec<[f64; 4]>, Vec<[f64; 4]>)> {
    let mut out: BTreeMap<String, (BTreeMap<String, Part>, Vec<[f64; 4]>, Vec<[f64; 4]>)> = BTreeMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split(' ').collect();
        if f.len() < 7 || f[0] != "VIEW" {
            continue;
        }
        let r = [f[3], f[4], f[5], f[6]].map(|v| v.parse::<f64>().unwrap());
        let rest = f[7..].join(" ");
        let entry = out.entry(f[1].to_string()).or_default();
        let label_text = rest.split(" bg=").next().unwrap_or("").split(" font=").next().unwrap_or("").trim().to_string();
        if f[2].contains("Label") && !label_text.is_empty() {
            entry.0.insert(label_text, (r, rest));
        } else if (r[3] - 1.0).abs() < 0.01 && rest.contains("bg=") {
            entry.1.push(r);
        } else if f[2] == "UIImageView" && !entry.2.contains(&r) {
            entry.2.push(r);
        }
    }
    for e in out.values_mut() {
        e.1.sort_by(|a, b| a.partial_cmp(b).unwrap());
        e.2.sort_by(|a, b| a.partial_cmp(b).unwrap());
    }
    out
}

fn close(a: &[f64; 4], b: &[f64; 4], width: f64) -> bool {
    (a[0] - b[0]).abs() <= 0.5 && (a[1] - b[1]).abs() <= 0.5 && (a[2] - b[2]).abs() <= width && (a[3] - b[3]).abs() <= 0.5
}

/// What pomme reports for a case, against the recording.
fn differences(case: &str, taps: &[(f64, f64)]) -> Option<(Vec<String>, String)> {
    let mut emu = common::boot("tableview")?;
    pomme_hle::libsystem::set_env(&mut emu, "PROBE_VARIANT", case);
    for (i, &p) in taps.iter().enumerate() {
        let at = 2_000_000_000 + i as u64 * 500_000_000;
        pomme_hle::touch::swipe(&mut emu.touch.script, at, p, p, 50_000_000);
    }
    common::views(&mut emu, if case == "long" { 3.0 } else { 2.5 + taps.len() as f64 * 0.5 });
    let console = emu.console.clone();
    let recording = std::fs::read_to_string(format!("{}/tests/fixtures/tableview/ios26-{case}.txt", env!("CARGO_MANIFEST_DIR"))).unwrap();
    let mut wrong = Vec::new();
    let (ours, ios) = (values(&console), values(&recording));
    for (name, want) in &ios {
        if name.ends_with("distinctCells") {
            continue;
        }
        if ours.get(name) != Some(want) {
            wrong.push(format!("{name}: {:?}, iOS {want:?}", ours.get(name)));
        }
    }
    let (our_rects, ios_rects) = (common::parse_metrics(&console), common::parse_metrics(&recording));
    for (name, want) in &ios_rects {
        match our_rects.get(name) {
            Some(got) if close(got, want, 0.5) => {}
            got => wrong.push(format!("{name}: {got:?}, iOS {want:?}")),
        }
    }
    let (our_parts, ios_parts) = (parts(&console), parts(&recording));
    for (owner, (labels, lines, images)) in &ios_parts {
        let Some((our_labels, our_lines, our_images)) = our_parts.get(owner) else {
            wrong.push(format!("{owner}: not listed"));
            continue;
        };
        for (text, (r, rest)) in labels {
            match our_labels.get(text) {
                Some((got, our_rest)) => {
                    // (Placed by either edge: a right-aligned label's left edge moves with its text.)
                    let right = |a: &[f64; 4]| a[0] + a[2];
                    let by_right = (right(got) - right(r)).abs() <= 0.5 && close(&[r[0], got[1], got[2], got[3]], r, 3.0);
                    if !close(got, r, 3.0) && !by_right {
                        wrong.push(format!("{owner} \"{text}\": {got:?}, iOS {r:?}"));
                    }
                    // Font and colour as recorded.
                    let style = |s: &str| s.split(' ').filter(|w| w.starts_with("font=") || w.starts_with("color=")).collect::<Vec<_>>().join(" ");
                    if style(our_rest) != style(rest) {
                        wrong.push(format!("{owner} \"{text}\": {}, iOS {}", style(our_rest), style(rest)));
                    }
                }
                None => wrong.push(format!("{owner} \"{text}\": no label")),
            }
        }
        if lines.len() != our_lines.len() || lines.iter().zip(our_lines).any(|(a, b)| !close(a, b, 0.5)) {
            wrong.push(format!("{owner} separators: {our_lines:?}, iOS {lines:?}"));
        }
        if images.len() != our_images.len() || images.iter().zip(our_images).any(|(a, b)| !close(a, b, 0.5)) {
            wrong.push(format!("{owner} images: {our_images:?}, iOS {images:?}"));
        }
    }
    Some((wrong, console))
}

fn check(case: &str) {
    let Some((wrong, _)) = differences(case, &[]) else { return };
    assert!(wrong.is_empty(), "{case}: differences from iOS 26:\n  {}", wrong.join("\n  "));
}

#[test]
fn plain() {
    check("plain");
}

#[test]
fn grouped() {
    check("grouped");
}

#[test]
fn grouped_without_titles() {
    check("groupedBare");
}

#[test]
fn inset_grouped() {
    check("insetGrouped");
}

#[test]
fn cell_styles() {
    check("styles");
}

/// 100 rows: only the rows on screen are asked for, at the top and at the bottom.
#[test]
fn long_table() {
    check("long");
}

/// A tap on the second row selects it (the table at y 100, the row 102 pt down its content).
#[test]
fn tapping_a_row_selects_it() {
    let Some((_, console)) = differences("plain", &[(200.0, 100.0 + 102.0 + 26.0)]) else { return };
    assert_eq!(values(&console).get("didSelect").map(String::as_str), Some("0 1"), "{console}");
}

//! End to end: a native UIKit app with text views (tests/fixtures/textview/main.m):
//! typing several lines, return, the delegate, scrolling to the caret, and sizing.
//! Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

use pomme_hle::Emu;
use pomme_hle::textinput::KeyInput;

/// The text of the view of class `cls` tagged `tag`.
fn text(views: &str, cls: &str, tag: u64) -> String {
    let line = views.lines().find(|l| l.trim_start().starts_with(&format!("{cls} (")) && l.contains(&format!(" tag {tag} "))).unwrap_or_else(|| panic!("no {cls} {tag} in\n{views}"));
    line.split_once(" \"").map(|(_, t)| t.trim_end_matches('"').to_string()).unwrap_or_default()
}

fn run(typing: &[(f64, &str)], secs: f64) -> Option<(Emu, String)> {
    let mut emu = common::boot("textview")?;
    for &(at, t) in typing {
        emu.input.script.push(KeyInput { at_ns: (at * 1e9) as u64, text: t.into() });
    }
    let v = common::views(&mut emu, secs);
    Some((emu, v))
}

/// Typing goes into the focused text view; return starts a new line; the
/// delegate can refuse a change; once the text is taller than the view, it
/// scrolls to keep the caret in sight.
#[test]
fn typing_lines() {
    let Some((emu, v)) = run(&[(1.0, "Line one\n"), (1.5, "Line #two\nthree\nfour\nfive")], 3.0) else { return };
    assert_eq!(text(&v, "UITextView", 1), r"Line one\nLine two\nthree\nfour\nfive", "{v}");
    // 34 keys, one refused ('#'): 33 changes, each moving the selection first.
    assert_eq!(text(&v, "UITextField", 100), "begin 1 change 33 selection 33 end 0 at 33", "{v}");
    // Five 17 pt lines (20.3 pt each) and the insets: 117 pt, in 100: scrolled by 17.
    assert_eq!(text(&v, "UITextField", 101), "content 117 offset 17 scrolls 1", "{v}");
    assert!(emu.input.keyboard_shown);
}

/// A view that isn't editable doesn't take focus; sizeThatFits: measures
/// the lines and the insets.
#[test]
fn sizing_and_read_only() {
    let Some((_, v)) = run(&[], 2.0) else { return };
    // Two lines of 17 pt (40.6) and 4 + 4 inset.
    assert_eq!(text(&v, "UITextField", 102), "fit 200x49 inset 4,6 padding 5 read-only 0", "{v}");
    assert_eq!(text(&v, "UITextView", 3), r"One\nTwo", "{v}");
    assert_eq!(text(&v, "UITextField", 100), "begin 1 change 0 selection 0 end 0 at 0", "{v}");
}

/// Return at the end of the text leaves the caret on a new, empty line: the
/// content grows by a line.
#[test]
fn return_adds_a_line() {
    let Some((_, v)) = run(&[(1.0, "a\n")], 2.0) else { return };
    assert_eq!(text(&v, "UITextView", 1), r"a\n", "{v}");
    // 8 + 2 × 20.3 + 8.
    assert_eq!(text(&v, "UITextField", 101), "content 57 offset 0 scrolls 0", "{v}");
}

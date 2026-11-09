//! End to end: a native UIKit app (tests/fixtures/clipboard/main.m) that
//! reads, writes and cuts, and takes a paste. Built with the iOS SDK; skipped
//! without Xcode or the sysroot.

mod common;

use pomme_hle::Emu;
use pomme_hle::clipboard::{Permission, PasteInput};

/// The text of the text field tagged `tag`.
fn field(views: &str, tag: u64) -> String {
    let line = views.lines().find(|l| l.trim_start().starts_with("UITextField (") && l.contains(&format!(" tag {tag} "))).unwrap_or_else(|| panic!("no field {tag} in\n{views}"));
    line.split_once(" \"").map(|(_, t)| t.trim_end_matches('"').to_string()).unwrap_or_default()
}

fn run(start: Option<&str>, permission: Permission, paste: Option<&str>) -> Option<(Emu, String)> {
    let mut emu = common::boot("clipboard")?;
    emu.clipboard.set_text(start);
    emu.clipboard.permission = permission;
    if let Some(p) = paste {
        emu.clipboard.script.push(PasteInput { at_ns: 2_000_000_000, text: Some(p.into()) });
    }
    let v = common::views(&mut emu, 5.0);
    Some((emu, v))
}

/// Another app's text: reading it asks ("Allow Paste"), and Allow lets it
/// through. The app's own copies and cuts don't ask, change the change
/// count, and land on the clipboard. A paste the user chose goes into the
/// focused field, and reading right after it doesn't ask; later it does.
#[test]
fn reads_writes_and_pastes() {
    let Some((mut emu, v)) = run(Some("from another app"), Permission::Allow, Some("https://expo.dev")) else { return };
    assert_eq!(field(&v, 100), "has 1: from another app", "{v}");
    assert_eq!(field(&v, 101), "copied in the app (+1)", "{v}");
    assert_eq!(field(&v, 102), "", "the cut text should be gone\n{v}");
    assert_eq!(field(&v, 103), "https://expo.dev", "the paste\n{v}");
    assert_eq!(field(&v, 104), "https://expo.dev", "{v}");
    assert_eq!(field(&v, 105), "https://expo.dev", "{v}");
    assert_eq!(field(&v, 106), "https://expo.dev", "{v}");
    // Asked at launch and twice at 4 s (string, URL); not at 2.5 s, just after the user pasted.
    assert_eq!(emu.clipboard.prompts, 3, "{v}");
    assert_eq!(emu.clipboard.text().as_deref(), Some("https://expo.dev"));
}

/// "Don't Allow": the app's reads of another app's text get nothing; its
/// own copies, and the paste the user chose, still work.
#[test]
fn denied() {
    let Some((mut emu, v)) = run(Some("secret"), Permission::Deny, Some("pasted")) else { return };
    assert_eq!(field(&v, 100), "has 1: (nil)", "hasStrings doesn't ask; string is denied\n{v}");
    assert_eq!(field(&v, 101), "copied in the app (+1)", "{v}");
    assert_eq!(field(&v, 103), "pasted", "{v}");
    assert_eq!(field(&v, 104), "pasted", "{v}");
    assert_eq!(field(&v, 105), "(nil)", "{v}");
    assert_eq!(emu.clipboard.prompts, 3);
    assert_eq!(emu.clipboard.text().as_deref(), Some("pasted"));
}

/// Nothing on the clipboard: nothing to ask about; the app's cut is what's left.
#[test]
fn empty() {
    let Some((mut emu, v)) = run(None, Permission::Allow, None) else { return };
    assert_eq!(field(&v, 100), "has 0: (nil)", "{v}");
    assert_eq!(field(&v, 103), "", "{v}");
    assert_eq!(field(&v, 105), "cut me", "the app's own cut, read back without asking\n{v}");
    assert_eq!(emu.clipboard.prompts, 0);
    assert_eq!(emu.clipboard.text().as_deref(), Some("cut me"));
}

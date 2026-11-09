//! Foundation collections compare by content (tests/fixtures/collections/main.m):
//! equal dictionaries and sets are isEqual: and hash alike, as Foundation's
//! are. (React Native compares dimension dictionaries to decide whether to
//! tell JavaScript the window changed.) Built with the iOS SDK; skipped
//! without Xcode or the sysroot.

mod common;

#[test]
fn equal_by_content() {
    let Some(mut emu) = common::boot("collections") else { return };
    let v = common::views(&mut emu, 1.0);
    let line = v.lines().find(|l| l.contains(" tag 1 ")).unwrap_or_else(|| panic!("{v}"));
    let expected = "dict=1 dictHash=1 mutable=1 empty=1 differs=0 notDict=0 set=1 setHash=1 setDiffers=0 dictInSet=1";
    assert!(line.ends_with(&format!("\"{expected}\"")), "{line}");
}

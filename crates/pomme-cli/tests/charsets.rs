//! NSCharacterSet and NSMutableCharacterSet (tests/fixtures/charsets/main.m):
//! the factories sent to NSMutableCharacterSet give a set of one's own to
//! change, as Firebase's startup relies on, and the mutators change it.
//! Built with the iOS SDK; skipped without Xcode or the sysroot.

mod common;

#[test]
fn mutable_character_sets() {
    let Some(mut emu) = common::boot("charsets") else { return };
    let v = common::views(&mut emu, 1.0);
    let line = v.lines().find(|l| l.contains(" tag 1 ")).unwrap_or_else(|| panic!("{v}"));
    let expected = "added=1 name=1 badName=1 sharedUntouched=1 range=1 intersect=1 union=1 invert=1";
    assert!(line.ends_with(&format!("\"{expected}\"")), "{line}");
}

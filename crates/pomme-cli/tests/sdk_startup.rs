//! What SDKs such as Firebase do as an app launches
//! (tests/fixtures/sdkstartup/main.m): read their plist, check IDs with
//! NSScanner, take dates apart with NSCalendar, ask sysctl for the model and
//! whether a debugger is attached, and name protocols. Built with the iOS
//! SDK; skipped without Xcode or the sysroot.

mod common;

#[test]
fn sdk_startup() {
    let Some(mut emu) = common::boot("sdkstartup") else { return };
    let v = common::views(&mut emu, 1.0);
    let line = v.lines().find(|l| l.contains(" tag 1 ")).unwrap_or_else(|| panic!("{v}"));
    let failed: Vec<&str> = line.split(['"', ' ']).filter(|c| c.ends_with("=0")).collect();
    assert!(failed.is_empty(), "failed checks: {failed:?}\n{line}");
    assert!(line.contains("underlyingQueue=1"), "{line}");
}

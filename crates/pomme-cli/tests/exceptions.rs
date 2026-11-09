//! End to end: C++ exceptions in a native app (tests/fixtures/exceptions/main.mm),
//! unwound by libunwind from the sysroot: caught several frames up with
//! destructors run, rethrown, and matched by type. Built with the iOS SDK;
//! skipped without Xcode or the sysroot.

mod common;

/// The text of the text field tagged `tag`.
fn field(views: &str, tag: u64) -> String {
    let line = views.lines().find(|l| l.trim_start().starts_with("UITextField (") && l.contains(&format!(" tag {tag} "))).unwrap_or_else(|| panic!("no field {tag} in\n{views}"));
    line.split_once(" \"").map(|(_, t)| t.trim_end_matches('"').to_string()).unwrap_or_default()
}

#[test]
fn throw_and_catch() {
    let Some(mut emu) = common::boot("exceptions") else { return };
    let v = common::views(&mut emu, 2.0);
    // deep(3) recurses to deep(0), which throws: four frames, four destructors.
    assert_eq!(field(&v, 100), "caught from deep down after dddd", "{v}");
    assert_eq!(field(&v, 101), "rethrown first after r", "{v}");
    assert_eq!(field(&v, 102), "int 42", "{v}");
    assert_eq!(field(&v, 103), "done", "{v}");
}

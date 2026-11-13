//! The C runtime under an app (tests/fixtures/runtime/main.m): atomics across
//! threads, sscanf, calendar time and strftime, C strings, string encodings, CFPreferences, the launch
//! arguments, environment and argument domain, and dispatch sources. Built with the iOS SDK;
//! skipped without Xcode or the sysroot.

mod common;

#[test]
fn runtime() {
    let Some(mut emu) = common::boot("runtime") else { return };
    pomme_hle::libsystem::set_boot_time(&mut emu, 1_794_553_712); // 2026-11-13T09:48:32Z
    pomme_hle::libsystem::set_env(&mut emu, "POMME_GREETING", "hello");
    emu.launch_args = vec!["-PommeFlag".into(), "-PommeLevel".into(), "3".into()];
    let v = common::views(&mut emu, 2.0);
    let line = v.lines().find(|l| l.contains(" tag 1 ")).unwrap_or_else(|| panic!("{v}"));
    let expected = "atomics=1 sscanf=1 time=1 strftimeFits=1 strings=1 encodings=1 preferences=1 arguments=1 argumentDomain=1 environment=1 sources=1";
    assert!(line.ends_with(&format!("\"{expected}\"")), "{line}");
}

//! The C runtime under an app (tests/fixtures/runtime/main.m): atomics across
//! threads, sscanf, calendar time and strftime, C strings, string encodings,
//! CFPreferences, the launch arguments, environment and argument domain,
//! dispatch sources, ordered sets, Mach threads and exception ports, file
//! errors, download tasks, and toll-free bridging. Built with the iOS SDK; skipped without Xcode
//! or the sysroot.

mod common;

use pomme_hle::net::{Fixtures, Response};

#[test]
fn runtime() {
    let Some(mut emu) = common::boot("runtime") else { return };
    pomme_hle::libsystem::set_boot_time(&mut emu, 1_794_553_712); // 2026-11-13T09:48:32Z
    pomme_hle::libsystem::set_env(&mut emu, "POMME_GREETING", "hello");
    emu.launch_args = vec!["-PommeFlag".into(), "-PommeLevel".into(), "3".into()];
    // HTTP/2 sends header names in lowercase.
    let headers = vec![("content-type".into(), "text/plain".into()), ("x-pomme-id".into(), "7".into())];
    let hello = Response { status: 200, headers, body: b"hello".to_vec() };
    emu.net.client = Some(Box::new(Fixtures(vec![("https://pomme.test/hello.txt".into(), hello)])));
    let v = common::views(&mut emu, 2.0);
    let line = v.lines().find(|l| l.contains(" tag 1 ")).unwrap_or_else(|| panic!("{v}"));
    let checks = "atomics=1 sscanf=1 time=1 strftimeFits=1 strings=1 encodings=1 preferences=1 selfImage=1 orderedSet=1 threads=1 exceptionPorts=1 errors=1 tollFree=1 arguments=1 argumentDomain=1 environment=1";
    let text = line.rsplit_once(" \"").map(|(_, t)| t.trim_end_matches('"')).unwrap_or_default();
    let mut got: Vec<&str> = text.split(' ').collect();
    let mut want: Vec<&str> = checks.split(' ').chain(["sources=1", "download=1"]).collect();
    got.sort();
    want.sort();
    assert_eq!(got, want, "{line}");
}

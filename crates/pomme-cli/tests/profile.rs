//! The guest profiler (tests/fixtures/profile/main.m): the hot function is
//! found with its callers, across pomme's own functions in between (UIKit
//! calls viewDidLoad), the speedscope file is well formed, and the same run
//! profiles the same way every time. Built with the iOS SDK; skipped without
//! Xcode or the sysroot.

mod common;

fn profile() -> Option<(String, String)> {
    let mut emu = common::boot("profile")?;
    emu.start_profiling(1000);
    common::views(&mut emu, 1.0);
    let p = emu.profiler.take().unwrap();
    Some((p.speedscope("profile", |_| "main".into()), p.summary(5)))
}

#[test]
fn finds_the_hot_function_and_its_callers() {
    let Some((json, summary)) = profile() else { return };
    // spin() runs most of the instructions, itself.
    let own = summary.lines().nth(1).unwrap();
    assert!(own.contains("spin  (Profile)") || own.contains("spin  ("), "{summary}");

    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["$schema"], "https://www.speedscope.app/file-format-schema.json");
    let frames: Vec<&str> = v["shared"]["frames"].as_array().unwrap().iter().map(|f| f["name"].as_str().unwrap()).collect();
    let main = &v["profiles"][0];
    assert_eq!(main["type"], "sampled");
    assert_eq!(main["samples"].as_array().unwrap().len(), main["weights"].as_array().unwrap().len());
    // A sample in spin: root first, it's called by viewDidLoad, which pomme
    // called (a host frame), from the app's main.
    let stack = main["samples"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_array().unwrap().iter().map(|i| frames[i.as_u64().unwrap() as usize]).collect::<Vec<_>>())
        .find(|s| s.last() == Some(&"spin"))
        .expect("a sample in spin");
    let at = |name: &str| stack.iter().position(|f| *f == name).unwrap_or_else(|| panic!("{name} not in {stack:?}"));
    assert!(at("main") < at("-[ViewController viewDidLoad]"), "{stack:?}");
    assert_eq!(at("-[ViewController viewDidLoad]") + 1, at("spin"), "{stack:?}");
    // Between main and viewDidLoad: pomme's UIApplicationMain.
    assert!(stack[at("main")..at("-[ViewController viewDidLoad]")].contains(&"UIApplicationMain"), "{stack:?}");
}

#[test]
fn same_run_same_profile() {
    let Some((a, _)) = profile() else { return };
    let (b, _) = profile().unwrap();
    assert_eq!(a, b);
}

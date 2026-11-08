//! End-to-end loader tests against tiny iOS binaries built from `tests/fixtures/*.c`.
//!
//! Each fixture is built twice: with chained fixups (the modern default) and with
//! classic dyld_info opcodes. Needs Xcode with the iOS SDK; skipped otherwise.

use pomme_loader::{Process, STUB_INSN, load};
use std::path::{Path, PathBuf};
use std::process::Command;

fn have_ios_sdk() -> bool {
    Command::new("xcrun")
        .args(["--sdk", "iphoneos", "--show-sdk-path"])
        .output()
        .is_ok_and(|o| o.status.success())
}

fn cc(args: &[&str]) {
    let out = Command::new("xcrun")
        .args(["--sdk", "iphoneos", "clang", "-target", "arm64-apple-ios16.0", "-O1"])
        .args(args)
        .output()
        .expect("run clang");
    assert!(out.status.success(), "clang {args:?} failed:\n{}", String::from_utf8_lossy(&out.stderr));
}

/// Build Fixture.app; returns its path.
fn build_fixture(variant: &str, extra: &[&str]) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("fixture-{variant}"));
    let _ = std::fs::remove_dir_all(&root);
    let app = root.join("Fixture.app");
    let fw = app.join("Frameworks");
    let unshipped = root.join("unshipped");
    std::fs::create_dir_all(&fw).unwrap();
    std::fs::create_dir_all(&unshipped).unwrap();
    let s = |p: &Path| p.to_str().unwrap().to_string();
    let f = |n: &str| s(&src.join(n));

    let args = |a: Vec<String>| {
        let mut all: Vec<&str> = a.iter().map(String::as_str).collect();
        all.extend_from_slice(extra);
        cc(&all);
    };
    args(vec![
        "-dynamiclib".into(), f("inner.c"), "-o".into(), s(&fw.join("libinner.dylib")),
        "-install_name".into(), "@rpath/libinner.dylib".into(),
    ]);
    args(vec![
        "-dynamiclib".into(), f("lib.c"), "-o".into(), s(&fw.join("libfix.dylib")),
        "-install_name".into(), "@rpath/libfix.dylib".into(),
        format!("-Wl,-reexport_library,{}", s(&fw.join("libinner.dylib"))),
    ]);
    args(vec![
        "-dynamiclib".into(), f("weak.c"), "-o".into(), s(&unshipped.join("libweak.dylib")),
        "-install_name".into(), "@rpath/libweak.dylib".into(),
    ]);
    args(vec![
        f("main.c"), "-o".into(), s(&app.join("fixture")),
        s(&fw.join("libfix.dylib")),
        // Tell ld where libfix's re-exported @rpath/libinner.dylib is at link time.
        format!("-Wl,-dylib_file,@rpath/libinner.dylib:{}", s(&fw.join("libinner.dylib"))),
        format!("-Wl,-weak_library,{}", s(&unshipped.join("libweak.dylib"))),
        "-Wl,-rpath,@executable_path/Frameworks".into(),
    ]);

    std::fs::write(
        app.join("Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleExecutable</key><string>fixture</string>
  <key>CFBundleIdentifier</key><string>dev.pomme.fixture</string>
</dict></plist>"#,
    )
    .unwrap();
    app
}

fn sym(p: &Process, image: &str, name: &str) -> u64 {
    p.images
        .iter()
        .find(|i| i.name == image)
        .unwrap_or_else(|| panic!("no image {image}"))
        .symbol(name)
        .unwrap_or_else(|| panic!("{image} does not export {name}"))
}

fn check(app: &Path) {
    let p = load(app).expect("load");
    let names: Vec<&str> = p.images.iter().map(|i| i.name.as_str()).collect();
    assert_eq!(names, ["fixture", "libfix.dylib", "libinner.dylib"], "libweak must not be loaded");
    assert!(p.unresolved.is_empty(), "unresolved: {:?}", p.unresolved);
    assert_eq!(p.stats.rebases_unmapped, 0);

    let m = &p.memory;
    let deref = |addr: u64| m.read_u64(addr).unwrap();

    // Rebase within the executable.
    let answer = sym(&p, "fixture", "_answer");
    assert_eq!(deref(sym(&p, "fixture", "_answer_ptr")), answer);
    assert_eq!(m.read_u32(answer).unwrap(), 42);

    // Rebase to a string literal.
    let greeting = deref(sym(&p, "fixture", "_greeting"));
    assert_eq!(m.read_cstr(greeting, 64).unwrap(), "hello pomme");

    // Bind to a bundled dylib, which was slid away from address 0.
    let lib = p.images.iter().find(|i| i.name == "libfix.dylib").unwrap();
    assert!(lib.slide != 0);
    let lib_value = deref(sym(&p, "fixture", "_lib_ptr"));
    assert_eq!(lib_value, sym(&p, "libfix.dylib", "_lib_value"));
    assert_eq!(m.read_u32(lib_value).unwrap(), 7);

    // Rebase inside the slid dylib.
    assert_eq!(deref(sym(&p, "libfix.dylib", "_lib_self_ptr")), lib_value);

    // Bind through a re-exported library.
    let reexported = deref(sym(&p, "fixture", "_reexported_ptr"));
    assert_eq!(m.read_u32(reexported).unwrap(), 99);

    // Missing weak import is null.
    assert_eq!(deref(sym(&p, "fixture", "_weak_ptr")), 0);

    // System symbol goes to a trapping HLE stub.
    let puts = deref(sym(&p, "fixture", "_puts_ptr"));
    let stub = p.stubs.at(puts).expect("puts should be a stub");
    assert_eq!(stub.symbol, "_puts");
    assert_eq!(stub.library, "/usr/lib/libSystem.B.dylib");
    assert_eq!(m.read_u32(puts).unwrap(), STUB_INSN);

    // Entry point is mapped, executable code.
    let entry = p.entry().expect("entry");
    assert!(m.region_of(entry).unwrap().prot.exec);
    assert_eq!(entry, sym(&p, "fixture", "_main"));
}

#[test]
fn chained_fixups() {
    if !have_ios_sdk() {
        eprintln!("skipping: no iOS SDK");
        return;
    }
    let app = build_fixture("chained", &[]);
    let bytes = std::fs::read(app.join("fixture")).unwrap();
    assert!(pomme_macho::Image::parse(&bytes).unwrap().uses_chained_fixups);
    check(&app);
}

#[test]
fn dyld_info_opcodes() {
    if !have_ios_sdk() {
        eprintln!("skipping: no iOS SDK");
        return;
    }
    let app = build_fixture("opcodes", &["-Wl,-no_fixup_chains"]);
    let bytes = std::fs::read(app.join("fixture")).unwrap();
    assert!(!pomme_macho::Image::parse(&bytes).unwrap().uses_chained_fixups);
    check(&app);
}

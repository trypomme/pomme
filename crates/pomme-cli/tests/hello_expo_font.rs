//! End to end: expo-font (apps/hello-expo-font) on pomme, as on the iOS 26
//! simulator: Roboto Slab embedded by the config plugin (Info.plist's
//! UIAppFonts, registered at launch) at its regular, 800 and 200 weights
//! (named instances of the variable font), Roboto Medium loaded at run time
//! (useFonts: CTFontManagerRegisterFontsForURL), what the module reports, and
//! text rendered to an image (renderToImageAsync). Skipped unless the app
//! (scripts/build-app.sh hello-expo-font) and the sysroot (with
//! scripts/combine/build.sh and scripts/swiftui/build.sh) have been built.

use pomme_hle::{Emu, Stop};
use std::path::Path;

const APP: &str = "../../apps/hello-expo-font/ios/build/Build/Products/Release-iphoneos/helloexpofont.app";
const SYSROOT: &str = "../../vendor/sysroot";

fn boot() -> Option<Emu> {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/SwiftUI.framework/SwiftUI").exists() {
        eprintln!("skipping: build hello-expo-font and SwiftUI first");
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    Some(Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap())
}

/// The fonts load and report as on iOS (the simulator showed the same text);
/// each line is as tall as its own font sets it (Roboto Slab's 37 pt at
/// 28 pt, the system font's 33.7); the rendered image is 144 × 42 at 3x.
#[test]
fn loads_and_uses_fonts() {
    let Some(mut emu) = boot() else { return };
    emu.time_limit_ns = Some(8_000_000_000);
    match emu.run_main() {
        Err(Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}\n{}", emu.describe_views()),
    }
    let views = emu.describe_views();
    let line = |text: &str| views.lines().find(|l| l.contains(text)).unwrap_or_else(|| panic!("no {text:?}\n{views}")).to_string();
    assert!(line("\"loaded true · isLoaded true\"").contains("RCTParagraphComponentView"));
    let fonts = "fonts Roboto-Medium, RobotoSlab-Black, RobotoSlab-Bold, RobotoSlab-ExtraBold, RobotoSlab-ExtraLight, RobotoSlab-Light, RobotoSlab-Medium, RobotoSlab-Regular, RobotoSlab-SemiBold, RobotoSlab-Thin";
    assert!(views.contains(fonts), "{views}");
    assert!(line("\"System font\"").contains("342.0×33.7"), "{}", line("\"System font\""));
    for slab in ["\"Embedded: Roboto Slab\"", "\"Roboto Slab 800\"", "\"Roboto Slab 200\""] {
        assert!(line(slab).contains("342.0×37.0"), "{}", line(slab));
    }
    assert!(line("\"image ").contains("\"image 144×42 @3x\""), "{}", line("\"image "));
}

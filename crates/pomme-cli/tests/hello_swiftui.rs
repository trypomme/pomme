//! End to end: a SwiftUI app on pomme's own SwiftUI.framework
//! (scripts/swiftui). Skipped unless the app (apps/hello-swiftui) and the
//! framework have been built.

use pomme_hle::{Emu, Stop};
use std::path::Path;

const APP: &str = "../../apps/hello-swiftui/build/Build/Products/Release-iphoneos/HelloSwiftUI.app";
const SYSROOT: &str = "../../vendor/sysroot";

fn boot() -> Option<Emu> {
    let (app, sysroot) = (Path::new(APP), Path::new(SYSROOT));
    if !app.exists() || !sysroot.join("System/Library/Frameworks/SwiftUI.framework/SwiftUI").exists() {
        eprintln!("skipping: build hello-swiftui and scripts/swiftui/build.sh first");
        return None;
    }
    let opts = pomme_loader::LoadOptions { sysroot: Some(sysroot.canonicalize().unwrap()) };
    Some(Emu::new(pomme_loader::load_with(app, &opts).unwrap()).unwrap())
}

/// The app binds every SwiftUI symbol it imports to pomme's framework, and
/// runs until the first API pomme doesn't implement: `App.main()`, which
/// stops with its name.
#[test]
fn binds_to_pommes_swiftui_and_stops_at_app_main() {
    let Some(mut emu) = boot() else { return };
    assert!(emu.p.images.iter().any(|i| i.name == "SwiftUI"), "SwiftUI isn't loaded");
    emu.time_limit_ns = Some(2_000_000_000);
    let stop = emu.run_main();
    assert!(matches!(stop, Err(Stop::Cpu { exit: pomme_cpu::Exit::Brk { .. }, .. })), "{stop:?}");
    let calls: Vec<String> = emu.backtrace(8).into_iter().map(|a| emu.symbolize(a)).collect();
    assert!(calls.iter().any(|c| c.contains("_pommeUnimplemented")), "{calls:#?}");
    assert!(calls.iter().any(|c| c.contains("_$s7SwiftUI3AppPAAE4mainyyFZ")), "{calls:#?}");
}

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

/// Run until the app idles past `secs` of virtual time; then the view tree.
fn run_for(emu: &mut Emu, secs: f64) -> String {
    emu.time_limit_ns = Some((secs * 1e9) as u64);
    match emu.run_main() {
        Err(Stop::TimeLimit(_)) => {}
        other => panic!("expected the app to idle until the time limit, got {other:?}\n{}", emu.describe_views()),
    }
    emu.describe_views()
}

/// The app runs on pomme's SwiftUI: `App.main` starts UIKit, the app's
/// delegate (`@UIApplicationDelegateAdaptor`) configures Firebase, the scene
/// delegate shows the window group, and the view graph resolves
/// NavigationStack → List → ForEach → rows. Each row's `.task` runs its check
/// and sets its `@State`, which updates the row. (Offline: the checks that
/// need the network show their errors.)
#[test]
fn shows_the_firebase_screen_and_updates_rows_from_their_tasks() {
    let Some(mut emu) = boot() else { return };
    assert!(emu.p.images.iter().any(|i| i.name == "SwiftUI"), "SwiftUI isn't loaded");
    let views = run_for(&mut emu, 8.0);
    // The navigation bar shows the stack's title; the list, its five rows in one section.
    assert!(views.contains("UINavigationBar"), "{views}");
    assert!(views.contains("bg #f2f2f7ff"), "the list's grouped background\n{views}");
    let rows: Vec<&str> = views.lines().filter(|l| l.trim_start().starts_with("UILabel")).collect();
    assert_eq!(rows.len(), 5, "{views}");
    // Every row's task finished: none still shows its placeholder.
    assert!(!views.contains(" …\""), "{views}");
    for (row, start) in rows.iter().zip(["\"app __FIRAPP_DEFAULT · pomme-hello-expo-a089 · ios\"", "\"analytics logged", "\"crashlytics logged", "\"auth error", "\"firestore error"]) {
        assert!(row.contains(start), "{row}\n{views}");
    }
}

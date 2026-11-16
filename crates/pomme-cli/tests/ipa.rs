//! Opening an app from an `.ipa`: hello-expo packed as one (its `.app` at
//! `Payload/helloexpo.app`, beside the `SwiftSupport` and `Symbols` folders an
//! exported `.ipa` has) runs exactly as its `.app` does, through the `pomme`
//! command. Skipped when hello-expo isn't built or there's no sysroot.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

const APP: &str = "../../apps/hello-expo/ios/build/Build/Products/Release-iphoneos/helloexpo.app";
const SYSROOT: &str = "../../vendor/sysroot";

/// Zip `app` into an `.ipa` at `out`, keeping file modes and symbolic links.
fn pack(app: &Path, out: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fn walk(dir: &Path, base: &Path, w: &mut zip::ZipWriter<std::fs::File>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            let name = format!("Payload/{}", p.strip_prefix(base).unwrap().to_string_lossy());
            let meta = std::fs::symlink_metadata(&p).unwrap();
            let opts = zip::write::SimpleFileOptions::default().unix_permissions(meta.permissions().mode() & 0o777);
            if meta.file_type().is_symlink() {
                let target = std::fs::read_link(&p).unwrap();
                w.add_symlink(name, target.to_string_lossy(), opts).unwrap();
            } else if meta.is_dir() {
                w.add_directory(format!("{name}/"), opts).unwrap();
                walk(&p, base, w);
            } else {
                w.start_file(name, opts).unwrap();
                w.write_all(&std::fs::read(&p).unwrap()).unwrap();
            }
        }
    }
    let mut w = zip::ZipWriter::new(std::fs::File::create(out).unwrap());
    let opts = zip::write::SimpleFileOptions::default();
    w.add_directory("Payload/", opts).unwrap();
    w.add_directory("Payload/helloexpo.app/", opts).unwrap();
    walk(app, app.parent().unwrap(), &mut w);
    w.start_file("SwiftSupport/iphoneos/README", opts).unwrap();
    w.write_all(b"not part of the app").unwrap();
    w.start_file("Symbols/0000.symbols", opts).unwrap();
    w.finish().unwrap();
}

fn pomme(args: &[&str], cache: &Path) -> std::process::Output {
    let out = Command::new(env!("CARGO_BIN_EXE_pomme")).args(args).env("POMME_CACHE", cache).output().unwrap();
    assert!(out.status.success(), "pomme {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    out
}

#[test]
fn an_ipa_runs_as_its_app_does() {
    if !Path::new(APP).is_dir() || !Path::new(SYSROOT).is_dir() {
        eprintln!("skipped: hello-expo or the sysroot isn't built");
        return;
    }
    let tmp: PathBuf = std::env::temp_dir().join(format!("pomme-ipa-e2e-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    let ipa = tmp.join("helloexpo.ipa");
    pack(Path::new(APP), &ipa);
    let cache = tmp.join("cache");
    let ipa_s = ipa.to_str().unwrap();

    // inspect: the same images, the path aside.
    let unpacked = || std::fs::read_dir(cache.join("ipa")).unwrap().flatten().next().unwrap().path().join("helloexpo.app");
    let a = String::from_utf8_lossy(&pomme(&["inspect", APP], &cache).stdout).replace(APP, "<app>");
    let b = String::from_utf8_lossy(&pomme(&["inspect", ipa_s], &cache).stdout).into_owned();
    let b = b.replace(unpacked().to_str().unwrap(), "<app>");
    assert_eq!(a, b, "inspect differs between the .app and the .ipa");

    // Find the counter button (#tap-button) by accessibility, wherever this build puts it.
    let out = pomme(&["run", APP, "--sysroot", SYSROOT, "--offline", "--steps", "4000000000", "--seconds", "3", "--elements"], &cache);
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().find(|l| l.contains("#tap-button")).expect("hello-expo has no #tap-button");
    let f: Vec<f64> = line.rsplit_once('(').unwrap().1.split([',', '×', ')']).filter_map(|t| t.trim().parse().ok()).collect();
    let at = |t: u32| format!("{:.0},{:.0}@{t}", f[0] + f[2] / 2.0, f[1] + f[3] / 2.0);
    let (tap1, tap2) = (at(3), at(4));

    // run: tap the counter twice; the screens are identical.
    let shot = |path: &str, png: &Path| {
        let png_s = png.to_str().unwrap();
        let args = ["run", path, "--sysroot", SYSROOT, "--offline", "--steps", "4000000000", "--seconds", "6", "--tap", &tap1, "--tap", &tap2, "--views", "--screenshot", png_s];
        let out = pomme(&args, &cache);
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(text.contains("\"Tapped 2 times\""), "{path} didn't count the taps:\n{}", &text[text.len().saturating_sub(3000)..]);
        std::fs::read(png).unwrap()
    };
    let from_app = shot(APP, &tmp.join("app.png"));
    let from_ipa = shot(ipa_s, &tmp.join("ipa.png"));
    assert!(from_app == from_ipa, "the screenshots differ: see {}", tmp.display());

    // Only the app was unpacked, once.
    assert_eq!(std::fs::read_dir(cache.join("ipa")).unwrap().count(), 1);
    assert!(!unpacked().parent().unwrap().join("SwiftSupport").exists());
    let _ = std::fs::remove_dir_all(&tmp);
}

//! Every shader Skia (Ganesh) generated for hello-skia translates to WGSL that
//! naga parses and validates, the same checks wgpu applies.

use std::path::Path;

fn validate(name: &str, wgsl: &str) {
    let module = naga::front::wgsl::parse_str(wgsl).unwrap_or_else(|e| panic!("{name}: WGSL doesn't parse:\n{}\n{wgsl}", e.emit_to_string(wgsl)));
    let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default());
    if let Err(e) = v.validate(&module) {
        panic!("{name}: WGSL doesn't validate:\n{}\n{wgsl}", e.emit_to_string(wgsl));
    }
}

#[test]
fn skia_ganesh_shaders() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/skia");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert!(files.len() >= 10);
    for f in files {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let msl = std::fs::read_to_string(&f).unwrap();
        let t = pomme_msl::translate(&msl).unwrap_or_else(|e| panic!("{name}: {e}"));
        validate(&name, &t.wgsl);
        assert_eq!(t.entry_points.len(), 1, "{name}");
    }
}

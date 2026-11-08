//! Flutter's built-in Impeller shaders (MSL from impellerc for the engine
//! hello-flutter uses; see scripts/flutter/impeller-msl.sh) translate to WGSL
//! that naga parses and validates, unspecialized and with every function
//! constant set.

use std::collections::HashMap;
use std::path::Path;

fn validate(name: &str, wgsl: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(wgsl).map_err(|e| format!("{name}: WGSL doesn't parse:\n{}", e.emit_to_string(wgsl)))?;
    let mut v = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default());
    v.validate(&module).map_err(|e| format!("{name}: WGSL doesn't validate:\n{}", e.emit_to_string(wgsl)))?;
    Ok(())
}

#[test]
fn impeller_shaders() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/impeller");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert!(files.len() >= 59, "{} shaders", files.len());
    let all_set: HashMap<u32, f64> = (0..8).map(|i| (i, 1.0)).collect();
    let mut failures = Vec::new();
    for f in &files {
        let name = f.file_name().unwrap().to_string_lossy().into_owned();
        let msl = std::fs::read_to_string(f).unwrap();
        for constants in [HashMap::new(), all_set.clone()] {
            match pomme_msl::translate_specialized(&msl, &constants) {
                Ok(t) => {
                    if let Err(e) = validate(&name, &t.wgsl) {
                        failures.push(e);
                    }
                }
                Err(e) => failures.push(format!("{name}: {e}")),
            }
        }
    }
    failures.dedup();
    assert!(failures.is_empty(), "{} of {} shaders fail:\n{}", failures.len(), files.len() * 2, failures.iter().map(|f| f.lines().take(12).collect::<Vec<_>>().join("\n")).collect::<Vec<_>>().join("\n\n"));
}

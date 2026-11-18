//! The JIT adds translated code to pomme's function table at run time
//! (crates/pomme-jit/src/web.rs), so the table must be able to grow.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo:rustc-link-arg-cdylib=--growable-table");
    }
}

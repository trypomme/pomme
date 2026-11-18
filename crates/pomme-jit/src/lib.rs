//! pomme's JIT (openspec/changes/add-jit).
//!
//! The front end ([`lift`]) turns a block of arm64 code, decoded as the
//! interpreter caches it ([`pomme_cpu::fetch_block`]), into a small IR
//! ([`ir`]). Back ends turn the IR into host code: WebAssembly in the
//! browser, machine code on desktops. [`eval`] runs IR directly; it is slow
//! and only used by tests, to check the front end against the interpreter
//! without any back end.
//!
//! The rule for everything here: running a block through the JIT gives
//! exactly the interpreter's registers, memory, exits, pc and instruction
//! count. The interpreter is the reference.

pub mod eval;
pub mod ir;
pub mod jit;
pub mod lift;
pub mod wasm;
#[cfg(target_arch = "wasm32")]
pub mod web;

//! The JIT in the browser: modules from [`crate::wasm`] are compiled by the
//! browser and their functions added to pomme's own function table, so Rust
//! calls a translated block through a plain function pointer.
//!
//! Only built for WebAssembly. The helpers the modules import are the
//! `pomme_jit_*` functions below, exported by pomme's module.

use crate::ir::Block;
use crate::jit::{Ctx, Engine, Link};
use crate::wasm::{self, Layout};
use js_sys::{Object, Reflect, Uint8Array, WebAssembly};
use pomme_cpu::{Cpu, Exit};
use pomme_mem::Memory;
use std::rc::Rc;
use wasm_bindgen::{JsCast, JsValue};

/// A translated block, called through pomme's function table.
type BlockFn = unsafe extern "C" fn(*mut Cpu, *mut Ctx) -> u32;

#[derive(Debug)]
pub struct WasmEngine {
    table: WebAssembly::Table,
    imports: Object,
    layout: Layout,
    /// Host time spent making and compiling modules, in ms (for measuring).
    pub compile_ms: f64,
    pub modules: u32,
    pub blocks: u32,
    pub bytes: usize,
}

impl WasmEngine {
    /// None when the browser refuses (the caller keeps the interpreter).
    pub fn new() -> Option<WasmEngine> {
        let table: WebAssembly::Table = wasm_bindgen::function_table().dyn_into().ok()?;
        let exports = wasm_bindgen::exports();
        let env = Object::new();
        Reflect::set(&env, &"memory".into(), &wasm_bindgen::memory()).ok()?;
        Reflect::set(&env, &"table".into(), &table).ok()?;
        for (name, export) in [("load", "pomme_jit_load"), ("store", "pomme_jit_store"), ("helper", "pomme_jit_helper")] {
            let f = Reflect::get(&exports, &export.into()).ok()?;
            if !f.is_function() {
                return None;
            }
            Reflect::set(&env, &name.into(), &f).ok()?;
        }
        let imports = Object::new();
        Reflect::set(&imports, &"env".into(), &env).ok()?;
        Some(WasmEngine { table, imports, layout: Layout::of_cpu(), compile_ms: 0.0, modules: 0, blocks: 0, bytes: 0 })
    }

    fn try_compile(&mut self, blocks: &[Rc<Block>]) -> Result<Vec<usize>, JsValue> {
        let refs: Vec<&Block> = blocks.iter().map(|b| &**b).collect();
        // The table indices first, so blocks can call each other directly.
        let first = self.table.grow(blocks.len() as u32)?;
        let bytes = wasm::module(&refs, &self.layout, Some(first));
        self.bytes += bytes.len();
        let module = WebAssembly::Module::new(&Uint8Array::from(&bytes[..]).into())?;
        let instance = WebAssembly::Instance::new(&module, &self.imports)?;
        let exports = instance.exports();
        let mut handles = Vec::with_capacity(blocks.len());
        for i in 0..blocks.len() {
            let f: js_sys::Function = Reflect::get(&exports, &format!("b{i}").into())?.dyn_into()?;
            let at = first + i as u32;
            self.table.set(at, &f)?;
            handles.push(at as usize);
        }
        Ok(handles)
    }
}

fn now() -> f64 {
    js_sys::Date::now()
}

impl Engine for WasmEngine {
    fn compile(&mut self, blocks: &[Rc<Block>]) -> Vec<usize> {
        let t = now();
        let handles = self.try_compile(blocks).unwrap_or_else(|e| {
            // A module the browser refuses is a bug in the back end. The
            // blocks stay on the interpreter (handle 0 means "not translated").
            web_sys_log(&format!("pomme JIT: a module failed: {e:?}"));
            vec![0; blocks.len()]
        });
        self.compile_ms += now() - t;
        self.modules += 1;
        self.blocks += blocks.len() as u32;
        handles
    }

    fn call(&mut self, handle: usize, block: &Block, cpu: &mut Cpu, mem: &mut Memory, link: &mut Link) -> Result<(), Exit> {
        let tlb = mem.tlb_layout().base;
        let generation = mem.code_generation();
        let mut ctx = Ctx { fault: 0, tlb, fast: link.fast, changed: 0, generation, stop: link.stop, chained: 0, cpu, mem, block, exit: None };
        // SAFETY: `handle` is a table index this engine set to a function of
        // the module's block type, (i32, i32) -> i32, the same as `BlockFn`
        // on wasm32. The block reads and writes the `Cpu` struct through its
        // address and calls the helpers below, which use `ctx`'s pointers;
        // nothing else touches `cpu`, `mem` or `block` during the call.
        let f: BlockFn = unsafe { std::mem::transmute::<usize, BlockFn>(handle) };
        let status = unsafe { f(ctx.cpu, &mut ctx) };
        link.chained = ctx.chained;
        match status {
            0 => Ok(()),
            _ => Err(ctx.exit.unwrap_or(Exit::Unimplemented { insn: 0 })),
        }
    }

    fn fresh(&self) -> Box<dyn Engine> {
        match WasmEngine::new() {
            Some(e) => Box::new(e),
            None => Box::new(crate::jit::EvalEngine),
        }
    }

    fn name(&self) -> &'static str {
        "jit (WebAssembly)"
    }

    fn report(&self) -> String {
        format!("{} modules, {} blocks, {} KB of code, {:.0} ms compiling", self.modules, self.blocks, self.bytes / 1024, self.compile_ms)
    }

    fn zero_means_failed(&self) -> bool {
        // Table index 0 is never a function pomme sets.
        true
    }
}

fn web_sys_log(s: &str) {
    let console = Reflect::get(&js_sys::global(), &"console".into()).ok();
    if let Some(c) = console
        && let Ok(log) = Reflect::get(&c, &"warn".into())
        && let Ok(log) = log.dyn_into::<js_sys::Function>()
    {
        let _ = log.call1(&c, &s.into());
    }
}

/// A guest load for translated code: the value, or 0 with the context's
/// fault word set and the exit kept.
///
/// # Safety
/// `ctx` is the context of the running block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pomme_jit_load(ctx: *mut Ctx, addr: u64, bytes: u32) -> u64 {
    let ctx = unsafe { &mut *ctx };
    let mem = unsafe { &*ctx.mem };
    let r = match bytes {
        1 => mem.read_u8(addr).map(|x| x as u64),
        2 => mem.read_u16(addr).map(|x| x as u64),
        4 => mem.read_u32(addr).map(|x| x as u64),
        _ => mem.read_u64(addr),
    };
    r.unwrap_or_else(|e| {
        ctx.fault = 1;
        ctx.exit = Some(e.into());
        0
    })
}

/// A guest store for translated code: 1 if it faulted (the exit is kept).
///
/// # Safety
/// `ctx` is the context of the running block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pomme_jit_store(ctx: *mut Ctx, addr: u64, value: u64, bytes: u32) -> u32 {
    let ctx = unsafe { &mut *ctx };
    let mem = unsafe { &mut *ctx.mem };
    let r = match bytes {
        1 => mem.write_u8(addr, value as u8),
        2 => mem.write_u16(addr, value as u16),
        4 => mem.write_u32(addr, value as u32),
        _ => mem.write_u64(addr, value),
    };
    if mem.code_generation() != ctx.generation {
        ctx.changed = 1;
    }
    match r {
        Ok(()) => 0,
        Err(e) => {
            ctx.exit = Some(e.into());
            1
        }
    }
}

/// Run guest instruction `insn` of the running block with the interpreter
/// (pc and the step count are already set): 1 if it stopped.
///
/// # Safety
/// `ctx` is the context of the running block.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pomme_jit_helper(ctx: *mut Ctx, insn: u32) -> u32 {
    let ctx = unsafe { &mut *ctx };
    let (cpu, mem, block) = unsafe { (&mut *ctx.cpu, &mut *ctx.mem, &*ctx.block) };
    let r = cpu.exec_op(&block.ops[insn as usize], mem);
    if mem.code_generation() != ctx.generation {
        ctx.changed = 1;
    }
    match r {
        Ok(()) => 0,
        Err(e) => {
            ctx.exit = Some(e);
            1
        }
    }
}

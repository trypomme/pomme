//! The WebAssembly back end does exactly what the evaluator (and so the
//! interpreter) does. Each random instruction or block is turned into a
//! module and run with wasmi (a WebAssembly interpreter in Rust), with the
//! registers in its memory and the same three helpers pomme gives the
//! module in the browser. Everything is compared with the interpreter.

mod common;

use common::*;
use pomme_cpu::{Cpu, Exit};
use pomme_jit::ir::Block;
use pomme_jit::{lift::lift, wasm};
use pomme_mem::Memory;
use wasmi::{Caller, Engine, Linker, Module, Store};

/// Where the test keeps the registers and the context in wasmi's memory.
const CPU_AT: usize = 0x100;
const CTX_AT: usize = 0x10;
const LAYOUT: wasm::Layout = wasm::Layout { x: 0, sp: 248, pc: 256, steps: 264, n: 272, z: 273, c: 274, v: 275, tlb: None, chain: None };

/// The same with a TLB in wasmi's memory: the test's four data pages are
/// copied to `MIRROR`; page 0 and 1 may be loaded and stored straight
/// there, page 2 only loaded, and page 3 (and the code) always go through
/// the helpers, which see the copy too.
const TLB_AT: u32 = 0x1000;
const MIRROR: usize = 0x10000;
const WITH_TLB: wasm::Layout = wasm::Layout { tlb: Some(wasm::InlineTlb { ctx: 4, slots: 1024, stride: 16, page: 0, host: 8, writable: 12 }), ..LAYOUT };

struct Host {
    mem: Memory,
    /// Everything but the registers in wasm memory (vector registers, FP status, the exclusive monitor).
    cpu: Cpu,
    block: Block,
    exit: Option<Exit>,
    wmem: Option<wasmi::Memory>,
    tlb: bool,
}

fn put(bytes: &mut [u8], c: &Cpu) {
    let at = |off: u32| CPU_AT + off as usize;
    for (i, x) in c.x.iter().enumerate() {
        bytes[at(LAYOUT.x) + 8 * i..][..8].copy_from_slice(&x.to_le_bytes());
    }
    bytes[at(LAYOUT.sp)..][..8].copy_from_slice(&c.sp.to_le_bytes());
    bytes[at(LAYOUT.pc)..][..8].copy_from_slice(&c.pc.to_le_bytes());
    bytes[at(LAYOUT.steps)..][..8].copy_from_slice(&c.steps.to_le_bytes());
    bytes[at(LAYOUT.n)] = c.n as u8;
    bytes[at(LAYOUT.z)] = c.z as u8;
    bytes[at(LAYOUT.c)] = c.c as u8;
    bytes[at(LAYOUT.v)] = c.v as u8;
}

fn take(bytes: &[u8], c: &mut Cpu) {
    let at = |off: u32| CPU_AT + off as usize;
    let u64_at = |o: usize| u64::from_le_bytes(bytes[o..o + 8].try_into().unwrap());
    for i in 0..31 {
        c.x[i] = u64_at(at(LAYOUT.x) + 8 * i);
    }
    c.sp = u64_at(at(LAYOUT.sp));
    c.pc = u64_at(at(LAYOUT.pc));
    c.steps = u64_at(at(LAYOUT.steps));
    c.n = bytes[at(LAYOUT.n)] != 0;
    c.z = bytes[at(LAYOUT.z)] != 0;
    c.c = bytes[at(LAYOUT.c)] != 0;
    c.v = bytes[at(LAYOUT.v)] != 0;
}

fn set_fault(caller: &mut Caller<'_, Host>, ctx: i32) {
    let wmem = caller.data().wmem.unwrap();
    wmem.data_mut(&mut *caller)[ctx as usize..][..4].copy_from_slice(&1u32.to_le_bytes());
}

/// With a TLB: the helpers work on `mem`, so copy the mirrored pages there
/// before (`to_mem`) and back after.
fn sync(caller: &mut Caller<'_, Host>, to_mem: bool) {
    let wmem = caller.data().wmem.unwrap();
    if !caller.data().tlb {
        return;
    }
    if to_mem {
        let copy = wmem.data(&*caller)[MIRROR..MIRROR + SIZE as usize].to_vec();
        caller.data_mut().mem.write(BASE, &copy).unwrap();
    } else {
        let copy = caller.data().mem.read(BASE, SIZE as usize).unwrap().to_vec();
        wmem.data_mut(&mut *caller)[MIRROR..MIRROR + SIZE as usize].copy_from_slice(&copy);
    }
}

/// Run `block` as WebAssembly on `cpu` and `mem`.
fn run_wasm(block: &Block, cpu: &mut Cpu, mem: &mut Memory, layout: &wasm::Layout) -> Result<(), Exit> {
    let bytes = wasm::module(&[block], layout, None);
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes[..]).expect("a valid module");
    let host = Host { mem: std::mem::take(mem), cpu: cpu.clone(), block: block.clone(), exit: None, wmem: None, tlb: layout.tlb.is_some() };
    let mut store = Store::new(&engine, host);
    let wmem = wasmi::Memory::new(&mut store, wasmi::MemoryType::new(3, None)).unwrap();
    store.data_mut().wmem = Some(wmem);
    let mut linker = <Linker<Host>>::new(&engine);
    linker.define("env", "memory", wmem).unwrap();
    linker
        .func_wrap("env", "load", |mut caller: Caller<'_, Host>, ctx: i32, addr: i64, bytes: i32| -> i64 {
            sync(&mut caller, true);
            let m = &caller.data().mem;
            let a = addr as u64;
            let r = match bytes {
                1 => m.read_u8(a).map(|x| x as u64),
                2 => m.read_u16(a).map(|x| x as u64),
                4 => m.read_u32(a).map(|x| x as u64),
                _ => m.read_u64(a),
            };
            match r {
                Ok(v) => v as i64,
                Err(e) => {
                    caller.data_mut().exit = Some(e.into());
                    set_fault(&mut caller, ctx);
                    0
                }
            }
        })
        .unwrap();
    linker
        .func_wrap("env", "store", |mut caller: Caller<'_, Host>, _ctx: i32, addr: i64, value: i64, bytes: i32| -> i32 {
            sync(&mut caller, true);
            let m = &mut caller.data_mut().mem;
            let (a, v) = (addr as u64, value as u64);
            let r = match bytes {
                1 => m.write_u8(a, v as u8),
                2 => m.write_u16(a, v as u16),
                4 => m.write_u32(a, v as u32),
                _ => m.write_u64(a, v),
            };
            sync(&mut caller, false);
            match r {
                Ok(()) => 0,
                Err(e) => {
                    caller.data_mut().exit = Some(e.into());
                    1
                }
            }
        })
        .unwrap();
    linker
        .func_wrap("env", "helper", |mut caller: Caller<'_, Host>, _ctx: i32, insn: i32| -> i32 {
            sync(&mut caller, true);
            let wmem = caller.data().wmem.unwrap();
            let mut c = caller.data().cpu.clone();
            take(wmem.data(&caller), &mut c);
            let op = caller.data().block.ops[insn as usize];
            let h = caller.data_mut();
            let r = c.exec_op(&op, &mut h.mem);
            h.cpu = c.clone();
            put(wmem.data_mut(&mut caller), &c);
            sync(&mut caller, false);
            match r {
                Ok(()) => 0,
                Err(e) => {
                    caller.data_mut().exit = Some(e);
                    1
                }
            }
        })
        .unwrap();
    let instance = linker.instantiate_and_start(&mut store, &module).unwrap();
    let f = instance.get_typed_func::<(i32, i32), i32>(&store, "b0").unwrap();
    put(wmem.data_mut(&mut store), cpu);
    if layout.tlb.is_some() {
        let copy = store.data().mem.read(BASE, SIZE as usize).unwrap().to_vec();
        let d = wmem.data_mut(&mut store);
        d[MIRROR..MIRROR + SIZE as usize].copy_from_slice(&copy);
        d[CTX_AT + 4..][..4].copy_from_slice(&TLB_AT.to_le_bytes());
        for slot in 0..1024usize {
            d[TLB_AT as usize + 16 * slot..][..8].copy_from_slice(&u64::MAX.to_le_bytes());
        }
        for k in 0..4u64 {
            let page = BASE / pomme_mem::PAGE_SIZE + k;
            let e = TLB_AT as usize + 16 * (page as usize % 1024);
            let host = if k == 3 { 0 } else { (MIRROR + k as usize * pomme_mem::PAGE_SIZE as usize) as u32 };
            d[e..][..8].copy_from_slice(&page.to_le_bytes());
            d[e + 8..][..4].copy_from_slice(&host.to_le_bytes());
            d[e + 12] = (k < 2) as u8;
        }
    }
    let status = f.call(&mut store, (CPU_AT as i32, CTX_AT as i32)).unwrap();
    if layout.tlb.is_some() {
        let copy = wmem.data(&store)[MIRROR..MIRROR + SIZE as usize].to_vec();
        store.data_mut().mem.write(BASE, &copy).unwrap();
    }
    let host = store.data();
    let mut out = host.cpu.clone();
    take(wmem.data(&store), &mut out);
    *cpu = out;
    let exit = host.exit;
    *mem = std::mem::take(&mut store.data_mut().mem);
    match status {
        0 => Ok(()),
        _ => Err(exit.expect("a stopped block says why")),
    }
}

fn check(code: &[u32], seed: u64) {
    check_with(code, seed, &LAYOUT);
    check_with(code, seed, &WITH_TLB);
}

fn check_with(code: &[u32], seed: u64, layout: &wasm::Layout) {
    let (start, mut mem) = setup(code, seed);
    let (mut a, mut b) = (start.clone(), start);
    let mut mb = mem.clone();
    let Some(ops) = pomme_cpu::fetch_block(PC, &mem) else {
        return;
    };
    let ra = interpret(&mut a, &mut mem, ops.len());
    let block = lift(PC, ops);
    let rb = run_wasm(&block, &mut b, &mut mb, layout);
    let what = || format!("{code:08x?} (inline TLB: {})\n{:?}", layout.tlb.is_some(), block.ops);
    assert_eq!(format!("{ra:?}"), format!("{rb:?}"), "{}", what());
    assert_eq!(state(&a), state(&b), "{}", what());
    assert_eq!(mem.read(BASE, SIZE as usize).unwrap(), mb.read(BASE, SIZE as usize).unwrap(), "{}", what());
}

#[test]
fn single_instructions_match_the_interpreter() {
    let mut r = Rng(0x0123_4567_89ab_cdef);
    for i in 0..20_000u32 {
        let insn = match i % 4 {
            0 => r.next() as u32,
            1 => pick(&mut r, BRANCHES),
            _ => pick(&mut r, TEMPLATES),
        };
        check(&[insn], r.next());
    }
}

#[test]
fn blocks_match_the_interpreter() {
    let mut r = Rng(0x0fed_cba9_8765_4321);
    for _ in 0..10_000 {
        let code = random_block(&mut r);
        check(&code, r.next());
    }
}

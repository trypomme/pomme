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

/// With chaining: the context is laid out as [`pomme_jit::jit::Ctx`] is on
/// 32-bit targets, the table of translated blocks is at `FAST_AT`, and each
/// entry's `block` is the block's index in the test's list.
const FAST_AT: usize = 0x20000;
const FAST_SLOTS: u32 = 256;
const CHAIN: wasm::Chain = wasm::Chain {
    ctx_fast: 8,
    ctx_changed: 12,
    ctx_stop: 16,
    ctx_chained: 24,
    ctx_block: 32,
    slots: FAST_SLOTS,
    stride: 32,
    pc: 0,
    len: 8,
    handle: 16,
    block: 20,
};
const CHAINED: wasm::Layout = wasm::Layout { chain: Some(CHAIN), ..WITH_TLB };

struct Host {
    mem: Memory,
    /// Everything but the registers in wasm memory (vector registers, FP status, the exclusive monitor).
    cpu: Cpu,
    /// The module's blocks; the context says which one runs.
    blocks: Vec<Block>,
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
    run_blocks(std::slice::from_ref(block), cpu, mem, layout, u64::MAX, false).0
}

/// Run `blocks[0]` as WebAssembly on `cpu` and `mem`, as one module; with
/// chaining, it goes on to the module's other blocks (all in the table of
/// translated blocks) while they fit before `stop`. Also how many blocks
/// went on to another. With `through_table`, the module is told its blocks
/// are somewhere else in the table, so each goes on to the next through the
/// table, as a block of another module would.
fn run_blocks(blocks: &[Block], cpu: &mut Cpu, mem: &mut Memory, layout: &wasm::Layout, stop: u64, through_table: bool) -> (Result<(), Exit>, u64) {
    let refs: Vec<&Block> = blocks.iter().collect();
    // Table index 0 stays empty, as in pomme.
    let first = layout.chain.map(|_| if through_table { 1 << 20 } else { 1 });
    let bytes = wasm::module(&refs, layout, first);
    let engine = Engine::default();
    let module = Module::new(&engine, &bytes[..]).expect("a valid module");
    let host = Host { mem: std::mem::take(mem), cpu: cpu.clone(), blocks: blocks.to_vec(), exit: None, wmem: None, tlb: layout.tlb.is_some() };
    let mut store = Store::new(&engine, host);
    let wmem = wasmi::Memory::new(&mut store, wasmi::MemoryType::new(3, None)).unwrap();
    store.data_mut().wmem = Some(wmem);
    let mut linker = <Linker<Host>>::new(&engine);
    linker.define("env", "memory", wmem).unwrap();
    let table =
        wasmi::Table::new(&mut store, wasmi::TableType::new(wasmi::RefType::Func, 1 + blocks.len() as u32, None), wasmi::Ref::Func(wasmi::Nullable::Null))
            .unwrap();
    if layout.chain.is_some() {
        linker.define("env", "table", table).unwrap();
    }
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
            let at = CTX_AT + CHAIN.ctx_block as usize;
            let block = u32::from_le_bytes(wmem.data(&caller)[at..at + 4].try_into().unwrap()) as usize;
            let op = caller.data().blocks[block].ops[insn as usize];
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
    if let Some(c) = layout.chain {
        let d = wmem.data_mut(&mut store);
        d[CTX_AT + c.ctx_fast as usize..][..4].copy_from_slice(&(FAST_AT as u32).to_le_bytes());
        d[CTX_AT + c.ctx_stop as usize..][..8].copy_from_slice(&stop.to_le_bytes());
        for (i, b) in blocks.iter().enumerate() {
            let e = FAST_AT + c.stride as usize * ((b.pc >> 2) as usize % c.slots as usize);
            d[e + c.pc as usize..][..8].copy_from_slice(&b.pc.to_le_bytes());
            d[e + c.len as usize..][..8].copy_from_slice(&(b.len() as u64).to_le_bytes());
            d[e + c.handle as usize..][..4].copy_from_slice(&(1 + i as u32).to_le_bytes());
            d[e + c.block as usize..][..4].copy_from_slice(&(i as u32).to_le_bytes());
        }
        for i in 0..blocks.len() {
            let func = instance.get_func(&store, &format!("b{i}")).unwrap();
            table.set(&mut store, 1 + i as u64, wasmi::Ref::Func(wasmi::Nullable::Val(func))).unwrap();
        }
    }
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
    let at = CTX_AT + CHAIN.ctx_chained as usize;
    let chained = u64::from_le_bytes(wmem.data(&store)[at..at + 8].try_into().unwrap());
    *mem = std::mem::take(&mut store.data_mut().mem);
    let r = match status {
        0 => Ok(()),
        _ => Err(exit.expect("a stopped block says why")),
    };
    (r, chained)
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

/// A branch at `from` to `to`: b, b.cond (falls through to the next
/// piece), cbz/cbnz, or ret (to x30, set to `to` first unless the piece's
/// own code changes it).
fn branch(r: &mut Rng, from: u64, to: u64) -> u32 {
    let off = (to.wrapping_sub(from) as i64 / 4) as u32;
    match r.next() % 5 {
        0 => 0x1400_0000 | (off & 0x03ff_ffff),
        1 | 2 => 0x5400_0000 | ((off & 0x7ffff) << 5) | (r.next() as u32 % 15),
        3 => 0xb400_0000 | ((r.next() as u32 & 1) << 24) | ((off & 0x7ffff) << 5) | (r.next() as u32 % 31),
        _ => 0xd65f_03c0,
    }
}

/// A few blocks that branch to each other (loops included) and to code
/// that isn't translated, run as one module with chaining: the same as
/// the interpreter running the same blocks, block by block, with the same
/// stop.
#[test]
fn chained_blocks_match_the_interpreter() {
    let mut r = Rng(0x005e_ed0f_b10c_cafe);
    let mut chained = 0;
    for i in 0..4000 {
        // Pieces of straight code, each ending with a branch to the start of one of them.
        let pieces = 2 + r.next() as usize % 4;
        let bodies: Vec<Vec<u32>> = (0..pieces)
            .map(|_| {
                let mut b = random_block(&mut r);
                b.pop();
                b.truncate(1 + r.next() as usize % 6);
                b
            })
            .collect();
        let mut starts = vec![PC];
        for b in &bodies {
            starts.push(starts.last().unwrap() + 4 * (b.len() as u64 + 1));
        }
        let mut code = Vec::new();
        let mut rets = Vec::new();
        for (i, b) in bodies.iter().enumerate() {
            code.extend(b);
            let at = starts[i] + 4 * b.len() as u64;
            // Now and then somewhere that isn't translated.
            let to = if r.next().is_multiple_of(6) { starts[pieces] + 64 } else { starts[r.next() as usize % pieces] };
            let insn = branch(&mut r, at, to);
            if insn == 0xd65f_03c0 {
                rets.push(to);
            }
            code.push(insn);
        }
        let seed = r.next();
        let (mut start, mut mem) = setup(&code, seed);
        if let Some(&to) = rets.first() {
            start.x[30] = to;
        }
        // The blocks, as the JIT would find them (one may end early, at an
        // instruction that ends a block, and then its rest isn't translated).
        let blocks: Vec<Block> = starts[..pieces].iter().filter_map(|&pc| pomme_cpu::fetch_block(pc, &mem).map(|ops| lift(pc, ops))).collect();
        if blocks.first().map(|b| b.pc) != Some(PC) {
            continue;
        }
        let len_at = |pc: u64| blocks.iter().find(|b| b.pc == pc).map(|b| b.len() as u64);
        let stop = start.steps + 5 + r.next() % 200;
        if start.steps + blocks[0].len() as u64 > stop {
            continue;
        }

        // The interpreter, block by block. (Code that writes over its own
        // code is skipped: the JIT then needs to look before going on.)
        let (mut a, mut ma) = (start.clone(), mem.clone());
        let code_now = |m: &Memory| m.read(PC, 4 * code.len()).unwrap().to_vec();
        let before = code_now(&mem);
        let ra = loop {
            let n = len_at(a.pc).unwrap();
            if let Err(e) = interpret(&mut a, &mut ma, n as usize) {
                break Err(e);
            }
            match len_at(a.pc) {
                Some(n) if a.steps + n <= stop => {}
                _ => break Ok(()),
            }
        };
        if code_now(&ma) != before {
            continue;
        }
        let mut b = start;
        let (rb, c) = run_blocks(&blocks, &mut b, &mut mem, &CHAINED, stop, i % 4 == 0);
        chained += c;
        let what =
            || format!("{code:08x?} stop {stop} blocks {:?} start {:x?} a.pc {:x}", blocks.iter().map(|b| (b.pc, b.len())).collect::<Vec<_>>(), starts, a.pc);
        assert_eq!(format!("{ra:?}"), format!("{rb:?}"), "{}", what());
        assert_eq!(state(&a), state(&b), "{}", what());
        assert_eq!(ma.read(BASE, SIZE as usize).unwrap(), mem.read(BASE, SIZE as usize).unwrap(), "{}", what());
    }
    // The test is only worth something if blocks did go on to each other.
    assert!(chained > 5_000, "only {chained} blocks went on to another");
}

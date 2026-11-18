//! The WebAssembly back end: IR blocks to one WebAssembly module.
//!
//! The module imports pomme's own memory (the `Cpu` struct lives in it) and
//! three helpers from pomme. Each block becomes one exported function
//! `b<i>(cpu, ctx) -> status`:
//!
//! - `cpu` is the address of the `Cpu` struct; registers and flags are read
//!   and written there, at the offsets in [`Layout`].
//! - `ctx` is the address of the run's [`crate::jit::Ctx`]; it is passed to
//!   the helpers, and its first word is set when a load or store faults.
//! - The status is 0 when the whole block ran, 1 when it stopped (the exit
//!   is kept in the context).
//!
//! The imports, all from module `env`:
//!
//! - `memory`: pomme's memory.
//! - `load(ctx, addr: i64, bytes: i32) -> i64`: a guest load.
//! - `store(ctx, addr: i64, value: i64, bytes: i32) -> i32`: a guest store; 1 if it faulted.
//! - `helper(ctx, insn: i32) -> i32`: run guest instruction `insn` of the
//!   block with the interpreter; 1 if it stopped.
//!
//! The code does exactly what [`crate::eval`] does.

use crate::ir::{BinOp, Block, ExitPoint, Inst, Loc, V};
use wasm_encoder::{
    CodeSection, EntityType, ExportKind, ExportSection, Function, FunctionSection, ImportSection, Instruction as I, MemArg, MemoryType, Module, TypeSection,
    ValType,
};

/// Where the guest state is in the `Cpu` struct (byte offsets from its start).
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    /// X0 (X1 is 8 bytes after it, and so on).
    pub x: u32,
    pub sp: u32,
    pub pc: u32,
    pub steps: u32,
    /// The flags, one byte each (0 or 1).
    pub n: u32,
    pub z: u32,
    pub c: u32,
    pub v: u32,
    /// Check the TLB in the generated code and load or store straight
    /// there on a hit (None: every access calls the helpers).
    pub tlb: Option<InlineTlb>,
    /// Go on to the next block from the end of a block, without returning
    /// (None: always return).
    pub chain: Option<Chain>,
}

/// What the end of a block needs to go on to the next one: the offsets in
/// the context ([`crate::jit::Ctx`]) and the layout of the JIT's table of
/// translated blocks ([`crate::jit::Fast`]). The module then imports
/// pomme's function table as `env.table`.
#[derive(Clone, Copy, Debug)]
pub struct Chain {
    pub ctx_fast: u32,
    pub ctx_changed: u32,
    pub ctx_stop: u32,
    pub ctx_chained: u32,
    pub ctx_block: u32,
    pub slots: u32,
    pub stride: u32,
    pub pc: u32,
    pub len: u32,
    pub handle: u32,
    pub block: u32,
}

/// The TLB as the generated code sees it: [`pomme_mem::TlbLayout`], with
/// the TLB's address read from the context at `ctx`. Host addresses are 32 bits.
#[derive(Clone, Copy, Debug)]
pub struct InlineTlb {
    /// Offset of the TLB's address in the context.
    pub ctx: u32,
    pub slots: u32,
    pub stride: u32,
    pub page: u32,
    pub host: u32,
    pub writable: u32,
}

impl Layout {
    /// The layout of pomme's `Cpu` struct in this build.
    pub fn of_cpu() -> Layout {
        use pomme_cpu::Cpu;
        use std::mem::offset_of;
        // Only the layout is used: each run's TLB address is in its context.
        let t = pomme_mem::Memory::default().tlb_layout();
        let tlb = (size_of::<usize>() == 4).then_some(InlineTlb {
            ctx: offset_of!(crate::jit::Ctx, tlb) as u32,
            slots: t.slots,
            stride: t.stride,
            page: t.page,
            host: t.host,
            writable: t.writable,
        });
        use crate::jit::{Ctx, FAST_SLOTS, Fast};
        let chain = (size_of::<usize>() == 4).then_some(Chain {
            ctx_fast: offset_of!(Ctx, fast) as u32,
            ctx_changed: offset_of!(Ctx, changed) as u32,
            ctx_stop: offset_of!(Ctx, stop) as u32,
            ctx_chained: offset_of!(Ctx, chained) as u32,
            ctx_block: offset_of!(Ctx, block) as u32,
            slots: FAST_SLOTS as u32,
            stride: size_of::<Fast>() as u32,
            pc: offset_of!(Fast, pc) as u32,
            len: offset_of!(Fast, len) as u32,
            handle: offset_of!(Fast, handle) as u32,
            block: offset_of!(Fast, block) as u32,
        });
        Layout {
            x: offset_of!(Cpu, x) as u32,
            sp: offset_of!(Cpu, sp) as u32,
            pc: offset_of!(Cpu, pc) as u32,
            steps: offset_of!(Cpu, steps) as u32,
            n: offset_of!(Cpu, n) as u32,
            z: offset_of!(Cpu, z) as u32,
            c: offset_of!(Cpu, c) as u32,
            v: offset_of!(Cpu, v) as u32,
            tlb,
            chain,
        }
    }
}

const LOAD: u32 = 0;
const STORE: u32 = 1;
const HELPER: u32 = 2;
/// Function indices of the blocks start after the imported helpers.
const FIRST_BLOCK: u32 = 3;
/// The type of a block's entry, `b<i>(cpu, ctx) -> status`.
const BLOCK_TYPE: u32 = 3;
/// The type of the module's one real function, `run(cpu, ctx, block) -> status`.
const RUN_TYPE: u32 = 4;

/// Parameters and locals of `run`.
const CPU: u32 = 0;
const CTX: u32 = 1;
/// Which of the module's blocks runs next.
const INDEX: u32 = 2;
const ENTRY: u32 = 3;
/// Scratch for inline loads and stores: the untagged address (i64), the TLB
/// entry's address and the host address (i32).
const ADDR: u32 = 4;
const SLOT: u32 = 5;
const HOST: u32 = 6;
const FIRST_VALUE: u32 = 7;

/// A module with one exported function per block, `b0`, `b1`, ….
///
/// With chaining, `first_handle` is the table index `b0` will get (the
/// others follow it): a block then goes on to a block of the same module
/// with a direct tail call, which is cheaper than one through the table.
pub fn module(blocks: &[&Block], layout: &Layout, first_handle: Option<u32>) -> Vec<u8> {
    let mut types = TypeSection::new();
    types.ty().function([ValType::I32, ValType::I64, ValType::I32], [ValType::I64]); // load
    types.ty().function([ValType::I32, ValType::I64, ValType::I64, ValType::I32], [ValType::I32]); // store
    types.ty().function([ValType::I32, ValType::I32], [ValType::I32]); // helper
    types.ty().function([ValType::I32, ValType::I32], [ValType::I32]); // a block
    types.ty().function([ValType::I32, ValType::I32, ValType::I32], [ValType::I32]); // run

    let mut imports = ImportSection::new();
    imports.import("env", "load", EntityType::Function(0));
    imports.import("env", "store", EntityType::Function(1));
    imports.import("env", "helper", EntityType::Function(2));
    imports.import("env", "memory", EntityType::Memory(MemoryType { minimum: 1, maximum: None, memory64: false, shared: false, page_size_log2: None }));
    if layout.chain.is_some() {
        imports.import(
            "env",
            "table",
            EntityType::Table(wasm_encoder::TableType {
                element_type: wasm_encoder::RefType::FUNCREF,
                table64: false,
                minimum: 1,
                maximum: None,
                shared: false,
            }),
        );
    }

    let mut funcs = FunctionSection::new();
    let mut exports = ExportSection::new();
    let mut code = CodeSection::new();
    // Blocks of this module by pc: (index in the module, table index).
    let here: std::collections::HashMap<u64, (u32, u32)> = match first_handle {
        Some(first) => blocks.iter().enumerate().map(|(i, b)| (b.pc, (i as u32, first + i as u32))).collect(),
        None => Default::default(),
    };
    // b<i>: run(cpu, ctx, i).
    let run = FIRST_BLOCK + blocks.len() as u32;
    for i in 0..blocks.len() {
        funcs.function(BLOCK_TYPE);
        exports.export(&format!("b{i}"), ExportKind::Func, FIRST_BLOCK + i as u32);
        let mut f = Function::new([]);
        f.instruction(&I::LocalGet(CPU));
        f.instruction(&I::LocalGet(CTX));
        f.instruction(&I::I32Const(i as i32));
        f.instruction(&if layout.chain.is_some() { I::ReturnCall(run) } else { I::Call(run) });
        f.instruction(&I::End);
        code.function(&f);
    }
    funcs.function(RUN_TYPE);
    code.function(&run_function(blocks, layout, &here));

    let mut m = Module::new();
    m.section(&types).section(&imports).section(&funcs).section(&exports).section(&code);
    m.finish()
}

fn mem(offset: u32, align: u32) -> MemArg {
    MemArg { offset: offset as u64, align, target: wasm_encoder::MemArgTarget::Memory(0), ordering: None }
}

struct Gen<'a> {
    f: Function,
    layout: &'a Layout,
    block: &'a Block,
    /// How many labels out the dispatch loop is, from the block's own code.
    to_loop: u32,
}

impl Gen<'_> {
    fn i(&mut self, ins: I) {
        self.f.instruction(&ins);
    }

    fn val(&mut self, v: V) {
        self.i(I::LocalGet(FIRST_VALUE + v.0));
    }

    fn def(&mut self, v: V) {
        self.i(I::LocalSet(FIRST_VALUE + v.0));
    }

    /// Push guest state `loc` (as i64).
    fn get(&mut self, loc: Loc) {
        let l = self.layout;
        self.i(I::LocalGet(CPU));
        match loc {
            Loc::X(r) => self.i(I::I64Load(mem(l.x + 8 * r as u32, 3))),
            Loc::Sp => self.i(I::I64Load(mem(l.sp, 3))),
            Loc::Pc => self.i(I::I64Load(mem(l.pc, 3))),
            Loc::N => self.i(I::I64Load8U(mem(l.n, 0))),
            Loc::Z => self.i(I::I64Load8U(mem(l.z, 0))),
            Loc::C => self.i(I::I64Load8U(mem(l.c, 0))),
            Loc::Vf => self.i(I::I64Load8U(mem(l.v, 0))),
        }
    }

    /// Store the value pushed by `push` into guest state `loc`.
    fn set(&mut self, loc: Loc, push: impl FnOnce(&mut Self)) {
        let l = *self.layout;
        self.i(I::LocalGet(CPU));
        push(self);
        let flag = |s: &mut Self, off: u32| {
            // A flag is a bool: store 1 for any value that isn't 0.
            s.i(I::I64Const(0));
            s.i(I::I64Ne);
            s.i(I::I32Store8(mem(off, 0)));
        };
        match loc {
            Loc::X(r) => self.i(I::I64Store(mem(l.x + 8 * r as u32, 3))),
            Loc::Sp => self.i(I::I64Store(mem(l.sp, 3))),
            Loc::Pc => self.i(I::I64Store(mem(l.pc, 3))),
            Loc::N => flag(self, l.n),
            Loc::Z => flag(self, l.z),
            Loc::C => flag(self, l.c),
            Loc::Vf => flag(self, l.v),
        }
    }

    /// pc and the step count at guest instruction `insn` (before it runs).
    fn at_insn(&mut self, insn: u16) {
        let pc = self.block.pc.wrapping_add(4 * insn as u64);
        self.set(Loc::Pc, |s| s.i(I::I64Const(pc as i64)));
        self.steps(insn as u64);
    }

    /// steps = entry + n.
    fn steps(&mut self, n: u64) {
        let off = self.layout.steps;
        self.i(I::LocalGet(CPU));
        self.i(I::LocalGet(ENTRY));
        self.i(I::I64Const(n as i64));
        self.i(I::I64Add);
        self.i(I::I64Store(mem(off, 3)));
    }

    /// At the end of a block (pc and the step count written): go on to the
    /// next block if it is translated, fits before the stop, and no code was
    /// written; otherwise fall through (and return 0).
    fn chain(&mut self, c: &Chain, direct: &[(u32, u32)]) {
        let len = self.block.len() as i64;
        let next = self.block.next;
        self.i(I::Block(wasm_encoder::BlockType::Empty));
        // code written: no
        self.i(I::LocalGet(CTX));
        self.i(I::I32Load(mem(c.ctx_changed, 2)));
        self.i(I::BrIf(0));
        // SLOT = ctx.fast + ((next >> 2) % slots) * stride
        self.i(I::LocalGet(CTX));
        self.i(I::I32Load(mem(c.ctx_fast, 2)));
        self.val(next);
        self.i(I::I64Const(2));
        self.i(I::I64ShrU);
        self.i(I::I32WrapI64);
        self.i(I::I32Const(c.slots as i32 - 1));
        self.i(I::I32And);
        self.i(I::I32Const(c.stride as i32));
        self.i(I::I32Mul);
        self.i(I::I32Add);
        self.i(I::LocalSet(SLOT));
        // another block there: no
        self.i(I::LocalGet(SLOT));
        self.i(I::I64Load(mem(c.pc, 3)));
        self.val(next);
        self.i(I::I64Ne);
        self.i(I::BrIf(0));
        // it would end past the stop: no
        self.i(I::LocalGet(ENTRY));
        self.i(I::I64Const(len));
        self.i(I::I64Add);
        self.i(I::LocalGet(SLOT));
        self.i(I::I64Load(mem(c.len, 3)));
        self.i(I::I64Add);
        self.i(I::LocalGet(CTX));
        self.i(I::I64Load(mem(c.ctx_stop, 3)));
        self.i(I::I64GtU);
        self.i(I::BrIf(0));
        // ctx.block = its block; ctx.chained += 1
        self.i(I::LocalGet(CTX));
        self.i(I::LocalGet(SLOT));
        self.i(I::I32Load(mem(c.block, 2)));
        self.i(I::I32Store(mem(c.ctx_block, 2)));
        self.i(I::LocalGet(CTX));
        self.i(I::LocalGet(CTX));
        self.i(I::I64Load(mem(c.ctx_chained, 3)));
        self.i(I::I64Const(1));
        self.i(I::I64Add);
        self.i(I::I64Store(mem(c.ctx_chained, 3)));
        // A block of this module (the table says it is current): go round
        // the dispatch loop to it.
        for &(index, handle) in direct {
            self.i(I::LocalGet(SLOT));
            self.i(I::I32Load(mem(c.handle, 2)));
            self.i(I::I32Const(handle as i32));
            self.i(I::I32Eq);
            self.i(I::If(wasm_encoder::BlockType::Empty));
            self.i(I::I32Const(index as i32));
            self.i(I::LocalSet(INDEX));
            self.i(I::Br(self.to_loop + 2));
            self.i(I::End);
        }
        self.i(I::LocalGet(CPU));
        self.i(I::LocalGet(CTX));
        self.i(I::LocalGet(SLOT));
        self.i(I::I32Load(mem(c.handle, 2)));
        self.i(I::ReturnCallIndirect { type_index: BLOCK_TYPE, table_index: 0 });
        self.i(I::End);
    }

    /// Inside `block $done (block $slow …)`: branch to `$slow` unless the TLB
    /// has the page of `addr` for an access of `bytes` (see
    /// [`pomme_mem::TlbLayout`]); on a hit, push the host address.
    fn tlb_hit(&mut self, t: &InlineTlb, addr: V, bytes: u8, store: bool) {
        let shift = pomme_mem::PAGE_SIZE.trailing_zeros() as i64;
        let in_page = pomme_mem::PAGE_SIZE as i64 - 1;
        // ADDR = untag(addr)
        self.val(addr);
        self.i(I::I64Const(0x00ff_ffff_ffff_ffff));
        self.i(I::I64And);
        self.i(I::LocalSet(ADDR));
        // SLOT = ctx.tlb + (page % slots) * stride
        self.i(I::LocalGet(CTX));
        self.i(I::I32Load(mem(t.ctx, 2)));
        self.i(I::LocalGet(ADDR));
        self.i(I::I64Const(shift));
        self.i(I::I64ShrU);
        self.i(I::I32WrapI64);
        self.i(I::I32Const(t.slots as i32 - 1));
        self.i(I::I32And);
        self.i(I::I32Const(t.stride as i32));
        self.i(I::I32Mul);
        self.i(I::I32Add);
        self.i(I::LocalSet(SLOT));
        // entry.page != page: slow
        self.i(I::LocalGet(SLOT));
        self.i(I::I64Load(mem(t.page, 3)));
        self.i(I::LocalGet(ADDR));
        self.i(I::I64Const(shift));
        self.i(I::I64ShrU);
        self.i(I::I64Ne);
        self.i(I::BrIf(0));
        // entry.host == 0: slow
        self.i(I::LocalGet(SLOT));
        self.i(I::I32Load(mem(t.host, 2)));
        self.i(I::LocalTee(HOST));
        self.i(I::I32Eqz);
        self.i(I::BrIf(0));
        // runs past the page: slow
        self.i(I::LocalGet(ADDR));
        self.i(I::I64Const(in_page));
        self.i(I::I64And);
        self.i(I::I64Const(in_page + 1 - bytes as i64));
        self.i(I::I64GtU);
        self.i(I::BrIf(0));
        if store {
            self.i(I::LocalGet(SLOT));
            self.i(I::I32Load8U(mem(t.writable, 0)));
            self.i(I::I32Eqz);
            self.i(I::BrIf(0));
        }
        // host + addr % PAGE_SIZE
        self.i(I::LocalGet(HOST));
        self.i(I::LocalGet(ADDR));
        self.i(I::I32WrapI64);
        self.i(I::I32Const(in_page as i32));
        self.i(I::I32And);
        self.i(I::I32Add);
    }

    /// The code after a call that pushed an i32 "stopped" flag: if set, write
    /// back what `at` lists, move to its instruction and return 1.
    fn stop_if(&mut self, at: Option<&ExitPoint>) {
        self.i(I::If(wasm_encoder::BlockType::Empty));
        if let Some(at) = at {
            for &(loc, v) in &at.dirty {
                self.set(loc, |s| s.val(v));
            }
            self.at_insn(at.insn);
        }
        self.i(I::I32Const(1));
        self.i(I::Return);
        self.i(I::End);
    }
}

/// Where a block may go next, among the blocks of its module: its
/// fall-through and the constants it computes (a branch's target is one).
fn successors(block: &Block, here: &std::collections::HashMap<u64, (u32, u32)>) -> Vec<(u32, u32)> {
    let fall = block.pc.wrapping_add(4 * block.len() as u64);
    let mut out = Vec::new();
    let consts = block.insts.iter().filter_map(|i| match i {
        Inst::Const { value, .. } => Some(*value),
        _ => None,
    });
    for pc in std::iter::once(fall).chain(consts) {
        if let Some(&t) = here.get(&pc)
            && !out.contains(&t)
            && out.len() < 4
        {
            out.push(t);
        }
    }
    out
}

/// `run(cpu, ctx, index)`: every block of the module, picked by a dispatch
/// loop, so a block goes on to another of the module's blocks with a branch.
///
/// ```text
/// loop $dispatch
///   block $bN-1 … block $b0
///     br_table index        ;; to the end of $b<index>
///   end  <block 0's code>   ;; returns, or sets index and goes round
///   …
/// end
/// ```
fn run_function(blocks: &[&Block], layout: &Layout, here: &std::collections::HashMap<u64, (u32, u32)>) -> Function {
    let values = blocks.iter().map(|b| b.values).max().unwrap_or(0);
    let locals = [(2u32, ValType::I64), (2, ValType::I32), (values, ValType::I64)];
    let mut f = Function::new(locals);
    let n = blocks.len() as u32;
    f.instruction(&I::Loop(wasm_encoder::BlockType::Empty));
    for _ in 0..n {
        f.instruction(&I::Block(wasm_encoder::BlockType::Empty));
    }
    f.instruction(&I::LocalGet(INDEX));
    let targets: Vec<u32> = (0..n).collect();
    f.instruction(&I::BrTable(targets.into(), n.saturating_sub(1)));
    for (k, block) in blocks.iter().enumerate() {
        f.instruction(&I::End);
        let mut g = Gen { f, layout, block, to_loop: n - 1 - k as u32 };
        block_code(&mut g, here);
        f = g.f;
    }
    f.instruction(&I::End); // the loop
    f.instruction(&I::Unreachable);
    f.instruction(&I::End);
    f
}

/// One block's code, inside `run`: it ends by returning or going round the
/// dispatch loop.
fn block_code(g: &mut Gen, here: &std::collections::HashMap<u64, (u32, u32)>) {
    let (block, layout) = (g.block, g.layout);
    // entry = cpu.steps
    g.i(I::LocalGet(CPU));
    g.i(I::I64Load(mem(layout.steps, 3)));
    g.i(I::LocalSet(ENTRY));
    for inst in &block.insts {
        match inst {
            Inst::Const { dst, value } => {
                g.i(I::I64Const(*value as i64));
                g.def(*dst);
            }
            Inst::Get { dst, loc } => {
                g.get(*loc);
                g.def(*dst);
            }
            Inst::Set { loc, src } => {
                let src = *src;
                g.set(*loc, |s| s.val(src));
            }
            Inst::Bin { dst, op, a, b } => {
                g.val(*a);
                g.val(*b);
                match op {
                    BinOp::Add => g.i(I::I64Add),
                    BinOp::Sub => g.i(I::I64Sub),
                    BinOp::Mul => g.i(I::I64Mul),
                    BinOp::And => g.i(I::I64And),
                    BinOp::Or => g.i(I::I64Or),
                    BinOp::Xor => g.i(I::I64Xor),
                    // WebAssembly shifts take the amount modulo 64, as the evaluator does.
                    BinOp::Shl => g.i(I::I64Shl),
                    BinOp::Lshr => g.i(I::I64ShrU),
                    BinOp::Ashr => g.i(I::I64ShrS),
                    BinOp::Eq | BinOp::Ne | BinOp::Ult => {
                        g.i(match op {
                            BinOp::Eq => I::I64Eq,
                            BinOp::Ne => I::I64Ne,
                            _ => I::I64LtU,
                        });
                        g.i(I::I64ExtendI32U);
                    }
                }
                g.def(*dst);
            }
            Inst::Not { dst, a } => {
                g.val(*a);
                g.i(I::I64Const(-1));
                g.i(I::I64Xor);
                g.def(*dst);
            }
            Inst::Select { dst, cond, a, b } => {
                g.val(*a);
                g.val(*b);
                g.val(*cond);
                g.i(I::I64Const(0));
                g.i(I::I64Ne);
                g.i(I::Select);
                g.def(*dst);
            }
            Inst::Load { dst, addr, bytes, at } => {
                let tlb = g.layout.tlb;
                if let Some(t) = tlb {
                    g.i(I::Block(wasm_encoder::BlockType::Empty)); // done
                    g.i(I::Block(wasm_encoder::BlockType::Empty)); // slow
                    g.tlb_hit(&t, *addr, *bytes, false);
                    g.i(match bytes {
                        1 => I::I64Load8U(mem(0, 0)),
                        2 => I::I64Load16U(mem(0, 0)),
                        4 => I::I64Load32U(mem(0, 0)),
                        _ => I::I64Load(mem(0, 0)),
                    });
                    g.def(*dst);
                    g.i(I::Br(1));
                    g.i(I::End);
                }
                g.i(I::LocalGet(CTX));
                g.val(*addr);
                g.i(I::I32Const(*bytes as i32));
                g.i(I::Call(LOAD));
                g.def(*dst);
                // The context's first word says whether it faulted.
                g.i(I::LocalGet(CTX));
                g.i(I::I32Load(mem(0, 2)));
                g.stop_if(Some(at));
                if tlb.is_some() {
                    g.i(I::End);
                }
            }
            Inst::Store { addr, value, bytes, at } => {
                let tlb = g.layout.tlb;
                if let Some(t) = tlb {
                    g.i(I::Block(wasm_encoder::BlockType::Empty)); // done
                    g.i(I::Block(wasm_encoder::BlockType::Empty)); // slow
                    g.tlb_hit(&t, *addr, *bytes, true);
                    g.val(*value);
                    g.i(match bytes {
                        1 => I::I64Store8(mem(0, 0)),
                        2 => I::I64Store16(mem(0, 0)),
                        4 => I::I64Store32(mem(0, 0)),
                        _ => I::I64Store(mem(0, 0)),
                    });
                    g.i(I::Br(1));
                    g.i(I::End);
                }
                g.i(I::LocalGet(CTX));
                g.val(*addr);
                g.val(*value);
                g.i(I::I32Const(*bytes as i32));
                g.i(I::Call(STORE));
                g.stop_if(Some(at));
                if tlb.is_some() {
                    g.i(I::End);
                }
            }
            Inst::Helper { insn } => {
                // The helper runs the instruction from its own pc and step count.
                g.at_insn(*insn);
                g.i(I::LocalGet(CTX));
                g.i(I::I32Const(*insn as i32));
                g.i(I::Call(HELPER));
                g.stop_if(None);
            }
        }
    }
    let next = block.next;
    g.set(Loc::Pc, |s| s.val(next));
    g.steps(block.len() as u64);
    if let Some(c) = layout.chain {
        g.chain(&c, &successors(block, here));
    }
    g.i(I::I32Const(0));
    g.i(I::Return);
}

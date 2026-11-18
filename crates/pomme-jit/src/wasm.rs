//! The WebAssembly back end: IR blocks to one WebAssembly module.
//!
//! The module imports pomme's own memory (the `Cpu` struct lives in it) and
//! three helpers from pomme. Each block becomes one exported function
//! `b<i>(cpu, ctx) -> status`:
//!
//! - `cpu` is the address of the `Cpu` struct, with the registers and flags
//!   at the offsets in [`Layout`]. The module keeps them in locals while its
//!   blocks run one after another, and writes them back on the way out.
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
    code.function(&run_function(blocks, layout, first_handle));

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
    /// Guest registers and flags live in locals while `run` runs (see
    /// [`Regs`]); the `Cpu` struct is only up to date on the way out.
    regs: &'a Regs,
    /// How many blocks the module has.
    blocks: u32,
}

/// The guest state `run` keeps in locals: the registers and flags any of
/// its blocks reads or writes, loaded when `run` starts and written back
/// whenever code outside it may look (an exit, a helper, another module).
/// The step count lives in a local too.
struct Regs {
    used: Vec<Loc>,
    /// The ones some block writes: only these can differ from the `Cpu` struct.
    written: Vec<Loc>,
    /// The local of X0; X1…X30, SP, N, Z, C and V follow it, then the step
    /// count, then the pc to leave with (i64), the status to return and the
    /// table index to go on to (i32).
    base: u32,
}

impl Regs {
    const COUNT: u32 = 36;

    fn index(loc: Loc) -> u32 {
        match loc {
            Loc::X(r) => r as u32,
            Loc::Sp => 31,
            Loc::N => 32,
            Loc::Z => 33,
            Loc::C => 34,
            Loc::Vf => 35,
            Loc::Pc => unreachable!("pc stays in the Cpu struct"),
        }
    }

    fn local(&self, loc: Loc) -> u32 {
        self.base + Self::index(loc)
    }

    fn steps(&self) -> u32 {
        self.base + Self::COUNT
    }

    fn pc_out(&self) -> u32 {
        self.base + Self::COUNT + 1
    }

    fn status(&self) -> u32 {
        self.base + Self::COUNT + 2
    }

    fn handle(&self) -> u32 {
        self.base + Self::COUNT + 3
    }

    fn new(blocks: &[&Block], base: u32) -> Regs {
        let mut seen = [false; Self::COUNT as usize];
        let mut wrote = [false; Self::COUNT as usize];
        let mut mark = |loc: Loc, write: bool| {
            if loc != Loc::Pc {
                seen[Self::index(loc) as usize] = true;
                wrote[Self::index(loc) as usize] |= write;
            }
        };
        for b in blocks {
            for inst in &b.insts {
                match inst {
                    Inst::Get { loc, .. } => mark(*loc, false),
                    Inst::Set { loc, .. } => mark(*loc, true),
                    Inst::Load { at, .. } | Inst::Store { at, .. } => at.dirty.iter().for_each(|(loc, _)| mark(*loc, true)),
                    _ => {}
                }
            }
        }
        let all = || (0..31).map(Loc::X).chain([Loc::Sp, Loc::N, Loc::Z, Loc::C, Loc::Vf]);
        Regs { used: all().filter(|l| seen[Self::index(*l) as usize]).collect(), written: all().filter(|l| wrote[Self::index(*l) as usize]).collect(), base }
    }
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
        if loc != Loc::Pc {
            self.i(I::LocalGet(self.regs.local(loc)));
            return;
        }
        self.load_from_cpu(loc);
    }

    /// Store the value pushed by `push` into guest state `loc`.
    fn set(&mut self, loc: Loc, push: impl FnOnce(&mut Self)) {
        if loc == Loc::Pc {
            return self.store_to_cpu(loc, push);
        }
        push(self);
        if matches!(loc, Loc::N | Loc::Z | Loc::C | Loc::Vf) {
            // A flag is a bool: 1 for any value that isn't 0.
            self.i(I::I64Const(0));
            self.i(I::I64Ne);
            self.i(I::I64ExtendI32U);
        }
        self.i(I::LocalSet(self.regs.local(loc)));
    }

    /// Load the registers `run` keeps in locals from the `Cpu` struct.
    fn load_regs(&mut self) {
        for loc in self.regs.used.clone() {
            self.load_from_cpu(loc);
            self.i(I::LocalSet(self.regs.local(loc)));
        }
    }

    /// Write the registers `run` keeps in locals (the ones it may have
    /// changed) back to the `Cpu` struct.
    fn store_regs(&mut self) {
        let l = *self.layout;
        for loc in self.regs.written.clone() {
            self.i(I::LocalGet(CPU));
            self.i(I::LocalGet(self.regs.local(loc)));
            // Flags in locals are already 0 or 1.
            self.i(match loc {
                Loc::X(r) => I::I64Store(mem(l.x + 8 * r as u32, 3)),
                Loc::Sp => I::I64Store(mem(l.sp, 3)),
                Loc::N => I::I64Store8(mem(l.n, 0)),
                Loc::Z => I::I64Store8(mem(l.z, 0)),
                Loc::C => I::I64Store8(mem(l.c, 0)),
                _ => I::I64Store8(mem(l.v, 0)),
            });
        }
    }

    /// Push guest state `loc` from the `Cpu` struct.
    fn load_from_cpu(&mut self, loc: Loc) {
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

    /// Store the value pushed by `push` into `loc` in the `Cpu` struct.
    fn store_to_cpu(&mut self, loc: Loc, push: impl FnOnce(&mut Self)) {
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
    fn chain(&mut self, c: &Chain, first_handle: Option<u32>) {
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
        let _ = len;
        self.i(I::LocalGet(self.regs.steps()));
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
        self.i(I::LocalGet(SLOT));
        self.i(I::I32Load(mem(c.handle, 2)));
        self.i(I::LocalSet(self.regs.handle()));
        // A block of this module (its table index is one of the module's):
        // go round the dispatch loop to it, with the registers in locals.
        if let Some(first) = first_handle {
            self.i(I::LocalGet(self.regs.handle()));
            self.i(I::I32Const(first as i32));
            self.i(I::I32Sub);
            self.i(I::LocalTee(INDEX));
            self.i(I::I32Const(self.blocks as i32));
            self.i(I::I32LtU);
            self.i(I::BrIf(self.to_loop + 1));
        }
        // Another module's block: it reads the state from the Cpu struct
        // (the shared exit after the loop writes it back).
        self.val(next);
        self.i(I::LocalSet(self.regs.pc_out()));
        self.i(I::Br(self.to_loop + 3));
        self.i(I::End);
    }

    /// Leave `run` through its shared exit (which writes the registers, the
    /// step count and pc back), returning `status`; `depth` is how many
    /// labels the code is inside, beyond the block's own level.
    fn leave(&mut self, status: i32, depth: u32) {
        self.i(I::I32Const(status));
        self.i(I::LocalSet(self.regs.status()));
        self.i(I::Br(self.to_loop + 1 + depth));
    }

    /// The code after a call that pushed an i32 "stopped" flag: if set, write
    /// back what `at` lists, move to its instruction and return 1. Without
    /// `at` (a helper, which wrote the `Cpu` struct itself), just return 1.
    fn stop_if(&mut self, at: Option<&ExitPoint>, depth: u32) {
        self.i(I::If(wasm_encoder::BlockType::Empty));
        if let Some(at) = at {
            for &(loc, v) in &at.dirty {
                self.set(loc, |s| s.val(v));
            }
            let pc = self.block.pc.wrapping_add(4 * at.insn as u64);
            self.i(I::I64Const(pc as i64));
            self.i(I::LocalSet(self.regs.pc_out()));
            self.i(I::LocalGet(ENTRY));
            self.i(I::I64Const(at.insn as i64));
            self.i(I::I64Add);
            self.i(I::LocalSet(self.regs.steps()));
            self.leave(1, depth + 1);
        } else {
            self.i(I::I32Const(1));
            self.i(I::Return);
        }
        self.i(I::End);
    }
}

/// Inside `block $slow`: branch to it unless the TLB has the page of the
/// address in local `addr` for an access of `bytes` (see
/// [`pomme_mem::TlbLayout`]); on a hit, push the host address.
fn tlb_hit(f: &mut Function, t: &InlineTlb, addr: u32, bytes: u8, store: bool) {
    let shift = pomme_mem::PAGE_SIZE.trailing_zeros() as i64;
    let in_page = pomme_mem::PAGE_SIZE as i64 - 1;
    // ADDR = untag(addr)
    f.instruction(&I::LocalGet(addr));
    f.instruction(&I::I64Const(0x00ff_ffff_ffff_ffff));
    f.instruction(&I::I64And);
    f.instruction(&I::LocalSet(ADDR));
    // SLOT = ctx.tlb + (page % slots) * stride
    f.instruction(&I::LocalGet(CTX));
    f.instruction(&I::I32Load(mem(t.ctx, 2)));
    f.instruction(&I::LocalGet(ADDR));
    f.instruction(&I::I64Const(shift));
    f.instruction(&I::I64ShrU);
    f.instruction(&I::I32WrapI64);
    f.instruction(&I::I32Const(t.slots as i32 - 1));
    f.instruction(&I::I32And);
    f.instruction(&I::I32Const(t.stride as i32));
    f.instruction(&I::I32Mul);
    f.instruction(&I::I32Add);
    f.instruction(&I::LocalSet(SLOT));
    // entry.page != page: slow
    f.instruction(&I::LocalGet(SLOT));
    f.instruction(&I::I64Load(mem(t.page, 3)));
    f.instruction(&I::LocalGet(ADDR));
    f.instruction(&I::I64Const(shift));
    f.instruction(&I::I64ShrU);
    f.instruction(&I::I64Ne);
    f.instruction(&I::BrIf(0));
    // entry.host == 0: slow
    f.instruction(&I::LocalGet(SLOT));
    f.instruction(&I::I32Load(mem(t.host, 2)));
    f.instruction(&I::LocalTee(HOST));
    f.instruction(&I::I32Eqz);
    f.instruction(&I::BrIf(0));
    // runs past the page: slow
    f.instruction(&I::LocalGet(ADDR));
    f.instruction(&I::I64Const(in_page));
    f.instruction(&I::I64And);
    f.instruction(&I::I64Const(in_page + 1 - bytes as i64));
    f.instruction(&I::I64GtU);
    f.instruction(&I::BrIf(0));
    if store {
        f.instruction(&I::LocalGet(SLOT));
        f.instruction(&I::I32Load8U(mem(t.writable, 0)));
        f.instruction(&I::I32Eqz);
        f.instruction(&I::BrIf(0));
    }
    // host + addr % PAGE_SIZE
    f.instruction(&I::LocalGet(HOST));
    f.instruction(&I::LocalGet(ADDR));
    f.instruction(&I::I32WrapI64);
    f.instruction(&I::I32Const(in_page as i32));
    f.instruction(&I::I32And);
    f.instruction(&I::I32Add);
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
fn run_function(blocks: &[&Block], layout: &Layout, first_handle: Option<u32>) -> Function {
    let values = blocks.iter().map(|b| b.values).max().unwrap_or(0);
    let locals = [(2u32, ValType::I64), (2, ValType::I32), (values + Regs::COUNT + 2, ValType::I64), (2, ValType::I32)];
    let regs = Regs::new(blocks, FIRST_VALUE + values);
    let mut f = Function::new(locals);
    let n = blocks.len() as u32;
    // The registers and the step count into locals.
    if let Some(first) = blocks.first() {
        let mut g = Gen { f, layout, block: first, to_loop: 0, regs: &regs, blocks: n };
        g.load_regs();
        g.i(I::LocalGet(CPU));
        g.i(I::I64Load(mem(layout.steps, 3)));
        g.i(I::LocalSet(regs.steps()));
        f = g.f;
    }
    f.instruction(&I::Block(wasm_encoder::BlockType::Empty)); // $chain_out
    f.instruction(&I::Block(wasm_encoder::BlockType::Empty)); // $out
    f.instruction(&I::Loop(wasm_encoder::BlockType::Empty));
    for _ in 0..n {
        f.instruction(&I::Block(wasm_encoder::BlockType::Empty));
    }
    f.instruction(&I::LocalGet(INDEX));
    let targets: Vec<u32> = (0..n).collect();
    f.instruction(&I::BrTable(targets.into(), n.saturating_sub(1)));
    for (k, block) in blocks.iter().enumerate() {
        f.instruction(&I::End);
        let mut g = Gen { f, layout, block, to_loop: n - 1 - k as u32, regs: &regs, blocks: n };
        block_code(&mut g, first_handle);
        f = g.f;
    }
    f.instruction(&I::End); // the loop
    f.instruction(&I::Unreachable);
    // $out and $chain_out: the registers, pc and the step count back to the
    // Cpu struct; then return the status, or go on to the block at `handle`.
    let write_back = |f: Function| {
        let mut g = Gen { f, layout, block: blocks[0], to_loop: 0, regs: &regs, blocks: n };
        g.store_regs();
        g.i(I::LocalGet(CPU));
        g.i(I::LocalGet(regs.pc_out()));
        g.i(I::I64Store(mem(layout.pc, 3)));
        g.i(I::LocalGet(CPU));
        g.i(I::LocalGet(regs.steps()));
        g.i(I::I64Store(mem(layout.steps, 3)));
        g.f
    };
    f.instruction(&I::End); // $out
    if !blocks.is_empty() {
        f = write_back(f);
    }
    f.instruction(&I::LocalGet(regs.status()));
    f.instruction(&I::Return);
    f.instruction(&I::End); // $chain_out
    if layout.chain.is_some() && !blocks.is_empty() {
        f = write_back(f);
        f.instruction(&I::LocalGet(CPU));
        f.instruction(&I::LocalGet(CTX));
        f.instruction(&I::LocalGet(regs.handle()));
        f.instruction(&I::ReturnCallIndirect { type_index: BLOCK_TYPE, table_index: 0 });
    } else {
        f.instruction(&I::Unreachable);
    }
    f.instruction(&I::End);
    f
}

/// One block's code, inside `run`: it ends by returning or going round the
/// dispatch loop.
fn block_code(g: &mut Gen, first_handle: Option<u32>) {
    let (block, layout) = (g.block, g.layout);
    // entry = the step count
    g.i(I::LocalGet(g.regs.steps()));
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
                // Straight from memory when the TLB has the page; otherwise
                // through pomme, which sets the context's first word if it faulted.
                g.i(I::Block(wasm_encoder::BlockType::Result(ValType::I64)));
                if let Some(t) = layout.tlb {
                    g.i(I::Block(wasm_encoder::BlockType::Empty));
                    tlb_hit(&mut g.f, &t, FIRST_VALUE + addr.0, *bytes, false);
                    g.i(match bytes {
                        1 => I::I64Load8U(mem(0, 0)),
                        2 => I::I64Load16U(mem(0, 0)),
                        4 => I::I64Load32U(mem(0, 0)),
                        _ => I::I64Load(mem(0, 0)),
                    });
                    g.i(I::Br(1));
                    g.i(I::End);
                }
                g.i(I::LocalGet(CTX));
                g.val(*addr);
                g.i(I::I32Const(*bytes as i32));
                g.i(I::Call(LOAD));
                g.i(I::LocalGet(CTX));
                g.i(I::I32Load(mem(0, 2)));
                g.stop_if(Some(at), 1);
                g.i(I::End);
                g.def(*dst);
            }
            Inst::Store { addr, value, bytes, at } => {
                g.i(I::Block(wasm_encoder::BlockType::Empty));
                if let Some(t) = layout.tlb {
                    g.i(I::Block(wasm_encoder::BlockType::Empty));
                    tlb_hit(&mut g.f, &t, FIRST_VALUE + addr.0, *bytes, true);
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
                g.stop_if(Some(at), 1);
                g.i(I::End);
            }
            Inst::Helper { insn } => {
                // The helper runs the instruction on the Cpu struct, from
                // its own pc and step count, and may change any register.
                g.store_regs();
                g.at_insn(*insn);
                g.i(I::LocalGet(CTX));
                g.i(I::I32Const(*insn as i32));
                g.i(I::Call(HELPER));
                g.stop_if(None, 0);
                g.load_regs();
            }
        }
    }
    // The step count after the block, then on to the next block or out.
    g.i(I::LocalGet(ENTRY));
    g.i(I::I64Const(block.len() as i64));
    g.i(I::I64Add);
    g.i(I::LocalSet(g.regs.steps()));
    if let Some(c) = layout.chain {
        g.chain(&c, first_handle);
    }
    g.val(block.next);
    g.i(I::LocalSet(g.regs.pc_out()));
    g.leave(0, 0);
}

//! A user-mode arm64 (AArch64) interpreter.
//!
//! Executes guest code out of [`Memory`] one instruction at a time and returns
//! an [`Exit`] whenever it needs the host: a `BRK` (HLE stub), an `SVC`, a
//! fault, an instruction we don't implement yet, or a return to the host.
//!
//! Deterministic by construction: no host time, threads or randomness; the
//! instruction counter is the only clock (see README "Design constraints").

mod block;
mod branch;
mod fast;
mod dp;
mod fp;
mod ldst;
mod neon;

use pomme_mem::{MemError, Memory};

/// Point LR here to call into guest code: returning to it exits with [`Exit::Returned`].
pub const HOST_RETURN: u64 = 0xffff_ffff_ffff_f000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// The guest returned to [`HOST_RETURN`].
    Returned,
    /// `BRK #imm` at `pc` (pc is left pointing at the BRK).
    Brk { imm: u16 },
    /// `SVC #imm`; pc has already advanced past it, like the hardware.
    Svc { imm: u16 },
    /// An instruction we can't execute yet (pc points at it).
    Unimplemented { insn: u32 },
    /// A load, store or fetch hit unmapped memory.
    Fault { addr: u64 },
    /// `run` hit its step budget.
    StepLimit,
    /// `steps` reached [`Cpu::sample_at`]: a profiler's turn (pc is the next instruction).
    Sample,
}

impl From<MemError> for Exit {
    fn from(e: MemError) -> Self {
        match e {
            MemError::Unmapped(addr) => Exit::Fault { addr },
            MemError::Overlap { base, .. } => Exit::Fault { addr: base },
        }
    }
}

type Step = Result<(), Exit>;

#[derive(Debug, Clone, Default)]
pub struct Cpu {
    /// X0..X30. Index 31 is never stored: it means XZR or SP depending on the instruction.
    pub x: [u64; 31],
    pub sp: u64,
    pub pc: u64,
    pub n: bool,
    pub z: bool,
    pub c: bool,
    pub v: bool,
    /// V0..V31 (128-bit SIMD/FP registers).
    pub q: [u128; 32],
    pub fpcr: u64,
    pub fpsr: u64,
    /// Thread pointers. iOS keeps the TLS base in TPIDRRO_EL0.
    pub tpidr_el0: u64,
    pub tpidrro_el0: u64,
    /// Instructions retired. The guest's only clock (see [`Cpu::ticks`]).
    pub steps: u64,
    /// Ticks added to the clock when every thread was idle (virtual time skipped ahead).
    pub clock_offset: u64,
    /// When `steps` reaches this, `run` stops with [`Exit::Sample`] (0: never).
    pub sample_at: u64,
    /// Code fetched so far, in basic blocks (see block.rs). Shared by every
    /// thread: whoever switches threads moves it to the incoming CPU.
    pub blocks: block::Blocks,
    /// Address held by the exclusive monitor (LDXR/STXR).
    exclusive: Option<u64>,
    /// Next pc, set by branches during a step.
    next_pc: u64,
}

/// The virtual clock: mach ticks run at 24 MHz (as on iOS devices) and each
/// instruction takes 1 ns, a 1 GHz phone. Deterministic: only instructions
/// retired (plus idle time skipped) move it.
pub const TICKS_PER_SEC: u64 = 24_000_000;
pub const STEPS_PER_SEC: u64 = 1_000_000_000;

impl Cpu {
    /// Mach ticks since boot: instructions retired at [`STEPS_PER_SEC`], plus idle time skipped.
    pub fn ticks(&self) -> u64 {
        (self.steps as u128 * TICKS_PER_SEC as u128 / STEPS_PER_SEC as u128) as u64 + self.clock_offset
    }

    pub fn new() -> Self {
        Self::default()
    }

    /// Run until something needs the host, or `max_steps` instructions retire.
    pub fn run(&mut self, mem: &mut Memory, max_steps: u64) -> Exit {
        let limit = self.steps.saturating_add(max_steps);
        // One check covers both the budget and the next profiler sample.
        let stop = if self.sample_at == 0 { limit } else { limit.min(self.sample_at) };
        let mut blocks = std::mem::take(&mut self.blocks);
        let exit = self.run_blocks(&mut blocks, mem, stop);
        self.blocks = blocks;
        exit
    }

    fn run_blocks(&mut self, blocks: &mut block::Blocks, mem: &mut Memory, stop: u64) -> Exit {
        loop {
            if self.pc == HOST_RETURN {
                return Exit::Returned;
            }
            if self.steps >= stop {
                return if self.sample_at != 0 && self.steps >= self.sample_at { Exit::Sample } else { Exit::StepLimit };
            }
            // A whole block when it fits before the stop; otherwise one instruction at a time.
            let room = stop - self.steps;
            match blocks.get(self.pc, mem) {
                Some(b) if b.len() as u64 <= room => {
                    for op in b {
                        if let Err(exit) = self.exec_op(op, mem) {
                            return exit;
                        }
                    }
                }
                _ => {
                    if let Err(exit) = self.step(mem) {
                        return exit;
                    }
                }
            }
        }
    }

    /// Execute one instruction.
    pub fn step(&mut self, mem: &mut Memory) -> Step {
        let insn = mem.read_u32(self.pc)?;
        self.exec(insn, mem)
    }

    /// Execute `insn`, the instruction at pc.
    #[inline(always)]
    fn exec(&mut self, insn: u32, mem: &mut Memory) -> Step {
        self.next_pc = self.pc.wrapping_add(4);
        let r = match (insn >> 25) & 0xf {
            0b1000 | 0b1001 => self.dp_imm(insn),
            0b1010 | 0b1011 => self.branch_sys(insn, mem),
            0b0100 | 0b0110 | 0b1100 | 0b1110 => self.ldst(insn, mem),
            0b0101 | 0b1101 => self.dp_reg(insn),
            0b0111 | 0b1111 => self.simd_fp(insn),
            _ => Err(Exit::Unimplemented { insn }),
        };
        match r {
            Ok(()) => {
                self.pc = self.next_pc;
                self.steps += 1;
                Ok(())
            }
            Err(Exit::Svc { imm }) => {
                self.pc = self.next_pc;
                self.steps += 1;
                Err(Exit::Svc { imm })
            }
            Err(e) => Err(e),
        }
    }

    // ---- register access ----

    /// Xn, where 31 is XZR.
    #[inline]
    pub fn xr(&self, r: u32) -> u64 {
        if r == 31 { 0 } else { self.x[r as usize] }
    }

    /// Xn, where 31 is SP.
    #[inline]
    pub fn xsp(&self, r: u32) -> u64 {
        if r == 31 { self.sp } else { self.x[r as usize] }
    }

    #[inline]
    pub fn set_xr(&mut self, r: u32, v: u64) {
        if r != 31 {
            self.x[r as usize] = v;
        }
    }

    #[inline]
    pub fn set_xsp(&mut self, r: u32, v: u64) {
        if r == 31 { self.sp = v } else { self.x[r as usize] = v }
    }

    /// Read a register as 32 or 64 bits (zero-extended).
    #[inline]
    fn rd(&self, r: u32, sf: bool) -> u64 {
        let v = self.xr(r);
        if sf { v } else { v as u32 as u64 }
    }

    /// Write a 32- or 64-bit result (32-bit results zero the top half).
    #[inline]
    fn wr(&mut self, r: u32, sf: bool, v: u64) {
        self.set_xr(r, if sf { v } else { v as u32 as u64 });
    }

    #[inline]
    fn wr_sp(&mut self, r: u32, sf: bool, v: u64) {
        self.set_xsp(r, if sf { v } else { v as u32 as u64 });
    }

    pub fn nzcv(&self) -> u64 {
        ((self.n as u64) << 31) | ((self.z as u64) << 30) | ((self.c as u64) << 29) | ((self.v as u64) << 28)
    }

    pub fn set_nzcv(&mut self, v: u64) {
        self.n = v & (1 << 31) != 0;
        self.z = v & (1 << 30) != 0;
        self.c = v & (1 << 29) != 0;
        self.v = v & (1 << 28) != 0;
    }

    fn set_nzcv4(&mut self, f: u32) {
        self.set_nzcv((f as u64 & 0xf) << 28);
    }

    /// Evaluate a 4-bit condition code.
    fn cond(&self, cond: u32) -> bool {
        let r = match cond >> 1 {
            0 => self.z,
            1 => self.c,
            2 => self.n,
            3 => self.v,
            4 => self.c && !self.z,
            5 => self.n == self.v,
            6 => !self.z && self.n == self.v,
            _ => true,
        };
        if cond & 1 == 1 && cond != 0xf { !r } else { r }
    }

    /// AddWithCarry, setting flags if `s`. Returns the (masked) result.
    fn add_with_carry(&mut self, a: u64, b: u64, carry: bool, sf: bool, s: bool) -> u64 {
        let (res, c, v) = if sf {
            let (r1, c1) = a.overflowing_add(b);
            let (r, c2) = r1.overflowing_add(carry as u64);
            let v = ((a ^ r) & (b ^ r)) >> 63 != 0;
            (r, c1 || c2, v)
        } else {
            let (a, b) = (a as u32, b as u32);
            let (r1, c1) = a.overflowing_add(b);
            let (r, c2) = r1.overflowing_add(carry as u32);
            let v = ((a ^ r) & (b ^ r)) >> 31 != 0;
            (r as u64, c1 || c2, v)
        };
        if s {
            self.set_nz(res, sf);
            self.c = c;
            self.v = v;
        }
        res
    }

    fn set_nz(&mut self, res: u64, sf: bool) {
        if sf {
            self.n = res >> 63 != 0;
            self.z = res == 0;
        } else {
            self.n = (res >> 31) & 1 != 0;
            self.z = res as u32 == 0;
        }
    }
}

// ---- bit helpers shared by the decoders ----

#[inline]
pub(crate) fn bits(insn: u32, hi: u32, lo: u32) -> u32 {
    (insn >> lo) & ((1u32 << (hi - lo + 1)) - 1)
}

#[inline]
pub(crate) fn bit(insn: u32, b: u32) -> bool {
    (insn >> b) & 1 != 0
}

#[inline]
pub(crate) fn sext(v: u64, width: u32) -> i64 {
    let shift = 64 - width;
    ((v << shift) as i64) >> shift
}

#[inline]
pub(crate) fn ones(n: u32) -> u64 {
    if n >= 64 { u64::MAX } else { (1u64 << n) - 1 }
}

#[inline]
pub(crate) fn ror(v: u64, amount: u32, width: u32) -> u64 {
    let amount = amount % width;
    let v = v & ones(width);
    if amount == 0 { v } else { ((v >> amount) | (v << (width - amount))) & ones(width) }
}

fn replicate(v: u64, esize: u32, width: u32) -> u64 {
    let mut out = 0u64;
    let mut i = 0;
    while i < width {
        out |= v << i;
        i += esize;
    }
    out & ones(width)
}

/// DecodeBitMasks from the Arm ARM. Returns (wmask, tmask), or None if reserved.
pub(crate) fn decode_bit_masks(n: u32, imms: u32, immr: u32, immediate: bool, width: u32) -> Option<(u64, u64)> {
    let combined = (n << 6) | (!imms & 0x3f);
    if combined == 0 {
        return None;
    }
    let len = 31 - combined.leading_zeros();
    if len < 1 {
        return None;
    }
    let levels = ones(len) as u32;
    if immediate && (imms & levels) == levels {
        return None;
    }
    let s = imms & levels;
    let r = immr & levels;
    let esize = 1u32 << len;
    if esize > width {
        return None;
    }
    let d = (s.wrapping_sub(r)) & levels;
    let welem = ones(s + 1);
    let telem = ones(d + 1);
    let wmask = replicate(ror(welem, r, esize), esize, width);
    let tmask = replicate(telem, esize, width);
    Some((wmask, tmask))
}

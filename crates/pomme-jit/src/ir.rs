//! The IR: a straight list of simple instructions for one guest block.
//!
//! Every value is 64 bits ([`V`] names one, defined once). Guest state is
//! read and written through [`Loc`]s. The front end keeps guest registers and
//! flags in values inside the block and writes them back only where it must
//! (before a helper call and at the end), so most flag results never reach
//! the `Cpu` struct. Loads and stores can fault: each carries an
//! [`ExitPoint`] that says which guest instruction it belongs to and which
//! values must be written back first, so the state at a fault is exactly the
//! interpreter's.

use pomme_cpu::fast::Op;

/// A value: the result of one instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct V(pub u32);

/// A piece of guest state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Loc {
    /// X0..X30.
    X(u8),
    Sp,
    /// The flags, each 0 or 1.
    N,
    Z,
    C,
    /// The overflow flag.
    Vf,
    /// The pc (only read, after a helper that may have branched).
    Pc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    And,
    Or,
    Xor,
    /// Shifts by less than 64 (the front end never makes a larger amount).
    Shl,
    Lshr,
    Ashr,
    /// Compares: 1 or 0.
    Eq,
    Ne,
    /// Unsigned less than.
    Ult,
}

/// Where a load or store may stop the block: guest instruction `insn` (its
/// index in the block) faults, after `dirty` is written back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExitPoint {
    pub insn: u16,
    pub dirty: Vec<(Loc, V)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inst {
    Const { dst: V, value: u64 },
    Get { dst: V, loc: Loc },
    Set { loc: Loc, src: V },
    Bin { dst: V, op: BinOp, a: V, b: V },
    Not { dst: V, a: V },
    /// `dst = if cond != 0 { a } else { b }`.
    Select { dst: V, cond: V, a: V, b: V },
    /// Load `bytes` (1, 2, 4 or 8), zero-extended.
    Load { dst: V, addr: V, bytes: u8, at: ExitPoint },
    /// Store the low `bytes` of `value`.
    Store { addr: V, value: V, bytes: u8, at: ExitPoint },
    /// Run guest instruction `insn` with the interpreter's own code. Every
    /// value of guest state is written back before it, and read again after.
    /// The helper sets pc and the step count to that instruction first; if
    /// the instruction stops (an exit or a fault), the block stops with it.
    Helper { insn: u16 },
}

/// One translated block.
#[derive(Clone, Debug)]
pub struct Block {
    /// The guest address of the first instruction.
    pub pc: u64,
    /// The decoded guest instructions (helpers run these).
    pub ops: Box<[Op]>,
    pub insts: Vec<Inst>,
    /// The pc after the block (it runs whole: the step count grows by `ops.len()`).
    pub next: V,
    /// How many values the block defines.
    pub values: u32,
}

impl Block {
    /// The number of guest instructions.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// How many guest instructions run as helper calls.
    pub fn helpers(&self) -> usize {
        self.insts.iter().filter(|i| matches!(i, Inst::Helper { .. })).count()
    }
}

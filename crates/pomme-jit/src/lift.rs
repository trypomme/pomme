//! The front end: arm64 block (as decoded [`Op`]s) to IR.
//!
//! Each `Op` form becomes the IR that does exactly what the interpreter does
//! for it (crates/pomme-cpu/src/fast.rs, `exec_op`). Forms without a
//! translation become a [`Inst::Helper`] call, so every block can be lifted.
//! `tests/lift.rs` checks every form against the interpreter.

use crate::ir::{BinOp, Block, ExitPoint, Inst, Loc, V};
use pomme_cpu::fast::{Addr, Kind, Op};
use std::collections::HashMap;

/// Lift the block `ops`, which starts at `pc`.
pub fn lift(pc: u64, ops: Box<[Op]>) -> Block {
    let mut l = Lifter { insts: Vec::new(), values: 0, cache: HashMap::new(), order: Vec::new(), insn: 0 };
    let mut next = None;
    for (i, op) in ops.iter().enumerate() {
        l.insn = i as u16;
        let at = pc.wrapping_add(4 * i as u64);
        next = l.op(op, at);
    }
    let next = match next {
        Some(v) => v,
        None => {
            // The block ended at a page end or its length limit, or on a helper.
            let last = ops.last().map(|o| matches!(o, Op::Generic(_))).unwrap_or(false);
            if last {
                l.get(Loc::Pc)
            } else {
                l.c(pc.wrapping_add(4 * ops.len() as u64))
            }
        }
    };
    l.flush();
    let insts = without_dead_values(l.insts, next, l.values);
    Block { pc, ops, insts, next, values: l.values }
}

/// Drop the instructions whose value nothing uses (most flag results:
/// the next instruction that sets the flags replaces them). Loads stay,
/// since they can fault.
fn without_dead_values(insts: Vec<Inst>, next: V, values: u32) -> Vec<Inst> {
    let mut used = vec![false; values as usize];
    used[next.0 as usize] = true;
    let mut keep = vec![true; insts.len()];
    for (i, inst) in insts.iter().enumerate().rev() {
        let pure_dst = match inst {
            Inst::Const { dst, .. } | Inst::Get { dst, .. } | Inst::Bin { dst, .. } | Inst::Not { dst, .. } | Inst::Select { dst, .. } => Some(*dst),
            _ => None,
        };
        if let Some(dst) = pure_dst
            && !used[dst.0 as usize]
        {
            keep[i] = false;
            continue;
        }
        let mut operands: Vec<V> = Vec::new();
        match inst {
            Inst::Const { .. } | Inst::Get { .. } | Inst::Helper { .. } => {}
            Inst::Bin { a, b, .. } => operands.extend([*a, *b]),
            Inst::Not { a, .. } => operands.push(*a),
            Inst::Select { cond, a, b, .. } => operands.extend([*cond, *a, *b]),
            Inst::Set { src, .. } => operands.push(*src),
            Inst::Load { addr, at, .. } => {
                operands.push(*addr);
                operands.extend(at.dirty.iter().map(|(_, v)| *v));
            }
            Inst::Store { addr, value, at, .. } => {
                operands.extend([*addr, *value]);
                operands.extend(at.dirty.iter().map(|(_, v)| *v));
            }
        }
        for v in operands {
            used[v.0 as usize] = true;
        }
    }
    insts.into_iter().zip(keep).filter_map(|(inst, k)| k.then_some(inst)).collect()
}

struct Lifter {
    insts: Vec<Inst>,
    values: u32,
    /// Guest state held in values: the value, and whether the `Cpu` struct is behind it.
    cache: HashMap<Loc, (V, bool)>,
    /// The order locations were first written (so write-backs are in a fixed order).
    order: Vec<Loc>,
    /// The guest instruction being lifted (its index in the block).
    insn: u16,
}

impl Lifter {
    fn new_v(&mut self) -> V {
        self.values += 1;
        V(self.values - 1)
    }

    fn c(&mut self, value: u64) -> V {
        let dst = self.new_v();
        self.insts.push(Inst::Const { dst, value });
        dst
    }

    fn bin(&mut self, op: BinOp, a: V, b: V) -> V {
        let dst = self.new_v();
        self.insts.push(Inst::Bin { dst, op, a, b });
        dst
    }

    fn bin_c(&mut self, op: BinOp, a: V, b: u64) -> V {
        let b = self.c(b);
        self.bin(op, a, b)
    }

    fn not(&mut self, a: V) -> V {
        let dst = self.new_v();
        self.insts.push(Inst::Not { dst, a });
        dst
    }

    fn select(&mut self, cond: V, a: V, b: V) -> V {
        let dst = self.new_v();
        self.insts.push(Inst::Select { dst, cond, a, b });
        dst
    }

    // ---- guest state ----

    fn get(&mut self, loc: Loc) -> V {
        if let Some(&(v, _)) = self.cache.get(&loc) {
            return v;
        }
        let dst = self.new_v();
        self.insts.push(Inst::Get { dst, loc });
        self.cache.insert(loc, (dst, false));
        dst
    }

    fn set(&mut self, loc: Loc, v: V) {
        if !self.order.contains(&loc) {
            self.order.push(loc);
        }
        self.cache.insert(loc, (v, true));
    }

    /// The values the `Cpu` struct is behind on, in a fixed order.
    fn dirty(&self) -> Vec<(Loc, V)> {
        self.order
            .iter()
            .filter_map(|loc| match self.cache.get(loc) {
                Some(&(v, true)) => Some((*loc, v)),
                _ => None,
            })
            .collect()
    }

    /// Write every changed value back to the `Cpu` struct.
    fn flush(&mut self) {
        for (loc, v) in self.dirty() {
            self.insts.push(Inst::Set { loc, src: v });
            self.cache.insert(loc, (v, false));
        }
    }

    fn exit_point(&self) -> ExitPoint {
        ExitPoint { insn: self.insn, dirty: self.dirty() }
    }

    /// Xn, where 31 is XZR.
    fn xr(&mut self, r: u8) -> V {
        if r == 31 { self.c(0) } else { self.get(Loc::X(r)) }
    }

    /// Xn, where 31 is SP.
    fn xsp(&mut self, r: u8) -> V {
        if r == 31 { self.get(Loc::Sp) } else { self.get(Loc::X(r)) }
    }

    fn set_xr(&mut self, r: u8, v: V) {
        if r != 31 {
            self.set(Loc::X(r), v);
        }
    }

    fn set_xsp(&mut self, r: u8, v: V) {
        if r == 31 { self.set(Loc::Sp, v) } else { self.set(Loc::X(r), v) }
    }

    /// `v` cut to 32 bits unless `sf`.
    fn width(&mut self, v: V, sf: bool) -> V {
        if sf { v } else { self.bin_c(BinOp::And, v, 0xffff_ffff) }
    }

    fn rd(&mut self, r: u8, sf: bool) -> V {
        let v = self.xr(r);
        self.width(v, sf)
    }

    fn wr(&mut self, r: u8, sf: bool, v: V) {
        if r != 31 {
            let v = self.width(v, sf);
            self.set_xr(r, v);
        }
    }

    fn wr_sp(&mut self, r: u8, sf: bool, v: V) {
        let v = self.width(v, sf);
        self.set_xsp(r, v);
    }

    // ---- arithmetic helpers (as in the interpreter) ----

    /// Sign-extend the low `bits` bits.
    fn sext(&mut self, v: V, bits: u32) -> V {
        if bits >= 64 {
            return v;
        }
        let s = self.bin_c(BinOp::Shl, v, (64 - bits) as u64);
        self.bin_c(BinOp::Ashr, s, (64 - bits) as u64)
    }

    /// Rotate right by a constant, within `width` bits (`v` already fits).
    fn ror(&mut self, v: V, amount: u32, width: u32) -> V {
        let amount = amount % width;
        if amount == 0 {
            return v;
        }
        let lo = self.bin_c(BinOp::Lshr, v, amount as u64);
        let hi = self.bin_c(BinOp::Shl, v, (width - amount) as u64);
        let r = self.bin(BinOp::Or, lo, hi);
        self.width(r, width == 64)
    }

    fn shift_reg(&mut self, v: V, kind: u8, amount: u8, sf: bool) -> V {
        let width = if sf { 64 } else { 32 };
        let v = self.width(v, sf);
        let amount = amount as u64;
        let r = match kind {
            0 => self.bin_c(BinOp::Shl, v, amount),
            1 => self.bin_c(BinOp::Lshr, v, amount),
            2 => {
                if sf {
                    self.bin_c(BinOp::Ashr, v, amount)
                } else {
                    let s = self.sext(v, 32);
                    self.bin_c(BinOp::Ashr, s, amount)
                }
            }
            _ => self.ror(v, amount as u32, width),
        };
        self.width(r, sf)
    }

    fn extend_reg(&mut self, rm: u8, option: u8, shift: u8) -> V {
        let v = self.xr(rm);
        let e = match option {
            0 => self.bin_c(BinOp::And, v, 0xff),
            1 => self.bin_c(BinOp::And, v, 0xffff),
            2 => self.bin_c(BinOp::And, v, 0xffff_ffff),
            4 => self.sext(v, 8),
            5 => self.sext(v, 16),
            6 => self.sext(v, 32),
            _ => v,
        };
        self.bin_c(BinOp::Shl, e, shift as u64)
    }

    /// AddWithCarry: the result and the flags it would set (N, Z, C, V).
    fn add_with_carry(&mut self, a: V, b: V, carry: V, sf: bool) -> (V, [V; 4]) {
        let a = self.width(a, sf);
        let b = self.width(b, sf);
        let (r, c) = if sf {
            let t = self.bin(BinOp::Add, a, b);
            let c1 = self.bin(BinOp::Ult, t, a);
            let r = self.bin(BinOp::Add, t, carry);
            let c2 = self.bin(BinOp::Ult, r, t);
            (r, self.bin(BinOp::Or, c1, c2))
        } else {
            let t = self.bin(BinOp::Add, a, b);
            let s = self.bin(BinOp::Add, t, carry);
            let r = self.bin_c(BinOp::And, s, 0xffff_ffff);
            (r, self.bin_c(BinOp::Lshr, s, 32))
        };
        let ar = self.bin(BinOp::Xor, a, r);
        let br = self.bin(BinOp::Xor, b, r);
        let both = self.bin(BinOp::And, ar, br);
        let top = self.bin_c(BinOp::Lshr, both, if sf { 63 } else { 31 });
        let v = self.bin_c(BinOp::And, top, 1);
        let [n, z] = self.nz(r, sf);
        (r, [n, z, c, v])
    }

    /// N and Z of a result.
    fn nz(&mut self, r: V, sf: bool) -> [V; 2] {
        let top = self.bin_c(BinOp::Lshr, r, if sf { 63 } else { 31 });
        let n = self.bin_c(BinOp::And, top, 1);
        let zero = self.c(0);
        let z = self.bin(BinOp::Eq, r, zero);
        [n, z]
    }

    fn set_flags(&mut self, [n, z, c, v]: [V; 4]) {
        self.set(Loc::N, n);
        self.set(Loc::Z, z);
        self.set(Loc::C, c);
        self.set(Loc::Vf, v);
    }

    fn add(&mut self, a: V, b: V, sub: bool, s: bool, sf: bool) -> V {
        let (b, carry) = if sub { (self.not(b), self.c(1)) } else { (b, self.c(0)) };
        let (r, flags) = self.add_with_carry(a, b, carry, sf);
        if s {
            self.set_flags(flags);
        }
        r
    }

    /// A 4-bit condition code, as 1 or 0.
    fn cond(&mut self, cond: u8) -> V {
        let r = match cond >> 1 {
            0 => self.get(Loc::Z),
            1 => self.get(Loc::C),
            2 => self.get(Loc::N),
            3 => self.get(Loc::Vf),
            4 => {
                let c = self.get(Loc::C);
                let z = self.get(Loc::Z);
                let one = self.c(1);
                let nz = self.bin(BinOp::Xor, z, one);
                self.bin(BinOp::And, c, nz)
            }
            5 | 6 => {
                let n = self.get(Loc::N);
                let v = self.get(Loc::Vf);
                let eq = self.bin(BinOp::Eq, n, v);
                if cond >> 1 == 5 {
                    eq
                } else {
                    let z = self.get(Loc::Z);
                    let one = self.c(1);
                    let nz = self.bin(BinOp::Xor, z, one);
                    self.bin(BinOp::And, nz, eq)
                }
            }
            _ => self.c(1),
        };
        if cond & 1 == 1 && cond != 0xf { self.bin_c(BinOp::Xor, r, 1) } else { r }
    }

    fn load(&mut self, addr: V, bytes: u8) -> V {
        let dst = self.new_v();
        let at = self.exit_point();
        self.insts.push(Inst::Load { dst, addr, bytes, at });
        dst
    }

    fn store(&mut self, addr: V, value: V, bytes: u8) {
        let at = self.exit_point();
        self.insts.push(Inst::Store { addr, value, bytes, at });
    }

    fn helper(&mut self) {
        self.flush();
        self.insts.push(Inst::Helper { insn: self.insn });
        // The helper may have changed any guest state.
        self.cache.clear();
    }

    /// Lift one instruction at `pc`. Returns the next pc when the instruction branches.
    fn op(&mut self, op: &Op, pc: u64) -> Option<V> {
        match *op {
            Op::Generic(_) | Op::LdSt { kind: Kind::VecLoad | Kind::VecStore, .. } => self.helper(),
            Op::Nop => {}
            Op::Mov { rd, rm, sf } => {
                let v = self.rd(rm, sf);
                self.wr(rd, sf, v);
            }
            Op::Movz { rd, imm, sf } => {
                let v = self.c(imm);
                self.wr(rd, sf, v);
            }
            Op::Movk { rd, shift, imm, sf } => {
                let old = self.xr(rd);
                let kept = self.bin_c(BinOp::And, old, !(0xffffu64 << shift));
                let v = self.bin_c(BinOp::Or, kept, imm);
                self.wr(rd, sf, v);
            }
            Op::AddImm { rd, rn, b, carry, sf, s } => {
                let a = self.xsp(rn);
                let b = self.c(b);
                let carry = self.c(carry as u64);
                let (r, flags) = self.add_with_carry(a, b, carry, sf);
                if s {
                    self.set_flags(flags);
                    self.wr(rd, sf, r);
                } else {
                    self.wr_sp(rd, sf, r);
                }
            }
            Op::AddReg { rd, rn, rm, kind, amount, sub, s, sf } => {
                let a = self.rd(rn, sf);
                let m = self.xr(rm);
                let b = self.shift_reg(m, kind, amount, sf);
                let r = self.add(a, b, sub, s, sf);
                self.wr(rd, sf, r);
            }
            Op::AddExt { rd, rn, rm, option, shift, sub, s, sf } => {
                let a = self.xsp(rn);
                let b = self.extend_reg(rm, option, shift);
                let r = self.add(a, b, sub, s, sf);
                if s { self.wr(rd, sf, r) } else { self.wr_sp(rd, sf, r) }
            }
            Op::Logic { rd, rn, rm, kind, amount, invert, opc, sf } => {
                let m = self.xr(rm);
                let mut b = self.shift_reg(m, kind, amount, sf);
                if invert {
                    let nb = self.not(b);
                    b = self.width(nb, sf);
                }
                let a = self.rd(rn, sf);
                let r = match opc {
                    0b00 | 0b11 => self.bin(BinOp::And, a, b),
                    0b01 => self.bin(BinOp::Or, a, b),
                    _ => self.bin(BinOp::Xor, a, b),
                };
                if opc == 0b11 {
                    self.logic_flags(r, sf);
                }
                self.wr(rd, sf, r);
            }
            Op::LogicImm { rd, rn, imm, opc, sf } => {
                let a = self.rd(rn, sf);
                let r = match opc {
                    0b00 | 0b11 => self.bin_c(BinOp::And, a, imm),
                    0b01 => self.bin_c(BinOp::Or, a, imm),
                    _ => self.bin_c(BinOp::Xor, a, imm),
                };
                if opc == 0b11 {
                    self.logic_flags(r, sf);
                    self.wr(rd, sf, r);
                } else {
                    self.wr_sp(rd, sf, r);
                }
            }
            Op::Bitfield { rd, rn, kind, immr, imms, wmask, tmask, sf } => {
                let width = if sf { 64 } else { 32 };
                let src = self.rd(rn, sf);
                let dst = if kind == 1 { self.rd(rd, sf) } else { self.c(0) };
                let kept = self.bin_c(BinOp::And, dst, !wmask);
                let rot = self.ror(src, immr as u32, width);
                let moved = self.bin_c(BinOp::And, rot, wmask);
                let bot = self.bin(BinOp::Or, kept, moved);
                let top = if kind == 0 {
                    let t = self.bin_c(BinOp::Lshr, src, imms as u64);
                    let bit = self.bin_c(BinOp::And, t, 1);
                    let zero = self.c(0);
                    let all = self.bin(BinOp::Sub, zero, bit);
                    self.width(all, sf)
                } else {
                    dst
                };
                let hi = self.bin_c(BinOp::And, top, !tmask);
                let lo = self.bin_c(BinOp::And, bot, tmask);
                let r = self.bin(BinOp::Or, hi, lo);
                self.wr(rd, sf, r);
            }
            Op::Csel { rd, rn, rm, cond, op, sf } => {
                let taken = self.cond(cond);
                let a = self.rd(rn, sf);
                let b = self.rd(rm, sf);
                let b = match op {
                    0 => b,
                    1 => self.bin_c(BinOp::Add, b, 1),
                    2 => self.not(b),
                    _ => {
                        let zero = self.c(0);
                        self.bin(BinOp::Sub, zero, b)
                    }
                };
                let r = self.select(taken, a, b);
                self.wr(rd, sf, r);
            }
            Op::Ccmp { rn, rm, imm, cond, nzcv, sub, sf } => {
                let taken = self.cond(cond);
                let a = self.rd(rn, sf);
                let b = if imm { self.c(rm as u64) } else { self.rd(rm, sf) };
                let (b, carry) = if sub {
                    let nb = self.not(b);
                    (self.width(nb, sf), self.c(1))
                } else {
                    (b, self.c(0))
                };
                let (_, flags) = self.add_with_carry(a, b, carry, sf);
                let consts = [(nzcv >> 3) & 1, (nzcv >> 2) & 1, (nzcv >> 1) & 1, nzcv & 1];
                let mut out = flags;
                for (i, f) in out.iter_mut().enumerate() {
                    let k = self.c(consts[i] as u64);
                    *f = self.select(taken, *f, k);
                }
                self.set_flags(out);
            }
            Op::Ldr { rt, rn, off, bytes } => {
                let base = self.xsp(rn);
                let addr = self.bin_c(BinOp::Add, base, off);
                let v = self.load(addr, bytes);
                self.set_xr(rt, v);
            }
            Op::Str { rt, rn, off, bytes } => {
                let base = self.xsp(rn);
                let addr = self.bin_c(BinOp::Add, base, off);
                let v = self.xr(rt);
                self.store(addr, v, bytes);
            }
            Op::LdSt { rt, rn, addr, bytes, kind } => {
                let base = self.xsp(rn);
                let (at, writeback) = match addr {
                    Addr::Offset(off) => (self.bin_c(BinOp::Add, base, off), None),
                    Addr::Reg { rm, option, shift } => {
                        let m = self.xr(rm);
                        let m = match option {
                            2 => self.bin_c(BinOp::And, m, 0xffff_ffff),
                            6 => self.sext(m, 32),
                            _ => m,
                        };
                        let off = self.bin_c(BinOp::Shl, m, shift as u64);
                        (self.bin(BinOp::Add, base, off), None)
                    }
                    Addr::Pre(imm) => {
                        let a = self.bin_c(BinOp::Add, base, imm);
                        (a, Some(a))
                    }
                    Addr::Post(imm) => (base, Some(self.bin_c(BinOp::Add, base, imm))),
                };
                match kind {
                    Kind::Store => {
                        let v = self.xr(rt);
                        self.store(at, v, bytes);
                    }
                    Kind::Load => {
                        let v = self.load(at, bytes);
                        self.set_xr(rt, v);
                    }
                    Kind::LoadSigned(to64) => {
                        let v = self.load(at, bytes);
                        let v = self.sext(v, bytes as u32 * 8);
                        let v = if to64 { v } else { self.bin_c(BinOp::And, v, 0xffff_ffff) };
                        self.set_xr(rt, v);
                    }
                    Kind::VecLoad | Kind::VecStore => unreachable!("lifted as a helper"),
                }
                if let Some(wb) = writeback {
                    self.set_xsp(rn, wb);
                }
            }
            Op::Pair { rt, rt2, rn, off, bytes, load, mode } => {
                let base = self.xsp(rn);
                let addr = if mode == 0b01 { base } else { self.bin_c(BinOp::Add, base, off as u64) };
                let second = self.bin_c(BinOp::Add, addr, bytes as u64);
                if load {
                    let a = self.load(addr, bytes);
                    let b = self.load(second, bytes);
                    self.set_xr(rt, a);
                    self.set_xr(rt2, b);
                } else {
                    let a = self.xr(rt);
                    let b = self.xr(rt2);
                    self.store(addr, a, bytes);
                    self.store(second, b, bytes);
                }
                if mode & 1 == 1 {
                    let wb = self.bin_c(BinOp::Add, base, off as u64);
                    self.set_xsp(rn, wb);
                }
            }
            Op::B { target, link } => {
                if link {
                    let ret = self.c(pc + 4);
                    self.set(Loc::X(30), ret);
                }
                return Some(self.c(target));
            }
            Op::BCond { cond, target } => {
                let taken = self.cond(cond);
                let t = self.c(target);
                let f = self.c(pc.wrapping_add(4));
                return Some(self.select(taken, t, f));
            }
            Op::Cbz { rt, sf, nonzero, target } => {
                let v = self.rd(rt, sf);
                let zero = self.c(0);
                let taken = self.bin(if nonzero { BinOp::Ne } else { BinOp::Eq }, v, zero);
                let t = self.c(target);
                let f = self.c(pc.wrapping_add(4));
                return Some(self.select(taken, t, f));
            }
            Op::Tbz { rt, bit, nonzero, target } => {
                let v = self.xr(rt);
                let s = self.bin_c(BinOp::Lshr, v, bit as u64);
                let b = self.bin_c(BinOp::And, s, 1);
                let taken = if nonzero { b } else { self.bin_c(BinOp::Xor, b, 1) };
                let t = self.c(target);
                let f = self.c(pc.wrapping_add(4));
                return Some(self.select(taken, t, f));
            }
            Op::Ret { rn } => return Some(self.xr(rn)),
            Op::Br { rn, link } => {
                let target = self.xr(rn);
                if link {
                    let ret = self.c(pc + 4);
                    self.set(Loc::X(30), ret);
                }
                return Some(target);
            }
        }
        None
    }

    /// ANDS / BICS: N and Z from the result, C and V cleared.
    fn logic_flags(&mut self, r: V, sf: bool) {
        let [n, z] = self.nz(r, sf);
        let zero = self.c(0);
        self.set_flags([n, z, zero, zero]);
    }
}

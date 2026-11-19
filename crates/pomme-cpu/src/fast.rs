//! Instructions decoded once, when their block is fetched: the forms that
//! make up most of what apps run (register moves, loads and stores, pairs,
//! arithmetic, logic and compares, bitfield moves, conditional selects,
//! branches), with their fields pulled out (and constants such as logical
//! immediates and ADRP's page worked out) ahead of time. Everything else
//! stays [`Op::Generic`] and runs through the full decoder.
//!
//! Each fast form does exactly what the full decoder does for that encoding,
//! using the same helpers; `tests/fast.rs` checks them against it on random
//! encodings and register states.

use crate::ldst::{load, mask, store};
use crate::{Cpu, Step, bit, bits, decode_bit_masks, ones, ror, sext};
use pomme_mem::Memory;

#[derive(Clone, Copy, Debug)]
pub enum Op {
    /// Any instruction, decoded when it runs.
    Generic(u32),
    /// `mov xd, xm` / `mov wd, wm` (ORR with the zero register, no shift).
    Mov { rd: u8, rm: u8, sf: bool },
    /// A constant into a register: `movz`, `movn` (the immediate already
    /// inverted), and `adr` / `adrp` (the address worked out from pc, `sf` set).
    Movz { rd: u8, imm: u64, sf: bool },
    /// `movk`: a 16-bit immediate into one halfword, keeping the rest.
    Movk { rd: u8, shift: u8, imm: u64, sf: bool },
    /// ADD/SUB (shifted register), setting flags when `s` (CMP, CMN, NEG).
    AddReg { rd: u8, rn: u8, rm: u8, kind: u8, amount: u8, sub: bool, s: bool, sf: bool },
    /// ADD/SUB (extended register): `option` picks the extension of Rm.
    AddExt { rd: u8, rn: u8, rm: u8, option: u8, shift: u8, sub: bool, s: bool, sf: bool },
    /// AND/ORR/EOR/ANDS (shifted register), with Rm inverted when `invert` (BIC, ORN, EON, BICS).
    Logic { rd: u8, rn: u8, rm: u8, kind: u8, amount: u8, invert: bool, opc: u8, sf: bool },
    /// AND/ORR/EOR/ANDS (immediate): the bit mask already decoded.
    LogicImm { rd: u8, rn: u8, imm: u64, opc: u8, sf: bool },
    /// SBFM (`kind` 0), BFM (1), UBFM (2): LSL, LSR, ASR, UBFX, SXTW, BFI and
    /// the rest, with the masks already decoded.
    Bitfield { rd: u8, rn: u8, kind: u8, immr: u8, imms: u8, wmask: u64, tmask: u64, sf: bool },
    /// CSEL / CSINC / CSINV / CSNEG (`op`: bit 30 and bit 10).
    Csel { rd: u8, rn: u8, rm: u8, cond: u8, op: u8, sf: bool },
    /// CCMP / CCMN with a register or a 5-bit immediate (`imm`).
    Ccmp { rn: u8, rm: u8, imm: bool, cond: u8, nzcv: u8, sub: bool, sf: bool },
    /// Any other single-register load or store, integer or SIMD&FP.
    LdSt { rt: u8, rn: u8, addr: Addr, bytes: u8, kind: Kind },
    /// ADD/SUB (immediate), setting flags when `s` (CMP, CMN): `b` is the
    /// immediate, already inverted for a subtraction (with carry in).
    AddImm { rd: u8, rn: u8, b: u64, carry: bool, sf: bool, s: bool },
    /// LDR (immediate, unsigned offset), zero-extending: x, w, h, b.
    Ldr { rt: u8, rn: u8, off: u64, bytes: u8 },
    /// STR (immediate, unsigned offset): x, w, h, b.
    Str { rt: u8, rn: u8, off: u64, bytes: u8 },
    /// LDP / STP of x or w registers: `mode` 0b01 post-index, 0b11 pre-index, else offset.
    Pair { rt: u8, rt2: u8, rn: u8, off: i64, bytes: u8, load: bool, mode: u8 },
    /// B / BL to a target (BL links the return address).
    B { target: u64, link: bool },
    /// B.cond.
    BCond { cond: u8, target: u64 },
    /// CBZ / CBNZ.
    Cbz { rt: u8, sf: bool, nonzero: bool, target: u64 },
    /// RET Xn.
    Ret { rn: u8 },
    /// TBZ / TBNZ.
    Tbz { rt: u8, bit: u8, nonzero: bool, target: u64 },
    /// BR / BLR.
    Br { rn: u8, link: bool },
    /// NOP and the other hints (BTI, PAC hints, YIELD …): nothing to do.
    Nop,
}

/// How [`Op::LdSt`] forms its address.
#[derive(Clone, Copy, Debug)]
pub enum Addr {
    /// Base plus an immediate (scaled or unscaled), no writeback.
    Offset(u64),
    /// Base plus Rm, extended by `option` and shifted.
    Reg { rm: u8, option: u8, shift: u8 },
    /// Base plus an immediate, written back first (pre-index).
    Pre(u64),
    /// Base, then base plus an immediate written back (post-index).
    Post(u64),
}

/// What [`Op::LdSt`] does with the register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Store,
    Load,
    /// Load, sign-extending to 64 bits (`true`) or 32.
    LoadSigned(bool),
    /// SIMD&FP register (Q, D, S, H, B).
    VecLoad,
    VecStore,
}

/// Decode `insn` at `pc` into a fast form if it has one.
pub fn decode(insn: u32, pc: u64) -> Op {
    let (rd, rn) = (bits(insn, 4, 0) as u8, bits(insn, 9, 5) as u8);
    let sf = bit(insn, 31);
    // ORR (shifted register) with rn = zr, no shift: MOV.
    if insn & 0x7fe0_ffe0 == 0x2a00_03e0 {
        return Op::Mov { rd, rm: bits(insn, 20, 16) as u8, sf };
    }
    // MOVZ.
    if insn & 0x7f80_0000 == 0x5280_0000 && (sf || !bit(insn, 22)) {
        return Op::Movz { rd, imm: (bits(insn, 20, 5) as u64) << (bits(insn, 22, 21) * 16), sf };
    }
    // ADD/SUB (immediate).
    if insn & 0x1f80_0000 == 0x1100_0000 {
        let mut imm = bits(insn, 21, 10) as u64;
        if bit(insn, 22) {
            imm <<= 12;
        }
        let sub = bit(insn, 30);
        return Op::AddImm { rd, rn, b: if sub { !imm } else { imm }, carry: sub, sf, s: bit(insn, 29) };
    }
    // LDR/STR (immediate, unsigned offset), integer registers, plain load or store.
    if insn & 0x3f00_0000 == 0x3900_0000 {
        let size = bits(insn, 31, 30);
        let bytes = 1u8 << size;
        let off = bits(insn, 21, 10) as u64 * bytes as u64;
        let rt = rd;
        match bits(insn, 23, 22) {
            0 => return Op::Str { rt, rn, off, bytes },
            1 => return Op::Ldr { rt, rn, off, bytes },
            _ => {}
        }
    }
    // LDP/STP of integer registers (not LDPSW, not SIMD): opc 00 (w) or 10 (x).
    if insn & 0x3a00_0000 == 0x2800_0000 && !bit(insn, 26) && matches!(bits(insn, 31, 30), 0 | 2) {
        let bytes = if bits(insn, 31, 30) == 2 { 8u8 } else { 4 };
        return Op::Pair {
            rt: rd,
            rt2: bits(insn, 14, 10) as u8,
            rn,
            off: sext(bits(insn, 21, 15) as u64, 7) * bytes as i64,
            bytes,
            load: bit(insn, 22),
            mode: bits(insn, 24, 23) as u8,
        };
    }
    // MOVN / MOVK (MOVZ is above).
    if insn & 0x1f80_0000 == 0x1280_0000 && (sf || !bit(insn, 22)) {
        let shift = bits(insn, 22, 21) * 16;
        let imm = (bits(insn, 20, 5) as u64) << shift;
        match bits(insn, 30, 29) {
            0b00 => return Op::Movz { rd, imm: !imm, sf },
            0b11 => return Op::Movk { rd, shift: shift as u8, imm, sf },
            _ => {}
        }
    }
    // ADR / ADRP: pc is known, so the address is a constant.
    if insn & 0x1f00_0000 == 0x1000_0000 {
        let imm = sext((bits(insn, 23, 5) << 2 | bits(insn, 30, 29)) as u64, 21);
        let v = if sf { (pc & !0xfff).wrapping_add((imm << 12) as u64) } else { pc.wrapping_add(imm as u64) };
        return Op::Movz { rd, imm: v, sf: true };
    }
    // Logical (immediate).
    if insn & 0x1f80_0000 == 0x1200_0000 && (sf || !bit(insn, 22)) {
        let width = if sf { 64 } else { 32 };
        if let Some((imm, _)) = decode_bit_masks(bits(insn, 22, 22), bits(insn, 15, 10), bits(insn, 21, 16), true, width) {
            return Op::LogicImm { rd, rn, imm, opc: bits(insn, 30, 29) as u8, sf };
        }
    }
    // Bitfield: SBFM, BFM, UBFM.
    if insn & 0x1f80_0000 == 0x1300_0000 && bit(insn, 22) == sf && bits(insn, 30, 29) != 0b11 {
        let width = if sf { 64 } else { 32 };
        let (immr, imms) = (bits(insn, 21, 16), bits(insn, 15, 10));
        if let Some((wmask, tmask)) = decode_bit_masks(bits(insn, 22, 22), imms, immr, false, width) {
            return Op::Bitfield { rd, rn, kind: bits(insn, 30, 29) as u8, immr: immr as u8, imms: imms as u8, wmask, tmask, sf };
        }
    }
    let rm = bits(insn, 20, 16) as u8;
    // Logical (shifted register).
    if insn & 0x1f00_0000 == 0x0a00_0000 && (sf || !bit(insn, 15)) {
        return Op::Logic {
            rd,
            rn,
            rm,
            kind: bits(insn, 23, 22) as u8,
            amount: bits(insn, 15, 10) as u8,
            invert: bit(insn, 21),
            opc: bits(insn, 30, 29) as u8,
            sf,
        };
    }
    // ADD/SUB (shifted register).
    if insn & 0x1f20_0000 == 0x0b00_0000 && bits(insn, 23, 22) != 3 && (sf || !bit(insn, 15)) {
        return Op::AddReg {
            rd,
            rn,
            rm,
            kind: bits(insn, 23, 22) as u8,
            amount: bits(insn, 15, 10) as u8,
            sub: bit(insn, 30),
            s: bit(insn, 29),
            sf,
        };
    }
    // ADD/SUB (extended register).
    if insn & 0x1f20_0000 == 0x0b20_0000 && bits(insn, 23, 22) == 0 && bits(insn, 12, 10) <= 4 {
        return Op::AddExt {
            rd,
            rn,
            rm,
            option: bits(insn, 15, 13) as u8,
            shift: bits(insn, 12, 10) as u8,
            sub: bit(insn, 30),
            s: bit(insn, 29),
            sf,
        };
    }
    // CCMN / CCMP (register or immediate).
    if insn & 0x1fe0_0000 == 0x1a40_0000 && bit(insn, 29) && !bit(insn, 10) && !bit(insn, 4) {
        return Op::Ccmp { rn, rm, imm: bit(insn, 11), cond: bits(insn, 15, 12) as u8, nzcv: bits(insn, 3, 0) as u8, sub: bit(insn, 30), sf };
    }
    // CSEL / CSINC / CSINV / CSNEG.
    if insn & 0x1fe0_0000 == 0x1a80_0000 && !bit(insn, 29) && !bit(insn, 11) {
        return Op::Csel { rd, rn, rm, cond: bits(insn, 15, 12) as u8, op: (bits(insn, 30, 30) << 1 | bits(insn, 10, 10)) as u8, sf };
    }
    // Load/store register, all single-register forms but the atomics.
    if insn & 0x3a00_0000 == 0x3800_0000 && !(!bit(insn, 24) && bit(insn, 21) && bits(insn, 11, 10) == 0) {
        if let Some(op) = decode_ldst(insn) {
            return op;
        }
    }
    // B / BL.
    if insn & 0x7c00_0000 == 0x1400_0000 {
        return Op::B { target: pc.wrapping_add((sext(bits(insn, 25, 0) as u64, 26) * 4) as u64), link: bit(insn, 31) };
    }
    // B.cond.
    if insn & 0xff00_0010 == 0x5400_0000 {
        return Op::BCond { cond: bits(insn, 3, 0) as u8, target: pc.wrapping_add((sext(bits(insn, 23, 5) as u64, 19) * 4) as u64) };
    }
    // CBZ / CBNZ.
    if insn & 0x7e00_0000 == 0x3400_0000 {
        return Op::Cbz { rt: rd, sf, nonzero: bit(insn, 24), target: pc.wrapping_add((sext(bits(insn, 23, 5) as u64, 19) * 4) as u64) };
    }
    // TBZ / TBNZ.
    if insn & 0x7e00_0000 == 0x3600_0000 {
        let target = pc.wrapping_add((sext(bits(insn, 18, 5) as u64, 14) * 4) as u64);
        return Op::Tbz { rt: rd, bit: ((bits(insn, 31, 31) << 5) | bits(insn, 23, 19)) as u8, nonzero: bit(insn, 24), target };
    }
    // RET, BR, BLR.
    match insn & 0xffff_fc1f {
        0xd65f_0000 => return Op::Ret { rn },
        0xd61f_0000 => return Op::Br { rn, link: false },
        0xd63f_0000 => return Op::Br { rn, link: true },
        _ => {}
    }
    // Hints.
    if insn & 0xffff_f01f == 0xd503_201f {
        return Op::Nop;
    }
    Op::Generic(insn)
}

/// A single-register load or store (not an atomic), as `Cpu::ldst` reads it;
/// None for the forms left to it (prefetches, reserved encodings).
fn decode_ldst(insn: u32) -> Option<Op> {
    let (rt, rn) = (bits(insn, 4, 0) as u8, bits(insn, 9, 5) as u8);
    let size = bits(insn, 31, 30);
    let opc = bits(insn, 23, 22);
    let (bytes, kind) = if bit(insn, 26) {
        let bytes = if opc & 2 != 0 {
            if size != 0 {
                return None;
            }
            16
        } else {
            1 << size
        };
        (bytes, if opc & 1 == 1 { Kind::VecLoad } else { Kind::VecStore })
    } else {
        let kind = match (size, opc) {
            (_, 0) => Kind::Store,
            (_, 1) => Kind::Load,
            (3, 2) | (2, 3) | (3, 3) => return None, // PRFM, reserved
            (_, 2) => Kind::LoadSigned(true),
            (_, _) => Kind::LoadSigned(false),
        };
        (1u32 << size, kind)
    };
    let addr = if bit(insn, 24) {
        Addr::Offset(bits(insn, 21, 10) as u64 * bytes as u64)
    } else if bit(insn, 21) {
        let option = bits(insn, 15, 13);
        if bits(insn, 11, 10) != 0b10 || option & 2 == 0 {
            return None;
        }
        let shift = if bit(insn, 12) { bytes.trailing_zeros() } else { 0 };
        Addr::Reg { rm: bits(insn, 20, 16) as u8, option: option as u8, shift: shift as u8 }
    } else {
        let imm = sext(bits(insn, 20, 12) as u64, 9) as u64;
        match bits(insn, 11, 10) {
            0b00 | 0b10 => Addr::Offset(imm),
            0b01 => Addr::Post(imm),
            _ => Addr::Pre(imm),
        }
    };
    Some(Op::LdSt { rt, rn, addr, bytes: bytes as u8, kind })
}

impl Cpu {
    /// Run one decoded instruction (at pc): like executing its encoding.
    /// A JIT calls this for the instructions it does not translate.
    #[inline(always)]
    pub fn exec_op(&mut self, op: &Op, mem: &mut Memory) -> Step {
        let next = match *op {
            Op::Generic(insn) => return self.exec(insn, mem),
            Op::Mov { rd, rm, sf } => {
                let v = self.rd(rm as u32, sf);
                self.wr(rd as u32, sf, v);
                self.pc.wrapping_add(4)
            }
            Op::Movz { rd, imm, sf } => {
                self.wr(rd as u32, sf, imm);
                self.pc.wrapping_add(4)
            }
            Op::Movk { rd, shift, imm, sf } => {
                let old = self.xr(rd as u32);
                self.wr(rd as u32, sf, (old & !(0xffffu64 << shift)) | imm);
                self.pc.wrapping_add(4)
            }
            Op::AddReg { rd, rn, rm, kind, amount, sub, s, sf } => {
                let a = self.rd(rn as u32, sf);
                let b = self.shift_reg(self.xr(rm as u32), kind as u32, amount as u32, sf);
                let r = if sub { self.add_with_carry(a, !b, true, sf, s) } else { self.add_with_carry(a, b, false, sf, s) };
                self.wr(rd as u32, sf, r);
                self.pc.wrapping_add(4)
            }
            Op::AddExt { rd, rn, rm, option, shift, sub, s, sf } => {
                let width = if sf { 64 } else { 32 };
                let a = self.xsp(rn as u32);
                let a = if sf { a } else { a as u32 as u64 };
                let b = self.extend_reg(rm as u32, option as u32, shift as u32) & ones(width);
                let r = if sub { self.add_with_carry(a, !b, true, sf, s) } else { self.add_with_carry(a, b, false, sf, s) };
                if s { self.wr(rd as u32, sf, r) } else { self.wr_sp(rd as u32, sf, r) }
                self.pc.wrapping_add(4)
            }
            Op::Logic { rd, rn, rm, kind, amount, invert, opc, sf } => {
                let mut b = self.shift_reg(self.xr(rm as u32), kind as u32, amount as u32, sf);
                if invert {
                    b = !b & ones(if sf { 64 } else { 32 });
                }
                let a = self.rd(rn as u32, sf);
                let r = match opc {
                    0b00 | 0b11 => a & b,
                    0b01 => a | b,
                    _ => a ^ b,
                };
                if opc == 0b11 {
                    self.set_nz(r, sf);
                    self.c = false;
                    self.v = false;
                }
                self.wr(rd as u32, sf, r);
                self.pc.wrapping_add(4)
            }
            Op::LogicImm { rd, rn, imm, opc, sf } => {
                let a = self.rd(rn as u32, sf);
                match opc {
                    0b00 => self.wr_sp(rd as u32, sf, a & imm),
                    0b01 => self.wr_sp(rd as u32, sf, a | imm),
                    0b10 => self.wr_sp(rd as u32, sf, a ^ imm),
                    _ => {
                        let r = a & imm;
                        self.set_nz(r, sf);
                        self.c = false;
                        self.v = false;
                        self.wr(rd as u32, sf, r);
                    }
                }
                self.pc.wrapping_add(4)
            }
            Op::Bitfield { rd, rn, kind, immr, imms, wmask, tmask, sf } => {
                let width = if sf { 64 } else { 32 };
                let src = self.rd(rn as u32, sf);
                let dst = if kind == 1 { self.rd(rd as u32, sf) } else { 0 };
                let bot = (dst & !wmask) | (ror(src, immr as u32, width) & wmask);
                let top = if kind == 0 {
                    if (src >> imms) & 1 == 1 { ones(width) } else { 0 }
                } else {
                    dst
                };
                self.wr(rd as u32, sf, (top & !tmask) | (bot & tmask));
                self.pc.wrapping_add(4)
            }
            Op::Csel { rd, rn, rm, cond, op, sf } => {
                let r = if self.cond(cond as u32) {
                    self.rd(rn as u32, sf)
                } else {
                    let b = self.rd(rm as u32, sf);
                    match op {
                        0 => b,
                        1 => b.wrapping_add(1),
                        2 => !b,
                        _ => b.wrapping_neg(),
                    }
                };
                self.wr(rd as u32, sf, r);
                self.pc.wrapping_add(4)
            }
            Op::Ccmp { rn, rm, imm, cond, nzcv, sub, sf } => {
                if self.cond(cond as u32) {
                    let a = self.rd(rn as u32, sf);
                    let b = if imm { rm as u64 } else { self.rd(rm as u32, sf) };
                    if sub {
                        self.add_with_carry(a, !b & ones(if sf { 64 } else { 32 }), true, sf, true);
                    } else {
                        self.add_with_carry(a, b, false, sf, true);
                    }
                } else {
                    self.set_nzcv4(nzcv as u32);
                }
                self.pc.wrapping_add(4)
            }
            Op::LdSt { rt, rn, addr, bytes, kind } => {
                let base = self.xsp(rn as u32);
                let (at, writeback) = match addr {
                    Addr::Offset(off) => (base.wrapping_add(off), None),
                    Addr::Reg { rm, option, shift } => {
                        let m = self.xr(rm as u32);
                        let off = match option {
                            2 => m as u32 as u64,
                            6 => m as u32 as i32 as i64 as u64,
                            _ => m,
                        } << shift;
                        (base.wrapping_add(off), None)
                    }
                    Addr::Pre(imm) => (base.wrapping_add(imm), Some(base.wrapping_add(imm))),
                    Addr::Post(imm) => (base, Some(base.wrapping_add(imm))),
                };
                let (rt, bytes) = (rt as u32, bytes as u32);
                match kind {
                    Kind::Store => store(mem, at, bytes, (self.xr(rt) & mask(bytes)) as u128)?,
                    Kind::Load => {
                        let v = load(mem, at, bytes)? as u64;
                        self.set_xr(rt, v);
                    }
                    Kind::LoadSigned(to64) => {
                        let v = sext(load(mem, at, bytes)? as u64, bytes * 8) as u64;
                        self.set_xr(rt, if to64 { v } else { v as u32 as u64 });
                    }
                    Kind::VecLoad => self.q[rt as usize] = load(mem, at, bytes)?,
                    Kind::VecStore => {
                        let v = self.q[rt as usize];
                        let v = if bytes == 16 { v } else { v & ((1u128 << (bytes * 8)) - 1) };
                        store(mem, at, bytes, v)?;
                    }
                }
                if let Some(wb) = writeback {
                    self.set_xsp(rn as u32, wb);
                }
                self.pc.wrapping_add(4)
            }
            Op::AddImm { rd, rn, b, carry, sf, s } => {
                let a = self.xsp(rn as u32);
                let a = if sf { a } else { a as u32 as u64 };
                let r = self.add_with_carry(a, b, carry, sf, s);
                if s { self.wr(rd as u32, sf, r) } else { self.wr_sp(rd as u32, sf, r) }
                self.pc.wrapping_add(4)
            }
            Op::Ldr { rt, rn, off, bytes } => {
                let addr = self.xsp(rn as u32).wrapping_add(off);
                let v = match bytes {
                    1 => mem.read_u8(addr)? as u64,
                    2 => mem.read_u16(addr)? as u64,
                    4 => mem.read_u32(addr)? as u64,
                    _ => mem.read_u64(addr)?,
                };
                self.set_xr(rt as u32, v);
                self.pc.wrapping_add(4)
            }
            Op::Str { rt, rn, off, bytes } => {
                let addr = self.xsp(rn as u32).wrapping_add(off);
                let v = self.xr(rt as u32);
                match bytes {
                    1 => mem.write_u8(addr, v as u8)?,
                    2 => mem.write_u16(addr, v as u16)?,
                    4 => mem.write_u32(addr, v as u32)?,
                    _ => mem.write_u64(addr, v)?,
                }
                self.pc.wrapping_add(4)
            }
            Op::Pair { rt, rt2, rn, off, bytes, load, mode } => {
                let base = self.xsp(rn as u32);
                let addr = if mode == 0b01 { base } else { base.wrapping_add(off as u64) };
                let second = addr.wrapping_add(bytes as u64);
                if load {
                    let (a, b) = if bytes == 8 {
                        (mem.read_u64(addr)?, mem.read_u64(second)?)
                    } else {
                        (mem.read_u32(addr)? as u64, mem.read_u32(second)? as u64)
                    };
                    self.set_xr(rt as u32, a);
                    self.set_xr(rt2 as u32, b);
                } else {
                    let (a, b) = (self.xr(rt as u32), self.xr(rt2 as u32));
                    if bytes == 8 {
                        mem.write_u64(addr, a)?;
                        mem.write_u64(second, b)?;
                    } else {
                        mem.write_u32(addr, a as u32)?;
                        mem.write_u32(second, b as u32)?;
                    }
                }
                if mode & 1 == 1 {
                    self.set_xsp(rn as u32, base.wrapping_add(off as u64));
                }
                self.pc.wrapping_add(4)
            }
            Op::B { target, link } => {
                if link {
                    self.x[30] = self.pc + 4;
                }
                target
            }
            Op::BCond { cond, target } => {
                if self.cond(cond as u32) { target } else { self.pc.wrapping_add(4) }
            }
            Op::Cbz { rt, sf, nonzero, target } => {
                if (self.rd(rt as u32, sf) == 0) != nonzero { target } else { self.pc.wrapping_add(4) }
            }
            Op::Ret { rn } => self.xr(rn as u32),
            Op::Tbz { rt, bit, nonzero, target } => {
                if ((self.xr(rt as u32) >> bit) & 1 == 1) == nonzero { target } else { self.pc.wrapping_add(4) }
            }
            Op::Br { rn, link } => {
                let target = self.xr(rn as u32);
                if link {
                    self.x[30] = self.pc + 4;
                }
                target
            }
            Op::Nop => self.pc.wrapping_add(4),
        };
        self.pc = next;
        self.steps += 1;
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use pomme_mem::Prot;

    const BASE: u64 = 0x1000_0000;
    const SIZE: u64 = 0x10000;
    const PC: u64 = 0x2000_0000;

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
    }

    fn memory(seed: u64) -> Memory {
        let mut mem = Memory::default();
        mem.map(BASE, SIZE, Prot::RW, "data").unwrap();
        // One write of the whole region: 8 bytes at a time is slow in unoptimized builds.
        let mut r = Rng(seed);
        let bytes: Vec<u8> = (0..SIZE / 8).flat_map(|_| r.next().to_le_bytes()).collect();
        mem.write(BASE, &bytes).unwrap();
        mem
    }

    fn cpu(r: &mut Rng) -> Cpu {
        let mut c = Cpu::new();
        for x in c.x.iter_mut() {
            // Mostly addresses in the data region (so loads and stores land), sometimes anything.
            *x = if r.next() % 4 == 0 { r.next() } else { BASE + (r.next() % (SIZE / 2)) & !7 };
        }
        c.sp = BASE + (r.next() % (SIZE / 2)) & !15;
        c.set_nzcv(r.next() & 0xf000_0000);
        for q in c.q.iter_mut() {
            *q = (r.next() as u128) << 64 | r.next() as u128;
        }
        c.pc = PC;
        c
    }

    fn state(c: &Cpu) -> (Vec<u64>, u64, u64, u64, u64, Vec<u128>) {
        (c.x.to_vec(), c.sp, c.pc, c.nzcv(), c.steps, c.q.to_vec())
    }

    /// Every encoding with a fast form does what the full decoder does, from random states.
    #[test]
    fn fast_forms_match_the_decoder() {
        let mut r = Rng(0x9e37_79b9_7f4a_7c15);
        let mut checked = std::collections::HashMap::<&str, u32>::new();
        // Encodings random bits rarely hit: (fixed bits, bits left random).
        const TEMPLATES: &[(u32, u32)] = &[
            (0x2a00_03e0, 0x801f_001f), // mov
            (0xd65f_0000, 0x0000_03e0), // ret
            (0x1100_0000, 0xe07f_ffff), // add/sub imm
            (0x3900_0000, 0xc0ff_ffff), // ldr/str imm
            (0x2800_0000, 0x81ff_ffff), // ldp/stp
            (0x1280_0000, 0xe07f_ffff), // move wide
            (0x1000_0000, 0xff7f_ffff), // adr/adrp
            (0x1200_0000, 0xe07f_ffff), // logical imm
            (0x1300_0000, 0xe07f_ffff), // bitfield
            (0x0a00_0000, 0xe0ff_ffff), // logical reg
            (0x0b00_0000, 0xe0df_ffff), // add/sub reg
            (0x0b20_0000, 0xe0df_ffff), // add/sub ext
            (0x1a40_0800, 0xc01f_f00f), // ccmp/ccmn
            (0x1a40_0000, 0xc01f_f00f), // ccmp/ccmn (register)
            (0x1a80_0000, 0xc01f_f7ff), // csel family
            (0x3800_0000, 0xc4ff_ffff), // ldr/str single, imm9 and register
            (0x3900_0000, 0xc4ff_ffff), // ldr/str single, unsigned offset, all kinds
            (0x3820_0800, 0xc4df_f3ff), // ldr/str register offset
            (0x3600_0000, 0x81ff_ffff), // tbz/tbnz
            (0xd61f_0000, 0x0020_03e0), // br/blr
            (0xd503_201f, 0x0000_0fe0), // hints
        ];
        for i in 0..1_000_000u32 {
            let insn = match i % 3 {
                0 => r.next() as u32,
                _ => {
                    let (fixed, free) = TEMPLATES[r.next() as usize % TEMPLATES.len()];
                    fixed | (r.next() as u32 & free)
                }
            };
            let op = decode(insn, PC);
            let kind = match op {
                Op::Generic(_) => continue,
                Op::Mov { .. } => "mov",
                Op::Movz { .. } => "movz",
                Op::AddImm { .. } => "add/sub imm",
                Op::Ldr { .. } => "ldr",
                Op::Str { .. } => "str",
                Op::Pair { .. } => "ldp/stp",
                Op::B { .. } => "b/bl",
                Op::BCond { .. } => "b.cond",
                Op::Cbz { .. } => "cbz",
                Op::Ret { .. } => "ret",
                Op::Movk { .. } => "movk",
                Op::AddReg { .. } => "add/sub reg",
                Op::AddExt { .. } => "add/sub ext",
                Op::Logic { .. } => "logical reg",
                Op::LogicImm { .. } => "logical imm",
                Op::Bitfield { .. } => "bitfield",
                Op::Csel { .. } => "csel",
                Op::Ccmp { .. } => "ccmp",
                Op::LdSt { addr: Addr::Reg { .. }, .. } => "ldst reg",
                Op::LdSt { addr: Addr::Pre(_) | Addr::Post(_), .. } => "ldst pre/post",
                Op::LdSt { kind: Kind::VecLoad | Kind::VecStore, .. } => "ldst vector",
                Op::LdSt { .. } => "ldst",
                Op::Tbz { .. } => "tbz",
                Op::Br { .. } => "br/blr",
                Op::Nop => "nop",
            };
            *checked.entry(kind).or_default() += 1;
            let seed = r.next();
            let (mut a, mut b) = (cpu(&mut Rng(seed | 1)), cpu(&mut Rng(seed | 1)));
            let mut ma = memory(seed);
            let mut mb = ma.clone();
            let ra = a.exec(insn, &mut ma);
            let rb = b.exec_op(&op, &mut mb);
            assert_eq!(format!("{ra:?}"), format!("{rb:?}"), "{insn:#010x} {op:?}");
            assert_eq!(state(&a), state(&b), "{insn:#010x} {op:?}");
            assert_eq!(ma.read(BASE, SIZE as usize).unwrap(), mb.read(BASE, SIZE as usize).unwrap(), "{insn:#010x} {op:?}");
        }
        // Every form was exercised.
        for kind in [
            "mov", "movz", "add/sub imm", "ldr", "str", "ldp/stp", "b/bl", "b.cond", "cbz", "ret", "movk", "add/sub reg", "add/sub ext",
            "logical reg", "logical imm", "bitfield", "csel", "ccmp", "ldst reg", "ldst pre/post", "ldst vector", "ldst", "tbz", "br/blr", "nop",
        ] {
            assert!(checked.get(kind).copied().unwrap_or(0) > 0, "{kind} never generated: {checked:?}");
        }
    }
}
